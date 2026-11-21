//! Aircraft that another network sees: the live positions of a source that
//! is a connection which stays open, kept apart from Venti's own live
//! tracks.
//!
//! ```text
//!  connect ──► login line (the regions' box) ──► a line at a time
//!     ▲                                             │ the adapter: a position,
//!     │ backoff, 5 s doubling to 5 min              │ hidden, or nothing
//!     │                                             ▼
//!  silence for a minute, an error,        the newest position of each
//!  the end of the stream                  device, in memory, at most
//!                                         `pass_on_hours` (24 h)
//!  the directory of devices, daily ──►    the owners' choices, applied
//!                                         when the API asks
//! ```
//!
//! **The network's rules are applied here, for every request.** A device
//! whose owner chose not to be tracked is never listed; one whose owner
//! chose not to be identified is listed without registration, competition
//! number or model, and so is a device the directory does not know. While
//! the server holds no directory younger than [`DIRECTORY_STALE`], it lists
//! nothing: it cannot know whom it may show. Nothing older than the
//! manifest's `pass_on_hours` is kept, and the API lists only the last
//! [`LIVE_OTHERS_LISTED_MINUTES`].
//!
//! **No address leaves the server.** An aircraft is named by a hash of its
//! address with a key drawn when the server starts: the same while the
//! server runs, so that a map can move its mark, and nothing that can be
//! looked up in the directory.
//!
//! **A position is where a person is.** Nothing here writes a position or
//! an address to the log.
//!
//! A request never waits for the network: the connection is a task of its
//! own, and the API reads what the task left in memory.

use std::collections::HashMap;
use std::collections::hash_map::{DefaultHasher, RandomState};
use std::hash::{BuildHasher, Hash, Hasher};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration as StdDuration;

use chrono::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;
use venti_api_types::v1::{
    AircraftKind, Attribution, LIVE_OTHERS_LISTED_MINUTES, LiveOthersLayer, MAX_LIVE_OTHERS,
    OtherAircraft,
};
use venti_domain::{BoundingBox, SourceId, Timestamp};
use venti_sources::{
    Aircraft, DeviceDirectory, Fetch, LiveLine, LivePosition, LiveSource, Manifest, Shown,
};

use crate::api::ServedSource;
use crate::clock::Clock;

/// For how long a directory of devices is trusted. It is fetched daily;
/// after three days without a new one the server lists nothing.
pub const DIRECTORY_STALE: Duration = Duration::days(3);

/// The longest a device's position is kept when the manifest sets no
/// limit of its own.
const KEPT_AT_MOST: Duration = Duration::hours(24);

/// The longest line read. An aircraft's line is some 150 characters.
const MAX_LINE: u64 = 1024;

/// How long the network may say nothing before the connection is dropped.
/// It sends a comment line about every 20 seconds.
const SILENCE: StdDuration = StdDuration::from_secs(60);

/// How often a keepalive is sent.
const KEEPALIVE: StdDuration = StdDuration::from_secs(240);

/// The first wait before connecting again, and the longest.
const FIRST_BACKOFF: StdDuration = StdDuration::from_secs(5);
const LONGEST_BACKOFF: StdDuration = StdDuration::from_secs(300);

/// How often the directory is fetched, and how soon again after a failure.
const DIRECTORY_EVERY: StdDuration = StdDuration::from_secs(24 * 3600);
const DIRECTORY_RETRY: StdDuration = StdDuration::from_secs(3600);

/// Ten seconds to connect.
const CONNECT: StdDuration = StdDuration::from_secs(10);

/// What the server holds of one network.
pub struct Network {
    manifest: Manifest,
    attribution: Attribution,
    clock: Arc<dyn Clock>,
    kept: Duration,
    /// Drawn when the server starts; what an aircraft's name is hashed
    /// with.
    salt: u64,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    positions: HashMap<String, LivePosition>,
    directory: Option<(DeviceDirectory, Timestamp)>,
    connected: bool,
}

impl std::fmt::Debug for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Network")
            .field("source", &self.manifest.id)
            .finish_non_exhaustive()
    }
}

impl Network {
    /// An empty store for the source `manifest` describes, with the clock
    /// its positions are aged by.
    pub fn new(manifest: &Manifest, clock: Arc<dyn Clock>) -> Self {
        let kept = manifest
            .pass_on_hours
            .map_or(KEPT_AT_MOST, |hours| Duration::hours(i64::from(hours)))
            .min(KEPT_AT_MOST);
        Self {
            attribution: ServedSource::from_manifest(manifest).attribution,
            manifest: manifest.clone(),
            clock,
            kept,
            salt: RandomState::new().hash_one(0_u64),
            state: Mutex::new(State::default()),
        }
    }

    /// The same, naming aircraft with `salt` instead of a key drawn at
    /// random: for the development mode and the tests, whose answers are
    /// held against golden files.
    #[must_use]
    pub fn with_salt(mut self, salt: u64) -> Self {
        self.salt = salt;
        self
    }

    /// The source.
    pub fn source(&self) -> &SourceId {
        &self.manifest.id
    }

    /// The credit that goes out with the positions.
    pub fn attribution(&self) -> &Attribution {
        &self.attribution
    }

    /// The clock the positions are aged by.
    pub fn now(&self) -> Timestamp {
        self.clock.now()
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Takes one line the adapter read: keeps a position if it is the
    /// newest of its device and not older than may be kept.
    pub fn take(&self, line: LiveLine) {
        let LiveLine::Position(position) = line else {
            return;
        };
        if position.time < self.now() - self.kept {
            return;
        }
        let mut state = self.state();
        let newer = state
            .positions
            .get(&position.device)
            .is_none_or(|held| held.time <= position.time);
        if newer {
            state.positions.insert(position.device.clone(), position);
        }
    }

    /// Takes a directory of devices fetched at `at`.
    pub fn take_directory(&self, directory: DeviceDirectory, at: Timestamp) {
        self.state().directory = Some((directory, at));
    }

    /// Notes whether the server is connected to the network now.
    pub fn set_connected(&self, connected: bool) {
        self.state().connected = connected;
    }

    /// Forgets every position older than may be kept.
    pub fn forget_old(&self) {
        let oldest = self.now() - self.kept;
        self.state()
            .positions
            .retain(|_, position| position.time >= oldest);
    }

    /// How many devices' positions are held.
    pub fn held(&self) -> usize {
        self.state().positions.len()
    }

    /// The aircraft heard in `area` in the last
    /// [`LIVE_OTHERS_LISTED_MINUTES`] before `now`, as the owners let them
    /// be shown.
    pub fn layer(&self, area: &BoundingBox, now: Timestamp) -> LiveOthersLayer {
        let since = now - Duration::minutes(LIVE_OTHERS_LISTED_MINUTES);
        let state = self.state();
        let directory = state
            .directory
            .as_ref()
            .filter(|(_, at)| now - *at <= DIRECTORY_STALE)
            .map(|(directory, _)| directory);
        let mut aircraft: Vec<OtherAircraft> = match directory {
            // Without the owners' choices nobody is shown.
            None => Vec::new(),
            Some(directory) => state
                .positions
                .values()
                .filter(|p| p.time >= since && p.time <= now + Duration::minutes(1))
                .filter(|p| area.contains(p.position))
                .filter_map(|p| self.shown(p, directory.shown(&p.device), now))
                .collect(),
        };
        let connected = state.connected;
        drop(state);
        aircraft.sort_by(|a, b| {
            (!a.aircraft.is_free_flight(), b.time, &a.id).cmp(&(
                !b.aircraft.is_free_flight(),
                a.time,
                &b.id,
            ))
        });
        let truncated = aircraft.len() > MAX_LIVE_OTHERS;
        aircraft.truncate(MAX_LIVE_OTHERS);
        LiveOthersLayer {
            source: self.manifest.id.clone(),
            connected,
            aircraft,
            truncated,
        }
    }

    fn shown(
        &self,
        position: &LivePosition,
        shown: Shown,
        now: Timestamp,
    ) -> Option<OtherAircraft> {
        let choice = match shown {
            Shown::Not => return None,
            Shown::Anonymous => None,
            Shown::Identified(choice) => Some(choice),
        };
        let age_s = u32::try_from((now - position.time).num_seconds().max(0)).unwrap_or(u32::MAX);
        let mut hasher = DefaultHasher::new();
        (self.salt, &position.device).hash(&mut hasher);
        let id = format!("{:016x}", hasher.finish());
        Some(OtherAircraft {
            id,
            aircraft: kind(position.aircraft),
            time: position.time,
            age_s,
            position: position.position,
            altitude_m: position.altitude_m,
            ground_speed_ms: position.ground_speed_ms,
            climb_ms: position.climb_ms,
            course_deg: position.course_deg,
            registration: choice.as_ref().and_then(|c| c.registration.clone()),
            competition_id: choice.as_ref().and_then(|c| c.competition_id.clone()),
            model: choice.and_then(|c| c.model),
        })
    }
}

fn kind(aircraft: Aircraft) -> AircraftKind {
    match aircraft {
        Aircraft::Paraglider => AircraftKind::Paraglider,
        Aircraft::HangGlider => AircraftKind::HangGlider,
        Aircraft::Glider => AircraftKind::Glider,
        Aircraft::TowPlane => AircraftKind::TowPlane,
        Aircraft::Helicopter => AircraftKind::Helicopter,
        Aircraft::Parachute => AircraftKind::Parachute,
        Aircraft::DropPlane => AircraftKind::DropPlane,
        Aircraft::PoweredAircraft => AircraftKind::PoweredAircraft,
        Aircraft::JetAircraft => AircraftKind::JetAircraft,
        Aircraft::Balloon => AircraftKind::Balloon,
        Aircraft::Airship => AircraftKind::Airship,
        Aircraft::Drone => AircraftKind::Drone,
        Aircraft::Obstacle => AircraftKind::Obstacle,
        Aircraft::Unknown => AircraftKind::Unknown,
    }
}

/// Feeds `network` with every line of `text` as if each came in at
/// `received`: for the development mode and the tests, which connect
/// nowhere.
pub fn feed(network: &Network, source: &dyn LiveSource, text: &str, received: Timestamp) {
    for line in text.lines() {
        network.take(source.parse_line(line, received));
    }
}

/// Listens to `source` for `area` until `shutdown`: the connection, made
/// again after every end with a growing wait, and the directory of devices,
/// fetched with `fetch` once a day. Neither ever blocks a request.
pub async fn run<F>(
    source: Arc<dyn LiveSource>,
    network: Arc<Network>,
    area: BoundingBox,
    fetch: F,
    shutdown: CancellationToken,
) where
    F: Fetch + Send + Sync + 'static,
{
    let directory = tokio::spawn(fetch_directory(
        source.clone(),
        network.clone(),
        fetch,
        shutdown.clone(),
    ));
    let mut backoff = FIRST_BACKOFF;
    loop {
        let began = tokio::time::Instant::now();
        let ended = tokio::select! {
            () = shutdown.cancelled() => break,
            ended = listen(source.as_ref(), &network, &area) => ended,
        };
        network.set_connected(false);
        network.forget_old();
        // A connection that held for a while starts the waits again.
        if began.elapsed() > StdDuration::from_secs(120) {
            backoff = FIRST_BACKOFF;
        }
        tracing::warn!(
            source = %network.source(),
            "the connection to the live network ended ({ended}); connecting again in {} s",
            backoff.as_secs()
        );
        tokio::select! {
            () = shutdown.cancelled() => break,
            () = tokio::time::sleep(backoff) => {}
        }
        backoff = (backoff * 2).min(LONGEST_BACKOFF);
    }
    network.set_connected(false);
    let _ = directory.await;
}

/// One connection, until it ends: why it ended.
async fn listen(source: &dyn LiveSource, network: &Network, area: &BoundingBox) -> String {
    let (host, port) = source.server();
    let stream = match tokio::time::timeout(CONNECT, TcpStream::connect((host, port))).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => return format!("cannot connect: {error}"),
        Err(_) => return "no connection within ten seconds".to_owned(),
    };
    let (read, mut write) = stream.into_split();
    if let Err(error) = write.write_all(source.login(area).as_bytes()).await {
        return format!("cannot log in: {error}");
    }
    network.set_connected(true);
    tracing::info!(source = %network.source(), "connected to the live network");
    let mut reader = BufReader::new(read);
    let mut line = Vec::with_capacity(256);
    let mut keepalive = tokio::time::interval(KEEPALIVE);
    keepalive.tick().await;
    let mut lines: u64 = 0;
    loop {
        // A keepalive that is due interrupts a read; what was read of the
        // line so far stays in `line`, and the read goes on after it.
        if line.len() as u64 >= MAX_LINE {
            line.clear();
        }
        let room = MAX_LINE - line.len() as u64;
        let mut limited = (&mut reader).take(room);
        let read = limited.read_until(b'\n', &mut line);
        tokio::select! {
            _ = keepalive.tick() => {
                if let Err(error) = write.write_all(source.keepalive().as_bytes()).await {
                    return format!("cannot send a keepalive: {error}");
                }
                network.forget_old();
                continue;
            }
            read = tokio::time::timeout(SILENCE, read) => match read {
                Err(_) => return "nothing for a minute".to_owned(),
                Ok(Err(error)) => return format!("cannot read: {error}"),
                Ok(Ok(0)) => return format!("the network closed it after {lines} lines"),
                Ok(Ok(_)) => {}
            },
        }
        lines += 1;
        // A line longer than any of the network's is cut; what is left of
        // it is read as a line of its own, and is nothing.
        let text = String::from_utf8_lossy(&line);
        network.take(source.parse_line(&text, network.now()));
        line.clear();
    }
}

/// Fetches the directory of devices now and then once a day, sooner again
/// after a failure.
async fn fetch_directory<F: Fetch>(
    source: Arc<dyn LiveSource>,
    network: Arc<Network>,
    fetch: F,
    shutdown: CancellationToken,
) {
    let request = source.directory();
    let max = source.manifest().max_response_bytes();
    loop {
        let wait = match fetch.get_within(&request, max).await {
            Ok(fetched) => match source.parse_directory(&fetched.body) {
                Ok(directory) => {
                    tracing::info!(
                        source = %network.source(),
                        "directory of devices: {} devices",
                        directory.devices.len()
                    );
                    network.take_directory(directory, network.now());
                    DIRECTORY_EVERY
                }
                Err(error) => {
                    tracing::warn!(source = %network.source(), "directory of devices: {error}");
                    DIRECTORY_RETRY
                }
            },
            Err(error) => {
                tracing::warn!(source = %network.source(), "directory of devices: {error}");
                DIRECTORY_RETRY
            }
        };
        tokio::select! {
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(wait) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use venti_sources::DeviceChoice;

    use super::*;
    use crate::adapters;
    use crate::clock::FixedClock;

    /// The made-up lines and directory, by the names of what this file is
    /// about.
    mod fixtures {
        use venti_domain::{BoundingBox, Timestamp};

        use crate::adapters;

        pub fn feed_at() -> Timestamp {
            adapters::live_feed_at()
        }
        pub fn area() -> BoundingBox {
            BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap()
        }
        pub fn made_up() -> &'static str {
            adapters::made_up_live_lines()
        }
        pub fn directory() -> &'static [u8] {
            adapters::made_up_live_directory()
        }
    }

    fn network(now: Timestamp) -> (Network, Arc<dyn LiveSource>) {
        let source = adapters::live_sources().remove(0);
        let network = Network::new(source.manifest(), Arc::new(FixedClock(now)));
        (network, source)
    }

    #[test]
    fn nobody_is_listed_before_the_owners_choices_are_known() {
        let now = fixtures::feed_at();
        let (network, ogn) = network(now);
        feed(&network, ogn.as_ref(), fixtures::made_up(), now);
        assert!(network.layer(&fixtures::area(), now).aircraft.is_empty());

        let directory = ogn.parse_directory(fixtures::directory()).unwrap();
        network.take_directory(directory.clone(), now);
        assert!(!network.layer(&fixtures::area(), now).aircraft.is_empty());

        // A directory older than three days is no longer trusted.
        network.take_directory(directory, now - Duration::days(4));
        assert!(network.layer(&fixtures::area(), now).aircraft.is_empty());
    }

    #[test]
    fn an_opt_out_is_honoured_and_nothing_old_is_kept() {
        let now = fixtures::feed_at();
        let (network, ogn) = network(now);
        feed(&network, ogn.as_ref(), fixtures::made_up(), now);
        let mut directory = ogn.parse_directory(fixtures::directory()).unwrap();
        // The made-up paraglider E00002, unknown to the directory, opts out.
        directory.devices.insert(
            "E00002".to_owned(),
            DeviceChoice {
                tracked: false,
                ..DeviceChoice::default()
            },
        );
        network.take_directory(directory, now);
        let layer = network.layer(&fixtures::area(), now);
        // E00001, E00003, E00006, then E00008, E00007, E0000B and E0000C:
        // not E00002 and E00004 (opted out), E00005 and E0000A (the line's
        // own flags), E00009 (eleven minutes old).
        assert_eq!(layer.aircraft.len(), 7);
        assert!(
            layer.aircraft[..3]
                .iter()
                .all(|a| a.aircraft.is_free_flight())
        );
        assert!(
            layer.aircraft[3..]
                .iter()
                .all(|a| !a.aircraft.is_free_flight())
        );
        let glider = layer
            .aircraft
            .iter()
            .find(|a| a.aircraft == AircraftKind::Glider)
            .unwrap();
        assert_eq!(glider.registration.as_deref(), Some("D-0000"));
        // No address is in an answer.
        let json = serde_json::to_string(&layer).unwrap();
        assert!(!json.contains("E0000"), "{json}");

        // A position older than a day is not kept.
        let day_old = LivePosition {
            time: now - Duration::hours(25),
            ..layer_position(&ogn, now)
        };
        network.take(LiveLine::Position(day_old));
        let later = Network::new(
            ogn.manifest(),
            Arc::new(FixedClock(now + Duration::hours(25))),
        );
        feed(&later, ogn.as_ref(), fixtures::made_up(), now);
        assert_eq!(later.held(), 0);
    }

    fn layer_position(ogn: &Arc<dyn LiveSource>, now: Timestamp) -> LivePosition {
        let line = fixtures::made_up().lines().nth(2).unwrap();
        match ogn.parse_line(line, now) {
            LiveLine::Position(position) => position,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_newest_position_of_a_device_is_kept() {
        let now = fixtures::feed_at();
        let (network, ogn) = network(now);
        let newer = layer_position(&ogn, now);
        let older = LivePosition {
            time: newer.time - Duration::seconds(30),
            ..newer.clone()
        };
        network.take(LiveLine::Position(newer.clone()));
        network.take(LiveLine::Position(older));
        assert_eq!(network.held(), 1);
        assert_eq!(network.state().positions["E00001"].time, newer.time);
    }
}
