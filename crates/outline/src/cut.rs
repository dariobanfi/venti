//! The cut: a tile on the border loses what lies beyond it.
//!
//! The provider's tile is a JPEG, white wherever the photograph ends. Each
//! pixel is placed against the outline and its band ([`Place`]) and:
//!
//! - inside the outline shrunk by the margin, it is kept, however white:
//!   a glacier, a roof, a snowfield;
//! - outside the outline grown by the margin, it is removed, whatever it
//!   shows;
//! - in the band between, it is removed if it is the provider's white
//!   (every channel at least [`WHITE`]) and kept otherwise. The outline is
//!   a few metres off here and there; within the band the provider's own
//!   white decides where its photograph ends.
//!
//! What is left goes out as a PNG whose removed pixels are transparent.

use venti_sources::{Cut, CutCounts, CutTile, SourceError};
use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

use crate::mercator::Rect;
use crate::outline::{Outline, Place};

/// The least value of each channel that counts as the provider's white.
/// The white beyond the border is 255 in the JPEG's source; next to the
/// photograph the compression rings, and values of 250 and below occur
/// there. A photograph has whites of its own, which is why white decides
/// only in the band.
pub const WHITE: u8 = 245;

/// The largest picture a tile may be, on each side. The provider's are 256
/// pixels; anything far larger is not a tile of its.
const MAX_SIDE: usize = 1024;

/// Cuts a tile that covers `rect` along `outline`.
pub fn cut(outline: &Outline, rect: Rect, jpeg: &[u8]) -> Result<CutTile, SourceError> {
    let options = DecoderOptions::default()
        .jpeg_set_out_colorspace(ColorSpace::RGB)
        .set_max_width(MAX_SIDE)
        .set_max_height(MAX_SIDE);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(jpeg), options);
    let pixels = decoder
        .decode()
        .map_err(|e| SourceError::Malformed(format!("the tile is not a JPEG it can read: {e}")))?;
    let info = decoder
        .info()
        .ok_or_else(|| SourceError::Malformed("the JPEG has no size".to_owned()))?;
    cut_rgb(
        outline,
        rect,
        &pixels,
        usize::from(info.width),
        usize::from(info.height),
    )
}

/// Cuts a tile's pixels, three bytes each, row by row from the north.
pub fn cut_rgb(
    outline: &Outline,
    rect: Rect,
    pixels: &[u8],
    width: usize,
    height: usize,
) -> Result<CutTile, SourceError> {
    if width == 0 || height == 0 || pixels.len() != width * height * 3 {
        return Err(SourceError::Malformed(format!(
            "the JPEG says {width} by {height} pixels and holds {} bytes of them",
            pixels.len()
        )));
    }

    let places = outline.places(rect, width, height);
    let mut counts = CutCounts::default();
    let mut rgba = Vec::with_capacity(width * height * 4);
    for (pixel, place) in pixels.chunks_exact(3).zip(&places) {
        let white = pixel.iter().all(|&channel| channel >= WHITE);
        let keep = match place {
            Place::Inside => true,
            Place::Outside => false,
            Place::Band => !white,
        };
        if keep {
            counts.kept += 1;
            rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
        } else {
            if *place == Place::Outside {
                counts.removed_outside += 1;
            } else {
                counts.removed_white += 1;
            }
            rgba.extend_from_slice(&[0, 0, 0, 0]);
        }
    }

    let cut = if counts.kept == 0 {
        Cut::Nothing
    } else if counts.removed_outside == 0 && counts.removed_white == 0 {
        Cut::Unchanged
    } else {
        Cut::Png(png(&rgba, width, height)?)
    };
    Ok(CutTile { cut, counts })
}

/// Encodes RGBA pixels as a PNG.
fn png(rgba: &[u8], width: usize, height: usize) -> Result<Vec<u8>, SourceError> {
    let failed = |e: png::EncodingError| SourceError::Malformed(format!("PNG: {e}"));
    let side = |n: usize| u32::try_from(n).map_err(|_| SourceError::Malformed("too large".into()));
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, side(width)?, side(height)?);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().map_err(failed)?;
    writer.write_image_data(rgba).map_err(failed)?;
    writer.finish().map_err(failed)?;
    Ok(out)
}
