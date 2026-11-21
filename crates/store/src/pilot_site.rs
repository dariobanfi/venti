//! Flying sites that pilots make: the community's own layer, beside the
//! whole sets of the sources ([`crate::site`]).
//!
//! A site of a source is a row that goes when its set is replaced. A site
//! a pilot made is a record of its own:
//!
//! - **It has an identifier of the server's making that is never used for
//!   another site.** No call removes a row: a site that is removed is
//!   marked as gone and stays.
//! - **Every change is a revision that is kept**, with the whole record as
//!   it stood from then on, who made the change, when, through what it
//!   came in, and a line saying why. Nothing is overwritten. Putting a
//!   site back is a revision that names the one it restores; removing a
//!   site is a revision too.
//! - **It outlives its author.** When an account is deleted, its sites and
//!   their revisions stay and lose their author.
//! - **Every write takes the next change number of the layer**, one count
//!   for all sites, so that a program can ask what changed since it last
//!   asked ([`Store::pilot_site_changes`](crate::Store::pilot_site_changes)).
//!
//! What both stores decide in the same way is decided here, in [`decide`]
//! and [`nearest_within`]; a store reads, asks, and writes what it is told.

use chrono::NaiveDate;
use venti_domain::{Site, SiteId, Timestamp};

use crate::StoreError;

/// A pilot as a site's history names them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitePilot {
    /// The account's identifier inside the server.
    pub account_id: String,
    /// Its handle.
    pub handle: String,
    /// The name the pilot is shown by.
    pub display_name: String,
}

/// Through what a revision came in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SiteVia {
    /// A session of the pilot's own sign-in.
    Session,
    /// Whoever runs the server, from its command line.
    Operator,
    /// A token the pilot gave to another program, by the name the pilot
    /// gave the token. The name is kept as it was at the write: it stays
    /// when the token is renamed, withdrawn or gone with its account.
    Token(String),
}

impl SiteVia {
    /// The word the database keeps.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Operator => "operator",
            Self::Token(_) => "token",
        }
    }

    /// The name of the token a revision came through, if it came through
    /// one.
    pub fn token(&self) -> Option<&str> {
        match self {
            Self::Token(name) => Some(name),
            Self::Session | Self::Operator => None,
        }
    }

    /// The way a word of the database names, with the token's name where
    /// the word is `token`.
    pub fn from_parts(name: &str, token: Option<String>) -> Option<Self> {
        match (name, token) {
            ("session", None) => Some(Self::Session),
            ("operator", None) => Some(Self::Operator),
            ("token", Some(token)) => Some(Self::Token(token)),
            _ => None,
        }
    }
}

/// What a revision did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevisionKind {
    /// It made the site.
    Created,
    /// It changed what the site says.
    Changed,
    /// It put the site back to what the revision with this number said.
    Restored(u32),
    /// It removed the site.
    Removed,
}

impl RevisionKind {
    /// The word the database keeps.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Changed => "changed",
            Self::Restored(_) => "restored",
            Self::Removed => "removed",
        }
    }

    /// The revision this one restored, if it restored one.
    pub fn restores(self) -> Option<u32> {
        match self {
            Self::Restored(number) => Some(number),
            _ => None,
        }
    }

    /// The kind a word of the database names, with the revision it
    /// restored where it is that kind.
    pub fn from_parts(name: &str, restores: Option<u32>) -> Option<Self> {
        match (name, restores) {
            ("created", None) => Some(Self::Created),
            ("changed", None) => Some(Self::Changed),
            ("restored", Some(number)) => Some(Self::Restored(number)),
            ("removed", None) => Some(Self::Removed),
            _ => None,
        }
    }
}

/// One revision of a site.
#[derive(Debug, Clone, PartialEq)]
pub struct SiteRevision {
    /// Its number: 1 for the first, one more for each later one.
    pub number: u32,
    /// When it was made.
    pub at: Timestamp,
    /// Who made it, as the account is now. `None` for a revision of
    /// whoever runs the server, and once the account is gone.
    pub author: Option<SitePilot>,
    /// Through what it came in.
    pub via: SiteVia,
    /// What it did.
    pub kind: RevisionKind,
    /// Why, in its author's words.
    pub comment: Option<String>,
    /// The record as it stood from this revision on; for one that removed
    /// the site, as it stood until then.
    pub record: Site,
}

/// A site that pilots made, as it stands.
#[derive(Debug, Clone, PartialEq)]
pub struct PilotSite {
    /// The record. For a removed site, as it last stood.
    pub site: Site,
    /// The number of the revision it stands at.
    pub revision: u32,
    /// When it was made.
    pub created_at: Timestamp,
    /// Who made it, as the account is now. `None` once the account is
    /// gone.
    pub created_by: Option<SitePilot>,
    /// When the revision it stands at was made.
    pub changed_at: Timestamp,
    /// Who made that revision; `None` as for [`SiteRevision::author`].
    pub changed_by: Option<SitePilot>,
    /// Since when the site is removed, if it is.
    pub removed_at: Option<Timestamp>,
    /// Whether whoever runs the server took the site down. A site that is
    /// taken down is removed, and only whoever runs the server brings it
    /// back.
    pub taken_down: bool,
    /// The change number of the layer that the site's latest revision
    /// took.
    pub change: i64,
    /// How many pilots say they took off here, and the newest day one of
    /// them speaks of.
    pub confirmed: SiteConfirmed,
}

impl PilotSite {
    /// Whether the site stands: it is not removed.
    pub fn stands(&self) -> bool {
        self.removed_at.is_none()
    }
}

/// A site to add.
#[derive(Debug, Clone, PartialEq)]
pub struct NewPilotSite {
    /// The record, under its new identifier.
    pub site: Site,
    /// The account that adds it.
    pub author_id: String,
    /// When.
    pub at: Timestamp,
    /// Through what it comes in.
    pub via: SiteVia,
    /// Why, or from what knowledge.
    pub comment: Option<String>,
}

/// What a revision is to do.
#[derive(Debug, Clone, PartialEq)]
pub enum SiteAct {
    /// Replace the record by this one, which carries the site's
    /// identifier.
    Change(Box<Site>),
    /// Put the site back to what the revision with this number said.
    Restore(u32),
    /// Remove the site; with `take_down`, so that only whoever runs the
    /// server brings it back.
    Remove {
        /// Whether the site is taken down.
        take_down: bool,
    },
}

/// A revision to write.
#[derive(Debug, Clone, PartialEq)]
pub struct SiteRevising {
    /// The site.
    pub id: SiteId,
    /// The account that makes the revision; `None` for whoever runs the
    /// server.
    pub author_id: Option<String>,
    /// When.
    pub at: Timestamp,
    /// Through what it comes in.
    pub via: SiteVia,
    /// What it does.
    pub act: SiteAct,
    /// Why.
    pub comment: Option<String>,
    /// The revision the change was made from, if its maker says: when the
    /// site stands at another one, nothing is written.
    pub expects: Option<u32>,
}

/// What became of an attempt to add a site or to write a revision.
#[derive(Debug, Clone, PartialEq)]
pub enum SiteWrite {
    /// It is written. This is the site as it now is.
    Written(Box<PilotSite>),
    /// Nothing was written, because nothing would change: the site says
    /// already what the revision would make it say. This is the site as
    /// it is.
    Same(Box<PilotSite>),
    /// Another standing site of the layer is nearer to the place than
    /// sites may be to each other. Nothing was written.
    TooClose(SiteId),
    /// The site stands at another revision than the one the change was
    /// made from. Nothing was written.
    Stale {
        /// The revision the site stands at.
        current: u32,
    },
    /// The site is removed: there is nothing to change or to remove.
    /// Restoring a revision brings it back.
    Removed,
    /// The site is taken down, and the revision is not one of whoever
    /// runs the server.
    TakenDown,
    /// The site has no revision with the number to restore.
    NoSuchRevision,
    /// There is no site with this identifier.
    NoSuchSite,
    /// The account that writes does not exist.
    NoSuchAccount,
}

/// One page of what changed in the layer.
#[derive(Debug, Clone, PartialEq)]
pub struct PilotSiteChanges {
    /// The sites whose latest revision took a change number after the one
    /// asked from, by change number, each as it is now: standing or
    /// removed.
    pub sites: Vec<PilotSite>,
    /// Whether changes beyond the limit of the request are waiting.
    pub more: bool,
    /// The largest change number the layer has used, zero if it has used
    /// none.
    pub latest: i64,
}

/// The layer of sites whose writers whoever runs the server names: the
/// partners' sites. Every other layer of the table is one that any pilot
/// writes to. The store knows the name for one thing, which site stands
/// in the way of which ([`in_the_way`]); who may write where is the
/// server's to decide.
pub const PARTNERS_LAYER: &str = "venti";

/// How many pilots confirmed a site, and when last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SiteConfirmed {
    /// How many accounts have a confirmation of the site.
    pub pilots: u32,
    /// The newest day one of them speaks of. `None` when nobody confirmed.
    pub latest_day: Option<NaiveDate>,
}

/// One pilot's word that they took off at a site and that it was good.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteConfirmation {
    /// Who says so.
    pub pilot: SitePilot,
    /// The day they speak of.
    pub day: NaiveDate,
    /// What they add, in one line.
    pub note: Option<String>,
    /// Whether the server found a flight of theirs that took off there.
    pub with_flight: bool,
    /// When it was said, or last said again.
    pub at: Timestamp,
}

/// What one account gave to the sites that pilots make and said to
/// whoever runs the server: what no other call lists by account. See
/// [`Store::given_by`](crate::Store::given_by).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Given {
    /// Every revision the account wrote, by site and then by number. A
    /// revision names its site in its record. The first revision of a
    /// site is the one that made it.
    pub revisions: Vec<SiteRevision>,
    /// Every confirmation the account has, by site.
    pub confirmations: Vec<(SiteId, SiteConfirmation)>,
    /// Every report the account sent that is still kept, oldest first.
    pub reports: Vec<crate::Report>,
}

/// A confirmation to keep: the one of this account for this site, in the
/// place of the one it had.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSiteConfirmation {
    /// The site.
    pub site: SiteId,
    /// Whose word it is.
    pub account_id: String,
    /// The day it speaks of.
    pub day: NaiveDate,
    /// What the pilot adds. The caller has cleaned it to one line.
    pub note: Option<String>,
    /// Whether the caller found a flight of the pilot's that took off
    /// there.
    pub with_flight: bool,
    /// Now.
    pub at: Timestamp,
}

/// What became of a confirmation.
#[derive(Debug, Clone, PartialEq)]
pub enum SiteConfirming {
    /// It is kept; the site as it stands with it.
    Confirmed(Box<PilotSite>),
    /// The site is removed or taken down: nobody takes off there.
    Removed,
    /// No site ever had the identifier.
    NoSuchSite,
    /// The account is gone.
    NoSuchAccount,
}

/// Whether a standing site stands in the way of a site that is written
/// at less than the distance from it.
///
/// A partner's site gives way to partners' sites only: what pilots put on
/// the map does not keep an official site off it. A site of any other
/// layer gives way to every site, a partner's included: where a site
/// stands already, a second one of the same place helps nobody.
pub(crate) fn in_the_way(written: &SiteId, standing: &SiteId) -> bool {
    written.source().as_str() != PARTNERS_LAYER || standing.source().as_str() == PARTNERS_LAYER
}

/// Checks a confirmation before it is kept: no text of it holds a zero
/// byte, which the database cannot keep.
pub(crate) fn check_confirmation(new: &NewSiteConfirmation) -> Result<(), StoreError> {
    let texts = [
        new.site.local(),
        &new.account_id,
        new.note.as_deref().unwrap_or_default(),
    ];
    if texts.iter().any(|text| text.contains('\0')) {
        return Err(StoreError::Invalid(
            "a confirmation with a zero byte in one of its texts".to_owned(),
        ));
    }
    Ok(())
}

/// Refuses what no store keeps: text with a zero byte in it, which the
/// database does not take, and a record under another site's identifier.
/// The API lets neither through.
pub(crate) fn check(id: &SiteId, record: Option<&Site>, texts: &[&str]) -> Result<(), StoreError> {
    let mut all: Vec<&str> = texts.to_vec();
    all.push(id.local());
    if let Some(site) = record {
        if &site.id != id {
            return Err(StoreError::Invalid(format!(
                "a record of site {} handed over for site {id}",
                site.id
            )));
        }
        all.extend(
            [
                &site.name,
                &site.url,
                &site.website,
                &site.description,
                &site.fee,
                &site.no_flight_time,
            ]
            .into_iter()
            .filter_map(|text| text.as_deref()),
        );
    }
    if all.iter().any(|text| text.contains('\0')) {
        return Err(StoreError::Invalid(
            "a site with a zero byte in one of its texts".to_owned(),
        ));
    }
    Ok(())
}

/// Refuses a site to add that no store keeps.
pub(crate) fn check_new(new: &NewPilotSite) -> Result<(), StoreError> {
    check(
        &new.site.id,
        Some(&new.site),
        &[&new.author_id, new.comment.as_deref().unwrap_or_default()],
    )
}

/// Refuses a revision no store keeps.
pub(crate) fn check_revising(revising: &SiteRevising) -> Result<(), StoreError> {
    let record = match &revising.act {
        SiteAct::Change(site) => Some(&**site),
        SiteAct::Restore(_) | SiteAct::Remove { .. } => None,
    };
    check(
        &revising.id,
        record,
        &[
            revising.author_id.as_deref().unwrap_or_default(),
            revising.comment.as_deref().unwrap_or_default(),
        ],
    )
}

/// What a store is to write for a revision.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Writing {
    /// The record the site has from now on.
    pub(crate) record: Site,
    /// What the revision did.
    pub(crate) kind: RevisionKind,
    /// Whether the site is removed from now on.
    pub(crate) removed: bool,
    /// Whether it is taken down from now on.
    pub(crate) taken_down: bool,
}

/// What a revision comes to, before anything is written.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Decision {
    /// Write this, if the place is free where the site then stands.
    Write(Box<Writing>),
    /// Write nothing and answer this.
    Answer(SiteWrite),
}

/// Decides what a revision comes to for a site as it is.
///
/// `restored` is the record of the revision that [`SiteAct::Restore`]
/// names, if the site has one with that number; it is not looked at for
/// the other acts. Both stores decide here, so that they decide alike.
pub(crate) fn decide(
    current: &PilotSite,
    revising: &SiteRevising,
    restored: Option<&Site>,
) -> Decision {
    let answer = |answer: SiteWrite| Decision::Answer(answer);
    let operator = revising.via == SiteVia::Operator;
    if current.taken_down && !operator {
        return answer(SiteWrite::TakenDown);
    }
    if revising
        .expects
        .is_some_and(|expected| expected != current.revision)
    {
        return answer(SiteWrite::Stale {
            current: current.revision,
        });
    }
    match &revising.act {
        SiteAct::Change(record) => {
            if !current.stands() {
                answer(SiteWrite::Removed)
            } else if **record == current.site {
                answer(SiteWrite::Same(Box::new(current.clone())))
            } else {
                Decision::Write(Box::new(Writing {
                    record: (**record).clone(),
                    kind: RevisionKind::Changed,
                    removed: false,
                    taken_down: false,
                }))
            }
        }
        SiteAct::Restore(number) => match restored {
            None => answer(SiteWrite::NoSuchRevision),
            Some(record) if current.stands() && *record == current.site => {
                answer(SiteWrite::Same(Box::new(current.clone())))
            }
            Some(record) => Decision::Write(Box::new(Writing {
                record: record.clone(),
                kind: RevisionKind::Restored(*number),
                removed: false,
                taken_down: false,
            })),
        },
        SiteAct::Remove { take_down } => {
            // A removed site is removed once more only to take it down.
            if !current.stands() && (!take_down || current.taken_down) {
                answer(SiteWrite::Removed)
            } else {
                Decision::Write(Box::new(Writing {
                    record: current.site.clone(),
                    kind: RevisionKind::Removed,
                    removed: true,
                    taken_down: *take_down,
                }))
            }
        }
    }
}

/// Whether a revision puts the site at a place it does not stand at
/// already, so that the place has to be free: one that moves it, and one
/// that brings a removed site back.
///
/// A revision that leaves a standing site where it is takes no place. It
/// is written whatever stands near by: a partner's site may have been put
/// beside a place from pilots since, and that place must stay correctable.
pub(crate) fn takes_a_place(current: &PilotSite, writing: &Writing) -> bool {
    !writing.removed && (!current.stands() || current.site.position != writing.record.position)
}

/// The site among `standing` that is nearer to `record` than `apart_m`
/// metres and is not the record's own: the nearest of them, and by
/// identifier among those equally near. `None` if the place is free.
pub(crate) fn nearest_within<'a>(
    record: &Site,
    standing: impl IntoIterator<Item = &'a Site>,
    apart_m: f64,
) -> Option<SiteId> {
    standing
        .into_iter()
        .filter(|other| other.id != record.id && in_the_way(&record.id, &other.id))
        .map(|other| (record.position.distance_m(other.position), &other.id))
        .filter(|(distance, _)| *distance < apart_m)
        .min_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(b.1)))
        .map(|(_, id)| id.clone())
}

#[cfg(test)]
mod tests {
    use venti_domain::{GeoPoint, SiteUse};

    use super::*;

    fn record(id: &str, lat: f64, lon: f64) -> Site {
        Site {
            id: id.parse().unwrap(),
            name: Some("Musterberg".to_owned()),
            position: GeoPoint::new(lat, lon).unwrap(),
            elevation_m: None,
            uses: vec![SiteUse::Takeoff],
            wind_directions: Vec::new(),
            paragliding: None,
            hang_gliding: None,
            official: None,
            discouraged: false,
            url: None,
            website: None,
            description: None,
            fee: None,
            guest_guidelines: None,
            no_flight_time: None,
            outline: None,
        }
    }

    /// What a decision is to write, if it is to write.
    fn written(decision: Decision) -> Option<Writing> {
        match decision {
            Decision::Write(writing) => Some(*writing),
            Decision::Answer(_) => None,
        }
    }

    fn standing(revision: u32) -> PilotSite {
        let at = chrono::DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        PilotSite {
            site: record("venti:a", 47.0, 11.0),
            revision,
            created_at: at,
            created_by: None,
            changed_at: at,
            changed_by: None,
            removed_at: None,
            taken_down: false,
            change: 1,
            confirmed: SiteConfirmed::default(),
        }
    }

    fn revising(act: SiteAct, via: SiteVia) -> SiteRevising {
        SiteRevising {
            id: "venti:a".parse().unwrap(),
            author_id: None,
            at: chrono::DateTime::from_timestamp(1_800_000_100, 0).unwrap(),
            via,
            act,
            comment: None,
            expects: None,
        }
    }

    #[test]
    fn a_change_is_written_unless_it_changes_nothing_or_was_made_from_an_older_revision() {
        let site = standing(3);
        let moved = record("venti:a", 47.001, 11.0);
        let change = revising(SiteAct::Change(Box::new(moved.clone())), SiteVia::Session);
        assert!(matches!(
            written(decide(&site, &change, None)),
            Some(Writing {
                kind: RevisionKind::Changed,
                removed: false,
                ..
            })
        ));
        let same = revising(
            SiteAct::Change(Box::new(site.site.clone())),
            SiteVia::Session,
        );
        assert_eq!(
            decide(&site, &same, None),
            Decision::Answer(SiteWrite::Same(Box::new(site.clone())))
        );
        let stale = SiteRevising {
            expects: Some(2),
            ..change
        };
        assert_eq!(
            decide(&site, &stale, None),
            Decision::Answer(SiteWrite::Stale { current: 3 })
        );
    }

    #[test]
    fn a_removed_site_comes_back_by_a_restore_and_a_taken_down_one_only_by_the_operator() {
        let removed = PilotSite {
            removed_at: Some(standing(1).changed_at),
            ..standing(2)
        };
        let earlier = removed.site.clone();
        let change = revising(SiteAct::Change(Box::new(earlier.clone())), SiteVia::Session);
        assert_eq!(
            decide(&removed, &change, None),
            Decision::Answer(SiteWrite::Removed)
        );
        let again = revising(SiteAct::Remove { take_down: false }, SiteVia::Session);
        assert_eq!(
            decide(&removed, &again, None),
            Decision::Answer(SiteWrite::Removed)
        );
        let restore = revising(SiteAct::Restore(1), SiteVia::Session);
        assert!(matches!(
            written(decide(&removed, &restore, Some(&earlier))),
            Some(Writing {
                kind: RevisionKind::Restored(1),
                removed: false,
                ..
            })
        ));
        assert_eq!(
            decide(&removed, &restore, None),
            Decision::Answer(SiteWrite::NoSuchRevision)
        );

        // Whoever runs the server takes a removed site down, once.
        let take_down = revising(SiteAct::Remove { take_down: true }, SiteVia::Operator);
        assert!(matches!(
            written(decide(&removed, &take_down, None)),
            Some(Writing {
                removed: true,
                taken_down: true,
                ..
            })
        ));
        let down = PilotSite {
            taken_down: true,
            ..removed
        };
        assert_eq!(
            decide(&down, &take_down, None),
            Decision::Answer(SiteWrite::Removed)
        );
        assert_eq!(
            decide(&down, &restore, Some(&earlier)),
            Decision::Answer(SiteWrite::TakenDown)
        );
        let by_operator = revising(SiteAct::Restore(1), SiteVia::Operator);
        assert!(matches!(
            written(decide(&down, &by_operator, Some(&earlier))),
            Some(Writing {
                removed: false,
                taken_down: false,
                ..
            })
        ));
    }

    #[test]
    fn the_nearest_other_site_within_the_distance_is_found() {
        let here = record("venti:a", 47.0, 11.0);
        // A hundredth of a second of arc of latitude is about 0.3 m; a
        // ten-thousandth of a degree about 11 m.
        let near = record("venti:b", 47.0001, 11.0);
        let nearer = record("venti:c", 47.00005, 11.0);
        let far = record("venti:d", 47.001, 11.0);
        let itself = here.clone();
        let all = [&near, &nearer, &far, &itself];
        assert_eq!(
            nearest_within(&here, all, 20.0),
            Some("venti:c".parse().unwrap())
        );
        assert_eq!(nearest_within(&here, [&far, &itself], 20.0), None);
        assert_eq!(nearest_within(&here, [&near], 5.0), None);
    }

    #[test]
    fn a_record_under_another_identifier_and_a_zero_byte_are_refused() {
        let at = chrono::DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let new = NewPilotSite {
            site: record("venti:a", 47.0, 11.0),
            author_id: "x".to_owned(),
            at,
            via: SiteVia::Session,
            comment: Some("a\0".to_owned()),
        };
        assert!(matches!(check_new(&new), Err(StoreError::Invalid(_))));
        let other = revising(
            SiteAct::Change(Box::new(record("venti:b", 47.0, 11.0))),
            SiteVia::Session,
        );
        assert!(matches!(
            check_revising(&other),
            Err(StoreError::Invalid(_))
        ));
    }
}
