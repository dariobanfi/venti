//! The `invite` subcommand: makes invite codes.
//!
//! While registration is by invite, an account can only be created with a
//! code made here. A code creates one account. The server keeps a hash of
//! it, so a code is shown once, when it is made: give it to the person it
//! is for, and make another if it gets lost.

use std::path::PathBuf;
use std::process::ExitCode;

use chrono::Duration;
use venti_store::{NewInvite, PgStore, Store};

use super::secret::InviteCode;
use crate::clock::{Clock, SystemClock};
use crate::config::Config;

/// The most codes one run makes. More than a beta needs at a time, and few
/// enough that a typing mistake does not fill the table.
pub const MAX_COUNT: u32 = 100;

/// What `venti-server invite` was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// The server's configuration file, if one was named. The database is
    /// taken from it or from the environment, as for `serve`.
    pub config: Option<PathBuf>,
    /// How many codes to make.
    pub count: u32,
    /// After how many days the codes stop working. `None` never.
    pub expires_in_days: Option<u32>,
}

/// Makes `count` invite codes that are good from now on, and until
/// `expires_in_days` from now if given.
pub async fn make(
    store: &impl Store,
    clock: &dyn Clock,
    count: u32,
    expires_in_days: Option<u32>,
) -> Result<Vec<InviteCode>, String> {
    let created_at = clock.now();
    let expires_at = expires_in_days.map(|days| created_at + Duration::days(i64::from(days)));
    let mut codes = Vec::new();
    for _ in 0..count {
        let code = InviteCode::new().map_err(|e| e.to_string())?;
        let invite = NewInvite {
            code: code.hash(),
            created_at,
            expires_at,
        };
        store
            .create_invite(&invite)
            .await
            .map_err(|e| format!("the invite could not be stored: {e}"))?;
        codes.push(code);
    }
    Ok(codes)
}

/// Runs the subcommand: prints one code per line on standard output.
pub async fn run(args: Args) -> ExitCode {
    let config = match Config::load(args.config.as_deref()) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let store = match PgStore::connect(config.database_url.expose()).await {
        Ok(store) => store,
        Err(error) => {
            eprintln!("cannot open the database: {error}");
            return ExitCode::FAILURE;
        }
    };
    match make(&store, &SystemClock, args.count, args.expires_in_days).await {
        Ok(codes) => {
            for code in &codes {
                println!("{code}");
            }
            let life = match args.expires_in_days {
                Some(days) => format!("good for {days} days"),
                None => "good until used".to_owned(),
            };
            eprintln!(
                "{} invite code(s), each for one account, {life}. \
                 They are not shown again.",
                codes.len()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use venti_store::{Account, AccountCreation, MemoryStore, NewAccount};

    use super::*;
    use crate::clock::FixedClock;

    fn account(
        handle: &str,
        invite: &InviteCode,
        created_at: venti_domain::Timestamp,
    ) -> NewAccount {
        NewAccount {
            account: Account {
                id: format!("{handle:0>26}"),
                handle: handle.to_owned(),
                display_name: handle.to_owned(),
                created_at,
                publishing_suspended_at: None,
                edits_sites_since: None,
                new_flights_public: false,
                country: None,
                glider_class: None,
            },
            password_hash: "$argon2id$x".to_owned(),
            invite: Some(invite.hash()),
        }
    }

    #[tokio::test]
    async fn each_code_made_creates_one_account_until_it_expires() {
        let made_at = Utc.with_ymd_and_hms(2026, 11, 19, 12, 0, 0).unwrap();
        let store = MemoryStore::new();
        let clock = FixedClock(made_at);

        let codes = make(&store, &clock, 3, Some(7)).await.unwrap();

        assert_eq!(codes.len(), 3);
        assert_ne!(codes[0], codes[1]);
        let create = |handle: &'static str, code: usize, days: i64| {
            let new = account(handle, &codes[code], made_at + Duration::days(days));
            let store = &store;
            async move { store.create_account(&new).await.unwrap() }
        };
        assert_eq!(create("anna", 0, 6).await, AccountCreation::Created);
        assert_eq!(create("bernd", 0, 6).await, AccountCreation::InviteRefused);
        assert_eq!(create("bernd", 1, 7).await, AccountCreation::InviteRefused);
        assert_eq!(create("bernd", 2, 0).await, AccountCreation::Created);

        // Without an expiry a code waits for as long as it takes.
        let patient = make(&store, &clock, 1, None).await.unwrap();
        let later = account("carla", &patient[0], made_at + Duration::days(3650));
        assert_eq!(
            store.create_account(&later).await.unwrap(),
            AccountCreation::Created
        );
    }
}
