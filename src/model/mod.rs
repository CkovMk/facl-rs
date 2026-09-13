/*
 * Copyright 2026 Chekhov Ma <maqike@qq.com>
 * SPDX-License-Identifier: Apache-2.0
 */
//! Pure data types for POSIX ACLs.
//!
//! This module has no I/O and no dependency on ratatui; it is the shared
//! vocabulary used by both the backend and the UI layers.

mod acl;
mod entry;
mod permissions;

pub use acl::{Acl, ObjectInfo};
pub use entry::{AclEntry, EntryKind, Scope};
pub use permissions::{Permissions, PermBit};
