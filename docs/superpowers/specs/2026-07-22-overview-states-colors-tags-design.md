# Overview depth — slice 3: states, colours and tags (design)

Date: 2026-07-22
Status: designed, ready for writing-plans.
Roadmap: **slice 3 of the "overview depth" milestone**. Slice 1 (tab management)
shipped v0.9.0; slice 2 shipped as 2a (preset management + tab→preset mapping,
v0.11.0) and 2b (preset group contents + built-in default profiles, v0.12.0).
One slice remains after this: import/export packs (slice 4).
Builds on: 2b's preset model and bundled-catalog idioms (`overview_presets.rs`,
`set_preset_groups`, `app/src/lib/data/overview-groups.json`, the fork-on-edit
path for read-only built-in defaults), the `edit_user_overview` /
`edit_user_tabs` wrappers in `ops.rs` (lock → inline-first → `reshare` →
re-project), and the frontend's drag-reorder and collapsible-`<details>`
patterns.

## 1. Goal

2b opened a preset's `groups` list — *which entity types* an overview shows.
Slice 3 opens the other half of the overview's behaviour: **pilot state**.

Three surfaces, all driven by one shared vocabulary of integer state ids:

1. **Background colours** — which pilot states tint an overview row, in what
   priority, and in what colour.
2. **Colortags** — which pilot states get a tag icon, and in what priority.
3. **Per-preset state filters** — which states a preset hides outright, and
   which it always shows regardless of its group filter.

(1) and (2) are **account-scoped**, living directly in the `core_user`
`overview` container. (3) is **per-preset**, in the same blob 2b edits —
`set_preset_groups` already carries a `// slice 3` note on exactly these two
fields. Everything here is user-file only; no `core_char` writes.

The slice also picks up the overview container's simple boolean settings
(EVE's overview "Misc" options), which sit beside these keys and belong on the
same screen.

## 2. The state model (confirmed from the corpus)

### 2.1 Account-scoped keys

Inside `core_user` root → `overview`:

| Key | Shape | Meaning |
|---|---|---|
| `backgroundStates2` | `(ts, [int])` | states that tint a row — the **enabled subset** |
| `backgroundOrder2` | `(ts, [int])` | **every** known state in priority order (first match wins) |
| `flagStates2` | `(ts, [int])` | states that show a colortag — enabled subset |
| `flagOrder2` | `(ts, [int])` | every known state in colortag priority order |
| `stateColors` | `(ts, {(bytes, int): (f,f,f,f)})` | **sparse** per-state RGBA override |

Two structural facts that shape the design:

- **Enabled and order are independent.** The order lists enumerate the full
  state universe regardless of whether a state is enabled; the `*States2` lists
  are the ticked subset. Toggling a state therefore writes only `*States2`,
  and reordering writes only `*Order2` — never a coupled write.
- **`stateColors` is sparse.** In a corpus account with 20 enabled background
  states, only 13 carry a colour entry. **Absent means "EVE's built-in default
  colour for that state"**, not black. Removing a key is how the UI offers
  "reset to default"; it must never be conflated with writing an explicit
  colour.

`stateColors` keys are `(surface, state_id)` **tuples**. Across the entire
corpus the surface is always `b"background"` — no colortag-colour surface was
ever observed, so colortag rows get toggle + order only, no colour picker. The
parser must still read the dict **generically** by surface string and write back
any non-`background` entry verbatim, so that a surface EVE adds later
round-trips untouched rather than being dropped.

The unsuffixed `backgroundOrder` / `backgroundStates` appear **only** inside
`overview → restoreData`, which `docs/format-notes.md` already classifies as
session state (raw-tree-only). There is therefore **no legacy/modern migration**
here, unlike `tabsettings` → `tabsettings_new`. `restoreData` is left untouched.

### 2.2 Per-preset keys

Each preset blob under `overview → overviewProfilePresets` holds exactly three
integer lists — 2b's `groups` plus:

- `filteredStates` — states this preset filters out.
- `alwaysShownStates` — states this preset always shows.

In the corpus the two are **disjoint** (a preset with 22 filtered states listed
exactly one, unrelated, always-shown state). The UI models this as a per-state
tri-state rather than two independent checkboxes, which makes the disjointness
structural instead of a validation rule.

### 2.3 The state vocabulary and its names

State ids carry no label anywhere in the settings file, and unlike 2b's group
ids there is no ESI or `fsdbinary` source — the id→label map lives in client
script, not data. The names below were derived **positionally**: EVE's in-game
Background tab lists states in exactly `backgroundOrder2` order, so a screenshot
of that tab maps row-by-row onto the array.

The mapping is **independently verified by colour**: all 13 `stateColors`
entries match the colour swatch shown on their mapped row (e.g. id 44 stores
`0.75, 0, 0` and lands on a red row; id 20 stores `0.7, 0.7, 0.7` and lands on a
grey one). `flagOrder2`, which orders the same ids differently, corroborates the
head of the list independently.

| Order pos | id | Label |
|---|---|---|
| 1 | 13 | Pilot is at war with your corporation/alliance |
| 2 | 44 | Pilot is at war with your militia |
| 3 | 52 | Pilot has a limited engagement with you |
| 4 | 11 | Pilot is in your fleet |
| 5 | 12 | Pilot is in your Capsuleer corporation |
| 6 | 14 | Pilot is in your alliance |
| 7 | 15 | Pilot has Excellent Standing |
| 8 | 16 | Pilot has Good Standing |
| 9 | 45 | Pilot is in your militia or allied to your militia |
| 10 | 49 | Pilot is an ally in one or more of your wars |
| 11 | 19 | Pilot has Terrible Standing |
| 12 | 18 | Pilot has Bad Standing |
| 13 | 9 | Pilot has a security status below -5 |
| 14 | 51 | Pilot is a criminal |
| 15 | 50 | Pilot is a suspect |
| 16 | 53 | Pilot has a kill right on them that you can activate |
| 17 | 10 | Pilot has a security status below 0 |
| 18 | 20 | Pilot (agent) is interactable |
| 19 | 21 | Pilot has Neutral Standing |
| 20 | 17 | Pilot has No Standing |
| 21 | 48 | Pilot is in your Non Capsuleer corporation |
| 22 | 66 | Pilot has retribution timer |
| 23 | 68 | **unresolved — below the screenshot fold** |

Two gaps close during implementation (Task 1), both by screenshot:

- **id 68** — the 23rd row, below the visible fold of the Background tab.
- **ids 36 and 37** — these appear in preset `filteredStates` but never in
  `backgroundOrder2` / `flagOrder2`, so the filterable vocabulary is a
  **superset** of the colourable one. Their labels come from a preset's States
  tab in-game.

An id with no bundled label is **not an error**: it displays as a raw `#id` row
and round-trips normally, exactly as 2b handles unrecognised group ids.

### 2.4 The clean-account case

**Verified real:** some accounts carry *none* of the four state keys. A corpus
account decodes cleanly, has an `overview` container with presets and
`tabsettings`, and has no `backgroundStates2`, `backgroundOrder2`,
`flagStates2` or `flagOrder2` at all — EVE falls back to client defaults.

This is structurally the same problem 2b solved for an empty
`overviewProfilePresets`, and gets the same solution: bundle EVE's default
order and enabled arrays, show them when the keys are absent, and **materialise
all four keys on the first edit**. 2b established that EVE accepts a
freshly-minted container of this kind.

## 3. Backend

### 3.1 New module: `crates/settings-model/src/overview_states.rs`

Mirrors `overview_presets.rs` in shape and idiom (inline-first, `OverviewError`,
key helpers). Public surface:

```rust
pub enum StateList { Background, BackgroundOrder, Flag, FlagOrder }

/// Replace one of the four account-scoped state lists.
/// Enabled lists (Background/Flag) are written sorted ascending — EVE's own
/// convention in the corpus, and what set_preset_groups does for `groups`.
/// Order lists are written in the caller's order. Preserves the (ts, _)
/// wrapper; materialises the key (and its siblings) if absent.
pub fn set_state_list(v: &mut Value, which: StateList, ids: &[i64]) -> Result<(), OverviewError>;

/// Set or clear one state's background colour. `None` removes the entry,
/// restoring EVE's built-in default for that state. Entries whose surface is
/// not `b"background"` are preserved untouched.
pub fn set_state_color(v: &mut Value, id: i64, rgba: Option<[f64; 4]>) -> Result<(), OverviewError>;

/// Set one of the overview container's simple boolean settings.
pub fn set_overview_bool(v: &mut Value, key: &str, on: bool) -> Result<(), OverviewError>;
```

`set_overview_bool` takes the key as a string validated against an allow-list of
known boolean keys, rather than an enum per setting — the settings are
homogeneous and the allow-list keeps a typo from minting a junk key. The
`showCategoryInTargetRange_<id>` family is **excluded**: those are keyed by
inventory category and would need group naming to present, which is out of scope.

### 3.2 `overview_presets.rs`

One addition, modelled directly on `set_preset_groups`:

```rust
/// Replace the named preset's two state lists. Both written sorted ascending.
/// `groups` is untouched.
pub fn set_preset_states(v: &mut Value, name: &str, filtered: &[i64], always_shown: &[i64])
    -> Result<(), OverviewTabError>;
```

Taking both lists in one call keeps the tri-state atomic — moving a state from
"filter out" to "always show" is one write, not a remove plus an add that could
interleave.

### 3.3 Read projection (`overview.rs`)

`OverviewColumns` gains:

```rust
pub struct StateSurface { pub enabled: Vec<i64>, pub order: Vec<i64> }
pub struct Appearance {
    pub background: StateSurface,
    pub flag: StateSurface,
    pub colors: Vec<(i64, [f64; 4])>,   // background surface only, sparse
    pub bools: Vec<(String, bool)>,     // known boolean settings present in the file
    pub defaulted: bool,                // true when the state keys were absent
}
```

`Preset` gains `filtered_states: Vec<i64>` and `always_shown_states: Vec<i64>`
beside 2b's `groups`.

`defaulted` lets the frontend say "these are EVE's defaults, not yet saved"
rather than silently presenting bundled data as though it came from the file.

### 3.4 `ops.rs`

Thin commands over the two existing edit wrappers — no new plumbing, since both
already do lock → read-only check → edit → `reshare` → re-project. They differ
only in the error type they accept, so each command uses the one matching its
module:

| Command | Wrapper | Error type |
|---|---|---|
| `overview_state_list_set(state, which, ids)` | `edit_user_overview` | `OverviewError` |
| `overview_state_color_set(state, id, rgba: Option<[f64; 4]>)` | `edit_user_overview` | `OverviewError` |
| `overview_bool_set(state, key, on)` | `edit_user_overview` | `OverviewError` |
| `overview_preset_states_set(state, name, filtered, always_shown)` | `edit_user_tabs` | `OverviewTabError` |

The preset command follows `set_preset_groups`, which is already wired through
`edit_user_tabs`.

### 3.5 Approaches considered and rejected

- **One module for all six lists**, account-scoped and per-preset together.
  Rejected: the per-preset half needs `overview_presets.rs`'s preset-key lookup,
  which would have to be duplicated or made public for no benefit. Scope
  boundary (account container vs preset blob) is the better seam.
- **A generic "set int list at path" command** with all semantics in the
  frontend. Rejected: that is a raw-tree write with extra steps — no validation,
  no materialise-when-absent, no sorted-write convention, and it puts file-format
  knowledge in TypeScript.

## 4. Frontend

### 4.1 Restructuring `OverviewView.svelte`

The view is 518 lines and already carries tabs, windows, presets, 2b's group
checklist and the column editor. The small-tasks ledger flags its UI/UX as
"rough — defer the polish to the later overview-depth slices, which will touch
this same Overview view anyway". This is that slice.

Split into sub-tabs mirroring EVE's own overview settings window:

```
Overview   [ Columns | Filter | Appearance ]
```

- The **tab/window strip** (create/rename/delete/reorder/move, add/remove
  window) stays in the parent `OverviewView.svelte` — it selects *what* the
  three sub-tabs are editing.
- `OverviewColumnsTab.svelte` — the existing column list, moved unchanged.
- `OverviewFilterTab.svelte` — the preset picker, 2b's group checklist, and the
  new per-state tri-state list.
- `OverviewAppearanceTab.svelte` — the two state lists and the boolean settings.

This is a move-and-split, not a rewrite: existing column and preset behaviour is
carried over as-is so the diff stays reviewable.

### 4.2 Filter tab — per-preset states

A list of states (bundled vocabulary, plus any unrecognised id present in the
file), each a three-way radio:

| Choice | Effect |
|---|---|
| Default | in neither list |
| Always show | in `alwaysShownStates` |
| Filter out | in `filteredStates` |

Reuses the group checklist's filter box. Editing a **built-in default preset**
auto-forks a user copy exactly as `setPresetGroup` does today — same `forkName`
helper, same mint-the-container path — so built-ins stay read-only.

### 4.3 Appearance tab

Two ordered lists, Background and Colortag, each row: checkbox (enabled) +
label + drag handle (priority). Background rows additionally carry a colour
swatch — a native `<input type="color">`, no dependency — and a **Reset** action
that removes the `stateColors` entry, restoring EVE's default. A row with no
stored colour shows EVE's default and is visually marked as unset, so "unset"
and "explicitly set to this colour" stay distinguishable.

Alpha is not exposed: every observed entry is `1.0`. When an entry already
carries a non-1.0 alpha, that alpha is **preserved** on a colour edit rather
than silently reset — the picker changes RGB only.

Below the lists, the overview container's boolean settings as labelled
checkboxes, driven off one small key→label table.

The whole tab writes account-wide, so it sits behind the existing shared-account
banner (`sharedWith` in `overview.ts`), which already names the sibling
characters an account write also affects.

### 4.4 Bundled data

New `app/src/lib/data/overview-states.json`:

```json
{
  "states": { "9": "Pilot has a security status below -5", "...": "..." },
  "defaultBackgroundOrder": [13, 44, "..."],
  "defaultBackgroundStates": [9, 10, "..."],
  "defaultFlagOrder": [13, 44, "..."],
  "defaultFlagStates": [9, 10, "..."]
}
```

Hand-authored from the screenshots, like the hand-corrected
`default-preset-names.json`. It carries a header comment recording that the
mapping is positional-against-`backgroundOrder2` and colour-verified, so a
future editor knows how to re-derive it. The default arrays are lifted from a
corpus account that has the keys, and serve the §2.4 clean-account case.

Native form controls (`input[type=color]`, checkboxes, radios) get explicit dark
background and colour per the standing WebView2 gotcha — light-rendering native
controls in the dark shell.

## 5. Testing

Backend unit tests, in the style of the existing `overview_presets` tests:

- `set_state_list` — replaces each of the four keys; enabled lists come out
  sorted; order lists keep caller order; the `(ts, _)` wrapper survives.
- **Absent-key materialise** — a fixture with no state keys gains all four on
  first edit (§2.4).
- `set_state_color` — adds, overwrites, and (with `None`) removes an entry;
  a non-`background` surface entry is preserved untouched; non-1.0 alpha is
  preserved across an RGB edit.
- `set_overview_bool` — sets an existing key; rejects a key outside the
  allow-list.
- `set_preset_states` — writes both lists sorted; leaves `groups` untouched;
  `UnknownPreset` for a bad name.
- Projection — `defaulted` is true exactly when the keys were absent;
  unrecognised ids survive a read/write round-trip.

Frontend tests: tri-state ↔ two-list mapping in both directions, and
fork-on-edit for a built-in default preset.

Closing with a live in-game smoke, as with 2a and 2b: edit each surface, launch
EVE, confirm the client accepts the file and shows the change.

## 6. Non-goals

- **Colortag colours / tag graphics.** No corpus evidence of a `stateColors`
  surface other than `background`. Colortag rows get toggle and order only.
  Non-`background` entries are preserved verbatim if EVE writes them.
- **`showCategoryInTargetRange_<id>`.** Category-keyed, needs group naming.
- **`restoreData`.** Session state, raw-tree-only per `format-notes.md`.
- **Import/export of state settings.** Slice 4 covers packs, including these.
- **Alpha editing.** §4.3 — preserved, not exposed.
