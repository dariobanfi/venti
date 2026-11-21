//! What lies over a place, and the map's filter by height.
//!
//! A map draws airspace as outlines, and where several lie over each other
//! an outline says little. Asked at a place, the set lists every airspace
//! whose outline contains it, from the lowest floor up, each with its
//! limits as published, how the app treats it and what its source says
//! about when it is in use. It is the list the assistant's `airspace_at`
//! gives, in the same order and with the same words and caveats
//! (`venti_tools::airspace`), so that the app and the assistant say the
//! same about the same place.
//!
//! The filter by height takes airspace that begins above a height of
//! interest off a map. It changes what is drawn and nothing else: the
//! cockpit's warnings and the list over a place go by every airspace.
//!
//! Nothing here knows what is active today, and none of it is a clearance.
//! The answer says so itself.

use venti_airspace::{AltitudeBand, Altitudes};
use venti_domain::{Airspace, BoundingBox, DayTime, GeoPoint, OperatingHours};
use venti_tools::airspace::{CAVEATS, floor_order, policy_word, published_use, verdict};

use crate::airspace::{kind_in_words, limit_in_words, name_in_words, policy_view, shape};
use crate::{AirspaceOrigin, AirspacePolicy, AirspaceSet, AirspaceShape, Bounds, Coordinate};

/// The heights the map's filter offers, metres above mean sea level, the
/// highest first: round figures for where flying ends on the highest days
/// in the Alps, on most days there, and in the foothills. Ours, and the
/// owner's to decide.
const HEIGHT_STEPS_M: [f64; 3] = [4500.0, 3500.0, 2500.0];

/// One airspace over a place, as a sheet shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct AirspaceOverhead {
    /// Its identifier: the one its shape on the map and a warning carry.
    pub id: String,
    /// Its name as a warning names it: as published, or "Unnamed airspace".
    pub name: String,
    /// What it is, in words: "Class D control zone".
    pub kind: String,
    /// Its floor as published: "GND", "1000 ft AGL", "FL 95".
    pub lower: String,
    /// Its ceiling as published.
    pub upper: String,
    /// A second floor, where the source publishes one: the airspace
    /// begins at `lower` but at least this high.
    pub lower_at_least: Option<String>,
    /// How far the ceiling may be raised, where the source says so.
    pub upper_at_most: Option<String>,
    /// How the app treats it.
    pub policy: AirspacePolicy,
    /// The same in the warning table's word: "Avoid", "Caution" or "Free".
    pub policy_words: String,
    /// Why, in the table's sentence.
    pub why: String,
    /// What the source publishes about when it is in use, in a line:
    /// "Active by NOTAM, has published hours". Absent when the source
    /// marks nothing. Published background, not the state of today.
    pub published_use: Option<String>,
    /// Its hours of use by day of the week as published, a line for each
    /// run of days: "Mon to Fri 08:00 to sunset, not on public holidays".
    /// Empty when the source publishes none, which says nothing about
    /// when the airspace is active.
    pub hours: Vec<String>,
    /// The source's note on its hours, as published.
    pub hours_remarks: Option<String>,
    /// For a temporary area: from when it exists, seconds since
    /// 1970-01-01 UTC.
    pub exists_from_unix_s: Option<i64>,
    /// For a temporary area: when it ceases to exist.
    pub exists_until_unix_s: Option<i64>,
    /// The source's remarks, as published.
    pub remarks: Option<String>,
    /// Whether it is from a file of the pilot's or from the pilot's
    /// server.
    pub origin: AirspaceOrigin,
    /// Where it comes from: the name of the pilot's file, or the provider
    /// the server has it from.
    pub source: String,
    /// The date of its data, as [`crate::AirspaceWarning::data_unix_s`]
    /// gives it.
    pub data_unix_s: Option<i64>,
}

/// What lies over a place: every airspace whose outline contains it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct AirspaceOverPlace {
    /// The place that was asked about.
    pub position: Coordinate,
    /// The airspaces, from the lowest floor up: the ground first, then by
    /// the published height of the floor, whatever it is measured from.
    pub airspaces: Vec<AirspaceOverhead>,
    /// The answer in a line, for the head of the sheet: "3 airspaces over
    /// this place".
    pub words: String,
    /// Whether the app's airspace reaches the place: whether "none" means
    /// that, and not "no data here".
    pub covered: bool,
    /// What to say when it does not. `None` when it does.
    pub cover_note: Option<String>,
    /// What to say under hours and marks of use, when any airspace listed
    /// carries some: that they are as published and that the app does not
    /// go by them.
    pub hours_note: Option<String>,
    /// What the list is not, to show with it always: not certified and
    /// no clearance, and nothing in it says what is active today.
    pub caveat: String,
}

/// One step of the map's filter by height.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct AirspaceHeightStep {
    /// The height to hand to [`AirspaceSet::in_area_below`], metres above
    /// mean sea level. `None` for the step that shows all airspace.
    pub ceiling_m: Option<f64>,
    /// The step's name: "All heights", "Up to 2500 m".
    pub words: String,
}

/// The map's filter by height: its steps, and what it does.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct AirspaceHeightFilter {
    /// The steps to offer: all airspace first, then from the highest
    /// height down.
    pub steps: Vec<AirspaceHeightStep>,
    /// What the filter does and does not do, to show where it is chosen.
    pub note: String,
}

/// The steps of the map's filter by height, and what to say with them.
#[uniffi::export]
pub fn airspace_height_filter() -> AirspaceHeightFilter {
    let all = AirspaceHeightStep {
        ceiling_m: None,
        words: "All heights".to_owned(),
    };
    let steps = HEIGHT_STEPS_M.iter().map(|&ceiling_m| AirspaceHeightStep {
        ceiling_m: Some(ceiling_m),
        words: format!("Up to {ceiling_m:.0} m"),
    });
    AirspaceHeightFilter {
        steps: std::iter::once(all).chain(steps).collect(),
        note: "Leaves airspace that begins above the chosen height off the map. A floor \
               above ground or at a flight level is taken as low as it can be, so nothing \
               that could reach down to the height is left out. Only the map changes: \
               warnings in flight go by all airspace."
            .to_owned(),
    }
}

#[uniffi::export]
impl AirspaceSet {
    /// The airspaces that touch an area and begin at or below a height,
    /// for a map with the filter by height. `ceiling_m` is the height of
    /// interest in metres above mean sea level, one of
    /// [`airspace_height_filter`]'s steps; `None` gives everything, as
    /// [`AirspaceSet::in_area`] does.
    ///
    /// An airspace is left out only when its floor certainly lies above
    /// the height, wherever the ground is and whatever the weather: a
    /// floor above ground is taken as if the ground lay 500 m below the
    /// sea, as the warnings take it when they do not know the ground, and
    /// a flight level at the lowest the day's pressure can put it.
    pub fn in_area_below(&self, area: Bounds, ceiling_m: Option<f64>) -> Vec<AirspaceShape> {
        let Ok(area) = BoundingBox::new(
            area.south.clamp(-90.0, 90.0),
            area.west.clamp(-180.0, 180.0),
            area.north.clamp(-90.0, 90.0),
            area.east.clamp(-180.0, 180.0),
        ) else {
            return Vec::new();
        };
        // A height that is no number filters nothing: better too much on
        // the map than airspace missing from it.
        let band = ceiling_m
            .filter(|ceiling| !ceiling.is_nan())
            .map(|top_m| AltitudeBand {
                bottom_m: f64::NEG_INFINITY,
                top_m,
            });
        self.index
            .in_box(&area, band)
            .into_iter()
            .map(|airspace| shape(self, airspace))
            .collect()
    }

    /// What lies over a place: every airspace whose outline contains it,
    /// at whatever height, from the lowest floor up. For a sheet that a
    /// tap on the map opens.
    ///
    /// The filter by height does not apply: what a map leaves out is still
    /// listed here.
    pub fn over_place(&self, place: Coordinate) -> AirspaceOverPlace {
        let position = GeoPoint::new(place.lat, place.lon).ok();
        let mut found: Vec<&Airspace> = position
            .map(|position| self.index.at(position, &Altitudes::gps(0.0)))
            .unwrap_or_default()
            .into_iter()
            .map(|at| at.airspace)
            .collect();
        // As a pilot climbing would meet them, in the assistant's order;
        // of two that begin together the lower ceiling first.
        found.sort_by(|a, b| {
            let ceiling = |airspace: &Airspace| airspace.upper.metres().unwrap_or(f64::INFINITY);
            floor_order(a)
                .total_cmp(&floor_order(b))
                .then(ceiling(a).total_cmp(&ceiling(b)))
                .then_with(|| a.id.cmp(&b.id))
        });
        let airspaces: Vec<AirspaceOverhead> = found
            .into_iter()
            .map(|airspace| overhead(self, airspace))
            .collect();

        let covered = position.is_some_and(|position| self.covers(position));
        let words = match (airspaces.len(), covered) {
            (0, false) => "No airspace data for this place".to_owned(),
            (0, true) => "No airspace over this place in the app's data".to_owned(),
            (1, _) => "1 airspace over this place".to_owned(),
            (count, _) => format!("{count} airspaces over this place"),
        };
        let says_use = airspaces.iter().any(|airspace| {
            airspace.published_use.is_some()
                || !airspace.hours.is_empty()
                || airspace.hours_remarks.is_some()
        });
        AirspaceOverPlace {
            position: place,
            words,
            covered,
            cover_note: (!covered).then(|| {
                "The app's airspace data does not reach this place. What is listed is not \
                 the whole airspace there, and nothing listed does not mean free airspace."
                    .to_owned()
            }),
            hours_note: says_use.then(|| {
                "Hours and marks of use are shown as the source publishes them. For hours \
                 by weekday the source does not say whether they are UTC or local time. \
                 The app does not go by any of them: every airspace counts as active at \
                 any time."
                    .to_owned()
            }),
            caveat: format!("{} {}", CAVEATS[0], CAVEATS[1]),
            airspaces,
        }
    }
}

fn overhead(set: &AirspaceSet, airspace: &Airspace) -> AirspaceOverhead {
    let (policy, why) = verdict(airspace, &set.policy);
    let activity = &airspace.activity;
    let (origin, source, data_unix_s) = set.origin(airspace);
    AirspaceOverhead {
        id: airspace.id.to_string(),
        name: name_in_words(airspace),
        kind: kind_in_words(airspace),
        lower: limit_in_words(&airspace.lower),
        upper: limit_in_words(&airspace.upper),
        lower_at_least: airspace.lower_at_least.as_ref().map(limit_in_words),
        upper_at_most: airspace.upper_at_most.as_ref().map(limit_in_words),
        policy: policy_view(policy),
        policy_words: capitalised(policy_word(policy)),
        why: why.to_owned(),
        published_use: Some(published_use(activity))
            .filter(|marks| !marks.is_empty())
            .map(|marks| capitalised(&marks.join(", "))),
        hours: hours_in_words(&activity.hours),
        hours_remarks: as_published(&activity.hours_remarks),
        exists_from_unix_s: activity.from.map(|time| time.timestamp()),
        exists_until_unix_s: activity.until.map(|time| time.timestamp()),
        remarks: as_published(&airspace.remarks),
        origin,
        source,
        data_unix_s,
    }
}

/// A source's text as it is, without the space around it; `None` when it
/// says nothing.
fn as_published(text: &Option<String>) -> Option<String> {
    text.as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// Words with a capital at their start, for a line that begins with them.
fn capitalised(words: &str) -> String {
    let mut letters = words.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().chain(letters).collect(),
        None => String::new(),
    }
}

/// Published hours, a line for each run of days in a row that have the
/// same hours: "Mon to Fri 08:00 to 16:00", "Sat sunrise to sunset, when a
/// NOTAM says so". As published: nothing here knows whether the times are
/// UTC or local.
fn hours_in_words(hours: &[OperatingHours]) -> Vec<String> {
    let same = |a: &OperatingHours, b: &OperatingHours| {
        (a.start, a.end, a.by_notam, a.except_public_holidays)
            == (b.start, b.end, b.by_notam, b.except_public_holidays)
            && a.remarks == b.remarks
    };
    let time = |time: DayTime| match time {
        DayTime::Clock(clock) => clock.format("%H:%M").to_string(),
        DayTime::Sunrise => "sunrise".to_owned(),
        DayTime::Sunset => "sunset".to_owned(),
    };
    let mut lines = Vec::new();
    let mut rest = hours;
    while let Some(first) = rest.first() {
        let mut days = 1;
        while rest
            .get(days)
            .is_some_and(|next| same(first, next) && next.day == rest[days - 1].day.succ())
        {
            days += 1;
        }
        let last = &rest[days - 1];
        let mut line = if days == 1 {
            first.day.to_string()
        } else {
            format!("{} to {}", first.day, last.day)
        };
        line.push_str(&format!(" {} to {}", time(first.start), time(first.end)));
        if first.by_notam {
            line.push_str(", when a NOTAM says so");
        }
        if first.except_public_holidays {
            line.push_str(", not on public holidays");
        }
        if let Some(remarks) = first.remarks.as_deref().map(str::trim)
            && !remarks.is_empty()
        {
            line.push_str(&format!(" ({remarks})"));
        }
        lines.push(line);
        rest = &rest[days..];
    }
    lines
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{NaiveTime, Weekday};

    use super::*;
    use crate::Core;
    use crate::testing::TempFolder;

    const NOW: i64 = 1_795_000_000; // 18 November 2026

    /// Airspace stacked over one place, 47°16' N 011°21' E, in the order
    /// a file may have it: a control zone from the ground, a terminal area
    /// over it with activation times, a danger area that begins above
    /// ground, areas that begin at 3000 m, at 4000 m, at FL 100 and at
    /// FL 195, and a restricted area by NOTAM that lies elsewhere.
    const STACK: &str = "\
AC C
AN FL195 UP
AL FL 195
AH UNL
V X=47:16:00 N 011:21:00 E
DC 12

AC E
AN FL100 UP
AL FL 100
AH FL 195
V X=47:16:00 N 011:21:00 E
DC 11

AC D
AY TMA
AN TMA STACK
AL 5000ft AMSL
AH FL 125
AA Mon-Fri 0700-1700
V X=47:16:00 N 011:21:00 E
DC 8

AC Q
AN ED-D STACK
AL 1000ft AGL
AH FL 95
V X=47:16:00 N 011:21:00 E
DC 3

AC W
AN GLIDERS 4000
AL 4000m AMSL
AH FL 195
V X=47:16:00 N 011:21:00 E
DC 9

AC R
AN
AL 3000m AMSL
AH 3500m AMSL
V X=47:16:00 N 011:21:00 E
DC 4

AC D
AY CTR
AN CTR STACK
AL GND
AH 5000ft AMSL
V X=47:16:00 N 011:21:00 E
DC 5

AC R
AN ED-R ELSEWHERE
AA NONE
AL GND
AH FL 100
V X=48:30:00 N 012:30:00 E
DC 2
";

    const OVER: Coordinate = Coordinate {
        lat: 47.0 + 16.0 / 60.0,
        lon: 11.0 + 21.0 / 60.0,
    };

    fn stacked(folder: &TempFolder) -> Arc<AirspaceSet> {
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        let import = core
            .import_airspace("stack.txt".to_owned(), STACK.as_bytes().to_vec(), NOW)
            .unwrap();
        assert_eq!((import.airspaces, import.left_out), (8, 0), "{import:?}");
        core.airspace().unwrap()
    }

    #[test]
    fn what_lies_over_a_place_is_listed_from_the_lowest_floor_up() {
        let folder = TempFolder::new();
        let over = stacked(&folder).over_place(OVER);

        let names: Vec<&str> = over.airspaces.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "CTR STACK",
                "ED-D STACK",
                "TMA STACK",
                "Unnamed airspace",
                "FL100 UP",
                "GLIDERS 4000",
                "FL195 UP"
            ]
        );
        assert_eq!(over.words, "7 airspaces over this place");
        assert_eq!(over.position, OVER);
        // The file holds airspace here, so the list can speak for the place.
        assert!(over.covered);
        assert_eq!(over.cover_note, None);

        // Each as a sheet shows it: what it is, its limits as published,
        // how the app treats it and why, and where it is from.
        let zone = &over.airspaces[0];
        assert_eq!(zone.kind, "Class D control zone");
        assert_eq!(
            (zone.lower.as_str(), zone.upper.as_str()),
            ("GND", "5000 ft AMSL")
        );
        assert_eq!(
            (zone.policy, zone.policy_words.as_str()),
            (AirspacePolicy::Avoid, "Avoid")
        );
        assert!(zone.why.contains("clearance"), "{}", zone.why);
        assert_eq!(
            (zone.origin, zone.source.as_str()),
            (AirspaceOrigin::File, "stack")
        );
        assert!(zone.data_unix_s.is_some());
        assert_eq!((zone.published_use.as_deref(), zone.hours.len()), (None, 0));
        // The identifier is the one the shape on the map carries.
        let shapes = stacked(&folder).in_area(Bounds {
            south: 47.2,
            west: 11.3,
            north: 47.3,
            east: 11.4,
        });
        assert!(shapes.iter().any(|shape| shape.id == zone.id));

        let danger = &over.airspaces[1];
        assert_eq!(
            (
                danger.kind.as_str(),
                danger.lower.as_str(),
                danger.upper.as_str()
            ),
            ("Danger area", "1000 ft AGL", "FL 95")
        );
        // What a paraglider may fly through is listed too: it says how
        // high the open air reaches.
        let open = &over.airspaces[4];
        assert_eq!(
            (open.kind.as_str(), open.policy, open.policy_words.as_str()),
            ("Class E airspace", AirspacePolicy::Free, "Free")
        );
    }

    #[test]
    fn what_the_source_says_about_use_is_shown_as_published_and_never_evaluated() {
        let folder = TempFolder::new();
        let set = stacked(&folder);
        let over = set.over_place(OVER);

        // Activation times of an OpenAir file are kept as its text.
        let area = &over.airspaces[2];
        assert_eq!(area.name, "TMA STACK");
        assert_eq!(
            area.hours_remarks.as_deref(),
            Some("Activation times in the file (UTC): Mon-Fri 0700-1700")
        );
        let note = over.hours_note.as_deref().unwrap();
        assert!(note.contains("as the source publishes them"), "{note}");
        assert!(note.contains("counts as active at any time"), "{note}");
        // Always with the list: not certified, and nothing known of today.
        assert!(over.caveat.contains("not certified"), "{}", over.caveat);
        assert!(over.caveat.contains("not a clearance"), "{}", over.caveat);
        assert!(
            over.caveat
                .contains("Nothing here says what is active today"),
            "{}",
            over.caveat
        );

        // "By NOTAM" is a mark of the record, in the assistant's words.
        let elsewhere = set.over_place(Coordinate {
            lat: 48.5,
            lon: 12.5,
        });
        assert_eq!(elsewhere.words, "1 airspace over this place");
        assert_eq!(
            elsewhere.airspaces[0].published_use.as_deref(),
            Some("Active by NOTAM")
        );
        assert!(elsewhere.hours_note.is_some());
    }

    #[test]
    fn a_place_without_airspace_says_whether_that_is_no_airspace_or_no_data() {
        let folder = TempFolder::new();
        let set = stacked(&folder);

        // Between the two: the file holds airspace within a hundred
        // kilometres, so nothing listed is what the file says.
        let between = set.over_place(Coordinate {
            lat: 47.9,
            lon: 12.0,
        });
        assert!(between.airspaces.is_empty());
        assert!(between.covered);
        assert_eq!(
            between.words,
            "No airspace over this place in the app's data"
        );
        assert_eq!((between.cover_note, between.hours_note), (None, None));
        assert!(between.caveat.contains("not certified"));

        // Far from any of it, and at a place that is none: no data, which
        // is not free air.
        for nowhere in [
            Coordinate {
                lat: 52.5,
                lon: 13.4,
            },
            Coordinate {
                lat: f64::NAN,
                lon: 0.0,
            },
        ] {
            let far = set.over_place(nowhere);
            assert!(far.airspaces.is_empty());
            assert!(!far.covered);
            assert_eq!(far.words, "No airspace data for this place");
            let note = far.cover_note.unwrap();
            assert!(note.contains("does not mean free airspace"), "{note}");
        }

        // A core without any airspace answers the same way.
        let empty = TempFolder::new();
        let core = Core::new(empty.path().to_str().unwrap().to_owned()).unwrap();
        let none = core.airspace().unwrap().over_place(OVER);
        assert_eq!(none.words, "No airspace data for this place");
        assert!(!none.covered);
    }

    #[test]
    fn the_height_filter_leaves_out_only_what_certainly_begins_above() {
        let folder = TempFolder::new();
        let set = stacked(&folder);
        let around = Bounds {
            south: 47.0,
            west: 11.0,
            north: 47.5,
            east: 11.7,
        };
        let below = |ceiling_m: Option<f64>| -> Vec<String> {
            let mut names: Vec<String> = set
                .in_area_below(around, ceiling_m)
                .into_iter()
                .map(|shape| shape.name)
                .collect();
            names.sort();
            names
        };

        // Without a height, everything: what `in_area` gives.
        let all = below(None);
        assert_eq!(all.len(), 7);
        assert_eq!(set.in_area(around).len(), 7);
        assert_eq!(below(Some(f64::NAN)), all);
        assert_eq!(below(Some(f64::INFINITY)), all);

        // Up to 4500 m: FL 195 begins far above on any day. The area from
        // 4000 m stays.
        assert_eq!(
            below(Some(4500.0)),
            [
                "",
                "CTR STACK",
                "ED-D STACK",
                "FL100 UP",
                "GLIDERS 4000",
                "TMA STACK"
            ]
        );
        // Up to 3500 m the area from 4000 m goes. FL 100 is nominally
        // 3048 m and stays.
        assert_eq!(
            below(Some(3500.0)),
            ["", "CTR STACK", "ED-D STACK", "FL100 UP", "TMA STACK"]
        );
        // Up to 2500 m: the area from 3000 m goes, and FL 100, which low
        // pressure and cold air bring down to 2573 m and no lower. The
        // danger area's floor is above ground, and the ground is not
        // known: it stays at every step.
        assert_eq!(
            below(Some(2500.0)),
            ["CTR STACK", "ED-D STACK", "TMA STACK"]
        );
        // Where low pressure can bring FL 100, it is drawn.
        assert!(below(Some(2600.0)).contains(&"FL100 UP".to_owned()));
        // The shapes are the map's own: the filter changes which, not what.
        let zone = &set.in_area_below(around, Some(2500.0))[0];
        assert!(!zone.outline.is_empty());
        assert_eq!(
            set.in_area(around)
                .into_iter()
                .find(|shape| shape.id == zone.id)
                .as_ref(),
            Some(zone)
        );

        // What the map leaves out is still listed over a place.
        assert_eq!(set.over_place(OVER).airspaces.len(), 7);
    }

    #[test]
    fn the_filter_offers_all_heights_and_three_steps_with_what_it_does() {
        let filter = airspace_height_filter();
        let steps: Vec<(Option<f64>, &str)> = filter
            .steps
            .iter()
            .map(|step| (step.ceiling_m, step.words.as_str()))
            .collect();
        assert_eq!(
            steps,
            [
                (None, "All heights"),
                (Some(4500.0), "Up to 4500 m"),
                (Some(3500.0), "Up to 3500 m"),
                (Some(2500.0), "Up to 2500 m"),
            ]
        );
        assert!(
            filter.note.contains("Only the map changes"),
            "{}",
            filter.note
        );
    }

    #[test]
    fn published_hours_are_written_a_line_for_each_run_of_days() {
        let clock =
            |hour, minute| DayTime::Clock(NaiveTime::from_hms_opt(hour, minute, 0).unwrap());
        let day = |day: Weekday, start: DayTime, end: DayTime| OperatingHours {
            day,
            start,
            end,
            by_notam: false,
            except_public_holidays: false,
            remarks: None,
        };
        let mut hours: Vec<OperatingHours> = [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
        ]
        .into_iter()
        .map(|weekday| day(weekday, clock(8, 0), clock(16, 30)))
        .collect();
        hours.push(OperatingHours {
            by_notam: true,
            except_public_holidays: true,
            remarks: Some(" gliding only ".to_owned()),
            ..day(Weekday::Sat, DayTime::Sunrise, DayTime::Sunset)
        });
        hours.push(day(Weekday::Sun, clock(8, 0), clock(16, 30)));

        assert_eq!(
            hours_in_words(&hours),
            [
                "Mon to Fri 08:00 to 16:30",
                "Sat sunrise to sunset, when a NOTAM says so, not on public holidays (gliding only)",
                "Sun 08:00 to 16:30",
            ]
        );
        // The same hours on days that are not in a row are not a run.
        let apart = [
            day(Weekday::Mon, clock(9, 0), clock(12, 0)),
            day(Weekday::Wed, clock(9, 0), clock(12, 0)),
        ];
        assert_eq!(
            hours_in_words(&apart),
            ["Mon 09:00 to 12:00", "Wed 09:00 to 12:00"]
        );
        assert!(hours_in_words(&[]).is_empty());
    }
}
