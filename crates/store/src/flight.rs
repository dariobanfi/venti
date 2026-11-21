//! Pilots' flights: the file as it was uploaded and what was worked out
//! from it.

use venti_domain::{BoundingBox, GeoPoint, Timestamp};

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
    /// Since when the flight is taken down, if whoever runs the server has
    /// done that. A flight that is taken down is private, and stays so
    /// whatever its pilot asks for, until it is released.
    pub taken_down_at: Option<Timestamp>,
    /// Since when the flight is public, without a break; `None` while it
    /// is not. A flight that is withdrawn and made public again has the
    /// moment of the second time.
    pub published_at: Option<Timestamp>,
    /// The number of the last change to what the public sees of the
    /// flight, in the one count all accounts share; `None` while it is not
    /// public. See [`PublicFlightChanges`].
    pub public_change: Option<i64>,
}

/// What became of a call that sets who may see a flight.
#[derive(Debug, Clone, PartialEq)]
pub enum VisibilityChange {
    /// The flight is as was asked for, whether or not the call changed
    /// it. This is the flight as it now is.
    Set(Box<Flight>),
    /// The account has no such flight.
    NoSuchFlight,
    /// The flight is taken down and stays private. Nothing was changed.
    TakenDown,
    /// The account may not publish, and the flight was to be made public.
    /// Nothing was changed.
    PublishingSuspended,
}

impl VisibilityChange {
    /// The flight as it now is, if it is as was asked for.
    pub fn flight(self) -> Option<Flight> {
        match self {
            Self::Set(flight) => Some(*flight),
            Self::NoSuchFlight | Self::TakenDown | Self::PublishingSuspended => None,
        }
    }
}

/// Whether a flight of an account may be given a visibility.
///
/// Both stores ask this, with the flight and its account read under one
/// lock, so that neither rule can be raced past.
pub(crate) fn may_be_set(
    taken_down: bool,
    publishing_suspended: bool,
    visibility: Visibility,
) -> Result<(), VisibilityChange> {
    if taken_down && visibility != Visibility::Private {
        return Err(VisibilityChange::TakenDown);
    }
    if publishing_suspended && visibility == Visibility::Public {
        return Err(VisibilityChange::PublishingSuspended);
    }
    Ok(())
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

/// Which of everyone's public flights a list holds.
///
/// Whatever is set here, a flight that is not public is never among them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PublicFlights {
    /// Only flights of this account.
    pub pilot: Option<String>,
    /// Only flights that took off inside this box, edges included. A
    /// flight whose recording never shows the glider flying took off
    /// nowhere and is in no box.
    pub area: Option<BoundingBox>,
    /// Only flights that started at this moment or later.
    pub from: Option<Timestamp>,
    /// Only flights that started before this moment.
    pub before: Option<Timestamp>,
    /// Only flights that took off within some metres of a place, that
    /// far included: the flights that began at a site, as
    /// [`crate::Store::flights_from`] counts them.
    pub near: Option<Near>,
}

/// A place and how far from it a flight may have taken off.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Near {
    /// The place.
    pub place: GeoPoint,
    /// Metres from it, that far included.
    pub within_m: f64,
}

impl PublicFlights {
    /// Whether a flight is one of these, as far as its values go. Whether
    /// it is public is the caller's to ask.
    pub(crate) fn holds(&self, flight: &Flight) -> bool {
        let start = flight.derived.start;
        let in_area = self.area.is_none_or(|area| {
            flight
                .derived
                .takeoff
                .is_some_and(|takeoff| area.contains(takeoff.position))
        });
        let near = self.near.is_none_or(|near| {
            flight
                .derived
                .takeoff
                .is_some_and(|takeoff| takeoff.position.distance_m(near.place) <= near.within_m)
        });
        in_area
            && near
            && self
                .pilot
                .as_ref()
                .is_none_or(|pilot| *pilot == flight.account_id)
            && self.from.is_none_or(|from| start >= from)
            && self.before.is_none_or(|before| start < before)
    }
}

/// The order of a list of public flights, with the place in that order
/// after which the list goes on: the key of the last flight of the page
/// before. `None` starts at the top.
#[derive(Debug, Clone, PartialEq)]
pub enum PublicOrder {
    /// Newest first: by start, and by address among flights that start in
    /// the same second. The key is [`Flight::key`].
    Newest(Option<FlightKey>),
    /// Most points first, and by address among flights with the same
    /// points. A flight too short to score is not in this list. The key is
    /// [`Flight::score_key`].
    Best(Option<ScoreKey>),
}

/// The place of a flight in a list sorted by points: its points and, for
/// flights with the same points, its address.
pub type ScoreKey = (f64, String);

impl Flight {
    /// The flight's place in a list sorted by points; `None` for a flight
    /// too short to score.
    pub fn score_key(&self) -> Option<ScoreKey> {
        self.derived
            .score
            .map(|score| (score.points, self.id.clone()))
    }
}

/// How two places in a list sorted by points compare: by points, and by
/// address where the points are the same. This is how the database
/// compares a pair of a number and a text; the in-memory store uses it so
/// that the two page alike. Points are finite numbers.
pub(crate) fn score_order(a: &ScoreKey, b: &ScoreKey) -> std::cmp::Ordering {
    a.0.partial_cmp(&b.0)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| a.1.cmp(&b.1))
}

/// A pilot's place among the pilots of a list of public flights: the sum
/// of the points of their best flights in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Ranked {
    /// The pilot's handle, as the account is now.
    pub pilot_handle: String,
    /// The pilot's display name, as the account is now.
    pub pilot_name: String,
    /// The sum of the points of the flights that count.
    pub points: f64,
    /// The flights that count, the best first.
    pub flights: Vec<Flight>,
    /// How many of the pilot's flights in the list scored at all, those
    /// that count among them.
    pub scored: u32,
}

impl Ranked {
    /// A pilot's place from the flights that count for it, the best
    /// first; `None` without a flight.
    ///
    /// The points are summed here, in that order, whichever store ranks:
    /// two stores that each summed in an order of their own could differ
    /// in the last digit.
    pub(crate) fn of(flights: Vec<Flight>, scored: u32) -> Option<Self> {
        let first = flights.first()?;
        Some(Self {
            pilot_handle: first.pilot_handle.clone(),
            pilot_name: first.pilot_name.clone(),
            points: flights
                .iter()
                .filter_map(|flight| flight.derived.score)
                .map(|score| score.points)
                .sum(),
            flights,
            scored,
        })
    }
}

/// How two places in a ranking compare: the pilot with more points comes
/// first, and among pilots with the same points the handle decides, byte
/// by byte.
pub(crate) fn ranked_order(a: &Ranked, b: &Ranked) -> std::cmp::Ordering {
    b.points
        .partial_cmp(&a.points)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| a.pilot_handle.cmp(&b.pilot_handle))
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
    cut_by(
        flights,
        |flight| flight.change,
        deleted,
        |deleted| deleted.change,
        limit,
    )
}

/// The same for the changes to what the public sees: flights by their
/// public change number, and the withdrawals.
pub(crate) fn cut_public(
    flights: &mut Vec<Flight>,
    withdrawn: &mut Vec<WithdrawnFlight>,
    limit: usize,
) -> bool {
    cut_by(
        flights,
        |flight| flight.public_change.unwrap_or(i64::MAX),
        withdrawn,
        |withdrawn| withdrawn.change,
        limit,
    )
}

fn cut_by<A, B>(
    one: &mut Vec<A>,
    number_of_one: impl Fn(&A) -> i64,
    other: &mut Vec<B>,
    number_of_other: impl Fn(&B) -> i64,
    limit: usize,
) -> bool {
    let mut numbers: Vec<i64> = one
        .iter()
        .map(&number_of_one)
        .chain(other.iter().map(&number_of_other))
        .collect();
    if numbers.len() <= limit {
        return false;
    }
    numbers.sort_unstable();
    // Change numbers of one count are unique, so this keeps exactly
    // `limit` changes.
    match limit.checked_sub(1).map(|last| numbers[last]) {
        Some(last_kept) => {
            one.retain(|change| number_of_one(change) <= last_kept);
            other.retain(|change| number_of_other(change) <= last_kept);
        }
        None => {
            one.clear();
            other.clear();
        }
    }
    true
}

/// A flight that was public and is not: made private or unlisted, taken
/// down, deleted, or gone with its account. Nothing of the flight is here
/// but its address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WithdrawnFlight {
    /// The address the flight has or had.
    pub id: String,
    /// The number the withdrawal took among the public changes.
    pub change: i64,
    /// When it was withdrawn. `None` for a flight that went with its
    /// account: the store is not told when an account goes.
    pub withdrawn_at: Option<Timestamp>,
}

/// What changed in what the public sees of flights after some number.
///
/// Whatever makes a flight public, changes a public flight (its values
/// worked out again) or takes one from the public gets a number that is
/// larger than every number used before, in one count for all accounts.
/// Numbers have gaps. Whoever keeps a copy of the public flights remembers
/// the largest number seen and asks for what came after it.
#[derive(Debug, Clone, PartialEq)]
pub struct PublicFlightChanges {
    /// Flights that became public or changed while public, each as it is
    /// now, by public change number.
    pub flights: Vec<Flight>,
    /// Flights that are public no more, by change number.
    pub withdrawn: Vec<WithdrawnFlight>,
    /// Whether changes beyond the limit of the request are waiting.
    pub more: bool,
    /// The largest number the count has used, zero if it has used none.
    pub latest: i64,
}

impl PublicFlightChanges {
    /// The largest change number among the changes listed here.
    pub fn last(&self) -> Option<i64> {
        let flights = self
            .flights
            .iter()
            .filter_map(|flight| flight.public_change);
        let withdrawn = self.withdrawn.iter().map(|withdrawn| withdrawn.change);
        flights.chain(withdrawn).max()
    }
}

/// What a change of a flight's visibility is to the public.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublicEffect {
    /// Nothing: the flight was not public and is not.
    Nothing,
    /// The flight became public.
    Published,
    /// The flight is public no more.
    Withdrawn,
}

impl PublicEffect {
    /// What it is to the public that a flight which `was` visible so `is`
    /// now visible so. Both stores ask this, so that they publish and
    /// withdraw alike.
    pub(crate) fn of(was: Visibility, is: Visibility) -> Self {
        match (was == Visibility::Public, is == Visibility::Public) {
            (false, true) => Self::Published,
            (true, false) => Self::Withdrawn,
            (false, false) | (true, true) => Self::Nothing,
        }
    }
}

/// The next number of the count of public changes whose latest is
/// `latest`. With the time of the change it follows the clock, as an
/// account's own numbers do ([`next_change`]); without, which is the case
/// only when an account goes, it is the one after the latest.
pub(crate) fn next_public_change(latest: i64, now: Option<Timestamp>) -> i64 {
    match now {
        Some(now) => next_change(latest, now),
        None => latest.saturating_add(1),
    }
}
