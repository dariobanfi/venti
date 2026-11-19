//! The address a request really came from.
//!
//! Attempts are counted per address, so the address must be one the client
//! cannot choose. The address of the connection always is. Behind a reverse
//! proxy it is the proxy's, and the client's is what the proxy wrote into
//! `X-Forwarded-For`; but anyone can send that header, so it is believed
//! only when the connection comes from a proxy the configuration names.
//!
//! Caddy, which the deployment puts in front, ignores an `X-Forwarded-For`
//! that arrives from outside and writes its own (its `reverse_proxy`
//! documentation, "Defaults", read 2026-11-19).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use ipnet::IpNet;

/// The proxies whose `X-Forwarded-For` is believed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrustedProxies(Vec<IpNet>);

impl TrustedProxies {
    /// Reads a list of addresses and networks, such as `172.18.0.2`,
    /// `10.0.0.0/8` or `fd00::/8`.
    pub fn parse<S: AsRef<str>>(list: &[S]) -> Result<Self, String> {
        list.iter()
            .map(|entry| {
                let entry = entry.as_ref().trim();
                entry
                    .parse::<IpNet>()
                    .or_else(|_| entry.parse::<IpAddr>().map(IpNet::from))
                    .map_err(|_| {
                        format!("{entry:?} is not an address or a network such as 10.0.0.0/8")
                    })
            })
            .collect::<Result<_, _>>()
            .map(Self)
    }

    fn trusts(&self, address: IpAddr) -> bool {
        let address = plain(address);
        self.0.iter().any(|net| net.contains(&address))
    }

    /// The client's address, given the address the connection came from and
    /// the values of the request's `X-Forwarded-For` headers in order.
    ///
    /// From a trusted proxy, it is the last address in the header that is
    /// not itself a trusted proxy: each proxy appends the address it saw,
    /// so everything to the left of that could have been written by the
    /// client. From anyone else the header is not looked at.
    pub fn client<'a>(
        &self,
        peer: IpAddr,
        forwarded_for: impl IntoIterator<Item = &'a str>,
    ) -> IpAddr {
        let peer = plain(peer);
        if !self.trusts(peer) {
            return peer;
        }
        let hops: Vec<&str> = forwarded_for
            .into_iter()
            .flat_map(|value| value.split(','))
            .collect();
        for hop in hops.into_iter().rev() {
            match hop.trim().parse::<IpAddr>() {
                Ok(address) if self.trusts(address) => {}
                Ok(address) => return plain(address),
                // A proxy we trust does not write anything else here; what
                // is left of it is not to be believed either.
                Err(_) => break,
            }
        }
        peer
    }
}

/// An IPv4 address that arrived inside an IPv6 one, as IPv4.
fn plain(address: IpAddr) -> IpAddr {
    match address {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(address, IpAddr::V4),
        IpAddr::V4(_) => address,
    }
}

/// What attempts from an address are counted under.
///
/// An IPv4 address counts for itself. An IPv6 client usually has a whole
/// /64 network to pick addresses from, so the network is what is counted.
pub fn counting_key(address: IpAddr) -> String {
    match plain(address) {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => {
            let network = u128::from(v6) & !u128::from(u64::MAX);
            format!("{}/64", Ipv6Addr::from(network))
        }
    }
}

/// The address used when the connection's own is not known, which happens
/// only where no network is involved, as in tests.
pub const UNKNOWN_PEER: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    fn proxies(list: &[&str]) -> TrustedProxies {
        TrustedProxies::parse(list).unwrap()
    }

    #[test]
    fn without_a_trusted_proxy_the_header_is_never_believed() {
        let none = TrustedProxies::default();
        assert_eq!(
            none.client(ip("203.0.113.9"), ["198.51.100.1"]),
            ip("203.0.113.9")
        );
        // Also not from an address that looks internal.
        assert_eq!(
            none.client(ip("172.18.0.2"), ["198.51.100.1"]),
            ip("172.18.0.2")
        );
        // A proxy is named, but this connection is not from it.
        let caddy = proxies(&["172.18.0.2"]);
        assert_eq!(
            caddy.client(ip("203.0.113.9"), ["198.51.100.1"]),
            ip("203.0.113.9")
        );
    }

    #[test]
    fn from_a_trusted_proxy_the_client_is_what_the_proxy_saw() {
        let caddy = proxies(&["172.18.0.0/16"]);
        assert_eq!(
            caddy.client(ip("172.18.0.2"), ["198.51.100.1"]),
            ip("198.51.100.1")
        );
        // The client sent a header of its own; a proxy that does not drop
        // it appends to it. Only the last entry is the proxy's word.
        assert_eq!(
            caddy.client(ip("172.18.0.2"), ["1.2.3.4, 198.51.100.1"]),
            ip("198.51.100.1")
        );
        assert_eq!(
            caddy.client(ip("172.18.0.2"), ["1.2.3.4", "198.51.100.1"]),
            ip("198.51.100.1")
        );
        // Two proxies in a row, both trusted.
        assert_eq!(
            caddy.client(ip("172.18.0.2"), ["1.2.3.4, 198.51.100.1, 172.18.0.7"]),
            ip("198.51.100.1")
        );
        // Nothing usable in the header: the connection's address it is.
        for header in ["", "unknown", "198.51.100.1, garbage", "172.18.0.9"] {
            assert_eq!(
                caddy.client(ip("172.18.0.2"), [header]),
                ip("172.18.0.2"),
                "{header:?}"
            );
        }
        assert_eq!(caddy.client(ip("172.18.0.2"), []), ip("172.18.0.2"));
    }

    #[test]
    fn proxies_are_named_by_address_or_network() {
        let named = proxies(&["172.18.0.2", " 10.0.0.0/8 ", "fd00::/8", "::1"]);
        assert!(named.trusts(ip("172.18.0.2")));
        assert!(!named.trusts(ip("172.18.0.3")));
        assert!(named.trusts(ip("10.200.3.4")));
        assert!(named.trusts(ip("fd12::1")));
        assert!(named.trusts(ip("::1")));
        // An IPv4 address that arrives wrapped in IPv6 is the same address.
        assert!(named.trusts(ip("::ffff:172.18.0.2")));
        assert_eq!(
            named.client(ip("::ffff:172.18.0.2"), ["::ffff:198.51.100.1"]),
            ip("198.51.100.1")
        );

        assert!(TrustedProxies::parse(&["caddy"]).is_err());
        assert!(TrustedProxies::parse(&["10.0.0.0/33"]).is_err());
        assert!(TrustedProxies::parse(&[""]).is_err());
    }

    #[test]
    fn ipv6_clients_are_counted_by_their_network() {
        assert_eq!(counting_key(ip("198.51.100.1")), "198.51.100.1");
        assert_eq!(counting_key(ip("::ffff:198.51.100.1")), "198.51.100.1");
        assert_eq!(
            counting_key(ip("2001:db8:1:2:aaaa:bbbb:cccc:dddd")),
            "2001:db8:1:2::/64"
        );
        assert_eq!(
            counting_key(ip("2001:db8:1:2::1")),
            counting_key(ip("2001:db8:1:2:ffff:ffff:ffff:ffff"))
        );
        assert_ne!(
            counting_key(ip("2001:db8:1:2::1")),
            counting_key(ip("2001:db8:1:3::1"))
        );
    }
}
