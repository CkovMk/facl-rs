/*
 * Copyright 2026 Chekhov Ma <maqike@qq.com>
 * SPDX-License-Identifier: Apache-2.0
 */
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{AddDialog, ConfirmDialog, Dialog, InputDialog};

/// Render a centered modal overlay for the active dialog within `area`.
pub fn draw(f: &mut Frame, dialog: &Dialog, area: Rect) {
    let (title, lines) = match dialog {
        Dialog::Add(ad) => build_add(ad),
        Dialog::Input(id) => build_input(id),
        Dialog::Confirm(cd) => build_confirm(cd),
    };

    let width = lines
        .iter()
        .map(|l| l.width() as u16)
        .max()
        .unwrap_or(20)
        .saturating_add(4)
        .max(title.len() as u16 + 4);
    let height = (lines.len() as u16).saturating_add(2);
    let popup = centered(area, width, height);

    f.render_widget(Clear, popup);
    let block = Block::default().borders(Borders::ALL).title(title);
    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    f.render_widget(paragraph, popup);
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

fn label(text: &str) -> Span {
    Span::styled(text.to_string(), Style::default().fg(Color::DarkGray))
}

/// Reverse (highlight) the span when `active`.
fn field(span: Span, active: bool) -> Span {
    if active {
        span.patch_style(Style::default().add_modifier(Modifier::REVERSED))
    } else {
        span
    }
}

fn radio(checked: bool, text: &str) -> Span {
    let mark = if checked { "(*)" } else { "( )" };
    Span::raw(format!("{mark} {text}  "))
}

fn build_add(ad: &AddDialog) -> (String, Vec<Line>) {
    let mut lines = Vec::new();

    // Scope
    let mut scope_spans = vec![label("Scope:")];
    scope_spans.push(radio(ad.scope == crate::model::Scope::Access, "Access"));
    if ad.can_default {
        scope_spans.push(radio(ad.scope == crate::model::Scope::Default, "Default"));
    }
    lines.push(Line::from(
        scope_spans
            .iter()
            .map(|s| field(s.clone(), ad.cursor == 0))
            .collect::<Vec<Span>>(),
    ));

    // Type
    let mut kind_spans = vec![label("Type:")];
    kind_spans.push(radio(ad.is_user, "Named user"));
    kind_spans.push(radio(!ad.is_user, "Named group"));
    lines.push(Line::from(
        kind_spans
            .iter()
            .map(|s| field(s.clone(), ad.cursor == 1))
            .collect::<Vec<Span>>(),
    ));

    // Name
    let name_text = format!("Name: [{}]", ad.name);
    lines.push(Line::from(field(
        Span::raw(name_text),
        ad.cursor == 2,
    )));

    // Permissions; each bit is highlighted individually under the cursor.
    let mut perm_spans = vec![label("Permissions:")];
    let bits: [(bool, &str); 3] = [
        (ad.read, "Read"),
        (ad.write, "Write"),
        (ad.execute, "Execute"),
    ];
    for (i, (on, name)) in bits.iter().enumerate() {
        let mark = if *on { "[x]" } else { "[ ]" };
        let span = Span::raw(format!(" {mark} {name} "));
        perm_spans.push(field(span, ad.cursor == 3 + i));
    }
    lines.push(Line::from(perm_spans));

    if let Some(err) = &ad.error {
        lines.push(Line::from(Span::styled(
            err.clone(),
            Style::default().fg(Color::Red),
        )));
    }
    lines.push(Line::from(label(
        "Arrows/Tab: move   Space: toggle   Enter: Add   Esc: Cancel",
    )));

    ("Add ACL entry".to_string(), lines)
}

fn build_input(id: &InputDialog) -> (String, Vec<Line>) {
    let mut lines = Vec::new();
    // The value field always has focus in this dialog, so it is highlighted
    // (reversed) just like the active field in the Add dialog.
    lines.push(Line::from(field(
        Span::raw(format!("Value: [{}]", id.value)),
        true,
    )));
    if let Some(err) = &id.error {
        lines.push(Line::from(Span::styled(
            err.clone(),
            Style::default().fg(Color::Red),
        )));
    }
    lines.push(Line::from(label("Enter: OK    Esc: Cancel")));
    (id.title.clone(), lines)
}

fn build_confirm(cd: &ConfirmDialog) -> (String, Vec<Line>) {
    let mut lines = Vec::new();
    for body_line in cd.body.lines() {
        lines.push(Line::from(Span::raw(body_line.to_string())));
    }
    lines.push(Line::from(""));
    for (i, choice) in cd.choices.iter().enumerate() {
        let prefix = if i == cd.cursor { "> " } else { "  " };
        lines.push(Line::from(Span::styled(
            format!("{prefix}({}) {}", choice.key, choice.label),
            if i == cd.cursor {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            },
        )));
    }
    (cd.title.clone(), lines)
}
