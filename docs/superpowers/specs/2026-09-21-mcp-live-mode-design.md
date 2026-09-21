# MCP live mode and workspaces (design)

Status: designed 2026-09-21.

Milestone context: the MCP server (`2026-09-19-mcp-server-design.md`) is
**headless** — `app.exe --mcp` is its own process with its own `AppState`, and
the window is another. They share the disk and nothing else. Two things follow
that this design removes:

1. An assistant cannot work *in the user's session*. The user editing in the
   window and asking "now do X for me" gets an edit on a private copy, invisible
   in the window, and a save that the window then sees as a conflict.
2. The server holds **one** character and its account at a time. `open`
   replaces the session, so reading a second character means dropping the
   first, and reading five means five reloads.

The 2026-09-19 spec chose headless because "no backend→frontend refresh
channel exists today" and deferred **live mode** (§11). This is that slice,
plus the multi-file **workspaces** it turns out live mode needs anyway.

## 1. Goal

- **Workspaces**: the server holds one `AppState` per account, each with its
  character slot swappable among that account's characters. `open` *selects*;
  it never discards. Reading across the whole roster is free. Works headless,
  with no window running.
- **Live mode**: when the window is running, the server runs *inside it* over
  the window's own `AppState`. The assistant's edits land in the user's open
  document, the window refreshes, one undo stack serves both, and the user's
  Ctrl+Z undoes the assistant's last step.
- **One writable account file**: at any moment exactly one in-memory copy of a
  given `core_user_<id>.dat` can be dirty — the window's when the window has
  that account open, one private workspace otherwise. Save conflicts between
  the assistant and the user become impossible by construction; the `conflict`
  check is left to catch what it was built for (the game client, a batch copy).
- Nothing changes for MCP clients: the same `app.exe --mcp` command line, the
  same tool set, the same config snippet in the AI access sheet.

Non-goals: a second in-window editor surface; per-tool permissions; an HTTP
transport (still §11 of the 2026-09-19 spec).

## 2. Decisions

| Decision | Over | Because |
|---|---|---|
| **`AppState` becomes a cloneable `Arc` handle** (`struct AppState(Arc<Inner>)` + `Deref`) | Tauri managing `Arc<AppState>`; a `&'static` leak | `ops.rs` (2684 lines of `state.char.lock()`), every `lib.rs` command and every test compile unchanged. The window and the in-window server hold clones of one handle. |
| **Workspaces keyed by account** (canonical `core_user` path), character slot swapped within | One `AppState` per character | Siblings share one account file; one workspace per account means one in-memory copy of it, which is the whole conflict story. A char swap is exactly what today's `open` does when the account matches. |
| **`--mcp` relays to the window over a named pipe** when the window is up, serves headless otherwise | Streamable-HTTP in the window; a sidecar | Every client keeps spawning `app.exe --mcp`; no config change, and Claude Desktop's "cannot reach local HTTP" objection never arises. The relay is a byte copy — it does not parse MCP. |
| **The window's `AppState` is the account's workspace** while the window has that account open; a sibling opened privately gets a **read-only account slot** | Refusing to open siblings of the window's character; a shared account `Document` across workspaces | Reads stay free. The refusal is enforced by `Document.fidelity = ReadOnly { reason }` and the guard every write op already routes through (`ops.rs:161`) — no per-tool table. A shared `Document` breaks undo, whose entries snapshot both slots. |
| **Refresh = emit `undo::outcome()` after any call that changed the window's workspace**, frontend applies it through `land()` | A file watcher; per-view events | `land()` (`undo.svelte.ts`) already replaces both trees, sets authoritative dirty flags and bumps every view's `refreshToken` — it was written for "the document changed under the UI". The emit is decided by comparing the history's counters before and after the call, not by a list of mutating tools. |
| **The assistant's `undo` in the window's workspace works only while its own step is on top** (fingerprint = history counters + depth) | Refusing `undo` in live mode; tagging undo entries by author | Entries are unlabelled snapshots; a shared stack cannot say whose step is on top, but the counters pair identifies the stack position. Five lines, and the assistant can never pop the user's edit. |
| **One toast per assistant tool call, with Undo** | Silent refresh; a change log panel | The user must see what moved and be one click from rejecting it. `undoAction()` already mints a depth-checked Undo. |
| **Two PRs, one spec** | One branch | Workspaces are useful headless on their own and are unit-testable with the fixture harness; live mode adds the transport and the window, which needs a manual smoke. |

## 3. Workspaces

### 3.1 State

```rust
// ops.rs
#[derive(Clone)]
pub struct AppState(Arc<Inner>);
pub struct Inner { pub char: Mutex<…>, pub user: Mutex<…>, pub capture: Mutex<…>, pub history: Mutex<History> }
impl Deref for AppState { type Target = Inner; … }
impl AppState { pub fn new() -> Self { … } }   // unchanged signature
```

Tauri still does `.manage(AppState::new())`; commands still take
`State<'_, AppState>`. The `setup` closure clones the handle for the in-window
server (§4.2).

```rust
// mcp.rs
pub struct EveMcp {
    /// Private workspaces, by canonical account-file path. Shared by every
    /// connection of one process.
    workspaces: Arc<Mutex<HashMap<PathBuf, AppState>>>,
    /// The window's own state, in live mode. `None` headless.
    window: Option<AppState>,
    /// What this connection last `open`ed: the workspace and the paths it
    /// attached with (§3.3, `switched`).
    current: Mutex<Option<Attached>>,
    /// Live mode: called after a call that changed the window's workspace or
    /// wrote files on disk (§4.3). `None` headless and in tests that don't care.
    on_change: Option<Arc<dyn Fn(Change) + Send + Sync>>,
    dir, roots, prefs_path,   // as today
}
struct Attached { state: AppState, char: Option<PathBuf>, user: PathBuf, in_window: bool }
```

Paths are `fs::canonicalize`d before use as keys, so an explicit `user_file`
spelled with forward slashes finds the same workspace as a `char_id` lookup.
Keys are never shown to the model.

### 3.2 Which workspace serves an `open`

`open` resolves `char_id` / `char_file` / `user_file` to paths exactly as
today (`locate`), then:

1. **The window has this account open** (`window`'s user slot path == key):
   - and the window's char slot is this character, or none was asked for →
     attach to the window's state (`in_window: true`). No load happens.
   - and the window has a *different* sibling open → serve the **private**
     workspace for this account (created if needed), whose account slot is
     **read-only** (§3.5 rule 2).
2. **Otherwise** → the private workspace for this account, created on first
   use, character slot swapped on later opens (§3.3).

Headless, `window` is `None` and only branch 2 exists.

### 3.3 Character swap and reload

On an existing private workspace:

- Same character → no load. Both slots stay as they are.
- A sibling → **blocked while either slot is dirty**: error `unsaved_edits` —
  "`<A>` (or its account) has unsaved edits; `save` or `undo` them before
  opening `<B>`". Clean → `open_file(Char, sibling)` into the same `AppState`;
  the account slot stays loaded, and `open_file` clears the undo stack (its
  existing rule: an entry holds both slots' trees). This is the forced
  workflow: finish and save one character of an account before starting the
  next. One rule for both slots, and no state that outlives the character it
  was made under.
- Before returning, **each clean slot whose file changed on disk since it was
  loaded is re-read** (`Document::load` compares mtime and length, the
  conflict check's own reference). A dirty slot is never re-read. This keeps a
  read-only or idle copy current after the user saves in the window, the game
  client writes, or `copy_apply` lands — and is the same guard against the
  client rewriting files that the primer already warns about.

`current` moves to the workspace the call selected. Every later tool call
checks that the attached state's slot paths still equal `Attached.char/user`;
in live mode the window can switch files under an attached connection (and a
second client can swap a shared private workspace), and the model must never
be silently redirected to another pilot. Mismatch → error `switched`: "this
workspace now has `<C>` (account `<Y>`) open — call `open` again".

### 3.4 `status` and `save`

`status` grows to:

```json
{
  "mode": "live" | "headless",
  "window": { "char": path?, "user": path? } | null,
  "current": { "char": path?, "user": path, "in_window": bool, "account_read_only": bool,
               "dirty": { "char": bool, "user": bool }, "can_undo": bool } | null,
  "workspaces": [ { "char": path?, "user": path, "dirty": { … } }, … ]
}
```

`workspaces` lists every private workspace so an unsaved one cannot be
forgotten. Headless, `status` also reports `window_available: true` when the
pipe exists (the window started after this server): "restart the MCP server to
work in the window".

`save` gains `all: true`: save every dirty slot of every private workspace and
of `current`, reporting per file as today's per-slot report does. Without
`all` it saves `current` only, as now.

### 3.5 One account, one writable copy

1. **One private workspace per account.** Guaranteed by the map key. Siblings
   swap the character slot (§3.3); the account slot is loaded once and edited
   from there whichever sibling is open. Different accounts coexist.
2. **The window owns its account.** A private workspace whose account the
   window has open gets `user.fidelity = ReadOnly { reason }` with the reason
   "account `<X>` is open in the window as `<A>` — open `<A>` to edit
   account-side settings". Every account-side write tool (overview presets,
   tabs, appearance, columns, probes, autofill, keybinds, chat, fleet's
   account half, pack import) then fails with `read_only` and that text through
   the existing guard; character-side tools (layout, stacks, Neocom, HUD,
   fleet's character half) work; every read works. The lock is re-evaluated at
   the top of each tool call on a private workspace, so it follows the window:
   it is set when the window holds the account and the slot is clean, and
   lifted — only if this code set it — when the window leaves. A slot that was
   read-only from load (decoder round-trip failure) stays that way.
3. **Nothing else.** A dirty private account slot when the window opens that
   account (the user opened the app mid-edit, headless) is left writable: locking
   it would strand the edits. Whichever side saves second hits `conflict`, the
   window's own overwrite prompt included. `status` names the situation.

## 4. Live mode

### 4.1 Transport

```
AI client ──stdio──▶ app.exe --mcp ──named pipe──▶ running window
                        (relay)                      └─ EveMcp per connection, shared workspaces
```

- **Endpoint**: Windows `\\.\pipe\eve-settings-editor-<username>`; elsewhere
  `<app dir>/mcp.sock` created `0600`. Both are reachable only by the same
  user, which is the trust boundary the exe already has.
- **`--mcp`** (`mcp::serve`): try to connect once. Connected → copy stdin to
  the pipe and the pipe to stdout until either side closes, then exit. Not
  connected → serve headless as today. The relay never parses a byte of MCP.
- **Window** (`lib.rs` `setup`): spawn a listener on `tauri::async_runtime`.
  Per connection: `EveMcp { window: Some(state.clone()), workspaces: shared,
  on_change: Some(emit), … }.serve((reader, writer))` — rmcp's `IntoTransport`
  takes any `(AsyncRead, AsyncWrite)` pair. A connection count is emitted as
  `ai-connected` for the indicator (§4.5).
- **Failure**: the window closes → the pipe closes → the relay exits → the
  client sees its server exit and offers reconnect, which spawns a headless
  server. A second window instance fails to bind the pipe and runs without
  live mode (two instances are already unsupported).
- `tokio` gains the `net` and `io-util` features.

### 4.2 The in-window server

`mcp::serve_in_window(app: AppHandle, state: AppState)` — the listener above.
`off_runtime` (scoped thread for `names::resolve_blocking`) already keeps
blocking work off the runtime thread, so `list_characters` needs no change.
Locks: `AppState` already serialises concurrent Tauri commands with the
documented `user → char → history` order; a tool call is one more caller of
the same functions.

### 4.3 Refresh channel

Around every `call()` on the window's workspace the handler snapshots
`(history.counters(), history.saved(), history.depth())` before and after. If
it moved, `on_change(Change::Edited { tool, outcome: undo::outcome(…) })`.
`History` gains `counters()` and `saved()` accessors. Reads emit nothing;
`save` emits (dirty flags change); `undo` emits. `restore_backup` replaces
the document and resets the history, so it emits unconditionally rather than
by comparison.

Tools that write files on disk directly — `copy_apply`, `copy_files`,
`settings_preset_edit` (apply), `restore_backup` on a private workspace —
emit `Change::Wrote(paths)`.

In the window, `emit` maps these to Tauri events: `ai-edit { tool, outcome }`
and `ai-wrote { paths }`. Frontend, in `+page.svelte`:

- `ai-edit` → `land(outcome)` (exported from `undo.svelte.ts`; today module
  private) then `toast("Claude: <tool>", { action: undoAction() })`.
- `ai-wrote` → the existing `onBatchApplied(paths)`: a clean slot is re-read,
  a dirty one gets the stale banner.

`land` replaces both trees, sets dirty flags from the response and bumps
`savedAt`, so every view re-reads through the mechanism it has. A view
mid-interaction re-reads exactly as it does after a Ctrl+Z today.

### 4.4 Undo

On the window's workspace the connection records the fingerprint
`(counters, depth)` after each of its own calls. `undo` proceeds only if the
current fingerprint equals the recorded one — the top of the stack is this
connection's own step — otherwise error `window_edited`: "the window changed
the document since your last edit; undo there (Ctrl+Z), or make a new edit".
A user Ctrl+Z followed by Ctrl+Y restores the fingerprint, and the
assistant's `undo` works again, which is correct: its step is back on top.
Private workspaces keep today's unrestricted `undo`.

### 4.5 Window UX

- **Toast** per assistant tool call that changed the document, "Claude:
  `overview_tabs_edit`", with the Undo action. One call is one toast — a
  ten-op `overview_tabs_edit` batch is one call.
- **Indicator**: a dot in the context bar while at least one connection is
  attached, tooltip "AI assistant connected". Driven by `ai-connected`.
- **AI access sheet**: one added sentence — "While this app is open, the
  assistant works in your window: its changes appear as you'd made them, and
  Ctrl+Z undoes them."

### 4.6 Primer

The `## workflow` section gains three sentences: `open` selects a character
and keeps others open; an account's settings are edited through whichever of
its characters is open, and unsaved edits must be saved (or undone) before
opening a sibling; when the window is open, `status` says so, edits to the window's
character appear there, and account-side edits for the window's account go
through the window's character.

## 5. Ceilings

Each caught by an existing check, none needing code now:

- **Dirty private account slot when the window opens that account** (§3.5
  rule 3): left writable; the second save conflicts.
- **Headless server started before the window**: cannot join it; `status`
  says to restart.
- **Window closes mid-session**: relay exits; the client reconnects headless.
- **Workspace memory**: one parsed `AppState` per account the assistant has
  opened, for the process lifetime. A twenty-character roster is a few tens of
  MB. No `close` tool; add one if it bites.
- **`ai-wrote` from a private workspace's `save`** is not emitted: by rule 2
  a private workspace can only save a *character* file the window does not
  have open, or an account file the window does not hold — except the rule 3
  case above, where the window's own changed-on-disk check answers at its
  next save.

## 6. Safety

- The write path is unchanged: every save is `settings_model::save`'s
  backup → verify → conflict-check → atomic write. Live mode adds no way to
  reach the disk.
- The pipe is same-user only. Anything that can connect could already run the
  exe.
- The assistant can never replace the window's document: `open` attaches or
  serves a private workspace; the window's slots are only ever changed by the
  window's own commands.
- Every assistant edit in the window is visible (toast) and reversible (Undo,
  Ctrl+Z, backups). Every account-side conflict path between the assistant
  and the user is closed by §3.5; the remaining `conflict` sources are third
  parties, as before.

## 7. Tests

**Workspaces** (`mcp.rs` unit tests, `for_tests` + fixture files):
- Open A, then sibling B: same `AppState` (`Arc::ptr_eq`), char slot is B,
  the account slot is the same `Document` (no reload).
- A dirty slot — character or account — blocks the swap with `unsaved_edits`;
  after `save` (and after `undo`) it swaps.
- Two accounts → two workspaces; `status.workspaces` lists both with dirty
  flags; `save {all}` writes both.
- A clean slot whose file was rewritten on disk is re-read on `open`; a dirty
  one is not.
- Explicit `user_file` with a different spelling resolves to the same
  workspace.

**Live mode** (`EveMcp` built with `window: Some(state)` and an `on_change`
that collects into a `Vec`; no Tauri, no pipe):
- `open` of the window's character attaches (`ptr_eq`); a sibling gets a
  private workspace whose account-side edit fails `read_only` with the reason,
  whose character-side edit succeeds; the lock lifts when the window opens
  another account.
- The window switching files makes the next call fail `switched`.
- An edit emits one `Edited` with trees and dirty flags; a read emits nothing;
  `save` emits; `copy_apply` emits `Wrote` with the target paths.
- Undo fingerprint: edit → `undo` ok; edit → window edits → `undo` is
  `window_edited`; window Ctrl+Z then Ctrl+Y → `undo` ok.
- An in-memory `tokio::io::duplex` end-to-end: initialize, `open`, one edit,
  one `Edited` observed.

**Frontend** (vitest; `test/setup.ts` gains a mock of
`@tauri-apps/api/event` with `events.fire(name, payload)`):
- `ai-edit` replaces the trees, sets dirty flags, raises a toast with Undo.
- `ai-wrote` on a clean slot re-opens it; on a dirty slot shows the banner.
- `ai-connected` toggles the indicator.

**Relay**: manual. The scripted stdio run from slice 1 against the release
exe, once with the window closed (headless, as before) and once with it open
(edit appears in the window, toast, Ctrl+Z reverts it). Recorded in the PR.

## 8. File-by-file

| File | Change |
|---|---|
| `app/src-tauri/src/ops.rs` | `AppState` → `Arc<Inner>` newtype with `Deref`. |
| `app/src-tauri/src/undo.rs` | `counters()`, `saved()` accessors. |
| `app/src-tauri/src/mcp.rs` | Workspace map, `Attached`, `open`/`status`/`save` changes, account lock, fingerprint `undo`, `on_change` around `call()`, `serve` split into relay-or-headless. |
| `app/src-tauri/src/mcp_live.rs` (new) | Endpoint name, window listener, relay, `Change` enum. |
| `app/src-tauri/src/lib.rs` | `setup` spawns the listener with the state handle and an emitting `on_change`. |
| `app/src-tauri/src/mcp_primer.md` | Workflow sentences (§4.6). |
| `app/src-tauri/Cargo.toml` | `tokio` features `net`, `io-util`. |
| `app/src/lib/undo.svelte.ts` | Export `land`. |
| `app/src/routes/+page.svelte` | Listeners for `ai-edit`, `ai-wrote`, `ai-connected`. |
| `app/src/lib/ContextBar.svelte` | Indicator. |
| `app/src/lib/AiAccessPanel.svelte` | One sentence. |
| `app/src/lib/test/setup.ts` | Event mock. |
| `CHANGELOG.md`, `README.md` | Entries. |

## 9. Delivery

- **PR 1 — workspaces** (`feat/mcp-workspaces`): §3 without the `window`
  branch, `status`/`save all`, primer, tests. Ships on its own: the assistant
  reads the whole roster headless without reloads, and the one-writable-copy
  rule already holds among private workspaces.
- **PR 2 — live mode** (`feat/mcp-live-mode`): §4, §3.2 branch 1, §3.5 rule 2,
  the fingerprint `undo`, window UX, frontend tests, manual smoke.

## 10. Definition of done

- With the window closed, the tool set behaves as before plus §3.
- With the window open on character A: a client's `overview_tabs_edit`
  appears in the Overview view without any user action, with a toast whose
  Undo reverts it; Ctrl+Z reverts it; `save` clears the window's unsaved
  badge; the History popover shows the backup.
- The client's `open` of sibling B reads B's layout and moves B's windows,
  and its `overview_tabs_edit` on B fails with the read-only reason naming A.
- The client's `open` of a character on another account works headlessly in
  the same session, and `status` lists it.
- `npm test`, `cargo test` and clippy green **by exit code** (the `npm test`
  ceiling noted in memory applies).
- CHANGELOG entry under the next draft version.

## 11. Deferred

- **HTTP transport** — unchanged from the 2026-09-19 spec.
- **Read-only knob** for full-auto agents — one guard on the write tools,
  orthogonal to this design.
- **`close` tool** / workspace eviction — when memory bites.
- **Author-tagged undo entries** — would let the assistant undo its own step
  beneath a user edit; not worth the accounting until asked for.
- **Locking a dirty private account slot when the window arrives** — §3.5
  rule 3 leaves it to the conflict check.
