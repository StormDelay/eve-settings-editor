# Reaching a window under another one on the canvas (design)

Status: designed 2026-07-30.

Milestone context: the **layout depth** milestone, ledger item at
`docs/small-tasks.md:89`. That entry's premise is wrong; §1 records why, because
the correction is most of the design.

## 1. What the ledger entry got wrong

The entry (added 2026-07-27) says:

> Overlapping windows in `LayoutView` can only be selected topmost-first, so
> anything fully covered is unreachable — you have to drag the top window away
> and put it back.

The second half is false, and was already false when it was written:

- **`.win.selected { z-index: 1 }`** has been there since `0f814e0`
  (2026-07-15), the commit that added the canvas. A selected rectangle paints
  above every unselected one, so a covered window becomes visible the moment it
  is selected — label, border and all four resize handles.
- **The window list already selects onto the canvas.** `WindowPanel.svelte:211`
  calls `onSelect(w.id)` on a row click, which reaches `selectWindow` in
  `LayoutView.svelte:205`. `02c7d42` (2026-07-26) added an explicit
  **"Select on canvas"** item to the list's right-click menu.
- `scrollOnSelect` then reveals the row in the list, so the two stay in sync.

So a covered window is reachable today, and movable once reached — by name,
through the list. Nothing needs building for that.

**What is genuinely missing is discovery from the canvas.** `unitAt`
(`layout.ts`) walks the units in reverse paint order and returns the first hit,
so a click always lands on the topmost rectangle and there is no way to learn
what else is under the cursor without already knowing its name. With a real
file carrying hundreds of window entries and heavy overlap
(`format-notes.md`: one character had 381 windows, ~9 on screen), "already
knowing its name" is the part that fails.

## 2. Goal

Right-click a point on the canvas and get a list of every rectangle containing
it, topmost first. Pick one to select it.

Nothing else: selection, highlighting, scrolling the list to match and lifting
the pick above its neighbours are all existing behaviour that this reuses.

## 3. Why right-click rather than click-cycling

The ledger offered three routes. Right-click wins on three counts:

- **Discovery, not just traversal.** Cycling tells you what is next; a list
  tells you what is *there*. With 2-5 rectangles overlapping a point, seeing
  the names beats clicking through them and guessing when you have wrapped.
- **No collision with drag.** A left-click on the canvas is also how a drag
  starts (`startMove` on `pointerdown`). Making a repeat click mean "descend"
  would need to distinguish a click from a grab — `LayoutView.svelte:573-580`
  shows that distinction is already subtle enough to have earned a comment.
  Right-click is unused on the canvas.
- **It is nearly free.** `ContextMenu.svelte` already exists and is already
  used for exactly this shape of interaction by `WindowPanel`.

## 4. Plural hit-testing

`layout.ts` gains a generic collector beside `unitAt`:

```ts
export function rectsAt<T>(items: T[], rectOf: (t: T) => Rect, x: number, y: number): T[];
```

Returns every item whose rect contains the point, **topmost first** — the same
last-painted-wins ranking `unitAt` uses, because the canvas paints `units` in
array order.

`unitAt` keeps its own early-returning reverse walk rather than becoming
`rectsAt(...)[0]`: it runs on every `pointermove` for the duration of a drag,
and it should not allocate an array per move to answer a question it can answer
by returning early. What the two DO share is the point-in-rect test, which is
extracted to one local `hits(r, x, y)` so there is a single predicate rather
than two copies drifting apart. A test pins that `unitAt` agrees with
`rectsAt(...)[0]`.

Generic over `T` because it serves two callers with different element types —
draw units (via their anchor's rect) and furniture rects (which are their own
rect). That is two real callers on day one, not speculative flexibility.

## 5. The menu

An `oncontextmenu` handler on the canvas element in `LayoutView.svelte`:

1. `preventDefault()`, then convert to data px with the existing
   `pointerData(e)` (`LayoutView.svelte:329`).
2. Windows: `rectsAt(units, (u) => rectOf(u.anchor), p.x, p.y)`.
3. Furniture: `rectsAt(hudRects(...), (f) => f, p.x, p.y)`, listed **after** the
   windows — furniture always paints beneath them, so that order is honest.
4. Nothing under the cursor: no menu at all.
5. Otherwise store `{ x: e.clientX, y: e.clientY, items }` and render the
   existing `<ContextMenu>`, which positions in client px and handles its own
   dismissal and edge-flipping.

A single rectangle under the cursor still opens a one-item menu. Predictable
beats clever, and "nothing else here" is information — a right-click that
sometimes shows a menu and sometimes silently selects is worse.

**Labels reuse the canvas's own**, so the menu cannot disagree with what is
drawn or with the list:

- free window — `displayNameOf(unit.anchor)`
- stack — `stackLabel(s) ?? displayName(s.container_id)`, the idiom
  `WindowPanel.svelte:285` already uses
- furniture — its `FurnitureRect.label` ("Neocom", "Ship HUD", …)

Picking an item calls `selectWindow(id)` or `selectFurniture(kind)`. Both
already exist and already clear each other — `LayoutView.svelte:204` documents
that the canvas shows one selection, not two.

## 6. Scope

Out, and why:

- **Click-cycling.** §3. The menu covers the need and the modifier-free variant
  fights the drag gesture.
- **Keyboard traversal of the stack under the cursor.** No evidence anyone
  wants it; the list is already keyboard-reachable.
- **Menu actions beyond selecting** (copy id, toggle open, delete). The list's
  own context menu already has those, and the pick lands there via
  `scrollOnSelect`. Adding a second place to do them is duplication.
- **Changing what the panel's right-click menu offers.** Untouched.

Also in scope, both small: correct `ContextMenu.svelte`'s "the panel is the
only caller" comment, and rewrite the `small-tasks.md` entry rather than
ticking it — a ledger that records a false premise as a completed finding
misleads whoever reads it next.

## 7. Testing

`layout.test.ts`, pure and throw-based like its neighbours:

1. `rectsAt` returns every unit containing the point, topmost first.
2. It returns `[]` for a point outside everything.
3. A point inside one rectangle but outside an overlapping neighbour returns
   only the one — the test that fails if `hits` is written with the comparison
   inverted on an edge.
4. `unitAt(...)` equals `rectsAt(...)[0]` for a point over a stack of
   rectangles. This is the one that matters: it pins the two walks to one
   ranking, so a future change to either cannot silently make the menu's
   "topmost" differ from the one a click selects.

No component test for the menu markup. `WindowPanel`'s existing context menu
has none either, and the logic worth testing — which rectangles are under a
point, in what order — is all in `rectsAt`.
