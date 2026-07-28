# Windowless Account Is A Normal State Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop the editor presenting a windowless overview account as damage, and give the user an explicit, warned way to opt into per-window tab management — without ever fabricating a window mapping behind their back.

**Architecture:** One new structural function that writes a single-window `tabsByWindowInstanceID` listing every tab, reachable only from a deliberate user action behind a confirm. The existing no-fabricate policy stays exactly as it is: nothing else in the codebase creates that key implicitly, and two functions that could still do so accidentally are fixed. The refusal messages are reworded to say "this account does not use per-window tabs", never anything that reads as corruption.

**Tech Stack:** Rust (`settings-model`), Tauri commands, Svelte 5 (runes), TypeScript, `cargo test`.

## Global Constraints

- **No new dependencies**, Rust or frontend.
- **Never fabricate `tabsByWindowInstanceID` implicitly.** An absent or empty mapping is a valid state that EVE itself produces; a partial or empty one **hides the account's entire overview**. The only code path allowed to create it is the explicit user action built in Task 3, and it always writes a complete mapping covering every tab.
- **Structural edits inline first, and the app layer reshares.** Every entry point in `overview_tabs.rs` starts with `inline_all(v)`; `edit_user_tabs` in `ops.rs` calls `blue_marshal::reshare` afterwards.
- **Wrapped container shape.** Real files store every container key as `(timestamp, payload)` — 4,187 of 4,187 across five untouched accounts. `groups_mut` already writes that shape; anything new must too.
- **No message may imply the file is damaged.** This state is produced by EVE's own overview importer.
- Run the Rust suite with `cargo test` from the repo root, the frontend suite with `npm test` from `app/`.
- Commit after each task.

---

## Background: why this is not a bug in the file

**Confirmed 2026-07-28: EVE's own overview importer deletes `tabsByWindowInstanceID`.** Account A carried the key through every offline staging session and lost it in the capture taken straight after a pack was imported through the client's own Overview Settings. The account has had `tabsettings_new` with no window mapping ever since, and its overview works in-game regardless.

So **any user who imports an overview pack the normal way and then opens our Overview editor is in this state.** It is not rare and it is not damage.

What the editor does today:

| Path | Behaviour |
|---|---|
| `overview.rs::window_groups` | Returns an empty vec — correct |
| `overview_tabs.rs::create_tab` | Adds the tab to `tabsettings_new`, leaves the mapping alone — correct, and verified in-game |
| `overview_tabs.rs::add_overview_window` | Refuses with `NoWindowMapping`: *"This overview has no window layout to add to."* — reads as damage |
| `OverviewView.svelte::startAddWindow` | Returns silently when `data.windows.length === 0` — the button does nothing at all |

### The design decision, and why not the obvious one

The ledger entry suggests "rebuild a single-window mapping from the tabs that exist, since that is evidently what the client does". **That over-reads the evidence.** What Session B proved is that the client *deletes* the mapping and the overview keeps working — not that it rebuilds one. `create_tab`'s own comment states the consequence:

> absent/empty means EVE distributes tabs across its (char-side) overview windows by default. We must NOT create or touch the mapping in that case — the per-window distribution is char-side state we can't reconstruct here.

Fabricating a single-window mapping therefore **pins every tab into one window** for a player whose overview spans several — a silent layout change that cannot be verified offline and cannot be undone once saved.

**Decision (confirmed with the developer 2026-07-28): opt-in, never silent.** The default stays no-fabricate and the wording gets fixed; a separate, explicit "Set up per-window tabs" action writes the mapping, behind a confirm that says plainly what it will do.

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `crates/settings-model/src/overview_tabs.rs` | Structural authoring for overview tabs/windows | Modify: `create_window_mapping`, two error variants, reworded messages, no-fabricate fixes |
| `crates/settings-model/src/lib.rs` | Crate re-exports | Modify: export the new function |
| `app/src-tauri/src/ops.rs` | Command implementations | Modify: `overview_create_window_mapping` |
| `app/src-tauri/src/lib.rs` | Tauri command registration | Modify: declare + register |
| `app/src/lib/api.ts` | Typed IPC surface | Modify: add the binding |
| `app/src/lib/OverviewView.svelte` | Overview editor UI | Modify: the windowless notice and the offer |
| `docs/small-tasks.md` | The ledger | Modify: close the entry and item (b) of the follow-ups entry |
| `docs/format-notes.md` | Format reference | Modify: record that the importer deletes the key |

---

### Task 1: Stop two functions fabricating the mapping on failure

**Files:**
- Modify: `crates/settings-model/src/overview_tabs.rs:308-335` (`reorder_tabs_in_window`, `move_tab`)

**Interfaces:**
- Consumes: `window_count(ov)` (already private in this file), `OverviewTabError::NoWindowMapping`.
- Produces: nothing other tasks depend on — but Task 2 relies on the invariant this establishes, that a *failed* op never leaves the key behind.

This is item (b) of the "Overview windowless-account + no-fabricate follow-ups" ledger entry, and it must land before the mapping becomes creatable: `groups_mut` **creates** `tabsByWindowInstanceID` if it is absent. Both functions call it before validating, so on a windowless account they return `UnknownWindow` *and leave an empty mapping behind* — and an empty mapping hides the entire overview. A failed edit must not mutate the file.

- [ ] **Step 1: Write the failing test**

Add to the `mod tests` block at the bottom of `crates/settings-model/src/overview_tabs.rs`. Look at the existing fixtures in that module first and reuse whichever builds an overview with tabs; the assertion below only needs one whose `tabsByWindowInstanceID` is absent. If no such fixture exists, build it inline in the shape the module's other fixtures use.

```rust
    /// An overview with tabs but no window mapping — the state EVE's own pack
    /// importer leaves behind (verified 2026-07-28).
    fn windowless_root() -> Value {
        let tab = Value::Dict(vec![
            (Value::Str("name".into()), Value::StrUcs2("Default".into())),
            (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
        ]);
        Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()), Value::Tuple(vec![
                Value::Long(vec![0u8; 8]),
                Value::Dict(vec![(Value::Int(0), tab)]),
            ])),
        ]))])
    }

    fn has_mapping(v: &Value) -> bool {
        let Value::Dict(top) = v else { return false };
        let Some((_, ov)) = top.iter().find(|(k, _)| is_b(k, b"overview")) else { return false };
        let Value::Dict(entries) = ov else { return false };
        entries.iter().any(|(k, _)| is_b(k, b"tabsByWindowInstanceID"))
    }

    #[test]
    fn reorder_on_a_windowless_account_refuses_without_fabricating() {
        let mut v = windowless_root();
        assert!(matches!(
            reorder_tabs_in_window(&mut v, 0, &[0]),
            Err(OverviewTabError::NoWindowMapping),
        ));
        assert!(!has_mapping(&v), "a refused reorder must not create the mapping");
    }

    #[test]
    fn move_on_a_windowless_account_refuses_without_fabricating() {
        let mut v = windowless_root();
        assert!(matches!(
            move_tab(&mut v, 0, 0, 0, 0),
            Err(OverviewTabError::NoWindowMapping),
        ));
        assert!(!has_mapping(&v), "a refused move must not create the mapping");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run from the repo root: `cargo test -p settings-model overview_tabs::`
Expected: FAIL — both return `UnknownWindow`, not `NoWindowMapping`, and `has_mapping` is true because `groups_mut` fabricated it.

- [ ] **Step 3: Write the implementation**

In `crates/settings-model/src/overview_tabs.rs`, add the guard as the first thing after `overview_mut` in both functions. `reorder_tabs_in_window`:

```rust
pub fn reorder_tabs_in_window(v: &mut Value, window_idx: usize, order: &[i64]) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    // `groups_mut` CREATES the mapping when it is absent, so this guard has to
    // come first: on a windowless account the call below would refuse the edit
    // and still leave an empty `tabsByWindowInstanceID` behind — which hides the
    // account's whole overview. A refused edit must not touch the file.
    if window_count(ov) == 0 {
        return Err(OverviewTabError::NoWindowMapping);
    }
    let inner = groups_mut(ov).get_mut(window_idx).and_then(list_inner_mut)
        .ok_or(OverviewTabError::UnknownWindow { index: window_idx })?;
    *inner = order.iter().map(|&i| Value::Int(i)).collect();
    Ok(())
}
```

And `move_tab`, with the same guard directly after its `overview_mut`:

```rust
    if window_count(ov) == 0 {
        return Err(OverviewTabError::NoWindowMapping);
    }
```

placed **above** the existing "Validate the destination window exists BEFORE mutating the source strip" block, leaving that block unchanged.

- [ ] **Step 4: Run the tests to verify they pass**

Run from the repo root: `cargo test -p settings-model overview_tabs::`
Expected: PASS, with every pre-existing test in the module still passing.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/overview_tabs.rs
git commit -m "Refuse a windowless reorder or move without fabricating a mapping"
```

---

### Task 2: Say what a windowless account is, not that it is broken

**Files:**
- Modify: `crates/settings-model/src/overview_tabs.rs:26-27` (the variant doc), `:48` (the message), `:337-342` (`add_overview_window`'s doc comment)

**Interfaces:**
- Consumes: nothing new.
- Produces: nothing other tasks depend on. The `NoWindowMapping` *code* is unchanged — only its text — so any frontend branching on the code keeps working.

- [ ] **Step 1: Write the failing test**

Add to the `mod tests` block in `crates/settings-model/src/overview_tabs.rs`:

```rust
    #[test]
    fn the_windowless_message_does_not_read_as_damage() {
        let msg = OverviewTabError::NoWindowMapping.to_string();
        // The state is one EVE's own importer produces, so the wording has to
        // describe a configuration, never a fault. "no ... to add to" read as a
        // missing piece of the file.
        assert!(msg.contains("per-window"), "message should name the feature: {msg}");
        for bad in ["no window layout", "missing", "damaged", "corrupt", "invalid"] {
            assert!(!msg.to_lowercase().contains(bad), "message still reads as damage ({bad}): {msg}");
        }
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run from the repo root: `cargo test -p settings-model the_windowless_message`
Expected: FAIL — the current text is "This overview has no window layout to add to."

- [ ] **Step 3: Write the implementation**

In `crates/settings-model/src/overview_tabs.rs`, replace the `NoWindowMapping` message on line 48:

```rust
            OverviewTabError::NoWindowMapping => write!(f, "This account does not use per-window tabs, so there are no windows to change. EVE removes the tab-to-window mapping whenever an overview pack is imported through the client, and the overview works normally without it."),
```

Replace the variant's doc comment (lines 26-27):

```rust
    /// Refused: this account has no tab-to-window mapping. NOT damage — EVE's
    /// own overview importer deletes `tabsByWindowInstanceID` (confirmed
    /// 2026-07-28) and the client distributes tabs across its char-side windows
    /// by default. `create_window_mapping` is the deliberate way out.
    NoWindowMapping,
```

And rewrite `add_overview_window`'s doc comment (lines 337-342) so it stops describing the refusal as a damage guard:

```rust
/// Add a new overview window (user-file grouping half). Appends an empty inner
/// list to `tabsByWindowInstanceID` and seeds it with one cloned tab (a window
/// must have ≥1 tab).
///
/// Refuses on an account with no mapping at all. That is not a damaged file: EVE
/// deletes the key on every pack import, and distributes tabs across its
/// char-side windows by default. Adding positionally there would fabricate a
/// PARTIAL mapping listing only the new tab, which hides every other tab the
/// account has. `create_window_mapping` writes a complete one instead, and is
/// the only path allowed to create the key.
///
/// Returns the new window's index, always ≥1 here (an account with no mapping is
/// refused with `NoWindowMapping`), so the char key is `overview_{idx}`.
```

- [ ] **Step 4: Run the test to verify it passes**

Run from the repo root: `cargo test -p settings-model overview_tabs::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/overview_tabs.rs
git commit -m "Word the windowless refusal as a configuration, not a fault"
```

---

### Task 3: Write a complete mapping, on purpose

**Files:**
- Modify: `crates/settings-model/src/overview_tabs.rs` (new function + two error variants)
- Modify: `crates/settings-model/src/lib.rs:42-46` (the re-export list)

**Interfaces:**
- Consumes: `overview_mut`, `tabs_mut`, `groups_mut`, `window_count`, `as_int`, `inline_all` — all already in `overview_tabs.rs`.
- Produces: `pub fn create_window_mapping(v: &mut Value) -> Result<usize, OverviewTabError>` — returns the number of tabs mapped. New error variants `OverviewTabError::WindowMappingExists` and `OverviewTabError::NoTabsToMap`. Task 4 calls this; Task 5 renders against the count.

- [ ] **Step 1: Write the failing tests**

Add to the `mod tests` block in `crates/settings-model/src/overview_tabs.rs`, reusing `windowless_root` and `has_mapping` from Task 1:

```rust
    /// The mapping's single inner list, as plain ints.
    fn mapped_tabs(v: &Value) -> Vec<i64> {
        let Value::Dict(top) = v else { panic!() };
        let (_, ov) = top.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(entries) = ov else { panic!() };
        let (_, wv) = entries.iter().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")).unwrap();
        let groups = list_inner(wv).unwrap();
        assert_eq!(groups.len(), 1, "exactly one window is created");
        list_inner(&groups[0]).unwrap().iter().filter_map(as_int).collect()
    }

    #[test]
    fn create_mapping_lists_every_tab_in_one_window() {
        let mut v = windowless_root();
        // Give it a second and third tab so "every tab, in index order" has
        // something to prove — a one-tab account cannot distinguish the cases.
        {
            let ov = overview_mut(&mut v).unwrap();
            let tabs = tabs_mut(ov);
            let clone = tabs[0].1.clone();
            tabs.push((Value::Int(5), clone.clone()));
            tabs.push((Value::Int(2), clone));
        }
        assert_eq!(create_window_mapping(&mut v).unwrap(), 3);
        // Ascending index order, and NOT dict order (0, 5, 2 as inserted).
        assert_eq!(mapped_tabs(&v), vec![0, 2, 5]);
    }

    #[test]
    fn create_mapping_refuses_when_one_already_exists() {
        let mut v = windowless_root();
        create_window_mapping(&mut v).unwrap();
        assert!(matches!(
            create_window_mapping(&mut v),
            Err(OverviewTabError::WindowMappingExists),
        ));
    }

    #[test]
    fn create_mapping_refuses_an_overview_with_no_tabs() {
        // A mapping whose only window lists no tabs would hide everything, and a
        // zero-tab overview is a state that exists in the wild.
        let mut v = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()), Value::Tuple(vec![
                Value::Long(vec![0u8; 8]),
                Value::Dict(Vec::new()),
            ])),
        ]))]);
        assert!(matches!(
            create_window_mapping(&mut v),
            Err(OverviewTabError::NoTabsToMap),
        ));
        assert!(!has_mapping(&v), "a refused create must not leave a partial mapping");
    }

    #[test]
    fn create_mapping_leaves_a_tree_that_still_encodes() {
        let mut v = windowless_root();
        create_window_mapping(&mut v).unwrap();
        let bytes = blue_marshal::encode(&v).expect("edited tree still encodes");
        assert_eq!(blue_marshal::decode(&bytes).unwrap(), v);
    }

    #[test]
    fn add_window_works_once_a_mapping_exists() {
        // The whole point of the opt-in: add_overview_window refused before, and
        // must succeed afterwards.
        let mut v = windowless_root();
        assert!(matches!(add_overview_window(&mut v, "Second", None), Err(OverviewTabError::NoWindowMapping)));
        create_window_mapping(&mut v).unwrap();
        assert_eq!(add_overview_window(&mut v, "Second", None).unwrap(), 1);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run from the repo root: `cargo test -p settings-model overview_tabs::`
Expected: FAIL to compile — `cannot find function create_window_mapping`.

- [ ] **Step 3: Add the error variants**

In `crates/settings-model/src/overview_tabs.rs`, add to the `OverviewTabError` enum (after `NoWindowMapping`):

```rust
    /// Refused: this account already maps tabs to windows.
    WindowMappingExists,
    /// Refused: there are no tabs to map, and a mapping whose window lists no
    /// tabs hides the entire overview.
    NoTabsToMap,
```

And to the `Display` impl:

```rust
            OverviewTabError::WindowMappingExists => write!(f, "This account already uses per-window tabs."),
            OverviewTabError::NoTabsToMap => write!(f, "There are no overview tabs to map to a window."),
```

- [ ] **Step 4: Write the implementation**

Add to `crates/settings-model/src/overview_tabs.rs`, immediately before `add_overview_window`:

```rust
/// Give an account an explicit tab-to-window mapping: one window listing every
/// tab it has, in ascending tab index.
///
/// This is the ONLY path in the codebase allowed to create
/// `tabsByWindowInstanceID`, and it exists because the absent state is normal —
/// EVE's own overview importer deletes the key (confirmed 2026-07-28) and the
/// client then distributes tabs across its char-side windows by default. That
/// default is char-side state this crate cannot read, so writing a mapping
/// REPLACES it: every tab is pinned into one window until the user rearranges
/// them. Destructive enough that it must never be implicit — the UI puts it
/// behind a confirm that says so, and nothing else calls it.
///
/// Completeness is the safety property. A mapping that omits a tab hides that
/// tab, and one that omits all of them hides the whole overview, so this either
/// lists every tab or refuses.
pub fn create_window_mapping(v: &mut Value) -> Result<usize, OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    if window_count(ov) > 0 {
        return Err(OverviewTabError::WindowMappingExists);
    }
    // Ascending index, not dict order: the tab strip is rendered in index order
    // and a mapping in insertion order would silently reshuffle it.
    let mut indices: Vec<i64> = tabs_mut(ov).iter().filter_map(|(k, _)| as_int(k)).collect();
    indices.sort_unstable();
    if indices.is_empty() {
        return Err(OverviewTabError::NoTabsToMap);
    }
    // groups_mut creates the key in the wrapped `(timestamp, list)` shape every
    // real file uses. Only reached once the refusals above have passed, so it
    // can never leave a partial mapping behind.
    let groups = groups_mut(ov);
    groups.push(Value::List(indices.iter().map(|&i| Value::Int(i)).collect()));
    Ok(indices.len())
}
```

- [ ] **Step 5: Export it**

In `crates/settings-model/src/lib.rs`, add to the `pub use overview_tabs::{...}` block (lines 42-46):

```rust
pub use overview_tabs::{
    add_overview_window, add_overview_window_geometry, create_tab, create_window_mapping,
    delete_tab, move_tab, remove_overview_window, remove_overview_window_geometry, rename_tab,
    reorder_tabs_in_window, set_tab_preset, OverviewTabError,
};
```

- [ ] **Step 6: Run the tests to verify they pass**

Run from the repo root: `cargo test`
Expected: PASS, whole workspace.

- [ ] **Step 7: Commit**

```bash
git add crates/settings-model/src/overview_tabs.rs crates/settings-model/src/lib.rs
git commit -m "Let an account opt into per-window tabs with a complete mapping"
```

---

### Task 4: Expose it as a command

**Files:**
- Modify: `app/src-tauri/src/ops.rs:22-23` (the `use` list) and beside `tab_create` (around `:959`)
- Modify: `app/src-tauri/src/lib.rs` (declare beside the other overview commands, and add to the handler list)
- Modify: `app/src/lib/api.ts` (beside the other overview window bindings)
- Test: `app/src-tauri/src/ops.rs` (the `mod tests` block)

**Interfaces:**
- Consumes: `create_window_mapping` from Task 3; `edit_user_tabs` (already private in `ops.rs`).
- Produces: `pub fn overview_create_window_mapping(state: &AppState) -> Result<OverviewColumns, ErrDto>`, reachable as the Tauri command `overview_create_window_mapping` taking no arguments; and `api.overviewCreateWindowMapping()`. Task 5 calls the api binding.

Note this edits the **user** slot only — unlike `overview_window_add`, which also writes char-side geometry. Window 0's geometry key (`overview`) already exists on any account that has an overview at all, so there is nothing to create char-side, and the frontend marks only the user document dirty.

- [ ] **Step 1: Write the failing test**

Add to the `mod tests` block in `app/src-tauri/src/ops.rs`, following the style of the existing overview tab tests in that module (find one that opens a user file with `open_file(&state, Slot::User, …)` and reuse its fixture builder if one fits; otherwise build the bytes inline as below):

```rust
    fn windowless_user_bytes() -> Vec<u8> {
        use blue_marshal::{encode, Value};
        fn bb(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        fn ts() -> Value { Value::Long(vec![0u8; 8]) }
        let tab = Value::Dict(vec![
            (Value::Str("name".into()), Value::StrUcs2("Default".into())),
            (bb("overview"), bb("P")),
        ]);
        encode(&Value::Dict(vec![(bb("overview"), Value::Dict(vec![
            (bb("tabsettings_new"), Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab)])])),
        ]))])).unwrap()
    }

    #[test]
    fn creating_a_window_mapping_projects_one_window_holding_every_tab() {
        let path = temp_file("ov-windowless", &windowless_user_bytes());
        let state = AppState::new();
        open_file(&state, Slot::User, path.to_str().unwrap()).unwrap();

        // Before: the account projects no windows at all.
        assert!(overview_columns(&state).unwrap().windows.is_empty());

        let cols = overview_create_window_mapping(&state).unwrap();
        assert_eq!(cols.windows.len(), 1);
        assert_eq!(cols.windows[0].tab_indices, vec![0]);

        // Doc still encodes/decodes (reshare ran without corrupting the tree).
        let guard = state.user.lock().unwrap();
        let bytes = blue_marshal::encode(&guard.as_ref().unwrap().value).unwrap();
        assert_eq!(blue_marshal::decode(&bytes).unwrap(), guard.as_ref().unwrap().value);
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run from the repo root: `cargo test -p app_lib window_mapping`
Expected: FAIL to compile — `cannot find function overview_create_window_mapping`.

- [ ] **Step 3: Write the implementation**

In `app/src-tauri/src/ops.rs`, add `create_window_mapping` to the `settings_model` import on line 22:

```rust
    create_tab, create_window_mapping, rename_tab, delete_tab, reorder_tabs_in_window, move_tab, set_tab_preset, OverviewTabError,
```

And add beside `tab_create`:

```rust
/// Give the account an explicit tab-to-window mapping. User slot only: window
/// 0's char-side geometry key already exists on any account with an overview.
pub fn overview_create_window_mapping(state: &AppState) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| create_window_mapping(v).map(|_| ()))
}
```

- [ ] **Step 4: Register the command and add the binding**

In `app/src-tauri/src/lib.rs`, beside the other overview window commands:

```rust
#[tauri::command]
fn overview_create_window_mapping(state: tauri::State<'_, AppState>) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::overview_create_window_mapping(&state)
}
```

Add `overview_create_window_mapping` to the `tauri::generate_handler![…]` list.

In `app/src/lib/api.ts`, beside the other overview window bindings:

```ts
  overviewCreateWindowMapping: () => invoke<OverviewColumns>("overview_create_window_mapping"),
```

- [ ] **Step 5: Run the tests to verify they pass**

Run from the repo root: `cargo test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Expose the per-window tab opt-in as a command"
```

---

### Task 5: Explain it in the overview editor

**Files:**
- Modify: `app/src/lib/OverviewView.svelte:66-69` (`startAddWindow`) and the `.ov-controls` markup around `:240`

**Interfaces:**
- Consumes: `api.overviewCreateWindowMapping` from Task 4; `confirm` and `message` from `@tauri-apps/plugin-dialog` (already imported in this file); `onUserDirty` (already a prop).
- Produces: nothing other tasks depend on.

Today `startAddWindow` returns silently when there are no windows, so the button does nothing and says nothing. Replace that dead end with an explanation and the opt-in.

- [ ] **Step 1: Add the action**

In `app/src/lib/OverviewView.svelte`, add beside `startAddWindow`:

```ts
  // A windowless account is normal: EVE's own overview importer deletes the
  // tab-to-window mapping, so anyone who has imported a pack lands here. Writing
  // one REPLACES the client's default distribution and pins every tab into a
  // single window, so it is offered rather than done — and the confirm says so.
  async function setUpWindowMapping() {
    if (!data || data.windows.length > 0) return;
    const n = data.tabs.length;
    const ok = await confirm(
      `Put all ${n} tab${n === 1 ? "" : "s"} in one overview window?\n\n` +
        `This account currently lets EVE decide which of your overview windows each tab ` +
        `appears in. Setting this up replaces that with an explicit list, so every tab ` +
        `starts in one window and you arrange them from there. EVE removes this list again ` +
        `whenever you import an overview pack through the client.`,
      { title: "Set up per-window tabs", kind: "warning" },
    );
    if (!ok) return;
    try {
      data = await api.overviewCreateWindowMapping();
      onUserDirty();
    } catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
```

- [ ] **Step 2: Render the notice**

In the `.ov-controls` block, beside the existing window controls, add:

```svelte
      {#if data.windows.length === 0}
        <div class="no-windows">
          <span>
            Tabs aren't assigned to specific overview windows on this account — EVE spreads them
            across your windows itself. That's normal: importing an overview pack through the
            client removes the assignment.
          </span>
          <button onclick={setUpWindowMapping}>Set up per-window tabs</button>
        </div>
      {/if}
```

And to the component's `<style>` block:

```css
  .no-windows {
    flex-basis: 100%;
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    padding: 0.35rem 0.4rem;
    font-size: 0.85em;
    color: var(--fg-dim);
    border: 1px solid var(--border);
    border-radius: 3px;
  }
  .no-windows button { flex: none; }
```

If the existing "Add window" button is rendered unconditionally, wrap it in `{#if data.windows.length > 0}` so a windowless account sees the notice instead of a button that cannot work.

- [ ] **Step 3: Run the full suite and type-check**

Run from `app/`: `npm test` then `npm run check`
Expected: PASS, 0 type errors. (Four pre-existing `state_referenced_locally` warnings in `ContextMenu.svelte`, `InsertForm.svelte` and `TreeNode.svelte` are unrelated; the count must not grow.)

- [ ] **Step 4: Commit**

```bash
git add app/src/lib/OverviewView.svelte
git commit -m "Explain a windowless account, and offer the opt-in"
```

---

### Task 6: Close the ledger and record the finding

**Files:**
- Modify: `docs/small-tasks.md` (the windowless entry, and item (b) of the follow-ups entry)
- Modify: `docs/format-notes.md`

**Interfaces:** none — documentation only.

- [ ] **Step 1: Move the entry to Shipped**

Delete the `- [ ] **A windowless account is a normal state, and the editor treats it as an error.**` entry from **Open** and add under `### Unreleased (on master)`:

```markdown
- [x] **A windowless account is a normal state, and the editor treats it as an
  error.** Reworded throughout, and given a way out that does not lie. The
  entry's suggested fix — rebuild a single-window mapping "since that is
  evidently what the client does" — was not taken, because it over-reads the
  evidence: Session B proved the client *deletes* the mapping and keeps working,
  not that it rebuilds one. An absent mapping means EVE distributes tabs across
  its char-side windows, which this crate cannot read, so fabricating one pins
  every tab into a single window and silently flattens a multi-window overview.
  Instead `create_window_mapping` writes a COMPLETE mapping (every tab, ascending
  index) and is reachable only from an explicit "Set up per-window tabs" action
  behind a confirm that says what it replaces. `NoWindowMapping` now describes a
  configuration rather than a fault, and `reorder_tabs_in_window` / `move_tab` no
  longer fabricate an empty mapping on a refused edit — which was item (b) of the
  windowless/no-fabricate follow-ups entry, closed with this. _Added 2026-07-28;
  done 2026-07-28._
```

- [ ] **Step 2: Strike item (b) from the follow-ups entry**

In the still-open "Overview windowless-account + no-fabricate follow-ups (tab-fix branch review)" entry, replace item (b)'s text with a done note, leaving (a) and (c) untouched:

```markdown
  (b) ~~Align `reorder_tabs_in_window` / `move_tab` to the no-fabricate read
  pattern~~ — **done 2026-07-28**: both now refuse with `NoWindowMapping` before
  reaching `groups_mut`, so a refused edit no longer leaves an empty mapping
  behind.
```

- [ ] **Step 3: Record the finding in the format notes**

In `docs/format-notes.md`, in the overview/tabs section, add:

```markdown
**EVE's own overview importer deletes `tabsByWindowInstanceID`.** Confirmed
2026-07-28: account A carried the key through every offline staging session and
lost it in the capture taken straight after a pack was imported through the
client's own Overview Settings. It has had `tabsettings_new` with no window
mapping ever since, and its overview works in-game regardless. So an account
with no tab-to-window mapping is a normal, common state — not a damaged file —
and anything that writes the key must write a COMPLETE mapping: one that omits a
tab hides that tab, and one that omits all of them hides the whole overview.
```

- [ ] **Step 4: Commit**

```bash
git add docs/small-tasks.md docs/format-notes.md
git commit -m "Close the windowless-account task, and record what the importer does"
```

---

## Self-review notes

- **Coverage.** The ledger asked for a decision on what the editor should do (Task 3, opt-in rather than the suggested auto-rebuild, with the reasoning recorded in Task 6) and for the refusal to be reworded (Task 2). Item (b) of the sibling follow-ups entry is folded in as Task 1 because it must land first — the invariant "a refused edit never creates the mapping" is what makes the new creator the only writer.
- **Ordering.** Task 1 before Task 3: with `reorder`/`move` still fabricating, "only `create_window_mapping` creates the key" would be false the moment a user dragged a tab. Tasks 4-5 need Task 3. Task 6 last.
- **Naming.** `create_window_mapping` (Rust) / `overview_create_window_mapping` (op + command) / `overviewCreateWindowMapping` (api) / `setUpWindowMapping` (UI handler). User-facing copy says "per-window tabs" everywhere and never "mapping", which is a file-format word.
- **Why the count is returned.** `create_window_mapping` returns the number of tabs mapped even though `ops.rs` discards it. It is what makes the function's completeness property testable in one assertion, and a caller that wants to report "12 tabs placed" needs no second projection pass.
- **Not in scope, deliberately.** Items (a) and (c) of the follow-ups entry (per-window placement needing the char-side capture; orphan-tab create placement), removing a mapping again, and any attempt to reconstruct EVE's default distribution from the char file. The last is the thing that cannot be done offline and is precisely why this is opt-in.
