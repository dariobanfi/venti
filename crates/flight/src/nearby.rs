//! The wind around a place: what a weather model has in the cells near a
//! take-off, against what it has at the take-off itself.
//!
//! A foehn two valleys away, or a front that has not arrived, is in the
//! model's cells around a place before it is in the place's own. So the
//! strongest mean wind and the strongest gust near by are looked for
//! ([`cells_within`], [`strongest`]), and a fixed rule says when that is
//! much more than the place has ([`much_stronger`]).
//!
//! It informs and judges nothing: no hour's verdict and no day's follows
//! from it. A strong wind near by is not a strong wind here, and in the
//! mountains a cell on a ridge always has more wind than one in a valley.
//! The limits are our own, the owner's to change, and to verify against
//! days that were flown.

use venti_domain::GeoPoint;

use crate::day::{GUSTS_KMH, WIND_KMH};
use crate::geodesy::Earth;

/// How far around a place the model's cells count as near by, km. Two
/// valleys in the Alps. Ours, and the owner's to change.
pub const NEARBY_KM: f64 = 30.0;
/// By how much the strongest wind or gust near by has to be above the
/// place's own to be called much stronger, km/h. Ours, and the owner's to
/// change.
pub const MUCH_STRONGER_KMH: f64 = 15.0;
/// What the strongest mean wind near by has to reach to be worth a word,
/// km/h: the wind with which an hour at a place is not flyable
/// ([`WIND_KMH`]).
pub const NEARBY_WIND_KMH: f64 = WIND_KMH.1;
/// The same for the strongest gust ([`GUSTS_KMH`]).
pub const NEARBY_GUSTS_KMH: f64 = GUSTS_KMH.1;

/// Where a cell lies from a place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Whereabouts {
    /// How far, km.
    pub distance_km: f64,
    /// Which way from the place, degrees clockwise from north.
    pub bearing_deg: f64,
}

/// Where `cell` lies from `place`.
#[must_use]
pub fn whereabouts(place: GeoPoint, cell: GeoPoint) -> Whereabouts {
    let course = Earth::Wgs84.course(place, cell);
    Whereabouts {
        distance_km: course.distance_m / 1000.0,
        bearing_deg: course.bearing_deg,
    }
}

/// The cells within [`NEARBY_KM`] of a place, as their indices in `cells`,
/// the nearest first; of two equally far the one listed first.
#[must_use]
pub fn cells_within(place: GeoPoint, cells: &[GeoPoint]) -> Vec<usize> {
    let mut near: Vec<(usize, f64)> = cells
        .iter()
        .enumerate()
        .map(|(index, cell)| (index, whereabouts(place, *cell).distance_km))
        .filter(|(_, distance)| *distance <= NEARBY_KM)
        .collect();
    near.sort_by(|a, b| a.1.total_cmp(&b.1));
    near.into_iter().map(|(index, _)| index).collect()
}

/// The strongest of some cells' values, with its place in the list. Of
/// equally strong ones the first, which among cells listed the nearest
/// first is the nearest. `None` when no cell has a value.
#[must_use]
pub fn strongest(values: impl IntoIterator<Item = Option<u16>>) -> Option<(usize, u16)> {
    let mut found: Option<(usize, u16)> = None;
    for (index, value) in values.into_iter().enumerate() {
        if let Some(value) = value
            && found.is_none_or(|(_, most)| value > most)
        {
            found = Some((index, value));
        }
    }
    found
}

/// What of the neighbourhood's wind is much stronger than the place's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stronger {
    /// The gusts.
    Gusts,
    /// The mean wind.
    Wind,
}

/// Whether a figure near by is much stronger than the same figure at the
/// place: it reaches `from_kmh`, and it is at least [`MUCH_STRONGER_KMH`]
/// above the place's. Never without a figure for the place: what is not
/// known cannot be compared.
#[must_use]
pub fn is_much_stronger(here_kmh: Option<f64>, near_kmh: f64, from_kmh: f64) -> bool {
    near_kmh >= from_kmh && here_kmh.is_some_and(|here| near_kmh - here >= MUCH_STRONGER_KMH)
}

/// What the neighbourhood has much more of than the place in an hour: the
/// gusts, if the strongest gust near by reaches [`NEARBY_GUSTS_KMH`] and
/// is [`MUCH_STRONGER_KMH`] above the place's; else the mean wind, by
/// [`NEARBY_WIND_KMH`] in the same way. `None` when neither is.
///
/// `here` and `near` are the mean wind and the gusts, km/h, at the place
/// and the strongest of each near by.
#[must_use]
pub fn much_stronger(
    here: (Option<f64>, Option<f64>),
    near: (Option<f64>, Option<f64>),
) -> Option<Stronger> {
    let gusts = near
        .1
        .is_some_and(|gusts| is_much_stronger(here.1, gusts, NEARBY_GUSTS_KMH));
    let wind = near
        .0
        .is_some_and(|wind| is_much_stronger(here.0, wind, NEARBY_WIND_KMH));
    if gusts {
        Some(Stronger::Gusts)
    } else if wind {
        Some(Stronger::Wind)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).unwrap()
    }

    #[test]
    fn the_cells_near_by_are_those_within_the_distance_the_nearest_first() {
        let place = point(47.66, 11.8);
        let cells = [
            // 0.4° of latitude is 44 km: too far.
            point(47.2, 11.8),
            // 0.2° is 22 km.
            point(47.46, 11.8),
            point(47.7, 11.8),
            // 0.3° of longitude is 22.5 km here.
            point(47.66, 12.1),
            // 0.5° is 37 km.
            point(47.66, 12.3),
        ];

        assert_eq!(cells_within(place, &cells), [2, 1, 3]);

        let south = whereabouts(place, cells[1]);
        assert!((south.distance_km - 22.2).abs() < 0.2, "{south:?}");
        assert!((south.bearing_deg - 180.0).abs() < 0.1, "{south:?}");
        let east = whereabouts(place, cells[3]);
        assert!((east.distance_km - 22.5).abs() < 0.2, "{east:?}");
        assert!((east.bearing_deg - 90.0).abs() < 0.5, "{east:?}");
    }

    #[test]
    fn the_strongest_is_the_first_of_equally_strong_ones() {
        assert_eq!(
            strongest([Some(12), None, Some(40), Some(40), Some(7)]),
            Some((2, 40))
        );
        assert_eq!(strongest([None, None]), None);
        assert_eq!(strongest([]), None);
    }

    #[test]
    fn much_stronger_takes_a_wind_that_counts_and_a_margin_over_the_place() {
        // Gusts of 40 near by against 22 here: both conditions.
        assert_eq!(
            much_stronger((Some(10.0), Some(22.0)), (Some(18.0), Some(40.0))),
            Some(Stronger::Gusts)
        );
        // 39 near by does not reach the limit, whatever the place has.
        assert_eq!(
            much_stronger((Some(5.0), Some(8.0)), (Some(18.0), Some(39.0))),
            None
        );
        // 45 near by against 31 here is 14 more: not much stronger.
        assert_eq!(
            much_stronger((Some(10.0), Some(31.0)), (Some(18.0), Some(45.0))),
            None
        );
        // The mean wind alone: 30 near by against 12 here.
        assert_eq!(
            much_stronger((Some(12.0), Some(30.0)), (Some(30.0), Some(38.0))),
            Some(Stronger::Wind)
        );
        // Both: the gusts are named.
        assert_eq!(
            much_stronger((Some(5.0), Some(10.0)), (Some(35.0), Some(60.0))),
            Some(Stronger::Gusts)
        );
        // Nothing known of the place: nothing to compare with.
        assert_eq!(much_stronger((None, None), (Some(50.0), Some(80.0))), None);
        // Nothing near by.
        assert_eq!(much_stronger((Some(5.0), Some(10.0)), (None, None)), None);
    }
}
