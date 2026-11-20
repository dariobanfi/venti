//! Two flights side by side.
//!
//! A pilot looks at a flight next to another: one of their own, or one
//! another pilot shows to everyone. Flights that were in the air at the
//! same time are laid over each other by the clock, so that the two
//! gliders are where they were at the same moment; flights that were not
//! are laid out from their first fixes.
//!
//! The shell draws the second track and its altitude; the core says how
//! the two lie to each other in time, how near they came, and what there
//! is to compare in numbers.

use venti_domain::GeoPoint;

use crate::{FlightAnalysis, FlightView, TrackPoint};

/// Two flights count as flown together when both were in the air for at
/// least this long at the same time.
const TOGETHER_AT_LEAST_S: i64 = 60;

/// How near the two gliders came is looked for at moments this far apart.
/// A paraglider moves some fifty metres in the time.
const NEARNESS_STEP_S: i64 = 5;

/// Who is ahead in one line of the comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum ComparisonLead {
    /// The flight the pilot opened.
    First,
    /// The flight laid beside it.
    Other,
    /// Neither, or the line is not one to be ahead in.
    Neither,
}

/// One line of the comparison: the same number of both flights.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ComparisonRow {
    /// What the number is: "Airtime".
    pub label: String,
    /// The opened flight's, with its unit: "2 h 14 min".
    pub first: String,
    /// The other flight's.
    pub other: String,
    /// Which of the two is ahead.
    pub lead: ComparisonLead,
}

/// The moment the two gliders were nearest to each other.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct NearestMoment {
    /// Seconds since the opened flight's first fix.
    pub seconds: u32,
    /// How far apart they were over the ground, metres.
    pub distance_m: f64,
    /// How much higher the other glider was, metres. Negative when it was
    /// lower.
    pub other_higher_m: f64,
}

/// How two flights lie to each other, and what there is to compare.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FlightComparison {
    /// Whether the two are laid over each other by the clock: they were
    /// in the air at the same time. Otherwise each begins at its first
    /// fix.
    pub by_the_clock: bool,
    /// Where the other flight's first fix lies on the opened flight's
    /// time, seconds since the opened flight's first fix. Negative when
    /// the other began earlier; zero when they are not laid out by the
    /// clock. A moment `s` of the opened flight is the moment
    /// `s - offset_s` of the other.
    pub offset_s: i64,
    /// How long both were in the air at the same time, seconds.
    pub together_s: i64,
    /// The moment they were nearest, when they flew at the same time.
    pub nearest: Option<NearestMoment>,
    /// How the two lie to each other, in a sentence or two.
    pub words: String,
    /// The lines of the comparison.
    pub rows: Vec<ComparisonRow>,
}

/// Compares a flight with another. The analyses add what is known of the
/// thermals and glides of each; without them those lines are left out.
#[uniffi::export]
pub fn compare_flights(
    first: FlightView,
    other: FlightView,
    first_analysis: Option<FlightAnalysis>,
    other_analysis: Option<FlightAnalysis>,
) -> FlightComparison {
    let start = first.start_unix_s.max(other.start_unix_s);
    let end = (first.start_unix_s + first.duration_s).min(other.start_unix_s + other.duration_s);
    let together_s = (end - start).max(0);
    let by_the_clock = together_s >= TOGETHER_AT_LEAST_S;
    let offset_s = if by_the_clock {
        other.start_unix_s - first.start_unix_s
    } else {
        0
    };
    let nearest = by_the_clock
        .then(|| nearest_moment(&first, &other, offset_s))
        .flatten();

    let words = if by_the_clock {
        let mut words = format!("In the air together for {}.", duration(together_s));
        if let Some(nearest) = &nearest {
            words.push_str(&format!(
                " Nearest to each other {} apart, the other {}.",
                distance(nearest.distance_m),
                height_between(nearest.other_higher_m)
            ));
        }
        words
    } else {
        format!(
            "{} Each is laid out from its first moment.",
            apart(first.start_unix_s, other.start_unix_s)
        )
    };

    FlightComparison {
        by_the_clock,
        offset_s,
        together_s,
        nearest,
        words,
        rows: rows(
            &first,
            &other,
            first_analysis.as_ref(),
            other_analysis.as_ref(),
            by_the_clock,
        ),
    }
}

/// The lines of the comparison.
fn rows(
    first: &FlightView,
    other: &FlightView,
    first_analysis: Option<&FlightAnalysis>,
    other_analysis: Option<&FlightAnalysis>,
    by_the_clock: bool,
) -> Vec<ComparisonRow> {
    let mut rows = Vec::new();
    // A line where more is ahead.
    let more = |rows: &mut Vec<ComparisonRow>,
                label: &str,
                one: f64,
                two: f64,
                said: &dyn Fn(f64) -> String| {
        let (first, other) = (said(one), said(two));
        // Ahead only where it shows: numbers that are said the same are even.
        let lead = if first == other {
            ComparisonLead::Neither
        } else if one > two {
            ComparisonLead::First
        } else {
            ComparisonLead::Other
        };
        rows.push(ComparisonRow {
            label: label.to_owned(),
            first,
            other,
            lead,
        });
    };

    if by_the_clock && first.start_unix_s != other.start_unix_s {
        let gap = duration((first.start_unix_s - other.start_unix_s).abs());
        let (one, two) = if first.start_unix_s < other.start_unix_s {
            (format!("{gap} earlier"), format!("{gap} later"))
        } else {
            (format!("{gap} later"), format!("{gap} earlier"))
        };
        rows.push(ComparisonRow {
            label: "First fix".to_owned(),
            first: one,
            other: two,
            lead: ComparisonLead::Neither,
        });
    }
    #[allow(clippy::cast_precision_loss)]
    more(
        &mut rows,
        "Airtime",
        airtime(first, first_analysis) as f64,
        airtime(other, other_analysis) as f64,
        &|seconds| duration(seconds.round() as i64),
    );
    let score = |analysis: Option<&FlightAnalysis>| {
        analysis
            .and_then(|analysis| analysis.score.as_ref())
            .map(|score| score.distance_m)
    };
    if let (Some(one), Some(two)) = (score(first_analysis), score(other_analysis)) {
        more(&mut rows, "Scored distance", one, two, &kilometres);
    }
    more(
        &mut rows,
        "Along the track",
        first.track_length_m,
        other.track_length_m,
        &kilometres,
    );
    more(
        &mut rows,
        "Highest",
        first.max_altitude_m,
        other.max_altitude_m,
        &metres,
    );
    more(
        &mut rows,
        "Best climb",
        first.max_climb_ms,
        other.max_climb_ms,
        &|climb| format!("{climb:.1} m/s"),
    );
    if let (Some(one), Some(two)) = (first_analysis, other_analysis) {
        if let (Some(one), Some(two)) = (thermal_climb(one), thermal_climb(two)) {
            more(&mut rows, "Climb in thermals", one, two, &|climb| {
                format!("{climb:.1} m/s")
            });
        }
        more(
            &mut rows,
            "Gained in thermals",
            one.height_gained_m,
            two.height_gained_m,
            &metres,
        );
        if let (Some(one), Some(two)) = (glide_ratio(one), glide_ratio(two)) {
            more(&mut rows, "Glide on glides", one, two, &|ratio| {
                format!("{ratio:.1}")
            });
        }
        if let (Some(one), Some(two)) = (circling_share(one), circling_share(two)) {
            let said = |share: f64| format!("{:.0} %", share * 100.0);
            rows.push(ComparisonRow {
                label: "Time circling".to_owned(),
                first: said(one),
                other: said(two),
                // Neither more nor less circling is better in itself.
                lead: ComparisonLead::Neither,
            });
        }
    }
    rows
}

/// The time in the air: from take-off to landing where the analysis
/// found them, else the length of the recording.
fn airtime(flight: &FlightView, analysis: Option<&FlightAnalysis>) -> i64 {
    analysis
        .filter(|analysis| analysis.takeoff.is_some())
        .map_or(flight.duration_s, |analysis| analysis.airtime_s)
}

/// The average climb over all thermals: height gained by time circling.
fn thermal_climb(analysis: &FlightAnalysis) -> Option<f64> {
    #[allow(clippy::cast_precision_loss)]
    (analysis.thermalling_s >= 60).then(|| analysis.height_gained_m / analysis.thermalling_s as f64)
}

/// The glide ratio over all glides that lost height: distance by height.
fn glide_ratio(analysis: &FlightAnalysis) -> Option<f64> {
    let (mut distance, mut lost) = (0.0, 0.0);
    for glide in &analysis.glides {
        if glide.height_lost_m > 0.0 {
            distance += glide.distance_m;
            lost += glide.height_lost_m;
        }
    }
    (lost >= 100.0).then(|| distance / lost)
}

/// The part of the airtime spent circling.
fn circling_share(analysis: &FlightAnalysis) -> Option<f64> {
    #[allow(clippy::cast_precision_loss)]
    (analysis.airtime_s >= 60).then(|| analysis.thermalling_s as f64 / analysis.airtime_s as f64)
}

/// The moment of the opened flight at which the other glider was nearest
/// over the ground, among moments a few seconds apart.
fn nearest_moment(first: &FlightView, other: &FlightView, offset_s: i64) -> Option<NearestMoment> {
    let from = offset_s.max(0);
    let to = first.duration_s.min(offset_s + other.duration_s);
    let mut nearest: Option<NearestMoment> = None;
    let (mut mine, mut theirs) = (0, 0);
    let mut second = from;
    while second <= to {
        let seconds = u32::try_from(second).ok()?;
        let theirs_seconds = u32::try_from(second - offset_s).ok()?;
        mine = at(&first.track, mine, seconds);
        theirs = at(&other.track, theirs, theirs_seconds);
        let (one, two) = (first.track.get(mine)?, other.track.get(theirs)?);
        if let (Ok(here), Ok(there)) = (
            GeoPoint::new(one.lat, one.lon),
            GeoPoint::new(two.lat, two.lon),
        ) {
            let distance_m = here.distance_m(there);
            if nearest.is_none_or(|nearest| distance_m < nearest.distance_m) {
                nearest = Some(NearestMoment {
                    seconds,
                    distance_m,
                    other_higher_m: two.altitude_m - one.altitude_m,
                });
            }
        }
        second += NEARNESS_STEP_S;
    }
    nearest
}

/// The last point of a track at or before a moment, looked for from
/// where the search before ended: the moments asked for only go forward.
fn at(track: &[TrackPoint], from: usize, seconds: u32) -> usize {
    let mut index = from;
    while track
        .get(index + 1)
        .is_some_and(|next| next.seconds <= seconds)
    {
        index += 1;
    }
    index
}

/// "2 h 14 min", "14 min", "40 s".
fn duration(seconds: i64) -> String {
    let minutes = (seconds + 30) / 60;
    if seconds < 60 {
        format!("{seconds} s")
    } else if minutes < 60 {
        format!("{minutes} min")
    } else {
        format!("{} h {:02} min", minutes / 60, minutes % 60)
    }
}

/// "45.2 km".
fn kilometres(metres: f64) -> String {
    format!("{:.1} km", metres / 1000.0)
}

/// "2412 m": a height has no mark between the thousands, as the design
/// language and an altimeter write it, and a loss has the true minus.
fn metres(metres: f64) -> String {
    let whole = metres.round() as i64;
    format!(
        "{}{} m",
        if whole < 0 { "\u{2212}" } else { "" },
        whole.abs()
    )
}

/// "85 m" up to a kilometre, then "1.2 km".
fn distance(metres_apart: f64) -> String {
    if metres_apart < 1000.0 {
        metres((metres_apart / 5.0).round() * 5.0)
    } else {
        kilometres(metres_apart)
    }
}

/// "120 m higher", "40 m lower", "at the same height".
fn height_between(other_higher_m: f64) -> String {
    let rounded = (other_higher_m / 10.0).round() * 10.0;
    if rounded > 0.0 {
        format!("{} higher", metres(rounded))
    } else if rounded < 0.0 {
        format!("{} lower", metres(-rounded))
    } else {
        "at the same height".to_owned()
    }
}

/// How far apart in time two flights began: "Flown 3 days apart.",
/// "Flown on the same day, not at the same time."
fn apart(first_unix_s: i64, other_unix_s: i64) -> String {
    let days = ((first_unix_s - other_unix_s).abs() + 43_200) / 86_400;
    match days {
        0 => "Flown on the same day, not at the same time.".to_owned(),
        1 => "Flown a day apart.".to_owned(),
        days if days < 60 => format!("Flown {days} days apart."),
        days if days < 730 => format!("Flown {} months apart.", (days + 15) / 30),
        days => format!("Flown {} years apart.", (days + 182) / 365),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Bounds, flight_analysis_from_file, flight_from_file};

    /// The sample flight, and its analysis.
    fn sample() -> (FlightView, FlightAnalysis) {
        let text = venti_flight::igc::write(&venti_flight::testing::synthetic_flight());
        (
            flight_from_file(text.clone()).unwrap(),
            flight_analysis_from_file(text).unwrap(),
        )
    }

    /// The same flight begun later and flown elsewhere: moved in time, to
    /// the east and up.
    fn moved(flight: &FlightView, later_s: i64, east_deg: f64, higher_m: f64) -> FlightView {
        let mut other = flight.clone();
        other.start_unix_s += later_s;
        other.max_altitude_m += higher_m;
        other.min_altitude_m += higher_m;
        for point in &mut other.track {
            point.lon += east_deg;
            point.altitude_m += higher_m;
        }
        other.bounds = Bounds {
            west: flight.bounds.west + east_deg,
            east: flight.bounds.east + east_deg,
            ..flight.bounds
        };
        other
    }

    fn row<'a>(comparison: &'a FlightComparison, label: &str) -> &'a ComparisonRow {
        comparison
            .rows
            .iter()
            .find(|row| row.label == label)
            .unwrap_or_else(|| panic!("no line {label:?} among {:?}", comparison.rows))
    }

    #[test]
    fn flights_in_the_air_together_are_laid_out_by_the_clock() {
        let (first, analysis) = sample();
        // Ten minutes later, 0.002 degrees (150 m) to the east, 80 m up.
        let other = moved(&first, 600, 0.002, 80.0);

        let comparison =
            compare_flights(first.clone(), other, Some(analysis.clone()), Some(analysis));

        assert!(comparison.by_the_clock);
        assert_eq!(comparison.offset_s, 600);
        assert_eq!(comparison.together_s, first.duration_s - 600);
        assert!(
            comparison.words.starts_with("In the air together for "),
            "{}",
            comparison.words
        );
        // The other is where this one was ten minutes before, so they are
        // nearest where the flight comes back over its own track or
        // lingers; never nearer than the 150 m the other was moved by
        // while it circles in one place, give or take.
        let nearest = comparison.nearest.unwrap();
        assert!(nearest.seconds >= 600, "{nearest:?}");
        assert!(i64::from(nearest.seconds) <= first.duration_s);
        assert!(nearest.distance_m < 1500.0, "{nearest:?}");
        assert!(comparison.words.contains("Nearest to each other"));

        let fix = row(&comparison, "First fix");
        assert_eq!(
            (fix.first.as_str(), fix.other.as_str()),
            ("10 min earlier", "10 min later")
        );
        assert_eq!(fix.lead, ComparisonLead::Neither);
        // The same flight: even where nothing was changed, and the other
        // ahead where it was moved up.
        assert_eq!(row(&comparison, "Airtime").lead, ComparisonLead::Neither);
        assert_eq!(
            row(&comparison, "Scored distance").lead,
            ComparisonLead::Neither
        );
        assert_eq!(row(&comparison, "Highest").lead, ComparisonLead::Other);
        assert!(row(&comparison, "Highest").other.ends_with(" m"));
        for label in [
            "Along the track",
            "Best climb",
            "Climb in thermals",
            "Gained in thermals",
            "Glide on glides",
            "Time circling",
        ] {
            assert_eq!(
                row(&comparison, label).lead,
                ComparisonLead::Neither,
                "{label}"
            );
        }
        assert!(row(&comparison, "Time circling").first.ends_with(" %"));
    }

    #[test]
    fn the_same_flight_beside_itself_is_nowhere_apart() {
        let (first, _) = sample();

        let comparison = compare_flights(first.clone(), first.clone(), None, None);

        assert!(comparison.by_the_clock);
        assert_eq!(comparison.offset_s, 0);
        assert_eq!(comparison.together_s, first.duration_s);
        let nearest = comparison.nearest.unwrap();
        assert_eq!(nearest.distance_m, 0.0);
        assert_eq!(nearest.other_higher_m, 0.0);
        assert!(
            comparison
                .words
                .contains("0 m apart, the other at the same height")
        );
        // Without the analyses the lines that need them are left out, and
        // two flights that began at the same second have no line for it.
        let labels: Vec<&str> = comparison
            .rows
            .iter()
            .map(|row| row.label.as_str())
            .collect();
        assert_eq!(
            labels,
            ["Airtime", "Along the track", "Highest", "Best climb"]
        );
    }

    #[test]
    fn flights_of_different_days_are_laid_out_from_their_first_moments() {
        let (first, analysis) = sample();
        let other = moved(&first, 3 * 86_400 + 1200, 0.0, -300.0);

        let comparison = compare_flights(first, other, Some(analysis), None);

        assert!(!comparison.by_the_clock);
        assert_eq!(comparison.offset_s, 0);
        assert_eq!(comparison.together_s, 0);
        assert_eq!(comparison.nearest, None);
        assert_eq!(
            comparison.words,
            "Flown 3 days apart. Each is laid out from its first moment."
        );
        assert!(comparison.rows.iter().all(|row| row.label != "First fix"));
        assert_eq!(row(&comparison, "Highest").lead, ComparisonLead::First);
        // One analysis is not enough for the lines that compare two.
        assert!(
            comparison
                .rows
                .iter()
                .all(|row| row.label != "Climb in thermals")
        );
    }

    #[test]
    fn numbers_are_said_as_the_apps_say_them() {
        assert_eq!(duration(40), "40 s");
        assert_eq!(duration(14 * 60 + 20), "14 min");
        assert_eq!(duration(2 * 3600 + 14 * 60), "2 h 14 min");
        assert_eq!(duration(3600 + 5 * 60), "1 h 05 min");
        assert_eq!(kilometres(45_230.0), "45.2 km");
        assert_eq!(metres(2412.4), "2412 m");
        assert_eq!(metres(812.0), "812 m");
        assert_eq!(metres(12_345_678.0), "12345678 m");
        assert_eq!(metres(-135.0), "\u{2212}135 m");
        assert_eq!(distance(83.0), "85 m");
        assert_eq!(distance(1240.0), "1.2 km");
        assert_eq!(height_between(124.0), "120 m higher");
        assert_eq!(height_between(-38.0), "40 m lower");
        assert_eq!(height_between(3.0), "at the same height");
        assert_eq!(
            apart(0, 3600),
            "Flown on the same day, not at the same time."
        );
        assert_eq!(apart(0, 86_400), "Flown a day apart.");
        assert_eq!(apart(0, 45 * 86_400), "Flown 45 days apart.");
        assert_eq!(apart(0, 200 * 86_400), "Flown 7 months apart.");
        assert_eq!(apart(3 * 365 * 86_400, 0), "Flown 3 years apart.");
    }
}
