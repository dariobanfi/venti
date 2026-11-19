//! The picture a chat app shows beside a link to a flight.
//!
//! The track's shape as a line in the app's colour, and the flight's
//! numbers beside it, on a plain light ground: 1200 by 630 pixels, the size
//! link previews are laid out for. There is no map under the line. A map
//! needs tiles, and the server has none yet.
//!
//! The picture is drawn here, in Rust, by `tiny-skia`; nothing is fetched
//! and no other program runs. Its letters come from the Nunito typeface,
//! which is compiled in (`assets/Nunito-wght.ttf`, SIL Open Font License
//! 1.1, see the README beside it). Only text this module writes itself is
//! set in it: numbers, units, a date. A pilot's name is not, so the
//! typeface never meets a script it has no letters for.

use std::sync::LazyLock;

use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Rect, Stroke, Transform,
};
use ttf_parser::{Face, OutlineBuilder, Tag};
use venti_store::Flight;

use super::share::{ACCENT_LIGHT, Facts, Measure, fit};

/// Raise this when the picture changes for a flight that did not: copies
/// that browsers and chat apps keep are then fetched again.
pub const DESIGN: u32 = 1;

/// The picture's width in pixels.
pub const WIDTH: u32 = 1200;
/// The picture's height in pixels.
pub const HEIGHT: u32 = 630;

static NUNITO: &[u8] = include_bytes!("../../assets/Nunito-wght.ttf");

/// The weight of the numbers and of the labels, on the typeface's scale
/// from 200 to 1000.
const BOLD: f32 = 700.0;
const REGULAR: f32 = 500.0;

static FACES: LazyLock<Option<(Face<'static>, Face<'static>)>> = LazyLock::new(|| {
    let face = |weight: f32| {
        let mut face = Face::parse(NUNITO, 0).ok()?;
        face.set_variation(Tag::from_bytes(b"wght"), weight)?;
        Some(face)
    };
    Some((face(BOLD)?, face(REGULAR)?))
});

/// Why no picture could be drawn. None of these depends on the flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PictureError {
    /// The compiled-in typeface cannot be read.
    #[error("the typeface for the preview picture cannot be read")]
    Typeface,
    /// The picture could not be written as a PNG file.
    #[error("the preview picture could not be encoded")]
    Encoding,
}

fn colour(hex: &str) -> Color {
    let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).unwrap_or(0);
    Color::from_rgba8(channel(1), channel(3), channel(5), 255)
}

fn paint(color: Color) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(color);
    paint.anti_alias = true;
    paint
}

/// Turns a glyph's outline into a path, at a place and size on the picture.
struct Glyph {
    path: PathBuilder,
    x: f32,
    baseline: f32,
    scale: f32,
}

impl Glyph {
    fn place(&self, x: f32, y: f32) -> (f32, f32) {
        // A typeface's y axis points up, a picture's down.
        (self.x + x * self.scale, self.baseline - y * self.scale)
    }
}

impl OutlineBuilder for Glyph {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.place(x, y);
        self.path.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.place(x, y);
        self.path.line_to(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let ((x1, y1), (x, y)) = (self.place(x1, y1), self.place(x, y));
        self.path.quad_to(x1, y1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let ((x1, y1), (x2, y2), (x, y)) =
            (self.place(x1, y1), self.place(x2, y2), self.place(x, y));
        self.path.cubic_to(x1, y1, x2, y2, x, y);
    }
    fn close(&mut self) {
        self.path.close();
    }
}

/// How wide `text` is when set at `size` pixels.
fn width_of(face: &Face<'_>, text: &str, size: f32) -> f32 {
    let scale = size / f32::from(face.units_per_em());
    text.chars()
        .filter_map(|c| face.glyph_index(c))
        .filter_map(|glyph| face.glyph_hor_advance(glyph))
        .map(|advance| f32::from(advance) * scale)
        .sum()
}

/// Sets `text` with its left end at `x` on the baseline `baseline`, and
/// returns where it ends. A character the typeface lacks is left out.
fn set(
    pixmap: &mut Pixmap,
    face: &Face<'_>,
    text: &str,
    size: f32,
    (x, baseline): (f32, f32),
    color: Color,
) -> f32 {
    let scale = size / f32::from(face.units_per_em());
    let mut pen = x;
    for glyph in text.chars().filter_map(|c| face.glyph_index(c)) {
        let mut outline = Glyph {
            path: PathBuilder::new(),
            x: pen,
            baseline,
            scale,
        };
        // A space has an advance and no outline.
        let drawn = face.outline_glyph(glyph, &mut outline).is_some();
        if let Some(path) = outline.path.finish().filter(|_| drawn) {
            pixmap.fill_path(
                &path,
                &paint(color),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
        pen += f32::from(face.glyph_hor_advance(glyph).unwrap_or(0)) * scale;
    }
    pen
}

/// A rectangle with rounded corners.
fn rounded(x: f32, y: f32, width: f32, height: f32, radius: f32) -> Option<Path> {
    // How far a curve's control point lies from the corner for the curve
    // to pass for a quarter circle.
    let k = radius * 0.5523;
    let (right, bottom) = (x + width, y + height);
    let mut path = PathBuilder::new();
    path.move_to(x + radius, y);
    path.line_to(right - radius, y);
    path.cubic_to(
        right - radius + k,
        y,
        right,
        y + radius - k,
        right,
        y + radius,
    );
    path.line_to(right, bottom - radius);
    path.cubic_to(
        right,
        bottom - radius + k,
        right - radius + k,
        bottom,
        right - radius,
        bottom,
    );
    path.line_to(x + radius, bottom);
    path.cubic_to(
        x + radius - k,
        bottom,
        x,
        bottom - radius + k,
        x,
        bottom - radius,
    );
    path.line_to(x, y + radius);
    path.cubic_to(x, y + radius - k, x + radius - k, y, x + radius, y);
    path.close();
    path.finish()
}

/// Draws the picture for a flight and returns it as a PNG file.
pub fn render(flight: &Flight) -> Result<Vec<u8>, PictureError> {
    let (bold, regular) = FACES.as_ref().ok_or(PictureError::Typeface)?;
    let facts = Facts::of(flight);
    let accent = colour(ACCENT_LIGHT);
    let ink = colour("#1c1c1e");
    let quiet = colour("#6b6b70");
    let white = Color::WHITE;

    let mut pixmap = Pixmap::new(WIDTH, HEIGHT).ok_or(PictureError::Encoding)?;
    pixmap.fill(white);

    // The track, on a tinted panel that takes the left of the picture.
    let (panel_x, panel_y, panel_w, panel_h) = (48.0, 48.0, 640.0, 534.0);
    if let Some(panel) = rounded(panel_x, panel_y, panel_w, panel_h, 36.0) {
        pixmap.fill_path(
            &panel,
            &paint(colour("#f3f1fb")),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
    let points: Vec<(f32, f32)> = fit(
        &flight.derived.outline,
        f64::from(panel_w),
        f64::from(panel_h),
        56.0,
    )
    .into_iter()
    .map(|(x, y)| (panel_x + x as f32, panel_y + y as f32))
    .collect();
    if let (Some(&start), Some(&end)) = (points.first(), points.last()) {
        let mut line = PathBuilder::new();
        line.move_to(start.0, start.1);
        for &(x, y) in &points[1..] {
            line.line_to(x, y);
        }
        let stroke = |width: f32| Stroke {
            width,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        if let Some(line) = line.finish() {
            pixmap.stroke_path(
                &line,
                &paint(accent),
                &stroke(9.0),
                Transform::identity(),
                None,
            );
        }
        // As in the app: a hollow dot where the flight began, a solid one
        // where it ended.
        let dot = |pixmap: &mut Pixmap, (x, y): (f32, f32), radius: f32, color: Color| {
            if let Some(circle) = PathBuilder::from_circle(x, y, radius) {
                pixmap.fill_path(
                    &circle,
                    &paint(color),
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        };
        dot(&mut pixmap, end, 15.0, accent);
        dot(&mut pixmap, start, 15.0, accent);
        dot(&mut pixmap, start, 8.0, white);
    }

    // The numbers, in a column on the right: each large, its unit small and
    // grey beside it, and what it measures below.
    let column = 740.0;
    let room = WIDTH as f32 - column - 48.0;
    let mut number = |baseline: f32, size: f32, measure: &Measure, caption: &str| {
        let unit_size = size * 0.42;
        // A value too wide for the column is set smaller, never cut off.
        let wide = width_of(bold, &measure.value, size)
            + size * 0.18
            + width_of(regular, measure.unit, unit_size);
        let shrink = (room / wide).min(1.0);
        let (size, unit_size) = (size * shrink, unit_size * shrink);
        let end = set(
            &mut pixmap,
            bold,
            &measure.value,
            size,
            (column, baseline),
            ink,
        );
        set(
            &mut pixmap,
            regular,
            measure.unit,
            unit_size,
            (end + size * 0.18, baseline),
            quiet,
        );
        set(
            &mut pixmap,
            regular,
            caption,
            30.0,
            (column, baseline + 44.0),
            quiet,
        );
    };
    number(170.0, 124.0, &facts.distance, &facts.distance_caption);
    number(330.0, 76.0, &facts.airtime, "airtime");
    if let Some(points) = &facts.points {
        // Venti's own count by the World XContest's rules, not the league's.
        number(466.0, 76.0, points, "scored by Venti");
    }

    // The day of the flight and the app's name along the bottom.
    set(
        &mut pixmap,
        regular,
        &facts.date,
        30.0,
        (column, 574.0),
        quiet,
    );
    let name = "Venti";
    let name_width = width_of(bold, name, 40.0);
    set(
        &mut pixmap,
        bold,
        name,
        40.0,
        (WIDTH as f32 - 48.0 - name_width, 576.0),
        accent,
    );

    // A thin frame, so that the white picture ends somewhere on a white
    // page.
    if let Some(frame) = Rect::from_xywh(0.5, 0.5, WIDTH as f32 - 1.0, HEIGHT as f32 - 1.0) {
        let frame = PathBuilder::from_rect(frame);
        pixmap.stroke_path(
            &frame,
            &paint(colour("#d9d9de")),
            &Stroke::default(),
            Transform::identity(),
            None,
        );
    }

    pixmap.encode_png().map_err(|_| PictureError::Encoding)
}

#[cfg(test)]
mod tests {
    use super::super::share::tests::flight;
    use super::*;

    fn decode(png: &[u8]) -> Pixmap {
        Pixmap::decode_png(png).expect("the picture is a PNG file")
    }

    /// How many pixels are close to a colour.
    fn count(pixmap: &Pixmap, hex: &str) -> usize {
        let wanted = colour(hex).to_color_u8();
        pixmap
            .pixels()
            .iter()
            .filter(|pixel| {
                let near = |a: u8, b: u8| a.abs_diff(b) < 12;
                near(pixel.red(), wanted.red())
                    && near(pixel.green(), wanted.green())
                    && near(pixel.blue(), wanted.blue())
            })
            .count()
    }

    #[test]
    fn the_picture_is_a_png_of_the_size_previews_expect() {
        let png = render(&flight()).unwrap();

        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let pixmap = decode(&png);
        assert_eq!((pixmap.width(), pixmap.height()), (WIDTH, HEIGHT));
        // Small enough to send to a phone on a mountain.
        assert!(png.len() < 200_000, "{} bytes", png.len());
    }

    #[test]
    fn it_shows_the_track_in_the_apps_colour_and_text_beside_it() {
        let pixmap = decode(&render(&flight()).unwrap());

        // The line, its two dots and the app's name are accent-coloured;
        // the numbers are dark; most of the picture is plain ground.
        assert!(count(&pixmap, ACCENT_LIGHT) > 5_000);
        assert!(count(&pixmap, "#1c1c1e") > 3_000);
        let ground = count(&pixmap, "#ffffff") + count(&pixmap, "#f3f1fb");
        assert!(ground > (WIDTH * HEIGHT) as usize * 8 / 10);
        // The start of the track is a hollow dot: white in the middle.
        let start = fit(&flight().derived.outline, 640.0, 534.0, 56.0)[0];
        let centre = pixmap
            .pixel(48 + start.0 as u32, 48 + start.1 as u32)
            .unwrap();
        assert_eq!(
            (centre.red(), centre.green(), centre.blue()),
            (255, 255, 255)
        );
    }

    #[test]
    fn the_same_flight_gives_the_same_picture() {
        assert_eq!(render(&flight()).unwrap(), render(&flight()).unwrap());
        let mut other = flight();
        other.derived.airtime_s += 600;
        assert_ne!(render(&flight()).unwrap(), render(&other).unwrap());
    }

    #[test]
    fn a_flight_with_little_to_show_still_gets_a_picture() {
        let mut bare = flight();
        bare.derived.outline = Vec::new();
        bare.derived.score = None;
        let pixmap = decode(&render(&bare).unwrap());
        // No track: only the app's name is accent-coloured.
        assert!(count(&pixmap, ACCENT_LIGHT) < 5_000);
        assert!(count(&pixmap, "#1c1c1e") > 1_000);

        bare.derived.outline = vec![[47.0, 11.0]];
        render(&bare).unwrap();
        // Numbers far beyond any flight do not break the layout.
        bare.derived.track_length_m = 9.9e12;
        bare.derived.airtime_s = i64::MAX;
        render(&bare).unwrap();
    }

    #[test]
    fn a_pilots_name_is_not_drawn() {
        // The picture depends on the flight's numbers and track alone, so
        // a name in a script the typeface lacks cannot spoil it.
        let mut named = flight();
        named.pilot_name = "山田 太郎 <script>".to_owned();
        named.derived.glider = Some("Ωmega".to_owned());
        assert_eq!(render(&named).unwrap(), render(&flight()).unwrap());
    }

    #[test]
    fn the_typeface_has_every_character_the_picture_sets() {
        let (bold, regular) = FACES.as_ref().expect("the typeface is readable");
        let needed = "0123456789.:- abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
        for face in [bold, regular] {
            for c in needed.chars() {
                assert!(face.glyph_index(c).is_some(), "no glyph for {c:?}");
            }
        }
        // The two weights really differ.
        let (heavy, light) = (
            width_of(bold, "Venti airtime", 100.0),
            width_of(regular, "Venti airtime", 100.0),
        );
        assert!(heavy > light, "{heavy} {light}");
    }
}
