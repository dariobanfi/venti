//! The Bluetooth LE services that carry an instrument's text stream.
//!
//! None of these is specific to flying: they are the serial-port emulations
//! of the common Bluetooth chips, and instruments built on those chips use
//! them unchanged. An app scans for these services, connects, subscribes to
//! the characteristic that notifies, and feeds what arrives to a
//! [`SentenceReader`](crate::SentenceReader).
//!
//! The identifiers are the ones the chip makers publish; they were taken
//! from XCSoar's device list (`android/src/BluetoothUuids.java`) on
//! 2026-11-19. To verify: which of them the MipFly MipBip uses. An app
//! should therefore also let the pilot pick a device that advertises none
//! of them and listen to whatever it notifies.

/// One serial-style service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SerialService {
    /// What the service is, for diagnostics.
    pub name: &'static str,
    /// The service's identifier, as a full 128-bit UUID in upper case.
    pub service: &'static str,
    /// The characteristic on which the instrument sends its stream.
    pub notify: &'static str,
}

/// The serial-style services to look for.
pub const SERIAL_SERVICES: [SerialService; 3] = [
    SerialService {
        name: "HM-10 serial (XC Tracer and others)",
        service: "0000FFE0-0000-1000-8000-00805F9B34FB",
        notify: "0000FFE1-0000-1000-8000-00805F9B34FB",
    },
    SerialService {
        name: "Nordic UART",
        service: "6E400001-B5A3-F393-E0A9-E50E24DCCA9E",
        notify: "6E400003-B5A3-F393-E0A9-E50E24DCCA9E",
    },
    SerialService {
        name: "Microchip transparent UART",
        service: "49535343-FE7D-4AE5-8FA9-9FAFD205E455",
        notify: "49535343-1E4D-4BD9-BA61-23C647249616",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_identifier_has_the_form_of_a_uuid() {
        for service in SERIAL_SERVICES {
            for id in [service.service, service.notify] {
                let parts: Vec<usize> = id.split('-').map(str::len).collect();
                assert_eq!(parts, [8, 4, 4, 4, 12], "{id}");
                assert!(
                    id.chars()
                        .all(|c| c == '-' || c.is_ascii_digit() || ('A'..='F').contains(&c)),
                    "{id}"
                );
            }
        }
    }
}
