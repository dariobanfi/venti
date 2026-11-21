//! The flight engine.
//!
//! The same code runs in the phone's flight recorder, in the browser and on
//! the server, so everything here is pure: no I/O and no clock. Callers pass
//! in text, positions and times, which makes every calculation replayable
//! from a recorded flight.
//!
//! So far it reads and writes IGC flight files, says what makes two of them
//! the same flight, summarises and analyses a track, prepares it for
//! drawing, counts how it was flown, runs the live flight computer, reads,
//! measures and follows competition tasks, works out the thermals of an
//! hour of a weather model, reads its wind at a height between the model's
//! levels, says what a forecast's hours make of a day for flying, finds the
//! strongest wind in the model's cells around a place, and marks where the
//! temperatures at the model's levels stop thermals, says what a glide
//! reaches over the ground and how steep the ground is.

pub mod aloft;
pub mod analysis;
pub mod cables;
pub mod convection;
pub mod day;
pub mod fai;
pub mod geodesy;
pub mod habits;
mod identity;
pub mod igc;
pub mod layering;
pub mod nearby;
pub mod route_plan;
pub mod slope;
mod summary;
pub mod testing;
pub mod track;
pub mod waypoints;

pub use identity::identity;
pub use summary::{FlightSummary, summarise};
pub mod live;
pub mod scoring;
pub mod sun;
pub mod task;
pub mod terrain;
