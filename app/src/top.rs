//! `fw-app top`: the demo dashboard. It runs on a blocking thread, and a SIGTERM sets the stop
//! flag so the terminal guard restores the screen.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub async fn run() -> Result<(), String> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    tokio::spawn(async move {
        fw_serve::shutdown_signal().await;
        flag.store(true, Ordering::Relaxed);
    });
    tokio::task::spawn_blocking(move || fw_tui::run::run_demo(&stop, Duration::from_secs(1)))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("terminal: {e}"))
}
