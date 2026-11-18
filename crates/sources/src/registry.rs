use std::collections::BTreeMap;

use venti_domain::SourceId;

use crate::{Kind, Manifest, ObservationSource};

/// Why an adapter could not be registered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    /// Two adapters claim the same identifier.
    #[error("source {0} is registered twice")]
    Duplicate(SourceId),
    /// The adapter's manifest does not list the kind it was registered for.
    #[error("source {0} is registered for a kind its manifest does not list")]
    KindNotDeclared(SourceId),
}

/// Every adapter the build contains, looked up by identifier.
///
/// Region packs name sources by identifier; the registry is where those names
/// are resolved to code.
#[derive(Default)]
pub struct Registry {
    observations: BTreeMap<SourceId, Box<dyn ObservationSource>>,
}

impl Registry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an observation source.
    pub fn register_observations(
        &mut self,
        source: Box<dyn ObservationSource>,
    ) -> Result<(), RegistryError> {
        let manifest = source.manifest();
        let id = manifest.id.clone();
        if !manifest.kinds.contains(&Kind::Observations) {
            return Err(RegistryError::KindNotDeclared(id));
        }
        if self.observations.contains_key(&id) {
            return Err(RegistryError::Duplicate(id));
        }
        self.observations.insert(id, source);
        Ok(())
    }

    /// The observation source with this identifier, if the build contains it.
    pub fn observations(&self, id: &SourceId) -> Option<&dyn ObservationSource> {
        self.observations.get(id).map(Box::as_ref)
    }

    /// The manifests of every registered source, sorted by identifier.
    pub fn manifests(&self) -> impl Iterator<Item = &Manifest> {
        self.observations.values().map(|source| source.manifest())
    }
}
