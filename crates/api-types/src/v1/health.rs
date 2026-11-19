use serde::{Deserialize, Serialize};
use venti_domain::{SourceId, Timestamp};

/// The answer of `GET /v1/health`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    /// The state of the whole server.
    pub status: HealthStatus,
    /// The server's version.
    pub version: String,
    /// The server's clock. Ages of observations are best computed against
    /// this time, not against the clock of the device.
    pub time: Timestamp,
    /// One entry per data source the server collects, sorted by identifier.
    pub sources: Vec<SourceHealth>,
}

/// The state of the whole server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// Every source delivered data recently.
    Ok,
    /// At least one source is stale.
    Degraded,
}

/// How one data source is doing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceHealth {
    /// The source.
    pub source: SourceId,
    /// Who publishes the data.
    pub provider: String,
    /// How often the server asks the provider for new data, in seconds.
    pub cadence_seconds: u32,
    /// When a run last delivered observations without trouble.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success: Option<Timestamp>,
    /// When a run last delivered nothing or ran into trouble.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure: Option<Timestamp>,
    /// The time of the newest observation the source has delivered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newest_observation: Option<Timestamp>,
    /// Whether the source is behind: no run has succeeded within three of its
    /// cadence intervals.
    pub stale: bool,
    /// How many stations the last successful run delivered.
    pub stations: u32,
}
