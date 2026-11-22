//! Seven tiles recorded from the service on 2026-11-22.
//!
//! `tests/fixtures/NOTICE.txt` says where each lies and how they were
//! recorded. The photographs are IGN's, under the Licence Ouverte:
//! "IGN – BD ORTHO®".
//!
//! The crate's own tests cut them and compare the result with golden
//! files. A server's development mode answers from them, so that a client
//! sees the aerial view of France, cut, without the network.

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
        recorded!("inside_annecy", 15, 16941, 11670),
        recorded!("outside_courmayeur", 15, 17018, 11685),
        recorded!("border_veyrier", 13, 4236, 2909),
        recorded!("border_veyrier", 15, 16946, 11636),
        recorded!("border_veyrier", 19, 271142, 186184),
        recorded!("border_montblanc", 15, 17008, 11679),
        recorded!("border_menton", 15, 17069, 11942),
    ]
}
