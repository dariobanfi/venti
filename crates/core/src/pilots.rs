//! Pilots of a Venti server, as everyone may see them: the search by name
//! and what a pilot's public flights add up to.
//!
//! As everywhere, the core does not talk to the server: one function says
//! what to ask for, another reads the answer into lines and rows for a
//! screen. A pilot's flights themselves are a page of
//! [`pilot_page_path`](crate::pilot_page_path), the newest first.
//!
//! When the pilot's page has a season chosen ([`PilotTotalsView`] of a
//! year), the list under the totals is that season's: the shell asks
//! [`pilot_flights_path`] with the season's `from_unix_s` and `to_unix_s`
//! and reads the answer with [`public_flights`](crate::public_flights),
//! page by page with its `next`. For "All years" both are absent and the
//! list is the pilot's page as before. The words around all of it, the
//! same in the three apps, are [`pilots_words`].
//!
//! A name another pilot chose is cut to one bounded line before it
//! reaches a screen.

use venti_api_types::v1::{
    self as api, PILOT_SEARCH_MIN_CHARS, PilotStatisticsResponse, PilotTotals, PilotsRequest,
    PilotsResponse, PublicFlightsRequest,
};

use crate::community::{ALL_YEARS, Which, first_second_of, which};
use crate::live::one_line;
use crate::server::answer;
use crate::takeoffs::{count, months, terms};
use crate::{Coordinate, CoreError, FlownMonth, PublicFlightTerms};

/// A pilot a search found, as a line of a list.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FoundPilotLine {
    /// The pilot's handle: for [`crate::pilot_page_path`],
    /// [`pilot_statistics_path`] and the list of flights.
    #[uniffi(default = "")]
    pub handle: String,
    /// The name the pilot is shown by, on one line.
    #[uniffi(default = "")]
    pub name: String,
    /// How many public flights the pilot has.
    #[uniffi(default = 0)]
    pub flights: u32,
    /// The path of the pilot's picture, to fetch with the server's address
    /// in front. `None` without one.
    #[uniffi(default = None)]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub picture_path: Option<String>,
    /// The line: "Lena Probe (@lena), 3 public flights".
    #[uniffi(default = "")]
    pub words: String,
}

/// What a search for pilots found.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PilotsFound {
    /// The pilots: those whose handle or name begins with the text first,
    /// then the pilot with the most public flights.
    #[uniffi(default = [])]
    pub pilots: Vec<FoundPilotLine>,
    /// The result in a line: "2 pilots", or that nobody was found.
    #[uniffi(default = "")]
    pub words: String,
}

/// The path and query to `GET` from the server for the pilots whose
/// handle or name holds `text`. Text of fewer than two characters is not
/// asked for and is refused in words: a field shows them under itself.
#[uniffi::export]
pub fn pilots_search_path(text: String) -> Result<String, CoreError> {
    let text = text.trim();
    if text.chars().count() < PILOT_SEARCH_MIN_CHARS {
        return Err(CoreError::Server {
            message: "Type at least two letters of a pilot's name or handle.".to_owned(),
        });
    }
    Ok(PilotsRequest {
        q: text.chars().take(api::PILOT_SEARCH_MAX_CHARS).collect(),
        limit: None,
    }
    .path_and_query())
}

/// Reads the server's answer to [`pilots_search_path`].
#[uniffi::export]
pub fn pilots_found(status: u16, body: Vec<u8>) -> Result<PilotsFound, CoreError> {
    let response: PilotsResponse = answer(status, &body)?;
    let pilots: Vec<FoundPilotLine> = response
        .pilots
        .iter()
        .map(|found| {
            let name = one_line(&found.pilot.display_name, 40);
            FoundPilotLine {
                words: format!(
                    "{name} (@{}), {}",
                    found.pilot.handle,
                    count(found.flights, "public flight", "public flights")
                ),
                handle: found.pilot.handle.to_string(),
                name,
                flights: found.flights,
                picture_path: found.pilot.picture.clone(),
            }
        })
        .collect();
    let found = u32::try_from(pilots.len()).unwrap_or(u32::MAX);
    Ok(PilotsFound {
        words: if pilots.is_empty() {
            "No pilot by this name has shared a flight with everyone.".to_owned()
        } else {
            count(found, "pilot", "pilots")
        },
        pilots,
    })
}

/// One number of a pilot's totals, as a row of a table.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PilotTotalRow {
    /// What it is: "Flights", "Distance scored".
    #[uniffi(default = "")]
    pub label: String,
    /// The number with its unit: "3", "28.2 km", "1 h 04 min".
    #[uniffi(default = "")]
    pub value: String,
    /// The flight the row is about, by its address on the server, for a
    /// link: the longest flight, the highest. Empty for a sum.
    #[uniffi(default = "")]
    pub flight_id: String,
}

/// A take-off site with how often a pilot flew from it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FlownSiteLine {
    /// The site's identifier: for its page and for the list of flights
    /// from it.
    #[uniffi(default = "")]
    pub id: String,
    /// The site's name, on one line.
    #[uniffi(default = "")]
    pub name: String,
    /// How many of the flights took off from it.
    #[uniffi(default = 0)]
    pub flights: u32,
    /// Where the site stands, for a map or a link. Absent from a server
    /// that does not say.
    #[uniffi(default = None)]
    pub position: Option<Coordinate>,
    /// The line: "Musterkogel West, 2 flights".
    #[uniffi(default = "")]
    pub words: String,
}

/// What some of a pilot's public flights add up to: all of them, or
/// those of one year.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PilotTotalsView {
    /// What the flights are: "All years", "2026".
    #[uniffi(default = "")]
    pub name: String,
    /// The year, for the list of the pilot's flights of it. 0 for all
    /// years.
    #[uniffi(default = 0)]
    pub year: i32,
    /// The year's first second, seconds since 1970-01-01 UTC, for
    /// [`pilot_flights_path`]. Absent for all years.
    #[uniffi(default = None)]
    pub from_unix_s: Option<i64>,
    /// The first second after the year. Absent for all years.
    #[uniffi(default = None)]
    pub to_unix_s: Option<i64>,
    /// The numbers as rows, in the order a table shows them: flights,
    /// distance scored, points, airtime, the longest flight, the highest
    /// point. A row whose number there is none of is left out.
    #[uniffi(default = [])]
    pub rows: Vec<PilotTotalRow>,
    /// The take-off sites flown from most, the most first: at most five.
    #[uniffi(default = [])]
    pub sites: Vec<FlownSiteLine>,
    /// The twelve months, January first.
    #[uniffi(default = [])]
    pub months: Vec<FlownMonth>,
    /// The totals in a sentence: "3 flights, 28.2 km scored, 1 h 04 min
    /// in the air".
    #[uniffi(default = "")]
    pub words: String,
}

/// What a pilot's public flights add up to.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PilotStatistics {
    /// The pilot's handle.
    #[uniffi(default = "")]
    pub handle: String,
    /// The name the pilot is shown by, on one line.
    #[uniffi(default = "")]
    pub name: String,
    /// The path of the pilot's picture, to fetch with the server's address
    /// in front. `None` without one.
    #[uniffi(default = None)]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub picture_path: Option<String>,
    /// All public flights together.
    pub overall: PilotTotalsView,
    /// Each year the pilot has a public flight in, the newest first.
    #[uniffi(default = [])]
    pub seasons: Vec<PilotTotalsView>,
    /// What the numbers are made of, for under them.
    #[uniffi(default = "")]
    pub note: String,
    /// Under what the flights are given.
    pub terms: PublicFlightTerms,
}

/// The path to `GET` from the server for what a pilot's public flights
/// add up to.
#[uniffi::export]
pub fn pilot_statistics_path(handle: String) -> Result<String, CoreError> {
    let handle = handle.parse().map_err(|_| CoreError::Server {
        message: "This is not the handle of a pilot.".to_owned(),
    })?;
    Ok(api::pilot_statistics_path(&handle))
}

/// The path and query to `GET` from the server for one page of a pilot's
/// public flights of a season, the newest first: those that began at or
/// after `from_unix_s` and before `to_unix_s`, as a [`PilotTotalsView`]
/// names them. `after` is the `next` of the page before. The answer is
/// read by [`public_flights`](crate::public_flights).
///
/// With neither bound it is all of the pilot's public flights; the
/// pilot's page ([`crate::pilot_page_path`]) lists those with the pilot's
/// name.
#[uniffi::export]
pub fn pilot_flights_path(
    handle: String,
    from_unix_s: Option<i64>,
    to_unix_s: Option<i64>,
    after: Option<String>,
) -> Result<String, CoreError> {
    let handle = handle.parse().map_err(|_| CoreError::Server {
        message: "This is not the handle of a pilot.".to_owned(),
    })?;
    let Which { from, to, .. } = which(
        None,
        from_unix_s,
        to_unix_s,
        "The app could not ask the server for these flights.",
    )?;
    Ok(PublicFlightsRequest {
        from,
        to,
        after: after.filter(|after| !after.is_empty()),
        ..PublicFlightsRequest::default()
    }
    .path_and_query_of(&handle))
}

/// The words around pilots, their totals and the ranking that are the
/// same in the three apps.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PilotsWords {
    /// The search field's placeholder: "Find a pilot".
    #[uniffi(default = "")]
    pub search_placeholder: String,
    /// The name of the button that empties the field: "Clear the search".
    #[uniffi(default = "")]
    pub search_clear: String,
    /// The line when the server cannot be reached.
    #[uniffi(default = "")]
    pub unreachable: String,
    /// The heading over a pilot's take-off sites: "Take-offs flown from
    /// most".
    #[uniffi(default = "")]
    pub sites_title: String,
    /// The heading over a pilot's twelve months: "Flights by month".
    #[uniffi(default = "")]
    pub months_title: String,
    /// The name of the choice of a season, on a pilot's page and over the
    /// ranking: "Season".
    #[uniffi(default = "")]
    pub season_choice: String,
    /// The choice of all years together, beside the years: "All years".
    #[uniffi(default = "")]
    pub all_years: String,
    /// The sentence when a month of the ranking has no ranked flight.
    #[uniffi(default = "")]
    pub month_empty: String,
}

/// The words around pilots, their totals and the ranking. The whole of
/// one year beside its months is the year's own `whole_name`
/// ([`crate::RankingYearChoice`]): "All of 2026".
#[uniffi::export]
pub fn pilots_words() -> PilotsWords {
    PilotsWords {
        search_placeholder: "Find a pilot".to_owned(),
        search_clear: "Clear the search".to_owned(),
        unreachable: "The server cannot be reached just now. Pilots and their flights are \
                      shown again once it answers."
            .to_owned(),
        sites_title: "Take-offs flown from most".to_owned(),
        months_title: "Flights by month".to_owned(),
        season_choice: "Season".to_owned(),
        all_years: ALL_YEARS.to_owned(),
        month_empty: "No flight shared with everyone scored in this month.".to_owned(),
    }
}

/// "2 h 14 min", "14 min": airtime added up, in whole minutes.
fn airtime(seconds: i64) -> String {
    let minutes = (seconds.max(0) + 30) / 60;
    if minutes < 60 {
        format!("{minutes} min")
    } else {
        format!("{} h {:02} min", minutes / 60, minutes % 60)
    }
}

/// "45.2 km".
fn kilometres(metres: f64) -> String {
    format!("{:.1} km", metres / 1000.0)
}

pub(crate) fn totals(
    name: String,
    year: i32,
    counted: u32,
    totals: &PilotTotals,
) -> PilotTotalsView {
    let row = |label: &str, value: String| PilotTotalRow {
        label: label.to_owned(),
        value,
        flight_id: String::new(),
    };
    let mut rows = vec![row("Flights", totals.flights.to_string())];
    if totals.scored > 0 {
        rows.push(row("Distance scored", kilometres(totals.distance_m)));
        let best = match counted.min(totals.scored) {
            1 => "Points, best flight".to_owned(),
            best => format!("Points, best {best} flights"),
        };
        rows.push(row(&best, format!("{:.2}", totals.points)));
    }
    rows.push(row("Airtime", airtime(totals.airtime_s)));
    if let Some(longest) = &totals.longest {
        rows.push(PilotTotalRow {
            flight_id: longest.id.to_string(),
            ..row("Longest flight", kilometres(longest.distance_m))
        });
    }
    if let Some(highest) = &totals.highest {
        rows.push(PilotTotalRow {
            flight_id: highest.id.to_string(),
            ..row("Highest point", format!("{} m", highest.max_altitude_m))
        });
    }
    let flights = count(totals.flights, "flight", "flights");
    let in_the_air = airtime(totals.airtime_s);
    // The span a season's flights are listed by: the calendar year by
    // UTC. The server counts a flight in the year of the clock where it
    // took off, which differs only near midnight on New Year's Eve.
    let first_second = |year: i32| first_second_of(year).map(|start| start.timestamp());
    let (from_unix_s, to_unix_s) = if year == 0 {
        (None, None)
    } else {
        (first_second(year), first_second(year + 1))
    };
    PilotTotalsView {
        name,
        year,
        from_unix_s,
        to_unix_s,
        rows,
        sites: site_lines(&totals.sites),
        months: months(&totals.by_month),
        words: if totals.scored > 0 {
            format!(
                "{flights}, {} scored, {in_the_air} in the air",
                kilometres(totals.distance_m)
            )
        } else {
            format!("{flights}, {in_the_air} in the air")
        },
    }
}

/// Take-off sites as lines: "Musterkogel West, 2 flights".
pub(crate) fn site_lines(sites: &[api::FlownSite]) -> Vec<FlownSiteLine> {
    sites
        .iter()
        .map(|flown| {
            let name = one_line(&flown.site.name, 60);
            FlownSiteLine {
                id: flown.site.id.to_string(),
                words: format!("{name}, {}", count(flown.flights, "flight", "flights")),
                name,
                flights: flown.flights,
                position: flown.position.map(|at| Coordinate {
                    lat: at.lat(),
                    lon: at.lon(),
                }),
            }
        })
        .collect()
}

/// Reads the server's answer to [`pilot_statistics_path`]. A pilot the
/// server does not know, or one who shares nothing with everyone, is said
/// in words.
#[uniffi::export]
pub fn pilot_statistics(status: u16, body: Vec<u8>) -> Result<PilotStatistics, CoreError> {
    if status == 404 {
        return Err(CoreError::Server {
            message: "The server knows no pilot by this handle.".to_owned(),
        });
    }
    let response: PilotStatisticsResponse = answer(status, &body)?;
    Ok(statistics(
        &response,
        "Of the flights this pilot shared with everyone.",
    ))
}

/// Reads the server's answer to [`crate::own_statistics_request`]: the
/// pilot's own statistics, over every flight of theirs, the private ones
/// too, in the shape of [`pilot_statistics`] with a note that says so.
#[uniffi::export]
pub fn own_statistics(status: u16, body: Vec<u8>) -> Result<PilotStatistics, CoreError> {
    let response: PilotStatisticsResponse = answer(status, &body)?;
    Ok(statistics(
        &response,
        "Of all your flights on the server, the private ones too.",
    ))
}

fn statistics(response: &PilotStatisticsResponse, note: &str) -> PilotStatistics {
    let counted = response.counted;
    PilotStatistics {
        handle: response.pilot.handle.to_string(),
        name: one_line(&response.pilot.display_name, 40),
        picture_path: response.pilot.picture.clone(),
        overall: totals(ALL_YEARS.to_owned(), 0, counted, &response.overall),
        seasons: response
            .seasons
            .iter()
            .map(|season| {
                totals(
                    season.year.to_string(),
                    season.year,
                    counted,
                    &season.totals,
                )
            })
            .collect(),
        note: note.to_owned(),
        terms: terms(&response.terms),
    }
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
    fn pilots_are_searched_by_two_letters_or_more_and_read_into_lines() {
        assert_eq!(
            pilots_search_path(" le ".to_owned()).unwrap(),
            "/v1/pilots?q=le"
        );
        assert_eq!(
            pilots_search_path("Lena Probe".to_owned()).unwrap(),
            "/v1/pilots?q=Lena%20Probe"
        );
        for short in ["", "l", " l "] {
            assert!(pilots_search_path(short.to_owned()).is_err(), "{short:?}");
        }
        let long = pilots_search_path("a".repeat(200)).unwrap();
        assert_eq!(long.len(), "/v1/pilots?q=".len() + 60);

        let found = pilots_found(200, golden("pilots.json")).unwrap();
        assert_eq!(found.words, "1 pilot");
        assert_eq!(
            found.pilots[0].words,
            "Lena Probe (@lena), 3 public flights"
        );
        assert_eq!(found.pilots[0].handle, "lena");
        assert_eq!(found.pilots[0].flights, 3);

        let one = json!({"pilots": [
            {"pilot": {"handle": "ines", "display_name": "Ines\nVorlage"}, "flights": 1}
        ]});
        let found = pilots_found(200, serde_json::to_vec(&one).unwrap()).unwrap();
        assert_eq!(
            found.pilots[0].words,
            "Ines Vorlage (@ines), 1 public flight"
        );
        let nobody = pilots_found(200, br#"{"pilots": []}"#.to_vec()).unwrap();
        assert!(nobody.pilots.is_empty());
        assert_eq!(
            nobody.words,
            "No pilot by this name has shared a flight with everyone."
        );
        assert!(pilots_found(503, Vec::new()).is_err());
        assert!(pilots_found(200, b"{}".to_vec()).is_err());
    }

    #[test]
    fn a_pilots_totals_are_rows_sites_and_twelve_months() {
        assert_eq!(
            pilot_statistics_path("lena".to_owned()).unwrap(),
            "/v1/pilots/lena/statistics"
        );
        assert!(pilot_statistics_path("No Handle".to_owned()).is_err());

        let said = pilot_statistics(200, golden("pilot_statistics.json")).unwrap();
        assert_eq!(
            (said.handle.as_str(), said.name.as_str()),
            ("lena", "Lena Probe")
        );
        let rows: Vec<(&str, &str)> = said
            .overall
            .rows
            .iter()
            .map(|row| (row.label.as_str(), row.value.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("Flights", "3"),
                ("Distance scored", "28.2 km"),
                ("Points, best 3 flights", "28.19"),
                ("Airtime", "1 h 04 min"),
                ("Longest flight", "12.0 km"),
                ("Highest point", "2117 m"),
            ]
        );
        assert_eq!(said.overall.name, "All years");
        assert_eq!(
            said.overall.words,
            "3 flights, 28.2 km scored, 1 h 04 min in the air"
        );
        // The longest and the highest flight are named for a link.
        assert_eq!(said.overall.rows[4].flight_id, "ww5qkxhxaedefvypb3wy3v6m3m");
        assert!(said.overall.rows[0].flight_id.is_empty());
        assert_eq!(said.overall.sites.len(), 1);
        assert_eq!(said.overall.sites[0].words, "Musterkogel West, 2 flights");
        assert_eq!(said.overall.sites[0].id, "venti:0000000000000000000000s001");
        assert_eq!(
            said.overall.sites[0].position,
            Some(Coordinate {
                lat: 47.3,
                lon: 10.9
            })
        );
        assert_eq!(said.overall.months.len(), 12);
        assert_eq!(said.overall.months[4].words, "May: 1 flight");
        assert_eq!(said.overall.months[4].share, 1.0);
        assert_eq!(said.overall.months[0].flights, 0);
        let seasons: Vec<(&str, i32)> = said
            .seasons
            .iter()
            .map(|season| (season.name.as_str(), season.year))
            .collect();
        assert_eq!(seasons, [("2026", 2026)]);
        // A season's list of flights is asked for by its bounds.
        let season = &said.seasons[0];
        assert_eq!(
            (season.from_unix_s, season.to_unix_s),
            (Some(1_767_225_600), Some(1_798_761_600))
        );
        assert_eq!(
            pilot_flights_path(
                said.handle.clone(),
                season.from_unix_s,
                season.to_unix_s,
                None
            )
            .unwrap(),
            "/v1/flights/public?from=2026-01-01T00:00:00Z&to=2027-01-01T00:00:00Z&pilot=lena"
        );
        assert_eq!(
            (said.overall.from_unix_s, said.overall.to_unix_s),
            (None, None)
        );
        assert_eq!(
            pilot_flights_path("lena".to_owned(), None, None, Some("c1".to_owned())).unwrap(),
            "/v1/flights/public?after=c1&pilot=lena"
        );
        assert!(pilot_flights_path("No Handle".to_owned(), None, None, None).is_err());
        assert!(pilot_flights_path("lena".to_owned(), Some(10), Some(10), None).is_err());
        assert_eq!(said.seasons[0].rows, said.overall.rows);
        assert!(!said.terms.decided);

        // A pilot whose one flight is too short to score has no distance
        // and no points to show.
        let mut answer: Value = serde_json::from_slice(&golden("pilot_statistics.json")).unwrap();
        let overall = &mut answer["overall"];
        overall["flights"] = json!(1);
        overall["scored"] = json!(0);
        overall["airtime_s"] = json!(840);
        overall.as_object_mut().unwrap().remove("longest");
        let said = pilot_statistics(200, serde_json::to_vec(&answer).unwrap()).unwrap();
        let labels: Vec<&str> = said
            .overall
            .rows
            .iter()
            .map(|row| row.label.as_str())
            .collect();
        assert_eq!(labels, ["Flights", "Airtime", "Highest point"]);
        assert_eq!(said.overall.words, "1 flight, 14 min in the air");

        // An older server names no site's position.
        let mut answer: Value = serde_json::from_slice(&golden("pilot_statistics.json")).unwrap();
        answer["overall"]["sites"][0]
            .as_object_mut()
            .unwrap()
            .remove("position");
        let said = pilot_statistics(200, serde_json::to_vec(&answer).unwrap()).unwrap();
        assert_eq!(said.overall.sites[0].position, None);

        assert!(pilot_statistics(404, Vec::new()).is_err());
        assert!(pilot_statistics(503, Vec::new()).is_err());
        assert!(pilot_statistics(200, b"{}".to_vec()).is_err());
    }

    #[test]
    fn the_pilots_own_statistics_say_they_count_every_flight() {
        let public = pilot_statistics(200, golden("pilot_statistics.json")).unwrap();
        let own = own_statistics(200, golden("account_statistics.json")).unwrap();
        assert!(
            public.note.contains("shared with everyone"),
            "{}",
            public.note
        );
        assert!(own.note.contains("private ones too"), "{}", own.note);
        assert!(!own.note.contains("shared with everyone"));
        assert!(own_statistics(401, Vec::new()).is_err());
    }

    #[test]
    fn the_words_are_asked_for_once() {
        let words = pilots_words();
        assert_eq!(words.search_placeholder, "Find a pilot");
        assert_eq!(words.all_years, "All years");
        assert_eq!(words.sites_title, "Take-offs flown from most");
        assert!(!words.unreachable.contains("  "));
    }
}
