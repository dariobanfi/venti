//! What every store must do with the browsers subscribed to Web Push, and
//! with the server's key for it.

use venti_store::{AddingPush, PushSubscription, Store};

use super::accounts::{create, id};
use super::at;

fn browser(account_id: &str, n: u32, minute: u32) -> PushSubscription {
    PushSubscription {
        account_id: account_id.to_owned(),
        endpoint: format!("https://push.example/{n}"),
        p256dh: format!("key{n}"),
        auth: format!("auth{n}"),
        created_at: at(10, minute),
    }
}

fn endpoints(subscriptions: &[PushSubscription]) -> Vec<&str> {
    subscriptions
        .iter()
        .map(|subscription| subscription.endpoint.as_str())
        .collect()
}

pub(crate) async fn a_browser_is_subscribed_bounded_and_removed(store: &impl Store) {
    let dario = create(store, "dario").await;
    for n in 1..=2 {
        assert_eq!(
            store
                .add_push_subscription(&browser(&dario.id, n, n), 2)
                .await
                .unwrap(),
            AddingPush::Added
        );
    }
    let held = store.push_subscriptions(&dario.id).await.unwrap();
    assert_eq!(held, [browser(&dario.id, 1, 1), browser(&dario.id, 2, 2)]);
    // As many as the account may have; the same browser again is not
    // another, and takes its new keys.
    assert_eq!(
        store
            .add_push_subscription(&browser(&dario.id, 3, 3), 2)
            .await
            .unwrap(),
        AddingPush::Full
    );
    let mut again = browser(&dario.id, 1, 4);
    again.auth = "new".to_owned();
    assert_eq!(
        store.add_push_subscription(&again, 2).await.unwrap(),
        AddingPush::Added
    );
    assert_eq!(
        store.push_subscriptions(&dario.id).await.unwrap(),
        [browser(&dario.id, 2, 2), again]
    );
    assert_eq!(
        store
            .add_push_subscription(&browser(&id("nobody"), 5, 5), 2)
            .await
            .unwrap(),
        AddingPush::NoAccount
    );

    // Another account cannot remove it; its own can.
    let anna = create(store, "anna").await;
    let first = "https://push.example/1";
    assert!(
        !store
            .remove_push_subscription(&anna.id, first)
            .await
            .unwrap()
    );
    assert!(
        store
            .remove_push_subscription(&dario.id, first)
            .await
            .unwrap()
    );
    assert_eq!(
        endpoints(&store.push_subscriptions(&dario.id).await.unwrap()),
        ["https://push.example/2"]
    );
}

pub(crate) async fn a_browser_is_of_the_account_that_subscribed_it_last(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    store
        .add_push_subscription(&browser(&dario.id, 1, 1), 10)
        .await
        .unwrap();
    store
        .add_push_subscription(&browser(&anna.id, 1, 2), 10)
        .await
        .unwrap();
    assert_eq!(store.push_subscriptions(&dario.id).await.unwrap(), []);
    assert_eq!(
        endpoints(&store.push_subscriptions(&anna.id).await.unwrap()),
        ["https://push.example/1"]
    );

    // Its push service said it is gone.
    assert!(
        store
            .drop_push_subscription("https://push.example/1")
            .await
            .unwrap()
    );
    assert!(
        !store
            .drop_push_subscription("https://push.example/1")
            .await
            .unwrap()
    );

    // The account's deletion takes its browsers with it.
    store
        .add_push_subscription(&browser(&anna.id, 2, 3), 10)
        .await
        .unwrap();
    store.delete_account(&anna.id).await.unwrap();
    assert_eq!(store.push_subscriptions(&anna.id).await.unwrap(), []);
    assert!(
        !store
            .drop_push_subscription("https://push.example/2")
            .await
            .unwrap()
    );
}

pub(crate) async fn the_first_push_key_kept_is_the_one(store: &impl Store) {
    assert_eq!(store.push_key().await.unwrap(), None);
    assert_eq!(
        store.keep_push_key("first", at(10, 0)).await.unwrap(),
        "first"
    );
    assert_eq!(
        store.keep_push_key("second", at(10, 1)).await.unwrap(),
        "first"
    );
    assert_eq!(store.push_key().await.unwrap().as_deref(), Some("first"));
}
