//! Adapter for Austria's orthophoto from basemap.at.
//!
//! basemap.at serves photographs from above of all of Austria at 29 cm and
//! finer, as tiles of the Web Mercator scheme ([`URL_TEMPLATE`]), under CC
//! BY 4.0. Its tiles reach beyond the border, and there they are white:
//! wholly, for a tile over Munich, or in part, for one on the border. A map
//! that lays them over another photograph would paint white over Bavaria,
//! South Tyrol and the Engadin, and a map style cannot cut a raster along a
//! line. So the tiles go through the Venti server, which asks this crate
//! three things about each:
//!
//! - **Whether the provider has it** ([`BasemapAt::covers`]): its zoom is
//!   0 to 19 and it reaches into the service's box ([`SERVICE_BOX`]).
//!   Outside the box the service answers 404.
//! - **Where it lies against Austria's border** ([`BasemapAt::classify`]):
//!   wholly inside, wholly outside, or on the edge, which is within
//!   [`MARGIN_M`] of the border. A tile wholly outside is never asked for.
//! - **What of a tile on the edge may be shown** ([`BasemapAt::cut`]):
//!   inside Austria everything, outside the margin nothing, and in the band
//!   between everything but the provider's white. See [`cut`] for the rule.
//!
//! Austria's outline is an asset of this crate (`assets/austria.geojson`),
//! made from the Federal Office of Metrology and Surveying's
//! administrative boundaries at 1:50 000, CC BY 4.0, like the photographs;
//! the manifest names its source. It is read once, the first time it is
//! needed ([`austria`]).
//!
//! Like every adapter this crate does no I/O and reads no clock.

pub mod fixtures;

use std::sync::{Arc, OnceLock};

use venti_sources::{Coverage, CutTile, ImagerySource, Manifest, SourceError, TileAddress};

use venti_outline::cut;
use venti_outline::mercator::Rect;
use venti_outline::outline::Side;

pub use venti_outline::{Outline, OutlineError};

/// Where the service has its tiles: zoom, row and column in this order,
/// the row before the column. From the service's capabilities, read on
/// 2026-11-20; basemap.at's changelog says the addresses stand until the
/// end of 2027.
pub const URL_TEMPLATE: &str =
    "https://mapsneu.wien.gv.at/basemap/bmaporthofoto30cm/normal/google3857/{z}/{y}/{x}.jpeg";

/// The coarsest zoom the service has.
pub const MIN_ZOOM: u8 = 0;

/// The finest zoom the service has. A tile there is about 52 m wide in
/// Austria, at about 20 cm a pixel.
pub const MAX_ZOOM: u8 = 19;

/// The box of the service's tiles, in degrees: west, south, east, north.
/// From the service's capabilities (`WGS84BoundingBox`).
pub const SERVICE_BOX: [f64; 4] = [8.782379, 46.358770, 17.5, 49.037872];

/// How far on each side of Austria's outline the provider's white decides
/// what is kept, in metres. The outline is drawn for 1:50 000 and may be
/// 10 to 25 m off in places; with 50 m an outline that is off never lets
/// white through, and a photograph that reaches over the line is cut
/// where its white begins.
pub const MARGIN_M: f64 = 50.0;

/// Austria's outline, with a band of [`MARGIN_M`], read from the crate's
/// asset the first time it is asked for.
///
/// # Panics
///
/// Panics if the asset compiled into the crate is not an outline, which
/// the crate's tests rule out.
pub fn austria() -> Arc<Outline> {
    static AUSTRIA: OnceLock<Arc<Outline>> = OnceLock::new();
    AUSTRIA
        .get_or_init(|| {
            let text = include_str!("../assets/austria.geojson");
            Arc::new(
                Outline::from_geojson(text, MARGIN_M)
                    .expect("the outline compiled into venti-sources-basemapat is an outline"),
            )
        })
        .clone()
}

/// The orthophoto of basemap.at, cut at Austria's border.
#[derive(Debug, Clone)]
pub struct BasemapAt {
    manifest: Manifest,
    outline: Arc<Outline>,
}

impl Default for BasemapAt {
    fn default() -> Self {
        Self::new()
    }
}

impl BasemapAt {
    /// The adapter, with Austria's outline.
    ///
    /// # Panics
    ///
    /// Panics if the manifest or the outline compiled into the crate is
    /// invalid, which the crate's tests rule out.
    pub fn new() -> Self {
        Self::with_outline(austria())
    }

    /// The adapter with another outline in place of Austria's. For tests.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn with_outline(outline: Arc<Outline>) -> Self {
        let manifest = Manifest::from_toml(include_str!("../manifest.toml"))
            .expect("the manifest compiled into venti-sources-basemapat is valid");
        Self { manifest, outline }
    }

    /// The outline tiles are cut along.
    pub fn outline(&self) -> &Outline {
        &self.outline
    }

    /// Whether the service has the tile: its zoom is from [`MIN_ZOOM`] to
    /// [`MAX_ZOOM`] and it reaches into [`SERVICE_BOX`].
    pub fn covers(&self, tile: TileAddress) -> bool {
        let [west, south, east, north] = SERVICE_BOX;
        (MIN_ZOOM..=MAX_ZOOM).contains(&tile.z())
            && Rect::of_tile(tile).overlaps(Rect::of_degrees(west, south, east, north))
    }

    /// Where the tile lies against the outline: wholly inside, wholly
    /// outside, or on the edge. A tile the service does not have is
    /// outside.
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

    /// The address of the tile at the service.
    pub fn url(&self, tile: TileAddress) -> String {
        URL_TEMPLATE
            .replace("{z}", &tile.z().to_string())
            .replace("{y}", &tile.y().to_string())
            .replace("{x}", &tile.x().to_string())
    }

    /// Cuts a tile on the edge: see [`cut`].
    pub fn cut(&self, tile: TileAddress, jpeg: &[u8]) -> Result<CutTile, SourceError> {
        cut::cut(&self.outline, Rect::of_tile(tile), jpeg)
    }
}

impl ImagerySource for BasemapAt {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn covers(&self, tile: TileAddress) -> bool {
        BasemapAt::covers(self, tile)
    }

    fn coverage(&self, tile: TileAddress) -> Coverage {
        self.classify(tile)
    }

    fn url(&self, tile: TileAddress) -> String {
        BasemapAt::url(self, tile)
    }

    fn media_type(&self) -> &'static str {
        "image/jpeg"
    }

    fn cut(&self, tile: TileAddress, body: &[u8]) -> Result<CutTile, SourceError> {
        BasemapAt::cut(self, tile, body)
    }
}

#[cfg(test)]
mod tests {
    use venti_outline::mercator;
    use venti_sources::{Cut, Kind, LicenceFamily};

    use super::*;

    fn tile(z: u8, x: u32, y: u32) -> TileAddress {
        TileAddress::new(z, x, y).unwrap()
    }

    /// The eastern side of [`small`]: seven tenths of the way into its
    /// tile at zoom 15 from the west, eight tenths at 17, two tenths at 19.
    const EAST: f64 = 11.5104;

    /// A square of about 23 by 15 km around Innsbruck, with a hole, in
    /// place of Austria.
    fn small() -> BasemapAt {
        let outer = vec![(11.2, 47.2), (EAST, 47.2), (EAST, 47.335), (11.2, 47.335)];
        let hole = vec![(11.3, 47.25), (11.36, 47.25), (11.36, 47.28), (11.3, 47.28)];
        BasemapAt::with_outline(Arc::new(Outline::new(&[outer, hole], MARGIN_M).unwrap()))
    }

    /// The tile at zoom `z` that holds a position.
    fn at(z: u8, lon: f64, lat: f64) -> TileAddress {
        let point = mercator::project(lon, lat);
        let n = f64::from(1u32 << z);
        tile(z, (point.u * n) as u32, (point.v * n) as u32)
    }

    #[test]
    fn the_manifest_is_valid_and_names_the_outline_it_is_cut_by() {
        let source = small();
        let manifest = ImagerySource::manifest(&source);
        assert_eq!(manifest.id.as_str(), "basemap-at");
        assert_eq!(manifest.kinds, [Kind::Imagery]);
        assert_eq!(manifest.licence.attribution, "Datenquelle: basemap.at");
        assert_eq!(manifest.licence.link, "https://basemap.at/");
        assert_eq!(manifest.licence.family(), LicenceFamily::Open);
        assert_eq!(manifest.max_response_bytes(), 1024 * 1024);
        assert_eq!(source.max_tile_bytes(), 1024 * 1024);
        let [outline] = manifest.assets.as_slice() else {
            panic!("the manifest names {} assets", manifest.assets.len());
        };
        assert_eq!(outline.file, "assets/austria.geojson");
        assert_eq!(outline.licence.family(), LicenceFamily::Open);
        assert!(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(&outline.file)
                .is_file()
        );
    }

    #[test]
    fn the_address_has_the_row_before_the_column() {
        assert_eq!(
            small().url(tile(15, 17448, 11446)),
            "https://mapsneu.wien.gv.at/basemap/bmaporthofoto30cm/normal/google3857/15/11446/17448.jpeg"
        );
    }

    #[test]
    fn the_service_has_zoom_0_to_19_inside_its_box() {
        let source = small();
        assert!(source.covers(tile(0, 0, 0)));
        assert!(source.covers(at(19, 11.39, 47.27)));
        assert!(!source.covers(at(20, 11.39, 47.27).to_owned()));
        // Bolzano and Munich are in the box; Zurich, Milan and Prague not.
        assert!(source.covers(at(15, 11.35, 46.50)));
        assert!(source.covers(at(15, 11.57, 48.14)));
        for (lon, lat) in [(8.54, 47.37), (9.19, 45.46), (14.42, 50.08)] {
            assert!(!source.covers(at(12, lon, lat)), "{lon}, {lat}");
            assert_eq!(source.classify(at(12, lon, lat)), Coverage::Outside);
        }
        // A tile beyond the service's zooms is outside, wherever it is.
        assert_eq!(source.classify(at(20, 11.25, 47.3)), Coverage::Outside);
        assert_eq!(source.classify(at(23, 11.25, 47.3)), Coverage::Outside);
    }

    #[test]
    fn a_tile_is_inside_outside_or_on_the_edge() {
        let source = small();
        assert_eq!(source.classify(at(15, 11.25, 47.3)), Coverage::Inside);
        assert_eq!(source.classify(at(15, 11.0, 47.3)), Coverage::Outside);
        assert_eq!(source.classify(at(15, EAST, 47.3)), Coverage::Edge);
        // In the hole, an enclave: outside, and its rim an edge.
        assert_eq!(source.classify(at(17, 11.33, 47.265)), Coverage::Outside);
        assert_eq!(source.classify(at(17, 11.36, 47.265)), Coverage::Edge);
        // A tile at zoom 19 on the border.
        assert_eq!(source.classify(at(19, EAST, 47.3)), Coverage::Edge);
        // Coarse tiles hold the whole outline.
        assert_eq!(source.classify(tile(0, 0, 0)), Coverage::Edge);
        assert_eq!(source.classify(at(8, 11.3, 47.3)), Coverage::Edge);
    }

    /// A tile's pixels as the provider's JPEG would decode to: 256 pixels
    /// square, each of the colour `paint` gives for its middle, as
    /// longitude and latitude.
    fn paint(tile: TileAddress, paint: impl Fn(f64, f64) -> [u8; 3]) -> Vec<u8> {
        const SIDE: usize = 256;
        let rect = Rect::of_tile(tile);
        let mut rgb = Vec::with_capacity(SIDE * SIDE * 3);
        for j in 0..SIDE {
            let v = rect.v0 + (j as f64 + 0.5) / SIDE as f64 * (rect.v1 - rect.v0);
            for i in 0..SIDE {
                let u = rect.u0 + (i as f64 + 0.5) / SIDE as f64 * (rect.u1 - rect.u0);
                rgb.extend(paint(mercator::longitude(u), mercator::latitude(v)));
            }
        }
        rgb
    }

    /// Cuts painted pixels as [`BasemapAt::cut`] cuts a decoded JPEG.
    fn cut_painted(source: &BasemapAt, tile: TileAddress, rgb: &[u8]) -> CutTile {
        cut::cut_rgb(source.outline(), Rect::of_tile(tile), rgb, 256, 256).unwrap()
    }

    const GREEN: [u8; 3] = [60, 110, 40];
    const WHITE: [u8; 3] = [255, 255, 255];

    #[test]
    fn white_well_inside_is_kept() {
        // A glacier: a white patch in the middle of a tile on the edge.
        let source = small();
        let tile = at(15, EAST, 47.3);
        let rect = Rect::of_tile(tile);
        let middle = mercator::longitude(rect.centre().u);
        let rgb = paint(tile, |lon, lat| {
            let glacier = (lon - (EAST - 0.004)).abs() < 0.0008 && (lat - 47.3).abs() < 0.0008;
            if lon > EAST || glacier { WHITE } else { GREEN }
        });
        let cut = cut_painted(&source, tile, &rgb);
        let Cut::Png(png) = &cut.cut else {
            panic!("{cut:?}");
        };
        let (width, rgba) = testing::decode_png(png);
        // The patch, about 300 m west of the border, is there and white.
        let px = |lon: f64, lat: f64| {
            let point = mercator::project(lon, lat);
            let i = ((point.u - rect.u0) / (rect.u1 - rect.u0) * width as f64) as usize;
            let j = ((point.v - rect.v0) / (rect.v1 - rect.v0) * width as f64) as usize;
            &rgba[(j * width + i) * 4..(j * width + i) * 4 + 4]
        };
        assert!(px(EAST - 0.004, 47.3)[0] > 240 && px(EAST - 0.004, 47.3)[3] == 255);
        // Green inside is kept, white beyond is gone.
        assert_eq!(px(EAST - 0.006, 47.3)[3], 255);
        assert_eq!(px(EAST + 0.002, 47.3)[3], 0);
        assert!(middle > EAST - 0.01 && middle < EAST + 0.01);
        assert!(cut.counts.kept > 0 && cut.counts.removed_outside > 0);
    }

    /// Whether the pixel of a cut tile at a position is kept.
    fn kept_at(cut: &CutTile, tile: TileAddress, lon: f64, lat: f64) -> bool {
        let Cut::Png(png) = &cut.cut else {
            return cut.cut == Cut::Unchanged;
        };
        let (width, rgba) = testing::decode_png(png);
        let rect = Rect::of_tile(tile);
        let point = mercator::project(lon, lat);
        let i = ((point.u - rect.u0) / (rect.u1 - rect.u0) * width as f64) as usize;
        let j = ((point.v - rect.v0) / (rect.v1 - rect.v0) * width as f64) as usize;
        rgba[(j * width + i) * 4 + 3] == 255
    }

    /// A metre to the east, in degrees of longitude, at 47.3°.
    fn metres_east(metres: f64) -> f64 {
        metres / (111_320.0 * 47.3_f64.to_radians().cos())
    }

    #[test]
    fn white_in_the_band_is_removed_and_a_photograph_in_it_is_kept() {
        let source = small();
        // About 415 m wide, the outline 40 % of the way in from the west.
        let tile = at(16, EAST, 47.3);
        // The photograph ends 20 m inside the outline: from there the
        // provider's white begins, in the band on the inside.
        let white_from = EAST - metres_east(20.0);
        let image = paint(tile, |lon, _| if lon > white_from { WHITE } else { GREEN });
        let cut = cut_painted(&source, tile, &image);
        assert!(cut.counts.removed_white > 0, "{:?}", cut.counts);
        assert!(kept_at(&cut, tile, EAST - metres_east(30.0), 47.3));
        assert!(!kept_at(&cut, tile, EAST - metres_east(10.0), 47.3));
        assert!(!kept_at(&cut, tile, EAST + metres_east(10.0), 47.3));
        let Cut::Png(png) = &cut.cut else {
            panic!("{cut:?}");
        };
        let (_, rgba) = testing::decode_png(png);
        // No white pixel is left.
        assert!(
            !rgba
                .chunks_exact(4)
                .any(|p| p[3] == 255 && p[..3].iter().all(|&c| c >= cut::WHITE))
        );

        // The photograph reaches 70 m beyond the outline: in the band on
        // the outside it is kept, past the band it is not.
        let green_to = EAST + metres_east(70.0);
        let image = paint(tile, |lon, _| if lon < green_to { GREEN } else { WHITE });
        let cut = cut_painted(&source, tile, &image);
        assert!(kept_at(&cut, tile, EAST + metres_east(40.0), 47.3));
        assert!(!kept_at(&cut, tile, EAST + metres_east(60.0), 47.3));
        assert!(!kept_at(&cut, tile, EAST + metres_east(100.0), 47.3));
        assert!(cut.counts.removed_outside > 0);
    }

    #[test]
    fn a_tile_cut_to_nothing_or_not_at_all_says_so() {
        let source = small();
        // The next tile east of the outline at zoom 19, all white: the
        // band reaches into it, and nothing is left.
        let border = at(19, EAST, 47.3);
        let tile = TileAddress::new(19, border.x() + 1, border.y()).unwrap();
        assert_eq!(source.classify(tile), Coverage::Edge);
        let white = cut_painted(&source, tile, &paint(tile, |_, _| WHITE));
        assert_eq!(white.cut, Cut::Nothing);
        assert_eq!(white.counts.kept, 0);
        // A tile that lies inside, cut all the same: nothing is removed.
        let inside = at(17, 11.25, 47.3);
        let green = cut_painted(&source, inside, &paint(inside, |_, _| GREEN));
        assert_eq!(green.cut, Cut::Unchanged);
        assert_eq!(green.counts.kept, 256 * 256);
    }

    #[test]
    fn a_tile_at_zoom_19_on_the_border_is_cut_by_the_metre() {
        let source = small();
        let tile = at(19, EAST, 47.3);
        // About 52 m wide: the band is most of it.
        let image = paint(tile, |lon, _| if lon > EAST { WHITE } else { GREEN });
        let cut = cut_painted(&source, tile, &image);
        assert!(matches!(cut.cut, Cut::Png(_)), "{cut:?}");
        assert_eq!(
            cut.counts.kept + cut.counts.removed_white + cut.counts.removed_outside,
            256 * 256
        );
        assert!(cut.counts.removed_white > 0);
        assert!(cut.counts.kept > 0);
    }

    #[test]
    fn what_is_not_a_jpeg_is_refused() {
        let source = small();
        let tile = at(15, EAST, 47.3);
        assert!(source.cut(tile, b"<html>404 Not Found</html>").is_err());
        assert!(source.cut(tile, &[]).is_err());
    }

    #[test]
    fn austrias_outline_puts_the_places_on_their_side() {
        let austria = austria();
        for (lon, lat) in [
            (11.39, 47.27),
            (16.37, 48.21),
            (13.18, 46.75),
            (9.75, 47.50),
            (10.45, 47.57),
            (11.70, 47.53),
        ] {
            assert!(austria.contains_position(lon, lat), "{lon}, {lat}");
        }
        for (lon, lat) in [
            (11.57, 48.14),
            (11.35, 46.50),
            (11.76, 47.71),
            (9.52, 47.14),
            (10.36, 46.94),
        ] {
            assert!(!austria.contains_position(lon, lat), "{lon}, {lat}");
        }
        // 20,163 positions, of which each of the two rings repeats its first.
        assert_eq!(austria.segment_count(), 20_161);
    }

    #[test]
    fn far_from_the_border_a_tile_is_answered_from_the_index() {
        let source = BasemapAt::new();
        assert_eq!(source.classify(at(15, 11.39, 47.27)), Coverage::Inside);
        assert_eq!(source.classify(at(15, 16.37, 48.21)), Coverage::Inside);
        assert_eq!(source.classify(at(12, 11.57, 48.14)), Coverage::Outside);
        assert_eq!(source.classify(at(15, 11.35, 46.50)), Coverage::Outside);
        assert_eq!(source.classify(at(19, 10.36, 46.94)), Coverage::Outside);
        assert_eq!(source.classify(at(14, 10.45, 47.57)), Coverage::Edge);
    }
}

/// Helpers for tests.
#[cfg(test)]
pub(crate) mod testing {
    /// The width and the RGBA pixels of a PNG.
    pub fn decode_png(png: &[u8]) -> (usize, Vec<u8>) {
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        pixels.truncate(info.buffer_size());
        (info.width as usize, pixels)
    }
}
