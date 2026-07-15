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
- Sidebar renders `Name (id)` when known, bare `id` otherwise.
- A manual **Refresh names** action re-fetches ignoring the cache (covers the
  rare paid rename).

Explicitly **not** in scope:

- **No network toggle / settings store.** This consciously overrides design
  spec §6/§11 ("network access is disableable via a settings toggle") — dropped
  for a personal/corp tool; re-addable before any public release. The network is
  simply always attempted, and failure is silent.
- Account IDs, aliases, char↔user association — the next sub-milestone.
- Names anywhere beyond the sidebar (backups panel, window titles) — trivial to
  extend once the resolver exists; not required to prove the feature.

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
- The production fetcher: async `reqwest` (rustls-tls, json) POST to the §2
  endpoint with the User-Agent header; maps any HTTP/transport/JSON error to
  `FetchError` (all handled identically — silent fallback).

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
- The sidebar (`Sidebar.svelte`), after loading profiles, collects all char
  `SettingsFile.id`s across profiles, calls `resolveCharacterNames` once, and
  holds an `id → name` map. Each char row shows `Name (id)` when the map has the
  id, else the bare id it already shows today. User/other files are unchanged.
- A **Refresh names** control (e.g. a small button by the profile list) calls
  `refreshCharacterNames` with the same ids and replaces the map.
- No spinner-blocking: names fill in when the promise resolves; the file list is
  usable immediately with bare ids.

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
- **Manual smoke (one-time, live ESI):** run the app against real profiles,
  confirm names appear next to char files, kill the network and confirm bare-id
  fallback with cached names retained. Mirrors M0/M1 live validation.

## 8. Dependencies

- `app` crate gains `reqwest = { version = "0.12", default-features = false,
  features = ["rustls-tls", "json"] }` (rustls avoids OpenSSL on Windows; async
  client runs on Tauri's existing tokio runtime). `blue-marshal` and
  `settings-model` stay dependency-free — the network dependency is confined to
  the Tauri binary, matching the architecture's crate boundaries.

## 9. Out of scope / deferred

- Network toggle / offline mode setting (dropped per above; re-add for public
  release).
- Per-ID bisect on a batch 404 to salvage the valid ids (`ponytail:` — retry
  in halves only if a real install actually hits mixed valid/invalid batches).
- Cache TTL / automatic staleness (manual Refresh covers renames; no evidence a
  TTL is worth the machinery).
- Names beyond the sidebar (backups panel, window titles) — one-line extensions
  once the resolver ships.
- Account IDs, aliases, char↔user association — next sub-milestone.
