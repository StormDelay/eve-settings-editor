use blue_marshal::Value;

/// True if any node in the subtree is a `Shared` store. Removing such a
/// subtree would orphan its slot (encode fails SlotOutOfRange) or dangle
/// Refs elsewhere — so removal is blocked at the mutation layer.
pub fn subtree_contains_shared(v: &Value) -> bool {
    match v {
        Value::Shared { .. } => true,
        Value::Tuple(items) | Value::List(items) => items.iter().any(subtree_contains_shared),
        Value::Dict(entries) => entries
            .iter()
            .any(|(k, val)| subtree_contains_shared(k) || subtree_contains_shared(val)),
        Value::Stream(inner) => subtree_contains_shared(inner),
        Value::Instance { class, state } => {
            subtree_contains_shared(class) || subtree_contains_shared(state)
        }
        Value::Reduce { ctor, items, pairs } => {
            subtree_contains_shared(ctor)
                || items.iter().any(subtree_contains_shared)
                || pairs
                    .iter()
                    .any(|(k, v)| subtree_contains_shared(k) || subtree_contains_shared(v))
        }
        _ => false,
    }
}

use serde::Deserialize;

use crate::path::{resolve_mut, NodePath, Step};

/// The raw editor's mutation set. Deliberately small for V1:
/// - scalar edits keep the node's wire kind (no kind changes);
/// - removal is dict entries and sequence (list/tuple) items, and refuses
///   subtrees containing `Shared` stores;
/// - inserts go into dicts (appended, wire order) and sequences.
///
/// Tuples are mutable as sequences because real settings entries (a chat
/// channel, an overview tab) ARE tuples: with tuples frozen there is no way
/// to build one. The wire format encodes any arity, so arity changes are the
/// user's business — the save chain backs the file up first.
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Mutation {
    SetScalar { path: NodePath, text: String },
    RemoveEntry { path: NodePath },
    InsertDictEntry { parent: NodePath, key: NewValue, value: NewValue },
    InsertListItem { parent: NodePath, index: usize, value: NewValue },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum NewValue {
    None,
    Bool(bool),
    Int(String),
    Float(String),
    Str(String),
    StrUcs2(String),
    /// Raw bytes as hex digits (e.g. "6f76657276696577" = b"overview").
    BytesHex(String),
    EmptyDict,
    EmptyList,
    EmptyTuple,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum MutateError {
    BadPath,
    NotScalar(&'static str),
    Parse(String),
    /// A `Parse` failure in the *key* half of an insert. Same failure, its own
    /// code, so the UI can put the message next to the field that caused it.
    ParseKey(String),
    /// Removal refused: the subtree contains a `Shared` store whose slot
    /// the encoder needs (and Refs elsewhere may point at).
    SharedSubtree,
    NotRemovable,
    NotAContainer(&'static str),
    BadIndex(usize),
}

/// Human-readable form — this is what the UI shows the user, so it reads as a
/// sentence rather than as a debug-printed variant.
impl std::fmt::Display for MutateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MutateError::BadPath => write!(f, "no such node"),
            MutateError::NotScalar(kind) => write!(f, "not an editable scalar: {kind}"),
            MutateError::Parse(detail) | MutateError::ParseKey(detail) => write!(f, "{detail}"),
            MutateError::SharedSubtree => write!(
                f,
                "contains a shared object that other entries point at — removing it \
                 would break them"
            ),
            MutateError::NotRemovable => write!(f, "this node cannot be removed"),
            MutateError::NotAContainer(kind) => write!(f, "not a container: {kind}"),
            MutateError::BadIndex(i) => write!(f, "index out of range: {i}"),
        }
    }
}

pub fn apply(root: &mut Value, m: &Mutation) -> Result<(), MutateError> {
    match m {
        Mutation::SetScalar { path, text } => {
            let node = resolve_mut(root, path).ok_or(MutateError::BadPath)?;
            set_scalar(node, text)
        }
        Mutation::RemoveEntry { path } => remove_entry(root, path),
        Mutation::InsertDictEntry { parent, key, value } => {
            let key = build_value(key).map_err(|e| match e {
                MutateError::Parse(d) => MutateError::ParseKey(d),
                other => other,
            })?;
            let value = build_value(value)?;
            match resolve_mut(root, parent).ok_or(MutateError::BadPath)? {
                Value::Dict(entries) => {
                    entries.push((key, value));
                    Ok(())
                }
                other => Err(MutateError::NotAContainer(crate::projection_kind(other))),
            }
        }
        Mutation::InsertListItem { parent, index, value } => {
            let value = build_value(value)?;
            match resolve_mut(root, parent).ok_or(MutateError::BadPath)? {
                Value::List(items) | Value::Tuple(items) => {
                    if *index > items.len() {
                        return Err(MutateError::BadIndex(*index));
                    }
                    items.insert(*index, value);
                    Ok(())
                }
                other => Err(MutateError::NotAContainer(crate::projection_kind(other))),
            }
        }
    }
}

/// Edit a scalar in place, keeping its wire kind. Parse rules per kind:
/// int: decimal i64 · float: finite f64 · str/str_ucs2: raw text ·
/// bytes/long: "hex:"-prefixed hex OR (bytes only) plain text taken as its
/// UTF-8 bytes · str_table: table index 1..=255 · bool: "true"/"false".
fn set_scalar(node: &mut Value, text: &str) -> Result<(), MutateError> {
    let parse_err = |what: &str| MutateError::Parse(format!("{what}: {text:?}"));
    match node {
        Value::Bool(b) => {
            *b = match text {
                "true" | "True" => true,
                "false" | "False" => false,
                _ => return Err(parse_err("expected true/false")),
            };
        }
        Value::Int(i) => *i = text.trim().parse::<i64>().map_err(|e| parse_err(&e.to_string()))?,
        Value::Float(f) => {
            let v = text.trim().parse::<f64>().map_err(|e| parse_err(&e.to_string()))?;
            if !v.is_finite() {
                return Err(parse_err("must be finite"));
            }
            *f = v;
        }
        Value::Str(s) => *s = text.to_string(),
        Value::StrUcs2(s) => *s = text.to_string(),
        Value::Bytes(b) => {
            *b = match text.strip_prefix("hex:") {
                Some(h) => parse_hex(h).ok_or_else(|| parse_err("bad hex"))?,
                None => text.as_bytes().to_vec(),
            };
        }
        Value::Long(b) => {
            let h = text.strip_prefix("hex:").unwrap_or(text);
            *b = parse_hex(h).ok_or_else(|| parse_err("long edits take hex bytes"))?;
        }
        Value::StrTable(idx) => {
            let v: u8 = text.trim().parse().map_err(|_| parse_err("table index 1-255"))?;
            if v == 0 {
                return Err(parse_err("table index 1-255"));
            }
            *idx = v;
        }
        other => return Err(MutateError::NotScalar(crate::projection_kind(other))),
    }
    Ok(())
}

fn remove_entry(root: &mut Value, path: &NodePath) -> Result<(), MutateError> {
    let Some((last, parent_path)) = path.split_last() else {
        return Err(MutateError::NotRemovable); // the root itself
    };
    // Guard BEFORE mutating: the node being removed (for dict entries: key
    // AND value) must not contain a Shared store.
    match last {
        Step::DictValue(i) | Step::DictKey(i) => {
            let parent = resolve_mut(root, parent_path).ok_or(MutateError::BadPath)?;
            let Value::Dict(entries) = parent else { return Err(MutateError::BadPath) };
            let (k, v) = entries.get(*i).ok_or(MutateError::BadPath)?;
            if subtree_contains_shared(k) || subtree_contains_shared(v) {
                return Err(MutateError::SharedSubtree);
            }
            entries.remove(*i);
            Ok(())
        }
        Step::List(i) | Step::Tuple(i) => {
            let parent = resolve_mut(root, parent_path).ok_or(MutateError::BadPath)?;
            let (Value::List(items) | Value::Tuple(items)) = parent else {
                return Err(MutateError::BadPath);
            };
            let item = items.get(*i).ok_or(MutateError::BadPath)?;
            if subtree_contains_shared(item) {
                return Err(MutateError::SharedSubtree);
            }
            items.remove(*i);
            Ok(())
        }
        _ => Err(MutateError::NotRemovable),
    }
}

fn build_value(nv: &NewValue) -> Result<Value, MutateError> {
    let parse_err = |what: &str, t: &str| MutateError::Parse(format!("{what}: {t:?}"));
    Ok(match nv {
        NewValue::None => Value::None,
        NewValue::Bool(b) => Value::Bool(*b),
        NewValue::Int(t) => {
            Value::Int(t.trim().parse::<i64>().map_err(|e| parse_err(&e.to_string(), t))?)
        }
        NewValue::Float(t) => {
            let v = t.trim().parse::<f64>().map_err(|e| parse_err(&e.to_string(), t))?;
            if !v.is_finite() {
                return Err(parse_err("must be finite", t));
            }
            Value::Float(v)
        }
        NewValue::Str(t) => Value::Str(t.clone()),
        NewValue::StrUcs2(t) => Value::StrUcs2(t.clone()),
        NewValue::BytesHex(h) => Value::Bytes(parse_hex(h).ok_or_else(|| parse_err("bad hex", h))?),
        NewValue::EmptyDict => Value::Dict(vec![]),
        NewValue::EmptyList => Value::List(vec![]),
        NewValue::EmptyTuple => Value::Tuple(vec![]),
    })
}

fn parse_hex(s: &str) -> Option<Vec<u8>> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if !s.is_ascii() || s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::Step;

    fn doc() -> Value {
        // { b"lists": [ "a", "b" ], b"geom": (1, 2), b"shared": shared[1]:[9] }
        Value::Dict(vec![
            (
                Value::Bytes(b"lists".to_vec()),
                Value::List(vec![Value::Str("a".into()), Value::Str("b".into())]),
            ),
            (
                Value::Bytes(b"geom".to_vec()),
                Value::Tuple(vec![Value::Int(1), Value::Int(2)]),
            ),
            (
                Value::Bytes(b"shared".to_vec()),
                Value::Shared { slot: 1, value: Box::new(Value::List(vec![Value::Int(9)])) },
            ),
        ])
    }

    #[test]
    fn set_scalar_per_kind() {
        let mut v = doc();
        // int inside the tuple
        apply(&mut v, &Mutation::SetScalar {
            path: vec![Step::DictValue(1), Step::Tuple(0)],
            text: "424".into(),
        }).unwrap();
        // str inside the list
        apply(&mut v, &Mutation::SetScalar {
            path: vec![Step::DictValue(0), Step::List(1)],
            text: "edited".into(),
        }).unwrap();
        // int inside the SHARED list — allowed (edits alias by design)
        apply(&mut v, &Mutation::SetScalar {
            path: vec![Step::DictValue(2), Step::SharedInner, Step::List(0)],
            text: "7".into(),
        }).unwrap();
        let Value::Dict(entries) = &v else { unreachable!() };
        assert_eq!(entries[1].1, Value::Tuple(vec![Value::Int(424), Value::Int(2)]));
        assert_eq!(
            entries[0].1,
            Value::List(vec![Value::Str("a".into()), Value::Str("edited".into())])
        );
    }

    #[test]
    fn set_scalar_rejects_bad_input_without_mutating() {
        let mut v = doc();
        let before = v.clone();
        let err = apply(&mut v, &Mutation::SetScalar {
            path: vec![Step::DictValue(1), Step::Tuple(0)],
            text: "not-a-number".into(),
        }).unwrap_err();
        assert!(matches!(err, MutateError::Parse(_)));
        assert_eq!(v, before);
        // wrong kind: the dict itself is not a scalar
        let err = apply(&mut v, &Mutation::SetScalar { path: vec![], text: "5".into() })
            .unwrap_err();
        assert_eq!(err, MutateError::NotScalar("dict"));
    }

    #[test]
    fn remove_list_item_and_dict_entry() {
        let mut v = doc();
        apply(&mut v, &Mutation::RemoveEntry {
            path: vec![Step::DictValue(0), Step::List(0)],
        }).unwrap();
        apply(&mut v, &Mutation::RemoveEntry { path: vec![Step::DictValue(1)] }).unwrap();
        let Value::Dict(entries) = &v else { unreachable!() };
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].1, Value::List(vec![Value::Str("b".into())]));
        assert_eq!(entries[1].0, Value::Bytes(b"shared".to_vec()));
    }

    #[test]
    fn remove_refuses_shared_subtrees() {
        let mut v = doc();
        assert_eq!(
            apply(&mut v, &Mutation::RemoveEntry { path: vec![Step::DictValue(2)] }),
            Err(MutateError::SharedSubtree)
        );
        // The root itself, and steps into fixed shapes, stay non-removable.
        assert_eq!(
            apply(&mut v, &Mutation::RemoveEntry { path: vec![] }),
            Err(MutateError::NotRemovable)
        );
    }

    #[test]
    fn tuples_are_editable_sequences() {
        // The chat-channel case: build a tuple in a list, fill it, fix a typo.
        let mut v = doc();
        let list = vec![Step::DictValue(0)];
        apply(&mut v, &Mutation::InsertListItem {
            parent: list.clone(),
            index: 2,
            value: NewValue::EmptyTuple,
        }).unwrap();
        let tup = vec![Step::DictValue(0), Step::List(2)];
        for (i, text) in ["chan", "oops"].iter().enumerate() {
            apply(&mut v, &Mutation::InsertListItem {
                parent: tup.clone(),
                index: i,
                value: NewValue::Str((*text).into()),
            }).unwrap();
        }
        // …and the typo is removable again (tuple elements were frozen before).
        apply(&mut v, &Mutation::RemoveEntry {
            path: vec![Step::DictValue(0), Step::List(2), Step::Tuple(1)],
        }).unwrap();
        let Value::Dict(entries) = &v else { unreachable!() };
        let Value::List(items) = &entries[0].1 else { unreachable!() };
        assert_eq!(items[2], Value::Tuple(vec![Value::Str("chan".into())]));

        // Guards still hold inside tuples: out-of-range index, Shared subtree.
        assert_eq!(
            apply(&mut v, &Mutation::InsertListItem {
                parent: tup.clone(),
                index: 9,
                value: NewValue::None,
            }),
            Err(MutateError::BadIndex(9))
        );
        let mut shared_tup = Value::Tuple(vec![Value::Shared {
            slot: 1,
            value: Box::new(Value::Int(9)),
        }]);
        assert_eq!(
            apply(&mut shared_tup, &Mutation::RemoveEntry { path: vec![Step::Tuple(0)] }),
            Err(MutateError::SharedSubtree)
        );
    }

    #[test]
    fn inserts_into_dict_and_list() {
        let mut v = doc();
        apply(&mut v, &Mutation::InsertDictEntry {
            parent: vec![],
            key: NewValue::BytesHex("6b32".into()), // b"k2"
            value: NewValue::EmptyList,
        }).unwrap();
        apply(&mut v, &Mutation::InsertListItem {
            parent: vec![Step::DictValue(0)],
            index: 1,
            value: NewValue::Str("mid".into()),
        }).unwrap();
        let Value::Dict(entries) = &v else { unreachable!() };
        assert_eq!(entries[3].0, Value::Bytes(b"k2".to_vec()));
        assert_eq!(entries[3].1, Value::List(vec![]));
        assert_eq!(
            entries[0].1,
            Value::List(vec![
                Value::Str("a".into()),
                Value::Str("mid".into()),
                Value::Str("b".into()),
            ])
        );
        // bad index
        assert_eq!(
            apply(&mut v, &Mutation::InsertListItem {
                parent: vec![Step::DictValue(0)],
                index: 99,
                value: NewValue::None,
            }),
            Err(MutateError::BadIndex(99))
        );
    }

    #[test]
    fn insert_parse_errors_name_the_field_that_failed() {
        // The UI anchors the message to a field by this code, and shows the
        // Display form — neither may leak the Rust variant name.
        let mut v = doc();
        let bad_key = apply(&mut v, &Mutation::InsertDictEntry {
            parent: vec![],
            key: NewValue::Int("df".into()),
            value: NewValue::None,
        }).unwrap_err();
        assert!(matches!(bad_key, MutateError::ParseKey(_)));
        assert_eq!(serde_json::to_value(&bad_key).unwrap()["code"], "parse_key");
        assert!(bad_key.to_string().contains("df"), "{bad_key}");
        assert!(!bad_key.to_string().contains("ParseKey"), "{bad_key}");

        let bad_value = apply(&mut v, &Mutation::InsertDictEntry {
            parent: vec![],
            key: NewValue::Str("k".into()),
            value: NewValue::Int("df".into()),
        }).unwrap_err();
        assert!(matches!(bad_value, MutateError::Parse(_)));
        assert_eq!(serde_json::to_value(&bad_value).unwrap()["code"], "parse");
        assert_eq!(v, doc(), "a rejected insert changes nothing");
    }

    #[test]
    fn edit_text_round_trips_through_set_scalar() {
        // For every editable kind: applying SetScalar with the node's own
        // projection edit_text must be a no-op — the inline-edit seed
        // contract shared between projection.rs and this module.
        let scalars = vec![
            Value::Bool(false),
            Value::Int(-42),
            Value::Float(2.5),
            Value::Str("plain".into()),
            Value::StrUcs2("u".into()),
            Value::Bytes(b"overview".to_vec()),
            Value::Bytes(vec![0x00, 0xFF]),
            Value::Bytes(b"hex:trap".to_vec()), // printable but ambiguous
            Value::Long(vec![0x2A, 0x00]),
            Value::StrTable(7),
        ];
        for s in scalars {
            let mut v = Value::List(vec![s.clone()]);
            let n = crate::projection::project(&v);
            let text = n.children[0].edit_text.clone().expect("editable");
            apply(&mut v, &Mutation::SetScalar { path: vec![Step::List(0)], text }).unwrap();
            assert_eq!(v, Value::List(vec![s]), "edit_text must be a no-op seed");
        }
    }

    #[test]
    fn mutation_json_shape_is_stable() {
        // The UI sends exactly this JSON — the serde shape is a contract.
        let m: Mutation = serde_json::from_str(
            r#"{"op":"set_scalar","path":[{"s":"dict_value","i":0}],"text":"5"}"#,
        ).unwrap();
        assert!(matches!(m, Mutation::SetScalar { .. }));
        let m: Mutation = serde_json::from_str(
            r#"{"op":"insert_dict_entry","parent":[],
                "key":{"kind":"str","v":"name"},"value":{"kind":"empty_dict"}}"#,
        ).unwrap();
        assert!(matches!(m, Mutation::InsertDictEntry { .. }));
    }

    #[test]
    fn parse_hex_rejects_non_ascii_without_panicking() {
        // Regression: byte-offset slicing used to panic mid-char on
        // multi-byte UTF-8 reaching set_scalar from raw UI text.
        let mut v = Value::List(vec![Value::Long(vec![0x2A])]);
        let err = apply(&mut v, &Mutation::SetScalar {
            path: vec![Step::List(0)],
            text: "hex:€€".into(),
        })
        .unwrap_err();
        assert!(matches!(err, MutateError::Parse(_)));
        assert_eq!(v, Value::List(vec![Value::Long(vec![0x2A])]));
    }
}
