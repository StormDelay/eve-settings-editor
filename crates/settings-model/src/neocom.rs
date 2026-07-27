//! The neocom button bar: `ui → neocomButtonRawData`, a `(timestamp, List)` of
//! `utillib.KeyVal` instances whose state is always exactly
//! `{btnType, children, iconPath, id}` (corpus-verified over 43,430 buttons —
//! see docs/format-notes.md, "Neocom buttons"). Character-side, unlike
//! `neocomWidth`, which is per account.
//!
//! Commands key by INDEX, not id: 11 corpus buttons carry an id that is a
//! `Tuple(bytes, None)` rather than plain bytes, so ids are neither unique nor
//! always well-formed. Reorder and remove move whole instances, so a button's
//! children, icon and odd id ride along untouched.

use blue_marshal::Value;
use serde::Serialize;

use crate::treewalk::{collect_shared, effective, is_bytes, section, SharedTable};

pub const BAR_KEY: &[u8] = b"neocomButtonRawData";
pub const ORIGINAL_KEY: &[u8] = b"neocomButtonRawDataOriginal";

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum NeocomError {
    /// No `ui` section in the document.
    NoUi,
    /// No `neocomButtonRawData` under `ui`.
    NoBar,
    /// Reset was asked for on a document with no `neocomButtonRawDataOriginal`.
    NoOriginal,
    /// A button index that does not exist.
    BadIndex,
    /// A reorder that is not a permutation of the current indices.
    BadOrder,
}

impl std::fmt::Display for NeocomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NeocomError::NoUi => write!(f, "This file has no UI section."),
            NeocomError::NoBar => write!(f, "This file has no neocom buttons."),
            NeocomError::NoOriginal => write!(f, "This character has no original neocom bar to reset to."),
            NeocomError::BadIndex => write!(f, "That neocom button no longer exists."),
            NeocomError::BadOrder => write!(f, "That is not a valid ordering of the neocom buttons."),
        }
    }
}

#[derive(Debug, PartialEq, Serialize)]
pub struct NeocomButton {
    pub index: usize,
    pub id: String,
    pub btn_type: i64,
    pub icon_path: String,
    /// 0 for `None` or an empty list. The write path never re-authors children,
    /// so None-vs-empty is a distinction the UI does not need.
    pub children: usize,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct NeocomBar {
    pub buttons: Vec<NeocomButton>,
    /// `neocomButtonRawDataOriginal`, as-is. Empty when the file has none.
    /// NOT an "addable" set: the frontend unions this with the bundled catalog,
    /// because Original is a stale snapshot that misses buttons later patches
    /// added (spec §2.1).
    pub original: Vec<NeocomButton>,
}

/// The `(timestamp, payload)` payload, or the value itself if unwrapped.
fn payload<'a>(v: &'a Value, sh: &SharedTable<'a>) -> &'a Value {
    match effective(v, sh) {
        Value::Tuple(t) if t.len() == 2 => effective(&t[1], sh),
        other => other,
    }
}

/// A button id: plain `Bytes`, or the `Tuple(bytes, None)` shape 11 corpus
/// buttons carry — rendered as its bytes half either way.
fn id_text(v: &Value, sh: &SharedTable) -> String {
    match effective(v, sh) {
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        Value::Tuple(t) => t.first().map(|e| id_text(e, sh)).unwrap_or_default(),
        _ => String::new(),
    }
}

fn state_field<'a>(state: &'a Value, name: &[u8], sh: &SharedTable<'a>) -> Option<&'a Value> {
    let Value::Dict(d) = effective(state, sh) else { return None };
    d.iter()
        .find(|(k, _)| is_bytes(effective(k, sh), name))
        .map(|(_, v)| effective(v, sh))
}

fn read_button(index: usize, v: &Value, sh: &SharedTable) -> Option<NeocomButton> {
    let Value::Instance { state, .. } = effective(v, sh) else { return None };
    let id = state_field(state, b"id", sh).map(|v| id_text(v, sh)).unwrap_or_default();
    let btn_type = match state_field(state, b"btnType", sh) {
        Some(Value::Int(i)) => *i,
        _ => 0,
    };
    let icon_path = match state_field(state, b"iconPath", sh) {
        Some(Value::Bytes(b)) => String::from_utf8_lossy(b).into_owned(),
        _ => String::new(),
    };
    let children = state_field(state, b"children", sh)
        .and_then(|v| match v {
            Value::List(l) | Value::Tuple(l) => Some(l.len()),
            _ => None,
        })
        .unwrap_or(0);
    Some(NeocomButton { index, id, btn_type, icon_path, children })
}

fn read_list(v: &Value, sh: &SharedTable) -> Vec<NeocomButton> {
    match payload(v, sh) {
        Value::List(l) | Value::Tuple(l) => {
            l.iter().enumerate().filter_map(|(i, b)| read_button(i, b, sh)).collect()
        }
        _ => Vec::new(),
    }
}

pub fn project_neocom(v: &Value) -> Result<NeocomBar, NeocomError> {
    let mut sh = SharedTable::new();
    collect_shared(v, &mut sh);
    // `section` returns (&Entries, NodePath) and resolves a Shared/Ref section
    // key — in account files the root `ui` key is itself a Ref, which a bare
    // is_bytes match misses. The path half is for writers; the reader drops it.
    let (entries, _) = section(v, b"ui", &sh).ok_or(NeocomError::NoUi)?;
    let find = |name: &[u8]| {
        entries.iter().find(|(k, _)| is_bytes(effective(k, &sh), name)).map(|(_, v)| v)
    };
    let bar = find(BAR_KEY).ok_or(NeocomError::NoBar)?;
    Ok(NeocomBar {
        buttons: read_list(bar, &sh),
        original: find(ORIGINAL_KEY).map(|o| read_list(o, &sh)).unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
    fn ts() -> Value { Value::Long(vec![0u8; 8]) }

    /// One button, in the corpus's own key order.
    fn button(id: Value, btn_type: i64, icon: &str, children: Value) -> Value {
        Value::Instance {
            class: Box::new(b("utillib.KeyVal")),
            state: Box::new(Value::Dict(vec![
                (b("btnType"), Value::Int(btn_type)),
                (b("children"), children),
                (b("iconPath"), b(icon)),
                (b("id"), id),
            ])),
        }
    }

    /// char -> ui -> { neocomButtonRawData: (ts, List), neocomButtonRawDataOriginal: (ts, Tuple) }
    fn doc() -> Value {
        Value::Dict(vec![(b("ui"), Value::Dict(vec![
            (b("neocomButtonRawData"), Value::Tuple(vec![ts(), Value::List(vec![
                button(b("chat"), 10, "res:/ui/Texture/WindowIcons/chatchannel.png", Value::None),
                // A folder: children is a one-element list (the only shape the corpus has).
                button(b("inventory"), 4, "res:/UI/Texture/WindowIcons/items.png",
                       Value::List(vec![button(b("InventoryStation"), 4, "res:/UI/Texture/WindowIcons/station.png", Value::None)])),
                // The malformed id 11 corpus buttons carry: Tuple(bytes, None).
                button(Value::Tuple(vec![b("shipTree"), Value::None]), 1, "res:/ui/Texture/WindowIcons/shiptree.png", Value::None),
            ])])),
            (b("neocomButtonRawDataOriginal"), Value::Tuple(vec![ts(), Value::Tuple(vec![
                button(b("chat"), 10, "res:/ui/Texture/WindowIcons/chatchannel.png", Value::None),
                button(b("wallet"), 1, "res:/ui/Texture/WindowIcons/wallet.png", Value::None),
            ])])),
        ]))])
    }

    #[test]
    fn projects_the_bar_in_order_with_indices() {
        let bar = project_neocom(&doc()).unwrap();
        assert_eq!(bar.buttons.len(), 3);
        assert_eq!(bar.buttons.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(),
                   vec!["chat", "inventory", "shipTree"]);
        assert_eq!(bar.buttons.iter().map(|b| b.index).collect::<Vec<_>>(), vec![0, 1, 2]);
    }

    #[test]
    fn reads_btn_type_icon_and_child_count() {
        let bar = project_neocom(&doc()).unwrap();
        assert_eq!(bar.buttons[0].btn_type, 10);
        assert_eq!(bar.buttons[0].icon_path, "res:/ui/Texture/WindowIcons/chatchannel.png");
        assert_eq!(bar.buttons[0].children, 0, "children: None reads as 0");
        assert_eq!(bar.buttons[1].children, 1, "a one-element children list reads as 1");
    }

    #[test]
    fn a_tuple_shaped_id_renders_as_its_bytes_half() {
        // 11 corpus buttons carry id = Tuple(bytes, None). It must not project
        // as a debug string or an empty id — the UI shows it like any other.
        let bar = project_neocom(&doc()).unwrap();
        assert_eq!(bar.buttons[2].id, "shipTree");
    }

    #[test]
    fn projects_the_original_baseline_separately() {
        let bar = project_neocom(&doc()).unwrap();
        assert_eq!(bar.original.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(),
                   vec!["chat", "wallet"]);
    }

    #[test]
    fn a_document_without_the_key_is_an_error_but_a_missing_original_is_not() {
        let empty = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
        assert!(matches!(project_neocom(&empty), Err(NeocomError::NoBar)));
        assert!(matches!(project_neocom(&Value::Dict(vec![])), Err(NeocomError::NoUi)));

        // No Original at all: the bar still projects, with an empty baseline.
        let no_orig = Value::Dict(vec![(b("ui"), Value::Dict(vec![
            (b("neocomButtonRawData"), Value::Tuple(vec![ts(), Value::List(vec![
                button(b("chat"), 10, "icon.png", Value::None),
            ])])),
        ]))]);
        let bar = project_neocom(&no_orig).unwrap();
        assert_eq!(bar.buttons.len(), 1);
        assert!(bar.original.is_empty());
    }

    #[test]
    fn projects_buttons_whose_keys_and_icons_are_interned() {
        // Real files intern the repeated state key names, and reuse an
        // identical icon path across buttons, as `Shared` definitions reached
        // by `Ref` from later buttons (corpus dump: shared[30]:b"iconPath" on
        // the first button, ref[30] on every one after it — the `ui` dict
        // itself is per-character and large, so it is NOT shared as a whole;
        // see hud.rs and treewalk's section_resolves_a_ref_wrapped_section_key
        // for the idiom this actually is). A projection that matches bare
        // `Value::Bytes` instead of resolving through `effective` reads
        // nothing from a file shaped like this.
        fn shared_key(slot: u32, name: &str) -> Value {
            Value::Shared { slot, value: Box::new(b(name)) }
        }
        fn interned_button(id: &str, btn_type: i64, icon: Value) -> Value {
            Value::Instance {
                class: Box::new(b("utillib.KeyVal")),
                state: Box::new(Value::Dict(vec![
                    (shared_key(1, "btnType"), Value::Int(btn_type)),
                    (shared_key(2, "children"), Value::None),
                    (shared_key(3, "iconPath"), icon),
                    (shared_key(4, "id"), b(id)),
                ])),
            }
        }
        fn refd_button(id: &str, btn_type: i64, icon: Value) -> Value {
            Value::Instance {
                class: Box::new(b("utillib.KeyVal")),
                state: Box::new(Value::Dict(vec![
                    (Value::Ref(1), Value::Int(btn_type)),
                    (Value::Ref(2), Value::None),
                    (Value::Ref(3), icon),
                    (Value::Ref(4), b(id)),
                ])),
            }
        }
        let shared_icon = Value::Shared { slot: 5, value: Box::new(b("res:/ui/Texture/WindowIcons/folder.png")) };
        let doc = Value::Dict(vec![(b("ui"), Value::Dict(vec![
            (b("neocomButtonRawData"), Value::Tuple(vec![ts(), Value::List(vec![
                interned_button("chat", 10, shared_icon),
                refd_button("inventory", 4, Value::Ref(5)),
            ])])),
        ]))]);
        let bar = project_neocom(&doc).unwrap();
        assert_eq!(bar.buttons.len(), 2);
        assert_eq!(bar.buttons[0].icon_path, "res:/ui/Texture/WindowIcons/folder.png");
        assert_eq!(bar.buttons[1].id, "inventory");
        assert_eq!(bar.buttons[1].btn_type, 4);
        assert_eq!(bar.buttons[1].icon_path, "res:/ui/Texture/WindowIcons/folder.png",
                   "the second button's icon is a Ref to the first's Shared definition");
    }
}
