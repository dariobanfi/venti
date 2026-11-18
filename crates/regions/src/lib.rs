//! Region packs.
//!
//! A region pack is configuration, not code. It says which data sources are
//! active in an area and which wins when two of them report the same thing.
//! Adding a country means adding adapters for its providers and one region
//! pack; nothing else changes.
//!
//! The area is a bounding box for now. A boundary polygon replaces it when a
//! region needs a shape a box cannot express.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use venti_domain::{BoundingBox, SourceId};
use venti_sources::Registry;

/// The configuration of one region.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionPack {
    /// Stable identifier, for example `alps-de-at`.
    pub id: String,
    /// Name shown to people.
    pub name: String,
    /// Languages offered in the region, most important first.
    pub languages: Vec<String>,
    /// The area the region covers.
    pub bounds: BoundingBox,
    /// The sources of station observations.
    pub observations: SourceList,
}

/// The sources of one kind of data in a region.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceList {
    /// Every active source.
    pub sources: Vec<SourceId>,
    /// Sources in the order they win when two report the same object. Sources
    /// not listed rank below all listed ones.
    #[serde(default)]
    pub priority: Vec<SourceId>,
}

/// Why a region pack was rejected.
#[derive(Debug, thiserror::Error)]
pub enum RegionError {
    /// The text is not valid TOML or does not match the region pack shape.
    #[error("region pack does not parse: {0}")]
    Parse(#[from] toml::de::Error),
    /// A source is listed twice.
    #[error("region {region}: source {source_id} is listed twice")]
    Duplicate {
        /// The region.
        region: String,
        /// The source listed twice.
        source_id: SourceId,
    },
    /// The priority list names a source that is not active in the region.
    #[error("region {region}: priority names {source_id}, which is not among the region's sources")]
    PriorityNotActive {
        /// The region.
        region: String,
        /// The source named in the priority list.
        source_id: SourceId,
    },
    /// The region names a source the build does not contain.
    #[error("region {region}: no adapter is registered for source {source_id}")]
    UnknownSource {
        /// The region.
        region: String,
        /// The source without an adapter.
        source_id: SourceId,
    },
}

impl RegionPack {
    /// Parses a region pack written in TOML and checks it against itself.
    pub fn from_toml(text: &str) -> Result<Self, RegionError> {
        let pack: RegionPack = toml::from_str(text)?;
        let mut seen = BTreeSet::new();
        for source in &pack.observations.sources {
            if !seen.insert(source) {
                return Err(RegionError::Duplicate {
                    region: pack.id.clone(),
                    source_id: source.clone(),
                });
            }
        }
        for source in &pack.observations.priority {
            if !seen.contains(source) {
                return Err(RegionError::PriorityNotActive {
                    region: pack.id.clone(),
                    source_id: source.clone(),
                });
            }
        }
        Ok(pack)
    }

    /// Checks that the build contains an adapter for every source the region names.
    pub fn check_against(&self, registry: &Registry) -> Result<(), RegionError> {
        for source in &self.observations.sources {
            if registry.observations(source).is_none() {
                return Err(RegionError::UnknownSource {
                    region: self.id.clone(),
                    source_id: source.clone(),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACK: &str = r#"
        id = "example"
        name = "Example"
        languages = ["de", "en"]

        [bounds]
        south = 46.3
        west = 9.4
        north = 48.5
        east = 17.2

        [observations]
        sources = ["one", "two"]
        priority = ["two"]
    "#;

    #[test]
    fn parses_a_region_pack() {
        let pack = RegionPack::from_toml(PACK).unwrap();
        assert_eq!(pack.observations.sources.len(), 2);
        assert_eq!(pack.observations.priority[0].as_str(), "two");
    }

    #[test]
    fn rejects_a_source_listed_twice() {
        let text = PACK.replace(r#"["one", "two"]"#, r#"["one", "one"]"#);
        assert!(matches!(
            RegionPack::from_toml(&text),
            Err(RegionError::Duplicate { .. })
        ));
    }

    #[test]
    fn rejects_a_priority_for_an_inactive_source() {
        let text = PACK.replace(r#"priority = ["two"]"#, r#"priority = ["three"]"#);
        assert!(matches!(
            RegionPack::from_toml(&text),
            Err(RegionError::PriorityNotActive { .. })
        ));
    }

    #[test]
    fn rejects_sources_without_an_adapter() {
        let pack = RegionPack::from_toml(PACK).unwrap();
        let registry = Registry::new();
        assert!(matches!(
            pack.check_against(&registry),
            Err(RegionError::UnknownSource { .. })
        ));
    }
}
