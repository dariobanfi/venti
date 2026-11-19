use serde::{Deserialize, Serialize};
use venti_domain::{Airspace, BoundingBox, Observation, Station, Timestamp};

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
