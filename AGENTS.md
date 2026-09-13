# AGENTS.md

Guidance for AI agents (and humans) working in this repository.

## What this is

`facl-rs` is a terminal TUI for editing **Linux POSIX ACLs** on a single
filesystem path. It presents a master-detail list of ACL entries plus a detail
editor, ownership editing, mask handling, a change preview, and an
atomic-looking save. It has **no FFI**: all filesystem access goes through
system commands.

## Commands

```sh

cargo build            # compile
cargo test             # run all unit + integration tests
cargo run -- <PATH>    # launch the TUI (needs a real TTY)
cargo clippy --all-targets   # lints (if installed)
cargo fmt              # format (if installed)
```

- The tool only runs against a real terminal; `cargo run` will fail without a
  TTY. Startup errors (e.g. a bad path) print cleanly before raw mode is
  entered, so `cargo run -- /nonexistent` is a safe smoke test.
- `--read-only` disables saving; `--mask=auto|preserve|manual` sets the mask
  policy (default `auto`).

## Toolchain / MSRV

- **Rust 1.75.0** is the floor. Keep it that way.
- Dependencies are pinned in `Cargo.toml`: `ratatui 0.26`, `crossterm 0.27`,
  `clap ~4.4`, `anyhow 1`. Do not bump major versions without re-checking MSRV.
- `unicode-segmentation` (a transitive dep of ratatui) must stay at **1.12.0**;
  newer versions require a newer rustc. If a `cargo update` bumps it, restore
  with:
  ```sh
  cargo update -p unicode-segmentation --precise 1.12.0
  ```

## Code structure and dependency rules

Strict layering — follow it or the build's clarity suffers:

```
model/    pure data, no I/O, no ratatui   (depends on: nothing)
backend/  all FS access via system cmds   (depends on: model)
ui/       ratatui rendering, read-only    (depends on: model + ratatui)
app.rs    state machine + input handling  (depends on: model, backend, cli, event)
event.rs  crossterm event polling
cli.rs    clap arg parsing
main.rs   wiring: parse CLI, terminal, loop
```

- **`ui` is the ONLY module that may import `ratatui`.** `model` and `backend`
  must never touch it.
- `backend` must never import `ui` or `app`.

### Files

- `src/model/{permissions,entry,acl}.rs` — `Permissions`, `AclEntry`, `Acl`,
  `ObjectInfo`.
- `src/backend/{posix_acl,identity,ownership}.rs` — read/write/backup/restore,
  user/group lookup, stat/chown.
- `src/ui/{layout,acl_table,entry_editor,status,dialogs}.rs` — rendering.
- `src/app.rs` — the whole state machine (see below).

## Key invariants

### The ACL is one sorted list

`Acl` stores a **single** `Vec<AclEntry>` (not separate access/default Vecs),
sorted by `AclEntry::order_key()` = `(scope, kind, identity)`. Canonical kind
order: owner, named user, owning group, named group, mask, other. Use the
`Acl` methods (`find`, `insert`, `remove_slot`, `in_scope`, `mask`,
`recalculate_mask`, `effective`, `set_mask`, `normalize`, `to_setfacl_text`)
rather than poking the vec directly.

### The mask

The mask caps the effective permission of named users, the owning group, and
named groups. A mask entry exists **iff** a scope has named entries.
- Call `Acl::normalize()` after any edit so a needed mask is present.
- `effective(entry)` applies the cap; base entries (owner/other) and the mask
  itself are never capped.
- Save-time policy (`MaskPolicy`): `Auto` recalculates, `Preserve` keeps the
  mask, `Manual` never touches it (and surfaces `mask_warnings()`).

### System commands

- Every spawned command sets `LC_ALL=C` (see `backend::run`) so `stat %F` and
  messages are stable regardless of locale.
- `setfacl --set-file=-` reads the ACL from stdin (`backend::write`).
- Backups go to `$XDG_CACHE_HOME/facl-rs/` (else `~/.cache/facl-rs/`).

## App state machine (`app.rs`)

- `App` holds `original` (disk baseline) and `working` (being edited) ACLs plus
  `owner`/`group` and their baselines. `is_modified()` compares working to
  original.
- `selected` indexes the **visible (filtered)** list, not the working list.
  Map to the working list via `selected_working_index()` (uses `ptr::eq`).
- Dialogs are handled by **taking** them out of `App.dialog`
  (`Option::take()`), stepping the owned value, then putting it back unless it
  finished — this avoids double-borrowing `self`.
- Save order: `build_save_acl()` → `backup` → `chown` (if changed) →
  `setfacl` → `reload`.

## Testing

- `cargo test` runs everything. There are 28 tests.
- `app` and `ui` tests build an `App` against a **real temp directory** and
  use the real `getfacl`/`setfacl`, so the `acl` package must be installed and
  the `nobody` user present (it is on standard Linux). Each test uses a unique
  temp dir (atomic counter) so parallel tests don't collide.
- `ui` tests render via ratatui's `TestBackend` (no TTY needed) and assert on
  the resulting buffer text.
- Add tests next to the code they cover (`#[cfg(test)] mod tests` in the same
  file).

## Ratatui 0.26 API notes (things that differ from newer versions)

- Full area is `Frame::size()` — **not** `Frame::area()`.
- Bold is `Style::default().add_modifier(Modifier::BOLD)` — there is no
  `Style::bold()`.
- `Span` has no `add_modifier`; use `span.patch_style(Style::...)` or build the
  styled span directly.
- `Rect` lives at `ratatui::layout::Rect` (also re-exported in the prelude).
- `Block` is moved into `render_widget`; compute `block.inner(area)` **before**
  rendering if you need the inner rect.

## Git workflow

- Commit per logical change with a conventional-style message
  (`feat:`, `refactor:`, `test:`, `fix:`, `chore:`).
- **Bugfixes to an earlier commit: create a `fixup! <sha>` commit** (or a
  normal `fix:` commit that clearly references it). Do **not** squash or amend
  existing commits.
- Do not commit generated artifacts; `target/` is gitignored.
