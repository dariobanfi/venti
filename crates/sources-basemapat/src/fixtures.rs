//! Six tiles recorded from the service on 2026-11-20.
//!
//! `tests/fixtures/NOTICE.txt` says where each lies and how they were
//! recorded. The photographs are basemap.at's, under CC BY 4.0:
//! "Datenquelle: basemap.at".
//!
//! The crate's own tests cut them and compare the result with golden
//! files. A server's development mode answers from them, so that a client
//! sees the aerial view of Austria, cut, without the network.

use venti_sources::TileAddress;

/// One recorded tile.
#[derive(Debug, Clone, Copy)]
pub struct Recorded {
    /// A short name: where it is.
    pub name: &'static str,
    /// Its address.
    pub tile: TileAddress,
    /// What the service answered.
    pub jpeg: &'static [u8],
}

macro_rules! recorded {
    ($name:literal, $z:literal, $x:literal, $y:literal) => {
        Recorded {
            name: $name,
            tile: match TileAddress::new($z, $x, $y) {
                Some(tile) => tile,
                None => panic!("a recorded tile has an address"),
            },
            jpeg: include_bytes!(concat!(
                "../tests/fixtures/",
                $name,
                "_",
                $z,
                "_",
                $x,
                "_",
                $y,
                ".jpeg"
            )),
        }
    };
}

/// Every recorded tile.
pub fn recorded() -> Vec<Recorded> {
    vec![
        recorded!("inside_innsbruck", 15, 17420, 11489),
        recorded!("outside_munich", 10, 544, 355),
        recorded!("border_achenpass", 12, 2181, 1430),
        recorded!("border_achenpass", 15, 17448, 11446),
        recorded!("border_achenpass", 19, 279183, 183141),
        recorded!("border_brenner", 15, 17431, 11524),
    ]
}
