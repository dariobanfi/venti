//! A search among public flights as XContest offers one: the take-off
//! within a radius of a point or inside an area drawn on the map, the
//! glider's class, the kind of route, the least distance and the take-off's
//! country, beside the box, the days and the order the lists had before.
//!
//! The shells hold what the pilot chose in a [`FlightSearch`] and ask the
//! core for the path to fetch and for the words of the chips that show the
//! choice. The bounds are the server's ([`flight_search_bounds`]): a shell
//! limits its radius and the corners of an area to them.

use venti_api_types::v1::{
    self as api, Area, Circle, FlightFilters, FlightSort, MAX_AREA_POINTS, MAX_MIN_DISTANCE_KM,
    MAX_RADIUS_KM, MIN_RADIUS_KM, PublicFlightsRequest, RankingRequest,
};
use venti_domain::GeoPoint;

use crate::community::{PublicFlightsOrder, Which, which};
use crate::{Bounds, Coordinate, CoreError, GliderClassKind, RouteKind};

/// What a pilot chose to search public flights by. The default chooses
/// nothing: every public flight.
#[derive(Debug, Clone, PartialEq, Default, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase", default))]
pub struct FlightSearch {
    /// The centre of a circle the take-off lies in, with `radius_km`.
    #[uniffi(default = None)]
    pub near: Option<Coordinate>,
    /// The radius of that circle, kilometres, from
    /// [`FlightSearchBounds::min_radius_km`] to its `max_radius_km`.
    #[uniffi(default = None)]
    pub radius_km: Option<f64>,
    /// The corners of an area drawn on the map the take-off lies in, in
    /// the order they were drawn; empty for none. Three to
    /// [`FlightSearchBounds::max_area_points`], edges that do not cross.
    #[uniffi(default = [])]
    pub area: Vec<Coordinate>,
    /// Only flights flown on a glider of this class.
    #[uniffi(default = None)]
    pub glider_class: Option<GliderClassKind>,
    /// Only flights scored as this kind of route.
    #[uniffi(default = None)]
    pub kind: Option<RouteKind>,
    /// Only flights of at least this scoring distance, kilometres.
    #[uniffi(default = None)]
    pub min_km: Option<f64>,
    /// Only flights that took off in this country, an ISO 3166-1 alpha-2
    /// code.
    #[uniffi(default = None)]
    pub country: Option<String>,
}

/// The bounds of a search, which the server holds requests to.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FlightSearchBounds {
    /// The smallest radius, kilometres.
    pub min_radius_km: f64,
    /// The largest radius, kilometres.
    pub max_radius_km: f64,
    /// The most corners of an area.
    pub max_area_points: u32,
    /// The largest least distance, kilometres.
    pub max_min_km: f64,
}

/// The bounds a shell holds its choices to.
#[uniffi::export]
pub fn flight_search_bounds() -> FlightSearchBounds {
    FlightSearchBounds {
        min_radius_km: MIN_RADIUS_KM,
        max_radius_km: MAX_RADIUS_KM,
        max_area_points: u32::try_from(MAX_AREA_POINTS).unwrap_or(u32::MAX),
        max_min_km: MAX_MIN_DISTANCE_KM,
    }
}

fn refused(words: &str) -> CoreError {
    CoreError::Server {
        message: words.to_owned(),
    }
}

fn point(coordinate: Coordinate) -> Result<GeoPoint, CoreError> {
    GeoPoint::new(coordinate.lat, coordinate.lon)
        .map_err(|_| refused("This place is not on the map."))
}

/// The search in the server's terms, or why it cannot be asked: in words
/// for the pilot.
fn filters(search: &FlightSearch) -> Result<FlightFilters, CoreError> {
    let near = match (search.near, search.radius_km) {
        (None, None) => None,
        (Some(near), Some(radius_km)) => {
            Some(Circle::new(point(near)?, radius_km).map_err(|_| {
                refused(&format!(
                    "Choose a radius from {MIN_RADIUS_KM:.0} to {MAX_RADIUS_KM:.0} km."
                ))
            })?)
        }
        _ => return Err(refused("Choose a place and a radius.")),
    };
    let area = if search.area.is_empty() {
        None
    } else {
        let corners = search
            .area
            .iter()
            .map(|corner| point(*corner))
            .collect::<Result<Vec<_>, _>>()?;
        Some(Area::new(corners).map_err(|_| {
            refused(&format!(
                "Draw an area of 3 to {MAX_AREA_POINTS} corners whose edges do not cross."
            ))
        })?)
    };
    let min_km = search
        .min_km
        .map(|km| {
            api::parse_min_distance(&km.to_string()).map_err(|_| {
                refused(&format!(
                    "Choose a distance from 0 to {MAX_MIN_DISTANCE_KM:.0} km."
                ))
            })
        })
        .transpose()?;
    let country = search
        .country
        .as_deref()
        .filter(|country| !country.trim().is_empty())
        .map(|country| api::parse_country(country).map_err(|_| refused("This is no country.")))
        .transpose()?;
    Ok(FlightFilters {
        near,
        area,
        class: search.glider_class.map(|class| match class {
            GliderClassKind::A => api::GliderClass::EnA,
            GliderClassKind::B => api::GliderClass::EnB,
            GliderClassKind::C => api::GliderClass::EnC,
            GliderClassKind::D => api::GliderClass::EnD,
            GliderClassKind::Ccc => api::GliderClass::Ccc,
        }),
        kind: search.kind.map(|kind| match kind {
            RouteKind::FreeFlight => api::RouteKind::FreeFlight,
            RouteKind::FreeTriangle => api::RouteKind::FreeTriangle,
            RouteKind::FaiTriangle => api::RouteKind::FaiTriangle,
        }),
        min_km,
        country,
    })
}

fn sort(order: PublicFlightsOrder) -> Option<FlightSort> {
    match order {
        // The server's own order: nothing to say.
        PublicFlightsOrder::Newest => None,
        PublicFlightsOrder::Best => Some(FlightSort::Score),
    }
}

/// The path and query to `GET` for one page of everyone's public flights
/// that a search finds: as [`crate::public_flights_path`], narrowed by
/// `search`. The answer is read by [`crate::public_flights`].
#[uniffi::export]
pub fn public_flights_search_path(
    order: PublicFlightsOrder,
    area: Option<Bounds>,
    from_unix_s: Option<i64>,
    to_unix_s: Option<i64>,
    search: FlightSearch,
    after: Option<String>,
) -> Result<String, CoreError> {
    let Which { bbox, from, to } = which(
        area,
        from_unix_s,
        to_unix_s,
        "The app could not ask the server for these flights.",
    )?;
    Ok(PublicFlightsRequest {
        bbox,
        from,
        to,
        filters: filters(&search)?,
        sort: sort(order),
        after: after.filter(|after| !after.is_empty()),
        ..PublicFlightsRequest::default()
    }
    .path_and_query())
}

/// The same for one pilot's public flights.
#[uniffi::export]
pub fn pilot_flights_search_path(
    handle: String,
    order: PublicFlightsOrder,
    from_unix_s: Option<i64>,
    to_unix_s: Option<i64>,
    search: FlightSearch,
    after: Option<String>,
) -> Result<String, CoreError> {
    let handle: api::Handle = handle
        .parse()
        .map_err(|_| refused("This is not the handle of a pilot."))?;
    let Which { from, to, .. } = which(
        None,
        from_unix_s,
        to_unix_s,
        "The app could not ask the server for these flights.",
    )?;
    Ok(PublicFlightsRequest {
        from,
        to,
        filters: filters(&search)?,
        sort: sort(order),
        after: after.filter(|after| !after.is_empty()),
        ..PublicFlightsRequest::default()
    }
    .path_and_query_of(&handle))
}

/// The path and query to `GET` for the ranking of the flights a search
/// finds: as [`crate::ranking_path`], narrowed by `search`. With
/// `pilot_country`, an ISO 3166-1 alpha-2 code, only the pilots of that
/// country are ranked (a national ranking; the take-off's country is
/// `search.country`). The answer is read by [`crate::ranking`].
#[uniffi::export(default(pilot_country = None))]
pub fn ranking_search_path(
    area: Option<Bounds>,
    from_unix_s: Option<i64>,
    to_unix_s: Option<i64>,
    search: FlightSearch,
    pilot_country: Option<String>,
) -> Result<String, CoreError> {
    let asked = "The app could not ask the server for this ranking.";
    let Which { bbox, from, to } = which(area, from_unix_s, to_unix_s, asked)?;
    let pilot_country = pilot_country
        .filter(|country| !country.is_empty())
        .map(|country| api::parse_country(&country))
        .transpose()
        .map_err(|_| refused("This is not the code of a country."))?;
    Ok(RankingRequest {
        bbox,
        from,
        to,
        filters: filters(&search)?,
        pilot_country,
        ..RankingRequest::default()
    }
    .path_and_query())
}

/// The words of the chips that show a search, one a choice, in the order
/// a filter bar shows them: "Within 10 km", "Drawn area", "EN C",
/// "FAI triangle", "50 km or more", "AT". Empty for a search of
/// everything.
#[uniffi::export]
pub fn flight_search_words(search: FlightSearch) -> Vec<String> {
    let mut words = Vec::new();
    if let Some(radius) = search.radius_km.filter(|_| search.near.is_some()) {
        words.push(format!("Within {} km", kilometres(radius)));
    }
    if !search.area.is_empty() {
        words.push("Drawn area".to_owned());
    }
    if let Some(class) = search.glider_class {
        words.push(crate::glider_class_name(Some(class)));
    }
    if let Some(kind) = search.kind {
        words.push(route_kind_words(kind));
    }
    if let Some(km) = search.min_km {
        words.push(format!("{} km or more", kilometres(km)));
    }
    if let Some(country) = search.country.filter(|country| !country.trim().is_empty()) {
        words.push(country.trim().to_uppercase());
    }
    words
}

/// The name of a kind of route in a choice of them: "Free flight",
/// "Flat triangle", "FAI triangle".
#[uniffi::export]
pub fn route_kind_words(kind: RouteKind) -> String {
    match kind {
        RouteKind::FreeFlight => "Free flight",
        RouteKind::FreeTriangle => "Flat triangle",
        RouteKind::FaiTriangle => "FAI triangle",
    }
    .to_owned()
}

/// Kilometres as a chip says them: whole, or to one decimal below ten.
fn kilometres(km: f64) -> String {
    if km < 10.0 && km.fract().abs() > f64::EPSILON {
        format!("{km:.1}")
    } else {
        format!("{km:.0}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(lat: f64, lon: f64) -> Coordinate {
        Coordinate { lat, lon }
    }

    fn innsbruck() -> FlightSearch {
        FlightSearch {
            near: Some(at(47.2654, 11.3933)),
            radius_km: Some(10.0),
            ..FlightSearch::default()
        }
    }

    #[test]
    fn a_search_is_asked_in_the_servers_words() {
        assert_eq!(
            public_flights_search_path(
                PublicFlightsOrder::Newest,
                None,
                None,
                None,
                FlightSearch::default(),
                None
            )
            .unwrap(),
            "/v1/flights/public"
        );
        assert_eq!(
            public_flights_search_path(
                PublicFlightsOrder::Best,
                None,
                None,
                None,
                innsbruck(),
                Some("p11.98_0123456789abcdefghjkmnpqrs".to_owned())
            )
            .unwrap(),
            "/v1/flights/public?near=47.2654,11.3933&radius_km=10&sort=score\
             &after=p11.98_0123456789abcdefghjkmnpqrs"
        );
        let everything = FlightSearch {
            near: None,
            radius_km: None,
            area: vec![at(47.3, 11.3), at(47.3, 11.5), at(47.2, 11.5)],
            glider_class: Some(GliderClassKind::C),
            kind: Some(RouteKind::FaiTriangle),
            min_km: Some(50.0),
            country: Some("at".to_owned()),
        };
        assert_eq!(
            public_flights_search_path(
                PublicFlightsOrder::Newest,
                None,
                None,
                None,
                everything.clone(),
                None
            )
            .unwrap(),
            "/v1/flights/public?area=47.3,11.3,47.3,11.5,47.2,11.5&class=en_c\
             &kind=fai_triangle&min_km=50&country=AT"
        );
        assert_eq!(
            pilot_flights_search_path(
                "anna".to_owned(),
                PublicFlightsOrder::Best,
                None,
                None,
                innsbruck(),
                None
            )
            .unwrap(),
            "/v1/flights/public?near=47.2654,11.3933&radius_km=10&sort=score&pilot=anna"
        );
        assert_eq!(
            ranking_search_path(None, None, None, everything.clone(), None).unwrap(),
            "/v1/ranking?area=47.3,11.3,47.3,11.5,47.2,11.5&class=en_c&kind=fai_triangle\
             &min_km=50&country=AT"
        );
        assert_eq!(
            ranking_search_path(
                None,
                None,
                None,
                FlightSearch::default(),
                Some("it".to_owned())
            )
            .unwrap(),
            "/v1/ranking?pilot_country=IT"
        );
        assert!(
            ranking_search_path(
                None,
                None,
                None,
                FlightSearch::default(),
                Some("ITA".to_owned())
            )
            .is_err()
        );
        assert_eq!(
            flight_search_words(everything),
            ["Drawn area", "EN C", "FAI triangle", "50 km or more", "AT"]
        );
        assert_eq!(flight_search_words(innsbruck()), ["Within 10 km"]);
    }

    #[test]
    fn a_search_out_of_bounds_is_refused_in_words() {
        let refusal = |search: FlightSearch| match public_flights_search_path(
            PublicFlightsOrder::Newest,
            None,
            None,
            None,
            search,
            None,
        ) {
            Err(CoreError::Server { message }) => message,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            refusal(FlightSearch {
                radius_km: Some(500.0),
                ..innsbruck()
            }),
            "Choose a radius from 1 to 100 km."
        );
        assert_eq!(
            refusal(FlightSearch {
                radius_km: None,
                ..innsbruck()
            }),
            "Choose a place and a radius."
        );
        // A bow tie.
        assert_eq!(
            refusal(FlightSearch {
                area: vec![
                    at(47.3, 11.3),
                    at(47.2, 11.5),
                    at(47.3, 11.5),
                    at(47.2, 11.3)
                ],
                ..FlightSearch::default()
            }),
            "Draw an area of 3 to 50 corners whose edges do not cross."
        );
        assert_eq!(
            refusal(FlightSearch {
                country: Some("Austria".to_owned()),
                ..FlightSearch::default()
            }),
            "This is no country."
        );
        assert_eq!(flight_search_bounds().max_radius_km, 100.0);
        assert_eq!(flight_search_bounds().max_area_points, 50);
    }
}
