//! Real-idiom guard for the overview *state* projection (Task 6 review finding):
//! the account-scoped state lists (`backgroundStates2`/`backgroundOrder2`/
//! `flagStates2`/`flagOrder2`) and a preset's `filteredStates`/`alwaysShownStates`
//! exception lists are interned as `Shared`/`Ref` on real `core_user` files, NOT
//! stored as plain `Value::List`s. A projection that matched `Value::List`
//! directly would pass every hand-built unit test in `overview.rs` (which only
//! ever builds plain lists) while reading nothing from a real file. This file
//! is fully synthetic — no bytes/ids here were read from a real file — and
//! exercises only the read side (no edit), so a plain encode/decode round-trip
//! is the right shape (mirrors `overview_realshape.rs`, not the reshare-after-edit
//! shape used by the presets/tabs realshape files).

use blue_marshal::{decode, encode, Value};
use settings_model::project_overview;

fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
fn ts() -> Value { Value::Long(vec![0u8; 8]) }

fn shared_list(slot: u32, ids: &[i64]) -> Value {
    Value::Shared { slot, value: Box::new(Value::List(ids.iter().map(|n| Value::Int(*n)).collect())) }
}

/// user -> overview -> {
///   900: Shared(1, [9,13])          // elsewhere: defines slot 1, unrelated field
///   901: Shared(2, [13,9,68])       // elsewhere: defines slot 2, unrelated field
///   902: Shared(3, [9,13])          // elsewhere: defines slot 3, unrelated field
///   903: Shared(4, [11])            // elsewhere: defines slot 4, unrelated field
///   backgroundStates2: (ts, Ref(1))
///   backgroundOrder2: (ts, Ref(2))
///   overviewProfilePresets: (ts, { "alpha": { groups:[1], filteredStates: Ref(3), alwaysShownStates: Ref(4) } })
/// }
///
/// Every list a real file interns is here reached only through a `Ref` to a
/// `Shared` defined at an unrelated sibling key (the account-scoped state
/// keys) or as a BARE `Ref` with no `(ts, _)` wrapper at all (the preset's two
/// exception lists) — the two indirection shapes real files actually use.
/// The int placeholder keys (900-903) are junk field names `find_child` never
/// matches by name; they exist purely to register the `Shared` slot BEFORE the
/// `Ref` that resolves it (blue-marshal's store-before-ref encode order), and
/// slots are 1-based and DENSE over the whole stream (not arbitrary numbers —
/// see the same note in `overview_realshape.rs`).
fn realish_user() -> Value {
    let overview = Value::Dict(vec![
        (Value::Int(900), shared_list(1, &[9, 13])),
        (Value::Int(901), shared_list(2, &[13, 9, 68])),
        (Value::Int(902), shared_list(3, &[9, 13])),
        (Value::Int(903), shared_list(4, &[11])),
        (b("backgroundStates2"), Value::Tuple(vec![ts(), Value::Ref(1)])),
        (b("backgroundOrder2"), Value::Tuple(vec![ts(), Value::Ref(2)])),
        (b("overviewProfilePresets"), Value::Tuple(vec![
            ts(),
            Value::Dict(vec![(
                b("alpha"),
                Value::Dict(vec![
                    (b("groups"), Value::List(vec![Value::Int(1)])),
                    (b("filteredStates"), Value::Ref(3)),
                    (b("alwaysShownStates"), Value::Ref(4)),
                ]),
            )]),
        ])),
    ]);
    Value::Dict(vec![(b("overview"), overview)])
}

#[test]
fn state_surfaces_and_preset_exceptions_resolve_through_shared_ref() {
    let tree = realish_user();
    let bytes = encode(&tree).expect("fully synthetic fixture must encode");
    let decoded = decode(&bytes).expect("must decode back");

    let cols = project_overview(&decoded, None);

    // Item 1: the account-scoped background surface, both halves reached
    // through a (ts, Ref) tuple resolving to a Shared defined elsewhere.
    assert_eq!(cols.appearance.background.enabled, vec![9, 13],
        "backgroundStates2 resolved through (ts, Ref(1)) -> Shared(1) registered elsewhere");
    assert_eq!(cols.appearance.background.order, vec![13, 9, 68],
        "backgroundOrder2 resolved through (ts, Ref(2)) -> Shared(2), including the unrendered id 68");
    assert!(!cols.appearance.defaulted, "the state keys were present, even though Ref/Shared-wrapped");

    // Item 2: a preset's filteredStates/alwaysShownStates as BARE Refs (no
    // (ts, _) wrapper) to Shareds defined elsewhere.
    let alpha = cols.presets.iter().find(|p| p.name == "alpha").expect("preset \"alpha\" present");
    assert_eq!(alpha.filtered_states, vec![9, 13],
        "filteredStates resolved through a bare Ref(3) -> Shared(3) registered elsewhere");
    assert_eq!(alpha.always_shown_states, vec![11],
        "alwaysShownStates resolved through a bare Ref(4) -> Shared(4) registered elsewhere");
    assert_eq!(alpha.groups, vec![1]);
}
