/*
 * Copyright 2026 Chekhov Ma <maqike@qq.com>
 * SPDX-License-Identifier: Apache-2.0
 */
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::app::App;
use crate::model::{EntryKind, PermBit};

/// The right panel: details for the selected entry, with the three permission
/// checkboxes (the active one under the cursor is reversed) and the effective
/// permission after the mask is applied.
pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title("Selected entry");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(e) = app.selected_entry() else {
        let empty = Paragraph::new(Line::from("No entry selected.").style(
            Style::default().fg(Color::DarkGray),
        ));
        f.render_widget(empty, inner);
        return;
    };

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("Scope: ", Style::default().fg(Color::DarkGray)),
        Span::raw(e.scope.label().to_string()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Type: ", Style::default().fg(Color::DarkGray)),
        Span::raw(e.kind.label().to_string()),
    ]));

    let identity = match e.kind {
        EntryKind::Owner => ("Owner", app.owner.clone()),
        EntryKind::OwningGroup => ("Owning group", app.group.clone()),
        _ => ("Identity", e.identity.clone().unwrap_or_default()),
    };
    lines.push(Line::from(vec![
        Span::styled(format!("{}: ", identity.0), Style::default().fg(Color::DarkGray)),
        Span::raw(identity.1),
    ]));
    lines.push(Line::from(""));

    // Permission checkboxes; the one under the cursor is reversed.
    let mut spans = vec![Span::styled(
        "Permissions:",
        Style::default().fg(Color::DarkGray),
    )];
    for (i, bit) in PermBit::ALL.iter().enumerate() {
        let checked = bit.get(e.permissions);
        let mark = if checked { "[x]" } else { "[ ]" };
        let style = if i == app.perm_cursor {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        spans.push(Span::styled(format!(" {mark} {} ", bit.label()), style));
    }
    lines.push(Line::from(spans));

    let effective = app.working.effective(e);
    lines.push(Line::from(""));
    let mut effective_line = format!("Effective after mask: {effective}");
    if effective != e.permissions {
        if let Some(mask) = app.working.mask(e.scope) {
            effective_line.push_str(&format!("  (mask::{mask})"));
        }
    }
    lines.push(Line::from(Span::styled(
        effective_line,
        Style::default().fg(Color::Cyan),
    )));

    let paragraph = Paragraph::new(lines).style(Style::default().fg(Color::White));
    f.render_widget(paragraph, inner);
}
