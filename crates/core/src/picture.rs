//! A pilot's picture, and the pilots who fly near a place.
//!
//! The picture: [`crate::Core::picture_request`] sends one,
//! [`crate::Core::picture_removal_request`] removes it, and both answers
//! go to [`crate::Core::account_read`], whose [`crate::AccountView`] names
//! the picture's path. A pilot's picture elsewhere is the `picture_path`
//! of the line that shows the pilot. A path is fetched with the server's
//! address in front of it, needs no token, and names one picture for
//! good: a shell may keep it as long as it likes.
//!
//! The pilots near a place: [`pilots_near_path`] says what to ask,
//! [`pilots_near`] reads the answer into lines. The list is made of public
//! flights only; a pilot leaves it with
//! [`crate::Core::shown_near_places_request`]. The words are
//! [`picture_words`].

use venti_api_types::v1::{
    self as api, MAX_PICTURE_BYTES, PICTURE_MEDIA_TYPES, PilotsNearRequest, PilotsNearResponse,
};
use venti_domain::GeoPoint;

use crate::live::one_line;
use crate::pilots::{FlownSiteLine, PilotTotalsView, site_lines, totals};
use crate::server::answer;
use crate::takeoffs::count;
use crate::{CoreError, ServerRequest};

fn refused(message: &str) -> CoreError {
    CoreError::Server {
        message: message.to_owned(),
    }
}

/// The request that sends a picture, checked as the server checks it
/// before it reads one.
pub(crate) fn picture_upload_request(
    picture: Vec<u8>,
    media_type: &str,
) -> Result<ServerRequest, CoreError> {
    let media_type = media_type.trim().to_ascii_lowercase();
    if !PICTURE_MEDIA_TYPES.contains(&media_type.as_str()) {
        return Err(refused("Choose a photo in JPEG, PNG or WebP."));
    }
    if picture.is_empty() {
        return Err(refused("The photo is empty."));
    }
    if picture.len() > MAX_PICTURE_BYTES {
        return Err(refused("The photo is too large. Choose a smaller one."));
    }
    Ok(ServerRequest {
        method: "PUT".to_owned(),
        path: api::ACCOUNT_PICTURE_PATH.to_owned(),
        body: Some(picture),
        content_type: Some(media_type),
        needs_token: true,
    })
}

/// The path and query to `GET` for the pilots who fly near a place:
/// within `radius_km` kilometres (1 to 100; absent, 25).
#[uniffi::export]
pub fn pilots_near_path(lat: f64, lon: f64, radius_km: Option<u32>) -> Result<String, CoreError> {
    let place = GeoPoint::new(lat, lon).map_err(|_| refused("This is no place on Earth."))?;
    if radius_km.is_some_and(|km| !(1..=api::MAX_PILOTS_NEAR_RADIUS_KM).contains(&km)) {
        return Err(refused("Pilots are looked for within 1 to 100 km."));
    }
    Ok(PilotsNearRequest {
        place,
        radius_km,
        limit: None,
    }
    .path_and_query())
}

/// A pilot who flies near a place, as a line of a list.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct NearPilotLine {
    /// The pilot's handle: for [`crate::pilot_page_path`].
    #[uniffi(default = "")]
    pub handle: String,
    /// The name the pilot is shown by, on one line.
    #[uniffi(default = "")]
    pub name: String,
    /// The path of the pilot's picture, to fetch with the server's address
    /// in front. `None` without one: show the initials.
    #[uniffi(default = None)]
    pub picture_path: Option<String>,
    /// The pilot's initials, for where there is no picture: "LP".
    #[uniffi(default = "")]
    pub initials: String,
    /// How many of the pilot's public flights took off near the place.
    #[uniffi(default = 0)]
    pub flights: u32,
    /// The take-offs near the place they flew from most, the most first:
    /// at most three.
    #[uniffi(default = [])]
    pub takeoffs: Vec<FlownSiteLine>,
    /// What their flights of this year add up to, everywhere. Absent for
    /// a pilot who has not flown this year.
    #[uniffi(default = None)]
    pub season: Option<PilotTotalsView>,
    /// The line under the name: "12 flights from here · Musterkogel West".
    #[uniffi(default = "")]
    pub words: String,
}

/// The pilots who fly near a place.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PilotsNear {
    /// The pilots, the one with the most flights from near the place
    /// first.
    #[uniffi(default = [])]
    pub pilots: Vec<NearPilotLine>,
    /// What the list is, for over it: "Within 25 km, the last 12 months".
    #[uniffi(default = "")]
    pub words: String,
    /// The sentence for an empty list.
    #[uniffi(default = "")]
    pub empty: String,
}

/// The initials of a name: the first letter of its first two words.
fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|word| word.chars().find(|c| c.is_alphanumeric()))
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}

/// Reads the server's answer to [`pilots_near_path`].
#[uniffi::export]
pub fn pilots_near(status: u16, body: Vec<u8>) -> Result<PilotsNear, CoreError> {
    let response: PilotsNearResponse = answer(status, &body)?;
    let pilots = response
        .pilots
        .iter()
        .map(|near| {
            let name = one_line(&near.pilot.display_name, 40);
            let takeoffs = site_lines(&near.takeoffs);
            let from_here = format!("{} from here", count(near.flights, "flight", "flights"));
            let words = match takeoffs.first() {
                Some(site) => format!("{from_here} · {}", site.name),
                None => from_here,
            };
            NearPilotLine {
                handle: near.pilot.handle.to_string(),
                initials: initials(&name),
                picture_path: near.pilot.picture.clone(),
                flights: near.flights,
                takeoffs,
                season: near.season.as_ref().map(|season| {
                    totals(
                        season.year.to_string(),
                        season.year,
                        api::RANKING_COUNTED as u32,
                        &season.totals,
                    )
                }),
                words,
                name,
            }
        })
        .collect();
    Ok(PilotsNear {
        pilots,
        words: format!(
            "Within {} km, the last {} months",
            response.radius_km, response.months
        ),
        empty: "No pilot has shared a flight from near here yet.".to_owned(),
    })
}

/// The words of a pilot's picture and of the pilots near a place, the
/// same in the three apps.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PictureWords {
    /// The button that chooses a picture: "Change picture".
    #[uniffi(default = "")]
    pub change: String,
    /// The button that removes it: "Remove picture".
    #[uniffi(default = "")]
    pub remove: String,
    /// The one line beside the picture: that it is public.
    #[uniffi(default = "")]
    pub public_line: String,
    /// The heading of the list of pilots near a place: "Pilots near".
    #[uniffi(default = "")]
    pub near_title: String,
    /// Where the pilots near were looked for when it was the pilot's own
    /// position, after [`PilotsNear::words`] and a " · ": "around you",
    /// as in "Within 25 km, the last 12 months · around you".
    #[uniffi(default = "")]
    pub near_around_you: String,
    /// The same when it was the middle of the map: "around the map's
    /// middle".
    #[uniffi(default = "")]
    pub near_around_map: String,
    /// The switch of the account's setting: "Show me among pilots near a
    /// place".
    #[uniffi(default = "")]
    pub shown_near_places: String,
    /// The one line under it.
    #[uniffi(default = "")]
    pub shown_near_places_line: String,
    /// What a report about a picture is called in a pilot's menu:
    /// "Report picture".
    #[uniffi(default = "")]
    pub report: String,
}

/// The words of a pilot's picture and of the pilots near a place.
#[uniffi::export]
pub fn picture_words() -> PictureWords {
    PictureWords {
        change: "Change picture".to_owned(),
        remove: "Remove picture".to_owned(),
        public_line: "Everyone sees it, on your page and beside your flights.".to_owned(),
        near_title: "Pilots near".to_owned(),
        near_around_you: "around you".to_owned(),
        near_around_map: "around the map's middle".to_owned(),
        shown_near_places: "Show me among pilots near a place".to_owned(),
        shown_near_places_line: "From your public flights only.".to_owned(),
        report: "Report picture".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn pilots_near_a_place_are_asked_and_read_into_lines() {
        assert_eq!(
            pilots_near_path(47.66, 11.8, None).unwrap(),
            "/v1/pilots/near?lat=47.66&lon=11.8"
        );
        assert_eq!(
            pilots_near_path(47.66, 11.8, Some(40)).unwrap(),
            "/v1/pilots/near?lat=47.66&lon=11.8&radius_km=40"
        );
        assert!(pilots_near_path(47.66, 11.8, Some(0)).is_err());
        assert!(pilots_near_path(47.66, 11.8, Some(101)).is_err());
        assert!(pilots_near_path(97.0, 11.8, None).is_err());

        let body = json!({
            "pilots": [{
                "pilot": {"handle": "lena", "display_name": "Lena  Probe",
                          "picture": "/v1/pictures/aaaaaaaaaaaaaaaaaaaaaaaaaa"},
                "flights": 2,
                "takeoffs": [{"site": {"id": "venti:0000000000000000000000s001",
                                       "name": "Musterkogel West"},
                              "position": {"lat": 47.3, "lon": 10.9}, "flights": 2}]
            }, {
                "pilot": {"handle": "jonas", "display_name": "jonas"},
                "flights": 1,
                "takeoffs": []
            }],
            "radius_km": 25,
            "months": 12
        });
        let near = pilots_near(200, serde_json::to_vec(&body).unwrap()).unwrap();
        assert_eq!(near.words, "Within 25 km, the last 12 months");
        let lena = &near.pilots[0];
        assert_eq!(lena.name, "Lena Probe");
        assert_eq!(lena.initials, "LP");
        assert_eq!(
            lena.picture_path.as_deref(),
            Some("/v1/pictures/aaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_eq!(lena.words, "2 flights from here · Musterkogel West");
        assert_eq!(lena.takeoffs[0].words, "Musterkogel West, 2 flights");
        assert!(lena.season.is_none());
        let jonas = &near.pilots[1];
        assert_eq!(
            (jonas.initials.as_str(), jonas.words.as_str()),
            ("J", "1 flight from here")
        );
        assert!(jonas.picture_path.is_none());
    }

    #[test]
    fn a_picture_is_sent_as_it_is_and_refused_in_words_where_the_server_would() {
        let request = picture_upload_request(vec![0xFF, 0xD8, 0xFF], " Image/JPEG ").unwrap();
        assert_eq!(
            (request.method.as_str(), request.path.as_str()),
            ("PUT", "/v1/account/picture")
        );
        assert_eq!(request.content_type.as_deref(), Some("image/jpeg"));
        assert!(request.needs_token);
        assert!(picture_upload_request(vec![1], "image/gif").is_err());
        assert!(picture_upload_request(Vec::new(), "image/png").is_err());
        assert!(picture_upload_request(vec![0; MAX_PICTURE_BYTES + 1], "image/png").is_err());
        assert_eq!(
            picture_words().shown_near_places,
            "Show me among pilots near a place"
        );
        let words = picture_words();
        assert_eq!(
            (
                words.near_around_you.as_str(),
                words.near_around_map.as_str()
            ),
            ("around you", "around the map's middle")
        );
    }
}
