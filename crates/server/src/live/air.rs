//! What many gliders say together: where pilots climb at this moment, and
//! how many are in the air at a flying site.
//!
//! ```text
//!  a network's free-flight positions ─┐   one glider's last minutes each,
//!  (the owners' choices applied)      ├─► in memory ([`Trails`])
//!  Venti's public live tracks ────────┘        │
//!                                              ▼ by fixed rules
//!                   climbs by cell of a grid · pilots by flying site
//!                   (no glider, no pilot, no track in either)
//! ```
//!
//! **What is kept.** Of every paraglider and hang glider a network hears,
//! and of every live track its pilot shows to everyone, the positions of
//! the last [`TRAIL_MINUTES`], no finer than one in [`THIN_S`] seconds,
//! and where it was first heard after a silence of [`APPEAR_AFTER`]. Only
//! in memory, never in the store, never in a log, and gone
//! [`FORGET_AFTER`] after its newest position: far inside the day the
//! network lets its data be passed on for. A track that only those with
//! its link may see is never taken in.
//!
//! **Circling and climbing** ([`circled`]) is decided by this rule, the
//! same for every glider:
//!
//! 1. Positions no more than [`MAX_STEP_S`] seconds apart follow one
//!    another; a longer silence ends a stretch, and nothing is made up
//!    for it.
//! 2. Each step has a course over the ground, and each pair of steps a
//!    turn, left negative. A step shorter than [`MIN_STEP_M`] has no
//!    course and turns nothing.
//! 3. The glider is circling at a position when it turned at least
//!    [`CIRCLING_TURN_DEG`] one way within the [`TURN_WINDOW_S`] seconds
//!    before it: a full circle a minute or tighter.
//! 4. Circling with no more than [`MERGE_GAP_S`] seconds between is one
//!    climb: the pilot centred again.
//! 5. It marks a thermal when the glider turned [`MIN_TURNED_DEG`] in all
//!    (a full circle), gained at least [`MIN_GAIN_M`] and climbed at
//!    least [`MIN_CLIMB_MS`] on average from its first circle to its
//!    last.
//!
//! A device heard less often than every [`MAX_STEP_S`] seconds marks no
//! thermal: its turns cannot be told. All limits are to verify against
//! real days.
//!
//! **In the air** ([`Flyer::airborne`]): heard in the last
//! [`SITE_AIR_HEARD_MINUTES`] and moving at [`AIRBORNE_MS`] or more along
//! its path of the last half minute (its reported speed, where the path
//! is too short to say). A tracker that is carried up a mountain in a
//! car counts while the car drives: to verify.
//!
//! **Took off at a site** ([`Flyer::took_off_at`]): first heard within
//! [`SITE_TAKEOFF_WITHIN_M`] of it in the last [`SITE_TAKEOFF_MINUTES`],
//! no more than [`TAKEOFF_ABOVE_M`] above it where the site has a height,
//! and in the air now or since heard farther from it than that.
//!
//! **What goes out names the air, not a person**: a climb is the middle
//! of a cell of [`LIVE_CLIMB_CELL_DEG`] with heights rounded to
//! [`LIVE_CLIMB_HEIGHT_STEP_M`]; a site has two numbers.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Mutex, MutexGuard};

use chrono::Duration;
use venti_api_types::v1::{
    LIVE_CLIMB_CELL_DEG, LIVE_CLIMB_HEIGHT_STEP_M, LIVE_CLIMB_MINUTES, LiveClimb, LiveClimbsLayer,
    LiveSitesLayer, MAX_LIVE_CLIMBS, MAX_LIVE_SITES, SITE_AIR_HEARD_MINUTES, SITE_AIR_WITHIN_M,
    SITE_TAKEOFF_MINUTES, SITE_TAKEOFF_WITHIN_M, SiteAir, cell_middle,
};
use venti_domain::{BoundingBox, GeoPoint, SiteId, Timestamp};

/// How much of a glider's way is kept, minutes: what a climb is listed
/// for, and two more for a climb that began before.
pub const TRAIL_MINUTES: i64 = LIVE_CLIMB_MINUTES + 2;

/// The least time between two positions that are kept, seconds.
pub const THIN_S: i64 = 2;

/// A glider not heard for this long that is heard again has appeared
/// anew: it landed and took off, or flew in from where nobody listens.
pub const APPEAR_AFTER: Duration = Duration::minutes(20);

/// How long after its newest position a glider is forgotten: as long as a
/// take-off is counted, and a little.
pub const FORGET_AFTER: Duration = Duration::minutes(SITE_TAKEOFF_MINUTES + 10);

/// The longest step between two positions that follow one another,
/// seconds.
pub const MAX_STEP_S: i64 = 12;

/// The shortest step that has a course, metres.
pub const MIN_STEP_M: f64 = 3.0;

/// The turn one way within [`TURN_WINDOW_S`] at which a glider circles,
/// degrees.
pub const CIRCLING_TURN_DEG: f64 = 120.0;

/// The seconds before a position its turning is summed over.
pub const TURN_WINDOW_S: i64 = 20;

/// Circling closer together than this, seconds, is one climb.
pub const MERGE_GAP_S: i64 = 20;

/// The least a glider turned in a climb, degrees: a full circle.
pub const MIN_TURNED_DEG: f64 = 360.0;

/// The least height gained in a climb, metres.
pub const MIN_GAIN_M: f64 = 30.0;

/// The least average climb, metres per second.
pub const MIN_CLIMB_MS: f64 = 0.5;

/// The speed along its path from which a glider is in the air, metres
/// per second: faster than anyone walks with a wing on the back.
pub const AIRBORNE_MS: f64 = 3.0;

/// How far above a site a glider may be first heard to have taken off
/// there, metres.
pub const TAKEOFF_ABOVE_M: f64 = 150.0;

/// How far outside a box a glider's newest position may lie for the
/// glider to be read for an answer about the box, degrees: more than a
/// glider flies in the ten minutes a climb is listed for, and more than
/// the distance a pilot is counted to a site from.
pub const AROUND_DEG: f64 = 0.15;

/// The most gliders one set of trails holds. A network's busiest day over
/// the Alps has some thousands of free-flight devices; beyond this a new
/// glider is not taken in until others are forgotten.
pub const MAX_TRAILS: usize = 20_000;

/// One position of a glider.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// When.
    pub time: Timestamp,
    /// Where.
    pub position: GeoPoint,
    /// Metres above mean sea level.
    pub altitude_m: f64,
}

/// One glider's last minutes, as the rules read them. It has no name.
#[derive(Debug, Clone, PartialEq)]
pub struct Flyer {
    /// Where it was first heard after its last silence.
    pub first: Sample,
    /// Its positions of the last [`TRAIL_MINUTES`], oldest first; never
    /// empty.
    pub trail: Vec<Sample>,
    /// The speed over the ground its newest position reported, m/s.
    pub speed_ms: Option<f64>,
    /// Whether its sender has said that it landed.
    pub landed: bool,
}

/// One climb of one glider.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Circled {
    /// The middle of its positions.
    pub position: GeoPoint,
    /// When the last circle ended.
    pub ended: Timestamp,
    /// From the first circle to the last, seconds.
    pub seconds: f64,
    /// The height at the first circle, metres.
    pub from_m: f64,
    /// The height at the last.
    pub to_m: f64,
}

struct Trail {
    first: Sample,
    samples: VecDeque<Sample>,
    speed_ms: Option<f64>,
    landed: bool,
}

/// The last minutes of many gliders, each under a key that never leaves
/// the server: a device's address, a live track's identifier.
#[derive(Default)]
pub struct Trails {
    state: Mutex<HashMap<String, Trail>>,
}

impl std::fmt::Debug for Trails {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the keys or a position.
        f.debug_struct("Trails").finish_non_exhaustive()
    }
}

impl Trails {
    fn state(&self) -> MutexGuard<'_, HashMap<String, Trail>> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Takes one position of the glider `key`. One that is not newer than
    /// the glider's newest by [`THIN_S`] is left out.
    pub fn take(&self, key: &str, sample: Sample, speed_ms: Option<f64>) {
        let mut state = self.state();
        if let Some(trail) = state.get_mut(key) {
            let newest = trail.samples.back().map_or(trail.first.time, |s| s.time);
            if sample.time < newest + Duration::seconds(THIN_S) {
                return;
            }
            if sample.time - newest > APPEAR_AFTER {
                trail.first = sample;
                trail.samples.clear();
                trail.landed = false;
            }
            trail.samples.push_back(sample);
            trail.speed_ms = speed_ms;
            let oldest = sample.time - Duration::minutes(TRAIL_MINUTES);
            while trail.samples.front().is_some_and(|s| s.time < oldest) {
                trail.samples.pop_front();
            }
        } else if state.len() < MAX_TRAILS {
            state.insert(
                key.to_owned(),
                Trail {
                    first: sample,
                    samples: VecDeque::from([sample]),
                    speed_ms,
                    landed: false,
                },
            );
        }
    }

    /// Notes whether the sender of `key` says that the glider landed.
    pub fn set_landed(&self, key: &str, landed: bool) {
        if let Some(trail) = self.state().get_mut(key) {
            trail.landed = landed;
        }
    }

    /// Forgets the glider `key`: its track was deleted, or is no longer
    /// shown to everyone.
    pub fn remove(&self, key: &str) {
        self.state().remove(key);
    }

    /// Forgets every glider whose newest position is older than
    /// [`FORGET_AFTER`] at `now`.
    pub fn forget_old(&self, now: Timestamp) {
        let oldest = now - FORGET_AFTER;
        self.state()
            .retain(|_, trail| trail.samples.back().is_some_and(|s| s.time >= oldest));
    }

    /// How many gliders are held.
    pub fn held(&self) -> usize {
        self.state().len()
    }

    /// The gliders heard in the last [`FORGET_AFTER`] before `now` whose
    /// key `may_be_read` allows, in the order of their keys. The keys
    /// stay here.
    pub fn flyers(&self, now: Timestamp, may_be_read: impl Fn(&str) -> bool) -> Vec<Flyer> {
        self.flyers_where(now, None, may_be_read)
    }

    /// The same, of the gliders whose newest position lies in `area` or
    /// within [`AROUND_DEG`] of it: what an answer about a box needs, so
    /// that it does not read every glider the server hears.
    pub fn flyers_near(
        &self,
        now: Timestamp,
        area: &BoundingBox,
        may_be_read: impl Fn(&str) -> bool,
    ) -> Vec<Flyer> {
        self.flyers_where(now, Some(area), may_be_read)
    }

    fn flyers_where(
        &self,
        now: Timestamp,
        area: Option<&BoundingBox>,
        may_be_read: impl Fn(&str) -> bool,
    ) -> Vec<Flyer> {
        let oldest = now - FORGET_AFTER;
        let near = |place: GeoPoint| {
            area.is_none_or(|area| {
                place.lat() >= area.south() - AROUND_DEG
                    && place.lat() <= area.north() + AROUND_DEG
                    && place.lon() >= area.west() - AROUND_DEG
                    && place.lon() <= area.east() + AROUND_DEG
            })
        };
        let state = self.state();
        let mut found: Vec<(&String, Flyer)> = state
            .iter()
            .filter(|(_, trail)| trail.samples.back().is_some_and(|s| near(s.position)))
            .filter(|(key, _)| may_be_read(key))
            .filter_map(|(key, trail)| {
                let newest = trail.samples.back()?;
                (newest.time >= oldest && newest.time <= now + Duration::minutes(1)).then(|| {
                    (
                        key,
                        Flyer {
                            first: trail.first,
                            trail: trail.samples.iter().copied().collect(),
                            speed_ms: trail.speed_ms,
                            landed: trail.landed,
                        },
                    )
                })
            })
            .collect();
        found.sort_by(|a, b| a.0.cmp(b.0));
        found.into_iter().map(|(_, flyer)| flyer).collect()
    }
}

fn seconds(from: Timestamp, to: Timestamp) -> i64 {
    (to - from).num_seconds()
}

/// The course from one position to another, degrees clockwise from north.
fn course_deg(from: GeoPoint, to: GeoPoint) -> f64 {
    let north = to.lat() - from.lat();
    let east = (to.lon() - from.lon()) * from.lat().to_radians().cos();
    east.atan2(north).to_degrees()
}

/// The climbs in one glider's positions, oldest first: the rule in the
/// head of this module.
pub fn circled(trail: &[Sample]) -> Vec<Circled> {
    let mut climbs = Vec::new();
    let mut from = 0;
    for at in 1..=trail.len() {
        let ends = at == trail.len()
            || seconds(trail[at - 1].time, trail[at].time) > MAX_STEP_S
            || trail[at].time <= trail[at - 1].time;
        if ends {
            climbs.extend(circled_in_a_stretch(&trail[from..at]));
            from = at;
        }
    }
    climbs
}

fn circled_in_a_stretch(stretch: &[Sample]) -> Vec<Circled> {
    if stretch.len() < 3 {
        return Vec::new();
    }
    // The turn made on arriving at each position, and its running sums:
    // one signed, one of the amounts.
    let mut signed = vec![0.0_f64; stretch.len()];
    let mut amount = vec![0.0_f64; stretch.len()];
    let mut course: Option<f64> = None;
    for at in 1..stretch.len() {
        let (before, here) = (stretch[at - 1], stretch[at]);
        let mut turn = 0.0;
        if before.position.distance_m(here.position) >= MIN_STEP_M {
            let now = course_deg(before.position, here.position);
            if let Some(was) = course {
                turn = (now - was + 540.0).rem_euclid(360.0) - 180.0;
            }
            course = Some(now);
        }
        signed[at] = signed[at - 1] + turn;
        amount[at] = amount[at - 1] + turn.abs();
    }
    // Where the glider is circling, and from which position that turning
    // began.
    let mut window_from = 0;
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for at in 1..stretch.len() {
        while seconds(stretch[window_from].time, stretch[at].time) > TURN_WINDOW_S {
            window_from += 1;
        }
        if (signed[at] - signed[window_from]).abs() < CIRCLING_TURN_DEG {
            continue;
        }
        match runs.last_mut() {
            Some((_, end))
                if seconds(stretch[*end].time, stretch[window_from].time) <= MERGE_GAP_S =>
            {
                *end = at;
            }
            _ => runs.push((window_from, at)),
        }
    }
    runs.into_iter()
        .filter_map(|(start, end)| {
            let (first, last) = (stretch[start], stretch[end]);
            let took = seconds(first.time, last.time) as f64;
            let gain = last.altitude_m - first.altitude_m;
            let is_a_climb = amount[end] - amount[start] >= MIN_TURNED_DEG
                && gain >= MIN_GAIN_M
                && took > 0.0
                && gain / took >= MIN_CLIMB_MS;
            if !is_a_climb {
                return None;
            }
            let circles = &stretch[start..=end];
            let count = circles.len() as f64;
            let lat = circles.iter().map(|s| s.position.lat()).sum::<f64>() / count;
            let lon = circles.iter().map(|s| s.position.lon()).sum::<f64>() / count;
            Some(Circled {
                position: GeoPoint::new(lat, lon).ok()?,
                ended: last.time,
                seconds: took,
                from_m: first.altitude_m,
                to_m: last.altitude_m,
            })
        })
        .collect()
}

impl Flyer {
    /// The newest position.
    fn newest(&self) -> Sample {
        self.trail.last().copied().unwrap_or(self.first)
    }

    /// Whether the glider is in the air at `now`: the rule in the head of
    /// this module.
    pub fn airborne(&self, now: Timestamp) -> bool {
        let newest = self.newest();
        if self.landed || now - newest.time > Duration::minutes(SITE_AIR_HEARD_MINUTES) {
            return false;
        }
        // Its path of the last half minute.
        let since = newest.time - Duration::seconds(30);
        let late: Vec<&Sample> = self.trail.iter().filter(|s| s.time >= since).collect();
        let took = late
            .first()
            .map_or(0, |first| seconds(first.time, newest.time));
        if took >= 6 {
            let path: f64 = late
                .windows(2)
                .map(|pair| pair[0].position.distance_m(pair[1].position))
                .sum();
            path / took as f64 >= AIRBORNE_MS
        } else {
            self.speed_ms.is_some_and(|speed| speed >= AIRBORNE_MS)
        }
    }

    /// Whether the glider took off at a site in the last
    /// [`SITE_TAKEOFF_MINUTES`] before `now`.
    pub fn took_off_at(&self, site: GeoPoint, elevation_m: Option<f64>, now: Timestamp) -> bool {
        if now - self.first.time > Duration::minutes(SITE_TAKEOFF_MINUTES)
            || self.first.position.distance_m(site) > SITE_TAKEOFF_WITHIN_M
            || elevation_m.is_some_and(|site_m| self.first.altitude_m > site_m + TAKEOFF_ABOVE_M)
        {
            return false;
        }
        self.airborne(now) || self.newest().position.distance_m(site) > SITE_TAKEOFF_WITHIN_M
    }
}

fn to_step(metres: f64) -> i32 {
    let step = f64::from(LIVE_CLIMB_HEIGHT_STEP_M);
    // Bounded: no glider is above 20 km or below the sea.
    #[allow(clippy::cast_possible_truncation)]
    let rounded = ((metres / step).round() * step).clamp(-1000.0, 20_000.0) as i32;
    rounded
}

/// The climbs of one source's gliders in a box at `now`, by cell.
pub fn climbs(
    source: &str,
    flyers: &[Flyer],
    area: &BoundingBox,
    now: Timestamp,
) -> LiveClimbsLayer {
    struct Cell {
        gain: f64,
        seconds: f64,
        from: f64,
        to: f64,
        pilots: u32,
        ended: Timestamp,
    }
    let since = now - Duration::minutes(LIVE_CLIMB_MINUTES);
    // By the cell's middle in millionths of a degree, so that the order
    // is fixed.
    let key = |place: GeoPoint| {
        #[allow(clippy::cast_possible_truncation)]
        let millionths = |degrees: f64| (degrees * 1e6).round() as i64;
        (millionths(place.lat()), millionths(place.lon()))
    };
    let mut cells: BTreeMap<(i64, i64), (GeoPoint, Cell)> = BTreeMap::new();
    for flyer in flyers {
        let mut counted: Vec<(i64, i64)> = Vec::new();
        for climb in circled(&flyer.trail) {
            if climb.ended < since || climb.ended > now + Duration::minutes(1) {
                continue;
            }
            let middle = cell_middle(climb.position, LIVE_CLIMB_CELL_DEG);
            if !area.contains(middle) {
                continue;
            }
            let (_, cell) = cells.entry(key(middle)).or_insert((
                middle,
                Cell {
                    gain: 0.0,
                    seconds: 0.0,
                    from: climb.from_m,
                    to: climb.to_m,
                    pilots: 0,
                    ended: climb.ended,
                },
            ));
            cell.gain += climb.to_m - climb.from_m;
            cell.seconds += climb.seconds;
            cell.from = cell.from.min(climb.from_m);
            cell.to = cell.to.max(climb.to_m);
            cell.ended = cell.ended.max(climb.ended);
            if !counted.contains(&key(middle)) {
                counted.push(key(middle));
                cell.pilots += 1;
            }
        }
    }
    let mut found: Vec<((i64, i64), LiveClimb, Timestamp)> = cells
        .into_iter()
        .map(|(key, (position, cell))| {
            let climb = LiveClimb {
                position,
                climb_ms: (cell.gain / cell.seconds.max(1.0) * 10.0).round() / 10.0,
                from_m: to_step(cell.from),
                to_m: to_step(cell.to),
                pilots: cell.pilots,
                age_s: u32::try_from(seconds(cell.ended, now).max(0)).unwrap_or(u32::MAX),
            };
            (key, climb, cell.ended)
        })
        .collect();
    found.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    let truncated = found.len() > MAX_LIVE_CLIMBS;
    found.truncate(MAX_LIVE_CLIMBS);
    LiveClimbsLayer {
        source: source.to_owned(),
        climbs: found.into_iter().map(|(_, climb, _)| climb).collect(),
        truncated,
    }
}

/// A flying site, as the counting needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    /// The site.
    pub site: SiteId,
    /// Where it is.
    pub position: GeoPoint,
    /// Its height, if it has one.
    pub elevation_m: Option<f64>,
}

/// How many of some gliders are in the air within [`SITE_AIR_WITHIN_M`]
/// of a place at `now`, and how many of them took off there.
pub fn in_the_air_at(
    flyers: &[Flyer],
    place: GeoPoint,
    elevation_m: Option<f64>,
    now: Timestamp,
) -> (u32, u32) {
    let (mut airborne, mut took_off) = (0, 0);
    for flyer in flyers {
        if flyer.newest().position.distance_m(place) <= SITE_AIR_WITHIN_M && flyer.airborne(now) {
            airborne += 1;
        }
        if flyer.took_off_at(place, elevation_m, now) {
            took_off += 1;
        }
    }
    (airborne, took_off)
}

/// The sites one source's gliders are in the air at.
pub fn sites(source: &str, flyers: &[Flyer], places: &[Place], now: Timestamp) -> LiveSitesLayer {
    let mut found: Vec<SiteAir> = places
        .iter()
        .filter_map(|place| {
            let (airborne, took_off) =
                in_the_air_at(flyers, place.position, place.elevation_m, now);
            (airborne > 0 || took_off > 0).then(|| SiteAir {
                site: place.site.clone(),
                airborne,
                took_off,
            })
        })
        .collect();
    found.sort_by(|a, b| (b.airborne, b.took_off, &a.site).cmp(&(a.airborne, a.took_off, &b.site)));
    let truncated = found.len() > MAX_LIVE_SITES;
    found.truncate(MAX_LIVE_SITES);
    LiveSitesLayer {
        source: source.to_owned(),
        sites: found,
        truncated,
    }
}

/// Made-up gliders for the tests and the development mode.
pub mod made_up {
    use super::*;

    /// A glider that circles once every `circle_s` seconds on a circle of
    /// 40 m around `centre` from `start`, for `took_s` seconds, a position
    /// every `every_s`, climbing `climb_ms` from `from_m`.
    pub fn circling(
        centre: GeoPoint,
        start: Timestamp,
        took_s: i64,
        every_s: i64,
        circle_s: f64,
        from_m: f64,
        climb_ms: f64,
    ) -> Vec<Sample> {
        let radius_deg = 40.0 / 111_195.0;
        (0..=took_s / every_s)
            .filter_map(|step| {
                let t = (step * every_s) as f64;
                let angle = std::f64::consts::TAU * t / circle_s;
                let lat = centre.lat() + radius_deg * angle.cos();
                let lon = centre.lon() + radius_deg * angle.sin() / centre.lat().to_radians().cos();
                Some(Sample {
                    time: start + Duration::seconds(step * every_s),
                    position: GeoPoint::new(lat, lon).ok()?,
                    altitude_m: from_m + climb_ms * t,
                })
            })
            .collect()
    }

    /// A glider that flies north from `from` at `speed_ms` from `start`,
    /// for `took_s` seconds, a position every `every_s`, sinking 1 m/s.
    pub fn gliding(
        from: GeoPoint,
        start: Timestamp,
        took_s: i64,
        every_s: i64,
        speed_ms: f64,
        from_m: f64,
    ) -> Vec<Sample> {
        (0..=took_s / every_s)
            .filter_map(|step| {
                let t = (step * every_s) as f64;
                Some(Sample {
                    time: start + Duration::seconds(step * every_s),
                    position: GeoPoint::new(from.lat() + speed_ms * t / 111_195.0, from.lon())
                        .ok()?,
                    altitude_m: from_m - t,
                })
            })
            .collect()
    }

    /// Feeds `trails` the positions of one glider.
    pub fn feed(trails: &Trails, key: &str, samples: &[Sample]) {
        for sample in samples {
            trails.take(key, *sample, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::made_up::{circling, feed, gliding};
    use super::*;

    fn now() -> Timestamp {
        Utc.with_ymd_and_hms(2026, 7, 4, 11, 0, 0).unwrap()
    }

    fn at(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).unwrap()
    }

    fn flyer(trail: Vec<Sample>) -> Flyer {
        Flyer {
            first: trail[0],
            trail,
            speed_ms: None,
            landed: false,
        }
    }

    #[test]
    fn circles_that_climb_are_a_climb_and_nothing_else_is() {
        let start = now() - Duration::minutes(5);
        let centre = at(47.7012, 11.7989);
        // Three minutes of 20-second circles at +2 m/s, a position every
        // four seconds.
        let thermal = circling(centre, start, 180, 4, 20.0, 1600.0, 2.0);
        let found = circled(&thermal);
        assert_eq!(found.len(), 1, "{found:?}");
        let climb = found[0];
        assert!(climb.position.distance_m(centre) < 30.0);
        assert!((climb.to_m - climb.from_m) / climb.seconds > 1.9);
        assert!(climb.to_m > 1900.0 && climb.from_m < 1650.0, "{climb:?}");

        // The same circles sinking: a spiral, no thermal.
        assert!(circled(&circling(centre, start, 180, 4, 20.0, 1600.0, -2.0)).is_empty());
        // Zero sink in circles: no thermal either.
        assert!(circled(&circling(centre, start, 180, 4, 20.0, 1600.0, 0.3)).is_empty());
        // A straight glide in rising air: no circle.
        let mut ridge = gliding(centre, start, 180, 4, 10.0, 1600.0);
        for (step, sample) in ridge.iter_mut().enumerate() {
            sample.altitude_m = 1600.0 + 2.0 * 4.0 * step as f64;
        }
        assert!(circled(&ridge).is_empty());
        // Half a circle that climbs: a turn, no thermal.
        assert!(circled(&circling(centre, start, 10, 2, 20.0, 1600.0, 4.0)).is_empty());
        // Heard only every fifteen seconds: its turns cannot be told.
        assert!(circled(&circling(centre, start, 180, 15, 20.0, 1600.0, 2.0)).is_empty());
        // Nothing, one position.
        assert!(circled(&[]).is_empty());
        assert!(circled(&thermal[..1]).is_empty());
    }

    #[test]
    fn a_silence_ends_a_climb_and_centring_again_does_not() {
        let start = now() - Duration::minutes(8);
        let centre = at(47.7012, 11.7989);
        let mut trail = circling(centre, start, 120, 4, 20.0, 1500.0, 1.5);
        // Eight seconds on, circles again 60 m further north.
        let again = start + Duration::seconds(128);
        trail.extend(circling(
            at(47.7017, 11.7989),
            again,
            120,
            4,
            20.0,
            1700.0,
            1.5,
        ));
        // Each step is short enough for the circles to be one stretch.
        assert_eq!(circled(&trail).len(), 1);

        // The same with a minute of silence between: two.
        let mut apart = circling(centre, start, 120, 4, 20.0, 1500.0, 1.5);
        apart.extend(circling(
            centre,
            start + Duration::seconds(180),
            120,
            4,
            20.0,
            1700.0,
            1.5,
        ));
        assert_eq!(circled(&apart).len(), 2);
    }

    #[test]
    fn climbs_are_gathered_by_cell_without_a_glider() {
        let centre = at(47.7012, 11.7989);
        let area = BoundingBox::new(47.5, 11.5, 47.9, 12.1).unwrap();
        let flyers = vec![
            // Two in one thermal, one of them left it four minutes ago.
            flyer(circling(
                centre,
                now() - Duration::minutes(3),
                180,
                4,
                20.0,
                1600.0,
                2.0,
            )),
            flyer(circling(
                at(47.7015, 11.7985),
                now() - Duration::minutes(8),
                240,
                4,
                24.0,
                1400.0,
                3.0,
            )),
            // One in another, far away.
            flyer(circling(
                at(47.62, 11.9),
                now() - Duration::minutes(6),
                200,
                4,
                18.0,
                1200.0,
                1.0,
            )),
            // One gliding.
            flyer(gliding(
                centre,
                now() - Duration::minutes(3),
                180,
                4,
                10.0,
                1900.0,
            )),
        ];
        let layer = climbs("ogn", &flyers, &area, now());
        assert_eq!(layer.source, "ogn");
        assert_eq!(layer.climbs.len(), 2);
        let first = &layer.climbs[0];
        assert_eq!(first.pilots, 2);
        assert_eq!(first.age_s, 0);
        // The middle of the cell, not of the circles.
        assert_eq!(
            (first.position.lat(), first.position.lon()),
            (47.7025, 11.7975)
        );
        assert_eq!((first.from_m, first.to_m), (1400, 2100));
        // 360 m in 180 s and 720 m in 240 s: 1080 m in 420 s.
        assert!((first.climb_ms - 2.6).abs() < 0.11, "{}", first.climb_ms);
        let second = &layer.climbs[1];
        assert_eq!(second.pilots, 1);
        assert!((150..=170).contains(&second.age_s), "{}", second.age_s);
        assert_eq!(second.climb_ms, 1.0);

        // A box without them, and a moment long after.
        let elsewhere = BoundingBox::new(46.0, 11.5, 46.5, 12.1).unwrap();
        assert!(climbs("ogn", &flyers, &elsewhere, now()).climbs.is_empty());
        let later = now() + Duration::minutes(15);
        assert!(climbs("ogn", &flyers, &area, later).climbs.is_empty());
    }

    #[test]
    fn pilots_are_counted_at_a_site_when_they_fly() {
        let site = at(47.6640, 11.7480);
        let places = [
            Place {
                site: "osm:node/1".parse().unwrap(),
                position: site,
                elevation_m: Some(1620.0),
            },
            Place {
                site: "osm:node/2".parse().unwrap(),
                position: at(47.2, 11.0),
                elevation_m: None,
            },
        ];
        let trails = Trails::default();
        // Took off at the site twenty minutes ago and is 2 km north now.
        feed(
            &trails,
            "a",
            &gliding(site, now() - Duration::minutes(20), 200, 4, 10.0, 1625.0),
        );
        feed(
            &trails,
            "a",
            &gliding(
                at(47.68, 11.748),
                now() - Duration::minutes(2),
                120,
                4,
                10.0,
                1500.0,
            ),
        );
        // Flew in from elsewhere and circles 3 km away.
        feed(
            &trails,
            "b",
            &circling(
                at(47.69, 11.75),
                now() - Duration::minutes(4),
                240,
                4,
                20.0,
                1900.0,
                1.0,
            ),
        );
        // Stands at the take-off with the tracker on.
        feed(
            &trails,
            "c",
            &gliding(site, now() - Duration::minutes(2), 120, 4, 0.0, 1620.0)
                .into_iter()
                .map(|sample| Sample {
                    altitude_m: 1620.0,
                    ..sample
                })
                .collect::<Vec<_>>(),
        );
        // Flies far to the north of it.
        feed(
            &trails,
            "d",
            &gliding(
                at(47.9, 11.748),
                now() - Duration::minutes(3),
                180,
                4,
                10.0,
                2400.0,
            ),
        );
        let flyers = trails.flyers(now(), |_| true);
        assert_eq!(flyers.len(), 4);
        let layer = sites("venti", &flyers, &places, now());
        assert_eq!(layer.sites.len(), 1);
        assert_eq!(layer.sites[0].site.to_string(), "osm:node/1");
        assert_eq!((layer.sites[0].airborne, layer.sites[0].took_off), (2, 1));

        // A glider that says it landed is not in the air; one that may
        // not be read is not there.
        trails.set_landed("b", true);
        let flyers = trails.flyers(now(), |key| key != "a");
        assert_eq!(sites("venti", &flyers, &places, now()).sites, []);
        // Six minutes on, nobody was heard lately.
        let flyers = trails.flyers(now() + Duration::minutes(6), |_| true);
        let later = sites("venti", &flyers, &places, now() + Duration::minutes(6));
        assert_eq!(
            later.sites.iter().map(|s| s.airborne).sum::<u32>(),
            0,
            "{later:?}"
        );
    }

    #[test]
    fn a_trail_is_thinned_bounded_and_forgotten() {
        let trails = Trails::default();
        let start = now() - Duration::minutes(30);
        // A position a second for half an hour.
        feed(
            &trails,
            "a",
            &gliding(at(47.0, 11.0), start, 1800, 1, 8.0, 3000.0),
        );
        let flyer = trails.flyers(now(), |_| true).remove(0);
        assert_eq!(flyer.first.time, start);
        // Twelve minutes, every other second.
        assert!(
            (355..=361).contains(&flyer.trail.len()),
            "{}",
            flyer.trail.len()
        );
        assert!(
            flyer
                .trail
                .windows(2)
                .all(|pair| seconds(pair[0].time, pair[1].time) >= THIN_S)
        );
        // An older position changes nothing.
        trails.take("a", flyer.trail[3], None);
        assert_eq!(
            trails.flyers(now(), |_| true)[0].trail.len(),
            flyer.trail.len()
        );

        // Heard again after half an hour of silence: it has appeared anew.
        let again = Sample {
            time: now() + Duration::minutes(30),
            ..flyer.first
        };
        trails.take("a", again, Some(9.0));
        let anew = trails.flyers(again.time, |_| true).remove(0);
        assert_eq!((anew.first, anew.trail.len()), (again, 1));
        assert!(anew.airborne(again.time), "by the speed it reports");

        // Read for a box: those in it or near it, and no others.
        let here = BoundingBox::new(46.9, 10.9, 47.1, 11.1).unwrap();
        let far = BoundingBox::new(40.0, 0.0, 41.0, 1.0).unwrap();
        assert_eq!(trails.flyers_near(again.time, &here, |_| true).len(), 1);
        assert!(trails.flyers_near(again.time, &far, |_| true).is_empty());

        trails.forget_old(again.time + Duration::minutes(71));
        assert_eq!(trails.held(), 0);
        trails.take("a", again, None);
        trails.remove("a");
        assert_eq!(trails.held(), 0);
        // What is printed of them names no glider.
        trails.take("device-key", again, None);
        assert!(!format!("{trails:?}").contains("device-key"));
    }
}
