//! The reader on the files recorded from the two providers whose radar
//! products it is for: a frame of DWD's composite RY (ODIM_H5) and a frame
//! of GeoSphere Austria's INCA nowcast (netCDF-4). The files belong to the
//! adapters' crates, where their NOTICE says what they are.

use venti_hdf5::{File, Value};

const DWD: &[u8] = include_bytes!(
    "../../sources-dwd-radar/tests/fixtures/raa01-ry_10000-2611211140-dwd---bin.hdf5"
);
const INCA: &[u8] =
    include_bytes!("../../sources-geosphere-radar/tests/fixtures/rr-202611211200.nc");

fn value<'a>(attributes: &'a [(String, Value)], name: &str) -> &'a Value {
    attributes
        .iter()
        .find(|(attribute, _)| attribute == name)
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("no {name}"))
}

#[test]
fn dwds_frame_has_a_superblock_of_version_0_and_one_packed_picture() {
    assert_eq!(DWD[8], 0);
    let file = File::open(DWD).unwrap();

    // Attributes in a header, and thirteen in a heap of one block.
    let what = file.attributes(&file.object(&["what"]).unwrap()).unwrap();
    assert_eq!(value(&what, "object").text(), Some("COMP"));
    let place = file.attributes(&file.object(&["where"]).unwrap()).unwrap();
    assert_eq!(place.len(), 13);
    assert_eq!(value(&place, "xsize").number(), Some(1100.0));

    let picture = file.object(&["dataset1", "data1", "data"]).unwrap();
    assert_eq!(file.shape(&picture).unwrap(), [1200, 1100]);
    let raw = file.grid_u16(&picture, 1200, 1100).unwrap();
    assert_eq!(raw.len(), 1_320_000);
    assert_eq!(raw.iter().filter(|v| **v == 65535).count(), 627_073);

    // It is none of the other kinds of dataset.
    let said = file.shuffled_i16(&picture, &[1200, 1100]).unwrap_err();
    assert!(said.contains("not signed 16-bit integers"), "{said}");
    let said = file.numbers(&picture, 1200).unwrap_err();
    assert!(said.contains("not 1200 numbers"), "{said}");
    assert!(
        file.object(&["dataset2"])
            .unwrap_err()
            .contains("no /dataset2")
    );
}

#[test]
fn geospheres_frame_has_a_superblock_of_version_2_and_shuffled_pictures() {
    assert_eq!(INCA[8], 2);
    let file = File::open(INCA).unwrap();

    // The rain: sixteen-bit integers of three dimensions, shuffled and
    // packed, with fourteen attributes in a heap of two blocks, twelve
    // of them one value.
    let rain = file.object(&["rr"]).unwrap();
    assert_eq!(file.shape(&rain).unwrap(), [1, 252, 600]);
    let attributes = file.attributes(&rain).unwrap();
    assert_eq!(attributes.len(), 12);
    assert_eq!(value(&attributes, "scale_factor"), &Value::Float(0.01));
    assert_eq!(value(&attributes, "missing_value"), &Value::Int(-999));
    assert_eq!(value(&attributes, "_FillValue"), &Value::Int(-999));
    assert_eq!(value(&attributes, "units").text(), Some("kg m-2"));
    let raw = file.shuffled_i16(&rain, &[1, 252, 600]).unwrap();
    assert_eq!(raw.len(), 151_200);
    // No rain at four points of five, 2.38 mm in the quarter of an hour
    // at the wettest, and a value everywhere.
    assert_eq!(raw.iter().filter(|v| **v == 0).count(), 123_775);
    assert_eq!(raw.iter().filter(|v| **v == 1).count(), 5721);
    assert_eq!((raw.iter().min(), raw.iter().max()), (Some(&0), Some(&238)));

    // The degrees of every point: floats, shuffled and packed.
    let lat = file
        .shuffled_f32(&file.object(&["lat"]).unwrap(), &[252, 600])
        .unwrap();
    let lon = file
        .shuffled_f32(&file.object(&["lon"]).unwrap(), &[252, 600])
        .unwrap();
    assert!((lat[0] - 46.234_165).abs() < 1e-6 && (lon[0] - 9.415_482_5).abs() < 1e-6);
    assert!((lat[151_199] - 48.492_016).abs() < 1e-6 && (lon[151_199] - 17.353_31).abs() < 1e-5);

    // Rows of numbers, stored as they are: the grid's metres as integers
    // of four bytes, the time as one of eight, the lead time as a double.
    let numbers = |name: &str, length: usize| {
        file.numbers(&file.object(&[name]).unwrap(), length)
            .unwrap()
    };
    let (x, y) = (numbers("x", 600), numbers("y", 252));
    assert_eq!((x[0], x[1], x[599]), (98_000.0, 99_000.0, 697_000.0));
    assert_eq!((y[0], y[251]), (267_000.0, 518_000.0));
    assert_eq!(numbers("time", 1), [0.0]);
    assert_eq!(numbers("leadtime", 1), [0.0]);
    let time = file.attributes(&file.object(&["time"]).unwrap()).unwrap();
    assert_eq!(
        value(&time, "units").text(),
        Some("days since 2026-11-21 12:00:00")
    );
    // The file's own attributes, in a heap of two blocks as well.
    let root = file.attributes(&file.object(&[]).unwrap()).unwrap();
    assert_eq!(value(&root, "crs").text(), Some("EPSG:31287"));
}

#[test]
fn a_dataset_is_read_only_as_what_it_is() {
    let file = File::open(INCA).unwrap();
    let (rain, lat, x) = (
        file.object(&["rr"]).unwrap(),
        file.object(&["lat"]).unwrap(),
        file.object(&["x"]).unwrap(),
    );

    // Another size, another type, another way of packing or storing.
    let said = file.shuffled_i16(&rain, &[252, 600]).unwrap_err();
    assert!(said.contains("is [1, 252, 600], not [252, 600]"), "{said}");
    let said = file.shuffled_f32(&rain, &[1, 252, 600]).unwrap_err();
    assert!(said.contains("not 32-bit floats"), "{said}");
    let said = file.shuffled_i16(&lat, &[252, 600]).unwrap_err();
    assert!(said.contains("not signed 16-bit integers"), "{said}");
    let said = file.grid_u16(&rain, 252, 600).unwrap_err();
    assert!(said.contains("not 252 rows of 600"), "{said}");
    let said = file.numbers(&x, 599).unwrap_err();
    assert!(said.contains("is [600], not 599 numbers"), "{said}");
    let said = file.numbers(&x, 5000).unwrap_err();
    assert!(said.contains("more than this reader reads"), "{said}");
    let said = file.shuffled_i16(&x, &[600]).unwrap_err();
    assert!(said.contains("not signed 16-bit integers"), "{said}");

    // A picture whose packed bytes are cut off, and a file cut short.
    let mut damaged = INCA.to_vec();
    damaged[14_444 + 24] = 0x10;
    let said = File::open(&damaged)
        .unwrap()
        .shuffled_i16(&rain_of(&damaged), &[1, 252, 600])
        .unwrap_err();
    assert!(said.contains("unpack"), "{said}");
    assert!(File::open(&INCA[..4096]).unwrap_err().contains("cut short"));
}

fn rain_of(bytes: &[u8]) -> venti_hdf5::Object<'_> {
    File::open(bytes).unwrap().object(&["rr"]).unwrap()
}
