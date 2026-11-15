//! Terminal setup that is always undone: when the guard drops (including while unwinding), when
//! any thread panics, and never twice.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

use crossterm::cursor::Show;
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static HOOK: Once = Once::new();

/// Holds the alternate screen and raw mode. Dropping it restores the terminal.
pub struct TerminalGuard {
    _private: (),
}

impl TerminalGuard {
    pub fn enter() -> io::Result<Self> {
        // A panic prints its message to the screen the user is looking at. Restore first, so the
        // message lands on the normal screen where it can be read.
        HOOK.call_once(|| {
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                restore();
                previous(info);
            }));
        });
        execute!(io::stdout(), EnterAlternateScreen)?;
        ACTIVE.store(true, Ordering::SeqCst);
        if let Err(error) = terminal::enable_raw_mode() {
            restore();
            return Err(error);
        }
        Ok(Self { _private: () })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore();
    }
}

/// Leaves raw mode and the alternate screen. Does nothing unless a guard is active, so the panic
/// hook and the guard can both call it.
pub fn restore() {
    if ACTIVE.swap(false, Ordering::SeqCst) {
        let _ = terminal::disable_raw_mode();
        let mut out = io::stdout();
        let _ = execute!(out, LeaveAlternateScreen, Show);
        let _ = out.flush();
    }
}
