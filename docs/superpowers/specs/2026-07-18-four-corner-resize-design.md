# Layout canvas — four-corner resize (design)

Date: 2026-07-18
Status: approved design, ready to plan.
Builds on: M2 layout canvas (`app/src/lib/LayoutView.svelte`, `WindowPanel.svelte`,
`app/src/lib/layout.ts`).
Independent of the codec foundation and the window-stacks milestone — ships on
its own. Its corner-resize math is what the later coherent stack-resize reuses
(window-stacks spec §9).

## 1. Goal

In the layout canvas a selected window resizes only from the bottom-right
handle today; the top-left corner is a fixed anchor. Add handles on all four
corners so a window can be resized from any corner, with the *opposite* corner
staying fixed. Edge handles are out of scope (YAGNI — the stack-resize only
needs corners; add edges later if a real need appears).

## 2. Scope

- Four corner handles: top-left, top-right, bottom-left, bottom-right.
- Handles render **only on the selected window** (`w.id === selectedId`), not on
  every open rectangle. Selecting a window then reveals its handles; today's
  always-visible single handle is replaced by this.
- No backend change. `geomMutations` in `LayoutView.svelte` already accepts all
  of `{x, y, w, h}`, diffs each field (unchanged fields emit no mutation), and
  aligns the window's saved resolution to the reference on any geometry change.
  Corner resize reuses it unchanged.

## 3. The rect transform (pure helper)

The corner math is extracted into one pure, testable function — this is the
piece the future coherent stack-resize reuses, so it must stand alone:

```
resizeRect(orig: {x, y, w, h}, corner: Corner, dx: number, dy: number)
  -> {x, y, w, h}
```

`dx`/`dy` are the pointer delta in **data px** (already un-scaled by the caller,
as the current resize branch does via `toData`). For each corner the opposite
corner is the fixed anchor:

| Corner grabbed | Anchor (fixed) | x      | y      | w        | h        |
|----------------|----------------|--------|--------|----------|----------|
| BR (today)     | TL             | —      | —      | ow + dx  | oh + dy  |
| TL             | BR             | ox+dx  | oy+dy  | ow − dx  | oh − dy  |
| TR             | BL             | —      | oy+dy  | ow + dx  | oh − dy  |
| BL             | TR             | ox+dx  | —      | ow − dx  | oh + dy  |

(“—” = unchanged from the original.)

**Clamp (anchor-relative).** A dragged corner must not cross its anchor. When a
left or top edge moves, floor its size at the minimum and pin the position to
`anchor − size` so the anchor edge stays put:

- Horizontal, left edge moving: `anchor_r = ox + ow`;
  `new_x = min(ox + dx, anchor_r − MIN)`; `new_w = anchor_r − new_x`.
- Horizontal, right edge moving: `new_w = max(MIN, ow + dx)`; `new_x = ox`.
- Vertical is the same with `y`/`h` and `anchor_b = oy + oh`.

`MIN = 0`, matching today's `Math.max(0, …)` behavior exactly (the current single
handle already allows shrinking a window to nothing). Not a new constraint.

## 4. Wiring (`LayoutView.svelte`)

- `Drag`'s `resize` variant gains `corner: Corner` and records `ox, oy, ow, oh`
  (today it keeps only `ow, oh`).
- `startResize(w, corner, e)` takes the corner it was grabbed from.
- `onPointerMove`'s resize branch calls `resizeRect(orig, drag.corner, dx, dy)`
  instead of the inline `w/h` math, and stores the full `{x,y,w,h}` in `preview`.
- `onPointerUp` commits `geomMutations(w, {x, y, w, h})` (today only `{w, h}`).
  Unchanged fields diff to nothing, so BR still emits just `w/h`.
- Markup: replace the single `.resize` span with four spans, rendered only when
  the window is selected, each tagged with its corner and cursor
  (`nwse-resize` for TL/BR, `nesw-resize` for TR/BL). Each handle stops
  propagation so grabbing it starts a resize, not a move (as the single handle
  does today).

The `Corner` type and `resizeRect` live in `app/src/lib/layout.ts` alongside the
existing `toCanvas`/`toData`/`openWindows` helpers, so the transform is import-
able by both the canvas and the later stack-resize.

## 5. Testing

- **Frontend (`node --test`, zero-dep):** one test file over `resizeRect` —
  for each of the four corners the anchor corner stays fixed under a delta, and
  the anchor-crossing clamp holds (a delta larger than the original size floors
  the size at `MIN` and keeps the anchor edge in place). Pure logic; matches the
  M2 testing norm.
- DOM drag/pointer handling is not unit-tested (consistent with M2).
- **Manual smoke (live, project norm):** resize a real window from each corner in
  the app, save, reopen, confirm the geometry landed as drawn.

## 6. Out of scope / deferred

- Edge (non-corner) handles.
- Any backend or codec change.
- Coherent stack resize — a separate milestone that will call `resizeRect` on the
  stack's anchor rect and fan the result to open members.
