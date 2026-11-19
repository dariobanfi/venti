//! Pilots' flights: the file as it was uploaded and what was worked out
//! from it.

use venti_domain::{GeoPoint, Timestamp};

/// Who may see a flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Visibility {
    /// Only its pilot.
    Private,
    /// Anyone who has its link.
    Unlisted,
    /// Anyone; it is listed on its pilot's page.
    Public,
}

impl Visibility {
    /// The name the database knows the setting by.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Unlisted => "unlisted",
            Self::Public => "public",
        }
    }

    /// The setting with this name.
    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Private, Self::Unlisted, Self::Public]
            .into_iter()
            .find(|visibility| visibility.as_str() == name)
    }
}

/// What a flight is scored as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScoreKind {
    /// The longest distance through up to three turnpoints.
    FreeFlight,
    /// A triangle that is not an FAI triangle.
    FreeTriangle,
    /// A triangle whose shortest leg is at least 28% of its perimeter.
    FaiTriangle,
}

impl ScoreKind {
    /// The name the database knows the kind by.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FreeFlight => "free_flight",
            Self::FreeTriangle => "free_triangle",
            Self::FaiTriangle => "fai_triangle",
        }
    }

    /// The kind with this name.
    pub fn from_name(name: &str) -> Option<Self> {
        [Self::FreeFlight, Self::FreeTriangle, Self::FaiTriangle]
            .into_iter()
            .find(|kind| kind.as_str() == name)
    }
}

/// A moment at one end of a flight.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Place {
    /// When.
    pub time: Timestamp,
    /// Where.
    pub position: GeoPoint,
    /// Satellite altitude, metres above mean sea level.
    pub altitude_m: i32,
}

/// How a flight scores as a cross-country flight.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Score {
    /// What it is scored as.
    pub kind: ScoreKind,
    /// For a triangle, whether it is closed.
    pub closed: bool,
    /// The scoring distance, metres.
    pub distance_m: f64,
    /// The points.
    pub points: f64,
}

/// Everything the server works out from a flight file.
///
/// None of it is the truth about the flight: the file is. When the analysis
/// improves, the version goes up and these values are worked out again.
#[derive(Debug, Clone, PartialEq)]
pub struct Derived {
    /// The version of the analysis that produced the values.
    pub version: u32,
    /// When the flown part of the recording begins.
    pub start: Timestamp,
    /// The length of the flown part, seconds.
    pub airtime_s: i64,
    /// Where and when the glider took off, if the recording shows it flying.
    pub takeoff: Option<Place>,
    /// Where and when it landed, if the recording shows that.
    pub landing: Option<Place>,
    /// Distance along the track, metres.
    pub track_length_m: f64,
    /// Highest satellite altitude, metres.
    pub max_altitude_m: i32,
    /// Best climb averaged over half a minute, metres per second.
    pub max_climb_ms: f64,
    /// How the flight scores, if it is long enough to score.
    pub score: Option<Score>,
    /// The glider, as the file names it.
    pub glider: Option<String>,
    /// The shape of the track for a thumbnail: latitude and longitude of a
    /// few dozen points.
    pub outline: Vec<[f64; 2]>,
    /// The altitude over time for a small chart: seconds since `start` and
    /// metres, for a hundred or two points.
    pub profile: Vec<[i32; 2]>,
}

/// The most one account may keep.
///
/// Sizes are those of the files as they were uploaded, before the
/// compression the caller applies: a number a pilot can check against the
/// files on their own device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Room {
    /// Flights.
    pub flights: u64,
    /// Bytes of flight files, all flights together.
    pub file_bytes: u64,
}

impl Room {
    /// No bound: for the flights a server stores of its own accord, and for
    /// tests that are not about room.
    pub const UNLIMITED: Self = Self {
        flights: u64::MAX,
        file_bytes: u64::MAX,
    };

    /// Whether an account that keeps `kept` has room for one more flight
    /// whose file is `file_bytes` long.
    pub fn has_room_for(&self, kept: &Kept, file_bytes: u32) -> bool {
        kept.flights < self.flights
            && kept.file_bytes.saturating_add(u64::from(file_bytes)) <= self.file_bytes
    }
}

/// What an account keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kept {
    /// How many flights.
    pub flights: u64,
    /// The size of their files together, before compression.
    pub file_bytes: u64,
}

/// What became of a flight handed to [`Store::save_flight`](crate::Store::save_flight).
#[derive(Debug, Clone, PartialEq)]
pub enum Saved {
    /// It was stored.
    Created(Flight),
    /// The account had it already; this is the flight that was there.
    AlreadyThere(Flight),
    /// The account has no room for it. Nothing was stored; this is what
    /// the account keeps.
    NoRoom(Kept),
}

impl Saved {
    /// The stored flight and whether this call created it; `None` when
    /// there was no room.
    pub fn stored(self) -> Option<(Flight, bool)> {
        match self {
            Self::Created(flight) => Some((flight, true)),
            Self::AlreadyThere(flight) => Some((flight, false)),
            Self::NoRoom(_) => None,
        }
    }
}

/// A flight to store.
#[derive(Debug, Clone, PartialEq)]
pub struct NewFlight {
    /// The address the flight gets. One account and identity always get
    /// the same address.
    pub id: String,
    /// The account it belongs to.
    pub account_id: String,
    /// The flight's identity: the hash of its fixes.
    pub identity: String,
    /// When the server received it.
    pub uploaded_at: Timestamp,
    /// The flight file, compressed by the caller. The store keeps these
    /// bytes as they are and hands them back as they are.
    pub file: Vec<u8>,
    /// The size of the file before compression, in bytes.
    pub file_bytes: u32,
    /// What was worked out from the file.
    pub derived: Derived,
}

/// A stored flight, without its file.
#[derive(Debug, Clone, PartialEq)]
pub struct Flight {
    /// The flight's address.
    pub id: String,
    /// The account it belongs to.
    pub account_id: String,
    /// That account's handle.
    pub pilot_handle: String,
    /// That account's display name.
    pub pilot_name: String,
    /// The flight's identity: the hash of its fixes.
    pub identity: String,
    /// Who may see it.
    pub visibility: Visibility,
    /// When the server first received it.
    pub uploaded_at: Timestamp,
    /// The size of the file before compression, in bytes.
    pub file_bytes: u32,
    /// What was worked out from the file.
    pub derived: Derived,
    /// The number of the change that last touched the flight; see
    /// [`FlightChanges`].
    pub change: i64,
}

/// A flight that was deleted, kept so that the pilot's other devices can
/// be told. Nothing of the flight itself is left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedFlight {
    /// The address the flight had.
    pub id: String,
    /// The identity the flight had.
    pub identity: String,
    /// The number of the change that deleted it.
    pub change: i64,
}

/// What changed among an account's flights after some change number.
///
/// Every upload, deletion or change to a flight gets a number that is
/// larger than every number the account has used before. Numbers have
/// gaps. A device that remembers the largest one it has seen can ask for
/// what came after it.
#[derive(Debug, Clone, PartialEq)]
pub struct FlightChanges {
    /// Flights that are new or changed, by change number.
    pub flights: Vec<Flight>,
    /// Flights that were deleted, by change number.
    pub deleted: Vec<DeletedFlight>,
    /// Whether changes beyond the limit of the request are waiting.
    pub more: bool,
    /// The largest change number the account has used, zero if it has
    /// used none.
    pub latest: i64,
}

impl FlightChanges {
    /// The largest change number among the changes listed here.
    pub fn last(&self) -> Option<i64> {
        let flights = self.flights.iter().map(|flight| flight.change);
        let deleted = self.deleted.iter().map(|deleted| deleted.change);
        flights.chain(deleted).max()
    }
}

/// A flight whose derived values come from an older analysis, with the
/// file to work them out from again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutdatedFlight {
    /// The flight's address.
    pub id: String,
    /// The version of the analysis its derived values come from.
    pub version: u32,
    /// Its file, as it was handed to the store.
    pub file: Vec<u8>,
}

impl OutdatedFlight {
    /// The flight's place among outdated flights: oldest analysis first,
    /// then by address.
    pub fn key(&self) -> (u32, String) {
        (self.version, self.id.clone())
    }
}

/// Which of an account's flights a list holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listing {
    /// All of them: what the account's owner sees.
    All,
    /// The public ones: what everyone sees.
    Public,
}

/// The place of a flight in a list sorted newest first: its start and, for
/// flights that start in the same second, its address.
pub type FlightKey = (Timestamp, String);

impl Flight {
    /// The flight's place in a list sorted newest first.
    pub fn key(&self) -> FlightKey {
        (self.derived.start, self.id.clone())
    }
}

/// The next change number of an account whose latest is `latest`, for a
/// change made at `now`.
///
/// It is the time in microseconds, or one more than the latest number if
/// the clock has not moved past that. Numbers that follow the clock keep
/// their meaning when a database is restored from a backup: changes made
/// after the restore still come after everything a device saw before it.
pub(crate) fn next_change(latest: i64, now: Timestamp) -> i64 {
    now.timestamp_micros().max(latest.saturating_add(1))
}

/// Keeps the `limit` changes with the smallest numbers of two lists that
/// are each sorted by change number, and says whether any were dropped.
///
/// Both stores read the two lists separately and cut them here, so they
/// page through changes in exactly the same way.
pub(crate) fn cut(
    flights: &mut Vec<Flight>,
    deleted: &mut Vec<DeletedFlight>,
    limit: usize,
) -> bool {
    let mut numbers: Vec<i64> = flights
        .iter()
        .map(|flight| flight.change)
        .chain(deleted.iter().map(|deleted| deleted.change))
        .collect();
    if numbers.len() <= limit {
        return false;
    }
    numbers.sort_unstable();
    // Change numbers of one account are unique, so this keeps exactly
    // `limit` changes.
    match limit.checked_sub(1).map(|last| numbers[last]) {
        Some(last_kept) => {
            flights.retain(|flight| flight.change <= last_kept);
            deleted.retain(|deleted| deleted.change <= last_kept);
        }
        None => {
            flights.clear();
            deleted.clear();
        }
    }
    true
}
