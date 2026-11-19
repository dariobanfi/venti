//! External flight instruments.
//!
//! A vario, a flight computer or a traffic receiver talks to the phone as a
//! stream of short text lines ("sentences") over a Bluetooth serial link.
//! This crate turns that stream into [`Reading`]s. It does no I/O and reads
//! no clock: the apps hand it the bytes as they arrive, which makes every
//! parser testable from a recorded stream.
//!
//! - [`SentenceReader`] cuts the byte stream into lines and checks their
//!   checksums. Bluetooth delivers bytes in arbitrary pieces, so a line can
//!   arrive in several parts or several lines in one.
//! - [`parse`] reads one line. Each sentence type has one small parser.
//! - [`ble`] lists the Bluetooth services that carry such a stream, so every
//!   app looks for the same devices.
//!
//! Sentences understood so far: `LK8EX1`, `POV` (OpenVario), `XCTRC`
//! (XC Tracer), `LXWP0` and BlueFly's `PRS`. They have been tested against
//! the examples in their specifications. Streams recorded from real devices
//! are still to be collected as fixtures.

pub mod ble;
mod parsers;
mod reader;
mod reading;

pub use parsers::parse;
pub use reader::{SentenceError, SentenceReader};
pub use reading::{Battery, DeviceFix, Reading};
