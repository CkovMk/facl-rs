/*
 * Copyright 2026 Chekhov Ma <maqike@qq.com>
 * SPDX-License-Identifier: Apache-2.0
 */
use anyhow::{bail, Result};

use super::run_stdout;
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
