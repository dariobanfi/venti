//! Everything a pilot gave, as one ZIP file: `GET /v1/account/archive`.
//!
//! The archive of the development mode's demo account is opened and held
//! against what the API answers for the same account, its flight files are
//! uploaded to a fresh account, and the bounds are tried on a server with
//! the limits a real one has.

mod common;

use std::collections::BTreeSet;
use std::io::{Cursor, Read};

use axum::http::{Method, StatusCode, header};
use serde::de::DeserializeOwned;
use venti_api_types::v1::{
    ACCOUNT_ARCHIVE_PATH, ACCOUNT_PATH, ACCOUNT_TOKENS_PATH, ALERTS_FIRED_PATH, ALERTS_PATH,
    ARCHIVE_FORMAT, ARCHIVE_INDEX_FILE, ARCHIVE_MEDIA_TYPE, AccessTokensResponse, AccountResponse,
    AlertsResponse, ArchiveAlerts, ArchiveIndex, ArchiveReports, ArchiveSites, ErrorCode,
    FLIGHTS_PATH, FiredAlertsResponse, Flight, FlightsResponse, GROUPS_PATH, GroupsResponse,
    KEPT_SITE_CHANGES_PATH, KeptSiteChangesResponse, MAX_ARCHIVE_FILE_BYTES, PlansResponse,
    REPORTS_PATH, ReportRequest, SESSIONS_PATH, SessionsResponse, SiteRevisionKind, UploadResponse,
    flight_file_path, site_confirmation_path,
};
use venti_server::accounts::Limits;
use venti_server::dev;
use venti_sources::testing::assert_golden;

use common::{Answer, TestClock, ask, golden, recorded, server, settings, sign_up};

/// An archive, opened.
struct Archive(zip::ZipArchive<Cursor<Vec<u8>>>);

impl Archive {
    fn of(answer: &Answer) -> Self {
        assert_eq!(answer.status, StatusCode::OK, "{:?}", answer.headers);
        assert_eq!(answer.header(header::CONTENT_TYPE), ARCHIVE_MEDIA_TYPE);
        // Made for one account: nobody keeps it.
        answer.is_private();
        // And not packed a second time on its way.
        assert!(!answer.headers.contains_key(header::CONTENT_ENCODING));
        Self(zip::ZipArchive::new(Cursor::new(answer.body.clone())).unwrap())
    }

    fn names(&self) -> BTreeSet<String> {
        self.0
            .file_names()
            .map(|name| name.unwrap().into_owned())
            .collect()
    }

    fn bytes(&mut self, name: &str) -> Vec<u8> {
        let mut file = self
            .0
            .by_name(name)
            .unwrap_or_else(|_| panic!("the archive has no {name}"));
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        bytes
    }

    fn json<T: DeserializeOwned>(&mut self, name: &str) -> T {
        serde_json::from_slice(&self.bytes(name))
            .unwrap_or_else(|error| panic!("{name} is not what its record says: {error}"))
    }
}

async fn archive_of(router: &axum::Router, token: &str) -> Answer {
    // As a browser asks: ready to take an answer that is compressed on
    // its way. A ZIP file is not.
    ask(Method::GET, ACCOUNT_ARCHIVE_PATH)
        .token(token)
        .header(header::ACCEPT_ENCODING, "gzip, br")
        .send(router)
        .await
}

async fn own<T: DeserializeOwned>(router: &axum::Router, token: &str, path: &str) -> T {
    ask(Method::GET, path)
        .token(token)
        .send(router)
        .await
        .ok(StatusCode::OK)
}

/// What makes two flights the same flight on two accounts: what the file
/// says, and what the server worked out of it.
fn what_it_is(flight: &Flight) -> (String, String, String) {
    (
        flight.identity.clone(),
        flight.start.to_rfc3339(),
        format!(
            "{:?} {:?} {:?} {:?}",
            flight.airtime_s, flight.track_length_m, flight.takeoff, flight.landing
        ),
    )
}

#[tokio::test]
async fn the_demo_accounts_archive_holds_its_flights_and_they_read_back_into_a_fresh_account() {
    let router = recorded().await;
    let token = dev::DEMO_TOKEN;
    let answer = archive_of(&router, token).await;
    assert_eq!(
        answer.header(header::CONTENT_DISPOSITION),
        "attachment; filename=\"venti-demo-2026-11-18.zip\""
    );
    let mut archive = Archive::of(&answer);

    // The index: what the archive is, and what each file holds.
    let index: ArchiveIndex = archive.json(ARCHIVE_INDEX_FILE);
    assert_golden(&golden("archive_index.json"), &index);
    assert_eq!(index.format, ARCHIVE_FORMAT);
    assert_eq!(index.account.as_str(), dev::DEMO_HANDLE);
    assert!(index.complete);

    // Its three flights, as the account's own list has them, each with
    // its file as the server hands it out.
    let listed: FlightsResponse = archive.json("flights.json");
    let theirs: FlightsResponse = own(&router, token, FLIGHTS_PATH).await;
    assert_eq!(listed.flights, theirs.flights);
    assert_eq!(index.flights.len(), 3);
    let fresh = sign_up(&router, "petra").await.token;
    for (flight, record) in index.flights.iter().zip(&listed.flights) {
        assert_eq!(flight.id, record.id);
        let name = flight.file.as_deref().expect("a file for every flight");
        assert!(name.starts_with("flights/") && name.ends_with(".igc"));
        assert!(name.contains(&record.start.format("%Y-%m-%d").to_string()));
        let file = archive.bytes(name);
        let served = ask(Method::GET, &flight_file_path(&flight.id))
            .token(token)
            .send(&router)
            .await;
        assert_eq!(file, served.body, "{name}");

        // And the file makes the flight again, on another account.
        let again: UploadResponse = ask(Method::POST, FLIGHTS_PATH)
            .token(&fresh)
            .bytes(file)
            .send(&router)
            .await
            .ok(StatusCode::CREATED);
        assert!(again.created);
    }
    let back: FlightsResponse = own(&router, &fresh, FLIGHTS_PATH).await;
    let was: Vec<_> = listed.flights.iter().map(what_it_is).collect();
    let is: Vec<_> = back.flights.iter().map(what_it_is).collect();
    assert_eq!(was.len(), 3);
    assert_eq!(is, was);
}

#[tokio::test]
async fn every_file_of_the_archive_is_listed_and_is_the_record_the_api_answers() {
    let router = recorded().await;
    let token = dev::DEMO_TOKEN;
    // Something of every kind: the demo account says where it took off
    // and reports a flight, beside what the development mode gave it.
    let site = dev::DEMO_SITES[0].parse().unwrap();
    let said = std::fs::read_to_string(golden("site_confirm_request.json")).unwrap();
    let said: serde_json::Value = serde_json::from_str(&said).unwrap();
    let confirmed = ask(Method::PUT, &site_confirmation_path(&site))
        .token(token)
        .json(&said)
        .send(&router)
        .await;
    assert_eq!(confirmed.status, StatusCode::OK);
    let sent = std::fs::read_to_string(golden("report_request.json")).unwrap();
    let sent: ReportRequest = serde_json::from_str(&sent).unwrap();
    let reported = ask(Method::POST, REPORTS_PATH)
        .token(token)
        .json(&sent)
        .send(&router)
        .await;
    assert_eq!(reported.status, StatusCode::CREATED);

    let answer = archive_of(&router, token).await;
    let mut archive = Archive::of(&answer);
    let index: ArchiveIndex = archive.json(ARCHIVE_INDEX_FILE);

    // The index lists every file there is, and nothing else is there.
    let mut listed: BTreeSet<String> = index.files.iter().map(|file| file.path.clone()).collect();
    listed.extend(
        index
            .flights
            .iter()
            .filter_map(|flight| flight.file.clone()),
    );
    listed.insert(ARCHIVE_INDEX_FILE.to_owned());
    listed.insert("README.txt".to_owned());
    assert_eq!(archive.names(), listed);

    // No secret is in any of them: not a session's token, not a
    // program's, not the password.
    for name in archive.names() {
        let bytes = archive.bytes(&name);
        let text = String::from_utf8_lossy(&bytes);
        for secret in [
            dev::DEMO_TOKEN,
            dev::DEMO_ACCESS_TOKEN,
            dev::DEMO_PASSWORD,
            "argon2",
        ] {
            assert!(!text.contains(secret), "{name} holds a secret");
        }
    }

    // Each file is the record its line of the index names: here, the
    // very answer of the request that reads the same.
    let account: AccountResponse = archive.json("account.json");
    assert_eq!(account, own(&router, token, ACCOUNT_PATH).await);
    let sessions: SessionsResponse = archive.json("sessions.json");
    assert_eq!(sessions, own(&router, token, SESSIONS_PATH).await);
    let tokens: AccessTokensResponse = archive.json("tokens.json");
    assert_eq!(tokens, own(&router, token, ACCOUNT_TOKENS_PATH).await);
    assert_eq!(tokens.tokens[0].name, dev::DEMO_ACCESS_TOKEN_NAME);
    // The groups as the list has them; how the pilot stands to each and
    // its last message are not in the archive.
    let groups: GroupsResponse = archive.json("groups.json");
    let listed: GroupsResponse = own(&router, token, GROUPS_PATH).await;
    assert_eq!(groups.groups, listed.groups);
    assert!(groups.chats.is_empty());
    assert!(!groups.groups.is_empty());
    let kept: KeptSiteChangesResponse = archive.json("kept-sites.json");
    let theirs: KeptSiteChangesResponse = own(&router, token, KEPT_SITE_CHANGES_PATH).await;
    assert_eq!(
        (&kept.kept, &kept.forgotten),
        (&theirs.kept, &theirs.forgotten)
    );

    // The alarms, and what they fired, as the account's own requests
    // answer them.
    let alerts: ArchiveAlerts = archive.json("alerts.json");
    let theirs: AlertsResponse = own(&router, token, ALERTS_PATH).await;
    assert!(!alerts.alerts.is_empty());
    assert_eq!(alerts.alerts, theirs.alerts);
    let fired: FiredAlertsResponse = own(&router, token, ALERTS_FIRED_PATH).await;
    assert!(!alerts.fired.is_empty());
    assert_eq!(alerts.fired, fired.fired);

    // What the pilot gave among pilots: whom they follow as their own
    // list has it, their cheers and their comments.
    let social: venti_api_types::v1::ArchiveSocial = archive.json("social.json");
    let theirs: venti_api_types::v1::FollowsResponse =
        own(&router, token, venti_api_types::v1::ACCOUNT_FOLLOWING_PATH).await;
    assert_eq!(social.following.len(), 3);
    assert_eq!(social.following, theirs.pilots);
    assert_eq!(social.cheers.len(), 2);
    let written: Vec<&str> = social.comments.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(
        written,
        [
            venti_server::dev::DEMO_COMMENTS[2],
            venti_server::dev::DEMO_COMMENTS[1]
        ]
    );

    // The plans the pilot posted, and those of others they are in on.
    let plans: PlansResponse = archive.json("plans.json");
    assert!(!plans.plans.is_empty());
    for plan in &plans.plans {
        let is_in = plan
            .pilots_in
            .iter()
            .any(|pilot| pilot.handle.as_str() == "demo");
        assert!(plan.author.handle.as_str() == "demo" || is_in, "{plan:?}");
    }

    // What they gave to the sites: their revisions and no one else's,
    // the first of a site among them, and their confirmation.
    let sites: ArchiveSites = archive.json("sites.json");
    assert!(sites.revisions.iter().all(|revision| {
        revision
            .by
            .as_ref()
            .is_some_and(|by| by.handle.as_str() == "demo")
    }));
    let made: Vec<String> = sites
        .revisions
        .iter()
        .filter(|revision| revision.kind == SiteRevisionKind::Created)
        .map(|revision| revision.site.id.to_string())
        .collect();
    assert_eq!(made, [dev::DEMO_SITES[0], dev::DEMO_SITES[2]]);
    // The crowdsourced take-off was jonas's; the demo account put it
    // back, and that revision is its own.
    assert!(sites.revisions.iter().any(|revision| {
        revision.site.id.to_string() == dev::DEMO_SITES[1]
            && revision.kind == SiteRevisionKind::Restored
    }));
    let [confirmation] = sites.confirmations.as_slice() else {
        panic!("one confirmation: {:?}", sites.confirmations);
    };
    assert_eq!(confirmation.site, site);
    assert_eq!(confirmation.day.to_string(), "2026-11-18");
    let licence = index
        .files
        .iter()
        .find(|file| file.path == "sites.json")
        .and_then(|file| file.licence.as_ref())
        .unwrap();
    assert_eq!(licence.name, "CC0 1.0");
    assert!(
        index
            .files
            .iter()
            .all(|file| file.path == "sites.json" || file.licence.is_none())
    );

    // The report, as it was sent.
    let reports: ArchiveReports = archive.json("reports.json");
    let [report] = reports.reports.as_slice() else {
        panic!("one report: {:?}", reports.reports);
    };
    assert_eq!(report.report, sent);

    // And the text for a person names every file.
    let readme = String::from_utf8(archive.bytes("README.txt")).unwrap();
    for file in &index.files {
        assert!(readme.contains(&file.path), "{}", file.path);
    }
    assert!(readme.contains("CC0 1.0") && readme.contains("3 of 3 flights"));
}

#[tokio::test]
async fn only_the_pilots_own_sign_in_gets_the_archive() {
    let router = recorded().await;
    ask(Method::GET, ACCOUNT_ARCHIVE_PATH)
        .send(&router)
        .await
        .error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
    // A token for a program, whatever its scopes.
    archive_of(&router, dev::DEMO_ACCESS_TOKEN)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::SessionRequired);
    // And an account gets its own and nobody else's.
    let fresh = sign_up(&router, "petra").await.token;
    let mut archive = Archive::of(&archive_of(&router, &fresh).await);
    let index: ArchiveIndex = archive.json(ARCHIVE_INDEX_FILE);
    assert_eq!(index.account.as_str(), "petra");
    assert!(index.flights.is_empty() && index.complete);
    assert!(index.files.iter().all(|file| {
        // One account, and the one device that is signed in.
        let expected = u32::from(file.path == "account.json" || file.path == "sessions.json");
        file.entries == expected
    }));
}

#[tokio::test]
async fn an_account_asks_for_its_archive_a_few_times_a_day() {
    let clock = TestClock::new();
    let limited = venti_server::api::Settings {
        limits: Limits::default(),
        ..settings()
    };
    let (router, _) = server(limited, &clock);
    let token = sign_up(&router, "petra").await.token;
    let most = Limits::default().archives_per_day;
    for _ in 0..most {
        assert_eq!(archive_of(&router, &token).await.status, StatusCode::OK);
    }
    let refused = archive_of(&router, &token).await;
    refused.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    assert!(refused.headers.contains_key(header::RETRY_AFTER));
    // Another account is not held up by it, and a day later neither is
    // this one.
    let other = sign_up(&router, "quirin").await.token;
    assert_eq!(archive_of(&router, &other).await.status, StatusCode::OK);
    clock.advance(chrono::Duration::days(1) + chrono::Duration::seconds(1));
    assert_eq!(archive_of(&router, &token).await.status, StatusCode::OK);
}

/// `docs/api.md` states the bounds; this holds it to the code.
#[test]
fn the_documentation_states_the_bounds_the_code_has() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/api.md");
    let docs = std::fs::read_to_string(path).unwrap();
    for stated in [
        format!(
            "asks for its archive at most {} times a day",
            Limits::default().archives_per_day
        ),
        format!(
            "at most {} MB of flight files",
            MAX_ARCHIVE_FILE_BYTES / (1024 * 1024)
        ),
    ] {
        assert!(docs.contains(&stated), "docs/api.md does not say: {stated}");
    }
    let index = std::fs::read_to_string(golden("archive_index.json")).unwrap();
    assert!(
        docs.contains(index.trim_end()),
        "docs/api.md does not show archive_index.json as the tests keep it"
    );
}
