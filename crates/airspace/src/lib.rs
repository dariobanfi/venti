//! Airspace for the cockpit and the map.
//!
//! Airspace is safety-relevant, so everything here is deterministic code
//! that works offline and without an assistant: no I/O, no clock, the same
//! answers on the phone, in the browser and on the server. The guide is
//! `docs/airspace.md`; its example is compiled with this crate's tests.
//!
//! The crate does three things with the canonical
//! [`Airspace`](venti_domain::Airspace) records of `venti-domain`:
//!
//! - [`openair`] reads the OpenAir files pilots already have, and reports
//!   every airspace it could not read.
//! - [`AirspaceIndex`] answers questions about a set of airspaces: what is
//!   at a position and how high it lies in relation to the pilot
//!   ([`AirspaceIndex::at`]), what is near ([`AirspaceIndex::nearby`]),
//!   what a map or a side view needs ([`AirspaceIndex::in_box`],
//!   [`AirspaceIndex::cross_section`]).
//! - [`WarningEngine`] turns a stream of samples from the cockpit into a
//!   small, stable set of warnings, using [`PARAGLIDER_RULES`] to decide
//!   which airspaces concern a paraglider.
//!
//! One rule runs through all of it: **a wrong "you are clear" is the worst
//! thing this code can say.** Where something is unknown (the ground
//! elevation, the air pressure, whether an area is active today) the answer
//! takes the airspace as reaching toward the pilot and says that it is
//! uncertain. Everything is advisory: the records name their source and
//! their age, and a shell must show both.
//!
//! Not here: whether an area is active today (every airspace counts as
//! potentially active), NOTAMs, the terrain below the pilot (pass the
//! ground elevation in), and drawing.

mod geometry;
mod index;
pub mod openair;
mod policy;
mod vertical;
mod warning;

pub use geometry::{
    ARC_MAX_ERROR_M, METRES_PER_NAUTICAL_MILE, bearing_deg, circle_ring, destination,
};
pub use index::{
    AirspaceAt, AirspaceIndex, AltitudeBand, Horizontal, IndexStats, Nearby, SectionSpan,
    horizontal,
};
pub use policy::{PARAGLIDER_RULES, Policy, PolicyTable, Rule};
pub use vertical::{
    Altitudes, HIGHEST_GROUND_M, HIGHEST_QNH_HPA, LOWEST_GROUND_M, LOWEST_QNH_HPA,
    PressureAltitude, ResolvedLimit, STANDARD_PRESSURE_HPA, TEMPERATURE_SPREAD, Unknown,
    VerticalRelation, VerticalStatus, flight_level_range_m, resolve, standard_altitude_m,
    vertical_status,
};
pub use warning::{
    Config, Entry, EntryVia, Level, Margin, Sample, SampleError, Silence, Silenced, Warning,
    WarningEngine,
};

/// The guide, `docs/airspace.md`. It is included only when the
/// documentation tests are built, so that the example in it is compiled and
/// run with this crate's tests and cannot fall behind the code.
#[cfg(doctest)]
#[doc = include_str!("../../../docs/airspace.md")]
pub struct Guide;
