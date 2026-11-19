//! Reading OpenAir airspace files.
//!
//! OpenAir is the text format pilots pass airspace around in. WinPilot
//! defined it in 1998; Naviter maintains the current specification, which
//! adds a type, a frequency and activation times to the original:
//! <https://github.com/naviter/seeyou_file_formats/blob/main/OpenAir_File_Format_Support.md>
//! (version 2.1.3 of 2026-11-04, read on 2026-11-19). The original WinPilot
//! page could not be reached that day, so what goes beyond Naviter's text
//! (the old two-letter classes, `AT`, `SP`, `SB`, `V Z=`) follows what
//! XCSoar's reader accepts. To verify against the original.
//!
//! A file is lines of text, each starting with a command:
//!
//! ```text
//! AC D                          class, begins a new airspace
//! AY CTR                        type
//! AN CTR Innsbruck              name
//! AF 120.100                    frequency, MHz
//! AG Innsbruck Tower            station on that frequency
//! AL GND                        floor
//! AH 5000ft AMSL                ceiling
//! V X=47:15:37 N 011:20:38 E    sets the centre for the lines below
//! V D=-                         arcs run counter-clockwise from here on
//! DP 47:18:00 N 011:10:00 E     a corner
//! DA 5, 270, 290                an arc: radius in nautical miles, from and to bearing
//! DB p1, p2                     an arc from one point to another
//! DC 5                          a whole circle of that radius
//! * anything                    a comment, also at the end of a line
//! ```
//!
//! # What this reader guarantees
//!
//! Real files are untidy and a file from the internet may be hostile. The
//! reader never panics and never works longer or uses more memory than the
//! size of the file justifies: see [`MAX_FILE_POINTS`].
//!
//! It never drops an airspace without saying so. Three things can happen to
//! a line it cannot use, and each is reported with its line number in
//! [`OpenAir::problems`]:
//!
//! - **A limit it cannot read** is replaced by the widest one: the floor by
//!   the ground, the ceiling by "unlimited". The airspace is kept and is
//!   larger than published, never smaller.
//! - **A shape it cannot read** costs the whole airspace, because half a
//!   shape is a wrong shape. This is the dangerous case: the pilot will not
//!   be warned about that airspace, and the app must say so.
//! - **Anything else** it does not know (a display hint, a newer attribute)
//!   is skipped and the airspace is kept.
//!
//! # Decisions
//!
//! - Circles and arcs become polygons that are never smaller than the true
//!   shape and at most [`ARC_MAX_ERROR_M`](crate::ARC_MAX_ERROR_M) larger.
//! - `DY`, an airway given as a centre line with a width, is not converted.
//!   Naviter advises against it, and no file read while writing this used
//!   it. An airspace that uses it is skipped and reported.
//! - `AA`, activation times, is kept as text in the record's hours remarks
//!   and not evaluated: every airspace counts as potentially active.
//!   `AA NONE` marks the airspace as active by NOTAM.
//! - `AX`, the transponder code, is read and not kept.
//! - OpenAir has no identifier for an airspace. The reader derives one from
//!   the name and the two limits, which stays the same when a later edition
//!   of the file moves a corner. `AI`, an identifier some exports carried
//!   for a while, is used when present.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use venti_domain::{
    Activity, Airspace, AirspaceClass, AirspaceId, AirspaceType, Circle, Frequency, GeoPoint,
    HeightUnit, MAX_RING_POINTS, Polygon, SourceId, Timestamp, Validity, VerticalLimit,
    VerticalReference,
};

use crate::geometry::{
    Arc, METRES_PER_NAUTICAL_MILE, bearing_deg, circle_ring, destination, doubled_signed_area,
};

/// Most polygon points one file may produce, over all its airspaces.
///
/// One arc line can stand for hundreds of points, so a small hostile file
/// could otherwise ask for gigabytes. The German and Austrian airspace of
/// 2026 together has about 60,000 points. Airspaces past the limit are
/// skipped and reported.
pub const MAX_FILE_POINTS: usize = 2_000_000;

/// Largest radius accepted for a circle or an arc, in nautical miles.
pub const MAX_RADIUS_NM: f64 = 500.0;

/// Most problems kept in [`OpenAir::problems`]. Further ones are counted in
/// [`OpenAir::problems_omitted`].
pub const MAX_PROBLEMS: usize = 1_000;

/// Longest name kept, in characters.
const MAX_NAME_CHARS: usize = 200;

/// What the caller knows about a file that the file does not say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The source the airspaces are filed under: the first half of every
    /// [`AirspaceId`].
    pub source: SourceId,
    /// The country the file covers, as an ISO 3166-1 alpha-2 code, if it
    /// covers one. OpenAir has no field for it.
    pub country: Option<String>,
    /// The date of the file.
    pub file_date: Option<Timestamp>,
    /// The AIRAC cycle of the file, if known.
    pub airac: Option<String>,
}

impl Options {
    /// Options with only a source.
    pub fn new(source: SourceId) -> Self {
        Self {
            source,
            country: None,
            file_date: None,
            airac: None,
        }
    }
}

/// What became of a line the reader could not use as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProblemKind {
    /// An airspace was left out. The pilot will not be warned about it.
    AirspaceSkipped,
    /// A limit could not be read and was replaced by the ground or by
    /// "unlimited". The airspace is kept, larger than published.
    LimitAssumed,
    /// A line was skipped. The airspace it belongs to is kept.
    LineIgnored,
}

/// One thing in a file the reader could not use as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The line it is about, counted from 1. For a skipped airspace, the
    /// line that made it unusable.
    pub line: usize,
    /// The name of the airspace it belongs to, if it has one yet.
    pub airspace: Option<String>,
    /// What became of it.
    pub kind: ProblemKind,
    /// What is wrong, in plain words.
    pub message: String,
}

/// What a file contained.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenAir {
    /// The airspaces that could be read, in the order of the file.
    pub airspaces: Vec<Airspace>,
    /// What could not be used as written, in the order of the file.
    pub problems: Vec<Problem>,
    /// How many further problems were found after [`MAX_PROBLEMS`].
    pub problems_omitted: usize,
    /// The format version the file names in its header, if any.
    pub version: Option<String>,
    /// The program or person the header says wrote the file, if any.
    pub written_by: Option<String>,
}

impl OpenAir {
    /// How many airspaces of the file are missing from [`OpenAir::airspaces`],
    /// among the problems that were kept.
    pub fn skipped(&self) -> usize {
        self.problems
            .iter()
            .filter(|problem| problem.kind == ProblemKind::AirspaceSkipped)
            .count()
    }
}

/// Reads a file whose encoding is not known.
///
/// The specification asks for UTF-8, and many files in circulation are in a
/// Western European single-byte encoding instead. Bytes that are not UTF-8
/// are read as Latin-1, which keeps umlauts and accents in names.
pub fn parse_bytes(bytes: &[u8], options: &Options) -> OpenAir {
    match std::str::from_utf8(bytes) {
        Ok(text) => parse(text, options),
        Err(_) => parse(
            &bytes
                .iter()
                .map(|&byte| char::from(byte))
                .collect::<String>(),
            options,
        ),
    }
}

/// Reads an OpenAir file.
pub fn parse(text: &str, options: &Options) -> OpenAir {
    let mut reader = Reader::new(options);
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    for (index, line) in text.lines().enumerate() {
        reader.line(index + 1, line);
    }
    reader.finish()
}

/// One piece of a boundary, in the order the file gives them.
#[derive(Debug, Clone, Copy)]
enum Piece {
    Point(GeoPoint),
    /// An arc with its two ends. The ends are corners of the polygon as they
    /// stand; the points between them are worked out once the direction of
    /// the whole boundary is known.
    Arc {
        start: GeoPoint,
        arc: Arc,
        end: GeoPoint,
    },
}

/// An arc as a line of the file gives it, before its direction is applied.
struct ArcEnds {
    centre: GeoPoint,
    start: GeoPoint,
    end: GeoPoint,
    from_deg: f64,
    to_deg: f64,
    start_radius_m: f64,
    end_radius_m: f64,
}

/// An airspace while its lines are being read.
#[derive(Debug)]
struct Draft {
    /// The line of its `AC`.
    line: usize,
    class: AirspaceClass,
    /// The type its `AC` implies, for files that put a type there.
    type_from_class: AirspaceType,
    airspace_type: Option<AirspaceType>,
    name: Option<String>,
    lower: Option<VerticalLimit>,
    upper: Option<VerticalLimit>,
    own_id: Option<String>,
    frequency: Option<f64>,
    station: Option<String>,
    by_notam: bool,
    activation_times: Vec<String>,
    pieces: Vec<Piece>,
    circle: Option<Circle>,
    /// The most points the pieces can turn into.
    points: usize,
    centre: Option<GeoPoint>,
    clockwise: bool,
    /// The first thing that made the shape unusable.
    broken: Option<(usize, String)>,
}

struct Reader<'a> {
    options: &'a Options,
    draft: Option<Draft>,
    airspaces: Vec<Airspace>,
    problems: Vec<Problem>,
    problems_omitted: usize,
    version: Option<String>,
    written_by: Option<String>,
    /// How often each derived identifier has been given out.
    ids: BTreeMap<String, usize>,
    file_points: usize,
}

impl<'a> Reader<'a> {
    fn new(options: &'a Options) -> Self {
        Self {
            options,
            draft: None,
            airspaces: Vec::new(),
            problems: Vec::new(),
            problems_omitted: 0,
            version: None,
            written_by: None,
            ids: BTreeMap::new(),
            file_points: 0,
        }
    }

    fn problem(&mut self, line: usize, kind: ProblemKind, message: impl Into<String>) {
        if self.problems.len() >= MAX_PROBLEMS {
            self.problems_omitted += 1;
            return;
        }
        let airspace = self.draft.as_ref().and_then(|draft| draft.name.clone());
        self.problems.push(Problem {
            line,
            airspace,
            kind,
            message: message.into(),
        });
    }

    fn line(&mut self, number: usize, raw: &str) {
        let (content, comment) = match raw.split_once('*') {
            Some((content, comment)) => (content, Some(comment)),
            None => (raw, None),
        };
        if let Some(comment) = comment
            && content.trim().is_empty()
        {
            self.header(comment);
        }
        let content = content.trim();
        if content.is_empty() {
            return;
        }

        let (command, rest) = match content.split_once(char::is_whitespace) {
            Some((command, rest)) => (command, rest.trim()),
            None => (content, ""),
        };
        let command = command.to_ascii_uppercase();

        if command == "AC" {
            self.close();
            self.draft = Some(Draft::new(number, rest));
            return;
        }
        if self.draft.is_none() {
            self.problem(
                number,
                ProblemKind::LineIgnored,
                format!(
                    "{} before the first AC line belongs to no airspace",
                    shorten(content)
                ),
            );
            return;
        }

        match command.as_str() {
            "AN" => self
                .with_draft(|draft| draft.name = Some(rest.chars().take(MAX_NAME_CHARS).collect())),
            "AY" => self.with_draft(|draft| draft.airspace_type = Some(type_from_text(rest))),
            "AI" => self.with_draft(|draft| draft.own_id = non_empty(rest)),
            "AG" => self.with_draft(|draft| draft.station = non_empty(rest)),
            "AF" => match leading_number(rest) {
                Some((mhz, _)) if (0.1..=400.0).contains(&mhz) => {
                    self.with_draft(|draft| draft.frequency = Some(mhz));
                }
                _ => self.problem(
                    number,
                    ProblemKind::LineIgnored,
                    format!("frequency {} cannot be read", shorten(rest)),
                ),
            },
            "AA" => self.with_draft(|draft| {
                if rest.eq_ignore_ascii_case("NONE") {
                    draft.by_notam = true;
                } else if draft.activation_times.len() < 32 {
                    draft.activation_times.push(rest.chars().take(64).collect());
                }
            }),
            // A transponder code, a label position, pens and brushes: read and not kept.
            "AX" | "AT" | "SP" | "SB" => {}
            "AL" | "AH" => self.limit(number, &command, rest),
            "V" => self.variable(number, rest),
            "DP" => match point(rest) {
                Some((corner, "")) => self.piece(number, Piece::Point(corner), 1),
                _ => self.broken(number, format!("corner {} cannot be read", shorten(rest))),
            },
            "DC" => self.circle(number, rest),
            "DA" => self.arc_by_bearings(number, rest),
            "DB" => self.arc_by_points(number, rest),
            "DY" => self.broken(number, "airway segments (DY) are not supported".to_owned()),
            // A shape command this reader does not know, or the stump of one
            // in a file that was cut off: the shape cannot be trusted.
            other if other.starts_with('D') && other.len() <= 2 => {
                self.broken(number, format!("shape command {other} is not known"));
            }
            other => self.problem(
                number,
                ProblemKind::LineIgnored,
                format!("command {} is not known", shorten(other)),
            ),
        }
    }

    /// Picks the version and the author out of a header comment.
    fn header(&mut self, comment: &str) {
        let Some((key, value)) = comment.split_once(':') else {
            return;
        };
        let value: String = value.trim().chars().take(MAX_NAME_CHARS).collect();
        if value.is_empty() {
            return;
        }
        match key.trim().to_ascii_uppercase().as_str() {
            "VERSION" => self.version = self.version.take().or(Some(value)),
            "WRITTEN_BY" => self.written_by = self.written_by.take().or(Some(value)),
            _ => {}
        }
    }

    fn with_draft(&mut self, change: impl FnOnce(&mut Draft)) {
        if let Some(draft) = self.draft.as_mut() {
            change(draft);
        }
    }

    /// Marks the shape of the airspace being read as unusable. Only the
    /// first reason is kept; the report is made when the airspace ends.
    fn broken(&mut self, line: usize, reason: String) {
        self.with_draft(|draft| {
            draft.broken.get_or_insert((line, reason));
        });
    }

    /// Adds a corner or an arc to the shape. `points` is the most polygon
    /// points it can turn into.
    fn piece(&mut self, line: usize, piece: Piece, points: usize) {
        let mut refused = None;
        self.with_draft(|draft| {
            draft.points = draft.points.saturating_add(points);
            if draft.circle.is_some() {
                refused = Some("a circle (DC) is mixed with other shape lines".to_owned());
            } else if draft.points > MAX_RING_POINTS {
                refused = Some(format!(
                    "more than {MAX_RING_POINTS} points in one airspace"
                ));
            } else if draft.broken.is_none() {
                draft.pieces.push(piece);
            }
        });
        if let Some(reason) = refused {
            self.broken(line, reason);
        }
    }

    fn limit(&mut self, line: usize, command: &str, text: &str) {
        let is_floor = command == "AL";
        let (limit, problem) = match limit(text) {
            Some(limit) => (limit, None),
            None if is_floor => (
                VerticalLimit::ground(),
                Some(format!(
                    "floor {} cannot be read; the ground is assumed",
                    shorten(text)
                )),
            ),
            None => (
                VerticalLimit::unlimited(),
                Some(format!(
                    "ceiling {} cannot be read; no upper limit is assumed",
                    shorten(text)
                )),
            ),
        };
        if let Some(problem) = problem {
            self.problem(line, ProblemKind::LimitAssumed, problem);
        }
        self.with_draft(|draft| {
            if is_floor {
                draft.lower = Some(limit);
            } else {
                draft.upper = Some(limit);
            }
        });
    }

    fn variable(&mut self, line: usize, text: &str) {
        let Some((name, value)) = text.split_once('=') else {
            self.broken(line, format!("variable {} cannot be read", shorten(text)));
            return;
        };
        let value = value.trim();
        match name.trim().to_ascii_uppercase().as_str() {
            "X" => match point(value) {
                Some((centre, "")) => self.with_draft(|draft| draft.centre = Some(centre)),
                _ => self.broken(line, format!("centre {} cannot be read", shorten(value))),
            },
            "D" => match value {
                "+" => self.with_draft(|draft| draft.clockwise = true),
                "-" => self.with_draft(|draft| draft.clockwise = false),
                _ => self.broken(
                    line,
                    format!("direction {} is neither + nor -", shorten(value)),
                ),
            },
            // The width of an airway, used only by DY, and a zoom level for drawing.
            "W" | "Z" | "T" => {}
            other => self.broken(line, format!("variable {} is not known", shorten(other))),
        }
    }

    /// The centre set by the last `V X=`, or a broken shape.
    fn centre(&mut self, line: usize) -> Option<GeoPoint> {
        let centre = self.draft.as_ref().and_then(|draft| draft.centre);
        if centre.is_none() {
            self.broken(
                line,
                "no centre was set with V X= before this line".to_owned(),
            );
        }
        centre
    }

    fn circle(&mut self, line: usize, text: &str) {
        let Some(centre) = self.centre(line) else {
            return;
        };
        let radius_m = match leading_number(text) {
            Some((miles, "")) if miles > 0.0 && miles <= MAX_RADIUS_NM => {
                miles * METRES_PER_NAUTICAL_MILE
            }
            _ => {
                self.broken(line, format!("radius {} cannot be used", shorten(text)));
                return;
            }
        };
        let already_shaped = self
            .draft
            .as_ref()
            .is_some_and(|draft| draft.circle.is_some() || !draft.pieces.is_empty());
        if already_shaped {
            self.broken(
                line,
                "a circle (DC) is mixed with other shape lines".to_owned(),
            );
            return;
        }
        self.with_draft(|draft| draft.circle = Some(Circle { centre, radius_m }));
    }

    fn arc_by_bearings(&mut self, line: usize, text: &str) {
        let Some(centre) = self.centre(line) else {
            return;
        };
        let numbers: Vec<Option<f64>> = text
            .split(',')
            .map(|part| match leading_number(part.trim()) {
                Some((number, "")) => Some(number),
                _ => None,
            })
            .collect();
        let [Some(miles), Some(from), Some(to)] = numbers[..] else {
            self.broken(
                line,
                format!("arc {} is not a radius and two bearings", shorten(text)),
            );
            return;
        };
        if !(miles > 0.0 && miles <= MAX_RADIUS_NM) || from.abs() > 720.0 || to.abs() > 720.0 {
            self.broken(line, format!("arc {} cannot be used", shorten(text)));
            return;
        }
        let radius_m = miles * METRES_PER_NAUTICAL_MILE;
        let (Some(start), Some(end)) = (
            destination(centre, from, radius_m),
            destination(centre, to, radius_m),
        ) else {
            self.broken(line, format!("arc {} cannot be placed", shorten(text)));
            return;
        };
        self.arc(
            line,
            ArcEnds {
                centre,
                start,
                end,
                from_deg: from,
                to_deg: to,
                start_radius_m: radius_m,
                end_radius_m: radius_m,
            },
        );
    }

    fn arc_by_points(&mut self, line: usize, text: &str) {
        let Some(centre) = self.centre(line) else {
            return;
        };
        let ends = point(text).and_then(|(start, rest)| {
            let rest = rest.strip_prefix(',').unwrap_or(rest).trim_start();
            match point(rest) {
                Some((end, "")) => Some((start, end)),
                _ => None,
            }
        });
        let Some((start, end)) = ends else {
            self.broken(line, format!("arc {} is not two points", shorten(text)));
            return;
        };
        let (start_radius, end_radius) = (centre.distance_m(start), centre.distance_m(end));
        let largest = MAX_RADIUS_NM * METRES_PER_NAUTICAL_MILE;
        if start_radius <= 0.0
            || end_radius <= 0.0
            || start_radius > largest
            || end_radius > largest
        {
            self.broken(
                line,
                format!(
                    "arc {} lies on its centre or too far from it",
                    shorten(text)
                ),
            );
            return;
        }
        self.arc(
            line,
            ArcEnds {
                centre,
                start,
                end,
                from_deg: bearing_deg(centre, start),
                to_deg: bearing_deg(centre, end),
                start_radius_m: start_radius,
                end_radius_m: end_radius,
            },
        );
    }

    /// Adds an arc that runs in the direction the last `V D=` set.
    fn arc(&mut self, line: usize, ends: ArcEnds) {
        let clockwise = self.draft.as_ref().is_none_or(|draft| draft.clockwise);
        let sweep_deg = if clockwise {
            (ends.to_deg - ends.from_deg).rem_euclid(360.0)
        } else {
            -(ends.from_deg - ends.to_deg).rem_euclid(360.0)
        };
        let arc = Arc {
            centre: ends.centre,
            start_deg: ends.from_deg,
            sweep_deg,
            start_radius_m: ends.start_radius_m,
            end_radius_m: ends.end_radius_m,
        };
        let points = arc.most_points() + 2;
        let piece = Piece::Arc {
            start: ends.start,
            arc,
            end: ends.end,
        };
        self.piece(line, piece, points);
    }

    /// Ends the airspace being read: it becomes a record or a problem.
    fn close(&mut self) {
        let Some(draft) = self.draft.take() else {
            return;
        };
        let name = draft.name.clone();
        let line = draft.line;
        let mut notes = Vec::new();
        match self.build(draft, &mut notes) {
            Ok(airspace) => {
                self.file_points += airspace.geometry.exterior().len();
                for (kind, message) in notes {
                    self.push_problem(line, name.clone(), kind, message);
                }
                self.airspaces.push(airspace);
            }
            Err((line, reason)) => {
                self.push_problem(line, name, ProblemKind::AirspaceSkipped, reason);
            }
        }
    }

    fn push_problem(
        &mut self,
        line: usize,
        airspace: Option<String>,
        kind: ProblemKind,
        message: String,
    ) {
        if self.problems.len() >= MAX_PROBLEMS {
            self.problems_omitted += 1;
            return;
        }
        self.problems.push(Problem {
            line,
            airspace,
            kind,
            message,
        });
    }

    fn build(
        &mut self,
        draft: Draft,
        notes: &mut Vec<(ProblemKind, String)>,
    ) -> Result<Airspace, (usize, String)> {
        if let Some(broken) = draft.broken {
            return Err(broken);
        }
        let over_budget = (
            draft.line,
            format!("the file has more than {MAX_FILE_POINTS} points; this airspace is left out"),
        );
        // Before laying the shape out: an arc line can stand for hundreds of points.
        if self.file_points.saturating_add(draft.pieces.len()) > MAX_FILE_POINTS {
            return Err(over_budget);
        }
        let fail = |error: venti_domain::AirspaceError| (draft.line, error.to_string());

        let geometry = match draft.circle {
            Some(circle) => {
                let ring = circle_ring(circle.centre, circle.radius_m)
                    .ok_or((draft.line, "the circle cannot be placed".to_owned()))?;
                Polygon::new(ring, Vec::new())
                    .and_then(|polygon| polygon.with_circle(circle))
                    .map_err(fail)?
            }
            None => {
                if draft.pieces.is_empty() {
                    return Err((draft.line, "the airspace has no shape".to_owned()));
                }
                let ring = ring_from(&draft.pieces)
                    .ok_or((draft.line, "an arc cannot be placed".to_owned()))?;
                Polygon::new(ring, Vec::new()).map_err(fail)?
            }
        };
        if self.file_points + geometry.exterior().len() > MAX_FILE_POINTS {
            return Err(over_budget);
        }

        let lower = draft.lower.unwrap_or_else(|| {
            notes.push((
                ProblemKind::LimitAssumed,
                "no floor (AL) is given; the ground is assumed".to_owned(),
            ));
            VerticalLimit::ground()
        });
        let upper = draft.upper.unwrap_or_else(|| {
            notes.push((
                ProblemKind::LimitAssumed,
                "no ceiling (AH) is given; no upper limit is assumed".to_owned(),
            ));
            VerticalLimit::unlimited()
        });

        let name = draft.name.unwrap_or_default();
        let local = match draft.own_id {
            Some(own) => own,
            None => {
                let derived = format!(
                    "{}_{}_{}",
                    slug(&name),
                    limit_slug(&lower),
                    limit_slug(&upper)
                );
                let count = self.ids.entry(derived.clone()).or_insert(0);
                *count += 1;
                if *count == 1 {
                    derived
                } else {
                    format!("{derived}_{count}")
                }
            }
        };
        let id = AirspaceId::new(self.options.source.clone(), local)
            .map_err(|e| (draft.line, e.to_string()))?;

        let hours_remarks = (!draft.activation_times.is_empty()).then(|| {
            format!(
                "Activation times in the file (UTC): {}",
                draft.activation_times.join("; ")
            )
        });
        let frequencies = draft
            .frequency
            .map(|mhz| Frequency {
                mhz,
                name: draft.station.clone(),
                primary: true,
            })
            .into_iter()
            .collect();

        let mut airspace = Airspace {
            id,
            name,
            class: draft.class,
            airspace_type: draft.airspace_type.unwrap_or(draft.type_from_class),
            country: self.options.country.clone(),
            lower,
            upper,
            lower_at_least: None,
            upper_at_most: None,
            geometry,
            activity: Activity {
                by_notam: draft.by_notam,
                hours_remarks,
                ..Activity::default()
            },
            frequencies,
            remarks: None,
            validity: Validity {
                airac: self.options.airac.clone(),
                file_date: self.options.file_date,
                updated: None,
            },
        };

        // Limits that contradict each other are replaced by the widest
        // pair rather than costing the airspace.
        if matches!(
            airspace.validate(),
            Err(venti_domain::AirspaceError::LimitsInverted
                | venti_domain::AirspaceError::LimitMisplaced)
        ) {
            notes.push((
                ProblemKind::LimitAssumed,
                "the floor lies above the ceiling; ground to unlimited is assumed".to_owned(),
            ));
            airspace.lower = VerticalLimit::ground();
            airspace.upper = VerticalLimit::unlimited();
        }
        airspace.validate().map_err(fail)?;
        Ok(airspace)
    }

    fn finish(mut self) -> OpenAir {
        self.close();
        OpenAir {
            airspaces: self.airspaces,
            problems: self.problems,
            problems_omitted: self.problems_omitted,
            version: self.version,
            written_by: self.written_by,
        }
    }
}

impl Draft {
    fn new(line: usize, class_text: &str) -> Self {
        let (class, type_from_class) = class_from_text(class_text);
        Self {
            line,
            class,
            type_from_class,
            airspace_type: None,
            name: None,
            lower: None,
            upper: None,
            own_id: None,
            frequency: None,
            station: None,
            by_notam: false,
            activation_times: Vec::new(),
            pieces: Vec::new(),
            circle: None,
            points: 0,
            centre: None,
            clockwise: true,
            broken: None,
        }
    }
}

/// Turns corners and arcs into one closed ring.
///
/// An arc is replaced by straight pieces on the side that makes the airspace
/// larger: outside its circle where the boundary bulges outward, inside
/// where it curves inward. Which is which depends on the direction the
/// whole boundary runs in, so the ring is laid out once with points on the
/// arcs to find that direction, and then for good.
fn ring_from(pieces: &[Piece]) -> Option<Vec<GeoPoint>> {
    let lay_out = |ring_is_clockwise: Option<bool>| -> Option<Vec<GeoPoint>> {
        let mut ring: Vec<GeoPoint> = Vec::new();
        let mut push = |corner: GeoPoint| {
            if ring.last() != Some(&corner) {
                ring.push(corner);
            }
        };
        for piece in pieces {
            match piece {
                Piece::Point(corner) => push(*corner),
                Piece::Arc { start, arc, end } => {
                    // An arc that turns the way the boundary turns bulges outward.
                    let bulges_out = ring_is_clockwise.is_some_and(|ring_is_clockwise| {
                        ring_is_clockwise == (arc.sweep_deg > 0.0)
                    });
                    push(*start);
                    for corner in arc.between(bulges_out)? {
                        push(corner);
                    }
                    push(*end);
                }
            }
        }
        if ring.first() != ring.last()
            && let Some(first) = ring.first().copied()
        {
            ring.push(first);
        }
        Some(ring)
    };

    if !pieces
        .iter()
        .any(|piece| matches!(piece, Piece::Arc { .. }))
    {
        return lay_out(None);
    }
    let draft = lay_out(None)?;
    lay_out(Some(doubled_signed_area(&draft) < 0.0))
}

/// The class an `AC` line names, and the type it implies in files that put
/// a type there, as the original format did.
fn class_from_text(text: &str) -> (AirspaceClass, AirspaceType) {
    let word = text.trim().to_ascii_uppercase();
    let class = match word.as_str() {
        "A" => Some(AirspaceClass::A),
        "B" => Some(AirspaceClass::B),
        "C" => Some(AirspaceClass::C),
        "D" => Some(AirspaceClass::D),
        "E" => Some(AirspaceClass::E),
        "F" => Some(AirspaceClass::F),
        "G" => Some(AirspaceClass::G),
        _ => None,
    };
    match class {
        Some(class) => (class, AirspaceType::Unspecified),
        None if word == "UNC" || word.is_empty() => {
            (AirspaceClass::Unclassified, AirspaceType::Unspecified)
        }
        None => (AirspaceClass::Unclassified, type_from_text(&word)),
    }
}

/// The type an `AY` line names, or an old `AC` line that carries a type.
fn type_from_text(text: &str) -> AirspaceType {
    let word = text.trim().to_ascii_uppercase();
    match word.as_str() {
        "CTR" => AirspaceType::Ctr,
        "MCTR" => AirspaceType::MilitaryCtr,
        "TMA" => AirspaceType::Tma,
        "CTA" => AirspaceType::Cta,
        "ATZ" => AirspaceType::Atz,
        "MATZ" => AirspaceType::Matz,
        "HTZ" => AirspaceType::Htz,
        "TIZ" => AirspaceType::Tiz,
        "TIA" => AirspaceType::Tia,
        "R" | "RESTRICTED" => AirspaceType::Restricted,
        "Q" | "DANGER" => AirspaceType::Danger,
        "P" | "PROHIBITED" => AirspaceType::Prohibited,
        "GP" => AirspaceType::GliderProhibited,
        "TMZ" => AirspaceType::Tmz,
        "RMZ" => AirspaceType::Rmz,
        "TRZ" => AirspaceType::TransponderRecommended,
        "TRP" => AirspaceType::TransponderSetting,
        "TRA" => AirspaceType::Tra,
        "TSA" => AirspaceType::Tsa,
        "TRAFR" => AirspaceType::FeedingRoute,
        "TFR" => AirspaceType::TemporaryRestriction,
        "N" => AirspaceType::NotamArea,
        "GSEC" | "W" => AirspaceType::GliderSector,
        "ASRA" | "AERIAL_SPORTING_RECREATIONAL" => AirspaceType::AerialSporting,
        "OFR" => AirspaceType::OverflightRestriction,
        "PROTECTED" => AirspaceType::Protected,
        "MTA" => AirspaceType::MilitaryTrainingArea,
        "MTR" => AirspaceType::MilitaryTrainingRoute,
        "MRT" => AirspaceType::MilitaryRoute,
        "ADIZ" => AirspaceType::Adiz,
        "ALERT" => AirspaceType::Alert,
        "WARNING" => AirspaceType::Warning,
        "FIR" => AirspaceType::Fir,
        "UIR" => AirspaceType::Uir,
        "FIS" => AirspaceType::FisSector,
        "ACCSEC" => AirspaceType::AccSector,
        "VFRSEC" => AirspaceType::VfrSector,
        "VFRR" => AirspaceType::VfrRoute,
        "AWY" => AirspaceType::Airway,
        "LTA" => AirspaceType::Lta,
        "UTA" => AirspaceType::Uta,
        "NONE" | "" => AirspaceType::Unspecified,
        _ => AirspaceType::Other(word.chars().take(40).collect()),
    }
}

/// Reads a limit in any of the spellings files use: `GND`, `SFC`, `UNL`,
/// `FL95`, `FL 095`, `4500ft MSL`, `4500 FT AMSL`, `1000 ft AGL`,
/// `1000ft GND`, `1500m AMSL`, `2500MSL`, `5000ft STD`, `3500` (feet above
/// sea level). `None` for anything else, including two limits on one line.
fn limit(text: &str) -> Option<VerticalLimit> {
    let upper = text.trim().to_ascii_uppercase();
    let words = limit_words(&upper);
    let words: Vec<&str> = words.iter().map(String::as_str).collect();

    match words[..] {
        ["GND" | "SFC" | "GROUND" | "SURFACE"] => return Some(VerticalLimit::ground()),
        ["UNL" | "UNLIM" | "UNLIMITED" | "UNLTD" | "UNLD"] => {
            return Some(VerticalLimit::unlimited());
        }
        ["FL", level] | [level, "FL"] => {
            let level: f64 = level.parse().ok()?;
            return VerticalLimit::new(VerticalReference::Standard, level, HeightUnit::FlightLevel)
                .ok();
        }
        _ => {}
    }

    let (value, rest) = words.split_first()?;
    let value: f64 = value.parse().ok()?;
    let (unit, rest) = match rest.split_first() {
        Some((&("FT" | "F" | "FEET"), rest)) => (HeightUnit::Feet, rest),
        Some((&("M" | "MTR" | "METER" | "METERS" | "METRE" | "METRES"), rest)) => {
            (HeightUnit::Metres, rest)
        }
        _ => (HeightUnit::Feet, rest),
    };
    let reference = match rest {
        [] | ["MSL" | "AMSL" | "ALT" | "QNH"] => VerticalReference::MeanSeaLevel,
        ["AGL" | "GND" | "SFC" | "ASFC" | "AAL"] => VerticalReference::AboveGround,
        ["STD"] => VerticalReference::Standard,
        _ => return None,
    };
    // Nothing above the ground, or at sea level with no reference given, is the ground.
    if value == 0.0 && (reference == VerticalReference::AboveGround || rest.is_empty()) {
        return Some(VerticalLimit::ground());
    }
    VerticalLimit::new(reference, value, unit).ok()
}

/// Splits a limit into numbers and words: `2000FT AGL` gives `2000`, `FT`,
/// `AGL`. Anything that is neither makes a word of its own, which no
/// spelling accepts.
fn limit_words(text: &str) -> Vec<String> {
    #[derive(PartialEq, Clone, Copy)]
    enum Kind {
        Number,
        Letters,
        Other,
    }
    let mut words: Vec<String> = Vec::new();
    let mut previous = None;
    for character in text.chars() {
        if character.is_whitespace() {
            previous = None;
            continue;
        }
        let kind = if character.is_ascii_digit() || character == '.' {
            Kind::Number
        } else if character.is_ascii_alphabetic() {
            Kind::Letters
        } else {
            Kind::Other
        };
        match words.last_mut() {
            Some(word) if previous == Some(kind) && kind != Kind::Other => word.push(character),
            _ => {
                // More words than any spelling has: stop collecting.
                if words.len() >= 8 {
                    break;
                }
                words.push(character.to_string());
            }
        }
        previous = Some(kind);
    }
    words
}

/// Reads a position from the start of a text and returns what follows it.
///
/// Both notations of the format are read, `47:15:37 N 011:20:38 E` (degrees,
/// minutes, seconds) and `47:15.617N 011:20.633E` (degrees and decimal
/// minutes), with or without a space before the hemisphere, and plain
/// decimal degrees as well.
fn point(text: &str) -> Option<(GeoPoint, &str)> {
    let (lat, rest) = angle(text, 90.0, 'N', 'S')?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(',').unwrap_or(rest).trim_start();
    let (lon, rest) = angle(rest, 180.0, 'E', 'W')?;
    Some((GeoPoint::new(lat, lon).ok()?, rest.trim_start()))
}

/// Reads `degrees[:minutes[:seconds]]` and a hemisphere letter.
fn angle(text: &str, most: f64, positive: char, negative: char) -> Option<(f64, &str)> {
    let (degrees, mut rest) = leading_number(text.trim_start())?;
    let mut value = degrees;
    let mut scale = 60.0;
    for _ in 0..2 {
        let Some(after_colon) = rest.strip_prefix(':') else {
            break;
        };
        let (part, after_part) = leading_number(after_colon.trim_start())?;
        if part >= 60.000_001 {
            return None;
        }
        value += part / scale;
        scale *= 60.0;
        rest = after_part;
    }
    let rest = rest.trim_start();
    let mut characters = rest.chars();
    let hemisphere = characters.next()?.to_ascii_uppercase();
    let sign = if hemisphere == positive {
        1.0
    } else if hemisphere == negative {
        -1.0
    } else {
        return None;
    };
    if value > most {
        return None;
    }
    Some((sign * value, characters.as_str()))
}

/// Reads an unsigned decimal number from the start of a text and returns
/// what follows it. Digits and one decimal point; no sign, no exponent.
fn leading_number(text: &str) -> Option<(f64, &str)> {
    let end = text
        .char_indices()
        .find(|&(_, character)| !(character.is_ascii_digit() || character == '.'))
        .map_or(text.len(), |(index, _)| index);
    // A number longer than this is not a coordinate, a radius or a height.
    if end == 0 || end > 24 {
        return None;
    }
    let value: f64 = text[..end].parse().ok()?;
    value
        .is_finite()
        .then_some((value, text[end..].trim_start()))
}

fn non_empty(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.chars().take(MAX_NAME_CHARS).collect())
}

/// A piece of a line for a message, shortened if long.
fn shorten(text: &str) -> String {
    let mut short: String = text.chars().take(40).collect();
    if text.chars().count() > 40 {
        short.push_str("...");
    }
    format!("{short:?}")
}

/// A name reduced to lower-case letters, digits and hyphens.
fn slug(text: &str) -> String {
    let mut slug = String::new();
    for character in text.chars().take(80) {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "unnamed".to_owned()
    } else {
        slug.to_owned()
    }
}

/// A limit as a short word for an identifier: `gnd`, `unl`, `fl100`, `4500ft-msl`.
fn limit_slug(limit: &VerticalLimit) -> String {
    let mut text = String::new();
    let value = limit.value();
    match (limit.reference(), limit.unit()) {
        (VerticalReference::Ground, _) => text.push_str("gnd"),
        (VerticalReference::Unlimited, _) => text.push_str("unl"),
        (_, HeightUnit::FlightLevel) => {
            let _ = write!(text, "fl{value}");
        }
        (reference, unit) => {
            let unit = if unit == HeightUnit::Metres {
                "m"
            } else {
                "ft"
            };
            let reference = match reference {
                VerticalReference::AboveGround => "agl",
                VerticalReference::Standard => "std",
                _ => "msl",
            };
            let _ = write!(text, "{value}{unit}-{reference}");
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use venti_domain::METRES_PER_FOOT;

    use super::*;
    use crate::ARC_MAX_ERROR_M;
    use crate::geometry::contains;

    fn options() -> Options {
        Options::new(SourceId::new("test-file").unwrap())
    }

    fn read(text: &str) -> OpenAir {
        parse(text, &options())
    }

    fn at(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).unwrap()
    }

    /// The example of the extended format from Naviter's specification,
    /// completed with limits and a shape.
    const NORDHOLZ: &str = "\
*VERSION: 2.1
*WRITTEN_BY: SeeYou

AC E
AY RMZ
AN RMZ ETMN-GLIDER HX
AF 123.300
AG Nordholz Radar
AX 7000
AA NONE
AH FL145
AL 1000ft AGL
DP 53:47:06N 008:21:41E
DP 53:50:58N 008:55:25E
DP 53:45:08N 008:57:21E
DP 53:41:10N 008:23:38E
DP 53:47:06N 008:21:41E
";

    #[test]
    fn reads_an_airspace_in_the_extended_format() {
        let file = read(NORDHOLZ);
        assert_eq!(file.problems, Vec::new());
        assert_eq!(file.version.as_deref(), Some("2.1"));
        assert_eq!(file.written_by.as_deref(), Some("SeeYou"));

        let rmz = &file.airspaces[0];
        assert_eq!(rmz.name, "RMZ ETMN-GLIDER HX");
        assert_eq!(rmz.class, AirspaceClass::E);
        assert_eq!(rmz.airspace_type, AirspaceType::Rmz);
        assert_eq!(
            rmz.id.to_string(),
            "test-file:rmz-etmn-glider-hx_1000ft-agl_fl145"
        );
        assert_eq!(rmz.lower.reference(), VerticalReference::AboveGround);
        assert!((rmz.lower.metres().unwrap() - 304.8).abs() < 1e-9);
        assert_eq!(rmz.upper, VerticalLimit::flight_level(145).unwrap());
        assert_eq!(rmz.frequencies.len(), 1);
        assert!((rmz.frequencies[0].mhz - 123.3).abs() < 1e-9);
        assert_eq!(rmz.frequencies[0].name.as_deref(), Some("Nordholz Radar"));
        assert!(rmz.activity.by_notam);
        let ring = rmz.geometry.exterior();
        assert_eq!(ring.len(), 5);
        assert!((ring[0].lat() - (53.0 + 47.0 / 60.0 + 6.0 / 3600.0)).abs() < 1e-9);
        assert!((ring[0].lon() - (8.0 + 21.0 / 60.0 + 41.0 / 3600.0)).abs() < 1e-9);
        assert_eq!(rmz.validate(), Ok(()));
    }

    #[test]
    fn reads_the_original_format_where_the_class_line_carries_the_type() {
        let file = read(
            "AC R\n\
             AN ED-R10B Todendorf-Putlos MON-SAT+\n\
             AH 40000ft AMSL\n\
             AL GND\n\
             DP 54:25:00 N 010:40:00 E\n\
             DP 54:25:00 N 010:50:00 E\n\
             DP 54:15:00 N 010:41:00 E\n",
        );
        assert!(file.problems.is_empty());
        let area = &file.airspaces[0];
        assert_eq!(area.class, AirspaceClass::Unclassified);
        assert_eq!(area.airspace_type, AirspaceType::Restricted);
        // The ring is closed for the file.
        assert_eq!(area.geometry.exterior().len(), 4);
        assert_eq!(area.geometry.exterior()[0], area.geometry.exterior()[3]);
        assert!((area.upper.metres().unwrap() - 40_000.0 * METRES_PER_FOOT).abs() < 1e-9);

        for (class_line, expected) in [
            ("Q", AirspaceType::Danger),
            ("P", AirspaceType::Prohibited),
            ("GP", AirspaceType::GliderProhibited),
            ("CTR", AirspaceType::Ctr),
            ("W", AirspaceType::GliderSector),
            ("TMZ", AirspaceType::Tmz),
            ("gsec", AirspaceType::GliderSector),
            ("WEIRD", AirspaceType::Other("WEIRD".to_owned())),
        ] {
            let text = format!("AC {class_line}\nAL GND\nAH UNL\nV X=47:00:00N 011:00:00E\nDC 1\n");
            let file = read(&text);
            assert_eq!(file.airspaces[0].airspace_type, expected, "{class_line}");
            assert_eq!(file.airspaces[0].class, AirspaceClass::Unclassified);
        }
        // The type line wins over a type in the class line, and a class stays a class.
        let file = read("AC D\nAY CTR\nAL GND\nAH UNL\nV X=47:00:00N 011:00:00E\nDC 1\n");
        assert_eq!(file.airspaces[0].class, AirspaceClass::D);
        assert_eq!(file.airspaces[0].airspace_type, AirspaceType::Ctr);
    }

    #[test]
    fn reads_limits_in_every_spelling() {
        let feet =
            |reference, value| VerticalLimit::new(reference, value, HeightUnit::Feet).unwrap();
        let metres =
            |reference, value| VerticalLimit::new(reference, value, HeightUnit::Metres).unwrap();
        let msl = VerticalReference::MeanSeaLevel;
        let agl = VerticalReference::AboveGround;
        let cases = [
            ("GND", VerticalLimit::ground()),
            ("SFC", VerticalLimit::ground()),
            ("gnd", VerticalLimit::ground()),
            ("0", VerticalLimit::ground()),
            ("0ft AGL", VerticalLimit::ground()),
            ("0 FT GND", VerticalLimit::ground()),
            ("UNL", VerticalLimit::unlimited()),
            ("UNLIM", VerticalLimit::unlimited()),
            ("UNLIMITED", VerticalLimit::unlimited()),
            ("FL95", VerticalLimit::flight_level(95).unwrap()),
            ("FL 95", VerticalLimit::flight_level(95).unwrap()),
            ("FL095", VerticalLimit::flight_level(95).unwrap()),
            ("fl100", VerticalLimit::flight_level(100).unwrap()),
            ("4500ft MSL", feet(msl, 4500.0)),
            ("4500 ft MSL", feet(msl, 4500.0)),
            ("4500FT AMSL", feet(msl, 4500.0)),
            ("4500ft AMSL", feet(msl, 4500.0)),
            ("4500 MSL", feet(msl, 4500.0)),
            ("4500MSL", feet(msl, 4500.0)),
            ("4500", feet(msl, 4500.0)),
            ("4500ft", feet(msl, 4500.0)),
            ("4500 F ALT", feet(msl, 4500.0)),
            ("1000 ft AGL", feet(agl, 1000.0)),
            ("1000ft AGL", feet(agl, 1000.0)),
            ("2000FT AGL", feet(agl, 2000.0)),
            ("1000ft GND", feet(agl, 1000.0)),
            ("1000 FT ASFC", feet(agl, 1000.0)),
            ("1500m AMSL", metres(msl, 1500.0)),
            ("1500 M MSL", metres(msl, 1500.0)),
            ("300m AGL", metres(agl, 300.0)),
            ("5000ft STD", feet(VerticalReference::Standard, 5000.0)),
        ];
        for (text, expected) in cases {
            assert_eq!(limit(text), Some(expected), "{text}");
        }
        // A flight level and the same height in feet of pressure altitude agree.
        assert_eq!(
            limit("FL50").unwrap().metres(),
            limit("5000ft STD").unwrap().metres()
        );

        for text in [
            "",
            "BY NOTAM",
            "100m AGL/423m AMSL",
            "4500ft MSL 1000ft AGL",
            "-500ft MSL",
            "4,500ft",
            "FL",
            "FL95 MSL",
            "FL AGL",
            "ft",
            "1e9",
            "999999999999999999999999999999 ft",
            "4500 ft MSL extra",
        ] {
            assert_eq!(limit(text), None, "{text}");
        }
    }

    #[test]
    fn a_limit_that_cannot_be_read_becomes_the_widest_one_and_is_reported() {
        let file = read(
            "AC R\n\
             AN ED-R 1\n\
             AL BY NOTAM\n\
             AH 100m AGL/423m AMSL\n\
             V X=47:00:00N 011:00:00E\n\
             DC 2\n",
        );
        assert_eq!(file.airspaces.len(), 1);
        assert_eq!(file.airspaces[0].lower, VerticalLimit::ground());
        assert_eq!(file.airspaces[0].upper, VerticalLimit::unlimited());
        assert_eq!(file.problems.len(), 2);
        assert_eq!(file.problems[0].line, 3);
        assert_eq!(file.problems[0].kind, ProblemKind::LimitAssumed);
        assert_eq!(file.problems[0].airspace.as_deref(), Some("ED-R 1"));
        assert_eq!(file.problems[1].line, 4);

        // Limits left out altogether, and limits the wrong way round.
        let file = read("AC R\nAN NO LIMITS\nV X=47:00:00N 011:00:00E\nDC 2\n");
        assert_eq!(file.airspaces[0].lower, VerticalLimit::ground());
        assert_eq!(file.airspaces[0].upper, VerticalLimit::unlimited());
        assert_eq!(file.problems.len(), 2);
        assert!(
            file.problems
                .iter()
                .all(|p| p.kind == ProblemKind::LimitAssumed && p.line == 1)
        );

        let file = read("AC R\nAL FL100\nAH FL65\nV X=47:00:00N 011:00:00E\nDC 2\n");
        assert_eq!(file.airspaces[0].lower, VerticalLimit::ground());
        assert_eq!(file.airspaces[0].upper, VerticalLimit::unlimited());
        assert_eq!(file.problems.len(), 1);
        assert_eq!(file.skipped(), 0);
    }

    #[test]
    fn reads_both_coordinate_notations() {
        let expected = at(
            47.0 + 15.0 / 60.0 + 37.0 / 3600.0,
            11.0 + 20.0 / 60.0 + 38.0 / 3600.0,
        );
        for text in [
            "47:15:37 N 011:20:38 E",
            "47:15:37N 011:20:38E",
            "47:15:37 n 11:20:38 e",
            "47:15:37N,011:20:38E",
            "47:15:37.0N 011:20:38.00E",
        ] {
            let (read, rest) = point(text).unwrap();
            assert!(read.distance_m(expected) < 0.01, "{text}");
            assert_eq!(rest, "");
        }
        let minutes = at(45.0 + 15.531 / 60.0, -(108.0 + 26.75 / 60.0));
        let (read, rest) = point("45:15.531N 108:26.750W next").unwrap();
        assert!(read.distance_m(minutes) < 0.01);
        assert_eq!(rest, "next");
        let (south, _) = point("33:52:00 S 151:12:30 E").unwrap();
        assert!(south.lat() < -33.8 && south.lon() > 151.2);
        let (decimal, _) = point("47.5N 11.25E").unwrap();
        assert_eq!(decimal, at(47.5, 11.25));

        for text in [
            "",
            "47:15:37 011:20:38",
            "47:15:37 E 011:20:38 N",
            "91:00:00 N 011:20:38 E",
            "47:61:00 N 011:20:38 E",
            "47:15:61 N 011:20:38 E",
            "47:15:37 N 181:00:00 E",
            "47:15:37 N",
            "N 47:15:37 E 011:20:38",
            "47::37 N 011:20:38 E",
            "1e2:00:00 N 011:20:38 E",
        ] {
            assert!(point(text).is_none(), "{text}");
        }
    }

    #[test]
    fn a_circle_of_five_nautical_miles() {
        // The parachute area from Naviter's specification, with a larger radius.
        let file = read(
            "AC Q\n\
             AN PARA Ailertchen EDGA\n\
             AH FL100\n\
             AL GND\n\
             V X=50:35:36N 007:56:42E\n\
             DC 5.00\n",
        );
        assert!(file.problems.is_empty());
        let area = &file.airspaces[0];
        let centre = at(
            50.0 + 35.0 / 60.0 + 36.0 / 3600.0,
            7.0 + 56.0 / 60.0 + 42.0 / 3600.0,
        );
        let circle = area.geometry.circle().unwrap();
        assert!(circle.centre.distance_m(centre) < 0.01);
        assert_eq!(circle.radius_m, 9260.0);

        // By the definition: every point within 5 NM of the centre is in,
        // every point more than 5 NM and the allowed error away is out.
        for degree in 0..360 {
            let bearing = f64::from(degree);
            let inside = destination(centre, bearing, 9259.0).unwrap();
            let outside = destination(centre, bearing, 9260.0 + ARC_MAX_ERROR_M + 1.0).unwrap();
            assert!(contains(&area.geometry, inside), "{degree}");
            assert!(!contains(&area.geometry, outside), "{degree}");
        }
        assert!(contains(&area.geometry, centre));
    }

    /// A control zone shaped like a stadium: two straight sides and two half
    /// circles of 3 NM around the runway ends, one given by bearings and one
    /// by its end points.
    const STADIUM: &str = "\
AC D
AY CTR
AN CTR STADIUM
AL GND
AH 4500ft MSL
V X=47:30:00 N 011:00:00 E
DA 3, 180, 0
V X=47:30:00 N 011:20:00 E
DB 47:33:00 N 011:20:00 E, 47:27:00 N 011:20:00 E
";

    #[test]
    fn a_control_zone_with_arcs_never_leaves_out_what_the_arcs_enclose() {
        let file = read(STADIUM);
        assert_eq!(file.problems, Vec::new());
        let ctr = &file.airspaces[0];
        let (west, east) = (at(47.5, 11.0), at(47.5, 11.0 + 20.0 / 60.0));
        // The western arc is given by its radius. The eastern one is given
        // by its end points, three minutes of latitude from its centre,
        // which is four metres more than three nautical miles.
        let west_radius = 3.0 * METRES_PER_NAUTICAL_MILE;
        let east_radius = east.distance_m(at(47.55, 11.0 + 20.0 / 60.0));
        assert!(
            (east_radius - west_radius - 3.7).abs() < 0.1,
            "{east_radius}"
        );

        // The western half circle bulges to the west, the eastern to the east.
        for half_degree in 0..=360 {
            let around = f64::from(half_degree) / 2.0;
            for (centre, toward, radius) in [
                (west, 180.0 + around, west_radius),
                (east, around, east_radius),
            ] {
                let just_inside = destination(centre, toward, radius - 1.0).unwrap();
                let beyond = destination(centre, toward, radius + ARC_MAX_ERROR_M + 1.0).unwrap();
                assert!(contains(&ctr.geometry, just_inside), "{toward}");
                assert!(!contains(&ctr.geometry, beyond), "{toward}");
            }
        }
        // The middle of the zone and a point well north of it.
        assert!(contains(&ctr.geometry, at(47.5, 11.15)));
        assert!(!contains(&ctr.geometry, at(47.6, 11.15)));
        // An arc line is not kept as a circle.
        assert!(ctr.geometry.circle().is_none());
    }

    #[test]
    fn an_arc_that_curves_inward_is_cut_on_the_airspace_side() {
        // A square with a bite of 2 NM radius taken out of its eastern side:
        // the arc runs counter-clockwise on a clockwise boundary.
        let file = read(
            "AC R\n\
             AN BITTEN\n\
             AL GND\n\
             AH UNL\n\
             DP 47:40:00 N 011:00:00 E\n\
             DP 47:40:00 N 011:20:00 E\n\
             V X=47:30:00 N 011:20:00 E\n\
             V D=-\n\
             DA 2, 0, 180\n\
             DP 47:20:00 N 011:20:00 E\n\
             DP 47:20:00 N 011:00:00 E\n",
        );
        assert_eq!(file.problems, Vec::new());
        let area = &file.airspaces[0];
        let centre = at(47.5, 11.0 + 20.0 / 60.0);
        let radius = 2.0 * METRES_PER_NAUTICAL_MILE;
        // Just outside the bite, on its western half, is airspace: the
        // straight pieces run on the airspace side of the true curve.
        for degree in 181..360 {
            let bearing = f64::from(degree);
            let in_airspace = destination(centre, bearing, radius + 1.0).unwrap();
            let in_bite = destination(centre, bearing, radius - ARC_MAX_ERROR_M - 1.0).unwrap();
            assert!(contains(&area.geometry, in_airspace), "{degree}");
            assert!(!contains(&area.geometry, in_bite), "{degree}");
        }
    }

    #[test]
    fn an_arc_between_points_at_unequal_distances_ends_where_the_file_says() {
        let file = read(
            "AC C\n\
             AL GND\n\
             AH UNL\n\
             V X=47:30:00 N 011:00:00 E\n\
             DP 47:30:00 N 011:00:00 E\n\
             DB 47:34:00 N 011:00:00 E, 47:30:00 N 011:06:30 E\n",
        );
        assert_eq!(file.problems, Vec::new());
        let ring = file.airspaces[0].geometry.exterior();
        assert!(ring.contains(&at(47.0 + 34.0 / 60.0, 11.0)));
        assert!(ring.contains(&at(47.5, 11.0 + 6.5 / 60.0)));
    }

    #[test]
    fn a_shape_that_cannot_be_read_costs_the_airspace_and_is_reported() {
        let file = read(
            "AC D\n\
             AN FINE BEFORE\n\
             AL GND\n\
             AH FL100\n\
             V X=47:00:00N 011:00:00E\n\
             DC 2\n\
             AC R\n\
             AN BAD CORNER\n\
             AL GND\n\
             AH FL100\n\
             DP 47:00:00 N 011:00:00 E\n\
             DP 47:99:00 N 011:10:00 E\n\
             DP 47:10:00 N 011:10:00 E\n\
             AC R\n\
             AN AIRWAY\n\
             DY 47:00:00 N 011:00:00 E\n\
             AC R\n\
             AN NO CENTRE\n\
             DC 5\n\
             AC R\n\
             AN TWO POINTS\n\
             DP 47:00:00 N 011:00:00 E\n\
             DP 47:10:00 N 011:10:00 E\n\
             AC R\n\
             AN NOTHING\n\
             AC R\n\
             AN FUTURE SHAPE\n\
             DX 1 2 3\n\
             AC R\n\
             AN MIXED\n\
             V X=47:00:00N 011:00:00E\n\
             DC 2\n\
             DP 47:00:00 N 011:00:00 E\n\
             AC C\n\
             AN FINE AFTER\n\
             AL GND\n\
             AH FL100\n\
             V X=48:00:00N 011:00:00E\n\
             DC 2\n",
        );
        let names: Vec<&str> = file.airspaces.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["FINE BEFORE", "FINE AFTER"]);
        assert_eq!(file.skipped(), 7);

        let skipped: Vec<(usize, &str)> = file
            .problems
            .iter()
            .filter(|p| p.kind == ProblemKind::AirspaceSkipped)
            .map(|p| (p.line, p.airspace.as_deref().unwrap()))
            .collect();
        assert_eq!(
            skipped,
            [
                (12, "BAD CORNER"),
                (16, "AIRWAY"),
                (19, "NO CENTRE"),
                (20, "TWO POINTS"),
                (24, "NOTHING"),
                (28, "FUTURE SHAPE"),
                (33, "MIXED"),
            ]
        );
        assert!(
            file.problems[0].message.contains("47:99:00"),
            "{}",
            file.problems[0].message
        );
    }

    #[test]
    fn unknown_attributes_and_stray_lines_are_reported_and_cost_nothing() {
        let file = read(
            "DP 47:00:00 N 011:00:00 E\n\
             AC D\n\
             AN WITH EXTRAS\n\
             AZ something new\n\
             AF not-a-frequency\n\
             AT 47:00:00 N 011:00:00 E\n\
             SP 0,1,255,0,0\n\
             SB 255,255,0\n\
             V Z=100\n\
             AL GND\n\
             AH FL100\n\
             V X=47:00:00N 011:00:00E\n\
             DC 2 * a comment after a command\n\
             hello world\n",
        );
        assert_eq!(file.airspaces.len(), 1);
        let ignored: Vec<usize> = file
            .problems
            .iter()
            .filter(|p| p.kind == ProblemKind::LineIgnored)
            .map(|p| p.line)
            .collect();
        assert_eq!(ignored, [1, 4, 5, 14]);
        assert_eq!(file.problems.len(), 4);
    }

    #[test]
    fn identifiers_are_stable_and_distinct() {
        let block = |name: &str, lower: &str| {
            format!("AC D\nAN {name}\nAL {lower}\nAH FL195\nV X=47:00:00N 011:00:00E\nDC 2\n")
        };
        let text = [
            block("CTA N", "9500ft MSL"),
            block("CTA N", "FL125"),
            block("CTA N", "9500ft MSL"),
            block("Zürich Ünter", "GND"),
            "AC D\nAI 7f3a-own-key\nAL GND\nAH FL195\nV X=47:00:00N 011:00:00E\nDC 2\n".to_owned(),
            "AC D\nAL GND\nAH FL195\nV X=47:00:00N 011:00:00E\nDC 2\n".to_owned(),
        ]
        .concat();
        let ids: Vec<String> = read(&text)
            .airspaces
            .iter()
            .map(|airspace| airspace.id.to_string())
            .collect();
        assert_eq!(
            ids,
            [
                "test-file:cta-n_9500ft-msl_fl195",
                "test-file:cta-n_fl125_fl195",
                "test-file:cta-n_9500ft-msl_fl195_2",
                "test-file:z-rich-nter_gnd_fl195",
                "test-file:7f3a-own-key",
                "test-file:unnamed_gnd_fl195",
            ]
        );
        // Moving a corner does not change an identifier.
        let moved = text.replace("011:00:00E", "011:00:05E");
        let moved_ids: Vec<String> = read(&moved)
            .airspaces
            .iter()
            .map(|a| a.id.to_string())
            .collect();
        assert_eq!(ids, moved_ids);
    }

    #[test]
    fn what_the_caller_knows_is_written_into_every_record() {
        let options = Options {
            source: SourceId::new("my-file").unwrap(),
            country: Some("DE".to_owned()),
            file_date: Some("2026-11-19T00:00:00Z".parse().unwrap()),
            airac: Some("2612".to_owned()),
        };
        let file = parse(NORDHOLZ, &options);
        let rmz = &file.airspaces[0];
        assert_eq!(rmz.id.source().as_str(), "my-file");
        assert_eq!(rmz.country.as_deref(), Some("DE"));
        assert_eq!(rmz.validity.airac.as_deref(), Some("2612"));
        assert_eq!(rmz.validity.file_date, options.file_date);
    }

    #[test]
    fn activation_times_are_kept_as_text() {
        let file = read(
            "AC R\n\
             AN TIMED\n\
             AA 2023-12-16T12:00Z/2023-12-16T13:00Z   * Active from 12:00 to 13:00 UTC\n\
             AA 2024-12-17T00:00Z/NONE\n\
             AL GND\n\
             AH UNL\n\
             V X=47:00:00N 011:00:00E\n\
             DC 2\n",
        );
        let activity = &file.airspaces[0].activity;
        assert!(!activity.by_notam);
        assert_eq!(
            activity.hours_remarks.as_deref(),
            Some(
                "Activation times in the file (UTC): \
                 2023-12-16T12:00Z/2023-12-16T13:00Z; 2024-12-17T00:00Z/NONE"
            )
        );
    }

    #[test]
    fn files_in_latin_1_and_with_a_byte_order_mark_are_read() {
        let latin_1: Vec<u8> =
            b"AC D\nAN M\xdcNCHEN\nAL GND\nAH FL100\nV X=48:00:00N 011:00:00E\nDC 2\n".to_vec();
        let file = parse_bytes(&latin_1, &options());
        assert_eq!(file.airspaces[0].name, "MÜNCHEN");

        let with_mark = format!("\u{feff}{NORDHOLZ}");
        let file = parse_bytes(with_mark.as_bytes(), &options());
        assert_eq!(file.airspaces.len(), 1);
        assert!(file.problems.is_empty());

        let windows_lines = NORDHOLZ.replace('\n', "\r\n");
        assert_eq!(read(&windows_lines).airspaces, read(NORDHOLZ).airspaces);
    }

    #[test]
    fn a_file_cannot_ask_for_more_points_than_it_is_allowed() {
        // One airspace with too many corners.
        let mut text = String::from("AC R\nAN HUGE\nAL GND\nAH UNL\n");
        for index in 0..=MAX_RING_POINTS {
            let minutes = index % 60;
            let _ = writeln!(text, "DP 47:{minutes:02}:00 N 011:{minutes:02}:30 E");
        }
        let file = read(&text);
        assert!(file.airspaces.is_empty());
        assert_eq!(file.skipped(), 1);
        assert!(file.problems[0].message.contains("more than"));

        // Many airspaces, each a large circle of nearly a thousand points.
        let mut text = String::new();
        for index in 0..3000 {
            let _ = write!(
                text,
                "AC R\nAN BIG {index}\nAL GND\nAH UNL\nV X=47:00:00N 011:00:00E\nDC 500\n"
            );
        }
        let file = read(&text);
        let points: usize = file
            .airspaces
            .iter()
            .map(|airspace| airspace.geometry.exterior().len())
            .sum();
        assert!(points <= MAX_FILE_POINTS, "{points}");
        assert!(file.airspaces.len() < 3000);
        assert_eq!(
            file.airspaces.len() + file.problems.len() + file.problems_omitted,
            3000
        );
        assert_eq!(
            file.problems.len(),
            MAX_PROBLEMS.min(3000 - file.airspaces.len())
        );
    }

    #[test]
    fn numbers_too_large_to_be_real_are_refused() {
        for shape in [
            "DC 501",
            "DC 0",
            "DC 1e300",
            "DC 99999999999999999999999999999999999999",
            "DA 501, 0, 90",
            "DA 5, 0, 99999",
            "DA 5, 0",
            "DA 5, 0, 90, 12",
            "DA nan, inf, -inf",
            "DB 47:00:00 N 011:00:00 E",
            "DB 47:00:00 N 011:00:00 E, 47:00:00 N 011:00:00 E",
            "DB 40:00:00 N 011:00:00 E, 47:00:00 N 011:00:00 E",
        ] {
            let text = format!("AC R\nAL GND\nAH UNL\nV X=47:00:00N 011:00:00E\n{shape}\n");
            let file = read(&text);
            assert!(file.airspaces.is_empty(), "{shape}");
            assert_eq!(file.skipped(), 1, "{shape}");
            assert_eq!(file.problems[0].line, 5, "{shape}");
        }
    }

    #[test]
    fn the_poles_and_the_antimeridian_do_not_panic() {
        let file = read(
            "AC R\nAN POLE\nAL GND\nAH UNL\nV X=90:00:00N 000:00:00E\nDC 10\n\
             AC R\nAN DATELINE\nAL GND\nAH UNL\nV X=00:00:00N 180:00:00E\nDC 10\n\
             AC R\nAN SOUTH\nAL GND\nAH UNL\nDP 90:00:00 S 180:00:00 W\nDP 89:00:00 S 000:00:00 E\nDP 89:00:00 S 090:00:00 E\n",
        );
        // Whatever is kept is a valid record; whatever is not is reported.
        assert_eq!(file.airspaces.len() + file.skipped(), 3);
        for airspace in &file.airspaces {
            assert_eq!(airspace.validate(), Ok(()));
        }
    }
}
