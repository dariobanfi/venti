//! The wind the cockpit's thermal view shows, with where it comes from:
//! one rule for the three apps.
//!
//! The flight's own wind, once the session has measured one, from its
//! circles or a turn on a glide. Before that, a wind station near the
//! glider at about its height; else the forecast's wind at the level
//! nearest the glider's altitude for the hour now; else the station the
//! core puts first for the landing or the glider. The shell hands the
//! session the forecast ([`crate::FlightSession::use_forecast`]) and the
//! stations ([`crate::FlightSession::use_ground_wind`]); it keeps no rule
//! of its own.
//!
//! The distances and heights are the project's own (*to verify* in the
//! air): a station counts as the glider's air within 15 km and 300 m of
//! its altitude, and a forecast hour within 90 minutes of now.

use crate::{ForecastView, GroundWindStation};

/// How far a station may stand from the glider and still speak for its
/// air, metres.
const STATION_NEAR_M: f64 = 15_000.0;

/// How far a station's height may lie from the glider's altitude and
/// still speak for its air, metres.
const STATION_AS_HIGH_M: f64 = 300.0;

/// How far a forecast hour may lie from now and still be the hour's wind,
/// seconds.
const FORECAST_HOUR_WITHIN_S: f64 = 90.0 * 60.0;

/// From this age a measured wind says how old it is, seconds.
const SAY_AGE_FROM_S: f64 = 5.0 * 60.0;

/// Where the wind the thermal view shows comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CockpitWindSource {
    /// The flight's own, from its circles or a turn on a glide.
    Measured,
    /// A wind station's reading.
    Station,
    /// The forecast's, at the level nearest the glider.
    Forecast,
}

/// The wind the thermal view shows, with where it comes from.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CockpitWindView {
    /// The direction it comes from, degrees clockwise from north.
    pub from_deg: f64,
    /// Its speed, metres per second.
    pub speed_ms: f64,
    /// The speed in whole km/h, for the number beside the arrow: "14".
    pub speed_kmh: String,
    /// Where it comes from.
    pub source: CockpitWindSource,
    /// The same in words, under the number: "measured", "measured 7 min
    /// ago", "forecast", or the station's name, "Wallberg".
    pub source_words: String,
}

/// The wind to show: the measured one (`measured`, with how old it is),
/// else the stations' and the forecast's by the rule of this module.
pub(crate) fn wind_shown(
    measured: Option<(f64, f64, f64)>,
    altitude_m: Option<f64>,
    stations: &[GroundWindStation],
    forecast: Option<&ForecastView>,
    now_unix_s: f64,
) -> Option<CockpitWindView> {
    if let Some((from_deg, speed_ms, age_s)) = measured {
        let words = if age_s < SAY_AGE_FROM_S {
            "measured".to_owned()
        } else {
            let minutes = (age_s / 60.0).floor() as i64;
            if minutes < 60 {
                format!("measured {minutes} min ago")
            } else {
                format!("measured {} h ago", minutes / 60)
            }
        };
        return Some(view(from_deg, speed_ms, CockpitWindSource::Measured, words));
    }
    let station_view = |station: &GroundWindStation| {
        station.from_deg.map(|from| {
            view(
                from,
                station.speed_ms,
                CockpitWindSource::Station,
                station.name.clone(),
            )
        })
    };
    // A station in the glider's own air: near, and about as high.
    let as_high = altitude_m.and_then(|altitude| {
        stations
            .iter()
            .filter(|station| station.from_deg.is_some())
            .filter(|station| station.distance_m <= STATION_NEAR_M)
            .filter(|station| {
                station
                    .elevation_m
                    .is_some_and(|height| (height - altitude).abs() <= STATION_AS_HIGH_M)
            })
            .min_by(|one, other| one.distance_m.total_cmp(&other.distance_m))
    });
    if let Some(station) = as_high {
        return station_view(station);
    }
    if let Some(wind) =
        forecast.and_then(|forecast| forecast_wind(forecast, altitude_m, now_unix_s))
    {
        return Some(wind);
    }
    // The station the core puts first: the landing's, else the nearest.
    stations.iter().find_map(station_view)
}

/// The forecast's wind for this moment at the level nearest the glider:
/// the finest model with an hour within [`FORECAST_HOUR_WITHIN_S`], its
/// hour nearest now, and of the wind 10 m above the model's ground and
/// its levels aloft the one nearest the altitude; without an altitude the
/// wind at 10 m.
fn forecast_wind(
    forecast: &ForecastView,
    altitude_m: Option<f64>,
    now_unix_s: f64,
) -> Option<CockpitWindView> {
    forecast.models.iter().find_map(|model| {
        let hour = model
            .days
            .iter()
            .flat_map(|day| &day.hours)
            .min_by(|one, other| {
                (one.time_unix_s as f64 - now_unix_s)
                    .abs()
                    .total_cmp(&(other.time_unix_s as f64 - now_unix_s).abs())
            })
            .filter(|hour| {
                (hour.time_unix_s as f64 - now_unix_s).abs() <= FORECAST_HOUR_WITHIN_S
            })?;
        // Each wind with the height it is at, where it has a direction.
        let ground = hour
            .wind
            .as_ref()
            .map(|wind| (model.cell_elevation_m.map(|ground| ground + 10.0), wind));
        let aloft = model
            .levels
            .iter()
            .zip(&hour.aloft)
            .filter_map(|(level, wind)| Some((Some(level.about_altitude_m?), wind.as_ref()?)));
        let candidates: Vec<_> = ground
            .into_iter()
            .chain(aloft)
            .filter(|(_, wind)| wind.from_deg.is_some())
            .collect();
        let chosen = match altitude_m {
            Some(altitude) => candidates.iter().min_by(|one, other| {
                let off =
                    |height: Option<f64>| height.map_or(f64::INFINITY, |h| (h - altitude).abs());
                off(one.0).total_cmp(&off(other.0))
            }),
            None => candidates.first(),
        }?;
        Some(view(
            chosen.1.from_deg?,
            chosen.1.speed_ms,
            CockpitWindSource::Forecast,
            "forecast".to_owned(),
        ))
    })
}

fn view(
    from_deg: f64,
    speed_ms: f64,
    source: CockpitWindSource,
    source_words: String,
) -> CockpitWindView {
    CockpitWindView {
        from_deg,
        speed_ms,
        speed_kmh: format!("{:.0}", speed_ms * 3.6),
        source,
        source_words,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Coordinate, ForecastDay, ForecastHourView, ForecastLevel, ForecastWind, ModelForecast,
        WindBand,
    };

    const NOW: f64 = 1_783_857_600.0;

    fn station(name: &str, distance_m: f64, elevation_m: f64, from_deg: f64) -> GroundWindStation {
        GroundWindStation {
            id: format!("test:{name}"),
            name: name.to_owned(),
            distance_m,
            bearing_deg: 0.0,
            elevation_m: Some(elevation_m),
            from_deg: Some(from_deg),
            speed_ms: 5.0,
            gust_ms: None,
            age_min: 6,
            place: String::new(),
            wind: String::new(),
            age: String::new(),
            badge: String::new(),
            is_at_landing: false,
            is_strong: false,
        }
    }

    fn wind(from_deg: f64, speed_ms: f64) -> Option<ForecastWind> {
        Some(ForecastWind {
            speed_ms,
            from_deg: Some(from_deg),
            band: WindBand::Light,
            label: String::new(),
        })
    }

    /// A forecast of one model, ground at 800 m, levels at about 1500 and
    /// 3000 m, with hours at `NOW` and two hours later.
    fn forecast() -> ForecastView {
        let hour = |time_unix_s: i64, offset: f64| ForecastHourView {
            time_unix_s,
            is_daylight: true,
            wind: wind(270.0 + offset, 3.0),
            aloft: vec![wind(300.0 + offset, 6.0), wind(320.0 + offset, 10.0)],
            temperature_c: None,
            cloud_pct: None,
            cloud_low_pct: None,
            cloud_mid_pct: None,
            cloud_high_pct: None,
            precipitation_mm: None,
            cloud_base_m: None,
            freezing_level_m: None,
            cape_j_kg: None,
            thermal_climb_ms: None,
            thermal_top_m: None,
            thermal_cloud: false,
        };
        let level = |name: &str, about: f64| ForecastLevel {
            name: name.to_owned(),
            about_altitude_m: Some(about),
        };
        ForecastView {
            position: Coordinate {
                lat: 47.0,
                lon: 11.0,
            },
            fetched_unix_s: NOW as i64,
            models: vec![ModelForecast {
                model: "icon-d2".to_owned(),
                name: "ICON-D2".to_owned(),
                grid: String::new(),
                source: "open-meteo".to_owned(),
                run_unix_s: NOW as i64,
                cell: Coordinate {
                    lat: 47.0,
                    lon: 11.0,
                },
                cell_distance_m: 0.0,
                cell_elevation_m: Some(800.0),
                levels: vec![level("850 hPa", 1500.0), level("700 hPa", 3000.0)],
                days: vec![ForecastDay {
                    start_unix_s: NOW as i64 - 12 * 3600,
                    date: "2026-07-12".to_owned(),
                    sunrise_unix_s: None,
                    sunset_unix_s: None,
                    hours: vec![hour(NOW as i64, 0.0), hour(NOW as i64 + 7200, 5.0)],
                }],
            }],
            credits: Vec::new(),
        }
    }

    fn said(wind: Option<CockpitWindView>) -> Option<(f64, CockpitWindSource, String)> {
        wind.map(|wind| (wind.from_deg, wind.source, wind.source_words))
    }

    #[test]
    fn the_measured_wind_comes_first_and_says_its_age_from_five_minutes() {
        let stations = [station("Wallberg", 2000.0, 1600.0, 90.0)];
        let fresh = wind_shown(
            Some((250.0, 4.0, 120.0)),
            Some(1600.0),
            &stations,
            Some(&forecast()),
            NOW,
        )
        .unwrap();
        assert_eq!(fresh.source, CockpitWindSource::Measured);
        assert_eq!(fresh.source_words, "measured");
        assert_eq!(fresh.speed_kmh, "14");
        let older = |age_s: f64| {
            wind_shown(Some((250.0, 4.0, age_s)), None, &[], None, NOW)
                .unwrap()
                .source_words
        };
        assert_eq!(older(7.0 * 60.0 + 30.0), "measured 7 min ago");
        assert_eq!(older(2.0 * 3600.0 + 60.0), "measured 2 h ago");
    }

    #[test]
    fn before_the_first_circle_a_station_in_the_gliders_air_then_the_forecast() {
        // A station near and about as high speaks for the glider's air.
        let stations = [
            station("Valley", 3000.0, 700.0, 10.0),
            station("Wallberg", 6000.0, 1620.0, 90.0),
        ];
        assert_eq!(
            said(wind_shown(
                None,
                Some(1700.0),
                &stations,
                Some(&forecast()),
                NOW
            )),
            Some((90.0, CockpitWindSource::Station, "Wallberg".to_owned()))
        );
        // Higher up the forecast's level nearest the glider: 850 hPa at
        // about 1500 m for a glider at 2000 m, of the hour nearest now.
        assert_eq!(
            said(wind_shown(
                None,
                Some(2000.0),
                &stations,
                Some(&forecast()),
                NOW + 1200.0
            )),
            Some((300.0, CockpitWindSource::Forecast, "forecast".to_owned()))
        );
        assert_eq!(
            said(wind_shown(
                None,
                Some(3200.0),
                &[],
                Some(&forecast()),
                NOW + 5400.0
            )),
            Some((325.0, CockpitWindSource::Forecast, "forecast".to_owned()))
        );
        // Without an altitude, the wind 10 m above the model's ground.
        assert_eq!(
            said(wind_shown(None, None, &[], Some(&forecast()), NOW)),
            Some((270.0, CockpitWindSource::Forecast, "forecast".to_owned()))
        );
    }

    #[test]
    fn the_session_shows_the_wind_of_what_it_was_handed() {
        let folder = std::env::temp_dir().join("venti-cockpit-wind-test");
        let session = crate::FlightSession::new(folder.to_string_lossy().into_owned());
        assert_eq!(session.view(NOW).wind_shown, None);
        session.use_ground_wind(vec![station("Valley", 3000.0, 700.0, 10.0)]);
        assert_eq!(
            said(session.view(NOW).wind_shown),
            Some((10.0, CockpitWindSource::Station, "Valley".to_owned()))
        );
        // Without a position the glider has no altitude: the forecast's
        // wind 10 m above the model's ground comes before the valley's.
        session.use_forecast(Some(forecast()));
        assert_eq!(
            said(session.view(NOW).wind_shown),
            Some((270.0, CockpitWindSource::Forecast, "forecast".to_owned()))
        );
        session.use_forecast(None);
        session.use_ground_wind(Vec::new());
        assert_eq!(session.view(NOW).wind_shown, None);
    }

    #[test]
    fn without_a_forecast_hour_the_first_station_and_else_nothing() {
        let stations = [station("Valley", 3000.0, 700.0, 10.0)];
        // A forecast with no hour within ninety minutes is none.
        let late = NOW + 4.0 * 3600.0;
        assert_eq!(
            said(wind_shown(
                None,
                Some(2500.0),
                &stations,
                Some(&forecast()),
                late
            )),
            Some((10.0, CockpitWindSource::Station, "Valley".to_owned()))
        );
        assert_eq!(
            said(wind_shown(None, Some(2500.0), &[], Some(&forecast()), late)),
            None
        );
        assert_eq!(said(wind_shown(None, None, &[], None, NOW)), None);
    }
}
