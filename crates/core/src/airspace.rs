//! Airspace in the app.
//!
//! The pilot brings airspace in as OpenAir files, the format XCTrack and
//! every other flight instrument reads. The core keeps the files as they
//! came, reads them with the airspace engine (`venti-airspace`) and builds
//! one set from all of them. A map asks the set for the airspaces in view;
//! the cockpit hands it to the flying session, which runs the engine's
//! warnings over every position. See `docs/airspace.md` for the rules,
//! every one of which errs toward warning.
//!
//! Everything here is advisory, and only as good as the files: an area the
//! pilot has no file for has no airspace as far as the app knows. The
//! screens say where their airspace comes from and how old it is.

use std::io;
use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};
use venti_airspace::openair::{self, Options, ProblemKind};
use venti_airspace::{
    AirspaceIndex, Config, EntryVia, Level, Policy, PolicyTable, PressureAltitude, Sample, Silence,
    Unknown, VerticalRelation, Warning, WarningEngine,
};
use venti_domain::{
    Airspace, AirspaceClass, AirspaceId, AirspaceType, BoundingBox, GeoPoint, HeightUnit, SourceId,
    VerticalLimit, VerticalReference,
};

use crate::{Bounds, Coordinate, Core, CoreError};

/// The folder the airspace files are stored in.
const FOLDER: &str = "airspace";

/// The largest file taken in. A whole country in OpenAir is under a
/// megabyte; Europe is a few.
const MAX_FILE_BYTES: usize = 32 * 1024 * 1024;

/// How the app treats an airspace, for a paraglider without a radio or a
/// transponder. The table and its sources are in `docs/airspace.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AirspacePolicy {
    /// Entering needs a clearance or equipment a paraglider normally
    /// lacks, or is forbidden while the airspace is active.
    Avoid,
    /// Not closed, but something there deserves attention.
    Caution,
    /// Nothing a paraglider needs permission for.
    Free,
}

/// How urgent a warning is, the least urgent first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, uniffi::Enum)]
pub enum AirspaceLevel {
    /// Within 2 km and 500 m of height.
    Near,
    /// Within 400 m and 100 m of height.
    VeryNear,
    /// On the present path and climb, inside within a minute.
    Entering,
    /// Inside, or possibly inside.
    Inside,
}

/// One airspace as a map draws it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AirspaceShape {
    /// Its identifier.
    pub id: String,
    /// Its name as published. May be empty.
    pub name: String,
    /// What it is, in words: "Class D control zone".
    pub kind: String,
    /// Its floor as published: "GND", "1000 ft AGL", "FL 95".
    pub lower: String,
    /// Its ceiling as published.
    pub upper: String,
    /// How the app treats it.
    pub policy: AirspacePolicy,
    /// Its outline, closed: the last point is the first again.
    pub outline: Vec<Coordinate>,
    /// Holes in it, each closed. Rare.
    pub holes: Vec<Vec<Coordinate>>,
}

/// One warning for the cockpit.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AirspaceWarning {
    /// The airspace's identifier, to acknowledge or ignore the warning.
    pub id: String,
    /// How urgent it is.
    pub level: AirspaceLevel,
    /// How the app treats the airspace.
    pub policy: AirspacePolicy,
    /// The warning in a line: "Inside CTR Innsbruck", "CTR Innsbruck in
    /// 30 s", "CTR Innsbruck 300 m NE".
    pub headline: String,
    /// The airspace in a line: "Class D control zone · GND to 5000 ft AMSL".
    pub detail: String,
    /// The airspace's name as published. May be empty.
    pub name: String,
    /// Its floor as published.
    pub lower: String,
    /// Its ceiling as published.
    pub upper: String,
    /// Whether the glider is inside the outline, at whatever height.
    pub inside_outline: bool,
    /// Metres over the ground to the outline: to the airspace from
    /// outside, to the nearest way out from inside.
    pub distance_m: f64,
    /// The direction to that point, degrees clockwise from north.
    pub bearing_deg: f64,
    /// Seconds until the glider is inside if course and climb stay as
    /// they are, when that is within reach.
    pub entry_seconds: Option<f64>,
    /// Set when the glider may in truth be clear of the airspace's height
    /// band, with what is missing to tell, in words. A screen must show
    /// it: "inside" then means "possibly inside".
    pub uncertain: Option<String>,
    /// Whether the pilot has silenced this warning.
    pub silenced: bool,
    /// Where the airspace comes from: the name of the pilot's file.
    pub source: String,
    /// The date of that data, seconds since 1970-01-01 UTC, if known.
    pub data_unix_s: Option<i64>,
}

/// What importing an airspace file did.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AirspaceImport {
    /// How many airspaces the file gave.
    pub airspaces: u32,
    /// How many airspaces in the file could not be read and were left
    /// out. The pilot will not be warned about them, and must be told.
    pub left_out: u32,
    /// How many times something could not be read and the cautious
    /// reading was taken instead. Those airspaces are kept, larger or
    /// more restricted than published.
    pub assumed: u32,
    /// The first problems, in words, each with its line.
    pub problems: Vec<String>,
}

/// One stored airspace file.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AirspaceFile {
    /// The name it is stored under, to delete it.
    pub stored_name: String,
    /// The name of the file the pilot chose.
    pub name: String,
    /// When it was brought in, seconds since 1970-01-01 UTC.
    pub imported_unix_s: Option<i64>,
    /// How many airspaces it gives.
    pub airspaces: u32,
    /// How many of its airspaces could not be read and are left out.
    pub left_out: u32,
}

/// Every airspace the pilot has files for, ready to be asked.
#[derive(Debug, uniffi::Object)]
pub struct AirspaceSet {
    pub(crate) index: AirspaceIndex,
    policy: PolicyTable,
    /// The file each source identifier stands for, and its date.
    sources: Vec<(SourceId, String, Option<DateTime<Utc>>)>,
}

#[uniffi::export]
impl AirspaceSet {
    /// How many airspaces the set holds.
    pub fn count(&self) -> u32 {
        u32::try_from(self.index.len()).unwrap_or(u32::MAX)
    }

    /// The airspaces that touch an area, for a map. Ask when the map has
    /// moved, not for every frame.
    pub fn in_area(&self, area: Bounds) -> Vec<AirspaceShape> {
        let Ok(area) = BoundingBox::new(
            area.south.clamp(-90.0, 90.0),
            area.west.clamp(-180.0, 180.0),
            area.north.clamp(-90.0, 90.0),
            area.east.clamp(-180.0, 180.0),
        ) else {
            return Vec::new();
        };
        self.index
            .in_box(&area, None)
            .into_iter()
            .map(|airspace| AirspaceShape {
                id: airspace.id.to_string(),
                name: airspace.name.clone(),
                kind: kind_in_words(airspace),
                lower: limit_in_words(&airspace.lower),
                upper: limit_in_words(&airspace.upper),
                policy: policy_view(self.policy.policy_for(airspace)),
                outline: ring(airspace.geometry.exterior()),
                holes: airspace
                    .geometry
                    .holes()
                    .iter()
                    .map(|hole| ring(hole))
                    .collect(),
            })
            .collect()
    }
}

impl AirspaceSet {
    /// The file an airspace came from, and the date of its data.
    fn origin(&self, airspace: &Airspace) -> (String, Option<i64>) {
        let known = self
            .sources
            .iter()
            .find(|(source, _, _)| source == airspace.id.source());
        (
            known.map_or_else(
                || airspace.id.source().to_string(),
                |(_, name, _)| name.clone(),
            ),
            airspace
                .validity
                .file_date
                .or(known.and_then(|(_, _, date)| *date))
                .map(|date| date.timestamp()),
        )
    }
}

#[uniffi::export]
impl Core {
    /// Takes in an airspace file in OpenAir format. `file_name` is the
    /// name of the file the pilot chose; `now_unix_s` the current time,
    /// kept as the age of the data. A file of the same name replaces the
    /// one brought in before.
    pub fn import_airspace(
        &self,
        file_name: String,
        bytes: Vec<u8>,
        now_unix_s: i64,
    ) -> Result<AirspaceImport, CoreError> {
        if bytes.len() > MAX_FILE_BYTES {
            return Err(CoreError::InvalidFile {
                message: "This file is too large to be an airspace file.".to_owned(),
            });
        }
        let slug = slug(&file_name);
        let read = openair::parse_bytes(&bytes, &Options::new(source_for(&slug)));
        if read.airspaces.is_empty() {
            return Err(CoreError::InvalidFile {
                message: match read.problems.first() {
                    Some(problem) if read.skipped() > 0 => format!(
                        "No airspace in this file could be read. Line {}: {}.",
                        problem.line, problem.message
                    ),
                    _ => "This is not an airspace file the app can read. It reads OpenAir files, which usually end in .txt.".to_owned(),
                },
            });
        }

        // A file of the same name, from any day, is replaced.
        for stored in self.stored_airspace_files()? {
            if stored_parts(&stored).is_some_and(|(_, old, _)| old == slug) {
                self.storage
                    .remove(&format!("{FOLDER}/{stored}"))
                    .map_err(storage_error)?;
            }
        }
        let day = DateTime::from_timestamp(now_unix_s, 0)
            .unwrap_or_default()
            .format("%Y%m%d");
        self.storage
            .write(&format!("{FOLDER}/{day}-{slug}.txt"), &bytes)
            .map_err(storage_error)?;
        self.forget_airspace();

        let count = |kind: ProblemKind| {
            let found = read
                .problems
                .iter()
                .filter(|problem| problem.kind == kind)
                .count();
            u32::try_from(found).unwrap_or(u32::MAX)
        };
        Ok(AirspaceImport {
            airspaces: u32::try_from(read.airspaces.len()).unwrap_or(u32::MAX),
            left_out: count(ProblemKind::AirspaceSkipped),
            assumed: count(ProblemKind::Assumed),
            problems: read
                .problems
                .iter()
                // What costs the pilot a warning comes first.
                .filter(|problem| problem.kind == ProblemKind::AirspaceSkipped)
                .chain(
                    read.problems
                        .iter()
                        .filter(|problem| problem.kind != ProblemKind::AirspaceSkipped),
                )
                .take(5)
                .map(|problem| match &problem.airspace {
                    Some(name) => format!("Line {} ({name}): {}.", problem.line, problem.message),
                    None => format!("Line {}: {}.", problem.line, problem.message),
                })
                .collect(),
        })
    }

    /// The airspace files the pilot has brought in, by name.
    pub fn airspace_files(&self) -> Result<Vec<AirspaceFile>, CoreError> {
        let mut files = Vec::new();
        for stored in self.stored_airspace_files()? {
            let Some((day, slug, name)) = stored_parts(&stored) else {
                continue;
            };
            let Some(bytes) = self
                .storage
                .read(&format!("{FOLDER}/{stored}"))
                .map_err(storage_error)?
            else {
                continue;
            };
            let read = openair::parse_bytes(&bytes, &Options::new(source_for(&slug)));
            files.push(AirspaceFile {
                stored_name: stored.clone(),
                name,
                imported_unix_s: day.map(|day| day.timestamp()),
                airspaces: u32::try_from(read.airspaces.len()).unwrap_or(u32::MAX),
                left_out: u32::try_from(read.skipped()).unwrap_or(u32::MAX),
            });
        }
        files.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(files)
    }

    /// Removes an airspace file. Its airspaces no longer warn.
    pub fn delete_airspace_file(&self, stored_name: String) -> Result<(), CoreError> {
        // Only names of this folder's own making are acted on.
        if stored_parts(&stored_name).is_none() {
            return Ok(());
        }
        self.storage
            .remove(&format!("{FOLDER}/{stored_name}"))
            .map_err(storage_error)?;
        self.forget_airspace();
        Ok(())
    }

    /// Every airspace of the pilot's files as one set, for the maps and
    /// for the cockpit. Built once and kept until the files change.
    pub fn airspace(&self) -> Result<Arc<AirspaceSet>, CoreError> {
        let mut kept = self
            .airspace
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(set) = kept.as_ref() {
            return Ok(Arc::clone(set));
        }
        let mut airspaces = Vec::new();
        let mut sources = Vec::new();
        for stored in self.stored_airspace_files()? {
            let Some((day, slug, name)) = stored_parts(&stored) else {
                continue;
            };
            let Some(bytes) = self
                .storage
                .read(&format!("{FOLDER}/{stored}"))
                .map_err(storage_error)?
            else {
                continue;
            };
            let source = source_for(&slug);
            let mut options = Options::new(source.clone());
            options.file_date = day;
            airspaces.extend(openair::parse_bytes(&bytes, &options).airspaces);
            sources.push((source, name, day));
        }
        let set = Arc::new(AirspaceSet {
            index: AirspaceIndex::new(airspaces),
            policy: PolicyTable::paraglider(),
            sources,
        });
        *kept = Some(Arc::clone(&set));
        Ok(set)
    }
}

impl Core {
    /// The names of the stored files, without the folder.
    fn stored_airspace_files(&self) -> Result<Vec<String>, CoreError> {
        let prefix = format!("{FOLDER}/");
        Ok(self
            .storage
            .list(FOLDER)
            .map_err(storage_error)?
            .into_iter()
            .filter_map(|name| name.strip_prefix(&prefix).map(str::to_owned))
            .collect())
    }

    fn forget_airspace(&self) {
        *self
            .airspace
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

fn storage_error(error: io::Error) -> CoreError {
    CoreError::Storage {
        message: format!("The airspace files could not be read or written: {error}."),
    }
}

/// A file name reduced to what is safe in a stored name and in a source
/// identifier: lower-case letters, digits and hyphens.
fn slug(file_name: &str) -> String {
    let stem = file_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(file_name)
        .trim();
    let stem = stem.rsplit_once('.').map_or(stem, |(stem, _)| stem);
    let mut slug = String::new();
    for letter in stem.chars().flat_map(char::to_lowercase) {
        if letter.is_ascii_lowercase() || letter.is_ascii_digit() {
            slug.push(letter);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
        if slug.len() >= 48 {
            break;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "airspace".to_owned()
    } else {
        slug.to_owned()
    }
}

/// The source identifier the airspaces of a file are filed under.
fn source_for(slug: &str) -> SourceId {
    SourceId::new(format!("file-{slug}")).unwrap_or_else(|_| {
        SourceId::new("file-airspace").expect("a fixed identifier of allowed letters")
    })
}

/// A stored name taken apart: the day it was brought in, its slug, and a
/// name to show. `None` for a file that is not of this module's making.
fn stored_parts(stored: &str) -> Option<(Option<DateTime<Utc>>, String, String)> {
    let stem = stored.strip_suffix(".txt")?;
    let (day, slug) = stem.split_once('-')?;
    let allowed =
        |letter: char| letter.is_ascii_lowercase() || letter.is_ascii_digit() || letter == '-';
    if slug.is_empty() || !slug.chars().all(allowed) || day.len() != 8 {
        return None;
    }
    let day = NaiveDate::parse_from_str(day, "%Y%m%d")
        .ok()
        .and_then(|day| day.and_hms_opt(12, 0, 0))
        .map(|noon| noon.and_utc());
    Some((day, slug.to_owned(), slug.to_owned()))
}

fn ring(points: &[GeoPoint]) -> Vec<Coordinate> {
    points
        .iter()
        .map(|point| Coordinate {
            lat: point.lat(),
            lon: point.lon(),
        })
        .collect()
}

fn policy_view(policy: Policy) -> AirspacePolicy {
    match policy {
        Policy::Avoid => AirspacePolicy::Avoid,
        Policy::Caution => AirspacePolicy::Caution,
        Policy::Free => AirspacePolicy::Free,
    }
}

/// A limit as published: "GND", "UNL", "FL 95", "5000 ft AMSL",
/// "300 m AGL".
fn limit_in_words(limit: &VerticalLimit) -> String {
    let value = limit.value();
    let number = if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    };
    match (limit.reference(), limit.unit()) {
        (VerticalReference::Ground, _) => "GND".to_owned(),
        (VerticalReference::Unlimited, _) => "UNL".to_owned(),
        (VerticalReference::Standard, _) | (_, HeightUnit::FlightLevel) => format!("FL {number}"),
        (VerticalReference::MeanSeaLevel, HeightUnit::Feet) => format!("{number} ft AMSL"),
        (VerticalReference::MeanSeaLevel, HeightUnit::Metres) => format!("{number} m AMSL"),
        (VerticalReference::AboveGround, HeightUnit::Feet) => format!("{number} ft AGL"),
        (VerticalReference::AboveGround, HeightUnit::Metres) => format!("{number} m AGL"),
    }
}

/// What an airspace is, in words: its class where it has one, and its type.
fn kind_in_words(airspace: &Airspace) -> String {
    let class = match airspace.class {
        AirspaceClass::A => Some("A"),
        AirspaceClass::B => Some("B"),
        AirspaceClass::C => Some("C"),
        AirspaceClass::D => Some("D"),
        AirspaceClass::E => Some("E"),
        AirspaceClass::F => Some("F"),
        AirspaceClass::G => Some("G"),
        AirspaceClass::Unclassified => None,
    };
    let kind = match &airspace.airspace_type {
        AirspaceType::Ctr => "control zone",
        AirspaceType::MilitaryCtr => "military control zone",
        AirspaceType::Tma => "terminal area",
        AirspaceType::Cta => "control area",
        AirspaceType::Atz => "aerodrome traffic zone",
        AirspaceType::Matz => "military aerodrome traffic zone",
        AirspaceType::Htz => "helicopter traffic zone",
        AirspaceType::Tiz => "traffic information zone",
        AirspaceType::Tia => "traffic information area",
        AirspaceType::Restricted => "restricted area",
        AirspaceType::Danger => "danger area",
        AirspaceType::Prohibited => "prohibited area",
        AirspaceType::GliderProhibited => "area closed to gliders",
        AirspaceType::Tmz => "transponder mandatory zone",
        AirspaceType::Rmz => "radio mandatory zone",
        AirspaceType::TransponderRecommended => "transponder recommended area",
        AirspaceType::TransponderSetting => "transponder code area",
        AirspaceType::Tra => "temporary reserved airspace",
        AirspaceType::Tsa => "temporary segregated area",
        AirspaceType::FeedingRoute => "feeding route",
        AirspaceType::TemporaryRestriction => "temporary restriction",
        AirspaceType::NotamArea => "NOTAM area",
        AirspaceType::GliderSector => "glider sector",
        AirspaceType::AerialSporting => "aerial sporting area",
        AirspaceType::OverflightRestriction => "overflight restriction",
        AirspaceType::Protected => "protected area",
        AirspaceType::MilitaryTrainingArea => "military training area",
        AirspaceType::MilitaryTrainingRoute => "military training route",
        AirspaceType::MilitaryRoute => "military route",
        AirspaceType::Adiz => "air defence identification zone",
        AirspaceType::Alert => "alert area",
        AirspaceType::Warning => "warning area",
        AirspaceType::Fir => "flight information region",
        AirspaceType::Uir => "upper flight information region",
        AirspaceType::FisSector => "flight information sector",
        AirspaceType::AccSector => "area control sector",
        AirspaceType::VfrSector => "VFR sector",
        AirspaceType::VfrRoute => "VFR route",
        AirspaceType::Airway => "airway",
        AirspaceType::Lta => "lower traffic area",
        AirspaceType::Uta => "upper traffic area",
        AirspaceType::Unspecified => "airspace",
        AirspaceType::Other(word) => word.as_str(),
    };
    match class {
        Some(class) => format!("Class {class} {kind}"),
        None => {
            let mut words = kind.to_owned();
            if let Some(first) = words.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            words
        }
    }
}

/// A direction as one of eight compass points.
fn compass(degrees: f64) -> &'static str {
    const POINTS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    POINTS[((degrees.rem_euclid(360.0) + 22.5) / 45.0) as usize % 8]
}

/// A distance in the cockpit: metres rounded to tens under a kilometre,
/// kilometres with one decimal beyond.
fn distance_in_words(metres: f64) -> String {
    if metres < 995.0 {
        format!("{:.0} m", (metres / 10.0).round() * 10.0)
    } else {
        format!("{:.1} km", metres / 1000.0)
    }
}

/// The warnings of a flying session.
#[derive(Debug)]
pub(crate) struct AirspaceWatch {
    set: Arc<AirspaceSet>,
    engine: WarningEngine,
    /// The warnings as of the latest position.
    warnings: Vec<AirspaceWarning>,
    /// The most urgent warning that became new and has not been sounded.
    alert: Option<AirspaceLevel>,
    /// When the engine was last fed, in sample time.
    fed_s: Option<f64>,
}

impl AirspaceWatch {
    pub(crate) fn new(set: Arc<AirspaceSet>) -> Self {
        Self {
            set,
            engine: WarningEngine::new(Config::default()),
            warnings: Vec::new(),
            alert: None,
            fed_s: None,
        }
    }

    /// Takes the state of the flight at a position, at most once a second.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn feed(
        &mut self,
        time_s: f64,
        position: GeoPoint,
        altitude_m: Option<f64>,
        pressure_altitude_m: Option<f64>,
        ground_speed_ms: Option<f64>,
        course_deg: Option<f64>,
        climb_ms: Option<f64>,
    ) {
        // Without a height nothing can be said; the last warnings stand.
        let Some(altitude_m) = altitude_m else {
            return;
        };
        if self
            .fed_s
            .is_some_and(|fed| (0.0..1.0).contains(&(time_s - fed)))
        {
            return;
        }
        self.fed_s = Some(time_s);
        let sample = Sample {
            time_s,
            position,
            altitude_m,
            pressure: pressure_altitude_m.map(PressureAltitude::standard),
            // No terrain model yet: limits above ground are taken as
            // reaching to the glider, and the warning says so.
            ground_elevation_m: None,
            ground_speed_ms,
            course_deg,
            climb_ms,
        };
        let Ok(found) = self.engine.update(&self.set.index, &sample) else {
            return;
        };
        for warning in &found {
            if warning.is_new && warning.silenced.is_none() {
                let level = level_view(warning.level);
                self.alert = Some(self.alert.map_or(level, |waiting| waiting.max(level)));
            }
        }
        self.warnings = found
            .iter()
            .map(|warning| view(&self.set, warning))
            .collect();
    }

    pub(crate) fn warnings(&self) -> Vec<AirspaceWarning> {
        self.warnings.clone()
    }

    pub(crate) fn take_alert(&mut self) -> Option<AirspaceLevel> {
        self.alert.take()
    }

    /// "I have seen it": quiet at its present level for some minutes.
    pub(crate) fn acknowledge(&mut self, id: &str, minutes: u32) {
        if let Ok(id) = id.parse::<AirspaceId>() {
            self.engine.acknowledge(
                &id,
                Silence::For {
                    seconds: f64::from(minutes) * 60.0,
                },
            );
            self.mark_silenced(&id, true);
        }
    }

    /// "This airspace does not apply to me today": quiet until midnight
    /// where the pilot is.
    pub(crate) fn ignore_today(&mut self, id: &str, utc_offset_s: i32) {
        if let Ok(id) = id.parse::<AirspaceId>() {
            self.engine.ignore(&id, Silence::RestOfDay { utc_offset_s });
            self.mark_silenced(&id, true);
        }
    }

    pub(crate) fn unsilence(&mut self, id: &str) {
        if let Ok(id) = id.parse::<AirspaceId>() {
            self.engine.unsilence(&id);
            self.mark_silenced(&id, false);
        }
    }

    /// Shows the pilot's choice at once, before the next position arrives.
    fn mark_silenced(&mut self, id: &AirspaceId, silenced: bool) {
        let id = id.to_string();
        for warning in &mut self.warnings {
            if warning.id == id {
                warning.silenced = silenced;
            }
        }
    }
}

fn level_view(level: Level) -> AirspaceLevel {
    match level {
        Level::Near => AirspaceLevel::Near,
        Level::VeryNear => AirspaceLevel::VeryNear,
        Level::Entering => AirspaceLevel::Entering,
        Level::Inside => AirspaceLevel::Inside,
    }
}

fn view(set: &AirspaceSet, warning: &Warning<'_>) -> AirspaceWarning {
    let airspace = warning.airspace;
    let name = if airspace.name.trim().is_empty() {
        "Unnamed airspace".to_owned()
    } else {
        airspace.name.clone()
    };
    let (lower, upper) = (
        limit_in_words(&airspace.lower),
        limit_in_words(&airspace.upper),
    );
    let headline = match (warning.level, warning.entry) {
        (Level::Inside, _) => format!("Inside {name}"),
        (Level::Entering, Some(entry)) => {
            let seconds = entry.seconds.max(0.0).round();
            match entry.via {
                EntryVia::Side => format!("{name} in {seconds:.0} s"),
                EntryVia::Floor => format!("Climbing into {name} in {seconds:.0} s"),
                EntryVia::Ceiling => format!("Sinking into {name} in {seconds:.0} s"),
            }
        }
        _ if warning.horizontal.inside => match warning.vertical.relation {
            // Under or over it: what matters is the height left.
            VerticalRelation::Below => {
                format!("{name} {:.0} m above", warning.vertical.separation_m)
            }
            VerticalRelation::Above => {
                format!("{name} {:.0} m below", warning.vertical.separation_m)
            }
            VerticalRelation::Inside => format!("Inside {name}"),
        },
        _ => format!(
            "{name} {} {}",
            distance_in_words(warning.horizontal.distance_m),
            compass(warning.horizontal.bearing_deg)
        ),
    };
    let missing = warning
        .vertical
        .floor
        .unknown
        .or(warning.vertical.ceiling.unknown);
    let uncertain = warning.uncertain().then(|| {
        match missing {
            Some(Unknown::GroundElevation) => {
                "Possibly clear: the app does not know the height of the ground here."
            }
            Some(Unknown::Pressure) => {
                "Possibly clear: no pressure sensor to place a flight level."
            }
            None => "Possibly clear: the airspace's height could not be placed exactly.",
        }
        .to_owned()
    });
    let (source, data_unix_s) = set.origin(airspace);
    AirspaceWarning {
        id: airspace.id.to_string(),
        level: level_view(warning.level),
        policy: policy_view(warning.policy),
        headline,
        detail: format!("{} · {lower} to {upper}", kind_in_words(airspace)),
        name,
        lower,
        upper,
        inside_outline: warning.horizontal.inside,
        distance_m: warning.horizontal.distance_m,
        bearing_deg: warning.horizontal.bearing_deg,
        entry_seconds: warning.entry.map(|entry| entry.seconds),
        uncertain,
        silenced: warning.silenced.is_some(),
        source,
        data_unix_s,
    }
}

#[cfg(test)]
mod tests {
    use venti_flight::igc;
    use venti_flight::testing::synthetic_flight;

    use super::*;
    use crate::testing::TempFolder;
    use crate::{FlightSession, Replay};

    const NOW: i64 = 1_795_000_000; // 18 November 2026

    /// A control zone of one nautical mile near the end of the sample
    /// flight, a danger area far from it, and an airspace that cannot be
    /// read because it has no shape.
    const FILE: &str = "\
* Test airspace for Venti
AC D
AY CTR
AN CTR TEST
AL GND
AH 9000ft AMSL
V X=47:42:00 N 011:44:00 E
DC 1

AC Q
AN ED-D TEST
AL 1000ft AGL
AH FL 95
DP 48:00:00 N 012:00:00 E
DP 48:00:00 N 012:10:00 E
DP 48:05:00 N 012:10:00 E
DP 48:05:00 N 012:00:00 E

AC C
AN NO SHAPE
AL GND
AH FL 100
";

    fn core(folder: &TempFolder) -> Arc<Core> {
        Core::new(folder.path().to_str().unwrap().to_owned()).unwrap()
    }

    #[test]
    fn a_file_is_taken_in_listed_replaced_and_removed() {
        let folder = TempFolder::new();
        let core = core(&folder);
        assert_eq!(core.airspace().unwrap().count(), 0);

        let import = core
            .import_airspace(
                "Test Zones 2026.txt".to_owned(),
                FILE.as_bytes().to_vec(),
                NOW,
            )
            .unwrap();
        assert_eq!(import.airspaces, 2);
        // The one that could not be read is counted and named first.
        assert_eq!(import.left_out, 1);
        assert!(
            import.problems[0].contains("NO SHAPE"),
            "{:?}",
            import.problems
        );

        let files = core.airspace_files().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "test-zones-2026");
        assert_eq!(files[0].stored_name, "20261118-test-zones-2026.txt");
        assert_eq!((files[0].airspaces, files[0].left_out), (2, 1));
        assert_eq!(core.airspace().unwrap().count(), 2);

        // The same file a day later replaces the first; another name adds.
        core.import_airspace(
            "test zones 2026.TXT".to_owned(),
            FILE.as_bytes().to_vec(),
            NOW + 86_400,
        )
        .unwrap();
        assert_eq!(core.airspace_files().unwrap().len(), 1);
        core.import_airspace("other.txt".to_owned(), FILE.as_bytes().to_vec(), NOW)
            .unwrap();
        assert_eq!(core.airspace_files().unwrap().len(), 2);
        assert_eq!(core.airspace().unwrap().count(), 4);

        // After a restart the files are still there.
        drop(core);
        let core = self::core(&folder);
        assert_eq!(core.airspace().unwrap().count(), 4);

        core.delete_airspace_file("20261118-other.txt".to_owned())
            .unwrap();
        core.delete_airspace_file("../logbook-index.json".to_owned())
            .unwrap();
        assert_eq!(core.airspace().unwrap().count(), 2);
    }

    #[test]
    fn files_that_hold_no_airspace_are_refused_in_words() {
        let folder = TempFolder::new();
        let core = core(&folder);
        for (bytes, expected) in [
            (b"hello".to_vec(), "not an airspace file"),
            (
                b"AC C\nAN NO SHAPE\nAL GND\nAH FL 100\n".to_vec(),
                "No airspace in this file could be read",
            ),
            (Vec::new(), "not an airspace file"),
        ] {
            let error = core
                .import_airspace("x.txt".to_owned(), bytes, NOW)
                .unwrap_err();
            assert!(matches!(error, CoreError::InvalidFile { .. }), "{error:?}");
            assert!(error.to_string().contains(expected), "{error}");
        }
        assert!(core.airspace_files().unwrap().is_empty());
    }

    #[test]
    fn a_map_gets_the_airspaces_in_view_in_words() {
        let folder = TempFolder::new();
        let core = core(&folder);
        core.import_airspace("zones.txt".to_owned(), FILE.as_bytes().to_vec(), NOW)
            .unwrap();
        let set = core.airspace().unwrap();

        let around_tegernsee = Bounds {
            south: 47.6,
            west: 11.6,
            north: 47.8,
            east: 11.9,
        };
        let shapes = set.in_area(around_tegernsee);
        assert_eq!(shapes.len(), 1);
        let zone = &shapes[0];
        assert_eq!(zone.name, "CTR TEST");
        assert_eq!(zone.kind, "Class D control zone");
        assert_eq!(
            (zone.lower.as_str(), zone.upper.as_str()),
            ("GND", "9000 ft AMSL")
        );
        assert_eq!(zone.policy, AirspacePolicy::Avoid);
        assert!(zone.outline.len() > 20);
        assert_eq!(zone.outline.first(), zone.outline.last());

        let everything = set.in_area(Bounds {
            south: 47.0,
            west: 11.0,
            north: 49.0,
            east: 13.0,
        });
        assert_eq!(everything.len(), 2);
        let danger = everything
            .iter()
            .find(|shape| shape.name == "ED-D TEST")
            .unwrap();
        assert_eq!(danger.kind, "Danger area");
        assert_eq!(
            (danger.lower.as_str(), danger.upper.as_str()),
            ("1000 ft AGL", "FL 95")
        );
    }

    #[test]
    fn the_cockpit_warns_as_the_glider_comes_near_and_can_be_quieted() {
        let folder = TempFolder::new();
        let core = core(&folder);
        core.import_airspace("zones.txt".to_owned(), FILE.as_bytes().to_vec(), NOW)
            .unwrap();
        let session = FlightSession::new(folder.path().to_str().unwrap().to_owned());
        session.use_airspace(Some(core.airspace().unwrap()));
        let replay = Replay::new(igc::write(&synthetic_flight())).unwrap();

        // At the take-off the zone is six kilometres away: nothing to say.
        replay.advance(session.clone(), 60.0);
        assert!(session.airspace_warnings().is_empty());
        assert_eq!(session.take_airspace_alert(), None);

        // Step through the flight until the zone speaks up.
        let mut first = None;
        while first.is_none() && replay.advance(session.clone(), 5.0) {
            first = session.airspace_warnings().into_iter().next();
        }
        let first = first.expect("the flight ends near the zone");
        assert_eq!(first.name, "CTR TEST");
        assert_eq!(first.level, AirspaceLevel::Near);
        assert_eq!(first.policy, AirspacePolicy::Avoid);
        assert_eq!(first.detail, "Class D control zone · GND to 9000 ft AMSL");
        assert!(
            first.headline.starts_with("CTR TEST "),
            "{}",
            first.headline
        );
        assert!(
            first.headline.ends_with("NW") || first.headline.ends_with('W'),
            "{}",
            first.headline
        );
        assert!(
            first.distance_m <= 2000.0 && first.distance_m > 1500.0,
            "{}",
            first.distance_m
        );
        assert!(!first.silenced);
        assert_eq!(first.source, "zones");
        assert!(first.data_unix_s.is_some());
        // Its floor is the ground and its ceiling an altitude: nothing
        // about the height is in doubt.
        assert_eq!(first.uncertain, None);
        // A new warning is to be sounded, once.
        assert_eq!(session.take_airspace_alert(), Some(AirspaceLevel::Near));
        assert_eq!(session.take_airspace_alert(), None);

        // Acknowledged, it stays on screen, marked, and is quiet.
        session.acknowledge_airspace(first.id.clone(), 10);
        assert!(session.airspace_warnings()[0].silenced);

        // To the end of the flight: closer, and more urgent levels speak
        // again although the pilot acknowledged "near".
        let mut closest = first.distance_m;
        let mut most_urgent = first.level;
        while replay.advance(session.clone(), 5.0) {
            if let Some(warning) = session.airspace_warnings().first() {
                closest = closest.min(warning.distance_m);
                most_urgent = most_urgent.max(warning.level);
            }
        }
        assert!(closest < 600.0, "{closest}");
        assert!(most_urgent >= AirspaceLevel::VeryNear, "{most_urgent:?}");
        assert!(session.take_airspace_alert().is_some());

        // Ignored for the day, it is quiet whatever happens; taking that
        // back makes it speak again.
        session.ignore_airspace_today(first.id.clone(), 3600);
        assert!(
            session
                .airspace_warnings()
                .iter()
                .all(|warning| warning.silenced)
        );
        session.unsilence_airspace(first.id.clone());
        assert!(
            session
                .airspace_warnings()
                .iter()
                .all(|warning| !warning.silenced)
        );

        // Without airspace there are no warnings.
        session.use_airspace(None);
        assert!(session.airspace_warnings().is_empty());
    }

    #[test]
    fn limits_and_kinds_are_worded_as_pilots_read_them() {
        let limit = |reference, value, unit| {
            limit_in_words(&VerticalLimit::new(reference, value, unit).unwrap())
        };
        assert_eq!(
            limit(VerticalReference::MeanSeaLevel, 4500.0, HeightUnit::Feet),
            "4500 ft AMSL"
        );
        assert_eq!(
            limit(VerticalReference::AboveGround, 300.0, HeightUnit::Metres),
            "300 m AGL"
        );
        assert_eq!(
            limit_in_words(&VerticalLimit::flight_level(95).unwrap()),
            "FL 95"
        );
        assert_eq!(distance_in_words(304.0), "300 m");
        assert_eq!(distance_in_words(1849.0), "1.8 km");
        assert_eq!(compass(315.0), "NW");
        assert_eq!(
            slug("My Airspace (DE+AT) 2026.txt"),
            "my-airspace-de-at-2026"
        );
        assert_eq!(slug("../../etc/passwd"), "passwd");
        assert_eq!(slug("ÄÖÜ.txt"), "airspace");
        assert!(stored_parts("20261118-zones.txt").is_some());
        assert!(stored_parts("../20261118-zones.txt").is_none());
        assert!(stored_parts("zones.txt").is_none());
    }
}
