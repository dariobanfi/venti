//! Running a source of flying sites.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use venti_domain::{Site, SiteId, SourceId, Timestamp};

use crate::run::fetch_all;
use crate::{Archive, Failure, Fetch, Fetched, PlanCtx, Request, RunOptions, SiteSource, Skipped};

/// Lowest and highest site heights accepted, in metres.
const ELEVATION_LIMITS: (f64, f64) = (-500.0, 9000.0);

/// The longest name a site may carry, in characters. The longest in the
/// first region's data of 2026 has 68.
const MAX_NAME_CHARS: usize = 200;

/// The most wind sectors a site may carry: more than the sixteen points of
/// a compass named one by one is a damaged record.
const MAX_SECTORS: usize = 16;

/// The longest web address a site may carry, in characters. OpenStreetMap
/// keeps no value longer than 255.
const MAX_WEBSITE_CHARS: usize = 500;

/// The longest description a site may carry, in characters.
const MAX_DESCRIPTION_CHARS: usize = 1000;

/// The longest word about a fee or a time without flying a site may
/// carry, in characters.
const MAX_REMARK_CHARS: usize = 100;

/// Something about a site that the pipeline refused to pass on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteRejection {
    /// The site's identifier, as text.
    pub site: String,
    /// What was refused: a field of the record, or `record` for the whole
    /// of it.
    pub field: String,
    /// Why.
    pub reason: String,
}

/// What happened in a site run. This is the source's health record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteRunReport {
    /// The source that ran.
    pub source: SourceId,
    /// Requests made.
    pub requests: usize,
    /// Requests that failed, or whose response could not be parsed or archived.
    pub failures: Vec<Failure>,
    /// Records, or parts of responses, the adapter left out.
    pub skipped: Vec<Skipped>,
    /// Records and values removed by validation.
    pub rejected: Vec<SiteRejection>,
    /// Sites dropped because an earlier one had the same identifier.
    pub duplicates: usize,
    /// Whether the run stopped early because it used up its request budget.
    pub budget_exhausted: bool,
    /// Sites delivered.
    pub sites: usize,
}

impl SiteRunReport {
    /// Whether the set this run delivered may replace the one before.
    ///
    /// As for airspace, every request must have worked: a run that could
    /// not fetch a part of its area would otherwise replace a whole set by
    /// one with a hole in it, and a map would show the hole as "no sites
    /// here". Whoever stores the result keeps the previous set after an
    /// unhealthy run. Records the adapter left out or validation rejected
    /// do not make a run unhealthy.
    pub fn is_healthy(&self) -> bool {
        self.sites > 0 && self.failures.is_empty() && !self.budget_exhausted
    }
}

/// The canonical records a site run produced, and its report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteRunOutput {
    /// Sites, sorted by identifier.
    pub sites: Vec<Site>,
    /// The date the set is of: the oldest date among the responses it was
    /// read from. A response is dated by what the adapter found in it,
    /// else by the date the provider's server put on it, else by the time
    /// of the run. It is never later than the run: data cannot be newer
    /// than the moment it was fetched, whatever a provider's clock says.
    pub as_of: Timestamp,
    /// What happened.
    pub report: SiteRunReport,
}

/// Runs one site source once: fetch, archive, parse, validate.
///
/// The result is a snapshot: every site the source has inside `ctx.area`,
/// sorted by identifier. Requests are budgeted and archived as for every
/// other kind of source. `options.since` plays no part.
///
/// Validation removes what cannot be right and keeps the rest of the
/// record: a height no site has, a name longer than any name, wind
/// directions outside the compass. A site outside the area is dropped
/// without a word, like a station outside it. A site that names another
/// source than the one that ran, and a second site with an identifier
/// already seen, are removed whole and reported.
pub async fn run_sites(
    source: &dyn SiteSource,
    fetch: &impl Fetch,
    archive: &impl Archive,
    ctx: &PlanCtx,
    options: &RunOptions,
) -> SiteRunOutput {
    let manifest = source.manifest();
    let mut skipped = Vec::new();
    let mut found: Vec<Site> = Vec::new();
    let mut as_of: Option<Timestamp> = None;

    let take = |request: &Request, response: &Fetched| {
        let parsed = source.parse(request, &response.body, ctx)?;
        let date = parsed
            .as_of
            .or(response.last_modified)
            .unwrap_or(ctx.now)
            .min(ctx.now);
        as_of = Some(as_of.map_or(date, |oldest| oldest.min(date)));
        found.extend(parsed.sites);
        skipped.extend(parsed.skipped);
        Ok(parsed.follow_up)
    };
    let fetched = fetch_all(
        manifest,
        source.plan(ctx),
        fetch,
        archive,
        ctx,
        options,
        take,
    )
    .await;

    let mut rejected = Vec::new();
    let mut duplicates = 0;
    let mut sites: BTreeMap<SiteId, Site> = BTreeMap::new();
    for mut site in found {
        if !ctx.area.contains(site.position) {
            continue;
        }
        if site.id.source() != &manifest.id {
            rejected.push(SiteRejection {
                site: site.id.to_string(),
                field: "record".to_owned(),
                reason: format!("the site does not belong to the source {}", manifest.id),
            });
            continue;
        }
        if sites.contains_key(&site.id) {
            duplicates += 1;
            continue;
        }
        rejected.extend(check_site(&mut site));
        sites.insert(site.id.clone(), site);
    }

    let sites: Vec<Site> = sites.into_values().collect();
    let report = SiteRunReport {
        source: manifest.id.clone(),
        requests: fetched.requests,
        failures: fetched.failures,
        skipped,
        rejected,
        duplicates,
        budget_exhausted: fetched.budget_exhausted,
        sites: sites.len(),
    };
    SiteRunOutput {
        sites,
        as_of: as_of.unwrap_or(ctx.now),
        report,
    }
}

/// Removes from a site what cannot be right, and puts its uses in order.
/// Returns what was removed.
fn check_site(site: &mut Site) -> Vec<SiteRejection> {
    let mut rejections = Vec::new();
    let mut reject = |field: &str, reason: String| {
        rejections.push(SiteRejection {
            site: site.id.to_string(),
            field: field.to_owned(),
            reason,
        });
    };

    site.uses.sort_unstable();
    site.uses.dedup();

    if let Some(name) = &site.name {
        let length = name.chars().count();
        if name.trim().is_empty() {
            site.name = None;
        } else if length > MAX_NAME_CHARS || name.chars().any(char::is_control) {
            reject(
                "name",
                format!("{length} characters, or one that does not print"),
            );
            site.name = None;
        }
    }

    if let Some(value) = site.elevation_m {
        let (low, high) = ELEVATION_LIMITS;
        if !value.is_finite() || !(low..=high).contains(&value) {
            reject("elevation_m", format!("{value} is outside {low}..={high}"));
            site.elevation_m = None;
        }
    }

    let on_the_compass = |deg: f64| deg.is_finite() && (0.0..=360.0).contains(&deg);
    let sectors_fit = site.wind_directions.len() <= MAX_SECTORS
        && site
            .wind_directions
            .iter()
            .all(|sector| on_the_compass(sector.from_deg) && on_the_compass(sector.to_deg));
    if !sectors_fit {
        reject(
            "wind_directions",
            "a direction outside 0..=360, or more sectors than a compass has points".to_owned(),
        );
        site.wind_directions.clear();
    }

    if site
        .url
        .as_ref()
        .is_some_and(|url| !url.starts_with("https://") || url.chars().any(char::is_whitespace))
    {
        reject("url", "not an https address".to_owned());
        site.url = None;
    }

    if site
        .website
        .as_ref()
        .is_some_and(|website| !is_web_address(website))
    {
        reject("website", "not an http or https address".to_owned());
        site.website = None;
    }

    // What somebody wrote about the site: it is shown as written, so what
    // does not print is removed with the whole text, and nothing is cut.
    let unfit = |text: &str, longest: usize, lines: bool| {
        let length = text.chars().count();
        let prints = |c: char| !c.is_control() || (lines && c == '\n');
        (length > longest || !text.chars().all(prints))
            .then(|| format!("{length} characters, or one that does not print"))
    };
    if let Some(reason) = site
        .description
        .as_ref()
        .and_then(|text| unfit(text, MAX_DESCRIPTION_CHARS, true))
    {
        reject("description", reason);
        site.description = None;
    }
    if let Some(reason) = site
        .fee
        .as_ref()
        .and_then(|text| unfit(text, MAX_REMARK_CHARS, false))
    {
        reject("fee", reason);
        site.fee = None;
    }
    if let Some(reason) = site
        .no_flight_time
        .as_ref()
        .and_then(|text| unfit(text, MAX_REMARK_CHARS, false))
    {
        reject("no_flight_time", reason);
        site.no_flight_time = None;
    }
    for text in [
        &mut site.description,
        &mut site.fee,
        &mut site.no_flight_time,
    ] {
        if text.as_ref().is_some_and(|text| text.trim().is_empty()) {
            *text = None;
        }
    }

    rejections
}

/// Whether a text is the address of a web page: `http://` or `https://`,
/// then a host of letters, digits, dots and hyphens with a dot in it and,
/// if it names one, a port, then nothing or a path that begins with `/`,
/// `?` or `#`. No blanks, nothing that does not print, no name and
/// password before the host. An address a browser would also open is
/// refused where it is not of this plain form; that costs a site its link
/// and nothing else.
fn is_web_address(text: &str) -> bool {
    let Some(rest) = text
        .strip_prefix("https://")
        .or_else(|| text.strip_prefix("http://"))
    else {
        return false;
    };
    if text.chars().count() > MAX_WEBSITE_CHARS
        || text.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return false;
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (host, port) = match rest[..end].split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (&rest[..end], None),
    };
    let label = |label: &str| {
        !label.is_empty()
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_alphanumeric() || c == '-')
    };
    host.contains('.')
        && host.split('.').all(label)
        && port.is_none_or(|port| !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use venti_domain::{GeoPoint, SiteUse, WindSector};

    use super::*;

    fn site() -> Site {
        Site {
            id: "example:node/1".parse().unwrap(),
            name: Some("Startplatz Ost".to_owned()),
            position: GeoPoint::new(47.66, 11.8).unwrap(),
            elevation_m: Some(1620.0),
            uses: vec![SiteUse::Takeoff],
            wind_directions: vec![WindSector::single(90.0)],
            paragliding: Some(true),
            hang_gliding: None,
            official: Some(true),
            discouraged: false,
            url: Some("https://example.org/node/1".to_owned()),
            website: Some("https://club.example.org/fluggebiet?id=3#start".to_owned()),
            description: Some("Nicht im hohen Gras landen.\nWindsack vorhanden.".to_owned()),
            fee: Some("yes".to_owned()),
            guest_guidelines: Some(true),
            no_flight_time: Some("01.04-30.09".to_owned()),
            outline: None,
        }
    }

    #[test]
    fn a_sound_site_passes_unchanged() {
        let mut checked = site();
        assert!(check_site(&mut checked).is_empty());
        assert_eq!(checked, site());
    }

    #[test]
    fn what_cannot_be_right_is_removed_and_the_rest_is_kept() {
        let mut checked = Site {
            name: Some("x".repeat(201)),
            elevation_m: Some(16_200.0),
            uses: vec![SiteUse::Landing, SiteUse::Takeoff, SiteUse::Landing],
            wind_directions: vec![WindSector {
                from_deg: 90.0,
                to_deg: 400.0,
            }],
            url: Some("javascript:alert(1)".to_owned()),
            ..site()
        };

        let rejections = check_site(&mut checked);

        let fields: Vec<&str> = rejections.iter().map(|r| r.field.as_str()).collect();
        assert_eq!(fields, ["name", "elevation_m", "wind_directions", "url"]);
        assert_eq!(checked.name, None);
        assert_eq!(checked.elevation_m, None);
        assert!(checked.wind_directions.is_empty());
        assert_eq!(checked.url, None);
        // Each use once, in the order of the type.
        assert_eq!(checked.uses, [SiteUse::Takeoff, SiteUse::Landing]);
        assert_eq!(checked.position, site().position);
    }

    #[test]
    fn what_somebody_wrote_is_kept_as_written_or_removed_whole() {
        let mut checked = Site {
            website: Some("javascript:alert(1)".to_owned()),
            description: Some("x".repeat(1001)),
            fee: Some("yes\u{7}".to_owned()),
            no_flight_time: Some("01.04-30.09\n".to_owned()),
            ..site()
        };

        let rejections = check_site(&mut checked);

        let fields: Vec<&str> = rejections.iter().map(|r| r.field.as_str()).collect();
        assert_eq!(fields, ["website", "description", "fee", "no_flight_time"]);
        assert_eq!(
            (
                &checked.website,
                &checked.description,
                &checked.fee,
                &checked.no_flight_time
            ),
            (&None, &None, &None, &None)
        );
        // What the source says of guests is a yes or a no and stays.
        assert_eq!(checked.guest_guidelines, Some(true));

        // Blank is nothing said, without a complaint.
        let mut blank = Site {
            description: Some(" \n".to_owned()),
            fee: Some(String::new()),
            ..site()
        };
        assert!(check_site(&mut blank).is_empty());
        assert_eq!((blank.description, blank.fee), (None, None));
    }

    #[test]
    fn a_web_address_is_http_or_https_with_a_host() {
        for address in [
            "https://www.gsc-hochries.de/fluggebiet/",
            "http://www.dc-isarwinkel.de/",
            "https://example.org",
            "https://example.org:8443/a?b=c#d",
            "https://www.syride.com//fr/sites/Alp_Tea",
            "https://flugschule-münchen.example/",
        ] {
            assert!(is_web_address(address), "{address}");
        }
        for other in [
            "",
            "www.example.org",
            "ftp://example.org/",
            "javascript:alert(1)",
            "https://",
            "https:///path",
            "https://localhost/",
            "https://user:secret@example.org/",
            "https://example.org /",
            "https://example..org/",
            "https://-example.org/",
            "https://example.org:port/",
            "https://example.org;https://example.com",
            "mailto:someone@example.org",
        ] {
            assert!(!is_web_address(other), "{other}");
        }
        assert!(!is_web_address(&format!(
            "https://example.org/{}",
            "x".repeat(500)
        )));
    }

    #[test]
    fn a_blank_name_is_no_name_and_no_complaint() {
        let mut checked = Site {
            name: Some("  ".to_owned()),
            ..site()
        };
        assert!(check_site(&mut checked).is_empty());
        assert_eq!(checked.name, None);
    }
}
