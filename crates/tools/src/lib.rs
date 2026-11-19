//! The tools an assistant can use.
//!
//! Each tool is defined here once: its name, what it is for, the shape of
//! its arguments, and whether it only reads. The same definitions serve the
//! assistant inside the app and the server's MCP connector, so a pilot gets
//! the same answers whichever assistant they ask.
//!
//! A tool does not know where its data comes from. Whoever offers the tools
//! implements [`Host`]: in the app, the client core answers from the
//! logbook on the device.
//!
//! The tools in [`wind`], [`airspace`], [`forecast`] and [`sites`] read
//! from a Venti server instead, and are answered in two steps so that this
//! crate stays free of I/O: one function says what to ask the server for,
//! one turns its answer into the tool's result. The server's connector
//! offers the wind and airspace tools and skips the first step: it builds
//! the answer from its own database. It does not offer the forecast, which
//! the server has to fetch when it is asked for, and nothing a connector's
//! tool call does may make the server fetch; it does not offer the sites
//! yet.
//!
//! Rules for tools:
//!
//! - A tool returns numbers with their units in the field names, and says
//!   where they come from. A model must never have to guess a unit.
//! - Calculations are done by the flight engine, never left to the model.
//! - Every tool declares whether it only reads. All of them do, so far.
//! - Results are small. A model pays for every token of them, and so does
//!   the pilot.

use serde_json::{Value, json};

pub mod airspace;
pub mod forecast;
pub mod sites;
pub mod wind;

/// One tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tool {
    /// The name the model calls it by.
    pub name: &'static str,
    /// What it does and when to use it, written for the model.
    pub description: &'static str,
    /// Whether it only reads. A tool that changes something must say so,
    /// so that the app can ask the pilot first.
    pub read_only: bool,
    /// A JSON Schema of its arguments, as JSON text.
    pub(crate) schema: &'static str,
}

impl Tool {
    /// The JSON Schema of the tool's arguments.
    pub fn input_schema(&self) -> Value {
        serde_json::from_str(self.schema).expect("every tool's schema is valid JSON")
    }
}

/// Why a tool call failed. The message goes back to the model, which can
/// correct itself or tell the pilot.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ToolError {
    /// No tool has this name.
    #[error("there is no tool named {0}")]
    UnknownTool(String),
    /// An argument is missing or of the wrong kind.
    #[error("bad arguments: {0}")]
    BadArguments(String),
    /// The host could not answer; the text says why.
    #[error("{0}")]
    Failed(String),
}

/// Where the tools get their answers. Each method returns JSON ready to be
/// handed to the model.
pub trait Host {
    /// Totals, the last twelve months, gliders and bests of the logbook.
    fn logbook_statistics(&self) -> Result<Value, ToolError>;

    /// Flights, newest first, optionally of one calendar year, at most
    /// `limit` of them.
    fn list_flights(&self, year: Option<i32>, limit: usize) -> Result<Value, ToolError>;

    /// One flight in full: its numbers, score, thermals and glides.
    fn flight(&self, flight_id: &str) -> Result<Value, ToolError>;
}

/// The most flights `list_flights` returns when the model does not say.
const DEFAULT_LIMIT: usize = 20;
/// The most it returns whatever the model says.
const MAX_LIMIT: usize = 100;

/// The tools that answer from the pilot's logbook, through a [`Host`].
pub const TOOLS: [Tool; 3] = [
    Tool {
        name: "logbook_statistics",
        description: "The pilot's logbook as a whole: number of flights, hours in the air and \
            cross-country distance in total and for each of the last twelve months, the flying \
            done on each glider, and the pilot's best flights (longest, farthest, most points, \
            highest, best climb) with their flight ids. Use it for questions about a season, \
            about progress, or about records.",
        read_only: true,
        schema: r#"{"type": "object", "properties": {}, "additionalProperties": false}"#,
    },
    Tool {
        name: "list_flights",
        description: "The pilot's flights, newest first, each with its id, date, take-off time, \
            airtime, cross-country score and glider. Use it to find a flight by date or to \
            compare flights. Give a year to see only that year.",
        read_only: true,
        schema: r#"{
            "type": "object",
            "properties": {
                "year": {"type": "integer", "description": "A calendar year, for example 2026."},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100,
                          "description": "How many flights to return at most. 20 if left out."}
            },
            "additionalProperties": false
        }"#,
    },
    Tool {
        name: "get_flight",
        description: "Everything about one flight: when and for how long, altitudes, its \
            cross-country score with the kind of route, the time spent in thermals and on \
            glides, and every thermal (when, height gained, average climb, the wind it showed) \
            and glide (distance, glide ratio, speed). Needs a flight id from list_flights or \
            logbook_statistics.",
        read_only: true,
        schema: r#"{
            "type": "object",
            "properties": {
                "flight_id": {"type": "string", "description": "The flight's id."}
            },
            "required": ["flight_id"],
            "additionalProperties": false
        }"#,
    },
];

/// Runs a tool and returns its result as JSON text.
pub fn run(host: &dyn Host, name: &str, input: &Value) -> Result<String, ToolError> {
    let result = match name {
        "logbook_statistics" => host.logbook_statistics()?,
        "list_flights" => {
            let year = match &input["year"] {
                Value::Null => None,
                year => Some(
                    year.as_i64()
                        .and_then(|year| i32::try_from(year).ok())
                        .ok_or_else(|| {
                            ToolError::BadArguments("year must be a whole number".to_owned())
                        })?,
                ),
            };
            let limit = match &input["limit"] {
                Value::Null => DEFAULT_LIMIT,
                limit => limit
                    .as_u64()
                    .filter(|limit| *limit >= 1)
                    .map(|limit| (limit as usize).min(MAX_LIMIT))
                    .ok_or_else(|| {
                        ToolError::BadArguments("limit must be a whole number from 1".to_owned())
                    })?,
            };
            host.list_flights(year, limit)?
        }
        "get_flight" => {
            let id = input["flight_id"]
                .as_str()
                .ok_or_else(|| ToolError::BadArguments("flight_id is required".to_owned()))?;
            host.flight(id)?
        }
        other => return Err(ToolError::UnknownTool(other.to_owned())),
    };
    Ok(result.to_string())
}

/// What to tell the model about a failed call, as a tool result.
pub fn failure(error: &ToolError) -> String {
    json!({"error": error.to_string()}).to_string()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// A host that remembers what it was asked.
    #[derive(Default)]
    struct Recorder {
        asked: RefCell<Vec<String>>,
    }

    impl Host for Recorder {
        fn logbook_statistics(&self) -> Result<Value, ToolError> {
            self.asked.borrow_mut().push("statistics".to_owned());
            Ok(json!({"flights": 3}))
        }

        fn list_flights(&self, year: Option<i32>, limit: usize) -> Result<Value, ToolError> {
            self.asked
                .borrow_mut()
                .push(format!("list {year:?} {limit}"));
            Ok(json!({"flights": []}))
        }

        fn flight(&self, flight_id: &str) -> Result<Value, ToolError> {
            self.asked.borrow_mut().push(format!("flight {flight_id}"));
            if flight_id == "gone" {
                return Err(ToolError::Failed(
                    "This flight is no longer in the logbook.".to_owned(),
                ));
            }
            Ok(json!({"id": flight_id}))
        }
    }

    #[test]
    fn every_tool_has_a_schema_a_description_and_a_unique_name() {
        let mut names = Vec::new();
        let all = TOOLS
            .iter()
            .chain(&wind::SERVER_TOOLS)
            .chain(&airspace::AIRSPACE_TOOLS);
        for tool in all {
            let schema = tool.input_schema();
            assert_eq!(schema["type"], "object", "{}", tool.name);
            assert_eq!(schema["additionalProperties"], false, "{}", tool.name);
            assert!(tool.description.len() > 60, "{}", tool.name);
            assert!(tool.read_only, "{}", tool.name);
            assert!(!names.contains(&tool.name));
            names.push(tool.name);
        }
    }

    #[test]
    fn calls_reach_the_host_with_their_arguments_and_defaults() {
        let host = Recorder::default();

        assert_eq!(
            run(&host, "logbook_statistics", &json!({})).unwrap(),
            r#"{"flights":3}"#
        );
        run(&host, "list_flights", &json!({})).unwrap();
        run(&host, "list_flights", &json!({"year": 2026, "limit": 5})).unwrap();
        run(&host, "list_flights", &json!({"limit": 5000})).unwrap();
        run(&host, "get_flight", &json!({"flight_id": "abc"})).unwrap();

        assert_eq!(
            *host.asked.borrow(),
            [
                "statistics",
                "list None 20",
                "list Some(2026) 5",
                "list None 100",
                "flight abc"
            ]
        );
    }

    #[test]
    fn bad_calls_are_explained_to_the_model() {
        let host = Recorder::default();

        let error = run(&host, "delete_everything", &json!({})).unwrap_err();
        assert_eq!(
            error,
            ToolError::UnknownTool("delete_everything".to_owned())
        );

        for input in [
            json!({"year": "last"}),
            json!({"limit": 0}),
            json!({"limit": -3}),
        ] {
            assert!(matches!(
                run(&host, "list_flights", &input).unwrap_err(),
                ToolError::BadArguments(_)
            ));
        }
        assert!(matches!(
            run(&host, "get_flight", &json!({})).unwrap_err(),
            ToolError::BadArguments(_)
        ));

        let error = run(&host, "get_flight", &json!({"flight_id": "gone"})).unwrap_err();
        assert_eq!(
            failure(&error),
            r#"{"error":"This flight is no longer in the logbook."}"#
        );
    }
}
