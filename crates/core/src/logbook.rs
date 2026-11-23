//! The pilot's flights on this device.
//!
//! Every imported flight file is kept exactly as it was received, as
//! `flights/<id>.igc` in the core's storage, so nothing a recorder wrote
//! (its signature included) is lost. `logbook-index.json` next to it holds
//! what the logbook list shows for each flight, so opening the app does not
//! mean reading every file again. The index is a cache: it is rebuilt from
//! the files whenever it is missing, out of date or disagrees with them.
//!
//! The logbook reaches its files only through [`Storage`], so it is the
//! same on a phone's disk and in a browser.
//!
//! A flight's identity is a hash of its fixes ([`venti_flight::identity`]).
//! The same recording imported twice, or downloaded from two places with
//! different headers, is one flight. The server computes the same identity
//! for an uploaded file, so a flight imported on two devices is one record
//! there too.
//!
//! A flight cut into pieces is one flight ([`crate::flight_pieces`]). A
//! recording that takes over from one in the logbook, or that one in the
//! logbook takes over from, is joined with it on import: the joined file
//! is stored, then the pieces' files are removed, then the index is
//! written. A crash on the way leaves a piece beside its joined flight at
//! worst, never a loss, and opening the logbook clears that piece. A piece
//! imported again is the joined flight, already there.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use venti_flight::habits::{self, FlightHabits};
use venti_flight::igc::{self, IgcFile};
use venti_flight::{analysis, summarise, track};

use crate::flight_pieces;
use crate::storage::Storage;
use crate::time_zones;

/// The folder the flight files are in.
const FLIGHTS_FOLDER: &str = "flights";

/// The file the index is kept in.
const INDEX_FILE: &str = "logbook-index.json";

/// Bump when [`Summary`] changes shape or meaning; older indexes are rebuilt.
const INDEX_VERSION: u32 = 4;

/// Bump when the rules of [`venti_flight::habits`] change what a figure
/// means. Figures kept under another version count as not yet worked out
/// and are worked out again the next time they are asked for, without the
/// whole index being rebuilt.
const HABITS_VERSION: u32 = 2;

/// The most points an outline keeps. Enough for a recognisable thumbnail and
/// an overview map, small enough to hold thousands of flights in memory.
const OUTLINE_POINTS: usize = 96;

/// What can go wrong in the logbook.
#[derive(Debug)]
pub(crate) enum LogbookError {
    /// The text is not a usable flight file.
    Invalid(igc::IgcError),
    /// No flight with this identifier is stored.
    Unknown,
    /// Reading or writing the storage failed.
    Storage(io::Error),
}

impl From<io::Error> for LogbookError {
    fn from(error: io::Error) -> Self {
        Self::Storage(error)
    }
}

/// What the logbook list shows for one flight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Summary {
    pub start_unix_s: i64,
    pub duration_s: i64,
    pub track_length_m: f64,
    pub max_altitude_m: i32,
    pub max_climb_ms: f64,
    pub pilot: Option<String>,
    pub glider: Option<String>,
    /// The simplified track as latitude and longitude pairs.
    pub outline: Vec<[f64; 2]>,
    /// How the flight scores as a cross-country flight.
    pub score: Option<StoredScore>,
    /// How the flight was flown, in the figures of
    /// [`venti_flight::habits`]. Missing from an index written before they
    /// were kept, and then worked out the first time they are asked for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub habits: Option<StoredHabits>,
    /// The identities of the recordings the flight was joined from, in
    /// order ([`flight_pieces::pieces`]); empty for a flight recorded in
    /// one piece, and missing from an index written before flights were
    /// joined, when none was.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pieces: Vec<String>,
    /// The zone the file declares for its recorder's clock
    /// ([`time_zones::declared_zone`]). Missing from an index written
    /// before it was kept: such a flight goes by the zone of its take-off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_zone: Option<String>,
}

impl Summary {
    /// The zone the flight was flown in: the one its file declares, else
    /// that of where it took off.
    pub fn time_zone(&self) -> Option<String> {
        time_zones::flight_zone(
            self.declared_zone.as_deref(),
            self.outline
                .first()
                .map(|&[lat, lon]| crate::Coordinate { lat, lon }),
        )
    }

    /// What the clock where the flight was flown showed at its start.
    pub fn local_start(&self) -> Option<chrono::NaiveDateTime> {
        time_zones::local_time(self.time_zone().as_deref(), self.start_unix_s)
    }
}

/// What importing a flight file did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Imported {
    /// The flight's identifier: the joined flight's, when it was joined.
    pub id: String,
    /// Whether the logbook already held the flight, or a flight it is a
    /// piece of. Nothing was stored then.
    pub already: bool,
    /// The flights of the logbook it was joined with, oldest first. They
    /// are gone from the logbook: the joined flight holds them.
    pub joined: Vec<String>,
}

/// A flight's habits as the index keeps them: the figures of
/// [`FlightHabits`], with the version of the rules they were counted by.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct StoredHabits {
    pub version: u32,
    pub thermals_left: u32,
    pub thermals_right: u32,
    pub thermals_both_ways: u32,
    pub circling_left_s: i64,
    pub circling_right_s: i64,
    pub thermals_measured: u32,
    pub thermals_ended_weak: u32,
    pub after_the_top_s: i64,
    /// Kept since version 2 of the rules; missing from figures kept before.
    #[serde(default)]
    pub below_the_top_m: i64,
    pub height_gained_m: i64,
    pub thermalling_s: i64,
    pub best_climb_ms: Option<f64>,
    pub glides: u32,
    pub glide_distance_m: f64,
    pub glide_height_lost_m: i64,
    pub gliding_s: i64,
    pub airtime_s: i64,
}

impl From<FlightHabits> for StoredHabits {
    fn from(habits: FlightHabits) -> Self {
        Self {
            version: HABITS_VERSION,
            thermals_left: habits.thermals_left,
            thermals_right: habits.thermals_right,
            thermals_both_ways: habits.thermals_both_ways,
            circling_left_s: habits.circling_left_s,
            circling_right_s: habits.circling_right_s,
            thermals_measured: habits.thermals_measured,
            thermals_ended_weak: habits.thermals_ended_weak,
            after_the_top_s: habits.after_the_top_s,
            below_the_top_m: habits.below_the_top_m,
            height_gained_m: habits.height_gained_m,
            thermalling_s: habits.thermalling_s,
            best_climb_ms: habits.best_climb_ms,
            glides: habits.glides,
            glide_distance_m: habits.glide_distance_m,
            glide_height_lost_m: habits.glide_height_lost_m,
            gliding_s: habits.gliding_s,
            airtime_s: habits.airtime_s,
        }
    }
}

impl StoredHabits {
    /// The figures, if they were counted by the rules of this version.
    fn current(&self) -> Option<FlightHabits> {
        (self.version == HABITS_VERSION).then_some(FlightHabits {
            thermals_left: self.thermals_left,
            thermals_right: self.thermals_right,
            thermals_both_ways: self.thermals_both_ways,
            circling_left_s: self.circling_left_s,
            circling_right_s: self.circling_right_s,
            thermals_measured: self.thermals_measured,
            thermals_ended_weak: self.thermals_ended_weak,
            after_the_top_s: self.after_the_top_s,
            below_the_top_m: self.below_the_top_m,
            height_gained_m: self.height_gained_m,
            thermalling_s: self.thermalling_s,
            best_climb_ms: self.best_climb_ms,
            glides: self.glides,
            glide_distance_m: self.glide_distance_m,
            glide_height_lost_m: self.glide_height_lost_m,
            gliding_s: self.gliding_s,
            airtime_s: self.airtime_s,
        })
    }
}

/// One flight's habits, as [`Logbook::habits`] hands them out.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FlightWithHabits {
    pub id: String,
    pub start_unix_s: i64,
    pub habits: FlightHabits,
    /// How far from where it began the flight ended, metres, in a straight
    /// line between the first and last points of its outline, which are the
    /// track's own.
    pub ended_from_start_m: Option<f64>,
}

/// A flight's score as the index keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct StoredScore {
    pub kind: StoredKind,
    pub closed: bool,
    pub distance_m: f64,
    pub points: f64,
}

/// What a flight is scored as, as the index keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum StoredKind {
    FreeFlight,
    FreeTriangle,
    FaiTriangle,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Index {
    version: u32,
    flights: BTreeMap<String, Summary>,
}

/// The stored flights and their index.
#[derive(Debug)]
pub(crate) struct Logbook {
    storage: Arc<dyn Storage>,
    index: Mutex<Index>,
}

impl Logbook {
    /// Opens the logbook kept in `storage` and brings the index in line
    /// with the flight files found there.
    pub fn open(storage: Arc<dyn Storage>) -> Result<Self, LogbookError> {
        let logbook = Self {
            storage,
            index: Mutex::new(Index::default()),
        };

        let mut index = logbook.read_index()?;
        let outdated = index.version != INDEX_VERSION;
        index.version = INDEX_VERSION;
        *logbook.lock() = index;

        logbook.reconcile(outdated)?;
        Ok(logbook)
    }

    /// Brings the index in line with the stored flight files again.
    ///
    /// Storage that fetches its files on request cannot hand over a file
    /// that is missing from the index the first time it is asked. Such a
    /// file is left out by [`open`](Self::open) and taken in by a refresh
    /// once the host has fetched it. On disk there is nothing to wait for
    /// and no reason to call this.
    pub fn refresh(&self) -> Result<(), LogbookError> {
        self.reconcile(false)
    }

    /// Forgets the flights whose files have gone, takes in the files the
    /// index does not know, and stores the index if any of that changed it
    /// or `changed` says it has to be stored anyway.
    fn reconcile(&self, mut changed: bool) -> Result<(), LogbookError> {
        let mut index = self.lock();

        let stored: BTreeSet<String> = self
            .storage
            .list(FLIGHTS_FOLDER)?
            .iter()
            .filter_map(|name| flight_id(name))
            .map(str::to_owned)
            .collect();

        let before = index.flights.len();
        index.flights.retain(|id, _| stored.contains(id));
        changed |= index.flights.len() != before;

        for id in stored {
            if index.flights.contains_key(&id) {
                continue;
            }
            // A stored file that no longer reads is left in place and out
            // of the list; it was valid when it was imported. So is one the
            // storage has yet to fetch, until the next refresh.
            if let Ok((_, summary)) = self.read_flight(&id).and_then(|text| read(&text)) {
                index.flights.insert(id, summary);
                changed = true;
            }
        }

        // A piece beside the flight it was joined into: a join that a
        // crash interrupted. The joined flight holds all of it.
        for id in held_by_others(&index) {
            index.flights.remove(&id);
            // If it cannot be removed now it is cleared again next time.
            let _ = self.storage.remove(&flight_file(&id));
            changed = true;
        }

        if changed {
            self.write_index(&index)?;
        }
        Ok(())
    }

    /// Stores a flight file. Returns the flight's identifier and whether it
    /// was already in the logbook.
    pub fn import(&self, igc_text: &str) -> Result<(String, bool), LogbookError> {
        self.import_joining(igc_text)
            .map(|imported| (imported.id, imported.already))
    }

    /// Stores a flight file, joined with the flights of the logbook it is
    /// a piece of ([`crate::flight_pieces`]), and says what it did.
    pub fn import_joining(&self, igc_text: &str) -> Result<Imported, LogbookError> {
        let (mut file, mut summary) = read(igc_text)?;
        let id = venti_flight::identity(&file.fixes);

        let mut index = self.lock();
        if let Some(holder) = holder(&index, &id, &summary.pieces) {
            return Ok(Imported {
                id: holder,
                already: true,
                joined: Vec::new(),
            });
        }

        let mut text = igc_text.to_owned();
        let mut joined: Vec<(i64, String)> = Vec::new();
        while let Some((other, other_text, other_file, other_first)) =
            self.piece_beside(&index, &file, &joined)
        {
            let (earlier, later, cut_s) = if other_first {
                let cut_s = cut_s(&other_file, &file);
                (other_text, text, cut_s)
            } else {
                let cut_s = cut_s(&file, &other_file);
                (text, other_text, cut_s)
            };
            text = flight_pieces::joined(&earlier, &later, cut_s);
            (file, summary) = read(&text)?;
            let start = other_file
                .fixes
                .first()
                .map_or(0, |fix| fix.time.timestamp());
            joined.push((start, other));
        }
        let id = venti_flight::identity(&file.fixes);
        if joined.is_empty() {
            self.storage.write(&flight_file(&id), text.as_bytes())?;
            index.flights.insert(id.clone(), summary);
            self.write_index(&index)?;
            return Ok(Imported {
                id,
                already: false,
                joined: Vec::new(),
            });
        }

        // The joined file first: until it is written the pieces are all
        // there is. Then the pieces, then the index.
        if !index.flights.contains_key(&id) {
            self.storage.write(&flight_file(&id), text.as_bytes())?;
        }
        joined.sort();
        for (_, piece) in &joined {
            index.flights.remove(piece);
            // A file left behind is cleared when the logbook next opens.
            let _ = self.storage.remove(&flight_file(piece));
        }
        index.flights.insert(id.clone(), summary);
        self.write_index(&index)?;
        Ok(Imported {
            id,
            already: false,
            joined: joined.into_iter().map(|(_, piece)| piece).collect(),
        })
    }

    /// A flight of the logbook that `file` takes over from or that takes
    /// over from `file`, other than those in `taken`: its identifier, its
    /// text, its file, and whether it is the earlier of the two.
    fn piece_beside(
        &self,
        index: &Index,
        file: &IgcFile,
        taken: &[(i64, String)],
    ) -> Option<(String, String, IgcFile, bool)> {
        let first = file.fixes.first()?.time.timestamp();
        let last = file.fixes.last()?.time.timestamp();
        // By the summaries first, which describe the flown part: for a
        // piece that is the whole of it, give or take the take-off rule.
        let near = |seconds: i64| (-NEAR_SLACK_S..=flight_pieces::LONGEST_CUT_S).contains(&seconds);
        for (id, summary) in &index.flights {
            if taken.iter().any(|(_, piece)| piece == id) {
                continue;
            }
            let ends = summary.start_unix_s + summary.duration_s;
            let before = near(first - ends);
            let after = near(summary.start_unix_s - last);
            if !before && !after {
                continue;
            }
            // A file the storage has yet to fetch is not joined now.
            let Ok(text) = self.read_flight(id) else {
                continue;
            };
            let Ok(other) = igc::parse(&text) else {
                continue;
            };
            if before && flight_pieces::continues(&other, file) {
                return Some((id.clone(), text, other, true));
            }
            if after && flight_pieces::continues(file, &other) {
                return Some((id.clone(), text, other, false));
            }
        }
        None
    }

    /// The text of a stored flight file.
    pub fn flight_text(&self, id: &str) -> Result<String, LogbookError> {
        if !self.lock().flights.contains_key(id) {
            return Err(LogbookError::Unknown);
        }
        self.read_flight(id)
    }

    /// Removes a flight and its file.
    pub fn delete(&self, id: &str) -> Result<(), LogbookError> {
        let mut index = self.lock();
        if index.flights.remove(id).is_none() {
            return Err(LogbookError::Unknown);
        }
        self.storage.remove(&flight_file(id))?;
        self.write_index(&index)?;
        Ok(())
    }

    /// Every flight with its summary, newest first.
    pub fn flights(&self) -> Vec<(String, Summary)> {
        let mut flights: Vec<(String, Summary)> = self
            .lock()
            .flights
            .iter()
            .map(|(id, summary)| (id.clone(), summary.clone()))
            .collect();
        flights.sort_by(|a, b| (b.1.start_unix_s, &b.0).cmp(&(a.1.start_unix_s, &a.0)));
        flights
    }

    /// The habits of every flight that has them, newest first.
    ///
    /// Flights imported before habits were kept have none in the index.
    /// Theirs are worked out here, from their files, and stored, so that
    /// this is done once for each. A flight whose file cannot be read now
    /// is left out; storage that fetches files on request is then asked
    /// for it, and the next call takes it in.
    pub fn habits(&self) -> Vec<FlightWithHabits> {
        let mut index = self.lock();
        let missing: Vec<String> = index
            .flights
            .iter()
            .filter(|(_, summary)| summary.habits.and_then(|kept| kept.current()).is_none())
            .map(|(id, _)| id.clone())
            .collect();
        let mut changed = false;
        for id in missing {
            let Ok(text) = self.read_flight(&id) else {
                continue;
            };
            let Ok(file) = igc::parse(&text) else {
                continue;
            };
            let analysis = analysis::analyse(&file.fixes);
            let figures = habits::habits(analysis::flown_part(&file.fixes), &analysis);
            if let Some(summary) = index.flights.get_mut(&id) {
                summary.habits = Some(figures.into());
                changed = true;
            }
        }
        // The figures are kept for the next time. If storing them fails
        // they are worked out again then.
        if changed {
            let _ = self.write_index(&index);
        }

        let mut flights: Vec<FlightWithHabits> = index
            .flights
            .iter()
            .filter_map(|(id, summary)| {
                Some(FlightWithHabits {
                    id: id.clone(),
                    start_unix_s: summary.start_unix_s,
                    habits: summary.habits?.current()?,
                    ended_from_start_m: ended_from_start_m(&summary.outline),
                })
            })
            .collect();
        flights.sort_by(|a, b| (b.start_unix_s, &b.id).cmp(&(a.start_unix_s, &a.id)));
        flights
    }

    fn lock(&self) -> MutexGuard<'_, Index> {
        // A panic while the lock was held cannot leave the index half
        // written: it is only replaced or changed by single inserts and
        // removals. Carry on with what is there.
        self.index
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The text of a flight file. A flight whose file has gone is no longer
    /// in the logbook, whatever the index says.
    fn read_flight(&self, id: &str) -> Result<String, LogbookError> {
        let bytes = self
            .storage
            .read(&flight_file(id))?
            .ok_or(LogbookError::Unknown)?;
        String::from_utf8(bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error).into())
    }

    /// The stored index, or an empty one if it is missing, unreadable or
    /// from another version.
    fn read_index(&self) -> Result<Index, LogbookError> {
        let bytes = match self.storage.read(INDEX_FILE) {
            // The index is there and only has to be fetched. Rebuilding it
            // from every flight file instead would be a waste.
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Err(error.into()),
            read => read.ok().flatten(),
        };
        Ok(bytes
            .and_then(|bytes| serde_json::from_slice::<Index>(&bytes).ok())
            .filter(|index| index.version == INDEX_VERSION)
            .unwrap_or_default())
    }

    fn write_index(&self, index: &Index) -> Result<(), LogbookError> {
        let bytes = serde_json::to_vec(index).map_err(io::Error::other)?;
        self.storage.write(INDEX_FILE, &bytes)?;
        Ok(())
    }
}

/// Parses a flight file and works out its logbook summary. The summary
/// describes the part of the recording that was flown; the file is kept
/// whole.
fn read(igc_text: &str) -> Result<(IgcFile, Summary), LogbookError> {
    let file = igc::parse(igc_text).map_err(LogbookError::Invalid)?;
    let flown = analysis::flown_part(&file.fixes);
    let summary = summarise(flown).ok_or(LogbookError::Invalid(igc::IgcError::NoFixes))?;
    let analysis = analysis::analyse(&file.fixes);
    let summary = Summary {
        start_unix_s: summary.start.timestamp(),
        duration_s: summary.duration_s(),
        track_length_m: summary.track_length_m,
        max_altitude_m: summary.max_altitude_m,
        max_climb_ms: summary.max_climb_ms,
        pilot: file.pilot.clone(),
        glider: file.glider_type.clone(),
        outline: track::outline(flown, OUTLINE_POINTS)
            .into_iter()
            .map(|point| [point.lat(), point.lon()])
            .collect(),
        score: analysis.score.as_ref().map(|score| StoredScore {
            kind: match score.kind {
                venti_flight::scoring::RouteKind::FreeFlight => StoredKind::FreeFlight,
                venti_flight::scoring::RouteKind::FreeTriangle => StoredKind::FreeTriangle,
                venti_flight::scoring::RouteKind::FaiTriangle => StoredKind::FaiTriangle,
            },
            closed: score.closed,
            distance_m: score.distance_m,
            points: score.points,
        }),
        habits: Some(habits::habits(flown, &analysis).into()),
        pieces: flight_pieces::pieces(igc_text),
        declared_zone: time_zones::declared_zone(igc_text),
    };
    Ok((file, summary))
}

/// How much sooner than a stored flight's flown part ends, or later than
/// it begins, a recording may begin or end and still be looked at as a
/// piece of it, seconds: the take-off rule places a take-off in the air a
/// few seconds after the first fix.
const NEAR_SLACK_S: i64 = 60;

/// The seconds from the last fix of `earlier` to the first of `later`.
fn cut_s(earlier: &IgcFile, later: &IgcFile) -> i64 {
    match (earlier.fixes.last(), later.fixes.first()) {
        (Some(last), Some(first)) => (first.time - last.time).num_seconds(),
        _ => 0,
    }
}

/// The recordings a flight is made of: its pieces, or itself.
fn made_of<'a>(id: &'a str, pieces: &'a [String]) -> Vec<&'a str> {
    if pieces.is_empty() {
        vec![id]
    } else {
        pieces.iter().map(String::as_str).collect()
    }
}

/// The stored flight that holds the whole of a flight with this identity
/// and these pieces: the flight itself, or one joined from more pieces.
fn holder(index: &Index, id: &str, pieces: &[String]) -> Option<String> {
    if index.flights.contains_key(id) {
        return Some(id.to_owned());
    }
    let ours = made_of(id, pieces);
    index
        .flights
        .iter()
        .filter(|(_, summary)| !summary.pieces.is_empty())
        .find(|(_, summary)| {
            ours.iter()
                .all(|piece| summary.pieces.iter().any(|p| p == piece))
        })
        .map(|(holder, _)| holder.clone())
}

/// The stored flights that a joined flight beside them holds the whole
/// of, with more besides.
fn held_by_others(index: &Index) -> Vec<String> {
    let joined: Vec<(&String, &Vec<String>)> = index
        .flights
        .iter()
        .filter(|(_, summary)| !summary.pieces.is_empty())
        .map(|(id, summary)| (id, &summary.pieces))
        .collect();
    if joined.is_empty() {
        return Vec::new();
    }
    index
        .flights
        .iter()
        .filter(|(id, summary)| {
            let ours = made_of(id, &summary.pieces);
            joined.iter().any(|&(other, pieces)| {
                other != *id
                    && pieces.len() > ours.len()
                    && ours.iter().all(|piece| pieces.iter().any(|p| p == piece))
            })
        })
        .map(|(id, _)| id.clone())
        .collect()
}

/// The straight distance between the first and last points of an outline,
/// metres.
fn ended_from_start_m(outline: &[[f64; 2]]) -> Option<f64> {
    let point = |[lat, lon]: [f64; 2]| venti_domain::GeoPoint::new(lat, lon).ok();
    let (first, last) = (point(*outline.first()?)?, point(*outline.last()?)?);
    Some(first.distance_m(last))
}

/// Whether the text has the form of a flight identifier. Identifiers arrive
/// from outside the core and become file names, so nothing else is accepted.
fn is_id(text: &str) -> bool {
    text.len() == 32
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// The name a flight's file is stored under.
fn flight_file(id: &str) -> String {
    format!("{FLIGHTS_FOLDER}/{id}.igc")
}

/// The flight a stored file belongs to, if it is a flight file at all.
fn flight_id(name: &str) -> Option<&str> {
    name.strip_prefix(FLIGHTS_FOLDER)?
        .strip_prefix('/')?
        .strip_suffix(".igc")
        .filter(|id| is_id(id))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use venti_flight::testing::synthetic_flight;

    use super::*;
    use crate::storage::testing::{Backing, on_every_storage};
    use crate::storage::{DiskStorage, StagedStorage};
    use crate::testing::TempFolder;

    fn sample() -> String {
        igc::write(&synthetic_flight())
    }

    /// The sample flight moved to another year, which makes it another flight.
    fn sample_in_2025() -> String {
        sample().replace("040726", "040725")
    }

    fn open(backing: &dyn Backing) -> Logbook {
        Logbook::open(backing.open()).unwrap()
    }

    #[test]
    fn an_imported_flight_is_listed_and_can_be_read_back() {
        on_every_storage(|backing| {
            let logbook = open(backing);

            let (id, already) = logbook.import(&sample()).unwrap();

            assert!(is_id(&id), "{id}");
            assert!(!already);
            let flights = logbook.flights();
            assert_eq!(flights.len(), 1);
            assert_eq!(flights[0].0, id);
            assert_eq!(flights[0].1.duration_s, 1744);
            assert_eq!(flights[0].1.max_altitude_m, 2117);
            assert_eq!(flights[0].1.glider.as_deref(), Some("Sample Wing"));
            assert!(flights[0].1.outline.len() <= OUTLINE_POINTS);
            assert_eq!(logbook.flight_text(&id).unwrap(), sample());
        });
    }

    #[test]
    fn the_same_recording_is_stored_once_whatever_its_headers_say() {
        on_every_storage(|backing| {
            let logbook = open(backing);
            let (id, _) = logbook.import(&sample()).unwrap();

            let renamed = sample().replace("Sample Pilot", "Somebody Else");
            assert_ne!(renamed, sample());
            assert_eq!(logbook.import(&renamed).unwrap(), (id, true));
            assert_eq!(logbook.flights().len(), 1);
        });
    }

    #[test]
    fn flights_are_listed_newest_first() {
        on_every_storage(|backing| {
            let logbook = open(backing);
            let (older, _) = logbook.import(&sample_in_2025()).unwrap();
            let (newer, _) = logbook.import(&sample()).unwrap();

            let ids: Vec<String> = logbook.flights().into_iter().map(|(id, _)| id).collect();
            assert_eq!(ids, [newer, older]);
        });
    }

    #[test]
    fn the_logbook_survives_closing_and_reopening() {
        on_every_storage(|backing| {
            let (id, _) = open(backing).import(&sample()).unwrap();

            let reopened = open(backing);
            assert_eq!(reopened.flights().len(), 1);
            assert_eq!(reopened.flights()[0].0, id);
            assert_eq!(reopened.flight_text(&id).unwrap(), sample());
        });
    }

    #[test]
    fn a_lost_or_damaged_index_is_rebuilt_from_the_flight_files() {
        on_every_storage(|backing| {
            let before = {
                let logbook = open(backing);
                logbook.import(&sample()).unwrap();
                logbook.import(&sample_in_2025()).unwrap();
                logbook.flights()
            };

            backing.open().remove(INDEX_FILE).unwrap();
            assert_eq!(open(backing).flights(), before);

            backing.open().write(INDEX_FILE, b"not json").unwrap();
            assert_eq!(open(backing).flights(), before);
        });
    }

    #[test]
    fn a_flight_file_removed_behind_the_logbooks_back_leaves_the_list() {
        on_every_storage(|backing| {
            let (id, _) = open(backing).import(&sample()).unwrap();

            backing.open().remove(&flight_file(&id)).unwrap();
            assert!(open(backing).flights().is_empty());
        });
    }

    #[test]
    fn a_flight_whose_file_goes_while_the_logbook_is_open_is_no_longer_in_it() {
        on_every_storage(|backing| {
            let logbook = open(backing);
            let (id, _) = logbook.import(&sample()).unwrap();

            backing.open().remove(&flight_file(&id)).unwrap();

            assert!(matches!(
                logbook.flight_text(&id),
                Err(LogbookError::Unknown)
            ));
        });
    }

    #[test]
    fn a_stored_file_that_is_not_a_flight_is_left_out_of_the_list() {
        on_every_storage(|backing| {
            let (id, _) = open(backing).import(&sample()).unwrap();
            let storage = backing.open();
            storage
                .write(&flight_file(&"0".repeat(32)), b"not a flight")
                .unwrap();
            storage.write("flights/notes.txt", b"hello").unwrap();
            storage.remove(INDEX_FILE).unwrap();

            let ids: Vec<String> = open(backing)
                .flights()
                .into_iter()
                .map(|(id, _)| id)
                .collect();
            assert_eq!(ids, [id]);
        });
    }

    #[test]
    fn a_deleted_flight_is_gone_from_the_list_and_the_storage() {
        on_every_storage(|backing| {
            let logbook = open(backing);
            let (id, _) = logbook.import(&sample()).unwrap();

            logbook.delete(&id).unwrap();

            assert!(logbook.flights().is_empty());
            assert!(matches!(
                logbook.flight_text(&id),
                Err(LogbookError::Unknown)
            ));
            assert!(matches!(logbook.delete(&id), Err(LogbookError::Unknown)));
            let left = backing.open().list(FLIGHTS_FOLDER).unwrap();
            assert!(left.is_empty(), "{left:?}");
            assert!(open(backing).flights().is_empty());
        });
    }

    #[test]
    fn text_that_is_not_a_flight_is_refused_and_nothing_is_stored() {
        on_every_storage(|backing| {
            let logbook = open(backing);

            assert!(matches!(
                logbook.import("hello"),
                Err(LogbookError::Invalid(igc::IgcError::NoDate))
            ));
            assert!(logbook.flights().is_empty());
            assert!(backing.open().list(FLIGHTS_FOLDER).unwrap().is_empty());
        });
    }

    #[test]
    fn an_identifier_that_could_leave_the_folder_is_never_looked_up() {
        on_every_storage(|backing| {
            let logbook = open(backing);
            logbook.import(&sample()).unwrap();

            for id in ["", "../../etc/passwd", "ABCDEF0123456789ABCDEF0123456789"] {
                assert!(!is_id(id));
                assert!(matches!(
                    logbook.flight_text(id),
                    Err(LogbookError::Unknown)
                ));
            }
        });
    }

    /// Phones already hold logbooks in this layout. It must not change
    /// without code that moves them.
    #[test]
    fn on_disk_a_flight_is_one_file_named_by_its_identifier_beside_the_index() {
        let folder = TempFolder::new();
        let storage = Arc::new(DiskStorage::open(folder.path()).unwrap());
        let (id, _) = Logbook::open(storage).unwrap().import(&sample()).unwrap();

        let file = folder.path().join("flights").join(format!("{id}.igc"));
        assert_eq!(fs::read_to_string(file).unwrap(), sample());
        let index = fs::read(folder.path().join("logbook-index.json")).unwrap();
        let index: serde_json::Value = serde_json::from_slice(&index).unwrap();
        assert_eq!(index["version"], INDEX_VERSION);
        assert_eq!(index["flights"][&id]["duration_s"], 1744);

        let mut left: Vec<_> = fs::read_dir(folder.path().join("flights"))
            .unwrap()
            .chain(fs::read_dir(folder.path()).unwrap())
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(
            left,
            [format!("{id}.igc"), "flights".into(), INDEX_FILE.into()]
        );
    }

    #[test]
    fn an_imported_flight_keeps_its_habits_in_the_index() {
        on_every_storage(|backing| {
            let (id, _) = open(backing).import(&sample()).unwrap();

            let flights = open(backing).flights();
            let kept = flights[0].1.habits.expect("worked out on import");
            assert_eq!(kept.version, HABITS_VERSION);
            assert_eq!(kept.thermals_right, 2);
            let habits = open(backing).habits();
            assert_eq!(habits.len(), 1);
            assert_eq!(habits[0].id, id);
            assert_eq!(Some(habits[0].habits), kept.current());
        });
    }

    /// The index as a version that kept no habits wrote it.
    fn without_habits(index: &[u8]) -> Vec<u8> {
        let mut index: serde_json::Value = serde_json::from_slice(index).unwrap();
        for summary in index["flights"].as_object_mut().unwrap().values_mut() {
            summary.as_object_mut().unwrap().remove("habits").unwrap();
        }
        serde_json::to_vec(&index).unwrap()
    }

    #[test]
    fn an_index_without_habits_opens_and_gets_them_the_first_time_they_are_asked_for() {
        on_every_storage(|backing| {
            let before = {
                let logbook = open(backing);
                logbook.import(&sample()).unwrap();
                logbook.import(&sample_in_2025()).unwrap();
                logbook.habits()
            };
            let storage = backing.open();
            let old = without_habits(&storage.read(INDEX_FILE).unwrap().unwrap());
            storage.write(INDEX_FILE, &old).unwrap();

            // It opens as it is: the list is there and nothing is rebuilt.
            let logbook = open(backing);
            assert_eq!(logbook.flights().len(), 2);
            assert!(logbook.flights().iter().all(|(_, s)| s.habits.is_none()));
            assert_eq!(backing.open().read(INDEX_FILE).unwrap().unwrap(), old);

            // Asked for, they are worked out from the files and kept.
            assert_eq!(logbook.habits(), before);
            let reopened = open(backing);
            assert!(reopened.flights().iter().all(|(_, s)| s.habits.is_some()));
            assert_eq!(reopened.habits(), before);
        });
    }

    #[test]
    fn habits_counted_by_older_rules_are_counted_again() {
        on_every_storage(|backing| {
            let before = {
                let logbook = open(backing);
                logbook.import(&sample()).unwrap();
                logbook.habits()
            };
            let storage = backing.open();
            let mut index: serde_json::Value =
                serde_json::from_slice(&storage.read(INDEX_FILE).unwrap().unwrap()).unwrap();
            // As version 1 wrote them: without the height below the top.
            for summary in index["flights"].as_object_mut().unwrap().values_mut() {
                let habits = summary["habits"].as_object_mut().unwrap();
                habits.insert("version".into(), 1.into());
                habits.insert("thermals_right".into(), 99.into());
                habits.remove("below_the_top_m").unwrap();
            }
            storage
                .write(INDEX_FILE, &serde_json::to_vec(&index).unwrap())
                .unwrap();

            // The index opens as it is, and the figures are counted again.
            let logbook = open(backing);
            assert_eq!(logbook.flights().len(), 1);
            assert_eq!(logbook.habits(), before);
            let kept = open(backing).flights()[0].1.habits.unwrap();
            assert_eq!(kept.version, HABITS_VERSION);
        });
    }

    #[test]
    fn a_flight_whose_file_cannot_be_read_is_left_out_of_the_habits() {
        on_every_storage(|backing| {
            let (id, _) = open(backing).import(&sample()).unwrap();
            let storage = backing.open();
            let old = without_habits(&storage.read(INDEX_FILE).unwrap().unwrap());
            storage.write(INDEX_FILE, &old).unwrap();
            let logbook = open(backing);
            storage.write(&flight_file(&id), b"not a flight").unwrap();

            assert!(logbook.habits().is_empty());
            assert_eq!(logbook.flights().len(), 1);
        });
    }

    /// The cut at fix 700 of the made-up recording, `cut_s` long.
    fn cut(cut_s: usize) -> (String, String) {
        crate::flight_pieces::testing::cut(700, cut_s)
    }

    #[test]
    fn a_flight_cut_by_ten_seconds_or_five_minutes_is_one_flight() {
        for cut_s in [10, 300] {
            on_every_storage(|backing| {
                let logbook = open(backing);
                let (first, second) = cut(cut_s);

                let (first_id, _) = logbook.import(&first).unwrap();
                let imported = logbook.import_joining(&second).unwrap();

                assert!(!imported.already);
                assert_eq!(imported.joined, std::slice::from_ref(&first_id));
                let flights = logbook.flights();
                assert_eq!(flights.len(), 1, "a cut of {cut_s} s");
                assert_eq!(flights[0].0, imported.id);
                // One file, in the order of its fixes, the gap a gap.
                let text = logbook.flight_text(&imported.id).unwrap();
                assert_eq!(
                    text,
                    crate::flight_pieces::joined(&first, &second, cut_s as i64 + 1)
                );
                let file = igc::parse(&text).unwrap();
                assert!(
                    file.fixes
                        .windows(2)
                        .all(|pair| pair[0].time < pair[1].time)
                );
                // The analysis is over the whole: the made-up flight's
                // half hour from its take-off to its landing.
                let whole = crate::flight_pieces::testing::recording();
                let (_, alone) = read(&igc::write(&whole)).unwrap();
                assert_eq!(flights[0].1.duration_s, alone.duration_s);
                assert_eq!(flights[0].1.max_altitude_m, alone.max_altitude_m);
                assert_eq!(flights[0].1.pieces.len(), 2);
                // The piece's own file has gone.
                let left = backing.open().list(FLIGHTS_FOLDER).unwrap();
                assert_eq!(left, [flight_file(&imported.id)]);
                assert!(matches!(
                    logbook.flight_text(&first_id),
                    Err(LogbookError::Unknown)
                ));
                // And it stays one when the logbook opens again.
                let reopened = open(backing).flights();
                assert_eq!(reopened.len(), 1);
                assert_eq!(reopened[0].0, imported.id);
            });
        }
    }

    #[test]
    fn pieces_imported_the_other_way_round_make_the_same_flight() {
        on_every_storage(|backing| {
            let (first, second) = cut(10);
            let in_order = {
                let logbook = Logbook::open(Arc::new(StagedStorage::default())).unwrap();
                logbook.import(&first).unwrap();
                logbook.import(&second).unwrap().0
            };

            let logbook = open(backing);
            let (second_id, _) = logbook.import(&second).unwrap();
            let imported = logbook.import_joining(&first).unwrap();

            assert_eq!(imported.id, in_order);
            assert_eq!(imported.joined, [second_id]);
            assert_eq!(logbook.flights().len(), 1);
        });
    }

    #[test]
    fn a_flight_cut_twice_is_one_flight_whatever_the_order_its_pieces_come_in() {
        let whole = crate::flight_pieces::testing::recording();
        let (first, rest) = crate::flight_pieces::testing::cut_file(&whole, 500, 10);
        let rest = igc::parse(&rest).unwrap();
        let (second, third) = crate::flight_pieces::testing::cut_file(&rest, 500, 30);
        let mut ids = BTreeSet::new();
        // Not the first and the last before the middle: those two would
        // be joined across the middle's ten minutes, which then stays a
        // flight of its own. On a phone the pieces come in their order.
        for order in [
            [&first, &second, &third],
            [&third, &second, &first],
            [&second, &first, &third],
            [&second, &third, &first],
        ] {
            let logbook = Logbook::open(Arc::new(StagedStorage::default())).unwrap();
            for piece in order {
                logbook.import(piece).unwrap();
            }
            let flights = logbook.flights();
            assert_eq!(flights.len(), 1);
            assert_eq!(flights[0].1.pieces.len(), 3);
            ids.insert(flights[0].0.clone());
        }
        // The same file every time.
        assert_eq!(ids.len(), 1, "{ids:?}");
    }

    #[test]
    fn a_second_flight_after_the_landing_stays_a_flight_of_its_own() {
        on_every_storage(|backing| {
            let logbook = open(backing);
            let first = crate::flight_pieces::testing::recording();
            // Off again a minute after the first recording ended, from
            // where it came down.
            let landed = first.fixes.last().unwrap().time;
            let shift = (landed - first.fixes[0].time).num_seconds() + 60;
            let second = crate::flight_pieces::testing::later_by(&first, shift);

            logbook.import(&igc::write(&first)).unwrap();
            let imported = logbook.import_joining(&igc::write(&second)).unwrap();

            assert!(imported.joined.is_empty());
            assert_eq!(logbook.flights().len(), 2);
        });
    }

    #[test]
    fn a_piece_imported_again_is_the_joined_flight_already_there() {
        on_every_storage(|backing| {
            let logbook = open(backing);
            let (first, second) = cut(10);
            logbook.import(&first).unwrap();
            let (joined, _) = logbook.import(&second).unwrap();

            // The recording found again at the next launch, because the
            // app was ended before it was cleared.
            for piece in [&first, &second] {
                let again = logbook.import_joining(piece).unwrap();
                assert_eq!(
                    again,
                    Imported {
                        id: joined.clone(),
                        already: true,
                        joined: Vec::new(),
                    }
                );
            }
            let text = logbook.flight_text(&joined).unwrap();
            assert_eq!(logbook.import(&text).unwrap(), (joined, true));
            assert_eq!(logbook.flights().len(), 1);
        });
    }

    #[test]
    fn a_join_cut_short_by_a_crash_leaves_one_flight_when_the_logbook_opens() {
        on_every_storage(|backing| {
            let (first, second) = cut(10);
            let (first_id, _) = open(backing).import(&first).unwrap();
            // The joined file was written, and then the app was ended:
            // the piece's file and the index are as they were.
            let text = crate::flight_pieces::joined(&first, &second, 11);
            let joined = venti_flight::identity(&igc::parse(&text).unwrap().fixes);
            backing
                .open()
                .write(&flight_file(&joined), text.as_bytes())
                .unwrap();

            let logbook = open(backing);
            let ids: Vec<String> = logbook.flights().into_iter().map(|(id, _)| id).collect();
            assert_eq!(ids, std::slice::from_ref(&joined));
            assert_eq!(
                backing.open().list(FLIGHTS_FOLDER).unwrap(),
                [flight_file(&joined)]
            );
            assert!(!ids.contains(&first_id));
            // The recording still waiting is the flight already there.
            assert_eq!(logbook.import(&second).unwrap(), (joined, true));
        });
    }

    /// What a browser's store holds after two flights were imported.
    fn two_stored_flights() -> BTreeMap<String, Vec<u8>> {
        let staged = Arc::new(StagedStorage::default());
        let logbook = Logbook::open(Arc::clone(&staged) as Arc<dyn Storage>).unwrap();
        logbook.import(&sample()).unwrap();
        logbook.import(&sample_in_2025()).unwrap();
        staged
            .take_staged()
            .into_iter()
            .map(|file| (file.name, file.contents.expect("nothing was removed")))
            .collect()
    }

    fn would_block<T: std::fmt::Debug>(result: Result<T, LogbookError>) {
        match result {
            Err(LogbookError::Storage(error)) if error.kind() == io::ErrorKind::WouldBlock => {}
            other => panic!("expected a read that would block, got {other:?}"),
        }
    }

    #[test]
    fn a_hosted_logbook_asks_for_its_index_and_then_for_each_flight_it_shows() {
        let store = two_stored_flights();
        let staged = Arc::new(StagedStorage::new(store.keys().cloned()));
        let storage = || Arc::clone(&staged) as Arc<dyn Storage>;

        would_block(Logbook::open(storage()));
        assert_eq!(staged.take_wanted(), [INDEX_FILE]);

        staged.provide(INDEX_FILE, store.get(INDEX_FILE).cloned());
        let logbook = Logbook::open(storage()).unwrap();
        staged.release();
        // An index that agrees with the files is neither rebuilt nor
        // written again, and no flight file is fetched to open the list.
        assert!(staged.take_wanted().is_empty());
        assert!(staged.take_staged().is_empty());
        assert_eq!(logbook.flights().len(), 2);

        let id = logbook.flights()[0].0.clone();
        would_block(logbook.flight_text(&id));
        assert_eq!(staged.take_wanted(), [flight_file(&id)]);
        staged.provide(&flight_file(&id), store.get(&flight_file(&id)).cloned());
        assert_eq!(logbook.flight_text(&id).unwrap(), sample());
    }

    #[test]
    fn a_hosted_logbook_rebuilds_a_lost_index_a_few_flights_at_a_time() {
        let mut store = two_stored_flights();
        store.remove(INDEX_FILE);
        let staged = Arc::new(StagedStorage::new(store.keys().cloned()));

        // Nothing is in memory: the logbook opens empty and wants both
        // flight files.
        let logbook = Logbook::open(Arc::clone(&staged) as Arc<dyn Storage>).unwrap();
        assert!(logbook.flights().is_empty());
        let wanted = staged.take_wanted();
        assert_eq!(wanted.len(), 2);

        // With one fetched, that one is taken in and the other still wanted.
        staged.provide(&wanted[0], store.get(&wanted[0]).cloned());
        logbook.refresh().unwrap();
        staged.release();
        assert_eq!(logbook.flights().len(), 1);
        assert_eq!(staged.take_wanted(), [wanted[1].clone()]);

        staged.provide(&wanted[1], store.get(&wanted[1]).cloned());
        logbook.refresh().unwrap();
        staged.release();
        assert_eq!(logbook.flights().len(), 2);
        assert!(staged.take_wanted().is_empty());

        // The rebuilt index is the only thing to store.
        let staged_names: Vec<String> = staged
            .take_staged()
            .into_iter()
            .map(|file| file.name)
            .collect();
        assert_eq!(staged_names, [INDEX_FILE]);

        // Nothing changes when there is nothing to take in.
        logbook.refresh().unwrap();
        assert!(staged.take_staged().is_empty());
    }
}
