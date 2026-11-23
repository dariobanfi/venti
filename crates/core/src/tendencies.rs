//! What the pilot's own tracks say about how they fly: which way they
//! circle, how their thermals end, how they climb and glide, and where
//! their flights end.
//!
//! The figures of each flight are [`venti_flight::habits`], kept with the
//! flight in the logbook's index. Here they are added up over the whole
//! logbook and said in words, each observation with a flight that shows it
//! most, so that the pilot can look at it. The words say what was measured
//! and nothing else: no verdict on the pilot, no advice, nothing about what
//! is safe. An observation is made only when there is enough flying behind
//! it; the minimums below are to verify with pilots.

use venti_api_types::v1::{self as api, HabitsResponse};
use venti_flight::habits::FlightHabits;

use crate::logbook::FlightWithHabits;
use crate::server::answer;
use crate::{Core, CoreError, ServerRequest};

/// Which way the pilot circles is said from this many thermals on.
const MIN_THERMALS: u32 = 10;
/// How thermals end, and the time after their top, are said from this
/// many measured thermals on.
const MIN_MEASURED_THERMALS: u32 = 10;
/// The flight with the longest time after the top is chosen among those
/// with at least this many measured thermals: one thermal is not a habit.
const MIN_MEASURED_IN_A_FLIGHT: u32 = 3;
/// The glides are said from this many glides of a kilometre on.
const MIN_GLIDES: u32 = 10;
/// Where flights end is said from this many flights on.
const MIN_FLIGHTS: u32 = 10;
/// A flight that ended closer than this to where it began, metres, ended
/// where it began: on the take-off's own landing field, or back at the
/// hill. A straight line, so a flight out and back counts too.
const ENDED_NEAR_M: f64 = 1000.0;
/// The share of the airtime spent circling is said from this much
/// airtime on, seconds: two hours.
const MIN_AIRTIME_S: i64 = 2 * 3600;

/// What one observation is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum TendencyKind {
    /// Which way the pilot circles.
    Circling,
    /// How the pilot's thermals end.
    ThermalEnds,
    /// The time between a thermal's highest point and leaving it.
    AfterTheTop,
    /// How far below a thermal's highest point it is left.
    BelowTheTop,
    /// How the pilot climbs.
    Climb,
    /// How the pilot glides.
    Glides,
    /// The share of the airtime spent circling.
    TimeCircling,
    /// How far from where they began flights end.
    Landings,
}

/// One thing the pilot's tracks show.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct Tendency {
    /// What it is about.
    pub kind: TendencyKind,
    /// Its heading: "Which way you circle".
    pub title: String,
    /// What was measured, in a sentence.
    pub words: String,
    /// Where the observation is a share, that share, from 0 to 1, for a
    /// bar.
    pub share: Option<f64>,
    /// A flight that shows it most, to open.
    pub flight_id: Option<String>,
    /// When that flight started, seconds since 1970-01-01 UTC.
    pub flight_unix_s: Option<i64>,
}

/// What the pilot's own tracks say about how they fly.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct Tendencies {
    /// The flights counted.
    pub flights: u32,
    /// The thermals in them.
    pub thermals: u32,
    /// What there is enough flying to say, in a fixed order.
    pub observations: Vec<Tendency>,
    /// What this is, for the foot of a screen.
    pub note: String,
}

/// What [`Tendencies::note`] says.
const NOTE: &str = "Counted from your own GPS tracks by fixed rules: a thermal is circled one \
     way when it turned at least one full circle with two thirds of its turning that way, how a thermal ends is \
     measured on those of two minutes or more, and glides from a kilometre. These are \
     observations of what was flown, not a judgement of you or of what is safe.";

#[uniffi::export]
impl Core {
    /// What the pilot's own tracks say about how they fly, over every
    /// flight of the logbook.
    ///
    /// Flights imported before the core kept their figures get them now,
    /// from their files, once. A flight whose file cannot be read now is
    /// left out; in a browser it is then among the files wanted, and the
    /// next call counts it.
    pub fn tendencies(&self) -> Tendencies {
        tendencies(&self.logbook.habits())
    }
}

/// The request for how each of the pilot's flights on the server was
/// flown, the private ones too, for a shell that keeps no logbook of its
/// own: the website (owner decision 20). The answer is read by
/// [`own_tendencies`].
#[uniffi::export]
pub fn own_habits_request() -> ServerRequest {
    ServerRequest {
        method: "GET".to_owned(),
        path: api::ACCOUNT_HABITS_PATH.to_owned(),
        body: None,
        content_type: None,
        needs_token: true,
    }
}

/// What the pilot's flights on the server say about how they fly, from
/// the answer to [`own_habits_request`], by the same rules and in the
/// same words as [`Core::tendencies`] over a logbook on the device. A
/// flight the server has not worked out yet is not counted.
#[uniffi::export]
pub fn own_tendencies(status: u16, body: Vec<u8>) -> Result<Tendencies, CoreError> {
    let response: HabitsResponse = answer(status, &body)?;
    let mut flights: Vec<FlightWithHabits> = response
        .flights
        .into_iter()
        .map(|flight| FlightWithHabits {
            id: flight.id.to_string(),
            start_unix_s: flight.start.timestamp(),
            habits: FlightHabits {
                thermals_left: flight.thermals_left,
                thermals_right: flight.thermals_right,
                thermals_both_ways: flight.thermals_both_ways,
                circling_left_s: flight.circling_left_s,
                circling_right_s: flight.circling_right_s,
                thermals_measured: flight.thermals_measured,
                thermals_ended_weak: flight.thermals_ended_weak,
                after_the_top_s: flight.after_the_top_s,
                below_the_top_m: flight.below_the_top_m,
                height_gained_m: flight.height_gained_m,
                thermalling_s: flight.thermalling_s,
                best_climb_ms: flight.best_climb_ms,
                glides: flight.glides,
                glide_distance_m: flight.glide_distance_m,
                glide_height_lost_m: flight.glide_height_lost_m,
                gliding_s: flight.gliding_s,
                airtime_s: flight.airtime_s,
            },
            ended_from_start_m: flight.ended_from_start_m,
        })
        .collect();
    // Newest first, as the logbook's are, whatever order the answer has.
    flights.sort_by(|a, b| (b.start_unix_s, &b.id).cmp(&(a.start_unix_s, &a.id)));
    Ok(tendencies(&flights))
}

/// The tendencies of these flights.
fn tendencies(flights: &[FlightWithHabits]) -> Tendencies {
    let mut all = FlightHabits::default();
    for flight in flights {
        all += flight.habits;
    }
    let observations = [
        circling(flights, &all),
        thermal_ends(flights, &all),
        after_the_top(flights, &all),
        below_the_top(flights, &all),
        climb(flights, &all),
        glides(&all),
        time_circling(&all),
        landings(flights),
    ]
    .into_iter()
    .flatten()
    .collect();
    Tendencies {
        flights: flights.len() as u32,
        thermals: all.thermals(),
        observations,
        note: NOTE.to_owned(),
    }
}

/// The flight with the highest figure above zero; of flights with the same
/// figure, the newest. `flights` are newest first.
fn most(
    flights: &[FlightWithHabits],
    figure: impl Fn(&FlightHabits) -> Option<f64>,
) -> Option<&FlightWithHabits> {
    let mut best: Option<(&FlightWithHabits, f64)> = None;
    for flight in flights {
        if let Some(value) = figure(&flight.habits).filter(|value| *value > 0.0)
            && best.is_none_or(|(_, most)| value > most)
        {
            best = Some((flight, value));
        }
    }
    best.map(|(flight, _)| flight)
}

fn observation(
    kind: TendencyKind,
    title: &str,
    words: String,
    share: Option<f64>,
    flight: Option<&FlightWithHabits>,
) -> Tendency {
    Tendency {
        kind,
        title: title.to_owned(),
        words,
        share,
        flight_id: flight.map(|flight| flight.id.clone()),
        flight_unix_s: flight.map(|flight| flight.start_unix_s),
    }
}

/// "Of 409 thermals circled one way, 337 went right and 72 left. 252 more
/// were flown back and forth or were too short to say." The flight is the
/// one with the most thermals circled the rarer way.
fn circling(flights: &[FlightWithHabits], all: &FlightHabits) -> Option<Tendency> {
    let (left, right, rest) = (
        all.thermals_left,
        all.thermals_right,
        all.thermals_both_ways,
    );
    // Only thermals circled one way say which way.
    let one_way = left + right;
    if one_way < MIN_THERMALS {
        return None;
    }
    let ((more, more_count), (fewer, fewer_count)) = if right >= left {
        (("right", right), ("left", left))
    } else {
        (("left", left), ("right", right))
    };
    let mut words = if fewer_count == 0 {
        format!(
            "All {} thermals circled one way went {more}.",
            count(one_way)
        )
    } else {
        format!(
            "Of {} thermals circled one way, {} went {more} and {} {fewer}.",
            count(one_way),
            count(more_count),
            count(fewer_count)
        )
    };
    if rest > 0 {
        words += &format!(
            " {} more {} flown back and forth or were too short to say.",
            count(rest),
            if rest == 1 { "was" } else { "were" }
        );
    }
    let flight = if left == right {
        None
    } else if left < right {
        most(flights, |habits| Some(f64::from(habits.thermals_left)))
    } else {
        most(flights, |habits| Some(f64::from(habits.thermals_right)))
    };
    Some(observation(
        TendencyKind::Circling,
        "Which way you circle",
        words,
        // The share of the way that leads the sentence, so that the bar
        // and the first figure say the same.
        Some(f64::from(more_count) / f64::from(one_way)),
        flight,
    ))
}

/// "In 12 of 34 thermals the last minute climbed at less than a third of
/// the thermal's average."
fn thermal_ends(flights: &[FlightWithHabits], all: &FlightHabits) -> Option<Tendency> {
    let (measured, weak) = (all.thermals_measured, all.thermals_ended_weak);
    if measured < MIN_MEASURED_THERMALS {
        return None;
    }
    Some(observation(
        TendencyKind::ThermalEnds,
        "How your thermals end",
        format!(
            "In {} of {} thermals the last minute climbed at less than a third of the \
             thermal's average.",
            count(weak),
            count(measured)
        ),
        Some(f64::from(weak) / f64::from(measured)),
        most(flights, |habits| {
            Some(f64::from(habits.thermals_ended_weak))
        }),
    ))
}

/// "On average 48 s between a thermal's highest point and leaving it."
fn after_the_top(flights: &[FlightWithHabits], all: &FlightHabits) -> Option<Tendency> {
    if all.thermals_measured < MIN_MEASURED_THERMALS {
        return None;
    }
    let average_s = all.after_the_top_s as f64 / f64::from(all.thermals_measured);
    Some(observation(
        TendencyKind::AfterTheTop,
        "Time after the top",
        format!(
            "On average {} between a thermal's highest point and leaving it.",
            seconds(average_s)
        ),
        None,
        most(flights, |habits| {
            (habits.thermals_measured >= MIN_MEASURED_IN_A_FLIGHT)
                .then(|| habits.after_the_top_s as f64 / f64::from(habits.thermals_measured))
        }),
    ))
}

/// "On average 35 m below a thermal's highest point when you leave it."
fn below_the_top(flights: &[FlightWithHabits], all: &FlightHabits) -> Option<Tendency> {
    if all.thermals_measured < MIN_MEASURED_THERMALS {
        return None;
    }
    let average_m = all.below_the_top_m as f64 / f64::from(all.thermals_measured);
    Some(observation(
        TendencyKind::BelowTheTop,
        "Leaving a thermal",
        format!(
            "On average {} below a thermal's highest point when you leave it.",
            metres(average_m)
        ),
        None,
        most(flights, |habits| {
            (habits.thermals_measured >= MIN_MEASURED_IN_A_FLIGHT)
                .then(|| habits.below_the_top_m as f64 / f64::from(habits.thermals_measured))
        }),
    ))
}

/// "1.4 m/s on average over 12300 m gained in thermals; the best thermal
/// gave 3.1 m/s."
fn climb(flights: &[FlightWithHabits], all: &FlightHabits) -> Option<Tendency> {
    if all.thermals() < MIN_THERMALS || all.thermalling_s <= 0 {
        return None;
    }
    let average_ms = all.height_gained_m as f64 / all.thermalling_s as f64;
    let mut words = format!(
        "{} m/s on average over {} gained in thermals",
        one_decimal(average_ms),
        metres(all.height_gained_m as f64)
    );
    if let Some(best) = all.best_climb_ms {
        words += &format!("; the best thermal gave {} m/s", one_decimal(best));
    }
    Some(observation(
        TendencyKind::Climb,
        "Climbing",
        words + ".",
        None,
        most(flights, |habits| habits.best_climb_ms),
    ))
}

/// "7.2 to 1 over 86 km in 54 glides of a kilometre or more."
fn glides(all: &FlightHabits) -> Option<Tendency> {
    if all.glides < MIN_GLIDES || all.glide_height_lost_m <= 0 {
        return None;
    }
    Some(observation(
        TendencyKind::Glides,
        "Gliding",
        format!(
            "{} to 1 over {} in {} glides of a kilometre or more.",
            one_decimal(all.glide_distance_m / all.glide_height_lost_m as f64),
            kilometres(all.glide_distance_m),
            count(all.glides)
        ),
        None,
        None,
    ))
}

/// "38% of your airtime was spent in thermals."
fn time_circling(all: &FlightHabits) -> Option<Tendency> {
    if all.airtime_s < MIN_AIRTIME_S {
        return None;
    }
    let share = (all.thermalling_s as f64 / all.airtime_s as f64).clamp(0.0, 1.0);
    Some(observation(
        TendencyKind::TimeCircling,
        "Time circling",
        format!(
            "{:.0}% of your airtime was spent in thermals.",
            share * 100.0
        ),
        Some(share),
        None,
    ))
}

/// "23 of 45 flights ended within a kilometre of where they began; the
/// farthest ended 44 km away." The flight is the one that ended farthest
/// away.
fn landings(flights: &[FlightWithHabits]) -> Option<Tendency> {
    let ended: Vec<(&FlightWithHabits, f64)> = flights
        .iter()
        .filter_map(|flight| Some((flight, flight.ended_from_start_m?)))
        .collect();
    let counted = ended.len() as u32;
    if counted < MIN_FLIGHTS {
        return None;
    }
    let near = ended.iter().filter(|(_, m)| *m < ENDED_NEAR_M).count() as u32;
    // The farthest; of flights as far, the newest, since they come newest
    // first.
    let mut farthest: Option<(&FlightWithHabits, f64)> = None;
    for &(flight, m) in &ended {
        if farthest.is_none_or(|(_, most)| m > most) {
            farthest = Some((flight, m));
        }
    }
    let (flight, farthest_m) = farthest?;
    let mut words = format!(
        "{} of {} flights ended within a kilometre of where they began",
        count(near),
        count(counted)
    );
    if farthest_m >= ENDED_NEAR_M {
        words += &format!("; the farthest ended {} away", kilometres(farthest_m));
    }
    Some(observation(
        TendencyKind::Landings,
        "Where your flights end",
        words + ".",
        Some(f64::from(near) / f64::from(counted)),
        (farthest_m >= ENDED_NEAR_M).then_some(flight),
    ))
}

/// "1,204": a count has a comma between the thousands.
fn count(number: u32) -> String {
    grouped(u64::from(number))
}

fn grouped(number: u64) -> String {
    let digits = number.to_string();
    let mut out = String::new();
    for (position, digit) in digits.chars().enumerate() {
        if position > 0 && (digits.len() - position).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// "1.4": climbs, sinks and glide ratios have one decimal.
fn one_decimal(value: f64) -> String {
    format!("{value:.1}")
}

/// "12300 m": a height has no mark between the thousands, as the design
/// language writes it.
fn metres(metres: f64) -> String {
    format!("{} m", metres.round() as i64)
}

/// "8.4 km" under ten kilometres, "86 km" and "1,204 km" above.
fn kilometres(metres: f64) -> String {
    let km = metres / 1000.0;
    if km < 10.0 {
        format!("{km:.1} km")
    } else {
        format!("{} km", grouped(km.round() as u64))
    }
}

/// "48 s" under two minutes, "3 min" from there.
fn seconds(seconds: f64) -> String {
    let whole = seconds.round() as i64;
    if whole < 120 {
        format!("{whole} s")
    } else {
        format!("{} min", (whole + 30) / 60)
    }
}

#[cfg(test)]
mod tests {
    use venti_flight::igc::{self, IgcFile};
    use venti_flight::testing::{Leg, made_up_flight, mirrored, synthetic_flight};

    use super::*;
    use crate::testing::TempFolder;

    fn core() -> (TempFolder, std::sync::Arc<Core>) {
        let folder = TempFolder::new();
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        (folder, core)
    }

    /// The flight's file, moved to another day of July 2026 so that it is
    /// another flight.
    fn on_day(flight: &IgcFile, day: u32) -> String {
        igc::write(flight).replace("040726", &format!("{day:02}0726"))
    }

    fn glide(seconds: i64) -> Leg {
        Leg::Glide {
            seconds,
            heading: 300.0,
            speed: 11.0,
            climb: -1.1,
        }
    }

    /// Three thermals that climb at 2 m/s for eight circles and at nothing
    /// for three more, with a glide after each. Its circles turn right.
    fn dying_thermals() -> IgcFile {
        let mut legs = vec![glide(60)];
        for _ in 0..3 {
            legs.extend([
                Leg::Circles {
                    circles: 8,
                    climb: 2.0,
                },
                Leg::Circles {
                    circles: 3,
                    climb: 0.0,
                },
                glide(380),
            ]);
        }
        made_up_flight(&legs)
    }

    /// Three thermals that climb at 2 m/s for eight circles and sink at
    /// 1 m/s for two more, 56 m below their top, with a glide after each.
    fn sinking_ends() -> IgcFile {
        let mut legs = vec![glide(60)];
        for _ in 0..3 {
            legs.extend([
                Leg::Circles {
                    circles: 8,
                    climb: 2.0,
                },
                Leg::Circles {
                    circles: 2,
                    climb: -1.0,
                },
                glide(380),
            ]);
        }
        made_up_flight(&legs)
    }

    /// Out to the north for five minutes, a thermal, and back south to
    /// where it began, give or take the thermal's drift of half a
    /// kilometre.
    fn out_and_back() -> IgcFile {
        made_up_flight(&[
            Leg::Glide {
                seconds: 300,
                heading: 0.0,
                speed: 10.0,
                climb: -1.0,
            },
            Leg::Circles {
                circles: 6,
                climb: 2.0,
            },
            Leg::Glide {
                seconds: 300,
                heading: 180.0,
                speed: 10.0,
                climb: -1.0,
            },
        ])
    }

    fn find(tendencies: &Tendencies, kind: TendencyKind) -> Option<&Tendency> {
        tendencies
            .observations
            .iter()
            .find(|observation| observation.kind == kind)
    }

    /// The server's answer for these flights of a logbook, each under a
    /// made-up address of the server's: the address of the logbook's
    /// `n`-th flight is `n` in 26 digits.
    fn server_answer(flights: &[FlightWithHabits]) -> Vec<u8> {
        let flights: Vec<api::FlightHabits> = flights
            .iter()
            .enumerate()
            .map(|(n, flight)| {
                let habits = flight.habits;
                api::FlightHabits {
                    id: format!("{n:026}").parse().unwrap(),
                    start: chrono::DateTime::from_timestamp(flight.start_unix_s, 0).unwrap(),
                    ended_from_start_m: flight.ended_from_start_m,
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
            })
            .rev()
            .collect();
        serde_json::to_vec(&HabitsResponse {
            flights,
            not_yet: 0,
        })
        .unwrap()
    }

    #[test]
    fn the_flights_on_the_server_say_what_the_same_flights_on_the_device_say() {
        let (_folder, core, _, _) = five_flights();
        let on_device = core.tendencies();
        let kept = core.logbook.habits();
        assert!(!on_device.observations.is_empty());

        // Given oldest first, read newest first as the logbook is.
        let mut on_server = own_tendencies(200, server_answer(&kept)).unwrap();
        for observation in &mut on_server.observations {
            if let Some(id) = &observation.flight_id {
                let n: usize = id.parse().unwrap();
                observation.flight_id = Some(kept[n].id.clone());
            }
        }
        assert_eq!(on_server, on_device);

        let request = own_habits_request();
        assert_eq!(
            (request.method.as_str(), request.path.as_str()),
            ("GET", "/v1/account/habits")
        );
        assert!(request.needs_token);
    }

    #[test]
    fn the_server_s_answer_for_the_demo_account_is_read() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../server/tests/golden/account_habits.json"
        );
        let tendencies = own_tendencies(200, std::fs::read(path).unwrap()).unwrap();
        // Three flights are too few to say anything of.
        assert_eq!(tendencies.flights, 3);
        assert!(tendencies.thermals > 0);
        assert!(
            own_tendencies(
                401,
                br#"{"error":"unauthorized","message":"Sign in."}"#.to_vec()
            )
            .is_err()
        );
    }

    #[test]
    fn an_empty_logbook_says_nothing() {
        let (_folder, core) = core();
        let tendencies = core.tendencies();

        assert_eq!((tendencies.flights, tendencies.thermals), (0, 0));
        assert!(tendencies.observations.is_empty());
        assert!(tendencies.note.contains("not a judgement"));
    }

    #[test]
    fn too_few_thermals_say_nothing() {
        let (_folder, core) = core();
        // Four flights of two thermals: eight, under the ten needed.
        for day in 1..=4 {
            core.import_flight(on_day(&synthetic_flight(), day))
                .unwrap();
        }
        let tendencies = core.tendencies();

        assert_eq!((tendencies.flights, tendencies.thermals), (4, 8));
        assert!(
            tendencies.observations.is_empty(),
            "{:#?}",
            tendencies.observations
        );
    }

    /// Three flights circled right (two thermals each, ending strong), one
    /// circled left, and one circled left with three thermals that die.
    fn five_flights() -> (TempFolder, std::sync::Arc<Core>, String, String) {
        let (folder, core) = core();
        let newest = core
            .import_flight(on_day(&synthetic_flight(), 5))
            .unwrap()
            .id;
        for day in [4, 3] {
            core.import_flight(on_day(&synthetic_flight(), day))
                .unwrap();
        }
        core.import_flight(on_day(&mirrored(&synthetic_flight()), 2))
            .unwrap();
        let dying = core
            .import_flight(on_day(&mirrored(&dying_thermals()), 1))
            .unwrap()
            .id;
        (folder, core, newest, dying)
    }

    #[test]
    fn which_way_the_pilot_circles_is_counted_with_the_flight_that_went_the_other_way() {
        let (_folder, core, _, dying) = five_flights();
        let tendencies = core.tendencies();
        assert_eq!((tendencies.flights, tendencies.thermals), (5, 11));

        let circling = find(&tendencies, TendencyKind::Circling).unwrap();
        assert_eq!(circling.title, "Which way you circle");
        assert_eq!(
            circling.words,
            "Of 11 thermals circled one way, 6 went right and 5 left."
        );
        assert!((circling.share.unwrap() - 6.0 / 11.0).abs() < 1e-9);
        // The rarer way is left, and the dying flight has three of those.
        assert_eq!(circling.flight_id.as_deref(), Some(dying.as_str()));
        assert!(circling.flight_unix_s.is_some());
    }

    #[test]
    fn how_thermals_end_and_the_time_after_the_top_are_counted() {
        let (_folder, core, _, dying) = five_flights();
        let tendencies = core.tendencies();

        let ends = find(&tendencies, TendencyKind::ThermalEnds).unwrap();
        assert_eq!(ends.title, "How your thermals end");
        assert_eq!(
            ends.words,
            "In 3 of 11 thermals the last minute climbed at less than a third of the \
             thermal's average."
        );
        assert!((ends.share.unwrap() - 3.0 / 11.0).abs() < 1e-9);
        assert_eq!(ends.flight_id.as_deref(), Some(dying.as_str()));

        let top = find(&tendencies, TendencyKind::AfterTheTop).unwrap();
        assert_eq!(top.title, "Time after the top");
        // Three thermals of some 80 s after the top and eight of a few
        // seconds: some half a minute on average.
        let average: i64 = top
            .words
            .strip_prefix("On average ")
            .and_then(|rest| rest.split_once(" s between a thermal's highest point"))
            .map(|(number, _)| number.parse().unwrap())
            .unwrap_or_else(|| panic!("{}", top.words));
        assert!((15..=40).contains(&average), "{}", top.words);
        assert_eq!(top.share, None);
        // The only flight with three measured thermals.
        assert_eq!(top.flight_id.as_deref(), Some(dying.as_str()));
    }

    #[test]
    fn how_far_below_the_top_a_thermal_is_left_is_counted() {
        let (_folder, core) = core();
        for day in 1..=4 {
            core.import_flight(on_day(&synthetic_flight(), day))
                .unwrap();
        }
        let sinking = core.import_flight(on_day(&sinking_ends(), 5)).unwrap().id;
        let tendencies = core.tendencies();

        let below = find(&tendencies, TendencyKind::BelowTheTop).unwrap();
        assert_eq!(below.title, "Leaving a thermal");
        // Three thermals left some 56 m below their top and eight a few
        // metres below: some twenty on average.
        let average: i64 = below
            .words
            .strip_prefix("On average ")
            .and_then(|rest| {
                rest.strip_suffix(" m below a thermal's highest point when you leave it.")
            })
            .map(|number| number.parse().unwrap())
            .unwrap_or_else(|| panic!("{}", below.words));
        assert!((10..=35).contains(&average), "{}", below.words);
        assert_eq!(below.share, None);
        assert_eq!(below.flight_id.as_deref(), Some(sinking.as_str()));
    }

    #[test]
    fn where_flights_end_is_counted_from_ten_flights() {
        let (_folder, core) = core();
        for day in 1..=4 {
            core.import_flight(on_day(&out_and_back(), day)).unwrap();
        }
        for day in 10..=14 {
            core.import_flight(on_day(&synthetic_flight(), day))
                .unwrap();
        }
        // Nine flights: under the ten needed.
        assert_eq!(find(&core.tendencies(), TendencyKind::Landings), None);

        let newest = core
            .import_flight(on_day(&synthetic_flight(), 15))
            .unwrap()
            .id;
        let tendencies = core.tendencies();
        let landings = find(&tendencies, TendencyKind::Landings).unwrap();
        assert_eq!(landings.title, "Where your flights end");
        // The made-up flight ends some kilometres from its take-off.
        let km: f64 = landings
            .words
            .strip_prefix(
                "4 of 10 flights ended within a kilometre of where they began; the farthest \
                 ended ",
            )
            .and_then(|rest| rest.strip_suffix(" km away."))
            .map(|number| number.parse().unwrap())
            .unwrap_or_else(|| panic!("{}", landings.words));
        assert!((2.0..10.0).contains(&km), "{}", landings.words);
        assert!((landings.share.unwrap() - 0.4).abs() < 1e-9);
        // Six flights end as far: the newest of them.
        assert_eq!(landings.flight_id.as_deref(), Some(newest.as_str()));
    }

    #[test]
    fn the_climb_names_the_flight_with_the_best_thermal() {
        let (_folder, core, newest, _) = five_flights();
        let tendencies = core.tendencies();

        let climb = find(&tendencies, TendencyKind::Climb).unwrap();
        assert_eq!(climb.title, "Climbing");
        assert!(
            climb.words.ends_with("; the best thermal gave 2.4 m/s."),
            "{}",
            climb.words
        );
        assert!(
            climb.words.contains(" m/s on average over "),
            "{}",
            climb.words
        );
        // Three flights share the best thermal: the newest of them.
        assert_eq!(climb.flight_id.as_deref(), Some(newest.as_str()));

        // Seven glides of a kilometre: under the ten needed.
        assert_eq!(find(&tendencies, TendencyKind::Glides), None);
        // Some two and a half hours in the air.
        let time = find(&tendencies, TendencyKind::TimeCircling).unwrap();
        assert_eq!(time.title, "Time circling");
        let share = time.share.unwrap();
        assert!((0.2..0.5).contains(&share), "{share}");
        assert_eq!(
            time.words,
            format!(
                "{:.0}% of your airtime was spent in thermals.",
                share * 100.0
            )
        );
    }

    #[test]
    fn the_glides_are_said_from_ten_of_a_kilometre() {
        let (_folder, core) = core();
        // Each made-up flight has one glide of 4.2 km at ten to one.
        for day in 1..=10 {
            core.import_flight(on_day(&synthetic_flight(), day))
                .unwrap();
        }
        let tendencies = core.tendencies();

        let glides = find(&tendencies, TendencyKind::Glides).unwrap();
        assert_eq!(glides.title, "Gliding");
        assert_eq!(glides.flight_id, None);
        let (ratio, rest) = glides.words.split_once(" to 1 over ").unwrap();
        assert!(
            (ratio.parse::<f64>().unwrap() - 10.0).abs() < 0.6,
            "{}",
            glides.words
        );
        assert!(
            rest.ends_with(" km in 10 glides of a kilometre or more."),
            "{}",
            glides.words
        );
        let km: u32 = rest.split_once(" km").unwrap().0.parse().unwrap();
        assert!((40..=44).contains(&km), "{}", glides.words);

        // Twenty thermals, all to the right: no rarer way, no flight.
        let circling = find(&tendencies, TendencyKind::Circling).unwrap();
        assert_eq!(
            circling.words,
            "All 20 thermals circled one way went right."
        );
        assert_eq!(circling.share, Some(1.0));
        assert_eq!(circling.flight_id, None);
        // None ended weak: no flight shows it.
        let ends = find(&tendencies, TendencyKind::ThermalEnds).unwrap();
        assert!(
            ends.words.starts_with("In 0 of 20 thermals"),
            "{}",
            ends.words
        );
        assert_eq!(ends.flight_id, None);
    }

    #[test]
    fn the_observations_come_in_a_fixed_order() {
        let (_folder, core) = core();
        for day in 1..=10 {
            core.import_flight(on_day(&synthetic_flight(), day))
                .unwrap();
        }
        let kinds: Vec<TendencyKind> = core
            .tendencies()
            .observations
            .iter()
            .map(|observation| observation.kind)
            .collect();
        assert_eq!(
            kinds,
            [
                TendencyKind::Circling,
                TendencyKind::ThermalEnds,
                TendencyKind::AfterTheTop,
                TendencyKind::BelowTheTop,
                TendencyKind::Climb,
                TendencyKind::Glides,
                TendencyKind::TimeCircling,
                TendencyKind::Landings,
            ]
        );
    }

    #[test]
    fn numbers_are_written_as_the_core_writes_them() {
        assert_eq!(count(1204), "1,204");
        assert_eq!(count(40), "40");
        assert_eq!(metres(12_345.4), "12345 m");
        assert_eq!(kilometres(8_440.0), "8.4 km");
        assert_eq!(kilometres(86_400.0), "86 km");
        assert_eq!(kilometres(1_204_000.0), "1,204 km");
        assert_eq!(seconds(47.6), "48 s");
        assert_eq!(seconds(200.0), "3 min");
        assert_eq!(one_decimal(1.36), "1.4");
    }
}
