use std::collections::BTreeMap;

use venti_domain::SourceId;

use crate::{AirspaceSource, ForecastSource, Kind, Manifest, ObservationSource, SiteSource};

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
/// are resolved to code. There is one list per kind of data. A provider that
/// offers two kinds registers once for each, under one identifier.
#[derive(Default)]
pub struct Registry {
    observations: BTreeMap<SourceId, Box<dyn ObservationSource>>,
    airspace: BTreeMap<SourceId, Box<dyn AirspaceSource>>,
    forecasts: BTreeMap<SourceId, Box<dyn ForecastSource>>,
    sites: BTreeMap<SourceId, Box<dyn SiteSource>>,
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
        let id = declared(source.manifest(), Kind::Observations)?;
        if self.observations.contains_key(&id) {
            return Err(RegistryError::Duplicate(id));
        }
        self.observations.insert(id, source);
        Ok(())
    }

    /// Adds an airspace source.
    pub fn register_airspace(
        &mut self,
        source: Box<dyn AirspaceSource>,
    ) -> Result<(), RegistryError> {
        let id = declared(source.manifest(), Kind::Airspace)?;
        if self.airspace.contains_key(&id) {
            return Err(RegistryError::Duplicate(id));
        }
        self.airspace.insert(id, source);
        Ok(())
    }

    /// Adds a forecast source.
    pub fn register_forecasts(
        &mut self,
        source: Box<dyn ForecastSource>,
    ) -> Result<(), RegistryError> {
        let id = declared(source.manifest(), Kind::Forecasts)?;
        if self.forecasts.contains_key(&id) {
            return Err(RegistryError::Duplicate(id));
        }
        self.forecasts.insert(id, source);
        Ok(())
    }

    /// Adds a source of flying sites.
    pub fn register_sites(&mut self, source: Box<dyn SiteSource>) -> Result<(), RegistryError> {
        let id = declared(source.manifest(), Kind::Sites)?;
        if self.sites.contains_key(&id) {
            return Err(RegistryError::Duplicate(id));
        }
        self.sites.insert(id, source);
        Ok(())
    }

    /// The observation source with this identifier, if the build contains it.
    pub fn observations(&self, id: &SourceId) -> Option<&dyn ObservationSource> {
        self.observations.get(id).map(Box::as_ref)
    }

    /// The airspace source with this identifier, if the build contains it.
    pub fn airspace(&self, id: &SourceId) -> Option<&dyn AirspaceSource> {
        self.airspace.get(id).map(Box::as_ref)
    }

    /// The forecast source with this identifier, if it is registered.
    ///
    /// A forecast source is a service somebody has to run, and its adapter
    /// is registered only by a server that was told where the service is.
    pub fn forecasts(&self, id: &SourceId) -> Option<&dyn ForecastSource> {
        self.forecasts.get(id).map(Box::as_ref)
    }

    /// The source of flying sites with this identifier, if the build
    /// contains it.
    pub fn sites(&self, id: &SourceId) -> Option<&dyn SiteSource> {
        self.sites.get(id).map(Box::as_ref)
    }

    /// The declared facts about the source with this identifier, if the
    /// build contains an adapter of it for this kind of data.
    pub fn manifest(&self, id: &SourceId, kind: Kind) -> Option<&Manifest> {
        match kind {
            Kind::Observations => self.observations(id).map(ObservationSource::manifest),
            Kind::Airspace => self.airspace(id).map(AirspaceSource::manifest),
            Kind::Forecasts => self.forecasts(id).map(ForecastSource::manifest),
            Kind::Sites => self.sites(id).map(SiteSource::manifest),
            // Imagery is not collected, and its sources are not in the
            // registry: the server asks for a tile when a client does.
            Kind::Imagery => None,
        }
    }

    /// The manifests of every registered source, sorted by identifier. A
    /// source registered for two kinds is listed once.
    pub fn manifests(&self) -> impl Iterator<Item = &Manifest> {
        let mut manifests: BTreeMap<&SourceId, &Manifest> = BTreeMap::new();
        let all = self
            .observations
            .values()
            .map(|source| source.manifest())
            .chain(self.airspace.values().map(|source| source.manifest()))
            .chain(self.forecasts.values().map(|source| source.manifest()))
            .chain(self.sites.values().map(|source| source.manifest()));
        for manifest in all {
            manifests.entry(&manifest.id).or_insert(manifest);
        }
        manifests.into_values()
    }
}

/// The identifier of a source, if its manifest lists the kind it is being
/// registered for.
fn declared(manifest: &Manifest, kind: Kind) -> Result<SourceId, RegistryError> {
    if manifest.kinds.contains(&kind) {
        Ok(manifest.id.clone())
    } else {
        Err(RegistryError::KindNotDeclared(manifest.id.clone()))
    }
}
