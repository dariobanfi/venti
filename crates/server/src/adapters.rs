//! Every adapter this build contains.
//!
//! This module is the only place in the workspace that names a provider
//! crate. Adding a provider means adding its crate and two lines here.

use venti_domain::Timestamp;
use venti_sources::Registry;
use venti_sources::testing::FixtureFetch;
use venti_sources_dwd::Dwd10Min;
use venti_sources_geosphere::Tawes10Min;
use venti_sources_openaip::OpenAip;

/// The registry of every adapter.
///
/// # Panics
///
/// Panics if two adapters claim one identifier or an adapter's manifest does
/// not list its kind; the tests of this module rule both out.
pub fn registry() -> Registry {
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
    registry
}

/// The payloads recorded from every provider, as a fetcher that answers
/// from them instead of the network. The development mode runs on these.
pub fn recorded_payloads() -> FixtureFetch {
    FixtureFetch::new()
        .with_all(venti_sources_dwd::fixtures::recorded())
        .with_all(venti_sources_geosphere::fixtures::recorded())
        .with_all(venti_sources_openaip::fixtures::recorded())
}

/// The moment the development mode's clock stands at: the latest recording
/// time of the observations.
///
/// Wind is what ages by the minute, so the clock stays with the wind. The
/// airspace files were recorded the day after; airspace changes with the
/// month, and the development mode serves them as fetched at this moment.
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
                };
                assert!(registered, "{} is not registered for {kind:?}", manifest.id);
            }
        }
        assert_eq!(registry.manifests().count(), 3);
    }
}
