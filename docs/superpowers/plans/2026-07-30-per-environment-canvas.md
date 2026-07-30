# Per-environment canvas views Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the layout canvas and window list an `All / Docked / In space` view selector that shows only the windows relevant to the chosen environment.

**Architecture:** A third dimension on the existing `WindowFilter`, applied through the single shared `windowMatches` predicate so the list and the canvas narrow together. The mapping is a curated pair of exclusives sets in `windowLabels.ts` where an unlisted id shows in **both** environments. One special case — the Inventory window is the only one EVE splits per context — is handled by a pure post-pass over `stackUnits`' output that reuses the existing `DrawUnit.fanTargets` machinery.

**Tech Stack:** TypeScript, Svelte 5 (runes: `$state`, `$derived`, `$bindable`), `node --test` for `*.test.ts` (Node strips the types), vitest + jsdom for `*.spec.ts`.

Design spec: `docs/superpowers/specs/2026-07-30-per-environment-canvas-design.md`.

## Global Constraints

- **No backend change.** `windowSizesAndPositions_1` is flat; this is a view filter over a single layout. Do not touch `crates/`, `src-tauri/`, or `api.ts`.
- **`windowLabels.ts` stays pure** — no DOM, no Svelte, no `api.ts` types. It must not import from `layout.ts` (`layout.ts` imports from *it*; the reverse is circular).
- **An unrecognised id shows in BOTH environments.** Same safe-failure direction `isClutter` documents: a wrong guess paints a harmless extra rectangle rather than hiding a window the player actually placed. Never make the mapping total.
- **Test style:** throw-based `check(name, ok)` helper, no framework, matching the existing `layout.test.ts` / `windowLabels.test.ts`. Both files already define `check` at the top — reuse it, do not redeclare.
- Run tests with `npm test` from `app/` (runs `node --test` then vitest). Type-check with `npm run check`.
- All work lands on branch `worktree-per-env-canvas`.

---

### Task 1: The environment mapping (`inEnv`)

**Files:**
- Modify: `app/src/lib/windowLabels.ts` (add after the clutter section, which ends at the `isClutter` function ~line 240)
- Test: `app/src/lib/windowLabels.test.ts` (append before the final `console.log`)

**Interfaces:**
- Consumes: `describe(id): WindowName` — already in this file; `describe(id).family` is the grouping key.
- Produces:
  - `export type Env = "all" | "docked" | "space";`
  - `export function inEnv(id: string, env: Env): boolean;`

- [ ] **Step 1: Write the failing test**

Append to `app/src/lib/windowLabels.test.ts`, immediately before the final `console.log("windowLabels.test.ts ok");` line. Add `inEnv` to the existing import statement on line 3.

```ts
// --- per-environment mapping -----------------------------------------------
{
  check("everything shows in the all view", inEnv("lobbyWnd", "all") && inEnv("overview", "all"));

  check("a docked-only id shows when docked", inEnv("lobbyWnd", "docked"));
  check("a docked-only id is hidden in space", !inEnv("lobbyWnd", "space"));
  check("the structure hangar is docked-only", inEnv("StructureItemHangar", "docked"));
  check("the structure hangar is hidden in space", !inEnv("StructureItemHangar", "space"));

  check("a space-only id shows in space", inEnv("overview", "space"));
  check("a space-only id is hidden when docked", !inEnv("overview", "docked"));
  check("the d-scan window is space-only", inEnv("directionalScannerWindow", "space"));
  check("the d-scan window is hidden when docked", !inEnv("directionalScannerWindow", "docked"));

  // A family entry covers the bare parent AND every spawned instance, so
  // `ShipCargo_<itemID>` needs no entry of its own.
  check("a spawned instance follows its family", inEnv("ShipCargo_1033391582929", "space"));
  check("a spawned instance is hidden in the other env", !inEnv("ShipCargo_1033391582929", "docked"));
  check("a numbered overview follows its family", inEnv("overview_1", "space"));
  check("a numbered overview is hidden when docked", !inEnv("overview_1", "docked"));

  // Unlike isClutter there is no `detail !== ""` requirement: for environment
  // purposes a spawned instance and its bare parent are in the same place.
  check("the bare parent is in the same env as its instances", inEnv("ShipCargo", "space"));

  // THE safe-failure property. If someone later "tidies" the tables into a
  // total mapping, this is what catches it.
  check("an unlisted id shows when docked", inEnv("market", "docked"));
  check("an unlisted id shows in space", inEnv("market", "space"));
  check("an unknown id shows in both", inEnv("someWindowNobodyCurated", "docked") && inEnv("someWindowNobodyCurated", "space"));

  // The three Inventory ids each declare their own environment.
  check("InventoryStation is docked", inEnv("InventoryStation", "docked") && !inEnv("InventoryStation", "space"));
  check("InventoryStructure is docked", inEnv("InventoryStructure", "docked") && !inEnv("InventoryStructure", "space"));
  check("InventorySpace is in space", inEnv("InventorySpace", "space") && !inEnv("InventorySpace", "docked"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd app && npx tsx --test src/lib/windowLabels.test.ts` (or `npm test`)
Expected: FAIL — `inEnv` is not exported from `./windowLabels.ts`.

- [ ] **Step 3: Write minimal implementation**

Add to `app/src/lib/windowLabels.ts`, after the `isClutter` function:

```ts
// --- environments ----------------------------------------------------------
// A player's screen differs by environment, and the canvas mixes every
// environment into one picture. This is a VIEW FILTER, not a data model:
// `windowSizesAndPositions_1` stores one geometry per window id, so there is a
// single layout underneath and these sets only decide what is painted.
//
// Two environments, not EVE's thirteen. `ui → InfoPanelModes_<context>`
// enumerates the client's own list (hangar, inflight, structure, charsel,
// planet, starmap…), but only hangar/inflight/structure have an arrangeable
// window layout, and NPC station and player structure are collapsed into one
// "docked" view — which is also the split `dockPanels` itself stores
// (widthProportion_docked). See the design spec for the corpus measurements.
//
// Only the EXCLUSIVES are listed. An id in neither set shows in both views —
// the same safe-failure direction as the clutter tables: showing a harmless
// extra rectangle beats hiding a window the player actually placed. Windows
// whose environment is genuinely uncertain (Fitting, Assets, Market, the chat
// stack) are deliberately absent rather than guessed at. Grow these lazily.

export type Env = "all" | "docked" | "space";

/** Windows that only exist while docked, in an NPC station or a player
 * structure. The Structure* ids have no station twin — the station equivalent
 * is the unified `InventoryStation`. */
const DOCKED_ONLY: ReadonlySet<string> = new Set([
  "lobbyWnd",
  "cloneBay",
  "CloneStationWindow",
  "CloneUpgradeWindow",
  "InventoryStation",
  "InventoryStructure",
  "StructureItemHangar",
  "StructureShipHangar",
  "StructureCorpHangar",
  "DeliverToStructure",
]);

/** Windows that only exist in space. */
const SPACE_ONLY: ReadonlySet<string> = new Set([
  "InventorySpace",
  "ShipCargo",
  "ShipDroneBay",
  "droneview",
  "selecteditemview",
  "directionalScannerWindow",
  "overview",
]);

/** Whether a window is shown in `env`. An id is a member of a set if EITHER
 * its exact id or its family is listed, so one entry covers a family's bare
 * parent and all its spawned instances (`ShipCargo` and
 * `ShipCargo_1033391582929`; `overview` and `overview_1`).
 *
 * No `detail !== ""` check, unlike isClutter: that check tells a spawned
 * instance from its bare parent, and for environment purposes they are in the
 * same place. */
export function inEnv(id: string, env: Env): boolean {
  if (env === "all") return true;
  const family = describe(id).family;
  const has = (s: ReadonlySet<string>) => s.has(id) || s.has(family);
  return env === "docked" ? !has(SPACE_ONLY) : !has(DOCKED_ONLY);
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd app && npm test`
Expected: PASS — `windowLabels.test.ts ok`, and every other test file still green.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/windowLabels.ts app/src/lib/windowLabels.test.ts
git commit -m "Map windows to docked / in-space environments

Only the exclusives are curated; an unlisted id shows in both views, the
same safe-failure direction as the clutter tables. Family membership covers
a bare parent and its spawned instances with one entry."
```

---

### Task 2: `env` on the shared filter

**Files:**
- Modify: `app/src/lib/layout.ts:336-352` (the `WindowFilter` interface, `NO_FILTER`, `filterIsActive`) and `layout.ts:367-377` (`windowMatches`)
- Test: `app/src/lib/layout.test.ts` (the `--- the shared filter predicate ---` block, ~lines 157-190)

**Interfaces:**
- Consumes: `inEnv(id, env)` and `type Env` from Task 1.
- Produces:
  - `WindowFilter` gains `env: Env`
  - `NO_FILTER` gains `env: "all"`
  - `export type { Env }` re-exported from `layout.ts`

- [ ] **Step 1: Write the failing test**

Append inside the existing `--- the shared filter predicate ---` block in `app/src/lib/layout.test.ts`, after the last `hideClutter` check (~line 190). The `market`, `bareCargo` and `spawnedCargo` fixtures are already declared at the top of that block — reuse them; do not redeclare.

```ts
  const lobby = win("lobbyWnd", true, true);
  const dscan = win("directionalScannerWindow", true, true);

  check("the default env does not make the filter active", !filterIsActive({ ...NO_FILTER, env: "all" }));
  check("a docked env makes the filter active", filterIsActive({ ...NO_FILTER, env: "docked" }));
  check("a space env makes the filter active", filterIsActive({ ...NO_FILTER, env: "space" }));

  check("docked keeps a docked-only window", windowMatches(lobby, { ...NO_FILTER, env: "docked" }));
  check("docked drops a space-only window", !windowMatches(dscan, { ...NO_FILTER, env: "docked" }));
  check("space keeps a space-only window", windowMatches(dscan, { ...NO_FILTER, env: "space" }));
  check("space drops a docked-only window", !windowMatches(lobby, { ...NO_FILTER, env: "space" }));
  check("an unlisted window survives both envs",
    windowMatches(market, { ...NO_FILTER, env: "docked" }) && windowMatches(market, { ...NO_FILTER, env: "space" }));

  // env composes with the other dimensions rather than replacing them.
  check("env and openOnly compose", !windowMatches(closedMarket, { ...NO_FILTER, env: "docked", openOnly: true }));
  check("env and text compose", !windowMatches(lobby, { ...NO_FILTER, env: "docked", text: "zzz" }));

  check("visibleIds narrows by env",
    !visibleIds([lobby, dscan, market], { ...NO_FILTER, env: "space" }).has("lobbyWnd"));
  check("visibleIds keeps the space window and the unlisted one",
    visibleIds([lobby, dscan, market], { ...NO_FILTER, env: "space" }).size === 2);
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd app && npm test`
Expected: FAIL — TypeScript/Node will reject `env` as an unknown property on `WindowFilter`, or `filterIsActive({...NO_FILTER, env: "docked"})` returns `false`.

- [ ] **Step 3: Write minimal implementation**

In `app/src/lib/layout.ts`, extend the import on line 4 and re-export the type:

```ts
import { isClutter, inEnv, nameOf, type ClutterOverrides, type Env } from "./windowLabels.ts";

/** Re-exported so callers that already import from layout.ts get it here.
 * It is DECLARED in windowLabels.ts, beside the mapping it belongs to —
 * layout.ts imports from windowLabels.ts and not the reverse. */
export type { Env };
```

Extend the interface and its constant (`layout.ts:336-347`):

```ts
export interface WindowFilter {
  /** Free text, matched against the friendly label, the detail and the raw id. */
  text: string;
  /** Drop windows EVE has not flagged open (roughly 77% of a real file). */
  openOnly: boolean;
  /** Drop windows EVE spawns per conversation, item or dialog — clutter, not
   * windows the player placed. Applies whether open or closed: kind of
   * window is the axis, not open/closed (see isClutter). */
  hideClutter: boolean;
  /** Show only the windows that exist in one environment. `all` — the default
   * — is today's mixed picture. Windows the mapping does not recognise show in
   * every environment, so this narrows rather than hides (see inEnv). */
  env: Env;
}

export const NO_FILTER: WindowFilter = { text: "", openOnly: false, hideClutter: false, env: "all" };
```

Add the two guards:

```ts
export function filterIsActive(f: WindowFilter): boolean {
  return f.text.trim() !== "" || f.openOnly || f.hideClutter || f.env !== "all";
}
```

and in `windowMatches`, after the `hideClutter` guards:

```ts
  if (!inEnv(w.id, f.env)) return false;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd app && npm test && npm run check`
Expected: PASS. `npm run check` is the one that catches any remaining `WindowFilter` literal built without `env` — every existing site spreads `NO_FILTER`, so there should be none, but confirm rather than assume.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/layout.ts app/src/lib/layout.test.ts
git commit -m "Add the environment dimension to WindowFilter

One guard in the shared predicate, so the list and the canvas narrow
together exactly as they already do for Hide clutter."
```

---

### Task 3: The Inventory fold (`linkInventory`)

**Files:**
- Modify: `app/src/lib/layout.ts` (add after `drawnWindowCount`, ~line 124)
- Test: `app/src/lib/layout.test.ts` (append a new block after the existing `stackUnits` blocks, before the filter-predicate block)

**Interfaces:**
- Consumes: `DrawUnit` (`layout.ts:59-68`) with fields `key`, `anchor`, `stack`, `tabs`, `fanTargets`; `type Env` from Task 1.
- Produces: `export function linkInventory(units: DrawUnit[], env: Env): DrawUnit[];`

**Why this exists:** Inventory is the only window family EVE splits per context, and on a real character the copies have drifted apart (`InventoryStation` at 624,260 623×450 vs `InventoryStructure` at 136,285 880×619). In the Docked view they would paint two rectangles 488px apart for what the player thinks of as one window. Folding them means one drag repositions both — the stated default, "move it in both environments at once".

- [ ] **Step 1: Write the failing test**

Add to `app/src/lib/layout.test.ts`. Add `linkInventory` to the existing import from `./layout.ts` on lines 3-6.

```ts
// --- the Inventory fold -----------------------------------------------------
{
  const station = win("InventoryStation", true, true);
  const structure = win("InventoryStructure", true, true);
  const market = win("market", true, true);
  const layout = { reference_w: 2560, reference_h: 1440, windows: [station, structure, market], stacks: [] };

  const all = linkInventory(stackUnits(layout as any), "all");
  check("all leaves both Inventory units alone", all.filter((u) => u.key.startsWith("Inventory")).length === 2);
  check("all leaves each fanning only to itself",
    all.every((u) => !u.key.startsWith("Inventory") || u.fanTargets.length === 1));

  const docked = linkInventory(stackUnits(layout as any), "docked");
  const inv = docked.filter((u) => u.key.startsWith("Inventory"));
  check("docked paints one Inventory rectangle", inv.length === 1);
  check("docked keeps the station copy as the anchor", inv[0].key === "InventoryStation");
  const ids = inv[0].fanTargets.map((w) => w.id).sort();
  check("docked fans a drag onto both copies", ids.join(",") === "InventoryStation,InventoryStructure");
  check("the fold leaves unrelated units untouched", docked.some((u) => u.key === "market"));

  // The space copy is a different id and is never folded — it is a genuinely
  // separate window with its own position, and it is not in the docked view.
  const spaceOnly = { ...layout, windows: [win("InventorySpace", true, true), market] };
  const space = linkInventory(stackUnits(spaceOnly as any), "space");
  check("the space copy is left alone", space.filter((u) => u.key === "InventorySpace").length === 1);

  // Partial presence: only one of the pair drawn (the other closed, or
  // filtered out). There is nothing to fold, and the survivor must still draw.
  const lone = { ...layout, windows: [structure, market] };
  const folded = linkInventory(stackUnits(lone as any), "docked");
  check("a lone structure copy still draws", folded.some((u) => u.key === "InventoryStructure"));
  check("a lone copy fans only to itself",
    folded.find((u) => u.key === "InventoryStructure")!.fanTargets.length === 1);

  // A stacked Inventory already fans to its stack; merging across stacks is
  // out of scope, so the fold declines rather than guessing. Note the stacked
  // copy's unit is keyed by its CONTAINER, so it never matches the fold's
  // `key === "InventoryStation"` test in the first place — this pins that.
  const stacked = {
    reference_w: 2560, reference_h: 1440,
    stacks: [{ container_id: "C", container_label: "C", anchor_id: "C", members: ["InventoryStation"] }],
    windows: [
      win("C", true, true, { container_id: "C", role: "container" }),
      win("InventoryStation", true, true, { container_id: "C", role: "member" }),
      win("InventoryStructure", true, true, null),
      market,
    ],
  };
  const untouched = linkInventory(stackUnits(stacked as any), "docked");
  check("a stacked Inventory is not folded", untouched.some((u) => u.key === "InventoryStructure"));
  check("the stacked copy still draws as its stack", untouched.some((u) => u.key === "C" && u.stack));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd app && npm test`
Expected: FAIL — `linkInventory` is not exported from `./layout.ts`.

- [ ] **Step 3: Write minimal implementation**

Add to `app/src/lib/layout.ts` after `drawnWindowCount`:

```ts
/**
 * Fold the two docked Inventory copies into one drawn rectangle.
 *
 * Inventory is the ONLY window family EVE splits per context: the character
 * file carries `InventoryStation`, `InventoryStructure` and `InventorySpace` as
 * three separate ids with three separate geometries in the otherwise-flat
 * `windowSizesAndPositions_1`. On a real character they have drifted apart
 * (624,260 623x450 vs 136,285 880x619), so the docked view would paint two
 * rectangles 488px apart for what the player thinks of is one window.
 *
 * In `docked` the structure copy is dropped as its own unit and appended to the
 * station copy's `fanTargets` — which is already "every window a coherent move
 * must repeat the rect onto", so the existing commit path repositions both from
 * one drag with no new drag code.
 *
 * `all` is deliberately left untouched: three independent rectangles, exactly
 * as today. That IS the escape hatch for a player who wants the station and
 * structure inventories in different places, so there is no toggle to build.
 *
 * A post-pass rather than a parameter on stackUnits: it keeps the grouping and
 * the fold separately testable, and it cannot affect the unfiltered denominator
 * `LayoutView` computes for "showing N of M".
 */
export function linkInventory(units: DrawUnit[], env: Env): DrawUnit[] {
  if (env !== "docked") return units;
  const station = units.find((u) => u.key === "InventoryStation" && !u.stack);
  const structure = units.find((u) => u.key === "InventoryStructure" && !u.stack);
  // Nothing to fold: one of the pair is closed, filtered out, or stacked (a
  // stacked copy already fans to its stack, and merging across stacks is out
  // of scope). The survivor draws on its own.
  if (!station || !structure) return units;
  const linked = { ...station, fanTargets: [...station.fanTargets, ...structure.fanTargets] };
  return units.filter((u) => u !== structure).map((u) => (u === station ? linked : u));
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd app && npm test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/layout.ts app/src/lib/layout.test.ts
git commit -m "Fold the two docked Inventory copies into one rectangle

Inventory is the only context-split family in the geometry dict, and its
copies drift. In the docked view one drag now repositions both, reusing
DrawUnit.fanTargets. The All view keeps them independent, which is the
escape hatch."
```

---

### Task 4: The view selector

**Files:**
- Modify: `app/src/lib/WindowPanel.svelte:319-337` (the `.filters` row markup) and its `<style>` block (`.toggle` rule ends ~line 516)
- Modify: `app/src/lib/LayoutView.svelte:83-85` (the `units` derivation)

**Interfaces:**
- Consumes: `filter.env` (Task 2), `linkInventory` (Task 3).
- Produces: no new exports. `WindowPanel` already receives `filter` as `$bindable`, and `LayoutView` already owns the `filter` state — the radios bind to the same prop the two checkboxes bind to, so nothing new is wired.

- [ ] **Step 1: Add the radio row to `WindowPanel.svelte`**

Insert after the `Hide clutter` `<label>`, still inside `<div class="filters">`:

```svelte
    <div
      class="envs"
      role="radiogroup"
      aria-label="Environment"
      title="Shows only the windows that exist in one environment. Station and player structure are one “Docked” view — EVE stores a single position per window, so this filters the picture, it does not switch layouts. A window the editor does not recognise shows in both.">
      {#each [["all", "All"], ["docked", "Docked"], ["space", "In space"]] as const as [value, label]}
        <label class="toggle">
          <input type="radio" name="env" {value} bind:group={filter.env} />
          {label}
        </label>
      {/each}
    </div>
```

- [ ] **Step 2: Add the style**

Add after the `.toggle input` rule in `WindowPanel.svelte`'s `<style>`:

```css
  .envs {
    display: flex;
    gap: 0.6rem;
  }
```

- [ ] **Step 3: Apply the fold in `LayoutView.svelte`**

Extend the import on lines 6-8 with `linkInventory`, then change the `units` derivation (`LayoutView.svelte:83-85`) from:

```svelte
  const units = $derived(
    stackUnits(layout ?? { reference_w: 0, reference_h: 0, windows: [], stacks: [] }, visible),
  );
```

to:

```svelte
  const units = $derived(
    linkInventory(
      stackUnits(layout ?? { reference_w: 0, reference_h: 0, windows: [], stacks: [] }, visible),
      filter.env,
    ),
  );
```

Leave `allUnits` alone. It is the unfiltered denominator for "showing N of M", so by definition it is the `all` view — folding it would change M as the player switches environments.

- [ ] **Step 4: Verify**

Run: `cd app && npm test && npm run check`
Expected: PASS, no svelte-check errors.

Then run the app and confirm by eye — this is the part no unit test covers:
1. Open a character with a real layout, go to the Layout view.
2. Default is `All`, and the canvas looks exactly as it did before.
3. Pick `In space`: the Overview, D-Scan, drone and cargo windows remain; `lobbyWnd` / Clone Bay / the structure hangars are gone; the `showing N of M` counter appears with N < M.
4. Pick `Docked`: the mirror, and **one** Inventory rectangle rather than two.
5. In `Docked`, drag Inventory, then switch to `All`: both `Inventory (Station)` and `Inventory (Structure)` are now at the dropped position.
6. Switch back to `All`: the counter's `reset` link clears the env back to `all` along with the other dimensions (it assigns `{ ...NO_FILTER }`, so this should already hold — confirm it does).

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/WindowPanel.svelte app/src/lib/LayoutView.svelte
git commit -m "Add the All / Docked / In space view selector

Radios rather than a select, so the active view is readable at a glance
beside the two existing toggles. They bind to the same filter prop, so the
canvas and the list narrow together with no new wiring."
```

---

### Task 5: Close the ledger entry and note the release

**Files:**
- Modify: `docs/small-tasks.md:288` (the `Per-environment canvas views` entry)
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Tick the ledger entry**

Change the entry's `- [ ]` to `- [x]` and append a closing line recording what shipped and what did not:

```markdown
  **Done 2026-07-30.** Shipped as a two-environment view filter (docked / in
  space) on `WindowFilter`, with a curated exclusives table in
  `windowLabels.ts` where an unlisted id shows in both views. Inventory — the
  only context-split family in the geometry dict — folds to one rectangle in
  the docked view and fans a drag onto both copies. Still open, deliberately:
  splitting NPC station from player structure, per-window user overrides of
  the env table, and the in-game dock/undock capture that would replace the
  curated mapping with a measured one (live-verification item 35).
```

- [ ] **Step 2: Add the changelog entry**

Follow the file's existing format for the current unreleased section (read the top of `CHANGELOG.md` first and match it exactly — do not invent a version number if the top section is already open).

```markdown
### Added

- Layout canvas: an `All / Docked / In space` view selector. The canvas mixed
  every environment into one picture, painting windows that can never be on
  screen together. The mapping is curated and deliberately partial — a window
  the editor does not recognise shows in both views. Inventory, the one window
  EVE stores per context, draws as a single rectangle when docked and a drag
  repositions the station and structure copies together.
```

- [ ] **Step 3: Commit**

```bash
git add docs/small-tasks.md CHANGELOG.md
git commit -m "Close the per-environment canvas ledger entry"
```

---

## Self-Review

**Spec coverage:**

| Spec section | Task |
|---|---|
| §4 filter dimension, `Env` re-export | Task 2 |
| §5 exclusives table + `inEnv` family rule | Task 1 |
| §6 Inventory fold, `All` as the escape hatch | Task 3 |
| §7 three-way control, `All` default | Task 4 |
| §8 tests 1-3 (`inEnv`) | Task 1 |
| §8 test 4 (`filterIsActive`) | Task 2 |
| §8 test 5 (`linkInventory`) | Task 3 |
| §8 "no component test for the radio row" | Task 4 — deliberately none |
| §9 live verification | Not blocking; Task 4 Step 4 covers the by-eye check, the in-game capture stays ledgered in Task 5 |

**Type consistency:** `Env` is declared once in `windowLabels.ts` (Task 1), imported by `layout.ts` and re-exported (Task 2), consumed by `linkInventory` (Task 3) and bound in the markup (Task 4). `inEnv(id, env)` keeps the same signature everywhere. `DrawUnit.fanTargets` is `WindowRect[]` in both `stackUnits` and `linkInventory`.

**Known gap accepted:** §3 lists per-window env overrides and the station/structure split as out of scope; Task 5 re-files both on the ledger rather than leaving them silently dropped.
