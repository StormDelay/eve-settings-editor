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
use crate::treewalk::{collect_shared, effective, inline_all, is_bytes, unwrap_shared, Entries, SharedTable};
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
    match locate(entries, &base, f, shared) {
        (Located::Writable(path), text) => (text, SetTarget::Set { path }),
        // Key present but unreadable (wrong wire kind, or a malformed point
        // tuple): refuse to write rather than clobber it or mint a duplicate key.
        (Located::Unwritable, _) => (None, SetTarget::Unavailable),
        // Genuinely absent: `set_hud_value` mints the `(timestamp, value)`
        // leaf. The parent/key here document the target; the op does the
        // insert, because a generic InsertDictEntry cannot build the
        // timestamp wrapper.
        (Located::Absent, _) => (
            None,
            SetTarget::Insert { parent: base, key: crate::mutate::NewValue::BytesHex(hex(f.key)) },
        ),
    }
}

/// What a write to this field may do — the single three-way decision shared by
/// the projection (`probe`) and the setter (`set_hud_value`), so the two can
/// never disagree about whether a key is absent or merely unreadable.
enum Located {
    /// Key present and readable: overwrite the scalar at this path.
    Writable(NodePath),
    /// Key present but not readable as this field's kind (wrong wire kind, or a
    /// malformed point tuple): refuse — overwriting would change its type and
    /// minting would duplicate the key.
    Unwritable,
    /// Key genuinely absent: mint the `(timestamp, value)` leaf.
    Absent,
}

/// Decide what a write to `f` may do. The `Option<String>` alongside `Located`
/// is the scalar text `probe` shows the user (present only for `Writable`) —
/// returned here, rather than re-derived by each caller, so the branch logic
/// that tells "absent" apart from "present but unreadable" (see `leaf`'s doc)
/// lives in exactly one place.
fn locate(entries: &Entries, base: &NodePath, f: &Field, shared: &SharedTable) -> (Located, Option<String>) {
    match leaf(entries, base, f.key, f.elem, shared) {
        Some((v, path)) => match scalar_text(v, f.kind, shared) {
            Some(text) => (Located::Writable(path), Some(text)),
            None => (Located::Unwritable, None),
        },
        // `leaf` returns a bare `None` for two different reasons: the key is
        // genuinely absent, or it's present but unreadable (a malformed point
        // tuple). Only the former is safe to insert: `Entries` is a plain
        // `Vec`, not a deduping map, so inserting on the latter would push a
        // second entry with the same key — reads keep finding the first
        // (malformed) one via `.find()`, silently orphaning every write.
        None if key_present(entries, f.key, shared) => (Located::Unwritable, None),
        None => (Located::Absent, None),
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

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum HudError {
    UnknownField(String),
    /// The file has no such section — nothing to write into. Real character and
    /// account files always have both `windows` and `ui`.
    NoSection,
    /// The key exists but holds an unexpected wire kind; overwriting it would
    /// change its type and minting would duplicate the key.
    NotEditable,
    Parse(String),
}

/// Write one HUD field. An existing key is overwritten in place (no reshare
/// needed — a scalar edit is not structural). An absent key is minted as the
/// `(timestamp, value)` leaf real files use, which needs `inline_all` first per
/// the house rule; the caller (`ops`) reshares afterwards.
///
/// Shares `locate`'s three-way decision with `probe` so the two can never
/// disagree about whether a key is genuinely absent (safe to mint) or merely
/// unreadable (must be refused, not duplicated).
pub fn set_hud_value(root: &mut Value, name: &str, text: &str) -> Result<(), HudError> {
    let f = FIELDS
        .iter()
        .find(|f| f.name == name)
        .ok_or_else(|| HudError::UnknownField(name.to_string()))?;

    // Resolve the decision under an immutable borrow, then mutate.
    let located = {
        let mut shared = SharedTable::new();
        collect_shared(root, &mut shared);
        let (entries, base) = section(root, f.section, &shared).ok_or(HudError::NoSection)?;
        locate(entries, &base, f, &shared).0
    };

    match located {
        Located::Writable(path) => {
            let m = crate::mutate::Mutation::SetScalar { path, text: text.to_string() };
            crate::mutate::apply(root, &m).map_err(|e| HudError::Parse(format!("{e:?}")))
        }
        Located::Unwritable => Err(HudError::NotEditable),
        Located::Absent => mint(root, f, text),
    }
}

/// Insert the absent leaf. After `inline_all` every key is a plain byte-string,
/// so this half needs no `Shared`/`Ref` resolution.
fn mint(root: &mut Value, f: &Field, text: &str) -> Result<(), HudError> {
    let value = build_scalar(f.kind, text)?;
    let leaf_value = match f.elem {
        None => value,
        Some(ix) => {
            // A point field mints the whole (x, y); the untouched axis takes the
            // sibling field's default.
            let sibling = FIELDS
                .iter()
                .find(|o| o.section == f.section && o.key == f.key && o.elem != f.elem)
                .expect("every point field has a sibling axis");
            let other = build_scalar(sibling.kind, sibling.default)?;
            let mut items = vec![Value::None, Value::None];
            items[ix] = value;
            items[sibling.elem.expect("sibling is a point axis")] = other;
            Value::Tuple(items)
        }
    };
    inline_all(root);
    let Value::Dict(entries) = root else { return Err(HudError::NoSection) };
    let (_, section_value) = entries
        .iter_mut()
        .find(|(k, _)| is_bytes(k, f.section))
        .ok_or(HudError::NoSection)?;
    let Value::Dict(section_entries) = section_value else { return Err(HudError::NoSection) };
    section_entries.push((
        Value::Bytes(f.key.to_vec()),
        Value::Tuple(vec![Value::Long(vec![0u8; 8]), leaf_value]),
    ));
    Ok(())
}

fn build_scalar(kind: HudKind, text: &str) -> Result<Value, HudError> {
    let err = || HudError::Parse(format!("{kind:?}: {text:?}"));
    Ok(match kind {
        HudKind::Float => {
            let v: f64 = text.trim().parse().map_err(|_| err())?;
            if !v.is_finite() {
                return Err(err());
            }
            Value::Float(v)
        }
        HudKind::Int => Value::Int(text.trim().parse().map_err(|_| err())?),
        HudKind::Bool => match text {
            "true" | "True" => Value::Bool(true),
            "false" | "False" => Value::Bool(false),
            _ => return Err(err()),
        },
    })
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

    /// An account document: neocomWidth under `windows`, the toggles under `ui`
    /// — and, as in real files, `ui` keyed by a Ref.
    fn user_doc() -> Value {
        Value::Dict(vec![
            (b("windows"), Value::Dict(vec![(b("neocomWidth"), wrapped(Value::Int(37)))])),
            (
                Value::Ref(9),
                Value::Dict(vec![
                    (b("shipuialigntop"), wrapped(Value::Bool(true))),
                    (b("detachFighterUI"), wrapped(Value::Bool(true))),
                    (b("displayFighterUI"), wrapped(Value::Bool(false))),
                ]),
            ),
            (b("anchor"), Value::Shared { slot: 9, value: Box::new(b("ui")) }),
        ])
    }

    #[test]
    fn projects_the_account_side_fields() {
        let cdoc = char_doc();
        let udoc = user_doc();
        let hud = project_hud(&cdoc, Some(&udoc));
        assert_eq!(entry(&hud, "neocom_width").value.as_deref(), Some("37"));
        assert_eq!(entry(&hud, "ship_top").value.as_deref(), Some("true"));
        assert_eq!(entry(&hud, "fighter_detached").value.as_deref(), Some("true"));
        assert_eq!(entry(&hud, "fighter_shown").value.as_deref(), Some("false"));
        assert_eq!(entry(&hud, "neocom_width").scope, HudScope::Account);
        // Paths address the ACCOUNT document, not the character one.
        match &entry(&hud, "neocom_width").set {
            SetTarget::Set { path } => assert_eq!(resolve(&udoc, path), Some(&Value::Int(37))),
            other => panic!("expected Set, got {other:?}"),
        }
    }

    #[test]
    fn without_an_account_file_the_account_fields_are_unavailable() {
        let hud = project_hud(&char_doc(), None);
        for name in ["ship_top", "fighter_detached", "fighter_shown", "neocom_width"] {
            let e = entry(&hud, name);
            assert!(e.value.is_none(), "{name} has no value");
            assert!(matches!(e.set, SetTarget::Unavailable), "{name} is unavailable");
        }
        // The character-side fields are unaffected.
        assert_eq!(entry(&hud, "fighter_x").value.as_deref(), Some("326"));
    }

    #[test]
    fn sets_an_existing_float_and_keeps_its_wire_kind() {
        let mut doc = char_doc();
        set_hud_value(&mut doc, "ship_offset", "-42").expect("set");
        let hud = project_hud(&doc, None);
        assert_eq!(entry(&hud, "ship_offset").value.as_deref(), Some("-42"));
        // Still a Float, not an Int — set_scalar edits in place.
        match &entry(&hud, "ship_offset").set {
            SetTarget::Set { path } => {
                assert!(matches!(resolve(&doc, path), Some(Value::Float(f)) if *f == -42.0));
            }
            other => panic!("expected Set, got {other:?}"),
        }
    }

    #[test]
    fn sets_one_axis_of_a_point_without_disturbing_the_other() {
        let mut doc = char_doc();
        set_hud_value(&mut doc, "fighter_y", "500").expect("set");
        let hud = project_hud(&doc, None);
        assert_eq!(entry(&hud, "fighter_x").value.as_deref(), Some("326"));
        assert_eq!(entry(&hud, "fighter_y").value.as_deref(), Some("500"));
    }

    #[test]
    fn sets_a_bool_in_the_account_document() {
        let mut doc = user_doc();
        set_hud_value(&mut doc, "fighter_shown", "true").expect("set");
        let hud = project_hud(&char_doc(), Some(&doc));
        assert_eq!(entry(&hud, "fighter_shown").value.as_deref(), Some("true"));
    }

    #[test]
    fn mints_an_absent_scalar_with_a_zero_timestamp() {
        // `windows` present but empty — the 69/384 corpus case.
        let mut doc = Value::Dict(vec![(b("windows"), Value::Dict(vec![]))]);
        set_hud_value(&mut doc, "ship_offset", "-100").expect("mint");
        let hud = project_hud(&doc, None);
        assert_eq!(entry(&hud, "ship_offset").value.as_deref(), Some("-100"));
        // The minted leaf is the (timestamp, value) wrapper real files use.
        let Value::Dict(root) = &doc else { panic!("root is a dict") };
        let (_, section) = &root[0];
        let Value::Dict(entries) = section else { panic!("section is a dict") };
        assert_eq!(entries.len(), 1);
        match &entries[0].1 {
            Value::Tuple(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], Value::Long(vec![0u8; 8]));
                assert!(matches!(items[1], Value::Float(f) if f == -100.0));
            }
            other => panic!("expected (ts, value), got {other:?}"),
        }
    }

    #[test]
    fn mints_an_absent_point_with_the_sibling_axis_defaulted() {
        let mut doc = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
        set_hud_value(&mut doc, "fighter_x", "640").expect("mint");
        let hud = project_hud(&doc, None);
        assert_eq!(entry(&hud, "fighter_x").value.as_deref(), Some("640"));
        assert_eq!(entry(&hud, "fighter_y").value.as_deref(), Some("0"));
    }

    #[test]
    fn a_minted_key_is_written_once_not_duplicated() {
        let mut doc = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
        set_hud_value(&mut doc, "fighter_x", "10").expect("mint");
        set_hud_value(&mut doc, "fighter_y", "20").expect("set");
        let Value::Dict(root) = &doc else { panic!() };
        let Value::Dict(entries) = &root[0].1 else { panic!() };
        assert_eq!(entries.len(), 1, "the second write reuses the minted key");
        let hud = project_hud(&doc, None);
        assert_eq!(entry(&hud, "fighter_x").value.as_deref(), Some("10"));
        assert_eq!(entry(&hud, "fighter_y").value.as_deref(), Some("20"));
    }

    #[test]
    fn errors_are_reported_not_papered_over() {
        let mut doc = char_doc();
        assert_eq!(
            set_hud_value(&mut doc, "no_such_field", "1"),
            Err(HudError::UnknownField("no_such_field".to_string()))
        );
        // Section missing entirely.
        let mut bare = Value::Dict(vec![(b("audio"), Value::Dict(vec![]))]);
        assert_eq!(set_hud_value(&mut bare, "ship_offset", "1"), Err(HudError::NoSection));
        // Key present with an unexpected wire kind.
        let mut odd = Value::Dict(vec![(
            b("windows"),
            Value::Dict(vec![(b("shipuialignleftoffset"), wrapped(b("nonsense")))]),
        )]);
        assert_eq!(set_hud_value(&mut odd, "ship_offset", "1"), Err(HudError::NotEditable));
        // Unparseable text.
        assert!(matches!(set_hud_value(&mut doc, "fighter_x", "abc"), Err(HudError::Parse(_))));
    }

    #[test]
    fn a_present_but_malformed_point_tuple_is_refused_not_minted() {
        let mut doc = Value::Dict(vec![(
            b("ui"),
            // One element instead of two — present, but not readable as a point.
            Value::Dict(vec![(b("fightersDetachedPosition"), wrapped(Value::Tuple(vec![Value::Int(1)])))]),
        )]);
        assert_eq!(set_hud_value(&mut doc, "fighter_x", "10"), Err(HudError::NotEditable));
        // Must not have minted a second entry alongside the malformed one.
        let Value::Dict(root) = &doc else { panic!() };
        let Value::Dict(entries) = &root[0].1 else { panic!() };
        assert_eq!(entries.len(), 1, "refusing must not duplicate the key");
    }

    #[test]
    fn all_nine_fields_are_projected_in_a_stable_order() {
        let hud = project_hud(&char_doc(), Some(&user_doc()));
        let names: Vec<&str> = hud.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "ship_offset", "fighter_x", "fighter_y", "badge_x", "badge_y",
                "ship_top", "fighter_detached", "fighter_shown", "neocom_width",
            ]
        );
    }
}
