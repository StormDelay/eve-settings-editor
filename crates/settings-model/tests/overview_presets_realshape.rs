//! Real-idiom guard for filter-preset authoring: `(timestamp, dict)` wrappers and
//! Shared/Ref-interned preset names (a preset key shared with a tab's `overview`
//! value), edited then reshared and re-decoded.

use blue_marshal::Value;
use settings_model::{create_preset, delete_preset, project_overview, rename_preset};

fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }

/// user -> overview -> {
///   tabsettings_new: (ts, { 0: { overview: Ref(7) } }),
///   overviewProfilePresets: (ts, { Shared(7,"pvp"): {groups:[25]}, "pve": {groups:[26]} }),
///   overviewProfilePresets_notSaved: (ts, { Ref(7): {groups:[99]} }),
/// }
/// Shared slot 7 interns the preset name "pvp"; the tab's overview value and the
/// notSaved key both Ref it — exactly how real files share the name.
fn realish_user() -> Value {
    let name_shared = Value::Shared { slot: 7, value: Box::new(b("pvp")) };
    let name_ref = Value::Ref(7);
    let tab0 = Value::Dict(vec![(b("overview"), name_ref.clone())]);
    let preset = |g: i64| Value::Dict(vec![(b("groups"), Value::List(vec![Value::Int(g)]))]);
    let overview = Value::Dict(vec![
        (b("tabsettings_new"), Value::Tuple(vec![
            Value::Int(1), Value::Dict(vec![(Value::Int(0), tab0)]),
        ])),
        (b("overviewProfilePresets"), Value::Tuple(vec![
            Value::Int(1),
            Value::Dict(vec![(name_shared, preset(25)), (b("pve"), preset(26))]),
        ])),
        (b("overviewProfilePresets_notSaved"), Value::Tuple(vec![
            Value::Int(1),
            Value::Dict(vec![(name_ref, preset(99))]),
        ])),
    ]);
    Value::Dict(vec![(b("overview"), overview)])
}

/// Reshare and confirm the tree still encodes+decodes to itself (the standard
/// regression check that an edit left a canonical, self-contained file).
fn reshare_roundtrips(v: &Value) -> Value {
    let reshared = blue_marshal::reshare(v);
    let bytes = blue_marshal::encode(&reshared).expect("encode");
    let decoded = blue_marshal::decode(&bytes).expect("decode");
    assert_eq!(decoded, reshared, "reshared tree must re-decode identically");
    reshared
}

#[test]
fn rename_across_shared_name_reshares_and_reprojects() {
    let mut v = realish_user();
    rename_preset(&mut v, "pvp", "pvp2").unwrap();
    let v = reshare_roundtrips(&v);
    let cols = project_overview(&v, None);
    assert!(cols.presets.contains(&"pvp2".to_string()));
    assert!(!cols.presets.contains(&"pvp".to_string()));
    assert_eq!(cols.tabs[0].preset, "pvp2", "the Ref'd tab followed the rename after inline");
}

#[test]
fn duplicate_then_delete_reshares_clean() {
    let mut v = realish_user();
    create_preset(&mut v, "pvp", "pvp copy").unwrap();
    delete_preset(&mut v, "pve").unwrap();
    let v = reshare_roundtrips(&v);
    let cols = project_overview(&v, None);
    assert!(cols.presets.contains(&"pvp copy".to_string()));
    assert!(!cols.presets.contains(&"pve".to_string()));
}
