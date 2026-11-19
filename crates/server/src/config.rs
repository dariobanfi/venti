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

use crate::accounts::Registration;
use crate::accounts::client::TrustedProxies;
use crate::flights::share::PublicUrl;

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
}

/// How long data is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retention {
    /// Observations are kept for this many whole calendar months before the
    /// current one.
    pub observation_months: u32,
    /// Raw provider payloads are kept for this many days.
    pub archive_days: u32,
}

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
    user_agent: Option<String>,
    cors_origins: Option<Vec<String>>,
    log_level: Option<String>,
    registration: Option<String>,
    public_url: Option<String>,
    trusted_proxies: Option<Vec<String>>,
    #[serde(default)]
    retention: FileRetention,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileRetention {
    observation_months: Option<u32>,
    archive_days: Option<u32>,
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
const PUBLIC_URL: Setting = Setting {
    key: "public_url",
    variable: "VENTI_PUBLIC_URL",
};
const TRUSTED_PROXIES: Setting = Setting {
    key: "trusted_proxies",
    variable: "VENTI_TRUSTED_PROXIES",
};
const OBSERVATION_MONTHS: Setting = Setting {
    key: "retention.observation_months",
    variable: "VENTI_OBSERVATION_MONTHS",
};
const ARCHIVE_DAYS: Setting = Setting {
    key: "retention.archive_days",
    variable: "VENTI_ARCHIVE_DAYS",
};

/// The `User-Agent` used when none is configured: the program and its
/// version, and nothing about who runs it.
pub fn default_user_agent() -> String {
    concat!("venti-server/", env!("CARGO_PKG_VERSION")).to_owned()
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
        };

        Ok(Self {
            listen,
            database_url: DatabaseUrl(database_url),
            regions: regions.into_iter().map(PathBuf::from).collect(),
            archive_dir,
            user_agent,
            cors_origins,
            log_level,
            retention,
            registration,
            public_url,
            trusted_proxies,
        })
    }
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
    fn the_example_file_is_a_complete_configuration() {
        let config = load(Some(&example()), &[]).unwrap();
        assert_eq!(config.listen, "127.0.0.1:8080".parse().unwrap());
        assert_eq!(
            config.regions,
            [PathBuf::from("regions/alps-de-at/region.toml")]
        );
        assert_eq!(config.archive_dir, Some(PathBuf::from("var/archive")));
        assert_eq!(config.user_agent, "venti-server/0.1");
        assert_eq!(config.cors_origins, ["http://localhost:5173"]);
        assert_eq!(config.log_level, Level::INFO);
        assert_eq!(
            config.retention,
            Retention {
                observation_months: 13,
                archive_days: 14
            }
        );
        assert_eq!(config.registration, Registration::Invite);
        assert_eq!(
            config.public_url,
            Some(PublicUrl::new("http://127.0.0.1:8080").unwrap())
        );
        assert_eq!(config.trusted_proxies, TrustedProxies::default());
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
        assert_eq!(config.user_agent, default_user_agent());
        assert!(config.cors_origins.is_empty());
        // Nobody registers on a server that was not told who may.
        assert_eq!(config.registration, Registration::Invite);
        assert_eq!(config.public_url, None);
        // And no header is believed about where a request came from.
        assert_eq!(config.trusted_proxies, TrustedProxies::default());
    }

    #[test]
    fn a_variable_wins_over_the_file_and_an_empty_one_does_not() {
        let config = load(
            Some(&example()),
            &[
                ("VENTI_LISTEN", "0.0.0.0:9000"),
                ("VENTI_ARCHIVE_DIR", ""),
                (
                    "VENTI_CORS_ORIGINS",
                    "https://venti.example,https://www.venti.example",
                ),
                ("VENTI_LOG_LEVEL", "debug"),
                ("VENTI_OBSERVATION_MONTHS", "24"),
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
        assert_eq!(
            config.cors_origins,
            ["https://venti.example", "https://www.venti.example"]
        );
        assert_eq!(config.log_level, Level::DEBUG);
        assert_eq!(config.retention.observation_months, 24);
        assert_eq!(config.retention.archive_days, 14);
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
        assert!(with("VENTI_ARCHIVE_DAYS", "0").contains("at least 1"));
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
