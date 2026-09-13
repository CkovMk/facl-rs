/*
 * Copyright 2026 Chekhov Ma <maqike@qq.com>
 * SPDX-License-Identifier: Apache-2.0
 */
mod app;
mod backend;
mod cli;
mod event;
mod model;
mod ui;

use std::io;
use std::time::Duration;

use anyhow::Result;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    // Build the app before entering raw mode so startup errors print cleanly.
    let mut app = app::App::new(cli)?;

    event::enable()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, &mut app);

    // Always restore the terminal, even if the loop errored.
    event::disable()?;
    terminal.show_cursor()?;

    result
}

/// The main loop: render, then poll for an event and let the app react.
fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut app::App,
) -> Result<()> {
    while app.running {
        terminal.draw(|f| ui::draw(f, app))?;
        match event::next(Duration::from_millis(16))? {
            Some(evt) => app.handle_event(evt),
            None => {}
        }
    }
    Ok(())
}
