//! Adapter for France's orthophotos from IGN's Géoplateforme.
//!
//! IGN serves photographs from above of all of France at 20 cm and finer
//! (BD ORTHO and its predecessors per area) as tiles of the Web Mercator
//! scheme ([`URL_TEMPLATE`]), under the Licence Ouverte. From zoom 13 its
//! tiles are white beyond the edge of the photographs: wholly, for a tile
//! over Courmayeur, or missing, for one over Turin; and the photographs
//! themselves reach some kilometres beyond the border before the white
//! begins. A map that lays them over another country's photographs would
//! paint France's, and white, over Geneva and the Aosta valley, and a map
//! style cannot cut a raster along a line. So the tiles go through the
//! Venti server, as basemap.at's do, and the server asks this crate the
//! same three things about each:
//!
//! - **Whether it is passed on at all** ([`Ign::covers`]): its zoom is
//!   [`MIN_ZOOM`] to [`MAX_ZOOM`] and it reaches into the box of
//!   metropolitan France ([`SERVICE_BOX`]).
//! - **Where it lies against France's border** ([`Ign::classify`]): wholly
//!   inside, wholly outside, or on the edge, which is within [`MARGIN_M`]
//!   of the border. A tile wholly outside is never asked for.
//! - **What of a tile on the edge may be shown** ([`Ign::cut`]): inside
//!   France everything, outside the margin nothing, and in the band between
//!   everything but the provider's white.
//!
//! The outline's index and the cut are `venti-outline`'s
//! ([`venti_outline::outline`], [`venti_outline::cut`]), which the
//! Austrian adapter cuts with too: one rule for both countries. What is
//! this crate's own is the provider's
//! address, its zooms, and France's outline (`assets/france.geojson`), made
//! from IGN's ADMIN EXPRESS COG of 2026, under the Licence Ouverte like the
//! photographs; the manifest names its source. It is read once, the first
//! time it is needed ([`france`]).
//!
//! **The coarse zooms are not passed on.** Up to zoom 12 the provider's
//! layer shows the ground beyond France as well, which is not IGN's aerial
//! photography and whose licence was not found. From zoom 13 the layer is
//! the photographs alone. A map shows another mosaic below that.
//!
//! **The sea and Lake Geneva.** The outline is France's land as its
//! regions have it, which ends at the shore of the sea and of Lake Geneva.
//! So there the photographs end within the margin of the shore, and the
//! water is another layer's.
//!
//! Like every adapter this crate does no I/O and reads no clock.

pub mod fixtures;

use std::sync::{Arc, OnceLock};

use venti_outline::cut;
use venti_outline::mercator::Rect;
use venti_outline::outline::Side;
use venti_sources::{Coverage, CutTile, ImagerySource, Manifest, SourceError, TileAddress};

pub use venti_outline::{Outline, OutlineError};

/// Where the service has its tiles, with the zoom, the column and the row
/// as the WMTS names them. Everything but the three numbers is fixed: the
/// layer, its style, the tile scheme and the format. From the service's
/// capabilities, read on 2026-11-22.
pub const URL_TEMPLATE: &str = "https://data.geopf.fr/wmts?SERVICE=WMTS&REQUEST=GetTile&VERSION=1.0.0&LAYER=ORTHOIMAGERY.ORTHOPHOTOS&STYLE=normal&TILEMATRIXSET=PM_0_19&TILEMATRIX={z}&TILECOL={x}&TILEROW={y}&FORMAT=image/jpeg";

/// The coarsest zoom that is passed on. The service has tiles from zoom 0,
/// but up to zoom 12 they show more than IGN's photographs of France; see
/// the crate's description.
pub const MIN_ZOOM: u8 = 13;

/// The finest zoom the service has. A tile there is about 53 m wide in the
/// French Alps, at about 21 cm a pixel.
pub const MAX_ZOOM: u8 = 19;

/// The box of metropolitan France, in degrees: west, south, east, north.
/// It is the box of the outline, with Corsica and the islands, rounded
/// outward. The service's own capabilities name the whole world, for the
/// overseas departments and the coarse zooms, none of which is passed on.
pub const SERVICE_BOX: [f64; 4] = [-5.1413, 41.3335, 9.56, 51.0892];

/// How far on each side of France's outline the provider's white decides
/// what is kept, in metres: as Austria's. The outline is simplified by 5 m
/// along the Alps and by 15 m elsewhere, and what its source is off by is
/// not stated; with 50 m an outline that is off never lets white through.
pub const MARGIN_M: f64 = 50.0;

/// France's outline, with a band of [`MARGIN_M`], read from the crate's
/// asset the first time it is asked for.
///
/// # Panics
///
/// Panics if the asset compiled into the crate is not an outline, which
/// the crate's tests rule out.
pub fn france() -> Arc<Outline> {
    static FRANCE: OnceLock<Arc<Outline>> = OnceLock::new();
    FRANCE
        .get_or_init(|| {
            let text = include_str!("../assets/france.geojson");
            Arc::new(
                Outline::from_geojson(text, MARGIN_M)
                    .expect("the outline compiled into venti-sources-ign is an outline"),
            )
        })
        .clone()
}

/// IGN's orthophotos, cut at France's border.
#[derive(Debug, Clone)]
pub struct Ign {
    manifest: Manifest,
    outline: Arc<Outline>,
}

impl Default for Ign {
    fn default() -> Self {
        Self::new()
    }
}

impl Ign {
    /// The adapter, with France's outline.
    ///
    /// # Panics
    ///
    /// Panics if the manifest or the outline compiled into the crate is
    /// invalid, which the crate's tests rule out.
    pub fn new() -> Self {
        Self::with_outline(france())
    }

    /// The adapter with another outline in place of France's. For tests.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn with_outline(outline: Arc<Outline>) -> Self {
        let manifest = Manifest::from_toml(include_str!("../manifest.toml"))
            .expect("the manifest compiled into venti-sources-ign is valid");
        Self { manifest, outline }
    }

    /// The outline tiles are cut along.
    pub fn outline(&self) -> &Outline {
        &self.outline
    }

    /// Whether the tile is one that is passed on: its zoom is from
    /// [`MIN_ZOOM`] to [`MAX_ZOOM`] and it reaches into [`SERVICE_BOX`].
    pub fn covers(&self, tile: TileAddress) -> bool {
        let [west, south, east, north] = SERVICE_BOX;
        (MIN_ZOOM..=MAX_ZOOM).contains(&tile.z())
            && Rect::of_tile(tile).overlaps(Rect::of_degrees(west, south, east, north))
    }

    /// Where the tile lies against the outline: wholly inside, wholly
    /// outside, or on the edge. A tile that is not passed on is outside.
    pub fn classify(&self, tile: TileAddress) -> Coverage {
        if !self.covers(tile) {
            return Coverage::Outside;
        }
        match self.outline.side_of(Rect::of_tile(tile)) {
            Side::Inside => Coverage::Inside,
            Side::Outside => Coverage::Outside,
            Side::Edge => Coverage::Edge,
        }
    }

    /// The address of the tile at the service: the template with three
    /// numbers in it, and nothing else of anyone's.
    pub fn url(&self, tile: TileAddress) -> String {
        URL_TEMPLATE
            .replace("{z}", &tile.z().to_string())
            .replace("{x}", &tile.x().to_string())
            .replace("{y}", &tile.y().to_string())
    }

    /// Cuts a tile on the edge: see [`venti_outline::cut`].
    pub fn cut(&self, tile: TileAddress, jpeg: &[u8]) -> Result<CutTile, SourceError> {
        cut::cut(&self.outline, Rect::of_tile(tile), jpeg)
    }
}

impl ImagerySource for Ign {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn covers(&self, tile: TileAddress) -> bool {
        Ign::covers(self, tile)
    }

    fn coverage(&self, tile: TileAddress) -> Coverage {
        self.classify(tile)
    }

    fn url(&self, tile: TileAddress) -> String {
        Ign::url(self, tile)
    }

    fn media_type(&self) -> &'static str {
        "image/jpeg"
    }

    fn cut(&self, tile: TileAddress, body: &[u8]) -> Result<CutTile, SourceError> {
        Ign::cut(self, tile, body)
    }
}

#[cfg(test)]
mod tests {
    use venti_outline::mercator;
    use venti_sources::{Kind, LicenceFamily};

    use super::*;

    fn tile(z: u8, x: u32, y: u32) -> TileAddress {
        TileAddress::new(z, x, y).unwrap()
    }

    /// The tile at zoom `z` that holds a position.
    fn at(z: u8, lon: f64, lat: f64) -> TileAddress {
        let point = mercator::project(lon, lat);
        let n = f64::from(1u32 << z);
        tile(z, (point.u * n) as u32, (point.v * n) as u32)
    }

    #[test]
    fn the_manifest_is_valid_and_names_the_outline_it_is_cut_by() {
        let source = Ign::new();
        let manifest = ImagerySource::manifest(&source);
        assert_eq!(manifest.id.as_str(), "ign-ortho");
        assert_eq!(manifest.kinds, [Kind::Imagery]);
        assert_eq!(manifest.licence.attribution, "IGN – BD ORTHO®");
        assert_eq!(
            manifest.licence.link,
            "https://cartes.gouv.fr/rechercher-une-donnee/dataset/IGNF_BD-ORTHO"
        );
        assert_eq!(manifest.licence.family(), LicenceFamily::Open);
        assert_eq!(manifest.max_response_bytes(), 1024 * 1024);
        assert_eq!(source.max_tile_bytes(), 1024 * 1024);
        let [outline] = manifest.assets.as_slice() else {
            panic!("the manifest names {} assets", manifest.assets.len());
        };
        assert_eq!(outline.file, "assets/france.geojson");
        assert_eq!(outline.licence.family(), LicenceFamily::Open);
        assert!(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(&outline.file)
                .is_file()
        );
    }

    #[test]
    fn the_address_is_the_fixed_layer_with_three_numbers() {
        let url = Ign::new().url(tile(15, 16946, 11636));
        assert_eq!(
            url,
            "https://data.geopf.fr/wmts?SERVICE=WMTS&REQUEST=GetTile&VERSION=1.0.0&LAYER=ORTHOIMAGERY.ORTHOPHOTOS&STYLE=normal&TILEMATRIXSET=PM_0_19&TILEMATRIX=15&TILECOL=16946&TILEROW=11636&FORMAT=image/jpeg"
        );
        // Whatever the tile, the address differs from the template in
        // digits alone.
        for tile in [tile(13, 0, 0), tile(19, 524_287, 524_287), tile(24, 1, 2)] {
            let url = Ign::new().url(tile);
            let fixed = |text: &str| -> String {
                text.replace(|c: char| c.is_ascii_digit(), "")
                    .replace("{z}", "")
                    .replace("{x}", "")
                    .replace("{y}", "")
            };
            assert_eq!(fixed(&url), fixed(URL_TEMPLATE));
            assert!(url.starts_with("https://data.geopf.fr/wmts?"));
        }
    }

    #[test]
    fn zoom_13_to_19_inside_the_box_of_france_are_passed_on() {
        let source = Ign::new();
        assert!(source.covers(at(13, 6.129, 45.899)));
        assert!(source.covers(at(19, 6.129, 45.899)));
        for z in [0, 8, 11, 12, 20, 24] {
            assert!(!source.covers(at(z, 6.129, 45.899)), "zoom {z}");
            assert_eq!(source.classify(at(z, 6.129, 45.899)), Coverage::Outside);
        }
        // Turin and Geneva are in the box; Vienna, London, Madrid and La
        // Réunion are not.
        assert!(source.covers(at(15, 7.68, 45.07)));
        assert!(source.covers(at(15, 6.14, 46.20)));
        for (lon, lat) in [
            (16.37, 48.21),
            (-0.13, 51.51),
            (-3.70, 40.42),
            (55.45, -20.88),
        ] {
            assert!(!source.covers(at(15, lon, lat)), "{lon}, {lat}");
            assert_eq!(source.classify(at(15, lon, lat)), Coverage::Outside);
        }
    }

    #[test]
    fn frances_outline_puts_the_places_on_their_side() {
        let france = france();
        for (name, lon, lat) in [
            ("Annecy", 6.129, 45.899),
            ("Chamonix", 6.869, 45.924),
            ("Saint-Hilaire-du-Touvet", 5.886, 45.307),
            ("Menton", 7.50, 43.78),
            ("Paris", 2.35, 48.86),
            ("Brest", -4.49, 48.39),
            ("Strasbourg", 7.75, 48.58),
            ("Ajaccio", 8.74, 41.93),
            ("Belle-Île", -3.19, 47.33),
            ("Bourg-Madame", 1.944, 42.435),
        ] {
            assert!(france.contains_position(lon, lat), "{name}");
        }
        for (name, lon, lat) in [
            ("Courmayeur", 6.97, 45.79),
            ("Geneva", 6.14, 46.20),
            ("Turin", 7.68, 45.07),
            ("Monaco", 7.42, 43.735),
            ("Ventimiglia", 7.61, 43.79),
            ("Llívia, Spain's town inside France", 1.98, 42.465),
            ("Andorra la Vella", 1.52, 42.51),
            ("Kehl", 7.82, 48.57),
            ("Jersey", -2.13, 49.21),
            ("the sea off Nice", 7.30, 43.60),
            ("Lake Geneva off Évian, which no commune holds", 6.60, 46.42),
        ] {
            assert!(!france.contains_position(lon, lat), "{name}");
        }
        // 112,398 positions, of which each of the 310 rings repeats its
        // first.
        assert_eq!(france.segment_count(), 112_088);
    }

    #[test]
    fn a_tile_is_inside_outside_or_on_the_edge() {
        let source = Ign::new();
        assert_eq!(source.classify(at(15, 6.129, 45.899)), Coverage::Inside);
        assert_eq!(source.classify(at(13, 2.35, 48.86)), Coverage::Inside);
        assert_eq!(source.classify(at(15, 6.97, 45.79)), Coverage::Outside);
        assert_eq!(source.classify(at(19, 7.68, 45.07)), Coverage::Outside);
        // In Llívia, the hole: outside, and its rim an edge.
        assert_eq!(source.classify(at(17, 1.98, 42.465)), Coverage::Outside);
        // The summit of Mont Blanc and the shore at Menton are edges.
        assert_eq!(source.classify(at(15, 6.865, 45.833)), Coverage::Edge);
        assert_eq!(source.classify(at(15, 7.5296, 43.7855)), Coverage::Edge);
    }

    #[test]
    fn what_is_not_a_jpeg_is_refused() {
        let source = Ign::new();
        let tile = at(15, 6.865, 45.833);
        assert!(source.cut(tile, b"<ExceptionReport/>").is_err());
        assert!(source.cut(tile, &[]).is_err());
    }
}
