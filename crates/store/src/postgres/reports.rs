//! Reports in PostgreSQL.

use deadpool_postgres::Client;
use tokio_postgres::Row;
use tokio_postgres::error::SqlState;
use tokio_postgres::types::ToSql;
use venti_domain::Timestamp;

use super::{corrupt, db_error};
use crate::report::{ReportedWords, check, check_note};
use crate::{
    NewReport, Report, ReportClosing, ReportClosure, ReportKey, ReportListing, ReportSaved,
    ReportSubject, StoreError,
};

/// The index that holds an account to one open report about a group,
/// whoever keeps the group (`0012_reports_about_groups.sql`).
const OPEN_ONCE_PER_GROUP: &str = "reports_open_once_group_idx";

fn report_from(row: &Row) -> Result<Report, StoreError> {
    let (kind, subject_id): (&str, &str) = (row.get("kind"), row.get("subject_id"));
    let words = ReportedWords {
        group_id: row.get("group_id"),
        name: row.get("reported_name"),
        place: row.get("reported_place"),
        note: row.get("reported_note"),
    };
    let closed_at: Option<Timestamp> = row.get("closed_at");
    let closing_note: Option<String> = row.get("closing_note");
    Ok(Report {
        id: row.get("id"),
        created_at: row.get("created_at"),
        reporter_handle: row.get("reporter_handle"),
        about_account_id: row.get("about_account_id"),
        about_handle: row.get("about_handle"),
        about_name: row.get("about_name"),
        subject: ReportSubject::from_parts(kind, subject_id, words)
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
                  reporter_text, group_id, reported_name, reported_place, reported_note)
             SELECT $1, $2, reporter.id, about.id, $5, $6, $7, $8, $9, $10, $11, $12
             FROM accounts reporter, accounts about
             WHERE reporter.id = $3 AND about.id = $4
             ON CONFLICT (reporter_id, about_account_id, kind, subject_id)
                 WHERE closed_at IS NULL DO NOTHING",
        )
        .await
        .map_err(db_error)?;
    let words = new.subject.words();
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
                &words.group_id,
                &words.name,
                &words.place,
                &words.note,
            ],
        )
        .await;
    let stored = match stored {
        Ok(stored) => stored,
        // The reporter's open report about this group was made while
        // another account kept it: the first index does not know the two
        // as one, and the second refuses the row.
        Err(error) if is_open_about_the_group(&error) => return Ok(ReportSaved::AlreadyOpen),
        Err(error) => return Err(db_error(error)),
    };
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

/// Whether a report was refused because its reporter has an open report
/// about the same group, under another keeper.
fn is_open_about_the_group(error: &tokio_postgres::Error) -> bool {
    error.as_db_error().is_some_and(|refusal| {
        refusal.code() == &SqlState::UNIQUE_VIOLATION
            && refusal.constraint() == Some(OPEN_ONCE_PER_GROUP)
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
                r.kind, r.subject_id, r.group_id, r.reported_name, r.reported_place,
                r.reported_note, r.reason, r.reporter_text, r.closed_at, r.closing_note
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
