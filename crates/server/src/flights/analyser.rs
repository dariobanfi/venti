//! Runs the analysis of an uploaded file where it can do no harm.
//!
//! A flight file comes from a stranger, and analysing one can take minutes
//! if it was built to (see [`super::derive`]). So the server does not
//! analyse it itself. It starts a copy of its own program, hands it the
//! file, and waits a few seconds. A copy that is not done by then is
//! killed, which is the one way to really stop a computation; a copy that
//! crashes takes nothing with it.
//!
//! Tests and the development mode can analyse in the server's own process
//! instead, on a thread beside the ones that answer requests. That needs
//! no second program, and it cannot stop a computation that runs away: it
//! is for files the developer chose.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::Semaphore;
use venti_domain::{GeoPoint, Timestamp};
use venti_store::{Derived, Place, Score, ScoreKind};

use super::derive::{Derivation, Unusable, derive};

/// How long one file may take. A twelve-hour flight at one fix a second
/// takes about half a second on one core (measured 2026-11-19 with a
/// made-up flight), so this is generous for any real file and still short
/// for an attacker.
pub const TIME_ALLOWED: Duration = Duration::from_secs(10);

/// How many files are analysed at the same time. More wait their turn, so
/// uploads can never take every core.
const AT_ONCE: usize = 2;

/// How long an upload waits for its turn before the server says it is busy.
const PATIENCE: Duration = Duration::from_secs(20);

/// The subcommand of the server's own program that analyses one file.
pub const SUBCOMMAND: &str = "derive";

/// The most the child may answer. An answer is a few kilobytes.
const MAX_ANSWER_BYTES: usize = 256 * 1024;

/// Where files are analysed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Where {
    /// In a copy of the program at this path, started for each file and
    /// killed if it takes too long. What the server uses.
    Child(PathBuf),
    /// On a thread of this process. Cannot be stopped; for tests and the
    /// development mode.
    Thread,
}

/// Why a file was not analysed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Failure {
    /// The file is not a usable flight file.
    #[error("the file cannot be used: {0}")]
    Unusable(#[from] Unusable),
    /// The analysis did not finish in the time allowed and was stopped.
    #[error("the file could not be analysed in the time allowed")]
    TooSlow,
    /// Too many files are being analysed; try again shortly.
    #[error("too many flights are being analysed at once")]
    Busy,
    /// The analysis crashed on this file.
    #[error("the analysis of the file crashed")]
    Crashed,
    /// The analysis could not be run at all, whatever the file.
    #[error("the analysis could not be run")]
    Broken,
}

/// Analyses flight files, a few at a time, each within a time limit.
#[derive(Debug, Clone)]
pub struct Analyser {
    place: Where,
    time_allowed: Duration,
    turns: Arc<Semaphore>,
}

impl Analyser {
    /// An analyser that works in `place`.
    pub fn new(place: Where) -> Self {
        Self::with_time_allowed(place, TIME_ALLOWED)
    }

    /// An analyser that gives each file `time_allowed`.
    pub fn with_time_allowed(place: Where, time_allowed: Duration) -> Self {
        Self {
            place,
            time_allowed,
            turns: Arc::new(Semaphore::new(AT_ONCE)),
        }
    }

    /// An analyser that starts a copy of the running program for each file.
    pub fn child_of_this_program() -> Result<Self, String> {
        let program = std::env::current_exe()
            .map_err(|e| format!("cannot find the server's own program file: {e}"))?;
        Ok(Self::new(Where::Child(program)))
    }

    /// Works out a file's identity and derived values.
    pub async fn analyse(&self, file: Arc<[u8]>) -> Result<Derivation, Failure> {
        self.analyse_timed(file).await.0
    }

    /// Like [`analyse`](Self::analyse), and says how long the analysis
    /// itself took: from the moment it had its turn, not from the moment
    /// it began to wait for one. A file that never got a turn took no
    /// time.
    pub async fn analyse_timed(&self, file: Arc<[u8]>) -> (Result<Derivation, Failure>, Duration) {
        let turn = tokio::time::timeout(PATIENCE, self.turns.clone().acquire_owned()).await;
        let turn = match turn {
            Ok(Ok(turn)) => turn,
            Ok(Err(_)) => return (Err(Failure::Broken), Duration::ZERO),
            Err(_) => return (Err(Failure::Busy), Duration::ZERO),
        };
        let began = tokio::time::Instant::now();
        let outcome = self.with_turn(turn, file).await;
        (outcome, began.elapsed().min(self.time_allowed))
    }

    /// The time one file may take.
    pub fn time_allowed(&self) -> Duration {
        self.time_allowed
    }

    async fn with_turn(
        &self,
        turn: tokio::sync::OwnedSemaphorePermit,
        file: Arc<[u8]>,
    ) -> Result<Derivation, Failure> {
        match &self.place {
            Where::Child(program) => {
                let _turn = turn;
                self.in_child(program, &file).await
            }
            Where::Thread => {
                // The thread keeps its turn until it is really done, also
                // when nobody waits for it any more.
                let work = tokio::task::spawn_blocking(move || {
                    let result = derive(&file);
                    drop(turn);
                    result
                });
                match tokio::time::timeout(self.time_allowed, work).await {
                    Ok(Ok(result)) => Ok(result?),
                    // The analysis panicked.
                    Ok(Err(_)) => Err(Failure::Crashed),
                    Err(_) => Err(Failure::TooSlow),
                }
            }
        }
    }

    async fn in_child(&self, program: &PathBuf, file: &[u8]) -> Result<Derivation, Failure> {
        let broken = |what: &str, error: &dyn std::fmt::Display| {
            tracing::error!("flight analysis: {what}: {error}");
            Failure::Broken
        };
        let mut child = Command::new(program)
            .arg(SUBCOMMAND)
            // The child needs nothing of the server's settings, and the
            // database password least of all.
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            // Whatever ends the wait below, the time limit included, ends
            // the child.
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| broken("the program could not be started", &e))?;

        let mut stdin = child.stdin.take().ok_or(Failure::Broken)?;
        let work = async {
            // A child that dies early closes its end; that shows in how it
            // ended, not here.
            let _ = stdin.write_all(file).await;
            drop(stdin);
            child.wait_with_output().await
        };
        let output = match tokio::time::timeout(self.time_allowed, work).await {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => return Err(broken("the program could not be waited for", &error)),
            Err(_) => return Err(Failure::TooSlow),
        };
        if !output.status.success() {
            // A panic in the analysis, or a kill by the system.
            tracing::warn!("flight analysis ended badly on a file: {}", output.status);
            return Err(Failure::Crashed);
        }
        if output.stdout.len() > MAX_ANSWER_BYTES {
            return Err(broken("the answer is too long", &output.stdout.len()));
        }
        let answer: Answer = serde_json::from_slice(&output.stdout)
            .map_err(|e| broken("the answer cannot be read", &e))?;
        match answer {
            Answer::Flight(flight) => {
                Derivation::try_from(*flight).map_err(|e| broken("bad answer", &e))
            }
            Answer::Unusable(reason) => Err(Failure::Unusable(reason)),
        }
    }
}

/// What the `derive` subcommand does: reads a flight file from standard
/// input and writes what it gives to standard output, as JSON.
///
/// A file that is no flight file is an answer, not a failure: the status
/// is success either way. Only a crash or a kill ends it otherwise.
pub fn run_child() -> Result<(), String> {
    use std::io::{Read, Write};

    let mut file = Vec::new();
    std::io::stdin()
        .lock()
        .read_to_end(&mut file)
        .map_err(|e| format!("cannot read the flight file from standard input: {e}"))?;
    let answer = match derive(&file) {
        Ok(derivation) => Answer::Flight(Box::new(derivation.into())),
        Err(reason) => Answer::Unusable(reason),
    };
    let text = serde_json::to_vec(&answer).map_err(|e| e.to_string())?;
    std::io::stdout()
        .lock()
        .write_all(&text)
        .map_err(|e| format!("cannot write the answer: {e}"))
}

/// What the child writes.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Answer {
    Flight(Box<Wire>),
    Unusable(Unusable),
}

/// A [`Derivation`] as it crosses from the child to the server.
#[derive(Debug, Serialize, Deserialize)]
struct Wire {
    identity: String,
    version: u32,
    start: Timestamp,
    airtime_s: i64,
    takeoff: Option<WirePlace>,
    landing: Option<WirePlace>,
    track_length_m: f64,
    max_altitude_m: i32,
    max_climb_ms: f64,
    score: Option<WireScore>,
    glider: Option<String>,
    outline: Vec<[f64; 2]>,
    profile: Vec<[i32; 2]>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WirePlace {
    time: Timestamp,
    position: GeoPoint,
    altitude_m: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct WireScore {
    kind: String,
    closed: bool,
    distance_m: f64,
    points: f64,
}

impl From<Derivation> for Wire {
    fn from(derivation: Derivation) -> Self {
        let derived = derivation.derived;
        let place = |place: Place| WirePlace {
            time: place.time,
            position: place.position,
            altitude_m: place.altitude_m,
        };
        Self {
            identity: derivation.identity,
            version: derived.version,
            start: derived.start,
            airtime_s: derived.airtime_s,
            takeoff: derived.takeoff.map(place),
            landing: derived.landing.map(place),
            track_length_m: derived.track_length_m,
            max_altitude_m: derived.max_altitude_m,
            max_climb_ms: derived.max_climb_ms,
            score: derived.score.map(|score| WireScore {
                kind: score.kind.as_str().to_owned(),
                closed: score.closed,
                distance_m: score.distance_m,
                points: score.points,
            }),
            glider: derived.glider,
            outline: derived.outline,
            profile: derived.profile,
        }
    }
}

impl TryFrom<Wire> for Derivation {
    type Error = String;
    fn try_from(wire: Wire) -> Result<Self, Self::Error> {
        let place = |place: WirePlace| Place {
            time: place.time,
            position: place.position,
            altitude_m: place.altitude_m,
        };
        let score = wire
            .score
            .map(|score| {
                Ok::<_, String>(Score {
                    kind: ScoreKind::from_name(&score.kind)
                        .ok_or_else(|| format!("unknown score kind {:?}", score.kind))?,
                    closed: score.closed,
                    distance_m: score.distance_m,
                    points: score.points,
                })
            })
            .transpose()?;
        Ok(Self {
            identity: wire.identity,
            derived: Derived {
                version: wire.version,
                start: wire.start,
                airtime_s: wire.airtime_s,
                takeoff: wire.takeoff.map(place),
                landing: wire.landing.map(place),
                track_length_m: wire.track_length_m,
                max_altitude_m: wire.max_altitude_m,
                max_climb_ms: wire.max_climb_ms,
                score,
                glider: wire.glider,
                outline: wire.outline,
                profile: wire.profile,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use venti_flight::igc;
    use venti_flight::testing::synthetic_flight;

    use super::*;

    fn sample() -> Arc<[u8]> {
        igc::write(&synthetic_flight()).into_bytes().into()
    }

    #[tokio::test]
    async fn a_thread_gives_what_the_function_gives() {
        let analyser = Analyser::new(Where::Thread);
        let derivation = analyser.analyse(sample()).await.unwrap();
        assert_eq!(derivation, derive(&sample()).unwrap());

        let refused = analyser.analyse(b"hello".to_vec().into()).await;
        assert_eq!(refused, Err(Failure::Unusable(Unusable::NoDate)));
    }

    #[test]
    fn what_crosses_from_the_child_is_what_went_in() {
        let derivation = derive(&sample()).unwrap();
        let answer = Answer::Flight(Box::new(derivation.clone().into()));
        let text = serde_json::to_vec(&answer).unwrap();
        let Answer::Flight(wire) = serde_json::from_slice(&text).unwrap() else {
            panic!("a flight went in");
        };
        assert_eq!(Derivation::try_from(*wire).unwrap(), derivation);

        let text = serde_json::to_string(&Answer::Unusable(Unusable::TooManyFixes)).unwrap();
        assert_eq!(text, r#"{"unusable":"too_many_fixes"}"#);
    }

    #[tokio::test]
    async fn only_a_few_files_are_analysed_at_once() {
        let analyser = Analyser::new(Where::Thread);
        let held: Vec<_> = (0..AT_ONCE)
            .map(|_| analyser.turns.clone().try_acquire_owned().unwrap())
            .collect();
        tokio::time::pause();

        assert_eq!(analyser.analyse(sample()).await, Err(Failure::Busy));

        drop(held);
        tokio::time::resume();
        assert!(analyser.analyse(sample()).await.is_ok());
    }

    #[tokio::test]
    async fn a_program_that_cannot_be_started_is_a_failure_not_a_crash() {
        let nowhere = Where::Child(PathBuf::from("/nonexistent/venti-server"));
        let result = Analyser::new(nowhere).analyse(sample()).await;
        assert_eq!(result, Err(Failure::Broken));
    }
}
