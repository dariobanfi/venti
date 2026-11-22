//! Helpers for testing adapters from recorded payloads.
//!
//! Every adapter crate tests itself the same way: recorded responses go into a
//! [`FixtureFetch`], the real pipeline runs over them, and the result is
//! compared with a golden file checked into the repository.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::Mutex;

use serde::Serialize;
use venti_domain::{BoundingBox, RADAR_UNSEEN, RadarFrame, Timestamp};

use crate::{
    Archive, ArchiveEntry, ArchiveError, Fetch, FetchError, Fetched, Kind, ObservationSource,
    PlanCtx, Request, RunOptions, RunOutput, run_observations,
};

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

/// Holds a station adapter to the contract every one of them meets, over a
/// recording of its provider's answers: `answers` are the answers with the
/// addresses they answer, `at` the moment of a run over them and `area`
/// the area of that run. An adapter crate calls this from a test of its
/// own, with what it recorded; the server's tests call it for every
/// network of the build, with the sample of the development mode.
///
/// What is held:
///
/// - **The manifest** names observations among its kinds, may be passed
///   on (the server refuses to collect a source that may not), and says
///   where and when its licence was read. That it parses, with a provider,
///   countries, a cadence and a whole licence, was checked when the
///   adapter was made.
/// - **The plan** asks HTTPS addresses only; the server's fetcher asks no
///   other.
/// - **Recorded answers in, canonical records out**: a run over the
///   recording is a healthy one: it delivers observations, and at most a
///   tenth of its requests fail (a station's file that is late is part of
///   an honest recording). Every station
///   is the source's own and lies in the area, every observation is of
///   one of those stations and not of the future, and at least one has a
///   wind.
/// - **Units in range**: nothing of a clean recording is refused by the
///   pipeline's limits. A value outside them in an answer that was
///   recorded whole is nearly always a unit read wrongly. (The limits are
///   wide: km/h read as m/s passes them. A unit test of the adapter with a
///   value converted by hand is what holds the unit.)
/// - **The same in, the same out**: a second run gives the same records,
///   so nothing in the adapter reads a clock or keeps a state.
/// - **A damaged answer is survived**: each answer in turn cut off in its
///   middle, emptied, and replaced by bytes that are no text, the others
///   left whole. No run panics, and each still gives a report.
///
/// What it cannot hold, and a review does: that the values mean what the
/// adapter takes them for, which the provider's own documentation says and
/// a golden file shows; see `docs/adding-a-source.md`.
///
/// # Panics
///
/// Panics with what is not met.
pub fn check_station_adapter<B: AsRef<[u8]>>(
    adapter: &dyn ObservationSource,
    answers: &[(String, B)],
    at: Timestamp,
    area: BoundingBox,
) {
    let manifest = adapter.manifest();
    let id = &manifest.id;
    assert!(
        manifest.kinds.contains(&Kind::Observations),
        "{id}: the manifest's kinds do not name observations"
    );
    assert!(
        manifest.licence.redistribution,
        "{id}: the manifest says the data may not be passed on, and passing on is all the \
         server does with it"
    );
    assert!(
        !manifest.licence.verified_from.is_empty(),
        "{id}: the manifest names no page its licence was read on"
    );

    let ctx = PlanCtx { now: at, area };
    let plan = adapter.plan(&ctx);
    assert!(!plan.is_empty(), "{id}: nothing is asked for {area:?}");
    for request in &plan {
        assert!(
            request.url.starts_with("https://"),
            "{id}: {} is not an HTTPS address",
            request.url
        );
    }

    let run = |answers: &[(String, Vec<u8>)]| -> RunOutput {
        let fetch = FixtureFetch::new().with_all(answers.iter().cloned());
        block_on(run_observations(
            adapter,
            &fetch,
            &MemoryArchive::default(),
            &ctx,
            &RunOptions::default(),
        ))
    };
    let whole: Vec<(String, Vec<u8>)> = answers
        .iter()
        .map(|(url, body)| (url.clone(), body.as_ref().to_vec()))
        .collect();
    assert!(!whole.is_empty(), "{id}: record a fixture");

    let output = run(&whole);
    let report = &output.report;
    assert!(report.is_healthy(), "{id}: not a healthy run: {report:?}");
    assert!(
        report.rejected.is_empty(),
        "{id}: values outside the limits, which in a clean recording is a unit read wrongly: \
         {:?}",
        report.rejected
    );
    let stations: BTreeSet<_> = output.stations.iter().map(|station| &station.id).collect();
    for station in &output.stations {
        assert_eq!(
            station.id.source(),
            id,
            "{}: another source's station",
            station.id
        );
        assert!(
            area.contains(station.position),
            "{}: outside the area",
            station.id
        );
        assert!(!station.name.trim().is_empty(), "{}: no name", station.id);
    }
    for observation in &output.observations {
        assert!(
            stations.contains(&observation.station),
            "{}: an observation without its station",
            observation.station
        );
        assert!(
            observation.time <= at + chrono::Duration::minutes(15),
            "{}: an observation of the future, {}",
            observation.station,
            observation.time
        );
    }
    assert!(
        output
            .observations
            .iter()
            .any(|observation| observation.wind_speed_ms.is_some()),
        "{id}: no observation of the recording has a wind"
    );

    assert_eq!(
        run(&whole),
        output,
        "{id}: a second run gives other records"
    );

    let no_text: &[u8] = &[0xff, 0xfe, 0x00, 0x7b, 0x5b, 0x3b, 0x0a, 0xc3];
    for damaged in 0..whole.len() {
        let body = &whole[damaged].1;
        for replaced in [&body[..body.len() / 2], &[][..], no_text] {
            let mut answers = whole.clone();
            answers[damaged].1 = replaced.to_vec();
            // Whatever comes of it is a report: what is asked is that the
            // adapter does not panic.
            let _ = run(&answers);
        }
    }
}

/// Looks through the Rust files of an adapter crate's `src` folder for
/// what an adapter does not do: open a connection, read or write a file,
/// read a clock, read the environment or start a process. `crate_dir` is
/// the crate's folder, `env!("CARGO_MANIFEST_DIR")` in a test of the
/// crate.
///
/// This reads text, so it finds the plain cases and no more: it is a
/// reminder to whoever writes an adapter, and the pipeline's shape is what
/// holds the rule, since an adapter is only ever handed bytes.
///
/// # Panics
///
/// Panics with the file and the word found, or when the folder cannot be
/// read.
pub fn check_adapter_does_no_io(crate_dir: &Path) {
    const NOT_IN_AN_ADAPTER: [&str; 11] = [
        "std::net",
        "std::fs",
        "std::env",
        "std::process",
        "std::thread",
        "SystemTime",
        "Instant::now",
        "Utc::now",
        "Local::now",
        "reqwest",
        "tokio",
    ];
    let src = crate_dir.join("src");
    let files = std::fs::read_dir(&src).unwrap_or_else(|e| panic!("{}: {e}", src.display()));
    let mut read = 0;
    for file in files {
        let path = file.expect("a folder's entry").path();
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for word in NOT_IN_AN_ADAPTER {
            assert!(
                !text.contains(word),
                "{} uses {word}: an adapter names addresses and reads bytes, and the pipeline \
                 does the rest",
                path.display()
            );
        }
        read += 1;
    }
    assert!(read > 0, "{} has no Rust files", src.display());
}
