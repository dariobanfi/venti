//! Reports in PostgreSQL.

use deadpool_postgres::Client;
use tokio_postgres::Row;
use tokio_postgres::types::ToSql;
use venti_domain::Timestamp;

use super::{corrupt, db_error};
use crate::report::{check, check_note};
use crate::{
    NewReport, Report, ReportClosing, ReportClosure, ReportKey, ReportListing, ReportSaved,
    ReportSubject, StoreError,
};

fn report_from(row: &Row) -> Result<Report, StoreError> {
    let (kind, subject_id): (&str, &str) = (row.get("kind"), row.get("subject_id"));
    let closed_at: Option<Timestamp> = row.get("closed_at");
    let closing_note: Option<String> = row.get("closing_note");
    Ok(Report {
        id: row.get("id"),
        created_at: row.get("created_at"),
        reporter_handle: row.get("reporter_handle"),
        about_account_id: row.get("about_account_id"),
        about_handle: row.get("about_handle"),
        about_name: row.get("about_name"),
        subject: ReportSubject::from_parts(kind, subject_id)
            .ok_or_else(|| corrupt("kind of report", kind))?,
        reason: row.get("reason"),
        text: row.get("reporter_text"),
        closed: closed_at
            .zip(closing_note)
            .map(|(at, note)| ReportClosure { at, note }),
    })
}

pub(super) async fn save(client: &Client, new: &NewReport) -> Result<ReportSaved, StoreError> {
    check(new)?;
    // One statement: the report is stored if both accounts are there and
    // its reporter has no open report about the same subject. Of two calls
    // that race, the unique index lets one through.
    let statement = client
        .prepare_cached(
            "INSERT INTO reports
                 (id, created_at, reporter_id, about_account_id, kind, subject_id, reason,
                  reporter_text)
             SELECT $1, $2, reporter.id, about.id, $5, $6, $7, $8
             FROM accounts reporter, accounts about
             WHERE reporter.id = $3 AND about.id = $4
             ON CONFLICT (reporter_id, about_account_id, kind, subject_id)
                 WHERE closed_at IS NULL DO NOTHING",
        )
        .await
        .map_err(db_error)?;
    let stored = client
        .execute(
            &statement,
            &[
                &new.id,
                &new.created_at,
                &new.reporter_id,
                &new.about_account_id,
                &new.subject.kind(),
                &new.subject.id(),
                &new.reason,
                &new.text,
            ],
        )
        .await
        .map_err(db_error)?;
    if stored > 0 {
        return Ok(ReportSaved::Stored);
    }
    // Nothing was stored: an account is missing, or the report is open.
    let accounts: i64 = client
        .query_one(
            "SELECT (SELECT count(*) FROM accounts WHERE id = $1)
                  + (SELECT count(*) FROM accounts WHERE id = $2)",
            &[&new.reporter_id, &new.about_account_id],
        )
        .await
        .map_err(db_error)?
        .get(0);
    Ok(if accounts == 2 {
        ReportSaved::AlreadyOpen
    } else {
        ReportSaved::NoSuchAccount
    })
}

pub(super) async fn list(
    client: &Client,
    listing: ReportListing,
    after: Option<&ReportKey>,
    limit: usize,
) -> Result<Vec<Report>, StoreError> {
    // The condition is spelled as in the index of the open reports.
    let only = match listing {
        ReportListing::Open => "AND r.closed_at IS NULL",
        ReportListing::All => "",
    };
    let page = match after {
        Some(_) => "AND (r.created_at, r.id) > ($2, $3)",
        None => "",
    };
    let query = format!(
        "SELECT r.id, r.created_at, reporter.handle AS reporter_handle, r.about_account_id,
                about.handle AS about_handle, about.display_name AS about_name,
                r.kind, r.subject_id, r.reason, r.reporter_text, r.closed_at, r.closing_note
         FROM reports r
         JOIN accounts reporter ON reporter.id = r.reporter_id
         JOIN accounts about ON about.id = r.about_account_id
         WHERE true {only} {page}
         ORDER BY r.created_at, r.id
         LIMIT $1"
    );
    let statement = client.prepare_cached(&query).await.map_err(db_error)?;
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let mut parameters: Vec<&(dyn ToSql + Sync)> = vec![&limit];
    if let Some((created_at, id)) = after {
        parameters.extend([created_at as &(dyn ToSql + Sync), id]);
    }
    let rows = client
        .query(&statement, &parameters)
        .await
        .map_err(db_error)?;
    rows.iter().map(report_from).collect()
}

pub(super) async fn close(
    client: &Client,
    id: &str,
    note: &str,
    now: Timestamp,
) -> Result<ReportClosing, StoreError> {
    check_note(note)?;
    // PostgreSQL takes no zero byte in text. No identifier holds one, so
    // the answer is known.
    if id.contains('\0') {
        return Ok(ReportClosing::NoSuchReport);
    }
    let closed = client
        .execute(
            "UPDATE reports SET closed_at = $2, closing_note = $3
             WHERE id = $1 AND closed_at IS NULL",
            &[&id, &now, &note],
        )
        .await
        .map_err(db_error)?;
    if closed > 0 {
        return Ok(ReportClosing::Closed);
    }
    let row = client
        .query_opt(
            "SELECT closed_at, closing_note FROM reports WHERE id = $1",
            &[&id],
        )
        .await
        .map_err(db_error)?;
    Ok(match row {
        None => ReportClosing::NoSuchReport,
        Some(row) => {
            let at: Option<Timestamp> = row.get(0);
            let note: Option<String> = row.get(1);
            let closure = at
                .zip(note)
                .map(|(at, note)| ReportClosure { at, note })
                .ok_or_else(|| corrupt("report", "it is neither open nor closed"))?;
            ReportClosing::AlreadyClosed(closure)
        }
    })
}

pub(super) async fn prune(client: &Client, closed_before: Timestamp) -> Result<u64, StoreError> {
    client
        .execute(
            "DELETE FROM reports WHERE closed_at < $1",
            &[&closed_before],
        )
        .await
        .map_err(db_error)
}
