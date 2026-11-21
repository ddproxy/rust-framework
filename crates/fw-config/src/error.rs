use std::fmt;

/// Why a config was refused. `Invalid` names the table at fault so the message can be acted on
/// without reading the whole file.
#[derive(Debug)]
pub enum ConfigError {
    Io {
        path: String,
        source: std::io::Error,
    },
    TooLarge {
        path: String,
        limit: u64,
    },
    Parse(String),
    Invalid {
        table: String,
        reason: String,
    },
    InsecureFile {
        what: String,
        path: String,
        mode: u32,
    },
    NotWritable {
        path: String,
        source: std::io::Error,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "cannot read {path}: {source}"),
            Self::TooLarge { path, limit } => {
                write!(f, "{path} is larger than {limit} bytes")
            }
            Self::Parse(message) => write!(f, "invalid TOML: {message}"),
            Self::Invalid { table, reason } => write!(f, "{table}: {reason}"),
            Self::InsecureFile { what, path, mode } => write!(
                f,
                "{what} {path} has mode {mode:03o}; it must not be accessible to group or others (chmod 600)"
            ),
            Self::NotWritable { path, source } => {
                write!(f, "state directory {path} is not writable: {source}")
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } | Self::NotWritable { source, .. } => Some(source),
            _ => None,
        }
    }
}
