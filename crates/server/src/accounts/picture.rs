//! A pilot's picture: what a picture that is sent to the server becomes.
//!
//! The server never hands out what it was sent. It reads the picture (a
//! JPEG, a PNG or a WebP, told apart by their first bytes and not by what
//! the request says), turns it as a JPEG's orientation says, cuts the
//! largest square out of its middle, scales that to
//! [`PICTURE_SIDE`] pixels on each side and writes it as a new WebP.
//! Nothing of the original goes along: no metadata (the place and time a
//! camera writes into a photograph, the camera's name), no colour profile,
//! no second frame.
//!
//! What is read is bounded before it is decoded: the bytes by the route's
//! limit, the sides and the pixels from the file's header
//! ([`MAX_PICTURE_SIDE`], [`MAX_PICTURE_PIXELS`]). A picture of 24 million
//! pixels takes about 100 MB while it is read, which is why only one is
//! read at a time ([`Turns`]).

use std::io::Cursor;

use tokio::sync::Semaphore;
use venti_api_types::v1::{MAX_PICTURE_PIXELS, MAX_PICTURE_SIDE, MIN_PICTURE_SIDE, PICTURE_SIDE};
use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

/// Why a picture was not taken. The words are for the pilot's app.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Refused(pub String);

fn refused(why: impl Into<String>) -> Refused {
    Refused(why.into())
}

/// The kinds of file a picture is read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Jpeg,
    Png,
    Webp,
}

/// The kind of a file, from its first bytes.
fn kind_of(bytes: &[u8]) -> Option<Kind> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(Kind::Jpeg)
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(Kind::Png)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(Kind::Webp)
    } else {
        None
    }
}

/// Pixels of four bytes each, red, green, blue and alpha, row by row from
/// the top.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Rgba {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

/// Whether a picture of these sides may be read at all.
fn check_sides(width: u64, height: u64) -> Result<(), Refused> {
    let (least, most) = (u64::from(MIN_PICTURE_SIDE), u64::from(MAX_PICTURE_SIDE));
    if width < least || height < least {
        return Err(refused(format!(
            "the picture is {width} by {height} pixels; it needs at least {least} on each side"
        )));
    }
    if width > most || height > most || width * height > MAX_PICTURE_PIXELS {
        return Err(refused(format!(
            "the picture is {width} by {height} pixels; it may have at most {most} on a side \
             and {MAX_PICTURE_PIXELS} in all. Scale it down before sending it"
        )));
    }
    Ok(())
}

/// The square picture the server keeps and hands out, made from a picture
/// that was sent to it, as a WebP.
pub fn square(bytes: &[u8]) -> Result<Vec<u8>, Refused> {
    let picture = read(bytes)?;
    let side = PICTURE_SIDE as usize;
    let squared = scaled(&cropped(&picture), side, side);
    encode(&squared)
}

/// The picture the server keeps of one sent with a message: the whole
/// picture, upright, scaled down so that its longer side is at most
/// `most` pixels, as a WebP. Its width and height go with it.
pub fn fitted(bytes: &[u8], most: u32) -> Result<(Vec<u8>, u32, u32), Refused> {
    let picture = read(bytes)?;
    let (w, h) = (picture.width, picture.height);
    let most = most as usize;
    let longer = w.max(h);
    let (out_w, out_h) = if longer <= most {
        (w, h)
    } else {
        (
            (w * most).div_ceil(longer).max(1),
            (h * most).div_ceil(longer).max(1),
        )
    };
    let fitted = scaled(&picture, out_w, out_h);
    let side = |value: usize| u32::try_from(value).map_err(|_| refused("too large"));
    Ok((encode(&fitted)?, side(out_w)?, side(out_h)?))
}

/// A picture sent to the server, read.
fn read(bytes: &[u8]) -> Result<Rgba, Refused> {
    match kind_of(bytes) {
        Some(Kind::Jpeg) => jpeg(bytes),
        Some(Kind::Png) => png(bytes),
        Some(Kind::Webp) => webp(bytes),
        None => Err(refused("the picture is no JPEG, PNG or WebP")),
    }
}

fn jpeg(bytes: &[u8]) -> Result<Rgba, Refused> {
    let most = MAX_PICTURE_SIDE as usize;
    let options = DecoderOptions::default()
        .jpeg_set_out_colorspace(ColorSpace::RGB)
        .set_max_width(most)
        .set_max_height(most);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    let unreadable = |e: zune_jpeg::errors::DecodeErrors| {
        refused(format!(
            "the picture is not a JPEG the server can read: {e}"
        ))
    };
    decoder.decode_headers().map_err(unreadable)?;
    let info = decoder
        .info()
        .ok_or_else(|| refused("the JPEG says nothing of its size"))?;
    check_sides(u64::from(info.width), u64::from(info.height))?;
    let orientation = decoder.exif().map_or(1, |exif| orientation(exif));
    let rgb = decoder.decode().map_err(unreadable)?;
    let (width, height) = (usize::from(info.width), usize::from(info.height));
    if rgb.len() != width * height * 3 {
        return Err(refused("the JPEG holds fewer pixels than it says"));
    }
    let mut pixels = Vec::with_capacity(width * height * 4);
    for pixel in rgb.chunks_exact(3) {
        pixels.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
    }
    Ok(oriented(
        Rgba {
            width,
            height,
            pixels,
        },
        orientation,
    ))
}

fn png(bytes: &[u8]) -> Result<Rgba, Refused> {
    let unreadable = |e: png::DecodingError| {
        refused(format!("the picture is not a PNG the server can read: {e}"))
    };
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            // Room for the largest picture taken, at eight bytes a pixel.
            bytes: usize::try_from(MAX_PICTURE_PIXELS * 8).unwrap_or(usize::MAX),
        },
    );
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let header = decoder.read_header_info().map_err(unreadable)?;
    check_sides(u64::from(header.width), u64::from(header.height))?;
    let mut reader = decoder.read_info().map_err(unreadable)?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| refused("the PNG is too large"))?;
    let mut buffer = vec![0; size];
    let frame = reader.next_frame(&mut buffer).map_err(unreadable)?;
    let (width, height) = (frame.width as usize, frame.height as usize);
    let samples = &buffer[..frame.buffer_size()];
    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err(refused("the PNG's palette cannot be read")),
    };
    if frame.bit_depth != png::BitDepth::Eight || samples.len() != width * height * channels {
        return Err(refused("the PNG holds fewer pixels than it says"));
    }
    let mut pixels = Vec::with_capacity(width * height * 4);
    for pixel in samples.chunks_exact(channels) {
        let rgba = match *pixel {
            [grey] => [grey, grey, grey, 255],
            [grey, alpha] => [grey, grey, grey, alpha],
            [red, green, blue] => [red, green, blue, 255],
            [red, green, blue, alpha] => [red, green, blue, alpha],
            _ => unreachable!("chunks of one to four"),
        };
        pixels.extend_from_slice(&rgba);
    }
    Ok(Rgba {
        width,
        height,
        pixels,
    })
}

fn webp(bytes: &[u8]) -> Result<Rgba, Refused> {
    let unreadable = |e: image_webp::DecodingError| {
        refused(format!(
            "the picture is not a WebP the server can read: {e}"
        ))
    };
    let mut decoder = image_webp::WebPDecoder::new(Cursor::new(bytes)).map_err(unreadable)?;
    let (width, height) = decoder.dimensions();
    check_sides(u64::from(width), u64::from(height))?;
    if decoder.is_animated() {
        return Err(refused("the picture moves; a still picture is taken"));
    }
    decoder.set_memory_limit(usize::try_from(MAX_PICTURE_PIXELS * 8).unwrap_or(usize::MAX));
    let size = decoder
        .output_buffer_size()
        .ok_or_else(|| refused("the WebP is too large"))?;
    let mut buffer = vec![0; size];
    decoder.read_image(&mut buffer).map_err(unreadable)?;
    let (width, height) = (width as usize, height as usize);
    let pixels = if decoder.has_alpha() {
        buffer
    } else {
        let mut pixels = Vec::with_capacity(width * height * 4);
        for pixel in buffer.chunks_exact(3) {
            pixels.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
        }
        pixels
    };
    if pixels.len() != width * height * 4 {
        return Err(refused("the WebP holds fewer pixels than it says"));
    }
    Ok(Rgba {
        width,
        height,
        pixels,
    })
}

/// The orientation an EXIF block gives (tag 0x0112 of its first image),
/// 1 to 8; 1, upright, when it says none or cannot be read.
fn orientation(exif: &[u8]) -> u16 {
    let tiff = exif.strip_prefix(b"Exif\0\0").unwrap_or(exif);
    let big = match tiff.get(..2) {
        Some(b"MM") => true,
        Some(b"II") => false,
        _ => return 1,
    };
    let u16_at = |at: usize| -> Option<u16> {
        let two: [u8; 2] = tiff.get(at..at + 2)?.try_into().ok()?;
        Some(if big {
            u16::from_be_bytes(two)
        } else {
            u16::from_le_bytes(two)
        })
    };
    let u32_at = |at: usize| -> Option<u32> {
        let four: [u8; 4] = tiff.get(at..at + 4)?.try_into().ok()?;
        Some(if big {
            u32::from_be_bytes(four)
        } else {
            u32::from_le_bytes(four)
        })
    };
    let read = || -> Option<u16> {
        let ifd = u32_at(4)? as usize;
        let entries = u16_at(ifd)?;
        (0..usize::from(entries)).find_map(|n| {
            let entry = ifd + 2 + n * 12;
            (u16_at(entry)? == 0x0112 && u16_at(entry + 2)? == 3)
                .then(|| u16_at(entry + 8))
                .flatten()
        })
    };
    read().filter(|value| (1..=8).contains(value)).unwrap_or(1)
}

/// The picture turned upright, as an EXIF orientation says it was taken.
fn oriented(picture: Rgba, orientation: u16) -> Rgba {
    if orientation == 1 {
        return picture;
    }
    let (w, h) = (picture.width, picture.height);
    // 5 to 8 swap the sides.
    let swapped = orientation >= 5;
    let (out_w, out_h) = if swapped { (h, w) } else { (w, h) };
    let mut pixels = vec![0; picture.pixels.len()];
    for y in 0..out_h {
        for x in 0..out_w {
            // The source pixel shown at (x, y).
            let (sx, sy) = match orientation {
                2 => (w - 1 - x, y),
                3 => (w - 1 - x, h - 1 - y),
                4 => (x, h - 1 - y),
                5 => (y, x),
                6 => (y, h - 1 - x),
                7 => (w - 1 - y, h - 1 - x),
                8 => (w - 1 - y, x),
                _ => (x, y),
            };
            let from = (sy * w + sx) * 4;
            let to = (y * out_w + x) * 4;
            pixels[to..to + 4].copy_from_slice(&picture.pixels[from..from + 4]);
        }
    }
    Rgba {
        width: out_w,
        height: out_h,
        pixels,
    }
}

/// The largest square in the middle of the picture.
fn cropped(picture: &Rgba) -> Rgba {
    let side = picture.width.min(picture.height);
    let (left, top) = ((picture.width - side) / 2, (picture.height - side) / 2);
    let mut pixels = Vec::with_capacity(side * side * 4);
    for y in top..top + side {
        let row = (y * picture.width + left) * 4;
        pixels.extend_from_slice(&picture.pixels[row..row + side * 4]);
    }
    Rgba {
        width: side,
        height: side,
        pixels,
    }
}

/// For each pixel of a line of `to` pixels, the pixels of a line of `from`
/// it covers, each with how much of it.
fn coverage(from: usize, to: usize) -> Vec<Vec<(usize, f32)>> {
    let step = from as f64 / to as f64;
    (0..to)
        .map(|i| {
            let (start, end) = (i as f64 * step, (i + 1) as f64 * step);
            let first = start.floor() as usize;
            let last = (end.ceil() as usize).min(from).max(first + 1);
            (first..last)
                .map(|j| {
                    let covered = (end.min(j as f64 + 1.0) - start.max(j as f64)).max(0.0);
                    (j, (covered / step) as f32)
                })
                .filter(|&(_, weight)| weight > 0.0)
                .collect()
        })
        .collect()
}

/// A picture scaled to `out_w` by `out_h` pixels: every pixel the mean of
/// what it covers, colour weighted by how opaque it is, so that a
/// transparent edge does not darken.
fn scaled(picture: &Rgba, out_w: usize, out_h: usize) -> Rgba {
    let (w, h) = (picture.width, picture.height);
    let side = out_w;
    let across = coverage(w, out_w);
    let down = coverage(h, out_h);
    // First along each row, in premultiplied colour.
    let mut rows = vec![0.0_f32; h * side * 4];
    for y in 0..h {
        for (x, taps) in across.iter().enumerate() {
            let mut sum = [0.0_f32; 4];
            for &(sx, weight) in taps {
                let p = &picture.pixels[(y * w + sx) * 4..(y * w + sx) * 4 + 4];
                let alpha = f32::from(p[3]) / 255.0;
                for c in 0..3 {
                    sum[c] += f32::from(p[c]) * alpha * weight;
                }
                sum[3] += alpha * weight;
            }
            rows[(y * side + x) * 4..(y * side + x) * 4 + 4].copy_from_slice(&sum);
        }
    }
    let mut pixels = vec![0_u8; out_w * out_h * 4];
    for (y, taps) in down.iter().enumerate() {
        for x in 0..side {
            let mut sum = [0.0_f32; 4];
            for &(sy, weight) in taps {
                let p = &rows[(sy * side + x) * 4..(sy * side + x) * 4 + 4];
                for c in 0..4 {
                    sum[c] += p[c] * weight;
                }
            }
            let out = &mut pixels[(y * side + x) * 4..(y * side + x) * 4 + 4];
            let alpha = sum[3].clamp(0.0, 1.0);
            for c in 0..3 {
                let colour = if alpha > 0.0 { sum[c] / alpha } else { 0.0 };
                out[c] = colour.round().clamp(0.0, 255.0) as u8;
            }
            out[3] = (alpha * 255.0).round() as u8;
        }
    }
    Rgba {
        width: out_w,
        height: out_h,
        pixels,
    }
}

/// The picture as a WebP of its own, with no metadata.
fn encode(picture: &Rgba) -> Result<Vec<u8>, Refused> {
    let width = u32::try_from(picture.width).map_err(|_| refused("too large"))?;
    let height = u32::try_from(picture.height).map_err(|_| refused("too large"))?;
    let opaque = picture.pixels.chunks_exact(4).all(|p| p[3] == 255);
    let mut out = Vec::new();
    let encoder = image_webp::WebPEncoder::new(&mut out);
    let result = if opaque {
        let rgb: Vec<u8> = picture
            .pixels
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        encoder.encode(&rgb, width, height, image_webp::ColorType::Rgb8)
    } else {
        encoder.encode(&picture.pixels, width, height, image_webp::ColorType::Rgba8)
    };
    result.map_err(|e| refused(format!("the picture could not be written: {e}")))?;
    Ok(out)
}

/// The one turn at reading a picture: a large one takes some hundred
/// megabytes while it is read, and two at once are not needed.
#[derive(Debug)]
pub struct Turns(Semaphore);

impl Default for Turns {
    fn default() -> Self {
        Self(Semaphore::new(1))
    }
}

impl Turns {
    /// The square picture made from `bytes`, read in its turn, away from
    /// the threads that answer requests.
    pub async fn square(&self, bytes: Vec<u8>) -> Result<Vec<u8>, Refused> {
        let _turn = self
            .0
            .acquire()
            .await
            .map_err(|_| refused("the server is stopping"))?;
        tokio::task::spawn_blocking(move || square(&bytes))
            .await
            .map_err(|_| refused("the picture could not be read"))?
    }

    /// The picture made from `bytes` for a message, in its turn; see
    /// [`fitted`].
    pub async fn fitted(&self, bytes: Vec<u8>, most: u32) -> Result<(Vec<u8>, u32, u32), Refused> {
        let _turn = self
            .0
            .acquire()
            .await
            .map_err(|_| refused("the server is stopping"))?;
        tokio::task::spawn_blocking(move || fitted(&bytes, most))
            .await
            .map_err(|_| refused("the picture could not be read"))?
    }
}

/// A made-up picture of `width` by `height` pixels as a PNG: a gradient
/// with a pale disc, for tests and the development mode's "Google".
pub fn made_up_png(width: u32, height: u32, seed: u8) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("a header in memory");
        let mut data = Vec::with_capacity((width * height * 3) as usize);
        let (cx, cy, r) = (
            width as f32 / 2.0,
            height as f32 / 2.0,
            width.min(height) as f32 / 3.0,
        );
        for y in 0..height {
            for x in 0..width {
                let inside = (x as f32 - cx).hypot(y as f32 - cy) < r;
                if inside {
                    data.extend_from_slice(&[240, 235, 220]);
                } else {
                    data.extend_from_slice(&[
                        seed.wrapping_add((x * 255 / width) as u8),
                        (y * 255 / height) as u8,
                        160,
                    ]);
                }
            }
        }
        writer.write_image_data(&data).expect("pixels in memory");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(webp: &[u8]) -> (u32, u32, Vec<u8>) {
        let mut decoder = image_webp::WebPDecoder::new(Cursor::new(webp)).unwrap();
        let (w, h) = decoder.dimensions();
        let mut buffer = vec![0; decoder.output_buffer_size().unwrap()];
        decoder.read_image(&mut buffer).unwrap();
        (w, h, buffer)
    }

    #[test]
    fn a_picture_becomes_a_square_webp_of_the_fixed_size() {
        let wide = made_up_png(600, 300, 0);
        let out = square(&wide).unwrap();
        assert_eq!(kind_of(&out), Some(Kind::Webp));
        let (w, h, _) = decoded(&out);
        assert_eq!((w, h), (PICTURE_SIDE, PICTURE_SIDE));
        // A small one is scaled up to the same size.
        let small = made_up_png(40, 64, 9);
        let (w, h, _) = decoded(&square(&small).unwrap());
        assert_eq!((w, h), (PICTURE_SIDE, PICTURE_SIDE));
        // And the WebP the server wrote is a picture it reads again.
        assert!(square(&out).is_ok());
    }

    #[test]
    fn what_is_no_picture_or_out_of_bounds_is_refused() {
        assert!(square(b"GIF89a....").is_err());
        assert!(square(b"").is_err());
        let tiny = made_up_png(20, 200, 0);
        assert!(square(&tiny).unwrap_err().0.contains("at least 32"));
        // A PNG that says it is huge is refused from its header, before
        // anything is decoded.
        let mut huge = made_up_png(64, 64, 0);
        // The width in the IHDR chunk, bytes 16 to 19.
        huge[16..20].copy_from_slice(&20_000_u32.to_be_bytes());
        assert!(square(&huge).is_err());
        let mut broken = made_up_png(64, 64, 0);
        broken.truncate(60);
        assert!(square(&broken).is_err());
    }

    #[test]
    fn the_middle_square_is_kept() {
        // Left third red, middle third green, right third blue.
        let picture = Rgba {
            width: 3,
            height: 1,
            pixels: vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255],
        };
        let square = cropped(&picture);
        assert_eq!((square.width, square.height), (1, 1));
        assert_eq!(square.pixels, [0, 255, 0, 255]);
    }

    #[test]
    fn scaling_down_takes_the_mean() {
        let picture = Rgba {
            width: 2,
            height: 2,
            pixels: vec![
                0, 0, 0, 255, 200, 200, 200, 255, 100, 100, 100, 255, 100, 100, 100, 255,
            ],
        };
        let one = scaled(&picture, 1, 1);
        assert_eq!(one.pixels, [100, 100, 100, 255]);
        // A transparent pixel adds nothing to the colour.
        let half = Rgba {
            width: 2,
            height: 1,
            pixels: vec![200, 100, 0, 255, 0, 0, 0, 0],
        };
        let one = scaled(
            &Rgba {
                width: 2,
                height: 2,
                pixels: [half.pixels.clone(), half.pixels].concat(),
            },
            1,
            1,
        );
        assert_eq!(one.pixels, [200, 100, 0, 128]);
    }

    #[test]
    fn a_photograph_is_turned_upright() {
        // Two pixels side by side, a and b; orientation 6 says the camera
        // was turned right: upright, a is above b.
        let picture = Rgba {
            width: 2,
            height: 1,
            pixels: vec![1, 1, 1, 255, 2, 2, 2, 255],
        };
        let turned = oriented(picture.clone(), 6);
        assert_eq!((turned.width, turned.height), (1, 2));
        assert_eq!(turned.pixels, [1, 1, 1, 255, 2, 2, 2, 255]);
        let turned = oriented(picture.clone(), 8);
        assert_eq!(turned.pixels, [2, 2, 2, 255, 1, 1, 1, 255]);
        assert_eq!(
            oriented(picture.clone(), 3).pixels,
            [2, 2, 2, 255, 1, 1, 1, 255]
        );

        // EXIF in both byte orders, with the tag where a camera writes it.
        let block = |big: bool, value: u16| {
            let mut tiff = Vec::new();
            let u16b = |v: u16| {
                if big {
                    v.to_be_bytes()
                } else {
                    v.to_le_bytes()
                }
            };
            let u32b = |v: u32| {
                if big {
                    v.to_be_bytes()
                } else {
                    v.to_le_bytes()
                }
            };
            tiff.extend_from_slice(if big { b"MM" } else { b"II" });
            tiff.extend_from_slice(&u16b(42));
            tiff.extend_from_slice(&u32b(8));
            tiff.extend_from_slice(&u16b(1));
            tiff.extend_from_slice(&u16b(0x0112));
            tiff.extend_from_slice(&u16b(3));
            tiff.extend_from_slice(&u32b(1));
            tiff.extend_from_slice(&u16b(value));
            tiff.extend_from_slice(&[0, 0]);
            [b"Exif\0\0".to_vec(), tiff].concat()
        };
        assert_eq!(orientation(&block(true, 6)), 6);
        assert_eq!(orientation(&block(false, 8)), 8);
        assert_eq!(orientation(&block(false, 9)), 1);
        assert_eq!(orientation(b"nonsense"), 1);
    }
}
