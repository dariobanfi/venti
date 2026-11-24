//! What a client may ask in an hour, and the headers that say what is left.
//!
//! **A token for a program has a budget of its own**: so many requests in a
//! window, of any kind, whatever their answers ([`Limits::token_requests_per_window`]).
//! No other token of the account shares it, and no session: a pilot's
//! logger that floods does not stop the same pilot's other program, and a
//! club's app with many pilots' tokens behind one address is not counted by
//! the address. The token is checked here, before the request reaches its
//! handler, and counted once it is known to be good: a token that is not
//! good is answered `401` whatever it asked for, so that sending made-up
//! tokens buys no way around the bound of an address. The handler then
//! finds the checked token ([`Checked`]) and does not ask the store again.
//!
//! **A read without a token is bounded by its address** where it is
//! bounded at all, as before: the public flights as data
//! ([`Api::count_public_read`]). A read of them with a token for a program
//! is counted against the token's budget only, and one with a good session
//! of Venti's own apps against the signed-in account's
//! ([`Limits::public_flight_reads_per_account`]): everyone behind one
//! address shares the address's budget, a pilot does not.
//!
//! **Every answer that a budget was spent for says what is left of it**,
//! in the two fields of the IETF's draft "RateLimit header fields for
//! HTTP" (draft-ietf-httpapi-ratelimit-headers-12, read on ietf.org on
//! 2026-11-21): `RateLimit-Policy: "token";q=3600;w=3600`, the quota and
//! its window in seconds, and `RateLimit: "token";a=3599;w=3540`, what is
//! available and in how many seconds the window closes. A spent budget is
//! `429` `rate_limited` with `Retry-After`, which names the same moment as
//! `w`. The draft is not a standard yet and has renamed its fields more
//! than once: older drafts had `RateLimit-Limit`, `-Remaining` and
//! `-Reset`. A session of Venti's own apps spends no such budget and its
//! answers carry neither field.
//!
//! Like every count of the server, these live in this process
//! ([`crate::accounts::throttle`]).

use std::net::IpAddr;
use std::sync::{Arc, Mutex, PoisonError};

use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use chrono::Duration;
use venti_store::Store;

use super::Api;
use super::auth::{By, Checked, Client, program, session_account};
use super::error::ApiError;
use crate::accounts::Limits;
use crate::accounts::client::counting_keys;
use crate::accounts::throttle::{Counted, Left};

/// The field that names a budget, its quota and its window.
pub(crate) const RATELIMIT_POLICY: HeaderName = HeaderName::from_static("ratelimit-policy");

/// The field that says what is left of it.
pub(crate) const RATELIMIT: HeaderName = HeaderName::from_static("ratelimit");

/// The name of a token's budget in the two fields.
const TOKEN_POLICY: &str = "token";

/// The name of an address's budget for reading public flights.
const ADDRESS_POLICY: &str = "address";

/// The name of a signed-in account's budget for reading them.
const ACCOUNT_POLICY: &str = "account";

/// A budget a request spent from, and what is left of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Budget {
    policy: &'static str,
    window: Duration,
    left: Left,
}

/// Whole seconds, rounded up: a client that waits as long finds the window
/// closed.
fn seconds(duration: Duration) -> i64 {
    let millis = duration.num_milliseconds().max(0);
    (millis + 999) / 1000
}

impl Budget {
    fn write(&self, response: &mut Response) {
        let policy = format!(
            "\"{}\";q={};w={}",
            self.policy,
            self.left.limit,
            seconds(self.window).max(1)
        );
        let left = format!(
            "\"{}\";a={};w={}",
            self.policy,
            self.left.remaining,
            seconds(self.left.reset)
        );
        let headers = response.headers_mut();
        for (name, value) in [(RATELIMIT_POLICY, policy), (RATELIMIT, left)] {
            if let Ok(value) = HeaderValue::from_str(&value) {
                headers.insert(name, value);
            }
        }
    }
}

/// Where a request notes the budget it spent from, for the answer to say.
/// It is in every request's extensions that passes [`count`].
#[derive(Debug, Clone, Default)]
pub(crate) struct Said(Arc<Mutex<Option<Budget>>>);

impl Said {
    fn note(&self, budget: Budget) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(budget);
    }

    fn write(&self, response: &mut Response) {
        let noted = *self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(budget) = noted {
            budget.write(response);
        }
    }
}

/// Checks a request's token for a program and counts the request against
/// its budget; then says in the answer what is left of whatever budget the
/// request spent from.
pub(crate) async fn count<S: Store>(
    State(api): State<Arc<Api<S>>>,
    mut request: Request,
    next: Next,
) -> Response {
    let said = Said::default();
    request.extensions_mut().insert(said.clone());
    let signed = match program(request.headers(), &api).await {
        Ok(signed) => signed,
        Err(error) => return error.into_response(),
    };
    if let Some(signed) = signed {
        if let By::Token(token) = &signed.by {
            let limits: &Limits = &api.limits;
            let by_token = [Counted::new(&token.id, limits.token_requests_per_window)];
            let (outcome, left) = api.token_requests.attempt_left(&by_token, api.clock.now());
            said.note(Budget {
                policy: TOKEN_POLICY,
                window: limits.token_window,
                left,
            });
            if let Err(refused) = outcome {
                let mut response = ApiError::rate_limited(
                    refused,
                    "this token has made as many requests as one token may in an hour",
                )
                .into_response();
                said.write(&mut response);
                return response;
            }
        }
        request.extensions_mut().insert(Checked(signed));
    }
    let mut response = next.run(request).await;
    said.write(&mut response);
    response
}

/// Who reads what is bounded by the address without a token: the address,
/// whether the request came with a token for a program, whose own budget
/// it spent instead, and the account a good session signs in, whose
/// budget it spends.
#[derive(Debug, Clone)]
pub(crate) struct Reader {
    client: IpAddr,
    said: Option<Said>,
    by_token: bool,
    account: Option<String>,
}

impl Reader {
    /// A reader at `client` that came without a token, and whose answer
    /// says nothing of its budget: the connector's.
    pub(crate) fn at(client: IpAddr) -> Self {
        Self {
            client,
            said: None,
            by_token: false,
            account: None,
        }
    }
}

impl<S: Store> FromRequestParts<Arc<Api<S>>> for Reader {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        api: &Arc<Api<S>>,
    ) -> Result<Self, Self::Rejection> {
        let Client(client) = Client::from_request_parts(parts, api).await?;
        let by_token = parts.extensions.get::<Checked>().is_some();
        let account = if by_token {
            None
        } else {
            session_account(&parts.headers, api).await
        };
        Ok(Self {
            client,
            said: parts.extensions.get::<Said>().cloned(),
            by_token,
            account,
        })
    }
}

impl<S: Store> Api<S> {
    /// Counts a read of public flights as data: a page of everyone's
    /// public flights, a page of what changed among them, a flight's
    /// track, what flights say about a site, the candidates for sites.
    ///
    /// These are open to anyone, and without a bound one address could
    /// take every public flight as fast as the database hands them out.
    /// So a read without a token is counted for the address that asks, by
    /// address and by network for IPv6, as the forecasts an address makes
    /// the server fetch are, and the answer says what the address has
    /// left. A read with a token for a program has spent from the token's
    /// own budget already ([`count`]) and is not counted again. A read
    /// with a good session is counted for the signed-in account, and the
    /// answer says what the account has left.
    pub(crate) fn count_public_read(&self, reader: &Reader) -> Result<(), ApiError> {
        if reader.by_token {
            return Ok(());
        }
        if let Some(account) = &reader.account {
            let by_account = [Counted::new(
                account,
                self.limits.public_flight_reads_per_account,
            )];
            let (outcome, left) = self
                .account_public_reads
                .attempt_left(&by_account, self.clock.now());
            if let Some(said) = &reader.said {
                said.note(Budget {
                    policy: ACCOUNT_POLICY,
                    window: self.limits.public_flight_window,
                    left,
                });
            }
            return outcome.map_err(|refused| {
                ApiError::rate_limited(
                    refused,
                    "this account has read public flights as often as one account may in an hour",
                )
            });
        }
        let address = counting_keys(reader.client);
        let by_address = address.counted(
            self.limits.public_flight_reads_per_address,
            self.limits.public_flight_reads_per_network,
        );
        let (outcome, left) = self
            .public_flight_reads
            .attempt_left(&by_address, self.clock.now());
        if let Some(said) = &reader.said {
            said.note(Budget {
                policy: ADDRESS_POLICY,
                window: self.limits.public_flight_window,
                left,
            });
        }
        outcome.map_err(|refused| {
            ApiError::rate_limited(
                refused,
                "this address has read public flights as often as one address may in an hour",
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;

    use super::*;

    #[test]
    fn the_fields_say_the_quota_the_window_and_what_is_left() {
        let budget = Budget {
            policy: TOKEN_POLICY,
            window: Duration::hours(1),
            left: Left {
                limit: 3600,
                remaining: 3599,
                reset: Duration::milliseconds(3_539_001),
            },
        };
        let mut response = Response::new(Body::empty());
        budget.write(&mut response);
        assert_eq!(
            response.headers()[RATELIMIT_POLICY],
            "\"token\";q=3600;w=3600"
        );
        assert_eq!(response.headers()[RATELIMIT], "\"token\";a=3599;w=3540");
    }
}
