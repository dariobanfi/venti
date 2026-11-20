//! Map imagery: raster tiles that a client asks the server for one at a
//! time.
//!
//! Imagery is not collected ahead and holds no records. A client's map asks
//! for a tile by its address in the Web Mercator scheme every web map uses
//! (`z/x/y`, with `y` counted from the north), and the server asks the
//! provider for that tile, or answers that there is nothing there. An
//! adapter of this kind says which tiles its provider has, where each is,
//! and what of a tile may go out: some providers send more than their
//! licence or their coverage gives, and the adapter cuts it away.
//!
//! Like every adapter it does no I/O and reads no clock. The server fetches,
//! keeps and answers ([`ImagerySource`] says what it asks of an adapter).

use std::fmt;

use crate::{Manifest, SourceError};

/// The deepest zoom level a tile address may have: a tile there is about
/// 2 cm wide at the equator, far past any photograph.
pub const MAX_TILE_ZOOM: u8 = 24;

/// The address of one tile in the Web Mercator scheme: zoom, column from
/// the west, row from the north.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileAddress {
    z: u8,
    x: u32,
    y: u32,
}

impl TileAddress {
    /// The tile at zoom `z`, column `x` and row `y`, if there is one: the
    /// zoom is at most [`MAX_TILE_ZOOM`] and both numbers are below `2^z`.
    pub fn new(z: u8, x: u32, y: u32) -> Option<Self> {
        if z > MAX_TILE_ZOOM {
            return None;
        }
        let side = 1u32 << z;
        (x < side && y < side).then_some(Self { z, x, y })
    }

    /// The zoom level.
    pub fn z(self) -> u8 {
        self.z
    }

    /// The column, counted from the west.
    pub fn x(self) -> u32 {
        self.x
    }

    /// The row, counted from the north.
    pub fn y(self) -> u32 {
        self.y
    }
}

impl fmt::Display for TileAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}/{}", self.z, self.x, self.y)
    }
}

/// Where a tile lies with respect to what a source may show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    /// Wholly inside: the provider's tile goes out as it is.
    Inside,
    /// Wholly outside, or not a tile the provider has: nothing goes out,
    /// and the provider is not asked.
    Outside,
    /// The edge runs through the tile or near it: the provider's tile is
    /// cut before it goes out.
    Edge,
}

/// What is left of a tile after cutting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cut {
    /// Nothing was removed: the provider's bytes go out as they came.
    Unchanged,
    /// Some was removed: the rest, as a PNG with transparency.
    Png(Vec<u8>),
    /// Everything was removed: there is nothing here.
    Nothing,
}

/// How many pixels of a tile were kept and removed, and why.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CutCounts {
    /// Pixels kept as the provider sent them.
    pub kept: u32,
    /// Pixels removed because they lie outside the edge and its margin.
    pub removed_outside: u32,
    /// Pixels removed because they lie near the edge and are the provider's
    /// white.
    pub removed_white: u32,
}

/// A tile after cutting, with the counts of its pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutTile {
    /// What goes out.
    pub cut: Cut,
    /// What was kept and removed.
    pub counts: CutCounts,
}

/// One provider's map imagery.
///
/// The server calls these in this order for a tile a client asks for:
/// [`covers`](Self::covers) and [`coverage`](Self::coverage) before it asks
/// the provider anything, then [`url`](Self::url), and for a tile on the
/// edge [`cut`](Self::cut) on what came back.
pub trait ImagerySource: Send + Sync {
    /// The declared facts about the source. Its kinds include
    /// [`Kind::Imagery`](crate::Kind::Imagery).
    fn manifest(&self) -> &Manifest;

    /// Whether the provider serves this tile at all: its zoom is in the
    /// provider's range and the tile reaches into the provider's box. A
    /// tile it does not cover is never asked for.
    fn covers(&self, tile: TileAddress) -> bool;

    /// Where the tile lies with respect to what may be shown.
    fn coverage(&self, tile: TileAddress) -> Coverage;

    /// The address the provider serves the tile at.
    fn url(&self, tile: TileAddress) -> String;

    /// The media type of what [`url`](Self::url) answers, such as
    /// `image/jpeg`.
    fn media_type(&self) -> &'static str;

    /// The most bytes one tile of the provider's may have. A larger answer
    /// is refused.
    fn max_tile_bytes(&self) -> usize {
        self.manifest().max_response_bytes()
    }

    /// Cuts a tile on the edge: removes what may not be shown.
    fn cut(&self, tile: TileAddress, body: &[u8]) -> Result<CutTile, SourceError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_a_tile_that_exists() {
        assert!(TileAddress::new(0, 0, 0).is_some());
        assert!(TileAddress::new(0, 1, 0).is_none());
        assert!(TileAddress::new(1, 1, 1).is_some());
        assert!(TileAddress::new(1, 2, 0).is_none());
        let deepest = (1u32 << MAX_TILE_ZOOM) - 1;
        assert!(TileAddress::new(MAX_TILE_ZOOM, deepest, deepest).is_some());
        assert!(TileAddress::new(MAX_TILE_ZOOM + 1, 0, 0).is_none());
        let tile = TileAddress::new(15, 17448, 11446).unwrap();
        assert_eq!((tile.z(), tile.x(), tile.y()), (15, 17448, 11446));
        assert_eq!(tile.to_string(), "15/17448/11446");
    }
}
