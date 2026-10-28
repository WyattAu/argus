//! argus-tui — the terminal surface of the argus platform.
//!
//! A thin client: it seeds from the REST API and lives on the WebSocket
//! stream, exactly like the web shell. Keybindings come from `tui-actions`
//! — the same chord table every estate TUI shares (`q` quit, `r` refresh).

#![forbid(unsafe_code)]

use std::io::stdout;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clap::Parser;
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Terminal;
use tui_actions::{Action, Key, Keymap, KeyEvent as TuiKeyEvent, Mode};

mod state;
mod ui;
mod ws;

use state::TuiState;
use ui::run_ui;

/// CLI arguments.
#[derive(Debug, Clone, clap::Parser)]
#[command(name = "argus-tui", version, about = "argus terminal surface")]
struct Args {
    /// Base URL of the argus core (REST seed).
    #[arg(long, default_value = "http://127.0.0.1:8420")]
    url: String,
    /// WebSocket stream URL.
    #[arg(long, default_value = "ws://127.0.0.1:8420/api/v1/stream")]
    ws: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let state = Arc::new(Mutex::new(TuiState::new()));
    let quit = Arc::new(AtomicBool::new(false));
    let keymap = build_keymap();

    // Stream + seed live on a runtime task; the UI owns the main thread.
    let ws_handle = runtime.spawn(ws::run(
        args.ws.clone(),
        args.url.clone(),
        Arc::clone(&state),
        Arc::clone(&quit),
    ));

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(ratatui::backend::CrosstermBackend::new(stdout()))?;

    let result = run_ui(&mut terminal, &state, &quit, &keymap);

    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    ws_handle.abort();
    runtime.shutdown_timeout(Duration::from_secs(2));
    result
}

/// The estate chord table: single source of truth for TUI keybindings.
fn build_keymap() -> Keymap {
    let mut map = Keymap::new(Mode::Normal);
    let _ = map.bind("q", Action::new("app.quit"), "Quit argus-tui");
    let _ = map.bind("ctrl+c", Action::new("app.quit"), "Quit argus-tui");
    let _ = map.bind("r", Action::new("view.refresh"), "Refresh observations");
    map
}

fn to_tui_event(key: crossterm::event::KeyEvent) -> Option<TuiKeyEvent> {
    use crossterm::event::KeyModifiers;
    use tui_actions::Modifiers as TM;
    let mut modifiers = TM::NONE;
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        modifiers = TM::CTRL;
    } else if key.modifiers.contains(KeyModifiers::ALT) {
        modifiers = TM::ALT;
    } else if key.modifiers.contains(KeyModifiers::SHIFT) {
        modifiers = TM::SHIFT;
    }
    let key = match key.code {
        crossterm::event::KeyCode::Char(c) => Key::Char(c),
        crossterm::event::KeyCode::Enter => Key::Enter,
        crossterm::event::KeyCode::Esc => Key::Esc,
        crossterm::event::KeyCode::Tab => Key::Tab,
        crossterm::event::KeyCode::BackTab => Key::BackTab,
        crossterm::event::KeyCode::Backspace => Key::Backspace,
        crossterm::event::KeyCode::Delete => Key::Delete,
        crossterm::event::KeyCode::Insert => Key::Insert,
        crossterm::event::KeyCode::Up => Key::Up,
        crossterm::event::KeyCode::Down => Key::Down,
        crossterm::event::KeyCode::Left => Key::Left,
        crossterm::event::KeyCode::Right => Key::Right,
        crossterm::event::KeyCode::Home => Key::Home,
        crossterm::event::KeyCode::End => Key::End,
        crossterm::event::KeyCode::PageUp => Key::PageUp,
        crossterm::event::KeyCode::PageDown => Key::PageDown,
        crossterm::event::KeyCode::F(n) => Key::Function(n),
        _ => return None,
    };
    Some(TuiKeyEvent::with(key, modifiers))
}
