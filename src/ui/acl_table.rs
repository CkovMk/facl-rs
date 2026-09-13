/*
 * Copyright 2026 Chekhov Ma <maqike@qq.com>
 * SPDX-License-Identifier: Apache-2.0
 */
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};
use ratatui::Frame;

use crate::app::App;
use crate::model::EntryKind;

/// The left panel: a table of the visible ACL entries with the A/D scope
/// column and a highlight on the selected row.
pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let filter_label = match app.list_filter {
        crate::app::ListFilter::All => "All",
        crate::app::ListFilter::Access => "Access",
        crate::app::ListFilter::Default => "Default",
    };
    let title = format!("ACL entries [{}]", filter_label);

    let rows: Vec<Row> = app
        .visible_entries()
        .iter()
        .map(|e| {
            let identity = match e.kind {
                EntryKind::Owner => app.owner.clone(),
                EntryKind::OwningGroup => app.group.clone(),
                _ => e.identity.clone().unwrap_or_default(),
            };
            let scope_style = if e.scope == crate::model::Scope::Default {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(e.scope.short().to_string()).style(scope_style),
                Cell::from(e.kind.short().to_string()),
                Cell::from(identity),
                Cell::from(e.permissions.to_string()),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Length(13),
            Constraint::Fill(1),
            Constraint::Length(5),
        ],
    )
    .header(Row::new(vec!["S", "Type", "Identity", "Perm"]).style(
        Style::default().add_modifier(Modifier::BOLD),
    ))
    .highlight_style(Style::default().bg(Color::DarkGray))
    .highlight_symbol("> ")
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(title),
    );

    let mut state = TableState::default();
    if app.selected < app.visible_entries().len() {
        state.select(Some(app.selected));
    } else {
        state.select(None);
    }
    f.render_stateful_widget(table, area, &mut state);
}
