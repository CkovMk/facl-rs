/*
 * Copyright 2026 Chekhov Ma <maqike@qq.com>
 * SPDX-License-Identifier: Apache-2.0
 */
use super::entry::{AclEntry, EntryKind, Scope};
use super::permissions::Permissions;

/// Static metadata about the filesystem object being edited, gathered from
/// `stat`. Kept separate from the ACL entries so ownership can be tracked and
/// diffed independently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectInfo {
    /// Resolved absolute path, displayed in the header.
    pub path: String,
    pub file_type: String,
    pub is_directory: bool,
    /// Owner name (or uid if unresolvable).
    pub owner: String,
    /// Owning group name (or gid if unresolvable).
    pub group: String,
    /// Octal mode string, e.g. `"0750"`.
    pub mode: String,
}

/// The complete POSIX ACL of one filesystem object: its access entries and,
/// for directories, its default entries, stored in a single canonically-ordered
/// list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Acl {
    entries: Vec<AclEntry>,
}

impl Acl {
    pub fn new(entries: Vec<AclEntry>) -> Self {
        let mut acl = Self { entries };
        acl.sort();
        acl
    }

    pub fn entries(&self) -> &[AclEntry] {
        &self.entries
    }

    pub fn entries_mut(&mut self) -> &mut Vec<AclEntry> {
        &mut self.entries
    }

    /// All entries in the given scope.
    pub fn in_scope(&self, scope: Scope) -> Vec<&AclEntry> {
        self.entries
            .iter()
            .filter(|e| e.scope == scope)
            .collect()
    }

    /// Whether the ACL has any default entries.
    pub fn has_default(&self) -> bool {
        self.entries.iter().any(|e| e.scope == Scope::Default)
    }

    /// Sort the entries into a stable canonical order (scope, kind, identity).
    pub fn sort(&mut self) {
        self.entries.sort_by_key(|e| e.order_key());
    }

    /// Find an entry by scope, kind and (for named entries) identity.
    pub fn find(
        &self,
        scope: Scope,
        kind: EntryKind,
        identity: Option<&str>,
    ) -> Option<&AclEntry> {
        self.entries
            .iter()
            .find(|e| e.scope == scope && e.kind == kind && e.identity.as_deref() == identity)
    }

    /// Insert an entry, keeping the list sorted.
    pub fn insert(&mut self, entry: AclEntry) {
        self.entries.push(entry);
        self.sort();
    }

    /// Remove the entry occupying the same slot (scope + kind + identity).
    ///
    /// Returns `true` if an entry was removed.
    pub fn remove_slot(&mut self, scope: Scope, kind: EntryKind, identity: Option<&str>) -> bool {
        let before = self.entries.len();
        self.entries
            .retain(|e| !(e.scope == scope && e.kind == kind && e.identity.as_deref() == identity));
        self.entries.len() != before
    }

    /// The mask entry for a scope, if present.
    pub fn mask(&self, scope: Scope) -> Option<Permissions> {
        self.entries
            .iter()
            .find(|e| e.scope == scope && e.kind == EntryKind::Mask)
            .map(|e| e.permissions)
    }

    /// Union of the group-class entries for a scope; this is the mask value
    /// `setfacl` computes when none is given explicitly.
    pub fn recalculate_mask(&self, scope: Scope) -> Permissions {
        let mut mask = Permissions::NONE;
        for e in self.entries.iter().filter(|e| e.scope == scope) {
            if matches!(
                e.kind,
                EntryKind::NamedUser | EntryKind::OwningGroup | EntryKind::NamedGroup
            ) {
                mask = mask.union(e.permissions);
            }
        }
        mask
    }

    /// The permission an entry grants after the mask is applied. Base entries
    /// (owner, other) and the mask itself are never capped.
    pub fn effective(&self, entry: &AclEntry) -> Permissions {
        let capped = matches!(
            entry.kind,
            EntryKind::NamedUser | EntryKind::OwningGroup | EntryKind::NamedGroup
        );
        if capped {
            if let Some(mask) = self.mask(entry.scope) {
                return entry.permissions.intersect(mask);
            }
        }
        entry.permissions
    }

    /// Whether a scope has any extended (named) entries requiring a mask line.
    pub fn has_named_entries(&self, scope: Scope) -> bool {
        self.entries
            .iter()
            .any(|e| {
                e.scope == scope
                    && matches!(e.kind, EntryKind::NamedUser | EntryKind::NamedGroup)
            })
    }

    /// Insert or update the mask entry for a scope.
    pub fn set_mask(&mut self, scope: Scope, mask: Permissions) {
        if let Some(m) = self
            .entries
            .iter_mut()
            .find(|e| e.scope == scope && e.kind == EntryKind::Mask)
        {
            m.permissions = mask;
        } else {
            self.insert(AclEntry::new(scope, EntryKind::Mask, None, mask));
        }
    }

    /// Keep the mask entry in sync with the named entries of each scope: add a
    /// mask when a scope has named entries but no mask, and remove the mask when
    /// a scope no longer has any (so it falls back to plain owner/group/other).
    /// Called after edits so effective-permission display stays correct.
    pub fn normalize(&mut self) {
        for scope in [Scope::Access, Scope::Default] {
            let has_mask = self
                .entries
                .iter()
                .any(|e| e.scope == scope && e.kind == EntryKind::Mask);
            match (self.has_named_entries(scope), has_mask) {
                (true, false) => self.set_mask(scope, self.recalculate_mask(scope)),
                (false, true) => {
                    self.remove_slot(scope, EntryKind::Mask, None);
                }
                _ => {}
            }
        }
    }

    /// Serialize the ACL to the text format accepted by `setfacl --set-file`.
    pub fn to_setfacl_text(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        for scope in [Scope::Access, Scope::Default] {
            for entry in self.in_scope(scope) {
                lines.push(entry.to_text());
            }
            if self.has_named_entries(scope)
                && !self
                    .entries
                    .iter()
                    .any(|e| e.scope == scope && e.kind == EntryKind::Mask)
            {
                lines.push(format!("{}mask::{}", scope.prefix(), self.recalculate_mask(scope)));
            }
        }
        lines.push(String::new());
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Permissions {
        Permissions::from_perm_string(s).unwrap()
    }

    fn sample() -> Acl {
        Acl::new(vec![
            AclEntry::new(Scope::Access, EntryKind::Owner, None, p("rwx")),
            AclEntry::new(
                Scope::Access,
                EntryKind::NamedUser,
                Some("alice".into()),
                p("rwx"),
            ),
            AclEntry::new(Scope::Access, EntryKind::OwningGroup, None, p("r-x")),
            AclEntry::new(
                Scope::Access,
                EntryKind::NamedGroup,
                Some("backup".into()),
                p("r-x"),
            ),
            AclEntry::new(Scope::Access, EntryKind::Mask, None, p("r-x")),
            AclEntry::new(Scope::Access, EntryKind::Other, None, p("---")),
            AclEntry::new(Scope::Default, EntryKind::Owner, None, p("rwx")),
            AclEntry::new(Scope::Default, EntryKind::OwningGroup, None, p("r-x")),
            AclEntry::new(Scope::Default, EntryKind::Mask, None, p("r-x")),
            AclEntry::new(Scope::Default, EntryKind::Other, None, p("---")),
        ])
    }

    #[test]
    fn effective_caps_named_entries() {
        let acl = sample();
        let alice = acl
            .find(Scope::Access, EntryKind::NamedUser, Some("alice"))
            .unwrap();
        assert_eq!(acl.effective(alice), p("r-x"));
        let owner = acl.find(Scope::Access, EntryKind::Owner, None).unwrap();
        assert_eq!(acl.effective(owner), p("rwx"));
    }

    #[test]
    fn recalc_mask_unions_group_class() {
        let acl = sample();
        // alice(rwx) | group-own(r-x) | backup(r-x) = rwx
        assert_eq!(acl.recalculate_mask(Scope::Access), p("rwx"));
    }

    #[test]
    fn serialize_round_trip() {
        let acl = sample();
        let text = acl.to_setfacl_text();
        let expected = "\
user::rwx
user:alice:rwx
group::r-x
group:backup:r-x
mask::r-x
other::---
default:user::rwx
default:group::r-x
default:mask::r-x
default:other::---
";
        assert_eq!(text, expected);
    }

    #[test]
    fn synthesize_missing_mask() {
        let mut acl = sample();
        acl.remove_slot(Scope::Access, EntryKind::Mask, None);
        let text = acl.to_setfacl_text();
        assert!(
            text.contains("mask::rwx"),
            "expected synthesized mask, got:\n{text}"
        );
    }

    #[test]
    fn sorting_is_canonical() {
        let acl = Acl::new(vec![
            AclEntry::new(Scope::Access, EntryKind::Other, None, p("---")),
            AclEntry::new(Scope::Access, EntryKind::Owner, None, p("rwx")),
            AclEntry::new(
                Scope::Access,
                EntryKind::NamedUser,
                Some("bob".into()),
                p("r-x"),
            ),
        ]);
        assert_eq!(
            acl.entries().iter().map(|e| e.kind).collect::<Vec<_>>(),
            vec![EntryKind::Owner, EntryKind::NamedUser, EntryKind::Other]
        );
    }

    #[test]
    fn remove_slot_is_scoped() {
        let mut acl = sample();
        assert!(acl.remove_slot(Scope::Access, EntryKind::NamedUser, Some("alice")));
        assert!(!acl.remove_slot(Scope::Access, EntryKind::NamedUser, Some("alice")));
        // default owner untouched by an access-scope removal
        assert!(acl.find(Scope::Default, EntryKind::Owner, None).is_some());
    }

    #[test]
    fn normalize_removes_mask_once_named_entries_gone() {
        let mut acl = sample();
        // Access scope has named entries, so a mask is present.
        assert!(acl.mask(Scope::Access).is_some());
        // Deleting every named entry in the scope drops the mask too.
        acl.remove_slot(Scope::Access, EntryKind::NamedUser, Some("alice"));
        acl.remove_slot(Scope::Access, EntryKind::NamedGroup, Some("backup"));
        acl.normalize();
        assert!(
            acl.mask(Scope::Access).is_none(),
            "mask should be removed with no named entries"
        );
        // And the serialized text has no mask line for that scope.
        assert!(!acl.to_setfacl_text().contains("mask::"));
    }

    #[test]
    fn normalize_readds_mask_after_inserting_named_entry() {
        let mut acl = sample();
        acl.remove_slot(Scope::Access, EntryKind::NamedUser, Some("alice"));
        acl.remove_slot(Scope::Access, EntryKind::NamedGroup, Some("backup"));
        acl.normalize();
        assert!(acl.mask(Scope::Access).is_none());
        acl.insert(AclEntry::new(
            Scope::Access,
            EntryKind::NamedUser,
            Some("carol".into()),
            p("r-x"),
        ));
        acl.normalize();
        assert_eq!(acl.mask(Scope::Access), Some(p("r-x")));
    }
}
