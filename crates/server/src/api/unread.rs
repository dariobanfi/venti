//! How much is new for the signed-in pilot, the same on every device:
//! the lines of what the account was told ([`super::social`]) and of what
//! its alarms fired ([`super::alerts`]) after the account's mark of what
//! the pilot saw. A device reads the count for a badge and moves the mark
//! when the pilot opened the lists; the mark is kept on the server
//! (`0033_seen_marks.sql`), so another device shows the same.
//!
//! Each count reads at most [`UNREAD_COUNTED_UP_TO`] rows of an index:
//! cheap enough to ask whenever an app comes to the front.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use chrono::Duration;
use venti_api_types::v1::{
    ErrorCode, FIRED_KEPT_DAYS, NOTIFICATIONS_KEPT_DAYS, Scope, SeenRequest, UNREAD_COUNTED_UP_TO,
    UnreadResponse,
};
use venti_store::Store;

use super::auth::{May, ReadSocial, Signed, WriteSocial};
use super::error::ApiError;
use super::{Api, body, cache};

/// The count for the caller: notifications with `social:read`, fired
/// alarms with `alerts:read`; a session may both.
async fn count<S: Store>(api: &Api<S>, signed: &Signed) -> Result<UnreadResponse, ApiError> {
    let now = api.clock.now();
    let found = api
        .store
        .unread(
            &signed.account.id,
            now - Duration::days(NOTIFICATIONS_KEPT_DAYS),
            now - Duration::days(FIRED_KEPT_DAYS),
            UNREAD_COUNTED_UP_TO,
        )
        .await?;
    let notifications = if signed.may(Scope::SocialRead) {
        found.notices
    } else {
        0
    };
    let alarms = if signed.may(Scope::AlertsRead) {
        found.fired
    } else {
        0
    };
    Ok(UnreadResponse::of(notifications, alarms))
}

/// A cursor of `GET /v1/account/notifications` or of `GET
/// /v1/account/alerts/fired`, as a number.
fn cursor(text: Option<&str>, field: &str) -> Result<Option<i64>, ApiError> {
    text.map(|text| {
        text.parse()
            .ok()
            .filter(|number: &i64| *number >= 0)
            .ok_or_else(|| {
                ApiError::bad_request(
                    ErrorCode::InvalidCursor,
                    format!("{field} must be the cursor of an earlier answer"),
                )
            })
    })
    .transpose()
}

/// `GET /v1/account/unread`: how much is new since the pilot last saw it.
pub(crate) async fn show<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<ReadSocial>,
) -> Result<Response, ApiError> {
    let answer = count(&api, &signed).await?;
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `PUT /v1/account/unread`: the pilot saw the lists, up to the cursors
/// the body names, or up to the newest of each without one. The answer is
/// the count after it.
pub(crate) async fn seen<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteSocial>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    // The body may be left out.
    let request: SeenRequest = if body.is_empty() {
        SeenRequest::default()
    } else {
        body::json(&headers, &body)?
    };
    let notices = cursor(
        request.notifications_cursor.as_deref(),
        "notifications_cursor",
    )?;
    let fired = cursor(request.alarms_cursor.as_deref(), "alarms_cursor")?;
    // A token that may not read the alarms leaves their mark as it is:
    // the mark never moves back, so zero changes nothing.
    let fired = if signed.may(Scope::AlertsRead) {
        fired
    } else {
        Some(0)
    };
    api.store
        .mark_seen(&signed.account.id, notices, fired, api.clock.now())
        .await?;
    let answer = count(&api, &signed).await?;
    Ok(cache::private_json(StatusCode::OK, &answer))
}
