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

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::accounts::AccountsStore;

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
///     A `Fetching` arriving while one is still unanswered discards it AND
///     poisons the next `Fetched`: with two requests in flight a `Fetched` can
///     be answering either, and pairing it with whichever is pending attributes
///     one account's whole character list to the other account. Both drop.
///     This is not airtight — a longer interleave can still leave exactly one
///     plausible candidate — which is why the vote exists rather than a single
///     reading being trusted.
///  2. **Recency.** Per account, keep only the most recently seen surviving set.
///     Logs span years and an account's characters do change.
///  3. **Disjointness.** A character claimed by two surviving accounts is
///     dropped from both — a character belongs to exactly one account, so two
///     claims mean one of them is wrong and we cannot tell which.
pub fn parse_logs<I: IntoIterator<Item = String>>(lines: I) -> LauncherRoster {
    let mut votes: HashMap<Vec<u64>, HashMap<u64, u32>> = HashMap::new();
    let mut last_seen: HashMap<Vec<u64>, usize> = HashMap::new();
    let mut pending: Option<Vec<u64>> = None;
    // Set when a `Fetching` displaced one that was never answered. The next
    // `Fetched` could be answering either request, so it may not pair.
    let mut contested = false;

    for (seq, line) in lines.into_iter().enumerate() {
        if let Some(ids) = fetching_ids(&line) {
            contested = pending.is_some();
            pending = Some(ids);
            continue;
        }
        if let Some((n, user)) = fetched_count_and_user(&line) {
            // `take` clears the pending set either way: an unmatched count is a
            // mis-pairing, not something to hold on to.
            let candidate = pending.take();
            if !contested {
                if let Some(ids) = candidate {
                    if ids.len() == n {
                        *votes.entry(ids.clone()).or_default().entry(user).or_default() += 1;
                        last_seen.insert(ids, seq);
                    }
                }
            }
            contested = false;
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

    // 2. Recency, per account. `last_seen` is written beside every vote, so the
    // lookup cannot miss — but index-panicking on that invariant is a footgun for
    // whoever edits this next, and dropping the entry is the safe direction anyway.
    let mut newest: HashMap<u64, (usize, Vec<u64>)> = HashMap::new();
    for (ids, user) in winners {
        let Some(&seq) = last_seen.get(&ids) else { continue };
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
        let kept: Vec<u64> = ids.into_iter().filter(|c| claims.get(c) == Some(&1)).collect();
        if !kept.is_empty() {
            accounts.insert(user, kept);
        }
    }
    LauncherRoster { accounts }
}

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
    fn an_interleaved_launch_drops_both_sets() {
        // Two launches overlap and only one answer arrives. WHICH request it
        // answers is not knowable from the log, so neither set is claimed.
        let r = parse_logs([
            fetching(&[90000001, 90000002, 90000003]),
            fetching(&[90000004, 90000005, 90000006]),
            fetched(3, 80000002),
        ]);
        assert!(r.accounts.is_empty(), "an unanswered request poisons the next answer");
    }

    #[test]
    fn two_overlapping_launches_pair_nothing_rather_than_guessing() {
        // The dangerous shape, and the reason `contested` exists: both requests
        // are answered. Pairing the first answer with what is pending hands one
        // account the OTHER account's three characters — a confident, wrong,
        // unopposed pairing. Real logs contain this shape.
        let r = parse_logs([
            fetching(&[90000001, 90000002, 90000003]),
            fetching(&[90000004, 90000005, 90000006]),
            fetched(3, 80000001),
            fetched(3, 80000002),
        ]);
        assert!(r.accounts.is_empty(), "a guess here misattributes a whole account");
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
}
