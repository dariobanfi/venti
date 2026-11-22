//! The seven tiles recorded from IGN's Géoplateforme on 2026-11-22, placed
//! against France's outline and cut, compared with golden files: the cut tile as
//! a PNG and a summary of what was kept and removed, so that a change
//! reads as a diff of numbers.
//!
//! The summary also counts what would show a fault of the outline: white
//! pixels that were kept, and pixels that were removed although they are
//! not white. Both should be near nothing; where the outline is more than
//! the margin off, the second grows.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use venti_outline::cut::WHITE;
use venti_sources::{Coverage, Cut, CutTile, TileAddress};
use venti_sources_ign::fixtures::{self, Recorded};
use venti_sources_ign::{Ign, MARGIN_M};
use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

/// Compares bytes with a golden file, or writes the file when
/// `VENTI_UPDATE_GOLDEN` is set.
fn assert_golden_bytes(name: &str, actual: &[u8]) {
    let path = golden(name);
    if std::env::var_os("VENTI_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, actual).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        return;
    }
    let expected = std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}\nrun with VENTI_UPDATE_GOLDEN=1 to create it",
            path.display()
        )
    });
    assert!(
        expected == actual,
        "{} differs\nrun with VENTI_UPDATE_GOLDEN=1 to accept the new output and look at it",
        path.display()
    );
}

fn rgb_of(jpeg: &[u8]) -> Vec<u8> {
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    JpegDecoder::new_with_options(ZCursor::new(jpeg), options)
        .decode()
        .unwrap()
}

fn alpha_of(png: &[u8]) -> Vec<u8> {
    let decoder = png::Decoder::new(std::io::Cursor::new(png));
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    pixels.truncate(info.buffer_size());
    pixels.chunks_exact(4).map(|p| p[3]).collect()
}

fn coverage_name(coverage: Coverage) -> &'static str {
    match coverage {
        Coverage::Inside => "inside",
        Coverage::Outside => "outside",
        Coverage::Edge => "on the edge",
    }
}

/// What the summary says about one tile.
fn summary(recorded: &Recorded, coverage: Coverage, cut: &CutTile) -> String {
    let tile = recorded.tile;
    let mut text = format!(
        "{} {tile}: {} bytes, {}\n",
        recorded.name,
        recorded.jpeg.len(),
        coverage_name(coverage)
    );
    let counts = cut.counts;
    let _ = writeln!(
        text,
        "  kept {}, removed outside the margin {}, removed as white in the band {}",
        counts.kept, counts.removed_outside, counts.removed_white
    );
    let rgb = rgb_of(recorded.jpeg);
    let white: Vec<bool> = rgb
        .chunks_exact(3)
        .map(|p| p.iter().all(|&c| c >= WHITE))
        .collect();
    let alpha = match &cut.cut {
        Cut::Png(png) => alpha_of(png),
        Cut::Unchanged => vec![255; white.len()],
        Cut::Nothing => vec![0; white.len()],
    };
    let kept_white = alpha
        .iter()
        .zip(&white)
        .filter(|(a, w)| **a == 255 && **w)
        .count();
    let removed_photo = alpha
        .iter()
        .zip(&white)
        .filter(|(a, w)| **a == 0 && !**w)
        .count();
    let _ = writeln!(
        text,
        "  white pixels kept {kept_white}, pixels that are not white removed {removed_photo}"
    );
    let _ = writeln!(
        text,
        "  goes out as: {}",
        match &cut.cut {
            Cut::Unchanged => "the provider's JPEG".to_owned(),
            Cut::Png(png) => format!("a PNG of {} bytes", png.len()),
            Cut::Nothing => "nothing (204)".to_owned(),
        }
    );
    text
}

fn file_name(tile: TileAddress) -> String {
    format!("{}_{}_{}", tile.z(), tile.x(), tile.y())
}

#[test]
fn the_recorded_tiles_are_placed_and_cut_as_the_golden_files_say() {
    let source = Ign::new();
    let mut text = format!(
        "Seven tiles of IGN's orthophotos recorded on 2026-11-22, against France's \
         outline with a margin of {MARGIN_M} m. White is every channel at least {WHITE}.\n\n"
    );
    for recorded in fixtures::recorded() {
        let coverage = source.classify(recorded.tile);
        let cut = source.cut(recorded.tile, recorded.jpeg).unwrap();
        text.push_str(&summary(&recorded, coverage, &cut));
        text.push('\n');
        if coverage == Coverage::Edge
            && let Cut::Png(png) = &cut.cut
        {
            let name = format!("{}_{}.png", recorded.name, file_name(recorded.tile));
            assert_golden_bytes(&name, png);
        }
    }
    assert_golden_bytes("summary.txt", text.as_bytes());
}

#[test]
fn each_recorded_tile_lies_where_it_was_chosen() {
    let source = Ign::new();
    for recorded in fixtures::recorded() {
        let expected = match recorded.name.split('_').next() {
            Some("inside") => Coverage::Inside,
            Some("outside") => Coverage::Outside,
            _ => Coverage::Edge,
        };
        assert_eq!(
            source.classify(recorded.tile),
            expected,
            "{} {}",
            recorded.name,
            recorded.tile
        );
    }
}

#[test]
fn the_white_tile_over_courmayeur_is_white_and_would_be_cut_to_nothing() {
    let source = Ign::new();
    let courmayeur = fixtures::recorded()
        .into_iter()
        .find(|recorded| recorded.name == "outside_courmayeur")
        .unwrap();
    assert_eq!(courmayeur.jpeg.len(), 1651);
    assert!(rgb_of(courmayeur.jpeg).iter().all(|&c| c >= WHITE));
    let cut = source.cut(courmayeur.tile, courmayeur.jpeg).unwrap();
    assert_eq!(cut.cut, Cut::Nothing);
}

/// Not a check: what cutting and placing take, for the record. Run with
/// `cargo test -p venti-sources-ign --test tiles measured --
/// --ignored --nocapture`; add `--release` for the times a server sees.
#[test]
#[ignore = "measures; prints, asserts nothing"]
fn measured() {
    let started = Instant::now();
    let source = Ign::new();
    eprintln!("reading the outline and its index: {:?}", started.elapsed());
    let tiles = fixtures::recorded();
    for recorded in &tiles {
        let rounds = 20;
        let started = Instant::now();
        for _ in 0..rounds {
            std::hint::black_box(source.cut(recorded.tile, recorded.jpeg).unwrap());
        }
        eprintln!(
            "cut {} {}: {:?}",
            recorded.name,
            recorded.tile,
            started.elapsed() / rounds
        );
    }
    let mut placed = 0u32;
    let started = Instant::now();
    for z in [13u8, 15, 19] {
        for recorded in &tiles {
            let shift = z as i32 - recorded.tile.z() as i32;
            let (x, y) = if shift >= 0 {
                (recorded.tile.x() << shift, recorded.tile.y() << shift)
            } else {
                (recorded.tile.x() >> -shift, recorded.tile.y() >> -shift)
            };
            for dy in 0..20 {
                for dx in 0..20 {
                    if let Some(tile) = TileAddress::new(z, x + dx, y + dy) {
                        std::hint::black_box(source.classify(tile));
                        placed += 1;
                    }
                }
            }
        }
    }
    eprintln!(
        "placed {placed} tiles: {:?} each",
        started.elapsed() / placed
    );
}
