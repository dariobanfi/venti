//! Live tracking: where a glider is while it flies, for those its pilot
//! lets see it.
//!
//! An app sends the positions of one flight in small batches. The server
//! keeps them as a live track, hands them to whoever may see it, and
//! removes the track a short while after the flight. A live track is never
//! made into a flight: the logbook's flight is the file the recorder wrote.
//!
//! - This module holds the numbers that bound live tracking ([`Limits`])
//!   and decides which fixes of a batch the server takes ([`check`]).
//! - [`page`] is what a link to a live track shows.
//! - [`network`] holds the aircraft that another network sees, apart from
//!   Venti's own live tracks: a source of another licence.
//!
//! **A position is where a person is.** Nothing here or in the handlers
//! writes a position, or the identifier of a track, into a log line or an
//! error message: whoever has a track's identifier can see the track.

pub mod network;
pub mod page;

use chrono::{Duration, SubsecRound};
use venti_api_types::v1::{self, LIVE_LISTED_MINUTES, MAX_LIVE_BATCH_FIXES};
use venti_domain::Timestamp;
use venti_store::LiveFix;

/// How long a live track is kept after its newest fix unless the
/// configuration says otherwise, in hours.
///
/// Long enough to look at where someone landed on the evening and the day
/// after, and to find a glider that did not come home. Short enough that
/// the server holds nobody's movements of last week.
pub const DEFAULT_RETENTION_HOURS: u32 = 48;

/// The numbers that bound live tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Fixes one account may send within a minute, whatever becomes of
    /// them. A batch is taken while the account is under this, so the
    /// minute can end up to one batch over.
    ///
    /// Five times what a fix a second comes to: after ten minutes without
    /// a network an app has six hundred fixes waiting, and sends them in
    /// two and a half minutes beside the new ones. It is also what bounds
    /// what one account can make the server store: 432,000 fixes a day,
    /// about 71 MB in PostgreSQL (165 bytes a fix with its indexes,
    /// measured 2026-11-19), and at most 100 MB if every minute ends a
    /// batch over. All of it is gone again two days later.
    pub fixes_per_minute: u32,
    /// Live tracks one account may begin within a day, and identifiers it
    /// may take out of use without a track. More than the flights of the
    /// longest day of tandem flying (an estimate), and what bounds how
    /// many gliders one account can put on everybody's map.
    pub tracks_per_day: u32,
    /// Fixes one track holds: twelve hours at a fix a second. A flight
    /// that long is a record; a track that is full takes no more, and the
    /// app begins another.
    pub fixes_per_track: u32,
    /// How far ahead of the server's clock the time of a fix may lie. A
    /// phone that takes its time from the network is seconds off at most;
    /// one whose clock was set by hand to something else is told.
    pub future: Duration,
    /// How old a fix may be. As long as the longest track, so that an app
    /// that had no network for hours can still hand in what it recorded,
    /// and no longer: a live track is not the place for last week's
    /// flight.
    pub past: Duration,
    /// How long after its newest fix a public track is listed.
    pub listed: Duration,
    /// How long after its newest fix a track is kept.
    pub retention: Duration,
    /// Whether the clock decides anything about a fix or a track: whether
    /// a fix is from the future or too old to take, and whether a track
    /// ended too long ago to be kept. `false` only in the development
    /// mode.
    pub by_the_clock: bool,
}

impl Limits {
    /// The limits of a server that keeps a live track for `retention`
    /// after its newest fix.
    pub fn keeping(retention: Duration) -> Self {
        Self {
            fixes_per_minute: 300,
            tracks_per_day: 30,
            fixes_per_track: 43_200,
            future: Duration::minutes(2),
            past: Duration::hours(12),
            listed: Duration::minutes(LIVE_LISTED_MINUTES),
            retention,
            by_the_clock: true,
        }
    }

    /// Limits for the development mode. Its clock stands still at the
    /// moment of its recorded data, while an app under test sends fixes
    /// with the time of its own clock, or of a recorded flight of last
    /// summer: fixes of any time are taken, no track is ever too old to
    /// read or to add to, and no count of attempts bites. Everything else
    /// is as on a real server: what a track holds, what an answer says
    /// about when a track would be removed, and that a public track is
    /// listed while its newest fix is no older than half an hour by that
    /// mode's clock.
    pub fn none() -> Self {
        Self {
            fixes_per_minute: u32::MAX,
            tracks_per_day: u32::MAX,
            by_the_clock: false,
            ..Self::default()
        }
    }

    /// The moment before which a track's newest fix must not lie for the
    /// track to be still there at `now`.
    pub fn expired_before(&self, now: Timestamp) -> Timestamp {
        if self.by_the_clock {
            now - self.retention
        } else {
            Timestamp::MIN_UTC
        }
    }

    /// The oldest and the newest time a fix may have at `now`; `None`
    /// where any time is taken.
    fn window(&self, now: Timestamp) -> Option<(Timestamp, Timestamp)> {
        self.by_the_clock
            .then(|| (now - self.past, now + self.future))
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self::keeping(Duration::hours(i64::from(DEFAULT_RETENTION_HOURS)))
    }
}

/// The values a fix may have. Wide enough for anything that flies under a
/// paraglider or beside one, narrow enough that nonsense is refused.
mod range {
    use std::ops::RangeInclusive;

    /// Metres above mean sea level: from below the lowest land to far
    /// above where anything without an engine flies.
    pub(super) const ALTITUDE_M: RangeInclusive<i32> = -1000..=20_000;
    /// Metres per second over the ground.
    pub(super) const GROUND_SPEED_MS: RangeInclusive<f64> = 0.0..=150.0;
    /// Metres per second up or down.
    pub(super) const CLIMB_MS: RangeInclusive<f64> = -100.0..=100.0;
    /// Degrees clockwise from north; both ends are north.
    pub(super) const COURSE_DEG: RangeInclusive<f64> = 0.0..=360.0;
}

/// Why a batch of fixes is refused as a whole. What is said names the
/// place of a fix in the batch and never one of its values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BadBatch {
    /// No fix at all.
    #[error("the request carries no fix; send at least one")]
    Empty,
    /// More than one request may carry.
    #[error(
        "the request carries {0} fixes and may carry {MAX_LIVE_BATCH_FIXES}; send the rest in \
         further requests"
    )]
    TooMany(usize),
    /// A fix that is older than the one before it.
    #[error("fix {0} is older than the fix before it; send fixes oldest first")]
    Order(usize),
    /// A value no glider has.
    #[error("the {what} of fix {index} is not a value the server takes")]
    Value {
        /// The place of the fix in the batch, counted from one.
        index: usize,
        /// Which value.
        what: &'static str,
    },
}

/// What is left of a batch once it has been looked at.
#[derive(Debug, Clone, PartialEq)]
pub struct Checked {
    /// The fixes the server takes, oldest first, one to a whole second.
    pub fixes: Vec<LiveFix>,
    /// Fixes left out because the batch had one for the same second
    /// already.
    pub repeated: u32,
    /// Fixes left out because their time lies in the future.
    pub future: u32,
    /// Fixes left out because they are too old.
    pub old: u32,
}

/// Looks at the fixes of one request at `now`.
///
/// A batch that is not what the API asks for is refused whole: no fix or
/// too many, fixes that are not oldest first, a value that cannot be
/// right. Those are faults of the program that sent it, to be found while
/// it is written.
///
/// A fix whose time the server does not take is left out and counted, and
/// the rest is kept: after a long gap a batch may hold fixes from either
/// side of the line, and an app should not have to know where the line
/// is. Times are cut to whole seconds, and of two fixes in one second the
/// first is kept.
pub fn check(fixes: &[v1::LiveFix], now: Timestamp, limits: &Limits) -> Result<Checked, BadBatch> {
    if fixes.is_empty() {
        return Err(BadBatch::Empty);
    }
    if fixes.len() > MAX_LIVE_BATCH_FIXES {
        return Err(BadBatch::TooMany(fixes.len()));
    }
    let window = limits.window(now);

    let mut checked = Checked {
        fixes: Vec::with_capacity(fixes.len()),
        repeated: 0,
        future: 0,
        old: 0,
    };
    let mut before: Option<Timestamp> = None;
    for (place, fix) in fixes.iter().enumerate() {
        let index = place + 1;
        let unusable = |what| BadBatch::Value { index, what };
        if !range::ALTITUDE_M.contains(&fix.altitude_m) {
            return Err(unusable("altitude"));
        }
        // A number that is not finite is in no range.
        let outside = |value: Option<f64>, range: &std::ops::RangeInclusive<f64>| {
            value.is_some_and(|value| !range.contains(&value))
        };
        if outside(fix.ground_speed_ms, &range::GROUND_SPEED_MS) {
            return Err(unusable("ground speed"));
        }
        if outside(fix.climb_ms, &range::CLIMB_MS) {
            return Err(unusable("climb"));
        }
        if outside(fix.course_deg, &range::COURSE_DEG) {
            return Err(unusable("course"));
        }

        let time = fix.time.trunc_subsecs(0);
        match before {
            Some(before) if time < before => return Err(BadBatch::Order(index)),
            Some(before) if time == before => {
                checked.repeated += 1;
                continue;
            }
            _ => before = Some(time),
        }
        if window.is_some_and(|(_, latest)| time > latest) {
            checked.future += 1;
        } else if window.is_some_and(|(earliest, _)| time < earliest) {
            checked.old += 1;
        } else {
            checked.fixes.push(LiveFix {
                time,
                position: fix.position,
                altitude_m: fix.altitude_m,
                ground_speed_ms: fix.ground_speed_ms,
                climb_ms: fix.climb_ms,
                course_deg: fix.course_deg,
            });
        }
    }
    Ok(checked)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use venti_domain::GeoPoint;

    use super::*;

    fn now() -> Timestamp {
        Utc.with_ymd_and_hms(2026, 11, 18, 19, 5, 0).unwrap()
    }

    /// A fix `seconds` after now (before it, when negative).
    fn fix(seconds: i64) -> v1::LiveFix {
        v1::LiveFix {
            time: now() + Duration::seconds(seconds),
            position: GeoPoint::new(47.66, 11.8).unwrap(),
            altitude_m: 1600,
            ground_speed_ms: Some(9.4),
            climb_ms: Some(-1.2),
            course_deg: Some(320.0),
        }
    }

    fn seconds(checked: &Checked) -> Vec<i64> {
        checked
            .fixes
            .iter()
            .map(|fix| (fix.time - now()).num_seconds())
            .collect()
    }

    #[test]
    fn the_defaults_are_the_numbers_the_documents_give() {
        let limits = Limits::default();
        assert_eq!(limits.fixes_per_minute, 300);
        assert_eq!(limits.tracks_per_day, 30);
        // Twelve hours at a fix a second.
        assert_eq!(limits.fixes_per_track, 12 * 3600);
        assert_eq!(limits.future, Duration::minutes(2));
        assert_eq!(limits.past, Duration::hours(12));
        assert_eq!(limits.listed, Duration::minutes(30));
        assert_eq!(limits.retention, Duration::hours(48));
        // A backlog of ten minutes at a fix a second is sent within three
        // minutes, beside the new fixes.
        let spare_per_minute = i64::from(limits.fixes_per_minute) - 60;
        assert!(600 / spare_per_minute < 3);

        assert_eq!(limits.expired_before(now()), now() - Duration::hours(48));

        // The development mode differs in what its standing clock would
        // spoil, and in nothing else.
        let dev = Limits::none();
        assert_eq!(dev.fixes_per_track, limits.fixes_per_track);
        assert_eq!(dev.listed, limits.listed);
        assert_eq!(dev.retention, limits.retention);
        assert!(dev.expired_before(now()) < now() - Duration::days(365 * 1000));
    }

    #[test]
    fn a_batch_of_good_fixes_is_taken_whole_in_whole_seconds() {
        let mut batch = vec![fix(-30), fix(-20), fix(-10)];
        batch[1].time += Duration::milliseconds(750);
        batch[2].ground_speed_ms = None;

        let checked = check(&batch, now(), &Limits::default()).unwrap();

        assert_eq!(seconds(&checked), [-30, -20, -10]);
        assert_eq!((checked.repeated, checked.future, checked.old), (0, 0, 0));
        assert_eq!(checked.fixes[0].ground_speed_ms, Some(9.4));
        assert_eq!(checked.fixes[2].ground_speed_ms, None);
        assert_eq!(checked.fixes[1].position, batch[1].position);
    }

    #[test]
    fn fixes_from_the_future_or_from_long_ago_are_left_out_and_counted() {
        let hour = 3600;
        let batch = [
            fix(-13 * hour),
            fix(-12 * hour - 1),
            // The oldest and the newest moment that are taken.
            fix(-12 * hour),
            fix(-5),
            fix(120),
            fix(121),
            fix(24 * hour),
        ];

        let checked = check(&batch, now(), &Limits::default()).unwrap();

        assert_eq!(seconds(&checked), [-12 * hour, -5, 120]);
        assert_eq!((checked.old, checked.future), (2, 2));

        // A phone whose clock is a day ahead: nothing is left.
        let ahead = [fix(24 * hour), fix(24 * hour + 1)];
        let checked = check(&ahead, now(), &Limits::default()).unwrap();
        assert!(checked.fixes.is_empty());
        assert_eq!(checked.future, 2);

        // The development mode takes fixes of any time.
        let any = [fix(-200 * 24 * hour), fix(400 * 24 * hour)];
        let checked = check(&any, now(), &Limits::none()).unwrap();
        assert_eq!(checked.fixes.len(), 2);
    }

    #[test]
    fn of_two_fixes_in_one_second_the_first_is_kept() {
        let mut batch = vec![fix(-10), fix(-10), fix(-9), fix(-9)];
        batch[1].altitude_m = 9999;
        batch[1].time += Duration::milliseconds(400);
        batch[3].time += Duration::milliseconds(999);

        let checked = check(&batch, now(), &Limits::default()).unwrap();

        assert_eq!(seconds(&checked), [-10, -9]);
        assert_eq!(checked.repeated, 2);
        assert_eq!(checked.fixes[0].altitude_m, 1600);
    }

    #[test]
    fn a_batch_that_is_not_what_the_api_asks_for_is_refused_whole() {
        let limits = Limits::default();
        assert_eq!(check(&[], now(), &limits), Err(BadBatch::Empty));
        let many: Vec<v1::LiveFix> = (0..121).map(|n| fix(n - 500)).collect();
        assert_eq!(check(&many, now(), &limits), Err(BadBatch::TooMany(121)));
        assert!(check(&many[..120], now(), &limits).is_ok());

        // Newest first, also among fixes that would be left out anyway.
        let unordered = [fix(-10), fix(-20)];
        assert_eq!(check(&unordered, now(), &limits), Err(BadBatch::Order(2)));
        let unordered = [fix(-5), fix(999_999), fix(999_998)];
        assert_eq!(check(&unordered, now(), &limits), Err(BadBatch::Order(3)));

        let refused = |change: fn(&mut v1::LiveFix), what: &'static str| {
            let mut batch = [fix(-20), fix(-10)];
            change(&mut batch[1]);
            assert_eq!(
                check(&batch, now(), &limits),
                Err(BadBatch::Value { index: 2, what }),
                "{what}"
            );
        };
        refused(|fix| fix.altitude_m = 20_001, "altitude");
        refused(|fix| fix.altitude_m = -1001, "altitude");
        refused(|fix| fix.ground_speed_ms = Some(-0.1), "ground speed");
        refused(|fix| fix.ground_speed_ms = Some(150.1), "ground speed");
        refused(|fix| fix.ground_speed_ms = Some(f64::NAN), "ground speed");
        refused(|fix| fix.climb_ms = Some(100.5), "climb");
        refused(|fix| fix.climb_ms = Some(f64::NEG_INFINITY), "climb");
        refused(|fix| fix.course_deg = Some(360.5), "course");
        refused(|fix| fix.course_deg = Some(-1.0), "course");

        // The ends of every range are values a fix may have.
        let mut edge = [fix(-20), fix(-10)];
        edge[0].altitude_m = -1000;
        edge[0].ground_speed_ms = Some(0.0);
        edge[0].climb_ms = Some(-100.0);
        edge[0].course_deg = Some(0.0);
        edge[1].altitude_m = 20_000;
        edge[1].ground_speed_ms = Some(150.0);
        edge[1].climb_ms = Some(100.0);
        edge[1].course_deg = Some(360.0);
        assert_eq!(check(&edge, now(), &limits).unwrap().fixes.len(), 2);
    }

    #[test]
    fn a_refusal_names_the_place_of_a_fix_and_none_of_its_values() {
        let mut batch = [fix(-20), fix(-10)];
        batch[1].altitude_m = 31_337;
        let error = check(&batch, now(), &Limits::default()).unwrap_err();
        let said = error.to_string();
        assert_eq!(
            said,
            "the altitude of fix 2 is not a value the server takes"
        );
        assert!(!said.contains("31337") && !said.contains("47.66"));
        assert_eq!(
            BadBatch::TooMany(500).to_string(),
            "the request carries 500 fixes and may carry 120; send the rest in further requests"
        );
    }
}
