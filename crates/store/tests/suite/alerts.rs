//! What every store must do with the alarms pilots set.

use venti_store::{
    AddingAlert, Alert, AlertCheck, DAY_ALERT, FiredAlerts, NewAlert, Store, WIND_ALERT,
};

use super::accounts::{create, id};
use super::at;

fn wind(name: &str, station: &str) -> NewAlert {
    NewAlert {
        id: id(name),
        name: format!("Alarm {name}"),
        enabled: true,
        kind: WIND_ALERT.to_owned(),
        station_id: Some(station.to_owned()),
        rule: format!(r#"{{"kind":"wind","station":"{station}"}}"#),
    }
}

fn day(name: &str) -> NewAlert {
    NewAlert {
        id: id(name),
        name: format!("Alarm {name}"),
        enabled: true,
        kind: DAY_ALERT.to_owned(),
        station_id: None,
        rule: r#"{"kind":"day"}"#.to_owned(),
    }
}

async fn add(store: &impl Store, account: &str, new: &NewAlert, minute: u32) -> Alert {
    match store
        .add_alert(account, new, at(10, minute), 3)
        .await
        .unwrap()
    {
        AddingAlert::Added(alert) => alert,
        other => panic!("expected the alarm to be added, got {other:?}"),
    }
}

fn names(alerts: &[Alert]) -> Vec<&str> {
    alerts.iter().map(|alert| alert.name.as_str()).collect()
}

pub(crate) async fn an_alarm_is_added_changed_and_removed(store: &impl Store) {
    let dario = create(store, "dario").await;
    let first = add(store, &dario.id, &wind("w1", "dwd-10min:1"), 0).await;
    assert_eq!(
        (first.created_at, first.changed_at, first.state.as_str()),
        (at(10, 0), at(10, 0), "")
    );
    add(store, &dario.id, &day("d1"), 1).await;
    add(store, &dario.id, &wind("w2", "dwd-10min:2"), 2).await;
    assert_eq!(
        names(&store.alerts(&dario.id).await.unwrap()),
        ["Alarm w1", "Alarm d1", "Alarm w2"]
    );
    // As many as the account may have.
    assert_eq!(
        store
            .add_alert(&dario.id, &wind("w3", "dwd-10min:3"), at(10, 3), 3)
            .await
            .unwrap(),
        AddingAlert::Full
    );
    assert_eq!(
        store
            .add_alert(&id("nobody"), &wind("w4", "dwd-10min:3"), at(10, 3), 3)
            .await
            .unwrap(),
        AddingAlert::NoAccount
    );

    // A change forgets what the server noted.
    let check = AlertCheck {
        account_id: dario.id.clone(),
        alert_id: first.id.clone(),
        changed_at: first.changed_at,
        state: r#"{"last":"x"}"#.to_owned(),
        fired: None,
    };
    assert!(store.note_alert_check(&check).await.unwrap());
    let mut changed = wind("w1", "dwd-10min:9");
    changed.name = "Renamed".to_owned();
    changed.enabled = false;
    let after = store
        .change_alert(&dario.id, &changed, at(11, 0))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            after.name.as_str(),
            after.enabled,
            after.station_id.as_deref(),
            after.state.as_str(),
            after.created_at,
            after.changed_at
        ),
        (
            "Renamed",
            false,
            Some("dwd-10min:9"),
            "",
            at(10, 0),
            at(11, 0)
        )
    );
    // A check of the alarm as it was before the change writes nothing.
    assert!(!store.note_alert_check(&check).await.unwrap());

    // Another account's alarm is not this one's to change or remove.
    let anna = create(store, "anna").await;
    assert_eq!(
        store
            .change_alert(&anna.id, &changed, at(11, 1))
            .await
            .unwrap(),
        None
    );
    assert!(!store.remove_alert(&anna.id, &first.id).await.unwrap());
    assert!(store.remove_alert(&dario.id, &first.id).await.unwrap());
    assert!(!store.remove_alert(&dario.id, &first.id).await.unwrap());
    assert_eq!(
        names(&store.alerts(&dario.id).await.unwrap()),
        ["Alarm d1", "Alarm w2"]
    );
}

pub(crate) async fn alarms_are_found_by_station_and_by_kind_when_on(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    add(store, &dario.id, &wind("w1", "dwd-10min:1"), 0).await;
    add(store, &anna.id, &wind("w2", "dwd-10min:1"), 1).await;
    add(store, &anna.id, &wind("w3", "dwd-10min:2"), 2).await;
    add(store, &anna.id, &day("d1"), 3).await;
    let mut off = wind("w2", "dwd-10min:1");
    off.enabled = false;
    store.change_alert(&anna.id, &off, at(11, 0)).await.unwrap();

    let at_one = store
        .wind_alerts_at(&["dwd-10min:1".to_owned(), "dwd-10min:7".to_owned()])
        .await
        .unwrap();
    assert_eq!(names(&at_one), ["Alarm w1"]);
    let both = store
        .wind_alerts_at(&["dwd-10min:1".to_owned(), "dwd-10min:2".to_owned()])
        .await
        .unwrap();
    // By account, then identifier.
    assert_eq!(names(&both), ["Alarm w3", "Alarm w1"]);
    let days = store.alerts_of_kind(DAY_ALERT).await.unwrap();
    assert_eq!(names(&days), ["Alarm d1"]);
}

pub(crate) async fn what_fired_is_numbered_kept_and_pruned(store: &impl Store) {
    let dario = create(store, "dario").await;
    let alarm = add(store, &dario.id, &wind("w1", "dwd-10min:1"), 0).await;
    let fire = |minute: u32, record: &str| AlertCheck {
        account_id: dario.id.clone(),
        alert_id: alarm.id.clone(),
        changed_at: alarm.changed_at,
        state: format!(r#"{{"minute":{minute}}}"#),
        fired: Some((at(12, minute), record.to_owned())),
    };
    assert_eq!(
        store.fired_alerts(&dario.id, 0, 10).await.unwrap(),
        FiredAlerts::default()
    );
    for (minute, record) in [(0, "a"), (0, "b"), (5, "c")] {
        assert!(store.note_alert_check(&fire(minute, record)).await.unwrap());
    }
    let all = store.fired_alerts(&dario.id, 0, 10).await.unwrap();
    let records: Vec<&str> = all.fired.iter().map(|f| f.record.as_str()).collect();
    assert_eq!(records, ["a", "b", "c"]);
    assert!(
        all.fired
            .windows(2)
            .all(|two| two[0].number < two[1].number)
    );
    assert_eq!(all.latest, all.fired[2].number);
    assert!(!all.more);
    assert_eq!(
        store.alerts(&dario.id).await.unwrap()[0].state,
        r#"{"minute":5}"#
    );

    let page = store.fired_alerts(&dario.id, 0, 2).await.unwrap();
    assert_eq!((page.fired.len(), page.more), (2, true));
    let rest = store
        .fired_alerts(&dario.id, page.fired[1].number, 2)
        .await
        .unwrap();
    assert_eq!(rest.fired, all.fired[2..]);

    // What fired stays when its alarm goes, until it is old.
    store.remove_alert(&dario.id, &alarm.id).await.unwrap();
    assert_eq!(
        store
            .fired_alerts(&dario.id, 0, 10)
            .await
            .unwrap()
            .fired
            .len(),
        3
    );
    assert_eq!(store.prune_fired_alerts(at(12, 1)).await.unwrap(), 2);
    let left = store.fired_alerts(&dario.id, 0, 10).await.unwrap();
    assert_eq!(left.fired, all.fired[2..]);
    assert_eq!(left.latest, all.latest);

    // And it goes with the account.
    store.delete_account(&dario.id).await.unwrap();
    assert_eq!(
        store.fired_alerts(&dario.id, 0, 10).await.unwrap(),
        FiredAlerts::default()
    );
    assert_eq!(store.alerts(&dario.id).await.unwrap(), []);
}
