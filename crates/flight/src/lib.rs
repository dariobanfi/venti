//! The flight engine.
//!
//! The same code runs in the phone's flight recorder, in the browser and on
//! the server, so everything here is pure: no I/O and no clock. Callers pass
//! in text, positions and times, which makes every calculation replayable
//! from a recorded flight.
//!
//! So far it reads and writes IGC flight files, summarises and analyses a
//! track, prepares it for drawing, and runs the live flight computer.

pub mod analysis;
pub mod igc;
mod summary;
pub mod testing;
pub mod track;

pub use summary::{FlightSummary, summarise};
pub mod live;
