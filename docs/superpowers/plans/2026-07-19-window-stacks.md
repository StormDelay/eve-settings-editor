# Window Stacks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Model EVE window stacks and make the layout canvas treat each open stack as one coherent tabbed rectangle you can move/resize as a unit and edit membership on (unstack / add / reorder / create).

**Architecture:** A read projection in `settings-model/windows.rs` resolves the two stack dicts (`stacksWindows` member→container, `preferredIdxInStack3` tab order) through Ref/Shared and exposes `stacks` + per-window `stack` refs with a chosen anchor. Membership authoring is a new `settings-model/stacks.rs` of pure `&mut Value` mutators (inline the `windows` subtree, edit plain values), driven by an `ops.rs` `edit_char_stacks` wrapper that reshares after each edit (mirrors `edit_user_overview`, but on the **char** slot). Coherent geometry move/resize reuses the existing M2 `set_scalar` path, fanned out to the container + every open member. The frontend groups open windows by stack, draws one tabbed rect, and adds panel controls.

**Tech Stack:** Rust (`settings-model` lib, `app` Tauri backend), SvelteKit 5 runes frontend, TypeScript, `node --test` (zero-dep). `blue_marshal` codec (local crate) provides `decode`/`encode`/`inline`/`reshare`.

## Global Constraints

- **No new dependencies** — `settings-model`, `app`, and the frontend use only what's already present (`blue_marshal` is a local crate).
- **Membership edits target the CHAR file** — the `windows` subtree lives in `core_char`. Edit the `Slot::Char` document; the app wrapper runs `doc.value = blue_marshal::reshare(&doc.value)` after each edit (mirrors `edit_user_overview`/`edit_user_autofill`, which do this on the user slot).
- **Structural edits inline first.** `RemoveEntry` refuses `Shared` subtrees and window ids are shared stores, so the `stacks.rs` mutators call `crate::treewalk::inline_all(v)` on the doc before editing plain values. The app wrapper's `reshare` re-compacts on the way out (v0.7.0 foundation).
- **Geometry move/resize is NOT a new backend path** — it emits `set_scalar` (via the existing `api.mutate`) on the `x/y/w/h` paths the projection already hands over, for the container + every open member.
- **Stack ids are window-id refs or `None`, never `Int`.** Resolve keys/values with `crate::treewalk::effective(k, &shared)`, exactly as the flag-key lookups already do. The dead Int-only `StackField`/`stack_field` is removed.
- **`preferredIdxInStack3` is a soft hint on read** (indices collide / name closed members); **normalize to clean 0..N on write.**
- **Anchor rule:** a stack's drawn rect = the container window's geom if the container is renderable, else the **frontmost open member** = the open member with the lowest normalized preferred index (ties broken by id), else the first member.
- **Create recipe** (from `docs/format-notes.md` → "Window stacks"): mint a **high free** integer container id `C` (free across the window dicts), land the stack at **M1's current rect** (M1 = the window the action started from; write `C` and M2 to that rect), `stacksWindows[M1]=stacksWindows[M2]=C`, `preferredIdxInStack3[C]={M1:0,M2:1}`, open `C`+M1+M2, and `C=False` in `isLightBackgroundWindows`/`isOverlayedWindows`/`minimizedWindows`.
- **Tests:** Rust unit (`cargo test -p settings-model` / `-p app`); frontend `node --test` zero-dep, throw-based `check(name, ok)` idiom (`npm test` from `app/`); `npm run check` (svelte-check) 0 errors. **cargo is on the Bash PATH; npm is NOT — run npm via the PowerShell tool from `app/`.** DOM drag/interaction is not unit-tested (M2 norm) — verified by svelte-check + the Task 8 live smoke.
- **Commits:** sentence-case, **no attribution trailers**.
- **Serde:** new enums use `#[serde(rename_all = "snake_case")]` / tagged unions matching the existing `SetTarget` style; the frontend `api.ts` types mirror them exactly.

---

### Task 1: Projection — real stack grouping in `windows.rs`

**Files:**
- Modify: `crates/settings-model/src/windows.rs` (add `Stack`/`StackRef`/`StackRole`, `WindowLayout.stacks`, `WindowRect.stack`; remove `StackField`/`stack_field`; add resolution + anchor logic + tests)
- Modify: `crates/settings-model/src/lib.rs` (export the new types)

**Interfaces:**
- Consumes: existing `treewalk::{child_dict, timestamped_dict, effective, collect_shared, SharedTable, Entries}`, `WindowRect`, `Geom`.
- Produces:
  - `pub struct Stack { pub container_id: String, pub container_label: String, pub anchor_id: String, pub members: Vec<String> }`
  - `pub enum StackRole { Container, Member }` (serde snake_case)
  - `pub struct StackRef { pub container_id: String, pub role: StackRole }`
  - `WindowLayout` gains `pub stacks: Vec<Stack>`; `WindowRect`'s `pub stacks: Option<StackField>` is replaced by `pub stack: Option<StackRef>`.

- [ ] **Step 1: Write the failing tests**

Add to `crates/settings-model/src/windows.rs`'s `#[cfg(test)] mod tests` (a `build(...)` helper for window dicts already exists there; these tests build the two stack dicts inline). Append:

```rust
    // Helper: a char-style root with geometry, openWindows, stacksWindows and
    // preferredIdxInStack3 — window ids as Bytes, stack values through a Shared.
    fn stacked_root() -> Value {
        fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        fn ts() -> Value { Value::Long(vec![0u8; 8]) }
        fn geom(x: i64) -> Value {
            Value::Tuple(vec![
                Value::Int(x), Value::Int(0), Value::Int(100), Value::Int(80),
                Value::Int(2560), Value::Int(1440),
            ])
        }
        // container "88" is a Shared, Ref'd from member m2 and from the pref dict.
        let container = Value::Shared { slot: 3, value: Box::new(b("88")) };
        let windows = Value::Dict(vec![
            (b("windowSizesAndPositions_1"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("88"), geom(500)),   // container has its own geom
                (b("m1"), geom(500)),   // active member shares the rect
                (b("m2"), geom(9)),     // stale drifted member
                (b("free"), geom(700)), // unstacked window
            ])])),
            (b("openWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("88"), Value::Bool(true)), (b("m1"), Value::Bool(true)),
                (b("m2"), Value::Bool(true)), (b("free"), Value::Bool(true)),
            ])])),
            (b("stacksWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("m1"), container),           // m1 -> Shared("88")
                (b("m2"), Value::Ref(3)),       // m2 -> Ref -> "88"
                (b("free"), Value::None),       // explicitly unstacked
            ])])),
            (b("preferredIdxInStack3"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (Value::Ref(3), Value::Dict(vec![
                    (b("m2"), Value::Int(1)),
                    (b("m1"), Value::Int(0)),
                ])),
            ])])),
        ]);
        Value::Dict(vec![(b("windows"), windows)])
    }

    #[test]
    fn groups_members_by_container_through_ref_and_shared() {
        let wl = window_layout(&stacked_root());
        assert_eq!(wl.stacks.len(), 1);
        let s = &wl.stacks[0];
        assert_eq!(s.container_id, "88");
        // members ordered by preferred index then id: m1(0), m2(1).
        assert_eq!(s.members, vec!["m1".to_string(), "m2".to_string()]);
    }

    #[test]
    fn per_window_stack_ref_tags_container_and_members() {
        let wl = window_layout(&stacked_root());
        let role = |id: &str| wl.windows.iter().find(|w| w.id == id).unwrap().stack.as_ref();
        assert!(matches!(role("88").map(|r| &r.role), Some(StackRole::Container)));
        assert!(matches!(role("m1").map(|r| &r.role), Some(StackRole::Member)));
        assert_eq!(role("m2").unwrap().container_id, "88");
        assert!(role("free").is_none(), "None value is not a member");
    }

    #[test]
    fn anchor_is_the_container_when_it_has_geom() {
        let wl = window_layout(&stacked_root());
        assert_eq!(wl.stacks[0].anchor_id, "88");
    }

    #[test]
    fn anchor_falls_back_to_frontmost_open_member_when_container_has_no_geom() {
        // Drop the container's own geometry entry: anchor should be m1 (tab 0).
        let mut root = stacked_root();
        if let Value::Dict(top) = &mut root {
            if let Value::Dict(win) = &mut top[0].1 {
                if let Value::Tuple(t) = &mut win[0].1 { // windowSizesAndPositions_1
                    if let Value::Dict(geoms) = &mut t[1] {
                        geoms.retain(|(k, _)| !matches!(k, Value::Bytes(b) if b == b"88"));
                    }
                }
            }
        }
        let wl = window_layout(&root);
        assert_eq!(wl.stacks[0].anchor_id, "m1", "frontmost open member (tab 0)");
    }

    #[test]
    fn colliding_and_missing_indices_still_order_deterministically() {
        // Two members share index 0 and one is absent from the pref dict: order
        // by (index, id) with absent treated as last.
        fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        fn ts() -> Value { Value::Long(vec![0u8; 8]) }
        let windows = Value::Dict(vec![
            (b("stacksWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("zeta"), b("C")), (b("alpha"), b("C")), (b("mid"), b("C")),
            ])])),
            (b("preferredIdxInStack3"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("C"), Value::Dict(vec![
                    (b("zeta"), Value::Int(0)),
                    (b("alpha"), Value::Int(0)), // collides with zeta
                    // "mid" absent -> treated as last
                ])),
            ])])),
        ]);
        let wl = window_layout(&Value::Dict(vec![(b("windows"), windows)]));
        assert_eq!(wl.stacks[0].members, vec!["alpha".to_string(), "zeta".to_string(), "mid".to_string()]);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p settings-model --lib windows`
Expected: FAIL to compile — `Stack`, `StackRole`, `wl.stacks`, `w.stack` do not exist yet.

- [ ] **Step 3: Implement the projection changes**

In `crates/settings-model/src/windows.rs`:

Replace the `StackField` struct (lines ~71-75) with the new types:
```rust
#[derive(Debug, Serialize)]
pub struct Stack {
    pub container_id: String,
    pub container_label: String,
    /// The window id whose geom the stack is drawn at (§ anchor rule).
    pub anchor_id: String,
    /// Member ids in tab order (preferred index, then id; absent = last).
    pub members: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StackRole {
    Container,
    Member,
}

#[derive(Debug, Serialize)]
pub struct StackRef {
    pub container_id: String,
    pub role: StackRole,
}
```

In `WindowRect`, replace `pub stacks: Option<StackField>,` with `pub stack: Option<StackRef>,`.

In `WindowLayout`, add `pub stacks: Vec<Stack>,` (after `windows`).

Remove the `stack_field` fn (lines ~229-237).

In `window_layout`: the per-window loop currently sets `stacks` on each `WindowRect`. Change it to set `stack: None` initially (grouping is computed after the loop). Then after the `reference_resolution` fixup, before returning, compute stacks:

```rust
    // --- stacks -------------------------------------------------------------
    // Resolve stacksWindows (member -> container) and preferredIdxInStack3
    // (container -> {member -> idx}) through Ref/Shared, then group.
    let mut member_container: Vec<(String, String)> = Vec::new();
    if let Some((sw, _)) = timestamped_dict(windows_dict, &windows_path, b"stacksWindows") {
        for (k, v) in sw {
            let member = decode_id(effective(k, &shared));
            match effective(v, &shared) {
                Value::None => {}
                cv => member_container.push((member, decode_id(cv))),
            }
        }
    }
    // pref[container][member] = idx
    let mut pref: std::collections::HashMap<String, std::collections::HashMap<String, i64>> =
        std::collections::HashMap::new();
    if let Some((pd, _)) = timestamped_dict(windows_dict, &windows_path, b"preferredIdxInStack3") {
        for (ck, cv) in pd {
            let container = decode_id(effective(ck, &shared));
            if let Value::Dict(inner) = effective(cv, &shared) {
                let m = pref.entry(container).or_default();
                for (mk, mv) in inner {
                    if let Value::Int(i) = effective(mv, &shared) {
                        m.insert(decode_id(effective(mk, &shared)), *i);
                    }
                }
            }
        }
    }

    // Group members by container, preserving first-seen container order.
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for (member, container) in &member_container {
        if !groups.contains_key(container) {
            order.push(container.clone());
        }
        groups.entry(container.clone()).or_default().push(member.clone());
    }

    let geom_of = |id: &str| windows.iter().find(|w| w.id == id).and_then(|w| w.geom.as_ref());
    let is_open = |id: &str| windows.iter().any(|w| w.id == id && w.open);

    let mut stacks = Vec::new();
    for container in order {
        let mut members = groups.remove(&container).unwrap_or_default();
        let idx = |id: &str| pref.get(&container).and_then(|m| m.get(id)).copied().unwrap_or(i64::MAX);
        members.sort_by(|a, b| idx(a).cmp(&idx(b)).then_with(|| a.cmp(b)));
        // Anchor: container geom if present, else frontmost open member (first in
        // tab order that is open), else the first member.
        let anchor_id = if geom_of(&container).is_some() {
            container.clone()
        } else {
            members.iter().find(|m| is_open(m)).cloned()
                .unwrap_or_else(|| members.first().cloned().unwrap_or_else(|| container.clone()))
        };
        stacks.push(Stack {
            container_label: container.clone(),
            anchor_id,
            members,
            container_id: container,
        });
    }

    // Tag each window's role. A window is a Container if it is some stack's
    // container_id; a Member if it is a member of a stack.
    for w in &mut windows {
        if let Some(s) = stacks.iter().find(|s| s.container_id == w.id) {
            w.stack = Some(StackRef { container_id: s.container_id.clone(), role: StackRole::Container });
        } else if let Some((_, c)) = member_container.iter().find(|(m, _)| *m == w.id) {
            w.stack = Some(StackRef { container_id: c.clone(), role: StackRole::Member });
        }
    }

    WindowLayout { reference_w, reference_h, windows, stacks }
```

(`decode_id` already exists in this file. The final `WindowLayout { ... }` literal replaces the current one — add `stacks`.)

- [ ] **Step 4: Update the empty-return and exports**

Still in `windows.rs`, the early `empty` return at the top of `window_layout` (`WindowLayout { reference_w: 0, reference_h: 0, windows: Vec::new() }`) needs `stacks: Vec::new()` added.

In `crates/settings-model/src/lib.rs`, wherever `WindowLayout`/`WindowRect` are re-exported (grep `WindowLayout`), add `Stack, StackRef, StackRole` to the same `pub use crate::windows::{...}` list.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p settings-model`
Expected: PASS — the new stack tests plus all existing windows/overview/corpus tests green. (The existing `stacks_is_an_editable_value_when_numeric` test referenced the removed `StackField`; delete that test in Step 3.)

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/src/windows.rs crates/settings-model/src/lib.rs
git commit -m "Project window stacks: group members by container with an anchor"
```

---

### Task 2: Authoring — unstack / add / reorder in `stacks.rs`

**Files:**
- Create: `crates/settings-model/src/stacks.rs`
- Modify: `crates/settings-model/src/lib.rs` (`mod stacks;` + exports)

**Interfaces:**
- Consumes: `blue_marshal::Value`, `crate::treewalk::inline_all`.
- Produces (all operate on the whole char-doc `&mut Value`, inline-first):
  - `pub enum StackError { NoWindows, NotStacked(String) }` (serde snake_case tagged)
  - `pub fn unstack(v: &mut Value, member: &str) -> Result<(), StackError>`
  - `pub fn add_to_stack(v: &mut Value, member: &str, container: &str) -> Result<(), StackError>`
  - `pub fn reorder_stack(v: &mut Value, container: &str, members_in_order: &[String]) -> Result<(), StackError>`

- [ ] **Step 1: Write the failing tests**

Create `crates/settings-model/src/stacks.rs` with just the test module first (implementation in Step 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
    fn ts() -> Value { Value::Long(vec![0u8; 8]) }

    // root -> windows -> { stacksWindows, preferredIdxInStack3 }, with a Shared
    // container id to prove inline-first (RemoveEntry would refuse it raw).
    fn root() -> Value {
        let container = Value::Shared { slot: 3, value: Box::new(b("C")) };
        Value::Dict(vec![(b("windows"), Value::Dict(vec![
            (b("stacksWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("m1"), container),
                (b("m2"), Value::Ref(3)),
            ])])),
            (b("preferredIdxInStack3"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("C"), Value::Dict(vec![
                    (b("m1"), Value::Int(0)), (b("m2"), Value::Int(1)),
                ])),
            ])])),
        ]))])
    }

    // Read helpers: navigate the (inlined) tree to the two dicts.
    fn win<'a>(v: &'a Value) -> &'a Vec<(Value, Value)> {
        let Value::Dict(top) = v else { panic!() };
        let (_, w) = top.iter().find(|(k, _)| matches!(k, Value::Bytes(x) if x == b"windows")).unwrap();
        let Value::Dict(d) = w else { panic!() };
        d
    }
    fn inner<'a>(win: &'a [(Value, Value)], name: &[u8]) -> &'a Vec<(Value, Value)> {
        let (_, v) = win.iter().find(|(k, _)| matches!(k, Value::Bytes(x) if x == name)).unwrap();
        match v { Value::Tuple(t) => { let Value::Dict(d) = &t[1] else { panic!() }; d }, Value::Dict(d) => d, _ => panic!() }
    }
    fn sw(v: &Value) -> &Vec<(Value, Value)> { inner(win(v), b"stacksWindows") }
    fn pref(v: &Value) -> &Vec<(Value, Value)> { inner(win(v), b"preferredIdxInStack3") }
    fn keys(d: &[(Value, Value)]) -> Vec<String> {
        d.iter().map(|(k, _)| match k { Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(), _ => String::new() }).collect()
    }

    #[test]
    fn unstack_removes_the_member_from_both_dicts() {
        let mut v = root();
        unstack(&mut v, "m1").unwrap();
        assert_eq!(keys(sw(&v)), vec!["m2".to_string()]);
        // preferredIdxInStack3[C] no longer lists m1.
        let (_, cdict) = pref(&v).iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == b"C")).unwrap();
        let Value::Dict(inner) = cdict else { panic!() };
        assert_eq!(keys(inner), vec!["m2".to_string()]);
    }

    #[test]
    fn add_inserts_into_both_dicts_with_next_index() {
        let mut v = root();
        add_to_stack(&mut v, "m3", "C").unwrap();
        assert!(keys(sw(&v)).contains(&"m3".to_string()));
        let (_, cdict) = pref(&v).iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == b"C")).unwrap();
        let Value::Dict(inner) = cdict else { panic!() };
        // m3 gets the next index (2) after m1(0), m2(1).
        let (_, idx) = inner.iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == b"m3")).unwrap();
        assert_eq!(*idx, Value::Int(2));
    }

    #[test]
    fn reorder_rewrites_indices_to_clean_0_n() {
        let mut v = root();
        reorder_stack(&mut v, "C", &["m2".into(), "m1".into()]).unwrap();
        let (_, cdict) = pref(&v).iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == b"C")).unwrap();
        let Value::Dict(inner) = cdict else { panic!() };
        let idx = |id: &[u8]| { let (_, v) = inner.iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == id)).unwrap(); v.clone() };
        assert_eq!(idx(b"m2"), Value::Int(0));
        assert_eq!(idx(b"m1"), Value::Int(1));
    }

    #[test]
    fn unstack_a_missing_member_errors() {
        let mut v = root();
        assert!(matches!(unstack(&mut v, "nope"), Err(StackError::NotStacked(_))));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p settings-model --lib stacks`
Expected: FAIL to compile — `unstack`/`add_to_stack`/`reorder_stack`/`StackError` undefined.

- [ ] **Step 3: Implement the mutators**

Prepend to `crates/settings-model/src/stacks.rs` (above the test module):

```rust
//! Structural authoring for window stacks: edit `stacksWindows` and
//! `preferredIdxInStack3` under `windows`. The window-id keys/values are
//! `Shared` stores, which `mutate::apply`'s `RemoveEntry` refuses, so every
//! entry point inlines the whole tree first (drops all sharing) and edits plain
//! values; the app layer reshares before saving. Mirrors overview.rs/autofill.rs.

use blue_marshal::Value;
use serde::Serialize;

use crate::treewalk::inline_all;

#[derive(Debug, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum StackError {
    /// No `windows` dict in the file.
    NoWindows,
    /// The named member is not present in `stacksWindows`.
    NotStacked(String),
}

fn is_b(k: &Value, name: &[u8]) -> bool { matches!(k, Value::Bytes(b) if b.as_slice() == name) }

/// Mutable `windows` dict (the file is already inlined, so no Shared wrapper).
fn windows_mut(v: &mut Value) -> Result<&mut Vec<(Value, Value)>, StackError> {
    let Value::Dict(top) = v else { return Err(StackError::NoWindows) };
    let (_, w) = top.iter_mut().find(|(k, _)| is_b(k, b"windows")).ok_or(StackError::NoWindows)?;
    match w { Value::Dict(d) => Ok(d), _ => Err(StackError::NoWindows) }
}

/// The inner dict under a `windows` child, unwrapping the `(timestamp, dict)`
/// tuple. Creates a bare dict entry if the child is absent.
fn child_inner<'a>(win: &'a mut Vec<(Value, Value)>, name: &[u8]) -> &'a mut Vec<(Value, Value)> {
    if !win.iter().any(|(k, _)| is_b(k, name)) {
        win.push((Value::Bytes(name.to_vec()), Value::Dict(Vec::new())));
    }
    let (_, v) = win.iter_mut().find(|(k, _)| is_b(k, name)).unwrap();
    match v {
        Value::Dict(d) => d,
        Value::Tuple(t) => {
            let slot = t.iter_mut().find(|e| matches!(e, Value::Dict(_)));
            match slot { Some(Value::Dict(d)) => d, _ => unreachable!("timestamped dict has a Dict") }
        }
        other => { *other = Value::Dict(Vec::new()); let Value::Dict(d) = other else { unreachable!() }; d }
    }
}

pub fn unstack(v: &mut Value, member: &str) -> Result<(), StackError> {
    inline_all(v);
    let win = windows_mut(v)?;
    let mb = member.as_bytes();
    let sw = child_inner(win, b"stacksWindows");
    let before = sw.len();
    sw.retain(|(k, _)| !is_b(k, mb));
    if sw.len() == before {
        return Err(StackError::NotStacked(member.to_string()));
    }
    // Remove the member from every preferredIdxInStack3[container] dict.
    let pref = child_inner(win, b"preferredIdxInStack3");
    for (_, inner) in pref.iter_mut() {
        if let Value::Dict(d) = inner {
            d.retain(|(k, _)| !is_b(k, mb));
        }
    }
    Ok(())
}

pub fn add_to_stack(v: &mut Value, member: &str, container: &str) -> Result<(), StackError> {
    inline_all(v);
    let win = windows_mut(v)?;
    let (mb, cb) = (member.as_bytes(), container.as_bytes());
    let sw = child_inner(win, b"stacksWindows");
    sw.retain(|(k, _)| !is_b(k, mb)); // re-stack cleanly if already present
    sw.push((Value::Bytes(mb.to_vec()), Value::Bytes(cb.to_vec())));

    let pref = child_inner(win, b"preferredIdxInStack3");
    let cdict = container_dict(pref, cb);
    cdict.retain(|(k, _)| !is_b(k, mb));
    let next = cdict.iter().filter_map(|(_, v)| if let Value::Int(i) = v { Some(*i) } else { None }).max().map(|m| m + 1).unwrap_or(0);
    cdict.push((Value::Bytes(mb.to_vec()), Value::Int(next)));
    Ok(())
}

pub fn reorder_stack(v: &mut Value, container: &str, members_in_order: &[String]) -> Result<(), StackError> {
    inline_all(v);
    let win = windows_mut(v)?;
    let cb = container.as_bytes();
    let pref = child_inner(win, b"preferredIdxInStack3");
    let cdict = container_dict(pref, cb);
    *cdict = members_in_order.iter().enumerate()
        .map(|(i, m)| (Value::Bytes(m.as_bytes().to_vec()), Value::Int(i as i64)))
        .collect();
    Ok(())
}

/// The `preferredIdxInStack3[container]` inner dict, created if absent.
fn container_dict<'a>(pref: &'a mut Vec<(Value, Value)>, cb: &[u8]) -> &'a mut Vec<(Value, Value)> {
    if !pref.iter().any(|(k, _)| is_b(k, cb)) {
        pref.push((Value::Bytes(cb.to_vec()), Value::Dict(Vec::new())));
    }
    let (_, v) = pref.iter_mut().find(|(k, _)| is_b(k, cb)).unwrap();
    match v { Value::Dict(d) => d, other => { *other = Value::Dict(Vec::new()); let Value::Dict(d) = other else { unreachable!() }; d } }
}
```

In `crates/settings-model/src/lib.rs`: add `mod stacks;` and `pub use crate::stacks::{unstack, add_to_stack, reorder_stack, create_stack, StackError};` (create_stack lands in Task 3 — add it to this export line now only if Task 3 is done together; otherwise export the three here and add create_stack in Task 3).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p settings-model --lib stacks`
Expected: PASS — all four tests green.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/stacks.rs crates/settings-model/src/lib.rs
git commit -m "Add stack membership editing: unstack, add, reorder"
```

---

### Task 3: Authoring — create a stack from two free windows

**Files:**
- Modify: `crates/settings-model/src/stacks.rs` (add `create_stack` + tests)
- Modify: `crates/settings-model/src/lib.rs` (export `create_stack` if not already)

**Interfaces:**
- Produces: `pub fn create_stack(v: &mut Value, member1: &str, member2: &str) -> Result<String, StackError>` — mints and returns the new container id `C`; materializes it per the format-notes recipe.

- [ ] **Step 1: Write the failing tests**

Add to `stacks.rs` test module (extend `root()` isn't enough — build a root with geometry + flag dicts). Append:

```rust
    fn free_windows_root() -> Value {
        // Two free windows m1 (rect x=10) and m2 (rect x=99), plus the flag dicts.
        fn geom(x: i64) -> Value {
            Value::Tuple(vec![Value::Int(x), Value::Int(0), Value::Int(100), Value::Int(80), Value::Int(2560), Value::Int(1440)])
        }
        let boolset = |ids: &[&str], val: bool| Value::Tuple(vec![ts(), Value::Dict(
            ids.iter().map(|i| (b(i), Value::Bool(val))).collect())]);
        Value::Dict(vec![(b("windows"), Value::Dict(vec![
            (b("windowSizesAndPositions_1"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("m1"), geom(10)), (b("m2"), geom(99)), (b("40"), geom(0)),
            ])])),
            (b("openWindows"), boolset(&["m1", "m2"], false)),
            (b("isLightBackgroundWindows"), boolset(&[], false)),
            (b("isOverlayedWindows"), boolset(&[], false)),
            (b("minimizedWindows"), boolset(&[], false)),
            (b("stacksWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![])])),
            (b("preferredIdxInStack3"), Value::Tuple(vec![ts(), Value::Dict(vec![])])),
        ]))])
    }

    fn geom_of(v: &Value, id: &[u8]) -> Vec<i64> {
        let g = inner(win(v), b"windowSizesAndPositions_1");
        let (_, t) = g.iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == id)).unwrap();
        let Value::Tuple(t) = t else { panic!() };
        t.iter().map(|e| if let Value::Int(i) = e { *i } else { 0 }).collect()
    }
    fn boolval(v: &Value, dict: &[u8], id: &[u8]) -> Option<bool> {
        let d = inner(win(v), dict);
        d.iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == id)).and_then(|(_, v)| if let Value::Bool(x) = v { Some(*x) } else { None })
    }

    #[test]
    fn create_mints_a_free_high_id_and_lands_at_m1_rect() {
        let mut v = free_windows_root();
        let c = create_stack(&mut v, "m1", "m2").unwrap();
        // "40" and "m1"/"m2" already exist; the minted id must be free (not "40").
        assert_ne!(c, "40");
        assert!(c.parse::<i64>().is_ok(), "container id is a numeric string");
        // Container + both members share M1's rect (x = 10).
        assert_eq!(geom_of(&v, c.as_bytes())[0], 10);
        assert_eq!(geom_of(&v, b"m2")[0], 10, "m2 moved to m1's rect");
        assert_eq!(geom_of(&v, b"m1")[0], 10);
    }

    #[test]
    fn create_links_members_opens_and_flags_the_container() {
        let mut v = free_windows_root();
        let c = create_stack(&mut v, "m1", "m2").unwrap();
        let cb = c.as_bytes();
        // stacksWindows: both members -> C.
        let get = |id: &[u8]| sw(&v).iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == id)).map(|(_, v)| v.clone());
        assert_eq!(get(b"m1"), Some(Value::Bytes(cb.to_vec())));
        assert_eq!(get(b"m2"), Some(Value::Bytes(cb.to_vec())));
        // preferredIdxInStack3[C] = {m1:0, m2:1}.
        let (_, cd) = pref(&v).iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == cb)).unwrap();
        let Value::Dict(cd) = cd else { panic!() };
        assert_eq!(keys(cd), vec!["m1".to_string(), "m2".to_string()]);
        // Open: C, m1, m2 all true.
        assert_eq!(boolval(&v, b"openWindows", cb), Some(true));
        assert_eq!(boolval(&v, b"openWindows", b"m1"), Some(true));
        // Container marked in the three state dicts (False).
        assert_eq!(boolval(&v, b"isOverlayedWindows", cb), Some(false));
        assert_eq!(boolval(&v, b"minimizedWindows", cb), Some(false));
        assert_eq!(boolval(&v, b"isLightBackgroundWindows", cb), Some(false));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p settings-model --lib stacks`
Expected: FAIL to compile — `create_stack` undefined.

- [ ] **Step 3: Implement `create_stack`**

Add to `stacks.rs`:

```rust
/// Create a new stack from two free windows. `member1` is the window the action
/// started from; the stack lands at its current rect. Returns the minted
/// container id. See docs/format-notes.md ("Window stacks") for the recipe.
pub fn create_stack(v: &mut Value, member1: &str, member2: &str) -> Result<String, StackError> {
    inline_all(v);
    let container = mint_free_id(windows_mut(v)?);
    let (cb, m1b, m2b) = (container.as_bytes().to_vec(), member1.as_bytes(), member2.as_bytes());

    // Geometry: C and M2 take M1's current rect.
    let win = windows_mut(v)?;
    let geoms = child_inner(win, b"windowSizesAndPositions_1");
    let m1_rect = geoms.iter().find(|(k, _)| is_b(k, m1b)).map(|(_, r)| r.clone());
    if let Some(rect) = m1_rect {
        set_entry(geoms, &cb, rect.clone());
        set_entry(geoms, m2b, rect);
    }
    // Membership.
    let sw = child_inner(win, b"stacksWindows");
    set_entry(sw, m1b, Value::Bytes(cb.clone()));
    set_entry(sw, m2b, Value::Bytes(cb.clone()));
    let pref = child_inner(win, b"preferredIdxInStack3");
    let cdict = container_dict(pref, &cb);
    *cdict = vec![
        (Value::Bytes(m1b.to_vec()), Value::Int(0)),
        (Value::Bytes(m2b.to_vec()), Value::Int(1)),
    ];
    // Open C + both members; mark C in the three state dicts.
    for (dict, val) in [(b"openWindows".as_slice(), true)] {
        let d = child_inner(win, dict);
        set_entry(d, &cb, Value::Bool(val));
        set_entry(d, m1b, Value::Bool(val));
        set_entry(d, m2b, Value::Bool(val));
    }
    for dict in [b"isLightBackgroundWindows".as_slice(), b"isOverlayedWindows", b"minimizedWindows"] {
        let d = child_inner(win, dict);
        set_entry(d, &cb, Value::Bool(false));
    }
    Ok(container)
}

/// Lowest free integer id, at least 1000, that is not already used as a key or
/// container value anywhere in the window dicts — a high value avoids colliding
/// with EVE's own low counter (spec §7).
fn mint_free_id(win: &[(Value, Value)]) -> String {
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (_, v) in win {
        collect_ids(v, &mut used);
    }
    let mut n: i64 = 1000;
    while used.contains(&n.to_string()) { n += 1; }
    n.to_string()
}

fn collect_ids(v: &Value, out: &mut std::collections::HashSet<String>) {
    match v {
        Value::Bytes(b) => { out.insert(String::from_utf8_lossy(b).into_owned()); }
        Value::Tuple(t) => t.iter().for_each(|e| collect_ids(e, out)),
        Value::Dict(d) => d.iter().for_each(|(k, val)| { collect_ids(k, out); collect_ids(val, out); }),
        _ => {}
    }
}

/// Insert or overwrite a byte-keyed dict entry.
fn set_entry(d: &mut Vec<(Value, Value)>, key: &[u8], val: Value) {
    if let Some(slot) = d.iter_mut().find(|(k, _)| is_b(k, key)) {
        slot.1 = val;
    } else {
        d.push((Value::Bytes(key.to_vec()), val));
    }
}
```

Ensure `create_stack, StackError` are in the `lib.rs` `pub use crate::stacks::{...}` list.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p settings-model`
Expected: PASS — the two create tests plus everything from Tasks 1-2 and the existing suite.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/stacks.rs crates/settings-model/src/lib.rs
git commit -m "Add stack creation from two free windows"
```

---

### Task 4: App wiring — ops command + Tauri commands + `api.ts`, retire the dead stack-id field

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (`edit_char_stacks` wrapper + 4 thin fns)
- Modify: `app/src-tauri/src/lib.rs` (4 `#[tauri::command]` + register in `generate_handler!`)
- Modify: `app/src/lib/api.ts` (add `Stack`/`StackRef` types; `WindowLayout.stacks`, `WindowRect.stack`; remove `StackField` + `stacks`; add 4 command wrappers)
- Modify: `app/src/lib/LayoutView.svelte` (drop the now-removed `onStack` prop + `onStack` handler)
- Modify: `app/src/lib/WindowPanel.svelte` (drop the dead stack-id number input + `onStack` prop)

**Interfaces:**
- Consumes: `settings_model::{unstack, add_to_stack, reorder_stack, create_stack, StackError, WindowLayout}`.
- Produces (ops.rs, all re-project and return `WindowLayout`):
  - `pub fn stack_unstack(state, member: &str) -> Result<WindowLayout, ErrDto>`
  - `pub fn stack_add(state, member: &str, container: &str) -> Result<WindowLayout, ErrDto>`
  - `pub fn stack_reorder(state, container: &str, members: Vec<String>) -> Result<WindowLayout, ErrDto>`
  - `pub fn stack_create(state, member1: &str, member2: &str) -> Result<WindowLayout, ErrDto>`
- Tauri commands: `stack_unstack`, `stack_add`, `stack_reorder`, `stack_create` (all `slot`-less — the char slot is implicit).

- [ ] **Step 1: Write the failing test (ops round-trip)**

Add to `app/src-tauri/src/ops.rs` `#[cfg(test)] mod tests`. Reuse the `window_layout_reads_the_open_document` shape but with stack dicts:

```rust
    fn stacked_char_bytes() -> Vec<u8> {
        fn bb(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        fn ts() -> Value { Value::Long(vec![0u8; 8]) }
        fn geom(x: i64) -> Value { Value::Tuple(vec![Value::Int(x), Value::Int(0), Value::Int(100), Value::Int(80), Value::Int(2560), Value::Int(1440)]) }
        encode(&Value::Dict(vec![(bb("windows"), Value::Dict(vec![
            (bb("windowSizesAndPositions_1"), Value::Tuple(vec![ts(), Value::Dict(vec![(bb("m1"), geom(0)), (bb("m2"), geom(0)), (bb("C"), geom(0))])])),
            (bb("openWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![(bb("m1"), Value::Bool(true)), (bb("m2"), Value::Bool(true)), (bb("C"), Value::Bool(true))])])),
            (bb("stacksWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![(bb("m1"), bb("C")), (bb("m2"), bb("C"))])])),
            (bb("preferredIdxInStack3"), Value::Tuple(vec![ts(), Value::Dict(vec![(bb("C"), Value::Dict(vec![(bb("m1"), Value::Int(0)), (bb("m2"), Value::Int(1))]))])])),
        ]))])).unwrap()
    }

    #[test]
    fn unstack_reprojects_and_reshares() {
        let path = temp_file("stack-unstack", &stacked_char_bytes());
        let state = AppState::new();
        open_file(&state, Slot::Char, path.to_str().unwrap()).unwrap();
        let wl = stack_unstack(&state, "m1").unwrap();
        // The stack now has only m2 (m1 unstacked).
        assert_eq!(wl.stacks.len(), 1);
        assert_eq!(wl.stacks[0].members, vec!["m2".to_string()]);
        // Doc still encodes/decodes (reshare ran without corrupting the tree).
        let guard = state.char.lock().unwrap();
        let bytes = blue_marshal::encode(&guard.as_ref().unwrap().value).unwrap();
        assert_eq!(blue_marshal::decode(&bytes).unwrap(), guard.as_ref().unwrap().value);
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p app --lib stack`
Expected: FAIL to compile — `stack_unstack` undefined.

- [ ] **Step 3: Implement the ops wrapper + fns**

In `app/src-tauri/src/ops.rs`, extend the `settings_model` import with `unstack, add_to_stack, reorder_stack, create_stack, StackError`. Add after `clear_all_autofill`:

```rust
/// Edit the CHAR slot's window stacks, reshare, then re-project the layout.
fn edit_char_stacks<F>(state: &AppState, edit: F) -> Result<WindowLayout, ErrDto>
where
    F: FnOnce(&mut blue_marshal::Value) -> Result<(), StackError>,
{
    {
        let mut guard = state.char.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no character file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        edit(&mut doc.value).map_err(|e| ErrDto::new("stack", format!("{e:?}")))?;
        doc.value = blue_marshal::reshare(&doc.value);
    }
    window_layout(state, Slot::Char)
}

pub fn stack_unstack(state: &AppState, member: &str) -> Result<WindowLayout, ErrDto> {
    edit_char_stacks(state, |v| unstack(v, member))
}
pub fn stack_add(state: &AppState, member: &str, container: &str) -> Result<WindowLayout, ErrDto> {
    edit_char_stacks(state, |v| add_to_stack(v, member, container))
}
pub fn stack_reorder(state: &AppState, container: &str, members: Vec<String>) -> Result<WindowLayout, ErrDto> {
    edit_char_stacks(state, |v| reorder_stack(v, container, &members))
}
pub fn stack_create(state: &AppState, member1: &str, member2: &str) -> Result<WindowLayout, ErrDto> {
    // create_stack returns the id; discard it here (the re-projection carries it).
    edit_char_stacks(state, |v| create_stack(v, member1, member2).map(|_| ()))
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p app --lib stack`
Expected: PASS.

- [ ] **Step 5: Register the Tauri commands**

In `app/src-tauri/src/lib.rs`, add four wrappers (mirroring `window_layout`'s shape — `state: tauri::State<'_, AppState>`, no `slot` arg):

```rust
#[tauri::command]
fn stack_unstack(state: tauri::State<'_, AppState>, member: String) -> Result<settings_model::WindowLayout, ErrDto> {
    ops::stack_unstack(&state, &member)
}
#[tauri::command]
fn stack_add(state: tauri::State<'_, AppState>, member: String, container: String) -> Result<settings_model::WindowLayout, ErrDto> {
    ops::stack_add(&state, &member, &container)
}
#[tauri::command]
fn stack_reorder(state: tauri::State<'_, AppState>, container: String, members: Vec<String>) -> Result<settings_model::WindowLayout, ErrDto> {
    ops::stack_reorder(&state, &container, members)
}
#[tauri::command]
fn stack_create(state: tauri::State<'_, AppState>, member1: String, member2: String) -> Result<settings_model::WindowLayout, ErrDto> {
    ops::stack_create(&state, &member1, &member2)
}
```

Add all four names to the `tauri::generate_handler![...]` list (grep `generate_handler` in lib.rs; append `, stack_unstack, stack_add, stack_reorder, stack_create`).

- [ ] **Step 6: Update `api.ts`**

In `app/src/lib/api.ts`:
- Replace the `StackField` interface with:
```ts
export type StackRole = "container" | "member";
export interface StackRef {
  container_id: string;
  role: StackRole;
}
export interface Stack {
  container_id: string;
  container_label: string;
  anchor_id: string;
  members: string[];
}
```
- In `WindowRect`, replace `stacks: StackField | null;` with `stack: StackRef | null;`.
- In `WindowLayout`, add `stacks: Stack[];`.
- Add to the `api` object:
```ts
  stackUnstack: (member: string) => invoke<WindowLayout>("stack_unstack", { member }),
  stackAdd: (member: string, container: string) => invoke<WindowLayout>("stack_add", { member, container }),
  stackReorder: (container: string, members: string[]) => invoke<WindowLayout>("stack_reorder", { container, members }),
  stackCreate: (member1: string, member2: string) => invoke<WindowLayout>("stack_create", { member1, member2 }),
```

- [ ] **Step 7: Remove the dead stack-id field from the frontend**

In `app/src/lib/WindowPanel.svelte`: remove the `onStack` prop (from the `$props()` type and destructure), and remove the entire `{#if w.stacks}` … stack-id `<label class="stack">` … `{/if}` block (lines ~118-127) plus the `.stack` CSS rule.

In `app/src/lib/LayoutView.svelte`: remove the `onStack` const (`const onStack = ...`) and the `{onStack}` prop passed to `<WindowPanel>`.

- [ ] **Step 8: Verify the frontend still type-checks**

Run (from `app/`, PowerShell): `npm run check`
Expected: 0 errors (the removed `stacks`/`onStack` references are gone; the new `stack`/`stacks` fields are declared).

- [ ] **Step 9: Commit**

```bash
git add crates/settings-model app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts app/src/lib/WindowPanel.svelte app/src/lib/LayoutView.svelte
git commit -m "Wire stack membership commands end-to-end and retire the dead stack-id field"
```

---

### Task 5: Frontend grouping helper (`layout.ts`)

**Files:**
- Modify: `app/src/lib/layout.ts` (add `stackUnits`)
- Modify: `app/src/lib/layout.test.ts` (append checks)

**Interfaces:**
- Consumes: `WindowRect`, `Stack` from `./api`.
- Produces: `export function stackUnits(layout: WindowLayout): DrawUnit[]` where a `DrawUnit` is either a single non-stacked window or a stack (anchor rect + open members in tab order). Exact shape:
```ts
export interface DrawUnit {
  key: string;              // window id (single) or container_id (stack)
  anchor: WindowRect;       // the rect to draw at
  stack: Stack | null;      // null for a plain window
  tabs: WindowRect[];       // [anchor] for a plain window; open members in tab order for a stack
}
```

- [ ] **Step 1: Write the failing tests**

Append to `app/src/lib/layout.test.ts` (extend the `import` from `./layout.ts` to include `stackUnits`, and reuse/extend the `win()` helper — note the helper must now set `stack` and geom). Add:

```ts
// --- stackUnits: group open windows into draw units -------------------------
{
  const g = (x: number) => ({ x, y: 0, w: 100, h: 80, screen_w: 2560, screen_h: 1440,
    x_path: [], y_path: [], w_path: [], h_path: [], screen_w_path: [], screen_h_path: [] });
  const mk = (id: string, open: boolean, stack: any): WindowRect => ({
    id, label: id, open, renderable: true, resolution_matches: true, geom: g(open ? 10 : 0),
    flags: [], stack,
  });
  const layout = {
    reference_w: 2560, reference_h: 1440,
    stacks: [{ container_id: "C", container_label: "C", anchor_id: "C", members: ["m1", "m2"] }],
    windows: [
      mk("C", true, { container_id: "C", role: "container" }),
      mk("m1", true, { container_id: "C", role: "member" }),
      mk("m2", false, { container_id: "C", role: "member" }), // closed member: excluded from tabs
      mk("free", true, null),
    ],
  };
  const units = stackUnits(layout as any);
  check("stackUnits produces one stack + one free unit", units.length === 2);
  const stackUnit = units.find((u) => u.stack)!;
  check("stack unit anchors on the container", stackUnit.anchor.id === "C");
  check("stack tabs are open members only, in tab order", stackUnit.tabs.map((t) => t.id).join(",") === "m1");
  const freeUnit = units.find((u) => !u.stack)!;
  check("free window is its own unit", freeUnit.key === "free" && freeUnit.tabs.length === 1);
}
```

- [ ] **Step 2: Run to verify it fails**

Run (from `app/`, PowerShell): `npm test`
Expected: FAIL — `stackUnits` is not exported.

- [ ] **Step 3: Implement `stackUnits`**

Append to `app/src/lib/layout.ts`:
```ts
import type { WindowLayout, Stack } from "./api";

export interface DrawUnit {
  key: string;
  anchor: WindowRect;
  stack: Stack | null;
  tabs: WindowRect[];
}

/**
 * Group the open, renderable windows into draw units: one per stack (drawn at
 * the stack's anchor, with its open members as tabs in preferred order) and one
 * per non-stacked window. A stack whose anchor is not open/renderable is dropped
 * (nothing to draw).
 */
export function stackUnits(layout: WindowLayout): DrawUnit[] {
  const drawn = openWindows(layout.windows);
  const byId = new Map(drawn.map((w) => [w.id, w]));
  const units: DrawUnit[] = [];
  const claimed = new Set<string>();

  for (const s of layout.stacks) {
    const anchor = byId.get(s.anchor_id);
    if (!anchor) continue; // anchor not open/renderable — skip the stack
    const tabs = s.members.map((id) => byId.get(id)).filter((w): w is WindowRect => !!w);
    // The container itself is not a tab unless it is also a member.
    for (const w of tabs) claimed.add(w.id);
    claimed.add(s.container_id);
    units.push({ key: s.container_id, anchor, stack: s, tabs });
  }
  for (const w of drawn) {
    if (claimed.has(w.id)) continue;
    units.push({ key: w.id, anchor: w, stack: null, tabs: [w] });
  }
  return units;
}
```

- [ ] **Step 4: Run to verify it passes**

Run (from `app/`, PowerShell): `npm test`
Expected: PASS — all suites green.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/layout.ts app/src/lib/layout.test.ts
git commit -m "Group open windows into stack draw units"
```

---

### Task 6: Frontend canvas — draw stacks, select tabs, coherent move/resize (`LayoutView.svelte`)

**Files:**
- Modify: `app/src/lib/LayoutView.svelte`

**Interfaces:**
- Consumes: `stackUnits`, `DrawUnit` (Task 5); `resizeRect` (already imported for four-corner resize); `api.stack*` (Task 4).
- Produces: no exported interface; DOM. Verified by `npm run check` + Task 8 smoke.

- [ ] **Step 1: Draw one rect per unit with tabs**

Replace the `{#each drawn as w (w.id)}` block with iteration over `stackUnits(layout)`. The `drawn` derived becomes `const units = $derived(stackUnits(layout ?? { reference_w: 0, reference_h: 0, windows: [], stacks: [] }));`. Each unit draws at `unit.anchor`'s rect (via `rectOf(unit.anchor)`), and when `unit.stack` is set, renders a tab strip of `unit.tabs` names along the top; a plain unit renders as today. Selection: clicking the rect selects `unit.anchor.id` (or, for a stack, the anchor's id); clicking a tab selects that member's id (`selectedId = tab.id`). Concretely:

```svelte
{#each units as unit (unit.key)}
  {@const r = rectOf(unit.anchor)}
  <div
    class="win"
    class:selected={unit.tabs.some((t) => t.id === selectedId) || unit.anchor.id === selectedId}
    class:stacked={!!unit.stack}
    style="left: {toCanvas(r.x, scale)}px; top: {toCanvas(r.y, scale)}px;
           width: {toCanvas(r.w, scale)}px; height: {toCanvas(r.h, scale)}px;"
    onpointerdown={(e) => startMove(unit, e)}>
    {#if unit.stack}
      <div class="tabs">
        {#each unit.tabs as tab (tab.id)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <span class="tab" class:active={tab.id === selectedId}
            onpointerdown={(e) => { e.stopPropagation(); selectedId = tab.id; }}>{tab.label}</span>
        {/each}
      </div>
    {:else}
      <span class="win-label">{unit.anchor.label}</span>
    {/if}
    {#if unit.anchor.id === selectedId || unit.tabs.some((t) => t.id === selectedId)}
      {#each (["tl", "tr", "bl", "br"] as const) as c}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span class="resize {c}" onpointerdown={(e) => startResize(unit, c, e)}></span>
      {/each}
    {/if}
  </div>
{/each}
```

- [ ] **Step 2: Make move/resize operate on the whole unit (coherent fan-out)**

Change the `Drag` type's `w: WindowRect` to `unit: DrawUnit` for both variants; `startMove(unit, e)` / `startResize(unit, corner, e)` record `unit.anchor`'s origin. In `onPointerUp`, build the geometry mutations for **every window in the unit** — the anchor and, for a stack, all open members (`unit.tabs`) plus the container if it is renderable — applying the same delta/new-rect. Replace the single `commit(geomMutations(w, ...))` with a loop:

```ts
async function onPointerUp() {
  if (!drag) return;
  const p = preview[drag.unit.anchor.id];
  const d = drag;
  drag = null;
  if (!p) return;
  // Fan the new anchor rect out to every renderable window in the unit so a
  // stack moves/resizes coherently and stale members are repaired.
  const targets = unitWindows(d.unit);
  const next = d.kind === "move" ? { x: p.x, y: p.y } : { x: p.x, y: p.y, w: p.w, h: p.h };
  const ms = targets.flatMap((w) => geomMutations(w, next));
  await commit(ms);
  clearPreview(d.unit.anchor.id);
}
```
where `unitWindows(unit)` returns `[unit.anchor, ...members-and-container-that-are-renderable]` de-duplicated by id (a helper reading from `layout.windows`). The `preview` continues to key on `unit.anchor.id`, and `rectOf(unit.anchor)` shows the live drag on the whole rect.

- [ ] **Step 3: Type-check**

Run (from `app/`, PowerShell): `npm run check`
Expected: 0 errors.

- [ ] **Step 4: Commit**

```bash
git add app/src/lib/LayoutView.svelte
git commit -m "Draw stacks as one tabbed rectangle with coherent move and resize"
```

---

### Task 7: Frontend panel — grouped rows + membership controls (`WindowPanel.svelte`)

**Files:**
- Modify: `app/src/lib/WindowPanel.svelte`
- Modify: `app/src/lib/LayoutView.svelte` (pass stack callbacks + `stacks` to the panel)

**Interfaces:**
- Consumes: `WindowLayout.stacks`, `api.stack*` (Task 4).
- Produces: DOM; verified by `npm run check` + Task 8 smoke.

- [ ] **Step 1: Add stack callbacks in `LayoutView.svelte`**

Add handlers that call the api and refresh the layout, mirroring `commit`:
```ts
async function runStack(p: Promise<WindowLayout>) {
  try { layout = await p; } catch (e) { await message(errMessage(e), { title: "Stack edit failed", kind: "error" }); }
}
const onUnstack = (id: string) => runStack(api.stackUnstack(id));
const onReorder = (container: string, members: string[]) => runStack(api.stackReorder(container, members));
const onAddToStack = (member: string, container: string) => runStack(api.stackAdd(member, container));
const onCreateStack = (m1: string, m2: string) => runStack(api.stackCreate(m1, m2));
```
Pass `stacks={layout.stacks}`, `{onUnstack}`, `{onReorder}`, `{onAddToStack}`, `{onCreateStack}` to `<WindowPanel>`.

- [ ] **Step 2: Grouped rows + controls in `WindowPanel.svelte`**

Add the new props to `$props()`. Render, above the flat window rows: for each stack in `stacks`, a group header (`container_label` + `members.length`) and its members (looked up in `windows` by id) as ordered sub-rows, each with **unstack** and **reorder up/down** buttons (up calls `onReorder(container, membersWithSwap)`, disabled at the ends). Then render the non-stacked windows (those whose `stack` is null) as today, each with an **add to stack ▾** control: a `<select>` of existing `stacks` container_labels (calls `onAddToStack(w.id, container)`) plus, to create, a `<select>` of the other free windows to **stack with** (calls `onCreateStack(w.id, other.id)`). Native `<select>`/`<option>` must carry explicit dark bg/color (see [[eve-editor-dark-native-controls]] — the existing `.detail input` dark styling is the reference). Keep the existing per-window detail (geom/flags) unchanged.

- [ ] **Step 3: Type-check**

Run (from `app/`, PowerShell): `npm run check`
Expected: 0 errors.

- [ ] **Step 4: Commit**

```bash
git add app/src/lib/WindowPanel.svelte app/src/lib/LayoutView.svelte
git commit -m "Group the window panel by stack with membership controls"
```

---

### Task 8: Whole-workspace verify + live smoke

**Files:** none (verification only).

- [ ] **Step 1: Full test suite + type check**

Run: `cargo test --workspace` (Bash) — expected: exit 0, all crates green (settings-model stacks/windows, app ops, blue-marshal corpus gates).
Run (from `app/`, PowerShell): `npm test` — expected: all `node --test` suites green. `npm run check` — expected: 0 errors.

- [ ] **Step 2: Live smoke (project norm — user runs the app)**

Launch `npm run tauri dev`. On a real char file in the Layout view:
- Confirm an open stack draws as **one** tabbed rectangle at the anchor, tabs = open members; clicking a tab selects that member; moving/resizing the rect moves the whole stack (stale members snap to the anchor).
- **Unstack** a member; **reorder** tabs; **add** a free window to an existing stack. Save, reopen Editable, confirm in-game.
- **Create** a stack from two free windows. Save, reopen Editable, confirm in-game.
- **§7 counter check:** after creating a stack (which minted a high id like `1000`), tab another pair of windows together in-game and confirm EVE assigns an id that does **not** collide with the minted one, and that no window state is corrupted. Record the result in `docs/format-notes.md` "Window stacks".

- [ ] **Step 3: Commit any smoke fixes**

Commit fixes discovered during the smoke with sentence-case messages; note the §7 counter-check result in the ledger.

---

## Self-Review

**Spec coverage:**
- §3 projection (Stack/StackRef, resolve both dicts through Ref/Shared, anchor rule, delete StackField) → Task 1. ✓
- §4 membership authoring (unstack/add/reorder/create on inlined subtree, reshare) → Tasks 2, 3 (settings-model) + Task 4 (ops reshare wrapper). ✓
- §4 geometry coherent move/resize (SetScalar fan-out to container + open members) → Task 6 Step 2. ✓
- §5 canvas (one tabbed rect per stack, tab select, stale positions not drawn) → Tasks 5, 6. ✓
- §6 panel (grouped rows, unstack/reorder/add/create, remove dead stack-id input) → Tasks 4 Step 7 (removal) + 7 (controls). ✓
- §7 creation (mint high free id, land at M1 rect, recipe, counter check in smoke) → Task 3 + Task 8 Step 2. ✓
- §8 testing (Rust projection + authoring unit tests, frontend grouping test, DOM-not-unit-tested, live smoke with counter check) → Tasks 1-5 tests + Task 8. ✓

**Placeholder scan:** No TBD/TODO. Rust tasks carry full code; the two Svelte DOM tasks (6, 7) give concrete markup/handlers and are verified by `npm run check` + the live smoke, consistent with the project's DOM-not-unit-tested norm.

**Type consistency:** `Stack`/`StackRef`/`StackRole` identical across Rust (Task 1) and `api.ts` (Task 4). `stackUnits`/`DrawUnit` defined in Task 5 and consumed in Task 6. The `edit_char_stacks` closure signature (`FnOnce(&mut Value) -> Result<(), StackError>`) matches the `stacks.rs` mutator signatures from Tasks 2-3 (create is adapted via `.map(|_| ())` in Task 4 Step 3). Command names (`stack_unstack`/`stack_add`/`stack_reorder`/`stack_create`) match across ops.rs, lib.rs, and `api.ts`.
