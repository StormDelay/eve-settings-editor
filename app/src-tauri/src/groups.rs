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
