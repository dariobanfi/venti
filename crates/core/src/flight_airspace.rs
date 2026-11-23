//! A finished flight checked against the airspace the app has.
//!
//! After a flight a pilot wants to know: was I in anything? The track is
//! run past the same airspace the cockpit warns from, and every stretch
//! inside an airspace a paraglider has to mind is listed, with where it
//! began and ended and how high the glider was.
//!
//! What this can and cannot say, and every screen that shows it says so:
//! it is the airspace the app has today, not the airspace of the day of
//! the flight; it knows nothing about what was active; and where a limit
//! is above ground or a flight level it can only say "possibly", because
//! it has no terrain and often no pressure altitude. It is a help for the
//! pilot's own review, not evidence of anything.

use venti_airspace::{Altitudes, Policy, PressureAltitude, Unknown, VerticalRelation};
use venti_flight::{analysis, igc, track};

use crate::airspace::{kind_in_words, limit_in_words, policy_view};
use crate::{
    AirspaceOrigin, AirspacePolicy, AirspaceSet, Coordinate, Core, CoreError, FlightMoment,
    LogbookError,
};

/// Two stretches inside the same airspace that lie closer together than
/// this, in seconds, are one: at an edge the position wanders in and out.
const SAME_STRETCH_WITHIN_S: i64 = 30;

/// How often along the flight the airspace's cover is looked at, seconds.
const LOOK_FOR_COVER_EVERY_S: i64 = 60;

/// How much of a flight the app's airspace can speak for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum AirspaceCover {
    /// The app has no airspace that reaches the flight: nothing was
    /// checked.
    None,
    /// Part of the flight lies where the app has no airspace. Of that part
    /// nothing is known.
    Partly,
    /// The app has airspace for the whole flight.
    Whole,
}

/// One stretch of a flight inside an airspace, or possibly inside it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct AirspaceCrossing {
    /// The airspace's identifier.
    pub id: String,
    /// Its name as published. May be empty.
    pub name: String,
    /// What it is, in words: "Class D control zone".
    pub kind: String,
    /// Its floor as published.
    pub lower: String,
    /// Its ceiling as published.
    pub upper: String,
    /// How the app treats it: to avoid, or deserving attention.
    pub policy: AirspacePolicy,
    /// The first moment inside.
    pub entry: FlightMoment,
    /// The last moment inside.
    pub exit: FlightMoment,
    /// Seconds from the first to the last.
    pub seconds: u32,
    /// The lowest the glider was while inside, metres.
    pub lowest_m: f64,
    /// The highest, metres.
    pub highest_m: f64,
    /// Set when the glider may in truth have been clear of the airspace's
    /// height band all the time, with what is missing to tell, in words. A
    /// screen must show it: the stretch is then "possibly inside".
    pub uncertain: Option<String>,
    /// Whether the airspace is from a file of the pilot's or from the
    /// pilot's server.
    pub origin: AirspaceOrigin,
    /// The file's name, or the provider's.
    pub source: String,
    /// See [`crate::AirspaceWarning::data_unix_s`].
    pub data_unix_s: Option<i64>,
}

/// A flight checked against the airspace the app has.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FlightAirspace {
    /// How much of the flight could be checked.
    pub cover: AirspaceCover,
    /// The stretches inside airspace to avoid or to mind, in flight order.
    /// Airspace a paraglider may simply fly through is not listed.
    pub crossings: Vec<AirspaceCrossing>,
}

#[uniffi::export]
impl Core {
    /// Checks one flight of the logbook against the airspace the app has:
    /// the pilot's files and what is kept from the server. Moments count
    /// from the take-off, like the track `flight` returns.
    pub fn flight_airspace(&self, id: String) -> Result<FlightAirspace, CoreError> {
        let text = self.logbook.flight_text(&id)?;
        self.flight_airspace_from_file(text)
    }

    /// Checks a flight that is not in the logbook against the airspace the
    /// app has, from its file's text: one of the pilot's flights as the
    /// server hands it out, on the website, which keeps no logbook. The
    /// same check as [`Core::flight_airspace`].
    pub fn flight_airspace_from_file(&self, igc_text: String) -> Result<FlightAirspace, CoreError> {
        let file = igc::parse(&igc_text).map_err(LogbookError::Invalid)?;
        let set = self.airspace()?;
        Ok(check(&set, analysis::flown_part(&file.fixes)))
    }
}

/// A stretch inside one airspace while it is being followed.
struct Open<'a> {
    airspace: &'a venti_domain::Airspace,
    entry: usize,
    exit: usize,
    lowest_m: i32,
    highest_m: i32,
    /// Whether every fix of it was inside only for want of knowledge, and
    /// of what.
    only_possibly: bool,
    missing: Option<Unknown>,
}

fn check(set: &AirspaceSet, fixes: &[igc::Fix]) -> FlightAirspace {
    let altitudes = track::altitudes(fixes);
    // A recorder without a pressure sensor writes zero throughout.
    let has_pressure = fixes.iter().any(|fix| fix.pressure_altitude_m != 0);
    let seconds = |index: usize| (fixes[index].time - fixes[0].time).num_seconds();

    let mut open: Vec<Open<'_>> = Vec::new();
    let mut closed: Vec<Open<'_>> = Vec::new();
    let (mut looked, mut covered, mut last_look) = (0u32, 0u32, None::<i64>);

    for (index, fix) in fixes.iter().enumerate() {
        let now = seconds(index);
        if last_look.is_none_or(|last| now - last >= LOOK_FOR_COVER_EVERY_S) {
            last_look = Some(now);
            looked += 1;
            covered += u32::from(set.covers(fix.position));
        }

        let here = Altitudes {
            altitude_m: f64::from(altitudes[index]),
            // A flight file holds no QNH: limits above sea level are
            // compared with the recorded altitude.
            qnh_altitude_m: None,
            pressure: has_pressure
                .then(|| PressureAltitude::standard(f64::from(fix.pressure_altitude_m))),
            // No terrain model yet: a limit above ground is taken to
            // reach the glider, and the stretch says "possibly".
            ground_elevation_m: None,
        };
        for found in set.index.at(fix.position, &here) {
            if found.vertical.relation != VerticalRelation::Inside
                || set.policy.policy_for(found.airspace) == Policy::Free
            {
                continue;
            }
            match open
                .iter_mut()
                .find(|stretch| stretch.airspace.id == found.airspace.id)
            {
                Some(stretch) => {
                    stretch.exit = index;
                    stretch.lowest_m = stretch.lowest_m.min(altitudes[index]);
                    stretch.highest_m = stretch.highest_m.max(altitudes[index]);
                    stretch.only_possibly &= found.vertical.uncertain;
                    stretch.missing = stretch.missing.or(found.vertical.missing());
                }
                None => open.push(Open {
                    airspace: found.airspace,
                    entry: index,
                    exit: index,
                    lowest_m: altitudes[index],
                    highest_m: altitudes[index],
                    only_possibly: found.vertical.uncertain,
                    missing: found.vertical.missing(),
                }),
            }
        }
        // A stretch not seen for a while has ended.
        let (still, ended): (Vec<_>, Vec<_>) = open
            .into_iter()
            .partition(|stretch| now - seconds(stretch.exit) < SAME_STRETCH_WITHIN_S);
        open = still;
        closed.extend(ended);
    }
    closed.extend(open);
    closed.sort_by_key(|stretch| (stretch.entry, stretch.airspace.id.to_string()));

    let moment = |index: usize| FlightMoment {
        track_index: u32::try_from(index).unwrap_or(u32::MAX),
        seconds: u32::try_from(seconds(index)).unwrap_or(0),
        position: Coordinate {
            lat: fixes[index].position.lat(),
            lon: fixes[index].position.lon(),
        },
        altitude_m: f64::from(altitudes[index]),
    };
    let crossings = closed
        .iter()
        .map(|stretch| {
            let (origin, source, data_unix_s) = set.origin(stretch.airspace);
            AirspaceCrossing {
                id: stretch.airspace.id.to_string(),
                name: stretch.airspace.name.clone(),
                kind: kind_in_words(stretch.airspace),
                lower: limit_in_words(&stretch.airspace.lower),
                upper: limit_in_words(&stretch.airspace.upper),
                policy: policy_view(set.policy.policy_for(stretch.airspace)),
                entry: moment(stretch.entry),
                exit: moment(stretch.exit),
                seconds: u32::try_from(seconds(stretch.exit) - seconds(stretch.entry))
                    .unwrap_or(0),
                lowest_m: f64::from(stretch.lowest_m),
                highest_m: f64::from(stretch.highest_m),
                uncertain: stretch.only_possibly.then(|| {
                    match stretch.missing {
                        Some(Unknown::GroundElevation) => {
                            "Possibly clear: the app does not know the height of the ground there."
                        }
                        Some(Unknown::Pressure) => {
                            "Possibly clear: the recording has no pressure altitude to place a flight level."
                        }
                        None => "Possibly clear: the airspace's height could not be placed exactly.",
                    }
                    .to_owned()
                }),
                origin,
                source,
                data_unix_s,
            }
        })
        .collect();

    FlightAirspace {
        cover: match (covered, looked) {
            (0, _) => AirspaceCover::None,
            (covered, looked) if covered == looked => AirspaceCover::Whole,
            _ => AirspaceCover::Partly,
        },
        crossings,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use venti_flight::testing::synthetic_flight;

    use super::*;
    use crate::testing::TempFolder;

    const NOW: i64 = 1_795_000_000;

    fn core(folder: &TempFolder) -> Arc<Core> {
        Core::new(folder.path().to_str().unwrap().to_owned()).unwrap()
    }

    fn zone(name: &str, limits: &str, shape: &str) -> String {
        format!("AC D\nAY CTR\nAN {name}\n{limits}\n{shape}\n\n")
    }

    #[test]
    fn a_flight_is_checked_against_the_airspace_the_app_has() {
        let folder = TempFolder::new();
        let core = core(&folder);
        let id = core
            .import_flight(igc::write(&synthetic_flight()))
            .unwrap()
            .id;

        // Without airspace nothing is checked, and it says so.
        let nothing = core.flight_airspace(id.clone()).unwrap();
        assert_eq!(nothing.cover, AirspaceCover::None);
        assert!(nothing.crossings.is_empty());

        // The zone the flight ends beside, one of a mile around the
        // flight's first thermal, one high above the flight, and one far
        // away.
        let flight = core.flight(id.clone()).unwrap();
        let thermal = flight.track[200];
        let degrees = |value: f64| {
            let whole = value.floor();
            let minutes = (value - whole) * 60.0;
            format!(
                "{whole:02.0}:{:02.0}:{:02.0}",
                minutes.floor(),
                (minutes.fract() * 60.0).floor()
            )
        };
        let around_thermal = format!(
            "V X={} N 0{} E\nDC 0.5",
            degrees(thermal.lat),
            degrees(thermal.lon)
        );
        let file = [
            zone(
                "CTR TEST",
                "AL GND\nAH 9000ft AMSL",
                "V X=47:42:00 N 011:44:00 E\nDC 1",
            ),
            zone("THERMAL ZONE", "AL GND\nAH 9000ft AMSL", &around_thermal),
            zone("HIGH ZONE", "AL FL 195\nAH FL 245", &around_thermal),
            zone(
                "FAR ZONE",
                "AL GND\nAH FL 100",
                "V X=50:00:00 N 008:00:00 E\nDC 5",
            ),
        ]
        .concat();
        core.import_airspace("zones.txt".to_owned(), file.into_bytes(), NOW)
            .unwrap();

        let checked = core.flight_airspace(id).unwrap();
        assert_eq!(checked.cover, AirspaceCover::Whole);
        // The same flight's file from the server is checked the same.
        assert_eq!(
            core.flight_airspace_from_file(igc::write(&synthetic_flight()))
                .unwrap(),
            checked
        );
        // In flight order: the zone around the first thermal, which the
        // take-off lies in too, and the edge of the zone the second
        // thermal drifts into. Not the zone far above, nor the one far away.
        let names: Vec<&str> = checked
            .crossings
            .iter()
            .map(|crossing| crossing.name.as_str())
            .collect();
        assert_eq!(
            names,
            ["THERMAL ZONE", "CTR TEST"],
            "{:?}",
            checked.crossings
        );

        let first = &checked.crossings[0];
        assert_eq!(first.kind, "Class D control zone");
        assert_eq!(first.policy, AirspacePolicy::Avoid);
        assert_eq!(
            (first.lower.as_str(), first.upper.as_str()),
            ("GND", "9000 ft AMSL")
        );
        // From the take-off through the thermal and out on the glide.
        assert_eq!(first.entry.track_index, 0);
        assert!(first.exit.track_index > 342, "{first:?}");
        assert_eq!(first.seconds, first.exit.seconds - first.entry.seconds);
        assert!(first.highest_m > first.lowest_m + 500.0);
        // Ground to an altitude above the sea: nothing in doubt.
        assert_eq!(first.uncertain, None);
        assert_eq!(first.origin, AirspaceOrigin::File);
        assert_eq!(first.source, "zones");

        // A minute inside the other, twelve minutes into the flight; the
        // moments are places on the flight's own track.
        let second = &checked.crossings[1];
        assert!(second.entry.seconds > 600 && second.seconds > 30 && second.seconds < 180);
        assert_eq!(
            flight.track[second.entry.track_index as usize].lat,
            second.entry.position.lat
        );
        assert!(second.entry.track_index > first.exit.track_index);
    }

    #[test]
    fn a_limit_above_ground_makes_a_stretch_only_possible() {
        let folder = TempFolder::new();
        let core = core(&folder);
        let id = core
            .import_flight(igc::write(&synthetic_flight()))
            .unwrap()
            .id;
        // The whole valley, from 300 m above ground: without terrain the
        // app cannot tell whether the glider was above that floor.
        let file = "AC R\nAN ED-R VALLEY\nAL 1000ft AGL\nAH FL 100\n\
                    DP 47:30:00 N 011:30:00 E\nDP 47:30:00 N 012:00:00 E\n\
                    DP 47:50:00 N 012:00:00 E\nDP 47:50:00 N 011:30:00 E\n";
        core.import_airspace("r.txt".to_owned(), file.as_bytes().to_vec(), NOW)
            .unwrap();

        let checked = core.flight_airspace(id).unwrap();
        assert_eq!(checked.crossings.len(), 1, "{:?}", checked.crossings);
        let crossing = &checked.crossings[0];
        assert_eq!(crossing.name, "ED-R VALLEY");
        assert_eq!(crossing.entry.track_index, 0);
        assert!(
            crossing
                .uncertain
                .as_deref()
                .is_some_and(|words| words.starts_with("Possibly clear")),
            "{crossing:?}"
        );
    }
}
