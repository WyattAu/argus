//! Rendering + input loop for the TUI.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ratatui::crossterm::event::{self, Event as CEvent, KeyEventKind};
use ratatui::prelude::{Color, Constraint, Direction, Frame, Layout, Line, Rect, Span, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;
use tui_actions::{Action, Keymap};

use crate::state::TuiState;

/// One draw + one input poll cycle. Returns when the user quits.
///
/// # Errors
/// Returns terminal I/O errors.
pub fn run_ui(
    terminal: &mut Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    state: &Arc<Mutex<TuiState>>,
    quit: &Arc<std::sync::atomic::AtomicBool>,
    keymap: &Keymap,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        if quit.load(Ordering::Relaxed) {
            return Ok(());
        }
        terminal.draw(|frame| draw(frame, state))?;
        if event::poll(Duration::from_millis(120))? {
            if let CEvent::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let Some(event) = crate::to_tui_event(key) else {
                        continue;
                    };
                    for action in keymap.resolve(&event) {
                        if action.is(&Action::new("app.quit")) {
                            quit.store(true, Ordering::Relaxed);
                        }
                        if action.is(&Action::new("view.refresh")) {
                            if let Ok(mut guard) = state.lock() {
                                guard.touch();
                            }
                        }
                    }
                }
            }
        }
    }
}

fn draw(frame: &mut Frame, state: &Arc<Mutex<TuiState>>) {
    let guard = match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    let rects: Vec<Rect> = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(8), Constraint::Length(1)])
        .split(frame.area())
        .to_vec();
    let (Some(&header_area), Some(&body_area), Some(&help_area)) =
        (rects.first(), rects.get(1), rects.get(2))
    else {
        return;
    };

    let freshness = if guard.stale { "STALE" } else { "LIVE" };
    let freshness_color = if guard.stale { Color::Yellow } else { Color::Green };
    let header = Paragraph::new(Line::from(vec![
        Span::styled(" argus-tui ", Style::default().fg(Color::Green).bold()),
        Span::styled(format!("● {freshness} "), Style::default().fg(freshness_color)),
        Span::raw(guard.quote_lines()),
    ]))
    .block(Block::default().borders(Borders::ALL));
    frame.render_widget(header, header_area);

    let columns: Vec<Rect> = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(body_area)
        .to_vec();
    let (Some(&left_area), Some(&right_area)) = (columns.first(), columns.get(1)) else {
        return;
    };

    let observations: Vec<Line> = guard.observation_lines().iter().map(Line::raw).collect();
    let obs_widget = Paragraph::new(observations)
        .block(Block::default().borders(Borders::ALL).title(" world intel "));
    frame.render_widget(obs_widget, left_area);

    let alerts: Vec<Line> = guard
        .alert_lines()
        .iter()
        .map(|line| Line::styled(line.clone(), Style::default().fg(Color::Yellow)))
        .collect();
    let alert_widget = Paragraph::new(alerts)
        .block(Block::default().borders(Borders::ALL).title(" alerts "));
    frame.render_widget(alert_widget, right_area);

    let help = Paragraph::new(Line::from(Span::styled(
        " q quit · r refresh",
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(help, help_area);
}
