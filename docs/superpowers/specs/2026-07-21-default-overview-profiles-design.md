# Default overview-profile support (design)

Date: 2026-07-21
Status: designed, ready for writing-plans.
Roadmap: a **fix/extension surfaced by the 2b live smoke** — 2a's preset
management and 2b's contents editor silently don't work on an account that has
never customized its overviews. Builds directly on 2a (`overview_presets.rs`,
`set_tab_preset`) and 2b (the contents editor, the group catalog + `overview_dump`
tool, the `default-preset-names.json` label snapshot).

## 1. Problem (confirmed from the live client)

EVE ships its built-in overview profiles ("All", "General", "Mining", "PvP", …)
in **static data**, not the settings file. A **clean/uncustomized account** has an
**empty `overviewProfilePresets`**; its tabs reference the built-ins by key. EVE
materializes a profile into the file only once the user edits it.

2a/2b assume **every tab references a preset that is stored in
`overviewProfilePresets`** (true only on accounts that have customized their
overviews — which is why the corpus and the 2a smoke passed). On a clean account
the projection returns **0 presets**, so:

- the preset dropdown lists only the current tab's reference (nothing to switch to);
- `presetIsReal` is false → Duplicate / Rename / Delete are disabled ("nothing
  happens" on Duplicate);
- the 2b contents section (`{#if presetIsReal && tab}`) is hidden → **you cannot
  edit groups at all**.

**Corpus-confirmed.** De l'opera's account `core_user_32945923.dat`: 0 stored
presets, 5 tabs → `DefaultPreset_639452` (General), `_639437` (Targets), `_639442`
(Mining), `_639448` (WarpTo), `_639443` (All), all `in_presets=false`. Across the
corpus, all 36 modern `DefaultPreset_63943x..63946x` **and** 6 legacy `default*`
profiles are materialized (with groups) in at least one file — so their
definitions are extractable.

Two on-disk regimes exist: **modern** (`tabsettings_new`, `DefaultPreset_<id>`
keys) and **legacy** (`tabsettings`, `default*` literal keys). Both must be
handled.

## 2. Model (matches EVE)

- **Default profiles are read-only templates**, always available to assign,
  sourced from a bundle (they may not be in the file).
- **Editing a default forks it** into a new user profile; the built-in stays
  pristine.
- **User profiles** (stored in `overviewProfilePresets`) are edited in place, as
  2a/2b already do.

## 3. Bundle the default definitions

A committed `app/src/lib/data/default-presets.json`:

```json
{ "modern": [ { "key": "DefaultPreset_639443", "name": "All",
               "groups": [ints], "filteredStates": [ints], "alwaysShownStates": [ints] } ],
  "legacy": [ { "key": "defaultall", "name": "All", "groups": [ints],
               "filteredStates": [ints], "alwaysShownStates": [ints] } ] }
```

- Extracted by the **`overview_dump` tool** (extended to dump each materialized
  default's full `{groups, filteredStates, alwaysShownStates}` blob) run over the
  corpus; for a default materialized in several files, take the richest (max
  groups) copy.
- **Names**: modern from the existing `default-preset-names.json` snapshot; legacy
  from a hardcoded map (`defaultall`→"All", `defaultpvp`→"PvP", `defaultmining`→
  "Mining", `defaultloot`→"Loot", `defaultdrones`→"Drones", `defaultwarpto`→"Warp
  To", `default`→"Default").
- **State lists are included** so a fork stays faithful even though 2b only
  *edits* groups (states are slice 3 — carried, not edited).
- Regenerated on an EVE update if CCP changes the defaults (rare).

**"Is this key a default?"** — the bundle is the authority: a key present in
`default-presets.json` (either set) is a default. Equivalent detection for
labels/UI: `/^DefaultPreset_\d+$/` or a known legacy literal.

## 4. Projection / dropdown merge (frontend)

The preset dropdown options = the account's **stored presets** ∪ the **bundled
defaults for the account's format**, deduped by key (a *materialized* default
appears once, using the file's stored copy). Rendered with `<optgroup>`:

- **"Default profiles"** — the default keys (bundled or stored-but-default).
- **"Your profiles"** — user-created keys.

Account format = modern if the overview uses `tabsettings_new`, legacy if
`tabsettings` (the projection already distinguishes these). Offer the matching
default set only. Labels via `labelFor`, extended to resolve the legacy literals
(§3) — this alone also makes the defaults **recognizable** in the dropdown
(previously shown as raw "defaultall").

## 5. Assigning a default (already works)

Selecting any default → `tabSetPreset(tab_idx, key)` sets the tab's `overview`
reference. **No materialization needed** — EVE fills the definition from static
data on load. 2a's `set_tab_preset` already does exactly this; the only gap was
the dropdown not *listing* unmaterialized defaults, which §4 closes.

## 6. Rendering a default's contents

When the selected tab's preset is a default, the contents checklist's membership
set comes from the **stored blob if materialized, else the bundled definition**.
So the frontend's `presetGroups` derived resolves from `data.presets` ∪ the
bundled default. The contents-section gate changes from `presetIsReal` to
**`presetIsReal || isBundledDefault(tab.preset)`**, so clean accounts can see and
edit.

## 7. Fork on edit / duplicate

Toggling a group on a default, **or** clicking Duplicate while a default is
selected:

1. Compute a unique fork name = `"<localized name> copy"` (append " 2", " 3"… if
   taken).
2. Create the fork from the default's full blob (groups + both state lists), from
   the file if materialized else the bundle — **with the edit already applied**
   (the toggled group set; unchanged for a plain Duplicate).
3. Retarget the current tab to the fork.
4. The tab now uses the fork — a normal user profile, edited in place thereafter.

**Backend:** one new command `preset_fork(state, tab_idx, name, groups,
filtered_states, always_shown_states) -> OverviewColumns` in the `overview_presets`
+ `ops`/`lib` layers: create the `{groups, filteredStates, alwaysShownStates}`
blob under `name` (a default may not be in the file, so we cannot reuse
`create_preset` which clones an existing key), then retarget `tab_idx` to it, all
through the existing `edit_user_tabs` inline→edit→reshare path. `PresetExists` if
`name` is taken. The frontend passes the toggled group set (from the displayed
membership — file if materialized, else bundle) plus **both state lists read from
the bundled default definition** — the projection exposes only groups (2b), and
every default has a bundle entry, so the bundle is the states source for a fork.
(A materialized default equals its static-data definition, since editing a default
forks rather than mutating it, so the bundle is authoritative here.)

Fork naming is **silent/auto** (no prompt) per the design decision — rename later
via the (now-enabled) Rename button on the user copy.

## 8. Read-only defaults

**Rename** and **Delete** are disabled whenever the selected preset is a default
(`isDefault(tab.preset)`) — you cannot modify a built-in, matching EVE. Editing
(group toggle) is *not* blocked; it forks (§7). Assigning a default to a tab is
fine (§5).

## 9. Legacy accounts

**Match the account's format** (design decision): a legacy account is offered the
legacy default set, a modern account the modern set. Forks are **name-keyed user
profiles** (format-agnostic) written into the account's existing
`overviewProfilePresets`; the tab retarget writes the tab's `overview` field in
whichever tab dict exists (`tabsettings` or `tabsettings_new`) — 2a's
`set_tab_preset` already handles both, so no forced legacy→modern migration is
needed here.

## 10. Testing

- **Backend** (`overview_presets` unit tests): `preset_fork` creates the blob from
  the given lists, retargets the tab, preserves the state lists, and errors
  `PresetExists`; on a synthetic clean tree (empty `overviewProfilePresets`) the
  fork materializes the first entry.
- **Frontend** (pure, node-testable): the dropdown merge (stored ∪ bundled,
  dedup, format-match, optgroup split); `isDefault` detection (modern + legacy);
  fork-name uniqueness; `presetGroups` resolving from the bundle when a default is
  unmaterialized; `labelFor` legacy-literal resolution.
- **Generator**: `overview_dump` blob-dump mode; `default-presets.json` non-empty
  with both `modern` and `legacy` sets covering the referenced defaults.
- **Round-trip guard**: a fork on a clean tree reshares and re-decodes equal.
- **Live smoke (De l'opera — the failing clean account)**: dropdown lists all
  defaults grouped with real names; switch a tab to a different default; edit a
  default's groups → auto-forks to "<name> copy", tab follows, built-in untouched;
  Duplicate switches to the copy; Rename/Delete disabled on defaults; save →
  reopen → EVE reflects it.

## 11. Non-goals / ceilings

- **Editing default STATES** — slice 3 (states are carried through a fork, not
  edited here).
- A **default not in the bundle** (a brand-new CCP default cut after the bundle)
  → still assignable by key, but its contents can't render/fork until the bundle
  is refreshed (`ponytail:` ceiling, mirrors the group-catalog ceiling).
- Bracket presets, standing/state colors + tags — out of scope.

## 12. Relationship to 2b

This completes 2b's usability (2b's contents editor is unreachable on clean
accounts without it). Whether it lands on the current `overview-filter-presets-2b`
branch before merge or as an immediate follow-up is a writing-plans/execution
decision; the design assumes 2b's editor, catalog, and `overview_dump` tool are
present.
