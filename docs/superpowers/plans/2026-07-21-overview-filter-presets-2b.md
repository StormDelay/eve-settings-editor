# Overview filter presets — slice 2b (preset contents editor) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user edit which entity **groups** an overview filter preset shows, through a category-grouped, human-named checklist — the second half of overview-depth slice 2.

**Architecture:** Enrich the existing preset projection to carry each preset's group IDs; add one full-replace authoring command through the established inline→edit→`reshare` path; name/browse groups from a bundled static catalog (generated once from ESI) merged with an append-only, ESI-synced delta for groups CCP adds later; render a `<details>` category tree with checkboxes in the existing Overview view.

**Tech Stack:** Rust (`settings-model` crate + Tauri `src-tauri`), TypeScript/Svelte 5 (runes), python stdlib (dev-time bundle generator), ESI (`esi.evetech.net`). Design: `docs/superpowers/specs/2026-07-21-overview-filter-presets-2b-design.md`.

## Global Constraints

- **Sentence-case commit subjects, NO attribution trailers** (repo convention). No `Co-Authored-By`.
- **Zero new runtime dependencies.** Backend reuses `reqwest`/`serde_json` (already used by `names.rs`); the generator is python **stdlib only**; frontend adds no npm deps.
- **Scope is groups only.** Do NOT read or write `filteredStates` / `alwaysShownStates` — those are slice 3. Preserve them untouched.
- **User-file only.** All edits go through the `core_user` `overview` container via `edit_user_tabs`; no `core_char` writes.
- **No personal data / real preset names / character ids** in code, tests, or commits (repo data rule). Tests use synthetic trees.
- **Group IDs are opaque ints to the backend** — it stores/reorders them and never needs a name; naming is a frontend concern.
- **Test commands** (run from repo root unless noted):
  - Rust: `cargo test -p settings-model`
  - Frontend unit: `npm test` (from `app/`, via PowerShell — npm is not on the Bash PATH) → `node --test "src/lib/**/*.test.ts"`. Single file: `node --test src/lib/groups.test.ts`.
  - Frontend types/build: `npm run check` and `npm run build` (from `app/`, PowerShell).
- **Bundle shape** (`app/src/lib/data/overview-groups.json`) is fixed across tasks:
  ```json
  { "categories": [ { "id": 6, "name": "Ship", "groups": [ { "id": 25, "name": "Frigate" } ] } ],
    "all_group_ids": [ 25, 26, 27 ] }
  ```
- **Synced-addition shape** (`GroupEntry`, backend→frontend): `{ id: number, name: string, category_id: number, category_name: string }`.

---

## Task 1: Enrich the preset projection with group IDs

Change `OverviewColumns.presets` from `Vec<String>` to `Vec<Preset>` end-to-end (Rust projection + api.ts type + the three `OverviewView.svelte` consumers), leaving existing behavior identical. No new UI yet.

**Files:**
- Modify: `crates/settings-model/src/overview.rs` (struct + `preset_names` → `presets_with_groups`, ~line 25-30, 67, 159-165)
- Modify: `crates/settings-model/tests/overview_presets_realshape.rs:54-67`
- Modify: `crates/settings-model/src/overview.rs` (inline projection test ~line 1347)
- Modify: `app/src/lib/api.ts` (interfaces ~172-193)
- Modify: `app/src/lib/OverviewView.svelte:40-47,154`

**Interfaces:**
- Produces (Rust): `pub struct Preset { pub name: String, pub groups: Vec<i64> }`; `OverviewColumns.presets: Vec<Preset>`.
- Produces (TS): `export interface Preset { name: string; groups: number[] }`; `OverviewColumns.presets: Preset[]`.

- [ ] **Step 1: Update the realshape test to the new shape and assert groups (failing test)**

In `crates/settings-model/tests/overview_presets_realshape.rs`, replace the four `cols.presets.contains(&"…".to_string())` assertions (lines 54-55, 66-67) with name lookups, and add a groups assertion to the first (projection) test. For example, the first test's assertions become:

```rust
    assert!(cols.presets.iter().any(|p| p.name == "pvp2"));
    assert!(!cols.presets.iter().any(|p| p.name == "pvp"));
    // 2b: the projection now carries each preset's group IDs. The fixture's "pvp"
    // preset (renamed to "pvp2") has groups:[25]; it survives reshare.
    let pvp2 = cols.presets.iter().find(|p| p.name == "pvp2").unwrap();
    assert_eq!(pvp2.groups, vec![25]);
```

And the duplicate test's assertions (lines 66-67):

```rust
    assert!(cols.presets.iter().any(|p| p.name == "pvp copy"));
    assert!(!cols.presets.iter().any(|p| p.name == "pve"));
```

- [ ] **Step 2: Update the inline projection test in `overview.rs`**

At `crates/settings-model/src/overview.rs:1347`, the assertion `assert_eq!(cols.presets, vec!["alpha".to_string(), "Zeta".to_string()]);` compares against `Vec<String>`. Change it to compare names:

```rust
        assert_eq!(
            cols.presets.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
            vec!["alpha".to_string(), "Zeta".to_string()]
        );
```

- [ ] **Step 3: Run the Rust tests to verify they fail to compile**

Run: `cargo test -p settings-model`
Expected: FAIL — `Preset` is undefined / `presets` is still `Vec<String>` (type/field errors).

- [ ] **Step 4: Add the `Preset` struct and change the field**

In `crates/settings-model/src/overview.rs`, add the struct next to `OverviewColumns` (after line 30) and change the field:

```rust
#[derive(Debug, Serialize, PartialEq)]
pub struct Preset {
    pub name: String,
    pub groups: Vec<i64>,
}
```

Change `pub presets: Vec<String>,` (line 29) to `pub presets: Vec<Preset>,`. The empty init at line 60 (`presets: vec![]`) is unchanged.

- [ ] **Step 5: Replace `preset_names` with `presets_with_groups`**

Change the call site at line 67 from `let presets = preset_names(overview, &sh);` to `let presets = presets_with_groups(overview, &sh);`, and replace the `preset_names` function (lines 159-165) with:

```rust
/// Each preset's name and its group IDs, sorted case-insensitively by name (the
/// SAME order the picker/neighbour logic uses). `groups` is the preset's `groups`
/// list (empty if absent); the two state lists are not read here (slice 3).
fn presets_with_groups(overview: &Entries, sh: &SharedTable) -> Vec<Preset> {
    let Some(dict) = find_child(overview, b"overviewProfilePresets", sh).and_then(|v| as_dict(v, sh))
    else { return vec![] };
    let mut out: Vec<Preset> = dict
        .iter()
        .filter_map(|(k, v)| {
            let name = preset_key_name(effective(k, sh))?;
            let groups = as_dict(v, sh)
                .and_then(|d| find_child(d, b"groups", sh))
                .and_then(|g| as_list_r(g, sh))
                .map(|l| l.iter().filter_map(|e| as_int(effective(e, sh))).collect())
                .unwrap_or_default();
            Some(Preset { name, groups })
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}
```

- [ ] **Step 6: Run the Rust tests to verify they pass**

Run: `cargo test -p settings-model`
Expected: PASS (all overview + preset tests, including the updated realshape/projection assertions).

- [ ] **Step 7: Update the frontend type**

In `app/src/lib/api.ts`, add the `Preset` interface and change `OverviewColumns.presets` (lines 189-193):

```ts
export interface Preset {
  name: string;
  groups: number[];
}
export interface OverviewColumns {
  tabs: OverviewTab[];
  windows: OverviewWindow[];
  presets: Preset[];
}
```

- [ ] **Step 8: Fix the three `OverviewView.svelte` consumers**

In `app/src/lib/OverviewView.svelte`:

- `presetOptions` (line 41): `const list = data?.presets ?? [];` → `const list = (data?.presets ?? []).map((p) => p.name);`
- `presetIsReal` (line 47): `(data?.presets.includes(tab.preset) ?? false)` → `(data?.presets.some((p) => p.name === tab.preset) ?? false)`
- `deletePreset` (line 154): `const list = data.presets;` → `const list = data.presets.map((p) => p.name);`

(The template `(data?.presets.length ?? 0) <= 1` at line 300 needs no change.)

- [ ] **Step 9: Run frontend type-check**

Run (from `app/`, PowerShell): `npm run check`
Expected: PASS — no type errors; `presets` is `Preset[]` and every consumer uses `.name`.

- [ ] **Step 10: Commit**

```bash
git add crates/settings-model/src/overview.rs crates/settings-model/tests/overview_presets_realshape.rs app/src/lib/api.ts app/src/lib/OverviewView.svelte
git commit -m "Project each overview preset's group IDs"
```

---

## Task 2: Backend authoring — `set_preset_groups`

Add the full-replace authoring function, its `ops` wrapper, the Tauri command, and the api.ts binding. No UI wiring yet.

**Files:**
- Modify: `crates/settings-model/src/overview_presets.rs` (new fn + tests)
- Modify: `crates/settings-model/src/lib.rs:39` (re-export)
- Modify: `app/src-tauri/src/ops.rs:20-23` (import) and near line 765 (wrapper)
- Modify: `app/src-tauri/src/lib.rs` (command ~line 199 + handler list ~285)
- Modify: `app/src/lib/api.ts` (binding ~287)

**Interfaces:**
- Consumes: `OverviewTabError` (existing; reuse `UnknownPreset { name }`), `presets_mut`, `as_str`, `dict_inner_mut`, `is_b`, `overview_mut`, `inline_all` (existing `pub(crate)` helpers).
- Produces (Rust): `pub fn set_preset_groups(v: &mut Value, name: &str, groups: &[i64]) -> Result<(), OverviewTabError>`; `ops::preset_set_groups(state, name: String, groups: Vec<i64>) -> Result<OverviewColumns, ErrDto>`.
- Produces (TS): `api.presetSetGroups(name: string, groups: number[]) => Promise<OverviewColumns>` (command `preset_set_groups`).

- [ ] **Step 1: Write failing unit tests**

In `crates/settings-model/src/overview_presets.rs`, add a reader helper and three tests inside the existing `mod tests` (the `user_with_presets` fixture, `b`, `is_b`, `as_str` are already in scope):

```rust
    fn preset_groups(v: &Value, name: &str) -> Vec<i64> {
        let Value::Dict(root) = v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, p) = ovd.iter().find(|(k, _)| is_b(k, b"overviewProfilePresets")).unwrap();
        let Value::Tuple(items) = p else { panic!() };
        let Value::Dict(pd) = &items[1] else { panic!() };
        let (_, blob) = pd.iter().find(|(k, _)| as_str(k).as_deref() == Some(name)).unwrap();
        let Value::Dict(bf) = blob else { panic!() };
        let (_, groups) = bf.iter().find(|(k, _)| is_b(k, b"groups")).unwrap();
        let Value::List(l) = groups else { panic!() };
        l.iter().filter_map(|e| if let Value::Int(n) = e { Some(*n) } else { None }).collect()
    }

    #[test]
    fn set_groups_replaces_with_sorted_list() {
        let mut v = user_with_presets();
        set_preset_groups(&mut v, "alpha", &[30, 10, 20]).unwrap();
        assert_eq!(preset_groups(&v, "alpha"), vec![10, 20, 30]);
    }

    #[test]
    fn set_groups_unknown_preset_errors() {
        let mut v = user_with_presets();
        assert!(matches!(
            set_preset_groups(&mut v, "nope", &[1]),
            Err(OverviewTabError::UnknownPreset { .. })
        ));
    }

    #[test]
    fn set_groups_inserts_groups_key_when_absent() {
        // A preset blob with no `groups` key at all.
        let overview = Value::Dict(vec![(b("overviewProfilePresets"), Value::Tuple(vec![
            Value::Int(1),
            Value::Dict(vec![(b("solo"), Value::Dict(vec![]))]),
        ]))]);
        let mut v = Value::Dict(vec![(b("overview"), overview)]);
        set_preset_groups(&mut v, "solo", &[5, 1]).unwrap();
        assert_eq!(preset_groups(&v, "solo"), vec![1, 5]);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p settings-model set_groups`
Expected: FAIL — `set_preset_groups` is not defined.

- [ ] **Step 3: Implement `set_preset_groups`**

In `crates/settings-model/src/overview_presets.rs`, after `delete_preset` (line 136), add:

```rust
/// Replace the named preset's `groups` list with `groups`, sorted ascending for a
/// deterministic on-disk order (EVE re-sorts for display). Inserts the `groups`
/// key if somehow absent (real presets always carry it). The two state lists
/// (`filteredStates` / `alwaysShownStates`) are untouched — slice 3.
pub fn set_preset_groups(v: &mut Value, name: &str, groups: &[i64]) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    let presets = presets_mut(ov).ok_or(OverviewTabError::UnknownPreset { name: name.to_string() })?;
    let (_, blob) = presets
        .iter_mut()
        .find(|(k, _)| as_str(k).as_deref() == Some(name))
        .ok_or(OverviewTabError::UnknownPreset { name: name.to_string() })?;
    let fields = dict_inner_mut(blob).ok_or(OverviewTabError::UnknownPreset { name: name.to_string() })?;
    let mut sorted = groups.to_vec();
    sorted.sort_unstable();
    let list = Value::List(sorted.into_iter().map(Value::Int).collect());
    if let Some((_, g)) = fields.iter_mut().find(|(k, _)| is_b(k, b"groups")) {
        *g = list;
    } else {
        fields.push((Value::Bytes(b"groups".to_vec()), list));
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p settings-model set_groups`
Expected: PASS (3 tests).

- [ ] **Step 5: Re-export and wire the ops wrapper**

In `crates/settings-model/src/lib.rs:39`, add `set_preset_groups` to the re-export:

```rust
pub use overview_presets::{create_preset, delete_preset, rename_preset, set_preset_groups};
```

In `app/src-tauri/src/ops.rs`, add `set_preset_groups` to the `settings_model` import group (the `create_preset, delete_preset, rename_preset` line, ~line 22), then add the wrapper after `tab_set_preset` (~line 767):

```rust
pub fn preset_set_groups(state: &AppState, name: String, groups: Vec<i64>) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| set_preset_groups(v, &name, &groups))
}
```

- [ ] **Step 6: Add the Tauri command and register it**

In `app/src-tauri/src/lib.rs`, add the command after `preset_delete` (~line 198):

```rust
#[tauri::command]
fn preset_set_groups(state: tauri::State<'_, AppState>, name: String, groups: Vec<i64>) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::preset_set_groups(&state, name, groups)
}
```

Add `preset_set_groups` to the `generate_handler!` list next to `preset_create, preset_rename, preset_delete, tab_set_preset` (~line 285).

- [ ] **Step 7: Add the api.ts binding**

In `app/src/lib/api.ts`, after `tabSetPreset` (~line 287):

```ts
  presetSetGroups: (name: string, groups: number[]) =>
    invoke<OverviewColumns>("preset_set_groups", { name, groups }),
```

- [ ] **Step 8: Build the app crate to verify wiring compiles**

Run: `cargo build -p app` (or `cargo test -p settings-model` if the app crate is slow; the wiring is type-checked by `cargo build`).
Expected: PASS — command and wrapper compile.

- [ ] **Step 9: Commit**

```bash
git add crates/settings-model/src/overview_presets.rs crates/settings-model/src/lib.rs app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Add set-preset-groups authoring and wiring"
```

---

## Task 3: Frontend catalog logic (`groups.ts`)

Pure, node-testable helpers that merge the bundle with synced additions, filter by name, and toggle membership. No JSON import here (the component imports the bundle and passes it in), so these are fully unit-testable on synthetic data.

**Files:**
- Create: `app/src/lib/groups.ts`
- Create: `app/src/lib/groups.test.ts`

**Interfaces:**
- Consumes: `GroupEntry` shape `{ id, name, category_id, category_name }` (from Task 5's backend; declared locally here to avoid a cycle).
- Produces: `CatalogBundle`, `Category`, `CatGroup`, `GroupEntry` types; `mergeCatalog(bundle, additions): Category[]`; `filterCatalog(cats, query): Category[]`; `toggleGroup(groups, id, on): number[]`; `unknownGroups(cats, presetGroups): number[]`.

- [ ] **Step 1: Write failing tests**

Create `app/src/lib/groups.test.ts`:

```ts
import { test } from "node:test";
import assert from "node:assert/strict";
import { mergeCatalog, filterCatalog, toggleGroup, unknownGroups, type CatalogBundle } from "./groups.ts";

const bundle: CatalogBundle = {
  categories: [
    { id: 6, name: "Ship", groups: [{ id: 25, name: "Frigate" }, { id: 26, name: "Cruiser" }] },
    { id: 18, name: "Drone", groups: [{ id: 100, name: "Combat Drone" }] },
  ],
  all_group_ids: [25, 26, 100],
};

test("mergeCatalog slots an addition under its existing category, sorted", () => {
  const cats = mergeCatalog(bundle, [{ id: 27, name: "Battleship", category_id: 6, category_name: "Ship" }]);
  const ship = cats.find((c) => c.id === 6)!;
  assert.deepEqual(ship.groups.map((g) => g.name), ["Battleship", "Cruiser", "Frigate"]);
});

test("mergeCatalog creates a category for an addition in a new category", () => {
  const cats = mergeCatalog(bundle, [{ id: 200, name: "Fighter", category_id: 87, category_name: "Fighter" }]);
  assert.ok(cats.find((c) => c.id === 87 && c.groups.some((g) => g.id === 200)));
});

test("mergeCatalog ignores an addition already present in the bundle", () => {
  const cats = mergeCatalog(bundle, [{ id: 25, name: "Frigate", category_id: 6, category_name: "Ship" }]);
  const ship = cats.find((c) => c.id === 6)!;
  assert.equal(ship.groups.filter((g) => g.id === 25).length, 1);
});

test("filterCatalog matches group names and drops empty categories", () => {
  const cats = filterCatalog(mergeCatalog(bundle, []), "frig");
  assert.deepEqual(cats.map((c) => c.name), ["Ship"]);
  assert.deepEqual(cats[0].groups.map((g) => g.name), ["Frigate"]);
});

test("filterCatalog keeps all groups when the category name matches", () => {
  const cats = filterCatalog(mergeCatalog(bundle, []), "drone");
  assert.equal(cats.find((c) => c.name === "Drone")!.groups.length, 1);
});

test("toggleGroup adds and removes, returning a sorted array", () => {
  assert.deepEqual(toggleGroup([26, 25], 100, true), [25, 26, 100]);
  assert.deepEqual(toggleGroup([25, 26], 25, false), [26]);
});

test("unknownGroups returns preset IDs not in any category", () => {
  const cats = mergeCatalog(bundle, []);
  assert.deepEqual(unknownGroups(cats, [25, 999]), [999]);
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run (from `app/`, PowerShell): `node --test src/lib/groups.test.ts`
Expected: FAIL — `./groups.ts` does not exist.

- [ ] **Step 3: Implement `groups.ts`**

Create `app/src/lib/groups.ts`:

```ts
// Pure helpers for the overview preset-contents catalog: merge the bundled static
// group tree with ESI-synced additions, filter by name, and toggle membership.
// No Svelte/Tauri/JSON-import deps, so this is node --test-able. The component
// imports the bundle JSON and the backend additions and passes them in.

export interface CatGroup { id: number; name: string; }
export interface Category { id: number; name: string; groups: CatGroup[]; }
export interface CatalogBundle { categories: Category[]; all_group_ids: number[]; }
export interface GroupEntry { id: number; name: string; category_id: number; category_name: string; }

// Bundle ∪ additions as a category tree: each addition slotted under its category
// (creating the category if new), skipping any group id the bundle already lists.
// Categories are sorted by name; groups within a category by name.
export function mergeCatalog(bundle: CatalogBundle, additions: GroupEntry[]): Category[] {
  const cats = new Map<number, Category>();
  for (const c of bundle.categories) {
    cats.set(c.id, { id: c.id, name: c.name, groups: [...c.groups] });
  }
  const known = new Set(bundle.all_group_ids);
  for (const a of additions) {
    if (known.has(a.id)) continue;
    let cat = cats.get(a.category_id);
    if (!cat) {
      cat = { id: a.category_id, name: a.category_name, groups: [] };
      cats.set(a.category_id, cat);
    }
    if (!cat.groups.some((g) => g.id === a.id)) cat.groups.push({ id: a.id, name: a.name });
  }
  const out = [...cats.values()];
  for (const c of out) c.groups.sort((x, y) => x.name.localeCompare(y.name));
  out.sort((x, y) => x.name.localeCompare(y.name));
  return out;
}

// The tree narrowed to a case-insensitive name query. A category whose own name
// matches keeps all its groups; otherwise only its matching groups are kept.
// Categories left with no groups are dropped. Empty query returns the tree as-is.
export function filterCatalog(cats: Category[], query: string): Category[] {
  const q = query.trim().toLowerCase();
  if (!q) return cats;
  const out: Category[] = [];
  for (const c of cats) {
    if (c.name.toLowerCase().includes(q)) { out.push(c); continue; }
    const groups = c.groups.filter((g) => g.name.toLowerCase().includes(q));
    if (groups.length) out.push({ ...c, groups });
  }
  return out;
}

// Add or remove a group id from a membership list, returning a new sorted array.
export function toggleGroup(groups: number[], id: number, on: boolean): number[] {
  const set = new Set(groups);
  if (on) set.add(id); else set.delete(id);
  return [...set].sort((a, b) => a - b);
}

// Preset group ids not present anywhere in the catalog (shown as `#id`, removable).
export function unknownGroups(cats: Category[], presetGroups: number[]): number[] {
  const known = new Set<number>();
  for (const c of cats) for (const g of c.groups) known.add(g.id);
  return presetGroups.filter((id) => !known.has(id));
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run (from `app/`, PowerShell): `node --test src/lib/groups.test.ts`
Expected: PASS (7 tests).

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/groups.ts app/src/lib/groups.test.ts
git commit -m "Add the overview group-catalog merge and filter helpers"
```

---

## Task 4: Bundle generator + committed catalog snapshot

Write the python generator (mirroring `tools/gen-default-preset-names.py`), run it against ESI, and commit the resulting `overview-groups.json`.

**Files:**
- Create: `tools/gen-overview-groups.py`
- Create: `app/src/lib/data/overview-groups.json` (generated, committed)

**Interfaces:**
- Produces: `app/src/lib/data/overview-groups.json` in the Global-Constraints bundle shape.

- [ ] **Step 1: Write the generator**

Create `tools/gen-overview-groups.py`:

```python
#!/usr/bin/env python3
"""Regenerate app/src/lib/data/overview-groups.json.

The overview filter presets store entity *group IDs*. This dev tool builds the
bundled catalog the app uses to name and browse those groups: the overview-
relevant category -> group tree (with names) plus a flat list of ALL current
group IDs (the sync-diff baseline; the app resolves anything newer from ESI).

Relevance is a hardcoded allowlist of "in-space" category IDs — the categories
whose items appear on the overview. Tune RELEVANT_CATEGORIES if CCP adds a new
in-space category.

Not shipped to users. Rerun and re-commit on an app release when CCP adds groups.

Usage:  python tools/gen-overview-groups.py
Requires Python 3 (stdlib only) and network access to ESI.
"""
import json
import os
import sys
import urllib.request

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(REPO, "app", "src", "lib", "data", "overview-groups.json")
BASE = "https://esi.evetech.net/latest"

# In-space categories shown on the overview. IDs are stable across EVE versions.
RELEVANT_CATEGORIES = {
    2: "Celestial",
    6: "Ship",
    18: "Drone",
    22: "Deployable",
    23: "Starbase",
    40: "Sovereignty Structure",
    46: "Orbital",
    65: "Structure",
    87: "Fighter",
    11: "Entity",
}


def get(url):
    req = urllib.request.Request(url, headers={"User-Agent": "eve-settings-editor-gen"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.load(resp)


def all_group_ids():
    """Every current group id, following ESI's X-Pages pagination."""
    ids, page = [], 1
    while True:
        req = urllib.request.Request(f"{BASE}/universe/groups/?page={page}",
                                     headers={"User-Agent": "eve-settings-editor-gen"})
        with urllib.request.urlopen(req, timeout=30) as resp:
            ids.extend(json.load(resp))
            pages = int(resp.headers.get("X-Pages", "1"))
        if page >= pages:
            return ids
        page += 1


def main():
    categories = []
    for cat_id in sorted(RELEVANT_CATEGORIES):
        cat = get(f"{BASE}/universe/categories/{cat_id}/")
        groups = []
        for gid in cat.get("groups", []):
            g = get(f"{BASE}/universe/groups/{gid}/")
            if not g.get("published", False):
                continue
            groups.append({"id": gid, "name": g["name"]})
        groups.sort(key=lambda x: x["name"].lower())
        categories.append({"id": cat_id, "name": cat["name"], "groups": groups})

    ids = sorted(set(all_group_ids()))

    # Refuse to clobber the committed snapshot with an empty/degenerate result.
    if not ids or not any(c["groups"] for c in categories):
        sys.exit("ESI returned no groups — refusing to overwrite the snapshot")

    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w", encoding="utf-8", newline="\n") as fh:
        json.dump({"categories": categories, "all_group_ids": ids}, fh, ensure_ascii=False, indent=2)
        fh.write("\n")

    total = sum(len(c["groups"]) for c in categories)
    print(f"wrote {len(categories)} categories, {total} groups, {len(ids)} total ids -> {OUT}")


if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Run the generator**

Run: `python tools/gen-overview-groups.py`
Expected: prints a category/group/id count and writes `app/src/lib/data/overview-groups.json`. (Requires network. If ESI is unreachable, note it and retry — do not hand-write the snapshot.)

- [ ] **Step 3: Sanity-check the generated JSON shape**

Run from `app/` (PowerShell) — a pure JSON structural check, no TS involved:

```
node -e "const b=require('./src/lib/data/overview-groups.json'); if(!Array.isArray(b.categories)||!b.categories.length||!Array.isArray(b.all_group_ids)||!b.all_group_ids.length||!b.categories.some(c=>Array.isArray(c.groups)&&c.groups.length)){console.error('bad bundle');process.exit(1)} console.log('ok',b.categories.length,'cats',b.all_group_ids.length,'ids')"
```

Expected: `ok N cats M ids` — confirms the committed bundle matches the Global-Constraints shape (non-empty categories, at least one with groups, non-empty id list).

- [ ] **Step 4: Commit**

```bash
git add tools/gen-overview-groups.py app/src/lib/data/overview-groups.json
git commit -m "Add the overview group-catalog generator and bundle"
```

---

## Task 5: Backend ESI sync (`groups.rs`) + command

Add the append-only, server_version-gated catalog sync, mirroring `names.rs`: cache-forever JSON, injectable fetcher (unit-testable, no network), silent fallback.

**Files:**
- Create: `app/src-tauri/src/groups.rs`
- Modify: `app/src-tauri/src/lib.rs` (`mod groups;` + command + handler list)
- Modify: `app/src/lib/api.ts` (`GroupEntry` type + `syncGroupCatalog` binding)

**Interfaces:**
- Produces (Rust): `pub struct GroupEntry { id: i64, name: String, category_id: i64, category_name: String }` (Serialize/Deserialize/Clone); `pub fn sync_blocking(dir: &Path, known_ids: &[i64], relevant_categories: &[i64]) -> Vec<GroupEntry>`.
- Produces (TS): `api.syncGroupCatalog(knownIds: number[], relevantCategories: number[]) => Promise<GroupEntry[]>` (command `sync_group_catalog`).

- [ ] **Step 1: Write failing unit tests**

Create `app/src-tauri/src/groups.rs` with ONLY the test module first (so it fails to compile against missing items), then fill in the implementation in Step 3. The tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("groups-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn entry(id: i64, cat: i64) -> GroupEntry {
        GroupEntry { id, name: format!("G{id}"), category_id: cat, category_name: format!("C{cat}") }
    }

    #[test]
    fn sync_skips_fetch_when_version_unchanged() {
        let dir = temp_dir("gate");
        // First sync at version "v1" resolves one relevant addition.
        sync_with(&dir, &[1], &[6], Some("v1".into()), |_known| Ok(vec![entry(2, 6)]));
        // Second sync at the SAME version must not call the fetcher.
        let out = sync_with(&dir, &[1], &[6], Some("v1".into()), |_known| panic!("must not fetch"));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, 2);
    }

    #[test]
    fn sync_resolves_delta_on_version_change_and_persists() {
        let dir = temp_dir("delta");
        let out = sync_with(&dir, &[1], &[6], Some("v1".into()), |known| {
            assert!(known.contains(&1), "the bundle's known ids seed the diff");
            Ok(vec![entry(2, 6)])
        });
        assert_eq!(out.iter().map(|e| e.id).collect::<Vec<_>>(), vec![2]);
        // A new version merges more; the cache persisted across the call.
        let out2 = sync_with(&dir, &[1], &[6], Some("v2".into()), |known| {
            assert!(known.contains(&2), "already-cached ids are known and not re-resolved");
            Ok(vec![entry(3, 6)])
        });
        let ids: Vec<i64> = out2.iter().map(|e| e.id).collect();
        assert!(ids.contains(&2) && ids.contains(&3));
    }

    #[test]
    fn sync_filters_out_irrelevant_categories() {
        let dir = temp_dir("relevance");
        let out = sync_with(&dir, &[1], &[6], Some("v1".into()), |_known| {
            Ok(vec![entry(2, 6), entry(9, 99)]) // category 99 is not relevant
        });
        assert_eq!(out.iter().map(|e| e.id).collect::<Vec<_>>(), vec![2]);
    }

    #[test]
    fn sync_falls_back_to_cache_on_fetch_error() {
        let dir = temp_dir("err");
        sync_with(&dir, &[1], &[6], Some("v1".into()), |_known| Ok(vec![entry(2, 6)]));
        let out = sync_with(&dir, &[1], &[6], Some("v2".into()), |_known| Err(FetchError("offline".into())));
        // The prior addition survives; the failed version is NOT recorded.
        assert_eq!(out.iter().map(|e| e.id).collect::<Vec<_>>(), vec![2]);
        // A later successful sync at v2 still runs (version wasn't advanced on failure).
        let out2 = sync_with(&dir, &[1], &[6], Some("v2".into()), |_known| Ok(vec![entry(3, 6)]));
        assert_eq!(out2.len(), 2);
    }

    #[test]
    fn sync_no_version_uses_cache_without_fetching() {
        let dir = temp_dir("noversion");
        sync_with(&dir, &[1], &[6], Some("v1".into()), |_known| Ok(vec![entry(2, 6)]));
        // A None server_version (e.g. /status unreachable) returns the cache, no fetch.
        let out = sync_with(&dir, &[1], &[6], None, |_known| panic!("must not fetch"));
        assert_eq!(out.len(), 1);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p app groups::`
Expected: FAIL — `GroupEntry`, `sync_with`, `FetchError` undefined.

- [ ] **Step 3: Implement the module**

Put this ABOVE the test module in `app/src-tauri/src/groups.rs`:

```rust
//! ESI overview-group catalog sync: an append-only, server_version-gated store
//! that resolves the group IDs CCP adds AFTER the bundled snapshot was cut. The
//! bundle (frontend) is the base catalog; this only adds names for newer group
//! IDs. Group id -> name/category is immutable, so the cache is never
//! invalidated — only extended. Mirrors `names.rs`: cache-forever JSON, silent
//! failure (fall back to the cache), an injectable fetcher for tests. Only
//! `esi_fetch_delta` / `fetch_server_version` touch the network.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupEntry {
    pub id: i64,
    pub name: String,
    pub category_id: i64,
    pub category_name: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct GroupCache {
    /// The `/status` server_version the cache was last synced at (None = never).
    version: Option<String>,
    /// Resolved additions beyond the bundle, by group id.
    groups: HashMap<i64, GroupEntry>,
}

#[derive(Debug)]
pub struct FetchError(pub String);

fn cache_path(dir: &Path) -> PathBuf {
    dir.join("groups-cache.json")
}

fn load_cache(dir: &Path) -> GroupCache {
    match fs::read(cache_path(dir)) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => GroupCache::default(),
    }
}

fn save_cache(dir: &Path, cache: &GroupCache) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let bytes = serde_json::to_vec_pretty(cache)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::write(cache_path(dir), bytes)
}

/// Core sync logic (injectable fetcher; no network). If `server_version` is Some
/// and differs from the cached one, resolve the delta (the fetcher gets the full
/// known-id set — bundle ∪ cache — so it only resolves genuinely-new ids), keep
/// the relevant-category ones, merge, and persist the new version. On fetch error
/// or `None` version, leave the cache untouched. Always returns the cache values.
fn sync_with<F>(
    dir: &Path,
    known_ids: &[i64],
    relevant_categories: &[i64],
    server_version: Option<String>,
    fetch: F,
) -> Vec<GroupEntry>
where
    F: FnOnce(&HashSet<i64>) -> Result<Vec<GroupEntry>, FetchError>,
{
    let mut cache = load_cache(dir);
    let changed = matches!(&server_version, Some(v) if cache.version.as_deref() != Some(v));
    if changed {
        let mut known: HashSet<i64> = known_ids.iter().copied().collect();
        known.extend(cache.groups.keys().copied());
        match fetch(&known) {
            Ok(found) => {
                let relevant: HashSet<i64> = relevant_categories.iter().copied().collect();
                for e in found {
                    if relevant.contains(&e.category_id) {
                        cache.groups.insert(e.id, e);
                    }
                }
                cache.version = server_version;
                let _ = save_cache(dir, &cache);
            }
            Err(e) => eprintln!("group catalog sync: fetch failed ({})", e.0),
        }
    }
    let mut out: Vec<GroupEntry> = cache.groups.into_values().collect();
    out.sort_by_key(|e| e.id);
    out
}

const ESI: &str = "https://esi.evetech.net/latest";

fn http_get(client: &reqwest::blocking::Client, url: &str) -> Result<reqwest::blocking::Response, FetchError> {
    client
        .get(url)
        .header(reqwest::header::USER_AGENT, "eve-settings-editor")
        .send()
        .map_err(|e| FetchError(e.to_string()))
        .and_then(|r| if r.status().is_success() { Ok(r) } else { Err(FetchError(format!("ESI status {}", r.status()))) })
}

/// The network delta resolve: enumerate current group ids, diff against `known`,
/// resolve each new id (+ its category name), and return the raw entries (the
/// caller applies the relevance filter). Untested (network), like names::esi_fetch.
fn esi_fetch_delta(known: &HashSet<i64>) -> Result<Vec<GroupEntry>, FetchError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| FetchError(e.to_string()))?;

    // Enumerate all current group ids across X-Pages.
    let mut all: Vec<i64> = Vec::new();
    let mut page = 1;
    loop {
        let resp = http_get(&client, &format!("{ESI}/universe/groups/?page={page}"))?;
        let pages: u32 = resp.headers().get("X-Pages")
            .and_then(|v| v.to_str().ok()).and_then(|s| s.parse().ok()).unwrap_or(1);
        let ids: Vec<i64> = resp.json().map_err(|e| FetchError(e.to_string()))?;
        all.extend(ids);
        if page >= pages { break; }
        page += 1;
    }

    let mut cat_names: HashMap<i64, String> = HashMap::new();
    let mut out = Vec::new();
    for id in all.into_iter().filter(|id| !known.contains(id)) {
        let g: serde_json::Value = http_get(&client, &format!("{ESI}/universe/groups/{id}/"))?
            .json().map_err(|e| FetchError(e.to_string()))?;
        if !g.get("published").and_then(|p| p.as_bool()).unwrap_or(false) { continue; }
        let name = g.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
        let category_id = g.get("category_id").and_then(|c| c.as_i64()).unwrap_or(0);
        let category_name = match cat_names.get(&category_id) {
            Some(n) => n.clone(),
            None => {
                let c: serde_json::Value = http_get(&client, &format!("{ESI}/universe/categories/{category_id}/"))?
                    .json().map_err(|e| FetchError(e.to_string()))?;
                let n = c.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                cat_names.insert(category_id, n.clone());
                n
            }
        };
        out.push(GroupEntry { id, name, category_id, category_name });
    }
    Ok(out)
}

fn fetch_server_version() -> Option<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .ok()?;
    let resp = http_get(&client, &format!("{ESI}/status/")).ok()?;
    let status: serde_json::Value = resp.json().ok()?;
    status.get("server_version").and_then(|v| v.as_str()).map(|s| s.to_string())
}

/// Production wiring: gate on the live server_version, resolve via ESI. Blocking.
pub fn sync_blocking(dir: &Path, known_ids: &[i64], relevant_categories: &[i64]) -> Vec<GroupEntry> {
    sync_with(dir, known_ids, relevant_categories, fetch_server_version(), esi_fetch_delta)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p app groups::`
Expected: PASS (5 tests).

- [ ] **Step 5: Register the module and add the command**

In `app/src-tauri/src/lib.rs`, add `mod groups;` next to `mod names;` (line 2). Add the command near the other async ESI commands (after `refresh_character_names`, ~line 100):

```rust
#[tauri::command]
async fn sync_group_catalog(
    app: tauri::AppHandle,
    known_ids: Vec<i64>,
    relevant_categories: Vec<i64>,
) -> Vec<groups::GroupEntry> {
    let dir = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir());
    tauri::async_runtime::spawn_blocking(move || groups::sync_blocking(&dir, &known_ids, &relevant_categories))
        .await
        .unwrap_or_default()
}
```

Add `sync_group_catalog` to the `generate_handler!` list next to `resolve_character_names, refresh_character_names` (~line 279).

- [ ] **Step 6: Add the api.ts type and binding**

In `app/src/lib/api.ts`, add the type (near `OverviewColumns`):

```ts
export interface GroupEntry {
  id: number;
  name: string;
  category_id: number;
  category_name: string;
}
```

And the binding (near `resolveCharacterNames`, ~line 247):

```ts
  syncGroupCatalog: (knownIds: number[], relevantCategories: number[]) =>
    invoke<GroupEntry[]>("sync_group_catalog", { knownIds, relevantCategories }),
```

- [ ] **Step 7: Build to verify wiring compiles**

Run: `cargo build -p app` and (from `app/`, PowerShell) `npm run check`.
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add app/src-tauri/src/groups.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Add the ESI overview group-catalog sync"
```

---

## Task 6: The contents editor UI (`OverviewView.svelte`)

Render the filterable category→group checklist for the selected tab's preset, wire toggles to `presetSetGroups`, and load the catalog (bundle ∪ synced additions) once on mount.

**Files:**
- Modify: `app/src/lib/OverviewView.svelte` (imports, catalog load, contents section markup + styles)

**Interfaces:**
- Consumes: `api.syncGroupCatalog`, `api.presetSetGroups`, `mergeCatalog`/`filterCatalog`/`toggleGroup`/`unknownGroups` + `Category`/`CatalogBundle` types (Task 3), `overview-groups.json` (Task 4), `data.presets: Preset[]` (Task 1), `presetIsReal`/`tab` (existing).

> **Why the loader lives in the component, not `groups.ts`:** `groups.ts` is imported by `groups.test.ts` under `node --test`. Adding a value `import { api }` (or the JSON import) to `groups.ts` would transitively pull `@tauri-apps/api/core` into the test process and break it. So the bundle JSON import + the `syncGroupCatalog` call stay in the Svelte component (which already imports JSON and `api`); `groups.ts` stays pure and node-testable. No session memo is needed — the backend's `server_version` gate already makes a repeat sync a cheap no-op.

- [ ] **Step 1: Add the imports to `OverviewView.svelte`**

Add to the `<script>` imports (top of the file), alongside the existing `default-preset-names.json` import:

```ts
  import overviewGroups from "./data/overview-groups.json";
  import { mergeCatalog, filterCatalog, toggleGroup, unknownGroups, type Category, type CatalogBundle } from "./groups";
```

- [ ] **Step 2: Add the catalog load + contents state**

Add near the other preset `$derived`/`$state` in the `<script>`:

```ts
  let catalog = $state<Category[]>([]);
  // Load once on mount: the backend server_version-gates the ESI sync, so this is
  // cheap on repeat; fall back to the bundle alone if the sync fails.
  $effect(() => {
    const b = overviewGroups as CatalogBundle;
    api
      .syncGroupCatalog(b.all_group_ids, b.categories.map((c) => c.id))
      .then((additions) => (catalog = mergeCatalog(b, additions)))
      .catch(() => (catalog = mergeCatalog(b, [])));
  });

  let groupFilter = $state("");
  const presetGroups = $derived(
    (data?.presets.find((p) => p.name === tab?.preset)?.groups) ?? []
  );
  const presetGroupSet = $derived(new Set(presetGroups));
  const visibleCategories = $derived(filterCatalog(catalog, groupFilter));
  const unknownIds = $derived(unknownGroups(catalog, presetGroups));

  async function setPresetGroup(id: number, on: boolean) {
    if (!tab) return;
    try { data = await api.presetSetGroups(tab.preset, toggleGroup(presetGroups, id, on)); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
```

- [ ] **Step 3: Add the contents section markup**

In the template, after the preset picker / management controls block (around the `.preset-actions` area, ~line 300), add:

```svelte
{#if presetIsReal}
  <div class="preset-contents">
    <div class="contents-head">
      <span class="contents-title">Shows: {labelFor(tab.preset)}</span>
      <input class="group-filter" type="text" placeholder="Filter groups…" bind:value={groupFilter} />
    </div>

    {#if unknownIds.length}
      <div class="unknown-groups">
        Unrecognized groups (not in the catalog):
        {#each unknownIds as id}
          <label><input type="checkbox" checked onchange={() => setPresetGroup(id, false)} /> #{id}</label>
        {/each}
      </div>
    {/if}

    {#each visibleCategories as cat (cat.id)}
      <details class="group-cat" open={!!groupFilter.trim()}>
        <summary>{cat.name}</summary>
        <div class="group-grid">
          {#each cat.groups as g (g.id)}
            <label class="group-item">
              <input type="checkbox" checked={presetGroupSet.has(g.id)}
                     onchange={(e) => setPresetGroup(g.id, (e.currentTarget as HTMLInputElement).checked)} />
              {g.name}
            </label>
          {/each}
        </div>
      </details>
    {/each}
  </div>
{/if}
```

- [ ] **Step 4: Add dark-safe styles**

In the `<style>` block (near `.preset-actions`, ~line 396), add (WebView2 renders native controls light by default — give explicit dark values, per the standing gotcha):

```css
  .preset-contents { margin-top: 0.6rem; display: flex; flex-direction: column; gap: 0.35rem; }
  .contents-head { display: flex; gap: 0.6rem; align-items: center; flex-wrap: wrap; }
  .contents-title { font-weight: 600; }
  .group-filter { background: #1b1f27; color: #e6e6e6; border: 1px solid #39414f; border-radius: 4px; padding: 0.25rem 0.4rem; }
  .group-cat > summary { cursor: pointer; padding: 0.2rem 0; }
  .group-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(11rem, 1fr)); gap: 0.15rem 0.8rem; padding: 0.2rem 0 0.4rem 1rem; }
  .group-item { display: flex; gap: 0.35rem; align-items: center; }
  .group-item input, .unknown-groups input { accent-color: #4a90d9; }
  .unknown-groups { display: flex; gap: 0.6rem; flex-wrap: wrap; align-items: center; color: #c9a227; }
```

(Match the file's existing color palette if it differs — reuse the same hex values already used by `.preset-actions`/inputs nearby rather than introducing new ones.)

- [ ] **Step 5: Type-check and build**

Run (from `app/`, PowerShell): `npm run check` then `npm run build`.
Expected: PASS — no type or Svelte errors.

- [ ] **Step 6: Run the full frontend test suite**

Run (from `app/`, PowerShell): `npm test`
Expected: PASS — `groups.test.ts` and all existing suites (`overview.test.ts`, etc.) green.

- [ ] **Step 7: Live smoke (manual)**

Launch the app (`npm run tauri dev` from `app/`, PowerShell), open a paired character with a real account file, go to Overview, pick a tab with a real preset, and in the contents section: check/uncheck a couple of groups, confirm the filter narrows the tree, Save, then reopen the file and confirm the groups persisted. (Full acceptance — confirming EVE itself reflects the change — is the milestone's live-smoke gate.)

- [ ] **Step 8: Commit**

```bash
git add app/src/lib/OverviewView.svelte
git commit -m "Add the preset group-contents editor to the overview view"
```

---

## Self-Review notes (for the implementer)

- **Spec coverage:** §3 → Task 1; §4/§5 → Task 2; §6.1/§7 → Task 4; §6.2 (`groups.rs` sync) → Task 5; §6.3 merge + §8 UI → Tasks 3 & 6. States (§1/§11 non-goal) are never read/written — verify no task touches `filteredStates`/`alwaysShownStates`.
- **Type consistency:** `Preset { name, groups }` (Task 1) is used by Task 6's `presetGroups` derived. `GroupEntry { id, name, category_id, category_name }` is identical in `groups.rs` (Task 5), `groups.ts` (Task 3), and `api.ts` (Task 5). `syncGroupCatalog(knownIds, relevantCategories)` params match between the command (Task 5) and `loadCatalog` (Task 6).
- **Ceiling:** a group whose category is not in `RELEVANT_CATEGORIES` (Task 4) / not in the bundle's category set (Task 5 filter) shows as `#id` via `unknownGroups` (Task 6) — the documented §6.4 fallback, correct behavior, not a bug.
