//! Push to the phone apps, the settings for what pilots nearby do, and
//! help after landing: the requests, the readers of the answers, and the
//! words the screens show.
//!
//! The shells' part: asking the system for permission to notify, getting
//! the token from APNs or FCM and handing it to
//! [`device_register_request`], telling a coarse position while the pilot
//! may be asked for help ([`position_request`]), and opening what a tapped
//! notification names ([`phone_message_target`]). The core does no I/O
//! (`docs/api.md`, "Push to the phone apps" and "Help after landing").

use venti_api_types::v1::{
    self as api, DEVICES_PATH, DeviceRemoveRequest, DeviceRequest, GroupTextError, HELP_PATH,
    HelpAnswerBody, HelpListResponse, HelpRequestBody, HelpResponse, NEARBY_PATH, NearbyCircle,
    NearbySettings, POSITION_PATH, PositionRequest, PushPlatform,
};
use venti_domain::GeoPoint;

use crate::account::json_request;
use crate::server::answer;
use crate::{Coordinate, CoreError, ServerRequest};

/// Whose push service a phone's token is of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum PhonePlatform {
    /// Apple's push service: an iPhone.
    Apple,
    /// Google's Firebase Cloud Messaging: an Android phone.
    Google,
}

impl PhonePlatform {
    fn wire(self) -> PushPlatform {
        match self {
            Self::Apple => PushPlatform::Apns,
            Self::Google => PushPlatform::Fcm,
        }
    }
}

/// The settings for what pilots nearby do, as the settings screen shows
/// them. Both off until the pilot turns them on.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct NearbyChoice {
    /// Where the pilot wants to be told of new open plans; `None` for
    /// nowhere.
    pub open_plans_near: Option<Coordinate>,
    /// How far around it, in kilometres, 1 to 100.
    pub open_plans_radius_km: u32,
    /// Whether pilots nearby who have landed may ask this pilot for help.
    pub may_be_asked_for_help: bool,
}

/// A pilot named on a request for help.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct HelpPilot {
    /// Their handle, for their page.
    pub handle: String,
    /// Their name.
    pub name: String,
}

/// A request for help, as a screen shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct HelpView {
    /// Its identifier on the server.
    pub id: String,
    /// The pilot who asks.
    pub pilot: HelpPilot,
    /// What they need, if they said.
    pub note: Option<String>,
    /// When they asked, RFC 3339.
    pub created_at: String,
    /// When it ends by itself, RFC 3339.
    pub ends_at: String,
    /// Whether it still stands: not closed, and not ended when the server
    /// answered.
    pub stands: bool,
    /// Whether the pilot of this device asks.
    pub is_mine: bool,
    /// Where the pilot who asks is: to them, and to one who said they can
    /// help.
    pub position: Option<Coordinate>,
    /// To one who was asked: how far, in kilometres.
    pub distance_km: Option<u32>,
    /// To one who was asked: whether they said they can help.
    pub can_help: Option<bool>,
    /// To the pilot who asks: how many were asked.
    pub asked: Option<u32>,
    /// To the pilot who asks: who can help, the first first.
    pub helpers: Vec<HelpPilot>,
    /// The line under the name: "8 km from you" or "Asked 12 pilots
    /// nearby · 2 can help".
    pub line: String,
}

/// The requests for help that stand.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct HelpLists {
    /// The pilot's own.
    pub mine: Vec<HelpView>,
    /// Those the pilot was asked.
    pub asked: Vec<HelpView>,
}

/// What a tapped notification opens.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, tsify::Tsify),
    serde(tag = "kind", rename_all = "camelCase")
)]
pub enum PhoneTarget {
    /// What an alarm fired: the list of what fired.
    Alert {
        /// The alarm.
        id: String,
    },
    /// An open plan's page.
    OpenPlan {
        /// The plan.
        id: String,
    },
    /// A request for help, asked of the pilot or answered on theirs.
    Help {
        /// The request.
        id: String,
    },
}

fn invalid(message: &str) -> CoreError {
    CoreError::Server {
        message: message.to_owned(),
    }
}

fn bare(method: &str, path: String) -> ServerRequest {
    ServerRequest {
        method: method.to_owned(),
        path,
        body: None,
        content_type: None,
        needs_token: true,
    }
}

fn point(at: Coordinate) -> Result<GeoPoint, CoreError> {
    GeoPoint::new(at.lat, at.lon).map_err(|_| invalid("The position could not be read."))
}

fn coordinate(at: GeoPoint) -> Coordinate {
    Coordinate {
        lat: at.lat(),
        lon: at.lon(),
    }
}

/// Reads an answer that carries nothing: `204`, or the server's refusal
/// in words.
#[uniffi::export]
pub fn phone_request_done(status: u16, body: Vec<u8>) -> Result<(), CoreError> {
    if (200..300).contains(&status) {
        return Ok(());
    }
    answer::<serde_json::Value>(status, &body).map(|_| ())
}

/// The request that registers this phone's push token on the account. On
/// an iPhone `sandbox` is `true` for a build for development.
#[uniffi::export]
pub fn device_register_request(
    platform: PhonePlatform,
    token: String,
    sandbox: bool,
) -> Result<ServerRequest, CoreError> {
    let body = DeviceRequest {
        platform: platform.wire(),
        token: token.trim().to_owned(),
        sandbox: sandbox && platform == PhonePlatform::Apple,
    };
    json_request("POST", DEVICES_PATH, &body, true)
}

/// The request that removes this phone from the account, sent before
/// signing out.
#[uniffi::export]
pub fn device_remove_request(
    platform: PhonePlatform,
    token: String,
) -> Result<ServerRequest, CoreError> {
    let body = DeviceRemoveRequest {
        platform: platform.wire(),
        token: token.trim().to_owned(),
    };
    json_request("DELETE", DEVICES_PATH, &body, true)
}

/// The request for the account's settings for what pilots nearby do.
#[uniffi::export]
pub fn nearby_settings_request() -> ServerRequest {
    bare("GET", NEARBY_PATH.to_owned())
}

/// The request that sets them, whole.
#[uniffi::export]
pub fn nearby_settings_change_request(choice: NearbyChoice) -> Result<ServerRequest, CoreError> {
    let open_plans_near = match choice.open_plans_near {
        None => None,
        Some(at) => {
            if !(1..=api::MAX_NEARBY_RADIUS_KM).contains(&choice.open_plans_radius_km) {
                return Err(invalid(&format!(
                    "Choose a distance from 1 to {} km.",
                    api::MAX_NEARBY_RADIUS_KM
                )));
            }
            Some(NearbyCircle {
                position: point(at)?,
                radius_km: choice.open_plans_radius_km,
            })
        }
    };
    let body = NearbySettings {
        open_plans_near,
        may_be_asked_for_help: choice.may_be_asked_for_help,
    };
    json_request("PUT", NEARBY_PATH, &body, true)
}

/// Reads the answer to [`nearby_settings_request`] or
/// [`nearby_settings_change_request`].
#[uniffi::export]
pub fn nearby_settings(status: u16, body: Vec<u8>) -> Result<NearbyChoice, CoreError> {
    let settings: NearbySettings = answer(status, &body)?;
    Ok(NearbyChoice {
        open_plans_near: settings
            .open_plans_near
            .map(|circle| coordinate(circle.position)),
        open_plans_radius_km: settings
            .open_plans_near
            .map_or(api::DEFAULT_NEARBY_RADIUS_KM, |circle| circle.radius_km),
        may_be_asked_for_help: settings.may_be_asked_for_help,
    })
}

/// The request that tells the server where the phone is, for being asked
/// for help. The server keeps it coarse, for an hour; sent while the
/// setting is on, at most every few minutes.
#[uniffi::export]
pub fn position_request(at: Coordinate) -> Result<ServerRequest, CoreError> {
    json_request(
        "PUT",
        POSITION_PATH,
        &PositionRequest {
            position: point(at)?,
        },
        true,
    )
}

/// The request that has the server forget the phone's position.
#[uniffi::export]
pub fn position_forget_request() -> ServerRequest {
    bare("DELETE", POSITION_PATH.to_owned())
}

/// The request that asks the pilots nearby for help, from where the pilot
/// is and what they wrote, which may be empty.
#[uniffi::export]
pub fn help_ask_request(at: Coordinate, note: String) -> Result<ServerRequest, CoreError> {
    let note = api::parse_help_note(&note).map_err(|error| match error {
        GroupTextError::TooLong(most) => {
            invalid(&format!("The note has room for {most} characters."))
        }
        GroupTextError::Empty => invalid("The note could not be read."),
    })?;
    let body = HelpRequestBody {
        position: point(at)?,
        note,
    };
    json_request("POST", HELP_PATH, &body, true)
}

/// The request for the requests that stand: the pilot's own and those
/// they were asked.
#[uniffi::export]
pub fn help_list_request() -> ServerRequest {
    bare("GET", HELP_PATH.to_owned())
}

/// The request for one request for help.
#[uniffi::export]
pub fn help_one_request(id: String) -> ServerRequest {
    bare("GET", api::help_path(&id))
}

/// The request that closes the pilot's own request: "I'm fine".
#[uniffi::export]
pub fn help_close_request(id: String) -> ServerRequest {
    bare("DELETE", api::help_path(&id))
}

/// The request that says whether the pilot can help.
#[uniffi::export]
pub fn help_answer_request(id: String, can_help: bool) -> Result<ServerRequest, CoreError> {
    json_request(
        "PUT",
        &api::help_answer_path(&id),
        &HelpAnswerBody { can_help },
        true,
    )
}

fn view(request: api::HelpRequest, now: chrono::DateTime<chrono::Utc>) -> HelpView {
    let pilot = |pilot: &api::Pilot| HelpPilot {
        handle: pilot.handle.as_str().to_owned(),
        name: pilot.display_name.clone(),
    };
    let helpers: Vec<HelpPilot> = request.helpers.iter().map(pilot).collect();
    let line = if request.mine {
        let asked = match request.asked.unwrap_or(0) {
            0 => "Nobody nearby could be asked".to_owned(),
            1 => "Asked 1 pilot nearby".to_owned(),
            n => format!("Asked {n} pilots nearby"),
        };
        match helpers.len() {
            0 => asked,
            1 => format!("{asked} · 1 can help"),
            n => format!("{asked} · {n} can help"),
        }
    } else {
        match (request.distance_km, request.can_help) {
            (_, Some(true)) => "You said you can help".to_owned(),
            (Some(0 | 1), _) => "Within a kilometre of you".to_owned(),
            (Some(km), _) => format!("About {km} km from you"),
            (None, _) => "Nearby".to_owned(),
        }
    };
    let stamp = |time: chrono::DateTime<chrono::Utc>| {
        time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    };
    HelpView {
        stands: request.closed_at.is_none() && request.ends_at > now,
        pilot: pilot(&request.pilot),
        created_at: stamp(request.created_at),
        ends_at: stamp(request.ends_at),
        is_mine: request.mine,
        position: request.position.map(coordinate),
        distance_km: request.distance_km,
        can_help: request.can_help,
        asked: request.asked,
        helpers,
        line,
        note: request.note,
        id: request.id,
    }
}

/// Reads the answer to [`help_ask_request`], [`help_one_request`] or
/// [`help_answer_request`]. `now_ms` is the device's clock, in
/// milliseconds since 1970, for whether it still stands.
#[uniffi::export]
pub fn help_request(status: u16, body: Vec<u8>, now_ms: i64) -> Result<HelpView, CoreError> {
    let response: HelpResponse = answer(status, &body)?;
    Ok(view(response.request, now_of(now_ms)))
}

/// Reads the answer to [`help_list_request`].
#[uniffi::export]
pub fn help_requests(status: u16, body: Vec<u8>, now_ms: i64) -> Result<HelpLists, CoreError> {
    let response: HelpListResponse = answer(status, &body)?;
    let now = now_of(now_ms);
    Ok(HelpLists {
        mine: response.mine.into_iter().map(|r| view(r, now)).collect(),
        asked: response.asked.into_iter().map(|r| view(r, now)).collect(),
    })
}

fn now_of(now_ms: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp_millis(now_ms).unwrap_or_default()
}

/// What a tapped notification opens, from the `kind` and `id` it carries;
/// `None` for a kind this version does not know.
#[uniffi::export]
pub fn phone_message_target(kind: String, id: String) -> Option<PhoneTarget> {
    match kind.as_str() {
        "alert" => Some(PhoneTarget::Alert { id }),
        "open_plan" => Some(PhoneTarget::OpenPlan { id }),
        "help_request" | "help_answer" => Some(PhoneTarget::Help { id }),
        _ => None,
    }
}

/// The first words of every screen of help after landing, before
/// anything else: call the emergency number for an injury.
#[uniffi::export]
pub fn help_emergency_words() -> String {
    api::HELP_EMERGENCY_WORDS.to_owned()
}

/// The words of the settings for what pilots nearby do: a title and a
/// line for each switch, in this order: open plans, being asked for help.
#[uniffi::export]
pub fn nearby_settings_words() -> Vec<String> {
    vec![
        "New open plans near".to_owned(),
        "Tell me when someone posts an open plan within this distance.".to_owned(),
        "Pilots nearby may ask me for help".to_owned(),
        "After landing, pilots near you can ask for a lift or a hand. While this is on, \
         Venti keeps roughly where you are (to about 5 km) for an hour, and shows it to \
         nobody."
            .to_owned(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "4bq6x0vd5cwm2b9r8y1hzk3tns";

    #[test]
    fn the_requests_carry_what_the_server_takes() {
        let register =
            device_register_request(PhonePlatform::Google, " x:y ".to_owned(), true).unwrap();
        assert_eq!(
            (register.method.as_str(), register.path.as_str()),
            ("POST", "/v1/account/devices")
        );
        let body: serde_json::Value = serde_json::from_slice(&register.body.unwrap()).unwrap();
        assert_eq!(body, serde_json::json!({"platform": "fcm", "token": "x:y"}));
        let choice = NearbyChoice {
            open_plans_near: Some(Coordinate {
                lat: 47.6,
                lon: 11.7,
            }),
            open_plans_radius_km: 101,
            may_be_asked_for_help: true,
        };
        assert!(nearby_settings_change_request(choice).is_err());
        let change = nearby_settings_change_request(NearbyChoice {
            open_plans_radius_km: 30,
            ..choice
        })
        .unwrap();
        assert_eq!(change.method, "PUT");
        let ask = help_ask_request(
            Coordinate {
                lat: 47.27,
                lon: 11.39,
            },
            "  ".to_owned(),
        )
        .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&ask.body.unwrap()).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"position": {"lat": 47.27, "lon": 11.39}})
        );
        assert!(
            help_ask_request(
                Coordinate { lat: 0.0, lon: 0.0 },
                "x".repeat(api::MAX_HELP_NOTE_LENGTH + 1)
            )
            .is_err()
        );
        assert_eq!(
            help_answer_request(ID.to_owned(), true).unwrap().path,
            format!("/v1/help/{ID}/answer")
        );
        assert_eq!(help_close_request(ID.to_owned()).method, "DELETE");
        assert!(help_emergency_words().contains("112"));
    }

    #[test]
    fn the_answers_are_read_into_what_a_screen_shows() {
        let seen = include_bytes!("../../server/tests/golden/help_seen.json");
        let at = chrono::DateTime::parse_from_rfc3339("2026-11-18T19:10:00Z")
            .unwrap()
            .timestamp_millis();
        let view = help_request(200, seen.to_vec(), at).unwrap();
        assert!(!view.is_mine && view.stands);
        assert_eq!(view.line, "About 2 km from you");
        assert_eq!(view.position, None);
        let asked = include_bytes!("../../server/tests/golden/help_asked.json");
        let mine = help_request(201, asked.to_vec(), at).unwrap();
        assert_eq!(mine.line, "Asked 2 pilots nearby");
        assert!(mine.position.is_some());
        let settings = include_bytes!("../../server/tests/golden/nearby_settings.json");
        let choice = nearby_settings(200, settings.to_vec()).unwrap();
        assert_eq!(choice.open_plans_radius_km, 30);
        assert!(choice.may_be_asked_for_help);
        assert_eq!(
            phone_message_target("help_answer".to_owned(), ID.to_owned()),
            Some(PhoneTarget::Help { id: ID.to_owned() })
        );
        assert_eq!(
            phone_message_target("other".to_owned(), ID.to_owned()),
            None
        );
        assert!(phone_request_done(204, Vec::new()).is_ok());
    }
}
