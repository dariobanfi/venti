use std::sync::{Arc, LazyLock};

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use chrono::NaiveDate;
use serde::Deserialize;
use venti_api_types::v1::{
    Attribution, ErrorCode, RegionFeature, RegionForecastResponse, RegionGeometry, RegionNames,
    RegionProperties, RegionsResponse, RouteWeatherResponse, ThermalHoursResponse, ThermalLayer,
    ThermalMapResponse, format_day, parse_day, parse_route_points,
};
use venti_sources::Kind;
use venti_store::Store;

use super::error::ApiError;
use super::{Api, ServedSource, cache};
use crate::thermals::{MAP_ABOVE_GROUND_M, MAP_HEIGHTS_M, ThermalMap};

const DATE_HELP: &str = "give the day as date=YYYY-MM-DD, one of the days /v1/thermals lists";

#[derive(Debug, Deserialize)]
pub(crate) struct HoursParams {
    date: Option<String>,
    layer: Option<String>,
    height: Option<String>,
    above_ground: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RegionForecastParams {
    date: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RouteParams {
    date: Option<String>,
    points: Option<String>,
}

/// After how many seconds a client that was told the map is not there yet
/// should ask again. Making the map takes a minute or two after a start.
const COME_BACK_S: u64 = 60;

/// `GET /v1/thermals`: the server's thermal map, the kind of day cell by
/// cell over the area it has forecasts for.
pub(crate) async fn thermals<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let answer = api.thermal_map()?;
    Ok(cache::json(&headers, &answer))
}

/// `GET /v1/thermals/hours?date=&layer=&height=&above_ground=`: one layer
/// of one day of the thermal map hour by hour: the hours' verdicts and
/// thermals, or the model's wind, cloud or rain in each.
pub(crate) async fn thermal_hours<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
    Query(params): Query<HoursParams>,
) -> Result<Response, ApiError> {
    let date = day(params.date.as_deref())?;
    // Without a layer, the hours' verdicts: what the map of the days is
    // made of.
    let layer = match params.layer.as_deref() {
        None => ThermalLayer::Thermals,
        Some(text) => ThermalLayer::parse(text).ok_or_else(|| {
            let layers: Vec<&str> = ThermalLayer::ALL
                .iter()
                .map(|layer| layer.as_str())
                .collect();
            ApiError::bad_request(
                ErrorCode::InvalidRequest,
                format!("layer is one of {}", layers.join(", ")),
            )
        })?,
    };
    let height_m = match params.height.as_deref() {
        None => None,
        Some(_) if layer != ThermalLayer::Wind => {
            return Err(ApiError::bad_request(
                ErrorCode::InvalidRequest,
                "height goes with layer=wind: the other layers have no height",
            ));
        }
        Some(text) => Some(
            text.parse::<i32>()
                .ok()
                .filter(|asked| MAP_HEIGHTS_M.iter().any(|kept| i32::from(*kept) == *asked))
                .ok_or_else(|| {
                    let heights: Vec<String> =
                        MAP_HEIGHTS_M.iter().map(ToString::to_string).collect();
                    ApiError::bad_request(
                        ErrorCode::InvalidRequest,
                        format!(
                            "height is one of {} (metres above mean sea level), or left out \
                             for the wind 10 m above the model's ground",
                            heights.join(", ")
                        ),
                    )
                })?,
        ),
    };
    let above_ground_m = match params.above_ground.as_deref() {
        None => None,
        Some(_) if layer != ThermalLayer::Wind || height_m.is_some() => {
            return Err(ApiError::bad_request(
                ErrorCode::InvalidRequest,
                "above_ground goes with layer=wind and without height: it is the wind at a \
                 height above the model's ground in place of one above the sea",
            ));
        }
        Some(text) => Some(
            text.parse::<i32>()
                .ok()
                .filter(|asked| {
                    MAP_ABOVE_GROUND_M
                        .iter()
                        .any(|kept| i32::from(*kept) == *asked)
                })
                .ok_or_else(|| {
                    let heights: Vec<String> =
                        MAP_ABOVE_GROUND_M.iter().map(ToString::to_string).collect();
                    ApiError::bad_request(
                        ErrorCode::InvalidRequest,
                        format!(
                            "above_ground is one of {} (metres above the model's ground), or \
                             left out for the wind 10 m above it",
                            heights.join(", ")
                        ),
                    )
                })?,
        ),
    };
    let answer = api.thermal_hours(date, layer, height_m, above_ground_m)?;
    Ok(cache::json(&headers, &answer))
}

/// `GET /v1/route/weather?date=&points=`: for each point of a route and
/// each daylight hour of a day of the thermal map, what the map holds for
/// the cell the point lies in. A `GET` and not a `POST` with a body: the
/// answer is the same for everyone who asks for the same points, so it is
/// cached and validated as the map's own answers are, and twenty-five
/// points are some 500 characters of a query string.
pub(crate) async fn route_weather<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
    Query(params): Query<RouteParams>,
) -> Result<Response, ApiError> {
    let date = day(params.date.as_deref())?;
    let points = parse_route_points(params.points.as_deref().unwrap_or_default())
        .map_err(|error| ApiError::bad_request(ErrorCode::InvalidPosition, error.to_string()))?;
    let answer = api.route_weather(date, &points)?;
    Ok(cache::json(&headers, &answer))
}

/// The credit for the regions of the forecast by region, from the
/// manifest of `venti-sources-eawsregions`.
fn regions_credit() -> Attribution {
    ServedSource::from_manifest(&venti_sources_eawsregions::manifest()).attribution
}

/// The answer of `GET /v1/regions`, written once: some 700 kB that change
/// only with the crate's asset.
static REGIONS_BODY: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let features = venti_sources_eawsregions::regions()
        .iter()
        .map(|region| RegionFeature {
            kind: "Feature".to_owned(),
            properties: RegionProperties {
                id: region.id.clone(),
                names: RegionNames {
                    de: region.names.de.clone(),
                    it: region.names.it.clone(),
                    en: region.names.en.clone(),
                    fr: region.names.fr.clone(),
                    sl: region.names.sl.clone(),
                },
                label: [region.label.0, region.label.1],
            },
            geometry: RegionGeometry {
                kind: "MultiPolygon".to_owned(),
                coordinates: region
                    .polygons
                    .iter()
                    .map(|polygon| {
                        polygon
                            .iter()
                            .map(|ring| ring.iter().map(|&(lon, lat)| [lon, lat]).collect())
                            .collect()
                    })
                    .collect(),
            },
        })
        .collect();
    let answer = RegionsResponse {
        kind: "FeatureCollection".to_owned(),
        version: venti_sources_eawsregions::version().to_owned(),
        features,
        attributions: vec![regions_credit()],
    };
    serde_json::to_vec(&answer).unwrap_or_default()
});

/// `GET /v1/regions`: the regions of the forecast by region, as a GeoJSON
/// collection of their outlines, names and labels.
pub(crate) async fn regions(headers: HeaderMap) -> Response {
    cache::slow_json_body(&headers, REGIONS_BODY.clone())
}

/// `GET /v1/regions/forecast?date=`: one day of the thermal map read
/// region by region, for the day and for each of its hours.
pub(crate) async fn region_forecast<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
    Query(params): Query<RegionForecastParams>,
) -> Result<Response, ApiError> {
    let date = match params.date.as_deref() {
        None => None,
        text => Some(day(text)?),
    };
    let answer = api.region_forecast(date)?;
    Ok(cache::json(&headers, &answer))
}

/// The `date` of a request, as a day the map names.
fn day(text: Option<&str>) -> Result<NaiveDate, ApiError> {
    text.ok_or_else(|| {
        ApiError::bad_request(
            ErrorCode::InvalidDay,
            format!("date is missing; {DATE_HELP}"),
        )
    })
    .and_then(|text| {
        parse_day(text).map_err(|error| {
            ApiError::bad_request(ErrorCode::InvalidDay, format!("date: {error}; {DATE_HELP}"))
        })
    })
}

impl<S: Store> Api<S> {
    /// The thermal map, as the API answers it, with its source's credit.
    ///
    /// A server without a forecast service has no map and says so as for a
    /// forecast. One that has a service and no map yet, after a start or
    /// while its source does not answer, says when to ask again.
    pub(crate) fn thermal_map(&self) -> Result<ThermalMapResponse, ApiError> {
        let (_, made) = self.made_thermal_map()?;
        let mut answer = ThermalMapResponse::clone(&made);
        answer.attributions = self.attributions([&answer.source]);
        Ok(answer)
    }

    /// One layer of the hours of one day of the thermal map, as the API
    /// answers it, with the source's credit. Refused as the map itself is
    /// while there is none; a day the map does not have is not found.
    /// `height_m` is one of the heights the map keeps, or none, and
    /// `above_ground_m` one of those above the ground, or none.
    pub(crate) fn thermal_hours(
        &self,
        date: NaiveDate,
        layer: ThermalLayer,
        height_m: Option<i32>,
        above_ground_m: Option<i32>,
    ) -> Result<ThermalHoursResponse, ApiError> {
        let (map, made) = self.made_thermal_map()?;
        let mut answer = map
            .hours_of(date)
            .and_then(|day| day.layer(layer, height_m, above_ground_m))
            .ok_or_else(|| no_such_day(date, &made))?;
        answer.attributions = self.attributions([&answer.source]);
        Ok(answer)
    }

    /// The day of the thermal map at each point of a route, as the API
    /// answers it, with the source's credit. Refused as the map itself is
    /// while there is none; a day the map does not have is not found.
    pub(crate) fn route_weather(
        &self,
        date: NaiveDate,
        points: &[venti_domain::GeoPoint],
    ) -> Result<RouteWeatherResponse, ApiError> {
        let (map, made) = self.made_thermal_map()?;
        let mut answer = map
            .hours_of(date)
            .map(|day| day.along(points))
            .ok_or_else(|| no_such_day(date, &made))?;
        answer.attributions = self.attributions([&answer.source]);
        Ok(answer)
    }

    /// The forecast by region of one day, as the API answers it, with the
    /// credit of the forecast source and of the regions. Refused as the
    /// map itself is while there is none; a day the map does not have is
    /// not found. `None` asks for today, or the map's first day.
    pub(crate) fn region_forecast(
        &self,
        date: Option<NaiveDate>,
    ) -> Result<RegionForecastResponse, ApiError> {
        let (map, made) = self.made_thermal_map()?;
        let mut answer = map.region_forecast(date).ok_or_else(|| {
            let asked = date.unwrap_or_else(|| made.computed_at.date_naive());
            no_such_day(asked, &made)
        })?;
        answer.attributions = self.attributions([&answer.source]);
        answer.attributions.push(regions_credit());
        Ok(answer)
    }

    /// The server's thermal map and the days it serves, or why there is
    /// none.
    fn made_thermal_map(&self) -> Result<(&Arc<ThermalMap>, Arc<ThermalMapResponse>), ApiError> {
        let serves_forecasts = self
            .sources
            .values()
            .any(|served| served.kinds.contains(&Kind::Forecasts));
        if self.forecasts.is_none() || !serves_forecasts {
            return Err(ApiError::new(
                StatusCode::NOT_IMPLEMENTED,
                ErrorCode::NoForecasts,
                "this server has no forecast service, so it has no thermal map; \
                 /v1/health says the same under \"forecasts\"",
            ));
        }
        self.thermal_map
            .as_ref()
            .filter(|map| self.serves(map.source()))
            .and_then(|map| Some((map, map.current()?)))
            .ok_or_else(|| {
                ApiError::busy(
                    "the thermal map is not made yet: the server makes it from each run of \
                     the model, and after a start that takes a few minutes",
                )
                .retry_after(COME_BACK_S)
            })
    }
}

/// A day the thermal map does not have, and the days it has.
fn no_such_day(date: NaiveDate, made: &ThermalMapResponse) -> ApiError {
    let has: Vec<String> = made.days.iter().map(|day| format_day(day.date)).collect();
    ApiError::new(
        StatusCode::NOT_FOUND,
        ErrorCode::NotFound,
        format!(
            "the thermal map has no hours for {}; it has {}",
            format_day(date),
            has.join(", ")
        ),
    )
}
