//! Groups of pilots and the plans their members make for a flying day,
//! on the pilot's Venti server.
//!
//! A group is seen by its members and by nobody else; a pilot comes into
//! one with an invite code its keeper hands out. What members share in it
//! is plans: who means to fly where on which day. The core puts the
//! requests together, refuses before sending what the server would
//! refuse, and reads the answers into what a screen shows. Nothing of it
//! is kept on the device.

use std::collections::BTreeMap;

use venti_api_types::v1::{
    self as api, CreateGroupRequest, CreateGroupResponse, Group, GroupId, GroupInviteCode,
    GroupInviteResponse, GroupResponse, GroupTextError, GroupsResponse, HandOverGroupRequest,
    Handle, JoinGroupRequest, JoinGroupResponse, Plan, PlanBoardRequest, PlanBoardResponse,
    PlanDays, PlanId, PlanRequest, PlanResponse, TimeOfDay, UpdateGroupRequest,
};
use venti_domain::GeoPoint;

use crate::account::{json_request, not_signed_in};
use crate::server::{answer, refusal};
use crate::{Coordinate, Core, CoreError, ServerRequest};

/// A group, as a list shows it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct GroupView {
    /// Its identifier on the server.
    pub id: String,
    /// Its name.
    pub name: String,
    /// The name of the member who keeps it.
    pub keeper: String,
    /// How many members it has.
    pub members: u32,
    /// Whether the pilot of this device keeps it: only the keeper renames
    /// it, makes a new invite code, removes a member or deletes it.
    pub is_kept_by_me: bool,
    /// Members and keeper in a line: "3 members · kept by Mara Muster".
    pub summary: String,
}

/// One member of a group.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct GroupMember {
    /// The member's handle.
    pub handle: String,
    /// The name the member is shown by.
    pub name: String,
    /// Whether this member keeps the group.
    pub is_keeper: bool,
    /// Whether this is the pilot of this device.
    pub is_me: bool,
}

/// A group with its members, in the order they came in.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct GroupDetail {
    /// The group.
    pub group: GroupView,
    /// Its members.
    pub members: Vec<GroupMember>,
}

/// A group just made, with the code that lets others in.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct GroupMade {
    /// The group.
    pub group: GroupView,
    /// Its invite code. The server shows it this once: it keeps no copy
    /// that can be read. The keeper can have a new one made, which ends
    /// this one.
    pub invite_code: String,
}

/// What became of an invite code.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct GroupJoined {
    /// The group the code belongs to.
    pub group: GroupView,
    /// In a line: "You are in Mustergruppe now." or that the pilot was in
    /// it already.
    pub words: String,
}

/// A plan for a flying day, as a member writes it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PlanDraft {
    /// The day, written `2026-11-21`.
    pub day: String,
    /// Where: a name the pilot types.
    pub place: String,
    /// Where that is on the map, if the pilot marked it.
    pub position: Option<Coordinate>,
    /// When to meet or start, written `09:30` on a 24-hour clock, if the
    /// plan says.
    pub meet_at: Option<String>,
    /// Anything else, in the pilot's own words. May be empty.
    pub note: String,
}

/// A plan for a flying day, as a list shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PlanView {
    /// Its identifier on the server.
    pub id: String,
    /// The group it belongs to.
    pub group_id: String,
    /// That group's name, where the answer it comes from holds the group;
    /// else empty.
    pub group_name: String,
    /// The day, written `2026-11-21`.
    pub day: String,
    /// Where.
    pub place: String,
    /// Where that is on the map, if the plan says.
    pub position: Option<Coordinate>,
    /// When to meet or start, written `09:30`, if the plan says.
    pub meet_at: Option<String>,
    /// What else the author wrote.
    pub note: Option<String>,
    /// The name of the member who posted it.
    pub author: String,
    /// Whether the pilot of this device posted it: only its author
    /// changes a plan.
    pub is_mine: bool,
    /// Whether the pilot of this device may delete it: its author, and
    /// the keeper of its group.
    pub can_delete: bool,
    /// Whether the pilot of this device has said to be in.
    pub am_in: bool,
    /// The names of those who are in, the pilot of this device as "You"
    /// and first.
    pub pilots_in: Vec<String>,
    /// Who is in, in a line: "You, Mara Muster and Jonas Beispiel".
    pub in_words: String,
}

/// The plans of one day.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PlanDay {
    /// The day, written `2026-11-21`.
    pub day: String,
    /// Its plans: those with a time first, by time, then the rest.
    pub plans: Vec<PlanView>,
}

/// Who flies where on which day: the plans of every group the pilot is
/// in.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PlanBoard {
    /// The pilot's groups. Without one there is nobody to plan with.
    pub groups: Vec<GroupView>,
    /// The days that have a plan, in order.
    pub days: Vec<PlanDay>,
    /// Whether the groups hold more plans than the answer carries: the
    /// later days are then missing, and a narrower span of days gets them.
    pub is_cut_short: bool,
}

#[uniffi::export]
impl Core {
    /// The request for the groups the pilot is in.
    pub fn groups_request(&self) -> Result<ServerRequest, CoreError> {
        self.me()?;
        Ok(bare("GET", api::GROUPS_PATH.to_owned()))
    }

    /// Reads the server's answer to [`Core::groups_request`]: the groups,
    /// the one the pilot has been in longest first.
    pub fn groups(&self, status: u16, body: Vec<u8>) -> Result<Vec<GroupView>, CoreError> {
        let response: GroupsResponse = answer(status, &body)?;
        let me = self.me_or_nobody()?;
        Ok(response
            .groups
            .iter()
            .map(|group| group_view(group, &me))
            .collect())
    }

    /// The request that makes a group, with the pilot as its keeper.
    pub fn create_group_request(&self, name: String) -> Result<ServerRequest, CoreError> {
        self.me()?;
        json_request(
            "POST",
            api::GROUPS_PATH,
            &CreateGroupRequest::named(group_name(&name)?),
            true,
        )
    }

    /// Reads the server's answer to [`Core::create_group_request`].
    pub fn group_made(&self, status: u16, body: Vec<u8>) -> Result<GroupMade, CoreError> {
        let response: CreateGroupResponse = answer(status, &body)?;
        Ok(GroupMade {
            group: group_view(&response.group, &self.me_or_nobody()?),
            invite_code: response.invite_code.expose().to_owned(),
        })
    }

    /// The request that takes the pilot into the group an invite code
    /// belongs to. Hyphens, spaces and capitals in the code do not matter.
    pub fn join_group_request(&self, code: String) -> Result<ServerRequest, CoreError> {
        self.me()?;
        let code = code.trim();
        if code.is_empty() {
            return Err(invalid("Type the invite code the group's keeper gave you."));
        }
        if code.chars().count() > 64 {
            return Err(invalid("This is too long for an invite code."));
        }
        json_request(
            "POST",
            api::GROUP_JOIN_PATH,
            &JoinGroupRequest {
                code: GroupInviteCode::new(code),
            },
            true,
        )
    }

    /// Reads the server's answer to [`Core::join_group_request`].
    pub fn group_joined(&self, status: u16, body: Vec<u8>) -> Result<GroupJoined, CoreError> {
        let response: JoinGroupResponse = answer(status, &body)?;
        let group = group_view(&response.group, &self.me_or_nobody()?);
        Ok(GroupJoined {
            words: if response.joined {
                format!("You are in {} now.", group.name)
            } else {
                format!("You were in {} already.", group.name)
            },
            group,
        })
    }

    /// The request for one group with its members.
    pub fn group_request(&self, id: String) -> Result<ServerRequest, CoreError> {
        self.me()?;
        Ok(bare("GET", api::group_path(&group_id(&id)?)))
    }

    /// Reads a group with its members: the server's answer to
    /// [`Core::group_request`], [`Core::rename_group_request`] and
    /// [`Core::hand_over_group_request`].
    pub fn group(&self, status: u16, body: Vec<u8>) -> Result<GroupDetail, CoreError> {
        let response: GroupResponse = answer(status, &body)?;
        let me = self.me_or_nobody()?;
        Ok(GroupDetail {
            group: group_view(&response.group, &me),
            members: response
                .members
                .iter()
                .map(|member| GroupMember {
                    handle: member.handle.to_string(),
                    name: member.display_name.clone(),
                    is_keeper: member.handle == response.group.keeper.handle,
                    is_me: member.handle.as_str() == me,
                })
                .collect(),
        })
    }

    /// The request that renames a group. Its keeper's to send.
    pub fn rename_group_request(
        &self,
        id: String,
        name: String,
    ) -> Result<ServerRequest, CoreError> {
        self.me()?;
        json_request(
            "PATCH",
            &api::group_path(&group_id(&id)?),
            &UpdateGroupRequest::rename(group_name(&name)?),
            true,
        )
    }

    /// The request that hands the keeping of a group to another member.
    /// Its keeper's to send.
    pub fn hand_over_group_request(
        &self,
        id: String,
        handle: String,
    ) -> Result<ServerRequest, CoreError> {
        self.me()?;
        json_request(
            "PUT",
            &api::group_keeper_path(&group_id(&id)?),
            &HandOverGroupRequest { handle },
            true,
        )
    }

    /// The request that makes a new invite code for a group. The code
    /// before it stops working. Its keeper's to send.
    pub fn new_group_invite_request(&self, id: String) -> Result<ServerRequest, CoreError> {
        self.me()?;
        Ok(bare("POST", api::group_invite_path(&group_id(&id)?)))
    }

    /// Reads the server's answer to [`Core::new_group_invite_request`]:
    /// the new code, shown this once.
    pub fn group_invite(&self, status: u16, body: Vec<u8>) -> Result<String, CoreError> {
        let response: GroupInviteResponse = answer(status, &body)?;
        Ok(response.invite_code.expose().to_owned())
    }

    /// The request that takes the pilot out of a group. What the pilot
    /// put into it goes with them: their plans, and their word on the
    /// plans of others.
    pub fn leave_group_request(&self, id: String) -> Result<ServerRequest, CoreError> {
        let me = self.me()?;
        self.remove_group_member_request(id, me)
    }

    /// The request that removes a member from a group. Its keeper's to
    /// send; it also ends the invite code, which the removed member
    /// knows.
    pub fn remove_group_member_request(
        &self,
        id: String,
        handle: String,
    ) -> Result<ServerRequest, CoreError> {
        self.me()?;
        let member: Handle = handle
            .parse()
            .map_err(|_| invalid("This is not a member's handle."))?;
        Ok(bare(
            "DELETE",
            api::group_member_path(&group_id(&id)?, &member),
        ))
    }

    /// The request that deletes a group with everything in it. Its
    /// keeper's to send.
    pub fn delete_group_request(&self, id: String) -> Result<ServerRequest, CoreError> {
        self.me()?;
        Ok(bare("DELETE", api::group_path(&group_id(&id)?)))
    }

    /// Reads the server's answer to a request that gives nothing back:
    /// leaving, removing a member, deleting a group or a plan.
    pub fn group_done(&self, status: u16, body: Vec<u8>) -> Result<(), CoreError> {
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(refusal(status, &body))
        }
    }

    /// The request for the plans of every group the pilot is in, from a
    /// day on and, if given, up to one: both written `2026-11-21`. The
    /// first day is the pilot's own today, which the device knows and the
    /// server does not.
    pub fn plan_board_request(
        &self,
        from_day: String,
        to_day: Option<String>,
    ) -> Result<ServerRequest, CoreError> {
        self.me()?;
        let days = PlanDays {
            from: Some(day(&from_day)?),
            to: to_day.as_deref().map(day).transpose()?,
        };
        Ok(bare("GET", PlanBoardRequest { days }.path_and_query()))
    }

    /// Reads the server's answer to [`Core::plan_board_request`].
    pub fn plan_board(&self, status: u16, body: Vec<u8>) -> Result<PlanBoard, CoreError> {
        let response: PlanBoardResponse = answer(status, &body)?;
        let me = self.me_or_nobody()?;
        let groups: BTreeMap<&GroupId, &Group> = response
            .groups
            .iter()
            .map(|group| (&group.id, group))
            .collect();
        // The server sends the plans by day, then by time with those
        // without one last, then as they were posted.
        let mut days: Vec<PlanDay> = Vec::new();
        for plan in &response.plans {
            let view = plan_view(plan, groups.get(&plan.group).copied(), &me);
            match days.last_mut() {
                Some(last) if last.day == view.day => last.plans.push(view),
                _ => days.push(PlanDay {
                    day: view.day.clone(),
                    plans: vec![view],
                }),
            }
        }
        Ok(PlanBoard {
            groups: response
                .groups
                .iter()
                .map(|group| group_view(group, &me))
                .collect(),
            days,
            is_cut_short: response.truncated,
        })
    }

    /// The request that posts a plan to a group. Its author is in from
    /// the start.
    pub fn post_plan_request(
        &self,
        group: String,
        draft: PlanDraft,
    ) -> Result<ServerRequest, CoreError> {
        self.me()?;
        json_request(
            "POST",
            &api::group_plans_path(&group_id(&group)?),
            &plan_request(&draft)?,
            true,
        )
    }

    /// The request that changes a plan: the whole plan as it is to be.
    /// Its author's to send.
    pub fn change_plan_request(
        &self,
        group: String,
        plan: String,
        draft: PlanDraft,
    ) -> Result<ServerRequest, CoreError> {
        self.me()?;
        json_request(
            "PUT",
            &api::plan_path(&group_id(&group)?, &plan_id(&plan)?),
            &plan_request(&draft)?,
            true,
        )
    }

    /// The request that deletes a plan. Its author's and the group's
    /// keeper's to send.
    pub fn delete_plan_request(
        &self,
        group: String,
        plan: String,
    ) -> Result<ServerRequest, CoreError> {
        self.me()?;
        Ok(bare(
            "DELETE",
            api::plan_path(&group_id(&group)?, &plan_id(&plan)?),
        ))
    }

    /// The request that says the pilot is in on a plan, or takes that
    /// back.
    pub fn plan_in_request(
        &self,
        group: String,
        plan: String,
        is_in: bool,
    ) -> Result<ServerRequest, CoreError> {
        self.me()?;
        Ok(bare(
            if is_in { "PUT" } else { "DELETE" },
            api::plan_in_path(&group_id(&group)?, &plan_id(&plan)?),
        ))
    }

    /// Reads one plan: the server's answer to posting it, changing it and
    /// to the pilot's word on it. The group's name is not in such an
    /// answer.
    pub fn plan(&self, status: u16, body: Vec<u8>) -> Result<PlanView, CoreError> {
        let response: PlanResponse = answer(status, &body)?;
        Ok(plan_view(&response.plan, None, &self.me_or_nobody()?))
    }
}

impl Core {
    /// The handle of the pilot signed in on this device.
    fn me(&self) -> Result<String, CoreError> {
        self.my_handle()?.ok_or_else(not_signed_in)
    }

    /// The same for reading an answer, which a device that was signed out
    /// meanwhile still reads: then nobody is "me". A logbook that cannot
    /// be read is an error, not a pilot who is nobody: the board would
    /// show the pilot's own plans as somebody else's.
    fn me_or_nobody(&self) -> Result<String, CoreError> {
        Ok(self.my_handle()?.unwrap_or_default())
    }
}

pub(crate) fn invalid(message: &str) -> CoreError {
    CoreError::Server {
        message: message.to_owned(),
    }
}

/// A request without a body, sent with the session token.
pub(crate) fn bare(method: &str, path: String) -> ServerRequest {
    ServerRequest {
        method: method.to_owned(),
        path,
        body: None,
        content_type: None,
        needs_token: true,
    }
}

pub(crate) fn group_id(id: &str) -> Result<GroupId, CoreError> {
    id.parse()
        .map_err(|_| invalid("This is not a group the app knows."))
}

fn plan_id(id: &str) -> Result<PlanId, CoreError> {
    id.parse()
        .map_err(|_| invalid("This is not a plan the app knows."))
}

fn day(text: &str) -> Result<chrono::NaiveDate, CoreError> {
    api::parse_day(text.trim()).map_err(|_| invalid("Choose the day of the plan."))
}

pub(crate) fn group_name(name: &str) -> Result<String, CoreError> {
    api::parse_group_name(name).map_err(|error| match error {
        GroupTextError::Empty => invalid("Give the group a name."),
        GroupTextError::TooLong(most) => {
            invalid(&format!("A group's name has room for {most} characters."))
        }
    })
}

/// A draft as the server takes it, with its texts cleaned as the server
/// cleans them.
fn plan_request(draft: &PlanDraft) -> Result<PlanRequest, CoreError> {
    let place = api::parse_plan_place(&draft.place).map_err(|error| match error {
        GroupTextError::Empty => invalid("Say where: the name of the place."),
        GroupTextError::TooLong(most) => {
            invalid(&format!("The place has room for {most} characters."))
        }
    })?;
    let note = api::parse_plan_note(&draft.note).map_err(|error| match error {
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
    let position = draft
        .position
        .map(|position| {
            GeoPoint::new(position.lat, position.lon)
                .map_err(|_| invalid("The place's position on the map could not be read."))
        })
        .transpose()?;
    Ok(PlanRequest {
        day: day(&draft.day)?,
        place,
        position,
        meet_at,
        note,
    })
}

fn group_view(group: &Group, me: &str) -> GroupView {
    let is_kept_by_me = group.keeper.handle.as_str() == me;
    let members = match group.member_count {
        1 => "1 member".to_owned(),
        count => format!("{count} members"),
    };
    GroupView {
        id: group.id.to_string(),
        name: group.name.clone(),
        keeper: group.keeper.display_name.clone(),
        members: group.member_count,
        is_kept_by_me,
        summary: if is_kept_by_me {
            format!("{members} · kept by you")
        } else {
            format!("{members} · kept by {}", group.keeper.display_name)
        },
    }
}

pub(crate) fn plan_view(plan: &Plan, group: Option<&Group>, me: &str) -> PlanView {
    let is_mine = plan.author.handle.as_str() == me;
    let am_in = plan
        .pilots_in
        .iter()
        .any(|pilot| pilot.handle.as_str() == me);
    // "You" first, then the others as the server lists them.
    let mut pilots_in: Vec<String> = Vec::new();
    if am_in {
        pilots_in.push("You".to_owned());
    }
    pilots_in.extend(
        plan.pilots_in
            .iter()
            .filter(|pilot| pilot.handle.as_str() != me)
            .map(|pilot| pilot.display_name.clone()),
    );
    PlanView {
        id: plan.id.to_string(),
        group_id: plan.group.to_string(),
        group_name: group.map(|group| group.name.clone()).unwrap_or_default(),
        day: api::format_day(plan.day),
        place: plan.place.clone(),
        position: plan.position.map(|position| Coordinate {
            lat: position.lat(),
            lon: position.lon(),
        }),
        meet_at: plan.meet_at.map(|time| time.to_string()),
        note: plan.note.clone(),
        author: plan.author.display_name.clone(),
        is_mine,
        can_delete: is_mine || group.is_some_and(|group| group.keeper.handle.as_str() == me),
        am_in,
        in_words: in_words(&pilots_in),
        pilots_in,
    }
}

/// Who is in, in a line: every name up to three, then the first two and
/// how many more.
pub(crate) fn in_words(names: &[String]) -> String {
    match names {
        [] => "Nobody is in yet".to_owned(),
        [only] if only == "You" => "Only you so far".to_owned(),
        [only] => only.clone(),
        [first, second] => format!("{first} and {second}"),
        [first, second, third] => format!("{first}, {second} and {third}"),
        [first, second, rest @ ..] => format!("{first}, {second} and {} others", rest.len()),
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

    /// A core signed in as the development mode's demo pilot.
    fn signed_in(folder: &TempFolder) -> Arc<Core> {
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        core.signed_in(
            "https://venti.example".to_owned(),
            201,
            golden("sign_in.json"),
        )
        .unwrap();
        core
    }

    fn words(result: Result<impl std::fmt::Debug, CoreError>) -> String {
        match result {
            Err(CoreError::Server { message }) => message,
            other => panic!("{other:?}"),
        }
    }

    fn json(request: &ServerRequest) -> serde_json::Value {
        serde_json::from_slice(request.body.as_ref().unwrap()).unwrap()
    }

    #[test]
    fn the_plans_of_the_pilots_groups_are_read_day_by_day() {
        let folder = TempFolder::new();
        let core = signed_in(&folder);

        let request = core
            .plan_board_request("2026-11-18".to_owned(), Some("2026-11-18".to_owned()))
            .unwrap();
        assert_eq!(request.method, "GET");
        assert_eq!(
            request.path,
            "/v1/groups/plans?from=2026-11-18&to=2026-11-18"
        );
        assert!(request.needs_token && request.body.is_none());
        // From a day on, with no end.
        assert_eq!(
            core.plan_board_request("2026-11-18".to_owned(), None)
                .unwrap()
                .path,
            "/v1/groups/plans?from=2026-11-18"
        );
        assert_eq!(
            words(core.plan_board_request("tomorrow".to_owned(), None)),
            "Choose the day of the plan."
        );

        let board = core.plan_board(200, golden("plan_board.json")).unwrap();
        assert!(!board.is_cut_short);
        assert_eq!(board.groups.len(), 1);
        let group = &board.groups[0];
        assert_eq!(group.name, "Mustergruppe");
        assert!(group.is_kept_by_me);
        assert_eq!(group.summary, "3 members · kept by you");

        assert_eq!(board.days.len(), 1);
        assert_eq!(board.days[0].day, "2026-11-18");
        let plan = &board.days[0].plans[0];
        assert_eq!(plan.group_name, "Mustergruppe");
        assert_eq!(plan.place, "Musterberg, Startplatz Ost");
        assert_eq!(plan.meet_at.as_deref(), Some("10:30"));
        assert_eq!(
            plan.position,
            Some(Coordinate {
                lat: 47.6642,
                lon: 11.7967
            })
        );
        assert!(plan.note.as_deref().unwrap().contains('\n'));
        assert_eq!(plan.author, "Demo Pilot");
        assert!(plan.is_mine && plan.can_delete && plan.am_in);
        assert_eq!(plan.pilots_in, ["You", "Mara Muster", "Jonas Beispiel"]);
        assert_eq!(plan.in_words, "You, Mara Muster and Jonas Beispiel");
    }

    #[test]
    fn a_plan_is_posted_as_the_server_reads_it_and_refused_before_it_is_sent() {
        let folder = TempFolder::new();
        let core = signed_in(&folder);
        let group = "44444444444444444444444444".to_owned();
        let draft = PlanDraft {
            day: "2026-11-21".to_owned(),
            place: "  Wallberg,  north side ".to_owned(),
            position: Some(Coordinate {
                lat: 47.6642,
                lon: 11.7967,
            }),
            meet_at: Some("09:30".to_owned()),
            note: " At the lift.\r\nBring a jacket. ".to_owned(),
        };

        let request = core
            .post_plan_request(group.clone(), draft.clone())
            .unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/v1/groups/44444444444444444444444444/plans");
        let body = json(&request);
        assert_eq!(body["day"], "2026-11-21");
        assert_eq!(body["meet_at"], "09:30");
        assert_eq!(body["position"]["lat"], 47.6642);
        // Cleaned as the server cleans it.
        assert_eq!(body["note"], "At the lift.\nBring a jacket.");
        assert!(!body["place"].as_str().unwrap().starts_with(' '));

        // Without a time, a position or a note those are left out.
        let plain = PlanDraft {
            position: None,
            meet_at: Some("  ".to_owned()),
            note: String::new(),
            ..draft.clone()
        };
        let body = json(&core.post_plan_request(group.clone(), plain).unwrap());
        for absent in ["position", "meet_at", "note"] {
            assert!(body.get(absent).is_none(), "{absent}");
        }

        // Changing sends the whole plan to the plan's own address.
        let request = core
            .change_plan_request(
                group.clone(),
                "66666666666666666666666666".to_owned(),
                draft.clone(),
            )
            .unwrap();
        assert_eq!(request.method, "PUT");
        assert_eq!(
            request.path,
            "/v1/groups/44444444444444444444444444/plans/66666666666666666666666666"
        );

        let refused = |change: fn(&mut PlanDraft)| {
            let mut draft = draft.clone();
            change(&mut draft);
            words(core.post_plan_request(group.clone(), draft))
        };
        assert_eq!(
            refused(|draft| draft.place = "  ".to_owned()),
            "Say where: the name of the place."
        );
        assert_eq!(
            refused(|draft| draft.place = "x".repeat(81)),
            "The place has room for 80 characters."
        );
        assert_eq!(
            refused(|draft| draft.note = "x".repeat(501)),
            "The note has room for 500 characters."
        );
        assert_eq!(
            refused(|draft| draft.meet_at = Some("half past nine".to_owned())),
            "Write the time like 09:30, on a 24-hour clock."
        );
        assert_eq!(
            refused(|draft| draft.day = "21.11.2026".to_owned()),
            "Choose the day of the plan."
        );
        assert_eq!(
            refused(|draft| draft.position = Some(Coordinate {
                lat: 91.0,
                lon: 0.0
            })),
            "The place's position on the map could not be read."
        );
        assert_eq!(
            words(core.post_plan_request("mine".to_owned(), draft.clone())),
            "This is not a group the app knows."
        );

        // The answer is the plan; the group's name is not in it.
        let plan = core.plan(201, golden("plan.json")).unwrap();
        assert_eq!(plan.group_name, "");
        assert!(!plan.day.is_empty() && !plan.place.is_empty());
    }

    #[test]
    fn being_in_on_a_plan_and_deleting_one_are_requests_without_a_body() {
        let folder = TempFolder::new();
        let core = signed_in(&folder);
        let (group, plan) = (
            "44444444444444444444444444".to_owned(),
            "77777777777777777777777777".to_owned(),
        );
        let path = "/v1/groups/44444444444444444444444444/plans/77777777777777777777777777";

        let request = core
            .plan_in_request(group.clone(), plan.clone(), true)
            .unwrap();
        assert_eq!(
            (request.method.as_str(), request.path.as_str()),
            ("PUT", format!("{path}/in").as_str())
        );
        assert!(request.body.is_none() && request.needs_token);
        let request = core
            .plan_in_request(group.clone(), plan.clone(), false)
            .unwrap();
        assert_eq!(request.method, "DELETE");
        assert_eq!(request.path, format!("{path}/in"));

        let request = core.delete_plan_request(group, plan).unwrap();
        assert_eq!(
            (request.method.as_str(), request.path.as_str()),
            ("DELETE", path)
        );
        assert_eq!(core.group_done(204, Vec::new()), Ok(()));
        assert_eq!(
            words(core.group_done(
                403,
                br#"{"error":{"code":"not_plan_author","message":"x"}}"#.to_vec()
            )),
            "Only the member who posted a plan can change it; to delete it, its author or the group's keeper."
        );
    }

    #[test]
    fn groups_are_made_joined_read_and_left() {
        let folder = TempFolder::new();
        let core = signed_in(&folder);
        let id = "44444444444444444444444444".to_owned();

        let request = core
            .create_group_request("  Mustergruppe ".to_owned())
            .unwrap();
        assert_eq!(
            (request.method.as_str(), request.path.as_str()),
            ("POST", "/v1/groups")
        );
        assert_eq!(json(&request)["name"], "Mustergruppe");
        assert_eq!(
            words(core.create_group_request(" ".to_owned())),
            "Give the group a name."
        );
        assert_eq!(
            words(core.create_group_request("x".repeat(61))),
            "A group's name has room for 60 characters."
        );
        let made = core.group_made(201, golden("group_created.json")).unwrap();
        assert!(made.group.is_kept_by_me);
        assert_eq!(made.invite_code.len(), 19);

        // A code as people type it; the server does not mind hyphens,
        // spaces or capitals.
        let request = core
            .join_group_request(" 5555-5555-5555-5555 ".to_owned())
            .unwrap();
        assert_eq!(request.path, "/v1/groups/join");
        assert_eq!(json(&request)["code"], "5555-5555-5555-5555");
        assert_eq!(
            words(core.join_group_request("  ".to_owned())),
            "Type the invite code the group's keeper gave you."
        );
        let joined = core.group_joined(201, golden("group_joined.json")).unwrap();
        assert!(!joined.group.is_kept_by_me);
        assert_eq!(
            joined.words,
            format!("You are in {} now.", joined.group.name)
        );
        assert!(
            joined
                .group
                .summary
                .ends_with(&format!("kept by {}", joined.group.keeper))
        );
        assert_eq!(
            words(core.group_joined(
                403,
                br#"{"error":{"code":"invalid_group_invite","message":"x"}}"#.to_vec()
            )),
            "This is not a group's invite code: it is mistyped, or the group's keeper has replaced it. Ask them for the one that works now."
        );

        // The pilot's groups, and one of them with its members.
        let groups = core.groups(200, golden("groups.json")).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, id);
        assert_eq!(
            core.group_request(id.clone()).unwrap().path,
            "/v1/groups/44444444444444444444444444"
        );
        let detail = core.group(200, golden("group.json")).unwrap();
        assert_eq!(detail.members.len(), 3);
        let me = detail.members.iter().find(|member| member.is_me).unwrap();
        assert_eq!(me.handle, "demo");
        assert!(me.is_keeper);
        assert_eq!(
            detail
                .members
                .iter()
                .filter(|member| member.is_keeper)
                .count(),
            1
        );

        // What the keeper can do, and leaving.
        let request = core
            .rename_group_request(id.clone(), "Neuer Name".to_owned())
            .unwrap();
        assert_eq!(request.method, "PATCH");
        assert_eq!(json(&request)["name"], "Neuer Name");
        let request = core.new_group_invite_request(id.clone()).unwrap();
        assert_eq!(
            (request.method.as_str(), request.path.as_str()),
            ("POST", "/v1/groups/44444444444444444444444444/invite")
        );
        assert!(request.body.is_none());
        assert_eq!(
            core.group_invite(200, golden("group_invite.json"))
                .unwrap()
                .len(),
            19
        );
        let request = core
            .hand_over_group_request(id.clone(), "mara".to_owned())
            .unwrap();
        assert_eq!(request.method, "PUT");
        assert_eq!(request.path, "/v1/groups/44444444444444444444444444/keeper");
        assert_eq!(json(&request)["handle"], "mara");
        assert_eq!(
            core.remove_group_member_request(id.clone(), "jonas".to_owned())
                .unwrap()
                .path,
            "/v1/groups/44444444444444444444444444/members/jonas"
        );
        // Leaving is removing oneself.
        let request = core.leave_group_request(id.clone()).unwrap();
        assert_eq!(
            (request.method.as_str(), request.path.as_str()),
            (
                "DELETE",
                "/v1/groups/44444444444444444444444444/members/demo"
            )
        );
        assert_eq!(
            core.delete_group_request(id).unwrap().path,
            "/v1/groups/44444444444444444444444444"
        );
        assert_eq!(
            words(core.group(
                404,
                br#"{"error":{"code":"group_not_found","message":"x"}}"#.to_vec()
            )),
            "This group is no longer there, or you are no longer in it."
        );
    }

    #[test]
    fn without_an_account_nothing_is_asked() {
        let folder = TempFolder::new();
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        assert_eq!(
            words(core.groups_request()),
            "This device is not signed in."
        );
        assert_eq!(
            words(core.plan_board_request("2026-11-18".to_owned(), None)),
            "This device is not signed in."
        );
        assert_eq!(
            words(core.join_group_request("4444-4444-4444-4444".to_owned())),
            "This device is not signed in."
        );
        // An answer is still read, with nobody as "me".
        let board = core.plan_board(200, golden("plan_board.json")).unwrap();
        let plan = &board.days[0].plans[0];
        assert!(!plan.is_mine && !plan.am_in && !plan.can_delete);
        assert_eq!(plan.in_words, "Demo Pilot, Mara Muster and Jonas Beispiel");
    }

    #[test]
    fn who_is_in_is_said_in_a_line() {
        let names = |names: &[&str]| -> Vec<String> {
            names.iter().map(|name| (*name).to_owned()).collect()
        };
        assert_eq!(in_words(&[]), "Nobody is in yet");
        assert_eq!(in_words(&names(&["You"])), "Only you so far");
        assert_eq!(in_words(&names(&["Mara Muster"])), "Mara Muster");
        assert_eq!(
            in_words(&names(&["You", "Mara Muster"])),
            "You and Mara Muster"
        );
        assert_eq!(
            in_words(&names(&["You", "Mara", "Jonas", "Ida", "Ben"])),
            "You, Mara and 3 others"
        );
    }
}
