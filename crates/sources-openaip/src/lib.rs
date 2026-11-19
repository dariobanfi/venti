//! Adapter for the airspace exports of openAIP.
//!
//! openAIP is a community database of aeronautical data. Besides an API
//! that needs a personal key, it publishes its whole dataset as files, one
//! per country and kind, rebuilt every day and readable by anyone without a
//! key. Its documentation (<https://www.openaip.net/docs>, read on
//! 2026-11-19) names the place and the pattern:
//! `https://storage.openaip.net/openaip-system-exports/<country>_<type>.<extension>`.
//! This adapter fetches `<country>_asp.geojson`, the airspace of one
//! country, for every country in its manifest.
//!
//! # Why GeoJSON and not OpenAir
//!
//! openAIP exports airspace as OpenAir files too. On 2026-11-19 the GeoJSON
//! files for Germany and Austria carried everything the OpenAir files did,
//! and more: hours of operation, the flags for activation by NOTAM, on
//! request and on demand, the period a temporary area exists, a second
//! floor ("but at least 1000 ft above ground"), remarks, the time each
//! record was last changed, and a stable identifier per airspace. They
//! give corners in decimal degrees, where the OpenAir files round to whole
//! seconds, about 30 m.
//!
//! # The format
//!
//! One GeoJSON `FeatureCollection`. Each feature is one airspace: a
//! `Polygon` and a set of properties in which types, classes, units and
//! reference levels are numbers. What the numbers mean is taken from
//! openAIP's published schema,
//! <https://api.core.openaip.net/api/schemas/response/airspace/airspace-schema.json>
//! (read on 2026-11-19), and kept in the tables of this file.
//!
//! # What is not known
//!
//! - Whether the hours of operation are in UTC or local time. The schema
//!   does not say. They are passed on as published and nothing computes
//!   with them. To verify.
//! - What tells "on demand" from "on request". Both flags are passed on.
//! - openAIP has no value for "no upper limit" and writes FL 999 where it
//!   means one. It is passed on as FL 999, which is above everything.

pub mod fixtures;

use chrono::{DateTime, NaiveTime, Utc, Weekday};
use serde::Deserialize;
use venti_domain::{
    Activity, Airspace, AirspaceClass, AirspaceId, AirspaceType, DayTime, Frequency, GeoPoint,
    HeightUnit, OperatingHours, Polygon, SourceId, Timestamp, Validity, VerticalLimit,
    VerticalReference,
};
use venti_sources::{
    AirspaceSource, Manifest, ParsedAirspace, PlanCtx, Request, Skipped, SourceError,
};

/// Where openAIP keeps its export files.
const EXPORTS: &str = "https://storage.openaip.net/openaip-system-exports";

/// The openAIP airspace source.
#[derive(Debug, Clone)]
pub struct OpenAip {
    manifest: Manifest,
}

impl OpenAip {
    /// Creates the adapter.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn new() -> Self {
        let manifest = Manifest::from_toml(include_str!("../manifest.toml"))
            .expect("the manifest compiled into venti-sources-openaip is valid");
        Self { manifest }
    }
}

impl Default for OpenAip {
    fn default() -> Self {
        Self::new()
    }
}

/// The address of the airspace file of one country.
fn export_url(country: &str) -> String {
    format!("{EXPORTS}/{}_asp.geojson", country.to_ascii_lowercase())
}

impl AirspaceSource for OpenAip {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// One request per country. The country code is the request's tag.
    fn plan(&self, _ctx: &PlanCtx) -> Vec<Request> {
        self.manifest
            .countries
            .iter()
            .map(|country| Request::new(country.clone(), export_url(country)))
            .collect()
    }

    fn parse(
        &self,
        request: &Request,
        body: &[u8],
        ctx: &PlanCtx,
    ) -> Result<ParsedAirspace, SourceError> {
        if !self.manifest.countries.contains(&request.tag) {
            return Err(SourceError::UnknownRequest(request.tag.clone()));
        }
        let collection: Collection = serde_json::from_slice(body).map_err(|e| {
            SourceError::Malformed(format!("the file is not the expected GeoJSON: {e}"))
        })?;

        let mut parsed = ParsedAirspace::default();
        for feature in collection.features {
            // Each feature is read on its own, so that one that cannot be
            // read costs only itself. Its name is found first, to report it by.
            let name = feature["properties"]["name"]
                .as_str()
                .or_else(|| feature["properties"]["_id"].as_str())
                .unwrap_or("an airspace without a name")
                .to_owned();
            let feature: Feature = match serde_json::from_value(feature) {
                Ok(feature) => feature,
                Err(error) => {
                    parsed.skipped.push(Skipped::new(&name, error.to_string()));
                    continue;
                }
            };
            match airspace(
                &self.manifest.id,
                &request.tag,
                feature,
                ctx.now,
                &mut parsed.skipped,
            ) {
                Ok(airspace) => parsed.airspaces.push(airspace),
                Err(reason) => parsed.skipped.push(Skipped::new(&name, reason)),
            }
        }
        Ok(parsed)
    }
}

#[derive(Deserialize)]
struct Collection {
    features: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct Feature {
    properties: Properties,
    geometry: Geometry,
}

#[derive(Deserialize)]
struct Geometry {
    #[serde(rename = "type")]
    kind: String,
    /// Rings of positions. A position is longitude, latitude and sometimes a height.
    coordinates: Vec<Vec<Vec<f64>>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Properties {
    #[serde(rename = "_id")]
    id: String,
    name: String,
    #[serde(rename = "type")]
    kind: u32,
    icao_class: u32,
    #[serde(default)]
    activity: u32,
    country: Option<String>,
    lower_limit: Limit,
    upper_limit: Limit,
    lower_limit_min: Option<Limit>,
    upper_limit_max: Option<Limit>,
    #[serde(default)]
    on_demand: bool,
    #[serde(default)]
    on_request: bool,
    #[serde(default)]
    by_notam: bool,
    #[serde(default)]
    special_agreement: bool,
    #[serde(default)]
    request_compliance: bool,
    #[serde(default)]
    frequencies: Vec<Radio>,
    hours_of_operation: Option<Hours>,
    active_from: Option<String>,
    active_until: Option<String>,
    remarks: Option<String>,
    updated_at: Option<String>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
struct Limit {
    value: f64,
    unit: u32,
    reference_datum: u32,
}

#[derive(Deserialize)]
struct Radio {
    value: String,
    name: Option<String>,
    #[serde(default)]
    primary: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Hours {
    #[serde(default)]
    operating_hours: Vec<Day>,
    remarks: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Day {
    day_of_week: u32,
    start_time: Option<String>,
    end_time: Option<String>,
    #[serde(default)]
    sunrise: bool,
    #[serde(default)]
    sunset: bool,
    #[serde(default)]
    by_notam: bool,
    #[serde(default)]
    public_holidays_excluded: bool,
    remarks: Option<String>,
}

/// openAIP's type numbers. A number that is not in its schema is kept as
/// an unknown type, which the warning table treats with caution.
fn airspace_type(code: u32) -> AirspaceType {
    match code {
        0 => AirspaceType::Unspecified,
        1 => AirspaceType::Restricted,
        2 => AirspaceType::Danger,
        3 => AirspaceType::Prohibited,
        4 => AirspaceType::Ctr,
        5 => AirspaceType::Tmz,
        6 => AirspaceType::Rmz,
        7 => AirspaceType::Tma,
        8 => AirspaceType::Tra,
        9 => AirspaceType::Tsa,
        10 => AirspaceType::Fir,
        11 => AirspaceType::Uir,
        12 => AirspaceType::Adiz,
        13 => AirspaceType::Atz,
        14 => AirspaceType::Matz,
        15 => AirspaceType::Airway,
        16 => AirspaceType::MilitaryTrainingRoute,
        17 => AirspaceType::Alert,
        18 => AirspaceType::Warning,
        19 => AirspaceType::Protected,
        20 => AirspaceType::Htz,
        21 => AirspaceType::GliderSector,
        22 => AirspaceType::TransponderSetting,
        23 => AirspaceType::Tiz,
        24 => AirspaceType::Tia,
        25 => AirspaceType::MilitaryTrainingArea,
        26 => AirspaceType::Cta,
        27 => AirspaceType::AccSector,
        28 => AirspaceType::AerialSporting,
        29 => AirspaceType::OverflightRestriction,
        30 => AirspaceType::MilitaryRoute,
        31 => AirspaceType::FeedingRoute,
        32 => AirspaceType::VfrSector,
        33 => AirspaceType::FisSector,
        34 => AirspaceType::Lta,
        35 => AirspaceType::Uta,
        36 => AirspaceType::MilitaryCtr,
        other => AirspaceType::Other(format!("openAIP type {other}")),
    }
}

/// openAIP's class numbers. A number that is not in its schema counts as
/// no class, which never makes an airspace look more open than it is.
fn airspace_class(code: u32) -> AirspaceClass {
    match code {
        0 => AirspaceClass::A,
        1 => AirspaceClass::B,
        2 => AirspaceClass::C,
        3 => AirspaceClass::D,
        4 => AirspaceClass::E,
        5 => AirspaceClass::F,
        6 => AirspaceClass::G,
        _ => AirspaceClass::Unclassified,
    }
}

/// What a sporting area is used for, in openAIP's numbering.
fn sport(code: u32) -> Option<&'static str> {
    match code {
        1 => Some("Parachuting."),
        2 => Some("Aerobatics."),
        3 => Some("Aero club and aerial work."),
        4 => Some("Ultralight flying."),
        5 => Some("Hang gliding and paragliding."),
        _ => None,
    }
}

/// Reads a limit. Units are 0 for metres, 1 for feet and 6 for flight
/// levels; reference levels are 0 for the ground, 1 for mean sea level and
/// 2 for standard pressure.
fn limit(limit: Limit) -> Option<VerticalLimit> {
    let unit = match limit.unit {
        0 => HeightUnit::Metres,
        1 => HeightUnit::Feet,
        6 => HeightUnit::FlightLevel,
        _ => return None,
    };
    let reference = match (limit.reference_datum, unit) {
        (0, HeightUnit::FlightLevel) => return None,
        (0, _) if limit.value == 0.0 => VerticalReference::Ground,
        (0, _) => VerticalReference::AboveGround,
        // A flight level is one whatever reference is written beside it.
        (1 | 2, HeightUnit::FlightLevel) | (2, _) => VerticalReference::Standard,
        (1, _) => VerticalReference::MeanSeaLevel,
        _ => return None,
    };
    VerticalLimit::new(reference, limit.value, unit).ok()
}

fn ring(positions: &[Vec<f64>]) -> Result<Vec<GeoPoint>, String> {
    let mut ring = Vec::with_capacity(positions.len() + 1);
    for position in positions {
        let &[lon, lat, ..] = position.as_slice() else {
            return Err("a corner has fewer than two coordinates".to_owned());
        };
        ring.push(GeoPoint::new(lat, lon).map_err(|e| e.to_string())?);
    }
    // GeoJSON asks for closed rings; close one that is not.
    if let Some(&first) = ring.first()
        && ring.last() != Some(&first)
    {
        ring.push(first);
    }
    Ok(ring)
}

fn time(text: &str) -> Option<Timestamp> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

/// The hours of operation, or nothing if they say nothing.
///
/// Nearly every record carries the same seven entries, midnight to midnight
/// on every day with no flag set: openAIP's way of writing "no hours given".
/// Those are left out. So are hours of which any entry cannot be read:
/// half a timetable is worse than none, and without one the airspace counts
/// as active at any time.
fn hours(days: &[Day]) -> Vec<OperatingHours> {
    let says_nothing = |day: &Day| {
        let midnight = |time: &Option<String>| time.as_deref().is_none_or(|time| time == "00:00");
        midnight(&day.start_time)
            && midnight(&day.end_time)
            && !(day.sunrise || day.sunset || day.by_notam || day.public_holidays_excluded)
            && day.remarks.is_none()
    };
    if days.iter().all(says_nothing) {
        return Vec::new();
    }
    let clock = |time: &Option<String>| {
        let text = time.as_deref().unwrap_or("00:00");
        NaiveTime::parse_from_str(text, "%H:%M")
            .or_else(|_| NaiveTime::parse_from_str(text, "%H:%M:%S"))
            .ok()
            .map(DayTime::Clock)
    };
    let weekday = |day: u32| -> Option<Weekday> {
        // Monday is 0.
        Weekday::try_from(u8::try_from(day).ok()?).ok()
    };
    days.iter()
        .map(|day| {
            Some(OperatingHours {
                day: weekday(day.day_of_week)?,
                start: if day.sunrise {
                    DayTime::Sunrise
                } else {
                    clock(&day.start_time)?
                },
                end: if day.sunset {
                    DayTime::Sunset
                } else {
                    clock(&day.end_time)?
                },
                by_notam: day.by_notam,
                except_public_holidays: day.public_holidays_excluded,
                remarks: day.remarks.clone().filter(|text| !text.trim().is_empty()),
            })
        })
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default()
}

/// Converts one feature. `Err` when it must be left out; notes about what
/// was assumed go to `notes`.
fn airspace(
    source: &SourceId,
    file_country: &str,
    feature: Feature,
    fetched: Timestamp,
    notes: &mut Vec<Skipped>,
) -> Result<Airspace, String> {
    let properties = feature.properties;
    let geometry = feature.geometry;
    if geometry.kind != "Polygon" {
        return Err(format!("the shape is a {}, not a Polygon", geometry.kind));
    }
    let mut rings = geometry.coordinates.iter().map(|positions| ring(positions));
    let exterior = rings.next().ok_or("the shape has no ring")??;
    let holes = rings.collect::<Result<Vec<_>, _>>()?;
    let geometry = Polygon::new(exterior, holes).map_err(|e| e.to_string())?;

    // A limit that cannot be read is replaced by the widest one, so that
    // the airspace is kept and is never smaller than published.
    let mut assume = |what: &str, assumed: &str| {
        notes.push(Skipped::new(
            &properties.name,
            format!("the {what} cannot be read; {assumed} is assumed"),
        ));
    };
    let lower = limit(properties.lower_limit).unwrap_or_else(|| {
        assume("floor", "the ground");
        VerticalLimit::ground()
    });
    let upper = limit(properties.upper_limit).unwrap_or_else(|| {
        assume("ceiling", "no upper limit");
        VerticalLimit::unlimited()
    });
    // A second floor only ever raises the floor: one that cannot be read is left out.
    let lower_at_least = properties.lower_limit_min.and_then(limit);
    let upper_at_most = properties.upper_limit_max.map(|raised| {
        limit(raised).unwrap_or_else(|| {
            assume("height the ceiling may be raised to", "no upper limit");
            VerticalLimit::unlimited()
        })
    });

    let country = properties
        .country
        .filter(|code| code.len() == 2 && code.bytes().all(|b| b.is_ascii_uppercase()))
        .unwrap_or_else(|| file_country.to_owned());

    let (hours, hours_remarks) = match properties.hours_of_operation {
        Some(published) => (
            hours(&published.operating_hours),
            published.remarks.filter(|text| !text.trim().is_empty()),
        ),
        None => (Vec::new(), None),
    };

    let mut remarks = Vec::new();
    remarks.extend(sport(properties.activity).map(str::to_owned));
    if properties.request_compliance {
        remarks.push("Not an official airspace: an authority asks pilots to comply.".to_owned());
    }
    remarks.extend(properties.remarks.filter(|text| !text.trim().is_empty()));

    let frequencies = properties
        .frequencies
        .into_iter()
        .filter_map(|radio| {
            Some(Frequency {
                mhz: radio.value.trim().parse().ok()?,
                name: radio.name.filter(|name| !name.trim().is_empty()),
                primary: radio.primary,
            })
        })
        .collect();

    Ok(Airspace {
        id: AirspaceId::new(source.clone(), properties.id).map_err(|e| e.to_string())?,
        name: properties.name,
        class: airspace_class(properties.icao_class),
        airspace_type: airspace_type(properties.kind),
        country: Some(country),
        lower,
        upper,
        lower_at_least,
        upper_at_most,
        geometry,
        activity: Activity {
            by_notam: properties.by_notam,
            on_request: properties.on_request,
            on_demand: properties.on_demand,
            special_agreement: properties.special_agreement,
            hours,
            hours_remarks,
            from: properties.active_from.as_deref().and_then(time),
            until: properties.active_until.as_deref().and_then(time),
        },
        frequencies,
        remarks: (!remarks.is_empty()).then(|| remarks.join(" ")),
        validity: Validity {
            // openAIP is revised continuously and names no AIRAC cycle.
            airac: None,
            // The files are rebuilt daily and carry no date of their own.
            file_date: Some(fetched),
            updated: properties.updated_at.as_deref().and_then(time),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feet(reference: VerticalReference, value: f64) -> VerticalLimit {
        VerticalLimit::new(reference, value, HeightUnit::Feet).unwrap()
    }

    fn published(value: f64, unit: u32, reference_datum: u32) -> Limit {
        Limit {
            value,
            unit,
            reference_datum,
        }
    }

    #[test]
    fn limits_are_read_from_their_three_numbers() {
        assert_eq!(limit(published(0.0, 1, 0)), Some(VerticalLimit::ground()));
        assert_eq!(
            limit(published(1000.0, 1, 0)),
            Some(feet(VerticalReference::AboveGround, 1000.0))
        );
        assert_eq!(
            limit(published(7000.0, 1, 1)),
            Some(feet(VerticalReference::MeanSeaLevel, 7000.0))
        );
        assert_eq!(
            limit(published(125.0, 6, 2)),
            Some(VerticalLimit::flight_level(125).unwrap())
        );
        // A flight level filed under mean sea level is still a flight level.
        assert_eq!(
            limit(published(125.0, 6, 1)),
            Some(VerticalLimit::flight_level(125).unwrap())
        );
        let metres = limit(published(1500.0, 0, 1)).unwrap();
        assert_eq!(metres.unit(), HeightUnit::Metres);
        assert_eq!(metres.metres(), Some(1500.0));
        // openAIP's "no upper limit".
        assert_eq!(limit(published(999.0, 6, 2)).unwrap().value(), 999.0);

        // Units and reference levels outside the schema, and nonsense.
        assert_eq!(limit(published(1000.0, 7, 1)), None);
        assert_eq!(limit(published(1000.0, 1, 3)), None);
        assert_eq!(limit(published(95.0, 6, 0)), None);
        assert_eq!(limit(published(f64::NAN, 1, 1)), None);
        assert_eq!(limit(published(-50.0, 1, 0)), None);
    }

    #[test]
    fn every_type_and_class_of_the_schema_has_a_meaning() {
        for code in 0..=36 {
            assert!(
                !matches!(airspace_type(code), AirspaceType::Other(_)),
                "type {code}"
            );
        }
        assert_eq!(
            airspace_type(37),
            AirspaceType::Other("openAIP type 37".to_owned())
        );
        assert_eq!(airspace_class(3), AirspaceClass::D);
        assert_eq!(airspace_class(8), AirspaceClass::Unclassified);
        // 7 is in no schema: no class, not an open one.
        assert_eq!(airspace_class(7), AirspaceClass::Unclassified);
    }

    fn day(day_of_week: u32, start: &str, end: &str) -> Day {
        Day {
            day_of_week,
            start_time: Some(start.to_owned()),
            end_time: Some(end.to_owned()),
            sunrise: false,
            sunset: false,
            by_notam: false,
            public_holidays_excluded: false,
            remarks: None,
        }
    }

    #[test]
    fn hours_that_say_nothing_are_left_out_and_real_ones_are_kept() {
        let always: Vec<Day> = (0..7).map(|d| day(d, "00:00", "00:00")).collect();
        assert!(hours(&always).is_empty());
        assert!(hours(&[]).is_empty());

        let mut weekdays: Vec<Day> = (0..5).map(|d| day(d, "08:00", "16:30")).collect();
        weekdays[4].sunset = true;
        weekdays[4].public_holidays_excluded = true;
        let read = hours(&weekdays);
        assert_eq!(read.len(), 5);
        assert_eq!(read[0].day, Weekday::Mon);
        assert_eq!(
            read[0].start,
            DayTime::Clock(NaiveTime::from_hms_opt(8, 0, 0).unwrap())
        );
        assert_eq!(read[4].day, Weekday::Fri);
        assert_eq!(read[4].end, DayTime::Sunset);
        assert!(read[4].except_public_holidays);

        // One entry that cannot be read, and the timetable is dropped.
        weekdays[2].start_time = Some("early".to_owned());
        assert!(hours(&weekdays).is_empty());
        weekdays[2] = day(9, "08:00", "16:30");
        assert!(hours(&weekdays).is_empty());
    }

    #[test]
    fn the_addresses_follow_the_documented_pattern() {
        assert_eq!(
            export_url("DE"),
            "https://storage.openaip.net/openaip-system-exports/de_asp.geojson"
        );
        let source = OpenAip::new();
        let ctx = PlanCtx {
            now: fixtures::recorded_at(),
            area: venti_domain::BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap(),
        };
        let requests = source.plan(&ctx);
        let tags: Vec<&str> = requests.iter().map(|r| r.tag.as_str()).collect();
        assert_eq!(tags, ["DE", "AT"]);
        assert!(requests[1].url.ends_with("/at_asp.geojson"));
    }
}
