//! fw-app: a reference service built from the fw-* crates. It is the thing you copy and rename.

mod logging;
mod service;
#[cfg(feature = "tui")]
mod top;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "fw-app",
    version,
    about = "Reference service built from the fw-* crates"
)]
struct Cli {
    /// Config file (TOML).
    #[arg(long, global = true, default_value = "fw-app.toml")]
    config: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Parse and validate the config and the TLS files, print a one-line summary, and exit.
    Check,
    /// Run the HTTP listener. This is the default.
    Serve,
    /// Live terminal dashboard demo. Needs a build with the `tui` feature. Ignores --config.
    #[cfg(feature = "tui")]
    Top,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Serve) {
        Command::Check => check(&cli.config),
        Command::Serve => serve(&cli.config).await,
        #[cfg(feature = "tui")]
        Command::Top => match top::run().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => fail(&error),
        },
    }
}

fn check(path: &std::path::Path) -> ExitCode {
    let config = match fw_config::load(path) {
        Ok(config) => config,
        Err(error) => return fail(&error.to_string()),
    };
    if let Some(tls) = &config.tls {
        if let Err(error) = fw_serve::tls::load(
            std::path::Path::new(&tls.cert),
            std::path::Path::new(&tls.key),
        ) {
            return fail(&error);
        }
    }
    println!(
        "ok: listen {}, {} token(s), tls {}, log {}",
        config.server.listen,
        config.auth.tokens.len(),
        if config.tls.is_some() { "on" } else { "off" },
        config.log.level,
    );
    ExitCode::SUCCESS
}

async fn serve(path: &std::path::Path) -> ExitCode {
    let config = match fw_config::load(path) {
        Ok(config) => config,
        Err(error) => return fail(&error.to_string()),
    };
    if let Err(error) = logging::init(&config.log) {
        return fail(&error);
    }
    match service::run(&config, path).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "serve failed");
            fail(&error)
        }
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("fw-app: {message}");
    ExitCode::FAILURE
}
