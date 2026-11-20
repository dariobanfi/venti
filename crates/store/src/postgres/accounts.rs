//! Accounts, sessions and invites in PostgreSQL.

use chrono::Duration;
use deadpool_postgres::Client;
use tokio_postgres::Row;
use venti_domain::Timestamp;

use super::{db_error, groups};
use crate::flight::next_change;
use crate::{
    Account, AccountCreation, NewAccount, NewInvite, SESSION_TOUCH_MINUTES, SecretHash, Session,
    StoreError, Suspension,
};

fn account_from(row: &Row) -> Account {
    Account {
        id: row.get("account_id"),
        handle: row.get("handle"),
        display_name: row.get("display_name"),
        created_at: row.get("account_created_at"),
        publishing_suspended_at: row.get("publishing_suspended_at"),
    }
}

fn session_from(row: &Row) -> Session {
    Session {
        id: row.get("id"),
        account_id: row.get("account_id"),
        device: row.get("device"),
        created_at: row.get("created_at"),
        last_used_at: row.get("last_used_at"),
    }
}

/// The columns [`account_from`] reads, for an `accounts` table named `a`.
const ACCOUNT_COLUMNS: &str = "a.id AS account_id, a.handle, a.display_name, \
     a.created_at AS account_created_at, a.publishing_suspended_at";

pub(super) async fn create(
    client: &mut Client,
    new: &NewAccount,
) -> Result<AccountCreation, StoreError> {
    let account = &new.account;
    // Dropping the transaction without a commit undoes what was done in it.
    let tx = client.transaction().await.map_err(db_error)?;

    // The invite first: without a good one, nothing is said about handles.
    // Marking it used locks its row, so of two requests that race for one
    // invite the second waits here and then finds it used.
    if let Some(code) = &new.invite {
        let claimed = tx
            .execute(
                "UPDATE invites SET used_at = $2
                 WHERE code_hash = $1 AND used_at IS NULL
                   AND (expires_at IS NULL OR expires_at > $2)",
                &[&code.0.as_slice(), &account.created_at],
            )
            .await
            .map_err(db_error)?;
        if claimed == 0 {
            return Ok(AccountCreation::InviteRefused);
        }
    }

    let inserted = tx
        .execute(
            "INSERT INTO accounts (id, handle, display_name, created_at)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (handle) DO NOTHING",
            &[
                &account.id,
                &account.handle,
                &account.display_name,
                &account.created_at,
            ],
        )
        .await
        .map_err(db_error)?;
    if inserted == 0 {
        return Ok(AccountCreation::HandleTaken);
    }

    tx.execute(
        "INSERT INTO credentials (account_id, kind, secret, created_at)
         VALUES ($1, 'password', $2, $3)",
        &[&account.id, &new.password_hash, &account.created_at],
    )
    .await
    .map_err(db_error)?;
    if let Some(code) = &new.invite {
        tx.execute(
            "UPDATE invites SET used_by = $2 WHERE code_hash = $1",
            &[&code.0.as_slice(), &account.id],
        )
        .await
        .map_err(db_error)?;
    }
    tx.commit().await.map_err(db_error)?;
    Ok(AccountCreation::Created)
}

pub(super) async fn by_handle(
    client: &Client,
    handle: &str,
) -> Result<Option<Account>, StoreError> {
    let query = format!("SELECT {ACCOUNT_COLUMNS} FROM accounts a WHERE a.handle = $1");
    let statement = client.prepare_cached(&query).await.map_err(db_error)?;
    let row = client
        .query_opt(&statement, &[&handle])
        .await
        .map_err(db_error)?;
    Ok(row.as_ref().map(account_from))
}

pub(super) async fn password_hash(
    client: &Client,
    handle: &str,
) -> Result<Option<(Account, String)>, StoreError> {
    let query = format!(
        "SELECT {ACCOUNT_COLUMNS}, c.secret
         FROM accounts a
         JOIN credentials c ON c.account_id = a.id AND c.kind = 'password'
         WHERE a.handle = $1"
    );
    let statement = client.prepare_cached(&query).await.map_err(db_error)?;
    let row = client
        .query_opt(&statement, &[&handle])
        .await
        .map_err(db_error)?;
    Ok(row
        .as_ref()
        .map(|row| (account_from(row), row.get("secret"))))
}

pub(super) async fn set_password(
    client: &mut Client,
    account_id: &str,
    password_hash: &str,
    keep_session: &str,
) -> Result<(), StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    tx.execute(
        "UPDATE credentials SET secret = $2
         WHERE account_id = $1 AND kind = 'password'",
        &[&account_id, &password_hash],
    )
    .await
    .map_err(db_error)?;
    tx.execute(
        "DELETE FROM sessions WHERE account_id = $1 AND id <> $2",
        &[&account_id, &keep_session],
    )
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)
}

/// How often a deletion that another call got in the way of is begun
/// again before the failure is passed on.
const DELETION_TRIES: u32 = 4;

pub(super) async fn delete(client: &mut Client, account_id: &str) -> Result<(), StoreError> {
    // A deletion can meet a call that makes this account the keeper of a
    // group, or one that deletes the account that would keep a group of
    // this one's next. The database then refuses the step that would leave
    // a group without its keeper, or ends one of two transactions that
    // wait for each other. Begun again, the deletion finds things as the
    // other call left them.
    let mut tries = 1;
    loop {
        match delete_once(client, account_id).await {
            Err(error) if tries < DELETION_TRIES && groups::is_a_race(&error) => tries += 1,
            outcome => return outcome.map_err(db_error),
        }
    }
}

async fn delete_once(client: &mut Client, account_id: &str) -> Result<(), tokio_postgres::Error> {
    let tx = client.transaction().await?;
    // First the groups the account keeps: each passes to its next keeper,
    // or goes if nobody else is in it.
    if groups::release_account(&tx, account_id).await? {
        // Everything else that belongs to the account names it and goes
        // with it, its places in groups and what it put into them
        // included.
        tx.execute("DELETE FROM accounts WHERE id = $1", &[&account_id])
            .await?;
    }
    tx.commit().await
}

pub(super) async fn suspend_publishing(
    client: &mut Client,
    account_id: &str,
    now: Timestamp,
) -> Result<Option<Suspension>, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    // The account's row is locked for the rest of the transaction, as it
    // is by everything that changes one of its flights: no flight is made
    // public beside this, and the change numbers given out here are the
    // account's next.
    let row = tx
        .query_opt(
            "SELECT flight_change, publishing_suspended_at FROM accounts
             WHERE id = $1 FOR UPDATE",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let latest: i64 = row.get(0);
    let since = row.get::<_, Option<Timestamp>>(1).unwrap_or(now);
    tx.execute(
        "UPDATE accounts SET publishing_suspended_at = $2 WHERE id = $1",
        &[&account_id, &since],
    )
    .await
    .map_err(db_error)?;

    // Each public flight becomes unlisted as a change of its own, in the
    // order of the flights' addresses.
    let first = next_change(latest, now);
    let flights = tx
        .execute(
            "WITH public AS (
                 SELECT id, row_number() OVER (ORDER BY id) AS place
                 FROM flights WHERE account_id = $1 AND visibility = 'public'
             )
             UPDATE flights f SET visibility = 'unlisted', change = $2 + public.place - 1
             FROM public WHERE f.id = public.id",
            &[&account_id, &first],
        )
        .await
        .map_err(db_error)?;
    if flights > 0 {
        let last = first.saturating_add(i64::try_from(flights - 1).unwrap_or(i64::MAX));
        tx.execute(
            "UPDATE accounts SET flight_change = $2 WHERE id = $1",
            &[&account_id, &last],
        )
        .await
        .map_err(db_error)?;
    }
    // A track that takes in a batch at this moment holds its row locked;
    // this waits for it, looks at the row again and narrows it.
    let live_tracks = tx
        .execute(
            "UPDATE live_tracks SET visibility = 'link'
             WHERE account_id = $1 AND visibility = 'public'",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(Some(Suspension {
        since,
        flights,
        live_tracks,
    }))
}

pub(super) async fn resume_publishing(
    client: &mut Client,
    account_id: &str,
) -> Result<Option<bool>, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let row = tx
        .query_opt(
            "SELECT publishing_suspended_at IS NOT NULL FROM accounts WHERE id = $1 FOR UPDATE",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let was_suspended: bool = row.get(0);
    if was_suspended {
        tx.execute(
            "UPDATE accounts SET publishing_suspended_at = NULL WHERE id = $1",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    }
    tx.commit().await.map_err(db_error)?;
    Ok(Some(was_suspended))
}

pub(super) async fn create_invite(client: &Client, invite: &NewInvite) -> Result<(), StoreError> {
    client
        .execute(
            "INSERT INTO invites (code_hash, created_at, expires_at)
             VALUES ($1, $2, $3)
             ON CONFLICT (code_hash) DO NOTHING",
            &[
                &invite.code.0.as_slice(),
                &invite.created_at,
                &invite.expires_at,
            ],
        )
        .await
        .map_err(db_error)?;
    Ok(())
}

pub(super) async fn create_session(
    client: &mut Client,
    session: &Session,
    token: &SecretHash,
    keep: usize,
) -> Result<(), StoreError> {
    let keep = i64::try_from(keep).unwrap_or(i64::MAX);
    let tx = client.transaction().await.map_err(db_error)?;
    tx.execute(
        "INSERT INTO sessions (id, account_id, token_hash, device, created_at, last_used_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
        &[
            &session.id,
            &session.account_id,
            &token.0.as_slice(),
            &session.device,
            &session.created_at,
            &session.last_used_at,
        ],
    )
    .await
    .map_err(db_error)?;
    tx.execute(
        "DELETE FROM sessions
         WHERE id IN (
             SELECT id FROM sessions WHERE account_id = $1
             ORDER BY last_used_at DESC, id DESC
             OFFSET $2
         )",
        &[&session.account_id, &keep],
    )
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)
}

/// One statement finds the session, checks that it has not expired and
/// writes down the use if the recorded one is old. The outer query sees the
/// row as it was before the update, so it works out the new time itself.
const USE_SESSION: &str = "
    WITH touched AS (
        UPDATE sessions SET last_used_at = $2
        WHERE token_hash = $1 AND last_used_at >= $3 AND last_used_at < $4
    )
    SELECT s.id, s.account_id, s.device, s.created_at,
           CASE WHEN s.last_used_at < $4 THEN $2 ELSE s.last_used_at END AS last_used_at,
           a.handle, a.display_name, a.created_at AS account_created_at,
           a.publishing_suspended_at
    FROM sessions s
    JOIN accounts a ON a.id = s.account_id
    WHERE s.token_hash = $1 AND s.last_used_at >= $3";

pub(super) async fn use_session(
    client: &Client,
    token: &SecretHash,
    now: Timestamp,
    idle_since: Timestamp,
) -> Result<Option<(Session, Account)>, StoreError> {
    let statement = client.prepare_cached(USE_SESSION).await.map_err(db_error)?;
    let touch_before = now - Duration::minutes(SESSION_TOUCH_MINUTES);
    let row = client
        .query_opt(
            &statement,
            &[&token.0.as_slice(), &now, &idle_since, &touch_before],
        )
        .await
        .map_err(db_error)?;
    Ok(row
        .as_ref()
        .map(|row| (session_from(row), account_from(row))))
}

pub(super) async fn sessions(
    client: &Client,
    account_id: &str,
) -> Result<Vec<Session>, StoreError> {
    let statement = client
        .prepare_cached(
            "SELECT id, account_id, device, created_at, last_used_at
             FROM sessions WHERE account_id = $1
             ORDER BY last_used_at DESC, id DESC",
        )
        .await
        .map_err(db_error)?;
    let rows = client
        .query(&statement, &[&account_id])
        .await
        .map_err(db_error)?;
    Ok(rows.iter().map(session_from).collect())
}

pub(super) async fn delete_session(
    client: &Client,
    account_id: &str,
    session_id: &str,
) -> Result<bool, StoreError> {
    // PostgreSQL takes no zero byte in text and would refuse the whole
    // statement. No stored identifier holds one, so the answer is known.
    if account_id.contains('\0') || session_id.contains('\0') {
        return Ok(false);
    }
    let deleted = client
        .execute(
            "DELETE FROM sessions WHERE account_id = $1 AND id = $2",
            &[&account_id, &session_id],
        )
        .await
        .map_err(db_error)?;
    Ok(deleted > 0)
}

pub(super) async fn delete_sessions(client: &Client, account_id: &str) -> Result<(), StoreError> {
    client
        .execute("DELETE FROM sessions WHERE account_id = $1", &[&account_id])
        .await
        .map_err(db_error)?;
    Ok(())
}

pub(super) async fn prune_sessions(
    client: &Client,
    idle_since: Timestamp,
) -> Result<u64, StoreError> {
    client
        .execute(
            "DELETE FROM sessions WHERE last_used_at < $1",
            &[&idle_since],
        )
        .await
        .map_err(db_error)
}
