//! Real-idiom corpus guard for the overview projection (Task R5).
//!
//! Fully synthetic `Value` trees that reproduce the STRUCTURE real EVE
//! `core_user` files use for the overview category — Ref/Shared-keyed
//! containers, string-table `name` keys, preset-fallback columns, partial
//! tab ownership, window grouping, and `(timestamp, list)` wrappers with
//! deduped column tokens — encoded, decoded, and projected through the
//! public API only. No bytes/ids/names here were read from a real file;
//! every id and token is invented (`0..3`, `"P"`, `"Alpha"`, `NAME`/`TYPE`/
//! `DISTANCE`/`ICON`).

use blue_marshal::{decode, encode, Value};
use settings_model::project_overview;

fn ts() -> Value {
    Value::Long(vec![0u8; 8])
}

fn b(s: &str) -> Value {
    Value::Bytes(s.as_bytes().to_vec())
}

/// A modern (`tabsettings_new`) tree whose `overview` container is found
/// through a bare `Ref` key (the pure-Ref-key path — a bare `is_bytes` key
/// match would miss it and project empty).
///
/// Covers:
/// - item 1 (Ref-keyed container, the Shared("overview") defined elsewhere)
/// - item 2 (tabsettings_new)
/// - item 3 (StrTable(52) "name" key, both Str and Bytes values)
/// - item 4 (preset fallback, with a Ref item in the preset's columns)
/// - item 5 (partial ownership: order-only and visible-only tabs)
/// - item 6 (window grouping)
/// - item 7 ((timestamp, list) master list + Shared column tokens)
fn modern_ref_keyed_tree() -> Value {
    // Column token "NAME" is deduped once (slot 2 — blue-marshal's tail-map
    // slots are 1-based and dense over the whole stream's Shared count, not
    // arbitrary numbers) and referenced from three places below (tab0's own
    // order list defines it; tab0's own visible list and the preset's columns
    // both reference it by Ref) — the real-file idiom of a repeated column
    // token being stored once and Ref'd elsewhere.
    let name_tok_shared = Value::Shared { slot: 2, value: Box::new(b("NAME")) };

    // tab 0: owns both lists (full ownership). tabColumnOrder is wrapped in a
    // (timestamp, list) tuple, and its first item is the Shared("NAME") def;
    // tabColumns then references it back via Ref(2) — exercises token_r and
    // as_list_r's (ts, list) unwrap through the public projection.
    let tab0 = Value::Dict(vec![
        (Value::StrTable(52), Value::Str("Alpha".into())),
        (
            b("tabColumnOrder"),
            Value::Tuple(vec![ts(), Value::List(vec![name_tok_shared, b("TYPE"), b("DISTANCE")])]),
        ),
        (b("tabColumns"), Value::List(vec![Value::Ref(2), b("DISTANCE")])),
    ]);

    // tab 1: fully inherits (no own lists), names preset "P" — name value is
    // Bytes here (the other half of item 3's Str/Bytes coverage).
    let tab1 = Value::Dict(vec![
        (Value::StrTable(52), b("Beta")),
        (b("overview"), b("P")),
    ]);

    // tab 2: owns ONLY tabColumnOrder — the visible half must fall back to
    // the preset (item 5, order-only half missing).
    let tab2 = Value::Dict(vec![
        (Value::Str("name".into()), Value::Str("Gamma".into())),
        (b("overview"), b("P")),
        (b("tabColumnOrder"), Value::List(vec![b("NAME"), b("TYPE"), b("DISTANCE")])),
    ]);

    // tab 3: owns ONLY tabColumns — the order half must fall back to the
    // preset (item 5, visible-only half missing).
    let tab3 = Value::Dict(vec![
        (Value::Str("name".into()), Value::Str("Delta".into())),
        (b("overview"), b("P")),
        (b("tabColumns"), Value::List(vec![b("TYPE")])),
    ]);

    let tabs = Value::Tuple(vec![
        ts(),
        Value::Dict(vec![
            (Value::Int(0), tab0),
            (Value::Int(1), tab1),
            (Value::Int(2), tab2),
            (Value::Int(3), tab3),
        ]),
    ]);

    // Preset "P": its columns list contains a Ref item (Ref(2) -> "NAME"),
    // plus a plain literal token — item 4's "with Ref items" requirement.
    let preset_p = Value::Dict(vec![(
        b("overviewColumns"),
        Value::List(vec![Value::Ref(2), b("ICON")]),
    )]);
    let presets = Value::Dict(vec![(b("P"), preset_p)]);

    // Two windows grouping four tabs: [[0,1],[2,3]].
    let windows = Value::List(vec![
        Value::List(vec![Value::Int(0), Value::Int(1)]),
        Value::List(vec![Value::Int(2), Value::Int(3)]),
    ]);

    // NOTE on encode ordering: `tabsettings_new` (which defines Shared slot
    // 2 inside tab0) must be emitted before `overviewProfilePresets` (which
    // Refs slot 2) — blue-marshal's encoder requires a Ref's Shared to have
    // already been stored, and Dict entries encode in vector order.
    let overview_container = Value::Dict(vec![
        (b("tabsettings_new"), tabs),
        (b("overviewProfilePresets"), presets),
        (b("tabsByWindowInstanceID"), windows),
    ]);

    // The `overview` container's KEY is a bare Ref(1); the matching
    // Shared(Bytes("overview")) is defined elsewhere in the tree (a sibling
    // field), not as this same key occurrence. (Slot 1, not an arbitrary
    // number — see the slot-2 comment above for why.)
    let shared_overview_name = Value::Shared { slot: 1, value: Box::new(b("overview")) };
    Value::Dict(vec![
        (Value::Int(999), shared_overview_name), // elsewhere: defines slot 1, unrelated field
        (Value::Ref(1), overview_container),
    ])
}

/// A legacy (`tabsettings`) tree whose `overview` container KEY is itself a
/// `Shared` (not a bare Ref) — covers item 1's second case and item 2's
/// legacy tab-container key.
fn legacy_shared_keyed_tree() -> Value {
    let tab0 = Value::Dict(vec![
        (Value::StrTable(52), Value::Str("Echo".into())),
        (b("tabColumnOrder"), Value::List(vec![b("NAME")])),
        (b("tabColumns"), Value::List(vec![b("NAME")])),
    ]);
    let overview_container = Value::Dict(vec![(
        b("tabsettings"),
        Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab0)])]),
    )]);
    Value::Dict(vec![(
        Value::Shared { slot: 1, value: Box::new(b("overview")) },
        overview_container,
    )])
}

#[test]
fn modern_ref_keyed_tree_round_trips_and_projects() {
    let tree = modern_ref_keyed_tree();
    let bytes = encode(&tree).expect("fully synthetic fixture must encode");
    let decoded = decode(&bytes).expect("must decode back");

    let oc = project_overview(&decoded, None);

    // item 6: window grouping from tabsByWindowInstanceID.
    assert_eq!(oc.windows.len(), 2, "two windows from [[0,1],[2,3]]");
    assert_eq!(oc.windows[0].tab_indices, vec![0, 1]);
    assert_eq!(oc.windows[1].tab_indices, vec![2, 3]);

    // item 1 (Ref-keyed container path): tabs found at all proves the
    // container was located through the bare Ref key.
    assert_eq!(oc.tabs.len(), 4, "all 4 tabs projected through the Ref-keyed container");

    // tab 0: full ownership; name via StrTable(52)+Str; (ts,list) wrapper and
    // Shared/Ref column tokens resolved (item 3 Str-value half, item 7).
    let t0 = oc.tabs.iter().find(|t| t.index == 0).unwrap();
    assert_eq!(t0.name, "Alpha");
    assert!(!t0.inherits, "tab 0 owns both lists");
    let names0: Vec<&str> = t0.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names0, vec!["NAME", "TYPE", "DISTANCE"], "order from the (ts,list)-wrapped list");
    assert!(t0.columns[0].visible, "NAME visible (resolved through Ref(2) -> Shared(\"NAME\"))");
    assert!(!t0.columns[1].visible, "TYPE not in tabColumns");
    assert!(t0.columns[2].visible, "DISTANCE visible");

    // tab 1: fully inherits; name via StrTable(52)+Bytes (item 3 Bytes-value
    // half); columns entirely from preset P (item 4, with a Ref item).
    let t1 = oc.tabs.iter().find(|t| t.index == 1).unwrap();
    assert_eq!(t1.name, "Beta");
    assert!(t1.inherits, "tab 1 has no own lists");
    let names1: Vec<&str> = t1.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names1, vec!["NAME", "ICON"], "preset's columns, NAME resolved through a Ref item");
    assert!(t1.columns.iter().all(|c| c.visible), "preset columns are all visible");

    // tab 2: owns ONLY tabColumnOrder — the visible half falls back to the
    // preset; the owned order columns are never dropped, just unvisible if
    // the preset doesn't list them (item 5, order-only).
    let t2 = oc.tabs.iter().find(|t| t.index == 2).unwrap();
    assert_eq!(t2.name, "Gamma");
    assert!(t2.inherits, "missing tabColumns half still counts as inheriting");
    let names2: Vec<&str> = t2.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(names2.contains(&"NAME") && names2.contains(&"TYPE") && names2.contains(&"DISTANCE"),
        "owned order columns are present, not dropped: {names2:?}");
    assert!(names2.contains(&"ICON"), "preset-only visible column ICON still appears: {names2:?}");
    let vis2 = |n: &str| t2.columns.iter().find(|c| c.name == n).unwrap().visible;
    assert!(vis2("NAME") && !vis2("TYPE") && !vis2("DISTANCE") && vis2("ICON"),
        "visible set came from the preset fallback, not the owned order list");

    // tab 3: owns ONLY tabColumns — the order half falls back to the preset;
    // the owned visible column is never silently hidden (item 5, visible-only).
    let t3 = oc.tabs.iter().find(|t| t.index == 3).unwrap();
    assert_eq!(t3.name, "Delta");
    assert!(t3.inherits, "missing tabColumnOrder half still counts as inheriting");
    let names3: Vec<&str> = t3.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(names3.contains(&"NAME") && names3.contains(&"ICON"), "preset order columns present: {names3:?}");
    assert!(names3.contains(&"TYPE"), "owned visible column TYPE still appears: {names3:?}");
    let vis3 = |n: &str| t3.columns.iter().find(|c| c.name == n).unwrap().visible;
    assert!(vis3("TYPE") && !vis3("NAME") && !vis3("ICON"),
        "TYPE (owned tabColumns) stayed visible; preset-only columns did not become visible");
}

#[test]
fn legacy_shared_keyed_tree_round_trips_and_projects() {
    let tree = legacy_shared_keyed_tree();
    let bytes = encode(&tree).expect("fully synthetic fixture must encode");
    let decoded = decode(&bytes).expect("must decode back");

    let oc = project_overview(&decoded, None);

    // item 1 (Shared-keyed container path) + item 2 (legacy `tabsettings` key).
    assert_eq!(oc.tabs.len(), 1, "tab found through the Shared-keyed container and legacy tab-key");
    let t0 = &oc.tabs[0];
    assert_eq!(t0.index, 0);
    assert_eq!(t0.name, "Echo", "name resolved from StrTable(52)+Str, not defaulted to \"Tab 0\"");
    assert!(!t0.inherits);
    assert_eq!(t0.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["NAME"]);
    assert!(t0.columns[0].visible);
}
