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
use crate::treewalk::{collect_shared, effective, inline_all, is_bytes, section, text, unwrap_shared, SharedTable};

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

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    fn ts() -> Value {
        Value::Long(vec![0u8; 8])
    }

    fn b(s: &str) -> Value {
        Value::Bytes(s.as_bytes().to_vec())
    }

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
}
