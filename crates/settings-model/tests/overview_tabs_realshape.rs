//! Real-idiom guard for overview_tabs: (ts,dict) wrappers, StrTable name keys,
//! Shared/Ref tokens, legacy tabsettings migration. Edits must survive an
//! encode -> decode round-trip after reshare (the save-chain boundary).
use blue_marshal::{decode, encode, inline, reshare, Value};
use settings_model::{create_tab, delete_tab, project_overview, rename_tab};

fn b(s: &[u8]) -> Value { Value::Bytes(s.to_vec()) }
fn ts() -> Value { Value::Long(vec![0u8; 8]) }

/// True if tab `idx` in `tabsettings_new` carries `key` (tree already inlined).
fn tab_field(v: &Value, idx: i64, key: &[u8]) -> bool {
    fn isb(k: &Value, n: &[u8]) -> bool { matches!(k, Value::Bytes(b) if b.as_slice() == n) }
    fn inner(v: &Value) -> Option<&Vec<(Value, Value)>> {
        match v {
            Value::Dict(d) => Some(d),
            Value::Tuple(t) => t.iter().find_map(|e| if let Value::Dict(d) = e { Some(d) } else { None }),
            _ => None,
        }
    }
    let Value::Dict(root) = v else { return false };
    let Some((_, ov)) = root.iter().find(|(k, _)| isb(k, b"overview")) else { return false };
    let Some(ovd) = inner(ov) else { return false };
    let Some((_, tabs)) = ovd.iter().find(|(k, _)| isb(k, b"tabsettings_new")) else { return false };
    let Some(td) = inner(tabs) else { return false };
    let Some((_, tab)) = td.iter().find(|(k, _)| matches!(k, Value::Int(i) if *i == idx)) else { return false };
    let Some(fields) = inner(tab) else { return false };
    fields.iter().any(|(k, _)| isb(k, key))
}

/// Legacy overview: `tabsettings` (NOT `_new`), (ts,dict)-wrapped, StrTable name
/// keys, a Shared preset token Ref'd across tabs.
fn legacy_user() -> Value {
    let preset = Value::Shared { slot: 3, value: Box::new(b(b"PvP")) };
    let tab0 = Value::Dict(vec![
        (b(b"bracket"), b(b"_BracketFilterShowAll")),
        (b(b"color"), Value::None),
        (Value::StrTable(52), Value::Str("Main".into())),
        (b(b"overview"), preset),
    ]);
    let tab1 = Value::Dict(vec![
        (Value::StrTable(52), b(b"Scan")),
        (b(b"overview"), Value::Ref(3)),
    ]);
    let overview = Value::Dict(vec![
        (b(b"tabsettings"),
         Value::Tuple(vec![ts(), Value::Dict(vec![
             (Value::Int(0), tab0), (Value::Int(1), tab1),
         ])])),
        (b(b"tabsByWindowInstanceID"),
         Value::Tuple(vec![ts(), Value::List(vec![
             Value::List(vec![Value::Int(0), Value::Int(1)]),
         ])])),
    ]);
    Value::Dict(vec![(b(b"overview"), overview)])
}

#[test]
fn edits_survive_reshare_roundtrip_and_migrate_legacy() {
    let mut v = legacy_user();

    // Rename tab 0, create tab 2 in window 0, delete tab 1.
    rename_tab(&mut v, 0, "Combat").unwrap();
    let idx = create_tab(&mut v, 0, "Mining", Some(0)).unwrap();
    assert_eq!(idx, 2);
    delete_tab(&mut v, 1).unwrap();

    // Reshare (app-layer boundary) then round-trip through the codec.
    v = reshare(&v);
    let bytes = encode(&v).expect("reshared tree encodes");
    let round = decode(&bytes).expect("re-decodes");
    assert_eq!(round, v, "reshared overview_tabs edit round-trips");

    // The created tab (index 2) cloned tab 0's bracket + color — the keys whose
    // absence broke EVE's "reset all overview settings" — and they survive the
    // reshare -> encode -> decode save path.
    let flat = inline(&round);
    assert!(tab_field(&flat, 2, b"bracket"), "cloned tab keeps bracket through the save path");
    assert!(tab_field(&flat, 2, b"color"), "cloned tab keeps color through the save path");

    // Project the result: tab 0 renamed, tab 2 present, tab 1 gone, legacy migrated.
    let cols = project_overview(&round, None);
    let names: Vec<_> = cols.tabs.iter().map(|t| (t.index, t.name.clone())).collect();
    assert!(names.contains(&(0, "Combat".to_string())));
    assert!(names.contains(&(2, "Mining".to_string())));
    assert!(!names.iter().any(|(i, _)| *i == 1), "tab 1 deleted");
    // Migration: the container now reads via tabsettings_new (project_overview
    // prefers it); the two surviving tabs (0 and 2) project, tab 1 was deleted.
    assert_eq!(cols.tabs.len(), 2);
}
