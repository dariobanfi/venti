//! The assistant inside the app.
//!
//! The pilot brings a key for their own account with a model provider. The
//! app talks to the provider directly; the project's server is not
//! involved and never sees the key or the conversation.
//!
//! This module runs one conversation as a loop without I/O. The shell asks
//! a question and gets back a request to send; it sends it and hands over
//! the answer; the core runs whatever tools the model asked for, against
//! the logbook on the device, and either hands back the next request or
//! says the reply is ready. The key is passed in with each step and kept
//! nowhere.
//!
//! What the pilot should know, and the app says where the key is entered:
//! a question, and whatever the tools looked up to answer it, go to the
//! provider.

use std::sync::{Arc, Mutex, MutexGuard};

use chrono::DateTime;
use serde_json::{Value, json};
use venti_llm::{
    Anthropic, Block, Conversation, LlmError, Message, Provider, Role, Stop, ToolSpec,
};
use venti_tools::{Host, ToolError};

use crate::{Core, FlightMoment};

/// How many times in a row the model may ask for tools before the app
/// stops it. A question about a logbook needs two or three.
const MAX_ROUNDS: u32 = 8;

/// The most tokens one reply may have.
const MAX_TOKENS: u32 = 2048;

/// A model the pilot can choose.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AssistantModel {
    /// The provider's identifier for it.
    pub id: String,
    /// Its name.
    pub name: String,
    /// One line on what it is like, to choose by.
    pub note: String,
}

/// The models the assistant can use, the default first. They are
/// Anthropic's current models as listed in its documentation on 2026-11-19.
#[uniffi::export]
pub fn assistant_models() -> Vec<AssistantModel> {
    [
        ("claude-fable-5-2", "Claude Fable 5.2", "The most capable."),
        ("claude-opus-5-5", "Claude Opus 5.5", "Very capable."),
        (
            "claude-sonnet-5-5",
            "Claude Sonnet 5.5",
            "Fast, and costs less.",
        ),
        (
            "claude-haiku-5-5",
            "Claude Haiku 5.5",
            "The fastest and cheapest.",
        ),
    ]
    .into_iter()
    .map(|(id, name, note)| AssistantModel {
        id: id.to_owned(),
        name: name.to_owned(),
        note: note.to_owned(),
    })
    .collect()
}

/// One header of a request.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct HttpHeader {
    /// Its name.
    pub name: String,
    /// Its value.
    pub value: String,
}

/// A request for the shell to send as an HTTP `POST`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AssistantRequest {
    /// Where to send it.
    pub url: String,
    /// Its headers. One of them holds the pilot's key: do not log them.
    pub headers: Vec<HttpHeader>,
    /// Its body.
    pub body: String,
}

/// What the shell does next.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum AssistantStep {
    /// Send this request and pass the answer to `receive`.
    Send {
        /// The request.
        request: AssistantRequest,
    },
    /// The reply is in the transcript.
    Finished,
    /// It did not work. The conversation is as it was before the question;
    /// the message tells the pilot why.
    Failed {
        /// Why, in words for the pilot.
        message: String,
    },
}

/// One entry of the conversation as the screen shows it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptEntry {
    /// Whether the pilot said it; otherwise the assistant did.
    pub from_pilot: bool,
    /// What was said.
    pub text: String,
    /// For the assistant: what it looked up to say it, in words, so the
    /// pilot can see where an answer came from.
    pub looked_up: Vec<String>,
}

/// One conversation with the assistant.
#[derive(Debug, uniffi::Object)]
pub struct Assistant {
    core: Arc<Core>,
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    model: String,
    messages: Vec<Message>,
    /// The date the current question was asked, for the instructions.
    today: String,
    /// How many messages the conversation had before the current question,
    /// to go back to if it fails.
    before_question: usize,
    /// Rounds of tool use in the current question.
    rounds: u32,
}

#[uniffi::export]
impl Core {
    /// Starts a conversation with the assistant, using the given model: one
    /// of the identifiers from [`assistant_models`].
    pub fn assistant(self: Arc<Self>, model: String) -> Arc<Assistant> {
        Arc::new(Assistant {
            core: self,
            state: Mutex::new(State {
                model,
                messages: Vec::new(),
                today: String::new(),
                before_question: 0,
                rounds: 0,
            }),
        })
    }
}

#[uniffi::export]
impl Assistant {
    /// Asks a question. `now_unix_s` is the current time; `key` is the
    /// pilot's key for the provider.
    pub fn ask(&self, question: String, now_unix_s: i64, key: String) -> AssistantStep {
        let mut state = self.lock();
        state.before_question = state.messages.len();
        state.rounds = 0;
        state.today = DateTime::from_timestamp(now_unix_s, 0)
            .map(|now| now.format("%A, %-d %B %Y").to_string())
            .unwrap_or_default();
        state.messages.push(Message::user(question));
        send(&state, &key)
    }

    /// Takes the provider's answer to the last request.
    pub fn receive(&self, status: u16, body: Vec<u8>, key: String) -> AssistantStep {
        let mut state = self.lock();
        let reply = match Anthropic.parse(status, &body) {
            Ok(reply) => reply,
            Err(error) => return fail(&mut state, explain(&error)),
        };

        let calls: Vec<(String, String, Value)> = reply
            .message
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::ToolCall { id, name, input } => {
                    Some((id.clone(), name.clone(), input.clone()))
                }
                _ => None,
            })
            .collect();
        let has_words = !reply.message.text().trim().is_empty();
        state.messages.push(reply.message);

        match reply.stop {
            Stop::ToolUse if !calls.is_empty() => {
                state.rounds += 1;
                if state.rounds > MAX_ROUNDS {
                    return fail(
                        &mut state,
                        "The assistant kept looking things up without answering. Try asking in another way.".to_owned(),
                    );
                }
                let results = calls
                    .into_iter()
                    .map(|(id, name, input)| {
                        let result = venti_tools::run(self.core.as_ref(), &name, &input);
                        Block::ToolResult {
                            call_id: id,
                            is_error: result.is_err(),
                            content: result.unwrap_or_else(|error| venti_tools::failure(&error)),
                        }
                    })
                    .collect();
                state.messages.push(Message::tool_results(results));
                send(&state, &key)
            }
            Stop::Finished | Stop::TooLong | Stop::Refused if has_words => AssistantStep::Finished,
            Stop::Refused => fail(
                &mut state,
                "The assistant declined to answer that.".to_owned(),
            ),
            Stop::TooLong => fail(
                &mut state,
                "The answer became too long. Try a narrower question.".to_owned(),
            ),
            _ => fail(
                &mut state,
                "The assistant stopped without an answer. Try again.".to_owned(),
            ),
        }
    }

    /// The conversation so far, as the screen shows it.
    pub fn transcript(&self) -> Vec<TranscriptEntry> {
        let state = self.lock();
        let mut entries: Vec<TranscriptEntry> = Vec::new();
        // What the assistant looked up since it last spoke.
        let mut looked_up: Vec<String> = Vec::new();
        for message in &state.messages {
            let text = message.text();
            let text = text.trim();
            match message.role {
                Role::User if !text.is_empty() => {
                    looked_up.clear();
                    entries.push(TranscriptEntry {
                        from_pilot: true,
                        text: text.to_owned(),
                        looked_up: Vec::new(),
                    });
                }
                Role::User => {}
                Role::Assistant => {
                    let calls: Vec<String> = message
                        .blocks
                        .iter()
                        .filter_map(|block| match block {
                            Block::ToolCall { name, .. } => Some(describe(name)),
                            _ => None,
                        })
                        .collect();
                    looked_up.extend(calls.iter().cloned());
                    // A message that only calls tools shows up with the
                    // answer that follows it.
                    if !calls.is_empty() {
                        continue;
                    }
                    if !text.is_empty() {
                        looked_up.dedup();
                        entries.push(TranscriptEntry {
                            from_pilot: false,
                            text: text.to_owned(),
                            looked_up: std::mem::take(&mut looked_up),
                        });
                    }
                }
            }
        }
        entries
    }

    /// Gives up on the current question, for when the request could not be
    /// sent at all. The conversation is as it was before the question.
    pub fn abandon(&self) {
        let mut state = self.lock();
        let keep = state.before_question;
        state.messages.truncate(keep);
    }

    /// Forgets the conversation.
    pub fn clear(&self) {
        let mut state = self.lock();
        state.messages.clear();
        state.before_question = 0;
        state.rounds = 0;
    }
}

impl Assistant {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The request for the conversation as it stands.
fn send(state: &State, key: &str) -> AssistantStep {
    let tools: Vec<ToolSpec> = venti_tools::TOOLS
        .iter()
        .map(|tool| ToolSpec {
            name: tool.name.to_owned(),
            description: tool.description.to_owned(),
            input_schema: tool.input_schema(),
        })
        .collect();
    let request = Anthropic.request(
        key,
        &Conversation {
            model: &state.model,
            system: &instructions(&state.today),
            tools: &tools,
            messages: &state.messages,
            max_tokens: MAX_TOKENS,
        },
    );
    AssistantStep::Send {
        request: AssistantRequest {
            url: request.url,
            headers: request
                .headers
                .into_iter()
                .map(|(name, value)| HttpHeader { name, value })
                .collect(),
            body: request.body,
        },
    }
}

/// Puts the conversation back to where it was before the question.
fn fail(state: &mut State, message: String) -> AssistantStep {
    let keep = state.before_question;
    state.messages.truncate(keep);
    AssistantStep::Failed { message }
}

/// A provider failure in words for the pilot.
fn explain(error: &LlmError) -> String {
    match error {
        LlmError::BadKey => {
            "The provider did not accept your key. Check it in the assistant's settings.".to_owned()
        }
        LlmError::Limited(said) => {
            format!("Your account with the provider is being limited. It says: {said}")
        }
        LlmError::Unavailable(_) => {
            "The provider is not available right now. Try again in a moment.".to_owned()
        }
        LlmError::Rejected(said) => format!("The provider rejected the request. It says: {said}"),
        LlmError::Malformed(_) => "The provider's reply could not be read.".to_owned(),
    }
}

/// What a tool does, in words for the transcript.
fn describe(tool: &str) -> String {
    match tool {
        "logbook_statistics" => "your logbook's statistics",
        "list_flights" => "your list of flights",
        "get_flight" => "a flight",
        other => other,
    }
    .to_owned()
}

/// The standing instructions for the model.
fn instructions(today: &str) -> String {
    format!(
        "You are the assistant inside Venti, a free paragliding app. You are talking with the \
pilot whose logbook is on this device.

Use the tools to look up anything about the pilot's flights. Never state a number about their \
flying that did not come from a tool. Units are metric: metres, kilometres, metres per second, \
hours and minutes. Times in tool results are UTC; if you quote a time of day, say that it is UTC.

What you cannot see in this version: live weather, wind stations, forecasts, airspace, flying \
sites and other pilots. If asked about them, say plainly that this version of the app cannot \
look that up. Do not guess at conditions.

You are not a substitute for the pilot's own judgement. Whether and where to fly is their \
decision, and you say so when a question is really about that.

Answer briefly and plainly, in the language the pilot writes in. Today is {today}."
    )
}

/// A flight moment for a tool result.
fn moment_json(moment: &FlightMoment) -> Value {
    json!({
        "minutes_after_takeoff": (f64::from(moment.seconds) / 6.0).round() / 10.0,
        "altitude_m": moment.altitude_m,
    })
}

/// Rounds to one decimal, so that tool results are not padded with digits
/// nobody measured.
fn tenth(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn date_time(unix_s: i64) -> String {
    DateTime::from_timestamp(unix_s, 0)
        .map(|time| time.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_default()
}

fn kind_name(kind: crate::RouteKind, closed: bool) -> &'static str {
    match (kind, closed) {
        (crate::RouteKind::FreeFlight, _) => "free flight",
        (crate::RouteKind::FreeTriangle, false) => "free triangle",
        (crate::RouteKind::FreeTriangle, true) => "closed free triangle",
        (crate::RouteKind::FaiTriangle, false) => "FAI triangle",
        (crate::RouteKind::FaiTriangle, true) => "closed FAI triangle",
    }
}

/// The tools answer from the logbook on the device.
impl Host for Core {
    fn logbook_statistics(&self) -> Result<Value, ToolError> {
        let (logbook, statistics) = (self.logbook(), self.statistics());
        let best = |best: &Option<crate::BestFlight>, unit: &str, scale: f64| {
            best.as_ref().map(|best| {
                json!({
                    "flight_id": best.flight_id,
                    "date": date_time(best.start_unix_s),
                    unit: tenth(best.value * scale),
                })
            })
        };
        Ok(json!({
            "source": "the pilot's logbook on this device",
            "total": {
                "flights": logbook.totals.flights,
                "airtime_h": tenth(logbook.totals.airtime_s as f64 / 3600.0),
                "cross_country_km": tenth(logbook.totals.distance_m / 1000.0),
                "cross_country_points": tenth(logbook.totals.points),
            },
            "by_year": logbook.years.iter().map(|year| json!({
                "year": year.year,
                "flights": year.totals.flights,
                "airtime_h": tenth(year.totals.airtime_s as f64 / 3600.0),
                "cross_country_km": tenth(year.totals.distance_m / 1000.0),
            })).collect::<Vec<_>>(),
            "last_twelve_months": statistics.months.iter().map(|month| json!({
                "month": format!("{}-{:02}", month.year, month.month),
                "flights": month.flights,
                "airtime_h": tenth(month.airtime_s as f64 / 3600.0),
                "cross_country_km": tenth(month.distance_m / 1000.0),
            })).collect::<Vec<_>>(),
            "gliders": statistics.gliders.iter().map(|glider| json!({
                "glider": glider.glider,
                "flights": glider.flights,
                "airtime_h": tenth(glider.airtime_s as f64 / 3600.0),
            })).collect::<Vec<_>>(),
            "bests": {
                "longest": best(&statistics.longest, "airtime_h", 1.0 / 3600.0),
                "farthest": best(&statistics.farthest, "cross_country_km", 0.001),
                "most_points": best(&statistics.most_points, "points", 1.0),
                "highest": best(&statistics.highest, "altitude_m", 1.0),
                "best_climb": best(&statistics.best_climb, "climb_m_per_s", 1.0),
            },
        }))
    }

    fn list_flights(&self, year: Option<i32>, limit: usize) -> Result<Value, ToolError> {
        let logbook = self.logbook();
        let flights: Vec<Value> = logbook
            .years
            .iter()
            .filter(|section| year.is_none_or(|year| section.year == year))
            .flat_map(|section| &section.flights)
            .take(limit)
            .map(|flight| {
                json!({
                    "flight_id": flight.id,
                    "takeoff": date_time(flight.start_unix_s),
                    "airtime_min": flight.duration_s / 60,
                    "scored_as": flight.score.map(|score| kind_name(score.kind, score.closed)),
                    "cross_country_km": flight.score.map(|score| tenth(score.distance_m / 1000.0)),
                    "points": flight.score.map(|score| tenth(score.points)),
                    "highest_m": flight.max_altitude_m,
                    "glider": flight.glider,
                })
            })
            .collect();
        let matching: u32 = logbook
            .years
            .iter()
            .filter(|section| year.is_none_or(|year| section.year == year))
            .map(|section| section.totals.flights)
            .sum();
        Ok(json!({
            "source": "the pilot's logbook on this device",
            "flights_matching": matching,
            "flights_listed": flights.len(),
            "flights": flights,
        }))
    }

    fn flight(&self, flight_id: &str) -> Result<Value, ToolError> {
        let failed = |error: crate::CoreError| ToolError::Failed(error.to_string());
        let flight = self.flight(flight_id.to_owned()).map_err(failed)?;
        let analysis = self.flight_analysis(flight_id.to_owned()).map_err(failed)?;
        Ok(json!({
            "source": "the pilot's logbook on this device",
            "flight_id": flight.id,
            "takeoff": date_time(flight.start_unix_s),
            "landed": analysis.landing.is_some(),
            "airtime_min": flight.duration_s / 60,
            "pilot": flight.pilot,
            "glider": flight.glider,
            "highest_m": flight.max_altitude_m,
            "lowest_m": flight.min_altitude_m,
            "best_climb_m_per_s": tenth(flight.max_climb_ms),
            "strongest_sink_m_per_s": tenth(flight.max_sink_ms),
            "track_length_km": tenth(flight.track_length_m / 1000.0),
            "score": analysis.score.as_ref().map(|score| json!({
                "scored_as": kind_name(score.kind, score.closed),
                "distance_km": tenth(score.distance_m / 1000.0),
                "points": tenth(score.points),
                "points_per_km": score.points_per_km,
                "rules": "World XContest, worked out by this app",
            })),
            "minutes_in_thermals": analysis.thermalling_s / 60,
            "minutes_on_glides": analysis.gliding_s / 60,
            "height_gained_in_thermals_m": analysis.height_gained_m,
            // A long flight has dozens of thermals; the strongest say most.
            "thermals_count": analysis.thermals.len(),
            "thermals": analysis.thermals.iter().take(40).map(|thermal| json!({
                "entered": moment_json(&thermal.entry),
                "left": moment_json(&thermal.exit),
                "gain_m": thermal.gain_m,
                "average_climb_m_per_s": tenth(thermal.climb_ms),
                "wind_from_deg": thermal.wind.map(|wind| wind.from_deg.round()),
                "wind_km_per_h": thermal.wind.map(|wind| tenth(wind.speed_ms * 3.6)),
            })).collect::<Vec<_>>(),
            "glides_count": analysis.glides.len(),
            "glides": analysis.glides.iter().take(40).map(|glide| json!({
                "started": moment_json(&glide.start),
                "distance_km": tenth(glide.distance_m / 1000.0),
                "height_lost_m": glide.height_lost_m,
                "glide_ratio": glide.glide_ratio.map(tenth),
                "speed_km_per_h": tenth(glide.speed_ms * 3.6),
            })).collect::<Vec<_>>(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use venti_flight::igc;
    use venti_flight::testing::synthetic_flight;

    use super::*;
    use crate::testing::TempFolder;

    const NOW: i64 = 1_795_000_000; // 18 November 2026

    fn core_with_flights(folder: &TempFolder) -> Arc<Core> {
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        let sample = igc::write(&synthetic_flight());
        core.import_flight(sample.clone()).unwrap();
        core.import_flight(sample.replace("040726", "030725"))
            .unwrap();
        core
    }

    /// A provider reply that asks for one tool.
    fn tool_call(id: &str, name: &str, input: Value) -> Vec<u8> {
        json!({
            "content": [{"type": "tool_use", "id": id, "name": name, "input": input}],
            "stop_reason": "tool_use",
        })
        .to_string()
        .into_bytes()
    }

    fn answer(text: &str) -> Vec<u8> {
        json!({"content": [{"type": "text", "text": text}], "stop_reason": "end_turn"})
            .to_string()
            .into_bytes()
    }

    fn body(step: &AssistantStep) -> Value {
        match step {
            AssistantStep::Send { request } => serde_json::from_str(&request.body).unwrap(),
            other => panic!("expected a request, got {other:?}"),
        }
    }

    #[test]
    fn a_question_is_answered_through_the_tools_and_the_key_is_kept_nowhere() {
        let folder = TempFolder::new();
        let assistant = core_with_flights(&folder).assistant("claude-fable-5-2".to_owned());

        let step = assistant.ask("How was my 2026?".to_owned(), NOW, "sk-secret".to_owned());
        let AssistantStep::Send { request } = &step else {
            panic!("{step:?}");
        };
        assert_eq!(request.url, "https://api.anthropic.com/v1/messages");
        assert!(
            request
                .headers
                .iter()
                .any(|header| header.name == "x-api-key" && header.value == "sk-secret")
        );
        let first = body(&step);
        assert_eq!(first["model"], "claude-fable-5-2");
        assert_eq!(first["tools"].as_array().unwrap().len(), 3);
        assert!(
            first["system"]
                .as_str()
                .unwrap()
                .contains("Today is Wednesday, 18 November 2026.")
        );
        assert!(!request.body.contains("sk-secret"));

        // The model asks for the 2026 flights; the core answers from the
        // logbook and asks the provider again.
        let step = assistant.receive(
            200,
            tool_call("call_1", "list_flights", json!({"year": 2026})),
            "sk-secret".to_owned(),
        );
        let second = body(&step);
        let result = &second["messages"][2]["content"][0];
        assert_eq!(result["type"], "tool_result");
        assert_eq!(result["tool_use_id"], "call_1");
        assert_eq!(result["is_error"], false);
        let listed: Value = serde_json::from_str(result["content"].as_str().unwrap()).unwrap();
        assert_eq!(listed["flights_matching"], 1);
        assert_eq!(listed["flights"][0]["takeoff"], "2026-07-04 10:30 UTC");
        assert_eq!(listed["flights"][0]["airtime_min"], 29);
        assert_eq!(listed["flights"][0]["glider"], "Sample Wing");
        let id = listed["flights"][0]["flight_id"]
            .as_str()
            .unwrap()
            .to_owned();

        // It then looks at that flight.
        let step = assistant.receive(
            200,
            tool_call("call_2", "get_flight", json!({"flight_id": id})),
            "sk-secret".to_owned(),
        );
        let third = body(&step);
        let flight: Value = serde_json::from_str(
            third["messages"][4]["content"][0]["content"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(flight["highest_m"], 2117.0);
        assert_eq!(flight["thermals_count"], 2);
        let thermal = &flight["thermals"][0];
        assert!((thermal["average_climb_m_per_s"].as_f64().unwrap() - 2.4).abs() < 0.2);
        assert!((thermal["wind_from_deg"].as_f64().unwrap() - 270.0).abs() < 8.0);
        assert!(flight["score"]["distance_km"].as_f64().unwrap() > 10.0);

        let step = assistant.receive(
            200,
            answer("One flight in 2026: 29 minutes from the Wallberg, with two thermals."),
            "sk-secret".to_owned(),
        );
        assert_eq!(step, AssistantStep::Finished);

        let transcript = assistant.transcript();
        assert_eq!(transcript.len(), 2);
        assert!(transcript[0].from_pilot);
        assert_eq!(transcript[0].text, "How was my 2026?");
        assert!(!transcript[1].from_pilot);
        assert!(transcript[1].text.starts_with("One flight in 2026"));
        assert_eq!(
            transcript[1].looked_up,
            ["your list of flights", "a flight"]
        );
    }

    #[test]
    fn a_tool_that_fails_tells_the_model_instead_of_ending_the_conversation() {
        let folder = TempFolder::new();
        let assistant = core_with_flights(&folder).assistant("claude-sonnet-5-5".to_owned());
        assistant.ask("Show me flight X".to_owned(), NOW, "k".to_owned());

        let step = assistant.receive(
            200,
            tool_call("call_1", "get_flight", json!({"flight_id": "nothing"})),
            "k".to_owned(),
        );
        let result = &body(&step)["messages"][2]["content"][0];
        assert_eq!(result["is_error"], true);
        assert!(
            result["content"]
                .as_str()
                .unwrap()
                .contains("no longer in the logbook")
        );
    }

    #[test]
    fn a_failure_leaves_the_conversation_as_it_was_before_the_question() {
        let folder = TempFolder::new();
        let assistant = core_with_flights(&folder).assistant("claude-fable-5-2".to_owned());
        assistant.ask("First question".to_owned(), NOW, "k".to_owned());
        assert_eq!(
            assistant.receive(200, answer("First answer."), "k".to_owned()),
            AssistantStep::Finished
        );

        assistant.ask("Second question".to_owned(), NOW, "bad".to_owned());
        let rejected = json!({"type": "error", "error": {"type": "authentication_error", "message": "invalid x-api-key"}});
        let step = assistant.receive(401, rejected.to_string().into_bytes(), "bad".to_owned());
        let AssistantStep::Failed { message } = step else {
            panic!("expected a failure");
        };
        assert!(message.contains("did not accept your key"), "{message}");

        // The same goes for a request that never reached the provider.
        assistant.ask("Third question".to_owned(), NOW, "k".to_owned());
        assistant.abandon();

        // Only the first exchange is left, and the next question starts
        // from there.
        assert_eq!(assistant.transcript().len(), 2);
        let step = assistant.ask("Second question".to_owned(), NOW, "k".to_owned());
        assert_eq!(body(&step)["messages"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn a_model_that_never_stops_looking_things_up_is_stopped() {
        let folder = TempFolder::new();
        let assistant = core_with_flights(&folder).assistant("claude-fable-5-2".to_owned());
        assistant.ask("Loop".to_owned(), NOW, "k".to_owned());

        let mut step = AssistantStep::Finished;
        for round in 0..=MAX_ROUNDS {
            step = assistant.receive(
                200,
                tool_call(&format!("call_{round}"), "logbook_statistics", json!({})),
                "k".to_owned(),
            );
        }
        assert!(matches!(step, AssistantStep::Failed { .. }), "{step:?}");
        assert!(assistant.transcript().is_empty());
    }

    #[test]
    fn statistics_reach_the_model_with_units_in_their_names() {
        let folder = TempFolder::new();
        let core = core_with_flights(&folder);
        let statistics: Value = serde_json::from_str(
            &venti_tools::run(core.as_ref(), "logbook_statistics", &json!({})).unwrap(),
        )
        .unwrap();

        assert_eq!(statistics["total"]["flights"], 2);
        assert_eq!(statistics["total"]["airtime_h"], 1.0);
        assert_eq!(statistics["by_year"][0]["year"], 2026);
        assert_eq!(
            statistics["last_twelve_months"].as_array().unwrap().len(),
            12
        );
        assert_eq!(statistics["bests"]["highest"]["altitude_m"], 2117.0);
        assert_eq!(statistics["bests"]["best_climb"]["climb_m_per_s"], 2.4);
    }

    #[test]
    fn clearing_forgets_the_conversation() {
        let folder = TempFolder::new();
        let assistant = core_with_flights(&folder).assistant("claude-fable-5-2".to_owned());
        assistant.ask("Hello".to_owned(), NOW, "k".to_owned());
        assistant.receive(200, answer("Hello."), "k".to_owned());
        assistant.clear();
        assert!(assistant.transcript().is_empty());
        assert_eq!(assistant_models()[0].id, "claude-fable-5-2");
    }
}
