/*
 * Copyright 2026 Chekhov Ma <maqike@qq.com>
 * SPDX-License-Identifier: Apache-2.0
 */
//! All filesystem access, performed through system commands.
//!
//! This module depends only on `model` and never touches ratatui. It shells
//! out to `stat`, `getfacl`, `setfacl`, `chown` and `getent` so the tool has
//! no FFI or raw-syscall dependencies.

mod identity;
mod ownership;
mod posix_acl;

pub use identity::{Identity, list_groups, list_users, lookup_group, lookup_user};
pub use ownership::{chown, stat};
pub use posix_acl::{AclHandle, backup, parse_getfacl, read, restore, write};

use std::process::Command;

use anyhow::{bail, Context, Result};

/// Run a command and return its stdout as a string.
///
/// Fails if the command cannot be spawned or exits non-zero.
pub fn run_stdout(program: &str, args: &[&str]) -> Result<String> {
    run(program, args, None)
}

/// Run a command feeding `stdin` on standard input and return its stdout.
pub fn run_stdin_stdout(program: &str, args: &[&str], stdin: &str) -> Result<String> {
    run(program, args, Some(stdin))
}

fn run(program: &str, args: &[&str], stdin: Option<&str>) -> Result<String> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    // Force the C locale so `stat`/`getfacl` report type names and messages
    // in English regardless of the user's locale.
    cmd.env("LC_ALL", "C");
    let out = if let Some(data) = stdin {
        use std::io::Write;
        let mut child = cmd
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to spawn `{program}`"))?;
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(data.as_bytes())
            .with_context(|| format!("failed writing stdin to `{program}`"))?;
        child
            .wait_with_output()
            .with_context(|| format!("waiting for `{program}`"))?
    } else {
        cmd.output().with_context(|| format!("failed to spawn `{program}`"))?
    };

    if !out.status.success() {
        bail!(
            "`{} {}` failed: {}",
            program,
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
