# Autofill editor — Design

**Date:** 2026-07-16
**Status:** Approved pending user review
**Milestone:** Autofill editor (the last of the three §6 editors; batch-apply/M4 follows).

## 1. Purpose

Edit the client's remembered text-input history — the strings the game
autocompletes in search boxes, filters, name fields, etc. — through a
purpose-built view instead of the raw tree. Two co-equal use cases:

- **Curation:** add a value you want suggested, remove a bad one, reorder so
  the useful one comes first, edit an entry in place.
- **Cleanup / privacy:** wipe remembered searches, other players' names, and
  stale filter text — per list, or all at once before sharing a file.

## 2. What is edited (M0 experiment 4)

All remembered text-input history is **one structure**, in
`core_user_<id>.dat` only:

- Path: user-file root → `b"ui"` → `b"editHistory"` → `(timestamp, dict)`.
- The inner dict is keyed by UI widget path (`Bytes`, e.g.
  `b"/addressbook/content/main/SearchPanel/Container/SingleLineEditText"`) →
  a list of remembered strings (`Str`; occasionally an empty `Bytes`). New
  entries append at the end.
- ~40 widget-path lists in one real file (People & Places searches, inventory
  quick-filters, structure browser search, skill catalogue search,
  fleet/fitting names, wallet transfer "reason", overview-export filename,
  chat channel names, bug-report title, …).

Out of scope (raw-tree-only per design §6): overview *filter presets*
(`overview → overviewProfilePresets`) — a different structure, not remembered
text.

## 3. Placement

A 4th button — **Autofill** — in the existing `Tree / Layout / Overview`
switcher in `+page.svelte`, gated on a user file being open (editHistory is
user-file-only). No new top-level view mode; mirrors how the Overview button
was added.

## 4. Rust — read-only projection

New `crates/settings-model/src/autofill.rs`, sibling to `windows.rs` /
`overview.rs`:

```
edit_history(doc) -> Vec<RememberedList>
```

Each `RememberedList` carries:

- the **raw** widget path (a `String` of the `Bytes` key),
- the resolved string entries (in stored order),
- the `NodePath` to the list and to each entry, so the frontend can edit
  through the existing mutation API.

The projection resolves `Ref`/`Shared` in **keys and values** using the
shared-slot table already lifted into `treewalk` for the overview editor —
real files re-dedup repeated values across writes, the same gotcha that bit
the layout and overview projections. No labels here: the projection is purely
structural and returns raw widget paths.

If the file has no `editHistory` (fresh account), the projection returns an
empty vector.

## 5. Writes — reuse `apply_mutation`, no new write path

Editing a list of strings is CRUD the raw-tree mutation API already supports
(M1b-2 made dict/list entries insert/remove/edit-able). The frontend issues
the same `Mutation` ops the tree editor uses, addressed by the `NodePath`s the
projection returned:

- **add** = append an entry to the list,
- **remove** = delete an entry,
- **edit** = edit a scalar entry,
- **reorder** = remove + insert at the target index.

This is the layout-canvas pattern: a typed projection for reading, the
existing write path for writing (no dedicated per-op backend like the overview
editor needed — this data is simpler, with no per-tab materialization logic).

**Clear** (one list) and **Clear all** (every list) get one dedicated
`Mutation` that empties the target list(s) in a single apply, so a bulk clear
is one atomic save with one backup rather than ~40 round-trips. "Empty" means
set the list to `[]` and keep its widget key — not remove the key; the client
re-populates as it always would, and this matches per-list-clear semantics.

Saves go through the **user slot's** existing backup → verify → atomic-write
chain; any edit marks the user slot dirty, and Save persists it. Same as the
overview editor's user-slot writes.

## 6. Frontend — `AutofillView.svelte`

A list of lists:

- **Per list:** header = friendly label + the raw widget path as a
  subtitle/tooltip + entry count + a **Clear** button. Body = each remembered
  string as an inline-editable row with a remove button and a drag handle for
  reorder, plus an **+ add** row.
- **Top of view:** a **Clear all remembered text** button, behind a confirm
  dialog (destructive; the backup is still taken as always).

**Labels.** A curated map (in the frontend — presentation only) gives friendly
names for the ~10-15 known widgets from the M0 notes (People & Places search,
inventory quick-filter, wallet transfer reason, overview export filename, …).
Anything unrecognized falls back to a **derived** label: strip boilerplate
segments (`/content/main/`, trailing `SingleLineEditText`, …), title-case the
meaningful segment. The raw path is always shown alongside so a wrong/missing
friendly name never hides which list you're editing.

New form controls get explicit dark background/color (native controls render
light in the WebView2 app — see the dark-native-controls guideline).

## 7. Edge cases

- **`(timestamp, dict)` wrapper:** preserve the timestamp verbatim (it is the
  client's own bookkeeping; leaving it produces a smaller, more faithful diff
  than bumping it).
- **Empty `Bytes` entries** (observed occasionally in a list): shown as an
  empty row, removable.
- **No `editHistory`** (fresh account): empty state ("No remembered text yet").
- **Ref/Shared** in keys/values: resolved on read; writes inherit whatever
  `apply_mutation` already does (the encoder-side re-share concern is the
  pre-existing small-tasks item, not specific to this editor).

## 8. Testing

- **Projection unit tests** for `edit_history`, **plus a realshape test
  against a real corpus user file** — the M3c lesson: the overview model was
  wrong three times because nothing tested a real dumped file. Enumerate the
  real lists, assert `Ref`/`Shared` resolves, assert entry ordering.
- **Round-trip:** apply add / remove / clear → encode → re-decode → assert the
  change stuck with no duplicate keys or dangling refs.
- **Frontend:** label-derivation unit test (`node --test`, zero-dep), covering
  a curated hit and a derived fallback.
- **Manual live smoke (§8 exit gate):** add and clear an entry in-app → save →
  launch EVE → confirm the remembered suggestion changed / the cleared one is
  gone, and the client otherwise behaves normally.

## 9. Out of scope / deferred

- Overview filter-preset editing (raw-tree-only, design §6/§10).
- Adding a brand-new widget-path list that the client never created (YAGNI —
  the raw tree covers it; you only edit lists the client made).
- Cross-file batch apply of suggestion lists is M4 (batch apply); the
  `SuggestionLists` category model this projection exposes is the unit that M4
  will apply.
