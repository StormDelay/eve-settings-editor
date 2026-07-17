//! Real-idiom guard for batch category copy. Synthetic trees reproducing the
//! STRUCTURE real files use, where the Shared/Ref pair genuinely CROSSES the
//! copied category's boundary: a window id shared between `windows` and a
//! sibling top-level key (char), a location name shared between `editHistory`
//! and a sibling `ui` key (user). That crossing is what makes `inline_all`
//! load-bearing here — a Shared/Ref pair fully contained INSIDE the copied
//! subtree would ride along intact on a whole-subtree clone and encode fine
//! with or without inlining, proving nothing about inline-first. Encoded,
//! decoded, and driven through the public batch API only. No bytes were read
//! from a real file.

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

/// char root -> { windowNames: [Shared id], windows: { windowSizesAndPositions_1:
/// (ts, { Ref(id): geom }), openWindows: (ts, { Ref(id): True }) } }
///
/// Real char files repeat a window id across several top-level trackers (marshal
/// dedups a repeated string wherever it first appears), so the id's `Shared`
/// definition lives in `windowNames` — a sibling of `windows` that precedes it
/// in the root dict, so the fixture itself is a valid, encodable file — while
/// every use inside `windows` is a bare `Ref`. Extracting `windows` without
/// first inlining the whole source clones a subtree holding only `Ref`s, with
/// no `Shared` anywhere in it: a dangling reference that fails to encode.
fn char_with_layout(id: &str) -> Value {
    let name = Value::Shared { slot: 1, value: Box::new(Value::Bytes(id.as_bytes().to_vec())) };
    let geoms = Value::Dict(vec![(Value::Ref(1), geom())]);
    let opens = Value::Dict(vec![(Value::Ref(1), Value::Bool(true))]);
    Value::Dict(vec![
        (b("windowNames"), Value::List(vec![name])), // Shared def, sibling of `windows`, precedes it
        (
            b("windows"),
            Value::Dict(vec![
                (b("windowSizesAndPositions_1"), Value::Tuple(vec![ts(), geoms])),
                (b("openWindows"), Value::Tuple(vec![ts(), opens])),
            ]),
        ),
    ])
}

/// user root -> ui -> { locationNames: [Shared "Jita"], editHistory: (ts, {
/// first: [Ref], "/b": [Ref] }) }
///
/// Real user files repeat a location name across widgets. The `Shared`
/// definition lives in `locationNames` — a sibling of `editHistory` that
/// precedes it in `ui`'s dict, mirroring `batch.rs`'s own
/// `shareDef`/`editHistory` unit-test idiom — while every widget's list holds
/// only a bare `Ref`. Extracting `editHistory` without first inlining the
/// whole source clones a subtree holding only `Ref`s to a `Shared` that lives
/// outside it: a dangling reference that fails to encode.
fn user_with_history(first: &str) -> Value {
    let jita = Value::Shared { slot: 1, value: Box::new(Value::Bytes(b"Jita".to_vec())) };
    let hist = Value::Dict(vec![
        (b(first), Value::List(vec![Value::Ref(1)])),
        (b("/b"), Value::List(vec![Value::Ref(1)])),
    ]);
    let ui = Value::Dict(vec![
        (b("locationNames"), Value::List(vec![jita])), // Shared def, sibling of editHistory, precedes it
        (b("editHistory"), Value::Tuple(vec![ts(), hist])),
    ]);
    Value::Dict(vec![(b("ui"), ui)])
}

#[test]
fn layout_copy_between_chars_encodes_and_matches_source() {
    let source = char_with_layout("overview");
    encode(&source).expect("source fixture encodes (def precedes ref)");
    let mut target = char_with_layout("market"); // different window id
    encode(&target).expect("target fixture encodes (def precedes ref)");

    let extracted = extract_categories(&source, &[Category::Layout]);
    apply_to_tree(&mut target, &extracted);
    let bytes = encode(&target)
        .expect("post-copy target encodes (cross-boundary Ref resolved by inline-first)");

    let wl = window_layout(&decode(&bytes).unwrap());
    let ids: Vec<&str> = wl.windows.iter().map(|w| w.id.as_str()).collect();
    assert_eq!(ids, vec!["overview"], "target now carries the source's window");
}

#[test]
fn autofill_copy_between_users_encodes_and_matches_source() {
    let source = user_with_history("/a");
    encode(&source).expect("source fixture encodes (def precedes ref)");
    let mut target = user_with_history("/other");
    encode(&target).expect("target fixture encodes (def precedes ref)");

    let extracted = extract_categories(&source, &[Category::Autofill]);
    apply_to_tree(&mut target, &extracted);
    let bytes = encode(&target)
        .expect("post-copy target encodes (cross-boundary Ref resolved by inline-first)");

    let lists = project_edit_history(&decode(&bytes).unwrap());
    let widgets: Vec<&str> = lists.iter().map(|l| l.widget.as_str()).collect();
    assert!(widgets.contains(&"/a"), "target now has the source's widget list");
    assert!(!widgets.contains(&"/other"), "target's old category was replaced wholesale");
}
