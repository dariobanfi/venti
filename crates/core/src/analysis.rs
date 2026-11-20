//! The analysis of one flight, as the shells show it.

use venti_flight::{analysis, igc, scoring};

use crate::{Coordinate, Core, CoreError, LogbookError, WindView};

/// A moment of a flight.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FlightMoment {
    /// Its position in the flight's track, to move a cursor there.
    pub track_index: u32,
    /// Seconds since the first point of the track.
    pub seconds: u32,
    /// Where.
    pub position: Coordinate,
    /// Altitude in metres.
    pub altitude_m: f64,
}

/// One thermal of a flight.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ThermalSummary {
    /// Where circling began.
    pub entry: FlightMoment,
    /// Where it ended.
    pub exit: FlightMoment,
    /// Height gained, metres.
    pub gain_m: f64,
    /// Average climb, metres per second.
    pub climb_ms: f64,
    /// The wind, from the drift of the circles.
    pub wind: Option<WindView>,
}

/// One glide of a flight.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct GlideSummary {
    /// Where it began.
    pub start: FlightMoment,
    /// Where it ended.
    pub end: FlightMoment,
    /// Distance in a straight line, metres.
    pub distance_m: f64,
    /// Height lost, metres. Negative if the glide gained height.
    pub height_lost_m: f64,
    /// Metres forward per metre lost. Absent if it did not lose height.
    pub glide_ratio: Option<f64>,
    /// Average speed over the ground, metres per second.
    pub speed_ms: f64,
}

/// What a flight is scored as under cross-country league rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum RouteKind {
    /// The longest distance through up to three turnpoints.
    FreeFlight,
    /// A triangle that is not an FAI triangle.
    FreeTriangle,
    /// A triangle whose shortest leg is at least 28% of its perimeter.
    FaiTriangle,
}

/// How a flight scores under the World XContest's rules. The points are
/// this app's own calculation, not the league's.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FlightScore {
    /// What the flight is scored as.
    pub kind: RouteKind,
    /// For a triangle, whether start and finish lie within 5% of its
    /// perimeter of each other.
    pub closed: bool,
    /// The scoring distance, metres. For a triangle, its perimeter minus
    /// the gap between start and finish.
    pub distance_m: f64,
    /// The points: kilometres times the points per kilometre.
    pub points: f64,
    /// Points per kilometre for this kind of route.
    pub points_per_km: f64,
    /// The points the route runs through, in flight order: start,
    /// turnpoints, finish.
    pub route: Vec<FlightMoment>,
    /// For a triangle, the gap between start and finish, metres.
    pub closing_m: Option<f64>,
}

/// What happened in a flight.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FlightAnalysis {
    /// Where the glider took off. Absent if the recording never shows it
    /// flying; then everything else is empty too.
    pub takeoff: Option<FlightMoment>,
    /// Where it landed. Absent if the recording ends in the air.
    pub landing: Option<FlightMoment>,
    /// Seconds from take-off to landing, or to the end of the recording.
    pub airtime_s: i64,
    /// Seconds spent in thermals.
    pub thermalling_s: i64,
    /// Seconds spent on glides.
    pub gliding_s: i64,
    /// Height gained in thermals, metres.
    pub height_gained_m: f64,
    /// Every thermal, in order.
    pub thermals: Vec<ThermalSummary>,
    /// Every glide, in order.
    pub glides: Vec<GlideSummary>,
    /// How the flight scores as a cross-country flight.
    pub score: Option<FlightScore>,
}

#[uniffi::export]
impl Core {
    /// Analyses one flight from the logbook: take-off and landing, every
    /// thermal and glide, and the free distance.
    pub fn flight_analysis(&self, id: String) -> Result<FlightAnalysis, CoreError> {
        let text = self.logbook.flight_text(&id)?;
        analysis_of(&text)
    }
}

/// Analyses a flight that is not in the logbook, from its file's text:
/// another pilot's public flight. Nothing is kept.
#[uniffi::export]
pub fn flight_analysis_from_file(igc_text: String) -> Result<FlightAnalysis, CoreError> {
    analysis_of(&igc_text)
}

fn analysis_of(text: &str) -> Result<FlightAnalysis, CoreError> {
    let file = igc::parse(text).map_err(LogbookError::Invalid)?;
    // Moments count from the take-off, like the track `flight` returns.
    let analysis = analysis::analyse(&file.fixes);

    let moment = |moment: analysis::Moment| FlightMoment {
        track_index: u32::try_from(moment.index).unwrap_or(u32::MAX),
        seconds: u32::try_from(moment.seconds).unwrap_or(0),
        position: Coordinate {
            lat: moment.position.lat(),
            lon: moment.position.lon(),
        },
        altitude_m: f64::from(moment.altitude_m),
    };

    Ok(FlightAnalysis {
        takeoff: analysis.takeoff.map(moment),
        landing: analysis.landing.map(moment),
        airtime_s: analysis.airtime_s,
        thermalling_s: analysis.thermalling_s,
        gliding_s: analysis.gliding_s,
        height_gained_m: f64::from(analysis.height_gained_m),
        thermals: analysis
            .thermals
            .iter()
            .map(|thermal| ThermalSummary {
                entry: moment(thermal.entry),
                exit: moment(thermal.exit),
                gain_m: f64::from(thermal.gain_m),
                climb_ms: thermal.climb_ms,
                wind: thermal.wind.map(|wind| WindView {
                    from_deg: wind.from_deg,
                    speed_ms: wind.speed_ms,
                }),
            })
            .collect(),
        glides: analysis
            .glides
            .iter()
            .map(|glide| GlideSummary {
                start: moment(glide.start),
                end: moment(glide.end),
                distance_m: glide.distance_m,
                height_lost_m: f64::from(glide.height_lost_m),
                glide_ratio: glide.glide_ratio,
                speed_ms: glide.speed_ms,
            })
            .collect(),
        score: analysis.score.map(|score| {
            let kind = match score.kind {
                scoring::RouteKind::FreeFlight => RouteKind::FreeFlight,
                scoring::RouteKind::FreeTriangle => RouteKind::FreeTriangle,
                scoring::RouteKind::FaiTriangle => RouteKind::FaiTriangle,
            };
            FlightScore {
                kind,
                closed: score.closed,
                distance_m: score.distance_m,
                points: score.points,
                points_per_km: score.kind.points_per_km(score.closed),
                route: score.points_on_track.into_iter().map(moment).collect(),
                closing_m: score.closing_m,
            }
        }),
    })
}

#[cfg(test)]
mod tests {
    use venti_flight::testing::synthetic_flight;

    use super::*;
    use crate::testing::TempFolder;

    /// The sample flight with two minutes of standing before and after, as
    /// a recorder left running writes it.
    fn sample_with_ground_time() -> String {
        let mut flight = synthetic_flight();
        let (first, last) = (flight.fixes[0], *flight.fixes.last().unwrap());
        let mut fixes: Vec<igc::Fix> = (0..120)
            .map(|second| igc::Fix {
                time: first.time - chrono::Duration::seconds(120 - second),
                ..first
            })
            .collect();
        fixes.extend(&flight.fixes);
        fixes.extend((1..=120).map(|second| igc::Fix {
            time: last.time + chrono::Duration::seconds(second),
            ..last
        }));
        flight.fixes = fixes;
        igc::write(&flight)
    }

    #[test]
    fn standing_around_before_and_after_is_left_out_everywhere() {
        let folder = TempFolder::new();
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        let id = core.import_flight(sample_with_ground_time()).unwrap().id;

        // The recording is 1984 s long; the flight in it is 1744 s.
        let entry = &core.logbook().years[0].flights[0];
        assert!(
            (entry.duration_s - 1744).abs() <= 15,
            "{}",
            entry.duration_s
        );
        let flight = core.flight(id.clone()).unwrap();
        assert_eq!(flight.duration_s, entry.duration_s);
        assert_eq!(flight.start_unix_s, entry.start_unix_s);
        assert_eq!(flight.track[0].seconds, 0);

        let analysis = core.flight_analysis(id).unwrap();
        assert_eq!(analysis.takeoff.unwrap().track_index, 0);
        let landing = analysis.landing.unwrap();
        assert_eq!(
            flight.track[landing.track_index as usize].seconds,
            landing.seconds
        );
        assert_eq!(analysis.thermals.len(), 2);
    }

    #[test]
    fn a_logbook_flight_is_analysed() {
        let folder = TempFolder::new();
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        let id = core
            .import_flight(igc::write(&synthetic_flight()))
            .unwrap()
            .id;

        let analysis = core.flight_analysis(id.clone()).unwrap();

        assert_eq!(analysis.takeoff.unwrap().seconds, 0);
        assert_eq!(analysis.landing, None);
        assert_eq!(analysis.thermals.len(), 2);
        assert_eq!(analysis.glides.len(), 3);
        assert!(analysis.thermals[0].wind.is_some());
        let score = analysis.score.unwrap();
        assert!(score.distance_m > 10_000.0);
        assert!((score.points - score.distance_m / 1000.0 * score.points_per_km).abs() < 1e-6);
        assert!(score.route.len() >= 2);
        // A moment's index points into the flight's track.
        let track = core.flight(id).unwrap().track;
        let exit = analysis.thermals[0].exit;
        assert_eq!(track[exit.track_index as usize].seconds, exit.seconds);

        let error = core.flight_analysis("nothing".to_owned()).unwrap_err();
        assert!(matches!(error, CoreError::UnknownFlight { .. }));
    }
}
