//! A route drawn on the map before the flight.
//!
//! The pilot taps the places to fly by, in order. The core says how long
//! each leg is and what the route comes to, and what it would score if it
//! were flown exactly as drawn: the same rules, and the same search for
//! the best start, turnpoints and finish, as for a flight that was flown
//! (see [`venti_flight::scoring`]). A route that comes back towards its
//! start is found to be a triangle by that search; nobody has to say so.

use chrono::DateTime;
use venti_domain::GeoPoint;
use venti_flight::geodesy::Earth;
use venti_flight::igc::Fix;
use venti_flight::live::navigation::bearing_deg;
use venti_flight::{fai, scoring};

use crate::airspace::compass;
use crate::{Coordinate, RouteKind};

/// The drawn legs are walked in steps of this length to score them, so
/// that the best start and finish may lie on a leg and not only at a
/// place that was tapped, metres.
const STEP_M: f64 = 250.0;

/// No more places than this are walked; a route longer than a thousand
/// kilometres is walked in longer steps.
const STEPS_AT_MOST: usize = 4000;

/// One leg of a drawn route.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RouteLegView {
    /// Its length, metres.
    pub distance_m: f64,
    /// The direction to fly it, degrees clockwise from north.
    pub bearing_deg: f64,
    /// In words: "12.4 km NE".
    pub words: String,
}

/// What a drawn route would score, flown exactly as drawn.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PlannedScore {
    /// What it would be scored as.
    pub kind: RouteKind,
    /// For a triangle, whether start and finish lie within 5% of its
    /// perimeter of each other.
    pub closed: bool,
    /// The scoring distance, metres.
    pub distance_m: f64,
    /// The points.
    pub points: f64,
    /// Points per kilometre for this kind of route.
    pub points_per_km: f64,
    /// The places the scored route runs through: start, turnpoints,
    /// finish. They lie on the drawn line, not always at a tapped place.
    pub route: Vec<Coordinate>,
    /// For a triangle, the gap between start and finish, metres.
    pub closing_m: Option<f64>,
    /// In words: "Closed FAI triangle".
    pub name: String,
}

/// Where a triangle closes: the circle around its start that the finish
/// has to come into, by XContest's rule that the engine scores with
/// ([`venti_flight::scoring`]): within 5% of the perimeter of the start.
/// The triangle's three turnpoints are taken as they stand; flying on can
/// find others, and then another circle.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ClosingCircleView {
    /// The triangle's start, the circle's centre.
    pub centre: Coordinate,
    /// Its radius, metres: 5% of the perimeter.
    pub radius_m: f64,
    /// The circle for the map, closed: a point every five degrees and the
    /// first again at the end.
    pub outline: Vec<Coordinate>,
    /// 20% of the perimeter, metres: a finish farther than this from the
    /// start makes no triangle at all.
    pub counts_radius_m: f64,
    /// The perimeter through the three turnpoints, metres.
    pub perimeter_m: f64,
    /// What it is closed: "Closed FAI triangle", "Closed triangle".
    pub kind_words: String,
    /// What it scores closed with the finish on the circle's edge, the
    /// least it scores closed: the perimeter less the radius, metres.
    pub closed_distance_m: f64,
    /// The points for that.
    pub closed_points: f64,
    /// How far the glider is from the circle's edge, metres; zero inside
    /// it. Absent for a route planned on the map.
    pub distance_to_edge_m: Option<f64>,
    /// In a line. In flight: "Closing circle 2.1 km away: closed FAI
    /// triangle of 57.0 km, 91.2 points"; inside it: "In the closing
    /// circle: …". Planned: "Closes within 1.6 km of the start: …".
    pub words: String,
}

/// The closing circle of a triangle as a map draws it, with the distance
/// to its edge from `from` when there is a glider.
pub(crate) fn closing_circle_view(
    circle: &scoring::ClosingCircle,
    from: Option<GeoPoint>,
) -> ClosingCircleView {
    let point = |point: GeoPoint| Coordinate {
        lat: point.lat(),
        lon: point.lon(),
    };
    let kind_words = match circle.kind {
        scoring::RouteKind::FaiTriangle => "Closed FAI triangle",
        _ => "Closed triangle",
    };
    let distance_to_edge_m =
        from.map(|from| (from.distance_m(circle.centre) - circle.radius_m).max(0.0));
    let scored = format!(
        "{} of {:.1} km, {:.1} points",
        kind_words.to_lowercase().replace("fai", "FAI"),
        circle.closed_distance_m / 1000.0,
        circle.closed_points
    );
    let words = match distance_to_edge_m {
        Some(edge) if edge <= 0.0 => format!("In the closing circle: {scored}"),
        Some(edge) => format!("Closing circle {:.1} km away: {scored}", edge / 1000.0),
        None => format!(
            "Closes within {:.1} km of the start: {scored}",
            circle.radius_m / 1000.0
        ),
    };
    ClosingCircleView {
        centre: point(circle.centre),
        radius_m: circle.radius_m,
        outline: (0..=72)
            .map(|step| {
                point(Earth::FaiSphere.destination(
                    circle.centre,
                    f64::from(step) * 5.0,
                    circle.radius_m,
                ))
            })
            .collect(),
        counts_radius_m: circle.counts_radius_m,
        perimeter_m: circle.perimeter_m,
        kind_words: kind_words.to_owned(),
        closed_distance_m: circle.closed_distance_m,
        closed_points: circle.closed_points,
        distance_to_edge_m,
        words,
    }
}

/// A drawn route in numbers.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RoutePlan {
    /// The legs, in order: one fewer than there are places.
    pub legs: Vec<RouteLegView>,
    /// The length along the legs, metres.
    pub distance_m: f64,
    /// What it would score. Absent for fewer than two places.
    pub score: Option<PlannedScore>,
    /// With one leg drawn: the outlines, closed, of the two areas in
    /// which a third place makes an FAI triangle with that leg as one
    /// side. Empty otherwise.
    pub fai_areas: Vec<Vec<Coordinate>>,
    /// Where the route closes as a triangle, when the drawn line makes
    /// one: the circle around its start that its finish has to come into.
    /// Absent for a free flight.
    #[uniffi(default = None)]
    #[cfg_attr(
        feature = "wasm",
        serde(skip_serializing_if = "Option::is_none"),
        tsify(optional)
    )]
    pub closing_circle: Option<ClosingCircleView>,
    /// In a sentence or two: "42.3 km along the route. Flown like this it
    /// scores as a closed FAI triangle: 40.1 km, 64.2 points."
    pub words: String,
}

/// Works out what a route drawn on the map comes to. Places that are not
/// on the map are left out.
#[uniffi::export]
pub fn plan_route(points: Vec<Coordinate>) -> RoutePlan {
    let places: Vec<GeoPoint> = points
        .iter()
        .filter_map(|point| GeoPoint::new(point.lat, point.lon).ok())
        .collect();
    let legs: Vec<RouteLegView> = places
        .windows(2)
        .map(|pair| {
            let (distance_m, bearing) =
                (pair[0].distance_m(pair[1]), bearing_deg(pair[0], pair[1]));
            RouteLegView {
                distance_m,
                bearing_deg: bearing,
                words: format!("{:.1} km {}", distance_m / 1000.0, compass(bearing)),
            }
        })
        .collect();
    let distance_m: f64 = legs.iter().map(|leg| leg.distance_m).sum();
    let (score, closing_circle) = planned_score(&places, distance_m).unzip();
    let closing_circle = closing_circle.flatten();
    // With one leg, where a third place would make an FAI triangle of it.
    let areas = match places.as_slice() {
        [from, to] => fai::areas(*from, *to),
        _ => None,
    };

    let words = match (&score, places.len()) {
        (_, 0) => "Tap the map where the route begins.".to_owned(),
        (_, 1) => "Tap the next place to fly to.".to_owned(),
        (Some(score), _) => {
            let along = format!("{:.1} km along the route.", distance_m / 1000.0);
            let mut scored = format!(
                " Flown like this it scores as {}: {:.1} km, {:.1} points.",
                with_article(&score.name),
                score.distance_m / 1000.0,
                score.points
            );
            if let Some(gap) = score.closing_m.filter(|_| !score.closed) {
                scored.push_str(&format!(
                    " Start and finish lie {:.1} km apart.",
                    gap / 1000.0
                ));
            }
            along + &scored
        }
        (None, _) => format!("{:.1} km along the route.", distance_m / 1000.0),
    };
    let words = match &areas {
        Some(areas) => format!(
            "{words} A third place inside a dashed area makes an FAI triangle of {:.0} to {:.0} km.",
            areas.smallest_m / 1000.0,
            areas.largest_m / 1000.0
        ),
        None => words,
    };

    RoutePlan {
        legs,
        distance_m,
        score,
        fai_areas: areas
            .map(|areas| {
                areas
                    .outlines
                    .iter()
                    .map(|outline| {
                        outline
                            .iter()
                            .map(|point| Coordinate {
                                lat: point.lat(),
                                lon: point.lon(),
                            })
                            .collect()
                    })
                    .collect()
            })
            .unwrap_or_default(),
        closing_circle,
        words,
    }
}

/// Scores the drawn line as a flight along it would be scored, with the
/// closing circle of its best triangle if it makes one.
fn planned_score(
    places: &[GeoPoint],
    distance_m: f64,
) -> Option<(PlannedScore, Option<ClosingCircleView>)> {
    if places.len() < 2 || distance_m <= 0.0 {
        return None;
    }
    // Walk the legs in steps, so that start and finish can be found on
    // them. Straight in latitude and longitude: over a leg of a flight
    // that differs from the shortest line by less than the step.
    #[allow(clippy::cast_precision_loss)]
    let step_m = STEP_M.max(distance_m / STEPS_AT_MOST as f64);
    let mut walked = vec![places[0]];
    for pair in places.windows(2) {
        let steps = (pair[0].distance_m(pair[1]) / step_m).ceil().max(1.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = steps as usize;
        for step in 1..=count {
            #[allow(clippy::cast_precision_loss)]
            let share = step as f64 / steps;
            let lat = pair[0].lat() + (pair[1].lat() - pair[0].lat()) * share;
            let lon = pair[0].lon() + (pair[1].lon() - pair[0].lon()) * share;
            walked.push(GeoPoint::new(lat, lon).ok()?);
        }
    }
    let fixes: Vec<Fix> = walked
        .iter()
        .enumerate()
        .map(|(second, position)| {
            Some(Fix {
                time: DateTime::from_timestamp(i64::try_from(second).ok()?, 0)?,
                position: *position,
                gps_altitude_m: 0,
                pressure_altitude_m: 0,
                valid: true,
            })
        })
        .collect::<Option<_>>()?;

    let scores = scoring::scores(&fixes)?;
    let closing = scores
        .triangle
        .as_ref()
        .and_then(|triangle| scoring::closing_circle(triangle, &fixes))
        .map(|circle| closing_circle_view(&circle, None));
    let score = scores.best;
    let kind = match score.kind {
        scoring::RouteKind::FreeFlight => RouteKind::FreeFlight,
        scoring::RouteKind::FreeTriangle => RouteKind::FreeTriangle,
        scoring::RouteKind::FaiTriangle => RouteKind::FaiTriangle,
    };
    let name = match (kind, score.closed) {
        (RouteKind::FreeFlight, _) => "Free flight",
        (RouteKind::FreeTriangle, false) => "Triangle",
        (RouteKind::FreeTriangle, true) => "Closed triangle",
        (RouteKind::FaiTriangle, false) => "FAI triangle",
        (RouteKind::FaiTriangle, true) => "Closed FAI triangle",
    };
    let planned = PlannedScore {
        kind,
        closed: score.closed,
        distance_m: score.distance_m,
        points: score.points,
        points_per_km: score.kind.points_per_km(score.closed),
        route: score
            .route
            .iter()
            .filter_map(|index| walked.get(*index))
            .map(|place| Coordinate {
                lat: place.lat(),
                lon: place.lon(),
            })
            .collect(),
        closing_m: score.closing_m,
        name: name.to_owned(),
    };
    Some((planned, closing))
}

/// "a free flight", "an FAI triangle", "a closed triangle".
fn with_article(name: &str) -> String {
    let lower = if name.starts_with("FAI") {
        name.to_owned()
    } else {
        let mut letters = name.chars();
        letters
            .next()
            .map(|first| first.to_lowercase().collect::<String>() + letters.as_str())
            .unwrap_or_default()
    };
    let article = if lower.starts_with("FAI") { "an" } else { "a" };
    format!("{article} {lower}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(lat: f64, lon: f64) -> Coordinate {
        Coordinate { lat, lon }
    }

    /// The Wallberg, and two places that make an even triangle with it of
    /// some ten kilometres a side.
    const WALLBERG: (f64, f64) = (47.6642, 11.7967);
    const EAST: (f64, f64) = (47.6642, 11.9300);
    const NORTH: (f64, f64) = (47.7420, 11.8630);

    #[test]
    fn a_route_being_drawn_says_what_to_do_next() {
        let nothing = plan_route(Vec::new());
        assert!(nothing.legs.is_empty());
        assert_eq!(nothing.score, None);
        assert_eq!(nothing.words, "Tap the map where the route begins.");

        let one = plan_route(vec![at(WALLBERG.0, WALLBERG.1)]);
        assert_eq!(one.distance_m, 0.0);
        assert_eq!(one.words, "Tap the next place to fly to.");
        // A place that is none is left out, and the same place twice is
        // no route.
        let twice = plan_route(vec![
            at(WALLBERG.0, WALLBERG.1),
            at(95.0, 0.0),
            at(WALLBERG.0, WALLBERG.1),
        ]);
        assert_eq!(twice.legs.len(), 1);
        assert_eq!(twice.score, None);
    }

    #[test]
    fn a_line_out_is_a_free_flight_of_its_length() {
        let plan = plan_route(vec![at(WALLBERG.0, WALLBERG.1), at(EAST.0, EAST.1)]);

        assert_eq!(plan.legs.len(), 1);
        assert_eq!(plan.closing_circle, None);
        assert_eq!(plan.legs[0].words, "10.0 km E");
        let score = plan.score.unwrap();
        assert_eq!(score.kind, RouteKind::FreeFlight);
        assert!((score.distance_m - plan.distance_m).abs() < 1.0);
        assert!((score.points - 10.0).abs() < 0.05, "{}", score.points);
        assert_eq!(score.points_per_km, 1.0);
        assert_eq!(score.route.len(), 2);
        // With one leg drawn the map can show where a third place makes
        // an FAI triangle of it: one area on each side, and the words say
        // how large such a triangle is. A leg of ten kilometres is at most
        // 44% and at least 28% of the way round.
        assert_eq!(plan.fai_areas.len(), 2);
        assert!(plan.fai_areas.iter().all(|outline| outline.len() > 8));
        assert_eq!(
            plan.words,
            "10.0 km along the route. Flown like this it scores as a free flight: 10.0 km, 10.0 points. A third place inside a dashed area makes an FAI triangle of 23 to 36 km."
        );
    }

    #[test]
    fn a_route_that_comes_back_is_found_to_be_a_triangle() {
        let corners = vec![
            at(WALLBERG.0, WALLBERG.1),
            at(EAST.0, EAST.1),
            at(NORTH.0, NORTH.1),
            at(WALLBERG.0, WALLBERG.1),
        ];

        let plan = plan_route(corners);

        assert_eq!(plan.legs.len(), 3);
        // The areas are for a route of one leg only.
        assert!(plan.fai_areas.is_empty());
        let score = plan.score.unwrap();
        // Three sides of about ten kilometres: every side is over 28% of
        // the way round, and it ends where it began.
        assert_eq!(score.kind, RouteKind::FaiTriangle);
        assert!(score.closed);
        assert_eq!(score.points_per_km, 1.6);
        assert!(
            (score.distance_m - plan.distance_m).abs() < 600.0,
            "{score:?}"
        );
        assert!((score.points - score.distance_m / 1000.0 * 1.6).abs() < 0.01);
        assert_eq!(score.route.len(), 5);
        assert_eq!(score.name, "Closed FAI triangle");
        // Its closing circle stands on the start, a twentieth of the way
        // round wide, drawn as a ring.
        let circle = plan.closing_circle.clone().unwrap();
        let start = GeoPoint::new(WALLBERG.0, WALLBERG.1).unwrap();
        let centre = GeoPoint::new(circle.centre.lat, circle.centre.lon).unwrap();
        assert!(centre.distance_m(start) < 600.0, "{circle:?}");
        assert!((circle.radius_m - 0.05 * circle.perimeter_m).abs() < 1e-6);
        assert_eq!(circle.outline.len(), 73);
        assert_eq!(circle.outline.first(), circle.outline.last());
        assert_eq!(circle.distance_to_edge_m, None);
        assert_eq!(circle.kind_words, "Closed FAI triangle");
        assert!(
            circle.words.starts_with("Closes within 1.")
                && circle.words.contains("closed FAI triangle of"),
            "{}",
            circle.words
        );
        assert!(
            plan.words.contains("scores as a closed FAI triangle"),
            "{}",
            plan.words
        );
    }

    #[test]
    fn a_triangle_left_open_says_how_far_apart_its_ends_are() {
        // The way back stops three kilometres short: more than a twentieth
        // of the way round, less than a fifth.
        let plan = plan_route(vec![
            at(WALLBERG.0, WALLBERG.1),
            at(EAST.0, EAST.1),
            at(NORTH.0, NORTH.1),
            at(47.6870, 11.8160),
        ]);

        let score = plan.score.unwrap();
        assert_ne!(score.kind, RouteKind::FreeFlight);
        assert!(!score.closed);
        let gap = score.closing_m.unwrap();
        assert!(gap > 1500.0 && gap < 6000.0, "{gap}");
        assert!(
            plan.words.contains("Start and finish lie"),
            "{}",
            plan.words
        );
        assert!(plan.words.contains("scores as a"), "{}", plan.words);
    }

    #[test]
    fn names_take_the_article_that_fits() {
        assert_eq!(with_article("Free flight"), "a free flight");
        assert_eq!(with_article("FAI triangle"), "an FAI triangle");
        assert_eq!(with_article("Closed FAI triangle"), "a closed FAI triangle");
        assert_eq!(with_article("Triangle"), "a triangle");
    }
}
