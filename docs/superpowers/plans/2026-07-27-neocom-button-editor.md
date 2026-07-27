# Neocom button editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the neocom's buttons editable — reorder, remove, add back, reset — from the Layout view's Neocom group, and carry them along with the layout batch aspect.

**Architecture:** A new `neocom.rs` in `settings-model` holds the projection and four index-keyed commands, following `stacks.rs` (inline first, edit, let the app layer reshare). `ops.rs` wraps them exactly as it wraps `stack_*`. The addable-button catalog is generated from the corpus by a Rust bin, shipped as JSON, and owned by the frontend, which unions it with the character's own `Original`. `Aspect::Layout` gains a second category so the bar rides along with a layout batch copy.

**Tech Stack:** Rust (`crates/settings-model`, no external deps beyond serde/blue_marshal), Tauri commands, Svelte 5 runes, TypeScript, `node --test` + vitest for the app, `cargo test` for the crate.

Spec: `docs/superpowers/specs/2026-07-27-neocom-button-editor-design.md`. Read its §2 before Task 1 — the data shape is corpus-verified and the plan's fixtures depend on it.

## Global Constraints

- **Commit messages are sentence case with NO attribution trailers** (no `Co-Authored-By`, no "Generated with"). Repo convention.
- **No new dependencies**, in the crate or the app.
- **Every structural editor inlines first.** `stacks.rs`'s module doc says why: window-id keys and other strings are `Shared` stores that the mutation layer refuses. `neocom.rs` follows: `inline_all(v)` as the first line of every command, and the app layer calls `blue_marshal::reshare` after.
- **Resolve section keys through `treewalk::section`.** In account files the root `ui` key is itself a `Ref`; a bare `is_bytes` match misses it. `section` handles it and is what `hud.rs` uses.
- **The instance shape is fixed** (spec §2): class `utillib.KeyVal`, state keys in the order `btnType, children, iconPath, id`. Anything authored uses exactly those four keys in that order.
- **`neocomButtonRawDataOriginal` is never written.** It is the character's own client baseline; the editor reads it and nothing more.
- **`cargo` and `npm` are not on the Bash tool's PATH on this machine — run them through PowerShell.** Crate suite from the repo root: `cargo test -p settings-model`. App suites from `app/`: `npm test`, `npm run check`, `npm run build`.
- **svelte-check baseline is 0 errors and 4 warnings**, all pre-existing in `ContextMenu.svelte`, `InsertForm.svelte`, `TreeNode.svelte`. The gate is 0 errors and no new warning in a file you changed.
- **Never commit corpus data.** `testdata/corpus` is real personal data. Generated artefacts must contain only client-generic ids and texture paths.

---

### Task 1: `neocom.rs` — the projection

**Files:**
- Create: `crates/settings-model/src/neocom.rs`
- Modify: `crates/settings-model/src/lib.rs` (module declaration + re-export, around lines 6-40)

**Interfaces:**
- Consumes: `treewalk`'s `section`, `effective`, `collect_shared`, `is_bytes`, `as_list`, `text` (all `pub(crate)`).
- Produces, and every later task depends on these exact names:
  - `pub struct NeocomBar { pub buttons: Vec<NeocomButton>, pub original: Vec<NeocomButton> }`
  - `pub struct NeocomButton { pub index: usize, pub id: String, pub btn_type: i64, pub icon_path: String, pub children: usize }`
  - `pub enum NeocomError { NoUi, NoBar, NoOriginal, BadIndex, BadOrder }`
  - `pub fn project_neocom(v: &Value) -> Result<NeocomBar, NeocomError>`

- [ ] **Step 1: Write the failing tests**

Create `crates/settings-model/src/neocom.rs` with ONLY the test module below plus `use blue_marshal::Value;` at the top. The fixture encodes the real shape from spec §2: a `utillib.KeyVal` instance whose state carries exactly `btnType, children, iconPath, id`.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
    fn ts() -> Value { Value::Long(vec![0u8; 8]) }

    /// One button, in the corpus's own key order.
    fn button(id: Value, btn_type: i64, icon: &str, children: Value) -> Value {
        Value::Instance {
            class: Box::new(b("utillib.KeyVal")),
            state: Box::new(Value::Dict(vec![
                (b("btnType"), Value::Int(btn_type)),
                (b("children"), children),
                (b("iconPath"), b(icon)),
                (b("id"), id),
            ])),
        }
    }

    /// char -> ui -> { neocomButtonRawData: (ts, List), neocomButtonRawDataOriginal: (ts, Tuple) }
    fn doc() -> Value {
        Value::Dict(vec![(b("ui"), Value::Dict(vec![
            (b("neocomButtonRawData"), Value::Tuple(vec![ts(), Value::List(vec![
                button(b("chat"), 10, "res:/ui/Texture/WindowIcons/chatchannel.png", Value::None),
                // A folder: children is a one-element list (the only shape the corpus has).
                button(b("inventory"), 4, "res:/UI/Texture/WindowIcons/items.png",
                       Value::List(vec![button(b("InventoryStation"), 4, "res:/UI/Texture/WindowIcons/station.png", Value::None)])),
                // The malformed id 11 corpus buttons carry: Tuple(bytes, None).
                button(Value::Tuple(vec![b("shipTree"), Value::None]), 1, "res:/ui/Texture/WindowIcons/shiptree.png", Value::None),
            ])])),
            (b("neocomButtonRawDataOriginal"), Value::Tuple(vec![ts(), Value::Tuple(vec![
                button(b("chat"), 10, "res:/ui/Texture/WindowIcons/chatchannel.png", Value::None),
                button(b("wallet"), 1, "res:/ui/Texture/WindowIcons/wallet.png", Value::None),
            ])])),
        ]))])
    }

    #[test]
    fn projects_the_bar_in_order_with_indices() {
        let bar = project_neocom(&doc()).unwrap();
        assert_eq!(bar.buttons.len(), 3);
        assert_eq!(bar.buttons.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(),
                   vec!["chat", "inventory", "shipTree"]);
        assert_eq!(bar.buttons.iter().map(|b| b.index).collect::<Vec<_>>(), vec![0, 1, 2]);
    }

    #[test]
    fn reads_btn_type_icon_and_child_count() {
        let bar = project_neocom(&doc()).unwrap();
        assert_eq!(bar.buttons[0].btn_type, 10);
        assert_eq!(bar.buttons[0].icon_path, "res:/ui/Texture/WindowIcons/chatchannel.png");
        assert_eq!(bar.buttons[0].children, 0, "children: None reads as 0");
        assert_eq!(bar.buttons[1].children, 1, "a one-element children list reads as 1");
    }

    #[test]
    fn a_tuple_shaped_id_renders_as_its_bytes_half() {
        // 11 corpus buttons carry id = Tuple(bytes, None). It must not project
        // as a debug string or an empty id — the UI shows it like any other.
        let bar = project_neocom(&doc()).unwrap();
        assert_eq!(bar.buttons[2].id, "shipTree");
    }

    #[test]
    fn projects_the_original_baseline_separately() {
        let bar = project_neocom(&doc()).unwrap();
        assert_eq!(bar.original.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(),
                   vec!["chat", "wallet"]);
    }

    #[test]
    fn a_document_without_the_key_is_an_error_but_a_missing_original_is_not() {
        let empty = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
        assert!(matches!(project_neocom(&empty), Err(NeocomError::NoBar)));
        assert!(matches!(project_neocom(&Value::Dict(vec![])), Err(NeocomError::NoUi)));

        // No Original at all: the bar still projects, with an empty baseline.
        let no_orig = Value::Dict(vec![(b("ui"), Value::Dict(vec![
            (b("neocomButtonRawData"), Value::Tuple(vec![ts(), Value::List(vec![
                button(b("chat"), 10, "icon.png", Value::None),
            ])])),
        ]))]);
        let bar = project_neocom(&no_orig).unwrap();
        assert_eq!(bar.buttons.len(), 1);
        assert!(bar.original.is_empty());
    }

    #[test]
    fn projects_through_shared_and_ref() {
        // Real files intern repeated key names and icon paths. Wrap the whole
        // ui value in a Shared and reach it by Ref, the way `section` must cope
        // with (the account-file `ui` key is itself a Ref — see hud.rs).
        let inner = doc();
        let Value::Dict(top) = &inner else { panic!() };
        let ui = top[0].1.clone();
        let shared = Value::Dict(vec![
            (b("other"), Value::Shared { slot: 1, value: Box::new(ui) }),
            (b("ui"), Value::Ref(1)),
        ]);
        let bar = project_neocom(&shared).unwrap();
        assert_eq!(bar.buttons.len(), 3);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run (PowerShell, repo root): `cargo test -p settings-model --lib neocom`

Expected: compile failure — `cannot find function project_neocom in this scope`, plus `NeocomBar`/`NeocomError` unresolved. That is the RED state for a from-scratch module.

- [ ] **Step 3: Write the implementation**

Put this ABOVE the test module in `crates/settings-model/src/neocom.rs`:

```rust
//! The neocom button bar: `ui → neocomButtonRawData`, a `(timestamp, List)` of
//! `utillib.KeyVal` instances whose state is always exactly
//! `{btnType, children, iconPath, id}` (corpus-verified over 43,430 buttons —
//! see docs/format-notes.md, "Neocom buttons"). Character-side, unlike
//! `neocomWidth`, which is per account.
//!
//! Commands key by INDEX, not id: 11 corpus buttons carry an id that is a
//! `Tuple(bytes, None)` rather than plain bytes, so ids are neither unique nor
//! always well-formed. Reorder and remove move whole instances, so a button's
//! children, icon and odd id ride along untouched.

use blue_marshal::Value;
use serde::Serialize;

use crate::treewalk::{as_list, collect_shared, effective, is_bytes, section, SharedTable};

pub const BAR_KEY: &[u8] = b"neocomButtonRawData";
pub const ORIGINAL_KEY: &[u8] = b"neocomButtonRawDataOriginal";

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum NeocomError {
    /// No `ui` section in the document.
    NoUi,
    /// No `neocomButtonRawData` under `ui`.
    NoBar,
    /// Reset was asked for on a document with no `neocomButtonRawDataOriginal`.
    NoOriginal,
    /// A button index that does not exist.
    BadIndex,
    /// A reorder that is not a permutation of the current indices.
    BadOrder,
}

impl std::fmt::Display for NeocomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NeocomError::NoUi => write!(f, "This file has no UI section."),
            NeocomError::NoBar => write!(f, "This file has no neocom buttons."),
            NeocomError::NoOriginal => write!(f, "This character has no original neocom bar to reset to."),
            NeocomError::BadIndex => write!(f, "That neocom button no longer exists."),
            NeocomError::BadOrder => write!(f, "That is not a valid ordering of the neocom buttons."),
        }
    }
}

#[derive(Debug, PartialEq, Serialize)]
pub struct NeocomButton {
    pub index: usize,
    pub id: String,
    pub btn_type: i64,
    pub icon_path: String,
    /// 0 for `None` or an empty list. The write path never re-authors children,
    /// so None-vs-empty is a distinction the UI does not need.
    pub children: usize,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct NeocomBar {
    pub buttons: Vec<NeocomButton>,
    /// `neocomButtonRawDataOriginal`, as-is. Empty when the file has none.
    /// NOT an "addable" set: the frontend unions this with the bundled catalog,
    /// because Original is a stale snapshot that misses buttons later patches
    /// added (spec §2.1).
    pub original: Vec<NeocomButton>,
}

/// The `(timestamp, payload)` payload, or the value itself if unwrapped.
fn payload<'a>(v: &'a Value, sh: &SharedTable<'a>) -> &'a Value {
    match effective(v, sh) {
        Value::Tuple(t) if t.len() == 2 => effective(&t[1], sh),
        other => other,
    }
}

/// A button id: plain `Bytes`, or the `Tuple(bytes, None)` shape 11 corpus
/// buttons carry — rendered as its bytes half either way.
fn id_text(v: &Value, sh: &SharedTable) -> String {
    match effective(v, sh) {
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        Value::Tuple(t) => t.first().map(|e| id_text(e, sh)).unwrap_or_default(),
        _ => String::new(),
    }
}

fn state_field<'a>(state: &'a Value, name: &[u8], sh: &SharedTable<'a>) -> Option<&'a Value> {
    let Value::Dict(d) = effective(state, sh) else { return None };
    d.iter()
        .find(|(k, _)| is_bytes(effective(k, sh), name))
        .map(|(_, v)| effective(v, sh))
}

fn read_button(index: usize, v: &Value, sh: &SharedTable) -> Option<NeocomButton> {
    let Value::Instance { state, .. } = effective(v, sh) else { return None };
    let id = state_field(state, b"id", sh).map(|v| id_text(v, sh)).unwrap_or_default();
    let btn_type = match state_field(state, b"btnType", sh) {
        Some(Value::Int(i)) => *i,
        _ => 0,
    };
    let icon_path = match state_field(state, b"iconPath", sh) {
        Some(Value::Bytes(b)) => String::from_utf8_lossy(b).into_owned(),
        _ => String::new(),
    };
    let children = state_field(state, b"children", sh)
        .and_then(|v| match v {
            Value::List(l) | Value::Tuple(l) => Some(l.len()),
            _ => None,
        })
        .unwrap_or(0);
    Some(NeocomButton { index, id, btn_type, icon_path, children })
}

fn read_list(v: &Value, sh: &SharedTable) -> Vec<NeocomButton> {
    match payload(v, sh) {
        Value::List(l) | Value::Tuple(l) => {
            l.iter().enumerate().filter_map(|(i, b)| read_button(i, b, sh)).collect()
        }
        _ => Vec::new(),
    }
}

pub fn project_neocom(v: &Value) -> Result<NeocomBar, NeocomError> {
    let mut sh = SharedTable::new();
    collect_shared(v, &mut sh);
    // `section` returns (&Entries, NodePath) and resolves a Shared/Ref section
    // key — in account files the root `ui` key is itself a Ref, which a bare
    // is_bytes match misses. The path half is for writers; the reader drops it.
    let (entries, _) = section(v, b"ui", &sh).ok_or(NeocomError::NoUi)?;
    let find = |name: &[u8]| {
        entries.iter().find(|(k, _)| is_bytes(effective(k, &sh), name)).map(|(_, v)| v)
    };
    let bar = find(BAR_KEY).ok_or(NeocomError::NoBar)?;
    Ok(NeocomBar {
        buttons: read_list(bar, &sh),
        original: find(ORIGINAL_KEY).map(|o| read_list(o, &sh)).unwrap_or_default(),
    })
}
```

Exact signatures, already checked — do not re-derive them:
`pub(crate) fn section<'a>(root: &'a Value, name: &[u8], shared: &SharedTable<'a>) -> Option<(&'a Entries, NodePath)>`,
`pub(crate) type SharedTable<'a> = std::collections::HashMap<u32, &'a Value>`,
`pub(crate) type Entries = Vec<(Value, Value)>`. `SharedTable::new()` is `HashMap::new()`.

`as_list` may prove unused once written — drop it from the `use` list rather than leaving a warning; the crate must compile clean.

- [ ] **Step 4: Declare and re-export the module**

In `crates/settings-model/src/lib.rs`, add `pub mod neocom;` beside the other `pub mod` lines, and this beside the other `pub use` lines:

```rust
pub use neocom::{project_neocom, NeocomBar, NeocomButton, NeocomError};
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p settings-model`

Expected: PASS, whole crate green, no warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/src/neocom.rs crates/settings-model/src/lib.rs
git commit -m "Project the neocom button bar"
```

---

### Task 2: `neocom.rs` — the four commands

**Files:**
- Modify: `crates/settings-model/src/neocom.rs` (implementation above the test module; new tests inside it)
- Modify: `crates/settings-model/src/lib.rs` (extend the re-export)

**Interfaces:**
- Consumes: Task 1's `NeocomError`, `BAR_KEY`, `ORIGINAL_KEY`, and its test fixtures (`b`, `ts`, `button`, `doc`).
- Produces:
  - `pub fn reorder(v: &mut Value, order: &[usize]) -> Result<(), NeocomError>`
  - `pub fn remove(v: &mut Value, index: usize) -> Result<(), NeocomError>`
  - `pub fn add(v: &mut Value, id: &str, btn_type: i64, icon_path: &str) -> Result<(), NeocomError>`
  - `pub fn reset(v: &mut Value) -> Result<(), NeocomError>`

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests` in `crates/settings-model/src/neocom.rs`:

```rust
    fn ids(v: &Value) -> Vec<String> {
        project_neocom(v).unwrap().buttons.into_iter().map(|b| b.id).collect()
    }

    #[test]
    fn reorder_rewrites_the_bar_in_the_given_order() {
        let mut v = doc();
        reorder(&mut v, &[2, 0, 1]).unwrap();
        assert_eq!(ids(&v), vec!["shipTree", "chat", "inventory"]);
    }

    #[test]
    fn reorder_moves_whole_instances_so_children_survive() {
        let mut v = doc();
        reorder(&mut v, &[1, 0, 2]).unwrap();
        let bar = project_neocom(&v).unwrap();
        assert_eq!(bar.buttons[0].id, "inventory");
        assert_eq!(bar.buttons[0].children, 1, "the folder kept its child");
        assert_eq!(bar.buttons[2].id, "shipTree", "the Tuple-shaped id survived the move");
    }

    #[test]
    fn reorder_rejects_anything_that_is_not_a_permutation() {
        for bad in [vec![0, 1], vec![0, 1, 3], vec![0, 0, 1], vec![0, 1, 2, 2]] {
            let mut v = doc();
            assert!(matches!(reorder(&mut v, &bad), Err(NeocomError::BadOrder)), "accepted {bad:?}");
            assert_eq!(ids(&v), vec!["chat", "inventory", "shipTree"], "a rejected reorder changed the bar");
        }
    }

    #[test]
    fn remove_drops_that_button_and_reindexes() {
        let mut v = doc();
        remove(&mut v, 1).unwrap();
        assert_eq!(ids(&v), vec!["chat", "shipTree"]);
        assert_eq!(project_neocom(&v).unwrap().buttons[1].index, 1);
    }

    #[test]
    fn remove_rejects_an_index_that_does_not_exist() {
        let mut v = doc();
        assert!(matches!(remove(&mut v, 3), Err(NeocomError::BadIndex)));
        assert_eq!(ids(&v), vec!["chat", "inventory", "shipTree"]);
    }

    #[test]
    fn add_appends_a_keyval_with_the_four_keys_in_order() {
        let mut v = doc();
        add(&mut v, "wallet", 1, "res:/ui/Texture/WindowIcons/wallet.png").unwrap();
        assert_eq!(ids(&v), vec!["chat", "inventory", "shipTree", "wallet"]);

        // The authored instance must match the corpus shape exactly: class
        // utillib.KeyVal, and the four keys in the corpus's own order.
        let bar = project_neocom(&v).unwrap();
        assert_eq!(bar.buttons[3].btn_type, 1);
        assert_eq!(bar.buttons[3].icon_path, "res:/ui/Texture/WindowIcons/wallet.png");
        assert_eq!(bar.buttons[3].children, 0);

        let Value::Dict(top) = &v else { panic!() };
        let (_, ui) = top.iter().find(|(k, _)| matches!(k, Value::Bytes(x) if x == b"ui")).unwrap();
        let Value::Dict(uid) = ui else { panic!() };
        let (_, raw) = uid.iter().find(|(k, _)| matches!(k, Value::Bytes(x) if x == b"neocomButtonRawData")).unwrap();
        let Value::Tuple(t) = raw else { panic!() };
        let Value::List(l) = &t[1] else { panic!() };
        let Value::Instance { class, state } = &l[3] else { panic!("added entry is not an instance") };
        assert_eq!(**class, b("utillib.KeyVal"));
        let Value::Dict(st) = &**state else { panic!() };
        let keys: Vec<String> = st.iter().map(|(k, _)| match k {
            Value::Bytes(x) => String::from_utf8_lossy(x).into_owned(),
            _ => String::new(),
        }).collect();
        assert_eq!(keys, vec!["btnType", "children", "iconPath", "id"]);
        assert_eq!(st[1].1, Value::None, "children is authored as None");
    }

    #[test]
    fn reset_replaces_the_bar_with_the_original_as_a_list() {
        let mut v = doc();
        reset(&mut v).unwrap();
        assert_eq!(ids(&v), vec!["chat", "wallet"]);

        // Original is STORED in a Tuple; the live bar must be a List.
        let Value::Dict(top) = &v else { panic!() };
        let (_, ui) = top.iter().find(|(k, _)| matches!(k, Value::Bytes(x) if x == b"ui")).unwrap();
        let Value::Dict(uid) = ui else { panic!() };
        let (_, raw) = uid.iter().find(|(k, _)| matches!(k, Value::Bytes(x) if x == b"neocomButtonRawData")).unwrap();
        let Value::Tuple(t) = raw else { panic!() };
        assert!(matches!(&t[1], Value::List(_)), "the live bar must stay a List");
    }

    #[test]
    fn reset_without_an_original_errors_and_changes_nothing() {
        let mut v = Value::Dict(vec![(b("ui"), Value::Dict(vec![
            (b("neocomButtonRawData"), Value::Tuple(vec![ts(), Value::List(vec![
                button(b("chat"), 10, "icon.png", Value::None),
            ])])),
        ]))]);
        assert!(matches!(reset(&mut v), Err(NeocomError::NoOriginal)));
        assert_eq!(ids(&v), vec!["chat"]);
    }

    #[test]
    fn every_command_leaves_a_tree_that_still_encodes() {
        // The commands inline first (dropping Shared/Ref); the result must
        // still round-trip, the way stacks.rs proves for its own edits.
        let mut v = doc();
        reorder(&mut v, &[2, 1, 0]).unwrap();
        remove(&mut v, 0).unwrap();
        add(&mut v, "wallet", 1, "wallet.png").unwrap();
        let bytes = blue_marshal::encode(&v).expect("edited tree still encodes");
        assert_eq!(blue_marshal::decode(&bytes).unwrap(), v);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p settings-model --lib neocom`

Expected: compile failure — `cannot find function reorder in this scope` (and `remove`, `add`, `reset`).

- [ ] **Step 3: Write the implementation**

Append to the implementation half of `crates/settings-model/src/neocom.rs`:

```rust
use crate::treewalk::inline_all;

/// The live bar's `List` payload, mutable. The document is already inlined, so
/// no `Shared` wrapper survives to resolve.
fn bar_list_mut(v: &mut Value) -> Result<&mut Vec<Value>, NeocomError> {
    let Value::Dict(top) = v else { return Err(NeocomError::NoUi) };
    let (_, ui) = top.iter_mut().find(|(k, _)| is_bytes(k, b"ui")).ok_or(NeocomError::NoUi)?;
    let Value::Dict(entries) = ui else { return Err(NeocomError::NoUi) };
    let (_, raw) = entries.iter_mut().find(|(k, _)| is_bytes(k, BAR_KEY)).ok_or(NeocomError::NoBar)?;
    // (timestamp, payload) on every real file; tolerate a bare payload too.
    let payload = match raw {
        Value::Tuple(t) if t.len() == 2 => &mut t[1],
        other => other,
    };
    match payload {
        Value::List(l) => Ok(l),
        // A Tuple-stored bar is not a shape the corpus has, but the reset path
        // below guarantees a List, so normalize rather than fail.
        Value::Tuple(t) => {
            let items = std::mem::take(t);
            *payload = Value::List(items);
            let Value::List(l) = payload else { unreachable!() };
            Ok(l)
        }
        _ => Err(NeocomError::NoBar),
    }
}

pub fn reorder(v: &mut Value, order: &[usize]) -> Result<(), NeocomError> {
    // Validate BEFORE inlining, so a rejected reorder leaves the document
    // byte-for-byte as it was (the tests assert exactly this).
    {
        let n = project_neocom(v)?.buttons.len();
        if order.len() != n {
            return Err(NeocomError::BadOrder);
        }
        let mut seen = vec![false; n];
        for &i in order {
            let slot = seen.get_mut(i).ok_or(NeocomError::BadOrder)?;
            if *slot {
                return Err(NeocomError::BadOrder); // a repeat
            }
            *slot = true;
        }
    }
    inline_all(v);
    let list = bar_list_mut(v)?;
    // Move whole instances: take them out, then put them back in the new order.
    let taken: Vec<Value> = std::mem::take(list);
    *list = order.iter().map(|&i| taken[i].clone()).collect();
    Ok(())
}

pub fn remove(v: &mut Value, index: usize) -> Result<(), NeocomError> {
    if index >= project_neocom(v)?.buttons.len() {
        return Err(NeocomError::BadIndex);
    }
    inline_all(v);
    let list = bar_list_mut(v)?;
    list.remove(index);
    Ok(())
}

pub fn add(v: &mut Value, id: &str, btn_type: i64, icon_path: &str) -> Result<(), NeocomError> {
    inline_all(v);
    let list = bar_list_mut(v)?;
    // The exact corpus shape: utillib.KeyVal, four keys, this order (spec §2).
    list.push(Value::Instance {
        class: Box::new(Value::Bytes(b"utillib.KeyVal".to_vec())),
        state: Box::new(Value::Dict(vec![
            (Value::Bytes(b"btnType".to_vec()), Value::Int(btn_type)),
            (Value::Bytes(b"children".to_vec()), Value::None),
            (Value::Bytes(b"iconPath".to_vec()), Value::Bytes(icon_path.as_bytes().to_vec())),
            (Value::Bytes(b"id".to_vec()), Value::Bytes(id.as_bytes().to_vec())),
        ])),
    });
    Ok(())
}

pub fn reset(v: &mut Value) -> Result<(), NeocomError> {
    if project_neocom(v)?.original.is_empty() {
        return Err(NeocomError::NoOriginal);
    }
    inline_all(v);
    // Read the (now inlined) Original, then write it over the live bar. Original
    // itself is never modified — it is the character's own client baseline.
    let original: Vec<Value> = {
        let Value::Dict(top) = &*v else { return Err(NeocomError::NoUi) };
        let (_, ui) = top.iter().find(|(k, _)| is_bytes(k, b"ui")).ok_or(NeocomError::NoUi)?;
        let Value::Dict(entries) = ui else { return Err(NeocomError::NoUi) };
        let (_, orig) = entries.iter().find(|(k, _)| is_bytes(k, ORIGINAL_KEY)).ok_or(NeocomError::NoOriginal)?;
        let payload = match orig {
            Value::Tuple(t) if t.len() == 2 => &t[1],
            other => other,
        };
        match payload {
            Value::List(l) | Value::Tuple(l) => l.clone(),
            _ => return Err(NeocomError::NoOriginal),
        }
    };
    let list = bar_list_mut(v)?;
    *list = original; // a List, whatever Original was stored as
    Ok(())
}
```

- [ ] **Step 4: Extend the re-export**

In `crates/settings-model/src/lib.rs`:

```rust
pub use neocom::{add as neocom_add, project_neocom, remove as neocom_remove, reorder as neocom_reorder, reset as neocom_reset, NeocomBar, NeocomButton, NeocomError};
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p settings-model`

Expected: PASS, whole crate green, no warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/src/neocom.rs crates/settings-model/src/lib.rs
git commit -m "Reorder, remove, add and reset neocom buttons"
```

---

### Task 3: The bundled catalog

**Files:**
- Create: `crates/settings-model/src/bin/neocom_catalog.rs`
- Create: `app/src/lib/data/neocom-buttons.json` (generated, committed)

**Interfaces:**
- Consumes: nothing from earlier tasks (it reads the corpus directly through `blue_marshal`).
- Produces: `app/src/lib/data/neocom-buttons.json`, an array sorted by id: `[{ "id": "wallet", "btnType": 1, "iconPath": "res:/ui/Texture/WindowIcons/wallet.png" }, …]`. Task 5 imports it.

- [ ] **Step 1: Write the generator**

`pack_palette.rs` in the same directory is the precedent — a research tool kept as a bin because it needs the codec (Python has no marshal decoder, which is why this is not a `tools/*.py` like the other generators).

Create `crates/settings-model/src/bin/neocom_catalog.rs`:

```rust
// Harvest the neocom button catalog (id -> btnType, iconPath) from the corpus,
// as JSON for app/src/lib/data/neocom-buttons.json. A button's btnType and icon
// are attributes of what the button IS, not of the character (spec §2), so the
// most common pairing per id is the canonical one.
//
// usage: cargo run -p settings-model --bin neocom_catalog -- <corpus-dir> > app/src/lib/data/neocom-buttons.json
use blue_marshal::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn child<'a>(v: &'a Value, key: &[u8]) -> Option<&'a Value> {
    let Value::Dict(d) = v else { return None };
    d.iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b.as_slice() == key)).map(|(_, v)| v)
}

fn payload(v: &Value) -> &Value {
    match v {
        Value::Tuple(t) if t.len() == 2 => &t[1],
        other => other,
    }
}

fn bytes(v: &Value) -> Option<String> {
    match v {
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        Value::Tuple(t) => t.first().and_then(bytes), // the Tuple(bytes, None) id shape
        _ => None,
    }
}

/// id -> (btnType, iconPath) -> count
type Tally = BTreeMap<String, BTreeMap<(i64, String), usize>>;

fn visit(v: &Value, out: &mut Tally) {
    let Value::Instance { state, .. } = v else { return };
    let (Some(id), Some(bt), Some(icon)) = (
        child(state, b"id").and_then(bytes),
        child(state, b"btnType").and_then(|v| if let Value::Int(i) = v { Some(*i) } else { None }),
        child(state, b"iconPath").and_then(bytes),
    ) else { return };
    if id.is_empty() { return }
    *out.entry(id).or_default().entry((bt, icon)).or_default() += 1;
    if let Some(Value::List(kids) | Value::Tuple(kids)) = child(state, b"children") {
        for k in kids { visit(k, out); }
    }
}

fn main() {
    let dir = std::env::args().nth(1).expect("usage: neocom_catalog <corpus-dir>");
    let mut files = Vec::new();
    collect(Path::new(&dir), &mut files);
    let mut tally: Tally = BTreeMap::new();
    for p in &files {
        if !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("core_char_")) { continue }
        let Ok(raw) = std::fs::read(p) else { continue };
        let Ok(decoded) = blue_marshal::decode(&raw) else { continue };
        let v = blue_marshal::inline(&decoded); // resolve Shared/Ref before reading
        let Some(ui) = child(&v, b"ui") else { continue };
        for key in [b"neocomButtonRawData".as_slice(), b"neocomButtonRawDataOriginal"] {
            if let Some(bar) = child(ui, key) {
                if let Value::List(l) | Value::Tuple(l) = payload(bar) {
                    for b in l { visit(b, &mut tally); }
                }
            }
        }
    }
    println!("[");
    let rows: Vec<String> = tally
        .iter()
        .filter_map(|(id, variants)| {
            let ((bt, icon), _) = variants.iter().max_by_key(|(_, n)| **n)?;
            Some(format!("  {{ \"id\": {}, \"btnType\": {bt}, \"iconPath\": {} }}", quote(id), quote(icon)))
        })
        .collect();
    println!("{}", rows.join(",\n"));
    println!("]");
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() { collect(&p, out); } else if p.extension().is_some_and(|x| x == "dat") { out.push(p); }
    }
}
```

- [ ] **Step 2: Generate the catalog**

Run (PowerShell, repo root):

```
cargo run -q -p settings-model --bin neocom_catalog -- testdata\corpus | Out-File -Encoding utf8 app\src\lib\data\neocom-buttons.json
```

- [ ] **Step 3: Check the output before committing it**

Read `app/src/lib/data/neocom-buttons.json`. It must:
- be valid JSON (an array of objects with exactly `id`, `btnType`, `iconPath`);
- hold roughly **25** entries — the corpus survey found 25 distinct ids (spec §2);
- contain `chat` with `btnType` 10, `airCareerProgram` with 21, `inventory` with 4, and a `btnType` 1 entry such as `wallet`;
- contain **no character names, character ids, file paths or anything else personal** — ids and `res:/…` texture paths only. `testdata/corpus` is real personal data and none of it may reach a committed file.

If the file has a UTF-8 BOM, strip it — the app's JSON import will choke on one.

If the entry count is far off 25, stop and report rather than committing a catalog you cannot explain.

- [ ] **Step 4: Commit**

```bash
git add crates/settings-model/src/bin/neocom_catalog.rs app/src/lib/data/neocom-buttons.json
git commit -m "Harvest the neocom button catalog from the corpus"
```

---

### Task 4: Backend wiring — ops, Tauri commands, api.ts

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (beside `edit_char_stacks` / `stack_*`, around lines 1068-1100)
- Modify: `app/src-tauri/src/lib.rs` (command wrappers around line 288, registration list around line 387)
- Modify: `app/src/lib/api.ts` (types beside `Stack`/`WindowLayout`, invokes beside `stack*` around line 398)

**Interfaces:**
- Consumes: Task 2's `settings_model::{project_neocom, neocom_add, neocom_remove, neocom_reorder, neocom_reset, NeocomBar, NeocomError}`.
- Produces, for Task 5:
  - TS types `NeocomButton { index: number; id: string; btn_type: number; icon_path: string; children: number }` and `NeocomBar { buttons: NeocomButton[]; original: NeocomButton[] }`
  - `api.neocomBar()`, `api.neocomReorder(order: number[])`, `api.neocomRemove(index: number)`, `api.neocomAdd(id, btnType, iconPath)`, `api.neocomReset()` — every one resolving to a fresh `NeocomBar`.

- [ ] **Step 1: Add the ops layer**

In `app/src-tauri/src/ops.rs`, directly after `stack_create`, add. This mirrors `edit_char_stacks` exactly — read it first (same file, just above) and match it, including the read-only check and the reshare:

```rust
/// Project the CHAR slot's neocom bar.
pub fn neocom_bar(state: &AppState) -> Result<NeocomBar, ErrDto> {
    let guard = state.char.lock().unwrap();
    let doc = guard.as_ref().ok_or_else(|| ErrDto::new("no_document", "no character file open"))?;
    settings_model::project_neocom(&doc.value).map_err(neocom_err)
}

fn neocom_err(e: NeocomError) -> ErrDto {
    let v = serde_json::to_value(&e).unwrap_or_default();
    ErrDto::new(v.get("code").and_then(|c| c.as_str()).unwrap_or("neocom"), e.to_string())
}

/// Edit the CHAR slot's neocom bar, reshare, then re-project it.
fn edit_char_neocom<F>(state: &AppState, edit: F) -> Result<NeocomBar, ErrDto>
where
    F: FnOnce(&mut blue_marshal::Value) -> Result<(), NeocomError>,
{
    {
        let mut guard = state.char.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no character file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        edit(&mut doc.value).map_err(neocom_err)?;
        doc.value = blue_marshal::reshare(&doc.value);
    }
    neocom_bar(state)
}

pub fn neocom_reorder(state: &AppState, order: Vec<usize>) -> Result<NeocomBar, ErrDto> {
    edit_char_neocom(state, |v| settings_model::neocom_reorder(v, &order))
}
pub fn neocom_remove(state: &AppState, index: usize) -> Result<NeocomBar, ErrDto> {
    edit_char_neocom(state, |v| settings_model::neocom_remove(v, index))
}
pub fn neocom_add(state: &AppState, id: &str, btn_type: i64, icon_path: &str) -> Result<NeocomBar, ErrDto> {
    edit_char_neocom(state, |v| settings_model::neocom_add(v, id, btn_type, icon_path))
}
pub fn neocom_reset(state: &AppState) -> Result<NeocomBar, ErrDto> {
    edit_char_neocom(state, settings_model::neocom_reset)
}
```

Extend the file's `use settings_model::{…}` line with `NeocomBar, NeocomError`.

- [ ] **Step 2: Add the Tauri commands**

In `app/src-tauri/src/lib.rs`, beside the `stack_*` wrappers:

```rust
#[tauri::command]
fn neocom_bar(state: tauri::State<'_, AppState>) -> Result<settings_model::NeocomBar, ErrDto> {
    ops::neocom_bar(&state)
}
#[tauri::command]
fn neocom_reorder(state: tauri::State<'_, AppState>, order: Vec<usize>) -> Result<settings_model::NeocomBar, ErrDto> {
    ops::neocom_reorder(&state, order)
}
#[tauri::command]
fn neocom_remove(state: tauri::State<'_, AppState>, index: usize) -> Result<settings_model::NeocomBar, ErrDto> {
    ops::neocom_remove(&state, index)
}
#[tauri::command]
fn neocom_add(state: tauri::State<'_, AppState>, id: String, btn_type: i64, icon_path: String) -> Result<settings_model::NeocomBar, ErrDto> {
    ops::neocom_add(&state, &id, btn_type, &icon_path)
}
#[tauri::command]
fn neocom_reset(state: tauri::State<'_, AppState>) -> Result<settings_model::NeocomBar, ErrDto> {
    ops::neocom_reset(&state)
}
```

Rust parameters stay snake_case and the JS side passes camelCase keys — Tauri converts. Already verified against `setup_preview` (`source_char_path` in Rust, `sourceCharPath` from `api.ts`), so do not "fix" the naming in either direction.

Add all five to the `tauri::generate_handler![…]` list beside `stack_unstack, stack_add, stack_reorder, stack_create,`.

- [ ] **Step 3: Add the frontend API**

In `app/src/lib/api.ts`, beside the `Stack`/`WindowLayout` interfaces:

```ts
export interface NeocomButton {
  index: number;
  id: string;
  btn_type: number;
  icon_path: string;
  /** 0 for a plain button; a folder's child count otherwise. */
  children: number;
}
export interface NeocomBar {
  buttons: NeocomButton[];
  /** The client's own baseline. Not the addable set — see neocom.ts. */
  original: NeocomButton[];
}
```

and beside the `stack*` invokes:

```ts
  neocomBar: () => invoke<NeocomBar>("neocom_bar"),
  neocomReorder: (order: number[]) => invoke<NeocomBar>("neocom_reorder", { order }),
  neocomRemove: (index: number) => invoke<NeocomBar>("neocom_remove", { index }),
  neocomAdd: (id: string, btnType: number, iconPath: string) =>
    invoke<NeocomBar>("neocom_add", { id, btnType, iconPath }),
  neocomReset: () => invoke<NeocomBar>("neocom_reset"),
```

- [ ] **Step 4: Verify**

Run from the repo root: `cargo test -p settings-model` and `cargo check --manifest-path app/src-tauri/Cargo.toml`
Run from `app/`: `npm run check`

Expected: crate green; the Tauri crate compiles with no warnings; svelte-check 0 errors and no new warning.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Wire the neocom commands through to the frontend"
```

---

### Task 5: The panel UI

**Files:**
- Create: `app/src/lib/neocom.ts` (pure: the addable-set derivation)
- Create: `app/src/lib/neocom.test.ts` (node --test)
- Create: `app/src/lib/NeocomButtons.svelte`
- Create: `app/src/lib/NeocomButtons.spec.ts` (vitest, jsdom — this is a panel list, not canvas pointer choreography, so a component test IS wanted here; `HudPanel.spec.ts` is the model to copy)
- Modify: `app/src/lib/HudPanel.svelte` (render the child inside the Neocom group)
- Modify: `app/src/lib/LayoutView.svelte` (load the bar, pass it down, dispatch the four commands)

**Interfaces:**
- Consumes: Task 4's `api.neocom*` and the `NeocomBar`/`NeocomButton` types; Task 3's `app/src/lib/data/neocom-buttons.json`.
- Produces: nothing later tasks consume.

- [ ] **Step 1: Write the failing test for the pure part**

Create `app/src/lib/neocom.test.ts`:

```ts
// Run: npm test (node --test; Node strips the types). Throw-based checks, no
// framework — matching layout.test.ts.
import { addableButtons } from "./neocom.ts";
import type { NeocomButton } from "./api.ts";

const check = (name: string, ok: boolean) => {
  if (!ok) throw new Error(`FAIL: ${name}`);
  console.log(`  ok - ${name}`);
};

const btn = (id: string, btn_type = 1, icon_path = `${id}.png`): NeocomButton =>
  ({ index: 0, id, btn_type, icon_path, children: 0 });

{
  const onBar = [btn("chat", 10), btn("wallet")];
  const original = [btn("chat", 10), btn("mail"), btn("wallet")];
  const catalog = [
    { id: "chat", btnType: 10, iconPath: "chat.png" },
    { id: "fleet", btnType: 1, iconPath: "fleet.png" },
    { id: "mail", btnType: 1, iconPath: "mail-catalog.png" },
  ];

  const add = addableButtons(onBar, original, catalog);
  const ids = add.map((a) => a.id);

  check("what is already on the bar is not addable", !ids.includes("chat") && !ids.includes("wallet"));
  check("the catalog contributes buttons Original never had", ids.includes("fleet"));
  check("Original contributes buttons the catalog never had", ids.includes("mail"));
  check("the result is sorted by id", ids.join(",") === [...ids].sort().join(","));
  check("no id appears twice", new Set(ids).size === ids.length);

  // Original came from this character's own client, so it wins a conflict.
  const mail = add.find((a) => a.id === "mail")!;
  check("Original wins over the catalog on a conflict", mail.iconPath === "mail.png");
  check("the source is reported", mail.source === "original"
    && add.find((a) => a.id === "fleet")!.source === "catalog");
}

{
  // A character with no Original still gets the whole catalog.
  const add = addableButtons([], [], [{ id: "fleet", btnType: 1, iconPath: "fleet.png" }]);
  check("no Original still yields the catalog", add.length === 1 && add[0].id === "fleet");
}
```

- [ ] **Step 2: Run it to verify it fails**

Run (from `app/`): `npm test`

Expected: FAIL — `Cannot find module './neocom.ts'`.

- [ ] **Step 3: Write the pure part**

Create `app/src/lib/neocom.ts`:

```ts
// Pure helpers for the neocom button list. No DOM, no Svelte — unit-tested in
// neocom.test.ts.
import type { NeocomButton } from "./api";

export interface CatalogButton {
  id: string;
  btnType: number;
  iconPath: string;
}

export interface Addable extends CatalogButton {
  /** Where this button's btnType/iconPath came from. A button the character's
   * own client wrote is more trustworthy than one the bundled catalog supplied. */
  source: "original" | "catalog";
}

/**
 * Buttons the user can add: the character's own `neocomButtonRawDataOriginal`
 * unioned with the bundled catalog, minus whatever is already on the bar.
 *
 * Both halves are needed. Original is a stale snapshot — only ~12% of corpus
 * characters have a bar that is a subset of it, and nine common ids (fleet,
 * accessgroups, corporation, …) appear on bars that Original never listed. The
 * catalog covers those. Original covers the reverse case: a client that knows a
 * button this catalog does not.
 */
export function addableButtons(
  onBar: NeocomButton[],
  original: NeocomButton[],
  catalog: CatalogButton[],
): Addable[] {
  const taken = new Set(onBar.map((b) => b.id));
  const by = new Map<string, Addable>();
  for (const c of catalog) {
    if (!taken.has(c.id)) by.set(c.id, { ...c, source: "catalog" });
  }
  // Original last: it overwrites the catalog entry, because it came from this
  // character's own client.
  for (const o of original) {
    if (!taken.has(o.id)) {
      by.set(o.id, { id: o.id, btnType: o.btn_type, iconPath: o.icon_path, source: "original" });
    }
  }
  return [...by.values()].sort((a, b) => a.id.localeCompare(b.id));
}
```

- [ ] **Step 4: Run it to verify it passes**

Run: `npm test`

Expected: PASS.

- [ ] **Step 5: Write the component**

Create `app/src/lib/NeocomButtons.svelte`. Match `HudPanel.svelte`'s conventions — 12px panel type, `disabled` when `readOnly`, comments that say *why*:

```svelte
<script lang="ts">
  import type { NeocomBar } from "$lib/api";
  import { addableButtons, type CatalogButton } from "$lib/neocom";
  import CATALOG from "$lib/data/neocom-buttons.json";

  let { bar, readOnly, onReorder, onRemove, onAdd, onReset }: {
    bar: NeocomBar;
    readOnly: boolean;
    /** A full permutation of the current indices — the backend rejects anything else. */
    onReorder: (order: number[]) => void;
    onRemove: (index: number) => void;
    onAdd: (id: string, btnType: number, iconPath: string) => void;
    onReset: () => void;
  } = $props();

  const addable = $derived(addableButtons(bar.buttons, bar.original, CATALOG as CatalogButton[]));
  // Reset needs a baseline to reset TO; a character whose client never wrote
  // one gets a disabled button rather than a backend error.
  const canReset = $derived(bar.original.length > 0);

  /** Swap two neighbours and send the whole ordering. */
  function move(index: number, delta: number) {
    const order = bar.buttons.map((b) => b.index);
    const to = index + delta;
    if (to < 0 || to >= order.length) return;
    [order[index], order[to]] = [order[to], order[index]];
    onReorder(order);
  }

  let addChoice = $state("");
  function doAdd() {
    const pick = addable.find((a) => a.id === addChoice);
    if (!pick) return;
    addChoice = "";
    onAdd(pick.id, pick.btnType, pick.iconPath);
  }
</script>

<div class="buttons">
  <p class="head">Buttons</p>
  {#each bar.buttons as b (b.index)}
    <div class="row">
      <span class="id" title={b.icon_path}>{b.id}</span>
      {#if b.children > 0}<span class="badge">{b.children}</span>{/if}
      <button class="mv" disabled={readOnly || b.index === 0} onclick={() => move(b.index, -1)} aria-label="Move {b.id} up">↑</button>
      <button class="mv" disabled={readOnly || b.index === bar.buttons.length - 1} onclick={() => move(b.index, 1)} aria-label="Move {b.id} down">↓</button>
      <button class="rm" disabled={readOnly} onclick={() => onRemove(b.index)} aria-label="Remove {b.id}">✕</button>
    </div>
  {/each}

  {#if addable.length > 0}
    <div class="row">
      <!-- Native select: give it explicit dark colours, or it renders light in
           this WebView2 app (standing project note). -->
      <select bind:value={addChoice} disabled={readOnly} aria-label="Add a neocom button">
        <option value="">Add…</option>
        {#each addable as a (a.id)}
          <option value={a.id}>{a.id}</option>
        {/each}
      </select>
      <button disabled={readOnly || addChoice === ""} onclick={doAdd}>Add</button>
    </div>
  {/if}

  <button
    class="reset"
    disabled={readOnly || !canReset}
    title={canReset ? "Replace the bar with the client's own original" : "This character has no original bar recorded"}
    onclick={() => { if (confirm("Reset the neocom to the client's original buttons?")) onReset(); }}>
    Reset to original
  </button>
</div>

<style>
  .buttons {
    margin: 0.3rem 0 0.2rem;
  }
  .head {
    margin: 0 0 0.2rem;
    color: var(--fg-dim);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.2rem;
    margin-bottom: 1px;
  }
  .id {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    color: var(--fg-dim);
    font-size: 10px;
  }
  .mv, .rm {
    padding: 0 0.25rem;
  }
  /* Native controls render light in WebView2 unless told otherwise. */
  select, option {
    background: var(--bg-panel);
    color: inherit;
    border: 1px solid #444;
    flex: 1;
    min-width: 0;
  }
  .reset {
    margin-top: 0.3rem;
    width: 100%;
  }
</style>
```

Check `app.css` for the custom properties used above (`--fg-dim`, `--bg-panel`); if a name differs, use the real one rather than inventing it.

- [ ] **Step 6: Render it inside HudPanel's Neocom group**

`HudPanel.svelte` keeps its one responsibility (the scalar field table) and renders the list as a child. Add to its props:

```ts
    /** The neocom bar, when a character file is open. Rendered inside the
     * Neocom group so the bar the user clicks on the canvas and the buttons
     * they edit are the same object. */
    neocom = null,
    onNeocomReorder,
    onNeocomRemove,
    onNeocomAdd,
    onNeocomReset,
```

with the matching types (`neocom?: NeocomBar | null` and the four callbacks, same signatures as `NeocomButtons.svelte`'s props), import the component, and inside the `{#each GROUPS}` block after the rows loop:

```svelte
      {#if g.kind === "neocom" && neocom}
        <NeocomButtons
          bar={neocom}
          {readOnly}
          onReorder={onNeocomReorder}
          onRemove={onNeocomRemove}
          onAdd={onNeocomAdd}
          onReset={onNeocomReset} />
      {/if}
```

- [ ] **Step 7: Load and dispatch in LayoutView**

In `app/src/lib/LayoutView.svelte`, beside the existing `hud` state and its load:

```ts
  let neocom = $state<NeocomBar | null>(null);
```

In `load()`, beside `hud = await api.hud().catch(() => null);`:

```ts
  // Same tolerance as the HUD: an account file opened on its own, or a document
  // with no neocom key, must not take the canvas down with it.
  neocom = await api.neocomBar().catch(() => null);
```

And the four dispatchers, beside `setHud`:

```ts
  /** Run a neocom command and take its refreshed projection. The bar lives in
   * the character document, so the char slot is what goes dirty. */
  async function runNeocom(p: Promise<NeocomBar>) {
    try {
      neocom = await p;
      onDirty("char");
    } catch (e) {
      await message(errMessage(e), { title: "Neocom edit failed", kind: "error" });
    }
  }
```

Pass the four through to `HudPanel`:

```svelte
          {neocom}
          onNeocomReorder={(order) => runNeocom(api.neocomReorder(order))}
          onNeocomRemove={(i) => runNeocom(api.neocomRemove(i))}
          onNeocomAdd={(id, t, icon) => runNeocom(api.neocomAdd(id, t, icon))}
          onNeocomReset={() => runNeocom(api.neocomReset())}
```

Add `NeocomBar` to the `$lib/api` type import.

- [ ] **Step 8: Write the component test**

Create `app/src/lib/NeocomButtons.spec.ts`, copying the mount/query helpers from `HudPanel.spec.ts` (read it first — it is the model for this file):

```ts
import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import NeocomButtons from "./NeocomButtons.svelte";
import type { NeocomBar, NeocomButton } from "./api";

const btn = (index: number, id: string, children = 0): NeocomButton =>
  ({ index, id, btn_type: 1, icon_path: `${id}.png`, children });

const bar = (buttons: NeocomButton[], original: NeocomButton[] = []): NeocomBar =>
  ({ buttons, original });

const props = (b: NeocomBar, readOnly = false) => ({
  bar: b, readOnly,
  onReorder: vi.fn(), onRemove: vi.fn(), onAdd: vi.fn(), onReset: vi.fn(),
});

describe("NeocomButtons", () => {
  test("lists the buttons in bar order", () => {
    render(NeocomButtons, props(bar([btn(0, "chat"), btn(1, "mail"), btn(2, "wallet")])));
    const ids = screen.getAllByTitle(/\.png$/).map((e) => e.textContent);
    expect(ids).toEqual(["chat", "mail", "wallet"]);
  });

  test("the end rows cannot move past the ends", () => {
    render(NeocomButtons, props(bar([btn(0, "chat"), btn(1, "mail")])));
    expect(screen.getByLabelText("Move chat up")).toBeDisabled();
    expect(screen.getByLabelText("Move mail down")).toBeDisabled();
    expect(screen.getByLabelText("Move chat down")).toBeEnabled();
  });

  test("moving a button sends the whole permutation", async () => {
    const p = props(bar([btn(0, "chat"), btn(1, "mail"), btn(2, "wallet")]));
    render(NeocomButtons, p);
    screen.getByLabelText("Move wallet up").click();
    expect(p.onReorder).toHaveBeenCalledWith([0, 2, 1]);
  });

  test("the add list excludes what is already on the bar", () => {
    render(NeocomButtons, props(bar([btn(0, "chat")], [btn(0, "chat"), btn(1, "mail")])));
    const options = screen.getAllByRole("option").map((o) => o.textContent);
    expect(options).toContain("mail");
    expect(options).not.toContain("chat");
  });

  test("reset is disabled when the character has no original", () => {
    render(NeocomButtons, props(bar([btn(0, "chat")], [])));
    expect(screen.getByText("Reset to original")).toBeDisabled();
  });

  test("read-only disables every control", () => {
    render(NeocomButtons, props(bar([btn(0, "chat"), btn(1, "mail")], [btn(0, "wallet")]), true));
    expect(screen.getByLabelText("Move chat down")).toBeDisabled();
    expect(screen.getByLabelText("Remove chat")).toBeDisabled();
    expect(screen.getByText("Reset to original")).toBeDisabled();
  });
});
```

If a query helper does not match how `HudPanel.spec.ts` does it, follow that file rather than this snippet — it is the working example in this repo.

- [ ] **Step 9: Verify**

Run from `app/`: `npm test`, `npm run check`, `npm run build`

Expected: node suites and vitest green; check 0 errors and no new warning in a changed file; build succeeds.

- [ ] **Step 10: Commit**

```bash
git add app/src/lib/neocom.ts app/src/lib/neocom.test.ts app/src/lib/NeocomButtons.svelte app/src/lib/NeocomButtons.spec.ts app/src/lib/HudPanel.svelte app/src/lib/LayoutView.svelte
git commit -m "Edit the neocom buttons from the layout panel"
```

---

### Task 6: The layout batch aspect carries the buttons

**Files:**
- Modify: `crates/settings-model/src/batch.rs` (the `Category` enum and `key_path`, lines 19-38; tests at the end)
- Modify: `app/src-tauri/src/ops.rs` (`aspect_writes`, around line 116)

**Interfaces:**
- Consumes: nothing from the other tasks — this is independent of Tasks 1-5 and can be reviewed on its own.
- Produces: `Category::NeocomButtons`.

- [ ] **Step 1: Write the failing tests**

In `crates/settings-model/src/batch.rs`'s test module, following the shape of the existing `Category::Keybinds` cases (read `extract_categories`/`apply_to_tree`'s existing tests first and match their fixture style):

```rust
    #[test]
    fn neocom_buttons_extract_and_apply_across_files() {
        let source = Value::Dict(vec![(b("ui"), Value::Dict(vec![
            (b("neocomButtonRawData"), Value::Tuple(vec![ts(), Value::List(vec![b("SOURCE-BAR")])])),
            (b("neocomButtonRawDataOriginal"), Value::Tuple(vec![ts(), Value::Tuple(vec![b("SOURCE-ORIGINAL")])])),
        ]))]);
        let mut target = Value::Dict(vec![(b("ui"), Value::Dict(vec![
            (b("neocomButtonRawData"), Value::Tuple(vec![ts(), Value::List(vec![b("TARGET-BAR")])])),
            (b("neocomButtonRawDataOriginal"), Value::Tuple(vec![ts(), Value::Tuple(vec![b("TARGET-ORIGINAL")])])),
        ]))]);

        let extracted = extract_categories(&source, &[Category::NeocomButtons]);
        apply_to_tree(&mut target, &extracted);

        let dumped = format!("{target:?}");
        assert!(dumped.contains("SOURCE-BAR"), "the source bar was not copied");
        assert!(!dumped.contains("TARGET-BAR"), "the target bar was not replaced");
        // The baseline is the TARGET's own client record: copying the source's
        // would corrupt what "reset to original" means on that character.
        assert!(dumped.contains("TARGET-ORIGINAL"), "the target's Original was overwritten");
        assert!(!dumped.contains("SOURCE-ORIGINAL"), "the source's Original leaked across");
    }
```

Add to `app/src-tauri/src/ops.rs`'s test module, beside the existing `aspect_writes` tests (around line 1589):

```rust
    #[test]
    fn the_layout_aspect_carries_the_neocom_buttons() {
        let w = aspect_writes(&[Aspect::Layout]);
        assert!(w.char_categories.contains(&Category::Layout));
        assert!(w.char_categories.contains(&Category::NeocomButtons),
                "a layout copy must bring the neocom bar with it");
        assert!(w.account_categories.is_empty(), "the neocom bar is character-side");
        assert!(w.copies_char_geometry(), "the resolution warning still applies");
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p settings-model --lib batch` and `cargo test --manifest-path app/src-tauri/Cargo.toml aspect`

Expected: compile failure — `no variant named NeocomButtons found for enum Category`.

- [ ] **Step 3: Implement**

In `crates/settings-model/src/batch.rs`, add the variant to the enum:

```rust
    Keybinds,
    NeocomButtons,
```

and its key path:

```rust
            Category::Keybinds => &[b"cmd", b"customCmds"],
            // Character-side: the neocom BAR is per account (neocomWidth), its
            // BUTTONS are per character. Original is deliberately not a category
            // — it is the target's own client baseline.
            Category::NeocomButtons => &[b"ui", b"neocomButtonRawData"],
```

In `app/src-tauri/src/ops.rs`'s `aspect_writes`, extend the Layout arm — the same one-aspect-to-two-categories shape `Aspect::Overview` already uses:

```rust
            Aspect::Layout => {
                char_categories.push(Category::Layout);
                char_categories.push(Category::NeocomButtons);
            }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p settings-model` and `cargo test --manifest-path app/src-tauri/Cargo.toml`

Expected: both green. Watch for a non-exhaustive `match` on `Category` elsewhere in either crate — if one appears, handle the new variant rather than adding a catch-all arm.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/batch.rs app/src-tauri/src/ops.rs
git commit -m "Carry the neocom buttons with a layout batch copy"
```

---

### Task 7: Corpus gate and the format record

**Files:**
- Create: `crates/settings-model/tests/neocom_corpus.rs`
- Modify: `docs/format-notes.md` (a new "Neocom buttons" section)
- Modify: `docs/settings-field-reference.md` (the Neocom paragraph, around line 271)

**Interfaces:**
- Consumes: Task 2's public API (`project_neocom`, `neocom_reorder`).
- Produces: nothing.

- [ ] **Step 1: Write the corpus gate**

The file is named `*_corpus.rs`, not `*_realshape.rs`: in this repo the
`*_realshape.rs` tests are fully synthetic fixtures shaped like real files,
while the `*_corpus.rs` ones walk the actual corpus through
`crates/settings-model/tests/common/mod.rs`. This one walks the corpus.

The walker's API, already checked — `common::char_files()` yields `CorpusFile`
with fields `path: PathBuf`, `bytes: Vec<u8>`, `identity_safe: bool`,
`synthetic: bool`. There is **no** pre-decoded `value` and no `name`: decode
`f.bytes` yourself and use `f.path.display()` in messages, exactly as
`hud_corpus.rs` does. `common::real_corpus_present()` reports whether the real
tree is checked out (CI has only the synthetic one).

Create `crates/settings-model/tests/neocom_corpus.rs`:

```rust
//! Real-data guard for the neocom projection. Real files intern the four state
//! key names and the repeated icon paths as `Shared`/`Ref`, and 11 corpus
//! buttons carry an `id` that is a `Tuple(bytes, None)` rather than plain bytes
//! — a reader that matched `Value::Bytes` directly would pass every hand-built
//! unit test in `neocom.rs` and still read nothing from a real file.
//!
//! Skips silently when the real corpus is not checked out.

mod common;

use settings_model::{neocom_reorder, project_neocom, NeocomError};

#[test]
fn every_corpus_character_file_projects_or_reports_no_bar() {
    let mut projected = 0;
    for f in common::char_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        match project_neocom(&doc) {
            Ok(bar) => {
                projected += 1;
                for b in &bar.buttons {
                    assert!(!b.id.is_empty(), "{}: a button projected with an empty id", f.path.display());
                    assert!(b.btn_type > 0, "{}: button {} projected btnType 0", f.path.display(), b.id);
                }
            }
            // A file with no neocom key at all is legitimate (spec §2: 154 of
            // 4,215 corpus character files have none).
            Err(NeocomError::NoBar) | Err(NeocomError::NoUi) => {}
            Err(e) => panic!("{}: neocom projection failed: {e}", f.path.display()),
        }
    }
    if common::real_corpus_present() {
        assert!(projected > 0, "the real corpus is present but nothing projected");
    }
}

#[test]
fn a_reorder_of_a_real_bar_round_trips_through_the_codec() {
    for f in common::char_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        let Ok(bar) = project_neocom(&doc) else { continue };
        if bar.buttons.len() < 2 { continue }
        let mut v = doc;
        // Reverse the bar: the most disruptive permutation there is.
        let order: Vec<usize> = (0..bar.buttons.len()).rev().collect();
        neocom_reorder(&mut v, &order).expect("reorder a real bar");
        let v = blue_marshal::reshare(&v); // what the app layer does before saving
        let bytes = blue_marshal::encode(&v).expect("edited real file still encodes");
        assert_eq!(blue_marshal::decode(&bytes).unwrap(), v, "{}: reorder broke the round trip", f.path.display());

        let after = project_neocom(&v).expect("re-project after reorder");
        let before: Vec<&str> = bar.buttons.iter().map(|b| b.id.as_str()).rev().collect();
        let now: Vec<&str> = after.buttons.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(before, now, "{}: the bar did not come back reversed", f.path.display());
    }
}
```

If `common::char_files()` is not the exact helper name, read the module and use
whichever one yields character files (`hud_corpus.rs` calls it) — do not fall
back to walking the corpus directory yourself.

- [ ] **Step 2: Run it**

Run: `cargo test -p settings-model --test neocom_corpus`

Expected: PASS against the real corpus present on this machine.

- [ ] **Step 3: Record the format**

Add a section to `docs/format-notes.md` in the style of its existing experiment write-ups (see "Window stacks"), titled **"Neocom buttons (corpus survey, not an in-game capture)"**, recording spec §2 and §2.1 verbatim in substance: the location and wrapper shape, the always-`utillib.KeyVal` class, the invariant four-key state, the four `btnType` values and what each tracks, the `children` shapes, the 25 ids, the `Tuple(bytes, None)` id anomaly, and — with its numbers — that `Original` is a stale snapshot rather than a catalog. Say plainly that these came from a survey of 4,215 corpus character files (4,061 carrying the key, 43,430 buttons) and that the in-game capture the milestone spec anticipated proved unnecessary.

In `docs/settings-field-reference.md`, replace the Neocom paragraph's "Instances are `utillib.KeyVal`-style; not decoded further here." with a one-line pointer to the new format-notes section and a note that the editor now edits these (as the other shipped-editor entries in that file do).

- [ ] **Step 4: Verify and commit**

Run: `cargo test -p settings-model`

```bash
git add crates/settings-model/tests/neocom_corpus.rs docs/format-notes.md docs/settings-field-reference.md
git commit -m "Guard the neocom projection against real files and record the format"
```

---

### Task 8: Whole-branch review and the live smoke

**Files:** none by default — this task produces fixes only if the review or the smoke finds something.

- [ ] **Step 1: Run every suite from a clean state**

From the repo root: `cargo test -p settings-model`, `cargo test --manifest-path app/src-tauri/Cargo.toml`
From `app/`: `npm test`, `npm run check`, `npm run build`

Expected: all green. Record the actual output; do not claim a pass without it.

- [ ] **Step 2: Whole-branch code review**

Use the `superpowers:requesting-code-review` skill against `master..HEAD`. Fix anything blocking; anything non-blocking goes into `docs/small-tasks.md` as ship-as-debt, in the style of the existing entries.

- [ ] **Step 3: Live smoke on a real character**

EVE writes its settings on **logout** — log the character out before saving from the editor, or the client overwrites the file on exit.

1. Open a character, select the neocom bar on the canvas, confirm the button list matches the bar in-game, top to bottom.
2. Move a button up and down with ↑/↓; save; log in; confirm the in-game bar matches.
3. Remove a button; confirm it is gone in-game and that nothing else moved.
4. Add it back from the dropdown; confirm it reappears **and works when clicked** — a wrong `btnType` or `iconPath` would show a dead or blank icon, which is the failure mode this step exists to catch. Add one sourced from the *catalog* (not from `Original`) specifically, since that is the path with no per-character provenance.
5. Reset to original; confirm the bar returns to the client's own set.
6. Batch-copy the **Window layout** aspect to a second character; confirm that character's neocom bar now matches the source's, and that its own *Reset to original* still restores *its* baseline rather than the source's.
7. A character whose file has no `Original`: the Reset button is disabled rather than erroring.

- [ ] **Step 4: Fix what the smoke finds, then re-verify**

Any fix gets its own commit and re-runs Step 1's suites.

---

## Notes for the reviewer

- **Index-keyed commands are deliberate**, not an oversight: 11 corpus buttons carry a `Tuple(bytes, None)` id, so ids are neither unique nor always well-formed. See spec §4.
- **`reorder` validates before inlining** so a rejected order leaves the document untouched — `inline_all` rewrites the whole tree, and doing it before the validation would mean a rejected command still dirtied the file.
- **`neocomButtonRawDataOriginal` is never written**, by any command or by the batch category. It is the character's own client baseline and the only thing *Reset to original* can mean.
- **The catalog is frontend-owned on purpose.** The backend writes the three fields it is handed rather than embedding and parsing a second copy of the same table; the union with `Original` happens where both halves are in hand.
