//! The Venti server.
//!
//! One process does four things:
//!
//! 1. **Collects.** Every source of the configured regions runs through the
//!    pipeline of `venti-sources` at the cadence its manifest states
//!    ([`ingest`]). Raw payloads go to the archive, validated records and
//!    the outcome of each run go to the store.
//! 2. **Keeps pilots' accounts and flights.** An account is signed in to
//!    with a handle and a password ([`accounts`]). A pilot's flight files
//!    are kept as they were uploaded, and what the apps show about a flight
//!    is worked out from its file with the flight engine ([`flights`]).
//!    While a flight is under way, the positions its pilot's app sends are
//!    kept as a live track for those the pilot lets see it, and removed a
//!    short while after ([`live`]). Pilots make groups, which only their
//!    members see, and plan flying days in them ([`groups`]).
//! 3. **Serves.** A small versioned HTTP API reads and writes the store
//!    ([`api`]). Its wire format is the `venti-api-types` crate, which the
//!    clients share. Beside it, a flight that its pilot shares has a page
//!    of its own for people and for link previews. The server also hands
//!    out the map regions that whoever runs it put in a folder, for apps
//!    to draw without a network ([`maps`]).
//! 4. **Cleans up.** Data older than the configured retention is removed
//!    ([`maintenance`]).
//!
//! The program's subcommands ([`cli`]): `serve` is the above against
//! PostgreSQL ([`serve`]); `dev` serves the same API over recorded payloads
//! and a demo account from an in-memory store, with no network and no
//! database ([`dev`]); `ingest` runs a region's sources once and prints
//! what they deliver ([`ingest::oneshot`]); `invite` makes invite codes
//! ([`accounts::invite`]); and a handful are for whoever runs the server
//! to read what pilots reported and to take a flight down, remove a live
//! track or suspend an account's publishing ([`moderation`]).
//!
//! This crate is where the pieces are assembled, and the only one that names
//! a provider's adapter ([`adapters`]). It holds no rules about weather or
//! flying: those live in the crates it calls.

pub mod accounts;
pub mod adapters;
pub mod api;
pub mod cli;
pub mod clock;
pub mod config;
pub mod dev;
pub mod flights;
pub mod forecast;
pub mod groups;
pub mod ingest;
pub mod live;
pub mod maintenance;
pub mod maps;
pub mod moderation;
pub mod plan;
pub mod serve;
