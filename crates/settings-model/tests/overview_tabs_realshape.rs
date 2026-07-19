//! Real-idiom guard for overview_tabs: (ts,dict) wrappers, StrTable name keys,
//! Shared/Ref tokens, legacy tabsettings migration. Edits must survive an
//! encode -> decode round-trip after reshare (the save-chain boundary).
use blue_marshal::{decode, encode, reshare, Value};
use settings_model::{create_tab, delete_tab, project_overview, rename_tab};

fn b(s: &[u8]) -> Value { Value::Bytes(s.to_vec()) }
fn ts() -> Value { Value::Long(vec![0u8; 8]) }

/// Legacy overview: `tabsettings` (NOT `_new`), (ts,dict)-wrapped, StrTable name
/// keys, a Shared preset token Ref'd across tabs.
fn legacy_user() -> Value {
    let preset = Value::Shared { slot: 3, value: Box::new(b(b"PvP")) };
    let tab0 = Value::Dict(vec![
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
    let idx = create_tab(&mut v, 0, "Mining", "PvE").unwrap();
    assert_eq!(idx, 2);
    delete_tab(&mut v, 1).unwrap();

    // Reshare (app-layer boundary) then round-trip through the codec.
    v = reshare(&v);
    let bytes = encode(&v).expect("reshared tree encodes");
    let round = decode(&bytes).expect("re-decodes");
    assert_eq!(round, v, "reshared overview_tabs edit round-trips");

    // Project the result: tab 0 renamed, tab 2 present, tab 1 gone, legacy migrated.
    let cols = project_overview(&round, None);
    let names: Vec<_> = cols.tabs.iter().map(|t| (t.index, t.name.clone())).collect();
    assert!(names.contains(&(0, "Combat".to_string())));
    assert!(names.contains(&(2, "Mining".to_string())));
    assert!(!names.iter().any(|(i, _)| *i == 1), "tab 1 deleted");
    // Migration: the container now reads via tabsettings_new (project_overview
    // prefers it), so all three project.
    assert_eq!(cols.tabs.len(), 2);
}
