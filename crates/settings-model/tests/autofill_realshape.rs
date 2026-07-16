//! Real-idiom corpus guard for the autofill (editHistory) projection and edits.
//! A synthetic tree reproducing the STRUCTURE real `core_user` files use —
//! (timestamp, dict) editHistory wrapper, widget lists that share a repeated
//! string via Shared/Ref, an empty-Bytes junk entry, a (timestamp, list)-wrapped
//! widget value — encoded, decoded, and driven through the public API only. No
//! bytes were read from a real file.

use blue_marshal::{decode, encode, Value};
use settings_model::{clear_all_history, project_edit_history, set_list_entries};

fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
fn ts() -> Value { Value::Long(vec![0u8; 8]) }

/// root -> b"ui" -> b"editHistory" -> (ts, {
///   "/a/box": ["Jita"(Shared 1), ""(empty Bytes)],
///   "/b/box": (ts, [Ref 1 -> "Jita"]),   // (timestamp, list)-wrapped value
/// })
fn realshape_user() -> Value {
    // EVE's marshal only shares Bytes/Long/List/Dict — never Str — so a shared
    // remembered string is stored as Bytes; `entry_str` lossy-decodes it to "Jita".
    let jita = Value::Shared { slot: 1, value: Box::new(Value::Bytes(b"Jita".to_vec())) };
    let hist = Value::Dict(vec![
        (b("/a/box"), Value::List(vec![jita, Value::Bytes(vec![])])),
        (b("/b/box"), Value::Tuple(vec![ts(), Value::List(vec![Value::Ref(1)])])),
    ]);
    let ui = Value::Dict(vec![(b("editHistory"), Value::Tuple(vec![ts(), hist]))]);
    Value::Dict(vec![(b("ui"), ui)])
}

#[test]
fn realshape_round_trips_and_projects() {
    let bytes = encode(&realshape_user()).expect("fixture must encode");
    let decoded = decode(&bytes).expect("must decode back");
    let lists = project_edit_history(&decoded);
    assert_eq!(lists.len(), 2);
    let a = lists.iter().find(|l| l.widget == "/a/box").unwrap();
    assert_eq!(a.entries, vec!["Jita", ""], "Shared resolved; empty Bytes -> \"\"");
    let bl = lists.iter().find(|l| l.widget == "/b/box").unwrap();
    assert_eq!(bl.entries, vec!["Jita"], "Ref resolved through the (ts,list) wrapper");
}

#[test]
fn realshape_edit_then_clear_all_still_encode() {
    let mut user = decode(&encode(&realshape_user()).unwrap()).unwrap();
    // Editing the list that owns the Shared def must not dangle /b/box's Ref.
    set_list_entries(&mut user, "/a/box", &["Dodixie".into()]).unwrap();
    let bytes = encode(&user).expect("post-edit tree must encode");
    let lists = project_edit_history(&decode(&bytes).unwrap());
    assert_eq!(lists.iter().find(|l| l.widget == "/a/box").unwrap().entries, vec!["Dodixie"]);
    assert_eq!(lists.iter().find(|l| l.widget == "/b/box").unwrap().entries, vec!["Jita"]);

    clear_all_history(&mut user).unwrap();
    encode(&user).expect("post-clear tree must encode");
    assert!(project_edit_history(&user).iter().all(|l| l.entries.is_empty()));
}
