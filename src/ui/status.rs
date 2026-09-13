/*
 * Copyright 2026 Chekhov Ma <maqike@qq.com>
 * SPDX-License-Identifier: Apache-2.0
 */
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, StatusLevel};

/// The single-line status bar at the bottom of the screen.
pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let style = match app.status.level {
        StatusLevel::Info => Style::default(),
        StatusLevel::Ok => Style::default().fg(Color::Green),
        StatusLevel::Error => Style::default().fg(Color::Red),
    };
    let paragraph = Paragraph::new(Span::styled(app.status.message.clone(), style))
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}
