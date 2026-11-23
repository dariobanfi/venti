//! Natural Earth's outlines of the countries of Europe, in the public
//! domain, for the country a flight took off in.
//!
//! Nothing is fetched: the outlines are the crate's asset,
//! `assets/europe.geojson`, made by `tools/build_countries.py` from
//! Natural Earth's "Admin 0 - Countries" at 1:10m, with the manifest's
//! `[[asset]]` saying which release. Drawn for 1:10 000 000 and simplified
//! by about 100 m: a place close to a border may be given the neighbour's
//! country, which is why the server asks a finer outline first where it
//! has one.

use std::sync::{Arc, LazyLock};

use venti_outline::Outline;
use venti_sources::Manifest;

/// The band the outlines are built with, metres. The country of a place is
/// asked without it ([`Outline::contains_position`]); an outline needs one.
const MARGIN_M: f64 = 100.0;

/// The source's manifest.
///
/// # Panics
///
/// Panics if the manifest compiled into the crate is not one, which the
/// crate's tests rule out.
pub fn manifest() -> Manifest {
    Manifest::from_toml(include_str!("../manifest.toml"))
        .expect("the manifest compiled into venti-sources-naturalearth is a manifest")
}

/// Every country of the asset, by its ISO 3166-1 alpha-2 code, with its
/// outline, in the order of the codes. Read the first time it is asked for.
///
/// # Panics
///
/// Panics if the asset compiled into the crate is not a collection of
/// outlines with codes, which the crate's tests rule out.
pub fn countries() -> &'static [(String, Arc<Outline>)] {
    static COUNTRIES: LazyLock<Vec<(String, Arc<Outline>)>> = LazyLock::new(|| {
        let text = include_str!("../assets/europe.geojson");
        let collection: serde_json::Value = serde_json::from_str(text).expect("the asset is JSON");
        collection["features"]
            .as_array()
            .expect("the asset is a FeatureCollection")
            .iter()
            .map(|feature| {
                let code = feature["properties"]["code"]
                    .as_str()
                    .expect("every country has its code")
                    .to_owned();
                let outline = Outline::from_geojson(&feature.to_string(), MARGIN_M)
                    .unwrap_or_else(|error| panic!("the outline of {code}: {error}"));
                (code, Arc::new(outline))
            })
            .collect()
    });
    &COUNTRIES
}

/// The country a place lies in, of those of the asset.
pub fn country_at(lon: f64, lat: f64) -> Option<&'static str> {
    countries()
        .iter()
        .find(|(_, outline)| outline.contains_position(lon, lat))
        .map(|(code, _)| code.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_and_the_asset_are_read() {
        let manifest = manifest();
        assert_eq!(manifest.id.as_str(), "natural-earth");
        assert_eq!(manifest.assets.len(), 1);
        let codes: Vec<&str> = countries().iter().map(|(code, _)| code.as_str()).collect();
        for code in ["AT", "CH", "DE", "FR", "IT", "SI", "ES"] {
            assert!(codes.contains(&code), "{code}");
        }
        // Each country the manifest names is in the asset.
        for code in &manifest.countries {
            assert!(codes.contains(&code.as_str()), "{code}");
        }
    }

    #[test]
    fn a_take_off_is_in_its_country() {
        // The Brauneck above Lenggries, Bavaria.
        assert_eq!(country_at(11.5236, 47.6650), Some("DE"));
        // Monte Grappa, Italy.
        assert_eq!(country_at(11.7967, 45.8728), Some("IT"));
        // Fiesch, Valais.
        assert_eq!(country_at(8.1356, 46.4087), Some("CH"));
        // The Kobala above Tolmin, Slovenia.
        assert_eq!(country_at(13.6890, 46.1700), Some("SI"));
        // San Marino and the Vatican, holes in Italy's outline.
        assert_eq!(country_at(12.4578, 43.9424), Some("SM"));
        assert_eq!(country_at(12.4534, 41.9029), Some("VA"));
        assert_eq!(country_at(12.4964, 41.9028), Some("IT"));
        // The Atlantic west of Portugal.
        assert_eq!(country_at(-12.0, 39.0), None);
        // Istanbul, which Natural Earth puts in Asia with all of Turkey.
        assert_eq!(country_at(28.9784, 41.0082), None);
    }
}
