//! Read-only projection of EVE's screen furniture — the ship HUD's horizontal
//! offset, the detached fighter UI and notification badge positions, the neocom
//! width, and the account-level HUD toggles. Every writable field carries the
//! resolved `NodePath` a `set_scalar` mutation targets. All format knowledge
//! (which section, which key, which tuple element, the `(timestamp, value)`
//! wrapper) lives here. The setter is `set_hud_value`; nothing else mutates.
//!
//! Values span two files: the anchors are per character, the toggles and the
//! neocom width are per account. See docs/format-notes.md, "HUD anchors".

use blue_marshal::Value;
use serde::Serialize;

use crate::path::{NodePath, Step};
use crate::treewalk::{collect_shared, effective, is_bytes, unwrap_shared, Entries, SharedTable};
use crate::windows::SetTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HudScope {
    Char,
    Account,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HudKind {
    Float,
    Int,
    Bool,
}

#[derive(Debug, Serialize)]
pub struct HudEntry {
    pub name: String,
    pub kind: HudKind,
    /// `None` when the key is absent or holds an unexpected wire kind — the UI
    /// then shows `default`.
    pub value: Option<String>,
    pub default: String,
    pub scope: HudScope,
    pub set: SetTarget,
}

#[derive(Debug, Serialize)]
pub struct Hud {
    pub entries: Vec<HudEntry>,
}

/// One editable value. `elem` indexes into an `(x, y)` tuple; `None` means the
/// leaf itself. Defaults are EVE's built-in behaviour when the key is absent
/// (assumed, confirmed in the slice's live smoke).
struct Field {
    name: &'static str,
    section: &'static [u8],
    key: &'static [u8],
    elem: Option<usize>,
    kind: HudKind,
    default: &'static str,
    scope: HudScope,
}

const FIELDS: [Field; 9] = [
    Field { name: "ship_offset", section: b"windows", key: b"shipuialignleftoffset",
            elem: None, kind: HudKind::Float, default: "0", scope: HudScope::Char },
    Field { name: "fighter_x", section: b"ui", key: b"fightersDetachedPosition",
            elem: Some(0), kind: HudKind::Int, default: "0", scope: HudScope::Char },
    Field { name: "fighter_y", section: b"ui", key: b"fightersDetachedPosition",
            elem: Some(1), kind: HudKind::Int, default: "0", scope: HudScope::Char },
    Field { name: "badge_x", section: b"ui", key: b"notification_badge_offset",
            elem: Some(0), kind: HudKind::Int, default: "0", scope: HudScope::Char },
    Field { name: "badge_y", section: b"ui", key: b"notification_badge_offset",
            elem: Some(1), kind: HudKind::Int, default: "0", scope: HudScope::Char },
    Field { name: "ship_top", section: b"ui", key: b"shipuialigntop",
            elem: None, kind: HudKind::Bool, default: "false", scope: HudScope::Account },
    Field { name: "fighter_detached", section: b"ui", key: b"detachFighterUI",
            elem: None, kind: HudKind::Bool, default: "false", scope: HudScope::Account },
    Field { name: "fighter_shown", section: b"ui", key: b"displayFighterUI",
            elem: None, kind: HudKind::Bool, default: "false", scope: HudScope::Account },
    Field { name: "neocom_width", section: b"windows", key: b"neocomWidth",
            elem: None, kind: HudKind::Int, default: "37", scope: HudScope::Account },
];

pub fn project_hud(char_root: &Value, user_root: Option<&Value>) -> Hud {
    let mut char_shared = SharedTable::new();
    collect_shared(char_root, &mut char_shared);
    let mut user_shared = SharedTable::new();
    if let Some(u) = user_root {
        collect_shared(u, &mut user_shared);
    }

    let entries = FIELDS
        .iter()
        .map(|f| {
            let (root, shared) = match f.scope {
                HudScope::Char => (Some(char_root), &char_shared),
                HudScope::Account => (user_root, &user_shared),
            };
            // No account file open is normal (an unpaired character): the four
            // account fields are then simply not writable.
            let (value, set) = root.map_or((None, SetTarget::Unavailable), |r| probe(r, f, shared));
            HudEntry {
                name: f.name.to_string(),
                kind: f.kind,
                value,
                default: f.default.to_string(),
                scope: f.scope,
                set,
            }
        })
        .collect();
    Hud { entries }
}

fn probe(root: &Value, f: &Field, shared: &SharedTable) -> (Option<String>, SetTarget) {
    let Some((entries, base)) = section(root, f.section, shared) else {
        // The whole section is missing (or unaddressable) — nothing to write to.
        return (None, SetTarget::Unavailable);
    };
    match leaf(entries, &base, f.key, f.elem, shared) {
        Some((v, path)) => match scalar_text(v, f.kind, shared) {
            Some(text) => (Some(text), SetTarget::Set { path }),
            // Key present but the wire kind is not what this field expects:
            // refuse to write rather than clobber it or mint a duplicate key.
            None => (None, SetTarget::Unavailable),
        },
        // `leaf` returns a bare `None` for two different reasons: the key is
        // genuinely absent, or it's present but unreadable (a malformed point
        // tuple). Only the former is safe to insert: `Entries` is a plain
        // `Vec`, not a deduping map, so inserting on the latter would push a
        // second entry with the same key — reads keep finding the first
        // (malformed) one via `.find()`, silently orphaning every write.
        None if key_present(entries, f.key, shared) => (None, SetTarget::Unavailable),
        // Genuinely absent: `set_hud_value` mints the `(timestamp, value)`
        // leaf. The parent/key here document the target; the op does the
        // insert, because a generic InsertDictEntry cannot build the
        // timestamp wrapper.
        None => (
            None,
            SetTarget::Insert { parent: base, key: crate::mutate::NewValue::BytesHex(hex(f.key)) },
        ),
    }
}

/// Find a root section by name. Section KEYS are resolved through `Ref`/`Shared`:
/// real account files store the `ui` section under a `Ref` to a byte-string
/// defined later in the stream (the trailing shared-object table makes that
/// legal), which `treewalk::child_dict`'s bare `is_bytes` comparison misses.
/// A section whose VALUE is a `Ref` is reported missing — there is no path step
/// into a ref, so it could be read but never written.
fn section<'a>(root: &'a Value, name: &[u8], shared: &SharedTable<'a>) -> Option<(&'a Entries, NodePath)> {
    let Value::Dict(entries) = effective(root, shared) else { return None };
    let (i, (_, v)) = entries.iter().enumerate().find(|(_, (k, _))| is_bytes(effective(k, shared), name))?;
    let (v, p) = unwrap_shared(v, vec![Step::DictValue(i)]);
    match v {
        Value::Dict(d) => Some((d, p)),
        _ => None,
    }
}

/// Locate `key` in a section and step through the `(timestamp, value)` wrapper
/// (and then into tuple element `elem`, for a point field), returning the scalar
/// and its path. Keys are resolved through `Ref`/`Shared` as in `section`.
fn leaf<'a>(
    entries: &'a Entries,
    base: &NodePath,
    key: &[u8],
    elem: Option<usize>,
    shared: &SharedTable<'a>,
) -> Option<(&'a Value, NodePath)> {
    let (i, (_, v)) = entries.iter().enumerate().find(|(_, (k, _))| is_bytes(effective(k, shared), key))?;
    let mut p = base.clone();
    p.push(Step::DictValue(i));
    let (v, p) = unwrap_shared(v, p);
    // (timestamp, value): take element 1. A bare value is tolerated the way
    // treewalk::timestamped_dict tolerates a bare dict.
    let (v, p) = match v {
        Value::Tuple(items) if items.len() == 2 => {
            let mut q = p;
            q.push(Step::Tuple(1));
            (&items[1], q)
        }
        other => (other, p),
    };
    let Some(ix) = elem else { return Some((v, p)) };
    let (v, p) = unwrap_shared(v, p);
    let Value::Tuple(items) = v else { return None };
    if items.len() != 2 {
        return None; // not an (x, y) point
    }
    let mut q = p;
    q.push(Step::Tuple(ix));
    Some((items.get(ix)?, q))
}

/// Whether `key` exists in `entries` at all, resolved through `Ref`/`Shared`
/// exactly as `leaf` does. Used to tell "genuinely absent" (safe to insert)
/// apart from "present but unreadable" (must not insert — see `probe`).
fn key_present(entries: &Entries, key: &[u8], shared: &SharedTable) -> bool {
    entries.iter().any(|(k, _)| is_bytes(effective(k, shared), key))
}

/// The stored value as the text the UI edits, or `None` if the wire kind is not
/// what this field expects. A float is rendered without a trailing `.0` so the
/// panel's number input shows `-189`, not `-189.0`; `set_scalar` keeps the wire
/// kind on write either way.
fn scalar_text(v: &Value, kind: HudKind, shared: &SharedTable) -> Option<String> {
    match (kind, effective(v, shared)) {
        // `format!` prints -189.0 as "-189", which is what the number input wants;
        // set_scalar keeps the leaf's Float wire kind on the way back in.
        (HudKind::Float, Value::Float(f)) => Some(format!("{f}")),
        (HudKind::Int, Value::Int(i)) => Some(i.to_string()),
        (HudKind::Bool, Value::Bool(b)) => Some(b.to_string()),
        _ => None,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::resolve;
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

    fn point(x: i64, y: i64) -> Value {
        Value::Tuple(vec![Value::Int(x), Value::Int(y)])
    }

    /// A character document with every character-scoped HUD key present.
    fn char_doc() -> Value {
        Value::Dict(vec![
            (
                b("windows"),
                Value::Dict(vec![(b("shipuialignleftoffset"), wrapped(Value::Float(-189.0)))]),
            ),
            (
                b("ui"),
                Value::Dict(vec![
                    (b("fightersDetachedPosition"), wrapped(point(326, 54))),
                    (b("notification_badge_offset"), wrapped(point(2519, 131))),
                ]),
            ),
        ])
    }

    fn entry<'a>(hud: &'a Hud, name: &str) -> &'a HudEntry {
        hud.entries.iter().find(|e| e.name == name).expect("entry present")
    }

    #[test]
    fn projects_the_ship_offset_with_a_resolvable_path() {
        let doc = char_doc();
        let hud = project_hud(&doc, None);
        let e = entry(&hud, "ship_offset");
        assert_eq!(e.value.as_deref(), Some("-189"));
        assert_eq!(e.kind, HudKind::Float);
        assert_eq!(e.scope, HudScope::Char);
        match &e.set {
            SetTarget::Set { path } => assert_eq!(resolve(&doc, path), Some(&Value::Float(-189.0))),
            other => panic!("expected Set, got {other:?}"),
        }
    }

    #[test]
    fn projects_each_point_axis_to_its_own_tuple_element() {
        let doc = char_doc();
        let hud = project_hud(&doc, None);
        assert_eq!(entry(&hud, "fighter_x").value.as_deref(), Some("326"));
        assert_eq!(entry(&hud, "fighter_y").value.as_deref(), Some("54"));
        assert_eq!(entry(&hud, "badge_x").value.as_deref(), Some("2519"));
        match &entry(&hud, "fighter_y").set {
            SetTarget::Set { path } => assert_eq!(resolve(&doc, path), Some(&Value::Int(54))),
            other => panic!("expected Set, got {other:?}"),
        }
    }

    /// The real-file case that a bare `is_bytes` lookup misses: the root section
    /// key is a Ref whose byte-string definition lives elsewhere in the tree.
    #[test]
    fn a_ref_keyed_section_still_resolves() {
        let doc = Value::Dict(vec![
            (
                Value::Ref(7),
                Value::Dict(vec![(b("fightersDetachedPosition"), wrapped(point(10, 20)))]),
            ),
            // The Shared definition of b"ui", stored later in the stream.
            (b("elsewhere"), Value::Shared { slot: 7, value: Box::new(b("ui")) }),
        ]);
        let hud = project_hud(&doc, None);
        assert_eq!(entry(&hud, "fighter_x").value.as_deref(), Some("10"));
    }

    #[test]
    fn an_absent_key_reports_the_default_and_an_insert_target() {
        // `windows` exists but holds no HUD key at all.
        let doc = Value::Dict(vec![(b("windows"), Value::Dict(vec![]))]);
        let hud = project_hud(&doc, None);
        let e = entry(&hud, "ship_offset");
        assert!(e.value.is_none());
        assert_eq!(e.default, "0");
        assert!(matches!(e.set, SetTarget::Insert { .. }));
    }

    #[test]
    fn a_missing_section_is_unavailable() {
        let doc = Value::Dict(vec![(b("audio"), Value::Dict(vec![]))]);
        let hud = project_hud(&doc, None);
        assert!(matches!(entry(&hud, "ship_offset").set, SetTarget::Unavailable));
        assert!(entry(&hud, "ship_offset").value.is_none());
    }

    #[test]
    fn a_malformed_point_tuple_is_unavailable_not_insertable() {
        let doc = Value::Dict(vec![(
            b("ui"),
            // One element instead of two.
            Value::Dict(vec![(b("fightersDetachedPosition"), wrapped(Value::Tuple(vec![Value::Int(1)])))]),
        )]);
        let hud = project_hud(&doc, None);
        let e = entry(&hud, "fighter_y");
        assert!(e.value.is_none());
        // Present but unreadable: must NOT report Insert — the key already
        // exists, so inserting would push a duplicate `.find()` never sees.
        assert!(matches!(e.set, SetTarget::Unavailable));
    }

    #[test]
    fn a_present_key_with_the_wrong_wire_kind_is_unavailable_not_insertable() {
        let doc = Value::Dict(vec![(
            b("windows"),
            // ship_offset expects a Float; this file has a Bool there instead.
            Value::Dict(vec![(b("shipuialignleftoffset"), wrapped(Value::Bool(true)))]),
        )]);
        let hud = project_hud(&doc, None);
        let e = entry(&hud, "ship_offset");
        assert!(e.value.is_none());
        assert!(matches!(e.set, SetTarget::Unavailable));
    }

    #[test]
    fn a_shared_leaf_value_is_read_through() {
        let doc = Value::Dict(vec![(
            b("windows"),
            Value::Dict(vec![(
                b("shipuialignleftoffset"),
                Value::Shared { slot: 3, value: Box::new(wrapped(Value::Float(-12.0))) },
            )]),
        )]);
        let hud = project_hud(&doc, None);
        assert_eq!(entry(&hud, "ship_offset").value.as_deref(), Some("-12"));
    }
}
