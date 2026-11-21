//! Helpers for testing adapters from recorded payloads.
//!
//! Every adapter crate tests itself the same way: recorded responses go into a
//! [`FixtureFetch`], the real pipeline runs over them, and the result is
//! compared with a golden file checked into the repository.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use serde::Serialize;
use venti_domain::{RADAR_UNSEEN, RadarFrame, Timestamp};

use crate::{Archive, ArchiveEntry, ArchiveError, Fetch, FetchError, Fetched, Request};

pub use futures::executor::block_on;

/// Answers requests from recorded payloads. Unknown URLs get a 404.
#[derive(Debug, Default)]
pub struct FixtureFetch {
    responses: HashMap<String, Fetched>,
}

impl FixtureFetch {
    /// A fetcher with no recorded responses.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the response for a URL.
    pub fn with(mut self, url: impl Into<String>, body: impl Into<Vec<u8>>) -> Self {
        self.responses
            .insert(url.into(), Fetched::from(body.into()));
        self
    }

    /// Records the response for a URL together with the date the provider
    /// put on it.
    pub fn dated(
        mut self,
        url: impl Into<String>,
        body: impl Into<Vec<u8>>,
        last_modified: Timestamp,
    ) -> Self {
        let response = Fetched {
            body: body.into(),
            last_modified: Some(last_modified),
        };
        self.responses.insert(url.into(), response);
        self
    }

    /// Records a response for each URL in a list, such as an adapter crate's
    /// `fixtures::recorded()`.
    pub fn with_all<U, B>(self, responses: impl IntoIterator<Item = (U, B)>) -> Self
    where
        U: Into<String>,
        B: Into<Vec<u8>>,
    {
        responses
            .into_iter()
            .fold(self, |fetch, (url, body)| fetch.with(url, body))
    }
}

impl Fetch for FixtureFetch {
    async fn get(&self, request: &Request) -> Result<Fetched, FetchError> {
        self.responses
            .get(&request.url)
            .cloned()
            .ok_or(FetchError::Status(404))
    }
}

/// Remembers which payloads were archived.
#[derive(Debug, Default)]
pub struct MemoryArchive {
    stored: Mutex<Vec<(String, usize)>>,
}

impl MemoryArchive {
    /// The request tag and body length of every stored payload, sorted.
    pub fn stored(&self) -> Vec<(String, usize)> {
        let mut stored = self.stored.lock().expect("archive lock").clone();
        stored.sort();
        stored
    }
}

impl Archive for MemoryArchive {
    fn store(&self, entry: ArchiveEntry<'_>) -> Result<(), ArchiveError> {
        let record = (entry.request.tag.clone(), entry.body.len());
        self.stored.lock().expect("archive lock").push(record);
        Ok(())
    }
}

/// A radar frame in a form a golden file can hold and a person can read:
/// its time, its grid, how many cells are in each step, and every row as
/// runs of equal cells.
///
/// A run is a count and a letter: `.` for dry, `1` to `6` for the steps of
/// rain, `x` for a cell the radar did not see. `"310x 12. 12"` is a row of 310
/// cells not seen, 12 dry ones and one of step 2.
pub fn radar_golden(frame: &RadarFrame) -> serde_json::Value {
    let letter = |value: u8| match value {
        0 => '.',
        RADAR_UNSEEN => 'x',
        step @ 1..=6 => char::from(b'0' + step),
        _ => '?',
    };
    let mut counts = [0usize; 8];
    for value in &frame.cells {
        counts[usize::from((*value).min(RADAR_UNSEEN))] += 1;
    }
    let rows: Vec<String> = frame
        .cells
        .chunks(frame.grid.columns.max(1) as usize)
        .map(|row| {
            let mut runs: Vec<String> = Vec::new();
            let mut cells = row.iter().peekable();
            while let Some(value) = cells.next() {
                let mut length = 1;
                while cells.next_if_eq(&value).is_some() {
                    length += 1;
                }
                runs.push(format!("{length}{}", letter(*value)));
            }
            runs.join(" ")
        })
        .collect();
    serde_json::json!({
        "source": frame.source,
        "time": frame.time,
        "interval_s": frame.interval_s,
        "grid": frame.grid,
        "dry": counts[0],
        "steps": &counts[1..7],
        "unseen": counts[7],
        "rows": rows,
    })
}

/// Compares a value with a golden file of pretty-printed JSON.
///
/// Run the tests with `VENTI_UPDATE_GOLDEN=1` to write the file instead, then
/// review the change like any other diff.
///
/// # Panics
///
/// Panics when the value differs from the file, or the file cannot be read or
/// written.
pub fn assert_golden(path: &Path, value: &impl Serialize) {
    let mut actual = serde_json::to_string_pretty(value).expect("value serialises to JSON");
    actual.push('\n');

    if std::env::var_os("VENTI_UPDATE_GOLDEN").is_some() {
        std::fs::write(path, &actual)
            .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
        return;
    }

    let expected = std::fs::read_to_string(path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}\nrun with VENTI_UPDATE_GOLDEN=1 to create it",
            path.display()
        )
    });
    if expected != actual {
        let line = expected
            .lines()
            .zip(actual.lines())
            .position(|(a, b)| a != b)
            .map_or_else(
                || expected.lines().count().min(actual.lines().count()) + 1,
                |i| i + 1,
            );
        panic!(
            "output differs from {} at line {line}\n\
             run with VENTI_UPDATE_GOLDEN=1 to accept the new output",
            path.display()
        );
    }
}
