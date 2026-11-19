use venti_domain::{GeoPoint, Timestamp};

/// What one sentence from an instrument says. A sentence carries only some
/// of these, so every field is optional.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reading {
    /// Static air pressure in hectopascals.
    pub pressure_hpa: Option<f64>,
    /// Altitude on the standard atmosphere (1013.25 hPa at sea level), in
    /// metres. Instruments send this instead of pressure, or as well.
    pub pressure_altitude_m: Option<f64>,
    /// Climb rate in metres per second as the instrument computed it.
    /// Negative is sink.
    pub vario_ms: Option<f64>,
    /// Air temperature in degrees Celsius.
    pub temperature_c: Option<f64>,
    /// State of the instrument's battery.
    pub battery: Option<Battery>,
    /// Airspeed in metres per second.
    pub airspeed_ms: Option<f64>,
    /// Compass heading or course in degrees, as the instrument reports it.
    pub heading_deg: Option<f64>,
    /// The instrument's own satellite fix.
    pub fix: Option<DeviceFix>,
}

impl Reading {
    /// Whether the sentence said nothing this crate understands.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// How an instrument reports its battery.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Battery {
    /// Charge in percent, 0 to 100.
    Percent(u8),
    /// Voltage in volts.
    Volts(f64),
}

/// A satellite fix from an instrument with its own receiver.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceFix {
    /// When the fix was taken, by the satellites' clock.
    pub time: Timestamp,
    /// Where.
    pub position: GeoPoint,
    /// Altitude in metres.
    pub altitude_m: Option<f64>,
    /// Speed over the ground in metres per second.
    pub ground_speed_ms: Option<f64>,
    /// Direction of travel over the ground in degrees.
    pub course_deg: Option<f64>,
}
