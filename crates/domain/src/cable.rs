//! Cables in the air: power lines, ropeways and the like.
//!
//! A cable is a line from pylon to pylon, hung above the ground. It is
//! drawn on a map and watched for in the cockpit. A record keeps what the
//! source says of it and no more: a height only where the source names one,
//! and a kind, which says how high such a cable usually hangs and how it is
//! called.

use serde::{Deserialize, Serialize};

use crate::{BoundingBox, GeoPoint, SourceId};

/// What a cable is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CableKind {
    /// A power line on pylons or towers, from some tens of kilovolts up.
    PowerLine,
    /// A low power line on poles, in villages and over fields: the one
    /// a pilot sees last.
    MinorPowerLine,
    /// A cable car: one or two large cabins that go to and fro.
    CableCar,
    /// A gondola lift: many small cabins on a rope that goes round, chair
    /// and cabin mixed among them.
    Gondola,
    /// A chair lift.
    ChairLift,
    /// A drag lift whose rope runs above the ground, kept only where its
    /// height is mapped.
    DragLift,
    /// A ropeway for goods, often a material ropeway up to a hut or a
    /// farm: thin, unmarked, and across a valley.
    MaterialRopeway,
    /// A zip line.
    ZipLine,
    /// Any other cable hung in the air.
    Other,
}

/// One cable of one source, as a line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cable {
    /// The source that describes the cable.
    pub source: SourceId,
    /// The source's own key for it, stable from one run to the next, for
    /// example `way/37016046`.
    pub local: String,
    /// What it is.
    pub kind: CableKind,
    /// Its course, from pylon to pylon: two points at least.
    pub line: Vec<GeoPoint>,
    /// How high it hangs above the ground, in metres, where the source
    /// names it. `None` when it does not: then nobody knows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_m: Option<f64>,
    /// The highest voltage it carries, in kilovolts, for a power line that
    /// names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voltage_kv: Option<f64>,
    /// Its name, as the source writes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Who runs it, for a power line or a lift for people: a company.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    /// The source's page about it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl Cable {
    /// The most points a line may have. A cable is a few kilometres long
    /// and hangs straight from pylon to pylon.
    pub const MAX_POINTS: usize = 2000;

    /// The highest height above the ground a record may name, in metres.
    /// The highest spans over Alpine valleys hang some hundreds of metres
    /// above the valley floor.
    pub const MAX_HEIGHT_M: f64 = 1000.0;

    /// The box around the line. `None` for a line without points.
    pub fn bounds(&self) -> Option<BoundingBox> {
        let first = self.line.first()?;
        let (mut south, mut west, mut north, mut east) =
            (first.lat(), first.lon(), first.lat(), first.lon());
        for point in &self.line {
            south = south.min(point.lat());
            north = north.max(point.lat());
            west = west.min(point.lon());
            east = east.max(point.lon());
        }
        BoundingBox::new(south, west, north, east).ok()
    }

    /// Whether any of the line lies inside `area`, judged by the box
    /// around it: a line whose box touches the area is in it.
    pub fn touches(&self, area: &BoundingBox) -> bool {
        self.bounds().is_some_and(|bounds| {
            bounds.south() <= area.north()
                && bounds.north() >= area.south()
                && bounds.west() <= area.east()
                && bounds.east() >= area.west()
        })
    }

    /// Removes what cannot be right and says what it removed: a height no
    /// cable has, a voltage that is no number of kilovolts, a name of more
    /// than 200 characters. `Err` when the record as a whole cannot be a
    /// cable: fewer than two points, or more than [`Self::MAX_POINTS`].
    pub fn check(&mut self) -> Result<Vec<&'static str>, String> {
        if self.line.len() < 2 {
            return Err("a cable needs two points at least".to_owned());
        }
        if self.line.len() > Self::MAX_POINTS {
            return Err(format!(
                "a line of {} points is more than {} a cable may have",
                self.line.len(),
                Self::MAX_POINTS
            ));
        }
        let mut removed = Vec::new();
        if self
            .height_m
            .is_some_and(|h| !(h.is_finite() && h > 0.0 && h <= Self::MAX_HEIGHT_M))
        {
            self.height_m = None;
            removed.push("height_m");
        }
        if self
            .voltage_kv
            .is_some_and(|kv| !(kv.is_finite() && kv > 0.0 && kv <= 2000.0))
        {
            self.voltage_kv = None;
            removed.push("voltage_kv");
        }
        for (field, text) in [("name", &mut self.name), ("operator", &mut self.operator)] {
            if let Some(value) = text {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    *text = None;
                } else if trimmed.chars().count() > 200 || trimmed.chars().any(char::is_control) {
                    *text = None;
                    removed.push(field);
                } else if trimmed.len() != value.len() {
                    *text = Some(trimmed.to_owned());
                }
            }
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cable() -> Cable {
        Cable {
            source: SourceId::new("example").unwrap(),
            local: "way/1".to_owned(),
            kind: CableKind::PowerLine,
            line: vec![
                GeoPoint::new(47.70, 11.70).unwrap(),
                GeoPoint::new(47.72, 11.75).unwrap(),
            ],
            height_m: Some(40.0),
            voltage_kv: Some(110.0),
            name: Some(" Leitung ".to_owned()),
            operator: None,
            url: None,
        }
    }

    #[test]
    fn a_cable_lies_in_a_box_its_box_touches() {
        let line = cable();
        let bounds = line.bounds().unwrap();
        assert_eq!(
            (bounds.south(), bounds.west(), bounds.north(), bounds.east()),
            (47.70, 11.70, 47.72, 11.75)
        );
        assert!(line.touches(&BoundingBox::new(47.71, 11.74, 47.9, 11.9).unwrap()));
        assert!(!line.touches(&BoundingBox::new(47.73, 11.70, 47.9, 11.9).unwrap()));
    }

    #[test]
    fn what_cannot_be_right_is_removed() {
        let mut checked = Cable {
            height_m: Some(-3.0),
            voltage_kv: Some(f64::NAN),
            ..cable()
        };
        assert_eq!(checked.check().unwrap(), ["height_m", "voltage_kv"]);
        assert_eq!(checked.name.as_deref(), Some("Leitung"));
        let mut short = Cable {
            line: vec![GeoPoint::new(47.7, 11.7).unwrap()],
            ..cable()
        };
        assert!(short.check().is_err());
    }
}
