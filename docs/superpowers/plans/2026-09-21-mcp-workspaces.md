# MCP workspaces — Implementation Plan (PR 1 of 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The MCP server holds one workspace per account instead of one session per process: `open` selects (and keeps) rather than replaces, so an assistant reads and edits across the whole roster without reloads, with exactly one in-memory copy of any account file.

**Architecture:** `AppState` becomes a cloneable `Arc` handle (nothing in `ops.rs` changes), and `EveMcp` keeps a `HashMap<canonical core_user path, AppState>` plus a `current` pointer that every tool reads through `self.state()`. `open` resolves paths as today, then attaches to the account's workspace, swapping the character slot only when both slots are clean and re-reading a clean slot whose file changed on disk. `status` lists every workspace; `save` gains `all`. This is spec §3 minus the live-mode branches (§3.2 branch 1, §3.5 rule 2, the `switched` check), which are PR 2.

**Tech Stack:** Rust (`app` and `settings-model` crates), `serde_json`; no new dependencies.

**Spec:** `docs/superpowers/specs/2026-09-21-mcp-live-mode-design.md` — §3 and §9. Read it first; the 2026-09-19 MCP spec §3.1 holds the tool conventions.

**Branch:** `feat/mcp-workspaces`, cut from `feat/mcp-live-mode` at this plan's commit so the spec and plan travel with the PR.

## Global Constraints

- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` must pass **by exit code** (CI runs exactly these).
- `mcp.rs` stdout IS the protocol: nothing in the `app` crate may print to it outside tests.
- Tool descriptions stay under ~80 words; they load every turn.
- No tool result may contain a path except `open`, `status`, `save`, `list_backups` and `restore_backup` (the existing `every_get_result_carries_no_paths` test enforces it).
- Error results are `{"code": …, "message": …}` built with `err(code, message)`; new codes in this plan: `unsaved_edits`.
- Lock order inside `AppState` is `user → char → history`, always.
- `open`'s existing guarantees stay: a failing open leaves whatever was open untouched, unsaved edits included.

Test commands used below (from the repo root):

- One Rust test: `cargo test -p app --lib <module>::tests::<name>` (e.g. `cargo test -p app --lib mcp::tests::open_of_a_second_account_keeps_the_first`).
- One settings-model test: `cargo test -p settings-model --lib document::tests::<name>`.
- Everything: `cargo test --workspace`.

---

### Task 1: `AppState` is a cloneable handle

**Files:**
- Modify: `app/src-tauri/src/ops.rs:8-12` (imports), `app/src-tauri/src/ops.rs:36-77` (the struct and `impl`)
- Test: `app/src-tauri/src/ops.rs` (the existing `mod tests`, after `open_missing_file_is_an_io_error`)

**Interfaces:**
- Produces: `#[derive(Clone)] pub struct AppState(Arc<AppStateInner>)` with `Deref<Target = AppStateInner>`; `AppState::new() -> Self` (unchanged signature); `AppState::ptr_eq(&self, other: &AppState) -> bool`. Every `state.char` / `state.user` / `state.capture` / `state.history` access in the crate keeps compiling through `Deref`.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `app/src-tauri/src/ops.rs`, right after `open_missing_file_is_an_io_error`:

```rust
    /// Every clone is the same workspace: the window's Tauri state and the
    /// in-window MCP server (mcp.rs) must edit one document, not two copies.
    #[test]
    fn clones_share_the_documents() {
        let a = AppState::new();
        let b = a.clone();
        let path = temp_file("shared", &encode(&Value::Dict(vec![])).unwrap());
        open_file(&a, Slot::Char, path.to_str().unwrap()).unwrap();
        assert!(b.char.lock().unwrap().is_some(), "opened through one handle, visible through the other");
        assert!(a.ptr_eq(&b));
        assert!(!a.ptr_eq(&AppState::new()));
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p app --lib ops::tests::clones_share_the_documents`
Expected: compile error — `AppState` has no method `clone` / `ptr_eq`.

- [ ] **Step 3: Make `AppState` a handle**

In `app/src-tauri/src/ops.rs` change the import at line 10:

```rust
use std::sync::{Arc, Mutex};
```

Replace the struct and its `impl` (lines 36–77, from the `/// Two open documents` doc comment through the closing brace of `impl AppState`) with:

```rust
/// Two open documents (char + user, for the two-file overview category) plus a
/// transient guided-capture baseline. Each document keeps its own save chain.
///
/// A cloneable handle: every clone shares the same slots and history. The
/// window's Tauri state and the in-window MCP server (mcp.rs) edit one
/// document through two handles, and `EveMcp` holds one handle per account.
///
/// # Lock order
///
/// **`user` → `char` → `history`. Always. No function takes a slot lock while
/// holding the history lock.**
///
/// The file already had the first half of that rule and says so in three
/// places: `window_layout` locks user before the requested slot, `hud_layout`
/// notes that a consistent order "rules out lock-order-inversion deadlock
/// between concurrently invoked commands", and `set_chat_splits` re-projects
/// after its guard drops because `std::sync::Mutex` is not reentrant. Undo
/// extends it by one level.
///
/// `history` goes last because it is the only lock a function might want AFTER
/// discovering something about a document — and because putting it last means
/// `edit_reshared`, `undo` and `redo`, the three functions that need all three,
/// take them in one identical sequence with no case analysis. Skipping a level
/// is safe; reordering is not.
#[derive(Clone)]
pub struct AppState(Arc<AppStateInner>);

/// The slots behind an `AppState` handle. Public only so `Deref` can name it;
/// nothing constructs one outside `AppState::new`.
pub struct AppStateInner {
    pub char: Mutex<Option<Document>>,
    pub user: Mutex<Option<Document>>,
    pub capture: Mutex<Option<accounts::Snapshot>>,
    pub history: Mutex<crate::undo::History>,
}

impl std::ops::Deref for AppState {
    type Target = AppStateInner;
    fn deref(&self) -> &AppStateInner {
        &self.0
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        AppState(Arc::new(AppStateInner {
            char: Mutex::new(None),
            user: Mutex::new(None),
            capture: Mutex::new(None),
            history: Mutex::new(crate::undo::History::default()),
        }))
    }
    /// Whether two handles are one workspace.
    pub fn ptr_eq(&self, other: &AppState) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    fn doc(&self, slot: Slot) -> &Mutex<Option<Document>> {
        match slot {
            Slot::Char => &self.char,
            Slot::User => &self.user,
        }
    }
}
```

- [ ] **Step 4: Run the test and the whole crate**

Run: `cargo test -p app --lib ops::tests::clones_share_the_documents`
Expected: PASS.

Run: `cargo test -p app` then `cargo clippy --workspace --all-targets -- -D warnings`
Expected: both exit 0. Every `state.char.lock()` in `ops.rs`, `setup.rs`, `undo.rs`, `mcp.rs`, `lib.rs` and the tests compiles unchanged through `Deref`. If clippy names `AppStateInner` in a `private_interfaces` warning, the type is not `pub` — it must be.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/ops.rs
git commit -m "AppState is a cloneable handle"
```

---

### Task 2: `Document::changed_on_disk`

**Files:**
- Modify: `crates/settings-model/src/document.rs:44-71` (add a method to `impl Document`), `crates/settings-model/src/save.rs:53-62` (use it)
- Test: `crates/settings-model/src/document.rs` (`mod tests`)

**Interfaces:**
- Produces: `pub fn Document::changed_on_disk(&self) -> bool` — true when the file's length or mtime differs from what was recorded at load or last save; false when the file is missing (a missing original is `save`'s own error, not a change).

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `crates/settings-model/src/document.rs`, after `canonical_file_loads_editable`:

```rust
    #[test]
    fn changed_on_disk_follows_the_file_not_the_document() {
        let bytes = encode(&Value::Dict(vec![(Value::Bytes(b"k".to_vec()), Value::Int(5))])).unwrap();
        let path = temp_file("changed", &bytes);
        let doc = Document::load(&path).unwrap();
        assert!(!doc.changed_on_disk(), "freshly loaded");
        // A different length is a change whatever the clock says.
        fs::write(&path, encode(&Value::Dict(vec![])).unwrap()).unwrap();
        assert!(doc.changed_on_disk(), "rewritten with other content");
        fs::remove_file(&path).unwrap();
        assert!(!doc.changed_on_disk(), "a missing original is save's error, not a change");
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p settings-model --lib document::tests::changed_on_disk_follows_the_file_not_the_document`
Expected: compile error — no method `changed_on_disk`.

- [ ] **Step 3: Add the method and use it in `save`**

In `crates/settings-model/src/document.rs`, inside `impl Document` after `load`:

```rust
    /// Whether the file differs from what this document was loaded from (or
    /// last saved as): length or mtime moved. `save`'s conflict check, exposed
    /// so a caller can re-read a clean document before it is too late to.
    /// A missing file is not a change — `save` reports that on its own.
    pub fn changed_on_disk(&self) -> bool {
        let Ok(meta) = fs::metadata(&self.path) else { return false };
        meta.len() != self.loaded_len
            || match (meta.modified().ok(), self.loaded_mtime) {
                (Some(now), Some(then)) => now != then,
                _ => false,
            }
    }
```

In `crates/settings-model/src/save.rs`, replace lines 53–62 (from `// 3. Conflict check.` through `return Err(SaveError::Conflict); }`) with:

```rust
    // 3. Conflict check. The metadata read doubles as the missing-original
    // check, which `changed_on_disk` deliberately does not report.
    fs::metadata(&doc.path).map_err(|e| SaveError::MissingOriginal(e.to_string()))?;
    if doc.changed_on_disk() && !force_conflict {
        return Err(SaveError::Conflict);
    }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p settings-model`
Expected: exit 0 — the new test and every existing save/conflict test pass.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/document.rs crates/settings-model/src/save.rs
git commit -m "settings-model: Document::changed_on_disk, shared with save's conflict check"
```

---

### Task 3: `EveMcp` reads its state through `state()`

A pure refactor: the server keeps behaving as one session, but every tool now reaches the state through an accessor, and the fields the next task fills exist. The whole existing `mcp::tests` suite is the check.

**Files:**
- Modify: `app/src-tauri/src/mcp.rs:25-43` (struct + `new`), `:918-922` (`for_tests`), every `self.state` in `impl EveMcp`, every `s.state` in `mod tests`

**Interfaces:**
- Produces: `EveMcp { workspaces: Arc<Mutex<HashMap<PathBuf, AppState>>>, current: Mutex<Option<AppState>>, dir, roots, prefs_path }`; `fn EveMcp::state(&self) -> AppState` (the current workspace, or a scratch one created on first use).

- [ ] **Step 1: Replace the struct and constructor**

In `app/src-tauri/src/mcp.rs` replace lines 25–43 (`pub struct EveMcp { … }` through the end of `impl EveMcp { pub fn new … }`) with:

```rust
pub struct EveMcp {
    /// One workspace per account file, by canonical `core_user` path
    /// (spec §3.1). `Arc` so PR 2 can share it between connections.
    workspaces: Arc<Mutex<HashMap<PathBuf, AppState>>>,
    /// What `open` last selected. `None` until then; `state()` lends a
    /// scratch workspace so an early tool fails with `no_document` as before.
    current: Mutex<Option<AppState>>,
    /// The app dir — names cache, accounts.json, groups cache. Shared with
    /// the window, read-mostly.
    pub dir: PathBuf,
    /// EVE settings roots to discover profiles under (`default_roots()` in
    /// production; a fixture dir in tests).
    pub roots: Vec<PathBuf>,
    /// Where preferences.json lives — the user's own clutter overrides
    /// (`prefs::path_base()` in production; `None`, or a private path a test
    /// points at, otherwise).
    pub prefs_path: Option<PathBuf>,
}

impl EveMcp {
    pub fn new(dir: PathBuf, roots: Vec<PathBuf>, prefs_path: Option<PathBuf>) -> Self {
        EveMcp { workspaces: Arc::default(), current: Mutex::new(None), dir, roots, prefs_path }
    }

    /// The workspace tools act on: the one `open` last selected, or a scratch
    /// workspace before any `open`. Cheap — an `Arc` clone.
    fn state(&self) -> AppState {
        self.current.lock().unwrap().get_or_insert_with(AppState::new).clone()
    }
}
```

Add to the imports at the top of the file:

```rust
use std::sync::{Arc, Mutex};
```

- [ ] **Step 2: Route every access through `state()`**

Replace every `self.state` with `self.state()` in `impl EveMcp` blocks, and every `s.state` with `s.state()` in `mod tests`. In PowerShell from the repo root:

```powershell
$f = "app/src-tauri/src/mcp.rs"
$t = [IO.File]::ReadAllText($f)
$t = $t.Replace("&self.state,", "&self.state(),").Replace("&self.state)", "&self.state())").Replace("self.state.", "self.state().")
$t = $t.Replace("&s.state,", "&s.state(),").Replace("&s.state)", "&s.state())").Replace("s.state.", "s.state().")
[IO.File]::WriteAllText($f, $t)
```

Then fix the sites where a guard outlives the expression — a temporary `AppState` dropped at the end of the statement while something still borrows it is a compile error (`temporary value dropped while borrowed`). Bind the handle first at each:

`doc_path`:

```rust
    fn doc_path(&self, slot: Slot) -> Option<String> {
        let st = self.state();
        let guard = match slot {
            Slot::Char => st.char.lock(),
            Slot::User => st.user.lock(),
        }
        .unwrap();
        guard.as_ref().map(|d| d.path.to_string_lossy().into_owned())
    }
```

`batch`:

```rust
        let ops_list: Vec<Args> = req(args, "ops")?;
        let st = self.state();
        {
            let _group = undo::group(&st);
            for (i, op) in ops_list.iter().enumerate() {
                if let Err(mut e) = apply(&st, op) {
                    // A write failure already rolled the group back inside
                    // `edit_reshared`; a parse failure did not. `rollback_group`
                    // is a no-op when nothing was written, so call it always.
                    let mut u = st.user.lock().unwrap();
                    let mut c = st.char.lock().unwrap();
                    st.history.lock().unwrap().rollback_group(&mut u, &mut c);
                    e["op_index"] = json!(i);
                    return Err(e);
                }
            }
        }
        finish(self)
```

The test `a_nested_undo_group_rides_the_outer_one`:

```rust
        let (s, _) = open_user(&overview_user_bytes());
        let st = s.state();
        let before = undo::undo_state(&st).depth;
        {
            let _outer = undo::group(&st);
            ops::set_overview_visible(&st, 0, "TYPE", true).unwrap();
            {
                let _inner = undo::group(&st);
                ops::set_overview_order(&st, 0, vec!["TYPE".into(), "NAME".into()]).unwrap();
            }
            ops::set_overview_visible(&st, 0, "TYPE", false).unwrap();
        }
        assert_eq!(undo::undo_state(&st).depth, before + 1, "three writes under one group, one entry");
```

Run `cargo build -p app --tests` and fix any further borrow error the same way (`let st = self.state();` / `let st = s.state();` before the borrowing statement). Single-expression uses such as `ops::open_file(&s.state(), …)` and `self.state().history.lock().unwrap().dirty(s)` are fine as they are.

- [ ] **Step 3: Run the suite**

Run: `cargo test -p app --lib mcp::` then `cargo clippy --workspace --all-targets -- -D warnings`
Expected: every existing test passes (the scratch workspace reproduces the old one-session behaviour exactly), clippy exit 0.

- [ ] **Step 4: Commit**

```bash
git add app/src-tauri/src/mcp.rs
git commit -m "MCP: tools reach the state through state(); workspace fields in place"
```

---

### Task 4: `open` selects a workspace

**Files:**
- Modify: `app/src-tauri/src/mcp.rs` — `open` (currently `:1292-1327`), the `open` tool description (`:597-604`), the test `a_failed_char_reopen_does_not_leave_the_previous_character_paired`
- Test: `app/src-tauri/src/mcp.rs` `mod tests`, after `open_of_an_undecodable_file_is_parse_failed`

**Interfaces:**
- Consumes: `EveMcp::state()`, `EveMcp.workspaces`, `EveMcp.current` (Task 3); `AppState::ptr_eq` (Task 1); `Document::changed_on_disk` (Task 2); `ops::open_file`, `ops::close_file`, `ops::set_overview_visible`, `ops::apply_mutation` (existing).
- Produces: `fn canonical(path: &str) -> Result<PathBuf, Value>` (`io` error when missing); `fn slot_view(state: &AppState, slot: Slot) -> Option<Value>` (`{path, fidelity}` of an open slot); `fn reload_stale(state: &AppState)`; the `unsaved_edits` error.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` after `open_of_an_undecodable_file_is_parse_failed`. `temp_file` writes every fixture as `core_user_5.dat` in its own fresh directory, so two calls are two distinct account files.

```rust
    fn open_args(user: &Path, char: Option<&Path>) -> Args {
        let mut a = Args::new();
        a.insert("user_file".into(), json!(user.to_string_lossy()));
        if let Some(c) = char {
            a.insert("char_file".into(), json!(c.to_string_lossy()));
        }
        a
    }

    fn empty_char_bytes() -> Vec<u8> {
        encode(&BmValue::Dict(vec![])).unwrap()
    }

    /// Two accounts are two workspaces: opening the second keeps the first,
    /// unsaved edits and all, and opening the first again is a switch back,
    /// not a reload.
    #[test]
    fn open_of_a_second_account_keeps_the_first() {
        let (s, a) = open_user(&overview_user_bytes());
        ops::set_overview_visible(&s.state(), 0, "TYPE", true).unwrap();
        let first = s.state();

        let b = temp_file("mcp-b", &overview_user_bytes());
        s.call("open", &open_args(&b, None)).unwrap();
        let st = s.call("status", &Args::new()).unwrap();
        assert_eq!(st["user"]["path"], json!(b.to_string_lossy()));
        assert_eq!(st["user"]["dirty"], false, "a fresh workspace");
        assert!(!s.state().ptr_eq(&first));

        s.call("open", &open_args(&a, None)).unwrap();
        assert!(s.state().ptr_eq(&first), "the same workspace, not a reload");
        assert_eq!(s.call("status", &Args::new()).unwrap()["user"]["dirty"], true, "the edit survived");
    }

    /// Siblings share the account: opening a second character of the same
    /// account swaps the character slot inside the one workspace.
    #[test]
    fn open_of_a_sibling_swaps_the_character_in_the_same_workspace() {
        let (s, a) = open_user(&overview_user_bytes());
        let c1 = temp_file("mcp-c1", &empty_char_bytes());
        let c2 = temp_file("mcp-c2", &empty_char_bytes());
        s.call("open", &open_args(&a, Some(&c1))).unwrap();
        let ws = s.state();

        let v = s.call("open", &open_args(&a, Some(&c2))).unwrap();
        assert_eq!(v["char"]["path"], json!(c2.to_string_lossy()));
        assert_eq!(v["user"]["path"], json!(a.to_string_lossy()));
        assert!(s.state().ptr_eq(&ws));
        assert_eq!(s.call("status", &Args::new()).unwrap()["char"]["path"], json!(c2.to_string_lossy()));
    }

    /// The forced workflow (spec §3.3): finish one character of an account
    /// before starting the next. Either dirty slot blocks; save or undo clears.
    #[test]
    fn a_sibling_swap_is_blocked_while_either_slot_is_dirty() {
        let (s, a) = open_user(&overview_user_bytes());
        let c1 = temp_file("mcp-c1", &empty_char_bytes());
        let c2 = temp_file("mcp-c2", &empty_char_bytes());
        s.call("open", &open_args(&a, Some(&c1))).unwrap();

        // Account side dirty.
        ops::set_overview_visible(&s.state(), 0, "TYPE", true).unwrap();
        let e = s.call("open", &open_args(&a, Some(&c2))).unwrap_err();
        assert_eq!(e["code"], "unsaved_edits");
        assert!(e["message"].as_str().unwrap().contains("save"));
        assert_eq!(s.call("status", &Args::new()).unwrap()["char"]["path"], json!(c1.to_string_lossy()), "untouched");
        s.call("save", &Args::new()).unwrap();
        s.call("open", &open_args(&a, Some(&c2))).unwrap();

        // Character side dirty; undo clears it too.
        ops::apply_mutation(
            &s.state(),
            Slot::Char,
            &settings_model::Mutation::InsertDictEntry {
                parent: vec![],
                key: settings_model::NewValue::Str("x".into()),
                value: settings_model::NewValue::Int("1".into()),
            },
        )
        .unwrap();
        assert_eq!(s.call("open", &open_args(&a, Some(&c1))).unwrap_err()["code"], "unsaved_edits");
        s.call("undo", &Args::new()).unwrap();
        s.call("open", &open_args(&a, Some(&c1))).unwrap();
        assert_eq!(s.call("status", &Args::new()).unwrap()["char"]["path"], json!(c1.to_string_lossy()));
    }

    /// Asking for the account alone on a workspace with a character open
    /// keeps the character: `open` never discards.
    #[test]
    fn open_with_only_the_account_keeps_the_open_character() {
        let (s, a) = open_user(&overview_user_bytes());
        let c1 = temp_file("mcp-c1", &empty_char_bytes());
        s.call("open", &open_args(&a, Some(&c1))).unwrap();
        let v = s.call("open", &open_args(&a, None)).unwrap();
        assert_eq!(v["char"]["path"], json!(c1.to_string_lossy()));
    }

    /// Spec §3.3, rule 3: `open` re-reads a clean slot whose file moved on
    /// disk (the window saved, the client wrote) and never a dirty one.
    #[test]
    fn open_rereads_a_clean_stale_slot_and_never_a_dirty_one() {
        let (s, a) = open_user(&overview_user_bytes());
        // Clean: the rewrite is picked up. The fixture's one tab has TYPE hidden;
        // an empty dict has no tabs at all.
        std::fs::write(&a, encode(&BmValue::Dict(vec![])).unwrap()).unwrap();
        s.call("open", &open_args(&a, None)).unwrap();
        let v = s.call("overview_get", &Args::new()).unwrap();
        assert!(v["tabs"].as_array().unwrap().is_empty(), "the rewrite (no tabs) was picked up");

        // Dirty: the in-memory edit wins and the later save conflicts, as today.
        let (s, a) = open_user(&overview_user_bytes());
        ops::set_overview_visible(&s.state(), 0, "TYPE", true).unwrap();
        std::fs::write(&a, encode(&BmValue::Dict(vec![])).unwrap()).unwrap();
        s.call("open", &open_args(&a, None)).unwrap();
        let v = s.call("overview_get", &Args::new()).unwrap();
        assert_eq!(visible_count(&v), 2, "the edit is still there");
        assert_eq!(s.call("save", &Args::new()).unwrap_err()["code"], "conflict");
    }

    /// The key is the canonical path, so a spelling the model chooses finds
    /// the workspace a roster lookup created.
    #[test]
    fn explicit_paths_with_different_spelling_hit_the_same_workspace() {
        let (s, a) = open_user(&overview_user_bytes());
        let ws = s.state();
        let respelled = a.to_string_lossy().replace('\\', "/");
        let mut args = Args::new();
        args.insert("user_file".into(), json!(respelled));
        s.call("open", &args).unwrap();
        assert!(s.state().ptr_eq(&ws));
    }
```

`visible_count` already exists further down in `mod tests` (it counts visible columns of tab 0); it is in scope for the whole module.

Then change the existing test `a_failed_char_reopen_does_not_leave_the_previous_character_paired` — under workspaces a failed open leaves the workspace exactly as it was, so the original character stays open:

```rust
    /// A failing open must not touch the workspace it would have changed:
    /// the character that was open stays open, paired with the same account.
    #[test]
    fn a_failed_char_reopen_leaves_the_workspace_untouched() {
        let (s, path) = open_user(&overview_user_bytes());
        let cpath = temp_file("mcp-char", &encode(&BmValue::Dict(vec![])).unwrap());
        s.call("open", &args(json!({ "user_file": path.to_string_lossy(), "char_file": cpath.to_string_lossy() }))).unwrap();
        assert!(s.call("status", &Args::new()).unwrap()["char"].is_object());

        let missing = cpath.with_file_name("does-not-exist.dat");
        let e = s
            .call("open", &args(json!({ "user_file": path.to_string_lossy(), "char_file": missing.to_string_lossy() })))
            .unwrap_err();
        assert_eq!(e["code"], "io");
        assert_eq!(s.call("status", &Args::new()).unwrap()["char"]["path"], json!(cpath.to_string_lossy()));
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p app --lib mcp::tests::open_`
Expected: `open_of_a_second_account_keeps_the_first` fails (`the edit survived` — today's `open` reloads); `open_of_a_sibling_swaps…` fails on `ptr_eq` or the swap; `a_sibling_swap_is_blocked…` fails (`unsaved_edits` never raised); `open_with_only_the_account…` fails (`char` is null); `open_rereads…` fails in its dirty half (today's open reloads the file and loses the edit — `visible_count` is 0, not 2); the respelling test fails on `ptr_eq`. `a_failed_char_reopen_leaves_the_workspace_untouched` fails on the last assertion.

- [ ] **Step 3: Implement `open` over the workspace map**

Add these free functions in `app/src-tauri/src/mcp.rs` just above `impl EveMcp { fn open …` (after `slot_label`):

```rust
/// The workspace key for a settings file. Canonical, so the spelling the
/// model chooses and the one `locate` produces name one workspace. A path
/// that does not exist is the same `io` error `open_file` would give.
fn canonical(path: &str) -> Result<PathBuf, Value> {
    std::fs::canonicalize(path).map_err(|e| err("io", format!("{path}: {e}")))
}

/// `{path, fidelity}` of an open slot — what `open` reports per slot — or
/// `None` for an empty one.
fn slot_view(state: &AppState, slot: Slot) -> Option<Value> {
    let guard = match slot {
        Slot::Char => state.char.lock(),
        Slot::User => state.user.lock(),
    }
    .unwrap();
    guard.as_ref().map(|d| json!({ "path": d.path.to_string_lossy(), "fidelity": d.fidelity }))
}

/// Spec §3.3, rule 3: re-read a clean slot whose file changed on disk (the
/// window saved it, the game client rewrote it, a batch copy landed). A
/// dirty slot is never re-read — its edits are the point, and `save`'s
/// conflict check will name the situation.
///
/// ponytail: `open_file` clears the undo stack when it reloads, so a dirty
/// sibling slot keeps its edits but loses undo. Rare (one slot dirty, the
/// other clean and stale, on a re-open); the edits are what matters.
fn reload_stale(state: &AppState) {
    for slot in [Slot::User, Slot::Char] {
        let path = {
            let guard = match slot {
                Slot::Char => state.char.lock(),
                Slot::User => state.user.lock(),
            }
            .unwrap();
            match guard.as_ref() {
                Some(d) if d.changed_on_disk() => d.path.to_string_lossy().into_owned(),
                _ => continue,
            }
        };
        if state.history.lock().unwrap().dirty(slot) {
            continue;
        }
        // A failing re-read leaves the slot as it was (`open_file`'s own
        // guarantee); the next `save` then reports the conflict.
        let _ = ops::open_file(state, slot, &path);
    }
}
```

Replace the body of `fn open` (keep the argument resolution — everything from `let char_id` through the `let Some(user_file) = user_file else { … };` block — and replace what follows it) with:

```rust
        let user_key = canonical(&user_file)?;
        let char_key = char_file.as_deref().map(canonical).transpose()?;

        let existing = self.workspaces.lock().unwrap().get(&user_key).cloned();
        let ws = match existing {
            Some(ws) => {
                let open_char = ws.char.lock().unwrap().as_ref().map(|d| d.path.clone());
                // Same character, or none asked for: nothing to load. `open`
                // never discards a character to satisfy an account-only call.
                let swap = match (&open_char, &char_key) {
                    (Some(have), Some(want)) => std::fs::canonicalize(have).ok().as_ref() != Some(want),
                    (None, Some(_)) => true,
                    (_, None) => false,
                };
                if swap {
                    let h = ws.history.lock().unwrap();
                    if h.dirty(Slot::Char) || h.dirty(Slot::User) {
                        let who = open_char
                            .as_ref()
                            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                            .unwrap_or_else(|| "the open character".into());
                        return Err(err(
                            "unsaved_edits",
                            format!("{who} or its account has unsaved edits; save or undo them before opening another character of this account"),
                        ));
                    }
                    drop(h);
                    // Both slots are clean, so closing first loses nothing, and
                    // a failing open leaves the slot empty rather than pointing
                    // at the previous sibling.
                    ops::close_file(&ws, Slot::Char);
                    if let Some(p) = &char_file {
                        open_slot(&ws, Slot::Char, p)?;
                    }
                }
                reload_stale(&ws);
                ws
            }
            None => {
                // A fresh workspace is built aside and inserted only once both
                // opens succeeded, so a bad path changes nothing — not the map,
                // not `current`.
                let ws = AppState::new();
                open_slot(&ws, Slot::User, &user_file)?;
                if let Some(p) = &char_file {
                    open_slot(&ws, Slot::Char, p)?;
                }
                self.workspaces.lock().unwrap().insert(user_key, ws.clone());
                ws
            }
        };
        *self.current.lock().unwrap() = Some(ws.clone());
        Ok(json!({ "char": slot_view(&ws, Slot::Char), "user": slot_view(&ws, Slot::User) }))
```

Delete the now-unused comment block about opening the user slot first ("Open the user slot FIRST … still intact.") and the `ops::close_file(&self.state(), Slot::Char);` line — the whole old tail is replaced.

Note the existing test `a_failed_account_open_leaves_the_previous_session_intact` (a `Z:/no/such/…` account): `canonical` fails with `io` before anything is touched, which is what it asserts.

Update the `open` tool description (`ToolDef { name: "open", … }`):

```rust
            description: "Select the files to edit. The account file (core_user_<id>.dat) is required: overview presets, appearance and probe formations live there. The character file holds the window layout, Neocom, HUD and the fleet watch list; open it too unless you only need account-side editors. Give char_id (from list_characters) to resolve both from the roster, or paths directly. Each account keeps its own workspace: opening another account keeps this one, edits and all; opening another character of the same account needs this one saved or undone first. Only edit a character that is logged out: the EVE client overwrites its settings on logout.",
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p app --lib mcp::`
Expected: all pass, the six new ones and the renamed one included. Then `cargo clippy --workspace --all-targets -- -D warnings`: exit 0.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/mcp.rs
git commit -m "MCP: open selects a workspace per account, swaps siblings only when clean"
```

---

### Task 5: `status` lists workspaces; `save` takes `all`

**Files:**
- Modify: `app/src-tauri/src/mcp.rs` — `status` (`:1063-1072`), `save` (`:1329-1362`), the `status` and `save` tool descriptions (`:586-590`, `:606-612`)
- Test: `app/src-tauri/src/mcp.rs` `mod tests`, after the Task 4 tests

**Interfaces:**
- Consumes: `EveMcp.workspaces`, `EveMcp::state()`, `slot_view`.
- Produces: `status` result gains `"workspaces": [{ "char": path|null, "user": path, "dirty": { "char": bool, "user": bool } }]`; `save {all: true}` result is `{ "saved": [{slot, path, backup_path}] }` across every workspace; `fn save_state(state: &AppState, force: bool) -> Result<(Vec<Value>, Vec<&'static str>), Value>`.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn status_lists_every_workspace_with_its_dirty_slots() {
        let (s, a) = open_user(&overview_user_bytes());
        ops::set_overview_visible(&s.state(), 0, "TYPE", true).unwrap();
        let b = temp_file("mcp-b", &overview_user_bytes());
        s.call("open", &open_args(&b, None)).unwrap();

        let st = s.call("status", &Args::new()).unwrap();
        let ws = st["workspaces"].as_array().unwrap();
        assert_eq!(ws.len(), 2);
        let by_path = |p: &Path| ws.iter().find(|w| w["user"] == json!(p.to_string_lossy())).unwrap().clone();
        assert_eq!(by_path(&a)["dirty"], json!({ "char": false, "user": true }));
        assert_eq!(by_path(&b)["dirty"], json!({ "char": false, "user": false }));
        assert_eq!(by_path(&b)["char"], Value::Null);
    }

    #[test]
    fn save_all_writes_every_dirty_workspace() {
        let (s, a) = open_user(&overview_user_bytes());
        ops::set_overview_visible(&s.state(), 0, "TYPE", true).unwrap();
        let b = temp_file("mcp-b", &overview_user_bytes());
        s.call("open", &open_args(&b, None)).unwrap();
        ops::set_overview_visible(&s.state(), 0, "TYPE", true).unwrap();

        let v = s.call("save", &args(json!({ "all": true }))).unwrap();
        let saved = v["saved"].as_array().unwrap();
        assert_eq!(saved.len(), 2);
        for entry in saved {
            assert!(PathBuf::from(entry["backup_path"].as_str().unwrap()).exists());
        }
        let paths: Vec<Value> = saved.iter().map(|e| e["path"].clone()).collect();
        assert!(paths.contains(&json!(a.to_string_lossy())) && paths.contains(&json!(b.to_string_lossy())));

        let st = s.call("status", &Args::new()).unwrap();
        assert!(st["workspaces"].as_array().unwrap().iter().all(|w| w["dirty"]["user"] == false));
    }

    /// Without `all`, `save` is the current workspace only — the other stays
    /// dirty and is still listed as such.
    #[test]
    fn save_without_all_is_the_current_workspace_only() {
        let (s, a) = open_user(&overview_user_bytes());
        ops::set_overview_visible(&s.state(), 0, "TYPE", true).unwrap();
        let b = temp_file("mcp-b", &overview_user_bytes());
        s.call("open", &open_args(&b, None)).unwrap();
        ops::set_overview_visible(&s.state(), 0, "TYPE", true).unwrap();

        let v = s.call("save", &Args::new()).unwrap();
        assert_eq!(v["saved"].as_array().unwrap().len(), 1);
        assert_eq!(v["saved"][0]["path"], json!(b.to_string_lossy()));
        let st = s.call("status", &Args::new()).unwrap();
        let a_ws = st["workspaces"].as_array().unwrap().iter().find(|w| w["user"] == json!(a.to_string_lossy())).unwrap().clone();
        assert_eq!(a_ws["dirty"]["user"], true);
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p app --lib mcp::tests::status_lists_every_workspace_with_its_dirty_slots mcp::tests::save_all_writes_every_dirty_workspace mcp::tests::save_without_all_is_the_current_workspace_only`
Expected: `status_lists…` fails (`workspaces` is null); `save_all…` fails (`saved` has 1 entry, `all` ignored); `save_without_all…` fails on `workspaces`.

- [ ] **Step 3: Implement**

Replace `fn status`:

```rust
    fn status(&self) -> ToolResult {
        let st = self.state();
        let slot = |s: Slot| {
            self.doc_path(s).map(|path| json!({ "path": path, "dirty": st.history.lock().unwrap().dirty(s) }))
        };
        let workspaces: Vec<Value> = self
            .workspaces
            .lock()
            .unwrap()
            .values()
            .map(|ws| {
                let h = ws.history.lock().unwrap();
                json!({
                    "char": slot_view(ws, Slot::Char).map(|v| v["path"].clone()),
                    "user": slot_view(ws, Slot::User).map(|v| v["path"].clone()),
                    "dirty": { "char": h.dirty(Slot::Char), "user": h.dirty(Slot::User) },
                })
            })
            .collect();
        Ok(json!({
            "char": slot(Slot::Char),
            "user": slot(Slot::User),
            "can_undo": undo::undo_state(&st).can_undo,
            "workspaces": workspaces,
        }))
    }
```

Lock order inside the `map`: `slot_view` takes a slot lock while `h` (the history lock) is held — that inverts `user → char → history`. Take the paths first:

```rust
            .map(|ws| {
                let char = slot_view(ws, Slot::Char).map(|v| v["path"].clone());
                let user = slot_view(ws, Slot::User).map(|v| v["path"].clone());
                let h = ws.history.lock().unwrap();
                json!({
                    "char": char,
                    "user": user,
                    "dirty": { "char": h.dirty(Slot::Char), "user": h.dirty(Slot::User) },
                })
            })
```

Use this second form.

Split `save` into a per-workspace worker and the tool. Replace `fn save` with:

```rust
    fn save(&self, args: &Args) -> ToolResult {
        let force: bool = opt(args, "force")?.unwrap_or(false);
        let all: bool = opt(args, "all")?.unwrap_or(false);
        if !all {
            let (saved, skipped) = save_state(&self.state(), force)?;
            return Ok(json!({ "saved": saved, "skipped": skipped }));
        }
        // Every workspace, in map order. A failure stops the loop and carries
        // what already reached disk, as the single-workspace form does.
        let all_ws: Vec<AppState> = self.workspaces.lock().unwrap().values().cloned().collect();
        let mut saved = Vec::new();
        for ws in &all_ws {
            match save_state(ws, force) {
                Ok((mut s, _)) => saved.append(&mut s),
                Err(mut e) => {
                    let mut earlier = saved;
                    if let Some(now) = e["saved"].as_array_mut() {
                        earlier.append(now);
                    }
                    e["saved"] = json!(earlier);
                    return Err(e);
                }
            }
        }
        Ok(json!({ "saved": saved }))
    }
```

And add, as a free function next to `slot_label`:

```rust
/// Save one workspace's dirty slots, account first. `(saved, skipped)` on
/// success; on failure the error carries `saved` and `skipped` so far, so a
/// half-saved pair is never mistaken for a save that touched nothing.
fn save_state(state: &AppState, force: bool) -> Result<(Vec<Value>, Vec<&'static str>), Value> {
    let mut saved = Vec::new();
    let mut skipped = Vec::new();
    for (slot, name) in [(Slot::User, "user"), (Slot::Char, "char")] {
        let Some(path) = slot_view(state, slot).map(|v| v["path"].clone()) else { continue };
        if !state.history.lock().unwrap().dirty(slot) {
            skipped.push(name);
            continue;
        }
        match ops::save_document(state, slot, force) {
            Ok(report) => saved.push(json!({ "slot": name, "path": path, "backup_path": report.backup_path })),
            Err(e) => {
                let mut v = match e.code.as_str() {
                    "conflict" => err(
                        "conflict",
                        format!(
                            "the {} file changed on disk since it was opened (the EVE client or the editor wrote it). Ask the user before retrying with force: true.",
                            slot_label(slot)
                        ),
                    ),
                    _ => err(&e.code, format!("{} file: {}", slot_label(slot), e.message)),
                };
                v["saved"] = json!(saved);
                v["skipped"] = json!(skipped);
                return Err(v);
            }
        }
    }
    Ok((saved, skipped))
}
```

The old `save` body's loop is this function verbatim, with `self.doc_path(slot)` replaced by `slot_view(state, slot)`; the `conflict` message text is unchanged (a test checks it contains "force").

Update the two tool descriptions:

```rust
        ToolDef {
            name: "status",
            description: "What is open right now: the current character and account file paths, whether each has unsaved edits, whether undo is possible — and every workspace this session holds (one per account) with its unsaved slots, so nothing is left unsaved by mistake. Call it to re-orient in a long conversation.",
            schema: || obj(json!({}), &[]),
        },
```

```rust
        ToolDef {
            name: "save",
            description: "Write every slot with unsaved edits to disk: encode, verify by decoding, back up the current file, then replace it atomically. Returns each backup path. The current workspace only, or every workspace with all: true. Fails with `conflict` if the file changed on disk since open (the EVE client or the editor wrote it) — ask the user before retrying with force. If a later slot fails, the result still lists what was already saved. Only edit a character that is logged out.",
            schema: || obj(json!({
                "force": { "type": "boolean", "description": "Overwrite a file that changed on disk since open. Only after the user agrees." },
                "all": { "type": "boolean", "description": "Save every workspace, not just the current one." }
            }), &[]),
        },
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p app --lib mcp::` and `cargo clippy --workspace --all-targets -- -D warnings`
Expected: exit 0 both; `save_skips_a_clean_slot…`, `save_conflict_is_an_error_until_forced` and `save_reports_what_was_already_saved_when_a_later_slot_fails` still pass on the unchanged single-workspace shape.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/mcp.rs
git commit -m "MCP: status lists every workspace; save takes all"
```

---

### Task 6: Primer, changelog, README

**Files:**
- Modify: `app/src-tauri/src/mcp_primer.md` (the `## workflow` section), `CHANGELOG.md` (the `[0.38.0]` draft's `### Added`), `README.md` (`## AI access`)
- Test: the existing `mcp::tests::primer_has_every_topic_in_order_and_none_is_empty` and the `instructions()` assertions

- [ ] **Step 1: Primer**

In `app/src-tauri/src/mcp_primer.md`, `## workflow`, replace the `Sequence:` paragraph with:

```markdown
Sequence: `list_characters` (which files exist and who they belong to) → `open` (the account file is required; add the character file for column widths) → `overview_get` or `probes_get` → the edit tools → `save`.

`open` selects: each account keeps its own workspace for the whole conversation, so opening another character keeps the first one's unsaved edits, and reading five characters is five `open` calls with no reloads. An account's settings are edited through whichever of its characters is open; opening another character of the *same* account needs the current one saved (or undone) first — `unsaved_edits` says so. `status` lists every workspace and what is unsaved in it; `save` writes the current workspace, `save {all: true}` all of them.
```

- [ ] **Step 2: Changelog and README**

In `CHANGELOG.md`, under `## [0.38.0]` → `### Added`, append:

```markdown
- **The assistant keeps every account it opens.** Opening a second character no longer drops the first: each account has its own workspace, reading across the roster needs no reloads, and `save all: true` writes them all. One character of an account at a time — the next one opens once the current is saved.
```

In `README.md`, `## AI access`, after the sentence that lists what the server exposes, add:

```markdown
Each account the assistant opens keeps its own workspace for the conversation, so it can read and edit across your characters without reloading.
```

- [ ] **Step 3: Run everything**

Run: `cargo test --workspace` then `cargo clippy --workspace --all-targets -- -D warnings`
Expected: exit 0 both. (`npm test` is unaffected — no frontend file changed — but run it once too: `cd app; npm test`; exit code per the memory note may be 1 with every test passing; read the summary.)

- [ ] **Step 4: Commit**

```bash
git add app/src-tauri/src/mcp_primer.md CHANGELOG.md README.md
git commit -m "MCP workspaces: primer, changelog, README"
```

---

### Task 7: Live check and PR

**Files:** none new.

- [ ] **Step 1: Release build in the background**

The registered MCP server is `target/release/app.exe` (memory: rebuild release before testing). Run in the background — a silent long build gets an agent killed:

```powershell
cargo build -p app --release
```

- [ ] **Step 2: Scripted stdio run**

With the build done and the window **closed**, drive the exe by hand the way slice 1 did (one JSON-RPC line per request on stdin; the reply per line on stdout):

```powershell
$lines = @(
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}',
  '{"jsonrpc":"2.0","method":"notifications/initialized"}',
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_characters","arguments":{}}}',
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"status","arguments":{}}}'
)
$lines -join "`n" | & target/release/app.exe --mcp
```

Then, using two real character ids from `list_characters` on **different** accounts (A, then B, then A again), send `open` for each and a `status` after the third; expected: `status.workspaces` has two entries, and the third `open` reports A's paths without a reload (sub-second). Do not `save` — the run is read-only. Paste the `status` reply into the PR description.

- [ ] **Step 3: Open the PR**

```bash
git push -u origin feat/mcp-workspaces
gh pr create --base master --title "MCP: one workspace per account" --body-file <(cat <<'EOF'
The MCP server keeps one workspace per account: `open` selects instead of replacing, siblings swap the character slot only when everything is saved, a clean slot that changed on disk is re-read on `open`, `status` lists every workspace and `save` takes `all`. Spec: docs/superpowers/specs/2026-09-21-mcp-live-mode-design.md §3, PR 1 of 2 (live mode follows).

`AppState` is now a cloneable `Arc` handle (the window's state and the future in-window server share one), and `Document::changed_on_disk` is `save`'s conflict check made public.

Smoke: <paste the status reply>

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)
```

(On PowerShell, write the body to a file in the scratchpad and pass `--body-file <path>`.)

---

## Self-review

**Spec coverage (§3, PR 1 scope):** §3.1 state → Tasks 1, 3. §3.2 branch 2 → Task 4 (branch 1 is PR 2). §3.3 swap, block, reload → Task 4. §3.4 `status`/`save all` → Task 5 (`mode`, `window`, `in_window`, `account_read_only`, `window_available` are PR 2; the `status` result keeps its top-level `char`/`user`/`can_undo` shape and adds `workspaces` rather than nesting under `current` — the spec's §3.4 JSON is amended to match in the spec commit that accompanies this plan). §3.5 rule 1 → the map key, Task 4; rules 2–3 → PR 2. §4.6 primer (the workspace sentences) → Task 6. §7 workspace tests → Tasks 4, 5. §8 rows for `ops.rs`, `mcp.rs`, `mcp_primer.md`, `CHANGELOG.md`, `README.md` → covered; `Document::changed_on_disk` is an addition to §8 (settings-model) noted in the same spec amendment.

**Type consistency:** `canonical(&str) -> Result<PathBuf, Value>`, `slot_view(&AppState, Slot) -> Option<Value>`, `reload_stale(&AppState)`, `save_state(&AppState, bool) -> Result<(Vec<Value>, Vec<&'static str>), Value>`, `EveMcp::state() -> AppState`, `AppState::ptr_eq(&AppState) -> bool`, `Document::changed_on_disk() -> bool` — each defined once and used with that signature.
