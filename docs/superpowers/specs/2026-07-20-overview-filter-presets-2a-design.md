# Overview filter presets — slice 2a: preset management + tab→preset mapping (design)

Date: 2026-07-20
Status: designed, ready for writing-plans.
Roadmap: **slice 2 of the "overview depth" milestone** (tab management [slice 1,
shipped] → **filter presets + tab→preset mapping [slice 2]** → standing/state
colors + tags → import/export packs). Slice 2 is split into **2a (this spec)** and
**2b** (the catalog-backed content editor); see §9.
Builds on: the overview-tabs structural-edit idiom (`overview_tabs.rs`,
inline-first → edit → `reshare`; `ops.rs` `edit_user_tabs`), the overview
projection (`overview.rs` `project_overview`, which already exposes each tab's
`preset` name), and the `OverviewView.svelte` inline name-entry pattern.

## 1. Goal

The overview editor edits **columns** (slice 1 added tab/window structure). This
slice adds **filter presets** — the named filter definitions a tab points at via
its `overview` field. Slice 2a delivers the parts that need **no external data**:

- **Assign** which existing preset each tab uses.
- **Manage** presets as named entities: **duplicate**, **rename**, **delete**.

Editing a preset's *contents* (which entity groups and states it shows) is slice
2b, which needs an EVE group-name catalog and is specced separately.

Everything here is **user-file only** (account-scoped): both the preset
definitions and the tab→preset link live in the `core_user` `overview`
container. No `core_char` writes — simpler than the slice-1 window add/remove.

## 2. The preset / tab-link model (confirmed from the corpus)

In the `core_user` file, under the `overview` container:

- **`overviewProfilePresets`** — a `(timestamp, dict)` keyed by **preset name**
  (a `Str`/`Bytes` key). Each value is a dict of exactly three integer lists:
  - `groups` — EVE inventory **group IDs** to show (e.g. `25`=Frigate,
    `26`=Cruiser, `27`=Battleship). The bulk of a preset.
  - `filteredStates` — a small list of state flag ints (standing/status filters).
  - `alwaysShownStates` — same domain, usually empty.
  On real files these three keys are `Shared`/`Ref` (interned across presets), so
  edits go through the standard inline-first idiom.
- Session-state siblings — `overviewProfilePresets_notSaved`, `presetHistoryKeys`,
  `restoreData` — hold EVE's unsaved-edit buffer, MRU history, and restore data.
  **2a does not touch them** (see §7).
- A **tab** references its preset by name in its `overview` field, inside
  `tabsettings_new` (modern) / `tabsettings` (legacy) — already projected as
  `OverviewTab.preset`.

**Every tab always references exactly one real preset** (verified across both the
modern and legacy shapes, and across sparse tabs that omit most other keys). A
tab's entire "what to display" lives in that named preset; there is **no inline,
on-the-tab filter and no preset-less tab**. The per-tab `showAll` / `showNone` /
`showSpecials` / `color` fields are display *modifiers*, not the filter. So a
projected `preset == ""` (field absent / non-string) is a defensive case that
does not occur on real files.

The preset name is **interned as a shared string** used by BOTH the preset's dict
key and the tab's `overview` value. The inline-first edit pass drops that sharing
into independent copies, which is exactly why `rename_preset` / `delete_preset`
must update the tab fields explicitly (§4), not rely on the sharing.

Slice 2a treats a preset's three lists as an **opaque blob** it copies wholesale
(duplicate) but never inspects — no group/state knowledge is needed until 2b.
Bracket presets (a separate per-tab `bracket` reference into their own store) are
out of scope. No character/account ids or real preset names appear in this
document, per the repo data rule.

## 3. Backend — read projection (`overview.rs`)

Add one field to `OverviewColumns`:

- `presets: Vec<String>` — the `overviewProfilePresets` key names, **sorted
  (case-insensitive)**. Empty when the container or key is absent.

This is all 2a needs: the per-tab picker offers these names, and the management
UI lists them. Each tab's current preset is already in `OverviewTab.preset` and,
per §2, is always one of `presets` on a real file. Defensively the picker's
option set is `presets ∪ {tab.preset}`, so even a hypothetical stale/empty
reference still shows and isn't silently lost — but no special "(default)" UI is
needed. Reads stay format-blind via the existing `treewalk` helpers; the
`(timestamp, dict)` wrapper is unwrapped as elsewhere.

## 4. Backend — authoring

**`set_tab_preset(v, tab_idx, name)`** — lives in `overview_tabs.rs` next to
`rename_tab` (it edits a tab's field). Sets the tab's `overview` value to `name`
(inserting the key if absent), matching how `rename_tab` writes the `name` field.
`UnknownTab` if the index is absent.

**New module `overview_presets.rs`** (sibling to `overview_tabs.rs`, reusing its
`pub(crate)` helpers — `is_b`, `as_int`, the `overview_mut`/inline pattern, and
`tabs_mut` for the tab retargets):

- `create_preset(v, from, new_name) -> ()` — **clone** the `from` preset's value
  (the whole `{groups, filteredStates, alwaysShownStates}` blob) under key
  `new_name`. Cloning — not fabricating an empty dict — keeps the required shape
  correct by construction, the same idiom slice 1 uses for tab creation.
  `UnknownPreset` if `from` is absent; `PresetExists` if `new_name` already
  exists.
- `rename_preset(v, old, new) -> ()` — rename the `overviewProfilePresets` key
  `old`→`new`, **and retarget every tab whose `overview == old` to `new`** so no
  tab is left dangling. `UnknownPreset` if `old` absent; `PresetExists` if `new`
  already exists; no-op-safe if `old == new`.
- `delete_preset(v, name) -> ()` — remove the key. **Refuse when it is the last
  preset** (`LastPreset`). Otherwise **reassign every tab using it to the
  immediately-preceding preset** in sorted order (the successor when deleting the
  first), then remove the key — deterministic, no target prompt. `UnknownPreset`
  if absent.

Extend `OverviewTabError` with `UnknownPreset { name }`, `PresetExists { name }`,
`LastPreset` (one enum, one `tab_err` mapping in `ops.rs` — less wiring than a
second error type; the enum already covers all overview structural edits).
`Display` messages mirror the existing style; `#[serde(tag = "code")]` gives the
frontend an error code.

## 5. Backend — commands (`ops.rs`)

Thin wrappers through the existing `edit_user_tabs` (inline → edit → `reshare` →
re-project → return the refreshed `OverviewColumns`, which now carries `presets`).
All user-file only:

- `tab_set_preset(state, tab_idx, name)`
- `preset_create(state, from, new_name)`
- `preset_rename(state, old, new)`
- `preset_delete(state, name)`

## 6. Frontend — Overview view (`OverviewView.svelte`)

Additions around the existing controls:

- **Per-tab preset picker** — a `<select>` bound to the selected tab's preset,
  options = the sorted `presets` (defensively including the current value if it is
  somehow not among them, per §3). Changing it calls `tabSetPreset(tabIndex,
  name)` and `onUserDirty()`.
- **Preset management** — a small control cluster: **Duplicate** (clones the
  selected tab's current preset — always a valid source, per §2 — prompting for a
  new name via the existing inline `pending` name-entry flow, extended with
  `duplicatePreset` / `renamePreset` kinds), **Rename**, and **Delete** (a
  `confirm` dialog that names the preceding preset the in-use tabs will move to,
  e.g. *"Delete 'PvP'? 3 tabs will move to 'Mining'."*; the UI computes that
  neighbour from the same sorted `presets` list the backend uses).

Native `<select>`/`<input>` get explicit dark styling (the WebView2 gotcha, per
the existing block). Failed edits surface via the existing `message()` error
dialog.

## 7. Deliberate simplifications (`ponytail:` ceilings)

- **Create = Duplicate only.** A blank preset shows nothing and can't be filled
  until 2b, so 2a offers no "New blank" — you duplicate an existing preset. 2b
  adds blank-create once contents are editable.
- **Session-state siblings untouched.** `overviewProfilePresets_notSaved` /
  `presetHistoryKeys` / `restoreData` may also name a preset. 2a leaves them to
  EVE's own reconciliation. **Live-smoke check:** after a rename/delete, confirm
  the client shows no phantom preset in its dropdown/history; if it does, add
  targeted sibling sync then — not before.

## 8. Testing

- `overview_presets` unit tests on synthetic trees: duplicate clones the source
  blob under the new key (and errors on unknown source / existing name); rename
  renames the key AND retargets referencing tabs (and errors on existing target);
  delete reassigns in-use tabs to the preceding preset and removes the key; delete
  refuses the last preset (`LastPreset`); `set_tab_preset` sets the field and
  errors on an unknown tab.
- Projection test: `presets` is populated and sorted; a realshape case with the
  `(timestamp, dict)` wrapper and `Shared`/`Ref` preset keys.
- Round-trip guard: every edit path `reshare`s and re-decodes equal (standard
  reshare regression check).
- Final acceptance: a live smoke (assign/duplicate/rename/delete in-app → save →
  confirm EVE reflects it and the §7 sibling check).

## 9. Dependencies, scope, non-goals

- **Depends on:** nothing new — reuses the slice-1 idiom and the existing
  projection/commands.
- **Non-goals (2a):** editing preset *contents* (groups/states) — that is **2b**,
  which adds the ESI-backed group-name catalog (verified present:
  `/universe/categories/{id}` + `/universe/groups/{id}`, cached on disk keyed by
  the `/status` `server_version` for invalidation) and a hardcoded state-flag
  table, then a type-tree + states editing UI. Also out of scope: standing/state
  colors + tags (slice 3), import/export packs (slice 4).
- **Deferred / open:** sibling session-state sync (§7), pending the live smoke.
