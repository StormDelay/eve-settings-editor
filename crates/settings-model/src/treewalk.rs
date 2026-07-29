//! Shared dict-traversal helpers for the typed category projections
//! (windows.rs, overview.rs): find a byte-keyed child dict, unwrap the
//! `(timestamp, dict)` wrappers and `Shared` indirection, all threading the
//! `NodePath` a later mutation targets.

use blue_marshal::Value;

use crate::path::{NodePath, Step};

pub(crate) type Entries = Vec<(Value, Value)>;

/// Shared-object slot table: slot number -> the value it stores. EVE files
/// store a repeated window-id string once as a `Shared` and reference it
/// elsewhere as `Ref(slot)`, so the same window id appears as `Shared` in one
/// dict and `Ref` in another. Resolving them is what makes ids real and unique
/// (an unresolved `Ref` would collapse every reference to the "ref" kind name,
/// producing duplicate ids that crash a keyed render).
pub(crate) type SharedTable<'a> = std::collections::HashMap<u32, &'a Value>;

/// Gather every `Shared { slot, value }` in the tree into a slot table.
pub(crate) fn collect_shared<'a>(v: &'a Value, out: &mut SharedTable<'a>) {
    match v {
        Value::Shared { slot, value } => {
            out.insert(*slot, value);
            collect_shared(value, out);
        }
        Value::Tuple(items) | Value::List(items) => {
            items.iter().for_each(|i| collect_shared(i, out));
        }
        Value::Dict(entries) => entries.iter().for_each(|(k, val)| {
            collect_shared(k, out);
            collect_shared(val, out);
        }),
        Value::Stream(inner) => collect_shared(inner, out),
        Value::Instance { class, state } => {
            collect_shared(class, out);
            collect_shared(state, out);
        }
        Value::Reduce { ctor, items, pairs } => {
            collect_shared(ctor, out);
            items.iter().for_each(|i| collect_shared(i, out));
            pairs.iter().for_each(|(k, val)| {
                collect_shared(k, out);
                collect_shared(val, out);
            });
        }
        _ => {}
    }
}

/// Follow `Ref`/`Shared` indirection to the underlying value (bounded against a
/// pathological chain; real files reference backwards so this terminates fast).
pub(crate) fn effective<'a>(v: &'a Value, shared: &SharedTable<'a>) -> &'a Value {
    let mut cur = v;
    for _ in 0..64 {
        cur = match cur {
            Value::Shared { value, .. } => value,
            Value::Ref(slot) => match shared.get(slot).copied() {
                Some(target) => target,
                None => return cur,
            },
            _ => return cur,
        };
    }
    cur
}

pub(crate) fn is_bytes(v: &Value, name: &[u8]) -> bool {
    matches!(v, Value::Bytes(b) if b.as_slice() == name)
}

/// Does this dict key name `name`, in whichever string shape it was stored?
/// Real overview files key tab fields with `StrTable`, but the read side
/// (`overview.rs`) and the authoring side (`overview_tabs.rs`) had grown one
/// predicate each, and the two disagreed about which of the OTHER shapes count:
/// one took `Bytes` but not `StrUcs2`, the other the reverse. Neither shape
/// occurs on a real file, so the fold is free — this accepts all of them.
pub(crate) fn key_is(k: &Value, name: &str) -> bool {
    match k {
        Value::Str(s) | Value::StrUcs2(s) => s == name,
        Value::Bytes(b) => b.as_slice() == name.as_bytes(),
        Value::StrTable(i) => {
            blue_marshal::string_table::STRING_TABLE.get(*i as usize) == Some(&name)
        }
        _ => false,
    }
}

/// Drop ALL Shared/Ref sharing from a tree in place (inline every Shared to its
/// value, resolve every Ref). Used before a structural list edit so replacing a
/// list can never destroy a Shared definition that a Ref elsewhere still needs.
/// The re-saved file is larger (dedup gone) but valid; EVE re-dedups on logout —
/// and the structural editors follow this with `blue_marshal::reshare` anyway.
///
/// This is `blue_marshal::inline` rather than a local walk because an embedded
/// `Value::Stream` is an independent marshal blob whose slots restart at 1: the
/// local version resolved the whole tree against one flat `SharedTable`, so a
/// stream carrying its own slot 1 would shadow the outer one. Unreachable on
/// real data (no corpus file contains a STREAM opcode), but there is no reason
/// to keep a second, wronger copy of the same pass.
pub(crate) fn inline_all(v: &mut Value) {
    *v = blue_marshal::inline(v);
}

pub(crate) fn unwrap_shared(v: &Value, mut path: NodePath) -> (&Value, NodePath) {
    if let Value::Shared { value, .. } = v {
        path.push(Step::SharedInner);
        return (value, path);
    }
    (v, path)
}

pub(crate) fn unwrap_shared_ref(v: &Value) -> &Value {
    match v {
        Value::Shared { value, .. } => value,
        other => other,
    }
}

/// `parent` must be a dict; find the entry keyed by the byte-string `name` and
/// return its value as a dict, threading the path (unwrapping one `Shared`).
pub(crate) fn child_dict<'a>(parent: &'a Value, name: &[u8], base: NodePath) -> Option<(&'a Entries, NodePath)> {
    let (parent, base) = unwrap_shared(parent, base);
    let Value::Dict(entries) = parent else { return None };
    let (i, (_, v)) = entries.iter().enumerate().find(|(_, (k, _))| is_bytes(k, name))?;
    let mut p = base;
    p.push(Step::DictValue(i));
    let (v, p) = unwrap_shared(v, p);
    match v {
        Value::Dict(d) => Some((d, p)),
        _ => None,
    }
}

/// Find `name` inside `parent` where the value is the `(timestamp, dict)`
/// wrapper (or, defensively, a bare dict or a `Shared` of either). Returns the
/// inner dict and the path to it.
pub(crate) fn timestamped_dict<'a>(
    parent: &'a Entries,
    base: &NodePath,
    name: &[u8],
) -> Option<(&'a Entries, NodePath)> {
    let (i, (_, v)) = parent.iter().enumerate().find(|(_, (k, _))| is_bytes(k, name))?;
    let mut p = base.clone();
    p.push(Step::DictValue(i));
    let (v, p) = unwrap_shared(v, p);
    match v {
        Value::Dict(d) => Some((d, p)),
        Value::Tuple(items) => {
            let (ti, inner) = items.iter().enumerate().find(|(_, e)| matches!(e, Value::Dict(_)))?;
            let Value::Dict(d) = inner else { return None };
            let mut p2 = p;
            p2.push(Step::Tuple(ti));
            Some((d, p2))
        }
        _ => None,
    }
}

// Ref/Shared resolution shared by the projection modules. These lived as
// private copies in overview.rs, autofill.rs and keybinds.rs — three
// byte-identical sets — until the keybindings work would have made it four.

/// Value of the entry whose RESOLVED key is `Bytes(name)`, itself resolved.
pub(crate) fn find_child<'a>(dict: &'a Entries, name: &[u8], sh: &SharedTable<'a>) -> Option<&'a Value> {
    dict.iter()
        .find(|(k, _)| matches!(effective(k, sh), Value::Bytes(b) if b.as_slice() == name))
        .map(|(_, v)| effective(v, sh))
}

/// Resolve to a dict, unwrapping a `(timestamp, dict)` wrapper.
pub(crate) fn as_dict<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<&'a Entries> {
    match effective(v, sh) {
        Value::Dict(d) => Some(d),
        Value::Tuple(items) => items.iter().find_map(|e| match effective(e, sh) {
            Value::Dict(d) => Some(d),
            _ => None,
        }),
        _ => None,
    }
}

/// Resolve to a list, unwrapping a `(timestamp, list)` wrapper.
pub(crate) fn as_list<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<&'a Vec<Value>> {
    match effective(v, sh) {
        Value::List(l) => Some(l),
        Value::Tuple(items) => items.iter().find_map(|e| match effective(e, sh) {
            Value::List(l) => Some(l),
            _ => None,
        }),
        _ => None,
    }
}

pub(crate) fn child_dict_mut<'a>(dict: &'a mut Entries, name: &[u8]) -> Option<&'a mut Entries> {
    let (_, v) = dict.iter_mut().find(|(k, _)| is_bytes(k, name))?;
    dict_inner_mut(v)
}

pub(crate) fn dict_inner_mut(v: &mut Value) -> Option<&mut Entries> {
    match v {
        Value::Dict(d) => Some(d),
        Value::Tuple(items) => items.iter_mut().find_map(|e| match e {
            Value::Dict(d) => Some(d),
            _ => None,
        }),
        _ => None,
    }
}

pub(crate) fn list_inner_mut(v: &mut Value) -> Option<&mut Vec<Value>> {
    match v {
        Value::List(l) => Some(l),
        Value::Tuple(items) => items.iter_mut().find_map(|e| match e {
            Value::List(l) => Some(l),
            _ => None,
        }),
        _ => None,
    }
}

/// Resolve a top-level section of a document root by name.
///
/// Hides three things callers keep getting wrong: the root itself may be
/// `Shared` (and the hop MUST appear in the returned path, or `resolve_mut`
/// fails on it), the section key may be a `Ref`/`Shared` rather than plain
/// `Bytes` (account files store it that way), and the section value may be
/// `Shared` too.
pub(crate) fn section<'a>(
    root: &'a Value,
    name: &[u8],
    shared: &SharedTable<'a>,
) -> Option<(&'a Entries, NodePath)> {
    let (root, base) = unwrap_shared(root, Vec::new());
    let Value::Dict(entries) = effective(root, shared) else { return None };
    let (i, (_, v)) = entries
        .iter()
        .enumerate()
        .find(|(_, (k, _))| is_bytes(effective(k, shared), name))?;
    let mut p = base;
    p.push(Step::DictValue(i));
    let (v, p) = unwrap_shared(v, p);
    match v {
        Value::Dict(d) => Some((d, p)),
        _ => None,
    }
}

/// A value's text, whatever string shape the client stored it in, resolving
/// `Shared`/`Ref` on the way. Absorbed the near-identical `bytes_str` (which
/// took an already-resolved value and missed `StrUcs2`); its two call sites
/// were both `bytes_str(effective(k, &sh))`, so they read the same keys as
/// before plus any stored as UCS2.
pub(crate) fn text<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<String> {
    match effective(v, sh) {
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        Value::Str(s) | Value::StrUcs2(s) => Some(s.clone()),
        _ => None,
    }
}

/// Lowercase hex, for rendering a non-UTF8 key as a stable id.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }

    #[test]
    fn section_finds_a_plain_top_level_dict() {
        let root = Value::Dict(vec![
            (b("windows"), Value::Dict(vec![(b("a"), Value::Int(1))])),
            (b("ui"), Value::Dict(vec![(b("chatchannels"), Value::Int(2))])),
        ]);
        let sh = SharedTable::new();
        let (entries, path) = section(&root, b"ui", &sh).expect("ui section");
        assert_eq!(entries.len(), 1);
        assert_eq!(path, vec![Step::DictValue(1)]);
    }

    #[test]
    fn section_sees_through_a_shared_root_and_records_the_step() {
        // A Shared-wrapped root is what an account file looks like. The old
        // hud.rs copy resolved the VALUE but never pushed Step::SharedInner,
        // so every path it returned was wrong by one hop and resolve_mut
        // failed on it.
        let inner = Value::Dict(vec![(b("tabgroups"), Value::Dict(vec![(b("76_names"), b("Character: Information"))]))]);
        let root = Value::Shared { slot: 1, value: Box::new(inner) };
        let mut sh = SharedTable::new();
        collect_shared(&root, &mut sh);
        let (entries, path) = section(&root, b"tabgroups", &sh).expect("tabgroups section");
        assert_eq!(entries.len(), 1);
        assert_eq!(path.first(), Some(&Step::SharedInner), "the Shared hop must be in the path");
        assert_eq!(path.last(), Some(&Step::DictValue(0)));
    }

    #[test]
    fn section_resolves_a_ref_wrapped_section_key() {
        // Account files store the root section KEY as a Ref — is_bytes alone
        // misses it, which is the gotcha this helper exists to hide.
        let key = Value::Shared { slot: 7, value: Box::new(b("tabgroups")) };
        let root = Value::Dict(vec![
            (key, Value::Dict(vec![(b("76_names"), b("Character: Information"))])),
            (Value::Ref(7), Value::Dict(vec![(b("x"), Value::Int(1))])),
        ]);
        let mut sh = SharedTable::new();
        collect_shared(&root, &mut sh);
        // The SECOND entry's key is a Ref to "tabgroups"; the finder must match
        // the first (a Shared wrapping the same bytes) and stop there.
        let (entries, _) = section(&root, b"tabgroups", &sh).expect("tabgroups section");
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn text_reads_every_string_shape_and_refuses_others() {
        let sh = SharedTable::new();
        assert_eq!(text(&b("Local"), &sh).as_deref(), Some("Local"));
        assert_eq!(text(&Value::Str("Local".into()), &sh).as_deref(), Some("Local"));
        assert_eq!(text(&Value::StrUcs2("Local".into()), &sh).as_deref(), Some("Local"));
        assert_eq!(text(&Value::Int(3), &sh), None);
    }

    #[test]
    fn inline_all_keeps_an_embedded_stream_in_its_own_slot_scope() {
        // An embedded Stream is its own marshal blob: its slot 1 has nothing to
        // do with the outer slot 1. The local walk this helper used to run
        // resolved both against one flat table, so whichever definition was
        // collected last won and the outer Ref came back with the stream's
        // value. Unreachable on real files (no STREAM opcode in the corpus).
        let mut root = Value::Dict(vec![
            (b("a"), Value::Shared { slot: 1, value: Box::new(b("outer")) }),
            (b("b"), Value::Ref(1)),
            (
                b("s"),
                Value::Stream(Box::new(Value::List(vec![
                    Value::Shared { slot: 1, value: Box::new(b("inner")) },
                    Value::Ref(1),
                ]))),
            ),
        ]);
        inline_all(&mut root);
        assert_eq!(
            root,
            Value::Dict(vec![
                (b("a"), b("outer")),
                (b("b"), b("outer")),
                (b("s"), Value::Stream(Box::new(Value::List(vec![b("inner"), b("inner")])))),
            ]),
            "each scope resolves against its own slots"
        );
    }

    #[test]
    fn hex_renders_lowercase_two_digit_bytes() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff]), "000fff");
    }
}
