//! The server's configuration.
//!
//! Settings come from a TOML file and from environment variables; a variable
//! wins over the file. Either alone is enough: a developer keeps a file, the
//! container deployment sets variables. `config.example.toml` beside this
//! crate's `Cargo.toml` documents every setting.
//!
//! The database URL holds a password. It is never printed, and it belongs in
//! the environment or in a file outside the repository.

use std::fmt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tracing::Level;
use venti_store::Room;

use crate::accounts::Registration;
use crate::accounts::client::TrustedProxies;
use crate::adapters::{self, Providers};
use crate::flights::share::PublicUrl;
use crate::groups::DEFAULT_PLAN_DAYS;
use crate::imagery::DEFAULT_CACHE_MB;
use crate::live::DEFAULT_RETENTION_HOURS;

/// Everything the server needs to be told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The address the HTTP API listens on.
    pub listen: SocketAddr,
    /// Where the database is.
    pub database_url: DatabaseUrl,
    /// The region packs to collect and serve, as paths to `region.toml` files.
    pub regions: Vec<PathBuf>,
    /// The folder raw provider payloads are archived in. `None` keeps none.
    pub archive_dir: Option<PathBuf>,
    /// The folder of the map regions the server hands out, one folder with
    /// a `map.toml` for each; see [`crate::maps`]. `None` hands out none.
    pub maps_dir: Option<PathBuf>,
    /// The folder tiles of map imagery are kept in; see
    /// [`crate::imagery`]. `None` passes no imagery on.
    pub imagery_dir: Option<PathBuf>,
    /// The most megabytes that folder is to hold.
    pub imagery_cache_mb: u32,
    /// The `User-Agent` sent to data providers.
    pub user_agent: String,
    /// Web origins that may call the API from a browser.
    pub cors_origins: Vec<String>,
    /// How much the server logs.
    pub log_level: Level,
    /// How long data is kept.
    pub retention: Retention,
    /// Who may create an account.
    pub registration: Registration,
    /// The address the server is reached at from outside, which links in
    /// share pages start with. `None` if it was not configured.
    pub public_url: Option<PublicUrl>,
    /// The reverse proxies whose `X-Forwarded-For` is believed.
    pub trusted_proxies: TrustedProxies,
    /// Where the providers are that have no address of their own.
    pub providers: Providers,
    /// The most one account may keep.
    pub room: Room,
    /// Whether the server takes flying sites from pilots; see
    /// [`crate::pilot_sites`]. Off unless it is set: the terms under which
    /// a pilot gives a site are not written yet, and until they are only
    /// a server whose pilots are its owner's own turns this on.
    pub sites_from_pilots: bool,
    /// The OAuth client IDs of Venti's apps at Google (web, Android,
    /// iPhone), for signing in with Google. Empty: the server does not
    /// offer it. Client IDs are no secret.
    pub google_client_ids: Vec<String>,
    /// The keys for push to the phone apps, as files. Without them the
    /// server tells no phone anything.
    pub push: PushConfig,
}

/// The files that let the server push to the phone apps. A secret each:
/// they are named here and read when the server starts, and never
/// printed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PushConfig {
    /// Apple's push service, for the iPhone app.
    pub apns: Option<ApnsConfig>,
    /// The JSON file of the Firebase project's service account, for the
    /// Android app.
    pub fcm_service_account_file: Option<PathBuf>,
}

/// The APNs key, as Apple's developer site hands it out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApnsConfig {
    /// The `.p8` file.
    pub key_file: PathBuf,
    /// The key's ID, ten characters.
    pub key_id: String,
    /// The team's ID, ten characters.
    pub team_id: String,
    /// The app's bundle ID, which APNs calls the topic.
    pub topic: String,
}

/// The bundle ID of the iPhone app (`apps/ios/Config/Base.xcconfig`): the
/// topic of APNs unless the configuration names another.
pub const DEFAULT_APNS_TOPIC: &str = "fly.venti.app";

/// The most flights an account may keep unless the configuration says
/// otherwise, and the most megabytes of flight files.
///
/// Meant never to be met by a pilot. Twenty years of flying a great deal:
/// someone who flies tandems for a living makes a thousand short flights a
/// year, 20,000 files of some 50 kB, a gigabyte; someone who flies 150
/// long cross-country flights a year, four hours each at a fix a second
/// (150 kB an hour), fills two. An account that reaches these numbers is
/// not keeping a logbook.
pub const DEFAULT_ACCOUNT_FLIGHTS: u32 = 25_000;
/// See [`DEFAULT_ACCOUNT_FLIGHTS`].
pub const DEFAULT_ACCOUNT_STORAGE_MB: u32 = 4096;

/// How long data is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retention {
    /// Observations are kept for this many whole calendar months before the
    /// current one.
    pub observation_months: u32,
    /// Raw provider payloads are kept for this many days.
    pub archive_days: u32,
    /// A live track is kept for this many hours after its newest fix.
    pub live_hours: u32,
    /// A report is kept for this many days after it was closed. An open
    /// report is kept until it is closed.
    pub report_days: u32,
    /// A plan that the members of a group made for a flying day is kept
    /// for this many days after that day.
    pub plan_days: u32,
}

/// For how many days a closed report is kept unless the configuration says
/// otherwise.
///
/// Long enough to see that the same thing is reported again, or that the
/// same account keeps being reported, and to answer someone who asks what
/// became of their report. Not for ever: a report names two people and
/// what one of them thought of the other.
pub const DEFAULT_REPORT_DAYS: u32 = 90;

/// A database URL. It contains a password, so it does not print itself.
#[derive(Clone, PartialEq, Eq)]
pub struct DatabaseUrl(String);

impl DatabaseUrl {
    /// The URL, for handing to the database client and nothing else.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for DatabaseUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DatabaseUrl(<hidden>)")
    }
}

/// Why the configuration could not be used.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The file could not be read.
    #[error("cannot read the configuration file {}: {error}", path.display())]
    Read {
        /// The file.
        path: PathBuf,
        /// What went wrong.
        error: std::io::Error,
    },
    /// The file is not valid TOML or has a setting this server does not know.
    #[error("{}: {error}", path.display())]
    Parse {
        /// The file.
        path: PathBuf,
        /// What is wrong with it.
        error: toml::de::Error,
    },
    /// A setting is missing or has a value that cannot be used.
    #[error("setting {key} (environment variable {variable}): {problem}")]
    Invalid {
        /// The setting's name in the file.
        key: &'static str,
        /// The environment variable that sets it.
        variable: &'static str,
        /// What is wrong.
        problem: String,
    },
}

/// The file's shape. Every setting is optional here; what is required is
/// decided after the environment has been applied.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    listen: Option<String>,
    database_url: Option<String>,
    regions: Option<Vec<String>>,
    archive_dir: Option<String>,
    maps_dir: Option<String>,
    imagery_dir: Option<String>,
    imagery_cache_mb: Option<u32>,
    user_agent: Option<String>,
    cors_origins: Option<Vec<String>>,
    log_level: Option<String>,
    registration: Option<String>,
    sites_from_pilots: Option<bool>,
    google_client_ids: Option<Vec<String>>,
    public_url: Option<String>,
    trusted_proxies: Option<Vec<String>>,
    overpass_url: Option<String>,
    open_meteo_url: Option<String>,
    #[serde(default)]
    retention: FileRetention,
    #[serde(default)]
    accounts: FileAccounts,
    #[serde(default)]
    push: FilePush,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FilePush {
    apns_key_file: Option<String>,
    apns_key_id: Option<String>,
    apns_team_id: Option<String>,
    apns_topic: Option<String>,
    fcm_service_account_file: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileAccounts {
    flights: Option<u32>,
    storage_mb: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileRetention {
    observation_months: Option<u32>,
    archive_days: Option<u32>,
    live_hours: Option<u32>,
    report_days: Option<u32>,
    plan_days: Option<u32>,
}

/// One setting: its name in the file and the variable that overrides it.
struct Setting {
    key: &'static str,
    variable: &'static str,
}

impl Setting {
    fn invalid(&self, problem: impl Into<String>) -> ConfigError {
        ConfigError::Invalid {
            key: self.key,
            variable: self.variable,
            problem: problem.into(),
        }
    }
}

const LISTEN: Setting = Setting {
    key: "listen",
    variable: "VENTI_LISTEN",
};
const DATABASE_URL: Setting = Setting {
    key: "database_url",
    variable: "VENTI_DATABASE_URL",
};
const REGIONS: Setting = Setting {
    key: "regions",
    variable: "VENTI_REGIONS",
};
const ARCHIVE_DIR: Setting = Setting {
    key: "archive_dir",
    variable: "VENTI_ARCHIVE_DIR",
};
const MAPS_DIR: Setting = Setting {
    key: "maps_dir",
    variable: "VENTI_MAPS_DIR",
};
const IMAGERY_DIR: Setting = Setting {
    key: "imagery_dir",
    variable: "VENTI_IMAGERY_DIR",
};
const IMAGERY_CACHE_MB: Setting = Setting {
    key: "imagery_cache_mb",
    variable: "VENTI_IMAGERY_CACHE_MB",
};
const USER_AGENT: Setting = Setting {
    key: "user_agent",
    variable: "VENTI_USER_AGENT",
};
const CORS_ORIGINS: Setting = Setting {
    key: "cors_origins",
    variable: "VENTI_CORS_ORIGINS",
};
const LOG_LEVEL: Setting = Setting {
    key: "log_level",
    variable: "VENTI_LOG_LEVEL",
};
const REGISTRATION: Setting = Setting {
    key: "registration",
    variable: "VENTI_REGISTRATION",
};
const SITES_FROM_PILOTS: Setting = Setting {
    key: "sites_from_pilots",
    variable: "VENTI_SITES_FROM_PILOTS",
};
const GOOGLE_CLIENT_IDS: Setting = Setting {
    key: "google_client_ids",
    variable: "VENTI_GOOGLE_CLIENT_IDS",
};
const PUBLIC_URL: Setting = Setting {
    key: "public_url",
    variable: "VENTI_PUBLIC_URL",
};
const TRUSTED_PROXIES: Setting = Setting {
    key: "trusted_proxies",
    variable: "VENTI_TRUSTED_PROXIES",
};
const OVERPASS_URL: Setting = Setting {
    key: "overpass_url",
    variable: "VENTI_OVERPASS_URL",
};
const OPEN_METEO_URL: Setting = Setting {
    key: "open_meteo_url",
    variable: "VENTI_OPEN_METEO_URL",
};
const OBSERVATION_MONTHS: Setting = Setting {
    key: "retention.observation_months",
    variable: "VENTI_OBSERVATION_MONTHS",
};
const ARCHIVE_DAYS: Setting = Setting {
    key: "retention.archive_days",
    variable: "VENTI_ARCHIVE_DAYS",
};
const LIVE_HOURS: Setting = Setting {
    key: "retention.live_hours",
    variable: "VENTI_LIVE_HOURS",
};
const REPORT_DAYS: Setting = Setting {
    key: "retention.report_days",
    variable: "VENTI_REPORT_DAYS",
};
const PLAN_DAYS: Setting = Setting {
    key: "retention.plan_days",
    variable: "VENTI_PLAN_DAYS",
};
const ACCOUNT_FLIGHTS: Setting = Setting {
    key: "accounts.flights",
    variable: "VENTI_ACCOUNT_FLIGHTS",
};
const ACCOUNT_STORAGE_MB: Setting = Setting {
    key: "accounts.storage_mb",
    variable: "VENTI_ACCOUNT_STORAGE_MB",
};
const APNS_KEY_FILE: Setting = Setting {
    key: "push.apns_key_file",
    variable: "VENTI_APNS_KEY_FILE",
};
const APNS_KEY_ID: Setting = Setting {
    key: "push.apns_key_id",
    variable: "VENTI_APNS_KEY_ID",
};
const APNS_TEAM_ID: Setting = Setting {
    key: "push.apns_team_id",
    variable: "VENTI_APNS_TEAM_ID",
};
const APNS_TOPIC: Setting = Setting {
    key: "push.apns_topic",
    variable: "VENTI_APNS_TOPIC",
};
const FCM_SERVICE_ACCOUNT_FILE: Setting = Setting {
    key: "push.fcm_service_account_file",
    variable: "VENTI_FCM_SERVICE_ACCOUNT_FILE",
};

/// The `User-Agent` used when none is configured: the program and its
/// version, and nothing about who runs it.
pub fn default_user_agent() -> String {
    concat!("venti-server/", env!("CARGO_PKG_VERSION")).to_owned()
}

/// Where the providers are that have no address of their own, from the
/// given environment alone. The `ingest` subcommand, which has no
/// configuration file, reads them with this.
pub fn providers_from_env(env: impl Fn(&str) -> Option<String>) -> Result<Providers, ConfigError> {
    let env = |setting: &Setting| env(setting.variable).filter(|value| !value.is_empty());
    providers(env(&OVERPASS_URL), env(&OPEN_METEO_URL))
}

/// Checks the addresses that were configured and fills in the others.
///
/// The address of the forecast service has no default: a server that is
/// not told one has no forecasts.
fn providers(
    overpass_url: Option<String>,
    open_meteo_url: Option<String>,
) -> Result<Providers, ConfigError> {
    let mut providers = Providers::default();
    if let Some(url) = overpass_url {
        adapters::check_overpass_url(&url).map_err(|problem| OVERPASS_URL.invalid(problem))?;
        providers.overpass_url = url;
    }
    if let Some(url) = open_meteo_url {
        adapters::check_open_meteo_url(&url).map_err(|problem| OPEN_METEO_URL.invalid(problem))?;
        providers.open_meteo_url = Some(url);
    }
    Ok(providers)
}

impl Config {
    /// Reads the configuration from an optional file and the process
    /// environment.
    pub fn load(file: Option<&Path>) -> Result<Self, ConfigError> {
        Self::load_with(file, |name| std::env::var(name).ok())
    }

    /// Reads the configuration from an optional file and the given
    /// environment. A variable that is unset or empty leaves the file's
    /// value, or the default, in place.
    pub fn load_with(
        file: Option<&Path>,
        env: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, ConfigError> {
        let file = match file {
            Some(path) => read_file(path)?,
            None => File::default(),
        };
        let env = |setting: &Setting| env(setting.variable).filter(|value| !value.is_empty());
        let list = |value: String| -> Vec<String> {
            value
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(str::to_owned)
                .collect()
        };

        let listen = env(&LISTEN)
            .or(file.listen)
            .unwrap_or_else(|| "127.0.0.1:8080".to_owned());
        let listen = listen.parse().map_err(|_| {
            LISTEN.invalid(format!("{listen:?} is not an address such as 0.0.0.0:8080"))
        })?;

        let database_url = env(&DATABASE_URL)
            .or(file.database_url)
            .ok_or_else(|| DATABASE_URL.invalid("is required"))?;
        if !(database_url.starts_with("postgres://") || database_url.starts_with("postgresql://")) {
            return Err(DATABASE_URL.invalid("must start with postgres://"));
        }

        let regions = env(&REGIONS).map(list).or(file.regions).unwrap_or_default();
        if regions.is_empty() {
            return Err(REGIONS.invalid("name at least one region.toml file"));
        }

        let archive_dir = env(&ARCHIVE_DIR).or(file.archive_dir).map(PathBuf::from);

        // No default: a server that is not told where its map regions are
        // hands out none. What is in the folder is looked at when the
        // server runs, not here.
        let maps_dir = env(&MAPS_DIR).or(file.maps_dir).map(PathBuf::from);

        // No default either: a server that is not told where to keep tiles
        // passes no imagery on, and asks no provider of imagery anything.
        let imagery_dir = env(&IMAGERY_DIR).or(file.imagery_dir).map(PathBuf::from);

        let user_agent = env(&USER_AGENT)
            .or(file.user_agent)
            .unwrap_or_else(default_user_agent);
        if user_agent.trim().is_empty() || !user_agent.chars().all(|c| (' '..='~').contains(&c)) {
            return Err(USER_AGENT.invalid("must be printable ASCII and not empty"));
        }

        let cors_origins = env(&CORS_ORIGINS)
            .map(list)
            .or(file.cors_origins)
            .unwrap_or_default();
        for origin in &cors_origins {
            check_origin(origin).map_err(|problem| CORS_ORIGINS.invalid(problem))?;
        }

        let log_level = env(&LOG_LEVEL)
            .or(file.log_level)
            .unwrap_or_else(|| "info".to_owned());
        let log_level = log_level.parse().map_err(|_| {
            LOG_LEVEL.invalid(format!(
                "{log_level:?} is not one of error, warn, info, debug, trace"
            ))
        })?;

        // By invite unless told otherwise: a server that was just set up
        // should not take accounts from whoever finds it.
        let registration = env(&REGISTRATION)
            .or(file.registration)
            .unwrap_or_else(|| "invite".to_owned());
        let registration = registration
            .parse()
            .map_err(|problem: String| REGISTRATION.invalid(problem))?;

        // Off unless told otherwise, and told in so many words.
        let sites_from_pilots = match env(&SITES_FROM_PILOTS).as_deref() {
            None => file.sites_from_pilots.unwrap_or(false),
            Some("true") => true,
            Some("false") => false,
            Some(_) => return Err(SITES_FROM_PILOTS.invalid("say true or false")),
        };

        let google_client_ids: Vec<String> = env(&GOOGLE_CLIENT_IDS)
            .map(list)
            .or(file.google_client_ids)
            .unwrap_or_default();
        if google_client_ids
            .iter()
            .any(|id| id.trim().is_empty() || id.chars().any(char::is_whitespace))
        {
            return Err(GOOGLE_CLIENT_IDS.invalid("a client ID is one word, with no spaces"));
        }

        let public_url = env(&PUBLIC_URL)
            .or(file.public_url)
            .map(|url| PublicUrl::new(&url).map_err(|problem| PUBLIC_URL.invalid(problem)))
            .transpose()?;

        let trusted_proxies = env(&TRUSTED_PROXIES)
            .map(list)
            .or(file.trusted_proxies)
            .unwrap_or_default();
        let trusted_proxies = TrustedProxies::parse(&trusted_proxies)
            .map_err(|problem| TRUSTED_PROXIES.invalid(problem))?;

        let providers = providers(
            env(&OVERPASS_URL).or(file.overpass_url),
            env(&OPEN_METEO_URL).or(file.open_meteo_url),
        )?;

        let number = |setting: &Setting, from_file: Option<u32>, default: u32| {
            let value = match env(setting) {
                Some(text) => text
                    .parse()
                    .map_err(|_| setting.invalid(format!("{text:?} is not a whole number")))?,
                None => from_file.unwrap_or(default),
            };
            if value == 0 {
                return Err(setting.invalid("must be at least 1"));
            }
            Ok(value)
        };
        let retention = Retention {
            observation_months: number(&OBSERVATION_MONTHS, file.retention.observation_months, 13)?,
            archive_days: number(&ARCHIVE_DAYS, file.retention.archive_days, 14)?,
            live_hours: number(
                &LIVE_HOURS,
                file.retention.live_hours,
                DEFAULT_RETENTION_HOURS,
            )?,
            report_days: number(
                &REPORT_DAYS,
                file.retention.report_days,
                DEFAULT_REPORT_DAYS,
            )?,
            plan_days: number(&PLAN_DAYS, file.retention.plan_days, DEFAULT_PLAN_DAYS)?,
        };
        let default_cache_mb = u32::try_from(DEFAULT_CACHE_MB).unwrap_or(u32::MAX);
        let imagery_cache_mb = number(&IMAGERY_CACHE_MB, file.imagery_cache_mb, default_cache_mb)?;
        let room = Room {
            flights: number(
                &ACCOUNT_FLIGHTS,
                file.accounts.flights,
                DEFAULT_ACCOUNT_FLIGHTS,
            )?
            .into(),
            file_bytes: u64::from(number(
                &ACCOUNT_STORAGE_MB,
                file.accounts.storage_mb,
                DEFAULT_ACCOUNT_STORAGE_MB,
            )?) * 1024
                * 1024,
        };

        let push = push_config(
            env(&APNS_KEY_FILE).or(file.push.apns_key_file),
            env(&APNS_KEY_ID).or(file.push.apns_key_id),
            env(&APNS_TEAM_ID).or(file.push.apns_team_id),
            env(&APNS_TOPIC).or(file.push.apns_topic),
            env(&FCM_SERVICE_ACCOUNT_FILE).or(file.push.fcm_service_account_file),
        )?;

        Ok(Self {
            listen,
            database_url: DatabaseUrl(database_url),
            regions: regions.into_iter().map(PathBuf::from).collect(),
            archive_dir,
            maps_dir,
            imagery_dir,
            imagery_cache_mb,
            user_agent,
            cors_origins,
            log_level,
            retention,
            registration,
            public_url,
            trusted_proxies,
            providers,
            room,
            sites_from_pilots,
            google_client_ids,
            push,
        })
    }
}

/// The settings of push to phones: APNs with all three of its key's
/// settings or none, FCM with its file or not.
fn push_config(
    key_file: Option<String>,
    key_id: Option<String>,
    team_id: Option<String>,
    topic: Option<String>,
    fcm_file: Option<String>,
) -> Result<PushConfig, ConfigError> {
    // Apple's IDs are ten letters and digits.
    let apple_id = |setting: &Setting, value: String| {
        if value.len() == 10 && value.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            Ok(value)
        } else {
            Err(setting.invalid("is ten letters and digits, as Apple's developer site shows it"))
        }
    };
    let apns = match (key_file, key_id, team_id) {
        (None, None, None) => None,
        (Some(key_file), Some(key_id), Some(team_id)) => {
            let topic = topic.unwrap_or_else(|| DEFAULT_APNS_TOPIC.to_owned());
            let plain = !topic.is_empty()
                && topic
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b".-".contains(&byte));
            if !plain {
                return Err(APNS_TOPIC.invalid("is an app's bundle ID, such as fly.venti.app"));
            }
            Some(ApnsConfig {
                key_file: PathBuf::from(key_file),
                key_id: apple_id(&APNS_KEY_ID, key_id)?,
                team_id: apple_id(&APNS_TEAM_ID, team_id)?,
                topic,
            })
        }
        (None, ..) => return Err(APNS_KEY_FILE.invalid("is needed with the key ID and team ID")),
        (_, None, _) => return Err(APNS_KEY_ID.invalid("is needed with the key file")),
        (_, _, None) => return Err(APNS_TEAM_ID.invalid("is needed with the key file")),
    };
    Ok(PushConfig {
        apns,
        fcm_service_account_file: fcm_file.map(PathBuf::from),
    })
}

fn read_file(path: &Path) -> Result<File, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|error| ConfigError::Read {
        path: path.to_owned(),
        error,
    })?;
    toml::from_str(&text).map_err(|error| ConfigError::Parse {
        path: path.to_owned(),
        error,
    })
}

/// A web origin is a scheme, a host and perhaps a port: no path, no slash at
/// the end. A browser sends it in exactly that form, and anything else would
/// never match.
fn check_origin(origin: &str) -> Result<(), String> {
    let rest = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))
        .ok_or_else(|| format!("{origin:?} must start with http:// or https://"))?;
    let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']');
    if rest.is_empty() || !rest.chars().all(plain) {
        return Err(format!(
            "{origin:?} must be an origin such as https://venti.example, without a path"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn example() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/config.example.toml"))
    }

    fn load(file: Option<&Path>, env: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let env: HashMap<String, String> = env
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        Config::load_with(file, |name| env.get(name).cloned())
    }

    fn problem(result: Result<Config, ConfigError>) -> String {
        result.unwrap_err().to_string()
    }

    #[test]
    fn push_to_phones_takes_all_of_apples_key_or_none() {
        let base = [
            ("VENTI_DATABASE_URL", "postgres://venti:pw@db/venti"),
            ("VENTI_REGIONS", "a/region.toml"),
        ];
        let with = |more: &[(&str, &str)]| {
            let mut env = base.to_vec();
            env.extend_from_slice(more);
            load(None, &env)
        };
        assert_eq!(with(&[]).unwrap().push, PushConfig::default());
        let apple = [
            ("VENTI_APNS_KEY_FILE", "/run/push/AuthKey.p8"),
            ("VENTI_APNS_KEY_ID", "ABC123DEFG"),
            ("VENTI_APNS_TEAM_ID", "TEAM456789"),
            ("VENTI_FCM_SERVICE_ACCOUNT_FILE", "/run/push/firebase.json"),
        ];
        let push = with(&apple).unwrap().push;
        assert_eq!(
            push.apns,
            Some(ApnsConfig {
                key_file: PathBuf::from("/run/push/AuthKey.p8"),
                key_id: "ABC123DEFG".to_owned(),
                team_id: "TEAM456789".to_owned(),
                topic: DEFAULT_APNS_TOPIC.to_owned(),
            })
        );
        assert_eq!(
            push.fcm_service_account_file,
            Some(PathBuf::from("/run/push/firebase.json"))
        );
        assert!(problem(with(&apple[..2])).contains("VENTI_APNS_TEAM_ID"));
        assert!(problem(with(&apple[1..3])).contains("VENTI_APNS_KEY_FILE"));
        let mut short = apple.to_vec();
        short[1] = ("VENTI_APNS_KEY_ID", "ABC");
        assert!(problem(with(&short)).contains("VENTI_APNS_KEY_ID"));
        short[1] = ("VENTI_APNS_TOPIC", "fly venti");
        short.push(("VENTI_APNS_KEY_ID", "ABC123DEFG"));
        assert!(problem(with(&short)).contains("VENTI_APNS_TOPIC"));
    }

    #[test]
    fn the_example_file_is_a_complete_configuration() {
        let config = load(Some(&example()), &[]).unwrap();
        assert_eq!(config.listen, "127.0.0.1:8080".parse().unwrap());
        assert_eq!(
            config.regions,
            [PathBuf::from("regions/alps-de-at/region.toml")]
        );
        assert_eq!(config.archive_dir, Some(PathBuf::from("var/archive")));
        // The example hands out no maps until a folder is named.
        assert_eq!(config.maps_dir, None);
        // It keeps tiles of imagery in var/imagery, up to 20 GB.
        assert_eq!(config.imagery_dir, Some(PathBuf::from("var/imagery")));
        assert_eq!(config.imagery_cache_mb, 20480);
        assert_eq!(config.user_agent, "venti-server/0.1");
        assert_eq!(config.cors_origins, ["http://localhost:5173"]);
        assert_eq!(config.log_level, Level::INFO);
        assert_eq!(
            config.retention,
            Retention {
                observation_months: 13,
                archive_days: 14,
                live_hours: 48,
                report_days: 90,
                plan_days: 2,
            }
        );
        assert_eq!(config.registration, Registration::Invite);
        // No pilot gives a site to a server that was not told to take one.
        assert!(!config.sites_from_pilots);
        assert_eq!(
            config.public_url,
            Some(PublicUrl::new("http://127.0.0.1:8080").unwrap())
        );
        assert_eq!(config.trusted_proxies, TrustedProxies::default());
        // The example names the defaults.
        assert_eq!(config.room.flights, u64::from(DEFAULT_ACCOUNT_FLIGHTS));
        assert_eq!(
            config.room.file_bytes,
            u64::from(DEFAULT_ACCOUNT_STORAGE_MB) * 1024 * 1024
        );
    }

    #[test]
    fn the_environment_alone_is_enough_and_has_defaults() {
        let config = load(
            None,
            &[
                ("VENTI_DATABASE_URL", "postgres://venti:pw@db/venti"),
                ("VENTI_REGIONS", "a/region.toml, b/region.toml"),
            ],
        )
        .unwrap();
        assert_eq!(config.listen, "127.0.0.1:8080".parse().unwrap());
        assert_eq!(config.database_url.expose(), "postgres://venti:pw@db/venti");
        assert_eq!(
            config.regions,
            [
                PathBuf::from("a/region.toml"),
                PathBuf::from("b/region.toml")
            ]
        );
        assert_eq!(config.archive_dir, None);
        // A server that is not told where its map regions are has none.
        assert_eq!(config.maps_dir, None);
        assert_eq!(config.user_agent, default_user_agent());
        assert!(config.cors_origins.is_empty());
        // Nobody registers on a server that was not told who may.
        assert_eq!(config.registration, Registration::Invite);
        assert_eq!(config.public_url, None);
        // And no header is believed about where a request came from.
        assert_eq!(config.trusted_proxies, TrustedProxies::default());
        // The flying sites are asked of the main public Overpass instance.
        assert_eq!(config.providers, Providers::default());
        assert_eq!(
            config.providers.overpass_url,
            "https://overpass-api.de/api/interpreter"
        );
        // A server that is not told where its forecast service is has none.
        assert_eq!(config.providers.open_meteo_url, None);
        // An account has room for twenty years of flying, and an end.
        assert_eq!(
            config.room,
            Room {
                flights: 25_000,
                file_bytes: 4096 * 1024 * 1024
            }
        );
        // A thousand tandem flights a year for twenty years.
        assert!(config.room.flights >= 20 * 1000);
        // 150 flights a year of four hours each, at the 150 kB an hour
        // that a position a second makes: twice that fits.
        let twenty_years: u64 = 20 * 150 * 4 * 150 * 1024;
        assert!(config.room.file_bytes >= 2 * twenty_years);
        // One account is not the disk.
        assert!(config.room.file_bytes <= 8 * 1024 * 1024 * 1024);
    }

    #[test]
    fn the_room_of_an_account_is_a_setting() {
        let config = load(
            None,
            &[
                ("VENTI_DATABASE_URL", "postgres://venti:pw@db/venti"),
                ("VENTI_REGIONS", "a/region.toml"),
                ("VENTI_ACCOUNT_FLIGHTS", "500"),
                ("VENTI_ACCOUNT_STORAGE_MB", "64"),
            ],
        )
        .unwrap();
        assert_eq!(
            config.room,
            Room {
                flights: 500,
                file_bytes: 64 * 1024 * 1024
            }
        );
    }

    #[test]
    fn the_forecast_service_is_where_the_operator_says_and_nowhere_else() {
        let own = "http://open-meteo:8080";
        let config = load(Some(&example()), &[("VENTI_OPEN_METEO_URL", own)]).unwrap();
        assert_eq!(config.providers.open_meteo_url.as_deref(), Some(own));

        // The one-shot run reads the same variable.
        let env = |name: &str| (name == "VENTI_OPEN_METEO_URL").then(|| own.to_owned());
        let providers = providers_from_env(env).unwrap();
        assert_eq!(providers.open_meteo_url.as_deref(), Some(own));

        let refused = load(
            Some(&example()),
            &[("VENTI_OPEN_METEO_URL", "open-meteo:8080")],
        );
        let problem = refused.unwrap_err().to_string();
        assert!(problem.contains("VENTI_OPEN_METEO_URL"), "{problem}");
    }

    #[test]
    fn the_providers_without_an_address_of_their_own_can_be_named() {
        let own = "http://overpass:12345/api/interpreter";
        let config = load(Some(&example()), &[("VENTI_OVERPASS_URL", own)]).unwrap();
        assert_eq!(config.providers.overpass_url, own);

        // The one-shot run reads the same variables and no file.
        let env = |name: &str| (name == "VENTI_OVERPASS_URL").then(|| own.to_owned());
        assert_eq!(providers_from_env(env).unwrap().overpass_url, own);
        assert_eq!(providers_from_env(|_| None).unwrap(), Providers::default());
        let bad = |_: &str| Some("ftp://overpass".to_owned());
        assert!(
            providers_from_env(bad)
                .unwrap_err()
                .to_string()
                .contains("VENTI_OVERPASS_URL")
        );
    }

    #[test]
    fn a_variable_wins_over_the_file_and_an_empty_one_does_not() {
        let config = load(
            Some(&example()),
            &[
                ("VENTI_LISTEN", "0.0.0.0:9000"),
                ("VENTI_ARCHIVE_DIR", ""),
                ("VENTI_MAPS_DIR", "/srv/maps"),
                ("VENTI_IMAGERY_DIR", "/srv/imagery"),
                ("VENTI_IMAGERY_CACHE_MB", "512"),
                (
                    "VENTI_CORS_ORIGINS",
                    "https://venti.example,https://www.venti.example",
                ),
                ("VENTI_LOG_LEVEL", "debug"),
                ("VENTI_OBSERVATION_MONTHS", "24"),
                ("VENTI_LIVE_HOURS", "24"),
                ("VENTI_REPORT_DAYS", "30"),
                ("VENTI_PLAN_DAYS", "7"),
                ("VENTI_SITES_FROM_PILOTS", "true"),
                ("VENTI_REGISTRATION", "open"),
                ("VENTI_PUBLIC_URL", "https://venti.example"),
                ("VENTI_TRUSTED_PROXIES", "172.18.0.0/16, ::1"),
            ],
        )
        .unwrap();
        assert_eq!(config.registration, Registration::Open);
        assert_eq!(
            config.public_url,
            Some(PublicUrl::new("https://venti.example").unwrap())
        );
        assert_eq!(
            config.trusted_proxies,
            TrustedProxies::parse(&["172.18.0.0/16", "::1"]).unwrap()
        );
        assert_eq!(config.listen, "0.0.0.0:9000".parse().unwrap());
        assert_eq!(config.archive_dir, Some(PathBuf::from("var/archive")));
        assert_eq!(config.maps_dir, Some(PathBuf::from("/srv/maps")));
        assert_eq!(config.imagery_dir, Some(PathBuf::from("/srv/imagery")));
        assert_eq!(config.imagery_cache_mb, 512);
        assert_eq!(
            config.cors_origins,
            ["https://venti.example", "https://www.venti.example"]
        );
        assert_eq!(config.log_level, Level::DEBUG);
        assert_eq!(config.retention.observation_months, 24);
        assert_eq!(config.retention.archive_days, 14);
        assert_eq!(config.retention.live_hours, 24);
        assert_eq!(config.retention.report_days, 30);
        assert_eq!(config.retention.plan_days, 7);
        assert!(config.sites_from_pilots);
    }

    #[test]
    fn the_database_password_is_never_printed() {
        let config = load(
            None,
            &[
                ("VENTI_DATABASE_URL", "postgres://venti:hunter2@db/venti"),
                ("VENTI_REGIONS", "a/region.toml"),
            ],
        )
        .unwrap();
        assert!(!format!("{config:?}").contains("hunter2"));
    }

    #[test]
    fn problems_name_the_setting_and_its_variable() {
        let base = [
            ("VENTI_DATABASE_URL", "postgres://venti:pw@db/venti"),
            ("VENTI_REGIONS", "a/region.toml"),
        ];
        let with = |name: &'static str, value: &'static str| {
            let mut env = base.to_vec();
            env.push((name, value));
            problem(load(None, &env))
        };

        assert!(problem(load(None, &base[1..])).contains("VENTI_DATABASE_URL"));
        assert!(problem(load(None, &base[..1])).contains("VENTI_REGIONS"));
        assert!(with("VENTI_LISTEN", "localhost").contains("listen"));
        assert!(with("VENTI_DATABASE_URL", "mysql://db").contains("postgres://"));
        assert!(with("VENTI_USER_AGENT", "Venti\nX-Injected: 1").contains("user_agent"));
        assert!(with("VENTI_CORS_ORIGINS", "venti.example").contains("http"));
        assert!(with("VENTI_CORS_ORIGINS", "https://venti.example/").contains("without a path"));
        assert!(with("VENTI_LOG_LEVEL", "chatty").contains("log_level"));
        assert!(with("VENTI_REGISTRATION", "everyone").contains("open, invite, closed"));
        assert!(with("VENTI_PUBLIC_URL", "venti.example").contains("public_url"));
        assert!(with("VENTI_PUBLIC_URL", "https://venti.example/app").contains("without a path"));
        assert!(with("VENTI_TRUSTED_PROXIES", "caddy").contains("trusted_proxies"));
        assert!(with("VENTI_OVERPASS_URL", "overpass-api.de").contains("overpass_url"));
        assert!(with("VENTI_ARCHIVE_DAYS", "0").contains("at least 1"));
        assert!(with("VENTI_LIVE_HOURS", "0").contains("retention.live_hours"));
        assert!(with("VENTI_REPORT_DAYS", "0").contains("retention.report_days"));
        assert!(with("VENTI_PLAN_DAYS", "0").contains("retention.plan_days"));
        assert!(with("VENTI_SITES_FROM_PILOTS", "yes").contains("sites_from_pilots"));
        assert!(with("VENTI_ACCOUNT_FLIGHTS", "0").contains("accounts.flights"));
        assert!(with("VENTI_ACCOUNT_STORAGE_MB", "plenty").contains("accounts.storage_mb"));
        assert!(with("VENTI_OBSERVATION_MONTHS", "many").contains("whole number"));
    }

    #[test]
    fn an_unknown_setting_in_the_file_is_an_error() {
        let path = std::env::temp_dir().join(format!("venti-config-{}.toml", std::process::id()));
        std::fs::write(
            &path,
            "listen = \"127.0.0.1:8080\"\nlisten_adress = \"x\"\n",
        )
        .unwrap();
        let result = load(Some(&path), &[]);
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(result, Err(ConfigError::Parse { .. })));

        let missing = PathBuf::from("no/such/venti-server.toml");
        assert!(matches!(
            load(Some(&missing), &[]),
            Err(ConfigError::Read { .. })
        ));
    }
}
