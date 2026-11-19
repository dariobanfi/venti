//! The canonical airspace record.
//!
//! Every airspace source is converted into [`Airspace`] before anything else
//! touches it: one shape whether the data came from an OpenAir file a pilot
//! imported or from a provider's export. The record keeps what was published
//! (a limit of `4500 ft` stays 4500 feet) and gives the same thing in metres,
//! so nothing downstream converts units or guesses a reference.
//!
//! What a record does not say: whether the airspace is active right now.
//! Activations are a kind of their own. Code that warns a pilot must treat
//! every airspace as potentially active.

use chrono::NaiveTime;
use serde::{Deserialize, Serialize};

use crate::{AirspaceId, BoundingBox, GeoPoint, Timestamp};

/// Metres in one foot, exactly.
pub const METRES_PER_FOOT: f64 = 0.3048;

/// Most points one ring of a polygon may have. The longest ring in the
/// German and Austrian data of 2026 has about 3,000.
pub const MAX_RING_POINTS: usize = 50_000;

/// Most holes a polygon may have.
pub const MAX_HOLES: usize = 1_000;

/// Widest a polygon may be in either direction, in degrees. Nothing a pilot
/// flies through is larger, and a ring that wraps around the antimeridian
/// shows up as one that is nearly 360 degrees wide.
pub const MAX_SPAN_DEG: f64 = 90.0;

/// Why an airspace, or a part of one, was rejected.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AirspaceError {
    /// A height is not a finite number or lies outside what an airspace limit can be.
    #[error("height {value} {unit:?} is not a usable airspace limit")]
    Height {
        /// The value as published.
        value: f64,
        /// Its unit.
        unit: HeightUnit,
    },
    /// A flight level was given with a reference other than standard pressure.
    #[error("a flight level is always measured from standard pressure, not from {0:?}")]
    FlightLevelReference(VerticalReference),
    /// A ring has fewer than four points; a triangle needs its first point repeated at the end.
    #[error("a ring needs at least four points, this one has {0}")]
    RingTooShort(usize),
    /// A ring has more points than any real airspace.
    #[error("a ring has {0} points, more than the {MAX_RING_POINTS} allowed")]
    RingTooLong(usize),
    /// A ring does not end where it starts.
    #[error("a ring does not end at its first point")]
    RingNotClosed,
    /// A ring encloses no area: all its points lie on one line or one spot.
    #[error("a ring encloses no area")]
    RingWithoutArea,
    /// A polygon is wider than any real airspace, or wraps around the antimeridian.
    #[error("a polygon spans {0:.1} degrees, more than the {MAX_SPAN_DEG} allowed")]
    TooLarge(f64),
    /// A polygon has more holes than any real airspace.
    #[error("a polygon has {0} holes, more than the {MAX_HOLES} allowed")]
    TooManyHoles(usize),
    /// A circle's radius is not a positive, finite number of plausible size.
    #[error("circle radius {0} m is not usable")]
    Radius(f64),
    /// The lower limit lies above the upper limit.
    #[error("the lower limit lies above the upper limit")]
    LimitsInverted,
    /// The lower limit is "unlimited" or the upper limit is the ground.
    #[error("an airspace cannot start at 'unlimited' or end at the ground")]
    LimitMisplaced,
    /// The country is not a two-letter upper-case code.
    #[error("country {0:?} is not a two-letter upper-case code")]
    Country(String),
    /// A frequency is not a positive, finite number of megahertz in the aviation bands.
    #[error("frequency {0} MHz is not usable")]
    Frequency(f64),
}

/// The class of an airspace: what a pilot needs in order to fly in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AirspaceClass {
    /// Class A: instrument flights only.
    A,
    /// Class B.
    B,
    /// Class C: every flight needs a clearance.
    C,
    /// Class D: every flight needs a clearance.
    D,
    /// Class E: controlled, but open to visual flights without a clearance.
    E,
    /// Class F: advisory airspace.
    F,
    /// Class G: uncontrolled.
    G,
    /// The source gives no class. Usual for restricted areas and other
    /// special-use airspace, whose type says what applies.
    #[serde(rename = "unclassified")]
    Unclassified,
}

/// What kind of airspace this is.
///
/// The list is what the OpenAir format (with Naviter's extensions) and
/// openAIP distinguish. A type none of the variants fits is kept as
/// [`AirspaceType::Other`] with the source's own word for it, so nothing is
/// forced into a wrong category.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AirspaceType {
    /// Control zone around an aerodrome (CTR).
    Ctr,
    /// Control zone of a military aerodrome (MCTR).
    MilitaryCtr,
    /// Terminal manoeuvring area (TMA): controlled airspace above and around a control zone.
    Tma,
    /// Control area (CTA).
    Cta,
    /// Aerodrome traffic zone (ATZ).
    Atz,
    /// Military aerodrome traffic zone (MATZ).
    Matz,
    /// Helicopter traffic zone (HTZ).
    Htz,
    /// Traffic information zone (TIZ).
    Tiz,
    /// Traffic information area (TIA).
    Tia,
    /// Restricted area: entry is restricted while it is active.
    Restricted,
    /// Danger area: activities dangerous to aircraft may take place.
    Danger,
    /// Prohibited area.
    Prohibited,
    /// Closed to gliders (the original OpenAir class `GP`).
    GliderProhibited,
    /// Transponder mandatory zone (TMZ).
    Tmz,
    /// Radio mandatory zone (RMZ).
    Rmz,
    /// Zone in which a transponder is recommended (TRZ).
    TransponderRecommended,
    /// Area with a transponder code to set (TRP).
    TransponderSetting,
    /// Temporary reserved area (TRA). What it means differs by country.
    Tra,
    /// Temporary segregated area (TSA).
    Tsa,
    /// Route by which a TRA or TSA is entered and left.
    FeedingRoute,
    /// Temporary flight restriction.
    TemporaryRestriction,
    /// Area that exists only while a NOTAM says so.
    NotamArea,
    /// Sector of controlled airspace that can be opened to gliders, and
    /// wave-soaring windows (the original OpenAir class `W`).
    GliderSector,
    /// Area of aerial sport or recreation: parachute drop zones, aerobatic boxes and the like.
    AerialSporting,
    /// Area that must not be overflown low, usually for nature protection.
    OverflightRestriction,
    /// Protected area.
    Protected,
    /// Military training area (MTA).
    MilitaryTrainingArea,
    /// Military training route (MTR).
    MilitaryTrainingRoute,
    /// Military route (MRT).
    MilitaryRoute,
    /// Air defence identification zone (ADIZ).
    Adiz,
    /// Alert area.
    Alert,
    /// Warning area.
    Warning,
    /// Flight information region (FIR).
    Fir,
    /// Upper flight information region (UIR).
    Uir,
    /// Sector of a flight information service.
    FisSector,
    /// Sector of an area control centre.
    AccSector,
    /// Sector for visual flights.
    VfrSector,
    /// Designated route for visual flights.
    VfrRoute,
    /// Airway.
    Airway,
    /// Lower traffic area (LTA).
    Lta,
    /// Upper traffic area (UTA).
    Uta,
    /// The source gives a class and no type: plain airspace of that class.
    Unspecified,
    /// A type none of the above fits, in the source's own words.
    Other(String),
}

/// What a vertical limit is measured from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerticalReference {
    /// The limit is the surface of the earth (`GND`, `SFC`).
    Ground,
    /// A height above the ground directly below (`AGL`).
    AboveGround,
    /// An altitude above mean sea level (`AMSL`, `MSL`).
    MeanSeaLevel,
    /// A pressure altitude on the standard atmosphere, 1013.25 hPa (`FL`, `STD`).
    Standard,
    /// No limit (`UNL`).
    Unlimited,
}

/// The unit a height was published in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HeightUnit {
    /// Feet.
    Feet,
    /// Metres.
    Metres,
    /// Flight levels: hundreds of feet.
    FlightLevel,
}

/// The floor or the ceiling of an airspace, as published.
///
/// A limit is a value, the unit it was published in and what it is measured
/// from. `FL 95` is 95 flight levels above standard pressure; `1000 ft AGL`
/// is 1000 feet above the ground. [`VerticalLimit::metres`] gives the same
/// height in metres, still above the same reference: turning that into an
/// altitude a pilot can compare with their own needs the ground elevation
/// or the air pressure, which the record does not have.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawLimit", into = "RawLimit")]
pub struct VerticalLimit {
    reference: VerticalReference,
    value: f64,
    unit: HeightUnit,
}

#[derive(Serialize, Deserialize)]
struct RawLimit {
    reference: VerticalReference,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    unit: Option<HeightUnit>,
    /// Written for readers; ignored when read, because it follows from the rest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    metres: Option<f64>,
}

impl VerticalLimit {
    /// The surface of the earth.
    pub const fn ground() -> Self {
        Self {
            reference: VerticalReference::Ground,
            value: 0.0,
            unit: HeightUnit::Feet,
        }
    }

    /// No limit.
    pub const fn unlimited() -> Self {
        Self {
            reference: VerticalReference::Unlimited,
            value: 0.0,
            unit: HeightUnit::Feet,
        }
    }

    /// A limit from its three published parts.
    ///
    /// For [`VerticalReference::Ground`] and [`VerticalReference::Unlimited`]
    /// the value and unit mean nothing and are dropped. A height must be a
    /// finite number between 500 m below and 100 km above its reference (not
    /// negative above ground or on the standard atmosphere), and a flight
    /// level is always measured from standard pressure.
    pub fn new(
        reference: VerticalReference,
        value: f64,
        unit: HeightUnit,
    ) -> Result<Self, AirspaceError> {
        match reference {
            VerticalReference::Ground => return Ok(Self::ground()),
            VerticalReference::Unlimited => return Ok(Self::unlimited()),
            VerticalReference::AboveGround
            | VerticalReference::MeanSeaLevel
            | VerticalReference::Standard => {}
        }
        if unit == HeightUnit::FlightLevel && reference != VerticalReference::Standard {
            return Err(AirspaceError::FlightLevelReference(reference));
        }
        let limit = Self {
            reference,
            value,
            unit,
        };
        let lowest = match reference {
            VerticalReference::MeanSeaLevel => -500.0,
            _ => 0.0,
        };
        match limit.metres() {
            Some(metres) if metres.is_finite() && (lowest..=100_000.0).contains(&metres) => {
                Ok(limit)
            }
            _ => Err(AirspaceError::Height { value, unit }),
        }
    }

    /// A flight level, for example `flight_level(95)` for FL 95.
    pub fn flight_level(level: u16) -> Result<Self, AirspaceError> {
        Self::new(
            VerticalReference::Standard,
            f64::from(level),
            HeightUnit::FlightLevel,
        )
    }

    /// What the limit is measured from.
    pub fn reference(&self) -> VerticalReference {
        self.reference
    }

    /// The value as published, in [`VerticalLimit::unit`]. Zero for the
    /// ground and for "unlimited".
    pub fn value(&self) -> f64 {
        self.value
    }

    /// The unit the value was published in.
    pub fn unit(&self) -> HeightUnit {
        self.unit
    }

    /// The height in metres above the limit's own reference: zero for the
    /// ground, `None` for "unlimited".
    pub fn metres(&self) -> Option<f64> {
        match self.reference {
            VerticalReference::Ground => Some(0.0),
            VerticalReference::Unlimited => None,
            _ => Some(match self.unit {
                HeightUnit::Feet => self.value * METRES_PER_FOOT,
                HeightUnit::Metres => self.value,
                HeightUnit::FlightLevel => self.value * 100.0 * METRES_PER_FOOT,
            }),
        }
    }

    /// Whether this limit certainly lies above `other`, which can only be
    /// told when both are measured from the same thing.
    fn certainly_above(&self, other: &VerticalLimit) -> bool {
        if self.reference != other.reference {
            return false;
        }
        match (self.metres(), other.metres()) {
            (Some(mine), Some(theirs)) => mine > theirs,
            _ => false,
        }
    }
}

impl TryFrom<RawLimit> for VerticalLimit {
    type Error = AirspaceError;
    fn try_from(raw: RawLimit) -> Result<Self, Self::Error> {
        Self::new(
            raw.reference,
            raw.value.unwrap_or(0.0),
            raw.unit.unwrap_or(HeightUnit::Feet),
        )
    }
}

impl From<VerticalLimit> for RawLimit {
    fn from(limit: VerticalLimit) -> Self {
        let has_value = !matches!(
            limit.reference,
            VerticalReference::Ground | VerticalReference::Unlimited
        );
        Self {
            reference: limit.reference,
            value: has_value.then_some(limit.value),
            unit: has_value.then_some(limit.unit),
            metres: limit.metres(),
        }
    }
}

/// A circle on the ground.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Circle {
    /// The centre.
    pub centre: GeoPoint,
    /// The radius in metres.
    pub radius_m: f64,
}

/// The horizontal shape of an airspace: one outer ring and, rarely, holes.
///
/// A ring is a list of points that ends where it starts. The edge between
/// two neighbouring points is the straight line between them in longitude
/// and latitude, as the GeoJSON standard defines it; sources that mean a
/// curve publish enough points for the difference not to matter.
///
/// Sources that define a shape by circles and arcs are converted to rings
/// by their adapter, which states the error it allows. Where the whole
/// airspace is one circle, [`Polygon::circle`] keeps the centre and radius,
/// so a map can draw a true circle. The ring is still what every calculation
/// uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawPolygon", into = "RawPolygon")]
pub struct Polygon {
    exterior: Vec<GeoPoint>,
    holes: Vec<Vec<GeoPoint>>,
    circle: Option<Circle>,
}

/// Rings are written as lists of `[longitude, latitude]`, the order GeoJSON uses.
#[derive(Serialize, Deserialize)]
struct RawPolygon {
    exterior: Vec<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    holes: Vec<Vec<[f64; 2]>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    circle: Option<Circle>,
}

impl Polygon {
    /// Creates a polygon, checking every ring: closed, at least a triangle,
    /// enclosing an area, and of a size an airspace can have.
    pub fn new(exterior: Vec<GeoPoint>, holes: Vec<Vec<GeoPoint>>) -> Result<Self, AirspaceError> {
        check_ring(&exterior)?;
        if holes.len() > MAX_HOLES {
            return Err(AirspaceError::TooManyHoles(holes.len()));
        }
        for hole in &holes {
            check_ring(hole)?;
        }
        let polygon = Self {
            exterior,
            holes,
            circle: None,
        };
        let bounds = polygon.bounds();
        let span = (bounds.north() - bounds.south()).max(bounds.east() - bounds.west());
        if span > MAX_SPAN_DEG {
            return Err(AirspaceError::TooLarge(span));
        }
        Ok(polygon)
    }

    /// Records that the polygon stands for this circle.
    pub fn with_circle(mut self, circle: Circle) -> Result<Self, AirspaceError> {
        if !circle.radius_m.is_finite() || circle.radius_m <= 0.0 || circle.radius_m > 2_000_000.0 {
            return Err(AirspaceError::Radius(circle.radius_m));
        }
        self.circle = Some(circle);
        Ok(self)
    }

    /// The outer ring. Its last point equals its first.
    pub fn exterior(&self) -> &[GeoPoint] {
        &self.exterior
    }

    /// The holes: rings inside the outer ring that do not belong to the airspace.
    pub fn holes(&self) -> &[Vec<GeoPoint>] {
        &self.holes
    }

    /// The circle this polygon stands for, if its source defined it as one.
    pub fn circle(&self) -> Option<Circle> {
        self.circle
    }

    /// The smallest box around the outer ring.
    pub fn bounds(&self) -> BoundingBox {
        let (mut south, mut west) = (f64::INFINITY, f64::INFINITY);
        let (mut north, mut east) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for point in &self.exterior {
            south = south.min(point.lat());
            north = north.max(point.lat());
            west = west.min(point.lon());
            east = east.max(point.lon());
        }
        BoundingBox::new(south, west, north, east)
            .expect("a checked ring has points, and points have valid coordinates")
    }
}

fn check_ring(ring: &[GeoPoint]) -> Result<(), AirspaceError> {
    if ring.len() < 4 {
        return Err(AirspaceError::RingTooShort(ring.len()));
    }
    if ring.len() > MAX_RING_POINTS {
        return Err(AirspaceError::RingTooLong(ring.len()));
    }
    if ring.first() != ring.last() {
        return Err(AirspaceError::RingNotClosed);
    }
    // Twice the enclosed area in square degrees, by the shoelace formula,
    // taken relative to the first point to keep the numbers small.
    let origin = ring[0];
    let doubled_area: f64 = ring
        .windows(2)
        .map(|pair| {
            let (ax, ay) = (pair[0].lon() - origin.lon(), pair[0].lat() - origin.lat());
            let (bx, by) = (pair[1].lon() - origin.lon(), pair[1].lat() - origin.lat());
            ax * by - bx * ay
        })
        .sum();
    // A tenth of a metre squared, in degrees.
    if doubled_area.abs() < 1e-12 {
        return Err(AirspaceError::RingWithoutArea);
    }
    Ok(())
}

impl TryFrom<RawPolygon> for Polygon {
    type Error = String;
    fn try_from(raw: RawPolygon) -> Result<Self, Self::Error> {
        let ring = |raw: Vec<[f64; 2]>| -> Result<Vec<GeoPoint>, String> {
            raw.into_iter()
                .map(|[lon, lat]| GeoPoint::new(lat, lon).map_err(|e| e.to_string()))
                .collect()
        };
        let exterior = ring(raw.exterior)?;
        let holes = raw.holes.into_iter().map(ring).collect::<Result<_, _>>()?;
        let polygon = Polygon::new(exterior, holes).map_err(|e| e.to_string())?;
        match raw.circle {
            Some(circle) => polygon.with_circle(circle).map_err(|e| e.to_string()),
            None => Ok(polygon),
        }
    }
}

impl From<Polygon> for RawPolygon {
    fn from(polygon: Polygon) -> Self {
        let ring = |ring: Vec<GeoPoint>| ring.iter().map(|p| [p.lon(), p.lat()]).collect();
        Self {
            exterior: ring(polygon.exterior),
            holes: polygon.holes.into_iter().map(ring).collect(),
            circle: polygon.circle,
        }
    }
}

/// A time of day in a schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DayTime {
    /// A time on the clock.
    Clock(NaiveTime),
    /// Sunrise.
    Sunrise,
    /// Sunset.
    Sunset,
}

/// The hours an airspace is in use on one day of the week, as published.
///
/// Whether the times are UTC or local is the source's convention and is
/// stated by its adapter. Nothing in this project computes with them yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatingHours {
    /// The day of the week.
    pub day: chrono::Weekday,
    /// When use begins.
    pub start: DayTime,
    /// When use ends.
    pub end: DayTime,
    /// The hours hold only when a NOTAM says so.
    pub by_notam: bool,
    /// The hours do not hold on public holidays.
    pub except_public_holidays: bool,
    /// The source's note on this day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remarks: Option<String>,
}

/// What a source says about when an airspace is in use.
///
/// This is published background, not the state of today: an area "active by
/// NOTAM" is active when a NOTAM says so, and the record cannot know that.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    /// Active only when a NOTAM announces it.
    pub by_notam: bool,
    /// Activated on the request of those who use it.
    pub on_request: bool,
    /// Activated on demand. Sources that tell this from "on request" do not
    /// define the difference; it is kept as published.
    pub on_demand: bool,
    /// Usable only under a special agreement, for example by the members of one club.
    pub special_agreement: bool,
    /// Hours of use by day of the week. Empty when the source publishes
    /// none, which says nothing about when the airspace is active.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hours: Vec<OperatingHours>,
    /// The source's note on its hours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hours_remarks: Option<String>,
    /// The airspace exists from this time on. Set for temporary areas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Timestamp>,
    /// The airspace ceases to exist at this time. Set for temporary areas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<Timestamp>,
}

/// A radio frequency on which the unit responsible for an airspace is reached.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frequency {
    /// The frequency in megahertz, for example 120.1.
    pub mhz: f64,
    /// The name of the station, as called on the radio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether the source marks this as the main frequency.
    pub primary: bool,
}

/// How current a record is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Validity {
    /// The AIRAC cycle the record belongs to, in the source's own notation,
    /// if the source states one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub airac: Option<String>,
    /// The date of the file the record was read from: when the source wrote
    /// it or, where the source does not say, when it was fetched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_date: Option<Timestamp>,
    /// When the source last changed this record, if it says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<Timestamp>,
}

/// One airspace: a volume of air with rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Airspace {
    /// Identifier, scoped to the source that publishes the airspace.
    pub id: AirspaceId,
    /// Name as the source publishes it. May be empty: some files name nothing.
    pub name: String,
    /// The class.
    pub class: AirspaceClass,
    /// The type.
    pub airspace_type: AirspaceType,
    /// The country whose airspace this is, as an ISO 3166-1 alpha-2 code, if
    /// the source says. The same type can mean different things in
    /// different countries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    /// The floor.
    pub lower: VerticalLimit,
    /// The ceiling.
    pub upper: VerticalLimit,
    /// A second floor, for limits published as "5000 ft AMSL but at least
    /// 1000 ft AGL": the airspace starts at whichever of the two is higher.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lower_at_least: Option<VerticalLimit>,
    /// The height up to which the ceiling may be raised, for limits
    /// published as "5000 ft AMSL, at most 8000 ft AMSL on request".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upper_at_most: Option<VerticalLimit>,
    /// The horizontal shape.
    pub geometry: Polygon,
    /// What the source says about when the airspace is in use.
    pub activity: Activity,
    /// Frequencies of the responsible unit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frequencies: Vec<Frequency>,
    /// The source's free-text note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remarks: Option<String>,
    /// How current the record is.
    pub validity: Validity,
}

impl Airspace {
    /// Checks what the types cannot: that the limits are the right way
    /// round where they can be compared, that the country is a code and that
    /// the frequencies are frequencies.
    ///
    /// Heights, rings and sizes are already checked when a [`VerticalLimit`]
    /// or a [`Polygon`] is made. Limits with different references (a floor
    /// above ground under a ceiling above sea level) cannot be compared
    /// without knowing the terrain and are accepted.
    pub fn validate(&self) -> Result<(), AirspaceError> {
        if self.lower.reference() == VerticalReference::Unlimited
            || self.upper.reference() == VerticalReference::Ground
        {
            return Err(AirspaceError::LimitMisplaced);
        }
        if self.lower.certainly_above(&self.upper) {
            return Err(AirspaceError::LimitsInverted);
        }
        if let Some(country) = &self.country {
            let is_code = country.len() == 2 && country.bytes().all(|b| b.is_ascii_uppercase());
            if !is_code {
                return Err(AirspaceError::Country(country.clone()));
            }
        }
        for frequency in &self.frequencies {
            // From the lowest aviation band to the top of the military one.
            if !frequency.mhz.is_finite() || !(0.1..=400.0).contains(&frequency.mhz) {
                return Err(AirspaceError::Frequency(frequency.mhz));
            }
        }
        Ok(())
    }

    /// The smallest box around the airspace.
    pub fn bounds(&self) -> BoundingBox {
        self.geometry.bounds()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).unwrap()
    }

    fn square() -> Vec<GeoPoint> {
        vec![
            point(47.0, 11.0),
            point(47.0, 11.2),
            point(47.1, 11.2),
            point(47.1, 11.0),
            point(47.0, 11.0),
        ]
    }

    fn feet(reference: VerticalReference, value: f64) -> VerticalLimit {
        VerticalLimit::new(reference, value, HeightUnit::Feet).unwrap()
    }

    fn airspace(lower: VerticalLimit, upper: VerticalLimit) -> Airspace {
        Airspace {
            id: "example:ctr".parse().unwrap(),
            name: "CTR EXAMPLE".to_owned(),
            class: AirspaceClass::D,
            airspace_type: AirspaceType::Ctr,
            country: Some("AT".to_owned()),
            lower,
            upper,
            lower_at_least: None,
            upper_at_most: None,
            geometry: Polygon::new(square(), Vec::new()).unwrap(),
            activity: Activity::default(),
            frequencies: vec![Frequency {
                mhz: 120.1,
                name: Some("EXAMPLE TOWER".to_owned()),
                primary: true,
            }],
            remarks: None,
            validity: Validity::default(),
        }
    }

    #[test]
    fn limits_convert_to_metres_above_their_own_reference() {
        let msl = feet(VerticalReference::MeanSeaLevel, 4500.0);
        assert!((msl.metres().unwrap() - 1371.6).abs() < 1e-9);
        let fl = VerticalLimit::flight_level(100).unwrap();
        assert!((fl.metres().unwrap() - 3048.0).abs() < 1e-9);
        assert_eq!(fl.reference(), VerticalReference::Standard);
        let metric = VerticalLimit::new(VerticalReference::AboveGround, 300.0, HeightUnit::Metres);
        assert_eq!(metric.unwrap().metres(), Some(300.0));
        assert_eq!(VerticalLimit::ground().metres(), Some(0.0));
        assert_eq!(VerticalLimit::unlimited().metres(), None);
    }

    #[test]
    fn limits_reject_what_cannot_be_a_height() {
        let new = VerticalLimit::new;
        assert!(new(VerticalReference::MeanSeaLevel, f64::NAN, HeightUnit::Feet).is_err());
        assert!(
            new(
                VerticalReference::MeanSeaLevel,
                f64::INFINITY,
                HeightUnit::Feet
            )
            .is_err()
        );
        assert!(new(VerticalReference::MeanSeaLevel, 1e9, HeightUnit::Feet).is_err());
        assert!(new(VerticalReference::AboveGround, -1.0, HeightUnit::Feet).is_err());
        // Below sea level happens; far below does not.
        assert!(new(VerticalReference::MeanSeaLevel, -100.0, HeightUnit::Metres).is_ok());
        assert!(new(VerticalReference::MeanSeaLevel, -5000.0, HeightUnit::Metres).is_err());
        // A flight level is a pressure altitude and nothing else.
        assert_eq!(
            new(
                VerticalReference::AboveGround,
                95.0,
                HeightUnit::FlightLevel
            ),
            Err(AirspaceError::FlightLevelReference(
                VerticalReference::AboveGround
            ))
        );
        // The ground and "unlimited" carry no number.
        let ground = new(VerticalReference::Ground, f64::NAN, HeightUnit::Metres).unwrap();
        assert_eq!(ground, VerticalLimit::ground());
    }

    #[test]
    fn a_limit_is_written_with_its_published_value_and_metres() {
        let limit = feet(VerticalReference::MeanSeaLevel, 9500.0);
        let json = serde_json::to_value(limit).unwrap();
        assert_eq!(json["reference"], "mean-sea-level");
        assert_eq!(json["value"], 9500.0);
        assert_eq!(json["unit"], "feet");
        assert!((json["metres"].as_f64().unwrap() - 2895.6).abs() < 1e-9);
        assert_eq!(
            serde_json::from_value::<VerticalLimit>(json).unwrap(),
            limit
        );

        let ground = serde_json::to_value(VerticalLimit::ground()).unwrap();
        assert_eq!(
            ground,
            serde_json::json!({"reference": "ground", "metres": 0.0})
        );
        let unlimited = serde_json::to_value(VerticalLimit::unlimited()).unwrap();
        assert_eq!(unlimited, serde_json::json!({"reference": "unlimited"}));
        assert_eq!(
            serde_json::from_value::<VerticalLimit>(unlimited).unwrap(),
            VerticalLimit::unlimited()
        );
        // A height that is not one is refused on reading too.
        let bad = serde_json::json!({"reference": "above-ground", "value": -3.0, "unit": "feet"});
        assert!(serde_json::from_value::<VerticalLimit>(bad).is_err());
    }

    #[test]
    fn rings_must_be_closed_and_enclose_an_area() {
        assert!(Polygon::new(square(), Vec::new()).is_ok());

        let mut open = square();
        open.pop();
        assert_eq!(
            Polygon::new(open, Vec::new()),
            Err(AirspaceError::RingNotClosed)
        );

        let short = vec![point(47.0, 11.0), point(47.1, 11.0), point(47.0, 11.0)];
        assert_eq!(
            Polygon::new(short, Vec::new()),
            Err(AirspaceError::RingTooShort(3))
        );

        let line = vec![
            point(47.0, 11.0),
            point(47.1, 11.1),
            point(47.2, 11.2),
            point(47.0, 11.0),
        ];
        assert_eq!(
            Polygon::new(line, Vec::new()),
            Err(AirspaceError::RingWithoutArea)
        );

        let spot = vec![point(47.0, 11.0); 4];
        assert_eq!(
            Polygon::new(spot, Vec::new()),
            Err(AirspaceError::RingWithoutArea)
        );
    }

    #[test]
    fn polygons_of_impossible_size_are_refused() {
        // A ring that wraps around the antimeridian looks 358 degrees wide.
        let wrapped = vec![
            point(10.0, 179.0),
            point(10.0, -179.0),
            point(11.0, -179.0),
            point(11.0, 179.0),
            point(10.0, 179.0),
        ];
        assert!(matches!(
            Polygon::new(wrapped, Vec::new()),
            Err(AirspaceError::TooLarge(_))
        ));

        let hole = vec![
            point(47.02, 11.02),
            point(47.02, 11.04),
            point(47.04, 11.04),
            point(47.02, 11.02),
        ];
        assert!(Polygon::new(square(), vec![hole.clone()]).is_ok());
        assert_eq!(
            Polygon::new(square(), vec![hole; MAX_HOLES + 1]),
            Err(AirspaceError::TooManyHoles(MAX_HOLES + 1))
        );

        let circle = |radius_m| Circle {
            centre: point(47.05, 11.1),
            radius_m,
        };
        let polygon = Polygon::new(square(), Vec::new()).unwrap();
        assert!(polygon.clone().with_circle(circle(5000.0)).is_ok());
        assert!(polygon.clone().with_circle(circle(0.0)).is_err());
        assert!(polygon.with_circle(circle(f64::NAN)).is_err());
    }

    #[test]
    fn a_polygon_knows_its_bounds_and_survives_json() {
        let polygon = Polygon::new(square(), Vec::new())
            .unwrap()
            .with_circle(Circle {
                centre: point(47.05, 11.1),
                radius_m: 9260.0,
            })
            .unwrap();
        let bounds = polygon.bounds();
        assert_eq!(
            (bounds.south(), bounds.west(), bounds.north(), bounds.east()),
            (47.0, 11.0, 47.1, 11.2)
        );

        let json = serde_json::to_value(&polygon).unwrap();
        // Longitude first, as in GeoJSON.
        assert_eq!(json["exterior"][1], serde_json::json!([11.2, 47.0]));
        assert_eq!(serde_json::from_value::<Polygon>(json).unwrap(), polygon);

        // What is read is checked like what is built.
        let open = serde_json::json!({"exterior": [[11.0, 47.0], [11.2, 47.0], [11.2, 47.1], [11.0, 47.1]]});
        assert!(serde_json::from_value::<Polygon>(open).is_err());
        let off_earth = serde_json::json!({"exterior": [[11.0, 97.0], [11.2, 47.0], [11.2, 47.1], [11.0, 97.0]]});
        assert!(serde_json::from_value::<Polygon>(off_earth).is_err());
    }

    #[test]
    fn limits_must_be_the_right_way_round_where_they_can_be_compared() {
        let msl = |value| feet(VerticalReference::MeanSeaLevel, value);
        let agl = |value| feet(VerticalReference::AboveGround, value);
        let fl = |level| VerticalLimit::flight_level(level).unwrap();
        let ground = VerticalLimit::ground();
        let unlimited = VerticalLimit::unlimited();

        assert!(airspace(ground, msl(2500.0)).validate().is_ok());
        assert!(airspace(msl(2500.0), fl(100)).validate().is_ok());
        assert!(airspace(fl(100), unlimited).validate().is_ok());
        assert!(airspace(agl(1000.0), msl(2500.0)).validate().is_ok());
        // Different references cannot be compared without terrain or pressure.
        assert!(airspace(msl(12000.0), fl(100)).validate().is_ok());
        // Equal limits make a sheet with no thickness, which sources do publish.
        assert!(airspace(msl(2500.0), msl(2500.0)).validate().is_ok());

        let inverted = Err(AirspaceError::LimitsInverted);
        assert_eq!(airspace(msl(5000.0), msl(2500.0)).validate(), inverted);
        assert_eq!(airspace(fl(130), fl(100)).validate(), inverted);
        assert_eq!(airspace(agl(2000.0), agl(1000.0)).validate(), inverted);

        let misplaced = Err(AirspaceError::LimitMisplaced);
        assert_eq!(airspace(unlimited, unlimited).validate(), misplaced);
        assert_eq!(airspace(ground, ground).validate(), misplaced);
        assert_eq!(airspace(msl(100.0), ground).validate(), misplaced);
    }

    #[test]
    fn country_and_frequencies_are_checked() {
        let good = airspace(
            VerticalLimit::ground(),
            feet(VerticalReference::MeanSeaLevel, 2500.0),
        );
        assert!(good.validate().is_ok());

        let mut lower_case = good.clone();
        lower_case.country = Some("at".to_owned());
        assert_eq!(
            lower_case.validate(),
            Err(AirspaceError::Country("at".to_owned()))
        );

        let mut no_country = good.clone();
        no_country.country = None;
        assert!(no_country.validate().is_ok());

        let mut bad_radio = good;
        bad_radio.frequencies[0].mhz = f64::NAN;
        assert!(matches!(
            bad_radio.validate(),
            Err(AirspaceError::Frequency(_))
        ));
    }

    #[test]
    fn an_airspace_survives_json_and_names_its_type_in_plain_words() {
        let mut original = airspace(
            VerticalLimit::ground(),
            feet(VerticalReference::MeanSeaLevel, 2500.0),
        );
        original.airspace_type = AirspaceType::Other("SPECIAL".to_owned());
        original.activity.by_notam = true;
        original.activity.hours = vec![OperatingHours {
            day: chrono::Weekday::Sat,
            start: DayTime::Clock(NaiveTime::from_hms_opt(8, 0, 0).unwrap()),
            end: DayTime::Sunset,
            by_notam: false,
            except_public_holidays: true,
            remarks: None,
        }];

        let json = serde_json::to_value(&original).unwrap();
        assert_eq!(json["class"], "D");
        assert_eq!(
            json["airspace_type"],
            serde_json::json!({"other": "SPECIAL"})
        );
        assert_eq!(json["id"], "example:ctr");
        assert_eq!(serde_json::from_value::<Airspace>(json).unwrap(), original);

        let plain = serde_json::to_value(AirspaceType::GliderSector).unwrap();
        assert_eq!(plain, "glider-sector");
        let class = serde_json::to_value(AirspaceClass::Unclassified).unwrap();
        assert_eq!(class, "unclassified");
    }
}
