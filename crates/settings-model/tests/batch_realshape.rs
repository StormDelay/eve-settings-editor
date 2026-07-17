//! Real-idiom guard for batch category copy. Synthetic trees reproducing the
//! STRUCTURE real files use — a `windows` container keyed by a Shared window id
//! with a Ref elsewhere (char), a `(ts, dict)` editHistory whose list shares a
//! Bytes string via Shared/Ref (user) — encoded, decoded, and driven through the
//! public batch API only. No bytes were read from a real file.

use blue_marshal::{decode, encode, Value};
use settings_model::{
    apply_to_tree, extract_categories, project_edit_history, window_layout, Category,
};

fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
fn ts() -> Value { Value::Long(vec![0u8; 8]) }

fn geom() -> Value {
    Value::Tuple(vec![
        Value::Int(1), Value::Int(2), Value::Int(3),
        Value::Int(4), Value::Int(2560), Value::Int(1440),
    ])
}

/// char root -> windows -> { windowSizesAndPositions_1: (ts, { "overview": geom }),
///                           openWindows: (ts, { <Shared "overview">: True }) }
/// The window id "overview" is shared between the two sub-dicts (Shared + Ref).
fn char_with_layout(id: &str) -> Value {
    let name = Value::Shared { slot: 1, value: Box::new(Value::Bytes(id.as_bytes().to_vec())) };
    let geoms = Value::Dict(vec![(name, geom())]);
    let opens = Value::Dict(vec![(Value::Ref(1), Value::Bool(true))]);
    Value::Dict(vec![(
        b("windows"),
        Value::Dict(vec![
            (b("windowSizesAndPositions_1"), Value::Tuple(vec![ts(), geoms])),
            (b("openWindows"), Value::Tuple(vec![ts(), opens])),
        ]),
    )])
}

/// user root -> ui -> editHistory -> (ts, { "/a": [Shared "Jita"], "/b": [Ref -> "Jita"] })
fn user_with_history(first: &str) -> Value {
    let jita = Value::Shared { slot: 1, value: Box::new(Value::Bytes(b"Jita".to_vec())) };
    let hist = Value::Dict(vec![
        (b(first), Value::List(vec![jita])),
        (b("/b"), Value::List(vec![Value::Ref(1)])),
    ]);
    let ui = Value::Dict(vec![(b("editHistory"), Value::Tuple(vec![ts(), hist]))]);
    Value::Dict(vec![(b("ui"), ui)])
}

#[test]
fn layout_copy_between_chars_encodes_and_matches_source() {
    let source = char_with_layout("overview");
    encode(&source).expect("source fixture encodes");
    let mut target = char_with_layout("market"); // different window id
    encode(&target).expect("target fixture encodes");

    let extracted = extract_categories(&source, &[Category::Layout]);
    apply_to_tree(&mut target, &extracted);
    let bytes = encode(&target).expect("post-copy target encodes (no dangling Ref)");

    let wl = window_layout(&decode(&bytes).unwrap());
    let ids: Vec<&str> = wl.windows.iter().map(|w| w.id.as_str()).collect();
    assert_eq!(ids, vec!["overview"], "target now carries the source's window");
}

#[test]
fn autofill_copy_between_users_encodes_and_matches_source() {
    let source = user_with_history("/a");
    encode(&source).expect("source fixture encodes");
    let mut target = user_with_history("/other");
    encode(&target).expect("target fixture encodes");

    let extracted = extract_categories(&source, &[Category::Autofill]);
    apply_to_tree(&mut target, &extracted);
    let bytes = encode(&target).expect("post-copy target encodes");

    let lists = project_edit_history(&decode(&bytes).unwrap());
    let widgets: Vec<&str> = lists.iter().map(|l| l.widget.as_str()).collect();
    assert!(widgets.contains(&"/a"), "target now has the source's widget list");
    assert!(!widgets.contains(&"/other"), "target's old category was replaced wholesale");
}
