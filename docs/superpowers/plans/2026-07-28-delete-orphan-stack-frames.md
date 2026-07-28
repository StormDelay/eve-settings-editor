# Delete Orphaned Stack Frames Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One action that removes every orphaned stack frame from the open character file, so the eight dead frames a real file accumulates stop painting phantom "Window stack" rectangles and stop cluttering the window list.

**Architecture:** One new structural function in `stacks.rs` that finds the orphans *itself* and deletes each id from the geometry dict, the eight boolean flag dicts and both stack dicts. The frontend never sends a list of ids to delete — it only asks "delete the orphans", and the backend re-derives which those are. The UI is a banner in the window panel plus a confirm dialog, reusing the existing `edit_char_stacks` → reshare → re-project plumbing that every other stack op goes through.

**Tech Stack:** Rust (`settings-model`, `blue_marshal`), Tauri commands, Svelte 5 (runes), TypeScript, `node --test` for pure logic (`*.test.ts`), `cargo test` for Rust.

## Global Constraints

- **No new dependencies**, Rust or frontend.
- **The backend re-derives orphanhood; it never accepts an id list from the frontend.** This is a destructive write to a settings file — the trust boundary is the Tauri command, and a caller-supplied list of window ids to delete would cross it unchecked.
- **Structural edits inline first, and the app layer reshares.** Every entry point in `stacks.rs` calls `inline_all(v)` before touching anything, because `mutate::apply`'s `RemoveEntry` refuses `Shared` stores. `edit_char_stacks` in `ops.rs` calls `blue_marshal::reshare` afterwards. Follow that pattern exactly; a new function that skips `inline_all` produces a tree that fails to encode.
- **Never fabricate an absent dict.** A file with no `lockedWindows` must still have no `lockedWindows` after the delete. `child_inner` CREATES the child if missing — so guard with a presence check before calling it, the way `adopt_container_rect` already does.
- Run the Rust suite with `cargo test` from the repo root, the frontend suite with `npm test` from `app/`.
- Commit after each task.

---

## Background: what an orphan frame is, and why deleting is safe

EVE mints a numeric-string window id **only** to serve as a window-stack container (see `docs/format-notes.md`, "Window stacks"). So a numeric id that is the container of no stack is a dead frame: its members are all gone, but its geometry and flag entries remain and the canvas paints an empty rectangle where the stack used to be.

They accumulate through ordinary use — unstacking a pair creates one. A real live character file (A1) carried **eight**: `43`, `51`, `63`, `82`, `156`, `181`, `219`, `221`.

**Confirmed in-game 2026-07-28: EVE does not re-create them.** Two orphan frames (`43`, `51`) were deleted from a real character file, the client was run through a full login/logout, and both stayed gone while six untouched controls sat still. The earlier live plan had this the other way round — its item 12 was written to *close* this task if the client restored the frames — so the confirmation is what makes the task worth building at all.

Each frame is 5-6 entries spread across `isLightBackgroundWindows`, `isOverlayedWindows`, `minimizedWindows`, `openWindows`, `windowSizesAndPositions_1` and sometimes `lockedWindows`. That spread is exactly why this is one offer rather than hand-deletion in the tree editor.

Today the editor only *hides* them, and only when `Hide clutter` is on (`layout.ts:313`).

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `crates/settings-model/src/windows.rs` | Window projection; owns `BOOL_FLAGS` | Modify: make `BOOL_FLAGS` `pub(crate)` so there is one list, not two |
| `crates/settings-model/src/stacks.rs` | Structural authoring for stacks | Modify: add `delete_orphan_frames` + tests |
| `app/src-tauri/src/ops.rs` | Command implementations | Modify: add `stack_delete_orphans` |
| `app/src-tauri/src/lib.rs` | Tauri command registration | Modify: declare + register the command |
| `app/src/lib/api.ts` | Typed IPC surface | Modify: add `stackDeleteOrphans` |
| `app/src/lib/layout.ts` | Filter predicates, furniture | Modify: extract `isOrphanFrame`, reuse it in `windowMatches` |
| `app/src/lib/layout.test.ts` | Pure logic tests | Modify: add `isOrphanFrame` cases |
| `app/src/lib/WindowPanel.svelte` | Window list UI | Modify: the offer banner + `onDeleteOrphans` prop |
| `app/src/lib/LayoutView.svelte` | Owns layout state, calls the api | Modify: confirm dialog + `runStack` call |
| `docs/small-tasks.md` | The ledger | Modify: close the two duplicate entries |
| `docs/format-notes.md` | Format reference | Modify: record that EVE does not re-create deleted frames |

---

### Task 1: Find and delete orphan frames in the model

**Files:**
- Modify: `crates/settings-model/src/windows.rs:22` (`BOOL_FLAGS` visibility)
- Modify: `crates/settings-model/src/stacks.rs` (new function + tests)

**Interfaces:**
- Consumes: `inline_all` from `crate::treewalk`, `windows_mut` / `child_inner` / `is_b` already private in `stacks.rs`, `crate::windows::BOOL_FLAGS`.
- Produces: `pub fn delete_orphan_frames(v: &mut Value) -> Vec<String>` — returns the ids it deleted, in the order found. Empty vec when there are none (not an error: "nothing to clean" is a normal outcome, and `ops.rs` re-projects either way).

- [ ] **Step 1: Write the failing tests**

Add to the `mod tests` block at the bottom of `crates/settings-model/src/stacks.rs`. Note the existing helpers `b`, `ts`, `win`, `inner`, `keys` are already defined in that module — reuse them rather than redefining.

```rust
    /// A file with one live stack (container "C", members m1/m2) and two orphan
    /// frames: "43" carries geometry + four flags, "51" only geometry. Also a
    /// non-numeric id "market" with no stack, which must survive — the orphan
    /// rule is about MINTED numeric ids, and a normal window is never one.
    fn orphans_root() -> Value {
        fn geom(x: i64) -> Value {
            Value::Tuple(vec![Value::Int(x), Value::Int(0), Value::Int(100), Value::Int(80), Value::Int(2560), Value::Int(1440)])
        }
        let boolset = |ids: &[&str]| Value::Tuple(vec![ts(), Value::Dict(
            ids.iter().map(|i| (b(i), Value::Bool(true))).collect())]);
        Value::Dict(vec![(b("windows"), Value::Dict(vec![
            (b("windowSizesAndPositions_1"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("m1"), geom(1)), (b("m2"), geom(1)), (b("C"), geom(1)),
                (b("43"), geom(7)), (b("51"), geom(9)), (b("market"), geom(3)),
            ])])),
            (b("openWindows"), boolset(&["m1", "m2", "C", "43", "market"])),
            (b("minimizedWindows"), boolset(&["43"])),
            (b("isOverlayedWindows"), boolset(&["43"])),
            (b("isLightBackgroundWindows"), boolset(&["43"])),
            (b("stacksWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("m1"), b("C")), (b("m2"), b("C")),
            ])])),
            (b("preferredIdxInStack3"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("C"), Value::Dict(vec![(b("m1"), Value::Int(0)), (b("m2"), Value::Int(1))])),
                // A stale leftover: the orphan still has its own member dict.
                (b("43"), Value::Dict(vec![(b("gone"), Value::Int(0))])),
            ])])),
        ]))])
    }

    #[test]
    fn delete_orphans_removes_only_the_dead_numeric_frames() {
        let mut v = orphans_root();
        let deleted = delete_orphan_frames(&mut v);
        assert_eq!(deleted, vec!["43".to_string(), "51".to_string()]);

        // Gone from geometry; the live stack and the ordinary window survive.
        let g = keys(inner(win(&v), b"windowSizesAndPositions_1"));
        assert_eq!(g, vec!["m1".to_string(), "m2".to_string(), "C".to_string(), "market".to_string()]);
    }

    #[test]
    fn delete_orphans_clears_every_flag_dict_and_the_stale_pref_entry() {
        let mut v = orphans_root();
        delete_orphan_frames(&mut v);
        for dict in [b"openWindows".as_slice(), b"minimizedWindows", b"isOverlayedWindows", b"isLightBackgroundWindows"] {
            assert!(
                !keys(inner(win(&v), dict)).contains(&"43".to_string()),
                "43 still present in {}", String::from_utf8_lossy(dict),
            );
        }
        // openWindows keeps everything else it had.
        assert!(keys(inner(win(&v), b"openWindows")).contains(&"market".to_string()));
        // The orphan's own preferredIdxInStack3 container entry goes too.
        assert!(!keys(inner(win(&v), b"preferredIdxInStack3")).contains(&"43".to_string()));
        assert!(keys(inner(win(&v), b"preferredIdxInStack3")).contains(&"C".to_string()));
    }

    #[test]
    fn a_container_and_a_member_are_never_orphans() {
        // "C" is a container (a VALUE in stacksWindows) and would otherwise look
        // orphaned if only keys were checked. Give the members numeric ids too,
        // so the numeric test alone cannot save them.
        let mut v = Value::Dict(vec![(b("windows"), Value::Dict(vec![
            (b("windowSizesAndPositions_1"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("70"), Value::Int(0)), (b("71"), Value::Int(0)), (b("99"), Value::Int(0)),
            ])])),
            (b("stacksWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("70"), b("99")), (b("71"), b("99")),
            ])])),
        ]))]);
        assert_eq!(delete_orphan_frames(&mut v), Vec::<String>::new());
    }

    #[test]
    fn delete_orphans_does_not_fabricate_absent_dicts() {
        // Only geometry and stacksWindows exist. The seven other flag dicts must
        // still be absent afterwards — child_inner would happily create them.
        let mut v = Value::Dict(vec![(b("windows"), Value::Dict(vec![
            (b("windowSizesAndPositions_1"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("43"), Value::Int(0)),
            ])])),
            (b("stacksWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![])])),
        ]))]);
        assert_eq!(delete_orphan_frames(&mut v), vec!["43".to_string()]);
        let names = keys(win(&v));
        assert_eq!(names, vec!["windowSizesAndPositions_1".to_string(), "stacksWindows".to_string()]);
    }

    #[test]
    fn delete_orphans_leaves_a_tree_that_still_encodes() {
        // Same guarantee unstack_that_drops_a_shared_def_still_encodes gives:
        // without inline-first, dropping a Shared def leaves a dangling Ref.
        let mut v = orphans_root();
        delete_orphan_frames(&mut v);
        let bytes = blue_marshal::encode(&v).expect("edited tree still encodes");
        assert_eq!(blue_marshal::decode(&bytes).unwrap(), v);
    }

    #[test]
    fn a_file_with_no_windows_dict_deletes_nothing() {
        let mut v = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
        assert_eq!(delete_orphan_frames(&mut v), Vec::<String>::new());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run from the repo root: `cargo test -p settings-model stacks::`
Expected: FAIL to compile — `cannot find function delete_orphan_frames in this scope`.

- [ ] **Step 3: Make `BOOL_FLAGS` visible to `stacks.rs`**

In `crates/settings-model/src/windows.rs`, change line 22 only — one list of flag dicts, shared:

```rust
pub(crate) const BOOL_FLAGS: [&str; 8] = [
```

- [ ] **Step 4: Write the implementation**

Add to `crates/settings-model/src/stacks.rs`, after `unstack` (it belongs with the other membership edits, above `add_to_stack`):

```rust
/// Delete every orphaned stack frame, returning the ids removed.
///
/// EVE mints a numeric-string window id ONLY to be a stack container (see
/// docs/format-notes.md, "Window stacks"), so a numeric id that is the
/// container of no stack, and a member of none, is a dead frame: its members
/// are gone but its geometry and flags remain, painting an empty rectangle.
/// One real character file carried eight, and unstacking a pair mints another,
/// so they accumulate through ordinary use.
///
/// Safe to delete: confirmed in-game 2026-07-28 that the client does not
/// re-create them across a full login/logout.
///
/// This re-derives the orphan set itself rather than taking a caller's list —
/// it is a destructive write reached from an IPC command, and the id list is
/// exactly the thing that must not be trusted from outside.
pub fn delete_orphan_frames(v: &mut Value) -> Vec<String> {
    inline_all(v);
    let Ok(win) = windows_mut(v) else { return Vec::new() };

    // Live ids: every member (key) and every container (value) in stacksWindows.
    let mut live: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Some((_, sw)) = win.iter().find(|(k, _)| is_b(k, b"stacksWindows")) {
        if let Some(entries) = dict_of(sw) {
            for (k, val) in entries {
                if let Value::Bytes(b) = k { live.insert(String::from_utf8_lossy(b).into_owned()); }
                if let Value::Bytes(b) = val { live.insert(String::from_utf8_lossy(b).into_owned()); }
            }
        }
    }

    // Candidates come from geometry: that is the dict the projection enumerates
    // windows from, so this deletes exactly what the UI counted as an orphan.
    let mut orphans: Vec<String> = Vec::new();
    if let Some((_, g)) = win.iter().find(|(k, _)| is_b(k, b"windowSizesAndPositions_1")) {
        if let Some(entries) = dict_of(g) {
            for (k, _) in entries {
                let Value::Bytes(bytes) = k else { continue };
                let id = String::from_utf8_lossy(bytes).into_owned();
                if !id.is_empty() && id.bytes().all(|c| c.is_ascii_digit()) && !live.contains(&id) {
                    orphans.push(id);
                }
            }
        }
    }
    if orphans.is_empty() {
        return orphans;
    }

    // Purge from geometry, the eight flag dicts, and both stack dicts. The
    // presence check matters: child_inner CREATES a missing child, and a file
    // that never had `lockedWindows` must not grow one from a delete.
    let mut names: Vec<&[u8]> = vec![b"windowSizesAndPositions_1", b"stacksWindows", b"preferredIdxInStack3"];
    names.extend(crate::windows::BOOL_FLAGS.iter().map(|n| n.as_bytes()));
    for name in names {
        if !win.iter().any(|(k, _)| is_b(k, name)) {
            continue;
        }
        let d = child_inner(win, name);
        d.retain(|(k, _)| !matches!(k, Value::Bytes(b) if orphans.iter().any(|o| o.as_bytes() == b.as_slice())));
    }
    orphans
}

/// The dict inside a `windows` child, whether bare or `(timestamp, dict)`.
/// Read-only counterpart to `child_inner`, which creates what it cannot find.
fn dict_of(v: &Value) -> Option<&Vec<(Value, Value)>> {
    match v {
        Value::Dict(d) => Some(d),
        Value::Tuple(t) => t.iter().find_map(|e| if let Value::Dict(d) = e { Some(d) } else { None }),
        _ => None,
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run from the repo root: `cargo test -p settings-model stacks::`
Expected: PASS, including the five pre-existing stack tests.

- [ ] **Step 6: Run the whole Rust suite**

Run from the repo root: `cargo test`
Expected: PASS. `windows.rs`'s visibility change touches nothing else, but the projection tests prove it.

- [ ] **Step 7: Commit**

```bash
git add crates/settings-model/src/stacks.rs crates/settings-model/src/windows.rs
git commit -m "Delete orphaned stack frames from every dict that names them"
```

---

### Task 2: Expose it as a command

**Files:**
- Modify: `app/src-tauri/src/ops.rs:21` (the `use` list) and `:1251` (after `stack_create`)
- Modify: `app/src-tauri/src/lib.rs:303` (after `stack_create`) and `:475` (the handler list)
- Test: `app/src-tauri/src/ops.rs` (the `mod tests` block, beside `unstack_reprojects_and_reshares`)

**Interfaces:**
- Consumes: `delete_orphan_frames` from Task 1; `edit_char_stacks` (already private in `ops.rs`).
- Produces: `pub fn stack_delete_orphans(state: &AppState) -> Result<WindowLayout, ErrDto>`, reachable from the frontend as the Tauri command `stack_delete_orphans` taking no arguments and returning `WindowLayout`.

- [ ] **Step 1: Write the failing test**

Add to the `mod tests` block in `app/src-tauri/src/ops.rs`, directly after `unstack_reprojects_and_reshares`:

```rust
    fn orphaned_char_bytes() -> Vec<u8> {
        fn bb(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        fn ts() -> Value { Value::Long(vec![0u8; 8]) }
        fn geom(x: i64) -> Value { Value::Tuple(vec![Value::Int(x), Value::Int(0), Value::Int(100), Value::Int(80), Value::Int(2560), Value::Int(1440)]) }
        // Live stack C(m1, m2) plus one orphan frame "43".
        encode(&Value::Dict(vec![(bb("windows"), Value::Dict(vec![
            (bb("windowSizesAndPositions_1"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (bb("m1"), geom(0)), (bb("m2"), geom(0)), (bb("C"), geom(0)), (bb("43"), geom(0)),
            ])])),
            (bb("openWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (bb("m1"), Value::Bool(true)), (bb("43"), Value::Bool(true)),
            ])])),
            (bb("stacksWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![(bb("m1"), bb("C")), (bb("m2"), bb("C"))])])),
        ]))])).unwrap()
    }

    #[test]
    fn delete_orphans_reprojects_and_reshares() {
        let path = temp_file("stack-orphans", &orphaned_char_bytes());
        let state = AppState::new();
        open_file(&state, Slot::Char, path.to_str().unwrap()).unwrap();

        let wl = stack_delete_orphans(&state).unwrap();
        // The orphan is gone from the projection; the live stack is untouched.
        assert!(!wl.windows.iter().any(|w| w.id == "43"));
        assert_eq!(wl.stacks.len(), 1);
        assert_eq!(wl.stacks[0].members, vec!["m1".to_string(), "m2".to_string()]);
        // Doc still encodes/decodes (reshare ran without corrupting the tree).
        let guard = state.char.lock().unwrap();
        let bytes = blue_marshal::encode(&guard.as_ref().unwrap().value).unwrap();
        assert_eq!(blue_marshal::decode(&bytes).unwrap(), guard.as_ref().unwrap().value);
    }

    #[test]
    fn delete_orphans_on_a_clean_file_is_a_no_op_not_an_error() {
        let path = temp_file("stack-noorphans", &stacked_char_bytes());
        let state = AppState::new();
        open_file(&state, Slot::Char, path.to_str().unwrap()).unwrap();
        let wl = stack_delete_orphans(&state).unwrap();
        assert_eq!(wl.stacks.len(), 1);
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run from the repo root: `cargo test -p app_lib delete_orphans`
Expected: FAIL to compile — `cannot find function stack_delete_orphans`.

- [ ] **Step 3: Write the implementation**

In `app/src-tauri/src/ops.rs`, add `delete_orphan_frames` to the existing `settings_model` import list on line 21:

```rust
    unstack, add_to_stack, reorder_stack, create_stack, delete_orphan_frames, StackError,
```

Then add after `stack_create` (line 1251):

```rust
pub fn stack_delete_orphans(state: &AppState) -> Result<WindowLayout, ErrDto> {
    // Returns the ids it removed; the re-projection is what the UI reads, so
    // they are discarded here. Deleting nothing is a success, not an error.
    edit_char_stacks(state, |v| { delete_orphan_frames(v); Ok(()) })
}
```

`delete_orphan_frames` is not re-exported yet, so the import above will not resolve until you extend `crates/settings-model/src/lib.rs:41`:

```rust
pub use stacks::{add_to_stack, create_stack, delete_orphan_frames, reorder_stack, unstack, StackError};
```

- [ ] **Step 4: Register the command**

In `app/src-tauri/src/lib.rs`, add after `stack_create` (line 303):

```rust
#[tauri::command]
fn stack_delete_orphans(state: tauri::State<'_, AppState>) -> Result<settings_model::WindowLayout, ErrDto> {
    ops::stack_delete_orphans(&state)
}
```

And extend the handler list on line 475:

```rust
            stack_unstack, stack_add, stack_reorder, stack_create, stack_delete_orphans,
```

- [ ] **Step 5: Run the tests to verify they pass**

Run from the repo root: `cargo test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs crates/settings-model/src/lib.rs
git commit -m "Expose orphan-frame deletion as a command"
```

---

### Task 3: One orphan predicate, shared by the filter and the offer

**Files:**
- Modify: `app/src/lib/layout.ts:306-320` (`windowMatches`)
- Modify: `app/src/lib/api.ts:442` (after `stackCreate`)
- Test: `app/src/lib/layout.test.ts`

**Interfaces:**
- Consumes: `WindowRect` from `./api` — the fields that matter are `id: string` and `stack: StackRef | null`.
- Produces: `export function isOrphanFrame(w: WindowRect): boolean`, and `api.stackDeleteOrphans(): Promise<WindowLayout>`. Task 4 uses both.

The orphan rule already exists, inline in `windowMatches` (line 313). Extracting it is what stops the banner's count and the filter's hiding from drifting apart — the banner must offer to delete exactly the frames the filter calls dead.

- [ ] **Step 1: Write the failing test**

`app/src/lib/layout.test.ts` already builds `orphanFrame`, `containerFrame` and `memberFrame` for the `hideClutter` cases at lines 202-204, with `market` defined earlier in the same block. They are **block-scoped** — the block closes at line 210 — so these checks go inside it, directly after the last existing check on line 209. Reuse the fixtures; do not make new ones:

```ts
check("an orphaned numeric frame is an orphan", isOrphanFrame(orphanFrame));
check("a numeric id that IS a stack container is not", !isOrphanFrame(containerFrame));
check("a numeric id that is a stack member is not", !isOrphanFrame(memberFrame));
check("a non-numeric id with no stack is not", !isOrphanFrame(market));
```

Add `isOrphanFrame` to the existing import from `./layout.ts` at the top of the file.

- [ ] **Step 2: Run the test to verify it fails**

Run from `app/`: `node --test "src/lib/layout.test.ts"`
Expected: FAIL — `isOrphanFrame is not a function`.

- [ ] **Step 3: Write the implementation**

In `app/src/lib/layout.ts`, add above `windowMatches`:

```ts
/**
 * A minted numeric window id that belongs to no stack — a dead frame whose
 * members are gone (see docs/format-notes.md, "Window stacks"). It paints a
 * phantom "Window stack" rectangle until it is deleted.
 *
 * Shared by the `Hide clutter` filter and the delete offer on purpose: the
 * offer must remove exactly the frames the filter calls dead, and two copies
 * of this rule would eventually disagree.
 */
export function isOrphanFrame(w: WindowRect): boolean {
  return w.stack === null && /^\d+$/.test(w.id);
}
```

And replace the inline rule in `windowMatches` (line 309-313) with:

```ts
  if (f.hideClutter && isOrphanFrame(w)) return false;
```

- [ ] **Step 4: Add the api binding**

In `app/src/lib/api.ts`, after `stackCreate` (line 442):

```ts
  stackDeleteOrphans: () => invoke<WindowLayout>("stack_delete_orphans"),
```

- [ ] **Step 5: Run the tests to verify they pass**

Run from `app/`: `npm test`
Expected: PASS — including the four pre-existing `hideClutter` orphan cases, which now exercise the extracted function.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/layout.ts app/src/lib/layout.test.ts app/src/lib/api.ts
git commit -m "Share one orphan-frame rule between the filter and the api"
```

---

### Task 4: The offer

**Files:**
- Modify: `app/src/lib/WindowPanel.svelte:7-51` (props) and `:307-325` (the filters block)
- Modify: `app/src/lib/LayoutView.svelte:237` (beside the other stack callbacks) and `:817-828` (the prop list)

**Interfaces:**
- Consumes: `isOrphanFrame` and `api.stackDeleteOrphans` from Task 3; `runStack` (already private in `LayoutView.svelte`).
- Produces: nothing other tasks depend on.

- [ ] **Step 1: Add the prop and the banner**

In `app/src/lib/WindowPanel.svelte`, add `onDeleteOrphans` to the destructured props (after `onCreateStack` on line 20) and to the type block (after line 38):

```ts
    onDeleteOrphans: () => void;
```

Import the predicate by extending the existing `$lib/layout` import on line 4:

```ts
  import { windowMatches, isOrphanFrame, NO_FILTER, type WindowFilter } from "$lib/layout";
```

Add the count as a derived value, beside the other script-level state (after line 53):

```ts
  // Counted from the same predicate the filter uses, so the offer can never
  // name a number the `Hide clutter` toggle disagrees with.
  const orphanCount = $derived(windows.filter(isOrphanFrame).length);
```

And render the banner immediately after the `</div>` closing `.filters` (line 325):

```svelte
  {#if orphanCount > 0 && !readOnly}
    <div class="orphans">
      <span>
        {orphanCount} empty stack frame{orphanCount === 1 ? "" : "s"} — leftovers that draw a
        rectangle with nothing in it.
      </span>
      <button type="button" onclick={onDeleteOrphans}>Delete them</button>
    </div>
  {/if}
```

Add to the component's `<style>` block:

```css
  .orphans {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    padding: 0.35rem 0.4rem;
    margin-bottom: 0.4rem;
    font-size: 0.85em;
    border: 1px solid var(--border);
    border-radius: 3px;
    color: var(--fg-dim);
  }
  .orphans button { flex: none; }
```

- [ ] **Step 2: Wire it up with a confirm**

In `app/src/lib/LayoutView.svelte`, add after `onCreateStack` (line 237):

```ts
  // Deleting window state is not something to get wrong, so it asks first and
  // names the count. Safe to offer at all only because the client was verified
  // not to re-create these (2026-07-28) — see docs/format-notes.md.
  async function onDeleteOrphans() {
    const n = layout.windows.filter(isOrphanFrame).length;
    const ok = await confirm(
      `Delete ${n} empty stack frame${n === 1 ? "" : "s"}? Each is a leftover container whose ` +
        `windows were unstacked. EVE does not re-create them. The change is applied to the open ` +
        `file — save to write it to disk.`,
      { title: "Delete empty stack frames", kind: "warning" },
    );
    if (ok) await runStack(api.stackDeleteOrphans());
  }
```

Import `isOrphanFrame` by extending the existing `$lib/layout` import. `confirm` is **not** imported in this file yet — line 14 imports only `message`, so widen it:

```ts
  import { confirm, message } from "@tauri-apps/plugin-dialog";
```

Then pass it down in the `<WindowPanel …>` prop list (after `{onCreateStack}` on line 824):

```svelte
        {onDeleteOrphans}
```

- [ ] **Step 3: Run the full frontend suite**

Run from `app/`: `npm test`
Expected: PASS.

- [ ] **Step 4: Type-check**

Run from `app/`: `npm run check`
Expected: 0 errors. (Four pre-existing `state_referenced_locally` warnings in `ContextMenu.svelte`, `InsertForm.svelte` and `TreeNode.svelte` are unrelated and must not grow.)

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/WindowPanel.svelte app/src/lib/LayoutView.svelte
git commit -m "Offer to delete the empty stack frames a file has collected"
```

---

### Task 5: Close the ledger and record the finding

**Files:**
- Modify: `docs/small-tasks.md` (two entries — around line 52 and line 588 of the pre-Task-1 file)
- Modify: `docs/format-notes.md` (the "Window stacks" section)

**Interfaces:** none — documentation only.

There are **two** ledger entries for this task: the 2026-07-26 one that specifies the dict list and ends "Check first whether EVE simply re-creates them, in which case it is not worth doing", and the 2026-07-28 one recording that the check came back negative. Both close together; do not leave the older one open.

- [ ] **Step 1: Move both entries to Shipped**

Delete both `- [ ]` entries from the **Open** section and add one merged entry under `### Unreleased (on master)` in the **Shipped** section:

```markdown
- [x] **Offer to delete orphaned stack frames from the file.** `delete_orphan_frames`
  in `stacks.rs` removes every numeric-string id that is neither a stack member
  nor a container, from `windowSizesAndPositions_1`, all eight `BOOL_FLAGS`
  dicts, `stacksWindows` and `preferredIdxInStack3` — one action rather than the
  5-6 hand-deletions each frame needs. The window panel offers it whenever the
  open file carries any, behind a confirm that names the count. Safe because the
  client was verified not to re-create them (2026-07-28: two frames deleted from
  a real file survived a full login/logout). The backend re-derives the orphan
  set rather than trusting an id list over IPC. _Added 2026-07-26 and 2026-07-28
  (two entries, one task); done 2026-07-28._
```

- [ ] **Step 2: Record the in-game finding in the format notes**

In `docs/format-notes.md`, find the "Window stacks" section and add to it:

```markdown
**Deleted frames stay deleted.** Verified 2026-07-28: two orphaned stack frames
(`43`, `51`) were removed from a real character file, the client was run through
a full login/logout, and neither came back — while six untouched controls in the
same file sat still. So an orphan frame is safe to delete outright; the client
neither restores it nor treats its absence as damage.
```

- [ ] **Step 3: Commit**

```bash
git add docs/small-tasks.md docs/format-notes.md
git commit -m "Close the orphan-frame task, and record that deletions stick"
```

---

## Self-review notes

- **Coverage.** The ledger asked for: deletion from the geometry dict and every window-id-keyed flag dict (Task 1, driven off `BOOL_FLAGS` so the list cannot drift), placement in `stacks.rs` with inline-first-then-reshare (Task 1 + the existing `edit_char_stacks`), a confirm step (Task 4), and the "check whether EVE re-creates them" precondition (already answered 2026-07-28, recorded in Task 5).
- **The live smoke.** The 2026-07-26 entry also asked for a live in-game smoke test of the deletion. That smoke has effectively already been run — the 2026-07-28 session deleted two frames from a real file by hand and logged in and out. What has not been smoked is this *button* doing it, which is worth a pass next time the client is open; it is not a code gap.
- **Naming.** `delete_orphan_frames` (Rust) / `isOrphanFrame` (TS) / `stackDeleteOrphans` (api) / `onDeleteOrphans` (prop). The UI copy deliberately says "empty stack frame" rather than "orphan" — the user-facing word describes what they see, the code word describes the structure.
- **No component test for the banner.** The logic worth testing is the predicate (Task 3, pure, in `layout.test.ts`) and the deletion (Task 1-2, Rust). `WindowPanel.svelte` has no `.spec.ts` today and mounting it needs seventeen props; adding that harness to assert one `{#if}` is more scaffolding than the risk justifies. Add it if the banner grows conditions.
- **Not in scope, deliberately.** Deleting a *live* stack (a container that still has members), any per-frame "delete just this one" affordance, and the account-file side. This is the bulk cleanup the ledger asked for and nothing more.
