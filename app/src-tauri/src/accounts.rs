//! Character↔account association: a persisted store (aliases + confirmed
//! character membership), roster assembly, and the guided-capture diff. All
//! logic is pure and FS-free behind injected
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

use std::collections::HashSet;

/// Only char/user files participate in correlation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Char,
    User,
}

/// A discovered char/user settings file reduced to what the roster and the
/// capture diff need.
#[derive(Debug, Clone)]
pub struct FileMeta {
    pub id: u64,
    pub kind: Kind,
    pub mtime: u64,
}

use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AccountRoster {
    pub accounts: Vec<AccountView>,
    pub unassigned: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AccountView {
    pub user_id: u64,
    pub alias: Option<String>,
    pub characters: Vec<u64>,
}

/// Assemble the roster the UI renders: one account per user id (discovered ∪
/// persisted), with its alias and confirmed characters, plus the discovered
/// characters no account claims. Returns ids only; the frontend maps them to
/// names via the M3a store.
pub fn build_roster(files: &[FileMeta], store: &AccountsStore) -> AccountRoster {
    let mut user_ids: BTreeSet<u64> =
        files.iter().filter(|f| f.kind == Kind::User).map(|f| f.id).collect();
    user_ids.extend(store.accounts.keys().copied());

    let confirmed: HashSet<u64> =
        store.accounts.values().flat_map(|a| a.characters.iter().copied()).collect();

    let accounts = user_ids
        .iter()
        .map(|&user_id| {
            let acct = store.accounts.get(&user_id);
            let characters = acct.map(|a| a.characters.clone()).unwrap_or_default();
            let alias = acct.and_then(|a| a.alias.clone());
            AccountView { user_id, alias, characters }
        })
        .collect();

    let mut unassigned: Vec<u64> = files
        .iter()
        .filter(|f| f.kind == Kind::Char && !confirmed.contains(&f.id))
        .map(|f| f.id)
        .collect();
    unassigned.sort_unstable();
    unassigned.dedup();

    AccountRoster { accounts, unassigned }
}

/// A point-in-time view of discovered char/user files keyed by path.
pub type Snapshot = HashMap<PathBuf, FileMeta>;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaptureResult {
    pub changed_chars: Vec<u64>,
    pub changed_users: Vec<u64>,
    /// `(char_id, user_id)` when exactly one char file and one user file
    /// advanced — the clean, confirmable pairing.
    pub detected: Option<(u64, u64)>,
}

/// Which char/user files advanced (or appeared) between two snapshots. A single
/// char + single user change yields a `detected` pairing; anything else is left
/// for the user to disambiguate.
pub fn capture_diff(baseline: &Snapshot, after: &Snapshot) -> CaptureResult {
    let mut changed_chars = Vec::new();
    let mut changed_users = Vec::new();
    for (path, meta) in after {
        let advanced = match baseline.get(path) {
            Some(old) => meta.mtime > old.mtime,
            None => true, // appeared since baseline
        };
        if !advanced {
            continue;
        }
        match meta.kind {
            Kind::Char => changed_chars.push(meta.id),
            Kind::User => changed_users.push(meta.id),
        }
    }
    changed_chars.sort_unstable();
    changed_chars.dedup();
    changed_users.sort_unstable();
    changed_users.dedup();
    let detected = match (changed_chars.as_slice(), changed_users.as_slice()) {
        ([c], [u]) => Some((*c, *u)),
        _ => None,
    };
    CaptureResult { changed_chars, changed_users, detected }
}

use settings_model::FileKind;

/// A point-in-time snapshot of discovered char/user files (those with an id),
/// optionally excluding one path (the currently-open document, which the app
/// itself might write during a capture).
pub fn snapshot_from_profiles(
    profiles: &[settings_model::Profile],
    exclude: Option<&Path>,
) -> Snapshot {
    let mut snap = Snapshot::new();
    for p in profiles {
        for f in &p.files {
            let (Some(id), Some(mtime)) = (f.id, f.modified_unix) else { continue };
            if exclude == Some(f.path.as_path()) {
                continue;
            }
            let kind = match f.kind {
                FileKind::Char => Kind::Char,
                FileKind::User => Kind::User,
                FileKind::Other => continue,
            };
            snap.insert(f.path.clone(), FileMeta { id, kind, mtime });
        }
    }
    snap
}

fn files_from_profiles(profiles: &[settings_model::Profile]) -> Vec<FileMeta> {
    snapshot_from_profiles(profiles, None).into_values().collect()
}

/// Build the roster: discover files + load the store. (Passive char↔account
/// suggestions were removed after the M3b live smoke — parsing every user file
/// on each load froze the UI, and manual pairing + guided capture cover the
/// need.)
pub fn load_roster(roots: &[PathBuf], dir: &Path) -> AccountRoster {
    let profiles = settings_model::discover(roots);
    let files = files_from_profiles(&profiles);
    let store = load_store(dir);
    build_roster(&files, &store)
}

// ponytail: each mutation reloads the whole roster (re-discovers + re-parses
// user files). Fine for a handful of local files and user-initiated edits; if
// it ever drags, cache the parsed texts in AppState.
pub fn set_account_alias(
    roots: &[PathBuf],
    dir: &Path,
    user_id: u64,
    alias: Option<String>,
) -> AccountRoster {
    let mut store = load_store(dir);
    set_alias(&mut store, user_id, alias);
    let _ = save_store(dir, &store);
    load_roster(roots, dir)
}

pub fn confirm_pairing(
    roots: &[PathBuf],
    dir: &Path,
    char_id: u64,
    user_id: u64,
) -> Result<AccountRoster, String> {
    let mut store = load_store(dir);
    confirm(&mut store, char_id, user_id)?;
    let _ = save_store(dir, &store);
    Ok(load_roster(roots, dir))
}

pub fn unpair_character(roots: &[PathBuf], dir: &Path, char_id: u64) -> AccountRoster {
    let mut store = load_store(dir);
    unpair(&mut store, char_id);
    let _ = save_store(dir, &store);
    load_roster(roots, dir)
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

    fn char(id: u64, mtime: u64, _profile: &str) -> FileMeta {
        FileMeta { id, kind: Kind::Char, mtime }
    }
    fn user(id: u64, mtime: u64, _profile: &str) -> FileMeta {
        FileMeta { id, kind: Kind::User, mtime }
    }

    #[test]
    fn build_roster_unions_discovered_and_stored_accounts() {
        let mut store = AccountsStore::default();
        confirm(&mut store, 90000001, 987654).unwrap();
        set_alias(&mut store, 987654, Some("Main".into()));
        // Discovery sees account 987654 plus a bare user 555 and an unassigned char 90000002.
        let files = vec![
            user(987654, 10, "p"),
            user(555, 10, "p"),
            char(90000001, 10, "p"),
            char(90000002, 10, "p"),
        ];
        let r = build_roster(&files, &store);

        let acct: Vec<u64> = r.accounts.iter().map(|a| a.user_id).collect();
        assert_eq!(acct, vec![555, 987654], "accounts sorted, union of stored + discovered");
        let main = r.accounts.iter().find(|a| a.user_id == 987654).unwrap();
        assert_eq!(main.alias.as_deref(), Some("Main"));
        assert_eq!(main.characters, vec![90000001]);
        assert_eq!(r.unassigned, vec![90000002], "confirmed char is not unassigned");
    }

    #[test]
    fn build_roster_includes_store_only_account_with_no_discovered_file() {
        // Account 42 lives only in the store (e.g. discovered on a prior run,
        // that file absent this time); it must still appear in the roster.
        let mut store = AccountsStore::default();
        confirm(&mut store, 90000001, 42).unwrap();
        set_alias(&mut store, 42, Some("Alt".into()));
        let files: Vec<FileMeta> = vec![];
        let r = build_roster(&files, &store);
        assert_eq!(r.accounts.len(), 1);
        let acct = &r.accounts[0];
        assert_eq!(acct.user_id, 42);
        assert_eq!(acct.alias.as_deref(), Some("Alt"));
        assert_eq!(acct.characters, vec![90000001]);
    }

    #[test]
    fn build_roster_discovered_account_with_no_store_entry_has_empty_characters() {
        let files = vec![user(555, 10, "p")];
        let r = build_roster(&files, &AccountsStore::default());
        let acct = r.accounts.iter().find(|a| a.user_id == 555).unwrap();
        assert!(acct.characters.is_empty(), "no store entry means no confirmed characters");
        assert!(acct.alias.is_none());
    }

    fn snap(entries: &[(&str, FileMeta)]) -> Snapshot {
        entries.iter().map(|(p, m)| (PathBuf::from(p), m.clone())).collect()
    }

    #[test]
    fn capture_diff_detects_the_single_changed_pair() {
        let base = snap(&[
            ("a.dat", char(90000001, 100, "p")),
            ("u.dat", user(987654, 100, "p")),
        ]);
        let after = snap(&[
            ("a.dat", char(90000001, 200, "p")), // char advanced
            ("u.dat", user(987654, 200, "p")),   // user advanced
        ]);
        let r = capture_diff(&base, &after);
        assert_eq!(r.detected, Some((90000001, 987654)));
    }

    #[test]
    fn capture_diff_user_only_is_not_detected() {
        let base = snap(&[("u.dat", user(987654, 100, "p"))]);
        let after = snap(&[("u.dat", user(987654, 200, "p"))]);
        let r = capture_diff(&base, &after);
        assert_eq!(r.changed_users, vec![987654]);
        assert!(r.changed_chars.is_empty());
        assert_eq!(r.detected, None);
    }

    #[test]
    fn capture_diff_char_only_is_not_detected() {
        let base = snap(&[("a.dat", char(90000001, 100, "p"))]);
        let after = snap(&[("a.dat", char(90000001, 200, "p"))]);
        let r = capture_diff(&base, &after);
        assert_eq!(r.changed_chars, vec![90000001]);
        assert!(r.changed_users.is_empty());
        assert_eq!(r.detected, None);
    }

    #[test]
    fn capture_diff_new_file_counts_as_changed() {
        let base = snap(&[]);
        let after = snap(&[("u.dat", user(987654, 200, "p")), ("a.dat", char(90000001, 200, "p"))]);
        assert_eq!(capture_diff(&base, &after).detected, Some((90000001, 987654)));
    }

    #[test]
    fn capture_diff_multiple_users_is_ambiguous() {
        let base = snap(&[("u1.dat", user(111, 100, "p")), ("u2.dat", user(222, 100, "p"))]);
        let after = snap(&[("u1.dat", user(111, 200, "p")), ("u2.dat", user(222, 200, "p"))]);
        let r = capture_diff(&base, &after);
        assert_eq!(r.detected, None);
        assert_eq!(r.changed_users, vec![111, 222]);
    }

    #[test]
    fn capture_diff_nothing_changed_yields_empty_result() {
        let base = snap(&[
            ("a.dat", char(90000001, 100, "p")),
            ("u.dat", user(987654, 100, "p")),
        ]);
        let after = base.clone();
        let r = capture_diff(&base, &after);
        assert!(r.changed_chars.is_empty());
        assert!(r.changed_users.is_empty());
        assert_eq!(r.detected, None);
    }

    #[test]
    fn capture_diff_unchanged_mtime_is_not_reported() {
        let base = snap(&[
            ("a.dat", char(90000001, 100, "p")), // stays put
            ("u.dat", user(987654, 100, "p")),   // advances
        ]);
        let after = snap(&[
            ("a.dat", char(90000001, 100, "p")), // same mtime, not reported
            ("u.dat", user(987654, 200, "p")),
        ]);
        let r = capture_diff(&base, &after);
        assert!(r.changed_chars.is_empty(), "unchanged mtime not reported");
        assert_eq!(r.changed_users, vec![987654]);
    }

    use blue_marshal::{encode, Value};

    #[test]
    fn load_roster_end_to_end_from_a_temp_tree() {
        // Discovery root: <root>/<install>_<server>/settings_Default/core_(char|user)_<id>.dat
        let root = temp_dir("roster-tree");
        let sdir = root.join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        fs::create_dir_all(&sdir).unwrap();
        fs::write(sdir.join("core_user_987654.dat"), encode(&Value::Int(1)).unwrap()).unwrap();
        fs::write(sdir.join("core_char_90000001.dat"), encode(&Value::Int(1)).unwrap()).unwrap();
        let appdir = temp_dir("roster-appdata");

        let roster = load_roster(&[root], &appdir);
        assert!(roster.accounts.iter().any(|a| a.user_id == 987654));
        assert_eq!(roster.unassigned, vec![90000001]);
    }

    #[test]
    fn confirm_pairing_persists_and_reflects_in_the_roster() {
        let root = temp_dir("confirm-tree");
        let sdir = root.join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        fs::create_dir_all(&sdir).unwrap();
        fs::write(sdir.join("core_user_987654.dat"), encode(&Value::Int(1)).unwrap()).unwrap();
        fs::write(sdir.join("core_char_90000001.dat"), encode(&Value::Int(1)).unwrap()).unwrap();
        let appdir = temp_dir("confirm-appdata");

        let roster = confirm_pairing(&[root.clone()], &appdir, 90000001, 987654).unwrap();
        let acct = roster.accounts.iter().find(|a| a.user_id == 987654).unwrap();
        assert_eq!(acct.characters, vec![90000001]);
        assert!(roster.unassigned.is_empty());
        // Persisted across a reload.
        assert_eq!(load_store(&appdir).accounts[&987654].characters, vec![90000001]);
    }

    #[test]
    fn set_account_alias_persists_and_appears_on_the_roster() {
        let root = temp_dir("alias-tree");
        let sdir = root.join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        fs::create_dir_all(&sdir).unwrap();
        fs::write(sdir.join("core_user_987654.dat"), encode(&Value::Int(1)).unwrap()).unwrap();
        let appdir = temp_dir("alias-appdata");

        let roster = set_account_alias(&[root.clone()], &appdir, 987654, Some("Main".into()));
        let acct = roster.accounts.iter().find(|a| a.user_id == 987654).unwrap();
        assert_eq!(acct.alias.as_deref(), Some("Main"));
        // Persisted across a reload.
        assert_eq!(load_store(&appdir).accounts[&987654].alias.as_deref(), Some("Main"));
    }

    #[test]
    fn unpair_character_removes_it_and_returns_it_to_unassigned() {
        let root = temp_dir("unpair-tree");
        let sdir = root.join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        fs::create_dir_all(&sdir).unwrap();
        fs::write(sdir.join("core_user_987654.dat"), encode(&Value::Int(1)).unwrap()).unwrap();
        fs::write(sdir.join("core_char_90000001.dat"), encode(&Value::Int(1)).unwrap()).unwrap();
        let appdir = temp_dir("unpair-appdata");

        confirm_pairing(&[root.clone()], &appdir, 90000001, 987654).unwrap();
        let roster = unpair_character(&[root.clone()], &appdir, 90000001);
        let acct = roster.accounts.iter().find(|a| a.user_id == 987654).unwrap();
        assert!(acct.characters.is_empty(), "char removed from the account");
        assert_eq!(roster.unassigned, vec![90000001], "char back in unassigned");
        // Persisted across a reload.
        assert!(load_store(&appdir).accounts[&987654].characters.is_empty());
    }

    #[test]
    fn snapshot_from_profiles_exclude_omits_exactly_that_file() {
        let root = temp_dir("snapshot-tree");
        let sdir = root.join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        fs::create_dir_all(&sdir).unwrap();
        let user_path = sdir.join("core_user_987654.dat");
        let char_path = sdir.join("core_char_90000001.dat");
        fs::write(&user_path, encode(&Value::Int(1)).unwrap()).unwrap();
        fs::write(&char_path, encode(&Value::Int(1)).unwrap()).unwrap();

        let profiles = settings_model::discover(&[root]);
        let snap = snapshot_from_profiles(&profiles, Some(user_path.as_path()));
        assert_eq!(snap.len(), 1, "only the excluded file is omitted");
        assert!(!snap.contains_key(&user_path), "excluded file omitted");
        assert!(snap.contains_key(&char_path), "other file still present");
    }
}
