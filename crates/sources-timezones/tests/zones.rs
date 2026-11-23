//! What the crate makes of its asset, held to a golden file: the cells and
//! points it keeps, and the zone of places near borders and coasts.
//! Rewritten with `VENTI_UPDATE_GOLDEN=1` after the asset is made again;
//! read the diff.

use std::path::Path;

use serde_json::json;
use venti_sources::testing::assert_golden;

#[test]
fn the_asset_is_read_into_the_same_zones() {
    let summary = venti_sources_timezones::summary();
    // Places a few kilometres from a border between zones or from a
    // coast, west to east.
    let places = [
        ("Torrey Pines, California", -117.2516, 32.8899),
        ("Tijuana, Mexico", -117.0382, 32.5149),
        ("El Paso, Texas", -106.4850, 31.7619),
        ("Ciudad Juárez, Mexico", -106.4245, 31.6904),
        ("Geneva", 6.1432, 46.2044),
        ("Annemasse", 6.2350, 46.1934),
        ("Basel", 7.5886, 47.5596),
        ("Lörrach", 7.6616, 47.6156),
        ("Chiasso", 9.0316, 45.8326),
        ("Como", 9.0852, 45.8081),
        ("Vaduz", 9.5209, 47.1410),
        ("Feldkirch", 9.5980, 47.2378),
        ("Salzburg", 13.0550, 47.8095),
        ("Freilassing", 12.9775, 47.8371),
        ("Gorizia", 13.6217, 45.9409),
        ("Nova Gorica", 13.6488, 45.9560),
        ("Off Nice, 20 km out", 7.30, 43.50),
        ("The Adriatic, between Ancona and Split", 15.0, 43.2),
    ];
    let places: Vec<_> = places
        .iter()
        .map(|&(name, lon, lat)| {
            json!({ "place": name, "zone": venti_sources_timezones::zone_at(lon, lat) })
        })
        .collect();
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/zones.json");
    assert_golden(
        &golden,
        &json!({
            "cell_units": summary.cell_units,
            "zones": summary.zones,
            "cells_without_zone": summary.cells_without_zone,
            "cells_of_one_zone": summary.cells_of_one_zone,
            "cells_shared": summary.cells_shared,
            "points": summary.points,
            "bytes": summary.bytes,
            "places": places,
        }),
    );
}
