//! Adapter for point forecasts from an Open-Meteo instance.
//!
//! Open-Meteo is open-source software that downloads the output of weather
//! models, keeps it as time series and answers for single places
//! (<https://open-meteo.com/en/docs/dwd-api>, read on 2026-11-19). This
//! adapter asks an instance of it what two models of Deutscher Wetterdienst
//! forecast for one place:
//!
//! - **ICON-D2**: cells about 2 km wide over Germany, the Alps and their
//!   surroundings, a new run every three hours, each reaching 48 hours.
//! - **ICON-EU**: cells about 7 km wide over Europe, a new run every three
//!   hours. The runs of 00, 06, 12 and 18 UTC reach 120 hours, the first 78
//!   of them hour by hour; the runs in between reach 30 hours.
//!
//! Which instance answers is a setting of whoever runs the server: see
//! [`OpenMeteo::new`] and the manifest. The adapter has no default address.
//!
//! # The requests
//!
//! Two per model, one after the other:
//!
//! 1. `{base}/data/{model}/static/meta.json` says which run of the model
//!    the instance holds, when that run began and when the instance had it
//!    ready.
//! 2. `{base}/v1/forecast` for the place and that one model, hour by hour
//!    for today and the two days after, in UTC. Wind is asked for in m/s
//!    and times as seconds since 1970. `cell_selection=nearest` and
//!    `elevation=nan` ask for the model's own value in the nearest cell:
//!    without them Open-Meteo looks for a cell at a similar height and
//!    shifts temperatures to the height of the place.
//!
//! Each model is asked for by itself, under Open-Meteo's name for it on the
//! grid the model computes on (`dwd_icon_d2_native`, `dwd_icon_eu_native`).
//! The "seamless" blends are not used: a blend does not say where one model
//! ends and the next begins.
//!
//! # What a series holds
//!
//! Only the hours that the named run computed, hour by hour: those after
//! the run began, as far as the run reaches. An instance keeps one value per
//! hour, the newest it has, so the hours before the run began, and those
//! beyond the reach of a short run, are values of earlier runs. The
//! instance does not say which, and so they are left out. The hour the run
//! begins at is left out too: gusts, precipitation and updraft at that hour
//! are the run before's.
//!
//! Open-Meteo fills the hours after the 78th of an ICON-EU run from values
//! three hours apart. Those are left out as well.
//!
//! # What is read
//!
//! At the ground: wind and gusts at 10 m, temperature and dew point at 2 m,
//! cloud cover in total and by layer, precipitation, CAPE, the height of
//! the 0 °C level, the base and top of convective cloud, and the heat the
//! ground gives to the air (the model's sensible heat flux, turned so that
//! upward is positive). From ICON-D2 also the strongest updraft in the
//! column. Higher up: wind and
//! temperature at the three lowest levels Open-Meteo keeps (nominally 80,
//! 120 and 180 m above the model's ground) and on the pressure surfaces of
//! 950, 850, 700, 600 and 500 hPa, each with the model's own height for it
//! at that hour. These five are surfaces both models compute; Open-Meteo
//! also answers for surfaces a model lacks, from its neighbours, and those
//! are not asked for.
//!
//! What the values mean, and what Open-Meteo makes of the model's output
//! before it answers, is written down in `docs/data-sources.md`. Two things
//! are done here: a base or top of convective cloud at or below 0 m is read
//! as "no convective cloud" (ICON writes -500 m where there is none, and
//! Open-Meteo hands on that or 0), and a value whose unit is not the one
//! asked for is not read.

pub mod fixtures;

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Timelike};
use serde::Deserialize;
use venti_domain::{ForecastHour, ForecastSeries, GeoPoint, ModelId, Timestamp, WindAloft};
use venti_sources::{
    ForecastSource, Manifest, ParsedForecast, PlanCtx, Request, Skipped, SourceError,
};

/// The heights above the model's ground the wind is read at, in metres.
/// They are the nominal heights of the model's three lowest levels that
/// Open-Meteo keeps.
pub const HEIGHTS_M: [u32; 3] = [80, 120, 180];

/// The pressure surfaces the wind is read on, in hPa. Both models compute
/// all five.
pub const PRESSURES_HPA: [u32; 5] = [950, 850, 700, 600, 500];

/// The days a forecast is asked for: today and the two after, in UTC.
const FORECAST_DAYS: u32 = 3;

/// How many places one request asks for at most. The answer of one model
/// for one place is about 19 kB (19.2 kB on average for 80 places of the
/// first region on 2026-11-21), so 80 places are about 1.5 MB, under the
/// manifest's limit of 2 MB for one answer with room for longer numbers.
pub const PLACES_PER_REQUEST: usize = 80;

/// A weather model the adapter asks for.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Model {
    /// The model's name in Venti.
    id: &'static str,
    /// Open-Meteo's name for the model on its own grid.
    name: &'static str,
    /// Whether the model gives the strongest updraft in the column.
    updraft: bool,
    /// How many hours a run that begins at 00, 06, 12 or 18 UTC reaches
    /// hour by hour.
    main_reach_h: i64,
    /// The same for a run that begins at 03, 09, 15 or 21 UTC.
    between_reach_h: i64,
}

/// The models, as Open-Meteo's own downloader takes them from Deutscher
/// Wetterdienst (`IconDownloadDomains.swift`, read on 2026-11-19): every
/// run of ICON-D2 reaches 48 hours; a run of ICON-EU at 00, 06, 12 or 18 UTC
/// has every hour to the 78th, and one in between reaches 30 hours.
const MODELS: [Model; 2] = [
    Model {
        id: "icon-d2",
        name: "dwd_icon_d2_native",
        updraft: true,
        main_reach_h: 48,
        between_reach_h: 48,
    },
    Model {
        id: "icon-eu",
        name: "dwd_icon_eu_native",
        updraft: false,
        main_reach_h: 78,
        between_reach_h: 30,
    },
];

/// The values at the ground: the canonical field, Open-Meteo's variable and
/// the unit it must come in.
const SURFACE: [(&str, &str, &str); 16] = [
    ("wind_speed_ms", "wind_speed_10m", "m/s"),
    ("wind_direction_deg", "wind_direction_10m", "°"),
    ("wind_gust_ms", "wind_gusts_10m", "m/s"),
    ("temperature_c", "temperature_2m", "°C"),
    ("dew_point_c", "dew_point_2m", "°C"),
    ("cloud_cover_pct", "cloud_cover", "%"),
    ("cloud_cover_low_pct", "cloud_cover_low", "%"),
    ("cloud_cover_mid_pct", "cloud_cover_mid", "%"),
    ("cloud_cover_high_pct", "cloud_cover_high", "%"),
    ("precipitation_mm", "precipitation", "mm"),
    ("cape_j_kg", "cape", "J/kg"),
    ("freezing_level_m", "freezing_level_height", "m"),
    ("updraft_ms", "updraft", "m/s"),
    ("convective_cloud_base_m", "convective_cloud_base", "m"),
    ("convective_cloud_top_m", "convective_cloud_top", "m"),
    ("heat_to_air_w_m2", "sensible_heat_flux", "W/m²"),
];

/// The values at a level: the canonical field, the start of Open-Meteo's
/// variable (the level follows it) and the unit.
const ALOFT: [(&str, &str, &str); 4] = [
    ("altitude_m", "geopotential_height", "m"),
    ("wind_speed_ms", "wind_speed", "m/s"),
    ("wind_direction_deg", "wind_direction", "°"),
    ("temperature_c", "temperature", "°C"),
];

/// The fields in which a height of zero or less means "there is none".
/// ICON writes -500 m where it has no convective cloud. In the answers
/// recorded on 2026-11-19 an instance handed that on unchanged for ICON-EU
/// and had turned it into 0 for ICON-D2. Neither is a height.
const NO_CLOUD: [&str; 2] = ["convective_cloud_base_m", "convective_cloud_top_m"];

/// The fields the model counts the other way round. ICON's sensible heat
/// flux at the ground is positive downward, towards the ground, and
/// Open-Meteo hands it on so: in the answers looked at on 2026-11-21 it was
/// negative under the midday sun and positive in a clear night. The
/// canonical field counts what the ground gives to the air.
const DOWNWARD: [&str; 1] = ["heat_to_air_w_m2"];

/// Point forecasts of ICON-D2 and ICON-EU from an Open-Meteo instance.
#[derive(Debug, Clone)]
pub struct OpenMeteo {
    manifest: Manifest,
    base: String,
}

impl OpenMeteo {
    /// Creates the adapter for the Open-Meteo instance at `base`, for
    /// example `http://open-meteo:8080`.
    ///
    /// Returns what is wrong with the address if it cannot be one: it must
    /// start with `https://` or `http://` and name a host, with a path in
    /// front of the API if the instance has one, and without a query.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn new(base: &str) -> Result<Self, String> {
        let manifest = Manifest::from_toml(include_str!("../manifest.toml"))
            .expect("the manifest compiled into venti-sources-openmeteo is valid");
        let base = base.trim_end_matches('/');
        let rest = base
            .strip_prefix("https://")
            .or_else(|| base.strip_prefix("http://"))
            .ok_or_else(|| format!("{base:?} must start with https:// or http://"))?;
        let plain =
            |c: char| c.is_ascii_graphic() && !matches!(c, '?' | '#' | '"' | '<' | '>' | '@');
        if rest.is_empty() || rest.starts_with('/') || !rest.chars().all(plain) {
            return Err(format!(
                "{base:?} must be the address of an Open-Meteo instance, \
                 such as http://open-meteo:8080, without a query"
            ));
        }
        Ok(Self {
            manifest,
            base: base.to_owned(),
        })
    }

    /// The address that says which run of a model the instance holds.
    fn run_url(&self, model: &Model) -> String {
        format!("{}/data/{}/static/meta.json", self.base, model.name)
    }

    /// The address that asks for what a model forecasts for some places:
    /// the forecast of one place, or of several in one request. Open-Meteo
    /// takes a list for each coordinate and for the height, one entry per
    /// place, and one choice of cell for them all.
    fn series_url(&self, model: &Model, points: &[GeoPoint]) -> String {
        let variables: Vec<String> = variables(model)
            .into_iter()
            .map(|variable| variable.name)
            .collect();
        let list = |of: fn(&GeoPoint) -> f64| {
            let values: Vec<String> = points
                .iter()
                .map(|point| format!("{:.4}", of(point)))
                .collect();
            values.join(",")
        };
        let heights = vec!["nan"; points.len()].join(",");
        format!(
            "{}/v1/forecast?latitude={}&longitude={}&models={}&hourly={}\
             &forecast_days={FORECAST_DAYS}&wind_speed_unit=ms&timeformat=unixtime\
             &elevation={heights}&cell_selection=nearest",
            self.base,
            list(GeoPoint::lat),
            list(GeoPoint::lon),
            model.name,
            variables.join(",")
        )
    }
}

/// Where a value of the answer belongs.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Place {
    /// At the ground.
    Ground,
    /// At a height above the model's ground, in metres.
    Height(u32),
    /// On a pressure surface, in hPa.
    Pressure(u32),
}

/// One variable the adapter asks for.
#[derive(Debug, Clone, PartialEq)]
struct Variable {
    /// Open-Meteo's name.
    name: String,
    /// The unit it must come in.
    unit: &'static str,
    /// The canonical field it fills.
    field: &'static str,
    /// The level it belongs to.
    place: Place,
}

/// Everything asked of a model, in the order of the request.
fn variables(model: &Model) -> Vec<Variable> {
    let mut all = Vec::new();
    for (field, name, unit) in SURFACE {
        if field == "updraft_ms" && !model.updraft {
            continue;
        }
        all.push(Variable {
            name: name.to_owned(),
            unit,
            field,
            place: Place::Ground,
        });
    }
    for height in HEIGHTS_M {
        for (field, name, unit) in ALOFT {
            // A height above the ground needs no height above the sea.
            if field == "altitude_m" {
                continue;
            }
            all.push(Variable {
                name: format!("{name}_{height}m"),
                unit,
                field,
                place: Place::Height(height),
            });
        }
    }
    for pressure in PRESSURES_HPA {
        for (field, name, unit) in ALOFT {
            all.push(Variable {
                name: format!("{name}_{pressure}hPa"),
                unit,
                field,
                place: Place::Pressure(pressure),
            });
        }
    }
    all
}

fn model(id: &str) -> Option<&'static Model> {
    MODELS.iter().find(|model| model.id == id)
}

/// What the instance says about the run of a model it holds.
#[derive(Debug, Deserialize)]
struct Meta {
    /// When the newest run it holds began, seconds since 1970.
    last_run_initialisation_time: i64,
    /// When the instance had that run ready.
    last_run_availability_time: Option<i64>,
    /// The first moment after the last value of that run. Open-Meteo's
    /// documentation does not describe this field; where it is there, it
    /// can only shorten what is read.
    data_end_time: Option<i64>,
    /// The step between two values, in seconds.
    temporal_resolution_seconds: Option<i64>,
}

/// The answer to a forecast request.
#[derive(Debug, Deserialize)]
struct Answer {
    /// Set when the instance refuses the request.
    #[serde(default)]
    error: bool,
    reason: Option<String>,
    /// The centre of the cell the values are for.
    latitude: Option<f64>,
    longitude: Option<f64>,
    /// The height of the model's ground in that cell.
    elevation: Option<f64>,
    /// In an answer for several places, the position of the place in the
    /// list asked for. Not written for the first.
    location_id: Option<usize>,
    #[serde(default)]
    hourly_units: BTreeMap<String, String>,
    #[serde(default)]
    hourly: BTreeMap<String, serde_json::Value>,
}

/// The run a series request was made for, as the request's tag carries it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Run {
    model: &'static Model,
    began: Timestamp,
    available: Option<Timestamp>,
    /// The first moment after the run's last value, if the instance says.
    end: Option<Timestamp>,
}

impl Run {
    fn tag(&self) -> String {
        let seconds = |time: Option<Timestamp>| {
            time.map_or_else(|| "-".to_owned(), |time| time.timestamp().to_string())
        };
        format!(
            "series:{}:{}:{}:{}",
            self.model.id,
            self.began.timestamp(),
            seconds(self.available),
            seconds(self.end)
        )
    }

    fn from_tag(tag: &str) -> Option<Self> {
        let mut parts = tag.strip_prefix("series:")?.split(':');
        let model = model(parts.next()?)?;
        let began = moment(parts.next()?.parse().ok()?)?;
        let mut optional = || match parts.next()? {
            "-" => Some(None),
            seconds => Some(Some(moment(seconds.parse().ok()?)?)),
        };
        let available = optional()?;
        let end = optional()?;
        parts.next().is_none().then_some(Self {
            model,
            began,
            available,
            end,
        })
    }

    /// The last hour the run computed for itself, hour by hour.
    fn last_hour(&self) -> Timestamp {
        let reach_h = if self.began.hour().is_multiple_of(6) {
            self.model.main_reach_h
        } else {
            self.model.between_reach_h
        };
        self.began + Duration::hours(reach_h)
    }

    /// Whether a moment is one of the hours this run computed.
    fn covers(&self, time: Timestamp) -> bool {
        time > self.began && time <= self.last_hour() && self.end.is_none_or(|end| time < end)
    }
}

fn moment(seconds: i64) -> Option<Timestamp> {
    DateTime::from_timestamp(seconds, 0)
}

impl ForecastSource for OpenMeteo {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// One request per model: which run does the instance hold?
    fn plan(&self, _point: GeoPoint, _ctx: &PlanCtx) -> Vec<Request> {
        MODELS
            .iter()
            .map(|model| Request::new(format!("run:{}", model.id), self.run_url(model)))
            .collect()
    }

    fn parse(
        &self,
        request: &Request,
        body: &[u8],
        point: GeoPoint,
        _ctx: &PlanCtx,
    ) -> Result<ParsedForecast, SourceError> {
        if let Some(model) = request.tag.strip_prefix("run:").and_then(model) {
            let run = read_run(model, body)?;
            let follow_up = Request::new(run.tag(), self.series_url(model, &[point]));
            return Ok(ParsedForecast {
                follow_up: vec![follow_up],
                ..ParsedForecast::default()
            });
        }
        let Some(run) = Run::from_tag(&request.tag) else {
            return Err(SourceError::UnknownRequest(request.tag.clone()));
        };
        let answer: Answer = serde_json::from_slice(body).map_err(|e| {
            SourceError::Malformed(format!(
                "{}: the answer is not a forecast: {e}",
                run.model.name
            ))
        })?;
        let (series, skipped) = self.read_series(&run, answer, point)?;
        Ok(ParsedForecast {
            series: vec![series],
            follow_up: Vec::new(),
            skipped,
        })
    }

    fn places_per_request(&self) -> usize {
        PLACES_PER_REQUEST
    }

    /// One request for the run the instance holds of the model, then one
    /// for the forecast of all the places.
    fn plan_places(&self, model: &ModelId, points: &[GeoPoint], _ctx: &PlanCtx) -> Vec<Request> {
        match self::model(model.as_str()) {
            Some(model) if !points.is_empty() && points.len() <= PLACES_PER_REQUEST => {
                vec![Request::new(
                    format!("run:{}", model.id),
                    self.run_url(model),
                )]
            }
            _ => Vec::new(),
        }
    }

    fn parse_places(
        &self,
        request: &Request,
        body: &[u8],
        points: &[GeoPoint],
        _ctx: &PlanCtx,
    ) -> Result<ParsedForecast, SourceError> {
        if let Some(model) = request.tag.strip_prefix("run:").and_then(model) {
            let run = read_run(model, body)?;
            let follow_up = Request::new(run.tag(), self.series_url(model, points));
            return Ok(ParsedForecast {
                follow_up: vec![follow_up],
                ..ParsedForecast::default()
            });
        }
        let Some(run) = Run::from_tag(&request.tag) else {
            return Err(SourceError::UnknownRequest(request.tag.clone()));
        };
        let mut parsed = ParsedForecast::default();
        for (answer, point) in read_places(run.model, body, points)?
            .into_iter()
            .zip(points)
        {
            match self.read_series(&run, answer, *point) {
                Ok((series, skipped)) => {
                    parsed.series.push(series);
                    parsed.skipped.extend(skipped);
                }
                // One place the instance could not answer for costs that
                // place, not the others.
                Err(error) => parsed.skipped.push(Skipped::new(
                    &format!("{:.4} N {:.4} E", point.lat(), point.lon()),
                    error.to_string(),
                )),
            }
        }
        Ok(parsed)
    }
}

/// The answers for several places, one for each in the order they were
/// asked for.
///
/// An instance answers a request for several places with a list of
/// answers, each like the answer for one place, in the order of the
/// places; every answer but the first names its position in the list
/// (`location_id`). It answers a request for one place with that answer
/// alone, and a request it refuses with one refusal for all.
fn read_places(
    model: &Model,
    body: &[u8],
    points: &[GeoPoint],
) -> Result<Vec<Answer>, SourceError> {
    let malformed = |what: String| SourceError::Malformed(format!("{}: {what}", model.name));
    let answers = match serde_json::from_slice::<serde_json::Value>(body)
        .map_err(|e| malformed(format!("the answer is not a forecast: {e}")))?
    {
        serde_json::Value::Array(answers) => answers,
        one => vec![one],
    };
    let answers: Vec<Answer> = answers
        .into_iter()
        .map(Answer::deserialize)
        .collect::<Result<_, _>>()
        .map_err(|e| malformed(format!("the answer is not a forecast: {e}")))?;
    if let [refusal] = answers.as_slice()
        && refusal.error
    {
        let reason = refusal.reason.clone().unwrap_or_default();
        return Err(malformed(format!("the instance refused: {reason}")));
    }
    if answers.len() != points.len() {
        return Err(malformed(format!(
            "{} answers for {} places",
            answers.len(),
            points.len()
        )));
    }
    for (position, answer) in answers.iter().enumerate() {
        if answer.location_id.is_some_and(|id| id != position) {
            return Err(malformed(format!(
                "the answer in position {position} says it is for place {}",
                answer.location_id.unwrap_or_default()
            )));
        }
    }
    Ok(answers)
}

/// Reads which run of a model the instance holds.
fn read_run(model: &'static Model, body: &[u8]) -> Result<Run, SourceError> {
    let malformed = |what: String| SourceError::Malformed(format!("{}: {what}", model.name));
    let meta: Meta = serde_json::from_slice(body)
        .map_err(|e| malformed(format!("the answer does not say which run is held: {e}")))?;
    if let Some(step) = meta.temporal_resolution_seconds
        && step != 3600
    {
        return Err(malformed(format!(
            "the instance keeps a value every {step} seconds, and the adapter reads hours"
        )));
    }
    let began = moment(meta.last_run_initialisation_time)
        .filter(|began| began.minute() == 0 && began.second() == 0)
        .ok_or_else(|| {
            malformed(format!(
                "a run cannot have begun at {}",
                meta.last_run_initialisation_time
            ))
        })?;
    Ok(Run {
        model,
        began,
        // A time that cannot be read is no time; the series goes without.
        available: meta.last_run_availability_time.and_then(moment),
        end: meta.data_end_time.and_then(moment),
    })
}

impl OpenMeteo {
    /// Reads what a model forecasts for a place, and what could not be read.
    fn read_series(
        &self,
        run: &Run,
        answer: Answer,
        point: GeoPoint,
    ) -> Result<(ForecastSeries, Vec<Skipped>), SourceError> {
        let name = run.model.name;
        let malformed = |what: String| SourceError::Malformed(format!("{name}: {what}"));
        if answer.error {
            let reason = answer.reason.unwrap_or_default();
            return Err(malformed(format!("the instance refused: {reason}")));
        }
        let cell = match (answer.latitude, answer.longitude) {
            (Some(lat), Some(lon)) => GeoPoint::new(lat, lon).ok(),
            _ => None,
        }
        .ok_or_else(|| malformed("the answer does not say which cell it is for".to_owned()))?;

        if answer.hourly_units.get("time").map(String::as_str) != Some("unixtime") {
            return Err(malformed(
                "the times are not in seconds since 1970".to_owned(),
            ));
        }
        let times: Vec<Timestamp> = answer
            .hourly
            .get("time")
            .and_then(|times| Vec::<i64>::deserialize(times).ok())
            .and_then(|times| times.into_iter().map(moment).collect())
            .ok_or_else(|| malformed("the answer has no list of times".to_owned()))?;
        let kept: Vec<usize> = (0..times.len())
            .filter(|&index| run.covers(times[index]))
            .collect();

        let mut skipped = Vec::new();
        if kept.is_empty() {
            skipped.push(Skipped::new(
                name,
                format!(
                    "the run of {} holds no hour of today and the two days after",
                    run.began.format("%Y-%m-%d %H:%M UTC")
                ),
            ));
        }

        // Each variable's values for the hours kept, where they can be read.
        let mut columns: Vec<(Variable, Vec<Option<f64>>)> = Vec::new();
        for variable in variables(run.model) {
            match column(&answer, &variable, times.len()) {
                Ok(values) => {
                    let values: Vec<Option<f64>> = kept.iter().map(|&i| values[i]).collect();
                    if !kept.is_empty() && values.iter().all(Option::is_none) {
                        let reason = "the instance has no value for any hour of the run";
                        skipped.push(Skipped::new(&variable.name, reason));
                    }
                    columns.push((variable, values));
                }
                Err(reason) => skipped.push(Skipped::new(&variable.name, reason)),
            }
        }

        let mut hours = Vec::with_capacity(kept.len());
        for (position, &index) in kept.iter().enumerate() {
            let mut hour = ForecastHour::empty(times[index]);
            let mut levels: Vec<(Place, WindAloft)> = HEIGHTS_M
                .iter()
                .map(|&height| {
                    let level = WindAloft::above_ground(f64::from(height));
                    (Place::Height(height), level)
                })
                .chain(PRESSURES_HPA.iter().map(|&pressure| {
                    let level = WindAloft::at_pressure(f64::from(pressure));
                    (Place::Pressure(pressure), level)
                }))
                .collect();

            for (variable, values) in &columns {
                // Adding zero turns the "-0.0" an instance sometimes writes
                // into plain zero.
                let mut value = values[position].map(|value| value + 0.0);
                if NO_CLOUD.contains(&variable.field) && value.is_some_and(|height| height <= 0.0) {
                    value = None;
                }
                if DOWNWARD.contains(&variable.field) {
                    value = value.map(|downward| 0.0 - downward);
                }
                let slot = match variable.place {
                    Place::Ground => hour
                        .fields_mut()
                        .into_iter()
                        .find(|(field, _)| *field == variable.field)
                        .map(|(_, slot)| slot),
                    place => levels
                        .iter_mut()
                        .find(|(level, _)| *level == place)
                        .and_then(|(_, level)| {
                            level
                                .fields_mut()
                                .into_iter()
                                .find(|(field, _)| *field == variable.field)
                                .map(|(_, slot)| slot)
                        }),
                };
                if let Some(slot) = slot {
                    *slot = value;
                }
            }
            hour.winds_aloft = levels
                .into_iter()
                .map(|(_, level)| level)
                .filter(|level| !level.is_empty())
                .collect();
            hours.push(hour);
        }

        let series = ForecastSeries {
            source: self.manifest.id.clone(),
            model: ModelId::new(run.model.id).expect("the model names are identifiers"),
            run: run.began,
            available_at: run.available,
            point,
            cell,
            cell_elevation_m: answer.elevation.filter(|height| height.is_finite()),
            hours,
        };
        Ok((series, skipped))
    }
}

/// The values of one variable, one per time of the answer, or why they
/// cannot be read.
fn column(answer: &Answer, variable: &Variable, times: usize) -> Result<Vec<Option<f64>>, String> {
    let Some(values) = answer.hourly.get(&variable.name) else {
        return Err("the answer does not have it".to_owned());
    };
    match answer.hourly_units.get(&variable.name) {
        Some(unit) if unit == variable.unit => {}
        Some(unit) => {
            return Err(format!(
                "it comes in {unit:?} where {:?} was asked for",
                variable.unit
            ));
        }
        None => return Err("the answer does not say in which unit it is".to_owned()),
    }
    let values = Vec::<Option<f64>>::deserialize(values)
        .map_err(|e| format!("its values are not numbers: {e}"))?;
    if values.len() != times {
        return Err(format!("it has {} values for {times} hours", values.len()));
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn source() -> OpenMeteo {
        OpenMeteo::new("http://open-meteo:8080").unwrap()
    }

    #[test]
    fn the_manifest_is_valid_and_names_what_the_adapter_delivers() {
        let source = source();
        let manifest = source.manifest();
        assert_eq!(manifest.id.as_str(), "open-meteo");
        assert_eq!(manifest.kinds, [venti_sources::Kind::Forecasts]);
        // The credit names both the service and the weather service whose
        // models these are.
        assert!(manifest.licence.attribution.contains("Open-Meteo.com"));
        assert!(
            manifest
                .licence
                .attribution
                .contains("Deutscher Wetterdienst")
        );
        assert!(!manifest.licence.share_alike);
    }

    #[test]
    fn an_address_must_be_one() {
        for good in [
            "http://open-meteo:8080",
            "http://open-meteo:8080/",
            "https://forecast.example/open-meteo",
            "http://127.0.0.1:8080",
        ] {
            assert!(OpenMeteo::new(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "open-meteo:8080",
            "ftp://open-meteo",
            "https://",
            "https:///v1",
            "https://forecast.example/?apikey=1",
            "https://user@forecast.example",
            "https://forecast.example/a b",
        ] {
            assert!(OpenMeteo::new(bad).is_err(), "{bad}");
        }
        // A slash at the end is not part of the address.
        let source = OpenMeteo::new("https://forecast.example/").unwrap();
        assert_eq!(
            source.run_url(&MODELS[0]),
            "https://forecast.example/data/dwd_icon_d2_native/static/meta.json"
        );
    }

    #[test]
    fn the_request_asks_for_the_models_own_values_in_units_that_need_no_conversion() {
        let point = GeoPoint::new(47.7, 11.8).unwrap();
        let url = source().series_url(&MODELS[0], &[point]);
        assert!(
            url.starts_with("http://open-meteo:8080/v1/forecast?latitude=47.7000&longitude=11.8000&models=dwd_icon_d2_native&hourly=wind_speed_10m,"),
            "{url}"
        );
        assert!(url.ends_with(
            "&forecast_days=3&wind_speed_unit=ms&timeformat=unixtime&elevation=nan&cell_selection=nearest"
        ));
        assert!(url.contains(",updraft,"));
        assert!(url.contains("wind_speed_80m,wind_direction_80m,temperature_80m,wind_speed_120m"));
        assert!(url.contains(
            "geopotential_height_950hPa,wind_speed_950hPa,wind_direction_950hPa,temperature_950hPa"
        ));
        // Surfaces a model lacks are not asked for: Open-Meteo would fill
        // them from their neighbours.
        for filled in ["925hPa", "900hPa", "800hPa"] {
            assert!(!url.contains(filled), "{filled}");
        }
        // ICON-EU has no updraft.
        let url = source().series_url(&MODELS[1], &[point]);
        assert!(url.contains("models=dwd_icon_eu_native"));
        assert!(!url.contains("updraft"));
        assert_eq!(variables(&MODELS[0]).len(), 45);
        assert_eq!(variables(&MODELS[1]).len(), 44);
    }

    #[test]
    fn a_run_travels_in_the_tag_of_the_request_made_for_it() {
        let began = Utc.with_ymd_and_hms(2026, 11, 19, 15, 0, 0).unwrap();
        let run = Run {
            model: &MODELS[1],
            began,
            available: Some(began + Duration::minutes(170)),
            end: None,
        };
        assert_eq!(run.tag(), "series:icon-eu:1795100400:1795110600:-");
        assert_eq!(Run::from_tag(&run.tag()), Some(run));
        for other in [
            "series:icon-eu",
            "series:icon-eu:soon:-:-",
            "series:gfs:1795100400:-:-",
            "series:icon-eu:1795100400:-:-:more",
            "run:icon-eu",
        ] {
            assert_eq!(Run::from_tag(other), None, "{other}");
        }
    }

    #[test]
    fn a_run_covers_the_hours_it_computed_and_no_others() {
        let at = |day, hour| Utc.with_ymd_and_hms(2026, 11, day, hour, 0, 0).unwrap();
        let run = |model: usize, began| Run {
            model: &MODELS[model],
            began,
            available: None,
            end: None,
        };

        // ICON-D2: 48 hours after the hour it began at.
        let d2 = run(0, at(19, 15));
        assert!(!d2.covers(at(19, 14)));
        assert!(!d2.covers(at(19, 15)));
        assert!(d2.covers(at(19, 16)));
        assert!(d2.covers(at(21, 15)));
        assert!(!d2.covers(at(21, 16)));

        // ICON-EU: 78 hours from a main run, 30 from one in between.
        let main = run(1, at(19, 12));
        assert!(main.covers(at(22, 18)));
        assert!(!main.covers(at(22, 19)));
        let between = run(1, at(19, 15));
        assert!(between.covers(at(20, 21)));
        assert!(!between.covers(at(20, 22)));

        // What the instance says about the end of the run can only shorten it.
        let short = Run {
            end: Some(at(20, 0)),
            ..d2
        };
        assert!(short.covers(at(19, 23)));
        assert!(!short.covers(at(20, 0)));
        let long = Run {
            end: Some(at(25, 0)),
            ..d2
        };
        assert!(!long.covers(at(21, 16)));
    }
}
