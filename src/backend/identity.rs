/*
 * Copyright 2026 Chekhov Ma <maqike@qq.com>
 * SPDX-License-Identifier: Apache-2.0
 */
use super::run_stdout;
use anyhow::Result;

/// A resolved user or group identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    pub id: u32,
}

/// Look up a user by name or numeric id.
pub fn lookup_user(name_or_id: &str) -> Result<Identity> {
    lookup("passwd", name_or_id)
}

/// Look up a group by name or numeric id.
pub fn lookup_group(name_or_id: &str) -> Result<Identity> {
    lookup("group", name_or_id)
}

fn lookup(database: &str, name_or_id: &str) -> Result<Identity> {
    let out = run_stdout("getent", &[database, name_or_id])?;
    parse_line(&out)
        .ok_or_else(|| anyhow::anyhow!("no {database} entry for '{name_or_id}'"))
}

/// All users known to the system (from `getent passwd`).
pub fn list_users() -> Result<Vec<Identity>> {
    list("passwd")
}

/// All groups known to the system (from `getent group`).
pub fn list_groups() -> Result<Vec<Identity>> {
    list("group")
}

fn list(database: &str) -> Result<Vec<Identity>> {
    let out = run_stdout("getent", &[database])?;
    Ok(out
        .lines()
        .filter_map(parse_line)
        .collect::<Vec<Identity>>())
}

/// Parse a single `getent` line: `name:password:numeric_id:...`.
fn parse_line(line: &str) -> Option<Identity> {
    // `splitn(4, ...)` isolates the numeric id (field 2) from the gecos
    // field, which itself may contain colons.
    let fields: Vec<&str> = line.splitn(4, ':').collect();
    if fields.len() < 3 {
        return None;
    }
    let name = fields[0].to_string();
    let id: u32 = fields[2].parse().ok()?;
    Some(Identity { name, id })
}

#[cfg(test)]
mod tests {
    use super::parse_line;

    #[test]
    fn parses_passwd_line() {
        let id = parse_line("alice:x:1000:1000:Alice:/home/alice:/bin/sh").unwrap();
        assert_eq!(id.name, "alice");
        assert_eq!(id.id, 1000);
    }

    #[test]
    fn parses_group_line() {
        let id = parse_line("backup:*:1001:alice,bob").unwrap();
        assert_eq!(id.name, "backup");
        assert_eq!(id.id, 1001);
    }

    #[test]
    fn rejects_bad_id() {
        assert!(parse_line("nobody:x:notanumber:x").is_none());
        assert!(parse_line("short:x").is_none());
    }

    /// Names and numeric ids are both accepted for lookups (getent handles
    /// both), so users may enter a raw uid/gid when creating entries or
    /// changing ownership.
    #[test]
    fn lookup_accepts_numeric_ids() {
        // uid/gid 0 (root) exists on every standard system.
        assert_eq!(super::lookup_user("0").unwrap().name, "root");
        assert_eq!(super::lookup_group("0").unwrap().name, "root");
        // A bogus numeric id is rejected, not treated as a literal name.
        assert!(super::lookup_user("999999999").is_err());
    }
}
