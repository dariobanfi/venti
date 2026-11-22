//! A country's outline on the map, and the cut of a map tile along it.
//!
//! Some providers' photographs from above come as JPEG tiles that are
//! white wherever the photograph ends, beyond the provider's border, or
//! that show a neighbour's ground there. A map that lays them over another
//! country's photographs needs them to end at the border, and a map style
//! cannot cut a raster along a line. What the adapters of such providers
//! share is here, written once:
//!
//! - [`mercator`]: the arithmetic of Web Mercator tiles;
//! - [`outline`]: an outline with a band around it and an index, which
//!   says where a tile and each of its pixels lie;
//! - [`cut`]: the rule that removes from a tile what lies beyond the
//!   outline, and the provider's white near it.
//!
//! Whose outline it is, where the tiles come from and which zooms there
//! are is each adapter's own (`venti-sources-basemapat` for Austria,
//! `venti-sources-ign` for France). This crate knows no provider, does no
//! I/O and reads no clock.

pub mod cut;
pub mod mercator;
pub mod outline;

pub use outline::{Outline, OutlineError};
