# Launcher-log character↔account association (design)

Date: 2026-08-13
Status: approved, pre-plan
Branch: `feat/launcher-log-association`
Revises: M3b — char/user association (`2026-07-15-m3b-char-user-association-design.md`)

## 1. Problem

The association mechanic has two paths, and both fail in ways users report:

- **Manual pick** (`AccountsView.svelte:150`) — a character is chosen into an
  account card labelled `core_user_<id>`. Nothing on screen says *which account
  that is*. On a multi-account install that is a guess, and a guess is how the
  wrong file gets associated.
- **Guided capture** (`accounts.rs:169`) — an mtime diff across a controlled
  logout. It costs one full login/logout cycle *per account*, it needs the user
  to remember an account-scoped setting change, and with a second client running
  "exactly one char file and one user file advanced" can be a coincidence.

The two reported symptoms — *the association fails*, and *it associates the
wrong account* — map onto exactly these.

## 2. Signals measured (2026-08-13)

Recorded so nobody re-derives them.

**No id cross-reference exists in the settings files.** Scanned the
`2026-07-27T131810Z_baseline` corpus (every `core_char_*`/`core_user_*` in every
profile folder) for each folder's opposite-kind ids encoded as LE32, LE64 and
decimal ASCII, in both directions: **zero hits**. Consistent with the M0 finding
that a char file does not even contain its own character id.

**The in-file name heuristic is dead.** Tested against ground truth on the live
Tranquility profile: a character's ESI name uniquely identified its account for
**4 of 27** characters. Most names appear in *all 17* user files — chat channel
labels and contact lists, not membership. M3b was right to drop it; do not
revive it.

**The EVE launcher logs the mapping in plaintext.** In
`%APPDATA%\EVE Online\logs\eve-online-launcher-*.log`:

```
[esi] Fetching character details for <char_id>, <char_id>, <char_id>
[esi] Fetched 3 character details for <user_id>
```

Measured on one real install: **186 paired observations → 10 accounts, exactly 3
characters each, fully disjoint**, and every `user_id` matched a discovered
`core_user_<id>.dat`. Logs retained back to 2023-11 (98 files, 8.1 MB), so
coverage is not limited to recent sessions.

The two lines are **not adjacent** — unrelated log lines sit between them. And
concurrent launches can interleave them: 3 of 189 *Fetching* lines had no
matching *Fetched*, and one account briefly claimed another's character set.
Both are handled in §3.

Also present and not used by this design: `[client-queue] Queued client startup
{ userId, characterId: <slot>, profile: '<name>' }`, which additionally names the
profile folder. Noted for later; the ESI pair is sufficient here.

## 3. Approach

The launcher log **proposes**; the user **confirms**. Manual pairing and guided
capture stay exactly as they are — a user whose accounts are not all on this
launcher (a second machine, a Steam install, an account not added) still has a
working path, and that escape hatch is the reason nothing here replaces them.

### 3.1 `app/src-tauri/src/launcher.rs` (new module)

Pure parse + impure reader, mirroring the `names.rs` / `accounts.rs` split.

```rust
pub struct LauncherRoster { pub accounts: HashMap<u64 /*user_id*/, Vec<u64>> }

pub fn parse_logs(lines: impl Iterator<Item = String>) -> LauncherRoster; // pure, FS-free
pub fn log_dir() -> Option<PathBuf>;                                      // per-OS
pub fn read_launcher_roster() -> LauncherRoster;                          // orchestrator
```

The complete parse rule:

1. `[esi] Fetching character details for <ids>` → hold `<ids>` as pending. A new
   *Fetching* arriving while one is still unanswered discards it **and marks the
   next *Fetched* as contested**: with two requests in flight, a *Fetched* may be
   answering either, and pairing it with whichever happens to be pending hands
   one account the other account's entire character list. Both drop.
2. `[esi] Fetched <n> character details for <user>` → if an uncontested pending
   list exists and holds exactly `n` ids, tally `sorted(ids) → user`. Clear
   pending either way.
3. **Majority vote** per id-set: the user id observed most often wins. A tie
   drops the set.
4. **Disjointness**: a character id claimed by two surviving accounts drops both
   claims.

Every mis-parse path yields *fewer* proposals. This is what makes an undocumented
log format safe to depend on: if CCP renames the lines, or changes the wording,
the result is an empty roster and the existing paths, not a bad pairing.

The three passes are defence in depth rather than one airtight rule. A long
enough interleave can still leave exactly one plausible candidate for a *Fetched*
line, which is why the vote exists and why a single reading is never trusted —
and why, ultimately, the user confirms.

`log_dir()` follows `discover.rs::default_roots()`'s shape — Windows
`%APPDATA%\EVE Online\logs` (verified against a real install); the Electron
`userData` equivalents on macOS (`~/Library/Application Support/EVE Online/logs`)
and Linux (`~/.config/EVE Online/logs`) are **inferred from Electron's standard
path mapping, not measured**. A missing directory yields an empty roster, never
an error.

### 3.2 Command surface

One new command:

```
launcher_proposals() -> Vec<Proposal>
Proposal { char_id: u64, user_id: u64, conflict: Option<u64> }
```

`conflict` carries the user id the persisted store currently holds the character
under, when that disagrees with the log.

Deliberately **not** folded into `account_roster()`: that reloads after every
alias edit and every confirm, and re-reading ~8 MB of logs on each call would be
wasteful. The Accounts view calls `launcher_proposals()` once on mount.

`accounts.json` is unchanged — no provenance field. The log is re-read live, so a
disagreement is detectable regardless of how a pairing was originally made;
storing *how* buys nothing the conflict check does not already give.

Accepting a single proposal goes through the existing `confirm_pairing`, so
single-membership and the hard 3-character cap are enforced by the code that
already enforces them.

**Accept all** needs one more command, `confirm_pairings(pairs) -> Result<AccountRoster,
ErrDto>`, applying each pair through the same `confirm` and saving once.
`confirm_pairing` re-runs discovery and rebuilds the roster per call (the
`ponytail:` note at `accounts.rs:238`); thirty of those in a row is seconds of
stall on the headline action. All-or-nothing — the first cap rejection aborts and
nothing is written.

### 3.3 Frontend — `AccountsView.svelte`

- Empty character slots render **ghost chips** from proposals: the resolved name
  plus a confirm affordance, with a card footnote naming the source ("from your
  launcher log").
- A single **Accept all** action, labelled with what it will do (e.g. "Accept all
  — 10 accounts, 30 characters"), shown when no proposal conflicts.
- A proposal that contradicts a confirmed pairing marks the filled chip and
  states the disagreement plainly — *"Your launcher log puts ‹name› on
  ‹alias or core_user_<id>›"* — offering **Move it** / **Keep mine**. This is the
  only path that repairs a user who is already mis-associated, and it never
  overwrites a deliberate choice.
- No logs found, or no proposals survive: one line pointing at the existing
  Calibrate flow. Not an error state.

Account cards, the manual picker and the capture dialog are otherwise untouched.

## 4. Error handling & edge cases

- Missing log directory, unreadable log file, or zero matching lines → empty
  proposals, UI says so, existing paths unaffected.
- Interleaved *Fetching*/*Fetched* lines → the orphaned pending is discarded
  (§3.1 step 1) and, where it still produced a tally, majority vote and the
  disjointness check remove it.
- A proposal naming a `user_id` with no discovered file and no store entry is
  still shown: `build_roster` already unions discovered ∪ persisted accounts, and
  accepting the proposal creates the entry.
- A proposal that would exceed the 3-character cap is rejected by
  `confirm_pairing` and surfaces as the existing inline card error.
- Log files are read-only; nothing in this feature writes outside
  `accounts.json`.

## 5. Testing

**Rust (`app` crate, `cargo test`, no FS, no network).** `parse_logs` over
synthetic lines — synthetic ids only, per the repo rule:

- a clean *Fetching* → *Fetched* pair with intervening unrelated lines;
- interleaved `Fetching A, Fetching B, Fetched B` → A dropped, B kept;
- majority vote resolving a 4:1 disagreement to the 4;
- a tie dropping the set;
- a character appearing in two winning sets dropping both;
- `n` disagreeing with the pending list length dropping the pair;
- no input lines → empty roster.

`log_dir()` returning `None` on a machine without the directory is covered by
`read_launcher_roster` yielding an empty roster.

**Frontend (`node --test`).** The proposals × roster merge — which slot gets a
ghost chip, which chip is marked conflicting — as a pure helper.

**Live check.** Needs **no game session**: run against the real launcher logs and
confirm the proposed accounts and characters match the roster already held. A
notable property of this path — unlike guided capture, it is verifiable without
launching EVE.

## 6. Out of scope / deferred

- **Hardening guided capture.** `finishCapture()` writes the pairing the moment
  `capture_diff` reports `detected`, without asking — contrary to the M3b spec's
  "capture *detects*, the user *confirms*". With a background client running,
  that turns a coincidence into a silent wrong write. Deliberately left for a
  follow-up once this path exists.
- **Evidence on account cards** (last EVE write time, profile folder) to make the
  manual picker less of a guess for users the launcher log does not cover.
- **`[client-queue] Queued client startup`** — carries `profile`, which could
  disambiguate which profile folder an account was launched into. Unused here.
- **Caching the parse** by `(path, mtime)`. One read per Accounts-view mount over
  ~8 MB is acceptable; revisit only if it drags.
