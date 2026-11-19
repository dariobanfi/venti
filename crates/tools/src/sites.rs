//! Flying sites near a place, for an assistant.
//!
//! Answered in two steps like the wind tools: [`request`] says what to ask
//! a Venti server for, [`result`] turns its answer into the tool's result.
//!
//! The sites are what their source has. The first source is
//! OpenStreetMap, which anyone can edit and which has only part of the
//! sites: a site listed is not permission to fly there, and a site that is
//! not listed may well exist. The result says so every time.

use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};
use venti_api_types::v1::{SitesRequest, SitesResponse};
use venti_domain::{GeoPoint, Site, SiteUse, WindSector};

use crate::wind::{
    bearing_deg, box_around, compass, error_code, place_and_radius, refusal, sources, tenth,
    unreadable,
};
use crate::{Tool, ToolError};

/// How far around the place to look when the model does not say.
const DEFAULT_RADIUS_KM: f64 = 15.0;
/// The farthest it may look.
const MAX_RADIUS_KM: f64 = 50.0;
/// The most sites a result lists: the nearest.
const MAX_SITES: usize = 25;

/// What every result says about itself.
pub const CAVEATS: [&str; 3] = [
    "A site listed here is not permission to fly there. Many sites belong to a club or a \
     landowner and have rules; the pilot asks them and looks at the place.",
    "The list is not complete. Its source has only part of the flying sites, and well-known \
     ones may be missing or have no name; no site listed does not mean there is none.",
    "wind_from says which wind directions the source gives for a site. Most sites have none \
     given, and a direction says nothing about how strong a wind the site takes.",
];

/// The tools that read flying sites from a Venti server.
pub const SITES_TOOLS: [Tool; 1] = [Tool {
    name: "sites_near",
    description: "Paragliding and hang-gliding sites near a place, from the data a Venti \
        server holds: take-offs, landings, winch sites and training slopes, each with its name \
        where the source has one, what it is used for, its height, the wind directions it \
        works in where the source gives them, how far it is from the place and in which \
        direction, and whether the source marks it official or discouraged. Give the place as \
        coordinates; you may use your own knowledge of where a mountain, town or valley is. \
        Use it to find where one can take off or land in an area, or which sites suit a wind \
        direction. The list comes from OpenStreetMap, is incomplete, and a site on it is not \
        permission to fly there: pass on the caveats in the result and name the source.",
    read_only: true,
    schema: r#"{
        "type": "object",
        "properties": {
            "latitude": {"type": "number", "minimum": -90, "maximum": 90,
                         "description": "Latitude of the place in decimal degrees, north positive."},
            "longitude": {"type": "number", "minimum": -180, "maximum": 180,
                          "description": "Longitude of the place in decimal degrees, east positive."},
            "radius_km": {"type": "number", "minimum": 1, "maximum": 50,
                          "description": "How far around the place to look, in kilometres. 15 if left out."}
        },
        "required": ["latitude", "longitude"],
        "additionalProperties": false
    }"#,
}];

/// Whether a tool is one of [`SITES_TOOLS`].
pub fn needs_server(name: &str) -> bool {
    SITES_TOOLS.iter().any(|tool| tool.name == name)
}

/// What a tool needs from the server: the path and query to `GET`.
pub fn request(name: &str, input: &Value) -> Result<String, ToolError> {
    match name {
        "sites_near" => {
            let (place, radius_km) = place_and_radius(input, DEFAULT_RADIUS_KM, MAX_RADIUS_KM)?;
            Ok(SitesRequest {
                bbox: box_around(place, radius_km),
            }
            .path_and_query())
        }
        other => Err(ToolError::UnknownTool(other.to_owned())),
    }
}

/// The tool's result, as JSON text, from the server's answer to
/// [`request`]. `now` is the current time, which the age of the data is
/// measured against.
pub fn result(
    name: &str,
    input: &Value,
    status: u16,
    body: &[u8],
    now: DateTime<Utc>,
) -> Result<String, ToolError> {
    if !(200..300).contains(&status) {
        return Err(refusal(status, error_code(body).as_deref()));
    }
    match name {
        "sites_near" => {
            let (place, radius_km) = place_and_radius(input, DEFAULT_RADIUS_KM, MAX_RADIUS_KM)?;
            let answer: SitesResponse = serde_json::from_slice(body).map_err(unreadable)?;
            Ok(sites_near(&answer, place, radius_km, now).to_string())
        }
        other => Err(ToolError::UnknownTool(other.to_owned())),
    }
}

/// The sites within a radius of a place, the nearest first, as the model
/// reads them.
pub fn sites_near(
    answer: &SitesResponse,
    place: GeoPoint,
    radius_km: f64,
    now: DateTime<Utc>,
) -> Value {
    // The server was asked for a box; the tool promises a circle.
    let mut near: Vec<(f64, &Site)> = answer
        .layers
        .iter()
        .flat_map(|layer| &layer.sites)
        .map(|site| (place.distance_m(site.position) / 1000.0, site))
        .filter(|(distance_km, _)| *distance_km <= radius_km)
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    let found = near.len();
    near.truncate(MAX_SITES);

    let sites: Vec<Value> = near
        .iter()
        .map(|(distance_km, site)| {
            let mut fields = Map::new();
            if let Some(name) = &site.name {
                fields.insert("name".to_owned(), json!(name));
            }
            fields.insert(
                "used_for".to_owned(),
                json!(
                    site.uses
                        .iter()
                        .map(|used| use_word(*used))
                        .collect::<Vec<_>>()
                ),
            );
            fields.insert("distance_km".to_owned(), json!(tenth(*distance_km)));
            fields.insert(
                "direction".to_owned(),
                json!(compass(bearing_deg(place, site.position))),
            );
            fields.insert(
                "position".to_owned(),
                json!({"latitude": site.position.lat(), "longitude": site.position.lon()}),
            );
            if let Some(elevation) = site.elevation_m {
                fields.insert("elevation_m".to_owned(), json!(elevation.round()));
            }
            if !site.wind_directions.is_empty() {
                fields.insert(
                    "wind_from".to_owned(),
                    json!(
                        site.wind_directions
                            .iter()
                            .map(sector_words)
                            .collect::<Vec<_>>()
                    ),
                );
            }
            if site.official == Some(true) {
                fields.insert("official".to_owned(), json!(true));
            }
            if site.discouraged {
                fields.insert("discouraged".to_owned(), json!(true));
            }
            if site.paragliding == Some(false) {
                fields.insert("not_for_paragliders".to_owned(), json!(true));
            }
            if let Some(url) = &site.url {
                fields.insert("url".to_owned(), json!(url));
            }
            Value::Object(fields)
        })
        .collect();

    let mut result = Map::new();
    result.insert(
        "place".to_owned(),
        json!({"latitude": place.lat(), "longitude": place.lon()}),
    );
    result.insert("radius_km".to_owned(), json!(radius_km));
    result.insert("sites_found".to_owned(), json!(found));
    if found > sites.len() {
        result.insert(
            "listed".to_owned(),
            json!(format!(
                "the {} nearest; ask with a smaller radius to see fewer at a time",
                sites.len()
            )),
        );
    }
    result.insert("sites".to_owned(), Value::Array(sites));
    result.insert(
        "data".to_owned(),
        json!(
            answer
                .layers
                .iter()
                .map(|layer| {
                    // Whether the server's set of this source holds the
                    // place at all: outside it, nothing found says nothing.
                    let holds = layer.covers.iter().any(|covered| covered.contains(place));
                    json!({
                        "source": layer.source.as_str(),
                        "age_days": (now - layer.as_of).num_days().max(0),
                        "covers_this_place": holds,
                    })
                })
                .collect::<Vec<_>>()
        ),
    );
    result.insert("caveats".to_owned(), json!(CAVEATS));
    result.insert("sources".to_owned(), sources(&answer.attributions));
    Value::Object(result)
}

fn use_word(used_for: SiteUse) -> &'static str {
    match used_for {
        SiteUse::Takeoff => "take-off",
        SiteUse::Landing => "landing",
        SiteUse::Towing => "winch",
        SiteUse::Training => "training",
        SiteUse::Other => "other",
    }
}

/// A span of wind directions in compass points: "W", or "SW to NW".
fn sector_words(sector: &WindSector) -> String {
    let (from, to) = (compass(sector.from_deg), compass(sector.to_deg));
    if sector.to_deg - sector.from_deg >= 360.0 {
        "any".to_owned()
    } else if from == to {
        from.to_owned()
    } else {
        format!("{from} to {to}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The server's own golden answer of the endpoint.
    const ANSWER: &str = include_str!("../../server/tests/golden/sites.json");

    fn now() -> DateTime<Utc> {
        "2026-11-20T12:00:00Z".parse().unwrap()
    }

    #[test]
    fn the_sites_around_a_place_are_asked_for_as_a_box() {
        let input = json!({"latitude": 47.68, "longitude": 11.9, "radius_km": 10});
        assert!(needs_server("sites_near"));
        assert!(!needs_server("forecast_at"));
        let path = request("sites_near", &input).unwrap();
        assert!(path.starts_with("/v1/sites?bbox=11.7"), "{path}");
        assert!(matches!(
            request("nothing", &input),
            Err(ToolError::UnknownTool(_))
        ));
        assert!(matches!(
            request("sites_near", &json!({"latitude": 47.68})),
            Err(ToolError::BadArguments(_))
        ));
    }

    #[test]
    fn the_servers_sites_are_listed_nearest_first_with_what_they_are() {
        let answer: SitesResponse = serde_json::from_str(ANSWER).unwrap();
        let all: Vec<&Site> = answer
            .layers
            .iter()
            .flat_map(|layer| &layer.sites)
            .collect();
        assert!(!all.is_empty());
        // Around the first site of the answer.
        let centre = all[0].position;
        let input = json!({"latitude": centre.lat(), "longitude": centre.lon(), "radius_km": 50});
        let text = result("sites_near", &input, 200, ANSWER.as_bytes(), now()).unwrap();
        let result: Value = serde_json::from_str(&text).unwrap();

        let sites = result["sites"].as_array().unwrap();
        assert!(!sites.is_empty() && sites.len() <= MAX_SITES);
        assert_eq!(sites[0]["distance_km"], 0.0);
        let distances: Vec<f64> = sites
            .iter()
            .map(|site| site["distance_km"].as_f64().unwrap())
            .collect();
        assert!(
            distances.windows(2).all(|pair| pair[0] <= pair[1]),
            "{distances:?}"
        );
        assert!(distances.iter().all(|distance| *distance <= 50.0));
        for site in sites {
            assert!(site["used_for"].is_array());
            assert!(site["direction"].is_string());
            assert!(site["position"]["latitude"].is_number());
        }
        assert!(result["sites_found"].as_u64().unwrap() as usize >= sites.len());
        assert_eq!(result["caveats"].as_array().unwrap().len(), CAVEATS.len());
        assert!(
            result["sources"][0]
                .as_str()
                .unwrap()
                .contains("OpenStreetMap")
        );
        assert!(result["data"][0]["age_days"].as_i64().unwrap() >= 0);
        assert_eq!(result["data"][0]["covers_this_place"], true);
        // A model pays for every token of a result.
        assert!(text.len() < 12_000, "{}", text.len());
    }

    #[test]
    fn a_circle_is_cut_out_of_the_box_and_an_empty_one_says_what_that_is_worth() {
        // Far from every site of the answer, in the Atlantic.
        let input = json!({"latitude": 40.0, "longitude": -30.0, "radius_km": 5});
        let text = result("sites_near", &input, 200, ANSWER.as_bytes(), now()).unwrap();
        let result: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(result["sites"], json!([]));
        assert_eq!(result["sites_found"], 0);
        // The server's set does not hold that place, and the result says so.
        assert_eq!(result["data"][0]["covers_this_place"], false);
    }

    #[test]
    fn wind_directions_are_given_in_compass_points() {
        assert_eq!(sector_words(&WindSector::single(270.0)), "W");
        assert_eq!(
            sector_words(&WindSector {
                from_deg: 225.0,
                to_deg: 315.0
            }),
            "SW to NW"
        );
        assert_eq!(
            sector_words(&WindSector {
                from_deg: 337.5,
                to_deg: 22.5
            }),
            "NNW to NNE"
        );
        assert_eq!(
            sector_words(&WindSector {
                from_deg: 0.0,
                to_deg: 360.0
            }),
            "any"
        );
    }

    #[test]
    fn a_refusal_is_passed_on_in_words() {
        let input = json!({"latitude": 47.68, "longitude": 11.9});
        let body = br#"{"error": {"code": "box_too_large", "message": "x"}}"#;
        assert!(matches!(
            result("sites_near", &input, 400, body, now()),
            Err(ToolError::Failed(message)) if message.contains("box_too_large")
        ));
    }
}
