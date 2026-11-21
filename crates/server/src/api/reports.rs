//! Reports: a signed-in pilot tells whoever runs the server that a flight,
//! a pilot's page, a live track, a group, a plan of a group or a site that
//! pilots made should not be there, or not as it is.
//!
//! A report is stored and nothing else happens: no flight is hidden, no
//! account is touched, no group is changed, nobody is told. Whoever runs
//! the server reads the reports with `venti-server reports` and decides
//! ([`crate::moderation`]).
//!
//! **Nobody learns from the server who reported them.** The API has no way
//! to read a report back, to list them or to count them, and the pilot who
//! is reported is told nothing, nor is anyone in a group. What a reporter
//! may report is what that reporter may see, and a thing they may not see
//! gets the answer it gets everywhere: reporting is no way to find out
//! whether a private flight, a quiet account or a group one is not in
//! exists.
//!
//! **A group's name and a plan's words are kept with the report**, as the
//! reporter saw them at that moment: both can be changed or be gone before
//! anyone reads the report. Nothing else of a group or a plan is.
//!
//! **A report about a site names no account**: a site that pilots made is
//! nobody's and outlives whoever added it. It names the revision it is
//! about and copies nothing: every revision of a site is kept.
//!
//! **Nothing of a report reaches the log.** What is reported is named in
//! the body, which is never logged, and not in the path: the identifier of
//! a live track is the key to it, and a group is its members' alone. No
//! line is written for a report, and no error message repeats a part of
//! one.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use venti_api_types::v1::{
    ErrorCode, ReportReason, ReportRequest, ReportResponse, ReportSubject, parse_report_text,
};
use venti_store::{NewReport, ReportSaved, Store};

use super::auth::{May, Viewer, WriteReports};
use super::error::ApiError;
use super::{Api, body, cache, flights, groups, live, pilots};
use crate::accounts::secret;
use crate::accounts::throttle::Counted;

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::bad_request(ErrorCode::InvalidReport, message)
}

/// `POST /v1/reports`: stores a report for whoever runs the server.
pub(crate) async fn create<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteReports>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    // Asked before anything is counted: what a page on another site made a
    // browser send does not count against the visitor.
    body::must_say_json(&headers)?;
    let now = api.clock.now();
    // Counted before the report is read, whatever becomes of it: a report
    // is something a person has to read, and looking a subject up is no
    // way to try addresses quickly.
    let by_day = [Counted::new(&signed.account.id, api.limits.reports_per_day)];
    api.reports.attempt(&by_day, now).map_err(|refused| {
        ApiError::rate_limited(
            refused,
            "this account has sent as many reports today as one account may",
        )
    })?;

    let request: ReportRequest = body::json(&headers, &body)?;
    // The reasons a report about this kind of subject can give. Which of
    // them apply is said by the kind alone, before anything is looked up:
    // the refusal tells nothing about the subject.
    let reasons = request.subject.reasons();
    let reason = request
        .reason
        .name()
        .filter(|_| reasons.contains(&request.reason))
        .ok_or_else(|| {
            let known: Vec<&str> = reasons.iter().filter_map(|reason| reason.name()).collect();
            invalid(format!("reason must be one of {}", known.join(", ")))
        })?;
    let text = parse_report_text(request.text.as_deref().unwrap_or_default())
        .map_err(|error| invalid(error.to_string()))?;
    if request.reason == ReportReason::Other && text.is_none() {
        return Err(invalid(
            "a report whose reason is other says in its text what it is about",
        ));
    }

    // What is reported, as the reporter may see it. Each of the six is
    // found as the endpoint that shows it finds it, and what that endpoint
    // would not show is not there to report. A group and a plan are shown
    // to the members of the group and to nobody else.
    let (about_account_id, subject, gone) = match &request.subject {
        ReportSubject::Flight(id) => {
            let viewer = Viewer::allowed(&signed);
            let flight = api.visible_flight(id, &viewer).await?;
            let subject = venti_store::ReportSubject::Flight(flight.id);
            (Some(flight.account_id), subject, flights::no_such_flight())
        }
        ReportSubject::Pilot(handle) => {
            let account = api.pilot_with_a_page(handle).await?;
            let subject = venti_store::ReportSubject::Pilot;
            (Some(account.id), subject, pilots::no_such_pilot())
        }
        ReportSubject::LiveTrack(id) => {
            let track = api.live_track(id, now).await?;
            let subject = venti_store::ReportSubject::LiveTrack(track.id);
            (Some(track.account_id), subject, live::no_such_track())
        }
        ReportSubject::Group(id) => {
            // The report is about the group as its keeper keeps it, and
            // takes the name along as it stands.
            let group = api.group_as_member(id, &signed).await?.group;
            let subject = venti_store::ReportSubject::Group {
                id: group.id,
                name: group.name,
            };
            (
                Some(group.keeper.account_id),
                subject,
                groups::no_such_group(),
            )
        }
        ReportSubject::Plan(named) => {
            // Of a plan the report takes the words along, and nothing of
            // when, where on the map or with whom.
            let plan = api
                .plan_as_member(&named.group, &named.id, &signed, now)
                .await?;
            let subject = venti_store::ReportSubject::Plan {
                id: plan.id,
                group_id: plan.group_id,
                place: plan.details.place,
                note: plan.details.note,
            };
            (
                Some(plan.author.account_id),
                subject,
                groups::no_such_plan(),
            )
        }
        ReportSubject::Site(named) => {
            // A site that pilots made is nobody's: the report names no
            // account, and nothing the site says is copied, because every
            // revision of it is kept where the sites are.
            let (id, revision) = api.reported_site(&named.id, named.revision).await?;
            let subject = venti_store::ReportSubject::Site {
                id: id.to_string(),
                revision,
            };
            // Only the reporter's own account can be gone.
            let gone = ApiError::unauthorized("the account this session belonged to is gone");
            (None, subject, gone)
        }
    };

    let new = NewReport {
        id: secret::random_id()?,
        created_at: now,
        reporter_id: signed.account.id.clone(),
        about_account_id,
        subject,
        reason: reason.to_owned(),
        text,
    };
    let created = match api.store.save_report(&new).await? {
        ReportSaved::Stored => true,
        ReportSaved::AlreadyOpen => false,
        // The account that was there a moment ago is gone, and what was
        // reported with it.
        ReportSaved::NoSuchAccount => return Err(gone),
    };
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok(cache::private_json(status, &ReportResponse { created }))
}
