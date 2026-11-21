//! What only a server that sees many gliders can say: where pilots climb
//! at this moment, how many are in the air at a flying site, and where the
//! public flights' thermals have been.
//!
//! All three are **counts and averages of the air, never a person**. No
//! answer here names a pilot, a device, a flight or a track, and every
//! place is the middle of a cell of a fixed grid: it marks air, not where
//! somebody is. `docs/architecture.md` has the rules in its decision log.
//!
//! - **Climbs now** ([`LiveClimbsResponse`]) and **pilots in the air**
//!   ([`LiveSitesResponse`]) are read from the live positions of the last
//!   minutes, in layers by where the positions come from: one for each
//!   network the server listens to, under the network's credit, and one
//!   for Venti's own live pilots who show their flight to everyone
//!   ([`VENTI_LIVE_SOURCE`]). The two are never one record.
//! - **Where thermals have been** ([`FlownThermalsResponse`]) is Venti's
//!   own, from the public flights the server holds, under their terms.

use serde::{Deserialize, Serialize};
use venti_domain::{BoundingBox, GeoPoint, SiteId, Timestamp};

use super::{Attribution, FlightTerms, format_bbox};

/// Path of the climbs of the last minutes in a box.
pub const LIVE_CLIMBS_PATH: &str = "/v1/live/climbs";

/// Path of the flying sites in a box with pilots in the air.
pub const LIVE_SITES_PATH: &str = "/v1/live/sites";

/// Path of where the public flights' thermals have been.
pub const FLOWN_THERMALS_PATH: &str = "/v1/thermals/flown";

/// What a layer made from Venti's own live pilots names as its source. It
/// is no data source and has no attribution: the pilots show their flight
/// to everyone, and what is derived names none of them.
pub const VENTI_LIVE_SOURCE: &str = "venti";

/// For how many minutes after a pilot left it a climb is listed.
pub const LIVE_CLIMB_MINUTES: i64 = 10;

/// The grid a climb's place is the middle of a cell of, in degrees of
/// latitude and of longitude: about 560 m by 370 m at 47° north, wider
/// than a thermal is worked.
pub const LIVE_CLIMB_CELL_DEG: f64 = 0.005;

/// The step a climb's heights are rounded to, metres.
pub const LIVE_CLIMB_HEIGHT_STEP_M: i32 = 50;

/// The most climbs one layer carries: the newest.
pub const MAX_LIVE_CLIMBS: usize = 500;

/// How far from a flying site a pilot in the air is counted to it, metres.
pub const SITE_AIR_WITHIN_M: f64 = 5000.0;

/// How near a flying site a glider must have been first heard, metres, to
/// have taken off there.
pub const SITE_TAKEOFF_WITHIN_M: f64 = 500.0;

/// For how many minutes a take-off is counted.
pub const SITE_TAKEOFF_MINUTES: i64 = 60;

/// A glider not heard for this many minutes is no longer in the air.
pub const SITE_AIR_HEARD_MINUTES: i64 = 5;

/// The most sites one layer of [`LiveSitesResponse`] carries: those with
/// the most pilots in the air.
pub const MAX_LIVE_SITES: usize = 500;

/// The grid of where thermals have been, in degrees of latitude and of
/// longitude: about 560 m by 370 m at 47° north.
pub const FLOWN_CELL_DEG: f64 = 0.005;

/// The fewest pilots whose public flights climbed in a cell for the cell
/// to be handed out: one pilot's climbs say where that pilot flies.
pub const FLOWN_MIN_PILOTS: u32 = 3;

/// The widest box `GET /v1/thermals/flown` answers for, in degrees.
pub const MAX_FLOWN_BOX_DEG: f64 = 1.0;

/// The most cells one answer carries: those with the most pilots.
pub const MAX_FLOWN_CELLS: usize = 2000;

/// A place where gliders circled and climbed in the last minutes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveClimb {
    /// The middle of the cell of [`LIVE_CLIMB_CELL_DEG`] the climb lies
    /// in: the air, not a glider.
    pub position: GeoPoint,
    /// The climb averaged over the circles, of every pilot who worked it,
    /// metres per second, to a tenth.
    pub climb_ms: f64,
    /// The lowest height a pilot began to circle at, metres above sea
    /// level, rounded to [`LIVE_CLIMB_HEIGHT_STEP_M`].
    pub from_m: i32,
    /// The greatest height a pilot circled to, rounded the same way.
    pub to_m: i32,
    /// How many pilots circled in it in the last [`LIVE_CLIMB_MINUTES`].
    pub pilots: u32,
    /// How long ago the last of them was still circling, seconds; 0 while
    /// one is.
    pub age_s: u32,
}

/// The climbs one source of live positions shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveClimbsLayer {
    /// A network's source, whose credit is the attribution of the same
    /// source, or [`VENTI_LIVE_SOURCE`].
    pub source: String,
    /// The climbs, the newest first.
    pub climbs: Vec<LiveClimb>,
    /// Whether the box holds more than [`MAX_LIVE_CLIMBS`].
    pub truncated: bool,
}

/// The answer of `GET /v1/live/climbs`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveClimbsResponse {
    /// The server's clock when it wrote this answer.
    pub time: Timestamp,
    /// For how many minutes a climb is listed.
    pub listed_minutes: i64,
    /// The grid, degrees.
    pub cell_deg: f64,
    /// One layer per source of live positions, the networks first by
    /// source, then Venti's own.
    pub layers: Vec<LiveClimbsLayer>,
    /// The credit of every network with a layer.
    pub attributions: Vec<Attribution>,
}

/// A flying site and the pilots in the air around it. Numbers only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteAir {
    /// The site, as `GET /v1/sites` names it.
    pub site: SiteId,
    /// Paragliders and hang gliders in the air within
    /// [`SITE_AIR_WITHIN_M`] of it, heard in the last
    /// [`SITE_AIR_HEARD_MINUTES`].
    pub airborne: u32,
    /// Those that took off there in the last [`SITE_TAKEOFF_MINUTES`]:
    /// first heard within [`SITE_TAKEOFF_WITHIN_M`] of it, and flying
    /// since.
    pub took_off: u32,
}

/// The sites one source of live positions has pilots in the air at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveSitesLayer {
    /// A network's source or [`VENTI_LIVE_SOURCE`].
    pub source: String,
    /// The sites with at least one pilot in the air or one take-off, the
    /// most pilots first. A site that is not listed has none.
    pub sites: Vec<SiteAir>,
    /// Whether the box holds more than [`MAX_LIVE_SITES`].
    pub truncated: bool,
}

/// The answer of `GET /v1/live/sites`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveSitesResponse {
    /// The server's clock when it wrote this answer.
    pub time: Timestamp,
    /// How far from a site a pilot is counted to it, metres.
    pub within_m: f64,
    /// For how many minutes a take-off is counted.
    pub took_off_minutes: i64,
    /// One layer per source of live positions, the networks first by
    /// source, then Venti's own. A pilot who is heard by a network and
    /// sends Venti a live track is in both: the layers are not to be
    /// added up.
    pub layers: Vec<LiveSitesLayer>,
    /// The credit of every network with a layer.
    pub attributions: Vec<Attribution>,
}

/// A cell of the grid in which the public flights of several pilots
/// climbed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlownThermalCell {
    /// The middle of the cell.
    pub position: GeoPoint,
    /// How many climbs began in it.
    pub climbs: u32,
    /// Of how many pilots; at least [`FLOWN_MIN_PILOTS`].
    pub pilots: u32,
    /// The climbs' average, metres per second, to a tenth.
    pub climb_ms: f64,
    /// The best of them, to a tenth.
    pub best_climb_ms: f64,
    /// The height a climb began at, on average, metres above sea level,
    /// rounded to 50 m.
    pub from_m: i32,
    /// The height a climb ended at, on average, rounded the same way.
    pub to_m: i32,
}

/// The answer of `GET /v1/thermals/flown`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlownThermalsResponse {
    /// The cells, the most pilots first.
    pub cells: Vec<FlownThermalCell>,
    /// Whether the box holds more than [`MAX_FLOWN_CELLS`].
    pub more: bool,
    /// The month the answer is of, 1 to 12; left out for the whole year.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub month: Option<u8>,
    /// The hours of the day the answer is of, by the sun at the place
    /// (UTC and four minutes for each degree east): from the first to
    /// before the second. Left out for the whole day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hours: Option<[u8; 2]>,
    /// The grid, degrees.
    pub cell_deg: f64,
    /// The fewest pilots a cell has.
    pub min_pilots: u32,
    /// How many public flights the server has read for this.
    pub flights: u32,
    /// What the public flights this is made from are given under.
    pub terms: FlightTerms,
}

/// A request for the climbs of the last minutes in a box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveClimbsRequest {
    /// The box, at most
    /// [`MAX_LIVE_OTHERS_BOX_DEG`](super::MAX_LIVE_OTHERS_BOX_DEG) degrees
    /// each way.
    pub bbox: BoundingBox,
}

impl LiveClimbsRequest {
    /// The path and query string to GET.
    #[must_use]
    pub fn path_and_query(&self) -> String {
        format!("{LIVE_CLIMBS_PATH}?bbox={}", format_bbox(&self.bbox))
    }
}

/// A request for the sites in a box with pilots in the air.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveSitesRequest {
    /// The box, at most
    /// [`MAX_LIVE_OTHERS_BOX_DEG`](super::MAX_LIVE_OTHERS_BOX_DEG) degrees
    /// each way.
    pub bbox: BoundingBox,
}

impl LiveSitesRequest {
    /// The path and query string to GET.
    #[must_use]
    pub fn path_and_query(&self) -> String {
        format!("{LIVE_SITES_PATH}?bbox={}", format_bbox(&self.bbox))
    }
}

/// A request for where thermals have been in a box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlownThermalsRequest {
    /// The box, at most [`MAX_FLOWN_BOX_DEG`] degrees each way.
    pub bbox: BoundingBox,
    /// Only the climbs of this month of any year, 1 to 12.
    pub month: Option<u8>,
    /// Only the climbs that began between these hours by the sun: from
    /// the first, 0 to 23, to before the second, at most 24.
    pub hours: Option<[u8; 2]>,
}

impl FlownThermalsRequest {
    /// The path and query string to GET.
    #[must_use]
    pub fn path_and_query(&self) -> String {
        let mut path = format!("{FLOWN_THERMALS_PATH}?bbox={}", format_bbox(&self.bbox));
        if let Some(month) = self.month {
            path.push_str(&format!("&month={month}"));
        }
        if let Some([from, to]) = self.hours {
            path.push_str(&format!("&hours={from}-{to}"));
        }
        path
    }
}

/// Reads `hours=11-15`: from an hour of 0 to 23 to a later one, at most
/// 24.
#[must_use]
pub fn parse_flown_hours(text: &str) -> Option<[u8; 2]> {
    let (from, to) = text.split_once('-')?;
    let (from, to): (u8, u8) = (from.parse().ok()?, to.parse().ok()?);
    (from < to && to <= 24).then_some([from, to])
}

/// The middle of the cell of a grid of `cell_deg` degrees that a place
/// lies in, as every answer here gives places.
#[must_use]
pub fn cell_middle(place: GeoPoint, cell_deg: f64) -> GeoPoint {
    let middle = |degrees: f64| ((degrees / cell_deg).floor() + 0.5) * cell_deg;
    // Six decimals: what a cell of thousandths of a degree needs, without
    // the tail a binary fraction leaves.
    let round = |degrees: f64| (degrees * 1e6).round() / 1e6;
    GeoPoint::new(round(middle(place.lat())), round(middle(place.lon()))).unwrap_or(place)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_requests_are_asked_for_with_a_box() {
        let bbox = BoundingBox::new(47.55, 11.6, 47.85, 12.0).unwrap();
        assert_eq!(
            LiveClimbsRequest { bbox }.path_and_query(),
            "/v1/live/climbs?bbox=11.6,47.55,12,47.85"
        );
        assert_eq!(
            LiveSitesRequest { bbox }.path_and_query(),
            "/v1/live/sites?bbox=11.6,47.55,12,47.85"
        );
        let mut flown = FlownThermalsRequest {
            bbox,
            month: None,
            hours: None,
        };
        assert_eq!(
            flown.path_and_query(),
            "/v1/thermals/flown?bbox=11.6,47.55,12,47.85"
        );
        flown.month = Some(7);
        flown.hours = Some([11, 15]);
        assert_eq!(
            flown.path_and_query(),
            "/v1/thermals/flown?bbox=11.6,47.55,12,47.85&month=7&hours=11-15"
        );
    }

    #[test]
    fn hours_run_from_one_to_a_later_one() {
        assert_eq!(parse_flown_hours("11-15"), Some([11, 15]));
        assert_eq!(parse_flown_hours("0-24"), Some([0, 24]));
        for bad in ["15-11", "11-11", "11-25", "11", "a-b", "-3-4", ""] {
            assert_eq!(parse_flown_hours(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_place_is_given_as_the_middle_of_its_cell() {
        let place = GeoPoint::new(47.7012, 11.7989).unwrap();
        let middle = cell_middle(place, LIVE_CLIMB_CELL_DEG);
        assert_eq!((middle.lat(), middle.lon()), (47.7025, 11.7975));
        // Every place of the cell has the same middle.
        let other = GeoPoint::new(47.7049, 11.7951).unwrap();
        assert_eq!(cell_middle(other, LIVE_CLIMB_CELL_DEG), middle);
    }
}
