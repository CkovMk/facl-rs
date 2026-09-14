/*
 * Copyright 2026 Chekhov Ma <maqike@qq.com>
 * SPDX-License-Identifier: Apache-2.0
 */
use anyhow::{bail, Context, Result};

use super::run_stdout;
use crate::backend::identity::lookup_user;
use crate::model::ObjectInfo;


/// Resolve `path`, confirm it exists, and gather its `stat` metadata.
///
/// The returned [`ObjectInfo`] carries a clean absolute path for display plus
/// the type, owner, group and octal mode of the object.
pub fn stat(path: &str) -> Result<ObjectInfo> {
    let (file_type, owner, group, mode) = {
        let out = run_stdout("stat", &["--printf", "%F|%U|%G|%a", "--", path])
            .map_err(|e| {
                anyhow::anyhow!(
                    "path '{path}' does not exist or is not accessible: {e}"
                )
            })?;
        let parts: Vec<&str> = out.trim_end_matches('\n').splitn(4, '|').collect();
        if parts.len() != 4 {
            bail!("unexpected `stat` output: {out:?}");
        }
        (
            parts[0].to_string(),
            parts[1].to_string(),
            parts[2].to_string(),
            parts[3].to_string(),
        )
    };

    if file_type == "symbolic link" {
        bail!("refusing to edit a symbolic link: {path}");
    }

    // `realpath -s` makes the path absolute and collapses `.`/`..` without
    // resolving symlinks, so the displayed path stays unambiguous.
    let path = run_stdout("realpath", &["-s", "--", path])?;
    let path = path.trim().to_string();

    Ok(ObjectInfo {
        is_directory: file_type == "directory",
        file_type,
        path,
        owner,
        group,
        mode,
    })
}

/// Change the owner and/or owning group of `path`.
///
/// A `None` component leaves that part unchanged: an owner-only change passes
/// `user`, a group-only change passes `:group`. With `recursive`, the change
/// is applied to every descendant (`chown -R`).
pub fn chown(path: &str, owner: Option<&str>, group: Option<&str>, recursive: bool) -> Result<()> {
    let spec = match (owner, group) {
        (Some(o), Some(g)) => format!("{o}:{g}"),
        (Some(o), None) => o.to_string(),
        (None, Some(g)) => format!(":{g}"),
        (None, None) => return Ok(()),
    };
    let mut args = Vec::new();
    if recursive {
        args.push("-R");
    }
    args.push("--");
    args.push(&spec);
    args.push(path);
    run_stdout("chown", &args)?;
    Ok(())
}

/// The effective user id of the current process (`id -u`).
pub fn current_uid() -> Result<u32> {
    let out = run_stdout("id", &["-u"]).context("cannot determine current user id")?;
    out.trim()
        .parse::<u32>()
        .with_context(|| format!("unexpected `id -u` output: {out:?}"))
}

/// Whether the current user is allowed to modify the ACL (and ownership) of
/// `object`.
///
/// On Linux, changing an ACL with `setfacl` requires being the file owner or
/// the superuser (uid 0); group members and others cannot. This mirrors that
/// kernel rule so the app can drop into read-only mode up front instead of
/// failing at save time.
pub fn can_modify(object: &ObjectInfo) -> Result<bool> {
    let uid = current_uid()?;
    if uid == 0 {
        return Ok(true);
    }
    // Compare the object's owner against the current user. `object.owner` may
    // be a name or a raw uid (when unresolvable); resolve it to a uid so both
    // forms compare correctly.
    let owner_uid = lookup_user(&object.owner).map(|id| id.id).ok();
    Ok(owner_uid == Some(uid))
}

#[cfg(test)]
mod tests {
    use super::{can_modify, current_uid, stat};

    /// A freshly created temp file is owned by the current user, so its ACL is
    /// modifiable and the app should not be forced into read-only mode.
    #[test]
    fn can_modify_a_file_we_own() {
        let path = std::env::temp_dir()
            .join(format!("facl_rs_perm_{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::fs::File::create(&path).unwrap();
        let object = stat(path.to_str().unwrap()).unwrap();
        assert!(can_modify(&object).unwrap(), "should be able to modify a file we own");
        let _ = std::fs::remove_file(&path);
    }

    /// A system-owned object (root:/etc/passwd) is not modifiable unless we are
    /// root; a non-root test process must be denied.
    #[test]
    fn cannot_modify_a_root_owned_file_as_non_root() {
        if current_uid().unwrap() == 0 {
            // Running as root can modify anything, so this case does not apply.
            return;
        }
        let object = match stat("/etc/passwd") {
            Ok(o) => o,
            Err(_) => return, // Skip if the path is unavailable in this env.
        };
        assert!(!can_modify(&object).unwrap());
    }
}


