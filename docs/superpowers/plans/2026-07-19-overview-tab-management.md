# Overview Tab Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add structural editing of overview tabs — create, rename, delete, reorder, and move tabs between windows — to the existing overview editor, plus a capture tool that unblocks the follow-up window add/remove work.

**Architecture:** A new `crates/settings-model/src/overview_tabs.rs` module holds the structural edits, mirroring `stacks.rs`: each entry point inlines the whole tree (drops all `Shared`/`Ref` sharing), edits plain dicts/lists under the user file's `overview` container, and the app layer reshares before save. The read projection gains one field (each tab's preset name). Thin `tab_*` commands in `ops.rs` mirror the `stack_*` commands. UI lives in the existing `OverviewView.svelte`.

**Tech Stack:** Rust (settings-model crate, Tauri app crate), Svelte 5, `blue_marshal` codec, `node --test` for frontend, `cargo test` for Rust.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-19-overview-tab-management-design.md`.
- Repo convention: sentence-case commit subjects, **no attribution trailers**.
- Structural edits use the inline-first idiom: `inline_all(v)` at the top of every edit fn; the app layer runs `blue_marshal::reshare` after the edit (never encode an inlined tree).
- `blue_marshal` shares `Bytes`/`Long`/`List`/`Dict` — **never `Str`**; string *values* still encode fine, but shared fixtures must use `Bytes`.
- Overview tab structure lives in the **user (`core_user`) file**, under `overview` → `tabsettings_new` (index-keyed dict, `(timestamp, dict)`-wrapped) and `overview` → `tabsByWindowInstanceID` (list of lists; window → tab indices).
- No character/account ids in code, tests, or docs (repo data rule); use synthetic tokens.
- Frontend DOM interactions are validated by manual smoke, not unit tests (project norm, per M2 / overview editor).
- Cargo is on the Bash tool PATH; `gh`/`npm` are **not** — use the PowerShell tool for `npm`. Frontend tests: `npm test` in `app/`.

---

### Task 1: Read projection — expose each tab's preset name

Add the tab's `overview` (filter-preset name) to the read projection so a created
tab can copy a sibling's preset and the UI can show it.

**Files:**
- Modify: `crates/settings-model/src/overview.rs` (the `OverviewTab` struct + `project_tab`)
- Test: `crates/settings-model/src/overview.rs` (existing `#[cfg(test)]` module)

**Interfaces:**
- Produces: `OverviewTab.preset: String` — the tab's `overview` field value, or `""` when the tab has none.

- [ ] **Step 1: Write the failing test**

Add to the `overview.rs` test module:

```rust
#[test]
fn project_tab_exposes_preset_name() {
    // A user tree: overview -> tabsettings_new (ts,dict) -> {0: {name, overview:"P"}}
    let tab = Value::Dict(vec![
        (Value::Str("name".into()), Value::Str("Main".into())),
        (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
    ]);
    let overview = Value::Dict(vec![
        (Value::Bytes(b"tabsettings_new".to_vec()),
         Value::Dict(vec![(Value::Int(0), tab)])),
    ]);
    let user = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), overview)]);

    let cols = project_overview(&user, None);
    assert_eq!(cols.tabs.len(), 1);
    assert_eq!(cols.tabs[0].preset, "P");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p settings-model project_tab_exposes_preset_name`
Expected: FAIL — `no field 'preset' on OverviewTab`.

- [ ] **Step 3: Add the field and populate it**

In `OverviewTab` (currently `{ index, name, inherits, columns }`) add:

```rust
    pub preset: String,
```

In `project_tab`, after `let name = ...;` add:

```rust
    let preset = str_field_r(fields, "overview", sh).unwrap_or_default();
```

and add `preset` to the returned `OverviewTab { index, name, preset, inherits, columns }`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p settings-model overview`
Expected: PASS (the new test and all existing overview tests; the added field is additive).

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/overview.rs
git commit -m "Expose each overview tab's preset name in the projection"
```

---

### Task 2: `overview_tabs.rs` scaffold + traversal helpers + `rename_tab`

Create the module, its error type, the shared traversal helpers, and the simplest
edit (rename) that exercises them.

**Files:**
- Create: `crates/settings-model/src/overview_tabs.rs`
- Modify: `crates/settings-model/src/lib.rs` (add `mod overview_tabs;` and re-exports)
- Test: `crates/settings-model/src/overview_tabs.rs` (inline `#[cfg(test)]`)

**Interfaces:**
- Consumes: `crate::treewalk::{inline_all, is_bytes, Entries}`, `blue_marshal::Value`.
- Produces:
  - `pub enum OverviewTabError { NoOverview, UnknownTab{index:i64}, UnknownWindow{index:usize}, LastTab, LastWindow }` (Serialize with `#[serde(tag="code")]`, Display).
  - `pub fn rename_tab(v:&mut Value, tab_idx:i64, name:&str) -> Result<(), OverviewTabError>`
  - crate-private helpers `overview_mut`, `tabs_mut`, `groups_mut`, `dict_inner_mut`, `list_inner_mut`, `as_int`, `is_b`, `set_name` (used by later tasks).

- [ ] **Step 1: Write the failing test**

Create `crates/settings-model/src/overview_tabs.rs` with only the test at first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    /// user tree: overview -> tabsettings_new (bare dict) -> {0:{name,overview:"P"}}
    fn user_with_tabs() -> Value {
        let tab = Value::Dict(vec![
            (Value::Str("name".into()), Value::Str("Main".into())),
            (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
        ]);
        let overview = Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()),
             Value::Dict(vec![(Value::Int(0), tab)])),
            (Value::Bytes(b"tabsByWindowInstanceID".to_vec()),
             Value::List(vec![Value::List(vec![Value::Int(0)])])),
        ]);
        Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), overview)])
    }

    fn tab_name(v: &Value, idx: i64) -> String {
        let Value::Dict(root) = v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, tabs) = ovd.iter().find(|(k, _)| is_b(k, b"tabsettings_new")).unwrap();
        let Value::Dict(td) = tabs else { panic!() };
        let (_, tab) = td.iter().find(|(k, _)| as_int(k) == Some(idx)).unwrap();
        let Value::Dict(fields) = tab else { panic!() };
        fields.iter().find_map(|(k, val)| match (k, val) {
            (Value::Str(s), Value::Str(name)) if s == "name" => Some(name.clone()),
            (Value::Str(s), Value::StrUcs2(name)) if s == "name" => Some(name.clone()),
            _ => None,
        }).unwrap()
    }

    #[test]
    fn rename_sets_the_name_field() {
        let mut v = user_with_tabs();
        rename_tab(&mut v, 0, "Combat").unwrap();
        assert_eq!(tab_name(&v, 0), "Combat");
    }

    #[test]
    fn rename_unknown_tab_errors() {
        let mut v = user_with_tabs();
        assert!(matches!(rename_tab(&mut v, 9, "X"), Err(OverviewTabError::UnknownTab { index: 9 })));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p settings-model overview_tabs`
Expected: FAIL — module has no `rename_tab` / helpers (compile error).

- [ ] **Step 3: Write the module above the test**

Prepend to `overview_tabs.rs`:

```rust
//! Structural authoring for overview tabs: edit the user file's `overview`
//! container — `tabsettings_new` (index-keyed tab dict) and
//! `tabsByWindowInstanceID` (window -> tab indices). Window-id/name keys and
//! tab tokens are `Shared`/`Ref` on real files, so every entry point inlines
//! the whole tree first (drops all sharing) and edits plain values; the app
//! layer reshares before saving. Mirrors stacks.rs / overview.rs.

use blue_marshal::Value;
use serde::Serialize;

use crate::treewalk::{inline_all, Entries};

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum OverviewTabError {
    /// No `overview` container in the file.
    NoOverview,
    /// No tab with this index in `tabsettings_new`.
    UnknownTab { index: i64 },
    /// No overview window at this position in `tabsByWindowInstanceID`.
    UnknownWindow { index: usize },
    /// Refused: would delete the last remaining tab.
    LastTab,
    /// Refused: would remove the last overview window.
    LastWindow,
}

impl std::fmt::Display for OverviewTabError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OverviewTabError::NoOverview => write!(f, "This file has no overview settings."),
            OverviewTabError::UnknownTab { index } => write!(f, "Tab {index} does not exist."),
            OverviewTabError::UnknownWindow { index } => write!(f, "Overview window {index} does not exist."),
            OverviewTabError::LastTab => write!(f, "An overview must keep at least one tab."),
            OverviewTabError::LastWindow => write!(f, "There must be at least one overview window."),
        }
    }
}

pub(crate) fn is_b(k: &Value, name: &[u8]) -> bool {
    matches!(k, Value::Bytes(b) if b.as_slice() == name)
}

pub(crate) fn as_int(v: &Value) -> Option<i64> {
    match v { Value::Int(i) => Some(*i), _ => None }
}

/// Inner dict of a plain (post-inline) value, unwrapping a `(ts, dict)` tuple.
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

/// Inner list of a plain (post-inline) value, unwrapping a `(ts, list)` tuple.
fn list_inner_mut(v: &mut Value) -> Option<&mut Vec<Value>> {
    match v {
        Value::List(l) => Some(l),
        Value::Tuple(items) => items.iter_mut().find_map(|e| match e {
            Value::List(l) => Some(l),
            _ => None,
        }),
        _ => None,
    }
}

/// Mutable `overview` container dict (tree already inlined).
fn overview_mut(v: &mut Value) -> Result<&mut Entries, OverviewTabError> {
    let Value::Dict(root) = v else { return Err(OverviewTabError::NoOverview) };
    let (_, ov) = root.iter_mut().find(|(k, _)| is_b(k, b"overview")).ok_or(OverviewTabError::NoOverview)?;
    dict_inner_mut(ov).ok_or(OverviewTabError::NoOverview)
}

/// Mutable tab dict under `tabsettings_new`, migrating a legacy `tabsettings`
/// key first (the two are structurally identical; EVE reads `tabsettings_new`).
/// Created empty if neither key exists.
fn tabs_mut(ov: &mut Entries) -> &mut Entries {
    if !ov.iter().any(|(k, _)| is_b(k, b"tabsettings_new")) {
        if let Some((k, _)) = ov.iter_mut().find(|(k, _)| is_b(k, b"tabsettings")) {
            *k = Value::Bytes(b"tabsettings_new".to_vec());
        }
    }
    if !ov.iter().any(|(k, _)| is_b(k, b"tabsettings_new")) {
        ov.push((Value::Bytes(b"tabsettings_new".to_vec()), Value::Dict(Vec::new())));
    }
    let (_, v) = ov.iter_mut().find(|(k, _)| is_b(k, b"tabsettings_new")).unwrap();
    dict_inner_mut(v).expect("tabsettings_new is a dict or (ts,dict)")
}

/// Mutable window-groups list under `tabsByWindowInstanceID`. Created empty if absent.
fn groups_mut(ov: &mut Entries) -> &mut Vec<Value> {
    if !ov.iter().any(|(k, _)| is_b(k, b"tabsByWindowInstanceID")) {
        ov.push((Value::Bytes(b"tabsByWindowInstanceID".to_vec()), Value::List(Vec::new())));
    }
    let (_, v) = ov.iter_mut().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")).unwrap();
    list_inner_mut(v).expect("tabsByWindowInstanceID is a list or (ts,list)")
}

/// Set a tab's name, preserving an existing name entry's value variant (real
/// files store names as Str / StrUcs2 / Bytes), inserting a plain `name` key
/// (unicode-safe `StrUcs2`) if the tab has none. The name KEY may itself be a
/// string-table token (`StrTable(52)`); we match it the same way the reader does.
fn set_name(fields: &mut Entries, name: &str) {
    if let Some((_, val)) = fields.iter_mut().find(|(k, _)| key_is_name(k)) {
        *val = match val {
            Value::Bytes(_) => Value::Bytes(name.as_bytes().to_vec()),
            Value::Str(_) => Value::Str(name.to_string()),
            _ => Value::StrUcs2(name.to_string()),
        };
        return;
    }
    fields.push((Value::Str("name".into()), Value::StrUcs2(name.to_string())));
}

/// True if a dict key is the tab-name key, whether stored as `Str("name")`,
/// `Bytes("name")`, or the string-table token `StrTable(52)` real files use.
fn key_is_name(k: &Value) -> bool {
    match k {
        Value::Str(s) => s == "name",
        Value::Bytes(b) => b.as_slice() == b"name",
        Value::StrTable(52) => true,
        _ => false,
    }
}

pub fn rename_tab(v: &mut Value, tab_idx: i64, name: &str) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    let tabs = tabs_mut(ov);
    let (_, tab) = tabs.iter_mut().find(|(k, _)| as_int(k) == Some(tab_idx))
        .ok_or(OverviewTabError::UnknownTab { index: tab_idx })?;
    let fields = dict_inner_mut(tab).ok_or(OverviewTabError::UnknownTab { index: tab_idx })?;
    set_name(fields, name);
    Ok(())
}
```

Then wire the module in `crates/settings-model/src/lib.rs`: add near the other `mod` lines:

```rust
mod overview_tabs;
```

and to the `pub use` block:

```rust
pub use overview_tabs::{rename_tab, OverviewTabError};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p settings-model overview_tabs`
Expected: PASS (both rename tests).

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/overview_tabs.rs crates/settings-model/src/lib.rs
git commit -m "Add overview_tabs module with rename_tab and traversal helpers"
```

---

### Task 3: `create_tab`

**Files:**
- Modify: `crates/settings-model/src/overview_tabs.rs`
- Modify: `crates/settings-model/src/lib.rs` (export `create_tab`)
- Test: `crates/settings-model/src/overview_tabs.rs`

**Interfaces:**
- Consumes: the Task 2 helpers.
- Produces: `pub fn create_tab(v:&mut Value, window_idx:usize, name:&str, preset:&str) -> Result<i64, OverviewTabError>` — allocates `max(existing index)+1`, writes `{name, overview:preset}` (columns omitted → inherits), appends the index to window `window_idx`, returns the new index.

- [ ] **Step 1: Write the failing test**

Add to the test module:

```rust
    fn window_indices(v: &Value, window: usize) -> Vec<i64> {
        let Value::Dict(root) = v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, g) = ovd.iter().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")).unwrap();
        let Value::List(outer) = g else { panic!() };
        let Value::List(inner) = &outer[window] else { panic!() };
        inner.iter().filter_map(as_int).collect()
    }

    #[test]
    fn create_allocates_next_index_and_joins_the_window() {
        let mut v = user_with_tabs(); // has tab 0 in window 0
        let idx = create_tab(&mut v, 0, "Mining", "P").unwrap();
        assert_eq!(idx, 1, "next free index after 0");
        assert_eq!(tab_name(&v, 1), "Mining");
        assert_eq!(window_indices(&v, 0), vec![0, 1], "appended to window 0's strip");
    }

    #[test]
    fn create_into_missing_window_errors() {
        let mut v = user_with_tabs();
        assert!(matches!(create_tab(&mut v, 5, "X", "P"), Err(OverviewTabError::UnknownWindow { index: 5 })));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p settings-model overview_tabs::tests::create`
Expected: FAIL — `create_tab` not found.

- [ ] **Step 3: Implement `create_tab`**

Add after `rename_tab`:

```rust
pub fn create_tab(v: &mut Value, window_idx: usize, name: &str, preset: &str) -> Result<i64, OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    if window_idx >= groups_mut(ov).len() {
        return Err(OverviewTabError::UnknownWindow { index: window_idx });
    }
    let new_idx = {
        let tabs = tabs_mut(ov);
        let new_idx = tabs.iter().filter_map(|(k, _)| as_int(k)).max().map(|m| m + 1).unwrap_or(0);
        let tab = Value::Dict(vec![
            (Value::Str("name".into()), Value::StrUcs2(name.to_string())),
            (Value::Bytes(b"overview".to_vec()), Value::Bytes(preset.as_bytes().to_vec())),
        ]);
        tabs.push((Value::Int(new_idx), tab));
        new_idx
    };
    if let Some(inner) = groups_mut(ov).get_mut(window_idx).and_then(list_inner_mut) {
        inner.push(Value::Int(new_idx));
    }
    Ok(new_idx)
}
```

Add `create_tab` to the `pub use overview_tabs::{...}` list in `lib.rs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p settings-model overview_tabs`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/overview_tabs.rs crates/settings-model/src/lib.rs
git commit -m "Add create_tab to overview_tabs"
```

---

### Task 4: `delete_tab` (with last-tab guard)

**Files:**
- Modify: `crates/settings-model/src/overview_tabs.rs`, `crates/settings-model/src/lib.rs`
- Test: `crates/settings-model/src/overview_tabs.rs`

**Interfaces:**
- Produces: `pub fn delete_tab(v:&mut Value, tab_idx:i64) -> Result<(), OverviewTabError>` — removes the tab from `tabsettings_new` **and** every window strip; refuses the last remaining tab (`LastTab`); unknown index → `UnknownTab`.

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn delete_removes_tab_and_purges_window_strips() {
        let mut v = user_with_tabs();
        create_tab(&mut v, 0, "Mining", "P").unwrap(); // now tabs 0,1 in window 0
        delete_tab(&mut v, 0).unwrap();
        assert_eq!(window_indices(&v, 0), vec![1], "0 purged from the strip");
        assert!(matches!(rename_tab(&mut v, 0, "X"), Err(OverviewTabError::UnknownTab { index: 0 })),
            "tab 0 is gone from tabsettings_new");
    }

    #[test]
    fn delete_last_tab_is_refused() {
        let mut v = user_with_tabs(); // single tab 0
        assert!(matches!(delete_tab(&mut v, 0), Err(OverviewTabError::LastTab)));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p settings-model overview_tabs::tests::delete`
Expected: FAIL — `delete_tab` not found.

- [ ] **Step 3: Implement `delete_tab`**

```rust
pub fn delete_tab(v: &mut Value, tab_idx: i64) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    {
        let tabs = tabs_mut(ov);
        if !tabs.iter().any(|(k, _)| as_int(k) == Some(tab_idx)) {
            return Err(OverviewTabError::UnknownTab { index: tab_idx });
        }
        if tabs.len() <= 1 {
            return Err(OverviewTabError::LastTab);
        }
        tabs.retain(|(k, _)| as_int(k) != Some(tab_idx));
    }
    for g in groups_mut(ov).iter_mut() {
        if let Some(inner) = list_inner_mut(g) {
            inner.retain(|e| as_int(e) != Some(tab_idx));
        }
    }
    Ok(())
}
```

Add `delete_tab` to the `lib.rs` export list.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p settings-model overview_tabs`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/overview_tabs.rs crates/settings-model/src/lib.rs
git commit -m "Add delete_tab to overview_tabs with a last-tab guard"
```

---

### Task 5: `reorder_tabs_in_window`

**Files:**
- Modify: `crates/settings-model/src/overview_tabs.rs`, `crates/settings-model/src/lib.rs`
- Test: `crates/settings-model/src/overview_tabs.rs`

**Interfaces:**
- Produces: `pub fn reorder_tabs_in_window(v:&mut Value, window_idx:usize, order:&[i64]) -> Result<(), OverviewTabError>` — replaces window `window_idx`'s strip with `order` (the app supplies a permutation of its current tabs); missing window → `UnknownWindow`.

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn reorder_replaces_the_window_strip() {
        let mut v = user_with_tabs();
        create_tab(&mut v, 0, "Mining", "P").unwrap(); // window 0 = [0,1]
        reorder_tabs_in_window(&mut v, 0, &[1, 0]).unwrap();
        assert_eq!(window_indices(&v, 0), vec![1, 0]);
    }

    #[test]
    fn reorder_missing_window_errors() {
        let mut v = user_with_tabs();
        assert!(matches!(reorder_tabs_in_window(&mut v, 3, &[0]), Err(OverviewTabError::UnknownWindow { index: 3 })));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p settings-model overview_tabs::tests::reorder`
Expected: FAIL — not found.

- [ ] **Step 3: Implement `reorder_tabs_in_window`**

```rust
pub fn reorder_tabs_in_window(v: &mut Value, window_idx: usize, order: &[i64]) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    let inner = groups_mut(ov).get_mut(window_idx).and_then(list_inner_mut)
        .ok_or(OverviewTabError::UnknownWindow { index: window_idx })?;
    *inner = order.iter().map(|&i| Value::Int(i)).collect();
    Ok(())
}
```

Add to the `lib.rs` export list.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p settings-model overview_tabs`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/overview_tabs.rs crates/settings-model/src/lib.rs
git commit -m "Add reorder_tabs_in_window to overview_tabs"
```

---

### Task 6: `move_tab` between windows

**Files:**
- Modify: `crates/settings-model/src/overview_tabs.rs`, `crates/settings-model/src/lib.rs`
- Test: `crates/settings-model/src/overview_tabs.rs`

**Interfaces:**
- Produces: `pub fn move_tab(v:&mut Value, tab_idx:i64, from_window:usize, to_window:usize, pos:usize) -> Result<(), OverviewTabError>` — removes `tab_idx` from `from_window`'s strip and inserts it at `pos` (clamped) in `to_window`'s strip; missing window → `UnknownWindow`.

- [ ] **Step 1: Write the failing test**

Add a two-window fixture and tests:

```rust
    fn user_two_windows() -> Value {
        let tab = |p: &str| Value::Dict(vec![
            (Value::Str("name".into()), Value::Str(p.to_string())),
            (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
        ]);
        let overview = Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()),
             Value::Dict(vec![(Value::Int(0), tab("A")), (Value::Int(1), tab("B"))])),
            (Value::Bytes(b"tabsByWindowInstanceID".to_vec()),
             Value::List(vec![
                 Value::List(vec![Value::Int(0)]), // window 0 = [0]
                 Value::List(vec![Value::Int(1)]), // window 1 = [1]
             ])),
        ]);
        Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), overview)])
    }

    #[test]
    fn move_relocates_tab_between_windows() {
        let mut v = user_two_windows();
        move_tab(&mut v, 0, 0, 1, 0).unwrap();
        assert_eq!(window_indices(&v, 0), Vec::<i64>::new(), "removed from source");
        assert_eq!(window_indices(&v, 1), vec![0, 1], "inserted at pos 0 of target");
    }

    #[test]
    fn move_to_missing_window_errors() {
        let mut v = user_two_windows();
        assert!(matches!(move_tab(&mut v, 0, 0, 9, 0), Err(OverviewTabError::UnknownWindow { index: 9 })));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p settings-model overview_tabs::tests::move`
Expected: FAIL — not found.

- [ ] **Step 3: Implement `move_tab`**

```rust
pub fn move_tab(v: &mut Value, tab_idx: i64, from_window: usize, to_window: usize, pos: usize) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    {
        let src = groups_mut(ov).get_mut(from_window).and_then(list_inner_mut)
            .ok_or(OverviewTabError::UnknownWindow { index: from_window })?;
        src.retain(|e| as_int(e) != Some(tab_idx));
    }
    let dst = groups_mut(ov).get_mut(to_window).and_then(list_inner_mut)
        .ok_or(OverviewTabError::UnknownWindow { index: to_window })?;
    let at = pos.min(dst.len());
    dst.insert(at, Value::Int(tab_idx));
    Ok(())
}
```

Add to the `lib.rs` export list.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p settings-model overview_tabs`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/overview_tabs.rs crates/settings-model/src/lib.rs
git commit -m "Add move_tab between overview windows"
```

---

### Task 7: Corpus-idiom realshape test (round-trip + legacy migration)

Guard the edits against real-file idioms: `(timestamp, dict)` wrappers,
`StrTable(52)` name keys, `Shared`/`Ref` tokens, and legacy `tabsettings`.

**Files:**
- Create: `crates/settings-model/tests/overview_tabs_realshape.rs`

**Interfaces:**
- Consumes: `settings_model::{create_tab, rename_tab, delete_tab, project_overview}`, `blue_marshal::{encode, decode, reshare, Value}`.

- [ ] **Step 1: Write the failing test**

```rust
//! Real-idiom guard for overview_tabs: (ts,dict) wrappers, StrTable name keys,
//! Shared/Ref tokens, legacy tabsettings migration. Edits must survive an
//! encode -> decode round-trip after reshare (the save-chain boundary).
use blue_marshal::{decode, encode, reshare, Value};
use settings_model::{create_tab, delete_tab, project_overview, rename_tab};

fn b(s: &[u8]) -> Value { Value::Bytes(s.to_vec()) }
fn ts() -> Value { Value::Long(1_700_000_000) }

/// Legacy overview: `tabsettings` (NOT `_new`), (ts,dict)-wrapped, StrTable name
/// keys, a Shared preset token Ref'd across tabs.
fn legacy_user() -> Value {
    let preset = Value::Shared { slot: 3, value: Box::new(b(b"PvP")) };
    let tab0 = Value::Dict(vec![
        (Value::StrTable(52), Value::Str("Main".into())),
        (b(b"overview"), preset),
    ]);
    let tab1 = Value::Dict(vec![
        (Value::StrTable(52), b(b"Scan")),
        (b(b"overview"), Value::Ref(3)),
    ]);
    let overview = Value::Dict(vec![
        (b(b"tabsettings"),
         Value::Tuple(vec![ts(), Value::Dict(vec![
             (Value::Int(0), tab0), (Value::Int(1), tab1),
         ])])),
        (b(b"tabsByWindowInstanceID"),
         Value::Tuple(vec![ts(), Value::List(vec![
             Value::List(vec![Value::Int(0), Value::Int(1)]),
         ])])),
    ]);
    Value::Dict(vec![(b(b"overview"), overview)])
}

#[test]
fn edits_survive_reshare_roundtrip_and_migrate_legacy() {
    let mut v = legacy_user();

    // Rename tab 0, create tab 2 in window 0, delete tab 1.
    rename_tab(&mut v, 0, "Combat").unwrap();
    let idx = create_tab(&mut v, 0, "Mining", "PvE").unwrap();
    assert_eq!(idx, 2);
    delete_tab(&mut v, 1).unwrap();

    // Reshare (app-layer boundary) then round-trip through the codec.
    v = reshare(&v);
    let bytes = encode(&v).expect("reshared tree encodes");
    let round = decode(&bytes).expect("re-decodes");
    assert_eq!(round, v, "reshared overview_tabs edit round-trips");

    // Project the result: tab 0 renamed, tab 2 present, tab 1 gone, legacy migrated.
    let cols = project_overview(&round, None);
    let names: Vec<_> = cols.tabs.iter().map(|t| (t.index, t.name.clone())).collect();
    assert!(names.contains(&(0, "Combat".to_string())));
    assert!(names.contains(&(2, "Mining".to_string())));
    assert!(!names.iter().any(|(i, _)| *i == 1), "tab 1 deleted");
    // Migration: the container now reads via tabsettings_new (project_overview
    // prefers it), so all three project.
    assert_eq!(cols.tabs.len(), 2);
}
```

- [ ] **Step 2: Run test to verify it fails or passes**

Run: `cargo test -p settings-model --test overview_tabs_realshape`
Expected: PASS if Tasks 2–6 are correct. If it FAILS, the failure is a real
model bug — fix the edit fn, not the test. (Common cause: a helper not
unwrapping the `(ts, dict)`/`(ts, list)` tuple — verify `dict_inner_mut` /
`list_inner_mut` are used on every `tabsettings_new` / `tabsByWindowInstanceID`
access.)

- [ ] **Step 3: Commit**

```bash
git add crates/settings-model/tests/overview_tabs_realshape.rs
git commit -m "Add overview_tabs realshape round-trip and legacy-migration test"
```

---

### Task 8: Backend commands — `tab_*` in `ops.rs` + Tauri registration + api bindings

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (add `edit_user_tabs` + `tab_*` functions)
- Modify: `app/src-tauri/src/lib.rs` (import + `#[tauri::command]` wrappers + `generate_handler!`)
- Modify: `app/src/lib/api.ts` (types + `api.tab*` invoke bindings)
- Test: `app/src-tauri/src/ops.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `settings_model::{create_tab, rename_tab, delete_tab, reorder_tabs_in_window, move_tab, OverviewTabError}`; existing `overview_columns`, `AppState`, `Fidelity`, `ErrDto`.
- Produces (ops.rs, all return the refreshed `OverviewColumns`):
  - `tab_create(state, window_idx: usize, name: String, from_tab: Option<i64>)`
  - `tab_rename(state, tab_idx: i64, name: String)`
  - `tab_delete(state, tab_idx: i64)`
  - `tab_reorder(state, window_idx: usize, order: Vec<i64>)`
  - `tab_move(state, tab_idx: i64, from_window: usize, to_window: usize, pos: usize)`

- [ ] **Step 1: Write the failing test**

Add to the `ops.rs` test module (reuses the existing `temp_file` helper and
open flow — model the setup on the existing overview test that opens a user file;
find it with `grep -n "overview" app/src-tauri/src/ops.rs` and copy its open
boilerplate). Minimal behavior test:

```rust
    #[test]
    fn tab_rename_then_reproject_reflects_the_new_name() {
        // Build a user file with one overview tab, open it into the user slot.
        let user = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()), Value::Dict(vec![(
                Value::Int(0),
                Value::Dict(vec![
                    (Value::Str("name".into()), Value::Str("Main".into())),
                    (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
                ]),
            )])),
            (Value::Bytes(b"tabsByWindowInstanceID".to_vec()),
             Value::List(vec![Value::List(vec![Value::Int(0)])])),
        ]))]);
        let path = temp_file("tabrename", &encode(&user).unwrap());
        let state = AppState::new();
        open_file(&state, Slot::User, path.to_str().unwrap()).unwrap();

        let cols = tab_rename(&state, 0, "Combat".into()).unwrap();
        assert_eq!(cols.tabs[0].name, "Combat");
    }
```

(`open_file(state, slot, path)` and the `temp_file` helper already exist in the
`ops.rs` test module — see the existing `open_editable_file_projects_and_stores_state`
test for the pattern.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p app tab_rename_then_reproject`
Expected: FAIL — `tab_rename` not found.

- [ ] **Step 3: Implement `edit_user_tabs` and the commands**

In `ops.rs`, extend the settings-model import to include the new names, then add:

```rust
/// Edit the user slot's overview tab structure, reshare, then re-project.
fn edit_user_tabs<F>(state: &AppState, edit: F) -> Result<OverviewColumns, ErrDto>
where
    F: FnOnce(&mut blue_marshal::Value) -> Result<(), OverviewTabError>,
{
    {
        let mut guard = state.user.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        edit(&mut doc.value).map_err(|e| {
            let jv = serde_json::to_value(&e).unwrap_or_default();
            ErrDto::new(
                jv.get("code").and_then(|c| c.as_str()).unwrap_or("tab").to_string(),
                e.to_string(),
            )
        })?;
        doc.value = blue_marshal::reshare(&doc.value);
    }
    overview_columns(state)
}

pub fn tab_rename(state: &AppState, tab_idx: i64, name: String) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| rename_tab(v, tab_idx, &name))
}

pub fn tab_delete(state: &AppState, tab_idx: i64) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| delete_tab(v, tab_idx))
}

pub fn tab_reorder(state: &AppState, window_idx: usize, order: Vec<i64>) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| reorder_tabs_in_window(v, window_idx, &order))
}

pub fn tab_move(state: &AppState, tab_idx: i64, from_window: usize, to_window: usize, pos: usize) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| move_tab(v, tab_idx, from_window, to_window, pos))
}

pub fn tab_create(state: &AppState, window_idx: usize, name: String, from_tab: Option<i64>) -> Result<OverviewColumns, ErrDto> {
    // Copy the preset name of the chosen sibling (else the first tab, else a
    // safe default); a new tab must reference a valid preset.
    let preset = {
        let cols = overview_columns(state)?;
        from_tab
            .and_then(|t| cols.tabs.iter().find(|x| x.index == t))
            .or_else(|| cols.tabs.first())
            .map(|t| t.preset.clone())
            .unwrap_or_else(|| "default".to_string())
    };
    edit_user_tabs(state, |v| create_tab(v, window_idx, &name, &preset).map(|_| ()))
}
```

In `app/src-tauri/src/lib.rs`, add `#[tauri::command]` wrappers mirroring the
existing `set_overview_*` commands (find them: `grep -n "set_overview" app/src-tauri/src/lib.rs`) — one per `tab_*` function, each taking `state: State<AppState>` and delegating to `ops::tab_*`, and add all five names to the `tauri::generate_handler![...]` list.

In `app/src/lib/api.ts`, add the `preset: string` field to the `OverviewTab`
type and the invoke bindings (mirror the existing `setOverviewVisible` etc.):

```ts
  tabCreate: (windowIdx: number, name: string, fromTab: number | null) =>
    invoke<OverviewColumns>("tab_create", { windowIdx, name, fromTab }),
  tabRename: (tabIdx: number, name: string) =>
    invoke<OverviewColumns>("tab_rename", { tabIdx, name }),
  tabDelete: (tabIdx: number) =>
    invoke<OverviewColumns>("tab_delete", { tabIdx }),
  tabReorder: (windowIdx: number, order: number[]) =>
    invoke<OverviewColumns>("tab_reorder", { windowIdx, order }),
  tabMove: (tabIdx: number, fromWindow: number, toWindow: number, pos: number) =>
    invoke<OverviewColumns>("tab_move", { tabIdx, fromWindow, toWindow, pos }),
```

(Tauri maps snake_case command args to camelCase JS keys; match the existing
overview bindings' casing convention exactly.)

- [ ] **Step 4: Run tests + typecheck**

Run: `cargo test -p app` — Expected: PASS (new test + existing).
Run (PowerShell, in `app/`): `npm run check` — Expected: 0 errors.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Wire overview tab commands through ops, Tauri, and the api layer"
```

---

### Task 9: Frontend — tab-management controls in `OverviewView.svelte`

Add create / rename / delete / reorder controls around the existing tab selector.
DOM interaction is validated by manual smoke (project norm), not unit tests.

**Files:**
- Modify: `app/src/lib/OverviewView.svelte`

**Interfaces:**
- Consumes: `api.tabCreate/tabRename/tabDelete/tabReorder/tabMove` (Task 8); the existing `data: OverviewColumns`, `tabIndex`, `tab`, `onUserDirty`, `charId`.

- [ ] **Step 1: Add the controls**

Around the existing tab `<select>` block, add (adapt markup/classes to the file's
existing style; reuse its drag-reorder pattern from the column list):

- A **New tab** button → prompts for a name (a small inline input or `window.prompt`
  is acceptable for v1), calls `api.tabCreate(currentWindowIndex, name, tabIndex)`,
  sets `data` to the result, calls `onUserDirty()`. `currentWindowIndex` = the
  index of the window whose strip contains `tabIndex` (derive from `data.windows`).
- A **Rename** affordance on the selected tab → `api.tabRename(tabIndex, name)`.
- A **Delete** button on the selected tab → confirm, then `api.tabDelete(tabIndex)`;
  on success pick a surviving tab for `tabIndex`.
- **Drag-reorder** of tabs within a window → on drop, build the new index order and
  call `api.tabReorder(windowIndex, order)`.
- Show **Move to window** only when `data.windows.length > 1` →
  `api.tabMove(tabIndex, fromWindow, toWindow, pos)`.

Every handler wraps the call in `try/catch` and surfaces failures via the existing
`message(errMessage(e), { title: "Edit failed", kind: "error" })` pattern already
in this file; a `<select>` used for an action resets to its placeholder after the
call (matching the stacks panel). Any new native `<select>`/`<input>` gets explicit
dark background/color (the WebView2 light-control gotcha — see
[[eve-editor-dark-native-controls]]).

- [ ] **Step 2: Typecheck**

Run (PowerShell, in `app/`): `npm run check`
Expected: 0 errors.

- [ ] **Step 3: Manual smoke (record result)**

Run the app (`npm run tauri dev` in `app/`, via PowerShell), open a real account
file, and verify: create a tab, rename it, reorder tabs, delete a tab, and (if the
file has >1 overview window) move a tab between windows. Save, reopen the file, and
confirm the structure persisted and the file is still Editable. If a real client is
available, confirm EVE accepts the saved file and the tabs appear. Note the outcome
in the commit message.

- [ ] **Step 4: Commit**

```bash
git add app/src/lib/OverviewView.svelte
git commit -m "Add overview tab management controls to the Overview view"
```

---

### Task 10: Capture tool for Phase B (overview-window discovery)

Build a throwaway diff tool so the user can capture how EVE represents a **second
overview window** — the one unknown blocking window add/remove (spec §6). This
is scratch tooling (built, used, then deleted); it ships nothing.

**Files:**
- Create: `crates/settings-model/src/bin/overviewdiff.rs`

**Interfaces:**
- Consumes: `blue_marshal::{decode, inline, dump_text, Value}`.

- [ ] **Step 1: Write the tool**

```rust
//! Overview-window capture diff — Phase B discovery for overview tab management
//! (docs/superpowers/specs/2026-07-19-overview-tab-management-design.md §6).
//!
//! Decodes two `core_char_*.dat` snapshots (before / after creating a SECOND
//! overview window in-game) and prints, sorted, both:
//!   - the `windows` dict entry keys + their geometry (to reveal the new
//!     window's key/name scheme and required fields), and
//!   - the overview `tabsByWindowInstanceID` list (to reveal how a new window
//!     position appears and links to the windows-dict entry).
//! The tree is inlined first so ids read as real values, not `ref[N]`.
//!
//! Usage: cargo run -p settings-model --bin overviewdiff -- <before.dat> <after.dat>
use std::fs;
use std::process::ExitCode;

use blue_marshal::{decode, dump_text, inline, Value};

fn is_b(k: &Value, name: &[u8]) -> bool { matches!(k, Value::Bytes(b) if b.as_slice() == name) }

fn subtree_text(path: &str, keys: &[&[u8]]) -> String {
    let bytes = fs::read(path).expect("read");
    let mut v = decode(&bytes).expect("decode");
    inline(&mut v);
    let mut cur = &v;
    for &k in keys {
        let Value::Dict(d) = cur else { return format!("<no dict at {path}>") };
        match d.iter().find(|(kk, _)| is_b(kk, k)) {
            Some((_, val)) => cur = val,
            None => return format!("<missing key {} in {path}>", String::from_utf8_lossy(k)),
        }
    }
    dump_text(cur) // dump_text sorts dict keys for a clean line-diff
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [before, after] = args.as_slice() else {
        eprintln!("usage: overviewdiff <before.dat> <after.dat>");
        return ExitCode::FAILURE;
    };
    for (label, path) in [("BEFORE", before), ("AFTER", after)] {
        println!("===== {label} {path} — windows =====");
        println!("{}", subtree_text(path, &[b"windows"]));
        println!("===== {label} {path} — overview.tabsByWindowInstanceID =====");
        println!("{}", subtree_text(path, &[b"overview", b"tabsByWindowInstanceID"]));
    }
    ExitCode::SUCCESS
}
```

(If `inline` / `dump_text` are not public from `blue_marshal`, check the crate's
`lib.rs` exports — the deleted `windowsdump.rs` used exactly these; mirror its
imports.)

- [ ] **Step 2: Verify it builds**

Run: `cargo build -p settings-model --bin overviewdiff`
Expected: builds clean.

- [ ] **Step 3: Do NOT commit — hand off to the user**

This bin is untracked scratch tooling (like the deleted `stackdiff`/`windowsdump`).
Leave it uncommitted. Ask the user to: snapshot a `core_char_*.dat`, create a
second overview window in-game (tab an overview window out / open a new one), snapshot
again, and run `cargo run -p settings-model --bin overviewdiff -- <before> <after>`.
Record the findings (the new window's key scheme + how `tabsByWindowInstanceID`
changed + linkage) into spec §6, then write the **Phase B addendum** to this plan
(`add_overview_window` / `remove_overview_window` tasks) with the real recipe.

---

## Phase B (deferred until the capture)

`add_overview_window` / `remove_overview_window` are intentionally **not** planned
here: their byte recipe (the new window's `windows`-dict key/name and how a
`tabsByWindowInstanceID` position links to it) is the one thing the static corpus
cannot tell us, and writing speculative TDD steps for an unknown shape would be a
placeholder. Task 10 produces that recipe; the Phase B addendum is written against
it. Phase A (Tasks 1–9) is complete and shippable on its own — full tab management
within existing overview windows.

## Verification (end of Phase A)

- [ ] `cargo test --workspace` — all green.
- [ ] `npm run check` in `app/` — 0 errors.
- [ ] Manual smoke recorded (Task 9 Step 3).
