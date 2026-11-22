//! The page a link to a flight shows, and its preview picture.
//!
//! These two are not part of the versioned API. They answer a browser or a
//! chat app's preview fetcher, neither of which sends a session token, so
//! they show a flight only if its pilot made it public or unlisted. A
//! private flight and a flight that does not exist get the same answer.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use tokio::sync::Semaphore;
use venti_store::{Flight, Store, Visibility};

use super::Api;
use super::auth::Viewer;
use super::cache::{self, Keeper};
use super::log::Sparse;
use crate::flights::{picture, share};

const HTML: &str = "text/html; charset=utf-8";

/// How many preview pictures are drawn at the same time.
const PICTURES_AT_ONCE: usize = 2;

/// How many pictures are kept after they were drawn, and how many bytes of
/// them. A picture is some tens of kilobytes, so the count is what bounds
/// this in practice; the bytes are there so that the bound does not rest
/// on what a picture happens to weigh.
const PICTURES_KEPT: usize = 64;
const PICTURE_BYTES_KEPT: usize = 8 * 1024 * 1024;

/// The preview pictures of shared flights.
///
/// Drawing one takes about ten milliseconds of a whole core, and the
/// address of a picture is public once its flight is shared: anyone can
/// ask for it as often as they like, and a link posted to a chat is
/// fetched by every member's app. So a picture is drawn once and kept,
/// under the flight's address and its change number: what the picture
/// shows changes only together with that number. The last few dozen are
/// kept, and the one asked for longest ago makes room.
///
/// Whether a flight may be seen is decided before this is looked at, for
/// every request. A picture kept here is never an answer by itself.
#[derive(Debug)]
pub(crate) struct Pictures {
    turns: Semaphore,
    kept: Mutex<VecDeque<(String, Bytes)>>,
    /// How many pictures were drawn, for the tests of the above.
    drawn: AtomicU64,
}

impl Default for Pictures {
    fn default() -> Self {
        Self {
            turns: Semaphore::new(PICTURES_AT_ONCE),
            kept: Mutex::new(VecDeque::new()),
            drawn: AtomicU64::new(0),
        }
    }
}

impl Pictures {
    /// The picture kept under `key`, which then counts as asked for last.
    fn get(&self, key: &str) -> Option<Bytes> {
        let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        let at = kept.iter().position(|(kept_key, _)| kept_key == key)?;
        let entry = kept.remove(at)?;
        let picture = entry.1.clone();
        kept.push_back(entry);
        Some(picture)
    }

    /// Keeps a picture, and forgets the ones asked for longest ago until
    /// what is kept fits.
    fn put(&self, key: String, picture: Bytes) {
        let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        kept.retain(|(kept_key, _)| *kept_key != key);
        kept.push_back((key, picture));
        let bytes = |kept: &VecDeque<(String, Bytes)>| -> usize {
            kept.iter().map(|(_, picture)| picture.len()).sum()
        };
        while kept.len() > PICTURES_KEPT || bytes(&kept) > PICTURE_BYTES_KEPT {
            kept.pop_front();
        }
    }
}

/// What every answer about a shared flight carries, page or picture.
fn dress(response: &mut Response, flight: Option<&Flight>) {
    cache::set(response, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    // A link to an unlisted flight is the key to it. It is not passed on
    // to other sites in the Referer header.
    cache::set(response, header::REFERRER_POLICY, "same-origin");
    // Search engines may list what is public and nothing else.
    if flight.is_none_or(|flight| flight.visibility != Visibility::Public) {
        cache::set(response, HeaderName::from_static("x-robots-tag"), "noindex");
    }
}

fn html(mut response: Response) -> Response {
    cache::set(&mut response, header::CONTENT_TYPE, HTML);
    if let Ok(policy) = HeaderValue::from_str(&share::CONTENT_SECURITY_POLICY) {
        response
            .headers_mut()
            .insert(header::CONTENT_SECURITY_POLICY, policy);
    }
    response
}

/// The answer for a link that leads to nothing anyone may see.
fn nothing_here() -> Response {
    let mut response = html((StatusCode::NOT_FOUND, share::not_found_page()).into_response());
    cache::set(&mut response, header::CACHE_CONTROL, "no-store");
    dress(&mut response, None);
    response
}

fn keeper(flight: &Flight) -> Keeper {
    match flight.visibility {
        Visibility::Public => Keeper::Anyone,
        Visibility::Unlisted | Visibility::Private => Keeper::OnlyTheAsker,
    }
}

/// The answer when the server cannot look the flight up right now.
fn try_again() -> Response {
    let mut response =
        html((StatusCode::SERVICE_UNAVAILABLE, share::trouble_page()).into_response());
    cache::set(&mut response, header::CACHE_CONTROL, "no-store");
    cache::set(&mut response, header::RETRY_AFTER, "30");
    dress(&mut response, None);
    response
}

impl<S: Store> Api<S> {
    /// The flight a share link names, if anyone who has the link may see
    /// it; otherwise the answer to give instead.
    async fn shared_flight(&self, id: &str) -> Result<Flight, Response> {
        // Nobody is signed in here.
        self.visible_flight(id, &Viewer::nobody())
            .await
            .map_err(|error| match error.is_missing() {
                true => nothing_here(),
                false => try_again(),
            })
    }
}

impl<S: Store> Api<S> {
    /// The preview picture of a flight the asker may see, drawn now or
    /// taken from the ones drawn before. `key` names the design, the
    /// flight and its change number.
    async fn picture(&self, flight: &Flight, key: &str) -> Result<Bytes, Response> {
        if let Some(picture) = self.pictures.get(key) {
            return Ok(picture);
        }
        // Drawing takes some milliseconds of a whole core: a few at a time,
        // and not on the threads that answer requests.
        let Ok(_turn) = self.pictures.turns.acquire().await else {
            return Err(try_again());
        };
        // Whoever waited for a turn behind the one that drew this picture
        // finds it here.
        if let Some(picture) = self.pictures.get(key) {
            return Ok(picture);
        }
        let drawn = {
            let flight = flight.clone();
            tokio::task::spawn_blocking(move || picture::render(&flight)).await
        };
        match drawn {
            Ok(Ok(png)) => {
                self.pictures.drawn.fetch_add(1, Ordering::Relaxed);
                let picture = Bytes::from(png);
                self.pictures.put(key.to_owned(), picture.clone());
                Ok(picture)
            }
            failed => {
                static GATE: Sparse = Sparse::new();
                GATE.error(format_args!(
                    "the preview picture of a flight could not be drawn: {failed:?}"
                ));
                let mut response = StatusCode::INTERNAL_SERVER_ERROR.into_response();
                cache::set(&mut response, header::CACHE_CONTROL, "no-store");
                Err(response)
            }
        }
    }
}

/// `GET /f/{id}`: the page about one flight.
pub(crate) async fn page<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let flight = match api.shared_flight(&id).await {
        Ok(flight) => flight,
        Err(answer) => return answer,
    };
    let body = share::page(&flight, &api.public_url).into_bytes();
    let mut response = html(cache::shared(&headers, HTML, body, keeper(&flight)));
    dress(&mut response, Some(&flight));
    response
}

/// `GET /f/{id}/preview.png`: the picture that goes with the page.
pub(crate) async fn preview<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let flight = match api.shared_flight(&id).await {
        Ok(flight) => flight,
        Err(answer) => return answer,
    };
    // The picture depends on the flight's derived values alone, and those
    // change only together with its change number. So whether the asker's
    // copy is still good can be told without drawing anything.
    let drawn_from = format!("{} {} {}", picture::DESIGN, flight.id, flight.change);
    let tag = cache::etag(drawn_from.as_bytes());
    let mut response = if cache::client_has(&headers, &tag) {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        match api.picture(&flight, &drawn_from).await {
            Ok(png) => ([(header::CONTENT_TYPE, "image/png")], png).into_response(),
            Err(answer) => return answer,
        }
    };
    let cache_control = match keeper(&flight) {
        Keeper::Anyone => "public, no-cache",
        Keeper::OnlyTheAsker => "private, no-cache",
    };
    cache::set(&mut response, header::CACHE_CONTROL, cache_control);
    if let Ok(value) = HeaderValue::from_str(&tag) {
        response.headers_mut().insert(header::ETAG, value);
    }
    dress(&mut response, Some(&flight));
    response
}

#[cfg(test)]
mod tests {
    use venti_flight::igc;
    use venti_flight::testing::synthetic_flight;
    use venti_store::{Account, MemoryStore, NewAccount, Room};

    use super::*;
    use crate::accounts::password::Cost;
    use crate::clock::FixedClock;
    use crate::flights::analyser::{Analyser, Where};
    use crate::flights::derive::derive;
    use crate::flights::new_flight;
    use crate::flights::share::PublicUrl;

    /// An API over a store with one account, and the store.
    async fn api_with_an_account() -> (Arc<Api<MemoryStore>>, Arc<MemoryStore>, Account) {
        let now = crate::adapters::recorded_at();
        let store = Arc::new(MemoryStore::new());
        let account = Account {
            id: "00000000000000000000000001".to_owned(),
            handle: "dario".to_owned(),
            display_name: "Dario B.".to_owned(),
            created_at: now,
            publishing_suspended_at: None,
            edits_sites_since: None,
            new_flights_public: false,
        };
        let new = NewAccount {
            account: account.clone(),
            password_hash: "not a hash".to_owned(),
            invite: None,
        };
        store.create_account(&new).await.unwrap();
        let settings = crate::dev::settings(
            PublicUrl::new("https://venti.example").unwrap(),
            Analyser::new(Where::Thread),
            Cost::TESTS_ONLY,
        );
        let api = Api::new(store.clone(), Arc::new(FixedClock(now)), [], settings);
        (Arc::new(api), store, account)
    }

    /// Stores the sample flight flown on the given day of July 2026 and
    /// shares it. Returns its address.
    async fn shared_flight(store: &MemoryStore, account: &Account, day: u32) -> String {
        let now = crate::adapters::recorded_at();
        let text = igc::write(&synthetic_flight())
            .replace("HFDTEDATE:040726", &format!("HFDTEDATE:{day:02}0726"));
        let derivation = derive(text.as_bytes()).unwrap();
        let new = new_flight(&account.id, text.as_bytes(), derivation, now);
        store.save_flight(&new, &Room::UNLIMITED).await.unwrap();
        store
            .set_visibility(&account.id, &new.id, Visibility::Unlisted, now)
            .await
            .unwrap();
        new.id
    }

    async fn ask_for(api: &Arc<Api<MemoryStore>>, id: &str) -> (StatusCode, Bytes) {
        let response = preview(State(api.clone()), Path(id.to_owned()), HeaderMap::new()).await;
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, body)
    }

    #[tokio::test]
    async fn a_picture_is_drawn_once_however_often_it_is_asked_for() {
        let (api, store, account) = api_with_an_account().await;
        let id = shared_flight(&store, &account, 4).await;
        let drawn = || api.pictures.drawn.load(Ordering::Relaxed);

        let (status, first) = ask_for(&api, &id).await;
        assert_eq!(status, StatusCode::OK);
        assert!(first.starts_with(b"\x89PNG"));
        assert_eq!(drawn(), 1);

        // A link posted to a chat: every member's app fetches the picture.
        for _ in 0..200 {
            let (status, again) = ask_for(&api, &id).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(again, first);
        }
        assert_eq!(drawn(), 1);
        // All at once as well: whoever waits for a turn at drawing finds
        // the picture drawn. Two are drawn at a time, so two requests that
        // arrive in the same instant may both draw it, and no third.
        let other = shared_flight(&store, &account, 5).await;
        let together = (0..20).map(|_| ask_for(&api, &other));
        for (status, _) in futures::future::join_all(together).await {
            assert_eq!(status, StatusCode::OK);
        }
        let at_once = drawn() - 1;
        assert!(
            (1..=PICTURES_AT_ONCE as u64).contains(&at_once),
            "{at_once}"
        );

        // A change to the flight moves its change number: the picture kept
        // under the old one is not the answer any more.
        let now = crate::adapters::recorded_at();
        store
            .set_visibility(&account.id, &id, Visibility::Public, now)
            .await
            .unwrap();
        let before = drawn();
        assert_eq!(ask_for(&api, &id).await.0, StatusCode::OK);
        assert_eq!(drawn(), before + 1);

        // A flight made private is not answered from what is kept: who may
        // see a flight is asked first, every time.
        store
            .set_visibility(&account.id, &other, Visibility::Private, now)
            .await
            .unwrap();
        assert_eq!(ask_for(&api, &other).await.0, StatusCode::NOT_FOUND);
        assert_eq!(drawn(), before + 1);
    }

    #[test]
    fn only_the_pictures_asked_for_last_are_kept() {
        let pictures = Pictures::default();
        let picture = |n: usize| Bytes::from(vec![n as u8; 1000]);
        for n in 0..PICTURES_KEPT {
            pictures.put(format!("flight {n}"), picture(n));
        }
        // The first is asked for again, and is then the last asked for.
        assert_eq!(pictures.get("flight 0"), Some(picture(0)));

        // Ten more: the ten asked for longest ago make room, which are
        // the second to the eleventh.
        for n in PICTURES_KEPT..PICTURES_KEPT + 10 {
            pictures.put(format!("flight {n}"), picture(n));
        }
        assert_eq!(pictures.kept.lock().unwrap().len(), PICTURES_KEPT);
        assert!(pictures.get("flight 0").is_some());
        assert!(pictures.get("flight 1").is_none());
        assert!(pictures.get("flight 10").is_none());
        assert!(pictures.get("flight 11").is_some());

        // Pictures of an unusual weight are held to the bytes: three of
        // three megabytes do not fit into eight.
        for n in 0..3 {
            pictures.put(format!("large {n}"), Bytes::from(vec![0; 3 * 1024 * 1024]));
        }
        let kept = pictures.kept.lock().unwrap();
        let bytes: usize = kept.iter().map(|(_, picture)| picture.len()).sum();
        assert!(bytes <= PICTURE_BYTES_KEPT, "{bytes}");
        assert!(kept.iter().any(|(key, _)| key == "large 2"));
        assert!(!kept.iter().any(|(key, _)| key == "large 0"));
    }
}
