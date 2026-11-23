//! A pilot's picture, and the pilots who fly near a place.
//!
//! A pilot has one picture, the server's own copy: a square
//! [`PICTURE_SIDE`] pixels on each side, as WebP, which the server made
//! from what the pilot sent or from the picture of the Google account they
//! sign in with. It is named by its path ([`picture_path`]) wherever the
//! pilot is: on the account, on a pilot's page and totals, on every flight,
//! in a search and among the pilots near a place. The path names one
//! picture for good: a new picture has a new path, so a client may keep
//! one as long as it likes.

use serde::{Deserialize, Serialize};
use venti_domain::GeoPoint;

use super::{FlownSite, PilotSeason, request::with_query};

/// Path of the account's picture: `PUT` sends one, `DELETE` removes it.
pub const ACCOUNT_PICTURE_PATH: &str = "/v1/account/picture";

/// Path of a picture, by its identifier: what the `picture` of an account
/// and of a pilot names.
pub fn picture_path(id: &str) -> String {
    format!("/v1/pictures/{id}")
}

/// The width and the height of every picture the server hands out, in
/// pixels.
pub const PICTURE_SIDE: u32 = 256;

/// The most bytes a picture sent to the server may have: 8 MB. A client
/// that scales a photograph down to about 1,000 pixels before it sends
/// it, as it should, sends a few hundred kilobytes.
pub const MAX_PICTURE_BYTES: usize = 8 * 1024 * 1024;

/// The most pixels a picture sent to the server may have, width times
/// height: 40 million, a photograph of the newest phones' main camera.
pub const MAX_PICTURE_PIXELS: u64 = 40_000_000;

/// The longest side a picture sent to the server may have, in pixels.
pub const MAX_PICTURE_SIDE: u32 = 10_000;

/// The shortest side a picture sent to the server must have, in pixels.
pub const MIN_PICTURE_SIDE: u32 = 32;

/// How many pictures one account may send in a day, whatever becomes of
/// them.
pub const PICTURES_PER_DAY: u32 = 10;

/// The kinds of file a picture may be sent as, by their media type.
pub const PICTURE_MEDIA_TYPES: [&str; 3] = ["image/jpeg", "image/png", "image/webp"];

/// The media type of every picture the server hands out.
pub const PICTURE_MEDIA_TYPE: &str = "image/webp";

/// Path of the pilots who fly near a place.
pub const PILOTS_NEAR_PATH: &str = "/v1/pilots/near";

/// How far around the place pilots are looked for when a request does not
/// say, in kilometres.
pub const DEFAULT_PILOTS_NEAR_RADIUS_KM: u32 = 25;

/// The farthest around a place pilots are looked for, in kilometres.
pub const MAX_PILOTS_NEAR_RADIUS_KM: u32 = 100;

/// How many months back a flight counts for the pilots near a place.
pub const PILOTS_NEAR_MONTHS: u32 = 12;

/// How many pilots near a place one answer has when the request does not
/// say.
pub const DEFAULT_PILOTS_NEAR: usize = 20;

/// The most pilots near a place one answer has.
pub const MAX_PILOTS_NEAR: usize = 50;

/// How many take-off sites each pilot near a place names at most.
pub const PILOTS_NEAR_TOP_SITES: usize = 3;

/// A request for the pilots who fly near a place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PilotsNearRequest {
    /// The place.
    pub place: GeoPoint,
    /// How far around it, in kilometres: 1 to
    /// [`MAX_PILOTS_NEAR_RADIUS_KM`]. `None` asks for
    /// [`DEFAULT_PILOTS_NEAR_RADIUS_KM`].
    pub radius_km: Option<u32>,
    /// How many pilots, 1 to [`MAX_PILOTS_NEAR`]. `None` asks for
    /// [`DEFAULT_PILOTS_NEAR`].
    pub limit: Option<usize>,
}

impl PilotsNearRequest {
    /// The path and query string to GET.
    pub fn path_and_query(&self) -> String {
        with_query(
            PILOTS_NEAR_PATH.to_owned(),
            [
                ("lat", Some(self.place.lat().to_string())),
                ("lon", Some(self.place.lon().to_string())),
                ("radius_km", self.radius_km.map(|km| km.to_string())),
                ("limit", self.limit.map(|limit| limit.to_string())),
            ],
        )
    }
}

/// The answer of `GET /v1/pilots/near`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PilotsNearResponse {
    /// The pilots, the one with the most public flights from near the
    /// place first, then by handle.
    pub pilots: Vec<NearPilot>,
    /// How far around the place they were looked for, in kilometres.
    pub radius_km: u32,
    /// How many months back their flights count:
    /// [`PILOTS_NEAR_MONTHS`].
    pub months: u32,
}

/// A pilot who flies near a place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NearPilot {
    /// The pilot, with their picture.
    pub pilot: super::Pilot,
    /// How many of their public flights took off within the radius in the
    /// months counted.
    pub flights: u32,
    /// The take-off sites within the radius they flew from most in those
    /// months, the most first: at most [`PILOTS_NEAR_TOP_SITES`]. A flight
    /// that took off where no site with a name stands is in none.
    pub takeoffs: Vec<FlownSite>,
    /// What their public flights of this year add up to, everywhere: as
    /// a season of their totals (`GET /v1/pilots/{handle}/statistics`).
    /// Left out for a pilot who has not flown this year.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<PilotSeason>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_for_pilots_near_a_place_says_only_what_it_asks() {
        let place = GeoPoint::new(47.66, 11.8).unwrap();
        let request = PilotsNearRequest {
            place,
            radius_km: None,
            limit: None,
        };
        assert_eq!(
            request.path_and_query(),
            "/v1/pilots/near?lat=47.66&lon=11.8"
        );
        let request = PilotsNearRequest {
            place,
            radius_km: Some(40),
            limit: Some(10),
        };
        assert_eq!(
            request.path_and_query(),
            "/v1/pilots/near?lat=47.66&lon=11.8&radius_km=40&limit=10"
        );
        assert_eq!(picture_path("abc"), "/v1/pictures/abc");
    }
}
