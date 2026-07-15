//! Character↔account association: a persisted store (aliases + confirmed
//! character membership) plus correlation logic (suggestion ranking, roster
//! assembly, capture diff). All logic is pure and FS-free behind injected
//! inputs; only the orchestrators at the bottom touch discovery/disk. Failure
//! is silent — a missing/corrupt store loads empty.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// EVE has always allowed at most 3 characters per account. Hard cap.
pub const MAX_CHARS_PER_ACCOUNT: usize = 3;

/// The persisted association state, keyed by account (user) id. Serialized to
/// JSON with string keys (serde_json), like the M3a names cache.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountsStore {
    #[serde(default)]
    pub accounts: HashMap<u64, Account>,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub characters: Vec<u64>,
}

fn store_path(dir: &Path) -> PathBuf {
    dir.join("accounts.json")
}

/// Load the store; any missing/corrupt/unreadable file yields an empty store.
pub fn load_store(dir: &Path) -> AccountsStore {
    match fs::read(store_path(dir)) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => AccountsStore::default(),
    }
}

/// Persist the store, creating the app-data dir if needed.
fn save_store(dir: &Path, store: &AccountsStore) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let bytes = serde_json::to_vec_pretty(store)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::write(store_path(dir), bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("accounts-test-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn load_missing_store_is_empty() {
        assert_eq!(load_store(&temp_dir("missing")), AccountsStore::default());
    }

    #[test]
    fn corrupt_store_loads_as_empty() {
        let dir = temp_dir("corrupt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(store_path(&dir), b"not json").unwrap();
        assert_eq!(load_store(&dir), AccountsStore::default());
    }

    #[test]
    fn store_round_trips_through_disk() {
        let dir = temp_dir("roundtrip");
        let mut store = AccountsStore::default();
        store.accounts.insert(
            987654,
            Account { alias: Some("Main".into()), characters: vec![90000001, 90000002] },
        );
        save_store(&dir, &store).unwrap();
        assert_eq!(load_store(&dir), store);
    }
}
