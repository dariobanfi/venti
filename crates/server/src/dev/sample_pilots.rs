//! Seven made-up pilots to test the groups with: names, countries, a
//! drawn picture each, flights made by the flight generator from take-offs
//! of the recorded flying sites, and places in the development mode's
//! public groups ([`super::DEMO_COMMUNITIES`]) with a message or two.
//!
//! The development mode does not start with them: its answers, and the
//! apps' tests that count them, stay as they are. `tools/sample-community/`
//! puts them into a running server through its API, from what `cargo run
//! -p venti-server --example sample_community` writes. No real pilot's
//! flight is copied: every track is the generator's, and every name,
//! picture and word is made up.

use chrono::{Duration, NaiveDate};
use venti_flight::igc::{self, IgcFile};
use venti_flight::testing::{Leg, made_up_flight_from};

/// A take-off of the recorded flying sites a sample pilot flies from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Takeoff {
    /// What pilots call it.
    pub name: &'static str,
    /// Where a flight from it starts, latitude: within a hundred metres of
    /// the take-off in the recorded sites, so that the flight counts for it.
    pub lat: f64,
    /// And longitude.
    pub lon: f64,
    /// The height a flight starts at, metres.
    pub altitude_m: f64,
    /// About the height of the valley it lands in, metres.
    pub valley_m: f64,
}

const BRAUNECK: Takeoff = Takeoff {
    name: "Brauneck",
    lat: 47.6634,
    lon: 11.5228,
    altitude_m: 1500.0,
    valley_m: 700.0,
};
const BLOMBERG: Takeoff = Takeoff {
    name: "Blomberg",
    lat: 47.7345,
    lon: 11.5066,
    altitude_m: 1200.0,
    valley_m: 700.0,
};
const HOCHRIES: Takeoff = Takeoff {
    name: "Hochries",
    lat: 47.7485,
    lon: 12.2502,
    altitude_m: 1520.0,
    valley_m: 600.0,
};
const KAMPENWAND: Takeoff = Takeoff {
    name: "Kampenwand",
    lat: 47.7537,
    lon: 12.3524,
    altitude_m: 1460.0,
    valley_m: 620.0,
};
const HOCHFELLN: Takeoff = Takeoff {
    name: "Hochfelln",
    lat: 47.7627,
    lon: 12.5598,
    altitude_m: 1640.0,
    valley_m: 560.0,
};
const UNTERBERGHORN: Takeoff = Takeoff {
    name: "Unterberghorn",
    lat: 47.6320,
    lon: 12.4315,
    altitude_m: 1700.0,
    valley_m: 600.0,
};
const MARKBACHJOCH: Takeoff = Takeoff {
    name: "Markbachjoch",
    lat: 47.4320,
    lon: 12.0940,
    altitude_m: 1500.0,
    valley_m: 830.0,
};

/// One sample pilot.
#[derive(Debug, Clone, Copy)]
pub struct SamplePilot {
    /// Their handle, the first name in lower case.
    pub handle: &'static str,
    /// First name and surname.
    pub name: &'static str,
    /// ISO 3166-1 alpha-2, upper case.
    pub country: &'static str,
    /// Where they live, in words: for the README and the report only.
    pub lives: &'static str,
    /// Their picture as drawn by `assets/sample-pilots/make.py`, a PNG.
    pub picture: &'static [u8],
    /// Their flights: from where, on which day of 2025 (month, day).
    pub flights: &'static [(Takeoff, (u32, u32))],
    /// The public groups they are in, by their place in
    /// [`super::DEMO_COMMUNITIES`], each with how many days ago they came
    /// in.
    pub groups: &'static [(usize, i64)],
    /// Their messages: the group's place, minutes ago, text. The script
    /// posts them oldest first.
    pub messages: &'static [(usize, i64, &'static str)],
}

/// The places in [`super::DEMO_COMMUNITIES`].
const MUNICH: usize = 0;
const ROSENHEIM: usize = 1;
const HOCHRIES_SCHOOL: usize = 2;
const TEGERNSEE: usize = 3;
const INNTAL: usize = 4;
const GARMISCH: usize = 5;

/// The seven, two of them around Munich.
pub const SAMPLE_PILOTS: [SamplePilot; 7] = [
    SamplePilot {
        handle: "sophie",
        name: "Sophie Huber",
        country: "DE",
        lives: "Munich",
        picture: include_bytes!("../../assets/sample-pilots/sophie.png"),
        flights: &[(BRAUNECK, (8, 2)), (BRAUNECK, (8, 23)), (BLOMBERG, (9, 14))],
        groups: &[(MUNICH, 45), (TEGERNSEE, 30)],
        messages: &[
            (
                MUNICH,
                6000,
                "Hi all, Sophie from Munich. Mostly Brauneck and Blomberg, happy to share the drive.",
            ),
            (
                TEGERNSEE,
                4000,
                "Brauneck was busy on Sunday, the north side worked from noon.",
            ),
        ],
    },
    SamplePilot {
        handle: "felix",
        name: "Felix Brandl",
        country: "DE",
        lives: "Munich",
        picture: include_bytes!("../../assets/sample-pilots/felix.png"),
        flights: &[
            (HOCHRIES, (8, 9)),
            (HOCHRIES, (9, 6)),
            (KAMPENWAND, (10, 4)),
        ],
        groups: &[(MUNICH, 40), (ROSENHEIM, 35), (HOCHRIES_SCHOOL, 25)],
        messages: &[
            (
                ROSENHEIM,
                7000,
                "Hochries north was working well last week, base at 2300 m.",
            ),
            (
                HOCHRIES_SCHOOL,
                5000,
                "Does the cable car still take gliders on the last run up?",
            ),
        ],
    },
    SamplePilot {
        handle: "kathi",
        name: "Katharina Moser",
        country: "AT",
        lives: "Kufstein",
        picture: include_bytes!("../../assets/sample-pilots/kathi.png"),
        flights: &[
            (UNTERBERGHORN, (8, 16)),
            (UNTERBERGHORN, (9, 20)),
            (MARKBACHJOCH, (10, 11)),
        ],
        groups: &[(INNTAL, 50), (ROSENHEIM, 20)],
        messages: &[(
            INNTAL,
            8000,
            "Servus from Kufstein! The Unterberghorn is my home hill, ask me about landing in Kössen.",
        )],
    },
    SamplePilot {
        handle: "giulia",
        name: "Giulia Bertolini",
        country: "IT",
        lives: "Bolzano, in Munich for the winter",
        picture: include_bytes!("../../assets/sample-pilots/giulia.png"),
        flights: &[
            (KAMPENWAND, (8, 30)),
            (KAMPENWAND, (9, 27)),
            (HOCHRIES, (10, 18)),
        ],
        groups: &[(MUNICH, 15), (ROSENHEIM, 12)],
        messages: &[
            (
                MUNICH,
                3000,
                "Ciao! I'm in Munich until spring and looking for people to fly with.",
            ),
            (
                ROSENHEIM,
                2600,
                "Kampenwand on Saturday? I can take two in my car.",
            ),
        ],
    },
    SamplePilot {
        handle: "reto",
        name: "Reto Ammann",
        country: "CH",
        lives: "Bern",
        picture: include_bytes!("../../assets/sample-pilots/reto.png"),
        flights: &[(BLOMBERG, (8, 5)), (BLOMBERG, (9, 2)), (BRAUNECK, (9, 28))],
        groups: &[(TEGERNSEE, 22), (GARMISCH, 18)],
        messages: &[
            (
                GARMISCH,
                6500,
                "Grüezi, here for two weeks from Bern. Any tips for the Wank?",
            ),
            (
                TEGERNSEE,
                3500,
                "Blomberg was smooth this morning, a light north-easterly.",
            ),
        ],
    },
    SamplePilot {
        handle: "camille",
        name: "Camille Durand",
        country: "FR",
        lives: "Annecy",
        picture: include_bytes!("../../assets/sample-pilots/camille.png"),
        flights: &[
            (HOCHFELLN, (8, 12)),
            (HOCHFELLN, (9, 10)),
            (HOCHRIES, (10, 25)),
        ],
        groups: &[(MUNICH, 30), (HOCHRIES_SCHOOL, 28), (ROSENHEIM, 26)],
        messages: &[(
            HOCHRIES_SCHOOL,
            6000,
            "Bonjour! Is there a training day for thermal flying this month?",
        )],
    },
    SamplePilot {
        handle: "nejc",
        name: "Nejc Kovač",
        country: "SI",
        lives: "Kranj",
        picture: include_bytes!("../../assets/sample-pilots/nejc.png"),
        flights: &[
            (MARKBACHJOCH, (8, 19)),
            (MARKBACHJOCH, (9, 17)),
            (UNTERBERGHORN, (10, 8)),
        ],
        groups: &[(INNTAL, 33), (GARMISCH, 21)],
        messages: &[
            (
                INNTAL,
                4500,
                "Markbachjoch to Kufstein and back yesterday, the inversion held until two.",
            ),
            (
                GARMISCH,
                2500,
                "Zdravo! Who is up for a day at the Wank next week?",
            ),
        ],
    },
];

/// The legs of a made-up flight from `takeoff`, different for each `seed`:
/// a glide out, three climbs with glides between them that never go lower
/// than 300 m over the valley, and a glide down to the valley.
fn legs(seed: u32, takeoff: &Takeoff) -> Vec<Leg> {
    let s = f64::from(seed);
    let heading = |turn: f64| (turn + s * 37.0).rem_euclid(360.0);
    let floor = takeoff.valley_m + 300.0;
    let mut altitude = takeoff.altitude_m;
    let mut legs = Vec::new();
    let glide = |altitude: &mut f64, seconds: i64, turn: f64, lowest: f64| {
        let sink = 1.1;
        let most = ((*altitude - lowest) / sink).floor().max(0.0) as i64;
        let seconds = seconds.min(most);
        *altitude -= sink * seconds as f64;
        Leg::Glide {
            seconds,
            heading: heading(turn),
            speed: 10.5 + 0.25 * f64::from(seed % 4),
            climb: -sink,
        }
    };
    let climb = |altitude: &mut f64, circles: i64, rate: f64| {
        *altitude += 28.0 * circles as f64 * rate;
        Leg::Circles {
            circles,
            climb: rate,
        }
    };
    let first = glide(&mut altitude, 60 + i64::from(seed % 4) * 20, 0.0, floor);
    legs.push(first);
    legs.push(climb(
        &mut altitude,
        7 + i64::from(seed % 5),
        1.8 + 0.2 * f64::from(seed % 4),
    ));
    let out = glide(&mut altitude, 300 + i64::from(seed % 5) * 60, 70.0, floor);
    legs.push(out);
    legs.push(climb(
        &mut altitude,
        5 + i64::from(seed % 4),
        1.4 + 0.1 * f64::from(seed % 3),
    ));
    let across = glide(&mut altitude, 400 + i64::from(seed % 3) * 100, 200.0, floor);
    legs.push(across);
    legs.push(climb(&mut altitude, 4 + i64::from(seed % 3), 1.5));
    let back = glide(&mut altitude, 3600, 250.0, takeoff.valley_m);
    legs.push(back);
    legs
}

/// The flight files of a pilot, in the order of [`SamplePilot::flights`]:
/// the generator's, with the pilot's name in the header and a minute and a
/// half standing still after landing. `index` is the pilot's place in
/// [`SAMPLE_PILOTS`]. The same files on every call.
pub fn flights(index: usize, pilot: &SamplePilot) -> Vec<Vec<u8>> {
    pilot
        .flights
        .iter()
        .enumerate()
        .map(|(number, (takeoff, (month, day)))| {
            let seed = u32::try_from(index * 3 + number).unwrap_or(0);
            let date = NaiveDate::from_ymd_opt(2025, *month, *day).expect("a day of 2025");
            let start = date.and_hms_opt(10, 0, 0).expect("a time of day").and_utc()
                + Duration::minutes(i64::from(seed * 17 % 150));
            let flown = made_up_flight_from(
                (takeoff.lat, takeoff.lon, takeoff.altitude_m),
                start,
                &legs(seed, takeoff),
            );
            let mut fixes = flown.fixes;
            if let Some(last) = fixes.last().copied() {
                for second in 1..=90 {
                    let mut standing = last;
                    standing.time += Duration::seconds(second);
                    fixes.push(standing);
                }
            }
            let file = IgcFile {
                pilot: Some(pilot.name.to_owned()),
                fixes,
                ..flown
            };
            igc::write(&file).into_bytes()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::is_reserved;
    use crate::flights::derive::derive;
    use venti_api_types::v1::{Handle, parse_display_name};

    #[test]
    fn the_sample_pilots_are_seven_from_six_countries_with_flights_of_their_own() {
        let mut countries: Vec<&str> = SAMPLE_PILOTS.iter().map(|p| p.country).collect();
        countries.sort_unstable();
        countries.dedup();
        assert_eq!(countries, ["AT", "CH", "DE", "FR", "IT", "SI"]);

        let mut identities = Vec::new();
        let mut days = Vec::new();
        for (index, pilot) in SAMPLE_PILOTS.iter().enumerate() {
            let handle = Handle::new(pilot.handle).unwrap();
            assert_eq!(handle.as_str(), pilot.handle);
            assert!(!is_reserved(&handle));
            assert_eq!(parse_display_name(pilot.name).unwrap(), pilot.name);
            assert!(pilot.picture.starts_with(b"\x89PNG"));
            assert!(pilot.picture.len() < 4096, "{}", pilot.handle);
            for (group, _) in pilot.groups {
                assert!(*group < super::super::DEMO_COMMUNITIES.len());
            }
            for (group, _, _) in pilot.messages {
                assert!(pilot.groups.iter().any(|(their, _)| their == group));
            }

            for ((takeoff, _), file) in pilot.flights.iter().zip(flights(index, pilot)) {
                let flight = derive(&file).unwrap();
                // It takes off at its take-off, lands, and is scored.
                let at = flight.derived.takeoff.unwrap().position;
                let off =
                    at.distance_m(venti_domain::GeoPoint::new(takeoff.lat, takeoff.lon).unwrap());
                assert!(
                    off < 100.0,
                    "{} from {}: {off} m",
                    pilot.handle,
                    takeoff.name
                );
                assert!(flight.derived.landing.is_some(), "{}", pilot.handle);
                assert!(flight.derived.score.is_some());
                let lowest = igc::parse(std::str::from_utf8(&file).unwrap())
                    .unwrap()
                    .fixes
                    .iter()
                    .map(|fix| fix.gps_altitude_m)
                    .min()
                    .unwrap();
                assert!(f64::from(lowest) >= takeoff.valley_m - 1.0, "{lowest}");
                identities.push(flight.identity);
                days.push(flight.derived.start.date_naive());
            }
        }
        // Twenty-one different flights, each on a day of its own.
        let all = identities.len();
        assert_eq!(all, 21);
        identities.sort();
        identities.dedup();
        assert_eq!(identities.len(), all);
        days.sort();
        days.dedup();
        assert_eq!(days.len(), all);
        // The same files on every call.
        assert_eq!(flights(0, &SAMPLE_PILOTS[0]), flights(0, &SAMPLE_PILOTS[0]));
    }
}
