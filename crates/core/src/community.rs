//! Other pilots' flights: the flights the pilots of a Venti server made
//! public, in one list, as XContest lists what was flown.
//!
//! As everywhere, the core does not talk to the server: one function says
//! what to ask for, another reads the answer into rows for a screen, and
//! the shell does the request in between. A flight of the list is opened
//! by fetching its file and handing the text to
//! [`flight_from_file`](crate::flight_from_file), which shows it like one
//! of the pilot's own and keeps nothing.
//!
//! A flight that is reached by its address alone, as from a link, is read
//! by [`public_flight_path`] and [`public_flight`].
//!
//! What another pilot wrote, a name or a glider, is cut to one bounded
//! line before it reaches a screen.

use chrono::DateTime;
use venti_api_types::v1::{
    self as api, Flight, FlightResponse, FlightSort, FlightsResponse, PilotRequest, PilotResponse,
    PublicFlightsRequest,
};
use venti_domain::BoundingBox;

use crate::live::one_line;
use crate::server::answer;
use crate::{Bounds, Coordinate, CoreError, EntryScore, RouteKind};

/// The order of a list of public flights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum PublicFlightsOrder {
    /// The newest first, by when the flight began.
    Newest,
    /// The most points first. Flights too short to score are left out.
    Best,
}

/// One public flight as a row of a list shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PublicFlight {
    /// The flight's address on the server.
    pub id: String,
    /// Its pilot's handle.
    pub handle: String,
    /// The name its pilot is shown by, on one line.
    pub pilot: String,
    /// When it began, seconds since 1970-01-01 UTC.
    pub start_unix_s: i64,
    /// Seconds in the air.
    pub airtime_s: i64,
    /// Where it took off, if the recording shows a take-off.
    pub takeoff: Option<Coordinate>,
    /// Distance along the track, metres.
    pub track_length_m: f64,
    /// The highest altitude, metres.
    pub max_altitude_m: f64,
    /// The best climb averaged over half a minute, metres per second.
    pub max_climb_ms: f64,
    /// The glider, as its pilot's file names it, on one line.
    pub glider: Option<String>,
    /// How it scores as a cross-country flight, by the server's reckoning,
    /// which is the logbook's.
    pub score: Option<EntryScore>,
    /// The shape of the track in a few dozen points.
    pub outline: Vec<Coordinate>,
    /// The path of the flight's page on the server: "/f/…".
    pub page_path: String,
    /// The path to fetch its file from, to open the flight.
    pub file_path: String,
}

/// One page of a list of public flights.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PublicFlights {
    /// The flights, in the order asked for.
    pub flights: Vec<PublicFlight>,
    /// What to pass as `after` for the next page. Absent on the last.
    pub next: Option<String>,
}

/// The path and query to `GET` from the server for one page of the public
/// flights.
///
/// `area` narrows the list to flights that took off inside a part of the
/// map. `from_unix_s` and `to_unix_s` narrow it to flights that began at
/// or after the one and before the other: a day is its first second and
/// the first second of the next. `after` is the `next` of the page before,
/// asked for with everything else unchanged.
#[uniffi::export]
pub fn public_flights_path(
    order: PublicFlightsOrder,
    area: Option<Bounds>,
    from_unix_s: Option<i64>,
    to_unix_s: Option<i64>,
    after: Option<String>,
) -> Result<String, CoreError> {
    let unreadable = || CoreError::Server {
        message: "The app could not ask the server for these flights.".to_owned(),
    };
    let bbox = area
        .map(|area| {
            BoundingBox::new(
                area.south.clamp(-90.0, 90.0),
                area.west.clamp(-180.0, 180.0),
                area.north.clamp(-90.0, 90.0),
                area.east.clamp(-180.0, 180.0),
            )
            .map_err(|_| unreadable())
        })
        .transpose()?;
    let time = |seconds: Option<i64>| {
        seconds
            .map(|seconds| DateTime::from_timestamp(seconds, 0).ok_or_else(unreadable))
            .transpose()
    };
    let (from, to) = (time(from_unix_s)?, time(to_unix_s)?);
    if from.zip(to).is_some_and(|(from, to)| to <= from) {
        return Err(unreadable());
    }
    Ok(PublicFlightsRequest {
        bbox,
        from,
        to,
        sort: match order {
            // The server's own order: nothing to say.
            PublicFlightsOrder::Newest => None,
            PublicFlightsOrder::Best => Some(FlightSort::Score),
        },
        after: after.filter(|after| !after.is_empty()),
        limit: None,
    }
    .path_and_query())
}

/// The path and query to `GET` for the public flights that took off near a
/// place, the newest first: what was flown from a site. The server is
/// asked for the square around the place, `radius_m` to each side, at
/// most fifty kilometres.
#[uniffi::export]
pub fn public_flights_near_path(lat: f64, lon: f64, radius_m: f64) -> Result<String, CoreError> {
    const M_PER_DEGREE: f64 = 111_320.0;
    let unreadable = || CoreError::Server {
        message: "The app could not ask the server for the flights from this place.".to_owned(),
    };
    if !(lat.is_finite() && lon.is_finite() && radius_m.is_finite()) || lat.abs() > 89.0 {
        return Err(unreadable());
    }
    let reach = radius_m.clamp(100.0, 50_000.0);
    let north_south = reach / M_PER_DEGREE;
    let east_west = reach / (M_PER_DEGREE * lat.to_radians().cos());
    public_flights_path(
        PublicFlightsOrder::Newest,
        Some(Bounds {
            south: lat - north_south,
            west: lon - east_west,
            north: lat + north_south,
            east: lon + east_west,
        }),
        None,
        None,
        None,
    )
}

/// Reads the server's answer to [`public_flights_path`].
#[uniffi::export]
pub fn public_flights(status: u16, body: Vec<u8>) -> Result<PublicFlights, CoreError> {
    let response: FlightsResponse = answer(status, &body)?;
    Ok(PublicFlights {
        flights: response.flights.iter().map(row).collect(),
        next: response.next,
    })
}

/// One pilot and a page of the flights they share with everyone.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PilotPage {
    /// The pilot's handle.
    pub handle: String,
    /// The name the pilot is shown by, on one line.
    pub name: String,
    /// The pilot's public flights, the newest first.
    pub flights: Vec<PublicFlight>,
    /// What to pass as `after` for the next page. Absent on the last.
    pub next: Option<String>,
}

/// The path and query to `GET` from the server for a pilot and their
/// public flights. `after` is the `next` of the page before.
#[uniffi::export]
pub fn pilot_page_path(handle: String, after: Option<String>) -> Result<String, CoreError> {
    let handle = handle.parse().map_err(|_| CoreError::Server {
        message: "This is not the handle of a pilot.".to_owned(),
    })?;
    Ok(PilotRequest {
        handle,
        after: after.filter(|after| !after.is_empty()),
        limit: None,
    }
    .path_and_query())
}

/// Reads the server's answer to [`pilot_page_path`]. A pilot the server
/// does not know is said in words.
#[uniffi::export]
pub fn pilot_page(status: u16, body: Vec<u8>) -> Result<PilotPage, CoreError> {
    if status == 404 {
        return Err(CoreError::Server {
            message: "The server knows no pilot by this handle.".to_owned(),
        });
    }
    let response: PilotResponse = answer(status, &body)?;
    Ok(PilotPage {
        handle: response.pilot.handle.to_string(),
        name: one_line(&response.pilot.display_name, 40),
        flights: response.flights.iter().map(row).collect(),
        next: response.next,
    })
}

fn row(flight: &Flight) -> PublicFlight {
    PublicFlight {
        id: flight.id.to_string(),
        handle: flight.pilot.handle.to_string(),
        pilot: one_line(&flight.pilot.display_name, 40),
        start_unix_s: flight.start.timestamp(),
        airtime_s: flight.airtime_s.max(0),
        takeoff: flight.takeoff.map(|takeoff| Coordinate {
            lat: takeoff.position.lat(),
            lon: takeoff.position.lon(),
        }),
        track_length_m: flight.track_length_m,
        max_altitude_m: f64::from(flight.max_altitude_m),
        max_climb_ms: flight.max_climb_ms,
        glider: flight
            .glider
            .as_deref()
            .map(|glider| one_line(glider, 40))
            .filter(|glider| !glider.is_empty()),
        score: flight.score.and_then(|score| {
            Some(EntryScore {
                kind: match score.kind {
                    api::RouteKind::FreeFlight => RouteKind::FreeFlight,
                    api::RouteKind::FreeTriangle => RouteKind::FreeTriangle,
                    api::RouteKind::FaiTriangle => RouteKind::FaiTriangle,
                    // A kind of route this version has no name for: the
                    // flight is listed without a score.
                    api::RouteKind::Unknown => return None,
                },
                closed: score.closed,
                distance_m: score.distance_m,
                points: score.points,
            })
        }),
        outline: flight
            .outline
            .iter()
            .map(|&[lat, lon]| Coordinate { lat, lon })
            .collect(),
        page_path: api::share_path(&flight.id),
        file_path: api::flight_file_path(&flight.id),
    }
}

/// The path to `GET` from the server for one flight that is reached by its
/// address alone, as from a link: what a row of the list says about it,
/// which names the pilot and where its file is.
#[uniffi::export]
pub fn public_flight_path(id: String) -> Result<String, CoreError> {
    let id = id.parse().map_err(|_| CoreError::Server {
        message: "This is not the address of a flight.".to_owned(),
    })?;
    Ok(api::flight_path(&id))
}

/// Reads the server's answer to [`public_flight_path`], and also what the
/// server says when it does not hand out the flight's file. A flight that
/// is not there for the asker is said to be no longer public: the server
/// does not tell a private flight from none.
#[uniffi::export]
pub fn public_flight(status: u16, body: Vec<u8>) -> Result<PublicFlight, CoreError> {
    if status == 404 {
        return Err(CoreError::Server {
            message: "This flight is no longer public: its pilot took it back, or deleted it."
                .to_owned(),
        });
    }
    let response: FlightResponse = answer(status, &body)?;
    Ok(row(&response.flight))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn golden(name: &str) -> Vec<u8> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../server/tests/golden/");
        std::fs::read(format!("{path}{name}")).unwrap()
    }

    #[test]
    fn the_list_is_asked_for_in_the_servers_words() {
        assert_eq!(
            public_flights_path(PublicFlightsOrder::Newest, None, None, None, None).unwrap(),
            "/v1/flights/public"
        );
        // The best flights of 15 August 2026 around the Tegernsee, second
        // page.
        let area = Bounds {
            south: 47.55,
            west: 11.6,
            north: 47.85,
            east: 12.0,
        };
        let path = public_flights_path(
            PublicFlightsOrder::Best,
            Some(area),
            Some(1_786_752_000),
            Some(1_786_838_400),
            Some("abc".to_owned()),
        )
        .unwrap();
        assert_eq!(
            path,
            "/v1/flights/public?bbox=11.6,47.55,12,47.85&from=2026-08-15T00:00:00Z&to=2026-08-16T00:00:00Z&sort=score&after=abc"
        );
        // A day that ends before it begins is not asked for.
        assert!(
            public_flights_path(PublicFlightsOrder::Newest, None, Some(10), Some(10), None)
                .is_err()
        );
        // An empty mark is the first page.
        assert_eq!(
            public_flights_path(
                PublicFlightsOrder::Newest,
                None,
                None,
                None,
                Some(String::new())
            )
            .unwrap(),
            "/v1/flights/public"
        );
    }

    #[test]
    fn the_flights_from_a_place_are_asked_for_as_the_square_around_it() {
        // A kilometre and a half around the Wallberg's take-off.
        let path = public_flights_near_path(47.6642, 11.7967, 1500.0).unwrap();
        let bbox = path.strip_prefix("/v1/flights/public?bbox=").unwrap();
        let edges: Vec<f64> = bbox.split(',').map(|edge| edge.parse().unwrap()).collect();
        let (west, south, east, north) = (edges[0], edges[1], edges[2], edges[3]);
        assert!((north - south - 2.0 * 1500.0 / 111_320.0).abs() < 1e-6);
        // Degrees of longitude are shorter there: the square is wider in them.
        assert!((east - west) > (north - south) * 1.4);
        assert!(west < 11.7967 && east > 11.7967 && south < 47.6642 && north > 47.6642);
        // No farther than fifty kilometres, whatever is asked, and what is
        // no place is not asked for.
        let far = public_flights_near_path(47.6642, 11.7967, 1e9).unwrap();
        let wide: Vec<f64> = far
            .strip_prefix("/v1/flights/public?bbox=")
            .unwrap()
            .split(',')
            .map(|edge| edge.parse().unwrap())
            .collect();
        assert!((wide[3] - wide[1] - 2.0 * 50_000.0 / 111_320.0).abs() < 1e-6);
        assert!(public_flights_near_path(f64::NAN, 11.8, 1500.0).is_err());
        assert!(public_flights_near_path(89.9, 11.8, 1500.0).is_err());
    }

    #[test]
    fn the_servers_list_becomes_rows() {
        let page = public_flights(200, golden("public_flights.json")).unwrap();
        assert!(!page.flights.is_empty());
        let raw: Value = serde_json::from_slice(&golden("public_flights.json")).unwrap();
        assert_eq!(page.flights.len(), raw["flights"].as_array().unwrap().len());
        assert_eq!(page.next.as_deref(), raw["next"].as_str());

        let (row, flight) = (&page.flights[0], &raw["flights"][0]);
        assert_eq!(row.id, flight["id"].as_str().unwrap());
        assert_eq!(row.handle, flight["pilot"]["handle"].as_str().unwrap());
        assert_eq!(row.pilot, flight["pilot"]["display_name"].as_str().unwrap());
        assert_eq!(row.airtime_s, flight["airtime_s"].as_i64().unwrap());
        assert_eq!(row.page_path, format!("/f/{}", row.id));
        assert_eq!(row.file_path, format!("/v1/flights/{}/igc", row.id));
        assert_eq!(
            row.outline.len(),
            flight["outline"].as_array().unwrap().len()
        );
        let score = row.score.unwrap();
        assert_eq!(
            score.distance_m,
            flight["score"]["distance_m"].as_f64().unwrap()
        );
        assert_eq!(score.kind, RouteKind::FreeFlight);
        assert!(row.takeoff.is_some());

        // The best first is read the same way.
        let best = public_flights(200, golden("public_flights_best.json")).unwrap();
        let points: Vec<f64> = best
            .flights
            .iter()
            .map(|flight| flight.score.unwrap().points)
            .collect();
        assert!(
            points.windows(2).all(|pair| pair[0] >= pair[1]),
            "{points:?}"
        );

        // A refusal comes in words.
        assert!(public_flights(503, Vec::new()).is_err());
    }

    #[test]
    fn a_pilots_page_is_the_pilot_and_their_public_flights() {
        assert_eq!(
            pilot_page_path("demo".to_owned(), None).unwrap(),
            "/v1/pilots/demo"
        );
        assert_eq!(
            pilot_page_path("demo".to_owned(), Some("abc".to_owned())).unwrap(),
            "/v1/pilots/demo?after=abc"
        );
        // What cannot be a handle is not asked for: it would be a path of
        // the asker's making.
        assert!(pilot_page_path("../accounts".to_owned(), None).is_err());
        assert!(pilot_page_path(String::new(), None).is_err());

        let page = pilot_page(200, golden("pilot.json")).unwrap();
        assert_eq!(
            (page.handle.as_str(), page.name.as_str()),
            ("demo", "Demo Pilot")
        );
        assert!(!page.flights.is_empty());
        assert!(page.flights.iter().all(|flight| flight.handle == "demo"));

        let unknown = pilot_page(404, b"{}".to_vec()).unwrap_err();
        assert!(unknown.to_string().contains("no pilot"), "{unknown}");
        assert!(pilot_page(503, Vec::new()).is_err());
    }

    #[test]
    fn what_another_pilot_wrote_is_one_bounded_line() {
        let mut answer: Value = serde_json::from_slice(&golden("public_flights.json")).unwrap();
        answer["flights"][0]["pilot"]["display_name"] = json!("Anna\nMaier ".repeat(20));
        answer["flights"][0]["glider"] = json!("  \n ");
        answer["flights"][1]["score"]["kind"] = json!("something_new");
        let page = public_flights(200, answer.to_string().into_bytes()).unwrap();
        assert!(page.flights[0].pilot.chars().count() <= 40);
        assert!(!page.flights[0].pilot.contains('\n'));
        // A glider of nothing but space is no glider, and a kind of route
        // this version does not know is no score.
        assert_eq!(page.flights[0].glider, None);
        assert_eq!(page.flights[1].score, None);
    }

    fn words<T: std::fmt::Debug>(result: Result<T, CoreError>) -> String {
        match result {
            Err(CoreError::Server { message }) => message,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_flight_reached_by_its_address_is_read_as_a_row() {
        let raw: Value = serde_json::from_slice(&golden("flight.json")).unwrap();
        let id = raw["flight"]["id"].as_str().unwrap();
        assert_eq!(
            public_flight_path(id.to_owned()).unwrap(),
            format!("/v1/flights/{id}")
        );
        // What is no address is never put into one.
        for text in ["", "public", "../account", "5T1ESHYPDTVRQ7RXW39A18VSRG"] {
            assert_eq!(
                words(public_flight_path(text.to_owned())),
                "This is not the address of a flight."
            );
        }

        let flight = public_flight(200, golden("flight.json")).unwrap();
        assert_eq!(flight.id, id);
        assert_eq!(
            flight.pilot,
            raw["flight"]["pilot"]["display_name"].as_str().unwrap()
        );
        assert_eq!(flight.page_path, format!("/f/{id}"));
        assert_eq!(flight.file_path, format!("/v1/flights/{id}/igc"));
    }

    #[test]
    fn a_flight_that_is_not_there_for_the_asker_is_no_longer_public() {
        let gone = br#"{"error":{"code":"flight_not_found","message":"x"}}"#.to_vec();
        assert_eq!(
            words(public_flight(404, gone)),
            "This flight is no longer public: its pilot took it back, or deleted it."
        );
        // Anything else wrong with the server is said as everywhere.
        assert_eq!(
            words(public_flight(503, Vec::new())),
            "The server is busy or being restarted. Try again in a moment."
        );
    }

    #[test]
    fn another_pilots_flight_is_shown_from_its_file_like_ones_own() {
        let text = venti_flight::igc::write(&venti_flight::testing::synthetic_flight());
        let flight = crate::flight_from_file(text.clone()).unwrap();
        assert_eq!(flight.id, "");
        assert!(flight.track.len() > 1000);
        assert_eq!(flight.pilot.as_deref(), Some("Sample Pilot"));
        let analysis = crate::flight_analysis_from_file(text).unwrap();
        assert_eq!(analysis.thermals.len(), 2);
        // What is no flight file is said so, and not shown.
        assert!(crate::flight_from_file("<html>not found</html>".to_owned()).is_err());
        assert!(crate::flight_analysis_from_file(String::new()).is_err());
    }
}
