# Layout depth — precision editing (design)

Status: designed 2026-07-26, not yet planned.

Milestone context: the **layout depth** milestone, cut into four slices (see the
HUD furniture spec §7). Slice 3 (HUD furniture) shipped as v0.15.0; slice 1a
(canvas & list usability) shipped as v0.16.0. This spec covers **slice 1b** —
the precision half of slice 1, split out of the 1a spec because the two halves
share no code: 1a touched labels, folding and filtering; this one touches the
drag handlers and `layout.ts` geometry.

## 1. Goal

Placing a window on the canvas today is freehand. A drag writes whatever data px
the pointer lands on, so getting two windows flush against each other, or a
window flush against the screen edge, is trial and error at a canvas scale that
is usually well under 1:1 — at a 1920-wide reference in a ~900px canvas, one
canvas pixel is more than two data pixels, and a one-pixel hand tremor is a
two-pixel gap the player sees in-game.

Two mechanisms fix that, and they are complementary:

- **Snapping** — while dragging, edges that come close to another window's edge,
  a screen-furniture edge, or the screen edge lock onto it exactly.
- **Nudging** — with a window selected, arrow keys move it one data pixel at a
  time, with no pointer involved at all.

## 2. Scope

In scope:

1. Edge snapping on move and on corner resize, with Alt held to disable.
2. Guide lines showing which edge a snap caught.
3. Arrow-key nudge of the selected window, previewed live and committed on key
   release.

Out of scope, and why:

- **Grid snap.** The HUD furniture spec's slice list named snap-to-grid
  alongside edge snapping. Dropped by the project owner during this design:
  EVE itself has no layout grid, and a fixed step does not get windows flush
  against each other or the screen unless their sizes happen to be multiples of
  it — which real EVE window sizes are not. The canvas's 40px background lines
  stay decorative. The Alt modifier that grid snap was going to need now
  disables edge snapping instead.
- **Snapping or nudging screen furniture** (ship HUD, fighter UI, badge). Their
  rectangles are *assumed* geometry — `HUD_NOMINAL` in `layout.ts` carries
  nominal sizes and a guessed point convention, still unconfirmed in-game (see
  the HUD furniture spec). Snapping a window *to* those edges is fine, since the
  player sees the same rectangle we do and a snap is only ever an offer.
  Snapping the guessed rectangle *itself* to a precise value is false precision,
  and nudging it would need a second debounced commit path through `setHud`.
- **Multi-select, alignment or distribution commands.** Selection stays one
  window at a time, as in every slice so far.
- **Any backend or wire-format change.** Snapping and nudging both end in the
  same `geomMutations` calls a drag already makes. No new Rust.

## 3. Snapping — `app/src/lib/layout.ts` (two new pure functions)

Same shape as the file's existing helpers (`resizeRect`, `stackUnits`): no DOM,
no Svelte, unit-tested in `layout.test.ts`.

```ts
export interface Rect { x: number; y: number; w: number; h: number }

/** Candidate edge coordinates, in data px, split by axis. */
export interface SnapLines { x: number[]; y: number[] }

export function snapLines(rects: Rect[], referenceW: number, referenceH: number): SnapLines;

/** The edges a drag actually moves: all four for a move, two for a resize. */
export function movingEdges(r: Rect, corner: Corner | null): { x: number[]; y: number[] };

/** The correction to add to a raw drag delta, plus the lines it caught. */
export interface SnapResult { dx: number; dy: number; gx: number | null; gy: number | null }

export function snapDelta(
  moving: { x: number[]; y: number[] },
  lines: SnapLines,
  tol: number,
): SnapResult;
```

**Candidates (`snapLines`).** Collected once, at drag start:

- every rect contributes `x` and `x + w` to `lines.x`, and `y` and `y + h` to
  `lines.y`;
- the canvas contributes `0` and `referenceW` to `lines.x`, `0` and
  `referenceH` to `lines.y`.

The caller assembles the rect list: the *displayed* rect of every draw unit
(`rectOf`, so a rect still showing a preview contributes where it is seen, not
where it was committed) plus every furniture rect, minus the dragged unit's own
ids — its `fanTargets` — so a window never snaps to itself or to a stack sibling
it is carrying along. Taking plain rects rather than `DrawUnit[]` keeps the
function honest about what it needs and keeps its tests free of stack fixtures.

Units, not the raw window list: what the canvas *draws* is what snaps, so the
1a filter already applies. Snapping to a window that isn't on screen would be
inexplicable, and the neocom's inner edge — the one furniture line that marks a
real screen boundary — is exactly the line a player wants to align against.

Duplicate coordinates are not deduplicated; on a real file the candidate arrays
run to a few hundred numbers and the search below is a linear scan per move,
which is nothing next to the DOM update the same move triggers.

**The search (`snapDelta`).** `moving` — built by `movingEdges` from the
raw-displaced rect — holds the edges the drag actually moves, in data px:

- a **move** (`corner: null`) passes all four: `x: [x, x + w]`, `y: [y, y + h]`;
- a **corner resize** passes only the two edges that corner moves — `tl` gives
  `x: [x]`, `y: [y]`; `br` gives `x: [x + w]`, `y: [y + h]`; and so on.

Per axis, every moving edge is tested against every candidate. The smallest
absolute distance within `tol` wins and its signed difference becomes the
correction; ties go to the lower candidate coordinate, so the result is
deterministic and testable. No candidate within `tol` yields `0` and a `null`
guide — the drag is untouched, which is also what an empty canvas gives.

**Tolerance.** `toData(6, scale)` — six *canvas* pixels expressed in data px, so
the grab feels the same regardless of how far the canvas is scaled down.

**Applying it** in `LayoutView.onPointerMove`, after the raw delta is computed
and before the preview is written. The `SnapLines` are computed in `startMove`
and `startResize` and carried on the `Drag` object, next to the origin
coordinates already stored there — one collection pass per drag, not per move,
and the candidate set stays fixed for the duration of the drag even though the
dragged rectangle is moving through it:

- **move**: `x = ox + dx + corr.dx`, `y = oy + dy + corr.dy`;
- **resize**: `resizeRect(orig, corner, dx + corr.dx, dy + corr.dy)` — the
  correction goes into the delta, so `resizeRect` and its anchor-crossing
  guards are untouched.

**Alt disables it.** Read as `e.altKey` off the pointer event, so pressing or
releasing Alt mid-drag takes effect on the next pointer move; no key listeners,
no state. When Alt is down the correction is skipped and the guides clear.

Alt was chosen over Shift (which is the natural big-step nudge modifier in §5,
and the conventional axis-constraint key if that is ever wanted) and over Ctrl
(the app already binds Ctrl+S and Ctrl+F globally on `svelte:window`, and those
fire during a drag).

## 4. Guides — `LayoutView.svelte`

One piece of state, `guides = $state<{ x: number | null; y: number | null }>`,
set from `snapDelta`'s `gx`/`gy` on every move and cleared on pointerup and
whenever a move produces no snap. Each non-null value renders as a 1px
absolutely-positioned line spanning the canvas at `toCanvas(value, scale)`, in
the same amber as `.win.selected`, `pointer-events: none`.

At most two lines exist at a time — one per axis. Without them a snap reads as
the drag stuttering, and with ~68 rectangles on a real canvas there is no way to
tell which neighbour was caught.

## 5. Nudge — `LayoutView.svelte`

A `svelte:window` `onkeydown`/`onkeyup` pair inside `LayoutView`, rather than a
`tabindex` on the canvas: the selection can be made from the window panel just
as well as from the canvas, and a focus-scoped handler would silently do nothing
in that case.

Guards, in order: not `readOnly`; `selectedId` is set and resolves to a drawn
unit; no drag in progress; the event target is not an `input`, `select` or
`textarea` (so the 1a filter box and the panel's own x/y/w/h number inputs keep
their native arrow behaviour); the key is one of the four arrows; no Ctrl/Meta.

- **Arrow** moves the selected unit by 1 data px; **Shift+arrow** by 10.
- Each keydown updates `preview` only — the same state a drag writes, so the
  rectangle and the panel's numbers both follow live, and nothing reaches the
  backend.
- **Keyup commits**, through the drag's existing drop path: fan the previewed
  rect out to `fanTargets` with `geomMutations`, `commit`, then clear the
  preview unless a new nudge or drag has claimed it in the meantime (the guard
  `onPointerUp` already implements).

Key auto-repeat is what makes this cheap: holding an arrow fires dozens of
keydowns and exactly one keyup, so a glide across the screen costs one
round-trip and one document rewrite. A tap costs the same round-trip as clicking
the panel's number spinner once, which is today's cost for the same edit.

`e.preventDefault()` on a handled arrow, so the canvas pane doesn't scroll under
the nudge.

**No snapping during a nudge.** Nudging is the deliberate-precision tool — the
thing you reach for when snapping put the window one pixel off where you wanted
it. Snapping it back would make the two features fight.

## 6. Testing

- `layout.test.ts` (node `--test`, zero-dep) gains:
  - `snapLines`: canvas edges always present even with no rects; each rect
    contributes exactly its two edges per axis.
  - `movingEdges`: all four edges for a move; exactly the two edges each of the
    four corners moves.
  - `snapDelta`: nearest candidate wins; a candidate outside `tol` is a no-op
    with `null` guides; both moving edges are tested, so a rect snaps by its
    right edge as readily as its left; the tie rule picks the lower coordinate;
    a single-edge (resize) `moving` set never corrects the axis it doesn't move.
  - The composition case that catches sign errors: a rect dragged to `x = 98`
    with a candidate at `100` lands at exactly `100`, not `102`.
- No `LayoutView.spec.ts`. Driving pointer capture and key auto-repeat through
  jsdom is a known trap in this repo, and every branch worth testing is in the
  pure functions above.
- No Rust changes, so no new backend tests.
- **Live smoke**, as every slice: on a real character, drag a window against
  another window's edge and against each screen edge and confirm the resulting
  numbers are exactly equal in the panel; hold Alt and confirm the same drag
  passes straight through; resize by each of the four corners onto a neighbour's
  edge; nudge with the arrows and with Shift held, watching that one held key
  produces one save; nudge a stacked window and confirm every member follows;
  and confirm the arrows still step the panel's number inputs when one is
  focused.

## 7. Non-goals

- Snapping to window *centres* or to equal-spacing hints. Edges are what EVE's
  own window snapping matches and what a player aligns by eye.
- Persisting a "snapping off" preference. Alt is held, not toggled — there is no
  mode to get stuck in, and nothing to store.
- Snapping during a nudge, and nudging furniture (§2, §5).
- Undo. The app's existing save/restore-backup model is the undo story, and this
  slice adds no new kind of edit — a nudge writes the same geometry scalars a
  drag does.
