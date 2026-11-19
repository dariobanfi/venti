//! Region packs.
//!
//! A region pack is configuration, not code. It says which data sources are
//! active in an area and which wins when two of them report the same thing.
//! Adding a country means adding adapters for its providers and one region
//! pack; nothing else changes.
//!
//! A pack has one section per kind of data: `[observations]`, which every
//! region has, and `[airspace]`, which a region may leave out.
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
    /// The sources of airspace. A region may have none.
    ///
    /// Airspace from different sources is never merged: each source stays a
    /// layer of its own under its own licence. The priority says which
    /// layer a client prefers where two cover the same airspace.
    #[serde(default, skip_serializing_if = "SourceList::is_empty")]
    pub airspace: SourceList,
}

/// The sources of one kind of data in a region.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceList {
    /// Every active source.
    pub sources: Vec<SourceId>,
    /// Sources in the order they win when two report the same object. Sources
    /// not listed rank below all listed ones.
    #[serde(default)]
    pub priority: Vec<SourceId>,
}

impl SourceList {
    /// Whether the list names no source.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty() && self.priority.is_empty()
    }
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
        for list in [&pack.observations, &pack.airspace] {
            let mut seen = BTreeSet::new();
            for source in &list.sources {
                if !seen.insert(source) {
                    return Err(RegionError::Duplicate {
                        region: pack.id.clone(),
                        source_id: source.clone(),
                    });
                }
            }
            for source in &list.priority {
                if !seen.contains(source) {
                    return Err(RegionError::PriorityNotActive {
                        region: pack.id.clone(),
                        source_id: source.clone(),
                    });
                }
            }
        }
        Ok(pack)
    }

    /// Checks that the build contains an adapter for every source the region
    /// names, of observations and of airspace alike. A region whose airspace
    /// the server could not collect must stop it at start, not leave pilots
    /// with a map that silently has none.
    pub fn check_against(&self, registry: &Registry) -> Result<(), RegionError> {
        let unknown = |source: &SourceId| RegionError::UnknownSource {
            region: self.id.clone(),
            source_id: source.clone(),
        };
        for source in &self.observations.sources {
            if registry.observations(source).is_none() {
                return Err(unknown(source));
            }
        }
        for source in &self.airspace.sources {
            if registry.airspace(source).is_none() {
                return Err(unknown(source));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use venti_sources::{
        AirspaceSource, Manifest, ObservationSource, Parsed, ParsedAirspace, PlanCtx, Request,
        SourceError,
    };

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

    /// An adapter that offers both kinds and never fetches anything.
    struct Fake(Manifest);

    impl Fake {
        fn new(id: &str) -> Self {
            let text = format!(
                r#"
                id = "{id}"
                provider = "Example"
                kinds = ["observations", "airspace"]
                countries = ["AT"]
                cadence_seconds = 86400
                auth = "none"

                [licence]
                name = "CC BY 4.0"
                commercial = true
                redistribution = true
                share_alike = false
                attribution = "Example"
                link = "https://example.org"
                verified_on = "2026-11-19"
                verified_from = ["https://example.org/licence"]
                "#
            );
            Self(Manifest::from_toml(&text).unwrap())
        }
    }

    impl ObservationSource for Fake {
        fn manifest(&self) -> &Manifest {
            &self.0
        }
        fn plan(&self, _: &PlanCtx) -> Vec<Request> {
            Vec::new()
        }
        fn parse(&self, _: &Request, _: &[u8], _: &PlanCtx) -> Result<Parsed, SourceError> {
            Ok(Parsed::default())
        }
    }

    impl AirspaceSource for Fake {
        fn manifest(&self) -> &Manifest {
            &self.0
        }
        fn plan(&self, _: &PlanCtx) -> Vec<Request> {
            Vec::new()
        }
        fn parse(&self, _: &Request, _: &[u8], _: &PlanCtx) -> Result<ParsedAirspace, SourceError> {
            Ok(ParsedAirspace::default())
        }
    }

    const WITH_AIRSPACE: &str = r#"

        [airspace]
        sources = ["charts", "club-file"]
        priority = ["charts"]
    "#;

    #[test]
    fn a_region_may_name_airspace_sources_or_none() {
        let without = RegionPack::from_toml(PACK).unwrap();
        assert!(without.airspace.is_empty());

        let with = RegionPack::from_toml(&format!("{PACK}{WITH_AIRSPACE}")).unwrap();
        assert_eq!(with.airspace.sources.len(), 2);
        assert_eq!(with.airspace.priority[0].as_str(), "charts");

        // The same rules as for observations hold within the list.
        let twice = WITH_AIRSPACE.replace("club-file", "charts");
        assert!(matches!(
            RegionPack::from_toml(&format!("{PACK}{twice}")),
            Err(RegionError::Duplicate { .. })
        ));
        let stranger = WITH_AIRSPACE.replace(r#"priority = ["charts"]"#, r#"priority = ["one"]"#);
        assert!(matches!(
            RegionPack::from_toml(&format!("{PACK}{stranger}")),
            Err(RegionError::PriorityNotActive { .. })
        ));
    }

    #[test]
    fn airspace_sources_need_an_adapter_like_any_other() {
        let pack = RegionPack::from_toml(&format!("{PACK}{WITH_AIRSPACE}")).unwrap();

        // A build with observation adapters only cannot serve this region.
        let mut registry = Registry::new();
        for id in ["one", "two"] {
            registry
                .register_observations(Box::new(Fake::new(id)))
                .unwrap();
        }
        match pack.check_against(&registry) {
            Err(RegionError::UnknownSource { source_id, .. }) => {
                assert_eq!(source_id.as_str(), "charts");
            }
            other => panic!("{other:?}"),
        }
        // A region without airspace asks for no airspace adapter.
        let without = RegionPack::from_toml(PACK).unwrap();
        assert!(without.check_against(&registry).is_ok());

        // An adapter for one of its two airspace sources is not enough.
        registry
            .register_airspace(Box::new(Fake::new("charts")))
            .unwrap();
        match pack.check_against(&registry) {
            Err(RegionError::UnknownSource { source_id, .. }) => {
                assert_eq!(source_id.as_str(), "club-file");
            }
            other => panic!("{other:?}"),
        }
        registry
            .register_airspace(Box::new(Fake::new("club-file")))
            .unwrap();
        assert!(pack.check_against(&registry).is_ok());
    }
}
