# Overview Tab Management — Phase B Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add / remove whole overview windows — spawn a real, positionable second overview window and remove the last one — completing the overview tab-management milestone.

**Architecture:** An overview window has two halves split across two files. The **grouping** (which tabs belong to which window) lives in the user (`core_user`) file's `overview` → `tabsByWindowInstanceID` (a list of tab-index lists; position `i` ↔ char window key `overview` for i=0, `overview_{i}` for i≥1). The **geometry** (position/size/flags) lives in the char (`core_char`) file's `windows` dict. `overview_tabs.rs` gains user-side `add_overview_window` / `remove_overview_window` and char-side `add_overview_window_geometry` / `remove_overview_window_geometry`; the char-side ones mint a window by *cloning the primary `overview` entry* in every `windows` subdict (correct-by-construction flags) and offsetting its geometry. `ops.rs` orchestrates both slots (user required, char best-effort). UI lives in the existing `OverviewView.svelte`.

**Tech Stack:** Rust (settings-model crate, Tauri app crate), Svelte 5, `blue_marshal` codec, `cargo test` for Rust, `npm run check` for the frontend.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-19-overview-tab-management-design.md` (§6 resolved 2026-07-20).
- Repo convention: **sentence-case commit subjects, no attribution trailers of any kind.**
- Structural edits use the inline-first idiom: `inline_all(v)` at the top of every edit fn; the app layer runs `blue_marshal::reshare` after the edit (never encode an inlined tree).
- `blue_marshal::reshare` shares `Bytes`/`Long`/`List`/`Dict` but **not `Str` and not `Tuple`** (the Tuple-non-sharing fix keeps geometry tuples from aliasing).
- Overview grouping lives in the **user file**: `overview` → `tabsByWindowInstanceID` (list of tab-index lists). Window geometry lives in the **char file**: `windows` (a plain `Dict`) → per-window subdicts (`windowSizesAndPositions_1`, `openWindows`, `minimizedWindows`, `lockedWindows`, `compactWindows`, `isLightBackgroundWindows`, …), each a `(timestamp, dict)` tuple keyed by window id.
- `Value` variants (from `blue_marshal`): `Int(i64)`, `Long(Vec<u8>)` (varint bytes — **not** an i64), `Bool(bool)`, `Bytes(Vec<u8>)`, `Str(String)`, `StrUcs2(String)`, `StrTable(u8)`, `Tuple(Vec<Value>)`, `List(Vec<Value>)`, `Dict(Vec<(Value,Value)>)`, `None`, `Shared { slot, value }`, `Ref(u32)`. Geometry coords are `Int`.
- Positional link ⇒ removing a *middle* window would shift later windows off their `overview_N` keys; **remove is last-window-only this slice** (documented deferral in `docs/small-tasks.md`).
- No character/account ids or names in code, tests, or docs (repo data rule); use synthetic tokens.
- Frontend DOM interactions are validated by manual smoke, not unit tests (project norm).
- Cargo is on the Bash tool PATH; `npm` is **not** — run `npm run check` via the PowerShell tool in `app/`.

## File Structure

- Modify `crates/settings-model/src/overview_tabs.rs` — the four new window fns + two error variants + `offset_coord`; inline `#[cfg(test)]` unit tests.
- Modify `crates/settings-model/src/lib.rs` — re-export the four new fns.
- Modify `crates/settings-model/tests/overview_tabs_realshape.rs` — realshape round-trip test for the window add (user + char halves).
- Modify `app/src-tauri/src/ops.rs` — cross-file `overview_window_add` / `overview_window_remove`, a `tab_err` helper (extracted from `edit_user_tabs`); inline ops test.
- Modify `app/src-tauri/src/lib.rs` — two `#[tauri::command]` wrappers + `generate_handler!` registration.
- Modify `app/src/lib/api.ts` — two invoke bindings.
- Modify `app/src/lib/OverviewView.svelte` — add / remove window controls + handlers.

---

### Task 1: User-file `add_overview_window` / `remove_overview_window` + error variants

The grouping half, in the user file. Reuses `create_tab` to seed the new window's tab.

**Files:**
- Modify: `crates/settings-model/src/overview_tabs.rs`
- Modify: `crates/settings-model/src/lib.rs`
- Test: `crates/settings-model/src/overview_tabs.rs` (inline `#[cfg(test)]`)

**Interfaces:**
- Consumes: existing `create_tab`, `overview_mut`, `groups_mut`, `list_inner`, `list_inner_mut`, `is_b`, `as_int`, `inline_all`.
- Produces:
  - `OverviewTabError::NoWindowMapping` and `OverviewTabError::NotLastWindow { index: usize }` (new variants).
  - `pub fn add_overview_window(v: &mut Value, name: &str, from_tab: Option<i64>) -> Result<usize, OverviewTabError>` — appends a new inner list to `tabsByWindowInstanceID`, seeds it with one cloned tab via `create_tab`, returns the new window index. Refuses (`NoWindowMapping`) when the account has no window mapping.
  - `pub fn remove_overview_window(v: &mut Value, window_idx: usize) -> Result<(), OverviewTabError>` — reassigns the window's tabs onto window 0, removes the inner list. Guards: `LastWindow` (≤1 window), `UnknownWindow` (out of range), `NotLastWindow` (not the last window).

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `overview_tabs.rs` (the `user_with_tabs`, `user_two_windows`, `tab_name`, `tab_has_key`, `window_indices` helpers already exist there):

```rust
    #[test]
    fn add_window_appends_a_group_with_a_cloned_tab() {
        let mut v = user_with_tabs(); // one window [0]
        let widx = add_overview_window(&mut v, "Scan", Some(0)).unwrap();
        assert_eq!(widx, 1, "new window appended at index 1");
        let new_tabs = window_indices(&v, 1);
        assert_eq!(new_tabs.len(), 1, "new window seeded with exactly one tab");
        assert_eq!(tab_name(&v, new_tabs[0]), "Scan");
        // Seeded via create_tab -> carries bracket/color like every valid EVE tab.
        assert!(tab_has_key(&v, new_tabs[0], b"bracket"), "seeded tab clones bracket");
        assert!(tab_has_key(&v, new_tabs[0], b"color"), "seeded tab clones color");
        assert_eq!(window_indices(&v, 0), vec![0], "window 0 untouched");
    }

    #[test]
    fn add_window_on_a_windowless_account_is_refused() {
        // Overview with tabs but no tabsByWindowInstanceID: positional add can't
        // fabricate a base mapping without hiding the account's existing tabs.
        let tab = Value::Dict(vec![
            (Value::Bytes(b"bracket".to_vec()), Value::Bytes(b"_BracketFilterShowAll".to_vec())),
            (Value::Bytes(b"color".to_vec()), Value::None),
            (Value::Str("name".into()), Value::Str("Main".into())),
            (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
        ]);
        let overview = Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()), Value::Dict(vec![(Value::Int(0), tab)])),
        ]);
        let mut v = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), overview)]);
        assert!(matches!(add_overview_window(&mut v, "X", Some(0)), Err(OverviewTabError::NoWindowMapping)));
    }

    #[test]
    fn remove_last_window_reassigns_its_tabs_to_window_zero() {
        let mut v = user_two_windows(); // window 0 = [0], window 1 = [1]
        remove_overview_window(&mut v, 1).unwrap();
        assert_eq!(window_indices(&v, 0), vec![0, 1], "removed window's tab moved to window 0");
        // Only one window remains.
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, g) = ovd.iter().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")).unwrap();
        let Value::List(outer) = g else { panic!() };
        assert_eq!(outer.len(), 1, "one window left");
        assert_eq!(tab_name(&v, 1), "B", "no tab deleted");
    }

    #[test]
    fn remove_non_last_window_is_refused() {
        let mut v = user_two_windows();
        assert!(matches!(remove_overview_window(&mut v, 0), Err(OverviewTabError::NotLastWindow { index: 0 })));
    }

    #[test]
    fn remove_the_only_window_is_refused() {
        let mut v = user_with_tabs(); // one window
        assert!(matches!(remove_overview_window(&mut v, 0), Err(OverviewTabError::LastWindow)));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p settings-model overview_tabs::tests::add_window overview_tabs::tests::remove`
Expected: FAIL — `add_overview_window` / `remove_overview_window` / `NoWindowMapping` / `NotLastWindow` not found (compile error).

- [ ] **Step 3: Add the error variants**

In `overview_tabs.rs`, extend the `OverviewTabError` enum with two variants (place after `LastWindow`):

```rust
    /// Refused: this overview has no window mapping to add onto (windowless account).
    NoWindowMapping,
    /// Refused: only the last overview window can be removed for now.
    NotLastWindow { index: usize },
```

And extend the `Display` match arms (place after the `LastWindow` arm):

```rust
            OverviewTabError::NoWindowMapping => write!(f, "This overview has no window layout to add to."),
            OverviewTabError::NotLastWindow { index } => write!(f, "Only the last overview window can be removed (tried {index})."),
```

- [ ] **Step 4: Implement the two functions**

Add after `move_tab` in `overview_tabs.rs`:

```rust
/// Add a new overview window (user-file grouping half). Appends an empty inner
/// list to `tabsByWindowInstanceID` and seeds it with one cloned tab (a window
/// must have ≥1 tab). Refuses on a windowless account: adding positionally there
/// would fabricate a partial mapping that hides the account's existing tabs (see
/// `create_tab`). Returns the new window's index (its char key is `overview_{idx}`).
pub fn add_overview_window(v: &mut Value, name: &str, from_tab: Option<i64>) -> Result<usize, OverviewTabError> {
    inline_all(v);
    let new_window_idx = {
        let ov = overview_mut(v)?;
        let window_count = ov.iter()
            .find(|(k, _)| is_b(k, b"tabsByWindowInstanceID"))
            .and_then(|(_, wv)| list_inner(wv))
            .map_or(0, |g| g.len());
        if window_count == 0 {
            return Err(OverviewTabError::NoWindowMapping);
        }
        let groups = groups_mut(ov);
        groups.push(Value::List(Vec::new()));
        groups.len() - 1
    };
    // Seed the new (empty) window with one cloned tab. create_tab re-inlines (a
    // no-op on the already-plain tree) and appends the tab to window `new_window_idx`.
    create_tab(v, new_window_idx, name, from_tab)?;
    Ok(new_window_idx)
}

/// Remove an overview window (user-file grouping half). Reassigns the window's
/// tabs onto window 0 (no tab loss), then drops the inner list. Last-window-only:
/// the positional link to the char-file `overview_N` keys makes middle removal a
/// re-key cascade (deferred).
pub fn remove_overview_window(v: &mut Value, window_idx: usize) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    // Read the mapping WITHOUT fabricating it (a windowless account has none).
    let groups = ov.iter_mut()
        .find(|(k, _)| is_b(k, b"tabsByWindowInstanceID"))
        .and_then(|(_, wv)| list_inner_mut(wv))
        .ok_or(OverviewTabError::LastWindow)?;
    let count = groups.len();
    if count <= 1 {
        return Err(OverviewTabError::LastWindow);
    }
    if window_idx >= count {
        return Err(OverviewTabError::UnknownWindow { index: window_idx });
    }
    if window_idx != count - 1 {
        return Err(OverviewTabError::NotLastWindow { index: window_idx });
    }
    let removed: Vec<Value> = list_inner(&groups[window_idx]).cloned().unwrap_or_default();
    if let Some(w0) = groups.get_mut(0).and_then(list_inner_mut) {
        w0.extend(removed);
    }
    groups.remove(window_idx);
    Ok(())
}
```

Then extend the `pub use overview_tabs::{...}` line in `crates/settings-model/src/lib.rs` to add `add_overview_window` and `remove_overview_window`:

```rust
pub use overview_tabs::{
    add_overview_window, create_tab, delete_tab, move_tab, remove_overview_window, rename_tab,
    reorder_tabs_in_window, OverviewTabError,
};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p settings-model overview_tabs`
Expected: PASS (the five new tests plus all existing `overview_tabs` tests).

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/src/overview_tabs.rs crates/settings-model/src/lib.rs
git commit -m "Add overview-window add/remove grouping edits to overview_tabs"
```

---

### Task 2: Char-file geometry — `add_overview_window_geometry` / `remove_overview_window_geometry`

Mint / drop the `overview_{N}` window in the char file's `windows` dict by cloning the primary `overview` entry across every subdict.

**Files:**
- Modify: `crates/settings-model/src/overview_tabs.rs`
- Modify: `crates/settings-model/src/lib.rs`
- Test: `crates/settings-model/src/overview_tabs.rs` (inline `#[cfg(test)]`)

**Interfaces:**
- Consumes: existing `dict_inner_mut`, `is_b`, `inline_all`.
- Produces:
  - `pub fn add_overview_window_geometry(v: &mut Value, window_idx: usize)` — clones the primary `overview` value in every `windows` subdict into key `overview_{window_idx}`, offsetting the geometry rect. No-op for `window_idx == 0`, or when there is no `windows` dict / no primary entry in a subdict. Idempotent (skips a subdict that already has the key).
  - `pub fn remove_overview_window_geometry(v: &mut Value, window_idx: usize)` — drops `overview_{window_idx}` from every `windows` subdict. No-op for `window_idx == 0` or when absent.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `overview_tabs.rs`:

```rust
    /// A char tree: `windows` (plain dict) -> two `(ts,dict)` subdicts, each with
    /// a primary `overview` entry (geometry tuple / bool flag), mirroring real files.
    fn char_with_primary_overview() -> Value {
        let geom = |x: i64| Value::Tuple(vec![
            Value::Int(x), Value::Int(100), Value::Int(400), Value::Int(300),
            Value::Int(2560), Value::Int(1440),
        ]);
        let sub = |entries: Vec<(Value, Value)>|
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Dict(entries)]);
        let windows = Value::Dict(vec![
            (Value::Bytes(b"windowSizesAndPositions_1".to_vec()),
             sub(vec![(Value::Bytes(b"overview".to_vec()), geom(1000))])),
            (Value::Bytes(b"openWindows".to_vec()),
             sub(vec![(Value::Bytes(b"overview".to_vec()), Value::Bool(true))])),
        ]);
        Value::Dict(vec![(Value::Bytes(b"windows".to_vec()), windows)])
    }

    /// The window-id keys present in one `windows` subdict (tree already plain).
    fn win_keys(v: &Value, subdict: &[u8]) -> Vec<Vec<u8>> {
        let Value::Dict(root) = v else { panic!() };
        let (_, wins) = root.iter().find(|(k, _)| is_b(k, b"windows")).unwrap();
        let Value::Dict(subs) = wins else { panic!() };
        let (_, sv) = subs.iter().find(|(k, _)| is_b(k, subdict)).unwrap();
        let d = dict_inner_ref(sv).unwrap();
        d.iter().filter_map(|(k, _)| if let Value::Bytes(b) = k { Some(b.clone()) } else { None }).collect()
    }

    /// The (x, y) of a window's geometry tuple in `windowSizesAndPositions_1`.
    fn geom_xy(v: &Value, key: &[u8]) -> (i64, i64) {
        let Value::Dict(root) = v else { panic!() };
        let (_, wins) = root.iter().find(|(k, _)| is_b(k, b"windows")).unwrap();
        let Value::Dict(subs) = wins else { panic!() };
        let (_, sv) = subs.iter().find(|(k, _)| is_b(k, b"windowSizesAndPositions_1")).unwrap();
        let d = dict_inner_ref(sv).unwrap();
        let (_, g) = d.iter().find(|(k, _)| is_b(k, key)).unwrap();
        let Value::Tuple(items) = g else { panic!() };
        let Value::Int(x) = items[0] else { panic!() };
        let Value::Int(y) = items[1] else { panic!() };
        (x, y)
    }

    /// Read-only `(ts,dict)`/dict unwrap, for the assertions above.
    fn dict_inner_ref(v: &Value) -> Option<&Entries> {
        match v {
            Value::Dict(d) => Some(d),
            Value::Tuple(items) => items.iter().find_map(|e| if let Value::Dict(d) = e { Some(d) } else { None }),
            _ => None,
        }
    }

    #[test]
    fn add_geometry_clones_primary_into_overview_n_with_offset() {
        let mut v = char_with_primary_overview();
        add_overview_window_geometry(&mut v, 1);
        assert!(win_keys(&v, b"windowSizesAndPositions_1").iter().any(|k| k == b"overview_1"),
            "overview_1 minted in the geometry subdict");
        assert!(win_keys(&v, b"openWindows").iter().any(|k| k == b"overview_1"),
            "overview_1 minted in the flags subdict too");
        assert_eq!(geom_xy(&v, b"overview"), (1000, 100), "primary unchanged");
        assert_eq!(geom_xy(&v, b"overview_1"), (1040, 140), "clone offset by (40, 40)");
    }

    #[test]
    fn add_geometry_is_idempotent() {
        let mut v = char_with_primary_overview();
        add_overview_window_geometry(&mut v, 1);
        add_overview_window_geometry(&mut v, 1);
        let count = win_keys(&v, b"windowSizesAndPositions_1")
            .iter().filter(|k| k.as_slice() == b"overview_1").count();
        assert_eq!(count, 1, "not double-added");
        assert_eq!(geom_xy(&v, b"overview_1"), (1040, 140), "not offset twice");
    }

    #[test]
    fn remove_geometry_drops_overview_n_everywhere() {
        let mut v = char_with_primary_overview();
        add_overview_window_geometry(&mut v, 1);
        remove_overview_window_geometry(&mut v, 1);
        assert!(!win_keys(&v, b"windowSizesAndPositions_1").iter().any(|k| k == b"overview_1"));
        assert!(!win_keys(&v, b"openWindows").iter().any(|k| k == b"overview_1"));
        assert!(win_keys(&v, b"windowSizesAndPositions_1").iter().any(|k| k == b"overview"),
            "primary untouched");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p settings-model overview_tabs::tests::add_geometry overview_tabs::tests::remove_geometry`
Expected: FAIL — `add_overview_window_geometry` / `remove_overview_window_geometry` not found.

- [ ] **Step 3: Implement the geometry functions**

Add after `remove_overview_window` in `overview_tabs.rs`:

```rust
/// Char-file: mint the window `overview_{window_idx}` by cloning the primary
/// `overview` window's value in every `windows` subdict (geometry + all flag
/// dicts) and offsetting the new window's on-screen position so it doesn't sit
/// exactly on the primary. Cloning the primary makes the required-flag set
/// correct by construction. `window_idx` must be ≥1 (0 IS the primary key). No-op
/// when there is no `windows` dict, or no primary entry in a given subdict;
/// idempotent (skips a subdict that already has the key).
pub fn add_overview_window_geometry(v: &mut Value, window_idx: usize) {
    if window_idx == 0 {
        return;
    }
    inline_all(v);
    let key = format!("overview_{window_idx}");
    let Value::Dict(root) = v else { return };
    let Some((_, wins)) = root.iter_mut().find(|(k, _)| is_b(k, b"windows")) else { return };
    let Value::Dict(subdicts) = wins else { return };
    for (subkey, subval) in subdicts.iter_mut() {
        let is_geom = is_b(subkey, b"windowSizesAndPositions_1");
        let Some(entries) = dict_inner_mut(subval) else { continue };
        if entries.iter().any(|(k, _)| is_b(k, key.as_bytes())) {
            continue;
        }
        let Some(prim) = entries.iter()
            .find(|(k, _)| is_b(k, b"overview"))
            .map(|(_, val)| val.clone()) else { continue };
        let mut newval = prim;
        if is_geom {
            if let Value::Tuple(items) = &mut newval {
                if let Some(x) = items.get_mut(0) { offset_coord(x, 40); }
                if let Some(y) = items.get_mut(1) { offset_coord(y, 40); }
            }
        }
        entries.push((Value::Bytes(key.as_bytes().to_vec()), newval));
    }
}

/// Char-file inverse of `add_overview_window_geometry`: drop `overview_{window_idx}`
/// from every `windows` subdict. No-op for `window_idx == 0` or when absent.
pub fn remove_overview_window_geometry(v: &mut Value, window_idx: usize) {
    if window_idx == 0 {
        return;
    }
    inline_all(v);
    let key = format!("overview_{window_idx}");
    let Value::Dict(root) = v else { return };
    let Some((_, wins)) = root.iter_mut().find(|(k, _)| is_b(k, b"windows")) else { return };
    let Value::Dict(subdicts) = wins else { return };
    for (_, subval) in subdicts.iter_mut() {
        if let Some(entries) = dict_inner_mut(subval) {
            entries.retain(|(k, _)| !is_b(k, key.as_bytes()));
        }
    }
}

/// Bump an integer geometry coordinate by `delta`. Coords are `Int` on real
/// files; any other variant is left unchanged (the window overlaps the primary
/// and the user drags it — acceptable, never wrong).
fn offset_coord(v: &mut Value, delta: i64) {
    if let Value::Int(n) = v {
        *n += delta;
    }
}
```

Then extend the `pub use overview_tabs::{...}` line in `crates/settings-model/src/lib.rs` to add the two geometry fns:

```rust
pub use overview_tabs::{
    add_overview_window, add_overview_window_geometry, create_tab, delete_tab, move_tab,
    remove_overview_window, remove_overview_window_geometry, rename_tab, reorder_tabs_in_window,
    OverviewTabError,
};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p settings-model overview_tabs`
Expected: PASS (the three new tests plus all existing `overview_tabs` tests).

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/overview_tabs.rs crates/settings-model/src/lib.rs
git commit -m "Mint overview-window geometry by cloning the primary window"
```

---

### Task 3: Realshape round-trip test (user grouping + char geometry)

Guard both halves against real idioms — `(ts,dict)` wrappers, `Shared`/`Ref` aliased geometry — surviving the `reshare → encode → decode` save boundary.

**Files:**
- Modify: `crates/settings-model/tests/overview_tabs_realshape.rs`

**Interfaces:**
- Consumes: `settings_model::{add_overview_window, add_overview_window_geometry, project_overview}`; `blue_marshal::{decode, encode, inline, reshare, Value}`; the file's existing `b`, `ts`, `legacy_user` helpers.

- [ ] **Step 1: Write the test**

Extend the `use settings_model::{...}` line at the top of `overview_tabs_realshape.rs` to include `add_overview_window` and `add_overview_window_geometry`:

```rust
use settings_model::{
    add_overview_window, add_overview_window_geometry, create_tab, delete_tab, project_overview,
    rename_tab,
};
```

Then append to the file:

```rust
/// A char tree with two windows aliasing one geometry tuple via Shared/Ref (real
/// files dedupe identical rects), a `(ts,dict)` wrapper, and a flags subdict.
fn char_realshape() -> Value {
    let geom = Value::Shared {
        slot: 5,
        value: Box::new(Value::Tuple(vec![
            Value::Int(1707), Value::Int(288), Value::Int(853), Value::Int(1152),
            Value::Int(2560), Value::Int(1440),
        ])),
    };
    let sizes = Value::Tuple(vec![ts(), Value::Dict(vec![
        (b(b"overview"), geom),
        (b(b"market"), Value::Ref(5)),
    ])]);
    let opens = Value::Tuple(vec![ts(), Value::Dict(vec![
        (b(b"overview"), Value::Bool(true)),
    ])]);
    let windows = Value::Dict(vec![
        (b(b"windowSizesAndPositions_1"), sizes),
        (b(b"openWindows"), opens),
    ]);
    Value::Dict(vec![(b(b"windows"), windows)])
}

/// True if window `key` is present in char `windows` subdict `subdict` (inlined).
fn char_win_has(v: &Value, subdict: &[u8], key: &[u8]) -> bool {
    fn isb(k: &Value, n: &[u8]) -> bool { matches!(k, Value::Bytes(b) if b.as_slice() == n) }
    fn inner(v: &Value) -> Option<&Vec<(Value, Value)>> {
        match v {
            Value::Dict(d) => Some(d),
            Value::Tuple(t) => t.iter().find_map(|e| if let Value::Dict(d) = e { Some(d) } else { None }),
            _ => None,
        }
    }
    let Value::Dict(root) = v else { return false };
    let Some((_, wins)) = root.iter().find(|(k, _)| isb(k, b"windows")) else { return false };
    let Value::Dict(subs) = wins else { return false };
    let Some((_, sv)) = subs.iter().find(|(k, _)| isb(k, subdict)) else { return false };
    let Some(d) = inner(sv) else { return false };
    d.iter().any(|(k, _)| isb(k, key))
}

#[test]
fn add_window_survives_reshare_roundtrip_user_and_char() {
    // USER half: add a window (clones a sibling tab into a fresh group).
    let mut user = legacy_user(); // one window [0, 1]
    let widx = add_overview_window(&mut user, "Scan", Some(0)).unwrap();
    assert_eq!(widx, 1);
    user = reshare(&user);
    let uround = decode(&encode(&user).expect("user encodes")).expect("user re-decodes");
    assert_eq!(uround, user, "user grouping add round-trips after reshare");
    let cols = project_overview(&uround, None);
    assert_eq!(cols.windows.len(), 2, "two overview windows after add");

    // CHAR half: mint overview_1 geometry by cloning primary; survives reshare.
    let mut charf = char_realshape();
    add_overview_window_geometry(&mut charf, 1);
    charf = reshare(&charf);
    let cround = decode(&encode(&charf).expect("char encodes")).expect("char re-decodes");
    assert_eq!(cround, charf, "char geometry mint round-trips after reshare");
    let flat = inline(&cround);
    assert!(char_win_has(&flat, b"windowSizesAndPositions_1", b"overview_1"),
        "overview_1 geometry present through the save path");
    assert!(char_win_has(&flat, b"openWindows", b"overview_1"),
        "overview_1 flag present through the save path");
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p settings-model --test overview_tabs_realshape`
Expected: PASS (the new test plus the existing `edits_survive_reshare_roundtrip_and_migrate_legacy`). If it FAILS, the failure is a real model bug — fix the edit fn, not the test.

- [ ] **Step 3: Commit**

```bash
git add crates/settings-model/tests/overview_tabs_realshape.rs
git commit -m "Add an overview-window add realshape round-trip test"
```

---

### Task 4: Backend commands — cross-file `overview_window_*` in `ops.rs` + Tauri + api

Orchestrate the two slots (user required, char best-effort), register the commands, and add the JS bindings.

**Files:**
- Modify: `app/src-tauri/src/ops.rs`
- Modify: `app/src-tauri/src/lib.rs`
- Modify: `app/src/lib/api.ts`
- Test: `app/src-tauri/src/ops.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `settings_model::{add_overview_window, remove_overview_window, add_overview_window_geometry, remove_overview_window_geometry, OverviewTabError}`; existing `overview_columns`, `AppState`, `Fidelity`, `ErrDto`, `open_file`, `temp_file`, `Slot`.
- Produces (ops.rs, both return the refreshed `OverviewColumns`):
  - `overview_window_add(state, name: String, from_tab: Option<i64>)`
  - `overview_window_remove(state, window_idx: usize)`
  - `tab_err(e: OverviewTabError) -> ErrDto` (extracted helper).

- [ ] **Step 1: Write the failing test**

Add to the `ops.rs` `#[cfg(test)]` module (model the fixture on the existing `tab_rename_then_reproject_reflects_the_new_name` test):

```rust
    #[test]
    fn overview_window_add_then_remove_roundtrips_the_projection() {
        // A user file with one overview window [0] holding tab 0.
        let user = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()), Value::Dict(vec![(
                Value::Int(0),
                Value::Dict(vec![
                    (Value::Bytes(b"bracket".to_vec()), Value::Bytes(b"_BracketFilterShowAll".to_vec())),
                    (Value::Bytes(b"color".to_vec()), Value::None),
                    (Value::Str("name".into()), Value::Str("Main".into())),
                    (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
                ]),
            )])),
            (Value::Bytes(b"tabsByWindowInstanceID".to_vec()),
             Value::List(vec![Value::List(vec![Value::Int(0)])])),
        ]))]);
        let path = temp_file("ovwin", &encode(&user).unwrap());
        let state = AppState::new();
        open_file(&state, Slot::User, path.to_str().unwrap()).unwrap();

        // Add a window -> two windows, the new one seeded with a cloned tab.
        let cols = overview_window_add(&state, "Scan".into(), Some(0)).unwrap();
        assert_eq!(cols.windows.len(), 2, "window added");
        assert_eq!(cols.tabs.len(), 2, "new window seeded with one cloned tab");

        // Remove the last window -> back to one, its tab reassigned to window 0.
        let cols = overview_window_remove(&state, 1).unwrap();
        assert_eq!(cols.windows.len(), 1, "window removed");
        assert_eq!(cols.windows[0].tab_indices.len(), 2, "removed window's tab moved to window 0");
        assert_eq!(cols.tabs.len(), 2, "no tabs deleted");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p app overview_window_add_then_remove`
Expected: FAIL — `overview_window_add` not found.

- [ ] **Step 3: Implement `tab_err` + the two commands**

In `ops.rs`, extend the `use settings_model::{...}` import to add `add_overview_window, remove_overview_window, add_overview_window_geometry, remove_overview_window_geometry` (find the existing import with `grep -n "use settings_model" app/src-tauri/src/ops.rs`; it already brings in `create_tab`, `OverviewTabError`, etc.).

Add the `tab_err` helper just above `edit_user_tabs`:

```rust
/// Map an `OverviewTabError` to a frontend `ErrDto`, carrying its `code` tag.
fn tab_err(e: OverviewTabError) -> ErrDto {
    let jv = serde_json::to_value(&e).unwrap_or_default();
    ErrDto::new(
        jv.get("code").and_then(|c| c.as_str()).unwrap_or("tab"),
        e.to_string(),
    )
}
```

Replace the inline error mapping inside `edit_user_tabs` (the `edit(&mut doc.value).map_err(|e| { ... })?;` block) with:

```rust
        edit(&mut doc.value).map_err(tab_err)?;
```

Add the two cross-file commands after `tab_create`:

```rust
/// Add an overview window: append the grouping (+ a cloned tab) in the user file,
/// then mint the paired `overview_N` geometry in the char file. The char write is
/// best-effort — skipped when no character is open or it is read-only; EVE
/// self-heals the window at default geometry on that character's next login.
pub fn overview_window_add(state: &AppState, name: String, from_tab: Option<i64>) -> Result<OverviewColumns, ErrDto> {
    let new_window_idx = {
        let mut guard = state.user.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        let idx = add_overview_window(&mut doc.value, &name, from_tab).map_err(tab_err)?;
        doc.value = blue_marshal::reshare(&doc.value);
        idx
    };
    {
        let mut guard = state.char.lock().unwrap();
        if let Some(doc) = guard.as_mut() {
            if !matches!(doc.fidelity, Fidelity::ReadOnly { .. }) {
                add_overview_window_geometry(&mut doc.value, new_window_idx);
                doc.value = blue_marshal::reshare(&doc.value);
            }
        }
    }
    overview_columns(state)
}

/// Remove the last overview window: drop the grouping in the user file and the
/// paired `overview_N` geometry in the char file (best-effort, as above).
pub fn overview_window_remove(state: &AppState, window_idx: usize) -> Result<OverviewColumns, ErrDto> {
    {
        let mut guard = state.user.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        remove_overview_window(&mut doc.value, window_idx).map_err(tab_err)?;
        doc.value = blue_marshal::reshare(&doc.value);
    }
    {
        let mut guard = state.char.lock().unwrap();
        if let Some(doc) = guard.as_mut() {
            if !matches!(doc.fidelity, Fidelity::ReadOnly { .. }) {
                remove_overview_window_geometry(&mut doc.value, window_idx);
                doc.value = blue_marshal::reshare(&doc.value);
            }
        }
    }
    overview_columns(state)
}
```

In `app/src-tauri/src/lib.rs`, add two `#[tauri::command]` wrappers after the `tab_move` command (line ~178):

```rust
#[tauri::command]
fn overview_window_add(state: tauri::State<'_, AppState>, name: String, from_tab: Option<i64>) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::overview_window_add(&state, name, from_tab)
}
#[tauri::command]
fn overview_window_remove(state: tauri::State<'_, AppState>, window_idx: usize) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::overview_window_remove(&state, window_idx)
}
```

And add both names to the `tauri::generate_handler![...]` list, on the line with the `tab_*` commands:

```rust
            tab_create, tab_rename, tab_delete, tab_reorder, tab_move,
            overview_window_add, overview_window_remove,
```

In `app/src/lib/api.ts`, add two bindings after `tabMove` (line ~274):

```ts
  overviewWindowAdd: (name: string, fromTab: number | null) =>
    invoke<OverviewColumns>("overview_window_add", { name, fromTab }),
  overviewWindowRemove: (windowIdx: number) =>
    invoke<OverviewColumns>("overview_window_remove", { windowIdx }),
```

- [ ] **Step 4: Run tests + typecheck**

Run: `cargo test -p app` — Expected: PASS (new test + existing).
Run (PowerShell, in `app/`): `npm run check` — Expected: 0 errors.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Wire overview window add/remove through ops, Tauri, and the api layer"
```

---

### Task 5: Frontend — add / remove window controls in `OverviewView.svelte`

Add "New window" and "Remove window" controls in the existing tab-actions bar. DOM interaction is validated by manual smoke (project norm), not unit tests.

**Files:**
- Modify: `app/src/lib/OverviewView.svelte`

**Interfaces:**
- Consumes: `api.overviewWindowAdd` / `api.overviewWindowRemove` (Task 4); the existing `data`, `tabIndex`, `currentWindow`, `onUserDirty`, and the file's `message` / `confirm` / `errMessage` imports.

- [ ] **Step 1: Add the two handlers**

In the `<script>` block, after the existing `moveTab` function (line ~74), add:

```ts
  async function addWindow() {
    if (!data || data.windows.length === 0) return;
    const name = window.prompt("Name for the new overview window's first tab:", "Overview");
    if (!name?.trim()) return;
    try {
      data = await api.overviewWindowAdd(name.trim(), tabIndex);
      // Select the newly created tab (highest index) so the new window shows.
      tabIndex = Math.max(...data.tabs.map((t) => t.index));
      onUserDirty();
    } catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
  async function removeWindow() {
    if (!data || data.windows.length <= 1 || !currentWindow) return;
    const ok = await confirm(
      `Remove Overview ${currentWindow.index + 1}? Its tabs move to Overview 1.`,
      { title: "Remove overview window", kind: "warning" },
    );
    if (!ok) return;
    try {
      data = await api.overviewWindowRemove(currentWindow.index);
      tabIndex = data.tabs[0]?.index ?? null;
      onUserDirty();
    } catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
```

- [ ] **Step 2: Add the controls to the tab-actions bar**

In the `<div class="tab-actions">` block, after the closing `{/if}` of the "Move to window" `<select>` (line ~169, just before `</div>`), add:

```svelte
      {#if data.windows.length >= 1}
        <button onclick={addWindow} title="Add a new overview window">+ Window</button>
      {/if}
      {#if currentWindow && data.windows.length > 1 && currentWindow.index === data.windows.length - 1}
        <button class="danger" onclick={removeWindow} title="Remove this (last) overview window">Remove Window</button>
      {/if}
```

(The `+ Window` button appears only when the account already has a window layout — positional add needs a base mapping, matching the backend `NoWindowMapping` guard. `Remove Window` appears only on the **last** window, enforcing the last-window-only rule in the UI. Both reuse existing `.danger` / button styles — no new native controls, so no dark-styling gotcha.)

- [ ] **Step 3: Typecheck**

Run (PowerShell, in `app/`): `npm run check`
Expected: 0 errors.

- [ ] **Step 4: Manual smoke (record result)**

Run the app (`npm run tauri dev` in `app/`, via PowerShell), open a real account file **and** its character, then:
1. Click **+ Window** — a new "Overview 2" appears in the tab selector with its seeded tab; the account is Editable.
2. Open the **Layout** view — confirm the new `overview_1` window is present and positionable (drag/resize) without having launched EVE.
3. Save, reopen the account file — confirm the second window and its tab persisted.
4. Select the last window, click **Remove Window** — confirm it's gone and its tab moved to Overview 1; save and reopen to confirm.
5. If a real EVE client is available: load the saved files in-game and confirm EVE renders a real, usable second overview window (this is the milestone's true validation gate).

Note the outcome in the commit message.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/OverviewView.svelte
git commit -m "Add overview window add/remove controls to the Overview view"
```

---

## Verification (end of Phase B)

- [ ] `cargo test --workspace` — all green.
- [ ] `npm run check` in `app/` — 0 errors.
- [ ] Manual smoke recorded (Task 5 Step 4), including the in-game check if a client is available.

## Scope / deferrals

- **Remove is last-window-only.** Middle-window removal (re-key cascade across the char `windows` subdicts, or window-reorder-then-remove) is deferred — logged in `docs/small-tasks.md`.
- **Windowless-account add** is refused (`NoWindowMapping`); adding a window there needs the char-side window count, out of scope.
- Char geometry write is **best-effort** (skipped when no char is open or it's read-only); EVE self-heals the window at default geometry on next login.
