# HUD Footprint Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the ship HUD and fighter UI cover the screen area they really occupy, so windows stop snapping to edges the player cannot see and overlapping the parts the editor never drew.

**Architecture:** Three corrections in `layout.ts`, all driven by measurements taken off three native screenshots on 2026-07-28 (see Background). The ship HUD's *anchor model* changes — the stored offset positions the capacitor wheel's centre, and the element is strongly asymmetric about it — so `hudRects` and its inverse `shipOffsetFromX` are corrected as a matched pair. Then the boxes get drawn contents (slot rows, ability grid) so a player recognises what they are positioning against.

**Tech Stack:** Svelte 5 (runes), TypeScript, `node --test` for pure logic (`layout.test.ts`).

## Global Constraints

- **No new dependencies.**
- **`hudRects` and `shipOffsetFromX` are a matched pair and must be corrected together.** `shipOffsetFromX` must be the exact algebraic inverse of the ship-HUD placement in `hudRects`; a test pins the round trip. Getting this wrong writes a wrong offset into a real settings file.
- **All measurements below are in data px at 2560×1440.** They are absolute screen pixels, like every other value in this file — the existing code already treats HUD sizes as absolute, and EVE stores the anchors in absolute px.
- Run the frontend suite with `npm test` from `app/`, type-check with `npm run check` from `app/`. Never run vitest from the repo root.
- Commit after each task.

---

## Background: what the screenshots measured

Three native 2560×1440 PNGs were captured on character **Storm Delay** (93622368), profile `g_eve_shared_cache_sharedcache_tq_tranquility`, on 2026-07-28: `hud_battleship.png` (16:47), `hud_frigate.png` (16:48), `fighter.png` (16:51). The settings file was written at 16:52 — *after* all three — so the stored anchors correspond to the pixels:

```
shipuialignleftoffset  = -642.0
fightersDetachedPosition = (329, 289)
```

### The finding that matters: the offset anchors the capacitor wheel, not the box

Two shots of the **same character at the same offset**, differing only in the ship flown:

| | left edge | right edge | width |
|---|---|---|---|
| Battleship (8-slot row) | 490 | **1133** | 643 |
| Frigate | 490 | **896** | 406 |

The left edge is pixel-identical; only the right moves. So the element does not expand symmetrically about its anchor — it **grows rightward from a fixed left edge**.

Isolating the capacitor wheel by colour (it is the only saturated orange in that region) puts its centre at **x = 638.5**. The file predicts `reference_w / 2 + offset = 1280 - 642 = 638`. **A half-pixel match.**

That single fact reconciles everything, including the 2026-07-27 experiment that reported "writing 0.0 drew the HUD dead centre" — it is the *capacitor* that lands dead centre, not the element's bounding box.

### Ship HUD, relative to the anchor `A = reference_w / 2 + offset`

| Quantity | Measured | What the code does today |
|---|---|---|
| Left edge | `A - 148` (490) | `A - w/2` = 295 — **195px too far left** |
| Right edge, widest row | `A + 495` (1133) | `A + w/2` = 981 — **misses 152px of rack** |
| Width | **643** | 686 |
| Vertical extent (top-aligned) | y **28 .. 187** | y 0 .. 250 |
| Height | **160** | 250 |
| Slot pitch | ~50 (8 slots span 735→1133) | — |

The battleship shot's widest row already carries **8 slots**, which the developer confirmed is the maximum (8 per row, 3 rows). So 1133 is the real maximum right edge — no extrapolation was needed.

### Fighter UI, relative to the anchor `(fx, fy)` = stored `(329, 289)`

The anchor is confirmed exactly: measured panel left edge ≈ 333 (stored 329) and ability-grid top ≈ 289 (stored 289).

The shot shows **4 squadrons, 3 launched** (so 3 ability columns). Column pitch is **86**, identical for ability columns and squadron circles.

| Quantity | Measured (4 squads) | Extrapolated to 5 | Today |
|---|---|---|---|
| Width | 381 | **467** (`381 + 86`) | 400 |
| Height | **253** | 253 (rows do not change) | 120 |

Some carriers field up to 5 squadrons, so 467 is the size to draw.

### The one assumption left unverified

The left edge at 490 is a column of four round ship-control buttons. Both shots share one offset, so they cannot prove those buttons *move with* the HUD rather than being independently screen-anchored. Everything else here is measured, this one is inferred from them sitting flush against the capacitor ring. **Cheap check when the client is next open:** drag the ship HUD sideways, screenshot, and confirm the button column moved with it and the capacitor centre still lands on `reference_w/2 + offset`. That would also confirm the anchor model at a second offset.

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `app/src/lib/layout.ts` | Furniture geometry and its inverse | Modify: `HUD_NOMINAL`, `hudRects`, `shipOffsetFromX` |
| `app/src/lib/layout.test.ts` | Pure logic tests | Modify: pin the measured geometry and the round trip |
| `app/src/lib/LayoutView.svelte` | The canvas | Modify: draw slot rows and the ability grid |
| `docs/format-notes.md` | Format reference | Modify: record the anchor model |
| `docs/small-tasks.md` | The ledger | Modify: close the entry |

---

### Task 1: Correct the ship HUD's size and anchor

**Files:**
- Modify: `app/src/lib/layout.ts:137-169` (`HUD_NOMINAL` + its comment), `:187-201` (`shipOffsetFromX`), `:244-261` (the shipui branch of `hudRects`)
- Test: `app/src/lib/layout.test.ts`

**Interfaces:**
- Consumes: `Hud`, `WindowLayout` from `./api`; `hudNum`, `hudFlag` (already in this file).
- Produces: `HUD_NOMINAL.shipui` becomes `{ w: 643, h: 160 }`; new exported const `SHIP_ANCHOR_LEFT = 148` and `SHIP_TOP_MARGIN = 28`. `shipOffsetFromX(x, referenceW)` keeps its signature, changed arithmetic. Task 3 reads `SHIP_ANCHOR_LEFT` for the drawn contents.

- [ ] **Step 1: Write the failing test**

Add to `app/src/lib/layout.test.ts`, after the existing `hudRects` / `shipOffsetFromX` cases. Add `SHIP_ANCHOR_LEFT` to the existing `./layout.ts` import on line 300.

Reuse the fixtures that file already defines — `fullHud(over)`, whose `over` map is keyed by field name and takes an `HudEntry` built with `hudEntry(name, value, kind, default)`, and the existing `layout2560` constant. Do **not** introduce new helpers. The values below reproduce the measured screenshot exactly.

```ts
// The 2026-07-28 screenshot, reproduced: Storm Delay, 2560x1440, offset -642,
// top-aligned. Every number is measured, not assumed — see the plan's
// Background table. If one of these changes, a real screenshot disagreed.
{
  const hud = fullHud({
    ship_offset: hudEntry("ship_offset", "-642", "float", "0"),
    ship_top: hudEntry("ship_top", "true", "bool", "false"),
  });
  const ship = hudRects(hud, layout2560).find((f) => f.kind === "shipui")!;

  check("the ship HUD's left edge sits 148px left of the anchor", ship.x === 490);
  check("its right edge covers the widest slot row", ship.x + ship.w === 1133);
  check("its top clears the screen edge by the measured margin", ship.y === 28);
  check("its bottom is where the speed readout ends", ship.y + ship.h === 188);

  // The anchor itself: the capacitor wheel's centre, measured at 638.5.
  check(
    "the anchor lands on the capacitor wheel, not the box centre",
    ship.x + SHIP_ANCHOR_LEFT === 2560 / 2 - 642,
  );
  check(
    "the box is NOT centred on the anchor (it grows rightward)",
    ship.x + ship.w / 2 !== 2560 / 2 - 642,
  );
}

// shipOffsetFromX must be the exact inverse of the placement above, or a drag
// writes an offset that puts the HUD somewhere other than where it was dropped.
{
  for (const offset of [-642, -189, 0, 300]) {
    const hud = fullHud({
      ship_offset: hudEntry("ship_offset", String(offset), "float", "0"),
      ship_top: hudEntry("ship_top", "true", "bool", "false"),
    });
    const ship = hudRects(hud, layout2560).find((f) => f.kind === "shipui")!;
    check(
      `dragging to its own drawn x round-trips the offset (${offset})`,
      shipOffsetFromX(ship.x, 2560) === offset,
    );
  }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run from `app/`: `node --test "src/lib/layout.test.ts"`
Expected: FAIL on the left-edge check — today `hudRects` draws `x = 1280 - 642 - 686/2 = 295`, not 490.

- [ ] **Step 3: Write the implementation**

In `app/src/lib/layout.ts`, replace the `HUD_NOMINAL` block and its preceding comment (lines 137-169) with:

```ts
// EVE stores anchors but never sizes, and never says what an anchor is relative
// to, so all of this began as assumption. Two live sessions settled the
// conventions and 2026-07-28 settled the sizes, off three native 2560x1440
// screenshots measured against the settings file that produced them.

/**
 * How far the ship HUD extends LEFT of its anchor.
 *
 * The anchor is the capacitor wheel's centre — NOT the element's centre. This is
 * the correction that matters: two shots of one character at one offset, flying
 * a battleship and a frigate, share a pixel-identical left edge (490) and differ
 * only on the right (1133 vs 896). The element grows rightward from a fixed left
 * edge, so it is strongly asymmetric about its anchor: 148px left, 495px right.
 *
 * Isolating the capacitor wheel by colour put its centre at x=638.5, against
 * `2560/2 + (-642) = 638` from the file — a half-pixel match. That also explains
 * the 2026-07-27 result that writing 0.0 drew the HUD "dead centre": it is the
 * capacitor that centres, not the box.
 */
export const SHIP_ANCHOR_LEFT = 148;

/** Gap between the screen edge and the HUD when it is top-aligned (measured). */
export const SHIP_TOP_MARGIN = 28;

/**
 * Drawn sizes for the screen furniture, in data px. MEASURED 2026-07-28 except
 * `badge`, which is still nominal.
 *
 * These are not cosmetic: `LayoutView` feeds each furniture rect's `w`/`h` into
 * the snap-line set, so a box smaller than the real element makes windows snap
 * against an edge the player cannot see and overlap the part we failed to draw.
 * The previous values (shipui 686x250, fighter 400x120) were invented, and the
 * shipui box was additionally drawn centred on the anchor — putting it 195px too
 * far left while missing 152px of module rack on the right.
 *
 * `shipui` covers the widest possible rack: the battleship shot's widest row
 * already carries the maximum 8 slots (pitch ~50), so 643 is a measured maximum
 * rather than an extrapolation.
 *
 * `fighter` covers 5 squadrons, the most a carrier can field. The shot had 4
 * (3 launched, so 3 ability columns); column pitch is 86, so the fifth adds 86
 * to the measured 381. Height does not change with squadron count.
 */
export const HUD_NOMINAL = {
  shipui: { w: 643, h: 160 },
  fighter: { w: 467, h: 253 },
  badge: { w: 32, h: 32 },
};
```

Replace `shipOffsetFromX` (lines 187-201) with:

```ts
/**
 * Stored offset for a ship-HUD rect whose left edge is at data-px `x`. The exact
 * inverse of hudRects' ship-HUD placement below — a matched pair, correct them
 * together, and `layout.test.ts` round-trips them because getting this wrong
 * writes a bad offset into a real settings file.
 *
 * CONFIRMED in-game 2026-07-27 that the offset is centre-relative and negative
 * is leftward; MEASURED 2026-07-28 that what it centres is the capacitor wheel,
 * which sits `SHIP_ANCHOR_LEFT` from the element's left edge. The old version
 * used `w/2` here and claimed the width cancelled out; that was true only while
 * the drawn box was (wrongly) centred on the anchor.
 */
export function shipOffsetFromX(x: number, referenceW: number): number {
  return Math.round(x + SHIP_ANCHOR_LEFT - referenceW / 2);
}
```

And in `hudRects`, replace the shipui branch (lines 244-261):

```ts
  // The stored offset places the capacitor wheel's centre at
  // `reference_w/2 + offset` (measured 2026-07-28 to within half a pixel). The
  // element then extends SHIP_ANCHOR_LEFT to the left of that point and the rest
  // to the right — it is NOT centred on it. Its inverse is shipOffsetFromX.
  const offset = hudNum(hud, "ship_offset");
  if (offset !== null) {
    const { w, h } = HUD_NOMINAL.shipui;
    out.push({
      kind: "shipui",
      label: "Ship HUD",
      x: Math.round(layout.reference_w / 2 + offset - SHIP_ANCHOR_LEFT),
      // Top-aligned leaves a measured 28px gap. The bottom-aligned case was not
      // captured; mirroring the same margin is the honest guess and is what a
      // screenshot should check next.
      y: hudFlag(hud, "ship_top") ? SHIP_TOP_MARGIN : layout.reference_h - SHIP_TOP_MARGIN - h,
      w,
      h,
      drag: "x",
    });
  }
```

- [ ] **Step 4: Run the test to verify it passes**

Run from `app/`: `node --test "src/lib/layout.test.ts"`
Expected: PASS.

**If a pre-existing test fails** because it asserted the old 686/250 geometry, update it to the measured numbers — the old values were invented and the new ones are measured. Do not weaken a round-trip or inverse assertion to make it pass; if one of those fails, the implementation is wrong.

- [ ] **Step 5: Run the full suite and type-check**

Run from `app/`: `npm test` then `npm run check`
Expected: PASS, 0 type errors. (Four pre-existing `state_referenced_locally` warnings in `ContextMenu.svelte`, `InsertForm.svelte` and `TreeNode.svelte` are unrelated; the count must not grow.)

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/layout.ts app/src/lib/layout.test.ts
git commit -m "Anchor the ship HUD on its capacitor, and size it from measurement"
```

---

### Task 2: Correct the fighter panel's size

**Files:**
- Modify: `app/src/lib/layout.ts` (`HUD_NOMINAL.fighter` — already done in Task 1's block; this task adds the test and the comment on `hudPointFromRect`)
- Test: `app/src/lib/layout.test.ts`

**Interfaces:**
- Consumes: `HUD_NOMINAL.fighter` from Task 1.
- Produces: nothing other tasks depend on.

The fighter anchor needs no change — it was confirmed exactly (stored `(329, 289)` against a measured left edge ≈333 and ability-grid top ≈289). Only the size was wrong, and badly: the stored height covered less than half the panel because the ability grid above the squadron row was never counted.

- [ ] **Step 1: Write the failing test**

Add to `app/src/lib/layout.test.ts`:

```ts
// The 2026-07-28 fighter shot: anchor (329, 289), 4 squadrons with 3 launched.
// The panel's own top-left IS the anchor — that half was already right — so this
// pins the size, and specifically that the ability grid is inside it.
{
  // fighter_detached and fighter_shown are already true in fullHud's base, which
  // is what makes hudRects emit the panel at all — only the point changes here.
  const hud = fullHud({
    fighter_x: hudEntry("fighter_x", "329", "int", "0"),
    fighter_y: hudEntry("fighter_y", "289", "int", "0"),
  });
  const f = hudRects(hud, layout2560).find((x) => x.kind === "fighter")!;

  check("the fighter panel starts at the stored anchor", f.x === 329 && f.y === 289);
  check("it is wide enough for five squadrons", f.w === 467);
  check(
    "it is tall enough for the ability grid, not just the squadron row",
    f.h === 253,
  );
  // The regression this guards: 120 covered the squadron row alone, so windows
  // snapped straight through the abilities above it.
  check("it is more than twice the old invented height", f.h > 2 * 120);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run from `app/`: `node --test "src/lib/layout.test.ts"`
Expected: FAIL if Task 1's `HUD_NOMINAL` block was not applied; PASS immediately if it was. Either is fine — this test exists to pin the numbers, and a test that passes on arrival still fails if someone later reverts them. Confirm it can fail by temporarily setting `fighter.h` back to `120`, watching it fail, then restoring `253`.

- [ ] **Step 3: Record the measurement beside the anchor doc**

In `app/src/lib/layout.ts`, extend the `hudPointFromRect` doc comment (which already documents the confirmed anchor) with:

```
 * Sizes MEASURED 2026-07-28 from the same shot the anchor was confirmed on:
 * with 4 squadrons (3 launched) the panel spans 381x253 from the anchor, on a
 * column pitch of 86 shared by the ability grid and the squadron row. Five
 * squadrons is the carrier maximum, hence the 467 width in HUD_NOMINAL. Height
 * is independent of squadron count.
```

- [ ] **Step 4: Run the full suite**

Run from `app/`: `npm test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/layout.ts app/src/lib/layout.test.ts
git commit -m "Size the fighter panel to include its ability grid"
```

---

### Task 3: Draw the racks and the ability grid

**Files:**
- Modify: `app/src/lib/LayoutView.svelte:744-756` (the furniture markup) and its `<style>` block

**Interfaces:**
- Consumes: `FurnitureRect` (`kind` discriminates); nothing new from `layout.ts` — the contents are expressed as percentages of the box, so they scale with the canvas for free.
- Produces: nothing other tasks depend on.

Sizing the box is half the job; the ledger asks for the other half, because a bare rectangle does not tell a player what they are positioning against. Contents are **decoration only** — they must not affect `hudRects`, the snap lines, or any drag.

Percentages come from the measurements. Ship HUD box is 643 wide, 160 tall, left edge at anchor−148:
- capacitor centre at `148/643` ≈ **23%** from the left, radius ~78px ≈ **12%** of width
- slot rows start at `(735-490)/643` ≈ **38%**, pitch `50/643` ≈ **7.8%**, 8 columns reaching ~100%
- three rows spanning roughly `2/160` to `89%` of the height

Fighter box is 467 wide, 253 tall, anchored at its top-left:
- ability grid starts at `(399-329)/467` ≈ **15%**, pitch `86/467` ≈ **18.4%**, 5 columns, 3 rows in the upper ~62%
- squadron row across the lower ~38%

- [ ] **Step 1: Add the contents**

In `app/src/lib/LayoutView.svelte`, replace the furniture body (line 754) so the label keeps its place and the schematic sits behind it:

```svelte
            <span class="furniture-label">{f.label}</span>
            {#if f.kind === "shipui"}
              <!-- Decoration only: a schematic of the capacitor and the three
                   module rows, so the box reads as the thing it represents.
                   Percentages come from the 2026-07-28 measurements, so it
                   rescales with the canvas and needs no scale arithmetic. -->
              <div class="cap"></div>
              {#each [0, 1, 2] as row}
                {#each [0, 1, 2, 3, 4, 5, 6, 7] as col}
                  <div class="slot" style="left: {38 + col * 7.8}%; top: {6 + row * 30}%;"></div>
                {/each}
              {/each}
            {:else if f.kind === "fighter"}
              {#each [0, 1, 2] as row}
                {#each [0, 1, 2, 3, 4] as col}
                  <div class="ability" style="left: {15 + col * 18.4}%; top: {2 + row * 20}%;"></div>
                {/each}
              {/each}
              {#each [0, 1, 2, 3, 4] as col}
                <div class="squad" style="left: {15 + col * 18.4}%;"></div>
              {/each}
            {/if}
```

And add to the `<style>` block:

```css
  /* HUD schematics. Pointer-events off throughout: the box is what gets
     dragged, and a child intercepting the pointer would break the drag. */
  .furniture .cap,
  .furniture .slot,
  .furniture .ability,
  .furniture .squad {
    position: absolute;
    border-radius: 50%;
    border: 1px solid currentColor;
    opacity: 0.35;
    pointer-events: none;
  }
  .furniture .cap { left: 11%; top: 18%; width: 24%; height: 64%; }
  .furniture .slot { width: 6.2%; height: 25%; }
  .furniture .ability { width: 14%; height: 17%; }
  .furniture .squad { width: 14%; height: 22%; top: 70%; }
```

- [ ] **Step 2: Check it renders and still drags**

Run from `app/`: `npm run check`
Expected: 0 type errors.

Then confirm by eye that the ship HUD box shows a capacitor circle plus three rows of eight, the fighter box shows a 5×3 grid above a squadron row, and **dragging either still works** — the schematic must not swallow the pointer. Use the `run` skill if a launch recipe is wanted.

- [ ] **Step 3: Run the full suite**

Run from `app/`: `npm test`
Expected: PASS. Nothing here touches geometry, so a failure means the schematic leaked into `hudRects` or the snap lines.

- [ ] **Step 4: Commit**

```bash
git add app/src/lib/LayoutView.svelte
git commit -m "Draw the module racks and the fighter ability grid in their boxes"
```

---

### Task 4: Record the measurements and close the ledger

**Files:**
- Modify: `docs/format-notes.md` (the "HUD anchors" section)
- Modify: `docs/small-tasks.md`

**Interfaces:** none — documentation only.

- [ ] **Step 1: Record the anchor model**

In `docs/format-notes.md`, in the "HUD anchors" section, add:

```markdown
**`shipuialignleftoffset` anchors the capacitor wheel, not the element's box.**
Measured 2026-07-28 from three native 2560×1440 screenshots (Storm Delay,
profile `g_eve_shared_cache_sharedcache_tq_tranquility`, file written after the
shots so the anchors match the pixels; `shipuialignleftoffset = -642.0`).

Isolating the capacitor wheel by colour put its centre at **x = 638.5**, against
`2560/2 + (-642) = 638` from the file. The element is strongly **asymmetric**
about that point: it extends **148px left** and **495px right**, spanning
490..1133 with the widest (8-slot) module row.

Two shots of the same character at the same offset, flying a battleship and a
frigate, share a pixel-identical left edge (490) and differ only on the right
(1133 vs 896) — the racks grow rightward from a fixed left edge. This is also why
the 2026-07-27 experiment saw offset 0.0 draw the HUD "dead centre": what centres
is the capacitor, not the box.

Vertical extent, top-aligned: **y 28..187** (height 160). The bottom-aligned case
has not been captured.

**`fightersDetachedPosition` is the panel's left edge and the ability grid's top**
— confirmed again 2026-07-28: stored `(329, 289)` against a measured left edge of
≈333 and a grid top of 289. With 4 squadrons (3 launched) the panel spans
**381×253**; column pitch is **86**, shared by the ability grid and the squadron
row, so the 5-squadron carrier maximum is **467** wide. Height is independent of
squadron count.
```

- [ ] **Step 2: Close the ledger entry**

Delete the `- [ ] **The HUD furniture must cover its real in-game footprint — module racks and fighter abilities included.**` entry from **Open** and add under `### Unreleased (on master)`:

```markdown
- [x] **The HUD furniture must cover its real in-game footprint — module racks
  and fighter abilities included.** Measured off three native screenshots
  2026-07-28 and corrected. The ship HUD was wrong in a way the entry did not
  anticipate: not merely mis-sized but mis-*anchored*. The stored offset centres
  the capacitor wheel (measured 638.5 against 638 predicted), and the element
  extends 148px left of it and 495px right — so the old box, centred on the
  anchor, sat 195px too far left and missed 152px of module rack. Now 643×160
  from `anchor-148`, with `shipOffsetFromX` corrected as its exact inverse and a
  round-trip test pinning the pair. The fighter panel keeps its (already correct)
  anchor and grows from 400×120 to 467×253 — the old height covered the squadron
  row alone, so windows snapped straight through the ability grid above it. Both
  boxes now draw their contents. One assumption remains unverified: that the
  left-hand ship-control button column moves with the HUD rather than being
  screen-anchored — both shots share one offset, so they cannot separate the two.
  _Added 2026-07-28; done 2026-07-28._
```

- [ ] **Step 3: Add the follow-up check to the ledger**

Add a new **Open** entry:

```markdown
- [ ] **Confirm the ship HUD's anchor at a second offset.** The 2026-07-28
  measurement fixed the anchor model from two screenshots that share one offset
  (-642), which pins the geometry but cannot prove the left-hand ship-control
  button column moves with the HUD rather than being independently
  screen-anchored — the 148px left extension assumes it does. One screenshot
  after dragging the ship HUD sideways settles both: the button column should
  have moved with it, and the capacitor wheel's centre should still land on
  `reference_w/2 + offset`. Cheap, and it is the only inferred number in
  `HUD_NOMINAL`. _Added 2026-07-28._
```

- [ ] **Step 4: Commit**

```bash
git add docs/format-notes.md docs/small-tasks.md
git commit -m "Record the measured HUD footprint and its anchor model"
```

---

## Self-review notes

- **Coverage.** The entry asked for both elements to cover their real footprint (Tasks 1-2) and for the racks and grid to be drawn rather than just sized (Task 3). It predicted the ship HUD was "narrow and tall" — measurement says narrow on the right but *too tall* (160 real vs 250 drawn), and the dominant error was the anchor, not the size.
- **The entry's own numbers were close but not right.** It cited a native crop of "roughly 715x205 including its margin" against 643×160 measured. The crop evidently included margin the element does not occupy; measuring icon-to-icon, as the entry itself suggested, is what settled it.
- **Why `shipOffsetFromX` changes even though "the width cancels".** The old comment was correct *given* the old placement: with the box centred on the anchor, `+w/2` in the inverse cancelled `-w/2` in the placement. Once placement uses `SHIP_ANCHOR_LEFT`, the inverse must too. The round-trip test is the guard.
- **Task 3 is decoration and is separable.** If it looks wrong in the app, drop it — Tasks 1-2 are the ones that fix snapping, and they stand alone.
- **Not in scope, deliberately.** The badge size (still nominal, never measured), the bottom-aligned ship HUD margin (not captured), the docked-vs-in-space neocom difference (its own ledger entry), and any per-resolution or UI-scale behaviour — every measurement here is from one 2560×1440 client with default UI scale.
