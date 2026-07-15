# M3a — ESI character-name resolution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show each character's name next to its ID in the sidebar file list and the open-file header, resolved from EVE's public ESI API and cached on disk, degrading silently to bare IDs when offline.

**Architecture:** Rust owns the single network call and the persistent cache. A new `app/src-tauri/src/names.rs` module holds small sync helpers (cache load/save, miss selection, merge, select, response parsing) plus one async orchestrator that awaits a `reqwest` POST to ESI `/universe/names`. Two thin `#[tauri::command]` wrappers expose it. The frontend reads the results through a Svelte-5 `$state` module (`names.svelte.ts`) that both the sidebar and the open-file header subscribe to.

**Tech Stack:** Rust, Tauri 2, `reqwest` 0.12 (rustls-tls), `serde`/`serde_json`; TypeScript, Svelte 5 (runes), Vite.

## Global Constraints

- **Commits:** sentence-case summary line, **no attribution trailers** of any kind (no `Co-Authored-By`, no "Generated with").
- **No personal data in the repo:** use obviously **synthetic** character IDs (e.g. `90000001`, `90000002`) in all fixtures/tests; never real character/account IDs.
- **Dependency boundary:** `reqwest` is added **only** to the `app` crate (`app/src-tauri/Cargo.toml`). `blue-marshal` and `settings-model` stay dependency-free.
- **Live-directory rule:** tests never read/write the live EVE settings directory and never hit the live network. The single real ESI call is exercised only by the manual smoke task.
- **No network toggle** (consciously overrides design-spec §6/§11): the network is always attempted; every failure is silent (bare IDs).
- **Windows shell:** `npm` and `gh` are **not** on the Bash tool's PATH — run `npm`/`cargo` commands with the **PowerShell** tool. `cargo` is invoked via `--manifest-path app/src-tauri/Cargo.toml` so it works from the repo root.
- **Frontend tests** run on `node --test` with zero dependencies (`npm test`); this feature adds none (display-only glue), verified by `npm run check` + the manual smoke.

---

### Task 1: Add `reqwest` and the names module cache (load/save)

**Files:**
- Modify: `app/src-tauri/Cargo.toml` (dependencies)
- Create: `app/src-tauri/src/names.rs`
- Modify: `app/src-tauri/src/lib.rs:1` (add `mod names;`)

**Interfaces:**
- Produces:
  - `pub struct ResolvedName { pub name: String, pub category: String }` (Serialize, Deserialize, Clone, Debug, PartialEq)
  - `pub type Cache = std::collections::HashMap<u64, ResolvedName>`
  - `pub fn load_cache(dir: &std::path::Path) -> Cache`
  - `fn save_cache(dir: &std::path::Path, cache: &Cache) -> std::io::Result<()>`

- [ ] **Step 1: Add the dependency**

In `app/src-tauri/Cargo.toml`, under `[dependencies]`, add:

```toml
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json", "blocking"] }
```

(The `blocking` client keeps all resolution logic synchronous and testable; the
command runs it on a worker thread via `spawn_blocking`, so it never blocks the
async runtime — see Tasks 3–4.)

- [ ] **Step 2: Create `names.rs` with types and cache I/O**

Create `app/src-tauri/src/names.rs`:

```rust
//! ESI character-name resolution: a single batched network call plus an
//! on-disk, cache-forever store. Small sync helpers (this file's lower half)
//! carry all the logic and are unit-tested without a Tauri runtime or the
//! network; only `esi_fetch`/`resolve` touch the wire. Failure is always
//! silent — the caller falls back to bare IDs.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A resolved name plus its ESI category (expected "character").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedName {
    pub name: String,
    pub category: String,
}

/// id -> resolved name. Serialized to JSON with string keys (serde_json).
pub type Cache = HashMap<u64, ResolvedName>;

fn cache_path(dir: &Path) -> PathBuf {
    dir.join("names-cache.json")
}

/// Load the cache; any missing/corrupt/unreadable file yields an empty cache,
/// never an error (the feature must degrade silently).
pub fn load_cache(dir: &Path) -> Cache {
    match fs::read(cache_path(dir)) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Cache::new(),
    }
}

/// Persist the cache, creating the app-data dir if needed.
fn save_cache(dir: &Path, cache: &Cache) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let bytes = serde_json::to_vec_pretty(cache)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::write(cache_path(dir), bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir()
            .join(format!("names-test-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn load_missing_cache_is_empty() {
        let dir = temp_dir("missing");
        assert!(load_cache(&dir).is_empty());
    }

    #[test]
    fn cache_round_trips_through_disk() {
        let dir = temp_dir("roundtrip");
        let mut cache = Cache::new();
        cache.insert(
            90000001,
            ResolvedName { name: "Test Pilot".into(), category: "character".into() },
        );
        save_cache(&dir, &cache).unwrap();
        assert_eq!(load_cache(&dir), cache);
    }

    #[test]
    fn corrupt_cache_loads_as_empty() {
        let dir = temp_dir("corrupt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(cache_path(&dir), b"not json").unwrap();
        assert!(load_cache(&dir).is_empty());
    }
}
```

- [ ] **Step 3: Register the module**

In `app/src-tauri/src/lib.rs`, add as the first line (above `mod ops;`):

```rust
mod names;
```

- [ ] **Step 4: Run the tests (they must compile and pass)**

Run (PowerShell): `cargo test --manifest-path app/src-tauri/Cargo.toml names::tests`
Expected: `load_missing_cache_is_empty`, `cache_round_trips_through_disk`, `corrupt_cache_loads_as_empty` all PASS. (First run downloads/builds `reqwest`; that is expected.)

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/Cargo.toml Cargo.lock app/src-tauri/src/names.rs app/src-tauri/src/lib.rs
git commit -m "Add reqwest and the ESI names cache module"
```

---

### Task 2: Miss selection, merge, and select logic

**Files:**
- Modify: `app/src-tauri/src/names.rs`

**Interfaces:**
- Consumes: `Cache`, `ResolvedName` (Task 1).
- Produces:
  - `struct EsiName { category: String, id: u64, name: String }` (Deserialize, Debug, PartialEq)
  - `fn needed(ids: &[u64], cache: &Cache, refetch_all: bool) -> Vec<u64>`
  - `fn apply_fetch(cache: &mut Cache, fetched: Vec<EsiName>)`
  - `fn select(ids: &[u64], cache: &Cache) -> Cache`

- [ ] **Step 1: Write the failing tests**

Add these tests inside the existing `mod tests` in `app/src-tauri/src/names.rs`:

```rust
    #[test]
    fn needed_returns_deduped_misses() {
        let mut cache = Cache::new();
        cache.insert(2, ResolvedName { name: "Two".into(), category: "character".into() });
        // ids has a duplicate (2) and a cached id (2); only 1 and 3 are missing.
        assert_eq!(needed(&[1, 2, 2, 3], &cache, false), vec![1, 3]);
    }

    #[test]
    fn needed_with_refetch_all_returns_every_id_deduped() {
        let mut cache = Cache::new();
        cache.insert(2, ResolvedName { name: "Two".into(), category: "character".into() });
        assert_eq!(needed(&[1, 2, 2, 3], &cache, true), vec![1, 2, 3]);
    }

    #[test]
    fn apply_fetch_merges_into_cache() {
        let mut cache = Cache::new();
        apply_fetch(
            &mut cache,
            vec![EsiName { category: "character".into(), id: 90000001, name: "Alpha".into() }],
        );
        assert_eq!(cache.get(&90000001).unwrap().name, "Alpha");
    }

    #[test]
    fn select_returns_known_ids_only() {
        let mut cache = Cache::new();
        cache.insert(1, ResolvedName { name: "One".into(), category: "character".into() });
        let out = select(&[1, 2], &cache);
        assert_eq!(out.len(), 1);
        assert!(out.contains_key(&1));
        assert!(!out.contains_key(&2), "unknown ids are absent, not blank");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path app/src-tauri/Cargo.toml names::tests`
Expected: FAIL to compile — `needed`, `apply_fetch`, `select`, `EsiName` not found.

- [ ] **Step 3: Implement the helpers**

In `app/src-tauri/src/names.rs`, add above the `#[cfg(test)]` block (after `save_cache`):

```rust
/// One entry from ESI `/universe/names`.
#[derive(Debug, PartialEq, Deserialize)]
struct EsiName {
    category: String,
    id: u64,
    name: String,
}

/// The (deduplicated) ids that must be fetched: cache misses, or every id when
/// `refetch_all` (a manual refresh that ignores existing cache entries).
fn needed(ids: &[u64], cache: &Cache, refetch_all: bool) -> Vec<u64> {
    let mut seen = std::collections::HashSet::new();
    ids.iter()
        .copied()
        .filter(|id| seen.insert(*id))
        .filter(|id| refetch_all || !cache.contains_key(id))
        .collect()
}

/// Merge freshly fetched names into the cache (newer wins).
fn apply_fetch(cache: &mut Cache, fetched: Vec<EsiName>) {
    for n in fetched {
        cache.insert(n.id, ResolvedName { name: n.name, category: n.category });
    }
}

/// The subset of `ids` the cache can name; ids with no name are omitted.
fn select(ids: &[u64], cache: &Cache) -> Cache {
    ids.iter()
        .filter_map(|id| cache.get(id).map(|n| (*id, n.clone())))
        .collect()
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path app/src-tauri/Cargo.toml names::tests`
Expected: all names tests PASS.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/names.rs
git commit -m "Add miss-selection, merge, and select logic for names"
```

---

### Task 3: ESI parsing and the (sync, injectable) orchestrator

**Files:**
- Modify: `app/src-tauri/src/names.rs`

**Interfaces:**
- Consumes: `needed`, `apply_fetch`, `select`, `load_cache`, `save_cache`, `EsiName` (Tasks 1–2).
- Produces:
  - `pub struct FetchError(pub String)`
  - `fn parse_names(bytes: &[u8]) -> Result<Vec<EsiName>, FetchError>`
  - `fn esi_fetch(ids: &[u64]) -> Result<Vec<EsiName>, FetchError>` (blocking reqwest)
  - `pub fn resolve_with<F>(dir: &Path, ids: &[u64], refetch_all: bool, fetch: F) -> Cache where F: FnOnce(&[u64]) -> Result<Vec<EsiName>, FetchError>` — the tested orchestrator with an injected fetcher
  - `pub fn resolve_blocking(dir: &Path, ids: &[u64], refetch_all: bool) -> Cache` — `resolve_with` wired to `esi_fetch`

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `app/src-tauri/src/names.rs`:

```rust
    #[test]
    fn parse_names_reads_an_esi_array() {
        let body = br#"[{"category":"character","id":90000001,"name":"Test Pilot"}]"#;
        let parsed = parse_names(body).unwrap();
        assert_eq!(
            parsed,
            vec![EsiName {
                category: "character".into(),
                id: 90000001,
                name: "Test Pilot".into(),
            }]
        );
    }

    #[test]
    fn parse_names_rejects_an_esi_error_object() {
        // ESI returns a JSON object (not an array) on a 404 bad-id batch.
        let body = br#"{"error":"Ensure all IDs are valid before resolving."}"#;
        assert!(parse_names(body).is_err());
    }

    #[test]
    fn resolve_with_skips_fetch_when_all_ids_cached() {
        let dir = temp_dir("all-cached");
        let mut seed = Cache::new();
        seed.insert(1, ResolvedName { name: "One".into(), category: "character".into() });
        seed.insert(2, ResolvedName { name: "Two".into(), category: "character".into() });
        save_cache(&dir, &seed).unwrap();
        // The fetcher must never be called when nothing is missing.
        let out = resolve_with(&dir, &[1, 2], false, |_| panic!("must not fetch"));
        assert_eq!(out, seed);
    }

    #[test]
    fn resolve_with_falls_back_to_cache_on_fetch_error() {
        let dir = temp_dir("fetch-err");
        let mut seed = Cache::new();
        seed.insert(1, ResolvedName { name: "One".into(), category: "character".into() });
        save_cache(&dir, &seed).unwrap();
        let out = resolve_with(&dir, &[1, 2], false, |_| Err(FetchError("offline".into())));
        // id 1 from cache; id 2 unresolved and therefore absent.
        assert_eq!(out.len(), 1);
        assert_eq!(out.get(&1).unwrap().name, "One");
    }

    #[test]
    fn resolve_with_fetches_only_misses_then_merges_and_persists() {
        let dir = temp_dir("partial");
        let mut seed = Cache::new();
        seed.insert(1, ResolvedName { name: "One".into(), category: "character".into() });
        save_cache(&dir, &seed).unwrap();
        let out = resolve_with(&dir, &[1, 2], false, |misses| {
            assert_eq!(misses, &[2], "only the uncached id is fetched");
            Ok(vec![EsiName { category: "character".into(), id: 2, name: "Two".into() }])
        });
        assert_eq!(out.get(&1).unwrap().name, "One");
        assert_eq!(out.get(&2).unwrap().name, "Two");
        // The fetched name was written back to disk.
        assert_eq!(load_cache(&dir).get(&2).unwrap().name, "Two");
    }

    #[test]
    fn resolve_with_refetch_all_ignores_cache() {
        let dir = temp_dir("refetch");
        let mut seed = Cache::new();
        seed.insert(1, ResolvedName { name: "Old".into(), category: "character".into() });
        save_cache(&dir, &seed).unwrap();
        let out = resolve_with(&dir, &[1], true, |misses| {
            assert_eq!(misses, &[1], "refetch_all re-requests cached ids");
            Ok(vec![EsiName { category: "character".into(), id: 1, name: "New".into() }])
        });
        assert_eq!(out.get(&1).unwrap().name, "New");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path app/src-tauri/Cargo.toml names::tests`
Expected: FAIL to compile — `parse_names` / `resolve_with` not found.

- [ ] **Step 3: Implement parsing, fetch, and orchestrator**

In `app/src-tauri/src/names.rs`, add after the `select` function:

```rust
const ESI_URL: &str = "https://esi.evetech.net/latest/universe/names/";

/// Any resolution failure, collapsed to a message. All failures are handled
/// identically by the caller (silent fallback), so the detail is only for logs.
#[derive(Debug)]
pub struct FetchError(pub String);

/// Parse an ESI `/universe/names` success body (a JSON array). An ESI error
/// body is a JSON object and fails here — treated as a fetch failure.
fn parse_names(bytes: &[u8]) -> Result<Vec<EsiName>, FetchError> {
    serde_json::from_slice(bytes).map_err(|e| FetchError(e.to_string()))
}

/// The one network call: POST the id array to ESI, batched. Blocking client —
/// callers run it off the async runtime (see the command's `spawn_blocking`).
/// Any HTTP, transport, or parse problem becomes `FetchError`.
fn esi_fetch(ids: &[u64]) -> Result<Vec<EsiName>, FetchError> {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(ESI_URL)
        .header(reqwest::header::USER_AGENT, "eve-settings-editor")
        .json(&ids)
        .send()
        .map_err(|e| FetchError(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(FetchError(format!("ESI status {}", resp.status())));
    }
    let bytes = resp.bytes().map_err(|e| FetchError(e.to_string()))?;
    parse_names(&bytes)
}

/// Resolve `ids` to names with an injectable fetcher (so tests never hit the
/// network): return cache hits immediately, fetch the misses (or every id when
/// `refetch_all`), merge and persist on success, and on any fetch failure
/// return whatever the cache already held. The result contains only ids that
/// could be named.
pub fn resolve_with<F>(dir: &Path, ids: &[u64], refetch_all: bool, fetch: F) -> Cache
where
    F: FnOnce(&[u64]) -> Result<Vec<EsiName>, FetchError>,
{
    let mut cache = load_cache(dir);
    let need = needed(ids, &cache, refetch_all);
    if !need.is_empty() {
        if let Ok(fetched) = fetch(&need) {
            apply_fetch(&mut cache, fetched);
            let _ = save_cache(dir, &cache);
        }
    }
    select(ids, &cache)
}

/// Production wiring: `resolve_with` driven by the real ESI fetcher. Blocking —
/// call it from a worker thread.
pub fn resolve_blocking(dir: &Path, ids: &[u64], refetch_all: bool) -> Cache {
    resolve_with(dir, ids, refetch_all, esi_fetch)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path app/src-tauri/Cargo.toml names::tests`
Expected: all names tests PASS (parse + the four `resolve_with` orchestration tests). Only `esi_fetch` (the raw HTTP round-trip) is not unit-tested — it is covered by the manual smoke (Task 8).

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/names.rs
git commit -m "Add ESI response parsing and the resolve orchestrator"
```

---

### Task 4: Expose the two Tauri commands

**Files:**
- Modify: `app/src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `names::resolve`, `names::ResolvedName` (Task 3).
- Produces (frontend contract): commands `resolve_character_names(ids: number[]) -> Record<string, {name,category}>` and `refresh_character_names(ids: number[]) -> …`.

- [ ] **Step 1: Add the command wrappers**

In `app/src-tauri/src/lib.rs`, after the `use ops::{...};` line add:

```rust
use std::collections::HashMap;
use tauri::Manager;
```

Then add two commands (e.g. after the `window_layout` command, before `run`):

```rust
#[tauri::command]
async fn resolve_character_names(
    app: tauri::AppHandle,
    ids: Vec<u64>,
) -> HashMap<u64, names::ResolvedName> {
    let dir = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir());
    // Blocking ESI/file work off the async runtime; empty map on join failure.
    tauri::async_runtime::spawn_blocking(move || names::resolve_blocking(&dir, &ids, false))
        .await
        .unwrap_or_default()
}

#[tauri::command]
async fn refresh_character_names(
    app: tauri::AppHandle,
    ids: Vec<u64>,
) -> HashMap<u64, names::ResolvedName> {
    let dir = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir());
    tauri::async_runtime::spawn_blocking(move || names::resolve_blocking(&dir, &ids, true))
        .await
        .unwrap_or_default()
}
```

- [ ] **Step 2: Register them in the handler**

In `app/src-tauri/src/lib.rs`, extend the `tauri::generate_handler!` list to include the two new commands:

```rust
        .invoke_handler(tauri::generate_handler![
            discover_profiles, open_file, close_file,
            apply_mutation, save_document, list_file_backups, restore_backup,
            window_layout, resolve_character_names, refresh_character_names
        ])
```

- [ ] **Step 3: Verify the crate compiles and all tests still pass**

Run: `cargo test --manifest-path app/src-tauri/Cargo.toml`
Expected: builds cleanly; every existing test plus the names tests PASS.

- [ ] **Step 4: Commit**

```bash
git add app/src-tauri/src/lib.rs
git commit -m "Expose resolve/refresh character-name commands"
```

---

### Task 5: Frontend API bindings and the names store

**Files:**
- Modify: `app/src/lib/api.ts`
- Create: `app/src/lib/names.svelte.ts`

**Interfaces:**
- Consumes: the two Rust commands (Task 4).
- Produces:
  - `api.ts`: `interface ResolvedName { name: string; category: string }`, `type NameMap = Record<string, ResolvedName>`, `api.resolveCharacterNames(ids: number[]): Promise<NameMap>`, `api.refreshCharacterNames(ids: number[]): Promise<NameMap>`
  - `names.svelte.ts`: `export const names` (a `$state` `Record<string, ResolvedName>`), `resolveNames(ids: number[]): Promise<void>`, `refreshNames(ids: number[]): Promise<void>`

- [ ] **Step 1: Add types and methods to `api.ts`**

In `app/src/lib/api.ts`, add near the other interfaces (e.g. after `BackupInfo`):

```ts
export interface ResolvedName {
  name: string;
  category: string;
}
export type NameMap = Record<string, ResolvedName>;
```

Then add two methods inside the `api` object (after `windowLayout`):

```ts
  resolveCharacterNames: (ids: number[]) =>
    invoke<NameMap>("resolve_character_names", { ids }),
  refreshCharacterNames: (ids: number[]) =>
    invoke<NameMap>("refresh_character_names", { ids }),
```

- [ ] **Step 2: Create the names store**

Create `app/src/lib/names.svelte.ts`:

```ts
// Shared, app-wide character-name map. A Svelte-5 rune module so the sidebar
// and the open-file header both react to the same state. Resolution failures
// are swallowed — unresolved ids simply render as bare ids.
import { api } from "./api";

export const names = $state<Record<string, { name: string; category: string }>>({});

function usable(ids: number[]): number[] {
  return ids.filter((id) => Number.isFinite(id));
}

export async function resolveNames(ids: number[]): Promise<void> {
  const wanted = usable(ids);
  if (wanted.length === 0) return;
  try {
    Object.assign(names, await api.resolveCharacterNames(wanted));
  } catch {
    // Silent: leave ids bare.
  }
}

export async function refreshNames(ids: number[]): Promise<void> {
  const wanted = usable(ids);
  if (wanted.length === 0) return;
  try {
    Object.assign(names, await api.refreshCharacterNames(wanted));
  } catch {
    // Silent.
  }
}
```

- [ ] **Step 3: Type-check**

Run (PowerShell): `npm run check --prefix app`
Expected: 0 errors, 0 warnings (the new files type-check).

- [ ] **Step 4: Commit**

```bash
git add app/src/lib/api.ts app/src/lib/names.svelte.ts
git commit -m "Add name-resolution API bindings and shared store"
```

---

### Task 6: Show names in the sidebar and add a Refresh names control

**Files:**
- Modify: `app/src/lib/Sidebar.svelte`

**Interfaces:**
- Consumes: `names`, `resolveNames`, `refreshNames` (Task 5); `Profile`/`SettingsFile` (existing `api.ts`).

- [ ] **Step 1: Import the store and add a char-id helper**

In `app/src/lib/Sidebar.svelte`, extend the existing import and add a helper in the `<script>`:

```ts
  import { names, resolveNames, refreshNames } from "./names.svelte";
```

Add (below the `rows` derived block):

```ts
  const charIds = (ps: Profile[]) =>
    ps
      .flatMap((p) => p.files)
      .filter((f) => f.kind === "char" && f.id != null)
      .map((f) => f.id as number);
```

- [ ] **Step 2: Resolve names after discovery**

In `Sidebar.svelte`, in `refresh()`, right after `profiles = await api.discover();` add:

```ts
      void resolveNames(charIds(profiles));
```

- [ ] **Step 3: Add the Refresh names button**

In the `.sidebar-actions` div, add a third button after the rescan (`⟳`) button:

```svelte
    <button
      onclick={() => refreshNames(charIds(profiles))}
      title="Re-fetch character names from ESI">Refresh names</button>
```

- [ ] **Step 4: Render the name in each char row**

In `Sidebar.svelte`, replace the file button's contents:

```svelte
            <button class="file" onclick={() => onOpen(f.path)} title={f.file_name}>
              {#if f.kind === "char" && f.id != null && names[f.id]}
                {names[f.id].name}
              {:else}
                {f.file_name}
              {/if}
              <span class="meta">{Math.round(f.size / 1024)} KB</span>
            </button>
```

- [ ] **Step 5: Type-check**

Run: `npm --prefix app run check`
Expected: 0 errors.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/Sidebar.svelte
git commit -m "Show character names in the sidebar with a refresh control"
```

---

### Task 7: Show the character name in the open-file header

**Files:**
- Modify: `app/src/routes/+page.svelte`

**Interfaces:**
- Consumes: `names` (Task 5); `current` (`OpenOutcome`, existing).

- [ ] **Step 1: Import the store**

In `app/src/routes/+page.svelte`, add to the imports:

```ts
  import { names } from "$lib/names.svelte";
```

- [ ] **Step 2: Derive the open file's character name**

In the `<script>` (e.g. after the `reveal` state declaration), add:

```ts
  // Name for the loaded char file, if resolved. `core_char_<id>.dat` -> name.
  const openCharName = $derived.by(() => {
    if (current?.status !== "opened") return null;
    const m = current.file_name.match(/^core_char_(\d+)\.dat$/);
    return m && names[m[1]] ? names[m[1]].name : null;
  });
```

- [ ] **Step 3: Render it in the filebar**

In `+page.svelte`, replace the filename span:

```svelte
        <span class="filename">
          {#if openCharName}{openCharName} — {/if}{current.file_name}
        </span>
```

- [ ] **Step 4: Type-check and build**

Run: `npm --prefix app run check`
Expected: 0 errors.

Run: `npm --prefix app run build`
Expected: build succeeds (produces `app/build`).

- [ ] **Step 5: Commit**

```bash
git add app/src/routes/+page.svelte
git commit -m "Show the character name in the open-file header"
```

---

### Task 8: Manual smoke against live ESI

**Files:** none (verification only).

This is the one step that exercises the real network call, mirroring how M0/M1 did live validation. It is not automated (live-directory / no-network test rule).

- [ ] **Step 1: Run the app**

Run (PowerShell): `npm --prefix app run tauri dev`

- [ ] **Step 2: Verify names appear**

With real profiles discovered, confirm each `core_char_<id>.dat` row shows the character's name (not the bare filename), and that opening one shows `Name — core_char_<id>.dat` in the header. Confirm `core_user_*` and anomalous files (`core_char__.dat`) are unaffected.

- [ ] **Step 3: Verify the cache and offline fallback**

Confirm `names-cache.json` now exists in the app-data dir. Disable networking, restart the app: names still appear (served from cache). Delete `names-cache.json`, disable networking, restart: rows fall back to bare filenames with no error dialog.

- [ ] **Step 4: Verify Refresh names**

Re-enable networking, click **Refresh names**, confirm names (re)appear. No errors on repeated clicks.

- [ ] **Step 5: Final full test sweep**

Run: `cargo test --manifest-path app/src-tauri/Cargo.toml`
Run: `npm --prefix app test`
Expected: both green (this feature added Rust tests and no frontend tests; the existing frontend tests still pass).

---

## Notes / deferred (from the spec)

- **Per-ID bisect on a 404 batch** (`ponytail:` deferral): if a real install hits mixed valid/invalid id batches, retry `needed` ids in halves to salvage the valid ones. Not built — no evidence it is needed.
- **OS window-title bar**, **backups panel** (shows no id today), **batch-apply lists** (M4), **account IDs / aliases / char↔user association** (next sub-milestone) are out of scope.
