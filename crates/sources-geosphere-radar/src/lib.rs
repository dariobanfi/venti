//! Adapter for the precipitation analysis of INCA, GeoSphere Austria's
//! nowcasting system, as a layer of the rain radar.
//!
//! GeoSphere Austria publishes no radar composite as open data (looked for
//! on its Data Hub on 2026-11-21). What it publishes is INCA: an analysis
//! and a nowcast on a grid of one kilometre over Austria and what lies
//! around it, 701 by 431 points, in which rain is put together from the
//! Austrian radars and the rain gauges. The dataset
//! `nowcast-v1-15min-1km` holds its runs, one every quarter of an hour,
//! each of 13 steps of a quarter of an hour. This adapter takes the
//! **first step of each run and nothing else**: the step at the run's own
//! reference time, lead time 0, the sum of rain of the quarter of an hour
//! that ends then. The later steps are a forecast, and a forecast is no
//! picture of the rain that fell.
//!
//! *To verify* with the provider: that this first step is INCA's analysis
//! (rain as it was measured and put together) and not already a forecast.
//! The dataset's page calls the whole dataset a forecast and does not say
//! what its first step is; the file gives it a lead time of 0 hours.
//!
//! Facts read on 2026-11-21, on GeoSphere Austria's own pages and in the
//! recorded answers (`tests/fixtures/NOTICE.txt` has the addresses):
//!
//! - **The service**: `…/v1/grid/forecast/nowcast-v1-15min-1km`.
//!   `/metadata` lists the runs it holds by their reference times, the
//!   newest first (`available_forecast_reftimes`, six of them). A frame is
//!   asked for with `forecast_offset`, the run's place in that list, and
//!   `start` and `end` both at the run's reference time. A run appears
//!   some 22 minutes after the quarter of an hour it is of.
//! - **The place in the list shifts** when a new run arrives between the
//!   list and the frame. So a frame is never taken at its word for being
//!   the one asked for: the file says its own time (`time`, as days since
//!   a moment its unit names, and `leadtime` in hours), and a file of
//!   another moment, or of a lead time other than 0, is refused. The next
//!   run asks again.
//! - **The values** (`rr`): sixteen-bit integers with `scale_factor` 0.01
//!   and `_FillValue` and `missing_value` -999, in `kg m-2`, which is
//!   millimetres: hundredths of a millimetre in a quarter of an hour. The
//!   rate in millimetres an hour is four times the sum. A point without a
//!   value is not seen, never dry; the recorded frames have a value at
//!   every point.
//! - **The grid**: EPSG:31287, see [`projection`]. The file's `x` and `y`
//!   say where its rectangle lies, in metres, each point the middle of a
//!   cell of one kilometre; the rows run from the south. The file carries
//!   the degrees of every point too, and the adapter holds its projection
//!   against the four corners of every frame.
//! - **The box**: the service cuts the grid to a box of degrees, and gives
//!   the rectangle of the grid's rows and columns that reach into it. The
//!   grid's rows are straight on INCA's map and bent on a map of degrees,
//!   so the rectangle's edges are not the box's: with the first region's
//!   own box, 107 cells in the middle of the map's last row lie south of
//!   the rectangle (counted on a recording of 2026-11-21, 12:39 UTC, made
//!   with the plain box). The adapter therefore asks for the box widened
//!   by a twentieth of a degree on every side. The recorded answers are
//!   answers to that widened request (13:17 UTC on the same day): a
//!   rectangle of 608 by 263 points, in which every one of the 162,864
//!   cells of the first region's map has a value.
//! - **The limit**: 5 requests a second and 240 an hour for an address,
//!   shared with the station adapter `geosphere-tawes`; see the manifest.
//!
//! A run takes two steps: the list of runs, then the first step of each
//! run the server does not hold yet. The adapter makes a frame's address
//! from the run's time and its place in the list, never from anything
//! else the list says.

pub mod fixtures;
mod projection;

use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};
use serde::Deserialize;
use venti_domain::{BoundingBox, RADAR_UNSEEN, RadarFrame, RadarGrid, Timestamp, rain_step};
use venti_hdf5::{File, Object, Value};
use venti_sources::{
    Manifest, ParsedRadar, PlanCtx, RadarSource, RadarWanted, Request, Skipped, SourceError,
};

use crate::projection::AustriaLambert;

/// The dataset on GeoSphere Austria's dataset service.
const BASE: &str = "https://dataset.api.hub.geosphere.at/v1/grid/forecast/nowcast-v1-15min-1km";

/// How long the interval of a step is: a quarter of an hour. The dataset's
/// name and its metadata (`frequency`, "15min") say so; the file does not.
const INTERVAL_S: u32 = 900;

/// By how much the box asked for is wider than the area on every side, in
/// degrees, some five cells: enough for the rectangle of the grid that
/// the service cuts out to cover the area's edges where the grid's rows
/// and columns bend away from them.
const WIDENED_DEG: f64 = 0.05;

/// The most runs a list may name and still be read. The service holds six.
const MAX_LISTED: usize = 64;

/// The most frames one run asks for: two hours of them and one more. The
/// service holds six; a list that names more gets its newest.
const MAX_FRAMES_PER_RUN: usize = 9;

/// The most points a side of a frame's rectangle may have. The whole grid
/// has 701 by 431.
const MAX_SIDE: usize = 2048;

/// How far a corner of a frame may lie from where the projection puts the
/// degrees the file gives for it, in metres. The recorded frames are
/// within a metre at every point.
const CORNER_TOLERANCE_M: f64 = 25.0;

/// The address of the list of runs.
pub fn list_url() -> String {
    format!("{BASE}/metadata")
}

/// The address of the first step of the run of `run`, which stands at
/// `place` in the list of runs, for an area: the box of `area` widened by
/// a twentieth of a degree, written to a hundredth.
pub fn frame_url(run: Timestamp, place: usize, area: &BoundingBox) -> String {
    let moment = run.format("%Y-%m-%dT%H:%M");
    format!(
        "{BASE}?parameters=rr&bbox={:.2},{:.2},{:.2},{:.2}&forecast_offset={place}\
         &start={moment}&end={moment}&output_format=netcdf",
        area.south() - WIDENED_DEG,
        area.west() - WIDENED_DEG,
        area.north() + WIDENED_DEG,
        area.east() + WIDENED_DEG,
    )
}

/// The tag of the request for the frame of a run: `frame/2611211245`.
fn frame_tag(run: Timestamp) -> String {
    format!("frame/{}", run.format("%y%m%d%H%M"))
}

/// The run a request's tag names, if it is one of a frame.
fn run_in_tag(tag: &str) -> Option<Timestamp> {
    let digits = tag.strip_prefix("frame/")?;
    if digits.len() != 10 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let naive = NaiveDateTime::parse_from_str(&format!("20{digits}"), "%Y%m%d%H%M").ok()?;
    Some(Utc.from_utc_datetime(&naive))
}

/// What is read of the list of runs.
#[derive(Deserialize)]
struct Listed {
    available_forecast_reftimes: Vec<String>,
}

/// The first step of INCA's nowcast as a source of rain radar.
#[derive(Debug, Clone)]
pub struct GeosphereIncaRr {
    manifest: Manifest,
}

impl GeosphereIncaRr {
    /// Creates the adapter.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn new() -> Self {
        let manifest = Manifest::from_toml(include_str!("../manifest.toml"))
            .expect("the manifest compiled into venti-sources-geosphere-radar is valid");
        Self { manifest }
    }

    /// Reads the list of runs: asks for the first step of those that are
    /// wanted, the newest [`MAX_FRAMES_PER_RUN`] of them.
    fn list(body: &[u8], ctx: &PlanCtx, wanted: &RadarWanted) -> Result<ParsedRadar, String> {
        let listed: Listed = serde_json::from_slice(body)
            .map_err(|error| format!("the answer is no list of runs: {error}"))?;
        if listed.available_forecast_reftimes.len() > MAX_LISTED {
            return Err(format!(
                "the list names {} runs",
                listed.available_forecast_reftimes.len()
            ));
        }
        // A run with its place in the list: the service counts the runs
        // from the newest, as the list gives them.
        let mut runs: Vec<(Timestamp, usize)> = Vec::new();
        let mut skipped = Vec::new();
        for (place, text) in listed.available_forecast_reftimes.iter().enumerate() {
            match DateTime::parse_from_str(text, "%Y-%m-%dT%H:%M%:z") {
                Ok(time) => runs.push((time.with_timezone(&Utc), place)),
                Err(_) => skipped.push(Skipped::new(text.as_str(), "not the time of a run")),
            }
        }
        runs.sort_unstable();
        runs.dedup_by_key(|(time, _)| *time);
        let newest_listed = runs.last().map(|(time, _)| *time);
        let mut asked: Vec<(Timestamp, usize)> = runs
            .into_iter()
            .filter(|(time, _)| *time > wanted.after && *time <= ctx.now + Duration::minutes(1))
            .collect();
        let surplus = asked.len().saturating_sub(MAX_FRAMES_PER_RUN);
        asked.drain(..surplus);
        Ok(ParsedRadar {
            frames: Vec::new(),
            newest_listed,
            follow_up: asked
                .into_iter()
                .map(|(run, place)| Request::new(frame_tag(run), frame_url(run, place, &ctx.area)))
                .collect(),
            skipped,
        })
    }

    /// Reads one frame and lays it on the grid that is wanted.
    fn frame(&self, named: Timestamp, body: &[u8], grid: &RadarGrid) -> Result<RadarFrame, String> {
        let file = File::open(body)?;
        let variable = |name: &str| -> Result<Variable<'_>, String> {
            let object = file.object(&[name])?;
            Ok(Variable {
                name: name.to_owned(),
                attributes: file.attributes(&object)?,
                object,
            })
        };

        // What the file is: INCA's grid, and a sum of rain.
        let crs = file.attributes(&file.object(&[])?)?;
        if !crs
            .iter()
            .any(|(name, value)| name == "crs" && value.text() == Some("EPSG:31287"))
        {
            return Err("the file does not say that its grid is EPSG:31287".to_owned());
        }
        let rain = variable("rr")?;
        if rain.text("standard_name")? != "precipitation_amount" || rain.text("units")? != "kg m-2"
        {
            return Err(format!(
                "the values are {:?} in {:?}, not a precipitation amount in kg m-2",
                rain.text("standard_name")?,
                rain.text("units")?
            ));
        }
        let scale = rain.number("scale_factor")?;
        if !(scale > 0.0 && scale <= 1.0) {
            return Err(format!("a scale factor of {scale}"));
        }
        if rain.has("add_offset") {
            return Err("the values have an offset, which this adapter does not know".to_owned());
        }
        let (fill, missing) = (rain.number("_FillValue")?, rain.number("missing_value")?);

        // Of when it is: the moment asked for, and no forecast.
        let (time, lead) = (variable("time")?, variable("leadtime")?);
        let since = time
            .text("units")?
            .strip_prefix("days since ")
            .and_then(|moment| NaiveDateTime::parse_from_str(moment, "%Y-%m-%d %H:%M:%S").ok())
            .map(|naive| Utc.from_utc_datetime(&naive))
            .ok_or_else(|| format!("the time is counted in {:?}", time.text("units")))?;
        let days = file.numbers(&time.object, 1)?[0];
        if !(0.0..=36_600.0).contains(&days) {
            return Err(format!("a time of {days} days"));
        }
        // Whole seconds of at most a hundred years.
        #[allow(clippy::cast_possible_truncation)]
        let valid = since + Duration::seconds((days * 86_400.0).round() as i64);
        if lead.text("units")? != "hours" {
            return Err(format!(
                "the lead time is in {:?}, not in hours",
                lead.text("units")?
            ));
        }
        let lead_hours = file.numbers(&lead.object, 1)?[0];
        // Zero is written as zero.
        #[allow(clippy::float_cmp)]
        if lead_hours != 0.0 {
            return Err(format!(
                "the file is a forecast {lead_hours} hours ahead, not the first step of a run"
            ));
        }
        if valid != named {
            return Err(format!(
                "the file is of {}, and was asked for as the run of {}",
                valid.format("%Y-%m-%d %H:%M UTC"),
                named.format("%Y-%m-%d %H:%M UTC")
            ));
        }

        // Where its rectangle lies.
        let shape = file.shape(&rain.object)?;
        let &[1, rows, columns] = shape.as_slice() else {
            return Err(format!("the values are {shape:?}, not one picture"));
        };
        if !(2..=MAX_SIDE).contains(&rows) || !(2..=MAX_SIDE).contains(&columns) {
            return Err(format!("a picture of {rows} rows of {columns}"));
        }
        let axis = |name: &str, length: usize| -> Result<(f64, f64), String> {
            let axis = variable(name)?;
            if axis.text("units")? != "m" {
                return Err(format!(
                    "{name} is in {:?}, not metres",
                    axis.text("units")?
                ));
            }
            let values = file.numbers(&axis.object, length)?;
            let step = values[1] - values[0];
            // Whole metres, each a kilometre from the last.
            #[allow(clippy::float_cmp)]
            let even = values.windows(2).all(|pair| pair[1] - pair[0] == step);
            if !even || !(100.0..=10_000.0).contains(&step) {
                return Err(format!(
                    "{name} does not go up in even steps of a cell's size"
                ));
            }
            Ok((values[0], step))
        };
        // Each the first point's place and the distance to the next: the
        // westernmost column and the southernmost row.
        let ((west, dx), (south, dy)) = (axis("x", columns)?, axis("y", rows)?);
        let projection = AustriaLambert::new();

        // The file's own degrees for its four corners must be where the
        // projection puts them: a file on another grid is refused.
        let degrees = |name: &str| file.shuffled_f32(&file.object(&[name])?, &[rows, columns]);
        let (lat, lon) = (degrees("lat")?, degrees("lon")?);
        for (row, column) in [
            (0, 0),
            (0, columns - 1),
            (rows - 1, 0),
            (rows - 1, columns - 1),
        ] {
            let at = row * columns + column;
            let (x, y) = projection.forward(f64::from(lon[at]), f64::from(lat[at]));
            // Sizes of a picture's side.
            #[allow(clippy::cast_precision_loss)]
            let off = (x - (west + column as f64 * dx)).hypot(y - (south + row as f64 * dy));
            if off.is_nan() || off > CORNER_TOLERANCE_M {
                return Err(format!(
                    "a corner of the grid lies {off:.0} m from where its own degrees put it"
                ));
            }
        }

        let raw = file.shuffled_i16(&rain.object, &[1, rows, columns])?;
        let step_of = |value: i16| -> u8 {
            let value = f64::from(value);
            // The marks are whole numbers a double holds exactly. A sum
            // below zero is no sum.
            #[allow(clippy::float_cmp)]
            if value == fill || value == missing || value < 0.0 {
                RADAR_UNSEEN
            } else {
                rain_step(value * scale * (3600.0 / f64::from(INTERVAL_S)))
            }
        };

        // A cell of the wanted grid takes the value of the point whose
        // cell its middle falls into; outside the rectangle it is not seen.
        // Sizes of a picture's side, and places checked to lie within it.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let value_at = |x: f64, y: f64| -> u8 {
            let (i, j) = (
                ((x - west) / dx + 0.5).floor(),
                ((y - south) / dy + 0.5).floor(),
            );
            if i >= 0.0 && j >= 0.0 && i < columns as f64 && j < rows as f64 {
                step_of(raw[j as usize * columns + i as usize])
            } else {
                RADAR_UNSEEN
            }
        };
        let mut cells = Vec::with_capacity(grid.cells());
        for row in 0..grid.rows {
            for column in 0..grid.columns {
                let (lon, lat) = grid.centre(column, row);
                let (x, y) = projection.forward(lon, lat);
                cells.push(value_at(x, y));
            }
        }
        Ok(RadarFrame {
            source: self.manifest.id.clone(),
            time: valid,
            interval_s: INTERVAL_S,
            grid: *grid,
            cells,
        })
    }
}

impl Default for GeosphereIncaRr {
    fn default() -> Self {
        Self::new()
    }
}

/// A variable of a frame: its object and its attributes.
struct Variable<'a> {
    /// Its name, for an error's words.
    name: String,
    object: Object<'a>,
    attributes: Vec<(String, Value)>,
}

impl Variable<'_> {
    fn has(&self, name: &str) -> bool {
        self.attributes
            .iter()
            .any(|(attribute, _)| attribute == name)
    }

    fn value(&self, name: &str) -> Result<&Value, String> {
        self.attributes
            .iter()
            .find(|(attribute, _)| attribute == name)
            .map(|(_, value)| value)
            .ok_or_else(|| format!("{} has no {name}", self.name))
    }

    fn text(&self, name: &str) -> Result<&str, String> {
        self.value(name)?
            .text()
            .ok_or_else(|| format!("{}'s {name} is not text", self.name))
    }

    fn number(&self, name: &str) -> Result<f64, String> {
        self.value(name)?
            .number()
            .filter(|number| number.is_finite())
            .ok_or_else(|| format!("{}'s {name} is not a number", self.name))
    }
}

impl RadarSource for GeosphereIncaRr {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn plan(&self, _ctx: &PlanCtx) -> Vec<Request> {
        vec![Request::new("list", list_url())]
    }

    fn parse(
        &self,
        request: &Request,
        body: &[u8],
        ctx: &PlanCtx,
        wanted: &RadarWanted,
    ) -> Result<ParsedRadar, SourceError> {
        if request.tag == "list" {
            return Self::list(body, ctx, wanted).map_err(SourceError::Malformed);
        }
        let named = run_in_tag(&request.tag)
            .ok_or_else(|| SourceError::UnknownRequest(request.tag.clone()))?;
        let frame = self
            .frame(named, body, &wanted.grid)
            .map_err(SourceError::Malformed)?;
        Ok(ParsedRadar {
            frames: vec![frame],
            ..ParsedRadar::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use venti_sources::Kind;

    use super::*;

    fn at(hour: u32, minute: u32) -> Timestamp {
        Utc.with_ymd_and_hms(2026, 11, 21, hour, minute, 0).unwrap()
    }

    fn wanted(after: Timestamp) -> RadarWanted {
        RadarWanted {
            after,
            grid: RadarGrid::over(&fixtures::area()).unwrap(),
        }
    }

    fn ctx(now: Timestamp) -> PlanCtx {
        PlanCtx {
            now,
            area: fixtures::area(),
        }
    }

    #[test]
    fn the_manifest_is_one_of_an_open_radar_source_within_the_shared_limit() {
        let manifest = GeosphereIncaRr::new().manifest().clone();
        assert_eq!(manifest.id.as_str(), "geosphere-inca-rr");
        assert_eq!(manifest.kinds, [Kind::Radar]);
        assert_eq!(manifest.cadence_seconds, 900);
        assert_eq!(
            manifest.licence.family(),
            venti_sources::LicenceFamily::Open
        );
        assert_eq!(manifest.max_response_bytes(), 2 * 1024 * 1024);
        // The service allows an address 5 requests a second and 240 an
        // hour, and the station adapter asks from the same address.
        let stations =
            Manifest::from_toml(include_str!("../../sources-geosphere/manifest.toml")).unwrap();
        assert_eq!(stations.id.as_str(), "geosphere-tawes");
        let per_hour = |manifest: &Manifest| manifest.rate_limit.per_hour.unwrap();
        assert!(per_hour(&manifest) + per_hour(&stations) <= 240);
        let per_second = |manifest: &Manifest| manifest.rate_limit.per_second.unwrap();
        assert!(per_second(&manifest) + per_second(&stations) <= 5);
        // A start asks for the list and the six runs the service holds.
        assert!(per_hour(&manifest) >= 4 * 7);
    }

    #[test]
    fn a_frames_address_names_the_run_its_place_and_the_widened_box() {
        assert_eq!(
            list_url(),
            "https://dataset.api.hub.geosphere.at/v1/grid/forecast/nowcast-v1-15min-1km/metadata"
        );
        // The first region's box is 46.3,9.4,48.5,17.2; the adapter asks
        // for a twentieth of a degree more on every side, and the
        // recording is of that request.
        assert_eq!(
            frame_url(at(12, 45), 1, &fixtures::area()),
            "https://dataset.api.hub.geosphere.at/v1/grid/forecast/nowcast-v1-15min-1km\
             ?parameters=rr&bbox=46.25,9.35,48.55,17.25&forecast_offset=1\
             &start=2026-11-21T12:45&end=2026-11-21T12:45&output_format=netcdf"
        );
        assert_eq!(run_in_tag(&frame_tag(at(13, 0))), Some(at(13, 0)));
        for not_a_frame in [
            "list",
            "frame/",
            "frame/26112112",
            "frame/2613411245",
            "frame/../../x",
        ] {
            assert_eq!(run_in_tag(not_a_frame), None, "{not_a_frame}");
        }
    }

    #[test]
    fn the_recorded_list_asks_for_the_runs_that_are_wanted_each_by_its_place() {
        let now = ctx(fixtures::recorded_at());

        // A server that holds the frames up to 12:30.
        let parsed = GeosphereIncaRr::list(fixtures::LIST, &now, &wanted(at(12, 30))).unwrap();
        let asked: Vec<&str> = parsed.follow_up.iter().map(|r| r.tag.as_str()).collect();
        assert_eq!(asked, ["frame/2611211245", "frame/2611211300"]);
        // The newest run is the list's first: place 0.
        let url = |at: usize| parsed.follow_up[at].url.as_str();
        assert!(url(0).contains("&forecast_offset=1&start=2026-11-21T12:45&"));
        assert!(url(1).contains("&forecast_offset=0&start=2026-11-21T13:00&"));
        assert_eq!(parsed.newest_listed, Some(at(13, 0)));
        assert!(parsed.skipped.is_empty());

        // A server that holds nothing gets the six runs the service has.
        let all = GeosphereIncaRr::list(fixtures::LIST, &now, &wanted(at(0, 0))).unwrap();
        assert_eq!(all.follow_up.len(), 6);
        assert_eq!(all.follow_up[0].tag, "frame/2611211145");
        assert!(all.follow_up[0].url.contains("&forecast_offset=5&"));
        // One that holds the newest asks for nothing.
        let none = GeosphereIncaRr::list(fixtures::LIST, &now, &wanted(at(13, 0))).unwrap();
        assert!(none.follow_up.is_empty());
        assert_eq!(none.newest_listed, Some(at(13, 0)));
        // A run of after the run's own time is not asked for.
        let early = GeosphereIncaRr::list(fixtures::LIST, &ctx(at(12, 50)), &wanted(at(12, 30)));
        assert_eq!(early.unwrap().follow_up.len(), 1);
    }

    #[test]
    fn a_list_names_no_address_of_its_own() {
        let list = br#"{"available_forecast_reftimes": [
            "2026-11-21T13:00+00:00",
            "https://example.org/2026-11-21T12:45+00:00",
            "2026-11-21T12:45+00:00&bbox=0,0,90,180",
            "2026-11-21T14:30+02:00"],
            "url": "https://example.org/frames"}"#;

        let parsed = GeosphereIncaRr::list(list, &ctx(at(13, 19)), &wanted(at(11, 0))).unwrap();

        // The two that are times, at the addresses the adapter makes: the
        // last is 12:30 UTC, written on another clock.
        let urls: Vec<&str> = parsed.follow_up.iter().map(|r| r.url.as_str()).collect();
        assert_eq!(
            urls,
            [
                frame_url(at(12, 30), 3, &fixtures::area()),
                frame_url(at(13, 0), 0, &fixtures::area())
            ]
        );
        assert_eq!(parsed.skipped.len(), 2);

        // What is no list is said to be none.
        let said = GeosphereIncaRr::list(b"<html>", &ctx(at(13, 19)), &wanted(at(11, 0)));
        assert!(said.unwrap_err().contains("no list of runs"));
        let many = format!(
            r#"{{"available_forecast_reftimes": [{}]}}"#,
            vec![r#""2026-11-21T13:00+00:00""#; 65].join(",")
        );
        let said = GeosphereIncaRr::list(many.as_bytes(), &ctx(at(13, 19)), &wanted(at(11, 0)));
        assert!(said.unwrap_err().contains("names 65 runs"));
    }

    #[test]
    fn a_recorded_frame_is_laid_on_the_grid_in_steps() {
        let adapter = GeosphereIncaRr::new();
        let grid = RadarGrid::over(&fixtures::area()).unwrap();
        let (time, body) = fixtures::frames()[0];

        let frame = adapter.frame(time, body, &grid).unwrap();

        assert_eq!(frame.time, at(12, 45));
        assert_eq!(frame.interval_s, 900);
        assert_eq!(frame.cells.len(), 624 * 261);
        // Vienna, which no German radar sees, has a value.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let column = ((16.37 - grid.west) / grid.step_deg) as u32;
        let row = (0..grid.rows)
            .find(|row| grid.centre(0, *row).1 < 48.21)
            .unwrap();
        assert_ne!(
            frame.cells[(row * grid.columns + column) as usize],
            RADAR_UNSEEN
        );
        // Rain over a ninth of the box that afternoon, none of it in the
        // two highest steps; and no cell without a value.
        let mut steps = [0usize; 8];
        for cell in &frame.cells {
            steps[usize::from(*cell)] += 1;
        }
        assert_eq!(steps, [143_965, 4470, 11_305, 3088, 36, 0, 0, 0]);
        // That is the widened box at work. The service gives the rows of
        // INCA's grid that reach into the box it is asked for. A row of
        // that grid is a straight line on INCA's map and bends south on a
        // map of degrees, least at the grid's meridian of 13.33 E: asked
        // for the region's own box, the rectangle ended north of the
        // map's last row there, and 107 cells of that row, from 12.66 E
        // to 14 E, had no value (the recording of 12:39 UTC on the same
        // day, which this one replaced). The map's last row is seen now.
        let last_row = &frame.cells[260 * 624..];
        assert!(last_row.iter().all(|cell| *cell != RADAR_UNSEEN));
        assert!((12.66..12.67).contains(&grid.centre(261, 260).0));
        assert!((13.99..14.0).contains(&grid.centre(367, 260).0));
    }

    #[test]
    fn a_frame_is_refused_when_it_is_not_what_was_asked_for() {
        let adapter = GeosphereIncaRr::new();
        let grid = RadarGrid::over(&fixtures::area()).unwrap();
        let (time, body) = fixtures::frames()[0];
        assert_eq!(time, at(12, 45));

        // A new run arrived between the list and the frame: the place
        // that was the run of 13:00 now gives another. Here, the frame of
        // 12:45 under the name of 13:00.
        let said = adapter.frame(at(13, 0), body, &grid).unwrap_err();
        assert!(
            said.contains(
                "is of 2026-11-21 12:45 UTC, and was asked for as the run of 2026-11-21 13:00"
            ),
            "{said}"
        );

        // An error page, and a frame cut off after four kilobytes.
        let said = adapter.frame(time, b"<html>404</html>", &grid).unwrap_err();
        assert!(said.contains("not an HDF5 file"), "{said}");
        let said = adapter.frame(time, &body[..4096], &grid).unwrap_err();
        assert!(said.contains("cut short"), "{said}");
    }

    /// Changes a recorded frame in memory: the first `find` after the
    /// bytes `after`, replaced by `with` of the same length.
    fn changed(body: &[u8], after: &[u8], find: &[u8], with: &[u8]) -> Vec<u8> {
        assert_eq!(find.len(), with.len());
        let position = |from: usize, bytes: &[u8]| {
            from + body[from..]
                .windows(bytes.len())
                .position(|window| window == bytes)
                .expect("the frame holds the bytes")
        };
        let at = position(position(0, after), find);
        let mut changed = body.to_vec();
        changed[at..at + with.len()].copy_from_slice(with);
        changed
    }

    #[test]
    fn a_frame_that_says_something_else_of_itself_is_refused() {
        let adapter = GeosphereIncaRr::new();
        let grid = RadarGrid::over(&fixtures::area()).unwrap();
        let (time, body) = fixtures::frames()[0];
        let refused = |body: &[u8]| adapter.frame(time, body, &grid).unwrap_err();

        // Another unit: a rate is no sum of a quarter of an hour.
        let said = refused(&changed(
            body,
            b"precipitation_amount",
            b"kg m-2",
            b"mm h-1",
        ));
        assert!(
            said.contains("not a precipitation amount in kg m-2"),
            "{said}"
        );
        // Another grid.
        let said = refused(&changed(body, b"crs\0", b"EPSG:31287", b"EPSG:03416"));
        assert!(
            said.contains("does not say that its grid is EPSG:31287"),
            "{said}"
        );
        // A step of the forecast: a lead time of a quarter of an hour.
        // The value stands where the file's layout of `leadtime` says.
        let mut later = body.to_vec();
        later[40_665..40_673].copy_from_slice(&0.25f64.to_le_bytes());
        let said = refused(&later);
        assert!(said.contains("a forecast 0.25 hours ahead"), "{said}");
        // The whole rectangle a kilometre further east than its degrees
        // say: every `x` a thousand metres more.
        let mut moved = body.to_vec();
        for point in 0..608 {
            let at = 48_447 + point * 4;
            let value = i32::from_le_bytes(moved[at..at + 4].try_into().unwrap()) + 1000;
            moved[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        let said = refused(&moved);
        assert!(said.contains("a corner of the grid lies 1000 m"), "{said}");
    }
}
