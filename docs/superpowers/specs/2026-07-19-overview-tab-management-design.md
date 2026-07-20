# Overview tab management (design)

Date: 2026-07-19 (Phase B recipe resolved 2026-07-20)
Status: Phase A shipped (PRs #11, #12). Phase B recipe resolved from existing
corpus data (§6) — no in-game capture needed; ready for writing-plans.
Builds on: the overview-columns editor (`overview.rs` projection + edits,
`OverviewView.svelte`), the window-stacks structural-edit idiom (`stacks.rs`,
inline-first → edit → `reshare`), and the M2 layout canvas (`windows.rs`) that
will position any overview window this milestone spawns.

## 1. Goal

The overview editor today edits **columns** within existing tabs (visibility,
order, width). This milestone adds structural editing of the **tab set and the
overview windows that hold it**: create / rename / delete / reorder tabs, move a
tab between overview windows, and add / remove overview windows themselves.

A guiding principle for window creation: a newly-added overview window must be a
**real, fully-formed window instance** the moment it is written — so the user can
position and size it in the Layout editor **without launching the EVE client
first**. We do not rely on EVE lazily materializing the window on next login.

This is **slice 1 of 4** in the "overview depth" roadmap (tab management →
filter presets + tab→preset mapping → standing/state colors + tags →
import/export overview packs). Each later slice is its own spec; this one is
self-contained.

## 2. The tab / window model (from the corpus + one discovery gap)

Overview structure lives in the **`core_user` (account) file**, under the
`overview` container. Confirmed against real files and the existing
`overview.rs` / `overview_realshape.rs` reading code:

- **`tabsettings_new`** (modern) / **`tabsettings`** (legacy) — a dict keyed by
  integer tab index, wrapped `(timestamp, dict)`. Each tab value is a dict:
  - `name` — the tab label, stored as a string-table key (`StrTable(52)`) or a
    plain `Str("name")` key; the value may be `Str` / `StrUcs2` / `Bytes`.
  - `overview` — the **filter-preset name** the tab uses (e.g. `Bytes("P")`).
    This is the tab→preset reference; preset *editing* is a later slice, but tab
    creation must set a valid preset name here.
  - optional `tabColumnOrder` / `tabColumns` — own column lists; absent = inherit
    the account defaults (existing column-editor behavior, unchanged).
- **`tabsByWindowInstanceID`** — a list of lists: one inner list per physical
  overview window, holding that window's tab indices in tab-strip order. This is
  the tab→window membership and the strip ordering, in one structure.
- **`overviewColumnOrder` / `overviewColumns`** — account-default columns
  (unchanged; read-only context here).

Window **geometry** is separate and lives in the **`core_char` file**, in the
`windows` dict — the same dict the Layout canvas edits. The primary overview
window is the entry keyed `overview` (confirmed in `windows.rs` tests). So the
two axes of an overview window are split by file:

- **membership / tab structure** → account-global, `core_user` overview container
- **geometry (position/size/open)** → per-character, `core_char` `windows` dict

The column editor already spans both files this way (visibility in user, width in
char), so the two-slot pattern is established.

**Discovery gap (Phase B only):** how a *second* overview window is keyed in the
`windows` dict and how that key links to a `tabsByWindowInstanceID` position is
not determinable from the static corpus (fresh files have a single overview
window). §6 resolves it with one before/after in-game capture, the same method
as the window-stacks §7 experiment. The design commits to spawning a real window
instance regardless; the capture only pins the exact representation.

No character/account ids appear in this document, per the repo data rule; token
examples (`"P"`, `StrTable(52)`) are the client's own idioms or synthetic
stand-ins.

## 3. Backend — read projection

Extend the overview projection so the UI can render structure, not just columns.
Today `OverviewColumns { windows: [OverviewWindow{index, tab_indices}], tabs:
[OverviewTab{index, name, inherits, columns}] }` already carries the window
grouping and per-tab data — most of what the UI needs is present. Add:

- `OverviewTab.preset: String` — the tab's `overview` preset-name, so a created
  tab can copy a sibling's value and so the UI can show which preset a tab uses.
- Nothing else structural is required: `windows[].tab_indices` already gives
  tab→window membership and strip order.

Reads stay format-blind and go through the existing `treewalk` / `SharedTable`
helpers.

## 4. Backend — authoring (`overview_tabs.rs`)

A new module sibling to the delicate `overview.rs`, exactly as `stacks.rs` split
out from `windows.rs`. Every entry point uses the proven idiom: inline the whole
tree (drop sharing), edit plain values, and let the app layer `reshare` before
save. All edits operate on the `overview` container's `tabsettings_new` (creating
it from a legacy `tabsettings` via the existing modern-migration on first edit)
and `tabsByWindowInstanceID`.

Tab-only edits (Phase A — no client capture needed):

- `create_tab(user, window_idx, from_tab) -> new_tab_index` — new index =
  `max(existing indices) + 1` (indices are dict keys referenced by value in the
  window lists; contiguity is not required). **CLONE a sibling tab** (the
  caller-selected `from_tab`, else the first tab) and override its name. Cloning
  — not building a minimal `{name, overview}` dict — is REQUIRED: every real EVE
  tab carries `bracket` and `color` keys, and EVE's "reset all overview settings"
  iterates tabs reading them, so a tab missing them makes the reset throw
  (found in the live smoke, 2026-07-19). The clone inherits the sibling's
  preset (`overview`) and its `t52:"name"` key encoding; its `tabColumns` /
  `tabColumnOrder` lists are dropped so the new tab inherits columns. Append the
  new index to `tabsByWindowInstanceID[window_idx]`.
- `rename_tab(user, tab_idx, name)` — set the tab's name field however it is
  stored (string-table or plain key), matching `str_field_r`'s key detection; if
  absent, insert a plain `name` key.
- `delete_tab(user, tab_idx)` — remove the entry from `tabsettings_new` **and**
  the index from **every** `tabsByWindowInstanceID` inner list (no dangling
  references). Guard: refuse deleting the last tab of the last window (an
  overview with zero tabs is invalid).
- `reorder_tabs_in_window(user, window_idx, indices_in_order)` — replace that
  window's inner list with the given permutation of its current members.
- `move_tab_between_windows(user, tab_idx, from_idx, to_idx, pos)` — remove from
  the source inner list, insert into the target at `pos`.

Overview-window edits (Phase B — recipe resolved in §6):

- `add_overview_window(user, name, from_tab)` — model half edits the user file
  only: append a new inner list to `tabsByWindowInstanceID` (new index = old
  length) and seed it by cloning `from_tab` via `create_tab` (a window must have
  ≥1 tab). The paired `core_char` geometry write — clone the primary `overview`
  window's value in every `windows` subdict into `overview_{new_idx}`, position
  cascade-offset from the primary — is orchestrated by `ops.rs` (§5), which holds
  both slots. Only the open character gets geometry; siblings on the account
  inherit the grouping and self-heal their own copy on next login (EVE recreates a
  missing overview window at default geometry — `format-notes.md`).
- `remove_overview_window(user, window_idx)` — model half edits the user file:
  reassign the window's tabs onto window 0's strip (no tab loss), then remove the
  inner list. `ops.rs` deletes the paired `overview_{window_idx}` key from every
  char `windows` subdict. **Last-window-only** this slice (the positional link
  makes middle removal a re-key cascade — deferred, see §6). Guard: refuse
  removing the last overview window.

Error type `OverviewTabError` mirrors `StackError`: variants for no overview
container, unknown tab index, unknown window index, and the last-tab /
last-window guards; `Display` for the message and `#[serde(tag = "code")]` for
the frontend error code, matching the `edit_char_stacks` mapping already in
`ops.rs`.

## 5. Backend — commands (`ops.rs`)

Thin wrappers mirroring the `stack_*` commands, each editing the relevant
slot(s), running `reshare`, and re-projecting the overview so the UI updates:

- `tab_create`, `tab_rename`, `tab_delete`, `tab_reorder`, `tab_move`
  — edit the user slot, return the refreshed `OverviewColumns`.
- `overview_window_add`, `overview_window_remove` — edit user **and** char slots
  (grouping + geometry), return the refreshed overview projection.

The user/char slot pairing and read-only guards already exist (the column editor
uses them); these reuse `edit_user_*` / char-slot helpers.

## 6. Phase B — overview-window representation (RESOLVED from existing data)

Goal: learn the exact representation of a second overview window so
`add_overview_window` produces a byte-plausible, EVE-accepted instance.

**Resolved 2026-07-20 without a fresh in-game capture.** The gap this section
feared (fresh files have a single overview window) does not apply: the local
corpus already contains multi-overview-window accounts (2 and 3 windows). The
`overviewdiff` scratch bin over the exp5 corpus pair answers all three questions
directly:

1. **Second-window key = `overview_1`, `overview_2`, …** — plain byte-string
   window ids in the char file's `windows` dict, the primary being `overview`
   (already recorded in `format-notes.md` "Window ids"). Confirmed by a
   3-overview-window char whose `windowSizesAndPositions_1` holds `overview`,
   `overview_1`, `overview_2`.
2. **The link is POSITIONAL.** `tabsByWindowInstanceID` carries no window id — it
   is a list of tab-index lists, and position `i` maps to window key `overview`
   (i=0) / `overview_{i}` (i≥1). Confirmed: the same account's
   `tabsByWindowInstanceID` has exactly 3 inner lists ↔ its 3 `overview*`
   geometry keys. (This is already how `project_overview` reads windows — by list
   position.)
3. **New-tab preset is moot.** `create_tab` clones a sibling tab (Phase A), so the
   new tab inherits a valid `overview` preset name; no default-fallback needed.

A real second overview window carries entries in the same `windows` subdicts as
the primary — `windowSizesAndPositions_1`, `openWindows`, `minimizedWindows`,
`lockedWindows`, `compactWindows`, `isLightBackgroundWindows` (and the other
boolean state dicts). So the **mint recipe is: clone the primary `overview`
window's value in every `windows` subdict into key `overview_{N}` and override its
geometry** — the same "clone a sibling, don't fabricate" idiom Phase A uses for
tabs, which makes the required-flag set correct by construction rather than
guessed. (`overviewdiff` is untracked scratch tooling — built, used, not shipped.)

**Design decisions (approved 2026-07-20):**

- **`add_overview_window(user, name, from_tab) -> new_idx`** (user file): inline →
  push an empty inner list to `tabsByWindowInstanceID` (new index = old length) →
  reuse `create_tab` to clone `from_tab` into it (a new window must have ≥1 tab;
  real second windows carry exactly one) → return the index. The paired char-file
  geometry (clone-primary → `overview_{new_idx}`, cascade-offset position) is
  written by `ops.rs`, which already pairs the user+char slots.
- **`remove_overview_window(user, window_idx)`** (user file): guard `LastWindow`
  (≤1 window) → append the window's tab indices onto window 0's strip (reassign,
  no tab loss) → remove the inner list. `ops.rs` deletes the paired
  `overview_{window_idx}` key from every char `windows` subdict.
- **Remove is last-window-only for this slice.** Because the link is positional,
  removing a middle window shifts later windows out from under their `overview_N`
  keys, forcing a re-key cascade across the char subdicts (plus a promote-primary
  edge case). Lazy-correct first cut: the UI offers Remove only on the last
  overview window and the model guards it. Middle-window removal (re-key cascade,
  or window-reorder-then-remove) is a documented deferral (small-tasks ledger).
- **Windowless account** (no char open): the user-file grouping edit still
  applies; the geometry write is skipped, and EVE self-heals the window at default
  geometry on that character's next login — matching Phase A's windowless handling.
- `reshare` both edited slots on save; the encoder's Tuple-non-sharing fix already
  keeps cloned geometry tuples from aliasing.

**Final validation** is a live smoke (add a window in-app → save → confirm EVE
renders a real, positionable second overview window), the same gate every prior
milestone used — not a blocker on building against the confirmed shapes.

## 7. Frontend — Overview view (`OverviewView.svelte`)

All controls live in the existing Overview screen, around the current tab
`<select>`:

- Per-tab: a rename affordance (inline edit), a delete button (guarded), and
  drag-reorder of the tab list within its window (reuse the column drag-reorder
  pattern already in this file).
- "New tab" — prompts for a name, targets the currently-shown window.
- Move-between-windows — only shown when more than one overview window exists.
- "Add overview window" / "Remove overview window" — window-level controls;
  after adding, a hint that the window is now positionable in the Layout editor.

Native `<select>` / `<input>` controls get explicit dark styling (the known
WebView2 light-control gotcha). Failed edits surface via the existing
`message()` error dialog; the `<select>` resets to its placeholder after a failed
op, matching the stacks panel.

## 8. Testing

- `overview_tabs` unit tests per edit on synthetic trees: create allocates a free
  index and appends to the right window; rename handles both name-key encodings;
  delete purges the tab from `tabsettings_new` and all window lists and honors the
  last-tab guard; reorder/move permute the correct inner lists; the window
  add/remove guards fire.
- A corpus realshape test exercising real idioms: `(timestamp, dict)` wrapper,
  `StrTable(52)` name keys, Ref/Shared tab tokens, legacy→modern migration on
  first structural edit.
- Round-trip guard: every edit path `reshare`s and the result re-decodes equal
  (the standard reshare regression check).
- Phase B: `add`/`remove` unit tests (new inner list + cloned tab lands in the new
  window; remove reassigns tabs to window 0 and pops the list; last-window guard),
  plus a realshape test that mints `overview_{N}` geometry by cloning the primary
  and round-trips through `reshare`. Final EVE acceptance is a manual live smoke.

## 9. Dependencies, scope, non-goals

- **Depends on:** nothing new for Phase A. Phase B's window representation is
  resolved from the corpus (§6); it reuses the M2 `windows.rs` geometry write path
  and the Phase A `create_tab` clone path.
- **Cross-file:** window add/remove writes both the user (grouping) and char
  (geometry) slots; the read-only / pairing guards already used by the column
  editor apply.
- **Non-goals:** window geometry editing (the Layout milestone; this only spawns
  a positionable instance), filter-preset *editing* (slice 2 — tab creation only
  copies an existing preset name), standing/state colors (slice 3), and
  import/export packs (slice 4).
- **Deferred / open:** middle-overview-window removal (remove is last-window-only
  this slice — the positional link makes it a re-key cascade; see §6 and the
  small-tasks ledger). Phase A has already shipped independently (PRs #11, #12).
