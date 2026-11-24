//! The thermal forecast by region: the thermal map read region by region,
//! one level per region for a day and for each of its hours.
//!
//! The regions are the Alpine avalanche warning micro-regions of the EAWS
//! (`venti-sources-eawsregions`, CC0 1.0). Owner decision 27 in
//! `docs/plan.md` gives the five levels; item 21a there the rule, which is
//! this:
//!
//! - **What is judged.** A region's known take-offs, each at its own
//!   height in its cell of the map: the cell's hours with their convection
//!   as the map judged them at the model's ground, judged again for the
//!   take-off ([`at_height`]): a cloud base under the take-off or just above
//!   it, and how far the thermals reach above it. A region with fewer than
//!   [`FEWEST_TAKEOFFS`] take-offs that have hours takes its cells instead,
//!   those whose model ground lies below [`HIGH_GROUND_M`], each at that
//!   ground: a cell of glaciers says nothing of where pilots fly. A
//!   region's cells are those whose centre lies in it, or, for a region too
//!   small to hold one, the cell its label lies in.
//! - **The level.** The best level at least a third of them reach
//!   ([`combined`]). Not flyable only when fewer than a third are flyable,
//!   with the reason met most often among those that are not.
//! - **The levels.** Not flyable; glides only (flyable, no thermals); weak
//!   (weak and local thermals); good (cross-country); very good (big). Local
//!   counts as weak: the session's choice of 2026-11-23, the owner's to
//!   change.
//! - **The day** of a take-off or cell is judged as the map judges a cell's
//!   day (`venti_flight::day::cell_days`), from its hours judged at its
//!   height; the region's day is then the third of those days.
//!
//! No data, no level: a region without a judged take-off or cell is not in
//! the lists.

use chrono::NaiveDate;
use venti_api_types::v1::{RegionHour, RegionLevel, RegionReading, ThermalGrid};
use venti_domain::{GeoPoint, Timestamp};
use venti_flight::day::{self, Against, CLOUD_BASE_CLEAR_M, Kind, RatedHour, Reason, Verdict};

use crate::thermals::reason;

/// The fewest take-offs with hours that a region is judged by; with fewer
/// it is judged by its cells.
pub const FEWEST_TAKEOFFS: usize = 2;

/// The model ground from which a cell is left out of its region's
/// judgement, metres above mean sea level.
pub const HIGH_GROUND_M: f64 = 2500.0;

/// A take-off the forecast by region judges, at its own height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TakeOff {
    /// Where it is.
    pub at: GeoPoint,
    /// Its height above mean sea level, metres.
    pub height_m: f64,
}

/// A take-off with its region and its cell of the map.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    region: usize,
    cell: usize,
    height_m: f64,
}

/// The regions over the map's grid, found once.
#[derive(Debug, Clone)]
pub struct Regions {
    /// By region, in the order of `venti_sources_eawsregions::regions`,
    /// the cells of the map it is judged by when it has too few take-offs.
    cells: Vec<Vec<usize>>,
}

impl Regions {
    /// The regions over a grid whose cells' centres are `points`.
    pub fn over(grid: &ThermalGrid, points: &[GeoPoint]) -> Self {
        let regions = venti_sources_eawsregions::regions();
        let mut cells = vec![Vec::new(); regions.len()];
        for (cell, point) in points.iter().enumerate() {
            if let Some(region) = venti_sources_eawsregions::region_at(point.lon(), point.lat()) {
                cells[region].push(cell);
            }
        }
        for (region, of_region) in regions.iter().zip(&mut cells) {
            if of_region.is_empty() {
                let (lon, lat) = region.label;
                if let Some(cell) = GeoPoint::new(lat, lon)
                    .ok()
                    .and_then(|label| grid.cell_at(label))
                {
                    of_region.push(cell);
                }
            }
        }
        Self { cells }
    }

    /// How many regions have a cell of the grid.
    pub fn over_the_grid(&self) -> usize {
        self.cells.iter().filter(|cells| !cells.is_empty()).count()
    }

    /// The take-offs that lie in a region and in a cell of the grid.
    pub fn place(grid: &ThermalGrid, takeoffs: &[TakeOff]) -> Vec<Placed> {
        takeoffs
            .iter()
            .filter_map(|takeoff| {
                let region =
                    venti_sources_eawsregions::region_at(takeoff.at.lon(), takeoff.at.lat())?;
                let cell = grid.cell_at(takeoff.at)?;
                Some(Placed {
                    region,
                    cell,
                    height_m: takeoff.height_m,
                })
            })
            .collect()
    }
}

/// What one model has of a day at one cell of the map.
#[derive(Debug, Clone, Default)]
pub struct CellOfDay {
    /// The day's hours, as the map judged them at the model's ground,
    /// oldest first.
    pub hours: Vec<RatedHour>,
    /// The height of the model's ground.
    pub ground_m: Option<f64>,
    /// The cell's centre.
    pub point: Option<GeoPoint>,
}

/// A day of the forecast by region.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RegionDay {
    /// Each region's level for the day.
    pub day: Vec<RegionReading>,
    /// Each hour's levels, oldest first.
    pub hours: Vec<RegionHour>,
}

/// The level a day's or an hour's thermals make.
fn of_kind(kind: Kind) -> RegionLevel {
    match kind {
        Kind::NoThermals => RegionLevel::GlidesOnly,
        Kind::Weak | Kind::Local => RegionLevel::Weak,
        Kind::CrossCountry => RegionLevel::Good,
        Kind::Big => RegionLevel::VeryGood,
    }
}

/// An hour's level and what speaks against it. None where it is flyable
/// and has no estimate of its thermals.
fn of_hour(hour: &RatedHour) -> Option<(RegionLevel, Option<Reason>)> {
    if hour.verdict == Verdict::NotFlyable {
        return Some((
            RegionLevel::NotFlyable,
            hour.against.map(|against| against.reason()),
        ));
    }
    hour.kind.map(|kind| (of_kind(kind), None))
}

/// A cell's hour judged for a place at `height_m` above the sea in it: the
/// cell's verdict, worse where the cumulus base lies under the place or
/// just above it, and what the thermals count as above the place.
#[must_use]
pub fn at_height(hour: &RatedHour, height_m: f64) -> RatedHour {
    let (mut verdict, mut against) = (hour.verdict, hour.against);
    if let Some(base) = hour.air.and_then(|air| air.cloud_base_m) {
        let found = if base < height_m {
            Some((Verdict::NotFlyable, Against::CloudBaseBelow))
        } else if base < height_m + CLOUD_BASE_CLEAR_M {
            Some((Verdict::Marginal, Against::CloudBaseNear))
        } else {
            None
        };
        if let Some((worse, what)) = found.filter(|(worse, _)| *worse > verdict) {
            verdict = worse;
            against = Some(what);
        }
    }
    RatedHour {
        verdict,
        against,
        kind: hour
            .air
            .and_then(|air| day::kind_of_hour(&air, height_m, verdict)),
        ..*hour
    }
}

/// The level of a region from the levels of what it is judged by: the
/// best one at least a third of them reach. Not flyable when fewer than a
/// third are flyable, with the reason met most often among those that are
/// not, of equally frequent ones the first met. None without any.
#[must_use]
pub fn combined(levels: &[(RegionLevel, Option<Reason>)]) -> Option<(RegionLevel, Option<Reason>)> {
    if levels.is_empty() {
        return None;
    }
    let reach = |level: RegionLevel| levels.iter().filter(|(of, _)| *of >= level).count();
    for level in [
        RegionLevel::VeryGood,
        RegionLevel::Good,
        RegionLevel::Weak,
        RegionLevel::GlidesOnly,
    ] {
        if 3 * reach(level) >= levels.len() {
            return Some((level, None));
        }
    }
    let mut counts: Vec<(Reason, usize)> = Vec::new();
    for reason in levels.iter().filter_map(|(_, why)| *why) {
        match counts.iter_mut().find(|(of, _)| *of == reason) {
            Some((_, count)) => *count += 1,
            None => counts.push((reason, 1)),
        }
    }
    let most = counts.iter().map(|(_, count)| *count).max();
    let why = counts
        .into_iter()
        .find(|(_, count)| Some(*count) == most)
        .map(|(reason, _)| reason);
    Some((RegionLevel::NotFlyable, why))
}

/// One take-off or cell, judged: its hours at its height and its centre.
struct Judged {
    hours: Vec<RatedHour>,
    point: GeoPoint,
}

/// The forecast by region of one day, from one model's cells: `cells` is
/// as long as the grid has cells, `placed` the take-offs as
/// [`Regions::place`] found them.
#[must_use]
pub fn day_of(
    date: NaiveDate,
    cells: &[CellOfDay],
    regions: &Regions,
    placed: &[Placed],
) -> RegionDay {
    let all = venti_sources_eawsregions::regions();
    let times: std::collections::BTreeSet<Timestamp> = cells
        .iter()
        .flat_map(|cell| cell.hours.iter().map(|hour| hour.time))
        .collect();
    let mut day = Vec::new();
    let mut hours: Vec<RegionHour> = times
        .iter()
        .map(|time| RegionHour {
            time: *time,
            regions: Vec::new(),
        })
        .collect();
    for (region, of_region) in regions.cells.iter().enumerate() {
        let has_hours = |cell: usize| cells.get(cell).is_some_and(|of| !of.hours.is_empty());
        let takeoffs: Vec<&Placed> = placed
            .iter()
            .filter(|takeoff| takeoff.region == region && has_hours(takeoff.cell))
            .collect();
        let by_takeoffs = takeoffs.len() >= FEWEST_TAKEOFFS;
        let judged: Vec<Judged> = if by_takeoffs {
            takeoffs
                .iter()
                .filter_map(|takeoff| {
                    let cell = &cells[takeoff.cell];
                    Some(Judged {
                        hours: cell
                            .hours
                            .iter()
                            .map(|hour| at_height(hour, takeoff.height_m))
                            .collect(),
                        point: cell.point?,
                    })
                })
                .collect()
        } else {
            of_region
                .iter()
                .filter_map(|cell| cells.get(*cell))
                .filter(|cell| cell.ground_m.is_some_and(|ground| ground < HIGH_GROUND_M))
                .filter(|cell| !cell.hours.is_empty())
                .filter_map(|cell| {
                    Some(Judged {
                        hours: cell.hours.clone(),
                        point: cell.point?,
                    })
                })
                .collect()
        };
        if judged.is_empty() {
            continue;
        }
        let id = &all[region].id;
        let reading =
            |(level, why): (RegionLevel, Option<Reason>), takeoffs: Option<u32>| RegionReading {
                id: id.clone(),
                level,
                why: why.map(reason),
                takeoffs,
            };
        let days: Vec<(RegionLevel, Option<Reason>)> = judged
            .iter()
            .filter_map(|judged| {
                let of_day = day::cell_days(&judged.hours, judged.point)
                    .into_iter()
                    .find(|of| of.date == date)?;
                if of_day.verdict == Verdict::NotFlyable {
                    return Some((RegionLevel::NotFlyable, of_day.why));
                }
                of_day
                    .thermals
                    .map(|thermals| (of_kind(thermals.kind), None))
            })
            .collect();
        if let Some(level) = combined(&days) {
            let count = u32::try_from(judged.len()).unwrap_or(u32::MAX);
            day.push(reading(level, by_takeoffs.then_some(count)));
        }
        for hour in &mut hours {
            let levels: Vec<(RegionLevel, Option<Reason>)> = judged
                .iter()
                .filter_map(|judged| {
                    judged
                        .hours
                        .iter()
                        .find(|of| of.time == hour.time)
                        .and_then(of_hour)
                })
                .collect();
            if let Some(level) = combined(&levels) {
                hour.regions.push(reading(level, None));
            }
        }
    }
    RegionDay { day, hours }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use venti_api_types::v1::ThermalReason;
    use venti_flight::convection::Convection;

    use super::*;

    fn air(top_m: f64, climb_ms: f64, cloud_base_m: Option<f64>) -> Convection {
        Convection {
            top_m,
            cloud_base_m,
            depth_m: 1500.0,
            updraft_ms: climb_ms + 1.0,
            climb_ms,
            wind_ms: Some(3.0),
        }
    }

    /// An hour of 12 July 2026 at the cell, judged at its ground.
    fn hour(utc: u32, verdict: Verdict, air: Option<Convection>, ground_m: f64) -> RatedHour {
        let against = match verdict {
            Verdict::Flyable => None,
            Verdict::Marginal => Some(Against::Gusts { kmh: 32.0 }),
            Verdict::NotFlyable => Some(Against::Wind { kmh: 35.0 }),
        };
        RatedHour {
            time: Utc.with_ymd_and_hms(2026, 7, 12, utc, 0, 0).unwrap(),
            verdict,
            against,
            air,
            kind: air.and_then(|air| day::kind_of_hour(&air, ground_m, verdict)),
        }
    }

    #[test]
    fn a_region_takes_the_level_a_third_reach() {
        use RegionLevel::{GlidesOnly, Good, NotFlyable, VeryGood, Weak};
        let level = |levels: &[RegionLevel]| {
            let levels: Vec<_> = levels.iter().map(|level| (*level, None)).collect();
            combined(&levels).map(|(level, _)| level)
        };
        assert_eq!(level(&[]), None);
        assert_eq!(level(&[Good]), Some(Good));
        // One of three is very good: a third.
        assert_eq!(level(&[VeryGood, Weak, GlidesOnly]), Some(VeryGood));
        // One of four is not: the next a third reach.
        assert_eq!(level(&[VeryGood, Good, Weak, Weak]), Some(Good));
        assert_eq!(level(&[VeryGood, Weak, NotFlyable, NotFlyable]), Some(Weak));
        // Not flyable only when fewer than a third are flyable.
        assert_eq!(
            level(&[GlidesOnly, NotFlyable, NotFlyable]),
            Some(GlidesOnly)
        );
        assert_eq!(
            level(&[GlidesOnly, NotFlyable, NotFlyable, NotFlyable]),
            Some(NotFlyable)
        );
        // With the reason met most often, of two as often the first.
        let levels = [
            (NotFlyable, Some(Reason::Rain)),
            (NotFlyable, Some(Reason::Wind)),
            (NotFlyable, Some(Reason::Wind)),
            (GlidesOnly, None),
        ];
        assert_eq!(combined(&levels), Some((NotFlyable, Some(Reason::Wind))));
        let levels = [
            (NotFlyable, Some(Reason::Rain)),
            (NotFlyable, Some(Reason::Wind)),
        ];
        assert_eq!(combined(&levels), Some((NotFlyable, Some(Reason::Rain))));
    }

    #[test]
    fn a_take_off_is_judged_at_its_own_height() {
        // Thermals of 2 m/s to 3200 m, the cumulus base at 3200 m, over a
        // cell whose ground is at 900 m: a big hour there.
        let noon = hour(
            11,
            Verdict::Flyable,
            Some(air(3200.0, 2.0, Some(3200.0))),
            900.0,
        );
        assert_eq!(noon.kind, Some(Kind::Big));
        // At 1700 m they reach 1500 m above it: still big; at 1800 m,
        // 1400 m: cross-country.
        assert_eq!(at_height(&noon, 1700.0).kind, Some(Kind::Big));
        assert_eq!(at_height(&noon, 1800.0).kind, Some(Kind::CrossCountry));
        // At 2500 m, 700 m: local.
        assert_eq!(at_height(&noon, 2500.0).kind, Some(Kind::Local));
        // At 3100 m the base is just above: marginal, local at most.
        let near = at_height(&noon, 3100.0);
        assert_eq!(
            (near.verdict, near.against),
            (Verdict::Marginal, Some(Against::CloudBaseNear))
        );
        assert_eq!(near.kind, Some(Kind::Weak));
        // At 3300 m the take-off is in the cloud.
        let inside = at_height(&noon, 3300.0);
        assert_eq!(
            (inside.verdict, inside.against, inside.kind),
            (Verdict::NotFlyable, Some(Against::CloudBaseBelow), None)
        );
        // A cell's reason that is worse stays.
        let windy = hour(
            11,
            Verdict::NotFlyable,
            Some(air(3200.0, 2.0, Some(3200.0))),
            900.0,
        );
        assert_eq!(
            at_height(&windy, 3300.0).against,
            Some(Against::Wind { kmh: 35.0 })
        );
    }

    /// A grid of six cells around the Brauneck: those at 47.6° N and the
    /// one at 47.7° N and 11.5° E, where the Brauneck is, in the region of
    /// the western Bavarian Prealps (DE-BY-41); the other two north of the
    /// Alps, in no region.
    fn grid() -> (ThermalGrid, Vec<GeoPoint>) {
        let grid = ThermalGrid {
            south: 47.6,
            west: 11.3,
            step_deg: 0.1,
            rows: 2,
            columns: 3,
        };
        let points = (0..grid.cells())
            .map(|cell| grid.point(cell).unwrap())
            .collect();
        (grid, points)
    }

    /// Every cell of the grid with the same hours over the same ground.
    fn day_of_cells(hours: &[RatedHour], ground_m: f64, points: &[GeoPoint]) -> Vec<CellOfDay> {
        points
            .iter()
            .map(|point| CellOfDay {
                hours: hours.to_vec(),
                ground_m: Some(ground_m),
                point: Some(*point),
            })
            .collect()
    }

    /// Six hours in a row of the same air, 10 to 15 UTC.
    fn hours(verdict: Verdict, air: Option<Convection>, ground_m: f64) -> Vec<RatedHour> {
        (10..16)
            .map(|utc| hour(utc, verdict, air, ground_m))
            .collect()
    }

    #[test]
    fn a_region_is_judged_by_its_take_offs_at_their_heights() {
        let (grid, points) = grid();
        let regions = Regions::over(&grid, &points);
        let brauneck = venti_sources_eawsregions::region_at(11.5236, 47.6650).unwrap();
        let id = &venti_sources_eawsregions::regions()[brauneck].id;
        let date = chrono::NaiveDate::from_ymd_opt(2026, 7, 12).unwrap();
        // Strong thermals to 3000 m over a ground of 900 m in both cells.
        let strong = Some(air(3000.0, 2.2, None));
        let cells = day_of_cells(&hours(Verdict::Flyable, strong, 900.0), 900.0, &points);
        assert_eq!(
            &venti_sources_eawsregions::regions()[brauneck].id,
            "DE-BY-41"
        );

        // Without take-offs, the cells below 2500 m: a very good day.
        let without = day_of(date, &cells, &regions, &[]);
        let of_region = |day: &RegionDay| day.day.iter().find(|reading| &reading.id == id).cloned();
        let reading = of_region(&without).unwrap();
        assert_eq!(
            (reading.level, reading.takeoffs),
            (RegionLevel::VeryGood, None)
        );
        assert_eq!(without.hours.len(), 6);
        assert!(
            without.hours[0]
                .regions
                .iter()
                .any(|reading| &reading.id == id)
        );

        // Three take-offs: the Brauneck at 1500 m, and two at 2300 m and
        // 2400 m, from which the thermals reach 700 and 600 m: local, so
        // weak. Two of three are weak, one very good: very good is a third.
        let at = |lat: f64, lon: f64, height_m: f64| TakeOff {
            at: GeoPoint::new(lat, lon).unwrap(),
            height_m,
        };
        let brauneck_at = at(47.6650, 11.5236, 1500.0);
        let high = [
            brauneck_at,
            at(47.6650, 11.5236, 2300.0),
            at(47.6650, 11.5236, 2400.0),
        ];
        let placed = Regions::place(&grid, &high);
        assert_eq!(placed.len(), 3);
        let reading = of_region(&day_of(date, &cells, &regions, &placed)).unwrap();
        assert_eq!(
            (reading.level, reading.takeoffs),
            (RegionLevel::VeryGood, Some(3))
        );
        // A fourth high one: one of four is no third; weak.
        let four = [high[0], high[1], high[2], at(47.6650, 11.5236, 2350.0)];
        let placed = Regions::place(&grid, &four);
        let day = day_of(date, &cells, &regions, &placed);
        let reading = of_region(&day).unwrap();
        assert_eq!(
            (reading.level, reading.takeoffs),
            (RegionLevel::Weak, Some(4))
        );
        assert_eq!(
            day.hours[0]
                .regions
                .iter()
                .find(|r| &r.id == id)
                .unwrap()
                .level,
            RegionLevel::Weak
        );

        // One take-off is too few: the cells again.
        let placed = Regions::place(&grid, &[brauneck_at]);
        let reading = of_region(&day_of(date, &cells, &regions, &placed)).unwrap();
        assert_eq!(
            (reading.level, reading.takeoffs),
            (RegionLevel::VeryGood, None)
        );
    }

    #[test]
    fn a_region_of_high_cells_or_wind_has_its_level() {
        let (grid, points) = grid();
        let regions = Regions::over(&grid, &points);
        let brauneck = venti_sources_eawsregions::region_at(11.5236, 47.6650).unwrap();
        let id = &venti_sources_eawsregions::regions()[brauneck].id;
        let date = chrono::NaiveDate::from_ymd_opt(2026, 7, 12).unwrap();
        let of_region = |day: &RegionDay| day.day.iter().find(|reading| &reading.id == id).cloned();
        let strong = Some(air(3000.0, 2.2, None));

        // Only cells above 2500 m: no level, and the region is not listed.
        let high = day_of_cells(&hours(Verdict::Flyable, strong, 2600.0), 2600.0, &points);
        let day = day_of(date, &high, &regions, &[]);
        assert_eq!(of_region(&day), None);
        assert!(
            day.hours
                .iter()
                .all(|hour| hour.regions.iter().all(|r| &r.id != id))
        );

        // Too much wind in both cells: not flyable, for the wind.
        let windy = day_of_cells(&hours(Verdict::NotFlyable, strong, 900.0), 900.0, &points);
        let reading = of_region(&day_of(date, &windy, &regions, &[])).unwrap();
        assert_eq!(
            (reading.level, reading.why),
            (RegionLevel::NotFlyable, Some(ThermalReason::Wind))
        );

        // Flyable, without thermals: glides only.
        let calm = day_of_cells(
            &hours(Verdict::Flyable, Some(air(900.0, 0.0, None)), 900.0),
            900.0,
            &points,
        );
        let reading = of_region(&day_of(date, &calm, &regions, &[])).unwrap();
        assert_eq!(reading.level, RegionLevel::GlidesOnly);
    }
}
