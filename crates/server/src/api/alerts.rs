//! The alarms a pilot sets on the account: the list, adding one, changing
//! one, removing one, and what they fired.
//!
//! The alarms are private. Every request here acts for the account of its
//! token and reads or writes that account's alarms and no other; the
//! account's deletion removes them and what they fired. The checks are
//! not here: they run where the data arrives ([`crate::alerts`]).

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::Duration;
use serde::Deserialize;
use venti_api_types::v1::{
    Alert, AlertRequest, AlertResponse, AlertRule, AlertsResponse, ErrorCode, FIRED_KEPT_DAYS,
    FiredAlertsResponse, MAX_ALERTS,
};
use venti_store::{AddingAlert, DAY_ALERT, NewAlert, PILOTS_ALERT, Store, WIND_ALERT};

use super::auth::{May, ReadAlerts, WriteAlerts};
use super::error::ApiError;
use super::{Api, body, cache};
use crate::accounts::secret::random_id;

/// A stored alarm as the API hands it out.
pub(super) fn wire(alert: &venti_store::Alert) -> Result<Alert, ApiError> {
    // The text is the server's own writing.
    let rule: AlertRule = serde_json::from_str(&alert.rule).map_err(|_| ApiError::internal())?;
    Ok(Alert {
        id: alert.id.clone(),
        name: alert.name.clone(),
        enabled: alert.enabled,
        rule,
        created_at: alert.created_at,
        changed_at: alert.changed_at,
    })
}

/// The alarm a request asks for, checked, as the store keeps it.
fn new_alert(id: String, headers: &HeaderMap, body: &Bytes) -> Result<NewAlert, ApiError> {
    let request: AlertRequest = body::json(headers, body)?;
    if let Some(fault) = request.check().first() {
        return Err(ApiError::bad_request(
            ErrorCode::InvalidAlert,
            format!("{}: {}", fault.field, fault.reason),
        ));
    }
    let (kind, station_id) = match &request.rule {
        AlertRule::Wind(wind) => (WIND_ALERT, Some(wind.station.to_string())),
        AlertRule::Day(_) => (DAY_ALERT, None),
        AlertRule::Pilots(_) => (PILOTS_ALERT, None),
        // `check` refuses it.
        AlertRule::Other => return Err(ApiError::internal()),
    };
    Ok(NewAlert {
        id,
        name: request.name.trim().to_owned(),
        enabled: request.enabled,
        kind: kind.to_owned(),
        station_id,
        rule: serde_json::to_string(&request.rule).map_err(|_| ApiError::internal())?,
    })
}

fn not_found() -> ApiError {
    ApiError::missing(ErrorCode::AlertNotFound, "the account has no such alarm")
}

/// `GET /v1/account/alerts`: the account's alarms.
pub(crate) async fn list<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<ReadAlerts>,
) -> Result<Response, ApiError> {
    let alerts = api.store.alerts(&signed.account.id).await?;
    let answer = AlertsResponse {
        alerts: alerts.iter().map(wire).collect::<Result<_, _>>()?,
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `POST /v1/account/alerts`: adds an alarm.
pub(crate) async fn add<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteAlerts>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let new = new_alert(random_id()?, &headers, &body)?;
    let added = api
        .store
        .add_alert(&signed.account.id, &new, api.clock.now(), MAX_ALERTS)
        .await?;
    let alert = match added {
        AddingAlert::Added(alert) => alert,
        AddingAlert::Full => {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                ErrorCode::TooManyAlerts,
                format!("an account has at most {MAX_ALERTS} alarms; remove one first"),
            ));
        }
        // Deleted between the token's check and this.
        AddingAlert::NoAccount => return Err(ApiError::unauthorized("the account is gone")),
    };
    let answer = AlertResponse {
        alert: wire(&alert)?,
    };
    Ok(cache::private_json(StatusCode::CREATED, &answer))
}

/// `PUT /v1/account/alerts/{id}`: changes an alarm. What the server noted
/// of it is forgotten: a wind alarm may fire at once, and a day alarm
/// fires for what the next run says.
pub(crate) async fn change<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteAlerts>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let new = new_alert(id, &headers, &body)?;
    let alert = api
        .store
        .change_alert(&signed.account.id, &new, api.clock.now())
        .await?
        .ok_or_else(not_found)?;
    let answer = AlertResponse {
        alert: wire(&alert)?,
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `DELETE /v1/account/alerts/{id}`: removes an alarm. Removing one the
/// account does not have is not an error: what was asked for holds.
pub(crate) async fn remove<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteAlerts>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    api.store.remove_alert(&signed.account.id, &id).await?;
    Ok(cache::private(StatusCode::NO_CONTENT.into_response()))
}

#[derive(Debug, Deserialize)]
pub(crate) struct FiredParams {
    since: Option<String>,
    limit: Option<String>,
}

/// `GET /v1/account/alerts/fired`: what the account's alarms fired since
/// the device last asked, of the last [`FIRED_KEPT_DAYS`] days.
pub(crate) async fn fired<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<ReadAlerts>,
    Query(params): Query<FiredParams>,
) -> Result<Response, ApiError> {
    let limit = body::page_size(params.limit.as_deref())?;
    let asked: i64 = match params.since.as_deref() {
        None => 0,
        Some(text) => text
            .parse()
            .ok()
            .filter(|since| *since >= 0)
            .ok_or_else(|| {
                ApiError::bad_request(
                    ErrorCode::InvalidCursor,
                    "since must be the cursor of an earlier answer",
                )
            })?,
    };
    let account_id = &signed.account.id;
    let mut fired = api.store.fired_alerts(account_id, asked, limit).await?;
    // A cursor from beyond the latest was not given out by this database
    // as it is now: start again, and say so.
    let reset = asked > fired.latest;
    let since = if reset { 0 } else { asked };
    if reset {
        fired = api.store.fired_alerts(account_id, 0, limit).await?;
    }
    let kept_from = api.clock.now() - Duration::days(FIRED_KEPT_DAYS);
    let cursor = fired.fired.last().map_or(since, |last| last.number);
    let answer = FiredAlertsResponse {
        fired: fired
            .fired
            .iter()
            .filter(|fired| fired.fired_at >= kept_from)
            .map(|fired| serde_json::from_str(&fired.record).map_err(|_| ApiError::internal()))
            .collect::<Result<_, _>>()?,
        cursor: cursor.to_string(),
        more: fired.more,
        reset,
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}
