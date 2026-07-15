# M3b — Character/user association Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user name EVE accounts (`core_user_*.dat`) and associate characters with their account — via passive correlation suggestions, manual assignment, and a guided controlled-logout capture — persisting the char↔user pairing the overview editor will consume.

**Architecture:** A new Rust `accounts.rs` in the `app` crate owns `accounts.json` (global-id-keyed aliases + confirmed char membership) and all correlation logic as pure, FS-free functions (suggestion ranking, roster assembly, capture diff) behind thin impure orchestrators — mirroring the M3a `names.rs` split. Tauri commands are one-line delegators. The frontend gets a shared `accounts.svelte.ts` store feeding a new dedicated **Accounts view** plus alias display in the sidebar and open-file header.

**Tech Stack:** Rust (`app` crate, `serde`/`serde_json`, `blue-marshal`, `settings-model`), Tauri v2 commands, SvelteKit 5 (runes), `node --test` for pure frontend helpers.

## Global Constraints

- **Repo rule — no real ids in fixtures/tests.** Use synthetic ids only (e.g. `90000001`, `987654`). Copied verbatim from every existing test module.
- **Dependency-free core.** `blue-marshal` and `settings-model` gain no new dependencies. New code lives in the `app` crate only.
- **Silent-by-default local behavior.** Missing/corrupt `accounts.json` → empty store, never an error (mirrors the M3a names cache). No network in this milestone.
- **`MAX_CHARS_PER_ACCOUNT = 3`** — a hard cap (EVE has always allowed at most 3 characters per account). Enforced in Rust; a 4th confirm is rejected.
- **Commits:** sentence-case subject, **no** attribution trailers (repo convention).
- **Test runners:** Rust `cargo test` (run from `app/src-tauri`); frontend `npm test` (from `app/`, zero-dep `node --test`). `cargo`/`npm`/`gh` are reached via the **PowerShell** tool, not Bash.
- **Svelte-5 runes** — new stores are `.svelte.ts` rune modules using `$state`, matching `app/src/lib/names.svelte.ts`.

## File Structure

- Create `app/src-tauri/src/accounts.rs` — store, persistence, and all correlation logic (pure) + impure orchestrators.
- Modify `app/src-tauri/src/ops.rs` — `AppState` gains a capture-baseline slot; `begin_capture`/`resolve_capture`.
- Modify `app/src-tauri/src/lib.rs` — `mod accounts;`, the six new commands, `generate_handler!`.
- Modify `app/src-tauri/Cargo.toml` — promote `blue-marshal` to `[dependencies]`.
- Modify `app/src/lib/api.ts` — typed mirror of the new commands.
- Create `app/src/lib/accounts.svelte.ts` — shared roster store + action helpers.
- Create `app/src/lib/AccountsView.svelte` — the dedicated Accounts view (cards, slots, unassigned, alias edit, suggestions, guided capture).
- Modify `app/src/lib/Sidebar.svelte` — alias for user-file rows + an "Accounts" entry point.
- Modify `app/src/routes/+page.svelte` — `mainView` switch; render `AccountsView`; open-file-header alias.

---

### Task 1: Accounts store + persistence (`accounts.rs`)

**Files:**
- Create: `app/src-tauri/src/accounts.rs`
- Modify: `app/src-tauri/src/lib.rs:1` (add `mod accounts;`)

**Interfaces:**
- Produces: `pub struct AccountsStore { pub accounts: HashMap<u64, Account> }`, `pub struct Account { pub alias: Option<String>, pub characters: Vec<u64> }`, `pub fn load_store(dir: &Path) -> AccountsStore`, `pub const MAX_CHARS_PER_ACCOUNT: usize = 3`.

- [ ] **Step 1: Declare the module.** Add to the top of `app/src-tauri/src/lib.rs` (it currently starts with `mod names;` / `mod ops;`):

```rust
mod accounts;
mod names;
mod ops;
```

- [ ] **Step 2: Write the failing tests.** Create `app/src-tauri/src/accounts.rs`:

```rust
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
```

- [ ] **Step 3: Run tests to verify they pass.**

Run (PowerShell): `cd app/src-tauri; cargo test --lib accounts::`
Expected: PASS (3 tests). `mod accounts;` compiles; `save_store` is exercised only by tests, so no dead-code warning under test cfg.

- [ ] **Step 4: Commit.**

```bash
git add app/src-tauri/src/accounts.rs app/src-tauri/src/lib.rs
git commit -m "Add the accounts store and its on-disk persistence"
```

---

### Task 2: Store mutators — alias, confirm, unpair (`accounts.rs`)

**Files:**
- Modify: `app/src-tauri/src/accounts.rs`

**Interfaces:**
- Consumes: `AccountsStore`, `Account`, `MAX_CHARS_PER_ACCOUNT` (Task 1).
- Produces: `pub fn set_alias(store: &mut AccountsStore, user_id: u64, alias: Option<String>)`, `pub fn confirm(store: &mut AccountsStore, char_id: u64, user_id: u64) -> Result<(), String>`, `pub fn unpair(store: &mut AccountsStore, char_id: u64)`.

- [ ] **Step 1: Write the failing tests.** Add to the `tests` module in `accounts.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail.**

Run: `cd app/src-tauri; cargo test --lib accounts::`
Expected: FAIL — `set_alias`, `confirm`, `unpair` not found.

- [ ] **Step 3: Implement the mutators.** Add above the `tests` module in `accounts.rs`:

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass.**

Run: `cd app/src-tauri; cargo test --lib accounts::`
Expected: PASS (all Task 1 + Task 2 tests).

- [ ] **Step 5: Commit.**

```bash
git add app/src-tauri/src/accounts.rs
git commit -m "Add alias, confirm and unpair mutators with the single-membership and 3-char rules"
```

---

### Task 3: In-file string collection (`accounts.rs`)

**Files:**
- Modify: `app/src-tauri/Cargo.toml:20-27` (promote `blue-marshal` to `[dependencies]`)
- Modify: `app/src-tauri/src/accounts.rs`

**Interfaces:**
- Produces: `pub fn collect_strings(v: &blue_marshal::Value) -> Vec<String>`.

- [ ] **Step 1: Promote `blue-marshal` to a runtime dependency.** In `app/src-tauri/Cargo.toml`, add to `[dependencies]` (it is currently only under `[dev-dependencies]`):

```toml
blue-marshal = { path = "../../crates/blue-marshal" }
```

Leave the existing `[dev-dependencies]` entry as-is (cargo dedups; test code keeps working).

- [ ] **Step 2: Write the failing test.** Add to the `tests` module in `accounts.rs`:

```rust
    use blue_marshal::Value;

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
```

- [ ] **Step 3: Run test to verify it fails.**

Run: `cd app/src-tauri; cargo test --lib accounts::collect_strings`
Expected: FAIL — `collect_strings` not found.

- [ ] **Step 4: Implement the walk.** Add to `accounts.rs` (above `tests`):

```rust
use blue_marshal::Value;

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
```

(The `use blue_marshal::Value;` at module scope replaces the test-local `use` — remove the `use blue_marshal::Value;` line inside the `tests` module if it now shadows/duplicates; keeping both compiles but Clippy warns. Prefer the single module-level `use`.)

- [ ] **Step 5: Run test to verify it passes.**

Run: `cd app/src-tauri; cargo test --lib accounts::`
Expected: PASS.

- [ ] **Step 6: Commit.**

```bash
git add app/src-tauri/Cargo.toml app/src-tauri/src/accounts.rs
git commit -m "Collect character-name text leaves from a decoded user file"
```

---

### Task 4: Suggestion ranking (`accounts.rs`)

**Files:**
- Modify: `app/src-tauri/src/accounts.rs`

**Interfaces:**
- Consumes: `AccountsStore`, `MAX_CHARS_PER_ACCOUNT`.
- Produces: `pub enum Kind { Char, User }`, `pub struct FileMeta { pub id: u64, pub kind: Kind, pub mtime: u64, pub profile: PathBuf }`, `pub enum Confidence { High, Medium, Low }`, `pub struct Suggestion { pub char_id: u64, pub user_id: u64, pub confidence: Confidence, pub basis: String }`, `pub fn rank(files: &[FileMeta], user_texts: &HashMap<u64, Vec<String>>, names: &HashMap<u64, String>, store: &AccountsStore) -> Vec<Suggestion>`.

- [ ] **Step 1: Write the failing tests.** Add to the `tests` module:

```rust
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
    fn rank_no_signal_yields_nothing() {
        let files = vec![char(90000001, 1000, "p1"), user(987654, 99999, "p2")];
        assert!(rank(&files, &HashMap::new(), &HashMap::new(), &AccountsStore::default()).is_empty());
    }
```

- [ ] **Step 2: Run tests to verify they fail.**

Run: `cd app/src-tauri; cargo test --lib accounts::rank`
Expected: FAIL — `rank`, `FileMeta`, `Kind`, `Confidence`, `Suggestion` not found.

- [ ] **Step 3: Implement the engine.** Add to `accounts.rs` (above `tests`):

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass.**

Run: `cd app/src-tauri; cargo test --lib accounts::`
Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add app/src-tauri/src/accounts.rs
git commit -m "Rank char/account correlation suggestions from name matches and mtime proximity"
```

---

### Task 5: Roster assembly (`accounts.rs`)

**Files:**
- Modify: `app/src-tauri/src/accounts.rs`

**Interfaces:**
- Consumes: `FileMeta`, `Kind`, `AccountsStore`, `Suggestion`, `MAX_CHARS_PER_ACCOUNT`.
- Produces: `pub struct AccountRoster { pub accounts: Vec<AccountView>, pub unassigned: Vec<u64> }`, `pub struct AccountView { pub user_id: u64, pub alias: Option<String>, pub characters: Vec<u64>, pub suggestions: Vec<Suggestion> }`, `pub fn build_roster(files: &[FileMeta], store: &AccountsStore, suggestions: &[Suggestion]) -> AccountRoster`.

- [ ] **Step 1: Write the failing tests.** Add to the `tests` module:

```rust
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
        let sugg = vec![Suggestion {
            char_id: 90000002,
            user_id: 555,
            confidence: Confidence::Low,
            basis: "recent logout".into(),
        }];
        let r = build_roster(&files, &store, &sugg);

        let acct: Vec<u64> = r.accounts.iter().map(|a| a.user_id).collect();
        assert_eq!(acct, vec![555, 987654], "accounts sorted, union of stored + discovered");
        let main = r.accounts.iter().find(|a| a.user_id == 987654).unwrap();
        assert_eq!(main.alias.as_deref(), Some("Main"));
        assert_eq!(main.characters, vec![90000001]);
        let other = r.accounts.iter().find(|a| a.user_id == 555).unwrap();
        assert_eq!(other.suggestions.len(), 1, "suggestion attached to the empty account");
        assert_eq!(r.unassigned, vec![90000002], "confirmed char is not unassigned");
    }

    #[test]
    fn build_roster_drops_suggestions_for_full_accounts() {
        let mut store = AccountsStore::default();
        for c in [1u64, 2, 3] {
            confirm(&mut store, c, 987654).unwrap();
        }
        let files = vec![user(987654, 10, "p"), char(90000009, 10, "p")];
        let sugg = vec![Suggestion {
            char_id: 90000009,
            user_id: 987654,
            confidence: Confidence::Low,
            basis: "recent logout".into(),
        }];
        let r = build_roster(&files, &store, &sugg);
        let full = &r.accounts.iter().find(|a| a.user_id == 987654).unwrap();
        assert!(full.suggestions.is_empty(), "no suggestions once the 3 slots are full");
    }
```

- [ ] **Step 2: Run tests to verify they fail.**

Run: `cd app/src-tauri; cargo test --lib accounts::build_roster`
Expected: FAIL — `build_roster`, `AccountRoster`, `AccountView` not found.

- [ ] **Step 3: Implement roster assembly.** Add to `accounts.rs` (above `tests`):

```rust
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
    pub suggestions: Vec<Suggestion>,
}

/// Assemble the roster the UI renders: one account per user id (discovered ∪
/// persisted), its alias and confirmed characters, and — only while it has an
/// empty slot — its suggestions; plus the discovered characters no account
/// claims. Returns ids only; the frontend maps them to names via the M3a store.
pub fn build_roster(
    files: &[FileMeta],
    store: &AccountsStore,
    suggestions: &[Suggestion],
) -> AccountRoster {
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
            let has_room = characters.len() < MAX_CHARS_PER_ACCOUNT;
            let suggestions = if has_room {
                suggestions
                    .iter()
                    .filter(|s| s.user_id == user_id && !confirmed.contains(&s.char_id))
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            };
            AccountView { user_id, alias, characters, suggestions }
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
```

- [ ] **Step 4: Run tests to verify they pass.**

Run: `cd app/src-tauri; cargo test --lib accounts::`
Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add app/src-tauri/src/accounts.rs
git commit -m "Assemble the account roster from the store, discovered files and suggestions"
```

---

### Task 6: Capture diff (`accounts.rs`)

**Files:**
- Modify: `app/src-tauri/src/accounts.rs`

**Interfaces:**
- Consumes: `FileMeta`, `Kind`.
- Produces: `pub type Snapshot = HashMap<PathBuf, FileMeta>`, `pub struct CaptureResult { pub changed_chars: Vec<u64>, pub changed_users: Vec<u64>, pub detected: Option<(u64, u64)> }`, `pub fn capture_diff(baseline: &Snapshot, after: &Snapshot) -> CaptureResult`.

- [ ] **Step 1: Write the failing tests.** Add to the `tests` module:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail.**

Run: `cd app/src-tauri; cargo test --lib accounts::capture_diff`
Expected: FAIL — `capture_diff`, `Snapshot`, `CaptureResult` not found.

- [ ] **Step 3: Implement the diff.** Add to `accounts.rs` (above `tests`):

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass.**

Run: `cd app/src-tauri; cargo test --lib accounts::`
Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add app/src-tauri/src/accounts.rs
git commit -m "Diff two file snapshots to detect a controlled-logout pairing"
```

---

### Task 7: Orchestration — discovery, scanning, and store-mutating helpers (`accounts.rs`)

**Files:**
- Modify: `app/src-tauri/src/accounts.rs`

**Interfaces:**
- Consumes: all of the above; `settings_model::{discover, Profile, SettingsFile, FileKind, Document}`, `crate::names::load_cache`.
- Produces: `pub fn snapshot_from_profiles(profiles: &[settings_model::Profile], exclude: Option<&Path>) -> Snapshot`, `pub fn load_roster(roots: &[PathBuf], dir: &Path) -> AccountRoster`, `pub fn set_account_alias(roots: &[PathBuf], dir: &Path, user_id: u64, alias: Option<String>) -> AccountRoster`, `pub fn confirm_pairing(roots: &[PathBuf], dir: &Path, char_id: u64, user_id: u64) -> Result<AccountRoster, String>`, `pub fn unpair_character(roots: &[PathBuf], dir: &Path, char_id: u64) -> AccountRoster`.

- [ ] **Step 1: Write the failing integration test.** Add to the `tests` module:

```rust
    use blue_marshal::encode;

    /// Write a real settings tree whose text embeds `name` so the name-scan can find it.
    fn write_user_file(path: &Path, name: &str) {
        let v = Value::Dict(vec![(
            Value::Bytes(b"capsuleWindow".to_vec()),
            Value::Str(format!("cap_{name}_window")),
        )]);
        fs::write(path, encode(&v).unwrap()).unwrap();
    }

    #[test]
    fn load_roster_end_to_end_from_a_temp_tree() {
        // Discovery root: <root>/<install>_<server>/settings_Default/core_(char|user)_<id>.dat
        let root = temp_dir("roster-tree");
        let sdir = root.join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        fs::create_dir_all(&sdir).unwrap();
        write_user_file(&sdir.join("core_user_987654.dat"), "Jita Trader");
        // A minimal char file (contents don't matter for the char side).
        fs::write(sdir.join("core_char_90000001.dat"), encode(&Value::Int(1)).unwrap()).unwrap();

        // Seed the M3a names cache so the char id resolves to the embedded name.
        let appdir = temp_dir("roster-appdata");
        fs::create_dir_all(&appdir).unwrap();
        fs::write(
            appdir.join("names-cache.json"),
            br#"{"90000001":{"name":"Jita Trader","category":"character"}}"#,
        )
        .unwrap();

        let roster = load_roster(&[root], &appdir);
        let acct = roster.accounts.iter().find(|a| a.user_id == 987654).unwrap();
        // Name match present → at least Medium; empty account → suggestion attached.
        assert!(acct.suggestions.iter().any(|s| s.char_id == 90000001));
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
```

- [ ] **Step 2: Run tests to verify they fail.**

Run: `cd app/src-tauri; cargo test --lib accounts::load_roster accounts::confirm_pairing`
Expected: FAIL — orchestrators not found.

- [ ] **Step 3: Implement the orchestrators.** Add to `accounts.rs` (above `tests`):

```rust
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
            snap.insert(f.path.clone(), FileMeta { id, kind, mtime, profile: p.dir.clone() });
        }
    }
    snap
}

fn files_from_profiles(profiles: &[settings_model::Profile]) -> Vec<FileMeta> {
    snapshot_from_profiles(profiles, None).into_values().collect()
}

/// Parse each discovered user file and collect its text leaves, for name
/// matching. A file that fails to parse simply contributes nothing.
fn load_user_texts(profiles: &[settings_model::Profile]) -> HashMap<u64, Vec<String>> {
    let mut out = HashMap::new();
    for p in profiles {
        for f in &p.files {
            if f.kind != FileKind::User {
                continue;
            }
            let Some(id) = f.id else { continue };
            if let Ok(doc) = settings_model::Document::load(&f.path) {
                out.insert(id, collect_strings(&doc.value));
            }
        }
    }
    out
}

/// Character id → name, from the M3a on-disk names cache.
fn names_map(dir: &Path) -> HashMap<u64, String> {
    crate::names::load_cache(dir).into_iter().map(|(id, r)| (id, r.name)).collect()
}

/// Build the full roster: discover, scan user files, load names + store, rank.
pub fn load_roster(roots: &[PathBuf], dir: &Path) -> AccountRoster {
    let profiles = settings_model::discover(roots);
    let files = files_from_profiles(&profiles);
    let user_texts = load_user_texts(&profiles);
    let names = names_map(dir);
    let store = load_store(dir);
    let suggestions = rank(&files, &user_texts, &names, &store);
    build_roster(&files, &store, &suggestions)
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
```

- [ ] **Step 4: Run tests to verify they pass.**

Run: `cd app/src-tauri; cargo test --lib accounts::`
Expected: PASS (all accounts tests).

- [ ] **Step 5: Commit.**

```bash
git add app/src-tauri/src/accounts.rs
git commit -m "Wire discovery, name-scanning and store mutations into roster orchestrators"
```

---

### Task 8: Commands + capture state (`ops.rs`, `lib.rs`, `api.ts`)

**Files:**
- Modify: `app/src-tauri/src/ops.rs:16-22` (AppState) and add `begin_capture`/`resolve_capture`
- Modify: `app/src-tauri/src/lib.rs` (commands + handler)
- Modify: `app/src/lib/api.ts` (typed mirror)

**Interfaces:**
- Consumes: everything in `accounts.rs`; `settings_model::default_roots`.
- Produces (commands): `account_roster`, `set_account_alias`, `confirm_pairing`, `unpair_character`, `begin_capture`, `resolve_capture`. TS: `api.accountRoster/setAccountAlias/confirmPairing/unpairCharacter/beginCapture/resolveCapture` and the `AccountRoster`/`AccountView`/`Suggestion`/`Confidence`/`CaptureResult` types.

- [ ] **Step 1: Add the capture slot to `AppState`.** In `app/src-tauri/src/ops.rs`, change the struct and constructor (currently a one-field tuple struct):

```rust
use crate::accounts;

/// One document open at a time (V1), plus a transient guided-capture baseline.
pub struct AppState(pub Mutex<Option<Document>>, pub Mutex<Option<accounts::Snapshot>>);

impl AppState {
    pub fn new() -> Self {
        AppState(Mutex::new(None), Mutex::new(None))
    }
}
```

(All existing `state.0` document accesses are unchanged; capture uses `state.1`.)

- [ ] **Step 2: Add capture ops.** Append to `ops.rs` (before the `tests` module):

```rust
use std::path::PathBuf;

/// Snapshot current file mtimes as the guided-capture baseline, excluding the
/// currently-open document (the app itself may write it).
pub fn begin_capture(state: &AppState, roots: &[PathBuf]) {
    let open_path = state.0.lock().unwrap().as_ref().map(|d| d.path.clone());
    let profiles = discover(roots);
    let snap = accounts::snapshot_from_profiles(&profiles, open_path.as_deref());
    *state.1.lock().unwrap() = Some(snap);
}

/// Diff the current files against the capture baseline (empty if none set).
pub fn resolve_capture(state: &AppState, roots: &[PathBuf]) -> accounts::CaptureResult {
    let baseline = state.1.lock().unwrap().clone().unwrap_or_default();
    let profiles = discover(roots);
    let after = accounts::snapshot_from_profiles(&profiles, None);
    accounts::capture_diff(&baseline, &after)
}
```

- [ ] **Step 3: Write a failing ops test.** Add to the `tests` module in `ops.rs`:

```rust
    #[test]
    fn capture_detects_a_user_file_touched_after_baseline() {
        // A temp discovery tree with one char + one user file.
        let root = std::env::temp_dir().join(format!("app-cap-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let sdir = root.join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        fs::create_dir_all(&sdir).unwrap();
        let cf = sdir.join("core_char_90000001.dat");
        let uf = sdir.join("core_user_987654.dat");
        fs::write(&cf, b"x").unwrap();
        fs::write(&uf, b"x").unwrap();

        let state = AppState::new();
        begin_capture(&state, &[root.clone()]);
        // Advance both mtimes (rewrite the files a moment later).
        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(&cf, b"xy").unwrap();
        fs::write(&uf, b"xy").unwrap();

        let r = resolve_capture(&state, &[root]);
        assert_eq!(r.detected, Some((90000001, 987654)));
    }
```

- [ ] **Step 4: Run to verify it fails, then (after Step 5) passes.**

Run: `cd app/src-tauri; cargo test --lib ops::capture`
Expected first: FAIL (functions missing until Step 2 compiled) → after Steps 1-2: PASS. (`modified_unix` is whole-seconds, hence the 1.1s sleep.)

- [ ] **Step 5: Add the commands.** In `app/src-tauri/src/lib.rs`, add a dir helper and six commands, and register them. Add near the top (after the existing `use` lines):

```rust
fn app_dir(app: &tauri::AppHandle) -> std::path::PathBuf {
    app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir())
}
```

Add the command functions (alongside the existing ones):

```rust
#[tauri::command]
fn account_roster(app: tauri::AppHandle) -> accounts::AccountRoster {
    accounts::load_roster(&settings_model::default_roots(), &app_dir(&app))
}

#[tauri::command]
fn set_account_alias(
    app: tauri::AppHandle,
    user_id: u64,
    alias: Option<String>,
) -> accounts::AccountRoster {
    accounts::set_account_alias(&settings_model::default_roots(), &app_dir(&app), user_id, alias)
}

#[tauri::command]
fn confirm_pairing(
    app: tauri::AppHandle,
    char_id: u64,
    user_id: u64,
) -> Result<accounts::AccountRoster, ErrDto> {
    accounts::confirm_pairing(&settings_model::default_roots(), &app_dir(&app), char_id, user_id)
        .map_err(|m| ErrDto { code: "cap".into(), message: m })
}

#[tauri::command]
fn unpair_character(app: tauri::AppHandle, char_id: u64) -> accounts::AccountRoster {
    accounts::unpair_character(&settings_model::default_roots(), &app_dir(&app), char_id)
}

#[tauri::command]
fn begin_capture(state: tauri::State<'_, AppState>) {
    ops::begin_capture(&state, &settings_model::default_roots());
}

#[tauri::command]
fn resolve_capture(state: tauri::State<'_, AppState>) -> accounts::CaptureResult {
    ops::resolve_capture(&state, &settings_model::default_roots())
}
```

Extend the `generate_handler!` list (append after `refresh_character_names`):

```rust
            window_layout, resolve_character_names, refresh_character_names,
            account_roster, set_account_alias, confirm_pairing, unpair_character,
            begin_capture, resolve_capture
```

`ErrDto`'s fields are already `pub`, so the struct literal compiles (it is already imported via `use ops::{AppState, ErrDto, OpenOutcome};`).

- [ ] **Step 6: Mirror the commands in `api.ts`.** Add the types (near the other interfaces) and methods (in the `api` object):

```ts
export type Confidence = "high" | "medium" | "low";
export interface Suggestion {
  char_id: number;
  user_id: number;
  confidence: Confidence;
  basis: string;
}
export interface AccountView {
  user_id: number;
  alias: string | null;
  characters: number[];
  suggestions: Suggestion[];
}
export interface AccountRoster {
  accounts: AccountView[];
  unassigned: number[];
}
export interface CaptureResult {
  changed_chars: number[];
  changed_users: number[];
  detected: [number, number] | null;
}
```

```ts
  accountRoster: () => invoke<AccountRoster>("account_roster"),
  setAccountAlias: (userId: number, alias: string | null) =>
    invoke<AccountRoster>("set_account_alias", { userId, alias }),
  confirmPairing: (charId: number, userId: number) =>
    invoke<AccountRoster>("confirm_pairing", { charId, userId }),
  unpairCharacter: (charId: number) =>
    invoke<AccountRoster>("unpair_character", { charId }),
  beginCapture: () => invoke<void>("begin_capture"),
  resolveCapture: () => invoke<CaptureResult>("resolve_capture"),
```

- [ ] **Step 7: Run the full Rust suite + build the frontend.**

Run: `cd app/src-tauri; cargo test`
Expected: PASS (accounts + ops + everything). No dead-code warnings now that the commands consume the module.
Run: `cd app; npm run check` (Svelte/TS typecheck)
Expected: no errors from `api.ts`.

- [ ] **Step 8: Commit.**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Expose account roster, pairing and guided-capture commands"
```

---

### Task 9: Shared roster store (`accounts.svelte.ts`)

**Files:**
- Create: `app/src/lib/accounts.svelte.ts`

**Interfaces:**
- Consumes: `api.accountRoster/setAccountAlias/confirmPairing/unpairCharacter`, `AccountRoster`.
- Produces: `accountsStore` (`{ roster: AccountRoster }` rune), `aliasFor(userId)`, `loadRoster()`, `setAlias(userId, alias)`, `confirmPairing(charId, userId)`, `unpair(charId)`.

- [ ] **Step 1: Write the store.** Create `app/src/lib/accounts.svelte.ts`:

```ts
// Shared, app-wide account roster: aliases + confirmed character membership +
// live suggestions. A Svelte-5 rune module so the sidebar, the open-file header
// and the Accounts view all react to the same state. Mirrors names.svelte.ts.
import { api, type AccountRoster } from "./api";

const empty: AccountRoster = { accounts: [], unassigned: [] };
export const accountsStore = $state<{ roster: AccountRoster }>({ roster: empty });

/// Alias for an account id, or null if unnamed/unknown.
export function aliasFor(userId: number): string | null {
  return accountsStore.roster.accounts.find((a) => a.user_id === userId)?.alias ?? null;
}

export async function loadRoster(): Promise<void> {
  try {
    accountsStore.roster = await api.accountRoster();
  } catch {
    // Silent: leave the last roster in place.
  }
}

export async function setAlias(userId: number, alias: string | null): Promise<void> {
  accountsStore.roster = await api.setAccountAlias(userId, alias);
}

// Throws on the hard-cap rejection so the caller can surface it.
export async function confirmPairing(charId: number, userId: number): Promise<void> {
  accountsStore.roster = await api.confirmPairing(charId, userId);
}

export async function unpair(charId: number): Promise<void> {
  accountsStore.roster = await api.unpairCharacter(charId);
}
```

- [ ] **Step 2: Typecheck.**

Run: `cd app; npm run check`
Expected: no errors.

- [ ] **Step 3: Commit.**

```bash
git add app/src/lib/accounts.svelte.ts
git commit -m "Add the shared account roster store"
```

---

### Task 10: Accounts view + entry point (`AccountsView.svelte`, `+page.svelte`, `Sidebar.svelte`)

**Files:**
- Create: `app/src/lib/AccountsView.svelte`
- Modify: `app/src/routes/+page.svelte` (mainView switch, render AccountsView)
- Modify: `app/src/lib/Sidebar.svelte` (Accounts entry point)

**Interfaces:**
- Consumes: `accountsStore`, `loadRoster`, `setAlias`, `confirmPairing`, `unpair` (Task 9); `names` (M3a store); `api.beginCapture/resolveCapture` (Task 8).

- [ ] **Step 1: Build `AccountsView.svelte`.** Create `app/src/lib/AccountsView.svelte`:

```svelte
<script lang="ts">
  import { api, errMessage, type Suggestion } from "./api";
  import { names } from "./names.svelte";
  import { accountsStore, loadRoster, setAlias, confirmPairing, unpair } from "./accounts.svelte";

  const MAX = 3;
  const roster = $derived(accountsStore.roster);
  let error: string | null = $state(null);

  // Guided capture state (see Task 11 for the flow body).
  let capturing = $state(false);
  let captureNote: string | null = $state(null);

  const nameOf = (id: number) => names[id]?.name ?? `char ${id}`;

  async function onConfirm(charId: number, userId: number) {
    error = null;
    try {
      await confirmPairing(charId, userId);
    } catch (e) {
      error = errMessage(e);
    }
  }

  async function commitAlias(userId: number, value: string) {
    await setAlias(userId, value.trim() === "" ? null : value);
  }

  loadRoster();
</script>

<section class="accounts">
  <header class="accounts-head">
    <h2>Accounts</h2>
    <div class="head-actions">
      <button onclick={() => loadRoster()}>Refresh</button>
      <button onclick={() => (capturing = true)}>Calibrate an account…</button>
    </div>
  </header>

  {#if error}<p class="error">{error}</p>{/if}
  {#if captureNote}<p class="flash" aria-live="polite">{captureNote}</p>{/if}

  {#if roster.accounts.length === 0}
    <p class="hint">No accounts discovered yet. Open a profile, or run a calibration.</p>
  {/if}

  <ul class="cards">
    {#each roster.accounts as acct (acct.user_id)}
      <li class="card">
        <input
          class="alias"
          value={acct.alias ?? ""}
          placeholder={`core_user_${acct.user_id}`}
          onblur={(e) => commitAlias(acct.user_id, e.currentTarget.value)}
          onkeydown={(e) => e.key === "Enter" && e.currentTarget.blur()} />
        <div class="slots">
          {#each Array(MAX) as _, i (i)}
            {@const charId = acct.characters[i]}
            {#if charId != null}
              <span class="chip filled">
                {nameOf(charId)}
                <button class="x" title="Unpair" onclick={() => unpair(charId)}>✕</button>
              </span>
            {:else}
              {@const sugg = acct.suggestions[i - acct.characters.length]}
              {#if sugg}
                <span class="chip ghost" title={`${sugg.basis} (${sugg.confidence})`}>
                  probably {nameOf(sugg.char_id)}?
                  <button class="ok" onclick={() => onConfirm(sugg.char_id, acct.user_id)}>✓</button>
                </span>
              {:else}
                <span class="chip empty">
                  <select
                    onchange={(e) => {
                      const v = Number(e.currentTarget.value);
                      if (v) onConfirm(v, acct.user_id);
                      e.currentTarget.selectedIndex = 0;
                    }}>
                    <option value="">＋ add character</option>
                    {#each roster.unassigned as uid (uid)}
                      <option value={uid}>{nameOf(uid)}</option>
                    {/each}
                  </select>
                </span>
              {/if}
            {/if}
          {/each}
        </div>
      </li>
    {/each}
  </ul>

  {#if roster.unassigned.length > 0}
    <div class="unassigned">
      <h3>Unassigned characters</h3>
      <ul>
        {#each roster.unassigned as uid (uid)}
          <li>{nameOf(uid)}</li>
        {/each}
      </ul>
    </div>
  {/if}
</section>

<style>
  .accounts { padding: 1rem; overflow: auto; }
  .accounts-head { display: flex; justify-content: space-between; align-items: baseline; }
  .cards { list-style: none; padding: 0; display: grid; gap: 0.75rem; }
  .card { border: 1px solid var(--line, #3333); border-radius: 8px; padding: 0.6rem; }
  .alias { font-weight: 600; width: 100%; margin-bottom: 0.5rem; }
  .slots { display: flex; gap: 0.4rem; flex-wrap: wrap; }
  .chip { display: inline-flex; align-items: center; gap: 0.3em; padding: 0.15em 0.5em;
          border-radius: 999px; border: 1px solid var(--line, #3333); font-size: 0.9em; }
  .chip.ghost { opacity: 0.7; font-style: italic; }
  .chip.empty select { border: none; background: transparent; }
  .x, .ok { border: none; background: transparent; cursor: pointer; }
  .error { color: #c0392b; }
  .unassigned h3 { margin: 1rem 0 0.3rem; font-size: 0.9em; opacity: 0.7; }
</style>
```

- [ ] **Step 2: Add the main-view switch in `+page.svelte`.** In the `<script>`, add the imports and state:

```ts
  import AccountsView from "$lib/AccountsView.svelte";
  import { aliasFor } from "$lib/accounts.svelte";

  let mainView: "file" | "accounts" = $state("file");
```

In `openFile`, after a successful open, return to the file view (find where `view = "tree"` is set on open and add beside it):

```ts
  mainView = "file";
```

Wrap the main pane so the Accounts view replaces the file editor. Locate the main content region rendered after the sidebar and gate it:

```svelte
  {#if mainView === "accounts"}
    <AccountsView />
  {:else}
    <!-- existing file editor markup (filebar + tree/layout/hex) stays here -->
  {/if}
```

- [ ] **Step 3: Add the entry point in `Sidebar.svelte`.** Add a prop and a button. In the `<script>`, extend the props:

```ts
  let { onOpen, onShowAccounts }: { onOpen: (path: string) => void; onShowAccounts: () => void } =
    $props();
```

In the `.sidebar-actions` block, add:

```svelte
    <button onclick={onShowAccounts} title="Manage account names and character associations">Accounts</button>
```

And pass it from `+page.svelte` where `<Sidebar>` is rendered:

```svelte
  <Sidebar onOpen={openFile} onShowAccounts={() => (mainView = "accounts")} />
```

- [ ] **Step 4: Typecheck + manual smoke.**

Run: `cd app; npm run check`
Expected: no errors.
Manual (PowerShell): `cd app; npm run tauri dev` — click **Accounts**: the view lists accounts with 3-slot cards; type an alias and click away (persists on reload); an unassigned character can be added to a slot; a suggestion ghost-chip confirms with ✓; adding a 4th character shows the cap error.

- [ ] **Step 5: Commit.**

```bash
git add app/src/lib/AccountsView.svelte app/src/routes/+page.svelte app/src/lib/Sidebar.svelte
git commit -m "Add the dedicated Accounts view with 3-slot cards and suggestions"
```

---

### Task 11: Guided capture flow (`AccountsView.svelte`)

**Files:**
- Modify: `app/src/lib/AccountsView.svelte`

**Interfaces:**
- Consumes: `api.beginCapture/resolveCapture`, `confirmPairing`, `loadRoster`, `names`.

- [ ] **Step 1: Add the capture modal + handlers.** In `AccountsView.svelte` `<script>`, add:

```ts
  async function startCapture() {
    captureNote = null;
    await api.beginCapture();
    capturing = true;
  }

  async function finishCapture() {
    const r = await api.resolveCapture();
    if (r.detected) {
      const [charId, userId] = r.detected;
      try {
        await confirmPairing(charId, userId);
        captureNote = `Paired ${nameOf(charId)} ↔ account ${userId}.`;
        capturing = false;
      } catch (e) {
        captureNote = errMessage(e);
      }
    } else if (r.changed_users.length === 0) {
      captureNote =
        "The account file didn't change. Make an account-wide change (so core_user is written), fully log out, then click Done again.";
    } else if (r.changed_users.length > 1) {
      captureNote = `Several account files changed (${r.changed_users.join(", ")}). Log out of just one account and retry.`;
    } else {
      captureNote = "No matching character file changed — log in as one character, change something, log out, and retry.";
    }
    await loadRoster();
  }
```

Change the header **Calibrate** button to call `startCapture`:

```svelte
      <button onclick={startCapture}>Calibrate an account…</button>
```

- [ ] **Step 2: Add the modal markup.** After the `<header>` block in the template:

```svelte
  {#if capturing}
    <div class="capture" role="dialog" aria-label="Calibrate an account">
      <p>1. Launch EVE and log in as the character whose account you want to identify.</p>
      <p>2. Change an account-wide setting so the account file is written.</p>
      <p>3. Fully log out / close the client, then click Done.</p>
      <div class="capture-actions">
        <button onclick={finishCapture}>Done</button>
        <button onclick={() => (capturing = false)}>Cancel</button>
      </div>
    </div>
  {/if}
```

Add styling:

```svelte
  .capture { border: 1px solid var(--line, #3333); border-radius: 8px; padding: 0.75rem;
             margin: 0.75rem 0; background: var(--panel, #0001); }
  .capture-actions { display: flex; gap: 0.5rem; margin-top: 0.5rem; }
```

- [ ] **Step 3: Typecheck + manual smoke (mocked).**

Run: `cd app; npm run check`
Expected: no errors.
Manual: click **Calibrate**, then **Done** without changing anything → the "account file didn't change" guidance appears (the tolerant path). The live end-to-end run is Task 12.

- [ ] **Step 4: Commit.**

```bash
git add app/src/lib/AccountsView.svelte
git commit -m "Add the guided controlled-logout capture flow"
```

---

### Task 12: Aliases in the sidebar and open-file header

**Files:**
- Modify: `app/src/lib/Sidebar.svelte` (user-file rows)
- Modify: `app/src/routes/+page.svelte` (open-file header)

**Interfaces:**
- Consumes: `aliasFor`, `accountsStore`, `loadRoster` (Task 9).

- [ ] **Step 1: Show aliases for user files in the sidebar.** In `Sidebar.svelte`, import the store and load it alongside names. In `<script>`:

```ts
  import { accountsStore, aliasFor, loadRoster } from "./accounts.svelte";
```

In `refresh()`, after `void resolveNames(charIds(profiles));`, add:

```ts
      void loadRoster();
```

In the file-row template, replace the label expression so user files prefer their alias. The row currently reads `{hit ? hit.name : f.file_name}`; change the `{@const hit ...}`/label to also cover user files:

```svelte
          {@const label =
            f.kind === "char" && f.id != null && names[f.id]
              ? names[f.id].name
              : f.kind === "user" && f.id != null && aliasFor(f.id)
                ? aliasFor(f.id)
                : f.file_name}
          <li>
            <button class="file" onclick={() => onOpen(f.path)} title={f.file_name}>
              {label}
              <span class="meta">{Math.round(f.size / 1024)} KB</span>
            </button>
          </li>
```

(Reading `accountsStore.roster` via `aliasFor` inside the derived label keeps the row reactive to roster changes.)

- [ ] **Step 2: Show the alias in the open-file header.** In `+page.svelte` `<script>`, add a derived alongside `openCharName`:

```ts
  // Alias for the loaded user file, if named. `core_user_<id>.dat` -> alias.
  const openUserAlias = $derived.by(() => {
    if (current?.status !== "opened") return null;
    const m = current.file_name.match(/^core_user_(\d+)\.dat$/);
    return m ? aliasFor(Number(m[1])) : null;
  });
```

In the `.filebar` header, where `openCharName` is rendered next to the file name, render `openUserAlias` the same way for user files (mirror the existing char-name markup).

- [ ] **Step 3: Typecheck + manual smoke.**

Run: `cd app; npm run check`
Expected: no errors.
Manual: name an account in the Accounts view, then confirm the sidebar row for that `core_user_*` file shows the alias, and opening it shows the alias in the header.

- [ ] **Step 4: Commit.**

```bash
git add app/src/lib/Sidebar.svelte app/src/routes/+page.svelte
git commit -m "Show account aliases in the sidebar and open-file header"
```

---

### Task 13: Manual smoke gate — live guided capture

**Files:** none (verification only).

- [ ] **Step 1: Full suite green.**

Run: `cd app/src-tauri; cargo test` → all pass.
Run: `cd app; npm test` → pure-helper tests pass (if any were added).
Run: `cd app; npm run check` → no type errors.

- [ ] **Step 2: Live run against real profiles.**

Run: `cd app; npm run tauri dev`.
- Open the Accounts view: confirm discovered accounts appear, suggestions look plausible (spot-check a name-match against a character you know is on that account).
- Name an account; reload the app; confirm the alias persisted and shows in the sidebar + header.

- [ ] **Step 3: Live guided capture (the calibration gate).**

- Click **Calibrate an account…**, then in the real EVE client log in as one known character, make an account-wide change, and fully log out.
- Click **Done**: verify the detected pairing is the correct character↔account. If the account file didn't advance, note which in-game change *did* write `core_user` (this identifies the concrete trigger the spec deferred, §8) and record it in `docs/format-notes.md` under a `## Account-file write trigger` heading.

- [ ] **Step 4: Record the outcome.** Append a one-line status to `docs/format-notes.md` (milestone smoke passed, plus the discovered trigger) and commit:

```bash
git add docs/format-notes.md
git commit -m "Record the M3b guided-capture smoke result and account-write trigger"
```

---

## Self-Review

**Spec coverage:**
- §2 name-match-primary/mtime-corroborator → Task 4 (`rank`).
- §3 data model, global-id keying, hard cap, durable write, corrupt→empty → Tasks 1, 2.
- §4.1 `accounts.rs` pure functions + seams → Tasks 3–7. §4.2 command surface + `AppState.capture` → Task 8.
- §5.1 Accounts view (3-slot cards, unassigned, one-click sibling fill, ghost chips) → Task 10. §5.2 guided capture → Task 11. §5.3 aliases everywhere via shared store → Tasks 9, 12.
- §6 error handling (corrupt store, single-membership, cap `ErrDto`, capture exclusions/retry, no network) → Tasks 1, 2, 7, 8, 11.
- §7 testing (rank/capture_diff/mutators/persistence pure units, temp-tree orchestrator, manual live gate) → Tasks 1–8, 13.
- §8 deferrals (exact trigger, no persisted dismissals) → left unbuilt; trigger identified in Task 13.

**Placeholder scan:** none — every step has concrete code/commands. The `+page.svelte` edits (Task 10 Step 2, Task 12 Step 2) reference existing markup regions rather than repeating the whole file; the anchor lines (`view = "tree"`, `.filebar`, `openCharName`) are named exactly.

**Type consistency:** `AccountRoster`/`AccountView`/`Suggestion`/`Confidence`/`CaptureResult` identical across `accounts.rs` → `api.ts` → stores/components. `confirm`/`confirm_pairing`/`confirmPairing`, `unpair`/`unpair_character`/`unpairCharacter`, `set_alias`/`set_account_alias`/`setAccountAlias` map consistently across layers. `FileMeta`/`Kind`/`Snapshot` are internal to `accounts.rs`/`ops.rs`. `MAX_CHARS_PER_ACCOUNT = 3` (Rust) mirrored by `const MAX = 3` in the view.
