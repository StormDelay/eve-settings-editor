# MCP live mode — Implementation Plan (PR 2 of 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the EVE Settings Editor window is open, an MCP client's `app.exe --mcp` reaches a server running *inside* the window over the window's own document, so the assistant's edits appear in the user's session at once, share its undo stack, and can be reverted with Ctrl+Z.

**Architecture:** `--mcp` first tries a same-user endpoint (a Windows named pipe, a Unix socket); when the window is listening it becomes a byte relay, otherwise it serves headless as today. The window serves one `EveMcp` per connection over its managed `AppState` (a cloneable handle since PR 1) plus a workspace map shared by every connection. The window's state is the workspace for whatever the window has open; a sibling character opened privately gets a read-only account slot. After each call, the server measures whether the window's history moved and, if so, emits the same `UndoOutcome` the frontend's `landUndo()` already applies; the assistant's `undo` in the window's workspace is allowed only while its own step is on top, tracked by per-entry serials.

**Tech Stack:** Rust (`app` crate): `rmcp` 3 (`serve()` over any `AsyncRead + AsyncWrite`), `tokio` (`net`, `io-util`: named pipes / Unix sockets, `copy_bidirectional`, `join`), Tauri 2 (`Emitter`, `async_runtime`). Svelte 5 / vitest for the window.

**Spec:** `docs/superpowers/specs/2026-09-21-mcp-live-mode-design.md` — §3.2 branch 1, §3.3 (`switched`), §3.5 rules 2–3, §4, §5–§7. PR 1 (`docs/superpowers/plans/2026-09-21-mcp-workspaces.md`, merged as #101 or pending) built §3; read both. Two amendments this plan makes to the spec are in Task 7.

**Branch:** `feat/mcp-live-mode`, stacked on `feat/mcp-workspaces` (PR #101). If #101 has merged when a task runs, rebase onto `master` first.

## Global Constraints

- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass **by exit code** at every commit; `npm test` (in `app/`) passes — read its vitest summary, its exit code can be 1 with every test green.
- `mcp.rs` stdout IS the protocol in headless mode: nothing in the `app` crate prints to stdout outside tests. In the window there is no stdout to speak of; still nothing prints.
- Lock order inside `AppState` is `user → char → history`, always; no guard outlives a temporary `AppState`; the workspace map lock is never held while a workspace lock is taken.
- Error results are `{"code","message"}` via `err(code, message)`. New codes: `switched`, `in_window`, `window_edited`. Existing: `read_only` (the account lock speaks through it).
- Tool descriptions stay under ~80 words.
- No tool result carries a path except `open`, `status`, `save`, `list_backups`, `restore_backup` (`every_get_result_carries_no_paths` enforces it).
- Product copy says "assistant", not "Claude" — any MCP client may be on the other end.
- The write path is unchanged: every save is `settings_model::save`'s backup → verify → conflict-check → atomic write. Live mode adds no way to reach the disk.
- Every commit message: subject, blank line, `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`.

Test commands (repo root): one Rust test `cargo test -p app --lib <module>::tests::<name>`; the crate `cargo test -p app`; an integration test `cargo test -p app --test <name>`; frontend `cd app && npm test`.

---

### Task 1: What a connection is attached to — `Attached` and the `switched` check

**Files:**
- Modify: `app/src-tauri/src/mcp.rs` — `EveMcp` struct (~line 27), `state()` (~60), `call()` prologue (~944), `open`'s two `*self.current.lock().unwrap() = Some(…)` sites (~1457, ~1499)
- Test: `app/src-tauri/src/mcp.rs` `mod tests`

**Interfaces:**
- Consumes (PR 1): `EveMcp { workspaces: Arc<Mutex<HashMap<PathBuf, AppState>>>, current: Mutex<Option<AppState>>, dir, roots, prefs_path }`, `fn state(&self) -> AppState`, `fn canonical(&str) -> Result<PathBuf, Value>`, `fn slot_view(&AppState, Slot) -> Option<Value>`, test helpers `open_user`, `open_args(&Path, Option<&Path>) -> Args`, `empty_char_bytes()`, `overview_user_bytes()`, `temp_file`.
- Produces: `#[derive(Clone)] struct Attached { state: AppState, char: Option<PathBuf>, user: Option<PathBuf>, in_window: bool }` with `Attached::scratch()`; `current: Mutex<Option<Attached>>`; `fn attached_paths(&AppState) -> (Option<PathBuf>, Option<PathBuf>)` (canonical); `fn attached(&self) -> Option<Attached>` (a clone); `const WORKSPACE_FREE: &[&str]`; `fn check_attached(&self, tool: &str) -> Result<(), Value>`; the `switched` error.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` after `open_with_discard_reloads_both_slots_and_drops_unsaved_edits`:

```rust
    /// Spec §3.3: the workspace a connection selected can change under it —
    /// the window switches files, another connection swaps a shared
    /// workspace. The model is told, never silently redirected.
    #[test]
    fn a_workspace_that_changed_under_the_connection_reports_switched() {
        let (s, a) = open_user(&overview_user_bytes());
        let c1 = temp_file("mcp-c1", &empty_char_bytes());
        let c2 = temp_file("mcp-c2", &empty_char_bytes());
        s.call("open", &open_args(&a, Some(&c1))).unwrap();
        s.call("layout_get", &Args::new()).unwrap();

        // Something else — here: a direct open on the shared state — moves the
        // character slot to c2.
        ops::open_file(&s.state(), Slot::Char, c2.to_str().unwrap()).unwrap();
        let e = s.call("layout_get", &Args::new()).unwrap_err();
        assert_eq!(e["code"], "switched");
        assert!(e["message"].as_str().unwrap().contains(c2.file_name().unwrap().to_str().unwrap()), "{e}");

        // `status`, `open` and the workspace-free tools still answer.
        s.call("status", &Args::new()).unwrap();
        s.call("eve_guide", &args(json!({ "topic": "workflow" }))).unwrap();
        s.call("open", &open_args(&a, Some(&c2))).unwrap();
        s.call("layout_get", &Args::new()).unwrap();
    }

    /// A slot closing under the connection is a switch too.
    #[test]
    fn a_slot_closed_under_the_connection_reports_switched() {
        let (s, a) = open_user(&overview_user_bytes());
        let c1 = temp_file("mcp-c1", &empty_char_bytes());
        s.call("open", &open_args(&a, Some(&c1))).unwrap();
        ops::close_file(&s.state(), Slot::Char);
        let e = s.call("hud_get", &Args::new()).unwrap_err();
        assert_eq!(e["code"], "switched");
        assert!(e["message"].as_str().unwrap().contains("no character"), "{e}");
    }

    /// Before any `open` there is nothing attached to check: tools fail with
    /// `no_document` as they always did, never `switched`.
    #[test]
    fn the_scratch_workspace_is_never_switched() {
        let s = EveMcp::for_tests();
        assert_eq!(s.call("layout_get", &Args::new()).unwrap_err()["code"], "no_document");
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p app --lib mcp::tests::a_workspace_that_changed_under_the_connection_reports_switched` (and the other two by name)
Expected: the first two fail — `layout_get`/`hud_get` succeed or fail with `no_document` instead of `switched`; the third passes already (it pins behaviour that must survive).

- [ ] **Step 3: Implement**

In `app/src-tauri/src/mcp.rs`, replace the `current` field and add `Attached` above the struct:

```rust
/// What a connection is attached to (spec §3.1): the workspace `open` last
/// selected and the canonical paths its slots held then, so a later call can
/// tell when the workspace no longer holds those files (§3.3 `switched`).
/// `in_window` marks the window's own state (PR 2, live mode).
#[derive(Clone)]
struct Attached {
    state: AppState,
    char: Option<PathBuf>,
    user: Option<PathBuf>,
    in_window: bool,
}

impl Attached {
    /// Before any `open`: an empty state nothing is compared against, so an
    /// early tool fails with `no_document` exactly as it always did.
    fn scratch() -> Self {
        Attached { state: AppState::new(), char: None, user: None, in_window: false }
    }
}
```

```rust
    /// What `open` last selected. `None` until then; `state()` lends a
    /// scratch workspace so an early tool fails with `no_document` as before.
    current: Mutex<Option<Attached>>,
```

Replace `state()` and add `attached()`:

```rust
    fn state(&self) -> AppState {
        self.current.lock().unwrap().get_or_insert_with(Attached::scratch).state.clone()
    }

    /// The attachment, or `None` before any `open`.
    fn attached(&self) -> Option<Attached> {
        self.current.lock().unwrap().clone()
    }
```

Add these free functions next to `canonical`:

```rust
/// The canonical paths a workspace's slots hold right now — `None` for an
/// empty slot or a path that no longer canonicalizes.
fn attached_paths(state: &AppState) -> (Option<PathBuf>, Option<PathBuf>) {
    let path = |slot: Slot| {
        let guard = state.doc(slot).lock().unwrap();
        guard.as_ref().and_then(|d| std::fs::canonicalize(&d.path).ok())
    };
    (path(Slot::Char), path(Slot::User))
}

/// The name a person recognises a settings file by.
fn file_label(p: Option<&Path>) -> String {
    p.and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "no character".into())
}

/// Tools that do not read the current workspace, so a stale attachment must
/// not stop them: `open` and `status` are how a connection re-orients.
const WORKSPACE_FREE: &[&str] = &[
    "open", "status", "eve_guide", "list_characters", "groups_search", "builtin_presets",
    "settings_presets_list", "lookup_character", "overview_pack_preview", "copy_preview", "copy_apply", "copy_files",
];
```

Add to `impl EveMcp`, above `call`:

```rust
    /// Spec §3.3: refuse to act on a workspace whose slots no longer hold the
    /// files this connection attached to. Live mode makes this real — the
    /// window switches files under an attached connection — and a second
    /// connection swapping a shared private workspace does the same.
    fn check_attached(&self, tool: &str) -> Result<(), Value> {
        if WORKSPACE_FREE.contains(&tool) {
            return Ok(());
        }
        let Some(a) = self.attached() else { return Ok(()) };
        if a.user.is_none() {
            return Ok(()); // the scratch: nothing was attached to
        }
        let (char_now, user_now) = attached_paths(&a.state);
        if char_now == a.char && user_now == a.user {
            return Ok(());
        }
        Err(err(
            "switched",
            format!(
                "this workspace now has {} open (account {}) — call open again",
                file_label(char_now.as_deref()),
                file_label(user_now.as_deref()),
            ),
        ))
    }
```

At the top of `call`, before the `match name`:

```rust
        self.check_attached(name)?;
```

In `open`, replace both `*self.current.lock().unwrap() = Some(ws.clone());` lines with:

```rust
                    let (char, user) = attached_paths(&ws);
                    *self.current.lock().unwrap() = Some(Attached { state: ws.clone(), char, user, in_window: false });
```

(the discard branch) and

```rust
        let (char, user) = attached_paths(&ws);
        *self.current.lock().unwrap() = Some(Attached { state: ws.clone(), char, user, in_window: false });
```

(the common tail). Both read the paths the workspace *actually* holds after the opens, so an account-only `open` records the character it kept.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p app` then `cargo clippy --workspace --all-targets -- -D warnings`
Expected: exit 0 both; every existing test still passes (tests that populate the scratch via `ops::open_file(&s.state(), …)` are exempt from the check because the scratch's `user` is `None`).

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/mcp.rs
git commit -m "MCP: a connection knows what it attached to; switched when it changed"
```

---

### Task 2: The window's state is the account's workspace; the account lock

**Files:**
- Modify: `app/src-tauri/src/mcp.rs` — `EveMcp` struct, `new`, a new `in_window` constructor, `open` (before `let existing`), `call` prologue, `status`, `for_tests`
- Modify: `app/src-tauri/src/lib.rs:20` — `pub use ops::AppState;`
- Test: `app/src-tauri/src/mcp.rs` `mod tests`

**Interfaces:**
- Consumes: Task 1's `Attached`, `attached()`, `attached_paths`, `file_label`, `check_attached`; PR 1's `slot_view`, `canonical`, `open_slot`, `reload_stale`; `settings_model::Fidelity`.
- Produces: `pub window: Option<AppState>` on `EveMcp`; `pub fn EveMcp::in_window(dir, roots, prefs_path, window: AppState, workspaces: Arc<Mutex<HashMap<PathBuf, AppState>>>) -> Self` (Task 3 adds `on_change` to it); `const ACCOUNT_LOCK: &str`; `fn sync_account_lock(&self)`; the `in_window` error; `status` fields `mode`, `window`, `in_window`, `account_read_only`; `#[cfg(test)] fn for_live_tests(window: AppState) -> Self`; `pub use ops::AppState` at the crate root.

- [ ] **Step 1: Write the failing tests**

```rust
    /// A server inside the window (spec §3.2 branch 1): `window` is the
    /// window's own state. Tests build one over a fresh `AppState` and open
    /// files into it directly, standing in for the window's commands.
    fn live_server() -> (EveMcp, AppState) {
        let win = AppState::new();
        (EveMcp::for_live_tests(win.clone()), win)
    }

    #[test]
    fn open_of_the_windows_character_attaches_to_the_windows_state() {
        let (s, win) = live_server();
        let a = temp_file("mcp-live-a", &overview_user_bytes());
        let c1 = temp_file("mcp-live-c1", &empty_char_bytes());
        ops::open_file(&win, Slot::User, a.to_str().unwrap()).unwrap();
        ops::open_file(&win, Slot::Char, c1.to_str().unwrap()).unwrap();

        let v = s.call("open", &open_args(&a, Some(&c1))).unwrap();
        assert_eq!(v["in_window"], true);
        assert!(s.state().ptr_eq(&win), "the window's state, not a copy");
        // An edit through the server is an edit in the window's document.
        s.call("overview_columns_edit", &args(json!({ "ops": [{ "op": "set_visible", "tab": 0, "column": "TYPE", "visible": true }] }))).unwrap();
        assert!(win.history.lock().unwrap().dirty(Slot::User));

        let st = s.call("status", &Args::new()).unwrap();
        assert_eq!(st["mode"], "live");
        assert_eq!(st["in_window"], true);
        assert_eq!(st["window"]["char"], json!(c1.to_string_lossy()));
        assert_eq!(st["account_read_only"], false);
        assert!(st["workspaces"].as_array().unwrap().is_empty(), "the window's state is not a private workspace");
    }

    /// The account-only form attaches too: nothing was asked that the window
    /// does not already have open.
    #[test]
    fn open_of_the_windows_account_alone_attaches() {
        let (s, win) = live_server();
        let a = temp_file("mcp-live-a", &overview_user_bytes());
        ops::open_file(&win, Slot::User, a.to_str().unwrap()).unwrap();
        s.call("open", &open_args(&a, None)).unwrap();
        assert!(s.state().ptr_eq(&win));
    }

    /// `discard` on the window's workspace would throw the USER's edits away.
    #[test]
    fn discard_is_refused_on_the_windows_workspace() {
        let (s, win) = live_server();
        let a = temp_file("mcp-live-a", &overview_user_bytes());
        ops::open_file(&win, Slot::User, a.to_str().unwrap()).unwrap();
        let mut args = open_args(&a, None);
        args.insert("discard".into(), json!(true));
        assert_eq!(s.call("open", &args).unwrap_err()["code"], "in_window");
    }

    /// Spec §3.5 rule 2: a sibling of the window's character is a private
    /// workspace whose account slot is read-only — reads work, character-side
    /// edits work, account-side edits name the window's character.
    #[test]
    fn a_sibling_of_the_windows_character_gets_a_read_only_account_slot() {
        let (s, win) = live_server();
        let a = temp_file("mcp-live-a", &overview_user_bytes());
        let c1 = temp_file("mcp-live-c1", &empty_char_bytes());
        let c2 = temp_file("mcp-live-c2", &layout_char_bytes());
        ops::open_file(&win, Slot::User, a.to_str().unwrap()).unwrap();
        ops::open_file(&win, Slot::Char, c1.to_str().unwrap()).unwrap();

        let v = s.call("open", &open_args(&a, Some(&c2))).unwrap();
        assert!(!s.state().ptr_eq(&win), "a private workspace");
        assert_eq!(v["user"]["fidelity"]["state"], "read_only");
        let reason = v["user"]["fidelity"]["reason"].as_str().unwrap();
        assert!(reason.contains(c1.file_name().unwrap().to_str().unwrap()), "{reason}");

        s.call("overview_get", &Args::new()).unwrap();
        s.call("layout_get", &Args::new()).unwrap();
        let e = s.call("overview_columns_edit", &args(json!({ "ops": [{ "op": "set_visible", "tab": 0, "column": "TYPE", "visible": true }] }))).unwrap_err();
        assert_eq!(e["code"], "read_only");
        assert!(e["message"].as_str().unwrap().contains(c1.file_name().unwrap().to_str().unwrap()), "{e}");
        let st = s.call("status", &Args::new()).unwrap();
        assert_eq!(st["in_window"], false);
        assert_eq!(st["account_read_only"], true);

        // The window moves to another account: the lock lifts on the next call.
        let b = temp_file("mcp-live-b", &overview_user_bytes());
        ops::open_file(&win, Slot::User, b.to_str().unwrap()).unwrap();
        s.call("overview_columns_edit", &args(json!({ "ops": [{ "op": "set_visible", "tab": 0, "column": "TYPE", "visible": true }] }))).unwrap();
        assert_eq!(s.call("status", &Args::new()).unwrap()["account_read_only"], false);
    }

    /// A slot that was read-only from load stays read-only when the window
    /// leaves: only a lock this code set is ever lifted.
    #[test]
    fn a_genuinely_read_only_account_slot_is_not_unlocked_by_the_window_leaving() {
        let (s, win) = live_server();
        // The document.rs fixture: a valid stream the encoder re-emits differently.
        let a = temp_file("mcp-live-ro", &[0x7E, 0, 0, 0, 0, 0x06, 0x01]);
        let c1 = temp_file("mcp-live-c1", &empty_char_bytes());
        let c2 = temp_file("mcp-live-c2", &empty_char_bytes());
        ops::open_file(&win, Slot::User, a.to_str().unwrap()).unwrap();
        ops::open_file(&win, Slot::Char, c1.to_str().unwrap()).unwrap();
        let v = s.call("open", &open_args(&a, Some(&c2))).unwrap();
        assert!(v["user"]["fidelity"]["reason"].as_str().unwrap().contains("re-encode"));
        ops::close_file(&win, Slot::User);
        s.call("status", &Args::new()).unwrap();
        let v = s.call("open", &open_args(&a, Some(&c2))).unwrap();
        assert_eq!(v["user"]["fidelity"]["state"], "read_only");
    }

    /// The window opens the very character a private workspace holds: the
    /// private copy is no longer where the model should be.
    #[test]
    fn the_window_taking_the_private_character_reports_switched() {
        let (s, win) = live_server();
        let a = temp_file("mcp-live-a", &overview_user_bytes());
        let c1 = temp_file("mcp-live-c1", &empty_char_bytes());
        let c2 = temp_file("mcp-live-c2", &empty_char_bytes());
        ops::open_file(&win, Slot::User, a.to_str().unwrap()).unwrap();
        ops::open_file(&win, Slot::Char, c1.to_str().unwrap()).unwrap();
        s.call("open", &open_args(&a, Some(&c2))).unwrap();
        ops::open_file(&win, Slot::Char, c2.to_str().unwrap()).unwrap();
        let e = s.call("layout_get", &Args::new()).unwrap_err();
        assert_eq!(e["code"], "switched");
        assert!(e["message"].as_str().unwrap().contains("the window"), "{e}");
        let v = s.call("open", &open_args(&a, Some(&c2))).unwrap();
        assert_eq!(v["in_window"], true);
    }
```

`layout_char_bytes()` exists in `mod tests` (a character file with `windows` and `ui` dicts).

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p app --lib mcp::tests::open_of_the_windows` (and the others by name)
Expected: compile error — no `for_live_tests`.

- [ ] **Step 3: Implement**

`app/src-tauri/src/lib.rs` line 20: change `use ops::{AppState, ErrDto, OpenOutcome};` to

```rust
pub use ops::AppState;
use ops::{ErrDto, OpenOutcome};
```

(the integration test in Task 5 constructs one; nothing else about `ops` is exposed).

`mcp.rs`: add `Fidelity` to the `settings_model` import list. Add the field after `current`:

```rust
    /// Live mode (spec §4): the window's own state, the workspace for whatever
    /// the window has open. `None` headless.
    pub window: Option<AppState>,
```

`new` sets `window: None`. Add the constructor and the test one:

```rust
    /// The in-window server (spec §4.2): one per connection, over the window's
    /// state and a workspace map shared by every connection of the process.
    pub fn in_window(
        dir: PathBuf,
        roots: Vec<PathBuf>,
        prefs_path: Option<PathBuf>,
        window: AppState,
        workspaces: Arc<Mutex<HashMap<PathBuf, AppState>>>,
    ) -> Self {
        EveMcp { workspaces, current: Mutex::new(None), window: Some(window), dir, roots, prefs_path }
    }

    #[cfg(test)]
    pub(crate) fn for_live_tests(window: AppState) -> Self {
        EveMcp::in_window(std::env::temp_dir().join("eve-mcp-tests"), vec![], None, window, Arc::default())
    }
```

Add the lock constant next to `WORKSPACE_FREE`:

```rust
/// How the account lock (spec §3.5 rule 2) introduces itself in a
/// `read_only` reason. The prefix is also how `sync_account_lock` recognises
/// a lock it set, as opposed to a slot that was read-only from load.
const ACCOUNT_LOCK: &str = "the account is open in the window as ";
```

Add to `impl EveMcp`:

```rust
    /// Spec §3.5 rule 2, re-evaluated per call: a private workspace whose
    /// account the window has open gets a read-only account slot, lifted
    /// when the window leaves. Only a slot this code locked is unlocked, and a
    /// dirty slot is never locked (rule 3) — locking it would strand its edits.
    fn sync_account_lock(&self) {
        let Some(win) = &self.window else { return };
        let Some(a) = self.attached() else { return };
        if a.in_window || a.user.is_none() {
            return;
        }
        let (win_char, win_user) = attached_paths(win);
        let held = win_user == a.user;
        // History first, released before the slot lock: `user → char → history`
        // forbids the reverse, and nothing here needs both at once.
        let dirty = a.state.history.lock().unwrap().dirty(Slot::User);
        let mut guard = a.state.user.lock().unwrap();
        let Some(doc) = guard.as_mut() else { return };
        match (&doc.fidelity, held) {
            (Fidelity::Editable, true) if !dirty => {
                let who = file_label(win_char.as_deref());
                doc.fidelity = Fidelity::ReadOnly { reason: format!("{ACCOUNT_LOCK}{who} — open {who} to edit account-side settings") };
            }
            (Fidelity::ReadOnly { reason }, false) if reason.starts_with(ACCOUNT_LOCK) => doc.fidelity = Fidelity::Editable,
            _ => {}
        }
    }

    /// Whether the current attachment's account slot carries this code's lock.
    fn account_read_only(&self) -> bool {
        let Some(a) = self.attached() else { return false };
        let guard = a.state.user.lock().unwrap();
        matches!(guard.as_ref().map(|d| &d.fidelity), Some(Fidelity::ReadOnly { reason }) if reason.starts_with(ACCOUNT_LOCK))
    }
```

In `call`, the prologue becomes:

```rust
        self.check_attached(name)?;
        self.sync_account_lock();
```

In `check_attached`, add the live case after the `if char_now == a.char && user_now == a.user { return Ok(()) }` — no: add it BEFORE that comparison, since the private workspace's own paths have not changed:

```rust
        if let Some(win) = &self.window {
            if !a.in_window && attached_paths(win) == (a.char.clone(), a.user.clone()) {
                return Err(err(
                    "switched",
                    format!("the window now has {} open — call open again to work in it", file_label(a.char.as_deref())),
                ));
            }
        }
```

In `open`, right before `// ponytail: check-then-insert …` / `let existing = …`:

```rust
        // Live mode (spec §3.2 branch 1): the window's own state is the
        // workspace for whatever the window has open. A sibling of the
        // window's character falls through to a private workspace whose
        // account slot `sync_account_lock` makes read-only.
        if let Some(win) = &self.window {
            let (win_char, win_user) = attached_paths(win);
            if win_user.as_ref() == Some(&user_key) && (char_key.is_none() || win_char == char_key) {
                if discard {
                    return Err(err("in_window", "the window holds this account; its unsaved edits are the user's — Discard is theirs to press, in the window"));
                }
                *self.current.lock().unwrap() = Some(Attached { state: win.clone(), char: win_char, user: win_user, in_window: true });
                return Ok(json!({ "char": slot_view(win, Slot::Char), "user": slot_view(win, Slot::User), "in_window": true }));
            }
        }
```

At the end of `open` (the common tail), apply the lock before answering so the returned fidelity is truthful:

```rust
        let (char, user) = attached_paths(&ws);
        *self.current.lock().unwrap() = Some(Attached { state: ws.clone(), char, user, in_window: false });
        self.sync_account_lock();
        Ok(json!({ "char": slot_view(&ws, Slot::Char), "user": slot_view(&ws, Slot::User) }))
```

In `status`, add to the returned object:

```rust
            "mode": if self.window.is_some() { "live" } else { "headless" },
            "window": self.window.as_ref().map(|w| json!({
                "char": slot_view(w, Slot::Char).map(|v| v["path"].clone()),
                "user": slot_view(w, Slot::User).map(|v| v["path"].clone()),
            })),
            "in_window": self.attached().is_some_and(|a| a.in_window),
            "account_read_only": self.account_read_only(),
```

`status`'s existing `workspaces` list is the private map only — the window's state is never inserted into it, which the first test pins. Update the `status` tool description (keep under ~80 words):

```rust
            description: "What is open right now: the current character and account file paths, whether each has unsaved edits, whether undo is possible, every workspace this session holds (one per account) with its unsaved slots — and, with the app window open, mode: live plus what the window has open. Call it to re-orient in a long conversation.",
```

The test `status_with_nothing_open_is_empty_and_cannot_undo` does exact equality on `status()`: extend its expected object with `"mode": "headless", "window": null, "in_window": false, "account_read_only": false`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p app` then `cargo clippy --workspace --all-targets -- -D warnings`
Expected: exit 0 both.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/mcp.rs app/src-tauri/src/lib.rs
git commit -m "MCP live: the window's state is its account's workspace; siblings get a read-only account slot"
```

---

### Task 3: The refresh channel — `Change` events after a call that moved the window

**Files:**
- Create: `app/src-tauri/src/mcp_live.rs` (the `Change` enum only, for now; Task 5 adds the transport)
- Modify: `app/src-tauri/src/lib.rs` (`pub mod mcp_live;`), `app/src-tauri/src/undo.rs` (`History::counters`, `saved`, `top_serial`; `Entry.serial`; `outcome_of`), `app/src-tauri/src/mcp.rs` (`on_change` field, `in_window` gains the parameter, `call_tool` calls `call_observed`)
- Test: `app/src-tauri/src/mcp.rs` `mod tests`; `app/src-tauri/src/ops.rs` `mod tests` (the behavioural undo tests and their fixtures live there — `undo.rs`'s own test module holds only source-scanning tripwires)

**Interfaces:**
- Consumes: Task 2's `in_window`, `attached()`; `undo::UndoOutcome` (Serialize); `setup::TargetResult { path, ok, .. }`.
- Produces: `pub enum mcp_live::Change { Edited { tool: String, outcome: UndoOutcome }, Wrote(Vec<PathBuf>) }`; `pub type OnChange = Arc<dyn Fn(Change) + Send + Sync>`; `pub on_change: Option<OnChange>` on `EveMcp`; `in_window(dir, roots, prefs_path, window, workspaces, on_change: OnChange)`; `History::counters() -> [u64; 2]`, `saved() -> [u64; 2]`, `top_serial() -> Option<u64>`; `pub fn undo::outcome_of(&AppState) -> UndoOutcome`; `fn Fingerprint::of(&AppState)`; `fn EveMcp::call_observed(&self, name, args) -> ToolResult`; `fn written_paths(tool, &ToolResult, in_window: bool) -> Option<Vec<PathBuf>>`.

- [ ] **Step 1: Write the failing tests**

In `app/src-tauri/src/ops.rs` `mod tests`, right after `undo_reverts_a_single_slot_edit` (reuse its fixture: `temp_file`, `overview_user_bytes()`, `open_file`, `set_overview_visible`, `depth`):

```rust
    /// Serials name stack entries uniquely across undo/redo/re-edit, which the
    /// edit counters cannot: undo one step, make another edit, and the
    /// counters read the same as before the undo. Live mode's `undo` gate
    /// (mcp.rs) keys on serials for exactly that reason.
    #[test]
    fn entry_serials_are_unique_and_survive_redo() {
        let path = temp_file("undo-serials", &overview_user_bytes());
        let state = AppState::new();
        open_file(&state, Slot::User, path.to_str().unwrap()).unwrap();
        let top = |state: &AppState| state.history.lock().unwrap().top_serial();
        assert_eq!(top(&state), None);

        set_overview_visible(&state, 0, "TYPE", true).unwrap();
        let s1 = top(&state).unwrap();
        set_overview_visible(&state, 0, "TYPE", false).unwrap();
        let s2 = top(&state).unwrap();
        assert_ne!(s1, s2);

        assert!(undo::undo(&state).is_some());
        assert_eq!(top(&state), Some(s1));
        assert!(undo::redo(&state).is_some());
        assert_eq!(top(&state), Some(s2), "redo puts the same entry back");

        // Undo, then a new edit: the counters return to where they stood after
        // s2, but the entry on top is a different one.
        assert!(undo::undo(&state).is_some());
        set_overview_visible(&state, 0, "TYPE", false).unwrap();
        let s3 = top(&state).unwrap();
        assert_ne!(s3, s2, "a new edit after an undo is a new entry, whatever the counters say");
        assert_eq!(depth(&state), 2);
    }
```

`undo::undo` / `undo::redo` are already reachable in that module (`undo_reverts_a_single_slot_edit` calls `undo::undo`).

In `mcp.rs` `mod tests`:

```rust
    /// A live server whose `on_change` collects into a shared Vec.
    fn observed_live_server() -> (EveMcp, AppState, Arc<Mutex<Vec<Change>>>) {
        let win = AppState::new();
        let seen: Arc<Mutex<Vec<Change>>> = Arc::default();
        let sink = seen.clone();
        let s = EveMcp::in_window(
            std::env::temp_dir().join("eve-mcp-tests"),
            vec![],
            None,
            win.clone(),
            Arc::default(),
            Arc::new(move |c| sink.lock().unwrap().push(c)),
        );
        (s, win, seen)
    }

    fn edited_tools(seen: &Arc<Mutex<Vec<Change>>>) -> Vec<String> {
        seen.lock().unwrap().iter().filter_map(|c| match c { Change::Edited { tool, .. } => Some(tool.clone()), _ => None }).collect()
    }

    /// Spec §4.3: after a call that moved the window's history, one `Edited`
    /// with the fresh projection and dirty flags; a read emits nothing; a
    /// save emits (the flags changed); `undo` emits.
    #[test]
    fn a_call_that_changes_the_windows_document_emits_one_edited() {
        let (s, win, seen) = observed_live_server();
        let a = temp_file("mcp-live-a", &overview_user_bytes());
        ops::open_file(&win, Slot::User, a.to_str().unwrap()).unwrap();
        s.call_observed("open", &open_args(&a, None)).unwrap();
        s.call_observed("overview_get", &Args::new()).unwrap();
        assert!(seen.lock().unwrap().is_empty(), "reads and attaching emit nothing");

        s.call_observed("overview_columns_edit", &args(json!({ "ops": [{ "op": "set_visible", "tab": 0, "column": "TYPE", "visible": true }] }))).unwrap();
        assert_eq!(edited_tools(&seen), ["overview_columns_edit"]);
        match &seen.lock().unwrap()[0] {
            Change::Edited { outcome, .. } => {
                assert!(outcome.dirty.user && !outcome.dirty.char);
                assert!(outcome.user_tree.is_some() && outcome.char_tree.is_none());
                assert!(outcome.state.can_undo);
            }
            other => panic!("{other:?}"),
        }

        s.call_observed("save", &Args::new()).unwrap();
        assert_eq!(edited_tools(&seen), ["overview_columns_edit", "save"]);
        s.call_observed("undo", &Args::new()).unwrap();
        assert_eq!(edited_tools(&seen).len(), 3);
    }

    /// A private workspace is not the window: its edits emit nothing.
    #[test]
    fn a_private_workspace_edit_emits_nothing() {
        let (s, _win, seen) = observed_live_server();
        let a = temp_file("mcp-live-a", &overview_user_bytes());
        s.call_observed("open", &open_args(&a, None)).unwrap();
        s.call_observed("overview_columns_edit", &args(json!({ "ops": [{ "op": "set_visible", "tab": 0, "column": "TYPE", "visible": true }] }))).unwrap();
        assert!(seen.lock().unwrap().is_empty());
    }

    /// `copy_files` writes settings files on disk behind the window: a `Wrote`
    /// with the targets that succeeded.
    #[test]
    fn copy_files_emits_wrote_with_the_written_targets() {
        let (s, _win, seen) = observed_live_server();
        let src = temp_file("mcp-live-src", &overview_user_bytes());
        let dst = temp_file("mcp-live-dst", &overview_user_bytes());
        s.call_observed("copy_files", &args(json!({ "source": src.to_string_lossy(), "targets": [dst.to_string_lossy()] }))).unwrap();
        let wrote: Vec<PathBuf> = seen.lock().unwrap().iter().filter_map(|c| match c { Change::Wrote(p) => Some(p.clone()), _ => None }).flatten().collect();
        assert_eq!(wrote, vec![dst]);
    }

    /// `restore_backup` replaces the document; on the window's workspace it
    /// emits `Edited` whatever the counters say, on a private one `Wrote`.
    #[test]
    fn restore_backup_emits_edited_in_the_window_and_wrote_privately() {
        let (s, win, seen) = observed_live_server();
        let a = temp_file("mcp-live-a", &overview_user_bytes());
        ops::open_file(&win, Slot::User, a.to_str().unwrap()).unwrap();
        s.call_observed("open", &open_args(&a, None)).unwrap();
        s.call_observed("overview_columns_edit", &args(json!({ "ops": [{ "op": "set_visible", "tab": 0, "column": "TYPE", "visible": true }] }))).unwrap();
        s.call_observed("save", &Args::new()).unwrap();
        let backups = s.call_observed("list_backups", &args(json!({ "slot": "user" }))).unwrap();
        let backup = backups[0]["path"].as_str().unwrap().to_string();
        seen.lock().unwrap().clear();
        s.call_observed("restore_backup", &args(json!({ "slot": "user", "backup_path": backup }))).unwrap();
        assert_eq!(edited_tools(&seen), ["restore_backup"]);

        let (s, _win, seen) = observed_live_server();
        let b = temp_file("mcp-live-b", &overview_user_bytes());
        s.call_observed("open", &open_args(&b, None)).unwrap();
        s.call_observed("overview_columns_edit", &args(json!({ "ops": [{ "op": "set_visible", "tab": 0, "column": "TYPE", "visible": true }] }))).unwrap();
        s.call_observed("save", &Args::new()).unwrap();
        let backups = s.call_observed("list_backups", &args(json!({ "slot": "user" }))).unwrap();
        let backup = backups[0]["path"].as_str().unwrap().to_string();
        seen.lock().unwrap().clear();
        s.call_observed("restore_backup", &args(json!({ "slot": "user", "backup_path": backup }))).unwrap();
        assert!(matches!(&seen.lock().unwrap()[..], [Change::Wrote(p)] if p == &vec![b.clone()]));
    }
```

`list_backups`' result shape: check the existing `list_backups` test in `mod tests` for the field holding the backup path (`path`) and adjust the index if it differs. Add `use crate::mcp_live::Change;` to the test module's imports if the module's own `use` does not already bring it in.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p app --lib ops::tests::entry_serials_are_unique_and_survive_redo` and `cargo test -p app --lib mcp::tests::a_call_that_changes_the_windows_document_emits_one_edited`
Expected: compile errors — `top_serial`, `Change`, `call_observed`, the sixth `in_window` argument.

- [ ] **Step 3: Implement**

`app/src-tauri/src/undo.rs`:

In `Entry`, add a field; the one construction site is `History::capture` (`undo.rs` ~line 215, `Some(Entry { char: …, user: …, counters: self.counters })`) — add `serial: 0,` there; `push` stamps the real value:

```rust
    /// Unique across the process, never reused: undo/redo move entries, a new
    /// edit after an undo is a new entry. What live mode's `undo` gate keys on.
    serial: u64,
```

In `History`, add a field with the others (derives `Default`, so `0` to start):

```rust
    /// The next `Entry::serial`. Monotone; never restored by undo.
    next_serial: u64,
```

In `push`, after `let entry = match before { Capture::Taken(e) => e, … }` and before pushing, stamp it:

```rust
        let mut entry = entry;
        self.next_serial += 1;
        entry.serial = self.next_serial;
```

(If `entry` is already `mut`, drop the rebinding.) Add accessors in `impl History` after `depth`:

```rust
    /// Per-slot edit counters `[char, user]`, and the values they had at the
    /// last load or save: together with `depth` and `top_serial`, the
    /// fingerprint live mode compares before and after a tool call.
    pub fn counters(&self) -> [u64; 2] {
        self.counters
    }
    pub fn saved(&self) -> [u64; 2] {
        self.saved
    }
    /// The serial of the entry on top of the undo stack, `None` when empty.
    pub fn top_serial(&self) -> Option<u64> {
        self.undo.back().map(|e| e.serial)
    }
```

After `undo_state`:

```rust
/// The projection the window lands after a change it did not make (live
/// mode) — the same shape `undo`/`redo` return, without a step.
pub fn outcome_of(state: &AppState) -> UndoOutcome {
    let u = state.user.lock().unwrap();
    let c = state.char.lock().unwrap();
    let h = state.history.lock().unwrap();
    outcome(&u, &c, &h)
}
```

Create `app/src-tauri/src/mcp_live.rs`:

```rust
//! Live mode (spec §4): the pieces that join the MCP server to the running
//! window. This file holds what the window hears about (`Change`); Task 5
//! adds the endpoint the window listens on and the `--mcp` relay.

use std::path::PathBuf;
use std::sync::Arc;

use crate::undo::UndoOutcome;

/// What a tool call changed, for the window (spec §4.3).
#[derive(Debug)]
pub enum Change {
    /// The window's document moved — an edit, a save, an undo, a restore —
    /// with the projection the frontend lands through `landUndo()`.
    Edited { tool: String, outcome: UndoOutcome },
    /// Settings files written on disk behind the open documents (a batch
    /// copy, a private workspace's restore): the frontend re-reads a clean
    /// slot and flags a dirty one, as it does after a batch.
    Wrote(Vec<PathBuf>),
}

/// The window's listener for changes; `None` headless.
pub type OnChange = Arc<dyn Fn(Change) + Send + Sync>;
```

`UndoOutcome` needs `Debug` — it already derives `Debug, Serialize`. Add `pub mod mcp_live;` to `lib.rs` after `pub mod mcp;`.

`mcp.rs`: add `use crate::mcp_live::{Change, OnChange};` and the field after `window`:

```rust
    /// Live mode: told after a call that changed the window's document or
    /// wrote files on disk (spec §4.3). `None` headless.
    pub on_change: Option<OnChange>,
```

`new` sets `on_change: None`; `in_window` gains a sixth parameter `on_change: OnChange` and sets `on_change: Some(on_change)`; `for_live_tests` passes `Arc::new(|_| {})`.

Add, near `attached_paths`:

```rust
/// Where the window's history stands. Compared before and after a call: if
/// it moved, the window has something to land. Serials make a new entry
/// after an undo distinct from the one it replaced; `saved` catches a save,
/// which moves no entry.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
struct Fingerprint {
    counters: [u64; 2],
    saved: [u64; 2],
    depth: usize,
    top: Option<u64>,
}

impl Fingerprint {
    fn of(state: &AppState) -> Self {
        let h = state.history.lock().unwrap();
        Fingerprint { counters: h.counters(), saved: h.saved(), depth: h.depth(), top: h.top_serial() }
    }
}

/// The settings files a tool wrote behind the open documents, from its own
/// result: `copy_apply`/`copy_files` report each target with `ok`;
/// `restore_backup` reports its `path` — on a private workspace that file may
/// be one the window has open (a sibling character), so it is announced.
fn written_paths(tool: &str, result: &ToolResult, in_window: bool) -> Option<Vec<PathBuf>> {
    let Ok(v) = result else { return None };
    match tool {
        "copy_apply" | "copy_files" => {
            let paths: Vec<PathBuf> = v
                .as_array()?
                .iter()
                .filter(|r| r["ok"] == true)
                .filter_map(|r| r["path"].as_str().map(PathBuf::from))
                .collect();
            (!paths.is_empty()).then_some(paths)
        }
        "restore_backup" if !in_window => v["path"].as_str().map(|p| vec![PathBuf::from(p)]),
        _ => None,
    }
}
```

Add to `impl EveMcp`, above `call`:

```rust
    /// `call`, plus what the window hears about it (spec §4.3). Decided by
    /// measuring the window's history before and after — not by a list of
    /// mutating tools — so a new tool cannot ship without the window
    /// refreshing. `restore_backup` replaces the document and resets the
    /// counters, which can land on the same fingerprint, so it always emits.
    pub(crate) fn call_observed(&self, name: &str, args: &Args) -> ToolResult {
        let before = self.window.as_ref().map(Fingerprint::of);
        let result = self.call(name, args);
        if let (Some(cb), Some(win)) = (&self.on_change, &self.window) {
            let in_window = self.attached().is_some_and(|a| a.in_window);
            let moved = Some(Fingerprint::of(win)) != before;
            if moved || (name == "restore_backup" && in_window && result.is_ok()) {
                cb(Change::Edited { tool: name.to_string(), outcome: undo::outcome_of(win) });
            }
            if let Some(paths) = written_paths(name, &result, in_window) {
                cb(Change::Wrote(paths));
            }
        }
        result
    }
```

In `call_tool`, replace `self.call(&request.name, &args)` with `self.call_observed(&request.name, &args)`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p app` then `cargo clippy --workspace --all-targets -- -D warnings`
Expected: exit 0 both. `Entry.serial` is read by `top_serial`, so no dead-code warning; if `next_serial` or a helper is flagged, it is because a step above was skipped — do not `allow` it.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/mcp_live.rs app/src-tauri/src/lib.rs app/src-tauri/src/undo.rs app/src-tauri/src/ops.rs app/src-tauri/src/mcp.rs
git commit -m "MCP live: emit the window's fresh projection after a call that moved it"
```

---

### Task 4: The assistant's `undo` in the window — only its own step, only while it is on top

**Files:**
- Modify: `app/src-tauri/src/mcp.rs` — `EveMcp` struct (`own_steps`), `call_observed`, `open` (reset on attach), `undo`, the `undo` tool description
- Test: `app/src-tauri/src/mcp.rs` `mod tests`

**Interfaces:**
- Consumes: Task 3's `Fingerprint`, `History::top_serial`, `call_observed`, `attached()`.
- Produces: `own_steps: Mutex<Vec<u64>>` on `EveMcp`; the `window_edited` error.

- [ ] **Step 1: Write the failing tests**

```rust
    fn set_type_visible(s: &EveMcp, visible: bool) -> ToolResult {
        s.call_observed("overview_columns_edit", &args(json!({ "ops": [{ "op": "set_visible", "tab": 0, "column": "TYPE", "visible": visible }] })))
    }

    /// Spec §4.4: in the window's workspace, `undo` pops only this connection's
    /// own step, and only while it is on top. The user's step beneath it is
    /// never reachable; a user Ctrl+Z / Ctrl+Y that puts the assistant's
    /// step back on top makes `undo` work again.
    #[test]
    fn undo_in_the_window_is_gated_on_the_assistants_own_step_being_on_top() {
        let (s, win) = live_server();
        let a = temp_file("mcp-live-a", &overview_user_bytes());
        ops::open_file(&win, Slot::User, a.to_str().unwrap()).unwrap();
        // The user's own edit, before the assistant arrives.
        ops::set_overview_visible(&win, 0, "TYPE", true).unwrap();
        s.call_observed("open", &open_args(&a, None)).unwrap();

        // Nothing of the assistant's on the stack yet: refused, and the user's
        // step is untouched.
        let e = s.call_observed("undo", &Args::new()).unwrap_err();
        assert_eq!(e["code"], "window_edited");
        assert_eq!(undo::undo_state(&win).depth, 1);

        // Two assistant steps: both undoable, in order; then the user's is not.
        set_type_visible(&s, false).unwrap();
        set_type_visible(&s, true).unwrap();
        assert_eq!(undo::undo_state(&win).depth, 3);
        s.call_observed("undo", &Args::new()).unwrap();
        s.call_observed("undo", &Args::new()).unwrap();
        assert_eq!(undo::undo_state(&win).depth, 1);
        assert_eq!(s.call_observed("undo", &Args::new()).unwrap_err()["code"], "window_edited");

        // The user edits after the assistant: the assistant's step is buried.
        set_type_visible(&s, false).unwrap();
        ops::set_overview_visible(&win, 0, "TYPE", true).unwrap();
        assert_eq!(s.call_observed("undo", &Args::new()).unwrap_err()["code"], "window_edited");
        // The user undoes their own step: the assistant's is on top again.
        undo::undo(&win).unwrap();
        s.call_observed("undo", &Args::new()).unwrap();
    }

    /// Redo puts the same entry back: the gate recognises it.
    #[test]
    fn undo_in_the_window_survives_a_user_undo_redo_of_the_assistants_step() {
        let (s, win) = live_server();
        let a = temp_file("mcp-live-a", &overview_user_bytes());
        ops::open_file(&win, Slot::User, a.to_str().unwrap()).unwrap();
        s.call_observed("open", &open_args(&a, None)).unwrap();
        set_type_visible(&s, true).unwrap();
        undo::undo(&win).unwrap();
        undo::redo(&win).unwrap();
        s.call_observed("undo", &Args::new()).unwrap();
        assert_eq!(undo::undo_state(&win).depth, 0);
    }

    /// A counters-only fingerprint would be fooled here: undo the assistant's
    /// step, make a user edit on the same slot, and the counters read the
    /// same. The serial does not.
    #[test]
    fn undo_in_the_window_is_not_fooled_by_a_user_edit_with_the_same_counters() {
        let (s, win) = live_server();
        let a = temp_file("mcp-live-a", &overview_user_bytes());
        ops::open_file(&win, Slot::User, a.to_str().unwrap()).unwrap();
        s.call_observed("open", &open_args(&a, None)).unwrap();
        set_type_visible(&s, true).unwrap();
        undo::undo(&win).unwrap();
        ops::set_overview_visible(&win, 0, "TYPE", true).unwrap();
        assert_eq!(s.call_observed("undo", &Args::new()).unwrap_err()["code"], "window_edited");
    }

    /// Private workspaces keep today's unrestricted undo.
    #[test]
    fn undo_in_a_private_workspace_is_unrestricted() {
        let (s, _win) = live_server();
        let a = temp_file("mcp-live-a", &overview_user_bytes());
        s.call_observed("open", &open_args(&a, None)).unwrap();
        ops::set_overview_visible(&s.state(), 0, "TYPE", true).unwrap();
        s.call_observed("undo", &Args::new()).unwrap();
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p app --lib mcp::tests::undo_in_the_window`
Expected: the first test fails at the first `undo` (it succeeds and pops the user's step instead of `window_edited`); the third fails the same way; the second and fourth pass already.

- [ ] **Step 3: Implement**

Add the field after `on_change`:

```rust
    /// Live mode, per connection: serials of the undo entries this connection
    /// pushed onto the WINDOW's stack, in order (spec §4.4). `undo` pops only
    /// while its own step is on top. Reset by `open`.
    own_steps: Mutex<Vec<u64>>,
```

`new` and `in_window` set `own_steps: Mutex::new(Vec::new())`.

In `call_observed`, record the step. Replace the body's `if let (Some(cb), Some(win)) …` block's surroundings so the serial bookkeeping runs whether or not there is a listener:

```rust
    pub(crate) fn call_observed(&self, name: &str, args: &Args) -> ToolResult {
        let before = self.window.as_ref().map(Fingerprint::of);
        let result = self.call(name, args);
        if let Some(win) = &self.window {
            let after = Fingerprint::of(win);
            let in_window = self.attached().is_some_and(|a| a.in_window);
            // A new entry on top after this connection's own call is its step.
            if in_window && name != "undo" && after.top != before.and_then(|b| b.top) {
                if let Some(serial) = after.top {
                    let mut own = self.own_steps.lock().unwrap();
                    own.push(serial);
                    // The stack itself holds twenty; older serials can never match.
                    if own.len() > 32 {
                        own.remove(0);
                    }
                }
            }
            if let Some(cb) = &self.on_change {
                let moved = Some(after) != before;
                if moved || (name == "restore_backup" && in_window && result.is_ok()) {
                    cb(Change::Edited { tool: name.to_string(), outcome: undo::outcome_of(win) });
                }
                if let Some(paths) = written_paths(name, &result, in_window) {
                    cb(Change::Wrote(paths));
                }
            }
        }
        result
    }
```

In `open`, both places that set `current` (the live attach in Task 2 and the private tail; the discard branch too): add `self.own_steps.lock().unwrap().clear();` right before the assignment — attaching anywhere starts the ledger afresh.

Replace `fn undo`:

```rust
    fn undo(&self) -> ToolResult {
        let st = self.state();
        if self.attached().is_some_and(|a| a.in_window) {
            // Spec §4.4: only this connection's own step, only while it is on
            // top. `rposition` lets a user Ctrl+Z that exposed an earlier own
            // step count as "on top" and forgets the steps above it.
            let top = st.history.lock().unwrap().top_serial();
            let mut own = self.own_steps.lock().unwrap();
            match top.and_then(|t| own.iter().rposition(|s| *s == t)) {
                Some(pos) => own.truncate(pos),
                None => {
                    return Err(err(
                        "window_edited",
                        "the top of the undo stack is not your step — the user edited in the window since. Undo there (Ctrl+Z) or make a new edit; only your own steps can be undone from here.",
                    ))
                }
            }
        }
        match undo::undo(&st) {
            Some(_) => self.status(),
            None => Err(err("nothing_to_undo", "the undo stack is empty")),
        }
    }
```

`own.truncate(pos)` drops the matched serial and everything above it — the step is popped by `undo::undo`, and anything above was already gone from the stack (the user undid it). Update the `undo` tool description:

```rust
            description: "Revert the last edit. The stack survives a save, so undoing past one re-dirties the slot; status shows it. One tool call is one step and a batch is one step. Returns status. Fails with `nothing_to_undo` when there is nothing to revert, and — in the window's workspace — with `window_edited` when the top step is the user's. For a saved change, use list_backups and restore_backup instead.",
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p app` then `cargo clippy --workspace --all-targets -- -D warnings`
Expected: exit 0 both.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/mcp.rs
git commit -m "MCP live: undo in the window pops only the assistant's own step"
```

---

### Task 5: The transport — the window listens, `--mcp` relays

**Files:**
- Modify: `app/src-tauri/Cargo.toml:39` (tokio features), `app/src-tauri/src/mcp_live.rs` (endpoint, listen, connect, relay, `serve_in_window`), `app/src-tauri/src/mcp.rs` (`serve`, `status` `window_available`), `app/src-tauri/src/lib.rs` (`setup` spawns the listener)
- Create: `app/src-tauri/tests/mcp_relay.rs`

**Interfaces:**
- Consumes: `EveMcp::in_window` (Tasks 2–3), `pub use ops::AppState` (Task 2), `Change` (Task 3).
- Produces: `pub fn mcp_live::endpoint() -> String`; `pub type mcp_live::Stream` (OS-specific, `AsyncRead + AsyncWrite + Send + 'static`); `pub async fn mcp_live::connect(&str) -> Option<Stream-like client>`; `pub async fn mcp_live::listen(&str, on_conn: impl Fn(Stream)) -> std::io::Result<()>`; `pub fn mcp_live::window_listening(&str) -> bool`; `pub async fn mcp_live::relay(stream) -> std::io::Result<()>`; `pub async fn mcp_live::serve_in_window(app: tauri::AppHandle, window: AppState)`; the Tauri events `ai-edit {tool, outcome}`, `ai-wrote {paths}`, `ai-connected {connections}`.

- [ ] **Step 1: Write the failing integration test**

Create `app/src-tauri/tests/mcp_relay.rs`:

```rust
//! Live mode end to end (spec §4.1): this test process plays the window —
//! it listens on a private endpoint and serves MCP over it — then spawns the
//! real exe with `--mcp` and `EVE_MCP_ENDPOINT` pointing at that endpoint.
//! The exe must relay: the `initialize` reply comes back through it, and
//! `status` says `mode: live`. Without a window (Task 5's other half) the
//! same exe serves headless, which `mcp_stdio.rs` already proves.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use app_lib::mcp::EveMcp;
use app_lib::mcp_live;
use app_lib::AppState;

fn exe() -> String {
    std::env::var("MCP_EXE").unwrap_or_else(|_| env!("CARGO_BIN_EXE_app").to_string())
}

fn request(stdin: &mut impl Write, out: &mut impl BufRead, body: &str) -> serde_json::Value {
    writeln!(stdin, "{body}").unwrap();
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON-RPC: {e}: {line}"))
}

fn private_endpoint() -> String {
    let tag = format!("eve-mcp-relay-test-{}", std::process::id());
    #[cfg(windows)]
    {
        format!(r"\\.\pipe\{tag}")
    }
    #[cfg(not(windows))]
    {
        std::env::temp_dir().join(format!("{tag}.sock")).to_string_lossy().into_owned()
    }
}

#[test]
fn the_exe_relays_to_a_listening_window() {
    let endpoint = private_endpoint();
    let listen_on = endpoint.clone();
    // The "window": a current-thread runtime on its own thread, one in-window
    // server per connection over one shared state, exactly as `serve_in_window`.
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async move {
            let window = AppState::new();
            let workspaces = Arc::default();
            let on_conn = move |stream: mcp_live::Stream| {
                let server = EveMcp::in_window(
                    std::env::temp_dir().join("eve-mcp-relay-test"),
                    vec![],
                    None,
                    window.clone(),
                    Arc::clone(&workspaces),
                    Arc::new(|_| {}),
                );
                tokio::spawn(async move {
                    if let Ok(running) = server.serve(stream).await {
                        let _ = running.waiting().await;
                    }
                });
            };
            let _ = mcp_live::listen(&listen_on, on_conn).await;
        });
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !mcp_live::window_listening(&endpoint) {
        assert!(Instant::now() < deadline, "the listener never came up on {endpoint}");
        std::thread::sleep(Duration::from_millis(20));
    }

    let mut child = Command::new(exe())
        .arg("--mcp")
        .env("EVE_MCP_ENDPOINT", &endpoint)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn --mcp");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    let init = request(
        &mut stdin,
        &mut out,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"relay-test","version":"0"}}}"#,
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "eve-settings-editor", "{init}");
    writeln!(stdin, r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#).unwrap();
    let status = request(&mut stdin, &mut out, r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"status","arguments":{}}}"#);
    let text = status["result"]["content"][0]["text"].as_str().unwrap_or_else(|| panic!("{status}"));
    let body: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(body["mode"], "live", "served by the listener, not headless: {body}");

    drop(stdin);
    let exit = child.wait().unwrap();
    assert!(exit.success(), "the relay exits cleanly when the client closes stdin: {exit}");
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p app --test mcp_relay`
Expected: compile error — `app_lib::mcp_live::listen`/`Stream`/`window_listening` do not exist.

- [ ] **Step 3: Implement**

`app/src-tauri/Cargo.toml` line 39:

```toml
tokio = { version = "1", features = ["rt", "time", "io-std", "io-util", "net"] }
```

Append to `app/src-tauri/src/mcp_live.rs` (keep the `Change` part from Task 3 at the top; update the module doc to say the file now also holds the transport):

```rust
/// The same-user endpoint the window listens on. `EVE_MCP_ENDPOINT`
/// overrides it — tests and debugging.
///
/// Windows: a named pipe. The default DACL lets other local users open it
/// for reading, so it is "writable only by this user", which is what the
/// protocol needs (nothing is said without a request). Unix: a socket in the
/// app dir, `0600`.
pub fn endpoint() -> String {
    if let Ok(e) = std::env::var("EVE_MCP_ENDPOINT") {
        return e;
    }
    #[cfg(windows)]
    {
        format!(r"\\.\pipe\eve-settings-editor-{}", std::env::var("USERNAME").unwrap_or_else(|_| "user".into()))
    }
    #[cfg(not(windows))]
    {
        crate::app_dir_base().unwrap_or_else(std::env::temp_dir).join("mcp.sock").to_string_lossy().into_owned()
    }
}

/// Whether a window is listening on `endpoint` — without connecting, which
/// would cost the window an accept. Windows enumerates `\\.\pipe\`; Unix
/// checks for the socket file (a stale one from a crash reads as listening
/// until the next window start replaces it; `connect` then fails and `--mcp`
/// serves headless, so the cost is one wrong `window_available`).
pub fn window_listening(endpoint: &str) -> bool {
    #[cfg(windows)]
    {
        let Some(name) = endpoint.strip_prefix(r"\\.\pipe\") else { return false };
        std::fs::read_dir(r"\\.\pipe\")
            .map(|it| it.flatten().any(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name)))
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        std::path::Path::new(endpoint).exists()
    }
}

/// One accepted connection, as the OS gives it. Both halves of the protocol
/// go over it; rmcp serves anything `AsyncRead + AsyncWrite`.
#[cfg(windows)]
pub type Stream = tokio::net::windows::named_pipe::NamedPipeServer;
#[cfg(not(windows))]
pub type Stream = tokio::net::UnixStream;

/// Listen on `endpoint` forever, handing each connection to `on_conn`
/// (which spawns its server and returns at once). Returns only on a bind
/// or accept error — a second window instance, typically.
#[cfg(windows)]
pub async fn listen(endpoint: &str, on_conn: impl Fn(Stream)) -> std::io::Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;
    // tokio's pattern: create the next instance before serving the connected
    // one, so a client that arrives meanwhile finds an instance to open.
    let mut server = ServerOptions::new().first_pipe_instance(true).create(endpoint)?;
    loop {
        server.connect().await?;
        let connected = server;
        server = ServerOptions::new().create(endpoint)?;
        on_conn(connected);
    }
}

#[cfg(not(windows))]
pub async fn listen(endpoint: &str, on_conn: impl Fn(Stream)) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::remove_file(endpoint);
    let listener = tokio::net::UnixListener::bind(endpoint)?;
    std::fs::set_permissions(endpoint, std::fs::Permissions::from_mode(0o600))?;
    loop {
        let (stream, _) = listener.accept().await?;
        on_conn(stream);
    }
}

/// The client half: the stream to the window, or `None` when no window
/// listens (or it cannot be reached), in which case `--mcp` serves headless.
#[cfg(windows)]
pub async fn connect(endpoint: &str) -> Option<tokio::net::windows::named_pipe::NamedPipeClient> {
    use tokio::net::windows::named_pipe::ClientOptions;
    // ERROR_PIPE_BUSY (231): the window is between `connect` and the next
    // instance. Brief retries, then give up rather than hang the client.
    for _ in 0..20 {
        match ClientOptions::new().open(endpoint) {
            Ok(c) => return Some(c),
            Err(e) if e.raw_os_error() == Some(231) => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
            Err(_) => return None,
        }
    }
    None
}

#[cfg(not(windows))]
pub async fn connect(endpoint: &str) -> Option<tokio::net::UnixStream> {
    tokio::net::UnixStream::connect(endpoint).await.ok()
}

/// `--mcp` with a window up: copy stdin to the window and the window to
/// stdout until either side closes. Not one byte of MCP is parsed here.
pub async fn relay<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(mut stream: S) -> std::io::Result<()> {
    let mut stdio = tokio::io::join(tokio::io::stdin(), tokio::io::stdout());
    tokio::io::copy_bidirectional(&mut stdio, &mut stream).await.map(|_| ())
}

/// The window's side of live mode (spec §4.2): listen, and serve one
/// `EveMcp` per connection over the window's state, forwarding `Change`s
/// and the connection count to the frontend as Tauri events.
pub async fn serve_in_window(app: tauri::AppHandle, window: crate::ops::AppState) {
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tauri::Emitter;

    let dir = crate::app_dir(&app);
    let workspaces: Arc<std::sync::Mutex<std::collections::HashMap<PathBuf, crate::ops::AppState>>> = Arc::default();
    let connections = Arc::new(AtomicUsize::new(0));
    let endpoint = endpoint();
    let on_conn = move |stream: Stream| {
        let (app, window, workspaces, connections, dir) =
            (app.clone(), window.clone(), Arc::clone(&workspaces), Arc::clone(&connections), dir.clone());
        tauri::async_runtime::spawn(async move {
            let n = connections.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = app.emit("ai-connected", json!({ "connections": n }));
            let emitter = app.clone();
            let on_change: OnChange = Arc::new(move |change| {
                let _ = match change {
                    Change::Edited { tool, outcome } => emitter.emit("ai-edit", json!({ "tool": tool, "outcome": outcome })),
                    Change::Wrote(paths) => emitter.emit("ai-wrote", json!({ "paths": paths })),
                };
            });
            let server = crate::mcp::EveMcp::in_window(dir, settings_model::default_roots(), crate::prefs::path_base(), window, workspaces, on_change);
            if let Ok(running) = server.serve(stream).await {
                let _ = running.waiting().await;
            }
            let n = connections.fetch_sub(1, Ordering::SeqCst) - 1;
            let _ = app.emit("ai-connected", json!({ "connections": n }));
        });
    };
    // A bind failure (a second window instance) means this window runs
    // without live mode; `--mcp` then serves headless for it. Not an error
    // the user can act on, so not surfaced.
    let _ = listen(&endpoint, on_conn).await;
}
```

`serve` in `mcp.rs` needs `rmcp::ServiceExt` in scope (it is: `use rmcp::{…, ServiceExt}`); `mcp_live.rs` calls `server.serve(stream)` so add `use rmcp::ServiceExt;` there. `crate::prefs::path_base` and `crate::app_dir` are both `pub(crate)` already; `crate::ops::AppState` is reachable inside the crate as `crate::ops::AppState` (the `pub use` in `lib.rs` is for the integration test).

`mcp.rs` `serve`:

```rust
/// `--mcp`: relay to the window when one is listening (spec §4.1), else run
/// headless until the client closes the pipe. Never builds a Tauri app.
pub fn serve() {
    let dir = crate::app_dir_base().unwrap_or_else(std::env::temp_dir);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(async {
        let endpoint = crate::mcp_live::endpoint();
        if let Some(stream) = crate::mcp_live::connect(&endpoint).await {
            let _ = crate::mcp_live::relay(stream).await;
            return;
        }
        let server = EveMcp::new(dir, settings_model::default_roots(), prefs::path_base());
        let running = server.serve(rmcp::transport::stdio()).await.expect("mcp initialize");
        let _ = running.waiting().await;
    });
}
```

`status`, headless only, after `"account_read_only"`:

```rust
            "window_available": self.window.is_none() && crate::mcp_live::window_listening(&crate::mcp_live::endpoint()),
```

The exact-equality test `status_with_nothing_open_is_empty_and_cannot_undo` gains `"window_available": false` — set `EVE_MCP_ENDPOINT` in that test to a name nothing listens on first: `std::env::set_var("EVE_MCP_ENDPOINT", r"\\.\pipe\eve-mcp-nobody")` is process-global and other tests run in parallel; instead make `window_listening` take the endpoint (it does) and have `status` read it through a small indirection the test can pin: add `#[cfg(test)]` is the wrong tool here. Ruling for the implementer: in that test, assert the other fields individually and assert `v["window_available"].is_boolean()` — the relay integration test is what proves `true`.

`lib.rs` `setup` closure, before `Ok(())`:

```rust
            // Live mode (MCP spec §4): serve MCP over the same-user endpoint,
            // one server per connection, over this window's own state.
            let handle = app.handle().clone();
            let state = app.state::<AppState>().inner().clone();
            tauri::async_runtime::spawn(mcp_live::serve_in_window(handle, state));
```

(`app.state::<AppState>()` needs `tauri::Manager`, already imported.)

Capabilities: `app/src-tauri/capabilities/default.json` lists `core:default`, which includes `core:event:default` (listen/unlisten). Nothing to add; verify by reading the file and say so in the report.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p app --test mcp_relay` (builds the exe, then runs); `cargo test -p app`; `cargo clippy --workspace --all-targets -- -D warnings`
Expected: exit 0 all. If the relay test fails with `os error 2` on connect, the exe read a different endpoint than the test listened on — the env var must reach the child (`.env(…)` is set above) and `endpoint()` must check it first. If the test hangs after `initialize`, the relay is not flushing: `copy_bidirectional` writes through as it reads, so check that rmcp's server actually ran (`serve(stream)` returned `Ok`) — `stderr(Stdio::inherit())` shows a panic.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/Cargo.toml app/src-tauri/src/mcp_live.rs app/src-tauri/src/mcp.rs app/src-tauri/src/lib.rs app/src-tauri/tests/mcp_relay.rs
git commit -m "MCP live: the window listens; --mcp relays to it and serves headless otherwise"
```

---

### Task 6: The window hears about it — listeners, toast, indicator

**Files:**
- Modify: `app/src/lib/test/setup.ts` (mock `@tauri-apps/api/event`), `app/src/lib/undo.svelte.ts` (export `landUndo`), `app/src/routes/+page.svelte` (three listeners, `aiConnections`), `app/src/lib/ContextBar.svelte` (`aiConnected` prop + chip), `app/src/lib/AiAccessPanel.svelte` (one sentence)
- Test: `app/src/routes/page.spec.ts`

**Interfaces:**
- Consumes: Task 5's events `ai-edit { tool, outcome: UndoOutcome }`, `ai-wrote { paths: string[] }`, `ai-connected { connections: number }`; existing `onBatchApplied(written: string[])` in `+page.svelte`, `undoAction()`, `toast()`, `UndoOutcome` in `api.ts`.
- Produces: `export function landUndo(r: UndoOutcome | null): boolean` in `undo.svelte.ts`; `export const events` in `test/setup.ts` with `fire(name, payload)` and `reset()`; ContextBar prop `aiConnected?: boolean`.

- [ ] **Step 1: The event mock**

In `app/src/lib/test/setup.ts`, after the `@tauri-apps/api/core` mock:

```ts
// Tauri events, the other boundary a browser test cannot cross. `listen`
// registers; `events.fire` plays a backend push — the in-window MCP server's
// `ai-edit` / `ai-wrote` / `ai-connected` — so a test drives the shell the
// way the backend does.
type Handler = (event: { payload: unknown }) => void;
class Events {
  private handlers = new Map<string, Set<Handler>>();
  on(name: string, cb: Handler) {
    if (!this.handlers.has(name)) this.handlers.set(name, new Set());
    this.handlers.get(name)!.add(cb);
  }
  off(name: string, cb: Handler) {
    this.handlers.get(name)?.delete(cb);
  }
  fire(name: string, payload: unknown) {
    for (const cb of this.handlers.get(name) ?? []) cb({ payload });
  }
  reset() {
    this.handlers.clear();
  }
}
export const events = new Events();

vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, cb: Handler) => {
    events.on(name, cb);
    return Promise.resolve(() => events.off(name, cb));
  },
}));
```

and in the existing `afterEach` that resets the stores, add `events.reset();`.

- [ ] **Step 2: Write the failing tests**

In `app/src/routes/page.spec.ts`, add `import { events } from "$lib/test/setup";` and, at the end of the file:

```ts
describe("live mode: the in-window assistant", () => {
  const tree2: TreeNodeData = { ...tree, display: "{edited}" };
  const outcome = (dirtyChar: boolean) => ({
    char_tree: tree2,
    user_tree: null,
    dirty: { char: dirtyChar, user: false },
    state: { can_undo: dirtyChar, can_redo: false, depth: dirtyChar ? 1 : 0 },
  });

  test("ai-edit lands the fresh projection, marks the slot dirty and toasts with Undo", async () => {
    calls.stub("open_file", opened("core_char_950.dat"));
    calls.stub("undo_state", { can_undo: true, can_redo: false, depth: 1 });
    await mount([both]);
    await openFile("core_char_950.dat");
    await waitFor(() => expect(subject.slots.char?.status).toBe("opened"));
    const before = subject.savedAt;

    events.fire("ai-edit", { tool: "layout_edit", outcome: outcome(true) });

    await waitFor(() => expect(subject.dirty.char).toBe(true));
    expect(subject.slots.char?.status === "opened" && subject.slots.char.tree.display).toBe("{edited}");
    expect(subject.savedAt).toBe(before + 1);
    const t = toasts.find((t) => t.message === "Assistant: layout_edit");
    expect(t?.action?.label).toBe("Undo");
  });

  test("ai-wrote re-reads a clean slot the assistant wrote behind the window", async () => {
    calls.stub("open_file", opened("core_char_950.dat"));
    await mount([both]);
    await openFile("core_char_950.dat");
    await waitFor(() => expect(calls.of("open_file").length).toBe(1));

    events.fire("ai-wrote", { paths: ["/eve/core_char_950.dat"] });

    await waitFor(() => expect(calls.of("open_file").length).toBe(2));
    expect(calls.of("open_file")[1].args).toMatchObject({ slot: "char", path: "/eve/core_char_950.dat" });
  });

  test("ai-connected shows the indicator while a connection is attached", async () => {
    await mount([both]);
    expect(screen.queryByTitle("AI assistant connected")).toBeNull();
    events.fire("ai-connected", { connections: 1 });
    expect(await screen.findByTitle("AI assistant connected")).toBeTruthy();
    events.fire("ai-connected", { connections: 0 });
    await waitFor(() => expect(screen.queryByTitle("AI assistant connected")).toBeNull());
  });
});
```

`both`, `tree`, `opened`, `mount`, `openFile` and `toasts` are already in this spec file; check `both` is the profile fixture holding `core_char_950.dat` (the "routing" tests use it) and reuse whatever name it has.

- [ ] **Step 3: Run them to verify they fail**

Run: `cd app && npx vitest run src/routes/page.spec.ts -t "live mode"`
Expected: all three fail — nothing listens for the events (the first two time out in `waitFor`; the third finds no indicator).

- [ ] **Step 4: Implement**

`app/src/lib/undo.svelte.ts`: rename `function land(` to `export function landUndo(` and its two callers (`doUndo`, `doRedo`) from `land(` to `landUndo(`. Update its doc comment's first line to: "Apply one step's outcome to the shell — ours, or one the in-window assistant made (`ai-edit`)."

`app/src/routes/+page.svelte`: add to the imports

```ts
  import { listen } from "@tauri-apps/api/event";
  import { landUndo, noteEdit, undoAction } from "$lib/undo.svelte";
  import type { UndoOutcome } from "$lib/api";
```

(replacing the existing `import { noteEdit } from "$lib/undo.svelte";`). After `void loadPrefs();`:

```ts
  // Live mode (MCP spec §4.3): an assistant working inside this window sends
  // what it changed. `ai-edit` is the shape undo lands; `ai-wrote` is a batch
  // copy behind the open documents, which the batch view's handler already
  // covers; `ai-connected` drives the context-bar indicator.
  let aiConnections = $state(0);
  void listen<{ tool: string; outcome: UndoOutcome }>("ai-edit", (e) => {
    landUndo(e.payload.outcome);
    toast(`Assistant: ${e.payload.tool}`, { action: undoAction() });
  });
  void listen<{ paths: string[] }>("ai-wrote", (e) => void onBatchApplied(e.payload.paths));
  void listen<{ connections: number }>("ai-connected", (e) => {
    aiConnections = e.payload.connections;
  });
```

`onBatchApplied` is declared later in the same script with `async function`, which hoists — the listener callback runs long after mount anyway. Pass the flag to the bar: find `<ContextBar` and add `aiConnected={aiConnections > 0}`.

`app/src/lib/ContextBar.svelte`: add to the destructured props `aiConnected = false,` and to the type block `aiConnected?: boolean;`. After the `preset` chip:

```svelte
  {#if aiConnected}
    <Chip tone="info" size="sm" title="AI assistant connected">AI</Chip>
  {/if}
```

`app/src/lib/AiAccessPanel.svelte`: after the sentence ending "Every change goes through the same backups and checks as editing here." add, in the same `<p>`:

```
    While this app is open, the assistant works in your window: its changes appear as if you'd made them, each with an Undo, and Ctrl+Z reverts them.
```

- [ ] **Step 5: Run the tests**

Run: `cd app && npm run check && npm test`
Expected: `check` exit 0; every vitest file green in the summary (the memory note: the exit code can be 1 with all green — read the summary lines). If `AiAccessPanel.spec.ts` asserts the intro paragraph's text verbatim, update that assertion to the new sentence.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/test/setup.ts app/src/lib/undo.svelte.ts app/src/routes/+page.svelte app/src/lib/ContextBar.svelte app/src/lib/AiAccessPanel.svelte app/src/routes/page.spec.ts
git commit -m "Window: land the assistant's edits, toast them with Undo, show when one is connected"
```

(add `app/src/lib/AiAccessPanel.spec.ts` if it changed.)

---

### Task 7: Primer, spec amendments, changelog, README

**Files:**
- Modify: `app/src-tauri/src/mcp_primer.md` (`## workflow`), `docs/superpowers/specs/2026-09-21-mcp-live-mode-design.md` (§4.1, §4.4, §4.5), `CHANGELOG.md`, `README.md`

- [ ] **Step 1: Primer**

In `## workflow`, after the paragraph that begins "`open` selects:", add:

```markdown
When the app window is open, this server runs inside it: `status` says `mode: live` and shows what the window has open. Opening the window's character works in the user's own session — every edit appears there at once and the user can Ctrl+Z it; `undo` from here works only while your own step is on top (`window_edited` otherwise). Account-side settings of the window's account are edited through the window's character; a sibling opened privately has a read-only account slot that names the character to open instead. If the window switches files under you, the next call says `switched` — call `open` again.
```

- [ ] **Step 2: Spec amendments**

`docs/superpowers/specs/2026-09-21-mcp-live-mode-design.md`:

§4.1, the **Endpoint** bullet: replace "Both are reachable only by the same user, which is the trust boundary the exe already has." with "The socket is `0600`; the pipe's default DACL lets other local users open it read-only, and the protocol says nothing without a request — so both are writable only by this user, which is the trust boundary the exe already has. `EVE_MCP_ENDPOINT` overrides the name (tests)."

§4.4, replace the paragraph with:

```markdown
Every undo entry carries a **serial** (monotone, never reused; undo and redo move entries, a new edit after an undo is a new entry). On the window's workspace the connection keeps the serials of the steps its own calls pushed; `undo` proceeds only if the top entry's serial is one of them — then pops it and forgets any of its steps that were above it — otherwise error `window_edited`: "the top of the undo stack is not your step — the user edited in the window since. Undo there (Ctrl+Z) or make a new edit". A user Ctrl+Z followed by Ctrl+Y puts the same entry back, and the assistant's `undo` works again. Edit counters would not do: undo a step and make another edit on the same slot and the counters read the same as before, while the entry on top is somebody else's. Private workspaces keep today's unrestricted `undo`.
```

§4.5, the **Toast** bullet: "Claude: `overview_tabs_edit`" → "Assistant: `overview_tabs_edit`" (any MCP client may be on the other end).

- [ ] **Step 3: Changelog and README**

`CHANGELOG.md`, under `## [0.38.0]` → `### Added`, append:

```markdown
- **The assistant works in your open window.** While the app is running, an AI assistant's changes land in the window as if you had made them — each announced with an Undo, and Ctrl+Z reverts them. Nothing changes in your MCP client's setup.
```

`README.md`, `## AI access`, after the workspace sentence added in PR 1, add:

```markdown
While the app is open, the assistant works inside it: its edits appear in the window at once, each with an Undo, and the window's own Ctrl+Z reverts them. The assistant's `undo` only ever reverts its own steps.
```

Preserve each file's line endings (CRLF except `CHANGELOG.md` and the spec, which are LF); the diff must show only the lines touched.

- [ ] **Step 4: Run everything**

Run: `cargo test --workspace` (the primer tests include `instructions().contains("save")`), `cargo clippy --workspace --all-targets -- -D warnings`, `cd app && npm test`
Expected: exit 0 for cargo; vitest summary all green.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/mcp_primer.md docs/superpowers/specs/2026-09-21-mcp-live-mode-design.md CHANGELOG.md README.md
git commit -m "MCP live: primer, spec amendments (serials, pipe DACL, copy), changelog, README"
```

---

### Task 8: Live smoke and PR

**Files:** none new.

- [ ] **Step 1: Build**

`cargo build -p app` (debug; the release exe is held by this session's registered server and cannot be replaced — see the PR 1 plan's Task 7 note). The window and the relay both come from `target/debug/app.exe`.

- [ ] **Step 2: Start the debug window**

Use the project's `starting-the-app` skill to launch `target/debug/app.exe` (the plain window, no `--mcp`) and open a character in it — one that is **logged out** of EVE. Leave it open. The installed release app at `G:\Eve Settings Editor\app.exe` may also be running; it does not listen on the endpoint and does not interfere.

- [ ] **Step 3: Drive the relay by hand**

From PowerShell, with the window open, one request per line through `& target/debug/app.exe --mcp` exactly as PR 1's smoke did:

1. `initialize` → `notifications/initialized` → `status`: expect `"mode":"live"` and `window.char` naming the file the window has open.
2. `open {char_id: <the window's character>}` → expect `"in_window": true`.
3. `overview_get`, then ONE reversible edit: `overview_columns_edit {ops:[{op:"set_visible", tab:0, column:"TYPE", visible:<the opposite of what overview_get showed>}]}`. In the window: the Overview view shows the change without a click, a toast "Assistant: overview_columns_edit" with an Undo button appears, the unsaved badge lights. Screenshot it (the skill has the capture recipe).
4. `undo` → the window reverts and the badge clears. Then repeat the edit and press **Ctrl+Z in the window** → it reverts; then `undo` from the client → `window_edited`.
5. `open {char_id: <a sibling on the same account>}` → `user.fidelity.state == "read_only"` with the reason naming the window's character; `layout_get` works; `overview_columns_edit` → `read_only`.
6. In the window, switch to another character → the client's next `layout_get` → `switched`.
7. Close the window → the relay process exits (the PowerShell pipeline returns).

Do not `save` from the client during the smoke; end with the window's document clean (Discard in the window if anything is left).

- [ ] **Step 4: PR**

```bash
git push -u origin feat/mcp-live-mode
```

If PR #101 is still open, `gh pr create --base feat/mcp-workspaces …`; if it merged, `git rebase master` first and `--base master`. Body (a file under the plan's SDD workspace, redact account/character ids and directory paths as PR 1's did):

```
Live mode for the MCP server: with the app window open, `app.exe --mcp` relays over a same-user named pipe to a server inside the window, over the window's own document — edits appear in the window at once with an Undo toast, Ctrl+Z reverts them, the assistant's `undo` pops only its own steps, siblings of the window's character get a read-only account slot, and a window that switches files tells the connection `switched`. Headless mode (no window) is unchanged. Spec: docs/superpowers/specs/2026-09-21-mcp-live-mode-design.md §4 (PR 2 of 2; PR 1 was #101).

Smoke (debug build, window open): <one line per step 1–7 above, what happened; the screenshot from step 3>.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
```

---

## Self-review

**Spec coverage:** §3.2 branch 1 → Task 2. §3.3 `switched` (both causes) → Tasks 1–2. §3.4 `mode`/`window`/`in_window`/`account_read_only`/`window_available` → Tasks 2, 5. §3.5 rule 2 (lock follows the window, never a dirty slot, only own locks lifted) → Task 2; rule 3 → Task 2's `!dirty` guard plus the ceiling stays as specced. §4.1 transport, endpoint, failure modes → Task 5. §4.2 in-window server, `off_runtime`, lock order → Task 5 (no change needed for `off_runtime`). §4.3 refresh (fingerprint, `restore_backup` unconditional, `Wrote`, frontend `landUndo`, `onBatchApplied`) → Tasks 3, 6. §4.4 undo → Task 4 (with the serial amendment in Task 7). §4.5 toast, indicator, sheet → Task 6 (copy says "Assistant", amended in Task 7). §4.6 primer → Task 7. §5 ceilings → unchanged by this plan; the `Wrote`-not-emitted-for-private-account-save ceiling holds because a private account slot is read-only while the window holds it. §7 tests: workspaces (PR 1) done; live tests → Tasks 2–4; duplex/relay end-to-end → Task 5's real-pipe integration test (stronger than the specced in-memory duplex); frontend three tests → Task 6; relay manual → Task 8. §10 DoD → Task 8's steps map one-to-one.

**Placeholders:** none. Two instructions tell the implementer to check an existing fixture/field name (`undo.rs` test helpers, `list_backups` result field, `both`) and use it — those are lookups, not gaps.

**Type consistency:** `Attached { state, char, user, in_window }` (T1) used in T2/T3/T4; `attached_paths(&AppState) -> (Option<PathBuf>, Option<PathBuf>)` (T1) used in T2; `file_label(Option<&Path>) -> String` (T1) used in T2; `EveMcp::in_window(dir, roots, prefs_path, window, workspaces, on_change: OnChange)` — T2 defines five parameters, T3 adds the sixth; T5's test calls the six-parameter form; `for_live_tests` passes `Arc::new(|_| {})` from T3 on; `Change::{Edited{tool, outcome}, Wrote(Vec<PathBuf>)}` (T3) used in T5; `Fingerprint::of(&AppState)` (T3) used in T4; `History::top_serial() -> Option<u64>` (T3) used in T4; `mcp_live::{endpoint, window_listening, Stream, listen, connect, relay, serve_in_window}` (T5) used by T5's test and `lib.rs`; `landUndo(UndoOutcome | null): boolean` (T6); events `ai-edit`/`ai-wrote`/`ai-connected` payload shapes identical in T5's emitter and T6's listeners.
