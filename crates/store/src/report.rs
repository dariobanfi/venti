//! Reports: what a pilot tells whoever runs the server about a flight, a
//! pilot's page, a live track, a group, a plan or a site that pilots made
//! that should not be there, or not as it is.
//!
//! A report is kept for the operator and for nobody else. Nothing reads it
//! back to a pilot: not to the one who wrote it, and not to the one it is
//! about, who must never learn from the server who reported them. It names
//! two accounts and goes when either of them goes. A report about a site
//! names one, its reporter: a site that pilots made is nobody's, and it
//! outlives the account that added it.
//!
//! A flight, a page and a live track are named by a report and looked at
//! where they are. A group's name and a plan's words can be changed or be
//! gone a moment after they were reported, so a report about either keeps
//! the words as they stood, for as long as the report is kept and no
//! longer.

use venti_domain::Timestamp;

use crate::StoreError;

/// Refuses a report no store keeps: one with a zero byte in any of its
/// texts, which the database does not take. The API lets none through.
pub(crate) fn check(new: &NewReport) -> Result<(), StoreError> {
    let (group, words) = match &new.subject {
        ReportSubject::Flight(_)
        | ReportSubject::Pilot
        | ReportSubject::Picture(_)
        | ReportSubject::LiveTrack(_)
        | ReportSubject::Site { .. } => ("", ["", ""]),
        ReportSubject::Group { name, .. } => ("", [name.as_str(), ""]),
        ReportSubject::Comment {
            flight_id, text, ..
        } => (flight_id.as_str(), [text.as_str(), ""]),
        ReportSubject::Plan {
            group_id,
            place,
            note,
            ..
        } => (
            group_id.as_str(),
            [place.as_str(), note.as_deref().unwrap_or_default()],
        ),
        ReportSubject::OpenPlan { place, note, .. } => {
            ("", [place.as_str(), note.as_deref().unwrap_or_default()])
        }
    };
    let texts = [
        new.id.as_str(),
        &new.reporter_id,
        new.about_account_id.as_deref().unwrap_or_default(),
        new.subject.id(),
        group,
        words[0],
        words[1],
        &new.reason,
        new.text.as_deref().unwrap_or_default(),
    ];
    if texts.iter().any(|text| text.contains('\0')) {
        return Err(StoreError::Invalid(
            "a report with a zero byte in it".to_owned(),
        ));
    }
    // A site is nobody's, and everything else is somebody's.
    if matches!(new.subject, ReportSubject::Site { .. }) == new.about_account_id.is_some() {
        return Err(StoreError::Invalid(
            "a report about a site names no second account, and every other report names one"
                .to_owned(),
        ));
    }
    if matches!(new.subject, ReportSubject::Site { revision: 0, .. }) {
        return Err(StoreError::Invalid(
            "a report about revision 0 of a site; the first is 1".to_owned(),
        ));
    }
    Ok(())
}

/// Whether a report is about the same thing, from the same reporter, as
/// another: what an account has one open report of.
///
/// A flight, a page, a live track and a plan are their account's for good.
/// A group passes from keeper to keeper and is the same group all the
/// while, so for a group the account the report is about does not count.
pub(crate) fn is_the_same_report(one: &NewReport, other: &NewReport) -> bool {
    one.reporter_id == other.reporter_id
        && one.subject.is_the_thing(&other.subject)
        && (matches!(one.subject, ReportSubject::Group { .. })
            || one.about_account_id == other.about_account_id)
}

/// Refuses a closing note no store keeps, for the same reason.
pub(crate) fn check_note(note: &str) -> Result<(), StoreError> {
    if note.contains('\0') {
        return Err(StoreError::Invalid(
            "a closing note with a zero byte in it".to_owned(),
        ));
    }
    Ok(())
}

/// What a report is about.
///
/// A group and a plan carry the words that were reported, as they stood
/// when the report was made: a name, a place and a note can be changed or
/// deleted afterwards, and whoever runs the server must still be able to
/// read what the report was about. Nothing else of a group or a plan is
/// kept with a report: not who was in the group, and of a plan neither its
/// day, its time, its position nor who was in on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReportSubject {
    /// A flight, by its address.
    Flight(String),
    /// A pilot's page.
    Pilot,
    /// A live track, by its identifier.
    LiveTrack(String),
    /// A group. The report's other account is the one that kept the group
    /// when it was reported.
    Group {
        /// The group's identifier.
        id: String,
        /// Its name as it stood when the report was made.
        name: String,
    },
    /// A plan of a group. The report's other account is the plan's author.
    Plan {
        /// The plan's identifier.
        id: String,
        /// The identifier of the group it was posted to.
        group_id: String,
        /// Its place as it stood when the report was made.
        place: String,
        /// Its note as it stood then, if it had one.
        note: Option<String>,
    },
    /// An open plan. The report's other account is the plan's author.
    OpenPlan {
        /// The plan's identifier.
        id: String,
        /// Its place as it stood when the report was made.
        place: String,
        /// Its note as it stood then, if it had one.
        note: Option<String>,
    },
    /// A site that pilots made. The report has no other account: such a
    /// site is nobody's. Nothing of what the site says is kept with the
    /// report: every revision of a site is kept where the sites are.
    Site {
        /// The site's identifier, as text.
        id: String,
        /// The number of the revision the report is about: the one its
        /// reporter named, or the one the site stood at.
        revision: u32,
    },
    /// A pilot's picture, by its identifier: the picture the pilot showed
    /// when the report was made. The report's other account is the
    /// pilot's. Nothing of the picture is kept with the report: once the
    /// pilot has another, the one reported is gone.
    Picture(String),
    /// A comment on a flight. The report's other account is the
    /// comment's author.
    Comment {
        /// The comment's identifier.
        id: String,
        /// The flight it was written under.
        flight_id: String,
        /// Its words as they stood when the report was made.
        text: String,
    },
}

/// What a report kept of a group or a plan, as the database holds it
/// beside the kind of subject and its identifier.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ReportedWords {
    /// The group a reported plan was posted to.
    pub(crate) group_id: Option<String>,
    /// A reported group's name.
    pub(crate) name: Option<String>,
    /// A reported plan's place.
    pub(crate) place: Option<String>,
    /// A reported plan's note.
    pub(crate) note: Option<String>,
    /// The revision a report about a site is about.
    pub(crate) site_revision: Option<u32>,
    /// The flight a reported comment was written under.
    pub(crate) flight_id: Option<String>,
    /// A reported comment's words.
    pub(crate) comment: Option<String>,
}

impl ReportSubject {
    /// The name the database knows the kind of subject by.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Flight(_) => "flight",
            Self::Pilot => "pilot",
            Self::Picture(_) => "picture",
            Self::LiveTrack(_) => "live_track",
            Self::Group { .. } => "group",
            Self::Plan { .. } => "plan",
            Self::OpenPlan { .. } => "open_plan",
            Self::Site { .. } => "site",
            Self::Comment { .. } => "comment",
        }
    }

    /// The address or identifier that names the subject beside its
    /// account; empty for a pilot's page, which the account names.
    pub fn id(&self) -> &str {
        match self {
            Self::Flight(id)
            | Self::Picture(id)
            | Self::LiveTrack(id)
            | Self::Group { id, .. }
            | Self::Plan { id, .. }
            | Self::OpenPlan { id, .. }
            | Self::Site { id, .. }
            | Self::Comment { id, .. } => id,
            Self::Pilot => "",
        }
    }

    /// Whether two subjects are the same thing, whatever words each was
    /// reported with: a group that was renamed is still that group, and a
    /// site is that site at every revision.
    pub fn is_the_thing(&self, other: &Self) -> bool {
        self.kind() == other.kind() && self.id() == other.id()
    }

    /// What the subject keeps of a group or a plan, as the database holds
    /// it.
    pub(crate) fn words(&self) -> ReportedWords {
        match self {
            Self::Flight(_) | Self::Pilot | Self::Picture(_) | Self::LiveTrack(_) => {
                ReportedWords::default()
            }
            Self::Group { name, .. } => ReportedWords {
                name: Some(name.clone()),
                ..ReportedWords::default()
            },
            Self::Plan {
                group_id,
                place,
                note,
                ..
            } => ReportedWords {
                group_id: Some(group_id.clone()),
                name: None,
                place: Some(place.clone()),
                note: note.clone(),
                ..ReportedWords::default()
            },
            Self::OpenPlan { place, note, .. } => ReportedWords {
                place: Some(place.clone()),
                note: note.clone(),
                ..ReportedWords::default()
            },
            Self::Comment {
                flight_id, text, ..
            } => ReportedWords {
                flight_id: Some(flight_id.clone()),
                comment: Some(text.clone()),
                ..ReportedWords::default()
            },
            Self::Site { revision, .. } => ReportedWords {
                site_revision: Some(*revision),
                ..ReportedWords::default()
            },
        }
    }

    /// The subject of this kind and identifier with the words it kept, as
    /// the database holds them. `None` for a kind nobody stored, and for a
    /// group or a plan without the words its kind keeps.
    pub(crate) fn from_parts(kind: &str, id: &str, words: ReportedWords) -> Option<Self> {
        match kind {
            "flight" => Some(Self::Flight(id.to_owned())),
            "pilot" => Some(Self::Pilot),
            "picture" => Some(Self::Picture(id.to_owned())),
            "live_track" => Some(Self::LiveTrack(id.to_owned())),
            "group" => Some(Self::Group {
                id: id.to_owned(),
                name: words.name?,
            }),
            "plan" => Some(Self::Plan {
                id: id.to_owned(),
                group_id: words.group_id?,
                place: words.place?,
                note: words.note,
            }),
            "open_plan" => Some(Self::OpenPlan {
                id: id.to_owned(),
                place: words.place?,
                note: words.note,
            }),
            "site" => Some(Self::Site {
                id: id.to_owned(),
                revision: words.site_revision?,
            }),
            "comment" => Some(Self::Comment {
                id: id.to_owned(),
                flight_id: words.flight_id?,
                text: words.comment?,
            }),
            _ => None,
        }
    }
}

/// A report to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewReport {
    /// The report's identifier.
    pub id: String,
    /// When it was made.
    pub created_at: Timestamp,
    /// The account that reports.
    pub reporter_id: String,
    /// The account whose flight, page, live track or plan is reported; for
    /// a group, the account that keeps it. `None` for a site that pilots
    /// made, and for nothing else.
    pub about_account_id: Option<String>,
    /// What is reported.
    pub subject: ReportSubject,
    /// Why, as one of the names the API takes. The store keeps the name
    /// as it is: which reasons there are is not its business.
    pub reason: String,
    /// What the reporter wrote beside the reason, if anything.
    pub text: Option<String>,
}

/// What became of a report handed to
/// [`Store::save_report`](crate::Store::save_report).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportSaved {
    /// It was stored.
    Stored,
    /// The reporter has an open report about the same subject already.
    /// Nothing was stored: the operator reads it once. A group is the same
    /// subject whoever keeps it and whatever it is called by now.
    AlreadyOpen,
    /// An account it names is not there. Nothing was stored.
    NoSuchAccount,
}

/// A stored report, as whoever runs the server reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The report's identifier.
    pub id: String,
    /// When it was made.
    pub created_at: Timestamp,
    /// The handle of the account that reported.
    pub reporter_handle: String,
    /// The account that is reported: whose flight, page, live track or
    /// plan it is, or who kept the group when it was reported. `None` for
    /// a report about a site that pilots made, which is nobody's.
    pub about: Option<ReportAbout>,
    /// What is reported.
    pub subject: ReportSubject,
    /// Why.
    pub reason: String,
    /// What the reporter wrote beside the reason, if anything.
    pub text: Option<String>,
    /// When and with what note the report was closed, if it was.
    pub closed: Option<ReportClosure>,
}

/// The account a report is about, as it is now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportAbout {
    /// The account's identifier.
    pub account_id: String,
    /// Its handle.
    pub handle: String,
    /// Its display name.
    pub name: String,
}

/// How a report was closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportClosure {
    /// When.
    pub at: Timestamp,
    /// What the operator noted: what was done, or why nothing was.
    pub note: String,
}

impl Report {
    /// The report's place in a list sorted oldest first: when it was made
    /// and, for reports of the same moment, its identifier.
    pub fn key(&self) -> ReportKey {
        (self.created_at, self.id.clone())
    }
}

/// The place of a report in a list sorted oldest first.
pub type ReportKey = (Timestamp, String);

/// Which reports a list holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportListing {
    /// The ones nobody has closed.
    Open,
    /// All that are kept, closed ones included.
    All,
}

/// What became of a call that closes a report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportClosing {
    /// It is closed now.
    Closed,
    /// It was closed before, like this. Nothing was changed.
    AlreadyClosed(ReportClosure),
    /// No report has this identifier.
    NoSuchReport,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(id: &str, name: &str) -> ReportSubject {
        ReportSubject::Group {
            id: id.to_owned(),
            name: name.to_owned(),
        }
    }

    fn plan(id: &str, place: &str, note: Option<&str>) -> ReportSubject {
        ReportSubject::Plan {
            id: id.to_owned(),
            group_id: "g".to_owned(),
            place: place.to_owned(),
            note: note.map(str::to_owned),
        }
    }

    #[test]
    fn a_subject_is_read_back_from_its_kind_its_identifier_and_its_words() {
        for subject in [
            ReportSubject::Flight("f".to_owned()),
            ReportSubject::Pilot,
            ReportSubject::Picture("p".to_owned()),
            ReportSubject::LiveTrack("t".to_owned()),
            group("g", "Mustergruppe"),
            plan("p", "Musterberg", None),
            plan("p", "Musterberg", Some("Bring a radio.")),
            ReportSubject::Site {
                id: "venti:s".to_owned(),
                revision: 3,
            },
            ReportSubject::Comment {
                id: "c".to_owned(),
                flight_id: "f".to_owned(),
                text: "Well flown.".to_owned(),
            },
        ] {
            let read = ReportSubject::from_parts(subject.kind(), subject.id(), subject.words());
            assert_eq!(read.as_ref(), Some(&subject));
        }
        // A group without its name, a plan without its place or its group
        // and a kind nobody stored are no subjects.
        let nothing = ReportedWords::default;
        assert_eq!(ReportSubject::from_parts("group", "g", nothing()), None);
        assert_eq!(ReportSubject::from_parts("plan", "p", nothing()), None);
        let no_group = ReportedWords {
            place: Some("Musterberg".to_owned()),
            ..nothing()
        };
        assert_eq!(ReportSubject::from_parts("plan", "p", no_group), None);
        // A site without the revision it was reported at is none either.
        assert_eq!(ReportSubject::from_parts("site", "s", nothing()), None);
        assert_eq!(ReportSubject::from_parts("page", "s", nothing()), None);
    }

    #[test]
    fn a_thing_is_the_same_thing_whatever_words_it_was_reported_with() {
        assert!(group("g", "Mustergruppe").is_the_thing(&group("g", "Another name")));
        assert!(!group("g", "Mustergruppe").is_the_thing(&group("h", "Mustergruppe")));
        assert!(plan("p", "Musterberg", None).is_the_thing(&plan("p", "Anderswo", Some("x"))));
        // A group, a plan and a flight with the same letters are three
        // things.
        assert!(!group("x", "Musterberg").is_the_thing(&plan("x", "Musterberg", None)));
        assert!(!ReportSubject::Flight("x".to_owned()).is_the_thing(&group("x", "x")));

        let report = |reporter: &str, about: &str, subject: ReportSubject| NewReport {
            id: "r".to_owned(),
            created_at: chrono::DateTime::from_timestamp(1_795_000_000, 0).unwrap(),
            reporter_id: reporter.to_owned(),
            about_account_id: Some(about.to_owned()).filter(|about| !about.is_empty()),
            subject,
            reason: "spam".to_owned(),
            text: None,
        };
        // A group is one thing to report whoever keeps it; a flight is its
        // pilot's, and the same address under another account is no flight
        // there is.
        let first = report("dario", "anna", group("g", "Mustergruppe"));
        assert!(is_the_same_report(
            &first,
            &report("dario", "bernd", group("g", "Renamed"))
        ));
        assert!(!is_the_same_report(
            &first,
            &report("carla", "anna", group("g", "Mustergruppe"))
        ));
        let flight = || ReportSubject::Flight("f".to_owned());
        assert!(!is_the_same_report(
            &report("dario", "anna", flight()),
            &report("dario", "bernd", flight())
        ));
        // A zero byte is refused wherever it stands.
        for subject in [
            group("g\0", "x"),
            group("g", "x\0"),
            plan("p\0", "x", None),
            plan("p", "x\0", None),
            plan("p", "x", Some("\0")),
        ] {
            let refused = check(&report("dario", "anna", subject));
            assert!(
                matches!(refused, Err(StoreError::Invalid(_))),
                "{refused:?}"
            );
        }
        assert_eq!(check(&first), Ok(()));

        // A site is nobody's and is one thing at every revision; nothing
        // else is without an account.
        let site = |revision| ReportSubject::Site {
            id: "venti:s".to_owned(),
            revision,
        };
        assert!(site(1).is_the_thing(&site(4)));
        assert_eq!(check(&report("dario", "", site(1))), Ok(()));
        assert!(is_the_same_report(
            &report("dario", "", site(1)),
            &report("dario", "", site(4))
        ));
        for wrong in [
            report("dario", "anna", site(1)),
            report("dario", "", flight()),
            report("dario", "", site(0)),
        ] {
            assert!(matches!(check(&wrong), Err(StoreError::Invalid(_))));
        }
    }
}
