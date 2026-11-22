//! What every store must do for the authorization server: registered
//! programs, the codes a pilot's agreement is turned into, and the
//! agreements themselves, which are tokens.

use chrono::Duration;
use venti_domain::Timestamp;
use venti_store::{
    AccessToken, Account, AuthorizationCode, AuthorizationRequest, NewGrant, OauthClient, Redeemed,
    Refreshed, RequestState, Rotation, Store, TokenClient,
};

use super::accounts::{create, id, secret};
use super::at;

const CONNECTOR: &str = "https://venti.example/mcp";

fn program(name: &str, registered: Timestamp) -> OauthClient {
    OauthClient {
        id: id(name),
        name: format!("Program {name}"),
        redirect_uris: vec![format!("https://{name}.example/callback")],
        created_at: registered,
        last_used_at: None,
    }
}

async fn register(store: &impl Store, name: &str, registered: Timestamp) -> OauthClient {
    let client = program(name, registered);
    assert!(store.create_oauth_client(&client, 100).await.unwrap());
    client
}

fn code_for(client: &OauthClient, account: &Account, made: Timestamp) -> AuthorizationCode {
    AuthorizationCode {
        client_id: client.id.clone(),
        account_id: account.id.clone(),
        redirect_uri: client.redirect_uris[0].clone(),
        scopes: vec!["flights:read".to_owned()],
        resource: CONNECTOR.to_owned(),
        challenge: "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM".to_owned(),
        created_at: made,
        expires_at: made + Duration::minutes(2),
    }
}

/// The agreement `name` of an account with a program, made at `made`: its
/// access token is good for an hour and the agreement for ninety days.
fn grant(name: &str, client: &OauthClient, account: &Account, made: Timestamp) -> AccessToken {
    AccessToken {
        id: id(name),
        account_id: account.id.clone(),
        name: client.name.clone(),
        scopes: vec!["flights:read".to_owned()],
        created_at: made,
        last_used_on: None,
        expires_at: Some(made + Duration::days(90)),
        client: Some(TokenClient {
            client_id: client.id.clone(),
            resource: CONNECTOR.to_owned(),
            access_until: made + Duration::hours(1),
        }),
    }
}

/// Stores a code and exchanges it for the agreement `name`, whose access
/// token hashes to `access` and whose refresh token to `refresh`.
async fn agree(
    store: &impl Store,
    name: &str,
    client: &OauthClient,
    account: &Account,
    made: Timestamp,
    (code, access, refresh): (u8, u8, u8),
) -> AccessToken {
    let record = code_for(client, account, made);
    assert!(
        store
            .create_authorization_code(&secret(code), &record)
            .await
            .unwrap()
    );
    let token = grant(name, client, account, made);
    let new = NewGrant {
        token: token.clone(),
        access: secret(access),
        refresh: secret(refresh),
    };
    let redeemed = store
        .redeem_authorization_code(&secret(code), &new, made, 20)
        .await
        .unwrap();
    assert_eq!(redeemed, Redeemed::Granted);
    token
}

fn rotation(access: u8, refresh: u8, now: Timestamp) -> Rotation {
    Rotation {
        access: secret(access),
        access_until: now + Duration::hours(1),
        refresh: secret(refresh),
        expires_at: now + Duration::days(90),
    }
}

async fn token_ids(store: &impl Store, account: &Account) -> Vec<String> {
    let tokens = store.access_tokens(&account.id).await.unwrap();
    tokens
        .iter()
        .map(|token| token.id.trim_start_matches('0').to_owned())
        .collect()
}

pub(crate) async fn the_oldest_unused_program_makes_room_for_a_new_one(store: &impl Store) {
    let dario = create(store, "dario").await;
    let first = register(store, "first", at(9, 0)).await;
    let second = register(store, "second", at(9, 1)).await;
    assert_eq!(
        store.oauth_client(&first.id).await.unwrap(),
        Some(first.clone())
    );
    assert_eq!(store.oauth_client(&id("nobody")).await.unwrap(), None);
    assert_eq!(store.oauth_client("a\0b").await.unwrap(), None);
    // The same identifier is not registered twice.
    assert!(!store.create_oauth_client(&first, 100).await.unwrap());

    // A pilot agrees to the first: it is in use from then on.
    agree(store, "grant", &first, &dario, at(10, 0), (1, 2, 3)).await;
    let used = store.oauth_client(&first.id).await.unwrap().unwrap();
    assert_eq!(used.last_used_at, Some(at(10, 0)));

    // The table is full with two. A third takes the place of the one that
    // registered longest ago among those nobody ever agreed to: the
    // second, not the first, which is older and in use.
    let third = program("third", at(11, 0));
    assert!(store.create_oauth_client(&third, 2).await.unwrap());
    assert_eq!(store.oauth_client(&second.id).await.unwrap(), None);
    assert!(store.oauth_client(&first.id).await.unwrap().is_some());
    assert_eq!(token_ids(store, &dario).await, ["grant"]);

    // Once every program in the full table is in use, no further one is
    // taken, and none is removed for it.
    agree(store, "other", &third, &dario, at(11, 30), (4, 5, 6)).await;
    let fourth = program("fourth", at(12, 0));
    assert!(!store.create_oauth_client(&fourth, 2).await.unwrap());
    assert!(store.oauth_client(&first.id).await.unwrap().is_some());
    assert!(store.oauth_client(&third.id).await.unwrap().is_some());
}

fn request_of(client: &OauthClient, made: Timestamp) -> AuthorizationRequest {
    AuthorizationRequest {
        client_id: client.id.clone(),
        redirect_uri: client.redirect_uris[0].clone(),
        scopes: vec!["flights:read".to_owned()],
        resource: CONNECTOR.to_owned(),
        challenge: "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM".to_owned(),
        state: Some("state-7f3a/+=&?".to_owned()),
        created_at: made,
        expires_at: made + Duration::minutes(15),
    }
}

pub(crate) async fn a_request_for_agreement_is_answered_once(store: &impl Store) {
    let client = register(store, "claude", at(9, 0)).await;
    let asked = request_of(&client, at(10, 0));
    let keep = |hash: u8, record: AuthorizationRequest, now: Timestamp, most: usize| async move {
        store
            .create_authorization_request(&secret(hash), &record, now, most)
            .await
            .unwrap()
    };
    assert!(keep(1, asked.clone(), at(10, 0), 10).await);
    // The same identifier is not kept twice, and a request of a program
    // that is not there is not kept at all.
    assert!(!keep(1, asked.clone(), at(10, 0), 10).await);
    let of_no_program = AuthorizationRequest {
        client_id: id("nothing"),
        ..asked.clone()
    };
    assert!(!keep(2, of_no_program, at(10, 0), 10).await);
    // One without a value of the program's own, and one whose value the
    // database could not hold, which is not kept and breaks nothing.
    let bare = AuthorizationRequest {
        state: None,
        ..asked.clone()
    };
    assert!(keep(3, bare.clone(), at(10, 1), 10).await);
    let zero = AuthorizationRequest {
        state: Some("a\0b".to_owned()),
        ..asked.clone()
    };
    let kept = keep(4, zero.clone(), at(10, 1), 10).await;
    let found = store
        .authorization_request(&secret(4), at(10, 1))
        .await
        .unwrap();
    assert_eq!(found == RequestState::Waiting(zero), kept);

    // Looking changes nothing, however often.
    let look = |hash: u8, now: Timestamp| async move {
        store
            .authorization_request(&secret(hash), now)
            .await
            .unwrap()
    };
    assert_eq!(
        look(1, at(10, 5)).await,
        RequestState::Waiting(asked.clone())
    );
    assert_eq!(
        look(1, at(10, 5)).await,
        RequestState::Waiting(asked.clone())
    );
    assert_eq!(
        look(3, at(10, 5)).await,
        RequestState::Waiting(bare.clone())
    );
    assert_eq!(look(9, at(10, 5)).await, RequestState::Unknown);

    // It is answered once: the first answer gets the request, and every
    // later one, and every look, is told that it was answered.
    let answer = |hash: u8, now: Timestamp| async move {
        store
            .answer_authorization_request(&secret(hash), now)
            .await
            .unwrap()
    };
    assert_eq!(answer(9, at(10, 5)).await, RequestState::Unknown);
    assert_eq!(
        answer(1, at(10, 5)).await,
        RequestState::Waiting(asked.clone())
    );
    assert_eq!(answer(1, at(10, 5)).await, RequestState::Answered);
    assert_eq!(look(1, at(10, 6)).await, RequestState::Answered);

    // At its end it is gone, answered or not, also before anything
    // removed it.
    assert_eq!(look(1, at(10, 15)).await, RequestState::Unknown);
    assert_eq!(answer(1, at(10, 15)).await, RequestState::Unknown);
    assert_eq!(look(3, at(10, 14)).await, RequestState::Waiting(bare));
    assert_eq!(look(3, at(10, 15)).await, RequestState::Unknown);
    assert_eq!(answer(3, at(10, 15)).await, RequestState::Unknown);

    // Only so many wait at once; those that ended make room.
    let later = request_of(&client, at(11, 0));
    assert!(keep(5, later.clone(), at(11, 0), 2).await);
    assert!(keep(6, later.clone(), at(11, 0), 2).await);
    assert!(!keep(7, later.clone(), at(11, 0), 2).await);
    assert_eq!(look(7, at(11, 0)).await, RequestState::Unknown);
    let much_later = request_of(&client, at(11, 15));
    assert!(keep(7, much_later.clone(), at(11, 15), 2).await);

    // The maintenance removes what ended, and a program that goes takes
    // its requests along.
    store.prune_oauth(at(11, 20), at(8, 0)).await.unwrap();
    assert_eq!(look(7, at(11, 20)).await, RequestState::Waiting(much_later));
    store.prune_oauth(at(11, 40), at(11, 39)).await.unwrap();
    assert_eq!(store.oauth_client(&client.id).await.unwrap(), None);
    assert_eq!(look(7, at(11, 20)).await, RequestState::Unknown);
}

pub(crate) async fn a_code_is_exchanged_once_and_no_second_time(store: &impl Store) {
    let dario = create(store, "dario").await;
    let client = register(store, "claude", at(9, 0)).await;
    let record = code_for(&client, &dario, at(10, 0));
    assert!(
        store
            .create_authorization_code(&secret(1), &record)
            .await
            .unwrap()
    );
    // A code of a program or an account that is not there is not stored.
    let of_nobody = AuthorizationCode {
        account_id: id("nobody"),
        ..record.clone()
    };
    assert!(
        !store
            .create_authorization_code(&secret(7), &of_nobody)
            .await
            .unwrap()
    );
    let of_no_program = AuthorizationCode {
        client_id: id("nothing"),
        ..record.clone()
    };
    assert!(
        !store
            .create_authorization_code(&secret(8), &of_no_program)
            .await
            .unwrap()
    );

    // Looking finds the code as it was stored, and changes nothing.
    for _ in 0..2 {
        let stored = store.authorization_code(&secret(1), at(10, 1)).await;
        let stored = stored.unwrap().unwrap();
        assert_eq!(stored.code, record);
        assert!(!stored.used);
    }
    assert_eq!(
        store
            .authorization_code(&secret(9), at(10, 1))
            .await
            .unwrap(),
        None
    );

    // A code is good until its end and not at it.
    let token = grant("grant", &client, &dario, at(10, 1));
    let new = NewGrant {
        token: token.clone(),
        access: secret(2),
        refresh: secret(3),
    };
    let late = store
        .redeem_authorization_code(&secret(1), &new, at(10, 2), 20)
        .await;
    assert_eq!(late.unwrap(), Redeemed::Unknown);
    assert_eq!(
        store
            .authorization_code(&secret(1), at(10, 2))
            .await
            .unwrap(),
        None
    );
    let unknown = store
        .redeem_authorization_code(&secret(9), &new, at(10, 1), 20)
        .await;
    assert_eq!(unknown.unwrap(), Redeemed::Unknown);
    assert!(token_ids(store, &dario).await.is_empty());

    // The exchange stores the agreement: a token of the account, found by
    // its access token until that ends.
    let granted = store
        .redeem_authorization_code(&secret(1), &new, at(10, 1), 20)
        .await;
    assert_eq!(granted.unwrap(), Redeemed::Granted);
    assert_eq!(
        store.access_tokens(&dario.id).await.unwrap(),
        std::slice::from_ref(&token)
    );
    let stored = store.authorization_code(&secret(1), at(10, 1)).await;
    assert!(stored.unwrap().unwrap().used);
    let (found, account) = store
        .use_access_token(&secret(2), at(10, 30))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(account, dario);
    assert_eq!(found.client, token.client);
    let ended = store.use_access_token(&secret(2), at(11, 1)).await;
    assert_eq!(ended.unwrap(), None, "an access token lasts an hour here");
    // A refresh token is no access token.
    let refresh = store.use_access_token(&secret(3), at(10, 30)).await;
    assert_eq!(refresh.unwrap(), None);

    // A second exchange of the same code gets nothing, and what came of
    // the first is withdrawn: the code was seen by somebody else.
    let again = NewGrant {
        token: grant("second", &client, &dario, at(10, 1)),
        access: secret(4),
        refresh: secret(5),
    };
    let spent = store
        .redeem_authorization_code(&secret(1), &again, at(10, 1), 20)
        .await;
    assert_eq!(spent.unwrap(), Redeemed::Spent);
    assert!(token_ids(store, &dario).await.is_empty());
    for gone in [2, 4] {
        let used = store.use_access_token(&secret(gone), at(10, 30)).await;
        assert_eq!(used.unwrap(), None);
    }
    let refreshed = store
        .refresh_grant(
            &secret(3),
            &client.id,
            &rotation(6, 7, at(10, 30)),
            at(10, 30),
        )
        .await;
    assert_eq!(refreshed.unwrap(), Refreshed::Unknown);
}

pub(crate) async fn a_refresh_token_is_used_once_and_no_second_time(store: &impl Store) {
    let dario = create(store, "dario").await;
    let client = register(store, "claude", at(9, 0)).await;
    let other = register(store, "other", at(9, 0)).await;
    let token = agree(store, "grant", &client, &dario, at(10, 0), (1, 2, 3)).await;

    // Another program cannot use this one's refresh token, and trying
    // changes nothing; neither can anyone use what is no refresh token.
    let now = at(10, 50);
    let by_another = store
        .refresh_grant(&secret(3), &other.id, &rotation(4, 5, now), now)
        .await;
    assert_eq!(by_another.unwrap(), Refreshed::Unknown);
    let with_the_access_token = store
        .refresh_grant(&secret(2), &client.id, &rotation(4, 5, now), now)
        .await;
    assert_eq!(with_the_access_token.unwrap(), Refreshed::Unknown);
    let with_a_zero_byte = store
        .refresh_grant(&secret(3), "a\0b", &rotation(4, 5, now), now)
        .await;
    assert_eq!(with_a_zero_byte.unwrap(), Refreshed::Unknown);
    assert!(
        store
            .use_access_token(&secret(2), now)
            .await
            .unwrap()
            .is_some()
    );

    // The refresh replaces both secrets and moves both ends on; the token
    // keeps its identifier, so what is counted for it goes on counting.
    let next = rotation(4, 5, now);
    let rotated = store
        .refresh_grant(&secret(3), &client.id, &next, now)
        .await;
    let Refreshed::Rotated(rotated) = rotated.unwrap() else {
        panic!("the refresh token was good");
    };
    let (fresh, account) = *rotated;
    assert_eq!(account, dario);
    assert_eq!(fresh.id, token.id);
    assert_eq!(fresh.expires_at, Some(next.expires_at));
    assert_eq!(
        fresh.client.as_ref().map(|client| client.access_until),
        Some(next.access_until)
    );
    assert_eq!(
        store.use_access_token(&secret(2), now).await.unwrap(),
        None,
        "the old access token is worth nothing"
    );
    let later = at(11, 30);
    let used = store.use_access_token(&secret(4), later).await.unwrap();
    assert_eq!(used.map(|(token, _)| token.id), Some(token.id.clone()));
    assert_eq!(token_ids(store, &dario).await, ["grant"]);
    let program = store.oauth_client(&client.id).await.unwrap().unwrap();
    assert_eq!(program.last_used_at, Some(now));

    // The old refresh token turns up again: one of its two users is not
    // the program, and the agreement ends for both.
    let reused = store
        .refresh_grant(&secret(3), &client.id, &rotation(6, 7, later), later)
        .await;
    assert_eq!(reused.unwrap(), Refreshed::Reused);
    assert!(token_ids(store, &dario).await.is_empty());
    assert_eq!(
        store.use_access_token(&secret(4), later).await.unwrap(),
        None
    );
    assert_eq!(
        store.use_access_token(&secret(6), later).await.unwrap(),
        None
    );
    let with_the_newest = store
        .refresh_grant(&secret(5), &client.id, &rotation(8, 9, later), later)
        .await;
    assert_eq!(with_the_newest.unwrap(), Refreshed::Unknown);
}

pub(crate) async fn a_refresh_token_is_good_until_its_end(store: &impl Store) {
    let dario = create(store, "dario").await;
    let client = register(store, "claude", at(9, 0)).await;
    agree(store, "grant", &client, &dario, at(10, 0), (1, 2, 3)).await;
    let end = at(10, 0) + Duration::days(90);
    let at_the_end = store
        .refresh_grant(&secret(3), &client.id, &rotation(4, 5, end), end)
        .await;
    assert_eq!(at_the_end.unwrap(), Refreshed::Unknown);
    let before = end - Duration::seconds(1);
    let just_before = store
        .refresh_grant(&secret(3), &client.id, &rotation(4, 5, before), before)
        .await;
    assert!(matches!(just_before.unwrap(), Refreshed::Rotated(_)));
}

pub(crate) async fn an_agreement_is_listed_and_withdrawn_as_a_token(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let claude = register(store, "claude", at(9, 0)).await;
    let other = register(store, "other", at(9, 0)).await;
    let by_hand = AccessToken {
        id: id("logger"),
        account_id: dario.id.clone(),
        name: "My logger".to_owned(),
        scopes: vec!["flights:write".to_owned()],
        created_at: at(9, 30),
        last_used_on: None,
        expires_at: None,
        client: None,
    };
    assert!(
        store
            .create_access_token(&by_hand, &secret(20), 20)
            .await
            .unwrap()
    );
    // A token that names a program is not made by hand.
    let forged = grant("forged", &claude, &dario, at(9, 31));
    assert!(
        !store
            .create_access_token(&forged, &secret(21), 20)
            .await
            .unwrap()
    );

    let first = agree(store, "first", &claude, &dario, at(10, 0), (1, 2, 3)).await;
    agree(store, "annas", &claude, &anna, at(10, 0), (11, 12, 13)).await;
    assert_eq!(
        store.access_tokens(&dario.id).await.unwrap(),
        [first.clone(), by_hand.clone()]
    );

    // Agreeing to the same program again replaces the agreement; its
    // secrets are worth nothing from then on. Anna's is not touched.
    agree(store, "again", &claude, &dario, at(10, 10), (4, 5, 6)).await;
    assert_eq!(token_ids(store, &dario).await, ["again", "logger"]);
    assert_eq!(
        store
            .use_access_token(&secret(2), at(10, 20))
            .await
            .unwrap(),
        None
    );
    let old_refresh = store
        .refresh_grant(
            &secret(3),
            &claude.id,
            &rotation(7, 8, at(10, 20)),
            at(10, 20),
        )
        .await;
    assert_eq!(old_refresh.unwrap(), Refreshed::Unknown);
    assert_eq!(token_ids(store, &anna).await, ["annas"]);

    // An account that holds as many tokens as it may gets no further
    // agreement, and the code stays good for the pilot to try again after
    // withdrawing one. Agreeing again with a program it has is no further
    // one.
    let record = code_for(&other, &dario, at(10, 30));
    assert!(
        store
            .create_authorization_code(&secret(30), &record)
            .await
            .unwrap()
    );
    let new = NewGrant {
        token: grant("third", &other, &dario, at(10, 30)),
        access: secret(31),
        refresh: secret(32),
    };
    let full = store
        .redeem_authorization_code(&secret(30), &new, at(10, 30), 2)
        .await;
    assert_eq!(full.unwrap(), Redeemed::Full);
    assert_eq!(token_ids(store, &dario).await, ["again", "logger"]);
    let record = code_for(&claude, &dario, at(10, 31));
    assert!(
        store
            .create_authorization_code(&secret(33), &record)
            .await
            .unwrap()
    );
    let replacing = NewGrant {
        token: grant("fourth", &claude, &dario, at(10, 31)),
        access: secret(34),
        refresh: secret(35),
    };
    let replaced = store
        .redeem_authorization_code(&secret(33), &replacing, at(10, 31), 2)
        .await;
    assert_eq!(replaced.unwrap(), Redeemed::Granted);
    assert_eq!(token_ids(store, &dario).await, ["fourth", "logger"]);

    // The pilot withdraws it as any token: its access token and its
    // refresh token are worth nothing at once.
    assert!(
        !store
            .delete_access_token(&anna.id, &id("fourth"))
            .await
            .unwrap()
    );
    assert!(
        store
            .delete_access_token(&dario.id, &id("fourth"))
            .await
            .unwrap()
    );
    assert_eq!(
        store
            .use_access_token(&secret(34), at(10, 40))
            .await
            .unwrap(),
        None
    );
    let after = store
        .refresh_grant(
            &secret(35),
            &claude.id,
            &rotation(36, 37, at(10, 40)),
            at(10, 40),
        )
        .await;
    assert_eq!(after.unwrap(), Refreshed::Unknown);

    // The program itself gives an agreement up, with either secret; no
    // other program can, and a token made by hand is not given up so.
    assert!(!store.revoke_grant(&secret(12), &other.id).await.unwrap());
    assert!(!store.revoke_grant(&secret(12), "a\0b").await.unwrap());
    assert!(!store.revoke_grant(&secret(20), &claude.id).await.unwrap());
    assert!(!store.revoke_grant(&secret(99), &claude.id).await.unwrap());
    assert!(store.revoke_grant(&secret(13), &claude.id).await.unwrap());
    assert!(token_ids(store, &anna).await.is_empty());
    assert_eq!(
        store
            .use_access_token(&secret(12), at(10, 40))
            .await
            .unwrap(),
        None
    );
    assert_eq!(token_ids(store, &dario).await, ["logger"]);
}

pub(crate) async fn an_agreement_and_its_codes_go_with_the_account(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let client = register(store, "claude", at(9, 0)).await;
    agree(store, "darios", &client, &dario, at(10, 0), (1, 2, 3)).await;
    agree(store, "annas", &client, &anna, at(10, 0), (11, 12, 13)).await;
    let waiting = code_for(&client, &dario, at(10, 5));
    assert!(
        store
            .create_authorization_code(&secret(4), &waiting)
            .await
            .unwrap()
    );

    store.delete_account(&dario.id).await.unwrap();

    let now = at(10, 6);
    assert_eq!(store.use_access_token(&secret(2), now).await.unwrap(), None);
    assert_eq!(
        store.authorization_code(&secret(4), now).await.unwrap(),
        None
    );
    let refreshed = store
        .refresh_grant(&secret(3), &client.id, &rotation(5, 6, now), now)
        .await;
    assert_eq!(refreshed.unwrap(), Refreshed::Unknown);
    // The other pilot's agreement with the same program stands.
    assert!(
        store
            .use_access_token(&secret(12), now)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(token_ids(store, &anna).await, ["annas"]);
}

pub(crate) async fn what_the_authorization_server_no_longer_needs_is_removed(store: &impl Store) {
    let dario = create(store, "dario").await;
    let never_used = register(store, "never", at(9, 0)).await;
    let in_use = register(store, "inuse", at(9, 0)).await;
    let given_up = register(store, "givenup", at(9, 0)).await;
    let recent = register(store, "recent", at(11, 0)).await;
    let by_hand = AccessToken {
        id: id("logger"),
        account_id: dario.id.clone(),
        name: "My logger".to_owned(),
        scopes: vec!["flights:write".to_owned()],
        created_at: at(9, 30),
        last_used_on: None,
        expires_at: Some(at(9, 45)),
        client: None,
    };
    assert!(
        store
            .create_access_token(&by_hand, &secret(20), 20)
            .await
            .unwrap()
    );
    agree(store, "kept", &in_use, &dario, at(9, 30), (1, 2, 3)).await;
    agree(store, "dropped", &given_up, &dario, at(9, 30), (4, 5, 6)).await;
    assert!(store.revoke_grant(&secret(5), &given_up.id).await.unwrap());

    // Nothing is due yet.
    assert_eq!(store.prune_oauth(at(9, 31), at(8, 0)).await.unwrap(), 0);
    assert!(store.oauth_client(&never_used.id).await.unwrap().is_some());

    // Programs that hold no agreement and showed no sign of life since
    // ten o'clock go: the one nobody agreed to and the one that gave its
    // agreement up. The one in use stays however old, and so does the one
    // that registered since.
    assert_eq!(store.prune_oauth(at(12, 0), at(10, 0)).await.unwrap(), 2);
    assert_eq!(store.oauth_client(&never_used.id).await.unwrap(), None);
    assert_eq!(store.oauth_client(&given_up.id).await.unwrap(), None);
    assert!(store.oauth_client(&in_use.id).await.unwrap().is_some());
    assert!(store.oauth_client(&recent.id).await.unwrap().is_some());
    assert_eq!(token_ids(store, &dario).await, ["logger", "kept"]);

    // An agreement whose end has come goes by itself, and then its
    // program; a token made by hand whose end has come stays listed until
    // its pilot withdraws it.
    let end = at(9, 30) + Duration::days(90);
    assert_eq!(store.prune_oauth(end, end).await.unwrap(), 3);
    assert_eq!(token_ids(store, &dario).await, ["logger"]);
    assert_eq!(store.oauth_client(&in_use.id).await.unwrap(), None);
    assert_eq!(store.oauth_client(&recent.id).await.unwrap(), None);
}
