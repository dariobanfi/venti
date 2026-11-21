//! The rain radar: the last frames of each radar source, kept in memory as
//! the pictures the API hands out.
//!
//! ```text
//! the scheduler, every 300 s for a source
//!         │
//!         ▼
//!   which frames are wanted?   those after the newest one held, and no
//!         │                    older than the server keeps (two hours)
//!         ▼
//!   sources::run_radar: the provider's list, then each wanted frame
//!         │  the adapter lays a frame on the server's grid, in steps of rain
//!         ▼
//!   each frame → a PNG of one byte a pixel, whose values are the steps
//!         │
//!         ▼
//!   kept by its time; frames older than two hours dropped
//! ```
//!
//! **What is kept.** Per source at most [`MAX_FRAMES`] pictures, nothing
//! else: not the provider's file and not the frame's cells. A picture of
//! the first region is 4 to 9 kB (measured on 2026-11-21: 4.5 kB on a
//! morning of light rain over southern Bavaria, 8.3 kB with rain over
//! half of what the radar sees there), so two hours are 0.1 to 0.2 MB, and
//! never more than the 4.1 MB that 25 unpacked pictures of 163 kB would be.
//! Nothing is written to the disk and nothing to the database: a server
//! that starts asks the provider for the two hours again, 25 requests, and
//! serves them some seconds later. The provider's files are not archived
//! either. A day of them is 15 to 125 MB, the archive keeps fourteen, and
//! the provider itself keeps two days of them to fetch again.
//!
//! **A picture's pixels are steps, not colours.** A pixel's value is 0
//! where the radar saw no rain, 1 to 6 for the steps of
//! `venti_domain::RAIN_STEPS_MM_H`, and 7 where the radar did not see. The
//! colours are the apps' own; the palette in the file is a plain grey one,
//! so that the picture can be looked at as it is, and an app's core
//! replaces it ([`palette`]).
//!
//! The radar has a clock of its own, as the forecasts have: in the
//! development mode it stands at the moment the frames were recorded.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use chrono::Duration;
use futures::future::BoxFuture;
use venti_domain::{
    BoundingBox, RADAR_UNSEEN, RAIN_STEPS_MM_H, RadarFrame, RadarGrid, SourceId, Timestamp,
};
use venti_sources::{
    Fetch, Kind, PlanCtx, RadarRunReport, RadarSource, RadarWanted, RunOptions, run_radar,
};

use crate::clock::Clock;
use crate::ingest::DirArchive;
use crate::plan::Job;

/// For how long a frame is kept: two hours. The last hour is what a pilot
/// looks at; the second shows where the rain came from.
pub const KEPT: Duration = Duration::hours(2);

/// The most frames kept of one source, whatever its cadence: two hours of
/// a frame every five minutes, and the one that begins them.
pub const MAX_FRAMES: usize = 25;

/// One frame as the API hands it out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    /// The end of the interval the frame is of.
    pub time: Timestamp,
    /// How long the interval is, in seconds.
    pub interval_s: u32,
    /// The picture: a PNG of one byte a pixel, the values being steps.
    pub png: Vec<u8>,
}

/// What the server holds of one radar source.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// The source.
    pub source: SourceId,
    /// The grid its pictures are on.
    pub grid: RadarGrid,
    /// The frames held, the oldest first.
    pub frames: Vec<Arc<Picture>>,
}

/// What the API and the collector ask of the radar.
pub trait Radar: Send + Sync {
    /// Runs a source once: fetches the frames that are wanted and keeps
    /// them. `None` if the radar has no such source.
    fn collect<'a>(&'a self, source: &'a SourceId) -> BoxFuture<'a, Option<RadarRunReport>>;

    /// What is held of every source, sorted by source.
    fn layers(&self) -> Vec<Layer>;

    /// One frame of a source, if it is held.
    fn picture(&self, source: &SourceId, time: Timestamp) -> Option<Arc<Picture>>;

    /// The moment by the radar's clock.
    fn now(&self) -> Timestamp;
}

/// The radar service for what a server collects: `sources` are the radar
/// adapters it has, `jobs` its plan. `None` when the plan holds no radar
/// source. `kept` is for how long a frame is kept: [`KEPT`] on a server.
///
/// A source's pictures cover the area of its job, which is the box around
/// the regions that list it.
pub fn service_for<F>(
    jobs: &[Job],
    sources: Vec<Box<dyn RadarSource>>,
    fetch: F,
    clock: Arc<dyn Clock>,
    kept: Duration,
) -> Option<Arc<dyn Radar>>
where
    F: Fetch + Send + 'static,
{
    let mut planned: Vec<Held> = sources
        .into_iter()
        .filter_map(|source| {
            let id = source.manifest().id.clone();
            let area = jobs
                .iter()
                .find(|job| job.kind == Kind::Radar && job.source == id)
                .map(|job| job.area)?;
            let Some(grid) = RadarGrid::over(&area) else {
                tracing::error!(source = %id, "the radar's area has no grid on the map: no frames");
                return None;
            };
            Some(Held {
                source,
                area,
                grid,
                frames: Mutex::new(BTreeMap::new()),
            })
        })
        .collect();
    if planned.is_empty() {
        return None;
    }
    planned.sort_by(|a, b| a.source.manifest().id.cmp(&b.source.manifest().id));
    Some(Arc::new(RadarService {
        sources: planned,
        fetch,
        clock,
        kept,
    }))
}

/// One source and its frames.
struct Held {
    source: Box<dyn RadarSource>,
    area: BoundingBox,
    grid: RadarGrid,
    frames: Mutex<BTreeMap<Timestamp, Arc<Picture>>>,
}

impl Held {
    fn frames(&self) -> std::sync::MutexGuard<'_, BTreeMap<Timestamp, Arc<Picture>>> {
        // Nothing panics while holding the lock; if something ever did,
        // the frames are still the frames.
        self.frames
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

struct RadarService<F> {
    sources: Vec<Held>,
    fetch: F,
    clock: Arc<dyn Clock>,
    kept: Duration,
}

impl<F: Fetch + Send> RadarService<F> {
    fn held(&self, source: &SourceId) -> Option<&Held> {
        self.sources
            .iter()
            .find(|held| &held.source.manifest().id == source)
    }

    async fn run(&self, held: &Held) -> RadarRunReport {
        let now = self.clock.now();
        let oldest_kept = now - self.kept;
        let newest_held = held.frames().keys().next_back().copied();
        let wanted = RadarWanted {
            after: newest_held.map_or(oldest_kept, |newest| newest.max(oldest_kept)),
            grid: held.grid,
        };
        let ctx = PlanCtx {
            now,
            area: held.area,
        };
        // The list and a window of frames: a run has no use for more.
        let options = RunOptions {
            since: None,
            concurrency: 2,
            max_requests: MAX_FRAMES + 1,
        };
        let output = run_radar(
            held.source.as_ref(),
            &self.fetch,
            &None::<DirArchive>,
            &ctx,
            &options,
            &wanted,
        )
        .await;

        let mut report = output.report;
        let mut pictures = Vec::with_capacity(output.frames.len());
        for frame in &output.frames {
            match picture(frame) {
                Ok(picture) => pictures.push(Arc::new(picture)),
                Err(error) => {
                    // The pipeline checked the frame, so this is a bug
                    // here. The run says so and the frame is asked for
                    // again.
                    report.frames = report.frames.saturating_sub(1);
                    report.failures.push(venti_sources::Failure {
                        tag: format!("picture/{}", frame.time.format("%y%m%d%H%M")),
                        url: String::new(),
                        error,
                    });
                }
            }
        }
        let mut frames = held.frames();
        for picture in pictures {
            frames.insert(picture.time, picture);
        }
        frames.retain(|time, _| *time > oldest_kept);
        while frames.len() > MAX_FRAMES {
            frames.pop_first();
        }
        report
    }
}

impl<F: Fetch + Send + 'static> Radar for RadarService<F> {
    fn collect<'a>(&'a self, source: &'a SourceId) -> BoxFuture<'a, Option<RadarRunReport>> {
        Box::pin(async move {
            let held = self.held(source)?;
            Some(self.run(held).await)
        })
    }

    fn layers(&self) -> Vec<Layer> {
        self.sources
            .iter()
            .map(|held| Layer {
                source: held.source.manifest().id.clone(),
                grid: held.grid,
                frames: held.frames().values().cloned().collect(),
            })
            .collect()
    }

    fn picture(&self, source: &SourceId, time: Timestamp) -> Option<Arc<Picture>> {
        self.held(source)?.frames().get(&time).cloned()
    }

    fn now(&self) -> Timestamp {
        self.clock.now()
    }
}

/// The palette a picture is written with: red, green, blue and how solid,
/// for each value a pixel can have. No rain is clear; the steps are greys
/// from light to dark; what the radar did not see is a faint veil.
///
/// These are not the colours an app draws rain in. They are there so that
/// the file is a picture by itself; an app's core writes the app's own
/// colours over them.
pub fn palette() -> [[u8; 4]; 8] {
    [
        [0, 0, 0, 0],
        [200, 200, 200, 255],
        [168, 168, 168, 255],
        [136, 136, 136, 255],
        [104, 104, 104, 255],
        [72, 72, 72, 255],
        [40, 40, 40, 255],
        [128, 128, 128, 48],
    ]
}

/// A frame as a picture: a PNG with a palette, a byte a pixel.
fn picture(frame: &RadarFrame) -> Result<Picture, String> {
    // The palette has an entry for every step and for the two other
    // values a cell can have.
    debug_assert_eq!(palette().len(), RAIN_STEPS_MM_H.len() + 2);
    debug_assert_eq!(usize::from(RADAR_UNSEEN), palette().len() - 1);
    let failed = |error: png::EncodingError| format!("the picture cannot be written: {error}");
    let colours = palette();
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, frame.grid.columns, frame.grid.rows);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_palette(
            colours
                .iter()
                .flat_map(|colour| colour[..3].iter().copied())
                .collect::<Vec<u8>>(),
        );
        encoder.set_trns(colours.iter().map(|colour| colour[3]).collect::<Vec<u8>>());
        encoder.set_compression(png::Compression::High);
        // A pixel is a step, not a shade: predicting it from its
        // neighbours, as a photograph's filter does, makes it larger.
        encoder.set_filter(png::Filter::NoFilter);
        let mut writer = encoder.write_header().map_err(failed)?;
        writer.write_image_data(&frame.cells).map_err(failed)?;
        writer.finish().map_err(failed)?;
    }
    Ok(Picture {
        time: frame.time,
        interval_s: frame.interval_s,
        png,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use venti_sources::testing::{FixtureFetch, block_on};
    use venti_sources::{Manifest, ParsedRadar, Request, SourceError};

    use super::*;
    use crate::clock::FixedClock;

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

    fn at(hour: u32, minute: u32) -> Timestamp {
        Utc.with_ymd_and_hms(2026, 11, 21, hour, minute, 0).unwrap()
    }

    /// A source that has a frame every five minutes from 08:00 up to the
    /// run's own time, each all of one step.
    struct Fake(Manifest);

    impl RadarSource for Fake {
        fn manifest(&self) -> &Manifest {
            &self.0
        }

        fn plan(&self, _: &PlanCtx) -> Vec<Request> {
            vec![Request::new("list", "https://example.org/list")]
        }

        fn parse(
            &self,
            request: &Request,
            _: &[u8],
            ctx: &PlanCtx,
            wanted: &RadarWanted,
        ) -> Result<ParsedRadar, SourceError> {
            if request.tag == "list" {
                let mut times = Vec::new();
                let mut time = at(8, 0);
                while time <= ctx.now {
                    times.push(time);
                    time += Duration::minutes(5);
                }
                return Ok(ParsedRadar {
                    newest_listed: times.last().copied(),
                    follow_up: times
                        .into_iter()
                        .filter(|time| *time > wanted.after)
                        .map(|time| {
                            let stamp = time.format("%H%M");
                            Request::new(
                                format!("frame/{stamp}"),
                                format!("https://example.org/frame/{stamp}"),
                            )
                        })
                        .collect(),
                    ..ParsedRadar::default()
                });
            }
            let stamp = request.tag.trim_start_matches("frame/");
            let (hour, minute) = (stamp[..2].parse().unwrap(), stamp[2..].parse().unwrap());
            Ok(ParsedRadar {
                frames: vec![RadarFrame {
                    source: self.0.id.clone(),
                    time: at(hour, minute),
                    interval_s: 300,
                    grid: wanted.grid,
                    cells: vec![3; wanted.grid.cells()],
                }],
                ..ParsedRadar::default()
            })
        }
    }

    /// Every address of the fake source answers.
    fn fetch() -> FixtureFetch {
        let mut fetch = FixtureFetch::new().with("https://example.org/list", "list");
        for hour in 8..14 {
            for minute in (0..60).step_by(5) {
                fetch = fetch.with(
                    format!("https://example.org/frame/{hour:02}{minute:02}"),
                    "frame",
                );
            }
        }
        fetch
    }

    fn service(now: Timestamp) -> Arc<dyn Radar> {
        let source = SourceId::new("example-radar").unwrap();
        let jobs = [Job {
            source,
            kind: Kind::Radar,
            area: BoundingBox::new(47.0, 11.0, 47.1, 11.2).unwrap(),
        }];
        service_for(
            &jobs,
            vec![Box::new(Fake(Manifest::from_toml(MANIFEST).unwrap()))],
            fetch(),
            Arc::new(FixedClock(now)),
            KEPT,
        )
        .unwrap()
    }

    fn times(radar: &Arc<dyn Radar>) -> Vec<Timestamp> {
        radar.layers()[0]
            .frames
            .iter()
            .map(|picture| picture.time)
            .collect()
    }

    #[test]
    fn a_first_run_takes_two_hours_of_frames_and_no_more() {
        let radar = service(at(12, 1));
        let source = SourceId::new("example-radar").unwrap();

        let report = block_on(radar.collect(&source)).unwrap();

        // The frames after 10:01, to 12:00: 24 of them, and the list.
        assert_eq!(report.requests, 25);
        assert_eq!(report.frames, 24);
        assert!(report.is_healthy());
        let held = times(&radar);
        assert_eq!(held.len(), 24);
        assert_eq!((held[0], held[23]), (at(10, 5), at(12, 0)));
        assert_eq!(radar.picture(&source, at(12, 0)).unwrap().interval_s, 300);
        assert!(radar.picture(&source, at(10, 0)).is_none());

        // A run with nothing new asks for the list alone.
        let again = block_on(radar.collect(&source)).unwrap();
        assert_eq!((again.requests, again.frames), (1, 0));
        assert!(again.is_healthy());
        assert_eq!(times(&radar).len(), 24);

        // Another source's frames are nobody's.
        let other = SourceId::new("another").unwrap();
        assert!(block_on(radar.collect(&other)).is_none());
        assert!(radar.picture(&other, at(12, 0)).is_none());
    }

    #[test]
    fn a_later_run_fetches_what_is_new_and_drops_what_is_older_than_two_hours() {
        let source = SourceId::new("example-radar").unwrap();
        let jobs = [Job {
            source: source.clone(),
            kind: Kind::Radar,
            area: BoundingBox::new(47.0, 11.0, 47.1, 11.2).unwrap(),
        }];
        let clock = Arc::new(MovingClock(Mutex::new(at(12, 1))));
        let radar = service_for(
            &jobs,
            vec![Box::new(Fake(Manifest::from_toml(MANIFEST).unwrap()))],
            fetch(),
            clock.clone(),
            KEPT,
        )
        .unwrap();
        block_on(radar.collect(&source)).unwrap();

        // Eleven minutes later the provider has two frames more.
        *clock.0.lock().unwrap() = at(12, 12);
        let report = block_on(radar.collect(&source)).unwrap();

        assert_eq!((report.requests, report.frames), (3, 2));
        let held = times(&radar);
        // 10:15 to 12:10: the frames of 10:05 and 10:10 are older than
        // two hours now.
        assert_eq!((held[0], *held.last().unwrap()), (at(10, 15), at(12, 10)));
        assert_eq!(held.len(), 24);
    }

    struct MovingClock(Mutex<Timestamp>);

    impl Clock for MovingClock {
        fn now(&self) -> Timestamp {
            *self.0.lock().unwrap()
        }
    }

    #[test]
    fn a_picture_is_a_png_of_steps_with_a_grey_palette() {
        let grid = RadarGrid::over(&BoundingBox::new(47.0, 11.0, 47.05, 11.05).unwrap()).unwrap();
        assert_eq!((grid.columns, grid.rows), (4, 6));
        let mut cells = vec![0u8; grid.cells()];
        cells[..8].copy_from_slice(&[0, 1, 2, 3, 4, 5, 6, 7]);
        let frame = RadarFrame {
            source: SourceId::new("example-radar").unwrap(),
            time: at(11, 40),
            interval_s: 300,
            grid,
            cells: cells.clone(),
        };

        let picture = picture(&frame).unwrap();

        let decoder = png::Decoder::new(std::io::Cursor::new(&picture.png));
        let mut reader = decoder.read_info().unwrap();
        let info = reader.info().clone();
        assert_eq!((info.width, info.height), (4, 6));
        assert_eq!(info.color_type, png::ColorType::Indexed);
        assert_eq!(info.bit_depth, png::BitDepth::Eight);
        // Eight colours, the first clear, the last a veil.
        assert_eq!(info.palette.as_ref().unwrap().len(), 24);
        assert_eq!(
            info.trns.as_ref().unwrap().as_ref(),
            [0, 255, 255, 255, 255, 255, 255, 48]
        );
        // The pixels are the cells, value for value.
        let mut pixels = vec![0u8; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut pixels).unwrap();
        assert_eq!(pixels, cells);
    }
}
