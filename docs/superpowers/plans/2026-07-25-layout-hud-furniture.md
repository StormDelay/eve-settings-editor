# Layout HUD Furniture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Project EVE's screen furniture — the ship HUD (capacitor + module racks), the detached fighter UI, the neocom and the notification badge — onto the layout canvas as draggable non-window elements, with exact numeric fields beside them.

**Architecture:** A new read-only projection module `crates/settings-model/src/hud.rs` reads six scalars and two point tuples spread across the character file (`windows`, `ui` sections) and the account file (same section names, different keys), each entry carrying a resolved `NodePath`. One setter in the same module writes a value, minting the `(timestamp, value)` leaf when the key is absent. Two ops commands (`hud_layout`, `set_hud_value`) return the fresh projection, so the frontend never builds a mutation. On the frontend, one pure function `hudRects` in `layout.ts` turns the projection into data-pixel rectangles that the existing canvas scaling draws, the existing drag state machine gains a `"furniture"` variant, and a new `HudPanel.svelte` holds the fields.

**Tech Stack:** Rust (workspace crates `blue-marshal`, `settings-model`, `app/src-tauri`), Tauri 2, SvelteKit 5 with runes, `node --test` for frontend unit tests.

**Spec:** `docs/superpowers/specs/2026-07-25-layout-hud-furniture-design.md`

## Global Constraints

- **Worktree:** work in `C:\Users\antoi\claude\eve-settings-editor-layout` on branch `layout-hud-furniture`. The primary checkout is in use by another session — do not touch it.
- **No new dependencies.** `settings-model` is dependency-free; frontend tests are zero-dep `node --test`.
- **No personal data in tests or docs.** Invented window ids and coordinates only — never a real character id or name.
- **Every read path resolves `Shared`/`Ref`.** Build a `SharedTable` with `collect_shared` once, then compare and read through `effective`. `treewalk::is_bytes`, `child_dict` and `timestamped_dict` do **not** resolve keys, and real account files store the root `ui` section under a `Ref` key — using them for section lookup projects nothing.
- **Structural edits inline first, ops reshares after.** `treewalk::inline_all(root)` before inserting a new dict entry; `blue_marshal::reshare` in the ops wrapper. A plain scalar overwrite does neither.
- **New native controls get explicit dark `background` and `color`** — WebView2 renders them light otherwise.
- **Commits:** sentence-case subject, no attribution trailers of any kind.
- **CI gates:** from `app/`: `npm run check`, `npm test`, `npm run build`. From the repo root: `cargo test --workspace`.

## File Structure

| File | Responsibility |
|---|---|
| `crates/settings-model/src/hud.rs` (new) | The field table, the projection, and the setter. All HUD format knowledge. |
| `crates/settings-model/src/lib.rs` | `mod hud;` + re-exports. |
| `crates/settings-model/src/windows.rs` | One-line change: `pinnedWindows` as the 8th bool flag. |
| `app/src-tauri/src/ops.rs` | `hud_layout` / `set_hud_value` commands (lock, read-only check, reshare, re-project). |
| `app/src-tauri/src/lib.rs` | Two `#[tauri::command]` wrappers + `generate_handler!` registration. |
| `app/src/lib/api.ts` | `Hud`, `HudEntry`, `HudScope`, `HudKind` types + two invoke wrappers. |
| `app/src/lib/layout.ts` | `hudRects` and the nominal-size table — all furniture geometry maths, pure. |
| `app/src/lib/layout.test.ts` | `hudRects` cases. |
| `app/src/lib/LayoutView.svelte` | Draw furniture, drag it, own the `hud` state, call `onDirty`. |
| `app/src/lib/HudPanel.svelte` (new) | The numeric/checkbox fields. |
| `app/src/routes/+page.svelte` | Pass `onDirty` to `LayoutView`. |
| `docs/format-notes.md` | New "HUD anchors" section, written from the live smoke. |

---

### Task 1: `pinnedWindows` as the eighth window flag

**Files:**
- Modify: `crates/settings-model/src/windows.rs:19-27` (the `BOOL_FLAGS` const) and `:502` (a length assertion)
- Test: `crates/settings-model/src/windows.rs` (the inline `mod tests`)

**Interfaces:**
- Consumes: nothing.
- Produces: nothing new — `BOOL_FLAGS` grows to 8, so every `WindowRect.flags` gains a `pinnedWindows` entry and `WindowPanel.svelte` lists it with no frontend change.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `crates/settings-model/src/windows.rs`, after `a_missing_flag_dict_is_unavailable`:

```rust
    /// pinnedWindows is a per-window bool dict like the other seven (352/384
    /// character files in the 2026-07-22 corpus carry it).
    #[test]
    fn pinned_windows_is_projected_as_a_flag() {
        let doc = Value::Dict(vec![(
            Value::Bytes(b"windows".to_vec()),
            Value::Dict(vec![
                (
                    Value::Bytes(b"windowSizesAndPositions_1".to_vec()),
                    Value::Tuple(vec![
                        ts(),
                        Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), geom(1, 2, 3, 4, 2560, 1440))]),
                    ]),
                ),
                (
                    Value::Bytes(b"pinnedWindows".to_vec()),
                    Value::Tuple(vec![
                        ts(),
                        Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Bool(true))]),
                    ]),
                ),
            ]),
        )]);
        let wl = window_layout(&doc);
        let pinned = flag(&wl.windows[0], "pinnedWindows");
        assert!(pinned.value);
        match &pinned.set {
            SetTarget::Set { path } => assert_eq!(resolve(&doc, path), Some(&Value::Bool(true))),
            other => panic!("expected Set, got {other:?}"),
        }
    }
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cargo test -p settings-model pinned_windows
```

Expected: FAIL — `flag` panics with "flag present" because `pinnedWindows` is not in `BOOL_FLAGS`.

- [ ] **Step 3: Add the flag**

In `crates/settings-model/src/windows.rs`, change the const (note the arity and the doc comment):

```rust
/// The eight boolean per-window flags (see docs/format-notes.md). `stacksWindows`
/// is handled separately — its value is a stack id, not a bool.
const BOOL_FLAGS: [&str; 8] = [
    "openWindows",
    "collapsedWindows",
    "minimizedWindows",
    "lockedWindows",
    "compactWindows",
    "isOverlayedWindows",
    "isLightBackgroundWindows",
    "pinnedWindows",
];
```

- [ ] **Step 4: Fix the flag-count assertion the change breaks**

In `open_and_present_flags_carry_set_targets`, `assert_eq!(ov.flags.len(), 7);` becomes:

```rust
        assert_eq!(ov.flags.len(), 8);
```

- [ ] **Step 5: Run the whole crate's tests**

```bash
cargo test -p settings-model
```

Expected: PASS, all tests.

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/src/windows.rs
git commit -m "Project the pinnedWindows flag"
```

---

### Task 2: `hud.rs` — field table, shared-resolving lookup, character-side projection

**Files:**
- Create: `crates/settings-model/src/hud.rs`
- Modify: `crates/settings-model/src/lib.rs:14-21` (add `pub mod hud;`) and the re-export block at `:30`
- Test: `crates/settings-model/src/hud.rs` (inline `mod tests`)

**Interfaces:**
- Consumes: `crate::treewalk::{collect_shared, effective, is_bytes, unwrap_shared, Entries, SharedTable}` (all `pub(crate)`), `crate::windows::SetTarget`, `crate::path::{NodePath, Step}`.
- Produces:
  - `pub enum HudScope { Char, Account }` — serialized `"char"` / `"account"`.
  - `pub enum HudKind { Float, Int, Bool }` — serialized `"float"` / `"int"` / `"bool"`.
  - `pub struct HudEntry { name: String, kind: HudKind, value: Option<String>, default: String, scope: HudScope, set: SetTarget }`.
  - `pub struct Hud { entries: Vec<HudEntry> }`.
  - `pub fn project_hud(char_root: &Value, user_root: Option<&Value>) -> Hud`.
  - Entry names, fixed and depended on by Tasks 4–8: `ship_offset`, `fighter_x`, `fighter_y`, `badge_x`, `badge_y`, `ship_top`, `fighter_detached`, `fighter_shown`, `neocom_width`. Entries come back in that order.

- [ ] **Step 1: Write the failing tests**

Create `crates/settings-model/src/hud.rs` containing only this test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::resolve;
    use blue_marshal::Value;

    fn ts() -> Value {
        Value::Long(vec![0u8; 8])
    }

    fn b(s: &str) -> Value {
        Value::Bytes(s.as_bytes().to_vec())
    }

    /// (timestamp, value) — the file-wide value-wrapper convention.
    fn wrapped(v: Value) -> Value {
        Value::Tuple(vec![ts(), v])
    }

    fn point(x: i64, y: i64) -> Value {
        Value::Tuple(vec![Value::Int(x), Value::Int(y)])
    }

    /// A character document with every character-scoped HUD key present.
    fn char_doc() -> Value {
        Value::Dict(vec![
            (
                b("windows"),
                Value::Dict(vec![(b("shipuialignleftoffset"), wrapped(Value::Float(-189.0)))]),
            ),
            (
                b("ui"),
                Value::Dict(vec![
                    (b("fightersDetachedPosition"), wrapped(point(326, 54))),
                    (b("notification_badge_offset"), wrapped(point(2519, 131))),
                ]),
            ),
        ])
    }

    fn entry<'a>(hud: &'a Hud, name: &str) -> &'a HudEntry {
        hud.entries.iter().find(|e| e.name == name).expect("entry present")
    }

    #[test]
    fn projects_the_ship_offset_with_a_resolvable_path() {
        let doc = char_doc();
        let hud = project_hud(&doc, None);
        let e = entry(&hud, "ship_offset");
        assert_eq!(e.value.as_deref(), Some("-189"));
        assert_eq!(e.kind, HudKind::Float);
        assert_eq!(e.scope, HudScope::Char);
        match &e.set {
            SetTarget::Set { path } => assert_eq!(resolve(&doc, path), Some(&Value::Float(-189.0))),
            other => panic!("expected Set, got {other:?}"),
        }
    }

    #[test]
    fn projects_each_point_axis_to_its_own_tuple_element() {
        let doc = char_doc();
        let hud = project_hud(&doc, None);
        assert_eq!(entry(&hud, "fighter_x").value.as_deref(), Some("326"));
        assert_eq!(entry(&hud, "fighter_y").value.as_deref(), Some("54"));
        assert_eq!(entry(&hud, "badge_x").value.as_deref(), Some("2519"));
        match &entry(&hud, "fighter_y").set {
            SetTarget::Set { path } => assert_eq!(resolve(&doc, path), Some(&Value::Int(54))),
            other => panic!("expected Set, got {other:?}"),
        }
    }

    /// The real-file case that a bare `is_bytes` lookup misses: the root section
    /// key is a Ref whose byte-string definition lives elsewhere in the tree.
    #[test]
    fn a_ref_keyed_section_still_resolves() {
        let doc = Value::Dict(vec![
            (
                Value::Ref(7),
                Value::Dict(vec![(b("fightersDetachedPosition"), wrapped(point(10, 20)))]),
            ),
            // The Shared definition of b"ui", stored later in the stream.
            (b("elsewhere"), Value::Shared { slot: 7, value: Box::new(b("ui")) }),
        ]);
        let hud = project_hud(&doc, None);
        assert_eq!(entry(&hud, "fighter_x").value.as_deref(), Some("10"));
    }

    #[test]
    fn an_absent_key_reports_the_default_and_an_insert_target() {
        // `windows` exists but holds no HUD key at all.
        let doc = Value::Dict(vec![(b("windows"), Value::Dict(vec![]))]);
        let hud = project_hud(&doc, None);
        let e = entry(&hud, "ship_offset");
        assert!(e.value.is_none());
        assert_eq!(e.default, "0");
        assert!(matches!(e.set, SetTarget::Insert { .. }));
    }

    #[test]
    fn a_missing_section_is_unavailable() {
        let doc = Value::Dict(vec![(b("audio"), Value::Dict(vec![]))]);
        let hud = project_hud(&doc, None);
        assert!(matches!(entry(&hud, "ship_offset").set, SetTarget::Unavailable));
        assert!(entry(&hud, "ship_offset").value.is_none());
    }

    #[test]
    fn a_malformed_point_tuple_reads_as_absent() {
        let doc = Value::Dict(vec![(
            b("ui"),
            // One element instead of two.
            Value::Dict(vec![(b("fightersDetachedPosition"), wrapped(Value::Tuple(vec![Value::Int(1)])))]),
        )]);
        let hud = project_hud(&doc, None);
        assert!(entry(&hud, "fighter_y").value.is_none());
    }

    #[test]
    fn a_shared_leaf_value_is_read_through() {
        let doc = Value::Dict(vec![(
            b("windows"),
            Value::Dict(vec![(
                b("shipuialignleftoffset"),
                Value::Shared { slot: 3, value: Box::new(wrapped(Value::Float(-12.0))) },
            )]),
        )]);
        let hud = project_hud(&doc, None);
        assert_eq!(entry(&hud, "ship_offset").value.as_deref(), Some("-12"));
    }
}
```

- [ ] **Step 2: Run and watch it fail to compile**

```bash
cargo test -p settings-model hud
```

Expected: the test module does not compile — `project_hud`, `Hud`, `HudEntry`, `HudKind`, `HudScope` are undefined. (The module also is not yet declared in `lib.rs`, so nothing runs.)

- [ ] **Step 3: Write the module above the test module**

Prepend to `crates/settings-model/src/hud.rs`:

```rust
//! Read-only projection of EVE's screen furniture — the ship HUD's horizontal
//! offset, the detached fighter UI and notification badge positions, the neocom
//! width, and the account-level HUD toggles. Every writable field carries the
//! resolved `NodePath` a `set_scalar` mutation targets. All format knowledge
//! (which section, which key, which tuple element, the `(timestamp, value)`
//! wrapper) lives here. The setter is `set_hud_value`; nothing else mutates.
//!
//! Values span two files: the anchors are per character, the toggles and the
//! neocom width are per account. See docs/format-notes.md, "HUD anchors".

use blue_marshal::Value;
use serde::Serialize;

use crate::path::{NodePath, Step};
use crate::treewalk::{collect_shared, effective, is_bytes, unwrap_shared, Entries, SharedTable};
use crate::windows::SetTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HudScope {
    Char,
    Account,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HudKind {
    Float,
    Int,
    Bool,
}

#[derive(Debug, Serialize)]
pub struct HudEntry {
    pub name: String,
    pub kind: HudKind,
    /// `None` when the key is absent or holds an unexpected wire kind — the UI
    /// then shows `default`.
    pub value: Option<String>,
    pub default: String,
    pub scope: HudScope,
    pub set: SetTarget,
}

#[derive(Debug, Serialize)]
pub struct Hud {
    pub entries: Vec<HudEntry>,
}

/// One editable value. `elem` indexes into an `(x, y)` tuple; `None` means the
/// leaf itself. Defaults are EVE's built-in behaviour when the key is absent
/// (assumed, confirmed in the slice's live smoke).
struct Field {
    name: &'static str,
    section: &'static [u8],
    key: &'static [u8],
    elem: Option<usize>,
    kind: HudKind,
    default: &'static str,
    scope: HudScope,
}

const FIELDS: [Field; 9] = [
    Field { name: "ship_offset", section: b"windows", key: b"shipuialignleftoffset",
            elem: None, kind: HudKind::Float, default: "0", scope: HudScope::Char },
    Field { name: "fighter_x", section: b"ui", key: b"fightersDetachedPosition",
            elem: Some(0), kind: HudKind::Int, default: "0", scope: HudScope::Char },
    Field { name: "fighter_y", section: b"ui", key: b"fightersDetachedPosition",
            elem: Some(1), kind: HudKind::Int, default: "0", scope: HudScope::Char },
    Field { name: "badge_x", section: b"ui", key: b"notification_badge_offset",
            elem: Some(0), kind: HudKind::Int, default: "0", scope: HudScope::Char },
    Field { name: "badge_y", section: b"ui", key: b"notification_badge_offset",
            elem: Some(1), kind: HudKind::Int, default: "0", scope: HudScope::Char },
    Field { name: "ship_top", section: b"ui", key: b"shipuialigntop",
            elem: None, kind: HudKind::Bool, default: "false", scope: HudScope::Account },
    Field { name: "fighter_detached", section: b"ui", key: b"detachFighterUI",
            elem: None, kind: HudKind::Bool, default: "false", scope: HudScope::Account },
    Field { name: "fighter_shown", section: b"ui", key: b"displayFighterUI",
            elem: None, kind: HudKind::Bool, default: "false", scope: HudScope::Account },
    Field { name: "neocom_width", section: b"windows", key: b"neocomWidth",
            elem: None, kind: HudKind::Int, default: "37", scope: HudScope::Account },
];

pub fn project_hud(char_root: &Value, user_root: Option<&Value>) -> Hud {
    let mut char_shared = SharedTable::new();
    collect_shared(char_root, &mut char_shared);
    let mut user_shared = SharedTable::new();
    if let Some(u) = user_root {
        collect_shared(u, &mut user_shared);
    }

    let entries = FIELDS
        .iter()
        .map(|f| {
            let (root, shared) = match f.scope {
                HudScope::Char => (Some(char_root), &char_shared),
                HudScope::Account => (user_root, &user_shared),
            };
            // No account file open is normal (an unpaired character): the four
            // account fields are then simply not writable.
            let (value, set) = root.map_or((None, SetTarget::Unavailable), |r| probe(r, f, shared));
            HudEntry {
                name: f.name.to_string(),
                kind: f.kind,
                value,
                default: f.default.to_string(),
                scope: f.scope,
                set,
            }
        })
        .collect();
    Hud { entries }
}

fn probe(root: &Value, f: &Field, shared: &SharedTable) -> (Option<String>, SetTarget) {
    let Some((entries, base)) = section(root, f.section, shared) else {
        // The whole section is missing (or unaddressable) — nothing to write to.
        return (None, SetTarget::Unavailable);
    };
    match leaf(entries, &base, f.key, f.elem, shared) {
        Some((v, path)) => match scalar_text(v, f.kind, shared) {
            Some(text) => (Some(text), SetTarget::Set { path }),
            // Key present but the wire kind is not what this field expects:
            // refuse to write rather than clobber it or mint a duplicate key.
            None => (None, SetTarget::Unavailable),
        },
        // Absent: `set_hud_value` mints the `(timestamp, value)` leaf. The
        // parent/key here document the target; the op does the insert, because
        // a generic InsertDictEntry cannot build the timestamp wrapper.
        None => (
            None,
            SetTarget::Insert { parent: base, key: crate::mutate::NewValue::BytesHex(hex(f.key)) },
        ),
    }
}

/// Find a root section by name. Section KEYS are resolved through `Ref`/`Shared`:
/// real account files store the `ui` section under a `Ref` to a byte-string
/// defined later in the stream (the trailing shared-object table makes that
/// legal), which `treewalk::child_dict`'s bare `is_bytes` comparison misses.
/// A section whose VALUE is a `Ref` is reported missing — there is no path step
/// into a ref, so it could be read but never written.
fn section<'a>(root: &'a Value, name: &[u8], shared: &SharedTable<'a>) -> Option<(&'a Entries, NodePath)> {
    let Value::Dict(entries) = effective(root, shared) else { return None };
    let (i, (_, v)) = entries.iter().enumerate().find(|(_, (k, _))| is_bytes(effective(k, shared), name))?;
    let (v, p) = unwrap_shared(v, vec![Step::DictValue(i)]);
    match v {
        Value::Dict(d) => Some((d, p)),
        _ => None,
    }
}

/// Locate `key` in a section and step through the `(timestamp, value)` wrapper
/// (and then into tuple element `elem`, for a point field), returning the scalar
/// and its path. Keys are resolved through `Ref`/`Shared` as in `section`.
fn leaf<'a>(
    entries: &'a Entries,
    base: &NodePath,
    key: &[u8],
    elem: Option<usize>,
    shared: &SharedTable<'a>,
) -> Option<(&'a Value, NodePath)> {
    let (i, (_, v)) = entries.iter().enumerate().find(|(_, (k, _))| is_bytes(effective(k, shared), key))?;
    let mut p = base.clone();
    p.push(Step::DictValue(i));
    let (v, p) = unwrap_shared(v, p);
    // (timestamp, value): take element 1. A bare value is tolerated the way
    // treewalk::timestamped_dict tolerates a bare dict.
    let (v, p) = match v {
        Value::Tuple(items) if items.len() == 2 => {
            let mut q = p;
            q.push(Step::Tuple(1));
            (&items[1], q)
        }
        other => (other, p),
    };
    let Some(ix) = elem else { return Some((v, p)) };
    let (v, p) = unwrap_shared(v, p);
    let Value::Tuple(items) = v else { return None };
    if items.len() != 2 {
        return None; // not an (x, y) point
    }
    let mut q = p;
    q.push(Step::Tuple(ix));
    Some((items.get(ix)?, q))
}

/// The stored value as the text the UI edits, or `None` if the wire kind is not
/// what this field expects. A float is rendered without a trailing `.0` so the
/// panel's number input shows `-189`, not `-189.0`; `set_scalar` keeps the wire
/// kind on write either way.
fn scalar_text(v: &Value, kind: HudKind, shared: &SharedTable) -> Option<String> {
    match (kind, effective(v, shared)) {
        // `format!` prints -189.0 as "-189", which is what the number input wants;
        // set_scalar keeps the leaf's Float wire kind on the way back in.
        (HudKind::Float, Value::Float(f)) => Some(format!("{f}")),
        (HudKind::Int, Value::Int(i)) => Some(i.to_string()),
        (HudKind::Bool, Value::Bool(b)) => Some(b.to_string()),
        _ => None,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
```

- [ ] **Step 4: Declare the module and re-export it**

In `crates/settings-model/src/lib.rs`, add the module beside the others (after `pub mod windows;`):

```rust
pub mod hud;
```

and a re-export beside the `windows` one:

```rust
pub use hud::{project_hud, Hud, HudEntry, HudKind, HudScope};
```

- [ ] **Step 5: Run the tests**

```bash
cargo test -p settings-model hud
```

Expected: PASS — 7 tests.

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/src/hud.rs crates/settings-model/src/lib.rs
git commit -m "Project the character-side HUD anchors"
```

---

### Task 3: Account-side entries

**Files:**
- Modify: `crates/settings-model/src/hud.rs` (tests only — the projection already covers both scopes)
- Test: `crates/settings-model/src/hud.rs`

**Interfaces:**
- Consumes: `project_hud(char_root, user_root)` from Task 2.
- Produces: nothing new. This task proves the account half works and pins the two structural facts it depends on: `neocomWidth` lives under the account file's `windows` section, the three toggles under its `ui` section.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/settings-model/src/hud.rs`:

```rust
    /// An account document: neocomWidth under `windows`, the toggles under `ui`
    /// — and, as in real files, `ui` keyed by a Ref.
    fn user_doc() -> Value {
        Value::Dict(vec![
            (b("windows"), Value::Dict(vec![(b("neocomWidth"), wrapped(Value::Int(37)))])),
            (
                Value::Ref(9),
                Value::Dict(vec![
                    (b("shipuialigntop"), wrapped(Value::Bool(true))),
                    (b("detachFighterUI"), wrapped(Value::Bool(true))),
                    (b("displayFighterUI"), wrapped(Value::Bool(false))),
                ]),
            ),
            (b("anchor"), Value::Shared { slot: 9, value: Box::new(b("ui")) }),
        ])
    }

    #[test]
    fn projects_the_account_side_fields() {
        let cdoc = char_doc();
        let udoc = user_doc();
        let hud = project_hud(&cdoc, Some(&udoc));
        assert_eq!(entry(&hud, "neocom_width").value.as_deref(), Some("37"));
        assert_eq!(entry(&hud, "ship_top").value.as_deref(), Some("true"));
        assert_eq!(entry(&hud, "fighter_detached").value.as_deref(), Some("true"));
        assert_eq!(entry(&hud, "fighter_shown").value.as_deref(), Some("false"));
        assert_eq!(entry(&hud, "neocom_width").scope, HudScope::Account);
        // Paths address the ACCOUNT document, not the character one.
        match &entry(&hud, "neocom_width").set {
            SetTarget::Set { path } => assert_eq!(resolve(&udoc, path), Some(&Value::Int(37))),
            other => panic!("expected Set, got {other:?}"),
        }
    }

    #[test]
    fn without_an_account_file_the_account_fields_are_unavailable() {
        let hud = project_hud(&char_doc(), None);
        for name in ["ship_top", "fighter_detached", "fighter_shown", "neocom_width"] {
            let e = entry(&hud, name);
            assert!(e.value.is_none(), "{name} has no value");
            assert!(matches!(e.set, SetTarget::Unavailable), "{name} is unavailable");
        }
        // The character-side fields are unaffected.
        assert_eq!(entry(&hud, "fighter_x").value.as_deref(), Some("326"));
    }

    #[test]
    fn all_nine_fields_are_projected_in_a_stable_order() {
        let hud = project_hud(&char_doc(), Some(&user_doc()));
        let names: Vec<&str> = hud.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "ship_offset", "fighter_x", "fighter_y", "badge_x", "badge_y",
                "ship_top", "fighter_detached", "fighter_shown", "neocom_width",
            ]
        );
    }
```

- [ ] **Step 2: Run them**

```bash
cargo test -p settings-model hud
```

Expected: PASS with no production change — Task 2's `project_hud` already routes by scope. If `projects_the_account_side_fields` fails on the Ref-keyed section, the bug is in `section`'s key resolution, not in the test.

- [ ] **Step 3: Commit**

```bash
git add crates/settings-model/src/hud.rs
git commit -m "Cover the account-side HUD fields"
```

---

### Task 4: `set_hud_value` — overwrite, and mint an absent leaf

**Files:**
- Modify: `crates/settings-model/src/hud.rs`, `crates/settings-model/src/lib.rs` (re-export)
- Test: `crates/settings-model/src/hud.rs`

**Interfaces:**
- Consumes: `FIELDS`, `section`, `leaf`, `scalar_text` from Task 2; `crate::mutate::{apply, Mutation}`; `crate::treewalk::inline_all`.
- Produces:
  - `pub fn set_hud_value(root: &mut Value, name: &str, text: &str) -> Result<(), HudError>`
  - `pub enum HudError { UnknownField(String), NoSection, NotEditable, Parse(String) }`, serialized `{"code": …, "detail": …}` like `MutateError`.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/settings-model/src/hud.rs`:

```rust
    #[test]
    fn sets_an_existing_float_and_keeps_its_wire_kind() {
        let mut doc = char_doc();
        set_hud_value(&mut doc, "ship_offset", "-42").expect("set");
        let hud = project_hud(&doc, None);
        assert_eq!(entry(&hud, "ship_offset").value.as_deref(), Some("-42"));
        // Still a Float, not an Int — set_scalar edits in place.
        match &entry(&hud, "ship_offset").set {
            SetTarget::Set { path } => {
                assert!(matches!(resolve(&doc, path), Some(Value::Float(f)) if *f == -42.0));
            }
            other => panic!("expected Set, got {other:?}"),
        }
    }

    #[test]
    fn sets_one_axis_of_a_point_without_disturbing_the_other() {
        let mut doc = char_doc();
        set_hud_value(&mut doc, "fighter_y", "500").expect("set");
        let hud = project_hud(&doc, None);
        assert_eq!(entry(&hud, "fighter_x").value.as_deref(), Some("326"));
        assert_eq!(entry(&hud, "fighter_y").value.as_deref(), Some("500"));
    }

    #[test]
    fn sets_a_bool_in_the_account_document() {
        let mut doc = user_doc();
        set_hud_value(&mut doc, "fighter_shown", "true").expect("set");
        let hud = project_hud(&char_doc(), Some(&doc));
        assert_eq!(entry(&hud, "fighter_shown").value.as_deref(), Some("true"));
    }

    #[test]
    fn mints_an_absent_scalar_with_a_zero_timestamp() {
        // `windows` present but empty — the 69/384 corpus case.
        let mut doc = Value::Dict(vec![(b("windows"), Value::Dict(vec![]))]);
        set_hud_value(&mut doc, "ship_offset", "-100").expect("mint");
        let hud = project_hud(&doc, None);
        assert_eq!(entry(&hud, "ship_offset").value.as_deref(), Some("-100"));
        // The minted leaf is the (timestamp, value) wrapper real files use.
        let Value::Dict(root) = &doc else { panic!("root is a dict") };
        let (_, section) = &root[0];
        let Value::Dict(entries) = section else { panic!("section is a dict") };
        assert_eq!(entries.len(), 1);
        match &entries[0].1 {
            Value::Tuple(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], Value::Long(vec![0u8; 8]));
                assert!(matches!(items[1], Value::Float(f) if f == -100.0));
            }
            other => panic!("expected (ts, value), got {other:?}"),
        }
    }

    #[test]
    fn mints_an_absent_point_with_the_sibling_axis_defaulted() {
        let mut doc = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
        set_hud_value(&mut doc, "fighter_x", "640").expect("mint");
        let hud = project_hud(&doc, None);
        assert_eq!(entry(&hud, "fighter_x").value.as_deref(), Some("640"));
        assert_eq!(entry(&hud, "fighter_y").value.as_deref(), Some("0"));
    }

    #[test]
    fn a_minted_key_is_written_once_not_duplicated() {
        let mut doc = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
        set_hud_value(&mut doc, "fighter_x", "10").expect("mint");
        set_hud_value(&mut doc, "fighter_y", "20").expect("set");
        let Value::Dict(root) = &doc else { panic!() };
        let Value::Dict(entries) = &root[0].1 else { panic!() };
        assert_eq!(entries.len(), 1, "the second write reuses the minted key");
        let hud = project_hud(&doc, None);
        assert_eq!(entry(&hud, "fighter_x").value.as_deref(), Some("10"));
        assert_eq!(entry(&hud, "fighter_y").value.as_deref(), Some("20"));
    }

    #[test]
    fn errors_are_reported_not_papered_over() {
        let mut doc = char_doc();
        assert_eq!(
            set_hud_value(&mut doc, "no_such_field", "1"),
            Err(HudError::UnknownField("no_such_field".to_string()))
        );
        // Section missing entirely.
        let mut bare = Value::Dict(vec![(b("audio"), Value::Dict(vec![]))]);
        assert_eq!(set_hud_value(&mut bare, "ship_offset", "1"), Err(HudError::NoSection));
        // Key present with an unexpected wire kind.
        let mut odd = Value::Dict(vec![(
            b("windows"),
            Value::Dict(vec![(b("shipuialignleftoffset"), wrapped(b("nonsense")))]),
        )]);
        assert_eq!(set_hud_value(&mut odd, "ship_offset", "1"), Err(HudError::NotEditable));
        // Unparseable text.
        assert!(matches!(set_hud_value(&mut doc, "fighter_x", "abc"), Err(HudError::Parse(_))));
    }
```

- [ ] **Step 2: Run and watch them fail**

```bash
cargo test -p settings-model hud
```

Expected: compile error — `set_hud_value` and `HudError` are undefined.

- [ ] **Step 3: Write the setter**

Append to the production half of `crates/settings-model/src/hud.rs` (before `mod tests`):

```rust
#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum HudError {
    UnknownField(String),
    /// The file has no such section — nothing to write into. Real character and
    /// account files always have both `windows` and `ui`.
    NoSection,
    /// The key exists but holds an unexpected wire kind; overwriting it would
    /// change its type and minting would duplicate the key.
    NotEditable,
    Parse(String),
}

/// Write one HUD field. An existing key is overwritten in place (no reshare
/// needed — a scalar edit is not structural). An absent key is minted as the
/// `(timestamp, value)` leaf real files use, which needs `inline_all` first per
/// the house rule; the caller (`ops`) reshares afterwards.
pub fn set_hud_value(root: &mut Value, name: &str, text: &str) -> Result<(), HudError> {
    let f = FIELDS
        .iter()
        .find(|f| f.name == name)
        .ok_or_else(|| HudError::UnknownField(name.to_string()))?;

    // Resolve the path under an immutable borrow, then mutate.
    let target = {
        let mut shared = SharedTable::new();
        collect_shared(root, &mut shared);
        match section(root, f.section, &shared) {
            None => Err(HudError::NoSection),
            Some((entries, base)) => Ok(leaf(entries, &base, f.key, f.elem, &shared)
                .map(|(v, path)| (scalar_text(v, f.kind, &shared).is_some(), path))),
        }?
    };

    match target {
        Some((true, path)) => {
            let m = crate::mutate::Mutation::SetScalar { path, text: text.to_string() };
            crate::mutate::apply(root, &m).map_err(|e| HudError::Parse(format!("{e:?}")))
        }
        Some((false, _)) => Err(HudError::NotEditable),
        None => mint(root, f, text),
    }
}

/// Insert the absent leaf. After `inline_all` every key is a plain byte-string,
/// so this half needs no `Shared`/`Ref` resolution.
fn mint(root: &mut Value, f: &Field, text: &str) -> Result<(), HudError> {
    let value = build_scalar(f.kind, text)?;
    let leaf_value = match f.elem {
        None => value,
        Some(ix) => {
            // A point field mints the whole (x, y); the untouched axis takes the
            // sibling field's default.
            let sibling = FIELDS
                .iter()
                .find(|o| o.section == f.section && o.key == f.key && o.elem != f.elem)
                .expect("every point field has a sibling axis");
            let other = build_scalar(sibling.kind, sibling.default)?;
            let mut items = vec![Value::None, Value::None];
            items[ix] = value;
            items[sibling.elem.expect("sibling is a point axis")] = other;
            Value::Tuple(items)
        }
    };
    inline_all(root);
    let Value::Dict(entries) = root else { return Err(HudError::NoSection) };
    let (_, section_value) = entries
        .iter_mut()
        .find(|(k, _)| is_bytes(k, f.section))
        .ok_or(HudError::NoSection)?;
    let Value::Dict(section_entries) = section_value else { return Err(HudError::NoSection) };
    section_entries.push((
        Value::Bytes(f.key.to_vec()),
        Value::Tuple(vec![Value::Long(vec![0u8; 8]), leaf_value]),
    ));
    Ok(())
}

fn build_scalar(kind: HudKind, text: &str) -> Result<Value, HudError> {
    let err = || HudError::Parse(format!("{kind:?}: {text:?}"));
    Ok(match kind {
        HudKind::Float => {
            let v: f64 = text.trim().parse().map_err(|_| err())?;
            if !v.is_finite() {
                return Err(err());
            }
            Value::Float(v)
        }
        HudKind::Int => Value::Int(text.trim().parse().map_err(|_| err())?),
        HudKind::Bool => match text {
            "true" | "True" => Value::Bool(true),
            "false" | "False" => Value::Bool(false),
            _ => return Err(err()),
        },
    })
}
```

Add `inline_all` to the `treewalk` import at the top of the file:

```rust
use crate::treewalk::{collect_shared, effective, inline_all, is_bytes, unwrap_shared, Entries, SharedTable};
```

- [ ] **Step 4: Extend the re-export**

In `crates/settings-model/src/lib.rs`:

```rust
pub use hud::{project_hud, set_hud_value, Hud, HudEntry, HudError, HudKind, HudScope};
```

- [ ] **Step 5: Run the tests**

```bash
cargo test -p settings-model hud
```

Expected: PASS — 17 tests. Then the whole workspace:

```bash
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/src/hud.rs crates/settings-model/src/lib.rs
git commit -m "Write HUD values, minting an absent leaf"
```

---

### Task 5: ops commands and Tauri registration

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (new functions near `window_layout` at `:577` and the stack ops at `:889`)
- Modify: `app/src-tauri/src/lib.rs` (two `#[tauri::command]` wrappers + `generate_handler!` at `:317-331`)
- Test: `app/src-tauri/src/ops.rs` (inline `mod tests`)

**Interfaces:**
- Consumes: `settings_model::{project_hud, set_hud_value, Hud, HudError, HudScope}`; `AppState`, `Slot`, `Fidelity`, `ErrDto` from this crate.
- Produces:
  - `pub fn hud_layout(state: &AppState) -> Result<Hud, ErrDto>`
  - `pub fn set_hud_field(state: &AppState, name: &str, text: &str) -> Result<Hud, ErrDto>` — named `_field`, not `_value`, because `ops.rs` imports `settings_model::set_hud_value` and the two would clash.
  - Tauri commands `hud_layout` (no args) and `set_hud_value` (`name`, `text`), both returning `Hud`.

- [ ] **Step 1: Write the failing test**

In `app/src-tauri/src/ops.rs`'s `mod tests`, mirror the existing
`unstack_reprojects_and_reshares` test (around `:1558`): it builds a document
with `temp_file("<prefix>", &bytes)`, opens it with
`open_file(&state, Slot::Char, &path.to_string_lossy())`, and asserts the doc
still encode/decode round-trips afterwards. Add:

```rust
    #[test]
    fn hud_projects_and_sets_the_ship_offset() {
        // A character document with an empty `windows` section: the projection
        // reports ship_offset absent, and the first write mints it.
        let state = AppState::default();
        let path = temp_file("hud-char", &hud_char_bytes());
        open_file(&state, Slot::Char, &path.to_string_lossy()).expect("open");

        let hud = hud_layout(&state).expect("project");
        let e = hud.entries.iter().find(|e| e.name == "ship_offset").expect("entry");
        assert!(e.value.is_none());

        let hud = set_hud_field(&state, "ship_offset", "-77").expect("set");
        let e = hud.entries.iter().find(|e| e.name == "ship_offset").expect("entry");
        assert_eq!(e.value.as_deref(), Some("-77"));

        // The document still encodes and round-trips (reshare ran cleanly).
        let guard = state.char.lock().unwrap();
        let doc = guard.as_ref().expect("open");
        let bytes = blue_marshal::encode(&doc.value).expect("encode");
        assert_eq!(blue_marshal::decode(&bytes).unwrap(), doc.value);
    }

    #[test]
    fn hud_without_a_character_file_is_an_error() {
        let state = AppState::default();
        assert!(hud_layout(&state).is_err());
    }
```

Add the fixture beside the other `*_bytes()` helpers in that test module:

```rust
    /// root -> { b"windows": {}, b"ui": {} } — sections present, no HUD keys.
    fn hud_char_bytes() -> Vec<u8> {
        let doc = blue_marshal::Value::Dict(vec![
            (blue_marshal::Value::Bytes(b"windows".to_vec()), blue_marshal::Value::Dict(vec![])),
            (blue_marshal::Value::Bytes(b"ui".to_vec()), blue_marshal::Value::Dict(vec![])),
        ]);
        blue_marshal::encode(&doc).expect("encode fixture")
    }
```

- [ ] **Step 2: Run and watch it fail**

```bash
cargo test -p eve-settings-editor hud
```

(If the crate name differs, use `cargo test --workspace hud`.) Expected: compile error — `hud_layout` and `set_hud_field` are undefined.

- [ ] **Step 3: Write the ops functions**

Add to `app/src-tauri/src/ops.rs`, after `window_layout`:

```rust
/// Project the HUD anchors: the character document is required, the account
/// document optional (an unpaired character still has its own anchors).
pub fn hud_layout(state: &AppState) -> Result<Hud, ErrDto> {
    let cguard = state.char.lock().unwrap();
    let cdoc = cguard.as_ref().ok_or_else(|| ErrDto::new("no_document", "no character file open"))?;
    let uguard = state.user.lock().unwrap();
    Ok(project_hud(&cdoc.value, uguard.as_ref().map(|d| &d.value)))
}

/// Write one HUD field into whichever document its scope names, reshare, and
/// re-project. The frontend marks that slot dirty from the entry's scope.
pub fn set_hud_field(state: &AppState, name: &str, text: &str) -> Result<Hud, ErrDto> {
    let scope = {
        // The projection is the single source of truth for which file a field
        // lives in, so ops never repeats the field table.
        let hud = hud_layout(state)?;
        hud.entries
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.scope)
            .ok_or_else(|| ErrDto::new("hud", format!("unknown field {name:?}")))?
    };
    let slot = match scope {
        HudScope::Char => Slot::Char,
        HudScope::Account => Slot::User,
    };
    {
        let mut guard = state.doc(slot).lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| {
            let which = match scope {
                HudScope::Char => "character",
                HudScope::Account => "account",
            };
            ErrDto::new("no_document", format!("no {which} file open"))
        })?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        set_hud_value(&mut doc.value, name, text).map_err(|e| ErrDto::new("hud", format!("{e:?}")))?;
        doc.value = blue_marshal::reshare(&doc.value);
    }
    hud_layout(state)
}
```

Extend the `settings_model::` import list at the top of `ops.rs` with `project_hud, set_hud_value, Hud, HudScope`. `state.doc(slot)` is the private `impl AppState` helper `window_layout` already uses — same module, no visibility change needed.

- [ ] **Step 4: Register the commands**

In `app/src-tauri/src/lib.rs`, beside the stack commands:

```rust
#[tauri::command]
fn hud_layout(state: tauri::State<'_, AppState>) -> Result<settings_model::Hud, ErrDto> {
    ops::hud_layout(&state)
}
#[tauri::command]
fn set_hud_value(
    state: tauri::State<'_, AppState>,
    name: String,
    text: String,
) -> Result<settings_model::Hud, ErrDto> {
    ops::set_hud_field(&state, &name, &text)
}
```

and in `generate_handler!`, extend the last line:

```rust
            stack_unstack, stack_add, stack_reorder, stack_create,
            hud_layout, set_hud_value
```

- [ ] **Step 5: Run the tests**

```bash
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs
git commit -m "Add the hud_layout and set_hud_value commands"
```

---

### Task 6: Frontend types and `hudRects`

**Files:**
- Modify: `app/src/lib/api.ts` (types beside `BoolFlag` at `:116-125`, wrappers beside `windowLayout` at `:270`)
- Modify: `app/src/lib/layout.ts`
- Test: `app/src/lib/layout.test.ts`

**Interfaces:**
- Consumes: the wire shapes from Tasks 2–5.
- Produces:
  - `api.ts`: `HudScope`, `HudKind`, `HudEntry`, `Hud`; `api.hud()`, `api.setHudValue(name, text)`.
  - `layout.ts`: `FurnitureRect`, `HUD_NOMINAL`, `hudNum(hud, name): number | null`, `hudFlag(hud, name): boolean`, `hudRects(hud, layout): FurnitureRect[]`, `shipOffsetFromX(x, referenceW): number`.

- [ ] **Step 1: Add the wire types and wrappers**

In `app/src/lib/api.ts`, after the `BoolFlag` interface:

```ts
export type HudScope = "char" | "account";
export type HudKind = "float" | "int" | "bool";

export interface HudEntry {
  name: string;
  kind: HudKind;
  /** null when the key is absent or holds an unexpected wire kind — use `default`. */
  value: string | null;
  default: string;
  scope: HudScope;
  set: SetTarget;
}

export interface Hud {
  entries: HudEntry[];
}
```

and beside `windowLayout` in the `api` object:

```ts
  hud: () => invoke<Hud>("hud_layout"),
  setHudValue: (name: string, text: string) =>
    invoke<Hud>("set_hud_value", { name, text }),
```

- [ ] **Step 2: Write the failing tests**

Append to `app/src/lib/layout.test.ts`:

```ts
import { hudRects, hudNum, hudFlag, shipOffsetFromX, HUD_NOMINAL } from "./layout.ts";
import type { Hud, HudEntry, WindowLayout } from "./api.ts";

const hudEntry = (name: string, value: string | null, kind: HudEntry["kind"], dflt: string, how: "set" | "unavailable" = "set"): HudEntry => ({
  name,
  kind,
  value,
  default: dflt,
  scope: name.startsWith("ship_top") || name.startsWith("fighter_d") || name.startsWith("fighter_s") || name === "neocom_width" ? "account" : "char",
  set: how === "set" ? { how: "set", path: [] } : { how: "unavailable" },
});

const fullHud = (over: Partial<Record<string, HudEntry>> = {}): Hud => {
  const base: HudEntry[] = [
    hudEntry("ship_offset", "-100", "float", "0"),
    hudEntry("fighter_x", "326", "int", "0"),
    hudEntry("fighter_y", "54", "int", "0"),
    hudEntry("badge_x", "1000", "int", "0"),
    hudEntry("badge_y", "20", "int", "0"),
    hudEntry("ship_top", "false", "bool", "false"),
    hudEntry("fighter_detached", "true", "bool", "false"),
    hudEntry("fighter_shown", "true", "bool", "false"),
    hudEntry("neocom_width", "37", "int", "37"),
  ];
  return { entries: base.map((e) => over[e.name] ?? e) };
};

const layout2560: WindowLayout = { reference_w: 2560, reference_h: 1440, windows: [], stacks: [] };

// An absent value falls back to the default; an unavailable field reads null.
check("hudNum uses the value when present", hudNum(fullHud(), "fighter_x") === 326);
check(
  "hudNum falls back to the default when the key is absent",
  hudNum(fullHud({ fighter_x: hudEntry("fighter_x", null, "int", "7") }), "fighter_x") === 7,
);
check(
  "hudNum is null when the field is unavailable",
  hudNum(fullHud({ neocom_width: hudEntry("neocom_width", null, "int", "37", "unavailable") }), "neocom_width") === null,
);
check("hudFlag reads a bool", hudFlag(fullHud(), "fighter_detached") === true);

{
  const rects = hudRects(fullHud(), layout2560);
  const kinds = rects.map((r) => r.kind).join(",");
  check("all four elements are drawn in a stable order", kinds === "neocom,shipui,fighter,badge");

  const neocom = rects[0];
  check("the neocom is a full-height left bar", neocom.x === 0 && neocom.y === 0 && neocom.w === 37 && neocom.h === 1440);
  check("the neocom is not draggable", neocom.drag === "none");

  // Centre-relative offset: x = w/2 + offset - nominal/2 = 1280 - 100 - 343.
  const ship = rects[1];
  check("the ship HUD is centred plus the offset", ship.x === 1280 - 100 - HUD_NOMINAL.shipui.w / 2);
  check("the ship HUD sits at the bottom by default", ship.y === 1440 - HUD_NOMINAL.shipui.h);
  check("the ship HUD drags on x only", ship.drag === "x");

  const fighter = rects[2];
  check("the fighter panel sits at its stored point", fighter.x === 326 && fighter.y === 54);
  check("the fighter panel drags freely", fighter.drag === "xy");
}

{
  const rects = hudRects(fullHud({ ship_top: hudEntry("ship_top", "true", "bool", "false") }), layout2560);
  check("ship_top anchors the HUD to the top", rects[1].y === 0);
}

{
  const rects = hudRects(fullHud({ fighter_shown: hudEntry("fighter_shown", "false", "bool", "false") }), layout2560);
  check("a hidden fighter UI is not drawn", !rects.some((r) => r.kind === "fighter"));
}

{
  const rects = hudRects(fullHud({ fighter_detached: hudEntry("fighter_detached", "false", "bool", "false") }), layout2560);
  check("an attached fighter UI is not drawn", !rects.some((r) => r.kind === "fighter"));
}

{
  const rects = hudRects(
    fullHud({ neocom_width: hudEntry("neocom_width", null, "int", "37", "unavailable") }),
    layout2560,
  );
  check("no account file means no neocom bar", !rects.some((r) => r.kind === "neocom"));
}

// The drag round-trip: a rect x converted back to the stored offset is itself.
check("shipOffsetFromX inverts the ship HUD placement", shipOffsetFromX(1280 - 100 - HUD_NOMINAL.shipui.w / 2, 2560) === -100);
```

- [ ] **Step 3: Run and watch it fail**

```bash
cd app && npm test
```

Expected: FAIL — `hudRects` and friends are not exported from `layout.ts`.

- [ ] **Step 4: Implement in `layout.ts`**

Append to `app/src/lib/layout.ts` (and extend its import to `import type { WindowLayout, Stack, WindowRect, Hud } from "./api";`):

```ts
export interface FurnitureRect {
  kind: "neocom" | "shipui" | "fighter" | "badge";
  label: string;
  /** Data px, like WindowRect.geom — the canvas scales it with toCanvas. */
  x: number;
  y: number;
  w: number;
  h: number;
  drag: "none" | "x" | "xy";
}

// ponytail: EVE stores anchors but never sizes, and never says what the anchor
// is relative to. These nominal sizes — and the centre-relative ship offset and
// top-left point convention below — are ASSUMPTIONS, corrected from the slice's
// live smoke. Nothing outside this table depends on the numbers.
export const HUD_NOMINAL = {
  shipui: { w: 686, h: 250 },
  fighter: { w: 400, h: 120 },
  badge: { w: 32, h: 32 },
};

const byName = (hud: Hud, name: string) => hud.entries.find((e) => e.name === name);

/** A field's number: its value, else its default; null when not writable at all. */
export function hudNum(hud: Hud, name: string): number | null {
  const e = byName(hud, name);
  if (!e || e.set.how === "unavailable") return null;
  const n = parseFloat(e.value ?? e.default);
  return Number.isFinite(n) ? n : null;
}

export function hudFlag(hud: Hud, name: string): boolean {
  const e = byName(hud, name);
  if (!e || e.set.how === "unavailable") return false;
  return (e.value ?? e.default) === "true";
}

/** Stored offset for a ship-HUD rect at data-px `x`. Inverse of hudRects. */
export function shipOffsetFromX(x: number, referenceW: number): number {
  return Math.round(x + HUD_NOMINAL.shipui.w / 2 - referenceW / 2);
}

/**
 * The screen furniture the canvas draws, in data px. Order is fixed
 * (neocom, ship HUD, fighter, badge) so the canvas paints the bar first and
 * tests can index. An element whose values aren't writable is omitted rather
 * than drawn at a guessed position.
 */
export function hudRects(hud: Hud, layout: WindowLayout): FurnitureRect[] {
  const out: FurnitureRect[] = [];

  const neocom = hudNum(hud, "neocom_width");
  if (neocom !== null && neocom > 0) {
    out.push({ kind: "neocom", label: "Neocom", x: 0, y: 0, w: neocom, h: layout.reference_h, drag: "none" });
  }

  const offset = hudNum(hud, "ship_offset");
  if (offset !== null) {
    const { w, h } = HUD_NOMINAL.shipui;
    out.push({
      kind: "shipui",
      label: "Ship HUD",
      x: Math.round(layout.reference_w / 2 + offset - w / 2),
      y: hudFlag(hud, "ship_top") ? 0 : layout.reference_h - h,
      w,
      h,
      drag: "x",
    });
  }

  const fx = hudNum(hud, "fighter_x");
  const fy = hudNum(hud, "fighter_y");
  if (fx !== null && fy !== null && hudFlag(hud, "fighter_detached") && hudFlag(hud, "fighter_shown")) {
    out.push({ kind: "fighter", label: "Fighter UI", x: fx, y: fy, ...HUD_NOMINAL.fighter, drag: "xy" });
  }

  const bx = hudNum(hud, "badge_x");
  const by = hudNum(hud, "badge_y");
  if (bx !== null && by !== null) {
    out.push({ kind: "badge", label: "Badge", x: bx, y: by, ...HUD_NOMINAL.badge, drag: "xy" });
  }

  return out;
}
```

- [ ] **Step 5: Run tests and the type check**

```bash
cd app && npm test && npm run check
```

Expected: PASS both.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/api.ts app/src/lib/layout.ts app/src/lib/layout.test.ts
git commit -m "Compute HUD furniture rects"
```

---

### Task 7: Draw and drag the furniture on the canvas

**Files:**
- Modify: `app/src/lib/LayoutView.svelte`
- Modify: `app/src/routes/+page.svelte:444-452` (pass `onDirty`)

**Interfaces:**
- Consumes: `api.hud`, `api.setHudValue` (Task 6), `hudRects`, `shipOffsetFromX`, `FurnitureRect`.
- Produces: a new `LayoutView` prop `onDirty: (slot: Slot) => void`, called after every backend-op edit.

**Why `onDirty`:** the existing stack ops (`api.stackUnstack`/`stackAdd`/`stackReorder`/`stackCreate`) change the character document in the backend but nothing marks `dirtySlots.char`, and `saveFile()` skips a non-dirty slot — so a stack edit with no accompanying drag is silently not saved. The HUD ops have the same shape, so one prop fixes both at the shared seam.

- [ ] **Step 1: Load the projection alongside the layout**

In `LayoutView.svelte`, extend the props and state:

```ts
  import { canvasScale, toCanvas, toData, resizeRect, stackUnits, hudRects, shipOffsetFromX, type Corner, type DrawUnit, type FurnitureRect } from "$lib/layout";
```

```ts
  let {
    slot,
    runMutations,
    readOnly,
    refreshToken,
    selectedId = $bindable(null),
    onReveal,
    onDirty,
  }: {
    slot: Slot;
    runMutations: (ms: Mutation[], rethrow?: boolean) => Promise<void>;
    readOnly: boolean;
    refreshToken: number;
    selectedId?: string | null;
    onReveal: (path: NodePath) => void;
    onDirty: (slot: Slot) => void;
  } = $props();

  let hud = $state<Hud | null>(null);
  // Live drag preview for furniture, keyed by kind (data px).
  let fPreview: Record<string, { x: number; y: number }> = $state({});
```

Add `Hud` to the `$lib/api` type import. In `load()`, fetch both — the HUD projection needs a character file, so a failure there must not break the layout:

```ts
  async function load() {
    try {
      layout = await api.windowLayout(slot);
      if (selectedId && !layout.windows.some((w) => w.id === selectedId)) {
        selectedId = null;
      }
    } catch (e) {
      await message(errMessage(e), { title: "Layout unavailable", kind: "error" });
    }
    // Furniture is a bonus view: an account file open on its own, or a document
    // with no HUD keys, must not take the canvas down with it.
    hud = await api.hud().catch(() => null);
  }
```

- [ ] **Step 2: Mark the slot dirty after every op edit**

Replace `runStack` and add the HUD writer:

```ts
  async function runStack(p: Promise<WindowLayout>) {
    try {
      layout = await p;
      onDirty("char"); // stack ops edit the character document in the backend
      if (selectedId && !layout.windows.some((w) => w.id === selectedId)) selectedId = null;
    } catch (e) {
      await message(errMessage(e), { title: "Stack edit failed", kind: "error" });
    }
  }

  /** Write one HUD field and refresh the projection. */
  async function setHud(name: string, text: string) {
    try {
      hud = await api.setHudValue(name, text);
      const e = hud.entries.find((x) => x.name === name);
      onDirty(e?.scope === "account" ? "user" : "char");
    } catch (e) {
      await message(errMessage(e), { title: "HUD edit failed", kind: "error" });
    }
  }
```

- [ ] **Step 3: Extend the drag union with furniture**

```ts
  type Drag =
    | { kind: "move"; unit: DrawUnit; startX: number; startY: number; ox: number; oy: number }
    | { kind: "resize"; unit: DrawUnit; corner: Corner; startX: number; startY: number; ox: number; oy: number; ow: number; oh: number }
    | { kind: "furniture"; f: FurnitureRect; startX: number; startY: number; ox: number; oy: number };
```

```ts
  const furniture = $derived(hud && layout ? hudRects(hud, layout) : []);
  const fRectOf = (f: FurnitureRect) => fPreview[f.kind] ?? { x: f.x, y: f.y };

  function startFurniture(f: FurnitureRect, e: PointerEvent) {
    if (readOnly || f.drag === "none") return;
    const r = fRectOf(f);
    drag = { kind: "furniture", f, startX: e.clientX, startY: e.clientY, ox: r.x, oy: r.y };
    canvasEl?.setPointerCapture(e.pointerId);
    e.preventDefault();
    e.stopPropagation();
  }
```

In `onPointerMove`, add the branch (a `"x"` element ignores dy at the source, so the vertical axis can never be written):

```ts
    if (drag.kind === "furniture") {
      const f = drag.f;
      fPreview = {
        ...fPreview,
        [f.kind]: { x: drag.ox + dx, y: f.drag === "xy" ? drag.oy + dy : drag.oy },
      };
      return;
    }
```

In `onPointerUp`, handle furniture before the window path:

```ts
    if (d.kind === "furniture") {
      const p = fPreview[d.f.kind];
      if (!p) return;
      if (d.f.kind === "shipui" && layout) {
        await setHud("ship_offset", String(shipOffsetFromX(p.x, layout.reference_w)));
      } else if (d.f.kind === "fighter" || d.f.kind === "badge") {
        const prefix = d.f.kind === "fighter" ? "fighter" : "badge";
        if (p.x !== d.f.x) await setHud(`${prefix}_x`, String(Math.round(p.x)));
        if (p.y !== d.f.y) await setHud(`${prefix}_y`, String(Math.round(p.y)));
      }
      const rest = { ...fPreview };
      delete rest[d.f.kind];
      fPreview = rest;
      return;
    }
```

`onPointerUp` currently reads `drag.unit` before the null check — restructure it so `const d = drag; drag = null;` happens first, then branch on `d.kind`, keeping the existing window behaviour (including the re-drag preview guard) intact for the `"move"`/`"resize"` cases.

- [ ] **Step 4: Draw the furniture**

Inside the canvas div, **before** the `{#each units …}` block so windows paint on top:

```svelte
        {#each furniture as f (f.kind)}
          {@const r = fRectOf(f)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="furniture"
            class:draggable={f.drag !== "none" && !readOnly}
            style="left: {toCanvas(r.x, scale)}px; top: {toCanvas(r.y, scale)}px;
                   width: {toCanvas(f.w, scale)}px; height: {toCanvas(f.h, scale)}px;"
            onpointerdown={(e) => startFurniture(f, e)}>
            <span class="furniture-label">{f.label}</span>
          </div>
        {/each}
```

and the styles (muted, dashed, never resizable; non-draggable furniture must not swallow a window drag):

```css
  .furniture {
    position: absolute;
    box-sizing: border-box;
    background: rgba(148, 163, 184, 0.12);
    border: 1px dashed #64748b;
    color: #94a3b8;
    font-size: 11px;
    overflow: hidden;
    pointer-events: none;
    touch-action: none;
  }
  .furniture.draggable {
    pointer-events: auto;
    cursor: move;
  }
  .furniture-label {
    padding: 1px 3px;
    pointer-events: none;
  }
```

Do **not** import or mount `HudPanel` yet — Task 8 creates that component and
mounts it, so this task stays independently buildable. Drop the
`import HudPanel …` line from Step 1 if you added it.

- [ ] **Step 5: Pass `onDirty` from the page**

In `app/src/routes/+page.svelte`, in the `LayoutView` block:

```svelte
            onReveal={revealInTree}
            onDirty={(slot) => (dirtySlots[slot] = true)} />
```

- [ ] **Step 6: Type-check and build**

```bash
cd app && npm run check && npm run build
```

Expected: PASS. The canvas now draws furniture and drags it; the fields arrive in Task 8.

- [ ] **Step 7: Commit**

```bash
git add app/src/lib/LayoutView.svelte app/src/routes/+page.svelte
git commit -m "Draw and drag the HUD furniture on the layout canvas"
```

---

### Task 8: `HudPanel.svelte` — the exact fields

**Files:**
- Create: `app/src/lib/HudPanel.svelte`
- Modify: `app/src/lib/LayoutView.svelte` (import and mount it)

**Interfaces:**
- Consumes: `Hud`, `HudEntry` from `$lib/api`; props `{ hud: Hud; readOnly: boolean; onSet: (name: string, text: string) => void }` — `onSet` is `setHud` from Task 7.
- Produces: nothing consumed elsewhere.

- [ ] **Step 1: Write the component**

```svelte
<script lang="ts">
  import type { Hud, HudEntry } from "$lib/api";

  let { hud, readOnly, onSet }: {
    hud: Hud;
    readOnly: boolean;
    onSet: (name: string, text: string) => void;
  } = $props();

  // Display order and labels. Account-scoped rows are flagged in the UI because
  // they change every character on the account.
  const GROUPS: { title: string; rows: { name: string; label: string }[] }[] = [
    { title: "Ship HUD", rows: [
      { name: "ship_offset", label: "Offset from centre" },
      { name: "ship_top", label: "Align to top" },
    ] },
    { title: "Fighter UI", rows: [
      { name: "fighter_x", label: "x" },
      { name: "fighter_y", label: "y" },
      { name: "fighter_detached", label: "Detached" },
      { name: "fighter_shown", label: "Shown" },
    ] },
    { title: "Neocom", rows: [{ name: "neocom_width", label: "Width" }] },
    { title: "Notification badge", rows: [
      { name: "badge_x", label: "x" },
      { name: "badge_y", label: "y" },
    ] },
  ];

  const find = (name: string): HudEntry | undefined => hud.entries.find((e) => e.name === name);
  const shown = (name: string) => find(name)?.value ?? find(name)?.default ?? "";
  const disabled = (e: HudEntry) => readOnly || e.set.how === "unavailable";
  const title = (e: HudEntry) =>
    e.set.how === "unavailable"
      ? "Not present in this file"
      : e.value === null
        ? `EVE's default (${e.default}) — editing stores a value`
        : e.scope === "account"
          ? "Account-wide: changes every character on this account"
          : "";

  const numberEdit = (name: string) => (ev: Event) => {
    const raw = (ev.target as HTMLInputElement).value;
    if (raw.trim() !== "" && Number.isFinite(Number(raw))) onSet(name, raw);
  };
</script>

<div class="hud-panel">
  {#each GROUPS as g (g.title)}
    <div class="group">
      <h4>{g.title}</h4>
      {#each g.rows as row (row.name)}
        {@const e = find(row.name)}
        {#if e}
          <label class="row" title={title(e)}>
            {#if e.kind === "bool"}
              <input
                type="checkbox"
                checked={shown(row.name) === "true"}
                disabled={disabled(e)}
                onchange={(ev) => onSet(row.name, (ev.target as HTMLInputElement).checked ? "true" : "false")} />
              <span class="label">{row.label}</span>
            {:else}
              <span class="label">{row.label}</span>
              <input
                type="number"
                value={shown(row.name)}
                disabled={disabled(e)}
                onchange={numberEdit(row.name)} />
            {/if}
            {#if e.scope === "account"}<span class="badge">account</span>{/if}
            {#if e.value === null && e.set.how !== "unavailable"}<span class="badge">default</span>{/if}
          </label>
        {/if}
      {/each}
    </div>
  {/each}
</div>

<style>
  .hud-panel {
    border-bottom: 1px solid #333;
    padding: 0.4rem 0.5rem;
    font-size: 12px;
  }
  .group {
    margin-bottom: 0.4rem;
  }
  h4 {
    margin: 0.2rem 0;
    color: #9aa4b2;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    padding: 1px 0;
  }
  .label {
    color: #cbd5e1;
    min-width: 8.5rem;
  }
  /* Native controls render light in WebView2 unless told otherwise. */
  input[type="number"] {
    width: 5.5rem;
    background: #1b1f27;
    color: #e5e7eb;
    border: 1px solid #444;
  }
  input[type="number"]:disabled {
    color: #6b7280;
  }
  .badge {
    color: #94a3b8;
    background: #262b36;
    border-radius: 3px;
    padding: 0 4px;
    font-size: 10px;
  }
</style>
```

- [ ] **Step 2: Mount it in `LayoutView.svelte`**

Import it beside the `WindowPanel` import:

```ts
  import HudPanel from "$lib/HudPanel.svelte";
```

The view's grid has exactly two children (`.canvas-wrap` and the panel), so wrap
the right-hand column rather than adding a third:

```svelte
    <div class="side">
      {#if hud}
        <HudPanel {hud} {readOnly} onSet={setHud} />
      {/if}
      <WindowPanel … />   <!-- existing props unchanged -->
    </div>
```

```css
  .side {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: auto;
  }
```

- [ ] **Step 3: Type-check, test, build**

```bash
cd app && npm run check && npm test && npm run build
```

Expected: PASS all three.

- [ ] **Step 4: Commit**

```bash
git add app/src/lib/HudPanel.svelte app/src/lib/LayoutView.svelte
git commit -m "Add the HUD fields panel"
```

---

### Task 9: Verification and the live smoke

**Files:**
- Modify: `docs/format-notes.md` (new "HUD anchors" section under `## Mappings`)
- Modify: `app/src/lib/layout.ts` (`HUD_NOMINAL` and, if the smoke says so, the anchor conventions in `hudRects`)
- Modify: `docs/small-tasks.md` (ledger entries for what this slice deliberately left out)

- [ ] **Step 1: Run every gate**

```bash
cargo test --workspace
cd app && npm run check && npm test && npm run build
```

Expected: all PASS. Do not proceed to the smoke with a red gate.

- [ ] **Step 2: Live smoke — the assumption checklist**

Launch the app (`cd app && npm run tauri dev`), open a real character with its account file paired, go to Layout, and check each line:

1. Furniture appears: a left neocom bar, a ship HUD block at the bottom centre, and — if the character has the fighter UI detached and shown — a fighter block. The notification badge block appears where the badge sits in game.
2. Drag the ship HUD sideways, drag the fighter block, save, and confirm the character/account dirty badges appeared before the save.
3. Note the numbers the panel shows, then start EVE with that character and compare: does the ship HUD sit where the canvas drew it? Is the fighter panel's stored point its top-left, or its centre? Is the drawn size roughly right?
4. Fix `HUD_NOMINAL` (and, if the point is centre-anchored, `hudRects`'s fighter/badge placement plus its matched inverse pair — `shipOffsetFromX` for the ship HUD, `hudPointFromRect` for the fighter/badge points) from what you saw. Re-run `npm test` — the `hudRects` cases encode the convention, so they must be updated deliberately, not incidentally.
5. Toggle "Align to top" and confirm EVE moves the HUD to the top on next login.
6. Check a character with **no** stored offset (the 18% case): the field shows the default with a "default" badge, and the first edit mints the key — after saving, reopen the file and confirm the value persisted.
7. Regression check for the `onDirty` fix: unstack a window with no other edit, confirm the character dirty badge appears, save, reopen, and confirm the unstack persisted.
8. Open a **paired** account whose neocom width has actually been changed in-game (not left at the default) and whose fighter UI is detached, and confirm those rows show the real stored values rather than a "default" badge — a "default" badge on a character known to have a non-default value is the tell that `hud.rs`'s account-side section (`windows` for `neocomWidth`, `ui` for the other three) is wrong.

- [ ] **Step 3: Record the confirmed format facts**

Add a `### HUD anchors` section to `docs/format-notes.md` under `## Mappings`, stating the keys, their files and sections (including that the account file's `ui` section key is a `Ref`), the confirmed anchor conventions, and the nominal sizes measured in game. No character ids or names.

- [ ] **Step 4: Ledger what was left out**

Append to the **Open** list in `docs/small-tasks.md`: `dockPanels` (proportional map/skill-planner panel geometry, 370/384 files), batch-copying HUD anchors character to character (needs the resolution question answered first), and the unexposed account toggles (`hudButtonsExpanded`, `offsetUIwithCamera`, `neocomSizeLocked`).

- [ ] **Step 5: Commit**

```bash
git add docs/format-notes.md docs/small-tasks.md app/src/lib/layout.ts app/src/lib/layout.test.ts
git commit -m "Confirm the HUD anchor conventions from the live smoke"
```

- [ ] **Step 6: Request a whole-branch review**

Use `superpowers:requesting-code-review` over the whole branch against the spec before opening the PR, as every prior slice did.
