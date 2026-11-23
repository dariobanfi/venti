//! Accounts, sessions and invites in PostgreSQL.

use chrono::Duration;
use deadpool_postgres::Client;
use tokio_postgres::Row;
use venti_domain::Timestamp;

use super::{db_error, flights, groups};
use crate::flight::next_change;
use crate::{
    AccessToken, Account, AccountCreation, AccountSettings, GliderClass, GoogleLink, GoogleLinking,
    NewAccount, NewGoogleAccount, NewInvite, SESSION_TOUCH_MINUTES, SecretHash, Session,
    StoreError, Suspension, TokenClient,
};

fn account_from(row: &Row) -> Account {
    Account {
        id: row.get("account_id"),
        handle: row.get("handle"),
        display_name: row.get("display_name"),
        created_at: row.get("account_created_at"),
        publishing_suspended_at: row.get("publishing_suspended_at"),
        edits_sites_since: row.get("edits_sites_since"),
        new_flights_public: row.get("new_flights_public"),
        country: row.get("country"),
        glider_class: row
            .get::<_, Option<&str>>("glider_class")
            .and_then(GliderClass::from_name),
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
     a.created_at AS account_created_at, a.publishing_suspended_at, a.edits_sites_since, \
     a.new_flights_public, a.country, a.glider_class";

pub(super) async fn create(
    client: &mut Client,
    new: &NewAccount,
) -> Result<AccountCreation, StoreError> {
    create_with(
        client,
        &new.account,
        &Credential::Password(&new.password_hash),
        new.invite.as_ref(),
    )
    .await
}

pub(super) async fn create_google(
    client: &mut Client,
    new: &NewGoogleAccount,
) -> Result<AccountCreation, StoreError> {
    create_with(
        client,
        &new.account,
        &Credential::Google(&new.google),
        new.invite.as_ref(),
    )
    .await
}

/// What an account is created with.
enum Credential<'a> {
    Password(&'a str),
    Google(&'a GoogleLink),
}

async fn create_with(
    client: &mut Client,
    account: &Account,
    credential: &Credential<'_>,
    invite: Option<&SecretHash>,
) -> Result<AccountCreation, StoreError> {
    // Dropping the transaction without a commit undoes what was done in it.
    let tx = client.transaction().await.map_err(db_error)?;

    // The invite first: without a good one, nothing is said about handles.
    // Marking it used locks its row, so of two requests that race for one
    // invite the second waits here and then finds it used.
    if let Some(code) = invite {
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
            "INSERT INTO accounts (id, handle, display_name, created_at, new_flights_public)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (handle) DO NOTHING",
            &[
                &account.id,
                &account.handle,
                &account.display_name,
                &account.created_at,
                &account.new_flights_public,
            ],
        )
        .await
        .map_err(db_error)?;
    if inserted == 0 {
        return Ok(AccountCreation::HandleTaken);
    }

    match credential {
        Credential::Password(hash) => {
            tx.execute(
                "INSERT INTO credentials (account_id, kind, secret, created_at)
                 VALUES ($1, 'password', $2, $3)",
                &[&account.id, hash, &account.created_at],
            )
            .await
            .map_err(db_error)?;
        }
        Credential::Google(google) => {
            // Of two requests that race for one Google account, the second
            // waits here for the first and then finds it taken.
            let inserted = tx
                .execute(
                    "INSERT INTO credentials (account_id, kind, secret, email, created_at)
                     VALUES ($1, 'google', $2, $3, $4)
                     ON CONFLICT (secret) WHERE kind = 'google' DO NOTHING",
                    &[
                        &account.id,
                        &google.subject,
                        &google.email,
                        &google.linked_at,
                    ],
                )
                .await
                .map_err(db_error)?;
            if inserted == 0 {
                return Ok(AccountCreation::GoogleTaken);
            }
        }
    }
    if let Some(code) = invite {
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

pub(super) async fn by_google(
    client: &Client,
    subject: &str,
) -> Result<Option<Account>, StoreError> {
    if subject.contains('\0') {
        return Ok(None);
    }
    let query = format!(
        "SELECT {ACCOUNT_COLUMNS}
         FROM credentials c JOIN accounts a ON a.id = c.account_id
         WHERE c.kind = 'google' AND c.secret = $1"
    );
    let statement = client.prepare_cached(&query).await.map_err(db_error)?;
    let row = client
        .query_opt(&statement, &[&subject])
        .await
        .map_err(db_error)?;
    Ok(row.as_ref().map(account_from))
}

pub(super) async fn google_link(
    client: &Client,
    account_id: &str,
) -> Result<Option<GoogleLink>, StoreError> {
    let statement = client
        .prepare_cached(
            "SELECT secret, email, created_at FROM credentials
             WHERE account_id = $1 AND kind = 'google'",
        )
        .await
        .map_err(db_error)?;
    let row = client
        .query_opt(&statement, &[&account_id])
        .await
        .map_err(db_error)?;
    Ok(row.map(|row| GoogleLink {
        subject: row.get(0),
        email: row.get::<_, Option<String>>(1).unwrap_or_default(),
        linked_at: row.get(2),
    }))
}

pub(super) async fn link_google(
    client: &mut Client,
    account_id: &str,
    google: &GoogleLink,
) -> Result<GoogleLinking, StoreError> {
    if google.subject.contains('\0') || google.email.contains('\0') {
        return Ok(GoogleLinking::TakenByAnother);
    }
    let tx = client.transaction().await.map_err(db_error)?;
    // The account's row is locked, so that two links of one account wait
    // for each other.
    let account = tx
        .query_opt(
            "SELECT 1 FROM accounts WHERE id = $1 FOR UPDATE",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    if account.is_none() {
        return Ok(GoogleLinking::NoSuchAccount);
    }
    let holder = tx
        .query_opt(
            "SELECT account_id FROM credentials WHERE kind = 'google' AND secret = $1",
            &[&google.subject],
        )
        .await
        .map_err(db_error)?
        .map(|row| row.get::<_, String>(0));
    match holder {
        Some(holder) if holder != account_id => return Ok(GoogleLinking::TakenByAnother),
        Some(_) => {
            tx.execute(
                "UPDATE credentials SET email = $2 WHERE kind = 'google' AND secret = $1",
                &[&google.subject, &google.email],
            )
            .await
            .map_err(db_error)?;
        }
        None => {
            tx.execute(
                "DELETE FROM credentials WHERE account_id = $1 AND kind = 'google'",
                &[&account_id],
            )
            .await
            .map_err(db_error)?;
            // Another account can take the Google account between the look
            // and this: the index then refuses, and nothing changes.
            let inserted = tx
                .execute(
                    "INSERT INTO credentials (account_id, kind, secret, email, created_at)
                     VALUES ($1, 'google', $2, $3, $4)
                     ON CONFLICT (secret) WHERE kind = 'google' DO NOTHING",
                    &[
                        &account_id,
                        &google.subject,
                        &google.email,
                        &google.linked_at,
                    ],
                )
                .await
                .map_err(db_error)?;
            if inserted == 0 {
                return Ok(GoogleLinking::TakenByAnother);
            }
        }
    }
    tx.commit().await.map_err(db_error)?;
    Ok(GoogleLinking::Linked)
}

pub(super) async fn unlink_google(client: &Client, account_id: &str) -> Result<bool, StoreError> {
    let deleted = client
        .execute(
            "DELETE FROM credentials WHERE account_id = $1 AND kind = 'google'",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    Ok(deleted > 0)
}

pub(super) async fn has_password(client: &Client, account_id: &str) -> Result<bool, StoreError> {
    let row = client
        .query_opt(
            "SELECT 1 FROM credentials WHERE account_id = $1 AND kind = 'password'",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    Ok(row.is_some())
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
        //
        // Its public flights are withdrawn first, in the order of their
        // addresses, so that whoever copied them learns that they are
        // gone. The account's row is locked before the count of public
        // changes, as by everything else that takes a number from it.
        tx.execute(
            "SELECT 1 FROM accounts WHERE id = $1 FOR UPDATE",
            &[&account_id],
        )
        .await?;
        let public = tx
            .query(
                "SELECT id FROM flights
                 WHERE account_id = $1 AND visibility = 'public' ORDER BY id",
                &[&account_id],
            )
            .await?;
        if !public.is_empty() {
            let ids: Vec<String> = public.iter().map(|row| row.get(0)).collect();
            let count = i64::try_from(ids.len()).unwrap_or(i64::MAX);
            let number = flights::take_public_changes(&tx, count, None).await?;
            flights::record_withdrawals(&tx, &ids, None, number, None).await?;
        }
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
    let unlisted = tx
        .query(
            "WITH public AS (
                 SELECT id, row_number() OVER (ORDER BY id) AS place
                 FROM flights WHERE account_id = $1 AND visibility = 'public'
             )
             UPDATE flights f
             SET visibility = 'unlisted', change = $2 + public.place - 1,
                 published_at = NULL, public_change = NULL
             FROM public WHERE f.id = public.id
             RETURNING f.id",
            &[&account_id, &first],
        )
        .await
        .map_err(db_error)?;
    let flights = unlisted.len() as u64;
    if flights > 0 {
        // Each of them is withdrawn from the public, in the same order.
        let mut ids: Vec<String> = unlisted.iter().map(|row| row.get(0)).collect();
        ids.sort_unstable();
        let count = i64::try_from(ids.len()).unwrap_or(i64::MAX);
        let number = flights::take_public_changes(&tx, count, Some(now))
            .await
            .map_err(db_error)?;
        flights::record_withdrawals(&tx, &ids, Some(account_id), number, Some(now))
            .await
            .map_err(db_error)?;
    }
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

pub(super) async fn let_edit_sites(
    client: &Client,
    account_id: &str,
    now: Timestamp,
) -> Result<Option<Timestamp>, StoreError> {
    // One statement: an account that is trusted already keeps its moment.
    let row = client
        .query_opt(
            "UPDATE accounts SET edits_sites_since = COALESCE(edits_sites_since, $2)
             WHERE id = $1 RETURNING edits_sites_since",
            &[&account_id, &now],
        )
        .await
        .map_err(db_error)?;
    Ok(row.map(|row| row.get(0)))
}

pub(super) async fn stop_editing_sites(
    client: &mut Client,
    account_id: &str,
) -> Result<Option<bool>, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let row = tx
        .query_opt(
            "SELECT edits_sites_since IS NOT NULL FROM accounts WHERE id = $1 FOR UPDATE",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let was_trusted: bool = row.get(0);
    if was_trusted {
        tx.execute(
            "UPDATE accounts SET edits_sites_since = NULL WHERE id = $1",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    }
    tx.commit().await.map_err(db_error)?;
    Ok(Some(was_trusted))
}

pub(super) async fn site_editors(client: &Client) -> Result<Vec<Account>, StoreError> {
    let query = format!(
        "SELECT {ACCOUNT_COLUMNS} FROM accounts a
         WHERE a.edits_sites_since IS NOT NULL ORDER BY a.handle"
    );
    let rows = client.query(&query, &[]).await.map_err(db_error)?;
    Ok(rows.iter().map(account_from).collect())
}

pub(super) async fn update_settings(
    client: &Client,
    account_id: &str,
    settings: &AccountSettings,
) -> Result<Option<Account>, StoreError> {
    // One statement: an upload that locks the row reads the settings as
    // they were before or as they are after, never between. A setting
    // that is not named keeps its value.
    let query = format!(
        "UPDATE accounts a SET
             new_flights_public = COALESCE($2, a.new_flights_public),
             country = CASE WHEN $3 THEN $4 ELSE a.country END,
             glider_class = CASE WHEN $5 THEN $6 ELSE a.glider_class END
         WHERE a.id = $1
         RETURNING {ACCOUNT_COLUMNS}"
    );
    let country = settings.country.clone().flatten();
    let class = settings.glider_class.flatten().map(GliderClass::as_str);
    let row = client
        .query_opt(
            &query,
            &[
                &account_id,
                &settings.new_flights_public,
                &settings.country.is_some(),
                &country,
                &settings.glider_class.is_some(),
                &class,
            ],
        )
        .await
        .map_err(db_error)?;
    Ok(row.as_ref().map(account_from))
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
           a.publishing_suspended_at, a.edits_sites_since, a.new_flights_public,
           a.country, a.glider_class
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

pub(super) fn access_token_from(row: &Row) -> AccessToken {
    let client_id: Option<String> = row.get("client_id");
    let resource: Option<String> = row.get("resource");
    let access_until: Option<Timestamp> = row.get("access_until");
    // The three are set together or not at all: the table checks it.
    let client = match (client_id, resource, access_until) {
        (Some(client_id), Some(resource), Some(access_until)) => Some(TokenClient {
            client_id,
            resource,
            access_until,
        }),
        _ => None,
    };
    AccessToken {
        id: row.get("id"),
        account_id: row.get("account_id"),
        name: row.get("name"),
        scopes: row.get("scopes"),
        created_at: row.get("created_at"),
        last_used_on: row.get("last_used_on"),
        expires_at: row.get("expires_at"),
        client,
    }
}

pub(super) fn account_of(row: &Row) -> Account {
    account_from(row)
}

pub(super) async fn create_access_token(
    client: &mut Client,
    token: &AccessToken,
    secret: &SecretHash,
    most: usize,
) -> Result<bool, StoreError> {
    // A token that names a program comes of an exchanged code.
    if token.client.is_some() {
        return Ok(false);
    }
    let most = i64::try_from(most).unwrap_or(i64::MAX);
    let tx = client.transaction().await.map_err(db_error)?;
    // The account's row is locked while its tokens are counted, so that of
    // two requests that race for the last place the second waits here and
    // then finds it taken.
    let account = tx
        .query_opt(
            "SELECT 1 FROM accounts WHERE id = $1 FOR UPDATE",
            &[&token.account_id],
        )
        .await
        .map_err(db_error)?;
    if account.is_none() {
        return Ok(false);
    }
    let held: i64 = tx
        .query_one(
            "SELECT count(*) FROM access_tokens WHERE account_id = $1",
            &[&token.account_id],
        )
        .await
        .map_err(db_error)?
        .get(0);
    if held >= most {
        return Ok(false);
    }
    tx.execute(
        "INSERT INTO access_tokens
             (id, account_id, token_hash, name, scopes, created_at, last_used_on, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        &[
            &token.id,
            &token.account_id,
            &secret.0.as_slice(),
            &token.name,
            &token.scopes,
            &token.created_at,
            &token.last_used_on,
            &token.expires_at,
        ],
    )
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(true)
}

/// One statement finds the token, checks that its end has not come and
/// writes down the day if the recorded one is an earlier day. The outer
/// query sees the row as it was before the update, so it works out the new
/// day itself.
const USE_ACCESS_TOKEN: &str = "
    WITH touched AS (
        UPDATE access_tokens SET last_used_on = $3
        WHERE token_hash = $1 AND (expires_at IS NULL OR expires_at > $2)
          AND (access_until IS NULL OR access_until > $2)
          AND (last_used_on IS NULL OR last_used_on < $3)
    )
    SELECT t.id, t.account_id, t.name, t.scopes, t.created_at, t.expires_at,
           t.client_id, t.resource, t.access_until,
           CASE WHEN t.last_used_on IS NULL OR t.last_used_on < $3 THEN $3
                ELSE t.last_used_on END AS last_used_on,
           a.handle, a.display_name, a.created_at AS account_created_at,
           a.publishing_suspended_at, a.edits_sites_since, a.new_flights_public,
           a.country, a.glider_class
    FROM access_tokens t
    JOIN accounts a ON a.id = t.account_id
    WHERE t.token_hash = $1 AND (t.expires_at IS NULL OR t.expires_at > $2)
      AND (t.access_until IS NULL OR t.access_until > $2)";

pub(super) async fn use_access_token(
    client: &Client,
    secret: &SecretHash,
    now: Timestamp,
) -> Result<Option<(AccessToken, Account)>, StoreError> {
    let statement = client
        .prepare_cached(USE_ACCESS_TOKEN)
        .await
        .map_err(db_error)?;
    let row = client
        .query_opt(&statement, &[&secret.0.as_slice(), &now, &now.date_naive()])
        .await
        .map_err(db_error)?;
    Ok(row
        .as_ref()
        .map(|row| (access_token_from(row), account_from(row))))
}

pub(super) async fn access_tokens(
    client: &Client,
    account_id: &str,
) -> Result<Vec<AccessToken>, StoreError> {
    let statement = client
        .prepare_cached(
            "SELECT id, account_id, name, scopes, created_at, last_used_on, expires_at,
                    client_id, resource, access_until
             FROM access_tokens WHERE account_id = $1
             ORDER BY created_at DESC, id DESC",
        )
        .await
        .map_err(db_error)?;
    let rows = client
        .query(&statement, &[&account_id])
        .await
        .map_err(db_error)?;
    Ok(rows.iter().map(access_token_from).collect())
}

pub(super) async fn delete_access_token(
    client: &Client,
    account_id: &str,
    token_id: &str,
) -> Result<bool, StoreError> {
    // As for a session: no stored identifier holds a zero byte.
    if account_id.contains('\0') || token_id.contains('\0') {
        return Ok(false);
    }
    let deleted = client
        .execute(
            "DELETE FROM access_tokens WHERE account_id = $1 AND id = $2",
            &[&account_id, &token_id],
        )
        .await
        .map_err(db_error)?;
    Ok(deleted > 0)
}
