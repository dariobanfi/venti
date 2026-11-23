//! The country a flight took off in, for the flag beside it.
//!
//! Worked out with every answer, as the take-off's site is, so that a
//! better outline changes every flight at once and nothing is stored that
//! could go stale. The outlines are the ones the server has already, for
//! cutting aerial photographs at a border: Austria's (BEV, CC BY 4.0) and
//! France's (IGN, Licence Ouverte 2.0), each a crate's asset. A take-off
//! in any other country has none; a public-domain outline of every country
//! would fill that in (`docs/data-platform.md`, "Public flights as data").
//!
//! Only the outline itself counts, without the band of a margin the
//! photographs are cut with: a take-off a few metres over a border may be
//! given the wrong one or none, as the outline is drawn for 1:50 000.

use std::sync::{Arc, LazyLock};

use venti_domain::GeoPoint;
use venti_sources_basemapat::Outline;

/// Every country an outline is had for, by its ISO 3166-1 alpha-2 code.
static COUNTRIES: LazyLock<Vec<(&'static str, Arc<Outline>)>> = LazyLock::new(|| {
    vec![
        ("AT", venti_sources_basemapat::austria()),
        ("FR", venti_sources_ign::france()),
    ]
});

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
        // The Brauneck above Lenggries, Bavaria: no outline of Germany.
        assert_eq!(at(47.6650, 11.5236), None);
        // Monte Grappa, Italy.
        assert_eq!(at(45.8728, 11.7967), None);
    }
}
