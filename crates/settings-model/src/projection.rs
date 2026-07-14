//! One-shot projection of a `Value` tree into a JSON-serializable node tree
//! for the raw editor. Rendering conventions match `blue_marshal::dump_text`
//! where both exist, so dumps and the UI read the same way.

use blue_marshal::{string_table::STRING_TABLE, Value};
use serde::Serialize;

use crate::mutate::subtree_contains_shared;
use crate::path::{NodePath, Step};

#[derive(Debug, Serialize)]
pub struct Node {
    pub label: Option<String>,
    pub kind: &'static str,
    pub display: String,
    pub path: NodePath,
    pub editable: bool,
    pub edit_text: Option<String>,
    pub removable: bool,
    pub in_shared: bool,
    pub children: Vec<Node>,
}

pub fn project(root: &Value) -> Node {
    build(root, None, Vec::new(), false, false)
}

fn build(
    v: &Value,
    label: Option<String>,
    path: NodePath,
    removable: bool,
    in_shared: bool,
) -> Node {
    let kind = crate::projection_kind(v);
    let editable = match v {
        // Non-finite floats have no text form that set_scalar can round-trip
        // without rewriting the payload's NaN bits — shown read-only.
        Value::Float(f) => f.is_finite(),
        Value::Bool(_)
        | Value::Int(_)
        | Value::Long(_)
        | Value::Bytes(_)
        | Value::Str(_)
        | Value::StrUcs2(_)
        | Value::StrTable(_) => true,
        _ => false,
    };
    let mut children = Vec::new();
    let child = |v: &Value, label: Option<String>, step: Step, removable: bool| {
        let mut p = path.clone();
        p.push(step);
        build(v, label, p, removable, in_shared)
    };
    match v {
        Value::Tuple(items) => {
            for (i, item) in items.iter().enumerate() {
                let removable = !subtree_contains_shared(item);
                children.push(child(item, Some(format!("[{i}]")), Step::Tuple(i), removable));
            }
        }
        Value::List(items) => {
            for (i, item) in items.iter().enumerate() {
                let removable = !subtree_contains_shared(item);
                children.push(child(item, Some(format!("[{i}]")), Step::List(i), removable));
            }
        }
        Value::Dict(entries) => {
            for (i, (key, value)) in entries.iter().enumerate() {
                let removable =
                    !subtree_contains_shared(key) && !subtree_contains_shared(value);
                children.push(child(
                    value,
                    Some(compact_display(key, 2)),
                    Step::DictValue(i),
                    removable,
                ));
            }
        }
        Value::Instance { class, state } => {
            children.push(child(class, Some("class".into()), Step::InstanceClass, false));
            children.push(child(state, Some("state".into()), Step::InstanceState, false));
        }
        Value::Reduce { ctor, items, pairs } => {
            children.push(child(ctor, Some("ctor".into()), Step::ReduceCtor, false));
            for (i, item) in items.iter().enumerate() {
                children.push(child(item, Some(format!("item[{i}]")), Step::ReduceItem(i), false));
            }
            for (i, (k, val)) in pairs.iter().enumerate() {
                children.push(child(k, Some(format!("pair[{i}].key")), Step::ReducePairKey(i), false));
                children.push(child(val, Some(format!("pair[{i}].value")), Step::ReducePairValue(i), false));
            }
        }
        Value::Shared { value, .. } => {
            let mut p = path.clone();
            p.push(Step::SharedInner);
            children.push(build(value, None, p, false, true));
        }
        Value::Stream(inner) => {
            let mut p = path.clone();
            p.push(Step::StreamInner);
            children.push(build(inner, None, p, false, in_shared));
        }
        _ => {}
    }
    Node {
        label,
        kind,
        display: node_display(v),
        path,
        editable,
        edit_text: edit_text(v),
        removable,
        in_shared,
        children,
    }
}

/// Raw text seeding an inline edit — chosen so that echoing it back through
/// `mutate::set_scalar` unchanged reproduces the same value. `None` for
/// non-editable kinds.
fn edit_text(v: &Value) -> Option<String> {
    Some(match v {
        Value::Bool(true) => "true".into(),
        Value::Bool(false) => "false".into(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) if f.is_finite() => format!("{f:?}"),
        Value::Str(s) | Value::StrUcs2(s) => s.clone(),
        Value::Bytes(b) => {
            // Printable bytes edit as plain text — EXCEPT text that itself
            // starts with "hex:", which must round-trip through the hex form
            // or set_scalar would reinterpret it.
            if !b.is_empty()
                && b.iter().all(|c| (0x20..0x7F).contains(c))
                && !b.starts_with(b"hex:")
            {
                String::from_utf8_lossy(b).into_owned()
            } else {
                format!("hex:{}", hex(b))
            }
        }
        Value::Long(b) => format!("hex:{}", hex(b)),
        Value::StrTable(i) => i.to_string(),
        _ => return None,
    })
}

/// Scalar rendering (and container summaries) for a node's own line.
fn node_display(v: &Value) -> String {
    match v {
        Value::None => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Int(i) => i.to_string(),
        Value::Long(bytes) => format!("hex:{}", hex(bytes)),
        Value::Float(f) => format!("{f:?}"),
        Value::Bytes(b) => bytes_display(b),
        Value::Str(s) => format!("{s:?}"),
        Value::StrUcs2(s) => format!("u{s:?}"),
        Value::StrTable(i) => format!("t{i}:{:?}", STRING_TABLE[*i as usize]),
        Value::Global(name) => format!("global:{}", bytes_display(name)),
        Value::Ref(n) => format!("ref[{n}]"),
        Value::Tuple(items) => format!("tuple ({})", items.len()),
        Value::List(items) => format!("list ({})", items.len()),
        Value::Dict(entries) => format!("dict ({})", entries.len()),
        Value::Stream(_) => "stream".into(),
        Value::Instance { .. } => "instance".into(),
        Value::Reduce { .. } => "reduce".into(),
        Value::Shared { slot, .. } => format!("shared[{slot}]"),
    }
}

/// One-line rendering for dict-key labels; containers render inline to
/// `depth` levels (tuple keys like ("overviewScroll2", 1) are real keys).
fn compact_display(v: &Value, depth: usize) -> String {
    match v {
        Value::Tuple(items) | Value::List(items) if depth > 0 => {
            let inner: Vec<String> =
                items.iter().map(|i| compact_display(i, depth - 1)).collect();
            let (open, close) = if matches!(v, Value::Tuple(_)) { ("(", ")") } else { ("[", "]") };
            format!("{open}{}{close}", inner.join(", "))
        }
        other => node_display(other),
    }
}

fn bytes_display(b: &[u8]) -> String {
    if b.iter().all(|c| (0x20..0x7F).contains(c)) {
        let mut out = String::from("b\"");
        for &c in b {
            if c == b'"' || c == b'\\' {
                out.push('\\');
            }
            out.push(c as char);
        }
        out.push('"');
        out
    } else {
        format!("hex:{}", hex(b))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_dict_with_labels_paths_and_flags() {
        let v = Value::Dict(vec![
            (Value::Bytes(b"geom".to_vec()), Value::Tuple(vec![Value::Int(5)])),
            (
                Value::Tuple(vec![Value::Bytes(b"overviewScroll2".to_vec()), Value::Int(1)]),
                Value::Int(7),
            ),
        ]);
        let n = project(&v);
        assert_eq!(n.kind, "dict");
        assert_eq!(n.display, "dict (2)");
        assert_eq!(n.children.len(), 2);
        assert_eq!(n.children[0].label.as_deref(), Some("b\"geom\""));
        assert_eq!(n.children[0].path, vec![Step::DictValue(0)]);
        assert!(n.children[0].removable);
        assert_eq!(
            n.children[1].label.as_deref(),
            Some("(b\"overviewScroll2\", 1)")
        );
        assert!(n.children[1].editable, "int value is editable");
        // tuple child of first entry
        let t = &n.children[0];
        assert_eq!(t.children[0].path, vec![Step::DictValue(0), Step::Tuple(0)]);
        assert!(t.children[0].removable, "tuples are editable sequences");
    }

    #[test]
    fn shared_subtree_is_flagged_and_not_removable() {
        let v = Value::Dict(vec![(
            Value::Bytes(b"k".to_vec()),
            Value::Shared { slot: 1, value: Box::new(Value::List(vec![Value::Int(1)])) },
        )]);
        let n = project(&v);
        let entry = &n.children[0];
        assert_eq!(entry.kind, "shared");
        assert!(!entry.removable, "entries containing Shared cannot be removed");
        let inner = &entry.children[0];
        assert!(inner.in_shared);
        assert_eq!(inner.path, vec![Step::DictValue(0), Step::SharedInner]);
        assert!(inner.children[0].in_shared, "flag propagates down");
    }

    #[test]
    fn node_serializes_to_json() {
        let v = Value::List(vec![Value::Str("hi".into())]);
        let json = serde_json::to_value(project(&v)).unwrap();
        assert_eq!(json["kind"], "list");
        assert_eq!(json["children"][0]["display"], "\"hi\"");
        assert_eq!(json["children"][0]["path"][0]["s"], "list");
    }

    #[test]
    fn non_finite_floats_are_not_editable() {
        let v = Value::List(vec![Value::Float(f64::NAN), Value::Float(2.5)]);
        let n = project(&v);
        assert!(!n.children[0].editable);
        assert_eq!(n.children[0].edit_text, None);
        assert!(n.children[1].editable, "finite floats stay editable");
    }

    // NOTE: the edit_text ↔ SetScalar round-trip contract is tested in
    // mutate.rs (Task 5), which owns the other half of that contract.
}
