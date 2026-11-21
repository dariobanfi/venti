use serde::{Deserialize, Serialize};
use venti_domain::{
    BoundingBox, GeoPoint, Site, SiteId, SiteOutline, SiteUse, SourceId, Timestamp, WindSector,
};

use super::{Attribution, Handle, text};

/// The source of the sites that pilots make on Venti: the layer of the
/// community's own sites, beside the layers of the sources Venti collects.
/// Every such site's identifier is `venti:` and 26 characters.
pub const VENTI_SITES_SOURCE: &str = "venti";

/// The longest line a revision may carry that says why it was made, in
/// characters.
pub const MAX_SITE_COMMENT_LENGTH: usize = 200;

/// The answer of `GET /v1/sites`.
///
/// Flying sites come in layers, one per data source, as airspace does. The
/// layers are never merged: each is under its own licence, with its own
/// credit and its own date, and a client that stores or draws sites keeps
/// them apart in the same way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SitesResponse {
    /// One layer for each source the server holds sites of, sorted by
    /// source. A source with nothing in the requested box is a layer
    /// without sites: it still says how old its set is and where the set
    /// is whole.
    pub layers: Vec<SiteLayer>,
    /// The credit for every source that has a layer in the answer, with
    /// what its licence asks a client to show.
    pub attributions: Vec<Attribution>,
}

/// The flying sites of one source inside the requested box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteLayer {
    /// The source. Its credit is in the response's attributions, and every
    /// site of the layer names it in its identifier.
    pub source: SourceId,
    /// When the server fetched the set this layer is a part of.
    pub fetched_at: Timestamp,
    /// The date the source's data is of, by the source's own word: for
    /// OpenStreetMap, the moment the copy that answered was taken. Show
    /// the pilot how old that is, measured against the server's clock.
    pub as_of: Timestamp,
    /// Where the server's set of this source is whole: inside these boxes
    /// it holds every site the source has. Outside them it holds nothing.
    ///
    /// This describes the server's set, not the request: it is the same
    /// whatever box was asked for. An empty list means the server does
    /// not say, which is not covered.
    ///
    /// **Whole is not complete.** The set is everything the source has,
    /// and a source that anyone can edit has what somebody entered. Where
    /// a layer shows no site there may well be one.
    #[serde(default)]
    pub covers: Vec<BoundingBox>,
    /// The canonical records, sorted by identifier: every site of the
    /// server's set inside the requested box, edges included. Empty when
    /// there is none.
    pub sites: Vec<Site>,
    /// For a layer of sites that pilots made ([`VENTI_SITES_SOURCE`]): who
    /// made each site of `sites` and when it last changed, one entry per
    /// site, in the same order. Empty for a layer that is a source's whole
    /// set, whose records have no author and no history here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub standing: Vec<SiteStanding>,
}

/// A pilot as a site's history names them: by handle and by the name they
/// are shown by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SitePilot {
    /// The pilot's handle.
    pub handle: Handle,
    /// The name the pilot is shown by.
    pub display_name: String,
}

/// Where a site that pilots made stands: its revision, who made it and who
/// changed it last.
///
/// A pilot is missing where the account is gone: what was given stays, and
/// loses its author's name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteStanding {
    /// The site.
    pub id: SiteId,
    /// The number of the revision the site stands at. The first is 1.
    pub revision: u32,
    /// When the site was made.
    pub created_at: Timestamp,
    /// Who made it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<SitePilot>,
    /// When it last changed: the moment of the revision it stands at.
    pub changed_at: Timestamp,
    /// Who made that revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed_by: Option<SitePilot>,
}

/// The answer of `GET /v1/sites/{id}`, and of every request that writes a
/// site: one site as it stands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteResponse {
    /// The canonical record.
    pub site: Site,
    /// For a site that pilots made: where it stands. Absent for a site of
    /// a source's whole set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standing: Option<SiteStanding>,
    /// Whether the address is for good. `true` for a site that pilots
    /// made: its identifier is the server's own and is never used for
    /// another site, and when the site is gone the address says so.
    /// `false` for a site of a source's whole set: the identifier is the
    /// source's, the record is the one of the set the server holds now,
    /// and when the source no longer has it the address answers that
    /// there is no such site.
    pub lasting: bool,
    /// The credit of the site's source, with the licence the record is
    /// given under.
    pub attributions: Vec<Attribution>,
}

/// What a revision did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteRevisionKind {
    /// It made the site.
    Created,
    /// It changed what the site says.
    Changed,
    /// It put the site back to what an earlier revision said.
    Restored,
    /// It removed the site.
    Removed,
    /// A kind this version of the code does not know.
    #[serde(other)]
    Other,
}

/// Through what a revision came in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteVia {
    /// A session of the pilot's own sign-in: one of Venti's apps.
    Session,
    /// Whoever runs the server, from its command line.
    Operator,
    /// A token the pilot gave to another program. Which token is not said
    /// here: its name is the pilot's own word for a program of theirs.
    Token,
    /// A way this version of the code does not know.
    #[serde(other)]
    Other,
}

/// One revision of a site that pilots made: the whole record as it stood
/// from then on, who made the change and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteRevision {
    /// The revision's number. The first is 1, and each later one is one
    /// more.
    pub number: u32,
    /// When it was made.
    pub at: Timestamp,
    /// Who made it. Absent for a revision of whoever runs the server, and
    /// where the account is gone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<SitePilot>,
    /// Through what it came in.
    pub via: SiteVia,
    /// What it did.
    pub kind: SiteRevisionKind,
    /// For a revision that restored an earlier one: that one's number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restores: Option<u32>,
    /// Why, in its author's words, if they said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// The record as it stood from this revision on. For a revision that
    /// removed the site: as it stood until then.
    pub site: Site,
}

/// The answer of `GET /v1/sites/{id}/history`: one page of a site's
/// revisions, the first one first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteHistoryResponse {
    /// The site.
    pub id: SiteId,
    /// The number of the revision the site stands at.
    pub revision: u32,
    /// Whether the site is removed. A removed site keeps its history, and
    /// a revision that restores an earlier one brings it back.
    pub removed: bool,
    /// The revisions of this page, by number.
    pub revisions: Vec<SiteRevision>,
    /// Whether the site has revisions after the last of this page. Ask
    /// again with `after` set to that one's number.
    pub more: bool,
    /// The credit of the layer, with the licence every revision is given
    /// under.
    pub attributions: Vec<Attribution>,
}

/// A site that pilots made, with where it stands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteRecord {
    /// The canonical record.
    pub site: Site,
    /// Where it stands.
    pub standing: SiteStanding,
}

/// The answer of `GET /v1/sites/changes`: what happened to the sites that
/// pilots made since a program last asked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteChangesResponse {
    /// Sites that are new, changed or back, each as it stands now.
    pub sites: Vec<SiteRecord>,
    /// Sites that are gone. Whoever keeps a copy removes them.
    pub removed: Vec<SiteId>,
    /// Pass this as `since` next time.
    pub cursor: String,
    /// Whether more changes are waiting. Ask again with `cursor` at once.
    pub more: bool,
    /// `true` when the server did not recognise the request's `since`, for
    /// example after it was restored from a backup, and therefore started
    /// again from the beginning. Whoever keeps a copy then replaces it
    /// with what the answers from here on hold.
    pub reset: bool,
    /// The credit of the layer, with the licence its records are given
    /// under.
    pub attributions: Vec<Attribution>,
}

/// What a pilot says about a site: the body of `POST /v1/sites` and of
/// `PUT /v1/sites/{id}`, which replaces all of it.
///
/// It is the canonical record without what a pilot does not say: the
/// identifier and the address of the record are the server's, and whether
/// a site is official or discouraged is not one pilot's to say.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteDraft {
    /// The site's name. One line.
    pub name: String,
    /// Where it is.
    pub position: GeoPoint,
    /// Height above mean sea level in metres, if the pilot knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elevation_m: Option<f64>,
    /// What the site is used for: at least one use.
    pub uses: Vec<SiteUse>,
    /// The wind directions the site works in.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wind_directions: Vec<WindSector>,
    /// Whether the site suits paragliders, if the pilot says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paragliding: Option<bool>,
    /// Whether the site suits hang gliders, if the pilot says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hang_gliding: Option<bool>,
    /// A web page about the site: an `http` or `https` address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    /// What the pilot says about the site in words. It may have several
    /// lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether a payment is asked for using the site: `yes`, `no`, or
    /// other words. One line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fee: Option<String>,
    /// Whether guests have rules to take notice of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guest_guidelines: Option<bool>,
    /// A time in which the site may not be used, in words. One line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_flight_time: Option<String>,
    /// The outline of a site that is a field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<SiteOutline>,
}

impl SiteDraft {
    /// The canonical record a draft makes under an identifier.
    ///
    /// Text is cleaned as all text is that one person types and another
    /// reads: what does not print or would hide text is dropped, the
    /// one-line fields become one line, and a text that is blank after
    /// that is no text. Nothing is checked here: the server's checks are
    /// those every site record passes.
    pub fn into_site(self, id: SiteId) -> Site {
        let one_line = |value: Option<String>| {
            value
                .map(|value| text::line(&value))
                .filter(|value| !value.is_empty())
        };
        let name = text::line(&self.name);
        Site {
            id,
            name: Some(name).filter(|name| !name.is_empty()),
            position: self.position,
            elevation_m: self.elevation_m,
            uses: self.uses,
            wind_directions: self.wind_directions,
            paragliding: self.paragliding,
            hang_gliding: self.hang_gliding,
            official: None,
            discouraged: false,
            url: None,
            website: self
                .website
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
            description: self
                .description
                .map(|value| text::lines(&value))
                .filter(|value| !value.is_empty()),
            fee: one_line(self.fee),
            guest_guidelines: self.guest_guidelines,
            no_flight_time: one_line(self.no_flight_time),
            outline: self.outline,
        }
    }

    /// What a record says, as a draft: the body that would write the same
    /// record again. For an app that lets a pilot correct a site.
    pub fn of(site: &Site) -> Self {
        Self {
            name: site.name.clone().unwrap_or_default(),
            position: site.position,
            elevation_m: site.elevation_m,
            uses: site.uses.clone(),
            wind_directions: site.wind_directions.clone(),
            paragliding: site.paragliding,
            hang_gliding: site.hang_gliding,
            website: site.website.clone(),
            description: site.description.clone(),
            fee: site.fee.clone(),
            guest_guidelines: site.guest_guidelines,
            no_flight_time: site.no_flight_time.clone(),
            outline: site.outline.clone(),
        }
    }
}

/// The line a revision carries that says why it was made, fit to keep:
/// one line, cleaned. `Ok(None)` for none or a blank one; an error with
/// the most characters it may have for one that is too long.
pub fn parse_site_comment(comment: Option<&str>) -> Result<Option<String>, usize> {
    let line = text::line(comment.unwrap_or_default());
    if line.chars().count() > MAX_SITE_COMMENT_LENGTH {
        return Err(MAX_SITE_COMMENT_LENGTH);
    }
    Ok(Some(line).filter(|line| !line.is_empty()))
}

/// The body of `POST /v1/sites`: a site to add.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddSiteRequest {
    /// What the site says.
    pub site: SiteDraft,
    /// Why, or from what knowledge, in one line. Kept with the revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// The body of `PUT /v1/sites/{id}`: a site as it is to stand from now on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeSiteRequest {
    /// What the site says from now on, whole: a field left out is a field
    /// the site no longer has.
    pub site: SiteDraft,
    /// The number of the revision the change was made from. When the site
    /// stands at another one by now, somebody else changed it meanwhile,
    /// and nothing is written.
    pub revision: u32,
    /// Why, in one line. Kept with the revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// The body of `POST /v1/sites/{id}/revisions`: puts a site back to what
/// an earlier revision said, as a new revision. This is how a change is
/// undone, and how a removed site comes back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreSiteRequest {
    /// The number of the revision to restore.
    pub restore: u32,
    /// Why, in one line. Kept with the revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// The body of `DELETE /v1/sites/{id}`, which may be left out.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveSiteRequest {
    /// Why, in one line. Kept with the revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[cfg(test)]
mod tests {
    use venti_domain::SiteUse;

    use super::*;

    #[test]
    fn a_layer_is_read_back_and_what_a_site_does_not_say_is_left_out() {
        let text = r#"{
            "layers": [{
                "source": "osm-free-flying",
                "fetched_at": "2026-11-19T18:10:00Z",
                "as_of": "2026-11-18T21:21:02Z",
                "covers": [{"south": 46.3, "west": 9.4, "north": 48.5, "east": 17.2}],
                "sites": [{
                    "id": "osm-free-flying:node/5964676196",
                    "position": {"lat": 47.6652, "lon": 11.7954},
                    "uses": ["takeoff"],
                    "url": "https://www.openstreetmap.org/node/5964676196",
                    "added_by_a_newer_server": true
                }]
            }],
            "attributions": []
        }"#;

        let answer: SitesResponse = serde_json::from_str(text).unwrap();

        let layer = &answer.layers[0];
        assert_eq!(layer.source.as_str(), "osm-free-flying");
        let site = &layer.sites[0];
        assert_eq!(site.id.source(), &layer.source);
        assert_eq!(site.uses, [SiteUse::Takeoff]);
        assert_eq!(site.name, None);
        assert!(site.wind_directions.is_empty());
        assert!(!site.discouraged);
        let written = serde_json::to_value(&answer).unwrap();
        let site = &written["layers"][0]["sites"][0];
        assert!(site.get("name").is_none());
        assert!(site.get("wind_directions").is_none());
        assert!(site.get("discouraged").is_none());
        assert_eq!(
            serde_json::from_value::<SitesResponse>(written).unwrap(),
            answer
        );

        // What a record gained on 2026-11-21 is read, and a record
        // without it reads as before.
        let first = &answer.layers[0].sites[0];
        assert_eq!(
            (&first.website, &first.description, &first.outline),
            (&None, &None, &None)
        );
        let field: Site = serde_json::from_str(
            r#"{"id": "osm-free-flying:way/1080988126",
                "position": {"lat": 47.6924086, "lon": 11.8865478}, "uses": ["landing"],
                "website": "https://www.dhv.de/", "fee": "yes", "guest_guidelines": true,
                "no_flight_time": "01.04-30.09", "description": "Nicht im hohen Gras landen.",
                "outline": [[11.8880464, 47.6919322], [11.8875034, 47.6925871],
                            [11.8850491, 47.6925639], [11.8880464, 47.6919322]]}"#,
        )
        .unwrap();
        let ring = field.outline.as_ref().unwrap().points();
        assert_eq!(ring.len(), 4);
        assert_eq!((ring[0].lat(), ring[0].lon()), (47.6919322, 11.8880464));
        assert_eq!(field.guest_guidelines, Some(true));
        assert_eq!(field.no_flight_time.as_deref(), Some("01.04-30.09"));

        // An answer that says nothing about where it is whole covers nothing.
        let bare = r#"{"source": "osm-free-flying", "fetched_at": "2026-11-19T18:10:00Z",
                       "as_of": "2026-11-18T21:21:02Z", "sites": []}"#;
        assert!(
            serde_json::from_str::<SiteLayer>(bare)
                .unwrap()
                .covers
                .is_empty()
        );
    }
}
