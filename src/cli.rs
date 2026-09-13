/*
 * Copyright 2026 Chekhov Ma <maqike@qq.com>
 * SPDX-License-Identifier: Apache-2.0
 */
use clap::{Parser, ValueEnum};

/// Policy for how the ACL mask is treated when an edit changes what it caps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum MaskPolicy {
    /// Recalculate the mask from the affected entries (matches `setfacl`).
    #[default]
    Auto,
    /// Keep the current mask even if it no longer caps all entries.
    Preserve,
    /// Never touch the mask; show a warning if an entry exceeds it.
    Manual,
}

/// Command-line options for `facl-rs`.
#[derive(Debug, Parser, Clone)]
#[command(name = "facl-rs", version, about = "Edit Linux POSIX ACLs in a terminal")]
pub struct Cli {
    /// Path to edit. Defaults to the current directory.
    #[arg(default_value = ".")]
    pub path: String,

    /// Open read-only; saving is disabled.
    #[arg(long)]
    pub read_only: bool,

    /// Mask handling policy applied on save.
    #[arg(long, value_enum, default_value_t = MaskPolicy::Auto)]
    pub mask: MaskPolicy,
}

impl Cli {
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }
}
