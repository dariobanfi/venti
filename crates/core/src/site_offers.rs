//! What a shell may offer for a flying site, the confirmations ("I took
//! off here and it was good"), and the words of a site's page.
//!
//! Venti holds two tiers of sites of its own: the partners' (`venti`),
//! written by accounts the owner named, and the crowdsourced ones
//! (`venti-pilots`), written by any signed-in pilot where the server
//! takes them. Who may do what with a site of which tier is the server's
//! rule (`docs/api.md`, "Sites that Venti's partners and pilots add"),
//! and it is stated **once** for the three apps, here, in [`offers`] and
//! [`adding`]:
//!
//! - A site of a source the server collects is its source's: nothing is
//!   written, confirmed or reported here, and it has no history.
//! - Anyone reads a site's history and its confirmations.
//! - Signed in, with a session that stands: a pilot reports a site of
//!   either tier.
//! - An account whose publishing is suspended writes and confirms
//!   nothing.
//! - A partner corrects and removes sites of both tiers, and adds to the
//!   partners', whatever the server's setting says.
//! - Every other pilot corrects and removes crowdsourced sites and adds
//!   to that tier, where the server takes sites from pilots, and never
//!   writes to a partner's site.
//! - Any pilot, a partner too, confirms a site of either tier where the
//!   server takes sites from pilots. The core offers it for a site that
//!   is a place to take off from (a take-off, a winch site or a training
//!   slope) and not for a landing: the sentence is "I took off here".
//!
//! - A site that is removed offers its history, and to bring it back to
//!   whoever may correct a site of its tier. A site that whoever runs
//!   Venti took down is brought back by nobody.
//! - What a history offers follows the same rule: whoever may correct the
//!   site puts a revision back, undoes the last change and brings a
//!   removed site back; signed in, a pilot reports a revision.
//!
//! A shell asks [`Core::site_offers`], [`Core::site_adding`],
//! [`site_correcting`] and [`Core::site_history_offers`] and shows
//! what is offered. The server still decides: what is offered here can
//! be refused there, and the readers say so in words.

use chrono::{Days, NaiveDate};
use venti_api_types::v1::{
    self as api, CONFIRMATION_DAYS_BACK, CONFIRMATION_FLIGHT_WITHIN_M, ConfirmSiteRequest,
    MAX_SITE_COMMENT_LENGTH, SiteChangesResponse, SiteConfirmationsResponse, SiteResponse,
    VENTI_PILOTS_SITES_SOURCE, VENTI_SITES_SOURCE, parse_site_comment,
};
use venti_domain::GeoPoint;

use crate::account::json_request;
use crate::server::code;
use crate::site_edit::{
    day_in_words, invalid, is_own_layer, made_here, quoted, read, site_form, site_refusal,
};
use crate::sites::{
    FROM_PILOTS_LABEL, author, author_in_words, confirmed_in_words, site_view, takes_off_here,
};
use crate::{Core, CoreError, ServerRequest, SiteAuthor, SiteHistory, SiteView, SiteWritten};

// --- Who asks -----------------------------------------------------------------

/// Who would write a site from this device, as the stored account says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Actor {
    /// Nobody is signed in.
    SignedOut,
    /// An account is known, and its session has ended.
    SessionEnded,
    /// A signed-in pilot.
    Pilot {
        /// The pilot's handle.
        handle: String,
        /// Whether the server says the account is a partner of Venti for
        /// flying sites.
        partner: bool,
        /// Whether the server says its publishing is suspended.
        suspended: bool,
    },
}

// --- What a shell may offer ---------------------------------------------------

/// One thing a shell may offer for a site.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteOffer {
    /// Whether the shell shows it.
    pub is_offered: bool,
    /// The words of its button or entry: "Correct this site". Empty when
    /// it is not offered.
    pub words: String,
}

/// Everything a shell may offer for one site to whoever is signed in on
/// this device: the one answer of [`Core::site_offers`]. A shell shows
/// what `is_offered` and combines nothing itself.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteOffers {
    /// To correct the site: "Correct this site".
    pub correct: SiteOffer,
    /// To remove it: "Remove this site".
    pub remove: SiteOffer,
    /// To say "I took off here and it was good"; for a pilot who has
    /// said so, to say it of another day: "Change the day".
    pub confirm: SiteOffer,
    /// To take the pilot's own confirmation back: "Take my confirmation
    /// back". Offered only when the confirmations handed in hold one of
    /// this pilot's.
    pub withdraw: SiteOffer,
    /// To report the site: "Report this site".
    pub report: SiteOffer,
    /// To read its history: "History".
    pub history: SiteOffer,
    /// Whether this pilot has confirmed the site, by the confirmations
    /// handed in. `false` when none were handed in, and when the list
    /// holds the newest hundred and this pilot's is older.
    pub has_confirmed: bool,
    /// The same in words: "You confirmed this site for Mon 16 Nov 2026."
    /// Empty when `has_confirmed` is not.
    pub confirmed_words: String,
    /// Why something a pilot might look for is not offered, in a line to
    /// show on the site's page: "Sign in to correct this site, to say
    /// that you took off here and to report it." Empty when nothing is
    /// held back that words would help with.
    pub note: String,
    /// For a removed site, in a view from its history
    /// ([`crate::SiteByPilots::is_removed`]): the words of the entry that
    /// brings it back, "Bring this site back", when this pilot may. The
    /// revision to bring back is the history's
    /// ([`SiteHistory::bring_back_revision`]). Empty for a site that
    /// stands, and when this pilot may not.
    #[uniffi(default = "")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub bring_back_words: String,
}

/// What a shell may offer in a site's history to whoever is signed in on
/// this device: the one answer of [`Core::site_history_offers`]. Which
/// revision each is about is the history's to say
/// ([`crate::SiteRevisionView::can_be_put_back`],
/// [`SiteHistory::undo_revision`], [`SiteHistory::bring_back_revision`]):
/// a shell shows an entry where the history has one and this says it is
/// offered.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteHistoryOffers {
    /// To put the site back to a revision: "Put back". On a row a shell
    /// shows the row's own words
    /// ([`crate::SiteRevisionView::put_back_words`]) where the revision
    /// can be put back.
    pub put_back: SiteOffer,
    /// To undo the last change: "Undo the last change". Not offered when
    /// the history names no revision for it.
    pub undo: SiteOffer,
    /// To bring a removed site back: "Bring this site back". Not offered
    /// for a site that stands.
    pub bring_back: SiteOffer,
    /// To report one revision: "Report this revision".
    pub report_revision: SiteOffer,
    /// Why something is not offered, in a line to show over the history:
    /// "Sign in to put a revision back or to report one." Empty when
    /// nothing is held back that words would help with.
    pub note: String,
}

/// What the form says when it corrects a given site: the one answer of
/// [`site_correcting`]. The words follow the site's tier, whoever
/// corrects it: a partner who corrects a place from pilots gives to that
/// tier.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteCorrecting {
    /// Whether the site is of the partners' tier.
    pub as_partner: bool,
    /// The form's title: "Correct this site".
    pub title: String,
    /// What giving means, in the tier's own sentence:
    /// [`crate::SiteForm`]'s `giving` for a place from pilots,
    /// `giving_as_partner` for a partner's site.
    pub giving: String,
    /// What is said wherever a site of the tier is shown, and on its
    /// form: the site's own [`crate::SiteByPilots::note`].
    pub note: String,
    /// The button that sends it: "Save for everyone".
    pub send_words: String,
}

/// Whether a shell may offer to add a site, and what the entry and its
/// form say: the one answer of [`Core::site_adding`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteAdding {
    /// Whether this pilot may add a site now. The entry is shown
    /// whenever `entry_words` is not empty: that is so when `may_add`,
    /// and for someone who has to sign in first (`leads_to_sign_in`).
    pub may_add: bool,
    /// Whether the site goes to the partners' tier: the account is a
    /// partner of Venti. `false` for the crowdsourced tier, and when
    /// nothing may be added.
    pub as_partner: bool,
    /// The layer the server will put the site in: `venti` or
    /// `venti-pilots`. Empty when nothing may be added. The request does
    /// not choose: the server decides by the account.
    pub layer: String,
    /// The entry's words: "Add a place pilots take off from" for the
    /// crowdsourced tier, "Add a site" for a partner, and for someone who
    /// is not signed in on a server that takes places from pilots "Sign
    /// in to add a place pilots take off from." (`leads_to_sign_in`).
    /// Empty when there is no entry to show: the server takes none, or
    /// the account may not publish.
    pub entry_words: String,
    /// The form's title: "A place pilots take off from", "Add a site".
    pub title: String,
    /// What giving means, in the tier's own sentence: [`crate::SiteForm`]'s
    /// `giving` or `giving_as_partner`.
    pub giving: String,
    /// What is said wherever a site of the tier is shown, and on its
    /// form.
    pub note: String,
    /// The button that sends it: "Add as a place from pilots", "Add for
    /// everyone".
    pub send_words: String,
    /// Why nothing may be added, in a line: "Sign in to add a place
    /// pilots take off from." Empty when `may_add`. Where the entry leads
    /// to signing in it is the entry's own sentence: a shell shows the
    /// entry and not this line beside it.
    pub why_not: String,
    /// Whether the entry leads to signing in and not to the form: nobody
    /// is signed in, or the session has ended, on a server that takes
    /// places from pilots. `may_add` is `false` then, and the form's
    /// words are empty.
    #[uniffi(default = false)]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub leads_to_sign_in: bool,
}

fn offer(is_offered: bool, words: &str) -> SiteOffer {
    SiteOffer {
        is_offered,
        words: if is_offered { words } else { "" }.to_owned(),
    }
}

/// The rules of the module's head, for one site.
pub(crate) fn offers(
    site: &SiteView,
    actor: &Actor,
    takes_sites_from_pilots: bool,
    confirmations: Option<&SiteConfirmations>,
) -> SiteOffers {
    let words = site_words();
    let Some(by) = site
        .by_pilots
        .as_ref()
        .filter(|_| is_own_layer(&site.source))
    else {
        // A source's site: its source's record.
        let nothing = || offer(false, "");
        return SiteOffers {
            correct: nothing(),
            remove: nothing(),
            confirm: nothing(),
            withdraw: nothing(),
            report: nothing(),
            history: nothing(),
            has_confirmed: false,
            confirmed_words: String::new(),
            note: "This site is its source's record: it is corrected at its source, and \
                   nothing is added to it here."
                .to_owned(),
            bring_back_words: String::new(),
        };
    };
    let (signed_in, partner, suspended, handle) = match actor {
        Actor::Pilot {
            handle,
            partner,
            suspended,
        } => (true, *partner, *suspended, Some(handle.as_str())),
        Actor::SignedOut | Actor::SessionEnded => (false, false, false, None),
    };
    let may_write = may_write(actor, by.from_pilots, takes_sites_from_pilots);
    if by.is_removed || by.is_of_history {
        // A view from the site's history: the site as it stood, or a
        // site that is gone. Nothing is written to it from here.
        let nothing = || offer(false, "");
        let bring_back = by.is_removed && may_write;
        return SiteOffers {
            correct: nothing(),
            remove: nothing(),
            confirm: nothing(),
            withdraw: nothing(),
            report: nothing(),
            history: offer(true, &words.history),
            has_confirmed: false,
            confirmed_words: String::new(),
            note: if by.is_removed {
                let mut note = "This site was removed: it is on no map.".to_owned();
                let why = history_note(actor, by.from_pilots, takes_sites_from_pilots, true);
                if !why.is_empty() {
                    note.push(' ');
                    note.push_str(&why);
                }
                note
            } else {
                format!(
                    "This is the site as revision {} of its history said it.",
                    by.revision
                )
            },
            bring_back_words: if bring_back {
                words.bring_back
            } else {
                String::new()
            },
        };
    }
    let may_confirm =
        signed_in && !suspended && takes_sites_from_pilots && takes_off_here(&site.uses);
    let mine = handle.and_then(|handle| {
        confirmations
            .filter(|list| list.site_id == site.id)?
            .lines
            .iter()
            .find(|line| line.by.handle == handle)
    });
    let note = match actor {
        Actor::SignedOut => words.signed_out,
        Actor::SessionEnded => words.signed_in_again,
        Actor::Pilot { .. } if suspended => STOPPED.to_owned(),
        Actor::Pilot { .. } => match (by.from_pilots, partner, takes_sites_from_pilots) {
            (false, false, true) => "A partner of Venti added this site, and only Venti's \
                                     partners change it. If something about it is wrong, \
                                     report it."
                .to_owned(),
            (false, false, false) => "A partner of Venti added this site, and only Venti's \
                                      partners change it. If something about it is wrong, \
                                      report it. This server takes no confirmations from \
                                      pilots yet."
                .to_owned(),
            (true, false, false) => {
                "This server takes no changes and no confirmations from pilots yet.".to_owned()
            }
            (_, true, false) => "This server takes no confirmations from pilots yet.".to_owned(),
            (_, _, true) => String::new(),
        },
    };
    SiteOffers {
        correct: offer(may_write, &site_form().title_correct),
        remove: offer(may_write, &words.remove),
        confirm: offer(
            may_confirm,
            if mine.is_some() {
                &words.confirm_again
            } else {
                &words.confirm
            },
        ),
        withdraw: offer(may_confirm && mine.is_some(), &words.withdraw),
        report: offer(signed_in, &words.report),
        history: offer(true, &words.history),
        has_confirmed: mine.is_some(),
        confirmed_words: mine.map_or_else(String::new, |line| {
            format!("You confirmed this site for {}.", line.day_in_words)
        }),
        note,
        bring_back_words: String::new(),
    }
}

/// Whether whoever asks may correct, remove, put back and bring back a
/// site of a tier: the one statement of that rule in the core.
fn may_write(actor: &Actor, from_pilots: bool, takes_sites_from_pilots: bool) -> bool {
    match actor {
        Actor::Pilot {
            partner, suspended, ..
        } => !suspended && (*partner || (from_pilots && takes_sites_from_pilots)),
        Actor::SignedOut | Actor::SessionEnded => false,
    }
}

const STOPPED: &str =
    "This account was stopped from publishing on Venti: it changes and confirms no site.";

/// Why a history does not offer to put back or to bring back, in a line.
/// Empty when it does.
fn history_note(
    actor: &Actor,
    from_pilots: bool,
    takes_sites_from_pilots: bool,
    is_removed: bool,
) -> String {
    match actor {
        Actor::SignedOut if is_removed => "Sign in to bring this site back.".to_owned(),
        Actor::SessionEnded if is_removed => "Sign in again to bring this site back.".to_owned(),
        Actor::SignedOut => "Sign in to put a revision back or to report one.".to_owned(),
        Actor::SessionEnded => "Sign in again to put a revision back or to report one.".to_owned(),
        Actor::Pilot {
            suspended: true, ..
        } => STOPPED.to_owned(),
        Actor::Pilot { partner: true, .. } => String::new(),
        Actor::Pilot { .. } => match (from_pilots, takes_sites_from_pilots, is_removed) {
            (true, true, _) => String::new(),
            (true, false, _) => "This server takes no changes from pilots yet.".to_owned(),
            (false, _, true) => "A partner of Venti added this site, and only Venti's \
                                 partners bring it back."
                .to_owned(),
            (false, _, false) => "A partner of Venti added this site, and only Venti's \
                                  partners change it. If something about it is wrong, report \
                                  it."
            .to_owned(),
        },
    }
}

/// The rules of the module's head, for a site's history.
pub(crate) fn history_offers(
    history: &SiteHistory,
    actor: &Actor,
    takes_sites_from_pilots: bool,
) -> SiteHistoryOffers {
    let words = site_words();
    let tier = history
        .site_id
        .parse::<venti_domain::SiteId>()
        .ok()
        .map(|id| id.source().as_str().to_owned())
        .filter(|source| is_own_layer(source));
    let Some(tier) = tier else {
        // A source's site has no history here.
        let nothing = || offer(false, "");
        return SiteHistoryOffers {
            put_back: nothing(),
            undo: nothing(),
            bring_back: nothing(),
            report_revision: nothing(),
            note: String::new(),
        };
    };
    let from_pilots = tier == VENTI_PILOTS_SITES_SOURCE;
    let may_write = may_write(actor, from_pilots, takes_sites_from_pilots);
    let signed_in = matches!(actor, Actor::Pilot { .. });
    SiteHistoryOffers {
        put_back: offer(may_write, &words.put_back),
        undo: offer(
            may_write && history.undo_revision.is_some(),
            "Undo the last change",
        ),
        bring_back: offer(
            may_write && history.bring_back_revision.is_some(),
            &words.bring_back,
        ),
        report_revision: offer(signed_in, &words.report_revision),
        note: history_note(
            actor,
            from_pilots,
            takes_sites_from_pilots,
            history.is_removed,
        ),
    }
}

/// What the form says when it corrects a site: by the site's tier.
///
/// # Errors
///
/// For a site of a source, which is corrected at its source: as
/// [`crate::site_draft_of`].
#[uniffi::export]
pub fn site_correcting(site: SiteView) -> Result<SiteCorrecting, CoreError> {
    let Some(by) = site
        .by_pilots
        .as_ref()
        .filter(|_| is_own_layer(&site.source))
    else {
        return Err(invalid(
            "This site is its source's record and is corrected at its source. Only a site \
             that a partner or pilots added on Venti is corrected here.",
        ));
    };
    let form = site_form();
    Ok(SiteCorrecting {
        as_partner: !by.from_pilots,
        title: form.title_correct,
        giving: if by.from_pilots {
            form.giving
        } else {
            form.giving_as_partner
        },
        note: by.note.clone(),
        send_words: form.send_correction,
    })
}

/// The rules of the module's head, for a new site.
pub(crate) fn adding(actor: &Actor, takes_sites_from_pilots: bool) -> SiteAdding {
    let form = site_form();
    let no = |why: &str| SiteAdding {
        may_add: false,
        as_partner: false,
        layer: String::new(),
        entry_words: String::new(),
        title: String::new(),
        giving: String::new(),
        note: String::new(),
        send_words: String::new(),
        why_not: why.to_owned(),
        leads_to_sign_in: false,
    };
    // Someone who has to sign in first: the sentence is the entry.
    let sign_in = |words: &str| SiteAdding {
        entry_words: words.to_owned(),
        leads_to_sign_in: true,
        ..no(words)
    };
    const CLOSED: &str = "This server takes no places from pilots yet.";
    match actor {
        Actor::Pilot {
            suspended: true, ..
        } => no("This account was stopped from publishing on Venti: it adds no site."),
        Actor::Pilot { partner: true, .. } => SiteAdding {
            may_add: true,
            as_partner: true,
            layer: VENTI_SITES_SOURCE.to_owned(),
            entry_words: form.title_new.clone(),
            title: form.title_new,
            giving: form.giving_as_partner,
            note: form.note_as_partner,
            send_words: form.send_new,
            why_not: String::new(),
            leads_to_sign_in: false,
        },
        Actor::Pilot { .. } if takes_sites_from_pilots => SiteAdding {
            may_add: true,
            as_partner: false,
            layer: VENTI_PILOTS_SITES_SOURCE.to_owned(),
            entry_words: "Add a place pilots take off from".to_owned(),
            title: "A place pilots take off from".to_owned(),
            giving: form.giving,
            note: form.note,
            send_words: "Add as a place from pilots".to_owned(),
            why_not: String::new(),
            leads_to_sign_in: false,
        },
        _ if !takes_sites_from_pilots => no(CLOSED),
        Actor::SessionEnded => sign_in("Sign in again to add a place pilots take off from."),
        _ => sign_in("Sign in to add a place pilots take off from."),
    }
}

#[uniffi::export]
impl Core {
    /// What a shell may offer for a site to whoever is signed in on this
    /// device, as one answer: to correct it, remove it, confirm it,
    /// report it and read its history, each with its words, and whether
    /// this pilot has confirmed it.
    ///
    /// `takes_sites_from_pilots` is what the server's status says
    /// ([`crate::ServerStatus`]); `false` while the shell has no status.
    /// `confirmations` is the site's list as [`site_confirmations`] read
    /// it, when the shell has it: without it the answer does not know
    /// whether this pilot confirmed the site, and offers no taking back.
    ///
    /// The account is the one this device stores: whether it is a
    /// partner is as the server said last ([`Core::account_request`]).
    pub fn site_offers(
        &self,
        site: SiteView,
        takes_sites_from_pilots: bool,
        confirmations: Option<SiteConfirmations>,
    ) -> Result<SiteOffers, CoreError> {
        Ok(offers(
            &site,
            &self.site_actor()?,
            takes_sites_from_pilots,
            confirmations.as_ref(),
        ))
    }

    /// Whether a shell may offer to add a site, into which tier it would
    /// go, and what the entry and the form say for that tier. A shell
    /// shows the entry whenever its words are not empty, and opens the
    /// account in place of the form where it leads to signing in.
    pub fn site_adding(&self, takes_sites_from_pilots: bool) -> Result<SiteAdding, CoreError> {
        Ok(adding(&self.site_actor()?, takes_sites_from_pilots))
    }

    /// What a shell may offer in a site's history to whoever is signed
    /// in on this device: to put a revision back, to undo the last
    /// change, to bring a removed site back and to report a revision.
    /// `history` is a page of it as [`crate::site_history`] read it; the
    /// tier is read from the site's identifier, so a history reached by
    /// identifier alone is answered too.
    pub fn site_history_offers(
        &self,
        history: SiteHistory,
        takes_sites_from_pilots: bool,
    ) -> Result<SiteHistoryOffers, CoreError> {
        Ok(history_offers(
            &history,
            &self.site_actor()?,
            takes_sites_from_pilots,
        ))
    }

    /// Reads the server's answer to [`sites_removed_path`]: the sites of
    /// a tier that are removed, each in a line with who removed it and
    /// when, and whether whoever is signed in on this device may bring
    /// it back. One answer names them all: no request per site.
    ///
    /// # Errors
    ///
    /// The server's refusal in words.
    pub fn sites_removed(
        &self,
        status: u16,
        body: Vec<u8>,
        takes_sites_from_pilots: bool,
    ) -> Result<SitesRemoved, CoreError> {
        let response: SiteChangesResponse = read(status, &body)?;
        Ok(removed(
            &response,
            &self.site_actor()?,
            takes_sites_from_pilots,
        ))
    }

    /// The pilot's own flights to name with a confirmation of a site for
    /// a day: those of the logbook that began on that day, took off
    /// within the server's 500 m of the site and are on the server, the
    /// earliest first. Empty when there is none, and then a shell shows
    /// no choice.
    ///
    /// `day` is `YYYY-MM-DD`, the pilot's own day; `utc_offset_s` is how
    /// far the device's clock is ahead of UTC on that day, in seconds,
    /// so that a flight counts to the day the pilot flew it on. A
    /// flight's take-off is taken as where its track begins: the server
    /// finds the take-off itself and has the last word.
    pub fn site_confirm_flights(
        &self,
        site: SiteView,
        day: String,
        utc_offset_s: i32,
    ) -> Result<Vec<SiteConfirmFlight>, CoreError> {
        let day = parse_day(&day)?;
        let Ok(place) = GeoPoint::new(site.position.lat, site.position.lon) else {
            return Ok(Vec::new());
        };
        let local = |unix_s: i64| {
            chrono::DateTime::from_timestamp(unix_s.saturating_add(i64::from(utc_offset_s)), 0)
        };
        let mut flights = Vec::new();
        for (id, summary) in self.logbook.flights() {
            let Some(began) = local(summary.start_unix_s) else {
                continue;
            };
            let near = summary.outline.first().is_some_and(|&[lat, lon]| {
                GeoPoint::new(lat, lon)
                    .is_ok_and(|start| start.distance_m(place) <= CONFIRMATION_FLIGHT_WITHIN_M)
            });
            if began.date_naive() != day || !near || self.flight_address(&id)?.is_none() {
                continue;
            }
            let minutes = summary.duration_s.max(0) / 60;
            flights.push((
                summary.start_unix_s,
                SiteConfirmFlight {
                    id,
                    words: format!(
                        "Your flight from {}, {} h {:02} min",
                        began.format("%H:%M"),
                        minutes / 60,
                        minutes % 60
                    ),
                },
            ));
        }
        flights.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id)));
        Ok(flights.into_iter().map(|(_, flight)| flight).collect())
    }

    /// The request that says of a site "I took off here and it was
    /// good", for a signed-in pilot. Sending it again takes the place of
    /// the pilot's first, which is how its day is changed.
    ///
    /// `day` is the day the pilot speaks of and `today` the device's
    /// date, both `YYYY-MM-DD`: one of the days [`site_confirm_form`]
    /// offers. `note` is a line for other pilots and may be empty.
    /// `flight` is the identifier of one of the logbook's flights, from
    /// [`Core::site_confirm_flights`]; the server is sent its address
    /// there, checks that it took off at the site and keeps only that it
    /// did.
    ///
    /// # Errors
    ///
    /// In words for the pilot: a site of a source, a day that is to come
    /// or too long ago, a note that is too long, and a flight the server
    /// does not have.
    pub fn site_confirm_request(
        &self,
        id: String,
        day: String,
        today: String,
        note: String,
        flight: Option<String>,
    ) -> Result<ServerRequest, CoreError> {
        let id = made_here(&id).map_err(|_| {
            invalid(
                "Only a site that a partner or pilots added on Venti is confirmed here. This \
                 one is its source's record.",
            )
        })?;
        let (day, today) = (parse_day(&day)?, parse_day(&today)?);
        if day > today || day < first_day(today) {
            return Err(invalid(&format!(
                "Choose today or one of the {} days before it.",
                DAYS_OFFERED - 1
            )));
        }
        let note = parse_site_comment(Some(&note))
            .map_err(|most| invalid(&format!("This line has room for {most} characters.")))?;
        let flight = match flight.filter(|flight| !flight.is_empty()) {
            None => None,
            Some(flight) => Some(self.flight_address(&flight)?.ok_or_else(|| {
                invalid(
                    "This flight is not on the server yet. Sync first, or confirm the site \
                     without it.",
                )
            })?),
        };
        json_request(
            "PUT",
            &api::site_confirmation_path(&id),
            &ConfirmSiteRequest { day, note, flight },
            true,
        )
    }
}

// --- Confirming -----------------------------------------------------------------

/// How many days the form offers: today and the days before it. One
/// fewer than the server takes, which counts from its own date: a device
/// west of it is a day behind, and its oldest day would be refused.
const DAYS_OFFERED: u64 = CONFIRMATION_DAYS_BACK as u64;

fn parse_day(day: &str) -> Result<NaiveDate, CoreError> {
    day.parse()
        .map_err(|_| invalid("The app could not read this day."))
}

/// The oldest day the form offers.
fn first_day(today: NaiveDate) -> NaiveDate {
    today
        .checked_sub_days(Days::new(DAYS_OFFERED - 1))
        .unwrap_or(today)
}

/// "Sat 14 Nov": a day of the last weeks, where the year is plain.
fn near_day(day: NaiveDate) -> String {
    day.format("%a %-d %b").to_string()
}

/// "Sat 14 Nov 2026": a day that may be long ago.
fn any_day(day: NaiveDate) -> String {
    day.format("%a %-d %b %Y").to_string()
}

/// A day a confirmation can speak of.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteConfirmDay {
    /// The day as `YYYY-MM-DD`, to hand to [`Core::site_confirm_request`]
    /// and [`Core::site_confirm_flights`].
    pub day: String,
    /// The day in words: "Today", "Yesterday", "Sat 14 Nov".
    pub words: String,
}

/// One of the pilot's flights to name with a confirmation.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteConfirmFlight {
    /// The flight's identifier in the logbook, to hand to
    /// [`Core::site_confirm_request`].
    pub id: String,
    /// The flight in a line: "Your flight from 11:42, 1 h 05 min".
    pub words: String,
}

/// What a shell shows to take a confirmation: the days to choose from
/// and the words around them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteConfirmForm {
    /// The sentence the pilot says, as a title and as the button that
    /// opens the form: "I took off here and it was good".
    pub title: String,
    /// What saying it means: "Other pilots see your handle, the day and
    /// your line with this site. …".
    pub about: String,
    /// What stands over the days: "On which day".
    pub day_title: String,
    /// The days, today first, back as far as the server takes them.
    pub days: Vec<SiteConfirmDay>,
    /// What stands over the note: "A line for other pilots".
    pub note_title: String,
    /// A line of help under it.
    pub note_help: String,
    /// The most characters the note may have.
    pub note_at_most: u32,
    /// What stands over the choice of a flight, shown only when
    /// [`Core::site_confirm_flights`] has one: "With one of your
    /// flights".
    pub flight_title: String,
    /// What naming a flight does: "Venti checks that it took off here
    /// and says only that it did, not which flight."
    pub flight_help: String,
    /// The choice that names no flight: "Without a flight".
    pub no_flight: String,
    /// The button that sends it: "Confirm".
    pub send: String,
}

/// The form for a confirmation. `today` is the device's date as
/// `YYYY-MM-DD`: the core reads no clock.
///
/// # Errors
///
/// When `today` is no date.
#[uniffi::export]
pub fn site_confirm_form(today: String) -> Result<SiteConfirmForm, CoreError> {
    let today = parse_day(&today)?;
    let days = (0..DAYS_OFFERED)
        .filter_map(|back| Some((back, today.checked_sub_days(Days::new(back))?)))
        .map(|(back, day)| SiteConfirmDay {
            day: day.format("%Y-%m-%d").to_string(),
            words: match back {
                0 => "Today".to_owned(),
                1 => "Yesterday".to_owned(),
                _ => near_day(day),
            },
        })
        .collect();
    let words = site_words();
    Ok(SiteConfirmForm {
        title: words.confirm,
        about: "Other pilots see your handle, the day and your line with this site. It says \
                that you took off here and that it went well on that day: it is no word that \
                the place is official or that anyone may fly there."
            .to_owned(),
        day_title: "On which day".to_owned(),
        days,
        note_title: "A line for other pilots".to_owned(),
        note_help: "What the wind and the launch were like. Everyone can read it; leave it \
                    empty if you like."
            .to_owned(),
        note_at_most: u32::try_from(MAX_SITE_COMMENT_LENGTH).unwrap_or(u32::MAX),
        flight_title: "With one of your flights".to_owned(),
        flight_help: "Venti checks that it took off here and says only that it did, not which \
                      flight."
            .to_owned(),
        no_flight: "Without a flight".to_owned(),
        send: "Confirm".to_owned(),
    })
}

/// The request that takes the pilot's own confirmation of a site back.
/// The server answers the same whether there was one or not.
#[uniffi::export]
pub fn site_withdraw_request(id: String) -> Result<ServerRequest, CoreError> {
    let id = made_here(&id)?;
    Ok(ServerRequest {
        method: "DELETE".to_owned(),
        path: api::site_confirmation_path(&id),
        body: None,
        content_type: None,
        needs_token: true,
    })
}

/// A refusal of a confirmation, in words for the pilot.
fn confirm_refusal(status: u16, body: &[u8]) -> CoreError {
    let message = match (status, code(body).as_deref()) {
        (_, Some("invalid_confirmation")) => format!(
            "The server does not take this confirmation. The day has to be today or one of \
             the {CONFIRMATION_DAYS_BACK} days before it, and a flight named with it has to \
             be yours and to have taken off within {CONFIRMATION_FLIGHT_WITHIN_M:.0} m of \
             the site. Try it without the flight."
        ),
        (_, Some("sites_closed")) => {
            "This server takes no confirmations from pilots yet.".to_owned()
        }
        (429, _) | (_, Some("rate_limited")) => {
            "You have confirmed and taken back as often today as one account may. Try again \
             tomorrow."
                .to_owned()
        }
        (401, _) | (_, Some("unauthorized")) => {
            "Sign in to say that you took off here. This device is no longer signed in.".to_owned()
        }
        _ => return site_refusal(status, body),
    };
    CoreError::Server { message }
}

/// Reads the server's answer to [`Core::site_confirm_request`]: the site
/// as it stands with the confirmation counted, and what to say after.
/// `day` is the day that was sent, for the words: "You confirmed this
/// site for Sat 14 Nov."
///
/// # Errors
///
/// What the refusal means, in words: the server does not take the day or
/// the flight (`invalid_confirmation`), it takes no confirmations from
/// pilots (`sites_closed`), the account confirmed as often today as one
/// may (`429`), the device is signed out (`401`), the account may not
/// publish, or the site is gone.
#[uniffi::export]
pub fn site_confirmed(status: u16, body: Vec<u8>, day: String) -> Result<SiteWritten, CoreError> {
    if !(200..300).contains(&status) {
        return Err(confirm_refusal(status, &body));
    }
    let response: SiteResponse = read(status, &body)?;
    let site = site_view(&response.site, response.standing.as_ref());
    let words = day.parse::<NaiveDate>().map_or_else(
        |_| "You confirmed this site.".to_owned(),
        |day| format!("You confirmed this site for {}.", near_day(day)),
    );
    Ok(SiteWritten { site, words })
}

/// Reads the server's answer to [`site_withdraw_request`] and says what
/// became of it. The site's count is read anew ([`crate::site_path`],
/// [`site_confirmations_path`]): this answer has no body.
///
/// # Errors
///
/// As [`site_confirmed`].
#[uniffi::export]
pub fn site_withdrawn(status: u16, body: Vec<u8>) -> Result<String, CoreError> {
    if (200..300).contains(&status) {
        Ok("Your confirmation of this site is taken back.".to_owned())
    } else {
        Err(confirm_refusal(status, &body))
    }
}

/// One pilot's confirmation of a site, as its page lists it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteConfirmationLine {
    /// Who says so.
    pub by: SiteAuthor,
    /// The day they speak of, as `YYYY-MM-DD`.
    pub day: String,
    /// The day in words: "Mon 16 Nov 2026".
    pub day_in_words: String,
    /// What they add, if anything. A pilot's text: shown as written.
    pub note: Option<String>,
    /// Whether the server found a flight of theirs that took off at the
    /// site.
    pub with_flight: bool,
    /// The confirmation in a line: "Mara Muster (@mara) on Mon 16 Nov
    /// 2026, with a flight: “North-west at noon, smooth.”"
    pub words: String,
}

/// Who says they took off at a site: the answer of
/// [`site_confirmations_path`]. A shell hands it back to
/// [`Core::site_offers`], which finds the signed-in pilot's own in it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteConfirmations {
    /// The site.
    pub site_id: String,
    /// How many pilots confirmed it.
    pub pilots: u32,
    /// The count and the newest day in a line: "2 pilots took off here,
    /// last on 16 November 2026". Empty while nobody has.
    pub words: String,
    /// What to say while nobody has: "No pilot has said yet that they
    /// took off here." Empty otherwise.
    pub nobody_words: String,
    /// The confirmations, the newest day first: at most a hundred.
    pub lines: Vec<SiteConfirmationLine>,
    /// Whether `lines` holds every confirmation: `false` when more
    /// pilots confirmed the site than one answer lists.
    pub is_whole: bool,
    /// What to say under the lines when they are not all: "The 100
    /// newest are shown." Empty when `is_whole`.
    #[uniffi(default = "")]
    #[cfg_attr(feature = "wasm", serde(default), tsify(optional))]
    pub not_whole_words: String,
}

/// The path to `GET` for what pilots confirm of a site that a partner or
/// pilots added. No token.
#[uniffi::export]
pub fn site_confirmations_path(id: String) -> Result<String, CoreError> {
    Ok(api::site_confirmations_path(&made_here(&id)?))
}

/// Reads the server's answer to [`site_confirmations_path`] for the
/// site's page: as [`site_confirmations`], and by what the site is used
/// for. For a site nobody takes off at (a landing), of which the
/// sentence is not offered, `nobody_words` is empty: its page does not
/// say that no pilot took off there.
///
/// # Errors
///
/// As [`site_confirmations`].
#[uniffi::export]
pub fn site_confirmations_of(
    site: SiteView,
    status: u16,
    body: Vec<u8>,
) -> Result<SiteConfirmations, CoreError> {
    let mut list = site_confirmations(status, body)?;
    if !takes_off_here(&site.uses) {
        list.nobody_words.clear();
    }
    Ok(list)
}

/// Reads the server's answer to [`site_confirmations_path`], without
/// the site: `nobody_words` is then said whatever the site is used for.
/// A site's page reads it with [`site_confirmations_of`].
///
/// # Errors
///
/// The server's refusal in words: that there is no such site, or that it
/// was removed.
#[uniffi::export]
pub fn site_confirmations(status: u16, body: Vec<u8>) -> Result<SiteConfirmations, CoreError> {
    let response: SiteConfirmationsResponse = read(status, &body)?;
    let lines: Vec<SiteConfirmationLine> = response
        .confirmations
        .iter()
        .map(|said| {
            let by = author(&said.by);
            let day_in_words = any_day(said.day);
            let mut words = format!("{} on {day_in_words}", author_in_words(&by));
            if said.with_flight {
                words.push_str(", with a flight");
            }
            if let Some(note) = &said.note {
                words.push_str(": ");
                words.push_str(&quoted(note));
            }
            SiteConfirmationLine {
                by,
                day: said.day.format("%Y-%m-%d").to_string(),
                day_in_words,
                note: said.note.clone(),
                with_flight: said.with_flight,
                words,
            }
        })
        .collect();
    let listed = u32::try_from(lines.len()).unwrap_or(u32::MAX);
    Ok(SiteConfirmations {
        site_id: response.id.to_string(),
        pilots: response.confirmed.pilots,
        words: confirmed_in_words(&response.confirmed),
        nobody_words: if response.confirmed.pilots == 0 {
            "No pilot has said yet that they took off here.".to_owned()
        } else {
            String::new()
        },
        is_whole: listed >= response.confirmed.pilots,
        not_whole_words: match listed {
            _ if listed >= response.confirmed.pilots => String::new(),
            1 => "The newest is shown.".to_owned(),
            listed => format!("The {listed} newest are shown."),
        },
        lines,
    })
}

// --- The words of a site's page -------------------------------------------------

/// The words around a site's page, its form and its history that are the
/// same in the three apps. What depends on the site and on who asks is
/// in [`SiteOffers`] and [`SiteAdding`]; the form's own words are in
/// [`crate::SiteForm`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteWords {
    /// What a signed-out pilot is told on a site's page: "Sign in to
    /// correct this site, to say that you took off here and to report
    /// it."
    pub signed_out: String,
    /// The same once the session has ended: "Sign in again to …".
    pub signed_in_again: String,
    /// The button that begins moving a site's place on the map: "Move".
    pub move_place: String,
    /// The button that takes the place under the map's mark: "Use this
    /// place".
    pub use_place: String,
    /// "Cancel".
    pub cancel: String,
    /// The entry and the title of a site's history: "History".
    pub history: String,
    /// The entry that removes a site: "Remove this site".
    pub remove: String,
    /// The question before it is removed: "Remove this site from the
    /// map?"
    pub remove_question: String,
    /// What removing does, under the question: "It goes from the map for
    /// everyone. Its history stays, and any pilot who may correct the
    /// site can bring it back."
    pub remove_about: String,
    /// The button on a revision of a site that stands: "Put back".
    pub put_back: String,
    /// The question before a revision is put back: "Put the site back to
    /// this revision?"
    pub put_back_question: String,
    /// What putting back does: "The site then says again what it said at
    /// this revision, as a new revision. Nothing of its history is
    /// lost."
    pub put_back_about: String,
    /// The button on a removed site: "Bring this site back".
    pub bring_back: String,
    /// The question before it: "Bring this site back?"
    pub bring_back_question: String,
    /// What bringing back does: "It is on the map again as it stood
    /// before it was removed, as a new revision."
    pub bring_back_about: String,
    /// "Report this site".
    pub report: String,
    /// "Report this revision".
    pub report_revision: String,
    /// After a correction was refused because the site changed
    /// meanwhile: "Show how it stands now".
    pub show_now: String,
    /// What a crowdsourced site is marked with by its name: "From
    /// pilots". The same as its [`crate::SiteByPilots::label`].
    pub from_pilots_label: String,
    /// The switch that shows the crowdsourced layer, in the sites' filter
    /// or the map's layers: "Places from pilots".
    pub pilots_layer: String,
    /// What the switch shows, under it: "Places pilots say they take off
    /// from. Not official and not checked by anyone: a place on the map
    /// is not permission to fly there."
    pub pilots_layer_about: String,
    /// The sentence a pilot says of a site: "I took off here and it was
    /// good".
    pub confirm: String,
    /// The same entry for a pilot who has said it: "Change the day".
    pub confirm_again: String,
    /// "Take my confirmation back".
    pub withdraw: String,
    /// The question before it: "Take your confirmation of this site
    /// back?"
    pub withdraw_question: String,
    /// The title over a site's confirmations: "Pilots who took off
    /// here".
    pub confirmations_title: String,
    /// The title of the list of removed sites: "Removed lately".
    pub removed_title: String,
}

/// The words of a site's page, its form and its history.
#[uniffi::export]
pub fn site_words() -> SiteWords {
    let text = str::to_owned;
    SiteWords {
        signed_out: text(
            "Sign in to correct this site, to say that you took off here and to report it.",
        ),
        signed_in_again: text(
            "Sign in again to correct this site, to say that you took off here and to report \
             it.",
        ),
        move_place: text("Move"),
        use_place: text("Use this place"),
        cancel: text("Cancel"),
        history: text("History"),
        remove: text("Remove this site"),
        remove_question: text("Remove this site from the map?"),
        remove_about: text(
            "It goes from the map for everyone. Its history stays, and any pilot who may \
             correct the site can bring it back.",
        ),
        put_back: text("Put back"),
        put_back_question: text("Put the site back to this revision?"),
        put_back_about: text(
            "The site then says again what it said at this revision, as a new revision. \
             Nothing of its history is lost.",
        ),
        bring_back: text("Bring this site back"),
        bring_back_question: text("Bring this site back?"),
        bring_back_about: text(
            "It is on the map again as it stood before it was removed, as a new revision.",
        ),
        report: text("Report this site"),
        report_revision: text("Report this revision"),
        show_now: text("Show how it stands now"),
        from_pilots_label: text(FROM_PILOTS_LABEL),
        pilots_layer: text(crate::site_filter::PILOTS_LAYER),
        pilots_layer_about: text(crate::site_filter::PILOTS_LAYER_ABOUT),
        confirm: text("I took off here and it was good"),
        confirm_again: text("Change the day"),
        withdraw: text("Take my confirmation back"),
        withdraw_question: text("Take your confirmation of this site back?"),
        confirmations_title: text("Pilots who took off here"),
        removed_title: text("Removed lately"),
    }
}

// --- Sites that were removed ----------------------------------------------------

/// One page of the sites of a tier that are removed.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SitesRemoved {
    /// The identifiers of sites this page says are removed.
    pub ids: Vec<String>,
    /// What to pass as `since` for the next page.
    pub cursor: String,
    /// Whether more is waiting: ask again with `cursor`.
    pub more: bool,
    /// Whether the server started again from the beginning: a shell
    /// forgets the identifiers and the lines of the pages before.
    pub reset: bool,
    /// The same sites as lines of "Removed lately", in the server's
    /// order: each with its name, who removed it and when. A server from
    /// before 2026-11-21 names none, and `lines` is then empty beside
    /// `ids`.
    #[uniffi(default = [])]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub lines: Vec<SiteRemovedLine>,
}

/// The path to `GET` for what happened to the sites of a tier, to find
/// the removed ones: the crowdsourced tier with `from_pilots`, else the
/// partners'. `since` is the cursor of the page before; without it the
/// server begins at the beginning, which names every site of the tier
/// once, as it stands. No token.
///
/// # Errors
///
/// A `since` that is no cursor of the server's.
#[uniffi::export]
pub fn sites_removed_path(from_pilots: bool, since: Option<String>) -> Result<String, CoreError> {
    let layer = if from_pilots {
        VENTI_PILOTS_SITES_SOURCE
    } else {
        VENTI_SITES_SOURCE
    };
    let mut path = format!("{}?layer={layer}", api::SITE_CHANGES_PATH);
    if let Some(since) = since.filter(|since| !since.is_empty()) {
        // The server's cursors are numbers; anything else is not one.
        if !since.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(invalid(
                "The app could not ask the server for these changes.",
            ));
        }
        path.push_str("&since=");
        path.push_str(&since);
    }
    Ok(path)
}

/// A removed site in a line of "Removed lately".
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteRemovedLine {
    /// The site, to open its history with ([`crate::site_history_path`]).
    pub id: String,
    /// Its name as it last stood. "A site that was taken down" for one
    /// that whoever runs Venti took down, which shows nothing of what it
    /// said.
    pub name: String,
    /// Who removed it. Absent for whoever runs Venti, and once the
    /// pilot's account is gone.
    pub by: Option<SiteAuthor>,
    /// When, seconds since 1970-01-01 UTC.
    pub at_unix_s: i64,
    /// The line: "Beispieljoch: removed by Jonas Beispiel (@jonas) on 21
    /// Nov 2026". The day is the day in UTC.
    pub words: String,
    /// Whether the site is of the crowdsourced tier.
    pub from_pilots: bool,
    /// Whether whoever is signed in on this device may bring it back:
    /// they may correct a site of its tier, and it was not taken down.
    /// The revision to bring back is its history's
    /// ([`SiteHistory::bring_back_revision`]), which a shell reads when
    /// the pilot asks for it: one request, for the one site.
    pub may_bring_back: bool,
    /// The words of that entry: "Bring this site back". Empty when
    /// `may_bring_back` is not.
    pub bring_back_words: String,
}

/// The removed sites of an answer, as lines, for whoever asks.
fn removed(
    response: &SiteChangesResponse,
    actor: &Actor,
    takes_sites_from_pilots: bool,
) -> SitesRemoved {
    let bring_back = site_words().bring_back;
    let lines = response
        .removals
        .iter()
        .map(|removal| {
            let by = removal.by.as_ref().map(author);
            let on = day_in_words(removal.at);
            let from_pilots = removal.id.source().as_str() == VENTI_PILOTS_SITES_SOURCE;
            // A pilot's text from a server: one line of it.
            let name = removal
                .name
                .as_deref()
                .map(|name| name.chars().filter(|c| !c.is_control()).collect::<String>())
                .map(|name| name.trim().to_owned())
                .filter(|name| !name.is_empty());
            let words = match (&name, &by) {
                (Some(name), Some(by)) => {
                    format!("{name}: removed by {} on {on}", author_in_words(by))
                }
                // The answer does not tell whoever runs Venti from a
                // pilot whose account is gone: the history does.
                (Some(name), None) => format!("{name}: removed on {on}"),
                (None, _) => format!("A site that whoever runs Venti took down on {on}"),
            };
            let may_bring_back =
                name.is_some() && may_write(actor, from_pilots, takes_sites_from_pilots);
            SiteRemovedLine {
                id: removal.id.to_string(),
                name: name.unwrap_or_else(|| "A site that was taken down".to_owned()),
                by,
                at_unix_s: removal.at.timestamp(),
                words,
                from_pilots,
                may_bring_back,
                bring_back_words: if may_bring_back {
                    bring_back.clone()
                } else {
                    String::new()
                },
            }
        })
        .collect();
    SitesRemoved {
        ids: response.removed.iter().map(ToString::to_string).collect(),
        cursor: response.cursor.clone(),
        more: response.more,
        reset: response.reset,
        lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempFolder;
    use crate::{Coordinate, SiteKind, site_history, site_page};

    fn golden(name: &str) -> Vec<u8> {
        let path = format!(
            "{}/../server/tests/golden/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read(path).unwrap()
    }

    fn words(result: Result<impl std::fmt::Debug, CoreError>) -> String {
        match result {
            Err(CoreError::Server { message }) => message,
            other => panic!("{other:?}"),
        }
    }

    fn refused(status: u16, code: &str) -> (u16, Vec<u8>) {
        (
            status,
            format!(r#"{{"error":{{"code":"{code}","message":"for developers"}}}}"#).into_bytes(),
        )
    }

    /// The partners' take-off of the development mode.
    fn partners_site() -> SiteView {
        site_page(200, golden("site.json")).unwrap().site
    }

    /// The same as a site of the crowdsourced layer.
    fn pilots_site() -> SiteView {
        let text = String::from_utf8(golden("site.json")).unwrap();
        let text = text
            .replace("\"venti:", "\"venti-pilots:")
            .replace("\"source\": \"venti\"", "\"source\": \"venti-pilots\"");
        let site = site_page(200, text.into_bytes()).unwrap().site;
        assert!(site.by_pilots.as_ref().unwrap().from_pilots);
        site
    }

    fn pilot(handle: &str, partner: bool) -> Actor {
        Actor::Pilot {
            handle: handle.to_owned(),
            partner,
            suspended: false,
        }
    }

    /// What is offered, as letters: Correct, Remove, conFirm, Withdraw,
    /// rePort, History.
    fn offered(offers: &SiteOffers) -> String {
        [
            (&offers.correct, 'C'),
            (&offers.remove, 'R'),
            (&offers.confirm, 'F'),
            (&offers.withdraw, 'W'),
            (&offers.report, 'P'),
            (&offers.history, 'H'),
        ]
        .into_iter()
        .filter(|(offer, _)| {
            assert_eq!(offer.is_offered, !offer.words.is_empty());
            offer.is_offered
        })
        .map(|(_, letter)| letter)
        .collect()
    }

    #[test]
    fn what_is_offered_for_a_site_follows_the_tier_the_account_and_the_server() {
        let confirmations = site_confirmations(200, golden("site_confirmations.json")).unwrap();
        let ask = |site: &SiteView, actor: &Actor, takes: bool| {
            offered(&offers(site, actor, takes, Some(&confirmations)))
        };
        let (partners, pilots) = (partners_site(), pilots_site());

        // Signed out: the history, and a line that says to sign in.
        for site in [&partners, &pilots] {
            assert_eq!(ask(site, &Actor::SignedOut, true), "H");
            assert_eq!(ask(site, &Actor::SessionEnded, true), "H");
        }
        let out = offers(&partners, &Actor::SignedOut, true, None);
        assert_eq!(
            out.note,
            "Sign in to correct this site, to say that you took off here and to report it."
        );
        assert!(
            offers(&partners, &Actor::SessionEnded, true, None)
                .note
                .starts_with("Sign in again")
        );

        // A pilot: confirms and reports a partner's site, and changes a
        // crowdsourced one, where the server takes sites from pilots.
        let jonas = pilot("jonas", false);
        assert_eq!(ask(&partners, &jonas, true), "FWPH");
        assert_eq!(ask(&pilots, &pilot("someone", false), true), "CRFPH");
        assert_eq!(ask(&partners, &jonas, false), "PH");
        assert_eq!(ask(&pilots, &jonas, false), "PH");
        assert!(
            offers(&partners, &jonas, true, None)
                .note
                .contains("only Venti's partners change it")
        );
        assert_eq!(
            offers(&pilots, &jonas, false, None).note,
            "This server takes no changes and no confirmations from pilots yet."
        );
        assert_eq!(offers(&pilots, &jonas, true, None).note, "");

        // A partner: changes both, whatever the setting; confirming
        // needs the setting, as for any pilot.
        let demo = pilot("demo", true);
        assert_eq!(ask(&partners, &demo, true), "CRFPH");
        assert_eq!(ask(&pilots, &demo, true), "CRFPH");
        assert_eq!(ask(&partners, &demo, false), "CRPH");
        assert_eq!(
            offers(&partners, &demo, false, None).note,
            "This server takes no confirmations from pilots yet."
        );

        // Suspended: reads and reports.
        let stopped = Actor::Pilot {
            handle: "demo".to_owned(),
            partner: true,
            suspended: true,
        };
        assert_eq!(ask(&partners, &stopped, true), "PH");
        assert!(
            offers(&pilots, &stopped, true, None)
                .note
                .contains("stopped")
        );

        // Who has confirmed is found by handle in the list handed in,
        // and nowhere without it.
        let mine = offers(&partners, &jonas, true, Some(&confirmations));
        assert!(mine.has_confirmed);
        assert_eq!(
            mine.confirmed_words,
            "You confirmed this site for Mon 9 Nov 2026."
        );
        assert_eq!(mine.confirm.words, "Change the day");
        assert_eq!(mine.withdraw.words, "Take my confirmation back");
        let unknown = offers(&partners, &jonas, true, None);
        assert!(!unknown.has_confirmed && !unknown.withdraw.is_offered);
        assert_eq!(unknown.confirm.words, "I took off here and it was good");
        assert_eq!(unknown.correct.words, "");
        // Another site's list says nothing of this one.
        assert!(!offers(&pilots, &jonas, true, Some(&confirmations)).has_confirmed);

        // A landing is not confirmed: nobody took off there.
        let landing = SiteView {
            uses: vec![SiteKind::Landing],
            ..pilots.clone()
        };
        assert_eq!(ask(&landing, &pilot("someone", false), true), "CRPH");

        // A source's site: nothing, and why.
        let osm = site_page(200, golden("site_of_a_source.json"))
            .unwrap()
            .site;
        let source = offers(&osm, &demo, true, None);
        assert_eq!(offered(&source), "");
        assert!(source.note.contains("corrected at its source"));
    }

    #[test]
    fn adding_goes_to_the_tier_of_the_account_with_the_tiers_words() {
        let form = site_form();
        let partner = adding(&pilot("demo", true), false);
        assert!(partner.may_add && partner.as_partner);
        assert_eq!(partner.layer, "venti");
        assert_eq!(partner.entry_words, "Add a site");
        assert_eq!(partner.giving, form.giving_as_partner);
        assert_eq!(partner.note, form.note_as_partner);
        assert_eq!(partner.send_words, "Add for everyone");

        let anyone = adding(&pilot("mara", false), true);
        assert!(anyone.may_add && !anyone.as_partner);
        assert_eq!(anyone.layer, "venti-pilots");
        assert_eq!(anyone.entry_words, "Add a place pilots take off from");
        assert_eq!(anyone.title, "A place pilots take off from");
        assert_eq!(anyone.giving, form.giving);
        assert_eq!(anyone.note, form.note);
        assert_eq!(anyone.send_words, "Add as a place from pilots");
        assert_eq!(anyone.why_not, "");

        let closed = adding(&pilot("mara", false), false);
        assert!(!closed.may_add && closed.layer.is_empty() && closed.entry_words.is_empty());
        assert_eq!(
            closed.why_not,
            "This server takes no places from pilots yet."
        );
        assert!(!closed.leads_to_sign_in && !anyone.leads_to_sign_in);
        assert!(!partner.leads_to_sign_in);

        // Signed out where the server takes places from pilots: the
        // entry is there for the three apps alike, says what it leads
        // to, and leads to signing in. Nothing of the form is given.
        let out = adding(&Actor::SignedOut, true);
        assert!(!out.may_add && out.leads_to_sign_in);
        assert_eq!(
            out.entry_words,
            "Sign in to add a place pilots take off from."
        );
        assert_eq!(out.why_not, out.entry_words);
        assert!(out.layer.is_empty() && out.title.is_empty() && out.send_words.is_empty());
        let ended = adding(&Actor::SessionEnded, true);
        assert!(!ended.may_add && ended.leads_to_sign_in);
        assert_eq!(
            ended.entry_words,
            "Sign in again to add a place pilots take off from."
        );
        assert_eq!(ended.why_not, ended.entry_words);
        // A server that takes none has no entry, whoever asks.
        for actor in [Actor::SignedOut, Actor::SessionEnded] {
            let none = adding(&actor, false);
            assert_eq!(none.why_not, closed.why_not);
            assert!(none.entry_words.is_empty() && !none.leads_to_sign_in);
        }
        let stopped = Actor::Pilot {
            handle: "demo".to_owned(),
            partner: true,
            suspended: true,
        };
        let stopped = adding(&stopped, true);
        assert!(!stopped.may_add && !stopped.leads_to_sign_in);
        assert!(stopped.entry_words.is_empty());
    }

    #[test]
    fn a_correction_takes_its_words_from_the_sites_tier() {
        let form = site_form();
        let pilots = site_correcting(pilots_site()).unwrap();
        assert!(!pilots.as_partner);
        assert_eq!(pilots.title, "Correct this site");
        assert_eq!(pilots.giving, form.giving);
        assert!(pilots.giving.contains("the places from pilots"));
        assert_eq!(pilots.note, form.note);
        assert_eq!(pilots.send_words, "Save for everyone");

        let partners = site_correcting(partners_site()).unwrap();
        assert!(partners.as_partner);
        assert_eq!(partners.title, "Correct this site");
        assert_eq!(partners.giving, form.giving_as_partner);
        assert_eq!(partners.note, form.note_as_partner);
        assert_eq!(partners.send_words, "Save for everyone");

        // A source's site is corrected at its source.
        let osm = site_page(200, golden("site_of_a_source.json"))
            .unwrap()
            .site;
        assert!(words(site_correcting(osm)).contains("corrected at its source"));
    }

    /// The history of the made-up crowdsourced site, as it stands and as
    /// it would read had its last revision removed it.
    fn histories() -> (SiteHistory, SiteHistory) {
        let stands = site_history(200, golden("site_history.json")).unwrap();
        let text = String::from_utf8(golden("site_history.json")).unwrap();
        let mut history: serde_json::Value = serde_json::from_str(&text).unwrap();
        history["removed"] = true.into();
        let last = history["revisions"].as_array().unwrap().len() - 1;
        history["revisions"][last]["kind"] = "removed".into();
        history["revisions"][last]
            .as_object_mut()
            .unwrap()
            .remove("restores");
        let removed = site_history(200, history.to_string().into_bytes()).unwrap();
        (stands, removed)
    }

    #[test]
    fn a_revisions_site_knows_its_tier_and_a_removed_site_offers_its_history_and_its_return() {
        let (stands, removed) = histories();
        assert!(!stands.is_removed && removed.is_removed);

        // Every revision's site says its tier, and that it is a view of
        // the history: no draft is made of it, and nothing is said of
        // who added the site or confirmed it.
        for (history, gone) in [(&stands, false), (&removed, true)] {
            for revision in &history.revisions {
                let by = revision.site.by_pilots.as_ref().expect("the tier");
                assert!(by.from_pilots && by.is_of_history);
                assert_eq!(by.is_removed, gone);
                assert_eq!(by.label, "From pilots");
                assert_eq!(by.note, site_form().note);
                assert_eq!(by.revision, revision.number);
                assert_eq!(by.changed_unix_s, revision.at_unix_s);
                assert_eq!(by.changed_by, revision.by);
                assert_eq!(
                    (by.added_in_words.as_str(), by.changed_in_words.as_str()),
                    ("", "")
                );
                assert!(by.added_by.is_none() && by.confirmed_in_words.is_empty());
                assert!(
                    words(crate::site_draft_of(revision.site.clone()))
                        .starts_with("This is the site as its history shows it.")
                );
            }
        }

        // A removed site, reached through its history alone: only the
        // history and bringing it back, for whoever may correct it.
        let gone = removed.revisions.last().unwrap().site.clone();
        let someone = pilot("someone", false);
        let asked = offers(&gone, &someone, true, None);
        assert_eq!(offered(&asked), "H");
        assert_eq!(asked.bring_back_words, "Bring this site back");
        assert_eq!(asked.note, "This site was removed: it is on no map.");
        let out = offers(&gone, &Actor::SignedOut, true, None);
        assert_eq!(offered(&out), "H");
        assert_eq!(out.bring_back_words, "");
        assert_eq!(
            out.note,
            "This site was removed: it is on no map. Sign in to bring this site back."
        );
        let closed = offers(&gone, &someone, false, None);
        assert_eq!(closed.bring_back_words, "");
        assert!(
            closed
                .note
                .ends_with("This server takes no changes from pilots yet.")
        );
        assert_eq!(
            offers(&gone, &pilot("demo", true), false, None).bring_back_words,
            "Bring this site back"
        );

        // The site as an earlier revision said it, of a site that
        // stands: the history, and nothing to write from there.
        let earlier = stands.revisions[0].site.clone();
        let asked = offers(&earlier, &someone, true, None);
        assert_eq!(offered(&asked), "H");
        assert_eq!(asked.bring_back_words, "");
        assert_eq!(
            asked.note,
            "This is the site as revision 1 of its history said it."
        );

        // A site that stands never offers to bring it back.
        assert_eq!(
            offers(&pilots_site(), &someone, true, None).bring_back_words,
            ""
        );
    }

    /// What a history offers, as letters: Put back, Undo, Bring back,
    /// Report a revision.
    fn history_offered(offers: &SiteHistoryOffers) -> String {
        [
            (&offers.put_back, 'P'),
            (&offers.undo, 'U'),
            (&offers.bring_back, 'B'),
            (&offers.report_revision, 'R'),
        ]
        .into_iter()
        .filter(|(offer, _)| {
            assert_eq!(offer.is_offered, !offer.words.is_empty());
            offer.is_offered
        })
        .map(|(_, letter)| letter)
        .collect()
    }

    #[test]
    fn what_a_history_offers_follows_the_tier_the_account_and_the_server() {
        let (stands, removed) = histories();
        assert!(stands.undo_revision.is_some() && removed.bring_back_revision.is_some());
        let partners = SiteHistory {
            site_id: "venti:0000000000000000000000s001".to_owned(),
            ..stands.clone()
        };
        let partners_removed = SiteHistory {
            site_id: partners.site_id.clone(),
            ..removed.clone()
        };
        let ask = |history: &SiteHistory, actor: &Actor, takes: bool| {
            let offers = history_offers(history, actor, takes);
            (history_offered(&offers), offers.note)
        };
        let (someone, demo) = (pilot("someone", false), pilot("demo", true));

        // A pilot in the history of a place from pilots.
        let (letters, note) = ask(&stands, &someone, true);
        assert_eq!((letters.as_str(), note.as_str()), ("PUR", ""));
        let all = history_offers(&stands, &someone, true);
        assert_eq!(all.put_back.words, "Put back");
        assert_eq!(all.undo.words, "Undo the last change");
        assert_eq!(all.report_revision.words, "Report this revision");
        let (letters, note) = ask(&removed, &someone, true);
        assert_eq!((letters.as_str(), note.as_str()), ("PBR", ""));
        assert_eq!(
            history_offers(&removed, &someone, true).bring_back.words,
            "Bring this site back"
        );
        // Where the server takes nothing from pilots: reporting only.
        let (letters, note) = ask(&stands, &someone, false);
        assert_eq!(letters, "R");
        assert_eq!(note, "This server takes no changes from pilots yet.");

        // A pilot in the history of a partner's site: reads and reports.
        let (letters, note) = ask(&partners, &someone, true);
        assert_eq!(letters, "R");
        assert!(note.contains("only Venti's partners change it"), "{note}");
        let (letters, note) = ask(&partners_removed, &someone, true);
        assert_eq!(letters, "R");
        assert_eq!(
            note,
            "A partner of Venti added this site, and only Venti's partners bring it back."
        );

        // A partner: both tiers, whatever the setting.
        for takes in [true, false] {
            assert_eq!(
                ask(&partners, &demo, takes),
                ("PUR".to_owned(), String::new())
            );
            assert_eq!(
                ask(&removed, &demo, takes),
                ("PBR".to_owned(), String::new())
            );
        }

        // Signed out, a session that ended, a suspended account.
        let (letters, note) = ask(&stands, &Actor::SignedOut, true);
        assert_eq!(letters, "");
        assert_eq!(note, "Sign in to put a revision back or to report one.");
        assert_eq!(
            ask(&removed, &Actor::SessionEnded, true).1,
            "Sign in again to bring this site back."
        );
        let stopped = Actor::Pilot {
            handle: "demo".to_owned(),
            partner: true,
            suspended: true,
        };
        let (letters, note) = ask(&removed, &stopped, true);
        assert_eq!(letters, "R");
        assert!(note.contains("stopped from publishing"));

        // A source's site has no history here: nothing, and no words.
        let source = SiteHistory {
            site_id: "osm-free-flying:way/1".to_owned(),
            ..stands
        };
        assert_eq!(ask(&source, &demo, true), (String::new(), String::new()));
    }

    #[test]
    fn the_core_asks_the_stored_account() {
        let folder = TempFolder::new();
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        let signed_out = core.site_adding(true).unwrap();
        assert!(!signed_out.may_add && signed_out.leads_to_sign_in);
        let (stands, _) = histories();
        assert_eq!(
            history_offered(&core.site_history_offers(stands, true).unwrap()),
            ""
        );
        assert_eq!(
            offered(&core.site_offers(pilots_site(), true, None).unwrap()),
            "H"
        );
        // The development server's demo account is a partner.
        core.signed_in(
            "http://127.0.0.1:8080".to_owned(),
            201,
            golden("sign_in.json"),
        )
        .unwrap();
        assert!(core.site_adding(false).unwrap().as_partner);
        assert_eq!(
            offered(&core.site_offers(partners_site(), true, None).unwrap()),
            "CRFPH"
        );
        let (_, removed) = histories();
        assert_eq!(
            history_offered(&core.site_history_offers(removed, false).unwrap()),
            "PBR"
        );
    }

    #[test]
    fn a_confirmation_is_sent_as_the_server_reads_it() {
        let folder = TempFolder::new();
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        let id = "venti:0000000000000000000000s001".to_owned();
        let request = core
            .site_confirm_request(
                id.clone(),
                "2026-11-18".to_owned(),
                "2026-11-21".to_owned(),
                "  South-west,\n easy launch. ".to_owned(),
                None,
            )
            .unwrap();
        assert_eq!(
            (request.method.as_str(), request.path.as_str()),
            (
                "PUT",
                "/v1/sites/venti:0000000000000000000000s001/confirmation"
            )
        );
        assert!(request.needs_token);
        // The server cleans the note as it cleans a comment; the core
        // sends what the same cleaning leaves.
        let sent: serde_json::Value =
            serde_json::from_slice(request.body.as_ref().unwrap()).unwrap();
        assert_eq!(sent["day"], "2026-11-18");
        assert_eq!(sent["note"], "South-west, easy launch.");
        assert!(sent.get("flight").is_none());

        let ask = |day: &str, note: &str, flight: Option<&str>| {
            core.site_confirm_request(
                id.clone(),
                day.to_owned(),
                "2026-11-21".to_owned(),
                note.to_owned(),
                flight.map(str::to_owned),
            )
        };
        // No note is no field, and the bounds are the form's.
        let plain = ask("2026-11-21", " ", None).unwrap();
        let sent: serde_json::Value = serde_json::from_slice(plain.body.as_ref().unwrap()).unwrap();
        assert!(sent.get("note").is_none());
        assert_eq!(
            words(ask("2026-11-22", "", None)),
            "Choose today or one of the 89 days before it."
        );
        assert!(ask("2026-08-24", "", None).is_ok());
        assert!(ask("2026-08-23", "", None).is_err());
        assert_eq!(
            words(ask("2026-11-21", &"x".repeat(201), None)),
            "This line has room for 200 characters."
        );
        assert_eq!(
            words(ask("soon", "", None)),
            "The app could not read this day."
        );
        // A flight the server does not have cannot be named.
        assert!(words(ask("2026-11-21", "", Some("nowhere"))).contains("Sync first"));
        // A source's site is not confirmed.
        assert!(
            words(core.site_confirm_request(
                "osm-free-flying:way/1".to_owned(),
                "2026-11-21".to_owned(),
                "2026-11-21".to_owned(),
                String::new(),
                None
            ))
            .contains("its source's record")
        );

        let back = site_withdraw_request(id).unwrap();
        assert_eq!(
            (back.method.as_str(), back.path.as_str(), back.body.as_ref()),
            (
                "DELETE",
                "/v1/sites/venti:0000000000000000000000s001/confirmation",
                None
            )
        );
        assert!(back.needs_token);
        assert!(site_withdraw_request("osm-free-flying:way/1".to_owned()).is_err());
    }

    #[test]
    fn the_form_offers_the_days_the_server_takes_in_words() {
        let form = site_confirm_form("2026-11-21".to_owned()).unwrap();
        assert_eq!(form.title, "I took off here and it was good");
        assert_eq!(form.days.len(), 90);
        let said: Vec<(&str, &str)> = form.days[..3]
            .iter()
            .map(|day| (day.day.as_str(), day.words.as_str()))
            .collect();
        assert_eq!(
            said,
            [
                ("2026-11-21", "Today"),
                ("2026-11-20", "Yesterday"),
                ("2026-11-19", "Thu 19 Nov")
            ]
        );
        assert_eq!(form.days[7].words, "Sat 14 Nov");
        assert_eq!(form.days[89].day, "2026-08-24");
        assert_eq!(form.note_at_most, 200);
        assert!(site_confirm_form("today".to_owned()).is_err());
    }

    #[test]
    fn the_answers_to_a_confirmation_are_said_in_words() {
        let done = site_confirmed(200, golden("site.json"), "2026-11-14".to_owned()).unwrap();
        assert_eq!(done.words, "You confirmed this site for Sat 14 Nov.");
        assert_eq!(done.site.by_pilots.unwrap().confirmed_pilots, 2);
        assert_eq!(
            site_withdrawn(204, Vec::new()).unwrap(),
            "Your confirmation of this site is taken back."
        );

        let said = |status: u16, code: &str| {
            let (status, body) = refused(status, code);
            let confirm = words(site_confirmed(
                status,
                body.clone(),
                "2026-11-14".to_owned(),
            ));
            assert_eq!(confirm, words(site_withdrawn(status, body)));
            confirm
        };
        assert!(said(400, "invalid_confirmation").contains("within 500 m of the site"));
        assert!(said(400, "invalid_confirmation").contains("one of the 90 days before it"));
        assert_eq!(
            said(403, "sites_closed"),
            "This server takes no confirmations from pilots yet."
        );
        assert!(said(429, "rate_limited").contains("as often today as one account may"));
        assert!(said(401, "unauthorized").starts_with("Sign in to say that you took off here."));
        assert!(said(403, "publishing_suspended").contains("stopped from publishing"));
        assert!(said(410, "site_gone").starts_with("This site was removed."));
        assert_eq!(said(404, "site_not_found"), "There is no such site.");
    }

    #[test]
    fn a_sites_confirmations_are_lines_in_words() {
        let list = site_confirmations(200, golden("site_confirmations.json")).unwrap();
        assert_eq!(list.site_id, "venti:0000000000000000000000s001");
        assert_eq!(list.pilots, 2);
        assert_eq!(
            list.words,
            "2 pilots took off here, last on 16 November 2026"
        );
        assert_eq!(list.nobody_words, "");
        assert!(list.is_whole);
        let lines: Vec<&str> = list.lines.iter().map(|line| line.words.as_str()).collect();
        assert_eq!(
            lines,
            [
                "Mara Muster (@mara) on Mon 16 Nov 2026: \u{201c}North-west at noon, smooth.\u{201d}",
                "Jonas Beispiel (@jonas) on Mon 9 Nov 2026"
            ]
        );
        assert_eq!(list.lines[0].day, "2026-11-16");
        assert!(!list.lines[0].with_flight);

        let with_flight = br#"{"id": "venti-pilots:0000000000000000000000s002",
            "confirmed": {"pilots": 3, "latest_day": "2026-11-14"},
            "confirmations": [{"by": {"handle": "mara", "display_name": ""},
                "day": "2026-11-14", "note": "light north wind,\teasy", "with_flight": true}]}"#;
        let list = site_confirmations(200, with_flight.to_vec()).unwrap();
        assert_eq!(
            list.lines[0].words,
            "@mara on Sat 14 Nov 2026, with a flight: \u{201c}light north wind,easy\u{201d}"
        );
        // More confirmed than the answer lists: a line says so.
        assert!(!list.is_whole);
        assert_eq!(list.not_whole_words, "The newest is shown.");
        let mut many: serde_json::Value = serde_json::from_slice(with_flight).unwrap();
        let one = many["confirmations"][0].clone();
        many["confirmations"] = vec![one; 100].into();
        many["confirmed"]["pilots"] = 250.into();
        let list = site_confirmations(200, many.to_string().into_bytes()).unwrap();
        assert!(!list.is_whole);
        assert_eq!(list.not_whole_words, "The 100 newest are shown.");
        many["confirmed"]["pilots"] = 100.into();
        let list = site_confirmations(200, many.to_string().into_bytes()).unwrap();
        assert!(list.is_whole);
        assert_eq!(list.not_whole_words, "");

        let nobody = br#"{"id": "venti:0000000000000000000000s003",
            "confirmed": {"pilots": 0}, "confirmations": []}"#;
        let list = site_confirmations(200, nobody.to_vec()).unwrap();
        assert_eq!(list.words, "");
        assert_eq!(
            list.nobody_words,
            "No pilot has said yet that they took off here."
        );
        assert!(list.is_whole);
        assert_eq!(list.not_whole_words, "");

        // Read for the site's page: said of a place to take off from,
        // and not of a landing, where nobody is asked to say so.
        let takeoff = pilots_site();
        assert_eq!(
            site_confirmations_of(takeoff.clone(), 200, nobody.to_vec())
                .unwrap()
                .nobody_words,
            "No pilot has said yet that they took off here."
        );
        for uses in [vec![SiteKind::Landing], vec![SiteKind::Other], Vec::new()] {
            let landing = SiteView {
                uses,
                ..takeoff.clone()
            };
            let list = site_confirmations_of(landing, 200, nobody.to_vec()).unwrap();
            assert_eq!(list.nobody_words, "");
            assert_eq!((list.pilots, list.is_whole), (0, true));
        }
        let both = SiteView {
            uses: vec![SiteKind::Landing, SiteKind::Training],
            ..takeoff.clone()
        };
        assert!(
            !site_confirmations_of(both, 200, nobody.to_vec())
                .unwrap()
                .nobody_words
                .is_empty()
        );
        // The lines and the count are the same either way.
        assert_eq!(
            site_confirmations_of(takeoff, 200, golden("site_confirmations.json")).unwrap(),
            site_confirmations(200, golden("site_confirmations.json")).unwrap()
        );

        assert_eq!(
            site_confirmations_path("venti-pilots:0000000000000000000000s002".to_owned()).unwrap(),
            "/v1/sites/venti-pilots:0000000000000000000000s002/confirmations"
        );
        assert!(site_confirmations_path("osm-free-flying:way/1".to_owned()).is_err());
        let (status, body) = refused(410, "site_gone");
        assert!(words(site_confirmations(status, body)).starts_with("This site was removed."));
    }

    #[test]
    fn flights_of_the_day_that_began_at_the_site_are_offered_once_the_server_has_them() {
        use venti_flight::igc;
        use venti_flight::testing::synthetic_flight;

        let folder = TempFolder::new();
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        core.signed_in(
            "http://127.0.0.1:8080".to_owned(),
            201,
            golden("sign_in.json"),
        )
        .unwrap();
        let id = core
            .import_flight(igc::write(&synthetic_flight()))
            .unwrap()
            .id;
        let (_, summary) = core
            .logbook
            .flights()
            .into_iter()
            .find(|(one, _)| *one == id)
            .unwrap();
        let [lat, lon] = summary.outline[0];
        let began = chrono::DateTime::from_timestamp(summary.start_unix_s, 0).unwrap();
        let day = began.format("%Y-%m-%d").to_string();
        let site = SiteView {
            position: Coordinate { lat, lon },
            ..pilots_site()
        };

        // Not on the server yet: nothing to name.
        let offered = |site: &SiteView, day: &str, offset: i32| {
            core.site_confirm_flights(site.clone(), day.to_owned(), offset)
                .unwrap()
        };
        assert_eq!(offered(&site, &day, 0), []);
        assert!(
            words(core.site_confirm_request(
                site.id.clone(),
                day.clone(),
                day.clone(),
                String::new(),
                Some(id.clone())
            ))
            .contains("not on the server yet")
        );

        // Once the server has it, it is offered for its day at its place.
        let path = folder.path().join("account.json");
        let mut stored: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        stored["flights"][&id] =
            serde_json::json!({"id": "0000000000000000000000f001", "visibility": "private"});
        std::fs::write(&path, stored.to_string()).unwrap();
        let flights = offered(&site, &day, 0);
        assert_eq!(flights.len(), 1);
        assert_eq!(flights[0].id, id);
        assert!(
            flights[0]
                .words
                .starts_with(&format!("Your flight from {}", began.format("%H:%M"))),
            "{}",
            flights[0].words
        );
        // Another day, by the pilot's own clock too, and another place.
        assert_eq!(offered(&site, "2020-01-01", 0), []);
        assert_eq!(offered(&site, &day, 24 * 3600), []);
        let away = SiteView {
            position: Coordinate {
                lat: lat + 0.01,
                lon,
            },
            ..site.clone()
        };
        assert_eq!(offered(&away, &day, 0), []);

        // And the request names its address on the server.
        let request = core
            .site_confirm_request(site.id.clone(), day.clone(), day, String::new(), Some(id))
            .unwrap();
        let sent: serde_json::Value =
            serde_json::from_slice(request.body.as_ref().unwrap()).unwrap();
        assert_eq!(sent["flight"], "0000000000000000000000f001");
    }

    #[test]
    fn removed_sites_are_lines_from_one_answer_with_who_may_bring_each_back() {
        assert_eq!(
            sites_removed_path(false, None).unwrap(),
            "/v1/sites/changes?layer=venti"
        );
        assert_eq!(
            sites_removed_path(true, Some("17".to_owned())).unwrap(),
            "/v1/sites/changes?layer=venti-pilots&since=17"
        );
        assert!(sites_removed_path(true, Some("1&layer=x".to_owned())).is_err());

        let read = |body: &[u8], actor: &Actor, takes: bool| {
            removed(&serde_json::from_slice(body).unwrap(), actor, takes)
        };
        let someone = pilot("someone", false);
        let page = read(&golden("site_changes.json"), &someone, true);
        assert!(!page.cursor.is_empty());
        assert!(page.ids.is_empty() && page.lines.is_empty());

        // A place a pilot removed, one whose remover's account is gone,
        // and one that whoever runs Venti took down, which has no name.
        let gone = br#"{"sites": [],
            "removed": ["venti-pilots:0000000000000000000000s004",
                        "venti-pilots:0000000000000000000000s005",
                        "venti-pilots:0000000000000000000000s006"],
            "removals": [
              {"id": "venti-pilots:0000000000000000000000s004", "name": "Beispieljoch",
               "at": "2026-11-21T08:30:00Z",
               "by": {"handle": "jonas", "display_name": "Jonas Beispiel"}},
              {"id": "venti-pilots:0000000000000000000000s005", "name": " Muster\tkopf ",
               "at": "2026-11-20T23:59:59Z"},
              {"id": "venti-pilots:0000000000000000000000s006", "at": "2026-11-19T12:00:00Z"}],
            "cursor": "9", "more": false, "reset": false, "attributions": []}"#;
        let page = read(gone, &someone, true);
        assert_eq!(page.ids.len(), 3);
        assert_eq!(
            (page.cursor.as_str(), page.more, page.reset),
            ("9", false, false)
        );
        let said: Vec<&str> = page.lines.iter().map(|line| line.words.as_str()).collect();
        assert_eq!(
            said,
            [
                "Beispieljoch: removed by Jonas Beispiel (@jonas) on 21 Nov 2026",
                "Musterkopf: removed on 20 Nov 2026",
                "A site that whoever runs Venti took down on 19 Nov 2026"
            ]
        );
        let first = &page.lines[0];
        assert_eq!(first.id, "venti-pilots:0000000000000000000000s004");
        assert_eq!(first.name, "Beispieljoch");
        assert_eq!(first.by.as_ref().unwrap().handle, "jonas");
        assert_eq!(first.at_unix_s, 1_795_249_800);
        assert!(first.from_pilots);
        assert_eq!(page.lines[1].name, "Musterkopf");
        assert_eq!(page.lines[2].name, "A site that was taken down");
        // Who may bring each back: whoever may correct a place of the
        // tier, and nobody a site that was taken down.
        let may = |page: &SitesRemoved| -> Vec<bool> {
            page.lines
                .iter()
                .map(|line| {
                    assert_eq!(
                        line.bring_back_words,
                        if line.may_bring_back {
                            "Bring this site back"
                        } else {
                            ""
                        }
                    );
                    line.may_bring_back
                })
                .collect()
        };
        assert_eq!(may(&page), [true, true, false]);
        assert_eq!(may(&read(gone, &someone, false)), [false, false, false]);
        assert_eq!(may(&read(gone, &Actor::SignedOut, true)), [false; 3]);
        assert_eq!(
            may(&read(gone, &pilot("demo", true), false)),
            [true, true, false]
        );

        // A partner's site that was removed: a partner brings it back.
        let partners = String::from_utf8(gone.to_vec())
            .unwrap()
            .replace("venti-pilots:", "venti:");
        let page = read(partners.as_bytes(), &someone, true);
        assert!(!page.lines[0].from_pilots);
        assert_eq!(may(&page), [false; 3]);
        assert_eq!(
            may(&read(partners.as_bytes(), &pilot("demo", true), true)),
            [true, true, false]
        );

        // A server from before it named them: the identifiers alone.
        let unnamed = br#"{"sites": [], "removed": ["venti-pilots:0000000000000000000000s004"],
            "cursor": "9", "more": false, "reset": false, "attributions": []}"#;
        let page = read(unnamed, &someone, true);
        assert_eq!(page.ids, ["venti-pilots:0000000000000000000000s004"]);
        assert!(page.lines.is_empty());

        // The core reads it for the account this device stores.
        let folder = TempFolder::new();
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        let page = core.sites_removed(200, gone.to_vec(), true).unwrap();
        assert_eq!(may(&page), [false; 3]);
        let (status, body) = refused(404, "not_found");
        assert!(core.sites_removed(status, body, true).is_err());
    }
}
