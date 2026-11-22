//! Recorded answers that are compiled into the server: empty for now.
//!
//! The crate's own tests read their fixtures from `tests/fixtures` as
//! files, and nothing of those is compiled in.
//!
//! A sample belongs here only when the network joins the first region
//! (`regions/alps-de-at`), which is the region the server's development
//! mode runs: a network named there without a sample fails that mode at
//! its start, and a test of the server says so. The sample is then a few
//! stations with what they measured at the development mode's clock, 19:05
//! UTC on 2026-11-18, returned by an `evening()` here and handed over in
//! `network()` by `.with_sample(fixtures::evening_at(), fixtures::evening())`;
//! `crates/sources-suedtirol/src/fixtures.rs` is one to copy. Each station's
//! newest reading in the sample has a wind: the development mode shows the
//! newest reading, and a test of the server holds this. More stations
//! in the development mode change what the apps' tests count: say so in
//! the pull request.
