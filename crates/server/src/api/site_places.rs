//! One place shown once where several sources describe it.
//!
//! The FFVL's list, OpenStreetMap and Venti's own layers describe many of
//! the same take-offs and landings. Each record stays its source's: the
//! store keeps every source's set whole, each record answers at its own
//! address, and no field of one is copied into another (the licence rules
//! of `docs/data-sources.md`). What an answer of `GET /v1/sites` shows of a
//! place several sources describe is one of their records, the one whose
//! source comes first in the server's order, and it names the others by
//! identifier in its layer's `same_place` (owner decision 22).
//!
//! **The same place** (the owner's to change; the decision log in
//! `docs/architecture.md`, 2026-11-22, has the measurement):
//!
//! - *Of different sources.* Two records of one source are two places,
//!   however near: the source says so. Two take-offs of one mountain, a
//!   north and a south one, stand 50 to 400 m apart.
//! - *No farther apart than [`SAME_PLACE_M`].*
//! - *For the same use:* the two share a use (take-off, landing, winch,
//!   training), or neither names one. A landing field below a take-off is
//!   another place, however near.
//! - *One to one.* A record is left out for one record of another source at
//!   most, and a shown record stands for one record of each other source
//!   at most: of several candidates the nearest pair is taken first. So
//!   two take-offs that one source has and another source has as one stay
//!   two.
//!
//! **Which record is shown:** the one whose source stands first in the
//! order. The order is Venti's partners' layer (`venti`, CC0, what Venti
//! holds itself), then the regions' `priority` of their sources of sites
//! (the FFVL's list before OpenStreetMap: a federation's list of the places
//! it has agreements for, under a licence without share-alike), then the
//! crowdsourced layer (`venti-pilots`), which owner decision 14 makes the
//! less official. A source that the order does not name is resolved with
//! no other, and all its records go out.

use std::collections::{BTreeMap, BTreeSet};

use venti_api_types::v1::{
    SAME_PLACE_M, SamePlace, SiteLayer, VENTI_PILOTS_SITES_SOURCE, VENTI_SITES_SOURCE,
};
use venti_domain::{Site, SiteId, SiteUse, SourceId};

/// The order in which sources are shown where they describe the same
/// place: Venti's partners' layer, the regions' order of their sources of
/// sites, the crowdsourced layer.
pub(crate) fn order(regions: &[SourceId]) -> Vec<SourceId> {
    let own = |name: &str| SourceId::new(name).expect("the layers' names are identifiers");
    let mut order = vec![own(VENTI_SITES_SOURCE)];
    for source in regions {
        if !order.contains(source) {
            order.push(source.clone());
        }
    }
    order.push(own(VENTI_PILOTS_SITES_SOURCE));
    order
}

/// Whether two records are for the same use: they share one, or neither
/// names one.
fn same_use(a: &Site, b: &Site) -> bool {
    let named = |site: &Site| -> BTreeSet<SiteUse> {
        site.uses
            .iter()
            .copied()
            .filter(|used_for| *used_for != SiteUse::Other)
            .collect()
    };
    let (a, b) = (named(a), named(b));
    if a.is_empty() && b.is_empty() {
        return true;
    }
    !a.is_disjoint(&b)
}

/// Shows each place once: leaves out of `layers` the records that a
/// record of a source earlier in `order` stands for, and names them in
/// that record's layer's `same_place`. Returns how many were left out.
pub(crate) fn resolve(layers: &mut [SiteLayer], order: &[SourceId]) -> usize {
    let rank = |source: &SourceId| order.iter().position(|named| named == source);
    // The layers the order names, first first.
    let mut ranked: Vec<usize> = (0..layers.len())
        .filter(|&i| rank(&layers[i].source).is_some())
        .collect();
    ranked.sort_by_key(|&i| rank(&layers[i].source));

    // The records shown so far, by layer and position, with the sources
    // each already stands for.
    let mut shown: Vec<(usize, usize)> = Vec::new();
    let mut stands_for: BTreeMap<SiteId, BTreeSet<SiteId>> = BTreeMap::new();
    let mut left_out: BTreeSet<(usize, usize)> = BTreeSet::new();

    for &layer in &ranked {
        let source = layers[layer].source.clone();
        // Every pair of a record of this layer and a record shown before,
        // near enough and for the same use, nearest first.
        let mut pairs: Vec<(f64, usize, (usize, usize))> = Vec::new();
        for (index, site) in layers[layer].sites.iter().enumerate() {
            for &(other_layer, other_index) in &shown {
                let other = &layers[other_layer].sites[other_index];
                // A degree of latitude is 111 km everywhere: what is
                // farther north or south than that is never near, and a
                // box of 5,000 sites is checked without a distance for
                // every pair.
                if (site.position.lat() - other.position.lat()).abs() > SAME_PLACE_M / 111_000.0 {
                    continue;
                }
                let apart = site.position.distance_m(other.position);
                if apart <= SAME_PLACE_M && same_use(site, other) {
                    pairs.push((apart, index, (other_layer, other_index)));
                }
            }
        }
        pairs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        let mut taken: BTreeSet<usize> = BTreeSet::new();
        for (_, index, (other_layer, other_index)) in pairs {
            if taken.contains(&index) {
                continue;
            }
            let other_id = layers[other_layer].sites[other_index].id.clone();
            let already = stands_for.entry(other_id).or_default();
            if already.iter().any(|id| id.source() == &source) {
                continue;
            }
            already.insert(layers[layer].sites[index].id.clone());
            taken.insert(index);
            left_out.insert((layer, index));
        }
        for index in 0..layers[layer].sites.len() {
            if !taken.contains(&index) {
                shown.push((layer, index));
            }
        }
    }

    for (layer, entry) in layers.iter_mut().enumerate() {
        let keep = |index: usize| !left_out.contains(&(layer, index));
        // A layer of Venti's own has one standing for each site, in the
        // same order: it loses the same entries.
        if entry.standing.len() == entry.sites.len() {
            let mut index = 0;
            entry.standing.retain(|_| {
                index += 1;
                keep(index - 1)
            });
        }
        let mut index = 0;
        entry.sites.retain(|_| {
            index += 1;
            keep(index - 1)
        });
        entry.same_place = entry
            .sites
            .iter()
            .filter_map(|site| {
                let also = stands_for.get(&site.id).filter(|also| !also.is_empty())?;
                Some(SamePlace {
                    id: site.id.clone(),
                    also: also.iter().cloned().collect(),
                })
            })
            .collect();
    }
    left_out.len()
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use venti_domain::GeoPoint;

    use super::*;

    fn site(id: &str, lat: f64, lon: f64, uses: &[SiteUse]) -> Site {
        Site {
            id: id.parse().unwrap(),
            name: None,
            position: GeoPoint::new(lat, lon).unwrap(),
            elevation_m: None,
            uses: uses.to_vec(),
            wind_directions: Vec::new(),
            paragliding: None,
            hang_gliding: None,
            official: None,
            discouraged: false,
            url: None,
            website: None,
            description: None,
            fee: None,
            guest_guidelines: None,
            no_flight_time: None,
            outline: None,
        }
    }

    fn layer(source: &str, sites: Vec<Site>) -> SiteLayer {
        let then = Utc.with_ymd_and_hms(2026, 11, 22, 7, 0, 0).unwrap();
        SiteLayer {
            source: SourceId::new(source).unwrap(),
            fetched_at: then,
            as_of: then,
            covers: Vec::new(),
            sites,
            standing: Vec::new(),
            same_place: Vec::new(),
        }
    }

    fn ids(layer: &SiteLayer) -> Vec<String> {
        layer.sites.iter().map(|s| s.id.to_string()).collect()
    }

    fn sources() -> Vec<SourceId> {
        order(&[
            SourceId::new("ffvl-sites").unwrap(),
            SourceId::new("osm-free-flying").unwrap(),
        ])
    }

    const UP: &[SiteUse] = &[SiteUse::Takeoff];
    const DOWN: &[SiteUse] = &[SiteUse::Landing];

    // About 0.0009° of latitude is 100 m.
    #[test]
    fn a_take_off_of_two_sources_is_shown_once_as_the_first_sources() {
        let mut layers = vec![
            layer(
                "osm-free-flying",
                vec![site("osm-free-flying:node/1", 45.9009, 6.1, UP)],
            ),
            layer("ffvl-sites", vec![site("ffvl-sites:1", 45.9, 6.1, UP)]),
        ];
        assert_eq!(resolve(&mut layers, &sources()), 1);
        assert!(layers[0].sites.is_empty());
        assert_eq!(ids(&layers[1]), ["ffvl-sites:1"]);
        assert_eq!(
            layers[1].same_place,
            [SamePlace {
                id: "ffvl-sites:1".parse().unwrap(),
                also: vec!["osm-free-flying:node/1".parse().unwrap()],
            }]
        );
        assert!(layers[0].same_place.is_empty());
    }

    #[test]
    fn too_far_another_use_or_one_source_are_two_places() {
        let mut layers = vec![
            layer(
                "osm-free-flying",
                vec![
                    // 200 m away.
                    site("osm-free-flying:node/1", 45.9018, 6.1, UP),
                    // As near, but a landing.
                    site("osm-free-flying:node/2", 45.9001, 6.1, DOWN),
                ],
            ),
            layer(
                "ffvl-sites",
                vec![
                    site("ffvl-sites:1", 45.9, 6.1, UP),
                    // Two of one source, 10 m apart, stay two.
                    site("ffvl-sites:2", 45.90009, 6.1, UP),
                ],
            ),
        ];
        assert_eq!(resolve(&mut layers, &sources()), 0);
        assert_eq!(layers[0].sites.len() + layers[1].sites.len(), 4);
        assert!(layers.iter().all(|l| l.same_place.is_empty()));
    }

    #[test]
    fn the_nearest_pair_is_taken_first_and_one_stands_for_one() {
        // One FFVL take-off, two of OpenStreetMap near it: the nearer is
        // the same place, the other stays.
        let mut layers = vec![
            layer(
                "osm-free-flying",
                vec![
                    site("osm-free-flying:node/1", 45.9010, 6.1, UP),
                    site("osm-free-flying:node/2", 45.9002, 6.1, UP),
                ],
            ),
            layer("ffvl-sites", vec![site("ffvl-sites:1", 45.9, 6.1, UP)]),
        ];
        assert_eq!(resolve(&mut layers, &sources()), 1);
        assert_eq!(ids(&layers[0]), ["osm-free-flying:node/1"]);
        assert_eq!(
            layers[1].same_place[0].also,
            ["osm-free-flying:node/2".parse::<SiteId>().unwrap()]
        );
    }

    #[test]
    fn venti_comes_first_the_pilots_last_and_an_unnamed_source_is_left_alone() {
        let mut layers = vec![
            layer("ffvl-sites", vec![site("ffvl-sites:1", 45.9, 6.1, UP)]),
            layer(
                "osm-free-flying",
                vec![site("osm-free-flying:node/1", 45.9, 6.1, UP)],
            ),
            layer("venti", vec![site("venti:a", 45.9001, 6.1, UP)]),
            layer(
                "venti-pilots",
                vec![site("venti-pilots:b", 45.9002, 6.1, UP)],
            ),
            layer("elsewhere", vec![site("elsewhere:x", 45.9, 6.1, UP)]),
        ];
        assert_eq!(resolve(&mut layers, &sources()), 3);
        assert_eq!(ids(&layers[2]), ["venti:a"]);
        let also: Vec<String> = layers[2].same_place[0]
            .also
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            also,
            ["ffvl-sites:1", "osm-free-flying:node/1", "venti-pilots:b"]
        );
        assert_eq!(ids(&layers[4]), ["elsewhere:x"]);
    }

    #[test]
    fn places_that_name_no_use_are_one_and_a_use_is_not_none() {
        assert!(same_use(
            &site("a:1", 45.9, 6.1, &[]),
            &site("b:1", 45.9, 6.1, &[])
        ));
        assert!(!same_use(
            &site("a:1", 45.9, 6.1, &[]),
            &site("b:1", 45.9, 6.1, UP)
        ));
        assert!(same_use(
            &site("a:1", 45.9, 6.1, &[SiteUse::Takeoff, SiteUse::Landing]),
            &site("b:1", 45.9, 6.1, DOWN)
        ));
    }
}
