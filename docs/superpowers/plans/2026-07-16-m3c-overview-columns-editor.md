# M3c — Overview Columns Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A per-tab overview-columns editor (show/hide, drag-reorder, per-column width) that reads/writes both the account's `core_user` file and a chosen character's `core_char` file.

**Architecture:** A pure `settings-model::overview` module (projection + edit functions, mirroring `windows.rs`) carries all format knowledge and is unit-tested without Tauri. The Tauri `AppState` gains two typed document slots (`char`, `user`), each keeping the existing independent save chain. A new Svelte `OverviewView` drives high-level overview commands; the char↔user loading is resolved on the frontend from the M3b roster + discovered profiles.

**Tech Stack:** Rust (settings-model crate, Tauri app crate), TypeScript + Svelte 5 (runes), `node --test` for pure TS, `cargo test` for Rust.

## Global Constraints

- **Dependency-free:** add **no** new crates or npm packages. (Spec/repo convention.)
- **Silent degradation:** a file with no overview data projects to empty tabs, never an error.
- **Two-file, independent saves:** the char slot and user slot save through the **existing** M1 chain separately; there is no coordinated write.
- **Format knowledge stays in `settings-model`:** the frontend never reconstructs a `NodePath` or reasons about `(FILETIME, value)` wrappers.
- **Commit convention:** sentence-case subject, **no** attribution/`Co-Authored-By` trailers.
- **Tests:** every Rust logic change is TDD (failing test first). Pure TS logic gets a `node --test` test. Svelte components are verified by running the app (no component test harness in this repo).
- **Column tokens** are `Value::Bytes` (e.g. `b"TRANSVERSALVELOCITY"`); tab-index keys are `Value::Int`; the width dict is keyed by the tuple `(b"overviewScroll2", tabIndex)`.

---

## File Structure

**Created:**
- `crates/settings-model/src/treewalk.rs` — shared dict tree-walk helpers (extracted from `windows.rs`; reused by `overview.rs`).
- `crates/settings-model/src/overview.rs` — overview projection + edit functions (all format knowledge).
- `app/src/lib/OverviewView.svelte` — the editor UI.
- `app/src/lib/overview.ts` — pure frontend helpers (roster/profile lookups for loading).
- `app/src/lib/overview.test.ts` — tests for `overview.ts`.

**Modified:**
- `crates/settings-model/src/windows.rs` — import the tree-walk helpers from `treewalk` instead of defining them privately.
- `crates/settings-model/src/lib.rs` — register `treewalk`; export the overview module’s public items.
- `app/src-tauri/src/ops.rs` — two-slot `AppState`, `Slot` enum, slot-param command fns, overview command fns.
- `app/src-tauri/src/lib.rs` — command wrappers gain `slot`; register overview commands.
- `app/src/lib/api.ts` — `slot` arg on doc commands; overview command bindings + types.
- `app/src/routes/+page.svelte` — two-slot state, slot switcher, loading flow, Overview view, cross-slot save/guard.

---

## Phase A — Overview model (pure, `settings-model`)

No app wiring in this phase; the app is unaffected and keeps working.

### Task A0: Extract shared tree-walk helpers

Pure refactor: lift the four dict-traversal helpers out of `windows.rs` into a
crate-internal `treewalk` module so `overview.rs` reuses them instead of
duplicating. Behavior is unchanged; `windows.rs`'s existing tests are the safety
net (no new test).

**Files:**
- Create: `crates/settings-model/src/treewalk.rs`
- Modify: `crates/settings-model/src/windows.rs` (delete the private copies, import from `treewalk`)
- Modify: `crates/settings-model/src/lib.rs` (add `mod treewalk;`)

**Interfaces:**
- Produces (all `pub(crate)`):
  - `type Entries = Vec<(blue_marshal::Value, blue_marshal::Value)>`
  - `fn is_bytes(v: &Value, name: &[u8]) -> bool`
  - `fn unwrap_shared(v: &Value, path: NodePath) -> (&Value, NodePath)`
  - `fn unwrap_shared_ref(v: &Value) -> &Value`
  - `fn child_dict<'a>(parent: &'a Value, name: &[u8], base: NodePath) -> Option<(&'a Entries, NodePath)>`
  - `fn timestamped_dict<'a>(parent: &'a Entries, base: &NodePath, name: &[u8]) -> Option<(&'a Entries, NodePath)>`

- [ ] **Step 1: Create `treewalk.rs`**

```rust
//! Shared dict-traversal helpers for the typed category projections
//! (windows.rs, overview.rs): find a byte-keyed child dict, unwrap the
//! `(timestamp, dict)` wrappers and `Shared` indirection, all threading the
//! `NodePath` a later mutation targets.

use blue_marshal::Value;

use crate::path::{NodePath, Step};

pub(crate) type Entries = Vec<(Value, Value)>;

pub(crate) fn is_bytes(v: &Value, name: &[u8]) -> bool {
    matches!(v, Value::Bytes(b) if b.as_slice() == name)
}

pub(crate) fn unwrap_shared(v: &Value, mut path: NodePath) -> (&Value, NodePath) {
    if let Value::Shared { value, .. } = v {
        path.push(Step::SharedInner);
        return (value, path);
    }
    (v, path)
}

pub(crate) fn unwrap_shared_ref(v: &Value) -> &Value {
    match v {
        Value::Shared { value, .. } => value,
        other => other,
    }
}

/// `parent` must be a dict; find the entry keyed by the byte-string `name` and
/// return its value as a dict, threading the path (unwrapping one `Shared`).
pub(crate) fn child_dict<'a>(parent: &'a Value, name: &[u8], base: NodePath) -> Option<(&'a Entries, NodePath)> {
    let (parent, base) = unwrap_shared(parent, base);
    let Value::Dict(entries) = parent else { return None };
    let (i, (_, v)) = entries.iter().enumerate().find(|(_, (k, _))| is_bytes(k, name))?;
    let mut p = base;
    p.push(Step::DictValue(i));
    let (v, p) = unwrap_shared(v, p);
    match v {
        Value::Dict(d) => Some((d, p)),
        _ => None,
    }
}

/// Find `name` inside `parent` where the value is the `(timestamp, dict)`
/// wrapper (or, defensively, a bare dict or a `Shared` of either). Returns the
/// inner dict and the path to it.
pub(crate) fn timestamped_dict<'a>(
    parent: &'a Entries,
    base: &NodePath,
    name: &[u8],
) -> Option<(&'a Entries, NodePath)> {
    let (i, (_, v)) = parent.iter().enumerate().find(|(_, (k, _))| is_bytes(k, name))?;
    let mut p = base.clone();
    p.push(Step::DictValue(i));
    let (v, p) = unwrap_shared(v, p);
    match v {
        Value::Dict(d) => Some((d, p)),
        Value::Tuple(items) => {
            let (ti, inner) = items.iter().enumerate().find(|(_, e)| matches!(e, Value::Dict(_)))?;
            let Value::Dict(d) = inner else { return None };
            let mut p2 = p;
            p2.push(Step::Tuple(ti));
            Some((d, p2))
        }
        _ => None,
    }
}
```

- [ ] **Step 2: Migrate `windows.rs`**

In `windows.rs`: delete the private `type Entries`, `child_dict`, `timestamped_dict`, `unwrap_shared`, and `is_bytes` definitions. Add near the top:

```rust
use crate::treewalk::{child_dict, is_bytes, timestamped_dict, unwrap_shared, Entries};
```

Leave everything else (the `Shared`/`Ref` slot resolution, `collect_shared`, `effective`, geom/flag extraction) untouched — those are windows-specific and stay.

- [ ] **Step 3: Register the module**

In `lib.rs`, add with the other `pub mod` lines:

```rust
mod treewalk;
```

- [ ] **Step 4: Verify no regression**

Run: `cargo test --manifest-path crates/settings-model/Cargo.toml windows::`
Expected: PASS — every existing `windows::` test still green (the traversal behavior is identical; only its home moved).

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/treewalk.rs crates/settings-model/src/windows.rs crates/settings-model/src/lib.rs
git commit -m "Extract shared dict tree-walk helpers into a treewalk module"
```

### Task A1: Overview read projection

**Files:**
- Create: `crates/settings-model/src/overview.rs`
- Modify: `crates/settings-model/src/lib.rs` (add `pub mod overview;` + re-exports)
- Test: in `overview.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: `blue_marshal::Value`, `crate::path::{NodePath, Step, resolve}`.
- Produces:
  - `pub struct OverviewColumns { pub tabs: Vec<OverviewTab> }`
  - `pub struct OverviewTab { pub index: i64, pub name: String, pub inherits: bool, pub columns: Vec<OverviewColumn> }`
  - `pub struct OverviewColumn { pub name: String, pub label: String, pub visible: bool, pub width: Option<i64> }`
  - `pub fn project_overview(user: &Value, char_tree: Option<&Value>) -> OverviewColumns`

- [ ] **Step 1: Write the failing test**

Add to a new `crates/settings-model/src/overview.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    fn ts() -> Value { Value::Long(vec![0u8; 8]) }
    fn bytes(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }

    /// user root -> b"overview" -> b"tabsettings_new" -> (ts, { 0: tab })
    /// where the tab has its own name/order/visible lists.
    fn user_with_tab() -> Value {
        let tab = Value::Dict(vec![
            (Value::Str("name".into()), Value::Str("PvP".into())),
            (bytes("tabColumnOrder"), Value::List(vec![bytes("NAME"), bytes("TYPE"), bytes("DISTANCE")])),
            (bytes("tabColumns"), Value::List(vec![bytes("NAME"), bytes("DISTANCE")])),
        ]);
        Value::Dict(vec![(
            bytes("overview"),
            Value::Dict(vec![(
                bytes("tabsettings_new"),
                Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab)])]),
            )]),
        )])
    }

    /// char root -> b"ui" -> b"SortHeadersSizes" -> (ts, { (overviewScroll2, 0): { NAME: 120 } })
    fn char_with_widths() -> Value {
        let widths = Value::Dict(vec![(bytes("NAME"), Value::Int(120))]);
        Value::Dict(vec![(
            bytes("ui"),
            Value::Dict(vec![(
                bytes("SortHeadersSizes"),
                Value::Tuple(vec![
                    ts(),
                    Value::Dict(vec![(
                        Value::Tuple(vec![bytes("overviewScroll2"), Value::Int(0)]),
                        widths,
                    )]),
                ]),
            )]),
        )])
    }

    #[test]
    fn projects_a_tab_with_order_visibility_and_widths() {
        let oc = project_overview(&user_with_tab(), Some(&char_with_widths()));
        assert_eq!(oc.tabs.len(), 1);
        let t = &oc.tabs[0];
        assert_eq!(t.index, 0);
        assert_eq!(t.name, "PvP");
        assert!(!t.inherits, "tab has its own lists");
        // Columns are in tabColumnOrder order.
        let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["NAME", "TYPE", "DISTANCE"]);
        // Visible set is tabColumns; TYPE is not visible.
        assert!(t.columns[0].visible && !t.columns[1].visible && t.columns[2].visible);
        // Width joined from the char tree for NAME only.
        assert_eq!(t.columns[0].width, Some(120));
        assert_eq!(t.columns[1].width, None);
        // Prettified label, raw token preserved.
        assert_eq!(t.columns[0].label, "Name");
    }

    #[test]
    fn a_file_without_overview_projects_empty() {
        let empty = Value::Dict(vec![]);
        assert!(project_overview(&empty, None).tabs.is_empty());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path crates/settings-model/Cargo.toml overview::`
Expected: FAIL to compile (`project_overview` not defined).

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/settings-model/src/overview.rs` (above the test module):

```rust
//! Read + edit projection of the overview-columns category. Visibility and
//! order live in the `core_user` file (per overview tab, with account-default
//! inheritance); widths live in the `core_char` file (per tab). All EVE format
//! knowledge (the `(timestamp, dict)` wrappers, the `(overviewScroll2, tab)`
//! width key, column tokens as Bytes) lives here so the UI stays format-blind.
//! Dict traversal reuses the shared `crate::treewalk` helpers.

use blue_marshal::Value;
use serde::Serialize;

use crate::path::NodePath;
use crate::treewalk::{child_dict, is_bytes, timestamped_dict, unwrap_shared_ref, Entries};

#[derive(Debug, Serialize, PartialEq)]
pub struct OverviewColumns {
    pub tabs: Vec<OverviewTab>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct OverviewTab {
    pub index: i64,
    pub name: String,
    pub inherits: bool,
    pub columns: Vec<OverviewColumn>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct OverviewColumn {
    pub name: String,
    pub label: String,
    pub visible: bool,
    pub width: Option<i64>,
}

pub fn project_overview(user: &Value, char_tree: Option<&Value>) -> OverviewColumns {
    let tabs = tab_settings(user)
        .map(|(dict, _)| dict.iter().filter_map(|(k, v)| project_tab(k, v, user, char_tree)).collect())
        .unwrap_or_default();
    OverviewColumns { tabs }
}

fn project_tab(key: &Value, tab: &Value, user: &Value, char_tree: Option<&Value>) -> Option<OverviewTab> {
    let index = as_int(key)?;
    let Value::Dict(fields) = unwrap_shared_ref(tab) else { return None };
    let name = str_field(fields, "name").unwrap_or_else(|| format!("Tab {index}"));

    let own_order = list_field(fields, b"tabColumnOrder");
    let own_visible = list_field(fields, b"tabColumns");
    let inherits = own_order.is_none() && own_visible.is_none();

    // Effective order/visible: the tab's own lists, else the account defaults.
    let (order, visible) = match (own_order, own_visible) {
        (Some(o), v) => (o, v.unwrap_or_default()),
        (None, Some(v)) => (v.clone(), v),
        (None, None) => account_defaults(user),
    };
    let widths = char_tree.and_then(|c| tab_widths(c, index));

    let columns = order
        .iter()
        .map(|tok| OverviewColumn {
            label: prettify(tok),
            visible: visible.iter().any(|v| v == tok),
            width: widths.as_ref().and_then(|w| w.get(tok).copied()),
            name: tok.clone(),
        })
        .collect();
    Some(OverviewTab { index, name, inherits, columns })
}

/// Account-level defaults: (overviewColumnOrder, overviewColumns) as token lists.
fn account_defaults(user: &Value) -> (Vec<String>, Vec<String>) {
    let Some((ov, _)) = child_dict(user, b"overview", Vec::new()) else { return (vec![], vec![]) };
    let order = list_field(ov, b"overviewColumnOrder").unwrap_or_default();
    let visible = list_field(ov, b"overviewColumns").unwrap_or_default();
    (order, visible)
}

/// Widths for a tab: column token -> px, from char root -> ui -> SortHeadersSizes.
fn tab_widths(char_tree: &Value, tab_index: i64) -> Option<std::collections::HashMap<String, i64>> {
    let (ui, ui_path) = child_dict(char_tree, b"ui", Vec::new())?;
    let (sizes, _) = timestamped_dict(ui, &ui_path, b"SortHeadersSizes")?;
    let (_, cols) = sizes.iter().find(|(k, _)| is_width_key(k, tab_index))?;
    let Value::Dict(entries) = unwrap_shared_ref(cols) else { return None };
    Some(
        entries
            .iter()
            .filter_map(|(k, v)| Some((token(k)?, as_int(v)?)))
            .collect(),
    )
}

fn is_width_key(k: &Value, tab_index: i64) -> bool {
    matches!(k, Value::Tuple(items) if items.len() == 2
        && matches!(&items[0], Value::Bytes(b) if b.as_slice() == b"overviewScroll2")
        && as_int(&items[1]) == Some(tab_index))
}

/// root -> b"overview" -> b"tabsettings_new" -> (ts, dict), returning that dict.
fn tab_settings(user: &Value) -> Option<(&Entries, NodePath)> {
    let (ov, ov_path) = child_dict(user, b"overview", Vec::new())?;
    timestamped_dict(ov, &ov_path, b"tabsettings_new")
}

fn prettify(token: &str) -> String {
    // ponytail: naive Title-case. Compound tokens (TRANSVERSALVELOCITY) are not
    // word-split — that needs a curated map (deferred). Raw token shown on hover.
    let mut c = token.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => String::new(),
    }
}

fn as_int(v: &Value) -> Option<i64> {
    match v {
        Value::Int(n) => Some(*n),
        _ => None,
    }
}

fn token(v: &Value) -> Option<String> {
    match v {
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        _ => None,
    }
}

fn str_field(fields: &Entries, name: &str) -> Option<String> {
    fields.iter().find_map(|(k, v)| match k {
        Value::Str(s) | Value::StrUcs2(s) if s == name => match v {
            Value::Str(t) | Value::StrUcs2(t) => Some(t.clone()),
            _ => None,
        },
        _ => None,
    })
}

/// A list-of-Bytes field (tabColumns / tabColumnOrder / overviewColumns…) as tokens.
fn list_field(fields: &Entries, name: &[u8]) -> Option<Vec<String>> {
    let (_, v) = fields.iter().find(|(k, _)| is_bytes(k, name))?;
    let Value::List(items) = unwrap_shared_ref(v) else { return None };
    Some(items.iter().filter_map(token).collect())
}
```

Add to `crates/settings-model/src/lib.rs` after the `pub mod windows;` line and the windows re-export:

```rust
pub mod overview;
```
```rust
pub use overview::{project_overview, OverviewColumn, OverviewColumns, OverviewTab};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path crates/settings-model/Cargo.toml overview::`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/overview.rs crates/settings-model/src/lib.rs
git commit -m "Add overview-columns read projection to settings-model"
```

### Task A2: Visibility toggle + reorder (user tree, with materialize)

**Files:**
- Modify: `crates/settings-model/src/overview.rs`
- Modify: `crates/settings-model/src/lib.rs` (re-export the new items)
- Test: in `overview.rs`

**Interfaces:**
- Produces:
  - `pub enum OverviewError { NoTab }` (`Debug, PartialEq, Serialize`)
  - `pub fn set_column_visible(user: &mut Value, tab_index: i64, column: &str, visible: bool) -> Result<(), OverviewError>`
  - `pub fn set_column_order(user: &mut Value, tab_index: i64, order: &[String]) -> Result<(), OverviewError>`
  - Materialization: a tab with no own `tabColumnOrder`/`tabColumns` gets them created from the account defaults before the edit applies.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `overview.rs`:

```rust
    fn tab_lists(user: &Value, index: i64) -> (Vec<String>, Vec<String>) {
        let t = project_overview(user, None).tabs.into_iter().find(|t| t.index == index).unwrap();
        let order: Vec<String> = t.columns.iter().map(|c| c.name.clone()).collect();
        let visible: Vec<String> = t.columns.iter().filter(|c| c.visible).map(|c| c.name.clone()).collect();
        (order, visible)
    }

    #[test]
    fn toggle_visibility_on_an_owning_tab() {
        let mut user = user_with_tab();
        // TYPE starts hidden; show it.
        set_column_visible(&mut user, 0, "TYPE", true).unwrap();
        let (_, visible) = tab_lists(&user, 0);
        assert!(visible.contains(&"TYPE".to_string()));
        // Hide NAME again.
        set_column_visible(&mut user, 0, "NAME", false).unwrap();
        let (_, visible) = tab_lists(&user, 0);
        assert!(!visible.contains(&"NAME".to_string()));
    }

    #[test]
    fn reorder_sets_the_full_order() {
        let mut user = user_with_tab();
        set_column_order(&mut user, 0, &["DISTANCE".into(), "NAME".into(), "TYPE".into()]).unwrap();
        let (order, _) = tab_lists(&user, 0);
        assert_eq!(order, vec!["DISTANCE", "NAME", "TYPE"]);
    }

    /// A tab that inherits (no own lists) materializes from the account defaults
    /// on first edit, then applies the edit.
    fn user_inheriting_tab() -> Value {
        let tab = Value::Dict(vec![(Value::Str("name".into()), Value::Str("General".into()))]);
        Value::Dict(vec![(
            bytes("overview"),
            Value::Dict(vec![
                (bytes("overviewColumnOrder"), Value::List(vec![bytes("NAME"), bytes("TYPE")])),
                (bytes("overviewColumns"), Value::List(vec![bytes("NAME")])),
                (
                    bytes("tabsettings_new"),
                    Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(1), tab)])]),
                ),
            ]),
        )])
    }

    #[test]
    fn editing_an_inheriting_tab_materializes_its_lists() {
        let mut user = user_inheriting_tab();
        assert!(project_overview(&user, None).tabs[0].inherits);
        // Show TYPE on the inheriting tab.
        set_column_visible(&mut user, 1, "TYPE", true).unwrap();
        let t = project_overview(&user, None).tabs.into_iter().find(|t| t.index == 1).unwrap();
        assert!(!t.inherits, "tab now owns its lists");
        assert_eq!(t.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>(), vec!["NAME", "TYPE"]);
        assert!(t.columns.iter().find(|c| c.name == "TYPE").unwrap().visible);
    }

    #[test]
    fn editing_a_missing_tab_errors() {
        let mut user = user_with_tab();
        assert_eq!(set_column_visible(&mut user, 99, "NAME", true), Err(OverviewError::NoTab));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path crates/settings-model/Cargo.toml overview::`
Expected: FAIL (`set_column_visible`/`set_column_order`/`OverviewError` not defined).

- [ ] **Step 3: Write minimal implementation**

Add to `overview.rs` (implementation area, above tests). These functions locate the tab dict mutably, materialize its lists if absent, then edit:

```rust
use crate::path::{resolve_mut, Step};

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum OverviewError {
    NoTab,
}

pub fn set_column_visible(user: &mut Value, tab_index: i64, column: &str, visible: bool) -> Result<(), OverviewError> {
    with_tab(user, tab_index, |tab| {
        materialize(tab);
        let tok = Value::Bytes(column.as_bytes().to_vec());
        let vis = list_mut(tab, b"tabColumns");
        let present = vis.iter().any(|v| v == &tok);
        if visible && !present {
            vis.push(tok.clone());
        } else if !visible && present {
            vis.retain(|v| v != &tok);
        }
        // A newly-shown column must also exist in the order list.
        let order = list_mut(tab, b"tabColumnOrder");
        if visible && !order.iter().any(|v| v == &tok) {
            order.push(tok);
        }
    })
}

pub fn set_column_order(user: &mut Value, tab_index: i64, order: &[String]) -> Result<(), OverviewError> {
    with_tab(user, tab_index, |tab| {
        materialize(tab);
        let list = list_mut(tab, b"tabColumnOrder");
        *list = order.iter().map(|t| Value::Bytes(t.as_bytes().to_vec())).collect();
    })
}

/// Resolve the mutable tab dict by its Int index and run `edit` on it.
fn with_tab<F: FnOnce(&mut Vec<(Value, Value)>)>(user: &mut Value, tab_index: i64, edit: F) -> Result<(), OverviewError> {
    let path = tab_dict_path(user, tab_index).ok_or(OverviewError::NoTab)?;
    let node = resolve_mut(user, &path).ok_or(OverviewError::NoTab)?;
    let Value::Dict(fields) = node else { return Err(OverviewError::NoTab) };
    edit(fields);
    Ok(())
}

/// Path to the mutable tab dict, resolving the account defaults for materialize
/// eagerly (read them before taking the &mut borrow).
fn tab_dict_path(user: &Value, tab_index: i64) -> Option<NodePath> {
    let (dict, base) = tab_settings(user)?;
    let (i, _) = dict.iter().enumerate().find(|(_, (k, _))| as_int(k) == Some(tab_index))?;
    let mut p = base;
    p.push(Step::DictValue(i));
    Some(p)
}

/// Ensure the tab owns `tabColumnOrder` and `tabColumns`. Called after the
/// borrow, so defaults were captured earlier — here we only add empty lists if
/// missing; `materialize_from` fills them from captured defaults.
fn materialize(tab: &mut Vec<(Value, Value)>) {
    ensure_list(tab, b"tabColumnOrder");
    ensure_list(tab, b"tabColumns");
}

fn ensure_list(tab: &mut Vec<(Value, Value)>, name: &[u8]) {
    if !tab.iter().any(|(k, _)| is_bytes(k, name)) {
        tab.push((Value::Bytes(name.to_vec()), Value::List(vec![])));
    }
}

fn list_mut<'a>(tab: &'a mut Vec<(Value, Value)>, name: &[u8]) -> &'a mut Vec<Value> {
    let (_, v) = tab.iter_mut().find(|(k, _)| is_bytes(k, name)).expect("ensured by materialize");
    let Value::List(items) = v else { panic!("overview column list is not a List") };
    items
}
```

**Wait — materialize must copy the account defaults, not create empty lists,** or an inheriting tab would lose its inherited columns. Adjust `with_tab` to pass the captured defaults down. Replace the two edit fns and `materialize` above with this version:

```rust
pub fn set_column_visible(user: &mut Value, tab_index: i64, column: &str, visible: bool) -> Result<(), OverviewError> {
    let (def_order, def_visible) = account_defaults(user);
    with_tab(user, tab_index, |tab| {
        materialize_from(tab, &def_order, &def_visible);
        let tok = Value::Bytes(column.as_bytes().to_vec());
        let vis = list_mut(tab, b"tabColumns");
        let present = vis.iter().any(|v| v == &tok);
        if visible && !present { vis.push(tok.clone()); }
        else if !visible && present { vis.retain(|v| v != &tok); }
        let order = list_mut(tab, b"tabColumnOrder");
        if visible && !order.iter().any(|v| v == &tok) { order.push(tok); }
    })
}

pub fn set_column_order(user: &mut Value, tab_index: i64, order: &[String]) -> Result<(), OverviewError> {
    let (def_order, def_visible) = account_defaults(user);
    with_tab(user, tab_index, |tab| {
        materialize_from(tab, &def_order, &def_visible);
        *list_mut(tab, b"tabColumnOrder") =
            order.iter().map(|t| Value::Bytes(t.as_bytes().to_vec())).collect();
    })
}

/// Create the tab's own lists from the account defaults when absent (mirrors the
/// client materializing an inheriting tab on first edit). No-op if already owned.
fn materialize_from(tab: &mut Vec<(Value, Value)>, def_order: &[String], def_visible: &[String]) {
    if !tab.iter().any(|(k, _)| is_bytes(k, b"tabColumnOrder")) {
        tab.push((Value::Bytes(b"tabColumnOrder".to_vec()), toks(def_order)));
    }
    if !tab.iter().any(|(k, _)| is_bytes(k, b"tabColumns")) {
        tab.push((Value::Bytes(b"tabColumns".to_vec()), toks(def_visible)));
    }
}

fn toks(tokens: &[String]) -> Value {
    Value::List(tokens.iter().map(|t| Value::Bytes(t.as_bytes().to_vec())).collect())
}
```

(Delete the earlier `materialize`/`ensure_list` versions; keep `list_mut`, `with_tab`, `tab_dict_path`.)

Re-export in `lib.rs`:
```rust
pub use overview::{project_overview, set_column_order, set_column_visible, OverviewColumn, OverviewColumns, OverviewError, OverviewTab};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path crates/settings-model/Cargo.toml overview::`
Expected: PASS (all overview tests).

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/overview.rs crates/settings-model/src/lib.rs
git commit -m "Add overview visibility toggle and reorder with tab materialize"
```

### Task A3: Column width edit (char tree, create-as-needed)

**Files:**
- Modify: `crates/settings-model/src/overview.rs`
- Modify: `crates/settings-model/src/lib.rs`
- Test: in `overview.rs`

**Interfaces:**
- Produces: `pub fn set_column_width(char_tree: &mut Value, tab_index: i64, column: &str, width: i64) -> Result<(), OverviewError>`
- Behavior: overwrites the existing width; if the column entry (or the tab's width dict) is absent, inserts it. Errors `NoTab` if `ui → SortHeadersSizes` is absent entirely (nothing to attach to without inventing structure — the char file always has `ui` in practice; live smoke confirms).

- [ ] **Step 1: Write the failing test**

Add to `overview.rs` tests:

```rust
    fn width_of(char_tree: &Value, tab: i64, col: &str) -> Option<i64> {
        let user = user_with_tab(); // provides the order so the column appears
        project_overview(&user, Some(char_tree))
            .tabs.into_iter().find(|t| t.index == tab)?
            .columns.into_iter().find(|c| c.name == col)?.width
    }

    #[test]
    fn set_width_overwrites_existing() {
        let mut c = char_with_widths();
        set_column_width(&mut c, 0, "NAME", 200).unwrap();
        assert_eq!(width_of(&c, 0, "NAME"), Some(200));
    }

    #[test]
    fn set_width_inserts_a_new_column_entry() {
        let mut c = char_with_widths();
        set_column_width(&mut c, 0, "TYPE", 88).unwrap();
        assert_eq!(width_of(&c, 0, "TYPE"), Some(88));
        assert_eq!(width_of(&c, 0, "NAME"), Some(120), "existing width untouched");
    }

    #[test]
    fn set_width_creates_the_tab_width_dict_when_absent() {
        // char_with_widths only has tab 0; write tab 1.
        let mut c = char_with_widths();
        set_column_width(&mut c, 1, "NAME", 77).unwrap();
        // Re-project a user that has tab 1 to read it back.
        let user = user_inheriting_tab();
        let w = project_overview(&user, Some(&c)).tabs.into_iter()
            .find(|t| t.index == 1).unwrap()
            .columns.into_iter().find(|col| col.name == "NAME").unwrap().width;
        assert_eq!(w, Some(77));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path crates/settings-model/Cargo.toml overview::`
Expected: FAIL (`set_column_width` not defined).

- [ ] **Step 3: Write minimal implementation**

Add to `overview.rs`:

```rust
pub fn set_column_width(char_tree: &mut Value, tab_index: i64, column: &str, width: i64) -> Result<(), OverviewError> {
    let sizes_path = sort_headers_sizes_path(char_tree).ok_or(OverviewError::NoTab)?;
    let Some(Value::Dict(sizes)) = resolve_mut(char_tree, &sizes_path) else {
        return Err(OverviewError::NoTab);
    };
    // Find or create the tab's width dict, keyed by (overviewScroll2, tabIndex).
    let pos = sizes.iter().position(|(k, _)| is_width_key(k, tab_index));
    let cols = match pos {
        Some(i) => &mut sizes[i].1,
        None => {
            let key = Value::Tuple(vec![Value::Bytes(b"overviewScroll2".to_vec()), Value::Int(tab_index)]);
            sizes.push((key, Value::Dict(vec![])));
            &mut sizes.last_mut().unwrap().1
        }
    };
    let Value::Dict(entries) = cols else { return Err(OverviewError::NoTab) };
    let tok = column.as_bytes();
    match entries.iter_mut().find(|(k, _)| is_bytes(k, tok)) {
        Some((_, v)) => *v = Value::Int(width),
        None => entries.push((Value::Bytes(tok.to_vec()), Value::Int(width))),
    }
    Ok(())
}

/// Path to the inner dict of char root -> ui -> SortHeadersSizes -> (ts, dict).
fn sort_headers_sizes_path(char_tree: &Value) -> Option<NodePath> {
    let (ui, ui_path) = child_dict(char_tree, b"ui", Vec::new())?;
    let (_, path) = timestamped_dict(ui, &ui_path, b"SortHeadersSizes")?;
    Some(path)
}
```

Re-export in `lib.rs`:
```rust
pub use overview::{project_overview, set_column_order, set_column_visible, set_column_width, OverviewColumn, OverviewColumns, OverviewError, OverviewTab};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path crates/settings-model/Cargo.toml overview::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/overview.rs crates/settings-model/src/lib.rs
git commit -m "Add overview column width edit to settings-model"
```

---

## Phase B — Two-slot backend

Keeps the app working: after Task B2 the raw editor and layout canvas behave exactly as before, but two files can be open.

### Task B1: Two-slot `AppState`, `Slot` enum, slot-param command fns

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (AppState, Slot, all doc-scoped fns, capture)
- Modify: `app/src-tauri/src/lib.rs` (command wrappers pass `slot`; `AppState::new()` call unchanged)
- Test: `ops.rs` `#[cfg(test)]` (adapt existing + one new two-slot test)

**Interfaces:**
- Produces:
  - `pub struct AppState { pub char: Mutex<Option<Document>>, pub user: Mutex<Option<Document>>, pub capture: Mutex<Option<accounts::Snapshot>> }` with `AppState::new()`.
  - `pub enum Slot { Char, User }` (`Deserialize`, `serde(rename_all="snake_case")`, `Clone, Copy`).
  - Every doc fn gains a leading `slot: Slot`: `open_file(state, slot, path)`, `close_file(state, slot)`, `apply_mutation(state, slot, mutation)`, `save_document(state, slot, force)`, `list_file_backups(state, slot)`, `window_layout(state, slot)`, `restore_backup(state, slot, backup_path)`.
- Consumes: `Document`, existing save/apply/project.

- [ ] **Step 1: Write the failing test**

Replace `AppState` usages in `ops.rs` tests to pass a slot, and add a new test. First add this new test to `ops.rs` tests:

```rust
    #[test]
    fn two_slots_hold_independent_documents() {
        let ubytes = encode(&Value::Dict(vec![(Value::Bytes(b"u".to_vec()), Value::Int(1))])).unwrap();
        let cbytes = encode(&Value::Dict(vec![(Value::Bytes(b"c".to_vec()), Value::Int(2))])).unwrap();
        let upath = temp_file("slot-user", &ubytes);
        let cpath = temp_file("slot-char", &cbytes);
        let state = AppState::new();
        open_file(&state, Slot::User, upath.to_str().unwrap()).unwrap();
        open_file(&state, Slot::Char, cpath.to_str().unwrap()).unwrap();
        assert!(state.user.lock().unwrap().is_some());
        assert!(state.char.lock().unwrap().is_some());
        // Closing one leaves the other.
        close_file(&state, Slot::User);
        assert!(state.user.lock().unwrap().is_none());
        assert!(state.char.lock().unwrap().is_some());
    }
```

Then in each existing `ops.rs` test, update calls: `open_file(&state, path)` → `open_file(&state, Slot::Char, path)` (use `Slot::User` for the tests whose temp file is `core_user_5.dat`, which is fine either way), and `apply_mutation(&state, m)` → `apply_mutation(&state, Slot::Char, m)`, `save_document(&state, false)` → `save_document(&state, Slot::Char, false)`, `window_layout(&state)` → `window_layout(&state, Slot::Char)`, `list_file_backups(&state)` → `list_file_backups(&state, Slot::Char)`, `restore_backup(&state, p)` → `restore_backup(&state, Slot::Char, p)`. Update `state.0.lock()` → `state.char.lock()` (or `.user`) and drop `state.1` in favor of `state.capture`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path app/src-tauri/Cargo.toml ops::`
Expected: FAIL to compile (`Slot` undefined, field mismatches).

- [ ] **Step 3: Write minimal implementation**

In `ops.rs`, replace the `AppState` definition and add `Slot`:

```rust
/// Two open documents (char + user, for the two-file overview category) plus a
/// transient guided-capture baseline. Each document keeps its own save chain.
pub struct AppState {
    pub char: Mutex<Option<Document>>,
    pub user: Mutex<Option<Document>>,
    pub capture: Mutex<Option<accounts::Snapshot>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState { char: Mutex::new(None), user: Mutex::new(None), capture: Mutex::new(None) }
    }
    fn doc(&self, slot: Slot) -> &Mutex<Option<Document>> {
        match slot { Slot::Char => &self.char, Slot::User => &self.user }
    }
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    Char,
    User,
}
```

Update every doc fn to take `slot: Slot` and use `state.doc(slot)` instead of `state.0`. Examples:

```rust
pub fn open_file(state: &AppState, slot: Slot, path: &str) -> Result<OpenOutcome, ErrDto> {
    // ... unchanged body, but the two `*state.0.lock().unwrap() = ...`
    // become `*state.doc(slot).lock().unwrap() = ...`.
}

pub fn close_file(state: &AppState, slot: Slot) {
    *state.doc(slot).lock().unwrap() = None;
}

pub fn apply_mutation(state: &AppState, slot: Slot, mutation: &Mutation) -> Result<Node, ErrDto> {
    let mut guard = state.doc(slot).lock().unwrap();
    // ... rest unchanged ...
}
```

Apply the same `state.0` → `state.doc(slot)` change in `save_document`, `list_file_backups`, `window_layout`, `restore_backup` (its inner `open_file` call passes the same `slot`).

Update capture to exclude **both** open documents. Replace the two capture fns' open-path logic:

```rust
pub fn begin_capture(state: &AppState, roots: &[PathBuf]) {
    let profiles = discover(roots);
    let mut snap = accounts::snapshot_from_profiles(&profiles, None);
    for p in open_paths(state) { snap.remove(&p); }
    *state.capture.lock().unwrap() = Some(snap);
}

pub fn resolve_capture(state: &AppState, roots: &[PathBuf]) -> accounts::CaptureResult {
    let baseline = state.capture.lock().unwrap().clone().unwrap_or_default();
    let profiles = discover(roots);
    let mut after = accounts::snapshot_from_profiles(&profiles, None);
    for p in open_paths(state) { after.remove(&p); }
    accounts::capture_diff(&baseline, &after)
}

/// Paths of whatever documents are open (either slot) — excluded from capture
/// diffs since the app itself may write them.
fn open_paths(state: &AppState) -> Vec<PathBuf> {
    [Slot::Char, Slot::User]
        .into_iter()
        .filter_map(|s| state.doc(s).lock().unwrap().as_ref().map(|d| d.path.clone()))
        .collect()
}
```

In `app/src-tauri/src/lib.rs`, add `slot` to each doc-command wrapper and forward it. Example:

```rust
#[tauri::command]
fn open_file(state: tauri::State<'_, AppState>, slot: ops::Slot, path: String) -> Result<OpenOutcome, ErrDto> {
    ops::open_file(&state, slot, &path)
}
```

Repeat for `close_file`, `apply_mutation`, `save_document`, `list_file_backups`, `restore_backup`, `window_layout` (add `slot: ops::Slot` as the first arg after `state`, pass it through). Add `use ops::Slot;` or reference `ops::Slot` as shown. No change to `run()`'s handler list or `AppState::new()`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path app/src-tauri/Cargo.toml`
Expected: PASS (all ops tests incl. the new two-slot test).

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs
git commit -m "Give AppState two typed document slots with per-slot commands"
```

### Task B2: Frontend routes opens to slots by kind; slot switcher

**Files:**
- Modify: `app/src/lib/api.ts` (add `slot` to doc commands + a `Slot` type)
- Modify: `app/src/routes/+page.svelte` (two-slot state, routing, switcher)

**Interfaces:**
- Consumes: backend commands now expecting `slot`.
- Produces (frontend): `type Slot = "char" | "user"`; `+page.svelte` holds `slots: { char: OpenOutcome|null, user: OpenOutcome|null }` and an `active: Slot`.

- [ ] **Step 1: Update `api.ts`**

Add near the top-level exports:

```ts
export type Slot = "char" | "user";
```

Change the doc commands to take a slot (the invoke arg name must be `slot`):

```ts
export const api = {
  discover: () => invoke<Profile[]>("discover_profiles"),
  open: (slot: Slot, path: string) => invoke<OpenOutcome>("open_file", { slot, path }),
  close: (slot: Slot) => invoke<void>("close_file", { slot }),
  mutate: (slot: Slot, mutation: Mutation) => invoke<TreeNodeData>("apply_mutation", { slot, mutation }),
  save: (slot: Slot, force: boolean) => invoke<SaveReport>("save_document", { slot, force }),
  listBackups: (slot: Slot) => invoke<BackupInfo[]>("list_file_backups", { slot }),
  restoreBackup: (slot: Slot, backupPath: string) => invoke<OpenOutcome>("restore_backup", { slot, backupPath }),
  windowLayout: (slot: Slot) => invoke<WindowLayout>("window_layout", { slot }),
  // ... existing name/account commands unchanged ...
};
```

- [ ] **Step 2: Rework `+page.svelte` state to two slots**

Replace the single `current`/`dirty` model. Key changes (keep the rest of the file intact):

```ts
  import { api, errMessage, type OpenOutcome, type Slot } from "$lib/api";

  const slots = $state<{ char: OpenOutcome | null; user: OpenOutcome | null }>({ char: null, user: null });
  const dirtySlots = $state<{ char: boolean; user: boolean }>({ char: false, user: false });
  let active = $state<Slot>("char");
  const current = $derived(slots[active]);

  // Route a settings file to its slot by filename kind. Non-standard/other
  // files use the char slot (the generic editing slot).
  function slotForName(name: string): Slot {
    return /^core_user_\d+\.dat$/.test(name) ? "user" : "char";
  }
```

`openFile` becomes slot-aware and sets `active` to the opened slot:

```ts
  async function openFile(path: string) {
    const name = path.split(/[\\/]/).pop() ?? "";
    const slot = slotForName(name);
    if (dirtySlots.char || dirtySlots.user) {
      const which = [dirtySlots.char && "character", dirtySlots.user && "account"].filter(Boolean).join(" and ");
      const discard = await ask(
        `You have unsaved changes to the ${which} file. Discard them and open another file?`,
        { title: "Unsaved changes", kind: "warning" },
      );
      if (!discard) return;
    }
    try {
      const outcome = await api.open(slot, path);
      slots[slot] = outcome;
      dirtySlots[slot] = false;
      active = slot;
      savedAt += 1;
      view = "tree";
      mainView = "file";
      selectedWindowId = null;
      reveal = null;
      try {
        layoutAvailable = outcome.status === "opened" && (await api.windowLayout(slot)).windows.length > 0;
      } catch { layoutAvailable = false; }
    } catch (e) {
      await message(errMessage(e), { title: "Open failed", kind: "error" });
    }
  }
```

Update the derived name helpers (`openCharName`, `openUserAlias`, `openDisplay`) to read `current` (unchanged logic; `current` is now the derived active slot). Update `runMutation`, `saveFile`, `handleEdit`, and the BackupsPanel/LayoutView calls to pass `active`:
- `runMutation`: `current.tree = await api.mutate(active, m)`, guard on `slots[active]?.status`, set `dirtySlots[active] = true`, and reassign `slots[active] = { ...slots[active], tree }` so the derived `current` updates.
- `saveFile`: operate on `active`; `await api.save(active, force)`; set `dirtySlots[active] = false`.
- `BackupsPanel`: `subtitle={openDisplay}` unchanged; its restore path calls `api.restoreBackup(active, ...)` (thread `active` in via the existing `onRestored` flow — the parent already re-sets `slots[active]`).

Add a slot switcher in the filebar, shown only when both slots hold a file:

```svelte
        {#if slots.char && slots.user}
          <span class="viewtabs">
            <button class:active={active === "char"} onclick={() => (active = "char")}>Character</button>
            <button class:active={active === "user"} onclick={() => (active = "user")}>Account</button>
          </span>
        {/if}
```

- [ ] **Step 3: Typecheck and build**

Run: `npm --prefix app run check`
Expected: 0 errors (pre-existing InsertForm/TreeNode warnings are fine).
Run: `npm --prefix app run build`
Expected: build succeeds.

- [ ] **Step 4: Verify in the app**

Use the `run` skill (or `npm --prefix app run tauri dev`). Confirm: opening a char file works as before (Tree/Layout unchanged); opening a user file then a char file shows the **Character/Account** switcher and each tab shows the right tree; editing + saving each still works independently.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/api.ts app/src/routes/+page.svelte
git commit -m "Route file opens to char/user slots with an active-slot switcher"
```

---

## Phase C — Overview commands, loading flow, and UI

### Task C1: Overview commands (ops.rs + lib.rs)

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (overview command fns)
- Modify: `app/src-tauri/src/lib.rs` (wrappers + handler registration)
- Test: `ops.rs`

**Interfaces:**
- Produces (ops):
  - `pub fn overview_columns(state: &AppState) -> Result<OverviewColumns, ErrDto>` — reads the **user** slot (required; `no_document` if empty) and the char slot if present (widths).
  - `pub fn set_overview_visible(state, tab_index: i64, column: &str, visible: bool) -> Result<OverviewColumns, ErrDto>` — edits the user slot (read-only → `read_only`), re-projects.
  - `pub fn set_overview_order(state, tab_index: i64, order: Vec<String>) -> Result<OverviewColumns, ErrDto>` — edits the user slot.
  - `pub fn set_overview_width(state, tab_index: i64, column: &str, width: i64) -> Result<OverviewColumns, ErrDto>` — edits the char slot (read-only → `read_only`; `no_document` if no char slot).
- Consumes: `settings_model::{project_overview, set_column_visible, set_column_order, set_column_width, OverviewColumns}`.

- [ ] **Step 1: Write the failing test**

Add to `ops.rs` tests (helpers build minimal user/char files):

```rust
    fn overview_user_bytes() -> Vec<u8> {
        // root -> b"overview" -> b"tabsettings_new" -> (ts, { 0: {name, order, visible} })
        let tab = Value::Dict(vec![
            (Value::Str("name".into()), Value::Str("PvP".into())),
            (Value::Bytes(b"tabColumnOrder".to_vec()),
             Value::List(vec![Value::Bytes(b"NAME".to_vec()), Value::Bytes(b"TYPE".to_vec())])),
            (Value::Bytes(b"tabColumns".to_vec()), Value::List(vec![Value::Bytes(b"NAME".to_vec())])),
        ]);
        encode(&Value::Dict(vec![(
            Value::Bytes(b"overview".to_vec()),
            Value::Dict(vec![(
                Value::Bytes(b"tabsettings_new".to_vec()),
                Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Dict(vec![(Value::Int(0), tab)])]),
            )]),
        )])).unwrap()
    }

    #[test]
    fn overview_reads_and_edits_the_user_slot() {
        let path = temp_file("ov-user", &overview_user_bytes());
        let state = AppState::new();
        open_file(&state, Slot::User, path.to_str().unwrap()).unwrap();

        let oc = overview_columns(&state).unwrap();
        assert_eq!(oc.tabs.len(), 1);
        assert_eq!(oc.tabs[0].columns.iter().filter(|c| c.visible).count(), 1);

        // Show TYPE, then reorder.
        let oc = set_overview_visible(&state, 0, "TYPE", true).unwrap();
        assert_eq!(oc.tabs[0].columns.iter().filter(|c| c.visible).count(), 2);
        let oc = set_overview_order(&state, 0, vec!["TYPE".into(), "NAME".into()]).unwrap();
        assert_eq!(oc.tabs[0].columns[0].name, "TYPE");
    }

    #[test]
    fn overview_without_a_user_slot_errors() {
        let state = AppState::new();
        assert_eq!(overview_columns(&state).unwrap_err().code, "no_document");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path app/src-tauri/Cargo.toml ops::overview`
Expected: FAIL (functions not defined).

- [ ] **Step 3: Write minimal implementation**

Add to `ops.rs` (import the items at the top `use settings_model::{... , project_overview, set_column_order, set_column_visible, set_column_width, OverviewColumns};`):

```rust
pub fn overview_columns(state: &AppState) -> Result<OverviewColumns, ErrDto> {
    let user = state.user.lock().unwrap();
    let udoc = user.as_ref().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
    let char_guard = state.char.lock().unwrap();
    let char_val = char_guard.as_ref().map(|d| &d.value);
    Ok(project_overview(&udoc.value, char_val))
}

/// Edit the user slot (visibility/order), then re-project including char widths.
fn edit_user_overview<F>(state: &AppState, edit: F) -> Result<OverviewColumns, ErrDto>
where
    F: FnOnce(&mut blue_marshal::Value) -> Result<(), settings_model::OverviewError>,
{
    {
        let mut guard = state.user.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        edit(&mut doc.value).map_err(|e| ErrDto::new("overview", format!("{e:?}")))?;
    }
    overview_columns(state)
}

pub fn set_overview_visible(state: &AppState, tab_index: i64, column: &str, visible: bool) -> Result<OverviewColumns, ErrDto> {
    edit_user_overview(state, |v| set_column_visible(v, tab_index, column, visible))
}

pub fn set_overview_order(state: &AppState, tab_index: i64, order: Vec<String>) -> Result<OverviewColumns, ErrDto> {
    edit_user_overview(state, |v| set_column_order(v, tab_index, &order))
}

pub fn set_overview_width(state: &AppState, tab_index: i64, column: &str, width: i64) -> Result<OverviewColumns, ErrDto> {
    {
        let mut guard = state.char.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no character file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        set_column_width(&mut doc.value, tab_index, column, width)
            .map_err(|e| ErrDto::new("overview", format!("{e:?}")))?;
    }
    overview_columns(state)
}
```

In `lib.rs`, add wrappers and register them:

```rust
#[tauri::command]
fn overview_columns(state: tauri::State<'_, AppState>) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::overview_columns(&state)
}
#[tauri::command]
fn set_overview_visible(state: tauri::State<'_, AppState>, tab_index: i64, column: String, visible: bool) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::set_overview_visible(&state, tab_index, &column, visible)
}
#[tauri::command]
fn set_overview_order(state: tauri::State<'_, AppState>, tab_index: i64, order: Vec<String>) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::set_overview_order(&state, tab_index, order)
}
#[tauri::command]
fn set_overview_width(state: tauri::State<'_, AppState>, tab_index: i64, column: String, width: i64) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::set_overview_width(&state, tab_index, &column, width)
}
```

Add these four names to the `tauri::generate_handler![...]` list.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path app/src-tauri/Cargo.toml ops::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs
git commit -m "Add overview read/edit commands over the two slots"
```

### Task C2: Frontend loading helpers (pure, tested)

**Files:**
- Create: `app/src/lib/overview.ts`
- Create: `app/src/lib/overview.test.ts`

**Interfaces:**
- Produces:
  - `associatedCharacters(userId: number, roster: AccountRoster): number[]`
  - `accountOf(charId: number, roster: AccountRoster): number | null`
  - `pairedFilePath(profiles: Profile[], anchorPath: string, id: number, kind: "char" | "user"): string | null` — the file with `id`+`kind` in the **same profile folder** as `anchorPath`.

- [ ] **Step 1: Write the failing test**

`app/src/lib/overview.test.ts`:

```ts
import { test } from "node:test";
import assert from "node:assert/strict";
import { associatedCharacters, accountOf, pairedFilePath } from "./overview.ts";
import type { AccountRoster, Profile } from "./api.ts";

const roster: AccountRoster = {
  accounts: [{ user_id: 456, alias: "Main", characters: [123, 124] }],
  unassigned: [999],
};

test("associatedCharacters returns the account's characters", () => {
  assert.deepEqual(associatedCharacters(456, roster), [123, 124]);
  assert.deepEqual(associatedCharacters(789, roster), []);
});

test("accountOf finds the account holding a character", () => {
  assert.equal(accountOf(123, roster), 456);
  assert.equal(accountOf(999, roster), null);
});

const profiles: Profile[] = [{
  install: "i", server: "tq", profile: "Default",
  dir: "/eve/settings_Default",
  files: [
    { path: "/eve/settings_Default/core_char_123.dat", file_name: "core_char_123.dat", kind: "char", id: 123, size: 1, modified_unix: 1 },
    { path: "/eve/settings_Default/core_user_456.dat", file_name: "core_user_456.dat", kind: "user", id: 456, size: 1, modified_unix: 1 },
  ],
}];

test("pairedFilePath finds a file by id+kind in the anchor's folder", () => {
  const anchor = "/eve/settings_Default/core_char_123.dat";
  assert.equal(pairedFilePath(profiles, anchor, 456, "user"), "/eve/settings_Default/core_user_456.dat");
  assert.equal(pairedFilePath(profiles, anchor, 123, "char"), "/eve/settings_Default/core_char_123.dat");
  assert.equal(pairedFilePath(profiles, anchor, 777, "user"), null);
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `node --test app/src/lib/overview.test.ts`
Expected: FAIL (module not found).

- [ ] **Step 3: Write minimal implementation**

`app/src/lib/overview.ts`:

```ts
// Pure helpers for the overview editor's char↔user loading: roster lookups and
// same-folder file resolution. No Svelte/Tauri deps so this is node --test-able.
import type { AccountRoster, Profile } from "./api";

export function associatedCharacters(userId: number, roster: AccountRoster): number[] {
  return roster.accounts.find((a) => a.user_id === userId)?.characters ?? [];
}

export function accountOf(charId: number, roster: AccountRoster): number | null {
  return roster.accounts.find((a) => a.characters.includes(charId))?.user_id ?? null;
}

function dirOf(path: string): string {
  const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return i < 0 ? "" : path.slice(0, i);
}

export function pairedFilePath(
  profiles: Profile[],
  anchorPath: string,
  id: number,
  kind: "char" | "user",
): string | null {
  const dir = dirOf(anchorPath);
  for (const p of profiles) {
    for (const f of p.files) {
      if (f.kind === kind && f.id === id && dirOf(f.path) === dir) return f.path;
    }
  }
  return null;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `node --test app/src/lib/overview.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/overview.ts app/src/lib/overview.test.ts
git commit -m "Add pure overview loading helpers (roster and same-folder lookup)"
```

### Task C3: Bidirectional loading flow in `+page.svelte`

**Files:**
- Modify: `app/src/routes/+page.svelte`

**Interfaces:**
- Consumes: `api.discover()` (profiles), `accountsStore.roster`, C2 helpers, `api.open(slot, path)`.
- Produces: after opening a char file, the paired user file auto-loads into the user slot; after opening a user file, the character selector (Task C4) can load a char file. Both nudge to Accounts when the association is missing.

- [ ] **Step 1: Add profiles + roster access and an auto-pair effect**

`+page.svelte` currently discovers profiles only inside the Sidebar. Expose them to the page: store the discovered profiles in a page-level rune, refreshed alongside the sidebar. Simplest: import discovery here too.

```ts
  import { accountsStore } from "$lib/accounts.svelte";
  import { associatedCharacters, accountOf, pairedFilePath } from "$lib/overview";
  import type { Profile } from "$lib/api";

  let profiles = $state<Profile[]>([]);
  api.discover().then((p) => (profiles = p)).catch(() => {});

  // After a char file lands in the char slot, auto-load its paired user file.
  async function autoLoadPairedUser(charOutcome: OpenOutcome) {
    if (charOutcome.status !== "opened") return;
    const m = charOutcome.file_name.match(/^core_char_(\d+)\.dat$/);
    if (!m) return;
    const charId = Number(m[1]);
    const userId = accountOf(charId, accountsStore.roster);
    if (userId === null) return; // unpaired: OverviewView shows the Accounts nudge
    const userPath = pairedFilePath(profiles, charOutcome.path, userId, "user");
    if (!userPath || slots.user?.status === "opened") return;
    try {
      slots.user = await api.open("user", userPath);
      dirtySlots.user = false;
    } catch { /* leave user slot empty; nudge shown */ }
  }

  // Load a selected character into the char slot (from the OverviewView selector).
  async function loadCharacter(charId: number) {
    const anchor = slots.user?.status === "opened" ? slots.user.path : "";
    const charPath = pairedFilePath(profiles, anchor, charId, "char");
    if (!charPath) return;
    try {
      slots.char = await api.open("char", charPath);
      dirtySlots.char = false;
      await resolveNames([charId]);
    } catch (e) {
      await message(errMessage(e), { title: "Open failed", kind: "error" });
    }
  }
```

Call `autoLoadPairedUser(outcome)` at the end of `openFile`'s success path when `slot === "char"`.

- [ ] **Step 2: Typecheck**

Run: `npm --prefix app run check`
Expected: 0 errors.

- [ ] **Step 3: Commit**

```bash
git add app/src/routes/+page.svelte
git commit -m "Auto-load the paired user file and support loading a selected character"
```

### Task C4: `OverviewView.svelte` editor component

**Files:**
- Create: `app/src/lib/OverviewView.svelte`
- Modify: `app/src/lib/api.ts` (overview types + bindings), `app/src/routes/+page.svelte` (mount the view + Overview tab)

**Interfaces:**
- Consumes: `api.overviewColumns()`, `api.setOverviewVisible/Order/Width`, the roster, `loadCharacter` + `associatedCharacters` for the selector.
- Produces: emits nothing back except marking slots dirty via callbacks; parent tracks dirty.

- [ ] **Step 1: Add api.ts types + bindings**

```ts
export interface OverviewColumn { name: string; label: string; visible: boolean; width: number | null; }
export interface OverviewTab { index: number; name: string; inherits: boolean; columns: OverviewColumn[]; }
export interface OverviewColumns { tabs: OverviewTab[]; }
```
Add to `api`:
```ts
  overviewColumns: () => invoke<OverviewColumns>("overview_columns"),
  setOverviewVisible: (tabIndex: number, column: string, visible: boolean) =>
    invoke<OverviewColumns>("set_overview_visible", { tabIndex, column, visible }),
  setOverviewOrder: (tabIndex: number, order: string[]) =>
    invoke<OverviewColumns>("set_overview_order", { tabIndex, order }),
  setOverviewWidth: (tabIndex: number, column: string, width: number) =>
    invoke<OverviewColumns>("set_overview_width", { tabIndex, column, width }),
```

- [ ] **Step 2: Write `OverviewView.svelte`**

Props: `{ userOpen: boolean, charId: number | null, characters: number[], onLoadCharacter: (id) => void, onUserDirty: () => void, onCharDirty: () => void }`. It fetches columns on mount and after each edit, renders the tab selector, the character selector, and the column rows. Drag-reorder reuses the native HTML5 drag pattern (same approach as the layout canvas uses for pointer interactions). Concrete component:

```svelte
<script lang="ts">
  import { api, errMessage, type OverviewColumns } from "./api";
  import { message } from "@tauri-apps/plugin-dialog";
  import { names } from "./names.svelte";

  let { userOpen, charId, characters, onLoadCharacter, onUserDirty, onCharDirty }:
    { userOpen: boolean; charId: number | null; characters: number[];
      onLoadCharacter: (id: number) => void; onUserDirty: () => void; onCharDirty: () => void } = $props();

  let data = $state<OverviewColumns | null>(null);
  let tabIndex = $state<number | null>(null);
  let error = $state<string | null>(null);

  async function reload() {
    if (!userOpen) { data = null; return; }
    try {
      data = await api.overviewColumns();
      if (tabIndex === null && data.tabs.length > 0) tabIndex = data.tabs[0].index;
    } catch (e) { error = errMessage(e); }
  }
  $effect(() => { void userOpen; void charId; reload(); });

  const tab = $derived(data?.tabs.find((t) => t.index === tabIndex) ?? null);

  async function toggle(column: string, visible: boolean) {
    try { data = await api.setOverviewVisible(tabIndex!, column, visible); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
  async function setWidth(column: string, width: number) {
    if (charId === null || Number.isNaN(width)) return;
    try { data = await api.setOverviewWidth(tabIndex!, column, width); onCharDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }

  // Drag-reorder: track the dragged row index, drop reorders the token list.
  let dragFrom = $state<number | null>(null);
  async function drop(to: number) {
    if (dragFrom === null || !tab) return;
    const order = tab.columns.map((c) => c.name);
    const [moved] = order.splice(dragFrom, 1);
    order.splice(to, 0, moved);
    dragFrom = null;
    try { data = await api.setOverviewOrder(tabIndex!, order); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
</script>

{#if !userOpen}
  <p class="hint">Open an account (core_user) file to edit overview columns.</p>
{:else if error}
  <p class="error">{error}</p>
{:else if data && data.tabs.length === 0}
  <p class="hint">This account file has no overview tabs.</p>
{:else if data}
  <div class="ov-controls">
    <label>Tab
      <select bind:value={tabIndex}>
        {#each data.tabs as t (t.index)}<option value={t.index}>{t.name}</option>{/each}
      </select>
    </label>
    <label>Character (for widths)
      <select value={charId ?? ""} onchange={(e) => onLoadCharacter(Number((e.target as HTMLSelectElement).value))}>
        <option value="" disabled>Select…</option>
        {#each characters as c (c)}<option value={c}>{names[c]?.name ?? c}</option>{/each}
      </select>
    </label>
  </div>
  {#if characters.length === 0}
    <p class="hint">No characters associated with this account yet — pair one in Accounts to edit widths.</p>
  {/if}
  {#if tab}
    <ul class="ov-cols">
      {#each tab.columns as col, i (col.name)}
        <li draggable="true"
            ondragstart={() => (dragFrom = i)}
            ondragover={(e) => e.preventDefault()}
            ondrop={() => drop(i)}>
          <span class="grip" title="Drag to reorder">⠿</span>
          <label title={col.name}>
            <input type="checkbox" checked={col.visible} onchange={(e) => toggle(col.name, (e.target as HTMLInputElement).checked)} />
            {col.label}
          </label>
          <input class="w" type="number" min="0" disabled={charId === null}
                 value={col.width ?? ""} placeholder="—"
                 onchange={(e) => setWidth(col.name, Number((e.target as HTMLInputElement).value))} />
        </li>
      {/each}
    </ul>
    {#if tab.inherits}<p class="meta">This tab inherits the account default columns; editing it will give it its own copy.</p>{/if}
  {/if}
{/if}

<style>
  .ov-controls { display: flex; gap: 1rem; margin-bottom: 0.5rem; }
  /* Dark native controls: the app runs in a dark WebView2; give selects/inputs
     explicit dark colors (see the dark-native-controls memo). */
  select, input.w { background: #23262b; color: #e6e6e6; border: 1px solid #444; }
  .ov-cols { list-style: none; padding: 0; }
  .ov-cols li { display: flex; align-items: center; gap: 0.5rem; padding: 0.15rem 0; }
  .grip { cursor: grab; opacity: 0.6; }
  input.w { width: 5rem; }
</style>
```

- [ ] **Step 3: Mount it in `+page.svelte`**

Add "Overview" to the view tabs when the user slot is open, and render the component in the tree-area when `view === "overview"`:

```svelte
  <!-- in the view-tabs span -->
  {#if slots.user?.status === "opened"}
    <button class:active={view === "overview"} onclick={() => (view = "overview")}>Overview</button>
  {/if}
```
```svelte
  {#if view === "overview"}
    <div class="tree-area">
      <OverviewView
        userOpen={slots.user?.status === "opened"}
        charId={openCharId}
        characters={openAccountCharacters}
        onLoadCharacter={loadCharacter}
        onUserDirty={() => (dirtySlots.user = true)}
        onCharDirty={() => (dirtySlots.char = true)} />
    </div>
  {/if}
```

Add the two derived inputs near the other deriveds:

```ts
  import OverviewView from "$lib/OverviewView.svelte";

  const openCharId = $derived.by(() => {
    const o = slots.char;
    if (o?.status !== "opened") return null;
    const m = o.file_name.match(/^core_char_(\d+)\.dat$/);
    return m ? Number(m[1]) : null;
  });
  const openUserId = $derived.by(() => {
    const o = slots.user;
    if (o?.status !== "opened") return null;
    const m = o.file_name.match(/^core_user_(\d+)\.dat$/);
    return m ? Number(m[1]) : null;
  });
  const openAccountCharacters = $derived(
    openUserId === null ? [] : associatedCharacters(openUserId, accountsStore.roster),
  );
```

Extend the `view` type to include `"overview"`: `let view = $state<"tree" | "layout" | "overview">("tree");`. Resolve names for the account's characters so the selector shows names: `$effect(() => { if (openAccountCharacters.length) void resolveNames(openAccountCharacters); });`

- [ ] **Step 4: Typecheck, build, verify**

Run: `npm --prefix app run check` → 0 errors.
Run: `npm --prefix app run build` → succeeds.
Verify in the app (`run` skill): open a user file → Overview tab appears → pick a tab → toggle a column, drag to reorder, set a width after choosing a character. Confirm the edits stick on re-fetch.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/OverviewView.svelte app/src/lib/api.ts app/src/routes/+page.svelte
git commit -m "Add the overview columns editor view"
```

### Task C5: Cross-slot save + per-slot dirty badges + guard polish

**Files:**
- Modify: `app/src/routes/+page.svelte`

**Interfaces:**
- Produces: `Save` writes every dirty slot via its own chain; the filebar shows which slot(s) are dirty; the unsaved-changes guard (already added in B2) names the file(s).

- [ ] **Step 1: Save both dirty slots**

Replace `saveFile` so it saves each dirty slot independently (a failure on one still reports; the other is attempted):

```ts
  async function saveFile(force = false) {
    for (const slot of ["char", "user"] as const) {
      const o = slots[slot];
      if (!dirtySlots[slot] || o?.status !== "opened" || o.fidelity.state !== "editable") continue;
      try {
        const report = await api.save(slot, force);
        dirtySlots[slot] = false;
        savedAt += 1;
        let note = `Saved ${report.bytes_written} bytes to ${o.file_name}.\nBackup: ${report.backup_path}`;
        if (report.recent_sibling_writes.length > 0) {
          note += `\n\nWarning: other files in this profile changed recently — the EVE client may overwrite your changes on logout:\n${report.recent_sibling_writes.join("\n")}`;
        }
        await message(note, { title: "Saved", kind: "info" });
      } catch (e) {
        const err = e as ErrDto;
        if (err.code === "conflict") {
          const overwrite = await ask(
            `${o.file_name} changed on disk after it was loaded (the EVE client may have written it). Overwrite anyway?\n\nA backup of the on-disk file is taken first either way.`,
            { title: "File changed on disk", kind: "warning" });
          if (overwrite) { try { await api.save(slot, true); dirtySlots[slot] = false; savedAt += 1; } catch (e2) { await message(errMessage(e2), { title: "Save failed", kind: "error" }); } }
        } else {
          await message(errMessage(e), { title: `Save failed — ${o.file_name} untouched`, kind: "error" });
        }
      }
    }
  }
```

Update the Ctrl+S handler and Save button `disabled` to use `dirtySlots.char || dirtySlots.user`.

- [ ] **Step 2: Per-slot dirty badges**

In the filebar, replace the single `{#if dirty}` badge:

```svelte
  {#if dirtySlots.char}<span class="badge dirty">character: unsaved</span>{/if}
  {#if dirtySlots.user}<span class="badge dirty">account: unsaved</span>{/if}
```

- [ ] **Step 3: Typecheck, build, verify**

Run: `npm --prefix app run check` → 0 errors; `npm --prefix app run build` → succeeds.
Verify: make a visibility edit (user dirty) and a width edit (char dirty), press Save once → both files saved, two "Saved" dialogs (or confirm both write); reopen and confirm persistence.

- [ ] **Step 4: Commit**

```bash
git add app/src/routes/+page.svelte
git commit -m "Save both dirty slots and show per-slot unsaved state"
```

---

## Phase D — Verification

### Task D1: Full suite, corpus smoke, live smoke

**Files:** none (verification only).

- [ ] **Step 1: Run every test**

Run: `cargo test --manifest-path crates/settings-model/Cargo.toml`
Run: `cargo test --manifest-path app/src-tauri/Cargo.toml`
Run: `node --test app/src/lib/*.test.ts`
Run: `npm --prefix app run check`
Expected: all green; 0 typecheck errors.

- [ ] **Step 2: Corpus smoke for the projection**

If the historical corpus includes any `core_user` files, add a throwaway assertion (or a one-off `bmdump`-style check) that `project_overview` runs without panicking over them. If none are present, load a **fresh** `core_user` file from the current client through the app and confirm tabs render.

- [ ] **Step 3: Live smoke (the two-file path the tests can't cover)**

Run the app (`run` skill). With a real associated character+account:
- Open the character → the account auto-loads; the switcher shows Character/Account; the OS window title names the character (M3a followups already on master).
- Overview tab: reorder a column, hide/show one, set a width; Save.
- Reopen both files; confirm all three edits persisted and the client still reads the files (open EVE, verify the overview tab looks right) — this validates the materialize output against the real client.
- Open an **unpaired** character → Overview tab shows the "pair the account" nudge.

- [ ] **Step 4: Update the milestone memory**

Note M3c complete in the milestone-state memory (and that M3 — a/b/c — is done bar packaging/autofill, which are their own later milestones).

---

## Self-Review

**Spec coverage:**
- §2 mappings → A1 (read), A2/A3 (edits) use the exact paths. ✓
- §3 two-slot state + slot-param commands + capture-excludes-both → B1. ✓
- §4 bidirectional loading (char→auto user, user→char selector, nudge) → C3 (auto/selector), C4 (nudge UI). ✓
- §5 editor (tab selector, show/hide, reorder, width, inheritance note, prettify, save-both) → C4, C5; prettify → A1. ✓
- §6 `overview.rs` projection + edit fns + commands → A1–A3, C1. ✓
- §7 scope + cross-slot unsaved-changes guard → B2 (guard), C4 (widths in, per-tab, prettify), presets untouched. ✓
- §8 testing (unit + two-slot + corpus) → A/B/C tests, D1. ✓

**Placeholder scan:** no TBD/TODO; every code step shows code. The one deliberate simplification (naive prettify) is marked `ponytail:` in A1. Shared tree-walk helpers are extracted (A0) rather than duplicated. ✓

**Type consistency:** `Slot`/`slot` arg names match between api.ts and lib.rs commands; `OverviewColumns`/`OverviewTab`/`OverviewColumn` field names match across A1, C1, and api.ts; `set_column_visible/order/width` signatures match their callers in C1; `set_overview_*` command arg names (`tabIndex`, `column`, `visible`, `order`, `width`) match api.ts. ✓
