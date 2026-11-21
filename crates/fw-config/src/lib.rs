//! Service config. Unknown keys are refused, the file is size-capped, every validation error
//! names its table, and a TLS key other users can read is refused. An optional state directory is
//! checked for writability at startup.

mod error;
mod secret;

use std::fs::OpenOptions;
use std::io::Read;
use std::net::SocketAddr;
use std::path::Path;

use serde::Deserialize;

pub use error::ConfigError;
pub use secret::check_private_file;

/// Where the service may write state. `None` means no state is persisted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct StateConfig {
    pub dir: Option<String>,
}

/// A config is a few KiB. The cap stops a runaway or hostile file from being read whole.
pub const MAX_CONFIG_BYTES: u64 = 64 * 1024;

const LOG_LEVELS: [&str; 5] = ["error", "warn", "info", "debug", "trace"];

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    pub tls: Option<TlsConfig>,
    #[serde(default)]
    pub log: LogConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub state: StateConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ServerConfig {
    pub listen: String,
    pub max_connections: usize,
    pub max_connections_per_addr: usize,
    pub header_read_timeout_secs: u64,
    /// Longest a request may take from the end of its headers to the end of its response.
    pub request_timeout_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:8080".to_owned(),
            max_connections: 256,
            max_connections_per_addr: 16,
            header_read_timeout_secs: 30,
            request_timeout_secs: 60,
        }
    }
}

/// Both paths are required when the table is present. Without the table the listener is plain HTTP.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TlsConfig {
    pub cert: String,
    pub key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct LogConfig {
    pub level: String,
    pub format: LogFormat,
    pub stderr: bool,
    pub file: Option<String>,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".to_owned(),
            format: LogFormat::Text,
            stderr: true,
            file: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct AuthConfig {
    pub throttle: bool,
    pub max_failures: u32,
    pub window_secs: u64,
    pub tokens: Vec<TokenEntry>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            throttle: true,
            max_failures: 10,
            window_secs: 60,
            tokens: Vec::new(),
        }
    }
}

/// A named bearer token, stored only as the lowercase hex SHA-256 of the token. Produce it with
/// `printf '%s' "$TOKEN" | sha256sum`. Tokens should be long: a hash cannot show how long its
/// source was.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenEntry {
    pub name: String,
    pub token_sha256: String,
}

impl Config {
    /// Parses and validates config text. Does not touch the filesystem.
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(text).map_err(|e| ConfigError::Parse(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        self.validate_server()?;
        self.validate_tls()?;
        self.validate_log()?;
        self.validate_auth()?;
        self.validate_state()
    }

    fn validate_state(&self) -> Result<(), ConfigError> {
        if self.state.dir.as_deref() == Some("") {
            return invalid("[state]", "dir must not be empty".to_owned());
        }
        Ok(())
    }

    fn validate_server(&self) -> Result<(), ConfigError> {
        let server = &self.server;
        if server.listen.parse::<SocketAddr>().is_err() {
            return invalid(
                "[server]",
                format!(
                    "listen must be a socket address like 127.0.0.1:8080 (got {:?})",
                    server.listen
                ),
            );
        }
        if !(1..=100_000).contains(&server.max_connections) {
            return invalid(
                "[server]",
                format!(
                    "max_connections must be 1 to 100000 (got {})",
                    server.max_connections
                ),
            );
        }
        if !(1..=server.max_connections).contains(&server.max_connections_per_addr) {
            return invalid(
                "[server]",
                format!(
                    "max_connections_per_addr must be 1 to max_connections ({}) (got {})",
                    server.max_connections, server.max_connections_per_addr
                ),
            );
        }
        if !(1..=300).contains(&server.header_read_timeout_secs) {
            return invalid(
                "[server]",
                format!(
                    "header_read_timeout_secs must be 1 to 300 (got {})",
                    server.header_read_timeout_secs
                ),
            );
        }
        if !(1..=600).contains(&server.request_timeout_secs) {
            return invalid(
                "[server]",
                format!(
                    "request_timeout_secs must be 1 to 600 (got {})",
                    server.request_timeout_secs
                ),
            );
        }
        // Bearer tokens travel in the clear without TLS, so only loopback may go without it.
        let listen: SocketAddr = server.listen.parse().expect("checked above");
        if self.tls.is_none() && !listen.ip().is_loopback() {
            return invalid(
                "[server]",
                format!(
                    "listen {} is not a loopback address, so [tls] is required",
                    server.listen
                ),
            );
        }
        Ok(())
    }

    fn validate_tls(&self) -> Result<(), ConfigError> {
        match &self.tls {
            Some(tls) if tls.cert.is_empty() || tls.key.is_empty() => {
                invalid("[tls]", "cert and key must both be set".to_owned())
            }
            _ => Ok(()),
        }
    }

    fn validate_log(&self) -> Result<(), ConfigError> {
        if !LOG_LEVELS.contains(&self.log.level.as_str()) {
            return invalid(
                "[log]",
                format!(
                    "level must be one of {} (got {:?})",
                    LOG_LEVELS.join(", "),
                    self.log.level
                ),
            );
        }
        if self.log.file.as_deref() == Some("") {
            return invalid(
                "[log]",
                "file must not be empty; leave it out to log without a file".to_owned(),
            );
        }
        Ok(())
    }

    fn validate_auth(&self) -> Result<(), ConfigError> {
        let auth = &self.auth;
        if !(1..=1000).contains(&auth.max_failures) {
            return invalid(
                "[auth]",
                format!("max_failures must be 1 to 1000 (got {})", auth.max_failures),
            );
        }
        if !(1..=86_400).contains(&auth.window_secs) {
            return invalid(
                "[auth]",
                format!("window_secs must be 1 to 86400 (got {})", auth.window_secs),
            );
        }
        let mut names = Vec::new();
        let mut hashes = Vec::new();
        for (index, token) in auth.tokens.iter().enumerate() {
            let table = format!("[[auth.tokens]] #{} ({})", index + 1, token.name);
            if !valid_name(&token.name) {
                return invalid(
                    &table,
                    "name must be 1 to 64 characters from A-Z a-z 0-9 . _ -".to_owned(),
                );
            }
            if names.contains(&token.name) {
                return invalid(&table, "name is used by an earlier token".to_owned());
            }
            if !valid_sha256_hex(&token.token_sha256) {
                return invalid(
                    &table,
                    "token_sha256 must be 64 lowercase hex characters".to_owned(),
                );
            }
            if hashes.contains(&token.token_sha256) {
                return invalid(
                    &table,
                    "token_sha256 is the same as an earlier token".to_owned(),
                );
            }
            names.push(token.name.clone());
            hashes.push(token.token_sha256.clone());
        }
        Ok(())
    }
}

fn invalid(table: &str, reason: String) -> Result<(), ConfigError> {
    Err(ConfigError::Invalid {
        table: table.to_owned(),
        reason,
    })
}

fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn valid_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Reads and validates a config file. Refuses files over [`MAX_CONFIG_BYTES`] and a TLS key that
/// group or others can read.
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let shown = path.display().to_string();
    let file = std::fs::File::open(path).map_err(|source| ConfigError::Io {
        path: shown.clone(),
        source,
    })?;
    let mut text = String::new();
    file.take(MAX_CONFIG_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|source| ConfigError::Io {
            path: shown.clone(),
            source,
        })?;
    if text.len() as u64 > MAX_CONFIG_BYTES {
        return Err(ConfigError::TooLarge {
            path: shown,
            limit: MAX_CONFIG_BYTES,
        });
    }
    let config = Config::parse(&text)?;
    if let Some(tls) = &config.tls {
        check_private_file("[tls] key", Path::new(&tls.key))?;
    }
    Ok(config)
}

/// Creates `path` if needed and proves it writable with a temporary probe file, which is removed
/// before returning. The error names the path so an operator can fix permissions or the mount.
pub fn check_state_dir(path: &Path) -> Result<(), ConfigError> {
    let shown = path.display().to_string();
    let writable = |source| ConfigError::NotWritable {
        path: shown.clone(),
        source,
    };
    std::fs::create_dir_all(path).map_err(writable)?;
    let probe = path.join(format!(".fw-state-probe-{}", std::process::id()));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(writable)?;
    drop(file);
    std::fs::remove_file(&probe).map_err(writable)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    fn scratch(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("fw-config-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn invalid_table(error: ConfigError) -> String {
        match error {
            ConfigError::Invalid { table, .. } => table,
            other => panic!("expected an invalid-value error, got {other}"),
        }
    }

    #[test]
    fn an_empty_file_gets_the_defaults() {
        let config = Config::parse("").unwrap();
        assert_eq!(config, Config::default());
        assert_eq!(config.server.listen, "127.0.0.1:8080");
        assert!(config.tls.is_none());
    }

    #[test]
    fn an_unknown_top_level_table_is_refused_by_name() {
        let error = Config::parse("[serevr]\nlisten = \"x\"\n").unwrap_err();
        assert!(error.to_string().contains("serevr"), "{error}");
    }

    #[test]
    fn an_unknown_key_inside_a_table_is_refused() {
        let error = Config::parse("[server]\nlissten = \"127.0.0.1:1\"\n").unwrap_err();
        assert!(error.to_string().contains("lissten"), "{error}");
    }

    #[test]
    fn an_out_of_range_value_names_its_table() {
        let error = Config::parse("[server]\nmax_connections = 0\n").unwrap_err();
        assert_eq!(invalid_table(error), "[server]");
    }

    #[test]
    fn a_per_address_limit_above_the_global_limit_is_refused() {
        let error = Config::parse("[server]\nmax_connections = 4\nmax_connections_per_addr = 5\n")
            .unwrap_err();
        assert!(
            error.to_string().contains("max_connections_per_addr"),
            "{error}"
        );
    }

    #[test]
    fn a_token_hash_of_the_wrong_shape_names_the_token() {
        let text = "[[auth.tokens]]\nname = \"peer-a\"\ntoken_sha256 = \"ABC\"\n";
        let error = Config::parse(text).unwrap_err();
        let table = invalid_table(error);
        assert!(table.contains("peer-a"), "{table}");
    }

    #[test]
    fn duplicate_token_names_and_hashes_are_refused() {
        let twice =
            |name: &str| format!("[[auth.tokens]]\nname = \"{name}\"\ntoken_sha256 = \"{HASH}\"\n");
        let same_name = Config::parse(&format!("{}{}", twice("a"), twice("a"))).unwrap_err();
        assert!(
            same_name.to_string().contains("name is used"),
            "{same_name}"
        );
        let same_hash = Config::parse(&format!(
            "[[auth.tokens]]\nname = \"a\"\ntoken_sha256 = \"{HASH}\"\n[[auth.tokens]]\nname = \"b\"\ntoken_sha256 = \"{HASH}\"\n"
        ))
        .unwrap_err();
        assert!(
            same_hash.to_string().contains("same as an earlier"),
            "{same_hash}"
        );
    }

    #[test]
    fn an_unknown_log_level_names_the_log_table() {
        let error = Config::parse("[log]\nlevel = \"loud\"\n").unwrap_err();
        assert_eq!(invalid_table(error), "[log]");
    }

    #[test]
    fn a_tls_table_needs_both_paths() {
        assert!(Config::parse("[tls]\ncert = \"c.pem\"\n").is_err());
        let error = Config::parse("[tls]\ncert = \"c.pem\"\nkey = \"\"\n").unwrap_err();
        assert_eq!(invalid_table(error), "[tls]");
    }

    #[test]
    fn a_file_over_the_size_cap_is_refused_before_parsing() {
        let dir = scratch("size");
        let path = dir.join("big.toml");
        std::fs::write(&path, "#".repeat(MAX_CONFIG_BYTES as usize + 1)).unwrap();
        assert!(matches!(load(&path), Err(ConfigError::TooLarge { .. })));
    }

    #[test]
    #[cfg(unix)]
    fn a_tls_key_readable_by_others_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("mode");
        let key = dir.join("key.pem");
        std::fs::write(&key, "not really a key").unwrap();
        let config = dir.join("fw.toml");
        std::fs::write(
            &config,
            format!("[tls]\ncert = \"c.pem\"\nkey = \"{}\"\n", key.display()),
        )
        .unwrap();

        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).unwrap();
        match load(&config) {
            Err(ConfigError::InsecureFile { mode, .. }) => assert_eq!(mode, 0o644),
            other => panic!("expected the key to be refused, got {other:?}"),
        }

        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(load(&config).is_ok());
    }

    #[test]
    fn a_missing_file_is_an_io_error_naming_the_path() {
        let dir = scratch("missing");
        let error = load(&dir.join("absent.toml")).unwrap_err();
        assert!(matches!(error, ConfigError::Io { .. }));
        assert!(error.to_string().contains("absent.toml"));
    }

    #[test]
    fn a_non_loopback_listen_needs_tls() {
        let error = Config::parse("[server]\nlisten = \"0.0.0.0:8080\"\n").unwrap_err();
        assert_eq!(invalid_table(error), "[server]");
        assert!(Config::parse("[server]\nlisten = \"[::1]:8080\"\n").is_ok());
        let with_tls =
            "[server]\nlisten = \"0.0.0.0:8443\"\n[tls]\ncert = \"c.pem\"\nkey = \"k.pem\"\n";
        assert!(Config::parse(with_tls).is_ok());
    }

    #[test]
    fn an_empty_state_dir_is_refused() {
        let error = Config::parse("[state]\ndir = \"\"\n").unwrap_err();
        assert_eq!(invalid_table(error), "[state]");
        assert!(Config::parse("[state]\ndir = \"./state\"\n").is_ok());
        assert!(Config::parse("[state]\nunknown = 1\n").is_err());
    }

    #[test]
    fn a_state_dir_is_created_and_probed_without_leaving_a_file() {
        let dir = scratch("state").join("nested").join("state");
        check_state_dir(&dir).unwrap();
        assert!(dir.is_dir());
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            0,
            "the probe is removed"
        );
    }

    #[test]
    fn a_file_cannot_be_used_as_a_state_dir() {
        let dir = scratch("state-file");
        let file = dir.join("not-a-dir");
        std::fs::write(&file, "keep").unwrap();
        let error = check_state_dir(&file).unwrap_err();
        assert!(matches!(error, ConfigError::NotWritable { .. }));
        assert!(error.to_string().contains("not-a-dir"), "{error}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "keep");
    }

    #[test]
    fn the_request_timeout_is_range_checked() {
        for bad in ["0", "601", "99999"] {
            let text = format!("[server]\nrequest_timeout_secs = {bad}\n");
            assert!(Config::parse(&text).is_err(), "{bad}");
        }
        assert!(Config::parse("[server]\nrequest_timeout_secs = 600\n").is_ok());
    }
}
