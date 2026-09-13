/*
 * Copyright 2026 Chekhov Ma <maqike@qq.com>
 * SPDX-License-Identifier: Apache-2.0
 */
use super::permissions::Permissions;

/// Which ACL set an entry belongs to.
///
/// Access ACLs govern the object itself. Default ACLs only exist on
/// directories and are inherited by newly created children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Access,
    Default,
}

impl Scope {
    /// The single-character shown in the list's scope column.
    pub fn short(self) -> char {
        match self {
            Scope::Access => 'A',
            Scope::Default => 'D',
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Scope::Access => "Access",
            Scope::Default => "Default",
        }
    }

    /// The `getfacl`/`setfacl` text prefix for this scope.
    pub fn prefix(self) -> &'static str {
        match self {
            Scope::Access => "",
            Scope::Default => "default:",
        }
    }
}

/// The logical kind of an ACL entry (TrueNAS calls this **Who**).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// The file owner (`user::`).
    Owner,
    /// A named user (`user:name:`).
    NamedUser,
    /// The owning group (`group::`).
    OwningGroup,
    /// A named group (`group:name:`).
    NamedGroup,
    /// The ACL mask (`mask::`).
    Mask,
    /// Permissions for "other" (`other::`).
    Other,
}

impl EntryKind {
    /// The `getfacl`/`setfacl` keyword for this kind.
    pub fn keyword(self) -> &'static str {
        match self {
            EntryKind::Owner | EntryKind::NamedUser => "user",
            EntryKind::OwningGroup | EntryKind::NamedGroup => "group",
            EntryKind::Mask => "mask",
            EntryKind::Other => "other",
        }
    }

    /// The compact label shown in the list's Type column.
    pub fn short(self) -> &'static str {
        match self {
            EntryKind::Owner => "owning-user",
            EntryKind::NamedUser => "user",
            EntryKind::OwningGroup => "owning-group",
            EntryKind::NamedGroup => "group",
            EntryKind::Mask => "mask",
            EntryKind::Other => "other",
        }
    }

    /// The long label shown in the entry editor.
    pub fn label(self) -> &'static str {
        match self {
            EntryKind::Owner => "Owning user",
            EntryKind::NamedUser => "Named user",
            EntryKind::OwningGroup => "Owning group",
            EntryKind::NamedGroup => "Named group",
            EntryKind::Mask => "Mask",
            EntryKind::Other => "Other",
        }
    }

    /// Base entries always exist in an access ACL and cannot be added/removed.
    pub fn is_base(self) -> bool {
        matches!(
            self,
            EntryKind::Owner | EntryKind::OwningGroup | EntryKind::Other
        )
    }

    /// Named entries carry an identity and can be added/removed.
    pub fn is_named(self) -> bool {
        matches!(self, EntryKind::NamedUser | EntryKind::NamedGroup)
    }

    /// Entries whose effective permission is capped by the mask.
    pub fn is_maskable(self) -> bool {
        matches!(
            self,
            EntryKind::NamedUser
                | EntryKind::OwningGroup
                | EntryKind::NamedGroup
                | EntryKind::Mask
        )
    }
}

/// One POSIX ACL entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AclEntry {
    pub scope: Scope,
    pub kind: EntryKind,
    /// The user or group identity for named entries; `None` otherwise.
    pub identity: Option<String>,
    pub permissions: Permissions,
}

impl AclEntry {
    pub fn new(
        scope: Scope,
        kind: EntryKind,
        identity: Option<String>,
        permissions: Permissions,
    ) -> Self {
        Self {
            scope,
            kind,
            identity,
            permissions,
        }
    }

    /// A human-readable description of the entry, e.g. `user:alice`.
    pub fn who(&self) -> String {
        match &self.identity {
            Some(name) => format!("{}:{}", self.kind.short(), name),
            None => self.kind.short().to_string(),
        }
    }

    /// The `setfacl`/`getfacl` textual form of this entry.
    pub fn to_text(&self) -> String {
        let identity = match &self.identity {
            Some(name) => name.clone(),
            None => String::new(),
        };
        format!(
            "{}{}:{}:{}",
            self.scope.prefix(),
            self.kind.keyword(),
            identity,
            self.permissions,
        )
    }

    /// Whether two entries address the same slot (scope + kind + identity).
    pub fn same_slot(&self, other: &AclEntry) -> bool {
        self.scope == other.scope
            && self.kind == other.kind
            && self.identity == other.identity
    }

    /// A sortable key giving the canonical POSIX order: scope, then kind
    /// (owner, named users, owning group, named groups, mask, other), then the
    /// identity name.
    pub fn order_key(&self) -> (u8, u8, String) {
        let scope = match self.scope {
            Scope::Access => 0,
            Scope::Default => 1,
        };
        let kind = match self.kind {
            EntryKind::Owner => 0,
            EntryKind::NamedUser => 1,
            EntryKind::OwningGroup => 2,
            EntryKind::NamedGroup => 3,
            EntryKind::Mask => 4,
            EntryKind::Other => 5,
        };
        (scope, kind, self.identity.clone().unwrap_or_default())
    }
}
