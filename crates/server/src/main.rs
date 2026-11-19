//! The `venti-server` program. See [`venti_server`] for what it does and
//! [`venti_server::cli::USAGE`] for its command line.

use std::process::ExitCode;

use tracing::Level;
use venti_server::cli::{self, Command, USAGE};
use venti_server::config::{Config, default_user_agent};
use venti_server::ingest::oneshot;
use venti_server::{dev, serve};

/// Exit status for a command line or configuration that cannot be used.
const USAGE_ERROR: u8 = 2;

#[tokio::main]
async fn main() -> ExitCode {
    let command = match cli::parse(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            return ExitCode::from(USAGE_ERROR);
        }
    };

    let outcome = match command {
        Command::Help => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Command::Version => {
            println!("venti-server {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Command::Ingest(args) => {
            // The one-shot run has no configuration file; the variable is
            // the same one the server reads.
            let user_agent = std::env::var("VENTI_USER_AGENT")
                .ok()
                .filter(|value| !value.is_empty())
                .unwrap_or_else(default_user_agent);
            return oneshot::run(args, &user_agent).await;
        }
        Command::Serve { config } => {
            let config = match Config::load(config.as_deref()) {
                Ok(config) => config,
                Err(error) => {
                    eprintln!("{error}");
                    return ExitCode::from(USAGE_ERROR);
                }
            };
            log_to_stderr(config.log_level);
            serve::serve(config).await
        }
        Command::Dev { listen } => {
            log_to_stderr(Level::INFO);
            dev::dev(listen).await
        }
    };

    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            tracing::error!("{message}");
            ExitCode::FAILURE
        }
    }
}

/// Sends log lines to standard error, where a container runtime or a
/// service manager collects them.
fn log_to_stderr(level: Level) {
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .init();
}
