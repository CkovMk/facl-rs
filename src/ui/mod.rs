/*
 * Copyright 2026 Chekhov Ma <maqike@qq.com>
 * SPDX-License-Identifier: Apache-2.0
 */
//! The rendering layer. This is the only module that depends on ratatui; it
//! draws `model`/`app` state read-only and never mutates the app directly.

mod acl_table;
mod dialogs;
mod entry_editor;
mod layout;
mod status;

pub use layout::draw;

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use crate::app::{App, Screen};
    use crate::cli::{Cli, MaskPolicy};
    use crate::model::{Acl, AclEntry, EntryKind, Permissions, Scope};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    static CTR: AtomicU32 = AtomicU32::new(0);

    fn make_app() -> (std::path::PathBuf, App) {
        let id = CTR.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir()
            .join(format!("facl_rs_ui_{}_{}", std::process::id(), id));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = |s: &str| Permissions::from_perm_string(s).unwrap();
        let acl = Acl::new(vec![
            AclEntry::new(Scope::Access, EntryKind::Owner, None, p("rwx")),
            AclEntry::new(
                Scope::Access,
                EntryKind::NamedUser,
                Some("nobody".into()),
                p("rwx"),
            ),
            AclEntry::new(Scope::Access, EntryKind::OwningGroup, None, p("r-x")),
            AclEntry::new(Scope::Access, EntryKind::Mask, None, p("rwx")),
            AclEntry::new(Scope::Access, EntryKind::Other, None, p("r--")),
        ]);
        crate::backend::write(dir.to_str().unwrap(), &acl, false).unwrap();
        let cli = Cli {
            path: dir.to_str().unwrap().to_string(),
            read_only: false,
            mask: MaskPolicy::Auto,
        };
        let app = App::new(cli).unwrap();
        (dir, app)
    }

    fn render_text(app: &App) -> String {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| super::draw(f, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol().to_string())
            .collect()
    }

    #[test]
    fn renders_editor() {
        let (dir, app) = make_app();
        let text = render_text(&app);
        assert!(text.contains("ACL entries"), "missing table title:\n{text}");
        assert!(
            text.contains("Effective after mask"),
            "missing entry editor:\n{text}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn renders_preview_and_help() {
        let (dir, mut app) = make_app();
        app.screen = Screen::Preview;
        let text = render_text(&app);
        assert!(text.contains("Preview changes"), "missing preview:\n{text}");
        app.screen = Screen::Help;
        let text = render_text(&app);
        assert!(text.contains("key bindings"), "missing help:\n{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn input_dialog_field_is_highlighted() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        use ratatui::style::Modifier;

        let (dir, mut app) = make_app();
        // Open the change-owner dialog.
        app.handle_event(crate::event::AppEvent::Key(KeyEvent::new(
            KeyCode::Char('o'),
            KeyModifiers::NONE,
        )));
        assert!(
            matches!(app.dialog, Some(crate::app::Dialog::Input(_))),
            "expected the input dialog"
        );
        let buf = buffer_of(100, 30, &app);
        let mut found = false;
        for y in 0..buf.area.height {
            let row: String = (0..buf.area.width)
                .map(|x| buf.get(x, y).symbol().to_string())
                .collect();
            if let Some(pos) = row.find("Value:") {
                let cell = buf.get(pos as u16, y);
                assert!(
                    cell.style().add_modifier.intersects(Modifier::REVERSED),
                    "input field should be highlighted (reversed):\n{row}"
                );
                found = true;
                break;
            }
        }
        assert!(found, "Value: line not found in buffer");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn header_shows_recursive_ownership_and_symmetric_kinds() {
        let (dir, mut app) = make_app();
        app.recursive = true;
        let text = render_text(&app);
        // Recursive toggle sits on the Type line with a parenthetical key hint.
        assert!(text.contains("recursive (t)"), "missing recursive hint:\n{text}");
        // Ownership line uses parenthetical key hints and a UGO label.
        assert!(text.contains("Owner (o):"), "missing owner line:\n{text}");
        assert!(text.contains("Group (g):"), "missing group line:\n{text}");
        assert!(text.contains("UGO:"), "missing UGO label:\n{text}");
        // The duplicate bottom ownership line is gone; the key legend is there.
        assert!(!text.contains("Change owner"), "bottom owner/group should be removed:\n{text}");
        assert!(text.contains("Save (S)"), "missing bottom key legend:\n{text}");
        // Symmetric kind labels in the table.
        assert!(text.contains("owning-user"), "missing owning-user:\n{text}");
        assert!(text.contains("owning-group"), "missing owning-group:\n{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recursive_hidden_for_a_single_file() {
        let id = CTR.fetch_add(1, Ordering::SeqCst);
        let file = std::env::temp_dir()
            .join(format!("facl_rs_file_{}_{}", std::process::id(), id));
        let _ = std::fs::remove_file(&file);
        std::fs::File::create(&file).unwrap();
        let cli = Cli {
            path: file.to_str().unwrap().to_string(),
            read_only: false,
            mask: MaskPolicy::Auto,
        };
        let app = App::new(cli).unwrap();
        let text = render_text(&app);
        assert!(
            !text.contains("recursive (t)"),
            "recursive should be hidden for a single file:\n{text}"
        );
        assert!(text.contains("Owner (o):"), "ownership line still shown:\n{text}");
        let _ = std::fs::remove_file(&file);
    }

    fn buffer_of(width: u16, height: u16, app: &App) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| super::draw(f, app)).unwrap();
        terminal.backend().buffer().clone()
    }

    #[test]
    fn restricts_and_top_left_anchors_on_a_large_terminal() {
        let (dir, app) = make_app();
        let buf = buffer_of(200, 80, &app);
        // The panel is capped at 120x40 and pinned to the top-left corner, so
        // the region to the right of and below it stays blank.
        assert_eq!(buf.get(195, 5).symbol(), " ", "right margin should be blank");
        assert_eq!(buf.get(5, 75).symbol(), " ", "bottom margin should be blank");
        let content: String = buf.content.iter().map(|c| c.symbol().to_string()).collect();
        assert!(content.contains("ACL entries"), "panel should render:\n{content}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn warns_when_the_terminal_is_too_small() {
        let (dir, app) = make_app();
        let buf = buffer_of(40, 10, &app);
        let content: String = buf.content.iter().map(|c| c.symbol().to_string()).collect();
        assert!(
            content.contains("Terminal too small"),
            "should warn rather than cram the UI:\n{content}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
