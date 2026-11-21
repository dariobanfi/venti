//! The canonical flying site.
//!
//! A [`Site`] is one place where free-flying pilots take off or land, as one
//! source describes it. A mountain with three take-offs and a landing field
//! is four sites. The record keeps what the source says and nothing more: a
//! take-off whose source gives no wind directions has none here, and a
//! place whose source does not say what it is used for has no use.
//!
//! What a record does not say: whether a pilot may fly there today, or at
//! all. Sites come from sources that anyone can edit. The rules of a site
//! are its operator's.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{GeoError, GeoPoint, SiteId};

/// A place where free-flying pilots take off or land.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Site {
    /// Identifier, scoped to the source that describes the site.
    pub id: SiteId,
    /// Name as the source gives it. Many sites have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Where the site is. For a site the source draws as an area, a point
    /// inside or near it: for OpenStreetMap's, the middle of the box around
    /// the area, which need not lie inside it.
    pub position: GeoPoint,
    /// Height above mean sea level in metres, if the source gives one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elevation_m: Option<f64>,
    /// What the site is used for, each use once, in the order of
    /// [`SiteUse`]. Empty when the source does not say.
    pub uses: Vec<SiteUse>,
    /// The wind directions the site works in, as the source gives them.
    /// Empty when it gives none, which says nothing about the site.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wind_directions: Vec<WindSector>,
    /// Whether the source says the site suits paragliders. `None` when it
    /// does not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paragliding: Option<bool>,
    /// Whether the source says the site suits hang gliders. `None` when it
    /// does not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hang_gliding: Option<bool>,
    /// Whether the source calls the site official: legal and looked after.
    /// `None` when it does not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub official: Option<bool>,
    /// Whether the source marks the site as unsafe, or one of its uses as
    /// discouraged. `false` says only that the source marks nothing of the
    /// kind.
    #[serde(default, skip_serializing_if = "is_false")]
    pub discouraged: bool,
    /// Where the source's record can be looked at, and corrected if the
    /// source is one that anyone can edit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// A web page about the site that the source names: its operator's,
    /// or one that says more about it. An `http` or `https` address. It is
    /// somebody else's page, and what it says is theirs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    /// What the source says about the site in words, as its author wrote
    /// it and in the author's language. It may have several lines. It is
    /// shown as written; nothing is read out of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether a payment is asked for using the site, as the source writes
    /// it: `yes`, `no`, or other words of its author's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fee: Option<String>,
    /// Whether pilots who are guests at the site have rules for guests to
    /// take notice of. `None` when the source does not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guest_guidelines: Option<bool>,
    /// A time in which the site may not be used, as the source writes it:
    /// `01.04-30.09`. It is shown as written and never reckoned with: no
    /// code says from it that a site is open or closed today. `None` says
    /// only that the source names no such time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_flight_time: Option<String>,
    /// The outline of a site its source draws as an area: a landing field,
    /// a take-off meadow. `None` for a site drawn as a point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<SiteOutline>,
}

/// The outline of a flying site: one closed ring of points, the last the
/// first again, with [`SiteOutline::MAX_POINTS`] points at most. It is for
/// drawing a field on a map. A source that draws a site with more points
/// has it drawn with fewer by its adapter.
///
/// Written as a list of `[longitude, latitude]`, the order GeoJSON uses and
/// the one an airspace's rings are written in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Vec<[f64; 2]>", into = "Vec<[f64; 2]>")]
pub struct SiteOutline(Vec<GeoPoint>);

/// Why a list of points is no outline of a site.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum SiteOutlineError {
    /// Fewer points than a closed triangle has.
    #[error("an outline needs at least 4 points, the last the first again, and has {0}")]
    TooFewPoints(usize),
    /// More points than an outline may have.
    #[error("an outline has at most {max} points and this one has {0}", max = SiteOutline::MAX_POINTS)]
    TooManyPoints(usize),
    /// The last point is not the first.
    #[error("the outline is not closed: its last point is not its first")]
    NotClosed,
    /// A point that is no place on the globe.
    #[error("a point of the outline is no place: {0}")]
    Point(#[from] GeoError),
}

impl SiteOutline {
    /// The most points an outline has, the closing one included. Of the
    /// 205 sites OpenStreetMap draws as areas in the first region, by its
    /// data of 2026-11-20, half have 6 points or fewer and two have more
    /// than this: 67 and 203.
    pub const MAX_POINTS: usize = 64;

    /// Creates an outline from a closed ring.
    pub fn new(points: Vec<GeoPoint>) -> Result<Self, SiteOutlineError> {
        if points.len() < 4 {
            return Err(SiteOutlineError::TooFewPoints(points.len()));
        }
        if points.len() > Self::MAX_POINTS {
            return Err(SiteOutlineError::TooManyPoints(points.len()));
        }
        if points.first() != points.last() {
            return Err(SiteOutlineError::NotClosed);
        }
        Ok(Self(points))
    }

    /// The ring: its last point is its first again.
    pub fn points(&self) -> &[GeoPoint] {
        &self.0
    }
}

impl TryFrom<Vec<[f64; 2]>> for SiteOutline {
    type Error = SiteOutlineError;

    fn try_from(ring: Vec<[f64; 2]>) -> Result<Self, Self::Error> {
        let points = ring
            .into_iter()
            .map(|[lon, lat]| GeoPoint::new(lat, lon))
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(points)
    }
}

impl From<SiteOutline> for Vec<[f64; 2]> {
    fn from(outline: SiteOutline) -> Self {
        outline
            .0
            .into_iter()
            .map(|point| [point.lon(), point.lat()])
            .collect()
    }
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde hands the field over by reference"
)]
fn is_false(value: &bool) -> bool {
    !*value
}

/// What a site is used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SiteUse {
    /// Taking off on foot.
    Takeoff,
    /// Landing. A place to land at the top of a hill counts too.
    Landing,
    /// Taking off on a tow line, behind a winch.
    Towing,
    /// Training: a slope or field to practise on, not to fly from.
    Training,
    /// A use this version of the code does not know.
    #[serde(other)]
    Other,
}

/// A span of wind directions, clockwise from one direction to another.
///
/// A direction is where the wind comes from, in degrees clockwise from true
/// north. A source that names single directions (`S`, `SW`) gives sectors
/// whose two ends are the same; nothing is widened here.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindSector {
    /// Where the span starts, degrees.
    pub from_deg: f64,
    /// Where it ends, going clockwise, degrees. Equal to `from_deg` for a
    /// single direction.
    pub to_deg: f64,
}

impl WindSector {
    /// A single direction.
    pub fn single(deg: f64) -> Self {
        Self {
            from_deg: deg,
            to_deg: deg,
        }
    }

    /// Whether wind from `deg` lies in the span, ends included. A span from
    /// 0 to 360 is the whole compass.
    pub fn contains(&self, deg: f64) -> bool {
        let turn = |angle: f64| angle.rem_euclid(360.0);
        if self.to_deg - self.from_deg >= 360.0 {
            return true;
        }
        let width = turn(self.to_deg - self.from_deg);
        turn(deg - self.from_deg) <= width
    }
}

impl Site {
    /// Whether the site has this use.
    pub fn is(&self, used_for: SiteUse) -> bool {
        self.uses.contains(&used_for)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wallberg() -> Site {
        Site {
            id: "osm-free-flying:node/5964676196".parse().unwrap(),
            name: None,
            position: GeoPoint::new(47.6652, 11.7954).unwrap(),
            elevation_m: None,
            uses: vec![SiteUse::Takeoff],
            wind_directions: Vec::new(),
            paragliding: None,
            hang_gliding: None,
            official: None,
            discouraged: false,
            url: None,
            website: None,
            description: None,
            fee: None,
            guest_guidelines: None,
            no_flight_time: None,
            outline: None,
        }
    }

    fn field() -> Vec<GeoPoint> {
        [
            (47.6921, 11.8861),
            (47.6927, 11.8862),
            (47.6926, 11.887),
            (47.6921, 11.8861),
        ]
        .into_iter()
        .map(|(lat, lon)| GeoPoint::new(lat, lon).unwrap())
        .collect()
    }

    #[test]
    fn what_a_source_does_not_say_is_left_out() {
        let json = serde_json::to_string(&wallberg()).unwrap();
        assert_eq!(
            json,
            r#"{"id":"osm-free-flying:node/5964676196","position":{"lat":47.6652,"lon":11.7954},"uses":["takeoff"]}"#
        );
        assert_eq!(serde_json::from_str::<Site>(&json).unwrap(), wallberg());
    }

    #[test]
    fn a_full_record_is_read_back() {
        let site = Site {
            name: Some("Fluggelände Jägerkamp/Nagelspitz".to_owned()),
            elevation_m: Some(1560.0),
            uses: vec![SiteUse::Takeoff, SiteUse::Landing],
            wind_directions: vec![WindSector::single(292.5), WindSector::single(270.0)],
            paragliding: Some(true),
            hang_gliding: Some(false),
            official: Some(true),
            discouraged: true,
            url: Some("https://www.openstreetmap.org/way/1080986173".to_owned()),
            website: Some("https://example.org/fluggebiet".to_owned()),
            description: Some("Nicht im hohen Gras landen.\nWindsack vorhanden.".to_owned()),
            fee: Some("yes".to_owned()),
            guest_guidelines: Some(true),
            no_flight_time: Some("01.04-30.09".to_owned()),
            outline: Some(SiteOutline::new(field()).unwrap()),
            ..wallberg()
        };
        let json = serde_json::to_string(&site).unwrap();
        // An outline is written as GeoJSON writes a ring: longitude first.
        assert!(
            json.contains(
                r#""outline":[[11.8861,47.6921],[11.8862,47.6927],[11.887,47.6926],[11.8861,47.6921]]"#
            ),
            "{json}"
        );
        assert!(json.contains(r#""guest_guidelines":true"#), "{json}");
        assert!(json.contains(r#""no_flight_time":"01.04-30.09""#), "{json}");
        assert!(json.contains(r#""uses":["takeoff","landing"]"#), "{json}");
        assert!(
            json.contains(r#""wind_directions":[{"from_deg":292.5,"to_deg":292.5},"#),
            "{json}"
        );
        assert!(json.contains(r#""discouraged":true"#), "{json}");
        assert_eq!(serde_json::from_str::<Site>(&json).unwrap(), site);
        assert!(site.is(SiteUse::Landing));
        assert!(!site.is(SiteUse::Towing));
    }

    #[test]
    fn an_outline_is_a_closed_ring_of_a_bounded_size() {
        let outline = SiteOutline::new(field()).unwrap();
        assert_eq!(outline.points().len(), 4);
        assert_eq!(outline.points().first(), outline.points().last());

        let mut open = field();
        open.pop();
        assert_eq!(
            SiteOutline::new(open.clone()),
            Err(SiteOutlineError::TooFewPoints(3))
        );
        open.push(GeoPoint::new(47.6922, 11.8868).unwrap());
        assert_eq!(SiteOutline::new(open), Err(SiteOutlineError::NotClosed));

        // A ring of more points than an outline may have.
        let mut long = field();
        let corner = long[1];
        long.splice(
            1..1,
            std::iter::repeat_n(corner, SiteOutline::MAX_POINTS - 3),
        );
        assert_eq!(
            SiteOutline::new(long),
            Err(SiteOutlineError::TooManyPoints(SiteOutline::MAX_POINTS + 1))
        );

        // A record with a damaged outline is no record.
        for damaged in [
            "[[11.8,47.6],[11.9,47.6]]",
            "[[11.8,147.6],[11.9,47.6],[11.9,47.7],[11.8,147.6]]",
        ] {
            let text = format!(
                r#"{{"id":"osm-free-flying:way/1","position":{{"lat":47.6,"lon":11.8}},"uses":[],"outline":{damaged}}}"#
            );
            assert!(serde_json::from_str::<Site>(&text).is_err(), "{damaged}");
        }
    }

    #[test]
    fn a_use_from_a_newer_source_reads_as_other() {
        let text = r#"{"id":"osm-free-flying:node/1","position":{"lat":47.0,"lon":11.0},
                       "uses":["takeoff","ground-handling"]}"#;
        let site: Site = serde_json::from_str(text).unwrap();
        assert_eq!(site.uses, [SiteUse::Takeoff, SiteUse::Other]);
    }

    #[test]
    fn a_sector_runs_clockwise_and_may_cross_north() {
        let south = WindSector {
            from_deg: 135.0,
            to_deg: 225.0,
        };
        assert!(south.contains(135.0) && south.contains(180.0) && south.contains(225.0));
        assert!(!south.contains(90.0) && !south.contains(270.0));

        let north = WindSector {
            from_deg: 315.0,
            to_deg: 45.0,
        };
        assert!(north.contains(0.0) && north.contains(360.0) && north.contains(330.0));
        assert!(!north.contains(180.0));

        let west = WindSector::single(270.0);
        assert!(west.contains(270.0));
        assert!(!west.contains(271.0));

        let all = WindSector {
            from_deg: 0.0,
            to_deg: 360.0,
        };
        assert!(all.contains(0.0) && all.contains(123.0) && all.contains(359.9));
    }
}
