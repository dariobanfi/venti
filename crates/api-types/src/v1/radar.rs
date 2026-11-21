use serde::{Deserialize, Serialize};
use venti_domain::{SourceId, Timestamp};

use super::Attribution;

/// The path of the list of radar frames.
pub const RADAR_PATH: &str = "/v1/radar";

/// The path of one frame's picture: a source's frame of a moment, as a
/// PNG. The moment is written as in a file's name, `20261121T1140Z`.
pub fn radar_frame_path(source: &SourceId, time: Timestamp) -> String {
    format!("{RADAR_PATH}/{source}/{}.png", format_radar_time(time))
}

/// A frame's moment as it stands in its path: `20261121T1140Z`, to the
/// minute, in UTC.
pub fn format_radar_time(time: Timestamp) -> String {
    time.format("%Y%m%dT%H%MZ").to_string()
}

/// Reads a frame's moment out of its path. `None` for anything but the
/// fourteen characters [`format_radar_time`] writes.
pub fn parse_radar_time(text: &str) -> Option<Timestamp> {
    let shape = text.len() == 14
        && text.bytes().enumerate().all(|(at, byte)| match at {
            8 => byte == b'T',
            13 => byte == b'Z',
            _ => byte.is_ascii_digit(),
        });
    if !shape {
        return None;
    }
    chrono::NaiveDateTime::parse_from_str(text, "%Y%m%dT%H%MZ")
        .ok()
        .map(|naive| naive.and_utc())
}

/// The answer of `GET /v1/radar`: the rain radar frames the server holds,
/// in layers by source.
///
/// A frame is a picture. Its list is here; the picture itself is at each
/// frame's `path`, a PNG whose pixels are not colours but steps: see
/// [`RadarResponse::steps_mm_h`].
///
/// The layers are never merged: two radar sources are two pictures, each
/// under its own licence and with its own credit, and a client that draws
/// both draws two layers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RadarResponse {
    /// The server's clock when it answered. A frame's age is best counted
    /// against this, not against the clock of the device.
    pub time: Timestamp,
    /// From how many millimetres an hour each step of rain begins, the
    /// first step first. A pixel of a frame's picture has the value 0
    /// where the radar saw no rain, and the number of its step, from 1,
    /// where it saw some.
    pub steps_mm_h: Vec<f64>,
    /// The value of a pixel the radar did not see: beyond its range,
    /// behind a mountain, outside the product. It says nothing about rain
    /// there, and a client draws it differently from "no rain".
    pub unseen: u8,
    /// One layer for each radar source the server collects, sorted by
    /// source. A source the server holds no frame of is a layer without
    /// frames.
    pub layers: Vec<RadarLayer>,
    /// The credit for every source that has a layer in the answer.
    pub attributions: Vec<Attribution>,
}

/// The frames of one radar source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RadarLayer {
    /// The source. Its credit is in the response's attributions.
    pub source: SourceId,
    /// The western edge of every picture of this layer, degrees of
    /// longitude.
    pub west: f64,
    /// The southern edge, degrees of latitude.
    pub south: f64,
    /// The eastern edge.
    pub east: f64,
    /// The northern edge.
    pub north: f64,
    /// How many pixels wide a picture is.
    pub columns: u32,
    /// How many pixels high. The pixels are square in Web Mercator, the
    /// projection web maps are drawn in: a picture lies right on such a
    /// map when its four corners are pinned to the four edges above.
    pub rows: u32,
    /// How often the source publishes a frame, in seconds.
    pub cadence_seconds: u32,
    /// The frames the server holds, the oldest first.
    pub frames: Vec<RadarFrameRef>,
}

/// One frame of a radar layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RadarFrameRef {
    /// The end of the interval the frame is of. Write it, and how long ago
    /// it is, on the map.
    pub time: Timestamp,
    /// How long the interval is, in seconds: a frame of 11:40 with 300 is
    /// the rain between 11:35 and 11:40.
    pub interval_s: u32,
    /// The path of the frame's picture on this server.
    pub path: String,
    /// The size of the picture, in bytes.
    pub bytes: u32,
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    #[test]
    fn a_frames_moment_is_written_to_the_minute_and_read_back() {
        let time = Utc.with_ymd_and_hms(2026, 11, 21, 11, 40, 0).unwrap();
        assert_eq!(format_radar_time(time), "20261121T1140Z");
        assert_eq!(parse_radar_time("20261121T1140Z"), Some(time));
        assert_eq!(
            radar_frame_path(&SourceId::new("dwd-radar-ry").unwrap(), time),
            "/v1/radar/dwd-radar-ry/20261121T1140Z.png"
        );
        for not_a_moment in [
            "",
            "20261121T1140",
            "20261121T1140Z ",
            "2026-11-21T11:40Z",
            "20261321T1140Z",
            "20261121T2560Z",
            "20261121t1140z",
            "+0261121T1140Z",
            "../../../etc/a",
        ] {
            assert_eq!(parse_radar_time(not_a_moment), None, "{not_a_moment:?}");
        }
    }

    #[test]
    fn an_answer_without_frames_is_still_an_answer() {
        let text = r#"{
            "time": "2026-11-21T11:44:00Z",
            "steps_mm_h": [0.1, 0.2, 1.0, 4.0, 10.0, 30.0],
            "unseen": 7,
            "layers": [{
                "source": "dwd-radar-ry",
                "west": 9.4, "south": 46.3, "east": 17.2, "north": 48.5,
                "columns": 624, "rows": 261, "cadence_seconds": 300,
                "frames": []
            }],
            "attributions": []
        }"#;
        let answer: RadarResponse = serde_json::from_str(text).unwrap();
        assert!(answer.layers[0].frames.is_empty());
        assert_eq!(answer.unseen, 7);
    }
}
