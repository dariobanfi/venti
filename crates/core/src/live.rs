//! Live tracks: letting others see a flight while it is under way, and
//! seeing who is in the air.
//!
//! As with everything the server is asked, the core sends nothing itself.
//! While a flight is shared it collects the positions, says when a request
//! is due and what is in it, and takes the answer; the shell carries the
//! request to the server with the pilot's token. What the server expects
//! of an app is in `docs/api.md` under "Live tracks", and [`LiveShare`]
//! follows it point by point.
//!
//! Nothing is shared unless the pilot switched it on for this flight, and
//! nothing before take-off: where a pilot walks about on the ground is
//! nobody's business.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::DateTime;
use venti_api_types::v1::{
    self as api, LiveFix, LivePositionsRequest, LivePositionsResponse, LiveTrack, LiveTrackId,
    LiveTrackRequest, LiveTrackResponse, LiveTracksRequest, LiveTracksResponse, LiveVisibility,
};
use venti_domain::{BoundingBox, GeoPoint, Timestamp};

use crate::server::{answer, code};
use crate::{Coordinate, CoreError, ServerRequest, TrailPointView};

/// Who may see a flight while it is under way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum LiveAudience {
    /// Whoever was given the link. The flight is on nobody's map.
    Link,
    /// Everyone who looks at the server's map.
    Public,
}

impl LiveAudience {
    fn visibility(self) -> LiveVisibility {
        match self {
            Self::Link => LiveVisibility::Link,
            Self::Public => LiveVisibility::Public,
        }
    }
}

/// Where the sharing of a flight stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum LiveShareState {
    /// Positions go out as they come, or will from take-off.
    Sharing,
    /// Positions wait on this device: no network, the server asked to
    /// wait, or the device is signed out. They are sent when that is over.
    Waiting,
    /// The glider has landed and the server knows. Nothing more is sent
    /// unless it flies on.
    Ended,
    /// The server takes no more for this track, or the pilot deleted it.
    /// Nothing more is sent.
    Stopped,
}

/// The sharing of the flight in the cockpit, for a line on the screen.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LiveShareView {
    /// Who may see it.
    pub audience: LiveAudience,
    /// The address to hand on: whoever opens it sees the flight. It is
    /// the key to the track, so it belongs in no log.
    pub link: String,
    /// Where the sharing stands.
    pub state: LiveShareState,
    /// Positions the server has taken.
    pub taken: u32,
    /// Positions waiting on this device.
    pub waiting: u32,
    /// Seconds since the server last took positions. Absent before the
    /// first.
    pub last_taken_age_s: Option<f64>,
    /// The same in a line: "Sent 8 s ago", "No network: 40 positions
    /// wait", "Begins at take-off".
    pub words: String,
}

/// A request goes out this often while positions come in, seconds. The
/// server asks for ten to fifteen.
const SEND_EVERY_S: f64 = 15.0;

/// The most positions in one request. The server takes 120, and 300 a
/// minute from one account: sixty every quarter minute stays under that
/// while a gap in the network is filled, with room for the new ones.
const AT_A_TIME: usize = 60;

/// The most positions kept while they cannot be sent: what a track holds,
/// twelve hours at one a second. Beyond that the oldest go.
const WAITING_AT_MOST: usize = 43_200;

/// How long to wait after a request that got no answer: this many seconds
/// the first time, twice as long each time after, a minute at most.
const FIRST_RETRY_S: f64 = 5.0;
const LONGEST_RETRY_S: f64 = 60.0;

/// One flight being shared: the positions not yet taken by the server,
/// and what was last said to it.
#[derive(Debug)]
pub(crate) struct LiveShare {
    id: LiveTrackId,
    /// The address of the pilot's server, without a slash at its end.
    server: String,
    audience: LiveAudience,
    /// Positions the server has not taken yet, oldest first.
    waiting: VecDeque<LiveFix>,
    /// The newest position there ever was, for a request that only says
    /// something.
    newest: Option<LiveFix>,
    /// The request under way, if one is.
    sent: Option<Sent>,
    landed: bool,
    /// What the server was last told and took: whether the glider has
    /// landed, as of which position, and who may see the track.
    told_landed: bool,
    told_newest_s: Option<i64>,
    told_audience: Option<LiveAudience>,
    last_request_s: Option<f64>,
    last_taken_s: Option<f64>,
    not_before_s: f64,
    failures: u32,
    taken: u32,
    /// The page's address as the server gives it, once it has.
    page: Option<String>,
    /// Why positions wait, in words, while they do.
    problem: Option<String>,
    /// Why nothing more is sent, in words, once that is so.
    stopped: Option<String>,
}

/// A request under way.
#[derive(Debug)]
struct Sent {
    /// The seconds of its fixes, oldest first.
    times: Vec<i64>,
    landed: bool,
    audience: LiveAudience,
}

impl LiveShare {
    /// Begins sharing. `random` is 26 bytes or more from the system's
    /// source of random numbers for secrets: the track's identifier is made
    /// of them, and whoever knows it sees the track.
    pub(crate) fn begin(
        server: &str,
        random: &[u8],
        audience: LiveAudience,
    ) -> Result<Self, CoreError> {
        let bytes: &[u8; LiveTrackId::LENGTH] = random
            .get(..LiveTrackId::LENGTH)
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| CoreError::Server {
                message: "The app could not make a link for this flight.".to_owned(),
            })?;
        // Twenty-six equal bytes are no random numbers. A shell that
        // forgot to fill them would give every flight the same link.
        if bytes.iter().all(|byte| *byte == bytes[0]) {
            return Err(CoreError::Server {
                message: "The app could not make a link for this flight.".to_owned(),
            });
        }
        let server = server.trim().trim_end_matches('/');
        if !(server.starts_with("https://") || server.starts_with("http://")) {
            return Err(CoreError::Server {
                message: "Sign in to a Venti server to share a flight.".to_owned(),
            });
        }
        Ok(Self {
            id: LiveTrackId::from_random(bytes),
            server: server.to_owned(),
            audience,
            waiting: VecDeque::new(),
            newest: None,
            sent: None,
            landed: false,
            told_landed: false,
            told_newest_s: None,
            told_audience: None,
            last_request_s: None,
            last_taken_s: None,
            not_before_s: 0.0,
            failures: 0,
            taken: 0,
            page: None,
            problem: None,
            stopped: None,
        })
    }

    /// Takes a position of the flight. One a second at most: the server
    /// keeps one for a second, and the first it gets.
    pub(crate) fn add(&mut self, fix: LiveFix) {
        if self.stopped.is_some() {
            return;
        }
        let second = fix.time.timestamp();
        if self
            .newest
            .is_some_and(|newest| newest.time.timestamp() >= second)
        {
            return;
        }
        self.newest = Some(fix);
        if self.waiting.len() == WAITING_AT_MOST {
            self.waiting.pop_front();
        }
        self.waiting.push_back(fix);
    }

    /// Says whether the glider has landed. A glider that lands and flies
    /// on is flying.
    pub(crate) fn set_landed(&mut self, landed: bool) {
        self.landed = landed;
    }

    /// Changes who may see the flight. Narrowing it is said to the server
    /// at once; widening it with the next positions.
    pub(crate) fn set_audience(&mut self, audience: LiveAudience) {
        self.audience = audience;
    }

    /// Whether there is something to tell the server that should not wait
    /// for the next positions: that the glider has landed, or that fewer
    /// people may see the track.
    fn has_something_to_say(&self) -> bool {
        let newest = self.newest.map(|fix| fix.time.timestamp());
        let landing = self.landed && !(self.told_landed && self.told_newest_s == newest);
        let narrowing =
            self.audience == LiveAudience::Link && self.told_audience == Some(LiveAudience::Public);
        self.newest.is_some() && (landing || narrowing)
    }

    /// The request to send now, if one is due. `None` while another is
    /// under way, while the server asked to wait, and while there is
    /// nothing to send.
    pub(crate) fn request(&mut self, now_s: f64) -> Option<ServerRequest> {
        if self.stopped.is_some() || self.sent.is_some() || now_s < self.not_before_s {
            return None;
        }
        let say_now = self.has_something_to_say();
        let due = self
            .last_request_s
            .is_none_or(|last| now_s - last >= SEND_EVERY_S);
        // The newest positions first: after a gap, those who watch see
        // where the glider is before they see where it was.
        let fixes: Vec<LiveFix> = if self.waiting.is_empty() {
            if !say_now {
                return None;
            }
            vec![self.newest?]
        } else if due || say_now {
            let from = self.waiting.len().saturating_sub(AT_A_TIME);
            self.waiting.iter().skip(from).copied().collect()
        } else {
            return None;
        };

        let body = serde_json::to_vec(&LivePositionsRequest {
            id: self.id.to_string(),
            visibility: self.audience.visibility(),
            landed: self.landed,
            fixes: fixes.clone(),
        })
        .ok()?;
        self.sent = Some(Sent {
            times: fixes.iter().map(|fix| fix.time.timestamp()).collect(),
            landed: self.landed,
            audience: self.audience,
        });
        self.last_request_s = Some(now_s);
        Some(ServerRequest {
            method: "POST".to_owned(),
            path: api::LIVE_POSITIONS_PATH.to_owned(),
            body: Some(body),
            content_type: Some("application/json".to_owned()),
            needs_token: true,
        })
    }

    /// Takes the server's answer to the request under way.
    pub(crate) fn answer(
        &mut self,
        status: u16,
        body: &[u8],
        retry_after_s: Option<u32>,
        now_s: f64,
    ) {
        let Some(sent) = self.sent.take() else {
            return;
        };
        match status {
            200..=299 => {
                // Taken, or refused for good: either way these are done
                // with, whatever the counts in the answer say.
                let before = self.waiting.len();
                self.drop_sent(&sent);
                self.taken = self
                    .taken
                    .saturating_add(u32::try_from(before - self.waiting.len()).unwrap_or(0));
                self.last_taken_s = Some(now_s);
                self.failures = 0;
                self.problem = None;
                self.told_landed = sent.landed;
                self.told_newest_s = sent.times.last().copied();
                self.told_audience = Some(sent.audience);
                if let Ok(response) = serde_json::from_slice::<LivePositionsResponse>(body) {
                    self.page = self.own_page(&response.page);
                }
            }
            // Asked to wait: the positions stay and go out afterwards.
            429 | 503 => {
                let wait = f64::from(retry_after_s.unwrap_or(30).clamp(5, 120));
                self.not_before_s = now_s + wait;
                self.problem = Some("The server asked to wait".to_owned());
            }
            401 => {
                self.not_before_s = now_s + LONGEST_RETRY_S;
                self.problem = Some("This device is signed out".to_owned());
            }
            // The identifier takes no more, from anyone.
            403 | 409 | 410 => {
                self.stopped = Some(
                    match code(body).as_deref() {
                        Some("live_track_full") => {
                            "The track is full: it holds twelve hours. Share again for a new link."
                        }
                        Some("live_track_gone") => "The track was deleted or has run out.",
                        _ => "The server takes no more positions for this track.",
                    }
                    .to_owned(),
                );
                self.waiting.clear();
            }
            // A server from before live tracks.
            404 | 405 => {
                self.stopped = Some("This server has no live tracking.".to_owned());
                self.waiting.clear();
            }
            // Refused for good: what the server would not take once it
            // will not take the next time. The flight goes on.
            400 | 413 | 415 | 422 => {
                self.drop_sent(&sent);
                self.failures = 0;
                self.problem = (code(body).as_deref() == Some("fixes_out_of_time"))
                    .then(|| "This device's clock is wrong".to_owned());
            }
            _ => self.wait_after_failure(now_s),
        }
    }

    /// Notes that the request under way got no answer. Its positions stay
    /// and go out again; sent twice, they change nothing on the server.
    pub(crate) fn no_answer(&mut self, now_s: f64) {
        if self.sent.take().is_some() {
            self.wait_after_failure(now_s);
        }
    }

    fn wait_after_failure(&mut self, now_s: f64) {
        self.failures = self.failures.saturating_add(1);
        let wait = FIRST_RETRY_S * 2.0_f64.powi(self.failures.min(8) as i32 - 1);
        self.not_before_s = now_s + wait.min(LONGEST_RETRY_S);
        // The next try goes out when that wait is over, not a quarter
        // minute after this one.
        self.last_request_s = None;
        self.problem = Some("No network".to_owned());
    }

    fn drop_sent(&mut self, sent: &Sent) {
        self.waiting
            .retain(|fix| sent.times.binary_search(&fix.time.timestamp()).is_err());
    }

    /// The server's address of the page, if it is one for this track. A
    /// server's answer is not handed to the pilot to pass on as it is.
    fn own_page(&self, page: &str) -> Option<String> {
        let ends_right = page.ends_with(&api::live_page_path(&self.id));
        let plain = page.len() <= 200
            && (page.starts_with("https://") || page.starts_with("http://"))
            && page.chars().all(|c| c.is_ascii_graphic());
        (ends_right && plain).then(|| page.to_owned())
    }

    /// The request that deletes the track on the server, for the pilot who
    /// wants it gone. Nothing more is sent after it.
    pub(crate) fn deletion(&mut self) -> ServerRequest {
        self.stopped = Some("You deleted the track.".to_owned());
        self.waiting.clear();
        self.sent = None;
        ServerRequest {
            method: "DELETE".to_owned(),
            path: api::live_track_path(&self.id),
            body: None,
            content_type: None,
            needs_token: true,
        }
    }

    pub(crate) fn view(&self, now_s: f64) -> LiveShareView {
        let waiting = u32::try_from(self.waiting.len()).unwrap_or(u32::MAX);
        let age = self.last_taken_s.map(|taken| (now_s - taken).max(0.0));
        let ended = self.landed && self.waiting.is_empty() && !self.has_something_to_say();
        let (state, words) = if let Some(reason) = &self.stopped {
            (LiveShareState::Stopped, reason.clone())
        } else if let Some(problem) = &self.problem {
            (
                LiveShareState::Waiting,
                match waiting {
                    0 => problem.clone(),
                    1 => format!("{problem}: 1 position waits"),
                    _ => format!("{problem}: {waiting} positions wait"),
                },
            )
        } else if self.newest.is_none() {
            (LiveShareState::Sharing, "Begins at take-off".to_owned())
        } else if ended && self.taken > 0 {
            (
                LiveShareState::Ended,
                "Landed. The last position is sent.".to_owned(),
            )
        } else {
            (
                LiveShareState::Sharing,
                match age {
                    Some(age) => format!("Sent {}", ago(age)),
                    None => "Sending the first position".to_owned(),
                },
            )
        };
        LiveShareView {
            audience: self.audience,
            link: self
                .page
                .clone()
                .unwrap_or_else(|| format!("{}{}", self.server, api::live_page_path(&self.id))),
            state,
            taken: self.taken,
            waiting,
            last_taken_age_s: age,
            words,
        }
    }
}

/// A position for a live track, from what the cockpit knows. `None` for a
/// place that is none or a time that is none.
pub(crate) fn fix(
    time_unix_s: f64,
    position: GeoPoint,
    altitude_m: f64,
    ground_speed_ms: Option<f64>,
    climb_ms: Option<f64>,
    course_deg: Option<f64>,
) -> Option<LiveFix> {
    // To the metre, the tenth and the degree: finer than that is noise,
    // and it is sent a few thousand times a flight.
    let round = |value: f64, steps: f64| (value * steps).round() / steps;
    let moving = ground_speed_ms.is_some_and(|speed| speed > 1.0);
    Some(LiveFix {
        time: DateTime::from_timestamp(time_unix_s.floor() as i64, 0)?,
        position: GeoPoint::new(
            round(position.lat(), 1_000_000.0),
            round(position.lon(), 1_000_000.0),
        )
        .ok()?,
        altitude_m: altitude_m.round().clamp(-1000.0, 20_000.0) as i32,
        ground_speed_ms: ground_speed_ms
            .filter(|speed| speed.is_finite())
            .map(|speed| round(speed.clamp(0.0, 150.0), 10.0)),
        climb_ms: climb_ms
            .filter(|climb| climb.is_finite())
            .map(|climb| round(climb.clamp(-100.0, 100.0), 10.0)),
        // A glider that stands still has no direction.
        course_deg: course_deg
            .filter(|course| moving && course.is_finite())
            .map(|course| course.rem_euclid(360.0).round().rem_euclid(360.0)),
    })
}

/// "8 s ago", "3 min ago", "2 h ago".
fn ago(age_s: f64) -> String {
    let seconds = age_s.max(0.0).round() as u64;
    match seconds {
        0..=89 => format!("{seconds} s ago"),
        90..=5399 => format!("{} min ago", (seconds + 30) / 60),
        _ => format!("{} h ago", (seconds + 1800) / 3600),
    }
}

/// A glider on the map of who is in the air.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct LiveGlider {
    /// The track's identifier.
    pub id: String,
    /// The pilot's handle.
    pub handle: String,
    /// The name the pilot is shown by, on one line.
    pub name: String,
    /// Where the glider is, or was last heard of.
    pub position: Coordinate,
    /// Its altitude there, metres above sea level.
    pub altitude_m: i32,
    /// Speed over the ground, metres per second.
    pub ground_speed_ms: Option<f64>,
    /// Climb rate, metres per second.
    pub climb_ms: Option<f64>,
    /// The direction it moves in, degrees clockwise from north.
    pub course_deg: Option<f64>,
    /// Whether the pilot's app has said that it landed. A glider nothing
    /// was heard of for a while has not landed: it has no network.
    pub landed: bool,
    /// How old the position was when the server answered, seconds. The
    /// shell adds the time since.
    pub age_s: f64,
    /// Whether nothing was heard of the glider for two minutes or more,
    /// while its pilot has not said that it landed. A map draws it held
    /// back: it is where the glider was, not where it is.
    pub quiet: bool,
    /// For the map: "Demo Pilot · 1596 m".
    pub label: String,
    /// The path of the track's page on the server: "/live/…".
    pub page_path: String,
}

/// Who is in the air in one part of the map.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct LiveMap {
    /// The gliders whose pilots show their flight to everyone.
    pub gliders: Vec<LiveGlider>,
    /// Whether there are more than the server lists at once: zoom in.
    pub truncated: bool,
}

/// The path and query to `GET` from the server for the gliders in a box.
/// Ask again every ten to fifteen seconds while the map is looked at.
#[uniffi::export]
pub fn live_map_path(south: f64, west: f64, north: f64, east: f64) -> Result<String, CoreError> {
    let bbox = BoundingBox::new(
        south.clamp(-90.0, 90.0),
        west.clamp(-180.0, 180.0),
        north.clamp(-90.0, 90.0),
        east.clamp(-180.0, 180.0),
    )
    .map_err(|_| CoreError::Server {
        message: "The app could not ask the server about this part of the map.".to_owned(),
    })?;
    Ok(LiveTracksRequest { bbox }.path_and_query())
}

/// Reads the server's answer to [`live_map_path`].
#[uniffi::export]
pub fn live_map(status: u16, body: Vec<u8>) -> Result<LiveMap, CoreError> {
    let response: LiveTracksResponse = answer(status, &body)?;
    Ok(LiveMap {
        gliders: response
            .tracks
            .iter()
            .map(|track| glider(track, response.time))
            .collect(),
        truncated: response.truncated,
    })
}

/// A track's glider as a map shows it, with its age at the server's `time`.
fn glider(track: &LiveTrack, time: Timestamp) -> LiveGlider {
    let name = one_line(&track.pilot.display_name, 40);
    let latest = track.latest;
    let age_s = (time - latest.time).num_seconds().max(0);
    LiveGlider {
        id: track.id.to_string(),
        handle: track.pilot.handle.to_string(),
        label: format!("{name} · {} m", latest.altitude_m),
        name,
        position: Coordinate {
            lat: latest.position.lat(),
            lon: latest.position.lon(),
        },
        altitude_m: latest.altitude_m,
        ground_speed_ms: latest.ground_speed_ms,
        climb_ms: latest.climb_ms,
        course_deg: latest.course_deg,
        landed: track.landed,
        age_s: age_s as f64,
        quiet: !track.landed && age_s >= QUIET_AFTER_S,
        page_path: api::live_page_path(&track.id),
    }
}

/// One flight being followed, as far as the server has handed it out.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct LiveFollowView {
    /// The glider now, as on the map.
    pub glider: LiveGlider,
    /// The track so far, oldest first, each point with its climb, for a
    /// line coloured like a flight's. At most a few thousand points: of a
    /// long flight every second or third position.
    pub track: Vec<TrailPointView>,
    /// When the track began, seconds since 1970-01-01 UTC.
    pub started_unix_s: i64,
    /// Seconds from its first position to its newest.
    pub duration_s: i64,
    /// The highest altitude so far, metres above sea level.
    pub highest_m: i32,
    /// Whether the server has more of the track waiting: ask again at
    /// once.
    pub more: bool,
}

/// The most points handed to a map for one track.
const POINTS_AT_MOST: usize = 4000;

/// Follows one live track: says what to ask the server for, and puts the
/// answers together. Ask once, then again every five to ten seconds, and
/// at once while the answer says there is more.
#[derive(Debug, uniffi::Object)]
pub struct LiveFollow {
    id: LiveTrackId,
    held: Mutex<Held>,
}

#[derive(Debug, Default)]
struct Held {
    /// The positions by their second. Late ones find their place.
    fixes: BTreeMap<i64, LiveFix>,
    cursor: Option<String>,
    over: bool,
}

#[uniffi::export]
impl LiveFollow {
    /// Begins to follow the track with this identifier.
    #[uniffi::constructor]
    pub fn new(id: String) -> Result<Arc<Self>, CoreError> {
        let id = id.parse().map_err(|_| CoreError::Server {
            message: "This is not the address of a live track.".to_owned(),
        })?;
        Ok(Arc::new(Self {
            id,
            held: Mutex::new(Held::default()),
        }))
    }

    /// The path and query to `GET` from the server now: the whole track
    /// the first time, afterwards what arrived since the last answer.
    pub fn path(&self) -> String {
        LiveTrackRequest {
            id: self.id.clone(),
            since: self.lock().cursor.clone(),
            after: None,
            limit: None,
        }
        .path_and_query()
    }

    /// Reads the server's answer to [`LiveFollow::path`] and returns the
    /// flight as far as it is known. A track that is gone, because its
    /// pilot deleted it or it ran out, is said in words, and
    /// [`LiveFollow::is_over`] is true from then on.
    pub fn receive(&self, status: u16, body: Vec<u8>) -> Result<LiveFollowView, CoreError> {
        let mut held = self.lock();
        if status == 404 {
            held.over = true;
            return Err(CoreError::Server {
                message: "This flight is no longer shown: its pilot stopped sharing it, or it is more than two days old."
                    .to_owned(),
            });
        }
        let response: LiveTrackResponse = answer(status, &body)?;
        if response.reset {
            // The server began again from the first position.
            held.fixes.clear();
        }
        for fix in &response.fixes {
            held.fixes.entry(fix.time.timestamp()).or_insert(*fix);
        }
        held.cursor = Some(response.cursor.clone());

        let step = held.fixes.len().div_ceil(POINTS_AT_MOST).max(1);
        let newest = held.fixes.len().saturating_sub(1);
        let track = held
            .fixes
            .values()
            .enumerate()
            // Every position of a short track, every few of a long one,
            // and always the newest: the line ends where the glider is.
            .filter(|(index, _)| index % step == 0 || *index == newest)
            .map(|(_, fix)| TrailPointView {
                lat: fix.position.lat(),
                lon: fix.position.lon(),
                climb_ms: fix.climb_ms.unwrap_or(0.0),
            })
            .collect();
        let first = held.fixes.keys().next().copied();
        let last = held.fixes.keys().next_back().copied();
        Ok(LiveFollowView {
            glider: glider(&response.track, response.time),
            track,
            started_unix_s: response.track.started_at.timestamp(),
            duration_s: first.zip(last).map_or(0, |(first, last)| last - first),
            highest_m: held
                .fixes
                .values()
                .map(|fix| fix.altitude_m)
                .max()
                .unwrap_or(response.track.latest.altitude_m),
            more: response.more,
        })
    }

    /// Whether the track is gone from the server: stop asking.
    pub fn is_over(&self) -> bool {
        self.lock().over
    }
}

impl LiveFollow {
    fn lock(&self) -> MutexGuard<'_, Held> {
        self.held
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A glider nothing was heard of for this many seconds has gone quiet.
const QUIET_AFTER_S: i64 = 120;

/// How old a glider's position is, in words for its label and its sheet:
/// "Now", "40 s ago", "Landed 12 min ago", "No position for 6 min". A
/// glider that has gone quiet is never called landed: no network and an
/// empty battery are far more common than anything else.
#[uniffi::export]
pub fn live_age_in_words(age_s: f64, landed: bool) -> String {
    let age_s = age_s.max(0.0);
    if landed {
        format!("Landed {}", ago(age_s))
    } else if age_s < 20.0 {
        "Now".to_owned()
    } else if age_s < QUIET_AFTER_S as f64 {
        ago(age_s)
    } else {
        format!("No position for {}", ago(age_s).trim_end_matches(" ago"))
    }
}

/// Text another pilot wrote, as one line of bounded length.
fn one_line(text: &str, at_most: usize) -> String {
    let mut line: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if line.chars().count() > at_most {
        line = line.chars().take(at_most - 1).collect::<String>() + "…";
    }
    line
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const START: f64 = 1_795_028_700.0;

    fn golden(name: &str) -> Vec<u8> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../server/tests/golden/");
        std::fs::read(format!("{path}{name}")).unwrap()
    }

    fn random() -> Vec<u8> {
        (0..26).collect()
    }

    fn share(audience: LiveAudience) -> LiveShare {
        LiveShare::begin("https://venti.example/", &random(), audience).unwrap()
    }

    /// The position `second` seconds into the flight.
    fn at(second: u32) -> LiveFix {
        fix(
            START + f64::from(second),
            GeoPoint::new(47.6642 + f64::from(second) * 0.0001, 11.7967).unwrap(),
            1620.4 - f64::from(second),
            Some(10.46),
            Some(-1.23),
            Some(319.6),
        )
        .unwrap()
    }

    fn body(request: &ServerRequest) -> Value {
        serde_json::from_slice(request.body.as_ref().unwrap()).unwrap()
    }

    fn seconds(request: &ServerRequest) -> Vec<i64> {
        body(request)["fixes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|fix| {
                DateTime::parse_from_rfc3339(fix["time"].as_str().unwrap())
                    .unwrap()
                    .timestamp()
                    - START as i64
            })
            .collect()
    }

    fn taken() -> Vec<u8> {
        // The identifier of `random()`: each byte's low five bits.
        let page = "https://venti.example/live/0123456789abcdefghjkmnpqrs";
        let mut answer: Value = serde_json::from_slice(&golden("live_positions.json")).unwrap();
        answer["page"] = json!(page);
        serde_json::to_vec(&answer).unwrap()
    }

    #[test]
    fn the_link_exists_before_anything_is_sent_and_nothing_is_sent_before_take_off() {
        let mut share = share(LiveAudience::Link);
        let view = share.view(START);
        assert_eq!(
            view.link,
            "https://venti.example/live/0123456789abcdefghjkmnpqrs"
        );
        assert_eq!(view.state, LiveShareState::Sharing);
        assert_eq!(view.words, "Begins at take-off");
        assert_eq!(share.request(START), None);
        assert_eq!(share.request(START + 600.0), None);
    }

    #[test]
    fn positions_go_out_at_once_and_then_every_quarter_minute() {
        let mut share = share(LiveAudience::Link);
        share.add(at(0));
        // The first as soon as there is one, so that the link shows a
        // glider.
        let first = share.request(START).unwrap();
        assert_eq!(first.method, "POST");
        assert_eq!(first.path, "/v1/live/positions");
        assert_eq!(first.content_type.as_deref(), Some("application/json"));
        assert!(first.needs_token);
        let sent = body(&first);
        assert_eq!(sent["id"], "0123456789abcdefghjkmnpqrs");
        assert_eq!(sent["visibility"], "link");
        assert_eq!(sent["landed"], false);
        // To the metre, the tenth and the degree.
        assert_eq!(
            sent["fixes"][0],
            json!({
                "time": "2026-11-18T19:05:00Z",
                "position": {"lat": 47.6642, "lon": 11.7967},
                "altitude_m": 1620,
                "ground_speed_ms": 10.5,
                "climb_ms": -1.2,
                "course_deg": 320.0
            })
        );
        assert_eq!(share.view(START).words, "Sending the first position");

        // While it is under way nothing else goes out.
        share.add(at(1));
        assert_eq!(share.request(START + 1.0), None);
        share.answer(201, &taken(), None, START + 1.0);
        let view = share.view(START + 9.0);
        assert_eq!((view.taken, view.waiting), (1, 1));
        assert_eq!(view.words, "Sent 8 s ago");
        assert_eq!(view.state, LiveShareState::Sharing);

        // Then what has gathered, a quarter of a minute after the last.
        for second in 2..=15 {
            share.add(at(second));
            assert_eq!(share.request(START + f64::from(second).min(14.0)), None);
        }
        let next = share.request(START + 15.0).unwrap();
        assert_eq!(seconds(&next), (1..=15).collect::<Vec<i64>>());
        share.answer(200, &taken(), None, START + 15.5);
        assert_eq!(share.view(START + 16.0).taken, 16);
        assert_eq!(share.view(START + 16.0).waiting, 0);
    }

    #[test]
    fn two_positions_of_one_second_are_one() {
        let mut share = share(LiveAudience::Link);
        share.add(fix(START + 0.2, at(0).position, 1620.0, None, None, None).unwrap());
        share.add(fix(START + 0.9, at(0).position, 1630.0, None, None, None).unwrap());
        // And one from before the newest is not taken either.
        share.add(at(5));
        share.add(at(3));
        assert_eq!(seconds(&share.request(START + 5.0).unwrap()), [0, 5]);
    }

    #[test]
    fn after_a_gap_the_newest_positions_go_first() {
        let mut share = share(LiveAudience::Public);
        for second in 0..200 {
            share.add(at(second));
        }
        // Nothing gets through for a while: each try waits twice as long
        // as the one before, a minute at most.
        let mut now = START + 200.0;
        assert!(share.request(now).is_some());
        share.no_answer(now);
        let view = share.view(now);
        assert_eq!(view.state, LiveShareState::Waiting);
        assert_eq!(view.words, "No network: 200 positions wait");
        let mut waits = Vec::new();
        for _ in 0..6 {
            let mut wait = 1.0;
            while share.request(now + wait).is_none() {
                wait += 1.0;
            }
            waits.push(wait);
            now += wait;
            share.no_answer(now);
        }
        assert_eq!(waits, [5.0, 10.0, 20.0, 40.0, 60.0, 60.0]);

        // Back in reach: the last minute first, then the minute before,
        // and so on back, sixty at a time.
        now += 100.0;
        let newest = share.request(now).unwrap();
        assert_eq!(seconds(&newest), (140..200).collect::<Vec<i64>>());
        share.answer(200, &taken(), None, now);
        assert_eq!(share.view(now).state, LiveShareState::Sharing);
        let before = share.request(now + 15.0).unwrap();
        assert_eq!(seconds(&before), (80..140).collect::<Vec<i64>>());
        share.answer(200, &taken(), None, now + 15.0);
        // A new position rides along with the next part of the gap.
        share.add(at(230));
        let mixed = seconds(&share.request(now + 30.0).unwrap());
        assert_eq!(mixed.len(), 60);
        assert_eq!((mixed[0], mixed[58], mixed[59]), (21, 79, 230));
    }

    #[test]
    fn the_server_is_told_of_the_landing_at_once_and_then_nothing_more() {
        let mut share = share(LiveAudience::Link);
        share.add(at(0));
        share.request(START).unwrap();
        share.answer(201, &taken(), None, START);

        // Landed three seconds after the last request: said now, not in
        // twelve seconds.
        share.add(at(3));
        share.set_landed(true);
        let landing = share.request(START + 3.0).unwrap();
        assert_eq!(body(&landing)["landed"], true);
        assert_eq!(seconds(&landing), [3]);
        // Lost on its way: said again.
        share.no_answer(START + 4.0);
        assert_eq!(share.view(START + 4.0).state, LiveShareState::Waiting);
        let again = share.request(START + 10.0).unwrap();
        assert_eq!(body(&again)["landed"], true);
        share.answer(200, &taken(), None, START + 10.0);
        let view = share.view(START + 60.0);
        assert_eq!(view.state, LiveShareState::Ended);
        assert_eq!(view.words, "Landed. The last position is sent.");
        assert_eq!(share.request(START + 600.0), None);

        // A landing with nothing left to send is said with the newest
        // position once more.
        let mut share = super::tests::share(LiveAudience::Link);
        share.add(at(0));
        share.request(START).unwrap();
        share.answer(201, &taken(), None, START);
        share.set_landed(true);
        let landing = share.request(START + 2.0).unwrap();
        assert_eq!(
            (body(&landing)["landed"].clone(), seconds(&landing)),
            (json!(true), vec![0])
        );
        share.answer(200, &taken(), None, START + 2.0);
        assert_eq!(share.request(START + 100.0), None);

        // A glider that flies on after a top landing is flying.
        share.set_landed(false);
        share.add(at(300));
        let on = share.request(START + 300.0).unwrap();
        assert_eq!(body(&on)["landed"], false);
        share.answer(200, &taken(), None, START + 300.0);
        assert_eq!(share.view(START + 301.0).state, LiveShareState::Sharing);
    }

    #[test]
    fn fewer_people_is_said_at_once_and_more_people_with_the_next_positions() {
        let mut share = share(LiveAudience::Public);
        share.add(at(0));
        let first = share.request(START).unwrap();
        assert_eq!(body(&first)["visibility"], "public");
        share.answer(201, &taken(), None, START);

        share.set_audience(LiveAudience::Link);
        let narrowed = share.request(START + 1.0).unwrap();
        assert_eq!(body(&narrowed)["visibility"], "link");
        assert_eq!(seconds(&narrowed), [0]);
        share.answer(200, &taken(), None, START + 1.0);
        assert_eq!(share.request(START + 2.0), None);
        assert_eq!(share.view(START + 2.0).audience, LiveAudience::Link);

        // Widening waits for the next positions.
        share.set_audience(LiveAudience::Public);
        assert_eq!(share.request(START + 3.0), None);
        share.add(at(20));
        assert_eq!(
            body(&share.request(START + 20.0).unwrap())["visibility"],
            "public"
        );
    }

    #[test]
    fn the_server_is_obeyed_when_it_asks_to_wait() {
        let mut share = share(LiveAudience::Link);
        share.add(at(0));
        share.request(START).unwrap();
        share.answer(429, b"", Some(40), START);
        assert_eq!(
            share.view(START).words,
            "The server asked to wait: 1 position waits"
        );
        assert_eq!(share.request(START + 39.0), None);
        assert!(share.request(START + 40.0).is_some());
        // An hour is not waited, whatever the server says: a flight is on.
        share.answer(503, b"", Some(3600), START + 40.0);
        assert!(share.request(START + 161.0).is_some());

        // Signed out: the positions wait, and a try a minute goes out, so
        // that sharing goes on once the pilot has signed in again.
        share.answer(401, b"", None, START + 161.0);
        assert_eq!(
            share.view(START + 161.0).words,
            "This device is signed out: 1 position waits"
        );
        assert_eq!(share.request(START + 200.0), None);
        assert!(share.request(START + 221.0).is_some());
    }

    #[test]
    fn what_the_server_refuses_for_good_is_not_sent_again() {
        let mut share = share(LiveAudience::Link);
        share.add(at(0));
        share.request(START).unwrap();
        let refused = json!({"error": {"code": "fixes_out_of_time", "message": "x"}}).to_string();
        share.answer(422, refused.as_bytes(), None, START);
        let view = share.view(START);
        assert_eq!((view.waiting, view.taken), (0, 0));
        assert_eq!(view.words, "This device's clock is wrong");
        // The flight goes on, and so does the sharing.
        share.add(at(20));
        assert_eq!(seconds(&share.request(START + 20.0).unwrap()), [20]);

        // An identifier that takes no more ends it, in words.
        for (status, code, words) in [
            (
                403,
                "live_track_full",
                "The track is full: it holds twelve hours. Share again for a new link.",
            ),
            (
                410,
                "live_track_gone",
                "The track was deleted or has run out.",
            ),
            (
                409,
                "live_track_taken",
                "The server takes no more positions for this track.",
            ),
            (404, "not_found", "This server has no live tracking."),
        ] {
            let mut share = super::tests::share(LiveAudience::Link);
            share.add(at(0));
            share.request(START).unwrap();
            let body = json!({"error": {"code": code, "message": "x"}}).to_string();
            share.answer(status, body.as_bytes(), None, START);
            let view = share.view(START);
            assert_eq!(view.state, LiveShareState::Stopped);
            assert_eq!(view.words, words);
            share.add(at(30));
            assert_eq!(share.request(START + 30.0), None);
        }
    }

    #[test]
    fn deleting_the_track_stops_the_sharing() {
        let mut share = share(LiveAudience::Public);
        share.add(at(0));
        share.request(START).unwrap();
        let deletion = share.deletion();
        assert_eq!(deletion.method, "DELETE");
        assert_eq!(deletion.path, "/v1/live/0123456789abcdefghjkmnpqrs");
        assert!(deletion.needs_token && deletion.body.is_none());
        // The answer to the request that was under way changes nothing.
        share.answer(410, b"", None, START + 1.0);
        share.add(at(5));
        assert_eq!(share.request(START + 60.0), None);
        let view = share.view(START + 60.0);
        assert_eq!(view.state, LiveShareState::Stopped);
        assert_eq!(view.words, "You deleted the track.");
    }

    #[test]
    fn a_link_is_made_only_from_random_numbers_and_a_server() {
        assert!(LiveShare::begin("https://venti.example", &[7; 26], LiveAudience::Link).is_err());
        assert!(
            LiveShare::begin("https://venti.example", &random()[..20], LiveAudience::Link).is_err()
        );
        assert!(LiveShare::begin("", &random(), LiveAudience::Link).is_err());
        assert!(LiveShare::begin("venti.example", &random(), LiveAudience::Link).is_err());
    }

    #[test]
    fn the_servers_address_of_the_page_is_taken_only_if_it_is_this_tracks() {
        let mut share = share(LiveAudience::Link);
        share.add(at(0));
        share.request(START).unwrap();
        // The golden answer is about another track.
        share.answer(201, &golden("live_positions.json"), None, START);
        assert_eq!(
            share.view(START).link,
            "https://venti.example/live/0123456789abcdefghjkmnpqrs"
        );
        // The server knows its public address better than the app does.
        share.add(at(20));
        share.request(START + 20.0).unwrap();
        let mut answer: Value = serde_json::from_slice(&taken()).unwrap();
        answer["page"] = json!("https://fly.venti.example/live/0123456789abcdefghjkmnpqrs");
        share.answer(200, answer.to_string().as_bytes(), None, START + 20.0);
        assert_eq!(
            share.view(START + 20.0).link,
            "https://fly.venti.example/live/0123456789abcdefghjkmnpqrs"
        );
    }

    #[test]
    fn the_request_is_what_the_server_documents() {
        // The server's own example, built from the same positions.
        let expected: Value =
            serde_json::from_slice(&golden("live_positions_request.json")).unwrap();
        let mut share = LiveShare::begin(
            "https://venti.example",
            &[4; 25].iter().copied().chain([36]).collect::<Vec<u8>>(),
            LiveAudience::Link,
        )
        .unwrap();
        let positions = [
            (0, 47.6642, 11.7967, 1620.0),
            (10, 47.6649, 11.7958, 1608.0),
            (20, 47.6656, 11.7949, 1596.0),
        ];
        let start = DateTime::parse_from_rfc3339("2026-11-18T19:04:40Z")
            .unwrap()
            .timestamp() as f64;
        for (index, (second, lat, lon, altitude)) in positions.into_iter().enumerate() {
            let moving = index > 0;
            share.add(
                fix(
                    start + f64::from(second),
                    GeoPoint::new(lat, lon).unwrap(),
                    altitude,
                    moving.then_some(10.5),
                    moving.then_some(-1.2),
                    moving.then_some(320.0),
                )
                .unwrap(),
            );
        }
        assert_eq!(body(&share.request(start + 20.0).unwrap()), expected);
    }

    #[test]
    fn who_is_in_the_air_is_read_from_the_servers_answer() {
        assert_eq!(
            live_map_path(47.55, 11.6, 47.85, 12.0).unwrap(),
            "/v1/live?bbox=11.6,47.55,12,47.85"
        );
        let map = live_map(200, golden("live.json")).unwrap();
        assert!(!map.truncated);
        assert_eq!(map.gliders.len(), 2);
        let flying = &map.gliders[0];
        assert_eq!(flying.handle, "demo");
        assert_eq!(flying.label, "Demo Pilot · 1596 m");
        assert_eq!(flying.page_path, "/live/11111111111111111111111111");
        assert!(!flying.landed);
        // Four seconds old by the server's own clock.
        assert_eq!(flying.age_s, 4.0);
        assert_eq!(flying.course_deg, Some(20.0));
        assert!(!flying.quiet);
        let landed = &map.gliders[1];
        assert!(landed.landed);
        assert_eq!(landed.age_s, 540.0);
        // Landed is not quiet: the pilot's app said so.
        assert!(!landed.quiet);
        // The same glider, had its app said nothing for those nine minutes.
        let mut answer: Value = serde_json::from_slice(&golden("live.json")).unwrap();
        answer["tracks"][1]["landed"] = json!(false);
        let map = live_map(200, answer.to_string().into_bytes()).unwrap();
        assert!(map.gliders[1].quiet);
        assert_eq!(landed.course_deg, None);

        // A refusal comes in words.
        assert!(live_map(503, Vec::new()).is_err());
    }

    #[test]
    fn a_followed_flight_is_put_together_from_the_servers_answers() {
        let follow = LiveFollow::new("11111111111111111111111111".to_owned()).unwrap();
        assert_eq!(follow.path(), "/v1/live/11111111111111111111111111");

        // The server's own example: the last three positions of a track.
        let view = follow.receive(200, golden("live_track.json")).unwrap();
        assert_eq!(view.glider.label, "Demo Pilot · 1596 m");
        assert_eq!(view.glider.age_s, 4.0);
        assert_eq!(view.track.len(), 3);
        assert_eq!(view.duration_s, 2);
        assert_eq!(view.highest_m, 1598);
        assert!(!view.more);
        assert!(!follow.is_over());
        // From now on only what arrived since.
        assert_eq!(
            follow.path(),
            "/v1/live/11111111111111111111111111?since=1201"
        );

        // Two positions arrive: one new, and one from a gap before the
        // three, which finds its place at the front of the line.
        let mut later: Value = serde_json::from_slice(&golden("live_track.json")).unwrap();
        let fix = |time: &str, lat: f64, altitude: i32| json!({"time": time, "position": {"lat": lat, "lon": 11.7431}, "altitude_m": altitude, "climb_ms": 2.0});
        later["fixes"] = json!([
            fix("2026-11-18T19:04:57Z", 47.6639, 1700),
            fix("2026-11-18T19:04:40Z", 47.6625, 1610),
        ]);
        later["cursor"] = json!("1203");
        let view = follow.receive(200, later.to_string().into_bytes()).unwrap();
        assert_eq!(view.track.len(), 5);
        assert_eq!(view.track[0].lat, 47.6625);
        assert_eq!(view.track[4].lat, 47.6639);
        assert_eq!(view.track[4].climb_ms, 2.0);
        assert_eq!((view.duration_s, view.highest_m), (17, 1700));
        assert_eq!(
            follow.path(),
            "/v1/live/11111111111111111111111111?since=1203"
        );

        // The same answer again changes nothing: a position is held once.
        let again = follow.receive(200, later.to_string().into_bytes()).unwrap();
        assert_eq!(again.track.len(), 5);

        // A server that lost its place begins again, and so does the app.
        later["reset"] = json!(true);
        later["fixes"] = json!([fix("2026-11-18T19:04:58Z", 47.664, 1702)]);
        let view = follow.receive(200, later.to_string().into_bytes()).unwrap();
        assert_eq!(view.track.len(), 1);

        // A busy server is a refusal in words and nothing more.
        assert!(follow.receive(503, Vec::new()).is_err());
        assert!(!follow.is_over());
        // A track that is gone ends the following.
        let gone = json!({"error": {"code": "live_track_not_found", "message": "x"}}).to_string();
        let error = follow.receive(404, gone.into_bytes()).unwrap_err();
        assert!(error.to_string().contains("no longer shown"), "{error}");
        assert!(follow.is_over());

        assert!(LiveFollow::new("not a track".to_owned()).is_err());
    }

    #[test]
    fn a_long_track_is_thinned_for_the_map_and_still_ends_at_the_glider() {
        let follow = LiveFollow::new("11111111111111111111111111".to_owned()).unwrap();
        let mut answer: Value = serde_json::from_slice(&golden("live_track.json")).unwrap();
        let start = DateTime::parse_from_rfc3339("2026-11-18T16:00:00Z").unwrap();
        let fixes: Vec<Value> = (0..9001)
            .map(|second| {
                json!({
                    "time": (start + chrono::Duration::seconds(second)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                    "position": {"lat": 47.0 + f64::from(second as i32) * 0.00001, "lon": 11.7},
                    "altitude_m": 1500
                })
            })
            .collect();
        answer["fixes"] = json!(fixes);
        let view = follow
            .receive(200, answer.to_string().into_bytes())
            .unwrap();
        // Nine thousand positions: every third, and the last.
        assert_eq!(view.track.len(), 3001);
        assert_eq!(view.duration_s, 9000);
        assert!((view.track.last().unwrap().lat - 47.09).abs() < 1e-9);
    }

    #[test]
    fn a_quiet_glider_is_not_called_landed() {
        assert_eq!(live_age_in_words(4.0, false), "Now");
        assert_eq!(live_age_in_words(45.0, false), "45 s ago");
        assert_eq!(live_age_in_words(400.0, false), "No position for 7 min");
        assert_eq!(live_age_in_words(7200.0, false), "No position for 2 h");
        assert_eq!(live_age_in_words(540.0, true), "Landed 9 min ago");
        assert_eq!(live_age_in_words(-3.0, false), "Now");
    }

    #[test]
    fn another_pilots_name_is_one_bounded_line() {
        assert_eq!(one_line("Anna\nMaier\t ", 40), "Anna Maier");
        let long = one_line(&"x".repeat(200), 40);
        assert_eq!(long.chars().count(), 40);
        assert!(long.ends_with('…'));
    }
}
