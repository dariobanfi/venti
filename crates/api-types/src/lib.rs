//! The wire format of the Venti server's HTTP API.
//!
//! The server builds its responses from these types and the clients decode
//! them, so the two cannot drift apart. The crate holds plain data: no I/O,
//! no clock, and no dependency other than `venti-domain`, which keeps it
//! usable on the phone, in the browser and on the server alike.
//!
//! # Versions
//!
//! Every path starts with a version, and each version has its own module
//! here; today that is [`v1`]. Within a version the format only grows:
//!
//! - a response may gain fields, so clients ignore fields they do not know;
//! - an optional value that is not known is left out, never sent as `null`,
//!   so clients treat a missing field as "unknown";
//! - an error may carry a code a client has not seen, which decodes as
//!   [`v1::ErrorCode::Unknown`].
//!
//! Removing or renaming a field, or changing what one means, needs a new
//! version beside the old one.
//!
//! The endpoints are described for people in `docs/api.md`.

pub mod v1;
