# Layout stack polish (slice 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make window stacks editable by dragging on the layout canvas — Shift-drag a window onto another to stack them, drag a tab to reorder it, move it to another stack, or pull it out to a place.

**Architecture:** The whole gesture matrix is decided by one pure function, `dropAction`, in `app/src/lib/layout.ts`, so it is unit-tested with no DOM; `LayoutView.svelte` gains only the pointer glue and dispatch into the four `api.stack*` calls that already exist. One backend correction: `add_to_stack` writes the geometry the client itself writes when a window joins a stack.

**Tech Stack:** Rust (`crates/settings-model`, no external deps), Svelte 5 runes, TypeScript, `node --test` for the pure-TS suite, `cargo test` for the crate.

Spec: `docs/superpowers/specs/2026-07-26-layout-stack-polish-design.md`.

## Global Constraints

- **Commit messages are sentence case with NO attribution trailers** (no `Co-Authored-By`, no `Generated with`). Repo convention.
- **No new dependencies**, in either the crate or the app. The crate is dependency-free apart from `serde`/`blue_marshal`.
- **`stacks.rs` inlines first.** Every entry point calls `inline_all(v)` before editing because window-id keys are `Shared` stores; the app layer reshares before saving. Do not remove or reorder that call.
- **Never fabricate a `windows` child dict you only mean to read.** `child_inner` CREATES the child when absent — guard with a presence check before calling it on a read path.
- **Canvas tests are pure only.** The canvas has never had component tests (M2 decision, held through slices 1a/1b); pointer choreography is covered by the live smoke, not by jsdom.
- Run the app suites from `app/`: `npm test` (node --test + vitest), `npm run check` (svelte-check), `npm run build`. Run the crate suite from the repo root: `cargo test -p settings-model`.
- **`npm`/`cargo` are not on the Bash tool's PATH on this machine — run them through PowerShell.**

---

### Task 1: `add_to_stack` adopts the container's rect

`create_stack` already copies member 1's rect onto the container and member 2, matching `docs/format-notes.md` experiment 6 ("all three share one identical rect — a member's prior free-floating rect is discarded"). `add_to_stack` writes no geometry at all, so a window joining an existing stack keeps its old rect and the file carries drift. Fixing it in the shared function also fixes the window panel's *Add to stack…* dropdown, which has the same gap.

**Files:**
- Modify: `crates/settings-model/src/stacks.rs:85-99` (`add_to_stack`) and its `#[cfg(test)] mod tests` at the end of the file.

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: no signature change. `pub fn add_to_stack(v: &mut Value, member: &str, container: &str) -> Result<(), StackError>` keeps its exact signature — the geometry write is additive and best-effort, and it must NOT introduce a new error case.

- [ ] **Step 1: Write the failing tests**

Add both tests at the end of `mod tests` in `crates/settings-model/src/stacks.rs`, after `create_links_members_opens_and_flags_the_container`. `free_windows_root()` (already in the file) has geometry for `m1` (x=10), `m2` (x=99) and `40` (x=0); `root()` (also already in the file) has no `windowSizesAndPositions_1` at all.

```rust
    #[test]
    fn add_moves_the_joining_member_onto_the_container_rect() {
        let mut v = free_windows_root();
        // "40" is an existing window with a rect at x = 0; treat it as the
        // stack container m2 joins. The client discards a joining member's own
        // rect (format-notes.md, experiment 6).
        add_to_stack(&mut v, "m2", "40").unwrap();
        assert_eq!(geom_of(&v, b"m2")[0], 0, "m2 takes the container's rect");
        assert_eq!(geom_of(&v, b"40")[0], 0, "the container's own rect is untouched");
        assert_eq!(geom_of(&v, b"m1")[0], 10, "an unrelated window is untouched");
    }

    #[test]
    fn add_without_geometry_still_writes_membership() {
        // root() has no windowSizesAndPositions_1 dict at all: the membership
        // write must still land, and the geometry dict must NOT be fabricated.
        let mut v = root();
        add_to_stack(&mut v, "m3", "C").unwrap();
        assert!(keys(sw(&v)).contains(&"m3".to_string()));
        assert!(
            !win(&v).iter().any(|(k, _)| matches!(k, Value::Bytes(x) if x == b"windowSizesAndPositions_1")),
            "the geometry dict must not be fabricated by a membership write",
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run (PowerShell, repo root): `cargo test -p settings-model --lib stacks`

Expected: `add_moves_the_joining_member_onto_the_container_rect` FAILS on the first assert (`assertion \`left == right\` failed: left: 99, right: 0` — m2 keeps its own rect). `add_without_geometry_still_writes_membership` PASSES already (nothing fabricates the dict yet) — that is fine, it is the guard test for Step 3.

- [ ] **Step 3: Write the implementation**

In `crates/settings-model/src/stacks.rs`, add this helper next to `set_entry` (near the bottom of the non-test code, after `set_entry`):

```rust
/// Copy the container's rect onto a joining member, as the client does — see
/// docs/format-notes.md ("Window stacks"): a stack's container and every member
/// share one identical rect, and a member's prior free-floating rect is
/// discarded. Best-effort: a file with no geometry dict, or a container with no
/// rect of its own, keeps its membership write and skips this. The presence
/// check matters — `child_inner` would otherwise CREATE the geometry dict.
fn adopt_container_rect(win: &mut Vec<(Value, Value)>, member: &[u8], container: &[u8]) {
    if !win.iter().any(|(k, _)| is_b(k, b"windowSizesAndPositions_1")) {
        return;
    }
    let geoms = child_inner(win, b"windowSizesAndPositions_1");
    let Some(rect) = geoms.iter().find(|(k, _)| is_b(k, container)).map(|(_, r)| r.clone()) else {
        return;
    };
    set_entry(geoms, member, rect);
}
```

Then call it as the first edit inside `add_to_stack`, so the `sw` / `pref` borrows below stay non-overlapping. The function becomes:

```rust
pub fn add_to_stack(v: &mut Value, member: &str, container: &str) -> Result<(), StackError> {
    inline_all(v);
    let win = windows_mut(v)?;
    let (mb, cb) = (member.as_bytes(), container.as_bytes());
    adopt_container_rect(win, mb, cb);

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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p settings-model`

Expected: PASS, whole crate green (the existing `add_inserts_into_both_dicts_with_next_index` must still pass — it runs on `root()`, which has no geometry dict).

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/stacks.rs
git commit -m "Give a window joining a stack the stack's own rect"
```

---

### Task 2: `unitAt` and `moveInOrder` — the two pure helpers

Hit-testing and list reordering, both pure, both needed by Task 3's `dropAction`.

**Files:**
- Modify: `app/src/lib/layout.ts` (append after `snapDelta`, at the end of the file)
- Test: `app/src/lib/layout.test.ts` (append at the end)

**Interfaces:**
- Consumes: `DrawUnit` and `Rect`, both already exported from `layout.ts`.
- Produces:
  - `export function unitAt(units: DrawUnit[], rectOf: (u: DrawUnit) => Rect, x: number, y: number): DrawUnit | null`
  - `export function moveInOrder(ids: string[], id: string, toIndex: number): string[]`

- [ ] **Step 1: Write the failing tests**

Append to `app/src/lib/layout.test.ts`. The file's style is throw-based `check(name, boolean)` — no framework. Add `unitAt, moveInOrder` to the existing import list from `./layout.ts` at the top of the file.

```ts
// --- unitAt: topmost drawn unit under a data-px point ------------------------
{
  const rect = (x: number, y: number, w: number, h: number) => ({ x, y, w, h });
  const u = (key: string, r: { x: number; y: number; w: number; h: number }) =>
    ({ key, anchor: { id: key }, stack: null, tabs: [], fanTargets: [], rect: r }) as any;
  // Two overlapping units; `big` is drawn first, `small` second (on top).
  const big = u("big", rect(0, 0, 500, 500));
  const small = u("small", rect(100, 100, 100, 100));
  const units = [big, small];
  const rectOf = (x: any) => x.rect;

  check("unitAt returns the later-drawn unit where they overlap",
    unitAt(units, rectOf, 150, 150)?.key === "small");
  check("unitAt returns the only unit under a non-overlapping point",
    unitAt(units, rectOf, 400, 400)?.key === "big");
  check("unitAt returns null on empty canvas",
    unitAt(units, rectOf, 900, 900) === null);
  check("unitAt counts the rect edge as inside",
    unitAt(units, rectOf, 100, 100)?.key === "small");
}

// --- moveInOrder: the full ordering reorder_stack takes ----------------------
{
  const ids = ["a", "b", "c", "d"];
  check("moveInOrder moves an id forward",
    moveInOrder(ids, "a", 2).join(",") === "b,c,a,d");
  check("moveInOrder moves an id backward",
    moveInOrder(ids, "d", 1).join(",") === "a,d,b,c");
  check("moveInOrder to the same index is unchanged",
    moveInOrder(ids, "b", 1).join(",") === "a,b,c,d");
  check("moveInOrder clamps an index past the end",
    moveInOrder(ids, "a", 99).join(",") === "b,c,d,a");
  check("moveInOrder leaves the input array alone",
    ids.join(",") === "a,b,c,d");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run (PowerShell, from `app/`): `npm test`

Expected: FAIL — `SyntaxError: The requested module './layout.ts' does not provide an export named 'unitAt'`.

- [ ] **Step 3: Write the implementation**

Append to `app/src/lib/layout.ts`:

```ts
/**
 * The topmost drawn unit whose displayed rect contains a data-px point, or
 * null for empty canvas. The canvas paints `units` in array order, so the LAST
 * match is the one on top — the one a click would hit — hence the reverse walk.
 * `rectOf` is passed in rather than read off the unit because the displayed
 * rect is the live drag preview when there is one, which only the component
 * knows.
 */
export function unitAt(
  units: DrawUnit[],
  rectOf: (u: DrawUnit) => Rect,
  x: number,
  y: number,
): DrawUnit | null {
  for (let i = units.length - 1; i >= 0; i--) {
    const r = rectOf(units[i]);
    if (x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h) return units[i];
  }
  return null;
}

/** `ids` with `id` moved to `toIndex` (clamped into range). This is the whole
 * ordering, because `reorder_stack` rewrites `preferredIdxInStack3[container]`
 * from the list it is given. Pure — the input array is not touched. */
export function moveInOrder(ids: string[], id: string, toIndex: number): string[] {
  const rest = ids.filter((x) => x !== id);
  const at = Math.max(0, Math.min(toIndex, rest.length));
  return [...rest.slice(0, at), id, ...rest.slice(at)];
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npm test`

Expected: PASS, whole suite green.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/layout.ts app/src/lib/layout.test.ts
git commit -m "Add the canvas hit-test and tab-reorder helpers"
```

---

### Task 3: `dropAction` — the gesture matrix as one pure function

Every row of the spec's §2 table is a case here. Nothing in this task touches the component.

**Files:**
- Modify: `app/src/lib/layout.ts` (append after `moveInOrder`)
- Test: `app/src/lib/layout.test.ts` (append at the end)

**Interfaces:**
- Consumes: `unitAt`/`moveInOrder` from Task 2 (`moveInOrder` is called inside `dropAction`), `DrawUnit`, `Rect`.
- Produces:
  - `export type DropAction` — the eight-variant union below. Task 4 and Task 5 switch on `a.op` and rely on these exact field names.
  - `export function dropAction(drag: DragSubject, target: DrawUnit | null, shift: boolean, hoverTabIndex: number | null): DropAction`
  - `export interface DragSubject { unit: DrawUnit; tabId: string | null; rect: Rect }`

- [ ] **Step 1: Write the failing tests**

Append to `app/src/lib/layout.test.ts`, and add `dropAction` to the import list from `./layout.ts`.

```ts
// --- dropAction: the whole canvas gesture matrix -----------------------------
{
  const rect = { x: 10, y: 20, w: 300, h: 200 };
  // Minimal DrawUnit shapes: dropAction only reads key / anchor.id / stack /
  // tabs[].id, so the fixtures carry exactly those.
  const freeUnit = (id: string) =>
    ({ key: id, anchor: { id }, stack: null, tabs: [{ id }], fanTargets: [] }) as any;
  const stackUnit = (container: string, members: string[]) =>
    ({
      key: container,
      anchor: { id: container },
      stack: { container_id: container, container_label: container, anchor_id: container, members },
      tabs: members.map((id) => ({ id })),
      fanTargets: [],
    }) as any;

  const dragged = freeUnit("w1");
  const other = freeUnit("w2");
  const stack = stackUnit("C", ["m1", "m2", "m3"]);
  const other2 = stackUnit("D", ["n1"]);
  const windowDrag = { unit: dragged, tabId: null, rect };

  // --- window drags ---
  check("a plain window drag is a move",
    dropAction(windowDrag, other, false, null).op === "move");
  check("a Shift drag over empty canvas is a move",
    dropAction(windowDrag, null, true, null).op === "move");
  check("a Shift drag onto itself is a move",
    dropAction(windowDrag, dragged, true, null).op === "move");
  {
    const a = dropAction(windowDrag, other, true, null);
    check("Shift onto a free window creates a stack", a.op === "create");
    // create_stack(m1, m2) lands the stack at m1's rect: the target stays put.
    check("the target is member 1, the dragged window member 2",
      a.op === "create" && a.first === "w2" && a.second === "w1");
  }
  {
    const a = dropAction(windowDrag, stack, true, null);
    check("Shift onto a stack joins it", a.op === "add");
    check("the dragged window joins that container",
      a.op === "add" && a.member === "w1" && a.container === "C");
  }
  check("Shift while dragging a whole stack is still a move",
    dropAction({ unit: stack, tabId: null, rect }, other, true, null).op === "move");

  // --- tab drags ---
  const tabDrag = { unit: stack, tabId: "m1", rect };
  {
    const a = dropAction(tabDrag, null, false, null);
    check("a tab dropped on empty canvas unstacks", a.op === "unstack");
    check("it lands at the drop rect",
      a.op === "unstack" && a.member === "m1" && a.rect.x === 10 && a.rect.w === 300);
  }
  {
    const a = dropAction(tabDrag, stack, false, 2);
    check("a tab dropped on its own strip reorders", a.op === "reorder");
    check("the order is the full member list, moved",
      a.op === "reorder" && a.container === "C" && a.order.join(",") === "m2,m3,m1");
  }
  check("a tab dropped on its own rect body does nothing",
    dropAction(tabDrag, stack, false, null).op === "none");
  check("a tab dropped on its own index does nothing",
    dropAction(tabDrag, stack, false, 0).op === "none");
  {
    const a = dropAction(tabDrag, other2, false, null);
    check("a tab dropped on another stack moves between stacks", a.op === "unstackInto");
    check("into that container",
      a.op === "unstackInto" && a.member === "m1" && a.container === "D");
  }
  {
    const a = dropAction(tabDrag, other, true, null);
    check("Shift + a tab onto a free window creates a stack there", a.op === "unstackCreate");
    check("with the free window as member 1",
      a.op === "unstackCreate" && a.member === "m1" && a.target === "w2");
  }
  check("without Shift, a tab onto a free window just lands there",
    dropAction(tabDrag, other, false, null).op === "unstack");

  // The reorder order must come from stack.members, NOT the visible tabs: a
  // filter can hide a member, and reorder_stack rewrites the whole dict from
  // the list it is given — dropping a hidden member would lose its index.
  {
    const filtered = stackUnit("C", ["m1", "m2", "m3"]);
    filtered.tabs = [{ id: "m1" }, { id: "m3" }]; // m2 hidden by the filter
    const a = dropAction({ unit: filtered, tabId: "m1", rect }, filtered, false, 1);
    check("a reorder under a filter keeps the hidden member",
      a.op === "reorder" && a.order.join(",") === "m2,m3,m1");
  }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run (from `app/`): `npm test`

Expected: FAIL — `does not provide an export named 'dropAction'`.

- [ ] **Step 3: Write the implementation**

Append to `app/src/lib/layout.ts`:

```ts
/** What a canvas drop does. One variant per row of the stack-polish spec's
 * gesture table; `none` is a drop that asks for nothing. */
export type DropAction =
  | { op: "move" }
  | { op: "none" }
  | { op: "create"; first: string; second: string }
  | { op: "add"; member: string; container: string }
  | { op: "unstack"; member: string; rect: Rect }
  | { op: "unstackInto"; member: string; container: string }
  | { op: "unstackCreate"; member: string; target: string }
  | { op: "reorder"; container: string; order: string[] };

/** What is being dragged: a unit, optionally by one of its tabs, and the rect
 * it would land at (the live preview). */
export interface DragSubject {
  unit: DrawUnit;
  /** The dragged tab's window id, or null when the whole rect is being moved. */
  tabId: string | null;
  rect: Rect;
}

/**
 * Decide a drop. `target` is the unit under the pointer (`unitAt`), `shift`
 * whether Shift is down, `hoverTabIndex` which of the target's VISIBLE tabs the
 * pointer is over (null when it is not over the strip) — the one input that has
 * to be measured from the DOM.
 *
 * Shift is only what disambiguates a drag that also has a plain-move meaning:
 * a window drag always could have been a move, so stacking needs the modifier;
 * a tab dropped on another *stack* has no competing meaning and needs none.
 */
export function dropAction(
  drag: DragSubject,
  target: DrawUnit | null,
  shift: boolean,
  hoverTabIndex: number | null,
): DropAction {
  const { unit, tabId } = drag;

  if (tabId === null) {
    // Whole-rect drag. A stack can't be dragged into another stack (merging is
    // out of scope), so Shift is ignored for one.
    if (!shift || !target || target.key === unit.key || unit.stack) return { op: "move" };
    return target.stack
      ? { op: "add", member: unit.anchor.id, container: target.stack.container_id }
      // create_stack(m1, m2) puts the stack at m1's rect: the window that
      // stayed put keeps its position and becomes tab 0.
      : { op: "create", first: target.anchor.id, second: unit.anchor.id };
  }

  // Tab drag. It always leaves its stack unless it is dropped back on it.
  if (!unit.stack) return { op: "none" }; // unreachable: a tab implies a stack
  if (target && target.key === unit.key) {
    if (hoverTabIndex === null) return { op: "none" }; // over the body, not the strip
    const over = unit.tabs[hoverTabIndex]?.id;
    if (over === undefined) return { op: "none" };
    // Reorder against the FULL member list, not the visible tabs: reorder_stack
    // rewrites the container's whole index dict from what it is given, so a
    // member the filter is hiding must still be in the list.
    const members = unit.stack.members;
    const to = members.indexOf(over);
    if (to < 0) return { op: "none" };
    const order = moveInOrder(members, tabId, to);
    if (order.join(" ") === members.join(" ")) return { op: "none" };
    return { op: "reorder", container: unit.stack.container_id, order };
  }
  if (target?.stack) return { op: "unstackInto", member: tabId, container: target.stack.container_id };
  if (target && shift) return { op: "unstackCreate", member: tabId, target: target.anchor.id };
  return { op: "unstack", member: tabId, rect: drag.rect };
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npm test`

Expected: PASS.

- [ ] **Step 5: Type-check**

Run: `npm run check`

Expected: 0 errors, 0 warnings.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/layout.ts app/src/lib/layout.test.ts
git commit -m "Decide every canvas stack gesture in one pure function"
```

---

### Task 4: Shift-drag a window onto another to stack it

Wires Task 3's decision into the existing pointer drag: the drop target highlight, the dispatch, and the discoverability hint. Tab drags are Task 5.

**Files:**
- Modify: `app/src/lib/LayoutView.svelte`

**Interfaces:**
- Consumes: `dropAction`, `unitAt`, `DropAction`, `DragSubject` from Task 3; the existing `api.stackCreate(member1, member2)` / `api.stackAdd(member, container)` (both return a fresh `WindowLayout`).
- Produces (Task 5 builds directly on these):
  - `runStack(p: Promise<WindowLayout>): Promise<boolean>` — was `Promise<void>`; now reports success so two-call sequences can stop after a failure.
  - `pointerData(e: PointerEvent): { x: number; y: number }` — pointer position in data px relative to the canvas.
  - `targetAt(e: PointerEvent, dragged: DrawUnit): DrawUnit | null` — the unit under the pointer, excluding the dragged one.
  - `applyDrop(a: DropAction, unit: DrawUnit): Promise<void>` — the dispatch switch.
  - `let dropTarget = $state<string | null>(null)` — the `DrawUnit.key` currently highlighted.

- [ ] **Step 1: Make `runStack` report success**

In `app/src/lib/LayoutView.svelte`, change the existing `runStack` (around line 217):

```ts
  async function runStack(p: Promise<WindowLayout>): Promise<boolean> {
    try {
      layout = await p;
      onDirty("char"); // stack ops edit the character document in the backend
      if (selectedId && !layout.windows.some((w) => w.id === selectedId)) selectedId = null;
      return true;
    } catch (e) {
      await message(errMessage(e), { title: "Stack edit failed", kind: "error" });
      return false;
    }
  }
```

The four existing `onUnstack` / `onReorder` / `onAddToStack` / `onCreateStack` arrow functions ignore the return value and need no change.

- [ ] **Step 2: Add the drop-target state and the pointer helper**

Add the import (extend the existing `$lib/layout` import list) with `unitAt, dropAction, type DropAction`.

Below `let guides = $state…` (around line 252) add:

```ts
  // The DrawUnit.key of the unit a Shift-drag (or a tab drag) is hovering as a
  // stack target; null when the drop would not stack anything. Drives the
  // highlight only — the drop re-resolves the target from the up event.
  let dropTarget = $state<string | null>(null);
```

And next to `rectOf` / `fRectOf` (around line 259):

```ts
  /** Pointer position in data px, relative to the canvas origin. */
  function pointerData(e: PointerEvent) {
    const box = canvasEl!.getBoundingClientRect();
    return { x: toData(e.clientX - box.left, scale), y: toData(e.clientY - box.top, scale) };
  }

  /** The unit under the pointer, excluding the one being dragged. */
  function targetAt(e: PointerEvent, dragged: DrawUnit): DrawUnit | null {
    const p = pointerData(e);
    const u = unitAt(units, (x) => rectOf(x.anchor), p.x, p.y);
    return u && u.key !== dragged.key ? u : null;
  }
```

- [ ] **Step 3: Highlight the target while Shift is held**

In `onPointerMove`, immediately after the `if (drag.kind === "furniture") { … return; }` block and before the `const tol = …` line, add:

```ts
    // Shift over another unit marks it as a stack target. Read off the event
    // like Alt is, so pressing or releasing Shift mid-drag takes effect on the
    // next pointer move. A stack can't be merged into another (spec §2), so a
    // stack drag never highlights anything.
    dropTarget = e.shiftKey && drag.kind === "move" && !drag.unit.stack
      ? (targetAt(e, drag.unit)?.key ?? null)
      : null;
```

- [ ] **Step 4: Dispatch on drop**

Change `onPointerUp` to take the event — the template already passes it (`onpointerup={onPointerUp}`), the function just ignores it today:

```ts
  async function onPointerUp(e: PointerEvent) {
```

Replace its final line `await commitUnit(d.unit);` with:

```ts
    const target = d.kind === "move" && e.shiftKey ? targetAt(e, d.unit) : null;
    dropTarget = null;
    await applyDrop(
      dropAction({ unit: d.unit, tabId: null, rect: rectOf(d.unit.anchor) }, target, e.shiftKey, null),
      d.unit,
    );
```

And add `applyDrop` after `commitUnit`:

```ts
  /** Carry out a decided drop. A stacking drop deliberately does NOT commit the
   * drag's geometry: the joining window adopts the stack's rect in the backend
   * (stacks.rs), so writing the drag coordinates first would be a write the
   * next projection immediately overwrites. */
  async function applyDrop(a: DropAction, unit: DrawUnit) {
    switch (a.op) {
      case "move":
        await commitUnit(unit);
        return;
      case "none":
        clearPreview(unit.anchor.id);
        return;
      case "create":
        clearPreview(unit.anchor.id);
        await runStack(api.stackCreate(a.first, a.second));
        return;
      case "add":
        clearPreview(unit.anchor.id);
        await runStack(api.stackAdd(a.member, a.container));
        return;
    }
  }
```

(The four tab-only variants are added in Task 5; TypeScript is satisfied because the switch returns in every handled case and falls through to the end of the function otherwise.)

- [ ] **Step 5: Clear the highlight on a file switch**

In the reload `$effect` (around line 129), beside `preview = {}` / `fPreview = {}` / `nudging = null`, add:

```ts
      dropTarget = null;
```

- [ ] **Step 6: Highlight and hint in the template**

On the `.win` div (around line 540) add a class directive after `class:stacked`:

```svelte
            class:droptarget={dropTarget === unit.key}
```

In the `<style>` block, after the `.win.stacked` rule, add:

```css
  /* The unit a Shift-drag would stack onto. Deliberately NOT the amber of a
     selection — this is a transient "drop here", not a state. */
  .win.droptarget {
    border-color: #34d399;
    background: rgba(52, 211, 153, 0.3);
    box-shadow: 0 0 0 2px rgba(52, 211, 153, 0.5);
    z-index: 1;
  }
```

And make the gesture discoverable — in the `.ref` paragraph (around line 566), after the `reference {layout.reference_w}×{layout.reference_h}` line:

```svelte
        {#if !readOnly}
          <span class="hintish">· Shift-drag onto another window to stack</span>
        {/if}
```

with the style, next to `.showing`:

```css
  .hintish {
    color: #666;
  }
```

- [ ] **Step 7: Verify**

Run (from `app/`): `npm run check`, then `npm test`, then `npm run build`.

Expected: check 0 errors; tests green; build succeeds.

- [ ] **Step 8: Commit**

```bash
git add app/src/lib/LayoutView.svelte
git commit -m "Stack two windows by Shift-dragging one onto the other"
```

---

### Task 5: Drag a tab to reorder it, move it, or pull it out

**Files:**
- Modify: `app/src/lib/LayoutView.svelte`
- Modify: `docs/small-tasks.md` (ledger the one-member-stack non-goal)

**Interfaces:**
- Consumes: everything Task 4 produced (`runStack` returning a boolean, `pointerData`, `targetAt`, `applyDrop`, `dropTarget`), plus `api.stackUnstack(member)` and `api.stackReorder(container, members)`.
- Produces: nothing later tasks consume — this is the last code task.

- [ ] **Step 1: Add the tab drag variant**

In the `Drag` union (around line 244) add a fourth variant:

```ts
    | { kind: "tab"; unit: DrawUnit; tabId: string; startX: number; startY: number; gx: number; gy: number };
```

`gx`/`gy` are the pointer's offset inside the stack's rect at grab time, so a pulled-out window lands under the cursor the way it looked, not with its corner teleported to the pointer.

Below `let dropTarget = …` add:

```ts
  // The tab id of a tab drag that has passed the travel threshold; null while a
  // press is still just a click. Without the threshold, selecting a tab with a
  // twitchy mouse would unstack it. $state because the template reads it (the
  // `drag` variable itself is deliberately not reactive and must not be read
  // from markup).
  let draggingTab = $state<string | null>(null);
  // Which of the hovered unit's VISIBLE tabs the pointer is over, or null.
  // Handler-only, so a plain let.
  let hoverTab: number | null = null;
```

- [ ] **Step 2: Start a tab drag on pointerdown**

Add next to `startMove`:

```ts
  /** A tab press selects (as it always has) and arms a drag. Whether that drag
   * reorders, moves the window to another stack, or pulls it out is decided
   * entirely by where it is released — see dropAction. */
  function startTab(unit: DrawUnit, tabId: string, e: PointerEvent) {
    e.stopPropagation(); // or the stack's own move drag starts underneath
    selectWindow(tabId);
    if (readOnly) return;
    const r = rectOf(unit.anchor);
    const p = pointerData(e);
    drag = { kind: "tab", unit, tabId, startX: e.clientX, startY: e.clientY, gx: p.x - r.x, gy: p.y - r.y };
    draggingTab = null;
    canvasEl?.setPointerCapture(e.pointerId);
    e.preventDefault();
  }
```

- [ ] **Step 3: Track the hover during a tab drag**

Add the tab branch in `onPointerMove`, immediately after the `furniture` branch and BEFORE the `dropTarget = e.shiftKey && …` line from Task 4 (that line reads `drag.unit`, which the tab branch handles itself):

```ts
    if (drag.kind === "tab") {
      // 4 canvas px of travel turns the press into a drag. Compared in client
      // px because it is a hand-tremor threshold, not a data-space distance.
      if (Math.abs(e.clientX - drag.startX) > 4 || Math.abs(e.clientY - drag.startY) > 4) {
        draggingTab = drag.tabId;
      }
      if (draggingTab === null) return;
      const p = pointerData(e);
      const over = unitAt(units, (x) => rectOf(x.anchor), p.x, p.y);
      const own = over?.key === drag.unit.key;
      // Highlight only a drop that goes somewhere else; hovering the tab's own
      // stack is a reorder, which the strip itself shows.
      dropTarget = own ? null : (over?.key ?? null);
      hoverTab = own ? tabIndexAt(e.clientX, e.clientY) : null;
      return;
    }
```

And add the DOM measurement helper next to `pointerData`:

```ts
  /** The index of the tab element under the pointer, or null. Read off the
   * elements' own data attribute rather than computed: tab widths come from
   * their text, so there is nothing in the data to compute from.
   * elementsFromPoint still sees them while the canvas holds pointer capture. */
  function tabIndexAt(clientX: number, clientY: number): number | null {
    for (const el of document.elementsFromPoint(clientX, clientY)) {
      const i = (el as HTMLElement).dataset?.tabIndex;
      if (i !== undefined) return Number(i);
    }
    return null;
  }
```

- [ ] **Step 4: Decide and dispatch the tab drop**

In `onPointerUp`, after the furniture branch and before the `const target = …` line from Task 4, add:

```ts
    if (d.kind === "tab") {
      const wasDrag = draggingTab !== null;
      draggingTab = null;
      const index = hoverTab;
      hoverTab = null;
      dropTarget = null;
      if (!wasDrag) return; // a press that never travelled is just a select
      const p = pointerData(e);
      const r = rectOf(d.unit.anchor);
      const target = unitAt(units, (x) => rectOf(x.anchor), p.x, p.y);
      await applyDrop(
        dropAction(
          { unit: d.unit, tabId: d.tabId, rect: { x: p.x - d.gx, y: p.y - d.gy, w: r.w, h: r.h } },
          target,
          e.shiftKey,
          index,
        ),
        d.unit,
      );
      return;
    }
```

Note the target here is `unitAt` directly, not `targetAt`: a tab dropped on its OWN stack is the reorder case, so the dragged unit must not be excluded.

- [ ] **Step 5: Handle the four tab actions in `applyDrop`**

Add these cases to the `applyDrop` switch from Task 4, after the `add` case:

```ts
      case "reorder":
        await runStack(api.stackReorder(a.container, a.order));
        return;
      case "unstack":
        await unstackTo(a.member, a.rect);
        return;
      case "unstackInto":
        if (await runStack(api.stackUnstack(a.member))) await runStack(api.stackAdd(a.member, a.container));
        return;
      case "unstackCreate":
        if (await runStack(api.stackUnstack(a.member))) await runStack(api.stackCreate(a.target, a.member));
        return;
```

And add `unstackTo` after `applyDrop`:

```ts
  /** Free a window from its stack and put it where it was dropped. The geometry
   * paths MUST come from the layout the unstack returned: the projection
   * captured before it describes a document that no longer exists in that
   * shape. Without the placement half the freed window would take the stack's
   * exact rect and sit invisibly behind it. */
  async function unstackTo(member: string, rect: Rect) {
    if (!(await runStack(api.stackUnstack(member)))) return;
    const w = layout?.windows.find((x) => x.id === member);
    if (!w?.geom) return;
    await commit(geomMutations(w, rect));
  }
```

Add `type Rect` to the existing `$lib/layout` import list.

- [ ] **Step 6: Wire the tabs in the template**

Replace the tab `{#each}` block (around line 548) with:

```svelte
                {#each unit.tabs as tab, i (tab.id)}
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <span class="tab" class:active={tab.id === selectedId}
                    class:dragging={draggingTab === tab.id}
                    data-tab-index={i} title={tab.id}
                    onpointerdown={(e) => startTab(unit, tab.id, e)}>{displayNameOf(tab)}</span>
                {/each}
```

Add to the `<style>` block after `.tab.active`:

```css
  /* The tab being dragged. No floating ghost rect: the target highlight and
     this are enough to read the gesture, and a ghost would need its own
     hit-test exclusions. */
  .tab.dragging {
    opacity: 0.45;
  }
```

Extend the hint from Task 4 so the tab gesture is discoverable too:

```svelte
          <span class="hintish">· Shift-drag onto another window to stack · drag a tab to reorder or pull out</span>
```

- [ ] **Step 7: Clear the tab-drag state on a file switch**

In the reload `$effect`, beside `dropTarget = null;` from Task 4, add:

```ts
      draggingTab = null;
      hoverTab = null;
```

- [ ] **Step 8: Ledger the one-member-stack non-goal**

Add to the **Open** list in `docs/small-tasks.md`, directly above the "Offer to delete orphaned stack frames from the file." item (they are the same subject):

```markdown
- [ ] **Decide what a one-member stack should do.** Slice 2 lets a tab be
  dragged out of a stack, which can leave the stack with a single member. The
  editor leaves it alone: what the client does with a one-member stack was
  never captured, and the file evidence points at leaving frames behind (a real
  character file carried 8 orphaned containers, below). Settle it in a live
  capture — drag the second-to-last window out of a stack in-game, log out, and
  look at whether `stacksWindows` / `preferredIdxInStack3` still name the last
  member — then either auto-dissolve on the drag-out or leave this closed.
  _Added 2026-07-26 (layout stack polish)._
```

- [ ] **Step 9: Verify**

Run (from `app/`): `npm run check`, then `npm test`, then `npm run build`.

Expected: check 0 errors; tests green; build succeeds.

- [ ] **Step 10: Commit**

```bash
git add app/src/lib/LayoutView.svelte docs/small-tasks.md
git commit -m "Drag a stack tab to reorder it, move it or pull it out"
```

---

### Task 6: Whole-branch review and the live smoke

**Files:** none by default — this task produces fixes only if the review or the smoke finds something.

- [ ] **Step 1: Run the full suites once more, from a clean state**

Run from the repo root: `cargo test -p settings-model`
Run from `app/`: `npm test`, `npm run check`, `npm run build`

Expected: all green. Record the actual output; do not claim a pass without it.

- [ ] **Step 2: Whole-branch code review**

Use the `superpowers:requesting-code-review` skill against `master..HEAD`. Fix anything blocking; anything non-blocking goes into `docs/small-tasks.md` as ship-as-debt, in the same style as the existing entries.

- [ ] **Step 3: Live smoke on a real character file**

The project ships nothing in this milestone without one. EVE writes its settings on **logout**, so log the character out before saving from the editor, or the client overwrites the file on exit.

1. Shift-drag one free window onto another. The target highlights green; on drop the two draw as one tabbed rectangle. Save, log in, confirm the two windows are tabbed together in-game.
2. Shift-drag a third window onto that stack. Confirm in-game that it joins **at the stack's position** — this is Task 1's fix seen from the outside; before it, the joining window kept its old rect.
3. Drag a tab out to an empty part of the canvas. The window must appear where it was dropped, at the stack's size, and in-game must be free-floating there.
4. Drag a tab from one stack onto another stack's rectangle. Confirm the window leaves the first and appears as a tab of the second.
5. Drag tabs within one stack's strip to reorder them; confirm the in-game tab order matches.
6. With `Hide clutter` or a filter narrowing the list, reorder a stack whose members are partly hidden, then clear the filter: no member may have lost its place.
7. Press a tab without moving: it still just selects, and nothing is written (the dirty badge must not appear).

- [ ] **Step 4: Fix what the smoke finds, then re-verify**

Any fix gets its own commit and re-runs Step 1's suites.

---

## Notes for the reviewer

- **Behaviour change worth knowing:** dragging a stack by its **tab strip** now reorders/pulls out instead of doing nothing. Dragging a stack by its **body** still moves it, exactly as before — the tab spans already called `stopPropagation`, so no move gesture is lost.
- **Why `add_to_stack` changed and not just the canvas caller:** the panel's *Add to stack…* dropdown has the same geometry gap, and one write in the shared function covers both call sites.
- **Sequencing, not transactions:** `unstackInto` / `unstackCreate` are two backend calls. If the second fails the window is simply free at the stack's rect and the dialog says what failed — each call is its own complete, valid edit, so there is no half-written document.
