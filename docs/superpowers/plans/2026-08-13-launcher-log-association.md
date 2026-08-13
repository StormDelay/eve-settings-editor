# Launcher-Log Character↔Account Association Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Read the EVE launcher's own logs to propose which characters belong to which account, and let the user confirm them in one click — without replacing the existing manual and guided-capture paths.

**Architecture:** A new `app/src-tauri/src/launcher.rs` mines two log lines the EVE launcher writes, resolves them to a `user_id → [char_id]` map through a majority vote plus a disjointness check, and diffs that against the persisted `accounts.json`. The result is a list of `Proposal`s the Accounts view renders as ghost chips (empty slots) and conflict warnings (chips the launcher disputes). Nothing is written without a click; every ambiguity in the parse degrades to *fewer* proposals, never a wrong one.

**Tech Stack:** Rust (Tauri 2 command layer, `serde`), Svelte 5 runes, vitest + `@testing-library/svelte`.

Spec: `docs/superpowers/specs/2026-08-13-launcher-log-association-design.md`
Branch: `feat/launcher-log-association` (already created, off `master`)

## Global Constraints

- **Synthetic ids only.** Never commit a real character or account id in tests, fixtures or docs. This plan uses `90000001…` for characters and `80000001…` for accounts, matching `accounts.rs` and `Sidebar.spec.ts`.
- **Failure is silent.** A missing log directory, an unreadable file, or zero matching lines yields an empty result — never an error, never a panic. Same rule as `accounts.rs::load_store` and `discover.rs::discover`.
- **Pure logic is FS-free.** Parsing and proposal computation take injected inputs and are tested without touching disk; only the orchestrators read files. Mirrors the `names.rs` / `accounts.rs` split.
- **This lives in the `app` crate.** `blue-marshal` and `settings-model` stay dependency-free.
- **Every new command must appear in three places** — `#[tauri::command]` fn, the `generate_handler!` list in `lib.rs`, and `api.ts`. `app/src/lib/ipc.test.ts` scans all three and fails if they disagree.
- **The existing paths are untouched.** Manual pairing and guided capture must still work exactly as they do today; they are the escape hatch for accounts the launcher does not cover.
- **Rust tests:** `cargo test` from `app/src-tauri`. **Frontend tests:** `npm test` from `app/` (runs `svelte-kit sync && vitest run`); a subset with `npx vitest run <pattern>`.

---

## File Structure

| File | Responsibility |
|---|---|
| `app/src-tauri/src/launcher.rs` | **New.** Parse launcher logs → `LauncherRoster`; locate the log dir; diff against `AccountsStore` → `Vec<Proposal>`. |
| `app/src-tauri/src/accounts.rs` | **Modify.** Add `confirm_pairings` — apply many pairings, save once. |
| `app/src-tauri/src/lib.rs` | **Modify.** `mod launcher;`, two commands, two `generate_handler!` entries. |
| `app/src/lib/api.ts` | **Modify.** `Proposal` interface, `launcherProposals()`, `confirmPairings()`. |
| `app/src/lib/launcher.ts` | **New.** Pure merge: proposals → per-card ghosts and conflicts. |
| `app/src/lib/launcher.test.ts` | **New.** Tests for the above. |
| `app/src/lib/accounts.svelte.ts` | **Modify.** Add `confirmMany`, the store-owned wrapper for the batched confirm. |
| `app/src/lib/AccountsView.svelte` | **Modify.** Ghost chips, Accept all, conflict rows, empty state. |
| `app/src/lib/AccountsView.spec.ts` | **New.** Component test for the rendering and the IPC it fires. |
| `docs/format-notes.md` | **Modify.** Record the launcher-log lines and the three measurements. |
| `CHANGELOG.md` | **Modify.** One user-facing bullet under `[Unreleased]`. |

---

### Task 1: `launcher.rs` — the pure log parse

**Files:**
- Create: `app/src-tauri/src/launcher.rs`
- Modify: `app/src-tauri/src/lib.rs:1-8` (add `mod launcher;`)
- Test: inline `#[cfg(test)] mod tests` in `app/src-tauri/src/launcher.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `pub struct LauncherRoster { pub accounts: HashMap<u64, Vec<u64>> }` and
  `pub fn parse_logs<I: IntoIterator<Item = String>>(lines: I) -> LauncherRoster`.
  Lines must be supplied **oldest-first**; the recency rule in step 3 depends on it.

**Background — what the log looks like.** The EVE launcher writes these two lines a few lines apart, with unrelated output between them:

```
2026-08-12 16:47:05.802    app     info:    [esi] Fetching character details for 90000001, 90000002, 90000003
2026-08-12 16:47:06.310    app     info:    [esi] Fetched 3 character details for 80000001
```

Three things go wrong in real logs and all three must degrade to *fewer* pairings:
1. Concurrent launches interleave, producing a `Fetching` with no matching `Fetched`.
2. The same character set can be attributed to two different accounts across sessions.
3. Logs span years, so an account's character set can legitimately change.

- [ ] **Step 1: Write the failing tests**

Create `app/src-tauri/src/launcher.rs` with only the test module and the doc comment (no implementation yet — the next step runs them and watches them fail to compile):

```rust
//! The EVE launcher's own char↔account mapping, mined from its log files.
//!
//! The launcher writes, a few lines apart:
//!   [esi] Fetching character details for <char_id>, <char_id>, <char_id>
//!   [esi] Fetched 3 character details for <user_id>
//!
//! Nothing else states the pairing. Measured 2026-08-13: char and user settings
//! files carry no id cross-reference at all, in either direction, and a
//! character's name appears in nearly every account file (chat and contacts),
//! so neither the files nor the names can answer this.
//!
//! The format is undocumented, so every ambiguity here resolves to FEWER
//! pairings, never a wrong one: a launcher release that renames these lines must
//! degrade to "no proposals", not to a bad proposal.

#[cfg(test)]
mod tests {
    use super::*;

    /// Real logs put unrelated lines between the pair; the parse must span them.
    fn noise() -> String {
        "2026-08-12 16:47:06.266    app     info:    [scheduler] Stopped scheduler foreground".into()
    }
    fn fetching(ids: &[u64]) -> String {
        format!(
            "2026-08-12 16:47:05.802    app     info:    [esi] Fetching character details for {}",
            ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", ")
        )
    }
    fn fetched(n: usize, user: u64) -> String {
        format!(
            "2026-08-12 16:47:06.310    app     info:    [esi] Fetched {n} character details for {user}"
        )
    }
    fn chars(r: &LauncherRoster, user: u64) -> Vec<u64> {
        r.accounts.get(&user).cloned().unwrap_or_default()
    }

    #[test]
    fn a_clean_pair_across_unrelated_lines_is_read() {
        let r = parse_logs([
            fetching(&[90000001, 90000002, 90000003]),
            noise(),
            fetched(3, 80000001),
        ]);
        assert_eq!(chars(&r, 80000001), vec![90000001, 90000002, 90000003]);
        assert_eq!(r.accounts.len(), 1);
    }

    #[test]
    fn an_interleaved_launch_drops_the_orphaned_set() {
        // Two launches overlap: A's ids are never confirmed, B's are.
        let r = parse_logs([
            fetching(&[90000001, 90000002, 90000003]),
            fetching(&[90000004, 90000005, 90000006]),
            fetched(3, 80000002),
        ]);
        assert_eq!(chars(&r, 80000002), vec![90000004, 90000005, 90000006]);
        assert_eq!(r.accounts.len(), 1, "the orphaned set claims nothing");
    }

    #[test]
    fn a_majority_of_observations_wins() {
        let mut lines = Vec::new();
        for _ in 0..4 {
            lines.push(fetching(&[90000001, 90000002, 90000003]));
            lines.push(fetched(3, 80000001));
        }
        lines.push(fetching(&[90000001, 90000002, 90000003]));
        lines.push(fetched(3, 80000009)); // one mis-attribution from an interleave
        let r = parse_logs(lines);
        assert_eq!(chars(&r, 80000001), vec![90000001, 90000002, 90000003]);
        assert!(!r.accounts.contains_key(&80000009), "the outvoted claim is dropped");
    }

    #[test]
    fn a_tied_vote_drops_the_set_entirely() {
        let r = parse_logs([
            fetching(&[90000001, 90000002, 90000003]),
            fetched(3, 80000001),
            fetching(&[90000001, 90000002, 90000003]),
            fetched(3, 80000002),
        ]);
        assert!(r.accounts.is_empty(), "1:1 is not evidence");
    }

    #[test]
    fn a_character_claimed_by_two_surviving_accounts_is_dropped_from_both() {
        let r = parse_logs([
            fetching(&[90000001, 90000002, 90000003]),
            fetched(3, 80000001),
            fetching(&[90000003, 90000004, 90000005]), // 90000003 in both
            fetched(3, 80000002),
        ]);
        assert_eq!(chars(&r, 80000001), vec![90000001, 90000002]);
        assert_eq!(chars(&r, 80000002), vec![90000004, 90000005]);
    }

    #[test]
    fn a_count_that_disagrees_with_the_id_list_is_ignored() {
        let r = parse_logs([fetching(&[90000001, 90000002, 90000003]), fetched(2, 80000001)]);
        assert!(r.accounts.is_empty());
    }

    #[test]
    fn the_most_recent_set_wins_for_an_account_whose_characters_changed() {
        // Lines arrive oldest-first. A character was transferred away years ago;
        // the stale set must not linger just because it was seen more often.
        let mut lines = Vec::new();
        for _ in 0..3 {
            lines.push(fetching(&[90000001, 90000002, 90000003]));
            lines.push(fetched(3, 80000001));
        }
        lines.push(fetching(&[90000001, 90000002, 90000007]));
        lines.push(fetched(3, 80000001));
        let r = parse_logs(lines);
        assert_eq!(chars(&r, 80000001), vec![90000001, 90000002, 90000007]);
    }

    #[test]
    fn no_lines_yield_an_empty_roster() {
        assert_eq!(parse_logs(Vec::<String>::new()), LauncherRoster::default());
    }

    #[test]
    fn unrelated_lines_alone_yield_an_empty_roster() {
        assert_eq!(parse_logs([noise(), noise()]), LauncherRoster::default());
    }
}
```

Add `mod launcher;` to `app/src-tauri/src/lib.rs`, in the alphabetical module list at the top:

```rust
mod accounts;
mod groups;
mod launcher;
mod names;
mod ops;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd app/src-tauri && cargo test launcher`
Expected: FAIL — compile errors, `cannot find type LauncherRoster` / `cannot find function parse_logs`.

- [ ] **Step 3: Write the implementation**

Insert above the `#[cfg(test)]` module in `launcher.rs`:

```rust
use std::collections::HashMap;

/// `user_id → its character ids`, as the launcher reported them.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct LauncherRoster {
    pub accounts: HashMap<u64, Vec<u64>>,
}

/// The id list from an `[esi] Fetching character details for …` line, sorted and
/// deduped so it can key the vote. `None` for any other line, or if any token is
/// not a plain integer.
fn fetching_ids(line: &str) -> Option<Vec<u64>> {
    let rest = line.split_once("[esi] Fetching character details for ")?.1;
    let mut ids: Vec<u64> =
        rest.split(',').map(|t| t.trim().parse::<u64>()).collect::<Result<_, _>>().ok()?;
    if ids.is_empty() {
        return None;
    }
    ids.sort_unstable();
    ids.dedup();
    Some(ids)
}

/// `(count, user_id)` from an `[esi] Fetched N character details for U` line.
fn fetched_count_and_user(line: &str) -> Option<(usize, u64)> {
    let rest = line.split_once("[esi] Fetched ")?.1;
    let (n, rest) = rest.split_once(" character details for ")?;
    Some((n.trim().parse().ok()?, rest.trim().parse().ok()?))
}

/// Mine `lines` — **oldest-first**, which `read_roster_from` guarantees — for the
/// launcher's char↔account pairings.
///
/// Three passes, each of which can only remove pairings:
///  1. **Vote.** Tally `sorted(char ids) → user id` over every matched pair.
///     A second `Fetching` before a `Fetched` discards the first: that is the
///     shape a concurrent launch leaves behind.
///  2. **Recency.** Per account, keep only the most recently seen surviving set.
///     Logs span years and an account's characters do change.
///  3. **Disjointness.** A character claimed by two surviving accounts is
///     dropped from both — a character belongs to exactly one account, so two
///     claims mean one of them is wrong and we cannot tell which.
pub fn parse_logs<I: IntoIterator<Item = String>>(lines: I) -> LauncherRoster {
    let mut votes: HashMap<Vec<u64>, HashMap<u64, u32>> = HashMap::new();
    let mut last_seen: HashMap<Vec<u64>, usize> = HashMap::new();
    let mut pending: Option<Vec<u64>> = None;

    for (seq, line) in lines.into_iter().enumerate() {
        if let Some(ids) = fetching_ids(&line) {
            pending = Some(ids);
            continue;
        }
        if let Some((n, user)) = fetched_count_and_user(&line) {
            // `take` clears the pending set either way: an unmatched count is a
            // mis-pairing, not something to hold on to.
            if let Some(ids) = pending.take() {
                if ids.len() == n {
                    *votes.entry(ids.clone()).or_default().entry(user).or_default() += 1;
                    last_seen.insert(ids, seq);
                }
            }
        }
    }

    // 1. Vote. A tie is not evidence and drops the set.
    let mut winners: Vec<(Vec<u64>, u64)> = Vec::new();
    for (ids, users) in votes {
        let mut best: Option<(u64, u32)> = None;
        let mut tied = false;
        for (&user, &count) in &users {
            match best {
                Some((_, b)) if count > b => {
                    best = Some((user, count));
                    tied = false;
                }
                Some((_, b)) if count == b => tied = true,
                Some(_) => {}
                None => best = Some((user, count)),
            }
        }
        if let (Some((user, _)), false) = (best, tied) {
            winners.push((ids, user));
        }
    }

    // 2. Recency, per account.
    let mut newest: HashMap<u64, (usize, Vec<u64>)> = HashMap::new();
    for (ids, user) in winners {
        let seq = last_seen[&ids];
        match newest.get(&user) {
            Some((s, _)) if *s >= seq => {}
            _ => {
                newest.insert(user, (seq, ids));
            }
        }
    }

    // 3. Disjointness.
    let mut claims: HashMap<u64, usize> = HashMap::new();
    for (_, ids) in newest.values() {
        for &c in ids {
            *claims.entry(c).or_default() += 1;
        }
    }
    let mut accounts: HashMap<u64, Vec<u64>> = HashMap::new();
    for (user, (_, ids)) in newest {
        let kept: Vec<u64> = ids.into_iter().filter(|c| claims[c] == 1).collect();
        if !kept.is_empty() {
            accounts.insert(user, kept);
        }
    }
    LauncherRoster { accounts }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd app/src-tauri && cargo test launcher`
Expected: PASS — 9 tests.

Then `cargo clippy --all-targets -- -D warnings` to confirm no new lint. `mod launcher;` with only `parse_logs` used by tests may warn `dead_code`; if it does, leave it — Task 2 adds the callers in the same branch. If you prefer a clean intermediate build, complete Tasks 1 and 2 before committing.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/launcher.rs app/src-tauri/src/lib.rs
git commit -m "Mine char/account pairings out of the EVE launcher's logs"
```

---

### Task 2: `launcher.rs` — locating the logs, and the proposal diff

**Files:**
- Modify: `app/src-tauri/src/launcher.rs`
- Test: inline `#[cfg(test)] mod tests` in the same file

**Interfaces:**
- Consumes: `LauncherRoster`, `parse_logs` (Task 1); `accounts::{AccountsStore, Account}`.
- Produces:
  - `pub fn log_dir() -> Option<PathBuf>`
  - `pub fn read_roster_from(dir: &Path) -> LauncherRoster`
  - `pub fn read_launcher_roster() -> LauncherRoster`
  - `pub struct Proposal { pub char_id: u64, pub user_id: u64, pub conflict: Option<u64> }` (derives `Serialize`)
  - `pub fn proposals(launcher: &LauncherRoster, store: &AccountsStore) -> Vec<Proposal>`

`conflict` is the account the **store** currently holds the character under, when that disagrees with the launcher. `None` means the store is silent about it (or already agrees, in which case no proposal is emitted at all).

- [ ] **Step 1: Write the failing tests**

Append these to the existing `mod tests` in `launcher.rs`:

```rust
    use crate::accounts::{confirm, AccountsStore};
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("launcher-test-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_missing_log_directory_reads_as_an_empty_roster() {
        assert_eq!(read_roster_from(&temp_dir("absent")), LauncherRoster::default());
    }

    #[test]
    fn logs_are_read_oldest_first_by_their_date_stamped_names() {
        // The launcher names logs eve-online-launcher-YYYY.MM.DD-HH.MM.SS.log,
        // so lexical order is chronological — which is what the recency rule needs.
        let d = temp_dir("order");
        fs::create_dir_all(&d).unwrap();
        fs::write(
            d.join("eve-online-launcher-2025.01.01-10.00.00.log"),
            format!("{}\n{}\n", fetching(&[90000001, 90000002, 90000003]), fetched(3, 80000001)),
        )
        .unwrap();
        fs::write(
            d.join("eve-online-launcher-2026.01.01-10.00.00.log"),
            format!("{}\n{}\n", fetching(&[90000001, 90000002, 90000007]), fetched(3, 80000001)),
        )
        .unwrap();
        // A non-log file in the same folder is ignored.
        fs::write(d.join("notes.txt"), fetching(&[90000009])).unwrap();

        let r = read_roster_from(&d);
        assert_eq!(chars(&r, 80000001), vec![90000001, 90000002, 90000007]);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn an_unreadable_byte_does_not_lose_the_rest_of_the_file() {
        let d = temp_dir("lossy");
        fs::create_dir_all(&d).unwrap();
        let mut bytes = vec![0xffu8, 0xfe, b'\n'];
        bytes.extend_from_slice(
            format!("{}\n{}\n", fetching(&[90000001, 90000002, 90000003]), fetched(3, 80000001))
                .as_bytes(),
        );
        fs::write(d.join("eve-online-launcher-2026.01.01-10.00.00.log"), bytes).unwrap();

        assert_eq!(chars(&read_roster_from(&d), 80000001), vec![90000001, 90000002, 90000003]);
        let _ = fs::remove_dir_all(&d);
    }

    fn roster(pairs: &[(u64, &[u64])]) -> LauncherRoster {
        LauncherRoster {
            accounts: pairs.iter().map(|(u, cs)| (*u, cs.to_vec())).collect(),
        }
    }

    #[test]
    fn a_pairing_the_store_already_holds_proposes_nothing() {
        let mut store = AccountsStore::default();
        confirm(&mut store, 90000001, 80000001).unwrap();
        let p = proposals(&roster(&[(80000001, &[90000001])]), &store);
        assert!(p.is_empty(), "agreement is not a proposal");
    }

    #[test]
    fn a_character_the_store_does_not_place_is_proposed_without_conflict() {
        let p = proposals(&roster(&[(80000001, &[90000001])]), &AccountsStore::default());
        assert_eq!(p, vec![Proposal { char_id: 90000001, user_id: 80000001, conflict: None }]);
    }

    #[test]
    fn a_character_the_store_places_elsewhere_is_proposed_with_the_conflict() {
        let mut store = AccountsStore::default();
        confirm(&mut store, 90000001, 80000002).unwrap();
        let p = proposals(&roster(&[(80000001, &[90000001])]), &store);
        assert_eq!(
            p,
            vec![Proposal { char_id: 90000001, user_id: 80000001, conflict: Some(80000002) }],
            "the conflict names where the chip is now, so the UI can show it there"
        );
    }

    #[test]
    fn proposals_come_back_in_a_stable_order() {
        let p = proposals(
            &roster(&[(80000002, &[90000004]), (80000001, &[90000003, 90000001])]),
            &AccountsStore::default(),
        );
        let seen: Vec<(u64, u64)> = p.iter().map(|p| (p.user_id, p.char_id)).collect();
        assert_eq!(seen, vec![(80000001, 90000001), (80000001, 90000003), (80000002, 90000004)]);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd app/src-tauri && cargo test launcher`
Expected: FAIL — `cannot find function read_roster_from` / `cannot find type Proposal` / `cannot find function proposals`.

- [ ] **Step 3: Write the implementation**

Extend the `use` block at the top of `launcher.rs`:

```rust
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::accounts::AccountsStore;
```

Append below `parse_logs`:

```rust
/// The launcher's log directory — Electron's `userData`/logs, per OS. Windows is
/// verified against a real install; the macOS and Linux paths follow Electron's
/// standard `userData` mapping and are not measured. `None` when it is absent,
/// which is an ordinary state (no launcher, or a fresh machine), not an error.
pub fn log_dir() -> Option<PathBuf> {
    let dir = if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var("APPDATA").ok()?).join("EVE Online").join("logs")
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var("HOME").ok()?)
            .join("Library/Application Support/EVE Online/logs")
    } else {
        PathBuf::from(std::env::var("HOME").ok()?).join(".config/EVE Online/logs")
    };
    dir.is_dir().then_some(dir)
}

/// Every `.log` in `dir`, oldest-first, fed through `parse_logs`. Split out from
/// `read_launcher_roster` so a test can point it at a temp directory.
pub fn read_roster_from(dir: &Path) -> LauncherRoster {
    let Ok(entries) = fs::read_dir(dir) else { return LauncherRoster::default() };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "log"))
        .collect();
    // Names are date-stamped (eve-online-launcher-YYYY.MM.DD-HH.MM.SS.log), so
    // lexical order is chronological — what parse_logs' recency rule needs.
    files.sort();
    let lines: Vec<String> = files
        .iter()
        .filter_map(|p| fs::read(p).ok())
        // Lossy rather than strict: one mangled byte must not cost a whole file.
        .flat_map(|b| String::from_utf8_lossy(&b).lines().map(str::to_owned).collect::<Vec<_>>())
        .collect();
    parse_logs(lines)
}

pub fn read_launcher_roster() -> LauncherRoster {
    log_dir().map(|d| read_roster_from(&d)).unwrap_or_default()
}

/// One pairing the launcher asserts and the store does not already hold.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Proposal {
    pub char_id: u64,
    pub user_id: u64,
    /// The account the store currently holds this character under, when that
    /// disagrees with the launcher. The UI shows the warning on THAT card,
    /// beside the chip it contradicts.
    pub conflict: Option<u64>,
}

/// What the launcher says that the store does not. Agreement produces nothing;
/// silence produces a plain proposal; disagreement produces one carrying the
/// account the store currently uses.
pub fn proposals(launcher: &LauncherRoster, store: &AccountsStore) -> Vec<Proposal> {
    let mut held: HashMap<u64, u64> = HashMap::new();
    for (&user, acct) in &store.accounts {
        for &c in &acct.characters {
            held.insert(c, user);
        }
    }
    let mut out = Vec::new();
    for (&user_id, chars) in &launcher.accounts {
        for &char_id in chars {
            match held.get(&char_id) {
                Some(&u) if u == user_id => {}
                Some(&u) => out.push(Proposal { char_id, user_id, conflict: Some(u) }),
                None => out.push(Proposal { char_id, user_id, conflict: None }),
            }
        }
    }
    // HashMap iteration order is not stable; the UI and the tests both want it to be.
    out.sort_by_key(|p| (p.user_id, p.char_id));
    out
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd app/src-tauri && cargo test launcher`
Expected: PASS — 16 tests.

Then: `cargo clippy --all-targets -- -D warnings` → no warnings.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/launcher.rs
git commit -m "Locate the launcher logs and diff them against the stored pairings"
```

---

### Task 3: The command surface

**Files:**
- Modify: `app/src-tauri/src/accounts.rs` (add `confirm_pairings` beside `confirm_pairing`, ~line 263)
- Modify: `app/src-tauri/src/lib.rs` (two commands + two `generate_handler!` entries)
- Modify: `app/src/lib/api.ts` (`Proposal` interface near `AccountRoster` ~line 226; two methods near `confirmPairing` ~line 426)
- Test: `app/src-tauri/src/accounts.rs` inline tests; `app/src/lib/ipc.test.ts` (existing, no edit — it scans)

**Interfaces:**
- Consumes: `launcher::{read_launcher_roster, proposals, Proposal}` (Task 2); `accounts::{load_store, confirm, save_store, load_roster}` (existing).
- Produces:
  - Rust: `accounts::confirm_pairings(roots: &[PathBuf], dir: &Path, pairs: &[(u64, u64)]) -> Result<AccountRoster, String>`
  - IPC: `launcher_proposals() -> Vec<Proposal>`, `confirm_pairings(pairs) -> Result<AccountRoster, ErrDto>`
  - TS: `interface Proposal { char_id: number; user_id: number; conflict: number | null }`,
    `api.launcherProposals(): Promise<Proposal[]>`,
    `api.confirmPairings(pairs: [number, number][]): Promise<AccountRoster>`

**Why a batch command exists.** `confirm_pairing` re-runs discovery and rebuilds the whole roster per call (the `ponytail:` note at `accounts.rs:238`). "Accept all" is 30 pairings on a ten-account install; thirty round trips of that is seconds of stall for the headline action. `confirm_pairings` mutates the in-memory store once and saves once.

- [ ] **Step 1: Write the failing tests**

Append to `mod tests` in `app/src-tauri/src/accounts.rs`:

```rust
    #[test]
    fn confirm_pairings_applies_every_pair_and_persists_once() {
        let root = temp_dir("many-tree");
        let sdir = root.join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        fs::create_dir_all(&sdir).unwrap();
        fs::write(sdir.join("core_user_80000001.dat"), encode(&Value::Int(1)).unwrap()).unwrap();
        fs::write(sdir.join("core_user_80000002.dat"), encode(&Value::Int(1)).unwrap()).unwrap();
        fs::write(sdir.join("core_char_90000001.dat"), encode(&Value::Int(1)).unwrap()).unwrap();
        fs::write(sdir.join("core_char_90000002.dat"), encode(&Value::Int(1)).unwrap()).unwrap();
        let appdir = temp_dir("many-appdata");

        let roster = confirm_pairings(
            std::slice::from_ref(&root),
            &appdir,
            &[(90000001, 80000001), (90000002, 80000002)],
        )
        .unwrap();

        let a = roster.accounts.iter().find(|a| a.user_id == 80000001).unwrap();
        assert_eq!(a.characters, vec![90000001]);
        let b = roster.accounts.iter().find(|a| a.user_id == 80000002).unwrap();
        assert_eq!(b.characters, vec![90000002]);
        assert!(roster.unassigned.is_empty());
        let store = load_store(&appdir);
        assert_eq!(store.accounts[&80000001].characters, vec![90000001]);
        assert_eq!(store.accounts[&80000002].characters, vec![90000002]);
    }

    #[test]
    fn confirm_pairings_writes_nothing_when_one_pair_is_rejected() {
        let root = temp_dir("many-abort-tree");
        let sdir = root.join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        fs::create_dir_all(&sdir).unwrap();
        fs::write(sdir.join("core_user_80000001.dat"), encode(&Value::Int(1)).unwrap()).unwrap();
        let appdir = temp_dir("many-abort-appdata");

        // A fourth character on one account trips the hard cap.
        let err = confirm_pairings(
            std::slice::from_ref(&root),
            &appdir,
            &[
                (90000001, 80000001),
                (90000002, 80000001),
                (90000003, 80000001),
                (90000004, 80000001),
            ],
        )
        .unwrap_err();
        assert!(err.contains('3'), "the cap message names the limit: {err}");
        assert!(
            load_store(&appdir).accounts.is_empty(),
            "an aborted batch leaves the store untouched"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd app/src-tauri && cargo test accounts::tests::confirm_pairings`
Expected: FAIL — `cannot find function confirm_pairings in this scope`.

- [ ] **Step 3: Write the implementation**

In `app/src-tauri/src/accounts.rs`, directly after `confirm_pairing` (~line 263):

```rust
/// Apply many pairings, saving once. All-or-nothing: the first rejection (the
/// hard cap) aborts and nothing is written, because a half-applied batch is
/// harder to reason about than one the user re-runs.
///
/// This exists because `confirm_pairing` reloads the whole roster per call (see
/// the `ponytail:` note above) and "accept everything the launcher proposes" is
/// thirty of them.
pub fn confirm_pairings(
    roots: &[PathBuf],
    dir: &Path,
    pairs: &[(u64, u64)],
) -> Result<AccountRoster, String> {
    let mut store = load_store(dir);
    for &(char_id, user_id) in pairs {
        confirm(&mut store, char_id, user_id)?;
    }
    let _ = save_store(dir, &store);
    Ok(load_roster(roots, dir))
}
```

In `app/src-tauri/src/lib.rs`, after `unpair_character` (~line 176):

```rust
#[tauri::command]
fn confirm_pairings(
    app: tauri::AppHandle,
    pairs: Vec<(u64, u64)>,
) -> Result<accounts::AccountRoster, ErrDto> {
    accounts::confirm_pairings(&settings_model::default_roots(), &app_dir(&app), &pairs)
        .map_err(|m| ErrDto { code: "cap".into(), message: m })
}

/// What the EVE launcher's own logs say about char↔account membership, minus
/// whatever the store already agrees with. Read-only, and separate from
/// `account_roster` on purpose: that one reloads after every alias edit and
/// every confirm, and re-reading megabytes of logs on each would be silly.
#[tauri::command]
fn launcher_proposals(app: tauri::AppHandle) -> Vec<launcher::Proposal> {
    launcher::proposals(&launcher::read_launcher_roster(), &accounts::load_store(&app_dir(&app)))
}
```

And in `generate_handler!`, extend the accounts line (~line 622):

```rust
            account_roster, set_account_alias, confirm_pairing, confirm_pairings, unpair_character,
            launcher_proposals,
            begin_capture, resolve_capture,
```

In `app/src/lib/api.ts`, after the `CaptureResult` interface (~line 234):

```ts
/** One char↔account pairing the EVE launcher's logs assert. */
export interface Proposal {
  char_id: number;
  user_id: number;
  /** Where the store currently puts this character, when it disagrees. */
  conflict: number | null;
}
```

And in the `api` object, after `unpairCharacter` (~line 429):

```ts
  confirmPairings: (pairs: [number, number][]) =>
    invoke<AccountRoster>("confirm_pairings", { pairs }),
  launcherProposals: () => invoke<Proposal[]>("launcher_proposals"),
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd app/src-tauri && cargo test accounts` → PASS
Run: `cd app && npx vitest run ipc` → PASS. This is the gate that both sides of the new commands agree; if a name is wrong it fails here with the mismatched command or argument name.
Run: `cd app && npm run check` → no new svelte-check errors.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/accounts.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Expose launcher proposals and a batched confirm over IPC"
```

---

### Task 4: The pure merge helper

**Files:**
- Create: `app/src/lib/launcher.ts`
- Test: `app/src/lib/launcher.test.ts`

**Interfaces:**
- Consumes: `Proposal` from `./api` (Task 3).
- Produces:
  - `export interface CardProposals { ghosts: number[]; conflicts: { charId: number; target: number }[] }`
  - `export function proposalsByCard(proposals: Proposal[], dismissed: ReadonlySet<number>): Map<number, CardProposals>`
  - `export function acceptAllPairs(proposals: Proposal[], dismissed: ReadonlySet<number>): [number, number][]`

The routing rule, which is the whole reason this is a separate function: a **plain** proposal is a ghost on the account the launcher names; a **conflicting** one is shown on the account whose card currently holds the chip, so the claim appears next to the thing it contradicts.

- [ ] **Step 1: Write the failing test**

Create `app/src/lib/launcher.test.ts`:

```ts
// Pure-module tests: plain data in, plain data out, no DOM. See test/README.md.
import { proposalsByCard, acceptAllPairs } from "./launcher.ts";
import type { Proposal } from "./api.ts";
import { check, eq } from "./test/check.ts";

const plain = (char_id: number, user_id: number): Proposal => ({
  char_id,
  user_id,
  conflict: null,
});
const disputed = (char_id: number, user_id: number, conflict: number): Proposal => ({
  char_id,
  user_id,
  conflict,
});

const none: ReadonlySet<number> = new Set();

check(
  "a plain proposal is a ghost on the account the launcher names",
  eq(proposalsByCard([plain(90000001, 80000001)], none).get(80000001), {
    ghosts: [90000001],
    conflicts: [],
  }),
);

check(
  "a disputed proposal is shown on the card that currently holds the chip",
  eq(proposalsByCard([disputed(90000001, 80000001, 80000002)], none).get(80000002), {
    ghosts: [],
    conflicts: [{ charId: 90000001, target: 80000001 }],
  }),
);

check(
  "a disputed proposal puts nothing on the account the launcher names",
  proposalsByCard([disputed(90000001, 80000001, 80000002)], none).get(80000001) === undefined,
);

check(
  "several ghosts land on the same card in order",
  eq(
    proposalsByCard([plain(90000001, 80000001), plain(90000002, 80000001)], none).get(80000001)
      ?.ghosts,
    [90000001, 90000002],
  ),
);

check(
  "a dismissed character disappears from the cards",
  proposalsByCard([plain(90000001, 80000001)], new Set([90000001])).size === 0,
);

check(
  "acceptAllPairs takes every plain proposal as a char/user pair",
  eq(acceptAllPairs([plain(90000001, 80000001), plain(90000002, 80000002)], none), [
    [90000001, 80000001],
    [90000002, 80000002],
  ]),
);

check(
  "acceptAllPairs never includes a disputed proposal",
  acceptAllPairs([disputed(90000001, 80000001, 80000002)], none).length === 0,
);

check(
  "acceptAllPairs skips dismissed characters",
  acceptAllPairs([plain(90000001, 80000001)], new Set([90000001])).length === 0,
);
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd app && npx vitest run launcher`
Expected: FAIL — cannot resolve `./launcher.ts`.

- [ ] **Step 3: Write the implementation**

Create `app/src/lib/launcher.ts`:

```ts
// Pure merge of the launcher's proposals onto the account cards. Kept out of the
// component so the routing rule — which card shows a ghost, which shows a
// dispute — is testable without mounting anything.
import type { Proposal } from "./api";

export interface CardProposals {
  /** Characters the launcher puts on this account that the store does not. */
  ghosts: number[];
  /** Chips ON this card the launcher disputes, and the account it names instead. */
  conflicts: { charId: number; target: number }[];
}

/**
 * Group proposals by the card that should show them.
 *
 * A disputed proposal is deliberately routed to `conflict` — the account whose
 * card holds the chip today — not to the account the launcher names. The user
 * needs to see the claim beside the thing it contradicts; showing it on the
 * target card would be a second, unexplained ghost.
 */
export function proposalsByCard(
  proposals: Proposal[],
  dismissed: ReadonlySet<number>,
): Map<number, CardProposals> {
  const out = new Map<number, CardProposals>();
  const card = (id: number) => {
    let c = out.get(id);
    if (!c) out.set(id, (c = { ghosts: [], conflicts: [] }));
    return c;
  };
  for (const p of proposals) {
    if (dismissed.has(p.char_id)) continue;
    if (p.conflict === null) card(p.user_id).ghosts.push(p.char_id);
    else card(p.conflict).conflicts.push({ charId: p.char_id, target: p.user_id });
  }
  return out;
}

/** Every undisputed proposal, shaped as `confirm_pairings`' argument. */
export function acceptAllPairs(
  proposals: Proposal[],
  dismissed: ReadonlySet<number>,
): [number, number][] {
  return proposals
    .filter((p) => p.conflict === null && !dismissed.has(p.char_id))
    .map((p) => [p.char_id, p.user_id]);
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd app && npx vitest run launcher`
Expected: PASS — 8 tests.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/launcher.ts app/src/lib/launcher.test.ts
git commit -m "Route launcher proposals to the account card that should show them"
```

---

### Task 5: The Accounts view

**Files:**
- Modify: `app/src/lib/accounts.svelte.ts`
- Modify: `app/src/lib/AccountsView.svelte`
- Create: `app/src/lib/AccountsView.spec.ts`

**Interfaces:**
- Consumes: `api.launcherProposals`, `api.confirmPairings`, `type Proposal` (Task 3); `proposalsByCard`, `acceptAllPairs` (Task 4); existing `confirmPairing`, `loadRoster`, `aliasFor` from `./accounts.svelte`; `resolveNames` from `./names.svelte`; the local `nameOf`.
- Produces: `export async function confirmMany(pairs: [number, number][]): Promise<void>` in `accounts.svelte.ts`. Nothing else; the view is the leaf.

**What changes on screen.** Three additions to each account card and two outside them. Nothing existing is removed: the alias field, the manual `＋ add character` picker, Refresh and Calibrate all stay exactly as they are, because they are the path for accounts the launcher does not cover.

1. An empty slot with a ghost available renders the proposed character's name with a ✓ (accept) and a ✕ (dismiss, this session only) **instead of** the picker; dismissing puts the picker back.
2. Ghosts with no empty slot left render as one line beneath the slots, with the same accept action. Clicking one hits the existing hard-cap error path, which is the correct and visible outcome.
3. A disputed chip gets a line naming where the launcher puts it, with **Move it** and **Keep mine**.
4. The header gains **Accept all — N characters** when any undisputed proposal survives.
5. When the proposals have loaded and there are none, one line says so and points at Calibrate. Absence must be legible, not blank.

**Two things the view has to do that are easy to miss.**

- **Names.** `names` is a module-level rune store the Sidebar populates; `AccountsView` never has. A proposed character always has a settings file in practice, but relying on another component having resolved it first is exactly the kind of coupling that renders `char 90000001` in a ghost chip. The view resolves the ids it is about to show.
- **Accepted proposals must leave the list.** `proposalsByCard` does not know the roster, so a ghost the user just accepted would re-render in the *next* empty slot — the character appearing twice on its own card. Every accept path drops the character from `proposals`.

- [ ] **Step 1: Write the failing component test**

Create `app/src/lib/AccountsView.spec.ts`:

```ts
// Component test: the Accounts view's launcher-proposal rendering and the IPC it
// fires. `openPath` is null throughout so the profile scope is inert and every
// account card renders.
import { describe, expect, test } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import AccountsView from "$lib/AccountsView.svelte";
import { calls } from "$lib/test/setup";
import type { AccountRoster, Proposal } from "$lib/api";

const ROSTER: AccountRoster = {
  accounts: [
    { user_id: 80000001, alias: "Main", characters: [] },
    { user_id: 80000002, alias: null, characters: [90000009] },
  ],
  unassigned: [90000001, 90000009],
};

// Shape must match `ResolvedName` in api.ts — `{ name, category }`, not `source`.
const NAMES = {
  90000001: { name: "Alpha", category: "character" },
  90000009: { name: "Zulu", category: "character" },
};

function mount(proposals: Proposal[], roster: AccountRoster = ROSTER) {
  calls.stub("account_roster", roster);
  calls.stub("discover_profiles", []);
  calls.stub("resolve_character_names", NAMES);
  calls.stub("launcher_proposals", proposals);
  calls.stub("confirm_pairing", roster);
  calls.stub("confirm_pairings", roster);
  render(AccountsView, { openPath: null });
}

describe("launcher proposals", () => {
  test("an undisputed proposal offers the character on the named account", async () => {
    mount([{ char_id: 90000001, user_id: 80000001, conflict: null }]);
    const accept = await waitFor(() => screen.getByRole("button", { name: /accept Alpha/i }));
    await fireEvent.click(accept);
    await waitFor(() => expect(calls.only("confirm_pairing").args).toEqual({
      charId: 90000001,
      userId: 80000001,
    }));
  });

  test("accept all sends every undisputed pair in one call", async () => {
    mount([
      { char_id: 90000001, user_id: 80000001, conflict: null },
      { char_id: 90000009, user_id: 80000001, conflict: 80000002 },
    ]);
    const all = await waitFor(() => screen.getByRole("button", { name: /accept all/i }));
    await fireEvent.click(all);
    await waitFor(() =>
      expect(calls.only("confirm_pairings").args).toEqual({ pairs: [[90000001, 80000001]] }),
    );
  });

  test("a disputed character is flagged on the card that holds it, naming the target", async () => {
    mount([{ char_id: 90000009, user_id: 80000001, conflict: 80000002 }]);
    const warning = await waitFor(() => screen.getByText(/launcher log puts Zulu on Main/i));
    expect(warning).toBeTruthy();
  });

  test("move it repairs the pairing to the account the launcher names", async () => {
    mount([{ char_id: 90000009, user_id: 80000001, conflict: 80000002 }]);
    const move = await waitFor(() => screen.getByRole("button", { name: /move Zulu/i }));
    await fireEvent.click(move);
    await waitFor(() => expect(calls.only("confirm_pairing").args).toEqual({
      charId: 90000009,
      userId: 80000001,
    }));
  });

  test("keep mine drops the warning and writes nothing", async () => {
    mount([{ char_id: 90000009, user_id: 80000001, conflict: 80000002 }]);
    const keep = await waitFor(() => screen.getByRole("button", { name: /keep Zulu/i }));
    await fireEvent.click(keep);
    await waitFor(() =>
      expect(screen.queryByText(/launcher log puts Zulu/i)).toBeNull(),
    );
    calls.never("confirm_pairing");
    calls.never("confirm_pairings");
  });

  test("an accepted ghost leaves the list instead of reappearing in the next slot", async () => {
    mount([{ char_id: 90000001, user_id: 80000001, conflict: null }]);
    const accept = await waitFor(() => screen.getByRole("button", { name: /accept Alpha/i }));
    await fireEvent.click(accept);
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: /accept Alpha/i })).toBeNull(),
    );
  });

  test("with no proposals there is no accept-all button, and the view says why", async () => {
    mount([]);
    await waitFor(() => screen.getByText(/launcher logs say nothing/i));
    expect(screen.queryByRole("button", { name: /accept all/i })).toBeNull();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd app && npx vitest run AccountsView`
Expected: FAIL — no element matching `/accept Alpha/i`; the view does not call `launcher_proposals` yet.

- [ ] **Step 3: Write the implementation**

In `app/src/lib/accounts.svelte.ts`, append beside `confirmPairing`:

```ts
// Several pairings in one round trip. `confirmPairing` rebuilds the whole roster
// per call, and "accept everything the launcher proposes" is thirty of them.
// Throws on the hard-cap rejection, like confirmPairing.
export async function confirmMany(pairs: [number, number][]): Promise<void> {
  accountsStore.roster = await api.confirmPairings(pairs);
}
```

In `app/src/lib/AccountsView.svelte`, extend the imports:

```ts
  import { api, errMessage, type Profile, type Proposal } from "./api";
  import { names, resolveNames } from "./names.svelte";
  import { resolvedName } from "./filesort.svelte";
  import {
    accountsStore,
    loadRoster,
    setAlias,
    confirmPairing,
    confirmMany,
    unpair,
    aliasFor,
  } from "./accounts.svelte";
  import { proposalsByCard, acceptAllPairs } from "./launcher";
```

Add state and derivations beside the existing capture state (~line 49):

```ts
  // Launcher-log proposals. Loaded once on mount: unlike the roster, this does
  // not change when the user edits an alias, and re-reading the logs on every
  // roster refresh would be waste.
  let proposals = $state<Proposal[]>([]);
  let proposalsLoaded = $state(false);
  // Session-only, like the M3b suggestion dismissals: a "keep mine" is a
  // judgement about this sitting, not something to persist.
  let dismissed = $state<number[]>([]);
  const dismissedSet = $derived(new Set(dismissed));
  const byCard = $derived(proposalsByCard(proposals, dismissedSet));
  const allPairs = $derived(acceptAllPairs(proposals, dismissedSet));

  const accountLabel = (userId: number) => aliasFor(userId) ?? `core_user_${userId}`;

  async function acceptAll() {
    error = null;
    try {
      await confirmMany(allPairs);
      // Drop what was just accepted. `proposalsByCard` cannot see the roster, so
      // a proposal left in the list re-renders as a ghost in the next empty slot
      // — the same character twice on one card.
      const accepted = new Set(allPairs.map(([charId]) => charId));
      proposals = proposals.filter((p) => !accepted.has(p.char_id));
    } catch (e) {
      error = errMessage(e);
    }
  }
```

Extend the existing `onConfirm` (~line 54) so the single-accept and Move-it paths drop their proposal too:

```ts
  async function onConfirm(charId: number, userId: number) {
    error = null;
    try {
      await confirmPairing(charId, userId);
      proposals = proposals.filter((p) => p.char_id !== charId);
    } catch (e) {
      error = errMessage(e);
    }
  }
```

Load the proposals next to the existing `loadRoster()` call at the bottom of the script (~line 100). The name resolution is not optional: `names` is populated by the sidebar, and a ghost chip for a character it never resolved would read `char 90000001`.

```ts
  loadRoster();
  api
    .launcherProposals()
    .then(async (p) => {
      proposals = p;
      await resolveNames(p.map((x) => x.char_id));
    })
    .catch(() => {})
    .finally(() => (proposalsLoaded = true));
```

In the header's `.head-actions`, before Refresh:

```svelte
      {#if allPairs.length > 0}
        <button onclick={acceptAll}>Accept all — {allPairs.length} characters</button>
      {/if}
```

Below the existing `{#if error}` / `{#if captureNote}` lines, add the empty state:

```svelte
  {#if proposalsLoaded && proposals.length === 0}
    <p class="hint">
      Your EVE launcher logs say nothing about these accounts — use “Calibrate an account…”
      to pair a character by hand.
    </p>
  {/if}
```

Inside the `{#each accounts as acct}` block, capture the card's proposals at the top of the `<li>`:

```svelte
      <li class="card">
        {@const card = byCard.get(acct.user_id)}
        {@const ghosts = card?.ghosts ?? []}
```

Replace the empty-slot branch (currently the bare `<select>`) with a ghost-or-picker choice. `slot` counts which empty slot this is, so ghosts fill them in order:

```svelte
            {:else}
              {@const slot = i - acct.characters.length}
              {#if ghosts[slot] != null}
                {@const gid = ghosts[slot]}
                <span class="chip ghost">
                  {nameOf(gid)}
                  <button class="ok" title="Accept {nameOf(gid)}"
                          aria-label="Accept {nameOf(gid)}"
                          onclick={() => onConfirm(gid, acct.user_id)}>✓</button>
                  <button class="x" title="Dismiss {nameOf(gid)}"
                          aria-label="Dismiss {nameOf(gid)}"
                          onclick={() => (dismissed = [...dismissed, gid])}>✕</button>
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
                    {#each sortedUnassigned as uid (uid)}
                      <option value={uid}>{nameOf(uid)}</option>
                    {/each}
                  </select>
                </span>
              {/if}
            {/if}
```

After the `.slots` div, inside the same `<li>`, add the source note, the overflow line and the conflict rows:

```svelte
        {#if ghosts.length > 0}
          <p class="from-launcher">From your launcher log.</p>
        {/if}
        {#each ghosts.slice(Math.max(0, MAX - acct.characters.length)) as gid (gid)}
          <p class="from-launcher">
            Your launcher log also puts {nameOf(gid)} here, but all three slots are full.
            <button onclick={() => onConfirm(gid, acct.user_id)}>Accept anyway</button>
          </p>
        {/each}
        {#each card?.conflicts ?? [] as c (c.charId)}
          <p class="conflict">
            Your launcher log puts {nameOf(c.charId)} on {accountLabel(c.target)}.
            <button aria-label="Move {nameOf(c.charId)}"
                    onclick={() => onConfirm(c.charId, c.target)}>Move it</button>
            <button aria-label="Keep {nameOf(c.charId)}"
                    onclick={() => (dismissed = [...dismissed, c.charId])}>Keep mine</button>
          </p>
        {/each}
```

Add the three styles beside the existing `.chip` rules:

```css
  .chip.ghost { border-style: dashed; opacity: 0.85; }
  .ok { border: none; background: transparent; cursor: pointer; color: inherit; }
  .from-launcher { margin: 0.3rem 0 0; font-size: 0.85em; opacity: 0.7; }
  .conflict { margin: 0.3rem 0 0; font-size: 0.9em; }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd app && npx vitest run AccountsView` → PASS — 7 tests.
Run: `cd app && npm test` → the whole suite green.
Run: `cd app && npm run check` → no new svelte-check errors.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/accounts.svelte.ts app/src/lib/AccountsView.svelte app/src/lib/AccountsView.spec.ts
git commit -m "Offer the launcher's pairings in the Accounts view, and flag the ones it disputes"
```

---

### Task 6: Live check and documentation

**Files:**
- Modify: `docs/format-notes.md` (new section)
- Modify: `CHANGELOG.md` (under `## [Unreleased]`)

**Interfaces:**
- Consumes: everything above.
- Produces: nothing code-facing.

**This is the only task that needs a real machine, and notably it does not need EVE running** — the launcher logs are already on disk. That is the property worth confirming.

- [ ] **Step 1: Run the app against real launcher logs**

Use the project's `starting-the-app` skill. Open the Accounts view and check, without clicking anything yet:

1. Proposals appear, and the accounts they name match the `core_user_<id>.dat` files in the sidebar.
2. Each proposed account carries at most three characters.
3. No character is proposed for two accounts.
4. Any conflict row names an account you recognise.

If proposals are empty, confirm `%APPDATA%\EVE Online\logs` exists and holds `eve-online-launcher-*.log` files containing `[esi] Fetched` — an empty result there is the designed behaviour, not a bug, but it means the check has not been performed.

- [ ] **Step 2: Accept them and confirm the write**

Click **Accept all**, then Refresh. Every proposed character should now sit as a filled chip on its account, and re-opening the view should show no remaining ghosts. Confirm `accounts.json` in the app-data dir holds the same mapping.

- [ ] **Step 3: Record the format finding**

Append to `docs/format-notes.md`, following the file's convention that **no real character or account ids appear** — the counts below are real, the ids are placeholders:

```markdown
### The EVE launcher logs char↔account membership (2026-08-13)

Neither settings file states which account a character belongs to. Measured over
the `2026-07-27T131810Z_baseline` corpus, every profile folder, checking each
file for the opposite kind's ids as LE32, LE64 and decimal ASCII in both
directions: **zero hits**. This extends the M0 finding (a char file does not
contain its own character id) to the cross-reference.

The in-file name heuristic is also dead, and now measured rather than assumed:
against a known-correct mapping on a live Tranquility profile, a character's ESI
name uniquely identified its account for **4 of 27** characters. Most names
appear in *every* account file in the folder — chat channel labels and contact
lists, not membership.

**The launcher does state it**, in
`%APPDATA%\EVE Online\logs\eve-online-launcher-*.log`:

```
[esi] Fetching character details for <char_id>, <char_id>, <char_id>
[esi] Fetched 3 character details for <user_id>
```

The two lines sit a few lines apart with unrelated output between them. On one
real install: **186 paired observations → 10 accounts, exactly 3 characters
each, fully disjoint**, every `user_id` matching a discovered
`core_user_<id>.dat`. Logs were retained back to 2023-11 (98 files, 8.1 MB), so
coverage is not limited to recent sessions.

Two hazards, both handled in `launcher.rs`: concurrent launches interleave the
pair (3 of 189 `Fetching` lines had no matching `Fetched`), and an account's
character set legitimately changes over a multi-year log. The parse votes,
prefers the most recent surviving set per account, and drops any character two
accounts claim.

Also present and unused: `[client-queue] Queued client startup { userId,
characterId: <slot>, profile: '<name>' }`, which additionally names the profile
folder. `characterId` there is a slot index, not a character id.
```

- [ ] **Step 4: Write the release note**

Under `## [Unreleased]` in `CHANGELOG.md` — one line, user-facing, no engineering detail:

```markdown
## [Unreleased]

### Added
- The Accounts view now offers your characters' accounts straight from the EVE launcher, and tells you when a pairing you already made disagrees with it.
```

- [ ] **Step 5: Commit**

```bash
git add docs/format-notes.md CHANGELOG.md
git commit -m "Record the launcher-log finding and the release note"
```

---

## Follow-ups (deliberately not in this plan)

Recorded in the spec's §6 and repeated here so they are not lost:

- **Guided capture auto-confirms.** `finishCapture()` writes the pairing the moment `capture_diff` reports `detected`, without asking — contrary to the M3b spec's "capture *detects*, the user *confirms*". With a background client running, that turns a coincidence into a silent wrong write. The user asked for this to wait until the launcher path exists.
- **Evidence on account cards** (last EVE write, profile folder), for users the launcher log does not cover.
- **Caching the log parse** by `(path, mtime)`, if one read per Accounts-view mount ever drags.
