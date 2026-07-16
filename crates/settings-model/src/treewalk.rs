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
