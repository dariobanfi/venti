//! `GET /v1/foehn`: the pressure difference across the Alps' main ridge for
//! each pair of stations, and what it says about foehn.
//!
//! Answered from the store: each station's readings of the last 24 hours,
//! its pressure at sea level in each (the provider's own, or reduced here
//! from the pressure at the station), and the rule of the flight engine
//! ([`venti_flight::foehn`]) over the newest two. A pair with a missing
//! reading, or one older than an hour, has the level `unknown` and says
//! why; that is never "no foehn". Public, like the stations themselves.

use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Response;
use chrono::{DateTime, Duration};
use venti_api_types::v1::{
    FoehnLevel, FoehnPair, FoehnPoint, FoehnPressure, FoehnReducedBy, FoehnResponse, FoehnSide,
    FoehnStation, FoehnUnknown,
};
use venti_domain::{GeoPoint, StationId, Timestamp};
use venti_flight::foehn::{
    self, Judgement, Level, PAIRS, Pair, PairStation, SeaLevelPressure, Side, StationReading,
    Unknown,
};
use venti_store::Store;

use super::error::ApiError;
use super::{Api, cache};

/// How far back a pair's series goes.
const SERIES_HOURS: i64 = 24;

/// What the reading does not say. Shown wherever a level is shown.
pub(crate) const CAUTION: &str = "A pressure difference across the main ridge is a sign of foehn, \
     not a measurement of it. Foehn can blow with a small difference, and with a large one it \
     can stay above a valley's cold air for hours and then break through within minutes. It \
     says nothing about one valley or one take-off: read the wind stations, the forecast and \
     the sky, and when in doubt do not fly.";

/// `GET /v1/foehn`.
pub(crate) async fn foehn<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let answer = api.foehn().await?;
    Ok(cache::json(&headers, &answer))
}

impl<S: Store> Api<S> {
    /// The foehn reading, as the API answers it. The connector's foehn tool
    /// is answered from this too.
    pub(crate) async fn foehn(&self) -> Result<FoehnResponse, ApiError> {
        let now = self.clock.now();
        let mut pairs = Vec::with_capacity(PAIRS.len());
        let mut sources = Vec::new();
        for pair in &PAIRS {
            let (south, south_series) = self.foehn_station(&pair.south, now).await?;
            let (north, north_series) = self.foehn_station(&pair.north, now).await?;
            for station in [&south, &north] {
                if station.pressure.is_some() {
                    sources.push(station.source.clone());
                }
            }
            pairs.push(judged(
                pair,
                south,
                north,
                &south_series,
                &north_series,
                now,
            ));
        }
        Ok(FoehnResponse {
            time: now,
            possible_from_hpa: foehn::POSSIBLE_HPA,
            likely_from_hpa: foehn::LIKELY_HPA,
            fresh_for_s: foehn::FRESH_S,
            pairs,
            caution: CAUTION.to_owned(),
            attributions: self.attributions(&sources),
        })
    }

    /// A station of a pair and its pressures at sea level of the last 24
    /// hours, oldest first. A station the server does not collect, or does
    /// not know, is a station without a pressure.
    async fn foehn_station(
        &self,
        station: &PairStation,
        now: Timestamp,
    ) -> Result<(FoehnStation, Vec<SeaLevelPressure>), ApiError> {
        // The pairs are fixed in the flight engine, and its tests and this
        // crate's read every one of them.
        let id: StationId = station.id.parse().map_err(|_| ApiError::internal())?;
        let position = GeoPoint::new(station.lat, station.lon).map_err(|_| ApiError::internal())?;
        let mut elevation_m = None;
        let mut series = Vec::new();
        if self.serves(id.source())
            && let Some(known) = self.store.station(&id).await?
        {
            elevation_m = known.elevation_m;
            let from = now - Duration::hours(SERIES_HOURS);
            series = self
                .store
                .observations(&id, from, now)
                .await?
                .iter()
                .filter_map(|observation| {
                    foehn::at_sea_level(
                        &StationReading {
                            time_unix_s: observation.time.timestamp(),
                            pressure_hpa: observation.pressure_hpa,
                            pressure_sea_level_hpa: observation.pressure_sea_level_hpa,
                            temperature_c: observation.temperature_c,
                        },
                        elevation_m,
                    )
                })
                .collect();
        }
        let pressure = series.last().and_then(|newest| {
            Some(FoehnPressure {
                time: time(newest.time_unix_s)?,
                sea_level_hpa: tenth(newest.hpa),
                reduced_by: if newest.reduced_here {
                    FoehnReducedBy::Venti
                } else {
                    FoehnReducedBy::Provider
                },
            })
        });
        Ok((
            FoehnStation {
                source: id.source().clone(),
                id,
                name: station.name.to_owned(),
                position,
                elevation_m,
                pressure,
            },
            series,
        ))
    }
}

/// A pair as the API answers it: the rule over the newest two readings,
/// and the series.
fn judged(
    pair: &Pair,
    south: FoehnStation,
    north: FoehnStation,
    south_series: &[SeaLevelPressure],
    north_series: &[SeaLevelPressure],
    now: Timestamp,
) -> FoehnPair {
    let judgement = foehn::judge(south_series.last(), north_series.last(), now.timestamp());
    let (level, unknown_because, difference, higher) = match judgement {
        Judgement::Known {
            difference,
            higher,
            level,
        } => (wire_level(level), None, Some(difference), higher),
        Judgement::Unknown(why) => (FoehnLevel::Unknown, Some(wire_unknown(why)), None, None),
    };
    let higher = higher.map(wire_side);
    FoehnPair {
        id: pair.id.to_owned(),
        south,
        north,
        level,
        unknown_because,
        difference_hpa: difference.map(|difference| difference.hpa),
        difference_time: difference.and_then(|difference| time(difference.time_unix_s)),
        higher,
        foehn: higher.filter(|_| matches!(level, FoehnLevel::Possible | FoehnLevel::Likely)),
        series: foehn::series(south_series, north_series)
            .into_iter()
            .filter_map(|point| {
                Some(FoehnPoint {
                    time: time(point.time_unix_s)?,
                    difference_hpa: point.hpa,
                })
            })
            .collect(),
        to_verify: pair.to_verify,
        basis: pair.basis.to_owned(),
    }
}

fn time(unix_s: i64) -> Option<Timestamp> {
    DateTime::from_timestamp(unix_s, 0)
}

fn tenth(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn wire_level(level: Level) -> FoehnLevel {
    match level {
        Level::None => FoehnLevel::None,
        Level::Possible => FoehnLevel::Possible,
        Level::Likely => FoehnLevel::Likely,
    }
}

fn wire_side(side: Side) -> FoehnSide {
    match side {
        Side::South => FoehnSide::South,
        Side::North => FoehnSide::North,
    }
}

fn wire_unknown(why: Unknown) -> FoehnUnknown {
    match why {
        Unknown::Missing(Side::South) => FoehnUnknown::SouthMissing,
        Unknown::Missing(Side::North) => FoehnUnknown::NorthMissing,
        Unknown::Old(Side::South) => FoehnUnknown::SouthOld,
        Unknown::Old(Side::North) => FoehnUnknown::NorthOld,
        Unknown::Apart => FoehnUnknown::Apart,
    }
}
