//! Read-only projection of the per-channel chat window splits: the member-list
//! width and the input-box height. Both live in the ACCOUNT document, under the
//! root `ui` section — corpus-verified 2026-07-30 (705 sightings across 184
//! real account files, zero under `windows`). See docs/format-notes.md, "Chat
//! window splits".
//!
//! The read path is `project_chat`; the write path is `set_chat_splits`. Only a
//! mint (an absent key being created) de-shares the document — the same
//! reshare contract `hud.rs::set_hud_value` uses.

use std::collections::BTreeMap;

use blue_marshal::Value;
use serde::Serialize;

use crate::path::{NodePath, Step};
use crate::treewalk::{collect_shared, effective, inline_all, is_bytes, key_is, section, text, unwrap_shared, SharedTable};
use crate::windows::decode_id;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChatPanel {
    /// The canvas window id, e.g. `chatchannel_local` — taken verbatim out of
    /// the key name, so there is no mapping table to get wrong.
    pub window_id: String,
    /// `None` when the player has never resized this channel's member list.
    /// The canvas then draws no split rather than inventing one.
    pub userlist_width: Option<i64>,
    pub input_height: Option<i64>,
}

/// Key shapes. Both carry the window id verbatim: `chatchannel_local` owns
/// `chatchannel_local_userlistwidth` and `chatinputsize_chatchannel_local`.
const WIDTH_SUFFIX: &str = "_userlistwidth";
const INPUT_PREFIX: &str = "chatinputsize_";
/// Only chat windows. A `_userlistwidth` key on some other window would produce
/// a panel no canvas rectangle can ever match.
const CHAT_PREFIX: &str = "chatchannel_";

/// Project every chat window that has at least one of the two keys.
///
/// `BTreeMap` rather than a `Vec` scan: the two keys for one channel are not
/// adjacent in the section, and the ordering it gives for free is what makes
/// the output deterministic regardless of dict order.
pub fn project_chat(user_root: &Value) -> Vec<ChatPanel> {
    let mut shared = SharedTable::new();
    collect_shared(user_root, &mut shared);
    let Some((entries, _)) = section(user_root, b"ui", &shared) else {
        return Vec::new();
    };
    let mut by_id: BTreeMap<String, ChatPanel> = BTreeMap::new();
    for (k, v) in entries {
        // Keys are resolved through Ref/Shared: real files dedup repeated
        // strings, so a bare Bytes match reads nothing from them.
        let Some(name) = text(k, &shared) else { continue };
        let (id, is_width) = match name.strip_suffix(WIDTH_SUFFIX) {
            Some(id) => (id, true),
            None => match name.strip_prefix(INPUT_PREFIX) {
                Some(id) => (id, false),
                None => continue,
            },
        };
        if !id.starts_with(CHAT_PREFIX) {
            continue;
        }
        let Some(n) = leaf_int(v, &shared) else { continue };
        let e = by_id.entry(id.to_string()).or_insert_with(|| ChatPanel {
            window_id: id.to_string(),
            userlist_width: None,
            input_height: None,
        });
        // First wins, matching every other read path here (`.find()` semantics):
        // a duplicate key must not silently override the entry reads land on.
        let field = if is_width { &mut e.userlist_width } else { &mut e.input_height };
        if field.is_none() {
            *field = Some(n);
        }
    }
    by_id.into_values().collect()
}

/// The `Int` inside a `(timestamp, value)` leaf. A bare value is tolerated the
/// way `hud.rs::leaf` tolerates one; anything else reads as absent rather than
/// panicking, mirroring `windows.rs`'s malformed-tuple skip.
fn leaf_int(v: &Value, shared: &SharedTable) -> Option<i64> {
    let v = match effective(v, shared) {
        Value::Tuple(items) if items.len() == 2 => effective(&items[1], shared),
        other => other,
    };
    match v {
        Value::Int(i) => Some(*i),
        _ => None,
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    use crate::testkit::{b, ts};

    /// (timestamp, value) — the file-wide value-wrapper convention.
    fn wrapped(v: Value) -> Value {
        Value::Tuple(vec![ts(), v])
    }

    /// An account document whose root `ui` section holds `entries`.
    fn user_doc(entries: Vec<(Value, Value)>) -> Value {
        Value::Dict(vec![(b("ui"), Value::Dict(entries))])
    }

    fn panel<'a>(panels: &'a [ChatPanel], id: &str) -> &'a ChatPanel {
        panels.iter().find(|p| p.window_id == id).expect("panel present")
    }

    #[test]
    fn projects_both_keys_for_one_channel() {
        let doc = user_doc(vec![
            (b("chatchannel_local_userlistwidth"), wrapped(Value::Int(135))),
            (b("chatinputsize_chatchannel_local"), wrapped(Value::Int(64))),
        ]);
        let panels = project_chat(&doc);
        assert_eq!(panels.len(), 1);
        let p = panel(&panels, "chatchannel_local");
        assert_eq!(p.userlist_width, Some(135));
        assert_eq!(p.input_height, Some(64));
    }

    #[test]
    fn a_channel_with_only_one_key_projects_the_other_as_none() {
        let doc = user_doc(vec![
            (b("chatchannel_fleet_userlistwidth"), wrapped(Value::Int(107))),
            (b("chatinputsize_chatchannel_corp"), wrapped(Value::Int(63))),
        ]);
        let panels = project_chat(&doc);
        assert_eq!(panels.len(), 2);
        assert_eq!(panel(&panels, "chatchannel_fleet").userlist_width, Some(107));
        assert_eq!(panel(&panels, "chatchannel_fleet").input_height, None);
        assert_eq!(panel(&panels, "chatchannel_corp").userlist_width, None);
        assert_eq!(panel(&panels, "chatchannel_corp").input_height, Some(63));
    }

    /// The states-slice regression, and the one that would make this project
    /// nothing from every real account file: real files `Shared`/`Ref` their
    /// repeated keys, and the account file's section key is itself `Ref`-keyed.
    #[test]
    fn resolves_a_shared_section_key_and_a_ref_entry_key() {
        let doc = Value::Dict(vec![(
            Value::Shared { slot: 1, value: Box::new(b("ui")) },
            Value::Dict(vec![
                (Value::Shared { slot: 2, value: Box::new(b("chatchannel_local_userlistwidth")) },
                 wrapped(Value::Int(135))),
                (Value::Ref(2), wrapped(Value::Int(999))),
            ]),
        )]);
        let panels = project_chat(&doc);
        // Both keys resolve to the same id; the FIRST wins, matching how every
        // other read path here uses `.find()`.
        assert_eq!(panels.len(), 1);
        assert_eq!(panel(&panels, "chatchannel_local").userlist_width, Some(135));
    }

    #[test]
    fn a_malformed_value_is_skipped_not_panicked_on() {
        let doc = user_doc(vec![
            // Not an Int.
            (b("chatchannel_local_userlistwidth"), wrapped(b("wide"))),
            // Wrapper of the wrong arity.
            (b("chatchannel_corp_userlistwidth"), Value::Tuple(vec![ts()])),
            // Readable, so the projection is not simply empty.
            (b("chatchannel_fleet_userlistwidth"), wrapped(Value::Int(107))),
        ]);
        let panels = project_chat(&doc);
        assert_eq!(panels.len(), 1);
        assert_eq!(panel(&panels, "chatchannel_fleet").userlist_width, Some(107));
    }

    /// Only chat windows. `_userlistwidth` on anything else is not a chat
    /// window id and would produce a panel the canvas can never match.
    #[test]
    fn ignores_keys_that_are_not_chat_windows() {
        let doc = user_doc(vec![
            (b("neocomWidth"), wrapped(Value::Int(37))),
            (b("someotherwindow_userlistwidth"), wrapped(Value::Int(90))),
        ]);
        assert!(project_chat(&doc).is_empty());
    }

    #[test]
    fn a_document_with_no_ui_section_projects_nothing() {
        let doc = Value::Dict(vec![(b("windows"), Value::Dict(vec![]))]);
        assert!(project_chat(&doc).is_empty());
    }

    #[test]
    fn panels_come_back_sorted_by_window_id() {
        let doc = user_doc(vec![
            (b("chatchannel_local_userlistwidth"), wrapped(Value::Int(135))),
            (b("chatchannel_alliance_userlistwidth"), wrapped(Value::Int(80))),
            (b("chatchannel_fleet_userlistwidth"), wrapped(Value::Int(107))),
        ]);
        let panels = project_chat(&doc);
        let ids: Vec<&str> = panels.iter().map(|p| p.window_id.as_str()).collect();
        assert_eq!(ids, ["chatchannel_alliance", "chatchannel_fleet", "chatchannel_local"]);
    }

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

    /// The only case the pre-validation loop exists for: within one batch, an
    /// EARLIER id would mint (absent, so writable) and a LATER id is refused
    /// (a malformed existing value). Without validating every key before
    /// mutating any, the mutation loop would mint the first id and only then
    /// discover the second is unwritable, leaving a half-written batch.
    #[test]
    fn a_valid_mint_before_an_unwritable_id_writes_nothing() {
        let mut doc = ui_doc(vec![(b("chatchannel_corp_userlistwidth"), wrapped(b("wide")))]);
        let before = doc.clone();
        let ids = vec!["chatchannel_local".into(), "chatchannel_corp".into()];
        let err = set_chat_splits(&mut doc, &ids, Some(90), None).unwrap_err();
        assert!(matches!(err, ChatError::NotEditable(_)));
        assert_eq!(doc, before, "chatchannel_local must not have been minted before chatchannel_corp was found unwritable");
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
            let d = match child {
                Value::Tuple(t) => t.iter().find_map(|e| if let Value::Dict(d) = e { Some(d) } else { None }),
                Value::Dict(d) => Some(d),
                _ => None,
            };
            for (k, v) in d.into_iter().flatten() {
                out.push(decode_id(k));
                if let Value::Dict(inner) = v {
                    out.extend(inner.iter().map(|(k, _)| decode_id(k)));
                }
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
}
