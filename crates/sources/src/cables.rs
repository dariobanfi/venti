//! Running a source of cables.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use venti_domain::{Cable, SourceId, Timestamp};

use crate::run::fetch_all;
use crate::sites::SiteRejection;
use crate::{Archive, CableSource, Failure, Fetch, Fetched, PlanCtx, Request, RunOptions, Skipped};

/// What happened in a cable run. This is the source's health record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CableRunReport {
    /// The source that ran.
    pub source: SourceId,
    /// Requests made.
    pub requests: usize,
    /// Requests that failed, or whose response could not be parsed or archived.
    pub failures: Vec<Failure>,
    /// Records, or parts of responses, the adapter left out.
    pub skipped: Vec<Skipped>,
    /// Records and values removed by validation. A rejection names the
    /// cable in its `site` field.
    pub rejected: Vec<SiteRejection>,
    /// Cables dropped because an earlier one had the same key.
    pub duplicates: usize,
    /// Whether the run stopped early because it used up its request budget.
    pub budget_exhausted: bool,
    /// Cables delivered.
    pub cables: usize,
}

impl CableRunReport {
    /// Whether the set this run delivered may replace the one before: as
    /// for sites, only when every request worked, so that a set with a
    /// hole in it never replaces a whole one.
    pub fn is_healthy(&self) -> bool {
        self.cables > 0 && self.failures.is_empty() && !self.budget_exhausted
    }
}

/// The canonical records a cable run produced, and its report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CableRunOutput {
    /// Cables, sorted by their key.
    pub cables: Vec<Cable>,
    /// The date the set is of, as for sites: the oldest date among the
    /// responses, never later than the run.
    pub as_of: Timestamp,
    /// What happened.
    pub report: CableRunReport,
}

/// Runs one cable source once: fetch, archive, parse, validate.
///
/// The result is a snapshot: every cable of the source that reaches into
/// `ctx.area`, sorted by key. A cable whose line lies wholly outside the
/// area is dropped without a word. A cable that names another source, one
/// that cannot be a line, and a second one with a key already seen are
/// removed whole and reported.
pub async fn run_cables(
    source: &dyn CableSource,
    fetch: &impl Fetch,
    archive: &impl Archive,
    ctx: &PlanCtx,
    options: &RunOptions,
) -> CableRunOutput {
    let manifest = source.manifest();
    let mut skipped = Vec::new();
    let mut found: Vec<Cable> = Vec::new();
    let mut as_of: Option<Timestamp> = None;

    let take = |request: &Request, response: &Fetched| {
        let parsed = source.parse(request, &response.body, ctx)?;
        let date = parsed
            .as_of
            .or(response.last_modified)
            .unwrap_or(ctx.now)
            .min(ctx.now);
        as_of = Some(as_of.map_or(date, |oldest| oldest.min(date)));
        found.extend(parsed.cables);
        skipped.extend(parsed.skipped);
        Ok(Vec::new())
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
    let mut cables: BTreeMap<String, Cable> = BTreeMap::new();
    for mut cable in found {
        let key = format!("{}:{}", cable.source, cable.local);
        let reject = |reason: String| SiteRejection {
            site: key.clone(),
            field: "record".to_owned(),
            reason,
        };
        if cable.source != manifest.id {
            rejected.push(reject(format!(
                "the cable does not belong to the source {}",
                manifest.id
            )));
            continue;
        }
        match cable.check() {
            Err(reason) => {
                rejected.push(reject(reason));
                continue;
            }
            Ok(removed) => rejected.extend(removed.into_iter().map(|field| SiteRejection {
                site: key.clone(),
                field: field.to_owned(),
                reason: "it cannot be right".to_owned(),
            })),
        }
        if !cable.touches(&ctx.area) {
            continue;
        }
        if cables.contains_key(&cable.local) {
            duplicates += 1;
            continue;
        }
        cables.insert(cable.local.clone(), cable);
    }

    let cables: Vec<Cable> = cables.into_values().collect();
    let report = CableRunReport {
        source: manifest.id.clone(),
        requests: fetched.requests,
        failures: fetched.failures,
        skipped,
        rejected,
        duplicates,
        budget_exhausted: fetched.budget_exhausted,
        cables: cables.len(),
    };
    CableRunOutput {
        cables,
        as_of: as_of.unwrap_or(ctx.now),
        report,
    }
}
