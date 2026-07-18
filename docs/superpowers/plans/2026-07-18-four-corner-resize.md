# Four-Corner Resize Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a selected layout-canvas window be resized from any of its four corners, with the opposite corner staying fixed.

**Architecture:** Extract the corner math into one pure, unit-tested helper `resizeRect` in `app/src/lib/layout.ts` (this is the piece the future coherent stack-resize reuses). Then rewire `LayoutView.svelte`'s existing single-handle resize to carry a corner, call `resizeRect`, and render four selected-only handles. No backend change — `geomMutations` already accepts `{x,y,w,h}` and diffs each field.

**Tech Stack:** SvelteKit 5 (runes) frontend, TypeScript, `node --test` (zero-dep, throw-based checks). Tauri app; backend is a Rust `settings-model` crate (untouched here).

## Global Constraints

- **No new dependencies** — frontend uses only what's already in `app/package.json`.
- **Tests:** zero-dep, throw-based `check(name, ok)` idiom appended to the existing `app/src/lib/layout.test.ts`. No test framework, no `test()` blocks. Run from `app/` with `npm test` (which runs `node --test "src/lib/**/*.test.ts"`; Node strips the TS types). On this Windows machine, `npm` is invoked through PowerShell, not the Bash tool's PATH.
- **Type check:** `npm run check` from `app/` (svelte-check) must pass.
- **Commits:** sentence-case messages, **no attribution trailers** (repo convention).
- **Behavior parity:** minimum size stays `0`, matching today's `Math.max(0, …)` — this is not a new constraint.
- **Backend untouched:** no Rust, no codec changes.

---

### Task 1: `resizeRect` pure helper + `Corner` type

**Files:**
- Modify: `app/src/lib/layout.ts` (add `Corner` type + `resizeRect`)
- Test: `app/src/lib/layout.test.ts` (append checks)

**Interfaces:**
- Consumes: nothing (pure function, no imports beyond what the file has).
- Produces:
  - `export type Corner = "tl" | "tr" | "bl" | "br";`
  - `export function resizeRect(orig: { x: number; y: number; w: number; h: number }, corner: Corner, dx: number, dy: number): { x: number; y: number; w: number; h: number }` — resizes `orig` by dragging `corner` by `(dx, dy)` data px, holding the opposite corner fixed.

- [ ] **Step 1: Write the failing tests**

Append to `app/src/lib/layout.test.ts` (the `check` helper and imports already exist at the top of the file — add `resizeRect` to the existing import line and append this block at the end, before the final `console.log`):

Change the import line at the top from:
```ts
import { canvasScale, toCanvas, toData, openWindows } from "./layout.ts";
```
to:
```ts
import { canvasScale, toCanvas, toData, openWindows, resizeRect } from "./layout.ts";
```

Append this block just before the final `console.log("layout: all checks passed");`:
```ts
// --- resizeRect: drag one corner, opposite corner stays anchored ------------
{
  const orig = { x: 100, y: 100, w: 200, h: 100 }; // right=300, bottom=200

  // BR: only w/h grow; top-left (100,100) anchored (today's behavior).
  const br = resizeRect(orig, "br", 40, 20);
  check("br keeps top-left anchored", br.x === 100 && br.y === 100);
  check("br grows w,h by the delta", br.w === 240 && br.h === 120);

  // TL: x/y move; bottom-right (300,200) stays fixed.
  const tl = resizeRect(orig, "tl", 40, 20);
  check("tl moves x,y by the delta", tl.x === 140 && tl.y === 120);
  check("tl keeps bottom-right fixed", tl.x + tl.w === 300 && tl.y + tl.h === 200);

  // TR: right/top move; bottom-left (100,200) stays fixed.
  const tr = resizeRect(orig, "tr", 40, 20);
  check("tr keeps bottom-left fixed", tr.x === 100 && tr.y + tr.h === 200);
  check("tr grows w, shrinks h", tr.w === 240 && tr.h === 80);

  // BL: left/bottom move; top-right (300,100) stays fixed.
  const bl = resizeRect(orig, "bl", 40, 20);
  check("bl keeps top-right fixed", bl.x + bl.w === 300 && bl.y === 100);
  check("bl shrinks w, grows h", bl.w === 160 && bl.h === 120);

  // Clamp: a delta larger than the size floors size at 0 and pins the dragged
  // corner to the anchor — it cannot cross it.
  const crossed = resizeRect(orig, "tl", 999, 999);
  check("clamp floors w,h at 0", crossed.w === 0 && crossed.h === 0);
  check("clamp pins the corner to the anchor", crossed.x === 300 && crossed.y === 200);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run (from `app/`, via PowerShell): `npm test`
Expected: FAIL — `resizeRect` is not exported (import error / `resizeRect is not a function`).

- [ ] **Step 3: Write the minimal implementation**

Append to `app/src/lib/layout.ts`:
```ts
export type Corner = "tl" | "tr" | "bl" | "br";

/**
 * Resize a rect by dragging one corner by (dx, dy) data px. The opposite
 * corner is the fixed anchor. Size floors at 0 (matching the canvas's existing
 * resize) and the dragged corner is pinned so it can't cross the anchor.
 */
export function resizeRect(
  orig: { x: number; y: number; w: number; h: number },
  corner: Corner,
  dx: number,
  dy: number,
): { x: number; y: number; w: number; h: number } {
  const left = corner === "tl" || corner === "bl";
  const top = corner === "tl" || corner === "tr";
  let { x, y, w, h } = orig;
  if (left) {
    const anchorR = orig.x + orig.w; // right edge stays fixed
    x = Math.min(orig.x + dx, anchorR);
    w = anchorR - x;
  } else {
    w = Math.max(0, orig.w + dx); // left edge fixed, right edge moves
  }
  if (top) {
    const anchorB = orig.y + orig.h; // bottom edge stays fixed
    y = Math.min(orig.y + dy, anchorB);
    h = anchorB - y;
  } else {
    h = Math.max(0, orig.h + dy);
  }
  return { x, y, w, h };
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run (from `app/`): `npm test`
Expected: PASS — every `resizeRect` check prints `ok - …` and the run ends with `layout: all checks passed`.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/layout.ts app/src/lib/layout.test.ts
git commit -m "Add resizeRect corner-resize helper"
```

---

### Task 2: Wire four-corner resize into the canvas

**Files:**
- Modify: `app/src/lib/LayoutView.svelte` (import, `Drag` type, `startResize`, `onPointerMove`, `onPointerUp`, handle markup, CSS)

**Interfaces:**
- Consumes from Task 1: `Corner`, `resizeRect` from `$lib/layout`.
- Produces: no exported interface (component-internal wiring). Manual/visual deliverable.

- [ ] **Step 1: Import `Corner` and `resizeRect`**

In `app/src/lib/LayoutView.svelte`, extend the existing layout import. Change:
```ts
import { canvasScale, toCanvas, toData, openWindows } from "$lib/layout";
```
to:
```ts
import { canvasScale, toCanvas, toData, openWindows, resizeRect, type Corner } from "$lib/layout";
```

- [ ] **Step 2: Widen the `Drag` resize variant to carry the corner and origin**

Replace the `Drag` type:
```ts
  type Drag =
    | { kind: "move"; w: WindowRect; startX: number; startY: number; ox: number; oy: number }
    | { kind: "resize"; w: WindowRect; startX: number; startY: number; ow: number; oh: number };
```
with:
```ts
  type Drag =
    | { kind: "move"; w: WindowRect; startX: number; startY: number; ox: number; oy: number }
    | { kind: "resize"; w: WindowRect; corner: Corner; startX: number; startY: number; ox: number; oy: number; ow: number; oh: number };
```

- [ ] **Step 3: Take the corner in `startResize`**

Replace `startResize`:
```ts
  function startResize(w: WindowRect, e: PointerEvent) {
    if (readOnly) return;
    selectedId = w.id;
    drag = { kind: "resize", w, startX: e.clientX, startY: e.clientY, ow: w.geom!.w, oh: w.geom!.h };
    canvasEl?.setPointerCapture(e.pointerId);
    e.preventDefault();
    e.stopPropagation();
  }
```
with:
```ts
  function startResize(w: WindowRect, corner: Corner, e: PointerEvent) {
    if (readOnly) return;
    selectedId = w.id;
    drag = {
      kind: "resize", w, corner, startX: e.clientX, startY: e.clientY,
      ox: w.geom!.x, oy: w.geom!.y, ow: w.geom!.w, oh: w.geom!.h,
    };
    canvasEl?.setPointerCapture(e.pointerId);
    e.preventDefault();
    e.stopPropagation();
  }
```

- [ ] **Step 4: Use `resizeRect` in `onPointerMove`**

Replace the `else` (resize) branch of `onPointerMove`:
```ts
    } else {
      preview = {
        ...preview,
        [drag.w.id]: { ...rectOf(drag.w), w: Math.max(0, drag.ow + dx), h: Math.max(0, drag.oh + dy) },
      };
    }
```
with:
```ts
    } else {
      preview = {
        ...preview,
        [drag.w.id]: resizeRect({ x: drag.ox, y: drag.oy, w: drag.ow, h: drag.oh }, drag.corner, dx, dy),
      };
    }
```

- [ ] **Step 5: Commit the full rect on resize in `onPointerUp`**

In `onPointerUp`, change the commit line:
```ts
    await commit(geomMutations(w, d.kind === "move" ? { x: p.x, y: p.y } : { w: p.w, h: p.h }));
```
to:
```ts
    await commit(geomMutations(w, d.kind === "move" ? { x: p.x, y: p.y } : { x: p.x, y: p.y, w: p.w, h: p.h }));
```
(`geomMutations` diffs each field against the committed geom, so a bottom-right resize — x/y unchanged — still emits only `w`/`h`.)

- [ ] **Step 6: Render four handles, selected-only**

Replace the single resize span inside the `.win` div:
```svelte
            <span class="win-label">{w.label}</span>
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <span class="resize" onpointerdown={(e) => startResize(w, e)}></span>
```
with:
```svelte
            <span class="win-label">{w.label}</span>
            {#if w.id === selectedId}
              {#each (["tl", "tr", "bl", "br"] as const) as c}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <span class="resize {c}" onpointerdown={(e) => startResize(w, c, e)}></span>
              {/each}
            {/if}
```

- [ ] **Step 7: Replace the resize CSS with four positioned corners**

Replace the `.resize` rule:
```css
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
```
with:
```css
  .resize {
    position: absolute;
    width: 12px;
    height: 12px;
    background: currentColor;
    opacity: 0.6;
    touch-action: none;
  }
  .resize.tl { left: 0; top: 0; cursor: nwse-resize; }
  .resize.tr { right: 0; top: 0; cursor: nesw-resize; }
  .resize.bl { left: 0; bottom: 0; cursor: nesw-resize; }
  .resize.br { right: 0; bottom: 0; cursor: nwse-resize; }
```

- [ ] **Step 8: Type-check**

Run (from `app/`): `npm run check`
Expected: PASS — no svelte-check errors. (`["tl","tr","bl","br"] as const` iterates as `Corner`, matching `startResize`'s parameter.)

- [ ] **Step 9: Manual smoke (DOM drag isn't unit-tested — M2 norm)**

Launch the app (`npm run tauri dev` from `app/`, or the project's run flow), open a char file, switch to Layout, select a window, and:
- Confirm four handles appear only on the selected window and disappear when another is selected.
- Drag each corner; confirm the opposite corner stays put and the rectangle resizes in the drag direction.
- Save, reopen the file, confirm the geometry persisted as drawn.

- [ ] **Step 10: Commit**

```bash
git add app/src/lib/LayoutView.svelte
git commit -m "Wire four-corner resize into the layout canvas"
```

---

## Self-Review

**Spec coverage:**
- Four corner handles (spec §2) → Task 2 Steps 6–7. ✓
- Handles on the selected window only (§2) → Task 2 Step 6 (`{#if w.id === selectedId}`). ✓
- No backend change; reuse `geomMutations` (§2) → Task 2 Step 5 commits `{x,y,w,h}`, no Rust touched. ✓
- Pure `resizeRect` helper in `layout.ts`, reusable by stack-resize (§3) → Task 1. ✓
- Per-corner anchor math + anchor-relative clamp, MIN=0 (§3) → Task 1 Step 3 + tests. ✓
- `Drag` gains `corner` + `ox,oy,ow,oh`; pointer move/up rewired; cursors nwse/nesw (§4) → Task 2 Steps 2–7. ✓
- One `node --test` file over `resizeRect`: anchor fixed per corner + clamp (§5) → Task 1 Step 1. ✓
- DOM drag not unit-tested; manual smoke (§5) → Task 2 Step 9. ✓
- Edges / backend / stack-resize out of scope (§6) → not in any task. ✓

**Placeholder scan:** No TBD/TODO/"handle edge cases"/"similar to Task N". All code shown in full. ✓

**Type consistency:** `Corner` and `resizeRect` signatures identical across Task 1 (definition) and Task 2 (import + `startResize(w, corner, e)` call). `resizeRect` returns `{x,y,w,h}` matching the `preview` entry shape. ✓
