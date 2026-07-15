# M3a — ESI character-name resolution (design)

Date: 2026-07-15
Status: approved, pre-plan
Builds on: M1 (discovery, app shell), design spec §6 "Name display & resolution".

M3 was re-ordered during this brainstorm into three sub-milestones, in order:
**M3a — ESI name resolution** (this doc), then the **character/user association
flow** (uses resolved names), then the **overview editor** (uses the
association for its char↔user file pairing; two-slot `char`/`user` app state,
approach A). Packaging and the autofill editor move to their own milestones
*after* M3.

## 1. Goal

Show a human-readable character name next to every character ID in the UI
(starting with the sidebar file list), so files read as "Jita Trader
(core_char_<id>.dat)" instead of a bare number. Names come from EVE's public
ESI API, are cached on disk, and degrade silently to bare IDs when the network
or cache can't supply one.

This is the app's only network behavior.

## 2. Data source — ESI `/universe/names` (from design spec §6, M0 findings)

Local extraction was rejected in M0 (files hold no ID→name structure). Character
IDs resolve online:

- Endpoint: `POST https://esi.evetech.net/latest/universe/names/`.
- Request body: a JSON array of integer IDs, e.g. `[90000001, 90000002]`.
  Batched — one request for all unresolved IDs (ESI accepts up to 1000; a real
  install has well under that).
- Response: a JSON array of `{ "category": "character", "id": <int>,
  "name": "<string>" }`. We keep `name` and `category`.
- `User-Agent: eve-settings-editor` header on every request (ESI etiquette).
- **Whole-batch failure mode:** if *any* ID in the batch is invalid, ESI returns
  HTTP 404 for the entire request — no partial results. V1 treats this like any
  other failure (fall back to cache). Per-ID bisect-to-salvage is deferred (§9).
- Account IDs have no public API and are never sent here — they get aliases in
  the association flow, not this milestone.

## 3. Scope

In scope for M3a:

- Resolve the character IDs discovery already extracts from `core_char_<id>`
  filenames (`SettingsFile.id`).
- Persistent on-disk cache in the app-data dir; cache-forever (character names
  effectively never change).
- Eager resolution: the frontend resolves right after `discover_profiles`, so
  names appear without user action.
- The resolved `id → name` map is shared app-wide (a small Svelte store, §5), so
  names show **everywhere a character id appears in V1**:
  - the **sidebar file list** — `Name (id)` per char file, bare `id` otherwise;
  - the **open-file header** (the `.filebar` in `+page.svelte`) — the character
    name alongside `core_char_<id>.dat` for the loaded char file.
  The backups panel shows no id of its own (it is scoped to the already-labeled
  open file), so it needs no change. Batch-apply source/target lists are M4.
- A manual **Refresh names** action re-fetches ignoring the cache (covers the
  rare paid rename).

Explicitly **not** in scope:

- **No network toggle / settings store.** This consciously overrides design
  spec §6/§11 ("network access is disableable via a settings toggle") — dropped
  for a personal/corp tool; re-addable before any public release. The network is
  simply always attempted, and failure is silent.
- Account IDs, aliases, char↔user association — the next sub-milestone.
- The OS window-title bar (Tauri window title) — the in-app filebar is the
  visible file identity; setting the native title too is a trivial later add.

## 4. Architecture — Rust owns the call and the cache

Chosen over a frontend `fetch` (approach B): the cache must live in the app-data
dir and the network call must be a single, well-defined owner, both of which
want one Rust module rather than a webview/Rust split (and B would also need a
new CSP `connect-src` entry, itself a still-open M1b-2 deferral).

### 4.1 `app/src-tauri/src/names.rs` (new module)

Mirrors `ops.rs`'s "plain functions, testable without a Tauri runtime" style.
The resolve logic is split from the network so `cargo test` never hits the wire:

- A **fetcher** parameter — any `Fn(&[u64]) -> Result<Vec<Resolved>, FetchError>`
  (closure or boxed trait object). Tests inject a fake; production injects the
  reqwest one.
- `resolve(ids, cache_dir, fetch) -> HashMap<u64, ResolvedName>`:
  1. Load `cache_dir/names-cache.json` (missing/corrupt → empty map, never an
     error).
  2. Split `ids` into cache hits and misses.
  3. If there are misses, call `fetch(misses)`. On `Ok`, merge results into the
     cache map and persist the file. On `Err`, skip — keep going with hits only.
  4. Return the union of cache hits and any freshly fetched names. IDs that
     resolved to nothing are simply **absent** from the returned map.
- `refresh(ids, cache_dir, fetch)` — same, but treats every id as a miss
  (ignores existing cache entries) so a rename can be picked up.
- The production fetcher: **blocking** `reqwest` (rustls-tls, json) POST to the
  §2 endpoint with the User-Agent header; maps any HTTP/transport/JSON error to
  `FetchError` (all handled identically — silent fallback). Blocking keeps the
  whole orchestrator (`resolve_with`) synchronous and unit-testable with a fake
  fetcher; the async Tauri command runs it via `spawn_blocking` so it never
  blocks the runtime.

`ResolvedName { name: String, category: String }`. Cache file shape:
`{ "<id>": { "name": ..., "category": ... } }` (serde_json).

No new `AppState` field — the cache is the JSON file, read-modify-written per
call. Resolution fires only on discovery and manual refresh (never concurrent in
a single-user desktop UI), so no in-memory cache or locking is needed.

### 4.2 Command surface (`lib.rs` + `ops.rs` + `api.ts`)

Two new async commands, taking `app: tauri::AppHandle` to reach
`app.path().app_data_dir()`:

- `resolve_character_names(ids: Vec<u64>) -> HashMap<u64, ResolvedName>`
- `refresh_character_names(ids: Vec<u64>) -> HashMap<u64, ResolvedName>`

Both are infallible to the frontend — they always return a map (possibly
partial, possibly empty). No error DTO: failure is indistinguishable from "not
found" by design, and both render as a bare ID. Added to the
`generate_handler!` list; `resolve`/`refresh` wrappers delegate to `names.rs`
with the real app-data dir and the reqwest fetcher.

## 5. Frontend

- `api.ts` gains `resolveCharacterNames(ids)` / `refreshCharacterNames(ids)`.
- **Shared store `app/src/lib/names.ts`** — a Svelte store holding the
  `id → name` map plus `resolveNames(ids)` / `refreshNames(ids)` helpers that
  call the API and update the store. This is the single source both the sidebar
  and the open-file header read, so no prop-drilling and one resolve per app run.
- The sidebar (`Sidebar.svelte`), after loading profiles, collects all char
  `SettingsFile.id`s across profiles and calls `resolveNames` once; each char
  row reads the store, showing `Name (id)` when known, else the bare id it shows
  today. User/other files are unchanged.
- The open-file header in `+page.svelte` reads the same store: for a
  `core_char_<id>.dat` file it prefixes the character name (id parsed from the
  filename, or carried on `OpenOutcome`); user/other files render as today.
- A **Refresh names** control (e.g. a small button by the profile list) calls
  `refreshNames` with the same ids; the store update repaints every consumer.
- No spinner-blocking: names fill in when the promise resolves; the file list
  and header are usable immediately with bare ids.

## 6. Error handling & edge cases

- **Offline / ESI down / transport error / malformed JSON:** silent. Return
  cached names; unresolved ids show bare. No error surfaced to the user.
- **Batch 404 (an invalid id in the set):** same as any failure — cache-only
  this call. (Salvaging the valid ids is §9.)
- **Corrupt/absent cache file:** treated as empty; a successful fetch rewrites
  it. A write failure is ignored (names still returned for this session).
- **Anomalous filenames** (`core_char__.dat`, `SettingsFile.id == None`): no id,
  never sent, always shown by filename as today.
- **Non-character category** in a response: stored as returned and displayed;
  not expected for char-file ids, but harmless.

## 7. Testing

- **Rust (`app` crate, `cargo test`, no network):** unit tests over `resolve`
  with an injected fetcher —
  - all-cache-hit (fetcher never called),
  - partial (some cached, some fetched, merged result correct + cache file
    updated on disk in a temp dir),
  - fetcher returns `Err` → returns cache-only, no panic,
  - unknown id absent from the result map,
  - `refresh` ignores cache and re-fetches every id.
  Plus a cache-file round-trip (write then reload) in a temp dir. Synthetic ids
  only (repo rule: no real character ids in fixtures).
- **Frontend:** display-only glue — the `names.ts` store forwards to the
  already-tested Rust commands, and the sidebar/header just look ids up and
  format `Name (id)`. No new pure logic that warrants a `node --test` (YAGNI);
  the manual smoke below exercises the wiring.
- **Manual smoke (one-time, live ESI):** run the app against real profiles,
  confirm names appear next to char files, kill the network and confirm bare-id
  fallback with cached names retained. Mirrors M0/M1 live validation.

## 8. Dependencies

- `app` crate gains `reqwest = { version = "0.12", default-features = false,
  features = ["rustls-tls", "json", "blocking"] }` (rustls avoids OpenSSL on
  Windows; the blocking client runs on a `spawn_blocking` worker thread).
  `blue-marshal` and `settings-model` stay dependency-free — the network dependency is confined to
  the Tauri binary, matching the architecture's crate boundaries.

## 9. Out of scope / deferred

- Network toggle / offline mode setting (dropped per above; re-add for public
  release).
- Per-ID bisect on a batch 404 to salvage the valid ids (`ponytail:` — retry
  in halves only if a real install actually hits mixed valid/invalid batches).
- Cache TTL / automatic staleness (manual Refresh covers renames; no evidence a
  TTL is worth the machinery).
- OS window-title bar (native Tauri title) — the in-app filebar covers the
  visible identity; setting the native title is a trivial later add.
- Account IDs, aliases, char↔user association — next sub-milestone.
