# M2 Layout Canvas Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a visual window-layout editor (draggable/resizable rectangles + a per-window detail panel) that edits the same document the raw tree editor edits and saves through the M1 save chain.

**Architecture:** A new read-only `window_layout()` projection in `settings-model` walks the `Value` tree once and returns a typed list of windows, each field carrying its resolved `NodePath` (or `insert` params for an absent flag). The Svelte frontend renders a canvas + accordion panel from that model and commits every edit through the **existing** `apply_mutation` command — no new write path. Format knowledge stays in Rust; the canvas is a thin renderer.

**Tech Stack:** Rust (`settings-model`, Tauri commands), Svelte 5 (runes), TypeScript. Frontend logic tests run on `node --test` (zero deps); Rust tests on `cargo test`.

## Global Constraints

- Commit messages: sentence-case summary line, **no attribution trailers** of any kind (no `Co-Authored-By`, no "Generated with").
- **No personal data** in any committed file: no character/account names, no real numeric IDs. Fixtures use synthetic, generic window ids (`b"overview"`, `b"market"`).
- `blue-marshal` stays dependency-free. `settings-model` adds no new dependencies for this work.
- Never read or write the live `%LOCALAPPDATA%\CCP\EVE` directory from code or tests. The only live-directory interaction is the deliberate manual smoke in Task 7.
- Frontend keeps its scaffolded dependency list: no test framework, no `@types/node`. Tests are `.test.ts` files using throw-based checks, run by `node --test "src/lib/**/*.test.ts"`.
- Rust commands: `powershell` tool has `cargo`/`npm`/`gh` on PATH; the Bash tool does not. Run `cargo` and `npm` via PowerShell.
- Path steps are **index-based** (`Step::DictValue(i)`, `Step::Tuple(i)`) — never key-by-string. The projection records positional indices as it descends.

---

### Task 1: Geometry projection in `settings-model`

Builds the read-only window model with geometry only (no flags yet): finds the geometry dict, extracts each window's six integers, records a `NodePath` to each, and computes the reference resolution.

**Files:**
- Create: `crates/settings-model/src/windows.rs`
- Modify: `crates/settings-model/src/lib.rs` (declare `pub mod windows;` and re-export)
- Test: inline `#[cfg(test)]` module in `windows.rs`

**Interfaces:**
- Consumes: `blue_marshal::Value`; `crate::path::{NodePath, Step}`; `crate::projection_kind`.
- Produces:
  - `pub fn window_layout(root: &Value) -> WindowLayout`
  - `pub struct WindowLayout { pub reference_w: i64, pub reference_h: i64, pub windows: Vec<WindowRect> }`
  - `pub struct WindowRect { pub id: String, pub label: String, pub open: bool, pub renderable: bool, pub resolution_matches: bool, pub geom: Option<Geom>, pub flags: Vec<BoolFlag>, pub stacks: Option<StackField> }` (Task 1 leaves `flags` empty and `stacks` `None`; `open` false)
  - `pub struct Geom { pub x: i64, pub y: i64, pub w: i64, pub h: i64, pub screen_w: i64, pub screen_h: i64, pub x_path: NodePath, pub y_path: NodePath, pub w_path: NodePath, pub h_path: NodePath, pub screen_w_path: NodePath, pub screen_h_path: NodePath }`
  - `pub struct BoolFlag { pub name: String, pub value: bool, pub set: SetTarget }` (used in Task 2)
  - `pub struct StackField { pub text: String, pub path: NodePath }` (used in Task 2)
  - `pub enum SetTarget { Set { path: NodePath }, Insert { parent: NodePath, key: NewValue }, Unavailable }` (used in Task 2)

- [ ] **Step 1: Write the failing test**

Add to the bottom of the new file `crates/settings-model/src/windows.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;
    use crate::path::{resolve, Step};

    fn ts() -> Value {
        // A stand-in FILETIME timestamp — the (timestamp, dict) wrapper.
        Value::Long(vec![0u8; 8])
    }

    fn geom(x: i64, y: i64, w: i64, h: i64, sw: i64, sh: i64) -> Value {
        Value::Tuple(vec![
            Value::Int(x), Value::Int(y), Value::Int(w),
            Value::Int(h), Value::Int(sw), Value::Int(sh),
        ])
    }

    /// root -> b"windows" -> { b"windowSizesAndPositions_1": (ts, { id: 6tuple }) }
    fn doc_with(geom_entries: Vec<(Value, Value)>) -> Value {
        Value::Dict(vec![(
            Value::Bytes(b"windows".to_vec()),
            Value::Dict(vec![(
                Value::Bytes(b"windowSizesAndPositions_1".to_vec()),
                Value::Tuple(vec![ts(), Value::Dict(geom_entries)]),
            )]),
        )])
    }

    #[test]
    fn extracts_windows_values_and_paths() {
        let doc = doc_with(vec![
            (Value::Bytes(b"overview".to_vec()), geom(100, 200, 400, 1000, 2560, 1440)),
            (Value::Bytes(b"market".to_vec()), geom(16, 825, 500, 600, 2560, 1440)),
        ]);
        let wl = window_layout(&doc);
        assert_eq!(wl.reference_w, 2560);
        assert_eq!(wl.reference_h, 1440);
        assert_eq!(wl.windows.len(), 2);

        let ov = &wl.windows[0];
        assert_eq!(ov.id, "overview");
        assert_eq!(ov.label, "overview");
        assert!(ov.renderable);
        assert!(ov.resolution_matches);
        let g = ov.geom.as_ref().expect("renderable window has geom");
        assert_eq!((g.x, g.y, g.w, g.h, g.screen_w, g.screen_h), (100, 200, 400, 1000, 2560, 1440));
        // Each path resolves to the right element in the original tree.
        assert_eq!(resolve(&doc, &g.x_path), Some(&Value::Int(100)));
        assert_eq!(resolve(&doc, &g.h_path), Some(&Value::Int(1000)));
        assert_eq!(resolve(&doc, &g.screen_w_path), Some(&Value::Int(2560)));
    }

    #[test]
    fn a_malformed_tuple_is_listed_but_not_renderable() {
        let doc = doc_with(vec![
            (Value::Bytes(b"overview".to_vec()), geom(1, 2, 3, 4, 2560, 1440)),
            // Only five ints — not a valid geometry tuple.
            (Value::Bytes(b"broken".to_vec()),
             Value::Tuple(vec![Value::Int(1), Value::Int(2), Value::Int(3), Value::Int(4), Value::Int(5)])),
        ]);
        let wl = window_layout(&doc);
        assert_eq!(wl.windows.len(), 2);
        assert!(!wl.windows[1].renderable);
        assert!(wl.windows[1].geom.is_none());
    }

    #[test]
    fn reference_resolution_is_the_most_common_and_flags_mismatches() {
        let doc = doc_with(vec![
            (Value::Bytes(b"a".to_vec()), geom(0, 0, 10, 10, 2560, 1440)),
            (Value::Bytes(b"b".to_vec()), geom(0, 0, 10, 10, 2560, 1440)),
            (Value::Bytes(b"c".to_vec()), geom(0, 0, 10, 10, 1920, 1080)),
        ]);
        let wl = window_layout(&doc);
        assert_eq!((wl.reference_w, wl.reference_h), (2560, 1440));
        assert!(wl.windows[0].resolution_matches);
        assert!(!wl.windows[2].resolution_matches);
    }

    #[test]
    fn a_file_without_geometry_is_empty() {
        let doc = Value::Dict(vec![(Value::Bytes(b"ui".to_vec()), Value::Dict(vec![]))]);
        let wl = window_layout(&doc);
        assert!(wl.windows.is_empty());
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run (PowerShell): `cargo test -p settings-model --lib windows::`
Expected: FAIL — `window_layout`, `WindowLayout`, etc. are not defined (compile error).

- [ ] **Step 3: Write the geometry implementation**

Put this at the **top** of `crates/settings-model/src/windows.rs` (above the `#[cfg(test)]` module). Note `reference_resolution` here filters over all renderable windows; Task 2 refines it to prefer *open* windows once `open` is real.

```rust
//! Read-only projection of the window-layout portion of a settings document:
//! per-window geometry and flags, each writable field carrying the resolved
//! `NodePath` a `set_scalar`/`insert_dict_entry` mutation targets. All EVE
//! window-format knowledge (the `(timestamp, dict)` wrappers, byte-string
//! window ids, tuple element order) lives here so the UI never reconstructs a
//! path from format details. Nothing in this module mutates.

use blue_marshal::Value;
use serde::Serialize;

use crate::mutate::NewValue;
use crate::path::{NodePath, Step};

/// The seven boolean per-window flags (see docs/format-notes.md). `stacksWindows`
/// is handled separately — its value is a stack id, not a bool.
const BOOL_FLAGS: [&str; 7] = [
    "openWindows",
    "collapsedWindows",
    "minimizedWindows",
    "lockedWindows",
    "compactWindows",
    "isOverlayedWindows",
    "isLightBackgroundWindows",
];

#[derive(Debug, Serialize)]
pub struct WindowLayout {
    pub reference_w: i64,
    pub reference_h: i64,
    pub windows: Vec<WindowRect>,
}

#[derive(Debug, Serialize)]
pub struct WindowRect {
    pub id: String,
    pub label: String,
    pub open: bool,
    pub renderable: bool,
    pub resolution_matches: bool,
    pub geom: Option<Geom>,
    pub flags: Vec<BoolFlag>,
    pub stacks: Option<StackField>,
}

#[derive(Debug, Serialize)]
pub struct Geom {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    pub screen_w: i64,
    pub screen_h: i64,
    pub x_path: NodePath,
    pub y_path: NodePath,
    pub w_path: NodePath,
    pub h_path: NodePath,
    pub screen_w_path: NodePath,
    pub screen_h_path: NodePath,
}

#[derive(Debug, Serialize)]
pub struct BoolFlag {
    pub name: String,
    pub value: bool,
    pub set: SetTarget,
}

#[derive(Debug, Serialize)]
pub struct StackField {
    pub text: String,
    pub path: NodePath,
}

/// How the UI writes a flag: overwrite an existing entry, insert a missing one,
/// or (when the whole flag dict is absent from the file) nothing.
#[derive(Debug, Serialize)]
#[serde(tag = "how", rename_all = "snake_case")]
pub enum SetTarget {
    Set { path: NodePath },
    Insert { parent: NodePath, key: NewValue },
    Unavailable,
}

type Entries = Vec<(Value, Value)>;

pub fn window_layout(root: &Value) -> WindowLayout {
    let empty = WindowLayout { reference_w: 0, reference_h: 0, windows: Vec::new() };

    let Some((windows_dict, windows_path)) = child_dict(root, b"windows", Vec::new()) else {
        return empty;
    };
    let Some((geom_dict, geom_path)) =
        timestamped_dict(windows_dict, &windows_path, b"windowSizesAndPositions_1")
    else {
        return empty;
    };

    let mut windows = Vec::new();
    for (wi, (key, val)) in geom_dict.iter().enumerate() {
        let id = decode_id(key);
        let mut entry_path = geom_path.clone();
        entry_path.push(Step::DictValue(wi));
        let geom = extract_geom(val, &entry_path);
        windows.push(WindowRect {
            id: id.clone(),
            label: id,
            open: false,          // filled in Task 2
            renderable: geom.is_some(),
            resolution_matches: true, // fixed up below
            geom,
            flags: Vec::new(),    // filled in Task 2
            stacks: None,         // filled in Task 2
        });
    }

    let (reference_w, reference_h) = reference_resolution(&windows);
    for w in &mut windows {
        if let Some(g) = &w.geom {
            w.resolution_matches = g.screen_w == reference_w && g.screen_h == reference_h;
        }
    }
    WindowLayout { reference_w, reference_h, windows }
}

/// `parent` must be a dict; find the entry keyed by the byte-string `name` and
/// return its value as a dict, threading the path (unwrapping one `Shared`).
fn child_dict<'a>(parent: &'a Value, name: &[u8], base: NodePath) -> Option<(&'a Entries, NodePath)> {
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
fn timestamped_dict<'a>(
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

fn unwrap_shared(v: &Value, mut path: NodePath) -> (&Value, NodePath) {
    if let Value::Shared { value, .. } = v {
        path.push(Step::SharedInner);
        return (value, path);
    }
    (v, path)
}

fn is_bytes(v: &Value, name: &[u8]) -> bool {
    matches!(v, Value::Bytes(b) if b.as_slice() == name)
}

fn decode_id(key: &Value) -> String {
    match key {
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        Value::Str(s) | Value::StrUcs2(s) => s.clone(),
        other => crate::projection_kind(other).to_string(),
    }
}

fn extract_geom(val: &Value, entry_path: &NodePath) -> Option<Geom> {
    let Value::Tuple(items) = val else { return None };
    if items.len() != 6 {
        return None;
    }
    let mut ints = [0i64; 6];
    for (i, e) in items.iter().enumerate() {
        match e {
            Value::Int(n) => ints[i] = *n,
            _ => return None,
        }
    }
    let path = |i: usize| {
        let mut q = entry_path.clone();
        q.push(Step::Tuple(i));
        q
    };
    Some(Geom {
        x: ints[0],
        y: ints[1],
        w: ints[2],
        h: ints[3],
        screen_w: ints[4],
        screen_h: ints[5],
        x_path: path(0),
        y_path: path(1),
        w_path: path(2),
        h_path: path(3),
        screen_w_path: path(4),
        screen_h_path: path(5),
    })
}

/// The resolution the most windows agree on. Task 2 refines this to prefer open
/// windows; here it is the mode across all renderable windows.
fn reference_resolution(windows: &[WindowRect]) -> (i64, i64) {
    mode(windows.iter().filter_map(|w| w.geom.as_ref().map(|g| (g.screen_w, g.screen_h))))
        .unwrap_or((0, 0))
}

fn mode(it: impl Iterator<Item = (i64, i64)>) -> Option<(i64, i64)> {
    let mut counts: Vec<((i64, i64), usize)> = Vec::new();
    for res in it {
        match counts.iter_mut().find(|(r, _)| *r == res) {
            Some(entry) => entry.1 += 1,
            None => counts.push((res, 1)),
        }
    }
    counts.into_iter().max_by_key(|(_, c)| *c).map(|(r, _)| r)
}
```

Then declare the module and re-export in `crates/settings-model/src/lib.rs`. Add after the existing `pub mod save;` line:

```rust
pub mod windows;
```

And after the existing `pub use save::{...};` line:

```rust
pub use windows::{window_layout, BoolFlag, Geom, SetTarget, StackField, WindowLayout, WindowRect};
```

- [ ] **Step 4: Run the tests to verify they pass**

Run (PowerShell): `cargo test -p settings-model --lib windows::`
Expected: PASS — 4 tests. (A warning about `BOOL_FLAGS` being unused is expected until Task 2; that is fine.)

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/windows.rs crates/settings-model/src/lib.rs
git commit -m "Add the window-layout geometry projection"
```

---

### Task 2: Flags and stacks in the projection

Layer the eight flags onto each window: the seven booleans (with `set` targets, including insert params for absent entries) and `stacksWindows` as an editable value when numeric. Make `open` real and let the reference resolution prefer open windows.

**Files:**
- Modify: `crates/settings-model/src/windows.rs`
- Modify: `crates/settings-model/src/mutate.rs:51` (add `Serialize` to `NewValue` so `SetTarget::Insert` serializes)
- Test: inline `#[cfg(test)]` module in `windows.rs`

**Interfaces:**
- Consumes: everything from Task 1.
- Produces: `WindowRect.flags` populated (7 entries, order per `BOOL_FLAGS`), `WindowRect.open` = the `openWindows` value, `WindowRect.stacks` = editable numeric stack id when present.

- [ ] **Step 1: Write the failing test**

Add these tests inside the existing `#[cfg(test)] mod tests` in `windows.rs`:

```rust
    /// Build root -> b"windows" -> { geometry, openWindows, lockedWindows, stacksWindows }.
    fn doc_with_flags() -> Value {
        Value::Dict(vec![(
            Value::Bytes(b"windows".to_vec()),
            Value::Dict(vec![
                (
                    Value::Bytes(b"windowSizesAndPositions_1".to_vec()),
                    Value::Tuple(vec![
                        ts(),
                        Value::Dict(vec![
                            (Value::Bytes(b"overview".to_vec()), geom(1, 2, 3, 4, 2560, 1440)),
                            (Value::Bytes(b"market".to_vec()), geom(5, 6, 7, 8, 2560, 1440)),
                        ]),
                    ]),
                ),
                (
                    Value::Bytes(b"openWindows".to_vec()),
                    Value::Tuple(vec![
                        ts(),
                        Value::Dict(vec![
                            (Value::Bytes(b"overview".to_vec()), Value::Bool(true)),
                            (Value::Bytes(b"market".to_vec()), Value::Bool(false)),
                        ]),
                    ]),
                ),
                (
                    Value::Bytes(b"lockedWindows".to_vec()),
                    // Only overview has an entry; market's locked flag is absent.
                    Value::Tuple(vec![
                        ts(),
                        Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Bool(true))]),
                    ]),
                ),
                (
                    Value::Bytes(b"stacksWindows".to_vec()),
                    Value::Tuple(vec![
                        ts(),
                        Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Int(42))]),
                    ]),
                ),
            ]),
        )])
    }

    fn flag<'a>(w: &'a WindowRect, name: &str) -> &'a BoolFlag {
        w.flags.iter().find(|f| f.name == name).expect("flag present")
    }

    #[test]
    fn open_and_present_flags_carry_set_targets() {
        let doc = doc_with_flags();
        let wl = window_layout(&doc);
        let ov = &wl.windows[0];
        assert!(ov.open, "overview is open");
        assert_eq!(ov.flags.len(), 7);
        let locked = flag(ov, "lockedWindows");
        assert!(locked.value);
        // A present flag resolves to a set path over the real Bool(true).
        match &locked.set {
            SetTarget::Set { path } => assert_eq!(resolve(&doc, path), Some(&Value::Bool(true))),
            other => panic!("expected Set, got {other:?}"),
        }
    }

    #[test]
    fn an_absent_flag_carries_insert_params() {
        let doc = doc_with_flags();
        let wl = window_layout(&doc);
        let market = &wl.windows[1];
        assert!(!market.open, "market is closed");
        let locked = flag(market, "lockedWindows");
        assert!(!locked.value);
        // market has no lockedWindows entry -> insert with its byte-string key.
        match &locked.set {
            SetTarget::Insert { key, .. } => {
                assert!(matches!(key, NewValue::BytesHex(h) if h == "6d61726b6574")); // b"market"
            }
            other => panic!("expected Insert, got {other:?}"),
        }
    }

    #[test]
    fn a_missing_flag_dict_is_unavailable() {
        // doc_with (Task 1) has geometry but no flag dicts at all.
        let doc = doc_with(vec![(Value::Bytes(b"overview".to_vec()), geom(1, 2, 3, 4, 2560, 1440))]);
        let wl = window_layout(&doc);
        assert!(matches!(flag(&wl.windows[0], "openWindows").set, SetTarget::Unavailable));
    }

    #[test]
    fn stacks_is_an_editable_value_when_numeric() {
        let doc = doc_with_flags();
        let wl = window_layout(&doc);
        let ov = &wl.windows[0];
        let s = ov.stacks.as_ref().expect("overview has a stack id");
        assert_eq!(s.text, "42");
        assert_eq!(resolve(&doc, &s.path), Some(&Value::Int(42)));
        // market has no stacks entry.
        assert!(wl.windows[1].stacks.is_none());
    }

    #[test]
    fn reference_prefers_open_windows() {
        // Two closed windows at 1920x1080, one open at 2560x1440: the open one wins.
        let doc = Value::Dict(vec![(
            Value::Bytes(b"windows".to_vec()),
            Value::Dict(vec![
                (
                    Value::Bytes(b"windowSizesAndPositions_1".to_vec()),
                    Value::Tuple(vec![
                        ts(),
                        Value::Dict(vec![
                            (Value::Bytes(b"a".to_vec()), geom(0, 0, 1, 1, 1920, 1080)),
                            (Value::Bytes(b"b".to_vec()), geom(0, 0, 1, 1, 1920, 1080)),
                            (Value::Bytes(b"c".to_vec()), geom(0, 0, 1, 1, 2560, 1440)),
                        ]),
                    ]),
                ),
                (
                    Value::Bytes(b"openWindows".to_vec()),
                    Value::Tuple(vec![
                        ts(),
                        Value::Dict(vec![(Value::Bytes(b"c".to_vec()), Value::Bool(true))]),
                    ]),
                ),
            ]),
        )]);
        let wl = window_layout(&doc);
        assert_eq!((wl.reference_w, wl.reference_h), (2560, 1440));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run (PowerShell): `cargo test -p settings-model --lib windows::`
Expected: FAIL — `flags` is empty (`flag()` panics) and `NewValue` may not yet derive `Serialize`.

- [ ] **Step 3: Add `Serialize` to `NewValue`**

In `crates/settings-model/src/mutate.rs`, change the derive on `NewValue` (currently at line 51):

```rust
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum NewValue {
```

And extend the serde import near the top of `mutate.rs` (currently `use serde::Deserialize;`):

```rust
use serde::{Deserialize, Serialize};
```

- [ ] **Step 4: Implement the flags**

In `windows.rs`, replace the per-window body of the `for (wi, (key, val)) in geom_dict.iter().enumerate()` loop so it resolves flags and stacks. First, just after computing `geom_dict`/`geom_path` in `window_layout`, resolve the flag dicts once:

```rust
    // Optional sibling flag dicts, resolved once (each may be absent).
    let bool_dicts: Vec<Option<(&Entries, NodePath)>> = BOOL_FLAGS
        .iter()
        .map(|name| timestamped_dict(windows_dict, &windows_path, name.as_bytes()))
        .collect();
    let stacks_dict = timestamped_dict(windows_dict, &windows_path, b"stacksWindows");
```

Then replace the `windows.push(WindowRect { ... })` block inside the loop with:

```rust
        let mut flags = Vec::with_capacity(BOOL_FLAGS.len());
        let mut open = false;
        for (name, dict) in BOOL_FLAGS.iter().zip(&bool_dicts) {
            let (value, set) = match dict {
                Some((entries, dpath)) => bool_flag(entries, dpath, key),
                None => (false, SetTarget::Unavailable),
            };
            if *name == "openWindows" {
                open = value;
            }
            flags.push(BoolFlag { name: (*name).to_string(), value, set });
        }
        let stacks = stacks_dict
            .as_ref()
            .and_then(|(entries, dpath)| stack_field(entries, dpath, key));

        windows.push(WindowRect {
            id: id.clone(),
            label: id,
            open,
            renderable: geom.is_some(),
            resolution_matches: true, // fixed up below
            geom,
            flags,
            stacks,
        });
```

Add these helper functions below `extract_geom`:

```rust
fn bool_flag(entries: &Entries, dpath: &NodePath, key: &Value) -> (bool, SetTarget) {
    match entries.iter().enumerate().find(|(_, (k, _))| k == key) {
        Some((i, (_, v))) => {
            let mut p = dpath.clone();
            p.push(Step::DictValue(i));
            (matches!(v, Value::Bool(true)), SetTarget::Set { path: p })
        }
        None => match key_as_new_value(key) {
            Some(nv) => (false, SetTarget::Insert { parent: dpath.clone(), key: nv }),
            None => (false, SetTarget::Unavailable),
        },
    }
}

fn stack_field(entries: &Entries, dpath: &NodePath, key: &Value) -> Option<StackField> {
    let (i, (_, v)) = entries.iter().enumerate().find(|(_, (k, _))| k == key)?;
    // Editable only when the stack id is a plain integer; anything else stays
    // raw-tree-only rather than exposing a control that cannot round-trip.
    let Value::Int(n) = v else { return None };
    let mut p = dpath.clone();
    p.push(Step::DictValue(i));
    Some(StackField { text: n.to_string(), path: p })
}

/// Reconstruct a dict key as the `NewValue` an insert mutation needs. Window
/// ids are byte-strings or (parameterized) strings.
fn key_as_new_value(key: &Value) -> Option<NewValue> {
    match key {
        Value::Bytes(b) => Some(NewValue::BytesHex(hex(b))),
        Value::Str(s) => Some(NewValue::Str(s.clone())),
        Value::StrUcs2(s) => Some(NewValue::StrUcs2(s.clone())),
        _ => None,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
```

Finally, refine `reference_resolution` to prefer open windows:

```rust
/// The resolution the most windows agree on. Prefers open windows (what the
/// canvas actually draws); falls back to all renderable windows, then (0, 0).
fn reference_resolution(windows: &[WindowRect]) -> (i64, i64) {
    let res = |w: &WindowRect| w.geom.as_ref().map(|g| (g.screen_w, g.screen_h));
    mode(windows.iter().filter(|w| w.open).filter_map(res))
        .or_else(|| mode(windows.iter().filter_map(res)))
        .unwrap_or((0, 0))
}
```

`SetTarget` must derive `Debug` for the `panic!("...{other:?}")` in the tests — it already does (Task 1 derives `Debug, Serialize`). `NewValue` also already derives `Debug`.

- [ ] **Step 5: Run the tests to verify they pass**

Run (PowerShell): `cargo test -p settings-model`
Expected: PASS — all `windows::` tests plus the existing suite (the `BOOL_FLAGS` unused warning is gone).

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/src/windows.rs crates/settings-model/src/mutate.rs
git commit -m "Project window flags and stack ids with their mutation targets"
```

---

### Task 3: Expose `window_layout` as a Tauri command and mirror it in `api.ts`

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (add `window_layout` fn + a test)
- Modify: `app/src-tauri/src/lib.rs` (command wrapper + register in `generate_handler!`)
- Modify: `app/src/lib/api.ts` (types + `api.windowLayout`)

**Interfaces:**
- Consumes: `settings_model::{window_layout, WindowLayout}`; `AppState`.
- Produces:
  - Rust: `pub fn window_layout(state: &AppState) -> Result<settings_model::WindowLayout, ErrDto>`; command `window_layout`.
  - TS: `api.windowLayout(): Promise<WindowLayout>` and the `WindowLayout`/`WindowRect`/`Geom`/`BoolFlag`/`StackField`/`SetTarget` interfaces.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` in `app/src-tauri/src/ops.rs` (it already imports `encode`, `Value`, `PathBuf`, `temp_file`):

```rust
    #[test]
    fn window_layout_reads_the_open_document() {
        // A minimal char-style file: one open window with geometry.
        let doc = Value::Dict(vec![(
            Value::Bytes(b"windows".to_vec()),
            Value::Dict(vec![
                (
                    Value::Bytes(b"windowSizesAndPositions_1".to_vec()),
                    Value::Tuple(vec![
                        Value::Long(vec![0u8; 8]),
                        Value::Dict(vec![(
                            Value::Bytes(b"overview".to_vec()),
                            Value::Tuple(vec![
                                Value::Int(1), Value::Int(2), Value::Int(3),
                                Value::Int(4), Value::Int(2560), Value::Int(1440),
                            ]),
                        )]),
                    ]),
                ),
                (
                    Value::Bytes(b"openWindows".to_vec()),
                    Value::Tuple(vec![
                        Value::Long(vec![0u8; 8]),
                        Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Bool(true))]),
                    ]),
                ),
            ]),
        )]);
        let path = temp_file("winlayout", &encode(&doc).unwrap());
        let state = AppState::new();
        open_file(&state, path.to_str().unwrap()).unwrap();

        let wl = window_layout(&state).unwrap();
        assert_eq!((wl.reference_w, wl.reference_h), (2560, 1440));
        assert_eq!(wl.windows.len(), 1);
        assert_eq!(wl.windows[0].id, "overview");
        assert!(wl.windows[0].open);
    }

    #[test]
    fn window_layout_without_a_document_errors() {
        let state = AppState::new();
        assert_eq!(window_layout(&state).unwrap_err().code, "no_document");
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run (PowerShell): `cargo test -p app --lib window_layout`
Expected: FAIL — `ops::window_layout` is not defined.

- [ ] **Step 3: Implement the command function**

In `app/src-tauri/src/ops.rs`, extend the `settings_model` import list (currently ends `...Node, Profile, SaveReport,`) to include the new items:

```rust
use settings_model::{
    apply, default_roots, discover, project, save, window_layout as project_window_layout,
    Document, Fidelity, LoadError, Mutation, Node, Profile, SaveReport, WindowLayout,
};
```

Add this function (next to `list_file_backups`):

```rust
pub fn window_layout(state: &AppState) -> Result<WindowLayout, ErrDto> {
    let guard = state.0.lock().unwrap();
    let doc = guard.as_ref().ok_or_else(|| ErrDto::new("no_document", "no file open"))?;
    Ok(project_window_layout(&doc.value))
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run (PowerShell): `cargo test -p app --lib window_layout`
Expected: PASS — 2 tests.

- [ ] **Step 5: Register the Tauri command**

In `app/src-tauri/src/lib.rs`, add the wrapper after `restore_backup`:

```rust
#[tauri::command]
fn window_layout(
    state: tauri::State<'_, AppState>,
) -> Result<settings_model::WindowLayout, ErrDto> {
    ops::window_layout(&state)
}
```

And add `window_layout` to the `generate_handler!` list:

```rust
        .invoke_handler(tauri::generate_handler![
            discover_profiles, open_file, close_file,
            apply_mutation, save_document, list_file_backups, restore_backup,
            window_layout
        ])
```

- [ ] **Step 6: Add the TypeScript contract**

In `app/src/lib/api.ts`, add these interfaces after the `Mutation` type:

```typescript
export interface Geom {
  x: number;
  y: number;
  w: number;
  h: number;
  screen_w: number;
  screen_h: number;
  x_path: NodePath;
  y_path: NodePath;
  w_path: NodePath;
  h_path: NodePath;
  screen_w_path: NodePath;
  screen_h_path: NodePath;
}

export type SetTarget =
  | { how: "set"; path: NodePath }
  | { how: "insert"; parent: NodePath; key: NewValue }
  | { how: "unavailable" };

export interface BoolFlag {
  name: string;
  value: boolean;
  set: SetTarget;
}

export interface StackField {
  text: string;
  path: NodePath;
}

export interface WindowRect {
  id: string;
  label: string;
  open: boolean;
  renderable: boolean;
  resolution_matches: boolean;
  geom: Geom | null;
  flags: BoolFlag[];
  stacks: StackField | null;
}

export interface WindowLayout {
  reference_w: number;
  reference_h: number;
  windows: WindowRect[];
}
```

And add to the `api` object (after the `restoreBackup` line):

```typescript
  windowLayout: () => invoke<WindowLayout>("window_layout"),
```

- [ ] **Step 7: Verify the frontend still type-checks**

Run (PowerShell): `npm run check --prefix app`
Expected: 0 errors (the 2 pre-existing `state_referenced_locally` warnings may remain).

- [ ] **Step 8: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Expose the window-layout projection as a command"
```

---

### Task 4: Frontend layout math (`layout.ts`) with tests

Pure helpers the canvas uses: scale, data↔canvas conversion (the drag math), and the open-window filter. These are the only canvas logic worth unit-testing per the spec; DOM interaction is covered by the manual smoke.

**Files:**
- Create: `app/src/lib/layout.ts`
- Test: `app/src/lib/layout.test.ts`

**Interfaces:**
- Consumes: `WindowRect` from `./api.ts`.
- Produces: `canvasScale`, `toCanvas`, `toData`, `openWindows`.

- [ ] **Step 1: Write the failing test**

Create `app/src/lib/layout.test.ts`:

```typescript
// Run: npm test (node --test; Node strips the types). Throw-based checks, no
// framework — matching search.test.ts.
import { canvasScale, toCanvas, toData, openWindows } from "./layout.ts";
import type { WindowRect } from "./api.ts";

const check = (name: string, ok: boolean) => {
  if (!ok) throw new Error(`FAIL: ${name}`);
  console.log(`  ok - ${name}`);
};

check("scale maps reference width onto the container", canvasScale(2560, 1280) === 0.5);
check("scale is 1 when the reference has no width", canvasScale(0, 1280) === 1);

// The drag round-trip: a data value converted to canvas px and back is itself.
for (const scale of [0.5, 0.37, 1, 2]) {
  for (const v of [0, 1, 16, 424, 2559]) {
    check(
      `round-trip v=${v} scale=${scale}`,
      toData(toCanvas(v, scale), scale) === v,
    );
  }
}

const win = (id: string, open: boolean, renderable: boolean): WindowRect => ({
  id,
  label: id,
  open,
  renderable,
  resolution_matches: true,
  geom: renderable
    ? {
        x: 0, y: 0, w: 1, h: 1, screen_w: 2560, screen_h: 1440,
        x_path: [], y_path: [], w_path: [], h_path: [],
        screen_w_path: [], screen_h_path: [],
      }
    : null,
  flags: [],
  stacks: null,
});

const wins = [win("a", true, true), win("b", false, true), win("c", true, false)];
const open = openWindows(wins);
check("open filter keeps only open AND renderable windows", open.length === 1);
check("open filter keeps the right window", open[0].id === "a");

console.log("layout: all checks passed");
```

- [ ] **Step 2: Run the test to verify it fails**

Run (PowerShell): `npm test --prefix app`
Expected: FAIL — `./layout.ts` does not exist.

- [ ] **Step 3: Implement `layout.ts`**

Create `app/src/lib/layout.ts`:

```typescript
// Pure geometry helpers for the layout canvas. No DOM, no Svelte — unit-tested
// in layout.test.ts.
import type { WindowRect } from "./api";

/** Canvas px per data px. 1 when the reference has no width (empty file). */
export function canvasScale(referenceWidth: number, containerWidth: number): number {
  return referenceWidth > 0 ? containerWidth / referenceWidth : 1;
}

/** Data px -> canvas px. */
export function toCanvas(dataPx: number, scale: number): number {
  return dataPx * scale;
}

/** Canvas px -> data px, rounded to the integer the wire format stores. */
export function toData(canvasPx: number, scale: number): number {
  return scale > 0 ? Math.round(canvasPx / scale) : 0;
}

/** Windows the canvas draws: open and with valid geometry. */
export function openWindows(windows: WindowRect[]): WindowRect[] {
  return windows.filter((w) => w.open && w.renderable);
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run (PowerShell): `npm test --prefix app`
Expected: PASS — `layout: all checks passed` (and `searchTree` still passes).

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/layout.ts app/src/lib/layout.test.ts
git commit -m "Add pure layout-canvas geometry helpers"
```

---

### Task 5: `WindowPanel.svelte` — accordion list with inline detail

Presentational master-detail: a row per window (open checkbox, name, mismatch badge); the selected row expands to `x/y/w/h` inputs, the seven flag toggles, and the stack-id input. It emits intents through callbacks; the parent owns the working model and commits.

**Files:**
- Create: `app/src/lib/WindowPanel.svelte`

**Interfaces:**
- Consumes: `WindowRect`, `BoolFlag` from `$lib/api`.
- Produces (props):
  - `windows: WindowRect[]`
  - `selectedId: string | null`
  - `readOnly: boolean`
  - `onSelect: (id: string) => void`
  - `onToggleOpen: (w: WindowRect) => void`
  - `onGeom: (w: WindowRect, field: "x" | "y" | "w" | "h", value: number) => void`
  - `onFlag: (w: WindowRect, flag: BoolFlag, value: boolean) => void`
  - `onStack: (w: WindowRect, text: string) => void`

- [ ] **Step 1: Create the component**

Create `app/src/lib/WindowPanel.svelte`:

```svelte
<script lang="ts">
  import type { WindowRect, BoolFlag } from "$lib/api";

  let {
    windows,
    selectedId,
    readOnly,
    onSelect,
    onToggleOpen,
    onGeom,
    onFlag,
    onStack,
  }: {
    windows: WindowRect[];
    selectedId: string | null;
    readOnly: boolean;
    onSelect: (id: string) => void;
    onToggleOpen: (w: WindowRect) => void;
    onGeom: (w: WindowRect, field: "x" | "y" | "w" | "h", value: number) => void;
    onFlag: (w: WindowRect, flag: BoolFlag, value: boolean) => void;
    onStack: (w: WindowRect, text: string) => void;
  } = $props();

  // Flags shown in the detail; openWindows lives on the row header instead.
  const detailFlags = (w: WindowRect) => w.flags.filter((f) => f.name !== "openWindows");

  const COORDS = ["x", "y", "w", "h"] as const;

  const numberEdit = (w: WindowRect, field: "x" | "y" | "w" | "h") => (e: Event) => {
    const v = parseInt((e.target as HTMLInputElement).value, 10);
    if (!Number.isNaN(v)) onGeom(w, field, v);
  };
</script>

<div class="window-panel">
  {#each windows as w (w.id)}
    {@const openFlag = w.flags.find((f) => f.name === "openWindows")}
    <div class="row" class:selected={w.id === selectedId}>
      <div class="row-head">
        <input
          type="checkbox"
          checked={w.open}
          disabled={readOnly || openFlag?.set.how === "unavailable"}
          title="Open (shown on the canvas)"
          onchange={() => onToggleOpen(w)} />
        <button class="name" onclick={() => onSelect(w.id)}>
          {w.label}
        </button>
        {#if !w.renderable}
          <span class="badge warn" title="Geometry is not a 6-tuple — edit in the raw tree">
            unrenderable
          </span>
        {:else if !w.resolution_matches}
          <span class="badge warn" title="Saved at a different resolution than the canvas">
            {w.geom?.screen_w}×{w.geom?.screen_h}
          </span>
        {/if}
      </div>

      {#if w.id === selectedId && w.geom}
        <div class="detail">
          <div class="coords">
            {#each COORDS as field}
              <label>
                {field}
                <input
                  type="number"
                  value={w.geom[field]}
                  disabled={readOnly}
                  onchange={numberEdit(w, field)} />
              </label>
            {/each}
          </div>
          <div class="flags">
            {#each detailFlags(w) as f (f.name)}
              <label class="flag" title={f.set.how === "unavailable" ? "Not present in this file" : ""}>
                <input
                  type="checkbox"
                  checked={f.value}
                  disabled={readOnly || f.set.how === "unavailable"}
                  onchange={(e) => onFlag(w, f, (e.target as HTMLInputElement).checked)} />
                {f.name}
              </label>
            {/each}
          </div>
          {#if w.stacks}
            <label class="stack">
              stack id
              <input
                type="number"
                value={w.stacks.text}
                disabled={readOnly}
                onchange={(e) => onStack(w, (e.target as HTMLInputElement).value)} />
            </label>
          {/if}
        </div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .window-panel {
    overflow-y: auto;
    font-size: 13px;
    border-left: 1px solid #ddd;
    min-width: 16rem;
  }
  .row {
    border-bottom: 1px solid #eee;
  }
  .row.selected {
    background: #eef4ff;
  }
  .row-head {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.25rem 0.5rem;
  }
  .name {
    flex: 1;
    text-align: left;
    background: none;
    border: none;
    cursor: pointer;
    font: inherit;
    padding: 0;
  }
  .badge.warn {
    background: #fde68a;
    border-radius: 3px;
    padding: 0 0.3rem;
    font-size: 11px;
  }
  .detail {
    padding: 0.4rem 0.6rem 0.6rem;
    display: grid;
    gap: 0.5rem;
  }
  .coords {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0.3rem;
  }
  .coords label,
  .stack {
    display: grid;
    gap: 0.1rem;
    font-size: 11px;
  }
  .coords input {
    width: 100%;
    box-sizing: border-box;
  }
  .flags {
    display: grid;
    gap: 0.15rem;
  }
  .flag {
    display: flex;
    align-items: center;
    gap: 0.3rem;
  }
</style>
```

- [ ] **Step 2: Verify it type-checks**

Run (PowerShell): `npm run check --prefix app`
Expected: 0 errors (the 2 pre-existing warnings may remain).

- [ ] **Step 3: Commit**

```bash
git add app/src/lib/WindowPanel.svelte
git commit -m "Add the window accordion panel component"
```

---

### Task 6: `LayoutView.svelte` + `+page.svelte` integration

The orchestrator: fetches `window_layout`, draws open windows as draggable/resizable rectangles, keeps the working model and selection, and commits every edit through the parent's `runMutation`. Then wire a Tree/Layout switch into the page, shown only when the file has a layout.

**Files:**
- Create: `app/src/lib/LayoutView.svelte`
- Modify: `app/src/routes/+page.svelte`

**Interfaces:**
- Consumes: `api.windowLayout`, `WindowLayout`, `WindowRect`, `BoolFlag`, `Mutation`, `NewValue` from `$lib/api`; `canvasScale`, `toCanvas`, `toData`, `openWindows` from `$lib/layout`; `WindowPanel`.
- Produces (props):
  - `runMutation: (m: Mutation, rethrow?: boolean) => Promise<void>`
  - `readOnly: boolean`
  - `refreshToken: number` — parent bumps it (e.g. `savedAt`) to force a reload after save/restore.

- [ ] **Step 1: Create `LayoutView.svelte`**

Create `app/src/lib/LayoutView.svelte`:

```svelte
<script lang="ts">
  import { api, errMessage } from "$lib/api";
  import type { WindowLayout, WindowRect, BoolFlag, Mutation, NewValue } from "$lib/api";
  import { canvasScale, toCanvas, toData, openWindows } from "$lib/layout";
  import WindowPanel from "$lib/WindowPanel.svelte";
  import { message } from "@tauri-apps/plugin-dialog";

  let {
    runMutation,
    readOnly,
    refreshToken,
  }: {
    runMutation: (m: Mutation, rethrow?: boolean) => Promise<void>;
    readOnly: boolean;
    refreshToken: number;
  } = $props();

  let layout: WindowLayout | null = $state(null);
  let selectedId: string | null = $state(null);
  let containerWidth = $state(0);
  let canvasEl: HTMLDivElement | undefined = $state();
  // Live drag/resize preview by window id (data px); absent when not dragging.
  let preview: Record<string, { x: number; y: number; w: number; h: number }> = $state({});

  const scale = $derived(layout ? canvasScale(layout.reference_w, containerWidth) : 1);
  const drawn = $derived(layout ? openWindows(layout.windows) : []);
  const canvasHeight = $derived(layout ? toCanvas(layout.reference_h, scale) : 0);

  async function load() {
    try {
      layout = await api.windowLayout();
      if (selectedId && !layout.windows.some((w) => w.id === selectedId)) {
        selectedId = null;
      }
    } catch (e) {
      await message(errMessage(e), { title: "Layout unavailable", kind: "error" });
    }
  }

  // Reload when the parent signals a save/restore.
  let lastToken = -1;
  $effect(() => {
    if (refreshToken !== lastToken) {
      lastToken = refreshToken;
      load();
    }
  });

  // Rect position/size in data px: the live preview if dragging, else committed.
  const rectOf = (w: WindowRect) => preview[w.id] ?? {
    x: w.geom!.x, y: w.geom!.y, w: w.geom!.w, h: w.geom!.h,
  };

  // --- Mutations -----------------------------------------------------------

  function flagMutation(flag: BoolFlag, next: boolean): Mutation | null {
    if (flag.set.how === "set") {
      return { op: "set_scalar", path: flag.set.path, text: next ? "true" : "false" };
    }
    if (flag.set.how === "insert") {
      const value: NewValue = { kind: "bool", v: next };
      return { op: "insert_dict_entry", parent: flag.set.parent, key: flag.set.key, value };
    }
    return null; // unavailable
  }

  function geomMutations(w: WindowRect, next: { x?: number; y?: number; w?: number; h?: number }): Mutation[] {
    const g = w.geom!;
    const ms: Mutation[] = [];
    const setInt = (path: typeof g.x_path, v: number) =>
      ms.push({ op: "set_scalar", path, text: String(v) });
    if (next.x !== undefined && next.x !== g.x) setInt(g.x_path, next.x);
    if (next.y !== undefined && next.y !== g.y) setInt(g.y_path, next.y);
    if (next.w !== undefined && next.w !== g.w) setInt(g.w_path, next.w);
    if (next.h !== undefined && next.h !== g.h) setInt(g.h_path, next.h);
    // New coords are in the reference resolution; align this window's saved
    // resolution to it so the numbers stay meaningful.
    if (ms.length > 0 && !w.resolution_matches && layout) {
      setInt(g.screen_w_path, layout.reference_w);
      setInt(g.screen_h_path, layout.reference_h);
    }
    return ms;
  }

  async function commit(ms: Mutation[]) {
    if (ms.length === 0) return;
    try {
      for (const m of ms) await runMutation(m, true);
    } catch (e) {
      await message(errMessage(e), { title: "Edit failed", kind: "error" });
    }
    await load(); // refresh paths/values from the authoritative document
  }

  // --- Panel callbacks -----------------------------------------------------

  const onSelect = (id: string) => (selectedId = id);

  function onToggleOpen(w: WindowRect) {
    const open = w.flags.find((f) => f.name === "openWindows");
    if (!open) return;
    const m = flagMutation(open, !open.value);
    if (m) commit([m]);
  }

  const onGeom = (w: WindowRect, field: "x" | "y" | "w" | "h", value: number) =>
    commit(geomMutations(w, { [field]: value }));

  function onFlag(w: WindowRect, flag: BoolFlag, value: boolean) {
    const m = flagMutation(flag, value);
    if (m) commit([m]);
  }

  const onStack = (w: WindowRect, text: string) =>
    w.stacks && commit([{ op: "set_scalar", path: w.stacks.path, text }]);

  // --- Canvas drag & resize ------------------------------------------------

  type Drag =
    | { kind: "move"; w: WindowRect; startX: number; startY: number; ox: number; oy: number }
    | { kind: "resize"; w: WindowRect; startX: number; startY: number; ow: number; oh: number };
  let drag: Drag | null = null;

  // Capture on the canvas (not the rectangle) so its onpointermove/up keep
  // firing even as the pointer leaves the rectangle during a drag.
  function startMove(w: WindowRect, e: PointerEvent) {
    if (readOnly) return;
    selectedId = w.id;
    drag = { kind: "move", w, startX: e.clientX, startY: e.clientY, ox: w.geom!.x, oy: w.geom!.y };
    canvasEl?.setPointerCapture(e.pointerId);
    e.preventDefault();
  }

  function startResize(w: WindowRect, e: PointerEvent) {
    if (readOnly) return;
    selectedId = w.id;
    drag = { kind: "resize", w, startX: e.clientX, startY: e.clientY, ow: w.geom!.w, oh: w.geom!.h };
    canvasEl?.setPointerCapture(e.pointerId);
    e.preventDefault();
    e.stopPropagation();
  }

  function onPointerMove(e: PointerEvent) {
    if (!drag) return;
    const dx = toData(e.clientX - drag.startX, scale);
    const dy = toData(e.clientY - drag.startY, scale);
    if (drag.kind === "move") {
      preview = { ...preview, [drag.w.id]: { ...rectOf(drag.w), x: drag.ox + dx, y: drag.oy + dy } };
    } else {
      preview = {
        ...preview,
        [drag.w.id]: { ...rectOf(drag.w), w: Math.max(0, drag.ow + dx), h: Math.max(0, drag.oh + dy) },
      };
    }
  }

  function onPointerUp() {
    if (!drag) return;
    const w = drag.w;
    const p = preview[w.id];
    const d = drag;
    drag = null;
    const rest = { ...preview };
    delete rest[w.id];
    preview = rest;
    if (!p) return;
    commit(geomMutations(w, d.kind === "move" ? { x: p.x, y: p.y } : { w: p.w, h: p.h }));
  }
</script>

{#if layout === null}
  <p class="hint">Loading layout…</p>
{:else}
  <div class="layout-view">
    <div class="canvas-wrap" bind:clientWidth={containerWidth}>
      <div
        class="canvas"
        bind:this={canvasEl}
        style="width: {toCanvas(layout.reference_w, scale)}px; height: {canvasHeight}px;"
        onpointermove={onPointerMove}
        onpointerup={onPointerUp}>
        {#each drawn as w (w.id)}
          {@const r = rectOf(w)}
          <div
            class="win"
            class:selected={w.id === selectedId}
            style="left: {toCanvas(r.x, scale)}px; top: {toCanvas(r.y, scale)}px;
                   width: {toCanvas(r.w, scale)}px; height: {toCanvas(r.h, scale)}px;"
            onpointerdown={(e) => startMove(w, e)}>
            <span class="win-label">{w.label}</span>
            <span class="resize" onpointerdown={(e) => startResize(w, e)}></span>
          </div>
        {/each}
      </div>
      <p class="ref">reference {layout.reference_w}×{layout.reference_h}</p>
    </div>
    <WindowPanel
      windows={layout.windows}
      {selectedId}
      {readOnly}
      {onSelect}
      {onToggleOpen}
      {onGeom}
      {onFlag}
      {onStack} />
  </div>
{/if}

<style>
  .layout-view {
    display: grid;
    grid-template-columns: 1fr auto;
    height: 100%;
    overflow: hidden;
  }
  .canvas-wrap {
    overflow: auto;
    padding: 0.5rem;
  }
  .canvas {
    position: relative;
    background: #1b1f27;
    background-image: linear-gradient(#2a2f3a 1px, transparent 1px),
      linear-gradient(90deg, #2a2f3a 1px, transparent 1px);
    background-size: 40px 40px;
    border: 1px solid #444;
  }
  .win {
    position: absolute;
    box-sizing: border-box;
    background: rgba(96, 165, 250, 0.25);
    border: 1px solid #60a5fa;
    color: #dbeafe;
    font-size: 11px;
    overflow: hidden;
    cursor: move;
    touch-action: none;
  }
  .win.selected {
    border-color: #f59e0b;
    background: rgba(245, 158, 11, 0.25);
    z-index: 1;
  }
  .win-label {
    padding: 1px 3px;
    display: inline-block;
    pointer-events: none;
  }
  .resize {
    position: absolute;
    right: 0;
    bottom: 0;
    width: 12px;
    height: 12px;
    cursor: se-resize;
    background: currentColor;
    opacity: 0.6;
    touch-action: none;
  }
  .ref {
    color: #888;
    font-size: 11px;
    margin: 0.3rem 0 0;
  }
  .hint {
    color: #888;
    padding: 1rem;
  }
</style>
```

- [ ] **Step 2: Wire the view switch into `+page.svelte`**

In `app/src/routes/+page.svelte`, add the import (with the other `$lib` imports):

```typescript
  import LayoutView from "$lib/LayoutView.svelte";
```

Add state (after the `savedAt` line):

```typescript
  let view: "tree" | "layout" = $state("tree");
  let layoutAvailable = $state(false);
```

In `openFile`, after `savedAt += 1;` on the success path, probe for a layout and reset the view:

```typescript
      view = "tree";
      try {
        layoutAvailable =
          current.status === "opened" && (await api.windowLayout()).windows.length > 0;
      } catch {
        layoutAvailable = false;
      }
```

In the `filebar` header, add view tabs before the `<span class="spacer">`:

```svelte
        {#if layoutAvailable}
          <span class="viewtabs">
            <button class:active={view === "tree"} onclick={() => (view = "tree")}>Tree</button>
            <button class:active={view === "layout"} onclick={() => (view = "layout")}>Layout</button>
          </span>
        {/if}
```

Replace the search bar + tree area block (the `<div class="searchbar">…</div>` and the following `<div class="tree-area">…</div>`) with a conditional on `view`:

```svelte
      {#if view === "layout"}
        <div class="tree-area">
          <LayoutView
            {runMutation}
            readOnly={current.fidelity.state !== "editable"}
            refreshToken={savedAt} />
        </div>
      {:else}
        <div class="searchbar">
          <input
            class="search"
            bind:this={searchBox}
            bind:value={query}
            placeholder="Search labels and values (Ctrl+F)" />
          {#if searching}
            <span class="meta">
              {found?.count ?? 0} match{found?.count === 1 ? "" : "es"}
            </span>
            <button class="mini" title="Clear search (Esc)" onclick={closeSearch}>×</button>
          {/if}
        </div>
        <div class="tree-area">
          {#if found?.tree}
            <TreeNode
              node={found.tree}
              autoExpand={searching}
              onEdit={handleEdit}
              onRemove={handleRemove}
              onInsertRequest={(n) => (insertTarget = n)} />
          {:else}
            <p class="hint">Nothing in this file matches “{query}”.</p>
          {/if}
        </div>
      {/if}
```

Add styling for the tabs in the `<style>` block:

```css
  .viewtabs button {
    border: 1px solid #ccc;
    background: #f6f6f6;
    padding: 0.1rem 0.6rem;
    cursor: pointer;
  }
  .viewtabs button.active {
    background: #dbeafe;
    border-color: #60a5fa;
  }
```

- [ ] **Step 3: Verify type-check and build**

Run (PowerShell): `npm run check --prefix app`
Expected: 0 errors (pre-existing warnings may remain).

Run (PowerShell): `npm run build --prefix app`
Expected: build completes with no errors.

- [ ] **Step 4: Commit**

```bash
git add app/src/lib/LayoutView.svelte app/src/routes/+page.svelte
git commit -m "Add the layout canvas view and the Tree/Layout switch"
```

---

### Task 7: [USER REQUIRED] Manual smoke validation

Automated tests cover the projection and the math; this confirms the canvas edits a real document end-to-end and the raw tree/save chain see the change. No live in-game gate is required — M1 already proved the save chain — but do exercise a real file copied into the corpus.

**Files:** none (manual).

- [ ] **Step 1: Run the app against a real char file**

Run (PowerShell): `npm run tauri dev --prefix app` (or the project's usual dev command). Open a `core_char_*.dat` from a profile (these are the files that carry geometry).

- [ ] **Step 2: Verify canvas behaviour**

Confirm: only open windows appear as rectangles; the window panel lists all windows; toggling a closed window's checkbox makes it appear on the canvas; dragging a rectangle moves it; the bottom-right handle resizes it; selecting a window shows its `x/y/w/h`, the seven flag toggles, and (if present) the stack-id input; typing a number moves/resizes the rectangle.

- [ ] **Step 3: Verify document consistency**

Switch to the Tree view and confirm the edited window's 6-tuple under `windows → windowSizesAndPositions_1` reflects the new values; the `unsaved changes` badge is shown. Save (Ctrl+S), reopen the file, and confirm the changes persisted and a backup was written.

- [ ] **Step 4: Record the result**

Note the outcome in the M2 section of `docs/format-notes.md` (or the progress ledger), mirroring how M1's live validation was recorded. Commit any doc note:

```bash
git add docs/format-notes.md
git commit -m "Record the M2 layout canvas manual validation result"
```

---

## Self-Review

**Spec coverage:**
- Canvas draws open windows, panel lists all with open toggle → Tasks 5, 6. ✓
- Geometry + all flags editable; `stacksWindows` as a value input → Tasks 2, 5. ✓
- Reference resolution = mode, warn on mismatch → Tasks 1, 2 (reference + `resolution_matches`), 5 (badge). ✓
- Rewrite `screen_w/h` to reference on edit → Task 6 (`geomMutations`). ✓
- Rust read projection, writes reuse `apply_mutation` → Tasks 1–3, 6. ✓
- Two components + one command, no new write path → Tasks 3, 5, 6. ✓
- Error handling: no geometry → no tab (Task 6 probe); read-only disables edits (Tasks 5, 6 `readOnly`); malformed window listed unrenderable (Tasks 1, 5); mismatch badge (Task 5). ✓
- Snap-to-grid, background image deferred → not built. ✓
- Testing: Rust projection unit + round-trip (Tasks 1–3), frontend node --test math (Task 4), manual smoke (Task 7). ✓

**Type consistency:** `WindowLayout`/`WindowRect`/`Geom`/`BoolFlag`/`StackField`/`SetTarget` are identical across Rust (Task 1/2), the command (Task 3), and TS (Task 3), and consumed unchanged in Tasks 5–6. `SetTarget` serde tag `how` with `set`/`insert`/`unavailable` matches the TS union. `flagMutation`/`geomMutations` use only fields defined on those types.

**Placeholder scan:** none — every step carries complete code or an exact command with expected output.

## Deferred (carried from the spec)

- Snap-to-grid; background game-screen image.
- Purpose-built window-stacking UI (M2 exposes only a numeric `stacksWindows` value).
- Top/left resize handles (M2 ships move + bottom-right resize; `x/y/w/h` inputs cover the rest).
- Known-window label prettification (`label` is the raw id for now; the field exists so a name map can be added without a shape change).
