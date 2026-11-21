//! Climbs near the glider on the cockpit's map: where others circle and
//! climb at this moment, the nearest first, each with its direction and
//! distance.
//!
//! The server answers with the climbs of the last minutes around the
//! glider ([`crate::live_air`]): places in the air and numbers, no
//! glider. The cockpit asks again every half minute, and the session
//! keeps the last answer and ages it: a climb is given until it is as
//! old as the server lists climbs for.
//!
//! This is a hint, not an instrument: a climb others found minutes ago
//! may be gone, and none is promised between them. Nothing here changes
//! what the cockpit says about airspace, terrain or landing.

use venti_domain::GeoPoint;
use venti_flight::live::navigation::bearing_deg;

use crate::airspace::compass;
use crate::community::area_around;
use crate::live_air::{ClimbView, LiveClimbs, live_climbs_path, signed};
use crate::wind::Credit;

/// Climbs farther than this from the glider, metres, are not given.
const NEAR_M: f64 = 15_000.0;

/// The most climbs given, the nearest.
const AT_MOST: usize = 5;

/// A climb near the glider, as the cockpit's map draws it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ClimbNearView {
    /// The climb. Its age is counted to the moment asked for.
    pub climb: ClimbView,
    /// How far it is from the glider, metres.
    #[uniffi(default = 0.0)]
    pub distance_m: f64,
    /// The direction to it from the glider, degrees clockwise from north.
    #[uniffi(default = 0.0)]
    pub bearing_deg: f64,
    /// How far its top is above the glider, metres; negative when the
    /// glider is above it. Absent without the glider's height.
    #[uniffi(default = None)]
    pub top_above_m: Option<f64>,
    /// "Climb 1.8 km NE, +2.1 m/s, 3 min ago".
    #[uniffi(default = "")]
    pub line: String,
}

/// The climbs near the glider.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ClimbsNear {
    /// Nearest first, within 15 km, at most five.
    pub climbs: Vec<ClimbNearView>,
    /// Credits to show while they are drawn: those of the networks whose
    /// positions a climb in the list is from.
    pub credits: Vec<Credit>,
}

/// The last answer about climbs the session was handed.
#[derive(Debug, Clone, Default)]
pub(crate) struct ClimbsHeard {
    answer: Option<(LiveClimbs, f64)>,
}

impl ClimbsHeard {
    /// Takes in an answer of the server, read at `now_s`.
    pub(crate) fn take(&mut self, climbs: LiveClimbs, now_s: f64) {
        self.answer = Some((climbs, now_s));
    }

    /// The climbs near the glider at `now_s`.
    pub(crate) fn near(
        &self,
        glider: Option<GeoPoint>,
        altitude_m: Option<f64>,
        now_s: f64,
    ) -> ClimbsNear {
        let (Some(here), Some((heard, heard_s))) = (glider, &self.answer) else {
            return ClimbsNear {
                climbs: Vec::new(),
                credits: Vec::new(),
            };
        };
        let listed_s = f64::from(heard.listed_minutes) * 60.0;
        let since = (now_s - heard_s).max(0.0);
        let mut climbs: Vec<ClimbNearView> = heard
            .climbs
            .iter()
            .filter_map(|climb| {
                let age_s = climb.age_s + since;
                let there = GeoPoint::new(climb.position.lat, climb.position.lon).ok()?;
                let distance_m = here.distance_m(there);
                if age_s > listed_s || distance_m > NEAR_M {
                    return None;
                }
                let bearing = bearing_deg(here, there);
                let ago = if age_s < 60.0 {
                    "just now".to_owned()
                } else {
                    format!("{} min ago", (age_s / 60.0).round())
                };
                Some(ClimbNearView {
                    line: format!(
                        "Climb {:.1} km {}, {} m/s, {ago}",
                        distance_m / 1000.0,
                        compass(bearing),
                        signed(climb.climb_ms)
                    ),
                    top_above_m: altitude_m.map(|glider_m| climb.to_m - glider_m),
                    climb: ClimbView {
                        age_s,
                        ..climb.clone()
                    },
                    distance_m,
                    bearing_deg: bearing,
                })
            })
            .collect();
        climbs.sort_by(|a, b| a.distance_m.total_cmp(&b.distance_m));
        climbs.truncate(AT_MOST);
        let credits = heard
            .credits
            .iter()
            .filter(|credit| climbs.iter().any(|near| near.climb.source == credit.source))
            .cloned()
            .collect();
        ClimbsNear { climbs, credits }
    }
}

/// The path and query to `GET` for the climbs around the glider: the box
/// 15 km to each side. Ask every half minute in the air while the pilot
/// has the climbs switched on, hand each answer, as
/// [`crate::live_climbs`] reads it, to
/// [`crate::FlightSession::on_climbs`], and draw
/// [`crate::FlightSession::climbs_near`]. `None` for a place that is none.
#[uniffi::export]
pub fn cockpit_climbs_path(lat: f64, lon: f64) -> Option<String> {
    let area = area_around(lat, lon, NEAR_M).ok()?;
    live_climbs_path(area.south, area.west, area.north, area.east)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Coordinate;

    fn climb(lat: f64, lon: f64, age_s: f64, climb_ms: f64) -> ClimbView {
        ClimbView {
            position: Coordinate { lat, lon },
            climb_ms,
            from_m: 1600.0,
            to_m: 2100.0,
            pilots: 3,
            age_s,
            source: "ogn".to_owned(),
            line: String::new(),
            label: String::new(),
        }
    }

    fn answer(climbs: Vec<ClimbView>) -> LiveClimbs {
        LiveClimbs {
            climbs,
            credits: vec![Credit {
                source: "ogn".to_owned(),
                provider: "Open Glider Network".to_owned(),
                licence: "ODbL 1.0".to_owned(),
                text: "Open Glider Network".to_owned(),
                url: "https://www.glidernet.org".to_owned(),
                notice: None,
            }],
            truncated: false,
            listed_minutes: 10,
            count_words: String::new(),
        }
    }

    #[test]
    fn the_nearest_climbs_are_given_with_direction_distance_and_age() {
        let glider = GeoPoint::new(47.70, 11.80).ok();
        let mut heard = ClimbsHeard::default();
        assert!(heard.near(glider, None, 0.0).climbs.is_empty());
        let start = 1_000_000.0;
        heard.take(
            answer(vec![
                // 1.8 km to the north-east, three minutes old.
                climb(47.7115, 11.8170, 170.0, 2.1),
                // 5.6 km south, now.
                climb(47.65, 11.80, 0.0, 1.2),
                // Far away.
                climb(48.2, 11.8, 0.0, 4.0),
            ]),
            start,
        );
        // Ten seconds after the answer.
        let near = heard.near(glider, Some(1900.0), start + 10.0);
        assert_eq!(near.climbs.len(), 2);
        assert_eq!(near.climbs[0].line, "Climb 1.8 km NE, +2.1 m/s, 3 min ago");
        assert_eq!(near.climbs[0].top_above_m, Some(200.0));
        assert_eq!(near.climbs[0].climb.age_s, 180.0);
        assert_eq!(near.climbs[1].line, "Climb 5.6 km S, +1.2 m/s, just now");
        assert_eq!(near.credits.len(), 1);

        // Eight minutes on with no new answer, the older one is past the
        // ten minutes climbs are listed for; eleven on, both are.
        let later = heard.near(glider, None, start + 480.0);
        assert_eq!(later.climbs.len(), 1);
        assert_eq!(later.climbs[0].line, "Climb 5.6 km S, +1.2 m/s, 8 min ago");
        let gone = heard.near(glider, None, start + 660.0);
        assert!(gone.climbs.is_empty() && gone.credits.is_empty());
        // Without a position of the glider nothing is near.
        assert!(heard.near(None, None, start).climbs.is_empty());
    }

    #[test]
    fn the_cockpit_asks_for_the_box_around_the_glider() {
        let path = cockpit_climbs_path(47.7, 11.8).unwrap();
        assert!(path.starts_with("/v1/live/climbs?bbox=11.59"), "{path}");
        assert!(path.contains(",47.56") && path.contains(",47.83"), "{path}");
        assert_eq!(cockpit_climbs_path(f64::NAN, 11.8), None);
    }
}
