//! What public flights say about where pilots take off, through the real
//! router: the flights that began at a site of any layer, and the places
//! where several pilots took off and no site stands.
//!
//! Over the development mode's recorded data, which holds the made-up
//! flights of `dev::add_demo_takeoffs`.

use axum::Router;
use axum::http::{Method, StatusCode, header};
use venti_api_types::v1::{
    AddSiteRequest, CANDIDATE_MIN_PILOTS, ErrorCode, FlightsResponse, MAX_CANDIDATE_BOX_DEG,
    PUBLIC_FLIGHTS_PATH, SITE_BEST_FLIGHTS, SITE_CANDIDATES_PATH, SITE_FLIGHTS_WITHIN_M,
    SignInResponse, SiteCandidatesResponse, SiteDraft, SiteFlightsResponse, SiteResponse,
    Visibility, site_flights_path, site_path,
};
use venti_domain::{GeoPoint, SiteId, SiteUse};
use venti_server::dev::{
    self, DEMO_CANDIDATE_BBOX, DEMO_PASSWORD, DEMO_SITES, DEMO_SITES_BBOX, DEMO_TAKEOFF_PILOTS,
};
use venti_sources::testing::assert_golden;

mod common;

use common::{ask, get, golden, recorded, set_visibility, try_sign_in};

fn site(id: &str) -> SiteId {
    id.parse().unwrap()
}

async fn flights_at(router: &Router, id: &str) -> SiteFlightsResponse {
    get(router, &site_flights_path(&site(id)))
        .await
        .ok(StatusCode::OK)
}

fn candidates_in(bbox: &str) -> String {
    format!("{SITE_CANDIDATES_PATH}?bbox={bbox}")
}

async fn candidates(router: &Router, bbox: &str) -> SiteCandidatesResponse {
    get(router, &candidates_in(bbox)).await.ok(StatusCode::OK)
}

async fn sign_in(router: &Router, handle: &str) -> String {
    let signed: SignInResponse = try_sign_in(router, handle, DEMO_PASSWORD)
        .await
        .ok(StatusCode::CREATED);
    signed.token
}

/// The origin of a page that is not Venti's own.
const ANOTHER_SITE: &str = "https://flights.example";

/// One of the two take-offs OpenStreetMap has beside the place the sample
/// flight begins at, 148 m from it. Its key holds a `/`.
const WALLBERG: &str = "osm-free-flying:node/5964676196";

#[tokio::test]
async fn a_site_says_what_the_public_flights_that_began_there_say() {
    let router = recorded().await;
    let [kogel, joch, wiese, removed] = DEMO_SITES;

    // The made-up take-off: three public flights of two pilots began
    // within the stated distance of it, in May and twice in July.
    let answer = ask(Method::GET, &site_flights_path(&site(kogel)))
        .header(header::ORIGIN, ANOTHER_SITE)
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::OK);
    // It names flights, so it is checked before every reuse, as the list
    // of public flights is; and it is open to every origin.
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, no-cache");
    assert!(answer.headers.contains_key(header::ETAG));
    assert_eq!(answer.header(header::ACCESS_CONTROL_ALLOW_ORIGIN), "*");
    let said: SiteFlightsResponse = answer.json();
    assert_golden(&golden("site_flights.json"), &said);
    assert_eq!(said.id, site(kogel));
    assert!(said.takeoff);
    #[allow(clippy::float_cmp)]
    {
        assert_eq!(said.within_m, SITE_FLIGHTS_WITHIN_M);
    }
    assert_eq!((said.flights, said.pilots), (3, 2));
    assert_eq!(said.latest_day.unwrap().to_string(), "2026-07-11");
    assert_eq!(said.by_month, [0, 0, 0, 0, 1, 0, 2, 0, 0, 0, 0, 0]);
    assert_eq!(said.by_month.iter().sum::<u32>(), said.flights);
    // The best by points, the best first: each is a public flight that
    // its own address shows, by the pilot the public list names.
    assert!(!said.best.is_empty() && said.best.len() <= SITE_BEST_FLIGHTS);
    let points: Vec<f64> = said.best.iter().map(|best| best.score.points).collect();
    assert!(
        points.windows(2).all(|pair| pair[0] >= pair[1]),
        "{points:?}"
    );
    let listed: FlightsResponse = get(&router, PUBLIC_FLIGHTS_PATH).await.ok(StatusCode::OK);
    for best in &said.best {
        let flight = listed
            .flights
            .iter()
            .find(|flight| flight.id == best.id)
            .expect("a best flight is in the list of public flights");
        assert_eq!(flight.pilot, best.pilot);
        assert_eq!(flight.score, Some(best.score));
        assert_eq!(flight.start.date_naive(), best.day);
    }
    let pilots: Vec<&str> = DEMO_TAKEOFF_PILOTS[..2].iter().map(|p| p.handle).collect();
    assert!(
        said.best
            .iter()
            .all(|best| pilots.contains(&best.pilot.handle.as_str()))
    );

    // A site of a source, whose key holds a slash: the five public
    // flights of the development mode itself began beside it, four in
    // August and one in September.
    let wallberg = flights_at(&router, WALLBERG).await;
    assert_eq!((wallberg.flights, wallberg.pilots), (5, 3));
    assert_eq!(wallberg.by_month, [0, 0, 0, 0, 0, 0, 0, 4, 1, 0, 0, 0]);
    assert_eq!(wallberg.latest_day.unwrap().to_string(), "2026-09-06");
    assert_eq!(wallberg.best.len(), SITE_BEST_FLIGHTS);

    // A take-off nobody has a public flight from, and a landing field:
    // of the second nothing is counted, and the answer says so.
    let quiet = flights_at(&router, joch).await;
    assert!(quiet.takeoff);
    assert_eq!(
        (quiet.flights, quiet.pilots, quiet.latest_day),
        (0, 0, None)
    );
    assert_eq!(quiet.by_month, [0; 12]);
    assert!(quiet.best.is_empty());
    let landing = flights_at(&router, wiese).await;
    assert!(!landing.takeoff);
    assert_eq!((landing.flights, landing.pilots), (0, 0));

    // A flight that its pilot makes private is in no count from then on.
    let lena = sign_in(&router, DEMO_TAKEOFF_PILOTS[0].handle).await;
    set_visibility(&router, &lena, &said.best[0].id, Visibility::Private).await;
    let after = flights_at(&router, kogel).await;
    assert_eq!(after.flights, 2);
    assert!(after.best.iter().all(|best| best.id != said.best[0].id));

    // A site that was removed is gone, and one that never was is not
    // found, of either kind of layer.
    get(&router, &site_flights_path(&site(removed)))
        .await
        .error(StatusCode::GONE, ErrorCode::SiteGone);
    for unknown in [
        "venti:0000000000000000000000zzzz",
        "osm-free-flying:node/1",
        "nobody:node/1",
    ] {
        get(&router, &site_flights_path(&site(unknown)))
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::SiteNotFound);
    }
    // The address is for reading.
    let written = ask(Method::DELETE, &site_flights_path(&site(kogel)))
        .token(dev::DEMO_TOKEN)
        .send(&router)
        .await;
    written.error(StatusCode::NOT_FOUND, ErrorCode::SiteNotFound);
    // And the site's own address is as it was.
    let read: SiteResponse = get(&router, &site_path(&site(WALLBERG)))
        .await
        .ok(StatusCode::OK);
    assert_eq!(read.site.id, site(WALLBERG));
}

#[tokio::test]
async fn a_place_pilots_take_off_at_that_no_site_names_is_offered() {
    let router = recorded().await;

    // Three pilots took off at a place where no site stands: it is
    // offered, with what their flights say and nothing of who they are.
    let answer = ask(Method::GET, &candidates_in(DEMO_CANDIDATE_BBOX))
        .header(header::ORIGIN, ANOTHER_SITE)
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=60");
    assert!(answer.headers.contains_key(header::ETAG));
    assert_eq!(answer.header(header::ACCESS_CONTROL_ALLOW_ORIGIN), "*");
    let said: SiteCandidatesResponse = answer.json();
    assert_golden(&golden("site_candidates.json"), &said);
    assert_eq!(said.min_pilots, CANDIDATE_MIN_PILOTS);
    assert!(!said.more);
    let [candidate] = &said.candidates[..] else {
        panic!("one candidate: {:?}", said.candidates);
    };
    assert_eq!(candidate.position, GeoPoint::new(47.2612, 10.9312).unwrap());
    // The private flight from there is in no count.
    assert_eq!((candidate.flights, candidate.pilots), (3, 3));
    assert_eq!(candidate.latest_day.to_string(), "2026-06-07");
    assert_eq!(candidate.by_month, [0, 0, 0, 1, 0, 2, 0, 0, 0, 0, 0, 0]);
    let text = answer.text();
    for pilot in DEMO_TAKEOFF_PILOTS {
        assert!(!text.contains(pilot.handle) && !text.contains(pilot.name));
    }
    // The whole box of the made-up sites holds the same one: at the
    // made-up take-off pilots took off too, and a site stands there. So
    // do the five flights of the development mode, beside OpenStreetMap's.
    assert_eq!(candidates(&router, DEMO_SITES_BBOX).await, said);
    let tegernsee = candidates(&router, "11.7,47.6,11.95,47.75").await;
    assert!(tegernsee.candidates.is_empty());

    // A pilot adds the site: the place is no candidate any more. The
    // server made no site of it by itself.
    let mara = sign_in(&router, "mara").await;
    let request = AddSiteRequest {
        site: SiteDraft {
            name: "Beispielhang".to_owned(),
            position: GeoPoint::new(47.2613, 10.9314).unwrap(),
            elevation_m: Some(1620.0),
            uses: vec![SiteUse::Takeoff],
            wind_directions: Vec::new(),
            paragliding: Some(true),
            hang_gliding: None,
            website: None,
            description: Some("Made up, for a test. No such place exists.".to_owned()),
            fee: None,
            guest_guidelines: None,
            no_flight_time: None,
            outline: None,
        },
        comment: None,
    };
    let added: SiteResponse = ask(Method::POST, "/v1/sites")
        .token(&mara)
        .json(&request)
        .send(&router)
        .await
        .ok(StatusCode::CREATED);
    assert_eq!(added.site.id.source().as_str(), "venti-pilots");
    assert!(
        candidates(&router, DEMO_CANDIDATE_BBOX)
            .await
            .candidates
            .is_empty()
    );
    // And the new site has the three flights.
    let at_it = flights_at(&router, &added.site.id.to_string()).await;
    assert_eq!((at_it.flights, at_it.pilots), (3, 3));
    // The site is removed again: the place is offered again.
    let removed = ask(Method::DELETE, &site_path(&added.site.id))
        .token(&mara)
        .send(&router)
        .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    assert_eq!(candidates(&router, DEMO_CANDIDATE_BBOX).await, said);

    // One of the three makes their flight private: two pilots are left,
    // and the place is not offered.
    let ines = sign_in(&router, DEMO_TAKEOFF_PILOTS[2].handle).await;
    let own: FlightsResponse = ask(Method::GET, "/v1/flights")
        .token(&ines)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    let public = own
        .flights
        .iter()
        .find(|flight| flight.visibility == Visibility::Public)
        .expect("a public flight");
    set_visibility(&router, &ines, &public.id, Visibility::Private).await;
    assert!(
        candidates(&router, DEMO_CANDIDATE_BBOX)
            .await
            .candidates
            .is_empty()
    );
    // Shared by link only is not public either; public again, it counts.
    set_visibility(&router, &ines, &public.id, Visibility::Unlisted).await;
    assert!(
        candidates(&router, DEMO_CANDIDATE_BBOX)
            .await
            .candidates
            .is_empty()
    );
    set_visibility(&router, &ines, &public.id, Visibility::Public).await;
    assert_eq!(candidates(&router, DEMO_CANDIDATE_BBOX).await, said);
}

#[tokio::test]
async fn candidates_are_asked_for_in_a_box_of_a_bounded_size() {
    // The development mode has one candidate. A box of the bound or
    // smaller that holds it has that one; one about the Tegernsee, where
    // its other public flights began beside a take-off of
    // OpenStreetMap's, has none.
    let router = recorded().await;
    let the_one = candidates(&router, DEMO_CANDIDATE_BBOX).await;
    assert_eq!(the_one.candidates.len(), 1);
    for bbox in [DEMO_SITES_BBOX, "10.5,46.5,12.5,48.5"] {
        assert_eq!(candidates(&router, bbox).await, the_one, "{bbox}");
    }
    let tegernsee = candidates(&router, "11.7,47.6,11.95,47.75").await;
    assert!(tegernsee.candidates.is_empty());

    get(&router, SITE_CANDIDATES_PATH)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidBbox);
    get(&router, &candidates_in("11.7,47.6,11.95"))
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidBbox);
    // A box wider or taller than the bound is refused whole.
    let wide = format!("10,47,{},47.5", 10.01 + MAX_CANDIDATE_BOX_DEG);
    let tall = format!("10,46,10.5,{}", 46.01 + MAX_CANDIDATE_BOX_DEG);
    for bbox in [wide.as_str(), tall.as_str(), "-180,-90,180,90"] {
        let message = get(&router, &candidates_in(bbox))
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::BoxTooLarge);
        assert!(message.contains("smaller boxes"), "{message}");
    }
    // The address is for reading.
    let written = ask(Method::POST, SITE_CANDIDATES_PATH).send(&router).await;
    assert_eq!(written.status, StatusCode::METHOD_NOT_ALLOWED);
}

/// The public list asked for by `site`: the flights that began at the
/// site, each with the site it took off from.
#[tokio::test]
async fn the_public_list_of_a_site_names_where_each_flight_took_off() {
    let router = recorded().await;
    let [kogel, _, wiese, _] = DEMO_SITES;
    let said = flights_at(&router, kogel).await;

    let path = format!("{PUBLIC_FLIGHTS_PATH}?site={kogel}");
    let listed: FlightsResponse = get(&router, &path).await.ok(StatusCode::OK);
    assert_eq!(listed.flights.len() as u32, said.flights);
    for flight in &listed.flights {
        let site = flight.takeoff_site.as_ref().unwrap();
        assert_eq!(site.id.to_string(), kogel);
    }
    let mut shown = listed.clone();
    shown.flights.truncate(1);
    for flight in &mut shown.flights {
        flight.outline.truncate(2);
    }
    assert_golden(&golden("public_flights_at_site.json"), &shown);

    // A landing field is no take-off: nothing began there.
    let path = format!("{PUBLIC_FLIGHTS_PATH}?site={wiese}");
    let none: FlightsResponse = get(&router, &path).await.ok(StatusCode::OK);
    assert!(none.flights.is_empty());

    // A site the server does not have.
    let path = format!("{PUBLIC_FLIGHTS_PATH}?site=venti:0000000000000000000000s999");
    let answer = get(&router, &path).await;
    assert_eq!(answer.status, StatusCode::NOT_FOUND);

    // Everyone's list names the site of those flights too, and of no
    // flight that began far from every site.
    let all: FlightsResponse = get(&router, PUBLIC_FLIGHTS_PATH).await.ok(StatusCode::OK);
    let named = all
        .flights
        .iter()
        .filter(|flight| flight.takeoff_site.is_some())
        .count();
    assert_eq!(named as u32, said.flights);
}
