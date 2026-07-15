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
