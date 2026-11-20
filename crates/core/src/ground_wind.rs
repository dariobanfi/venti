//! The wind on the ground while the glider is in the air.
//!
//! In flight a pilot knows the wind at their own height, from the drift
//! of their circles. What it does on the valley floor, where they will
//! land, they do not: that is what wind stations measure. With a network
//! the cockpit asks the pilot's server for the stations around; the core
//! picks the ones to show and says when it blows hard where the pilot
//! means to land. Without a network there is nothing to say, and nothing
//! is said: no word about the wind is not a word that it is calm.

use venti_domain::GeoPoint;
use venti_flight::live::navigation::bearing_deg;

use crate::airspace::{compass, distance_in_words};
use crate::{Coordinate, WindStation, wind_speeds_label};

/// No more stations than this are listed.
const STATIONS_AT_MOST: usize = 8;

/// Stations further from the glider than this are left out, metres.
/// Wide, because the stations of the weather services stand tens of
/// kilometres apart, and a summit station a valley away is what shows a
/// foehn coming.
const AROUND_M: f64 = 75_000.0;

/// A station closer to the glider than this is near enough for its wind
/// to stand in the cockpit without its distance, metres. To verify.
const NEAR_M: f64 = 15_000.0;

/// A station speaks for a landing when it stands within this distance of
/// it, metres. To verify with pilots who know their valleys: a station
/// round a corner measures another wind.
const LANDING_AROUND_M: f64 = 3_000.0;

/// And within this height of it, metres: a station on a summit above a
/// landing measures the wind up there.
const LANDING_HEIGHT_M: f64 = 250.0;

/// The wind is strong for a landing when it leaves the glider less than
/// this much way into it at trim speed, metres per second: 10 km/h, as
/// for the wind aloft.
const STRONG_MARGIN_MS: f64 = 10.0 / 3.6;

/// Without a trim speed to measure against, the wind counts as strong
/// from this mean on, metres per second: 25 km/h. To verify.
const STRONG_MEAN_MS: f64 = 25.0 / 3.6;

/// And from these gusts on: 35 km/h. To verify.
const STRONG_GUST_MS: f64 = 35.0 / 3.6;

/// The place the pilot means to land at: the place the cockpit navigates
/// to, or the landing it names as within reach.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct GroundWindPlace {
    /// Its name.
    pub name: String,
    /// Where it is.
    pub position: Coordinate,
    /// Its height above sea level, metres, if known.
    pub elevation_m: Option<f64>,
}

/// One wind station near the glider, with what it measured last.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct GroundWindStation {
    /// The station's identifier.
    pub id: String,
    /// Its name.
    pub name: String,
    /// How far from the glider it stands, metres.
    pub distance_m: f64,
    /// The direction from the glider to it, degrees clockwise from north.
    pub bearing_deg: f64,
    /// Its height above sea level, metres.
    pub elevation_m: Option<f64>,
    /// The direction the wind comes from, degrees clockwise from north.
    pub from_deg: Option<f64>,
    /// The mean wind, metres per second.
    pub speed_ms: f64,
    /// The gusts, metres per second.
    pub gust_ms: Option<f64>,
    /// How old the reading is, minutes.
    pub age_min: i64,
    /// Where it stands, in words: "4.2 km SW · 740 m".
    pub place: String,
    /// What it measured, in words: "NW 12/25 km/h · 6 min ago".
    pub wind: String,
    /// The few characters of the cockpit's badge: mean and gusts in
    /// km/h, "12/25", and for a station that neither speaks for the
    /// landing nor stands near the glider how far off it is, "12/25 ·
    /// 54 km". A wind from that far is another valley's.
    pub badge: String,
    /// Whether it is the station that speaks for the landing.
    pub is_at_landing: bool,
    /// Whether its wind is strong for a landing.
    pub is_strong: bool,
}

/// A warning that it blows hard where the pilot means to land.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GroundWindAlert {
    /// The station it is from. The shell says a warning once for a
    /// station, not every time it asks.
    pub station_id: String,
    /// "Strong wind at Sample landing field".
    pub title: String,
    /// "Rottach-Egern, 1.2 km from it: NW 28/41 km/h, 6 min ago".
    pub detail: String,
    /// For the voice: "Strong wind at the landing. 28 kilometres per
    /// hour, gusts 41, from the north-west."
    pub spoken: String,
}

/// The wind on the ground around the glider.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct GroundWind {
    /// The stations with a fresh reading, the one that speaks for the
    /// landing first, then the nearest to the glider first.
    pub stations: Vec<GroundWindStation>,
    /// Set when it blows hard at the landing.
    pub alert: Option<GroundWindAlert>,
}

/// Picks from the stations the server has around the glider those to
/// show, and says whether it blows hard at the landing.
///
/// `trim_speed_ms` is the glider's speed with the brakes up, by the
/// pilot's setting; zero or less when there is none.
#[uniffi::export]
pub fn ground_wind(
    stations: Vec<WindStation>,
    position: Coordinate,
    landing: Option<GroundWindPlace>,
    trim_speed_ms: f64,
) -> GroundWind {
    let nothing = GroundWind {
        stations: Vec::new(),
        alert: None,
    };
    let Ok(here) = GeoPoint::new(position.lat, position.lon) else {
        return nothing;
    };
    let landing_at = landing
        .as_ref()
        .and_then(|place| GeoPoint::new(place.position.lat, place.position.lon).ok());

    // Every station with a mean wind that the wind map counts as
    // current: the weather services' readings arrive up to three quarters
    // of an hour late, and each is shown with its age.
    let mut fresh: Vec<(WindStation, GeoPoint, f64, i64)> = stations
        .into_iter()
        .filter_map(|station| {
            let speed_ms = station.latest.as_ref()?.speed_ms?;
            let age_min = station.age_min?;
            let at = GeoPoint::new(station.position.lat, station.position.lon).ok()?;
            (!station.is_old && age_min >= 0 && speed_ms >= 0.0)
                .then_some((station, at, speed_ms, age_min))
        })
        .collect();
    fresh.sort_by(|one, other| {
        here.distance_m(one.1)
            .total_cmp(&here.distance_m(other.1))
            .then_with(|| one.0.id.cmp(&other.0.id))
    });

    // The one that speaks for the landing: the nearest to it that stands
    // close by and about as high.
    let at_landing: Option<String> = landing_at.and_then(|landing_at| {
        let landing_m = landing.as_ref().and_then(|place| place.elevation_m);
        fresh
            .iter()
            .filter(|(station, at, _, _)| {
                let about_as_high = match (landing_m, station.elevation_m) {
                    (Some(landing_m), Some(station_m)) => {
                        (landing_m - station_m).abs() <= LANDING_HEIGHT_M
                    }
                    // Where a height is missing, nearness has to do.
                    _ => true,
                };
                landing_at.distance_m(*at) <= LANDING_AROUND_M && about_as_high
            })
            .min_by(|one, other| {
                landing_at
                    .distance_m(one.1)
                    .total_cmp(&landing_at.distance_m(other.1))
            })
            .map(|(station, _, _, _)| station.id.clone())
    });

    let mut shown: Vec<GroundWindStation> = fresh
        .iter()
        .filter(|(station, at, _, _)| {
            here.distance_m(*at) <= AROUND_M || at_landing.as_deref() == Some(station.id.as_str())
        })
        .map(|(station, at, speed_ms, age_min)| {
            let reading = station.latest.as_ref();
            let gust_ms = reading.and_then(|reading| reading.gust_ms);
            let from_deg = reading.and_then(|reading| reading.direction_deg);
            let (distance_m, bearing) = (here.distance_m(*at), bearing_deg(here, *at));
            let mut place = format!("{} {}", distance_in_words(distance_m), compass(bearing));
            if let Some(elevation_m) = station.elevation_m {
                place.push_str(&format!(" · {elevation_m:.0} m"));
            }
            let is_at_landing = at_landing.as_deref() == Some(station.id.as_str());
            let mut badge = wind_speeds_label(*speed_ms, gust_ms);
            if !is_at_landing && distance_m > NEAR_M {
                badge.push_str(&format!(" · {:.0} km", distance_m / 1000.0));
            }
            GroundWindStation {
                id: station.id.clone(),
                name: station.name.clone(),
                distance_m,
                bearing_deg: bearing,
                elevation_m: station.elevation_m,
                from_deg,
                speed_ms: *speed_ms,
                gust_ms,
                age_min: *age_min,
                place,
                wind: format!(
                    "{} · {}",
                    wind_in_words(from_deg, *speed_ms, gust_ms),
                    age_in_words(*age_min)
                ),
                badge,
                is_at_landing,
                is_strong: is_strong(*speed_ms, gust_ms, trim_speed_ms),
            }
        })
        .collect();
    // The landing's station first; the others stay nearest first.
    shown.sort_by_key(|station| !station.is_at_landing);
    shown.truncate(STATIONS_AT_MOST);

    let alert = match (&landing, landing_at, shown.first()) {
        (Some(place), Some(landing_at), Some(station))
            if station.is_at_landing && station.is_strong =>
        {
            let away = fresh
                .iter()
                .find(|(found, _, _, _)| found.id == station.id)
                .map_or(0.0, |(_, at, _, _)| landing_at.distance_m(*at));
            Some(GroundWindAlert {
                station_id: station.id.clone(),
                title: format!("Strong wind at {}", place.name),
                detail: format!(
                    "{}, {} from it: {}, {}",
                    station.name,
                    distance_in_words(away),
                    wind_in_words(station.from_deg, station.speed_ms, station.gust_ms),
                    age_in_words(station.age_min)
                ),
                spoken: spoken(station.from_deg, station.speed_ms, station.gust_ms),
            })
        }
        _ => None,
    };

    GroundWind {
        stations: shown,
        alert,
    }
}

/// Whether a wind is strong for landing a paraglider in.
fn is_strong(speed_ms: f64, gust_ms: Option<f64>, trim_speed_ms: f64) -> bool {
    if trim_speed_ms > 0.0 {
        // Little way into the mean wind, or gusts as fast as the glider.
        trim_speed_ms - speed_ms < STRONG_MARGIN_MS
            || gust_ms.is_some_and(|gust_ms| gust_ms >= trim_speed_ms)
    } else {
        speed_ms >= STRONG_MEAN_MS || gust_ms.is_some_and(|gust_ms| gust_ms >= STRONG_GUST_MS)
    }
}

/// "NW 12/25 km/h", or "12 km/h" where the station gives no direction.
fn wind_in_words(from_deg: Option<f64>, speed_ms: f64, gust_ms: Option<f64>) -> String {
    let speeds = wind_speeds_label(speed_ms, gust_ms);
    match from_deg {
        Some(from_deg) => format!("{} {speeds} km/h", compass(from_deg)),
        None => format!("{speeds} km/h"),
    }
}

/// "just now", "6 min ago".
fn age_in_words(age_min: i64) -> String {
    if age_min < 1 {
        "just now".to_owned()
    } else {
        format!("{age_min} min ago")
    }
}

/// The warning as it is said aloud.
fn spoken(from_deg: Option<f64>, speed_ms: f64, gust_ms: Option<f64>) -> String {
    let whole = |speed_ms: f64| (speed_ms * 3.6).round();
    let mut words = format!(
        "Strong wind at the landing. {:.0} kilometres per hour",
        whole(speed_ms)
    );
    if let Some(gust_ms) = gust_ms.filter(|gust_ms| whole(*gust_ms) > whole(speed_ms)) {
        words.push_str(&format!(", gusts {:.0}", whole(gust_ms)));
    }
    if let Some(from_deg) = from_deg {
        let from = match compass(from_deg) {
            "N" => "north",
            "NE" => "north-east",
            "E" => "east",
            "SE" => "south-east",
            "S" => "south",
            "SW" => "south-west",
            "W" => "west",
            _ => "north-west",
        };
        words.push_str(&format!(", from the {from}"));
    }
    words.push('.');
    words
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WindMark, WindReading, WindSymbol};

    /// A station with a reading: where, how high, and the wind in km/h.
    fn station(
        id: &str,
        lat: f64,
        lon: f64,
        elevation_m: Option<f64>,
        wind_kmh: Option<(f64, Option<f64>)>,
        age_min: i64,
    ) -> WindStation {
        WindStation {
            id: id.to_owned(),
            name: format!("Station {id}"),
            source: "made-up".to_owned(),
            position: Coordinate { lat, lon },
            elevation_m,
            latest: wind_kmh.map(|(mean, gusts)| WindReading {
                time_unix_s: 0,
                speed_ms: Some(mean / 3.6),
                gust_ms: gusts.map(|gusts| gusts / 3.6),
                direction_deg: Some(315.0),
                temperature_c: None,
            }),
            age_min: Some(age_min),
            is_old: age_min > 60,
            mark: WindMark {
                symbol: WindSymbol::Calm,
                heading_deg: 135.0,
                label: String::new(),
            },
        }
    }

    const GLIDER: Coordinate = Coordinate {
        lat: 47.6642,
        lon: 11.7967,
    };

    fn landing() -> GroundWindPlace {
        GroundWindPlace {
            name: "Sample landing field".to_owned(),
            position: Coordinate {
                lat: 47.6833,
                lon: 11.7550,
            },
            elevation_m: Some(735.0),
        }
    }

    /// Thirty-eight km/h at trim.
    const TRIM_MS: f64 = 38.0 / 3.6;

    #[test]
    fn the_stations_around_are_listed_nearest_first_with_fresh_readings_only() {
        let stations = vec![
            station("far", 47.80, 11.80, Some(700.0), Some((10.0, None)), 5),
            station(
                "near",
                47.67,
                11.80,
                Some(1600.0),
                Some((12.0, Some(25.0))),
                6,
            ),
            station(
                "stale",
                47.665,
                11.797,
                Some(1600.0),
                Some((40.0, None)),
                70,
            ),
            station("silent", 47.665, 11.797, Some(1600.0), None, 2),
            station("elsewhere", 48.60, 11.80, Some(500.0), Some((5.0, None)), 1),
        ];

        let wind = ground_wind(stations, GLIDER, None, TRIM_MS);

        // The stale one, the one without a wind, and the one a hundred
        // kilometres off are left out.
        let ids: Vec<&str> = wind
            .stations
            .iter()
            .map(|station| station.id.as_str())
            .collect();
        assert_eq!(ids, ["near", "far"]);
        assert_eq!(wind.alert, None);
        let near = &wind.stations[0];
        assert!(
            near.distance_m > 600.0 && near.distance_m < 800.0,
            "{near:?}"
        );
        assert_eq!(near.place, "690 m N · 1600 m");
        assert_eq!(near.wind, "NW 12/25 km/h · 6 min ago");
        // Near the glider its wind stands for itself; the one fifteen
        // kilometres to the north says how far off it is.
        assert_eq!(near.badge, "12/25");
        assert_eq!(wind.stations[1].badge, "10 · 15 km");
        assert!(!near.is_at_landing && !near.is_strong);
        assert_eq!(wind.stations[1].wind, "NW 10 km/h · 5 min ago");
        // Without a place to land nothing speaks for one.
        assert!(wind.stations.iter().all(|station| !station.is_at_landing));
    }

    #[test]
    fn a_strong_wind_at_the_landing_is_warned_of() {
        let stations = vec![
            station(
                "summit",
                47.6662,
                11.7982,
                Some(1700.0),
                Some((45.0, Some(60.0))),
                3,
            ),
            station(
                "valley",
                47.690,
                11.760,
                Some(740.0),
                Some((30.0, Some(41.0))),
                6,
            ),
            station(
                "valley-far",
                47.720,
                11.760,
                Some(730.0),
                Some((8.0, None)),
                2,
            ),
        ];

        let wind = ground_wind(stations.clone(), GLIDER, Some(landing()), TRIM_MS);

        // The valley station, 830 m from the landing, speaks for it and
        // leads the list; the summit's storm is the summit's.
        let first = &wind.stations[0];
        assert_eq!(first.id, "valley");
        assert!(first.is_at_landing && first.is_strong);
        assert!(
            wind.stations[1..]
                .iter()
                .all(|station| !station.is_at_landing)
        );
        let alert = wind.alert.unwrap();
        assert_eq!(alert.station_id, "valley");
        assert_eq!(alert.title, "Strong wind at Sample landing field");
        assert_eq!(
            alert.detail,
            "Station valley, 830 m from it: NW 30/41 km/h, 6 min ago"
        );
        assert_eq!(
            alert.spoken,
            "Strong wind at the landing. 30 kilometres per hour, gusts 41, from the north-west."
        );

        // A gentle wind there warns of nothing, whatever blows on the
        // summit.
        let mut calm = stations.clone();
        calm[1] = station(
            "valley",
            47.690,
            11.760,
            Some(740.0),
            Some((12.0, Some(20.0))),
            6,
        );
        let wind = ground_wind(calm, GLIDER, Some(landing()), TRIM_MS);
        assert_eq!(wind.alert, None);
        assert!(wind.stations[0].is_at_landing && !wind.stations[0].is_strong);

        // With no station close to the landing and about as high, none
        // speaks for it: a summit station right above does not.
        let above = vec![station(
            "summit",
            47.684,
            11.756,
            Some(1700.0),
            Some((45.0, None)),
            3,
        )];
        let wind = ground_wind(above, GLIDER, Some(landing()), TRIM_MS);
        assert_eq!(wind.alert, None);
        assert!(!wind.stations[0].is_at_landing);
        assert!(wind.stations[0].is_strong);
    }

    #[test]
    fn what_is_strong_is_measured_against_the_glider() {
        // At 38 km/h trim: a mean above 28, or gusts of 38.
        assert!(!is_strong(27.0 / 3.6, Some(35.0 / 3.6), TRIM_MS));
        assert!(is_strong(29.0 / 3.6, None, TRIM_MS));
        assert!(is_strong(15.0 / 3.6, Some(38.0 / 3.6), TRIM_MS));
        // A faster glider has more room.
        assert!(!is_strong(29.0 / 3.6, None, 45.0 / 3.6));
        // Without a trim speed: 25 km/h, or gusts of 35.
        assert!(!is_strong(24.0 / 3.6, Some(34.0 / 3.6), 0.0));
        assert!(is_strong(25.0 / 3.6, None, 0.0));
        assert!(is_strong(10.0 / 3.6, Some(35.0 / 3.6), 0.0));
        // Gusts that round to the mean are not said twice, and a station
        // without a vane gives no direction.
        assert_eq!(
            spoken(None, 30.0 / 3.6, Some(30.2 / 3.6)),
            "Strong wind at the landing. 30 kilometres per hour."
        );
        assert_eq!(wind_in_words(None, 3.0, None), "11 km/h");
        assert_eq!(age_in_words(0), "just now");
    }

    #[test]
    fn a_landing_far_from_the_glider_still_has_its_station() {
        // A hundred kilometres to the west: beyond what is listed around
        // the glider, but it is where the pilot means to land.
        let goal = GroundWindPlace {
            name: "Goal".to_owned(),
            position: Coordinate {
                lat: 47.66,
                lon: 10.46,
            },
            elevation_m: None,
        };
        let stations = vec![
            station("goal", 47.665, 10.465, Some(600.0), Some((33.0, None)), 4),
            station("near", 47.67, 11.80, Some(1600.0), Some((12.0, None)), 6),
        ];

        let wind = ground_wind(stations, GLIDER, Some(goal), TRIM_MS);

        assert_eq!(wind.stations[0].id, "goal");
        assert!(wind.stations[0].distance_m > 99_000.0);
        // It speaks for the landing: no distance beside its wind.
        assert_eq!(wind.stations[0].badge, "33");
        assert_eq!(wind.alert.unwrap().title, "Strong wind at Goal");
        assert_eq!(wind.stations.len(), 2);
    }
}
