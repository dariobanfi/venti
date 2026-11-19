use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Why an identifier was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    /// The identifier was empty.
    #[error("identifier is empty")]
    Empty,
    /// The identifier contained a character outside the allowed set.
    #[error("identifier {0:?} may only contain a-z, 0-9 and '-'")]
    InvalidSourceId(String),
    /// A station identifier was not of the form `source:local`.
    #[error("station identifier {0:?} is not of the form source:local")]
    InvalidStationId(String),
    /// An airspace identifier was not of the form `source:local`.
    #[error("airspace identifier {0:?} is not of the form source:local")]
    InvalidAirspaceId(String),
}

/// Identifies one data source, for example `dwd-10min`.
///
/// Lower-case letters, digits and hyphens only, so it is safe in paths and URLs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SourceId(String);

impl SourceId {
    /// Creates a source identifier, checking its characters.
    pub fn new(id: impl Into<String>) -> Result<Self, IdError> {
        let id = id.into();
        if id.is_empty() {
            return Err(IdError::Empty);
        }
        let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-';
        if !id.chars().all(allowed) {
            return Err(IdError::InvalidSourceId(id));
        }
        Ok(Self(id))
    }

    /// The identifier as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for SourceId {
    type Error = IdError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<SourceId> for String {
    fn from(value: SourceId) -> Self {
        value.0
    }
}

/// Identifies one station as one source knows it.
///
/// The same physical station reported by two sources has two `StationId`s.
/// Linking them is the job of the resolve stage, not of the identifier.
/// Written as `source:local`, for example `dwd-10min:05792`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct StationId {
    source: SourceId,
    local: String,
}

impl StationId {
    /// Creates a station identifier from a source and that source's own station key.
    pub fn new(source: SourceId, local: impl Into<String>) -> Result<Self, IdError> {
        let local = local.into();
        if local.is_empty() {
            return Err(IdError::Empty);
        }
        Ok(Self { source, local })
    }

    /// The source that reports this station.
    pub fn source(&self) -> &SourceId {
        &self.source
    }

    /// The station key in the source's own numbering.
    pub fn local(&self) -> &str {
        &self.local
    }
}

impl fmt::Display for StationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.source, self.local)
    }
}

impl FromStr for StationId {
    type Err = IdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (source, local) = s
            .split_once(':')
            .ok_or_else(|| IdError::InvalidStationId(s.to_owned()))?;
        Self::new(SourceId::new(source)?, local)
    }
}

impl TryFrom<String> for StationId {
    type Error = IdError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<StationId> for String {
    fn from(value: StationId) -> Self {
        value.to_string()
    }
}

/// Identifies one airspace as one source knows it.
///
/// The same airspace published by two sources has two `AirspaceId`s; data
/// from different sources is never merged into one record. Written as
/// `source:local`, for example `openaip:677d59be13e31af480aca6d9`. The local
/// part is the source's own key where it has one. Where it has none, the
/// adapter derives one from what identifies the airspace (its name and
/// limits), so that it stays the same from one edition of the data to the
/// next.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AirspaceId {
    source: SourceId,
    local: String,
}

impl AirspaceId {
    /// Creates an airspace identifier from a source and that source's own key.
    pub fn new(source: SourceId, local: impl Into<String>) -> Result<Self, IdError> {
        let local = local.into();
        if local.is_empty() {
            return Err(IdError::Empty);
        }
        Ok(Self { source, local })
    }

    /// The source that publishes this airspace.
    pub fn source(&self) -> &SourceId {
        &self.source
    }

    /// The airspace's key within its source.
    pub fn local(&self) -> &str {
        &self.local
    }
}

impl fmt::Display for AirspaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.source, self.local)
    }
}

impl FromStr for AirspaceId {
    type Err = IdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (source, local) = s
            .split_once(':')
            .ok_or_else(|| IdError::InvalidAirspaceId(s.to_owned()))?;
        Self::new(SourceId::new(source)?, local)
    }
}

impl TryFrom<String> for AirspaceId {
    type Error = IdError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<AirspaceId> for String {
    fn from(value: AirspaceId) -> Self {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airspace_id_round_trips_through_text() {
        let id: AirspaceId = "openaip:677d59be13e31af480aca6d9".parse().unwrap();
        assert_eq!(id.source().as_str(), "openaip");
        assert_eq!(id.local(), "677d59be13e31af480aca6d9");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"openaip:677d59be13e31af480aca6d9\"");
        assert_eq!(serde_json::from_str::<AirspaceId>(&json).unwrap(), id);
        // Only the first colon splits, so a local key may contain one.
        let odd: AirspaceId = "file:ctr:muenchen".parse().unwrap();
        assert_eq!(odd.local(), "ctr:muenchen");
        assert!("nothing".parse::<AirspaceId>().is_err());
        assert!("openaip:".parse::<AirspaceId>().is_err());
    }

    #[test]
    fn source_id_rejects_unsafe_characters() {
        assert!(SourceId::new("dwd-10min").is_ok());
        assert_eq!(SourceId::new(""), Err(IdError::Empty));
        assert!(SourceId::new("DWD").is_err());
        assert!(SourceId::new("a/b").is_err());
        assert!(SourceId::new("a:b").is_err());
    }

    #[test]
    fn station_id_round_trips_through_text() {
        let id: StationId = "dwd-10min:05792".parse().unwrap();
        assert_eq!(id.source().as_str(), "dwd-10min");
        assert_eq!(id.local(), "05792");
        assert_eq!(id.to_string(), "dwd-10min:05792");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"dwd-10min:05792\"");
        assert_eq!(serde_json::from_str::<StationId>(&json).unwrap(), id);
    }

    #[test]
    fn station_id_needs_both_parts() {
        assert!("05792".parse::<StationId>().is_err());
        assert!("dwd-10min:".parse::<StationId>().is_err());
    }
}
