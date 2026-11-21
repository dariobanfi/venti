//! Web Push for the web app: the requests that subscribe this browser on
//! the account and remove it, the reader of the server's key, and the
//! words of the permission line.
//!
//! The browser's part is the shell's: it asks for the permission, calls
//! `pushManager.subscribe` with the key, and shows what its service
//! worker receives (`docs/api.md`, "Web Push"). The core does no I/O: it
//! hands the shell a [`ServerRequest`] and reads the answer.

use venti_api_types::v1::{
    PUSH_KEY_PATH, PUSH_SUBSCRIPTION_PATH, PushKeyResponse, PushKeys, PushRemoveRequest,
    PushSubscriptionRequest,
};

use crate::account::json_request;
use crate::server::answer;
use crate::{CoreError, ServerRequest};

/// The line beside the switch that asks the browser for its permission.
#[uniffi::export]
pub fn push_permission_line() -> String {
    "Let this browser tell you when an alarm fires, even while Venti is closed.".to_owned()
}

/// The request for the server's public key for Web Push. Open: it needs
/// no token.
#[uniffi::export]
pub fn push_key_request() -> ServerRequest {
    ServerRequest {
        method: "GET".to_owned(),
        path: PUSH_KEY_PATH.to_owned(),
        body: None,
        content_type: None,
        needs_token: false,
    }
}

/// Reads the answer to [`push_key_request`]: the key, base64url, for
/// `applicationServerKey`.
#[uniffi::export]
pub fn push_key(status: u16, body: Vec<u8>) -> Result<String, CoreError> {
    let response: PushKeyResponse = answer(status, &body)?;
    Ok(response.public_key)
}

/// The request that subscribes this browser on the account, from what
/// its `PushSubscription` holds: the endpoint and the two keys, base64url
/// as `toJSON()` gives them. The answer is `204`: [`push_saved`] reads it.
#[uniffi::export]
pub fn push_subscribe_request(
    endpoint: String,
    p256dh: String,
    auth: String,
) -> Result<ServerRequest, CoreError> {
    let body = PushSubscriptionRequest {
        endpoint,
        expiration_time: None,
        keys: PushKeys { p256dh, auth },
    };
    json_request("POST", PUSH_SUBSCRIPTION_PATH, &body, true)
}

/// The request that removes this browser's subscription from the
/// account, by its endpoint.
#[uniffi::export]
pub fn push_unsubscribe_request(endpoint: String) -> Result<ServerRequest, CoreError> {
    json_request(
        "DELETE",
        PUSH_SUBSCRIPTION_PATH,
        &PushRemoveRequest { endpoint },
        true,
    )
}

/// Reads the answer to [`push_subscribe_request`] or
/// [`push_unsubscribe_request`]: nothing when it was done, the server's
/// refusal in words when not.
#[uniffi::export]
pub fn push_saved(status: u16, body: Vec<u8>) -> Result<(), CoreError> {
    if (200..300).contains(&status) {
        return Ok(());
    }
    answer::<serde_json::Value>(status, &body).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_requests_carry_the_browsers_subscription() {
        let request = push_subscribe_request(
            "https://fcm.googleapis.com/fcm/send/x".to_owned(),
            "BCVx".to_owned(),
            "BTBZ".to_owned(),
        )
        .unwrap();
        assert_eq!(
            (
                request.method.as_str(),
                request.path.as_str(),
                request.needs_token
            ),
            ("POST", "/v1/account/push", true)
        );
        let body: serde_json::Value = serde_json::from_slice(&request.body.unwrap()).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "endpoint": "https://fcm.googleapis.com/fcm/send/x",
                "keys": {"p256dh": "BCVx", "auth": "BTBZ"}
            })
        );
        let remove =
            push_unsubscribe_request("https://fcm.googleapis.com/fcm/send/x".to_owned()).unwrap();
        assert_eq!(remove.method, "DELETE");
        assert!(!push_key_request().needs_token);
    }

    #[test]
    fn the_key_and_the_answers_are_read() {
        assert_eq!(
            push_key(200, br#"{"public_key":"BP4z"}"#.to_vec()).unwrap(),
            "BP4z"
        );
        assert!(push_saved(204, Vec::new()).is_ok());
        let refused = push_saved(
            409,
            br#"{"error":{"code":"too_many_push_subscriptions","message":"an account has at most 10 browsers subscribed; remove one first"}}"#
                .to_vec(),
        );
        assert!(refused.is_err());
    }
}
