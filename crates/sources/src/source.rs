use serde::{Deserialize, Serialize};
use venti_domain::{
    Airspace, BoundingBox, Cable, ForecastSeries, GeoPoint, ModelId, Observation, Site, Station,
    Timestamp,
};

use crate::Manifest;

/// One HTTP GET an adapter wants made.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Request {
    /// A label the adapter chooses so it can recognise the response in
    /// [`ObservationSource::parse`]. Also used to name the archived payload.
    pub tag: String,
    /// The URL to fetch.
    pub url: String,
}

impl Request {
    /// Creates a request.
    pub fn new(tag: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            url: url.into(),
        }
    }
}

/// What an adapter is told about the run it is part of.
#[derive(Debug, Clone, Copy)]
pub struct PlanCtx {
    /// The time of the run. Adapters never read a clock themselves.
    pub now: Timestamp,
    /// The area the run covers. Adapters skip stations outside it; the
    /// pipeline drops airspace that does not reach into it.
    pub area: BoundingBox,
}

/// A piece of a payload the adapter could not convert.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skipped {
    /// The text that could not be converted, shortened if long.
    pub what: String,
    /// Why.
    pub reason: String,
}

impl Skipped {
    /// Records a skipped piece, keeping at most 120 characters of it.
    pub fn new(what: &str, reason: impl Into<String>) -> Self {
        let what: String = what.chars().take(120).collect();
        Self {
            what,
            reason: reason.into(),
        }
    }
}

/// What an adapter made of one response.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Parsed {
    /// Stations found in the response.
    pub stations: Vec<Station>,
    /// Observations found in the response. They may be partial; the pipeline
    /// merges observations that share a station and a time.
    pub observations: Vec<Observation>,
    /// Further requests the adapter needs now that it has seen this response.
    pub follow_up: Vec<Request>,
    /// Pieces of the response that could not be converted.
    pub skipped: Vec<Skipped>,
}

/// Why a whole response was unusable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SourceError {
    /// The response is not in the format the adapter expects.
    #[error("malformed response: {0}")]
    Malformed(String),
    /// The adapter was handed a response to a request it did not make.
    #[error("no parser for request tag {0:?}")]
    UnknownRequest(String),
}

/// An adapter for a source of station observations.
///
/// Implementations are pure: no network, no file system, no clock. Given the
/// same inputs they return the same outputs, which is what lets them be tested
/// from recorded payloads.
pub trait ObservationSource: Send + Sync {
    /// The declared facts about this source.
    fn manifest(&self) -> &Manifest;

    /// The requests that start a run.
    fn plan(&self, ctx: &PlanCtx) -> Vec<Request>;

    /// Converts the response to one request into canonical records and, where
    /// the provider needs several steps, further requests.
    fn parse(&self, request: &Request, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError>;
}

/// What an airspace adapter made of one response.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedAirspace {
    /// Airspaces found in the response.
    pub airspaces: Vec<Airspace>,
    /// Further requests the adapter needs now that it has seen this response.
    pub follow_up: Vec<Request>,
    /// Airspaces, or parts of the response, that could not be converted. An
    /// adapter never leaves an airspace out without an entry here: a pilot
    /// is not warned about an airspace that is missing.
    pub skipped: Vec<Skipped>,
}

/// An adapter for a source of airspace.
///
/// Pure, like [`ObservationSource`]: it names the files it needs and turns
/// their bytes into canonical records. Unlike observations, an airspace run
/// is a snapshot. Each run delivers the whole set the source has for the
/// area, and that set replaces the one before: see
/// [`run_airspace`](crate::run_airspace).
pub trait AirspaceSource: Send + Sync {
    /// The declared facts about this source.
    fn manifest(&self) -> &Manifest;

    /// The requests that start a run.
    fn plan(&self, ctx: &PlanCtx) -> Vec<Request>;

    /// Converts the response to one request into canonical records and, where
    /// the provider needs several steps, further requests.
    fn parse(
        &self,
        request: &Request,
        body: &[u8],
        ctx: &PlanCtx,
    ) -> Result<ParsedAirspace, SourceError>;
}

/// What a site adapter made of one response.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedSites {
    /// Sites found in the response.
    pub sites: Vec<Site>,
    /// The date the response's data is of, if the response says: a source
    /// that answers from a copy of a database names the moment the copy
    /// was taken.
    pub as_of: Option<Timestamp>,
    /// Further requests the adapter needs now that it has seen this response.
    pub follow_up: Vec<Request>,
    /// Records, or parts of the response, that were left out, and why.
    pub skipped: Vec<Skipped>,
}

/// An adapter for a source of flying sites.
///
/// Pure, like [`ObservationSource`]. Like an airspace run, a site run is a
/// snapshot: it delivers every site the source has in the area, and that
/// set replaces the one before. See [`run_sites`](crate::run_sites).
pub trait SiteSource: Send + Sync {
    /// The declared facts about this source.
    fn manifest(&self) -> &Manifest;

    /// The requests that start a run.
    fn plan(&self, ctx: &PlanCtx) -> Vec<Request>;

    /// Converts the response to one request into canonical records and, where
    /// the provider needs several steps, further requests.
    fn parse(
        &self,
        request: &Request,
        body: &[u8],
        ctx: &PlanCtx,
    ) -> Result<ParsedSites, SourceError>;
}

/// What a cable adapter made of one response.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedCables {
    /// Cables found in the response.
    pub cables: Vec<Cable>,
    /// The date the response's data is of, if the response says.
    pub as_of: Option<Timestamp>,
    /// Records, or parts of the response, that were left out, and why.
    pub skipped: Vec<Skipped>,
}

/// An adapter for a source of cables.
///
/// Pure, like the others. A cable run is a snapshot, like a site run: it
/// delivers every cable the source has in the area, and that set replaces
/// the one before. See [`run_cables`](crate::run_cables).
pub trait CableSource: Send + Sync {
    /// The declared facts about this source.
    fn manifest(&self) -> &Manifest;

    /// The requests that start a run.
    fn plan(&self, ctx: &PlanCtx) -> Vec<Request>;

    /// Converts the response to one request into canonical records.
    fn parse(
        &self,
        request: &Request,
        body: &[u8],
        ctx: &PlanCtx,
    ) -> Result<ParsedCables, SourceError>;
}

/// What a forecast adapter made of one response.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedForecast {
    /// The series found in the response: one per model.
    pub series: Vec<ForecastSeries>,
    /// Further requests the adapter needs now that it has seen this
    /// response. A provider that says in one place which run of a model it
    /// holds and in another what the model forecasts is asked in two steps.
    pub follow_up: Vec<Request>,
    /// Parts of the response that could not be converted.
    pub skipped: Vec<Skipped>,
}

/// An adapter for a source of point forecasts.
///
/// Pure, like [`ObservationSource`]. What differs is what a run is asked
/// for: not an area on a schedule but one place, when somebody wants to
/// know. The adapter names the requests for that place and turns the
/// answers into one [`ForecastSeries`] per weather model. Every series must
/// say which run of its model it is from; an adapter that cannot find that
/// out delivers no series. See [`run_forecast`](crate::run_forecast).
pub trait ForecastSource: Send + Sync {
    /// The declared facts about this source.
    fn manifest(&self) -> &Manifest;

    /// The requests that start the forecast for `point`.
    fn plan(&self, point: GeoPoint, ctx: &PlanCtx) -> Vec<Request>;

    /// Converts the response to one request into series and, where the
    /// provider needs several steps, further requests.
    fn parse(
        &self,
        request: &Request,
        body: &[u8],
        point: GeoPoint,
        ctx: &PlanCtx,
    ) -> Result<ParsedForecast, SourceError>;

    /// How many places one forecast of [`plan_places`](Self::plan_places)
    /// may be asked for. Zero, the default, for a source that answers for
    /// one place at a time only.
    fn places_per_request(&self) -> usize {
        0
    }

    /// The requests that start the forecast of one model for several
    /// places at once, at most [`places_per_request`](Self::places_per_request)
    /// of them. None, the default, for a source that cannot, for a model it
    /// does not know, and for too many places.
    fn plan_places(&self, _model: &ModelId, _points: &[GeoPoint], _ctx: &PlanCtx) -> Vec<Request> {
        Vec::new()
    }

    /// Converts the response to one request of a forecast for several
    /// places into one series for each place it has, and further requests.
    /// `points` are the places asked for, in the order they were asked.
    fn parse_places(
        &self,
        request: &Request,
        _body: &[u8],
        _points: &[GeoPoint],
        _ctx: &PlanCtx,
    ) -> Result<ParsedForecast, SourceError> {
        Err(SourceError::UnknownRequest(request.tag.clone()))
    }
}
