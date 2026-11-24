//! The Alpine avalanche warning micro-regions of the EAWS, under CC0 1.0:
//! the regions of the thermal forecast by region.
//!
//! Nothing is fetched: the regions are the crate's asset,
//! `assets/regions.geojson`, made by `tools/build_regions.py` from the
//! repository gitlab.com/eaws/eaws-regions, with the manifest's `[[asset]]`
//! saying which commit. Each region is a massif or a group of valleys as an
//! avalanche service splits its area, with its identifier ("AT-07-14-01"),
//! its names in German, Italian, English, French and Slovenian, a point
//! inside it for its label, and its outline simplified by about 100 m.
//!
//! A place is in the first region whose outline holds it
//! ([`region_at`]). The outlines of two neighbours are simplified each on
//! its own, so near a common border a place may lie in both or in
//! neither; the foreland is in none.

use std::sync::LazyLock;

use venti_sources::Manifest;

/// The asset, as it is: a GeoJSON `FeatureCollection` with a `version`.
const ASSET: &str = include_str!("../assets/regions.geojson");

/// A region's names. The repository's own fallback stands where a
/// language has no name of its own (most of the Slovenian ones are the
/// English).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Names {
    /// In German.
    pub de: String,
    /// In Italian.
    pub it: String,
    /// In English.
    pub en: String,
    /// In French.
    pub fr: String,
    /// In Slovenian.
    pub sl: String,
}

/// One micro-region.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    /// The EAWS identifier: the provider's code and the region's number,
    /// "AT-07-14-01", "CH-4115", "IT-32-BZ-05-01".
    pub id: String,
    /// Its names.
    pub names: Names,
    /// A point inside it for its label, (longitude, latitude), degrees:
    /// the point of its largest polygon furthest from the edges, as the
    /// tool found it.
    pub label: (f64, f64),
    /// Its polygons, each an outer ring and its holes, each ring a list
    /// of (longitude, latitude) whose last point repeats the first.
    pub polygons: Vec<Vec<Vec<(f64, f64)>>>,
    /// The box around it: west, south, east, north.
    bounds: (f64, f64, f64, f64),
}

impl Region {
    /// Whether the region holds a place: a line from it to the east
    /// crosses its rings an odd number of times.
    #[must_use]
    pub fn contains(&self, lon: f64, lat: f64) -> bool {
        let (west, south, east, north) = self.bounds;
        if lon < west || lon > east || lat < south || lat > north {
            return false;
        }
        let mut inside = false;
        for ring in self.polygons.iter().flatten() {
            for pair in ring.windows(2) {
                let ((ax, ay), (bx, by)) = (pair[0], pair[1]);
                if (ay > lat) != (by > lat) && lon < ax + (lat - ay) * (bx - ax) / (by - ay) {
                    inside = !inside;
                }
            }
        }
        inside
    }
}

/// The source's manifest.
///
/// # Panics
///
/// Panics if the manifest compiled into the crate is not one, which the
/// crate's tests rule out.
pub fn manifest() -> Manifest {
    Manifest::from_toml(include_str!("../manifest.toml"))
        .expect("the manifest compiled into venti-sources-eawsregions is a manifest")
}

/// The asset's version, "eaws-regions 439ec676 2026-11-24": the commit it
/// was made from and the day that chose the current regions. It changes
/// whenever the asset is made again, which is what a client that keeps
/// the outlines asks after.
///
/// # Panics
///
/// Panics if the asset has none, which the crate's tests rule out.
pub fn version() -> &'static str {
    static VERSION: LazyLock<String> = LazyLock::new(|| {
        let collection: serde_json::Value = serde_json::from_str(ASSET).expect("the asset is JSON");
        collection["version"]
            .as_str()
            .expect("the asset has a version")
            .to_owned()
    });
    &VERSION
}

/// Every region of the asset, in the order of their identifiers. Read the
/// first time it is asked for.
///
/// # Panics
///
/// Panics if the asset compiled into the crate is not a collection of
/// regions, which the crate's tests rule out.
pub fn regions() -> &'static [Region] {
    static REGIONS: LazyLock<Vec<Region>> = LazyLock::new(|| {
        let collection: serde_json::Value = serde_json::from_str(ASSET).expect("the asset is JSON");
        collection["features"]
            .as_array()
            .expect("the asset is a FeatureCollection")
            .iter()
            .map(region)
            .collect()
    });
    &REGIONS
}

/// One feature of the asset as a region.
fn region(feature: &serde_json::Value) -> Region {
    let properties = &feature["properties"];
    let id = properties["id"]
        .as_str()
        .expect("every region has its identifier")
        .to_owned();
    let name = |language: &str| {
        properties["names"][language]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };
    let pair = |value: &serde_json::Value| {
        let lon = value[0].as_f64();
        let lat = value[1].as_f64();
        lon.zip(lat)
            .unwrap_or_else(|| panic!("a position of {id} is two numbers"))
    };
    let polygons: Vec<Vec<Vec<(f64, f64)>>> = feature["geometry"]["coordinates"]
        .as_array()
        .unwrap_or_else(|| panic!("{id} is a MultiPolygon"))
        .iter()
        .map(|polygon| {
            polygon
                .as_array()
                .unwrap_or_else(|| panic!("a polygon of {id} is a list of rings"))
                .iter()
                .map(|ring| {
                    ring.as_array()
                        .unwrap_or_else(|| panic!("a ring of {id} is a list of positions"))
                        .iter()
                        .map(pair)
                        .collect()
                })
                .collect()
        })
        .collect();
    let mut bounds = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &(lon, lat) in polygons.iter().flatten().flatten() {
        bounds = (
            bounds.0.min(lon),
            bounds.1.min(lat),
            bounds.2.max(lon),
            bounds.3.max(lat),
        );
    }
    Region {
        label: pair(&properties["label"]),
        names: Names {
            de: name("de"),
            it: name("it"),
            en: name("en"),
            fr: name("fr"),
            sl: name("sl"),
        },
        id,
        polygons,
        bounds,
    }
}

/// The number in [`regions`] of the region a place lies in: the first
/// whose outline holds it. `None` outside every region.
pub fn region_at(lon: f64, lat: f64) -> Option<usize> {
    regions()
        .iter()
        .position(|region| region.contains(lon, lat))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_and_the_asset_are_read() {
        let manifest = manifest();
        assert_eq!(manifest.id.as_str(), "eaws-regions");
        assert_eq!(manifest.assets.len(), 1);
        assert_eq!(manifest.licence.attribution, "Regions: EAWS (CC0)");
        assert!(version().starts_with("eaws-regions "));
        let regions = regions();
        assert!(regions.len() > 400, "{}", regions.len());
        // In the order of the identifiers, each once.
        assert!(regions.windows(2).all(|pair| pair[0].id < pair[1].id));
        for region in regions {
            assert!(!region.names.en.is_empty() && !region.names.de.is_empty());
            // The label lies inside its own region and no other.
            let (lon, lat) = region.label;
            assert!(region.contains(lon, lat), "{}", region.id);
            assert_eq!(
                regions[region_at(lon, lat).unwrap()].id,
                region.id,
                "{}",
                region.id
            );
        }
    }

    #[test]
    fn a_take_off_is_in_its_region() {
        let id = |lon, lat| region_at(lon, lat).map(|at| regions()[at].id.as_str());
        // The Brauneck above Lenggries: the Bavarian Prealps.
        assert!(id(11.5236, 47.6650).is_some_and(|id| id.starts_with("DE-BY-")));
        // Fiesch, Valais.
        assert!(id(8.1356, 46.4087).is_some_and(|id| id.starts_with("CH-")));
        // The Kobala above Tolmin, Slovenia.
        assert!(id(13.6890, 46.1700).is_some_and(|id| id.starts_with("SI")));
        // Monte Grappa, the Veneto's Prealps.
        assert!(id(11.7967, 45.8728).is_some_and(|id| id.starts_with("IT-34-")));
        // Munich, the Po plain at Milan, Zurich: the foreland is in none.
        assert_eq!(id(11.5761, 48.1374), None);
        assert_eq!(id(9.1900, 45.4642), None);
    }
}
