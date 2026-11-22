//! Flying sites that Venti itself holds: the two layers, the licence
//! their sites are given under, who may write to which, and the numbers
//! that bound them.
//!
//! **Two layers, never merged with each other or with a source's.**
//! [`Layer::Partners`] (`venti`) holds the sites that Venti's partners
//! add: accounts whoever runs the server named as trusted with that
//! (`venti-server grant-sites`). It is official in manner and is in every
//! answer beside the sources' layers. [`Layer::Pilots`] (`venti-pilots`)
//! is the crowdsourced one: any signed-in pilot adds to it, it is less
//! official, and an answer holds it only when it is asked for. Which
//! layer a site is of follows from who added it and never changes: it is
//! the beginning of its identifier.
//!
//! A site of either layer has an identifier of the server's own making
//! that is never used again, and every change to it is a revision that is
//! kept: `venti_store` holds that, the handlers in `crate::api` check
//! what comes in, and whoever runs the server looks at a site, restores a
//! revision and takes a site down from the command line
//! ([`crate::moderation`]).
//!
//! **The licence is named here and nowhere else.** [`Layer::licence`] is
//! what every answer that carries a site of a layer says in its
//! `attributions`, and what the documents state. Both layers are given
//! under CC0 1.0.
//!
//! **Who may write is decided here and nowhere else**, in [`may_write`]
//! and [`may_confirm`].
//!
//! **A server takes sites and confirmations from pilots only when it is
//! told to** ([`Settings::accept`], the configuration's
//! `sites_from_pilots`). The licence is decided; the terms for
//! contributors, which say that a site is given under it and stays when
//! the account goes, are not written yet, and until they are no pilot but
//! the owner should give a site to a public server. The partners' layer
//! needs no setting: the trust is its gate. Reading either layer is never
//! off.
//!
//! The names `pilot_sites` and `PilotSite`, here and in the store, are
//! from before there were two layers: they stand for a site of either.

use venti_api_types::v1::{
    Attribution, VENTI_PILOTS_SITES_SOURCE, VENTI_SITES_LICENCE, VENTI_SITES_LICENCE_URL,
    VENTI_SITES_SOURCE,
};
use venti_domain::{BoundingBox, GeoPoint, SiteId, SourceId};
use venti_store::Account;

/// The source of the partners' layer, as every record and every credit
/// names it.
pub const SOURCE: &str = VENTI_SITES_SOURCE;

/// The source of the crowdsourced layer.
pub const PILOTS_SOURCE: &str = VENTI_PILOTS_SITES_SOURCE;

/// One of the two layers of sites that Venti itself holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// The sites that Venti's partners add: `venti`.
    Partners,
    /// The sites that any pilot adds: `venti-pilots`.
    Pilots,
}

/// The licence of a layer's sites, with what an answer says of it.
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

impl Layer {
    /// Both, in the order of their names.
    pub const ALL: [Self; 2] = [Self::Partners, Self::Pilots];

    /// The layer's name: the source of its sites.
    pub fn name(self) -> &'static str {
        match self {
            Self::Partners => SOURCE,
            Self::Pilots => PILOTS_SOURCE,
        }
    }

    /// The layer with this name.
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|layer| layer.name() == name)
    }

    /// The layer a site's identifier says it is of. `None` for a site of
    /// a source's set.
    pub fn of(id: &SiteId) -> Option<Self> {
        Self::named(id.source().as_str())
    }

    /// The layer's source as an identifier.
    ///
    /// # Panics
    ///
    /// Never: the names are constants that a test holds to the form.
    pub fn source(self) -> SourceId {
        SourceId::new(self.name())
            .unwrap_or_else(|_| unreachable!("the layer's name is a source name"))
    }

    /// Whether an answer holds the layer when the request names none.
    /// The partners' is; the crowdsourced one is asked for.
    pub fn by_default(self) -> bool {
        self == Self::Partners
    }

    /// The licence of the layer: CC0 1.0 for both, the owner's decision
    /// of 2026-11-21 ("CC0, public domain"; the decision log in
    /// `docs/architecture.md`). Its name and address are constants of the
    /// wire types, where the apps' core reads them too; nothing else
    /// names the licence. The notices differ: they say whose word a site
    /// is.
    pub fn licence(self) -> Licence {
        match self {
            Self::Partners => Licence {
                provider: "Venti partners",
                name: VENTI_SITES_LICENCE,
                url: VENTI_SITES_LICENCE_URL,
                credit: "Venti partners",
                notice: "Added by a partner of Venti and not checked by Venti. A site on the \
                         map is not permission to fly there.",
            },
            Self::Pilots => Licence {
                provider: "Venti contributors",
                name: VENTI_SITES_LICENCE,
                url: VENTI_SITES_LICENCE_URL,
                credit: "Venti contributors",
                notice: "Added by pilots: not official and not checked by anyone. A site on \
                         the map is not permission to fly there.",
            },
        }
    }

    /// The credit of the layer, as an answer carries it.
    pub fn attribution(self) -> Attribution {
        let licence = self.licence();
        Attribution {
            source: self.source(),
            provider: licence.provider.to_owned(),
            short_name: None,
            licence: licence.name.to_owned(),
            text: licence.credit.to_owned(),
            url: licence.url.to_owned(),
            notice: Some(licence.notice.to_owned()),
        }
    }
}

/// How near two standing sites may be to each other, in metres: nearer
/// than this they are taken for the same place, and the second is refused
/// with the first one's identifier, to be corrected instead. A phone's
/// position is good to some metres; two take-offs on one hill are further
/// apart than this (an estimate, the owner's to change).
///
/// A pilot's site keeps this distance from every site of both layers; a
/// partner's keeps it from partners' sites only (the store's rule:
/// `venti_store::PARTNERS_LAYER`), so that what pilots put on the map
/// keeps no official site off it.
pub const APART_M: f64 = 20.0;

/// Why an account may not write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Whoever runs the server has suspended the account's publishing.
    Suspended,
    /// The site is of the partners' layer and the account is no partner.
    ForPartners,
    /// The server takes nothing from pilots.
    Closed,
}

/// Whether an account is one whoever runs the server named as trusted to
/// add and correct sites: a partner.
pub fn is_partner(account: &Account) -> bool {
    account.edits_sites_since.is_some()
}

/// The layer a site that this account adds goes to: a partner's to the
/// partners' layer, every other pilot's to the crowdsourced one.
pub fn layer_for(account: &Account) -> Layer {
    if is_partner(account) {
        Layer::Partners
    } else {
        Layer::Pilots
    }
}

/// Whether an account may add a site to a layer and write a revision of
/// one of its sites. `accepts` is whether this server takes sites from
/// pilots ([`Settings::accept`]).
///
/// - Nobody whose publishing is suspended writes anything.
/// - **A partner** writes to both layers, whatever the setting says: the
///   trust is the gate. The owner's rule of 2026-11-21: take-offs "usually
///   are … really official", and the server takes them from "trusted
///   partners".
/// - **Any other signed-in pilot** writes to the crowdsourced layer where
///   the server takes sites from pilots, and never to the partners': a
///   pilot who finds a partner's site wrong reports it.
///
/// This function is the one place to change the rule.
pub fn may_write(account: &Account, layer: Layer, accepts: bool) -> Result<(), Refused> {
    if account.publishing_suspended_at.is_some() {
        return Err(Refused::Suspended);
    }
    if is_partner(account) {
        return Ok(());
    }
    match layer {
        Layer::Partners => Err(Refused::ForPartners),
        Layer::Pilots if accepts => Ok(()),
        Layer::Pilots => Err(Refused::Closed),
    }
}

/// Whether an account may say of a site of either layer "I took off here
/// and it was good", and take it back: any signed-in pilot whose
/// publishing is not suspended, on a server that takes sites from pilots.
/// A partner's word counts as any pilot's here and needs the setting too.
pub fn may_confirm(account: &Account, accepts: bool) -> Result<(), Refused> {
    if account.publishing_suspended_at.is_some() {
        Err(Refused::Suspended)
    } else if accepts {
        Ok(())
    } else {
        Err(Refused::Closed)
    }
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
    /// Confirmations one account may give or take back within a day (an
    /// estimate).
    pub confirmations_per_day: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            added_per_day: 20,
            revisions_per_day: 100,
            confirmations_per_day: 50,
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
            confirmations_per_day: u32::MAX,
        }
    }
}

/// How a server treats the sites it holds itself.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// Whether the server takes sites for the crowdsourced layer, and
    /// confirmations, from pilots.
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

    fn account(partner: bool, suspended: bool) -> Account {
        let at = crate::adapters::recorded_at();
        Account {
            id: "0000000000000000000000dario".to_owned(),
            handle: "dario".to_owned(),
            display_name: "Dario".to_owned(),
            created_at: at,
            publishing_suspended_at: suspended.then_some(at),
            edits_sites_since: partner.then_some(at),
            new_flights_public: false,
        }
    }

    #[test]
    fn each_layer_has_a_source_and_a_credit_that_names_the_licence() {
        assert_eq!(Layer::Partners.source().as_str(), "venti");
        assert_eq!(Layer::Pilots.source().as_str(), "venti-pilots");
        // The store's rule of which site gives way to which names the
        // partners' layer by the same word.
        assert_eq!(venti_store::PARTNERS_LAYER, SOURCE);
        for layer in Layer::ALL {
            let credit = layer.attribution();
            assert_eq!(credit.licence, "CC0 1.0");
            assert!(
                credit
                    .url
                    .starts_with("https://creativecommons.org/publicdomain/zero/1.0")
            );
            assert!(
                credit
                    .notice
                    .unwrap()
                    .ends_with("A site on the map is not permission to fly there.")
            );
            assert_eq!(Layer::named(layer.name()), Some(layer));
        }
        assert_eq!(Layer::named("openstreetmap"), None);
        assert!(Layer::Partners.by_default() && !Layer::Pilots.by_default());
    }

    #[test]
    fn a_partner_writes_both_layers_and_a_pilot_the_crowdsourced_one() {
        let partner = account(true, false);
        let pilot = account(false, false);
        assert_eq!(layer_for(&partner), Layer::Partners);
        assert_eq!(layer_for(&pilot), Layer::Pilots);
        for accepts in [true, false] {
            // The trust is the gate: the setting does not close it.
            assert_eq!(may_write(&partner, Layer::Partners, accepts), Ok(()));
            assert_eq!(may_write(&partner, Layer::Pilots, accepts), Ok(()));
            assert_eq!(
                may_write(&pilot, Layer::Partners, accepts),
                Err(Refused::ForPartners)
            );
        }
        assert_eq!(may_write(&pilot, Layer::Pilots, true), Ok(()));
        assert_eq!(
            may_write(&pilot, Layer::Pilots, false),
            Err(Refused::Closed)
        );
        // A suspended account writes nothing, a partner's neither.
        for layer in Layer::ALL {
            for partner in [true, false] {
                assert_eq!(
                    may_write(&account(partner, true), layer, true),
                    Err(Refused::Suspended)
                );
            }
        }
    }

    #[test]
    fn any_pilot_confirms_where_the_server_takes_it() {
        for partner in [true, false] {
            assert_eq!(may_confirm(&account(partner, false), true), Ok(()));
            assert_eq!(
                may_confirm(&account(partner, false), false),
                Err(Refused::Closed)
            );
            assert_eq!(
                may_confirm(&account(partner, true), true),
                Err(Refused::Suspended)
            );
        }
    }
}
