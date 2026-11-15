use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::model::Dashboard;
use crate::render;
use crate::terminal::TerminalGuard;

/// Longest a key press waits before the screen is redrawn anyway.
const POLL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    TogglePause,
}

pub fn action_for(event: KeyEvent) -> Option<Action> {
    // Some terminals also report key releases; acting on them would double every press.
    if event.kind != KeyEventKind::Press {
        return None;
    }
    match (event.code, event.modifiers) {
        (KeyCode::Char('q') | KeyCode::Esc, _) => Some(Action::Quit),
        (KeyCode::Char('c'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::Quit)
        }
        (KeyCode::Char('p' | ' '), _) => Some(Action::TogglePause),
        _ => None,
    }
}

/// Runs the demo dashboard until the user quits or `stop` is set. The caller sets `stop` from a
/// signal handler; the guard restores the terminal on the way out either way.
pub fn run_demo(stop: &AtomicBool, tick: Duration) -> io::Result<()> {
    let _guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut dashboard = Dashboard::new(&["peer-a", "peer-b", "host-1"]);
    let mut last_tick = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        let now = Instant::now();
        if now.duration_since(last_tick) >= tick {
            dashboard.tick();
            last_tick = now;
        }
        terminal.draw(|frame| render::draw(frame, &dashboard.view(now)))?;
        if event::poll(POLL.min(tick))? {
            if let Event::Key(key) = event::read()? {
                match action_for(key) {
                    Some(Action::Quit) => break,
                    Some(Action::TogglePause) => dashboard.toggle_pause(Instant::now()),
                    None => {}
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyEventState;

    use super::*;

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn the_keys_map_to_their_actions() {
        let none = KeyModifiers::NONE;
        assert_eq!(
            action_for(press(KeyCode::Char('q'), none)),
            Some(Action::Quit)
        );
        assert_eq!(action_for(press(KeyCode::Esc, none)), Some(Action::Quit));
        assert_eq!(
            action_for(press(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Action::Quit)
        );
        assert_eq!(
            action_for(press(KeyCode::Char('p'), none)),
            Some(Action::TogglePause)
        );
        assert_eq!(
            action_for(press(KeyCode::Char(' '), none)),
            Some(Action::TogglePause)
        );
        assert_eq!(action_for(press(KeyCode::Char('x'), none)), None);
        assert_eq!(
            action_for(press(KeyCode::Char('c'), none)),
            None,
            "plain c is not quit"
        );
    }

    #[test]
    fn key_releases_do_nothing() {
        let mut release = press(KeyCode::Char('q'), KeyModifiers::NONE);
        release.kind = KeyEventKind::Release;
        assert_eq!(action_for(release), None);
    }
}
