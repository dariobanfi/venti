//! Every adapter this build contains.
//!
//! This module is the only place in the workspace that names a provider
//! crate. Adding a provider means adding its crate and two lines here.

use venti_domain::{GeoPoint, Timestamp};
use venti_sources::testing::FixtureFetch;
use venti_sources::{ForecastSource, Registry};
use venti_sources_dwd::Dwd10Min;
use venti_sources_geosphere::Tawes10Min;
use venti_sources_openaip::OpenAip;
use venti_sources_openmeteo::OpenMeteo;
use venti_sources_osm::OsmFreeFlying;

/// What whoever runs the server tells it about the services behind some of
/// its sources.
///
/// Most providers are at an address the adapter knows. Two are not. An
/// Overpass API instance is one of several, and its operators ask that the
/// choice be left to whoever runs an application. And forecasts come from
/// an Open-Meteo instance that whoever runs the server runs themself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Providers {
    /// The interpreter of the Overpass API instance that is asked for the
    /// flying sites of OpenStreetMap.
    pub overpass_url: String,
    /// The address of the Open-Meteo instance that is asked for forecasts.
    /// `None` means the server has no forecasts. There is no default, and
    /// in particular it is never Open-Meteo's own public service.
    pub open_meteo_url: Option<String>,
}

impl Default for Providers {
    /// The main public Overpass instance, and no forecasts.
    fn default() -> Self {
        Self {
            overpass_url: venti_sources_osm::DEFAULT_INTERPRETER.to_owned(),
            open_meteo_url: None,
        }
    }
}

impl Providers {
    /// Every address whoever runs the server gave it for a provider.
    ///
    /// The fetcher asks no address without encryption unless it is one of
    /// these and was written with `http://`: a service kept beside the
    /// server, on a network of its own. An address added to this struct
    /// belongs in this list, or its `http://` form is refused at the first
    /// run.
    pub fn addresses(&self) -> Vec<&str> {
        std::iter::once(self.overpass_url.as_str())
            .chain(self.open_meteo_url.as_deref())
            .collect()
    }

    /// The addresses the recorded payloads answer for. The development
    /// mode and the tests run on these.
    pub fn recorded() -> Self {
        Self {
            overpass_url: venti_sources_osm::fixtures::INTERPRETER.to_owned(),
            open_meteo_url: Some(venti_sources_openmeteo::fixtures::BASE.to_owned()),
        }
    }
}

/// What is wrong with an address given for the Overpass instance, if
/// anything is.
pub fn check_overpass_url(url: &str) -> Result<(), String> {
    OsmFreeFlying::new(url).map(|_| ())
}

/// What is wrong with an address given for the Open-Meteo instance, if
/// anything is.
pub fn check_open_meteo_url(url: &str) -> Result<(), String> {
    OpenMeteo::new(url).map(|_| ())
}

/// The adapters for point forecasts, made for the addresses in `providers`:
/// none when the server was not told where its forecast service is.
///
/// The forecast service asks these when a place is asked for. The registry
/// holds adapters of its own for the same sources, made by
/// [`registry_for`].
pub fn forecast_sources(providers: &Providers) -> Result<Vec<Box<dyn ForecastSource>>, String> {
    let Some(url) = &providers.open_meteo_url else {
        return Ok(Vec::new());
    };
    let open_meteo = OpenMeteo::new(url)
        .map_err(|problem| format!("the address of the Open-Meteo instance: {problem}"))?;
    Ok(vec![Box::new(open_meteo)])
}

/// The registry of every adapter, for a server that was told nothing about
/// its providers: see [`registry_for`].
///
/// # Panics
///
/// Panics if two adapters claim one identifier or an adapter's manifest does
/// not list its kind; the tests of this module rule both out.
pub fn registry() -> Registry {
    registry_for(&Providers::default()).expect("the default addresses are addresses")
}

/// The registry of every adapter, with the adapters that need to be told
/// where their provider is made for the addresses in `providers`.
///
/// Returns what is wrong with an address if one cannot be used.
///
/// # Panics
///
/// Panics if two adapters claim one identifier or an adapter's manifest does
/// not list its kind; the tests of this module rule both out.
pub fn registry_for(providers: &Providers) -> Result<Registry, String> {
    let mut registry = Registry::new();
    registry
        .register_observations(Box::new(Dwd10Min::new()))
        .expect("dwd-10min registers once");
    registry
        .register_observations(Box::new(Tawes10Min::new()))
        .expect("geosphere-tawes registers once");
    registry
        .register_airspace(Box::new(OpenAip::new()))
        .expect("openaip registers once");
    let sites = OsmFreeFlying::new(&providers.overpass_url)
        .map_err(|problem| format!("the address of the Overpass instance: {problem}"))?;
    registry
        .register_sites(Box::new(sites))
        .expect("osm-free-flying registers once");
    for forecasts in forecast_sources(providers)? {
        registry
            .register_forecasts(forecasts)
            .expect("open-meteo registers once");
    }
    Ok(registry)
}

/// The payloads recorded from every provider, as a fetcher that answers
/// from them instead of the network. The development mode runs on these,
/// with the adapters made for [`Providers::recorded`].
pub fn recorded_payloads() -> FixtureFetch {
    FixtureFetch::new()
        .with_all(venti_sources_dwd::fixtures::recorded())
        .with_all(venti_sources_geosphere::fixtures::recorded())
        .with_all(venti_sources_openaip::fixtures::recorded())
        .with_all(venti_sources_osm::fixtures::recorded())
        .with_all(venti_sources_openmeteo::fixtures::recorded())
}

/// The places the recorded forecasts are for. The development mode has a
/// forecast for these and no others.
pub fn recorded_forecast_places() -> Vec<GeoPoint> {
    venti_sources_openmeteo::fixtures::places()
}

/// The moment the development mode's clock stands at for forecasts: when
/// they were recorded, a day after the observations.
///
/// A forecast cannot be served as fetched before its model ran, so the
/// forecasts do not share the clock of [`recorded_at`].
pub fn forecasts_recorded_at() -> Timestamp {
    venti_sources_openmeteo::fixtures::recorded_at()
}

/// The moment the development mode's clock stands at: the latest recording
/// time of the observations.
///
/// Wind is what ages by the minute, so the clock stays with the wind. The
/// airspace files and the flying sites were recorded the day after; both
/// change slowly, and the development mode serves them as fetched at this
/// moment.
pub fn recorded_at() -> Timestamp {
    venti_sources_dwd::fixtures::recorded_at().max(venti_sources_geosphere::fixtures::recorded_at())
}

#[cfg(test)]
mod tests {
    use venti_regions::RegionPack;
    use venti_sources::Kind;

    use super::*;

    #[test]
    fn every_shipped_region_pack_is_valid_for_this_build() {
        let regions = concat!(env!("CARGO_MANIFEST_DIR"), "/../../regions");
        let registry = registry();
        let mut checked = 0;
        for entry in std::fs::read_dir(regions).unwrap() {
            let path = entry.unwrap().path().join("region.toml");
            let text = std::fs::read_to_string(&path).unwrap();
            let pack =
                RegionPack::from_toml(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            pack.check_against(&registry)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            checked += 1;
        }
        assert!(checked > 0, "no region packs found");
    }

    #[test]
    fn every_adapter_is_registered_for_what_its_manifest_offers() {
        let registry = registry();
        for manifest in registry.manifests() {
            for kind in &manifest.kinds {
                let registered = match kind {
                    Kind::Observations => registry.observations(&manifest.id).is_some(),
                    Kind::Airspace => registry.airspace(&manifest.id).is_some(),
                    Kind::Forecasts => registry.forecasts(&manifest.id).is_some(),
                    Kind::Sites => registry.sites(&manifest.id).is_some(),
                };
                assert!(registered, "{} is not registered for {kind:?}", manifest.id);
            }
        }
        // A server that was told nothing has no forecast source.
        assert_eq!(registry.manifests().count(), 4);
        assert!(forecast_sources(&Providers::default()).unwrap().is_empty());
    }

    #[test]
    fn forecasts_are_there_only_when_the_server_is_told_where_from() {
        let told = registry_for(&Providers::recorded()).unwrap();
        assert_eq!(told.manifests().count(), 5);
        let open_meteo = venti_domain::SourceId::new("open-meteo").unwrap();
        let manifest = told.manifest(&open_meteo, Kind::Forecasts).unwrap();
        assert_eq!(manifest.kinds, [Kind::Forecasts]);
        assert_eq!(forecast_sources(&Providers::recorded()).unwrap().len(), 1);
        // No adapter knows an address of its own, least of all that of
        // Open-Meteo's public service.
        assert!(!registry().manifests().any(|m| m.id == open_meteo));
    }

    #[test]
    fn an_address_that_cannot_be_one_is_refused_by_name() {
        let providers = Providers {
            overpass_url: "overpass-api.de".to_owned(),
            ..Providers::default()
        };
        let Err(problem) = registry_for(&providers) else {
            panic!("an address without a scheme was accepted");
        };
        assert!(problem.contains("Overpass"), "{problem}");

        let providers = Providers {
            open_meteo_url: Some("open-meteo:8080".to_owned()),
            ..Providers::default()
        };
        let Err(problem) = registry_for(&providers) else {
            panic!("an address without a scheme was accepted");
        };
        assert!(problem.contains("Open-Meteo"), "{problem}");
        assert!(check_open_meteo_url("http://open-meteo:8080").is_ok());
        // Every address the server was given is one the fetcher is told of:
        // an instance of the operator's own may be asked without encryption.
        let providers = Providers {
            open_meteo_url: Some("http://open-meteo:8080".to_owned()),
            ..Providers::default()
        };
        assert_eq!(
            providers.addresses(),
            [
                "https://overpass-api.de/api/interpreter",
                "http://open-meteo:8080"
            ]
        );
        assert_eq!(Providers::default().addresses().len(), 1);

        // The recorded payloads answer for the addresses they were made for.
        assert!(registry_for(&Providers::recorded()).is_ok());
    }
}
