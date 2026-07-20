# Overview filter presets — slice 2a Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the overview editor read the account's filter-preset list, assign an existing preset to each tab, and duplicate / rename / delete presets — all user-file only.

**Architecture:** Mirror the shipped overview-tabs structural-edit idiom. Pure edit functions in the settings-model crate take `&mut blue_marshal::Value`, inline the tree (drop `Shared`/`Ref` sharing), edit plain values, and return `Result<(), OverviewTabError>`; the Tauri `ops.rs` layer wraps each through the existing `edit_user_tabs` helper (inline → edit → `reshare` → re-project → return `OverviewColumns`). The frontend calls thin `api.ts` bindings and re-renders from the returned projection.

**Tech Stack:** Rust (settings-model crate, `blue_marshal` codec), Tauri commands (`app/src-tauri`), SvelteKit + TypeScript frontend (`app/src`).

## Global Constraints

- **Sentence-case git commit subjects, NO attribution trailers** (no `Co-Authored-By`, no `Generated with`). Match the repo's existing history.
- **No personal data in code, tests, or docs** — use synthetic preset names (`"alpha"`, `"Zeta"`) and ids only.
- **settings-model stays dependency-free** — no new crates; reuse `blue_marshal` and existing `crate::treewalk` / `overview_tabs` helpers.
- **Dark native controls:** every new `<select>`/`<input>` needs explicit dark bg/color (WebView2 renders them light otherwise) — the existing `OverviewView.svelte` `<style>` block already covers `select, option, optgroup, input`; new selects inherit it, new button clusters do not need it.
- **Preset value blob is opaque in 2a:** duplicate copies a preset's `{groups, filteredStates, alwaysShownStates}` dict wholesale; never inspect or build its contents (that is slice 2b).
- **Rust tests:** `cargo test -p settings-model` from the repo root. **Frontend checks:** run from `app/` via PowerShell (npm is not on the Bash PATH): `npm run check` (svelte-check) and `npm run build`.
- **Commit after every task** (each task ends green).

Spec: `docs/superpowers/specs/2026-07-20-overview-filter-presets-2a-design.md`.

---

### Task 1: Projection — expose the preset name list

Add `presets: Vec<String>` (sorted, case-insensitive) to the `OverviewColumns` projection, read from the user file's `overview → overviewProfilePresets` keys, and mirror the field into the TypeScript interface.

**Files:**
- Modify: `crates/settings-model/src/overview.rs` (struct at `:25-29`, constructions at `:59` and `:66`, add reader + test)
- Modify: `app/src/lib/api.ts` (`OverviewColumns` interface at `:189-192`)

**Interfaces:**
- Produces: `OverviewColumns.presets: Vec<String>` — the account's preset names, sorted case-insensitively. Empty when the container/key is absent.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `crates/settings-model/src/overview.rs` (any existing test module in the file):

```rust
#[test]
fn project_exposes_sorted_preset_names() {
    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
    // overview -> overviewProfilePresets -> (ts, { "Zeta": {}, "alpha": {} })
    let presets = Value::Dict(vec![
        (b("Zeta"), Value::Dict(vec![])),
        (b("alpha"), Value::Dict(vec![])),
    ]);
    let overview = Value::Dict(vec![
        (b("overviewProfilePresets"), Value::Tuple(vec![Value::Int(1), presets])),
    ]);
    let user = Value::Dict(vec![(b("overview"), overview)]);
    let cols = project_overview(&user, None);
    assert_eq!(cols.presets, vec!["alpha".to_string(), "Zeta".to_string()]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p settings-model project_exposes_sorted_preset_names`
Expected: compile error — `OverviewColumns` has no field `presets` (and the two literal constructions are missing it).

- [ ] **Step 3: Add the field, the reader, and update both constructions**

In `crates/settings-model/src/overview.rs`, add the field to the struct (`:25-29`):

```rust
#[derive(Debug, Serialize, PartialEq)]
pub struct OverviewColumns {
    pub windows: Vec<OverviewWindow>,
    pub tabs: Vec<OverviewTab>,
    pub presets: Vec<String>,
}
```

Update the two constructions in `project_overview`:

```rust
    let empty = OverviewColumns { windows: vec![], tabs: vec![], presets: vec![] };
    let Some(overview) = overview_container(user, &sh) else { return empty };

    let windows = window_groups(overview, &sh);
    let tabs = tab_dict(overview, &sh)
        .map(|d| d.iter().filter_map(|(k, v)| project_tab(k, v, overview, char_tree, &sh)).collect())
        .unwrap_or_default();
    let presets = preset_names(overview, &sh);
    OverviewColumns { windows, tabs, presets }
```

Add the reader near the other `overview.rs` helpers (e.g. below `default_columns`):

```rust
/// Sorted (case-insensitive) preset names from `overviewProfilePresets`
/// (a `(timestamp, dict)` keyed by preset name). Empty when the key is absent.
fn preset_names(overview: &Entries, sh: &SharedTable) -> Vec<String> {
    let Some(dict) = find_child(overview, b"overviewProfilePresets", sh).and_then(|v| as_dict(v, sh))
    else { return vec![] };
    let mut names: Vec<String> = dict.iter().filter_map(|(k, _)| preset_key_name(effective(k, sh))).collect();
    names.sort_by_key(|s| s.to_lowercase());
    names
}

/// A preset dict key as a string (Bytes on real files; Str/StrUcs2 defensively).
fn preset_key_name(k: &Value) -> Option<String> {
    match k {
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        Value::Str(s) | Value::StrUcs2(s) => Some(s.clone()),
        _ => None,
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p settings-model project_exposes_sorted_preset_names`
Expected: PASS.

- [ ] **Step 5: Mirror the field in the TypeScript interface**

In `app/src/lib/api.ts`, update `OverviewColumns` (`:189-192`):

```ts
export interface OverviewColumns {
  tabs: OverviewTab[];
  windows: OverviewWindow[];
  presets: string[];
}
```

- [ ] **Step 6: Verify the whole crate still builds and the frontend type-checks**

Run: `cargo test -p settings-model` (Expected: all pass)
Run from `app/` (PowerShell): `npm run check` (Expected: no svelte-check errors)

- [ ] **Step 7: Commit**

```bash
git add crates/settings-model/src/overview.rs app/src/lib/api.ts
git commit -m "Project the overview preset name list"
```

---

### Task 2: Preset authoring module + duplicate

Create `overview_presets.rs`, extend `OverviewTabError` with the preset variants, make the reused `overview_tabs` helpers `pub(crate)`, implement `create_preset` (duplicate), and wire the `preset_create` command end to end.

**Files:**
- Create: `crates/settings-model/src/overview_presets.rs`
- Modify: `crates/settings-model/src/overview_tabs.rs` (error enum `:15-44`, helper visibility at `:55`, `:91`, `:100`)
- Modify: `crates/settings-model/src/lib.rs` (`:19` module decl, `:33-36` re-exports)
- Modify: `app/src-tauri/src/ops.rs` (import `:21-23`, add wrapper)
- Modify: `app/src-tauri/src/lib.rs` (command + `:260` handler list)
- Modify: `app/src/lib/api.ts` (binding)

**Interfaces:**
- Consumes: `OverviewTabError`, `is_b`, `as_int`, `overview_mut`, `dict_inner_mut`, `tabs_mut` from `overview_tabs`; `inline_all`, `Entries` from `crate::treewalk`.
- Produces:
  - `overview_presets::create_preset(v: &mut Value, from: &str, new_name: &str) -> Result<(), OverviewTabError>`
  - `overview_presets::as_str(v: &Value) -> Option<String>` (`pub(crate)`, reused by later tasks)
  - `overview_presets::presets_mut(ov: &mut Entries) -> Option<&mut Entries>` (`pub(crate)`)
  - `overview_presets::not_saved_mut(ov: &mut Entries) -> Option<&mut Entries>` (`pub(crate)`)
  - `overview_presets::sorted_names(presets: &Entries) -> Vec<String>` (`pub(crate)`)
  - `overview_presets::retarget_tabs(tabs: &mut Entries, old: &str, new: &str)` (`pub(crate)`)
  - `OverviewTabError::UnknownPreset { name }`, `PresetExists { name }`, `LastPreset`
  - ops `preset_create(state, from: String, new_name: String) -> Result<OverviewColumns, ErrDto>`
  - api `presetCreate(from, newName)`

- [ ] **Step 1: Extend the error enum**

In `crates/settings-model/src/overview_tabs.rs`, add three variants to `OverviewTabError` (after `NotLastWindow`, `:29`):

```rust
    /// No preset with this name in `overviewProfilePresets`.
    UnknownPreset { name: String },
    /// A preset with the target name already exists.
    PresetExists { name: String },
    /// Refused: would delete the last remaining preset.
    LastPreset,
```

Add their `Display` arms in the `match` (after the `NotLastWindow` arm, `:41`):

```rust
            OverviewTabError::UnknownPreset { name } => write!(f, "Preset \"{name}\" does not exist."),
            OverviewTabError::PresetExists { name } => write!(f, "A preset named \"{name}\" already exists."),
            OverviewTabError::LastPreset => write!(f, "An overview must keep at least one preset."),
```

- [ ] **Step 2: Make the reused helpers `pub(crate)`**

In `crates/settings-model/src/overview_tabs.rs`, change these three signatures (leave bodies unchanged):

```rust
pub(crate) fn dict_inner_mut(v: &mut Value) -> Option<&mut Entries> {   // was: fn  (:55)
pub(crate) fn overview_mut(v: &mut Value) -> Result<&mut Entries, OverviewTabError> {   // was: fn  (:91)
pub(crate) fn tabs_mut(ov: &mut Entries) -> &mut Entries {   // was: fn  (:100)
```

- [ ] **Step 3: Create the module with `create_preset` and its unit tests**

Create `crates/settings-model/src/overview_presets.rs`:

```rust
//! Structural authoring for overview *filter presets*: the named filter
//! definitions in the user file's `overview` container under
//! `overviewProfilePresets` (a `(timestamp, dict)` keyed by preset name; each
//! value is an opaque `{groups, filteredStates, alwaysShownStates}` blob 2a
//! copies wholesale but never inspects). A tab points at a preset by name in its
//! `overview` field. Edits use the same inline-first idiom as `overview_tabs.rs`
//! and reuse its `pub(crate)` helpers; the app layer reshares before saving.
//!
//! `overviewProfilePresets_notSaved` is a parallel, name-keyed buffer holding
//! EVE's unsaved working copy of a preset. It is populated on most real files, so
//! rename/delete mirror into it to avoid stranding a stale entry that could
//! resurrect a phantom preset on next login.

use blue_marshal::Value;

use crate::overview_tabs::{dict_inner_mut, is_b, overview_mut, tabs_mut, OverviewTabError};
use crate::treewalk::{inline_all, Entries};

/// String form of a preset dict key or a tab's `overview` value (Bytes on real
/// files; Str/StrUcs2 defensively). Used for name comparison after inlining.
pub(crate) fn as_str(v: &Value) -> Option<String> {
    match v {
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        Value::Str(s) | Value::StrUcs2(s) => Some(s.clone()),
        _ => None,
    }
}

/// Mutable inner dict of `overviewProfilePresets` (unwrapping `(ts, dict)`).
/// None when the container is absent.
pub(crate) fn presets_mut(ov: &mut Entries) -> Option<&mut Entries> {
    let (_, v) = ov.iter_mut().find(|(k, _)| is_b(k, b"overviewProfilePresets"))?;
    dict_inner_mut(v)
}

/// Mutable inner dict of `overviewProfilePresets_notSaved`, if present (it may be
/// absent or empty — callers do nothing then).
pub(crate) fn not_saved_mut(ov: &mut Entries) -> Option<&mut Entries> {
    let (_, v) = ov.iter_mut().find(|(k, _)| is_b(k, b"overviewProfilePresets_notSaved"))?;
    dict_inner_mut(v)
}

/// Preset names sorted case-insensitively — the SAME order the projection shows,
/// so the delete-neighbour the UI names matches the one the model reassigns to.
pub(crate) fn sorted_names(presets: &Entries) -> Vec<String> {
    let mut names: Vec<String> = presets.iter().filter_map(|(k, _)| as_str(k)).collect();
    names.sort_by_key(|s| s.to_lowercase());
    names
}

/// Repoint every tab whose `overview` field equals `old` to `new` (Bytes value,
/// matching real files). No-op for tabs pointing elsewhere.
pub(crate) fn retarget_tabs(tabs: &mut Entries, old: &str, new: &str) {
    for (_, tab) in tabs.iter_mut() {
        if let Some(fields) = dict_inner_mut(tab) {
            if let Some((_, val)) = fields.iter_mut().find(|(k, _)| is_b(k, b"overview")) {
                if as_str(val).as_deref() == Some(old) {
                    *val = Value::Bytes(new.as_bytes().to_vec());
                }
            }
        }
    }
}

/// Duplicate the `from` preset's whole value blob under a new key `new_name`.
/// Cloning keeps the required `{groups, filteredStates, alwaysShownStates}` shape
/// correct by construction (2a never inspects it).
pub fn create_preset(v: &mut Value, from: &str, new_name: &str) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    let presets = presets_mut(ov).ok_or(OverviewTabError::UnknownPreset { name: from.to_string() })?;
    if presets.iter().any(|(k, _)| as_str(k).as_deref() == Some(new_name)) {
        return Err(OverviewTabError::PresetExists { name: new_name.to_string() });
    }
    let blob = presets
        .iter()
        .find(|(k, _)| as_str(k).as_deref() == Some(from))
        .map(|(_, val)| val.clone())
        .ok_or(OverviewTabError::UnknownPreset { name: from.to_string() })?;
    presets.push((Value::Bytes(new_name.as_bytes().to_vec()), blob));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }

    /// user -> overview -> {
    ///   tabsettings_new: { 0: {overview:"alpha"}, 1: {overview:"beta"} },
    ///   overviewProfilePresets: (ts, { "alpha": {groups:[1]}, "beta": {groups:[2]} }),
    ///   overviewProfilePresets_notSaved: (ts, { "alpha": {groups:[9]} }),
    /// }
    fn user_with_presets() -> Value {
        let tab0 = Value::Dict(vec![(b("overview"), b("alpha"))]);
        let tab1 = Value::Dict(vec![(b("overview"), b("beta"))]);
        let preset = |g: i64| Value::Dict(vec![(b("groups"), Value::List(vec![Value::Int(g)]))]);
        let overview = Value::Dict(vec![
            (b("tabsettings_new"), Value::Dict(vec![
                (Value::Int(0), tab0), (Value::Int(1), tab1),
            ])),
            (b("overviewProfilePresets"), Value::Tuple(vec![
                Value::Int(1),
                Value::Dict(vec![(b("alpha"), preset(1)), (b("beta"), preset(2))]),
            ])),
            (b("overviewProfilePresets_notSaved"), Value::Tuple(vec![
                Value::Int(1),
                Value::Dict(vec![(b("alpha"), preset(9))]),
            ])),
        ]);
        Value::Dict(vec![(b("overview"), overview)])
    }

    fn preset_names(v: &Value) -> Vec<String> {
        let Value::Dict(root) = v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, p) = ovd.iter().find(|(k, _)| is_b(k, b"overviewProfilePresets")).unwrap();
        let Value::Tuple(items) = p else { panic!() };
        let Value::Dict(pd) = &items[1] else { panic!() };
        pd.iter().filter_map(|(k, _)| as_str(k)).collect()
    }

    #[test]
    fn duplicate_clones_the_blob_under_the_new_key() {
        let mut v = user_with_presets();
        create_preset(&mut v, "alpha", "gamma").unwrap();
        let names = preset_names(&v);
        assert!(names.contains(&"gamma".to_string()));
        // The clone carries alpha's blob: groups == [1].
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, p) = ovd.iter().find(|(k, _)| is_b(k, b"overviewProfilePresets")).unwrap();
        let Value::Tuple(items) = p else { panic!() };
        let Value::Dict(pd) = &items[1] else { panic!() };
        let (_, gamma) = pd.iter().find(|(k, _)| as_str(k).as_deref() == Some("gamma")).unwrap();
        let Value::Dict(gf) = gamma else { panic!() };
        let (_, groups) = gf.iter().find(|(k, _)| is_b(k, b"groups")).unwrap();
        assert_eq!(groups, &Value::List(vec![Value::Int(1)]));
    }

    #[test]
    fn duplicate_unknown_source_errors() {
        let mut v = user_with_presets();
        assert!(matches!(
            create_preset(&mut v, "nope", "gamma"),
            Err(OverviewTabError::UnknownPreset { .. })
        ));
    }

    #[test]
    fn duplicate_existing_target_errors() {
        let mut v = user_with_presets();
        assert!(matches!(
            create_preset(&mut v, "alpha", "beta"),
            Err(OverviewTabError::PresetExists { .. })
        ));
    }
}
```

- [ ] **Step 4: Register the module and re-export `create_preset`**

In `crates/settings-model/src/lib.rs`, add the module (near `:19`):

```rust
mod overview_presets;
```

Add a preset re-export line (only `create_preset` exists in this task; Tasks 4 and 5 widen this line as they add `rename_preset` / `delete_preset`):

```rust
pub use overview_presets::create_preset;
```

Leave the existing `overview_tabs` re-export (`:33-36`) untouched — `set_tab_preset` is added to it in Task 3.

- [ ] **Step 5: Run the crate tests**

Run: `cargo test -p settings-model`
Expected: PASS (the three new `overview_presets` tests plus everything prior).

- [ ] **Step 6: Wire the `preset_create` op, command, and binding**

In `app/src-tauri/src/ops.rs`, add `create_preset` to the `settings_model` import (`:21-23`, the overview line). Import ONLY `create_preset` now — `set_tab_preset`/`rename_preset`/`delete_preset` aren't exported from settings-model until Tasks 3–5, so importing them here now would fail to compile. Each later task adds its own name to this import.

```rust
    create_preset,
```

Add the wrapper next to `tab_create` (`:745`):

```rust
pub fn preset_create(state: &AppState, from: String, new_name: String) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| create_preset(v, &from, &new_name))
}
```

In `app/src-tauri/src/lib.rs`, add the command (next to `tab_create`, `:159`):

```rust
#[tauri::command]
fn preset_create(state: tauri::State<'_, AppState>, from: String, new_name: String) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::preset_create(&state, from, new_name)
}
```

Add `preset_create` to the `generate_handler!` list (`:266-268` area):

```rust
            tab_create, tab_rename, tab_delete, tab_reorder, tab_move,
            overview_window_add, overview_window_remove,
            preset_create,
```

In `app/src/lib/api.ts`, add the binding (after `overviewWindowRemove`, `:278`):

```ts
  presetCreate: (from: string, newName: string) =>
    invoke<OverviewColumns>("preset_create", { from, newName }),
```

- [ ] **Step 7: Build the app crate and type-check the frontend**

Run: `cargo build -p app` (Expected: builds clean)
Run from `app/` (PowerShell): `npm run check` (Expected: no errors)

- [ ] **Step 8: Commit**

```bash
git add crates/settings-model/src/overview_presets.rs crates/settings-model/src/overview_tabs.rs crates/settings-model/src/lib.rs app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Add filter-preset duplicate authoring and wiring"
```

---

### Task 3: Assign a preset to a tab (`set_tab_preset`)

Set a tab's `overview` field to a chosen preset name, and wire the `tab_set_preset` command.

**Files:**
- Modify: `crates/settings-model/src/overview_tabs.rs` (new fn + tests)
- Modify: `crates/settings-model/src/lib.rs` (`:33-36` re-export)
- Modify: `app/src-tauri/src/ops.rs`, `app/src-tauri/src/lib.rs`, `app/src/lib/api.ts`

**Interfaces:**
- Produces:
  - `overview_tabs::set_tab_preset(v: &mut Value, tab_idx: i64, preset: &str) -> Result<(), OverviewTabError>`
  - ops `tab_set_preset(state, tab_idx: i64, preset: String) -> Result<OverviewColumns, ErrDto>`
  - api `tabSetPreset(tabIdx, preset)`

- [ ] **Step 1: Write the failing tests**

Add to `crates/settings-model/src/overview_tabs.rs` `mod tests` (reuse the file's existing `user_with_tabs()`, whose tab 0 has `overview: "P"`):

```rust
    fn tab_preset(v: &Value, idx: i64) -> String {
        let Value::Dict(root) = v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, tabs) = ovd.iter().find(|(k, _)| is_b(k, b"tabsettings_new")).unwrap();
        let Value::Dict(td) = tabs else { panic!() };
        let (_, tab) = td.iter().find(|(k, _)| as_int(k) == Some(idx)).unwrap();
        let Value::Dict(fields) = tab else { panic!() };
        let (_, val) = fields.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        match val { Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(), _ => panic!() }
    }

    #[test]
    fn set_tab_preset_changes_the_field() {
        let mut v = user_with_tabs();
        set_tab_preset(&mut v, 0, "combat").unwrap();
        assert_eq!(tab_preset(&v, 0), "combat");
    }

    #[test]
    fn set_tab_preset_unknown_tab_errors() {
        let mut v = user_with_tabs();
        assert!(matches!(
            set_tab_preset(&mut v, 9, "combat"),
            Err(OverviewTabError::UnknownTab { index: 9 })
        ));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p settings-model set_tab_preset`
Expected: compile error — `set_tab_preset` not found.

- [ ] **Step 3: Implement `set_tab_preset`**

Add to `crates/settings-model/src/overview_tabs.rs` after `rename_tab` (`:168`):

```rust
/// Point a tab at a filter preset by name (its `overview` field). Stores the
/// name as `Bytes`, matching real files; inserts the key if the tab lacks it.
pub fn set_tab_preset(v: &mut Value, tab_idx: i64, preset: &str) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    let tabs = tabs_mut(ov);
    let (_, tab) = tabs.iter_mut().find(|(k, _)| as_int(k) == Some(tab_idx))
        .ok_or(OverviewTabError::UnknownTab { index: tab_idx })?;
    let fields = dict_inner_mut(tab).ok_or(OverviewTabError::UnknownTab { index: tab_idx })?;
    if let Some((_, val)) = fields.iter_mut().find(|(k, _)| is_b(k, b"overview")) {
        *val = Value::Bytes(preset.as_bytes().to_vec());
    } else {
        fields.push((Value::Bytes(b"overview".to_vec()), Value::Bytes(preset.as_bytes().to_vec())));
    }
    Ok(())
}
```

Add `set_tab_preset` to the overview_tabs re-export in `crates/settings-model/src/lib.rs` (`:33-36`):

```rust
pub use overview_tabs::{
    add_overview_window, add_overview_window_geometry, create_tab, delete_tab, move_tab,
    remove_overview_window, remove_overview_window_geometry, rename_tab, reorder_tabs_in_window,
    set_tab_preset, OverviewTabError,
};
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p settings-model set_tab_preset`
Expected: PASS.

- [ ] **Step 5: Wire op, command, binding**

`app/src-tauri/src/ops.rs` — add `set_tab_preset` to the `settings_model` import (`:21-23`), then add the wrapper next to `preset_create`:

```rust
pub fn tab_set_preset(state: &AppState, tab_idx: i64, preset: String) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| set_tab_preset(v, tab_idx, &preset))
}
```

`app/src-tauri/src/lib.rs` — add the command and handler entry:

```rust
#[tauri::command]
fn tab_set_preset(state: tauri::State<'_, AppState>, tab_idx: i64, preset: String) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::tab_set_preset(&state, tab_idx, preset)
}
```

Add `tab_set_preset` to the `generate_handler!` list (with `preset_create`).

`app/src/lib/api.ts` — add:

```ts
  tabSetPreset: (tabIdx: number, preset: string) =>
    invoke<OverviewColumns>("tab_set_preset", { tabIdx, preset }),
```

- [ ] **Step 6: Build + check**

Run: `cargo build -p app`
Run from `app/`: `npm run check`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add crates/settings-model/src/overview_tabs.rs crates/settings-model/src/lib.rs app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Add set-tab-preset authoring and wiring"
```

---

### Task 4: Rename a preset (retarget tabs + mirror `_notSaved`)

Rename the `overviewProfilePresets` key, repoint referencing tabs, and rename the matching `_notSaved` buffer entry.

**Files:**
- Modify: `crates/settings-model/src/overview_presets.rs` (new fn + tests)
- Modify: `crates/settings-model/src/lib.rs` (widen preset re-export)
- Modify: `app/src-tauri/src/ops.rs`, `app/src-tauri/src/lib.rs`, `app/src/lib/api.ts`

**Interfaces:**
- Consumes: `presets_mut`, `not_saved_mut`, `retarget_tabs`, `as_str`, `overview_mut`, `tabs_mut`.
- Produces:
  - `overview_presets::rename_preset(v: &mut Value, old: &str, new: &str) -> Result<(), OverviewTabError>`
  - ops `preset_rename(state, old_name: String, new_name: String) -> Result<OverviewColumns, ErrDto>`
  - api `presetRename(oldName, newName)`

- [ ] **Step 1: Write the failing tests**

Add to `overview_presets.rs` `mod tests`:

```rust
    fn tab_preset(v: &Value, idx: i64) -> String {
        let Value::Dict(root) = v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, tabs) = ovd.iter().find(|(k, _)| is_b(k, b"tabsettings_new")).unwrap();
        let Value::Dict(td) = tabs else { panic!() };
        let (_, tab) = td.iter().find(|(k, _)| matches!(k, Value::Int(i) if *i == idx)).unwrap();
        let Value::Dict(fields) = tab else { panic!() };
        let (_, val) = fields.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        as_str(val).unwrap()
    }

    fn not_saved_names(v: &Value) -> Vec<String> {
        let Value::Dict(root) = v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, p) = ovd.iter().find(|(k, _)| is_b(k, b"overviewProfilePresets_notSaved")).unwrap();
        let Value::Tuple(items) = p else { panic!() };
        let Value::Dict(pd) = &items[1] else { panic!() };
        pd.iter().filter_map(|(k, _)| as_str(k)).collect()
    }

    #[test]
    fn rename_renames_key_retargets_tabs_and_mirrors_notsaved() {
        let mut v = user_with_presets();
        rename_preset(&mut v, "alpha", "alpha2").unwrap();
        let names = preset_names(&v);
        assert!(names.contains(&"alpha2".to_string()) && !names.contains(&"alpha".to_string()));
        assert_eq!(tab_preset(&v, 0), "alpha2", "tab 0 followed the rename");
        assert_eq!(tab_preset(&v, 1), "beta", "tab 1 unaffected");
        assert!(not_saved_names(&v).contains(&"alpha2".to_string()), "notSaved buffer followed");
    }

    #[test]
    fn rename_unknown_source_errors() {
        let mut v = user_with_presets();
        assert!(matches!(rename_preset(&mut v, "nope", "x"), Err(OverviewTabError::UnknownPreset { .. })));
    }

    #[test]
    fn rename_to_existing_name_errors() {
        let mut v = user_with_presets();
        assert!(matches!(rename_preset(&mut v, "alpha", "beta"), Err(OverviewTabError::PresetExists { .. })));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p settings-model rename_`
Expected: compile error — `rename_preset` not found.

- [ ] **Step 3: Implement `rename_preset`**

Add to `overview_presets.rs` (after `create_preset`):

```rust
/// Rename a preset: the `overviewProfilePresets` key, every tab that references
/// it, and any matching `overviewProfilePresets_notSaved` buffer entry.
pub fn rename_preset(v: &mut Value, old: &str, new: &str) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    {
        let presets = presets_mut(ov).ok_or(OverviewTabError::UnknownPreset { name: old.to_string() })?;
        if old != new && presets.iter().any(|(k, _)| as_str(k).as_deref() == Some(new)) {
            return Err(OverviewTabError::PresetExists { name: new.to_string() });
        }
        let entry = presets.iter_mut().find(|(k, _)| as_str(k).as_deref() == Some(old))
            .ok_or(OverviewTabError::UnknownPreset { name: old.to_string() })?;
        entry.0 = Value::Bytes(new.as_bytes().to_vec());
    }
    if old == new {
        return Ok(());
    }
    if let Some(ns) = not_saved_mut(ov) {
        if let Some(entry) = ns.iter_mut().find(|(k, _)| as_str(k).as_deref() == Some(old)) {
            entry.0 = Value::Bytes(new.as_bytes().to_vec());
        }
    }
    retarget_tabs(tabs_mut(ov), old, new);
    Ok(())
}
```

Widen the preset re-export in `crates/settings-model/src/lib.rs`:

```rust
pub use overview_presets::{create_preset, rename_preset};
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p settings-model rename_`
Expected: PASS.

- [ ] **Step 5: Wire op, command, binding**

`app/src-tauri/src/ops.rs` — add `rename_preset` to the `settings_model` import, then add the wrapper:

```rust
pub fn preset_rename(state: &AppState, old_name: String, new_name: String) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| rename_preset(v, &old_name, &new_name))
}
```

`app/src-tauri/src/lib.rs` (command + handler entry):

```rust
#[tauri::command]
fn preset_rename(state: tauri::State<'_, AppState>, old_name: String, new_name: String) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::preset_rename(&state, old_name, new_name)
}
```

`app/src/lib/api.ts`:

```ts
  presetRename: (oldName: string, newName: string) =>
    invoke<OverviewColumns>("preset_rename", { oldName, newName }),
```

- [ ] **Step 6: Build + check**

Run: `cargo build -p app`
Run from `app/`: `npm run check`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add crates/settings-model/src/overview_presets.rs crates/settings-model/src/lib.rs app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Add preset rename with tab retarget and notSaved mirror"
```

---

### Task 5: Delete a preset (reassign to the preceding preset)

Remove a preset; reassign every tab using it to the immediately-preceding preset in sorted order (successor when deleting the first); refuse the last preset; remove any matching `_notSaved` entry.

**Files:**
- Modify: `crates/settings-model/src/overview_presets.rs` (new fn + tests)
- Modify: `crates/settings-model/src/lib.rs` (widen preset re-export)
- Modify: `app/src-tauri/src/ops.rs`, `app/src-tauri/src/lib.rs`, `app/src/lib/api.ts`

**Interfaces:**
- Consumes: `presets_mut`, `not_saved_mut`, `retarget_tabs`, `sorted_names`, `as_str`, `overview_mut`, `tabs_mut`.
- Produces:
  - `overview_presets::delete_preset(v: &mut Value, name: &str) -> Result<(), OverviewTabError>`
  - ops `preset_delete(state, name: String) -> Result<OverviewColumns, ErrDto>`
  - api `presetDelete(name)`

- [ ] **Step 1: Write the failing tests**

Add to `overview_presets.rs` `mod tests` (`user_with_presets` sorts as `alpha`, `beta`; tab 0→alpha, tab 1→beta):

```rust
    #[test]
    fn delete_reassigns_tabs_to_preceding_preset_and_pops_notsaved() {
        // Delete "beta" (pos 1) -> preceding is "alpha"; tab 1 moves to "alpha".
        let mut v = user_with_presets();
        delete_preset(&mut v, "beta").unwrap();
        let names = preset_names(&v);
        assert!(!names.contains(&"beta".to_string()));
        assert_eq!(tab_preset(&v, 1), "alpha", "tab moved to the preceding preset");
    }

    #[test]
    fn delete_first_reassigns_to_successor() {
        // Delete "alpha" (pos 0) -> successor "beta"; tab 0 moves to "beta".
        let mut v = user_with_presets();
        delete_preset(&mut v, "alpha").unwrap();
        assert!(!preset_names(&v).contains(&"alpha".to_string()));
        assert_eq!(tab_preset(&v, 0), "beta");
        assert!(!not_saved_names(&v).contains(&"alpha".to_string()), "notSaved entry removed");
    }

    #[test]
    fn delete_last_preset_is_refused() {
        // A tree with a single preset.
        let overview = Value::Dict(vec![
            (b("overviewProfilePresets"), Value::Tuple(vec![
                Value::Int(1), Value::Dict(vec![(b("only"), Value::Dict(vec![]))]),
            ])),
        ]);
        let mut v = Value::Dict(vec![(b("overview"), overview)]);
        assert!(matches!(delete_preset(&mut v, "only"), Err(OverviewTabError::LastPreset)));
    }

    #[test]
    fn delete_unknown_preset_errors() {
        let mut v = user_with_presets();
        assert!(matches!(delete_preset(&mut v, "nope"), Err(OverviewTabError::UnknownPreset { .. })));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p settings-model delete_`
Expected: compile error — `delete_preset` not found.

- [ ] **Step 3: Implement `delete_preset`**

Add to `overview_presets.rs`:

```rust
/// Delete a preset, reassigning every tab that used it to the immediately
/// preceding preset in sorted order (the successor when deleting the first).
/// Refuses the last preset. Also drops any matching `_notSaved` buffer entry.
pub fn delete_preset(v: &mut Value, name: &str) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    let target = {
        let presets = presets_mut(ov).ok_or(OverviewTabError::UnknownPreset { name: name.to_string() })?;
        if !presets.iter().any(|(k, _)| as_str(k).as_deref() == Some(name)) {
            return Err(OverviewTabError::UnknownPreset { name: name.to_string() });
        }
        if presets.len() <= 1 {
            return Err(OverviewTabError::LastPreset);
        }
        let names = sorted_names(presets);
        let pos = names.iter().position(|n| n == name).expect("name is present");
        if pos > 0 { names[pos - 1].clone() } else { names[pos + 1].clone() }
    };
    retarget_tabs(tabs_mut(ov), name, &target);
    if let Some(ns) = not_saved_mut(ov) {
        ns.retain(|(k, _)| as_str(k).as_deref() != Some(name));
    }
    if let Some(presets) = presets_mut(ov) {
        presets.retain(|(k, _)| as_str(k).as_deref() != Some(name));
    }
    Ok(())
}
```

Widen the preset re-export in `crates/settings-model/src/lib.rs` to its final form:

```rust
pub use overview_presets::{create_preset, delete_preset, rename_preset};
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p settings-model delete_`
Expected: PASS.

- [ ] **Step 5: Wire op, command, binding**

`app/src-tauri/src/ops.rs` — add `delete_preset` to the `settings_model` import, then add the wrapper:

```rust
pub fn preset_delete(state: &AppState, name: String) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| delete_preset(v, &name))
}
```

`app/src-tauri/src/lib.rs` (command + handler entry):

```rust
#[tauri::command]
fn preset_delete(state: tauri::State<'_, AppState>, name: String) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::preset_delete(&state, name)
}
```

`app/src/lib/api.ts`:

```ts
  presetDelete: (name: string) =>
    invoke<OverviewColumns>("preset_delete", { name }),
```

- [ ] **Step 6: Build + full crate test + check**

Run: `cargo test -p settings-model` (Expected: all pass)
Run: `cargo build -p app`
Run from `app/`: `npm run check`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add crates/settings-model/src/overview_presets.rs crates/settings-model/src/lib.rs app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Add preset delete with reassign-to-preceding and notSaved cleanup"
```

---

### Task 6: Corpus realshape + reshare round-trip guard

Prove the preset edits survive real EVE idioms (`(timestamp, dict)` wrappers, `Shared`/`Ref` preset keys/values) and that every edit path `reshare`s to a re-decodable file.

**Files:**
- Create: `crates/settings-model/tests/overview_presets_realshape.rs`

**Interfaces:**
- Consumes: `create_preset`, `rename_preset`, `delete_preset`, `project_overview` (public crate API), `blue_marshal::{encode, decode, reshare}`.

- [ ] **Step 1: Write the realshape test**

Create `crates/settings-model/tests/overview_presets_realshape.rs`. Mirror the style of `crates/settings-model/tests/overview_tabs_realshape.rs` (Shared/Ref-keyed tree, `(ts, dict)` wrappers), building a user tree whose preset name is a `Shared` token referenced (`Ref`) by a tab's `overview` field, then exercising rename and asserting the reshared tree re-decodes and the projection reflects the change:

```rust
//! Real-idiom guard for filter-preset authoring: `(timestamp, dict)` wrappers and
//! Shared/Ref-interned preset names (a preset key shared with a tab's `overview`
//! value), edited then reshared and re-decoded.

use blue_marshal::Value;
use settings_model::{create_preset, delete_preset, project_overview, rename_preset};

fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }

/// user -> overview -> {
///   tabsettings_new: (ts, { 0: { overview: Ref(7) } }),
///   overviewProfilePresets: (ts, { Shared(7,"pvp"): {groups:[25]}, "pve": {groups:[26]} }),
///   overviewProfilePresets_notSaved: (ts, { Ref(7): {groups:[99]} }),
/// }
/// Shared slot 7 interns the preset name "pvp"; the tab's overview value and the
/// notSaved key both Ref it — exactly how real files share the name.
fn realish_user() -> Value {
    let name_shared = Value::Shared { slot: 7, value: Box::new(b("pvp")) };
    let name_ref = Value::Ref(7);
    let tab0 = Value::Dict(vec![(b("overview"), name_ref.clone())]);
    let preset = |g: i64| Value::Dict(vec![(b("groups"), Value::List(vec![Value::Int(g)]))]);
    let overview = Value::Dict(vec![
        (b("tabsettings_new"), Value::Tuple(vec![
            Value::Int(1), Value::Dict(vec![(Value::Int(0), tab0)]),
        ])),
        (b("overviewProfilePresets"), Value::Tuple(vec![
            Value::Int(1),
            Value::Dict(vec![(name_shared, preset(25)), (b("pve"), preset(26))]),
        ])),
        (b("overviewProfilePresets_notSaved"), Value::Tuple(vec![
            Value::Int(1),
            Value::Dict(vec![(name_ref, preset(99))]),
        ])),
    ]);
    Value::Dict(vec![(b("overview"), overview)])
}

/// Reshare and confirm the tree still encodes+decodes to itself (the standard
/// regression check that an edit left a canonical, self-contained file).
fn reshare_roundtrips(v: &Value) -> Value {
    let reshared = blue_marshal::reshare(v);
    let bytes = blue_marshal::encode(&reshared).expect("encode");
    let decoded = blue_marshal::decode(&bytes).expect("decode");
    assert_eq!(decoded, reshared, "reshared tree must re-decode identically");
    reshared
}

#[test]
fn rename_across_shared_name_reshares_and_reprojects() {
    let mut v = realish_user();
    rename_preset(&mut v, "pvp", "pvp2").unwrap();
    let v = reshare_roundtrips(&v);
    let cols = project_overview(&v, None);
    assert!(cols.presets.contains(&"pvp2".to_string()));
    assert!(!cols.presets.contains(&"pvp".to_string()));
    assert_eq!(cols.tabs[0].preset, "pvp2", "the Ref'd tab followed the rename after inline");
}

#[test]
fn duplicate_then_delete_reshares_clean() {
    let mut v = realish_user();
    create_preset(&mut v, "pvp", "pvp copy").unwrap();
    delete_preset(&mut v, "pve").unwrap();
    let v = reshare_roundtrips(&v);
    let cols = project_overview(&v, None);
    assert!(cols.presets.contains(&"pvp copy".to_string()));
    assert!(!cols.presets.contains(&"pve".to_string()));
}
```

Confirmed variant/API shapes: `Value::Shared { slot: u32, value: Box<Value> }` and `Value::Ref(u32)` (a **tuple** variant — write `Ref(7)`, not `Ref { slot: 7 }`); `blue_marshal::{encode, decode, reshare}` are all public. These match `crates/settings-model/tests/overview_tabs_realshape.rs`.

- [ ] **Step 2: Run the test**

Run: `cargo test -p settings-model --test overview_presets_realshape`
Expected: PASS (both tests).

- [ ] **Step 3: Commit**

```bash
git add crates/settings-model/tests/overview_presets_realshape.rs
git commit -m "Guard preset edits against real Shared/Ref idioms"
```

---

### Task 7: Frontend — preset picker + management controls

Add a per-tab preset `<select>` and a Duplicate / Rename / Delete cluster to `OverviewView.svelte`, driven by the new api bindings. (No Svelte unit tests in this repo — verified by `npm run check`, `npm run build`, and the final live smoke.)

**Files:**
- Modify: `app/src/lib/OverviewView.svelte`

**Interfaces:**
- Consumes: `api.tabSetPreset`, `api.presetCreate`, `api.presetRename`, `api.presetDelete`, `OverviewColumns.presets`, `OverviewTab.preset`.

- [ ] **Step 1: Extend the `pending` union and add derived preset options**

In the `<script>` of `app/src/lib/OverviewView.svelte`, extend the `pending` state union (`:41-46`) with two preset kinds:

```ts
  let pending = $state<
    | { kind: "createTab"; value: string }
    | { kind: "renameTab"; value: string; tabIdx: number }
    | { kind: "addWindow"; value: string }
    | { kind: "duplicatePreset"; value: string; from: string }
    | { kind: "renamePreset"; value: string; old: string }
    | null
  >(null);
```

Add a derived option list after `currentWindowIndex` (`:36`):

```ts
  // The preset dropdown options: the sorted account presets, plus the tab's
  // current value if (defensively) it isn't among them. Empty "" shows as (default).
  const presetOptions = $derived.by(() => {
    const list = data?.presets ?? [];
    const cur = tab?.preset ?? "";
    return list.includes(cur) ? list : [cur, ...list];
  });
  // Preset-management actions operate on the selected tab's current preset; they
  // are meaningful only when that preset is a real (listed) account preset.
  const presetIsReal = $derived(!!tab && (data?.presets.includes(tab.preset) ?? false));
```

- [ ] **Step 2: Add the handlers**

Add these functions in the `<script>` (near `deleteTab`, `:103`):

```ts
  async function setTabPreset(preset: string) {
    if (!tab || preset === tab.preset) return;
    try { data = await api.tabSetPreset(tab.index, preset); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
  function startDuplicatePreset() {
    if (!tab) return;
    pending = { kind: "duplicatePreset", value: `${tab.preset} copy`, from: tab.preset };
  }
  function startRenamePreset() {
    if (!tab) return;
    pending = { kind: "renamePreset", value: tab.preset, old: tab.preset };
  }
  async function deletePreset() {
    if (!tab || !data) return;
    const name = tab.preset;
    const list = data.presets;
    const pos = list.indexOf(name);
    if (pos < 0 || list.length <= 1) return;
    const neighbour = pos > 0 ? list[pos - 1] : list[pos + 1];
    const ok = await confirm(
      `Delete preset "${name}"? Tabs using it will move to "${neighbour}".`,
      { title: "Delete preset", kind: "warning" },
    );
    if (!ok) return;
    try { data = await api.presetDelete(name); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
```

- [ ] **Step 3: Handle the new kinds in `submitPending`**

Rewrite `submitPending`'s body (`:61-92`) to branch on all kinds explicitly (the current trailing `else` assumed `addWindow`):

```ts
  async function submitPending() {
    if (!pending) return;
    const p = pending;
    const name = p.value.trim();
    pending = null;
    if (!name) return;
    try {
      if (p.kind === "createTab") {
        data = await api.tabCreate(currentWindowIndex ?? 0, name, tabIndex);
        tabIndex = Math.max(...data.tabs.map((t) => t.index));
        onUserDirty();
      } else if (p.kind === "renameTab") {
        if (name === data?.tabs.find((t) => t.index === p.tabIdx)?.name) return;
        data = await api.tabRename(p.tabIdx, name);
        onUserDirty();
      } else if (p.kind === "addWindow") {
        data = await api.overviewWindowAdd(name, tabIndex);
        tabIndex = Math.max(...data.tabs.map((t) => t.index));
        onUserDirty();
        onCharDirty();
        const w = data.windows[data.windows.length - 1];
        if (w) onWindowAdded(w.index === 0 ? "overview" : `overview_${w.index}`);
      } else if (p.kind === "duplicatePreset") {
        data = await api.presetCreate(p.from, name);
        onUserDirty();
      } else if (p.kind === "renamePreset") {
        if (name === p.old) return;
        data = await api.presetRename(p.old, name);
        onUserDirty();
      }
    } catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
```

- [ ] **Step 4: Add the picker + management markup, and generalize the name-entry copy**

In the markup, after the `Tab` `<label>`/`<select>` block and its `.tab-actions` (`:200-227`), add the preset picker and management cluster inside `.ov-controls`:

```svelte
    {#if tab}
      <label>Preset
        <select value={tab.preset} onchange={(e) => setTabPreset((e.currentTarget as HTMLSelectElement).value)}>
          {#each presetOptions as p (p)}<option value={p}>{p || "(default)"}</option>{/each}
        </select>
      </label>
      <div class="preset-actions">
        <button onclick={startDuplicatePreset} disabled={!presetIsReal} title="Duplicate this preset">Duplicate preset</button>
        <button onclick={startRenamePreset} disabled={!presetIsReal} title="Rename this preset">Rename preset</button>
        <button class="danger" onclick={deletePreset}
                disabled={!presetIsReal || (data?.presets.length ?? 0) <= 1}
                title="Delete this preset">Delete preset</button>
      </div>
    {/if}
```

Update the name-entry `<input>` placeholder and submit-button label (`:230-238`) to cover the preset kinds:

```svelte
    {#if pending}
      <div class="name-entry">
        <input type="text" bind:value={pending.value} use:focusInput
               placeholder={pending.kind === "addWindow" ? "First tab name"
                 : pending.kind === "duplicatePreset" || pending.kind === "renamePreset" ? "Preset name"
                 : "Tab name"}
               onkeydown={(e) => {
                 if (e.key === "Enter") { e.preventDefault(); submitPending(); }
                 else if (e.key === "Escape") pending = null;
               }} />
        <button onclick={submitPending}>
          {pending.kind === "addWindow" ? "Add window"
            : pending.kind === "renameTab" ? "Rename"
            : pending.kind === "duplicatePreset" ? "Duplicate"
            : pending.kind === "renamePreset" ? "Rename preset"
            : "Add tab"}
        </button>
        <button onclick={() => (pending = null)}>Cancel</button>
      </div>
    {/if}
```

Add the button-cluster style in the `<style>` block (next to `.tab-actions`, `:313`):

```css
  .preset-actions { display: flex; gap: 0.4rem; align-items: center; flex-wrap: wrap; }
```

- [ ] **Step 5: Type-check and build**

Run from `app/` (PowerShell):
`npm run check` (Expected: no errors)
`npm run build` (Expected: builds clean)

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/OverviewView.svelte
git commit -m "Add the preset picker and management controls to the overview editor"
```

---

### Task 8: Live smoke + small-tasks/memory bookkeeping

Validate against a real EVE client (the gate every prior milestone used), and record the outcome.

- [ ] **Step 1: Full verification pass**

Run: `cargo test` (whole workspace — Expected: all pass)
Run from `app/`: `npm run check` and `npm run build` (Expected: clean)

- [ ] **Step 2: Live smoke (manual, with the EVE client closed)**

Using the built app on a real profile:
1. Open a character paired to an account; open the Overview editor.
2. **Assign:** change a tab's Preset dropdown to another preset; Save.
3. **Duplicate:** duplicate the tab's preset under a new name; assign a tab to the copy; Save.
4. **Rename:** rename a preset in use by a tab; confirm the tab still points at it (its dropdown shows the new name); Save.
5. **Delete:** delete a preset used by a tab; confirm the tab moved to the named preceding preset; Save.
6. Launch EVE, log in the character, and confirm: each tab shows the expected filter, no phantom/duplicate preset appears in the in-game overview preset dropdown (the §7 `_notSaved` check), and "reset overview settings" does not throw.

Record any deviation as a new item in `docs/small-tasks.md`.

- [ ] **Step 3: Update the small-tasks ledger and memory**

If the smoke surfaced fixes, apply them (each as its own commit) and re-smoke. When green, note slice 2a as shipped in `docs/small-tasks.md` history if appropriate, and update the milestone-state memory (`eve-editor-milestone-state.md`) to mark overview-depth slice 2a done and 2b next.

- [ ] **Step 4: Finish the branch**

Use the `superpowers:finishing-a-development-branch` skill to open the PR (or merge), following the repo's PR conventions (body via `--body-file`, PowerShell for `gh`).

---

## Notes for the implementer

- **Re-export widening:** Tasks 2/4/5 each widen `pub use overview_presets::{…}` in `crates/settings-model/src/lib.rs` as they add a function. If you implement out of order, only export what exists, or the crate won't compile.
- **ops.rs import:** the `settings_model::{…}` import in `app/src-tauri/src/ops.rs` must list exactly the names in scope. Add `create_preset`/`set_tab_preset`/`rename_preset`/`delete_preset` as each task introduces them, not before.
- **Borrow discipline:** the preset functions reborrow `ov` (`presets_mut` → `not_saved_mut` → `tabs_mut`) in sequential statements; keep them sequential (don't hold two of these borrows at once) and they compile under NLL.
- **Sort parity:** the projection (`preset_names`, Task 1) and the model (`sorted_names`, Task 2) must both sort by `to_lowercase()` so the UI-named delete neighbour matches the model's reassignment target.
