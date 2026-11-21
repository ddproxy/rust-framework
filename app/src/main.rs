//! fw-app: a reference service built from the fw-* crates. It is the thing you copy and rename.

mod logging;
mod service;
#[cfg(feature = "tui")]
mod top;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::SystemTime;

use clap::{Parser, Subcommand};
use fw_config::Config;

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
    /// Parse and validate the config, TLS files and state directory, print a one-line summary,
    /// and exit.
    Check,
    /// Run the HTTP listener. This is the default.
    Serve,
    /// Live terminal dashboard demo. Needs a build with the `tui` feature. Ignores --config.
    #[cfg(feature = "tui")]
    Top,
}

#[tokio::main]
async fn main() -> ExitCode {
    logging::install_panic_hook();
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

fn check(path: &Path) -> ExitCode {
    let config = match fw_config::load(path) {
        Ok(config) => config,
        Err(error) => return fail(&error.to_string()),
    };
    if let Err(error) = check_state(&config) {
        return fail(&error);
    }
    let not_after = match &config.tls {
        Some(tls) => match fw_serve::tls::load_info(Path::new(&tls.cert), Path::new(&tls.key)) {
            Ok(info) => Some(info.not_after),
            Err(error) => return fail(&error),
        },
        None => None,
    };
    println!("{}", summary(&config, not_after));
    ExitCode::SUCCESS
}

fn summary(config: &Config, not_after: Option<SystemTime>) -> String {
    let tls = match not_after {
        Some(time) => format!("on (not_after={})", fw_serve::unix_secs(time)),
        None => "off".to_owned(),
    };
    format!(
        "ok: listen {}, {} token(s), tls {}, log {}",
        config.server.listen,
        config.auth.tokens.len(),
        tls,
        config.log.level,
    )
}

fn check_state(config: &Config) -> Result<(), String> {
    match &config.state.dir {
        Some(dir) => fw_config::check_state_dir(Path::new(dir)).map_err(|e| e.to_string()),
        None => Ok(()),
    }
}

async fn serve(path: &Path) -> ExitCode {
    let config = match fw_config::load(path) {
        Ok(config) => config,
        Err(error) => return fail(&error.to_string()),
    };
    if let Err(error) = check_state(&config) {
        return fail(&error);
    }
    if let Err(error) = logging::init(&config.log) {
        return fail(&error);
    }
    tokio::spawn(logging::watch_hangup());
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn the_check_summary_reports_the_certificate_expiry_when_tls_is_on() {
        let summary_without_tls = summary(&Config::default(), None);
        assert!(
            summary_without_tls.contains("tls off"),
            "{summary_without_tls}"
        );
        let expires = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        let with_tls = summary(&Config::default(), Some(expires));
        assert!(with_tls.contains("not_after=1800000000"), "{with_tls}");
    }
}
