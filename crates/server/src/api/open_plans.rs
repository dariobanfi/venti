//! Open plans: a pilot posts a plan for everyone flying from the area,
//! pilots asking about a place find the open plans near it for the coming
//! days, and every signed-in pilot can say "I am in".
//!
//! **Public by its author's choice.** An open plan is in no group, and
//! everyone may read it without a token, as every other public record of
//! the platform (owner decision 16). Its author chose that when posting it
//! open; whoever says they are in chooses to be named on it. A plan and
//! everyone on it are named by handle and display name and nothing else:
//! no picture, no country, no flight.
//!
//! **Bounded.** An account holds a few open plans at a time, posts or
//! changes a few a day, and a plan has room for so many pilots; a note is
//! short ([`venti_api_types::v1::MAX_OPEN_PLAN_NOTE_LENGTH`]). A plan is
//! for a day from the server's yesterday to a month ahead, and is gone two
//! days after its day like a group's plan. An account whose publishing is
//! suspended posts none and says it is in on none.
//!
//! **Nothing of a plan reaches the log**, as for a group's: no error
//! message repeats a place or a note.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{Days, NaiveDate, NaiveTime, Timelike};
use serde::Deserialize;
use venti_api_types::v1::{
    self as v1, DEFAULT_OPEN_PLANS_RADIUS_KM, ErrorCode, MAX_OPEN_PLAN_SEATS, MAX_OPEN_PLANS,
    MAX_OPEN_PLANS_RADIUS_KM, OPEN_PLAN_DAYS_AHEAD, OPEN_PLANS_DAYS, OpenPlanRequest,
    OpenPlanResponse, OpenPlansResponse, PlanId, format_day, parse_day, parse_open_plan_note,
    parse_plan_place, parse_position,
};
use venti_domain::Timestamp;
use venti_store::{
    NewOpenPlan, OpenPlan, OpenPlanArea, OpenPlanChange, OpenPlanDetails, OpenPlanRemoval,
    OpenPlanRoom, OpenPlanSaving, Store,
};

use super::auth::{May, ReadGroups, Signed, WriteGroups};
use super::budget::Reader;
use super::error::ApiError;
use super::groups::{DayParams, invalid_plan, text_problem};
use super::{Api, body, cache};
use crate::accounts::secret::random_id;
use crate::accounts::throttle::Counted;
use crate::groups::today;

/// The answer for an open plan that is not there: none ever, deleted,
/// removed, or one whose day lies further back than plans are kept.
pub(crate) fn no_such_open_plan() -> ApiError {
    ApiError::missing(
        ErrorCode::PlanNotFound,
        "there is no open plan with this identifier",
    )
}

fn not_author() -> ApiError {
    ApiError::forbidden(
        ErrorCode::NotPlanAuthor,
        "only the pilot who posted an open plan may change or delete it",
    )
}

fn suspended() -> ApiError {
    ApiError::forbidden(
        ErrorCode::PublishingSuspended,
        "whoever runs this server has suspended this account's publishing: it posts no open \
         plan and says it is in on none; what it posted it can still delete",
    )
}

/// The identifier of an open plan from a path. Text of another form is the
/// identifier of none.
fn plan_id(text: &str) -> Result<PlanId, ApiError> {
    text.parse().map_err(|_| no_such_open_plan())
}

/// A stored open plan as everyone reads it: its author and its pilots by
/// handle and display name only.
pub(crate) fn wire(plan: &OpenPlan) -> Result<v1::OpenPlan, ApiError> {
    let pilot =
        |pilot: &venti_store::GroupPilot| super::wire::pilot(&pilot.handle, &pilot.display_name);
    let details = &plan.details;
    let meet_at = details.meet_at.and_then(|time| {
        let part = |value: u32| u8::try_from(value).ok();
        part(time.hour())
            .zip(part(time.minute()))
            .and_then(|(hour, minute)| v1::TimeOfDay::new(hour, minute))
    });
    Ok(v1::OpenPlan {
        id: plan.id.parse().map_err(|_| ApiError::internal())?,
        day: details.day,
        place: details.place.clone(),
        position: details.position,
        meet_at,
        seats: details.seats,
        note: details.note.clone(),
        author: pilot(&plan.author)?,
        created_at: plan.created_at,
        changed_at: plan.changed_at,
        pilots_in: plan.pilots_in.iter().map(pilot).collect::<Result<_, _>>()?,
    })
}

fn plan_answer(status: StatusCode, plan: &OpenPlan) -> Result<Response, ApiError> {
    let plan = wire(plan)?;
    Ok(cache::private_json(status, &OpenPlanResponse { plan }))
}

fn plans_answer(mut plans: Vec<OpenPlan>) -> Result<OpenPlansResponse, ApiError> {
    let truncated = plans.len() > MAX_OPEN_PLANS;
    plans.truncate(MAX_OPEN_PLANS);
    Ok(OpenPlansResponse {
        plans: plans.iter().map(wire).collect::<Result<_, _>>()?,
        truncated,
    })
}

impl<S: Store> Api<S> {
    /// How many open plans an account may hold and how many pilots one
    /// may have in at `now`, as the store is told.
    fn open_plan_room(&self, now: Timestamp) -> OpenPlanRoom {
        let limits = &self.groups.limits;
        OpenPlanRoom {
            plans_per_account: limits.open_plans_per_account,
            pilots_per_plan: limits.pilots_per_open_plan,
            gone_before: limits.gone_before(now),
        }
    }

    /// What an open plan is to say, from a request, as it is kept.
    fn open_plan_details(
        &self,
        request: &OpenPlanRequest,
        now: Timestamp,
    ) -> Result<OpenPlanDetails, ApiError> {
        let limits = &self.groups.limits;
        let first = limits.first_day(now);
        // A month ahead, and any day in the development mode, whose clock
        // stands still.
        let last = limits.days_ahead.map(|_| {
            today(now)
                .checked_add_days(Days::new(u64::from(OPEN_PLAN_DAYS_AHEAD)))
                .unwrap_or(NaiveDate::MAX)
        });
        if request.day < first || last.is_some_and(|last| request.day > last) {
            let until = last.map_or_else(String::new, |last| format!(" to {}", format_day(last)));
            return Err(invalid_plan(format!(
                "an open plan is for a day from {}{until}; the server's clock shows {} in UTC",
                format_day(first),
                format_day(today(now)),
            )));
        }
        let place = parse_plan_place(&request.place)
            .map_err(|error| invalid_plan(text_problem("the place of a plan", error)))?;
        let note = parse_open_plan_note(request.note.as_deref().unwrap_or_default())
            .map_err(|error| invalid_plan(text_problem("the note of an open plan", error)))?;
        if request
            .seats
            .is_some_and(|seats| !(1..=MAX_OPEN_PLAN_SEATS).contains(&seats))
        {
            return Err(invalid_plan(format!(
                "seats is 1 to {MAX_OPEN_PLAN_SEATS}, or left out by a pilot looking for company"
            )));
        }
        Ok(OpenPlanDetails {
            day: request.day,
            place,
            position: request.position,
            meet_at: request.meet_at.and_then(|time| {
                NaiveTime::from_hms_opt(time.hour().into(), time.minute().into(), 0)
            }),
            seats: request.seats,
            note,
        })
    }

    /// Counts an open plan posted or changed, whatever becomes of it; an
    /// account whose publishing is suspended posts none.
    fn count_open_plan(&self, signed: &Signed, now: Timestamp) -> Result<(), ApiError> {
        if signed.account.publishing_suspended_at.is_some() {
            return Err(suspended());
        }
        let limit = self.groups.limits.open_plans_posted_per_day;
        let by_day = [Counted::new(&signed.account.id, limit)];
        self.groups
            .open_plans
            .attempt(&by_day, now)
            .map_err(|refused| {
                ApiError::rate_limited(
                    refused,
                    "this account has posted or changed as many open plans today as one \
                     account may",
                )
            })
    }

    /// One open plan as everyone reads it, for a report about it.
    pub(crate) async fn open_plan_as_anyone(
        &self,
        id: &str,
        now: Timestamp,
    ) -> Result<OpenPlan, ApiError> {
        let id = plan_id(id)?;
        let gone_before = self.groups.limits.gone_before(now);
        self.store
            .open_plan(id.as_str(), gone_before)
            .await?
            .ok_or_else(no_such_open_plan)
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct NearParams {
    lat: Option<String>,
    lon: Option<String>,
    radius_km: Option<String>,
    #[serde(flatten)]
    days: DayParams,
}

/// `GET /v1/open-plans?lat=&lon=&radius_km=&from=&to=`: the open plans
/// whose place is near a place, for the days asked for. Open to anyone,
/// and counted as a read of public data is.
pub(crate) async fn near<S: Store>(
    State(api): State<Arc<Api<S>>>,
    reader: Reader,
    Query(params): Query<NearParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    api.count_public_read(&reader)?;
    let now = api.clock.now();
    let around = parse_position(params.lat.as_deref(), params.lon.as_deref()).map_err(|error| {
        ApiError::bad_request(
            ErrorCode::InvalidPosition,
            format!(
                "{error}; give the place as lat and lon in degrees, for example lat=47.7&lon=11.8"
            ),
        )
    })?;
    let radius_km = match params.radius_km.as_deref() {
        None => DEFAULT_OPEN_PLANS_RADIUS_KM,
        Some(text) => text
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|km| (1..=MAX_OPEN_PLANS_RADIUS_KM).contains(km))
            .ok_or_else(|| {
                ApiError::bad_request(
                    ErrorCode::InvalidRequest,
                    format!("radius_km is a whole number from 1 to {MAX_OPEN_PLANS_RADIUS_KM}"),
                )
            })?,
    };
    let read = |name: &str, text: &Option<String>| {
        text.as_deref()
            .map(|text| {
                parse_day(text).map_err(|error| {
                    ApiError::bad_request(ErrorCode::InvalidDay, format!("{name}: {error}"))
                })
            })
            .transpose()
    };
    let from = read("from", &params.days.from)?.unwrap_or_else(|| today(now));
    let most = from
        .checked_add_days(Days::new(u64::from(OPEN_PLANS_DAYS - 1)))
        .unwrap_or(NaiveDate::MAX);
    let to = read("to", &params.days.to)?.unwrap_or(most);
    if from > to {
        return Err(ApiError::bad_request(
            ErrorCode::InvalidRange,
            "from lies after to",
        ));
    }
    if to > most {
        return Err(ApiError::bad_request(
            ErrorCode::InvalidRange,
            format!("one request covers at most {OPEN_PLANS_DAYS} days"),
        ));
    }
    let area = OpenPlanArea {
        around,
        radius_m: f64::from(radius_km) * 1000.0,
        from: from.max(api.groups.limits.gone_before(now)),
        to,
    };
    // One more than an answer carries, to know whether there are more.
    let plans = api.store.open_plans_near(&area, MAX_OPEN_PLANS + 1).await?;
    let answer = plans_answer(plans)?;
    Ok(cache::shared_json(&headers, &answer, cache::Keeper::Anyone))
}

/// `GET /v1/open-plans/{plan}`: one open plan, for anyone.
pub(crate) async fn show<S: Store>(
    State(api): State<Arc<Api<S>>>,
    reader: Reader,
    Path(plan): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    api.count_public_read(&reader)?;
    let plan = api.open_plan_as_anyone(&plan, api.clock.now()).await?;
    let answer = OpenPlanResponse { plan: wire(&plan)? };
    Ok(cache::shared_json(&headers, &answer, cache::Keeper::Anyone))
}

/// `GET /v1/account/open-plans`: the open plans the asker posted or is in
/// on, from the server's yesterday on.
pub(crate) async fn mine<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<ReadGroups>,
) -> Result<Response, ApiError> {
    let now = api.clock.now();
    let from = api.groups.limits.first_day(now);
    let plans = api
        .store
        .open_plans_of(&signed.account.id, from, MAX_OPEN_PLANS + 1)
        .await?;
    Ok(cache::private_json(StatusCode::OK, &plans_answer(plans)?))
}

/// `POST /v1/open-plans`: a pilot posts an open plan.
pub(crate) async fn post<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteGroups>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    body::must_say_json(&headers)?;
    let now = api.clock.now();
    api.count_open_plan(&signed, now)?;
    let request: OpenPlanRequest = body::json(&headers, &body)?;
    let new = NewOpenPlan {
        id: random_id()?,
        author_id: signed.account.id.clone(),
        created_at: now,
        details: api.open_plan_details(&request, now)?,
    };
    let room = api.open_plan_room(now);
    match api.store.save_open_plan(&new, &room).await? {
        OpenPlanSaving::Stored(plan) => plan_answer(StatusCode::CREATED, &plan),
        OpenPlanSaving::TooMany => Err(ApiError::forbidden(
            ErrorCode::TooManyOpenPlans,
            format!(
                "this account holds {} open plans, which is as many as an account may hold at \
                 a time; delete one to make room",
                room.plans_per_account
            ),
        )),
    }
}

/// `PUT /v1/open-plans/{plan}`: the pilot who posted an open plan replaces
/// what it says.
pub(crate) async fn change<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteGroups>,
    Path(plan): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    body::must_say_json(&headers)?;
    let now = api.clock.now();
    api.count_open_plan(&signed, now)?;
    let plan = plan_id(&plan)?;
    let request: OpenPlanRequest = body::json(&headers, &body)?;
    let details = api.open_plan_details(&request, now)?;
    let gone_before = api.groups.limits.gone_before(now);
    let change = api
        .store
        .change_open_plan(
            plan.as_str(),
            &signed.account.id,
            &details,
            now,
            gone_before,
        )
        .await?;
    match change {
        OpenPlanChange::Now(plan) => plan_answer(StatusCode::OK, &plan),
        OpenPlanChange::NotAuthor => Err(not_author()),
        // The store answers this to saying "I am in" only.
        OpenPlanChange::Full => Err(ApiError::internal()),
        OpenPlanChange::NoSuchPlan => Err(no_such_open_plan()),
    }
}

/// `DELETE /v1/open-plans/{plan}`: the pilot who posted an open plan
/// deletes it.
pub(crate) async fn delete<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteGroups>,
    Path(plan): Path<String>,
) -> Result<Response, ApiError> {
    let plan = plan_id(&plan)?;
    let gone_before = api.groups.limits.gone_before(api.clock.now());
    let removal = api
        .store
        .delete_open_plan(plan.as_str(), &signed.account.id, gone_before)
        .await?;
    match removal {
        OpenPlanRemoval::Removed => Ok(cache::private(StatusCode::NO_CONTENT.into_response())),
        OpenPlanRemoval::NotAuthor => Err(not_author()),
        OpenPlanRemoval::NoSuchPlan => Err(no_such_open_plan()),
    }
}

/// Notes that the asker is in on an open plan, or takes that back.
async fn set_in<S: Store>(
    api: &Api<S>,
    signed: &Signed,
    plan: &str,
    is_in: bool,
) -> Result<Response, ApiError> {
    // Taking back is always allowed; coming in is publishing one's name.
    if is_in && signed.account.publishing_suspended_at.is_some() {
        return Err(suspended());
    }
    let plan = plan_id(plan)?;
    let now = api.clock.now();
    let room = api.open_plan_room(now);
    let change = api
        .store
        .set_open_plan_in(plan.as_str(), &signed.account.id, is_in, now, &room)
        .await?;
    match change {
        OpenPlanChange::Now(plan) => plan_answer(StatusCode::OK, &plan),
        OpenPlanChange::Full => Err(ApiError::forbidden(
            ErrorCode::OpenPlanFull,
            format!(
                "{} pilots are in on this plan, which is as many as one may have",
                room.pilots_per_plan
            ),
        )),
        // The store answers this to a change of what a plan says only.
        OpenPlanChange::NotAuthor => Err(ApiError::internal()),
        OpenPlanChange::NoSuchPlan => Err(no_such_open_plan()),
    }
}

/// `PUT /v1/open-plans/{plan}/in`: the asker is in.
pub(crate) async fn say_in<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteGroups>,
    Path(plan): Path<String>,
) -> Result<Response, ApiError> {
    set_in(&api, &signed, &plan, true).await
}

/// `DELETE /v1/open-plans/{plan}/in`: the asker takes that back.
pub(crate) async fn take_back<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteGroups>,
    Path(plan): Path<String>,
) -> Result<Response, ApiError> {
    set_in(&api, &signed, &plan, false).await
}
