//! Read-only projection of the per-channel chat window splits: the member-list
//! width and the input-box height. Both live in the ACCOUNT document, under the
//! root `ui` section — corpus-verified 2026-07-30 (705 sightings across 184
//! real account files, zero under `windows`). See docs/format-notes.md, "Chat
//! window splits".
//!
//! Nothing here mutates. The canvas detail layer draws these; no editor writes
//! them (design spec §6).

use std::collections::BTreeMap;

use blue_marshal::Value;
use serde::Serialize;

use crate::treewalk::{collect_shared, effective, section, text, SharedTable};

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
}
