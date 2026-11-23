//! Pilots' pictures, and whether a pilot is among the pilots near a place,
//! in PostgreSQL.

use deadpool_postgres::Client;
use tokio_postgres::Row;

use super::{corrupt, db_error};
use crate::{NewPicture, Picture, PictureOrigin, StoreError};

fn picture_from(row: &Row) -> Result<Picture, StoreError> {
    let origin: String = row.get("origin");
    Ok(Picture {
        id: row.get("id"),
        account_id: row.get("account_id"),
        bytes: row.get("bytes"),
        origin: PictureOrigin::from_name(&origin)
            .ok_or_else(|| corrupt("origin of a picture", origin))?,
        created_at: row.get("created_at"),
    })
}

/// PostgreSQL takes no zero byte in text; no identifier holds one.
fn unusable(text: &str) -> bool {
    text.contains('\0')
}

pub(super) async fn set_picture(client: &Client, new: &NewPicture) -> Result<bool, StoreError> {
    if unusable(&new.account_id) || unusable(&new.id) {
        return Ok(false);
    }
    // One statement: of two pictures sent at once one stands, whole. One
    // from Google takes no place that a picture holds.
    let on_conflict = match new.origin {
        PictureOrigin::Upload => {
            "DO UPDATE SET id = excluded.id, bytes = excluded.bytes,
                 origin = excluded.origin, created_at = excluded.created_at"
        }
        PictureOrigin::Google => "DO NOTHING",
    };
    let statement = format!(
        "WITH account AS (SELECT id FROM accounts WHERE id = $2)
         INSERT INTO pictures (id, account_id, bytes, origin, created_at)
         SELECT $1, account.id, $3, $4, $5 FROM account
         ON CONFLICT (account_id) {on_conflict}"
    );
    client
        .execute(
            &statement,
            &[
                &new.id,
                &new.account_id,
                &new.bytes,
                &new.origin.as_str(),
                &new.created_at,
            ],
        )
        .await
        .map_err(db_error)?;
    let there = client
        .query_opt("SELECT 1 FROM accounts WHERE id = $1", &[&new.account_id])
        .await
        .map_err(db_error)?;
    Ok(there.is_some())
}

pub(super) async fn remove_picture(
    client: &Client,
    account_id: &str,
) -> Result<Option<Picture>, StoreError> {
    if unusable(account_id) {
        return Ok(None);
    }
    let row = client
        .query_opt(
            "DELETE FROM pictures WHERE account_id = $1
             RETURNING id, account_id, bytes, origin, created_at",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    row.as_ref().map(picture_from).transpose()
}

pub(super) async fn picture(client: &Client, id: &str) -> Result<Option<Picture>, StoreError> {
    if unusable(id) {
        return Ok(None);
    }
    let row = client
        .query_opt(
            "SELECT id, account_id, bytes, origin, created_at FROM pictures WHERE id = $1",
            &[&id],
        )
        .await
        .map_err(db_error)?;
    row.as_ref().map(picture_from).transpose()
}

pub(super) async fn claim_google_picture(
    client: &Client,
    account_id: &str,
) -> Result<bool, StoreError> {
    if unusable(account_id) {
        return Ok(false);
    }
    // One statement: of two sign-ins at once one claims it.
    let claimed = client
        .execute(
            "UPDATE accounts SET google_picture_tried = true
             WHERE id = $1 AND NOT google_picture_tried
               AND NOT EXISTS (SELECT 1 FROM pictures WHERE account_id = $1)",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    Ok(claimed > 0)
}
