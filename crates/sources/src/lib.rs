//! The framework every external data source plugs into.
//!
//! An *adapter* turns one provider's data into the canonical types of
//! `venti-domain`. Adapters do no I/O of their own. They say which URLs they
//! need ([`ObservationSource::plan`]) and turn the bytes that come back into
//! records ([`ObservationSource::parse`]). The pipeline in [`run_observations`]
//! does the fetching, archiving, merging and validation, the same way for
//! every source.
//!
//! That split is what makes an adapter testable from recorded payloads alone,
//! and what guarantees that rate limits, archiving and validation cannot be
//! skipped by one careless adapter.
//!
//! Each kind of data has its own trait and its own run. Observations
//! ([`ObservationSource`], [`run_observations`]) arrive every few minutes in
//! pieces that are merged. Airspace ([`AirspaceSource`], [`run_airspace`])
//! changes with the 28-day publication cycle of aviation data; a run
//! delivers the whole set, which replaces the one before. Flying sites
//! ([`SiteSource`], [`run_sites`]) are a snapshot in the same way. A point
//! forecast ([`ForecastSource`], [`run_forecast`]) is not collected for an
//! area at all: a run fetches what the weather models say about one place,
//! when somebody asks. Map imagery ([`ImagerySource`]) is not collected
//! either: the server asks for one tile when a client's map does, and the
//! adapter says what of it may be shown. Rain radar ([`RadarSource`],
//! [`run_radar`]) arrives every few minutes as a picture of an area; a run
//! delivers the frames the server does not hold yet, each on the grid the
//! server asks for. Live positions of another network ([`LiveSource`])
//! come over a connection that stays open, which the server holds; the
//! adapter only reads its lines.
//!
//! See `docs/data-sources.md` for the design and the source catalogue.

mod archive;
mod cables;
mod fetch;
mod forecast;
mod imagery;
mod live;
mod manifest;
mod network;
mod radar;
mod registry;
mod run;
mod sites;
mod source;
pub mod testing;
mod validate;

pub use archive::{Archive, ArchiveEntry, ArchiveError};
pub use cables::{CableRunOutput, CableRunReport, run_cables};
pub use fetch::{Fetch, FetchError, Fetched};
pub use forecast::{
    ForecastRejection, ForecastRunOutput, ForecastRunReport, run_forecast, run_forecast_places,
};
pub use imagery::{Coverage, Cut, CutCounts, CutTile, ImagerySource, MAX_TILE_ZOOM, TileAddress};
pub use live::{
    Aircraft, DeviceChoice, DeviceDirectory, LiveLine, LivePosition, LiveSource, Shown,
};
pub use manifest::{
    Asset, Auth, DEFAULT_MAX_RESPONSE_KB, Kind, Licence, LicenceFamily, MAX_SHORT_NAME_CHARS,
    Manifest, ManifestError, RateLimit,
};
pub use network::{Sample, StationNetwork};
pub use radar::{
    ParsedRadar, RadarRejection, RadarRunOutput, RadarRunReport, RadarSource, RadarWanted,
    run_radar,
};
pub use registry::{Registry, RegistryError};
pub use run::{
    AirspaceRejection, AirspaceRunOutput, AirspaceRunReport, Failure, RunOptions, RunOutput,
    RunReport, run_airspace, run_observations,
};
pub use sites::{SiteRejection, SiteRunOutput, SiteRunReport, check_site, run_sites};
pub use source::{
    AirspaceSource, CableSource, ForecastSource, ObservationSource, Parsed, ParsedAirspace,
    ParsedCables, ParsedForecast, ParsedSites, PlanCtx, Request, SiteSource, Skipped, SourceError,
};
pub use validate::Rejection;
