//! Public flights as data, through the real router: the moment a flight
//! became public, the list by pilot, what changed among the public flights
//! with those that were withdrawn, a flight's track as GeoJSON, the terms
//! every such answer carries, and the bound on how often an address reads.
//!
//! The first test is the one the piece is done by: a program that follows
//! the changes holds exactly the public flights, a day after flights were
//! made private, shared by link only and deleted too.

use std::collections::BTreeMap;

use axum::Router;
use axum::http::{Method, StatusCode, header};
use sha2::{Digest, Sha256};
use venti_api_types::v1::{
    ErrorCode, FLIGHT_TERMS_UNDECIDED, Flight, FlightId, FlightResponse, FlightTerms, FlightTrack,
    FlightsResponse, Handle, MAX_PAGE_SIZE, PUBLIC_FLIGHT_CHANGES_PATH, PUBLIC_FLIGHTS_PATH,
    PilotResponse, PublicFlightChangesRequest, PublicFlightChangesResponse, PublicFlightsRequest,
    RANKING_PATH, RankingResponse, Visibility, flight_file_path, flight_path, flight_track_path,
};
use venti_flight::testing::synthetic_flight;
use venti_server::dev::{
    DEMO_ACCESS_TOKEN, DEMO_HANDLE, DEMO_PILOTS, DEMO_TAKEOFF_PILOTS, DEMO_TOKEN,
};
use venti_sources::testing::assert_golden;
use venti_store::Store;

mod common;

use common::{
    Answer, TestClock, ask, get, golden, recorded, sample_file, sample_file_on, server,
    set_visibility, settings, sign_up, upload,
};

// --- Helpers ----------------------------------------------------------------

/// How a list of withdrawn flights names a flight, worked out as a program
/// of another maker would: SHA-256 of the address, in hexadecimal.
fn hash_of(id: &FlightId) -> String {
    Sha256::digest(id.as_str().as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

async fn changes(
    router: &Router,
    request: &PublicFlightChangesRequest,
) -> PublicFlightChangesResponse {
    get(router, &request.path_and_query())
        .await
        .ok(StatusCode::OK)
}

async fn public_list(router: &Router) -> Vec<Flight> {
    let answer: FlightsResponse = get(router, PUBLIC_FLIGHTS_PATH).await.ok(StatusCode::OK);
    assert_eq!(answer.next, None);
    answer.flights
}

/// A program that keeps a copy of the public flights: what it holds, by
/// address, and where it stands in the changes.
#[derive(Default)]
struct Mirror {
    held: BTreeMap<FlightId, Flight>,
    cursor: Option<String>,
    /// Of one pilot's flights only.
    pilot: Option<Handle>,
}

impl Mirror {
    /// Asks for what changed, a page at a time, until nothing is waiting,
    /// and does what the document tells a program to do with each answer.
    async fn follow(&mut self, router: &Router, page: usize) {
        loop {
            let answer = changes(
                router,
                &PublicFlightChangesRequest {
                    since: self.cursor.clone(),
                    pilot: self.pilot.clone(),
                    limit: Some(page),
                },
            )
            .await;
            assert!(answer.flights.len() + answer.withdrawn.len() <= page);
            assert_eq!(answer.terms, FlightTerms::current());
            if answer.reset {
                self.held.clear();
            }
            for flight in answer.flights {
                assert_eq!(flight.visibility, Visibility::Public);
                assert!(flight.published_at.is_some());
                self.held.insert(flight.id.clone(), flight);
            }
            for withdrawn in answer.withdrawn {
                self.held.retain(|id, _| hash_of(id) != withdrawn.id_sha256);
            }
            self.cursor = Some(answer.cursor);
            if !answer.more {
                return;
            }
        }
    }

    /// The copy holds exactly what the list of public flights holds, each
    /// flight as the list has it.
    async fn is_true(&self, router: &Router) {
        let mut listed = public_list(router).await;
        if let Some(pilot) = &self.pilot {
            listed.retain(|flight| flight.pilot.handle == *pilot);
        }
        let listed: BTreeMap<FlightId, Flight> = listed
            .into_iter()
            .map(|flight| (flight.id.clone(), flight))
            .collect();
        assert_eq!(self.held, listed);
    }
}

fn open_server(clock: &TestClock) -> (Router, std::sync::Arc<venti_store::MemoryStore>) {
    server(settings(), clock)
}

// --- Following ----------------------------------------------------------------

#[tokio::test]
async fn a_copy_that_follows_the_changes_holds_exactly_the_public_flights() {
    let clock = TestClock::new();
    let (router, store) = open_server(&clock);
    let minute = chrono::Duration::minutes(1);
    let anna = sign_up(&router, "anna").await;
    let ben = sign_up(&router, "ben").await;
    let cleo = sign_up(&router, "cleo").await;

    // Nothing is public: the changes are empty, and say where to go on.
    let mut mirror = Mirror::default();
    mirror.follow(&router, 100).await;
    assert_eq!(mirror.cursor.as_deref(), Some("0"));
    assert!(mirror.held.is_empty());

    // Seven flights of three pilots. Five become public, one is shared by
    // link, one stays private.
    let mut flights = Vec::new();
    for (pilot, day) in [
        (&anna, "150826"),
        (&anna, "160826"),
        (&anna, "170826"),
        (&ben, "150826"),
        (&ben, "160826"),
        (&cleo, "150826"),
        (&cleo, "160826"),
    ] {
        clock.advance(minute);
        let flight = upload(&router, &pilot.token, &sample_file_on(day))
            .await
            .flight;
        assert_eq!(flight.published_at, None);
        flights.push(flight.id);
    }
    let [a1, a2, a3, b1, b2, c1, c2] = &flights[..] else {
        panic!("seven flights")
    };
    for (pilot, flight) in [
        (&anna, a1),
        (&anna, a2),
        (&ben, b1),
        (&ben, b2),
        (&cleo, c1),
    ] {
        clock.advance(minute);
        set_visibility(&router, &pilot.token, flight, Visibility::Public).await;
    }
    clock.advance(minute);
    set_visibility(&router, &anna.token, a3, Visibility::Unlisted).await;
    let _private = c2;

    // Read a change at a time, and again from where it stood.
    mirror.follow(&router, 1).await;
    assert_eq!(mirror.held.len(), 5);
    mirror.is_true(&router).await;
    let stood = mirror.cursor.clone();
    mirror.follow(&router, 100).await;
    assert_eq!(mirror.cursor, stood);
    // Neither the flight shared by link nor the private one was ever in
    // an answer.
    assert!(!mirror.held.contains_key(a3) && !mirror.held.contains_key(c2));

    // Each way a flight leaves the public: made private, shared by link
    // only, deleted, gone with its account. And one that becomes public
    // meanwhile.
    clock.advance(minute);
    set_visibility(&router, &anna.token, a1, Visibility::Private).await;
    clock.advance(minute);
    set_visibility(&router, &anna.token, a2, Visibility::Unlisted).await;
    clock.advance(minute);
    let deleted = ask(Method::DELETE, &flight_path(b1))
        .token(&ben.token)
        .send(&router)
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    clock.advance(minute);
    set_visibility(&router, &anna.token, a3, Visibility::Public).await;
    let cleos = store.account("cleo").await.unwrap().unwrap();
    store.delete_account(&cleos.id).await.unwrap();

    // A day later the copy asks again, and holds what is public then:
    // nothing of what was withdrawn.
    clock.advance(chrono::Duration::days(1));
    let before = changes(
        &router,
        &PublicFlightChangesRequest {
            since: mirror.cursor.clone(),
            ..PublicFlightChangesRequest::default()
        },
    )
    .await;
    assert_eq!(before.flights.len(), 1);
    assert_eq!(&before.flights[0].id, a3);
    let withdrawn: Vec<&str> = before
        .withdrawn
        .iter()
        .map(|flight| flight.id_sha256.as_str())
        .collect();
    assert_eq!(
        withdrawn,
        [hash_of(a1), hash_of(a2), hash_of(b1), hash_of(c1)]
    );
    // When, for all but the flight that went with its account.
    assert!(
        before.withdrawn[..3]
            .iter()
            .all(|w| w.withdrawn_at.is_some())
    );
    assert_eq!(before.withdrawn[3].withdrawn_at, None);
    // No address of a withdrawn flight is in the answer: the one shared
    // by link is still read by whoever has its address.
    let text = serde_json::to_string(&before).unwrap();
    for gone in [a1, a2, b1, c1] {
        assert!(!text.contains(gone.as_str()));
    }

    mirror.follow(&router, 2).await;
    let held: Vec<&FlightId> = mirror.held.keys().collect();
    let mut public = vec![a3, b2];
    public.sort();
    assert_eq!(held, public);
    mirror.is_true(&router).await;

    // A program that begins now gets the same from the beginning.
    let mut late = Mirror::default();
    late.follow(&router, 100).await;
    assert_eq!(late.held, mirror.held);
    assert_eq!(late.cursor, mirror.cursor);

    // A cursor this server never gave out: it starts again and says so.
    let beyond = changes(
        &router,
        &PublicFlightChangesRequest {
            since: Some(i64::MAX.to_string()),
            ..PublicFlightChangesRequest::default()
        },
    )
    .await;
    assert!(beyond.reset);
    assert_eq!(beyond.flights.len(), 2);
    assert_eq!(Some(beyond.cursor), mirror.cursor);
}

#[tokio::test]
async fn the_changes_of_one_pilot_are_followed_like_those_of_everyone() {
    let clock = TestClock::new();
    let (router, _) = open_server(&clock);
    let minute = chrono::Duration::minutes(1);
    let anna = sign_up(&router, "anna").await;
    let ben = sign_up(&router, "ben").await;
    let of = |handle: &str| PublicFlightChangesRequest {
        pilot: Some(Handle::new(handle).unwrap()),
        ..PublicFlightChangesRequest::default()
    };

    // A pilot who shares nothing publicly is a pilot nobody has, here as
    // on the pilot's page, byte for byte.
    let nobody = get(&router, &of("nobody").path_and_query()).await;
    nobody.error(StatusCode::NOT_FOUND, ErrorCode::PilotNotFound);
    let a1 = upload(&router, &anna.token, &sample_file_on("150826")).await;
    let hidden = get(&router, &of("anna").path_and_query()).await;
    assert_eq!(hidden.body, nobody.body);
    let listed = get(&router, &format!("{PUBLIC_FLIGHTS_PATH}?pilot=anna")).await;
    assert_eq!(listed.body, nobody.body);
    let malformed = get(
        &router,
        &format!("{PUBLIC_FLIGHT_CHANGES_PATH}?pilot=No%20Handle"),
    )
    .await;
    assert_eq!(malformed.body, nobody.body);

    let a2 = upload(&router, &anna.token, &sample_file_on("160826")).await;
    let b1 = upload(&router, &ben.token, &sample_file_on("150826")).await;
    for (pilot, flight) in [(&anna, &a1), (&ben, &b1), (&anna, &a2)] {
        clock.advance(minute);
        set_visibility(&router, &pilot.token, &flight.flight.id, Visibility::Public).await;
    }

    let mut annas = Mirror {
        pilot: Some(Handle::new("anna").unwrap()),
        ..Mirror::default()
    };
    annas.follow(&router, 1).await;
    assert_eq!(annas.held.len(), 2);
    annas.is_true(&router).await;
    // The list by pilot holds the same, and narrows like the whole list.
    let listed: FlightsResponse = get(
        &router,
        &PublicFlightsRequest::default().path_and_query_of(&Handle::new("anna").unwrap()),
    )
    .await
    .ok(StatusCode::OK);
    assert_eq!(listed.flights.len(), 2);
    assert!(
        listed
            .flights
            .iter()
            .all(|f| f.pilot.handle.as_str() == "anna")
    );
    assert_eq!(listed.terms, Some(FlightTerms::current()));
    let one_day: FlightsResponse = get(
        &router,
        &format!(
            "{PUBLIC_FLIGHTS_PATH}?pilot=anna&from=2026-08-16T00:00:00Z&to=2026-08-17T00:00:00Z"
        ),
    )
    .await
    .ok(StatusCode::OK);
    assert_eq!(one_day.flights.len(), 1);
    assert_eq!(one_day.flights[0].id, a2.flight.id);
    let elsewhere: FlightsResponse = get(
        &router,
        &format!("{PUBLIC_FLIGHTS_PATH}?pilot=anna&bbox=9,46,10,47"),
    )
    .await
    .ok(StatusCode::OK);
    assert!(elsewhere.flights.is_empty());

    // One withdrawn: the pilot's changes say so, and nothing of Ben's.
    clock.advance(minute);
    set_visibility(&router, &anna.token, &a1.flight.id, Visibility::Private).await;
    clock.advance(minute);
    set_visibility(&router, &ben.token, &b1.flight.id, Visibility::Private).await;
    let since = changes(
        &router,
        &PublicFlightChangesRequest {
            since: annas.cursor.clone(),
            ..of("anna")
        },
    )
    .await;
    assert!(since.flights.is_empty());
    assert_eq!(since.withdrawn.len(), 1);
    assert_eq!(since.withdrawn[0].id_sha256, hash_of(&a1.flight.id));
    annas.follow(&router, 100).await;
    annas.is_true(&router).await;

    // The last one withdrawn: the pilot has no page any more, and that is
    // what a program that followed them is told.
    clock.advance(minute);
    set_visibility(&router, &anna.token, &a2.flight.id, Visibility::Unlisted).await;
    let gone = get(
        &router,
        &PublicFlightChangesRequest {
            since: annas.cursor.clone(),
            ..of("anna")
        }
        .path_and_query(),
    )
    .await;
    assert_eq!(gone.body, nobody.body);
}

// --- The moment, and the terms -------------------------------------------------

#[tokio::test]
async fn a_flight_says_since_when_it_is_public_and_under_what_it_is_given() {
    let clock = TestClock::new();
    let (router, _) = open_server(&clock);
    let anna = sign_up(&router, "anna").await;
    let id = upload(&router, &anna.token, &sample_file()).await.flight.id;
    let read = |token: Option<&'static str>| {
        let (router, path) = (&router, flight_path(&id));
        async move {
            let request = ask(Method::GET, &path);
            let request = match token {
                Some(token) => request.token(token),
                None => request,
            };
            request.send(router).await
        }
    };
    let annas: &'static str = Box::leak(anna.token.clone().into_boxed_str());

    // Private: its pilot reads it, and it is theirs alone.
    let private: FlightResponse = read(Some(annas)).await.ok(StatusCode::OK);
    assert_eq!(private.flight.published_at, None);
    assert_eq!(private.terms, None);
    let text = read(Some(annas)).await;
    assert!(!text.text().contains("published_at") && !text.text().contains("terms"));

    // Shared by link: not public, and given under the terms.
    clock.advance(chrono::Duration::minutes(5));
    set_visibility(&router, &anna.token, &id, Visibility::Unlisted).await;
    let by_link: FlightResponse = read(None).await.ok(StatusCode::OK);
    assert_eq!(by_link.flight.published_at, None);
    assert_eq!(by_link.terms, Some(FlightTerms::current()));

    // Public, from the moment it was made so.
    clock.advance(chrono::Duration::minutes(5));
    let published = venti_server::clock::Clock::now(&clock);
    set_visibility(&router, &anna.token, &id, Visibility::Public).await;
    clock.advance(chrono::Duration::hours(3));
    let public: FlightResponse = read(None).await.ok(StatusCode::OK);
    assert_eq!(public.flight.published_at, Some(published));
    assert_eq!(public.terms, Some(FlightTerms::current()));

    // Every answer that holds flights of others says the same, in the
    // one wording there is. It is not decided: it grants reading only.
    let terms = FlightTerms::current();
    assert_eq!(terms.text, FLIGHT_TERMS_UNDECIDED);
    assert_eq!((terms.licence.as_deref(), terms.decided), (None, false));
    let list: FlightsResponse = get(&router, PUBLIC_FLIGHTS_PATH).await.ok(StatusCode::OK);
    assert_eq!(list.terms.as_ref(), Some(&terms));
    assert_eq!(list.flights[0].published_at, Some(published));
    let page: PilotResponse = get(&router, "/v1/pilots/anna").await.ok(StatusCode::OK);
    assert_eq!(page.terms.as_ref(), Some(&terms));
    let ranking: RankingResponse = get(&router, RANKING_PATH).await.ok(StatusCode::OK);
    assert_eq!(ranking.terms.as_ref(), Some(&terms));
    let track: FlightTrack = get(&router, &flight_track_path(&id))
        .await
        .ok(StatusCode::OK);
    assert_eq!(track.terms.as_ref(), Some(&terms));
    // The pilot's own list is the pilot's own.
    let mine: FlightsResponse = ask(Method::GET, "/v1/flights")
        .token(&anna.token)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(mine.terms, None);
    assert_eq!(mine.flights[0].published_at, Some(published));

    // Withdrawn and public again: public since the second time.
    set_visibility(&router, &anna.token, &id, Visibility::Private).await;
    let private: FlightResponse = read(Some(annas)).await.ok(StatusCode::OK);
    assert_eq!(private.flight.published_at, None);
    clock.advance(chrono::Duration::hours(1));
    let again = venti_server::clock::Clock::now(&clock);
    set_visibility(&router, &anna.token, &id, Visibility::Public).await;
    let public: FlightResponse = read(None).await.ok(StatusCode::OK);
    assert_eq!(public.flight.published_at, Some(again));
}

// --- The track -------------------------------------------------------------------

#[tokio::test]
async fn a_track_is_read_by_whoever_may_see_the_flight() {
    let clock = TestClock::new();
    let (router, _) = open_server(&clock);
    let anna = sign_up(&router, "anna").await;
    let ben = sign_up(&router, "ben").await;
    let id = upload(&router, &anna.token, &sample_file()).await.flight.id;
    let path = flight_track_path(&id);
    assert_eq!(path, format!("/v1/flights/{id}/track"));

    // Private: its pilot, and nobody else. To anyone else it is a flight
    // that does not exist, as its record and its file are.
    let nowhere = get(&router, &flight_path(&id)).await;
    nowhere.error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
    assert_eq!(get(&router, &path).await.body, nowhere.body);
    let bens = ask(Method::GET, &path)
        .token(&ben.token)
        .send(&router)
        .await;
    assert_eq!(bens.body, nowhere.body);
    let own = ask(Method::GET, &path)
        .token(&anna.token)
        .send(&router)
        .await;
    own.is_private();
    assert_eq!(own.header(header::CONTENT_TYPE), "application/geo+json");
    let own: FlightTrack = own.ok(StatusCode::OK);
    assert_eq!(own.terms, None);

    // Every fix of the file, in its order.
    let sample = synthetic_flight();
    let fixes = &sample.fixes;
    assert_eq!(own.kind, "Feature");
    assert_eq!(own.id, id);
    assert_eq!(own.geometry.kind, "LineString");
    assert_eq!(own.geometry.coordinates.len(), fixes.len());
    assert_eq!(own.properties.times_s.len(), fixes.len());
    assert_eq!(
        (own.properties.fixes, own.properties.every),
        (fixes.len(), 1)
    );
    assert_eq!(own.properties.start, fixes[0].time);
    // The take-off and the landing are those of the flight's record.
    let record: FlightResponse = ask(Method::GET, &flight_path(&id))
        .token(&anna.token)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    let takeoff = record.flight.takeoff.unwrap();
    let at = own.properties.takeoff_s.unwrap();
    assert_eq!(fixes[0].time + chrono::Duration::seconds(at), takeoff.time);
    let index = own
        .properties
        .times_s
        .iter()
        .position(|s| *s == at)
        .unwrap();
    let [lon, lat, altitude] = own.geometry.coordinates[index];
    assert_eq!((lat, lon), (takeoff.position.lat(), takeoff.position.lon()));
    assert_eq!(altitude, f64::from(takeoff.altitude_m));

    // Shared by link and public: anyone, without a token.
    for visibility in [Visibility::Unlisted, Visibility::Public] {
        set_visibility(&router, &anna.token, &id, visibility).await;
        let answer = get(&router, &path).await;
        assert_eq!(answer.header(header::CONTENT_TYPE), "application/geo+json");
        let track: FlightTrack = answer.ok(StatusCode::OK);
        assert_eq!(track.terms, Some(FlightTerms::current()));
        assert_eq!(track.geometry, own.geometry);
        assert_eq!(track.properties, own.properties);
        // Checked before every reuse, as the flight's record is.
        let tag = answer.header(header::ETAG).to_owned();
        let again = ask(Method::GET, &path)
            .header(header::IF_NONE_MATCH, &tag)
            .send(&router)
            .await;
        assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    }

    // Beside the track, the file as it was uploaded is still the file.
    let file = get(&router, &flight_file_path(&id)).await;
    assert_eq!(file.body, sample_file());

    // No flight, no track.
    for none in ["0123456789abcdefghjkmnpqrs", "public", "nonsense"] {
        get(&router, &format!("/v1/flights/{none}/track"))
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
    }
}

// --- The bound -------------------------------------------------------------------

#[tokio::test]
async fn an_address_reads_public_flights_only_so_often_in_an_hour() {
    let clock = TestClock::new();
    let mut settings = settings();
    settings.limits.public_flight_reads_per_address = 3;
    settings.limits.public_flight_reads_per_network = 5;
    let (router, _) = server(settings, &clock);
    let anna = sign_up(&router, "anna").await;
    let id = upload(&router, &anna.token, &sample_file()).await.flight.id;
    set_visibility(&router, &anna.token, &id, Visibility::Public).await;
    let from = |peer: &'static str, path: String| {
        let router = &router;
        async move { ask(Method::GET, &path).from(peer).send(router).await }
    };
    let reads = [
        PUBLIC_FLIGHTS_PATH.to_owned(),
        PUBLIC_FLIGHT_CHANGES_PATH.to_owned(),
        flight_track_path(&id),
    ];

    // The three kinds of read share one count.
    for path in &reads {
        assert_eq!(
            from("203.0.113.7", path.clone()).await.status,
            StatusCode::OK
        );
    }
    for path in &reads {
        let refused = from("203.0.113.7", path.clone()).await;
        let message = refused.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
        assert!(message.contains("public flights"), "{message}");
        assert_eq!(refused.header(header::RETRY_AFTER), "3600");
    }
    // Another address is not held up, and what is not such a read is not
    // counted: a flight by its address, its file, a pilot's page.
    assert_eq!(
        from("203.0.113.8", reads[0].clone()).await.status,
        StatusCode::OK
    );
    for path in [
        flight_path(&id),
        flight_file_path(&id),
        "/v1/pilots/anna".to_owned(),
    ] {
        assert_eq!(from("203.0.113.7", path).await.status, StatusCode::OK);
    }
    // A token does not lift it: the bound is the address's.
    let with_a_token = ask(Method::GET, &reads[2])
        .token(&anna.token)
        .from("203.0.113.7")
        .send(&router)
        .await;
    assert_eq!(with_a_token.status, StatusCode::TOO_MANY_REQUESTS);

    // An hour later the address reads again.
    clock.advance(chrono::Duration::minutes(59));
    assert_eq!(
        from("203.0.113.7", reads[0].clone()).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );
    clock.advance(chrono::Duration::minutes(2));
    assert_eq!(
        from("203.0.113.7", reads[0].clone()).await.status,
        StatusCode::OK
    );

    // IPv6: counted by the /64, and the /48 around it has its own bound.
    for host in ["2001:db8:1:1::1", "2001:db8:1:1::2", "2001:db8:1:1::3"] {
        assert_eq!(from(host, reads[0].clone()).await.status, StatusCode::OK);
    }
    assert_eq!(
        from("2001:db8:1:1::4", reads[0].clone()).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );
    for host in ["2001:db8:1:2::1", "2001:db8:1:3::1"] {
        assert_eq!(from(host, reads[0].clone()).await.status, StatusCode::OK);
    }
    assert_eq!(
        from("2001:db8:1:4::1", reads[0].clone()).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn a_question_about_the_changes_that_cannot_be_read_is_explained() {
    let clock = TestClock::new();
    let (router, _) = open_server(&clock);
    for bad in ["since=yesterday", "since=-1", "since=1.5", "since="] {
        get(&router, &format!("{PUBLIC_FLIGHT_CHANGES_PATH}?{bad}"))
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidCursor);
    }
    for bad in [
        "limit=0",
        "limit=many",
        &format!("limit={}", MAX_PAGE_SIZE + 1),
    ] {
        get(&router, &format!("{PUBLIC_FLIGHT_CHANGES_PATH}?{bad}"))
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidLimit);
    }
    // The address of a flight is not read as a word of this path, nor the
    // other way round.
    get(&router, "/v1/flights/public/track")
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
}

// --- The development mode ------------------------------------------------------

/// An answer short enough to stand in the documentation: outlines cut to
/// two points.
fn short(mut answer: PublicFlightChangesResponse) -> PublicFlightChangesResponse {
    for flight in &mut answer.flights {
        flight.outline.truncate(2);
    }
    answer
}

#[tokio::test]
async fn the_development_mode_has_eleven_flights_to_follow_and_one_withdrawn() {
    let router = recorded().await;
    let [mara, jonas] = DEMO_PILOTS.map(|pilot| pilot.handle);
    let [lena, timo, ines] = DEMO_TAKEOFF_PILOTS.map(|pilot| pilot.handle);

    let mut mirror = Mirror::default();
    mirror.follow(&router, 100).await;
    assert_eq!(mirror.held.len(), 11);
    mirror.is_true(&router).await;
    let all = changes(&router, &PublicFlightChangesRequest::default()).await;
    let by: Vec<&str> = all
        .flights
        .iter()
        .map(|flight| flight.pilot.handle.as_str())
        .collect();
    // In the order they were made public: the pilots of the development
    // mode one after the other, as it puts them in.
    assert_eq!(
        by,
        [
            DEMO_HANDLE,
            mara,
            mara,
            jonas,
            jonas,
            lena,
            lena,
            lena,
            timo,
            timo,
            ines
        ]
    );
    assert_eq!(all.withdrawn.len(), 1);
    assert!(!all.more && !all.reset);

    // The flight that was withdrawn is the one Mara shares by link: a
    // program that had kept it finds it by the hash of its address.
    let maras: FlightsResponse = ask(Method::GET, "/v1/flights")
        .token(&sign_in(&router, mara).await)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    let by_link: Vec<&Flight> = maras
        .flights
        .iter()
        .filter(|flight| flight.visibility == Visibility::Unlisted)
        .collect();
    assert_eq!(by_link.len(), 1);
    assert_eq!(all.withdrawn[0].id_sha256, hash_of(&by_link[0].id));
    assert_eq!(by_link[0].published_at, None);

    // The examples of the documentation: the first four changes, of which
    // the document shows none, and what came after them.
    let first = changes(
        &router,
        &PublicFlightChangesRequest {
            limit: Some(4),
            ..PublicFlightChangesRequest::default()
        },
    )
    .await;
    assert!(first.more);
    assert_eq!(first.flights.len(), 4);
    // The document names this cursor.
    assert_eq!(first.cursor, "1795028700000003");
    // The two after them, which the document shows: Jonas's second flight
    // and the one that was withdrawn, with more to come.
    let then = PublicFlightChangesRequest {
        since: Some(first.cursor.clone()),
        limit: Some(2),
        ..PublicFlightChangesRequest::default()
    };
    assert_eq!(
        then.path_and_query(),
        format!("/v1/flights/public/changes?since={}&limit=2", first.cursor)
    );
    let next = changes(&router, &then).await;
    assert_eq!((next.flights.len(), next.withdrawn.len()), (1, 1));
    assert!(next.more);
    let rest = changes(
        &router,
        &PublicFlightChangesRequest {
            since: Some(next.cursor.clone()),
            ..PublicFlightChangesRequest::default()
        },
    )
    .await;
    // And the rest: the six flights that say something about take-offs.
    assert_eq!((rest.flights.len(), rest.withdrawn.len()), (6, 0));
    assert!(!rest.more);
    assert_eq!(rest.cursor, all.cursor);
    assert_golden(&golden("public_flight_changes.json"), &short(next.clone()));

    // One pilot's.
    let maras = changes(
        &router,
        &PublicFlightChangesRequest {
            pilot: Some(Handle::new(mara).unwrap()),
            ..PublicFlightChangesRequest::default()
        },
    )
    .await;
    assert_eq!((maras.flights.len(), maras.withdrawn.len()), (2, 1));
    // Her cursor stands at her last change, the withdrawal.
    assert_eq!(maras.cursor, next.cursor);

    // The track of the demo account's public flight, cut to its first
    // three positions for the document.
    let public = &all.flights[0];
    let answer: Answer = get(&router, &flight_track_path(&public.id)).await;
    let mut track: FlightTrack = answer.ok(StatusCode::OK);
    assert_eq!(track.properties.fixes, track.geometry.coordinates.len());
    assert!(track.properties.fixes > 1000);
    track.geometry.coordinates.truncate(3);
    track.properties.times_s.truncate(3);
    if let Some(pressure) = &mut track.properties.pressure_altitudes_m {
        pressure.truncate(3);
    }
    assert_golden(&golden("flight_track.json"), &track);

    // The demo account's token for a program reads the track of its
    // private flight, which nobody else does.
    let mine: FlightsResponse = ask(Method::GET, "/v1/flights")
        .token(DEMO_TOKEN)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    let private = mine
        .flights
        .iter()
        .find(|flight| flight.visibility == Visibility::Private)
        .unwrap();
    let path = flight_track_path(&private.id);
    get(&router, &path)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
    let own: FlightTrack = ask(Method::GET, &path)
        .token(DEMO_ACCESS_TOKEN)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(own.id, private.id);
    assert_eq!(own.terms, None);
}

/// Signs one of the development mode's pilots in and returns the token.
async fn sign_in(router: &Router, handle: &str) -> String {
    let answer = common::try_sign_in(router, handle, venti_server::dev::DEMO_PASSWORD).await;
    let signed: venti_api_types::v1::SignInResponse = answer.ok(StatusCode::CREATED);
    signed.token
}
