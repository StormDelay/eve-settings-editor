# Leave a Chat Channel — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the layout editor (and the MCP `layout_edit` tool) leave a player chat channel or private conversation: remove its window and channel row from the character file, and its per-channel settings from the account file when no linked character still has it.

**Architecture:** Two pure tree edits in `settings-model/src/chat.rs` (character side, account side) plus a key reader in `windows.rs`. One app command `ops::chat_leave` runs the character edit, decides the account side from the linked characters' files on disk, and runs the account edit, all in one undo group. The Tauri command and the MCP op resolve the linked characters (`accounts::linked_char_files`) and pass them in. The UI adds one button in the chat section of `WindowPanel`.

**Tech Stack:** Rust (settings-model crate, Tauri 2 app crate), Svelte 5 + TypeScript, vitest.

**Spec:** `docs/superpowers/specs/2026-09-28-chat-leave-design.md`

## Global Constraints

- Leavable ids: `chatchannel_player_<non-empty>` and `chatchannel_private_<non-empty>`. Everything else is refused with code `not_leavable`, before any mutation.
- Linked characters = the Accounts store's characters for the open account's id, excluding the open character, whose `core_char_<id>.dat` exists in the SAME folder as the open `core_user_<id>.dat`. Unpaired characters are ignored.
- The account side is never edited when any linked file lists the key, or when any linked file fails to read/decode.
- Nothing reaches disk until save. One `Ctrl+Z` reverts both files.
- Window-id keys are matched through `windows::decode_id` (Bytes, Str, StrUcs2 alike); dict keys by name through `treewalk::key_is`.
- Nothing is ever created in either file by a leave.
- Commits: identity StormDelay, no AI attribution, no `Co-Authored-By` (CLAUDE.md; `.githooks/commit-msg` enforces).
- UI copy: sentence case; in-memory edits get a toast with an undo action, not a confirm dialog (house style since the dialogs redesign — `LayoutView.svelte:335-341`).
- Verify frontend runs by EXIT CODE of `npm test` / `npm run check`, not by skimming output.

## Deviations from the spec (decided while planning)

1. **No confirm dialog.** The spec (§4.4) asked for one; the house style for an in-memory, undoable edit is a toast with an undo action (`onDeleteOrphans` is the precedent). Task 7 updates the spec.
2. **No command-palette entry.** No layout window action has one (`commands.ts` carries none), so adding one for this alone is inconsistent. Dropped; Task 7 updates the spec.
3. **One more outcome, `nothing_to_clean`:** the account file is open and no linked character has the channel, but none of the §3.2 keys exist. Reported instead of `cleaned` so the account file is not marked unsaved for a no-op. The UI shows it like `cleaned`.
4. **MCP test lives in `mcp.rs`'s test module** (as every other `layout_edit` test does), not `tests/mcp_stdio.rs`.

## Review Focus

1. **A key stored as `Str` in one dict and `Bytes` in another** (real files mix them) — every dict must lose it. Pinned in Task 1 (`purges_str_and_bytes_keys_alike`).
2. **A `Shared`/`Ref`-keyed `ui` or `windows` section** (real files dedup keys) — the edit must still find them. Pinned in Task 1 (`leave_finds_a_shared_section_key`) and Task 2 (`account_side_finds_a_shared_section_key`).
3. **A linked character that is ALSO the open character** (store lists it) — must not be checked against itself, or every leave reports `kept_shared`. Pinned in Task 3 (`excludes_the_open_character`).
4. **A paired character whose file lives in another settings folder** — must not count. Pinned in Task 3 (`ignores_a_character_in_another_folder`).
5. **Leaving from a batch that then fails on a later op** — the whole batch, both files, rolls back. Pinned in Task 5 (`a_failing_later_op_rolls_back_the_leave`).

---

### Task 1: Character-side leave in the model

**Files:**
- Modify: `crates/settings-model/src/chat.rs` (new code after `mint`, ~line 249; tests in its `mod tests`)
- Modify: `crates/settings-model/src/lib.rs:52`

**Interfaces:**
- Produces:
  - `pub fn is_leavable(window_id: &str) -> bool`
  - `pub fn leave_chat_char(root: &mut Value, window_id: &str) -> Result<String, ChatLeaveError>` — returns the channel key (`window_id` minus `chatchannel_`)
  - `pub enum ChatLeaveError { NotLeavable { window: String }, NotFound { window: String } }` — `Serialize` with `#[serde(tag = "code", rename_all = "snake_case")]`, `Display`
  - private `fn dict_mut(v: &mut Value) -> Option<&mut Vec<(Value, Value)>>` (reused by Task 2)

- [ ] **Step 1: Write the failing tests** — append inside `mod tests` in `chat.rs`:

```rust
    // ---- leave_chat_char ----

    fn geom() -> Value {
        Value::Tuple(vec![Value::Int(0), Value::Int(0), Value::Int(256), Value::Int(424), Value::Int(2560), Value::Int(1440)])
    }

    /// A character document shaped like the Holy Storm spike file: the channel
    /// is a pinned, open member of ChatWindowStack with a stack index, has a
    /// settings-dialog window, and a `chatchannels` row. `keep` is a second
    /// channel that must survive untouched.
    fn char_doc() -> Value {
        let w = "chatchannel_player_-88620541";
        let keep = "chatchannel_player_-1";
        Value::Dict(vec![
            (b("windows"), Value::Dict(vec![
                (b("windowSizesAndPositions_1"), wrapped(Value::Dict(vec![
                    (b(w), geom()), (b(keep), geom()),
                    (b("ChannelSettingsDlg_player_-88620541"), geom()),
                ]))),
                (b("openWindows"), wrapped(Value::Dict(vec![(b(w), Value::Bool(true)), (b(keep), Value::Bool(true))]))),
                // Str key here, Bytes elsewhere: real files mix them.
                (b("pinnedWindows"), wrapped(Value::Dict(vec![(Value::Str(w.into()), Value::Bool(true))]))),
                (b("stacksWindows"), wrapped(Value::Dict(vec![(b(w), b("ChatWindowStack")), (b(keep), b("ChatWindowStack"))]))),
                (b("preferredIdxInStack3"), wrapped(Value::Dict(vec![
                    (b("ChatWindowStack"), Value::Dict(vec![(b(w), Value::Int(2)), (b(keep), Value::Int(3))])),
                ]))),
            ])),
            (b("ui"), Value::Dict(vec![
                (b("chatchannels"), wrapped(Value::List(vec![
                    Value::Tuple(vec![Value::Str("player_-88620541".into()), Value::Str("player_-88620541".into()), Value::Str("Bean-Intel".into())]),
                    Value::Tuple(vec![Value::Str("player_-1".into()), Value::Str("player_-1".into()), Value::Str("Keep".into())]),
                ]))),
            ])),
        ])
    }

    /// Every window-id key left anywhere under `windows`, one level of nesting deep.
    fn window_keys(doc: &Value) -> Vec<String> {
        let Value::Dict(top) = doc else { panic!() };
        let (_, Value::Dict(win)) = top.iter().find(|(k, _)| key_is(k, "windows")).unwrap() else { panic!() };
        let mut out = Vec::new();
        for (_, child) in win {
            let d = match child { Value::Tuple(t) => t.iter().find_map(|e| if let Value::Dict(d) = e { Some(d) } else { None }), Value::Dict(d) => Some(d), _ => None };
            for (k, v) in d.into_iter().flatten() {
                out.push(decode_id(k));
                if let Value::Dict(inner) = v { out.extend(inner.iter().map(|(k, _)| decode_id(k))); }
            }
        }
        out
    }

    #[test]
    fn leave_purges_the_window_the_dialog_and_the_row() {
        let mut doc = char_doc();
        assert_eq!(leave_chat_char(&mut doc, "chatchannel_player_-88620541").unwrap(), "player_-88620541");
        let keys = window_keys(&doc);
        assert!(!keys.iter().any(|k| k.contains("-88620541")), "left behind: {keys:?}");
        assert_eq!(crate::windows::chat_channel_keys(&doc), vec!["player_-1".to_string()]);
    }

    #[test]
    fn purges_str_and_bytes_keys_alike() {
        let mut doc = char_doc();
        leave_chat_char(&mut doc, "chatchannel_player_-88620541").unwrap();
        // pinnedWindows held the id as a Str key.
        assert!(!window_keys(&doc).contains(&"chatchannel_player_-88620541".to_string()));
    }

    #[test]
    fn leave_leaves_every_other_window_alone() {
        let mut doc = char_doc();
        leave_chat_char(&mut doc, "chatchannel_player_-88620541").unwrap();
        let keep = window_keys(&doc).into_iter().filter(|k| k == "chatchannel_player_-1").count();
        // geometry, openWindows, stacksWindows, preferredIdxInStack3 inner dict
        assert_eq!(keep, 4);
    }

    #[test]
    fn leave_finds_a_shared_section_key() {
        let mut doc = char_doc();
        let Value::Dict(top) = &mut doc else { panic!() };
        let k = std::mem::replace(&mut top[0].0, Value::None);
        top[0].0 = Value::Shared { slot: 1, value: Box::new(k) };
        leave_chat_char(&mut doc, "chatchannel_player_-88620541").unwrap();
        assert!(!window_keys(&doc).iter().any(|k| k.contains("-88620541")));
    }

    #[test]
    fn the_edited_document_still_encodes() {
        let mut doc = char_doc();
        leave_chat_char(&mut doc, "chatchannel_player_-88620541").unwrap();
        let bytes = blue_marshal::encode(&blue_marshal::reshare(&doc)).unwrap();
        blue_marshal::decode(&bytes).unwrap();
    }

    #[test]
    fn a_private_conversation_is_leavable() {
        assert!(is_leavable("chatchannel_private_009e6df0127111ecaa569abe94f5b483"));
        assert!(is_leavable("chatchannel_player_-88620541"));
    }

    #[test]
    fn standing_channels_and_unknown_shapes_are_refused_untouched() {
        for id in ["chatchannel_local", "chatchannel_corp", "chatchannel_alliance", "chatchannel_fleet",
                   "chatchannel_incursion", "chatchannel_invasion", "chatchannel_player_", "market", "ChatWindowStack"] {
            let mut doc = char_doc();
            let before = doc.clone();
            assert_eq!(leave_chat_char(&mut doc, id), Err(ChatLeaveError::NotLeavable { window: id.into() }), "{id}");
            assert_eq!(doc, before, "{id} must leave the tree untouched");
        }
    }

    #[test]
    fn an_absent_channel_is_not_found() {
        let mut doc = char_doc();
        assert_eq!(
            leave_chat_char(&mut doc, "chatchannel_player_-999"),
            Err(ChatLeaveError::NotFound { window: "chatchannel_player_-999".into() }),
        );
    }

    #[test]
    fn a_row_without_a_window_is_still_left() {
        // A channel the character is in whose window was never opened.
        let mut doc = char_doc();
        leave_chat_char(&mut doc, "chatchannel_player_-1").unwrap();
        assert_eq!(crate::windows::chat_channel_keys(&doc), vec!["player_-88620541".to_string()]);
    }
```

`key_is` and `decode_id` reach the tests through `use super::*` once Step 3 imports them into `chat.rs`. `chat_channel_keys` is implemented in this task's Step 3 too.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p settings-model --lib chat::tests`
Expected: compile errors — `leave_chat_char`, `is_leavable`, `ChatLeaveError`, `chat_channel_keys` not found.

- [ ] **Step 3: Implement** — first the key reader in `crates/settings-model/src/windows.rs`, replacing `chat_channel_names` (lines 373-405) with:

```rust
/// `ui → chatchannels` is `(timestamp, List[Tuple(key, fullChannelId, label)])`.
/// A channel's window id is `chatchannel_<key>`, the FIRST element — confirmed
/// in-game 2026-07-28.
///
/// Two traps, both of which shipped here and named nothing on any real file
/// while four unit tests passed. Keying on the SECOND element looks right
/// because `player_*` rows repeat the same string in both, and silently misses
/// every standing channel (`corp`, `alliance`, `local`, `fleet`, `faction`),
/// whose second element is the fully-qualified `corp_98835672` form. And the
/// wrapper is not optional in practice — matching a bare `List` returns an empty
/// map for every real file.
///
/// `tests/chat_names_corpus.rs` is the guard; it counts names off the corpus, so
/// the counts live there and cannot rot in a comment. An absent section is
/// normal, not an error.
fn chat_channel_rows<'a>(root: &'a Value, sh: &SharedTable<'a>) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    let Some((ui, _)) = section(root, b"ui", sh) else { return out };
    let Some((_, v)) = ui.iter().find(|(k, _)| is_bytes(effective(k, sh), b"chatchannels")) else {
        return out;
    };
    let Some(items) = as_list(v, sh) else { return out };
    for it in items {
        let Value::Tuple(parts) = effective(it, sh) else { continue };
        let Some(key) = parts.first().and_then(|p| text(p, sh)).filter(|k| !k.is_empty()) else { continue };
        out.push((key, parts.get(2).and_then(|p| text(p, sh))));
    }
    out
}

/// key → label, for naming chat windows.
fn chat_channel_names<'a>(root: &'a Value, sh: &SharedTable<'a>) -> HashMap<String, String> {
    chat_channel_rows(root, sh)
        .into_iter()
        .filter_map(|(k, l)| l.filter(|l| !l.is_empty()).map(|l| (k, l)))
        .collect()
}

/// Every channel key this character is in — a row counts whatever its label,
/// because the leave check (chat.rs) must not miss a channel for want of a name.
pub fn chat_channel_keys(root: &Value) -> Vec<String> {
    let mut sh = SharedTable::new();
    collect_shared(root, &mut sh);
    chat_channel_rows(root, &sh).into_iter().map(|(k, _)| k).collect()
}
```

Export it: `crates/settings-model/src/lib.rs:46` becomes
`pub use windows::{chat_channel_keys, window_layout, BoolFlag, Geom, SetTarget, Stack, StackRef, StackRole, WindowLayout, WindowRect};`

Then in `chat.rs`, extend the treewalk import to `use crate::treewalk::{collect_shared, effective, inline_all, is_bytes, key_is, section, text, unwrap_shared, SharedTable};`, add `use crate::windows::decode_id;`, and append after `mint`:

```rust
/// Leavable window-id prefixes. Standing channels (local, corp, alliance, fleet,
/// incursion, invasion, faction) are assigned by the server and come back, so
/// they are refused — as is any shape not listed here.
const LEAVABLE: [&str; 2] = ["chatchannel_player_", "chatchannel_private_"];

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum ChatLeaveError {
    NotLeavable { window: String },
    NotFound { window: String },
}

impl std::fmt::Display for ChatLeaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatLeaveError::NotLeavable { window } => {
                write!(f, "{window:?} can't be left: only player channels and private conversations can.")
            }
            ChatLeaveError::NotFound { window } => write!(f, "{window:?} is not in this character's file."),
        }
    }
}

pub fn is_leavable(window_id: &str) -> bool {
    LEAVABLE.iter().any(|p| window_id.strip_prefix(p).is_some_and(|rest| !rest.is_empty()))
}

/// Leave a chat channel on the CHARACTER side: drop `window_id` and its
/// settings dialog (`ChannelSettingsDlg_<key>`) from every dict under
/// `windows` and from the dicts nested one level inside them (the
/// per-container `preferredIdxInStack3` dicts), and drop the
/// `ui → chatchannels` row keyed `<key>`. Returns `<key>`.
///
/// The row is what makes it stick: removing only the window entries, EVE
/// rebuilds the window from the row on the next login. Verified in game
/// 2026-09-28 (docs/format-notes.md, "Leaving a chat channel").
///
/// Every dict under `windows` is swept rather than a named list, so a table EVE
/// adds later is covered too. Refusal happens before `inline_all`, so a refused
/// call leaves the tree untouched; `NotFound` does not, and the caller's
/// rollback (`edit_slot`) restores it.
pub fn leave_chat_char(root: &mut Value, window_id: &str) -> Result<String, ChatLeaveError> {
    if !is_leavable(window_id) {
        return Err(ChatLeaveError::NotLeavable { window: window_id.to_string() });
    }
    let key = window_id[CHAT_PREFIX.len()..].to_string();
    let dialog = format!("ChannelSettingsDlg_{key}");
    let gone = |k: &Value| {
        let id = decode_id(k);
        id == window_id || id == dialog
    };

    inline_all(root);
    let mut hit = false;
    let Value::Dict(top) = root else { return Err(ChatLeaveError::NotFound { window: window_id.to_string() }) };
    for (k, sect) in top.iter_mut() {
        if key_is(k, "windows") {
            let Value::Dict(win) = sect else { continue };
            for (_, child) in win.iter_mut() {
                let Some(d) = dict_mut(child) else { continue };
                let n = d.len();
                d.retain(|(k, _)| !gone(k));
                hit |= d.len() != n;
                for (_, inner) in d.iter_mut() {
                    if let Some(dd) = dict_mut(inner) {
                        let n = dd.len();
                        dd.retain(|(k, _)| !gone(k));
                        hit |= dd.len() != n;
                    }
                }
            }
        } else if key_is(k, "ui") {
            let Value::Dict(ui) = sect else { continue };
            for (k, v) in ui.iter_mut() {
                if !key_is(k, "chatchannels") {
                    continue;
                }
                let Some(rows) = list_mut(v) else { continue };
                let n = rows.len();
                rows.retain(|row| !matches!(row, Value::Tuple(t) if t.first().is_some_and(|k0| decode_id(k0) == key)));
                hit |= rows.len() != n;
            }
        }
    }
    if hit { Ok(key) } else { Err(ChatLeaveError::NotFound { window: window_id.to_string() }) }
}

/// The dict inside a value, unwrapping the `(timestamp, dict)` wrapper.
fn dict_mut(v: &mut Value) -> Option<&mut Vec<(Value, Value)>> {
    match v {
        Value::Dict(d) => Some(d),
        Value::Tuple(t) => t.iter_mut().find_map(|e| if let Value::Dict(d) = e { Some(d) } else { None }),
        _ => None,
    }
}

/// The list inside a value, unwrapping the `(timestamp, list)` wrapper.
fn list_mut(v: &mut Value) -> Option<&mut Vec<Value>> {
    match v {
        Value::List(l) => Some(l),
        Value::Tuple(t) => t.iter_mut().find_map(|e| if let Value::List(l) = e { Some(l) } else { None }),
        _ => None,
    }
}
```

Export: `crates/settings-model/src/lib.rs:52` becomes
`pub use chat::{is_leavable, leave_chat_char, project_chat, set_chat_splits, ChatError, ChatLeaveError, ChatPanel};`

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p settings-model --lib chat:: && cargo test -p settings-model --lib windows::`
Expected: all pass (the existing `windows::` chat-name tests guard the `chat_channel_rows` refactor).

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/chat.rs crates/settings-model/src/windows.rs crates/settings-model/src/lib.rs
git commit -m "Model: leave a chat channel on the character side"
```

---

### Task 2: Account-side cleanup in the model, and the corpus guard

**Files:**
- Modify: `crates/settings-model/src/chat.rs` (after `leave_chat_char`; tests in `mod tests`)
- Modify: `crates/settings-model/src/lib.rs:52`
- Create: `crates/settings-model/tests/chat_leave_corpus.rs`

**Interfaces:**
- Consumes: `is_leavable`, `dict_mut`, `CHAT_PREFIX`, `WIDTH_SUFFIX`, `INPUT_PREFIX` (Task 1 / existing); `chat_channel_keys` (Task 1).
- Produces: `pub fn leave_chat_account(root: &mut Value, window_id: &str) -> bool` — true when anything was removed.

- [ ] **Step 1: Write the failing tests** — append inside `mod tests` in `chat.rs`:

```rust
    // ---- leave_chat_account ----

    fn account_doc() -> Value {
        let w = "chatchannel_player_-88620541";
        ui_doc(vec![
            (b(&format!("{w}_userlistwidth")), wrapped(Value::Int(104))),
            (b(&format!("chatinputsize_{w}")), wrapped(Value::Int(62))),
            (b(&format!("chatfontsize_{w}")), wrapped(Value::Int(13))),
            (b(&format!("chatWindowBlink_{w}")), wrapped(Value::Bool(true))),
            (b(&format!("chatCondensedUserList_{w}")), wrapped(Value::Bool(false))),
            (b("chatCondensedUserList_player_-88620541"), wrapped(Value::Bool(false))),
            (b("chatchannel_player_-1_userlistwidth"), wrapped(Value::Int(90))),
            (b("chatPlayerChannelsJoined"), wrapped(Value::Dict(vec![
                (Value::Str("player_-88620541".into()), Value::Str("Bean-Intel".into())),
                (b("player_-1"), Value::Str("Keep".into())),
            ]))),
        ])
    }

    fn ui_keys(doc: &Value) -> Vec<String> {
        let Value::Dict(top) = doc else { panic!() };
        let (_, Value::Dict(ui)) = top.iter().find(|(k, _)| key_is(k, "ui")).unwrap() else { panic!() };
        ui.iter().map(|(k, _)| decode_id(k)).collect()
    }

    fn joined(doc: &Value) -> Vec<String> {
        let Value::Dict(top) = doc else { panic!() };
        let (_, Value::Dict(ui)) = top.iter().find(|(k, _)| key_is(k, "ui")).unwrap() else { panic!() };
        let (_, Value::Tuple(t)) = ui.iter().find(|(k, _)| key_is(k, "chatPlayerChannelsJoined")).unwrap() else { panic!() };
        let Some(Value::Dict(d)) = t.iter().find(|e| matches!(e, Value::Dict(_))) else { panic!() };
        d.iter().map(|(k, _)| decode_id(k)).collect()
    }

    #[test]
    fn account_side_removes_every_per_channel_key_and_the_joined_entry() {
        let mut doc = account_doc();
        assert!(leave_chat_account(&mut doc, "chatchannel_player_-88620541"));
        let keys = ui_keys(&doc);
        assert!(!keys.iter().any(|k| k.contains("-88620541")), "left behind: {keys:?}");
        assert!(keys.contains(&"chatchannel_player_-1_userlistwidth".to_string()));
        assert!(keys.contains(&"neocomWidth".to_string()));
        assert_eq!(joined(&doc), vec!["player_-1".to_string()]);
    }

    #[test]
    fn account_side_with_nothing_to_remove_reports_false_and_creates_nothing() {
        let mut doc = ui_doc(vec![]);
        assert!(!leave_chat_account(&mut doc, "chatchannel_player_-88620541"));
        assert_eq!(ui_keys(&doc), vec!["neocomWidth".to_string()]);
    }

    #[test]
    fn account_side_finds_a_shared_section_key() {
        let mut doc = account_doc();
        let Value::Dict(top) = &mut doc else { panic!() };
        let k = std::mem::replace(&mut top[0].0, Value::None);
        top[0].0 = Value::Shared { slot: 1, value: Box::new(k) };
        assert!(leave_chat_account(&mut doc, "chatchannel_player_-88620541"));
    }

    #[test]
    fn account_side_refuses_a_standing_channel() {
        let mut doc = ui_doc(vec![(b("chatchannel_local_userlistwidth"), wrapped(Value::Int(135)))]);
        let before = doc.clone();
        assert!(!leave_chat_account(&mut doc, "chatchannel_local"));
        assert_eq!(doc, before);
    }
```

Create `crates/settings-model/tests/chat_leave_corpus.rs`:

```rust
//! Real-data guard for the leave rule (`chat.rs::is_leavable`). The rule
//! refuses anything that is not `player_*` or `private_*`; this pins that the
//! corpus holds no THIRD kind of user channel the rule would silently refuse,
//! and that `chatPlayerChannelsJoined` is keyed the way `leave_chat_account`
//! matches it.

mod common;

use blue_marshal::Value;
use settings_model::{chat_channel_keys, is_leavable};

/// Server-assigned channels. A key outside this set that is not leavable is a
/// shape nobody has classified yet.
const STANDING: [&str; 7] = ["local", "corp", "alliance", "fleet", "incursion", "invasion", "faction"];

#[test]
fn every_chat_channel_key_is_leavable_or_standing() {
    let mut unknown = std::collections::BTreeSet::new();
    let mut seen = 0usize;
    for f in common::char_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        for key in chat_channel_keys(&doc) {
            seen += 1;
            if !is_leavable(&format!("chatchannel_{key}")) && !STANDING.contains(&key.as_str()) {
                unknown.insert(key);
            }
        }
    }
    eprintln!("{seen} chatchannels rows");
    assert!(seen > 0, "no chatchannels rows read at all — the reader broke");
    assert!(unknown.is_empty(), "unclassified channel keys: {unknown:?}");
}

fn key_text(v: &Value) -> Option<String> {
    match v {
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        Value::Str(s) | Value::StrUcs2(s) => Some(s.clone()),
        _ => None,
    }
}

/// The dict under `(timestamp, dict)` or a bare dict.
fn inner_dict(v: &Value) -> Option<&Vec<(Value, Value)>> {
    match v {
        Value::Dict(d) => Some(d),
        Value::Tuple(t) => t.iter().find_map(|e| if let Value::Dict(d) = e { Some(d) } else { None }),
        _ => None,
    }
}

#[test]
fn chat_player_channels_joined_is_keyed_by_player_keys() {
    let (mut bad, mut seen) = (std::collections::BTreeSet::new(), 0usize);
    for f in common::user_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        let Value::Dict(top) = blue_marshal::inline(&doc) else { continue };
        let Some((_, Value::Dict(ui))) = top.iter().find(|(k, _)| key_text(k).as_deref() == Some("ui")) else { continue };
        let Some((_, joined)) = ui.iter().find(|(k, _)| key_text(k).as_deref() == Some("chatPlayerChannelsJoined")) else { continue };
        for (k, _) in inner_dict(joined).into_iter().flatten() {
            seen += 1;
            let k = key_text(k).unwrap_or_else(|| format!("{k:?}"));
            if !k.starts_with("player_") {
                bad.insert(k);
            }
        }
    }
    eprintln!("{seen} chatPlayerChannelsJoined entries");
    assert!(bad.is_empty(), "chatPlayerChannelsJoined keys not shaped player_*: {bad:?}");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p settings-model --lib chat::tests::account_side`
Expected: compile error — `leave_chat_account` not found.

- [ ] **Step 3: Implement** — in `chat.rs` after `leave_chat_char`:

```rust
/// Leave a chat channel on the ACCOUNT side: drop its per-channel settings
/// from the root `ui` section — the member-list width, input height, font
/// size, blink flag, both spellings of the condensed-list flag
/// (docs/format-notes.md, "Chat window splits") — and its
/// `chatPlayerChannelsJoined` entry. Returns whether anything was removed.
///
/// Every character on the account shares this file, so the caller decides
/// whether to call it at all (ops.rs::chat_leave).
pub fn leave_chat_account(root: &mut Value, window_id: &str) -> bool {
    if !is_leavable(window_id) {
        return false;
    }
    let key = &window_id[CHAT_PREFIX.len()..];
    let names = [
        format!("{window_id}{WIDTH_SUFFIX}"),
        format!("{INPUT_PREFIX}{window_id}"),
        format!("chatfontsize_{window_id}"),
        format!("chatWindowBlink_{window_id}"),
        format!("chatCondensedUserList_{window_id}"),
        format!("chatCondensedUserList_{key}"),
    ];
    inline_all(root);
    let Value::Dict(top) = root else { return false };
    let Some((_, Value::Dict(ui))) = top.iter_mut().find(|(k, _)| key_is(k, "ui")) else { return false };
    let n = ui.len();
    ui.retain(|(k, _)| !names.iter().any(|name| key_is(k, name)));
    let mut hit = ui.len() != n;
    if let Some((_, joined)) = ui.iter_mut().find(|(k, _)| key_is(k, "chatPlayerChannelsJoined")) {
        if let Some(d) = dict_mut(joined) {
            let n = d.len();
            d.retain(|(k, _)| decode_id(k) != key);
            hit |= d.len() != n;
        }
    }
    hit
}
```

Export: `crates/settings-model/src/lib.rs:52` becomes
`pub use chat::{is_leavable, leave_chat_account, leave_chat_char, project_chat, set_chat_splits, ChatError, ChatLeaveError, ChatPanel};`

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p settings-model --lib chat:: && cargo test -p settings-model --test chat_leave_corpus -- --nocapture`
Expected: all pass. If `every_chat_channel_key_is_leavable_or_standing` lists unknown keys, STOP and report them — a new shape needs a human decision (add to `STANDING` if server-assigned, or to `LEAVABLE`), not a silent edit. If the joined-keys test flags only dump-format noise (e.g. a stray timestamp line), fix the parser, not the assertion.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/chat.rs crates/settings-model/src/lib.rs crates/settings-model/tests/chat_leave_corpus.rs
git commit -m "Model: account-side chat cleanup, and a corpus guard for the leave rule"
```

---

### Task 3: Linked-character resolution

**Files:**
- Modify: `app/src-tauri/src/accounts.rs` (new fn after `load_roster`, ~line 239; tests in its `mod tests`)

**Interfaces:**
- Produces: `pub fn linked_char_files(store: &AccountsStore, user_path: &Path, open_char: Option<&Path>) -> Vec<(u64, PathBuf)>`

- [ ] **Step 1: Write the failing tests** — inside `accounts.rs`'s `mod tests`:

```rust
    fn linked_fixture() -> (PathBuf, PathBuf, AccountsStore) {
        let dir = std::env::temp_dir().join(format!("linked-{}-{}", std::process::id(), rand_suffix()));
        let other = dir.join("other");
        std::fs::create_dir_all(&other).unwrap();
        for f in ["core_user_7.dat", "core_char_1.dat", "core_char_2.dat"] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        std::fs::write(other.join("core_char_3.dat"), b"x").unwrap();
        let mut store = AccountsStore::default();
        store.accounts.insert(7, Account { alias: None, characters: vec![1, 2, 3, 4] });
        (dir.join("core_user_7.dat"), dir.join("core_char_1.dat"), store)
    }

    fn rand_suffix() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        N.fetch_add(1, Ordering::Relaxed)
    }

    #[test]
    fn linked_are_the_paired_characters_in_the_same_folder() {
        let (user, open, store) = linked_fixture();
        let dir = user.parent().unwrap();
        assert_eq!(linked_char_files(&store, &user, Some(&open)), vec![(2, dir.join("core_char_2.dat"))]);
    }

    #[test]
    fn excludes_the_open_character() {
        let (user, open, store) = linked_fixture();
        assert!(!linked_char_files(&store, &user, Some(&open)).iter().any(|(id, _)| *id == 1));
    }

    #[test]
    fn ignores_a_character_in_another_folder() {
        // 3 is paired but its file is in `other/`; 4 is paired with no file at all.
        let (user, open, store) = linked_fixture();
        let ids: Vec<u64> = linked_char_files(&store, &user, Some(&open)).into_iter().map(|(id, _)| id).collect();
        assert_eq!(ids, vec![2]);
    }

    #[test]
    fn an_unpaired_account_has_no_linked_characters() {
        let (user, open, _) = linked_fixture();
        assert!(linked_char_files(&AccountsStore::default(), &user, Some(&open)).is_empty());
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p app --lib accounts::tests::linked`
Expected: compile error — `linked_char_files` not found.

- [ ] **Step 3: Implement** — after `load_roster`:

```rust
/// The character files that share `user_path`'s account file: the characters
/// the store pairs to that account whose `core_char_<id>.dat` sits in the SAME
/// folder — each settings folder has its own copy of the account file — minus
/// `open_char`. Unpaired characters are not considered (decided 2026-09-28,
/// docs/superpowers/specs/2026-09-28-chat-leave-design.md §3.2).
pub fn linked_char_files(store: &AccountsStore, user_path: &Path, open_char: Option<&Path>) -> Vec<(u64, PathBuf)> {
    fn id_of(p: &Path, prefix: &str) -> Option<u64> {
        p.file_stem()?.to_str()?.strip_prefix(prefix)?.parse().ok()
    }
    let (Some(user_id), Some(dir)) = (id_of(user_path, "core_user_"), user_path.parent()) else {
        return Vec::new();
    };
    let Some(acct) = store.accounts.get(&user_id) else { return Vec::new() };
    let open = open_char.and_then(|p| id_of(p, "core_char_"));
    acct.characters
        .iter()
        .filter(|&&c| Some(c) != open)
        .map(|&c| (c, dir.join(format!("core_char_{c}.dat"))))
        .filter(|(_, p)| p.is_file())
        .collect()
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p app --lib accounts::`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/accounts.rs
git commit -m "Accounts: resolve the characters that share an account file"
```

---

### Task 4: The `chat_leave` command

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (new code after `stack_delete_orphans`, ~line 1049; tests in `mod tests`)
- Modify: `app/src-tauri/src/lib.rs` (command after `stack_delete_orphans` at ~line 423; register at ~line 759)

**Interfaces:**
- Consumes: `settings_model::{leave_chat_char, leave_chat_account, chat_channel_keys}`, `accounts::{load_store, linked_char_files}`.
- Produces:
  - `pub enum AccountOutcome { Cleaned, NothingToClean, KeptShared { chars: Vec<u64> }, KeptUnreadable { chars: Vec<u64> }, KeptNoAccountFile }` — `#[serde(tag = "outcome", rename_all = "snake_case")]`
  - `pub struct ChatLeaveResult { pub layout: WindowLayout, pub account: AccountOutcome }` — `Serialize`
  - `pub fn linked_chars(state: &AppState, dir: &Path) -> Vec<(u64, PathBuf)>`
  - `pub fn chat_leave(state: &AppState, window_id: &str, linked: &[(u64, PathBuf)]) -> Result<ChatLeaveResult, ErrDto>`
  - Tauri command `chat_leave { window: String } -> ChatLeaveResult`

- [ ] **Step 1: Write the failing tests** — inside `ops.rs`'s `mod tests` (the module's `bb`, `encode`, `temp_file`, `tree_of`, `depth` helpers are in scope, as `two_slot_state` uses them):

```rust
    // ---- chat_leave ----

    const BEAN: &str = "chatchannel_player_-88620541";

    fn chat_char_bytes(keys: &[&str]) -> Vec<u8> {
        let ts = || Value::Long(vec![0u8; 8]);
        let geom = Value::Tuple(vec![Value::Int(0), Value::Int(0), Value::Int(256), Value::Int(424), Value::Int(2560), Value::Int(1440)]);
        let wins = keys.iter().map(|k| (bb(&format!("chatchannel_{k}")), geom.clone())).collect();
        let rows = keys
            .iter()
            .map(|k| Value::Tuple(vec![Value::Str((*k).into()), Value::Str((*k).into()), Value::Str(format!("Name {k}"))]))
            .collect();
        encode(&Value::Dict(vec![
            (bb("windows"), Value::Dict(vec![(bb("windowSizesAndPositions_1"), Value::Tuple(vec![ts(), Value::Dict(wins)]))])),
            (bb("ui"), Value::Dict(vec![(bb("chatchannels"), Value::Tuple(vec![ts(), Value::List(rows)]))])),
        ]))
        .unwrap()
    }

    fn chat_user_bytes(with_keys: bool) -> Vec<u8> {
        let ts = || Value::Long(vec![0u8; 8]);
        let mut ui = vec![(bb("neocomWidth"), Value::Tuple(vec![ts(), Value::Int(37)]))];
        if with_keys {
            ui.push((bb(&format!("{BEAN}_userlistwidth")), Value::Tuple(vec![ts(), Value::Int(104)])));
            ui.push((bb("chatPlayerChannelsJoined"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (Value::Str("player_-88620541".into()), Value::Str("Bean-Intel".into())),
            ])])));
        }
        encode(&Value::Dict(vec![(bb("ui"), Value::Dict(ui))])).unwrap()
    }

    /// Open char (in Bean + one other channel) and, optionally, the account file.
    fn chat_state(user: Option<Vec<u8>>) -> AppState {
        let state = AppState::new();
        if let Some(u) = user {
            let upath = temp_file("chat-user", &u);
            open_file(&state, Slot::User, upath.to_str().unwrap()).unwrap();
        }
        let cpath = temp_file("chat-char", &chat_char_bytes(&["player_-88620541", "player_-1"]));
        open_file(&state, Slot::Char, cpath.to_str().unwrap()).unwrap();
        state
    }

    fn sibling(id: u64, keys: &[&str]) -> (u64, PathBuf) {
        (id, temp_file(&format!("chat-sib-{id}"), &chat_char_bytes(keys)))
    }

    #[test]
    fn chat_leave_cleans_the_account_when_no_sibling_has_the_channel() {
        let state = chat_state(Some(chat_user_bytes(true)));
        let r = chat_leave(&state, BEAN, &[sibling(2, &["player_-1"])]).unwrap();
        assert_eq!(r.account, AccountOutcome::Cleaned);
        assert!(!r.layout.windows.iter().any(|w| w.id == BEAN));
        assert!(settings_model::project_chat(&tree_of(&state, Slot::User)).is_empty());
    }

    #[test]
    fn chat_leave_keeps_the_account_when_a_sibling_has_the_channel() {
        let state = chat_state(Some(chat_user_bytes(true)));
        let user_before = tree_of(&state, Slot::User);
        let r = chat_leave(&state, BEAN, &[sibling(2, &["player_-88620541"]), sibling(3, &[])]).unwrap();
        assert_eq!(r.account, AccountOutcome::KeptShared { chars: vec![2] });
        assert_eq!(tree_of(&state, Slot::User), user_before);
    }

    #[test]
    fn chat_leave_keeps_the_account_when_a_sibling_is_unreadable() {
        let state = chat_state(Some(chat_user_bytes(true)));
        let user_before = tree_of(&state, Slot::User);
        let bad = (5, temp_file("chat-sib-bad", b"not a settings file"));
        let r = chat_leave(&state, BEAN, &[bad]).unwrap();
        assert_eq!(r.account, AccountOutcome::KeptUnreadable { chars: vec![5] });
        assert_eq!(tree_of(&state, Slot::User), user_before);
    }

    #[test]
    fn chat_leave_without_an_account_file_still_leaves() {
        let state = chat_state(None);
        let r = chat_leave(&state, BEAN, &[]).unwrap();
        assert_eq!(r.account, AccountOutcome::KeptNoAccountFile);
        assert!(!r.layout.windows.iter().any(|w| w.id == BEAN));
    }

    #[test]
    fn chat_leave_with_no_account_keys_reports_nothing_to_clean() {
        let state = chat_state(Some(chat_user_bytes(false)));
        let d0 = depth(&state);
        let r = chat_leave(&state, BEAN, &[]).unwrap();
        assert_eq!(r.account, AccountOutcome::NothingToClean);
        assert_eq!(depth(&state), d0 + 1);
    }

    #[test]
    fn one_undo_reverts_both_files() {
        let state = chat_state(Some(chat_user_bytes(true)));
        let (u0, c0) = (tree_of(&state, Slot::User), tree_of(&state, Slot::Char));
        let d0 = depth(&state);
        chat_leave(&state, BEAN, &[]).unwrap();
        assert_eq!(depth(&state), d0 + 1, "one command, one undo entry");
        assert!(undo::undo(&state).is_some());
        assert_eq!(tree_of(&state, Slot::User), u0);
        assert_eq!(tree_of(&state, Slot::Char), c0);
    }

    #[test]
    fn chat_leave_refuses_a_standing_channel_and_changes_nothing() {
        let state = chat_state(Some(chat_user_bytes(true)));
        let (u0, c0) = (tree_of(&state, Slot::User), tree_of(&state, Slot::Char));
        let e = chat_leave(&state, "chatchannel_local", &[]).unwrap_err();
        assert_eq!(e.code, "not_leavable");
        assert_eq!(tree_of(&state, Slot::User), u0);
        assert_eq!(tree_of(&state, Slot::Char), c0);
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p app --lib ops::tests::chat_leave ops::tests::one_undo_reverts_both_files`
Expected: compile error — `chat_leave`, `AccountOutcome` not found.

- [ ] **Step 3: Implement** — in `ops.rs` after `stack_delete_orphans`:

```rust
/// What a chat leave did to the ACCOUNT file, which every character on the
/// account shares.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum AccountOutcome {
    Cleaned,
    /// No linked character has the channel, but none of its account keys exist.
    NothingToClean,
    /// These linked characters are still in the channel.
    KeptShared { chars: Vec<u64> },
    /// These linked characters' files could not be read, so it is unknown.
    KeptUnreadable { chars: Vec<u64> },
    /// No account file open, or it is read-only.
    KeptNoAccountFile,
}

#[derive(Debug, Serialize)]
pub struct ChatLeaveResult {
    pub layout: WindowLayout,
    pub account: AccountOutcome,
}

/// The characters sharing the open account file (see
/// `accounts::linked_char_files`). Empty when no account file is open.
pub fn linked_chars(state: &AppState, dir: &Path) -> Vec<(u64, PathBuf)> {
    let user = state.user.lock().unwrap().as_ref().map(|d| d.path.clone());
    let char_path = state.char.lock().unwrap().as_ref().map(|d| d.path.clone());
    let Some(user) = user else { return Vec::new() };
    crate::accounts::linked_char_files(&crate::accounts::load_store(dir), &user, char_path.as_deref())
}

/// Leave a player channel or private conversation: the character side always,
/// the account side only when no linked character is still in it
/// (docs/superpowers/specs/2026-09-28-chat-leave-design.md). One undo group,
/// so one `Ctrl+Z` reverts both files.
pub fn chat_leave(state: &AppState, window_id: &str, linked: &[(u64, PathBuf)]) -> Result<ChatLeaveResult, ErrDto> {
    let _group = undo::group(state);
    let key = edit_slot(state, Slot::Char, |v| settings_model::leave_chat_char(v, window_id), |e| coded_err("chat_leave", e))?;
    let account = chat_leave_account(state, window_id, &key, linked)?;
    Ok(ChatLeaveResult { layout: window_layout(state, Slot::Char)?, account })
}

fn chat_leave_account(state: &AppState, window_id: &str, key: &str, linked: &[(u64, PathBuf)]) -> Result<AccountOutcome, ErrDto> {
    // A clone to probe on: the edit below runs only when it would remove
    // something, so a no-op does not mark the account file unsaved.
    let mut probe = match state.user.lock().unwrap().as_ref() {
        Some(d) if !matches!(d.fidelity, Fidelity::ReadOnly { .. }) => d.value.clone(),
        _ => return Ok(AccountOutcome::KeptNoAccountFile),
    };
    let (mut shared, mut unreadable) = (Vec::new(), Vec::new());
    for (id, path) in linked {
        match std::fs::read(path).ok().and_then(|b| blue_marshal::decode(&b).ok()) {
            Some(doc) if settings_model::chat_channel_keys(&doc).iter().any(|k| k == key) => shared.push(*id),
            Some(_) => {}
            None => unreadable.push(*id),
        }
    }
    if !shared.is_empty() {
        return Ok(AccountOutcome::KeptShared { chars: shared });
    }
    if !unreadable.is_empty() {
        return Ok(AccountOutcome::KeptUnreadable { chars: unreadable });
    }
    if !settings_model::leave_chat_account(&mut probe, window_id) {
        return Ok(AccountOutcome::NothingToClean);
    }
    edit_slot(
        state,
        Slot::User,
        |v| Ok::<_, std::convert::Infallible>(settings_model::leave_chat_account(v, window_id)),
        |e| match e {},
    )?;
    Ok(AccountOutcome::Cleaned)
}
```

Add `use std::path::{Path, PathBuf};` to `ops.rs` if not already imported (check the top of the file), and `WindowLayout` from `settings_model` if not already in scope (`window_layout`'s return type already is).

In `lib.rs`, after `stack_delete_orphans`:

```rust
#[tauri::command]
fn chat_leave(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    window: String,
) -> Result<ops::ChatLeaveResult, ErrDto> {
    let linked = ops::linked_chars(&state, &app_dir(&app));
    ops::chat_leave(&state, &window, &linked)
}
```

and add `chat_leave` to the `generate_handler!` list right after `stack_delete_orphans` (line ~759).

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p app --lib ops::` then `cargo build -p app`
Expected: all pass; build clean. Run the build in the background if it exceeds a few minutes (release-build watchdog, see memory).

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs
git commit -m "App: chat_leave command, one undo step across both files"
```

---

### Task 5: MCP `chat_leave` op

**Files:**
- Modify: `app/src-tauri/src/mcp.rs` — `batch` (~line 2094), the `layout_edit` arm (~line 1236), the `layout_edit` ToolDef (~lines 882-891), tests near `layout_edit_stack_ops_round_trip` (~line 3549)
- Modify: `app/src-tauri/src/mcp_primer.md:100`

**Interfaces:**
- Consumes: `ops::chat_leave`, `ops::linked_chars`, `ops::ChatLeaveResult` (Task 4).
- Produces: `layout_edit` op `{"op": "chat_leave", "window": "<id>"}`; result gains `"chat_leave": [{"window", "account": {"outcome", "chars"?}}]` when the batch had any.

- [ ] **Step 1: Write the failing tests** — in `mcp.rs`'s tests, next to `layout_edit_stack_ops_round_trip`:

```rust
    fn chat_char_mcp_bytes() -> Vec<u8> {
        let ts = || BmValue::Long(vec![0u8; 8]);
        let geom = BmValue::Tuple(vec![BmValue::Int(0), BmValue::Int(0), BmValue::Int(256), BmValue::Int(424), BmValue::Int(2560), BmValue::Int(1440)]);
        encode(&BmValue::Dict(vec![
            (b("windows"), BmValue::Dict(vec![
                (b("windowSizesAndPositions_1"), BmValue::Tuple(vec![ts(), BmValue::Dict(vec![
                    (b("chatchannel_player_-5"), geom.clone()), (b("market"), geom),
                ])])),
                (b("openWindows"), BmValue::Tuple(vec![ts(), BmValue::Dict(vec![
                    (b("chatchannel_player_-5"), BmValue::Bool(true)), (b("market"), BmValue::Bool(true)),
                ])])),
            ])),
            (b("ui"), BmValue::Dict(vec![(b("chatchannels"), BmValue::Tuple(vec![ts(), BmValue::List(vec![
                BmValue::Tuple(vec![BmValue::Str("player_-5".into()), BmValue::Str("player_-5".into()), BmValue::Str("Intel".into())]),
            ])]))])),
        ]))
        .unwrap()
    }

    #[test]
    fn layout_edit_chat_leave_reports_the_account_outcome() {
        let (s, _) = open_char(&chat_char_mcp_bytes());
        let v = s.call("layout_edit", &args(json!({ "ops": [{ "op": "chat_leave", "window": "chatchannel_player_-5" }] }))).unwrap();
        assert_eq!(v["chat_leave"][0]["window"], "chatchannel_player_-5");
        assert_eq!(v["chat_leave"][0]["account"]["outcome"], "kept_no_account_file");
        let all = s.call("layout_get", &args(json!({ "include_closed": true, "hide_clutter": false }))).unwrap();
        assert!(!all["windows"].as_array().unwrap().iter().any(|w| w["id"] == "chatchannel_player_-5"));
    }

    #[test]
    fn layout_edit_chat_leave_refuses_a_standing_channel() {
        let (s, _) = open_char(&chat_char_mcp_bytes());
        let e = s.call("layout_edit", &args(json!({ "ops": [{ "op": "chat_leave", "window": "chatchannel_local" }] }))).unwrap_err();
        assert_eq!(e["code"], "not_leavable");
    }

    #[test]
    fn a_failing_later_op_rolls_back_the_leave() {
        let (s, _) = open_char(&chat_char_mcp_bytes());
        let e = s.call("layout_edit", &args(json!({ "ops": [
            { "op": "chat_leave", "window": "chatchannel_player_-5" },
            { "op": "set_geometry", "window": "nope", "x": 1 }
        ]}))).unwrap_err();
        assert_eq!(e["op_index"], 1);
        let all = s.call("layout_get", &args(json!({ "include_closed": true, "hide_clutter": false }))).unwrap();
        assert!(all["windows"].as_array().unwrap().iter().any(|w| w["id"] == "chatchannel_player_-5"));
    }

    #[test]
    fn a_batch_without_chat_leave_has_no_chat_leave_field() {
        let (s, _) = open_char(&layout_char_bytes());
        let v = s.call("layout_edit", &args(json!({ "ops": [{ "op": "set_geometry", "window": "market", "x": 5 }] }))).unwrap();
        assert!(v.get("chat_leave").is_none());
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p app --lib mcp::tests::layout_edit_chat_leave mcp::tests::a_failing_later_op_rolls_back_the_leave`
Expected: FAIL — `unknown_op` for `chat_leave`.

- [ ] **Step 3: Implement**

(a) `batch` takes a closure instead of a fn pointer — change its parameter (line ~2097):

```rust
        apply: impl Fn(&AppState, &Args) -> Result<(), Value>,
```

Every existing caller passes a fn item, which satisfies `impl Fn`; nothing else changes.

(b) Replace the `layout_edit` arm (line ~1236):

```rust
            "layout_edit" => {
                // chat_leave needs the app dir (to resolve linked characters)
                // and reports a per-op outcome, so it is handled here rather
                // than in the plain `layout_op`.
                let left = std::cell::RefCell::new(Vec::new());
                let dir = self.dir.clone();
                let v = self.batch(
                    args,
                    |st, a| {
                        if a.get("op").and_then(Value::as_str) != Some("chat_leave") {
                            return layout_op(st, a);
                        }
                        let w: String = req(a, "window")?;
                        let r = ops::chat_leave(st, &w, &ops::linked_chars(st, &dir)).map_err(fail)?;
                        left.borrow_mut().push(json!({ "window": w, "account": r.account }));
                        Ok(())
                    },
                    |s| {
                        let wl = ops::window_layout(&s.state(), Slot::Char).map_err(fail)?;
                        Ok(layout_view(&wl, &window_filter(&Args::new(), s.window_view().as_ref())?, &s.overrides()))
                    },
                )?;
                let left = left.into_inner();
                Ok(if left.is_empty() { v } else { let mut v = v; v["chat_leave"] = json!(left); v })
            }
```

(c) The ToolDef: append to the description, before "Returns the open, non-clutter windows…":
`chat_leave {window} (leave a player channel or private conversation — chatchannel_player_* / chatchannel_private_*; standing channels like local, corp, fleet are refused: removes the window and the channel from the character file, and its per-channel settings from the account file unless another character on the account is still in it; each is reported under chat_leave[].account.outcome: cleaned, nothing_to_clean, kept_shared, kept_unreadable, kept_no_account_file). `
and add `"chat_leave"` to the `op_item` list: `&["set_geometry", "set_flag", "stack_create", "stack_add", "stack_unstack", "stack_reorder", "stack_delete_orphans", "chat_leave"]`.

(d) `mcp_primer.md:100` — append one sentence to the paragraph:
`` `chat_leave {window}` leaves a player channel or private conversation for good — the window and the channel go from the character file, so EVE does not rebuild it; standing channels (local, corp, alliance, fleet) cannot be left. The character must be logged out, like every edit.``

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p app --lib mcp::` then `cargo test -p app --test mcp_stdio`
Expected: all pass (the stdio suite covers the schema invariants for the changed ToolDef).

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/mcp.rs app/src-tauri/src/mcp_primer.md
git commit -m "MCP: chat_leave op in layout_edit"
```

---

### Task 6: UI — Leave channel button

**Files:**
- Modify: `app/src/lib/api.ts` (after `stackDeleteOrphans`, ~line 641; types near `WindowLayout`, ~line 234)
- Modify: `app/src/lib/LayoutView.svelte` (handler near `onDeleteOrphans`, ~line 342; prop pass at ~line 1087)
- Modify: `app/src/lib/WindowPanel.svelte` (prop ~lines 15-75; chat section ~line 304)
- Test: `app/src/lib/WindowPanel.spec.ts`, `app/src/lib/layout.test.ts`

**Interfaces:**
- Consumes: Tauri command `chat_leave { window }` → `ChatLeaveResult` (Task 4).
- Produces: `api.chatLeave(window: string): Promise<ChatLeaveResult>`; `isLeavableChat(id: string): boolean` and `leaveToast(label: string, r: AccountOutcome, nameOf: (id: number) => string): string` in `layout.ts`; `WindowPanel` prop `onLeaveChat: (w: WindowRect) => void`.

- [ ] **Step 1: Write the failing tests**

In `app/src/lib/layout.test.ts` (append):

```ts
import { isLeavableChat, leaveToast } from "./layout";

describe("chat leave", () => {
  test("only player channels and private conversations are leavable", () => {
    expect(isLeavableChat("chatchannel_player_-88620541")).toBe(true);
    expect(isLeavableChat("chatchannel_private_009e6df0")).toBe(true);
    for (const id of ["chatchannel_local", "chatchannel_corp", "chatchannel_fleet", "chatchannel_player_", "market"]) {
      expect(isLeavableChat(id)).toBe(false);
    }
  });

  test("toast wording per account outcome", () => {
    const nameOf = (id: number) => (id === 2 ? "Other Char" : String(id));
    expect(leaveToast("Bean-Intel", { outcome: "cleaned" }, nameOf)).toBe("Left Bean-Intel.");
    expect(leaveToast("Bean-Intel", { outcome: "nothing_to_clean" }, nameOf)).toBe("Left Bean-Intel.");
    expect(leaveToast("Bean-Intel", { outcome: "kept_shared", chars: [2] }, nameOf)).toBe(
      "Left Bean-Intel. Its account settings stay: Other Char still has it.",
    );
    expect(leaveToast("Bean-Intel", { outcome: "kept_unreadable", chars: [2, 3] }, nameOf)).toBe(
      "Left Bean-Intel. Its account settings stay: Other Char and 3's files couldn't be read.",
    );
    expect(leaveToast("Bean-Intel", { outcome: "kept_no_account_file" }, nameOf)).toBe(
      "Left Bean-Intel. Open the account file to clear its account settings too.",
    );
  });
});
```

(If `layout.test.ts` already imports `describe`/`test`/`expect` from vitest, do not re-import them.)

In `app/src/lib/WindowPanel.spec.ts`: add `onLeaveChat: vi.fn()` to the `spies` object in `mount`, then append:

```ts
describe("Leave channel", () => {
  test("shown on a player channel and fires with the window", async () => {
    const w = win("chatchannel_player_-5", { name: "Intel" } as Partial<WindowRect>);
    const { onLeaveChat } = mount([w], { selectedId: w.id });
    await fireEvent.click(screen.getByRole("button", { name: "Leave channel" }));
    expect(onLeaveChat).toHaveBeenCalledWith(expect.objectContaining({ id: "chatchannel_player_-5" }));
  });

  test("a private conversation says conversation", () => {
    const w = win("chatchannel_private_abc");
    mount([w], { selectedId: w.id });
    expect(screen.getByRole("button", { name: "Leave conversation" })).toBeTruthy();
  });

  test("absent on a standing channel", () => {
    const w = win("chatchannel_local");
    mount([w], { selectedId: w.id });
    expect(screen.queryByRole("button", { name: /Leave/ })).toBeNull();
  });

  test("disabled on a read-only file", () => {
    const w = win("chatchannel_player_-5");
    mount([w], { selectedId: w.id, readOnly: true });
    expect((screen.getByRole("button", { name: "Leave channel" }) as HTMLButtonElement).disabled).toBe(true);
  });
});
```

(`mount` must return the spies — it already does if other tests destructure `onGeom` from it, as at line 112.)

- [ ] **Step 2: Run to verify failure**

Run (from `app/`): `npx vitest run src/lib/layout.test.ts src/lib/WindowPanel.spec.ts`
Expected: FAIL — `isLeavableChat` not exported; no "Leave channel" button.

- [ ] **Step 3: Implement**

`api.ts` — types after `WindowLayout`:

```ts
export type AccountOutcome =
  | { outcome: "cleaned" }
  | { outcome: "nothing_to_clean" }
  | { outcome: "kept_shared"; chars: number[] }
  | { outcome: "kept_unreadable"; chars: number[] }
  | { outcome: "kept_no_account_file" };

export interface ChatLeaveResult {
  layout: WindowLayout;
  account: AccountOutcome;
}
```

and in the `api` object after `stackDeleteOrphans`:

```ts
  chatLeave: (window: string) => invoke<ChatLeaveResult>("chat_leave", { window }),
```

`layout.ts` — append:

```ts
import type { AccountOutcome } from "./api";

/** Mirrors settings-model `chat.rs::is_leavable`: player channels and private
 *  conversations only. Standing channels are the server's and come back. */
export function isLeavableChat(id: string): boolean {
  return ["chatchannel_player_", "chatchannel_private_"].some((p) => id.startsWith(p) && id.length > p.length);
}

function listNames(ids: number[], nameOf: (id: number) => string): string {
  const n = ids.map(nameOf);
  return n.length <= 1 ? (n[0] ?? "") : `${n.slice(0, -1).join(", ")} and ${n[n.length - 1]}`;
}

/** The toast after a leave, by what happened to the shared account file. */
export function leaveToast(label: string, r: AccountOutcome, nameOf: (id: number) => string): string {
  switch (r.outcome) {
    case "cleaned":
    case "nothing_to_clean":
      return `Left ${label}.`;
    case "kept_shared":
      return `Left ${label}. Its account settings stay: ${listNames(r.chars, nameOf)} still has it.`;
    case "kept_unreadable":
      return `Left ${label}. Its account settings stay: ${listNames(r.chars, nameOf)}'s files couldn't be read.`;
    case "kept_no_account_file":
      return `Left ${label}. Open the account file to clear its account settings too.`;
  }
}
```

(Put the `import type` at the top of `layout.ts` with its other imports.)

`LayoutView.svelte` — imports: add `isLeavableChat` is not needed here; add `leaveToast` to the existing `./layout` import, and `import { names } from "./names.svelte";` if not already imported. After `onDeleteOrphans`:

```ts
  // No confirm: the edit is in memory and one Ctrl+Z reverts both files, the
  // same reasoning as onDeleteOrphans above.
  async function onLeaveChat(w: WindowRect) {
    stackError = null;
    try {
      const r = await api.chatLeave(w.id);
      layout = r.layout;
      onDirty("char");
      if (r.account.outcome === "cleaned") onDirty("user");
      if (selectedId === w.id) selectedId = null;
      const label = w.name ?? w.label;
      toast(`${leaveToast(label, r.account, (id) => names[id]?.name ?? String(id))} Save to write it to disk.`, {
        action: undoAction(),
      });
    } catch (e) {
      stackError = { text: `The channel wasn't left — ${errText(e)}`, detail: errMessage(e) };
    }
  }
```

and pass `{onLeaveChat}` to `<WindowPanel … >` beside `{onDeleteOrphans}` (~line 1091).

`WindowPanel.svelte` — add `onLeaveChat,` to the destructured props and `onLeaveChat: (w: WindowRect) => void;` to the prop types (beside `onDeleteOrphans`). Import `isLeavableChat` from `./layout`. Inside `{#if w.id.startsWith("chatchannel_")}` at line ~304, before the `{@const chatStack …}` line:

```svelte
      {#if isLeavableChat(w.id)}
        <div class="leave-chat">
          <Button size="sm" type="button" disabled={readOnly} onclick={() => onLeaveChat(w)}
            title="Remove this channel from the character, so EVE does not bring it back">
            {w.id.startsWith("chatchannel_private_") ? "Leave conversation" : "Leave channel"}
          </Button>
        </div>
      {/if}
```

Match the `disabled`/`disabledReason` prop convention `Button` uses elsewhere in this file (e.g. the `Unstack` button at ~line 520): if `Button` takes `disabledReason`, pass `disabledReason="This file is read-only"`. Add `.leave-chat { margin-block: var(--space-2, 8px); }` to the component's `<style>` only if the file styles its other action rows the same way; otherwise omit it.

- [ ] **Step 4: Run to verify pass**

Run (from `app/`): `npx vitest run src/lib/layout.test.ts src/lib/WindowPanel.spec.ts src/lib/LayoutView.spec.ts`, then `npm run check`, then `npm test`
Expected: exit code 0 for each (check the exit code, not the summary line — `npm test` can exit 1 with every test passing).

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/api.ts app/src/lib/layout.ts app/src/lib/layout.test.ts app/src/lib/LayoutView.svelte app/src/lib/WindowPanel.svelte app/src/lib/WindowPanel.spec.ts
git commit -m "UI: leave a chat channel from the layout editor"
```

---

### Task 7: Docs, spec amendments, and the live check

**Files:**
- Modify: `docs/format-notes.md` (new section after "Chat window splits", ~line 1262)
- Modify: `docs/superpowers/specs/2026-09-28-chat-leave-design.md` (§4.2 outcomes, §4.4 UI)

- [ ] **Step 1: format-notes.md** — add after the "Chat window splits" section:

```markdown
### Leaving a chat channel

Verified in game 2026-09-28 on Holy Storm (`core_char_96821229`), Bean-Intel
(`chatchannel_player_-88620541`, a pinned member of `ChatWindowStack`).

**The character file's `ui → chatchannels` row is what brings a chat window
back.** Removing only the window's entries under `windows` (what a layout copy
does) is undone on the next login. Removing the window from every `windows`
dict AND its `chatchannels` row sticks: the client's rewrite differed from the
edited file by nothing but timestamps, and the other tabs' stack indices were
left as they were. In game, the channel then shows only as a join suggestion.

The account file keeps per-channel state under the root `ui` section, keyed by
the window id: `<W>_userlistwidth`, `chatinputsize_<W>`, `chatfontsize_<W>`,
`chatWindowBlink_<W>`, `chatCondensedUserList_<W>` or `chatCondensedUserList_<key>`,
and `chatPlayerChannelsJoined` (`(timestamp, dict)` of `player_<key>` → label,
player channels only). It survived the logout above untouched — the likely
source of the join suggestion. `chat.rs::leave_chat_account` clears it only
when no other paired character in the same folder is in the channel.

The channel's settings dialog is a window of its own, `ChannelSettingsDlg_<key>`,
and goes with it.
```

- [ ] **Step 2: Spec amendments** — in the spec:
  - §4.2: add `NothingToClean` to the outcome list, described as "the account file is open and no linked character has the channel, but none of §3.2's keys exist; not marked unsaved".
  - §4.4: replace the "Confirm, house style" bullet with "No confirm: the edit is in memory and one `Ctrl+Z` reverts it, the `onDeleteOrphans` precedent. The toast carries an undo action." Delete the command-palette bullet and add "No palette entry: no layout window action has one."

- [ ] **Step 3: Commit**

```bash
git add docs/format-notes.md docs/superpowers/specs/2026-09-28-chat-leave-design.md
git commit -m "Docs: leaving a chat channel, and the spec's planning amendments"
```

- [ ] **Step 4: Live check (the user, in game)** — needs a build and the user's EVE client; do not attempt it autonomously. Hand the user:
  1. With Holy Storm logged out, open it in the app (character + account file), select a private conversation window, **Leave conversation**, save. Log in, check the conversation is gone, log out.
  2. Leave one more player channel on Holy Storm whose toast reads just "Left <name>. Save to write it to disk." — the account side was cleaned — then save, log in, and check whether that channel's join suggestion is gone. (Bean-Intel itself cannot be used: the spike already removed its window and row, so there is nothing left to leave; its `chatPlayerChannelsJoined` entry stays until EVE or a later tidy-up drops it.)
  3. Record the result in `docs/format-notes.md` under "Leaving a chat channel" (suggestion gone / suggestion survives) and commit. If the suggestion survives, the release note makes no promise about suggestions (spec §5).
```
