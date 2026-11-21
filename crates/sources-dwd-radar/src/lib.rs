//! Adapter for the rain radar composite "RY" of the Deutscher Wetterdienst.
//!
//! RY is the five-minute composite of RADOLAN, DWD's routine for rain from
//! radar: the precipitation scans of its 17 radars, checked for quality and
//! turned into a rain rate with DWD's refined Z-R relations, joined into one
//! picture of Germany of 1100 by 1200 pixels of one kilometre. A file is
//! one such picture. DWD publishes one every five minutes on its open-data
//! server, some twenty seconds after the five minutes it is of, and keeps
//! two days of them.
//!
//! Facts read on 2026-11-21, on DWD's own pages and in the recorded frames:
//!
//! - **The product** ("RADOLAN/RADVOR: Beschreibung des Kompositformats",
//!   version 2.6 of 2025-01-14): RY is "qualitätsgeprüfte Radardaten nach
//!   Anwendung der verfeinerten Z-R-Beziehungen in Niederschlagshöhen
//!   umgerechnet", every five minutes. Since the migration that began in
//!   2023 the national composites come as HDF5 by the ODIM_H5 2.3
//!   convention of EUMETNET's OPERA programme, on the WGS84 ellipsoid, and
//!   are 1200 km by 1100 km.
//! - **The values** (a frame's `/dataset1/data1/what`): the quantity is
//!   `RATE`, which ODIM defines as a rain rate; a raw value times `gain`
//!   0.01 plus `offset` 0 is that rate; `nodata` 65535 marks a place no
//!   radar saw and `undetect` 0 one that was seen and had no echo. The
//!   adapter takes the rate as millimetres an hour. ODIM's table gives the
//!   unit of `RATE` so, and the frames agree with it: their smallest value
//!   that is not zero is 12, which is 0.12 mm/h, the 0.01 mm in five
//!   minutes that was the old format's smallest step. DWD's own text still
//!   calls the values "Niederschlagshöhen", amounts; that the unit is
//!   mm/h and not mm in five minutes is marked *to verify* in
//!   `docs/data-sources.md`.
//! - **The time** (`/what`, `/dataset1/what`): a frame named `…2611211140…`
//!   has the nominal time 11:40 UTC and covers 11:35 to 11:40. The adapter
//!   dates a frame by the end of its interval.
//! - **The grid** (`/where`): a polar stereographic projection; see
//!   [`projection`]. The first row is the northernmost, as ODIM says.
//! - **The range**: what a frame holds is what the 17 German radars see,
//!   150 km around each. In the first region that is Bavaria, the north of
//!   Tirol and Vorarlberg and little beyond; over the main ridge of the
//!   Alps the beams are blocked by the mountains in front of it.
//!
//! A run takes two steps: the list of the frames DWD has, then each frame
//! the server does not hold yet. The adapter makes a frame's address from
//! the digits of its time alone, never from what the list says.

pub mod fixtures;
mod hdf5;
mod projection;

use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use venti_domain::{RADAR_UNSEEN, RadarFrame, RadarGrid, Timestamp, rain_step};
use venti_sources::{
    Manifest, ParsedRadar, PlanCtx, RadarSource, RadarWanted, Request, Skipped, SourceError,
};

use crate::hdf5::{File, Value};
use crate::projection::{Placement, PolarStereographic};

/// The folder of RY on DWD's open-data server.
const BASE: &str = "https://opendata.dwd.de/weather/radar/radolan/ry/";

/// How a frame's file begins and ends; between the two stand the ten
/// digits of its time, `YYMMDDhhmm`.
const FILE_PREFIX: &str = "raa01-ry_10000-";
const FILE_SUFFIX: &str = "-dwd---bin.hdf5";

/// The most frames one run asks for: two hours of them and one more. The
/// server keeps no more; a list that names more gets its newest.
const MAX_FRAMES_PER_RUN: usize = 25;

/// The longest interval a frame may cover: an hour. RY covers five
/// minutes; anything much longer is another product.
const MAX_INTERVAL_S: i64 = 3600;

/// The address of the list of frames: the folder itself.
pub fn list_url() -> String {
    BASE.to_owned()
}

/// The address of the frame whose interval ends at `time`.
pub fn frame_url(time: Timestamp) -> String {
    format!(
        "{BASE}{FILE_PREFIX}{}{FILE_SUFFIX}",
        time.format("%y%m%d%H%M")
    )
}

/// The time in a frame's file name, if the name is one of a frame: ten
/// digits between the product's prefix and suffix, read as a time of this
/// century.
fn time_in_name(name: &str) -> Option<Timestamp> {
    let digits = name.strip_prefix(FILE_PREFIX)?.strip_suffix(FILE_SUFFIX)?;
    if digits.len() != 10 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let naive = NaiveDateTime::parse_from_str(&format!("20{digits}"), "%Y%m%d%H%M").ok()?;
    Some(Utc.from_utc_datetime(&naive))
}

/// DWD's radar composite RY as a source of rain radar.
#[derive(Debug, Clone)]
pub struct DwdRadarRy {
    manifest: Manifest,
}

impl DwdRadarRy {
    /// Creates the adapter.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn new() -> Self {
        let manifest = Manifest::from_toml(include_str!("../manifest.toml"))
            .expect("the manifest compiled into venti-sources-dwd-radar is valid");
        Self { manifest }
    }

    /// Reads the list of frames: asks for those that are wanted, the
    /// newest [`MAX_FRAMES_PER_RUN`] of them.
    fn list(body: &[u8], ctx: &PlanCtx, wanted: &RadarWanted) -> ParsedRadar {
        let text = String::from_utf8_lossy(body);
        let mut times: Vec<Timestamp> = Vec::new();
        let mut skipped = Vec::new();
        // The list is a page of links, one to each file. Only the name in
        // a link is read, and only for its ten digits.
        for link in text.split("href=\"").skip(1) {
            let Some(name) = link.split('"').next() else {
                continue;
            };
            if !name.starts_with(FILE_PREFIX) {
                continue;
            }
            match time_in_name(name) {
                Some(time) => times.push(time),
                // "latest" is the newest frame under a second name.
                None if name == format!("{FILE_PREFIX}latest{FILE_SUFFIX}") => {}
                None => skipped.push(Skipped::new(name, "not the name of a frame")),
            }
        }
        times.sort_unstable();
        times.dedup();
        let newest_listed = times.last().copied();
        let mut asked: Vec<Timestamp> = times
            .into_iter()
            .filter(|time| *time > wanted.after && *time <= ctx.now + chrono::Duration::minutes(1))
            .collect();
        let surplus = asked.len().saturating_sub(MAX_FRAMES_PER_RUN);
        asked.drain(..surplus);
        ParsedRadar {
            frames: Vec::new(),
            newest_listed,
            follow_up: asked
                .into_iter()
                .map(|time| {
                    Request::new(
                        format!("frame/{}", time.format("%y%m%d%H%M")),
                        frame_url(time),
                    )
                })
                .collect(),
            skipped,
        }
    }

    /// Reads one frame and lays it on the grid that is wanted.
    fn frame(&self, named: Timestamp, body: &[u8], grid: &RadarGrid) -> Result<RadarFrame, String> {
        let file = File::open(body)?;
        let attributes = |path: &[&str]| -> Result<Attributes, String> {
            Ok(Attributes {
                of: format!("/{}", path.join("/")),
                values: file.attributes(&file.object(path)?)?,
            })
        };

        // What the file is, and of when.
        let what = attributes(&["what"])?;
        if what.text("object")? != "COMP" {
            return Err("the file is not a composite".to_owned());
        }
        let nominal = moment(what.text("date")?, what.text("time")?)?;
        let product = attributes(&["dataset1", "what"])?;
        if product.text("prodname")? != "RY" {
            return Err(format!(
                "the file is the product {:?}, not RY",
                product.text("prodname")?
            ));
        }
        let start = moment(product.text("startdate")?, product.text("starttime")?)?;
        let end = moment(product.text("enddate")?, product.text("endtime")?)?;
        let interval_s = (end - start).num_seconds();
        if end != named || nominal != named {
            return Err(format!(
                "the file is of {}, and was asked for as the frame of {}",
                end.format("%Y-%m-%d %H:%M UTC"),
                named.format("%Y-%m-%d %H:%M UTC")
            ));
        }
        if !(1..=MAX_INTERVAL_S).contains(&interval_s) {
            return Err(format!("the frame covers {interval_s} seconds"));
        }

        // What its values mean.
        let values = attributes(&["dataset1", "data1", "what"])?;
        if values.text("quantity")? != "RATE" {
            return Err(format!(
                "the values are {:?}, not a rain rate (RATE)",
                values.text("quantity")?
            ));
        }
        let (gain, offset) = (values.number("gain")?, values.number("offset")?);
        let (nodata, undetect) = (values.number("nodata")?, values.number("undetect")?);
        if !(gain > 0.0 && gain <= 10.0 && offset.is_finite()) {
            return Err(format!("a gain of {gain} and an offset of {offset}"));
        }

        // Where its grid lies.
        let place = attributes(&["where"])?;
        let size = |name: &str| -> Result<usize, String> {
            let value = place.number(name)?;
            // A whole number in the range of a picture's side.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            if (1.0..=4096.0).contains(&value) && value.fract() == 0.0 {
                Ok(value as usize)
            } else {
                Err(format!("{name} is {value}"))
            }
        };
        let (columns, rows) = (size("xsize")?, size("ysize")?);
        let corner = |name: &str| -> Result<(f64, f64), String> {
            Ok((
                place.number(&format!("{name}_lon"))?,
                place.number(&format!("{name}_lat"))?,
            ))
        };
        let placement = Placement::new(
            PolarStereographic::from_proj(place.text("projdef")?)?,
            (columns, rows),
            (place.number("xscale")?, place.number("yscale")?),
            corner("UL")?,
            [
                ("upper right", corner("UR")?, (1.0, 0.0)),
                ("lower left", corner("LL")?, (0.0, 1.0)),
                ("lower right", corner("LR")?, (1.0, 1.0)),
            ],
        )?;

        let raw = file.grid_u16(&file.object(&["dataset1", "data1", "data"])?, rows, columns)?;
        // Each raw value's step, worked out once for the values there are.
        let step_of = |value: u16| -> u8 {
            let value = f64::from(value);
            // The marks are whole numbers a double holds exactly.
            #[allow(clippy::float_cmp)]
            if value == nodata {
                RADAR_UNSEEN
            } else if value == undetect {
                venti_domain::RADAR_DRY
            } else {
                rain_step(offset + gain * value)
            }
        };

        // A cell of the wanted grid takes the value of the pixel its
        // middle falls into. A parallel of the wanted grid is a circle on
        // the composite's map, so its radius is worked out once a row.
        let mut cells = Vec::with_capacity(grid.cells());
        for row in 0..grid.rows {
            let (_, lat) = grid.centre(0, row);
            let radius = placement.projection.radius(lat);
            for column in 0..grid.columns {
                let (lon, _) = grid.centre(column, row);
                let cell = placement
                    .pixel(placement.projection.at(radius, lon))
                    .map_or(RADAR_UNSEEN, |(x, y)| step_of(raw[y * columns + x]));
                cells.push(cell);
            }
        }
        Ok(RadarFrame {
            source: self.manifest.id.clone(),
            time: end,
            // Checked above to be between a second and an hour.
            interval_s: u32::try_from(interval_s).unwrap_or(u32::MAX),
            grid: *grid,
            cells,
        })
    }
}

impl Default for DwdRadarRy {
    fn default() -> Self {
        Self::new()
    }
}

/// The attributes of one group of a frame.
struct Attributes {
    /// The group, for an error's words.
    of: String,
    values: Vec<(String, Value)>,
}

impl Attributes {
    fn value(&self, name: &str) -> Result<&Value, String> {
        self.values
            .iter()
            .find(|(attribute, _)| attribute == name)
            .map(|(_, value)| value)
            .ok_or_else(|| format!("{} has no {name}", self.of))
    }

    fn text(&self, name: &str) -> Result<&str, String> {
        self.value(name)?
            .text()
            .ok_or_else(|| format!("{}/{name} is not text", self.of))
    }

    fn number(&self, name: &str) -> Result<f64, String> {
        self.value(name)?
            .number()
            .filter(|number| number.is_finite())
            .ok_or_else(|| format!("{}/{name} is not a number", self.of))
    }
}

/// A date `YYYYMMDD` and a time `HHmmss` of ODIM, as a moment in UTC.
fn moment(date: &str, time: &str) -> Result<Timestamp, String> {
    let read = || {
        let date = NaiveDate::parse_from_str(date, "%Y%m%d").ok()?;
        let time = NaiveTime::parse_from_str(time, "%H%M%S").ok()?;
        Some(Utc.from_utc_datetime(&date.and_time(time)))
    };
    read().ok_or_else(|| format!("{date:?} {time:?} is no date and time"))
}

impl RadarSource for DwdRadarRy {
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
            return Ok(Self::list(body, ctx, wanted));
        }
        let named = request
            .tag
            .strip_prefix("frame/")
            .and_then(|digits| time_in_name(&format!("{FILE_PREFIX}{digits}{FILE_SUFFIX}")))
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

/// Lets the crate's tests reach a group of a recorded frame.
#[cfg(test)]
pub(crate) fn object<'a>(file: &File<'a>, path: &[&str]) -> hdf5::Object<'a> {
    file.object(path).expect("the recorded frame has the group")
}

#[cfg(test)]
mod tests {
    use venti_domain::BoundingBox;
    use venti_sources::Kind;

    use super::*;

    fn at(day: u32, hour: u32, minute: u32) -> Timestamp {
        Utc.with_ymd_and_hms(2026, 11, day, hour, minute, 0)
            .unwrap()
    }

    fn first_region() -> BoundingBox {
        BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap()
    }

    fn wanted(after: Timestamp) -> RadarWanted {
        RadarWanted {
            after,
            grid: RadarGrid::over(&first_region()).unwrap(),
        }
    }

    #[test]
    fn the_manifest_is_one_of_an_open_radar_source() {
        let manifest = DwdRadarRy::new().manifest().clone();
        assert_eq!(manifest.id.as_str(), "dwd-radar-ry");
        assert_eq!(manifest.kinds, [Kind::Radar]);
        assert_eq!(manifest.cadence_seconds, 300);
        assert_eq!(
            manifest.licence.family(),
            venti_sources::LicenceFamily::Open
        );
        assert_eq!(manifest.max_response_bytes(), 3 * 1024 * 1024);
    }

    #[test]
    fn a_frames_address_is_made_of_its_time_and_read_back_from_a_name() {
        assert_eq!(
            frame_url(at(21, 11, 40)),
            "https://opendata.dwd.de/weather/radar/radolan/ry/raa01-ry_10000-2611211140-dwd---bin.hdf5"
        );
        let name = |text: &str| time_in_name(text);
        assert_eq!(
            name("raa01-ry_10000-2611211140-dwd---bin.hdf5"),
            Some(at(21, 11, 40))
        );
        for not_a_frame in [
            "raa01-ry_10000-latest-dwd---bin.hdf5",
            "raa01-ry_10000-261121114-dwd---bin.hdf5",
            "raa01-ry_10000-26112111400-dwd---bin.hdf5",
            "raa01-ry_10000-2611211140-dwd---bin.hdf5.tmp",
            "raa01-ry_10000-2613411140-dwd---bin.hdf5",
            "raa01-ry_10000-../../etc-dwd---bin.hdf5",
            "raa01-rw_10000-2611211140-dwd---bin.hdf5",
        ] {
            assert_eq!(name(not_a_frame), None, "{not_a_frame}");
        }
    }

    #[test]
    fn the_recorded_list_asks_for_the_frames_that_are_wanted() {
        let list = fixtures::LIST;
        let ctx = PlanCtx {
            now: fixtures::recorded_at(),
            area: first_region(),
        };

        // A server that holds the frames up to 11:30.
        let parsed = DwdRadarRy::list(list, &ctx, &wanted(at(21, 11, 30)));
        let asked: Vec<&str> = parsed.follow_up.iter().map(|r| r.tag.as_str()).collect();
        assert_eq!(asked, ["frame/2611211135", "frame/2611211140"]);
        assert_eq!(parsed.follow_up[1].url, frame_url(at(21, 11, 40)));
        assert_eq!(parsed.newest_listed, Some(at(21, 11, 40)));
        // 576 frames and "latest", which is no frame of its own.
        assert!(parsed.skipped.is_empty(), "{:?}", parsed.skipped);

        // A server that holds nothing gets two hours and one frame, not
        // the two days the list has.
        let all = DwdRadarRy::list(list, &ctx, &wanted(at(18, 0, 0)));
        assert_eq!(all.follow_up.len(), MAX_FRAMES_PER_RUN);
        assert_eq!(all.follow_up[0].tag, "frame/2611210940");
        // One that holds the newest asks for nothing.
        let none = DwdRadarRy::list(list, &ctx, &wanted(at(21, 11, 40)));
        assert!(none.follow_up.is_empty());
        assert_eq!(none.newest_listed, Some(at(21, 11, 40)));
    }

    #[test]
    fn a_list_names_no_address_of_its_own() {
        // A list that somebody else wrote: links that lead elsewhere, a
        // name with a path in it, a frame of next year.
        let list = br#"<a href="https://example.org/raa01-ry_10000-2611211140-dwd---bin.hdf5">x</a>
            <a href="raa01-ry_10000-2611211135-dwd---bin.hdf5?x=https://example.org">x</a>
            <a href="raa01-ry_10000-../secret-dwd---bin.hdf5">x</a>
            <a href="raa01-ry_10000-2711211140-dwd---bin.hdf5">x</a>
            <a href="raa01-ry_10000-2611211130-dwd---bin.hdf5">x</a>"#;
        let ctx = PlanCtx {
            now: at(21, 11, 44),
            area: first_region(),
        };

        let parsed = DwdRadarRy::list(list, &ctx, &wanted(at(21, 11, 0)));

        // The one name that is a frame's, at the address the adapter makes.
        let urls: Vec<&str> = parsed.follow_up.iter().map(|r| r.url.as_str()).collect();
        assert_eq!(urls, [frame_url(at(21, 11, 30))]);
        assert_eq!(parsed.skipped.len(), 2, "{:?}", parsed.skipped);
    }

    #[test]
    fn a_recorded_frame_says_what_it_is_in_its_own_attributes() {
        let (_, body) = fixtures::frames()[1];
        let file = File::open(body).unwrap();
        let attributes = |path: &[&str]| file.attributes(&object(&file, path)).unwrap();
        let text = |values: &[(String, Value)], name: &str| {
            values
                .iter()
                .find(|(attribute, _)| attribute == name)
                .and_then(|(_, value)| value.text().map(str::to_owned))
                .unwrap_or_else(|| panic!("no {name}"))
        };

        let root = attributes(&[]);
        assert_eq!(text(&root, "Conventions"), "ODIM_H5/V2_3");
        let what = attributes(&["what"]);
        assert_eq!(text(&what, "object"), "COMP");
        assert_eq!(text(&what, "date"), "20261121");
        assert_eq!(text(&what, "time"), "114000");
        assert_eq!(
            text(&what, "source"),
            "ORG:78,CTY:616,CMT:Deutscher Wetterdienst radolan@dwd.de"
        );
        let product = attributes(&["dataset1", "what"]);
        assert_eq!(text(&product, "prodname"), "RY");
        assert_eq!(text(&product, "starttime"), "113500");
        assert_eq!(text(&product, "endtime"), "114000");
        let how = attributes(&["how"]);
        // The 17 radars of the composite.
        assert_eq!(text(&how, "nodes").split(',').count(), 17);

        // The thirteen attributes of /where are kept outside its header.
        let place = attributes(&["where"]);
        assert_eq!(place.len(), 13);
        assert_eq!(
            text(&place, "projdef"),
            "+proj=stere +lat_0=90 +lat_ts=60 +lon_0=10 +a=6378137 +b=6356752.3142451802 \
             +no_defs +x_0=543196.83521776402 +y_0=3622588.8619310018 +units=m"
        );
        let number = |name: &str| {
            place
                .iter()
                .find(|(attribute, _)| attribute == name)
                .and_then(|(_, value)| value.number())
                .unwrap()
        };
        assert_eq!((number("xsize"), number("ysize")), (1100.0, 1200.0));
        assert_eq!((number("xscale"), number("yscale")), (1000.0, 1000.0));
        assert!((number("UL_lon") - 1.463_301_51).abs() < 1e-9);
        assert!((number("LR_lat") - 45.684_605_78).abs() < 1e-9);

        let values = attributes(&["dataset1", "data1", "what"]);
        assert_eq!(text(&values, "quantity"), "RATE");
        let raw = file
            .grid_u16(&object(&file, &["dataset1", "data1", "data"]), 1200, 1100)
            .unwrap();
        assert_eq!(raw.len(), 1_320_000);
        // Nearly half of the picture no radar sees; the smallest rate
        // that is not zero is 12 hundredths of a millimetre an hour.
        assert_eq!(raw.iter().filter(|v| **v == 65535).count(), 627_073);
        assert_eq!(raw.iter().filter(|v| **v != 0).min(), Some(&12));
        // Asked for as a picture of another size, it is refused.
        let said = file
            .grid_u16(&object(&file, &["dataset1", "data1", "data"]), 900, 900)
            .unwrap_err();
        assert!(said.contains("not 900 rows of 900"), "{said}");
    }

    #[test]
    fn the_edge_of_what_the_radars_see_lies_where_their_range_ends() {
        // Isen's radar stands at 48°10'28.9" N 12°06'06.4" E (DWD's
        // format description) and has no neighbour to its south-east. On
        // its own meridian the first region's grid is seen to 150 km south
        // of it and no further: 46.83° N.
        let adapter = DwdRadarRy::new();
        let grid = RadarGrid::over(&first_region()).unwrap();
        let (time, body) = fixtures::frames()[1];
        let frame = adapter.frame(time, body, &grid).unwrap();

        let isen = (
            12.0 + 6.0 / 60.0 + 6.4 / 3600.0,
            48.0 + 10.0 / 60.0 + 28.9 / 3600.0,
        );
        // The column of Isen's meridian, and the rows going south.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let column = ((isen.0 - grid.west) / grid.step_deg) as u32;
        let seen_to = (0..grid.rows)
            .filter(|row| frame.cells[(row * grid.columns + column) as usize] != RADAR_UNSEEN)
            .map(|row| grid.centre(column, row).1)
            .fold(f64::INFINITY, f64::min);
        // A degree of latitude is 111.2 km there; a cell is a kilometre.
        let km_south = (isen.1 - seen_to) * 111.2;
        assert!(
            (147.5..151.0).contains(&km_south),
            "{km_south} km, to {seen_to}"
        );

        // Vienna is on the composite's grid and beyond every German
        // radar: nothing is seen there, which is not the same as dry.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let vienna = ((16.37 - grid.west) / grid.step_deg) as u32;
        assert!(
            (0..grid.rows)
                .all(|row| { frame.cells[(row * grid.columns + vienna) as usize] == RADAR_UNSEEN })
        );
    }

    #[test]
    fn a_frame_is_refused_when_it_is_not_what_was_asked_for() {
        let adapter = DwdRadarRy::new();
        let grid = RadarGrid::over(&first_region()).unwrap();
        let (time, body) = fixtures::frames()[0];
        assert_eq!(time, at(21, 11, 35));

        // The frame of 11:35 under the name of 11:40.
        let said = adapter.frame(at(21, 11, 40), body, &grid).unwrap_err();
        assert!(
            said.contains("was asked for as the frame of 2026-11-21 11:40"),
            "{said}"
        );

        // An error page, and a frame cut off after four kilobytes.
        let said = adapter
            .frame(time, fixtures::NOT_A_FRAME, &grid)
            .unwrap_err();
        assert!(said.contains("not an HDF5 file"), "{said}");
        let said = adapter.frame(time, fixtures::CUT_SHORT, &grid).unwrap_err();
        assert!(said.contains("cut short"), "{said}");
    }

    /// Changes a recorded frame in memory: the first `find` after the
    /// attribute's name `after`, replaced by `with` of the same length.
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
        let adapter = DwdRadarRy::new();
        let grid = RadarGrid::over(&first_region()).unwrap();
        let (time, body) = fixtures::frames()[0];
        let refused = |body: &[u8]| adapter.frame(time, body, &grid).unwrap_err();

        // Another quantity: a reflectivity is no rain rate.
        let said = refused(&changed(body, b"quantity\0", b"RATE\0", b"DBZH\0"));
        assert!(said.contains("not a rain rate"), "{said}");
        // Another product of the same family.
        let said = refused(&changed(body, b"prodname\0", b"RY\0", b"RW\0"));
        assert!(said.contains("not RY"), "{said}");
        // Another projection.
        let said = refused(&changed(body, b"projdef\0", b"+proj=stere", b"+proj=merc "));
        assert!(said.contains("not stereographic"), "{said}");
        // Another meridian: the corners no longer fit the grid.
        let said = refused(&changed(body, b"projdef\0", b"+lon_0=10 ", b"+lon_0=11 "));
        assert!(said.contains("corner lies"), "{said}");
        // A frame that says it ends an hour and five minutes after it began.
        let said = refused(&changed(body, b"starttime\0", b"113000\0", b"103000\0"));
        assert!(said.contains("covers 3900 seconds"), "{said}");
    }

    #[test]
    fn data_that_unpacks_to_more_than_the_picture_holds_is_refused() {
        use std::io::Write;

        let adapter = DwdRadarRy::new();
        let grid = RadarGrid::over(&first_region()).unwrap();
        let (time, body) = fixtures::frames()[0];

        // The frame with its data replaced by sixteen megabytes of zeros,
        // packed, appended to the file; the chunk's entry in its index
        // points there.
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&vec![0u8; 16 * 1024 * 1024]).unwrap();
        let bomb = encoder.finish().unwrap();
        let mut file = body.to_vec();
        let tree = file
            .windows(4)
            .position(|window| window == b"TREE")
            .expect("the frame has the chunks' index");
        // After the signature: four bytes of the node, two neighbours,
        // then the chunk's packed size, eight bytes of flags and offsets
        // times three, and its address.
        let size_at = tree + 4 + 4 + 16;
        let address_at = size_at + 4 + 4 + 24;
        let end = file.len() as u64;
        file[size_at..size_at + 4].copy_from_slice(&(bomb.len() as u32).to_le_bytes());
        file[address_at..address_at + 8].copy_from_slice(&end.to_le_bytes());
        file.extend(&bomb);
        // The file's own word for its size, in the superblock.
        let new_end = file.len() as u64;
        file[40..48].copy_from_slice(&new_end.to_le_bytes());

        let said = adapter.frame(time, &file, &grid).unwrap_err();
        assert!(
            said.contains("and more, and its dataset holds 2640000"),
            "{said}"
        );
    }
}
