//! Flying sites that pilots make: the licence they are given under, who
//! may write, and the numbers that bound the layer.
//!
//! The community's own sites are a layer beside the sources' ([`SOURCE`]),
//! never merged with them. A site has an identifier of the server's own
//! making that is never used again, and every change to it is a revision
//! that is kept: `venti_store` holds that, the handlers in
//! `crate::api` check what comes in, and whoever runs the server looks at
//! a site, restores a revision and takes a site down from the command line
//! ([`crate::moderation`]).
//!
//! **The licence is named here and nowhere else.** [`LICENCE`] is what
//! every answer that carries a site of the layer says in its
//! `attributions`, and what the documents state.
//!
//! **Who may write is decided here and nowhere else**, in [`may_write`].
//!
//! **A server takes sites from pilots only when it is told to**
//! ([`Settings::accept`], the configuration's `sites_from_pilots`). The
//! licence is decided; the terms for contributors, which say that a site
//! is given under it and stays when the account goes, are not written
//! yet, and until they are no pilot but the owner should give a site to a
//! public server. Reading the layer is never off.

use venti_api_types::v1::{Attribution, VENTI_SITES_SOURCE};
use venti_domain::{BoundingBox, GeoPoint, SourceId};
use venti_store::Account;

/// The source of the layer, as every record and every credit names it.
pub const SOURCE: &str = VENTI_SITES_SOURCE;

/// The licence of what pilots give as sites, with what an answer says of
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Licence {
    /// Who the records are from.
    pub provider: &'static str,
    /// The licence's name.
    pub name: &'static str,
    /// The address of its text.
    pub url: &'static str,
    /// The credit line. The licence asks for none; this says where the
    /// records are from.
    pub credit: &'static str,
    /// What is to be said wherever a site of the layer is shown.
    pub notice: &'static str,
}

/// The licence of the layer: CC0 1.0, the owner's decision of 2026-11-21
/// ("CC0, public domain"; the decision log in `docs/architecture.md`).
/// The one place that names it.
pub const LICENCE: Licence = Licence {
    provider: "Venti contributors",
    name: "CC0 1.0",
    url: "https://creativecommons.org/publicdomain/zero/1.0/",
    credit: "Venti contributors",
    notice: "Added by pilots and not verified. A site on the map is not permission to fly there.",
};

/// The credit of the layer, as an answer carries it.
pub fn attribution() -> Attribution {
    Attribution {
        source: source(),
        provider: LICENCE.provider.to_owned(),
        licence: LICENCE.name.to_owned(),
        text: LICENCE.credit.to_owned(),
        url: LICENCE.url.to_owned(),
        notice: Some(LICENCE.notice.to_owned()),
    }
}

/// The layer's source as an identifier.
///
/// # Panics
///
/// Never: the name is a constant that a test holds to the form.
pub fn source() -> SourceId {
    SourceId::new(SOURCE).unwrap_or_else(|_| unreachable!("the layer's name is a source name"))
}

/// How near two standing sites of the layer may be to each other, in
/// metres: nearer than this they are taken for the same place, and the
/// second is refused with the first one's identifier, to be corrected
/// instead. A phone's position is good to some metres; two take-offs on
/// one hill are further apart than this (an estimate, the owner's to
/// change).
pub const APART_M: f64 = 20.0;

/// Whether an account may add a site and write a revision of one.
///
/// **Any signed-in pilot may, of any site**, as long as whoever runs the
/// server has not suspended the account's publishing. Every change is
/// kept and can be undone by any other pilot. This is the rule the review
/// recommends (`docs/data-platform.md`); it is the owner's to change, and
/// this function is the one place to change it: the other rule would ask
/// here whether the account made the site.
pub fn may_write(account: &Account) -> bool {
    account.publishing_suspended_at.is_none()
}

/// The numbers that bound what one account writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Sites one account may add within a day, whatever becomes of them.
    /// A pilot who walks every take-off of a valley in a day adds fewer
    /// (an estimate).
    pub added_per_day: u32,
    /// Revisions one account may write within a day: changes, restores
    /// and removals together. It bounds what there is to undo after an
    /// account that means harm.
    pub revisions_per_day: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            added_per_day: 20,
            revisions_per_day: 100,
        }
    }
}

impl Limits {
    /// Limits for the development mode, whose clock stands still: no
    /// count ever bites.
    pub fn none() -> Self {
        Self {
            added_per_day: u32::MAX,
            revisions_per_day: u32::MAX,
        }
    }
}

/// How a server treats the sites that pilots make.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// Whether the server takes sites and revisions from pilots.
    pub accept: bool,
    /// The regions the server serves: a site is taken only inside one.
    pub regions: Vec<BoundingBox>,
    /// What bounds one account.
    pub limits: Limits,
}

impl Settings {
    /// A server that takes nothing from pilots and serves no region.
    pub fn closed() -> Self {
        Self {
            accept: false,
            regions: Vec::new(),
            limits: Limits::default(),
        }
    }

    /// Whether a place is inside a region the server serves.
    pub fn serves(&self, place: GeoPoint) -> bool {
        self.regions.iter().any(|region| region.contains(place))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layer_has_a_source_and_a_credit_that_names_the_licence() {
        assert_eq!(source().as_str(), "venti");
        let credit = attribution();
        assert_eq!(credit.licence, "CC0 1.0");
        assert!(
            credit
                .url
                .starts_with("https://creativecommons.org/publicdomain/zero/1.0")
        );
    }
}
