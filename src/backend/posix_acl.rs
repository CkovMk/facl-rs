/*
 * Copyright 2026 Chekhov Ma <maqike@qq.com>
 * SPDX-License-Identifier: Apache-2.0
 */
use std::path::PathBuf;

use anyhow::{bail, Context, Result};

use super::{run_stdout, run_stdin_stdout};
use crate::backend::ownership;
use crate::model::{Acl, AclEntry, EntryKind, ObjectInfo, Permissions, Scope};

/// The result of reading one object: its metadata plus its current ACL.
pub struct AclHandle {
    pub object: ObjectInfo,
    pub acl: Acl,
}

/// Read the object's `stat` metadata and ACL from the filesystem.
pub fn read(path: &str) -> Result<AclHandle> {
    let object = ownership::stat(path)?;
    let text = run_stdout("getfacl", &["-p", "--", &object.path])
        .with_context(|| format!("getfacl failed for {}", object.path))?;
    let acl = parse_getfacl(&text)?;
    Ok(AclHandle { object, acl })
}

/// Apply the full ACL of `acl` to `path`, replacing whatever is currently
/// set. With `recursive`, the ACL is applied to every descendant of `path`
/// (`setfacl -R`); files receive the access entries and directories also
/// receive the default entries.
pub fn write(path: &str, acl: &Acl, recursive: bool) -> Result<()> {
    let text = acl.to_setfacl_text();
    let mut args = vec!["--set-file=-"];
    if recursive {
        args.push("-R");
    }
    args.push("--");
    args.push(path);
    run_stdin_stdout("setfacl", &args, &text)
        .with_context(|| format!("setfacl{} {path} failed", if recursive { " -R" } else { "" }))?;
    Ok(())
}

/// Capture the current `getfacl` output to a backup file so it can be
/// restored. With `recursive`, the whole subtree is captured (`getfacl -R`).
///
/// Returns the path of the written backup.
pub fn backup(path: &str, recursive: bool) -> Result<PathBuf> {
    let mut args = vec!["-p"];
    if recursive {
        args.insert(0, "-R");
    }
    args.push("--");
    args.push(path);
    let text = run_stdout("getfacl", &args)?;
    let dir = backup_dir()?;
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating backup directory {}", dir.display()))?;
    let name = if recursive {
        format!("{}_R.acl.bak", sanitize_path(path))
    } else {
        format!("{}.acl.bak", sanitize_path(path))
    };
    let file = dir.join(name);
    std::fs::write(&file, text).with_context(|| format!("writing backup {}", file.display()))?;
    Ok(file)
}

/// Restore an ACL from a backup file produced by [`backup`].
#[allow(dead_code)]
pub fn restore(path: &str, backup_file: &std::path::Path) -> Result<()> {
    run_stdout(
        "setfacl",
        &["--set-file", backup_file.to_str().unwrap_or_default(), "--", path],
    )
    .with_context(|| format!("restoring ACL of {path} from {}", backup_file.display()))?;
    Ok(())
}

/// Parse `getfacl` text output into an [`Acl`].
///
/// Comment lines (the `# file:` / `# owner:` header) are skipped; every other
/// line must be a valid ACL entry.
pub fn parse_getfacl(text: &str) -> Result<Acl> {
    let mut entries = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        entries.push(parse_entry(line)?);
    }
    Ok(Acl::new(entries))
}

fn parse_entry(line: &str) -> Result<AclEntry> {
    let (scope, rest) = if let Some(r) = line.strip_prefix("default:") {
        (Scope::Default, r)
    } else {
        (Scope::Access, line)
    };

    let parts: Vec<&str> = rest.splitn(3, ':').collect();
    if parts.len() != 3 {
        bail!("malformed ACL entry: {line:?}");
    }
    let (keyword, identity, perms) = (parts[0], parts[1], parts[2]);

    let kind = if keyword == "user" {
        if identity.is_empty() {
            EntryKind::Owner
        } else {
            EntryKind::NamedUser
        }
    } else if keyword == "group" {
        if identity.is_empty() {
            EntryKind::OwningGroup
        } else {
            EntryKind::NamedGroup
        }
    } else if keyword == "mask" {
        EntryKind::Mask
    } else if keyword == "other" {
        EntryKind::Other
    } else {
        bail!("unknown ACL keyword {keyword:?} in {line:?}")
    };

    let identity = if identity.is_empty() {
        None
    } else {
        Some(identity.to_string())
    };
    let permissions = Permissions::from_perm_string(perms)
        .with_context(|| format!("invalid permissions {perms:?} in {line:?}"))?;

    Ok(AclEntry::new(scope, kind, identity, permissions))
}

fn backup_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .context("cannot locate a home directory for backups")?;
    let cache = match std::env::var("XDG_CACHE_HOME") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => home.join(".cache"),
    };
    Ok(cache.join("facl-rs"))
}

fn sanitize_path(path: &str) -> String {
    path.replace('/', "_").replace('\\', "_")
}

#[cfg(test)]
mod tests {
    use super::parse_getfacl;
    use crate::model::{Acl, AclEntry, EntryKind, Permissions, Scope};

    fn p(s: &str) -> Permissions {
        Permissions::from_perm_string(s).unwrap()
    }

    #[test]
    fn parses_extended_dir_acl() {
        let text = "\
# file: srv/share
# owner: root
# group: storage
# flags: -b-
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
        let acl = parse_getfacl(text).unwrap();
        assert_eq!(acl.in_scope(Scope::Access).len(), 6);
        assert_eq!(acl.in_scope(Scope::Default).len(), 4);

        let alice = acl
            .find(Scope::Access, EntryKind::NamedUser, Some("alice"))
            .unwrap();
        assert_eq!(alice.permissions, p("rwx"));

        assert_eq!(acl.mask(Scope::Access), Some(p("r-x")));
        assert_eq!(acl.mask(Scope::Default), Some(p("r-x")));
    }

    #[test]
    fn parses_plain_file_acl_without_mask() {
        let text = "\
# file: notes.txt
# owner: bob
# group: staff
user::rw-
group::r--
other::r--
";
        let acl = parse_getfacl(text).unwrap();
        assert_eq!(acl.in_scope(Scope::Access).len(), 3);
        assert!(!acl.has_default());
        assert!(acl.mask(Scope::Access).is_none());
    }

    #[test]
    fn round_trips_through_setfacl_text() {
        let text = "\
user::rwx
user:alice:rwx
group::r-x
mask::rwx
other::---
";
        let acl = parse_getfacl(text).unwrap();
        assert_eq!(acl.to_setfacl_text(), text);
    }

    #[test]
    fn rejects_unknown_keyword() {
        assert!(parse_getfacl("nobody::rwx\n").is_err());
    }

    /// End-to-end: write an ACL with `setfacl`, read it back with `getfacl`.
    #[test]
    fn write_then_read_round_trip() {
        let path = std::env::temp_dir().join(format!("facl_rs_test_{}", std::process::id()));
        std::fs::File::create(&path).unwrap();

        let acl = parse_getfacl(
            "user::rw-\nuser:nobody:r-x\ngroup::r--\ngroup:staff:---\nmask::r-x\nother::r--\n",
        )
        .unwrap();
        super::write(path.to_str().unwrap(), &acl, false).unwrap();

        let read = super::read(path.to_str().unwrap()).unwrap();
        let alice = read
            .acl
            .find(Scope::Access, EntryKind::NamedUser, Some("nobody"))
            .unwrap();
        assert_eq!(alice.permissions, p("r-x"));
        assert_eq!(read.acl.mask(Scope::Access), Some(p("r-x")));
        assert_eq!(read.acl.to_setfacl_text(), acl.to_setfacl_text());

        let _ = std::fs::remove_file(&path);
    }

    /// End-to-end: a recursive write applies the access entries to files and
    /// the access + default entries to child directories.
    #[test]
    fn write_recursive_applies_to_descendants() {
        let root = std::env::temp_dir()
            .join(format!("facl_rs_rec_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::File::create(root.join("file")).unwrap();

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
            AclEntry::new(Scope::Default, EntryKind::Owner, None, p("rwx")),
            AclEntry::new(
                Scope::Default,
                EntryKind::NamedUser,
                Some("nobody".into()),
                p("r-x"),
            ),
            AclEntry::new(Scope::Default, EntryKind::OwningGroup, None, p("r-x")),
            AclEntry::new(Scope::Default, EntryKind::Mask, None, p("r-x")),
            AclEntry::new(Scope::Default, EntryKind::Other, None, p("---")),
        ]);
        super::write(root.to_str().unwrap(), &acl, true).unwrap();

        // A child directory carries both the access and default entries.
        let sub = super::read(root.join("sub").to_str().unwrap()).unwrap();
        assert!(sub
            .acl
            .find(Scope::Default, EntryKind::NamedUser, Some("nobody"))
            .is_some());

        // A child file gets the access entries but no default entries.
        let file = super::read(root.join("file").to_str().unwrap()).unwrap();
        assert!(file
            .acl
            .find(Scope::Access, EntryKind::NamedUser, Some("nobody"))
            .is_some());
        assert!(!file.acl.has_default());

        let _ = std::fs::remove_dir_all(&root);
    }
}
