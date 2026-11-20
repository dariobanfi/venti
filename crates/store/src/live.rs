//! Live tracks: the positions an app sends while a glider flies, for those
//! its pilot lets see them.
//!
//! A live track is not a flight. It is kept for a short time and then
//! removed, and nothing here ever turns one into a flight of the logbook.
//!
//! Both stores decide what a batch of fixes does to a track with the one
//! function here, [`merge`], so that they cannot come to differ.

use std::collections::BTreeSet;

use venti_domain::{GeoPoint, Timestamp};

use crate::StoreError;

/// Who may see a live track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LiveVisibility {
    /// Whoever has its identifier. It is in no list.
    Link,
    /// Everyone. It is listed for the part of the map it is in.
    Public,
}

impl LiveVisibility {
    /// The name the database knows the setting by.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::Public => "public",
        }
    }

    /// The setting with this name.
    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Link, Self::Public]
            .into_iter()
            .find(|visibility| visibility.as_str() == name)
    }

    /// Of two settings, the one that shows the track to fewer people.
    pub fn narrower(self, other: Self) -> Self {
        if self == Self::Link || other == Self::Link {
            Self::Link
        } else {
            Self::Public
        }
    }
}

/// Where a glider was at one moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveFix {
    /// When, in whole seconds.
    pub time: Timestamp,
    /// Where.
    pub position: GeoPoint,
    /// Altitude, metres above mean sea level.
    pub altitude_m: i32,
    /// Speed over the ground, m/s.
    pub ground_speed_ms: Option<f64>,
    /// Climb rate, m/s.
    pub climb_ms: Option<f64>,
    /// The direction of the movement over the ground, degrees clockwise
    /// from true north.
    pub course_deg: Option<f64>,
}

/// Fixes to add to a live track, with what their sender says about the
/// track.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveBatch {
    /// The track's identifier, made by the sender.
    pub id: String,
    /// The account that sends.
    pub account_id: String,
    /// Who may see the track.
    pub visibility: LiveVisibility,
    /// Whether the glider has landed.
    pub landed: bool,
    /// The fixes, oldest first: at least one, each in a later second than
    /// the one before, each at a whole second.
    pub fixes: Vec<LiveFix>,
}

impl LiveBatch {
    /// The same batch, saying that only those with the link may see the
    /// track: what a batch of an account that may not publish is taken as.
    pub(crate) fn for_those_with_the_link(&self) -> Self {
        Self {
            visibility: LiveVisibility::Link,
            ..self.clone()
        }
    }
}

/// What a call that adds fixes may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveRoom {
    /// The most fixes one track holds, at least one.
    pub fixes_per_track: u32,
    /// Whether the call may begin a track. Without it a batch for an
    /// identifier nothing is known of is turned away.
    pub may_create: bool,
    /// A track whose newest fix is older than this has ended for good: it
    /// takes no more fixes and is removed on the spot.
    pub expired_before: Timestamp,
    /// The moment of the call.
    pub now: Timestamp,
}

/// A stored live track, without its fixes.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveTrack {
    /// The track's identifier.
    pub id: String,
    /// The account it belongs to.
    pub account_id: String,
    /// That account's handle.
    pub pilot_handle: String,
    /// That account's display name.
    pub pilot_name: String,
    /// Who may see it.
    pub visibility: LiveVisibility,
    /// Whether its sender has said that the glider landed.
    pub landed: bool,
    /// The time of its oldest fix.
    pub started_at: Timestamp,
    /// Its newest fix.
    pub latest: LiveFix,
    /// How many fixes it holds.
    pub fixes: u32,
    /// The number of the fix that arrived last; see [`LiveFixAt`].
    pub cursor: i64,
}

/// A fix with its place in the order in which a track's fixes arrived.
///
/// Every fix a track takes in gets a number larger than all before it.
/// Whoever remembers the largest number seen can ask for what arrived
/// after it, and misses nothing: also not fixes that arrived late and lie
/// before others in time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveFixAt {
    /// The fix's number in its track.
    pub seq: i64,
    /// The fix.
    pub fix: LiveFix,
}

/// What became of a batch handed to
/// [`Store::save_live_fixes`](crate::Store::save_live_fixes).
#[derive(Debug, Clone, PartialEq)]
pub enum LiveSaved {
    /// The track has taken the batch in.
    Kept(Box<LiveKept>),
    /// Nothing is known under the identifier, and the call was not allowed
    /// to begin a track. Nothing was stored.
    NotCreated,
    /// The identifier was used for a track that is gone: deleted by its
    /// pilot, or ended too long ago. Nothing was stored, and nothing will
    /// be under this identifier.
    Gone,
    /// The identifier is that of another account's track. Nothing was
    /// stored.
    Taken,
}

/// What a track kept of a batch.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveKept {
    /// The track as it now is.
    pub track: LiveTrack,
    /// Whether the batch began the track.
    pub created: bool,
    /// Fixes that were added.
    pub stored: u32,
    /// Fixes the track already held: one for the same second.
    pub known: u32,
    /// Fixes that were new and found no room.
    pub full: u32,
}

/// What became of a call to
/// [`Store::delete_live_track`](crate::Store::delete_live_track).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveRemoval {
    /// The account's track was removed with its fixes. Its identifier
    /// takes nothing more.
    Removed,
    /// The account had removed the track, or its identifier, before.
    AlreadyRemoved,
    /// Nothing was known under the identifier. It is the account's now
    /// and takes nothing: fixes that are still on their way will not
    /// begin a track.
    Retired,
    /// Nothing was known under the identifier, and the call was not
    /// allowed to take it out of use.
    NotRetired,
    /// The identifier is another account's.
    NotYours,
}

/// What a track holds, as far as taking in more fixes needs to know.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Held {
    pub(crate) visibility: LiveVisibility,
    pub(crate) landed: bool,
    pub(crate) started_at: Timestamp,
    pub(crate) latest: LiveFix,
    pub(crate) fixes: u32,
    pub(crate) last_seq: i64,
}

/// What a batch does to a track.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Merged {
    /// The fixes to add, with their numbers.
    pub(crate) stored: Vec<LiveFixAt>,
    /// Fixes of the batch the track already held.
    pub(crate) known: u32,
    /// New fixes of the batch that found no room.
    pub(crate) full: u32,
    /// The track after the batch.
    pub(crate) held: Held,
    /// Whether the track differs from what it was.
    pub(crate) changed: bool,
}

/// Refuses a batch no track can take.
pub(crate) fn check(batch: &LiveBatch, room: &LiveRoom) -> Result<(), StoreError> {
    if batch.id.contains('\0') || batch.account_id.contains('\0') {
        return Err(StoreError::Invalid(
            "a live batch whose identifiers cannot be identifiers".to_owned(),
        ));
    }
    if batch.fixes.is_empty() {
        return Err(StoreError::Invalid("a live batch without a fix".to_owned()));
    }
    let whole_seconds = batch
        .fixes
        .iter()
        .all(|fix| fix.time.timestamp_subsec_nanos() == 0);
    let in_order = batch
        .fixes
        .windows(2)
        .all(|pair| pair[0].time < pair[1].time);
    if !whole_seconds || !in_order {
        return Err(StoreError::Invalid(
            "live fixes that are not oldest first, one to a whole second".to_owned(),
        ));
    }
    if room.fixes_per_track == 0 {
        return Err(StoreError::Invalid(
            "a live track with room for no fix".to_owned(),
        ));
    }
    Ok(())
}

/// Works out what a batch does to a track that holds `before`, or begins
/// if there is none. `known` holds the times of the batch's fixes that
/// the track already has.
///
/// - A fix for a second the track already has a fix for changes nothing:
///   the first one received stays.
/// - New fixes are taken oldest first while there is room.
/// - Who may see the track and whether it landed is what the batch with
///   the newest fix says. A batch that brings a newer fix than the track
///   had replaces both. A batch whose newest fix is the track's newest
///   can only narrow who sees it and can only say that it landed, so two
///   batches for the same moment give the same track in either order. A
///   batch of older fixes, such as one that fills a gap, changes neither.
///   Only fixes the track holds afterwards count for this.
///
/// The batch must have passed [`check`].
pub(crate) fn merge(
    before: Option<&Held>,
    batch: &LiveBatch,
    known: &BTreeSet<Timestamp>,
    room: &LiveRoom,
) -> Merged {
    let have = before.map_or(0, |held| held.fixes);
    let mut left = room.fixes_per_track.saturating_sub(have);
    let mut seq = before.map_or(0, |held| held.last_seq);

    let mut stored = Vec::new();
    let (mut known_count, mut full) = (0_u32, 0_u32);
    // The newest fix of the batch that the track holds after it.
    let mut stated: Option<Timestamp> = None;
    for fix in &batch.fixes {
        if known.contains(&fix.time) {
            known_count += 1;
            stated = Some(fix.time);
        } else if left > 0 {
            left -= 1;
            seq += 1;
            stored.push(LiveFixAt { seq, fix: *fix });
            stated = Some(fix.time);
        } else {
            full += 1;
        }
    }

    let held = match before {
        Some(before) => {
            let mut held = before.clone();
            held.fixes += u32::try_from(stored.len()).unwrap_or(u32::MAX);
            held.last_seq = seq;
            if let Some(first) = stored.first() {
                held.started_at = held.started_at.min(first.fix.time);
            }
            if let Some(last) = stored.last()
                && last.fix.time > held.latest.time
            {
                held.latest = last.fix;
            }
            match stated {
                Some(stated) if stated > before.latest.time => {
                    held.visibility = batch.visibility;
                    held.landed = batch.landed;
                }
                Some(stated) if stated == before.latest.time => {
                    held.visibility = before.visibility.narrower(batch.visibility);
                    held.landed = before.landed || batch.landed;
                }
                _ => {}
            }
            held
        }
        None => {
            // `check` saw at least one fix and room for at least one, and
            // a track that does not exist knows none.
            let (first, last) = match (stored.first(), stored.last()) {
                (Some(first), Some(last)) => (first.fix, last.fix),
                _ => (batch.fixes[0], batch.fixes[0]),
            };
            Held {
                visibility: batch.visibility,
                landed: batch.landed,
                started_at: first.time,
                latest: last,
                fixes: u32::try_from(stored.len()).unwrap_or(u32::MAX),
                last_seq: seq,
            }
        }
    };
    let changed = before != Some(&held);
    Merged {
        stored,
        known: known_count,
        full,
        held,
        changed,
    }
}

impl Held {
    /// The track these values belong to, as the store hands it out.
    pub(crate) fn track(&self, id: &str, account_id: &str, handle: &str, name: &str) -> LiveTrack {
        LiveTrack {
            id: id.to_owned(),
            account_id: account_id.to_owned(),
            pilot_handle: handle.to_owned(),
            pilot_name: name.to_owned(),
            visibility: self.visibility,
            landed: self.landed,
            started_at: self.started_at,
            latest: self.latest,
            fixes: self.fixes,
            cursor: self.last_seq,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};

    use super::*;

    fn at(second: i64) -> Timestamp {
        Utc.with_ymd_and_hms(2026, 11, 18, 18, 0, 0).unwrap() + Duration::seconds(second)
    }

    fn fix(second: i64) -> LiveFix {
        LiveFix {
            time: at(second),
            position: GeoPoint::new(47.66, 11.8).unwrap(),
            altitude_m: 1600,
            ground_speed_ms: None,
            climb_ms: None,
            course_deg: None,
        }
    }

    fn batch(seconds: &[i64], visibility: LiveVisibility, landed: bool) -> LiveBatch {
        LiveBatch {
            id: "track".to_owned(),
            account_id: "account".to_owned(),
            visibility,
            landed,
            fixes: seconds.iter().map(|second| fix(*second)).collect(),
        }
    }

    fn room(fixes_per_track: u32) -> LiveRoom {
        LiveRoom {
            fixes_per_track,
            may_create: true,
            expired_before: at(-3600),
            now: at(100),
        }
    }

    /// Applies batches one after another, as a store does, and returns
    /// the track and the times it holds.
    fn apply(batches: &[&LiveBatch], cap: u32) -> (Held, BTreeSet<Timestamp>) {
        let mut held: Option<Held> = None;
        let mut times = BTreeSet::new();
        for batch in batches {
            let known = batch
                .fixes
                .iter()
                .map(|fix| fix.time)
                .filter(|time| times.contains(time))
                .collect();
            let merged = merge(held.as_ref(), batch, &known, &room(cap));
            times.extend(merged.stored.iter().map(|at| at.fix.time));
            held = Some(merged.held);
        }
        (held.unwrap(), times)
    }

    #[test]
    fn the_narrower_of_two_settings_is_the_one_with_the_link() {
        use LiveVisibility::{Link, Public};
        assert_eq!(Public.narrower(Public), Public);
        assert_eq!(Public.narrower(Link), Link);
        assert_eq!(Link.narrower(Public), Link);
        assert_eq!(LiveVisibility::from_name("public"), Some(Public));
        assert_eq!(LiveVisibility::from_name("private"), None);
    }

    #[test]
    fn a_first_batch_begins_the_track_and_numbers_its_fixes() {
        let first = batch(&[10, 11, 12], LiveVisibility::Public, false);
        let merged = merge(None, &first, &BTreeSet::new(), &room(100));

        assert_eq!(merged.stored.len(), 3);
        let seqs: Vec<i64> = merged.stored.iter().map(|at| at.seq).collect();
        assert_eq!(seqs, [1, 2, 3]);
        assert_eq!((merged.known, merged.full), (0, 0));
        assert!(merged.changed);
        assert_eq!(merged.held.started_at, at(10));
        assert_eq!(merged.held.latest, fix(12));
        assert_eq!(merged.held.fixes, 3);
        assert_eq!(merged.held.last_seq, 3);
    }

    #[test]
    fn the_same_batch_again_changes_nothing() {
        let first = batch(&[10, 11, 12], LiveVisibility::Public, false);
        let begun = merge(None, &first, &BTreeSet::new(), &room(100));
        let known = first.fixes.iter().map(|fix| fix.time).collect();

        let again = merge(Some(&begun.held), &first, &known, &room(100));

        assert!(again.stored.is_empty());
        assert_eq!((again.known, again.full), (3, 0));
        assert!(!again.changed);
        assert_eq!(again.held, begun.held);
    }

    #[test]
    fn a_gap_filled_late_moves_the_start_and_not_the_newest_fix() {
        let newest = batch(&[50, 51], LiveVisibility::Public, true);
        let gap = batch(&[5, 6], LiveVisibility::Link, false);

        let (held, times) = apply(&[&newest, &gap], 100);

        assert_eq!(times.len(), 4);
        assert_eq!(held.started_at, at(5));
        assert_eq!(held.latest, fix(51));
        assert_eq!(held.last_seq, 4);
        // The late batch says nothing about the track as it is now.
        assert_eq!(held.visibility, LiveVisibility::Public);
        assert!(held.landed);
    }

    #[test]
    fn what_a_track_says_of_itself_does_not_depend_on_the_order_of_arrival() {
        use LiveVisibility::{Link, Public};
        let batches = [
            batch(&[1, 2, 3], Public, false),
            batch(&[4, 5], Link, false),
            // Two batches whose newest fix is the same moment.
            batch(&[6, 7, 8], Public, false),
            batch(&[8], Public, true),
            batch(&[0], Link, true),
        ];
        let orders: [[usize; 5]; 6] = [
            [0, 1, 2, 3, 4],
            [4, 3, 2, 1, 0],
            [3, 0, 4, 2, 1],
            [2, 3, 1, 0, 4],
            [1, 4, 0, 3, 2],
            [3, 2, 4, 1, 0],
        ];
        for order in orders {
            let ordered: Vec<&LiveBatch> = order.iter().map(|index| &batches[*index]).collect();
            let (held, times) = apply(&ordered, 100);
            assert_eq!(times.len(), 9, "{order:?}");
            assert_eq!(held.fixes, 9, "{order:?}");
            assert_eq!(held.started_at, at(0), "{order:?}");
            assert_eq!(held.latest, fix(8), "{order:?}");
            // The newest moment is second 8, and of the two batches that
            // end there one says landed.
            assert_eq!(held.visibility, Public, "{order:?}");
            assert!(held.landed, "{order:?}");
        }

        // At the same moment the narrower setting holds, whichever came
        // first, and a newer fix opens the track again.
        let public = batch(&[8], Public, false);
        let link = batch(&[8], Link, false);
        assert_eq!(apply(&[&public, &link], 100).0.visibility, Link);
        assert_eq!(apply(&[&link, &public], 100).0.visibility, Link);
        let later = batch(&[9], Public, false);
        assert_eq!(apply(&[&link, &later], 100).0.visibility, Public);
        // And a glider that flies on after a landing is flying.
        let landed = batch(&[8], Public, true);
        assert!(!apply(&[&landed, &later], 100).0.landed);
        assert!(apply(&[&later, &landed], 100).0.latest == fix(9));
        assert!(!apply(&[&later, &landed], 100).0.landed);
    }

    #[test]
    fn a_full_track_takes_the_oldest_new_fixes_and_no_more() {
        let first = batch(&[1, 2, 3], LiveVisibility::Public, false);
        let begun = merge(None, &first, &BTreeSet::new(), &room(5));
        // One fix it has, four it has not, and room for two.
        let more = batch(&[3, 4, 5, 6, 7], LiveVisibility::Link, true);
        let known = [at(3)].into_iter().collect();

        let merged = merge(Some(&begun.held), &more, &known, &room(5));

        let times: Vec<Timestamp> = merged.stored.iter().map(|at| at.fix.time).collect();
        assert_eq!(times, [at(4), at(5)]);
        assert_eq!((merged.known, merged.full), (1, 2));
        assert_eq!(merged.held.fixes, 5);
        assert_eq!(merged.held.latest, fix(5));
        // The batch's word counts as of the newest fix the track took.
        assert_eq!(merged.held.visibility, LiveVisibility::Link);
        assert!(merged.held.landed);

        // Nothing fits any more: nothing changes, whatever the batch says.
        let late = batch(&[8, 9], LiveVisibility::Public, false);
        let merged = merge(Some(&merged.held), &late, &BTreeSet::new(), &room(5));
        assert!(merged.stored.is_empty());
        assert_eq!((merged.known, merged.full), (0, 2));
        assert!(!merged.changed);
    }

    #[test]
    fn a_batch_no_track_can_take_is_refused() {
        let good = batch(&[1, 2], LiveVisibility::Link, false);
        assert_eq!(check(&good, &room(10)), Ok(()));

        let refused = |batch: &LiveBatch, room: &LiveRoom| {
            assert!(matches!(check(batch, room), Err(StoreError::Invalid(_))));
        };
        refused(&batch(&[], LiveVisibility::Link, false), &room(10));
        refused(&batch(&[2, 1], LiveVisibility::Link, false), &room(10));
        refused(&batch(&[1, 1], LiveVisibility::Link, false), &room(10));
        refused(&good, &room(0));
        let mut fraction = good.clone();
        fraction.fixes[0].time += Duration::milliseconds(500);
        refused(&fraction, &room(10));
        let mut zero_byte = good.clone();
        zero_byte.id.push('\0');
        refused(&zero_byte, &room(10));
    }
}
