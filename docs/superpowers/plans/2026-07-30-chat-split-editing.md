# Chat split editing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make each chat window's member-list width and input-box height editable from the layout window panel, per channel and across a whole chat stack.

**Architecture:** One new authoring function in the existing read-only `chat.rs`, modelled line-for-line on `hud.rs::set_hud_value` (same section resolution, same zero-timestamp mint, same "caller reshares only after a mint" contract). One Tauri command returning the refreshed projection. On the frontend, a new `ChatSplit.svelte` mounted inside `WindowPanel`'s existing `{#snippet detail(w)}` — which serves free windows, stack containers and stack members from one place.

**Tech Stack:** Rust (`settings-model`, `blue_marshal`, `serde`), Tauri commands, TypeScript, Svelte 5 (runes), `cargo test`, `node --test` for `*.test.ts`, vitest + jsdom for `*.spec.ts`.

Design spec: `docs/superpowers/specs/2026-07-30-chat-split-editing-design.md`.

## Global Constraints

- **Branch dependency.** This branch (`worktree-chat-split-editing`) is stacked on `worktree-canvas-detail-layer` (PR #47), which introduced `chat.rs`. Do not merge to master before that lands. Do not create branches, merge, or push.
- **Both keys live in the account file's root `ui` section**, NOT `windows`. Corpus-verified: 705 sightings across 184 real account files, zero under `windows`. Key names are `{id}_userlistwidth` and `chatinputsize_{id}`.
- **Rust read AND write paths thread `SharedTable`/`effective` end to end** — section key and entry keys. A path matching bare `Value::Bytes` finds nothing in real account files. This bug class has shipped here before (v0.15.0).
- **Only `chatchannel_*` ids may be written.** Key names are built by string concatenation, so an unvalidated id would mint `market_userlistwidth` — a key EVE never reads and nothing ever cleans up.
- **Nothing is written if any input is invalid.** Validation runs to completion before the first mutation, so a bad id or a negative value in a batch leaves the document byte-identical.
- **A negative value is refused, never clamped.** Silently rewriting a typed number makes the field untrustworthy.
- **Values are NOT clamped to the window's size.** The split is account-scoped; the geometry is character-scoped. Clamping against whichever character is open would write a number chosen for that character into a setting every sibling shares.
- **Only a mint de-shares the document.** `set_chat_splits` returns whether it minted; `ops` reshares only then. A plain scalar overwrite needs no reshare.
- **No personal data in fixtures.** Rust tests build synthetic trees with invented channel names.
- Run Rust tests with `cargo test -p settings-model` from the worktree root; frontend with `npm test` from `app/`; type-check with `npm run check` from `app/`. `node_modules` may be absent in a fresh worktree — run `npm install` from `app/` first if npm fails for that reason (it changes no tracked files).

---

### Task 1: `chat.rs` — the authoring path

**Files:**
- Modify: `crates/settings-model/src/chat.rs` (append below `leaf_int`; extend the `#[cfg(test)] mod tests`)
- Modify: `crates/settings-model/src/lib.rs` (extend the `pub use chat::{...}` line)

**Interfaces:**
- Consumes: `crate::treewalk::{collect_shared, effective, inline_all, is_bytes, section, text, Entries, SharedTable}`, `crate::path::{NodePath, Step}`, `crate::mutate`. All already used by `hud.rs`.
- Produces:
  - `pub enum ChatError { NotAChatWindow(String), NoSection, NotEditable(String), Negative(i64) }` — `Serialize` with `#[serde(tag = "code", content = "detail", rename_all = "snake_case")]`, plus a `Display` impl, exactly like `HudError`.
  - `pub fn set_chat_splits(root: &mut Value, ids: &[String], userlist: Option<i64>, input: Option<i64>) -> Result<bool, ChatError>` — `Ok(true)` when at least one key was MINTED (the only case needing a reshare).
  - Re-exported as `settings_model::{set_chat_splits, ChatError}`.

**Deviation from the spec, deliberate:** §4.1 sketches a `ChatField` enum. It is not needed — taking both fields as `Option<i64>` in one call covers the single-field edit (one `Some`) and the stack apply (both `Some`), and an enum alongside that would be a second way to say the same thing. Do not add it.

- [ ] **Step 1: Write the failing tests**

Append these to the existing `#[cfg(test)] mod tests` in `crates/settings-model/src/chat.rs`, reusing its `ts()`, `b()`, `wrapped()` and `user_doc()` helpers (read them first — do not redeclare):

```rust
    /// The `ui` section, with `entries` plus one unrelated key so the section is
    /// never empty by accident.
    fn ui_doc(entries: Vec<(Value, Value)>) -> Value {
        let mut all = vec![(b("neocomWidth"), wrapped(Value::Int(37)))];
        all.extend(entries);
        user_doc(all)
    }

    fn width_of(doc: &Value, id: &str) -> Option<i64> {
        project_chat(doc).into_iter().find(|p| p.window_id == id)?.userlist_width
    }
    fn input_of(doc: &Value, id: &str) -> Option<i64> {
        project_chat(doc).into_iter().find(|p| p.window_id == id)?.input_height
    }

    #[test]
    fn overwrites_an_existing_key_without_minting() {
        let mut doc = ui_doc(vec![(b("chatchannel_local_userlistwidth"), wrapped(Value::Int(135)))]);
        let minted = set_chat_splits(&mut doc, &["chatchannel_local".into()], Some(200), None).unwrap();
        assert!(!minted, "overwriting an existing key must not report a mint");
        assert_eq!(width_of(&doc, "chatchannel_local"), Some(200));
    }

    #[test]
    fn mints_an_absent_key_with_a_zero_timestamp() {
        let mut doc = ui_doc(vec![]);
        let minted = set_chat_splits(&mut doc, &["chatchannel_local".into()], Some(120), None).unwrap();
        assert!(minted, "minting must be reported so the caller reshares");
        assert_eq!(width_of(&doc, "chatchannel_local"), Some(120));
        // The leaf must be the (timestamp, value) shape real files use.
        let Value::Dict(root) = &doc else { panic!("root is a dict") };
        let (_, ui) = root.iter().find(|(k, _)| is_bytes(k, b"ui")).expect("ui section");
        let Value::Dict(entries) = ui else { panic!("ui is a dict") };
        let (_, leaf) = entries
            .iter()
            .find(|(k, _)| is_bytes(k, b"chatchannel_local_userlistwidth"))
            .expect("minted key");
        assert_eq!(leaf, &Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Int(120)]));
    }

    #[test]
    fn writes_both_fields_in_one_call() {
        let mut doc = ui_doc(vec![]);
        set_chat_splits(&mut doc, &["chatchannel_local".into()], Some(120), Some(70)).unwrap();
        assert_eq!(width_of(&doc, "chatchannel_local"), Some(120));
        assert_eq!(input_of(&doc, "chatchannel_local"), Some(70));
    }

    /// The stack apply: many ids, one call.
    #[test]
    fn writes_every_id_in_one_call() {
        let mut doc = ui_doc(vec![(b("chatchannel_corp_userlistwidth"), wrapped(Value::Int(50)))]);
        let ids = vec!["chatchannel_local".into(), "chatchannel_corp".into(), "chatchannel_fleet".into()];
        set_chat_splits(&mut doc, &ids, Some(111), Some(60)).unwrap();
        for id in ["chatchannel_local", "chatchannel_corp", "chatchannel_fleet"] {
            assert_eq!(width_of(&doc, id), Some(111), "{id} width");
            assert_eq!(input_of(&doc, id), Some(60), "{id} input");
        }
    }

    /// A non-chat id is refused AND nothing at all is written — not even the
    /// valid ids beside it. Validation completes before the first mutation.
    #[test]
    fn a_non_chat_id_writes_nothing() {
        let mut doc = ui_doc(vec![]);
        let before = doc.clone();
        let ids = vec!["chatchannel_local".into(), "market".into()];
        let err = set_chat_splits(&mut doc, &ids, Some(120), None).unwrap_err();
        assert_eq!(err, ChatError::NotAChatWindow("market".into()));
        assert_eq!(doc, before, "a refused batch must leave the document untouched");
    }

    #[test]
    fn a_negative_value_writes_nothing() {
        let mut doc = ui_doc(vec![]);
        let before = doc.clone();
        let err = set_chat_splits(&mut doc, &["chatchannel_local".into()], Some(-1), None).unwrap_err();
        assert_eq!(err, ChatError::Negative(-1));
        assert_eq!(doc, before);
    }

    /// The write path must resolve Ref/Shared exactly as the read path does —
    /// real account files dedup their repeated key strings, and the `ui` section
    /// key itself is Ref-keyed.
    #[test]
    fn writes_through_a_shared_section_key_and_a_shared_entry_key() {
        let mut doc = Value::Dict(vec![(
            Value::Shared { slot: 1, value: Box::new(b("ui")) },
            Value::Dict(vec![(
                Value::Shared { slot: 2, value: Box::new(b("chatchannel_local_userlistwidth")) },
                wrapped(Value::Int(135)),
            )]),
        )]);
        let minted = set_chat_splits(&mut doc, &["chatchannel_local".into()], Some(90), None).unwrap();
        assert!(!minted, "the key is present, just shared — this is an overwrite");
        assert_eq!(width_of(&doc, "chatchannel_local"), Some(90));
    }

    /// Present but the wrong wire kind: refuse rather than clobber it or mint a
    /// duplicate key beside it. Mirrors hud.rs's Unwritable.
    #[test]
    fn a_malformed_existing_value_is_refused() {
        let mut doc = ui_doc(vec![(b("chatchannel_local_userlistwidth"), wrapped(b("wide")))]);
        let before = doc.clone();
        let err = set_chat_splits(&mut doc, &["chatchannel_local".into()], Some(90), None).unwrap_err();
        assert!(matches!(err, ChatError::NotEditable(_)));
        assert_eq!(doc, before);
    }

    #[test]
    fn a_document_with_no_ui_section_is_refused() {
        let mut doc = Value::Dict(vec![(b("windows"), Value::Dict(vec![]))]);
        let err = set_chat_splits(&mut doc, &["chatchannel_local".into()], Some(90), None).unwrap_err();
        assert_eq!(err, ChatError::NoSection);
    }

    #[test]
    fn passing_neither_field_writes_nothing() {
        let mut doc = ui_doc(vec![]);
        let before = doc.clone();
        assert!(!set_chat_splits(&mut doc, &["chatchannel_local".into()], None, None).unwrap());
        assert_eq!(doc, before);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p settings-model chat::`
Expected: FAIL to compile — `cannot find function 'set_chat_splits'`, `cannot find type 'ChatError'`.

- [ ] **Step 3: Write the implementation**

Append to `crates/settings-model/src/chat.rs`, after `leaf_int`. Extend the `use` lines at the top to add `inline_all`, `is_bytes` and `unwrap_shared` from `treewalk`, and `crate::path::{NodePath, Step}`:

```rust
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum ChatError {
    /// An id that is not a chat channel. The key names are built by
    /// concatenation, so an unchecked id would mint `market_userlistwidth` —
    /// a key EVE never reads and nothing ever cleans up.
    NotAChatWindow(String),
    /// The account file has no `ui` section to write into.
    NoSection,
    /// The key exists but holds an unexpected wire kind; overwriting would
    /// change its type and minting would duplicate the key.
    NotEditable(String),
    /// Refused, not clamped: silently rewriting a typed number makes the field
    /// untrustworthy.
    Negative(i64),
}

impl std::fmt::Display for ChatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatError::NotAChatWindow(id) => write!(f, "{id:?} is not a chat window."),
            ChatError::NoSection => write!(f, "This file has no section to write these values into."),
            ChatError::NotEditable(key) => {
                write!(f, "{key:?} has an unexpected type here and cannot be edited safely.")
            }
            ChatError::Negative(v) => write!(f, "A chat split cannot be negative (got {v})."),
        }
    }
}

/// Write the member-list width and/or the input-box height for every id.
///
/// Returns `true` when at least one key was MINTED. That is the only path that
/// de-shares the document (`inline_all`), and so the only one whose caller must
/// `reshare` before encoding — the same contract `hud.rs::set_hud_value` has.
///
/// Both fields are optional so one function covers the single-field edit and the
/// stack apply. Passing neither is a no-op rather than an error: the UI can call
/// it with nothing changed and get a harmless re-projection.
///
/// NOTHING is written unless everything validates. A batch carrying one bad id
/// leaves the document byte-identical, which is what makes the stack apply safe
/// to offer as a single button.
pub fn set_chat_splits(
    root: &mut Value,
    ids: &[String],
    userlist: Option<i64>,
    input: Option<i64>,
) -> Result<bool, ChatError> {
    for v in [userlist, input].into_iter().flatten() {
        if v < 0 {
            return Err(ChatError::Negative(v));
        }
    }
    for id in ids {
        if !id.starts_with(CHAT_PREFIX) {
            return Err(ChatError::NotAChatWindow(id.clone()));
        }
    }

    let keys: Vec<(String, i64)> = ids
        .iter()
        .flat_map(|id| {
            [
                userlist.map(|v| (format!("{id}{WIDTH_SUFFIX}"), v)),
                input.map(|v| (format!("{INPUT_PREFIX}{id}"), v)),
            ]
        })
        .flatten()
        .collect();
    if keys.is_empty() {
        return Ok(false);
    }

    // Validate every target BEFORE mutating anything. `NotEditable` can only be
    // found by looking, so without this pass a batch could write half its keys
    // and then refuse — the state this function exists to make impossible.
    for (key, _) in &keys {
        if matches!(locate(root, key)?, Target::Unwritable) {
            return Err(ChatError::NotEditable(key.clone()));
        }
    }

    let mut minted = false;
    for (key, value) in &keys {
        // Re-located per key on purpose: minting runs `inline_all`, which
        // rewrites the tree, so a NodePath computed before it can be stale.
        match locate(root, key)? {
            Target::Writable(path) => {
                let m = crate::mutate::Mutation::SetScalar { path, text: value.to_string() };
                // Unreachable in practice — `locate` already proved the leaf is
                // an Int and the text is an integer's own Display.
                crate::mutate::apply(root, &m).map_err(|_| ChatError::NotEditable(key.clone()))?;
            }
            Target::Unwritable => return Err(ChatError::NotEditable(key.clone())),
            Target::Absent => {
                mint(root, key, *value)?;
                minted = true;
            }
        }
    }
    Ok(minted)
}

/// What a write to `key` may do. The same three-way split `hud.rs` uses, and for
/// the same reason: "absent" (safe to mint) and "present but unreadable" (must
/// be refused) look identical to a lookup that only asks whether it found a
/// readable value.
enum Target {
    Writable(NodePath),
    Unwritable,
    Absent,
}

fn locate(root: &Value, key: &str) -> Result<Target, ChatError> {
    let mut shared = SharedTable::new();
    collect_shared(root, &mut shared);
    let (entries, base) = section(root, b"ui", &shared).ok_or(ChatError::NoSection)?;
    let found = entries
        .iter()
        .enumerate()
        .find(|(_, (k, _))| text(k, &shared).as_deref() == Some(key));
    let Some((i, (_, v))) = found else { return Ok(Target::Absent) };

    let mut p = base;
    p.push(Step::DictValue(i));
    let (v, p) = unwrap_shared(v, p);
    // (timestamp, value): take element 1. A bare value is tolerated the way
    // leaf_int tolerates one.
    let (v, p) = match v {
        Value::Tuple(items) if items.len() == 2 => {
            let mut q = p;
            q.push(Step::Tuple(1));
            (&items[1], q)
        }
        other => (other, p),
    };
    Ok(match effective(v, &shared) {
        Value::Int(_) => Target::Writable(p),
        _ => Target::Unwritable,
    })
}

/// Insert the absent leaf. After `inline_all` every key is a plain byte-string,
/// so this half needs no `Shared`/`Ref` resolution.
fn mint(root: &mut Value, key: &str, value: i64) -> Result<(), ChatError> {
    inline_all(root);
    let Value::Dict(entries) = root else { return Err(ChatError::NoSection) };
    let (_, ui) = entries.iter_mut().find(|(k, _)| is_bytes(k, b"ui")).ok_or(ChatError::NoSection)?;
    let Value::Dict(section_entries) = ui else { return Err(ChatError::NoSection) };
    section_entries.push((
        Value::Bytes(key.as_bytes().to_vec()),
        Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Int(value)]),
    ));
    Ok(())
}
```

Update the module doc comment at the top of the file: it currently says "Nothing here mutates… no editor writes them". Replace that paragraph with a note that the read path is `project_chat` and the write path is `set_chat_splits`, and that only a mint de-shares the document.

Extend `crates/settings-model/src/lib.rs`'s existing chat re-export to:

```rust
pub use chat::{project_chat, set_chat_splits, ChatError, ChatPanel};
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p settings-model chat::`
Expected: PASS — the 7 pre-existing projection tests plus the 10 new ones.

- [ ] **Step 5: Run the whole crate**

Run: `cargo test -p settings-model`
Expected: PASS, no regressions.

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/src/chat.rs crates/settings-model/src/lib.rs
git commit -m "Add the chat split write path"
```

---

### Task 2: Expose `set_chat_splits` to the frontend

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (add after `chat_panels`)
- Modify: `app/src-tauri/src/lib.rs` (command fn beside `chat_panels`; registration in the `invoke_handler!` list)
- Modify: `app/src/lib/api.ts` (binding beside `chatPanels`)

**Interfaces:**
- Consumes: `settings_model::{set_chat_splits, ChatError, ChatPanel, project_chat}` from Task 1.
- Produces:
  - Rust: `pub fn set_chat_splits(state: &AppState, ids: Vec<String>, userlist: Option<i64>, input: Option<i64>) -> Result<Vec<ChatPanel>, ErrDto>`
  - TS: `api.setChatSplits(ids: string[], userlistWidth: number | null, inputHeight: number | null): Promise<ChatPanel[]>`

- [ ] **Step 1: Write the failing test**

Append to the `mod tests` block at the bottom of `app/src-tauri/src/ops.rs`, beside the existing `chat_panels_is_empty_without_an_account_file`. `AppState` is constructed with `AppState::new()` — it has no `Default` impl:

```rust
#[test]
fn setting_a_chat_split_without_an_account_file_errors() {
    let state = AppState::new();
    // Contrast with chat_panels, which returns an empty list: reading an
    // unpaired character is normal, but there is nowhere to write.
    let err = set_chat_splits(&state, vec!["chatchannel_local".into()], Some(120), None).unwrap_err();
    assert_eq!(err.code, "no_document");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p eve-settings-editor setting_a_chat_split`
Expected: FAIL to compile — `cannot find function 'set_chat_splits' in this scope`.

(If that package name does not resolve, take it from `app/src-tauri/Cargo.toml`, or use `cargo test --workspace setting_a_chat_split`.)

- [ ] **Step 3: Write the op**

Add to `app/src-tauri/src/ops.rs` immediately after `chat_panels`. Read `set_hud_field` (a little further down the file) first — this mirrors its reshare-only-after-mint and read-only-guard structure:

```rust
/// Write the chat splits for one or more channels, reshare if anything was
/// minted, and return the fresh projection.
///
/// The account slot only: the character document holds the chat WINDOW, but the
/// split is account-scoped, so nothing here touches it. The frontend marks the
/// user slot dirty.
///
/// Projects from inside the same guard rather than calling `chat_panels`, which
/// would take the same lock again — `std::sync::Mutex` is not reentrant.
pub fn set_chat_splits(
    state: &AppState,
    ids: Vec<String>,
    userlist: Option<i64>,
    input: Option<i64>,
) -> Result<Vec<ChatPanel>, ErrDto> {
    let mut guard = state.user.lock().unwrap();
    let doc = guard
        .as_mut()
        .ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
    if let Fidelity::ReadOnly { reason } = &doc.fidelity {
        return Err(ErrDto::new("read_only", reason.clone()));
    }
    let minted = settings_model::set_chat_splits(&mut doc.value, &ids, userlist, input)
        .map_err(chat_err)?;
    // Only a mint de-shares the document; a scalar overwrite sets one value in
    // place, where a whole-tree reshare would buy nothing.
    if minted {
        doc.value = blue_marshal::reshare(&doc.value);
    }
    Ok(project_chat(&doc.value))
}

fn chat_err(e: settings_model::ChatError) -> ErrDto {
    let v = serde_json::to_value(&e).unwrap_or_default();
    ErrDto::new(v.get("code").and_then(|c| c.as_str()).unwrap_or("chat"), e.to_string())
}
```

Add `ChatError` to the `use settings_model::{...}` list at the top of `ops.rs` if the compiler asks for it (the fully-qualified path above avoids needing it for the signature).

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p eve-settings-editor setting_a_chat_split`
Expected: PASS.

- [ ] **Step 5: Wire the Tauri command**

Add to `app/src-tauri/src/lib.rs`, immediately after the `chat_panels` command fn:

```rust
#[tauri::command]
fn set_chat_splits(
    state: tauri::State<'_, AppState>,
    ids: Vec<String>,
    userlistWidth: Option<i64>,
    inputHeight: Option<i64>,
) -> Result<Vec<settings_model::ChatPanel>, ErrDto> {
    ops::set_chat_splits(&state, ids, userlistWidth, inputHeight)
}
```

Rust will warn about the non-snake-case argument names. Tauri matches command arguments to the JS object's keys, and the rest of this file uses camelCase for multi-word arguments (`tabIndex`, `windowIdx`, `fromTab`) — follow that, and silence the warning with `#[allow(non_snake_case)]` on the function if the build treats warnings as errors.

Add `set_chat_splits,` to the `invoke_handler![...]` list, on the same line as `chat_panels`.

- [ ] **Step 6: Add the frontend binding**

In `app/src/lib/api.ts`, add immediately after the `chatPanels` line:

```ts
  setChatSplits: (ids: string[], userlistWidth: number | null, inputHeight: number | null) =>
    invoke<ChatPanel[]>("set_chat_splits", { ids, userlistWidth, inputHeight }),
```

- [ ] **Step 7: Verify**

Run from the worktree root: `cargo test --workspace`
Run from `app/`: `npm run check`
Expected: all pass, no new type errors.

- [ ] **Step 8: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Expose the chat split write path to the frontend"
```

---

### Task 3: The two pure helpers

**Files:**
- Modify: `app/src/lib/detail.ts` (append at the end, after `windowDetail`)
- Modify: `app/src/lib/detail.test.ts` (append before the final `console.log`)

**Interfaces:**
- Consumes: types `Stack`, `Geom`, `ChatPanel` from `./api`. `Stack` is `{ container_id: string; container_label: string; anchor_id: string; members: string[] }`; `Geom` carries `x`, `y`, `w`, `h` among other fields.
- Produces:
  - `export function chatStackTargets(stack: Stack): string[]`
  - `export function historyArea(geom: { w: number; h: number }, panel: ChatPanel | undefined): { w: number; h: number }`

**Deviation from the spec, deliberate:** §5.2 gives `chatStackTargets(stack, windows)`. The `windows` argument is unused — a stack's `members` are already window ids from the same projection, and filtering by prefix is the whole job. Take one argument.

- [ ] **Step 1: Write the failing test**

Append to `app/src/lib/detail.test.ts`, immediately before the final `console.log("detail.test.ts ok");`. Add `chatStackTargets, historyArea` to the existing import from `./detail.ts` and `Stack` to the type import from `./api.ts`:

```ts
// --- chat stack targets ----------------------------------------------------
{
  const stack = (members: string[]): Stack =>
    ({ container_id: "ChatWindowStack", container_label: "Chat stack", anchor_id: members[0], members });

  const mixed = stack(["chatchannel_local", "market", "chatchannel_corp"]);
  check("only chat channels are targeted", chatStackTargets(mixed).join(",") === "chatchannel_local,chatchannel_corp");
  // A non-chat window sharing the stack must be skipped, not have a meaningless
  // key minted for it.
  check("a non-chat member is skipped", !chatStackTargets(mixed).includes("market"));
  check("member order is preserved", chatStackTargets(stack(["chatchannel_b", "chatchannel_a"]))[0] === "chatchannel_b");
  check("a stack with no chat members yields nothing", chatStackTargets(stack(["market", "overview"])).length === 0);
}

// --- history area ----------------------------------------------------------
{
  const geom = { w: 256, h: 424 };
  const both: ChatPanel = { window_id: "chatchannel_local", userlist_width: 104, input_height: 63 };
  const a = historyArea(geom, both);
  check("history is what the two splits leave", a.w === 152 && a.h === 361);

  // An absent split takes nothing away — the player has never resized it.
  const none: ChatPanel = { window_id: "c", userlist_width: null, input_height: null };
  const b = historyArea(geom, none);
  check("an absent split subtracts nothing", b.w === 256 && b.h === 424);
  check("no panel at all subtracts nothing", historyArea(geom, undefined).w === 256);

  // The case the panel exists to surface: a split wider than the window leaves
  // the history area NEGATIVE. Not clamped — see the spec's §6. This is what
  // tells the player the account-wide value does not fit this character.
  const over: ChatPanel = { window_id: "c", userlist_width: 300, input_height: 500 };
  const c = historyArea(geom, over);
  check("an oversized split reports a negative history area", c.w === -44 && c.h === -76);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run from `app/`: `node --test src/lib/detail.test.ts`
Expected: FAIL — `chatStackTargets is not a function` (or an import error naming it).

- [ ] **Step 3: Write the implementation**

Append to `app/src/lib/detail.ts`. Extend the type import from `./api` to include `Stack` and `ChatPanel` (`ChatPanel` is likely already there):

```ts
/**
 * The chat channels in a stack — what the "apply to this stack" button writes
 * to. Non-chat members are skipped: the split keys are named by concatenating
 * the window id, so minting one for `market` would leave a key EVE never reads.
 *
 * Takes only the stack: its `members` are already window ids from the same
 * projection, so there is nothing to cross-reference them against.
 */
export function chatStackTargets(stack: Stack): string[] {
  return stack.members.filter((id) => id.startsWith("chatchannel_"));
}

/**
 * What the chat history area is left with, for THIS character's window.
 *
 * Deliberately unclamped, and it can go negative. The splits are account-scoped
 * while the window geometry is character-scoped, so a value that fits one
 * character can overflow another's window — and reporting that honestly is the
 * point. Clamping would hide exactly the case worth seeing.
 *
 * An absent split subtracts nothing: the player has never resized it, so EVE's
 * own default applies and the file has no number to show.
 */
export function historyArea(
  geom: { w: number; h: number },
  panel: ChatPanel | undefined,
): { w: number; h: number } {
  return {
    w: geom.w - (panel?.userlist_width ?? 0),
    h: geom.h - (panel?.input_height ?? 0),
  };
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run from `app/`: `node --test src/lib/detail.test.ts` then `npm run check`
Expected: PASS, no type errors.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/detail.ts app/src/lib/detail.test.ts
git commit -m "Add the chat stack target and history area helpers"
```

---

### Task 4: `ChatSplit.svelte`

**Files:**
- Create: `app/src/lib/ChatSplit.svelte`
- Create: `app/src/lib/ChatSplit.spec.ts`

**Interfaces:**
- Consumes: `chatStackTargets`, `historyArea` from Task 3; `ChatPanel`, `Stack` types from `./api`.
- Produces: a component taking

```ts
{
  windowId: string;
  geom: { w: number; h: number } | null;
  panel: ChatPanel | undefined;
  stack: Stack | null;
  readOnly: boolean;
  sharedNames: string[];
  onSet: (ids: string[], userlistWidth: number | null, inputHeight: number | null) => void;
}
```

- [ ] **Step 1: Write the component**

Create `app/src/lib/ChatSplit.svelte`:

```svelte
<script lang="ts">
  import { chatStackTargets, historyArea } from "$lib/detail";
  import type { ChatPanel, Stack } from "$lib/api";

  let { windowId, geom, panel, stack, readOnly, sharedNames, onSet }: {
    windowId: string;
    geom: { w: number; h: number } | null;
    panel: ChatPanel | undefined;
    stack: Stack | null;
    readOnly: boolean;
    sharedNames: string[];
    onSet: (ids: string[], userlistWidth: number | null, inputHeight: number | null) => void;
  } = $props();

  // The stack apply writes both current values to every channel, so it needs
  // both — a channel that has never been resized has nothing to copy out.
  const targets = $derived(stack ? chatStackTargets(stack) : []);
  const area = $derived(geom ? historyArea(geom, panel) : null);

  /** Commit one field. A blank or non-numeric input writes NOTHING and snaps
   * back to the stored value — the same rule HudPanel documents, and the reason
   * it exists: an empty box is a half-typed number, not a request to store one. */
  function edit(field: "userlist" | "input") {
    return (e: Event) => {
      const el = e.currentTarget as HTMLInputElement;
      const v = Number(el.value);
      if (el.value.trim() !== "" && Number.isFinite(v)) {
        // Rounded because <input type="number"> does not enforce integrality
        // and the backend stores an Int.
        onSet([windowId], field === "userlist" ? Math.round(v) : null, field === "input" ? Math.round(v) : null);
      } else {
        el.value = String((field === "userlist" ? panel?.userlist_width : panel?.input_height) ?? "");
      }
    };
  }

  const nothingToCopy = $derived(panel?.userlist_width == null && panel?.input_height == null);

  const applyToStack = () =>
    onSet(targets, panel?.userlist_width ?? null, panel?.input_height ?? null);
</script>

<div class="chat-split">
  <div class="legend">
    Chat layout — account-wide{#if sharedNames.length > 0}, shared with {sharedNames.join(", ")}{/if}
  </div>
  <div class="fields">
    <label>
      Member list
      <input type="number" min="0" value={panel?.userlist_width ?? ""} disabled={readOnly}
        onchange={edit("userlist")} />
    </label>
    <label>
      Input box
      <input type="number" min="0" value={panel?.input_height ?? ""} disabled={readOnly}
        onchange={edit("input")} />
    </label>
  </div>
  {#if area}
    <!-- Unclamped on purpose: a negative area means this account-wide split does
         not fit THIS character's window. See detail.ts's historyArea. -->
    <div class="area" class:bad={area.w <= 0 || area.h <= 0}>
      history area {area.w} × {area.h}
    </div>
  {/if}
  {#if targets.length > 1}
    <!-- Disabled when this channel has neither value stored: there would be
         nothing to copy out, and the click would be a silent no-op. -->
    <button
      class="stack-apply"
      disabled={readOnly || nothingToCopy}
      title={nothingToCopy ? "This channel has no stored sizes to copy" : undefined}
      onclick={applyToStack}>
      Apply to all {targets.length} channels in this stack
    </button>
  {/if}
</div>

<style>
  .chat-split {
    border-top: 1px solid #333;
    margin-top: 0.3rem;
    padding-top: 0.3rem;
  }
  .legend {
    color: var(--warn);
    font-size: 10px;
    margin-bottom: 0.2rem;
  }
  .fields {
    display: flex;
    gap: 0.5rem;
  }
  .fields label {
    color: #aaa;
    display: flex;
    flex-direction: column;
    font-size: 10px;
    gap: 1px;
  }
  /* Explicit dark styling per the repo's dark-native-controls note: an unstyled
     number input renders light-on-light in this theme. */
  .fields input {
    background: #11141a;
    border: 1px solid #444;
    color: #dbeafe;
    font: inherit;
    padding: 1px 3px;
    width: 5rem;
  }
  .area {
    color: #888;
    font-size: 10px;
    margin-top: 0.2rem;
  }
  .area.bad {
    color: var(--warn);
  }
  .stack-apply {
    background: #2a2f3a;
    border: 1px solid #444;
    color: #dbeafe;
    cursor: pointer;
    font: inherit;
    font-size: 10px;
    margin-top: 0.3rem;
    padding: 2px 6px;
  }
  .stack-apply:disabled {
    cursor: default;
    opacity: 0.5;
  }
</style>
```

Note the `targets.length > 1` gate: a "stack" of one channel is the channel you are already editing, so the button would be a no-op with a misleading label.

- [ ] **Step 2: Write the component test**

Create `app/src/lib/ChatSplit.spec.ts`, matching `HudPanel.spec.ts`'s style (read it first for the render/query idiom):

```ts
// Component test: run with `npm run test:ui` (vitest + jsdom).
//
// The rules worth pinning are the ones a type check cannot see: a blank input
// writes nothing, the stack button only appears for a real stack and names the
// right count, and the history area reports the overflow case rather than
// hiding it.
import { describe, expect, test, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import ChatSplit from "$lib/ChatSplit.svelte";
import type { ChatPanel, Stack } from "$lib/api";

const panel = (userlist: number | null, input: number | null): ChatPanel => ({
  window_id: "chatchannel_local",
  userlist_width: userlist,
  input_height: input,
});

const stack = (members: string[]): Stack => ({
  container_id: "ChatWindowStack",
  container_label: "Chat stack",
  anchor_id: members[0],
  members,
});

type Props = {
  windowId: string;
  geom: { w: number; h: number } | null;
  panel: ChatPanel | undefined;
  stack: Stack | null;
  readOnly: boolean;
  sharedNames: string[];
  onSet: (ids: string[], userlistWidth: number | null, inputHeight: number | null) => void;
};

function setup(over: Partial<Props> = {}) {
  const onSet = vi.fn();
  render(ChatSplit, {
    windowId: "chatchannel_local",
    geom: { w: 256, h: 424 },
    panel: panel(104, 63),
    stack: null,
    readOnly: false,
    sharedNames: [],
    onSet,
    ...over,
  } satisfies Props);
  return { onSet };
}

describe("ChatSplit", () => {
  test("shows the history area the splits leave", () => {
    setup();
    expect(screen.getByText(/history area 152 × 361/)).toBeTruthy();
  });

  test("flags an oversized split instead of hiding it", () => {
    setup({ panel: panel(300, 500) });
    // Negative, not clamped: the account-wide value does not fit this window.
    expect(screen.getByText(/history area -44 × -76/)).toBeTruthy();
  });

  test("a blank input writes nothing", async () => {
    const { onSet } = setup();
    const input = screen.getByLabelText("Member list") as HTMLInputElement;
    await fireEvent.change(input, { target: { value: "" } });
    expect(onSet).not.toHaveBeenCalled();
    expect(input.value).toBe("104");
  });

  test("a number writes only its own field", async () => {
    const { onSet } = setup();
    await fireEvent.change(screen.getByLabelText("Member list"), { target: { value: "120" } });
    expect(onSet).toHaveBeenCalledWith(["chatchannel_local"], 120, null);
  });

  test("no stack button when the window is not stacked", () => {
    setup();
    expect(screen.queryByRole("button", { name: /Apply to all/ })).toBeNull();
  });

  test("the stack button names the chat channels only", async () => {
    const { onSet } = setup({ stack: stack(["chatchannel_local", "market", "chatchannel_corp"]) });
    const button = screen.getByRole("button", { name: "Apply to all 2 channels in this stack" });
    await fireEvent.click(button);
    expect(onSet).toHaveBeenCalledWith(["chatchannel_local", "chatchannel_corp"], 104, 63);
  });

  test("read-only disables the inputs", () => {
    setup({ readOnly: true });
    expect((screen.getByLabelText("Member list") as HTMLInputElement).disabled).toBe(true);
  });

  test("the stack button is disabled when there is nothing to copy", () => {
    setup({ panel: panel(null, null), stack: stack(["chatchannel_local", "chatchannel_corp"]) });
    const button = screen.getByRole("button", { name: /Apply to all/ }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
  });
});
```

- [ ] **Step 3: Run the component test**

Run from `app/`: `npm run test:ui`
Expected: PASS, 7 new tests. If `getByLabelText` cannot find the inputs, the `<label>` wrapping in the component is what associates them — check the markup rather than switching to a different query.

- [ ] **Step 4: Type-check**

Run from `app/`: `npm run check`
Expected: no new errors.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/ChatSplit.svelte app/src/lib/ChatSplit.spec.ts
git commit -m "Add the chat split fields component"
```

---

### Task 5: Wire it into the window panel

**Files:**
- Modify: `app/src/lib/WindowPanel.svelte` (props block ~lines 7-50; the `{#snippet detail(w)}` block starting ~line 225)
- Modify: `app/src/lib/LayoutView.svelte` (the `<WindowPanel …>` call ~line 889; a new handler beside `setHud`)

**Interfaces:**
- Consumes: `ChatSplit.svelte` from Task 4; `api.setChatSplits` from Task 2; the `chats` state already present in `LayoutView`.
- Produces: no new exports. `WindowPanel` gains four props: `chats: ChatPanel[]`, `accountReadOnly: boolean`, `sharedNames: string[]`, `onSetChatSplits: (ids: string[], userlistWidth: number | null, inputHeight: number | null) => void`.

- [ ] **Step 1: Add the props to WindowPanel**

In `app/src/lib/WindowPanel.svelte`, add to the `let { … }` destructuring and its type annotation:

```ts
    chats,
    accountReadOnly,
    sharedNames,
    onSetChatSplits,
```

```ts
    /** Per-channel chat splits, from the ACCOUNT document. Empty when no
     * account file is open, which is also when the fields are read-only. */
    chats: ChatPanel[];
    /** The account document's read-only flag. The chat splits are the only
     * thing this panel writes to that file, so it is theirs alone to honour. */
    accountReadOnly: boolean;
    /** Other characters on this account — named in the chat block's legend,
     * because these two fields are account-wide. */
    sharedNames: string[];
    onSetChatSplits: (ids: string[], userlistWidth: number | null, inputHeight: number | null) => void;
```

Add the imports at the top of the `<script>`:

```ts
  import ChatSplit from "$lib/ChatSplit.svelte";
  import type { ChatPanel } from "$lib/api";
```

(`ChatPanel` may need appending to an existing `import type { … } from "$lib/api"` line rather than a new one.)

- [ ] **Step 2: Render it in the detail snippet**

In the `{#snippet detail(w)}` block, immediately after the closing `</div>` of the `.flags` div and before the snippet's own closing `</div>`, add:

```svelte
    {#if w.id.startsWith("chatchannel_")}
      <ChatSplit
        windowId={w.id}
        geom={w.geom}
        panel={chats.find((c) => c.window_id === w.id)}
        stack={w.stack ? (stacks.find((s) => s.container_id === w.stack!.container_id) ?? null) : null}
        readOnly={accountReadOnly || chats.length === 0}
        {sharedNames}
        onSet={onSetChatSplits} />
    {/if}
```

This one snippet serves free windows, stack containers and stack members, so all three get the block with no further edits. `readOnly` folds in "no account file open" (`chats.length === 0`) because there is then nothing to write to.

- [ ] **Step 3: Wire LayoutView**

In `app/src/lib/LayoutView.svelte`, add a handler beside `setHud`:

```ts
  /** Write one or more channels' splits and take the refreshed projection. The
   * splits live in the account document, so that is the slot that goes dirty. */
  async function setChatSplits(ids: string[], userlistWidth: number | null, inputHeight: number | null) {
    try {
      chats = await api.setChatSplits(ids, userlistWidth, inputHeight);
      onDirty("user");
    } catch (e) {
      await message(errMessage(e), { title: "Chat layout edit failed", kind: "error" });
    }
  }
```

And pass the four new props in the existing `<WindowPanel …>` call:

```svelte
        {chats}
        {accountReadOnly}
        {sharedNames}
        onSetChatSplits={setChatSplits}
```

- [ ] **Step 4: Verify**

Run from `app/`: `npm run check` then `npm test`
Expected: no new type errors, all tests pass.

- [ ] **Step 5: Verify in the app**

Run from `app/`: `npm run tauri dev`

With a character open that has an account file paired:

1. Select a chat window in the layout list — two fields appear under its flags, with the account-wide legend naming your other characters.
2. Change Member list; the canvas's drawn split moves to match (Detail on).
3. The history-area line updates.
4. Set Member list larger than the window's width — the line goes negative and turns amber.
5. Blank the field and tab away — nothing is written and the old value comes back.
6. On a stacked chat window, click **Apply to all N channels in this stack**, then check the other channels in the stack carry the same two values.
7. Save, reopen the file, and confirm the values persisted.
8. Open a character with NO account file — the fields are disabled.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/WindowPanel.svelte app/src/lib/LayoutView.svelte
git commit -m "Wire the chat split fields into the window panel"
```

---

### Task 6: Documentation

**Files:**
- Modify: `docs/format-notes.md` (the "Chat window splits" section)
- Modify: `docs/small-tasks.md` (the draggable-splits Open entry)
- Modify: `CHANGELOG.md` (`## [Unreleased]` → `### Added`)

**Interfaces:** none.

- [ ] **Step 1: Note the write path in format-notes**

The "Chat window splits" section describes these keys as read-only. Append this to the end of that section (read it first so the heading level and surrounding prose match):

```markdown
**Both keys are now written**, by `chat.rs::set_chat_splits`. An existing key is
overwritten in place; an absent one is minted as `(Long(0), Int(v))` under `ui`,
the same zero-timestamp mint the overview-presets container and the HUD anchors
use. Only the mint de-shares the document, so only then does `ops` reshare.

Ids are validated against the `chatchannel_` prefix before anything is written.
The key names are built by concatenating the window id, so an unvalidated id
would mint `market_userlistwidth` — a key EVE never reads and nothing ever
cleans up. Validation of the whole batch completes before the first mutation, so
a refused write leaves the document byte-identical.
```

- [ ] **Step 2: Narrow the ledger entry**

`docs/small-tasks.md` has an Open entry covering draggable chat splits AND overview column edges on the canvas. The chat half is now editable, just not by dragging. Replace that entry with:

```markdown
- [ ] **Draggable splits and column edges on the canvas.** The chat splits are
  now editable as numeric fields on the selected window (2026-07-30), but not by
  dragging the splitter on the canvas, and the overview column widths are still
  editable only from the Overview view. Dragging was considered and dropped
  twice over: `DetailParts.svelte` is `pointer-events: none` by construction —
  the one declaration that stops decoration swallowing a canvas gesture, pinned
  by a test — so a splitter drag means punching a hole in it, adding `Drag`
  variants and adding hit-test exclusions; and at a typical canvas scale of ~0.3
  a chat window's input band is about 19 screen px tall, which is not a drag
  target. Worth revisiting only if the canvas gains a zoom. Wiring
  `set_overview_width` into the Layout view is the smaller, independent half.
  _Added 2026-07-30, narrowed from the detail layer's original entry._
```

- [ ] **Step 3: Add the changelog entry**

Add to the existing `## [Unreleased]` → `### Added` block, above the detail-layer entry. User-facing copy — say what a player gets, no internal names:

```markdown
- **Edit a chat window's member list and input box.** Selecting a chat window in
  the Layout view now offers its member-list width and input-box height, with
  the chat history area it leaves shown alongside — and a button to apply both
  to every channel in the same chat stack at once. These are account-wide
  settings, shared with your other characters on that account, and the panel
  says so. If a width is too wide for the window on the character you are
  looking at, the history area is shown negative rather than quietly clamped:
  the same setting can fit one character's chat window and overflow another's.
```

- [ ] **Step 4: Commit**

```bash
git add docs/format-notes.md docs/small-tasks.md CHANGELOG.md
git commit -m "Note chat split editing in the changelog, ledger and format notes"
```

- [ ] **Step 5: Full verification**

Run, and paste the output rather than summarising it:

```bash
cargo test --workspace
cd app && npm test && npm run check
```

Expected: all green.
