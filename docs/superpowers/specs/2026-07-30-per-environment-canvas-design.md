# Per-environment canvas views — docked vs in space (design)

Status: designed 2026-07-30, not yet planned.

Milestone context: the **layout depth** milestone. This is the ledger item at
`docs/small-tasks.md:288`, deferred out of the names-and-noise slice
(`2026-07-26-layout-names-and-noise-design.md` §2) on the grounds that mapping
windows to environments needed either curation or an in-game capture. This spec
takes the curation route, and §3 records why the capture is no longer blocking.

## 1. Goal

A player's screen differs by environment, and the canvas mixes every environment
into one picture. That is part of why it paints far more windows than are ever
visible at once: the Overview and the ship cargo hold cannot be on screen at the
same time as the station Clone Bay, but the canvas draws all three.

Give the canvas a view selector — **All / Docked / In space** — that shows only
the windows relevant to the chosen environment.

## 2. What the files actually carry

Measured 2026-07-30 across the whole corpus (6,502 `core_char_*.dat` files in 32
snapshots) plus the `exp1-char-after.txt` text dump of a real character. Read
this before changing the mapping — it is the evidence the design rests on.

### 2.1 EVE's own context list

`ui → InfoPanelModes_<context>` is the client naming its own environments.
Counts are char files carrying the key:

| context | files | what it is |
|---|---|---|
| `hangar` | 5995 | docked, NPC station |
| `charsel` | 5697 | character select screen |
| `inflight` | 5536 | in space |
| `structure` | 5324 | docked, player structure |
| `ActivityTracker` | 5024 | overlay (+ `_dockablePanel`, 2919) |
| `skill_plan` | 4316 | skill planning screen |
| `None` | 205 | fallback bucket |
| `starmap_new` | 200 | new star map |
| `planet` | 190 | planet view / PI |
| `systemmap_new` | 136 | new system map |
| `charactercreation` | 96 | |
| `shiptree` | 86 | (+ `ShipTree_dockablePanel`, 86) |
| `starmap` | 72 | legacy star map |

Only `inflight` / `hangar` / `structure` are places with an arrangeable window
layout. The rest are full-screen modes or overlays, so a view for them would be
empty by construction.

Two other key families corroborate the docking split:
`infowindow_type_station` (129) / `infowindow_type_structure` (152), and
`dockPanels`' `widthProportion_docked` / `heightProportion_docked` (5528 / 5408)
— note the latter is a **two**-state split, docked vs not.

### 2.2 Geometry is flat, with exactly one exception

`windows → windowSizesAndPositions_1` is keyed by window id with no
per-environment copies. **Inventory is the only context-split family** in it —
and on a real character the three copies have genuinely drifted apart:

| id | x, y | w × h |
|---|---|---|
| `InventorySpace` | 256, 538 | 675 × 447 |
| `InventoryStation` | 624, 260 | 623 × 450 |
| `InventoryStructure` | 136, 285 | 880 × 619 |

Every other window has one geometry shared by every environment. Enumerating all
~200 geometry keys on that character turned up no other context pair: the
structure hangar windows (`StructureItemHangar`, `StructureShipHangar`,
`DeliverToStructure`) are structure-*only* ids with no station twin, and
`lobbyWnd` / `CloneStationWindow` / `cloneBay` are station-only.

### 2.3 Consequence for the design

This is a **view filter over a single layout**, not a new data model and not a
backend change. And "moving a window in both environments at once" — the project
owner's stated default — is already true for ~290 of ~296 windows, because there
is physically one number to move. It is a live choice for Inventory alone.

## 3. Scope

Two environments, not three: **Docked** collapses NPC station and player
structure. The project owner's call, and it matches what `dockPanels` stores.
The cost is that `lobbyWnd` and `StructureItemHangar` share a view despite being
different screens; the benefit is halving the mapping surface and avoiding a
third view whose exclusives are three windows.

In scope:

1. A third dimension on `WindowFilter`, applied to the list and the canvas
   together, exactly as `hideClutter` already is.
2. A curated exclusives table in `windowLabels.ts`.
3. The Inventory link: in the Docked view, one Inventory rectangle whose drag
   writes to both `InventoryStation` and `InventoryStructure`.
4. A three-way control in the window panel's filter row.

Out of scope, and why:

- **NPC station and player structure as separate views.** Folded into Docked
  per §3 above. Re-openable: the exclusives table already distinguishes them by
  id, so splitting Docked later is a table change plus a third radio.
- **Per-window user overrides of the env table.** `hideClutter` has them
  (`ClutterOverrides`, `preferences.json`) and this could reuse the machinery,
  but the exclusives list is ~15 curated entries against clutter's ~70 plus
  families, and an unlisted id fails safe into both views. Add when a mis-mapped
  id actually bites someone.
- **The dock/undock capture session** (live-verification ledger item 35). It
  would measure the mapping instead of curating it, but it needs two full login
  cycles because `windowSizesAndPositions_1` is only written on logout. The
  curated table ships without it and the capture can correct it later.
- **An unlink toggle for the Inventory pair.** §5 gets it for free.
- **Per-environment *geometry*.** The format does not store it (§2.2). Nothing
  to build, and building it would mean inventing keys EVE does not read.

## 4. The filter dimension

```ts
export type Env = "all" | "docked" | "space";
```

`Env` is declared in `windowLabels.ts` beside `inEnv` (§5) and re-exported from
`layout.ts` for callers that already import from there. It cannot live in
`layout.ts`: `layout.ts` imports from `windowLabels.ts` and not the reverse, and
`inEnv` needs the type.

`WindowFilter` gains `env: Env`; `NO_FILTER` sets `"all"`; `filterIsActive`
adds `f.env !== "all"`; `windowMatches` adds one `inEnv(w.id, f.env)` guard
alongside the existing clutter and open-only guards.

Nothing else needs wiring. The canvas already narrows through
`visibleIds` → `stackUnits(layout, visible)` (`LayoutView.svelte:82-85`), the
list already calls `windowMatches` per row, and the "showing N of M" counter
already counts what `stackUnits` draws. All three pick this up unchanged.

## 5. The mapping

A pair of sets in `windowLabels.ts`, beside `CLUTTER_IDS`, plus:

```ts
export function inEnv(id: string, env: Env): boolean;
```

`DOCKED_ONLY`: `lobbyWnd`, `cloneBay`, `CloneStationWindow`,
`CloneUpgradeWindow`, `InventoryStation`, `InventoryStructure`,
`StructureItemHangar`, `StructureShipHangar`, `StructureCorpHangar`,
`DeliverToStructure`.

`SPACE_ONLY`: `InventorySpace`, `droneview`, `selecteditemview`,
`directionalScannerWindow`, `overview`.

Unlike `DOCKED_ONLY`, nothing in `SPACE_ONLY` is corroborated by the corpus
measurement in §2.2 — that pass only turned up docked-side exclusives.
`SPACE_ONLY` is game-knowledge curation, not measured data, and should not be
read as if it were. `ShipCargo` and `ShipDroneBay` were considered for it and
deliberately left out: a docked player can open the active ship's cargo hold
and drone bay from the station hangar, so they are not space-exclusive, and
the cost of guessing wrong here is hiding a window the player actually has
open — the one direction §3's safe-failure rule forbids. They fail safe into
both views instead, same as any other unlisted window.

An id is a member if **either** its exact id or its `describe(id).family` is in
the set. Both `overview` and `overview_1` have family `overview`, so one entry
covers a family's bare parent and every spawned instance.

Unlike `isClutter`, there is no `detail !== ""` check: that check exists to tell
a spawned instance from its bare parent, and for environment purposes the two
belong to the same place. So the rule here is plain set membership.

**An unlisted id shows in both views.** This is the same safe-failure direction
`isClutter` documents: a wrong guess paints a harmless extra rectangle rather
than hiding a window the player actually placed. Windows whose environment is
genuinely uncertain — Fitting, Assets, Market, Watchlist, the chat stack — are
deliberately *absent* from both sets rather than guessed at. The tables are
meant to grow lazily as real ids show up, exactly like `CURATED` and `PARAM`.

## 6. The Inventory link

In the Docked view the two docked Inventory ids must not paint two rectangles at
two different places (§2.2 shows they currently would, 488px apart). They fold
into one unit:

```ts
linkInventory(units: DrawUnit[], env: Env): DrawUnit[]
```

A pure post-pass over `stackUnits`' output — no signature change to `stackUnits`
itself, so it stays unit-testable on its own and the change has a small blast
radius. In `"docked"` it drops the `InventoryStructure` unit and appends that
window to the `InventoryStation` unit's `fanTargets`.

`fanTargets` is already precisely "every window a coherent move must repeat the
rect onto" (`layout.ts:66`), and `LayoutView.svelte:601` already commits
`unit.fanTargets.flatMap((w) => geomMutations(w, next))`. So one drag writing to
both ids needs no new drag, preview or commit code.

**The escape hatch is free.** `All` is left exactly as it is today: three
separate Inventory rectangles, independently positionable. Docked links them,
All does not, and there is no toggle to build. A player who genuinely wants the
station and structure inventories in different places works in All.

Note the first Docked drag reconciles the two windows — that is the intent
("move both at once to reduce duplicating work"), not a side effect, and it
writes the anchor's full `{x, y, w, h}` to both ids, so it **resizes as well
as repositions** the structure copy. This application has **no undo** — the
only occurrences of the word in the codebase are prose warnings that an action
"can't be undone" — so recovery from an unwanted reconciliation is the
whole-session Discard button (which throws away every unsaved edit, not just
this one) or restoring from a backup.

## 7. UI

A three-way `All | Docked | In space` control in `WindowPanel.svelte`'s filter
row, beside the `Open only` and `Hide clutter` checkboxes, bound to the same
`filter` prop the checkboxes already bind to. Radio inputs rather than a
`<select>`, so all three states are visible without opening anything, and so the
active view is readable at a glance next to the two toggles.

`All` is the default, so the canvas opens exactly as it does today.

## 8. Testing

`layout.test.ts` (pure, `node --test`, like the rest of the file):

1. `inEnv` admits a `DOCKED_ONLY` id in `docked` and rejects it in `space`, and
   the mirror for `SPACE_ONLY`.
2. A suffixed id (`overview_1`) follows its family.
3. An unlisted id (`market`) is admitted in **both** environments — the
   safe-failure direction, which is the property most likely to regress if
   someone later "tidies" the tables into a total mapping. `ShipCargo` and
   `ShipDroneBay` are pinned the same way: considered for `SPACE_ONLY` and
   deliberately left out (§5), so they fail safe into both views too.
4. `filterIsActive` is true for a non-`all` env with no other filter set.
5. `linkInventory` in `docked` yields one Inventory unit whose `tabs` and
   `fanTargets` carry both `InventoryStation` and `InventoryStructure`; in
   `all` it yields the units unchanged.

No component test for the radio row — it binds to the same prop the existing
checkboxes do, and there is no component test for this panel at all (nine
other `*.spec.ts` files exist; `WindowPanel.spec.ts` is not one of them).

## 9. Live verification

Not blocking, but worth one check the next time the client is up: dock, note
which windows are on screen, undock, note again, and diff against what the two
views claim. That is the ledger's item 35 in its cheap form — it corrects the
curated table without needing the two logout cycles a *geometry* capture does.
