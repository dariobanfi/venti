//! Whether the server sees its visitors' addresses at all.
//!
//! Attempts are counted by client address. Behind a proxy that hides the
//! addresses, every visitor arrives as the same one: thirty wrong
//! passwords by anybody then close sign-in for everybody, and the
//! connector's bound per address is all it answers. Docker does this to
//! connections its own forwarder handles (IPv6 to a network without it,
//! for example), and a proxy whose address is not the one the
//! configuration names has the same effect.
//!
//! Nothing in a single request shows it. Over many it is plain: a server
//! on the internet whose requests come from one private address is not
//! seeing who asks. This module counts where requests come from, by the
//! hour, and says so in the log and in `/v1/health`.
//!
//! A server on a home network sees private addresses and nothing else, and
//! is right to. With several of them in an hour nothing is said; with a
//! single one, which is one person using their own server, the warning is
//! a false alarm, and its words say when it is.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex, PoisonError};

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use chrono::Duration;
use venti_api_types::v1::ClientAddressesHealth;
use venti_domain::Timestamp;
use venti_store::Store;

use super::Api;
use super::auth::Client;

/// How long requests are counted before the count starts again.
const WINDOW: Duration = Duration::hours(1);

/// How many requests from one private address it takes before anything is
/// said. Whoever runs the server asks it a few things from the machine
/// itself, and those requests arrive from a private address too.
const ENOUGH: u64 = 20;

/// The share of all requests that the one private address must have, as
/// one in this many.
const SHARE: u64 = 10;

/// How many private addresses are told apart. More than one already means
/// a private network with several people on it.
const TOLD_APART: usize = 4;

/// Counts where requests come from.
#[derive(Debug, Default)]
pub(crate) struct ClientWatch {
    state: Mutex<Tally>,
}

#[derive(Debug, Default)]
struct Tally {
    /// When the hour being counted began.
    since: Option<Timestamp>,
    counted: Counts,
    /// What the hour before showed, for as long as this one has too few
    /// requests to tell.
    earlier: Option<ClientAddressesHealth>,
    /// Whether the log has been told in this hour that addresses are
    /// hidden.
    told: bool,
}

#[derive(Debug, Default)]
struct Counts {
    /// Requests from anywhere but this machine itself.
    all: u64,
    /// Of those, the ones from private addresses, by address.
    private: BTreeMap<IpAddr, u64>,
    /// Requests from private addresses beyond those told apart.
    other_private: u64,
}

impl Counts {
    /// What these requests show, if they are enough to show anything.
    fn verdict(&self) -> Option<ClientAddressesHealth> {
        if self.all < ENOUGH {
            return None;
        }
        Some(match self.the_one_private_address() {
            Some(_) => ClientAddressesHealth::Hidden,
            None => ClientAddressesHealth::Seen,
        })
    }

    /// The private address behind which visitors seem to be hidden: the
    /// only private one seen, with enough requests and a share of all
    /// requests that the machine's own keeper would not reach.
    fn the_one_private_address(&self) -> Option<(IpAddr, u64)> {
        if self.other_private > 0 || self.private.len() != 1 {
            return None;
        }
        let (address, requests) = self.private.iter().next()?;
        (*requests >= ENOUGH && requests.saturating_mul(SHARE) >= self.all)
            .then_some((*address, *requests))
    }
}

impl ClientWatch {
    /// Takes note of a request from `client` at `now`.
    pub(crate) fn saw(&self, client: IpAddr, now: Timestamp) {
        // The machine itself (the container's health check), and requests
        // whose connection is not known, say nothing about visitors.
        if client.is_loopback() || client.is_unspecified() {
            return;
        }
        let mut state = self.lock();
        state.turn(now);
        let counted = &mut state.counted;
        counted.all += 1;
        if is_private(client) {
            let room = counted.private.len() < TOLD_APART;
            match counted.private.get_mut(&client) {
                Some(requests) => *requests += 1,
                None if room => {
                    counted.private.insert(client, 1);
                }
                None => counted.other_private += 1,
            }
        }

        let hidden = counted.the_one_private_address();
        if let Some((address, requests)) = hidden.filter(|_| !state.told) {
            // One line an hour for as long as it lasts.
            state.told = true;
            tracing::warn!(
                "{requests} of the {} requests of this hour came from {address}, a private \
                 address, and none from another private one: the server does not see its \
                 visitors' addresses and counts them as one visitor. Thirty wrong passwords \
                 by anyone then close sign-in for everyone. If this server is on the \
                 internet, see \"The visitor's address\" in deploy/README.md. If it serves \
                 one person on a private network, this is as it should be",
                state.counted.all,
            );
        }
    }

    /// What `/v1/health` says: `None` while too few requests have arrived
    /// to tell.
    pub(crate) fn verdict(&self, now: Timestamp) -> Option<ClientAddressesHealth> {
        let mut state = self.lock();
        state.turn(now);
        state.counted.verdict().or(state.earlier)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Tally> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Tally {
    /// Starts a new hour of counting when the current one is over.
    fn turn(&mut self, now: Timestamp) {
        let over = self
            .since
            .is_none_or(|since| now < since || now - since >= WINDOW);
        if !over {
            return;
        }
        // An hour with too few requests to tell leaves the earlier word
        // standing.
        if let Some(verdict) = self.counted.verdict() {
            self.earlier = Some(verdict);
        }
        self.since = Some(now);
        self.counted = Counts::default();
        self.told = false;
    }
}

/// Takes note of where a request comes from, on its way to its handler.
pub(crate) async fn note<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Client(client): Client,
    request: Request,
    next: Next,
) -> Response {
    api.clients.saw(client, api.clock.now());
    next.run(request).await
}

/// Whether an address is one that never crosses the internet: a private
/// network's, a link's own, or a carrier's shared range.
fn is_private(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            v4.is_private() || v4.is_link_local() || (a == 100 && (64..128).contains(&b))
        }
        IpAddr::V6(v6) => v6.is_unique_local() || v6.is_unicast_link_local(),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use ClientAddressesHealth::{Hidden, Seen};

    fn at(minute: i64) -> Timestamp {
        Utc.with_ymd_and_hms(2026, 11, 19, 12, 0, 0).unwrap() + Duration::minutes(minute)
    }

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    /// `times` requests from `address` at `minute`.
    fn see(watch: &ClientWatch, address: &str, times: usize, minute: i64) {
        for _ in 0..times {
            watch.saw(ip(address), at(minute));
        }
    }

    /// The address visitors arrive from when Docker hides their own: the
    /// gateway of the services' network.
    const GATEWAY: &str = "172.30.250.1";

    #[test]
    fn every_request_from_one_private_address_means_visitors_are_hidden() {
        let watch = ClientWatch::default();
        // A few requests say nothing yet: the keeper's own look around.
        see(&watch, GATEWAY, 19, 0);
        assert_eq!(watch.verdict(at(0)), None);

        see(&watch, GATEWAY, 1, 1);

        assert_eq!(watch.verdict(at(1)), Some(Hidden));
    }

    #[test]
    fn visitors_with_addresses_of_their_own_are_seen() {
        let watch = ClientWatch::default();
        for n in 0..30 {
            see(&watch, &format!("198.51.100.{n}"), 1, 0);
        }
        see(&watch, "2001:db8:1:2::7", 5, 0);
        // The keeper asks a few things from the machine itself.
        see(&watch, GATEWAY, 3, 0);

        assert_eq!(watch.verdict(at(1)), Some(Seen));
    }

    #[test]
    fn a_part_of_the_visitors_hidden_is_hidden() {
        // IPv4 visitors arrive with their own addresses, IPv6 visitors all
        // from the gateway: what a published AAAA record does to a network
        // that has no IPv6.
        let watch = ClientWatch::default();
        for n in 0..200 {
            see(&watch, &format!("198.51.100.{n}"), 1, 0);
        }
        see(&watch, GATEWAY, 19, 0);
        assert_eq!(watch.verdict(at(0)), Some(Seen));
        // Twenty requests, and a tenth of everything.
        see(&watch, GATEWAY, 6, 0);

        assert_eq!(watch.verdict(at(0)), Some(Hidden));
    }

    #[test]
    fn several_private_addresses_are_a_private_network() {
        let watch = ClientWatch::default();
        see(&watch, "192.168.1.20", 40, 0);
        see(&watch, "192.168.1.21", 1, 0);
        assert_eq!(watch.verdict(at(0)), Some(Seen));
        // However many.
        for n in 30..60 {
            see(&watch, &format!("192.168.1.{n}"), 2, 0);
        }
        assert_eq!(watch.verdict(at(0)), Some(Seen));
        let state = watch.lock();
        assert_eq!(state.counted.private.len(), TOLD_APART);
    }

    #[test]
    fn the_machine_itself_and_unknown_connections_are_not_counted() {
        let watch = ClientWatch::default();
        see(&watch, "127.0.0.1", 500, 0);
        see(&watch, "::1", 500, 0);
        see(&watch, "0.0.0.0", 500, 0);
        assert_eq!(watch.verdict(at(0)), None);
    }

    #[test]
    fn the_word_follows_the_last_hour_with_enough_requests() {
        let watch = ClientWatch::default();
        see(&watch, GATEWAY, 50, 0);
        assert_eq!(watch.verdict(at(59)), Some(Hidden));
        // A new hour with nothing in it yet: the word stands.
        assert_eq!(watch.verdict(at(60)), Some(Hidden));
        see(&watch, "198.51.100.7", 5, 61);
        assert_eq!(watch.verdict(at(61)), Some(Hidden));
        // The setup was repaired: visitors arrive as themselves.
        for n in 0..25 {
            see(&watch, &format!("203.0.113.{n}"), 1, 62);
        }
        assert_eq!(watch.verdict(at(62)), Some(Seen));
        // A quiet night later it is still the last word.
        assert_eq!(watch.verdict(at(900)), Some(Seen));
    }

    #[test]
    fn private_addresses_are_told_from_public_ones() {
        for private in [
            "10.1.2.3",
            "172.16.0.1",
            "172.30.250.1",
            "192.168.0.9",
            "169.254.1.1",
            "100.64.0.1",
            "100.127.255.254",
            "fd00::1",
            "fe80::1",
        ] {
            assert!(is_private(ip(private)), "{private}");
        }
        for public in [
            "198.51.100.7",
            "172.32.0.1",
            "100.128.0.1",
            "8.8.8.8",
            "2001:db8::1",
            "2a01:4f8::1",
        ] {
            assert!(!is_private(ip(public)), "{public}");
        }
    }
}
