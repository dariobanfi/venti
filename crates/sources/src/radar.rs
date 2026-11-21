//! Running a source of rain radar.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use venti_domain::{RADAR_UNSEEN, RadarFrame, RadarGrid, SourceId, Timestamp};

use crate::run::fetch_all;
use crate::{
    Archive, Failure, Fetch, Fetched, Manifest, PlanCtx, Request, RunOptions, Skipped, SourceError,
};

/// How far ahead of the run a frame's time may lie and still be taken:
/// a provider's clock and the server's need not agree to the second.
const CLOCK_SLACK_S: i64 = 60;

/// What a radar run is asked for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadarWanted {
    /// Frames of this moment and before are not wanted: the server holds
    /// them already, or they are older than it keeps. The adapter does not
    /// fetch them.
    pub after: Timestamp,
    /// The grid every frame is delivered on.
    pub grid: RadarGrid,
}

/// What a radar adapter made of one response.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedRadar {
    /// Frames found in the response, on the grid that was asked for.
    pub frames: Vec<RadarFrame>,
    /// The time of the newest frame the provider has, if the response
    /// says: a list of the provider's frames does. It is how a run that
    /// had nothing new to fetch still knows the provider is publishing.
    pub newest_listed: Option<Timestamp>,
    /// Further requests the adapter needs now that it has seen this
    /// response: the frames a list names.
    pub follow_up: Vec<Request>,
    /// Parts of the response that could not be converted.
    pub skipped: Vec<Skipped>,
}

/// An adapter for a source of rain radar.
///
/// Pure, like [`ObservationSource`](crate::ObservationSource): it names the
/// addresses it needs and turns their bytes into canonical frames. What a
/// provider publishes is a picture on its own grid, in its own projection
/// and units; the adapter lays it on the grid it is asked for and turns its
/// values into the steps of `venti_domain::RAIN_STEPS_MM_H`. A place its
/// product has no value for is `RADAR_UNSEEN`, never dry.
///
/// A run is told which frames are wanted ([`RadarWanted`]): the adapter
/// asks for no frame the server already holds. See
/// [`run_radar`](crate::run_radar).
pub trait RadarSource: Send + Sync {
    /// The declared facts about this source.
    fn manifest(&self) -> &Manifest;

    /// The requests that start a run: as a rule the provider's list of the
    /// frames it has.
    fn plan(&self, ctx: &PlanCtx) -> Vec<Request>;

    /// Converts the response to one request into frames and, where the
    /// provider needs several steps, further requests.
    fn parse(
        &self,
        request: &Request,
        body: &[u8],
        ctx: &PlanCtx,
        wanted: &RadarWanted,
    ) -> Result<ParsedRadar, SourceError>;
}

/// A frame the pipeline refused to pass on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RadarRejection {
    /// The frame's time.
    pub time: Timestamp,
    /// Why.
    pub reason: String,
}

/// What happened in a radar run. This is the source's health record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RadarRunReport {
    /// The source that ran.
    pub source: SourceId,
    /// The time of the run.
    pub now: Timestamp,
    /// How often the provider publishes a frame, in seconds, by the
    /// source's manifest.
    pub cadence_seconds: u32,
    /// Requests made.
    pub requests: usize,
    /// Requests that failed, or whose response could not be parsed or archived.
    pub failures: Vec<Failure>,
    /// Parts of responses the adapter left out.
    pub skipped: Vec<Skipped>,
    /// Frames removed by validation.
    pub rejected: Vec<RadarRejection>,
    /// Whether the run stopped early because it used up its request budget.
    pub budget_exhausted: bool,
    /// Frames delivered.
    pub frames: usize,
    /// The time of the newest frame the provider has, by its own list or,
    /// without one, by the frames delivered.
    pub newest_listed: Option<Timestamp>,
}

impl RadarRunReport {
    /// Whether the run did its job: every request worked, and the
    /// provider's newest frame is no older than three of its intervals.
    ///
    /// A run that delivered nothing is healthy when there was nothing new:
    /// a server asks more often than a provider publishes. A provider
    /// whose newest frame is a quarter of an hour old has stopped
    /// publishing, whatever its server answers, and a pilot must not be
    /// shown old rain as the rain of now.
    pub fn is_healthy(&self) -> bool {
        let patience = chrono::Duration::seconds(3 * i64::from(self.cadence_seconds));
        self.failures.is_empty()
            && !self.budget_exhausted
            && self
                .newest_listed
                .is_some_and(|newest| self.now - newest <= patience)
    }
}

/// The frames a radar run produced, and its report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RadarRunOutput {
    /// Frames, sorted by time, the oldest first.
    pub frames: Vec<RadarFrame>,
    /// What happened.
    pub report: RadarRunReport,
}

/// Runs one radar source once: fetch, archive, parse, validate.
///
/// The result is the frames after `wanted.after` that the provider had,
/// each on `wanted.grid`, sorted by time. Requests are budgeted and
/// archived as for every other kind of source; `options.since` plays no
/// part.
///
/// Validation removes a frame whole, and reports it: one of another source
/// than the one that ran, one on another grid than was asked for, one
/// whose cells are not as many as its grid's or hold a value that is no
/// step, and one dated after the run. A frame that is not wanted, and a
/// second frame of a time already seen, are dropped without a word.
pub async fn run_radar(
    source: &dyn RadarSource,
    fetch: &impl Fetch,
    archive: &impl Archive,
    ctx: &PlanCtx,
    options: &RunOptions,
    wanted: &RadarWanted,
) -> RadarRunOutput {
    let manifest = source.manifest();
    let mut skipped = Vec::new();
    let mut found: Vec<RadarFrame> = Vec::new();
    let mut newest_listed: Option<Timestamp> = None;

    let take = |request: &Request, response: &Fetched| {
        let parsed = source.parse(request, &response.body, ctx, wanted)?;
        newest_listed = newest_listed.max(parsed.newest_listed);
        found.extend(parsed.frames);
        skipped.extend(parsed.skipped);
        Ok(parsed.follow_up)
    };
    let fetched = fetch_all(
        manifest,
        source.plan(ctx),
        fetch,
        archive,
        ctx,
        options,
        take,
    )
    .await;

    let mut rejected = Vec::new();
    let mut frames: BTreeMap<Timestamp, RadarFrame> = BTreeMap::new();
    for frame in found {
        if frame.time <= wanted.after || frames.contains_key(&frame.time) {
            continue;
        }
        match check_frame(&frame, manifest, ctx, wanted) {
            Ok(()) => {
                frames.insert(frame.time, frame);
            }
            Err(reason) => rejected.push(RadarRejection {
                time: frame.time,
                reason,
            }),
        }
    }

    let newest_delivered = frames.keys().next_back().copied();
    let frames: Vec<RadarFrame> = frames.into_values().collect();
    let report = RadarRunReport {
        source: manifest.id.clone(),
        now: ctx.now,
        cadence_seconds: manifest.cadence_seconds,
        requests: fetched.requests,
        failures: fetched.failures,
        skipped,
        rejected,
        budget_exhausted: fetched.budget_exhausted,
        frames: frames.len(),
        // A provider's list cannot name a frame of a later time than now;
        // one that does has a clock that is off, and is not believed
        // beyond the run's own time.
        newest_listed: newest_listed.max(newest_delivered).map(|t| t.min(ctx.now)),
    };
    RadarRunOutput { frames, report }
}

/// What is wrong with a frame, if anything is.
fn check_frame(
    frame: &RadarFrame,
    manifest: &Manifest,
    ctx: &PlanCtx,
    wanted: &RadarWanted,
) -> Result<(), String> {
    if frame.source != manifest.id {
        return Err(format!(
            "the frame does not belong to the source {}",
            manifest.id
        ));
    }
    if frame.time > ctx.now + chrono::Duration::seconds(CLOCK_SLACK_S) {
        return Err("the frame is dated after the run".to_owned());
    }
    if frame.grid != wanted.grid {
        return Err("the frame is on another grid than was asked for".to_owned());
    }
    if frame.cells.len() != frame.grid.cells() {
        return Err(format!(
            "{} cells on a grid of {}",
            frame.cells.len(),
            frame.grid.cells()
        ));
    }
    if frame.interval_s == 0 {
        return Err("the frame covers no time".to_owned());
    }
    if let Some(value) = frame.cells.iter().find(|value| **value > RADAR_UNSEEN) {
        return Err(format!("a cell holds {value}, which is no step of rain"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use venti_domain::BoundingBox;

    use super::*;
    use crate::testing::{FixtureFetch, MemoryArchive, block_on};

    const MANIFEST: &str = r#"
        id = "example-radar"
        provider = "Example Weather Service"
        kinds = ["radar"]
        countries = ["DE"]
        cadence_seconds = 300
        auth = "none"

        [licence]
        name = "CC BY 4.0"
        commercial = true
        redistribution = true
        share_alike = false
        attribution = "Source: Example"
        link = "https://example.org/data"
        verified_on = "2026-11-21"
        verified_from = ["https://example.org/legal"]
    "#;

    fn at(minute: u32) -> Timestamp {
        Utc.with_ymd_and_hms(2026, 11, 21, 11, minute, 0).unwrap()
    }

    /// A source whose list names a frame every five minutes from 11:20 to
    /// 11:40, and whose frames are whatever `frame` makes of their minute.
    struct Fake {
        manifest: Manifest,
        frame: fn(u32, &RadarWanted) -> RadarFrame,
    }

    fn sound(minute: u32, wanted: &RadarWanted) -> RadarFrame {
        RadarFrame {
            source: SourceId::new("example-radar").unwrap(),
            time: at(minute),
            interval_s: 300,
            grid: wanted.grid,
            cells: vec![0; wanted.grid.cells()],
        }
    }

    impl RadarSource for Fake {
        fn manifest(&self) -> &Manifest {
            &self.manifest
        }

        fn plan(&self, _: &PlanCtx) -> Vec<Request> {
            vec![Request::new("list", "https://example.org/list")]
        }

        fn parse(
            &self,
            request: &Request,
            _: &[u8],
            _: &PlanCtx,
            wanted: &RadarWanted,
        ) -> Result<ParsedRadar, SourceError> {
            if request.tag == "list" {
                let follow_up = (20..=40)
                    .step_by(5)
                    .filter(|minute| at(*minute) > wanted.after)
                    .map(|minute| {
                        Request::new(
                            format!("frame/{minute}"),
                            format!("https://example.org/frame/{minute}"),
                        )
                    })
                    .collect();
                return Ok(ParsedRadar {
                    newest_listed: Some(at(40)),
                    follow_up,
                    ..ParsedRadar::default()
                });
            }
            let minute: u32 = request.tag.trim_start_matches("frame/").parse().unwrap();
            Ok(ParsedRadar {
                frames: vec![(self.frame)(minute, wanted)],
                ..ParsedRadar::default()
            })
        }
    }

    fn run(
        frame: fn(u32, &RadarWanted) -> RadarFrame,
        now: Timestamp,
        after: Timestamp,
    ) -> RadarRunOutput {
        let source = Fake {
            manifest: Manifest::from_toml(MANIFEST).unwrap(),
            frame,
        };
        let area = BoundingBox::new(47.0, 11.0, 47.1, 11.1).unwrap();
        let wanted = RadarWanted {
            after,
            grid: RadarGrid::over(&area).unwrap(),
        };
        let fetch = (20..=40).step_by(5).fold(
            FixtureFetch::new().with("https://example.org/list", "list"),
            |fetch, minute| fetch.with(format!("https://example.org/frame/{minute}"), "frame"),
        );
        let ctx = PlanCtx { now, area };
        block_on(run_radar(
            &source,
            &fetch,
            &MemoryArchive::default(),
            &ctx,
            &RunOptions::default(),
            &wanted,
        ))
    }

    #[test]
    fn a_run_delivers_the_frames_that_are_wanted_and_asks_for_no_other() {
        let output = run(sound, at(41), at(30));

        let times: Vec<Timestamp> = output.frames.iter().map(|frame| frame.time).collect();
        assert_eq!(times, [at(35), at(40)]);
        // The list and the two frames after 11:30.
        assert_eq!(output.report.requests, 3);
        assert_eq!(output.report.newest_listed, Some(at(40)));
        assert!(output.report.is_healthy());
    }

    #[test]
    fn a_run_with_nothing_new_is_healthy_and_one_of_a_silent_provider_is_not() {
        // Everything the provider lists is held: one request, no frame.
        let nothing_new = run(sound, at(42), at(40));
        assert_eq!(nothing_new.report.requests, 1);
        assert!(nothing_new.frames.is_empty());
        assert!(nothing_new.report.is_healthy());

        // The same list twenty minutes later: the provider has stopped.
        let silent = run(
            sound,
            Utc.with_ymd_and_hms(2026, 11, 21, 12, 0, 0).unwrap(),
            at(40),
        );
        assert!(silent.report.failures.is_empty());
        assert!(!silent.report.is_healthy());
    }

    #[test]
    fn a_frame_that_cannot_be_right_is_removed_whole_and_named() {
        let damaged: fn(u32, &RadarWanted) -> RadarFrame = |minute, wanted| {
            let mut frame = sound(minute, wanted);
            match minute {
                // Fewer cells than the grid has.
                20 => {
                    frame.cells.pop();
                }
                // A value that is no step.
                25 => frame.cells[0] = 8,
                // Another grid than was asked for.
                30 => frame.grid.west += 1.0,
                // Another source's.
                35 => frame.source = SourceId::new("someone-else").unwrap(),
                _ => {}
            }
            frame
        };

        let output = run(damaged, at(41), at(0));

        let kept: Vec<Timestamp> = output.frames.iter().map(|frame| frame.time).collect();
        assert_eq!(kept, [at(40)]);
        let why: Vec<(Timestamp, &str)> = output
            .report
            .rejected
            .iter()
            .map(|rejection| (rejection.time, rejection.reason.as_str()))
            .collect();
        assert_eq!(why.len(), 4);
        assert!(why[0].1.contains("cells on a grid of"), "{why:?}");
        assert!(why[1].1.contains("no step of rain"), "{why:?}");
        assert!(why[2].1.contains("another grid"), "{why:?}");
        assert!(why[3].1.contains("does not belong"), "{why:?}");
        // What was removed does not make the run unhealthy: the requests
        // worked and the provider is publishing.
        assert!(output.report.is_healthy());
    }

    #[test]
    fn a_frame_from_the_future_is_refused() {
        // The run is at 11:32; the list's frames of 11:35 and 11:40 are
        // dated after it.
        let output = run(sound, at(32), at(0));

        let kept: Vec<Timestamp> = output.frames.iter().map(|frame| frame.time).collect();
        assert_eq!(kept, [at(20), at(25), at(30)]);
        assert_eq!(output.report.rejected.len(), 2);
        // The provider's word for its newest frame is not believed beyond
        // the run's own time.
        assert_eq!(output.report.newest_listed, Some(at(32)));
    }
}
