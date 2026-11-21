//! Tests of the sites a pilot keeps as their own on the account, through
//! the real router: keeping, forgetting, what changed since, whose list
//! it is, and what goes with the account.

use axum::Router;
use axum::http::{Method, StatusCode};
use chrono::Duration;
use venti_api_types::v1::{
    ACCOUNT_PATH, DeleteAccountRequest, ErrorCode, KEPT_SITE_CHANGES_PATH, KeepSiteRequest,
    KeptSiteChangesRequest, KeptSiteChangesResponse, KeptSiteResponse, MAX_KEPT_SITES, Password,
    SitesResponse, kept_site_path,
};
use venti_domain::{SiteId, Timestamp};
use venti_server::clock::Clock;
use venti_server::dev::DEMO_TOKEN;
use venti_sources::testing::assert_golden;

mod common;

use common::{Answer, PASSWORD, TestClock, ask, get, golden, recorded, server, settings, sign_up};

// --- Helpers ----------------------------------------------------------------

const TEGERNSEE: &str = "/v1/sites?bbox=11.7,47.6,11.95,47.75";

/// The sites the development mode serves around the Tegernsee, each as a
/// device would send it to keep: the record, its source's credit and the
/// date of the source's data.
async fn seen(router: &Router) -> Vec<KeepSiteRequest> {
    let answer: SitesResponse = get(router, TEGERNSEE).await.json();
    let mut seen = Vec::new();
    for layer in &answer.layers {
        let attribution = answer
            .attributions
            .iter()
            .find(|credit| credit.source == layer.source);
        for site in &layer.sites {
            seen.push(KeepSiteRequest {
                site: site.clone(),
                standing: None,
                attribution: attribution.cloned(),
                as_of: Some(layer.as_of),
                kept_at: None,
            });
        }
    }
    assert!(seen.len() >= 3, "{}", seen.len());
    seen
}

async fn try_keep(router: &Router, token: &str, id: &SiteId, request: &KeepSiteRequest) -> Answer {
    ask(Method::PUT, &kept_site_path(id))
        .token(token)
        .json(request)
        .send(router)
        .await
}

async fn keep(router: &Router, token: &str, request: &KeepSiteRequest) -> KeptSiteResponse {
    let answer = try_keep(router, token, &request.site.id, request).await;
    answer.is_private();
    answer.ok(StatusCode::OK)
}

async fn forget(router: &Router, token: &str, id: &SiteId) {
    let answer = ask(Method::DELETE, &kept_site_path(id))
        .token(token)
        .send(router)
        .await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
}

async fn changes(router: &Router, token: &str, since: Option<&str>) -> KeptSiteChangesResponse {
    let request = KeptSiteChangesRequest {
        since: since.map(str::to_owned),
        limit: Some(100),
    };
    let answer = ask(Method::GET, &request.path_and_query())
        .token(token)
        .send(router)
        .await;
    answer.is_private();
    answer.ok(StatusCode::OK)
}

fn ids(changes: &KeptSiteChangesResponse) -> Vec<&SiteId> {
    changes.kept.iter().map(|kept| &kept.site.id).collect()
}

// --- Behaviours -------------------------------------------------------------

#[tokio::test]
async fn a_pilot_keeps_a_site_and_finds_it_from_another_device() {
    let clock = TestClock::new();
    let (router, _) = server(settings(), &clock);
    let phone = sign_up(&router, "dario").await.token;
    let sites = seen(&recorded().await).await;
    let (first, second) = (&sites[0], &sites[1]);

    let nothing = changes(&router, &phone, None).await;
    assert_eq!((nothing.kept.len(), nothing.forgotten.len()), (0, 0));
    assert_eq!(
        (nothing.cursor.as_str(), nothing.more, nothing.reset),
        ("0", false, false)
    );

    // Kept as it was sent, from the moment it arrived.
    let kept = keep(&router, &phone, first).await.kept;
    assert_eq!(kept.site, first.site);
    assert_eq!(kept.attribution, first.attribution);
    assert_eq!(kept.as_of, first.as_of);
    assert_eq!(kept.kept_at, clock.now());

    // Another device of the same account reads it, and keeps one itself.
    let browser = common::try_sign_in(&router, "dario", PASSWORD)
        .await
        .ok::<venti_api_types::v1::SignInResponse>(StatusCode::CREATED)
        .token;
    let there = changes(&router, &browser, None).await;
    assert_eq!(there.kept, std::slice::from_ref(&kept));
    clock.advance(Duration::minutes(1));
    keep(&router, &browser, second).await;

    // The phone asks for what changed since, and gets that one.
    let since = changes(&router, &phone, Some(&there.cursor)).await;
    assert_eq!(ids(&since), [&second.site.id]);
    assert!(!since.reset);

    // Forgotten in the browser, it is named to the phone, once.
    clock.advance(Duration::minutes(1));
    forget(&router, &browser, &first.site.id).await;
    forget(&router, &browser, &first.site.id).await;
    let gone = changes(&router, &phone, Some(&since.cursor)).await;
    assert_eq!(
        (gone.kept.len(), &gone.forgotten),
        (0, &vec![first.site.id.clone()])
    );
    let after = changes(&router, &phone, Some(&gone.cursor)).await;
    assert_eq!((after.kept.len(), after.forgotten.len()), (0, 0));
    assert_eq!(after.cursor, gone.cursor);

    // A cursor this server never gave out starts again, and says so.
    let reset = changes(&router, &phone, Some("99999999999999999")).await;
    assert!(reset.reset);
    assert_eq!(ids(&reset), [&second.site.id]);
    ask(
        Method::GET,
        &format!("{KEPT_SITE_CHANGES_PATH}?since=yesterday"),
    )
    .token(&phone)
    .send(&router)
    .await
    .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidCursor);
}

#[tokio::test]
async fn of_two_records_of_a_site_the_one_kept_later_stays() {
    let clock = TestClock::new();
    let (router, _) = server(settings(), &clock);
    let token = sign_up(&router, "dario").await.token;
    let sites = seen(&recorded().await).await;
    let at = |minutes: i64| -> Timestamp { clock.now() - Duration::minutes(minutes) };

    let noted = KeepSiteRequest {
        kept_at: Some(at(60)),
        ..sites[0].clone()
    };
    assert_eq!(keep(&router, &token, &noted).await.kept.kept_at, at(60));
    let cursor = changes(&router, &token, None).await.cursor;

    // An older note, and no note at all, leave what the account has: the
    // answer is the account's record, and nothing changed.
    let mut other = noted.clone();
    other.site.name = Some("Another name".to_owned());
    for kept_at in [Some(at(120)), None] {
        let offered = KeepSiteRequest {
            kept_at,
            ..other.clone()
        };
        let stays = keep(&router, &token, &offered).await.kept;
        assert_eq!((stays.kept_at, &stays.site), (at(60), &noted.site));
    }
    assert_eq!(changes(&router, &token, Some(&cursor)).await.kept.len(), 0);

    // A newer note replaces it. One from a clock that runs ahead counts
    // as now.
    let newer = KeepSiteRequest {
        kept_at: Some(at(30)),
        ..other.clone()
    };
    let replaced = keep(&router, &token, &newer).await.kept;
    assert_eq!((replaced.kept_at, &replaced.site), (at(30), &other.site));
    let ahead = KeepSiteRequest {
        kept_at: Some(at(-600)),
        ..noted.clone()
    };
    assert_eq!(
        keep(&router, &token, &ahead).await.kept.kept_at,
        clock.now()
    );
}

#[tokio::test]
async fn a_site_to_keep_is_checked_and_an_account_keeps_only_so_many() {
    let clock = TestClock::new();
    let (router, _) = server(settings(), &clock);
    let token = sign_up(&router, "dario").await.token;
    let sites = seen(&recorded().await).await;
    let good = &sites[0];
    let id = &good.site.id;
    let refused = |answer: Answer, part: &str| {
        let message = answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidKeptSite);
        assert!(message.contains(part), "{message}");
    };

    // Under its own identifier, and nothing else.
    refused(
        try_keep(&router, &token, &sites[1].site.id, good).await,
        "site.id",
    );
    let no_site = ask(Method::PUT, "/v1/account/kept-sites/not-a-site")
        .token(&token)
        .json(good)
        .send(&router)
        .await;
    refused(no_site, "identifier");
    // What no site record may hold.
    let mut long = good.clone();
    long.site.name = Some("x".repeat(201));
    refused(try_keep(&router, &token, id, &long).await, "site.name");
    // With its own source's credit, of a length a credit has.
    let mut foreign = good.clone();
    let credit = foreign.attribution.as_mut().unwrap();
    credit.source = venti_domain::SourceId::new("venti").unwrap();
    refused(
        try_keep(&router, &token, id, &foreign).await,
        "attribution.source",
    );
    let mut wordy = good.clone();
    wordy.attribution.as_mut().unwrap().notice = Some("x".repeat(501));
    refused(
        try_keep(&router, &token, id, &wordy).await,
        "attribution.notice",
    );
    // A body of a size no site has.
    let mut huge = good.clone();
    huge.attribution.as_mut().unwrap().text = "x".repeat(40_000);
    try_keep(&router, &token, id, &huge)
        .await
        .error(StatusCode::PAYLOAD_TOO_LARGE, ErrorCode::PayloadTooLarge);
    assert_eq!(changes(&router, &token, None).await.kept.len(), 0);

    // As many as an account keeps, and then no more until one is forgotten.
    let numbered = |n: usize| -> KeepSiteRequest {
        let mut request = good.clone();
        request.site.id = format!("osm-free-flying:node/{n}").parse().unwrap();
        request
    };
    for n in 0..MAX_KEPT_SITES {
        keep(&router, &token, &numbered(n)).await;
    }
    let one_more = numbered(MAX_KEPT_SITES);
    try_keep(&router, &token, &one_more.site.id, &one_more)
        .await
        .error(StatusCode::CONFLICT, ErrorCode::TooManyKeptSites);
    // One that is kept already is kept anew.
    keep(&router, &token, &numbered(7)).await;
    forget(&router, &token, &numbered(0).site.id).await;
    keep(&router, &token, &one_more).await;

    // The whole list is read page by page.
    let first = changes(&router, &token, None).await;
    assert_eq!(
        (first.kept.len() + first.forgotten.len(), first.more),
        (100, true)
    );
    let second = changes(&router, &token, Some(&first.cursor)).await;
    let third = changes(&router, &token, Some(&second.cursor)).await;
    assert_eq!(
        first.kept.len() + second.kept.len() + third.kept.len(),
        MAX_KEPT_SITES
    );
    assert!(!third.more);
}

#[tokio::test]
async fn a_kept_site_is_nobody_elses_and_goes_with_the_account() {
    let clock = TestClock::new();
    let (router, store) = server(settings(), &clock);
    let dario = sign_up(&router, "dario").await;
    let anna = sign_up(&router, "anna").await.token;
    let sites = seen(&recorded().await).await;
    keep(&router, &dario.token, &sites[0]).await;
    keep(&router, &anna, &sites[1]).await;

    // Each reads their own list; without an account there is none.
    assert_eq!(
        ids(&changes(&router, &anna, None).await),
        [&sites[1].site.id]
    );
    assert_eq!(
        ids(&changes(&router, &dario.token, None).await),
        [&sites[0].site.id]
    );
    for (method, path) in [
        (Method::GET, KEPT_SITE_CHANGES_PATH.to_owned()),
        (Method::PUT, kept_site_path(&sites[0].site.id)),
        (Method::DELETE, kept_site_path(&sites[0].site.id)),
    ] {
        ask(method, &path)
            .json(&sites[0])
            .send(&router)
            .await
            .error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
    }
    // Forgetting a site another pilot keeps forgets nothing of theirs.
    forget(&router, &anna, &sites[0].site.id).await;
    assert_eq!(
        ids(&changes(&router, &dario.token, None).await),
        [&sites[0].site.id]
    );

    // The list goes with the account.
    use venti_store::Store;
    let (account, _) = store.password_hash("dario").await.unwrap().unwrap();
    let held = store.kept_site_changes(&account.id, 0, 100).await.unwrap();
    assert_eq!(held.kept.len(), 1);
    let deleted = ask(Method::DELETE, ACCOUNT_PATH)
        .token(&dario.token)
        .json(&DeleteAccountRequest {
            password: Password::new(PASSWORD),
            google_id_token: None,
        })
        .send(&router)
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let left = store.kept_site_changes(&account.id, 0, 100).await.unwrap();
    assert_eq!(
        (left.kept.len(), left.forgotten.len(), left.latest),
        (0, 0, 0)
    );
    assert_eq!(
        ids(&changes(&router, &anna, None).await),
        [&sites[1].site.id]
    );
}

#[tokio::test]
async fn the_documents_show_keeping_a_site_as_the_development_mode_answers() {
    let router = recorded().await;
    let sites = seen(&router).await;
    let landing = sites.iter().find(|seen| seen.site.name.is_some()).unwrap();
    let request = KeepSiteRequest {
        kept_at: Some("2026-11-18T17:30:00Z".parse().unwrap()),
        ..landing.clone()
    };
    assert_golden(&golden("kept_site_keep_request.json"), &request);
    let kept = keep(&router, DEMO_TOKEN, &request).await;
    assert_golden(&golden("kept_site.json"), &kept);

    let other = sites
        .iter()
        .find(|seen| seen.site.id != landing.site.id)
        .unwrap();
    keep(&router, DEMO_TOKEN, other).await;
    forget(&router, DEMO_TOKEN, &other.site.id).await;
    let all = changes(&router, DEMO_TOKEN, None).await;
    let example = KeptSiteChangesResponse {
        cursor: "1795028700000002".to_owned(),
        ..all
    };
    assert_golden(&golden("kept_site_changes.json"), &example);
}
