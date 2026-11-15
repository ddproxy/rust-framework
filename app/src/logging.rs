//! Logging for `serve`: stderr and/or an append-only file, text or JSON, level from the config
//! unless `RUST_LOG` is set.

use std::fs::{File, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use fw_config::{LogConfig, LogFormat};
use tracing_subscriber::fmt::{self, MakeWriter};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};

type BoxedLayer = Box<dyn Layer<Registry> + Send + Sync>;

/// Starts the global subscriber. Fails when the log file cannot be opened, so the caller can
/// report it before exiting.
pub fn init(config: &LogConfig) -> Result<(), String> {
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(&config.level))
        .unwrap_or_else(|_| EnvFilter::new("info"));
    let mut layers: Vec<BoxedLayer> = Vec::new();
    let file = match &config.file {
        Some(path) => {
            let file = open_log(Path::new(path))
                .map_err(|e| format!("cannot open log file {path}: {e}"))?;
            Some(Arc::new(Mutex::new(file)))
        }
        None => None,
    };
    if config.stderr || file.is_none() {
        let ansi = io::stderr().is_terminal();
        layers.push(layer(config.format, io::stderr, ansi));
    }
    if let Some(file) = file {
        layers.push(layer(config.format, FileSink(file), false));
    }
    // The filter goes last so it sees every event; placed first it would not type-check against
    // the boxed layers.
    tracing_subscriber::registry()
        .with(layers)
        .with(filter)
        .try_init()
        .map_err(|e| format!("cannot start logging: {e}"))
}

fn layer<W>(format: LogFormat, writer: W, ansi: bool) -> BoxedLayer
where
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    let base = fmt::layer().with_writer(writer).with_ansi(ansi);
    match format {
        LogFormat::Json => base.json().boxed(),
        LogFormat::Text => base.boxed(),
    }
}

fn open_log(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // A symlink at the log path would send lines to a file the operator did not name.
        options.mode(0o640).custom_flags(libc::O_NOFOLLOW);
    }
    options.open(path)
}

/// One shared file handle. Each event takes the lock for its write.
#[derive(Clone)]
struct FileSink(Arc<Mutex<File>>);

impl<'a> MakeWriter<'a> for FileSink {
    type Writer = FileWriter;

    fn make_writer(&'a self) -> Self::Writer {
        FileWriter(Arc::clone(&self.0))
    }
}

struct FileWriter(Arc<Mutex<File>>);

impl Write for FileWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        // A failed log write must not fail the request that logged it, so the error is dropped.
        let mut file = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = file.write_all(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn scratch(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("fw-app-log-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_new_log_file_is_created_0640_and_appended_to() {
        let dir = scratch("mode");
        let path = dir.join("fw.log");
        let mut file = open_log(&path).unwrap();
        file.write_all(b"one\n").unwrap();
        drop(file);
        let mut file = open_log(&path).unwrap();
        file.write_all(b"two\n").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o640);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo\n");
    }

    #[test]
    fn a_symlink_at_the_log_path_is_refused_and_its_target_untouched() {
        let dir = scratch("symlink");
        let target = dir.join("target");
        std::fs::write(&target, "keep\n").unwrap();
        let path = dir.join("fw.log");
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(open_log(&path).is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "keep\n");
    }
}
