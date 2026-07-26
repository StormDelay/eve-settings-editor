# Layout precision editing (slice 1b) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the layout canvas precise — dragging and corner-resizing snap to
other windows', furniture's and the screen's edges (Alt to disable, with guide
lines showing what was caught), and arrow keys nudge the selected window one
data pixel at a time.

**Architecture:** Three new pure functions in `app/src/lib/layout.ts`
(`snapLines`, `movingEdges`, `snapDelta`) do all the arithmetic and carry all
the tests. `app/src/lib/LayoutView.svelte` computes the candidate lines once per
drag, stores them on its existing `Drag` object, applies the correction inside
`onPointerMove` before writing the preview, and renders at most two guide lines
from the result. Nudge is a `svelte:window` key pair in the same component that
writes the same `preview` state and commits through the same fan-out path a
drag drop already uses. No Rust, no wire-format change.

**Tech Stack:** SvelteKit 5 (runes) + TypeScript, Tauri 2 shell, `node --test`
for the pure frontend tests (zero-dep, throw-based), `vitest` +
`@testing-library/svelte` for the `.spec.ts` component tests (not used here).

**Spec:** `docs/superpowers/specs/2026-07-26-layout-precision-editing-design.md`

## Global Constraints

- **No new dependencies.** Frontend and backend both. Nothing in this slice
  needs one.
- **No Rust and no wire-format change.** Snapping and nudging end in the same
  `geomMutations` calls a drag already makes.
- **Pure geometry lives in `layout.ts`.** No DOM, no Svelte, no imports from
  `.svelte` files — that is what makes it testable under `node --test`.
- **Tests are throw-based, framework-free**, matching the existing
  `layout.test.ts`: a local `check(name, ok)` that throws on failure. Do not
  introduce `describe`/`it`.
- **All commands run from `app/`**, in **PowerShell** — `npm` is not on the Bash
  tool's PATH in this environment.
- **Commit style:** sentence-case subject, imperative, **no attribution
  trailers** (no `Co-Authored-By`, no `Generated with`). Match
  `git log --oneline`.
- **Units are data px** everywhere in `layout.ts`. The only place canvas px
  appear is the tolerance, converted with `toData(6, scale)` at the call site.

---

## File Structure

- `app/src/lib/layout.ts` — **modify.** Gains `Rect`, `SnapLines`,
  `SnapResult`, and the three pure functions, appended as a new
  `// --- snapping ---` section after the filtering section at the end of the
  file. Nothing existing changes.
- `app/src/lib/layout.test.ts` — **modify.** Gains a `// --- snapping ---`
  block at the end. Nothing existing changes.
- `app/src/lib/LayoutView.svelte` — **modify.** Drag state gains the candidate
  lines; `onPointerMove` applies the correction; new `guides` state and two
  guide `<div>`s; new `svelte:window` key handlers and the nudge commit.
- `docs/small-tasks.md` — **modify** only if the implementation defers
  something. Not expected.

---

## Task 1: Pure snap geometry in `layout.ts`

**Files:**
- Modify: `app/src/lib/layout.ts` (append after the filtering section, which
  currently ends the file at line 272)
- Test: `app/src/lib/layout.test.ts` (append at the end, after the last
  `hudPointFromRect` check)

**Interfaces:**
- Consumes: `Corner` and `toData`, both already exported from `layout.ts`.
- Produces, for Task 2:
  - `export interface Rect { x: number; y: number; w: number; h: number }`
  - `export interface SnapLines { x: number[]; y: number[] }`
  - `export interface SnapResult { dx: number; dy: number; gx: number | null; gy: number | null }`
  - `export function snapLines(rects: Rect[], referenceW: number, referenceH: number): SnapLines`
  - `export function movingEdges(r: Rect, corner: Corner | null): { x: number[]; y: number[] }`
  - `export function snapDelta(moving: { x: number[]; y: number[] }, lines: SnapLines, tol: number): SnapResult`

- [ ] **Step 1: Write the failing tests**

Append to `app/src/lib/layout.test.ts`. Add the four new names to the existing
`import { … } from "./layout.ts";` list at the top of the file (`snapLines`,
`movingEdges`, `snapDelta`, and the type-only ones are not needed at runtime).

```ts
// --- snapping: candidate lines ---------------------------------------------
{
  // The canvas edges are candidates even when nothing is drawn.
  const empty = snapLines([], 2560, 1440);
  check("snapLines always offers the canvas x edges", empty.x.join(",") === "0,2560");
  check("snapLines always offers the canvas y edges", empty.y.join(",") === "0,1440");

  // Each rect contributes exactly its two edges per axis.
  const lines = snapLines([{ x: 100, y: 50, w: 200, h: 80 }], 2560, 1440);
  check("a rect contributes its left and right edges", lines.x.includes(100) && lines.x.includes(300));
  check("a rect contributes its top and bottom edges", lines.y.includes(50) && lines.y.includes(130));
  check("a rect adds exactly two x candidates", lines.x.length === 4);
  check("a rect adds exactly two y candidates", lines.y.length === 4);
}

// --- snapping: which edges a drag moves -------------------------------------
{
  const r = { x: 100, y: 50, w: 200, h: 80 }; // right = 300, bottom = 130

  const move = movingEdges(r, null);
  check("a move tests both x edges", move.x.join(",") === "100,300");
  check("a move tests both y edges", move.y.join(",") === "50,130");

  // Each corner moves exactly one edge per axis: the one it is named for.
  check("tl moves left and top", movingEdges(r, "tl").x.join(",") === "100" && movingEdges(r, "tl").y.join(",") === "50");
  check("tr moves right and top", movingEdges(r, "tr").x.join(",") === "300" && movingEdges(r, "tr").y.join(",") === "50");
  check("bl moves left and bottom", movingEdges(r, "bl").x.join(",") === "100" && movingEdges(r, "bl").y.join(",") === "130");
  check("br moves right and bottom", movingEdges(r, "br").x.join(",") === "300" && movingEdges(r, "br").y.join(",") === "130");
}

// --- snapping: the search ---------------------------------------------------
{
  const lines = { x: [0, 100, 500, 2560], y: [0, 200, 1440] };

  // The correction is what CLOSES the gap: an edge at 98 with a candidate at
  // 100 corrects by +2 and lands on 100 — not 102. This is the sign test.
  const near = snapDelta({ x: [98], y: [] }, lines, 6);
  check("a near edge corrects toward the candidate", near.dx === 2);
  check("the caught candidate is reported as the guide", near.gx === 100);
  check("an axis with no moving edge does not move", near.dy === 0 && near.gy === null);

  // Outside the tolerance nothing happens at all.
  const far = snapDelta({ x: [90], y: [] }, lines, 6);
  check("an edge outside the tolerance is untouched", far.dx === 0 && far.gx === null);

  // The tolerance is inclusive at the boundary.
  const edge = snapDelta({ x: [94], y: [] }, lines, 6);
  check("an edge exactly at the tolerance still snaps", edge.dx === 6 && edge.gx === 100);

  // A rect's RIGHT edge snaps as readily as its left: the rect spans 400..502,
  // so it is the trailing edge that is 2px from the candidate at 500.
  const byRight = snapDelta({ x: [400, 502], y: [] }, lines, 6);
  check("the right edge can win the snap", byRight.dx === -2 && byRight.gx === 500);

  // Nearest wins when several candidates are in range.
  const nearest = snapDelta({ x: [102], y: [] }, { x: [100, 104], y: [] }, 6);
  check("the nearest candidate wins", nearest.dx === -2 && nearest.gx === 100);

  // Ties go to the LOWER candidate, so the result never depends on array order.
  const tie = snapDelta({ x: [102], y: [] }, { x: [104, 100], y: [] }, 6);
  check("a tie goes to the lower candidate", tie.gx === 100 && tie.dx === -2);

  // Both axes resolve independently in one call.
  const both = snapDelta({ x: [3], y: [198] }, lines, 6);
  check("both axes snap in one call", both.dx === -3 && both.dy === 2);
  check("both guides are reported", both.gx === 0 && both.gy === 200);

  // No candidates at all (an empty canvas) is a clean no-op, not a crash.
  const none = snapDelta({ x: [50], y: [50] }, { x: [], y: [] }, 6);
  check("no candidates is a no-op", none.dx === 0 && none.dy === 0 && none.gx === null && none.gy === null);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run (PowerShell, from `app/`):

```powershell
node --test src/lib/layout.test.ts
```

Expected: FAIL — `SyntaxError: The requested module './layout.ts' does not
provide an export named 'snapLines'`.

- [ ] **Step 3: Write the implementation**

Append to `app/src/lib/layout.ts`:

```ts
// --- snapping ---------------------------------------------------------------
// Edge snapping, in data px. EVE has no layout grid; it snaps windows to each
// other and to the screen, so these candidates are edges, never a fixed step.

export interface Rect { x: number; y: number; w: number; h: number }

/** Candidate edge coordinates a drag can lock onto, split by axis. */
export interface SnapLines { x: number[]; y: number[] }

/**
 * Every edge worth snapping to: the four edges of each rect, plus the screen's
 * own. The caller decides what is in `rects` — what the canvas DRAWS (so the
 * filter already applies), minus the dragged unit's own windows, plus the
 * furniture. Duplicates are kept: a few hundred numbers scanned linearly per
 * pointer move is nothing next to the DOM update that move triggers.
 */
export function snapLines(rects: Rect[], referenceW: number, referenceH: number): SnapLines {
  const x = [0, referenceW];
  const y = [0, referenceH];
  for (const r of rects) {
    x.push(r.x, r.x + r.w);
    y.push(r.y, r.y + r.h);
  }
  return { x, y };
}

/**
 * The edges a drag actually moves. A move carries all four; a corner resize
 * moves only the two edges its name points at (the opposite corner is the fixed
 * anchor — see resizeRect), and snapping an edge that isn't moving would drag
 * the anchor along with it.
 */
export function movingEdges(r: Rect, corner: Corner | null): { x: number[]; y: number[] } {
  if (corner === null) return { x: [r.x, r.x + r.w], y: [r.y, r.y + r.h] };
  const left = corner === "tl" || corner === "bl";
  const top = corner === "tl" || corner === "tr";
  return { x: [left ? r.x : r.x + r.w], y: [top ? r.y : r.y + r.h] };
}

/** A correction to add to a drag's delta, plus the candidates it caught. */
export interface SnapResult { dx: number; dy: number; gx: number | null; gy: number | null }

/** Nearest candidate within `tol` wins; ties go to the lower coordinate, so the
 * outcome never depends on the order rects were collected in. */
function nearest(edges: number[], lines: number[], tol: number): { d: number; line: number | null } {
  let d = 0;
  let line: number | null = null;
  let best = Infinity;
  for (const e of edges) {
    for (const c of lines) {
      const diff = c - e;
      const dist = Math.abs(diff);
      if (dist > tol) continue;
      if (dist < best || (dist === best && line !== null && c < line)) {
        best = dist;
        d = diff;
        line = c;
      }
    }
  }
  return { d, line };
}

/**
 * Snap a drag. `moving` is the edges that move, already displaced by the raw
 * pointer delta; the returned dx/dy is the extra correction that lands them on
 * a candidate, and gx/gy the lines caught (null when nothing was in range, in
 * which case the drag passes through untouched).
 */
export function snapDelta(
  moving: { x: number[]; y: number[] },
  lines: SnapLines,
  tol: number,
): SnapResult {
  const x = nearest(moving.x, lines.x, tol);
  const y = nearest(moving.y, lines.y, tol);
  return { dx: x.d, dy: y.d, gx: x.line, gy: y.line };
}
```

- [ ] **Step 4: Run the tests to verify they pass**

```powershell
node --test src/lib/layout.test.ts
```

Expected: PASS, ending in `layout: all checks passed`.

- [ ] **Step 5: Typecheck**

```powershell
npm run check
```

Expected: `svelte-check found 0 errors`.

- [ ] **Step 6: Commit**

```powershell
git add app/src/lib/layout.ts app/src/lib/layout.test.ts
git commit -m "Add edge-snapping geometry to the layout helpers"
```

---

## Task 2: Snap the canvas drag and resize, with guide lines

**Files:**
- Modify: `app/src/lib/LayoutView.svelte` — the import block (lines 4-8), the
  `Drag` type and `startMove`/`startResize`/`startFurniture` (lines 232-282),
  `onPointerMove` (lines 284-304), `onPointerUp` (line 312), the canvas markup
  (around line 404), and the `<style>` block.
- Test: none — see the note below.

**Interfaces:**
- Consumes: `snapLines`, `movingEdges`, `snapDelta`, `Rect`, `SnapLines` from
  Task 1; the component's existing `rectOf`, `fRectOf`, `units`, `furniture`,
  `scale`, `toData`, `toCanvas`, `resizeRect`.
- Produces, for Task 3: nothing new — Task 3 uses `preview` and `commitUnit`,
  the latter extracted in this task's Step 5.

**Why no unit test here:** every branch worth testing is in Task 1's pure
functions. This component has no `.spec.ts` and driving pointer capture through
jsdom is a known trap in this repo (see the test-architecture notes). The gates
for this task are `npm run check`, `npm run build`, and the live smoke in
Task 4.

- [ ] **Step 1: Import the new helpers**

In `app/src/lib/LayoutView.svelte`, extend the existing `$lib/layout` import:

```ts
  import {
    canvasScale, toCanvas, toData, resizeRect, stackUnits, hudRects, shipOffsetFromX,
    hudPointFromRect, NO_FILTER, filterIsActive, visibleIds, drawnWindowCount,
    snapLines, movingEdges, snapDelta,
    type Corner, type DrawUnit, type FurnitureRect, type WindowFilter, type SnapLines,
  } from "$lib/layout";
```

- [ ] **Step 2: Carry the candidate lines on the drag, and add the guide state**

Replace the `Drag` type and add the two new pieces of state beneath it:

```ts
  type Drag =
    | { kind: "move"; unit: DrawUnit; startX: number; startY: number; ox: number; oy: number; lines: SnapLines }
    | { kind: "resize"; unit: DrawUnit; corner: Corner; startX: number; startY: number; ox: number; oy: number; ow: number; oh: number; lines: SnapLines }
    | { kind: "furniture"; f: FurnitureRect; startX: number; startY: number; ox: number; oy: number };
  let drag: Drag | null = null;

  // The lines the current drag has locked onto, in data px; null when this axis
  // isn't snapped. Drawn as guides, cleared on drop.
  let guides = $state<{ x: number | null; y: number | null }>({ x: null, y: null });
```

Note that the `furniture` variant deliberately has no `lines`: furniture
geometry is assumed (`HUD_NOMINAL`), so it is a snap *source* but never a snap
*target* — see the spec's §2.

Add the collector, next to `fRectOf`:

```ts
  /** Candidate edges for a drag of `unit`: every rect the canvas currently
   * draws except the dragged unit's own windows, plus the furniture, plus the
   * screen. Displayed rects (rectOf), so a neighbour still showing a preview
   * offers the edge the player can see. Collected once per drag — the set stays
   * fixed while the dragged rect moves through it. */
  function linesFor(unit: DrawUnit): SnapLines {
    const mine = new Set(unit.fanTargets.map((w) => w.id));
    const rects = units
      .filter((u) => !mine.has(u.anchor.id))
      .map((u) => rectOf(u.anchor));
    for (const f of furniture) rects.push({ ...fRectOf(f), w: f.w, h: f.h });
    return snapLines(rects, layout?.reference_w ?? 0, layout?.reference_h ?? 0);
  }
```

- [ ] **Step 3: Populate `lines` at drag start**

In `startMove`, the assignment becomes:

```ts
    drag = { kind: "move", unit, startX: e.clientX, startY: e.clientY, ox: r.x, oy: r.y, lines: linesFor(unit) };
```

In `startResize`:

```ts
    drag = {
      kind: "resize", unit, corner, startX: e.clientX, startY: e.clientY,
      ox: r.x, oy: r.y, ow: r.w, oh: r.h, lines: linesFor(unit),
    };
```

- [ ] **Step 4: Apply the correction in `onPointerMove`**

Replace the body of `onPointerMove` with:

```ts
  function onPointerMove(e: PointerEvent) {
    if (!drag) return;
    const dx = toData(e.clientX - drag.startX, scale);
    const dy = toData(e.clientY - drag.startY, scale);
    if (drag.kind === "furniture") {
      const f = drag.f;
      fPreview = {
        ...fPreview,
        [f.kind]: { x: drag.ox + dx, y: f.drag === "xy" ? drag.oy + dy : drag.oy },
      };
      return;
    }
    // Six CANVAS px, so the grab feels identical however far the canvas is
    // scaled down. Alt held passes the drag straight through — read off the
    // event, so pressing or releasing it mid-drag takes effect immediately.
    const tol = toData(6, scale);
    const raw = drag.kind === "move"
      ? { ...rectOf(drag.unit.anchor), x: drag.ox + dx, y: drag.oy + dy }
      : resizeRect({ x: drag.ox, y: drag.oy, w: drag.ow, h: drag.oh }, drag.corner, dx, dy);
    const corner = drag.kind === "resize" ? drag.corner : null;
    const snap = e.altKey
      ? { dx: 0, dy: 0, gx: null, gy: null }
      : snapDelta(movingEdges(raw, corner), drag.lines, tol);
    guides = { x: snap.gx, y: snap.gy };
    preview = {
      ...preview,
      [drag.unit.anchor.id]: drag.kind === "move"
        ? { ...raw, x: raw.x + snap.dx, y: raw.y + snap.dy }
        // The correction goes into the DELTA, so resizeRect's anchor-crossing
        // guards still run on the final numbers.
        : resizeRect({ x: drag.ox, y: drag.oy, w: drag.ow, h: drag.oh }, drag.corner, dx + snap.dx, dy + snap.dy),
    };
  }
```

- [ ] **Step 5: Clear the guides on drop, and extract the commit path**

At the top of `onPointerUp`, right after `drag = null;`, add:

```ts
    guides = { x: null, y: null };
```

Then replace the tail of `onPointerUp` — from `const p = preview[...]` to the
end of the function — with a call to a new helper, so Task 3's nudge can commit
through exactly the same path:

```ts
    await commitUnit(d.unit);
  }

  /** Commit a unit's previewed rect: fan it out to every renderable window in
   * the unit so a stack moves/resizes coherently and stale members are
   * repaired, then drop the preview unless something has re-claimed it. The
   * full rect (not just x/y) is sent even for a move, so members also snap to
   * the anchor's w/h — geomMutations diffs per field, so an unchanged w/h emits
   * nothing and plain single-window units are unaffected. */
  async function commitUnit(unit: DrawUnit) {
    const p = preview[unit.anchor.id];
    if (!p) return;
    const next = { x: p.x, y: p.y, w: p.w, h: p.h };
    await commit(unit.fanTargets.flatMap((w) => geomMutations(w, next)));
    // A re-drag or a fresh nudge on the same window may have started during the
    // async commit and now owns the preview — don't wipe it out from under it.
    // (The cast: TS narrowed `drag` to null in onPointerUp and can't see a
    // reassignment a concurrent startMove may have made across the await.)
    const active = drag as Drag | null;
    const dragging = active && active.kind !== "furniture" && active.unit.anchor.id === unit.anchor.id;
    if (!dragging && nudging !== unit.anchor.id) clearPreview(unit.anchor.id);
  }
```

`nudging` does not exist yet — Task 3 declares it. To keep this task's build
green on its own, declare it now, next to `guides`:

```ts
  // The window id a key-repeat nudge is currently in flight for (Task 3), so a
  // commit landing mid-nudge doesn't clear the preview under it.
  let nudging: string | null = null;
```

- [ ] **Step 6: Draw the guides**

In the canvas markup, immediately after the opening `<div class="canvas" …>`
element's `>` and before the `{#each furniture …}` block:

```svelte
        {#if guides.x !== null}
          <div class="guide vertical" style="left: {toCanvas(guides.x, scale)}px;"></div>
        {/if}
        {#if guides.y !== null}
          <div class="guide horizontal" style="top: {toCanvas(guides.y, scale)}px;"></div>
        {/if}
```

And in `<style>`, after the `.win.stacked` rule:

```css
  /* Snap feedback: the edge the dragged rect locked onto. Same amber as a
     selection, above every rect, never in the way of the pointer. */
  .guide {
    position: absolute;
    background: #f59e0b;
    pointer-events: none;
    z-index: 2;
  }
  .guide.vertical {
    top: 0;
    bottom: 0;
    width: 1px;
  }
  .guide.horizontal {
    left: 0;
    right: 0;
    height: 1px;
  }
```

- [ ] **Step 7: Verify**

```powershell
npm run check
npm test
npm run build
```

Expected: `0 errors` from check; all node `--test` files pass and `vitest`
reports its existing suites passing; build succeeds.

- [ ] **Step 8: Commit**

```powershell
git add app/src/lib/LayoutView.svelte
git commit -m "Snap canvas drags and resizes to nearby edges"
```

---

## Task 3: Arrow-key nudge

**Files:**
- Modify: `app/src/lib/LayoutView.svelte` — new key handlers in the script, and
  a `<svelte:window>` element in the markup.
- Test: none, for the reasons in Task 2. Key auto-repeat cannot be faithfully
  simulated in jsdom, and the arithmetic here is a single addition.

**Interfaces:**
- Consumes: `commitUnit` and `nudging` from Task 2; the existing `preview`,
  `rectOf`, `units`, `selectedId`, `readOnly`, `drag`.
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Add the handlers**

In `app/src/lib/LayoutView.svelte`, after `commitUnit`:

```ts
  // --- Arrow-key nudge -------------------------------------------------------
  // Bound on the window rather than a focusable canvas: the selection can just
  // as well have been made in the window panel, and a focus-scoped handler
  // would silently do nothing in that case.

  const NUDGE = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] } as const;

  /** The unit the nudge moves: the one whose anchor or tabs carry the
   * selection, so nudging a stacked window moves its whole stack — the same
   * unit a canvas drag would have grabbed. */
  const selectedUnit = () =>
    units.find((u) => u.anchor.id === selectedId || u.tabs.some((t) => t.id === selectedId)) ?? null;

  function onKeyDown(e: KeyboardEvent) {
    if (readOnly || drag || e.ctrlKey || e.metaKey || e.altKey) return;
    const step = NUDGE[e.key as keyof typeof NUDGE];
    if (!step) return;
    // Never steal the arrows from a text field: the window filter and the
    // panel's own x/y/w/h number inputs both want them.
    const t = e.target as HTMLElement | null;
    if (t && ["INPUT", "SELECT", "TEXTAREA"].includes(t.tagName)) return;
    const unit = selectedUnit();
    if (!unit) return;
    e.preventDefault(); // or the canvas pane scrolls out from under the nudge
    const n = e.shiftKey ? 10 : 1;
    const r = rectOf(unit.anchor);
    // Preview only — no backend traffic per keypress. Key auto-repeat fires
    // dozens of keydowns and exactly ONE keyup, so a glide costs one commit.
    // No snapping: nudging is the tool you reach for when a snap put the window
    // one pixel off, and snapping it back would make the two fight.
    nudging = unit.anchor.id;
    preview = { ...preview, [unit.anchor.id]: { ...r, x: r.x + step[0] * n, y: r.y + step[1] * n } };
  }

  async function onKeyUp(e: KeyboardEvent) {
    if (!nudging || !(e.key in NUDGE)) return;
    const id = nudging;
    nudging = null;
    const unit = units.find((u) => u.anchor.id === id);
    if (unit) await commitUnit(unit);
  }
```

- [ ] **Step 2: Bind them**

At the very top of the markup, before `{#if layout === null}`:

```svelte
<svelte:window onkeydown={onKeyDown} onkeyup={onKeyUp} />
```

- [ ] **Step 3: Verify**

```powershell
npm run check
npm test
npm run build
```

Expected: `0 errors`, all suites pass, build succeeds.

- [ ] **Step 4: Manual check in the running app**

```powershell
npm run tauri dev
```

Open a character, select a window on the canvas, and confirm:
- a single arrow tap moves the rectangle by 1 (watch the panel's x/y numbers);
- Shift+arrow moves by 10;
- holding an arrow glides the rectangle and produces **one** save when released;
- clicking into the window filter box and pressing an arrow moves the text
  caret, not the window;
- clicking into a panel x/y/w/h number input and pressing an arrow steps the
  number, not the window.

- [ ] **Step 5: Commit**

```powershell
git add app/src/lib/LayoutView.svelte
git commit -m "Nudge the selected window with the arrow keys"
```

---

## Task 4: Whole-slice verification and live smoke

**Files:**
- Modify: `docs/small-tasks.md` — only to log anything deliberately deferred.

**Interfaces:**
- Consumes: everything above.
- Produces: a merge-ready branch.

- [ ] **Step 1: Full gate**

```powershell
npm run check
npm test
npm run build
cargo test --manifest-path ../src-tauri/Cargo.toml
```

Expected: all green. The Rust suite is untouched by this slice and must stay
that way — if it fails, something outside this plan's scope changed.

- [ ] **Step 2: Live smoke against a real character**

Run `npm run tauri dev`, open a real character file with a populated layout, and
walk the spec's §6 checklist:

- drag a window so its left edge approaches another window's right edge; confirm
  it locks flush and the panel's numbers are exactly equal (`a.x === b.x + b.w`);
- drag a window against each of the four screen edges; confirm `x === 0`,
  `y === 0`, `x + w === reference_w`, `y + h === reference_h`;
- drag a window against the neocom's inner edge; confirm it locks there;
- hold **Alt** during the same drag and confirm it passes straight through with
  no guide line;
- resize by each of the four corners onto a neighbour's edge; confirm only the
  dragged corner moves and the opposite one stays put;
- confirm the guide line appears on snap and disappears on drop, and that at
  most one line per axis is ever visible;
- turn on a filter that hides some windows, then drag; confirm the hidden
  windows do not attract the drag;
- drag a **stacked** window and confirm every member follows (open and closed —
  reopen the file and check a closed member's geometry in the tree);
- nudge with arrows and with Shift held, per Task 3 Step 4;
- nudge a stacked window and confirm the whole stack moves.

- [ ] **Step 3: Record any deferrals**

If anything was consciously left undone, append an item to the **Open** section
of `docs/small-tasks.md` in the house format (bold title, a sentence of why, and
`_Added 2026-07-26._`). If nothing was deferred, skip this step and do not touch
the file.

- [ ] **Step 4: Commit anything from Step 3 and open the PR**

```powershell
git add docs/small-tasks.md
git commit -m "Ledger the precision-editing follow-ups"
gh pr create --title "Layout precision editing (slice 1b)" --body-file <path-to-body>
```

`--body-file`, not `--body`: multi-line bodies do not survive the shell here.
The PR body states what shipped (edge snapping on move and resize, Alt to
disable, guide lines, arrow-key nudge), that the live smoke was run and what it
covered, and that there is no Rust or wire-format change.

---

## Self-Review

**Spec coverage**

| Spec section | Task |
|---|---|
| §3 `snapLines` / `movingEdges` / `snapDelta` | Task 1 |
| §3 candidates from drawn units + furniture + canvas, self excluded | Task 2 Step 2 (`linesFor`) |
| §3 tolerance `toData(6, scale)` | Task 2 Step 4 |
| §3 move and resize application | Task 2 Step 4 |
| §3 Alt disables, read off the event | Task 2 Step 4 |
| §4 guide lines | Task 2 Steps 2, 6 |
| §5 nudge: guards, 1px / Shift 10px, preview on keydown, commit on keyup | Task 3 |
| §5 nudge commits through the drag's fan-out path | Task 2 Step 5 (`commitUnit`), used by Task 3 |
| §6 unit tests | Task 1 Step 1 |
| §6 no `LayoutView.spec.ts` | Task 2 note |
| §6 live smoke | Task 4 Step 2 |
| §2 furniture is a snap source, never a target, and is not nudged | Task 2 Step 2 (`furniture` variant has no `lines`; `linesFor` includes furniture rects) |

**Placeholders:** none — every code step carries the actual code, and the only
`<path-to-body>` placeholder is a file the implementer writes at PR time.

**Type consistency:** `snapLines`/`movingEdges`/`snapDelta`/`SnapLines`/
`SnapResult`/`Rect` are named identically in Task 1's implementation, Task 1's
tests, and Task 2's call sites. `commitUnit(unit: DrawUnit)` and
`nudging: string | null` are both declared in Task 2 and consumed in Task 3.
`rectOf` returns `{x, y, w, h}`, which satisfies `Rect` structurally.
