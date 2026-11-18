//! Adapter for the 10-minute station observations of the Deutscher Wetterdienst.
//!
//! The data lives on DWD's open-data file server as three datasets: mean wind,
//! wind extremes and air temperature. Each dataset has a station list and one
//! zip file per station holding the current day as a semicolon-separated table.
//!
//! Facts taken from DWD's dataset description, read 2026-11-18:
//!
//! - time stamps are UTC and mark the end of the 10-minute interval;
//! - a missing value is written as `-999`;
//! - station lists are Latin-1 text with one station per line.
//!
//! A run therefore takes two steps. First the three station lists, which also
//! say which stations are still reporting. Then, for every current station
//! inside the run's area, that station's file from each dataset. The pipeline
//! merges the three partial observations per station and time.

mod stations;
mod table;

use venti_sources::{Manifest, ObservationSource, Parsed, PlanCtx, Request, SourceError};

const BASE: &str =
    "https://opendata.dwd.de/climate_environment/CDC/observations_germany/climate/10_minutes";

/// One of the three DWD datasets this adapter reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dataset {
    Wind,
    Extremes,
    Air,
}

impl Dataset {
    const ALL: [Dataset; 3] = [Dataset::Wind, Dataset::Extremes, Dataset::Air];

    /// Short name used in request tags.
    fn key(self) -> &'static str {
        match self {
            Dataset::Wind => "wind",
            Dataset::Extremes => "extremes",
            Dataset::Air => "air",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|dataset| dataset.key() == key)
    }

    fn directory(self) -> &'static str {
        match self {
            Dataset::Wind => "wind",
            Dataset::Extremes => "extreme_wind",
            Dataset::Air => "air_temperature",
        }
    }

    fn station_list_file(self) -> &'static str {
        match self {
            Dataset::Wind => "zehn_now_ff_Beschreibung_Stationen.txt",
            Dataset::Extremes => "zehn_now_fx_Beschreibung_Stationen.txt",
            Dataset::Air => "zehn_now_tu_Beschreibung_Stationen.txt",
        }
    }

    fn data_file(self, station: &str) -> String {
        match self {
            Dataset::Wind => format!("10minutenwerte_wind_{station}_now.zip"),
            Dataset::Extremes => format!("10minutenwerte_extrema_wind_{station}_now.zip"),
            Dataset::Air => format!("10minutenwerte_TU_{station}_now.zip"),
        }
    }

    fn station_list_request(self) -> Request {
        let url = format!(
            "{BASE}/{}/now/{}",
            self.directory(),
            self.station_list_file()
        );
        Request::new(format!("stations/{}", self.key()), url)
    }

    fn data_request(self, station: &str) -> Request {
        let url = format!(
            "{BASE}/{}/now/{}",
            self.directory(),
            self.data_file(station)
        );
        Request::new(format!("data/{}/{station}", self.key()), url)
    }
}

/// The DWD 10-minute observation source.
#[derive(Debug, Clone)]
pub struct Dwd10Min {
    manifest: Manifest,
}

impl Dwd10Min {
    /// Creates the adapter.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn new() -> Self {
        let manifest = Manifest::from_toml(include_str!("../manifest.toml"))
            .expect("the manifest compiled into venti-sources-dwd is valid");
        Self { manifest }
    }
}

impl Default for Dwd10Min {
    fn default() -> Self {
        Self::new()
    }
}

impl ObservationSource for Dwd10Min {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn plan(&self, _ctx: &PlanCtx) -> Vec<Request> {
        Dataset::ALL
            .into_iter()
            .map(Dataset::station_list_request)
            .collect()
    }

    fn parse(&self, request: &Request, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
        let unknown = || SourceError::UnknownRequest(request.tag.clone());
        let mut parts = request.tag.split('/');
        let step = parts.next().ok_or_else(unknown)?;
        let dataset = parts
            .next()
            .and_then(Dataset::from_key)
            .ok_or_else(unknown)?;
        match step {
            "stations" => stations::parse(&self.manifest.id, dataset, body, ctx),
            "data" => table::parse(&self.manifest.id, dataset, body),
            _ => Err(unknown()),
        }
    }
}
