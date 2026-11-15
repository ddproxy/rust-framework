use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::model::ViewModel;

/// Lays out a view model. All text decisions were made in the model.
pub fn draw(frame: &mut Frame, view: &ViewModel) {
    let [body, footer] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());
    let mut lines: Vec<Line> = view
        .rows
        .iter()
        .map(|row| Line::from(row.as_str()))
        .collect();
    lines.push(Line::default());
    let status_style = if view.paused {
        Style::default()
            .add_modifier(Modifier::BOLD)
            .fg(ratatui::style::Color::Yellow)
    } else {
        Style::default()
    };
    lines.push(Line::from(view.status.as_str()).style(status_style));
    let block = Block::bordered().title(view.title.as_str());
    frame.render_widget(Paragraph::new(Text::from(lines)).block(block), body);
    frame.render_widget(Paragraph::new(view.footer.as_str()), footer);
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;

    use super::*;
    use crate::model::Dashboard;

    fn screen(view: &ViewModel, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| draw(frame, view)).unwrap();
        text_of(terminal.backend().buffer())
    }

    fn text_of(buffer: &Buffer) -> String {
        let area = buffer.area;
        (area.y..area.y + area.height)
            .map(|y| {
                (area.x..area.x + area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_live_screen_draws_rows_status_and_footer() {
        let mut dashboard = Dashboard::new(&["peer-a"]);
        dashboard.tick();
        let text = screen(&dashboard.view(Instant::now()), 50, 8);
        assert!(text.contains("fw-app top"));
        assert!(text.contains("peer-a"));
        assert!(text.contains("live, update 1"));
        assert!(text.contains("q quit"));
    }

    #[test]
    fn a_paused_screen_says_how_old_its_data_is() {
        let mut dashboard = Dashboard::new(&["host-1"]);
        let t0 = Instant::now();
        dashboard.toggle_pause(t0);
        let text = screen(&dashboard.view(t0 + Duration::from_secs(7)), 60, 8);
        assert!(text.contains("paused: showing data from 7s ago"), "{text}");
    }

    #[test]
    fn a_tiny_terminal_draws_without_panicking() {
        let view = Dashboard::new(&["peer-a"]).view(Instant::now());
        let _ = screen(&view, 1, 1);
        let _ = screen(&view, 0, 0);
        let _ = screen(&view, 12, 2);
    }
}
