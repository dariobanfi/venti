//! The flight computer: what the cockpit shows while flying.
//!
//! Like the rest of this crate it is pure. The apps pass in positions and
//! pressures with the time each was taken, and get back the state of the
//! flight. Because nothing here reads a clock or a sensor, a recorded flight
//! replayed through it gives exactly what the pilot saw in the air.

pub mod atmosphere;
mod computer;
pub mod navigation;
pub mod tone;
pub mod vario;
pub mod wind;

pub use computer::{
    Config, FlightComputer, LiveState, Phase, PositionSample, Thermal, ThermalMark, TrailPoint,
    VarioSource,
};
pub use wind::Wind;
