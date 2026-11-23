//! The pilot's own flights as the server holds them, for a shell that
//! keeps no logbook of its own: the website (owner decision 20), and a
//! phone that shows the account's flights from the server.
//!
//! As everywhere, the core does not talk to the server. It says what to
//! ask for ([`own_flights_request`], [`upload_request`],
//! [`flight_visibility_request`], [`flight_glider_class_request`],
//! [`delete_flight_request`], [`own_statistics_request`]), each with the
//! session's token, and reads the answers into the rows everyone's
//! flights have, with who may see each one ([`OwnFlight`]). Nothing is
//! kept. A flight is opened from its file, which the shell fetches from
//! [`PublicFlight::file_path`] with the token and hands to
//! [`crate::flight_from_file`]; the airspace check, the hand-over to a
//! league and the file's name work from the same text
//! ([`crate::Core::flight_airspace_from_file`],
//! [`crate::league_handover_from_file`], [`crate::flight_file_from_text`]).
//!
//! The phones keep their logbook on the device and sync it
//! ([`crate::FlightSync`]); these calls are for what has no place there.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use venti_api_types::v1::{
    self as api, FlightId, FlightResponse, FlightsRequest, FlightsResponse, UpdateFlightRequest,
    UploadResponse,
};

use crate::account::{json_request, sharing_of, wire_visibility};
use crate::community::row;
use crate::server::{answer, refusal};
use crate::{
    CoreError, FlightSharing, FlightVisibility, GliderClassKind, PublicFlight, ServerRequest,
};

/// How many of the pilot's flights are asked for at a time: the most the
/// server gives.
const PAGE: usize = api::MAX_PAGE_SIZE;

/// One of the pilot's own flights, as their list shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct OwnFlight {
    /// The flight as a row of everyone's flights shows it.
    pub row: PublicFlight,
    /// Who may see it, with its link.
    pub sharing: FlightSharing,
    /// Whether whoever runs the server took it down: it is private then,
    /// and the choice of who sees it is not offered.
    pub taken_down: bool,
}

/// One page of the pilot's own flights, the newest first.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct OwnFlights {
    /// The flights.
    pub flights: Vec<OwnFlight>,
    /// What to pass as `after` for the next page. Absent on the last.
    pub next: Option<String>,
}

/// What became of a flight file sent to the account.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct UploadedFlight {
    /// The flight as the server now holds it.
    pub flight: OwnFlight,
    /// Whether the account had it already; nothing changed then.
    pub already_there: bool,
}

/// The request for one page of the pilot's own flights, the private ones
/// too, the newest first. `after` is the `next` of the page before. The
/// answer is read by [`own_flights`].
#[uniffi::export]
pub fn own_flights_request(after: Option<String>) -> ServerRequest {
    ServerRequest {
        method: "GET".to_owned(),
        path: FlightsRequest {
            after: after.filter(|after| !after.is_empty()),
            limit: Some(PAGE),
        }
        .path_and_query(),
        body: None,
        content_type: None,
        needs_token: true,
    }
}

/// Reads the server's answer to [`own_flights_request`]. `server` is the
/// address the request went to, for the flights' links.
#[uniffi::export]
pub fn own_flights(server: String, status: u16, body: Vec<u8>) -> Result<OwnFlights, CoreError> {
    let response: FlightsResponse = answer(status, &body)?;
    Ok(OwnFlights {
        flights: response
            .flights
            .iter()
            .map(|flight| own(&server, flight))
            .collect::<Result<_, _>>()?,
        next: response.next,
    })
}

/// The request for one of the pilot's flights, private or not, by its
/// address. The answer is read by [`own_flight`].
#[uniffi::export]
pub fn own_flight_request(id: String) -> Result<ServerRequest, CoreError> {
    Ok(ServerRequest {
        method: "GET".to_owned(),
        path: api::flight_path(&address(&id)?),
        body: None,
        content_type: None,
        needs_token: true,
    })
}

/// Reads the server's answer to [`own_flight_request`], to
/// [`flight_visibility_request`] and to [`flight_glider_class_request`]: the
/// flight as it now is.
#[uniffi::export]
pub fn own_flight(server: String, status: u16, body: Vec<u8>) -> Result<OwnFlight, CoreError> {
    let response: FlightResponse = answer(status, &body)?;
    own(&server, &response.flight)
}

/// The request that sends a flight file to the account. The server keeps
/// it byte for byte; it is public from the start unless the pilot keeps
/// new flights private, and marked with the class of the glider the pilot
/// set. The answer is read by [`uploaded_flight`].
#[uniffi::export]
pub fn upload_request(file: Vec<u8>) -> ServerRequest {
    ServerRequest {
        method: "POST".to_owned(),
        path: api::FLIGHTS_PATH.to_owned(),
        body: Some(file),
        content_type: Some("application/octet-stream".to_owned()),
        needs_token: true,
    }
}

/// Reads the server's answer to [`upload_request`]. A file the server
/// cannot use, and an account that is full, are refused in words.
#[uniffi::export]
pub fn uploaded_flight(
    server: String,
    status: u16,
    body: Vec<u8>,
) -> Result<UploadedFlight, CoreError> {
    let response: UploadResponse = answer(status, &body)?;
    Ok(UploadedFlight {
        flight: own(&server, &response.flight)?,
        already_there: !response.created,
    })
}

/// The request that changes who may see one of the pilot's flights. The
/// answer is read by [`own_flight`]; a flight that was taken down, and a
/// public one from an account whose publishing is suspended, are refused
/// in words.
#[uniffi::export]
pub fn flight_visibility_request(
    id: String,
    visibility: FlightVisibility,
) -> Result<ServerRequest, CoreError> {
    json_request(
        "PATCH",
        &api::flight_path(&address(&id)?),
        &UpdateFlightRequest {
            visibility: Some(wire_visibility(visibility)),
            ..UpdateFlightRequest::default()
        },
        true,
    )
}

/// The request that sets the class of the glider one of the pilot's
/// flights was flown on, or says with `None` that it is not known. The
/// answer is read by [`own_flight`].
#[uniffi::export]
pub fn flight_glider_class_request(
    id: String,
    class: Option<GliderClassKind>,
) -> Result<ServerRequest, CoreError> {
    json_request(
        "PATCH",
        &api::flight_path(&address(&id)?),
        &UpdateFlightRequest {
            glider_class: Some(class.map(wire_class)),
            ..UpdateFlightRequest::default()
        },
        true,
    )
}

/// The request that deletes one of the pilot's flights on the server, at
/// once and for good. The answer is read by [`flight_deleted`].
#[uniffi::export]
pub fn delete_flight_request(id: String) -> Result<ServerRequest, CoreError> {
    Ok(ServerRequest {
        method: "DELETE".to_owned(),
        path: api::flight_path(&address(&id)?),
        body: None,
        content_type: None,
        needs_token: true,
    })
}

/// Reads the server's answer to [`delete_flight_request`]: nothing when
/// the flight is gone, the refusal in words otherwise.
#[uniffi::export]
pub fn flight_deleted(status: u16, body: Vec<u8>) -> Result<(), CoreError> {
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(refusal(status, &body))
    }
}

/// The request for what all the pilot's flights add up to, the private
/// ones too, all together and year by year. The answer has the shape of a
/// pilot's public statistics and is read by [`crate::pilot_statistics`].
#[uniffi::export]
pub fn own_statistics_request() -> ServerRequest {
    ServerRequest {
        method: "GET".to_owned(),
        path: api::ACCOUNT_STATISTICS_PATH.to_owned(),
        body: None,
        content_type: None,
        needs_token: true,
    }
}

/// The address of a flight on the server, from what a shell holds.
fn address(id: &str) -> Result<FlightId, CoreError> {
    id.parse().map_err(|_| CoreError::Server {
        message: "This is not a flight the app knows.".to_owned(),
    })
}

/// One of the pilot's flights from the server's record of it.
fn own(server: &str, flight: &api::Flight) -> Result<OwnFlight, CoreError> {
    let sharing = sharing_of(server, flight.id.as_str(), flight.visibility).ok_or_else(|| {
        CoreError::Server {
            message: "The server's answer could not be read. The app may be older than the server."
                .to_owned(),
        }
    })?;
    Ok(OwnFlight {
        row: row(flight),
        sharing,
        taken_down: flight.taken_down_at.is_some(),
    })
}

/// A glider's class as the API names it.
pub(crate) fn wire_class(class: GliderClassKind) -> api::GliderClass {
    match class {
        GliderClassKind::A => api::GliderClass::EnA,
        GliderClassKind::B => api::GliderClass::EnB,
        GliderClassKind::C => api::GliderClass::EnC,
        GliderClassKind::D => api::GliderClass::EnD,
        GliderClassKind::Ccc => api::GliderClass::Ccc,
    }
}

/// A glider's class from the API's name. `None` for one this version does
/// not know.
pub(crate) fn glider_class_kind(class: api::GliderClass) -> Option<GliderClassKind> {
    match class {
        api::GliderClass::EnA => Some(GliderClassKind::A),
        api::GliderClass::EnB => Some(GliderClassKind::B),
        api::GliderClass::EnC => Some(GliderClassKind::C),
        api::GliderClass::EnD => Some(GliderClassKind::D),
        api::GliderClass::Ccc => Some(GliderClassKind::Ccc),
        api::GliderClass::Unknown => None,
    }
}

/// A glider's class in the stored account, by the API's name: the core's
/// own kind has no form on disk.
pub(crate) mod stored_class {
    use super::*;

    #[allow(clippy::ref_option, reason = "the signature serde's `with` asks for")]
    pub(crate) fn serialize<S: Serializer>(
        class: &Option<GliderClassKind>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        class.map(wire_class).serialize(serializer)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<GliderClassKind>, D::Error> {
        Ok(Option::<api::GliderClass>::deserialize(deserializer)?.and_then(glider_class_kind))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const ID: &str = "7cf5ytrw45b0aeb5tcr8wet2km";

    fn flight(visibility: &str) -> serde_json::Value {
        json!({
            "id": ID,
            "identity": "00112233445566778899aabbccddeeff",
            "pilot": {"handle": "dario", "display_name": "Dario", "country": "DE"},
            "visibility": visibility,
            "uploaded_at": "2026-07-04T15:00:00Z",
            "start": "2026-07-04T11:00:00Z",
            "airtime_s": 3600,
            "takeoff": {
                "time": "2026-07-04T11:00:00Z",
                "position": {"lat": 47.4644, "lon": 12.1675},
                "altitude_m": 1800
            },
            "takeoff_site": {"id": "venti:01jzx3h2k4m5n6p7q8r9s0t1v2", "name": "Hohe Salve"},
            "takeoff_country": "AT",
            "track_length_m": 25000.0,
            "max_altitude_m": 2400,
            "max_climb_ms": 3.2,
            "glider": "Rush 6",
            "glider_class": "en_b",
            "outline": [[47.46, 12.16], [47.47, 12.17]],
            "file_bytes": 120000,
            "analysis_version": 1
        })
    }

    fn bytes(value: &serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(value).unwrap()
    }

    #[test]
    fn the_pilots_flights_are_read_with_who_sees_them_and_what_the_logbook_shows() {
        let request = own_flights_request(Some("next-page".to_owned()));
        assert_eq!(
            (request.method.as_str(), request.needs_token),
            ("GET", true)
        );
        assert_eq!(request.path, "/v1/flights?after=next-page&limit=100");
        assert_eq!(
            own_flights_request(Some(String::new())).path,
            "/v1/flights?limit=100"
        );

        let mut taken_down = flight("private");
        taken_down["taken_down_at"] = json!("2026-07-05T08:00:00Z");
        let page = json!({"flights": [flight("unlisted"), taken_down], "next": "n"});
        let read = own_flights("https://venti.example/".to_owned(), 200, bytes(&page)).unwrap();
        assert_eq!(read.next.as_deref(), Some("n"));
        let first = &read.flights[0];
        assert_eq!(first.sharing.visibility, FlightVisibility::Unlisted);
        assert_eq!(first.sharing.link, format!("https://venti.example/f/{ID}"));
        assert_eq!(first.sharing.flight_id, ID);
        assert!(!first.taken_down);
        assert!(read.flights[1].taken_down);
        // The flags and the class, beside the row everyone's flights have.
        assert_eq!(first.row.takeoff_country.as_deref(), Some("AT"));
        assert_eq!(first.row.pilot_country.as_deref(), Some("DE"));
        assert_eq!(first.row.glider_class, Some(GliderClassKind::B));
        assert_eq!(first.row.takeoff_name.as_deref(), Some("Hohe Salve"));

        // A setting this version does not know is not guessed at.
        let unknown = json!({"flights": [flight("followers")]});
        assert!(own_flights(String::new(), 200, bytes(&unknown)).is_err());
        // A refusal is said in words.
        let refused = own_flights(String::new(), 401, b"{}".to_vec()).unwrap_err();
        assert!(matches!(refused, CoreError::Server { .. }));
    }

    #[test]
    fn a_flight_is_uploaded_shared_marked_and_deleted_by_its_address() {
        let upload = upload_request(b"AXVE001".to_vec());
        assert_eq!(
            (upload.method.as_str(), upload.path.as_str()),
            ("POST", "/v1/flights")
        );
        assert_eq!(
            upload.content_type.as_deref(),
            Some("application/octet-stream")
        );
        assert!(upload.needs_token);
        let answer = json!({"flight": flight("public"), "created": false});
        let uploaded =
            uploaded_flight("https://v.example".to_owned(), 200, bytes(&answer)).unwrap();
        assert!(uploaded.already_there);
        assert_eq!(uploaded.flight.sharing.visibility, FlightVisibility::Public);

        let share = flight_visibility_request(ID.to_owned(), FlightVisibility::Private).unwrap();
        assert_eq!(share.method, "PATCH");
        assert_eq!(share.path, format!("/v1/flights/{ID}"));
        assert_eq!(share.body.unwrap(), br#"{"visibility":"private"}"#);
        let class = flight_glider_class_request(ID.to_owned(), Some(GliderClassKind::Ccc)).unwrap();
        assert_eq!(class.body.unwrap(), br#"{"glider_class":"ccc"}"#);
        let unknown = flight_glider_class_request(ID.to_owned(), None).unwrap();
        assert_eq!(unknown.body.unwrap(), br#"{"glider_class":null}"#);
        let read = own_flight(
            "https://v.example".to_owned(),
            200,
            bytes(&json!({"flight": flight("private")})),
        )
        .unwrap();
        assert_eq!(read.sharing.visibility, FlightVisibility::Private);
        assert_eq!(own_flight_request(ID.to_owned()).unwrap().path, share.path);

        let delete = delete_flight_request(ID.to_owned()).unwrap();
        assert_eq!((delete.method.as_str(), delete.body), ("DELETE", None));
        assert!(flight_deleted(204, Vec::new()).is_ok());
        assert!(flight_deleted(404, br#"{"error":{"code":"flight_not_found"}}"#.to_vec()).is_err());
        // What is no flight's address is not asked for.
        assert!(delete_flight_request("../account".to_owned()).is_err());

        let statistics = own_statistics_request();
        assert_eq!(
            (statistics.path.as_str(), statistics.needs_token),
            ("/v1/account/statistics", true)
        );
    }
}
