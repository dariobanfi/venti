//! What the crate makes of its asset, held to a golden file: each country
//! with the number of segments of its outline, and the country of a few
//! places near borders. Rewritten with `VENTI_UPDATE_GOLDEN=1` after the
//! asset is made again; read the diff.

use std::path::Path;

use serde_json::json;
use venti_sources::testing::assert_golden;

#[test]
fn the_asset_is_read_into_the_same_countries() {
    let countries: Vec<_> = venti_sources_naturalearth::countries()
        .iter()
        .map(|(code, outline)| json!({ "code": code, "segments": outline.segment_count() }))
        .collect();
    // Places a few kilometres from a border, west to east. Chiasso (CH)
    // and Gorizia (IT) are each given the neighbour: Natural Earth's own
    // border, before any simplifying, passes on their other side (checked
    // on the file of 2026-11-23). Hence the finer outlines first.
    let places = [
        ("Geneva", 6.1432, 46.2044),
        ("Annemasse", 6.2350, 46.1934),
        ("Basel", 7.5886, 47.5596),
        ("Lörrach", 7.6616, 47.6156),
        ("Como", 9.0852, 45.8081),
        ("Chiasso", 9.0316, 45.8326),
        ("Salzburg", 13.0550, 47.8095),
        ("Freilassing", 12.9775, 47.8371),
        ("Gorizia", 13.6217, 45.9409),
        ("Nova Gorica", 13.6488, 45.9560),
    ];
    let places: Vec<_> = places
        .iter()
        .map(|&(name, lon, lat)| {
            json!({ "place": name, "country": venti_sources_naturalearth::country_at(lon, lat) })
        })
        .collect();
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/countries.json");
    assert_golden(
        &golden,
        &json!({ "countries": countries, "places": places }),
    );
}
