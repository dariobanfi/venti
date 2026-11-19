//! What makes two recordings the same flight.

use sha2::{Digest, Sha256};

use crate::igc::Fix;

/// A flight's identity: 32 hexadecimal characters from a hash of its fixes.
///
/// The hash covers the time, position and both altitudes of every fix, in
/// the order they were recorded, and nothing else in the file. The same
/// recording imported twice, or downloaded from two places with different
/// headers, is therefore one flight; any change to a fix makes another.
///
/// The phone, the browser and the server all call this function, which is
/// what lets a flight imported on two devices land on one record on the
/// server. The value must never change for a given file: logbooks on
/// phones and the server's records are keyed by it. That includes how
/// [`crate::igc::parse`] turns a `B` line into a fix, because positions go
/// into the hash as the numbers the parser produced.
pub fn identity(fixes: &[Fix]) -> String {
    let mut hash = Sha256::new();
    for fix in fixes {
        hash.update(fix.time.timestamp().to_le_bytes());
        hash.update(fix.position.lat().to_bits().to_le_bytes());
        hash.update(fix.position.lon().to_bits().to_le_bytes());
        hash.update(fix.gps_altitude_m.to_le_bytes());
        hash.update(fix.pressure_altitude_m.to_le_bytes());
    }
    hash.finalize()
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::igc;
    use crate::testing::synthetic_flight;

    fn sample() -> String {
        igc::write(&synthetic_flight())
    }

    fn identity_of(text: &str) -> String {
        identity(&igc::parse(text).unwrap().fixes)
    }

    /// Logbooks on phones and records on the server are keyed by this value.
    /// If this test fails, the change that broke it would orphan every one
    /// of them: change the code back, not the expected value.
    ///
    /// The file read here is the sample bundled with the iOS app, so the
    /// test depends on the reader alone and not on how the sample is made.
    #[test]
    fn the_identity_of_the_sample_flight_never_changes() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../apps/ios/Venti/Resources/sample.igc"
        );
        let bundled = std::fs::read_to_string(path).expect("the bundled sample exists");
        assert_eq!(identity_of(&bundled), "6c55247b80a34b03fa0bb3bd38173c59");
    }

    #[test]
    fn headers_do_not_change_a_flights_identity() {
        let renamed = sample().replace("Sample Pilot", "Somebody Else");
        assert_ne!(renamed, sample());
        assert_eq!(identity_of(&renamed), identity_of(&sample()));
        // Neither do line endings or lines the reader skips.
        let unix = sample().replace("\r\n", "\n") + "LXVEa comment\nGSIGNATURE\n";
        assert_eq!(identity_of(&unix), identity_of(&sample()));
    }

    #[test]
    fn any_change_to_a_fix_makes_another_flight() {
        let original = synthetic_flight();
        let base = identity(&original.fixes);

        let mut later = original.clone();
        later.fixes[100].time += chrono::Duration::seconds(1);
        assert_ne!(identity(&later.fixes), base);

        let mut higher = original.clone();
        higher.fixes[100].gps_altitude_m += 1;
        assert_ne!(identity(&higher.fixes), base);

        let mut pressure = original.clone();
        pressure.fixes[100].pressure_altitude_m += 1;
        assert_ne!(identity(&pressure.fixes), base);

        let mut shorter = original.clone();
        shorter.fixes.pop();
        assert_ne!(identity(&shorter.fixes), base);

        // Another day is another flight.
        assert_ne!(identity_of(&sample().replace("040726", "040725")), base);
    }

    #[test]
    fn an_identity_is_32_lowercase_hexadecimal_characters() {
        for fixes in [&synthetic_flight().fixes[..], &[]] {
            let id = identity(fixes);
            assert_eq!(id.len(), 32);
            assert!(
                id.bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
                "{id}"
            );
        }
    }
}
