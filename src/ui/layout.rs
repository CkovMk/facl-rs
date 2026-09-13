/*
 * Copyright 2026 Chekhov Ma <maqike@qq.com>
 * SPDX-License-Identifier: Apache-2.0
 */
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Screen};

use super::{acl_table, dialogs, entry_editor, status};

/// Comfortable maximum panel size; a larger terminal keeps the panel at this
/// size anchored to the top-left corner.
const MAX_WIDTH: u16 = 120;
const MAX_HEIGHT: u16 = 40;
/// Below this, a "terminal too small" notice is shown instead of the UI.
const MIN_WIDTH: u16 = 72;
const MIN_HEIGHT: u16 = 22;
/// Fixed width of the selected-entry editor pane.
const EDITOR_WIDTH: u16 = 50;

/// The single render entry point: size-check, dispatch by screen, then
/// overlay any dialog. The panel is capped at [`MAX_WIDTH`]x[`MAX_HEIGHT`]
/// and anchored to the top-left corner of the terminal; if the terminal is
/// too small a notice is shown instead of cramming the layout in.
pub fn draw(f: &mut Frame, app: &App) {
    let full = f.size();
    f.render_widget(Clear, full);
    if full.width < MIN_WIDTH || full.height < MIN_HEIGHT {
        draw_too_small(f, full);
        return;
    }
    let area = restrict(full);
    match app.screen {
        Screen::Preview => draw_preview(f, app, area),
        Screen::Help => draw_help(f, app, area),
        Screen::Editor => draw_editor(f, app, area),
    }
    if let Some(dialog) = &app.dialog {
        dialogs::draw(f, dialog, area);
    }
}

/// Clamp the terminal to the max panel size, anchored to the top-left corner.
fn restrict(full: Rect) -> Rect {
    let width = full.width.min(MAX_WIDTH);
    let height = full.height.min(MAX_HEIGHT);
    Rect::new(full.x, full.y, width, height)
}

/// Center a `width`x`height` rect (clamped to `area`) inside `area`.
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}

/// Shown when the terminal is smaller than the minimum: the requested minimum
/// and the actual size, rather than a squashed editor.
fn draw_too_small(f: &mut Frame, area: Rect) {
    let msg = format!(
        "Terminal too small: need at least {}x{}, have {}x{}. Enlarge the terminal.",
        MIN_WIDTH, MIN_HEIGHT, area.width, area.height
    );
    if area.width < 20 || area.height < 3 {
        f.render_widget(Paragraph::new(msg), area);
        return;
    }
    let popup = centered(area, area.width.min(58), area.height.min(6));
    let block = Block::default()
        .borders(Borders::ALL)
        .title("facl-rs");
    f.render_widget(
        Paragraph::new(msg)
            .block(block)
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Center),
        popup,
    );
}

fn draw_editor(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    draw_header(f, app, chunks[0]);

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(EDITOR_WIDTH)])
        .split(chunks[1]);
    acl_table::draw(f, app, main[0]);
    entry_editor::draw(f, app, main[1]);

    draw_tips(f, app, chunks[2]);
    status::draw(f, app, chunks[3]);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let mut title = Line::from(Span::styled(
        "POSIX ACL Editor",
        Style::default().add_modifier(Modifier::BOLD),
    ));
    if app.is_modified() {
        title.push_span(Span::styled(
            "  MODIFIED",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    }
    if app.read_only {
        title.push_span(Span::styled(
            "  (read-only)",
            Style::default().fg(Color::Yellow),
        ));
    }
    let path = Line::from(format!("Path: {}", app.object.path));

    // Type line; the recursive toggle only applies to directories, so it is
    // hidden entirely for a single file.
    let mut type_line = Line::from(vec![
        Span::styled("Type: ", Style::default().fg(Color::DarkGray)),
        Span::raw(&app.object.file_type),
    ]);
    if app.object.is_directory {
        let mark = if app.recursive { "[x]" } else { "[ ]" };
        type_line.push_span(Span::styled(
            format!("   {mark} recursive (t)"),
            if app.recursive {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ));
    }

    // Ownership line with inline key hints; stat falls back to the numeric
    // uid/gid when a name cannot be resolved.
    let owner_line = Line::from(vec![
        Span::styled("Owner (o): ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("{}   ", app.owner)),
        Span::styled("Group (g): ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("{}   ", app.group)),
        Span::styled("UGO: ", Style::default().fg(Color::DarkGray)),
        Span::raw(app.object.mode.clone()),
    ]);

    let lines = vec![title, path, type_line, owner_line];
    f.render_widget(Paragraph::new(lines), area);
}

/// The key-binding legend, anchored at the bottom of the panel.
fn draw_tips(f: &mut Frame, _app: &App, area: Rect) {
    let line = Line::from(Span::styled(
        "Add (a)  Delete (d)  Identity (e)  Filter (f)  preview (p)  Save (S)  Reload (R)  Help (?)  Quit (q)",
        Style::default().fg(Color::DarkGray),
    ));
    f.render_widget(Paragraph::new(line), area);
}

fn draw_preview(f: &mut Frame, app: &App, area: Rect) {
    let lines: Vec<Line> = app.build_preview().iter().map(|s| {
        let style = if s.starts_with('+') {
            Style::default().fg(Color::Green)
        } else if s.starts_with('-') {
            Style::default().fg(Color::Red)
        } else if s.starts_with('!') {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        Line::from(Span::styled(s.clone(), style))
    }).collect();
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Preview changes  (Enter = apply, Esc/q/P = back)");
    f.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_help(f: &mut Frame, _app: &App, area: Rect) {
    let lines: Vec<Line> = App::help_lines()
        .iter()
        .map(|s| Line::from(Span::raw(s.to_string())))
        .collect();
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Help  (Esc/q/? = back)");
    f.render_widget(Paragraph::new(lines).block(block), area);
}
