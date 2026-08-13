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
