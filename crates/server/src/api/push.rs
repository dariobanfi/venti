//! A browser's subscription to Web Push on the account, and the server's
//! public key for it. What is sent, and when, is [`crate::push`].

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use venti_api_types::v1::{
    ErrorCode, MAX_PUSH_SUBSCRIPTIONS, PushKeyResponse, PushRemoveRequest, PushSubscriptionRequest,
};
use venti_store::{AddingPush, PushSubscription, Store};

use super::auth::{May, WriteAlerts};
use super::error::ApiError;
use super::{Api, body, cache};
use crate::push::{crypto, push_service};

fn invalid(field: &str, reason: &str) -> ApiError {
    ApiError::bad_request(
        ErrorCode::InvalidPushSubscription,
        format!("{field} {reason}"),
    )
}

/// `GET /v1/push/key`: the server's public key for Web Push.
pub(crate) async fn key<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let public_key = api.push_key.clone().ok_or_else(ApiError::internal)?;
    Ok(cache::slow_json(&headers, &PushKeyResponse { public_key }))
}

/// `POST /v1/account/push`: subscribes a browser. The same browser again
/// takes its new keys; a browser another account subscribed is this
/// account's from now on.
pub(crate) async fn subscribe<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteAlerts>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: PushSubscriptionRequest = body::json(&headers, &body)?;
    push_service(&request.endpoint).map_err(|why| invalid("endpoint", why))?;
    let p256dh = crypto::from_base64url(&request.keys.p256dh)
        .filter(|key| crypto::browser_key(key).is_ok())
        .ok_or_else(|| invalid("keys.p256dh", "is not a P-256 public key in base64url"))?;
    let auth = crypto::from_base64url(&request.keys.auth)
        .filter(|auth| auth.len() == 16)
        .ok_or_else(|| invalid("keys.auth", "is not 16 bytes in base64url"))?;
    let base64url = |bytes: &[u8]| {
        use base64::Engine as _;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    };
    let subscription = PushSubscription {
        account_id: signed.account.id.clone(),
        endpoint: request.endpoint,
        p256dh: base64url(&p256dh),
        auth: base64url(&auth),
        created_at: api.clock.now(),
    };
    match api
        .store
        .add_push_subscription(&subscription, MAX_PUSH_SUBSCRIPTIONS)
        .await?
    {
        AddingPush::Added => Ok(cache::private(StatusCode::NO_CONTENT.into_response())),
        AddingPush::Full => Err(ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::TooManyPushSubscriptions,
            format!(
                "an account has at most {MAX_PUSH_SUBSCRIPTIONS} browsers subscribed; \
                 remove one first"
            ),
        )),
        // Deleted between the token's check and this.
        AddingPush::NoAccount => Err(ApiError::unauthorized("the account is gone")),
    }
}

/// `DELETE /v1/account/push`: removes a browser's subscription. Removing
/// one the account does not have is not an error.
pub(crate) async fn unsubscribe<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteAlerts>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: PushRemoveRequest = body::json(&headers, &body)?;
    api.store
        .remove_push_subscription(&signed.account.id, &request.endpoint)
        .await?;
    Ok(cache::private(StatusCode::NO_CONTENT.into_response()))
}
