/*
 * Copyright 2026 Chekhov Ma <maqike@qq.com>
 * SPDX-License-Identifier: Apache-2.0
 */
//! Terminal event loop: poll crossterm and surface events to the app.

use std::io;
use std::time::Duration;

use crossterm::event::{Event as CrosstermEvent, KeyEvent, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::ExecutableCommand;

/// A normalized event the app reacts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEvent {
    /// A key-down event.
    Key(KeyEvent),
    /// The terminal was resized.
    Resize(u16, u16),
    /// No input arrived within the poll window (drives redraws).
    Tick,
}

impl AppEvent {
    /// The key pressed by an [`AppEvent::Key`], if any.
    pub fn key(&self) -> Option<KeyEvent> {
        match self {
            AppEvent::Key(k) => Some(*k),
            _ => None,
        }
    }
}

/// Enter raw mode and the alternate screen buffer.
pub fn enable() -> io::Result<()> {
    enable_raw_mode()?;
    io::stdout().execute(crossterm::terminal::EnterAlternateScreen)?;
    Ok(())
}

/// Leave raw mode and the alternate screen buffer.
pub fn disable() -> io::Result<()> {
    io::stdout().execute(crossterm::terminal::LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}

/// Wait up to `timeout` for the next event. Returns `None` on timeout.
pub fn next(timeout: Duration) -> io::Result<Option<AppEvent>> {
    if !crossterm::event::poll(timeout)? {
        return Ok(None);
    }
    match crossterm::event::read()? {
        // Ignore key-up / repeat events; we act on key-down only.
        CrosstermEvent::Key(k) if k.kind == KeyEventKind::Press => Ok(Some(AppEvent::Key(k))),
        CrosstermEvent::Resize(w, h) => Ok(Some(AppEvent::Resize(w, h))),
        _ => Ok(Some(AppEvent::Tick)),
    }
}
