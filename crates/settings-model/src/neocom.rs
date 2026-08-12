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

use crate::treewalk::{collect_shared, effective, inline_all, is_bytes, section, SharedTable};

const BAR_KEY: &[u8] = b"neocomButtonRawData";
const ORIGINAL_KEY: &[u8] = b"neocomButtonRawDataOriginal";

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

/// The live bar's `List` payload, mutable. The document is already inlined, so
/// no `Shared` wrapper survives to resolve.
fn bar_list_mut(v: &mut Value) -> Result<&mut Vec<Value>, NeocomError> {
    let Value::Dict(top) = v else { return Err(NeocomError::NoUi) };
    let (_, ui) = top.iter_mut().find(|(k, _)| is_bytes(k, b"ui")).ok_or(NeocomError::NoUi)?;
    let Value::Dict(entries) = ui else { return Err(NeocomError::NoUi) };
    let (_, raw) = entries.iter_mut().find(|(k, _)| is_bytes(k, BAR_KEY)).ok_or(NeocomError::NoBar)?;
    // (timestamp, payload) on every real file; tolerate a bare payload too.
    // (The length check is split out of the match so the arm below is an
    // unguarded reborrow — a guarded arm here does not borrow-check when
    // paired with a move-catch-all on a `&mut` scrutinee.)
    let is_pair = matches!(raw, Value::Tuple(t) if t.len() == 2);
    let payload = if is_pair {
        let Value::Tuple(t) = raw else { unreachable!() };
        &mut t[1]
    } else {
        raw
    };
    // The live bar is a List on every corpus file, and `reset` below writes one
    // whatever Original was stored as — so a Tuple-stored bar is a shape nothing
    // produces. Refuse it rather than rewriting it into a List: a silent reshape
    // of a file we do not understand is worse than an error.
    match payload {
        Value::List(l) => Ok(l),
        _ => Err(NeocomError::NoBar),
    }
}

pub fn reorder(v: &mut Value, order: &[usize]) -> Result<(), NeocomError> {
    // Validate BEFORE inlining, so a rejected reorder leaves the document
    // byte-for-byte as it was (the tests assert exactly this).
    let n = project_neocom(v)?.buttons.len();
    if order.len() != n {
        return Err(NeocomError::BadOrder);
    }
    {
        let mut seen = vec![false; n];
        for &i in order {
            let slot = seen.get_mut(i).ok_or(NeocomError::BadOrder)?;
            if *slot {
                return Err(NeocomError::BadOrder); // a repeat
            }
            *slot = true;
        }
    }
    inline_all(v);
    let list = bar_list_mut(v)?;
    // `read_list` filter_maps away any non-Instance entry, so the projected
    // count and the raw list length can desync — a single junk entry would
    // otherwise let `order` (validated against the projected count) index the
    // raw list wrong, silently truncating or misplacing a button. Refuse
    // instead: unreachable on today's corpus (43,430/43,430 real buttons are
    // Instances), but the failure mode without this is silent, not loud.
    if list.len() != n {
        return Err(NeocomError::BadOrder);
    }
    // Move whole instances: take them out, then put them back in the new order.
    let taken: Vec<Value> = std::mem::take(list);
    *list = order.iter().map(|&i| taken[i].clone()).collect();
    Ok(())
}

pub fn remove(v: &mut Value, index: usize) -> Result<(), NeocomError> {
    let n = project_neocom(v)?.buttons.len();
    if index >= n {
        return Err(NeocomError::BadIndex);
    }
    inline_all(v);
    let list = bar_list_mut(v)?;
    // Same desync guard as `reorder`: a non-Instance entry in the raw list
    // would make `n` (the projected count `index` was validated against) not
    // match the raw list this indexes into.
    if list.len() != n {
        return Err(NeocomError::BadIndex);
    }
    list.remove(index);
    Ok(())
}

pub fn add(v: &mut Value, id: &str, btn_type: i64, icon_path: &str) -> Result<(), NeocomError> {
    inline_all(v);
    let list = bar_list_mut(v)?;
    // The exact corpus shape: utillib.KeyVal, four keys, this order (spec §2).
    list.push(Value::Instance {
        class: Box::new(Value::Bytes(b"utillib.KeyVal".to_vec())),
        state: Box::new(Value::Dict(vec![
            (Value::Bytes(b"btnType".to_vec()), Value::Int(btn_type)),
            (Value::Bytes(b"children".to_vec()), Value::None),
            (Value::Bytes(b"iconPath".to_vec()), Value::Bytes(icon_path.as_bytes().to_vec())),
            (Value::Bytes(b"id".to_vec()), Value::Bytes(id.as_bytes().to_vec())),
        ])),
    });
    Ok(())
}

pub fn reset(v: &mut Value) -> Result<(), NeocomError> {
    if project_neocom(v)?.original.is_empty() {
        return Err(NeocomError::NoOriginal);
    }
    inline_all(v);
    // Read the (now inlined) Original, then write it over the live bar. Original
    // itself is never modified — it is the character's own client baseline.
    let original: Vec<Value> = {
        let Value::Dict(top) = &*v else { return Err(NeocomError::NoUi) };
        let (_, ui) = top.iter().find(|(k, _)| is_bytes(k, b"ui")).ok_or(NeocomError::NoUi)?;
        let Value::Dict(entries) = ui else { return Err(NeocomError::NoUi) };
        let (_, orig) = entries.iter().find(|(k, _)| is_bytes(k, ORIGINAL_KEY)).ok_or(NeocomError::NoOriginal)?;
        let payload = match orig {
            Value::Tuple(t) if t.len() == 2 => &t[1],
            other => other,
        };
        match payload {
            Value::List(l) | Value::Tuple(l) => l.clone(),
            _ => return Err(NeocomError::NoOriginal),
        }
    };
    let list = bar_list_mut(v)?;
    *list = original; // a List, whatever Original was stored as
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{b, ts};
    use blue_marshal::Value;


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

    fn ids(v: &Value) -> Vec<String> {
        project_neocom(v).unwrap().buttons.into_iter().map(|b| b.id).collect()
    }

    #[test]
    fn reorder_rewrites_the_bar_in_the_given_order() {
        let mut v = doc();
        reorder(&mut v, &[2, 0, 1]).unwrap();
        assert_eq!(ids(&v), vec!["shipTree", "chat", "inventory"]);
    }

    #[test]
    fn reorder_moves_whole_instances_so_children_survive() {
        let mut v = doc();
        reorder(&mut v, &[1, 0, 2]).unwrap();
        let bar = project_neocom(&v).unwrap();
        assert_eq!(bar.buttons[0].id, "inventory");
        assert_eq!(bar.buttons[0].children, 1, "the folder kept its child");
        assert_eq!(bar.buttons[2].id, "shipTree", "the Tuple-shaped id survived the move");
    }

    #[test]
    fn reorder_rejects_anything_that_is_not_a_permutation() {
        for bad in [vec![0, 1], vec![0, 1, 3], vec![0, 0, 1], vec![0, 1, 2, 2]] {
            let mut v = doc();
            assert!(matches!(reorder(&mut v, &bad), Err(NeocomError::BadOrder)), "accepted {bad:?}");
            assert_eq!(ids(&v), vec!["chat", "inventory", "shipTree"], "a rejected reorder changed the bar");
        }
    }

    #[test]
    fn remove_drops_that_button_and_reindexes() {
        let mut v = doc();
        remove(&mut v, 1).unwrap();
        assert_eq!(ids(&v), vec!["chat", "shipTree"]);
        assert_eq!(project_neocom(&v).unwrap().buttons[1].index, 1);
    }

    #[test]
    fn remove_rejects_an_index_that_does_not_exist() {
        let mut v = doc();
        assert!(matches!(remove(&mut v, 3), Err(NeocomError::BadIndex)));
        assert_eq!(ids(&v), vec!["chat", "inventory", "shipTree"]);
    }

    /// A raw bar list with a non-Instance entry mixed in: `read_list`'s
    /// `filter_map` drops it from the projection, so the projected count (2)
    /// no longer matches the raw list length (3).
    fn doc_with_a_junk_entry() -> Value {
        Value::Dict(vec![(b("ui"), Value::Dict(vec![
            (b("neocomButtonRawData"), Value::Tuple(vec![ts(), Value::List(vec![
                button(b("chat"), 10, "icon.png", Value::None),
                b("not-a-button"), // not an Instance: filtered out of the projection
                button(b("wallet"), 1, "wallet.png", Value::None),
            ])])),
        ]))])
    }

    #[test]
    fn reorder_refuses_rather_than_scramble_when_the_raw_list_has_a_non_instance_entry() {
        let mut v = doc_with_a_junk_entry();
        assert!(matches!(reorder(&mut v, &[1, 0]), Err(NeocomError::BadOrder)));
        assert_eq!(ids(&v), vec!["chat", "wallet"], "a refused reorder changed nothing");
    }

    #[test]
    fn remove_refuses_rather_than_drop_the_wrong_button_when_the_raw_list_has_a_non_instance_entry() {
        let mut v = doc_with_a_junk_entry();
        assert!(matches!(remove(&mut v, 0), Err(NeocomError::BadIndex)));
        assert_eq!(ids(&v), vec!["chat", "wallet"], "a refused remove changed nothing");
    }

    #[test]
    fn add_appends_a_keyval_with_the_four_keys_in_order() {
        let mut v = doc();
        add(&mut v, "wallet", 1, "res:/ui/Texture/WindowIcons/wallet.png").unwrap();
        assert_eq!(ids(&v), vec!["chat", "inventory", "shipTree", "wallet"]);

        // The authored instance must match the corpus shape exactly: class
        // utillib.KeyVal, and the four keys in the corpus's own order.
        let bar = project_neocom(&v).unwrap();
        assert_eq!(bar.buttons[3].btn_type, 1);
        assert_eq!(bar.buttons[3].icon_path, "res:/ui/Texture/WindowIcons/wallet.png");
        assert_eq!(bar.buttons[3].children, 0);

        let Value::Dict(top) = &v else { panic!() };
        let (_, ui) = top.iter().find(|(k, _)| matches!(k, Value::Bytes(x) if x == b"ui")).unwrap();
        let Value::Dict(uid) = ui else { panic!() };
        let (_, raw) = uid.iter().find(|(k, _)| matches!(k, Value::Bytes(x) if x == b"neocomButtonRawData")).unwrap();
        let Value::Tuple(t) = raw else { panic!() };
        let Value::List(l) = &t[1] else { panic!() };
        let Value::Instance { class, state } = &l[3] else { panic!("added entry is not an instance") };
        assert_eq!(**class, b("utillib.KeyVal"));
        let Value::Dict(st) = &**state else { panic!() };
        let keys: Vec<String> = st.iter().map(|(k, _)| match k {
            Value::Bytes(x) => String::from_utf8_lossy(x).into_owned(),
            _ => String::new(),
        }).collect();
        assert_eq!(keys, vec!["btnType", "children", "iconPath", "id"]);
        assert_eq!(st[1].1, Value::None, "children is authored as None");
    }

    #[test]
    fn reset_replaces_the_bar_with_the_original_as_a_list() {
        let mut v = doc();
        reset(&mut v).unwrap();
        assert_eq!(ids(&v), vec!["chat", "wallet"]);

        // Original is STORED in a Tuple; the live bar must be a List.
        let Value::Dict(top) = &v else { panic!() };
        let (_, ui) = top.iter().find(|(k, _)| matches!(k, Value::Bytes(x) if x == b"ui")).unwrap();
        let Value::Dict(uid) = ui else { panic!() };
        let (_, raw) = uid.iter().find(|(k, _)| matches!(k, Value::Bytes(x) if x == b"neocomButtonRawData")).unwrap();
        let Value::Tuple(t) = raw else { panic!() };
        assert!(matches!(&t[1], Value::List(_)), "the live bar must stay a List");
    }

    #[test]
    fn reset_without_an_original_errors_and_changes_nothing() {
        let mut v = Value::Dict(vec![(b("ui"), Value::Dict(vec![
            (b("neocomButtonRawData"), Value::Tuple(vec![ts(), Value::List(vec![
                button(b("chat"), 10, "icon.png", Value::None),
            ])])),
        ]))]);
        assert!(matches!(reset(&mut v), Err(NeocomError::NoOriginal)));
        assert_eq!(ids(&v), vec!["chat"]);
    }

    #[test]
    fn every_command_leaves_a_tree_that_still_encodes() {
        // The commands inline first (dropping Shared/Ref); the result must
        // still round-trip, the way stacks.rs proves for its own edits.
        let mut v = doc();
        reorder(&mut v, &[2, 1, 0]).unwrap();
        remove(&mut v, 0).unwrap();
        add(&mut v, "wallet", 1, "wallet.png").unwrap();
        let bytes = blue_marshal::encode(&v).expect("edited tree still encodes");
        assert_eq!(blue_marshal::decode(&bytes).unwrap(), v);
    }
}
