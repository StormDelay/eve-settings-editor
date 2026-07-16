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
- the resolved string entries (in stored order).

Writes are keyed by the widget path (see §5), so the projection needs no
`NodePath`s — just the widget string and its entries.

The projection resolves `Ref`/`Shared` in **keys and values** using the
shared-slot table already lifted into `treewalk` for the overview editor —
real files re-dedup repeated values across writes, the same gotcha that bit
the layout and overview projections. No labels here: the projection is purely
structural and returns raw widget paths.

If the file has no `editHistory` (fresh account), the projection returns an
empty vector.

## 5. Writes — dedicated inline-first edit functions (the `overview.rs` pattern)

The original plan here was to reuse the raw `apply_mutation`. That does **not**
work: `mutate.rs::apply` refuses to remove any subtree containing a `Shared`
store (`MutateError::SharedSubtree`), and real files dedup repeated/empty
editHistory entries into `Shared`s — so a remove/clear would fail exactly when a
list holds a deduped entry, breaking the cleanup use case. Instead the editor
uses dedicated `settings-model` functions, mirroring the overview editor:

- `set_list_entries(user, widget, entries)` — replaces one widget's list with
  the given strings (empty slice = clear). Covers **add / edit / remove /
  reorder / per-list clear**: the frontend holds the list and always sends the
  new full contents, so one primitive serves every per-list operation.
- `clear_all_history(user)` — empties every list in one pass (the privacy nuke),
  one atomic save with one backup rather than ~40 round-trips. Clearing keeps
  each widget key, mapping it to `[]`.

Both **inline all sharing first** (`treewalk::inline_all`, dropping every
`Shared`/`Ref`) so a wholesale list replacement can never dangle a `Ref`
(`RefBeforeStore` on encode). Same accepted trade-off as the overview editor:
the saved file is larger (dedup gone) but valid, and EVE re-dedups it on next
logout (the existing small-tasks re-share item).

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
