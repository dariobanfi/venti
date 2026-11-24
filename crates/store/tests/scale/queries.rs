//! Every question the API asks of everyone's flights, as the store asks it:
//! for measuring at the size of a year (`tests/scale.rs`) and for holding
//! the plans to the indexes (`flight_searches_use_the_indexes` in
//! `tests/postgres.rs`). The values are those of `seed.sql`.

// Each of the two tests uses a part.
#![allow(dead_code)]

use chrono::{TimeZone, Utc};
use venti_domain::{Area, BoundingBox, GeoPoint, Timestamp};
use venti_store::{GliderClass, Near, PgStore, PublicFlights, PublicOrder, ScoreKind, StoreError};

/// A question: what it is called, and what the store is asked.
pub enum Asked {
    /// A page of the list.
    List(PublicFlights, PublicOrder),
    /// The ranking, with the flights that count of each pilot.
    Ranking(PublicFlights),
}

fn moment(year: i32, month: u32, day: u32) -> Timestamp {
    Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).unwrap()
}

/// The questions, around `centre`, of the pilot `pilot`.
pub fn questions(centre: GeoPoint, pilot: &str) -> Vec<(&'static str, Asked)> {
    let all = PublicFlights::default;
    let near = |km: f64| Near {
        place: centre,
        within_m: km * 1000.0,
    };
    let (lat, lon) = (centre.lat(), centre.lon());
    // A pentagon of some 30 km around the centre.
    let drawn = Area::new(
        [
            (0.15, 0.0),
            (0.05, 0.2),
            (-0.15, 0.15),
            (-0.15, -0.15),
            (0.05, -0.2),
        ]
        .into_iter()
        .map(|(dlat, dlon)| GeoPoint::new(lat + dlat, lon + dlon).unwrap())
        .collect(),
    )
    .unwrap();
    let innsbruck = BoundingBox::new(47.0, 11.0, 47.5, 11.8).unwrap();
    let alps = BoundingBox::new(45.5, 5.5, 48.0, 15.0).unwrap();
    let july = || PublicFlights {
        from: Some(moment(2026, 7, 1)),
        before: Some(moment(2026, 8, 1)),
        ..all()
    };
    let year = || PublicFlights {
        from: Some(moment(2025, 11, 23)),
        before: Some(moment(2026, 11, 23)),
        ..all()
    };
    let deep_newest = PublicOrder::Newest(Some((moment(2026, 3, 15), "8".repeat(26))));
    let deep_best = PublicOrder::Best(Some((20.0, "8".repeat(26))));
    let newest = || PublicOrder::Newest(None);
    let best = || PublicOrder::Best(None);
    let of_pilot = || PublicFlights {
        pilot: Some(pilot.to_owned()),
        ..all()
    };
    let list = Asked::List;
    vec![
        ("newest, first page", list(all(), newest())),
        ("best, first page", list(all(), best())),
        ("newest, deep page", list(all(), deep_newest.clone())),
        ("best, deep page", list(all(), deep_best.clone())),
        ("July, newest", list(july(), newest())),
        ("July, best", list(july(), best())),
        ("pilot, newest", list(of_pilot(), newest())),
        ("pilot, best", list(of_pilot(), best())),
        ("pilot, newest, deep page", list(of_pilot(), deep_newest)),
        ("pilot, best, deep page", list(of_pilot(), deep_best)),
        (
            "pilot in July, best",
            list(
                PublicFlights {
                    pilot: Some(pilot.to_owned()),
                    ..july()
                },
                best(),
            ),
        ),
        (
            "box Innsbruck, newest",
            list(
                PublicFlights {
                    area: Some(innsbruck),
                    ..all()
                },
                newest(),
            ),
        ),
        (
            "box Innsbruck, best",
            list(
                PublicFlights {
                    area: Some(innsbruck),
                    ..all()
                },
                best(),
            ),
        ),
        (
            "box Alps, best",
            list(
                PublicFlights {
                    area: Some(alps),
                    ..all()
                },
                best(),
            ),
        ),
        (
            "radius 2 km, newest",
            list(
                PublicFlights {
                    near: Some(near(2.0)),
                    ..all()
                },
                newest(),
            ),
        ),
        (
            "radius 10 km, newest",
            list(
                PublicFlights {
                    near: Some(near(10.0)),
                    ..all()
                },
                newest(),
            ),
        ),
        (
            "radius 10 km, best",
            list(
                PublicFlights {
                    near: Some(near(10.0)),
                    ..all()
                },
                best(),
            ),
        ),
        (
            "radius 100 km, best",
            list(
                PublicFlights {
                    near: Some(near(100.0)),
                    ..all()
                },
                best(),
            ),
        ),
        (
            "area drawn, newest",
            list(
                PublicFlights {
                    drawn: Some(drawn.clone()),
                    ..all()
                },
                newest(),
            ),
        ),
        (
            "area drawn, best",
            list(
                PublicFlights {
                    drawn: Some(drawn.clone()),
                    ..all()
                },
                best(),
            ),
        ),
        (
            "class CCC, newest",
            list(
                PublicFlights {
                    class: Some(GliderClass::Ccc),
                    ..all()
                },
                newest(),
            ),
        ),
        (
            "class CCC, best",
            list(
                PublicFlights {
                    class: Some(GliderClass::Ccc),
                    ..all()
                },
                best(),
            ),
        ),
        (
            "FAI triangles, newest",
            list(
                PublicFlights {
                    kind: Some(ScoreKind::FaiTriangle),
                    ..all()
                },
                newest(),
            ),
        ),
        (
            "FAI triangles, best",
            list(
                PublicFlights {
                    kind: Some(ScoreKind::FaiTriangle),
                    ..all()
                },
                best(),
            ),
        ),
        (
            "at least 100 km, newest",
            list(
                PublicFlights {
                    min_distance_m: Some(100_000.0),
                    ..all()
                },
                newest(),
            ),
        ),
        (
            "at least 100 km, best",
            list(
                PublicFlights {
                    min_distance_m: Some(100_000.0),
                    ..all()
                },
                best(),
            ),
        ),
        (
            "at least 300 km, newest",
            list(
                PublicFlights {
                    min_distance_m: Some(300_000.0),
                    ..all()
                },
                newest(),
            ),
        ),
        (
            "country SI, newest",
            list(
                PublicFlights {
                    country: Some("SI".to_owned()),
                    ..all()
                },
                newest(),
            ),
        ),
        (
            "country SI, best",
            list(
                PublicFlights {
                    country: Some("SI".to_owned()),
                    ..all()
                },
                best(),
            ),
        ),
        (
            "country NO, newest",
            list(
                PublicFlights {
                    country: Some("NO".to_owned()),
                    ..all()
                },
                newest(),
            ),
        ),
        (
            "radius 30 km + EN B, best",
            list(
                PublicFlights {
                    near: Some(near(30.0)),
                    class: Some(GliderClass::EnB),
                    ..all()
                },
                best(),
            ),
        ),
        (
            "country AT + FAI + 50 km, best",
            list(
                PublicFlights {
                    country: Some("AT".to_owned()),
                    kind: Some(ScoreKind::FaiTriangle),
                    min_distance_m: Some(50_000.0),
                    ..all()
                },
                best(),
            ),
        ),
        (
            "July + area + CCC, best",
            list(
                PublicFlights {
                    drawn: Some(drawn),
                    class: Some(GliderClass::Ccc),
                    ..july()
                },
                best(),
            ),
        ),
        (
            "July + country NO, newest",
            list(
                PublicFlights {
                    country: Some("NO".to_owned()),
                    ..july()
                },
                newest(),
            ),
        ),
        ("ranking, July", Asked::Ranking(july())),
        ("ranking, a year", Asked::Ranking(year())),
        (
            "ranking, a year in SI",
            Asked::Ranking(PublicFlights {
                country: Some("SI".to_owned()),
                ..year()
            }),
        ),
        (
            "ranking, July, CCC",
            Asked::Ranking(PublicFlights {
                class: Some(GliderClass::Ccc),
                ..july()
            }),
        ),
        (
            "ranking, July, radius 30 km",
            Asked::Ranking(PublicFlights {
                near: Some(near(30.0)),
                ..july()
            }),
        ),
    ]
}

/// What the database did for one question: the plan as text, or the
/// error of a question the database gave up after its time.
pub async fn explain(store: &PgStore, asked: &Asked) -> Result<String, StoreError> {
    match asked {
        // A page of 50 and one to know whether more follow, as the API asks.
        Asked::List(which, order) => store.explain_public_flights(which, order, 51).await,
        // The six best of each of the first 50 pilots.
        Asked::Ranking(which) => store.explain_ranking(which, 6, 50).await,
    }
}

/// The time the plan says the query took, milliseconds.
pub fn execution_ms(plan: &str) -> f64 {
    plan.lines()
        .find_map(|line| line.trim().strip_prefix("Execution Time: "))
        .and_then(|rest| rest.trim_end_matches(" ms").parse().ok())
        .expect("a plan of EXPLAIN ANALYZE says its execution time")
}

/// Whether the plan reads the whole table of flights.
pub fn reads_all_flights(plan: &str) -> bool {
    plan.contains("Seq Scan on flights")
}

/// The centre the questions are asked around: where the first made-up
/// flight took off, at one of the busiest sites.
pub async fn centre(url: &str) -> GeoPoint {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls)
        .await
        .unwrap();
    tokio::spawn(connection);
    let row = client
        .query_one(
            "SELECT takeoff_lat, takeoff_lon FROM flights WHERE id = substr(md5('seed flight 1'), 1, 26)",
            &[],
        )
        .await
        .unwrap();
    GeoPoint::new(row.get(0), row.get(1)).unwrap()
}
