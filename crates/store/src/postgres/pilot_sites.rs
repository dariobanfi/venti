//! Sites that pilots make, in PostgreSQL (`0013_pilot_sites.sql`).
//!
//! **One write to the layer at a time.** Whatever adds a site or writes a
//! revision first locks the one row of `pilot_site_change`. So two pilots
//! who race for one place get it one after the other, and the change
//! numbers are committed in the order in which they were taken: whoever
//! asks what changed since a number never misses a lower one that is
//! committed later.
//!
//! **The counter first, then the account, then the site.** A call that
//! writes for an account holds that account's row against deletion until
//! it is done, and locks it before the site's row, never after: an
//! account that is deleted at the same moment waits for the call, or the
//! call finds the account gone.
//!
//! **What a revision comes to is decided in [`crate::pilot_site`]**, by
//! the code the in-memory store asks too, and so is which site is too
//! near: the database finds the standing sites in a box around the place,
//! and the distance is measured there.
//!
//! No statement here deletes a row of either table.

use deadpool_postgres::{Client, GenericClient, Transaction};
use tokio_postgres::Row;
use venti_domain::{BoundingBox, Site, SiteId, SourceId};

use super::{corrupt, db_error};
use crate::pilot_site::{
    Decision, Writing, check_confirmation, check_new, check_revising, decide, nearest_within,
    takes_a_place,
};
use crate::{
    NewPilotSite, NewSiteConfirmation, PilotSite, PilotSiteChanges, RevisionKind, SiteAct,
    SiteConfirmation, SiteConfirmed, SiteConfirming, SitePilot, SiteRevising, SiteRevision,
    SiteVia, SiteWrite, StoreError,
};

/// Everything [`site_from`] reads: a site `s`, the account `c` that made
/// it and the account `h` that changed it last, each as it is now and
/// absent once it is gone.
const SITE: &str = "
    SELECT s.record, s.revision, s.created_at, s.changed_at, s.removed_at, s.taken_down,
           s.change,
           (SELECT count(*) FROM site_confirmations k WHERE k.site_id = s.id) AS confirmed_by,
           (SELECT max(k.day) FROM site_confirmations k WHERE k.site_id = s.id) AS confirmed_on,
           c.id AS created_id, c.handle AS created_handle, c.display_name AS created_name,
           h.id AS changed_id, h.handle AS changed_handle, h.display_name AS changed_name
    FROM pilot_sites s
    LEFT JOIN accounts c ON c.id = s.created_by
    LEFT JOIN accounts h ON h.id = s.changed_by";

/// The standing sites whose point lies in a box. `&&` lets the index find
/// them; it compares rounded boxes, so it may find a few more, which is
/// what the caller wants of a box drawn generously.
const STANDING_NEAR: &str = "
    SELECT s.record
    FROM pilot_sites s
    WHERE s.removed_at IS NULL
      AND ST_SetSRID(ST_MakePoint(s.lon, s.lat), 4326)
          && ST_MakeEnvelope($1, $2, $3, $4, 4326)";

const INSERT_SITE: &str = "
    INSERT INTO pilot_sites
        (id, lon, lat, record, revision, created_at, created_by, changed_at, changed_by,
         change)
    VALUES ($1, $2, $3, $4, 1, $5, $6, $5, $6, $7)";

const INSERT_REVISION: &str = "
    INSERT INTO pilot_site_revisions
        (site_id, number, author_id, at, via, via_token, kind, restores, comment, record)
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)";

const UPDATE_SITE: &str = "
    UPDATE pilot_sites
    SET lon = $2, lat = $3, record = $4, revision = $5, changed_at = $6, changed_by = $7,
        removed_at = $8, taken_down = $9, change = $10
    WHERE id = $1";

fn record_from(text: &str) -> Result<Site, StoreError> {
    serde_json::from_str(text).map_err(|e| corrupt("site record", e))
}

fn text_of(record: &Site) -> Result<String, StoreError> {
    serde_json::to_string(record)
        .map_err(|error| StoreError::Failure(format!("a site cannot be written as JSON: {error}")))
}

fn number_from(value: i32) -> Result<u32, StoreError> {
    u32::try_from(value).map_err(|e| corrupt("revision number", e))
}

/// A revision's number as the database keeps it.
fn number_of(value: u32) -> Result<i32, StoreError> {
    i32::try_from(value)
        .map_err(|_| StoreError::Failure("a site has more revisions than it can keep".to_owned()))
}

/// The pilot the three columns name; `None` where the account is gone or
/// there never was one.
fn pilot_from(row: &Row, id: &str, handle: &str, name: &str) -> Option<SitePilot> {
    Some(SitePilot {
        account_id: row.get::<_, Option<String>>(id)?,
        handle: row.get::<_, Option<String>>(handle)?,
        display_name: row.get::<_, Option<String>>(name)?,
    })
}

fn site_from(row: &Row) -> Result<PilotSite, StoreError> {
    Ok(PilotSite {
        site: record_from(row.get("record"))?,
        revision: number_from(row.get("revision"))?,
        created_at: row.get("created_at"),
        created_by: pilot_from(row, "created_id", "created_handle", "created_name"),
        changed_at: row.get("changed_at"),
        changed_by: pilot_from(row, "changed_id", "changed_handle", "changed_name"),
        removed_at: row.get("removed_at"),
        taken_down: row.get("taken_down"),
        change: row.get("change"),
        confirmed: SiteConfirmed {
            pilots: u32::try_from(row.get::<_, i64>("confirmed_by"))
                .map_err(|e| corrupt("count of confirmations", e))?,
            latest_day: row.get("confirmed_on"),
        },
    })
}

fn revision_from(row: &Row) -> Result<SiteRevision, StoreError> {
    let (via, kind): (&str, &str) = (row.get("via"), row.get("kind"));
    let restores = row
        .get::<_, Option<i32>>("restores")
        .map(number_from)
        .transpose()?;
    Ok(SiteRevision {
        number: number_from(row.get("number"))?,
        at: row.get("at"),
        author: pilot_from(row, "author_id", "author_handle", "author_name"),
        via: SiteVia::from_parts(via, row.get("via_token"))
            .ok_or_else(|| corrupt("way a revision came in", via))?,
        kind: RevisionKind::from_parts(kind, restores)
            .ok_or_else(|| corrupt("kind of revision", kind))?,
        comment: row.get("comment"),
        record: record_from(row.get("record"))?,
    })
}

/// Whether an identifier can be the identifier of a stored site. One with
/// a zero byte in it is the identifier of none, which is known without
/// asking a database that would refuse to compare it.
fn can_name(id: &SiteId) -> bool {
    !id.local().contains('\0')
}

async fn read(db: &impl GenericClient, id: &SiteId) -> Result<Option<PilotSite>, StoreError> {
    let query = format!("{SITE} WHERE s.id = $1");
    let statement = db.prepare_cached(&query).await.map_err(db_error)?;
    let row = db
        .query_opt(&statement, &[&id.to_string()])
        .await
        .map_err(db_error)?;
    row.as_ref().map(site_from).transpose()
}

/// A site that the calling transaction has just written.
async fn written(tx: &Transaction<'_>, id: &SiteId) -> Result<SiteWrite, StoreError> {
    let site = read(tx, id)
        .await?
        .ok_or_else(|| corrupt("site", "it is gone while its row is held"))?;
    Ok(SiteWrite::Written(Box::new(site)))
}

/// Locks the layer for writing and returns the largest change number it
/// has used. Every write begins here; see the top of the file.
async fn lock_layer(tx: &Transaction<'_>) -> Result<i64, StoreError> {
    let row = tx
        .query_opt("SELECT latest FROM pilot_site_change FOR UPDATE", &[])
        .await
        .map_err(db_error)?
        .ok_or_else(|| corrupt("count of changes to the sites", "its row is missing"))?;
    Ok(row.get("latest"))
}

/// Takes the next change number of the layer, which [`lock_layer`] holds.
async fn next_change(tx: &Transaction<'_>, latest: i64) -> Result<i64, StoreError> {
    let next = latest
        .checked_add(1)
        .ok_or_else(|| StoreError::Failure("the sites have used every change number".to_owned()))?;
    tx.execute("UPDATE pilot_site_change SET latest = $1", &[&next])
        .await
        .map_err(db_error)?;
    Ok(next)
}

/// Whether the account exists, holding it against deletion until the
/// transaction ends.
async fn hold_account(tx: &Transaction<'_>, id: &str) -> Result<bool, StoreError> {
    if id.contains('\0') {
        return Ok(false);
    }
    let row = tx
        .query_opt("SELECT 1 FROM accounts WHERE id = $1 FOR KEY SHARE", &[&id])
        .await
        .map_err(db_error)?;
    Ok(row.is_some())
}

/// West, south, east and north of a box that holds every place nearer to
/// `record` than `apart_m` metres, drawn generously: a degree of latitude
/// is taken as 100 km, which is less than it is anywhere, and a degree of
/// longitude as what it is at the edge of the box that is nearest a pole.
/// Near a pole, and across the line where east meets west, the box is the
/// whole band around the globe.
fn around(record: &Site, apart_m: f64) -> [f64; 4] {
    let (lat, lon) = (record.position.lat(), record.position.lon());
    let step = apart_m / 100_000.0;
    let south = (lat - step).max(-90.0);
    let north = (lat + step).min(90.0);
    let narrowest = south.abs().max(north.abs()).to_radians().cos();
    let wide = if narrowest > 1e-6 {
        step / narrowest
    } else {
        f64::INFINITY
    };
    if wide.is_finite() && lon - wide >= -180.0 && lon + wide <= 180.0 {
        [lon - wide, south, lon + wide, north]
    } else {
        [-180.0, south, 180.0, north]
    }
}

/// The standing site that is too near to `record`, if one is. The caller
/// holds the layer, so no site comes to stand there meanwhile.
async fn too_close(
    tx: &Transaction<'_>,
    record: &Site,
    apart_m: f64,
) -> Result<Option<SiteId>, StoreError> {
    // No site is nearer than nothing.
    if apart_m.is_nan() || apart_m <= 0.0 {
        return Ok(None);
    }
    let [west, south, east, north] = around(record, apart_m);
    let statement = tx.prepare_cached(STANDING_NEAR).await.map_err(db_error)?;
    let rows = tx
        .query(&statement, &[&west, &south, &east, &north])
        .await
        .map_err(db_error)?;
    let standing = rows
        .iter()
        .map(|row| record_from(row.get("record")))
        .collect::<Result<Vec<Site>, _>>()?;
    Ok(nearest_within(record, &standing, apart_m))
}

/// One revision to store.
struct Kept<'a> {
    id: &'a str,
    number: u32,
    author_id: Option<&'a str>,
    at: venti_domain::Timestamp,
    via: &'a SiteVia,
    kind: RevisionKind,
    comment: Option<&'a str>,
    record: &'a str,
}

async fn insert_revision(tx: &Transaction<'_>, kept: &Kept<'_>) -> Result<(), StoreError> {
    let restores = kept.kind.restores().map(number_of).transpose()?;
    let statement = tx.prepare_cached(INSERT_REVISION).await.map_err(db_error)?;
    tx.execute(
        &statement,
        &[
            &kept.id,
            &number_of(kept.number)?,
            &kept.author_id,
            &kept.at,
            &kept.via.as_str(),
            &kept.via.token(),
            &kept.kind.as_str(),
            &restores,
            &kept.comment,
            &kept.record,
        ],
    )
    .await
    .map_err(db_error)?;
    Ok(())
}

pub(super) async fn add(
    client: &mut Client,
    new: &NewPilotSite,
    apart_m: f64,
) -> Result<SiteWrite, StoreError> {
    check_new(new)?;
    // Dropping the transaction without a commit undoes what was done in it.
    let tx = client.transaction().await.map_err(db_error)?;
    let latest = lock_layer(&tx).await?;
    if !hold_account(&tx, &new.author_id).await? {
        return Ok(SiteWrite::NoSuchAccount);
    }
    let id = new.site.id.to_string();
    // Removed sites are rows too: an identifier is used once.
    let in_use = tx
        .query_opt("SELECT 1 FROM pilot_sites WHERE id = $1", &[&id])
        .await
        .map_err(db_error)?
        .is_some();
    if in_use {
        return Err(StoreError::Invalid(
            "a site under an identifier that is in use".to_owned(),
        ));
    }
    if let Some(other) = too_close(&tx, &new.site, apart_m).await? {
        return Ok(SiteWrite::TooClose(other));
    }

    let change = next_change(&tx, latest).await?;
    let record = text_of(&new.site)?;
    let statement = tx.prepare_cached(INSERT_SITE).await.map_err(db_error)?;
    tx.execute(
        &statement,
        &[
            &id,
            &new.site.position.lon(),
            &new.site.position.lat(),
            &record,
            &new.at,
            &new.author_id,
            &change,
        ],
    )
    .await
    .map_err(db_error)?;
    let first = Kept {
        id: &id,
        number: 1,
        author_id: Some(&new.author_id),
        at: new.at,
        via: &new.via,
        kind: RevisionKind::Created,
        comment: new.comment.as_deref(),
        record: &record,
    };
    insert_revision(&tx, &first).await?;

    let answer = written(&tx, &new.site.id).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(answer)
}

/// The record of the revision of a site with this number, if it has one.
async fn record_of_revision(
    tx: &Transaction<'_>,
    id: &str,
    number: u32,
) -> Result<Option<Site>, StoreError> {
    // A number the database cannot keep is the number of no revision.
    let Ok(number) = i32::try_from(number) else {
        return Ok(None);
    };
    let statement = tx
        .prepare_cached(
            "SELECT r.record FROM pilot_site_revisions r
             WHERE r.site_id = $1 AND r.number = $2",
        )
        .await
        .map_err(db_error)?;
    let row = tx
        .query_opt(&statement, &[&id, &number])
        .await
        .map_err(db_error)?;
    row.map(|row| record_from(row.get("record"))).transpose()
}

pub(super) async fn revise(
    client: &mut Client,
    revising: &SiteRevising,
    apart_m: f64,
) -> Result<SiteWrite, StoreError> {
    check_revising(revising)?;
    let tx = client.transaction().await.map_err(db_error)?;
    let latest = lock_layer(&tx).await?;
    if let Some(author) = &revising.author_id
        && !hold_account(&tx, author).await?
    {
        return Ok(SiteWrite::NoSuchAccount);
    }
    let id = revising.id.to_string();
    // The site's own row is held too, against whatever else may come to
    // write it: the accounts of its authors are joined and not locked.
    let query = format!("{SITE} WHERE s.id = $1 FOR UPDATE OF s");
    let statement = tx.prepare_cached(&query).await.map_err(db_error)?;
    let row = tx.query_opt(&statement, &[&id]).await.map_err(db_error)?;
    let Some(current) = row.as_ref().map(site_from).transpose()? else {
        return Ok(SiteWrite::NoSuchSite);
    };

    let restored = match revising.act {
        SiteAct::Restore(number) => record_of_revision(&tx, &id, number).await?,
        SiteAct::Change(_) | SiteAct::Remove { .. } => None,
    };
    let writing: Writing = match decide(&current, revising, restored.as_ref()) {
        Decision::Answer(answer) => return Ok(answer),
        Decision::Write(writing) => *writing,
    };
    if takes_a_place(&current, &writing)
        && let Some(other) = too_close(&tx, &writing.record, apart_m).await?
    {
        return Ok(SiteWrite::TooClose(other));
    }

    let change = next_change(&tx, latest).await?;
    let number = current.revision.checked_add(1).ok_or_else(|| {
        StoreError::Failure("a site has more revisions than it can keep".to_owned())
    })?;
    let record = text_of(&writing.record)?;
    let revision = Kept {
        id: &id,
        number,
        author_id: revising.author_id.as_deref(),
        at: revising.at,
        via: &revising.via,
        kind: writing.kind,
        comment: revising.comment.as_deref(),
        record: &record,
    };
    insert_revision(&tx, &revision).await?;
    let statement = tx.prepare_cached(UPDATE_SITE).await.map_err(db_error)?;
    tx.execute(
        &statement,
        &[
            &id,
            &writing.record.position.lon(),
            &writing.record.position.lat(),
            &record,
            &number_of(number)?,
            &revising.at,
            &revising.author_id,
            &writing.removed.then_some(revising.at),
            &writing.taken_down,
            &change,
        ],
    )
    .await
    .map_err(db_error)?;

    let answer = written(&tx, &revising.id).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(answer)
}

/// What every identifier of a layer begins with: the layer's name and
/// the colon. No layer's is the beginning of another's, since a name
/// holds no colon.
fn beginning(layer: &SourceId) -> String {
    format!("{layer}:")
}

pub(super) async fn in_box(
    client: &Client,
    layer: &SourceId,
    area: &BoundingBox,
    limit: usize,
) -> Result<Vec<PilotSite>, StoreError> {
    // As for the sites of a source: the index finds the candidates, and
    // the comparison of the coordinates that follows is the exact one.
    let query = format!(
        "{SITE}
         WHERE s.removed_at IS NULL
           AND ST_SetSRID(ST_MakePoint(s.lon, s.lat), 4326)
               && ST_MakeEnvelope($1, $2, $3, $4, 4326)
           AND s.lon BETWEEN $1 AND $3
           AND s.lat BETWEEN $2 AND $4
           AND starts_with(s.id, $6)
         ORDER BY s.id
         LIMIT $5"
    );
    let statement = client.prepare_cached(&query).await.map_err(db_error)?;
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let rows = client
        .query(
            &statement,
            &[
                &area.west(),
                &area.south(),
                &area.east(),
                &area.north(),
                &limit,
                &beginning(layer),
            ],
        )
        .await
        .map_err(db_error)?;
    rows.iter().map(site_from).collect()
}

pub(super) async fn one(client: &Client, id: &SiteId) -> Result<Option<PilotSite>, StoreError> {
    if !can_name(id) {
        return Ok(None);
    }
    read(client, id).await
}

pub(super) async fn revisions(
    client: &Client,
    id: &SiteId,
    after: u32,
    limit: usize,
) -> Result<Option<(PilotSite, Vec<SiteRevision>)>, StoreError> {
    if !can_name(id) {
        return Ok(None);
    }
    let statement = client
        .prepare_cached(
            "SELECT r.number, r.at, r.via, r.via_token, r.kind, r.restores, r.comment, r.record,
                    a.id AS author_id, a.handle AS author_handle,
                    a.display_name AS author_name
             FROM pilot_site_revisions r
             LEFT JOIN accounts a ON a.id = r.author_id
             WHERE r.site_id = $1 AND r.number > $2::bigint
             ORDER BY r.number
             LIMIT $3",
        )
        .await
        .map_err(db_error)?;
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    // The page is read before the site: a revision written between the
    // two shows in the site's number and is on a later page, and no page
    // ever holds a revision beyond the number the site is said to stand
    // at.
    let rows = client
        .query(&statement, &[&id.to_string(), &i64::from(after), &limit])
        .await
        .map_err(db_error)?;
    let Some(site) = read(client, id).await? else {
        return Ok(None);
    };
    let revisions = rows
        .iter()
        .map(revision_from)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some((site, revisions)))
}

/// Every revision of the sites named, by site and number.
pub(super) async fn revisions_of(
    client: &Client,
    ids: &[SiteId],
) -> Result<Vec<SiteRevision>, StoreError> {
    // An identifier with a zero byte in it is the identifier of no site.
    let ids: Vec<String> = ids
        .iter()
        .filter(|id| can_name(id))
        .map(ToString::to_string)
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let statement = client
        .prepare_cached(
            "SELECT r.number, r.at, r.via, r.via_token, r.kind, r.restores, r.comment, r.record,
                    a.id AS author_id, a.handle AS author_handle,
                    a.display_name AS author_name
             FROM pilot_site_revisions r
             LEFT JOIN accounts a ON a.id = r.author_id
             WHERE r.site_id = ANY($1)",
        )
        .await
        .map_err(db_error)?;
    let rows = client.query(&statement, &[&ids]).await.map_err(db_error)?;
    let mut revisions = rows
        .iter()
        .map(revision_from)
        .collect::<Result<Vec<_>, _>>()?;
    // By site as identifiers are ordered, layer first, which is not the
    // order of their text: the order the in-memory store gives.
    revisions.sort_by(|a, b| (&a.record.id, a.number).cmp(&(&b.record.id, b.number)));
    Ok(revisions)
}

pub(super) async fn changes(
    client: &Client,
    layer: &SourceId,
    since: i64,
    limit: usize,
) -> Result<PilotSiteChanges, StoreError> {
    // The count is read before the sites, so it is never ahead of what
    // the list below was taken from.
    let latest: i64 = client
        .query_opt("SELECT latest FROM pilot_site_change", &[])
        .await
        .map_err(db_error)?
        .ok_or_else(|| corrupt("count of changes to the sites", "its row is missing"))?
        .get("latest");
    let query =
        format!("{SITE} WHERE s.change > $1 AND starts_with(s.id, $3) ORDER BY s.change LIMIT $2");
    let statement = client.prepare_cached(&query).await.map_err(db_error)?;
    // One more than was asked for, to know whether more are waiting.
    let one_more = i64::try_from(limit).unwrap_or(i64::MAX).saturating_add(1);
    let rows = client
        .query(&statement, &[&since, &one_more, &beginning(layer)])
        .await
        .map_err(db_error)?;
    let mut sites = rows.iter().map(site_from).collect::<Result<Vec<_>, _>>()?;
    let more = sites.len() > limit;
    sites.truncate(limit);
    // A site written after the count was read took a later number.
    let latest = sites.iter().map(|site| site.change).fold(latest, i64::max);
    Ok(PilotSiteChanges {
        sites,
        more,
        latest,
    })
}

/// Holds a site's row for the rest of the transaction and says whether it
/// stands. `None` if no site ever had the identifier.
async fn hold_site(tx: &Transaction<'_>, id: &str) -> Result<Option<bool>, StoreError> {
    let row = tx
        .query_opt(
            "SELECT removed_at IS NULL FROM pilot_sites WHERE id = $1 FOR UPDATE",
            &[&id],
        )
        .await
        .map_err(db_error)?;
    Ok(row.map(|row| row.get(0)))
}

/// Gives a site the next change number without a revision: what was said
/// about it changed, and what it says did not.
async fn touch(tx: &Transaction<'_>, id: &str, latest: i64) -> Result<(), StoreError> {
    let change = next_change(tx, latest).await?;
    tx.execute(
        "UPDATE pilot_sites SET change = $2 WHERE id = $1",
        &[&id, &change],
    )
    .await
    .map_err(db_error)?;
    Ok(())
}

pub(super) async fn confirm(
    client: &mut Client,
    new: &NewSiteConfirmation,
) -> Result<SiteConfirming, StoreError> {
    check_confirmation(new)?;
    let tx = client.transaction().await.map_err(db_error)?;
    // The counter first, then the account, then the site, as every write.
    let latest = lock_layer(&tx).await?;
    if !hold_account(&tx, &new.account_id).await? {
        return Ok(SiteConfirming::NoSuchAccount);
    }
    let id = new.site.to_string();
    match hold_site(&tx, &id).await? {
        None => return Ok(SiteConfirming::NoSuchSite),
        Some(false) => return Ok(SiteConfirming::Removed),
        Some(true) => {}
    }
    tx.execute(
        "INSERT INTO site_confirmations (site_id, account_id, day, note, with_flight, at)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (site_id, account_id) DO UPDATE
         SET day = EXCLUDED.day, note = EXCLUDED.note, with_flight = EXCLUDED.with_flight,
             at = EXCLUDED.at",
        &[
            &id,
            &new.account_id,
            &new.day,
            &new.note,
            &new.with_flight,
            &new.at,
        ],
    )
    .await
    .map_err(db_error)?;
    touch(&tx, &id, latest).await?;
    let site = read(&tx, &new.site)
        .await?
        .ok_or_else(|| corrupt("site", "it is gone while its row is held"))?;
    tx.commit().await.map_err(db_error)?;
    Ok(SiteConfirming::Confirmed(Box::new(site)))
}

pub(super) async fn withdraw(
    client: &mut Client,
    site: &SiteId,
    account_id: &str,
) -> Result<Option<(PilotSite, bool)>, StoreError> {
    if !can_name(site) || account_id.contains('\0') {
        return Ok(None);
    }
    let tx = client.transaction().await.map_err(db_error)?;
    let latest = lock_layer(&tx).await?;
    let id = site.to_string();
    if hold_site(&tx, &id).await?.is_none() {
        return Ok(None);
    }
    let withdrawn = tx
        .execute(
            "DELETE FROM site_confirmations WHERE site_id = $1 AND account_id = $2",
            &[&id, &account_id],
        )
        .await
        .map_err(db_error)?;
    if withdrawn > 0 {
        touch(&tx, &id, latest).await?;
    }
    let read = read(&tx, site)
        .await?
        .ok_or_else(|| corrupt("site", "it is gone while its row is held"))?;
    tx.commit().await.map_err(db_error)?;
    Ok(Some((read, withdrawn > 0)))
}

pub(super) async fn confirmations(
    client: &Client,
    site: &SiteId,
    limit: usize,
) -> Result<Option<(PilotSite, Vec<SiteConfirmation>)>, StoreError> {
    if !can_name(site) {
        return Ok(None);
    }
    let statement = client
        .prepare_cached(
            "SELECT k.day, k.note, k.with_flight, k.at,
                    a.id AS author_id, a.handle AS author_handle,
                    a.display_name AS author_name
             FROM site_confirmations k
             JOIN accounts a ON a.id = k.account_id
             WHERE k.site_id = $1
             ORDER BY k.day DESC, a.handle
             LIMIT $2",
        )
        .await
        .map_err(db_error)?;
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let rows = client
        .query(&statement, &[&site.to_string(), &limit])
        .await
        .map_err(db_error)?;
    let Some(read) = read(client, site).await? else {
        return Ok(None);
    };
    let list = rows
        .iter()
        .filter_map(|row| {
            Some(SiteConfirmation {
                pilot: pilot_from(row, "author_id", "author_handle", "author_name")?,
                day: row.get("day"),
                note: row.get("note"),
                with_flight: row.get("with_flight"),
                at: row.get("at"),
            })
        })
        .collect();
    Ok(Some((read, list)))
}

/// Every revision an account wrote, by site and number.
pub(super) async fn revisions_by(
    client: &Client,
    account_id: &str,
) -> Result<Vec<SiteRevision>, StoreError> {
    let statement = client
        .prepare_cached(
            "SELECT r.number, r.at, r.via, r.via_token, r.kind, r.restores, r.comment, r.record,
                    a.id AS author_id, a.handle AS author_handle,
                    a.display_name AS author_name
             FROM pilot_site_revisions r
             JOIN accounts a ON a.id = r.author_id
             WHERE r.author_id = $1",
        )
        .await
        .map_err(db_error)?;
    let rows = client
        .query(&statement, &[&account_id])
        .await
        .map_err(db_error)?;
    let mut revisions = rows
        .iter()
        .map(revision_from)
        .collect::<Result<Vec<_>, _>>()?;
    // By site as identifiers are ordered, layer first, which is not the
    // order of their text: the order the in-memory store gives.
    revisions.sort_by(|a, b| (&a.record.id, a.number).cmp(&(&b.record.id, b.number)));
    Ok(revisions)
}

/// Every confirmation an account has, by site.
pub(super) async fn confirmations_by(
    client: &Client,
    account_id: &str,
) -> Result<Vec<(SiteId, SiteConfirmation)>, StoreError> {
    let statement = client
        .prepare_cached(
            "SELECT k.site_id, k.day, k.note, k.with_flight, k.at,
                    a.id AS author_id, a.handle AS author_handle,
                    a.display_name AS author_name
             FROM site_confirmations k
             JOIN accounts a ON a.id = k.account_id
             WHERE k.account_id = $1",
        )
        .await
        .map_err(db_error)?;
    let rows = client
        .query(&statement, &[&account_id])
        .await
        .map_err(db_error)?;
    let mut confirmations = rows
        .iter()
        .map(|row| {
            let site: &str = row.get("site_id");
            let site: SiteId = site
                .parse()
                .map_err(|e| corrupt("identifier of a confirmed site", e))?;
            let pilot = pilot_from(row, "author_id", "author_handle", "author_name")
                .ok_or_else(|| corrupt("confirmation", "it has no account"))?;
            let confirmation = SiteConfirmation {
                pilot,
                day: row.get("day"),
                note: row.get("note"),
                with_flight: row.get("with_flight"),
                at: row.get("at"),
            };
            Ok((site, confirmation))
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    // By site, in the order of identifiers; see `revisions_by`.
    confirmations.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(confirmations)
}

#[cfg(test)]
mod tests {
    use venti_domain::{GeoPoint, SiteUse};

    use super::*;

    fn at(lat: f64, lon: f64) -> Site {
        Site {
            id: "venti:a".parse().unwrap(),
            name: None,
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

    /// The box the database is asked for holds every place that is as
    /// near as sites may not be: a place just inside the distance, in
    /// each of eight directions, lies in it.
    #[test]
    fn the_box_around_a_place_holds_everything_within_the_distance() {
        for (lat, lon) in [(47.6, 11.8), (0.0, 0.0), (-33.9, 151.2), (69.7, 19.0)] {
            let here = at(lat, lon);
            let [west, south, east, north] = around(&here, 20.0);
            assert!(west < lon && lon < east && south < lat && lat < north);
            // 20 m is less than 0.00019 degrees of latitude everywhere,
            // and less than that over the cosine in longitude.
            let step = 0.00019;
            let wide = step / lat.to_radians().cos();
            for (north_by, east_by) in [
                (1.0, 0.0),
                (-1.0, 0.0),
                (0.0, 1.0),
                (0.0, -1.0),
                (0.7, 0.7),
                (-0.7, 0.7),
                (0.7, -0.7),
                (-0.7, -0.7),
            ] {
                let (near_lat, near_lon) = (lat + north_by * step, lon + east_by * wide);
                assert!(
                    (south..=north).contains(&near_lat) && (west..=east).contains(&near_lon),
                    "{near_lat}, {near_lon} is outside the box around {lat}, {lon}"
                );
            }
        }
    }

    #[test]
    fn near_a_pole_and_where_east_meets_west_the_box_goes_around_the_globe() {
        let [west, south, east, north] = around(&at(89.99999, 10.0), 20.0);
        assert_eq!((west, east, north), (-180.0, 180.0, 90.0));
        assert!(south < 89.99999);
        let [west, _, east, _] = around(&at(-17.0, 179.99999), 20.0);
        assert_eq!((west, east), (-180.0, 180.0));
        // No distance at all, and one without end, are boxes too.
        let [west, south, east, north] = around(&at(47.0, 11.0), f64::INFINITY);
        assert_eq!((west, south, east, north), (-180.0, -90.0, 180.0, 90.0));
    }
}
