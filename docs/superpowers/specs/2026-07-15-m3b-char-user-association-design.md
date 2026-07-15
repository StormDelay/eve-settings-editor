# M3b — Character/user association (design)

Date: 2026-07-15
Status: approved, pre-plan
Builds on: M1 (discovery, app shell), M3a (ESI name resolution, the shared
names store), design spec §6 "Name display & resolution" items 2 (account
aliases) and 3 (correlation suggestions).

Second of the three M3 sub-milestones: **M3a — ESI names** (done) → **M3b —
char/user association** (this doc) → **overview editor** (consumes the
char↔user pairing this milestone produces; two-slot `char`/`user` app state,
approach A). Packaging and the autofill editor are their own later milestones.

## 1. Goal

Accounts (`core_user_<id>.dat`) have no public name API, so this milestone lets
the user **name accounts** and **associate characters with their account**, and
persists both. It produces the char↔user pairing the overview editor needs, and
makes account files read as their alias instead of a bare `core_user_<id>`
wherever an account id shows (§6).

Three evidence tiers, weakest to strongest:

1. **Passive suggestions** — a labelled *guess* from an in-file name match
   (primary) plus mtime recency (corroborator). Never authoritative.
2. **Manual assignment** — the user directly picks/overrides a character's
   account.
3. **Guided capture** — a controlled-logout calibration that yields a
   ground-truth pairing.

This milestone adds **no** network behavior; it reuses the ESI names M3a already
cached, and otherwise reads only local files.

## 2. The two correlation signals (and why name-match is primary)

- **In-file name match — primary, durable.** A `core_user` file's text
  accumulates its characters' *names* (M0 finding: capsule/container window ids
  embed them), regardless of when a character last played. So a character whose
  resolved ESI name appears in exactly one user file's strings is probably on
  that account. It is only *probably* — a name can appear for unrelated reasons
  (contacts, chat) — so it is always shown as a guess.
- **mtime recency — corroborator, recency-limited.** A user file's mtime is its
  *last* logout; only the character from that most-recent session shares the
  timestamp. Characters played long ago will not mtime-match. So mtime is a
  strong "same session" signal but only for recent activity — it corroborates a
  name match and raises confidence; it is weak alone.

Because the heuristic only needs to **anchor one** character per account (the
user can read the remaining slots off the in-game account roster and fill them
in one click each, §5.1), suggestion quality is a convenience, not a
correctness requirement.

Confidence: name-match **and** mtime agreement → `high`; either alone →
`medium`/`low`. Name matches are computed across **all** discovered user files
(global); mtime clustering compares files **within one profile**.

## 3. Data model & persistence

**Only ground truth is persisted; suggestions are always recomputed live** (no
stale-suggestion invalidation).

A new `accounts.json` in the app-data dir (next to `names-cache.json`), keyed by
**global EVE ids** — character/account ids are unique across profiles and
installs, so an association is global, not per-profile:

```json
{
  "accounts": {
    "<user_id>": { "alias": "Main", "characters": [<char_id>, <char_id>] }
  }
}
```

- A character belongs to **at most one** account. Confirming X→U removes X from
  any other account (single-membership invariant, enforced in Rust).
- An account holds **at most 3** characters — a hard cap that has always existed
  in EVE. A 4th confirm is rejected with an error the UI surfaces.
  `MAX_CHARS_PER_ACCOUNT = 3` is a named constant, but enforced as a hard limit.
- `alias` optional; its absence → the UI falls back to `core_user_<id>`.
- Durable write (temp file + rename); missing/corrupt file → empty store, same
  as the M3a names cache.

## 4. Architecture — Rust owns the engine (`accounts.rs`)

Chosen over a frontend-owned engine (B) and a hybrid (C): the heuristic is pure
logic over `{id, kind, mtime, contents}`, needs the `settings-model` parser for
the in-file scan anyway, and its state must live in the app-data dir — all of
which want one Rust module. Mirrors the M3a `names.rs` split: pure ranking +
persistence, testable without a Tauri runtime or the filesystem, behind injected
seams. `blue-marshal` and `settings-model` stay dependency-free — this all lives
in the `app` crate.

### 4.1 `app/src-tauri/src/accounts.rs` (new module)

Persisted state:

```rust
struct AccountsStore { accounts: HashMap<u64 /*user_id*/, Account> }
struct Account { alias: Option<String>, characters: Vec<u64> }
```

- `load(dir) / save(dir, &store)` — JSON round-trip; missing/corrupt → empty;
  durable write. No `AppState` field for the store (read-modify-write per call,
  never concurrent in a single-user UI) — same rationale as the names cache.
- **Suggestion ranking is a pure function**, FS-free:

  ```rust
  fn rank(
      files: &[FileMeta],                    // {id, kind, mtime, profile_dir}
      user_texts: &HashMap<u64, Vec<String>>,// user_id → its string leaves
      names: &HashMap<u64, String>,          // char_id → resolved name (from M3a cache)
      store: &AccountsStore,                 // to exclude already-confirmed chars
  ) -> Vec<Suggestion>                        // {char_id, user_id, confidence, basis}
  ```

  Gathering `user_texts` (parse each user file, collect string leaves) and
  `files`/`names` is the impure caller's job (injected in tests), exactly like
  the `names.rs` fetcher seam.
- **Capture diff is a pure function**, FS-free:

  ```rust
  fn capture_diff(baseline: &Snapshot, after: &Snapshot)
      -> CaptureResult // { changed_chars, changed_users, detected: Option<(char_id,user_id)> }
  ```

  `detected = Some((c,u))` only when exactly one char file and one user file
  advanced. `Snapshot` is `path → mtime` over discovered files.
- Mutators enforcing the invariants: `confirm(&mut store, char_id, user_id)`
  (removes char from any prior account; errors if the target already holds
  `MAX_CHARS_PER_ACCOUNT`), `unpair(&mut store, char_id)`,
  `set_alias(&mut store, user_id, Option<String>)`.

### 4.2 Command surface (`lib.rs` + `ops.rs` + `api.ts`)

Guided-capture baseline is transient session state — a new
`AppState` field `capture: Mutex<Option<Snapshot>>` (the user alt-tabs to EVE
and back with the app still open; if the app closes mid-capture the attempt is
simply abandoned).

- `account_roster() -> AccountRoster` — re-runs discovery, parses user files for
  their string leaves, reads the persisted store and the M3a names cache, and
  returns everything the view renders (below). Recomputed fresh each call.
- `set_account_alias(user_id, alias: Option<String>) -> AccountRoster`
- `confirm_pairing(char_id, user_id) -> Result<AccountRoster, ErrDto>` — the
  only fallible one (hard-cap violation → `ErrDto` the UI messages).
- `unpair(char_id) -> AccountRoster`
- `begin_capture(expected_char: Option<u64>)` — snapshot mtimes into
  `AppState.capture`, **excluding the currently-open document** (the app itself
  may write it).
- `resolve_capture() -> CaptureResult` — re-scan, diff against the stored
  snapshot, return what advanced. Persisting is a separate explicit
  `confirm_pairing` (capture *detects*, the user *confirms*).

Mutators return the fresh `AccountRoster` so the store updates in one round-trip.

```
AccountRoster {
  accounts: Vec<AccountView>,   // one per user_id in discovery ∪ persisted store
  unassigned: Vec<u64>,         // char ids with no account
}
AccountView { user_id, alias: Option<String>,
              characters: Vec<u64>,        // confirmed, ≤3
              suggestions: Vec<Suggestion> } // guesses for empty slots
Suggestion { char_id, confidence: "high"|"medium"|"low", basis: String }
```

The roster returns **ids**; the frontend maps char ids → names through the
existing M3a `names.svelte.ts` store, so no name duplication here.

## 5. Frontend

### 5.1 Accounts view (new main-pane mode)

The `Tree | Layout` tabs live inside the per-open-file `filebar`, so the
app-global Accounts view is **not** a sibling tab there. Instead `+page.svelte`
gains a top-level `mainView: "file" | "accounts"` state; the entry point is a
button in the always-visible **sidebar**. When `accounts` is active the main
pane renders `AccountsView.svelte` instead of the file editor.

`AccountsView.svelte`:

- One **account card** per account:
  - Alias — inline-editable, placeholder `core_user_<id>`, committed on
    blur/Enter (→ `set_account_alias`).
  - **Exactly three character slots** (the hard cap). Filled = char chip
    (resolved name via the names store) + unpair ✕. Empty = **＋ add character**
    → pick from the unassigned list (→ `confirm_pairing`). A passive suggestion
    renders as a *ghost* chip in an empty slot: "probably ‹name›? ✓ / ✕", ✓ →
    `confirm_pairing`, ✕ dismisses for this session (§7 deferral).
  - So once one character is anchored (capture or accepted suggestion), filling
    the sibling slots the user reads off the in-game roster is one click each.
- An **unassigned characters** strip: characters with no account; each assignable
  to an account or used to start a capture.
- Actions: **Calibrate an account** (guided capture, §5.2) · **Refresh**
  (re-fetch the roster).
- A cap-violation `ErrDto` from `confirm_pairing` shows as an inline message on
  the card.

### 5.2 Guided capture (a small stepper/dialog)

1. Optionally pre-pick the character you'll log in as (labels the instructions).
   → `begin_capture(expected_char?)`.
2. Instructions (generic for now, §8): *"Launch EVE, log in as ‹char›, change an
   account-wide setting so the account file is written, then fully log out /
   close the client. Come back and click Done."*
3. **Done** → `resolve_capture()`. Interpret `CaptureResult`:
   - `detected = (char, user)` → "‹char› ↔ account ‹U› — confirm?" →
     `confirm_pairing`, then offer *"name this account?"* inline.
   - user changed but no char (or vice versa), or nothing changed → explain
     exactly what did/didn't move and offer **Retry** (re-scan against the *same*
     baseline — the account file may just not have been written yet).
   - multiple user files advanced → ask which one to pair.

### 5.3 Aliases everywhere (§6)

A shared `app/src/lib/accounts.svelte.ts` store (mirrors `names.svelte.ts`)
holds `user_id → alias` and is loaded once, feeding:

- **Sidebar** — user files show their alias when set (currently bare
  `core_user_<id>.dat`); char files unchanged (ESI names from M3a).
- **Open-file header** (`.filebar`) — an open user file shows its alias.
- The **Accounts view** itself.

The backups panel shows no id of its own — unchanged.

## 6. Error handling & edge cases

- `accounts.json` missing/corrupt → empty store, continue; write failure ignored
  for the session (mirrors the names cache).
- Single-membership + hard 3-cap enforced in `accounts.rs`; the cap surfaces as
  an `ErrDto` on `confirm_pairing`.
- **Guided capture** excludes the currently-open document from the baseline; if
  the account file didn't advance, the flow says so and offers a retry rather
  than failing silently; multiple changed user files → user disambiguates.
- No network: name matches use whatever ESI names M3a already cached; a character
  whose name is unresolved simply contributes no match.
- Anomalous files (`id == None`, `kind == "other"`) are never accounts or
  characters — excluded from the roster.

## 7. Testing

- **Rust (`app` crate, `cargo test`, no FS, no network):**
  - `rank` over synthetic snapshots: name-match hit, ambiguous name (multiple
    user files), mtime cluster, both-agree → `high`, no-signal → nothing,
    already-confirmed char excluded.
  - `capture_diff`: one-char+one-user → `detected`; user-only / char-only /
    nothing → `detected = None` with the right `changed_*`; multiple users →
    `detected = None`.
  - Mutators: single-membership move, hard-cap rejection at the 4th, alias
    set/clear.
  - `accounts.json` persistence round-trip in a temp dir.
  Synthetic ids only (repo rule: no real ids in fixtures).
- **Frontend (`node --test`):** any non-trivial pure roster-shaping helper only;
  the view is manual-smoke (display glue over tested commands).
- **Manual smoke (live client — the calibration gate):** on real profiles,
  confirm suggestions look sane, then run **one guided capture end-to-end**
  against the live EVE client and verify the detected pairing is correct. Real
  client write behavior must be observed, like the M1 exit gate — this is also
  where the concrete §8 account-write trigger gets identified.

## 8. Out of scope / deferred

- **Exact capture trigger.** The instruction stays generic ("change an
  account-wide setting") for now; the tolerant detector guides a retry if the
  account file didn't move. Pinning down a specific in-game change that reliably
  rewrites `core_user` is a manual-smoke follow-up once the flow works.
- **Persisted suggestion dismissals** (`ponytail:` in-session only) — a rejected
  guess may reappear next launch until the character is assigned. Add a
  persisted dismissal set only if it proves annoying.
- **Overview editor / two-slot char↔user app state** — the next sub-milestone;
  it consumes the pairing this one persists.
- Batch-apply source/target lists (M4) already benefit from aliases via the
  shared store, but their UI is M4.
