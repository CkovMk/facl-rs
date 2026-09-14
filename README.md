# facl-rs

A fast, keyboard-driven terminal UI for editing **Linux POSIX ACLs** on any file or directory — see every entry at a glance, tweak permissions with a keystroke, preview the diff, and save atomically.

No graphical stack, no FFI: `facl-rs` reads and writes through the standard `getfacl` / `setfacl` / `stat` / `chown` / `getent` tools you already trust.

## Highlights

- **Master-detail view** — the full ACL on the left, a focused editor on the right.
- **Effective permissions** — always see what the mask actually grants, so a cap is never a surprise.
- **Ownership & mask** — change owner/group and manage the mask without leaving the app.
- **Preview before you commit** — a before/after diff of every change.
- **Recursive apply** — push an ACL down a whole tree with one toggle.
- **Safe** — automatic backups before each save, plus a `--read-only` mode.

## Install

Requires **Rust 1.75+** and the `acl` package (`getfacl` / `setfacl`):

```sh
# Debian/Ubuntu
sudo apt install acl
# Fedora/RHEL
sudo dnf install acl
```

```sh
cargo build --release
./target/release/facl-rs <PATH>
```

### Building distro packages

`cargo build` only produces the `facl-rs` binary — it cannot emit `.deb` or
`.rpm` artifacts on its own. Packaging metadata lives in `Cargo.toml`
(`[package.metadata.deb]` and `[package.metadata.generate-rpm]`) and is
consumed by two optional cargo subcommands:

```sh
# Debian/Ubuntu (.deb)
cargo install cargo-deb
cargo deb                 # builds release + writes target/debian/*.deb

# Fedora/RHEL (.rpm)
cargo install cargo-generate-rpm
cargo build --release
cargo generate-rpm        # writes target/generate-rpm/*.rpm
```

Both packages declare `acl` as a runtime dependency and install the binary to
`/usr/bin/facl-rs` plus docs under `/usr/share/doc/facl-rs/`.

## Usage

```sh
facl-rs                     # edit the current directory
facl-rs /srv/share          # edit a specific path
sudo facl-rs /srv/share     # privileged edit
facl-rs --read-only ./proj  # inspect without changing anything
facl-rs --mask=manual /data # never let the tool touch the mask
```

| Option | Meaning |
| ------ | ------- |
| `PATH` | Path to edit (defaults to the current directory). |
| `--read-only` | Open read-only; saving is disabled. |
| `--mask <POLICY>` | `auto` (default), `preserve`, or `manual`. |

## A look at it

```
POSIX ACL Editor  MODIFIED
Path: /tmp/some_dir
Type: directory   [ ] recursive (t)
Owner (o): ckovmk   Group (g): ckovmk   UGO: 775
┌ACL entries [All]───────────────────────────┐┌Selected entry──────────────────────────────────┐
│  S  Type          Identity            Perm ││Scope: Access                                   │
│  A  owning-user   ckovmk              rwx  ││Type: Named user                                │
│> A  user          www-data            r--  ││Identity: www-data                              │
│  A  owning-group  ckovmk              rwx  ││                                                │
│  A  mask                              rwx  ││Permissions: [x] Read  [ ] Write  [ ] Execute   │
│  A  other                             r-x  ││                                                │
│                                            ││Effective after mask: r--                       │
└────────────────────────────────────────────┘└────────────────────────────────────────────────┘
Add (a)  Delete (d)  Identity (e)  Filter (f)  preview (p)  Save (S)  Reload (R)  Help (?)  Quit (q)
added www-data
```

## Keys

| Key | Action |
| --- | ------ |
| `↑`/`↓` or `j`/`k` | Move between entries |
| `←`/`→` | Move the Read / Write / Execute cursor |
| `Space` | Toggle the permission under the cursor |
| `Tab` / `Shift+Tab` | Switch focus between the list and the editor |
| `a` / `d` / `e` | Add / delete / rename a named user or group entry |
| `o` / `g` | Change the owner / owning group |
| `m` | Jump to the mask entry |
| `f` | Cycle the list filter (All / Access / Default) |
| `t` | Toggle recursive apply (directories only) |
| `p` | Preview pending changes |
| `S` / `R` | Save / reload from disk |
| `?` / `q` | Help / quit (prompts to save or discard if there are unsaved changes) |

Press `?` inside the app for the full reference.

## Contributing

Run `cargo test` to check everything. See [AGENTS.md](AGENTS.md) for the architecture, invariants, and contributor conventions.

## License

Apache-2.0 (see [LICENSE](LICENSE)).
