//! Logging for `serve`: stderr and/or a file, text or JSON, level from the config unless
//! `RUST_LOG` is set. The file sink reopens on SIGHUP for rotation and counts failed writes.

use std::any::Any;
use std::fs::{File, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use fw_config::{LogConfig, LogFormat};
use tracing_subscriber::fmt::{self, MakeWriter};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};

type BoxedLayer = Box<dyn Layer<Registry> + Send + Sync>;

const WARN_INTERVAL: Duration = Duration::from_secs(60);
static WRITE_ERRORS: OnceLock<Arc<AtomicU64>> = OnceLock::new();
static LOG_FILE: OnceLock<Arc<FileSink>> = OnceLock::new();

/// Failed writes to the log file since startup.
pub fn write_errors() -> u64 {
    WRITE_ERRORS
        .get()
        .map_or(0, |counter| counter.load(Ordering::Relaxed))
}

/// Starts the global subscriber. Fails when the log file cannot be opened, so the caller can
/// report it before exiting.
pub fn init(config: &LogConfig) -> Result<(), String> {
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(&config.level))
        .unwrap_or_else(|_| EnvFilter::new("info"));
    let mut layers: Vec<BoxedLayer> = Vec::new();
    let file = match &config.file {
        Some(path) => {
            let errors = WRITE_ERRORS.get_or_init(|| Arc::new(AtomicU64::new(0)));
            let sink = FileSink::open(Path::new(path), Arc::clone(errors))
                .map_err(|e| format!("cannot open log file {path}: {e}"))?;
            let _ = LOG_FILE.set(Arc::new(sink.clone()));
            Some(sink)
        }
        None => None,
    };
    if config.stderr || file.is_none() {
        let ansi = io::stderr().is_terminal();
        layers.push(layer(config.format, io::stderr, ansi));
    }
    if let Some(file) = file {
        layers.push(layer(config.format, file, false));
    }
    // The filter goes last so it sees every event; placed first it would not type-check against
    // the boxed layers.
    tracing_subscriber::registry()
        .with(layers)
        .with(filter)
        .try_init()
        .map_err(|e| format!("cannot start logging: {e}"))
}

/// Reopens the log file after rotation. Fails clearly when no file sink is configured.
pub fn reopen() -> Result<(), String> {
    let Some(file) = LOG_FILE.get() else {
        return Err("no log file configured".to_owned());
    };
    file.reopen()
        .map_err(|e| format!("cannot reopen log file {}: {e}", file.file.path.display()))
}

/// Watches for SIGHUP and reopens the log file so a rotated path gets a fresh descriptor.
#[cfg(unix)]
pub async fn watch_hangup() {
    use tokio::signal::unix::SignalKind;
    let mut signals = match tokio::signal::unix::signal(SignalKind::hangup()) {
        Ok(signals) => signals,
        Err(error) => {
            tracing::warn!(%error, "cannot watch SIGHUP");
            return;
        }
    };
    while signals.recv().await.is_some() {
        match reopen() {
            Ok(()) => tracing::info!("log file reopened"),
            Err(error) => tracing::warn!("{error}"),
        }
    }
}

/// SIGHUP has no signal API here, so the file is only opened once at startup.
#[cfg(not(unix))]
pub async fn watch_hangup() {}

/// Logs panics with their location through tracing. Before logging is set up, the message also
/// goes to stderr so it is not lost.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let payload = panic_message(info.payload());
        let location = info
            .location()
            .map_or_else(|| "unknown".to_owned(), |site| site.to_string());
        tracing::error!(%payload, %location, "panic");
        if !tracing::dispatcher::has_been_set() {
            eprintln!("fw-app: panic at {location}: {payload}");
        }
    }));
}

fn panic_message(payload: &(dyn Any + Send)) -> &str {
    if let Some(message) = payload.downcast_ref::<&str>() {
        message
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message
    } else {
        "non-string panic payload"
    }
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

/// The path and its current descriptor, shared by the writer and the SIGHUP handler.
struct LogFile {
    path: PathBuf,
    file: Mutex<File>,
}

impl LogFile {
    fn open(path: &Path) -> io::Result<Self> {
        Ok(Self {
            path: path.to_owned(),
            file: Mutex::new(open_log(path)?),
        })
    }

    fn reopen(&self) -> io::Result<()> {
        let file = open_log(&self.path)?;
        *self.file.lock().unwrap_or_else(PoisonError::into_inner) = file;
        Ok(())
    }
}

/// One shared file handle. Each event takes the lock for its write.
#[derive(Clone)]
struct FileSink {
    file: Arc<LogFile>,
    errors: Arc<AtomicU64>,
    last_warning: Arc<Mutex<Instant>>,
}

impl FileSink {
    fn open(path: &Path, errors: Arc<AtomicU64>) -> io::Result<Self> {
        Ok(Self {
            file: Arc::new(LogFile::open(path)?),
            errors,
            last_warning: Arc::new(Mutex::new(Instant::now() - WARN_INTERVAL)),
        })
    }

    fn reopen(&self) -> io::Result<()> {
        self.file.reopen()
    }
}

impl<'a> MakeWriter<'a> for FileSink {
    type Writer = FileWriter;

    fn make_writer(&'a self) -> Self::Writer {
        FileWriter(self.clone())
    }
}

struct FileWriter(FileSink);

impl Write for FileWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let result = self
            .0
            .file
            .file
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .write_all(data);
        if let Err(error) = result {
            self.0.errors.fetch_add(1, Ordering::Relaxed);
            let now = Instant::now();
            if take_warning_slot(&self.0.last_warning, now) {
                tracing::warn!(%error, "log file write failed");
            }
        }
        // A failed log write must not fail the request that logged it, so the error is dropped.
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn take_warning_slot(last: &Mutex<Instant>, now: Instant) -> bool {
    let mut last = last.lock().unwrap_or_else(PoisonError::into_inner);
    if now.saturating_duration_since(*last) >= WARN_INTERVAL {
        *last = now;
        true
    } else {
        false
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

    #[test]
    fn a_reopen_switches_writes_to_the_new_file_after_rotation() {
        let dir = scratch("reopen");
        let path = dir.join("fw.log");
        let rotated = dir.join("fw.log.1");
        let sink = FileSink::open(&path, Arc::new(AtomicU64::new(0))).unwrap();
        sink.make_writer().write_all(b"before\n").unwrap();
        std::fs::rename(&path, &rotated).unwrap();
        sink.make_writer().write_all(b"during\n").unwrap();
        sink.reopen().unwrap();
        sink.make_writer().write_all(b"after\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(&rotated).unwrap(),
            "before\nduring\n"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "after\n");
    }

    #[test]
    fn failed_writes_are_counted_without_failing_the_writer() {
        let dir = scratch("errors");
        let path = dir.join("fw.log");
        std::fs::write(&path, "keep\n").unwrap();
        let errors = Arc::new(AtomicU64::new(0));
        // A read-only descriptor cannot be written to, so every write fails.
        let sink = FileSink {
            file: Arc::new(LogFile {
                path: path.clone(),
                file: Mutex::new(File::open(&path).unwrap()),
            }),
            errors: Arc::clone(&errors),
            last_warning: Arc::new(Mutex::new(Instant::now() - WARN_INTERVAL)),
        };
        let mut writer = sink.make_writer();
        writer.write_all(b"one\n").unwrap();
        writer.write_all(b"two\n").unwrap();
        assert_eq!(errors.load(Ordering::Relaxed), 2);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep\n");
    }

    #[test]
    fn a_warning_is_allowed_at_most_once_a_minute() {
        let last = Mutex::new(Instant::now() - 2 * WARN_INTERVAL);
        let now = Instant::now();
        assert!(take_warning_slot(&last, now));
        let later = now + Duration::from_secs(30);
        assert!(!take_warning_slot(&last, later));
        assert!(take_warning_slot(&last, now + WARN_INTERVAL));
    }

    #[test]
    fn panic_payloads_are_rendered_as_text() {
        assert_eq!(panic_message(&"boom"), "boom");
        assert_eq!(panic_message(&String::from("boom")), "boom");
        assert_eq!(panic_message(&42u8), "non-string panic payload");
    }
}
