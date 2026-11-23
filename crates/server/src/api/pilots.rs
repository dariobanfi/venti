//! Pilots as everyone may see them: a pilot's page, the search for pilots
//! by name, and what a pilot's public flights add up to.
//!
//! All three are made of flights their pilots made public and of nothing
//! else. A pilot who has made no flight public has no page, is found by no
//! search and has no totals, and nothing here tells such a pilot from a
//! handle nobody has.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use chrono::Datelike;
use serde::Deserialize;
use venti_api_types::v1::{
    ErrorCode, FlightSite, FlightTerms, FlownSite, FoundPilot, Handle, HighestFlight,
    LongestFlight, PILOT_SEARCH_MAX_CHARS, PILOT_SEARCH_MIN_CHARS, PILOT_SITE_STEP_DEG,
    PILOT_TOP_SITES, PilotResponse, PilotSeason, PilotStatisticsResponse, PilotTotals,
    PilotsResponse, RANKING_COUNTED,
};
use venti_domain::{GeoPoint, SiteId};
use venti_store::{Account, Flown, Listing, Store};

use super::auth::{May, ReadFlights};
use super::budget::Reader;
use super::cache::Keeper;
use super::error::ApiError;
use super::flights::PageParams;
use super::ranking::rounded;
use super::{Api, body, cache, wire};

/// The answer for a handle that leads to no page: nobody has it, or its
/// pilot shares nothing publicly. The two must not differ in anything.
pub(crate) fn no_such_pilot() -> ApiError {
    ApiError::missing(
        ErrorCode::PilotNotFound,
        "there is no pilot with this handle",
    )
}

impl<S: Store> Api<S> {
    /// The account behind a handle, whether or not it has a page.
    async fn account_of(&self, handle: &str) -> Result<Account, ApiError> {
        // Text that cannot be a handle is the handle of no pilot.
        let handle = Handle::new(handle).map_err(|_| no_such_pilot())?;
        self.store
            .account(handle.as_str())
            .await?
            .ok_or_else(no_such_pilot)
    }

    /// Whether an account has a page: whether it has made a flight public.
    async fn has_a_page(&self, account: &Account) -> Result<bool, ApiError> {
        let any = self
            .store
            .flights_of(&account.id, Listing::Public, None, 1)
            .await?;
        Ok(!any.is_empty())
    }

    /// The account whose page a handle leads to.
    ///
    /// A pilot has a page once they have made a flight public. One who has
    /// not gets the answer for a handle nobody has.
    pub(crate) async fn pilot_with_a_page(&self, handle: &str) -> Result<Account, ApiError> {
        let account = self.account_of(handle).await?;
        if !self.has_a_page(&account).await? {
            return Err(no_such_pilot());
        }
        Ok(account)
    }
}

/// `GET /v1/pilots/{handle}`: a pilot and their public flights, newest
/// first.
///
/// The answer is the same whoever asks, signed in or not: a pilot looking
/// at their own page sees what everyone sees.
///
/// A pilot who has made no flight public has no page: the answer is the
/// one for a handle nobody has. An account that shares nothing stays
/// invisible, and this endpoint is no way to find out which handles exist.
pub(crate) async fn show<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Path(handle): Path<String>,
    Query(params): Query<PageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let account = api.account_of(&handle).await?;

    let (flights, next) = api
        .flight_page(
            &account.id,
            Listing::Public,
            params.after.as_deref(),
            params.limit.as_deref(),
        )
        .await?;
    // An empty page is the end of the list only if there is a list.
    if flights.is_empty() && !api.has_a_page(&account).await? {
        return Err(no_such_pilot());
    }
    let answer = PilotResponse {
        pilot: wire::pilot_with_country(
            &account.handle,
            &account.display_name,
            account.country.as_deref(),
        )?,
        flights: api.shown_flights(&flights).await?,
        next,
        terms: Some(FlightTerms::current()),
    };
    // Checked before every reuse: a flight made private leaves this list
    // at once.
    Ok(cache::shared_json(&headers, &answer, Keeper::Anyone))
}

#[derive(Debug, Deserialize)]
pub(crate) struct SearchParams {
    q: Option<String>,
    limit: Option<String>,
}

/// `GET /v1/pilots?q=`: the pilots with a public flight whose handle or
/// display name holds the text, whatever the case of either.
///
/// Open to anyone and counted as a read of public flights is. Like a
/// pilot's page it is checked before every reuse: a pilot who withdraws
/// their last public flight is found no more at once.
pub(crate) async fn search<S: Store>(
    State(api): State<Arc<Api<S>>>,
    reader: Reader,
    Query(params): Query<SearchParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    api.count_public_read(&reader)?;
    let limit = body::page_size(params.limit.as_deref())?;
    let text = params.q.as_deref().map(str::trim).unwrap_or_default();
    if !(PILOT_SEARCH_MIN_CHARS..=PILOT_SEARCH_MAX_CHARS).contains(&text.chars().count()) {
        return Err(ApiError::bad_request(
            ErrorCode::InvalidRequest,
            format!(
                "q must be {PILOT_SEARCH_MIN_CHARS} to {PILOT_SEARCH_MAX_CHARS} characters of a \
                 pilot's handle or name, for example q=le"
            ),
        ));
    }
    let found = api.store.pilots_named(text, limit).await?;
    let answer = PilotsResponse {
        pilots: found
            .iter()
            .map(|found| {
                Ok(FoundPilot {
                    pilot: wire::pilot(&found.handle, &found.name)?,
                    flights: found.flights,
                })
            })
            .collect::<Result<_, ApiError>>()?,
    };
    Ok(cache::shared_json(&headers, &answer, Keeper::Anyone))
}

/// A sum of scoring distances as it is handed out: in tenths of a metre,
/// without what adding many numbers leaves behind the last digit.
fn tenths(distance_m: f64) -> f64 {
    (distance_m * 10.0).round() / 10.0
}

/// A site a flight took off from, with where the site stands.
type Placed = (FlightSite, GeoPoint);

/// What some of a pilot's public flights add up to. The flights come
/// newest first, each with the site it took off from; of two that are as
/// long or as high, the newer is named.
fn totals(flights: &[(&Flown, Option<&Placed>)]) -> Result<PilotTotals, ApiError> {
    let address = |flown: &Flown| flown.id.parse().map_err(|_| ApiError::internal());
    let mut by_month = [0_u32; 12];
    let mut airtime_s = 0_i64;
    let mut distance_m = 0.0;
    let mut scores: Vec<(f64, &str)> = Vec::new();
    let mut longest: Option<(&Flown, f64)> = None;
    let mut highest: Option<&Flown> = None;
    let mut sites: BTreeMap<&SiteId, (&Placed, u32)> = BTreeMap::new();
    for &(flown, site) in flights {
        by_month[flown.start.month0() as usize] += 1;
        airtime_s = airtime_s.saturating_add(flown.airtime_s);
        if let Some((distance, points)) = flown.score {
            distance_m += distance;
            scores.push((points, &flown.id));
            if longest.is_none_or(|(_, most)| distance > most) {
                longest = Some((flown, distance));
            }
        }
        if highest.is_none_or(|most| flown.max_altitude_m > most.max_altitude_m) {
            highest = Some(flown);
        }
        if let Some(placed) = site {
            sites.entry(&placed.0.id).or_insert((placed, 0)).1 += 1;
        }
    }
    // The best count, in the order a ranking sums them: by points, and by
    // address among flights with the same points.
    scores.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| b.1.cmp(a.1)));
    let points: f64 = scores
        .iter()
        .take(RANKING_COUNTED)
        .map(|&(points, _)| points)
        .sum();
    let mut sites: Vec<(&Placed, u32)> = sites.into_values().collect();
    sites.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.0.id.cmp(&b.0.0.id)));
    Ok(PilotTotals {
        flights: u32::try_from(flights.len()).unwrap_or(u32::MAX),
        scored: u32::try_from(scores.len()).unwrap_or(u32::MAX),
        distance_m: tenths(distance_m),
        points: rounded(points),
        airtime_s,
        longest: longest
            .map(|(flown, distance_m)| {
                Ok::<_, ApiError>(LongestFlight {
                    id: address(flown)?,
                    distance_m,
                })
            })
            .transpose()?,
        highest: highest
            .map(|flown| {
                Ok::<_, ApiError>(HighestFlight {
                    id: address(flown)?,
                    max_altitude_m: flown.max_altitude_m,
                })
            })
            .transpose()?,
        sites: sites
            .into_iter()
            .take(PILOT_TOP_SITES)
            .map(|((site, position), flights)| FlownSite {
                site: site.clone(),
                position: Some(*position),
                flights,
            })
            .collect(),
        by_month,
    })
}

/// A take-off on the grid its site is looked up at
/// ([`PILOT_SITE_STEP_DEG`]): a pilot's hundred starts from one meadow are
/// a handful of places to ask about.
fn on_the_grid(takeoff: GeoPoint) -> (i64, i64) {
    let step = |degrees: f64| (degrees / PILOT_SITE_STEP_DEG).round() as i64;
    (step(takeoff.lat()), step(takeoff.lon()))
}

impl<S: Store> Api<S> {
    /// The site each of some flights took off from, in their order, as a
    /// flight's `takeoff_site` names one, with the take-off taken on the
    /// grid of [`on_the_grid`].
    async fn sites_flown_from(&self, flights: &[Flown]) -> Result<Vec<Option<Placed>>, ApiError> {
        let mut cells: BTreeMap<(i64, i64), usize> = BTreeMap::new();
        let mut places: Vec<GeoPoint> = Vec::new();
        for takeoff in flights.iter().filter_map(|flown| flown.takeoff) {
            let cell = on_the_grid(takeoff);
            if cells.contains_key(&cell) {
                continue;
            }
            let at = GeoPoint::new(
                cell.0 as f64 * PILOT_SITE_STEP_DEG,
                cell.1 as f64 * PILOT_SITE_STEP_DEG,
            );
            if let Ok(at) = at {
                cells.insert(cell, places.len());
                places.push(at);
            }
        }
        let sites = self.placed_takeoff_sites(&places).await?;
        Ok(flights
            .iter()
            .map(|flown| {
                let at = cells.get(&on_the_grid(flown.takeoff?))?;
                sites.get(*at)?.clone()
            })
            .collect())
    }

    /// What a pilot's public flights add up to, as
    /// `GET /v1/pilots/{handle}/statistics` answers it. Counting the read
    /// is the caller's.
    pub(crate) async fn pilot_statistics(
        &self,
        handle: &str,
    ) -> Result<PilotStatisticsResponse, ApiError> {
        let account = self.account_of(handle).await?;
        // The store lists public flights only: one that is private or
        // shared by link is in no number here.
        let flown = self.store.flown_by(&account.id, Listing::Public).await?;
        if flown.is_empty() {
            return Err(no_such_pilot());
        }
        self.statistics_of(&account, &flown).await
    }

    /// What some flights of an account add up to, all together and year
    /// by year.
    pub(crate) async fn statistics_of(
        &self,
        account: &Account,
        flown: &[Flown],
    ) -> Result<PilotStatisticsResponse, ApiError> {
        let sites = self.sites_flown_from(flown).await?;
        let all: Vec<(&Flown, Option<&Placed>)> = flown
            .iter()
            .zip(&sites)
            .map(|(flown, site)| (flown, site.as_ref()))
            .collect();
        let mut years: BTreeMap<i32, Vec<(&Flown, Option<&Placed>)>> = BTreeMap::new();
        for &flight in &all {
            years.entry(flight.0.start.year()).or_default().push(flight);
        }
        Ok(PilotStatisticsResponse {
            pilot: wire::pilot_with_country(
                &account.handle,
                &account.display_name,
                account.country.as_deref(),
            )?,
            counted: u32::try_from(RANKING_COUNTED).unwrap_or(u32::MAX),
            overall: totals(&all)?,
            seasons: years
                .iter()
                .rev()
                .map(|(&year, flights)| {
                    Ok(PilotSeason {
                        year,
                        totals: totals(flights)?,
                    })
                })
                .collect::<Result<_, ApiError>>()?,
            terms: FlightTerms::current(),
        })
    }
}

/// `GET /v1/pilots/{handle}/statistics`: what a pilot's public flights
/// add up to, all together and year by year.
///
/// Open to anyone, counted as a read of public flights is, and the same
/// whoever asks. A pilot who has made no flight public has none: the
/// answer is the one for a handle nobody has.
pub(crate) async fn statistics<S: Store>(
    State(api): State<Arc<Api<S>>>,
    reader: Reader,
    Path(handle): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    api.count_public_read(&reader)?;
    let answer = api.pilot_statistics(&handle).await?;
    // Checked before every reuse: a flight made private is in no number
    // of the next answer.
    Ok(cache::shared_json(&headers, &answer, Keeper::Anyone))
}

/// `GET /v1/account/statistics`: what all the flights of the account add
/// up to, the private ones and those shared by link too, in the shape of
/// a pilot's public statistics. For the pilot themself: a session, or a
/// token with `flights:read`. An account without flights has totals of
/// nothing.
pub(crate) async fn own_statistics<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<ReadFlights>,
) -> Result<Response, ApiError> {
    let flown = api.store.flown_by(&signed.account.id, Listing::All).await?;
    let answer = api.statistics_of(&signed.account, &flown).await?;
    Ok(cache::private_json(StatusCode::OK, &answer))
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn flown(id: &str, month: u32, score: Option<(f64, f64)>, high: i32) -> Flown {
        Flown {
            id: format!("{id:0>26}"),
            start: Utc.with_ymd_and_hms(2026, month, 10, 9, 0, 0).unwrap(),
            airtime_s: 3600,
            score,
            max_altitude_m: high,
            takeoff: None,
        }
    }

    #[test]
    fn totals_add_up_and_the_best_three_count() {
        let site = |id: &str, name: &str| {
            (
                FlightSite {
                    id: id.parse().unwrap(),
                    name: name.to_owned(),
                },
                GeoPoint::new(47.6, 11.7).unwrap(),
            )
        };
        let (wallberg, brauneck) = (site("osm:1", "Wallberg"), site("osm:2", "Brauneck"));
        let flights = [
            flown("e", 8, Some((40_000.1, 40.0)), 2900),
            flown("d", 7, Some((10_000.1, 10.0)), 3100),
            flown("c", 7, None, 1500),
            flown("b", 5, Some((30_000.1, 30.0)), 3100),
            flown("a", 5, Some((20_000.1, 20.01)), 2000),
        ];
        let with_sites: Vec<(&Flown, Option<&Placed>)> = flights
            .iter()
            .zip([
                Some(&brauneck),
                Some(&wallberg),
                None,
                Some(&brauneck),
                Some(&wallberg),
            ])
            .collect();
        let totals = totals(&with_sites).unwrap();
        assert_eq!((totals.flights, totals.scored), (5, 4));
        assert_eq!(totals.distance_m, 100_000.4);
        assert_eq!(totals.points, 90.01);
        assert_eq!(totals.airtime_s, 5 * 3600);
        let longest = totals.longest.unwrap();
        assert!(longest.id.as_str().ends_with('e'));
        assert_eq!(longest.distance_m, 40_000.1);
        // Of two as high, the newer.
        let highest = totals.highest.unwrap();
        assert!(highest.id.as_str().ends_with('d'));
        assert_eq!(highest.max_altitude_m, 3100);
        // Of two sites flown from as often, the smaller identifier.
        let sites: Vec<(&str, u32)> = totals
            .sites
            .iter()
            .map(|flown| (flown.site.name.as_str(), flown.flights))
            .collect();
        assert_eq!(sites, [("Wallberg", 2), ("Brauneck", 2)]);
        assert_eq!(totals.by_month, [0, 0, 0, 0, 2, 0, 2, 1, 0, 0, 0, 0]);

        let nothing = super::totals(&[]).unwrap();
        assert_eq!((nothing.flights, nothing.points), (0, 0.0));
        assert!(nothing.longest.is_none() && nothing.highest.is_none());
    }

    #[test]
    fn take_offs_a_few_metres_apart_are_one_place_to_ask_about() {
        let at = |lat, lon| GeoPoint::new(lat, lon).unwrap();
        assert_eq!(on_the_grid(at(47.66421, 11.79672)), (476_642, 117_967));
        assert_eq!(
            on_the_grid(at(47.66421, 11.79672)),
            on_the_grid(at(47.66424, 11.79668))
        );
        assert_ne!(
            on_the_grid(at(47.66421, 11.79672)),
            on_the_grid(at(47.6644, 11.79672))
        );
    }
}
