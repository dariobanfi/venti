//! Others in the air on the cockpit's map: the aircraft the Open Glider
//! Network hears around the glider, each with its track of the last
//! minutes.
//!
//! The server answers with where each aircraft was last heard and keeps no
//! track for anyone ([`crate::live_others`]). The cockpit asks again every
//! ten to fifteen seconds, and the session keeps what each answer said:
//! from those positions it draws each aircraft's track of the last five
//! minutes. A track has a point for each answer, so it is as fine as the
//! shell asks; an aircraft the network did not hear between two answers
//! has a straight line there.
//!
//! The network's positions are under its licence (the ODbL for the Open
//! Glider Network) and stay its own: they are kept apart from Venti's
//! pilots and shown with the network's credit.

use std::collections::{HashMap, VecDeque};

use venti_domain::GeoPoint;
use venti_flight::live::navigation::bearing_deg;

use crate::live_others::{LiveOthers, OtherAircraftView, live_others_path};
use crate::wind::Credit;
use crate::{Coordinate, area_around, live_age_in_words};

/// How much of each aircraft's track is kept, seconds.
const TRACK_S: f64 = 300.0;

/// An aircraft not heard for this long, seconds, is no longer drawn,
/// unless the server lists aircraft for longer.
const FORGET_S: f64 = 600.0;

/// Aircraft farther than this from the glider, metres, are not given.
const NEAR_M: f64 = 30_000.0;

/// The most aircraft given, the nearest.
const AT_MOST: usize = 50;

/// An aircraft of another network near the glider, as the cockpit's map
/// draws it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct OtherNearView {
    /// The aircraft as last heard. Its age is counted to the moment
    /// asked for.
    pub aircraft: OtherAircraftView,
    /// How far it is from the glider, metres.
    pub distance_m: f64,
    /// The direction to it from the glider, degrees clockwise from north.
    pub bearing_deg: f64,
    /// How far it is above the glider, metres; negative below. Absent
    /// without both heights.
    pub above_m: Option<f64>,
    /// Where it was heard in the last five minutes, oldest first, its
    /// latest position last: for a line on the map. One point for each
    /// answer of the server that had it.
    pub track: Vec<Coordinate>,
}

/// The aircraft of other networks around the glider.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct OthersNear {
    /// Nearest first, within 30 km, at most fifty.
    pub aircraft: Vec<OtherNearView>,
    /// Credits to show while they are drawn: those of the networks with
    /// an aircraft in the list.
    pub credits: Vec<Credit>,
    /// How many there are, in words: "3 others in the air". Empty when
    /// there are none.
    pub count_words: String,
}

/// One aircraft as the session has heard it.
#[derive(Debug, Clone)]
struct Heard {
    /// The network it was heard on.
    source: String,
    view: OtherAircraftView,
    /// When it was last heard, seconds since 1970-01-01 UTC.
    heard_s: f64,
    /// Where, oldest first, with when.
    track: VecDeque<(f64, Coordinate)>,
}

/// What the session has heard of others.
#[derive(Debug, Clone, Default)]
pub(crate) struct OthersHeard {
    aircraft: HashMap<String, Heard>,
    credits: HashMap<String, Credit>,
    forget_s: f64,
}

impl OthersHeard {
    /// Takes in an answer of the server, read at `now_s`.
    pub(crate) fn take(&mut self, others: LiveOthers, now_s: f64) {
        self.forget_s = FORGET_S.max(f64::from(others.listed_minutes) * 60.0);
        for network in others.networks {
            if let Some(credit) = network.credit {
                self.credits.insert(network.source.clone(), credit);
            }
            for view in network.aircraft {
                let heard_s = now_s - view.age_s;
                let entry = self
                    .aircraft
                    .entry(view.id.clone())
                    .or_insert_with(|| Heard {
                        source: network.source.clone(),
                        view: view.clone(),
                        heard_s,
                        track: VecDeque::new(),
                    });
                if entry.track.back().is_none_or(|(time, _)| heard_s > *time) {
                    entry.track.push_back((heard_s, view.position));
                    entry.heard_s = heard_s;
                    entry.source.clone_from(&network.source);
                    entry.view = view;
                }
            }
        }
        self.forget(now_s);
    }

    /// Lets go of what is too old to draw.
    fn forget(&mut self, now_s: f64) {
        let forget_s = self.forget_s.max(FORGET_S);
        self.aircraft
            .retain(|_, heard| now_s - heard.heard_s <= forget_s);
        for heard in self.aircraft.values_mut() {
            while heard
                .track
                .front()
                .is_some_and(|(time, _)| now_s - time > TRACK_S)
                && heard.track.len() > 1
            {
                heard.track.pop_front();
            }
        }
    }

    /// The aircraft near the glider at `now_s`.
    pub(crate) fn near(
        &self,
        glider: Option<GeoPoint>,
        altitude_m: Option<f64>,
        now_s: f64,
    ) -> OthersNear {
        let forget_s = self.forget_s.max(FORGET_S);
        let mut aircraft: Vec<(String, OtherNearView)> = match glider {
            None => Vec::new(),
            Some(here) => self
                .aircraft
                .values()
                .filter(|heard| now_s - heard.heard_s <= forget_s)
                .filter_map(|heard| {
                    let there =
                        GeoPoint::new(heard.view.position.lat, heard.view.position.lon).ok()?;
                    let distance_m = here.distance_m(there);
                    if distance_m > NEAR_M {
                        return None;
                    }
                    let age_s = (now_s - heard.heard_s).max(0.0);
                    let mut view = heard.view.clone();
                    view.age_s = age_s;
                    view.age_words = live_age_in_words(age_s, false);
                    Some((
                        heard.source.clone(),
                        OtherNearView {
                            above_m: view
                                .altitude_m
                                .zip(altitude_m)
                                .map(|(there, here)| there - here),
                            aircraft: view,
                            distance_m,
                            bearing_deg: bearing_deg(here, there),
                            track: heard
                                .track
                                .iter()
                                .filter(|(time, _)| now_s - time <= TRACK_S)
                                .map(|(_, place)| *place)
                                .collect(),
                        },
                    ))
                })
                .collect(),
        };
        aircraft.sort_by(|a, b| a.1.distance_m.total_cmp(&b.1.distance_m));
        aircraft.truncate(AT_MOST);
        let mut credits: Vec<Credit> = Vec::new();
        for (source, _) in &aircraft {
            if credits.iter().any(|credit| &credit.source == source) {
                continue;
            }
            credits.extend(self.credits.get(source).cloned());
        }
        let count_words = match aircraft.len() {
            0 => String::new(),
            1 => "1 other in the air".to_owned(),
            n => format!("{n} others in the air"),
        };
        OthersNear {
            aircraft: aircraft.into_iter().map(|(_, view)| view).collect(),
            credits,
            count_words,
        }
    }
}

/// The path and query to `GET` for the aircraft other networks hear
/// around the glider: the box 30 km to each side. Ask every ten to fifteen
/// seconds in the air, hand each answer, as [`crate::live_others`] reads
/// it, to [`crate::FlightSession::on_others`], and draw
/// [`crate::FlightSession::others_near`]. `None` for a place that is none.
#[uniffi::export]
pub fn cockpit_others_path(lat: f64, lon: f64) -> Option<String> {
    let area = area_around(lat, lon, NEAR_M).ok()?;
    live_others_path(area.south, area.west, area.north, area.east)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_others::{LiveOthersNetwork, OtherAircraftKind};

    fn aircraft(id: &str, lat: f64, age_s: f64, altitude_m: Option<f64>) -> OtherAircraftView {
        OtherAircraftView {
            id: id.to_owned(),
            kind: OtherAircraftKind::Paraglider,
            kind_words: "Paraglider".to_owned(),
            is_free_flight: true,
            position: Coordinate { lat, lon: 11.8 },
            altitude_m,
            climb_ms: None,
            ground_speed_ms: None,
            course_deg: None,
            age_s,
            age_words: String::new(),
            name: None,
            line: String::new(),
            label: String::new(),
        }
    }

    fn answer(aircraft: Vec<OtherAircraftView>) -> LiveOthers {
        LiveOthers {
            networks: vec![LiveOthersNetwork {
                source: "ogn".to_owned(),
                connected: true,
                aircraft,
                truncated: false,
                credit: Some(Credit {
                    source: "ogn".to_owned(),
                    provider: "Open Glider Network".to_owned(),
                    licence: "ODbL 1.0".to_owned(),
                    text: "Open Glider Network".to_owned(),
                    url: "https://www.glidernet.org".to_owned(),
                    notice: None,
                }),
            }],
            listed_minutes: 10,
        }
    }

    #[test]
    fn others_are_given_nearest_first_with_their_tracks_of_the_last_minutes() {
        let glider = GeoPoint::new(47.70, 11.80).ok();
        let mut heard = OthersHeard::default();
        assert!(heard.near(glider, Some(2000.0), 0.0).aircraft.is_empty());

        // Every fifteen seconds for ten minutes, one paraglider flies
        // north, a second stays where it is, and one far away.
        let start = 1_000_000.0;
        for step in 0..=40 {
            let now = start + 15.0 * f64::from(step);
            heard.take(
                answer(vec![
                    aircraft("a", 47.71 + 0.0005 * f64::from(step), 5.0, Some(2300.0)),
                    aircraft("b", 47.705, 2.0, None),
                    aircraft("far", 48.2, 2.0, None),
                ]),
                now,
            );
        }
        let now = start + 600.0;
        let near = heard.near(glider, Some(2000.0), now + 5.0);
        let ids: Vec<&str> = near
            .aircraft
            .iter()
            .map(|other| other.aircraft.id.as_str())
            .collect();
        assert_eq!(ids, ["b", "a"]);
        assert_eq!(near.count_words, "2 others in the air");
        assert_eq!(near.credits.len(), 1);
        let a = &near.aircraft[1];
        // Five minutes at one point every fifteen seconds.
        assert!((20..=21).contains(&a.track.len()), "{}", a.track.len());
        assert_eq!(a.track.last(), Some(&a.aircraft.position));
        assert!(a.track.windows(2).all(|pair| pair[0].lat < pair[1].lat));
        assert_eq!(a.above_m, Some(300.0));
        // Heard five seconds before the answer, which is five seconds old.
        assert_eq!(a.aircraft.age_s, 10.0);
        assert_eq!(near.aircraft[0].above_m, None);
        // The same answer twice is one point.
        let before = a.track.len();
        heard.take(answer(vec![aircraft("a", 47.73, 5.0, Some(2300.0))]), now);
        assert_eq!(
            heard.near(glider, None, now).aircraft[1].track.len(),
            before
        );

        // Not heard for longer than the server lists them: gone.
        let later = heard.near(glider, None, now + 700.0);
        assert!(later.aircraft.is_empty());
        assert!(later.count_words.is_empty() && later.credits.is_empty());
        // Without a position of the glider nothing is near.
        assert!(heard.near(None, None, now).aircraft.is_empty());
    }

    #[test]
    fn the_cockpit_asks_for_the_box_around_the_glider() {
        let path = cockpit_others_path(47.7, 11.8).unwrap();
        assert!(path.contains("47.4"), "{path}");
        assert_eq!(cockpit_others_path(f64::NAN, 11.8), None);
    }
}
