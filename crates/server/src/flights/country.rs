//! The country a flight took off in, for the flag beside it.
//!
//! Worked out with every answer, as the take-off's site is, so that a
//! better outline changes every flight at once. It is also kept with the
//! flight's analysis ([`super::derive`]) so that flights can be searched by
//! it; a better outline then needs the analysis version raised, so that the
//! refresh writes it anew. The outlines are [`adapters::country_outlines`]: the
//! finer ones first, Austria's (BEV, CC BY 4.0) and France's (IGN, Licence
//! Ouverte 2.0), which the server has for cutting aerial photographs at a
//! border, then Natural Earth's of every other country of Europe (public
//! domain), drawn for 1:10 000 000. Outside Europe a take-off has none.
//!
//! Only the outline itself counts, without the band of a margin the
//! photographs are cut with: a take-off a few metres over a border may be
//! given the wrong country, as the finer outlines are drawn for 1:50 000,
//! and one a few hundred metres over a border of Natural Earth's too.

use std::sync::{Arc, LazyLock};

use venti_domain::GeoPoint;
use venti_sources_basemapat::Outline;

use crate::adapters;

static COUNTRIES: LazyLock<Vec<(&'static str, Arc<Outline>)>> =
    LazyLock::new(adapters::country_outlines);

/// The country a place lies in, of those the server has an outline of.
pub fn country_at(place: GeoPoint) -> Option<&'static str> {
    COUNTRIES
        .iter()
        .find(|(_, outline)| outline.contains_position(place.lon(), place.lat()))
        .map(|&(code, _)| code)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(lat: f64, lon: f64) -> Option<&'static str> {
        country_at(GeoPoint::new(lat, lon).unwrap())
    }

    #[test]
    fn a_take_off_is_in_the_country_whose_outline_holds_it() {
        // The Hohe Salve above Hopfgarten, Tyrol.
        assert_eq!(at(47.4644, 12.1675), Some("AT"));
        // Planfait above Annecy.
        assert_eq!(at(45.8964, 6.2178), Some("FR"));
        // The Brauneck above Lenggries, Bavaria, by Natural Earth's.
        assert_eq!(at(47.6650, 11.5236), Some("DE"));
        // Monte Grappa, Italy.
        assert_eq!(at(45.8728, 11.7967), Some("IT"));
        // Fiesch, Valais.
        assert_eq!(at(46.4087, 8.1356), Some("CH"));
        // The Atlantic.
        assert_eq!(at(39.0, -12.0), None);
    }

    #[test]
    fn the_finer_outline_decides_at_the_border() {
        // Kufstein, by the Bavarian border: Austria's own outline holds
        // it, whatever Natural Earth's coarser one says.
        assert_eq!(at(47.5833, 12.1700), Some("AT"));
        // Oberaudorf across the Inn, in Bavaria.
        assert_eq!(at(47.6480, 12.1730), Some("DE"));
    }
}
