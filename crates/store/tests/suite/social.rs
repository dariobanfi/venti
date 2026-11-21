//! What every store must do with follows, cheers, comments and notices.

use venti_domain::Timestamp;
use venti_store::{
    Account, Cheering, Commenting, FlightSocial, FollowCounts, FollowSide, Following, NewComment,
    NewReport, ReportListing, ReportSaved, ReportSubject, SocialPilot, Store, Visibility,
};

use super::accounts::{create, id};
use super::at;
use super::flights::{new_flight, put};

async fn public_flight(store: &impl Store, pilot: &Account, n: u8, start: Timestamp) -> String {
    let new = new_flight(pilot, n, start);
    put(store, &new).await;
    store
        .set_visibility(&pilot.id, &new.id, Visibility::Public, at(21, 0))
        .await
        .unwrap()
        .flight()
        .expect("the flight was just stored");
    new.id
}

async fn follow(store: &impl Store, one: &Account, other: &Account, minute: u32) -> Following {
    store
        .follow(&one.id, &other.id, at(12, minute), 2)
        .await
        .unwrap()
}

fn handles(pilots: &[SocialPilot]) -> Vec<&str> {
    pilots.iter().map(|pilot| pilot.handle.as_str()).collect()
}

fn comment(name: &str, flight: &str, author: &Account, minute: u32) -> NewComment {
    NewComment {
        id: id(name),
        flight_id: flight.to_owned(),
        author_id: author.id.clone(),
        text: format!("Comment {name}"),
        created_at: at(13, minute),
    }
}

pub(crate) async fn pilots_follow_each_other(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    let carla = create(store, "carla").await;
    // Anna has a page; Bernd shares nothing.
    public_flight(store, &anna, 1, at(10, 0)).await;

    assert_eq!(
        follow(store, &dario, &anna, 0).await,
        Following::Follows { new: true }
    );
    assert_eq!(
        follow(store, &dario, &anna, 5).await,
        Following::Follows { new: false }
    );
    assert_eq!(
        follow(store, &dario, &bernd, 1).await,
        Following::Follows { new: true }
    );
    // As many as the account may follow, nobody twice, and not oneself.
    assert_eq!(follow(store, &dario, &carla, 2).await, Following::Full);
    assert_eq!(follow(store, &dario, &dario, 2).await, Following::NoAccount);
    let nobody = store
        .follow(&dario.id, &id("nobody"), at(12, 2), 2)
        .await
        .unwrap();
    assert_eq!(nobody, Following::NoAccount);
    assert_eq!(
        follow(store, &bernd, &anna, 3).await,
        Following::Follows { new: true }
    );

    assert_eq!(
        store.follow_counts(&anna.id).await.unwrap(),
        FollowCounts {
            followers: 2,
            following: 0
        }
    );
    assert_eq!(
        store.follow_counts(&dario.id).await.unwrap(),
        FollowCounts {
            followers: 0,
            following: 2
        }
    );
    assert!(store.follows_pilot(&dario.id, &anna.id).await.unwrap());
    assert!(!store.follows_pilot(&anna.id, &dario.id).await.unwrap());

    // The lists, the newest first, page by page.
    let following = store
        .follows(&dario.id, FollowSide::Following, false, None, 10)
        .await
        .unwrap();
    assert_eq!(handles(&following), ["bernd", "anna"]);
    assert_eq!(following[1].at, at(12, 0));
    assert_eq!(following[1].name, "Pilot anna");
    let first = store
        .follows(&anna.id, FollowSide::Followers, false, None, 1)
        .await
        .unwrap();
    assert_eq!(handles(&first), ["bernd"]);
    let second = store
        .follows(
            &anna.id,
            FollowSide::Followers,
            false,
            Some(&first[0].key()),
            1,
        )
        .await
        .unwrap();
    assert_eq!(handles(&second), ["dario"]);
    // Only those with a public flight, where that is asked for.
    let with_a_page = store
        .follows(&dario.id, FollowSide::Following, true, None, 10)
        .await
        .unwrap();
    assert_eq!(handles(&with_a_page), ["anna"]);

    assert!(store.unfollow(&dario.id, &bernd.id).await.unwrap());
    assert!(!store.unfollow(&dario.id, &bernd.id).await.unwrap());
    assert_eq!(
        follow(store, &dario, &carla, 4).await,
        Following::Follows { new: true }
    );
}

pub(crate) async fn the_feed_is_what_followed_pilots_share(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    let carla = create(store, "carla").await;
    let early = public_flight(store, &anna, 1, at(9, 0)).await;
    let late = public_flight(store, &bernd, 2, at(11, 0)).await;
    let middle = public_flight(store, &anna, 3, at(10, 0)).await;
    // Private, and of a pilot Dario does not follow.
    put(store, &new_flight(&anna, 4, at(12, 0))).await;
    public_flight(store, &carla, 5, at(13, 0)).await;
    public_flight(store, &dario, 6, at(14, 0)).await;

    let feed = |after, limit| {
        let dario = dario.id.clone();
        async move {
            let page = store.followed_flights(&dario, after, limit).await.unwrap();
            page.iter()
                .map(|flight| flight.id.clone())
                .collect::<Vec<_>>()
        }
    };
    assert_eq!(feed(None, 10).await, Vec::<String>::new());
    follow(store, &dario, &anna, 0).await;
    follow(store, &dario, &bernd, 1).await;
    assert_eq!(
        feed(None, 10).await,
        [late.clone(), middle.clone(), early.clone()]
    );
    let first = store.followed_flights(&dario.id, None, 2).await.unwrap();
    assert_eq!(first.len(), 2);
    assert_eq!(first[0].pilot_handle, "bernd");
    let key = first[1].key();
    assert_eq!(feed(Some(&key), 2).await, std::slice::from_ref(&early));

    // A flight made private leaves it, and so do a pilot's flights when
    // they are followed no more.
    store
        .set_visibility(&anna.id, &middle, Visibility::Unlisted, at(22, 0))
        .await
        .unwrap();
    assert_eq!(feed(None, 10).await, [late, early.clone()]);
    store.unfollow(&dario.id, &bernd.id).await.unwrap();
    assert_eq!(feed(None, 10).await, [early]);
}

pub(crate) async fn a_flight_is_cheered_once_per_pilot(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    let flight = public_flight(store, &anna, 1, at(10, 0)).await;
    let private = new_flight(&anna, 2, at(11, 0));
    put(store, &private).await;

    let cheer = |who: &Account, flight: &str, minute| {
        let (who, flight) = (who.id.clone(), flight.to_owned());
        async move { store.cheer(&who, &flight, at(12, minute)).await.unwrap() }
    };
    assert_eq!(
        cheer(&dario, &flight, 0).await,
        Cheering::Cheers { new: true }
    );
    assert_eq!(
        cheer(&dario, &flight, 5).await,
        Cheering::Cheers { new: false }
    );
    assert_eq!(
        cheer(&bernd, &flight, 1).await,
        Cheering::Cheers { new: true }
    );
    // Only a public flight is cheered, and only by an account.
    assert_eq!(cheer(&dario, &private.id, 2).await, Cheering::NoSuchFlight);
    assert_eq!(cheer(&dario, &id("none"), 2).await, Cheering::NoSuchFlight);
    let nobody = store.cheer(&id("nobody"), &flight, at(12, 2)).await;
    assert_eq!(nobody.unwrap(), Cheering::NoSuchFlight);

    let cheers = store.cheers(&flight, None, 10).await.unwrap();
    assert_eq!(handles(&cheers), ["bernd", "dario"]);
    assert_eq!(cheers[1].at, at(12, 0));
    let next = store.cheers(&flight, Some(&cheers[0].key()), 10).await;
    assert_eq!(handles(&next.unwrap()), ["dario"]);

    let asked = [flight.clone(), private.id.clone()];
    let social = store.flight_social(&asked, Some(&dario.id)).await.unwrap();
    assert_eq!(
        social,
        [
            FlightSocial {
                flight_id: flight.clone(),
                cheers: 2,
                comments: 0,
                cheered: true,
                comments_closed: false,
            },
            FlightSocial {
                flight_id: private.id.clone(),
                ..FlightSocial::default()
            },
        ]
    );
    let for_anna = store.flight_social(&asked[..1], Some(&anna.id)).await;
    assert!(!for_anna.unwrap()[0].cheered);
    let for_nobody = store.flight_social(&asked[..1], None).await.unwrap();
    assert_eq!((for_nobody[0].cheers, for_nobody[0].cheered), (2, false));

    // A flight that is public no more keeps its cheers, and takes none.
    store
        .set_visibility(&anna.id, &flight, Visibility::Private, at(22, 0))
        .await
        .unwrap();
    assert_eq!(cheer(&anna, &flight, 3).await, Cheering::NoSuchFlight);
    assert_eq!(store.cheers(&flight, None, 10).await.unwrap().len(), 2);
    assert!(store.uncheer(&bernd.id, &flight).await.unwrap());
    assert!(!store.uncheer(&bernd.id, &flight).await.unwrap());
    assert_eq!(store.cheers(&flight, None, 10).await.unwrap().len(), 1);
}

pub(crate) async fn comments_are_kept_listed_and_removed(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let flight = public_flight(store, &anna, 1, at(10, 0)).await;
    let private = new_flight(&anna, 2, at(11, 0));
    put(store, &private).await;

    let first = comment("c1", &flight, &dario, 0);
    let Commenting::Added(stored) = store.add_comment(&first).await.unwrap() else {
        panic!("expected the comment to be stored");
    };
    assert_eq!(
        (
            stored.id.as_str(),
            stored.author_handle.as_str(),
            stored.author_name.as_str(),
            stored.text.as_str(),
            stored.created_at
        ),
        (
            id("c1").as_str(),
            "dario",
            "Pilot dario",
            "Comment c1",
            at(13, 0)
        )
    );
    for (name, author, minute) in [("c2", &anna, 1), ("c3", &dario, 2)] {
        let added = store
            .add_comment(&comment(name, &flight, author, minute))
            .await
            .unwrap();
        assert!(matches!(added, Commenting::Added(_)), "{added:?}");
    }
    // Only on a public flight, and only by an account.
    for bad in [
        comment("c4", &private.id, &dario, 3),
        comment("c5", &id("none"), &dario, 3),
        NewComment {
            author_id: id("nobody"),
            ..comment("c6", &flight, &dario, 3)
        },
    ] {
        assert_eq!(
            store.add_comment(&bad).await.unwrap(),
            Commenting::NoSuchFlight
        );
    }

    // The oldest first, page by page.
    let page = store.comments(&flight, None, 2).await.unwrap();
    let texts = |comments: &[venti_store::Comment]| {
        comments
            .iter()
            .map(|comment| comment.text.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(texts(&page), ["Comment c1", "Comment c2"]);
    let rest = store
        .comments(&flight, Some(&page[1].key()), 2)
        .await
        .unwrap();
    assert_eq!(texts(&rest), ["Comment c3"]);
    assert_eq!(
        store.comment(&id("c2")).await.unwrap().unwrap().author_id,
        anna.id
    );
    assert_eq!(store.comment(&id("c9")).await.unwrap(), None);
    let social = store
        .flight_social(std::slice::from_ref(&flight), None)
        .await
        .unwrap();
    assert_eq!((social[0].comments, social[0].comments_closed), (3, false));

    // The flight's pilot takes no more; what is there stays.
    assert!(
        !store
            .close_comments(&dario.id, &flight, true, at(14, 0))
            .await
            .unwrap()
    );
    assert!(
        store
            .close_comments(&anna.id, &flight, true, at(14, 0))
            .await
            .unwrap()
    );
    assert!(
        store
            .close_comments(&anna.id, &flight, true, at(14, 1))
            .await
            .unwrap()
    );
    assert_eq!(
        store
            .add_comment(&comment("c7", &flight, &dario, 4))
            .await
            .unwrap(),
        Commenting::Closed
    );
    let social = store
        .flight_social(std::slice::from_ref(&flight), None)
        .await
        .unwrap();
    assert_eq!((social[0].comments, social[0].comments_closed), (3, true));
    assert!(
        store
            .close_comments(&anna.id, &flight, false, at(14, 2))
            .await
            .unwrap()
    );
    let added = store.add_comment(&comment("c7", &flight, &dario, 4)).await;
    assert!(matches!(added.unwrap(), Commenting::Added(_)));

    // Removed, a comment is gone, and what it was is handed back once.
    let removed = store.remove_comment(&id("c2")).await.unwrap().unwrap();
    assert_eq!(removed.text, "Comment c2");
    assert_eq!(store.remove_comment(&id("c2")).await.unwrap(), None);
    let left = store.comments(&flight, None, 10).await.unwrap();
    assert_eq!(texts(&left), ["Comment c1", "Comment c3", "Comment c7"]);

    // What an account gave, for its archive.
    store.cheer(&dario.id, &flight, at(15, 0)).await.unwrap();
    let given = store.social_given(&dario.id).await.unwrap();
    assert_eq!(given.cheers, [(flight.clone(), at(15, 0))]);
    assert_eq!(
        texts(&given.comments),
        ["Comment c1", "Comment c3", "Comment c7"]
    );
    assert_eq!(
        store.social_given(&anna.id).await.unwrap(),
        venti_store::SocialGiven::default()
    );
}

pub(crate) async fn notices_are_numbered_kept_and_pruned(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let micros = |time: Timestamp| time.timestamp_micros();
    assert_eq!(
        store.notices(&dario.id, 0, 10).await.unwrap(),
        venti_store::Notices::default()
    );
    for (minute, record) in [(0, "one"), (0, "two"), (5, "three")] {
        assert!(
            store
                .add_notice(&dario.id, &anna.id, at(12, minute), record)
                .await
                .unwrap()
        );
    }
    // Not for an account that is not there, nor from one.
    for (to, from) in [(id("nobody"), anna.id.clone()), (dario.id.clone(), id("x"))] {
        assert!(!store.add_notice(&to, &from, at(12, 6), "no").await.unwrap());
    }

    let all = store.notices(&dario.id, 0, 10).await.unwrap();
    let numbers: Vec<i64> = all.notices.iter().map(|notice| notice.number).collect();
    assert_eq!(
        numbers,
        [micros(at(12, 0)), micros(at(12, 0)) + 1, micros(at(12, 5))]
    );
    assert_eq!(all.latest, micros(at(12, 5)));
    assert!(!all.more);
    assert_eq!(all.notices[1].record, "two");
    assert_eq!(all.notices[1].at, at(12, 0));
    let first = store.notices(&dario.id, 0, 2).await.unwrap();
    assert!(first.more);
    let rest = store.notices(&dario.id, numbers[1], 2).await.unwrap();
    assert_eq!(rest.notices.len(), 1);
    assert_eq!(rest.notices[0].record, "three");
    assert_eq!(store.notices(&anna.id, 0, 10).await.unwrap().notices, []);

    assert_eq!(store.prune_notices(at(12, 3)).await.unwrap(), 2);
    let left = store.notices(&dario.id, 0, 10).await.unwrap();
    assert_eq!(left.notices.len(), 1);
    assert_eq!(left.latest, micros(at(12, 5)));
}

pub(crate) async fn what_pilots_gave_goes_with_them(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    let annas = public_flight(store, &anna, 1, at(10, 0)).await;
    let darios = public_flight(store, &dario, 2, at(11, 0)).await;
    let gone = public_flight(store, &anna, 3, at(9, 0)).await;

    follow(store, &dario, &anna, 0).await;
    follow(store, &anna, &dario, 1).await;
    follow(store, &bernd, &dario, 2).await;
    for (who, flight) in [
        (&dario, &annas),
        (&bernd, &annas),
        (&anna, &darios),
        (&bernd, &gone),
    ] {
        store.cheer(&who.id, flight, at(12, 0)).await.unwrap();
    }
    for new in [
        comment("c1", &annas, &dario, 0),
        comment("c2", &annas, &bernd, 1),
        comment("c3", &darios, &anna, 2),
        comment("c4", &gone, &bernd, 3),
    ] {
        store.add_comment(&new).await.unwrap();
    }
    store
        .add_notice(&anna.id, &dario.id, at(12, 0), "from dario")
        .await
        .unwrap();
    store
        .add_notice(&anna.id, &bernd.id, at(12, 1), "from bernd")
        .await
        .unwrap();
    store
        .add_notice(&dario.id, &anna.id, at(12, 2), "from anna")
        .await
        .unwrap();

    // A flight that is deleted takes its cheers and comments along, and
    // they are not back with the flight.
    assert!(
        store
            .delete_flight(&anna.id, &gone, at(15, 0))
            .await
            .unwrap()
    );
    let again = public_flight(store, &anna, 3, at(9, 0)).await;
    assert_eq!(again, gone);
    let social = store
        .flight_social(std::slice::from_ref(&gone), Some(&bernd.id))
        .await
        .unwrap();
    assert_eq!(
        social,
        [FlightSocial {
            flight_id: gone.clone(),
            ..FlightSocial::default()
        }]
    );
    assert_eq!(store.comment(&id("c4")).await.unwrap(), None);

    // An account that is deleted takes what it gave, what it was told
    // and what it caused others to be told.
    store.delete_account(&dario.id).await.unwrap();
    assert_eq!(
        store.follow_counts(&anna.id).await.unwrap(),
        FollowCounts::default()
    );
    assert_eq!(
        store
            .follows(&bernd.id, FollowSide::Following, false, None, 10)
            .await
            .unwrap(),
        []
    );
    let social = store
        .flight_social(std::slice::from_ref(&annas), None)
        .await
        .unwrap();
    assert_eq!((social[0].cheers, social[0].comments), (1, 1));
    let left = store.comments(&annas, None, 10).await.unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].author_handle, "bernd");
    assert_eq!(store.comment(&id("c3")).await.unwrap(), None);
    let told = store.notices(&anna.id, 0, 10).await.unwrap();
    let records: Vec<&str> = told.notices.iter().map(|n| n.record.as_str()).collect();
    assert_eq!(records, ["from bernd"]);
    assert_eq!(store.notices(&dario.id, 0, 10).await.unwrap().notices, []);
    assert_eq!(
        store.social_given(&dario.id).await.unwrap(),
        venti_store::SocialGiven::default()
    );
}

pub(crate) async fn a_comment_is_reported_with_its_words(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let subject = ReportSubject::Comment {
        id: id("c1"),
        flight_id: id("f1"),
        text: "Comment as it stood".to_owned(),
    };
    let new = NewReport {
        id: id("r1"),
        created_at: at(16, 0),
        reporter_id: dario.id.clone(),
        about_account_id: Some(anna.id.clone()),
        subject: subject.clone(),
        reason: "offensive".to_owned(),
        text: None,
    };
    assert_eq!(store.save_report(&new).await.unwrap(), ReportSaved::Stored);
    // One open report per reporter and comment.
    let again = NewReport {
        id: id("r2"),
        ..new.clone()
    };
    assert_eq!(
        store.save_report(&again).await.unwrap(),
        ReportSaved::AlreadyOpen
    );
    let reports = store.reports(ReportListing::Open, None, 10).await.unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].subject, subject);
    assert_eq!(
        reports[0].about.as_ref().map(|about| about.handle.as_str()),
        Some("anna")
    );
}
