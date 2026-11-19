//! The command line of `venti-server`.

use std::net::SocketAddr;
use std::path::PathBuf;

use venti_domain::SourceId;

use crate::ingest::oneshot;

/// What `venti-server --help` prints.
pub const USAGE: &str = "\
usage: venti-server <command>

  serve [--config FILE]
      Run the server: collect the configured sources on their schedule,
      store them in PostgreSQL and serve the API. Settings come from FILE
      and from VENTI_* environment variables; see config.example.toml.

  dev [--listen ADDRESS]
      Serve the same API over recorded data. No network, no database.
      Listens on 127.0.0.1:8080 unless told otherwise.

  ingest <region.toml> [--source ID] [--archive DIR] [--json]
      Run a region's sources once against the live providers and print a
      health line for each. Stores nothing. Fails when a source delivers
      nothing or reports problems.

  --help, --version";

/// What the program was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Run the server.
    Serve {
        /// The configuration file, if one was named.
        config: Option<PathBuf>,
    },
    /// Serve recorded data.
    Dev {
        /// The address to listen on.
        listen: SocketAddr,
    },
    /// Run a region's sources once.
    Ingest(oneshot::Args),
    /// Print the usage.
    Help,
    /// Print the version.
    Version,
}

/// Reads the arguments that follow the program's name.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut args = args.into_iter();
    let command = args.next().ok_or("no command given")?;
    match command.as_str() {
        "serve" => parse_serve(args),
        "dev" => parse_dev(args),
        "ingest" => parse_ingest(args),
        "--help" | "-h" | "help" => Ok(Command::Help),
        "--version" | "-V" => Ok(Command::Version),
        other => Err(format!("unknown command {other:?}")),
    }
}

/// The value that must follow an option.
fn value(option: &str, args: &mut impl Iterator<Item = String>) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{option} needs a value"))
}

fn parse_serve(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut config = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => config = Some(PathBuf::from(value("--config", &mut args)?)),
            other => return Err(format!("serve does not take {other:?}")),
        }
    }
    Ok(Command::Serve { config })
}

fn parse_dev(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut listen = SocketAddr::from(([127, 0, 0, 1], 8080));
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--listen" => {
                let address = value("--listen", &mut args)?;
                listen = address
                    .parse()
                    .map_err(|_| format!("{address:?} is not an address such as 127.0.0.1:8080"))?;
            }
            other => return Err(format!("dev does not take {other:?}")),
        }
    }
    Ok(Command::Dev { listen })
}

fn parse_ingest(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut region = None;
    let mut source = None;
    let mut archive = None;
    let mut json = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--source" => {
                let id = value("--source", &mut args)?;
                source = Some(SourceId::new(id).map_err(|e| e.to_string())?);
            }
            "--archive" => archive = Some(PathBuf::from(value("--archive", &mut args)?)),
            "--json" => json = true,
            _ if arg.starts_with("--") => return Err(format!("ingest does not take {arg:?}")),
            _ if region.is_none() => region = Some(PathBuf::from(arg)),
            _ => return Err("ingest takes one region file".to_owned()),
        }
    }
    Ok(Command::Ingest(oneshot::Args {
        region: region.ok_or("ingest needs a region file")?,
        source,
        archive,
        json,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_line(line: &str) -> Result<Command, String> {
        parse(line.split_whitespace().map(str::to_owned))
    }

    #[test]
    fn serve_takes_an_optional_configuration_file() {
        assert_eq!(parse_line("serve"), Ok(Command::Serve { config: None }));
        assert_eq!(
            parse_line("serve --config var/venti-server.toml"),
            Ok(Command::Serve {
                config: Some(PathBuf::from("var/venti-server.toml"))
            })
        );
        assert!(parse_line("serve --config").is_err());
        assert!(parse_line("serve --port 80").is_err());
    }

    #[test]
    fn dev_listens_on_the_loopback_address_unless_told_otherwise() {
        assert_eq!(
            parse_line("dev"),
            Ok(Command::Dev {
                listen: "127.0.0.1:8080".parse().unwrap()
            })
        );
        assert_eq!(
            parse_line("dev --listen 0.0.0.0:9000"),
            Ok(Command::Dev {
                listen: "0.0.0.0:9000".parse().unwrap()
            })
        );
        assert!(parse_line("dev --listen localhost").is_err());
    }

    #[test]
    fn ingest_keeps_the_options_of_the_tool_it_replaces() {
        let command = parse_line(
            "ingest regions/alps-de-at/region.toml --source dwd-10min --archive var/raw --json",
        );
        assert_eq!(
            command,
            Ok(Command::Ingest(oneshot::Args {
                region: PathBuf::from("regions/alps-de-at/region.toml"),
                source: Some(SourceId::new("dwd-10min").unwrap()),
                archive: Some(PathBuf::from("var/raw")),
                json: true,
            }))
        );
        assert!(parse_line("ingest").is_err());
        assert!(parse_line("ingest a.toml b.toml").is_err());
        assert!(parse_line("ingest a.toml --verbose").is_err());
        assert!(parse_line("ingest a.toml --source Not/An/Id").is_err());
    }

    #[test]
    fn help_version_and_mistakes() {
        assert_eq!(parse_line("--help"), Ok(Command::Help));
        assert_eq!(parse_line("--version"), Ok(Command::Version));
        assert!(parse_line("").is_err());
        assert!(parse_line("migrate").is_err());
    }
}
