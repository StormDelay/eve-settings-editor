# Default overview-profile support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make overview preset management + the 2b contents editor work on *clean* accounts, whose `overviewProfilePresets` is empty/absent and whose tabs reference EVE's built-in default profiles (which live in static data, not the file).

**Architecture:** Bundle the built-in default profiles' definitions (extracted from the corpus). List all defaults in the preset dropdown (grouped), render a default's contents from the bundle, and **fork** a default into a new user profile on edit/duplicate (the built-in stays untouched). Rename/Delete are disabled on built-ins.

**Tech Stack:** Rust (`settings-model` + `src-tauri`), TypeScript/Svelte 5, python stdlib (bundle generator). Design: `docs/superpowers/specs/2026-07-21-default-overview-profiles-design.md`. Builds on slice 2b (contents editor, group catalog, `overview_dump`).

## Global Constraints

- Sentence-case commit subjects, **NO attribution trailers**.
- Zero new dependencies.
- **User-file only**; all edits go through the existing `edit_user_tabs` (inline→edit→reshare) path.
- No personal data / real preset names / character ids in code or committed data (the bundle holds only CCP built-in profiles — keys `DefaultPreset_<id>` / `default*` and CCP names, which are game reference data, not personal).
- Test commands: Rust `cargo test -p settings-model` and `cargo build -p app`; frontend from `app/` via **PowerShell** (npm not on Bash PATH): `npm test` (node --test), `npm run check`, `npm run build`. Frontend `*.test.ts` use the repo's throw-based `check`/`eq` convention (NO `node:test`/`node:assert`), importing with `.ts` extensions.
- **A default key** is `/^DefaultPreset_\d+$/` OR a legacy literal starting with `default` (case-insensitive): `defaultall`, `defaultpvp`, `defaultmining`, `defaultloot`, `defaultdrones`, `defaultwarpto`, `default`.
- **Fresh `(timestamp, dict)` containers** are minted with `Value::Long(vec![0u8; 8])` as the timestamp (matches the codebase, e.g. `overview_tabs.rs` tests).
- **Bundle shape** `app/src/lib/data/default-presets.json` (fixed across tasks):
  ```json
  { "modern": [ { "key": "DefaultPreset_639443", "name": "All",
                  "groups": [ints], "filteredStates": [ints], "alwaysShownStates": [ints] } ],
    "legacy": [ { "key": "defaultall", "name": "All", "groups": [ints],
                  "filteredStates": [ints], "alwaysShownStates": [ints] } ] }
  ```
- **Legacy name map** (used by the generator and `labelFor`): `defaultall`→"All", `defaultpvp`→"PvP", `defaultmining`→"Mining", `defaultloot`→"Loot", `defaultdrones`→"Drones", `defaultwarpto`→"Warp To", `default`→"Default".

---

## Task 1: Bundle the default profile definitions

Extract each built-in default's full blob (groups + both state lists) from the corpus and commit `default-presets.json`.

**Files:**
- Modify: `crates/settings-model/src/bin/overview_dump.rs` (add a `BLOBS` mode)
- Create: `tools/gen-default-presets.py`
- Create: `app/src/lib/data/default-presets.json` (generated, committed)

**Interfaces:**
- Produces: `app/src/lib/data/default-presets.json` in the Global-Constraints shape.

- [ ] **Step 1: Add a `BLOBS` mode to `overview_dump`**

Append to `overview_dump.rs`'s per-file loop a branch guarded by `std::env::var("BLOBS").is_ok()` that flattens Shared/Ref and dumps each default preset's raw lists as TSV to stdout (one line per default key). Add this helper and call it:

```rust
fn is_default_key(k: &str) -> bool {
    k.strip_prefix("DefaultPreset_").map_or(false, |n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        || k.to_ascii_lowercase().starts_with("default")
}

fn ints(v: &blue_marshal::Value) -> Vec<i64> {
    match v { blue_marshal::Value::List(l) =>
        l.iter().filter_map(|e| if let blue_marshal::Value::Int(n) = e { Some(*n) } else { None }).collect(),
        _ => Vec::new() }
}

fn dump_default_blobs(v: &blue_marshal::Value) {
    use blue_marshal::Value;
    let flat = blue_marshal::inline(v);
    let Value::Dict(root) = &flat else { return };
    let Some((_, ov)) = root.iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == b"overview")) else { return };
    let Value::Dict(ovd) = ov else { return };
    let Some((_, p)) = ovd.iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == b"overviewProfilePresets")) else { return };
    // (timestamp, dict) or bare dict
    let inner = match p { Value::Tuple(items) => items.iter().find_map(|e| if let Value::Dict(d) = e { Some(d) } else { None }),
                          Value::Dict(d) => Some(d), _ => None };
    let Some(pd) = inner else { return };
    for (k, blob) in pd {
        let Value::Bytes(kb) = k else { continue };
        let key = String::from_utf8_lossy(kb).into_owned();
        if !is_default_key(&key) { continue; }
        let Value::Dict(fields) = blob else { continue };
        let field = |name: &[u8]| fields.iter().find(|(fk, _)| matches!(fk, Value::Bytes(b) if b.as_slice() == name)).map(|(_, fv)| ints(fv)).unwrap_or_default();
        let g = field(b"groups"); let fs = field(b"filteredStates"); let a = field(b"alwaysShownStates");
        let csv = |xs: &[i64]| xs.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(",");
        println!("BLOB\t{}\t{}\t{}\t{}", key, csv(&g), csv(&fs), csv(&a));
    }
}
```

In the loop, after decoding `v`: `if std::env::var("BLOBS").is_ok() { dump_default_blobs(&v); continue; }` (skip the union/best work in BLOBS mode).

- [ ] **Step 2: Build and dump the blobs**

Run: `cargo build -q -p settings-model --bin overview_dump`
Then: `find testdata -name 'core_user*.dat' | BLOBS=1 target/debug/overview_dump.exe > "$CLAUDE_JOB_DIR/tmp/blobs.tsv"` (the bin reads paths from stdin when no args). Expected: many `BLOB\t<key>\t...` lines covering `DefaultPreset_63943x..63946x` and `defaultall`/`defaultpvp`/etc.

- [ ] **Step 3: Write the generator**

Create `tools/gen-default-presets.py`:

```python
#!/usr/bin/env python3
"""Regenerate app/src/lib/data/default-presets.json.

EVE's built-in overview profiles live in the client's static data, not the
settings file, so a clean account stores none of them. This tool bundles their
definitions (groups + state lists) so the app can list, show, and fork them.

The blobs come from the `overview_dump` bin in BLOBS mode over the local corpus
(TSV: BLOB<TAB>key<TAB>groups_csv<TAB>filtered_csv<TAB>alwaysshown_csv); the
richest copy (most groups) per key wins. Names: DefaultPreset_<id> via
default-preset-names.json; legacy default* via LEGACY_NAMES.

Usage:  find testdata -name 'core_user*.dat' | BLOBS=1 \\
            target/debug/overview_dump.exe | python tools/gen-default-presets.py
Requires Python 3 (stdlib only) and the built overview_dump bin.
"""
import json, os, re, sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(REPO, "app", "src", "lib", "data", "default-presets.json")
NAMES = json.load(open(os.path.join(REPO, "app", "src", "lib", "data", "default-preset-names.json"), encoding="utf-8"))
LEGACY_NAMES = {"defaultall": "All", "defaultpvp": "PvP", "defaultmining": "Mining",
                "defaultloot": "Loot", "defaultdrones": "Drones", "defaultwarpto": "Warp To",
                "default": "Default"}

def ints(csv):
    return [int(x) for x in csv.split(",") if x]

def main():
    best = {}  # key -> (groups, filtered, always)
    for line in sys.stdin:
        parts = line.rstrip("\n").split("\t")
        if len(parts) != 5 or parts[0] != "BLOB":
            continue
        _, key, g, f, a = parts
        g, f, a = ints(g), ints(f), ints(a)
        if key not in best or len(g) > len(best[key][0]):
            best[key] = (g, f, a)

    modern, legacy = [], []
    for key, (g, f, a) in sorted(best.items()):
        m = re.match(r"^DefaultPreset_(\d+)$", key)
        if m:
            name = NAMES.get(m.group(1), key)
            modern.append({"key": key, "name": name, "groups": sorted(g),
                           "filteredStates": f, "alwaysShownStates": a})
        else:
            name = LEGACY_NAMES.get(key, key)
            legacy.append({"key": key, "name": name, "groups": sorted(g),
                           "filteredStates": f, "alwaysShownStates": a})

    if not modern and not legacy:
        sys.exit("no default profiles found on stdin — refusing to overwrite the snapshot")

    with open(OUT, "w", encoding="utf-8", newline="\n") as fh:
        json.dump({"modern": modern, "legacy": legacy}, fh, ensure_ascii=False, indent=2)
        fh.write("\n")
    print(f"wrote {len(modern)} modern + {len(legacy)} legacy default profiles -> {OUT}")

if __name__ == "__main__":
    main()
```

- [ ] **Step 4: Generate and sanity-check**

Run (Bash): `find testdata -name 'core_user*.dat' | BLOBS=1 target/debug/overview_dump.exe | python tools/gen-default-presets.py`
Expected: `wrote N modern + M legacy default profiles`. Then verify shape (from `app/`, PowerShell):
```
node -e "const d=require('./src/lib/data/default-presets.json'); const ok=Array.isArray(d.modern)&&d.modern.length&&d.modern.every(p=>p.key&&p.name&&Array.isArray(p.groups)); if(!ok){console.error('bad');process.exit(1)} console.log('ok',d.modern.length,'modern',d.legacy.length,'legacy')"
```
Expected: `ok <N> modern <M> legacy`.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/bin/overview_dump.rs tools/gen-default-presets.py app/src/lib/data/default-presets.json
git commit -m "Bundle EVE's built-in overview profile definitions"
```

---

## Task 2: Backend fork command (`create_preset_from_lists` + `preset_fork`)

Add authoring to create a preset from explicit lists (materializing the `overviewProfilePresets` container if absent), and an ops command that forks + retargets a tab in one edit.

**Files:**
- Modify: `crates/settings-model/src/overview_presets.rs` (new fn + `presets_mut_or_create` + tests)
- Modify: `crates/settings-model/src/lib.rs:39` (re-export)
- Modify: `app/src-tauri/src/ops.rs` (import + `preset_fork` wrapper)
- Modify: `app/src-tauri/src/lib.rs` (command + handler registration)
- Modify: `app/src/lib/api.ts` (binding)

**Interfaces:**
- Consumes: `inline_all`, `overview_mut`, `dict_inner_mut`, `is_b`, `as_str` (existing `overview_presets`/`overview_tabs` helpers); `set_tab_preset` (existing, in `overview_tabs`).
- Produces (Rust): `pub fn create_preset_from_lists(v: &mut Value, name: &str, groups: &[i64], filtered_states: &[i64], always_shown_states: &[i64]) -> Result<(), OverviewTabError>`; `ops::preset_fork(state, tab_idx: i64, name: String, groups: Vec<i64>, filtered_states: Vec<i64>, always_shown_states: Vec<i64>) -> Result<OverviewColumns, ErrDto>`.
- Produces (TS): `api.presetFork(tabIdx, name, groups, filteredStates, alwaysShownStates) => Promise<OverviewColumns>` (command `preset_fork`).

- [ ] **Step 1: Write failing unit tests**

In `crates/settings-model/src/overview_presets.rs` `mod tests`, add (reuses `b`, `is_b`, `as_str`, and the `preset_groups` reader from the 2b tests):

```rust
    fn overview_container_absent_presets() -> Value {
        // A clean account: overview has tabs but NO overviewProfilePresets.
        let tab0 = Value::Dict(vec![(b("overview"), b("DefaultPreset_1"))]);
        let overview = Value::Dict(vec![
            (b("tabsettings_new"), Value::Tuple(vec![Value::Int(1), Value::Dict(vec![(Value::Int(0), tab0)])])),
        ]);
        Value::Dict(vec![(b("overview"), overview)])
    }

    #[test]
    fn create_from_lists_materializes_absent_container() {
        let mut v = overview_container_absent_presets();
        create_preset_from_lists(&mut v, "All copy", &[30, 10], &[5], &[]).unwrap();
        assert_eq!(preset_groups(&v, "All copy"), vec![10, 30]); // sorted
    }

    #[test]
    fn create_from_lists_errors_on_existing_name() {
        let mut v = user_with_presets(); // has "alpha"
        assert!(matches!(
            create_preset_from_lists(&mut v, "alpha", &[1], &[], &[]),
            Err(OverviewTabError::PresetExists { .. })
        ));
    }

    #[test]
    fn create_from_lists_stores_state_lists() {
        let mut v = overview_container_absent_presets();
        create_preset_from_lists(&mut v, "PvP copy", &[1], &[7, 8], &[9]).unwrap();
        // read filteredStates back
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, p) = ovd.iter().find(|(k, _)| is_b(k, b"overviewProfilePresets")).unwrap();
        let inner = match p { Value::Tuple(it) => it.iter().find_map(|e| if let Value::Dict(d)=e {Some(d)} else {None}).unwrap(), Value::Dict(d)=>d, _=>panic!() };
        let (_, blob) = inner.iter().find(|(k, _)| as_str(k).as_deref() == Some("PvP copy")).unwrap();
        let Value::Dict(bf) = blob else { panic!() };
        let (_, fs) = bf.iter().find(|(k, _)| is_b(k, b"filteredStates")).unwrap();
        assert_eq!(fs, &Value::List(vec![Value::Int(7), Value::Int(8)]));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p settings-model create_from_lists`
Expected: FAIL — `create_preset_from_lists` undefined.

- [ ] **Step 3: Implement `presets_mut_or_create` + `create_preset_from_lists`**

In `overview_presets.rs`, after `presets_mut`:

```rust
/// Mutable inner dict of `overviewProfilePresets`, MINTING an empty
/// `(timestamp, dict)` container if the key is absent (a clean account stores no
/// presets at all). The timestamp is a zero `Long`, matching how the codebase
/// mints fresh `(ts, dict)` containers; EVE re-timestamps on its next save.
pub(crate) fn presets_mut_or_create(ov: &mut Entries) -> &mut Entries {
    if !ov.iter().any(|(k, _)| is_b(k, b"overviewProfilePresets")) {
        ov.push((
            Value::Bytes(b"overviewProfilePresets".to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Dict(Vec::new())]),
        ));
    }
    let (_, v) = ov.iter_mut().find(|(k, _)| is_b(k, b"overviewProfilePresets")).unwrap();
    dict_inner_mut(v).expect("just-created or existing (ts, dict)")
}

/// Create a NEW user preset `name` from explicit lists — used to fork a built-in
/// default that may not be stored in the file (so `create_preset`, which clones
/// an existing key, cannot be used). Groups are sorted ascending; the state lists
/// are stored as given (opaque; slice 3 edits them).
pub fn create_preset_from_lists(
    v: &mut Value, name: &str,
    groups: &[i64], filtered_states: &[i64], always_shown_states: &[i64],
) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    let presets = presets_mut_or_create(ov);
    if presets.iter().any(|(k, _)| as_str(k).as_deref() == Some(name)) {
        return Err(OverviewTabError::PresetExists { name: name.to_string() });
    }
    let list = |xs: &[i64], sorted: bool| {
        let mut xs = xs.to_vec();
        if sorted { xs.sort_unstable(); }
        Value::List(xs.into_iter().map(Value::Int).collect())
    };
    let blob = Value::Dict(vec![
        (Value::Bytes(b"groups".to_vec()), list(groups, true)),
        (Value::Bytes(b"filteredStates".to_vec()), list(filtered_states, false)),
        (Value::Bytes(b"alwaysShownStates".to_vec()), list(always_shown_states, false)),
    ]);
    presets.push((Value::Bytes(name.as_bytes().to_vec()), blob));
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p settings-model create_from_lists`
Expected: PASS (3 tests).

- [ ] **Step 5: Wire the fork command**

Re-export in `crates/settings-model/src/lib.rs:39`: add `create_preset_from_lists`.

In `app/src-tauri/src/ops.rs`, add `create_preset_from_lists` to the `settings_model` import group, and add the wrapper near `preset_set_groups` (it composes two existing edits in ONE `edit_user_tabs` closure — fork then retarget):

```rust
pub fn preset_fork(
    state: &AppState, tab_idx: i64, name: String,
    groups: Vec<i64>, filtered_states: Vec<i64>, always_shown_states: Vec<i64>,
) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| {
        create_preset_from_lists(v, &name, &groups, &filtered_states, &always_shown_states)?;
        set_tab_preset(v, tab_idx, &name)
    })
}
```

In `app/src-tauri/src/lib.rs`, add the command near `preset_set_groups`:

```rust
#[tauri::command]
fn preset_fork(state: tauri::State<'_, AppState>, tab_idx: i64, name: String, groups: Vec<i64>, filtered_states: Vec<i64>, always_shown_states: Vec<i64>) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::preset_fork(&state, tab_idx, name, groups, filtered_states, always_shown_states)
}
```

Add `preset_fork` to the `generate_handler!` list next to `preset_set_groups`.

- [ ] **Step 6: Add the api.ts binding**

In `app/src/lib/api.ts`, near `presetSetGroups`:

```ts
  presetFork: (tabIdx: number, name: string, groups: number[], filteredStates: number[], alwaysShownStates: number[]) =>
    invoke<OverviewColumns>("preset_fork", { tabIdx, name, groups, filteredStates, alwaysShownStates }),
```

- [ ] **Step 7: Build to verify wiring**

Run: `cargo test -p settings-model` (full) and `cargo build -p app`, then (from `app/`, PowerShell) `npm run check`.
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/settings-model/src/overview_presets.rs crates/settings-model/src/lib.rs app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Add preset-fork authoring for building a preset from explicit lists"
```

---

## Task 3: Frontend pure helpers (`presets.ts`)

Node-testable logic for default detection, dropdown merge, fork naming, and resolving a default's definition — no Svelte/Tauri/JSON imports (the component passes the bundle in).

**Files:**
- Create: `app/src/lib/presets.ts`
- Create: `app/src/lib/presets.test.ts`

**Interfaces:**
- Produces: types `DefaultProfile` (`{ key, name, groups, filteredStates, alwaysShownStates }`), `DefaultsBundle` (`{ modern: DefaultProfile[], legacy: DefaultProfile[] }`); `LEGACY_NAMES`; `isDefaultKey(key): boolean`; `accountFormat(tabPresets: string[]): "modern" | "legacy"`; `defaultsForFormat(bundle, format): DefaultProfile[]`; `mergePresetOptions(storedNames, defaults): { defaults: string[]; user: string[] }`; `forkName(baseLabel, existingKeys): string`; `findDefault(defaults, key): DefaultProfile | undefined`.

- [ ] **Step 1: Write failing tests**

Create `app/src/lib/presets.test.ts`:

```ts
// Run: npm test (node --test). Throw-based checks, no framework.
import { isDefaultKey, accountFormat, defaultsForFormat, mergePresetOptions, forkName, findDefault, type DefaultsBundle } from "./presets.ts";

const check = (name: string, ok: boolean) => { if (!ok) throw new Error(`FAIL: ${name}`); console.log(`  ok - ${name}`); };
const eq = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

const bundle: DefaultsBundle = {
  modern: [
    { key: "DefaultPreset_639443", name: "All", groups: [25, 26], filteredStates: [], alwaysShownStates: [] },
    { key: "DefaultPreset_639442", name: "Mining", groups: [462], filteredStates: [1], alwaysShownStates: [] },
  ],
  legacy: [ { key: "defaultall", name: "All", groups: [25], filteredStates: [], alwaysShownStates: [] } ],
};

check("isDefaultKey modern", isDefaultKey("DefaultPreset_639443"));
check("isDefaultKey legacy", isDefaultKey("defaultpvp"));
check("isDefaultKey user is false", !isDefaultKey("my pvp"));

check("accountFormat modern from a DefaultPreset ref", accountFormat(["DefaultPreset_639452", "x"]) === "modern");
check("accountFormat legacy from a default* ref", accountFormat(["defaultall"]) === "legacy");
check("accountFormat defaults to modern with no default refs", accountFormat(["my pvp"]) === "modern");

check("defaultsForFormat picks the set", defaultsForFormat(bundle, "legacy")[0].key === "defaultall");

// merge: stored user presets + bundled defaults, deduped, split into defaults/user.
const merged = mergePresetOptions(["My PvP", "DefaultPreset_639443"], defaultsForFormat(bundle, "modern"));
check("merged defaults include both bundled defaults", eq(merged.defaults.sort(), ["DefaultPreset_639442", "DefaultPreset_639443"]));
check("merged user excludes the materialized default", eq(merged.user, ["My PvP"]));

check("forkName unique base", forkName("All", ["All copy"]) === "All copy 2");
check("forkName free base", forkName("All", []) === "All copy");

check("findDefault returns the profile", findDefault(defaultsForFormat(bundle, "modern"), "DefaultPreset_639442")!.groups[0] === 462);
check("findDefault miss", findDefault(defaultsForFormat(bundle, "modern"), "nope") === undefined);
```

- [ ] **Step 2: Run to verify it fails**

Run (from `app/`, PowerShell): `node --test src/lib/presets.test.ts`
Expected: FAIL — `./presets.ts` missing.

- [ ] **Step 3: Implement `presets.ts`**

Create `app/src/lib/presets.ts`:

```ts
// Pure helpers for EVE's built-in default overview profiles. No Svelte/Tauri/JSON
// deps, so this is node --test-able; the component imports the bundle JSON and
// passes it in.

export interface DefaultProfile {
  key: string;
  name: string;
  groups: number[];
  filteredStates: number[];
  alwaysShownStates: number[];
}
export interface DefaultsBundle { modern: DefaultProfile[]; legacy: DefaultProfile[]; }

export const LEGACY_NAMES: Record<string, string> = {
  defaultall: "All", defaultpvp: "PvP", defaultmining: "Mining", defaultloot: "Loot",
  defaultdrones: "Drones", defaultwarpto: "Warp To", default: "Default",
};

// A preset key that belongs to a built-in default (read-only, forked on edit).
export function isDefaultKey(key: string): boolean {
  return /^DefaultPreset_\d+$/.test(key) || key.toLowerCase().startsWith("default");
}

// The account's on-disk regime, inferred from the default profiles its tabs
// reference: modern (DefaultPreset_<id>) vs legacy (default* literals). Defaults
// to modern when no tab references a default (the offered defaults are a nicety).
export function accountFormat(tabPresets: string[]): "modern" | "legacy" {
  if (tabPresets.some((p) => /^DefaultPreset_\d+$/.test(p))) return "modern";
  if (tabPresets.some((p) => p.toLowerCase().startsWith("default"))) return "legacy";
  return "modern";
}

export function defaultsForFormat(bundle: DefaultsBundle, format: "modern" | "legacy"): DefaultProfile[] {
  return format === "legacy" ? bundle.legacy : bundle.modern;
}

// Dropdown split: all default keys (bundled ∪ any stored default) vs user keys.
// A materialized default appears once (in `defaults`, not `user`).
export function mergePresetOptions(storedNames: string[], defaults: DefaultProfile[]): { defaults: string[]; user: string[] } {
  const defaultKeys = new Set(defaults.map((d) => d.key));
  for (const n of storedNames) if (isDefaultKey(n)) defaultKeys.add(n);
  const user = storedNames.filter((n) => !isDefaultKey(n));
  return { defaults: [...defaultKeys].sort(), user };
}

// Unique "<base> copy" name given the keys already in use.
export function forkName(baseLabel: string, existingKeys: string[]): string {
  const set = new Set(existingKeys);
  const base = `${baseLabel} copy`;
  if (!set.has(base)) return base;
  let i = 2;
  while (set.has(`${base} ${i}`)) i++;
  return `${base} ${i}`;
}

export function findDefault(defaults: DefaultProfile[], key: string): DefaultProfile | undefined {
  return defaults.find((d) => d.key === key);
}
```

- [ ] **Step 4: Run to verify it passes**

Run (from `app/`, PowerShell): `node --test src/lib/presets.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/presets.ts app/src/lib/presets.test.ts
git commit -m "Add pure helpers for built-in default overview profiles"
```

---

## Task 4: Dropdown lists all defaults, grouped and assignable

Merge bundled defaults into the preset picker, grouped Default/Yours, with friendly labels.

**Files:**
- Modify: `app/src/lib/OverviewView.svelte`

**Interfaces:**
- Consumes: `presets.ts` (Task 3), `default-presets.json` (Task 1), `default-preset-names.json` (existing), `data.presets` (2b).

- [ ] **Step 1: Imports + derived defaults**

Add imports:
```ts
  import defaultPresetsBundle from "./data/default-presets.json";
  import { isDefaultKey, accountFormat, defaultsForFormat, mergePresetOptions, forkName, findDefault, LEGACY_NAMES, type DefaultsBundle, type DefaultProfile } from "./presets";
```
Add deriveds near the preset logic:
```ts
  const fmt = $derived(accountFormat((data?.tabs ?? []).map((t) => t.preset)));
  const bundledDefaults = $derived(defaultsForFormat(defaultPresetsBundle as DefaultsBundle, fmt));
  const storedNames = $derived((data?.presets ?? []).map((p) => p.name));
  const grouped = $derived(mergePresetOptions(storedNames, bundledDefaults));
```

- [ ] **Step 2: Extend `labelFor` for legacy literals**

Change `labelFor` (currently only handles `DefaultPreset_<id>`) so a legacy `default*` key resolves via `LEGACY_NAMES`:
```ts
  function labelFor(name: string): string {
    if (!name) return "(default)";
    const m = /^DefaultPreset_(\d+)$/.exec(name);
    if (m) return (defaultPresetNames as Record<string, string>)[m[1]] ?? name;
    return LEGACY_NAMES[name.toLowerCase()] ?? name;
  }
```

- [ ] **Step 3: Grouped `<optgroup>` dropdown**

Replace the preset `<select>`'s flat `{#each presetOptions...}` with two optgroups from `grouped` (keep `value={tab.preset}` and the same `onchange`/`setTabPreset`):
```svelte
<select value={tab.preset} onchange={(e) => setTabPreset((e.currentTarget as HTMLSelectElement).value)}>
  {#if !grouped.defaults.includes(tab.preset) && !grouped.user.includes(tab.preset)}
    <option value={tab.preset}>{labelFor(tab.preset)}</option>
  {/if}
  <optgroup label="Default profiles">
    {#each grouped.defaults as k (k)}<option value={k}>{labelFor(k)}</option>{/each}
  </optgroup>
  {#if grouped.user.length}
    <optgroup label="Your profiles">
      {#each grouped.user as k (k)}<option value={k}>{labelFor(k)}</option>{/each}
    </optgroup>
  {/if}
</select>
```

- [ ] **Step 4: Type-check and build**

Run (from `app/`, PowerShell): `npm run check` then `npm run build`.
Expected: 0 errors. (`presetOptions` may now be unused — remove it if so.)

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/OverviewView.svelte
git commit -m "List all built-in default profiles in the preset picker, grouped"
```

---

## Task 5: Contents render from bundle; edit/duplicate fork; defaults read-only

**Files:**
- Modify: `app/src/lib/OverviewView.svelte`

**Interfaces:**
- Consumes: Task 3 helpers, Task 2 `api.presetFork`, Task 4 deriveds.

- [ ] **Step 1: Resolve the current preset's groups from stored-or-bundle**

Replace the `presetGroups` derived so a default that isn't stored resolves from the bundle:
```ts
  const storedPreset = $derived(data?.presets.find((p) => p.name === tab?.preset));
  const currentDefault = $derived(tab ? findDefault(bundledDefaults, tab.preset) : undefined);
  const presetGroups = $derived(storedPreset?.groups ?? currentDefault?.groups ?? []);
  const editable = $derived(!!tab && (!!storedPreset || !!currentDefault));
```
Change the contents-section gate from `{#if presetIsReal && tab}` to `{#if editable && tab}`.

- [ ] **Step 2: Fork on group edit**

Rewrite `setPresetGroup` so editing a default forks it:
```ts
  async function setPresetGroup(id: number, on: boolean) {
    if (!tab) return;
    const next = toggleGroup(presetGroups, id, on);
    try {
      if (isDefaultKey(tab.preset)) {
        const def = currentDefault;
        const name = forkName(labelFor(tab.preset), (data?.presets ?? []).map((p) => p.name));
        data = await api.presetFork(tab.index, name, next, def?.filteredStates ?? [], def?.alwaysShownStates ?? []);
      } else {
        data = await api.presetSetGroups(tab.preset, next);
      }
      onUserDirty();
    } catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
```
(After a fork, `tab.preset` becomes the new user key via the refreshed `data`, so later toggles edit in place.)

- [ ] **Step 3: Duplicate forks a default / clones a user preset, and switches the tab**

Update the duplicate flow: a default → fork (unchanged groups); a user preset → clone via `presetCreate`; both retarget the tab to the copy. Replace `startDuplicatePreset` + its `submitPending` branch with a direct action (no name prompt — auto-named per the design):
```ts
  async function duplicatePreset() {
    if (!tab) return;
    const name = forkName(labelFor(tab.preset), (data?.presets ?? []).map((p) => p.name));
    try {
      if (isDefaultKey(tab.preset)) {
        const def = currentDefault;
        data = await api.presetFork(tab.index, name, presetGroups, def?.filteredStates ?? [], def?.alwaysShownStates ?? []);
      } else {
        data = await api.presetCreate(tab.preset, name);
        data = await api.tabSetPreset(tab.index, name);
      }
      onUserDirty();
    } catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
```
Point the "Duplicate preset" button's `onclick` at `duplicatePreset` (remove the old `startDuplicatePreset` + the `duplicatePreset` `pending` kind if now unused).

- [ ] **Step 4: Defaults are read-only for Rename/Delete; Duplicate always available**

Update the button gating:
```svelte
<button onclick={duplicatePreset} disabled={!editable} title="Duplicate this preset">Duplicate preset</button>
<button onclick={startRenamePreset} disabled={!storedPreset || isDefaultKey(tab.preset)} title="Rename this preset">Rename preset</button>
<button class="danger" onclick={deletePreset} disabled={!storedPreset || isDefaultKey(tab.preset) || (data?.presets.length ?? 0) <= 1} title="Delete this preset">Delete preset</button>
```
(Rename/Delete require a stored, non-default preset. Duplicate works on defaults too.)

- [ ] **Step 5: Type-check, build, test**

Run (from `app/`, PowerShell): `npm run check`, `npm run build`, `npm test`.
Expected: 0 errors; all suites (incl. `presets.test.ts`, `groups.test.ts`) green.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/OverviewView.svelte
git commit -m "Fork built-in profiles on edit and duplicate; keep defaults read-only"
```

---

## Task 6: Verify + live smoke

**Files:** none (verification).

- [ ] **Step 1: Full automated gates**

Run: `cargo test -p settings-model`, `cargo build -p app`; from `app/` (PowerShell) `npm run check`, `npm run build`, `npm test`.
Expected: all green.

- [ ] **Step 2: Live smoke on a CLEAN account (De l'opera)**

Launch (`npm run tauri dev` from `app/`, PowerShell). Open De l'opera → Overview:
- The preset dropdown lists **all** default profiles under "Default profiles", with real names (All, General, Mining, …), plus any user profiles under "Your profiles".
- Switch the tab to a different default (e.g. Mining) — the tab's preset changes and its contents show.
- Toggle a group on a default → it **auto-forks** to "<name> copy", the tab switches to the copy, and the checklist reflects the edit.
- Click Duplicate on a default → a "<name> copy" is created and the tab switches to it.
- Rename/Delete are **disabled** while a default is selected, enabled on the forked copy.
- Save → reopen the file → the fork persisted; confirm EVE accepts it (the minted `overviewProfilePresets` container with a zero timestamp loads cleanly, and the forked profile appears in-game).

- [ ] **Step 3: Record the smoke result** in the ledger/memory; note especially whether EVE accepts the freshly-minted `overviewProfilePresets` container (the one timestamp-related risk).

---

## Self-Review notes (for the implementer)

- **Spec coverage:** §3 bundle → Task 1; §4 dropdown merge/format/optgroup → Tasks 3+4; §5 assign → Task 4 (reuses `set_tab_preset`); §6 contents-from-bundle → Task 5 Step 1; §7 fork → Tasks 2+5; §8 read-only → Task 5 Step 4; §9 legacy → `accountFormat`/`defaultsForFormat` (Task 3) + Task 4.
- **Type consistency:** `DefaultProfile { key, name, groups, filteredStates, alwaysShownStates }` is identical in the bundle JSON (Task 1), `presets.ts` (Task 3), and the `api.presetFork` args (Task 2). `preset_fork(tab_idx, name, groups, filtered_states, always_shown_states)` params match between ops (Task 2), the command (Task 2), and `api.presetFork` (camelCase→snake_case via Tauri).
- **The one runtime risk** is EVE accepting a freshly-minted `overviewProfilePresets` `(Long zeros, dict)` on a clean account — gated by the Task 6 live smoke, not unit-testable.
- States are carried through a fork but never edited (slice 3).
