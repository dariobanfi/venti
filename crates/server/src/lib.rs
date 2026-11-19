//! The Venti server.
//!
//! One process does three things:
//!
//! 1. **Collects.** Every source of the configured regions runs through the
//!    pipeline of `venti-sources` at the cadence its manifest states
//!    ([`ingest`]). Raw payloads go to the archive, validated records and
//!    the outcome of each run go to the store.
//! 2. **Serves.** A small versioned HTTP API reads the store ([`api`]). Its
//!    wire format is the `venti-api-types` crate, which the clients share.
//! 3. **Cleans up.** Data older than the configured retention is removed
//!    ([`maintenance`]).
//!
//! The program has three subcommands ([`cli`]): `serve` is the above against
//! PostgreSQL ([`serve`]); `dev` serves the same API over recorded payloads
//! from an in-memory store, with no network and no database ([`dev`]); and
//! `ingest` runs a region's sources once and prints what they deliver
//! ([`ingest::oneshot`]).
//!
//! This crate is where the pieces are assembled, and the only one that names
//! a provider's adapter ([`adapters`]). It holds no rules about weather or
//! flying: those live in the crates it calls.

pub mod adapters;
pub mod api;
pub mod cli;
pub mod clock;
pub mod config;
pub mod dev;
pub mod ingest;
pub mod maintenance;
pub mod plan;
pub mod serve;
