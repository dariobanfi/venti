//! Tests of pilots among each other, through the real router: following,
//! the feed, cheers, comments, and what a pilot is told.

use axum::Router;
use axum::http::{Method, StatusCode, header};
use venti_api_types::v1::{
    ACCOUNT_FEED_PATH, ACCOUNT_FOLLOWING_PATH, ACCOUNT_NOTIFICATIONS_PATH, CheerResponse,
    CheersResponse, CommentRequest, CommentResponse, CommentsOpen, CommentsResponse, ErrorCode,
    FeedResponse, FlightId, FlightsResponse, FollowResponse, FollowsResponse, MAX_COMMENT_LENGTH,
    NotificationKind, NotificationsResponse, PUBLIC_FLIGHTS_PATH, REPORTS_PATH, ReportReason,
    ReportRequest, ReportResponse, ReportSubject, ReportedComment, Visibility, feed_path,
    flight_cheer_path, flight_cheers_path, flight_comment_path, flight_comments_path,
    notifications_path, pilot_follow_path, pilot_followers_path, pilot_following_path,
};
use venti_server::clock::Clock;
use venti_server::dev::{DEMO_COMMENTS, DEMO_HANDLE, DEMO_TOKEN};
use venti_server::moderation::{Action, Site, perform};
use venti_sources::testing::assert_golden;
use venti_store::{ReportListing, Store};

mod common;

use common::{
    Answer, TestClock, ask, get, golden, recorded, sample_file, sample_file_on, server,
    set_visibility, settings, sign_up, sign_up_private, upload,
};

async fn with(router: &Router, method: Method, path: &str, token: &str) -> Answer {
    ask(method, path).token(token).send(router).await
}

async fn public_flight(router: &Router, token: &str, file: &[u8]) -> FlightId {
    let flight = upload(router, token, file).await.flight.id;
    set_visibility(router, token, &flight, Visibility::Public).await;
    flight
}

async fn comment(router: &Router, token: &str, flight: &FlightId, text: &str) -> Answer {
    ask(Method::POST, &flight_comments_path(flight))
        .token(token)
        .json(&CommentRequest {
            text: text.to_owned(),
        })
        .send(router)
        .await
}

fn handles(follows: &FollowsResponse) -> Vec<&str> {
    follows
        .pilots
        .iter()
        .map(|followed| followed.pilot.handle.as_str())
        .collect()
}

#[tokio::test]
async fn a_pilot_follows_another_and_reads_their_flights() {
    let clock = TestClock::new();
    let (router, _) = server(settings(), &clock);
    let dario = sign_up(&router, "dario").await.token;
    let anna = sign_up(&router, "anna").await.token;
    sign_up(&router, "bernd").await;
    let flight = public_flight(&router, &anna, &sample_file()).await;

    // Nobody follows without an account, a pilot without a page is
    // nobody, and nobody follows themselves.
    ask(Method::PUT, &pilot_follow_path("anna"))
        .send(&router)
        .await
        .error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
    for nobody in ["bernd", "carla", "Not-A-Handle"] {
        with(&router, Method::PUT, &pilot_follow_path(nobody), &dario)
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::PilotNotFound);
    }
    with(&router, Method::PUT, &pilot_follow_path("anna"), &anna)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidFollow);

    let empty: FeedResponse = with(&router, Method::GET, ACCOUNT_FEED_PATH, &dario)
        .await
        .ok(StatusCode::OK);
    assert_eq!(empty.flights, []);

    for _ in 0..2 {
        let answer = with(&router, Method::PUT, &pilot_follow_path("anna"), &dario).await;
        answer.is_private();
        let followed: FollowResponse = answer.ok(StatusCode::OK);
        assert_eq!(
            followed,
            FollowResponse {
                following: true,
                followers: 1
            }
        );
    }

    // To everyone: one follower, who is not named while they share
    // nothing. The answer may be kept, and says nothing about who asks.
    let answer = get(&router, &pilot_followers_path("anna")).await;
    assert!(answer.headers.contains_key(header::ETAG));
    let followers: FollowsResponse = answer.ok(StatusCode::OK);
    assert_eq!((followers.followers, followers.following), (1, 0));
    assert_eq!(handles(&followers), Vec::<&str>::new());
    assert_eq!(followers.you_follow, None);
    // To the follower: that they follow.
    let answer = with(&router, Method::GET, &pilot_followers_path("anna"), &dario).await;
    answer.is_private();
    let followers: FollowsResponse = answer.ok(StatusCode::OK);
    assert_eq!(followers.you_follow, Some(true));
    // With a page of their own the follower is named.
    public_flight(&router, &dario, &sample_file_on("050726")).await;
    let followers: FollowsResponse = get(&router, &pilot_followers_path("anna"))
        .await
        .ok(StatusCode::OK);
    assert_eq!(handles(&followers), ["dario"]);
    assert_eq!(followers.pilots[0].since, clock.now());
    let following: FollowsResponse = get(&router, &pilot_following_path("dario"))
        .await
        .ok(StatusCode::OK);
    assert_eq!(handles(&following), ["anna"]);
    assert_eq!((following.followers, following.following), (0, 1));
    let own: FollowsResponse = with(&router, Method::GET, ACCOUNT_FOLLOWING_PATH, &dario)
        .await
        .ok(StatusCode::OK);
    assert_eq!(handles(&own), ["anna"]);
    get(&router, ACCOUNT_FOLLOWING_PATH)
        .await
        .error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);

    // The feed: the followed pilot's public flights, and not one's own.
    let answer = with(&router, Method::GET, ACCOUNT_FEED_PATH, &dario).await;
    answer.is_private();
    let feed: FeedResponse = answer.ok(StatusCode::OK);
    assert_eq!(feed.flights.len(), 1);
    assert_eq!(feed.flights[0].flight.id, flight);
    assert_eq!(
        (
            feed.flights[0].cheers,
            feed.flights[0].comments,
            feed.flights[0].cheered
        ),
        (0, 0, false)
    );
    assert_eq!(feed.next, None);
    with(
        &router,
        Method::GET,
        &feed_path(Some("nonsense"), None),
        &dario,
    )
    .await
    .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidCursor);
    // A flight made private leaves it at once.
    set_visibility(&router, &anna, &flight, Visibility::Unlisted).await;
    let feed: FeedResponse = with(&router, Method::GET, ACCOUNT_FEED_PATH, &dario)
        .await
        .ok(StatusCode::OK);
    assert_eq!(feed.flights, []);
    set_visibility(&router, &anna, &flight, Visibility::Public).await;

    // The followed pilot is told, once.
    let told: NotificationsResponse = with(&router, Method::GET, ACCOUNT_NOTIFICATIONS_PATH, &anna)
        .await
        .ok(StatusCode::OK);
    assert_eq!(told.notifications.len(), 1);
    assert_eq!(told.notifications[0].kind, NotificationKind::Followed);
    assert_eq!(told.notifications[0].pilot.handle.as_str(), "dario");
    assert_eq!(told.notifications[0].flight, None);

    // Following no more; a second time changes nothing, and a handle one
    // never followed that leads nowhere is nobody's.
    for _ in 0..2 {
        let left: FollowResponse =
            with(&router, Method::DELETE, &pilot_follow_path("anna"), &dario)
                .await
                .ok(StatusCode::OK);
        assert_eq!(
            left,
            FollowResponse {
                following: false,
                followers: 0
            }
        );
    }
    with(&router, Method::DELETE, &pilot_follow_path("bernd"), &dario)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::PilotNotFound);
}

#[tokio::test]
async fn a_public_flight_is_cheered_and_commented_on() {
    let clock = TestClock::new();
    let (router, store) = server(settings(), &clock);
    let dario = sign_up_private(&router, "dario").await.token;
    let anna = sign_up_private(&router, "anna").await.token;
    let bernd = sign_up_private(&router, "bernd").await.token;
    let flight = public_flight(&router, &anna, &sample_file()).await;
    let private = upload(&router, &anna, &sample_file_on("050726"))
        .await
        .flight
        .id;

    // A cheer: one per pilot, none for one's own flight, none for a
    // flight that is not public.
    with(&router, Method::PUT, &flight_cheer_path(&flight), &anna)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidCheer);
    with(&router, Method::PUT, &flight_cheer_path(&private), &dario)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
    for _ in 0..2 {
        let cheered: CheerResponse =
            with(&router, Method::PUT, &flight_cheer_path(&flight), &dario)
                .await
                .ok(StatusCode::OK);
        assert_eq!(
            cheered,
            CheerResponse {
                cheered: true,
                cheers: 1
            }
        );
    }
    clock.advance(chrono::Duration::minutes(1));
    with(&router, Method::PUT, &flight_cheer_path(&flight), &bernd)
        .await
        .ok::<CheerResponse>(StatusCode::OK);
    let cheers: CheersResponse = get(&router, &flight_cheers_path(&flight))
        .await
        .ok(StatusCode::OK);
    assert_eq!((cheers.count, cheers.cheered), (2, None));
    let who: Vec<&str> = cheers
        .cheers
        .iter()
        .map(|cheer| cheer.pilot.handle.as_str())
        .collect();
    assert_eq!(who, ["bernd", "dario"]);
    let answer = with(&router, Method::GET, &flight_cheers_path(&flight), &anna).await;
    answer.is_private();
    assert_eq!(
        answer.ok::<CheersResponse>(StatusCode::OK).cheered,
        Some(false)
    );
    let page: CheersResponse = get(&router, &format!("{}?limit=1", flight_cheers_path(&flight)))
        .await
        .ok(StatusCode::OK);
    assert_eq!(page.cheers.len(), 1);
    let next = page.next.expect("a second page");
    let rest: CheersResponse = get(
        &router,
        &format!("{}?limit=1&after={next}", flight_cheers_path(&flight)),
    )
    .await
    .ok(StatusCode::OK);
    assert_eq!(rest.cheers[0].pilot.handle.as_str(), "dario");
    assert_eq!(rest.next, None);

    // Comments: plain text, bounded, the oldest first.
    for bad in ["   ", &"x".repeat(MAX_COMMENT_LENGTH + 1)] {
        comment(&router, &dario, &flight, bad)
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidComment);
    }
    comment(&router, &dario, &private, "Hello")
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
    let answer = comment(&router, &dario, &flight, "  Well flown!\r\nWhat a line.  ").await;
    answer.is_private();
    let first: CommentResponse = answer.ok(StatusCode::CREATED);
    assert_eq!(first.comment.text, "Well flown!\nWhat a line.");
    assert_eq!(first.comment.pilot.handle.as_str(), "dario");
    assert_eq!(first.count, 1);
    clock.advance(chrono::Duration::minutes(1));
    let second: CommentResponse = comment(&router, &anna, &flight, "Thank you.")
        .await
        .ok(StatusCode::CREATED);
    let listed: CommentsResponse = get(&router, &flight_comments_path(&flight))
        .await
        .ok(StatusCode::OK);
    assert_eq!((listed.count, listed.open), (2, true));
    assert_eq!(listed.comments, [first.comment.clone(), second.comment]);

    // The flight's pilot turns comments off, and on again. Nobody else.
    let off = CommentsOpen { open: false };
    ask(Method::PATCH, &flight_comments_path(&flight))
        .token(&dario)
        .json(&off)
        .send(&router)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
    let set: CommentsOpen = ask(Method::PATCH, &flight_comments_path(&flight))
        .token(&anna)
        .json(&off)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(set, off);
    comment(&router, &dario, &flight, "Once more")
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::CommentsOff);
    let listed: CommentsResponse = get(&router, &flight_comments_path(&flight))
        .await
        .ok(StatusCode::OK);
    assert_eq!((listed.count, listed.open), (2, false));
    ask(Method::PATCH, &flight_comments_path(&flight))
        .token(&anna)
        .json(&CommentsOpen { open: true })
        .send(&router)
        .await
        .ok::<CommentsOpen>(StatusCode::OK);

    // What the flight's pilot was told: two cheers and one comment, and
    // nothing of what they wrote themselves.
    let told: NotificationsResponse = with(&router, Method::GET, ACCOUNT_NOTIFICATIONS_PATH, &anna)
        .await
        .ok(StatusCode::OK);
    let kinds: Vec<(NotificationKind, &str)> = told
        .notifications
        .iter()
        .map(|told| (told.kind, told.pilot.handle.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            (NotificationKind::Cheered, "dario"),
            (NotificationKind::Cheered, "bernd"),
            (NotificationKind::Commented, "dario"),
        ]
    );
    let about = told.notifications[2].flight.as_ref().unwrap();
    assert_eq!(about.id, flight);
    assert_eq!(
        told.notifications[2].comment.as_deref(),
        Some(first.comment.id.as_str())
    );
    assert!(!told.more && !told.reset);
    let since = notifications_path(Some(&told.cursor), None);
    let later: NotificationsResponse = with(&router, Method::GET, &since, &anna)
        .await
        .ok(StatusCode::OK);
    assert_eq!(later.notifications, []);
    assert_eq!(later.cursor, told.cursor);

    // A comment is reported with its words.
    let report = ReportRequest {
        subject: ReportSubject::Comment(ReportedComment {
            flight: flight.to_string(),
            id: first.comment.id.clone(),
        }),
        reason: ReportReason::Offensive,
        text: None,
    };
    let sent: ReportResponse = ask(Method::POST, REPORTS_PATH)
        .token(&bernd)
        .json(&report)
        .send(&router)
        .await
        .ok(StatusCode::CREATED);
    assert!(sent.created);
    let reports = store.reports(ReportListing::Open, None, 10).await.unwrap();
    assert_eq!(
        reports[0].subject,
        venti_store::ReportSubject::Comment {
            id: first.comment.id.clone(),
            flight_id: flight.to_string(),
            text: "Well flown!\nWhat a line.".to_owned(),
        }
    );
    assert_eq!(reports[0].about.as_ref().unwrap().handle, "dario");

    // A comment is removed by its author or by the flight's pilot, and
    // by nobody else.
    let path = flight_comment_path(&flight, &first.comment.id);
    with(&router, Method::DELETE, &path, &bernd)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::CommentNotFound);
    let removed = with(&router, Method::DELETE, &path, &anna).await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    with(&router, Method::DELETE, &path, &dario)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::CommentNotFound);
    let listed: CommentsResponse = get(&router, &flight_comments_path(&flight))
        .await
        .ok(StatusCode::OK);
    assert_eq!(listed.count, 1);

    // A flight that is public no more shows nothing of it and takes
    // nothing; what was given is kept and is back with the flight.
    set_visibility(&router, &anna, &flight, Visibility::Unlisted).await;
    for path in [flight_cheers_path(&flight), flight_comments_path(&flight)] {
        get(&router, &path)
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
        with(&router, Method::GET, &path, &anna)
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
    }
    comment(&router, &dario, &flight, "Hello")
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
    // A cheer is still its pilot's to take back.
    let back: CheerResponse = with(&router, Method::DELETE, &flight_cheer_path(&flight), &bernd)
        .await
        .ok(StatusCode::OK);
    assert_eq!(
        back,
        CheerResponse {
            cheered: false,
            cheers: 0
        }
    );
    with(&router, Method::DELETE, &flight_cheer_path(&flight), &bernd)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
    set_visibility(&router, &anna, &flight, Visibility::Public).await;
    let cheers: CheersResponse = get(&router, &flight_cheers_path(&flight))
        .await
        .ok(StatusCode::OK);
    assert_eq!(cheers.count, 1);

    // An account that may not publish gives nothing new.
    let account = store.account("bernd").await.unwrap().unwrap();
    store
        .suspend_publishing(&account.id, clock.now())
        .await
        .unwrap();
    with(&router, Method::PUT, &flight_cheer_path(&flight), &bernd)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::PublishingSuspended);
    comment(&router, &bernd, &flight, "Hello")
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::PublishingSuspended);
    with(&router, Method::PUT, &pilot_follow_path("anna"), &bernd)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::PublishingSuspended);

    // An account that goes takes its cheers and comments along.
    let account = store.account("dario").await.unwrap().unwrap();
    store.delete_account(&account.id).await.unwrap();
    let cheers: CheersResponse = get(&router, &flight_cheers_path(&flight))
        .await
        .ok(StatusCode::OK);
    assert_eq!((cheers.count, cheers.cheers.len()), (0, 0));

    // After its days, what was told is told no more.
    clock.advance(chrono::Duration::days(31));
    let old: NotificationsResponse = with(&router, Method::GET, ACCOUNT_NOTIFICATIONS_PATH, &anna)
        .await
        .ok(StatusCode::OK);
    assert_eq!(old.notifications, []);
}

#[tokio::test]
async fn the_development_mode_has_pilots_among_each_other() {
    let router = recorded().await;

    // The made-up pilots add no flight: the public list is as it was.
    let all: FlightsResponse = get(&router, &format!("{PUBLIC_FLIGHTS_PATH}?limit=100"))
        .await
        .ok(StatusCode::OK);
    assert_eq!(all.flights.len(), 11);

    let followers: FollowsResponse = get(&router, &pilot_followers_path(DEMO_HANDLE))
        .await
        .ok(StatusCode::OK);
    assert_eq!(handles(&followers), ["lena", "jonas", "mara"]);
    assert_eq!((followers.followers, followers.following), (3, 3));
    assert_golden(&golden("followers.json"), &followers);

    let feed: FeedResponse = with(&router, Method::GET, &feed_path(None, Some(3)), DEMO_TOKEN)
        .await
        .ok(StatusCode::OK);
    assert_eq!(feed.flights.len(), 3);
    assert!(feed.next.is_some());
    assert!(feed.flights.iter().any(|flight| flight.cheered));
    assert_golden(&golden("feed.json"), &feed);
    let mut seen = feed.flights.len();
    let mut next = feed.next;
    while let Some(before) = next {
        let page: FeedResponse = with(
            &router,
            Method::GET,
            &feed_path(Some(&before), Some(2)),
            DEMO_TOKEN,
        )
        .await
        .ok(StatusCode::OK);
        seen += page.flights.len();
        next = page.next;
    }
    // Every public flight of the three followed pilots, and no other.
    let followed = all
        .flights
        .iter()
        .filter(|flight| ["mara", "jonas", "lena"].contains(&flight.pilot.handle.as_str()))
        .count();
    assert_eq!(seen, followed);

    let own = all
        .flights
        .iter()
        .find(|flight| flight.pilot.handle.as_str() == DEMO_HANDLE)
        .expect("the demo account has a public flight");
    let cheers: CheersResponse = get(&router, &flight_cheers_path(&own.id))
        .await
        .ok(StatusCode::OK);
    assert_eq!(cheers.count, 2);
    assert_golden(&golden("cheers.json"), &cheers);
    let comments: CommentsResponse = get(&router, &flight_comments_path(&own.id))
        .await
        .ok(StatusCode::OK);
    let ids: Vec<&str> = comments.comments.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, DEMO_COMMENTS[..2]);
    assert_golden(&golden("comments.json"), &comments);

    let told: NotificationsResponse =
        with(&router, Method::GET, ACCOUNT_NOTIFICATIONS_PATH, DEMO_TOKEN)
            .await
            .ok(StatusCode::OK);
    assert_eq!(told.notifications.len(), 4);
    assert_golden(&golden("notifications.json"), &told);

    // What the writes answer.
    let followed: FollowResponse =
        with(&router, Method::PUT, &pilot_follow_path("timo"), DEMO_TOKEN)
            .await
            .ok(StatusCode::OK);
    assert_golden(&golden("follow.json"), &followed);
    let theirs = feed.flights.iter().find(|flight| !flight.cheered).unwrap();
    let cheered: CheerResponse = with(
        &router,
        Method::PUT,
        &flight_cheer_path(&theirs.flight.id),
        DEMO_TOKEN,
    )
    .await
    .ok(StatusCode::OK);
    assert_golden(&golden("cheer.json"), &cheered);
    let request = CommentRequest {
        text: "A made-up comment of a test.".to_owned(),
    };
    assert_golden(&golden("comment_request.json"), &request);
    let mut written: CommentResponse = ask(Method::POST, &flight_comments_path(&theirs.flight.id))
        .token(DEMO_TOKEN)
        .json(&request)
        .send(&router)
        .await
        .ok(StatusCode::CREATED);
    // The identifier is made by chance: the file keeps a made-up one.
    "0000000000000000000000c009".clone_into(&mut written.comment.id);
    assert_golden(&golden("comment.json"), &written);
    let open: CommentsOpen = ask(Method::PATCH, &flight_comments_path(&own.id))
        .token(DEMO_TOKEN)
        .json(&CommentsOpen { open: false })
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_golden(&golden("comments_open.json"), &open);
}

#[tokio::test]
async fn whoever_runs_the_server_reads_a_reported_comment_and_removes_it() {
    let clock = TestClock::new();
    let (router, store) = server(settings(), &clock);
    let dario = sign_up(&router, "dario").await.token;
    let anna = sign_up(&router, "anna").await.token;
    let flight = public_flight(&router, &anna, &sample_file()).await;
    let written: CommentResponse = comment(&router, &dario, &flight, "Words that should go.")
        .await
        .ok(StatusCode::CREATED);
    let id = written.comment.id;

    // A comment that is not under the flight the report names is none.
    let report = |flight: String, id: String| ReportRequest {
        subject: ReportSubject::Comment(ReportedComment { flight, id }),
        reason: ReportReason::Spam,
        text: None,
    };
    for (missing, code) in [
        (
            report(flight.to_string(), "0000000000000000000000c001".to_owned()),
            ErrorCode::CommentNotFound,
        ),
        (
            report("0123456789abcdefghjkmnpqrs".to_owned(), id.clone()),
            ErrorCode::FlightNotFound,
        ),
    ] {
        ask(Method::POST, REPORTS_PATH)
            .token(&anna)
            .json(&missing)
            .send(&router)
            .await
            .error(StatusCode::NOT_FOUND, code);
    }
    // A reason that is about a flight is none for words.
    let not_a_flight = ReportRequest {
        reason: ReportReason::NotAFlight,
        ..report(flight.to_string(), id.clone())
    };
    ask(Method::POST, REPORTS_PATH)
        .token(&anna)
        .json(&not_a_flight)
        .send(&router)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidReport);
    ask(Method::POST, REPORTS_PATH)
        .token(&anna)
        .json(&report(flight.to_string(), id.clone()))
        .send(&router)
        .await
        .ok::<ReportResponse>(StatusCode::CREATED);

    let site = Site {
        public_url: None,
        live_hours: 48,
        plan_days: 2,
    };
    let operator = |action: Action| {
        let (store, clock) = (store.clone(), clock.clone());
        async move { perform(&*store, &clock, site, &action).await.unwrap() }
    };
    let listed = operator(Action::Reports { all: false }).await;
    for expected in [
        format!("comment {id} of dario (Pilot dario), under flight {flight}"),
        "Words that should go.".to_owned(),
        "still there, the same".to_owned(),
        format!("venti-server remove-comment {id}"),
        "Reported by:  anna".to_owned(),
    ] {
        assert!(
            listed.said.contains(&expected),
            "{expected}\n{}",
            listed.said
        );
    }

    let removed = operator(Action::RemoveComment {
        comment: id.clone(),
    })
    .await;
    assert!(removed.found, "{}", removed.said);
    assert!(removed.said.contains("Words that should go."));
    let listed: CommentsResponse = get(&router, &flight_comments_path(&flight))
        .await
        .ok(StatusCode::OK);
    assert_eq!((listed.count, listed.comments.len()), (0, 0));
    // The report stays, with the words it kept, and says the comment is
    // gone; a second removal finds nothing, and neither does nonsense.
    let listed = operator(Action::Reports { all: false }).await;
    assert!(listed.said.contains("Words that should go."));
    assert!(listed.said.contains("gone: removed"), "{}", listed.said);
    for nothing in [id.as_str(), "nonsense"] {
        let again = operator(Action::RemoveComment {
            comment: nothing.to_owned(),
        })
        .await;
        assert!(!again.found, "{}", again.said);
    }
}
