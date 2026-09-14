/*
 * Copyright 2026 Chekhov Ma <maqike@qq.com>
 * SPDX-License-Identifier: Apache-2.0
 */
use std::path::PathBuf;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::backend;
use crate::cli::{Cli, MaskPolicy};
use crate::event::AppEvent;
use crate::model::{Acl, AclEntry, EntryKind, ObjectInfo, Permissions, PermBit, Scope};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Editor,
    Preview,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    List,
    Editor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListFilter {
    All,
    Access,
    Default,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel {
    Info,
    Ok,
    Error,
}

#[derive(Debug, Clone)]
pub struct Status {
    pub level: StatusLevel,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum Dialog {
    Add(AddDialog),
    Input(InputDialog),
    Confirm(ConfirmDialog),
}

#[derive(Debug, Clone)]
pub struct AddDialog {
    /// Whether the object supports a default ACL (i.e. it is a directory).
    pub can_default: bool,
    pub scope: Scope,
    /// `true` for a named user, `false` for a named group.
    pub is_user: bool,
    pub name: String,
    pub read: bool,
    pub write: bool,
    pub execute: bool,
    /// 0 = Scope, 1 = Kind, 2 = Name, 3 = Read, 4 = Write, 5 = Execute.
    pub cursor: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InputDialog {
    pub title: String,
    pub target: InputTarget,
    pub value: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum InputTarget {
    Owner,
    Group,
    /// Edit the identity of an existing named entry.
    Identity(Scope, EntryKind, Option<String>),
}

#[derive(Debug, Clone)]
pub struct ConfirmDialog {
    pub title: String,
    pub body: String,
    pub cursor: usize,
    pub choices: Vec<ConfirmChoice>,
}

#[derive(Debug, Clone)]
pub struct ConfirmChoice {
    pub key: char,
    pub label: String,
    pub action: ConfirmAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    Quit,
    Save,
    SaveAndQuit,
    Cancel,
    DeleteEntry,
    Reload,
}

/// The whole application state: the object being edited, the original and
/// working ACLs, ownership, UI focus/selection, and any open dialog.
pub struct App {
    pub running: bool,
    pub object: ObjectInfo,
    /// The ACL as first read from disk; the baseline for change detection.
    pub original: Acl,
    pub original_owner: String,
    pub original_group: String,
    /// The ACL being edited; becomes `original` after a save or reload.
    pub working: Acl,
    pub owner: String,
    pub group: String,
    pub screen: Screen,
    pub focus: Focus,
    /// Index into the *visible* (filtered) entry list.
    pub selected: usize,
    /// Which permission bit the cursor is on: 0 = read, 1 = write, 2 = execute.
    pub perm_cursor: usize,
    pub list_filter: ListFilter,
    pub status: Status,
    pub dialog: Option<Dialog>,
    pub read_only: bool,
    pub mask_policy: MaskPolicy,
    /// When true, saving applies the ACL to every descendant of the object.
    pub recursive: bool,
    pub backup_path: Option<PathBuf>,
}

impl App {
    pub fn new(cli: Cli) -> Result<Self> {
        let handle = backend::read(&cli.path)?;
        let object = handle.object;
        let original_owner = object.owner.clone();
        let original_group = object.group.clone();
        let owner = original_owner.clone();
        let group = original_group.clone();
        // Enter read-only mode when requested on the command line or when the
        // current user is not allowed to modify this object's ACL. Checking up
        // front means editing keys are disabled and the reason is shown,
        // rather than letting a save fail later.
        let writable = backend::can_modify(&object).unwrap_or(false);
        let read_only = cli.read_only || !writable;
        let status = if !writable && !cli.read_only {
            Status {
                level: StatusLevel::Info,
                message: "read-only: you do not own this object; ACL changes are not permitted"
                    .to_string(),
            }
        } else {
            Status {
                level: StatusLevel::Info,
                message: "loaded".to_string(),
            }
        };
        Ok(Self {
            running: true,
            object,
            original: handle.acl.clone(),
            original_owner,
            original_group,
            working: handle.acl,
            owner,
            group,
            screen: Screen::Editor,
            focus: Focus::List,
            selected: 0,
            perm_cursor: 0,
            list_filter: ListFilter::All,
            status,
            dialog: None,
            read_only,
            mask_policy: cli.mask,
            recursive: false,
            backup_path: None,
        })
    }

    /// The entries currently shown in the list, honoring the active filter.
    pub fn visible_entries(&self) -> Vec<&AclEntry> {
        let all = self.working.entries();
        match self.list_filter {
            ListFilter::All => all.iter().collect(),
            ListFilter::Access => all
                .iter()
                .filter(|e| e.scope == Scope::Access)
                .collect(),
            ListFilter::Default => all
                .iter()
                .filter(|e| e.scope == Scope::Default)
                .collect(),
        }
    }

    pub fn selected_entry(&self) -> Option<&AclEntry> {
        self.visible_entries().get(self.selected).copied()
    }

    fn selected_working_index(&self) -> Option<usize> {
        let target = *self.visible_entries().get(self.selected)?;
        self.working
            .entries()
            .iter()
            .position(|e| core::ptr::eq(e, target))
    }

    fn selected_entry_mut(&mut self) -> Option<&mut AclEntry> {
        let idx = self.selected_working_index()?;
        self.working.entries_mut().get_mut(idx)
    }

    /// Whether any pending change (ACL or ownership) differs from the disk.
    pub fn is_modified(&self) -> bool {
        self.working != self.original
            || self.owner != self.original_owner
            || self.group != self.original_group
    }

    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = Status {
            level: StatusLevel::Info,
            message: message.into(),
        };
    }

    pub fn set_ok(&mut self, message: impl Into<String>) {
        self.status = Status {
            level: StatusLevel::Ok,
            message: message.into(),
        };
    }

    pub fn set_error(&mut self, message: impl Into<String>) {
        self.status = Status {
            level: StatusLevel::Error,
            message: message.into(),
        };
    }

    fn visible_count(&self) -> usize {
        self.visible_entries().len()
    }

    /// Move the selection by `delta`, wrapping around the visible list.
    fn select(&mut self, delta: isize) {
        let n = self.visible_count();
        if n == 0 {
            self.selected = 0;
            return;
        }
        self.selected = ((self.selected as isize + delta).rem_euclid(n as isize)) as usize;
        self.sync_status_to_selection();
    }

    fn clamp_selection(&mut self) {
        let n = self.visible_count();
        if n == 0 {
            self.selected = 0;
        } else if self.selected >= n {
            self.selected = n - 1;
        }
    }

    /// Point the status bar at the selected entry, showing requested vs.
    /// effective permission and any mask capping.
    fn sync_status_to_selection(&mut self) {
        let (who, requested, effective, scope) = match self.selected_entry() {
            Some(e) => (e.who(), e.permissions, self.working.effective(e), e.scope),
            None => return,
        };
        let mut message = format!("{who}  requested {requested}  effective {effective}");
        if effective != requested {
            if let Some(mask) = self.working.mask(scope) {
                message.push_str(&format!("  (capped by mask::{mask})"));
            }
        }
        self.status = Status {
            level: StatusLevel::Info,
            message,
        };
    }

    /// Warnings for entries whose requested permission exceeds their scope's
    /// mask, relevant when the mask policy is manual/preserve.
    pub fn mask_warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        for scope in [Scope::Access, Scope::Default] {
            if let Some(mask) = self.working.mask(scope) {
                for e in self.working.in_scope(scope) {
                    if e.kind.is_maskable()
                        && e.kind != EntryKind::Mask
                        && !mask.covers(e.permissions)
                    {
                        warnings.push(format!(
                            "{} {}: requested {} exceeds mask::{mask}",
                            e.scope.label(),
                            e.who(),
                            e.permissions
                        ));
                    }
                }
            }
        }
        warnings
    }

    /// React to one polled terminal event.
    pub fn handle_event(&mut self, event: AppEvent) {
        if let Some(key) = event.key() {
            self.handle_key(key);
        }
        // Resize and Tick need no state change; the terminal redraws each loop.
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if let Some(dialog) = self.dialog.take() {
            self.handle_owned_dialog(dialog, key);
            return;
        }
        match self.screen {
            Screen::Editor => self.handle_editor_key(key),
            Screen::Preview => self.handle_preview_key(key),
            Screen::Help => self.handle_help_key(key),
        }
    }

    /// Own the (taken-out) dialog, step it, and put it back unless it finished.
    fn handle_owned_dialog(&mut self, mut dialog: Dialog, key: KeyEvent) {
        let finished = match &mut dialog {
            Dialog::Add(ad) => self.step_add(ad, key),
            Dialog::Input(id) => self.step_input(id, key),
            Dialog::Confirm(cd) => self.step_confirm(cd, key),
        };
        if !finished {
            self.dialog = Some(dialog);
        }
    }

    /// Step the Add dialog. Returns `true` when the dialog should close.
    fn step_add(&mut self, ad: &mut AddDialog, key: KeyEvent) -> bool {
        // The field list: Scope, Type, Name, Read, Write, Execute.
        const FIELDS: usize = 6;
        if key.code == KeyCode::Esc {
            return true;
        }
        if key.code == KeyCode::Enter {
            self.submit_add(ad);
            return true;
        }
        ad.error = None;
        // Navigation: arrows and Tab walk the field list and wrap around at
        // both ends. Space is the only key that changes a value.
        match key.code {
            KeyCode::Up | KeyCode::Left | KeyCode::BackTab => {
                ad.cursor = (ad.cursor + FIELDS - 1) % FIELDS;
                return false;
            }
            KeyCode::Down | KeyCode::Right | KeyCode::Tab => {
                ad.cursor = (ad.cursor + 1) % FIELDS;
                return false;
            }
            _ => {}
        }
        // The Name field is free text; type into it directly.
        if ad.cursor == 2 {
            match key.code {
                KeyCode::Char(c) => ad.name.push(c),
                KeyCode::Backspace => {
                    ad.name.pop();
                }
                _ => {}
            }
            return false;
        }
        // Space toggles the field under the cursor.
        if key.code == KeyCode::Char(' ') {
            match ad.cursor {
                0 => {
                    if ad.can_default {
                        ad.scope = match ad.scope {
                            Scope::Access => Scope::Default,
                            Scope::Default => Scope::Access,
                        };
                    }
                }
                1 => ad.is_user = !ad.is_user,
                3 => ad.read = !ad.read,
                4 => ad.write = !ad.write,
                5 => ad.execute = !ad.execute,
                _ => {}
            }
        }
        false
    }

    fn submit_add(&mut self, ad: &mut AddDialog) {
        let name = ad.name.trim().to_string();
        if name.is_empty() {
            ad.error = Some("name is required".to_string());
            return;
        }
        let kind = if ad.is_user {
            EntryKind::NamedUser
        } else {
            EntryKind::NamedGroup
        };
        if !self.identity_resolves(kind, &name) {
            ad.error = Some(format!("'{name}' not found"));
            return;
        }
        if self.working.find(ad.scope, kind, Some(&name)).is_some() {
            ad.error = Some("entry already exists".to_string());
            return;
        }
        let entry = AclEntry::new(
            ad.scope,
            kind,
            Some(name.clone()),
            Permissions::new(ad.read, ad.write, ad.execute),
        );
        self.add_entry(entry);
        self.set_ok(format!("added {name}"));
    }

    fn step_input(&mut self, id: &mut InputDialog, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => true,
            KeyCode::Enter => {
                let value = id.value.trim().to_string();
                if value.is_empty() {
                    id.error = Some("value is required".to_string());
                    return false;
                }
                match &id.target {
                    InputTarget::Owner => {
                        if backend::lookup_user(&value).is_err() {
                            id.error = Some(format!("no user '{value}'"));
                            return false;
                        }
                        self.owner = value;
                        self.set_ok("owner updated (pending)");
                    }
                    InputTarget::Group => {
                        if backend::lookup_group(&value).is_err() {
                            id.error = Some(format!("no group '{value}'"));
                            return false;
                        }
                        self.group = value;
                        self.set_ok("group updated (pending)");
                    }
                    InputTarget::Identity(scope, kind, current) => {
                        if !self.identity_resolves(*kind, &value) {
                            id.error = Some(format!("'{value}' not found"));
                            return false;
                        }
                        if let Some(existing) = self.working.find(*scope, *kind, Some(&value)) {
                            if existing.identity.as_deref() != current.as_deref() {
                                id.error = Some("entry already exists".to_string());
                                return false;
                            }
                        }
                        if let Some(pos) = self.working.entries().iter().position(|e| {
                            e.scope == *scope
                                && e.kind == *kind
                                && e.identity.as_deref() == current.as_deref()
                        }) {
                            self.working.entries_mut()[pos].identity = Some(value);
                            self.working.sort();
                        }
                        self.set_ok("identity updated (pending)");
                    }
                }
                true
            }
            KeyCode::Backspace => {
                id.value.pop();
                id.error = None;
                false
            }
            KeyCode::Char(c) => {
                id.value.push(c);
                id.error = None;
                false
            }
            _ => false,
        }
    }

    /// Step the Confirm dialog. Returns `true` when it should close.
    fn step_confirm(&mut self, cd: &mut ConfirmDialog, key: KeyEvent) -> bool {
        if cd.choices.is_empty() {
            return true;
        }
        match key.code {
            KeyCode::Esc => true,
            KeyCode::Up => {
                cd.cursor = (cd.cursor + cd.choices.len() - 1) % cd.choices.len();
                false
            }
            KeyCode::Down => {
                cd.cursor = (cd.cursor + 1) % cd.choices.len();
                false
            }
            KeyCode::Enter => {
                let action = cd.choices[cd.cursor].action.clone();
                self.perform_confirm(action);
                true
            }
            KeyCode::Char(c) => {
                if let Some(choice) = cd.choices.iter().find(|ch| ch.key == c) {
                    let action = choice.action.clone();
                    self.perform_confirm(action);
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn perform_confirm(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::Quit => self.running = false,
            ConfirmAction::Save => self.save_or_report(),
            ConfirmAction::SaveAndQuit => {
                let _ = self.save();
                self.running = false;
            }
            ConfirmAction::Cancel => self.set_status("cancelled".to_string()),
            ConfirmAction::DeleteEntry => self.delete_selected(),
            ConfirmAction::Reload => match self.reload() {
                Ok(()) => self.set_ok("reloaded from filesystem"),
                Err(e) => self.set_error(format!("reload failed: {e}")),
            },
        }
    }

    fn identity_resolves(&self, kind: EntryKind, name: &str) -> bool {
        match kind {
            EntryKind::NamedUser => backend::lookup_user(name).is_ok(),
            EntryKind::NamedGroup => backend::lookup_group(name).is_ok(),
            _ => false,
        }
    }

    fn handle_editor_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.quit_or_confirm(),
            KeyCode::Char('?') => self.screen = Screen::Help,
            KeyCode::Char('j') | KeyCode::Down => self.select(1),
            KeyCode::Char('k') | KeyCode::Up => self.select(-1),
            KeyCode::Left => self.perm_cursor = self.perm_cursor.saturating_sub(1),
            KeyCode::Right => {
                if self.perm_cursor < 2 {
                    self.perm_cursor += 1;
                }
            }
            KeyCode::Char(' ') => self.toggle_perm(),
            KeyCode::Tab => self.focus = Focus::Editor,
            KeyCode::BackTab => self.focus = Focus::List,
            KeyCode::Char('f') => self.cycle_filter(),
            KeyCode::Char('e') => self.edit_identity(),
            KeyCode::Char('a') => self.open_add(),
            KeyCode::Char('d') => self.confirm_delete(),
            KeyCode::Char('o') => self.open_owner_input(),
            KeyCode::Char('g') => self.open_group_input(),
            KeyCode::Char('m') => self.jump_to_mask(),
            KeyCode::Char('t') => self.toggle_recursive(),
            KeyCode::Char('p') => self.open_preview(),
            KeyCode::Char('S') => self.try_save(),
            KeyCode::Char('R') => self.reload_or_confirm(),
            KeyCode::Esc => {}
            _ => {}
        }
    }

    fn handle_preview_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('p') => {
                self.screen = Screen::Editor;
            }
            KeyCode::Enter => {
                self.screen = Screen::Editor;
                self.try_save();
            }
            _ => {}
        }
    }

    fn handle_help_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => self.screen = Screen::Editor,
            _ => {}
        }
    }

    fn toggle_perm(&mut self) {
        if self.read_only {
            self.set_error("read-only mode: saving is disabled".to_string());
            return;
        }
        let bit = PermBit::ALL[self.perm_cursor];
        if let Some(e) = self.selected_entry_mut() {
            e.permissions.toggle(bit);
            self.working.normalize();
            self.sync_status_to_selection();
        }
    }

    fn cycle_filter(&mut self) {
        self.list_filter = match self.list_filter {
            ListFilter::All => ListFilter::Access,
            ListFilter::Access => {
                if self.object.is_directory {
                    ListFilter::Default
                } else {
                    ListFilter::All
                }
            }
            ListFilter::Default => ListFilter::All,
        };
        self.clamp_selection();
        let label = match self.list_filter {
            ListFilter::All => "All",
            ListFilter::Access => "Access",
            ListFilter::Default => "Default",
        };
        self.set_status(format!("showing {label}"));
    }

    fn edit_identity(&mut self) {
        if self.read_only {
            self.set_error("read-only mode".to_string());
            return;
        }
        let Some(e) = self.selected_entry() else {
            return;
        };
        if !e.kind.is_named() {
            self.set_error("identity only applies to named user/group entries".to_string());
            return;
        }
        let (scope, kind, identity) = (e.scope, e.kind, e.identity.clone());
        let title = format!("Edit {}", e.who());
        let value = identity.clone().unwrap_or_default();
        self.dialog = Some(Dialog::Input(InputDialog {
            title,
            target: InputTarget::Identity(scope, kind, identity),
            value,
            error: None,
        }));
    }

    fn open_add(&mut self) {
        if self.read_only {
            self.set_error("read-only mode".to_string());
            return;
        }
        let can_default = self.object.is_directory;
        let scope = if can_default {
            match self.list_filter {
                ListFilter::Default => Scope::Default,
                _ => Scope::Access,
            }
        } else {
            Scope::Access
        };
        self.dialog = Some(Dialog::Add(AddDialog {
            can_default,
            scope,
            is_user: true,
            name: String::new(),
            read: true,
            write: false,
            execute: false,
            cursor: 0,
            error: None,
        }));
    }

    fn confirm_delete(&mut self) {
        if self.read_only {
            self.set_error("read-only mode".to_string());
            return;
        }
        let Some(e) = self.selected_entry() else {
            return;
        };
        if !e.kind.is_named() {
            self.set_error("only named user/group entries can be deleted".to_string());
            return;
        }
        let desc = e.who();
        self.dialog = Some(Dialog::Confirm(ConfirmDialog {
            title: "Delete entry".to_string(),
            body: format!("Delete {desc}? This is pending until you save."),
            cursor: 0,
            choices: vec![
                ConfirmChoice {
                    key: 'y',
                    label: "Delete".to_string(),
                    action: ConfirmAction::DeleteEntry,
                },
                ConfirmChoice {
                    key: 'c',
                    label: "Cancel".to_string(),
                    action: ConfirmAction::Cancel,
                },
            ],
        }));
    }

    fn delete_selected(&mut self) {
        let Some(e) = self.selected_entry() else {
            return;
        };
        if e.kind.is_named() {
            let desc = e.who();
            let (scope, kind, identity) = (e.scope, e.kind, e.identity.clone());
            self.working
                .remove_slot(scope, kind, identity.as_deref());
            self.working.normalize();
            self.clamp_selection();
            self.sync_status_to_selection();
            self.set_ok(format!("deleted {desc} (pending)"));
        }
    }

    fn open_owner_input(&mut self) {
        if self.read_only {
            self.set_error("read-only mode".to_string());
            return;
        }
        self.dialog = Some(Dialog::Input(InputDialog {
            title: "Change owner".to_string(),
            target: InputTarget::Owner,
            value: self.owner.clone(),
            error: None,
        }));
    }

    fn open_group_input(&mut self) {
        if self.read_only {
            self.set_error("read-only mode".to_string());
            return;
        }
        self.dialog = Some(Dialog::Input(InputDialog {
            title: "Change owning group".to_string(),
            target: InputTarget::Group,
            value: self.group.clone(),
            error: None,
        }));
    }

    fn jump_to_mask(&mut self) {
        let Some(e) = self.selected_entry() else {
            return;
        };
        let scope = e.scope;
        let pos = self
            .working
            .entries()
            .iter()
            .position(|en| en.scope == scope && en.kind == EntryKind::Mask);
        let Some(pos) = pos else {
            self.set_status(
                "no mask entry in this scope (no named entries)".to_string(),
            );
            return;
        };
        let target = &self.working.entries()[pos];
        let visible = self.visible_entries();
        if let Some(vi) = visible.iter().position(|en| core::ptr::eq(*en, target)) {
            self.selected = vi;
            let perms = target.permissions;
            self.set_status(format!("mask::{perms} ({})", scope.label()));
        } else {
            self.set_status("mask entry is hidden by the current filter".to_string());
        }
    }

    fn toggle_recursive(&mut self) {
        if !self.object.is_directory {
            self.set_status("recursive only applies to directories".to_string());
            return;
        }
        self.recursive = !self.recursive;
        self.set_status(if self.recursive {
            "recursive: on (saving applies to all descendants)"
        } else {
            "recursive: off (this object only)"
        });
    }

    fn open_preview(&mut self) {
        self.screen = Screen::Preview;
    }

    fn try_save(&mut self) {
        if self.read_only {
            self.set_error("read-only mode: saving is disabled".to_string());
            return;
        }
        if !self.is_modified() {
            self.set_status("no pending changes".to_string());
            return;
        }
        // Recursive saves touch many objects, so ask first.
        if self.recursive {
            self.dialog = Some(Dialog::Confirm(ConfirmDialog {
                title: "Apply recursively".to_string(),
                body: format!(
                    "Apply the ACL to every descendant of {}?\nThis modifies many objects at once.",
                    self.object.path
                ),
                cursor: 0,
                choices: vec![
                    ConfirmChoice {
                        key: 'y',
                        label: "Apply".to_string(),
                        action: ConfirmAction::Save,
                    },
                    ConfirmChoice {
                        key: 'n',
                        label: "Cancel".to_string(),
                        action: ConfirmAction::Cancel,
                    },
                ],
            }));
            return;
        }
        self.save_or_report();
    }

    fn save_or_report(&mut self) {
        match self.save() {
            Ok(()) => {}
            Err(e) => self.set_error(format!("save failed: {e}")),
        }
    }

    fn reload_or_confirm(&mut self) {
        if self.is_modified() {
            self.dialog = Some(Dialog::Confirm(ConfirmDialog {
                title: "Discard changes".to_string(),
                body: "Reload from the filesystem and discard pending changes?".to_string(),
                cursor: 0,
                choices: vec![
                    ConfirmChoice {
                        key: 'y',
                        label: "Reload".to_string(),
                        action: ConfirmAction::Reload,
                    },
                    ConfirmChoice {
                        key: 'c',
                        label: "Cancel".to_string(),
                        action: ConfirmAction::Cancel,
                    },
                ],
            }));
        } else {
            match self.reload() {
                Ok(()) => self.set_ok("reloaded from filesystem"),
                Err(e) => self.set_error(format!("reload failed: {e}")),
            }
        }
    }

    fn quit_or_confirm(&mut self) {
        if self.is_modified() {
            self.dialog = Some(Dialog::Confirm(ConfirmDialog {
                title: "Unsaved ACL changes".to_string(),
                body: format!("You have unsaved changes to {}.", self.object.path),
                cursor: 0,
                choices: vec![
                    ConfirmChoice {
                        key: 's',
                        label: "Save and quit".to_string(),
                        action: ConfirmAction::SaveAndQuit,
                    },
                    ConfirmChoice {
                        key: 'd',
                        label: "Discard and quit".to_string(),
                        action: ConfirmAction::Quit,
                    },
                    ConfirmChoice {
                        key: 'c',
                        label: "Return to editor".to_string(),
                        action: ConfirmAction::Cancel,
                    },
                ],
            }));
        } else {
            self.running = false;
        }
    }

    fn add_entry(&mut self, entry: AclEntry) {
        let scope = entry.scope;
        let key = (entry.scope, entry.kind, entry.identity.clone());
        self.working.insert(entry);
        self.working.normalize();
        self.adjust_filter_for(scope);
        let pos = self
            .working
            .entries()
            .iter()
            .position(|e| (e.scope, e.kind, e.identity.clone()) == key);
        if let Some(pos) = pos {
            let target = &self.working.entries()[pos];
            let visible = self.visible_entries();
            if let Some(vi) = visible.iter().position(|e| core::ptr::eq(*e, target)) {
                self.selected = vi;
            }
        }
        self.clamp_selection();
        self.sync_status_to_selection();
    }

    /// Widen the filter if it would hide the scope an entry was just added to.
    fn adjust_filter_for(&mut self, scope: Scope) {
        let conflict = match self.list_filter {
            ListFilter::All => false,
            ListFilter::Access => scope == Scope::Default,
            ListFilter::Default => scope == Scope::Access,
        };
        if conflict {
            self.list_filter = ListFilter::All;
        }
    }

    /// Apply the mask policy to a copy of the working ACL, for saving.
    fn build_save_acl(&self) -> Acl {
        let mut acl = self.working.clone();
        acl.normalize();
        if self.mask_policy == MaskPolicy::Auto {
            for scope in [Scope::Access, Scope::Default] {
                if acl.has_named_entries(scope) {
                    acl.set_mask(scope, acl.recalculate_mask(scope));
                }
            }
        }
        acl
    }

    /// Persist pending changes: back up the original, chown, setfacl, reload.
    /// With recursive on, each step reaches every descendant of the object.
    fn save(&mut self) -> Result<()> {
        let acl = self.build_save_acl();
        let recursive = self.recursive;
        let backup = backend::backup(&self.object.path, recursive)?;
        if self.owner != self.original_owner || self.group != self.original_group {
            let owner = (self.owner != self.original_owner).then(|| self.owner.as_str());
            let group = (self.group != self.original_group).then(|| self.group.as_str());
            backend::chown(&self.object.path, owner, group, recursive)?;
        }
        backend::write(&self.object.path, &acl, recursive)?;
        self.reload()?;
        let path = backup.display().to_string();
        self.backup_path = Some(backup);
        self.set_ok(format!("saved (backup at {path})"));
        Ok(())
    }

    /// Re-read the object and ACL from disk, dropping any pending changes.
    fn reload(&mut self) -> Result<()> {
        let handle = backend::read(&self.object.path)?;
        self.object = handle.object;
        self.original = handle.acl.clone();
        self.original_owner = self.object.owner.clone();
        self.original_group = self.object.group.clone();
        self.working = handle.acl;
        self.owner = self.original_owner.clone();
        self.group = self.original_group.clone();
        self.clamp_selection();
        self.sync_status_to_selection();
        Ok(())
    }

    /// The text shown on the Preview screen: a per-line diff of ownership and
    /// each ACL scope, plus mask warnings and the command equivalent.
    pub fn build_preview(&self) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(format!("Preview changes: {}", self.object.path));
        lines.push(String::new());
        if self.read_only {
            lines.push("Read-only mode: saving is disabled.".to_string());
            lines.push(String::new());
        }

        lines.push("Ownership:".to_string());
        let owner_same = self.owner == self.original_owner;
        let group_same = self.group == self.original_group;
        if !owner_same {
            lines.push(format!("  owner:  {} -> {}", self.original_owner, self.owner));
        }
        if !group_same {
            lines.push(format!("  group:  {} -> {}", self.original_group, self.group));
        }
        if owner_same && group_same {
            lines.push("  unchanged".to_string());
        }
        lines.push(String::new());

        lines.push("Access ACL:".to_string());
        for line in self.scope_diff(Scope::Access) {
            lines.push(format!("  {line}"));
        }
        lines.push(String::new());

        if self.object.is_directory {
            lines.push("Default ACL:".to_string());
            for line in self.scope_diff(Scope::Default) {
                lines.push(format!("  {line}"));
            }
            lines.push(String::new());
        }

        let warnings = self.mask_warnings();
        if !warnings.is_empty() {
            lines.push("Mask warnings:".to_string());
            for warning in &warnings {
                lines.push(format!("  ! {warning}"));
            }
            lines.push(String::new());
        }

        let policy = match self.mask_policy {
            MaskPolicy::Auto => "auto (recalculate)",
            MaskPolicy::Preserve => "preserve",
            MaskPolicy::Manual => "manual",
        };
        lines.push(format!("Mask policy: {policy}"));
        lines.push(format!(
            "Recursive: {}",
            if self.recursive {
                "yes (applies to all descendants)"
            } else {
                "no (this object only)"
            }
        ));
        lines.push(String::new());

        let r = if self.recursive { " -R" } else { "" };
        lines.push("Command-equivalent:".to_string());
        lines.push(format!(
            "  setfacl --set-file=<generated-acl>{r} -- {}",
            self.object.path
        ));
        if !owner_same || !group_same {
            lines.push(format!(
                "  chown{r} {} -- {}",
                self.chown_spec(),
                self.object.path
            ));
        }
        lines
    }

    /// A `+`/`-` line diff between the original and working ACLs in one scope.
    fn scope_diff(&self, scope: Scope) -> Vec<String> {
        let mut slots: Vec<(EntryKind, Option<String>)> = Vec::new();
        for e in self.original.in_scope(scope) {
            let slot = (e.kind, e.identity.clone());
            if !slots.contains(&slot) {
                slots.push(slot);
            }
        }
        for e in self.working.in_scope(scope) {
            let slot = (e.kind, e.identity.clone());
            if !slots.contains(&slot) {
                slots.push(slot);
            }
        }
        let mut out = Vec::new();
        for (kind, identity) in &slots {
            let original = self.original.find(scope, *kind, identity.as_deref());
            let working = self.working.find(scope, *kind, identity.as_deref());
            match (original, working) {
                (Some(o), Some(w)) if o.permissions == w.permissions => {}
                (Some(o), Some(w)) => {
                    out.push(format!("- {}", o.to_text()));
                    out.push(format!("+ {}", w.to_text()));
                }
                (Some(o), None) => out.push(format!("- {}", o.to_text())),
                (None, Some(w)) => out.push(format!("+ {}", w.to_text())),
                (None, None) => {}
            }
        }
        if out.is_empty() {
            out.push("unchanged".to_string());
        }
        out
    }

    /// The `chown` spec for pending ownership changes.
    fn chown_spec(&self) -> String {
        let owner = (self.owner != self.original_owner).then(|| self.owner.as_str());
        let group = (self.group != self.original_group).then(|| self.group.as_str());
        match (owner, group) {
            (Some(o), Some(g)) => format!("{o}:{g}"),
            (Some(o), None) => o.to_string(),
            (None, Some(g)) => format!(":{g}"),
            (None, None) => String::new(),
        }
    }

    /// The static help text shown on the Help screen.
    pub fn help_lines() -> Vec<&'static str> {
        vec![
            "POSIX ACL Editor - key bindings",
            "",
            "Navigation",
            "  Up/Down or j/k     Select ACL entry",
            "  Left/Right         Move permission cursor (Read/Write/Execute)",
            "  Space              Toggle the permission under the cursor",
            "  Tab / Shift+Tab    Move focus: list <-> editor",
            "  f                  Cycle list filter: All / Access / Default",
            "",
            "Entry CRUD",
            "  a                  Add a named user/group entry",
            "  d                  Delete selected named entry",
            "  e                  Edit selected user/group identity",
            "  o                  Change file owner",
            "  g                  Change owning group",
            "  m                  Jump to the mask entry of the selected scope",
            "",
            "Save / rollback",
            "  t                  Toggle recursive (apply to all descendants)",
            "  p                  Preview pending changes",
            "  S                  Save (backup + chown + setfacl)",
            "  R                  Reload from filesystem",
            "  ?                  This help",
            "  q                  Quit (confirms if modified)",
            "",
            "The mask caps named users, the owning group and named groups.",
            "The detail pane shows requested vs. effective permission.",
        ]
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use crossterm::event::KeyModifiers;

    use super::*;
    use crate::event::AppEvent;

    static CTR: AtomicU32 = AtomicU32::new(0);

    /// Build an [`App`] backed by a fresh temp directory carrying an extended
    /// ACL (a named user plus a mask), so entry selection and mask logic have
    /// something real to operate on.
    fn test_app() -> (std::path::PathBuf, App) {
        let id = CTR.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir()
            .join(format!("facl_rs_app_test_{}_{}", std::process::id(), id));
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
        backend::write(dir.to_str().unwrap(), &acl, false).unwrap();
        let cli = crate::cli::Cli {
            path: dir.to_str().unwrap().to_string(),
            read_only: false,
            mask: crate::cli::MaskPolicy::Auto,
        };
        let app = App::new(cli).unwrap();
        (dir, app)
    }

    fn key(code: KeyCode) -> AppEvent {
        AppEvent::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn new_loads_entries_unmodified() {
        let (dir, app) = test_app();
        assert!(app.working.in_scope(Scope::Access).len() >= 5);
        assert!(app.working.mask(Scope::Access).is_some());
        assert!(!app.is_modified());
        assert_eq!(app.screen, Screen::Editor);
        assert_eq!(app.selected, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn down_selects_next_and_space_marks_modified() {
        let (dir, mut app) = test_app();
        app.handle_event(key(KeyCode::Down));
        assert_eq!(app.selected, 1);
        app.handle_event(key(KeyCode::Char(' ')));
        assert!(app.is_modified());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn f_cycles_the_list_filter() {
        let (dir, mut app) = test_app();
        assert_eq!(app.list_filter, ListFilter::All);
        app.handle_event(key(KeyCode::Char('f')));
        assert_eq!(app.list_filter, ListFilter::Access);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_opens_the_add_dialog() {
        let (dir, mut app) = test_app();
        app.handle_event(key(KeyCode::Char('a')));
        assert!(matches!(app.dialog, Some(Dialog::Add(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn p_toggles_preview_then_back() {
        let (dir, mut app) = test_app();
        app.handle_event(key(KeyCode::Char('p')));
        assert_eq!(app.screen, Screen::Preview);
        app.handle_event(key(KeyCode::Char('q')));
        assert_eq!(app.screen, Screen::Editor);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preview_reports_diff_after_an_edit() {
        let (dir, mut app) = test_app();
        app.handle_event(key(KeyCode::Char(' ')));
        let preview = app.build_preview().join("\n");
        assert!(preview.contains('+') && preview.contains('-'), "preview: {preview}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_then_reopen_persists_change() {
        let (dir, mut app) = test_app();
        app.handle_event(key(KeyCode::Char(' ')));
        assert!(app.is_modified());
        app.try_save();
        assert!(!app.is_modified(), "should be clean after save");
        // A fresh read sees the change and the backup file was written.
        let reloaded = crate::backend::read(dir.to_str().unwrap()).unwrap();
        assert_eq!(reloaded.acl, app.working);
        assert!(app.backup_path.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn add_state(app: &App) -> (usize, bool, String, bool, bool, bool) {
        match &app.dialog {
            Some(Dialog::Add(ad)) => (
                ad.cursor,
                ad.is_user,
                ad.name.clone(),
                ad.read,
                ad.write,
                ad.execute,
            ),
            other => panic!("expected the Add dialog, got {other:?}"),
        }
    }

    #[test]
    fn add_dialog_arrows_navigate_and_space_toggles() {
        let (dir, mut app) = test_app();
        app.handle_event(key(KeyCode::Char('a')));
        // Starts on Scope (cursor 0), default to a named user with read on.
        assert_eq!(
            add_state(&app),
            (0, true, String::new(), true, false, false)
        );

        app.handle_event(key(KeyCode::Right)); // -> Type
        app.handle_event(key(KeyCode::Char(' '))); // is_user -> false
        app.handle_event(key(KeyCode::Down)); // -> Name
        app.handle_event(key(KeyCode::Char('b')));
        app.handle_event(key(KeyCode::Char('a')));
        app.handle_event(key(KeyCode::Char('c')));
        app.handle_event(key(KeyCode::Down)); // -> Read
        app.handle_event(key(KeyCode::Char(' '))); // read -> false
        app.handle_event(key(KeyCode::Right)); // -> Write
        app.handle_event(key(KeyCode::Char(' '))); // write -> true

        assert_eq!(
            add_state(&app),
            (4, false, "bac".to_string(), false, true, false)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_dialog_navigation_wraps_around() {
        let (dir, mut app) = test_app();
        app.handle_event(key(KeyCode::Char('a')));
        // Walk to the last field (Execute, cursor 5).
        for _ in 0..5 {
            app.handle_event(key(KeyCode::Down));
        }
        assert_eq!(add_state(&app).0, 5);
        // Tab at the end wraps back to the first field (Scope, cursor 0).
        app.handle_event(key(KeyCode::Tab));
        assert_eq!(add_state(&app).0, 0);
        // BackTab at the start wraps to the last field (cursor 5).
        app.handle_event(key(KeyCode::BackTab));
        assert_eq!(add_state(&app).0, 5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn toggle_recursive_flips_the_flag() {
        let (dir, mut app) = test_app();
        assert!(!app.recursive);
        app.handle_event(key(KeyCode::Char('t')));
        assert!(app.recursive);
        app.handle_event(key(KeyCode::Char('t')));
        assert!(!app.recursive);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn toggle_recursive_is_a_noop_for_a_file() {
        let id = CTR.fetch_add(1, Ordering::SeqCst);
        let file = std::env::temp_dir()
            .join(format!("facl_rs_file_{}_{}", std::process::id(), id));
        let _ = std::fs::remove_file(&file);
        std::fs::File::create(&file).unwrap();
        let cli = crate::cli::Cli {
            path: file.to_str().unwrap().to_string(),
            read_only: false,
            mask: crate::cli::MaskPolicy::Auto,
        };
        let mut app = App::new(cli).unwrap();
        assert!(!app.object.is_directory);
        app.handle_event(key(KeyCode::Char('t')));
        assert!(
            !app.recursive,
            "toggling recursive on a single file must be a no-op"
        );
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn preview_shows_recursive_mode() {
        let (dir, mut app) = test_app();
        app.recursive = true;
        let preview = app.build_preview().join("\n");
        assert!(preview.contains("Recursive: yes"));
        assert!(preview.contains("setfacl --set-file=<generated-acl> -R"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
