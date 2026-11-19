//! Tests of the page a link to a flight shows, and of its preview picture,
//! through the real router.

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use venti_api_types::v1::{
    ACCOUNTS_PATH, CreateAccountRequest, FLIGHTS_PATH, FlightId, FlightsResponse, Password,
    SignInResponse, Visibility, flight_file_path, share_image_path, share_path,
};
use venti_server::dev::DEMO_TOKEN;

mod common;

use common::{
    PASSWORD, PUBLIC_URL, TestClock, ask, assert_golden_text, get, get_with, recorded, sample_file,
    send, server, set_visibility, settings, upload,
};

/// The demo account's flights by who may see them: public, unlisted,
/// private.
async fn demo_flights(router: &Router) -> [FlightId; 3] {
    let mine: FlightsResponse = ask(Method::GET, FLIGHTS_PATH)
        .token(DEMO_TOKEN)
        .send(router)
        .await
        .ok(StatusCode::OK);
    let id = |visibility: Visibility| {
        let flight = mine.flights.iter().find(|f| f.visibility == visibility);
        flight.unwrap().id.clone()
    };
    [
        id(Visibility::Public),
        id(Visibility::Unlisted),
        id(Visibility::Private),
    ]
}

#[tokio::test]
async fn a_public_flight_has_a_page_for_people_and_for_previews() {
    let router = recorded().await;
    let [public, _, _] = demo_flights(&router).await;

    let answer = get(&router, &share_path(&public)).await;

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(
        answer.header(header::CONTENT_TYPE),
        "text/html; charset=utf-8"
    );
    let html = answer.text();
    // What a chat app reads to build its preview, with complete addresses.
    let page_url = format!("{PUBLIC_URL}/f/{public}");
    for line in [
        r#"<meta property="og:title" content="12.0 km free flight by Demo Pilot">"#.to_owned(),
        r#"<meta property="og:description" content="15 August 2026 · 29 min in the air · 11.98 points · Sample Wing">"#.to_owned(),
        format!(r#"<meta property="og:url" content="{page_url}">"#),
        format!(r#"<meta property="og:image" content="{page_url}/preview.png">"#),
        r#"<meta name="twitter:card" content="summary_large_image">"#.to_owned(),
    ] {
        assert!(html.contains(&line), "missing: {line}");
    }
    // What a person reads, and the way to the file.
    assert!(html.contains("<h1>12.0 km free flight</h1>"));
    assert!(html.contains("<b>Demo Pilot</b> @demo"));
    assert!(html.contains(&format!("href=\"{}\"", flight_file_path(&public))));
    let file = get(&router, &flight_file_path(&public)).await;
    assert_eq!(file.status, StatusCode::OK);
    assert!(file.body.starts_with(b"AXVE000"));

    // The page is the same on every start of the development mode.
    assert_golden_text("share_page.html", html);
}

#[tokio::test]
async fn the_page_forbids_itself_everything_it_does_not_need() {
    let router = recorded().await;
    let [public, unlisted, _] = demo_flights(&router).await;

    let answer = get(&router, &share_path(&public)).await;

    let policy = answer.header(header::CONTENT_SECURITY_POLICY);
    assert!(
        policy.starts_with("default-src 'none'; style-src 'sha256-"),
        "{policy}"
    );
    assert!(!policy.contains("unsafe-inline") && !policy.contains("script"));
    assert!(policy.contains("frame-ancestors 'none'"));
    assert_eq!(answer.header(header::X_CONTENT_TYPE_OPTIONS), "nosniff");
    assert_eq!(answer.header(header::REFERRER_POLICY), "same-origin");
    assert!(!answer.headers.contains_key(header::SET_COOKIE));
    // A public page may be kept and indexed; it is checked before reuse.
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, no-cache");
    assert!(!answer.headers.contains_key("x-robots-tag"));
    assert!(!answer.text().contains("noindex"));
    let tag = answer.header(header::ETAG).to_owned();
    let again = get_with(
        &router,
        &share_path(&public),
        &[(header::IF_NONE_MATCH, &tag)],
    )
    .await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    assert!(again.body.is_empty());

    // An unlisted page is for whoever has the link: not for shared caches,
    // not for search engines.
    let answer = get(&router, &share_path(&unlisted)).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header(header::CACHE_CONTROL), "private, no-cache");
    assert_eq!(answer.header("x-robots-tag".parse().unwrap()), "noindex");
    assert!(
        answer
            .text()
            .contains(r#"<meta name="robots" content="noindex">"#)
    );
    let picture = get(&router, &share_image_path(&unlisted)).await;
    assert_eq!(picture.header(header::CACHE_CONTROL), "private, no-cache");
    assert_eq!(picture.header("x-robots-tag".parse().unwrap()), "noindex");

    // Asked for with HEAD, as preview fetchers do first: the headers alone.
    let head = ask(Method::HEAD, &share_path(&public)).send(&router).await;
    assert_eq!(head.status, StatusCode::OK);
    assert!(head.body.is_empty());
    assert_eq!(
        head.header(header::CONTENT_TYPE),
        "text/html; charset=utf-8"
    );
}

#[tokio::test]
async fn a_link_to_nothing_gets_a_page_that_says_so_and_no_more() {
    let router = recorded().await;
    let [_, _, private] = demo_flights(&router).await;
    let mut answers = Vec::new();
    for path in [
        share_path(&private),
        "/f/0123456789abcdefghjkmnpqrs".to_owned(),
        "/f/x".to_owned(),
        "/f/..%2F..%2Fetc%2Fpasswd".to_owned(),
        "/f/%3Cscript%3Ealert(1)%3C%2Fscript%3E".to_owned(),
    ] {
        let answer = get(&router, &path).await;
        assert_eq!(answer.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(
            answer.header(header::CONTENT_TYPE),
            "text/html; charset=utf-8"
        );
        assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");
        assert_eq!(answer.header("x-robots-tag".parse().unwrap()), "noindex");
        assert!(answer.headers.contains_key(header::CONTENT_SECURITY_POLICY));
        answers.push(answer.text().to_owned());
    }
    // One page for all of them: nothing of the address is repeated in it,
    // and nothing tells a private flight from no flight.
    assert!(answers.iter().all(|page| page == &answers[0]));
    assert!(answers[0].contains("<h1>No flight here</h1>"));
    assert!(!answers[0].contains("script>"));

    // The owner's token changes nothing here: a browser that follows a
    // link sends none, so the page never depends on one.
    let owner = ask(Method::GET, &share_path(&private))
        .token(DEMO_TOKEN)
        .send(&router)
        .await;
    assert_eq!(owner.status, StatusCode::NOT_FOUND);
    // Only GET and HEAD are answered.
    let post = ask(Method::POST, &share_path(&private)).send(&router).await;
    assert_eq!(post.status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn the_preview_picture_is_a_png_drawn_by_the_server() {
    let router = recorded().await;
    let [public, _, _] = demo_flights(&router).await;
    let path = share_image_path(&public);

    let answer = get_with(&router, &path, &[(header::ACCEPT_ENCODING, "gzip")]).await;

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header(header::CONTENT_TYPE), "image/png");
    // A PNG is compressed already and is sent as it is.
    assert!(!answer.headers.contains_key(header::CONTENT_ENCODING));
    assert_eq!(&answer.body[..8], b"\x89PNG\r\n\x1a\n");
    // Width and height, as the page's head promises them.
    assert_eq!(&answer.body[16..24], [0, 0, 0x04, 0xb0, 0, 0, 0x02, 0x76]);
    assert_eq!(answer.header(header::X_CONTENT_TYPE_OPTIONS), "nosniff");
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, no-cache");

    // Whoever has it already is told so without the picture being drawn
    // again, until the flight changes.
    let tag = answer.header(header::ETAG).to_owned();
    let again = get_with(&router, &path, &[(header::IF_NONE_MATCH, &tag)]).await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    assert!(again.body.is_empty());
    set_visibility(&router, DEMO_TOKEN, &public, Visibility::Unlisted).await;
    let changed = get_with(&router, &path, &[(header::IF_NONE_MATCH, &tag)]).await;
    assert_eq!(changed.status, StatusCode::OK);
    assert_ne!(changed.header(header::ETAG), tag);
    // Taken back, the picture is gone like the page.
    set_visibility(&router, DEMO_TOKEN, &public, Visibility::Private).await;
    let gone = get_with(&router, &path, &[(header::IF_NONE_MATCH, &tag)]).await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn names_from_a_pilot_and_from_a_file_reach_the_page_as_text() {
    let (router, _) = server(settings(), &TestClock::new());
    // A display name is a pilot's to choose, and a glider's name comes out
    // of a file anyone can write.
    let request = CreateAccountRequest {
        handle: "mallory".to_owned(),
        display_name: "<script>alert('name')</script>".to_owned(),
        password: Password::new(PASSWORD),
        invite: None,
        device: None,
    };
    let mallory: SignInResponse = ask(Method::POST, ACCOUNTS_PATH)
        .json(&request)
        .send(&router)
        .await
        .ok(StatusCode::CREATED);
    let file = String::from_utf8(sample_file())
        .unwrap()
        .replace("Sample Wing", "\"><img src=x onerror=alert('glider')><!--");
    let flight = upload(&router, &mallory.token, file.as_bytes())
        .await
        .flight;
    set_visibility(&router, &mallory.token, &flight.id, Visibility::Public).await;
    // The API hands both on as the text they are; JSON has no markup.
    assert_eq!(
        flight.glider.as_deref(),
        Some("\"><img src=x onerror=alert('glider')><!--")
    );

    let answer = get(&router, &share_path(&flight.id)).await;

    assert_eq!(answer.status, StatusCode::OK);
    let html = answer.text();
    assert!(!html.contains("<script"), "{html}");
    assert!(!html.contains("<img"), "{html}");
    assert!(!html.contains("<!--"), "{html}");
    assert!(html.contains("&lt;script&gt;alert(&#39;name&#39;)&lt;/script&gt;"));
    assert!(html.contains("&quot;&gt;&lt;img src=x onerror=alert(&#39;glider&#39;)&gt;&lt;!--"));
    // And if a mistake ever let markup through, the page's own policy
    // would still keep it from running.
    assert!(
        answer
            .header(header::CONTENT_SECURITY_POLICY)
            .starts_with("default-src 'none'")
    );
    // The picture does not set either name.
    let picture = get(&router, &share_image_path(&flight.id)).await;
    assert_eq!(picture.status, StatusCode::OK);
}

#[tokio::test]
async fn links_on_the_page_come_from_the_configuration_not_from_the_request() {
    let router = recorded().await;
    let [public, _, _] = demo_flights(&router).await;
    let honest = get(&router, &share_path(&public)).await;

    // A request that claims to be for another site.
    let request = Request::builder()
        .uri(share_path(&public))
        .header(header::HOST, "evil.example")
        .header("x-forwarded-host", "evil.example")
        .header("x-forwarded-proto", "http")
        .header(header::REFERER, "https://evil.example/\"><script>")
        .header(header::USER_AGENT, "<script>alert(1)</script>")
        .body(Body::empty())
        .unwrap();
    let answer = send(&router, request).await;

    assert_eq!(answer.status, StatusCode::OK);
    assert!(!answer.text().contains("evil.example"));
    assert_eq!(answer.body, honest.body);
}
