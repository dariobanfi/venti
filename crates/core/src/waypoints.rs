//! The pilot's waypoints.
//!
//! They are kept as one SeeYou CUP file in the core's storage, written by
//! the flight engine: a pilot's waypoints stay a file that any other flying
//! program reads. Importing a file adds what is new and leaves alone what
//! is already there.

use std::io;

use venti_domain::GeoPoint;
use venti_flight::live::navigation::bearing_deg;
use venti_flight::waypoints::{self, Waypoint, WaypointError};

use crate::{Coordinate, Core, CoreError};

/// The name the waypoints are stored under.
const FILE: &str = "waypoints.cup";

/// Two waypoints of the same name this close together are the same one.
const SAME_PLACE_M: f64 = 50.0;

/// What a waypoint is, as far as it matters for flying.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
pub enum WaypointKind {
    /// A place to take off from.
    Takeoff,
    /// A place to land.
    Landing,
    /// Anything else: a turnpoint, a summit, a pass.
    Other,
}

/// One waypoint as a screen shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct WaypointView {
    /// Its identifier, to delete it or to fly to it.
    pub id: String,
    /// Its name.
    pub name: String,
    /// A short name, as flight instruments list it. May be empty.
    pub code: String,
    /// Where it is.
    pub position: Coordinate,
    /// Its height above sea level, metres, if known.
    pub elevation_m: Option<f64>,
    /// What it is.
    pub kind: WaypointKind,
    /// Whatever else its file said about it.
    pub description: String,
    /// How far it is from the place asked from, metres. Absent when the
    /// list was asked for without a place.
    pub distance_m: Option<f64>,
    /// The direction to it from that place, degrees clockwise from north.
    pub bearing_deg: Option<f64>,
}

/// What importing a waypoint file did.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct WaypointImport {
    /// How many waypoints were added.
    pub added: u32,
    /// How many the pilot already had.
    pub already_there: u32,
    /// How many lines of the file could not be read.
    pub skipped_lines: u32,
    /// The first of those lines and what was wrong with it, in words.
    pub first_problem: Option<String>,
}

/// The waypoints as a file to share or save elsewhere.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct WaypointsFile {
    /// A name to save it under.
    pub name: String,
    /// The file's text, in SeeYou CUP format.
    pub text: String,
}

#[uniffi::export]
impl Core {
    /// The pilot's waypoints. With a place given they are listed nearest
    /// first, each with its distance and direction from there; without,
    /// by name.
    pub fn waypoints(&self, from: Option<Coordinate>) -> Result<Vec<WaypointView>, CoreError> {
        let from = from.and_then(|from| GeoPoint::new(from.lat, from.lon).ok());
        let mut views: Vec<WaypointView> = self
            .stored_waypoints()?
            .iter()
            .map(|waypoint| view(waypoint, from))
            .collect();
        match from {
            Some(_) => views.sort_by(|a, b| {
                a.distance_m
                    .unwrap_or(f64::MAX)
                    .total_cmp(&b.distance_m.unwrap_or(f64::MAX))
            }),
            None => views.sort_by_cached_key(|view| (view.name.to_lowercase(), view.id.clone())),
        }
        Ok(views)
    }

    /// Adds the waypoints of a file (SeeYou `.cup`, or GEO `.wpt`) to the
    /// pilot's. A waypoint that is already there is not added again.
    pub fn import_waypoints(&self, text: String) -> Result<WaypointImport, CoreError> {
        let file = waypoints::parse(&text).map_err(|error| CoreError::InvalidFile {
            message: match error {
                WaypointError::UnknownFormat | WaypointError::Empty => error.to_string(),
            },
        })?;
        let mut stored = self.stored_waypoints()?;
        let mut import = WaypointImport {
            added: 0,
            already_there: 0,
            skipped_lines: u32::try_from(file.skipped.len()).unwrap_or(u32::MAX),
            first_problem: file
                .skipped
                .first()
                .map(|skipped| format!("Line {}: {}.", skipped.line, skipped.reason)),
        };
        for waypoint in file.waypoints {
            if stored.iter().any(|known| same(known, &waypoint)) {
                import.already_there += 1;
            } else {
                stored.push(waypoint);
                import.added += 1;
            }
        }
        if import.added > 0 {
            self.store_waypoints(&stored)?;
        }
        Ok(import)
    }

    /// Adds one waypoint, for example a place the pilot marked on the map.
    pub fn add_waypoint(
        &self,
        name: String,
        position: Coordinate,
        elevation_m: Option<f64>,
    ) -> Result<WaypointView, CoreError> {
        let name = name.trim().to_owned();
        if name.is_empty() {
            return Err(CoreError::InvalidFile {
                message: "A waypoint needs a name.".to_owned(),
            });
        }
        let point =
            GeoPoint::new(position.lat, position.lon).map_err(|_| CoreError::InvalidFile {
                message: "This position is not on the globe.".to_owned(),
            })?;
        let waypoint = Waypoint {
            name,
            code: String::new(),
            position: point,
            elevation_m: elevation_m.filter(|elevation| elevation.is_finite()),
            kind: waypoints::WaypointKind::Other,
            description: String::new(),
        };
        let mut stored = self.stored_waypoints()?;
        if !stored.iter().any(|known| same(known, &waypoint)) {
            stored.push(waypoint.clone());
            self.store_waypoints(&stored)?;
        }
        Ok(view(&waypoint, None))
    }

    /// Removes one waypoint. Removing one that is not there is not an error.
    pub fn delete_waypoint(&self, id: String) -> Result<(), CoreError> {
        let mut stored = self.stored_waypoints()?;
        let before = stored.len();
        stored.retain(|waypoint| identifier(waypoint) != id);
        if stored.len() != before {
            self.store_waypoints(&stored)?;
        }
        Ok(())
    }

    /// Removes every waypoint.
    pub fn delete_all_waypoints(&self) -> Result<(), CoreError> {
        self.storage.remove(FILE).map_err(storage_error)
    }

    /// The pilot's waypoints as a file for other programs.
    pub fn waypoints_file(&self) -> Result<WaypointsFile, CoreError> {
        Ok(WaypointsFile {
            name: "venti-waypoints.cup".to_owned(),
            text: waypoints::write_cup(&self.stored_waypoints()?),
        })
    }
}

impl Core {
    fn stored_waypoints(&self) -> Result<Vec<Waypoint>, CoreError> {
        let Some(bytes) = self.storage.read(FILE).map_err(storage_error)? else {
            return Ok(Vec::new());
        };
        // The file is the app's own writing. Should it be damaged, what can
        // still be read of it is kept.
        Ok(waypoints::parse(&String::from_utf8_lossy(&bytes))
            .map(|file| file.waypoints)
            .unwrap_or_default())
    }

    fn store_waypoints(&self, stored: &[Waypoint]) -> Result<(), CoreError> {
        self.storage
            .write(FILE, waypoints::write_cup(stored).as_bytes())
            .map_err(storage_error)
    }
}

fn storage_error(error: io::Error) -> CoreError {
    CoreError::Storage {
        message: format!("The waypoints could not be read or written: {error}."),
    }
}

/// Whether two waypoints are the same one: the same name, ignoring case,
/// at the same place.
fn same(a: &Waypoint, b: &Waypoint) -> bool {
    a.name.to_lowercase() == b.name.to_lowercase()
        && a.position.distance_m(b.position) < SAME_PLACE_M
}

/// A waypoint's identifier: stable for as long as its name and its place
/// stay what the file says, on every platform.
fn identifier(waypoint: &Waypoint) -> String {
    // The place as the file holds it: thousandths of a minute.
    let text = format!(
        "{}|{}|{}",
        waypoint.name,
        (waypoint.position.lat() * 60_000.0).round() as i64,
        (waypoint.position.lon() * 60_000.0).round() as i64,
    );
    // FNV-1a, 64 bits.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn view(waypoint: &Waypoint, from: Option<GeoPoint>) -> WaypointView {
    WaypointView {
        id: identifier(waypoint),
        name: waypoint.name.clone(),
        code: waypoint.code.clone(),
        position: Coordinate {
            lat: waypoint.position.lat(),
            lon: waypoint.position.lon(),
        },
        elevation_m: waypoint.elevation_m,
        kind: match waypoint.kind {
            waypoints::WaypointKind::Takeoff => WaypointKind::Takeoff,
            waypoints::WaypointKind::Landing => WaypointKind::Landing,
            waypoints::WaypointKind::Other => WaypointKind::Other,
        },
        description: waypoint.description.clone(),
        distance_m: from.map(|from| from.distance_m(waypoint.position)),
        bearing_deg: from.map(|from| bearing_deg(from, waypoint.position)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempFolder;

    const FILE_TEXT: &str = "name,code,country,lat,lon,elev,style,rwdir,rwlen,rwwidth,freq,desc\n\
\"Wallberg\",WB,DE,4739.800N,01147.600E,1620m,20,,,,,\"Start nach Norden\"\n\
\"Landeplatz Rottach\",LR,DE,4741.000N,01145.500E,740m,21,,,,,\n\
Zugspitze,ZS,DE,4725.260N,01059.090E,2962m,7,,,,,\n";

    fn core(folder: &TempFolder) -> std::sync::Arc<Core> {
        Core::new(folder.path().to_str().unwrap().to_owned()).unwrap()
    }

    #[test]
    fn a_file_is_imported_once_and_listed_by_name_or_by_distance() {
        let folder = TempFolder::new();
        let core = core(&folder);
        assert!(core.waypoints(None).unwrap().is_empty());

        let import = core.import_waypoints(FILE_TEXT.to_owned()).unwrap();
        assert_eq!(
            (import.added, import.already_there, import.skipped_lines),
            (3, 0, 0)
        );
        // The same file again, with one more line and a broken one.
        let again = format!(
            "{FILE_TEXT}Neu,N,DE,4700.000N,01100.000E,900m,1\nKaputt,K,DE,north,east,1m,1\n"
        );
        let import = core.import_waypoints(again).unwrap();
        assert_eq!(
            (import.added, import.already_there, import.skipped_lines),
            (1, 3, 1)
        );
        assert_eq!(
            import.first_problem.as_deref(),
            Some("Line 6: its latitude could not be read.")
        );

        let by_name: Vec<String> = core
            .waypoints(None)
            .unwrap()
            .into_iter()
            .map(|waypoint| waypoint.name)
            .collect();
        assert_eq!(
            by_name,
            ["Landeplatz Rottach", "Neu", "Wallberg", "Zugspitze"]
        );

        // From the Wallberg's summit: the take-off first, then the landing
        // 3.4 km to the north-west, the Zugspitze 65 km to the west-south-west.
        let near = core
            .waypoints(Some(Coordinate {
                lat: 47.6633,
                lon: 11.7933,
            }))
            .unwrap();
        assert_eq!(near[0].name, "Wallberg");
        assert!(near[0].distance_m.unwrap() < 100.0);
        assert_eq!(near[0].kind, WaypointKind::Takeoff);
        assert_eq!(near[0].elevation_m, Some(1620.0));
        assert_eq!(near[0].description, "Start nach Norden");
        assert_eq!(near[1].name, "Landeplatz Rottach");
        assert_eq!(near[1].kind, WaypointKind::Landing);
        assert!((near[1].distance_m.unwrap() - 3400.0).abs() < 200.0);
        assert!((near[1].bearing_deg.unwrap() - 310.0).abs() < 10.0);
        assert_eq!(near[2].name, "Zugspitze");
        assert!((near[2].distance_m.unwrap() - 65_000.0).abs() < 3000.0);
    }

    #[test]
    fn waypoints_survive_a_restart_and_leave_as_a_file_other_programs_read() {
        let folder = TempFolder::new();
        core(&folder)
            .import_waypoints(FILE_TEXT.to_owned())
            .unwrap();

        let core = core(&folder);
        let stored = core.waypoints(None).unwrap();
        assert_eq!(stored.len(), 3);

        let file = core.waypoints_file().unwrap();
        assert_eq!(file.name, "venti-waypoints.cup");
        assert!(
            file.text
                .starts_with("name,code,country,lat,lon,elev,style")
        );
        assert!(
            file.text
                .contains("\"Wallberg\",\"WB\",,4739.800N,01147.600E,1620.0m,20")
        );

        // Identifiers are the same after the round trip through the file.
        let ids: Vec<String> = stored.iter().map(|waypoint| waypoint.id.clone()).collect();
        core.delete_waypoint(ids[0].clone()).unwrap();
        core.delete_waypoint("no such waypoint".to_owned()).unwrap();
        let left = core.waypoints(None).unwrap();
        assert_eq!(left.len(), 2);
        assert!(left.iter().all(|waypoint| waypoint.id != ids[0]));

        core.delete_all_waypoints().unwrap();
        core.delete_all_waypoints().unwrap();
        assert!(core.waypoints(None).unwrap().is_empty());
    }

    #[test]
    fn a_place_marked_by_hand_becomes_a_waypoint() {
        let folder = TempFolder::new();
        let core = core(&folder);
        let here = Coordinate {
            lat: 47.5,
            lon: 11.5,
        };

        let added = core
            .add_waypoint("  Wiese  ".to_owned(), here, Some(812.0))
            .unwrap();
        assert_eq!(added.name, "Wiese");
        // Marking the same place twice keeps one.
        core.add_waypoint("wiese".to_owned(), here, None).unwrap();
        assert_eq!(core.waypoints(None).unwrap().len(), 1);

        for (name, position) in [
            ("", here),
            (
                "Mond",
                Coordinate {
                    lat: 120.0,
                    lon: 0.0,
                },
            ),
        ] {
            let error = core
                .add_waypoint(name.to_owned(), position, None)
                .unwrap_err();
            assert!(matches!(error, CoreError::InvalidFile { .. }), "{error:?}");
        }
    }

    #[test]
    fn a_file_that_is_no_waypoint_file_is_refused_in_words() {
        let folder = TempFolder::new();
        let error = core(&folder)
            .import_waypoints("AXXX001 Venti\nHFDTE040726\n".to_owned())
            .unwrap_err();
        assert!(matches!(error, CoreError::InvalidFile { .. }));
        assert!(error.to_string().contains(".cup"), "{error}");
    }
}
