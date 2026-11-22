//! What the authorization server keeps, in PostgreSQL.

use deadpool_postgres::Client;
use tokio_postgres::Row;
use venti_domain::Timestamp;

use super::accounts::{access_token_from, account_of};
use super::db_error;
use crate::{
    AuthorizationCode, AuthorizationRequest, NewGrant, OauthClient, Redeemed, Refreshed,
    RequestState, Rotation, SecretHash, StoreError, StoredCode,
};

fn client_from(row: &Row) -> OauthClient {
    OauthClient {
        id: row.get("id"),
        name: row.get("name"),
        redirect_uris: row.get("redirect_uris"),
        created_at: row.get("created_at"),
        last_used_at: row.get("last_used_at"),
    }
}

pub(super) async fn create_oauth_client(
    client: &mut Client,
    new: &OauthClient,
    most: usize,
) -> Result<bool, StoreError> {
    let most = i64::try_from(most).unwrap_or(i64::MAX);
    let tx = client.transaction().await.map_err(db_error)?;
    // One registration at a time counts the table and makes room in it, so
    // that two that race for the last place do not both take it.
    tx.execute(
        "SELECT pg_advisory_xact_lock(hashtext('venti oauth clients')::bigint)",
        &[],
    )
    .await
    .map_err(db_error)?;
    let held: i64 = tx
        .query_one("SELECT count(*) FROM oauth_clients", &[])
        .await
        .map_err(db_error)?
        .get(0);
    if held >= most {
        let removed = tx
            .execute(
                "DELETE FROM oauth_clients WHERE id = (
                     SELECT id FROM oauth_clients WHERE last_used_at IS NULL
                     ORDER BY created_at, id LIMIT 1)",
                &[],
            )
            .await
            .map_err(db_error)?;
        if removed == 0 {
            return Ok(false);
        }
    }
    let stored = tx
        .execute(
            "INSERT INTO oauth_clients (id, name, redirect_uris, created_at, last_used_at)
             VALUES ($1, $2, $3, $4, $5) ON CONFLICT (id) DO NOTHING",
            &[
                &new.id,
                &new.name,
                &new.redirect_uris,
                &new.created_at,
                &new.last_used_at,
            ],
        )
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(stored > 0)
}

pub(super) async fn oauth_client(
    client: &Client,
    client_id: &str,
) -> Result<Option<OauthClient>, StoreError> {
    // No stored identifier holds a zero byte, and the database refuses to
    // compare one with anything.
    if client_id.contains('\0') {
        return Ok(None);
    }
    let statement = client
        .prepare_cached(
            "SELECT id, name, redirect_uris, created_at, last_used_at
             FROM oauth_clients WHERE id = $1",
        )
        .await
        .map_err(db_error)?;
    let row = client
        .query_opt(&statement, &[&client_id])
        .await
        .map_err(db_error)?;
    Ok(row.as_ref().map(client_from))
}

pub(super) async fn create_authorization_request(
    client: &mut Client,
    request: &SecretHash,
    record: &AuthorizationRequest,
    now: Timestamp,
    most: usize,
) -> Result<bool, StoreError> {
    // The program's own value is text the database must be able to hold.
    // One with a zero byte in it is not kept, and so not answered here.
    if record
        .state
        .as_deref()
        .is_some_and(|state| state.contains('\0'))
    {
        return Ok(false);
    }
    let most = i64::try_from(most).unwrap_or(i64::MAX);
    let tx = client.transaction().await.map_err(db_error)?;
    // One at a time counts the table, so that two requests that race for
    // the last place do not both take it.
    tx.execute(
        "SELECT pg_advisory_xact_lock(hashtext('venti oauth requests')::bigint)",
        &[],
    )
    .await
    .map_err(db_error)?;
    tx.execute("DELETE FROM oauth_requests WHERE expires_at <= $1", &[&now])
        .await
        .map_err(db_error)?;
    let held: i64 = tx
        .query_one("SELECT count(*) FROM oauth_requests", &[])
        .await
        .map_err(db_error)?
        .get(0);
    let stored = if held >= most {
        0
    } else {
        tx.execute(
            "INSERT INTO oauth_requests
                 (request_hash, client_id, redirect_uri, scopes, resource, challenge, state,
                  created_at, expires_at)
             SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9
             WHERE EXISTS (SELECT 1 FROM oauth_clients WHERE id = $2)
             ON CONFLICT (request_hash) DO NOTHING",
            &[
                &request.0.as_slice(),
                &record.client_id,
                &record.redirect_uri,
                &record.scopes,
                &record.resource,
                &record.challenge,
                &record.state,
                &record.created_at,
                &record.expires_at,
            ],
        )
        .await
        .map_err(db_error)?
    };
    // Also when nothing was stored: the ended ones stay removed.
    tx.commit().await.map_err(db_error)?;
    Ok(stored > 0)
}

const REQUEST_COLUMNS: &str =
    "client_id, redirect_uri, scopes, resource, challenge, state, created_at, expires_at";

fn request_from(row: &Row) -> AuthorizationRequest {
    AuthorizationRequest {
        client_id: row.get("client_id"),
        redirect_uri: row.get("redirect_uri"),
        scopes: row.get("scopes"),
        resource: row.get("resource"),
        challenge: row.get("challenge"),
        state: row.get("state"),
        created_at: row.get("created_at"),
        expires_at: row.get("expires_at"),
    }
}

pub(super) async fn authorization_request(
    client: &Client,
    request: &SecretHash,
    now: Timestamp,
) -> Result<RequestState, StoreError> {
    let statement = client
        .prepare_cached(&format!(
            "SELECT {REQUEST_COLUMNS}, answered_at IS NOT NULL AS answered
             FROM oauth_requests WHERE request_hash = $1 AND expires_at > $2"
        ))
        .await
        .map_err(db_error)?;
    let row = client
        .query_opt(&statement, &[&request.0.as_slice(), &now])
        .await
        .map_err(db_error)?;
    Ok(match row {
        None => RequestState::Unknown,
        Some(row) if row.get::<_, bool>("answered") => RequestState::Answered,
        Some(row) => RequestState::Waiting(request_from(&row)),
    })
}

pub(super) async fn answer_authorization_request(
    client: &Client,
    request: &SecretHash,
    now: Timestamp,
) -> Result<RequestState, StoreError> {
    // One statement: of two answers that race, the second waits for the
    // first's row, finds it answered and changes nothing.
    let taken = client
        .query_opt(
            &format!(
                "UPDATE oauth_requests SET answered_at = $2
                 WHERE request_hash = $1 AND expires_at > $2 AND answered_at IS NULL
                 RETURNING {REQUEST_COLUMNS}"
            ),
            &[&request.0.as_slice(), &now],
        )
        .await
        .map_err(db_error)?;
    if let Some(row) = taken {
        return Ok(RequestState::Waiting(request_from(&row)));
    }
    let there = client
        .query_opt(
            "SELECT 1 FROM oauth_requests WHERE request_hash = $1 AND expires_at > $2",
            &[&request.0.as_slice(), &now],
        )
        .await
        .map_err(db_error)?;
    Ok(match there {
        Some(_) => RequestState::Answered,
        None => RequestState::Unknown,
    })
}

pub(super) async fn create_authorization_code(
    client: &mut Client,
    code: &SecretHash,
    record: &AuthorizationCode,
) -> Result<bool, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let touched = tx
        .execute(
            "UPDATE oauth_clients SET last_used_at = $2 WHERE id = $1",
            &[&record.client_id, &record.created_at],
        )
        .await
        .map_err(db_error)?;
    if touched == 0 {
        return Ok(false);
    }
    let stored = tx
        .execute(
            "INSERT INTO oauth_codes
                 (code_hash, client_id, account_id, redirect_uri, scopes, resource, challenge,
                  created_at, expires_at)
             SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9
             WHERE EXISTS (SELECT 1 FROM accounts WHERE id = $3)
             ON CONFLICT (code_hash) DO NOTHING",
            &[
                &code.0.as_slice(),
                &record.client_id,
                &record.account_id,
                &record.redirect_uri,
                &record.scopes,
                &record.resource,
                &record.challenge,
                &record.created_at,
                &record.expires_at,
            ],
        )
        .await
        .map_err(db_error)?;
    if stored == 0 {
        return Ok(false);
    }
    tx.commit().await.map_err(db_error)?;
    Ok(true)
}

pub(super) async fn authorization_code(
    client: &Client,
    code: &SecretHash,
    now: Timestamp,
) -> Result<Option<StoredCode>, StoreError> {
    let statement = client
        .prepare_cached(
            "SELECT client_id, account_id, redirect_uri, scopes, resource, challenge,
                    created_at, expires_at, used_at IS NOT NULL AS used
             FROM oauth_codes WHERE code_hash = $1 AND expires_at > $2",
        )
        .await
        .map_err(db_error)?;
    let row = client
        .query_opt(&statement, &[&code.0.as_slice(), &now])
        .await
        .map_err(db_error)?;
    Ok(row.map(|row| StoredCode {
        code: AuthorizationCode {
            client_id: row.get("client_id"),
            account_id: row.get("account_id"),
            redirect_uri: row.get("redirect_uri"),
            scopes: row.get("scopes"),
            resource: row.get("resource"),
            challenge: row.get("challenge"),
            created_at: row.get("created_at"),
            expires_at: row.get("expires_at"),
        },
        used: row.get("used"),
    }))
}

pub(super) async fn redeem_authorization_code(
    client: &mut Client,
    code: &SecretHash,
    grant: &NewGrant,
    now: Timestamp,
    most: usize,
) -> Result<Redeemed, StoreError> {
    let most = i64::try_from(most).unwrap_or(i64::MAX);
    let tx = client.transaction().await.map_err(db_error)?;
    // The code's row is locked: of two exchanges that race, the second
    // waits here and then finds the code used.
    let row = tx
        .query_opt(
            "SELECT client_id, account_id, used_at IS NOT NULL AS used, token_id
             FROM oauth_codes WHERE code_hash = $1 AND expires_at > $2 FOR UPDATE",
            &[&code.0.as_slice(), &now],
        )
        .await
        .map_err(db_error)?;
    let Some(row) = row else {
        return Ok(Redeemed::Unknown);
    };
    let (client_id, account_id): (String, String) = (row.get("client_id"), row.get("account_id"));
    if row.get::<_, bool>("used") {
        if let Some(token_id) = row.get::<_, Option<String>>("token_id") {
            tx.execute(
                "DELETE FROM access_tokens WHERE id = $1 AND client_id IS NOT NULL",
                &[&token_id],
            )
            .await
            .map_err(db_error)?;
        }
        tx.commit().await.map_err(db_error)?;
        return Ok(Redeemed::Spent);
    }
    let token = &grant.token;
    let (Some(bound), Some(expires_at)) = (&token.client, token.expires_at) else {
        return Ok(Redeemed::Unknown);
    };
    if token.account_id != account_id || bound.client_id != client_id {
        return Ok(Redeemed::Unknown);
    }
    // As for a token made by hand: the account's row is locked while its
    // tokens are counted.
    let account = tx
        .query_opt(
            "SELECT 1 FROM accounts WHERE id = $1 FOR UPDATE",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    if account.is_none() {
        return Ok(Redeemed::Unknown);
    }
    let held: i64 = tx
        .query_one(
            "SELECT count(*) FROM access_tokens
             WHERE account_id = $1 AND client_id IS DISTINCT FROM $2",
            &[&account_id, &client_id],
        )
        .await
        .map_err(db_error)?
        .get(0);
    if held >= most {
        return Ok(Redeemed::Full);
    }
    tx.execute(
        "DELETE FROM access_tokens WHERE account_id = $1 AND client_id = $2",
        &[&account_id, &client_id],
    )
    .await
    .map_err(db_error)?;
    tx.execute(
        "INSERT INTO access_tokens
             (id, account_id, token_hash, name, scopes, created_at, last_used_on, expires_at,
              client_id, resource, access_until)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        &[
            &token.id,
            &token.account_id,
            &grant.access.0.as_slice(),
            &token.name,
            &token.scopes,
            &token.created_at,
            &token.last_used_on,
            &expires_at,
            &bound.client_id,
            &bound.resource,
            &bound.access_until,
        ],
    )
    .await
    .map_err(db_error)?;
    tx.execute(
        "INSERT INTO oauth_refresh_tokens (token_hash, token_id, created_at, expires_at)
         VALUES ($1, $2, $3, $4)",
        &[&grant.refresh.0.as_slice(), &token.id, &now, &expires_at],
    )
    .await
    .map_err(db_error)?;
    tx.execute(
        "UPDATE oauth_codes SET used_at = $2, token_id = $3 WHERE code_hash = $1",
        &[&code.0.as_slice(), &now, &token.id],
    )
    .await
    .map_err(db_error)?;
    tx.execute(
        "UPDATE oauth_clients SET last_used_at = $2 WHERE id = $1",
        &[&client_id, &now],
    )
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(Redeemed::Granted)
}

pub(super) async fn refresh_grant(
    client: &mut Client,
    refresh: &SecretHash,
    client_id: &str,
    next: &Rotation,
    now: Timestamp,
) -> Result<Refreshed, StoreError> {
    if client_id.contains('\0') {
        return Ok(Refreshed::Unknown);
    }
    let tx = client.transaction().await.map_err(db_error)?;
    // Both rows are locked: of two uses of one refresh token that race,
    // the second waits here and then finds it used.
    let row = tx
        .query_opt(
            "SELECT r.token_id, r.spent_at IS NOT NULL AS spent
             FROM oauth_refresh_tokens r
             JOIN access_tokens t ON t.id = r.token_id
             WHERE r.token_hash = $1 AND t.client_id = $2 AND r.expires_at > $3
             FOR UPDATE OF r, t",
            &[&refresh.0.as_slice(), &client_id, &now],
        )
        .await
        .map_err(db_error)?;
    let Some(row) = row else {
        return Ok(Refreshed::Unknown);
    };
    let token_id: String = row.get("token_id");
    if row.get::<_, bool>("spent") {
        tx.execute("DELETE FROM access_tokens WHERE id = $1", &[&token_id])
            .await
            .map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;
        return Ok(Refreshed::Reused);
    }
    tx.execute(
        "UPDATE oauth_refresh_tokens SET spent_at = $2 WHERE token_hash = $1",
        &[&refresh.0.as_slice(), &now],
    )
    .await
    .map_err(db_error)?;
    tx.execute(
        "INSERT INTO oauth_refresh_tokens (token_hash, token_id, created_at, expires_at)
         VALUES ($1, $2, $3, $4)",
        &[
            &next.refresh.0.as_slice(),
            &token_id,
            &now,
            &next.expires_at,
        ],
    )
    .await
    .map_err(db_error)?;
    tx.execute(
        "UPDATE access_tokens SET token_hash = $2, access_until = $3, expires_at = $4
         WHERE id = $1",
        &[
            &token_id,
            &next.access.0.as_slice(),
            &next.access_until,
            &next.expires_at,
        ],
    )
    .await
    .map_err(db_error)?;
    tx.execute(
        "UPDATE oauth_clients SET last_used_at = $2 WHERE id = $1",
        &[&client_id, &now],
    )
    .await
    .map_err(db_error)?;
    let row = tx
        .query_opt(
            "SELECT t.id, t.account_id, t.name, t.scopes, t.created_at, t.last_used_on,
                    t.expires_at, t.client_id, t.resource, t.access_until,
                    a.handle, a.display_name, a.created_at AS account_created_at,
                    a.publishing_suspended_at, a.edits_sites_since, a.new_flights_public
             FROM access_tokens t JOIN accounts a ON a.id = t.account_id
             WHERE t.id = $1",
            &[&token_id],
        )
        .await
        .map_err(db_error)?;
    let Some(row) = row else {
        return Ok(Refreshed::Unknown);
    };
    let rotated = (access_token_from(&row), account_of(&row));
    tx.commit().await.map_err(db_error)?;
    Ok(Refreshed::Rotated(Box::new(rotated)))
}

pub(super) async fn revoke_grant(
    client: &Client,
    secret: &SecretHash,
    client_id: &str,
) -> Result<bool, StoreError> {
    if client_id.contains('\0') {
        return Ok(false);
    }
    let deleted = client
        .execute(
            "DELETE FROM access_tokens t
             WHERE t.client_id = $2
               AND (t.token_hash = $1
                    OR t.id IN (SELECT token_id FROM oauth_refresh_tokens
                                WHERE token_hash = $1 AND spent_at IS NULL))",
            &[&secret.0.as_slice(), &client_id],
        )
        .await
        .map_err(db_error)?;
    Ok(deleted > 0)
}

pub(super) async fn prune_oauth(
    client: &mut Client,
    now: Timestamp,
    unused_since: Timestamp,
) -> Result<u64, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    tx.execute("DELETE FROM oauth_requests WHERE expires_at <= $1", &[&now])
        .await
        .map_err(db_error)?;
    tx.execute("DELETE FROM oauth_codes WHERE expires_at <= $1", &[&now])
        .await
        .map_err(db_error)?;
    let ended = tx
        .execute(
            "DELETE FROM access_tokens WHERE client_id IS NOT NULL AND expires_at <= $1",
            &[&now],
        )
        .await
        .map_err(db_error)?;
    tx.execute(
        "DELETE FROM oauth_refresh_tokens WHERE expires_at <= $1",
        &[&now],
    )
    .await
    .map_err(db_error)?;
    let idle = tx
        .execute(
            "DELETE FROM oauth_clients c
             WHERE coalesce(c.last_used_at, c.created_at) < $1
               AND NOT EXISTS (SELECT 1 FROM access_tokens t WHERE t.client_id = c.id)",
            &[&unused_since],
        )
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(ended + idle)
}
