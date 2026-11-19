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

use crate::{GeoPoint, SiteId};

/// A place where free-flying pilots take off or land.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Site {
    /// Identifier, scoped to the source that describes the site.
    pub id: SiteId,
    /// Name as the source gives it. Many sites have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Where the site is. For a site the source draws as an area, a point
    /// inside or near it, as the source computes it.
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
        }
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
            ..wallberg()
        };
        let json = serde_json::to_string(&site).unwrap();
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
