//! Real-idiom guard for the pack READ path. On real `core_user` files every
//! repeated value is interned as `Shared`/`Ref` — the preset field names, the
//! `background` surface string, the state lists. A `read_pack` that matched
//! bare `Value::Bytes`/`Value::List` would pass every hand-built unit test in
//! `overview_pack.rs` and export an EMPTY pack from a real file (the exact bug
//! slice 3 shipped and had to fix). Fully synthetic: no bytes here came from a
//! real file.

use blue_marshal::{decode, encode, Value};
use settings_model::{emit_pack, parse_pack, read_pack};

// NOTE: the pack node type is re-exported as `PackNode`, not `Node` — the crate
// root already exports `projection::Node`. Inside `overview_pack.rs` it is still
// plain `Node`; only the external name is aliased.

fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
// A distinguishable non-zero seed, matching the fixture convention used
// elsewhere in this crate (see overview_pack.rs's own tests): keeps this file
// from silently relying on an all-zero timestamp coincidence.
fn ts() -> Value { Value::Long(vec![7, 0, 0, 0, 0, 0, 0, 0]) }

/// Slots are 1-based and dense over the whole stream (blue-marshal's
/// store-before-ref encode order), so each `Shared` is defined at an earlier
/// unrelated key and referenced later by `Ref` — the shape real files use.
fn realish_user() -> Value {
    let overview = Value::Dict(vec![
        (Value::Int(900), Value::Shared { slot: 1, value: Box::new(Value::List(vec![Value::Int(9), Value::Int(13)])) }),
        (Value::Int(901), Value::Shared { slot: 2, value: Box::new(b("background")) }),
        (Value::Int(902), Value::Shared { slot: 3, value: Box::new(b("groups")) }),
        (b("backgroundStates2"), Value::Tuple(vec![ts(), Value::Ref(1)])),
        (b("overviewProfilePresets"), Value::Tuple(vec![ts(), Value::Dict(vec![
            (b("Friendly"), Value::Dict(vec![
                (Value::Ref(3), Value::List(vec![Value::Int(25)])),
            ])),
        ])])),
        (b("stateColors"), Value::Tuple(vec![ts(), Value::Dict(vec![
            (Value::Tuple(vec![Value::Ref(2), Value::Int(16)]),
             Value::Tuple(vec![Value::Float(0.0), Value::Float(0.15), Value::Float(0.6), Value::Float(1.0)])),
        ])])),
    ]);
    Value::Dict(vec![(b("overview"), overview)])
}

#[test]
fn reads_through_shared_and_ref() {
    // Round-trip through the codec so the tree is exactly what a file holds.
    let doc = decode(&encode(&realish_user()).unwrap()).unwrap();
    let (pack, _) = read_pack(&doc);

    let states = pack.get("backgroundStates").expect("state list read through a Ref");
    assert_eq!(settings_model::PackNode::Seq(vec![settings_model::PackNode::Int(9), settings_model::PackNode::Int(13)]), *states);
    assert!(pack.get("presets").is_some(), "preset field name read through a Ref");
    let colors = pack.get("stateColorsNameList").expect("colour surface read through a Ref");
    assert!(format!("{colors:?}").contains("background_16"));

    // and it still emits valid YAML
    assert!(parse_pack(&emit_pack(&pack)).is_ok());
}

/// `realish_user` above only puts `Shared`/`Ref` indirection inside VALUES and
/// nested dict keys — never on the `overview` container's own SECTION-NAME
/// keys, which is exactly what `overview_entries`/`find` match against via
/// `shared_is_b`. On a real `core_user` file those section keys are interned
/// too: a dump shows entries whose key prints as `shared[255]:b"stateBlinks"`
/// or `shared[319]:b"shipLabels"`, the same string `Ref`'d elsewhere in the
/// file. This fixture puts one section key behind a `Shared` DEFINITION and a
/// different section key behind a `Ref` to a slot stored earlier in the
/// stream — the two shapes a real file's section keys actually take — so a
/// section-key matcher that regresses to bare `Value::Bytes` (no `effective`)
/// finds neither section and gets caught here.
fn realish_user_section_keys() -> Value {
    let overview = Value::Dict(vec![
        // Decoy: stores the "shipLabels" bytes at slot 1 BEFORE anything
        // refs it, mirroring an earlier unrelated occurrence of the same
        // string elsewhere in a real file.
        (Value::Int(900), Value::Shared { slot: 1, value: Box::new(b("shipLabels")) }),
        // stateBlinks: section key reached as a Shared DEFINITION.
        (
            Value::Shared { slot: 2, value: Box::new(b("stateBlinks")) },
            Value::Tuple(vec![ts(), Value::Dict(vec![
                (Value::Tuple(vec![b("flag"), Value::Int(9)]), Value::Bool(true)),
            ])]),
        ),
        // shipLabels: section key reached as a Ref to the earlier slot.
        (
            Value::Ref(1),
            Value::Tuple(vec![ts(), Value::List(vec![
                Value::Dict(vec![(b("type"), b("hull"))]),
            ])]),
        ),
    ]);
    Value::Dict(vec![(b("overview"), overview)])
}

#[test]
fn reads_sections_keyed_by_shared_and_ref() {
    let doc = decode(&encode(&realish_user_section_keys()).unwrap()).unwrap();
    let (pack, _) = read_pack(&doc);

    let blinks = pack
        .get("stateBlinks")
        .expect("stateBlinks section key (a Shared definition) must still be found");
    assert_eq!(
        *blinks,
        settings_model::PackNode::Seq(vec![settings_model::PackNode::Seq(vec![
            settings_model::PackNode::Str("flag_9".into()),
            settings_model::PackNode::Bool(true),
        ])]),
    );

    let order = pack
        .get("shipLabelOrder")
        .expect("shipLabels section key (a Ref) must still be found");
    assert_eq!(
        *order,
        settings_model::PackNode::Seq(vec![settings_model::PackNode::Str("hull".into())]),
    );
    assert!(pack.get("shipLabels").is_some());

    // and it still emits valid YAML
    assert!(parse_pack(&emit_pack(&pack)).is_ok());
}
