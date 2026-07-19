# Overview tab management (design)

Date: 2026-07-19
Status: approved design. Ready for user review, then writing-plans.
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

- `create_tab(user, window_idx, name) -> new_tab_index` — new index =
  `max(existing indices) + 1` (indices are dict keys referenced by value in the
  window lists; contiguity is not required). Set `name`; set `overview` to the
  preset-name of the caller-selected sibling tab (fallback: the first existing
  tab's preset, else the account default preset name — a minor discovery item,
  §6). Omit column lists so the tab inherits. Append the new index to
  `tabsByWindowInstanceID[window_idx]`.
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

Overview-window edits (Phase B — depends on §6 capture):

- `add_overview_window(user, char)` — cross-file: append a new inner list to
  `tabsByWindowInstanceID` (account-global, user file) **and** write a
  full default window instance into the `core_char` `windows` dict (per-character
  geometry) so the Layout canvas shows it immediately. The window key/name and
  the list↔instance linkage come from §6. Default geometry: a sensible on-screen
  rect at the stored reference resolution (e.g. offset from the primary overview
  window). Only the open character gets geometry; siblings on the account inherit
  the grouping and position their own copy later — matching EVE's per-character
  window behavior.
- `remove_overview_window(user, char, window_idx)` — remove the inner list and
  its window instance; its tabs are either reassigned to another window
  (preferred: append to window 0) or deleted with the window — decided in §6
  once the linkage is known. Guard: refuse removing the last overview window.

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

## 6. Phase B — overview-window capture experiment

Goal: learn the exact representation of a second overview window so
`add_overview_window` produces a byte-plausible, EVE-accepted instance.

Method (mirrors window-stacks §7): a throwaway `--bin` capture/diff tool
(inlined, sorted `windows` + overview dicts) run over two snapshots — before and
after **tabbing out / creating a second overview window in-game** — plus a
before/after around **creating the first extra tab** to confirm the tab-creation
shape. Answer:

1. The `windows`-dict key/name of a second overview window (e.g. `overview2`, an
   instance-suffixed name, or a numeric instance id) and its minimal required
   fields for EVE to accept it.
2. How a `tabsByWindowInstanceID` position maps to that window key (positional,
   or an explicit instance id stored elsewhere).
3. The default preset-name a brand-new tab / overview receives, to seed
   `create_tab`'s fallback.

The capture tool is scratch tooling (built, used, deleted — it is not shipped),
and its findings are recorded back into this section before Phase B is
implemented. Phase A does not depend on it and can ship first.

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
- Phase B: the in-game capture is manual; its result is asserted by a realshape
  fixture built from the captured shape.

## 9. Dependencies, scope, non-goals

- **Depends on:** nothing new for Phase A. Phase B depends on the §6 capture and
  reuses the M2 `windows.rs` geometry write path.
- **Cross-file:** window add/remove writes both the user (grouping) and char
  (geometry) slots; the read-only / pairing guards already used by the column
  editor apply.
- **Non-goals:** window geometry editing (the Layout milestone; this only spawns
  a positionable instance), filter-preset *editing* (slice 2 — tab creation only
  copies an existing preset name), standing/state colors (slice 3), and
  import/export packs (slice 4).
- **Deferred / open:** the new-tab default-preset fallback and the
  window↔list linkage are pinned by §6 before Phase B lands; Phase A can ship
  independently if we choose to stage the release.
