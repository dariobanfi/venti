//! Groups of pilots and the plans their members make for a flying day:
//! making a group, coming into one with an invite code, what its members
//! and its keeper can do in it, posting a plan, saying "I am in", and the
//! plans of all of a pilot's groups in one answer.
//!
//! **A group is seen by its members and by nobody else.** Every request
//! about a group asks the store as the account that sent it, and the store
//! tells an account that is not in the group what it tells of a group that
//! does not exist: the two arrive here as one value and leave as one
//! answer, [`no_such_group`]. No handler looks a group up first and asks
//! afterwards who may see it. A group is in no list but its members' own,
//! and nothing finds one by its name.
//!
//! **Whoever has the invite code can come in**, so it is treated as a
//! secret: it travels in a body and never in a path, it is shown to the
//! keeper once and kept as a hash, and trying codes is counted.
//!
//! **Nothing of a group reaches the log.** No line is written for
//! anything a pilot does here, and no error message repeats a name, a
//! code, a place or a note. The request log leaves out of a path whatever
//! follows `/v1/groups/`: a group's and a plan's identifier and a member's
//! handle; see [`super::log`].

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{NaiveDate, NaiveTime};
use serde::Deserialize;
use venti_api_types::v1::{
    CreateGroupRequest, CreateGroupResponse, ErrorCode, GroupId, GroupInviteCode,
    GroupInviteResponse, GroupTextError, GroupsResponse, HandOverGroupRequest, Handle,
    JoinGroupRequest, JoinGroupResponse, MAX_BOARD_PLANS, PlanBoardResponse, PlanId, PlanRequest,
    PlanResponse, PlansResponse, UpdateGroupRequest, format_day, parse_day, parse_group_name,
    parse_plan_note, parse_plan_place,
};
use venti_domain::Timestamp;
use venti_store::{
    GroupCreation, GroupJoining, KeeperAct, MemberRemoval, NewGroup, NewPlan, PlanChange,
    PlanDetails, PlanRemoval, PlanSaving, Store,
};

use super::auth::Signed;
use super::error::ApiError;
use super::{Api, body, cache, wire};
use crate::accounts::secret::{InviteCode, random_id};
use crate::accounts::throttle::{Counted, Throttle};
use crate::groups::{Limits, today};

/// What the handlers keep between requests: the limits, and how much each
/// account has used of those that are counted over a day.
#[derive(Debug)]
pub(crate) struct Groups {
    pub(crate) limits: Limits,
    /// Groups made, by account.
    made: Throttle,
    /// Invite codes tried, by account.
    joins: Throttle,
    /// Plans posted or changed, by account.
    plans: Throttle,
}

impl Groups {
    pub(crate) fn new(limits: Limits, watched_keys: usize) -> Self {
        let a_day = chrono::Duration::days(1);
        Self {
            limits,
            made: Throttle::new("accounts that made groups", a_day, watched_keys),
            joins: Throttle::new("accounts that tried invite codes", a_day, watched_keys),
            plans: Throttle::new("accounts that posted plans", a_day, watched_keys),
        }
    }
}

// --- The answers that say no ----------------------------------------------------

/// The answer for a group the asker is not in: no group has the
/// identifier, or one has and the asker is no member of it. The two must
/// not differ in anything.
fn no_such_group() -> ApiError {
    ApiError::missing(
        ErrorCode::GroupNotFound,
        "there is no group with this identifier that you are in",
    )
}

fn no_such_plan() -> ApiError {
    ApiError::missing(
        ErrorCode::PlanNotFound,
        "the group has no plan with this identifier",
    )
}

fn no_such_member() -> ApiError {
    ApiError::missing(
        ErrorCode::GroupMemberNotFound,
        "the group has no member with this handle",
    )
}

fn not_keeper() -> ApiError {
    ApiError::forbidden(
        ErrorCode::NotGroupKeeper,
        "only the keeper of a group may do this",
    )
}

/// The answer for a code that lets nobody in. It is the same whatever is
/// wrong with the code.
fn invalid_invite() -> ApiError {
    ApiError::forbidden(
        ErrorCode::InvalidGroupInvite,
        "this is not the invite code of a group: it is mistyped, or the group's keeper has \
         replaced or ended it; ask them for the code that works now",
    )
}

fn too_many_groups(limits: &Limits) -> ApiError {
    ApiError::forbidden(
        ErrorCode::TooManyGroups,
        format!(
            "this account is in {} groups, which is as many as an account may be in; leave \
             one to make room",
            limits.groups_per_account
        ),
    )
}

/// What is wrong with a name, a place or a note, in words that repeat
/// nothing of it.
fn text_problem(what: &str, error: GroupTextError) -> String {
    match error {
        GroupTextError::Empty => format!("{what} must not be empty"),
        GroupTextError::TooLong(most) => format!("{what} has at most {most} characters"),
    }
}

fn invalid_name(error: GroupTextError) -> ApiError {
    ApiError::bad_request(
        ErrorCode::InvalidGroupName,
        text_problem("the name of a group", error),
    )
}

fn invalid_plan(problem: impl Into<String>) -> ApiError {
    ApiError::bad_request(ErrorCode::InvalidPlan, problem)
}

/// What became of something only the keeper may do, as an answer.
fn kept<T>(act: KeeperAct<T>) -> Result<T, ApiError> {
    match act {
        KeeperAct::Done(done) => Ok(done),
        KeeperAct::NotKeeper => Err(not_keeper()),
        KeeperAct::NoSuchMember => Err(no_such_member()),
        KeeperAct::NoSuchGroup => Err(no_such_group()),
    }
}

// --- Reading a request -------------------------------------------------------------

/// The identifier of a group from a path. Text of another form is the
/// identifier of no group.
fn group_id(text: &str) -> Result<GroupId, ApiError> {
    text.parse().map_err(|_| no_such_group())
}

/// A member's handle as the store is asked for it. Text that cannot be a
/// handle is the handle of no member; the empty text stands for it, which
/// no account has.
fn member_handle(text: &str) -> String {
    Handle::new(text)
        .map(|handle| handle.as_str().to_owned())
        .unwrap_or_default()
}

#[derive(Debug, Deserialize)]
pub(crate) struct DayParams {
    from: Option<String>,
    to: Option<String>,
}

impl<S: Store> Api<S> {
    /// The identifiers of a group and of one of its plans from a path. A
    /// plan's identifier of another form is the identifier of no plan,
    /// which only a member of the group is told.
    async fn plan_ids(
        &self,
        signed: &Signed,
        group: &str,
        plan: &str,
    ) -> Result<(GroupId, PlanId), ApiError> {
        let group = group_id(group)?;
        if let Ok(plan) = plan.parse() {
            return Ok((group, plan));
        }
        let in_it = self
            .store
            .group(group.as_str(), &signed.account.id)
            .await?
            .is_some();
        Err(if in_it {
            no_such_plan()
        } else {
            no_such_group()
        })
    }

    /// The days a list of plans is asked for: the first, and the last if
    /// one was named. Without a first, the server's own date. Days whose
    /// plans are gone are not asked for, whatever the request says.
    fn days(
        &self,
        params: &DayParams,
        now: Timestamp,
    ) -> Result<(NaiveDate, Option<NaiveDate>), ApiError> {
        let read = |name: &str, text: &Option<String>| {
            text.as_deref()
                .map(|text| {
                    parse_day(text).map_err(|error| {
                        ApiError::bad_request(ErrorCode::InvalidDay, format!("{name}: {error}"))
                    })
                })
                .transpose()
        };
        let from = read("from", &params.from)?.unwrap_or_else(|| today(now));
        let to = read("to", &params.to)?;
        if to.is_some_and(|to| from > to) {
            return Err(ApiError::bad_request(
                ErrorCode::InvalidRange,
                "from lies after to",
            ));
        }
        Ok((from.max(self.groups.limits.gone_before(now)), to))
    }

    /// What a plan is to say, from a request, as it is kept.
    fn plan_details(&self, request: &PlanRequest, now: Timestamp) -> Result<PlanDetails, ApiError> {
        let limits = &self.groups.limits;
        let (first, last) = (limits.first_day(now), limits.last_day(now));
        if request.day < first || last.is_some_and(|last| request.day > last) {
            let until = last.map_or_else(String::new, |last| format!(" to {}", format_day(last)));
            return Err(invalid_plan(format!(
                "a plan is for a day from {}{until}; the server's clock shows {} in UTC",
                format_day(first),
                format_day(today(now)),
            )));
        }
        let place = parse_plan_place(&request.place)
            .map_err(|error| invalid_plan(text_problem("the place of a plan", error)))?;
        let note = parse_plan_note(request.note.as_deref().unwrap_or_default())
            .map_err(|error| invalid_plan(text_problem("the note of a plan", error)))?;
        Ok(PlanDetails {
            day: request.day,
            place,
            position: request.position,
            meet_at: request.meet_at.and_then(|time| {
                NaiveTime::from_hms_opt(time.hour().into(), time.minute().into(), 0)
            }),
            note,
        })
    }

    /// Counts a plan posted or changed by an account, whatever becomes of
    /// it.
    fn count_plan(&self, signed: &Signed, now: Timestamp) -> Result<(), ApiError> {
        let limit = self.groups.limits.plans_posted_per_day;
        let by_day = [Counted::new(&signed.account.id, limit)];
        self.groups.plans.attempt(&by_day, now).map_err(|refused| {
            ApiError::rate_limited(
                refused,
                "this account has posted or changed as many plans today as one account may",
            )
        })
    }
}

fn plan_answer(status: StatusCode, plan: &venti_store::Plan) -> Result<Response, ApiError> {
    let plan = wire::plan(plan)?;
    Ok(cache::private_json(status, &PlanResponse { plan }))
}

fn no_content() -> Response {
    cache::private(StatusCode::NO_CONTENT.into_response())
}

// --- Groups ------------------------------------------------------------------------

/// `POST /v1/groups`: makes a group that the asker keeps.
pub(crate) async fn create<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    body::must_say_json(&headers)?;
    let now = api.clock.now();
    let limits = &api.groups.limits;
    let by_day = [Counted::new(&signed.account.id, limits.groups_made_per_day)];
    api.groups.made.attempt(&by_day, now).map_err(|refused| {
        ApiError::rate_limited(
            refused,
            "this account has made as many groups today as one account may",
        )
    })?;

    let request: CreateGroupRequest = body::json(&headers, &body)?;
    let name = parse_group_name(&request.name).map_err(invalid_name)?;
    // The code is shown in this answer and nowhere else: the store is
    // handed its hash.
    let code = InviteCode::new()?;
    let new = NewGroup {
        id: random_id()?,
        name,
        created_at: now,
        keeper_id: signed.account.id.clone(),
        invite: code.group_hash(),
    };
    match api.store.create_group(&new, &limits.room()).await? {
        GroupCreation::Created(group) => {
            let answer = CreateGroupResponse {
                group: wire::group(&group)?,
                invite_code: GroupInviteCode::new(code.to_string()),
            };
            Ok(cache::private_json(StatusCode::CREATED, &answer))
        }
        GroupCreation::TooManyGroups => Err(too_many_groups(limits)),
    }
}

/// `GET /v1/groups`: the groups the asker is in.
pub(crate) async fn list<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
) -> Result<Response, ApiError> {
    let groups = api.store.groups_of(&signed.account.id).await?;
    let answer = GroupsResponse {
        groups: wire::groups(&groups)?,
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `POST /v1/groups/join`: makes the asker a member of the group whose
/// invite code the body carries.
pub(crate) async fn join<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    body::must_say_json(&headers)?;
    let now = api.clock.now();
    let limits = &api.groups.limits;
    // Counted before the code is looked at, right or wrong: this is what
    // makes trying codes slow.
    let by_day = [Counted::new(&signed.account.id, limits.joins_per_day)];
    api.groups.joins.attempt(&by_day, now).map_err(|refused| {
        ApiError::rate_limited(
            refused,
            "this account has tried as many invite codes today as one account may",
        )
    })?;

    let request: JoinGroupRequest = body::json(&headers, &body)?;
    // Text that cannot be a code is the code of no group.
    let code = InviteCode::parse(request.code.expose()).ok_or_else(invalid_invite)?;
    let joining = api
        .store
        .join_group(&signed.account.id, &code.group_hash(), &limits.room(), now)
        .await?;
    let (group, joined) = match joining {
        GroupJoining::Joined(group) => (group, true),
        GroupJoining::AlreadyIn(group) => (group, false),
        GroupJoining::NoSuchInvite => return Err(invalid_invite()),
        GroupJoining::GroupFull => {
            return Err(ApiError::forbidden(
                ErrorCode::GroupFull,
                format!(
                    "the group has {} members, which is as many as a group may have",
                    limits.members_per_group
                ),
            ));
        }
        GroupJoining::TooManyGroups => return Err(too_many_groups(limits)),
    };
    let status = if joined {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    let answer = JoinGroupResponse {
        group: wire::group(&group)?,
        joined,
    };
    Ok(cache::private_json(status, &answer))
}

/// `GET /v1/groups/{id}`: a group and its members, for one of them.
pub(crate) async fn show<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let id = group_id(&id)?;
    let seen = api
        .store
        .group(id.as_str(), &signed.account.id)
        .await?
        .ok_or_else(no_such_group)?;
    let answer = wire::group_with_members(&seen)?;
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `PATCH /v1/groups/{id}`: the keeper gives the group a new name.
pub(crate) async fn rename<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let id = group_id(&id)?;
    let request: UpdateGroupRequest = body::json(&headers, &body)?;
    let name = parse_group_name(&request.name).map_err(invalid_name)?;
    let renamed = api
        .store
        .rename_group(id.as_str(), &signed.account.id, &name)
        .await?;
    let answer = wire::group_with_members(&kept(renamed)?)?;
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `DELETE /v1/groups/{id}`: the keeper deletes the group, with its plans.
pub(crate) async fn delete<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let id = group_id(&id)?;
    let deleted = api
        .store
        .delete_group(id.as_str(), &signed.account.id)
        .await?;
    kept(deleted)?;
    Ok(no_content())
}

/// `POST /v1/groups/{id}/invite`: the keeper makes a new invite code. The
/// one before it lets nobody in from then on.
pub(crate) async fn renew_invite<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let id = group_id(&id)?;
    let code = InviteCode::new()?;
    let renewed = api
        .store
        .renew_group_invite(id.as_str(), &signed.account.id, &code.group_hash())
        .await?;
    kept(renewed)?;
    let answer = GroupInviteResponse {
        invite_code: GroupInviteCode::new(code.to_string()),
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `PUT /v1/groups/{id}/keeper`: the keeper hands the keeping to another
/// member.
pub(crate) async fn hand_over<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let id = group_id(&id)?;
    let request: HandOverGroupRequest = body::json(&headers, &body)?;
    let handed = api
        .store
        .hand_over_group(
            id.as_str(),
            &signed.account.id,
            &member_handle(&request.handle),
        )
        .await?;
    let answer = wire::group_with_members(&kept(handed)?)?;
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `DELETE /v1/groups/{id}/members/{handle}`: a member leaves, by their
/// own handle, or the keeper removes the member with the handle.
pub(crate) async fn remove_member<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Path((id, handle)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let id = group_id(&id)?;
    let removal = api
        .store
        .remove_group_member(id.as_str(), &signed.account.id, &member_handle(&handle))
        .await?;
    match removal {
        MemberRemoval::Removed | MemberRemoval::GroupRemoved => Ok(no_content()),
        MemberRemoval::NotKeeper => Err(ApiError::forbidden(
            ErrorCode::NotGroupKeeper,
            "a member leaves a group by their own handle; only the keeper of the group \
             removes another member",
        )),
        MemberRemoval::NoSuchMember => Err(no_such_member()),
        MemberRemoval::NoSuchGroup => Err(no_such_group()),
    }
}

// --- Plans ---------------------------------------------------------------------------

/// `GET /v1/groups/{id}/plans?from=&to=`: a group's plans for today and
/// the days ahead, or for the days asked for.
pub(crate) async fn plans<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Path(id): Path<String>,
    Query(params): Query<DayParams>,
) -> Result<Response, ApiError> {
    let id = group_id(&id)?;
    let (from, to) = api.days(&params, api.clock.now())?;
    let plans = api
        .store
        .plans_of_group(id.as_str(), &signed.account.id, from, to)
        .await?
        .ok_or_else(no_such_group)?;
    let answer = PlansResponse {
        plans: wire::plans(&plans)?,
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `GET /v1/groups/plans?from=&to=`: the plans of every group the asker
/// is in, with those groups.
pub(crate) async fn board<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Query(params): Query<DayParams>,
) -> Result<Response, ApiError> {
    let (from, to) = api.days(&params, api.clock.now())?;
    let account_id = &signed.account.id;
    let groups = api.store.groups_of(account_id).await?;
    // One more than an answer carries, to know whether there are more.
    let mut plans = api
        .store
        .plans_for(account_id, from, to, MAX_BOARD_PLANS + 1)
        .await?;
    let truncated = plans.len() > MAX_BOARD_PLANS;
    plans.truncate(MAX_BOARD_PLANS);
    let answer = PlanBoardResponse {
        groups: wire::groups(&groups)?,
        plans: wire::plans(&plans)?,
        truncated,
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `POST /v1/groups/{id}/plans`: a member posts a plan to the group.
pub(crate) async fn post_plan<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    body::must_say_json(&headers)?;
    let now = api.clock.now();
    api.count_plan(&signed, now)?;
    let id = group_id(&id)?;
    let request: PlanRequest = body::json(&headers, &body)?;
    let new = NewPlan {
        id: random_id()?,
        group_id: id.as_str().to_owned(),
        author_id: signed.account.id.clone(),
        created_at: now,
        details: api.plan_details(&request, now)?,
    };
    let room = api.groups.limits.plan_room(now);
    match api.store.save_plan(&new, &room).await? {
        PlanSaving::Stored(plan) => plan_answer(StatusCode::CREATED, &plan),
        PlanSaving::GroupFull => Err(ApiError::forbidden(
            ErrorCode::GroupPlansFull,
            format!(
                "the group holds {} plans, which is as many as a group may hold at a time; \
                 delete one to make room",
                room.plans_per_group
            ),
        )),
        PlanSaving::NoSuchGroup => Err(no_such_group()),
    }
}

/// `PUT /v1/groups/{id}/plans/{plan}`: the member who posted a plan
/// replaces what it says.
pub(crate) async fn change_plan<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Path((id, plan)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    body::must_say_json(&headers)?;
    let now = api.clock.now();
    api.count_plan(&signed, now)?;
    let (id, plan) = api.plan_ids(&signed, &id, &plan).await?;
    let request: PlanRequest = body::json(&headers, &body)?;
    let details = api.plan_details(&request, now)?;
    let gone_before = api.groups.limits.gone_before(now);
    let change = api
        .store
        .change_plan(
            id.as_str(),
            plan.as_str(),
            &signed.account.id,
            &details,
            now,
            gone_before,
        )
        .await?;
    match change {
        PlanChange::Now(plan) => plan_answer(StatusCode::OK, &plan),
        PlanChange::NotAuthor => Err(ApiError::forbidden(
            ErrorCode::NotPlanAuthor,
            "only the member who posted a plan may change it",
        )),
        PlanChange::NoSuchPlan => Err(no_such_plan()),
        PlanChange::NoSuchGroup => Err(no_such_group()),
    }
}

/// `DELETE /v1/groups/{id}/plans/{plan}`: the member who posted a plan, or
/// the keeper of its group, deletes it.
pub(crate) async fn delete_plan<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Path((id, plan)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let (id, plan) = api.plan_ids(&signed, &id, &plan).await?;
    let gone_before = api.groups.limits.gone_before(api.clock.now());
    let removal = api
        .store
        .delete_plan(id.as_str(), plan.as_str(), &signed.account.id, gone_before)
        .await?;
    match removal {
        PlanRemoval::Removed => Ok(no_content()),
        PlanRemoval::NotAllowed => Err(ApiError::forbidden(
            ErrorCode::NotPlanAuthor,
            "only the member who posted a plan and the keeper of its group may delete it",
        )),
        PlanRemoval::NoSuchPlan => Err(no_such_plan()),
        PlanRemoval::NoSuchGroup => Err(no_such_group()),
    }
}

/// Notes that the asker is in on a plan, or takes that back.
async fn set_in<S: Store>(
    api: &Api<S>,
    signed: &Signed,
    (id, plan): (&str, &str),
    is_in: bool,
) -> Result<Response, ApiError> {
    let (id, plan) = api.plan_ids(signed, id, plan).await?;
    let now = api.clock.now();
    let change = api
        .store
        .set_plan_in(
            id.as_str(),
            plan.as_str(),
            &signed.account.id,
            is_in,
            now,
            api.groups.limits.gone_before(now),
        )
        .await?;
    match change {
        PlanChange::Now(plan) => plan_answer(StatusCode::OK, &plan),
        // The store answers this to a change of what a plan says only.
        PlanChange::NotAuthor => Err(ApiError::internal()),
        PlanChange::NoSuchPlan => Err(no_such_plan()),
        PlanChange::NoSuchGroup => Err(no_such_group()),
    }
}

/// `PUT /v1/groups/{id}/plans/{plan}/in`: the asker is in.
pub(crate) async fn say_in<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Path((id, plan)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    set_in(&api, &signed, (&id, &plan), true).await
}

/// `DELETE /v1/groups/{id}/plans/{plan}/in`: the asker takes that back.
pub(crate) async fn take_back<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Path((id, plan)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    set_in(&api, &signed, (&id, &plan), false).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_handle_from_a_path_is_read_as_a_handle_or_as_nobodys() {
        assert_eq!(member_handle("Mara"), "mara");
        assert_eq!(member_handle("@jonas "), "jonas");
        for nobody in ["", "x", "../account", "a\0b", "mara' OR '1'='1", "häns"] {
            assert_eq!(member_handle(nobody), "", "{nobody:?}");
        }
    }

    #[test]
    fn what_is_wrong_with_a_text_is_said_without_the_text() {
        assert_eq!(
            text_problem("the name of a group", GroupTextError::Empty),
            "the name of a group must not be empty"
        );
        assert_eq!(
            text_problem("the note of a plan", GroupTextError::TooLong(500)),
            "the note of a plan has at most 500 characters"
        );
    }
}
