# M3c — Overview columns editor (design, v2 — real-file model)

Date: 2026-07-16 (v2 after live-smoke findings)
Status: approved, pre-plan
Builds on: M1 (codec, raw tree, save chain), M2 (layout canvas — the shared-slot
`Ref`/`Shared` resolution this reuses), M3a (ESI names), M3b (char↔user pairing /
account roster). Design spec §6 "Overview editor".

> **v2 note.** The v1 model in this file was built on hand-made `Value` fixtures
> and a "does-not-panic" corpus check; **no test ever read a real file's
> structure.** Live smoke showed the model was wrong. This revision re-grounds
> the format model on the user's actual on-disk files (dumped and verified),
> and supports both the **modern** (`tabsettings_new`) and **legacy / imported
> preset-pack** (`tabsettings`) formats. See §10 for the lessons.
>
> **v3 correction (2026-07-16, after full live guided-diff validation).** v2's
> "inherit from the tab's `overviewProfilePresets` preset" fallback (§2 Presets,
> §3) is **wrong** — `overviewProfilePresets` are **FILTER** presets (which ships/
> states to show, integer id lists) and carry **no columns**. The real model,
> pinned by in-game-change→dump diffs:
> - **Visibility** = per-tab `tabColumns` (a set, stored alphabetically),
>   materialized on a visibility change; else account-global `overviewColumns`.
> - **Order** = per-tab `tabColumnOrder`, materialized only on a reorder; else
>   account-global `overviewColumnOrder`. **Ceiling:** an inheriting tab's *exact*
>   on-screen order is EVE runtime state, not saved anywhere — the account-global
>   order is only an approximation until the tab is reordered (which pins it).
> - `tabColumns` and `tabColumnOrder` inherit **independently**.
> - **Encode safety:** a column edit rebuilds/shrinks a token list, which can
>   destroy a `Shared` token definition the file still `Ref`s → `RefBeforeStore`
>   on save. The write path inlines all `Shared`/`Ref` first (see the built code:
>   `treewalk::inline_shares` + `overview::inline_user`).
>
> Read the bullets below through this correction; §2 Presets and §3 are superseded.

## 1. Goal

A per-overview-tab columns editor — **show/hide**, **drag-to-reorder**, and
**per-column width** — instead of hand-editing the raw tree. It is the first
**two-file** category (visibility/order in `core_user`, widths in `core_char`),
so it also introduces the **two-slot** app state.

## 2. Format model (evidence-based)

Everything below was read from real files, not assumed. `Ref`/`Shared`
indirection is pervasive; the read path resolves it via a shared-slot table
(the `windows.rs` technique: `collect_shared` + `effective`).

**In the `core_user` (account) file**, under the overview container
(`root → b"overview"` on modern files; the legacy container is pinned
programmatically at implementation, §9):

- **Tabs** — `b"tabsettings_new"` (modern) *or* `b"tabsettings"` (legacy) →
  `(FILETIME, dict)` keyed by a **global tab index** (`Int`, sequentially
  allocated — **not** derivable from the window). Each tab dict holds:
  - `"name"` — the tab label. **The key is a string-table ref (`t52`)**, not a
    plain string; the value is `Str`/`StrUcs2`/`Bytes`.
  - `b"overview"` — the **FILTER preset name** the tab references (which ships/
    states to show). **Unrelated to columns** (see v3 correction).
  - `b"bracket"`, `b"color"` — left untouched.
  - Optionally `b"tabColumnOrder"` (full ordered column list) and/or `b"tabColumns"`
    (visible subset, stored alphabetically) — the tab's **own column override**,
    each present only after the tab has been reordered / had its visibility
    changed respectively (they appear **independently**). Bare lists; items are
    column tokens (`Bytes`), frequently `Ref`/`Shared`.
- **Account-default columns** (what an inheriting tab uses) — in the overview
  container: `b"overviewColumns"` (the visible set) + `b"overviewColumnOrder"`
  (the master ordered list of all columns). Items are column tokens (`Bytes`),
  frequently `Ref`/`Shared`. *(`b"overviewProfilePresets"` is the FILTER-preset
  system and carries no columns — not a column source.)*
- **Window→tab mapping** — `b"tabsByWindowInstanceID"` = a **list of lists**;
  outer index = overview window instance, inner list = that window's tab indices
  in display order. Observed: `[[0,1,…,9,12,13],[10,11,14]]` — window 0 owns the
  first list, window 1 the second.

**In the `core_char` file:**

- **Widths** — `root → b"ui" → b"SortHeadersSizes" → (FILETIME, dict)` keyed by
  the tuple `(b"overviewScroll2", tabIndex)` → dict of column token → width px.
  **Per tab** (by the same global tab index). `Ref`/`Shared` keys and tokens.
  (The v1 width mapping was already correct.)

**Legacy vs modern** differ only in the tab-container key (`tabsettings` vs
`tabsettings_new`) and container nesting; presets, master list, window map, and
widths are structurally identical.

## 3. A tab's effective columns (core semantics)

For a tab, resolved **independently per axis** (visibility vs order):
1. **Visible set** = its own `tabColumns` if present, else the account-default
   `overviewColumns`.
2. **Order** = its own `tabColumnOrder` if present, else the account-default
   `overviewColumnOrder` (an *approximation* for inheriting tabs — the exact
   per-tab inherited order is EVE runtime state, not in the file; see v3 ceiling).

A tab may own one axis and inherit the other. **Editing MATERIALIZES only the
affected axis's own list** (`tabColumns` on a visibility change, `tabColumnOrder`
on a reorder), seeded from the account default, then applies the edit — what the
EVE client does. **Column edits are strictly per-tab; other tabs and the account
defaults are untouched.** The write path inlines all `Shared`/`Ref` first so a
rebuilt list can't orphan a `Ref` the file still points at (`RefBeforeStore`).

## 4. Two-slot app state + loading (already built, validated, kept)

Unchanged from v1 and confirmed correct in smoke:

- `AppState` has typed `char` + `user` document slots (+ capture); every
  doc-scoped command takes a `slot`; each slot saves through its own M1 chain.
- **Loading reconciles the other slot** so the two are always a matching
  char/user pair or one is empty: opening a character loads its paired account
  file (M3b roster) or clears the user slot; opening an account file clears the
  char slot unless it holds one of that account's characters. The character
  selector loads a specific character for widths. The cross-slot unsaved-changes
  guard covers every load path.

## 5. The editor (tab-anchored)

- **Tab selector** — the account's overview tabs, **grouped by window** via
  `tabsByWindowInstanceID`, shown by name.
- **Character selector** — whose per-tab widths to edit (char file), from the
  M3b roster. (This is the char selector's real purpose.)
- **Column rows** for the selected tab, in the tab's effective order:
  - **checkbox** = visible; toggling **materializes** the tab's own `tabColumns`
    (seeded from the account-default visible set) then edits — per-tab, defaults
    untouched.
  - **drag** = reorder the tab's `tabColumnOrder` (materialize from the account
    default if needed).
  - **width** = the selected character's width for this tab
    (`(overviewScroll2, tabIndex)`, char file).
- A small note while the tab inherits any axis: it uses the account-default
  columns, and (for order) an inheriting tab's exact order isn't saved, so the
  shown order is the account default — editing gives the tab its own copy.
- **Save** writes each dirty slot (user = column overrides, char = widths)
  through its own chain.

## 6. Backend — `settings-model::overview` (rebuild)

Rebuild the visibility/order half around §2/§3; keep the width half.

- **Ref/Shared resolution** via a shared-slot table (reuse/extend the
  `windows.rs` approach; likely lift it into `treewalk` or a shared helper).
- **Read**: tabs from `tabsettings_new`|`tabsettings`; per tab, own columns else
  preset fallback; window grouping from `tabsByWindowInstanceID`; widths from
  char `SortHeadersSizes`. Robust to string-table `name` keys, `(ts, list)`
  master list, and `Shared`-wrapped tokens (fixes already in the working tree).
- **Edit**: materialize-from-preset, then toggle/reorder the tab's own lists;
  set width. Column tokens written as `Bytes`.
- All EVE format knowledge stays in this module.

## 7. Scope

**In:** tab-anchored per-tab column editing (own-or-preset-fallback + materialize),
window grouping, per-tab widths, both formats, `Ref`/`Shared` resolution, the
two-slot state + loading + save (kept).

**Deferred:** a standalone **preset editor** (edit `overviewProfilePresets`
directly — the shared base); editing the account master column list; multi-window
width edge cases (the rare non-tuple `overviewScroll2` key).

## 8. Testing

**Unit tests over `Value` trees that match REAL idioms** (not v1's clean
synthetic shapes): string-table `name` keys; `Ref`/`Shared` column tokens;
`(ts, list)` master list; **preset fallback**; **materialize-from-preset**;
`tabsByWindowInstanceID` grouping; both tab-keys; per-tab widths.
**Plus a real-file check** — a committed non-personal fixture whose structure
mirrors a real file, and the live-smoke gate (§10) — because synthetic tests are
exactly what missed this the first time.

## 9. Risks / implementation notes

- **Locate the overview container for both formats.** Modern nests the pieces
  under `root → b"overview"`; the legacy/pack file's container was not a
  depth-1 `b"overview"` in the dump — verify programmatically at implementation
  (decode a real legacy file, inspect the root keys) rather than trusting dumps.
- **Global tab indices are sequential, not window-derived** — never infer the
  window from the index; use `tabsByWindowInstanceID`.
- **Write-side Ref handling.** Materializing a tab's own lists writes column
  tokens as `Bytes`; the re-encode will differ from the original bytes (inline
  vs `Ref`). That is expected — fidelity's "editable" gate is about round-trip of
  the *unedited* file; edits legitimately change bytes. The client reads
  materialized bare-`Bytes` lists (it writes them itself); **confirm in live
  smoke.**
- **Re-validate against real files.** The whole feature must pass live smoke on
  a real char/user pair (both formats) — the gate that caught the v1 miss.

## 10. Lessons (why v1 was wrong)

v1 was built and tested entirely on hand-made `Value` trees plus a corpus test
that only asserted "does not panic"; empty-tabs-on-a-real-file passed it. Every
real idiom — preset indirection, string-table keys, `(ts, list)` wrappers,
`Ref`/`Shared`, sequential global tab indices, the `tabsByWindowInstanceID`
window map — was absent from the fixtures, so nothing exercised them. The
correctness gate for this milestone is **reading and editing real files**, not
fixture shape. Every task's tests must reflect real structure, and merge is
gated on live smoke.
