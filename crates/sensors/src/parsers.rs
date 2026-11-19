//! One small parser per sentence type.
//!
//! Where each format is defined:
//!
//! - `LK8EX1`: the LK8000 project's own description, `Docs/LK8EX1.txt` in
//!   its repository, read 2026-11-19.
//! - `POV`: the OpenVario protocol, as listed in XCSoar's OpenVario driver.
//! - `XCTRC` and its `LXWP0` subset: XC Tracer's protocol, as quoted in
//!   XCSoar's XC Tracer driver. To verify against XC Tracer's own page: that
//!   speed is in metres per second.
//! - `LXWP0`: LXNAV's sentence, as listed in XCSoar's LX driver.
//! - `PRS`: the BlueFly vario's pressure line, as in XCSoar's BlueFly driver.

use chrono::NaiveDate;
use venti_domain::GeoPoint;

use crate::reading::{Battery, DeviceFix, Reading};

/// Reads one sentence, as returned by a
/// [`SentenceReader`](crate::SentenceReader). Returns `None` for a sentence
/// of a type this crate does not know, or one that says nothing usable.
pub fn parse(line: &str) -> Option<Reading> {
    // A checksum that was not stripped is not part of the last field.
    let line = line.split('*').next()?.trim();

    let reading = if let Some(body) = line.strip_prefix('$') {
        let (kind, fields) = body.split_once(',')?;
        let fields: Vec<&str> = fields.split(',').map(str::trim).collect();
        match kind {
            "LK8EX1" => lk8ex1(&fields),
            "POV" => pov(&fields),
            "XCTRC" => xctrc(&fields),
            "LXWP0" => lxwp0(&fields),
            _ => return None,
        }
    } else {
        let hex = line.strip_prefix("PRS ")?;
        Reading {
            pressure_hpa: u32::from_str_radix(hex.trim(), 16)
                .ok()
                .map(|pascal| f64::from(pascal) / 100.0)
                .and_then(pressure),
            ..Reading::default()
        }
    };

    (!reading.is_empty()).then_some(reading)
}

/// The number in a field, if there is one.
fn number(fields: &[&str], index: usize) -> Option<f64> {
    fields
        .get(index)?
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

/// A pressure, if it is one the atmosphere can have where people fly.
fn pressure(hpa: f64) -> Option<f64> {
    (100.0..=1100.0).contains(&hpa).then_some(hpa)
}

/// A climb rate, if it is one a glider can have.
fn vario(ms: f64) -> Option<f64> {
    (ms.abs() <= 50.0).then_some(ms)
}

/// `$LK8EX1,pressure,altitude,vario,temperature,battery,`
///
/// Each field has its own "not available" number: 999999, 99999, 9999, 99
/// and 999.
fn lk8ex1(fields: &[&str]) -> Reading {
    let available = |index: usize, missing: f64| number(fields, index).filter(|&v| v != missing);

    let pressure_hpa = available(0, 999_999.0)
        .map(|v| v / 100.0)
        .and_then(pressure);
    Reading {
        pressure_hpa,
        // The specification says altitude is ignored when pressure is sent.
        pressure_altitude_m: pressure_hpa
            .is_none()
            .then(|| available(1, 99_999.0))
            .flatten(),
        vario_ms: available(2, 9_999.0).map(|v| v / 100.0).and_then(vario),
        temperature_c: available(3, 99.0),
        // Percentages are sent with 1000 added; anything below is a voltage.
        battery: available(4, 999.0).map(|v| {
            if v >= 1000.0 {
                Battery::Percent((v - 1000.0).clamp(0.0, 100.0) as u8)
            } else {
                Battery::Volts(v)
            }
        }),
        ..Reading::default()
    }
}

/// `$POV,P,1013.25,E,1.20,T,23.1`: pairs of a letter and a value.
fn pov(fields: &[&str]) -> Reading {
    let mut reading = Reading::default();
    let mut uncompensated = None;
    for pair in fields.chunks(2) {
        let (Some(&key), Some(value)) = (pair.first(), number(pair, 1)) else {
            continue;
        };
        match key {
            "P" => reading.pressure_hpa = pressure(value),
            "E" => reading.vario_ms = vario(value),
            "e" => uncompensated = vario(value),
            "T" => reading.temperature_c = Some(value),
            "S" => reading.airspeed_ms = Some(value / 3.6),
            "V" => reading.battery = Some(Battery::Volts(value)),
            _ => {}
        }
    }
    // A paraglider has no use for the difference between the two varios:
    // take the compensated one if it is there, otherwise the plain one.
    reading.vario_ms = reading.vario_ms.or(uncompensated);
    reading
}

/// `$XCTRC,year,month,day,hour,minute,second,centisecond,latitude,longitude,
/// altitude,speed,course,climb,,,,pressure,battery`
fn xctrc(fields: &[&str]) -> Reading {
    let whole = |index: usize| number(fields, index).map(|v| v as u32);
    let time = (|| {
        NaiveDate::from_ymd_opt(whole(0)? as i32, whole(1)?, whole(2)?)?.and_hms_milli_opt(
            whole(3)?,
            whole(4)?,
            whole(5)?,
            whole(6)? * 10,
        )
    })();
    let position = number(fields, 7)
        .zip(number(fields, 8))
        .and_then(|(lat, lon)| GeoPoint::new(lat, lon).ok());

    Reading {
        fix: time.zip(position).map(|(time, position)| DeviceFix {
            time: time.and_utc(),
            position,
            altitude_m: number(fields, 9),
            ground_speed_ms: number(fields, 10),
            course_deg: number(fields, 11),
        }),
        vario_ms: number(fields, 12).and_then(vario),
        pressure_hpa: number(fields, 16).and_then(pressure),
        battery: number(fields, 17)
            .filter(|percent| (0.0..=100.0).contains(percent))
            .map(|percent| Battery::Percent(percent as u8)),
        ..Reading::default()
    }
}

/// `$LXWP0,logger,airspeed km/h,altitude,vario × 6,heading,wind course,wind speed`
fn lxwp0(fields: &[&str]) -> Reading {
    Reading {
        airspeed_ms: number(fields, 1)
            .filter(|kmh| (0.0..=250.0).contains(kmh))
            .map(|kmh| kmh / 3.6),
        pressure_altitude_m: number(fields, 2),
        // Up to six readings from the last second; the first is the oldest.
        vario_ms: (3..=8)
            .rev()
            .find_map(|index| number(fields, index))
            .and_then(vario),
        heading_deg: number(fields, 9),
        ..Reading::default()
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::SentenceReader;

    /// Reads a line through the reader first, as an app does, so the
    /// examples' checksums are checked too.
    fn read(line: &str) -> Option<Reading> {
        let mut lines = SentenceReader::new().push(format!("{line}\r\n").as_bytes());
        assert_eq!(lines.len(), 1);
        parse(&lines.remove(0).expect("the example's checksum is right"))
    }

    #[test]
    fn lk8ex1_with_pressure_vario_temperature_and_battery_percent() {
        let reading = read("$LK8EX1,98725,99999,-120,21,1085,*0A").unwrap();
        assert_eq!(reading.pressure_hpa, Some(987.25));
        assert_eq!(reading.pressure_altitude_m, None);
        assert_eq!(reading.vario_ms, Some(-1.2));
        assert_eq!(reading.temperature_c, Some(21.0));
        assert_eq!(reading.battery, Some(Battery::Percent(85)));
    }

    #[test]
    fn lk8ex1_with_altitude_instead_of_pressure_and_battery_volts() {
        let reading = read("$LK8EX1,999999,1520,9999,99,3.9,").unwrap();
        assert_eq!(reading.pressure_hpa, None);
        assert_eq!(reading.pressure_altitude_m, Some(1520.0));
        assert_eq!(reading.vario_ms, None);
        assert_eq!(reading.temperature_c, None);
        assert_eq!(reading.battery, Some(Battery::Volts(3.9)));
    }

    #[test]
    fn lk8ex1_with_nothing_available_is_no_reading() {
        assert_eq!(read("$LK8EX1,999999,99999,9999,99,999,"), None);
    }

    #[test]
    fn openvario_pairs() {
        let reading = read("$POV,P,945.20,E,2.15,T,18.5,S,36").unwrap();
        assert_eq!(reading.pressure_hpa, Some(945.2));
        assert_eq!(reading.vario_ms, Some(2.15));
        assert_eq!(reading.temperature_c, Some(18.5));
        assert_eq!(reading.airspeed_ms, Some(10.0));

        // Only the uncompensated vario: it is the vario.
        assert_eq!(read("$POV,e,-0.8").unwrap().vario_ms, Some(-0.8));
        // Both: the compensated one wins, whatever the order.
        assert_eq!(read("$POV,e,-0.8,E,-0.5").unwrap().vario_ms, Some(-0.5));
    }

    #[test]
    fn xc_tracer_native_sentence() {
        // The example with a valid checksum from XCSoar's driver. The one
        // printed first in the specification has a checksum that does not
        // match its text.
        let reading = read(
            "$XCTRC,2015,8,11,10,56,23,80,48.62825,8.104885,129.4,0.01,322.76,-0.05,,,,997.79,77*66",
        )
        .unwrap();

        let fix = reading.fix.unwrap();
        assert_eq!(
            fix.time,
            Utc.with_ymd_and_hms(2015, 8, 11, 10, 56, 23).unwrap()
                + chrono::Duration::milliseconds(800)
        );
        assert_eq!(
            (fix.position.lat(), fix.position.lon()),
            (48.62825, 8.104885)
        );
        assert_eq!(fix.altitude_m, Some(129.4));
        assert_eq!(fix.ground_speed_ms, Some(0.01));
        assert_eq!(fix.course_deg, Some(322.76));
        assert_eq!(reading.vario_ms, Some(-0.05));
        assert_eq!(reading.pressure_hpa, Some(997.79));
        assert_eq!(reading.battery, Some(Battery::Percent(77)));
    }

    #[test]
    fn xc_tracer_sentence_without_a_fix_still_gives_the_vario() {
        let reading = read("$XCTRC,,,,,,,,,,,,,-0.05,,,,997.79,77").unwrap();
        assert_eq!(reading.fix, None);
        assert_eq!(reading.vario_ms, Some(-0.05));
        assert_eq!(reading.pressure_hpa, Some(997.79));
    }

    #[test]
    fn lxwp0_as_an_xc_tracer_sends_it() {
        let reading = read("$LXWP0,N,,119.9,0.16,,,,,,259,,*64").unwrap();
        assert_eq!(reading.pressure_altitude_m, Some(119.9));
        assert_eq!(reading.vario_ms, Some(0.16));
        assert_eq!(reading.heading_deg, Some(259.0));
        assert_eq!(reading.airspeed_ms, None);
    }

    #[test]
    fn lxwp0_with_six_varios_takes_the_latest() {
        let reading = read("$LXWP0,Y,36.0,1665.5,1.1,1.2,1.3,1.4,1.5,1.71,239,174,10.1").unwrap();
        assert_eq!(reading.vario_ms, Some(1.71));
        assert_eq!(reading.airspeed_ms, Some(10.0));
    }

    #[test]
    fn bluefly_pressure_line() {
        // 0x17CBA pascal.
        assert_eq!(read("PRS 17CBA").unwrap().pressure_hpa, Some(974.66));
        assert_eq!(read("PRS zz"), None);
    }

    #[test]
    fn values_no_glider_or_atmosphere_can_have_are_dropped() {
        assert_eq!(read("$POV,P,20.0,E,400"), None);
        assert_eq!(read("$POV,P,NaN"), None);
    }

    #[test]
    fn sentences_of_unknown_types_are_ignored() {
        assert_eq!(read("$GPGSV,3,1,11,03,03,111,00"), None);
        assert_eq!(read("BAT 1234"), None);
        assert_eq!(read("hello"), None);
    }
}
