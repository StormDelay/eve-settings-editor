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

/// Set or clear an account's alias (blank/whitespace clears). Creates the
/// account entry if absent.
pub fn set_alias(store: &mut AccountsStore, user_id: u64, alias: Option<String>) {
    let acct = store.accounts.entry(user_id).or_default();
    acct.alias = alias.filter(|a| !a.trim().is_empty());
}

/// Confirm `char_id` belongs to `user_id`. Single-membership: the character is
/// removed from any other account. Idempotent. Rejects a 4th character on the
/// target account (the hard cap).
pub fn confirm(store: &mut AccountsStore, char_id: u64, user_id: u64) -> Result<(), String> {
    let already = store.accounts.get(&user_id).is_some_and(|a| a.characters.contains(&char_id));
    if already {
        return Ok(());
    }
    let full = store.accounts.get(&user_id).is_some_and(|a| a.characters.len() >= MAX_CHARS_PER_ACCOUNT);
    if full {
        return Err(format!("Account already has {MAX_CHARS_PER_ACCOUNT} characters"));
    }
    for acct in store.accounts.values_mut() {
        acct.characters.retain(|&c| c != char_id);
    }
    store.accounts.entry(user_id).or_default().characters.push(char_id);
    Ok(())
}

/// Remove a character from whatever account holds it (if any).
pub fn unpair(store: &mut AccountsStore, char_id: u64) {
    for acct in store.accounts.values_mut() {
        acct.characters.retain(|&c| c != char_id);
    }
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

    #[test]
    fn set_alias_sets_and_blank_clears() {
        let mut s = AccountsStore::default();
        set_alias(&mut s, 987654, Some("Main".into()));
        assert_eq!(s.accounts[&987654].alias.as_deref(), Some("Main"));
        set_alias(&mut s, 987654, Some("   ".into())); // blank clears
        assert_eq!(s.accounts[&987654].alias, None);
        set_alias(&mut s, 987654, None);
        assert_eq!(s.accounts[&987654].alias, None);
    }

    #[test]
    fn confirm_moves_char_to_exactly_one_account() {
        let mut s = AccountsStore::default();
        confirm(&mut s, 90000001, 111).unwrap();
        confirm(&mut s, 90000001, 222).unwrap(); // reassign to a different account
        assert_eq!(s.accounts[&111].characters, Vec::<u64>::new());
        assert_eq!(s.accounts[&222].characters, vec![90000001]);
    }

    #[test]
    fn confirm_is_idempotent() {
        let mut s = AccountsStore::default();
        confirm(&mut s, 90000001, 111).unwrap();
        confirm(&mut s, 90000001, 111).unwrap();
        assert_eq!(s.accounts[&111].characters, vec![90000001]);
    }

    #[test]
    fn confirm_rejects_the_fourth_character() {
        let mut s = AccountsStore::default();
        for c in [1u64, 2, 3] {
            confirm(&mut s, c, 111).unwrap();
        }
        let err = confirm(&mut s, 4, 111).unwrap_err();
        assert!(err.contains('3'), "cap message names the limit: {err}");
        assert_eq!(s.accounts[&111].characters, vec![1, 2, 3], "4th not added");
    }

    #[test]
    fn unpair_removes_from_whichever_account_holds_it() {
        let mut s = AccountsStore::default();
        confirm(&mut s, 90000001, 111).unwrap();
        unpair(&mut s, 90000001);
        assert!(s.accounts[&111].characters.is_empty());
        unpair(&mut s, 90000001); // no-op, no panic
    }
}
