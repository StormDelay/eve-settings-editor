//! The EVE launcher's own char↔account mapping, mined from its log files.
//!
//! The launcher writes, a few lines apart:
//!   [esi] Fetching character details for <char_id>, <char_id>, <char_id>
//!   [esi] Fetched 3 character details for <user_id>
//!
//! and, immediately BEFORE the request, on most launches:
//!   [virtual-goods] Fetched Plex status for '<user_id>' on 'tranquility' …
//!
//! The last of those is the strongest signal, because it names the account
//! beside the ids at REQUEST time rather than at reply time, and two adjacent
//! lines cannot be interleaved by a concurrent launch.
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

/// A comma-separated id list, sorted and deduped so it can key the vote.
/// `None` if any token is not a plain integer.
fn parse_id_list(rest: &str) -> Option<Vec<u64>> {
    let mut ids: Vec<u64> =
        rest.split(',').map(|t| t.trim().parse::<u64>()).collect::<Result<_, _>>().ok()?;
    if ids.is_empty() {
        return None;
    }
    ids.sort_unstable();
    ids.dedup();
    Some(ids)
}

/// Whether this is an `[esi] Fetching character details for …` line, and the
/// ids it names.
///
/// `None` — not a request line at all.
/// `Some(None)` — a request line whose id list will not parse. The request still
/// happened, so it must still count as in flight; treating it as "not a line"
/// would credit the next answer to the *previous* request, which is a confident
/// wrong pairing rather than a missing one.
fn fetching_ids(line: &str) -> Option<Option<Vec<u64>>> {
    let rest = line.split_once("[esi] Fetching character details for ")?.1;
    Some(parse_id_list(rest))
}

/// `(count, user_id)` from an `[esi] Fetched N character details for U` line.
fn fetched_count_and_user(line: &str) -> Option<(usize, u64)> {
    let rest = line.split_once("[esi] Fetched ")?.1;
    let (n, rest) = rest.split_once(" character details for ")?;
    Some((n.trim().parse().ok()?, rest.trim().parse().ok()?))
}

/// The account id from `[virtual-goods] Fetched Plex status for '<user>' …`.
///
/// Only the first quoted field is read. The line also carries a PLEX balance,
/// and a looser parse that scanned for "a number" could take that for an account
/// id — an invented pairing, which is the one failure mode this module forbids.
fn plex_user(line: &str) -> Option<u64> {
    let rest = line.split_once("[virtual-goods] Fetched Plex status for '")?.1;
    rest.split_once('\'')?.0.trim().parse().ok()
}

/// Undo a tally recorded from a claim — **both halves of it**, which is why the
/// two maps are undone here together rather than at the call site.
///
/// The vote must genuinely leave the map: left to stand beside the contradicting
/// one it turns positive evidence of interleaving into a tie at best, and a
/// surviving wrong pairing at worst. And `last_seen` must go back to `prior`: a
/// leftover timestamp from a retracted observation carries its id set past a
/// genuinely newer one in the recency pass even with its own vote gone, as long
/// as the set holds any other surviving vote.
fn retract(
    votes: &mut HashMap<Vec<u64>, HashMap<u64, u32>>,
    last_seen: &mut HashMap<Vec<u64>, usize>,
    ids: &[u64],
    user: u64,
    prior: Option<usize>,
) {
    if let Some(users) = votes.get_mut(ids) {
        if let Some(count) = users.get_mut(&user) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                users.remove(&user);
            }
        }
        if users.is_empty() {
            votes.remove(ids);
        }
    }
    match prior {
        Some(seq) => last_seen.insert(ids.to_vec(), seq),
        None => last_seen.remove(ids),
    };
}

/// Mine the launcher's char↔account pairings from `files` — one `Vec` of lines
/// per log file, **files oldest-first and lines in order**, which
/// `read_roster_from` guarantees.
///
/// Three passes, each of which can only remove pairings:
///  1. **Vote.** Tally `sorted(char ids) → user id` over every matched pair, and
///     only when **exactly one request is in flight**. With two outstanding, a
///     `Fetched` can be answering either, and pairing it with whichever is
///     pending attributes one account's whole character list to the other
///     account. A bool is not enough here: it forgets the still-outstanding
///     request as soon as one answer is dropped, so the *next* pair looks clean
///     while a late reply is in flight — measured at 10 of 186 tallies on a real
///     install, each one a confident, unopposed, wrong pairing.
///
///     The counter is **reset per file**, which is why this takes files rather
///     than one line stream: a request that never got its answer would otherwise
///     leave the count above one forever and silently drop everything after it
///     (measured: 170 of 182 tallies lost).
///
///     This is not airtight — a longer interleave can still leave exactly one
///     plausible candidate — which is why the vote exists rather than a single
///     reading being trusted.
///
///     **Except when the request line carries a claimed account.** A
///     `[virtual-goods] Fetched Plex status for '<user>'` line names the account
///     for the *next* request, and the pairing is then complete from two
///     adjacent lines, so it is tallied immediately and the in-flight count is
///     irrelevant to it: interleaving cannot corrupt a fact that never spans a
///     gap. Measured over 98 real log files, 148 of 191 requests carry such a
///     claim, and every one of them that a reply could check agreed. It takes
///     the corpus from 176 observations to 188 — every one the counter dropped —
///     behind the same 10 accounts × 3 disjoint characters.
///
///     A claim is cleared by **another request or another reply** — either means
///     the launch cycle that Plex line belonged to has moved on, and the account
///     it named no longer describes what is being requested now — and it is
///     **voided outright by a second, differing Plex line**, which is two
///     concurrent launches naming two accounts for one request with nothing in
///     between for the other barriers to catch. Not cleared by a line-distance
///     window: the honest gaps run from 1 line to 38, so any distance is a
///     guess, while these structural barriers separate the corpus exactly.
///
///     If a later `Fetched` for that same request still names a *different*
///     account, the observation is **retracted**, not merely doubted — a
///     contradiction is positive evidence of interleaving, the one thing the
///     counter can only avoid, never detect.
///  2. **Recency.** Per account, keep only the most recently seen surviving set.
///     Logs span years and an account's characters do change.
///  3. **Disjointness.** A character claimed by two surviving accounts is
///     dropped from both — a character belongs to exactly one account, so two
///     claims mean one of them is wrong and we cannot tell which.
pub fn parse_logs(files: &[Vec<String>]) -> LauncherRoster {
    let mut votes: HashMap<Vec<u64>, HashMap<u64, u32>> = HashMap::new();
    let mut last_seen: HashMap<Vec<u64>, usize> = HashMap::new();
    // Runs across files so the recency rule sees one timeline.
    let mut seq = 0usize;

    for lines in files {
        // The pending request's ids, the account a Plex line claimed for it, and
        // the `last_seen` value that claim's tally displaced.
        let mut pending: Option<(Vec<u64>, Option<u64>, Option<usize>)> = None;
        // Absent / one claim / **void**. Not last-write-wins: two concurrent
        // launches emit two Plex lines back to back, and neither barrier below
        // fires on that — no request and no reply comes between them. Taking the
        // last would hand one account the other's whole character list, with
        // nothing left to contradict it if the file ends before the reply.
        let mut claimed: Option<Option<u64>> = None;
        let mut in_flight = 0usize;
        for line in lines {
            seq += 1;
            if let Some(user) = plex_user(line) {
                claimed = Some(match claimed {
                    // A differing account before the request: ambiguous, so void
                    // — and it stays void until something consumes it.
                    Some(Some(held)) if held != user => None,
                    // A repeat of the same account is the same claim, not a
                    // conflict; an already-void claim stays void.
                    Some(held) => held,
                    None => Some(user),
                });
                continue;
            }
            if let Some(ids) = fetching_ids(line) {
                in_flight += 1;
                // `take`: the claim belongs to ONE request. A second request
                // finds nothing to claim. No line-distance window — the real
                // gaps vary and a distance is arbitrary; what makes a claim
                // stale is a competing claim, another request or another reply,
                // never its age. `flatten` drops a void claim to the in-flight
                // rule, which is where every ambiguity here is meant to land.
                let claim = claimed.take().flatten();
                let mut prior = None;
                if let (Some(ids), Some(user)) = (&ids, claim) {
                    *votes.entry(ids.clone()).or_default().entry(user).or_default() += 1;
                    // `insert` hands back what it displaced — exactly what a
                    // retraction has to put back.
                    prior = last_seen.insert(ids.clone(), seq);
                }
                pending = ids.map(|ids| (ids, claim, prior));
                continue;
            }
            if let Some((n, user)) = fetched_count_and_user(line) {
                // `take` clears the pending set either way: an unmatched count is
                // a mis-pairing, not something to hold on to.
                match pending.take() {
                    // Already tallied at request time. The reply adds nothing
                    // when it agrees — a second tally would let one launch
                    // outvote a genuinely disagreeing one — and when it
                    // disagrees it retracts.
                    Some((ids, Some(claim), prior)) => {
                        if claim != user {
                            retract(&mut votes, &mut last_seen, &ids, claim, prior);
                        }
                    }
                    Some((ids, None, _)) if in_flight == 1 && ids.len() == n => {
                        *votes.entry(ids.clone()).or_default().entry(user).or_default() += 1;
                        last_seen.insert(ids, seq);
                    }
                    _ => {}
                }
                // An answer with nothing outstanding means the log starts
                // mid-request; saturating rather than panicking.
                in_flight = in_flight.saturating_sub(1);
                // A reply ends the launch cycle its Plex line belonged to, so an
                // unconsumed claim is stale from here on. Measured over 98 real
                // log files, this separates the signal exactly: of 153 claims
                // that a reply could check, all 146 with no reply in between
                // agreed, and all 7 with one disagreed. Without this the parser
                // relies on the retraction to catch them — which only works when
                // a contradicting reply happens to arrive at all.
                claimed = None;
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
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("log")))
        .collect();
    // Names are date-stamped (eve-online-launcher-YYYY.MM.DD-HH.MM.SS.log), so
    // lexical order is chronological — what parse_logs' recency rule needs.
    files.sort();
    // One Vec per file, not one flat stream: parse_logs resets its in-flight
    // counter at each boundary, and an unanswered request must not cross one.
    let per_file: Vec<Vec<String>> = files
        .iter()
        .filter_map(|p| fs::read(p).ok())
        // Lossy rather than strict: one mangled byte must not cost a whole file.
        .map(|b| String::from_utf8_lossy(&b).lines().map(str::to_owned).collect())
        .collect();
    parse_logs(&per_file)
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
    /// The request-time claim. `balance` is deliberately account-id-shaped: it
    /// must never be mistaken for the id.
    fn plex(user: u64, balance: u64) -> String {
        format!(
            "2026-08-12 16:47:05.700    app     info:    [virtual-goods] Fetched Plex status for '{user}' on 'tranquility' (eve-online) with balance: {balance}"
        )
    }
    fn chars(r: &LauncherRoster, user: u64) -> Vec<u64> {
        r.accounts.get(&user).cloned().unwrap_or_default()
    }

    #[test]
    fn a_clean_pair_across_unrelated_lines_is_read() {
        let r = parse_logs(&[vec![
            fetching(&[90000001, 90000002, 90000003]),
            noise(),
            fetched(3, 80000001),
        ]]);
        assert_eq!(chars(&r, 80000001), vec![90000001, 90000002, 90000003]);
        assert_eq!(r.accounts.len(), 1);
    }

    #[test]
    fn an_interleaved_launch_drops_both_sets() {
        // Two launches overlap and only one answer arrives. WHICH request it
        // answers is not knowable from the log, so neither set is claimed.
        let r = parse_logs(&[vec![
            fetching(&[90000001, 90000002, 90000003]),
            fetching(&[90000004, 90000005, 90000006]),
            fetched(3, 80000002),
        ]]);
        assert!(r.accounts.is_empty(), "an unanswered request poisons the next answer");
    }

    #[test]
    fn two_overlapping_launches_pair_nothing_rather_than_guessing() {
        // The dangerous shape, and the reason the counter exists: both requests
        // are answered. Pairing the first answer with what is pending hands one
        // account the OTHER account's three characters — a confident, wrong,
        // unopposed pairing. Real logs contain this shape.
        let r = parse_logs(&[vec![
            fetching(&[90000001, 90000002, 90000003]),
            fetching(&[90000004, 90000005, 90000006]),
            fetched(3, 80000001),
            fetched(3, 80000002),
        ]]);
        assert!(r.accounts.is_empty(), "a guess here misattributes a whole account");
    }

    #[test]
    fn a_reply_still_in_flight_does_not_make_the_next_pair_look_clean() {
        // The shape a bool misses: after the first dropped answer it forgets
        // that request B is STILL outstanding, so C/U2 reads as an undisputed
        // pair — a ghost carrying no conflict and no opposing vote, which rides
        // along in Accept all. Counting requests instead keeps C poisoned.
        let r = parse_logs(&[vec![
            fetching(&[90000001, 90000002, 90000003]), // A
            fetching(&[90000004, 90000005, 90000006]), // B
            fetched(3, 80000001),                      // answers A or B
            fetching(&[90000007, 90000008, 90000009]), // C, with one still open
            fetched(3, 80000002),
            fetched(3, 80000003),
        ]]);
        assert!(r.accounts.is_empty(), "a late reply must not certify the next pair");
    }

    #[test]
    fn an_unanswered_request_does_not_poison_the_next_file() {
        // The counter resets at every file boundary. Without that, one request
        // that never got its answer leaves it above one for the rest of time and
        // every later pairing is silently lost.
        let r = parse_logs(&[
            vec![fetching(&[90000001, 90000002, 90000003])], // no answer, file ends
            vec![fetching(&[90000004, 90000005, 90000006]), fetched(3, 80000002)],
        ]);
        assert_eq!(chars(&r, 80000002), vec![90000004, 90000005, 90000006]);
    }

    #[test]
    fn an_unparseable_id_list_still_counts_as_a_request() {
        // The ids are unreadable, so this request's own pairing is lost — but it
        // IS a request, and the answer that follows may be its. Crediting that
        // answer to the earlier clean request would invent a wrong pairing.
        let r = parse_logs(&[vec![
            fetching(&[90000001, 90000002, 90000003]),
            format!("{} (retry)", fetching(&[90000004])),
            fetched(3, 80000001),
        ]]);
        assert!(r.accounts.is_empty(), "an unreadable request is opaque, not absent");
    }

    #[test]
    fn a_claimed_request_pairs_even_with_other_requests_in_flight() {
        // The account is named on the line before the ids, so no gap exists for
        // a concurrent launch to interleave — the in-flight count says nothing
        // about this observation and must not veto it.
        let r = parse_logs(&[vec![
            fetching(&[90000001, 90000002, 90000003]),
            fetching(&[90000004, 90000005, 90000006]),
            plex(80000003, 80000009),
            fetching(&[90000007, 90000008, 90000009]),
        ]]);
        assert_eq!(chars(&r, 80000003), vec![90000007, 90000008, 90000009]);
        assert_eq!(r.accounts.len(), 1, "only the claimed request pairs");
    }

    #[test]
    fn a_balance_is_never_read_as_an_account_id() {
        let r = parse_logs(&[vec![
            plex(80000001, 80000009),
            fetching(&[90000001, 90000002, 90000003]),
        ]]);
        assert_eq!(chars(&r, 80000001), vec![90000001, 90000002, 90000003]);
        assert!(!r.accounts.contains_key(&80000009), "the balance is not the account");
    }

    #[test]
    fn an_intervening_request_takes_the_claim_away_from_the_second() {
        // The claim belongs to the next request only. B must fall through to the
        // in-flight rule — which, with A still open, drops it.
        let r = parse_logs(&[vec![
            plex(80000001, 80000009),
            fetching(&[90000001, 90000002, 90000003]), // A, claimed
            fetching(&[90000004, 90000005, 90000006]), // B, unclaimed
            fetched(3, 80000002),
        ]]);
        assert_eq!(chars(&r, 80000001), vec![90000001, 90000002, 90000003]);
        assert_eq!(r.accounts.len(), 1, "B's claim was A's, and B has none of its own");
    }

    #[test]
    fn two_accounts_claiming_one_request_void_the_claim() {
        // Two concurrent launches, and the file ends before either reply. No
        // request and no reply sits between the two Plex lines, so the other
        // barriers see nothing — and nothing ever arrives to retract with.
        // Last-write-wins here hands one account the other's whole character
        // list, unopposed. Voiding drops it to the in-flight rule instead.
        let r = parse_logs(&[vec![
            plex(80000002, 80000009),
            plex(80000001, 80000009),
            fetching(&[90000004, 90000005, 90000006]),
        ]]);
        assert!(r.accounts.is_empty(), "two claimants for one request is not evidence");
    }

    #[test]
    fn a_repeated_plex_line_for_the_same_account_is_still_a_claim() {
        // Voiding is about disagreement, not about counting lines: the same
        // account said twice says the same thing.
        let r = parse_logs(&[vec![
            plex(80000001, 80000009),
            plex(80000001, 80000009),
            fetching(&[90000001, 90000002, 90000003]),
        ]]);
        assert_eq!(chars(&r, 80000001), vec![90000001, 90000002, 90000003]);
    }

    #[test]
    fn a_retraction_gives_recency_back_to_the_genuinely_newer_set() {
        // The retracted observation's vote is gone, but its timestamp must go
        // too. The old set still holds its own honest vote, so a leftover
        // timestamp is enough to carry it past the newer set in the recency pass
        // — a character transferred away years ago reappearing on the card.
        let r = parse_logs(&[vec![
            fetching(&[90000001, 90000002, 90000003]), // the old set
            fetched(3, 80000001),
            fetching(&[90000001, 90000002, 90000007]), // the newer set
            fetched(3, 80000001),
            plex(80000001, 80000009), // a claimed observation for the OLD set…
            fetching(&[90000001, 90000002, 90000003]),
            fetched(3, 80000002), // …contradicted, so retracted
        ]]);
        assert_eq!(chars(&r, 80000001), vec![90000001, 90000002, 90000007]);
    }

    #[test]
    fn a_reply_between_the_claim_and_the_request_makes_the_claim_stale() {
        // The Plex line belonged to a launch that has since finished, so it says
        // nothing about the request that comes after. Measured over 98 real log
        // files: of the 153 claims a reply could check, all 146 with no reply in
        // between agreed and all 7 with one disagreed — a clean separation, and
        // the only claims the previous rule got wrong.
        let r = parse_logs(&[vec![
            fetching(&[90000001, 90000002, 90000003]),
            plex(80000002, 80000009),
            fetched(3, 80000001), // that launch is answered; the claim is spent
            fetching(&[90000004, 90000005, 90000006]),
            fetched(3, 80000003),
        ]]);
        assert_eq!(chars(&r, 80000001), vec![90000001, 90000002, 90000003]);
        assert_eq!(chars(&r, 80000003), vec![90000004, 90000005, 90000006]);
        assert!(!r.accounts.contains_key(&80000002), "a spent claim must not pair");
    }

    #[test]
    fn a_reply_contradicting_the_claim_retracts_the_observation() {
        // Two lines apart said one account, the reply says another: the request
        // and the answer were interleaved. Neither reading survives.
        let r = parse_logs(&[vec![
            plex(80000001, 80000009),
            fetching(&[90000001, 90000002, 90000003]),
            fetched(3, 80000002),
        ]]);
        assert!(r.accounts.is_empty(), "a contradiction removes the vote, it does not add one");
    }

    #[test]
    fn a_reply_agreeing_with_the_claim_does_not_vote_twice() {
        // A double tally would let one launch outvote a genuine disagreement.
        // Held against exactly one opposing observation, the correct count ties
        // and drops the set; a doubled one would win it.
        let r = parse_logs(&[vec![
            plex(80000001, 80000009),
            fetching(&[90000001, 90000002, 90000003]),
            fetched(3, 80000001), // agrees
            fetching(&[90000001, 90000002, 90000003]),
            fetched(3, 80000002), // one opposing observation
        ]]);
        assert!(r.accounts.is_empty(), "1:1 is a tie — the claim counts once, not twice");
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
        let r = parse_logs(&[lines]);
        assert_eq!(chars(&r, 80000001), vec![90000001, 90000002, 90000003]);
        assert!(!r.accounts.contains_key(&80000009), "the outvoted claim is dropped");
    }

    #[test]
    fn a_tied_vote_drops_the_set_entirely() {
        let r = parse_logs(&[vec![
            fetching(&[90000001, 90000002, 90000003]),
            fetched(3, 80000001),
            fetching(&[90000001, 90000002, 90000003]),
            fetched(3, 80000002),
        ]]);
        assert!(r.accounts.is_empty(), "1:1 is not evidence");
    }

    #[test]
    fn a_character_claimed_by_two_surviving_accounts_is_dropped_from_both() {
        let r = parse_logs(&[vec![
            fetching(&[90000001, 90000002, 90000003]),
            fetched(3, 80000001),
            fetching(&[90000003, 90000004, 90000005]), // 90000003 in both
            fetched(3, 80000002),
        ]]);
        assert_eq!(chars(&r, 80000001), vec![90000001, 90000002]);
        assert_eq!(chars(&r, 80000002), vec![90000004, 90000005]);
    }

    #[test]
    fn a_count_that_disagrees_with_the_id_list_is_ignored() {
        let r = parse_logs(&[vec![fetching(&[90000001, 90000002, 90000003]), fetched(2, 80000001)]]);
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
        let r = parse_logs(&[lines]);
        assert_eq!(chars(&r, 80000001), vec![90000001, 90000002, 90000007]);
    }

    #[test]
    fn no_lines_yield_an_empty_roster() {
        assert_eq!(parse_logs(&[]), LauncherRoster::default());
        assert_eq!(parse_logs(&[vec![]]), LauncherRoster::default());
    }

    #[test]
    fn unrelated_lines_alone_yield_an_empty_roster() {
        assert_eq!(parse_logs(&[vec![noise(), noise()]]), LauncherRoster::default());
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
    fn an_uppercase_extension_is_still_a_log() {
        let d = temp_dir("case");
        fs::create_dir_all(&d).unwrap();
        fs::write(
            d.join("eve-online-launcher-2026.01.01-10.00.00.LOG"),
            format!("{}\n{}\n", fetching(&[90000001, 90000002, 90000003]), fetched(3, 80000001)),
        )
        .unwrap();
        assert_eq!(chars(&read_roster_from(&d), 80000001), vec![90000001, 90000002, 90000003]);
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
