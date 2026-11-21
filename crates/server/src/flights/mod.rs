//! Pilots' flights on the server.
//!
//! - [`derive`] works out everything the server says about a flight from
//!   its file; [`analyser`] runs that where a hostile file can do no harm.
//! - [`share`] is the page a link to a flight shows, [`picture`] the image
//!   that goes with it.
//! - [`refresh`] works stored flights out again after the analysis changed.
//! - [`track`] is a flight's recording as a GeoJSON line, for programs
//!   that do not read flight files.
//!
//! A flight file is kept byte for byte as it was uploaded: a recorder's
//! signature covers every byte, and a league will only accept the file it
//! signed. It is kept compressed, which for a flight file saves about two
//! thirds, and is unpacked when someone asks for it.

pub mod analyser;
pub mod derive;
pub mod flown;
pub mod picture;
pub mod refresh;
pub mod share;
pub mod track;

use std::io::{Read, Write};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use venti_api_types::v1::MAX_FLIGHT_FILE_BYTES;
use venti_domain::Timestamp;
use venti_store::NewFlight;

use crate::accounts::secret;
use derive::Derivation;

/// Compresses a flight file for storing.
pub fn pack(file: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::with_capacity(file.len() / 4), Compression::default());
    // Writing into a vector cannot fail.
    let _ = encoder.write_all(file);
    encoder.finish().unwrap_or_default()
}

/// Gives a stored flight file back exactly as it was uploaded.
///
/// `None` if the stored bytes are not a compressed file, or unpack to more
/// than the server ever accepted: neither can be something [`pack`] made.
pub fn unpack(packed: &[u8]) -> Option<Vec<u8>> {
    let limit = MAX_FLIGHT_FILE_BYTES as u64;
    let mut file = Vec::new();
    GzDecoder::new(packed)
        .take(limit + 1)
        .read_to_end(&mut file)
        .ok()?;
    (file.len() as u64 <= limit).then_some(file)
}

/// The record of a file an account uploaded, ready to store.
///
/// Compressing takes a few milliseconds for a long flight; call this off
/// the threads that answer requests.
pub fn new_flight(
    account_id: &str,
    file: &[u8],
    derivation: Derivation,
    now: Timestamp,
) -> NewFlight {
    NewFlight {
        id: secret::flight_id(account_id, &derivation.identity),
        account_id: account_id.to_owned(),
        identity: derivation.identity,
        uploaded_at: now,
        file: pack(file),
        file_bytes: u32::try_from(file.len()).unwrap_or(u32::MAX),
        derived: derivation.derived,
    }
}

#[cfg(test)]
mod tests {
    use venti_flight::igc;
    use venti_flight::testing::synthetic_flight;

    use super::*;

    #[test]
    fn a_file_comes_back_byte_for_byte() {
        let sample = igc::write(&synthetic_flight()).into_bytes();
        let packed = pack(&sample);
        assert!(packed.len() * 3 < sample.len(), "{} bytes", packed.len());
        assert_eq!(unpack(&packed), Some(sample));

        // Bytes that are not text, and no bytes at all.
        let binary: Vec<u8> = (0..=255).cycle().take(5000).collect();
        assert_eq!(unpack(&pack(&binary)), Some(binary));
        assert_eq!(unpack(&pack(&[])), Some(Vec::new()));
    }

    #[test]
    fn what_was_never_packed_does_not_unpack() {
        assert_eq!(unpack(b"not gzip"), None);
        assert_eq!(unpack(&[]), None);
        let packed = pack(b"a flight file");
        assert_eq!(unpack(&packed[..packed.len() - 4]), None);
    }

    #[test]
    fn a_stored_file_never_unpacks_to_more_than_an_upload_may_hold() {
        // A megabyte of zeros packs into a kilobyte. Thirty megabytes of
        // them could only come from a damaged database.
        let at_limit = vec![0_u8; MAX_FLIGHT_FILE_BYTES];
        assert_eq!(
            unpack(&pack(&at_limit)).map(|file| file.len()),
            Some(MAX_FLIGHT_FILE_BYTES)
        );
        let beyond = vec![0_u8; MAX_FLIGHT_FILE_BYTES + 1];
        assert_eq!(unpack(&pack(&beyond)), None);
    }

    #[test]
    fn a_record_carries_the_flights_address_identity_and_packed_file() {
        let sample = igc::write(&synthetic_flight()).into_bytes();
        let derivation = derive::derive(&sample).unwrap();
        let now = chrono::DateTime::from_timestamp(1_795_000_000, 0).unwrap();

        let record = new_flight(
            "0123456789abcdefghjkmnpqrs",
            &sample,
            derivation.clone(),
            now,
        );

        assert_eq!(record.identity, derivation.identity);
        assert_eq!(
            record.id,
            secret::flight_id("0123456789abcdefghjkmnpqrs", &derivation.identity)
        );
        assert_eq!(record.file_bytes as usize, sample.len());
        assert_eq!(unpack(&record.file), Some(sample));
        assert_eq!(record.uploaded_at, now);
    }
}
