# Layout depth — canvas & list usability (design)

Status: designed 2026-07-26, not yet planned.

Milestone context: the **layout depth** milestone, cut into four slices (see the
HUD furniture spec §7). Slice 3 (HUD furniture) shipped as v0.15.0. This spec
covers **slice 1a** — taming the window list and the canvas pile-up. The
precision-editing half of slice 1 (grid snap with a hold-to-disable modifier,
arrow-key nudge, snapping to canvas and window edges) is deliberately split out
as **slice 1b** and is not designed here: it shares no code with this half.

## 1. Goal

The Layout editor is unusable at real scale. Measured over the corpus
(`testdata/corpus`, character files with ≥ 50 windows, 316 files):

| | median | max |
|---|---|---|
| rows in the window list | **296** | 381 |
| rectangles drawn on the canvas | **68** | 84 |
| distinct id prefixes ("families") | 139 | — |

Only ~23 % of a character's windows are flagged open, and those are what the
canvas draws — 68 overlapping blue boxes, most of them chat noise the player has
never positioned deliberately. The list is worse: 296 rows labelled with raw
window ids (`ChannelSettingsDlg_fleet_1038711647935`,
`('corpassets', 1037014587783L)`, `76`), no filter, no grouping.

The window id families, by share of all window rows across the corpus:

```
160891  chatchannel_*          25967  contactmanagement_*
 87063  ChatInvitation_*       20801  <numeric>  (stack containers)
 63691  ChannelSettingsDlg_*    7793  ViewFitting
 35521  mail_*                  7513  overview*
 28041  groupInfoWnd_*          6728  bookmarkLocationWindow
```

Six families account for the large majority of the noise. Everything below hangs
off that: name what we can, fold what repeats, filter what's left, and let the
filter reach the canvas so decluttering the list declutters the picture too.

Note that prefix-grouping *alone* is not the answer — 296 rows fold to 139
families, which is still an unusable list. The compression has to come from
folding **and** filtering together.

## 2. Scope

In scope:

1. Friendly window labels, derived in the frontend.
2. Folding repeated instance families in the list.
3. A filter box plus two toggles, driving the list **and** the canvas.
4. A right-click context menu replacing the direct jump-to-tree (the M2 spec's
   deferred item).

Out of scope, and why:

- **Slice 1b** — snap-to-grid, arrow-key nudge, edge/window snapping. Separate
  spec, separate PR; it touches `layout.ts` geometry and the drag handlers,
  which this slice does not.
- **Friendlier stack labels from `tabgroups`.** The *account* file's
  `tabgroups` section stores `<containerId>_names` → EVE's own tab label
  (`"Character: Information"`), which would be strictly better than anything we
  can derive. Taking it needs `window_layout` to accept the user root, which
  ripples through `ops.rs` and every call site. Ledger it; this slice labels a
  stack from its anchor window instead. (Supersedes the ledger's
  "friendlier stack-frame labels" item only in part — the item stays open.)
- **Any backend or wire-format change.** `WindowRect.label` keeps carrying the
  raw id; the friendly name is a pure frontend projection of it. No new Rust.

## 3. Labels — `app/src/lib/windowLabels.ts` (new, pure, unit-tested)

One exported function, no state, no DOM — the same shape as `layout.ts` and
`search.ts`, tested by `windowLabels.test.ts` under node `--test`.

```ts
export interface WindowName {
  /** Friendly display name: "Chat", "Market", "Mail message". */
  label: string;
  /** The instance discriminator, shown dim beside the label; "" when singular. */
  detail: string;
  /** Grouping key — every id with the same family folds into one group. */
  family: string;
}
export function describe(id: string): WindowName;
```

Resolution order, first match wins:

1. **Stringified Python tuple** — `('corpassets', 1037014587783L)`,
   `('myPlaces', (12345, None))`, `('RolesSummary', 'Container Access')`. Take
   the first element as the family, look it up in `CURATED`, and render the
   remainder as `detail`. Parsing is deliberately shallow: a regex for the
   leading `('name'` and the rest as an opaque string. These ids are display
   material only; nothing writes them.
2. **All-digits id** — a minted stack container. `label: "Window stack"`,
   `detail: <id>`, `family: "stack"`.
3. **Parameterized family** — the id starts with a known `PARAM` prefix
   followed by `_`. The prefix's curated label becomes `label`, the suffix
   becomes `detail`. **Longest prefix wins**: `mail_readingWnd_380729425` is a
   Mail message, not a `mail` instance, and adding a shorter overlapping prefix
   later must not change an existing longer match. Seed table (extend as the
   corpus shows more):

   | prefix | label |
   |---|---|
   | `chatchannel` | Chat |
   | `ChannelSettingsDlg` | Chat settings |
   | `ChatInvitation` | Chat invitation |
   | `mail_readingWnd` | Mail message |
   | `contactmanagement` | Contacts |
   | `groupInfoWnd` | Info |
   | `ShipCargo` | Ship cargo |
   | `ShipDroneBay` | Drone bay |
   | `StructureShipHangar` | Ship hangar |
   | `containerWnd` | Container |
   | `overview` | Overview |

   `chatchannel_player_-78564080` yields `detail: "player"` — the suffix is
   truncated at the first segment that is numeric, hex-GUID-shaped, or negative,
   because the raw ids are long and meaningless to the reader. The untruncated
   id remains available (see §5).

4. **Curated singleton** — exact-id lookup in `CURATED`. Seed set, drawn from
   the corpus's most common ids (the plan writes them all out; this is the
   naming convention, not an exhaustive list):

   `overview` → Overview · `market` → Market · `fittingWnd`/`ViewFitting` →
   Fitting · `charactersheet` → Character Sheet · `assets` → Assets ·
   `walletWindow` → Wallet · `droneview` → Drones · `selecteditemview` →
   Selected Item · `watchlistpanel` → Watchlist · `fleetwindow` → Fleet ·
   `mail` → EVE Mail · `notepad` → Notepad · `mapbrowser` → Map Browser ·
   `directionalScannerWindow` → Directional Scanner ·
   `probeScannerFilterEditor` → Scanner Filters · `InventoryStation` →
   Inventory (Station) · `InventorySpace` → Inventory (Space) ·
   `InventoryStructure` → Inventory (Structure) · `corporation` → Corporation ·
   `addressbook` → People & Places · `contracts` → Contracts · `redeem` →
   Redeem Queue · `AgencyWndNew` → Agency · `StructureBrowser` → Structure
   Browser · `KillReportWnd` → Kill Report · `infowindow` → Show Info ·
   `ChatWindowStack` → Chat stack · `invitestack` → Invitation stack ·
   `overviewsettings` → Overview Settings · `logger` → Combat Log ·
   `previewWnd` → Preview · `typecompare` → Compare Tool · `help` → Help.

5. **Mechanical fallback** — strip a trailing/embedded `Wnd`, `Window`, `Dlg`,
   `Panel`, `View`, `New`; split camelCase and `_`; title-case each word.
   `BugReportingWindow` → "Bug Reporting", `attributerespecification` →
   "Attributerespecification" (all-lowercase runs cannot be split — accepted,
   the raw id is always visible and the curated table is the fix).

The fallback is the reason the curated tables can stay small and grow lazily: an
unrecognised id is ugly, never wrong.

## 4. Fold and filter — two different mechanisms

This distinction is the crux of the design and must not be blurred in
implementation.

**Folding is list presentation only.** Any family with more than one member
renders as one collapsible `<details>` row — `Chat · 47` — with the members
inside. Singleton families render as plain rows, as today. Folded by default.
Folding **never** changes what the canvas draws: collapsing a group must not
make rectangles disappear, or the canvas stops being a picture of the screen.

**Filtering is a shared predicate**, owned by `LayoutView.svelte` and applied to
both the window list and the input to `stackUnits`:

- a text box, matching case-insensitively against `label`, `detail` and the raw
  id, so `market`, `corpassets` and `1037014587783` all find what you expect
  (the same contract `search.ts` documents for the tree);
- `Open only` — drops windows without the `openWindows` flag (296 rows → ~68);
- `Hide chat & session windows` — drops the six noise families named in §1
  (canvas ~68 rects → ~20 in one click).

All three compose into one `visibleIds: Set<string>` derived in `LayoutView`.

**Nothing hides silently.** Whenever the predicate is narrowing, a line under
the canvas reads `showing 24 of 68 windows · reset`, where `reset` clears all
three controls. The existing `reference 2560×1440` line sits beside it.

A stack draw-unit survives if **any** of its members is visible, and its tab
strip shows only the visible members — a filter that matched one tab must not
erase the whole stack from the canvas. The unit's **anchor is chosen exactly as
today, ignoring the filter**: the anchor is where the rectangle's geometry comes
from and what a drag fans out from, so filtering it out would move the stack or
lose the drag target. Only the tab strip and the list are filtered; `fanTargets`
is likewise unfiltered, or a drag under an active filter would leave the hidden
members behind.

The default state — empty text box, both toggles off, families folded — is
exactly today's canvas with a much shorter list. Decluttering the canvas is an
opt-in the counter advertises.

## 5. Context menu — `app/src/lib/ContextMenu.svelte` (new)

Today `WindowPanel` binds `oncontextmenu` on each coordinate label and flag to
jump straight into the raw tree, with a `TODO(revisit)` saying a menu was
intended (the M2 deferral, and the small-tasks ledger's "panel right-click
should be a context menu" item — this slice closes it).

A minimal component: `{ x, y, items: { label: string; run: () => void }[] }`,
rendered as an absolutely-positioned list, closing on outside pointerdown,
Escape, or after an item runs. No portals, no nesting, no submenus, no
icons — one flat list.

Items, by target:

- a **window row**: *Show geometry in tree*, *Copy window id*, *Select on
  canvas*;
- a **coordinate field or flag**: *Show in tree* (the current behaviour, now
  named), *Copy window id*.

*Copy window id* copies the raw id — the escape hatch that makes friendly labels
safe, since the id is what `format-notes.md` and the raw tree speak.

## 6. Testing

- `windowLabels.test.ts` (node `--test`, zero-dep, matching the existing
  frontend tests): one case per resolution rule in §3, including the tuple id,
  the negative-suffix chat id, an all-digits id, a curated hit, and a fallback
  id; plus the invariant that `describe` never returns an empty `label`.
- `layout.test.ts` gains cases for the shared predicate: a stack whose single
  matching member keeps the unit alive with only that tab; a filter matching
  nothing yields no units; the unfiltered default returns exactly what
  `stackUnits` returns today (the no-regression case).
- No Rust changes, so no new backend tests. `tests/hud_corpus.rs` (added when
  the badge-section bug was caught) already guards the layout-adjacent backend.
- **Live smoke**, as every slice: open a real character with ~300 windows;
  confirm the list is navigable, the families fold, the filter narrows list and
  canvas together, the counter is honest, the reset restores, and that dragging
  a window while a filter is active still writes the right geometry.

## 7. Non-goals

- Renaming windows. EVE has no per-window user label; the friendly names are
  ours and are never written to the file.
- Hiding windows *from EVE*. `Open only` and the noise filter are editor-side
  view state, not edits — nothing toggles `openWindows` as a side effect.
- Persisting filter state across sessions (a restart). Within a session the
  filter DELIBERATELY persists across file and character switches — a slot,
  refreshToken or userOpen change does not clear it — so the same subset can
  stay applied while flipping between characters to compare them; this
  reverses the plan as originally written, per the project owner's ruling
  during the slice. The "showing N of M · reset" counter exists precisely so a
  carried-over filter is never silently misleading. It still resets when
  leaving and returning to the Layout view, since the view is `{#if}`-gated
  and its state is destroyed with it.
- Reordering or bulk-editing the list. Selection stays one window at a time.
