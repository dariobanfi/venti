//! XContest: where a pilot claims a flight.
//!
//! XContest's own pages publish no interface for other apps and no terms
//! for one (looked for on 2026-11-20). Its interface for apps needs a key
//! that XContest gives out on request, and this project has asked for none
//! yet. So nothing here sends anything to XContest: an app hands the pilot
//! the flight's file and opens XContest's page, where the pilot, signed in
//! as themselves, uploads it.

/// The page on which a signed-in pilot claims a flight for the World
/// XContest, which the national contests' pilots are in as well.
///
/// XContest's own instructions name the page by its place in the menu
/// ("My Contest", "Claim flight"); the address answers with "sign in
/// first" to whoever is not, so what it shows was not read.
pub const CLAIM_URL: &str = "https://www.xcontest.org/world/en/flight-claim/";

/// XContest's rules, which say until when a flight is taken (3.1) and what
/// a recording must be like (8).
pub const RULES_URL: &str = "https://www.xcontest.org/world/en/rules/";
