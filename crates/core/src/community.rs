//! Other pilots' flights: the flights the pilots of a Venti server made
//! public, in one list, as XContest lists what was flown.
//!
//! As everywhere, the core does not talk to the server: one function says
//! what to ask for, another reads the answer into rows for a screen, and
//! the shell does the request in between. A flight of the list is opened
//! by fetching its file and handing the text to
//! [`flight_from_file`](crate::flight_from_file), which shows it like one
//! of the pilot's own and keeps nothing.
//!
//! A flight that is reached by its address alone, as from a link, is read
//! by [`public_flight_path`] and [`public_flight`].
//!
//! What another pilot wrote, a name or a glider, is cut to one bounded
//! line before it reaches a screen.

use chrono::{DateTime, Datelike, TimeZone, Utc};
use venti_api_types::v1::{
    self as api, Flight, FlightFilters, FlightResponse, FlightSort, FlightsResponse, PilotRequest,
    PilotResponse, PublicFlightsRequest, RankingRequest, RankingResponse, RankingYear,
};
use venti_domain::{BoundingBox, Timestamp};

use crate::live::one_line;
use crate::server::answer;
use crate::takeoffs::{MONTHS, count};
use crate::{Bounds, Coordinate, CoreError, EntryScore, RouteKind};

/// The order of a list of public flights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum PublicFlightsOrder {
    /// The newest first, by when the flight began.
    Newest,
    /// The most points first. Flights too short to score are left out.
    Best,
}

/// One public flight as a row of a list shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PublicFlight {
    /// The flight's address on the server.
    pub id: String,
    /// Its pilot's handle.
    pub handle: String,
    /// The name its pilot is shown by, on one line.
    pub pilot: String,
    /// When it began, seconds since 1970-01-01 UTC.
    pub start_unix_s: i64,
    /// Seconds in the air.
    pub airtime_s: i64,
    /// Where it took off, if the recording shows a take-off.
    pub takeoff: Option<Coordinate>,
    /// The name of the site it took off from, on one line: the server's
    /// take-off nearest to where it took off, within 500 m. Absent when no
    /// site stands there; show `takeoff` then.
    #[uniffi(default = None)]
    pub takeoff_name: Option<String>,
    /// The identifier of that site, to open its page.
    #[uniffi(default = None)]
    pub takeoff_site_id: Option<String>,
    /// Distance along the track, metres.
    pub track_length_m: f64,
    /// The highest altitude, metres.
    pub max_altitude_m: f64,
    /// The best climb averaged over half a minute, metres per second.
    pub max_climb_ms: f64,
    /// The glider, as its pilot's file names it, on one line.
    pub glider: Option<String>,
    /// How it scores as a cross-country flight, by the server's reckoning,
    /// which is the logbook's.
    pub score: Option<EntryScore>,
    /// The shape of the track in a few dozen points.
    pub outline: Vec<Coordinate>,
    /// The path of the flight's page on the server: "/f/…".
    pub page_path: String,
    /// The path to fetch its file from, to open the flight.
    pub file_path: String,
    /// The country it took off in, an ISO 3166-1 alpha-2 code, where the
    /// server knows it. A shell draws its flag from the code.
    #[uniffi(default = None)]
    pub takeoff_country: Option<String>,
    /// Its pilot's country, an ISO 3166-1 alpha-2 code, if the pilot set
    /// one. A shell draws its flag from the code.
    #[uniffi(default = None)]
    pub pilot_country: Option<String>,
    /// The address of its pilot's picture on the server, relative to it
    /// (`/v1/pictures/{id}`), if the pilot has one; the shell shows the
    /// pilot's initials without one.
    #[uniffi(default = None)]
    pub pilot_picture: Option<String>,
    /// The class of the glider it was flown on, where it is known; name it
    /// with [`crate::glider_class_name`].
    #[uniffi(default = None)]
    pub glider_class: Option<crate::GliderClassKind>,
    /// The time zone it took off in, an IANA name (`Europe/Vienna`), to
    /// show its times by the clock there. `None` where the recording shows
    /// no take-off, or one at sea away from a coast.
    #[uniffi(default = None)]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    #[cfg_attr(feature = "wasm", serde(skip_serializing_if = "Option::is_none"))]
    pub time_zone: Option<String>,
}

/// One page of a list of public flights.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PublicFlights {
    /// The flights, in the order asked for.
    pub flights: Vec<PublicFlight>,
    /// What to pass as `after` for the next page. Absent on the last.
    pub next: Option<String>,
}

/// The path and query to `GET` from the server for one page of the public
/// flights.
///
/// `area` narrows the list to flights that took off inside a part of the
/// map. `from_unix_s` and `to_unix_s` narrow it to flights that began at
/// or after the one and before the other: a day is its first second and
/// the first second of the next. `after` is the `next` of the page before,
/// asked for with everything else unchanged.
#[uniffi::export]
pub fn public_flights_path(
    order: PublicFlightsOrder,
    area: Option<Bounds>,
    from_unix_s: Option<i64>,
    to_unix_s: Option<i64>,
    after: Option<String>,
) -> Result<String, CoreError> {
    let Which { bbox, from, to } = which(
        area,
        from_unix_s,
        to_unix_s,
        "The app could not ask the server for these flights.",
    )?;
    Ok(PublicFlightsRequest {
        bbox,
        from,
        to,
        site: None,
        filters: FlightFilters::default(),
        sort: match order {
            // The server's own order: nothing to say.
            PublicFlightsOrder::Newest => None,
            PublicFlightsOrder::Best => Some(FlightSort::Score),
        },
        after: after.filter(|after| !after.is_empty()),
        limit: None,
    }
    .path_and_query())
}

/// Which public flights a question means: a part of the map and a span of
/// time, as the server's requests name them.
pub(crate) struct Which {
    pub(crate) bbox: Option<BoundingBox>,
    pub(crate) from: Option<Timestamp>,
    pub(crate) to: Option<Timestamp>,
}

/// Reads what a screen asks for into the server's terms. What cannot be
/// asked for is refused in `words`.
pub(crate) fn which(
    area: Option<Bounds>,
    from_unix_s: Option<i64>,
    to_unix_s: Option<i64>,
    words: &str,
) -> Result<Which, CoreError> {
    let unreadable = || CoreError::Server {
        message: words.to_owned(),
    };
    let bbox = area
        .map(|area| {
            BoundingBox::new(
                area.south.clamp(-90.0, 90.0),
                area.west.clamp(-180.0, 180.0),
                area.north.clamp(-90.0, 90.0),
                area.east.clamp(-180.0, 180.0),
            )
            .map_err(|_| unreadable())
        })
        .transpose()?;
    let time = |seconds: Option<i64>| {
        seconds
            .map(|seconds| DateTime::from_timestamp(seconds, 0).ok_or_else(unreadable))
            .transpose()
    };
    let (from, to) = (time(from_unix_s)?, time(to_unix_s)?);
    if from.zip(to).is_some_and(|(from, to)| to <= from) {
        return Err(unreadable());
    }
    Ok(Which { bbox, from, to })
}

/// How far from a place a flight may have taken off to count as flown from
/// there: a site's page lists those flights, and ranks their pilots.
/// A take-off is a slope of some hundred metres, and the recorder marks
/// the moment the glider flew, which is a little way out from it.
pub const FLOWN_FROM_HERE_M: f64 = 1500.0;

/// The square around a place, `radius_m` to each side and at most fifty
/// kilometres: the part of the map a list of the flights from a place is
/// asked for.
pub fn area_around(lat: f64, lon: f64, radius_m: f64) -> Result<Bounds, CoreError> {
    const M_PER_DEGREE: f64 = 111_320.0;
    let unreadable = || CoreError::Server {
        message: "The app could not ask the server for the flights from this place.".to_owned(),
    };
    if !(lat.is_finite() && lon.is_finite() && radius_m.is_finite()) || lat.abs() > 89.0 {
        return Err(unreadable());
    }
    let reach = radius_m.clamp(100.0, 50_000.0);
    let north_south = reach / M_PER_DEGREE;
    let east_west = reach / (M_PER_DEGREE * lat.to_radians().cos());
    Ok(Bounds {
        south: lat - north_south,
        west: lon - east_west,
        north: lat + north_south,
        east: lon + east_west,
    })
}

/// The path and query to `GET` for the public flights that began at a
/// site: those that took off within 500 m of it, as the site's page counts
/// them, in `order`. A site that is no take-off has none. `after` is the
/// `next` of the page before. The answer is read by [`public_flights`].
///
/// # Errors
///
/// An identifier that is not a site's.
#[uniffi::export]
pub fn public_flights_at_site_path(
    site_id: String,
    order: PublicFlightsOrder,
    after: Option<String>,
) -> Result<String, CoreError> {
    let site = site_id.parse().map_err(|_| CoreError::Server {
        message: "This is not a site the app knows.".to_owned(),
    })?;
    Ok(PublicFlightsRequest {
        site: Some(site),
        sort: match order {
            PublicFlightsOrder::Newest => None,
            PublicFlightsOrder::Best => Some(FlightSort::Score),
        },
        after: after.filter(|after| !after.is_empty()),
        ..PublicFlightsRequest::default()
    }
    .path_and_query())
}

/// Reads the server's answer to [`public_flights_path`] and to
/// [`public_flights_at_site_path`].
#[uniffi::export]
pub fn public_flights(status: u16, body: Vec<u8>) -> Result<PublicFlights, CoreError> {
    let response: FlightsResponse = answer(status, &body)?;
    Ok(PublicFlights {
        flights: response.flights.iter().map(row).collect(),
        next: response.next,
    })
}

/// One pilot and a page of the flights they share with everyone.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PilotPage {
    /// The pilot's handle.
    pub handle: String,
    /// The name the pilot is shown by, on one line.
    pub name: String,
    /// The pilot's public flights, the newest first.
    pub flights: Vec<PublicFlight>,
    /// What to pass as `after` for the next page. Absent on the last.
    pub next: Option<String>,
}

/// The path and query to `GET` from the server for a pilot and their
/// public flights. `after` is the `next` of the page before.
#[uniffi::export]
pub fn pilot_page_path(handle: String, after: Option<String>) -> Result<String, CoreError> {
    let handle = handle.parse().map_err(|_| CoreError::Server {
        message: "This is not the handle of a pilot.".to_owned(),
    })?;
    Ok(PilotRequest {
        handle,
        after: after.filter(|after| !after.is_empty()),
        limit: None,
    }
    .path_and_query())
}

/// Reads the server's answer to [`pilot_page_path`]. A pilot the server
/// does not know is said in words.
#[uniffi::export]
pub fn pilot_page(status: u16, body: Vec<u8>) -> Result<PilotPage, CoreError> {
    if status == 404 {
        return Err(CoreError::Server {
            message: "The server knows no pilot by this handle.".to_owned(),
        });
    }
    let response: PilotResponse = answer(status, &body)?;
    Ok(PilotPage {
        handle: response.pilot.handle.to_string(),
        name: one_line(&response.pilot.display_name, 40),
        flights: response.flights.iter().map(row).collect(),
        next: response.next,
    })
}

pub(crate) fn row(flight: &Flight) -> PublicFlight {
    let takeoff = flight.takeoff.map(|takeoff| Coordinate {
        lat: takeoff.position.lat(),
        lon: takeoff.position.lon(),
    });
    PublicFlight {
        id: flight.id.to_string(),
        handle: flight.pilot.handle.to_string(),
        pilot: one_line(&flight.pilot.display_name, 40),
        start_unix_s: flight.start.timestamp(),
        airtime_s: flight.airtime_s.max(0),
        takeoff,
        time_zone: crate::time_zones::flight_zone(None, takeoff),
        takeoff_name: flight
            .takeoff_site
            .as_ref()
            .map(|site| one_line(&site.name, 60))
            .filter(|name| !name.is_empty()),
        takeoff_site_id: flight.takeoff_site.as_ref().map(|site| site.id.to_string()),
        track_length_m: flight.track_length_m,
        max_altitude_m: f64::from(flight.max_altitude_m),
        max_climb_ms: flight.max_climb_ms,
        glider: flight
            .glider
            .as_deref()
            .map(|glider| one_line(glider, 40))
            .filter(|glider| !glider.is_empty()),
        score: flight.score.and_then(|score| {
            Some(EntryScore {
                kind: match score.kind {
                    api::RouteKind::FreeFlight => RouteKind::FreeFlight,
                    api::RouteKind::FreeTriangle => RouteKind::FreeTriangle,
                    api::RouteKind::FaiTriangle => RouteKind::FaiTriangle,
                    // A kind of route this version has no name for: the
                    // flight is listed without a score.
                    api::RouteKind::Unknown => return None,
                },
                closed: score.closed,
                distance_m: score.distance_m,
                points: score.points,
            })
        }),
        outline: flight
            .outline
            .iter()
            .map(|&[lat, lon]| Coordinate { lat, lon })
            .collect(),
        page_path: api::share_path(&flight.id),
        file_path: api::flight_file_path(&flight.id),
        takeoff_country: flight
            .takeoff_country
            .as_deref()
            .and_then(api::country_code),
        pilot_country: flight.pilot.country.as_deref().and_then(api::country_code),
        pilot_picture: flight.pilot.picture.clone(),
        glider_class: flight
            .glider_class
            .and_then(crate::own_flights::glider_class_kind),
    }
}

/// A span of time a ranking is asked for, as a screen offers it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RankingSeason {
    /// What it is called: "2026", "All years".
    pub name: String,
    /// Its first second, seconds since 1970-01-01 UTC. Absent for all
    /// years.
    pub from_unix_s: Option<i64>,
    /// The first second after it. Absent for all years.
    pub to_unix_s: Option<i64>,
}

/// How many years a screen offers a ranking for.
const SEASONS_OFFERED: i32 = 4;

/// The seasons a screen offers at a moment: the year of that moment and
/// the three before it, the newest first, and all years together.
///
/// A season is a calendar year by UTC. The owner has not decided what a
/// season is; a year is what needs no explaining.
///
/// A `now_unix_s` of zero or less, the clock not known, offers all years
/// together alone: which year it is is not known.
#[uniffi::export]
pub fn ranking_seasons(now_unix_s: i64) -> Vec<RankingSeason> {
    let all_years = RankingSeason {
        name: ALL_YEARS.to_owned(),
        from_unix_s: None,
        to_unix_s: None,
    };
    if now_unix_s <= 0 {
        return vec![all_years];
    }
    let this_year = DateTime::from_timestamp(now_unix_s, 0).map_or(1970, |now| now.year());
    let first_second = |year: i32| first_second_of(year).map(|start| start.timestamp());
    let mut seasons: Vec<RankingSeason> = (0..SEASONS_OFFERED)
        .map(|back| this_year - back)
        .filter_map(|year| {
            Some(RankingSeason {
                name: year.to_string(),
                from_unix_s: Some(first_second(year)?),
                to_unix_s: Some(first_second(year + 1)?),
            })
        })
        .collect();
    seasons.push(all_years);
    seasons
}

/// A pilot's place in a ranking, as a row shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RankedPilot {
    /// The place: 1 for the most points. Pilots with the same points
    /// share a place, and the place after them is left out: 1, 2, 2, 4.
    pub place: u32,
    /// The pilot's handle.
    pub handle: String,
    /// The name the pilot is shown by, on one line.
    pub name: String,
    /// The sum of the points of the flights that count.
    pub points: f64,
    /// What counts, in words: "1 flight", "3 flights", "Best 3 of 14
    /// flights".
    pub counted: String,
    /// The flights that count, the best first.
    pub flights: Vec<PublicFlight>,
    /// The path of the pilot's picture, to fetch with the server's address
    /// in front. `None` without one: show the initials.
    #[uniffi(default = None)]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub picture_path: Option<String>,
    /// The pilot's country, an ISO 3166-1 alpha-2 code, where they set
    /// one: for a flag beside the name.
    #[uniffi(default = None)]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub country: Option<String>,
}

/// The pilots of a server's public flights, ranked.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct Ranking {
    /// The pilots, the one with the most points first.
    pub pilots: Vec<RankedPilot>,
    /// The rule in a sentence, for under the list.
    pub rule: String,
    /// The years that have a ranking, the newest first, each with its
    /// months that have one: what a page offers beside "All years". They
    /// are the same whichever of them the ranking is of. Empty from a
    /// server that does not say; [`ranking_seasons`] then names years.
    #[uniffi(default = [])]
    pub periods: Vec<RankingYearChoice>,
    /// Which period the ranking is of, as the server says: what a page
    /// marks as chosen and puts in its title.
    pub period: RankingPeriod,
}

/// The period a ranking is of.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RankingPeriod {
    /// The year, for a ranking of a year or of one of its months. 0 for
    /// all years, and for a span that is neither a year nor a month.
    #[uniffi(default = 0)]
    pub year: i32,
    /// The month as the server writes it, "2026-08", as a
    /// [`RankingMonthChoice`] names it. Absent but for a month.
    #[uniffi(default = None)]
    pub month: Option<String>,
    /// The period in words, for a title: "August 2026", "2026", "All
    /// years", "1 Mar 2026 to 31 May 2026".
    #[uniffi(default = "")]
    pub title: String,
    /// Its first second, seconds since 1970-01-01 UTC. Absent when the
    /// span has no beginning.
    #[uniffi(default = None)]
    pub from_unix_s: Option<i64>,
    /// The first second after it. Absent when the span has no end.
    #[uniffi(default = None)]
    pub to_unix_s: Option<i64>,
}

/// A year a ranking can be asked for, because flights that score were
/// flown in it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RankingYearChoice {
    /// What it is called: "2026".
    #[uniffi(default = "")]
    pub name: String,
    /// Its first second, seconds since 1970-01-01 UTC: for
    /// [`ranking_path`].
    #[uniffi(default = 0)]
    pub from_unix_s: i64,
    /// The first second after it.
    #[uniffi(default = 0)]
    pub to_unix_s: i64,
    /// How many public flights that score began in it.
    #[uniffi(default = 0)]
    pub flights: u32,
    /// The year in words, for a reader of the screen: "2026: 10 flights".
    #[uniffi(default = "")]
    pub words: String,
    /// The whole year as the first choice beside its months: "All of
    /// 2026".
    #[uniffi(default = "")]
    pub whole_name: String,
    /// Its months that have a ranking, the newest first.
    #[uniffi(default = [])]
    pub months: Vec<RankingMonthChoice>,
}

/// A month a ranking can be asked for.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RankingMonthChoice {
    /// What it is called beside its year: "August".
    #[uniffi(default = "")]
    pub name: String,
    /// What it is called alone: "August 2026".
    #[uniffi(default = "")]
    pub title: String,
    /// The month as the server writes it, "2026-08": for
    /// [`ranking_month_path`].
    #[uniffi(default = "")]
    pub month: String,
    /// How many public flights that score began in it.
    #[uniffi(default = 0)]
    pub flights: u32,
    /// The month in words: "August 2026: 4 flights".
    #[uniffi(default = "")]
    pub words: String,
}

/// What all years together are called, beside the years.
pub(crate) const ALL_YEARS: &str = "All years";

/// The first second of a year by UTC.
pub(crate) fn first_second_of(year: i32) -> Option<DateTime<Utc>> {
    Utc.with_ymd_and_hms(year, 1, 1, 0, 0, 0).single()
}

/// The period a ranking is of, from what the server says it counted.
fn period_of(response: &RankingResponse) -> RankingPeriod {
    let span = RankingPeriod {
        year: 0,
        month: None,
        title: String::new(),
        from_unix_s: response.from.map(|from| from.timestamp()),
        to_unix_s: response.to.map(|to| to.timestamp()),
    };
    if let Some(month) = response.month {
        let name = month
            .month()
            .checked_sub(1)
            .and_then(|index| MONTHS.get(index as usize))
            .map_or("", |(_, long)| long);
        return RankingPeriod {
            year: month.year(),
            month: Some(month.to_string()),
            title: format!("{name} {}", month.year()),
            ..span
        };
    }
    let day = |moment: DateTime<Utc>| moment.format("%-d %b %Y").to_string();
    // The last day of a span is the day of its last second.
    let last_day = |end: DateTime<Utc>| day(end - chrono::TimeDelta::seconds(1));
    match (response.from, response.to) {
        (None, None) => RankingPeriod {
            title: ALL_YEARS.to_owned(),
            ..span
        },
        (Some(from), Some(to))
            if first_second_of(from.year()) == Some(from)
                && first_second_of(from.year() + 1) == Some(to) =>
        {
            RankingPeriod {
                year: from.year(),
                title: from.year().to_string(),
                ..span
            }
        }
        (Some(from), Some(to)) => RankingPeriod {
            title: format!("{} to {}", day(from), last_day(to)),
            ..span
        },
        (Some(from), None) => RankingPeriod {
            title: format!("Since {}", day(from)),
            ..span
        },
        (None, Some(to)) => RankingPeriod {
            title: format!("Until {}", last_day(to)),
            ..span
        },
    }
}

fn period_choices(periods: &[RankingYear]) -> Vec<RankingYearChoice> {
    let first_second = |year: i32| first_second_of(year).map(|start| start.timestamp());
    periods
        .iter()
        .filter_map(|year| {
            Some(RankingYearChoice {
                name: year.year.to_string(),
                from_unix_s: first_second(year.year)?,
                to_unix_s: first_second(year.year + 1)?,
                flights: year.flights,
                words: format!(
                    "{}: {}",
                    year.year,
                    count(year.flights, "flight", "flights")
                ),
                whole_name: format!("All of {}", year.year),
                months: year
                    .months
                    .iter()
                    .filter_map(|month| {
                        let (_, long) = MONTHS.get(month.month.month().checked_sub(1)? as usize)?;
                        let title = format!("{long} {}", month.month.year());
                        Some(RankingMonthChoice {
                            name: (*long).to_owned(),
                            words: format!(
                                "{title}: {}",
                                count(month.flights, "flight", "flights")
                            ),
                            title,
                            month: month.month.to_string(),
                            flights: month.flights,
                        })
                    })
                    .collect(),
            })
        })
        .collect()
}

/// The path and query to `GET` from the server for the ranking of one
/// month: `month` as a [`RankingMonthChoice`] names it, "2026-08".
/// `area` counts only flights that took off inside a part of the map.
#[uniffi::export]
pub fn ranking_month_path(area: Option<Bounds>, month: String) -> Result<String, CoreError> {
    let asked = "The app could not ask the server for this ranking.";
    let Which { bbox, .. } = which(area, None, None, asked)?;
    let month = month.parse().map_err(|_| CoreError::Server {
        message: asked.to_owned(),
    })?;
    Ok(RankingRequest {
        bbox,
        from: None,
        to: None,
        month: Some(month),
        filters: FlightFilters::default(),
        pilot_country: None,
        limit: None,
    }
    .path_and_query())
}

/// The path and query to `GET` from the server for the ranking of its
/// pilots: by the sum of the points of each pilot's best public flights.
///
/// `area` counts only flights that took off inside a part of the map;
/// `from_unix_s` and `to_unix_s` only flights that began at or after the
/// one and before the other, as [`ranking_seasons`] names them.
#[uniffi::export]
pub fn ranking_path(
    area: Option<Bounds>,
    from_unix_s: Option<i64>,
    to_unix_s: Option<i64>,
) -> Result<String, CoreError> {
    let Which { bbox, from, to } = which(
        area,
        from_unix_s,
        to_unix_s,
        "The app could not ask the server for this ranking.",
    )?;
    Ok(RankingRequest {
        month: None,
        bbox,
        from,
        to,
        filters: FlightFilters::default(),
        pilot_country: None,
        limit: None,
    }
    .path_and_query())
}

/// Reads the server's answer to [`ranking_path`].
#[uniffi::export]
pub fn ranking(status: u16, body: Vec<u8>) -> Result<Ranking, CoreError> {
    let response: RankingResponse = answer(status, &body)?;
    let mut pilots: Vec<RankedPilot> = Vec::with_capacity(response.pilots.len());
    for (index, ranked) in response.pilots.iter().enumerate() {
        // The server sends the pilots in order. One with the points of
        // the pilot before shares that pilot's place.
        let place = match pilots.last() {
            Some(before) if before.points == ranked.points => before.place,
            _ => u32::try_from(index + 1).unwrap_or(u32::MAX),
        };
        let counting = ranked.flights.len();
        pilots.push(RankedPilot {
            place,
            handle: ranked.pilot.handle.to_string(),
            name: one_line(&ranked.pilot.display_name, 40),
            points: ranked.points,
            counted: match (counting, ranked.scored as usize) {
                (1, scored) if scored <= 1 => "1 flight".to_owned(),
                (counting, scored) if scored <= counting => format!("{counting} flights"),
                (1, scored) => format!("Best of {scored} flights"),
                (counting, scored) => format!("Best {counting} of {scored} flights"),
            },
            flights: ranked.flights.iter().map(row).collect(),
            picture_path: ranked.pilot.picture.clone(),
            country: ranked.pilot.country.clone(),
        });
    }
    let rule = match response.counted {
        1 => "A pilot's points are those of their best flight shared with everyone.".to_owned(),
        counted => format!(
            "A pilot's points are those of their best {counted} flights shared with everyone, \
             added up."
        ),
    };
    Ok(Ranking {
        period: period_of(&response),
        periods: period_choices(&response.periods),
        pilots,
        rule: format!(
            "{rule} Worked out by Venti from the flights' files, by the same rules for \
             every flight. It is not a league's result."
        ),
    })
}

/// The path to `GET` from the server for one flight that is reached by its
/// address alone, as from a link: what a row of the list says about it,
/// which names the pilot and where its file is.
#[uniffi::export]
pub fn public_flight_path(id: String) -> Result<String, CoreError> {
    let id = id.parse().map_err(|_| CoreError::Server {
        message: "This is not the address of a flight.".to_owned(),
    })?;
    Ok(api::flight_path(&id))
}

/// Reads the server's answer to [`public_flight_path`], and also what the
/// server says when it does not hand out the flight's file. A flight that
/// is not there for the asker is said to be no longer public: the server
/// does not tell a private flight from none.
#[uniffi::export]
pub fn public_flight(status: u16, body: Vec<u8>) -> Result<PublicFlight, CoreError> {
    if status == 404 {
        return Err(CoreError::Server {
            message: "This flight is no longer public: its pilot took it back, or deleted it."
                .to_owned(),
        });
    }
    let response: FlightResponse = answer(status, &body)?;
    Ok(row(&response.flight))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn golden(name: &str) -> Vec<u8> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../server/tests/golden/");
        std::fs::read(format!("{path}{name}")).unwrap()
    }

    #[test]
    fn against_an_unknown_clock_all_years_are_offered_alone() {
        let seasons = ranking_seasons(0);
        assert_eq!(seasons.len(), 1);
        assert_eq!(seasons[0].name, "All years");
        assert_eq!((seasons[0].from_unix_s, seasons[0].to_unix_s), (None, None));
    }

    #[test]
    fn a_ranking_is_asked_for_by_season_and_read_into_places() {
        // 18 November 2026, 19:05 UTC: the development mode's clock.
        let seasons = ranking_seasons(1_795_028_700);
        let names: Vec<&str> = seasons.iter().map(|season| season.name.as_str()).collect();
        assert_eq!(names, ["2026", "2025", "2024", "2023", "All years"]);
        let this_year = &seasons[0];
        assert_eq!(
            ranking_path(None, this_year.from_unix_s, this_year.to_unix_s).unwrap(),
            "/v1/ranking?from=2026-01-01T00:00:00Z&to=2027-01-01T00:00:00Z"
        );
        let always = seasons.last().unwrap();
        assert_eq!(
            ranking_path(None, always.from_unix_s, always.to_unix_s).unwrap(),
            "/v1/ranking"
        );
        let area = Bounds {
            south: 47.55,
            west: 11.6,
            north: 47.85,
            east: 12.0,
        };
        assert_eq!(
            ranking_path(Some(area), None, None).unwrap(),
            "/v1/ranking?bbox=11.6,47.55,12,47.85"
        );
        // A season that ends before it begins is not asked for.
        assert!(ranking_path(None, Some(10), Some(10)).is_err());

        // The development mode's August: Mara's two flights before the
        // demo pilot's one.
        let ranking = ranking(200, golden("ranking.json")).unwrap();
        let rows: Vec<(u32, &str, f64, &str, usize)> = ranking
            .pilots
            .iter()
            .map(|pilot| {
                (
                    pilot.place,
                    pilot.name.as_str(),
                    pilot.points,
                    pilot.counted.as_str(),
                    pilot.flights.len(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                (1, "Mara Muster", 23.42, "2 flights", 2),
                (2, "Demo Pilot", 11.98, "1 flight", 1),
            ]
        );
        assert_eq!(ranking.pilots[0].handle, "mara");
        // The answer says which period it is of.
        assert_eq!(
            ranking.period,
            RankingPeriod {
                year: 2026,
                month: Some("2026-08".to_owned()),
                title: "August 2026".to_owned(),
                from_unix_s: Some(1_785_542_400),
                to_unix_s: Some(1_788_220_800),
            }
        );
        let of = |from: Option<&str>, to: Option<&str>| {
            let mut answer: Value = serde_json::from_slice(&golden("ranking.json")).unwrap();
            let fields = answer.as_object_mut().unwrap();
            fields.remove("month");
            fields.remove("from");
            fields.remove("to");
            if let Some(from) = from {
                fields.insert("from".to_owned(), json!(from));
            }
            if let Some(to) = to {
                fields.insert("to".to_owned(), json!(to));
            }
            super::ranking(200, serde_json::to_vec(&answer).unwrap())
                .unwrap()
                .period
        };
        let year = of(Some("2026-01-01T00:00:00Z"), Some("2027-01-01T00:00:00Z"));
        assert_eq!((year.year, year.title.as_str()), (2026, "2026"));
        assert_eq!(year.month, None);
        let always = of(None, None);
        assert_eq!((always.year, always.title.as_str()), (0, "All years"));
        assert_eq!((always.from_unix_s, always.to_unix_s), (None, None));
        let span = of(Some("2026-03-01T00:00:00Z"), Some("2026-06-01T00:00:00Z"));
        assert_eq!(
            (span.year, span.title.as_str()),
            (0, "1 Mar 2026 to 31 May 2026")
        );
        assert_eq!(
            of(Some("2026-03-01T00:00:00Z"), None).title,
            "Since 1 Mar 2026"
        );
        assert_eq!(
            of(None, Some("2026-06-01T00:00:00Z")).title,
            "Until 31 May 2026"
        );
        // The periods the server says there are, the newest first.
        let years: Vec<&str> = ranking
            .periods
            .iter()
            .map(|year| year.words.as_str())
            .collect();
        assert_eq!(years, ["2026: 10 flights", "2025: 1 flight"]);
        let this_year = &ranking.periods[0];
        assert_eq!(this_year.whole_name, "All of 2026");
        assert_eq!(
            ranking_path(None, Some(this_year.from_unix_s), Some(this_year.to_unix_s)).unwrap(),
            "/v1/ranking?from=2026-01-01T00:00:00Z&to=2027-01-01T00:00:00Z"
        );
        let august = &this_year.months[1];
        assert_eq!(
            (august.name.as_str(), august.title.as_str(), august.flights),
            ("August", "August 2026", 4)
        );
        assert_eq!(august.words, "August 2026: 4 flights");
        assert_eq!(this_year.months[0].words, "September 2026: 1 flight");
        assert_eq!(
            ranking_month_path(None, august.month.clone()).unwrap(),
            "/v1/ranking?month=2026-08"
        );
        let area = Bounds {
            south: 47.55,
            west: 11.6,
            north: 47.85,
            east: 12.0,
        };
        assert_eq!(
            ranking_month_path(Some(area), "2026-08".to_owned()).unwrap(),
            "/v1/ranking?bbox=11.6,47.55,12,47.85&month=2026-08"
        );
        assert!(ranking_month_path(None, "August".to_owned()).is_err());
        assert_eq!(ranking.pilots[0].flights[0].pilot, "Mara Muster");
        assert_eq!(
            ranking.rule,
            "A pilot's points are those of their best 3 flights shared with everyone, added up. \
             Worked out by Venti from the flights' files, by the same rules for every \
             flight. It is not a league's result."
        );
    }

    #[test]
    fn pilots_with_the_same_points_share_a_place_and_more_flights_than_count_are_said() {
        let mut answer: Value = serde_json::from_slice(&golden("ranking.json")).unwrap();
        let first = answer["pilots"][0].clone();
        // Four pilots: the second and third with the same points, the last
        // with fourteen scored flights of which the two listed count.
        let with = |handle: &str, points: f64, scored: u32| {
            let mut pilot = first.clone();
            pilot["pilot"]["handle"] = json!(handle);
            pilot["points"] = json!(points);
            pilot["scored"] = json!(scored);
            pilot
        };
        answer["pilots"] = json!([
            with("anna", 300.0, 2),
            with("bernd", 250.5, 2),
            with("carla", 250.5, 2),
            with("dario", 100.0, 14),
        ]);
        let ranking = ranking(200, serde_json::to_vec(&answer).unwrap()).unwrap();
        let places: Vec<(u32, &str)> = ranking
            .pilots
            .iter()
            .map(|pilot| (pilot.place, pilot.handle.as_str()))
            .collect();
        assert_eq!(
            places,
            [(1, "anna"), (2, "bernd"), (2, "carla"), (4, "dario")]
        );
        assert_eq!(ranking.pilots[3].counted, "Best 2 of 14 flights");

        // A server that cannot answer says so in the app's words.
        assert!(super::ranking(503, Vec::new()).is_err());
        assert!(super::ranking(200, b"{}".to_vec()).is_err());
    }

    #[test]
    fn the_list_is_asked_for_in_the_servers_words() {
        assert_eq!(
            public_flights_path(PublicFlightsOrder::Newest, None, None, None, None).unwrap(),
            "/v1/flights/public"
        );
        // The best flights of 15 August 2026 around the Tegernsee, second
        // page.
        let area = Bounds {
            south: 47.55,
            west: 11.6,
            north: 47.85,
            east: 12.0,
        };
        let path = public_flights_path(
            PublicFlightsOrder::Best,
            Some(area),
            Some(1_786_752_000),
            Some(1_786_838_400),
            Some("abc".to_owned()),
        )
        .unwrap();
        assert_eq!(
            path,
            "/v1/flights/public?bbox=11.6,47.55,12,47.85&from=2026-08-15T00:00:00Z&to=2026-08-16T00:00:00Z&sort=score&after=abc"
        );
        // A day that ends before it begins is not asked for.
        assert!(
            public_flights_path(PublicFlightsOrder::Newest, None, Some(10), Some(10), None)
                .is_err()
        );
        // An empty mark is the first page.
        assert_eq!(
            public_flights_path(
                PublicFlightsOrder::Newest,
                None,
                None,
                None,
                Some(String::new())
            )
            .unwrap(),
            "/v1/flights/public"
        );
    }

    #[test]
    fn the_flights_from_a_place_are_asked_for_as_the_square_around_it() {
        // A kilometre and a half around the Wallberg's take-off.
        let Bounds {
            south,
            west,
            north,
            east,
        } = area_around(47.6642, 11.7967, 1500.0).unwrap();
        assert!((north - south - 2.0 * 1500.0 / 111_320.0).abs() < 1e-6);
        // Degrees of longitude are shorter there: the square is wider in them.
        assert!((east - west) > (north - south) * 1.4);
        assert!(west < 11.7967 && east > 11.7967 && south < 47.6642 && north > 47.6642);
        // No farther than fifty kilometres, whatever is asked, and what is
        // no place is not asked for.
        let wide = area_around(47.6642, 11.7967, 1e9).unwrap();
        assert!((wide.north - wide.south - 2.0 * 50_000.0 / 111_320.0).abs() < 1e-6);
        assert!(area_around(f64::NAN, 11.8, 1500.0).is_err());
        assert!(area_around(89.9, 11.8, 1500.0).is_err());
    }

    #[test]
    fn a_row_names_the_site_its_flight_took_off_from() {
        let page = public_flights(200, golden("public_flights_at_site.json")).unwrap();
        let row = &page.flights[0];
        assert_eq!(row.takeoff_name.as_deref(), Some("Musterkogel West"));
        assert_eq!(
            row.takeoff_site_id.as_deref(),
            Some("venti:0000000000000000000000s001")
        );
        // Without a site, nothing: the shell shows where it took off.
        let page = public_flights(200, golden("public_flights.json")).unwrap();
        assert!(page.flights.iter().all(|row| row.takeoff_name.is_none()));
        assert_eq!(
            public_flights_at_site_path(
                "venti:0000000000000000000000s001".to_owned(),
                PublicFlightsOrder::Best,
                None
            )
            .unwrap(),
            "/v1/flights/public?site=venti:0000000000000000000000s001&sort=score"
        );
        assert!(
            public_flights_at_site_path("no site".to_owned(), PublicFlightsOrder::Newest, None)
                .is_err()
        );
    }

    #[test]
    fn the_servers_list_becomes_rows() {
        let page = public_flights(200, golden("public_flights.json")).unwrap();
        assert!(!page.flights.is_empty());
        let raw: Value = serde_json::from_slice(&golden("public_flights.json")).unwrap();
        assert_eq!(page.flights.len(), raw["flights"].as_array().unwrap().len());
        assert_eq!(page.next.as_deref(), raw["next"].as_str());

        let (row, flight) = (&page.flights[0], &raw["flights"][0]);
        assert_eq!(row.id, flight["id"].as_str().unwrap());
        assert_eq!(row.handle, flight["pilot"]["handle"].as_str().unwrap());
        assert_eq!(row.pilot, flight["pilot"]["display_name"].as_str().unwrap());
        assert_eq!(row.airtime_s, flight["airtime_s"].as_i64().unwrap());
        assert_eq!(row.page_path, format!("/f/{}", row.id));
        assert_eq!(row.file_path, format!("/v1/flights/{}/igc", row.id));
        assert_eq!(
            row.outline.len(),
            flight["outline"].as_array().unwrap().len()
        );
        let score = row.score.unwrap();
        assert_eq!(
            score.distance_m,
            flight["score"]["distance_m"].as_f64().unwrap()
        );
        assert_eq!(score.kind, RouteKind::FreeFlight);
        assert!(row.takeoff.is_some());

        // The best first is read the same way.
        let best = public_flights(200, golden("public_flights_best.json")).unwrap();
        let points: Vec<f64> = best
            .flights
            .iter()
            .map(|flight| flight.score.unwrap().points)
            .collect();
        assert!(
            points.windows(2).all(|pair| pair[0] >= pair[1]),
            "{points:?}"
        );

        // A refusal comes in words.
        assert!(public_flights(503, Vec::new()).is_err());
    }

    #[test]
    fn a_pilots_page_is_the_pilot_and_their_public_flights() {
        assert_eq!(
            pilot_page_path("demo".to_owned(), None).unwrap(),
            "/v1/pilots/demo"
        );
        assert_eq!(
            pilot_page_path("demo".to_owned(), Some("abc".to_owned())).unwrap(),
            "/v1/pilots/demo?after=abc"
        );
        // What cannot be a handle is not asked for: it would be a path of
        // the asker's making.
        assert!(pilot_page_path("../accounts".to_owned(), None).is_err());
        assert!(pilot_page_path(String::new(), None).is_err());

        let page = pilot_page(200, golden("pilot.json")).unwrap();
        assert_eq!(
            (page.handle.as_str(), page.name.as_str()),
            ("demo", "Demo Pilot")
        );
        assert!(!page.flights.is_empty());
        assert!(page.flights.iter().all(|flight| flight.handle == "demo"));

        let unknown = pilot_page(404, b"{}".to_vec()).unwrap_err();
        assert!(unknown.to_string().contains("no pilot"), "{unknown}");
        assert!(pilot_page(503, Vec::new()).is_err());
    }

    #[test]
    fn what_another_pilot_wrote_is_one_bounded_line() {
        let mut answer: Value = serde_json::from_slice(&golden("public_flights.json")).unwrap();
        answer["flights"][0]["pilot"]["display_name"] = json!("Anna\nMaier ".repeat(20));
        answer["flights"][0]["glider"] = json!("  \n ");
        answer["flights"][1]["score"]["kind"] = json!("something_new");
        let page = public_flights(200, answer.to_string().into_bytes()).unwrap();
        assert!(page.flights[0].pilot.chars().count() <= 40);
        assert!(!page.flights[0].pilot.contains('\n'));
        // A glider of nothing but space is no glider, and a kind of route
        // this version does not know is no score.
        assert_eq!(page.flights[0].glider, None);
        assert_eq!(page.flights[1].score, None);
    }

    fn words<T: std::fmt::Debug>(result: Result<T, CoreError>) -> String {
        match result {
            Err(CoreError::Server { message }) => message,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_flight_reached_by_its_address_is_read_as_a_row() {
        let raw: Value = serde_json::from_slice(&golden("flight.json")).unwrap();
        let id = raw["flight"]["id"].as_str().unwrap();
        assert_eq!(
            public_flight_path(id.to_owned()).unwrap(),
            format!("/v1/flights/{id}")
        );
        // What is no address is never put into one.
        for text in ["", "public", "../account", "5T1ESHYPDTVRQ7RXW39A18VSRG"] {
            assert_eq!(
                words(public_flight_path(text.to_owned())),
                "This is not the address of a flight."
            );
        }

        let flight = public_flight(200, golden("flight.json")).unwrap();
        assert_eq!(flight.id, id);
        assert_eq!(
            flight.pilot,
            raw["flight"]["pilot"]["display_name"].as_str().unwrap()
        );
        assert_eq!(flight.page_path, format!("/f/{id}"));
        assert_eq!(flight.file_path, format!("/v1/flights/{id}/igc"));
    }

    #[test]
    fn a_flight_that_is_not_there_for_the_asker_is_no_longer_public() {
        let gone = br#"{"error":{"code":"flight_not_found","message":"x"}}"#.to_vec();
        assert_eq!(
            words(public_flight(404, gone)),
            "This flight is no longer public: its pilot took it back, or deleted it."
        );
        // Anything else wrong with the server is said as everywhere.
        assert_eq!(
            words(public_flight(503, Vec::new())),
            "The server is busy or being restarted. Try again in a moment."
        );
    }

    #[test]
    fn another_pilots_flight_is_shown_from_its_file_like_ones_own() {
        let text = venti_flight::igc::write(&venti_flight::testing::synthetic_flight());
        let flight = crate::flight_from_file(text.clone()).unwrap();
        assert_eq!(flight.id, "");
        assert!(flight.track.len() > 1000);
        assert_eq!(flight.pilot.as_deref(), Some("Sample Pilot"));
        let analysis = crate::flight_analysis_from_file(text).unwrap();
        assert_eq!(analysis.thermals.len(), 2);
        // What is no flight file is said so, and not shown.
        assert!(crate::flight_from_file("<html>not found</html>".to_owned()).is_err());
        assert!(crate::flight_analysis_from_file(String::new()).is_err());
    }
}
