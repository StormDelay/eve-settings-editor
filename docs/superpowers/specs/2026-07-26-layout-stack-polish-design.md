# Layout depth — stack polish (design)

Status: designed 2026-07-26, not yet planned.

Milestone context: the **layout depth** milestone, cut into four slices (see the
HUD furniture spec §7). Slice 3 (HUD furniture) shipped as v0.15.0, slice 1a
(canvas & list usability) as v0.16.0, slice 1b (precision editing) as v0.17.0,
and the names-and-noise slice as v0.19.0. This spec covers **slice 2 — stack
polish**: the membership editing the window-stacks V1 spec (§9) deferred out of
the canvas, plus tab reordering by drag.

Builds on: `stacks.rs` (`unstack` / `add_to_stack` / `reorder_stack` /
`create_stack`, all four already shipped and wired to `api.stack*`),
`layout.ts`'s `DrawUnit` / `stackUnits`, and the pointer-drag state machine
`LayoutView.svelte` grew in slices 1a/1b.

## 1. Goal

Every stack operation exists and works today — through the **window panel**.
Making a stack means finding a window in a ~300-row list, opening its *Stack
with…* dropdown, and picking a second window by name. Reordering tabs means
clicking ↑/↓ buttons on member rows. None of it happens where the user is
actually looking, which is the canvas: the picture of their screen, where the
two windows they want tabbed together are visibly right next to each other.

In-game, stacking a window is a drag. This slice makes it a drag here too, and
adds the one thing the panel has no way to express at all — pulling a window out
of a stack *to a place*, rather than out to wherever it happened to be before.

## 2. Scope

In scope, as one gesture vocabulary:

| gesture | result |
|---|---|
| drag a window | move (snapping as in 1b) — unchanged |
| Alt + drag | move, no snap — unchanged |
| **Shift + drag a window onto another window** | create a stack at the target's rect |
| **Shift + drag a window onto a stack** | join that stack |
| **drag a tab inside its own stack's rect** | reorder to the position under the pointer |
| **drag a tab onto another stack** | leave the old stack, join that one |
| **Shift + drag a tab onto a free window** | leave the stack, create a stack with it |
| **drag a tab onto empty canvas** | leave the stack and land at the drop position |
| drag a whole stack rect | plain move; Shift is ignored |

Plus one backend correction the gestures depend on (§5).

Out of scope, and why:

- **Merging two stacks.** Shift-dragging a stack rect onto another unit would
  have to fan out one `add_to_stack` per member, with a partial-failure state to
  design, for a case no user has asked for. Shift on a stack drag does nothing.
- **Dissolving a stack that a drag-out leaves with one member.** What the client
  does with a one-member stack was never captured, and the file evidence points
  the other way: a real character file carried **8** orphaned containers whose
  members were all gone (see the ledger's "offer to delete orphaned stack
  frames"), so EVE leaves frames behind rather than tidying them. Guessing here
  would write a file shape nothing has been observed to produce. The user can
  still unstack the last member by hand.
- **Drag-and-drop in the window panel list.** The panel keeps its dropdowns and
  its ↑/↓ buttons; they stay the keyboard-reachable path, and they are how a
  window hidden by the filter is still stackable.
- **Touch.** The canvas is pointer-events based and would mostly work, but no
  touch target sizing or gesture disambiguation is designed here.

## 3. The decision is one pure function

The gesture table above is a state machine with exactly one interesting
question: *given what is being dragged, what is under the pointer, and whether
Shift is down — what happens on drop?* That question is answered by a pure
function in `layout.ts`, so the whole matrix is unit-testable with no DOM and
`LayoutView.svelte` (already 772 lines) gains only glue:

```ts
export type DropAction =
  | { op: "move" }                                             // commit geometry, as today
  | { op: "none" }                                             // no-op drop
  | { op: "create"; first: string; second: string }            // create_stack(first, second)
  | { op: "add"; member: string; container: string }           // add_to_stack
  | { op: "unstack"; member: string; rect: Rect }              // unstack, then place
  | { op: "unstackInto"; member: string; container: string }   // unstack, then add
  | { op: "unstackCreate"; member: string; target: string }    // unstack, then create
  | { op: "reorder"; container: string; order: string[] };

export function dropAction(
  drag: { unit: DrawUnit; tabId: string | null; rect: Rect },
  target: DrawUnit | null,
  shift: boolean,
  hoverTabIndex: number | null,
): DropAction;
```

`rect` is the previewed drop rect — where an `unstack` places the freed window.
`hoverTabIndex` is the only DOM-derived input: which tab of the target the
pointer is over, measured from the tab elements (§4.2) and `null` when the
pointer is not over the strip. Everything else the function needs it already has.

`first` in a `create` is always the **target** window, because
`create_stack(m1, m2)` puts the new stack at `m1`'s rect and gives it tab 0 —
the window that stayed put should keep its position, and the window that was
dragged onto it becomes the second tab.

Two supporting helpers, also pure and also in `layout.ts`:

- `unitAt(units, rectOf, x, y): DrawUnit | null` — the topmost drawn unit whose
  displayed rect contains a data-px point. Iterates `units` in **reverse**: the
  canvas renders them in array order, so the last one drawn is the one on top
  and the one a click would hit. Returns `null` for empty canvas.
- `moveInOrder(ids, id, toIndex): string[]` — the reordered member list a
  `reorder` action sends to `reorder_stack`, which takes a full ordering.

Everything else — which tab element the pointer is over, the highlight class,
the drag threshold — stays in the component, because it is DOM.

## 4. Interaction

### 4.1 Dragging a window with Shift

`Drag`'s `"move"` variant is unchanged. During `onPointerMove`, when `e.shiftKey`
is down and the dragged unit is **not** a stack, the unit under the pointer
(excluding the dragged one) is resolved with `unitAt` and stored in a
`dropTarget` state; the canvas gives that rect a highlight class. Releasing Shift
mid-drag clears it, exactly as releasing Alt re-enables snapping — both read the
modifier off the pointer event rather than a key listener, and both therefore
take effect on the next pointer move (1b's established behaviour).

On drop with a `dropTarget`, the geometry preview is **discarded rather than
committed**: the dragged window is about to adopt the stack's rect (§5), so
writing the drag's coordinates first would be a write the next projection
overwrites. Snapping still runs while Shift is held; it is harmless, since
nothing is written.

### 4.2 Dragging a tab

A tab's `pointerdown` currently selects the window and stops propagation, so the
gesture is unclaimed — no modifier is needed to disambiguate it. It now also
arms a drag, which becomes real only past a **4 canvas px** threshold; under
that, the drop is still an ordinary click-to-select, which is what it has always
been.

What a tab drag does is decided entirely by where it is dropped:

- **Inside its own stack's rect** — reorder. The insertion index is the tab the
  pointer is over, found from the tab elements' own bounding boxes (each carries
  a `data-tab-id`); dropping over the rect's body rather than the strip yields
  the tab's current index, which `dropAction` reports as `none`.
- **Over another unit** — leave and join, per the table. A stack target needs no
  modifier (a tab drag has no competing meaning); a free-window target needs
  Shift, matching window-onto-window, so that a tab dropped on a big background
  window without Shift means "put it here", not "stack it with that".
- **Over empty canvas** — unstack and place, keeping the stack's width and
  height. Without the placement half, the freed window would take the stack's
  exact rect and sit invisibly behind it, and the gesture would look like it did
  nothing.

The tab being dragged is drawn with a dragging class; the canvas does not paint
a floating ghost rect for it (the highlight on the target and the tab's own
state are enough, and a ghost would need its own hit-test exclusions).

### 4.3 Ordering, and why it matters

`unstack`, `add_to_stack`, `reorder_stack` and `create_stack` all return a fresh
`WindowLayout`, so each call re-projects. The two-call actions must therefore
**re-resolve from the returned layout**, not from the pre-drag one:
`unstack` → find the member in the new layout → emit `geomMutations` against its
new `geom` paths. A path captured before the unstack describes a document that
no longer exists in that shape.

After any stack action the layout is replaced (as `runStack` already does) and
the moved window stays selected, so the user can see where it landed.

## 5. `add_to_stack` does not write geometry — and should

`format-notes.md` (experiment 6, "Window stacks") records what the client itself
does when two windows are tabbed together: **all three ids share one identical
rect** — "a member's prior free-floating rect is discarded".

`create_stack` implements that: the container and member 2 both take member 1's
rect. `add_to_stack` does not touch `windowSizesAndPositions_1` at all, so a
window joining an existing stack keeps whatever rect it had. The projection hides
the discrepancy — a stack draws at its anchor — but the file carries a member
whose geometry disagrees with its stack until some later move fans the anchor's
rect back over it.

The fix belongs in `add_to_stack`, not in the canvas caller: the panel's *Add to
stack…* dropdown has had the same gap since V1, and one write in the shared
function covers both call sites. The joining member takes the **container's**
rect (per the format notes, the container's rect is the stack's true on-screen
position). If the container has no geometry entry, the membership write still
goes through and the geometry is skipped — membership is the user's intent, and
a stack whose container has no rect is already not drawn.

This is the only Rust in the slice.

## 6. Errors

Every action routes through the existing `runStack`, which replaces the layout on
success and raises the standard dialog on failure. The two-call actions are
sequenced, not transactional, and that is acceptable because the intermediate
state is coherent rather than corrupt: if the `add` half fails after the
`unstack` half succeeded, the window is simply free, sitting at the stack's rect,
and the dialog says what failed. There is no half-written document — each backend
call is its own complete edit.

A window drag that finds no stack target — Shift held over empty canvas, Shift
held while dragging a stack, or no Shift at all — resolves to `move` and commits
its geometry exactly as it does today. `none` is reserved for the tab drags that
ask for nothing: a tab returned to its own index, or dropped back on its own
stack's body. Those write nothing, because a tab drag has no geometry of its own
to fall back on.

## 7. Testing

- **`layout.test.ts`** (node --test, no DOM): the `dropAction` matrix — one case
  per row of §2's table plus the no-op rows; `unitAt` for topmost-wins between
  two overlapping units, a hit inside a stack rect, and a miss; `moveInOrder` for
  a forward move, a backward move, and a move to the same index.
- **`stacks.rs`**: `add_to_stack` copies the container's rect onto the joining
  member; and the container-without-geometry case still writes membership.
- **Component tests**: none. The canvas has never had them (M2's decision, held
  through 1a/1b) — the pointer choreography is what the live smoke is for.
- **Live smoke**, as every slice, on a real character file: Shift-drag two free
  windows together and confirm in-game that they are tabbed; Shift-drag a third
  onto the resulting stack and confirm it joins **at the stack's position**
  (that is §5's fix, seen from the outside); drag a tab out to an empty spot and
  confirm the window appears there in-game; drag a tab from one stack to another;
  reorder tabs by drag and confirm the in-game tab order matches. Note the write
  order the project always observes: EVE writes its settings on **logout**, so
  log the character out before saving from the editor.

## 8. Non-goals

Restated from §2 so they are not re-litigated at review time: no stack merging,
no auto-dissolve of a one-member stack, no drag-and-drop in the panel list, no
touch support, and no change to how stacks are drawn — this slice adds gestures
to the existing rectangle, it does not redesign it.
