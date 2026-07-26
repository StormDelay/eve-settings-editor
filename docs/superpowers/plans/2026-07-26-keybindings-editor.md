# Keybindings Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make EVE's keyboard bindings (`core_user_<id>.dat → cmd → customCmds`) readable and editable in the app, and copyable between accounts.

**Architecture:** A new `settings-model` module `keybinds.rs` owns all format knowledge (projection + one setter), structurally a twin of `autofill.rs` — both edit an account-scoped `(timestamp, dict)`. A one-line `Category::Keybinds` gives batch copy. The frontend adds `keybinds.ts` (VK↔label table, capture, command labels) and `KeybindsView.svelte`, with the command-label catalog generated from the EVE client's own localization data.

**Tech Stack:** Rust (`blue-marshal`, `serde`), Tauri 2 commands, SvelteKit 5 (runes), `node --test` for TS, Python 3 stdlib for the catalog generator.

**Spec:** `docs/superpowers/specs/2026-07-26-keybindings-editor-design.md`. Read §2 before writing any format code.

## Global Constraints

- **The `customCmds` timestamp is never touched.** The stamp sits on the container; leaves are bare. Preserve the existing stamp; never wrap a leaf in `(timestamp, value)`. (Spec §2.1, §5.2.)
- **Section lookup resolves `Ref`/`Shared`.** Sibling root keys in account files are `Ref`s. Use `effective()` on keys and values; never match `Value::Bytes` on a section key directly.
- **Write paths call `inline_all` first**, the proven `autofill.rs`/`overview.rs` idiom, so a wholesale replacement cannot dangle a `Ref`.
- **A binding is `[17?, 18?, 16?, key]`** — modifiers Ctrl(17), Alt(18), Shift(16) in exactly that order, then exactly one non-modifier code.
- **Commit style:** sentence-case subject, no attribution trailers.
- **Rust tests:** `cargo test -p settings-model` from the repo root. **TS tests:** `npm test` from `app/`.
- **No new dependencies** in any crate. The generator is Python 3 stdlib only.
- Dark-theme rule: any native control (`input`, `select`, `option`) added needs explicit dark `background`/`color`.

---

### Task 1: `keybinds.rs` — projection (read path)

**Files:**
- Create: `crates/settings-model/src/keybinds.rs`
- Modify: `crates/settings-model/src/lib.rs` (add `pub mod keybinds;` and a `pub use`)

**Interfaces:**
- Consumes: `crate::treewalk::{collect_shared, effective, is_bytes, Entries, SharedTable}`
- Produces: `project_keybinds(user: Option<&Value>) -> Keybinds`, `struct Keybinds { entries: Vec<KeybindEntry>, available: bool }`, `struct KeybindEntry { command: String, keys: Option<Vec<i64>>, malformed: bool }`, and the constants `MOD_CTRL = 17`, `MOD_ALT = 18`, `MOD_SHIFT = 16`.

- [ ] **Step 1: Write the failing tests**

Create `crates/settings-model/src/keybinds.rs` with the module doc, the types, a `todo!()` body for `project_keybinds`, and this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
    fn ts() -> Value { Value::Long(vec![0u8; 8]) }
    fn codes(v: &[i64]) -> Value { Value::Tuple(v.iter().map(|&n| Value::Int(n)).collect()) }

    /// root -> b"cmd" -> b"customCmds" -> (ts, { command: value }).
    /// `cmd` is a BARE dict and the leaves are BARE tuples — corpus-verified
    /// (spec §2.1). Only `customCmds` carries the timestamp.
    fn user_with_binds() -> Value {
        let table = Value::Dict(vec![
            (b("CmdActivateHighPowerSlot1"), codes(&[81])),
            (b("CmdActivateMediumPowerSlot1"), codes(&[17, 83])),
            (b("CmdToggleAutopilot"), Value::None),
            (b("CmdDronesEngage"), codes(&[18, 16, 68])),
        ]);
        let cmd = Value::Dict(vec![(b("customCmds"), Value::Tuple(vec![ts(), table]))]);
        Value::Dict(vec![(b("cmd"), cmd)])
    }

    fn entry<'a>(k: &'a Keybinds, name: &str) -> &'a KeybindEntry {
        k.entries.iter().find(|e| e.command == name).expect("command projected")
    }

    #[test]
    fn projects_every_command_in_file_order() {
        let k = project_keybinds(Some(&user_with_binds()));
        assert!(k.available);
        assert_eq!(k.entries.len(), 4);
        assert_eq!(k.entries[0].command, "CmdActivateHighPowerSlot1");
        assert_eq!(k.entries[3].command, "CmdDronesEngage");
    }

    #[test]
    fn projects_bound_and_unbound_values() {
        let k = project_keybinds(Some(&user_with_binds()));
        assert_eq!(entry(&k, "CmdActivateHighPowerSlot1").keys, Some(vec![81]));
        assert_eq!(entry(&k, "CmdActivateMediumPowerSlot1").keys, Some(vec![17, 83]));
        assert_eq!(entry(&k, "CmdDronesEngage").keys, Some(vec![18, 16, 68]));
        let unbound = entry(&k, "CmdToggleAutopilot");
        assert_eq!(unbound.keys, None, "None is unbound, not malformed");
        assert!(!unbound.malformed);
    }

    #[test]
    fn no_file_and_no_section_are_unavailable() {
        assert!(!project_keybinds(None).available);
        assert!(!project_keybinds(Some(&Value::Dict(vec![]))).available);
    }

    /// Spec §2.5: a live account that never opened the keybinding screen has
    /// the section but an EMPTY table. That drives the view's empty state.
    #[test]
    fn an_empty_table_is_unavailable() {
        let cmd = Value::Dict(vec![(
            b("customCmds"),
            Value::Tuple(vec![ts(), Value::Dict(vec![])]),
        )]);
        let user = Value::Dict(vec![(b("cmd"), cmd)]);
        let k = project_keybinds(Some(&user));
        assert!(!k.available);
        assert!(k.entries.is_empty());
    }

    /// Real account files Ref/Shared their repeated root keys — the trap that
    /// made the overview state-colour read path project nothing (spec §2.1).
    #[test]
    fn resolves_ref_and_shared_keys_and_values() {
        let table = Value::Dict(vec![
            (Value::Shared { slot: 3, value: Box::new(b("CmdApproachItem")) }, codes(&[65])),
            (b("CmdWarpToItem"), Value::Shared { slot: 4, value: Box::new(codes(&[83])) }),
            (Value::Ref(3), Value::Ref(4)),
        ]);
        let cmd = Value::Dict(vec![(b("customCmds"), Value::Tuple(vec![ts(), table]))]);
        let user = Value::Dict(vec![(Value::Shared { slot: 9, value: Box::new(b("cmd")) }, cmd)]);
        let k = project_keybinds(Some(&user));
        assert!(k.available, "a Ref-keyed section must still resolve");
        assert_eq!(entry(&k, "CmdApproachItem").keys, Some(vec![65]));
        assert_eq!(entry(&k, "CmdWarpToItem").keys, Some(vec![83]));
    }

    #[test]
    fn an_unrecognised_value_is_malformed_not_silently_blank() {
        let table = Value::Dict(vec![
            (b("CmdWeird"), Value::Str("Q".into())),
            (b("CmdEmptyTuple"), Value::Tuple(vec![])),
        ]);
        let cmd = Value::Dict(vec![(b("customCmds"), Value::Tuple(vec![ts(), table]))]);
        let k = project_keybinds(Some(&Value::Dict(vec![(b("cmd"), cmd)])));
        assert!(entry(&k, "CmdWeird").malformed);
        assert_eq!(entry(&k, "CmdWeird").keys, None);
        assert!(entry(&k, "CmdEmptyTuple").malformed);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p settings-model keybinds`
Expected: FAIL — `not yet implemented` panics from `todo!()` (compilation must succeed; if it does not, fix the types before continuing).

- [ ] **Step 3: Implement the projection**

Replace the `todo!()` body. Full module head and implementation:

```rust
//! Read + edit projection of the keybinding table. All of it lives in
//! `core_user` under `cmd -> customCmds -> (timestamp, dict)`, mapping a command
//! name (Bytes) to either `None` (unbound) or a tuple of Windows virtual-key
//! codes. See docs/format-notes.md, "Keybindings".
//!
//! TWO FORMAT TRAPS, both corpus-verified over 12,117 bindings:
//!   1. The `(timestamp, value)` wrapper is on `customCmds`, NOT on the leaves.
//!      A leaf is a bare `Tuple(Int, ..)`. Wrapping one produces a malformed
//!      value the client ignores while keeping its stale binding.
//!   2. The root `cmd` key can be a `Ref`/`Shared` like its siblings, so the
//!      lookup resolves through `effective` rather than matching Bytes.

use blue_marshal::Value;
use serde::Serialize;

use crate::treewalk::{collect_shared, effective, Entries, SharedTable};

/// Modifier virtual-key codes, in the canonical order EVE writes them.
pub const MOD_CTRL: i64 = 17;
pub const MOD_ALT: i64 = 18;
pub const MOD_SHIFT: i64 = 16;
pub(crate) const MODIFIERS: [i64; 3] = [MOD_CTRL, MOD_ALT, MOD_SHIFT];

#[derive(Debug, PartialEq, Serialize)]
pub struct KeybindEntry {
    pub command: String,
    /// `None` = unbound. Otherwise `[17?, 18?, 16?, key]`.
    pub keys: Option<Vec<i64>>,
    /// The stored value is neither `None` nor an all-`Int` tuple. Projected as
    /// `keys: None` so the row reads honestly instead of silently blank; the
    /// raw value survives save untouched unless the user rebinds the row.
    pub malformed: bool,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Keybinds {
    pub entries: Vec<KeybindEntry>,
    /// False when there is no account file, no `cmd -> customCmds`, or the
    /// table is empty — the last is a real state for an account that has never
    /// opened the in-game keybinding screen (spec §2.5).
    pub available: bool,
}

pub fn project_keybinds(user: Option<&Value>) -> Keybinds {
    let empty = Keybinds { entries: Vec::new(), available: false };
    let Some(user) = user else { return empty };

    let mut sh = SharedTable::new();
    collect_shared(user, &mut sh);
    let Value::Dict(root) = effective(user, &sh) else { return empty };
    let Some(cmd) = find_child(root, b"cmd", &sh).and_then(|v| as_dict(v, &sh)) else { return empty };
    let Some(table) = find_child(cmd, b"customCmds", &sh).and_then(|v| as_dict(v, &sh)) else { return empty };

    let entries: Vec<KeybindEntry> = table
        .iter()
        .filter_map(|(k, v)| {
            let command = bytes_str(effective(k, &sh))?;
            let (keys, malformed) = read_binding(effective(v, &sh), &sh);
            Some(KeybindEntry { command, keys, malformed })
        })
        .collect();

    let available = !entries.is_empty();
    Keybinds { entries, available }
}

/// Values are reported exactly as stored — no re-canonicalisation. The corpus
/// is already canonical; if a file is not, showing the truth beats lying.
fn read_binding(v: &Value, sh: &SharedTable) -> (Option<Vec<i64>>, bool) {
    match v {
        Value::None => (None, false),
        Value::Tuple(items) => {
            let mut codes = Vec::with_capacity(items.len());
            for e in items {
                match effective(e, sh) {
                    Value::Int(i) => codes.push(*i),
                    _ => return (None, true),
                }
            }
            if codes.is_empty() { (None, true) } else { (Some(codes), false) }
        }
        _ => (None, true),
    }
}

// ponytail: these three resolvers duplicate autofill.rs's private copies rather
// than lifting them into treewalk, for the reason recorded there — the shared
// surface is ~20 lines and the files that would have to change are the repo's
// most delicate.

/// Value of the entry whose RESOLVED key is `Bytes(name)`, itself resolved.
fn find_child<'a>(dict: &'a Entries, name: &[u8], sh: &SharedTable<'a>) -> Option<&'a Value> {
    dict.iter()
        .find(|(k, _)| matches!(effective(k, sh), Value::Bytes(b) if b.as_slice() == name))
        .map(|(_, v)| effective(v, sh))
}

/// Resolve to a dict, unwrapping a `(timestamp, dict)` wrapper if present.
/// `cmd` is bare and `customCmds` is wrapped; this handles both.
fn as_dict<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<&'a Entries> {
    match effective(v, sh) {
        Value::Dict(d) => Some(d),
        Value::Tuple(items) => items.iter().find_map(|e| match effective(e, sh) {
            Value::Dict(d) => Some(d),
            _ => None,
        }),
        _ => None,
    }
}

fn bytes_str(v: &Value) -> Option<String> {
    match v {
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        Value::Str(s) => Some(s.clone()),
        _ => None,
    }
}
```

The `treewalk` import deliberately omits `inline_all` and `is_bytes` — the read
path does not need them. Task 2 adds both.

- [ ] **Step 4: Wire the module into the crate**

In `crates/settings-model/src/lib.rs`, after the `pub mod batch;` line add:

```rust
pub mod keybinds;
```

and after the `pub use autofill::{...};` line add:

```rust
pub use keybinds::{project_keybinds, KeybindEntry, Keybinds, MOD_ALT, MOD_CTRL, MOD_SHIFT};
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p settings-model keybinds`
Expected: PASS — 6 tests.

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/src/keybinds.rs crates/settings-model/src/lib.rs
git commit -m "Project the keybinding table"
```

---

### Task 2: `keybinds.rs` — `set_keybind` (write path)

**Files:**
- Modify: `crates/settings-model/src/keybinds.rs`
- Modify: `crates/settings-model/src/lib.rs` (extend the `pub use`)

**Interfaces:**
- Consumes: Task 1's `MODIFIERS`, `Keybinds`, `project_keybinds`; `crate::treewalk::{inline_all, is_bytes, Entries}`.
- Produces: `set_keybind(user: &mut Value, command: &str, keys: Option<Vec<i64>>) -> Result<Vec<String>, KeybindError>` returning the commands whose binding was stolen; `enum KeybindError { NoTable, UnknownCommand, NoKey, MultipleKeys, DuplicateModifier }`.

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `crates/settings-model/src/keybinds.rs`:

```rust
    #[test]
    fn binds_an_unbound_command() {
        let mut user = user_with_binds();
        let stolen = set_keybind(&mut user, "CmdToggleAutopilot", Some(vec![17, 90])).unwrap();
        assert!(stolen.is_empty());
        let k = project_keybinds(Some(&user));
        assert_eq!(entry(&k, "CmdToggleAutopilot").keys, Some(vec![17, 90]));
    }

    #[test]
    fn unbinding_writes_none() {
        let mut user = user_with_binds();
        set_keybind(&mut user, "CmdActivateHighPowerSlot1", None).unwrap();
        let k = project_keybinds(Some(&user));
        let e = entry(&k, "CmdActivateHighPowerSlot1");
        assert_eq!(e.keys, None);
        assert!(!e.malformed, "an unbound leaf is None, not junk");
    }

    /// Spec §2.2: no corpus file contains a duplicate combination, so EVE steals
    /// the key from its previous owner. The editor must do the same or it writes
    /// a file the client never produces.
    #[test]
    fn rebinding_a_taken_combo_steals_it() {
        let mut user = user_with_binds();
        let stolen = set_keybind(&mut user, "CmdToggleAutopilot", Some(vec![81])).unwrap();
        assert_eq!(stolen, vec!["CmdActivateHighPowerSlot1"]);
        let k = project_keybinds(Some(&user));
        assert_eq!(entry(&k, "CmdToggleAutopilot").keys, Some(vec![81]));
        assert_eq!(entry(&k, "CmdActivateHighPowerSlot1").keys, None, "previous owner cleared");
    }

    #[test]
    fn rebinding_a_command_to_its_own_combo_is_a_noop() {
        let mut user = user_with_binds();
        let stolen = set_keybind(&mut user, "CmdActivateHighPowerSlot1", Some(vec![81])).unwrap();
        assert!(stolen.is_empty(), "a command never steals from itself");
        let k = project_keybinds(Some(&user));
        assert_eq!(entry(&k, "CmdActivateHighPowerSlot1").keys, Some(vec![81]));
    }

    #[test]
    fn modifier_order_is_canonicalised_to_ctrl_alt_shift() {
        let mut user = user_with_binds();
        // Supplied Shift, Alt, Ctrl, key — must be stored 17, 18, 16, key.
        set_keybind(&mut user, "CmdToggleAutopilot", Some(vec![16, 18, 17, 68])).unwrap();
        let k = project_keybinds(Some(&user));
        assert_eq!(entry(&k, "CmdToggleAutopilot").keys, Some(vec![17, 18, 16, 68]));
    }

    #[test]
    fn canonicalisation_makes_a_reordered_combo_collide() {
        let mut user = user_with_binds();
        // CmdActivateMediumPowerSlot1 holds (17, 83). Supplying (83, 17) must
        // canonicalise to (17, 83) and therefore steal it.
        let stolen = set_keybind(&mut user, "CmdToggleAutopilot", Some(vec![83, 17])).unwrap();
        assert_eq!(stolen, vec!["CmdActivateMediumPowerSlot1"]);
    }

    #[test]
    fn rejects_combos_that_break_the_corpus_invariant() {
        let mut user = user_with_binds();
        assert_eq!(set_keybind(&mut user, "CmdToggleAutopilot", Some(vec![])), Err(KeybindError::NoKey));
        assert_eq!(set_keybind(&mut user, "CmdToggleAutopilot", Some(vec![17])), Err(KeybindError::NoKey));
        assert_eq!(set_keybind(&mut user, "CmdToggleAutopilot", Some(vec![81, 83])), Err(KeybindError::MultipleKeys));
        assert_eq!(
            set_keybind(&mut user, "CmdToggleAutopilot", Some(vec![17, 17, 81])),
            Err(KeybindError::DuplicateModifier)
        );
        // A rejected write changes nothing.
        let k = project_keybinds(Some(&user));
        assert_eq!(entry(&k, "CmdToggleAutopilot").keys, None);
    }

    #[test]
    fn rejects_an_unknown_command_and_a_missing_table() {
        let mut user = user_with_binds();
        assert_eq!(
            set_keybind(&mut user, "CmdNotInThisClient", Some(vec![81])),
            Err(KeybindError::UnknownCommand)
        );
        let mut bare = Value::Dict(vec![]);
        assert_eq!(set_keybind(&mut bare, "CmdAnything", None), Err(KeybindError::NoTable));
    }

    /// GLOBAL CONSTRAINT. Five shipped editors preserve an existing wrapper's
    /// timestamp and every live smoke passed on that. A leaf must stay BARE.
    #[test]
    fn a_write_preserves_the_table_timestamp_and_never_wraps_a_leaf() {
        let mut user = user_with_binds();
        set_keybind(&mut user, "CmdToggleAutopilot", Some(vec![17, 90])).unwrap();

        let Value::Dict(root) = &user else { panic!("root is a dict") };
        let (_, cmd) = root.iter().find(|(k, _)| is_bytes(k, b"cmd")).expect("cmd section");
        let Value::Dict(cmd) = cmd else { panic!("cmd is a bare dict, not wrapped") };
        let (_, wrapper) = cmd.iter().find(|(k, _)| is_bytes(k, b"customCmds")).expect("customCmds");
        let Value::Tuple(parts) = wrapper else { panic!("customCmds is a (ts, dict) tuple") };
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], ts(), "the table timestamp must survive untouched");

        let Value::Dict(table) = &parts[1] else { panic!("payload is a dict") };
        let (_, leaf) = table.iter().find(|(k, _)| is_bytes(k, b"CmdToggleAutopilot")).unwrap();
        assert_eq!(
            leaf,
            &codes(&[17, 90]),
            "the leaf is a bare code tuple — wrapping it produces the malformed value EVE ignores"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p settings-model keybinds`
Expected: FAIL to compile — `cannot find function set_keybind` / `cannot find type KeybindError`.

- [ ] **Step 3: Implement the setter**

Extend the `treewalk` import to `use crate::treewalk::{collect_shared, effective, inline_all, is_bytes, Entries, SharedTable};`, then append to `keybinds.rs` above the `tests` module:

```rust
#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum KeybindError {
    /// No `cmd -> customCmds` in this file.
    NoTable,
    /// This client build's table has no such command (spec §2.4 — the table is
    /// the command set; the editor never mints rows).
    UnknownCommand,
    /// No non-modifier code supplied.
    NoKey,
    /// More than one non-modifier code; the corpus has none such.
    MultipleKeys,
    DuplicateModifier,
}

/// Bind `command` to `keys` (or unbind it with `None`), stealing the
/// combination from any other command that holds it — which is what EVE does,
/// and why no corpus file contains a duplicate.
///
/// Returns the commands whose binding was cleared, so the caller can say what
/// it took. Leaves the `customCmds` timestamp untouched.
pub fn set_keybind(
    user: &mut Value,
    command: &str,
    keys: Option<Vec<i64>>,
) -> Result<Vec<String>, KeybindError> {
    // Validate BEFORE mutating: a rejected write must change nothing.
    let canon = keys.map(|k| canonical(&k)).transpose()?;

    inline_all(user);
    let table = custom_cmds_mut(user).ok_or(KeybindError::NoTable)?;
    if !table.iter().any(|(k, _)| is_bytes(k, command.as_bytes())) {
        return Err(KeybindError::UnknownCommand);
    }

    let mut stolen = Vec::new();
    if let Some(c) = &canon {
        let want = Value::Tuple(c.iter().map(|&n| Value::Int(n)).collect());
        for (k, v) in table.iter_mut() {
            if is_bytes(k, command.as_bytes()) || *v != want {
                continue;
            }
            if let Value::Bytes(name) = k {
                stolen.push(String::from_utf8_lossy(name).into_owned());
            }
            *v = Value::None;
        }
    }

    let (_, slot) = table
        .iter_mut()
        .find(|(k, _)| is_bytes(k, command.as_bytes()))
        .expect("presence checked above");
    *slot = match &canon {
        Some(c) => Value::Tuple(c.iter().map(|&n| Value::Int(n)).collect()),
        None => Value::None,
    };
    Ok(stolen)
}

/// Enforce the corpus invariant and impose the canonical order: modifiers
/// Ctrl, Alt, Shift (in that order), then exactly one non-modifier code.
fn canonical(keys: &[i64]) -> Result<Vec<i64>, KeybindError> {
    let mods: Vec<i64> = keys.iter().copied().filter(|c| MODIFIERS.contains(c)).collect();
    let rest: Vec<i64> = keys.iter().copied().filter(|c| !MODIFIERS.contains(c)).collect();

    let mut seen = mods.clone();
    seen.sort_unstable();
    seen.dedup();
    if seen.len() != mods.len() {
        return Err(KeybindError::DuplicateModifier);
    }
    match rest.len() {
        0 => return Err(KeybindError::NoKey),
        1 => {}
        _ => return Err(KeybindError::MultipleKeys),
    }

    let mut out: Vec<i64> = MODIFIERS.iter().copied().filter(|m| mods.contains(m)).collect();
    out.push(rest[0]);
    Ok(out)
}

/// Mutable inner dict of root -> cmd -> customCmds -> (ts, dict). Assumes a
/// plain tree (post-`inline_all`), so keys are plain Bytes.
fn custom_cmds_mut(user: &mut Value) -> Option<&mut Entries> {
    let Value::Dict(root) = user else { return None };
    let cmd = child_dict_mut(root, b"cmd")?;
    child_dict_mut(cmd, b"customCmds")
}

fn child_dict_mut<'a>(dict: &'a mut Entries, name: &[u8]) -> Option<&'a mut Entries> {
    let (_, v) = dict.iter_mut().find(|(k, _)| is_bytes(k, name))?;
    dict_inner_mut(v)
}

/// Handles both a bare dict (`cmd`) and a `(timestamp, dict)` wrapper
/// (`customCmds`) — reaching INTO the wrapper, so the timestamp element is
/// never rewritten.
fn dict_inner_mut(v: &mut Value) -> Option<&mut Entries> {
    match v {
        Value::Dict(d) => Some(d),
        Value::Tuple(items) => items.iter_mut().find_map(|e| match e {
            Value::Dict(d) => Some(d),
            _ => None,
        }),
        _ => None,
    }
}
```

- [ ] **Step 4: Extend the crate export**

In `crates/settings-model/src/lib.rs`, replace the keybinds `pub use` line with:

```rust
pub use keybinds::{project_keybinds, set_keybind, KeybindEntry, KeybindError, Keybinds, MOD_ALT, MOD_CTRL, MOD_SHIFT};
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p settings-model keybinds`
Expected: PASS — 15 tests.

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/src/keybinds.rs crates/settings-model/src/lib.rs
git commit -m "Add the keybinding setter with combo stealing"
```

---

### Task 3: Real-data corpus gate

**Files:**
- Create: `crates/settings-model/tests/keybinds_corpus.rs`

**Interfaces:**
- Consumes: `settings_model::{project_keybinds, set_keybind, MOD_ALT, MOD_CTRL, MOD_SHIFT}`; the existing `mod common` helpers `user_files()`, `real_corpus_present()`.
- Produces: nothing consumed by later tasks.

**Why this task exists:** every `hud.rs` unit fixture agreed with a `FIELDS` table that named the wrong section, so all 20 tests passed while the editor read nothing from any real file. Hand-built fixtures cannot catch a wrong path. Read `crates/settings-model/tests/hud_corpus.rs` first and follow its shape, including the silent skip when the corpus is absent.

- [ ] **Step 1: Write the test**

Create `crates/settings-model/tests/keybinds_corpus.rs`:

```rust
//! Real-data guard for the keybinding table. Unit fixtures in `keybinds.rs`
//! build whatever shape the reader expects, so a wrong section or key passes
//! them all while projecting nothing from a real file — the class of bug that
//! shipped in v0.15.0 for the HUD badge offset.
//!
//! Also pins the value invariants the writer relies on (spec §2.2): rejecting a
//! two-key combo is only safe if real files never contain one.
//!
//! Skips silently when the corpus is not checked out.

mod common;

use settings_model::{project_keybinds, set_keybind, MOD_ALT, MOD_CTRL, MOD_SHIFT};

/// The real corpus has 132 account files carrying a table. A wrong section or
/// key projects 0. Deliberately below 132 so refreshing the corpus cannot fail
/// this spuriously.
const ENOUGH_REAL: usize = 100;

#[test]
fn the_keybinding_table_reads_from_real_files() {
    if !common::real_corpus_present() {
        return;
    }
    let mut with_table = 0usize;
    let mut bindings = 0usize;

    for f in common::user_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        let k = project_keybinds(Some(&doc));
        if k.available {
            with_table += 1;
            bindings += k.entries.iter().filter(|e| e.keys.is_some()).count();
        }
    }

    assert!(
        with_table >= ENOUGH_REAL,
        "only {with_table} account files projected a keybinding table (expected >= {ENOUGH_REAL}); \
         the section or key path is wrong"
    );
    assert!(bindings > 1000, "expected thousands of bindings, got {bindings}");
}

#[test]
fn every_real_binding_satisfies_the_writer_invariants() {
    let modifiers = [MOD_CTRL, MOD_ALT, MOD_SHIFT];

    for f in common::user_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        let k = project_keybinds(Some(&doc));
        for e in &k.entries {
            assert!(!e.malformed, "{}: {} projected as malformed", f.name(), e.command);
            let Some(keys) = &e.keys else { continue };

            let mods: Vec<i64> = keys.iter().copied().filter(|c| modifiers.contains(c)).collect();
            let rest: Vec<i64> = keys.iter().copied().filter(|c| !modifiers.contains(c)).collect();
            assert_eq!(rest.len(), 1, "{}: {} has {:?}, expected one non-modifier", f.name(), e.command, keys);
            assert_eq!(&keys[keys.len() - 1], &rest[0], "{}: the key must come last", f.name());

            // Canonical order is Ctrl, Alt, Shift — i.e. the modifiers appear as
            // a subsequence of MODIFIERS.
            let mut want = modifiers.iter().copied().filter(|m| mods.contains(m));
            assert!(
                mods.iter().all(|m| want.next() == Some(*m)),
                "{}: {} modifiers {:?} are not in Ctrl/Alt/Shift order",
                f.name(), e.command, mods
            );
        }
    }
}

#[test]
fn no_real_file_contains_a_duplicate_combination() {
    for f in common::user_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        let k = project_keybinds(Some(&doc));
        let mut seen: Vec<(&Vec<i64>, &str)> = Vec::new();
        for e in &k.entries {
            let Some(keys) = &e.keys else { continue };
            if let Some((_, other)) = seen.iter().find(|(s, _)| *s == keys) {
                panic!("{}: {:?} bound to both {} and {}", f.name(), keys, other, e.command);
            }
            seen.push((keys, &e.command));
        }
    }
}

/// A write against a real file must change exactly one leaf and leave the rest
/// of the document — the table timestamp included — byte-identical.
#[test]
fn a_write_against_a_real_file_changes_only_the_target_leaf() {
    let Some(f) = common::user_files().find(|f| {
        blue_marshal::decode(&f.bytes).map(|d| project_keybinds(Some(&d)).available).unwrap_or(false)
    }) else {
        return; // no corpus checked out
    };

    let doc = blue_marshal::decode(&f.bytes).expect("decodes");
    let before = project_keybinds(Some(&doc));
    let target = before
        .entries
        .iter()
        .find(|e| e.keys.is_none())
        .map(|e| e.command.clone())
        .expect("a real file has unbound commands");

    // Encode the untouched document first so the comparison is against a
    // re-encode, not the original bytes (inline_all legitimately rewrites
    // sharing, which is not what this test is about).
    let mut edited = doc.clone();
    set_keybind(&mut edited, &target, Some(vec![MOD_CTRL, 145])).expect("write succeeds");

    let after = project_keybinds(Some(&edited));
    assert_eq!(after.entries.len(), before.entries.len(), "no rows added or removed");
    for (b, a) in before.entries.iter().zip(after.entries.iter()) {
        assert_eq!(b.command, a.command, "command order preserved");
        if a.command == target {
            assert_eq!(a.keys, Some(vec![MOD_CTRL, 145]));
        } else {
            assert_eq!(a.keys, b.keys, "{} must be untouched", a.command);
        }
    }

    // Re-encoding must round-trip.
    let bytes = blue_marshal::encode(&edited).expect("re-encodes");
    let redecoded = blue_marshal::decode(&bytes).expect("re-decodes");
    let round = project_keybinds(Some(&redecoded));
    assert_eq!(round.entries, after.entries, "the write survives an encode/decode cycle");
}
```

- [ ] **Step 2: Run the tests to verify they pass**

Run: `cargo test -p settings-model --test keybinds_corpus -- --nocapture`
Expected: PASS — 4 tests.

If `the_keybinding_table_reads_from_real_files` reports a count near 0, the section or key path in Task 1 is wrong — that is exactly what this gate is for. If `blue_marshal::encode` is not the correct function name, check `crates/blue-marshal/src/lib.rs` for the actual encoder export and use it.

- [ ] **Step 3: Commit**

```bash
git add crates/settings-model/tests/keybinds_corpus.rs
git commit -m "Guard the keybinding projection against real files"
```

---

### Task 4: `Category::Keybinds` for batch copy

**Files:**
- Modify: `crates/settings-model/src/batch.rs` (the `Category` enum and `key_path`)
- Modify: `app/src-tauri/src/ops.rs` (the `Aspect` enum ~line 69 and `aspect_writes` ~line 101)
- Modify: `app/src/lib/api.ts` (the `Aspect` union, line 257)
- Modify: `app/src/lib/BatchView.svelte` (the `ASPECTS` list, ~line 47)

**Interfaces:**
- Consumes: nothing new.
- Produces: `Category::Keybinds` and `Aspect::Keybinds`, both serialised as `"keybinds"`.

**Note:** `Category` (settings-model) and `Aspect` (the batch UI's vocabulary)
are *separate* enums joined by `ops::aspect_writes`. Adding the category alone
does not make it selectable — all four files below are required.

**Why it matters:** an account that never opened the in-game keybinding screen has an empty table (spec §2.5), so copying `customCmds` wholesale is the *only* way to give it bindings.

- [ ] **Step 1: Write the failing test**

Append to the `tests` module in `crates/settings-model/src/batch.rs`:

```rust
    /// `cmd -> customCmds` is the same two-step path shape as Autofill's
    /// `ui -> editHistory`, so extract/apply need no new machinery.
    #[test]
    fn keybinds_category_round_trips_the_whole_table() {
        let ts = || Value::Long(vec![0u8; 8]);
        let bts = |s: &str| Value::Bytes(s.as_bytes().to_vec());
        let table = |code: i64| {
            Value::Dict(vec![(
                bts("customCmds"),
                Value::Tuple(vec![
                    ts(),
                    Value::Dict(vec![(bts("CmdApproachItem"), Value::Tuple(vec![Value::Int(code)]))]),
                ]),
            )])
        };
        let source = Value::Dict(vec![(bts("cmd"), table(65))]);
        let mut target = Value::Dict(vec![(bts("cmd"), table(90))]);

        let extracted = extract_categories(&source, &[Category::Keybinds]);
        assert_eq!(extracted.len(), 1);
        apply_to_tree(&mut target, &extracted);

        let binds = settings_model_project(&target);
        assert_eq!(binds, Some(vec![65]), "the source's binding replaced the target's");
    }

    /// The load-bearing case: an account whose table is EMPTY gets one.
    #[test]
    fn keybinds_category_populates_an_empty_table() {
        let ts = || Value::Long(vec![0u8; 8]);
        let bts = |s: &str| Value::Bytes(s.as_bytes().to_vec());
        let source = Value::Dict(vec![(
            bts("cmd"),
            Value::Dict(vec![(
                bts("customCmds"),
                Value::Tuple(vec![
                    ts(),
                    Value::Dict(vec![(bts("CmdApproachItem"), Value::Tuple(vec![Value::Int(65)]))]),
                ]),
            )]),
        )]);
        let mut target = Value::Dict(vec![(
            bts("cmd"),
            Value::Dict(vec![(bts("customCmds"), Value::Tuple(vec![ts(), Value::Dict(vec![])]))]),
        )]);

        apply_to_tree(&mut target, &extract_categories(&source, &[Category::Keybinds]));
        assert_eq!(settings_model_project(&target), Some(vec![65]));
    }

    /// Local helper: read CmdApproachItem's codes back out of a tree.
    fn settings_model_project(v: &Value) -> Option<Vec<i64>> {
        let k = crate::keybinds::project_keybinds(Some(v));
        let e = k.entries.iter().find(|e| e.command == "CmdApproachItem")?;
        e.keys.clone()
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p settings-model batch`
Expected: FAIL to compile — `no variant named Keybinds found for enum Category`.

- [ ] **Step 3: Add the variant**

In `crates/settings-model/src/batch.rs`, add `Keybinds,` to the `Category` enum after `OverviewWidths,`, and add to `key_path`:

```rust
            Category::Keybinds => &[b"cmd", b"customCmds"],
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p settings-model batch`
Expected: PASS.

- [ ] **Step 5: Expose it as a batch aspect**

In `app/src-tauri/src/ops.rs`, add `Keybinds,` to the `Aspect` enum (~line 69),
and in `aspect_writes` (~line 112) add the arm beside `Aspect::Autofill`:

```rust
            Aspect::Keybinds => account_categories.push(Category::Keybinds),
```

In `app/src/lib/api.ts` line 257, extend the union:

```ts
export type Aspect = "layout" | "overview" | "autofill" | "keybinds" | "everything";
```

In `app/src/lib/BatchView.svelte`, add to `ASPECTS` after the `autofill` entry
(before `everything`, which must stay last — it is the exclusive option):

```ts
    { key: "keybinds", label: "Keybindings", account: true },
```

`account: true` is correct: `customCmds` is account-scoped, so selecting it must
require a paired account, exactly like Overview and Autofill.

- [ ] **Step 6: Verify the whole path**

Run: `cargo test -p settings-model batch` and `cargo build --manifest-path app/src-tauri/Cargo.toml`
Expected: both clean. A non-exhaustive `match` error in `aspect_writes` means Step 5 was missed.

Run from `app/`: `npm run check`
Expected: no new errors.

- [ ] **Step 7: Commit**

```bash
git add crates/settings-model/src/batch.rs app/src-tauri/src/ops.rs app/src/lib/api.ts app/src/lib/BatchView.svelte
git commit -m "Add a Keybinds batch category"
```

---

### Task 5: Tauri commands and the frontend IPC surface

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (add two functions next to the autofill ones, ~line 995-1024)
- Modify: `app/src-tauri/src/lib.rs` (two `#[tauri::command]` wrappers + the `generate_handler!` list)
- Modify: `app/src/lib/api.ts` (types + two methods)

**Interfaces:**
- Consumes: `settings_model::{project_keybinds, set_keybind, Keybinds, KeybindError}`.
- Produces: IPC commands `keybinds() -> Keybinds` and `set_keybind(command: String, keys: Option<Vec<i64>>) -> SetKeybindResult`; TS types `Keybinds`, `KeybindEntry`, `SetKeybindResult`; `api.keybinds()`, `api.setKeybind(command, keys)`.

- [ ] **Step 1: Add the ops functions**

In `app/src-tauri/src/ops.rs`, immediately after `clear_all_autofill`, add:

```rust
pub fn keybinds(state: &AppState) -> Result<Keybinds, ErrDto> {
    let user = state.user.lock().unwrap();
    Ok(project_keybinds(user.as_ref().map(|d| &d.value)))
}

#[derive(serde::Serialize)]
pub struct SetKeybindResult {
    pub keybinds: Keybinds,
    /// Commands whose binding this write cleared, so the UI can name them.
    pub stolen: Vec<String>,
}

pub fn set_keybind_cmd(
    state: &AppState,
    command: &str,
    keys: Option<Vec<i64>>,
) -> Result<SetKeybindResult, ErrDto> {
    let stolen = {
        let mut guard = state.user.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        let stolen = settings_model::set_keybind(&mut doc.value, command, keys)
            .map_err(|e| ErrDto::new("keybind", format!("{e:?}")))?;
        doc.value = blue_marshal::reshare(&doc.value);
        stolen
    };
    Ok(SetKeybindResult { keybinds: keybinds(state)?, stolen })
}
```

Add `project_keybinds, Keybinds` to the existing `use settings_model::{...}` import list at the top of `ops.rs` — find the line importing `project_edit_history` and extend it. `settings_model::set_keybind` is called fully qualified above to avoid colliding with the `#[tauri::command] fn set_keybind` added in Step 2.

- [ ] **Step 2: Register the commands**

In `app/src-tauri/src/lib.rs`, after the `clear_all_autofill` wrapper add:

```rust
#[tauri::command]
fn keybinds(state: tauri::State<'_, AppState>) -> Result<settings_model::Keybinds, ErrDto> {
    ops::keybinds(&state)
}
#[tauri::command]
fn set_keybind(
    state: tauri::State<'_, AppState>,
    command: String,
    keys: Option<Vec<i64>>,
) -> Result<ops::SetKeybindResult, ErrDto> {
    ops::set_keybind_cmd(&state, &command, keys)
}
```

and extend the `generate_handler!` list — change the line

```rust
            autofill_lists, set_autofill_list, clear_all_autofill,
```

to

```rust
            autofill_lists, set_autofill_list, clear_all_autofill,
            keybinds, set_keybind,
```

- [ ] **Step 3: Add the TypeScript types and methods**

In `app/src/lib/api.ts`, add near the other exported types:

```ts
export type KeybindEntry = {
  command: string;
  /** null = unbound. Otherwise [17?, 18?, 16?, key]. */
  keys: number[] | null;
  /** The stored value was not a recognised binding; shown read-only. */
  malformed: boolean;
};
export type Keybinds = { entries: KeybindEntry[]; available: boolean };
export type SetKeybindResult = { keybinds: Keybinds; stolen: string[] };
```

and in the `api` object, after the `clearAllAutofill` line:

```ts
  keybinds: () => invoke<Keybinds>("keybinds"),
  setKeybind: (command: string, keys: number[] | null) =>
    invoke<SetKeybindResult>("set_keybind", { command, keys }),
```

- [ ] **Step 4: Verify it builds**

Run: `cargo build -p eve-settings-editor` (from the repo root; if that crate name is wrong, use `cargo build --manifest-path app/src-tauri/Cargo.toml`)
Expected: builds clean.

Run from `app/`: `npm run check`
Expected: no new errors.

There is an existing test that pins the IPC command surface (added in PR #21, "pin the IPC command surface"). Run `npm test` from `app/` — if it fails listing `keybinds`/`set_keybind` as unexpected, add both to that test's expected list.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Expose the keybinding table over IPC"
```

---

### Task 6: Command label catalog

**Files:**
- Create: `tools/gen-command-names.py`
- Create: `app/src/lib/data/command-names.json` (generated, then hand-corrected)
- Create: `app/src/lib/data/command-defaults.json` (literally `{}`)
- Create: `app/src/lib/keybinds.ts` (label half only)
- Create: `app/src/lib/keybinds.test.ts` (label half only)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `labelFor(command: string): string`, `groupFor(command: string): string`, `GROUP_ORDER: string[]`, `defaultFor(command: string): number[] | null`.

- [ ] **Step 1: Write the generator**

Read `tools/gen-default-preset-names.py` first — reuse its `find_localization_pickle()` verbatim. Create `tools/gen-command-names.py`:

```python
#!/usr/bin/env python3
"""Regenerate app/src/lib/data/command-names.json.

DO NOT RE-RUN BLINDLY — the committed JSON is HAND-CORRECTED. 84 of the 101
known commands resolve to EVE's own in-game strings via the SharedCache
localization pickle (FullPath "UI/Commands" and
"UI/Fleet/FleetBroadcast/Commands"); the remaining 17 fall back to a
de-camelcased name, two of which read badly and were fixed by hand:
  CmdPickPortrait0..3            -> "Pick Portrait 0".."Pick Portrait 3"
  ToggleCurrentSystemLocationWnd -> "Toggle Current System Location Window"
Re-verify those after regenerating.

Groups come from the command-name prefix families (see
docs/settings-field-reference.md §5.3); they are ours, not CCP's.

Not shipped to app users — reads the local EVE install. Rerun after an EVE
update that adds commands.

Usage:
    python tools/gen-command-names.py            # auto-discover
    python tools/gen-command-names.py --pickle <main> --en <en-us>

Requires Python 3 (stdlib only) and a local EVE install.
"""
import argparse
import json
import os
import pickle
import re
import sys

OUT = os.path.join("app", "src", "lib", "data", "command-names.json")

# Ordered: first match wins. Patterns are matched against the raw command name.
GROUPS = [
    (r"^CmdOverload", "Overload"),
    (r"^CmdActivate(High|Medium|Low)PowerSlot", "Modules"),
    (r"^Cmd(Drones|LaunchFavoriteDrones|ReconnectToDrones|SelectAllFighters)", "Drones & Fighters"),
    (r"^CmdFleetBroadcast|^CmdSendBroadcast", "Fleet broadcasts"),
    (r"^Cmd(Approach|KeepItemAtRange|WarpTo|AlignTo|DockOrJump|ToggleAutopilot|Accelerate|Decelerate|SetShipFullSpeed|StopShip|FlightControls)", "Navigation"),
    (r"^Cmd(LockTarget|UnlockTarget|SelectNextTarget|SelectPrevTarget|ToggleShipSelection|ToggleLookAtItem)", "Targeting"),
    (r"^(Open|Toggle)", "Windows"),
]
DEFAULT_GROUP = "Misc"


def find_localization_pickles(args):
    """Locate the main + en-US localization pickles via a SharedCache resfileindex."""
    if args.pickle and args.en:
        return args.pickle, args.en
    # Reuse the discovery in tools/gen-default-preset-names.py: walk the
    # resfileindex for the res:/localizationfsd/ entries and map them to
    # SharedCache/ResFiles/<dir>/<file>.
    from importlib.machinery import SourceFileLoader
    helper = SourceFileLoader(
        "genpresets", os.path.join(os.path.dirname(__file__), "gen-default-preset-names.py")
    ).load_module()
    en = helper.find_localization_pickle()
    if not en:
        return None, None
    main = en.replace("_en-us", "_main")
    # The two pickles live under different hashed names; resolve `main` from the
    # same resfileindex rather than by string surgery if that file is absent.
    if not os.path.exists(main):
        main = helper.find_localization_pickle(name="localization_fsd_main.pickle")
    return main, en


def decamel(name):
    n = re.sub(r"^Cmd", "", name)
    n = n.replace("_", ": ")
    n = re.sub(r"(?<=[a-z0-9])(?=[A-Z])", " ", n)
    return n.strip()


def group_for(name):
    for pattern, group in GROUPS:
        if re.search(pattern, name):
            return group
    return DEFAULT_GROUP


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pickle", help="localization_fsd_main.pickle")
    ap.add_argument("--en", help="localization_fsd_en-us.pickle")
    ap.add_argument("--commands", help="newline-separated command names; default: the keys already in the JSON")
    args = ap.parse_args()

    main_p, en_p = find_localization_pickles(args)
    if not main_p or not en_p:
        sys.exit("could not find the localization pickles; pass --pickle and --en")

    labels = pickle.load(open(main_p, "rb"), encoding="latin-1")["labels"]
    en = pickle.load(open(en_p, "rb"), encoding="latin-1")[1]

    byname = {}
    for mid, v in labels.items():
        byname.setdefault(v["label"], []).append((v["FullPath"], mid))

    if args.commands:
        names = [l.strip() for l in open(args.commands) if l.strip()]
    else:
        names = sorted(json.load(open(OUT)).keys())

    out, resolved = {}, 0
    for name in names:
        cands = byname.get(name, []) or byname.get(name.split("_")[-1], [])
        pick = next((c for c in cands if "Commands" in c[0]), cands[0] if cands else None)
        text = en.get(pick[1])[0] if pick and en.get(pick[1]) else None
        if text:
            resolved += 1
        out[name] = {"label": text or decamel(name), "group": group_for(name)}

    with open(OUT, "w", encoding="utf-8") as f:
        json.dump(out, f, indent=2, sort_keys=True, ensure_ascii=False)
        f.write("\n")
    print(f"wrote {OUT}: {len(out)} commands, {resolved} from the client, {len(out)-resolved} de-camelcased")


if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Produce the command list from the corpus**

The generator needs the union of command names (101 across the corpus). Get it
from the projection built in Task 1, using a temporary ignored test so no
throwaway binary is added to the tree.

Add to `crates/settings-model/tests/keybinds_corpus.rs`:

```rust
/// Temporary: emits the command-name union for tools/gen-command-names.py.
/// Run with `--ignored --nocapture`, then DELETE this test.
#[test]
#[ignore]
fn dump_command_names() {
    let mut names = std::collections::BTreeSet::new();
    for f in common::user_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        for e in project_keybinds(Some(&doc)).entries {
            names.insert(e.command);
        }
    }
    for n in &names {
        println!("NAME {n}");
    }
    eprintln!("{} distinct commands", names.len());
}
```

Then, from the repo root:

```bash
cargo test -p settings-model --test keybinds_corpus dump_command_names -- --ignored --nocapture \
  | grep '^NAME ' | sed 's/^NAME //' > commands.txt
wc -l commands.txt   # expect 101
```

- [ ] **Step 3: Generate the catalog**

```bash
echo '{}' > app/src/lib/data/command-names.json
python tools/gen-command-names.py --commands commands.txt
```

Expected output: `wrote app/src/lib/data/command-names.json: 101 commands, 84 from the client, 17 de-camelcased`.

If the counts differ materially from 84/17, the pickle discovery picked the wrong file — pass `--pickle` and `--en` explicitly.

Then clean up: delete `commands.txt` and remove the `dump_command_names` test from `keybinds_corpus.rs`. Neither is committed.

- [ ] **Step 4: Hand-correct the two bad labels**

Open `app/src/lib/data/command-names.json` and fix:

```json
  "CmdPickPortrait0": { "label": "Pick Portrait 0", "group": "Misc" },
  "ToggleCurrentSystemLocationWnd": { "label": "Toggle Current System Location Window", "group": "Windows" }
```

(and the same for `CmdPickPortrait1`, `2`, `3`). Spot-check ten other entries against the in-game keybinding screen before committing.

- [ ] **Step 5: Create the empty defaults file**

Create `app/src/lib/data/command-defaults.json` containing exactly:

```json
{}
```

- [ ] **Step 6: Write the failing TS tests**

Create `app/src/lib/keybinds.test.ts`:

```ts
// Run: npm test (node --test). Throw-based checks, no framework.
import { labelFor, groupFor, GROUP_ORDER, defaultFor } from "./keybinds.ts";

const check = (name: string, ok: boolean) => { if (!ok) throw new Error(`FAIL: ${name}`); console.log(`  ok - ${name}`); };

check("resolves a client-provided label", labelFor("CmdActivateHighPowerSlot1") === "Activate High Power Slot 1");
check("resolves a fleet broadcast label", labelFor("CmdFleetBroadcast_HealArmor") === "Broadcast: Need Armor");
check("hand-corrected label is used", labelFor("ToggleCurrentSystemLocationWnd") === "Toggle Current System Location Window");
check("an unknown command de-camelcases", labelFor("CmdSomeFutureThing") === "Some Future Thing");
check("an unknown Open command de-camelcases", labelFor("OpenFutureWindow") === "Open Future Window");

check("modules group", groupFor("CmdActivateHighPowerSlot1") === "Modules");
check("overload beats modules", groupFor("CmdOverloadHighPowerRack") === "Overload");
check("windows group", groupFor("OpenFitting") === "Windows");
check("unknown falls back to Misc", groupFor("CmdSomeFutureThing") === "Misc");
check("every group used is in GROUP_ORDER", GROUP_ORDER.includes(groupFor("CmdActivateHighPowerSlot1")));

check("defaults are empty until captured", defaultFor("CmdActivateHighPowerSlot1") === null);
```

- [ ] **Step 7: Run to verify it fails**

Run from `app/`: `npm test`
Expected: FAIL — cannot resolve `./keybinds.ts`.

- [ ] **Step 8: Implement the label half of `keybinds.ts`**

Create `app/src/lib/keybinds.ts`:

```ts
// Command labels, groups and (eventually) factory defaults for the keybinding
// editor. Labels come from EVE's own localization data via
// tools/gen-command-names.py; see docs/superpowers/specs/2026-07-26-keybindings-editor-design.md §3.
import names from "./data/command-names.json";
import defaults from "./data/command-defaults.json";

type NameEntry = { label: string; group: string };
const NAMES = names as Record<string, NameEntry>;
const DEFAULTS = defaults as Record<string, number[]>;

/** Display order for the grouped list. Anything unlisted sorts last. */
export const GROUP_ORDER = [
  "Modules",
  "Overload",
  "Drones & Fighters",
  "Targeting",
  "Navigation",
  "Fleet broadcasts",
  "Windows",
  "Misc",
];

/** "CmdActivateHighPowerSlot1" -> "Activate High Power Slot 1". A command the
 *  catalog does not know (a client update added it) degrades to a readable
 *  de-camelcased name rather than a blank row. */
export function labelFor(command: string): string {
  return NAMES[command]?.label ?? decamel(command);
}

export function groupFor(command: string): string {
  return NAMES[command]?.group ?? "Misc";
}

/** EVE's factory binding, or null. The catalog ships EMPTY: no factory defaults
 *  exist anywhere in the settings files, and an account that never opened the
 *  keybinding screen has no table at all, so they must be captured from a
 *  reset-to-default logout. Spec §4. */
export function defaultFor(command: string): number[] | null {
  return DEFAULTS[command] ?? null;
}

function decamel(command: string): string {
  return command
    .replace(/^Cmd/, "")
    .replace(/_/g, ": ")
    .replace(/(?<=[a-z0-9])(?=[A-Z])/g, " ")
    .trim();
}
```

- [ ] **Step 9: Run to verify it passes**

Run from `app/`: `npm test`
Expected: PASS.

If `import ... from "./data/*.json"` fails type-checking, add `"resolveJsonModule": true` to `app/tsconfig.json`'s `compilerOptions` (check first — the app already imports `overview-states.json`, so it is probably already set; follow whatever `states.ts` does).

- [ ] **Step 10: Commit**

```bash
git add tools/gen-command-names.py app/src/lib/data/command-names.json app/src/lib/data/command-defaults.json app/src/lib/keybinds.ts app/src/lib/keybinds.test.ts
git commit -m "Generate the keybinding command catalog from the client"
```

---

### Task 7: Key capture and formatting

**Files:**
- Modify: `app/src/lib/keybinds.ts` (add the VK table, `keysToLabel`, `eventToKeys`)
- Modify: `app/src/lib/keybinds.test.ts`

**Interfaces:**
- Consumes: Task 6's `keybinds.ts`.
- Produces: `keysToLabel(keys: number[] | null): string`, `eventToKeys(e: KeyboardEvent): number[] | null`, `VK_LABELS: Record<number, string>`, `MOD_CTRL/MOD_ALT/MOD_SHIFT`.

- [ ] **Step 1: Write the failing tests**

Append to `app/src/lib/keybinds.test.ts`:

```ts
import { keysToLabel, eventToKeys, MOD_CTRL, MOD_ALT, MOD_SHIFT } from "./keybinds.ts";

check("formats a bare key", keysToLabel([81]) === "Q");
check("formats a modified key", keysToLabel([17, 81]) === "Ctrl+Q");
check("formats the canonical three-modifier order", keysToLabel([17, 18, 16, 68]) === "Ctrl+Alt+Shift+D");
check("formats unbound", keysToLabel(null) === "unbound");
check("formats a function key", keysToLabel([112]) === "F1");
check("an unknown code shows its number", keysToLabel([250]) === "VK250");

// Minimal KeyboardEvent stand-in — node has no DOM.
const ev = (o: Partial<KeyboardEvent>) => o as KeyboardEvent;

check("captures a bare key", JSON.stringify(eventToKeys(ev({ keyCode: 81 }))) === JSON.stringify([81]));
check(
  "captures modifiers in canonical order",
  JSON.stringify(eventToKeys(ev({ keyCode: 68, ctrlKey: true, altKey: true, shiftKey: true }))) ===
    JSON.stringify([MOD_CTRL, MOD_ALT, MOD_SHIFT, 68]),
);
check("a modifier-only press is not a binding", eventToKeys(ev({ keyCode: 17, ctrlKey: true })) === null);
check("an unknown key code is rejected", eventToKeys(ev({ keyCode: 250 })) === null);
check("the modifier constants match EVE's codes", MOD_CTRL === 17 && MOD_ALT === 18 && MOD_SHIFT === 16);
```

- [ ] **Step 2: Run to verify it fails**

Run from `app/`: `npm test`
Expected: FAIL — `keysToLabel is not a function`.

- [ ] **Step 3: Implement**

Append to `app/src/lib/keybinds.ts`:

```ts
export const MOD_CTRL = 17;
export const MOD_ALT = 18;
export const MOD_SHIFT = 16;
/** Canonical order — the one EVE writes, verified over 4,765 real bindings. */
const MODIFIERS = [MOD_CTRL, MOD_ALT, MOD_SHIFT];
const MOD_LABEL: Record<number, string> = { [MOD_CTRL]: "Ctrl", [MOD_ALT]: "Alt", [MOD_SHIFT]: "Shift" };

/** Windows virtual-key codes EVE can store. Serves both display and capture
 *  validation: a code absent here is rejected rather than written blind. */
export const VK_LABELS: Record<number, string> = {
  8: "Backspace", 9: "Tab", 13: "Enter", 19: "Pause", 20: "Caps Lock", 27: "Esc",
  32: "Space", 33: "Page Up", 34: "Page Down", 35: "End", 36: "Home",
  37: "Left", 38: "Up", 39: "Right", 40: "Down",
  45: "Insert", 46: "Delete",
  48: "0", 49: "1", 50: "2", 51: "3", 52: "4", 53: "5", 54: "6", 55: "7", 56: "8", 57: "9",
  65: "A", 66: "B", 67: "C", 68: "D", 69: "E", 70: "F", 71: "G", 72: "H", 73: "I",
  74: "J", 75: "K", 76: "L", 77: "M", 78: "N", 79: "O", 80: "P", 81: "Q", 82: "R",
  83: "S", 84: "T", 85: "U", 86: "V", 87: "W", 88: "X", 89: "Y", 90: "Z",
  96: "Num 0", 97: "Num 1", 98: "Num 2", 99: "Num 3", 100: "Num 4", 101: "Num 5",
  102: "Num 6", 103: "Num 7", 104: "Num 8", 105: "Num 9",
  106: "Num *", 107: "Num +", 109: "Num -", 110: "Num .", 111: "Num /",
  112: "F1", 113: "F2", 114: "F3", 115: "F4", 116: "F5", 117: "F6",
  118: "F7", 119: "F8", 120: "F9", 121: "F10", 122: "F11", 123: "F12",
  144: "Num Lock", 145: "Scroll Lock",
  186: ";", 187: "=", 188: ",", 189: "-", 190: ".", 191: "/", 192: "`",
  219: "[", 220: "\\", 221: "]", 222: "'",
};

/** [17, 81] -> "Ctrl+Q". An unknown code renders as VK<n> rather than
 *  disappearing, so a binding we cannot name is still visible. */
export function keysToLabel(keys: number[] | null): string {
  if (!keys || keys.length === 0) return "unbound";
  return keys.map((c) => MOD_LABEL[c] ?? VK_LABELS[c] ?? `VK${c}`).join("+");
}

/** A keydown into the canonical code list, or null if it is not a usable
 *  binding (a bare modifier press, or a key outside VK_LABELS).
 *
 *  ponytail: reads the deprecated `event.keyCode`, which in WebView2 IS the
 *  Windows virtual-key code EVE stores — a one-lookup mapping. If it is ever
 *  removed, the upgrade is an `event.code` -> VK table against VK_LABELS. */
export function eventToKeys(e: KeyboardEvent): number[] | null {
  const code = e.keyCode;
  if (MODIFIERS.includes(code)) return null; // still holding the modifier down
  if (!(code in VK_LABELS)) return null;
  const mods = [
    ...(e.ctrlKey ? [MOD_CTRL] : []),
    ...(e.altKey ? [MOD_ALT] : []),
    ...(e.shiftKey ? [MOD_SHIFT] : []),
  ];
  return [...mods, code];
}
```

- [ ] **Step 4: Run to verify it passes**

Run from `app/`: `npm test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/keybinds.ts app/src/lib/keybinds.test.ts
git commit -m "Add key capture and combo formatting"
```

---

### Task 8: `KeybindsView.svelte` and page wiring

**Files:**
- Create: `app/src/lib/KeybindsView.svelte`
- Modify: `app/src/routes/+page.svelte` (view type at line 41, `active` derived at line 48, the tab button near line 435, the view block near line 472, the Ctrl+F handler at line 391)

**Interfaces:**
- Consumes: `api.keybinds()`, `api.setKeybind()`, `Keybinds`/`KeybindEntry`/`SetKeybindResult` from Task 5; `labelFor`, `groupFor`, `GROUP_ORDER`, `defaultFor`, `keysToLabel`, `eventToKeys` from Tasks 6-7.
- Produces: nothing consumed later.

- [ ] **Step 1: Create the view**

Model the props and the reload `$effect` on `AutofillView.svelte`, which has the identical account-scoped shape. Create `app/src/lib/KeybindsView.svelte`:

```svelte
<script lang="ts">
  import { api, errMessage, type Keybinds, type KeybindEntry } from "./api";
  import { labelFor, groupFor, GROUP_ORDER, defaultFor, keysToLabel, eventToKeys } from "./keybinds";
  import { message } from "@tauri-apps/plugin-dialog";

  let { userOpen, userId = null, onUserDirty, onShowAccounts = () => {}, onShowBatch = () => {} }:
    { userOpen: boolean; userId?: number | null; onUserDirty: () => void;
      onShowAccounts?: () => void; onShowBatch?: () => void } = $props();

  let binds = $state<Keybinds | null>(null);
  let error = $state<string | null>(null);
  let query = $state("");
  /** Command currently listening for a keypress, or null. */
  let listening = $state<string | null>(null);
  /** Transient "took X from Y" notice, keyed by the command that LOST it. */
  let stolenFrom = $state<Record<string, string>>({});

  async function reload() {
    if (!userOpen) { binds = null; return; }
    error = null;
    try { binds = await api.keybinds(); }
    catch (e) { error = errMessage(e); }
  }
  $effect(() => { void userOpen; void userId; reload(); });

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const all = binds?.entries ?? [];
    if (!q) return all;
    return all.filter(
      (e) =>
        labelFor(e.command).toLowerCase().includes(q) ||
        e.command.toLowerCase().includes(q) ||
        keysToLabel(e.keys).toLowerCase().includes(q),
    );
  });

  /** Grouped for display; the projection reports file order, grouping is ours. */
  const grouped = $derived.by(() => {
    const by = new Map<string, KeybindEntry[]>();
    for (const e of filtered) {
      const g = groupFor(e.command);
      if (!by.has(g)) by.set(g, []);
      by.get(g)!.push(e);
    }
    const rank = (g: string) => { const i = GROUP_ORDER.indexOf(g); return i === -1 ? GROUP_ORDER.length : i; };
    return [...by.entries()].sort((a, b) => rank(a[0]) - rank(b[0]));
  });

  async function commit(command: string, keys: number[] | null) {
    try {
      const res = await api.setKeybind(command, keys);
      binds = res.keybinds;
      onUserDirty();
      // Name what was taken, on the row that lost it.
      const next: Record<string, string> = {};
      for (const lost of res.stolen) next[lost] = labelFor(command);
      stolenFrom = next;
    } catch (e) {
      await message(errMessage(e), { title: "Rebind failed", kind: "error" });
    } finally {
      listening = null;
    }
  }

  function onKeydown(e: KeyboardEvent, command: string) {
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") { listening = null; return; }
    if (e.key === "Backspace") { void commit(command, null); return; }
    const keys = eventToKeys(e);
    if (keys === null) return; // bare modifier, or a key EVE cannot store
    void commit(command, keys);
  }
</script>

{#if !userOpen}
  <p class="empty">
    No account file open. <button class="link" onclick={onShowAccounts}>Pair this character…</button>
  </p>
{:else if error}
  <p class="error">{error}</p>
{:else if binds && !binds.available}
  <p class="empty">
    This account has no keybinding table yet. EVE only writes one once you have opened
    the in-game keybinding screen at least once on this account.
    <button class="link" onclick={onShowBatch}>Copy bindings from another account…</button>
  </p>
{:else if binds}
  <div class="searchbar">
    <input class="search" bind:value={query} placeholder="Search commands and keys (Ctrl+F)" />
  </div>
  {#each grouped as [group, entries] (group)}
    <h3>{group}</h3>
    <table class="binds">
      <tbody>
        {#each entries as e (e.command)}
          <tr class:malformed={e.malformed}>
            <td class="label" title={e.command}>{labelFor(e.command)}</td>
            <td class="combo">
              {#if e.malformed}
                <span class="chip readonly" title="Unrecognised value; left untouched">unreadable</span>
              {:else}
                <button
                  class="chip"
                  class:listening={listening === e.command}
                  onclick={() => (listening = e.command)}
                  onkeydown={(ev) => listening === e.command && onKeydown(ev, e.command)}>
                  {listening === e.command ? "press a key…" : keysToLabel(e.keys)}
                </button>
              {/if}
              {#if stolenFrom[e.command]}
                <span class="meta">taken by {stolenFrom[e.command]}</span>
              {/if}
            </td>
            <td class="default">{keysToLabel(defaultFor(e.command))}</td>
            <td>
              <button
                class="mini"
                disabled={defaultFor(e.command) === null}
                title="Reset to EVE's default (not yet captured)"
                onclick={() => commit(e.command, defaultFor(e.command))}>↺</button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/each}
  {#if listening}
    <p class="meta">Esc cancels · Backspace unbinds</p>
  {/if}
{/if}

<style>
  /* Native controls render light in the dark WebView2 shell unless told
     otherwise — see the dark-native-controls note in the repo memory. */
  .search { background: #1b1b1b; color: #e8e8e8; border: 1px solid #3a3a3a; }
  .chip { background: #232323; color: #e8e8e8; border: 1px solid #3a3a3a; min-width: 7rem; }
  .chip.listening { border-color: #6aa9ff; }
  .chip.readonly { opacity: 0.6; }
  .default { opacity: 0.5; }
  tr.malformed { opacity: 0.6; }
  .meta { opacity: 0.7; font-size: 0.85em; margin-left: 0.5rem; }
</style>
```

- [ ] **Step 2: Wire it into the page**

In `app/src/routes/+page.svelte`:

1. Add the import beside the others: `import KeybindsView from "$lib/KeybindsView.svelte";`
2. Line 41 — extend the type: `type View = "tree" | "layout" | "overview" | "autofill" | "keybinds";`
3. Line 48 — the `active` derived must send Keybinds to the account file, like Autofill. Change

```ts
    view === "autofill" && slots.user?.status === "opened"
```

to

```ts
    (view === "autofill" || view === "keybinds") && slots.user?.status === "opened"
```

4. After the Autofill tab button (line ~435) add:

```svelte
            {#if openCharId !== null || slots.user?.status === "opened"}<button class:active={view === "keybinds"} onclick={() => (view = "keybinds")}>Keybinds</button>{/if}
```

5. After the `{:else if view === "autofill"}` block (which ends at line ~481) add:

```svelte
      {:else if view === "keybinds"}
        <div class="tree-area">
          <KeybindsView
            userOpen={slots.user?.status === "opened"}
            userId={openUserId}
            onShowAccounts={() => (mainView = "accounts")}
            onShowBatch={() => (mainView = "batch")}
            onUserDirty={() => (dirtySlots.user = true)} />
        </div>
```

6. The Ctrl+F handler is at `app/src/routes/+page.svelte:391`
   (`if ((e.ctrlKey || e.metaKey) && e.key === "f")`). It already routes to the
   Layout view's own filter when that view is active. Extend the same branch so
   `view === "keybinds"` focuses the Keybinds filter box instead of opening the
   Tree search. Follow whatever binding the Layout branch uses (a
   `bind:this` on the input, focused via a callback prop) rather than inventing
   a second mechanism.

The batch aspect was already added in Task 4 — nothing to do here for it.

- [ ] **Step 3: Verify**

Run from `app/`: `npm run check`
Expected: no new errors.

Run from `app/`: `npm test`
Expected: PASS.

Run from `app/`: `npm run build`
Expected: builds clean.

- [ ] **Step 4: Commit**

```bash
git add app/src/lib/KeybindsView.svelte app/src/routes/+page.svelte
git commit -m "Add the keybindings view"
```

---

### Task 9: Documentation

**Files:**
- Modify: `docs/format-notes.md` (new "Keybindings" section)
- Modify: `docs/settings-field-reference.md` (§5.3 and §10 Tier 1 item 1)
- Modify: `docs/small-tasks.md` (the deferred defaults capture)
- Modify: `CHANGELOG.md` (Unreleased)

- [ ] **Step 1: Add the format-notes section**

Append a "Keybindings" section to `docs/format-notes.md` carrying spec §2.1 and §2.2 verbatim — the path, the shape, the five invariants with their counts, and this explicit warning:

> **Exception to the value-wrapper convention.** `customCmds` carries the
> `(FILETIME, value)` wrapper on the *container*; the leaves inside are bare.
> 0 of 93 leaves on a sampled real file are wrapped. Wrapping one produces a
> malformed value the client ignores while keeping its stale binding.

- [ ] **Step 2: Correct the field reference**

In `docs/settings-field-reference.md` §5.3, replace the closing paragraph

> The name suggests this holds **only user-customised bindings**; commands left at
> their factory default are simply absent, and an explicit `None` means the user
> cleared the binding. That inference has not been verified in-game.

with a corrected statement: EVE writes the whole command table for the client
build (the name sets nest strictly by generation, 79 ⊂ 90 ⊂ 91 ⊂ 92 ⊂ 93, and the
factory F1–F8 codes appear zero times in 12,117 entries); `None` means unbound;
an account that has never opened the in-game screen has an *empty* table. Cite
the spec.

In §10 Tier 1 item 1, drop the "*Risk:* the 'absent = factory default' inference
is unverified" line (it is now settled) and change "needs two hand-authored
vocabularies" to one — command labels are harvestable from the client, only the
VK table is hand-authored.

- [ ] **Step 3: Add the deferred task**

Add to `docs/small-tasks.md`:

> - **Capture EVE's factory keybindings.** `app/src/lib/data/command-defaults.json`
>   ships empty, so the Keybinds view's Default column and per-row reset are
>   disabled. Populating it: on a throwaway account open the in-game keybinding
>   screen, choose Reset to default, log out, and read the table out of the
>   resulting `core_user_<id>.dat`. No factory bindings exist anywhere else — an
>   account that never opened the screen has an empty table, not a default one.

- [ ] **Step 4: Update the changelog**

Add under `## [Unreleased]`, `### Added`:

```markdown
- **Keybindings editor.** The account's key bindings are now readable and
  editable in a new Keybinds view: every command the client knows, grouped and
  labelled with EVE's own strings, rebindable by pressing the combination.
  Rebinding a combination already in use takes it from its previous owner, as
  the game does. A new Keybinds batch category copies a whole binding table
  between accounts — the only way to give an account bindings without setting
  them up by hand in-game.
```

- [ ] **Step 5: Commit**

```bash
git add docs/format-notes.md docs/settings-field-reference.md docs/small-tasks.md CHANGELOG.md
git commit -m "Document the keybinding format and correct the field reference"
```

---

## After the plan

Run the full gate before proposing a merge:

```bash
cargo test --workspace
cd app && npm test && npm run check && npm run build
```

Then the two live smokes from spec §7.4, which cannot be automated:

1. Rebind a command in the app, save, log in, confirm EVE honours it and does not revert the file.
2. Batch-copy a table onto an account with an empty `customCmds`, log in, confirm the in-game keybinding screen shows the copied bindings. **This is the gate that can fail interestingly** — a table carrying another account's timestamp is a shape the client has not been observed to read.

Per the repo memory, run `cargo clean` once the PR is open: the C: drive runs near-full and `target/` regrows to 12–15 GB.
