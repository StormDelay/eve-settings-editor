//! ESI character-name resolution: a single batched network call plus an
//! on-disk, cache-forever store. Small sync helpers (this file's lower half)
//! carry all the logic and are unit-tested without a Tauri runtime or the
//! network; only `esi_fetch` touches the wire. Failure is always
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
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| FetchError(e.to_string()))?;
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
fn resolve_with<F>(dir: &Path, ids: &[u64], refetch_all: bool, fetch: F) -> Cache
where
    F: FnOnce(&[u64]) -> Result<Vec<EsiName>, FetchError>,
{
    let mut cache = load_cache(dir);
    let need = needed(ids, &cache, refetch_all);
    if !need.is_empty() {
        match fetch(&need) {
            Ok(fetched) => {
                apply_fetch(&mut cache, fetched);
                let _ = save_cache(dir, &cache);
            }
            // Read the detail (silences the never-read warning) and leave a
            // stderr breadcrumb; the user still just sees bare ids (silent).
            Err(e) => eprintln!("name resolution: fetch failed ({})", e.0),
        }
    }
    select(ids, &cache)
}

/// Production wiring: `resolve_with` driven by the real ESI fetcher. Blocking —
/// call it from a worker thread.
pub fn resolve_blocking(dir: &Path, ids: &[u64], refetch_all: bool) -> Cache {
    resolve_with(dir, ids, refetch_all, esi_fetch)
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
}
