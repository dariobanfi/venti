//! One station where two sources measure at the same place.
//!
//! An airfield's weather report and the weather service's own station on
//! that airfield are two stations to the store, each with its source, its
//! identifier and its history, and they stay two there. What is handed out
//! as the stations of an area holds one of them: the one whose source
//! stands first in the regions' priority. Nothing is copied from the other,
//! so every station that is handed out still belongs to one source.
//!
//! **The same place** is a fixed rule, from what was measured on 2026-11-21
//! between the 435 stations of the first region's four sources:
//!
//! - *No farther apart than 2.75 km.* Ten airfields had a station of DWD or
//!   of GeoSphere Austria beside them, between 40 m and 2.45 km from the
//!   position their report gives. The nearest pair that is known to be two
//!   places stood 3.03 km apart: Innsbruck's airfield and the university in
//!   the town, known because GeoSphere has a station on the airfield as
//!   well. The bound lies halfway between the two.
//! - *No more than 50 m apart in height.* The ten pairs differed by 0 to
//!   9 m. A station in a valley and one on the ridge above it, which can
//!   stand as near to each other, differ by hundreds. A station whose
//!   height is not known is at the same place as no other.
//! - *Of different sources.* Two stations of one source are two stations,
//!   however near: the source says so.
//!
//! **Which one is handed out:** the one that has a wind to show, and of two
//! that have, or of two that have not, the one whose source stands first in
//! the priority. A wind to show is a wind speed measured within the last
//! hour, which is how long the apps show a reading as current. So while the
//! weather service's station is silent, or measures no wind, the airfield's
//! report is not kept back for its sake. Two sources of which neither is
//! named in the priority have no order, and both their stations go out.
//!
//! The distance, the height, the hour, and whether one record should one
//! day be made of two are the owner's to change: see the decision log.

use chrono::Duration;
use venti_domain::{BoundingBox, SourceId, Station, Timestamp};
use venti_store::StationLatest;

/// The farthest apart two stations at the same place stand, in metres.
pub(crate) const SAME_PLACE_M: f64 = 2750.0;

/// The most the heights of two stations at the same place differ, in
/// metres.
pub(crate) const SAME_HEIGHT_M: f64 = 50.0;

/// How long a wind reading counts as a wind to show, in minutes.
pub(crate) const WIND_TO_SHOW_MIN: i64 = 60;

/// The length of a degree of latitude on the sphere distances are measured
/// on, in metres.
const DEGREE_M: f64 = 111_195.0;

/// Whether two stations are the same place measured by two sources.
pub(crate) fn same_place(a: &Station, b: &Station) -> bool {
    if a.id.source() == b.id.source() {
        return false;
    }
    let (Some(height_a), Some(height_b)) = (a.elevation_m, b.elevation_m) else {
        return false;
    };
    (height_a - height_b).abs() <= SAME_HEIGHT_M
        // The latitudes alone rule out nearly every pair, without a cosine.
        && (a.position.lat() - b.position.lat()).abs() * DEGREE_M <= SAME_PLACE_M
        && a.position.distance_m(b.position) <= SAME_PLACE_M
}

/// The box with as much room on every side as two stations at the same
/// place can stand apart, and a little more.
///
/// Whether a station is handed out must not depend on where a box ends: a
/// station at the edge is judged against its neighbour just outside.
pub(crate) fn with_room(area: &BoundingBox) -> BoundingBox {
    let lat = SAME_PLACE_M / DEGREE_M * 1.01;
    // A degree of longitude is shortest at the edge nearest to a pole.
    let nearest_to_a_pole = area.south().abs().max(area.north().abs()) + lat;
    let lon = if nearest_to_a_pole < 89.0 {
        lat / nearest_to_a_pole.to_radians().cos()
    } else {
        360.0
    };
    BoundingBox::new(
        (area.south() - lat).max(-90.0),
        (area.west() - lon).max(-180.0),
        (area.north() + lat).min(90.0),
        (area.east() + lon).min(180.0),
    )
    .unwrap_or(*area)
}

/// Which of `stations` are not handed out, in the order of `stations`:
/// `true` for a station that another source's station at the same place
/// stands for.
///
/// `priority` names the sources in the order they are preferred; a source
/// it does not name comes after all it names.
pub(crate) fn left_out(
    stations: &[StationLatest],
    priority: &[SourceId],
    now: Timestamp,
) -> Vec<bool> {
    struct Candidate {
        index: usize,
        has_wind: bool,
        rank: usize,
    }
    let rank = |source: &SourceId| {
        priority
            .iter()
            .position(|named| named == source)
            .unwrap_or(priority.len())
    };
    let mut candidates: Vec<Candidate> = stations
        .iter()
        .enumerate()
        .map(|(index, entry)| Candidate {
            index,
            has_wind: has_wind_to_show(entry, now),
            rank: rank(entry.station.id.source()),
        })
        .collect();
    // The best first. A station is then judged only against stations that
    // are handed out themselves, so that nothing is left out for the sake
    // of a station that is left out too.
    candidates.sort_by_key(|candidate| (!candidate.has_wind, candidate.rank));

    let mut left_out = vec![false; stations.len()];
    let mut handed_out: Vec<&Candidate> = Vec::new();
    for candidate in &candidates {
        let station = &stations[candidate.index].station;
        let stood_for = handed_out.iter().any(|better| {
            // Two sources that the priority gives the same place, which is
            // none, have no order: neither stands for the other.
            let preferred = better.rank != candidate.rank
                && (better.has_wind != candidate.has_wind || better.rank < candidate.rank);
            preferred && same_place(&stations[better.index].station, station)
        });
        if stood_for {
            left_out[candidate.index] = true;
        } else {
            handed_out.push(candidate);
        }
    }
    left_out
}

/// Whether a station's newest observation is a wind an app shows as
/// current.
fn has_wind_to_show(entry: &StationLatest, now: Timestamp) -> bool {
    entry.latest.as_ref().is_some_and(|reading| {
        reading.wind_speed_ms.is_some() && now - reading.time <= Duration::minutes(WIND_TO_SHOW_MIN)
    })
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use venti_domain::{GeoPoint, Observation};

    use super::*;

    fn now() -> Timestamp {
        Utc.with_ymd_and_hms(2026, 11, 21, 10, 52, 0).unwrap()
    }

    /// A station with a wind measured `age_min` minutes ago, or with no
    /// observation at all.
    fn station(id: &str, lat: f64, lon: f64, height: f64, age_min: Option<i64>) -> StationLatest {
        let station = Station {
            id: id.parse().unwrap(),
            name: id.to_owned(),
            position: GeoPoint::new(lat, lon).unwrap(),
            elevation_m: Some(height),
        };
        let latest = age_min.map(|age| {
            let mut observation =
                Observation::empty(station.id.clone(), now() - Duration::minutes(age));
            observation.wind_speed_ms = Some(3.0);
            observation
        });
        StationLatest { station, latest }
    }

    fn priority() -> Vec<SourceId> {
        [
            "dwd-10min",
            "geosphere-tawes",
            "suedtirol-meteo",
            "awc-metar",
        ]
        .map(|source| SourceId::new(source).unwrap())
        .to_vec()
    }

    /// The identifiers of the stations that are left out.
    fn out(stations: &[StationLatest]) -> Vec<String> {
        left_out(stations, &priority(), now())
            .into_iter()
            .zip(stations)
            .filter(|(left_out, _)| *left_out)
            .map(|(_, entry)| entry.station.id.to_string())
            .collect()
    }

    // Positions and heights as the sources gave them on 2026-11-21.
    fn innsbruck_airfield_by_geosphere(age_min: Option<i64>) -> StationLatest {
        station("geosphere-tawes:11121", 47.26, 11.356_667, 578.0, age_min)
    }
    fn innsbruck_airfield_by_its_report(age_min: Option<i64>) -> StationLatest {
        station("awc-metar:LOWI", 47.26, 11.344, 570.0, age_min)
    }
    fn innsbruck_university(age_min: Option<i64>) -> StationLatest {
        station("geosphere-tawes:11320", 47.26, 11.384_167, 578.0, age_min)
    }

    #[test]
    fn an_airfields_report_gives_way_to_the_weather_services_station_beside_it() {
        // 0.96 km and 8 m apart.
        let stations = [
            innsbruck_airfield_by_its_report(Some(22)),
            innsbruck_airfield_by_geosphere(Some(2)),
            innsbruck_university(Some(2)),
        ];
        assert_eq!(out(&stations), ["awc-metar:LOWI"]);
        // The order they come in changes nothing.
        let mut turned = stations.clone();
        turned.reverse();
        assert_eq!(out(&turned), ["awc-metar:LOWI"]);
    }

    #[test]
    fn three_kilometres_apart_are_two_places() {
        // The airfield and the university: 3.03 km and 8 m apart.
        let stations = [
            innsbruck_airfield_by_its_report(Some(22)),
            innsbruck_university(Some(2)),
        ];
        assert!(out(&stations).is_empty());
        // The farthest of the ten pairs measured: Wiener Neustadt's
        // airfield and GeoSphere's station, 2.45 km and 9 m apart.
        let stations = [
            station("awc-metar:LOAN", 47.843, 16.26, 266.0, Some(52)),
            station(
                "geosphere-tawes:11182",
                47.832_222,
                16.231_389,
                275.0,
                Some(2),
            ),
        ];
        assert_eq!(out(&stations), ["awc-metar:LOAN"]);
    }

    #[test]
    fn a_valley_and_the_ridge_above_it_are_two_places() {
        // A kilometre apart on the map and 400 m in height.
        let ridge = station("awc-metar:XXXX", 47.26, 11.344, 978.0, Some(5));
        assert!(out(&[ridge, innsbruck_airfield_by_geosphere(Some(2))]).is_empty());
        // 50 m is still the same place, 51 m is not.
        let at = |height| {
            let report = station("awc-metar:XXXX", 47.26, 11.344, height, Some(5));
            out(&[report, innsbruck_airfield_by_geosphere(Some(2))]).len()
        };
        assert_eq!((at(528.0), at(628.0)), (1, 1));
        assert_eq!((at(527.0), at(629.0)), (0, 0));
        // A height that is not known is no height to compare.
        let mut unknown = innsbruck_airfield_by_its_report(Some(5));
        unknown.station.elevation_m = None;
        assert!(out(&[unknown, innsbruck_airfield_by_geosphere(Some(2))]).is_empty());
    }

    #[test]
    fn two_stations_of_one_source_are_two_stations() {
        let beside = station("geosphere-tawes:99999", 47.261, 11.357, 578.0, Some(2));
        assert!(out(&[innsbruck_airfield_by_geosphere(Some(2)), beside]).is_empty());
    }

    #[test]
    fn a_silent_station_does_not_keep_back_one_that_has_a_wind() {
        // The weather service's station has not been heard for two hours,
        // or never: the report is what there is, and it goes out alone.
        for silent in [Some(120), None] {
            let stations = [
                innsbruck_airfield_by_its_report(Some(22)),
                innsbruck_airfield_by_geosphere(silent),
            ];
            assert_eq!(out(&stations), ["geosphere-tawes:11121"]);
        }
        // An hour is still a wind to show; a minute more is not.
        let stations = [
            innsbruck_airfield_by_its_report(Some(22)),
            innsbruck_airfield_by_geosphere(Some(WIND_TO_SHOW_MIN)),
        ];
        assert_eq!(out(&stations), ["awc-metar:LOWI"]);
        // It measures, and has no wind: the same.
        let mut no_wind = innsbruck_airfield_by_geosphere(Some(2));
        no_wind.latest.as_mut().unwrap().wind_speed_ms = None;
        no_wind.latest.as_mut().unwrap().temperature_c = Some(7.0);
        let stations = [innsbruck_airfield_by_its_report(Some(22)), no_wind];
        assert_eq!(out(&stations), ["geosphere-tawes:11121"]);
        // Both silent: the priority decides, as between two that speak.
        let stations = [
            innsbruck_airfield_by_its_report(Some(200)),
            innsbruck_airfield_by_geosphere(Some(120)),
        ];
        assert_eq!(out(&stations), ["awc-metar:LOWI"]);
    }

    #[test]
    fn nothing_is_left_out_for_the_sake_of_a_station_that_is_left_out() {
        // Three in a row, 2 km from one to the next and 4 km from end to
        // end. The middle one gives way to the first. The last is not at
        // the first one's place, and stays.
        let stations = [
            station("dwd-10min:1", 47.0, 11.0, 500.0, Some(5)),
            station("geosphere-tawes:2", 47.018, 11.0, 500.0, Some(5)),
            station("awc-metar:3", 47.036, 11.0, 500.0, Some(5)),
        ];
        assert_eq!(out(&stations), ["geosphere-tawes:2"]);
    }

    #[test]
    fn sources_without_a_place_in_the_priority_come_last_and_have_no_order() {
        let stranger = station("pioupiou:1", 47.26, 11.345, 570.0, Some(5));
        let other = station("holfuy:1", 47.26, 11.346, 570.0, Some(5));
        // Against a source that is named, a stranger gives way.
        let with_report = [stranger.clone(), innsbruck_airfield_by_its_report(Some(22))];
        assert_eq!(out(&with_report), ["pioupiou:1"]);
        // Two strangers are both handed out, also when one of them is
        // silent, and so is everything where no priority was given at all.
        assert!(out(&[stranger.clone(), other.clone()]).is_empty());
        let silent = station("holfuy:1", 47.26, 11.346, 570.0, None);
        assert!(out(&[stranger.clone(), silent]).is_empty());
        for heard in [Some(2), None] {
            let pair = [
                innsbruck_airfield_by_its_report(Some(22)),
                innsbruck_airfield_by_geosphere(heard),
            ];
            assert_eq!(left_out(&pair, &[], now()), [false, false]);
        }
    }

    #[test]
    fn the_room_around_a_box_holds_every_neighbour_at_the_same_place() {
        let area = BoundingBox::new(47.0, 11.0, 47.5, 11.5).unwrap();
        let wider = with_room(&area);
        // A station on a corner of the box, and points 2.75 km from it to
        // the north, the east and between them.
        let corner = GeoPoint::new(47.5, 11.5).unwrap();
        let degrees = SAME_PLACE_M / DEGREE_M;
        let east = degrees / 47.5_f64.to_radians().cos();
        for (north, east) in [(degrees, 0.0), (0.0, east), (degrees * 0.7, east * 0.7)] {
            let neighbour = GeoPoint::new(47.5 + north, 11.5 + east).unwrap();
            assert!(corner.distance_m(neighbour) <= SAME_PLACE_M + 1.0);
            assert!(wider.contains(neighbour), "{neighbour:?}");
        }
        // And not much more: under 3 km.
        assert!((wider.north() - area.north()) * DEGREE_M < 3000.0);
        // At the ends of the world it stays a box.
        let top = BoundingBox::new(89.5, -180.0, 90.0, 180.0).unwrap();
        assert_eq!(with_room(&top).north(), 90.0);
        assert_eq!(with_room(&top).west(), -180.0);
    }
}
