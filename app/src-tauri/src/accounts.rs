//! Character↔account association: a persisted store (aliases + confirmed
//! character membership) plus correlation logic (suggestion ranking, roster
//! assembly, capture diff). All logic is pure and FS-free behind injected
//! inputs; only the orchestrators at the bottom touch discovery/disk. Failure
//! is silent — a missing/corrupt store loads empty.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use blue_marshal::Value;
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

/// Collect human-readable text leaves from a decoded settings value: UTF-8 and
/// UCS-2 strings, plus printable-ASCII byte strings (EVE stores window ids and
/// embedded character names as byte strings). Used to scan a user file for a
/// character's resolved name.
pub fn collect_strings(v: &Value) -> Vec<String> {
    let mut out = Vec::new();
    walk_strings(v, &mut out);
    out
}

fn walk_strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Str(s) | Value::StrUcs2(s) => out.push(s.clone()),
        Value::Bytes(b) if !b.is_empty() && b.iter().all(|c| (0x20..0x7F).contains(c)) => {
            out.push(String::from_utf8_lossy(b).into_owned());
        }
        Value::Tuple(items) | Value::List(items) => {
            for it in items {
                walk_strings(it, out);
            }
        }
        Value::Dict(entries) => {
            for (k, val) in entries {
                walk_strings(k, out);
                walk_strings(val, out);
            }
        }
        Value::Stream(inner) | Value::Shared { value: inner, .. } => walk_strings(inner, out),
        Value::Instance { class, state } => {
            walk_strings(class, out);
            walk_strings(state, out);
        }
        Value::Reduce { ctor, items, pairs } => {
            walk_strings(ctor, out);
            for it in items {
                walk_strings(it, out);
            }
            for (k, val) in pairs {
                walk_strings(k, out);
                walk_strings(val, out);
            }
        }
        _ => {}
    }
}

use std::collections::HashSet;

/// Only char/user files participate in correlation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Char,
    User,
}

/// A discovered settings file reduced to what correlation needs. `profile` is
/// the settings dir, so mtime clustering stays within one profile.
#[derive(Debug, Clone)]
pub struct FileMeta {
    pub id: u64,
    pub kind: Kind,
    pub mtime: u64,
    pub profile: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

/// A labelled guess that `char_id` belongs to `user_id`. Never authoritative.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Suggestion {
    pub char_id: u64,
    pub user_id: u64,
    pub confidence: Confidence,
    pub basis: String,
}

// ponytail: 10s clustering window eyeballed from logout write spacing; widen
// once real captures show how far apart the char/user writes actually land.
const MTIME_WINDOW_SECS: u64 = 10;

/// Rank correlation guesses. In-file name match is the primary, durable signal
/// (a user file's text keeps its characters' names regardless of when they last
/// played); mtime proximity within a profile is a recency corroborator. Confirmed
/// characters and full accounts are excluded.
pub fn rank(
    files: &[FileMeta],
    user_texts: &HashMap<u64, Vec<String>>,
    names: &HashMap<u64, String>,
    store: &AccountsStore,
) -> Vec<Suggestion> {
    let confirmed: HashSet<u64> =
        store.accounts.values().flat_map(|a| a.characters.iter().copied()).collect();
    let full: HashSet<u64> = store
        .accounts
        .iter()
        .filter(|(_, a)| a.characters.len() >= MAX_CHARS_PER_ACCOUNT)
        .map(|(&u, _)| u)
        .collect();

    let chars = files.iter().filter(|f| f.kind == Kind::Char);
    let users: Vec<&FileMeta> = files.iter().filter(|f| f.kind == Kind::User).collect();

    let mut out = Vec::new();
    for c in chars {
        if confirmed.contains(&c.id) {
            continue;
        }
        let name = names.get(&c.id).map(|n| n.to_lowercase());
        for u in &users {
            if full.contains(&u.id) {
                continue;
            }
            let name_match = match &name {
                Some(n) => user_texts
                    .get(&u.id)
                    .is_some_and(|texts| texts.iter().any(|t| t.to_lowercase().contains(n.as_str()))),
                None => false,
            };
            let mtime_match =
                c.profile == u.profile && c.mtime.abs_diff(u.mtime) <= MTIME_WINDOW_SECS;
            let (confidence, basis) = match (name_match, mtime_match) {
                (true, true) => (Confidence::High, "name match + recent logout"),
                (true, false) => (Confidence::Medium, "name match"),
                (false, true) => (Confidence::Low, "recent logout"),
                (false, false) => continue,
            };
            out.push(Suggestion { char_id: c.id, user_id: u.id, confidence, basis: basis.into() });
        }
    }
    out.sort_by(|a, b| (a.char_id, a.user_id).cmp(&(b.char_id, b.user_id)));
    out
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
    fn confirm_rejects_reassignment_to_a_full_account_and_keeps_the_original() {
        let mut s = AccountsStore::default();
        confirm(&mut s, 90000001, 222).unwrap(); // starts on 222
        for c in [1u64, 2, 3] {
            confirm(&mut s, c, 111).unwrap(); // fill 111 to the cap
        }
        assert!(confirm(&mut s, 90000001, 111).is_err());
        assert_eq!(s.accounts[&222].characters, vec![90000001], "not stripped from its account");
        assert_eq!(s.accounts[&111].characters, vec![1, 2, 3], "111 untouched by the rejected reassignment");
    }

    #[test]
    fn unpair_removes_from_whichever_account_holds_it() {
        let mut s = AccountsStore::default();
        confirm(&mut s, 90000001, 111).unwrap();
        unpair(&mut s, 90000001);
        assert!(s.accounts[&111].characters.is_empty());
        unpair(&mut s, 90000001); // no-op, no panic
    }

    #[test]
    fn collect_strings_gathers_text_and_printable_bytes_recursively() {
        let v = Value::Dict(vec![
            (Value::Bytes(b"charName".to_vec()), Value::Str("Jita Trader".into())),
            (Value::Bytes(b"ucs".to_vec()), Value::StrUcs2("Amarr Alt".into())),
            (
                Value::Bytes(b"nested".to_vec()),
                Value::List(vec![Value::Tuple(vec![Value::Str("Deep Name".into())])]),
            ),
            (Value::Bytes(b"binary".to_vec()), Value::Bytes(vec![0x00, 0xFF])), // non-printable, skipped
        ]);
        let got = collect_strings(&v);
        for want in ["Jita Trader", "Amarr Alt", "Deep Name", "charName", "ucs", "nested", "binary"] {
            assert!(got.contains(&want.to_string()), "missing {want:?} in {got:?}");
        }
        assert!(!got.iter().any(|s| s.contains('\u{0}')), "non-printable bytes excluded");
    }

    #[test]
    fn collect_strings_recurses_all_container_variants_and_skips_empty_bytes() {
        let v = Value::List(vec![
            Value::Stream(Box::new(Value::Str("in_stream".into()))),
            Value::Shared { slot: 1, value: Box::new(Value::Str("in_shared".into())) },
            Value::Instance {
                class: Box::new(Value::Str("in_class".into())),
                state: Box::new(Value::Str("in_state".into())),
            },
            Value::Reduce {
                ctor: Box::new(Value::Str("in_ctor".into())),
                items: vec![Value::Str("in_item".into())],
                pairs: vec![(Value::Str("pk".into()), Value::Str("pv".into()))],
            },
            Value::Bytes(vec![]),
        ]);
        let got = collect_strings(&v);
        for want in
            ["in_stream", "in_shared", "in_class", "in_state", "in_ctor", "in_item", "pk", "pv"]
        {
            assert!(got.contains(&want.to_string()), "missing {want:?} in {got:?}");
        }
        assert!(!got.iter().any(|s| s.is_empty()), "empty bytes contributed nothing: {got:?}");
    }

    fn char(id: u64, mtime: u64, profile: &str) -> FileMeta {
        FileMeta { id, kind: Kind::Char, mtime, profile: PathBuf::from(profile) }
    }
    fn user(id: u64, mtime: u64, profile: &str) -> FileMeta {
        FileMeta { id, kind: Kind::User, mtime, profile: PathBuf::from(profile) }
    }

    #[test]
    fn rank_name_and_mtime_agree_is_high() {
        let files = vec![char(90000001, 1000, "p"), user(987654, 1005, "p")];
        let texts = HashMap::from([(987654u64, vec!["capsule Jita Trader window".to_string()])]);
        let names = HashMap::from([(90000001u64, "Jita Trader".to_string())]);
        let s = rank(&files, &texts, &names, &AccountsStore::default());
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].char_id, 90000001);
        assert_eq!(s[0].user_id, 987654);
        assert_eq!(s[0].confidence, Confidence::High);
    }

    #[test]
    fn rank_name_only_is_medium_across_profiles() {
        // mtimes far apart AND different profiles → no mtime signal.
        let files = vec![char(90000001, 1000, "p1"), user(987654, 99999, "p2")];
        let texts = HashMap::from([(987654u64, vec!["Jita Trader".to_string()])]);
        let names = HashMap::from([(90000001u64, "Jita Trader".to_string())]);
        let s = rank(&files, &texts, &names, &AccountsStore::default());
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].confidence, Confidence::Medium);
    }

    #[test]
    fn rank_mtime_only_is_low() {
        let files = vec![char(90000001, 1000, "p"), user(987654, 1003, "p")];
        let s = rank(&files, &HashMap::new(), &HashMap::new(), &AccountsStore::default());
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].confidence, Confidence::Low);
    }

    #[test]
    fn rank_excludes_confirmed_characters_and_full_accounts() {
        let mut store = AccountsStore::default();
        confirm(&mut store, 90000001, 987654).unwrap(); // already assigned
        let files = vec![char(90000001, 1000, "p"), user(987654, 1002, "p")];
        // No suggestion for an already-confirmed char.
        assert!(rank(&files, &HashMap::new(), &HashMap::new(), &store).is_empty());
    }

    #[test]
    fn rank_excludes_full_accounts() {
        let mut store = AccountsStore::default();
        for c in [1u64, 2, 3] {
            confirm(&mut store, c, 987654).unwrap(); // fill 987654 to the cap
        }
        // An unconfirmed char with a matching mtime signal to the now-full account.
        let files = vec![char(90000009, 1000, "p"), user(987654, 1005, "p")];
        assert!(
            rank(&files, &HashMap::new(), &HashMap::new(), &store).is_empty(),
            "full account suppresses the otherwise-valid mtime suggestion"
        );
    }

    #[test]
    fn rank_mtime_requires_same_profile() {
        // mtime diff is 3s (within the 10s window) but profiles differ, and there's no name signal.
        let files = vec![char(90000001, 1000, "p1"), user(987654, 1003, "p2")];
        assert!(
            rank(&files, &HashMap::new(), &HashMap::new(), &AccountsStore::default()).is_empty(),
            "same-profile check, not just the time window, gates the mtime match"
        );
    }

    #[test]
    fn rank_no_signal_yields_nothing() {
        let files = vec![char(90000001, 1000, "p1"), user(987654, 99999, "p2")];
        assert!(rank(&files, &HashMap::new(), &HashMap::new(), &AccountsStore::default()).is_empty());
    }
}
