//! Reports: a signed-in pilot tells whoever runs the server that a flight,
//! a pilot's page or a live track should not be there.
//!
//! A report is stored and nothing else happens: no flight is hidden, no
//! account is touched, nobody is told. Whoever runs the server reads the
//! reports with `venti-server reports` and decides ([`crate::moderation`]).
//!
//! **Nobody learns from the server who reported them.** The API has no way
//! to read a report back, to list them or to count them, and the pilot who
//! is reported is told nothing. What a reporter may report is what that
//! reporter may see, and a thing they may not see gets the answer it gets
//! everywhere: reporting is no way to find out whether a private flight or
//! a quiet account exists.
//!
//! **Nothing of a report reaches the log.** What is reported is named in
//! the body, which is never logged, and not in the path: the identifier of
//! a live track is the key to it. No line is written for a report, and no
//! error message repeats a part of one.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use venti_api_types::v1::{
    ErrorCode, ReportReason, ReportRequest, ReportResponse, ReportSubject, parse_report_text,
};
use venti_store::{NewReport, ReportSaved, Store};

use super::auth::{Signed, Viewer};
use super::error::ApiError;
use super::{Api, body, cache, flights, live, pilots};
use crate::accounts::secret;
use crate::accounts::throttle::Counted;

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::bad_request(ErrorCode::InvalidReport, message)
}

/// `POST /v1/reports`: stores a report for whoever runs the server.
pub(crate) async fn create<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
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
    let reason = request.reason.name().ok_or_else(|| {
        let known: Vec<&str> = ReportReason::ALL
            .iter()
            .filter_map(|reason| reason.name())
            .collect();
        invalid(format!("reason must be one of {}", known.join(", ")))
    })?;
    let text = parse_report_text(request.text.as_deref().unwrap_or_default())
        .map_err(|error| invalid(error.to_string()))?;
    if request.reason == ReportReason::Other && text.is_none() {
        return Err(invalid(
            "a report whose reason is other says in its text what it is about",
        ));
    }

    // What is reported, as the reporter may see it. Each of the three is
    // found as the endpoint that shows it finds it, and what that endpoint
    // would not show is not there to report.
    let (about_account_id, subject, gone) = match &request.subject {
        ReportSubject::Flight(id) => {
            let viewer = Viewer(Some(signed.clone()));
            let flight = api.visible_flight(id, &viewer).await?;
            let subject = venti_store::ReportSubject::Flight(flight.id);
            (flight.account_id, subject, flights::no_such_flight())
        }
        ReportSubject::Pilot(handle) => {
            let account = api.pilot_with_a_page(handle).await?;
            let subject = venti_store::ReportSubject::Pilot;
            (account.id, subject, pilots::no_such_pilot())
        }
        ReportSubject::LiveTrack(id) => {
            let track = api.live_track(id, now).await?;
            let subject = venti_store::ReportSubject::LiveTrack(track.id);
            (track.account_id, subject, live::no_such_track())
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
