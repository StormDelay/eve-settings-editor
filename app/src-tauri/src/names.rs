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
}
