//! The filters of a search among public flights beyond the box, the days
//! and the site: a circle around a point, an area drawn on the map, the
//! glider's class, the kind of route, the least distance and the take-off's
//! country. Every one of them is about where and how a flight was flown,
//! and narrows; none widens.
//!
//! Both sides read and write them here, the client to build a request and
//! the server to read it, so there is one definition of the format and of
//! the bounds.

pub use venti_domain::Area;
use venti_domain::{AreaError, GeoError, GeoPoint};

use super::{GliderClass, RouteKind};

/// The smallest radius of a circle a search asks for, kilometres.
pub const MIN_RADIUS_KM: f64 = 1.0;

/// The largest radius of a circle a search asks for, kilometres: the
/// Alps from end to end are some 1,200 km, and a search wider than a
/// region is asked by box.
pub const MAX_RADIUS_KM: f64 = 100.0;

/// The most corners of an area drawn on the map.
pub const MAX_AREA_POINTS: usize = Area::MAX_CORNERS;

/// The longest `area` parameter the server reads: fifty corners, each two
/// numbers of up to five decimals and their signs and commas.
pub const MAX_AREA_LENGTH: usize = 1100;

/// The largest least distance a search asks for, kilometres: more than
/// the longest paraglider flight flown.
pub const MAX_MIN_DISTANCE_KM: f64 = 1000.0;

/// Flights that took off within a distance of a point, that far included.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Circle {
    /// The point.
    pub centre: GeoPoint,
    /// How far from it, kilometres, from [`MIN_RADIUS_KM`] to
    /// [`MAX_RADIUS_KM`].
    pub radius_km: f64,
}

/// Why a filter of a search was refused.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SearchError {
    /// `near` was sent without `radius_km`, or the other way round.
    #[error("near and radius_km go together: near=lat,lon&radius_km=10")]
    Half,
    /// `near` is not two numbers.
    #[error("near must be lat,lon in degrees, for example near=47.2,11.4")]
    Near,
    /// A number is not one, or not a place on the globe.
    #[error("not a position: {0}")]
    Geo(#[from] GeoError),
    /// The radius is not a number of kilometres in bounds.
    #[error("radius_km must be a number from {MIN_RADIUS_KM} to {MAX_RADIUS_KM}")]
    Radius,
    /// The area is not an even list of numbers, or too long.
    #[error(
        "area must be lat,lon,lat,lon,… in degrees, three to {MAX_AREA_POINTS} corners, at most {MAX_AREA_LENGTH} characters"
    )]
    AreaFormat,
    /// The corners are no area.
    #[error("area: {0}")]
    Area(#[from] AreaError),
    /// `class` names no glider class.
    #[error("class must be one of en_a, en_b, en_c, en_d, ccc")]
    Class,
    /// `kind` names no kind of route.
    #[error("kind must be one of free_flight, free_triangle, fai_triangle")]
    Kind,
    /// `min_km` is not a distance in bounds.
    #[error("min_km must be a number of kilometres from 0 to {MAX_MIN_DISTANCE_KM}")]
    MinDistance,
    /// `country` is not two letters.
    #[error("country must be two letters, an ISO 3166-1 alpha-2 code such as AT")]
    Country,
}

impl Circle {
    /// A circle, refused when its radius is out of bounds.
    pub fn new(centre: GeoPoint, radius_km: f64) -> Result<Self, SearchError> {
        if radius_km.is_finite() && (MIN_RADIUS_KM..=MAX_RADIUS_KM).contains(&radius_km) {
            Ok(Self { centre, radius_km })
        } else {
            Err(SearchError::Radius)
        }
    }

    /// The radius in metres.
    pub fn radius_m(&self) -> f64 {
        self.radius_km * 1000.0
    }
}

/// The filters beyond the box, the days and the site, which the list of
/// public flights, a pilot's public flights and the ranking take alike.
/// Each that is set narrows the flights to those that took off or were
/// flown so; the default narrows nothing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlightFilters {
    /// Only flights that took off within this circle (`near`, `radius_km`).
    pub near: Option<Circle>,
    /// Only flights that took off inside this area (`area`).
    pub area: Option<Area>,
    /// Only flights flown on a glider of this class (`class`).
    pub class: Option<GliderClass>,
    /// Only flights scored as this kind of route (`kind`).
    pub kind: Option<RouteKind>,
    /// Only flights whose scoring distance is at least this many
    /// kilometres (`min_km`). A flight too short to score has none.
    pub min_km: Option<f64>,
    /// Only flights that took off in this country, an ISO 3166-1 alpha-2
    /// code in upper case (`country`).
    pub country: Option<String>,
}

impl FlightFilters {
    /// The parameters of the query that are numbers and commas only, in
    /// the order they are written: written with their commas, as a box is.
    pub fn plain_parameters(&self) -> [(&'static str, Option<String>); 2] {
        [
            ("near", self.near.as_ref().map(format_near)),
            ("area", self.area.as_ref().map(format_area)),
        ]
    }

    /// The other parameters of the query, in the order they are written.
    pub fn parameters(&self) -> [(&'static str, Option<String>); 5] {
        [
            (
                "radius_km",
                self.near.map(|circle| circle.radius_km.to_string()),
            ),
            (
                "class",
                self.class.and_then(glider_class_name).map(str::to_owned),
            ),
            (
                "kind",
                self.kind.and_then(route_kind_name).map(str::to_owned),
            ),
            ("min_km", self.min_km.map(|km| km.to_string())),
            ("country", self.country.clone()),
        ]
    }

    /// Reads them from the parameters as they were sent.
    pub fn parse(sent: &SentFilters<'_>) -> Result<Self, SearchError> {
        Ok(Self {
            near: parse_circle(sent.near, sent.radius_km)?,
            area: sent.area.map(parse_area).transpose()?,
            class: sent.class.map(parse_glider_class).transpose()?,
            kind: sent.kind.map(parse_route_kind).transpose()?,
            min_km: sent.min_km.map(parse_min_distance).transpose()?,
            country: sent.country.map(parse_country).transpose()?,
        })
    }

    /// Whether none is set.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The text of the filters' parameters as a request sent them.
#[derive(Debug, Clone, Copy, Default)]
pub struct SentFilters<'a> {
    /// `near`.
    pub near: Option<&'a str>,
    /// `radius_km`.
    pub radius_km: Option<&'a str>,
    /// `area`.
    pub area: Option<&'a str>,
    /// `class`.
    pub class: Option<&'a str>,
    /// `kind`.
    pub kind: Option<&'a str>,
    /// `min_km`.
    pub min_km: Option<&'a str>,
    /// `country`.
    pub country: Option<&'a str>,
}

/// Writes a point as `near` takes it: `lat,lon`, to a metre or so.
pub fn format_near(circle: &Circle) -> String {
    format!(
        "{},{}",
        rounded(circle.centre.lat()),
        rounded(circle.centre.lon())
    )
}

/// Degrees to five decimals, about a metre, without trailing zeros.
fn rounded(degrees: f64) -> f64 {
    (degrees * 100_000.0).round() / 100_000.0
}

/// Reads the `near` and `radius_km` parameters. `None` when neither was
/// sent.
pub fn parse_circle(
    near: Option<&str>,
    radius_km: Option<&str>,
) -> Result<Option<Circle>, SearchError> {
    let (near, radius) = match (near, radius_km) {
        (None, None) => return Ok(None),
        (Some(near), Some(radius)) => (near, radius),
        _ => return Err(SearchError::Half),
    };
    let numbers = numbers(near).ok_or(SearchError::Near)?;
    let [lat, lon] = numbers[..] else {
        return Err(SearchError::Near);
    };
    let radius: f64 = radius.trim().parse().map_err(|_| SearchError::Radius)?;
    Circle::new(GeoPoint::new(lat, lon)?, radius).map(Some)
}

/// Numbers separated by commas, `None` if one is not a number.
fn numbers(text: &str) -> Option<Vec<f64>> {
    text.split(',')
        .map(|part| part.trim().parse::<f64>().ok().filter(|n| n.is_finite()))
        .collect()
}

/// Writes an area as `area` takes it: `lat,lon,lat,lon,…`, each to five
/// decimals.
pub fn format_area(area: &Area) -> String {
    area.corners()
        .iter()
        .map(|c| format!("{},{}", rounded(c.lat()), rounded(c.lon())))
        .collect::<Vec<_>>()
        .join(",")
}

/// Reads the `area` parameter.
pub fn parse_area(text: &str) -> Result<Area, SearchError> {
    if text.len() > MAX_AREA_LENGTH {
        return Err(SearchError::AreaFormat);
    }
    let numbers = numbers(text).ok_or(SearchError::AreaFormat)?;
    if numbers.len() % 2 != 0 {
        return Err(SearchError::AreaFormat);
    }
    let corners = numbers
        .chunks_exact(2)
        .map(|pair| GeoPoint::new(pair[0], pair[1]))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Area::new(corners)?)
}

/// The name of a glider class in a query, as in the answers.
pub fn glider_class_name(class: GliderClass) -> Option<&'static str> {
    Some(match class {
        GliderClass::EnA => "en_a",
        GliderClass::EnB => "en_b",
        GliderClass::EnC => "en_c",
        GliderClass::EnD => "en_d",
        GliderClass::Ccc => "ccc",
        GliderClass::Unknown => return None,
    })
}

/// Reads the `class` parameter.
pub fn parse_glider_class(text: &str) -> Result<GliderClass, SearchError> {
    [
        GliderClass::EnA,
        GliderClass::EnB,
        GliderClass::EnC,
        GliderClass::EnD,
        GliderClass::Ccc,
    ]
    .into_iter()
    .find(|class| glider_class_name(*class) == Some(text))
    .ok_or(SearchError::Class)
}

/// The name of a kind of route in a query, as in the answers.
pub fn route_kind_name(kind: RouteKind) -> Option<&'static str> {
    Some(match kind {
        RouteKind::FreeFlight => "free_flight",
        RouteKind::FreeTriangle => "free_triangle",
        RouteKind::FaiTriangle => "fai_triangle",
        RouteKind::Unknown => return None,
    })
}

/// Reads the `kind` parameter.
pub fn parse_route_kind(text: &str) -> Result<RouteKind, SearchError> {
    [
        RouteKind::FreeFlight,
        RouteKind::FreeTriangle,
        RouteKind::FaiTriangle,
    ]
    .into_iter()
    .find(|kind| route_kind_name(*kind) == Some(text))
    .ok_or(SearchError::Kind)
}

/// Reads the `min_km` parameter.
pub fn parse_min_distance(text: &str) -> Result<f64, SearchError> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|km| km.is_finite() && (0.0..=MAX_MIN_DISTANCE_KM).contains(km))
        .ok_or(SearchError::MinDistance)
}

/// Reads the `country` parameter: two letters, given back in upper case.
pub fn parse_country(text: &str) -> Result<String, SearchError> {
    super::country_code(text).ok_or(SearchError::Country)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).unwrap()
    }

    #[test]
    fn a_circle_is_a_point_and_a_radius_in_bounds() {
        let circle = parse_circle(Some("47.2654,11.3933"), Some("10"))
            .unwrap()
            .unwrap();
        assert_eq!(circle.centre, at(47.2654, 11.3933));
        assert_eq!(circle.radius_m(), 10_000.0);
        assert_eq!(format_near(&circle), "47.2654,11.3933");
        assert_eq!(parse_circle(None, None).unwrap(), None);
        for (near, radius, error) in [
            (Some("47,11"), None, SearchError::Half),
            (None, Some("5"), SearchError::Half),
            (Some("47"), Some("5"), SearchError::Near),
            (Some("47,11,3"), Some("5"), SearchError::Near),
            (Some("north,11"), Some("5"), SearchError::Near),
            (Some("47,11"), Some("0.5"), SearchError::Radius),
            (Some("47,11"), Some("100.1"), SearchError::Radius),
            (Some("47,11"), Some("NaN"), SearchError::Radius),
        ] {
            assert_eq!(
                parse_circle(near, radius),
                Err(error),
                "{near:?} {radius:?}"
            );
        }
        assert!(matches!(
            parse_circle(Some("91,11"), Some("5")),
            Err(SearchError::Geo(_))
        ));
    }

    #[test]
    fn an_area_is_a_simple_polygon_of_three_to_fifty_corners() {
        // Around Innsbruck, drawn clockwise.
        let area = parse_area("47.3,11.3,47.3,11.5,47.2,11.5,47.2,11.3").unwrap();
        assert_eq!(area.corners().len(), 4);
        assert!(area.holds(at(47.25, 11.4)));
        assert!(area.holds(at(47.3, 11.4)), "an edge is inside");
        assert!(!area.holds(at(47.35, 11.4)));
        assert_eq!(
            format_area(&area),
            "47.3,11.3,47.3,11.5,47.2,11.5,47.2,11.3"
        );
        assert_eq!(parse_area(&format_area(&area)).unwrap(), area);
        assert_eq!(
            area.wkt(),
            "POLYGON((11.3 47.3,11.5 47.3,11.5 47.2,11.3 47.2,11.3 47.3))"
        );
        assert_eq!(area.bounds(), [11.3, 47.2, 11.5, 47.3]);
        // Closed by its first corner: the same area.
        assert_eq!(
            parse_area("47.3,11.3,47.3,11.5,47.2,11.5,47.2,11.3,47.3,11.3").unwrap(),
            area
        );

        // A bow tie crosses itself.
        assert_eq!(
            parse_area("47.3,11.3,47.2,11.5,47.3,11.5,47.2,11.3"),
            Err(SearchError::Area(AreaError::NotSimple))
        );
        // Three corners on a line have no surface.
        assert_eq!(
            parse_area("47.1,11.1,47.2,11.2,47.3,11.3"),
            Err(SearchError::Area(AreaError::NotSimple))
        );
        assert_eq!(
            parse_area("47.1,11.1,47.2,11.2"),
            Err(SearchError::Area(AreaError::Corners(2)))
        );
        assert_eq!(parse_area("47.1,11.1,47.2"), Err(SearchError::AreaFormat));
        assert_eq!(parse_area("47.1,11.1,47.2,x"), Err(SearchError::AreaFormat));
        assert_eq!(
            parse_area("10,-100,10,100,-10,0"),
            Err(SearchError::Area(AreaError::TooWide))
        );
        let many: Vec<String> = (0..51)
            .map(|i| {
                let angle = f64::from(i) / 51.0 * std::f64::consts::TAU;
                format!(
                    "{:.4},{:.4}",
                    47.0 + angle.sin() * 0.1,
                    11.0 + angle.cos() * 0.1
                )
            })
            .collect();
        assert_eq!(
            parse_area(&many.join(",")),
            Err(SearchError::Area(AreaError::Corners(51)))
        );
        assert_eq!(
            parse_area(&many[..50].join(",")).unwrap().corners().len(),
            50
        );
        assert_eq!(parse_area(&"1".repeat(1200)), Err(SearchError::AreaFormat));
    }

    #[test]
    fn the_other_filters_read_their_names_and_bounds() {
        assert_eq!(parse_glider_class("en_c"), Ok(GliderClass::EnC));
        assert_eq!(parse_glider_class("EN C"), Err(SearchError::Class));
        assert_eq!(parse_route_kind("fai_triangle"), Ok(RouteKind::FaiTriangle));
        assert_eq!(parse_route_kind("triangle"), Err(SearchError::Kind));
        assert_eq!(parse_min_distance("50"), Ok(50.0));
        assert_eq!(parse_min_distance("-1"), Err(SearchError::MinDistance));
        assert_eq!(parse_min_distance("1e9"), Err(SearchError::MinDistance));
        assert_eq!(parse_country("at"), Ok("AT".to_owned()));
        assert_eq!(parse_country("AUT"), Err(SearchError::Country));
    }
}
