# Overview filter presets — slice 2b: preset contents (group) editor (design)

Date: 2026-07-21
Status: designed, ready for writing-plans.
Roadmap: **slice 2 of the "overview depth" milestone**, second half. Slice 2 was
split into **2a** (preset management + tab→preset mapping — shipped v0.11.0, PR
#15) and **2b (this spec)**. Remaining overview-depth slices after this:
standing/state colors + tags (slice 3) → import/export packs (slice 4).
Builds on: 2a's preset model and idioms (`overview_presets.rs`, inline-first →
edit → `reshare`; `ops.rs` `edit_user_tabs`), the overview projection
(`overview.rs` `project_overview`, which already lists preset names), the ESI
resolve/cache pattern (`names.rs`), and the frontend's collapsible `<details>`
sidebar + autofill search-box patterns.

## 1. Goal

2a made presets first-class **named entities** you can assign/duplicate/rename/
delete, but treated each preset's filter definition as an **opaque blob**. 2b
opens that blob: **edit which entity groups a preset shows.** A preset's `groups`
list (EVE inventory **group IDs** — Frigate, Cruiser, Drone, Structure, …) is the
bulk of what "this overview shows"; 2b makes it editable through a category-
grouped, human-named checklist instead of raw IDs.

**Scope is groups only.** A preset also carries two state-filter lists
(`filteredStates` / `alwaysShownStates`); those overlap heavily with **slice 3**
(standing/state colors + tags, which is entirely about the same state flags), so
all state-flag work — filtering *and* coloring/tags — lands together in slice 3.
2b reads and writes only `groups`, leaving the two state lists untouched (as 2a's
create/duplicate already preserves them).

Everything here is **user-file only** (account-scoped): the preset definitions
live in the `core_user` `overview` container, same as 2a. No `core_char` writes.

## 2. The preset-contents model (confirmed from the corpus)

From 2a (`docs/…/2026-07-20-overview-filter-presets-2a-design.md` §2): in the
`core_user` file, `overview` → `overviewProfilePresets` is a `(timestamp, dict)`
keyed by preset name; each value is a dict of exactly three integer lists:

- **`groups`** — EVE inventory **group IDs** to show. The bulk of a preset; the
  only list 2b edits.
- `filteredStates`, `alwaysShownStates` — state-flag ints. **Untouched by 2b**
  (slice 3).

On real files these three lists are `Shared`/`Ref` (interned across presets), so
edits go through the standard inline-first idiom — the whole document is inlined,
the target list is rewritten, then `reshare` re-derives canonical sharing before
encode. A group ID is an opaque integer to the backend: it stores and reorders
them but never needs to know a group's *name* — naming is a pure frontend/display
concern (§4). No character/account ids or real preset names appear in this
document, per the repo data rule.

## 3. Backend — read projection (`overview.rs`)

Enrich the preset projection to carry each preset's group IDs. Today
`OverviewColumns.presets` is `Vec<String>` (names only, for 2a's picker and
duplicate/delete neighbour logic). Change it to:

```
pub struct Preset { pub name: String, pub groups: Vec<i64> }
// OverviewColumns.presets: Vec<Preset>   (was Vec<String>)
```

`groups` is read from each preset's `groups` list, unwrapping `Shared`/`Ref`
through the existing `treewalk` helpers (`effective`, `as_int`, …) exactly as the
rest of the projection does. Preserve the stored order on read — display order is
a frontend concern (the checklist keys off a membership set, not order). Presets
stay **sorted case-insensitively by name** (unchanged from 2a).

**Ripple:** the 2a frontend consumes `presets` as `string[]` (picker options and
the delete-neighbour computation). Those few call sites become `presets.map(p =>
p.name)`. Cleaner than a parallel `preset_groups` array — one representation of a
preset.

## 4. Backend — authoring (`overview_presets.rs`)

One new function, next to 2a's `create_preset` / `rename_preset` / `delete_preset`
and reusing their `pub(crate)` helpers:

- **`set_preset_groups(v, name, groups: &[i64])`** — full-replace the named
  preset's `groups` list with a fresh `Value::List` of `Value::Int`s, **sorted
  ascending** for a deterministic on-disk order (the client re-sorts for display
  anyway). `UnknownPreset { name }` if the preset is absent (reuse the existing
  `OverviewTabError` — no new error variant needed). If the `groups` key is
  somehow absent on the target preset, insert it (defensive; real presets always
  carry it).

Full-replace — not add/remove deltas — because the checklist naturally holds the
whole desired set; one command, one code path, one test. The blob's other two
lists (`filteredStates`, `alwaysShownStates`) are not read or written.

## 5. Backend — command (`ops.rs` + `lib.rs`)

Thin wrapper through the existing `edit_user_tabs` (inline → edit → `reshare` →
re-project → return the refreshed `OverviewColumns`, now carrying enriched
presets), user-file only:

- `preset_set_groups(state, name, groups)`

Each checkbox toggle in the UI is one call that marks the user document dirty —
the same per-edit round-trip pattern as slice 1's column-visibility toggles.
(Per-toggle full-document `reshare` cost is already tracked by the open
reshare-profiling small-task; 2b adds no new perf concern.)

## 6. Group-name catalog

The backend stores opaque group IDs; only the **frontend** needs to turn a group
ID into a human name and slot it under a category to render the checklist. The
catalog is **bundled static + an append-only, ESI-synced delta** — never
invalidated, because EVE group IDs are immutable (a group ID's name/category
never changes), so the catalog only ever *grows*.

### 6.1 Bundle (frontend JSON, cut at build time)

A committed JSON asset under `app/src/lib/` carrying two things:

- The **overview-relevant category→group tree with names** — what the checklist
  renders: `[{ category_id, category_name, groups: [{ id, name }] }]`.
- A flat list of **all** group IDs known at cut time (just ints).

The all-IDs list is what keeps the sync cheap (§6.2): diffing current ESI groups
against the *full* known-ID set — not just the relevant subset — means the delta
is only what CCP genuinely added, not the ~1600 irrelevant groups that would
otherwise all look "new".

### 6.2 Backend `groups.rs` (mirrors `names.rs`)

A new module structurally parallel to `names.rs`: cache-forever JSON
(`groups-cache.json` in the app-data dir), all failures silent (fall back to
bundle ∪ cache), and an **injectable fetcher** so the sync logic unit-tests
without the network. On startup (§ open-decision B: startup, alongside the
existing character-name resolution):

1. `GET /status` → `server_version`. Compare to the stored `synced_server_version`.
   Unchanged → do nothing (return the cache as-is).
2. Changed (or first run): enumerate `/universe/groups/` → current group IDs
   (IDs only, ~2 paginated requests, cheap). Delta = current − (bundle-all-IDs ∪
   cache).
3. For each delta ID: `/universe/groups/{id}` → name + `category_id` (+
   `published`); `/universe/categories/{id}` → category name (cached, so a
   category is fetched at most once). **Keep** only IDs whose category is
   overview-relevant (i.e. already present in the bundle's category set) and
   published.
4. Write the kept additions to `groups-cache.json` and update
   `synced_server_version`. On any fetch failure, leave the cache/version
   unchanged and return what's there — a stale-but-working catalog, never an
   error.

Exposed as a `lib.rs`/`ops.rs` command (like `resolve_character_names`) returning
the cached additions: `[{ id, name, category_id, category_name }]`.

### 6.3 Frontend merge

The frontend renders **bundle ∪ cached additions**: a synced new group appears
under its (bundle-known) category, fully addable. Any group ID present in a preset
but in neither bundle nor cache renders as a bare `#id` (never silently dropped —
it still round-trips through the backend as an opaque int).

### 6.4 Ceiling (`ponytail:`)

A new group whose **category** is itself brand-new (not in the bundle) is not
auto-placed and stays a bare `#id` until the next bundle refresh. New *categories*
are far rarer than new *groups*, so this is a mild, rare fallback — not worth
handling now. Upgrade path: refresh the bundle, or extend §6.2 step 3 to admit
new in-space categories.

## 7. Bundle generation (dev-time, committed)

A small committed dev script (`tools/gen-overview-groups.py`, python stdlib —
mirroring the existing `tools/gen-default-preset-names.py`) that hits ESI once and
writes the bundle JSON to `app/src/lib/data/overview-groups.json`. **Relevance is
a documented, hardcoded allowlist of overview-relevant ("in-space") category
IDs** — the categories whose items appear on the overview (Ship, Drone,
Celestial, Structure, Deployable, Fighter, Entity/NPC, Orbital, …). For each
allowlisted category: `/universe/categories/{id}` → name + group IDs;
`/universe/groups/{id}` → group name + `published`; bundle the **published**
groups under each category. Also enumerate `/universe/groups/` for the flat
**all-group-IDs** list (the §6.1 sync-diff baseline). Refuse to overwrite the
committed snapshot with an empty result (mirrors the sibling script). Re-run and
re-commit on an app release when CCP adds groups, tuning the allowlist there if a
new in-space category appears.

*(A corpus-derived allowlist was considered but rejected: corpus group IDs are
binary-encoded marshal ints — not raw-byte-scannable like the `DefaultPreset_<id>`
text tokens the sibling script keys on — so deriving them in a python byte-scan
isn't practical, and the hardcoded in-space set is in any case more complete than
"categories some corpus file happened to filter on".)*

## 8. Frontend — the contents editor (`OverviewView.svelte`, new `groups.ts`, bundle JSON)

Below 2a's preset-management controls, a **contents section** that edits the
currently-picked preset's groups:

- A **filter box** on top (reusing the autofill search-box pattern) that narrows
  the tree by group/category name as you type.
- The **category→group tree** as native `<details>` per category (reusing the
  sidebar's collapsible pattern), a **checkbox per group**; checked = the group
  ID is in the preset's `groups` set. Toggling a box computes the next set and
  calls `presetSetGroups(name, nextGroups)` + `onUserDirty()`.
- Unknown IDs (in the preset, not in the catalog) surface as a `#id` row so they
  are visible and removable rather than silently lost.

`groups.ts` (no Svelte/Tauri deps, so `node --test`-able) holds the pure logic:
merge bundle ∪ additions into a render tree, the name-filter, and the
membership-set toggle. Native `<input type="checkbox">` / `<input>` get explicit
dark styling per the standing WebView2 gotcha. **State filters are not rendered**
(slice 3).

## 9. Testing

- `overview_presets` unit tests (synthetic trees): `set_preset_groups` replaces
  the list with the sorted set and errors `UnknownPreset` on an absent preset;
  the insert-when-`groups`-absent defensive branch.
- Projection: each preset's `groups` is exposed; a realshape case with the
  `(timestamp, dict)` wrapper and `Shared`/`Ref` group lists.
- Round-trip guard: the edit path `reshare`s and re-decodes equal (standard
  reshare regression check).
- `groups.rs` (injected fetcher, no network — mirrors the `names.rs` suite):
  delta diff resolves only genuinely-new IDs; the `server_version` gate skips
  re-enumeration when unchanged; a fetch failure falls back to the cache
  silently; relevance filter drops out-of-catalog-category IDs; cache round-trips
  through disk.
- `groups.ts`: bundle ∪ additions merge, name filter, membership toggle.
- Final acceptance: a live smoke — edit a preset's groups in-app → save → confirm
  EVE reflects the change (and no phantom/duplicate preset, per 2a's sibling
  check).

## 10. Open decisions (defaults taken)

- **A.** `OverviewColumns.presets` enriched to `Vec<{name, groups}>` (chosen) vs.
  a separate parallel `preset_groups` field. → **Enrich.**
- **B.** Catalog sync timing. Original default was **on app startup**; the
  implementation instead fires it **on first mount of the Overview view** (its
  only consumer) — chosen during build as the better trade (no ESI hit if the
  user never opens Overview; the backend `server_version` gate makes remounts
  cheap). The checklist is seeded synchronously from the static bundle so it
  renders instantly offline; the sync only *upgrades* it with additions. →
  **On Overview-view mount, bundle-seeded.**

## 11. Dependencies, scope, non-goals

- **Depends on:** nothing new structurally — reuses 2a's preset idioms, the
  `edit_user_tabs` command path, the `names.rs` resolve/cache pattern, and the
  frontend collapsible/search patterns. Adds one committed data asset (the
  bundle) and its generation script.
- **Non-goals (2b):** editing state filters (`filteredStates` /
  `alwaysShownStates`) — **slice 3**, together with standing/state colors + tags;
  import/export YAML packs — **slice 4**; the broader Overview-view UX rework
  (flagged rough during earlier smokes) — still open debt, not this slice.
- **Ceilings:** the brand-new-category naming fallback (§6.4); per-toggle
  full-document reshare (existing reshare-profiling task).
