//! Adding and correcting a flying site of Venti's own layer: the draft a
//! form edits, what the form shows, the check before sending, the
//! requests, and the answers in words.
//!
//! A site that pilots make is given to everyone under the layer's
//! licence, any signed-in pilot corrects it, and every change is a
//! revision that is kept and can be put back. A shell holds no rule of
//! that: it shows the form this module describes ([`site_form`]), lets
//! the pilot edit a draft ([`site_draft_new`], [`site_draft_of`]), asks
//! what is wrong with it ([`site_draft_check`]), sends what the request
//! builders give and shows what the readers say.
//!
//! **What a site has to be is said in one place below this crate**: the
//! check is the one the server runs ([`venti_api_types::v1::SiteDraft::record`],
//! with the numbers of [`venti_domain::Site`]), and this module only puts
//! each fault into words for the pilot.
//!
//! **Only a site of Venti's own layer has a draft.** A source's site is
//! its source's record under its source's licence: copying it into the
//! layer would give away what is not the pilot's to give, so
//! [`site_draft_of`] refuses it.

use venti_api_types::v1::{
    self as api, AddSiteRequest, ChangeSiteRequest, MAX_SITE_COMMENT_LENGTH, RemoveSiteRequest,
    RestoreSiteRequest, SiteHistoryResponse, SiteResponse, SiteRevisionKind, SiteVia,
    VENTI_SITES_LICENCE, VENTI_SITES_LICENCE_URL, VENTI_SITES_SOURCE, parse_site_comment,
};
use venti_domain::{GeoPoint, Site, SiteId, SiteOutline, SiteUse, WindSector};

use crate::account::json_request;
use crate::server::{answer, code, refusal};
use crate::sites::{
    BY_PILOTS_NOTE, author, author_in_words, kind, site_view, uses_in_words, wind_in_words,
};
use crate::wind::credit;
use crate::{
    Coordinate, CoreError, Credit, ServerRequest, SiteAuthor, SiteKind, SiteView, SitesWindChoice,
};

/// One thing a form for a site asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum SiteField {
    /// The site's name.
    Name,
    /// Where it is.
    Place,
    /// Its height above sea level.
    Height,
    /// What it is used for.
    Uses,
    /// The wind directions it works in.
    Wind,
    /// What the pilot says about it in words.
    Description,
    /// Whether a payment is asked for using it.
    Fee,
    /// A web page about it.
    Website,
    /// A time in which it may not be used.
    NoFlightTime,
    /// Whether guests have rules to take notice of.
    Guests,
    /// Why the change is made: the line kept with the revision.
    Comment,
}

/// A site as a form edits it: what the server takes of a site from a
/// pilot, and nothing it refuses. Whether a site is official or
/// discouraged is not one pilot's to say, and is not here.
///
/// A shell changes the fields the form shows and hands the draft back.
/// What the form does not show, it leaves as it is: a correction replaces
/// the whole record, and a field that is not sent is a field the site no
/// longer has.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteDraftView {
    /// The site the draft corrects. Absent for a site that is new.
    pub site_id: Option<String>,
    /// The number of the revision the draft was made from. The server
    /// refuses the correction when the site stands at another one by
    /// then. Absent for a site that is new.
    pub revision: Option<u32>,
    /// Its name. One line.
    pub name: String,
    /// Where it is. To move a site, ask [`site_draft_moved`]: it knows
    /// what else of the draft goes with the old place.
    pub position: Coordinate,
    /// Its height above sea level in metres, if the pilot knows it.
    pub elevation_m: Option<f64>,
    /// What it is used for: at least one of take-off, landing, towing
    /// and training.
    pub uses: Vec<SiteKind>,
    /// The wind directions it works in, as the points of the compass that
    /// are ticked: each the `from_deg` of one of the form's sixteen
    /// ([`SiteForm::winds`]). Points that lie side by side are sent as
    /// one span.
    pub wind_from_deg: Vec<f64>,
    /// What the pilot says about the site. It may have several lines.
    /// Empty for none.
    pub description: String,
    /// Whether a payment is asked for using the site: "yes", "no", or
    /// other words. One line; empty when the pilot does not say.
    pub fee: String,
    /// A web page about the site. Empty for none.
    pub website: String,
    /// A time in which the site may not be used, in words. One line;
    /// empty for none.
    pub no_flight_time: String,
    /// Whether guests have rules to take notice of, if the pilot says.
    pub guest_guidelines: Option<bool>,
    /// Whether the site suits paragliders, if its record says. The form
    /// does not ask; a draft made from a site carries what the site says.
    pub paragliding: Option<bool>,
    /// The same for hang gliders.
    pub hang_gliding: Option<bool>,
    /// The outline of a site that is drawn as a field, closed. The form
    /// does not edit it; a draft made from a site carries the site's.
    pub outline: Vec<Coordinate>,
}

/// One field of the form: what it is called and what is said under it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteFormField {
    /// Which field.
    pub field: SiteField,
    /// Its title: "Name".
    pub title: String,
    /// A line of help under it.
    pub help: String,
    /// Whether a site cannot be sent without it.
    pub is_needed: bool,
    /// The most characters its text may have, for a field that is text.
    pub at_most: Option<u32>,
}

/// A use a form offers.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteUseChoice {
    /// The use, to put into [`SiteDraftView::uses`].
    pub kind: SiteKind,
    /// Its name: "Take-off".
    pub words: String,
}

/// An answer to the question about guests' rules.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteGuestsChoice {
    /// What to put into [`SiteDraftView::guest_guidelines`].
    pub value: Option<bool>,
    /// The answer in words: "Not said".
    pub words: String,
}

/// What a form for a site has to work with: its fields in the order to
/// show them, the choices, the limits as the server has them, and the
/// words around the act of giving.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteForm {
    /// The fields, in the order to show them. The last is the comment,
    /// which belongs to the change and not to the site.
    pub fields: Vec<SiteFormField>,
    /// The uses to tick: take-off, landing, towing, training.
    pub uses: Vec<SiteUseChoice>,
    /// The sixteen wind directions to tick, clockwise from north: the
    /// ones the filter of sites offers.
    pub winds: Vec<SitesWindChoice>,
    /// The answers to the question about guests' rules.
    pub guests: Vec<SiteGuestsChoice>,
    /// The lowest height a site may have, metres.
    pub lowest_m: f64,
    /// The highest, metres.
    pub highest_m: f64,
    /// What giving a site means, to show before the pilot's first site is
    /// sent and to keep within reach after: "You give this site to
    /// everyone under CC0 1.0 …".
    pub giving: String,
    /// The licence's name, for a link: "CC0 1.0".
    pub licence: String,
    /// The address of the licence's text.
    pub licence_url: String,
    /// What is said wherever such a site is shown, and on the form:
    /// "Added by pilots on Venti and not verified. A site on the map is
    /// not permission to fly there."
    pub note: String,
    /// What to say where the place is chosen on the map.
    pub place_words: String,
    /// The title of a form for a new site: "Add a site".
    pub title_new: String,
    /// The title of a form that corrects one: "Correct this site".
    pub title_correct: String,
    /// The button that sends a new site: "Add for everyone".
    pub send_new: String,
    /// The button that sends a correction: "Save for everyone".
    pub send_correction: String,
}

/// Something wrong with a draft, in words for the pilot.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteDraftFault {
    /// The field it is about, to show the words under.
    pub field: SiteField,
    /// What is wrong and what to do: "Give the site a name."
    pub words: String,
}

/// A site as its own address answers it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SitePage {
    /// The site.
    pub site: SiteView,
    /// Whose data it is, with the licence it is under.
    pub credit: Option<Credit>,
    /// Whether the address is for good: `true` for a site that pilots
    /// made, whose identifier is never used for another site.
    pub is_lasting: bool,
    /// Whether a pilot can correct it here: it is a site of Venti's own
    /// layer. A source's site is corrected at its source (`site.url`).
    pub can_be_corrected: bool,
}

/// What became of a site that was added, corrected or put back.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteWritten {
    /// The site as it stands now.
    pub site: SiteView,
    /// In a line: "Added. The site is on the map for everyone now."
    pub words: String,
}

/// What a revision did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum SiteRevisionWas {
    /// It added the site.
    Added,
    /// It changed what the site says.
    Changed,
    /// It put the site back to what an earlier revision said.
    PutBack,
    /// It removed the site.
    Removed,
    /// Something this version does not know.
    Other,
}

/// One revision of a site, as its history shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteRevisionView {
    /// Its number. The first is 1.
    pub number: u32,
    /// When it was made, seconds since 1970-01-01 UTC.
    pub at_unix_s: i64,
    /// Who made it. Absent for whoever runs Venti, and once the pilot's
    /// account is gone.
    pub by: Option<SiteAuthor>,
    /// What it did.
    pub was: SiteRevisionWas,
    /// For a revision that put an earlier one back: that one's number.
    pub restores: Option<u32>,
    /// Why, in its author's words, if they said. A pilot's text: shown
    /// as written.
    pub comment: Option<String>,
    /// The revision in a line: "Changed by Mara Beispiel (@mara) on 21
    /// Nov 2026: \"fixed the height\"". The day is the day in UTC; a
    /// shell that shows the device's day beside it has `at_unix_s`.
    pub words: String,
    /// What it changed against the revision before, a line per field:
    /// "Height: 1862 m → 1871 m". Empty for the first revision, for one
    /// that removed the site, and for the first of a page whose
    /// predecessor is on the page before ([`site_changes_between`] says
    /// it for any two).
    pub changes: Vec<String>,
    /// The site as it stood from this revision on; for one that removed
    /// the site, as it stood until then.
    pub site: SiteView,
    /// Whether the site stands at this revision.
    pub is_current: bool,
    /// Whether putting the site back to this revision would change
    /// anything: it is not the one the site stands at, or the site is
    /// removed.
    pub can_be_put_back: bool,
}

/// One page of a site's history.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteHistory {
    /// The site.
    pub site_id: String,
    /// The number of the revision it stands at.
    pub revision: u32,
    /// Whether it is removed. Putting a revision back brings it back.
    pub is_removed: bool,
    /// The revisions of this page, the first one first.
    pub revisions: Vec<SiteRevisionView>,
    /// What to pass as `after` to [`site_history_path`] for the next
    /// page. Absent when this is the last.
    pub next_after: Option<u32>,
    /// The revision that undoes the last change when it is put back: the
    /// one before the one the site stands at. Absent for a site at its
    /// first revision, where the last change is that it was added; when
    /// the revision before is not on this page; and when it removed the
    /// site, where the undoing is to remove it again.
    pub undo_revision: Option<u32>,
    /// The same as the words of a button: "Undo the last change".
    /// Empty when `undo_revision` is absent.
    pub undo_words: String,
    /// Whose data it is, with the licence every revision is under.
    pub credit: Option<Credit>,
}

/// The site an error named: the one that stands where another was to be
/// added.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteInTheWay {
    /// Its identifier, to open it with ([`site_path`]).
    pub id: String,
    /// Its name, or "A site without a name".
    pub name: String,
    /// The offer in words: "Open Beispielkopf Süd".
    pub words: String,
}

// --- The form ----------------------------------------------------------------

/// The four uses a pilot can say, in the order of the record.
const USES: [(SiteKind, &str); 4] = [
    (SiteKind::Takeoff, "Take-off"),
    (SiteKind::Landing, "Landing"),
    (SiteKind::Towing, "Winch site"),
    (SiteKind::Training, "Training slope"),
];

fn chars(count: usize) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// What a form for a site shows: its fields with their titles and helps,
/// the choices, the limits, and the words around the act of giving. The
/// same for a new site and for a correction.
#[uniffi::export]
pub fn site_form() -> SiteForm {
    let (lowest_m, highest_m) = Site::ELEVATION_LIMITS_M;
    let field = |field, title: &str, help: &str, is_needed, at_most: Option<usize>| SiteFormField {
        field,
        title: title.to_owned(),
        help: help.to_owned(),
        is_needed,
        at_most: at_most.map(chars),
    };
    SiteForm {
        fields: vec![
            field(
                SiteField::Name,
                "Name",
                "What pilots call the place.",
                true,
                Some(Site::MAX_NAME_CHARS),
            ),
            field(
                SiteField::Place,
                "Place",
                "Where pilots stand or touch down, as exactly as you can mark it.",
                true,
                None,
            ),
            field(SiteField::Uses, "Used for", "At least one.", true, None),
            field(
                SiteField::Height,
                "Height",
                "Above sea level, in metres. Leave it empty if you do not know it.",
                false,
                None,
            ),
            field(
                SiteField::Wind,
                "Wind directions",
                "Where the wind comes from when the site works. Tick none if you do not know.",
                false,
                None,
            ),
            field(
                SiteField::Description,
                "Description",
                "What a pilot who has not been there should know. Everyone can read it: write \
                 nothing private.",
                false,
                Some(Site::MAX_DESCRIPTION_CHARS),
            ),
            field(
                SiteField::Fee,
                "Fee",
                "Whether a payment is asked for using the site: yes, no, or a few words.",
                false,
                Some(Site::MAX_REMARK_CHARS),
            ),
            field(
                SiteField::Guests,
                "Rules for guests",
                "Whether pilots who are guests have rules to read before they fly here.",
                false,
                None,
            ),
            field(
                SiteField::NoFlightTime,
                "Not to be used",
                "A time in which the site may not be used, as its keepers write it: \
                 01.04-30.09. It is shown as written.",
                false,
                Some(Site::MAX_REMARK_CHARS),
            ),
            field(
                SiteField::Website,
                "Web page",
                "A page about the site: its club's, or one that says more. It begins with \
                 https:// or http://.",
                false,
                Some(Site::MAX_WEBSITE_CHARS),
            ),
            field(
                SiteField::Comment,
                "Why",
                "A line for other pilots about this change: what you corrected, or how you \
                 know. It is kept in the site's history.",
                false,
                Some(MAX_SITE_COMMENT_LENGTH),
            ),
        ],
        uses: USES
            .into_iter()
            .map(|(kind, words)| SiteUseChoice {
                kind,
                words: words.to_owned(),
            })
            .collect(),
        winds: crate::sites_filter_choices().winds,
        guests: [
            (None, "Not said"),
            (Some(true), "Yes, guests have rules to read"),
            (Some(false), "No rules for guests"),
        ]
        .into_iter()
        .map(|(value, words)| SiteGuestsChoice {
            value,
            words: words.to_owned(),
        })
        .collect(),
        lowest_m,
        highest_m,
        giving: format!(
            "You give this site to everyone under {VENTI_SITES_LICENCE}, the public domain: \
             anyone may use it and change it, on Venti and anywhere else. It is shown with your \
             handle, any pilot can correct it, and it stays on Venti when your account goes."
        ),
        licence: VENTI_SITES_LICENCE.to_owned(),
        licence_url: VENTI_SITES_LICENCE_URL.to_owned(),
        note: BY_PILOTS_NOTE.to_owned(),
        place_words: "Move the map until the mark is where pilots stand or touch down.".to_owned(),
        title_new: "Add a site".to_owned(),
        title_correct: "Correct this site".to_owned(),
        send_new: "Add for everyone".to_owned(),
        send_correction: "Save for everyone".to_owned(),
    }
}

// --- The draft ---------------------------------------------------------------

/// A draft for a new site at a place, with nothing said yet.
#[uniffi::export]
pub fn site_draft_new(place: Coordinate) -> SiteDraftView {
    SiteDraftView {
        site_id: None,
        revision: None,
        name: String::new(),
        position: place,
        elevation_m: None,
        uses: Vec::new(),
        wind_from_deg: Vec::new(),
        description: String::new(),
        fee: String::new(),
        website: String::new(),
        no_flight_time: String::new(),
        guest_guidelines: None,
        paragliding: None,
        hang_gliding: None,
        outline: Vec::new(),
    }
}

/// A draft that corrects a site: everything the site says, and the
/// revision it stands at.
///
/// # Errors
///
/// For a site that is not of Venti's own layer: a source's record is
/// corrected at its source, and is not copied into the layer.
#[uniffi::export]
pub fn site_draft_of(site: SiteView) -> Result<SiteDraftView, CoreError> {
    let Some(by_pilots) = site
        .by_pilots
        .as_ref()
        .filter(|_| site.source == VENTI_SITES_SOURCE)
    else {
        return Err(invalid(
            "This site is its source's record and is corrected at its source. Only a site \
             that pilots added on Venti is corrected here.",
        ));
    };
    let sectors: Vec<WindSector> = site
        .wind
        .iter()
        .map(|span| WindSector {
            from_deg: span.from_deg,
            to_deg: span.to_deg,
        })
        .collect();
    Ok(SiteDraftView {
        site_id: Some(site.id),
        revision: Some(by_pilots.revision),
        name: if site.is_named {
            site.name
        } else {
            String::new()
        },
        position: site.position,
        elevation_m: site.elevation_m,
        uses: site.uses,
        wind_from_deg: points_of(&sectors),
        description: site.description.unwrap_or_default(),
        fee: site.fee.unwrap_or_default(),
        website: site.website.unwrap_or_default(),
        no_flight_time: site.no_flight_time.unwrap_or_default(),
        guest_guidelines: site.guest_guidelines,
        paragliding: site.paragliding,
        hang_gliding: site.hang_gliding,
        outline: site.outline,
    })
}

/// The draft with the site at another place. An outline goes with the
/// old place: a field that was drawn there is not the field here.
#[uniffi::export]
pub fn site_draft_moved(draft: SiteDraftView, place: Coordinate) -> SiteDraftView {
    let moved = draft.position != place;
    SiteDraftView {
        position: place,
        outline: if moved { Vec::new() } else { draft.outline },
        ..draft
    }
}

/// One sixteenth of the compass, degrees.
const POINT_DEG: f64 = 22.5;

/// The point of the compass nearest to a direction, 0 to 15.
fn point(deg: f64) -> usize {
    ((deg.rem_euclid(360.0) + POINT_DEG / 2.0) / POINT_DEG) as usize % 16
}

fn point_deg(point: usize) -> f64 {
    // At most 15: exact.
    POINT_DEG * f64::from(u8::try_from(point % 16).unwrap_or(0))
}

/// The points of the compass a site's spans name: every point inside a
/// span, and for a span that holds none, the one nearest to its middle.
fn points_of(sectors: &[WindSector]) -> Vec<f64> {
    let mut ticked = [false; 16];
    for sector in sectors {
        if !sector.from_deg.is_finite() || !sector.to_deg.is_finite() {
            continue;
        }
        let mut any = false;
        for (index, tick) in ticked.iter_mut().enumerate() {
            if sector.contains(point_deg(index)) {
                *tick = true;
                any = true;
            }
        }
        if !any {
            let width = (sector.to_deg - sector.from_deg).rem_euclid(360.0);
            ticked[point(sector.from_deg + width / 2.0)] = true;
        }
    }
    (0..16)
        .filter(|&index| ticked[index])
        .map(point_deg)
        .collect()
}

/// The spans that ticked points make: points side by side are one span,
/// clockwise, a point alone is a single direction, and all sixteen are
/// the whole compass. Always the same spans for the same points, so a
/// correction that leaves the wind alone sends it as it was.
fn sectors_of(points: &[f64]) -> Vec<WindSector> {
    let mut ticked = [false; 16];
    for deg in points.iter().filter(|deg| deg.is_finite()) {
        ticked[point(*deg)] = true;
    }
    if ticked.iter().all(|tick| *tick) {
        return vec![WindSector {
            from_deg: 0.0,
            to_deg: 360.0,
        }];
    }
    let mut sectors = Vec::new();
    for start in 0..16 {
        // A run begins at a ticked point whose neighbour before is not.
        if !ticked[start] || ticked[(start + 15) % 16] {
            continue;
        }
        let mut end = start;
        while ticked[(end + 1) % 16] {
            end = (end + 1) % 16;
        }
        sectors.push(WindSector {
            from_deg: point_deg(start),
            to_deg: point_deg(end),
        });
    }
    sectors
}

fn invalid(message: &str) -> CoreError {
    CoreError::Server {
        message: message.to_owned(),
    }
}

fn fault(field: SiteField, words: impl Into<String>) -> SiteDraftFault {
    SiteDraftFault {
        field,
        words: words.into(),
    }
}

/// The draft as the server takes it. What is no place and no outline
/// cannot be written down at all, and is a fault here.
fn wire(draft: &SiteDraftView) -> Result<api::SiteDraft, SiteDraftFault> {
    let no_place = || fault(SiteField::Place, "Mark the place on the map.");
    let position = GeoPoint::new(draft.position.lat, draft.position.lon).map_err(|_| no_place())?;
    let outline = if draft.outline.is_empty() {
        None
    } else {
        let points = draft
            .outline
            .iter()
            .map(|point| GeoPoint::new(point.lat, point.lon))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| no_place())?;
        Some(SiteOutline::new(points).map_err(|_| {
            fault(
                SiteField::Place,
                "The outline of this site cannot be kept. Mark the place again.",
            )
        })?)
    };
    let text = |text: &str| Some(text.to_owned()).filter(|text| !text.trim().is_empty());
    Ok(api::SiteDraft {
        name: draft.name.clone(),
        position,
        elevation_m: draft.elevation_m,
        uses: draft
            .uses
            .iter()
            .map(|kind| match kind {
                SiteKind::Takeoff => SiteUse::Takeoff,
                SiteKind::Landing => SiteUse::Landing,
                SiteKind::Towing => SiteUse::Towing,
                SiteKind::Training => SiteUse::Training,
                SiteKind::Other => SiteUse::Other,
            })
            .collect(),
        wind_directions: sectors_of(&draft.wind_from_deg),
        paragliding: draft.paragliding,
        hang_gliding: draft.hang_gliding,
        website: text(&draft.website),
        description: text(&draft.description),
        fee: text(&draft.fee),
        guest_guidelines: draft.guest_guidelines,
        no_flight_time: text(&draft.no_flight_time),
        outline,
    })
}

/// A fault of the record, by the name of its field on the wire, in words
/// for the pilot. The numbers are the record's own.
fn in_words(field: &str, draft: &SiteDraftView) -> SiteDraftFault {
    let room = |what: &str, most: usize| format!("{what} has room for {most} characters.");
    match field {
        "name" if draft.name.trim().is_empty() => fault(SiteField::Name, "Give the site a name."),
        "name" => fault(SiteField::Name, room("A name", Site::MAX_NAME_CHARS)),
        "uses" => fault(
            SiteField::Uses,
            "Say what the site is used for: take-off, landing, winch site or training slope.",
        ),
        "elevation_m" => {
            let (low, high) = Site::ELEVATION_LIMITS_M;
            fault(
                SiteField::Height,
                format!("A height is between {low:.0} and {high:.0} m above sea level."),
            )
        }
        "wind_directions" => fault(
            SiteField::Wind,
            "The wind directions cannot be kept. Tick them again.",
        ),
        "website" => fault(
            SiteField::Website,
            format!(
                "A web page's address begins with https:// or http://, has no blanks and at \
                 most {} characters.",
                Site::MAX_WEBSITE_CHARS
            ),
        ),
        "description" => fault(
            SiteField::Description,
            room("The description", Site::MAX_DESCRIPTION_CHARS),
        ),
        "fee" => fault(SiteField::Fee, room("The fee", Site::MAX_REMARK_CHARS)),
        "no_flight_time" => fault(
            SiteField::NoFlightTime,
            room("This line", Site::MAX_REMARK_CHARS),
        ),
        // A field this version has no words for: said at the name, where
        // a form begins.
        _ => fault(SiteField::Name, "The site cannot be sent as it is."),
    }
}

/// An identifier for a record that is only checked and never sent.
fn checking_id() -> Option<SiteId> {
    format!("{VENTI_SITES_SOURCE}:draft").parse().ok()
}

/// The draft as the body of a request, or everything that is wrong with
/// it.
fn checked(
    draft: &SiteDraftView,
    comment: &str,
) -> Result<(api::SiteDraft, Option<String>), Vec<SiteDraftFault>> {
    let mut faults = Vec::new();
    let wire = match wire(draft) {
        Ok(wire) => Some(wire),
        Err(fault) => {
            faults.push(fault);
            None
        }
    };
    if let Some((wire, id)) = wire.clone().zip(checking_id())
        && let Err(found) = wire.record(id)
    {
        faults.extend(found.iter().map(|fault| in_words(fault.field, draft)));
    }
    let comment = parse_site_comment(Some(comment)).unwrap_or_else(|most| {
        faults.push(fault(
            SiteField::Comment,
            format!("This line has room for {most} characters."),
        ));
        None
    });
    match wire {
        Some(wire) if faults.is_empty() => Ok((wire, comment)),
        _ => Err(faults),
    }
}

/// What is wrong with a draft, field by field, by the server's own check:
/// what this lets through the server takes, unless the place is outside
/// the area it serves or another site stands there, which only the server
/// knows. Empty when the draft can be sent. `comment` is the line about
/// the change, which may be empty.
#[uniffi::export]
pub fn site_draft_check(draft: SiteDraftView, comment: String) -> Vec<SiteDraftFault> {
    checked(&draft, &comment).err().unwrap_or_default()
}

/// The first thing wrong with a draft, as the refusal of a request.
fn first(faults: Vec<SiteDraftFault>) -> CoreError {
    CoreError::Server {
        message: faults.into_iter().next().map_or_else(
            || "The site cannot be sent as it is.".to_owned(),
            |f| f.words,
        ),
    }
}

fn made_here(id: &str) -> Result<SiteId, CoreError> {
    id.parse::<SiteId>()
        .ok()
        .filter(|id| id.source().as_str() == VENTI_SITES_SOURCE)
        .ok_or_else(|| {
            invalid(
                "This site is its source's record and is corrected at its source. Only a \
                 site that pilots added on Venti is changed here.",
            )
        })
}

fn comment_of(comment: &str) -> Result<Option<String>, CoreError> {
    parse_site_comment(Some(comment))
        .map_err(|most| invalid(&format!("This line has room for {most} characters.")))
}

// --- The requests ------------------------------------------------------------

/// The request that adds a site, for a signed-in pilot. `comment` is the
/// line kept with the first revision, and may be empty.
///
/// # Errors
///
/// The first thing [`site_draft_check`] finds, in its words.
#[uniffi::export]
pub fn site_add_request(draft: SiteDraftView, comment: String) -> Result<ServerRequest, CoreError> {
    let (site, comment) = checked(&draft, &comment).map_err(first)?;
    json_request(
        "POST",
        api::SITES_PATH,
        &AddSiteRequest { site, comment },
        true,
    )
}

/// The request that corrects a site: the whole draft, as a new revision,
/// made from the revision the draft names.
///
/// # Errors
///
/// The first thing [`site_draft_check`] finds, in its words, and a draft
/// that was not made from a site ([`site_draft_of`]).
#[uniffi::export]
pub fn site_change_request(
    draft: SiteDraftView,
    comment: String,
) -> Result<ServerRequest, CoreError> {
    let (Some(id), Some(revision)) = (draft.site_id.as_deref(), draft.revision) else {
        return Err(invalid(
            "This site is new: add it instead of correcting it.",
        ));
    };
    let id = made_here(id)?;
    let (site, comment) = checked(&draft, &comment).map_err(first)?;
    json_request(
        "PUT",
        &api::site_path(&id),
        &ChangeSiteRequest {
            site,
            revision,
            comment,
        },
        true,
    )
}

/// The request that removes a site, as a new revision. The history
/// stays, and any pilot brings the site back by putting a revision back.
#[uniffi::export]
pub fn site_remove_request(id: String, comment: String) -> Result<ServerRequest, CoreError> {
    let id = made_here(&id)?;
    json_request(
        "DELETE",
        &api::site_path(&id),
        &RemoveSiteRequest {
            comment: comment_of(&comment)?,
        },
        true,
    )
}

/// The request that puts a site back to what one of its revisions said,
/// as a new revision: how a change is undone, and how a removed site
/// comes back.
#[uniffi::export]
pub fn site_restore_request(
    id: String,
    revision: u32,
    comment: String,
) -> Result<ServerRequest, CoreError> {
    let id = made_here(&id)?;
    if revision == 0 {
        return Err(invalid("The site has no such revision."));
    }
    json_request(
        "POST",
        &api::site_revisions_path(&id),
        &RestoreSiteRequest {
            restore: revision,
            comment: comment_of(&comment)?,
        },
        true,
    )
}

/// The path to `GET` for one site by its identifier, of any layer.
#[uniffi::export]
pub fn site_path(id: String) -> Result<String, CoreError> {
    let id: SiteId = id
        .parse()
        .map_err(|_| invalid("This is not a site the app knows."))?;
    Ok(api::site_path(&id))
}

/// The path to `GET` for a page of the history of a site that pilots
/// made: the revisions after number `after`, or from the first.
#[uniffi::export]
pub fn site_history_path(id: String, after: Option<u32>) -> Result<String, CoreError> {
    let id = made_here(&id)?;
    let path = api::site_history_path(&id);
    Ok(match after {
        Some(after) if after > 0 => format!("{path}?after={after}"),
        _ => path,
    })
}

// --- The answers -------------------------------------------------------------

/// A refusal of a request about a site, in words for the pilot.
fn site_refusal(status: u16, body: &[u8]) -> CoreError {
    let message = match (status, code(body).as_deref()) {
        (_, Some("invalid_site")) => {
            "The server does not accept this site. The app may be older than the server.".to_owned()
        }
        (_, Some("site_out_of_area")) => {
            "This place lies outside the area Venti takes sites for.".to_owned()
        }
        (_, Some("site_too_close")) => match site_in_the_way(status, body.to_vec()) {
            Some(other) => format!(
                "A site that pilots added stands at this place already: {}. If it is the same \
                 place, correct that one.",
                other.name
            ),
            None => "A site that pilots added stands at this place already. If it is the same \
                     place, correct that one."
                .to_owned(),
        },
        (_, Some("site_changed")) => {
            "Someone else changed this site while you were editing it. Look at how it stands \
             now, then make your change again."
                .to_owned()
        }
        (_, Some("site_gone")) => {
            "This site was removed. Its history says by whom; if a pilot removed it, putting \
             an earlier revision back brings it back."
                .to_owned()
        }
        (_, Some("site_not_found")) => "There is no such site.".to_owned(),
        (_, Some("site_revision_not_found")) => "The site has no such revision.".to_owned(),
        (_, Some("sites_closed")) => {
            "This server takes no sites from pilots. A site of another source is corrected at \
             its source."
                .to_owned()
        }
        (_, Some("publishing_suspended")) => {
            "This account was stopped from publishing on Venti. It can add and change no site."
                .to_owned()
        }
        (429, _) | (_, Some("rate_limited")) => {
            "You have added or changed as many sites today as one account may. Try again \
             tomorrow."
                .to_owned()
        }
        _ => return refusal(status, body),
    };
    CoreError::Server { message }
}

/// The site that stands in the way, from the server's answer to a
/// request that was refused because another site stands at the place
/// (`site_too_close`): for a shell to offer it, to be corrected instead.
/// `None` for every other answer.
#[uniffi::export]
pub fn site_in_the_way(status: u16, body: Vec<u8>) -> Option<SiteInTheWay> {
    if (200..300).contains(&status) {
        return None;
    }
    let error: api::ErrorResponse = serde_json::from_slice(&body).ok()?;
    if error.error.code != api::ErrorCode::SiteTooClose {
        return None;
    }
    let site = error.error.site?;
    // A pilot's text from a server: one line of it, and no more than a
    // name may have.
    let name: String = site
        .name
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(Site::MAX_NAME_CHARS)
        .collect();
    let name = match name.trim() {
        "" => "A site without a name".to_owned(),
        name => name.to_owned(),
    };
    Some(SiteInTheWay {
        id: site.id.to_string(),
        words: format!("Open {name}"),
        name,
    })
}

fn read<T: serde::de::DeserializeOwned>(status: u16, body: &[u8]) -> Result<T, CoreError> {
    if (200..300).contains(&status) {
        answer(status, body)
    } else {
        Err(site_refusal(status, body))
    }
}

/// Reads the server's answer to [`site_path`]: one site as it stands.
///
/// # Errors
///
/// The server's refusal in words: that there is no such site, or that it
/// was removed.
#[uniffi::export]
pub fn site_page(status: u16, body: Vec<u8>) -> Result<SitePage, CoreError> {
    let response: SiteResponse = read(status, &body)?;
    let site = site_view(&response.site, response.standing.as_ref());
    Ok(SitePage {
        credit: response
            .attributions
            .iter()
            .find(|attribution| attribution.source.as_str() == site.source)
            .map(credit),
        is_lasting: response.lasting,
        can_be_corrected: site.by_pilots.is_some(),
        site,
    })
}

/// Reads the server's answer to [`site_add_request`],
/// [`site_change_request`] and [`site_restore_request`]: the site as it
/// stands now, and what became of the request in a line.
///
/// # Errors
///
/// The server's refusal in words for the pilot: the place is outside the
/// area, another site stands there (then [`site_in_the_way`] names it),
/// someone else changed the site meanwhile, the server takes no sites,
/// the account may not publish, or it has written as much today as an
/// account may.
#[uniffi::export]
pub fn site_written(status: u16, body: Vec<u8>) -> Result<SiteWritten, CoreError> {
    let response: SiteResponse = read(status, &body)?;
    let site = site_view(&response.site, response.standing.as_ref());
    let revision = response
        .standing
        .as_ref()
        .map_or(0, |standing| standing.revision);
    let words = if status == 201 || revision <= 1 {
        "Added. The site is on the map for everyone now.".to_owned()
    } else {
        format!("Saved for everyone, as revision {revision} of this site.")
    };
    Ok(SiteWritten { site, words })
}

/// Reads the server's answer to [`site_remove_request`] and says in a
/// line what became of it.
///
/// # Errors
///
/// The server's refusal, as for [`site_written`].
#[uniffi::export]
pub fn site_removed(status: u16, body: Vec<u8>) -> Result<String, CoreError> {
    if (200..300).contains(&status) {
        Ok("Removed from the map. Its history stays, and any pilot can bring it back.".to_owned())
    } else {
        Err(site_refusal(status, &body))
    }
}

/// A day in words, in UTC: "21 Nov 2026".
fn day(at: venti_domain::Timestamp) -> String {
    at.format("%-d %b %Y").to_string()
}

/// A pilot's line as a quotation: one line, between quotation marks.
fn quoted(text: &str) -> String {
    let line: String = text.chars().filter(|c| !c.is_control()).collect();
    format!("\u{201c}{}\u{201d}", line.trim())
}

fn metres(height: Option<f64>) -> String {
    height.map_or_else(|| "none".to_owned(), |height| format!("{height:.0} m"))
}

fn said(text: Option<&str>) -> String {
    text.map_or_else(|| "none".to_owned(), quoted)
}

fn yes_no(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "yes",
        Some(false) => "no",
        None => "not said",
    }
}

/// Adds a line that says a field went from one thing to another, when
/// it did.
fn differ(lines: &mut Vec<String>, title: &str, from: String, to: String) {
    if from != to {
        lines.push(format!("{title}: {from} \u{2192} {to}"));
    }
}

/// What differs between two records of one site, a line per field, in
/// the order of the form.
fn changes(before: &Site, after: &Site) -> Vec<String> {
    let mut lines = Vec::new();
    differ(
        &mut lines,
        "Name",
        said(before.name.as_deref()),
        said(after.name.as_deref()),
    );
    let moved_m = before.position.distance_m(after.position);
    if before.position != after.position {
        lines.push(if moved_m < 1.0 {
            "Place: moved by less than a metre".to_owned()
        } else {
            format!(
                "Place: moved by {}",
                crate::airspace::distance_in_words(moved_m)
            )
        });
    }
    let uses = |site: &Site| {
        let kinds: Vec<SiteKind> = site.uses.iter().copied().map(kind).collect();
        uses_in_words(&kinds)
    };
    differ(&mut lines, "Used for", uses(before), uses(after));
    differ(
        &mut lines,
        "Height",
        metres(before.elevation_m),
        metres(after.elevation_m),
    );
    let wind = |site: &Site| match wind_in_words(&site.wind_directions) {
        words if words.is_empty() => "none".to_owned(),
        words => words,
    };
    differ(&mut lines, "Wind directions", wind(before), wind(after));
    // A description may be long: that it changed is said, and the two
    // records are there to read.
    match (&before.description, &after.description) {
        (one, other) if one == other => {}
        (None, Some(_)) => lines.push("Description: added".to_owned()),
        (Some(_), None) => lines.push("Description: taken out".to_owned()),
        _ => lines.push("Description: rewritten".to_owned()),
    }
    differ(
        &mut lines,
        "Fee",
        said(before.fee.as_deref()),
        said(after.fee.as_deref()),
    );
    differ(
        &mut lines,
        "Rules for guests",
        yes_no(before.guest_guidelines).to_owned(),
        yes_no(after.guest_guidelines).to_owned(),
    );
    differ(
        &mut lines,
        "Not to be used",
        said(before.no_flight_time.as_deref()),
        said(after.no_flight_time.as_deref()),
    );
    differ(
        &mut lines,
        "Web page",
        before.website.clone().unwrap_or_else(|| "none".to_owned()),
        after.website.clone().unwrap_or_else(|| "none".to_owned()),
    );
    differ(
        &mut lines,
        "Paragliders",
        yes_no(before.paragliding).to_owned(),
        yes_no(after.paragliding).to_owned(),
    );
    differ(
        &mut lines,
        "Hang gliders",
        yes_no(before.hang_gliding).to_owned(),
        yes_no(after.hang_gliding).to_owned(),
    );
    match (&before.outline, &after.outline) {
        (one, other) if one == other => {}
        (None, Some(_)) => lines.push("Outline: drawn".to_owned()),
        (Some(_), None) => lines.push("Outline: taken out".to_owned()),
        _ => lines.push("Outline: redrawn".to_owned()),
    }
    lines
}

/// A view back to the record it shows, as far as two records are
/// compared: what a pilot says of a site.
fn record_of(view: &SiteView) -> Option<Site> {
    let draft = SiteDraftView {
        site_id: None,
        revision: None,
        name: if view.is_named {
            view.name.clone()
        } else {
            String::new()
        },
        position: view.position,
        elevation_m: view.elevation_m,
        uses: view.uses.clone(),
        wind_from_deg: Vec::new(),
        description: view.description.clone().unwrap_or_default(),
        fee: view.fee.clone().unwrap_or_default(),
        website: view.website.clone().unwrap_or_default(),
        no_flight_time: view.no_flight_time.clone().unwrap_or_default(),
        guest_guidelines: view.guest_guidelines,
        paragliding: view.paragliding,
        hang_gliding: view.hang_gliding,
        outline: view.outline.clone(),
    };
    let mut wire = wire(&draft).ok()?;
    // The wind as the view has it, not as points.
    wire.wind_directions = view
        .wind
        .iter()
        .map(|span| WindSector {
            from_deg: span.from_deg,
            to_deg: span.to_deg,
        })
        .collect();
    Some(wire.into_site(checking_id()?))
}

/// What differs between a site as one revision said it and as another
/// did, a line per field: "Height: 1862 m → 1871 m". For two revisions a
/// history does not show side by side, and across its pages. Empty when
/// they say the same.
#[uniffi::export]
pub fn site_changes_between(before: SiteView, after: SiteView) -> Vec<String> {
    match (record_of(&before), record_of(&after)) {
        (Some(before), Some(after)) => changes(&before, &after),
        _ => Vec::new(),
    }
}

/// Reads the server's answer to [`site_history_path`]: a page of a
/// site's revisions, each in words, with what it changed.
///
/// # Errors
///
/// The server's refusal in words: that there is no such site, or that
/// whoever runs Venti took it down, which shows nothing of it.
#[uniffi::export]
pub fn site_history(status: u16, body: Vec<u8>) -> Result<SiteHistory, CoreError> {
    let response: SiteHistoryResponse = read(status, &body)?;
    let mut revisions: Vec<SiteRevisionView> = Vec::with_capacity(response.revisions.len());
    let mut before: Option<&api::SiteRevision> = None;
    for revision in &response.revisions {
        let by = revision.by.as_ref().map(author);
        let who = match (&by, revision.via) {
            (Some(author), _) => author_in_words(author),
            (None, SiteVia::Operator) => "whoever runs Venti".to_owned(),
            (None, _) => "a pilot who is no longer on Venti".to_owned(),
        };
        let (was, did) = match (revision.kind, revision.restores) {
            (SiteRevisionKind::Created, _) => (SiteRevisionWas::Added, "Added".to_owned()),
            (SiteRevisionKind::Changed, _) => (SiteRevisionWas::Changed, "Changed".to_owned()),
            (SiteRevisionKind::Restored, Some(number)) => (
                SiteRevisionWas::PutBack,
                format!("Put back to revision {number}"),
            ),
            (SiteRevisionKind::Restored, None) => (SiteRevisionWas::PutBack, "Put back".to_owned()),
            (SiteRevisionKind::Removed, _) => (SiteRevisionWas::Removed, "Removed".to_owned()),
            (SiteRevisionKind::Other, _) => (SiteRevisionWas::Other, "Revised".to_owned()),
        };
        let mut words = format!("{did} by {who} on {}", day(revision.at));
        if let Some(comment) = &revision.comment {
            words.push_str(": ");
            words.push_str(&quoted(comment));
        }
        // Against the revision before, when it is on this page and this
        // one says anything new: a removal says what stood until then.
        let changed = match before {
            Some(before)
                if before.number + 1 == revision.number
                    && revision.kind != SiteRevisionKind::Removed =>
            {
                changes(&before.site, &revision.site)
            }
            _ => Vec::new(),
        };
        let is_current = revision.number == response.revision;
        revisions.push(SiteRevisionView {
            number: revision.number,
            at_unix_s: revision.at.timestamp(),
            by,
            was,
            restores: revision.restores,
            comment: revision.comment.clone(),
            words,
            changes: changed,
            site: site_view(&revision.site, None),
            is_current,
            can_be_put_back: !is_current || response.removed,
        });
        before = Some(revision);
    }
    // Undoing the last change is putting back the revision before it,
    // when that one is here to be seen and did not remove the site.
    let undo_revision = response
        .revision
        .checked_sub(1)
        .filter(|number| *number >= 1)
        .filter(|number| {
            revisions
                .iter()
                .any(|one| one.number == *number && one.was != SiteRevisionWas::Removed)
        });
    Ok(SiteHistory {
        site_id: response.id.to_string(),
        revision: response.revision,
        is_removed: response.removed,
        next_after: response
            .more
            .then(|| response.revisions.last().map(|last| last.number))
            .flatten(),
        undo_words: if undo_revision.is_some() {
            "Undo the last change".to_owned()
        } else {
            String::new()
        },
        undo_revision,
        revisions,
        credit: response.attributions.first().map(credit),
    })
}

/// The compass points a draft has ticked, in words: "SW to NW". Empty
/// when none is ticked. For the line under the form's wind rose.
#[uniffi::export]
pub fn site_draft_wind_in_words(wind_from_deg: Vec<f64>) -> String {
    wind_in_words(&sectors_of(&wind_from_deg))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(result: Result<impl std::fmt::Debug, CoreError>) -> String {
        match result {
            Err(CoreError::Server { message }) => message,
            other => panic!("{other:?}"),
        }
    }

    /// An example of the server's documentation, as its tests keep it.
    fn golden(name: &str) -> Vec<u8> {
        let path = format!(
            "{}/../server/tests/golden/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read(path).unwrap()
    }

    fn json(bytes: &[u8]) -> serde_json::Value {
        serde_json::from_slice(bytes).unwrap()
    }

    fn body(request: &ServerRequest) -> serde_json::Value {
        json(request.body.as_ref().unwrap())
    }

    /// The take-off of the documentation's example of adding a site.
    fn a_takeoff() -> SiteDraftView {
        SiteDraftView {
            name: "Beispielkopf Süd".to_owned(),
            elevation_m: Some(1540.0),
            uses: vec![SiteKind::Takeoff],
            wind_from_deg: vec![135.0, 157.5, 180.0, 202.5, 225.0],
            description: "Made up, for the documentation. No such place exists.".to_owned(),
            paragliding: Some(true),
            ..site_draft_new(Coordinate {
                lat: 47.4,
                lon: 11.2,
            })
        }
    }

    #[test]
    fn a_site_is_added_as_the_server_reads_it() {
        let request = site_add_request(
            a_takeoff(),
            " From where I stood on 14 November.\n".to_owned(),
        )
        .unwrap();
        assert_eq!(
            (request.method.as_str(), request.path.as_str()),
            ("POST", "/v1/sites")
        );
        assert!(request.needs_token);
        assert_eq!(request.content_type.as_deref(), Some("application/json"));
        // The example of the server's documentation, word for word.
        assert_eq!(body(&request), json(&golden("site_add_request.json")));
        assert!(site_draft_check(a_takeoff(), String::new()).is_empty());
        // What the form does not say is not sent.
        let sent = body(&request);
        for unsaid in ["fee", "website", "official", "outline", "hang_gliding"] {
            assert!(sent["site"].get(unsaid).is_none(), "{unsaid}");
        }
    }

    #[test]
    fn ticked_points_side_by_side_are_one_span_and_come_back_as_they_were() {
        let spans = |points: &[f64]| -> Vec<(f64, f64)> {
            sectors_of(points)
                .iter()
                .map(|sector| (sector.from_deg, sector.to_deg))
                .collect()
        };
        assert_eq!(spans(&[]), []);
        assert_eq!(spans(&[180.0]), [(180.0, 180.0)]);
        assert_eq!(spans(&[225.0, 180.0, 202.5]), [(180.0, 225.0)]);
        // Two runs, and one across north.
        assert_eq!(spans(&[90.0, 180.0, 202.5]), [(90.0, 90.0), (180.0, 202.5)]);
        assert_eq!(spans(&[337.5, 0.0, 22.5]), [(337.5, 22.5)]);
        let all: Vec<f64> = (0..16).map(|point| f64::from(point) * 22.5).collect();
        assert_eq!(spans(&all), [(0.0, 360.0)]);
        // What is not on a point goes to the nearest, and no number to none.
        assert_eq!(
            spans(&[185.0, f64::NAN, 359.0]),
            [(0.0, 0.0), (180.0, 180.0)]
        );

        // The points of spans are the points inside them, across north too.
        let points = |from_deg, to_deg| points_of(&[WindSector { from_deg, to_deg }]);
        assert_eq!(points(225.0, 315.0), [225.0, 247.5, 270.0, 292.5, 315.0]);
        assert_eq!(points(337.5, 22.5), [0.0, 22.5, 337.5]);
        assert_eq!(points(200.0, 200.0), [202.5]);
        assert_eq!(points(0.0, 360.0).len(), 16);
        // And a span made of points comes back as the same span.
        for (from_deg, to_deg) in [(225.0, 315.0), (337.5, 22.5), (90.0, 90.0), (0.0, 360.0)] {
            assert_eq!(spans(&points(from_deg, to_deg)), [(from_deg, to_deg)]);
        }
        assert_eq!(
            site_draft_wind_in_words(vec![225.0, 247.5, 270.0, 90.0]),
            "E, SW to W"
        );
        assert_eq!(site_draft_wind_in_words(Vec::new()), "");
    }

    #[test]
    fn what_the_server_would_refuse_is_said_field_by_field_before_sending() {
        let nothing = site_draft_new(Coordinate {
            lat: 47.4,
            lon: 11.2,
        });
        let faults = site_draft_check(nothing.clone(), String::new());
        assert_eq!(
            faults,
            [
                fault(SiteField::Name, "Give the site a name."),
                fault(
                    SiteField::Uses,
                    "Say what the site is used for: take-off, landing, winch site or training slope."
                ),
            ]
        );
        assert_eq!(
            words(site_add_request(nothing, String::new())),
            "Give the site a name."
        );

        let wrong = SiteDraftView {
            name: "x".repeat(201),
            elevation_m: Some(9001.0),
            uses: vec![SiteKind::Other],
            website: "www.example.org".to_owned(),
            description: "x".repeat(1001),
            fee: "x".repeat(101),
            no_flight_time: "x".repeat(101),
            ..a_takeoff()
        };
        let faults = site_draft_check(wrong, "x".repeat(201));
        let said: Vec<(SiteField, &str)> = faults
            .iter()
            .map(|fault| (fault.field, fault.words.as_str()))
            .collect();
        assert_eq!(
            said,
            [
                (
                    SiteField::Uses,
                    "Say what the site is used for: take-off, landing, winch site or training slope."
                ),
                (SiteField::Name, "A name has room for 200 characters."),
                (
                    SiteField::Height,
                    "A height is between -500 and 9000 m above sea level."
                ),
                (
                    SiteField::Website,
                    "A web page's address begins with https:// or http://, has no blanks and at most 500 characters."
                ),
                (
                    SiteField::Description,
                    "The description has room for 1000 characters."
                ),
                (SiteField::Fee, "The fee has room for 100 characters."),
                (
                    SiteField::NoFlightTime,
                    "This line has room for 100 characters."
                ),
                (SiteField::Comment, "This line has room for 200 characters."),
            ]
        );
        // At the limits everything is taken.
        let full = SiteDraftView {
            name: "x".repeat(200),
            elevation_m: Some(9000.0),
            description: "x".repeat(1000),
            fee: "x".repeat(100),
            website: "http://example.org/a?b#c".to_owned(),
            ..a_takeoff()
        };
        assert_eq!(site_draft_check(full, "x".repeat(200)), []);
        // No place on the globe is no place to send.
        let nowhere = SiteDraftView {
            position: Coordinate {
                lat: 91.0,
                lon: 11.2,
            },
            ..a_takeoff()
        };
        assert_eq!(
            site_draft_check(nowhere, String::new()),
            [fault(SiteField::Place, "Mark the place on the map.")]
        );
    }

    #[test]
    fn the_form_names_every_field_and_the_limits_the_server_has() {
        let form = site_form();
        let asked: Vec<SiteField> = form.fields.iter().map(|field| field.field).collect();
        assert_eq!(
            asked,
            [
                SiteField::Name,
                SiteField::Place,
                SiteField::Uses,
                SiteField::Height,
                SiteField::Wind,
                SiteField::Description,
                SiteField::Fee,
                SiteField::Guests,
                SiteField::NoFlightTime,
                SiteField::Website,
                SiteField::Comment,
            ]
        );
        for field in &form.fields {
            assert!(
                !field.title.is_empty() && field.help.ends_with('.'),
                "{field:?}"
            );
        }
        let most = |asked: SiteField| {
            form.fields
                .iter()
                .find(|field| field.field == asked)
                .and_then(|field| field.at_most)
        };
        assert_eq!(most(SiteField::Name), Some(200));
        assert_eq!(most(SiteField::Description), Some(1000));
        assert_eq!(most(SiteField::Comment), Some(200));
        assert_eq!(most(SiteField::Place), None);
        assert_eq!((form.lowest_m, form.highest_m), (-500.0, 9000.0));
        // The four uses a pilot can say, and the filter's sixteen winds.
        let uses: Vec<SiteKind> = form.uses.iter().map(|choice| choice.kind).collect();
        assert_eq!(
            uses,
            [
                SiteKind::Takeoff,
                SiteKind::Landing,
                SiteKind::Towing,
                SiteKind::Training
            ]
        );
        assert_eq!(form.winds, crate::sites_filter_choices().winds);
        assert_eq!(form.winds.len(), 16);
        assert_eq!(form.guests.len(), 3);
        // What giving means: the licence, for everyone, with the handle,
        // and for good.
        assert_eq!(
            form.giving,
            "You give this site to everyone under CC0 1.0, the public domain: anyone may use it and change it, on Venti and anywhere else. It is shown with your handle, any pilot can correct it, and it stays on Venti when your account goes."
        );
        assert_eq!(form.licence, "CC0 1.0");
        assert!(form.licence_url.starts_with("https://creativecommons.org/"));
        assert_eq!(
            form.note,
            "Added by pilots on Venti and not verified. A site on the map is not permission to fly there."
        );
    }

    #[test]
    fn a_site_is_read_corrected_and_sent_from_the_revision_it_stood_at() {
        // The site of the documentation, as its address answers it.
        let page = site_page(200, golden("site.json")).unwrap();
        assert!(page.is_lasting && page.can_be_corrected);
        assert_eq!(page.credit.as_ref().unwrap().licence, "CC0 1.0");
        let standing = page.site.by_pilots.clone().unwrap();
        let draft = site_draft_of(page.site.clone()).unwrap();
        assert_eq!(draft.site_id.as_deref(), Some(page.site.id.as_str()));
        assert_eq!(draft.revision, Some(standing.revision));
        assert_eq!(draft.name, page.site.name);
        assert_eq!(site_draft_check(draft.clone(), String::new()), []);

        // Its height is corrected: the whole record goes, with the
        // revision the draft was made from.
        let corrected = SiteDraftView {
            elevation_m: Some(1871.0),
            ..draft.clone()
        };
        let request = site_change_request(corrected, "Measured on the spot.".to_owned()).unwrap();
        assert_eq!(request.method, "PUT");
        assert_eq!(request.path, format!("/v1/sites/{}", page.site.id));
        assert!(request.needs_token);
        let sent = body(&request);
        assert_eq!(sent["revision"], standing.revision);
        assert_eq!(sent["comment"], "Measured on the spot.");
        assert_eq!(sent["site"]["elevation_m"], 1871.0);
        // Everything else is what the site said: sent unchanged, the
        // record comes back the same.
        let unchanged = body(&site_change_request(draft.clone(), String::new()).unwrap());
        let written: serde_json::Value = json(&golden("site.json"));
        for field in ["name", "position", "uses", "wind_directions", "elevation_m"] {
            assert_eq!(unchanged["site"][field], written["site"][field], "{field}");
        }
        assert!(unchanged.get("comment").is_none());

        // A new site is not corrected, and a source's site not here.
        assert_eq!(
            words(site_change_request(a_takeoff(), String::new())),
            "This site is new: add it instead of correcting it."
        );
        let of_a_source = site_page(200, golden("site_of_a_source.json")).unwrap();
        assert!(!of_a_source.can_be_corrected && !of_a_source.is_lasting);
        assert!(words(site_draft_of(of_a_source.site.clone())).contains("corrected at its source"));
        assert!(
            words(site_remove_request(of_a_source.site.id, String::new()))
                .contains("corrected at its source")
        );

        // Moved, an outline goes; left where it is, it stays.
        let field = SiteDraftView {
            outline: vec![
                Coordinate {
                    lat: 47.0,
                    lon: 11.0,
                },
                Coordinate {
                    lat: 47.001,
                    lon: 11.0,
                },
                Coordinate {
                    lat: 47.001,
                    lon: 11.001,
                },
                Coordinate {
                    lat: 47.0,
                    lon: 11.0,
                },
            ],
            ..draft
        };
        let same = site_draft_moved(field.clone(), field.position);
        assert_eq!(same.outline.len(), 4);
        let moved = site_draft_moved(
            field,
            Coordinate {
                lat: 47.31,
                lon: 10.91,
            },
        );
        assert!(moved.outline.is_empty());
        assert_eq!(moved.position.lat, 47.31);
    }

    #[test]
    fn a_site_is_removed_and_put_back_with_a_line_that_says_why() {
        let id = "venti:0000000000000000000000s001";
        let request = site_remove_request(id.to_owned(), " Added twice. ".to_owned()).unwrap();
        assert_eq!(
            (request.method.as_str(), request.path.as_str()),
            ("DELETE", "/v1/sites/venti:0000000000000000000000s001")
        );
        assert_eq!(
            body(&request),
            serde_json::json!({"comment": "Added twice."})
        );
        assert_eq!(
            site_removed(204, Vec::new()).unwrap(),
            "Removed from the map. Its history stays, and any pilot can bring it back."
        );

        let request =
            site_restore_request(id.to_owned(), 2, "The height was right before.".to_owned())
                .unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(
            request.path,
            "/v1/sites/venti:0000000000000000000000s001/revisions"
        );
        // The example of the server's documentation.
        assert_eq!(body(&request), json(&golden("site_restore_request.json")));
        assert_eq!(
            words(site_restore_request(id.to_owned(), 0, String::new())),
            "The site has no such revision."
        );
        assert_eq!(
            words(site_remove_request(id.to_owned(), "x".repeat(201))),
            "This line has room for 200 characters."
        );

        assert_eq!(
            site_path(id.to_owned()).unwrap(),
            "/v1/sites/venti:0000000000000000000000s001"
        );
        assert_eq!(
            site_history_path(id.to_owned(), None).unwrap(),
            "/v1/sites/venti:0000000000000000000000s001/history"
        );
        assert_eq!(
            site_history_path(id.to_owned(), Some(50)).unwrap(),
            "/v1/sites/venti:0000000000000000000000s001/history?after=50"
        );
        assert!(site_history_path("osm-free-flying:node/1".to_owned(), None).is_err());

        // What was written is read back, with what became of it.
        let written = site_written(200, golden("site_written.json")).unwrap();
        let revision = written.site.by_pilots.as_ref().unwrap().revision;
        assert!(revision > 1);
        assert_eq!(
            written.words,
            format!("Saved for everyone, as revision {revision} of this site.")
        );
        let added = site_written(201, golden("site_written.json")).unwrap();
        assert_eq!(
            added.words,
            "Added. The site is on the map for everyone now."
        );
    }

    #[test]
    fn a_history_says_each_revision_in_words_with_what_it_changed() {
        // The history of the documentation: added, changed for the worse,
        // put back.
        let history = site_history(200, golden("site_history.json")).unwrap();
        assert_eq!(history.revisions.len(), 3);
        assert_eq!(history.revision, 3);
        assert!(!history.is_removed);
        assert_eq!(history.next_after, None);
        let lines: Vec<&str> = history
            .revisions
            .iter()
            .map(|revision| revision.words.as_str())
            .collect();
        let raw = json(&golden("site_history.json"));
        let comment = |index: usize| {
            raw["revisions"][index]["comment"]
                .as_str()
                .unwrap()
                .to_owned()
        };
        assert!(lines[0].starts_with("Added by "), "{}", lines[0]);
        assert!(lines[0].contains(" on "), "{}", lines[0]);
        assert!(
            lines[0].ends_with(&format!(": \u{201c}{}\u{201d}", comment(0))),
            "{}",
            lines[0]
        );
        assert!(lines[1].starts_with("Changed by "), "{}", lines[1]);
        assert!(
            lines[2].starts_with("Put back to revision 1 by "),
            "{}",
            lines[2]
        );
        let kinds: Vec<SiteRevisionWas> = history.revisions.iter().map(|one| one.was).collect();
        assert_eq!(
            kinds,
            [
                SiteRevisionWas::Added,
                SiteRevisionWas::Changed,
                SiteRevisionWas::PutBack
            ]
        );
        // The first changed nothing; the others say what, field by field,
        // and putting back is the same lines the other way round.
        assert!(history.revisions[0].changes.is_empty());
        let changed = &history.revisions[1].changes;
        assert!(!changed.is_empty());
        for line in changed {
            assert!(line.contains(": "), "{line}");
        }
        assert_eq!(
            history.revisions[2].changes.len(),
            changed.len(),
            "{:?}",
            history.revisions[2].changes
        );
        assert_eq!(
            site_changes_between(
                history.revisions[0].site.clone(),
                history.revisions[1].site.clone()
            ),
            *changed
        );
        assert!(
            site_changes_between(
                history.revisions[0].site.clone(),
                history.revisions[2].site.clone()
            )
            .is_empty()
        );
        // The one it stands at cannot be put back; the others can, and
        // the last change is undone by the one before it.
        let can: Vec<bool> = history
            .revisions
            .iter()
            .map(|one| one.can_be_put_back)
            .collect();
        assert_eq!(can, [true, true, false]);
        assert!(history.revisions[2].is_current);
        assert_eq!(history.undo_revision, Some(2));
        assert_eq!(history.undo_words, "Undo the last change");
        assert_eq!(history.credit.unwrap().licence, "CC0 1.0");
    }

    #[test]
    fn what_changed_is_said_in_the_words_of_the_form() {
        let site = |change: &dyn Fn(&mut SiteDraftView)| {
            let mut draft = a_takeoff();
            change(&mut draft);
            wire(&draft).unwrap().into_site(checking_id().unwrap())
        };
        let before = site(&|_| {});
        let after = site(&|draft| {
            draft.name = "Beispielkopf".to_owned();
            draft.position = Coordinate {
                lat: 47.4003,
                lon: 11.2,
            };
            draft.elevation_m = Some(1571.0);
            draft.uses = vec![SiteKind::Takeoff, SiteKind::Landing];
            draft.wind_from_deg = vec![180.0];
            draft.description = "Steep.".to_owned();
            draft.fee = "no".to_owned();
            draft.guest_guidelines = Some(true);
        });
        assert_eq!(
            changes(&before, &after),
            [
                "Name: \u{201c}Beispielkopf Süd\u{201d} \u{2192} \u{201c}Beispielkopf\u{201d}",
                "Place: moved by 30 m",
                "Used for: Take-off \u{2192} Take-off and landing",
                "Height: 1540 m \u{2192} 1571 m",
                "Wind directions: SE to SW \u{2192} S",
                "Description: rewritten",
                "Fee: none \u{2192} \u{201c}no\u{201d}",
                "Rules for guests: not said \u{2192} yes",
            ]
        );
        assert!(changes(&before, &before).is_empty());
    }

    #[test]
    fn every_refusal_of_a_site_is_said_in_words_for_the_pilot() {
        let refused = |status: u16, code: &str| {
            let body = format!(r#"{{"error":{{"code":"{code}","message":"x"}}}}"#);
            words(site_written(status, body.into_bytes()))
        };
        assert_eq!(
            refused(400, "site_out_of_area"),
            "This place lies outside the area Venti takes sites for."
        );
        assert_eq!(
            refused(409, "site_changed"),
            "Someone else changed this site while you were editing it. Look at how it stands now, then make your change again."
        );
        assert!(refused(410, "site_gone").starts_with("This site was removed."));
        assert!(
            refused(403, "sites_closed").starts_with("This server takes no sites from pilots.")
        );
        assert_eq!(
            refused(403, "publishing_suspended"),
            "This account was stopped from publishing on Venti. It can add and change no site."
        );
        assert_eq!(
            refused(429, "rate_limited"),
            "You have added or changed as many sites today as one account may. Try again tomorrow."
        );
        assert!(refused(400, "invalid_site").starts_with("The server does not accept this site."));
        assert_eq!(refused(404, "site_not_found"), "There is no such site.");
        assert_eq!(
            refused(404, "site_revision_not_found"),
            "The site has no such revision."
        );
        // What every request shares is said as everywhere.
        assert_eq!(
            refused(401, "unauthorized"),
            "This device is no longer signed in. Sign in again."
        );
        // Without the other site's name the words still say what to do.
        assert_eq!(
            refused(409, "site_too_close"),
            "A site that pilots added stands at this place already. If it is the same place, correct that one."
        );

        // The example of the server's documentation names the other site.
        let too_close = golden("site_too_close.json");
        let other = site_in_the_way(409, too_close.clone()).unwrap();
        assert_eq!(other.name, "Musterkogel West");
        assert_eq!(other.words, "Open Musterkogel West");
        assert!(other.id.starts_with("venti:"));
        assert_eq!(
            words(site_written(409, too_close.clone())),
            "A site that pilots added stands at this place already: Musterkogel West. If it is the same place, correct that one."
        );
        assert_eq!(site_page(200, too_close.clone()).ok(), None);
        assert_eq!(site_in_the_way(200, too_close), None);
        let other_error = br#"{"error":{"code":"site_changed","message":"x"}}"#.to_vec();
        assert_eq!(site_in_the_way(409, other_error), None);
    }
}
