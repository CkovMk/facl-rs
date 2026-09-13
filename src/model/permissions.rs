/*
 * Copyright 2026 Chekhov Ma <maqike@qq.com>
 * SPDX-License-Identifier: Apache-2.0
 */
use std::fmt;

use anyhow::{bail, Result};

/// A single permission bit, in the fixed r, w, x order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermBit {
    Read,
    Write,
    Execute,
}

impl PermBit {
    /// The three bits in display order.
    pub const ALL: [PermBit; 3] = [PermBit::Read, PermBit::Write, PermBit::Execute];

    pub fn label(self) -> &'static str {
        match self {
            PermBit::Read => "Read",
            PermBit::Write => "Write",
            PermBit::Execute => "Execute",
        }
    }

    /// The single-character representation used in `getfacl` output.
    pub fn on_char(self) -> char {
        match self {
            PermBit::Read => 'r',
            PermBit::Write => 'w',
            PermBit::Execute => 'x',
        }
    }

    /// The value of this bit within a [`Permissions`].
    pub fn get(self, p: Permissions) -> bool {
        match self {
            PermBit::Read => p.read,
            PermBit::Write => p.write,
            PermBit::Execute => p.execute,
        }
    }
}

/// The three classic read/write/execute permission bits for one ACL entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Permissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl Permissions {
    pub const NONE: Permissions = Permissions {
        read: false,
        write: false,
        execute: false,
    };
    #[allow(dead_code)]
    pub const ALL: Permissions = Permissions {
        read: true,
        write: true,
        execute: true,
    };

    pub fn new(read: bool, write: bool, execute: bool) -> Self {
        Self {
            read,
            write,
            execute,
        }
    }

    #[allow(dead_code)]
    pub fn from_bits(bits: u8) -> Self {
        Self {
            read: bits & 4 != 0,
            write: bits & 2 != 0,
            execute: bits & 1 != 0,
        }
    }

    /// The numeric 0-7 value of these bits.
    #[allow(dead_code)]
    pub fn bits(self) -> u8 {
        let mut b = 0;
        if self.read {
            b |= 4;
        }
        if self.write {
            b |= 2;
        }
        if self.execute {
            b |= 1;
        }
        b
    }

    /// Parse a three-character permission string such as `"r-x"` or `"rwx"`.
    pub fn from_perm_string(s: &str) -> Result<Self> {
        let bytes = s.as_bytes();
        if bytes.len() != 3 {
            bail!("invalid permission string: {s:?} (expected 3 characters)");
        }
        let read = parse_char(bytes[0], 'r')?;
        let write = parse_char(bytes[1], 'w')?;
        // Accept 'x' as well as 's' (setuid/sticky exec) as the execute bit.
        let execute = match bytes[2] {
            b'x' | b's' | b't' => true,
            b'-' => false,
            c => bail!("invalid permission character: '{}'", c as char),
        };
        Ok(Self {
            read,
            write,
            execute,
        })
    }

    /// Bitwise OR of two permission sets.
    pub fn union(self, other: Self) -> Self {
        Self {
            read: self.read || other.read,
            write: self.write || other.write,
            execute: self.execute || other.execute,
        }
    }

    /// Bitwise AND of two permission sets.
    pub fn intersect(self, other: Self) -> Self {
        Self {
            read: self.read && other.read,
            write: self.write && other.write,
            execute: self.execute && other.execute,
        }
    }

    /// Flip a single bit, used by the permission checkboxes.
    pub fn toggle(&mut self, bit: PermBit) {
        match bit {
            PermBit::Read => self.read = !self.read,
            PermBit::Write => self.write = !self.write,
            PermBit::Execute => self.execute = !self.execute,
        }
    }

    #[allow(dead_code)]
    pub fn is_none(self) -> bool {
        !self.read && !self.write && !self.execute
    }

    #[allow(dead_code)]
    pub fn is_all(self) -> bool {
        self.read && self.write && self.execute
    }

    /// `true` when `self` grants a strict superset of `other`.
    pub fn covers(self, other: Self) -> bool {
        self.intersect(other) == other
    }
}

fn parse_char(c: u8, expected: char) -> Result<bool> {
    match c {
        e if e == expected as u8 => Ok(true),
        b'-' => Ok(false),
        c => bail!("invalid permission character: '{}'", c as char),
    }
}

impl fmt::Display for Permissions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = String::with_capacity(3);
        for bit in PermBit::ALL {
            s.push(if bit.get(*self) { bit.on_char() } else { '-' });
        }
        f.write_str(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_round_trip() {
        for s in ["rwx", "r-x", "---", "r--", "--x", "rw-", "r-x", "-w-", "-wx"] {
            let p = Permissions::from_perm_string(s).unwrap();
            assert_eq!(p.to_string(), s, "round-trip failed for {s}");
        }
    }

    #[test]
    fn rejects_bad_strings() {
        assert!(Permissions::from_perm_string("rw").is_err());
        assert!(Permissions::from_perm_string("xyz").is_err());
        assert!(Permissions::from_perm_string("r--x").is_err());
    }

    #[test]
    fn bits_round_trip() {
        assert_eq!(Permissions::ALL.bits(), 7);
        assert_eq!(Permissions::NONE.bits(), 0);
        assert_eq!(Permissions::from_bits(5), Permissions::from_perm_string("r-x").unwrap());
        assert_eq!(Permissions::from_bits(3), Permissions::from_perm_string("-wx").unwrap());
    }

    #[test]
    fn set_ops() {
        let a = Permissions::from_perm_string("rwx").unwrap();
        let b = Permissions::from_perm_string("r-x").unwrap();
        assert_eq!(a.union(b), a);
        assert_eq!(a.intersect(b), b);
        assert!(a.covers(b));
        assert!(!b.covers(a));
    }

    #[test]
    fn toggle_flips_single_bit() {
        let mut p = Permissions::from_perm_string("r-x").unwrap();
        p.toggle(PermBit::Write);
        assert_eq!(p, Permissions::ALL);
        p.toggle(PermBit::Read);
        assert_eq!(p, Permissions::from_perm_string("-wx").unwrap());
    }
}
