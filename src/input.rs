use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use tokio::sync::mpsc::UnboundedSender;

use crate::action::Action;

static INPUT_PAUSED: AtomicBool = AtomicBool::new(false);

pub fn set_input_paused(paused: bool) {
    INPUT_PAUSED.store(paused, Ordering::SeqCst);
}

pub fn spawn_input_listener(tx: UnboundedSender<Action>) {
    tokio::task::spawn_blocking(move || loop {
        if tx.is_closed() {
            break;
        }
        if INPUT_PAUSED.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(40));
            continue;
        }
        match event::poll(Duration::from_millis(100)) {
            Ok(false) => continue,
            Ok(true) => match event::read() {
                Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                    let action = match (key.code, key.modifiers) {
                        (KeyCode::Tab, _) => Some(Action::NextView),
                        (KeyCode::BackTab, _) => Some(Action::PrevView),
                        (KeyCode::Left, _) => Some(Action::SettingsDecrease),
                        (KeyCode::Right, _) => Some(Action::SettingsIncrease),
                        (KeyCode::Esc, _) => Some(Action::CloseRuntimePrompt),
                        (KeyCode::Up, _) => Some(Action::MoveUp),
                        (KeyCode::Down, _) => Some(Action::MoveDown),
                        (KeyCode::PageUp, _) => Some(Action::LogScrollPageUp),
                        (KeyCode::PageDown, _) => Some(Action::LogScrollPageDown),
                        (KeyCode::End, _) => Some(Action::LogFollowLatest),
                        (KeyCode::Enter, _) => Some(Action::SelectRuntimeCandidate),
                        (KeyCode::Backspace, _) => Some(Action::Backspace),
                        (KeyCode::Char('s'), KeyModifiers::CONTROL) => {
                            Some(Action::SaveInventoryEditor)
                        }
                        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Some(Action::Quit),
                        (KeyCode::Char(ch), _) => Some(Action::CharInput(ch)),
                        _ => None,
                    };
                    if let Some(action) = action {
                        if tx.send(action).is_err() {
                            break;
                        }
                    }
                }
                Ok(Event::Mouse(mouse)) => {
                    let Ok((cols, rows)) = crossterm::terminal::size() else {
                        continue;
                    };
                    let Some((row, viewport_height)) =
                        logs_viewport_hit(cols, rows, mouse.column, mouse.row)
                    else {
                        continue;
                    };

                    let action = match mouse.kind {
                        MouseEventKind::Down(MouseButton::Left) => Some(Action::LogMouseDown {
                            row,
                            viewport_height,
                        }),
                        MouseEventKind::Drag(MouseButton::Left) => Some(Action::LogMouseDrag {
                            row,
                            viewport_height,
                        }),
                        MouseEventKind::Up(MouseButton::Left) => Some(Action::LogMouseUp),
                        MouseEventKind::ScrollUp => Some(Action::LogScrollUp),
                        MouseEventKind::ScrollDown => Some(Action::LogScrollDown),
                        _ => None,
                    };

                    if let Some(action) = action {
                        if tx.send(action).is_err() {
                            break;
                        }
                    }
                }
                Ok(_) => {}
                Err(err) => {
                    let _ = tx.send(Action::Error(format!("input error: {err}")));
                    break;
                }
            },
            Err(err) => {
                let _ = tx.send(Action::Error(format!("input polling error: {err}")));
                break;
            }
        }
    });
}

fn logs_viewport_hit(cols: u16, rows: u16, mouse_col: u16, mouse_row: u16) -> Option<(u16, u16)> {
    if cols < 10 || rows < 8 {
        return None;
    }

    let body_y: u16 = 3;
    let body_h = rows.saturating_sub(6);
    if body_h < 3 {
        return None;
    }

    let main_w = cols.saturating_mul(40) / 100;
    let logs_x = main_w;
    let logs_w = cols.saturating_sub(main_w);
    if logs_w < 3 {
        return None;
    }

    let logs_y = body_y;
    let logs_h = body_h;

    // Restrict to log content area (inside borders).
    let content_x0 = logs_x.saturating_add(1);
    let content_x1 = logs_x.saturating_add(logs_w.saturating_sub(1));
    let content_y0 = logs_y.saturating_add(1);
    let content_y1 = logs_y.saturating_add(logs_h.saturating_sub(1));
    if mouse_col < content_x0 || mouse_col >= content_x1 {
        return None;
    }
    if mouse_row < content_y0 || mouse_row >= content_y1 {
        return None;
    }

    let row = mouse_row.saturating_sub(content_y0);
    let viewport_height = logs_h.saturating_sub(2);
    Some((row, viewport_height))
}
