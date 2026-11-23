//! Open plans: a plan for everyone flying from an area, on the pilot's
//! Venti server. "Saturday at the Wallberg, two seats free from Munich",
//! or "looking for company": posted some days ahead, found by the pilots
//! who ask for the plans near a place, who can say "I'm in".
//!
//! An open plan is public by its author's choice: everyone reads it,
//! without signing in, and everyone in on it is named by handle and name.
//! A plan meant for friends goes to a group ([`crate::groups`]). The core
//! puts the requests together, refuses before sending what the server
//! would refuse, and reads the answers into what a screen shows. Nothing
//! of it is kept on the device. The server pushes a new plan to the phones
//! of pilots whose circle holds it ([`crate::phones`]); someone coming in
//! tells nobody.

use venti_api_types::v1::{
    self as api, GroupTextError, OpenPlan, OpenPlanRequest, OpenPlanResponse, OpenPlansRequest,
    OpenPlansResponse, PlanId, TimeOfDay,
};
use venti_domain::GeoPoint;

use crate::account::{json_request, not_signed_in};
use crate::server::answer;
use crate::{Coordinate, Core, CoreError, ServerRequest};

/// An open plan, as the pilot writes it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct OpenPlanDraft {
    /// The day, written `2026-11-21`.
    pub day: String,
    /// Where: a name the pilot types.
    pub place: String,
    /// Where that is on the map. An open plan is found by it, so it is
    /// needed: the screen asks the pilot to mark the place.
    pub position: Coordinate,
    /// When to meet or start, written `09:30` on a 24-hour clock, if the
    /// plan says.
    pub meet_at: Option<String>,
    /// The seats free in the pilot's car, 1 to 8; `None` when the pilot is
    /// looking for company.
    pub seats: Option<u8>,
    /// Anything else, in the pilot's own words, at most 280 characters.
    /// May be empty.
    pub note: String,
}

/// An open plan, as a list shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct OpenPlanView {
    /// Its identifier on the server.
    pub id: String,
    /// The day, written `2026-11-21`.
    pub day: String,
    /// Where.
    pub place: String,
    /// Where that is on the map.
    pub position: Coordinate,
    /// When to meet or start, written `09:30`, if the plan says.
    pub meet_at: Option<String>,
    /// The seats free in the author's car; `None` when the author is
    /// looking for company.
    pub seats: Option<u8>,
    /// What the author offers, in a few words: "2 seats free", "1 seat
    /// free" or "Looking for company".
    pub offer_words: String,
    /// What else the author wrote.
    pub note: Option<String>,
    /// The name of the pilot who posted it.
    pub author: String,
    /// That pilot's handle, for their page.
    pub author_handle: String,
    /// Whether the pilot of this device posted it: only its author
    /// changes or deletes it.
    pub is_mine: bool,
    /// Whether the pilot of this device has said to be in.
    pub am_in: bool,
    /// The names of those who are in, the pilot of this device as "You"
    /// and first.
    pub pilots_in: Vec<String>,
    /// Who is in, in a line: "You, Mara Muster and Jonas Beispiel".
    pub in_words: String,
    /// What the pilot is told before saying "I'm in" or changing the
    /// plan: that everyone can read it, with their name. The same as
    /// [`OpenPlans::public_words`], for a plan's page opened by its
    /// address without the list.
    #[uniffi(default = "")]
    pub public_words: String,
}

/// The open plans of one day.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct OpenPlanDay {
    /// The day, written `2026-11-21`.
    pub day: String,
    /// Its plans: those with a time first, by time, then the rest.
    pub plans: Vec<OpenPlanView>,
}

/// Open plans by day: those near a place, or the pilot's own.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct OpenPlans {
    /// The days that have a plan, in order.
    pub days: Vec<OpenPlanDay>,
    /// Whether there were more than the answer carries: the later days
    /// are then missing, and fewer days or a smaller radius get them.
    pub is_cut_short: bool,
    /// What to say when there is none: "No open plans near here for these
    /// days." Empty when there are some.
    pub empty_words: String,
    /// What the pilot is told before posting one or saying "I'm in": that
    /// everyone can read it, with their name.
    pub public_words: String,
}

/// What an open plan's screen says before the pilot posts one or says
/// "I'm in".
const PUBLIC_WORDS: &str = "Everyone can read an open plan, without signing in, with your handle and name on it. For friends only, post the plan to a group.";

#[uniffi::export]
impl Core {
    /// The request for the open plans near a place, from a day on and, if
    /// given, up to one, both written `2026-11-21`, within `radius_km`
    /// (1 to 100; 25 when `None`). The first day is the pilot's own today,
    /// which the device knows and the server does not. Anyone may ask,
    /// signed in or not.
    pub fn open_plans_near_request(
        &self,
        place: Coordinate,
        radius_km: Option<u32>,
        from_day: String,
        to_day: Option<String>,
    ) -> Result<ServerRequest, CoreError> {
        let request = OpenPlansRequest {
            place: position(place)?,
            radius_km: radius_km.map(|km| km.clamp(1, api::MAX_OPEN_PLANS_RADIUS_KM)),
            from: Some(day(&from_day)?),
            to: to_day.as_deref().map(day).transpose()?,
        };
        Ok(bare("GET", request.path_and_query(), false))
    }

    /// The request for the open plans the pilot posted or is in on.
    pub fn my_open_plans_request(&self) -> Result<ServerRequest, CoreError> {
        self.open_plans_me()?;
        Ok(bare("GET", api::ACCOUNT_OPEN_PLANS_PATH.to_owned(), true))
    }

    /// Reads the server's answer to [`Core::open_plans_near_request`] and
    /// to [`Core::my_open_plans_request`].
    pub fn open_plans(&self, status: u16, body: Vec<u8>) -> Result<OpenPlans, CoreError> {
        let response: OpenPlansResponse = answer(status, &body)?;
        let me = self.my_handle()?.unwrap_or_default();
        let mut days: Vec<OpenPlanDay> = Vec::new();
        for plan in &response.plans {
            let view = open_plan_view(plan, &me);
            match days.last_mut() {
                Some(last) if last.day == view.day => last.plans.push(view),
                _ => days.push(OpenPlanDay {
                    day: view.day.clone(),
                    plans: vec![view],
                }),
            }
        }
        let empty_words = if days.is_empty() {
            "No open plans near here for these days.".to_owned()
        } else {
            String::new()
        };
        Ok(OpenPlans {
            days,
            is_cut_short: response.truncated,
            empty_words,
            public_words: PUBLIC_WORDS.to_owned(),
        })
    }

    /// The request for one open plan, which anyone may read.
    pub fn open_plan_request(&self, plan: String) -> Result<ServerRequest, CoreError> {
        Ok(bare("GET", api::open_plan_path(&plan_id(&plan)?), false))
    }

    /// The request that posts an open plan. Its author is in from the
    /// start, and everyone can read it.
    pub fn post_open_plan_request(&self, draft: OpenPlanDraft) -> Result<ServerRequest, CoreError> {
        self.open_plans_me()?;
        json_request(
            "POST",
            api::OPEN_PLANS_PATH,
            &open_plan_request(&draft)?,
            true,
        )
    }

    /// The request that changes an open plan: the whole plan as it is to
    /// be. Its author's to send.
    pub fn change_open_plan_request(
        &self,
        plan: String,
        draft: OpenPlanDraft,
    ) -> Result<ServerRequest, CoreError> {
        self.open_plans_me()?;
        json_request(
            "PUT",
            &api::open_plan_path(&plan_id(&plan)?),
            &open_plan_request(&draft)?,
            true,
        )
    }

    /// The request that deletes an open plan. Its author's to send; the
    /// answer has no body, and [`Core::group_done`] reads it.
    pub fn delete_open_plan_request(&self, plan: String) -> Result<ServerRequest, CoreError> {
        self.open_plans_me()?;
        Ok(bare("DELETE", api::open_plan_path(&plan_id(&plan)?), true))
    }

    /// The request that says "I'm in" on an open plan, or takes that back.
    pub fn open_plan_in_request(
        &self,
        plan: String,
        is_in: bool,
    ) -> Result<ServerRequest, CoreError> {
        self.open_plans_me()?;
        Ok(bare(
            if is_in { "PUT" } else { "DELETE" },
            api::open_plan_in_path(&plan_id(&plan)?),
            true,
        ))
    }

    /// Reads one open plan: the server's answer to reading, posting and
    /// changing it, and to the pilot's word on it.
    pub fn open_plan(&self, status: u16, body: Vec<u8>) -> Result<OpenPlanView, CoreError> {
        let response: OpenPlanResponse = answer(status, &body)?;
        let me = self.my_handle()?.unwrap_or_default();
        Ok(open_plan_view(&response.plan, &me))
    }
}

impl Core {
    /// The handle of the pilot signed in on this device.
    fn open_plans_me(&self) -> Result<String, CoreError> {
        self.my_handle()?.ok_or_else(not_signed_in)
    }
}

fn invalid(message: &str) -> CoreError {
    CoreError::Server {
        message: message.to_owned(),
    }
}

fn bare(method: &str, path: String, needs_token: bool) -> ServerRequest {
    ServerRequest {
        method: method.to_owned(),
        path,
        body: None,
        content_type: None,
        needs_token,
    }
}

fn plan_id(id: &str) -> Result<PlanId, CoreError> {
    id.parse()
        .map_err(|_| invalid("This is not a plan the app knows."))
}

fn day(text: &str) -> Result<chrono::NaiveDate, CoreError> {
    api::parse_day(text.trim()).map_err(|_| invalid("Choose the day of the plan."))
}

fn position(place: Coordinate) -> Result<GeoPoint, CoreError> {
    GeoPoint::new(place.lat, place.lon)
        .map_err(|_| invalid("Mark the place on the map: an open plan is found by it."))
}

/// A draft as the server takes it, with its texts cleaned as the server
/// cleans them.
fn open_plan_request(draft: &OpenPlanDraft) -> Result<OpenPlanRequest, CoreError> {
    let place = api::parse_plan_place(&draft.place).map_err(|error| match error {
        GroupTextError::Empty => invalid("Say where: the name of the place."),
        GroupTextError::TooLong(most) => {
            invalid(&format!("The place has room for {most} characters."))
        }
    })?;
    let note = api::parse_open_plan_note(&draft.note).map_err(|error| match error {
        GroupTextError::Empty => invalid("The note could not be read."),
        GroupTextError::TooLong(most) => {
            invalid(&format!("The note has room for {most} characters."))
        }
    })?;
    let meet_at = draft
        .meet_at
        .as_deref()
        .map(str::trim)
        .filter(|time| !time.is_empty())
        .map(|time| {
            time.parse::<TimeOfDay>()
                .map_err(|_| invalid("Write the time like 09:30, on a 24-hour clock."))
        })
        .transpose()?;
    if draft
        .seats
        .is_some_and(|seats| !(1..=api::MAX_OPEN_PLAN_SEATS).contains(&seats))
    {
        return Err(invalid(&format!(
            "Offer 1 to {} seats, or none if you are looking for company.",
            api::MAX_OPEN_PLAN_SEATS
        )));
    }
    Ok(OpenPlanRequest {
        day: day(&draft.day)?,
        place,
        position: position(draft.position)?,
        meet_at,
        seats: draft.seats,
        note,
    })
}

fn offer_words(seats: Option<u8>) -> String {
    match seats {
        None => "Looking for company".to_owned(),
        Some(1) => "1 seat free".to_owned(),
        Some(seats) => format!("{seats} seats free"),
    }
}

fn open_plan_view(plan: &OpenPlan, me: &str) -> OpenPlanView {
    let is_mine = !me.is_empty() && plan.author.handle.as_str() == me;
    let am_in = !me.is_empty()
        && plan
            .pilots_in
            .iter()
            .any(|pilot| pilot.handle.as_str() == me);
    let mut pilots_in: Vec<String> = Vec::new();
    if am_in {
        pilots_in.push("You".to_owned());
    }
    pilots_in.extend(
        plan.pilots_in
            .iter()
            .filter(|pilot| me.is_empty() || pilot.handle.as_str() != me)
            .map(|pilot| pilot.display_name.clone()),
    );
    OpenPlanView {
        id: plan.id.to_string(),
        day: api::format_day(plan.day),
        place: plan.place.clone(),
        position: Coordinate {
            lat: plan.position.lat(),
            lon: plan.position.lon(),
        },
        meet_at: plan.meet_at.map(|time| time.to_string()),
        seats: plan.seats,
        offer_words: offer_words(plan.seats),
        note: plan.note.clone(),
        author: plan.author.display_name.clone(),
        author_handle: plan.author.handle.to_string(),
        is_mine,
        am_in,
        in_words: crate::groups::in_words(&pilots_in),
        public_words: PUBLIC_WORDS.to_owned(),
        pilots_in,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::testing::TempFolder;

    fn golden(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/../server/tests/golden/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    fn core(folder: &TempFolder) -> Arc<Core> {
        Core::new(folder.path().to_str().unwrap().to_owned()).unwrap()
    }

    /// A core signed in as the development mode's demo pilot.
    fn signed_in(folder: &TempFolder) -> Arc<Core> {
        let core = core(folder);
        core.signed_in(
            "https://venti.example".to_owned(),
            201,
            golden("sign_in.json"),
        )
        .unwrap();
        core
    }

    fn draft() -> OpenPlanDraft {
        OpenPlanDraft {
            day: "2026-11-21".to_owned(),
            place: " Beispielwiese,\tParkplatz ".to_owned(),
            position: Coordinate {
                lat: 47.6922,
                lon: 11.7675,
            },
            meet_at: Some("08:30".to_owned()),
            seats: Some(3),
            note: "A made-up open plan. From the station at half past eight.".to_owned(),
        }
    }

    #[test]
    fn anyone_asks_for_the_open_plans_near_a_place_and_reads_them_by_day() {
        let folder = TempFolder::new();
        let core = core(&folder);
        let request = core
            .open_plans_near_request(
                Coordinate {
                    lat: 47.7,
                    lon: 11.75,
                },
                Some(25),
                "2026-11-18".to_owned(),
                None,
            )
            .unwrap();
        assert_eq!(
            request.path,
            "/v1/open-plans?lat=47.7&lon=11.75&radius_km=25&from=2026-11-18"
        );
        assert!(!request.needs_token);

        let plans = core.open_plans(200, golden("open_plans.json")).unwrap();
        assert_eq!(plans.days.len(), 2);
        let first = &plans.days[0].plans[0];
        assert_eq!(plans.days[0].day, "2026-11-19");
        assert_eq!(first.offer_words, "2 seats free");
        assert_eq!(first.in_words, "Mara Muster and Jonas Beispiel");
        assert!(!first.is_mine && !first.am_in);
        assert_eq!(plans.days[1].plans[0].offer_words, "Looking for company");
        assert!(plans.empty_words.is_empty());
        assert!(plans.public_words.contains("Everyone can read"));
    }

    #[test]
    fn a_signed_in_pilot_posts_an_open_plan_as_the_server_takes_it() {
        let folder = TempFolder::new();
        assert!(core(&folder).post_open_plan_request(draft()).is_err());
        let core = signed_in(&folder);
        let request = core.post_open_plan_request(draft()).unwrap();
        assert_eq!(
            (request.method.as_str(), request.path.as_str()),
            ("POST", "/v1/open-plans")
        );
        assert!(request.needs_token);
        let sent: serde_json::Value =
            serde_json::from_slice(request.body.as_ref().unwrap()).unwrap();
        let example: serde_json::Value =
            serde_json::from_slice(&golden("open_plan_request.json")).unwrap();
        assert_eq!(sent, example);

        let posted = core.open_plan(201, golden("open_plan.json")).unwrap();
        assert!(posted.is_mine && posted.am_in);
        assert_eq!(posted.in_words, "Only you so far");
        assert_eq!(posted.offer_words, "3 seats free");
        // A plan's page opened by its address says what the list says.
        assert!(posted.public_words.contains("Everyone can read"));

        let too_many = OpenPlanDraft {
            seats: Some(9),
            ..draft()
        };
        assert!(core.post_open_plan_request(too_many).is_err());
        let long = OpenPlanDraft {
            note: "x".repeat(281),
            ..draft()
        };
        assert!(core.post_open_plan_request(long).is_err());

        let said = core
            .open_plan_in_request("aaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(), true)
            .unwrap();
        assert_eq!(
            (said.method.as_str(), said.path.as_str()),
            ("PUT", "/v1/open-plans/aaaaaaaaaaaaaaaaaaaaaaaaaa/in")
        );
        assert!(
            core.open_plan_in_request("nonsense".to_owned(), true)
                .is_err()
        );
        assert_eq!(
            core.my_open_plans_request().unwrap().path,
            "/v1/account/open-plans"
        );
    }
}
