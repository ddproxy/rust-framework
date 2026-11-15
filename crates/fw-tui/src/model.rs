//! State and view model for the dashboard. Nothing here touches a terminal, so the freeze-on-pause
//! behaviour is tested without a tty.

use std::time::{Duration, Instant};

use crate::sanitize::printable;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub count: u64,
}

/// Live rows keep moving. Pausing keeps a copy of them and the moment it was taken, and the view
/// shows that copy with its age. A paused screen therefore never looks live.
#[derive(Debug, Clone)]
pub struct Dashboard {
    live: Vec<Row>,
    frozen: Option<(Vec<Row>, Instant)>,
    seed: u64,
    ticks: u64,
}

impl Dashboard {
    pub fn new(names: &[&str]) -> Self {
        Self {
            live: names
                .iter()
                .map(|name| Row {
                    name: (*name).to_owned(),
                    count: 0,
                })
                .collect(),
            frozen: None,
            seed: 0x9e37_79b9_7f4a_7c15,
            ticks: 0,
        }
    }

    /// Advances the live rows by a demo step. A real feed would apply its own updates here.
    pub fn tick(&mut self) {
        for row in &mut self.live {
            self.seed = self
                .seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            row.count += (self.seed >> 33) % 4;
        }
        self.ticks += 1;
    }

    pub fn paused(&self) -> bool {
        self.frozen.is_some()
    }

    /// Freezes a copy of the live rows, or resumes when already paused.
    pub fn toggle_pause(&mut self, now: Instant) {
        self.frozen = match self.frozen.take() {
            Some(_) => None,
            None => Some((self.live.clone(), now)),
        };
    }

    pub fn view(&self, now: Instant) -> ViewModel {
        match &self.frozen {
            Some((rows, taken)) => ViewModel::build(
                rows,
                Some(now.saturating_duration_since(*taken)),
                self.ticks,
            ),
            None => ViewModel::build(&self.live, None, self.ticks),
        }
    }
}

/// What the screen shows, as plain text. The renderer only lays these lines out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewModel {
    pub title: String,
    pub rows: Vec<String>,
    pub status: String,
    pub footer: String,
    pub paused: bool,
}

impl ViewModel {
    fn build(rows: &[Row], paused_for: Option<Duration>, ticks: u64) -> Self {
        let rows = rows
            .iter()
            .map(|row| format!("{:<16} {:>10}", printable(&row.name), row.count))
            .collect();
        let (status, paused) = match paused_for {
            Some(age) => (
                format!("paused: showing data from {}s ago", age.as_secs()),
                true,
            ),
            None => (format!("live, update {ticks}"), false),
        };
        Self {
            title: "fw-app top".to_owned(),
            rows,
            status,
            footer: "p pause/resume   q quit".to_owned(),
            paused,
        }
    }

    /// The whole screen as text, one line per row. Tests assert against this.
    pub fn plain_text(&self) -> String {
        let mut lines = vec![self.title.clone()];
        lines.extend(self.rows.iter().cloned());
        lines.push(self.status.clone());
        lines.push(self.footer.clone());
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paused_view_keeps_the_rows_from_the_moment_of_the_pause() {
        let mut dashboard = Dashboard::new(&["peer-a", "host-1"]);
        dashboard.tick();
        let t0 = Instant::now();
        dashboard.toggle_pause(t0);
        let frozen_text = dashboard.view(t0).plain_text();
        for _ in 0..5 {
            dashboard.tick();
        }
        let later = dashboard.view(t0 + Duration::from_secs(10));
        assert!(later.paused);
        assert_eq!(later.rows, dashboard.view(t0).rows, "the rows did not move");
        assert!(later.status.contains("10s ago"), "{}", later.status);
        assert!(frozen_text.contains("peer-a"));
    }

    #[test]
    fn resuming_shows_the_live_rows_again() {
        let mut dashboard = Dashboard::new(&["peer-a"]);
        let t0 = Instant::now();
        dashboard.toggle_pause(t0);
        dashboard.tick();
        dashboard.tick();
        let paused_rows = dashboard.view(t0).rows;
        dashboard.toggle_pause(t0);
        assert!(!dashboard.paused());
        let live = dashboard.view(t0);
        assert!(!live.paused);
        assert_ne!(live.rows, paused_rows, "the live count moved while paused");
        assert!(live.status.starts_with("live"));
    }

    #[test]
    fn names_from_the_network_are_sanitized_in_the_view() {
        let dashboard = Dashboard::new(&["peer\u{1b}[31m"]);
        let view = dashboard.view(Instant::now());
        assert!(!view.plain_text().contains('\u{1b}'));
        assert!(view.rows[0].starts_with("peer?"));
    }

    #[test]
    fn an_empty_dashboard_renders_its_chrome() {
        let view = Dashboard::new(&[]).view(Instant::now());
        assert!(view.rows.is_empty());
        assert!(view.plain_text().contains("q quit"));
    }
}
