# M3c Overview Editor — v2 Rebuild Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the overview-columns model on the *real* file structure (spec v2) so the editor works on both modern (`tabsettings_new`) and legacy/preset-pack (`tabsettings`) files.

**Architecture:** The read/edit model in `settings-model::overview` is reworked to resolve `Ref`/`Shared` in **keys and values** (reusing `windows.rs`'s shared-slot table), fall back a tab's columns to **its preset** (not a nonexistent account set), group tabs by window via `tabsByWindowInstanceID`, and handle both tab-container keys. The two-slot app state, loading/slot-reconcile, per-slot save, the overview commands, and the width half are already correct and kept; only the visibility/order projection + edit and the frontend tab selector change.

**Tech Stack:** Rust (`settings-model`, `blue-marshal`, Tauri app crate), TypeScript + Svelte 5, `cargo test`, `node --test`.

## Global Constraints

- Add **no** new crates or npm packages.
- **All EVE format knowledge stays in `settings-model::overview`.** The frontend never reasons about `Ref`/`Shared`, wrappers, or key names.
- **`Ref`/`Shared` resolution is mandatory for both keys and values** — real files store the same key as `Bytes`, `Shared`, or `Ref(slot)` interchangeably across client writes. Reuse `windows.rs`'s proven shared-slot approach (`collect_shared` + `effective`).
- **A tab's columns** = its own `tabColumns`/`tabColumnOrder` if present, else its **preset**'s `overviewColumns` (`overview` field → `overviewProfilePresets[name]`). Editing **materializes the tab's own lists from the preset**, per-tab; the preset and other tabs are never touched.
- **Widths** are per-tab, char file `SortHeadersSizes → (overviewScroll2, tabIndex) → {token: px}`. This half is already correct — do not change its semantics.
- Tab indices are **global sequential** — never infer the window from the index; use `tabsByWindowInstanceID`.
- Commit convention: sentence-case subject, **no** attribution/`Co-Authored-By` trailers.
- **Merge is gated on live smoke** against a real char/user pair (both formats). Unit tests use `Value` trees that mirror **real idioms** (`Ref`/`Shared` keys+values, string-table `name` keys, `(ts,list)` master list, preset fallback, `tabsByWindowInstanceID`), never the old clean synthetic shapes.

---

## Current state (what's already in the tree, at branch tip)

- `settings-model::overview` (v1): per-tab projection + edits + width read. Has the read-robustness fixes (`token`/`str_field`/`list_field`/`key_is`/`as_list` handle string-table keys, `(ts,list)` wrappers, `Shared` **values**). **Still wrong:** falls back to a nonexistent account column set instead of the preset; matches keys only as bare `Bytes` (fails on `Ref`/`Shared` keys); only reads `tabsettings_new`; no window grouping.
- `windows.rs`: has private `type Shared`, `collect_shared`, `effective`, `decode_id` — the proven `Ref`/`Shared` resolution to reuse.
- Two-slot `AppState`, slot-param commands, overview commands (`overview_columns`/`set_overview_visible`/`set_overview_order`/`set_overview_width`), loading + slot-reconcile, save-both, `OverviewView` + tab/char selectors + materialize note: **all present and kept** (only the projection shape and tab-selector grouping change).

## File Structure

**Modified:**
- `crates/settings-model/src/treewalk.rs` — gains the shared-slot table + `Ref`/`Shared`-resolving lookups (lifted from `windows.rs`).
- `crates/settings-model/src/windows.rs` — imports the lifted helpers instead of its private copies.
- `crates/settings-model/src/overview.rs` — reworked read (Ref-aware, preset fallback, both tab-keys, window grouping) + edit (materialize-from-preset).
- `crates/settings-model/src/lib.rs` — export any new public projection types.
- `app/src/lib/api.ts` — `OverviewColumns` gains window grouping.
- `app/src/lib/OverviewView.svelte` — tab selector grouped by window.
- `app/src-tauri/src/ops.rs` — only if the overview command return type changes (it re-projects, so likely just type flow).

---

## Phase R — settings-model model rebuild

### Task R1: Lift the shared-slot resolver into `treewalk`

Pure refactor: move `windows.rs`'s `Ref`/`Shared` resolution into `treewalk` so `overview.rs` reuses it. `windows.rs`'s tests are the safety net.

**Files:** Modify `treewalk.rs`, `windows.rs`, `lib.rs` (register nothing new — `treewalk` already a module).

**Interfaces produced (all `pub(crate)`):**
- `type SharedTable<'a> = std::collections::HashMap<u32, &'a blue_marshal::Value>`
- `fn collect_shared<'a>(v: &'a Value, out: &mut SharedTable<'a>)`
- `fn effective<'a>(v: &'a Value, shared: &SharedTable<'a>) -> &'a Value` — follows `Shared`/`Ref` (bounded 64 hops).

- [ ] **Step 1: Move the code.** Cut `windows.rs`'s private `type Shared<'a>`, `collect_shared`, and `effective` (currently near the `collect_shared`/`effective` block) into `treewalk.rs`, renaming the type to `SharedTable` and marking all three `pub(crate)`. Keep the bodies byte-identical (only the type name and visibility change).

- [ ] **Step 2: Migrate `windows.rs`.** In `windows.rs`, delete those three private items; add `use crate::treewalk::{collect_shared, effective, SharedTable};` and replace its local `Shared` type alias uses with `SharedTable`. `decode_id`, geom/flag logic stay.

- [ ] **Step 3: Verify no regression.**
Run: `cargo test --manifest-path crates/settings-model/Cargo.toml windows::`
Expected: all `windows::` tests pass unchanged.

- [ ] **Step 4: Commit**
```bash
git add crates/settings-model/src/treewalk.rs crates/settings-model/src/windows.rs
git commit -m "Lift the shared-slot Ref resolver into treewalk for reuse"
```

### Task R2: Ref/Shared-aware overview lookups

Make `overview.rs` resolve `Ref`/`Shared` in **keys and values**, driven by one shared table built at the projection entry. This replaces the bare-`Bytes`-key matching that fails when the client stores the `overview` container (and other keys) as `Ref`/`Shared`.

**Files:** Modify `overview.rs`. **Test:** in `overview.rs`.

**Interfaces produced (module-private):**
- `fn find_child<'a>(dict: &'a Entries, name: &[u8], sh: &SharedTable<'a>) -> Option<&'a Value>` — the value of the entry whose **resolved** key is `Bytes(name)`, itself resolved.
- `fn as_dict<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<&'a Entries>` / `fn as_list<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<&'a Vec<Value>>` — resolve then match, unwrapping the `(timestamp, X)` wrapper.
- `fn token_r(v: &Value, sh: &SharedTable) -> Option<String>` — resolved column token.

- [ ] **Step 1: Write the failing test** (add to `overview.rs` tests; import `blue_marshal::Value`):

```rust
#[test]
fn find_child_resolves_ref_and_shared_keys() {
    use blue_marshal::Value;
    // A dict whose "overview" key is a Ref to a Shared("overview") elsewhere.
    let doc = Value::Dict(vec![
        (Value::Shared { slot: 5, value: Box::new(Value::Bytes(b"overview".to_vec())) },
         Value::Dict(vec![(Value::Bytes(b"x".to_vec()), Value::Int(1))])),
        (Value::Ref(5), Value::Dict(vec![(Value::Bytes(b"y".to_vec()), Value::Int(2))])),
    ]);
    let mut sh = SharedTable::new();
    collect_shared(&doc, &mut sh);
    let Value::Dict(entries) = &doc else { unreachable!() };
    // Both entries resolve to key "overview"; find_child returns the FIRST.
    let got = find_child(entries, b"overview", &sh).and_then(|v| as_dict(v, &sh));
    assert!(got.is_some(), "a Shared-keyed child is found");
}

#[test]
fn token_r_resolves_ref_tokens() {
    use blue_marshal::Value;
    let doc = Value::List(vec![
        Value::Shared { slot: 9, value: Box::new(Value::Bytes(b"NAME".to_vec())) },
        Value::Ref(9),
    ]);
    let mut sh = SharedTable::new();
    collect_shared(&doc, &mut sh);
    let Value::List(items) = &doc else { unreachable!() };
    assert_eq!(token_r(&items[0], &sh).as_deref(), Some("NAME"));
    assert_eq!(token_r(&items[1], &sh).as_deref(), Some("NAME"), "a Ref token resolves");
}
```

- [ ] **Step 2: Run — expect FAIL** (`find_child`/`token_r`/`SharedTable` undefined).
Run: `cargo test --manifest-path crates/settings-model/Cargo.toml overview::`

- [ ] **Step 3: Implement.** Add to `overview.rs`:

```rust
use crate::treewalk::{collect_shared, effective, SharedTable};

/// Value of the entry whose resolved key is `Bytes(name)`, itself resolved.
fn find_child<'a>(dict: &'a Entries, name: &[u8], sh: &SharedTable<'a>) -> Option<&'a Value> {
    dict.iter()
        .find(|(k, _)| matches!(effective(k, sh), Value::Bytes(b) if b.as_slice() == name))
        .map(|(_, v)| effective(v, sh))
}

/// Resolve to a dict, unwrapping a `(timestamp, dict)` wrapper.
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

/// Resolve to a list, unwrapping a `(timestamp, list)` wrapper.
fn as_list_r<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<&'a Vec<Value>> {
    match effective(v, sh) {
        Value::List(l) => Some(l),
        Value::Tuple(items) => items.iter().find_map(|e| match effective(e, sh) {
            Value::List(l) => Some(l),
            _ => None,
        }),
        _ => None,
    }
}

fn token_r(v: &Value, sh: &SharedTable) -> Option<String> {
    match effective(v, sh) {
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        _ => None,
    }
}

/// Resolved list-of-tokens for a byte-named field within `dict`.
fn token_list(dict: &Entries, name: &[u8], sh: &SharedTable) -> Option<Vec<String>> {
    let v = find_child(dict, name, sh)?;
    Some(as_list_r(v, sh)?.iter().filter_map(|t| token_r(t, sh)).collect())
}
```

- [ ] **Step 4: Run — expect PASS.**
Run: `cargo test --manifest-path crates/settings-model/Cargo.toml overview::`

- [ ] **Step 5: Commit**
```bash
git add crates/settings-model/src/overview.rs
git commit -m "Add Ref/Shared-aware overview lookups (keys and values)"
```

### Task R3: Rebuild the read projection (preset fallback, both tab-keys, window grouping)

Replace `project_overview`/`project_tab`/`account_defaults`/`tab_settings` with a shared-table-driven read: locate the overview container by resolved key, read tabs from `tabsettings_new` **or** `tabsettings`, fall a tab's columns back to **its preset**, and group tabs by window via `tabsByWindowInstanceID`.

**Files:** Modify `overview.rs`, `lib.rs` (export the new struct fields). **Test:** in `overview.rs`.

**Interfaces produced:**
- `OverviewColumns { pub windows: Vec<OverviewWindow>, pub tabs: Vec<OverviewTab> }` — `windows[i].tab_indices` lists that window's tab indices in order (from `tabsByWindowInstanceID`); `tabs` is every tab (projected once). `OverviewWindow { pub index: usize, pub tab_indices: Vec<i64> }`.
- `OverviewTab` unchanged shape (`index, name, inherits, columns`), but `inherits`/columns now come from the preset fallback.
- `fn project_overview(user: &Value, char_tree: Option<&Value>) -> OverviewColumns` — same signature.

- [ ] **Step 1: Write the failing test.** Build a realistic user tree: `overview` container keyed by a `Ref`; `tabsettings_new` with tab 0 referencing preset "P" (no own lists) and tab 1 with its own `tabColumns`; `overviewProfilePresets["P"].overviewColumns = [NAME, TYPE]`; `tabsByWindowInstanceID = [[0],[1]]`.

```rust
#[test]
fn projects_preset_fallback_and_window_grouping() {
    use blue_marshal::Value;
    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
    fn ts() -> Value { Value::Long(vec![0u8; 8]) }
    // preset P: visible [NAME, TYPE]
    let preset = Value::Dict(vec![(b("overviewColumns"), Value::List(vec![b("NAME"), b("TYPE")]))]);
    let presets = Value::Dict(vec![(b("P"), preset)]);
    let tab0 = Value::Dict(vec![
        (Value::StrTable(52), Value::Str("Alpha".into())),      // name (string-table key)
        (b("overview"), b("P")),                                // references preset P (no own lists)
    ]);
    let tab1 = Value::Dict(vec![
        (Value::StrTable(52), Value::Str("Beta".into())),
        (b("tabColumnOrder"), Value::List(vec![b("NAME"), b("TYPE"), b("DISTANCE")])),
        (b("tabColumns"), Value::List(vec![b("DISTANCE")])),
    ]);
    let tabs = Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab0), (Value::Int(1), tab1)])]);
    let overview = Value::Dict(vec![
        (b("overviewProfilePresets"), presets),
        (b("tabsettings_new"), tabs),
        (b("tabsByWindowInstanceID"), Value::List(vec![
            Value::List(vec![Value::Int(0)]),
            Value::List(vec![Value::Int(1)]),
        ])),
    ]);
    // The overview container's KEY is a Ref to a Shared("overview").
    let user = Value::Dict(vec![
        (Value::Shared { slot: 1, value: Box::new(b("overview")) }, overview),
    ]);

    let oc = project_overview(&user, None);
    // window grouping
    assert_eq!(oc.windows.len(), 2);
    assert_eq!(oc.windows[0].tab_indices, vec![0]);
    assert_eq!(oc.windows[1].tab_indices, vec![1]);
    // tab 0 inherits preset P -> [NAME(hidden? no, preset visible), TYPE]
    let t0 = oc.tabs.iter().find(|t| t.index == 0).unwrap();
    assert!(t0.inherits, "tab 0 has no own lists");
    assert_eq!(t0.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["NAME", "TYPE"]);
    assert!(t0.columns.iter().all(|c| c.visible), "preset columns are the visible set");
    // tab 1 owns its lists
    let t1 = oc.tabs.iter().find(|t| t.index == 1).unwrap();
    assert!(!t1.inherits);
    assert_eq!(t1.columns.iter().filter(|c| c.visible).map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["DISTANCE"]);
    assert_eq!(t1.columns.len(), 3, "order list of 3");
}
```

- [ ] **Step 2: Run — expect FAIL.**

- [ ] **Step 3: Implement.** Rewrite the read half of `overview.rs`. Full shape:

```rust
#[derive(Debug, Serialize, PartialEq)]
pub struct OverviewColumns {
    pub windows: Vec<OverviewWindow>,
    pub tabs: Vec<OverviewTab>,
}
#[derive(Debug, Serialize, PartialEq)]
pub struct OverviewWindow {
    pub index: usize,
    pub tab_indices: Vec<i64>,
}
// OverviewTab / OverviewColumn unchanged.

pub fn project_overview(user: &Value, char_tree: Option<&Value>) -> OverviewColumns {
    let mut sh = SharedTable::new();
    collect_shared(user, &mut sh);
    let empty = OverviewColumns { windows: vec![], tabs: vec![] };
    let Some(overview) = overview_container(user, &sh) else { return empty };

    let windows = window_groups(overview, &sh);
    let tabs = tab_dict(overview, &sh)
        .map(|d| d.iter().filter_map(|(k, v)| project_tab(k, v, overview, char_tree, &sh)).collect())
        .unwrap_or_default();
    OverviewColumns { windows, tabs }
}

/// The `overview` container dict (key resolved through Ref/Shared).
fn overview_container<'a>(user: &'a Value, sh: &SharedTable<'a>) -> Option<&'a Entries> {
    let Value::Dict(root) = effective(user, sh) else { return None };
    find_child(root, b"overview", sh).and_then(|v| as_dict(v, sh))
}

/// The tab dict from `tabsettings_new` (modern) or `tabsettings` (legacy).
fn tab_dict<'a>(overview: &'a Entries, sh: &SharedTable<'a>) -> Option<&'a Entries> {
    for key in [b"tabsettings_new".as_slice(), b"tabsettings"] {
        if let Some(v) = find_child(overview, key, sh) {
            if let Some(d) = as_dict(v, sh) {
                return Some(d);
            }
        }
    }
    None
}

/// Window groups from tabsByWindowInstanceID (list of lists of tab indices).
fn window_groups(overview: &Entries, sh: &SharedTable) -> Vec<OverviewWindow> {
    let Some(v) = find_child(overview, b"tabsByWindowInstanceID", sh) else { return vec![] };
    let Some(outer) = as_list_r(v, sh) else { return vec![] };
    outer.iter().enumerate().filter_map(|(i, inner)| {
        let list = as_list_r(inner, sh)?;
        let tab_indices = list.iter().filter_map(|e| as_int(effective(e, sh))).collect();
        Some(OverviewWindow { index: i, tab_indices })
    }).collect()
}

fn project_tab(key: &Value, tab: &Value, overview: &Entries, char_tree: Option<&Value>, sh: &SharedTable) -> Option<OverviewTab> {
    let index = as_int(effective(key, sh))?;
    let fields = as_dict(tab, sh)?;
    let name = str_field_r(fields, "name", sh).unwrap_or_else(|| format!("Tab {index}"));

    let own_order = token_list(fields, b"tabColumnOrder", sh);
    let own_visible = token_list(fields, b"tabColumns", sh);
    let inherits = own_order.is_none() || own_visible.is_none();
    let (def_order, def_visible) = preset_columns(fields, overview, sh); // FALLBACK = the tab's preset
    let order = own_order.unwrap_or(def_order);
    let visible = own_visible.unwrap_or(def_visible);
    let widths = char_tree.and_then(|c| tab_widths(c, index));

    let mut ordered = order.clone();
    for tok in &visible { if !ordered.contains(tok) { ordered.push(tok.clone()); } }
    let columns = ordered.iter().map(|tok| OverviewColumn {
        label: prettify(tok),
        visible: visible.iter().any(|v| v == tok),
        width: widths.as_ref().and_then(|w| w.get(tok).copied()),
        name: tok.clone(),
    }).collect();
    Some(OverviewTab { index, name, inherits, columns })
}

/// The tab's preset columns: resolve the tab's `overview` field (preset name) to
/// overviewProfilePresets[name].overviewColumns. Order == visible == that list.
fn preset_columns(tab: &Entries, overview: &Entries, sh: &SharedTable) -> (Vec<String>, Vec<String>) {
    let preset_name = find_child(tab, b"overview", sh).and_then(|v| token_r(v, sh));
    let cols = preset_name.and_then(|name| {
        let presets = find_child(overview, b"overviewProfilePresets", sh).and_then(|v| as_dict(v, sh))?;
        let preset = find_child(presets, name.as_bytes(), sh).and_then(|v| as_dict(v, sh))?;
        token_list(preset, b"overviewColumns", sh)
    }).unwrap_or_default();
    (cols.clone(), cols)
}
```

Also add `str_field_r` (string-table `name` key + resolved value) mirroring the current `str_field`/`key_is` but taking `sh` and using `effective` on the value. Delete the now-unused `account_defaults`, `tab_settings`, `child_dict`-based readers, and any `list_field`/`token`/`as_list` superseded by the `_r` versions (keep `is_width_key`, `as_int`, `prettify`, `tab_widths` — update `tab_widths` to use `find_child`/`token_r`/`sh`). Update `lib.rs` exports to add `OverviewWindow`.

- [ ] **Step 4: Run — expect PASS** (the new test + keep any still-valid old tests; delete tests that asserted the old account-fallback behavior and replace with preset-fallback equivalents).
Run: `cargo test --manifest-path crates/settings-model/Cargo.toml overview::`

- [ ] **Step 5: Commit**
```bash
git add crates/settings-model/src/overview.rs crates/settings-model/src/lib.rs
git commit -m "Rebuild overview read: preset fallback, both tab-keys, window grouping"
```

### Task R4: Rebuild the edit path (materialize from the preset)

Update `set_column_visible`/`set_column_order` so a tab with no own lists **materializes them from its preset** (not the old account defaults) before editing; `set_column_width` keeps its `(overviewScroll2, tabIndex)` target. Editing stays per-tab.

**Files:** Modify `overview.rs`. **Test:** in `overview.rs`.

**Interfaces:** signatures unchanged (`set_column_visible(user, tab_index, column, visible)`, `set_column_order(user, tab_index, order)`, `set_column_width(char_tree, tab_index, column, width)`).

- [ ] **Step 1: Write the failing test.** Editing an inheriting tab (references preset "P" with visible `[NAME, TYPE]`) to show "DISTANCE" must materialize the tab's own `tabColumnOrder`/`tabColumns` from the preset set, then add DISTANCE — leaving `overviewProfilePresets["P"]` unchanged.

```rust
#[test]
fn editing_inheriting_tab_materializes_from_preset_not_account() {
    use blue_marshal::Value;
    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
    fn ts() -> Value { Value::Long(vec![0u8; 8]) }
    let preset = Value::Dict(vec![(b("overviewColumns"), Value::List(vec![b("NAME"), b("TYPE")]))]);
    let tab0 = Value::Dict(vec![(b("overview"), b("P"))]);
    let overview = Value::Dict(vec![
        (b("overviewProfilePresets"), Value::Dict(vec![(b("P"), preset)])),
        (b("tabsettings_new"), Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab0)])])),
    ]);
    let mut user = Value::Dict(vec![(b("overview"), overview)]);

    set_column_visible(&mut user, 0, "DISTANCE", true).unwrap();

    let oc = project_overview(&user, None);
    let t0 = oc.tabs.iter().find(|t| t.index == 0).unwrap();
    assert!(!t0.inherits, "tab now owns its lists (materialized)");
    let visible: Vec<_> = t0.columns.iter().filter(|c| c.visible).map(|c| c.name.clone()).collect();
    assert!(visible.contains(&"DISTANCE".to_string()));
    assert!(visible.contains(&"NAME".to_string()) && visible.contains(&"TYPE".to_string()),
        "preset's visible columns carried into the materialized tab");
    // preset untouched
    let oc2 = project_overview(&user, None);
    assert_eq!(oc2.tabs.iter().find(|t| t.index==0).map(|t| t.columns.len()), t0.columns.len().into());
}
```

- [ ] **Step 2: Run — expect FAIL.**

- [ ] **Step 3: Implement.** Change the materialize source. The current `set_column_visible`/`set_column_order` call `account_defaults(user)`; replace that with a `preset_columns_for_tab(user, tab_index)` that resolves the same shared table + preset fallback the read uses, returning the effective `(order, visible)` for the tab to seed materialization. Reach the mutable tab dict by index via the existing `with_tab`/`tab_dict_path` (which already threads `Step::SharedInner`; extend it to locate `tabsettings_new` OR `tabsettings` and the overview container by resolved key — note: `resolve_mut` needs concrete indices, so build the path from the *resolved* container/tab positions). Materialize writes bare `Bytes` token lists (`tabColumnOrder`, `tabColumns`), then toggles/reorders as today. Keep `set_column_width` unchanged except confirm its width-dict key match tolerates `Ref`/`Shared` keys (use `effective` in `is_width_key`).

- [ ] **Step 4: Run — expect PASS** (this test + all overview tests).
Run: `cargo test --manifest-path crates/settings-model/Cargo.toml overview::`

- [ ] **Step 5: Commit**
```bash
git add crates/settings-model/src/overview.rs
git commit -m "Materialize a tab's columns from its preset on first edit"
```

### Task R5: Real-file corpus guard

Add a test that decodes a **committed, non-personal** fixture mirroring a real file (both a `tabsettings_new` and a `tabsettings` shape, with `Ref`/`Shared` keys) and asserts `project_overview` returns non-empty tabs with resolved names and preset-filled columns — so a real-idiom regression fails CI, not just live smoke.

**Files:** Create `crates/settings-model/tests/overview_realshape.rs`; create small hand-authored fixture bytes via `blue_marshal::encode` in the test itself (no personal data on disk).

- [ ] **Step 1: Write the test** encoding two user-shaped `Value` trees (modern + legacy tab-key) with `Ref`/`Shared` keys and a preset, then `decode` + `project_overview`, asserting: container found through the `Ref` key, tabs present, names resolved, an inheriting tab filled from its preset, window groups from `tabsByWindowInstanceID`.
- [ ] **Step 2: Run — PASS.** `cargo test --manifest-path crates/settings-model/Cargo.toml --test overview_realshape`
- [ ] **Step 3: Commit** `git commit -m "Guard overview projection against real-file idioms (both tab-keys)"`

---

## Phase F — frontend

### Task F1: Window grouping in the tab selector

**Files:** Modify `app/src/lib/api.ts` (add `windows` to `OverviewColumns`, `OverviewWindow` type), `app/src/lib/OverviewView.svelte` (group the tab `<select>` by window using `data.windows`; each window's `tab_indices` in order, labeled "Overview N"). Keep the char selector, checkbox/drag/width rows, and the inherits note.

- [ ] **Step 1:** Add types: `export interface OverviewWindow { index: number; tab_indices: number[]; }` and `windows: OverviewWindow[]` on `OverviewColumns`.
- [ ] **Step 2:** In `OverviewView.svelte`, render the tab selector as `<optgroup label="Overview {w.index+1}">` per window (map each `tab_index` to its tab's name via `data.tabs`), falling back to a flat list if `windows` is empty. The rest of the component is unchanged.
- [ ] **Step 3: Verify.** `npm --prefix app run check` (0 errors) + `npm --prefix app run build` (succeeds). Do NOT launch the GUI here.
- [ ] **Step 4: Commit** `git commit -m "Group the overview tab selector by window"`

---

## Phase V — verification

### Task V1: Full suites + live smoke (MERGE GATE)

- [ ] **Step 1: Automated.** `cargo test --manifest-path crates/settings-model/Cargo.toml`; `cargo test --manifest-path app/src-tauri/Cargo.toml`; `node --test app/src/lib/*.test.ts`; `npm --prefix app run check`; `npm --prefix app run build` — all green.
- [ ] **Step 2: Live smoke on real files (the gate).** Run the app. For a **modern** character (e.g. 2124209999 / user 32945923, paired in Accounts): open it → Overview tab → tabs appear grouped by window, real names, columns from each tab's preset; toggle/reorder a column (materializes) + set a width; Save; reopen both files; confirm persisted **and that the EVE client still reads them correctly** (open EVE, check the overview). Repeat for a **legacy/preset-pack** character. Confirm an unpaired character shows the Accounts nudge.
- [ ] **Step 3:** Only after live smoke passes on both formats, proceed to finishing-a-development-branch.

---

## Self-Review

**Spec coverage:** §2 model → R2/R3/R4 (Ref keys+values, both tab-keys, preset fallback, window map, widths kept); §3 semantics (own-or-preset, materialize per-tab) → R3/R4; §4 two-slot/loading → kept (no task, already landed); §5 editor → F1 (+ kept selectors/rows); §6 rebuild → R1–R4; §7 scope (preset editor deferred) → not built; §8 testing (real idioms) → R2/R3/R4/R5; §9 risks (legacy container via resolved key R3; write-side Ref R4; re-validate V1) ✓; §10 lessons → R5 + V1 gate.

**Placeholder scan:** code shown for every model step; F1/edit-path steps describe concrete changes to existing named functions. No TBD.

**Type consistency:** `SharedTable`/`collect_shared`/`effective` names match R1↔R2↔R3; `find_child`/`as_dict`/`as_list_r`/`token_r`/`token_list` used consistently R2→R3→R4; `OverviewColumns.windows`/`OverviewWindow` match R3↔lib.rs↔api.ts (F1); command signatures unchanged so `ops.rs`/api overview bindings still line up.
