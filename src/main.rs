mod action;
mod ansible_cfg;
mod app;
mod config;
mod history;
mod input;
mod playbook_settings;
mod projects;
mod run;
mod run_store;
mod theme;
mod ui;

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;

use crate::action::Action;
use crate::app::App;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = setup_terminal()?;
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut app = App::new(cwd);
    let (tx, mut rx) = mpsc::unbounded_channel::<Action>();
    input::spawn_input_listener(tx.clone());

    let mut tick = tokio::time::interval(Duration::from_millis(250));

    loop {
        terminal.draw(|f| ui::render(f, &app))?;

        tokio::select! {
            _ = tick.tick() => {
                app.update(Action::Tick, &tx);
                if app.take_full_redraw_request() {
                    terminal.clear()?;
                }
            }
            Some(action) = rx.recv() => {
                app.update(action, &tx);
                if app.take_full_redraw_request() {
                    terminal.clear()?;
                }
                if app.should_quit {
                    while let Ok(pending) = rx.try_recv() {
                        app.update(pending, &tx);
                    }
                    break;
                }
            }
        }
    }

    drop(rx);
    drop(tx);
    restore_terminal(&mut terminal)?;
    Ok(())
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()
}
