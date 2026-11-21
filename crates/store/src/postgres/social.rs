//! Follows, cheers, comments and notices in PostgreSQL.

use deadpool_postgres::Client;
use tokio_postgres::Row;
use tokio_postgres::types::ToSql;
use venti_domain::Timestamp;

use super::db_error;
use super::flights::{FLIGHT_COLUMNS, FLIGHT_FROM, flight_from};
use crate::flight::next_change;
use crate::{
    Cheering, Comment, Commenting, Flight, FlightKey, FlightSocial, FollowCounts, FollowSide,
    Following, NewComment, Notice, Notices, SocialGiven, SocialKey, SocialPilot, StoreError,
};

fn as_limit(limit: usize) -> i64 {
    i64::try_from(limit).unwrap_or(i64::MAX)
}

fn as_count(count: i64) -> u64 {
    u64::try_from(count).unwrap_or(0)
}

fn pilot(row: &Row) -> SocialPilot {
    SocialPilot {
        account_id: row.get("id"),
        handle: row.get("handle"),
        name: row.get("display_name"),
        at: row.get("at"),
    }
}

pub(super) async fn follow(
    client: &mut Client,
    follower: &str,
    followed: &str,
    now: Timestamp,
    most: usize,
) -> Result<Following, StoreError> {
    if follower == followed {
        return Ok(Following::NoAccount);
    }
    let tx = client.transaction().await.map_err(db_error)?;
    // The follower's row, held: two follows of one account come one after
    // the other, and the count holds.
    let there: i64 = tx
        .query_one(
            "SELECT count(*) FROM (
                 SELECT 1 FROM accounts WHERE id IN ($1, $2) ORDER BY id FOR NO KEY UPDATE
             ) both_accounts",
            &[&follower, &followed],
        )
        .await
        .map_err(db_error)?
        .get(0);
    if there != 2 {
        return Ok(Following::NoAccount);
    }
    let already = tx
        .query_opt(
            "SELECT 1 FROM follows WHERE follower_id = $1 AND followed_id = $2",
            &[&follower, &followed],
        )
        .await
        .map_err(db_error)?;
    if already.is_some() {
        return Ok(Following::Follows { new: false });
    }
    let have: i64 = tx
        .query_one(
            "SELECT count(*) FROM follows WHERE follower_id = $1",
            &[&follower],
        )
        .await
        .map_err(db_error)?
        .get(0);
    if usize::try_from(have).unwrap_or(usize::MAX) >= most {
        return Ok(Following::Full);
    }
    tx.execute(
        "INSERT INTO follows (follower_id, followed_id, followed_at) VALUES ($1, $2, $3)",
        &[&follower, &followed, &now],
    )
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(Following::Follows { new: true })
}

pub(super) async fn unfollow(
    client: &Client,
    follower: &str,
    followed: &str,
) -> Result<bool, StoreError> {
    let removed = client
        .execute(
            "DELETE FROM follows WHERE follower_id = $1 AND followed_id = $2",
            &[&follower, &followed],
        )
        .await
        .map_err(db_error)?;
    Ok(removed > 0)
}

pub(super) async fn counts(client: &Client, account_id: &str) -> Result<FollowCounts, StoreError> {
    let row = client
        .query_one(
            "SELECT (SELECT count(*) FROM follows WHERE followed_id = $1),
                    (SELECT count(*) FROM follows WHERE follower_id = $1)",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    Ok(FollowCounts {
        followers: as_count(row.get(0)),
        following: as_count(row.get(1)),
    })
}

pub(super) async fn follows_pilot(
    client: &Client,
    follower: &str,
    followed: &str,
) -> Result<bool, StoreError> {
    let row = client
        .query_opt(
            "SELECT 1 FROM follows WHERE follower_id = $1 AND followed_id = $2",
            &[&follower, &followed],
        )
        .await
        .map_err(db_error)?;
    Ok(row.is_some())
}

pub(super) async fn follows(
    client: &Client,
    account_id: &str,
    side: FollowSide,
    with_a_page: bool,
    after: Option<&SocialKey>,
    limit: usize,
) -> Result<Vec<SocialPilot>, StoreError> {
    let (of, other) = match side {
        FollowSide::Followers => ("followed_id", "follower_id"),
        FollowSide::Following => ("follower_id", "followed_id"),
    };
    let page = match after {
        Some(_) => format!("AND (w.followed_at, w.{other}) < ($3, $4)"),
        None => String::new(),
    };
    let only = if with_a_page {
        "AND EXISTS (SELECT 1 FROM flights f
                     WHERE f.account_id = a.id AND f.visibility = 'public')"
    } else {
        ""
    };
    let query = format!(
        "SELECT a.id, a.handle, a.display_name, w.followed_at AS at
         FROM follows w JOIN accounts a ON a.id = w.{other}
         WHERE w.{of} = $1 {page} {only}
         ORDER BY w.followed_at DESC, w.{other} DESC
         LIMIT $2"
    );
    let statement = client.prepare_cached(&query).await.map_err(db_error)?;
    let limit = as_limit(limit);
    let mut parameters: Vec<&(dyn ToSql + Sync)> = vec![&account_id, &limit];
    if let Some((at, id)) = after {
        parameters.extend([at as &(dyn ToSql + Sync), id]);
    }
    let rows = client
        .query(&statement, &parameters)
        .await
        .map_err(db_error)?;
    Ok(rows.iter().map(pilot).collect())
}

pub(super) async fn followed_flights(
    client: &Client,
    account_id: &str,
    after: Option<&FlightKey>,
    limit: usize,
) -> Result<Vec<Flight>, StoreError> {
    let page = match after {
        Some(_) => "AND (f.started_at, f.id) < ($3, $4)",
        None => "",
    };
    let query = format!(
        "SELECT {FLIGHT_COLUMNS} {FLIGHT_FROM}
         WHERE f.visibility = 'public'
           AND f.account_id IN (SELECT followed_id FROM follows WHERE follower_id = $1)
           {page}
         ORDER BY f.started_at DESC, f.id DESC
         LIMIT $2"
    );
    let statement = client.prepare_cached(&query).await.map_err(db_error)?;
    let limit = as_limit(limit);
    let mut parameters: Vec<&(dyn ToSql + Sync)> = vec![&account_id, &limit];
    if let Some((start, id)) = after {
        parameters.extend([start as &(dyn ToSql + Sync), id]);
    }
    let rows = client
        .query(&statement, &parameters)
        .await
        .map_err(db_error)?;
    rows.iter().map(flight_from).collect()
}

pub(super) async fn cheer(
    client: &Client,
    account_id: &str,
    flight_id: &str,
    now: Timestamp,
) -> Result<Cheering, StoreError> {
    // One statement: the cheer is stored if the account is there and the
    // flight is public.
    let stored = client
        .execute(
            "INSERT INTO flight_cheers (flight_id, account_id, cheered_at)
             SELECT f.id, a.id, $3 FROM flights f, accounts a
             WHERE f.id = $1 AND f.visibility = 'public' AND a.id = $2
             ON CONFLICT (flight_id, account_id) DO NOTHING",
            &[&flight_id, &account_id, &now],
        )
        .await
        .map_err(db_error)?;
    if stored > 0 {
        return Ok(Cheering::Cheers { new: true });
    }
    let there = client
        .query_opt(
            "SELECT 1 FROM flight_cheers c JOIN flights f ON f.id = c.flight_id
             WHERE c.flight_id = $1 AND c.account_id = $2 AND f.visibility = 'public'",
            &[&flight_id, &account_id],
        )
        .await
        .map_err(db_error)?;
    Ok(match there {
        Some(_) => Cheering::Cheers { new: false },
        None => Cheering::NoSuchFlight,
    })
}

pub(super) async fn uncheer(
    client: &Client,
    account_id: &str,
    flight_id: &str,
) -> Result<bool, StoreError> {
    let removed = client
        .execute(
            "DELETE FROM flight_cheers WHERE flight_id = $1 AND account_id = $2",
            &[&flight_id, &account_id],
        )
        .await
        .map_err(db_error)?;
    Ok(removed > 0)
}

pub(super) async fn cheers(
    client: &Client,
    flight_id: &str,
    after: Option<&SocialKey>,
    limit: usize,
) -> Result<Vec<SocialPilot>, StoreError> {
    let page = match after {
        Some(_) => "AND (c.cheered_at, c.account_id) < ($3, $4)",
        None => "",
    };
    let query = format!(
        "SELECT a.id, a.handle, a.display_name, c.cheered_at AS at
         FROM flight_cheers c JOIN accounts a ON a.id = c.account_id
         WHERE c.flight_id = $1 {page}
         ORDER BY c.cheered_at DESC, c.account_id DESC
         LIMIT $2"
    );
    let statement = client.prepare_cached(&query).await.map_err(db_error)?;
    let limit = as_limit(limit);
    let mut parameters: Vec<&(dyn ToSql + Sync)> = vec![&flight_id, &limit];
    if let Some((at, id)) = after {
        parameters.extend([at as &(dyn ToSql + Sync), id]);
    }
    let rows = client
        .query(&statement, &parameters)
        .await
        .map_err(db_error)?;
    Ok(rows.iter().map(pilot).collect())
}

pub(super) async fn of_flights(
    client: &Client,
    flight_ids: &[String],
    viewer: Option<&str>,
) -> Result<Vec<FlightSocial>, StoreError> {
    let statement = client
        .prepare_cached(
            "SELECT asked.id,
                    (SELECT count(*) FROM flight_cheers c WHERE c.flight_id = asked.id) AS cheers,
                    (SELECT count(*) FROM flight_comments m WHERE m.flight_id = asked.id)
                        AS comments,
                    EXISTS (SELECT 1 FROM flight_cheers c
                            WHERE c.flight_id = asked.id AND c.account_id = $2) AS cheered,
                    EXISTS (SELECT 1 FROM flight_comments_closed x
                            WHERE x.flight_id = asked.id) AS closed
             FROM unnest($1::text[]) WITH ORDINALITY AS asked (id, place)
             ORDER BY asked.place",
        )
        .await
        .map_err(db_error)?;
    let rows = client
        .query(&statement, &[&flight_ids, &viewer])
        .await
        .map_err(db_error)?;
    Ok(rows
        .iter()
        .map(|row| FlightSocial {
            flight_id: row.get("id"),
            cheers: as_count(row.get("cheers")),
            comments: as_count(row.get("comments")),
            cheered: row.get("cheered"),
            comments_closed: row.get("closed"),
        })
        .collect())
}

const COMMENT_COLUMNS: &str = "m.id, m.flight_id, m.author_id, a.handle, a.display_name, m.body, \
                               m.created_at";

fn comment_from(row: &Row) -> Comment {
    Comment {
        id: row.get("id"),
        flight_id: row.get("flight_id"),
        author_id: row.get("author_id"),
        author_handle: row.get("handle"),
        author_name: row.get("display_name"),
        text: row.get("body"),
        created_at: row.get("created_at"),
    }
}

pub(super) async fn add_comment(
    client: &mut Client,
    new: &NewComment,
) -> Result<Commenting, StoreError> {
    if new.text.contains('\0') {
        return Err(StoreError::Invalid(
            "a comment with a zero byte in it".to_owned(),
        ));
    }
    let tx = client.transaction().await.map_err(db_error)?;
    // The flight's row, held: a comment and the closing of the comments
    // come one after the other.
    let flight = tx
        .query_opt(
            "SELECT 1 FROM flights WHERE id = $1 AND visibility = 'public' FOR SHARE",
            &[&new.flight_id],
        )
        .await
        .map_err(db_error)?;
    let author = tx
        .query_opt("SELECT 1 FROM accounts WHERE id = $1", &[&new.author_id])
        .await
        .map_err(db_error)?;
    if flight.is_none() || author.is_none() {
        return Ok(Commenting::NoSuchFlight);
    }
    let closed = tx
        .query_opt(
            "SELECT 1 FROM flight_comments_closed WHERE flight_id = $1",
            &[&new.flight_id],
        )
        .await
        .map_err(db_error)?;
    if closed.is_some() {
        return Ok(Commenting::Closed);
    }
    tx.execute(
        "INSERT INTO flight_comments (id, flight_id, author_id, body, created_at)
         VALUES ($1, $2, $3, $4, $5)",
        &[
            &new.id,
            &new.flight_id,
            &new.author_id,
            &new.text,
            &new.created_at,
        ],
    )
    .await
    .map_err(db_error)?;
    let row = tx
        .query_one(
            &format!(
                "SELECT {COMMENT_COLUMNS} FROM flight_comments m
                 JOIN accounts a ON a.id = m.author_id WHERE m.id = $1"
            ),
            &[&new.id],
        )
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(Commenting::Added(comment_from(&row)))
}

pub(super) async fn comments(
    client: &Client,
    flight_id: &str,
    after: Option<&SocialKey>,
    limit: usize,
) -> Result<Vec<Comment>, StoreError> {
    let page = match after {
        Some(_) => "AND (m.created_at, m.id) > ($3, $4)",
        None => "",
    };
    let query = format!(
        "SELECT {COMMENT_COLUMNS} FROM flight_comments m JOIN accounts a ON a.id = m.author_id
         WHERE m.flight_id = $1 {page}
         ORDER BY m.created_at, m.id
         LIMIT $2"
    );
    let statement = client.prepare_cached(&query).await.map_err(db_error)?;
    let limit = as_limit(limit);
    let mut parameters: Vec<&(dyn ToSql + Sync)> = vec![&flight_id, &limit];
    if let Some((at, id)) = after {
        parameters.extend([at as &(dyn ToSql + Sync), id]);
    }
    let rows = client
        .query(&statement, &parameters)
        .await
        .map_err(db_error)?;
    Ok(rows.iter().map(comment_from).collect())
}

pub(super) async fn comment(client: &Client, id: &str) -> Result<Option<Comment>, StoreError> {
    let row = client
        .query_opt(
            &format!(
                "SELECT {COMMENT_COLUMNS} FROM flight_comments m
                 JOIN accounts a ON a.id = m.author_id WHERE m.id = $1"
            ),
            &[&id],
        )
        .await
        .map_err(db_error)?;
    Ok(row.as_ref().map(comment_from))
}

pub(super) async fn remove_comment(
    client: &Client,
    id: &str,
) -> Result<Option<Comment>, StoreError> {
    let row = client
        .query_opt(
            &format!(
                "WITH m AS (DELETE FROM flight_comments WHERE id = $1 RETURNING *)
                 SELECT {COMMENT_COLUMNS} FROM m JOIN accounts a ON a.id = m.author_id"
            ),
            &[&id],
        )
        .await
        .map_err(db_error)?;
    Ok(row.as_ref().map(comment_from))
}

pub(super) async fn close_comments(
    client: &Client,
    account_id: &str,
    flight_id: &str,
    closed: bool,
    now: Timestamp,
) -> Result<bool, StoreError> {
    let own = client
        .query_opt(
            "SELECT 1 FROM flights WHERE id = $1 AND account_id = $2",
            &[&flight_id, &account_id],
        )
        .await
        .map_err(db_error)?;
    if own.is_none() {
        return Ok(false);
    }
    if closed {
        client
            .execute(
                "INSERT INTO flight_comments_closed (flight_id, closed_at)
                 SELECT id, $2 FROM flights WHERE id = $1
                 ON CONFLICT (flight_id) DO NOTHING",
                &[&flight_id, &now],
            )
            .await
            .map_err(db_error)?;
    } else {
        client
            .execute(
                "DELETE FROM flight_comments_closed WHERE flight_id = $1",
                &[&flight_id],
            )
            .await
            .map_err(db_error)?;
    }
    Ok(true)
}

pub(super) async fn given(client: &Client, account_id: &str) -> Result<SocialGiven, StoreError> {
    let cheers = client
        .query(
            "SELECT flight_id, cheered_at FROM flight_cheers
             WHERE account_id = $1 ORDER BY cheered_at, flight_id",
            &[&account_id],
        )
        .await
        .map_err(db_error)?
        .iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect();
    let comments = client
        .query(
            &format!(
                "SELECT {COMMENT_COLUMNS} FROM flight_comments m
                 JOIN accounts a ON a.id = m.author_id
                 WHERE m.author_id = $1 ORDER BY m.created_at, m.id"
            ),
            &[&account_id],
        )
        .await
        .map_err(db_error)?
        .iter()
        .map(comment_from)
        .collect();
    Ok(SocialGiven { cheers, comments })
}

pub(super) async fn add_notice(
    client: &mut Client,
    account_id: &str,
    actor_id: &str,
    at: Timestamp,
    record: &str,
) -> Result<bool, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let there: i64 = tx
        .query_one(
            "SELECT count(*) FROM accounts WHERE id IN ($1, $2)",
            &[&account_id, &actor_id],
        )
        .await
        .map_err(db_error)?
        .get(0);
    let wanted = if account_id == actor_id { 1 } else { 2 };
    if there != wanted {
        return Ok(false);
    }
    let latest: i64 = tx
        .query_one(
            "INSERT INTO notice_numbers (account_id, latest) VALUES ($1, 0)
             ON CONFLICT (account_id) DO UPDATE SET latest = notice_numbers.latest
             RETURNING latest",
            &[&account_id],
        )
        .await
        .map_err(db_error)?
        .get(0);
    let number = next_change(latest, at);
    tx.execute(
        "UPDATE notice_numbers SET latest = $2 WHERE account_id = $1",
        &[&account_id, &number],
    )
    .await
    .map_err(db_error)?;
    tx.execute(
        "INSERT INTO notices (account_id, number, actor_id, at, record)
         VALUES ($1, $2, $3, $4, $5)",
        &[&account_id, &number, &actor_id, &at, &record],
    )
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(true)
}

pub(super) async fn notices(
    client: &Client,
    account_id: &str,
    since: i64,
    limit: usize,
) -> Result<Notices, StoreError> {
    let latest: i64 = client
        .query_opt(
            "SELECT latest FROM notice_numbers WHERE account_id = $1",
            &[&account_id],
        )
        .await
        .map_err(db_error)?
        .map_or(0, |row| row.get(0));
    let asked = i64::try_from(limit).unwrap_or(i64::MAX - 1) + 1;
    let rows = client
        .query(
            "SELECT number, at, record FROM notices
             WHERE account_id = $1 AND number > $2 ORDER BY number LIMIT $3",
            &[&account_id, &since, &asked],
        )
        .await
        .map_err(db_error)?;
    let mut notices: Vec<Notice> = rows
        .iter()
        .map(|row| Notice {
            number: row.get("number"),
            at: row.get("at"),
            record: row.get("record"),
        })
        .collect();
    let more = notices.len() > limit;
    notices.truncate(limit);
    Ok(Notices {
        notices,
        more,
        latest,
    })
}

pub(super) async fn prune_notices(client: &Client, before: Timestamp) -> Result<u64, StoreError> {
    client
        .execute("DELETE FROM notices WHERE at < $1", &[&before])
        .await
        .map_err(db_error)
}
