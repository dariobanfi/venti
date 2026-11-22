//! A pilot's own logbook for the connector's tools: `logbook_statistics`,
//! `list_flights` and `get_flight`, answered for one account from what the
//! server holds of its flights, the private ones too.
//!
//! These are the tools the app's own assistant answers from the logbook on
//! the device, defined once in `venti-tools`; here the server is their
//! [`Host`]. The flights are read by the methods the API's own handlers
//! call for `GET /v1/flights` and `GET /v1/flights/{id}`
//! ([`Api::flight_page`], [`Api::visible_flight`]), never by a second way
//! to the store, and always for the account of the token that asked: no
//! argument of a tool names an account.
//!
//! The server keeps a flight's summary and its file. It does not keep the
//! thermals and glides the app works out on the device, and a result says
//! so instead of leaving them out silently.

use std::collections::BTreeMap;

use chrono::{Datelike, Months};
use serde_json::{Value, json};
use venti_domain::Timestamp;
use venti_store::{Account, Flight, Listing, ScoreKind, Store, Visibility};
use venti_tools::{Host, ToolError, source_text};

use super::Api;
use super::auth::Viewer;
use super::error::ApiError;

/// Where every result says it comes from.
const SOURCE: &str = "the pilot's logbook on the Venti server";

/// The most flights a question about the whole logbook reads: the newest.
/// A pilot who flies every day of the year for five years has fewer.
const MOST_FLIGHTS: usize = 2000;

/// How many flights are read at a time: the API's largest page.
const PAGE: &str = "100";

/// The most characters of a glider's name that are passed on.
const MAX_GLIDER_CHARS: usize = 40;

/// The flights of one account that a tool call needs, read before the
/// tool runs: a tool's own code reads no store.
pub(crate) struct Logbook {
    flights: Vec<Flight>,
    /// Whether these are all the account's flights, or the newest
    /// [`MOST_FLIGHTS`] of more.
    whole: bool,
    now: Timestamp,
}

impl<S: Store> Api<S> {
    /// What `tool` needs of the logbook of `account`, for whom `owner` is
    /// allowed to look: every flight for a question about the logbook, the
    /// one flight for `get_flight`.
    pub(crate) async fn logbook_for(
        &self,
        account: &Account,
        owner: &Viewer,
        tool: &str,
        input: &Value,
    ) -> Result<Logbook, ApiError> {
        let now = self.clock.now();
        if tool == "get_flight" {
            let flight = match input["flight_id"].as_str() {
                Some(id) => match self.visible_flight(id, owner).await {
                    Ok(flight) => Some(flight),
                    Err(error) if error.is_missing() => None,
                    Err(error) => return Err(error),
                },
                None => None,
            };
            // The pilot's own flights, and no others: a public flight of
            // another pilot is read with `public_flight`, which says under
            // what terms.
            let own = flight.filter(|flight| flight.account_id == account.id);
            return Ok(Logbook {
                flights: own.into_iter().collect(),
                whole: true,
                now,
            });
        }
        let mut flights = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let (page, next) = self
                .flight_page(&account.id, Listing::All, after.as_deref(), Some(PAGE))
                .await?;
            flights.extend(page);
            match next {
                Some(next) if flights.len() < MOST_FLIGHTS => after = Some(next),
                next => {
                    return Ok(Logbook {
                        flights,
                        whole: next.is_none(),
                        now,
                    });
                }
            }
        }
    }
}

fn tenth(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn hours(seconds: i64) -> f64 {
    tenth(seconds as f64 / 3600.0)
}

fn date_time(time: Timestamp) -> String {
    time.format("%Y-%m-%d %H:%M UTC").to_string()
}

fn scored_as(flight: &Flight) -> Option<&'static str> {
    flight
        .derived
        .score
        .map(|score| match (score.kind, score.closed) {
            (ScoreKind::FreeFlight, _) => "free flight",
            (ScoreKind::FreeTriangle, false) => "free triangle",
            (ScoreKind::FreeTriangle, true) => "closed free triangle",
            (ScoreKind::FaiTriangle, false) => "FAI triangle",
            (ScoreKind::FaiTriangle, true) => "closed FAI triangle",
        })
}

/// The glider as the flight's file names it: text somebody typed into an
/// instrument, bounded and made one line as every other such text is.
fn glider(flight: &Flight) -> Option<String> {
    flight
        .derived
        .glider
        .as_deref()
        .map(|glider| source_text(glider, MAX_GLIDER_CHARS))
        .filter(|glider| !glider.is_empty())
}

/// Who can see a flight, in the words the apps use.
fn shared(flight: &Flight) -> &'static str {
    match flight.visibility {
        Visibility::Private => "private: only the pilot",
        Visibility::Unlisted => "by link: whoever has its link",
        Visibility::Public => "public: everyone",
    }
}

/// What the flights add up to.
#[derive(Default)]
struct Sum {
    flights: u32,
    airtime_s: i64,
    distance_m: f64,
    points: f64,
}

impl Sum {
    fn add(&mut self, flight: &Flight) {
        self.flights += 1;
        self.airtime_s += flight.derived.airtime_s.max(0);
        if let Some(score) = flight.derived.score {
            self.distance_m += score.distance_m;
            self.points += score.points;
        }
    }
}

impl Logbook {
    /// What a result says when it was made from a part of the logbook.
    fn counted(&self) -> Option<String> {
        (!self.whole).then(|| {
            format!(
                "the newest {} flights; the logbook holds more",
                self.flights.len()
            )
        })
    }

    fn best(&self, unit: &str, value: impl Fn(&Flight) -> Option<f64>) -> Value {
        let best = self
            .flights
            .iter()
            .filter_map(|flight| Some((value(flight)?, flight)))
            .max_by(|a, b| a.0.total_cmp(&b.0));
        match best {
            Some((value, flight)) => json!({
                "flight_id": flight.id,
                "date": date_time(flight.derived.start),
                unit: tenth(value),
            }),
            None => Value::Null,
        }
    }
}

impl Host for Logbook {
    fn logbook_statistics(&self) -> Result<Value, ToolError> {
        let mut total = Sum::default();
        let mut years: BTreeMap<i32, Sum> = BTreeMap::new();
        let mut months: BTreeMap<(i32, u32), Sum> = BTreeMap::new();
        let mut gliders: BTreeMap<String, Sum> = BTreeMap::new();
        // The twelve calendar months that end with this one.
        let first_month = self
            .now
            .date_naive()
            .with_day(1)
            .and_then(|day| day.checked_sub_months(Months::new(11)));
        for flight in &self.flights {
            let start = flight.derived.start;
            total.add(flight);
            years.entry(start.year()).or_default().add(flight);
            if first_month.is_some_and(|first| start.date_naive() >= first) {
                months
                    .entry((start.year(), start.month()))
                    .or_default()
                    .add(flight);
            }
            if let Some(glider) = glider(flight) {
                gliders.entry(glider).or_default().add(flight);
            }
        }
        let score = |flight: &Flight| flight.derived.score;
        let mut result = json!({
            "source": SOURCE,
            "total": {
                "flights": total.flights,
                "airtime_h": hours(total.airtime_s),
                "cross_country_km": tenth(total.distance_m / 1000.0),
                "cross_country_points": tenth(total.points),
            },
            "by_year": years.iter().rev().map(|(year, sum)| json!({
                "year": year,
                "flights": sum.flights,
                "airtime_h": hours(sum.airtime_s),
                "cross_country_km": tenth(sum.distance_m / 1000.0),
            })).collect::<Vec<_>>(),
            "last_twelve_months": months.iter().map(|((year, month), sum)| json!({
                "month": format!("{year}-{month:02}"),
                "flights": sum.flights,
                "airtime_h": hours(sum.airtime_s),
                "cross_country_km": tenth(sum.distance_m / 1000.0),
            })).collect::<Vec<_>>(),
            "gliders": gliders.iter().map(|(glider, sum)| json!({
                "glider": glider,
                "flights": sum.flights,
                "airtime_h": hours(sum.airtime_s),
            })).collect::<Vec<_>>(),
            "bests": {
                "longest": self.best("airtime_h", |flight| {
                    Some(flight.derived.airtime_s as f64 / 3600.0)
                }),
                "farthest": self.best("cross_country_km", |flight| {
                    score(flight).map(|score| score.distance_m / 1000.0)
                }),
                "most_points": self.best("points", |flight| score(flight).map(|score| score.points)),
                "highest": self.best("altitude_m", |flight| {
                    Some(f64::from(flight.derived.max_altitude_m))
                }),
                "best_climb": self.best("climb_m_per_s", |flight| Some(flight.derived.max_climb_ms)),
            },
            "scoring": "Distances and points are Venti's own reckoning by the World XContest's \
                        rules. They are no league's result.",
        });
        if let Some(counted) = self.counted() {
            result["counted"] = json!(counted);
        }
        Ok(result)
    }

    fn list_flights(&self, year: Option<i32>, limit: usize) -> Result<Value, ToolError> {
        let matching: Vec<&Flight> = self
            .flights
            .iter()
            .filter(|flight| year.is_none_or(|year| flight.derived.start.year() == year))
            .collect();
        let flights: Vec<Value> = matching
            .iter()
            .take(limit)
            .map(|flight| {
                let score = flight.derived.score;
                json!({
                    "flight_id": flight.id,
                    "takeoff": date_time(flight.derived.start),
                    "airtime_min": flight.derived.airtime_s.max(0) / 60,
                    "scored_as": scored_as(flight),
                    "cross_country_km": score.map(|score| tenth(score.distance_m / 1000.0)),
                    "points": score.map(|score| tenth(score.points)),
                    "highest_m": flight.derived.max_altitude_m,
                    "glider": glider(flight),
                    "shared": shared(flight),
                })
            })
            .collect();
        let mut result = json!({
            "source": SOURCE,
            "flights_matching": matching.len(),
            "flights_listed": flights.len(),
            "flights": flights,
        });
        if let Some(counted) = self.counted() {
            result["counted"] = json!(counted);
        }
        Ok(result)
    }

    fn flight(&self, flight_id: &str) -> Result<Value, ToolError> {
        let Some(flight) = self.flights.iter().find(|flight| flight.id == flight_id) else {
            // The answer for a flight that is not there and for one that
            // is another pilot's: this tool reads the pilot's own.
            return Err(ToolError::Failed(
                "There is no flight with this id in the pilot's logbook on the Venti server. \
                 Ids come from list_flights or logbook_statistics."
                    .to_owned(),
            ));
        };
        let derived = &flight.derived;
        let place = |place: &venti_store::Place| {
            json!({
                "time": date_time(place.time),
                "latitude": place.position.lat(),
                "longitude": place.position.lon(),
                "altitude_m": place.altitude_m,
            })
        };
        Ok(json!({
            "source": SOURCE,
            "flight_id": flight.id,
            "takeoff": date_time(derived.start),
            "took_off_at": derived.takeoff.as_ref().map(place),
            "landed_at": derived.landing.as_ref().map(place),
            "landed": derived.landing.is_some(),
            "airtime_min": derived.airtime_s.max(0) / 60,
            "glider": glider(flight),
            "highest_m": derived.max_altitude_m,
            "best_climb_m_per_s": tenth(derived.max_climb_ms),
            "track_length_km": tenth(derived.track_length_m / 1000.0),
            "score": derived.score.map(|score| json!({
                "scored_as": scored_as(flight),
                "distance_km": tenth(score.distance_m / 1000.0),
                "points": tenth(score.points),
                "rules": "World XContest, worked out by Venti; no league's result",
            })),
            "shared": shared(flight),
            "uploaded": date_time(flight.uploaded_at),
            "not_included": "Thermals, glides, the lowest point and the strongest sink: the \
                             server keeps a flight's summary and its file, and the Venti app \
                             works those out on the pilot's device.",
        }))
    }
}

#[cfg(test)]
mod tests {
    use venti_flight::igc;
    use venti_flight::testing::synthetic_flight;

    use super::*;
    use crate::flights::derive::derive;

    fn flown_on(day: &str, visibility: Visibility) -> Flight {
        let text = igc::write(&synthetic_flight())
            .replace("HFDTEDATE:040726", &format!("HFDTEDATE:{day}"));
        let derived = derive(text.as_bytes()).unwrap();
        Flight {
            id: format!("{:0>26}", day),
            account_id: "00000000000000000000000001".to_owned(),
            pilot_handle: "dario".to_owned(),
            pilot_name: "Dario".to_owned(),
            identity: day.to_owned(),
            visibility,
            uploaded_at: crate::adapters::recorded_at(),
            file_bytes: 1,
            derived: derived.derived,
            change: 1,
            taken_down_at: None,
            published_at: None,
            public_change: None,
        }
    }

    fn logbook(flights: Vec<Flight>, whole: bool) -> Logbook {
        Logbook {
            flights,
            whole,
            now: "2026-11-18T19:05:00Z".parse().unwrap(),
        }
    }

    #[test]
    fn the_logbook_adds_up_by_year_month_and_glider() {
        let book = logbook(
            vec![
                flown_on("040726", Visibility::Private),
                flown_on("050726", Visibility::Public),
                flown_on("040725", Visibility::Unlisted),
            ],
            true,
        );
        let statistics = book.logbook_statistics().unwrap();
        assert_eq!(statistics["source"], SOURCE);
        assert_eq!(statistics["total"]["flights"], 3);
        let years = statistics["by_year"].as_array().unwrap();
        assert_eq!(years[0]["year"], 2026);
        assert_eq!(years[0]["flights"], 2);
        assert_eq!(years[1]["year"], 2025);
        // July 2026 is within the twelve months before November 2026;
        // July 2025 is not.
        let months = statistics["last_twelve_months"].as_array().unwrap();
        assert_eq!(months.len(), 1);
        assert_eq!(months[0]["month"], "2026-07");
        assert_eq!(months[0]["flights"], 2);
        assert_eq!(statistics["gliders"][0]["flights"], 3);
        assert!(statistics["bests"]["longest"]["flight_id"].is_string());
        assert!(statistics.get("counted").is_none());

        let part = logbook(vec![flown_on("040726", Visibility::Private)], false);
        let statistics = part.logbook_statistics().unwrap();
        assert!(statistics["counted"].as_str().unwrap().contains("newest 1"));
        // An empty logbook has no bests, and says so with nothing.
        let empty = logbook(Vec::new(), true).logbook_statistics().unwrap();
        assert_eq!(empty["total"]["flights"], 0);
        assert!(empty["bests"]["longest"].is_null());
    }

    #[test]
    fn flights_are_listed_by_year_with_who_can_see_them() {
        let book = logbook(
            vec![
                flown_on("050726", Visibility::Public),
                flown_on("040726", Visibility::Private),
                flown_on("040725", Visibility::Unlisted),
            ],
            true,
        );
        let all = venti_tools::run(&book, "list_flights", &json!({})).unwrap();
        let all: Value = serde_json::from_str(&all).unwrap();
        assert_eq!(all["flights_matching"], 3);
        assert_eq!(all["flights"][0]["shared"], "public: everyone");
        assert_eq!(all["flights"][1]["shared"], "private: only the pilot");
        assert_eq!(all["flights"][0]["takeoff"].as_str().unwrap().len(), 20);

        let one = venti_tools::run(&book, "list_flights", &json!({"year": 2026, "limit": 1}));
        let one: Value = serde_json::from_str(&one.unwrap()).unwrap();
        assert_eq!(one["flights_matching"], 2);
        assert_eq!(one["flights_listed"], 1);
    }

    #[test]
    fn one_flight_is_shown_with_what_the_server_keeps_and_what_it_does_not() {
        let flight = flown_on("040726", Visibility::Private);
        let id = flight.id.clone();
        let book = logbook(vec![flight], true);
        let shown = venti_tools::run(&book, "get_flight", &json!({"flight_id": id})).unwrap();
        let shown: Value = serde_json::from_str(&shown).unwrap();
        assert_eq!(shown["flight_id"], id);
        assert_eq!(shown["shared"], "private: only the pilot");
        assert!(shown["took_off_at"]["latitude"].is_number());
        assert!(shown["not_included"].as_str().unwrap().contains("Thermals"));

        let missing = venti_tools::run(&book, "get_flight", &json!({"flight_id": "nothing"}));
        assert!(matches!(missing, Err(ToolError::Failed(_))));
    }
}
