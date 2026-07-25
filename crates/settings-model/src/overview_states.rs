//! Structural authoring for the account-scoped overview *state* settings: which
//! pilot states tint an overview row (`backgroundStates2`) or carry a colortag
//! (`flagStates2`), the priority order of each (`backgroundOrder2` /
//! `flagOrder2`), the sparse per-state colour overrides (`stateColors`), and the
//! container's boolean settings. All live directly in the user file's `overview`
//! container. Edits use the same inline-first idiom as `overview_tabs.rs` and
//! reuse its `pub(crate)` helpers; the app layer reshares before saving.
//!
//! The enabled lists and the order lists are INDEPENDENT: an order list
//! enumerates every state the client knows regardless of whether it is ticked,
//! and can contain an id the client never renders (id 68 on current files), so
//! writes must preserve unknown ids rather than rebuild from what is on screen.

use blue_marshal::Value;

use crate::overview_tabs::{dict_inner_mut, is_b, overview_mut, OverviewTabError};
use crate::treewalk::{collect_shared, effective, inline_all, Entries, SharedTable};

/// Which of the four account-scoped state lists to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateList {
    /// `backgroundStates2` — states that tint a row (the ticked subset).
    Background,
    /// `backgroundOrder2` — every known state in row-tint priority order.
    BackgroundOrder,
    /// `flagStates2` — states that carry a colortag (the ticked subset).
    Flag,
    /// `flagOrder2` — every known state in colortag priority order.
    FlagOrder,
}

impl StateList {
    fn key(self) -> &'static [u8] {
        match self {
            StateList::Background => b"backgroundStates2",
            StateList::BackgroundOrder => b"backgroundOrder2",
            StateList::Flag => b"flagStates2",
            StateList::FlagOrder => b"flagOrder2",
        }
    }

    /// Enabled lists are stored sorted ascending (EVE's own convention on real
    /// files, matching the sort `set_preset_groups` does for `groups`). They are
    /// ALSO deduplicated, which `set_preset_groups` does not do: an enabled-states
    /// list is a set, so a duplicate id is meaningless and must not round-trip.
    /// Order lists are a priority sequence and must keep the caller's order.
    fn sorted(self) -> bool {
        matches!(self, StateList::Background | StateList::Flag)
    }
}

/// Replace one of the four account-scoped state lists.
///
/// Preserves an existing `(timestamp, list)` wrapper, and mints one — with a
/// zero `Long`, matching `presets_mut_or_create` — when the key is absent, which
/// is the case on an account that has never customised its overview states.
///
/// The caller owns the contents: `ids` is written as given (modulo the sort for
/// enabled lists), so a caller rebuilding an order list from a UI MUST carry
/// over ids the client does not render, or they are silently dropped.
pub fn set_state_list(v: &mut Value, which: StateList, ids: &[i64]) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;

    let mut out = ids.to_vec();
    if which.sorted() {
        out.sort_unstable();
        out.dedup();
    }
    let list = Value::List(out.into_iter().map(Value::Int).collect());

    match ov.iter_mut().find(|(k, _)| is_b(k, which.key())) {
        // Existing key: replace the inner list, leaving the (ts, _) wrapper.
        Some((_, existing)) => match existing {
            Value::Tuple(items) => {
                match items.iter_mut().find(|e| matches!(e, Value::List(_))) {
                    Some(slot) => *slot = list,
                    None => items.push(list),
                }
            }
            other => *other = list,
        },
        // Absent: mint a fresh (ts, list). EVE re-timestamps on its next save.
        None => ov.push((
            Value::Bytes(which.key().to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), list]),
        )),
    }
    Ok(())
}

/// The surface component of a `stateColors` key. Only this surface is edited;
/// any other is read past and written back untouched.
const BACKGROUND_SURFACE: &[u8] = b"background";

/// Key match for the READ path. The write path inlines the tree first and can
/// use `is_b`, which matches bare `Bytes`; a read cannot — real files store a
/// repeated byte string once and `Ref` it everywhere else (so does our own
/// `reshare` pass), and an unresolved `Ref` silently matches nothing.
fn shared_is_b<'a>(k: &'a Value, name: &[u8], sh: &SharedTable<'a>) -> bool {
    matches!(effective(k, sh), Value::Bytes(b) if b.as_slice() == name)
}

/// The `overview` container's entries, resolving indirection at every hop.
fn overview_entries<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<&'a Entries> {
    let Value::Dict(root) = effective(v, sh) else { return None };
    let (_, ov) = root.iter().find(|(k, _)| shared_is_b(k, b"overview", sh))?;
    match effective(ov, sh) {
        Value::Dict(d) => Some(d),
        _ => None,
    }
}

fn as_f64<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<f64> {
    match effective(v, sh) {
        Value::Float(f) => Some(*f),
        Value::Int(i) => Some(*i as f64),
        _ => None,
    }
}

/// Read a `(surface, id)` colour key, returning the id only for the background
/// surface. On a real file the surface string is stored once and the other keys
/// carry a `Ref` to it, so both parts go through `effective`.
fn background_color_id<'a>(k: &'a Value, sh: &SharedTable<'a>) -> Option<i64> {
    let Value::Tuple(parts) = effective(k, sh) else { return None };
    let [surface, id] = parts.as_slice() else { return None };
    match (effective(surface, sh), effective(id, sh)) {
        (Value::Bytes(s), Value::Int(n)) if s.as_slice() == BACKGROUND_SURFACE => Some(*n),
        _ => None,
    }
}

fn as_rgba<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<[f64; 4]> {
    let Value::Tuple(parts) = effective(v, sh) else { return None };
    let [r, g, b, a] = parts.as_slice() else { return None };
    Some([as_f64(r, sh)?, as_f64(g, sh)?, as_f64(b, sh)?, as_f64(a, sh)?])
}

/// Every background-surface colour override in the file, as `(state_id, rgba)`.
/// SPARSE: a state absent from this list uses EVE's built-in default colour.
pub fn state_colors(v: &Value) -> Vec<(i64, [f64; 4])> {
    let mut sh = SharedTable::new();
    collect_shared(v, &mut sh);
    let Some(ovd) = overview_entries(v, &sh) else { return Vec::new() };
    let Some((_, sc)) = ovd.iter().find(|(k, _)| shared_is_b(k, b"stateColors", &sh)) else {
        return Vec::new();
    };
    let inner = match effective(sc, &sh) {
        Value::Dict(d) => Some(d),
        Value::Tuple(items) => items.iter().find_map(|e| match effective(e, &sh) {
            Value::Dict(d) => Some(d),
            _ => None,
        }),
        _ => None,
    };
    let Some(d) = inner else { return Vec::new() };
    d.iter()
        .filter_map(|(k, val)| Some((background_color_id(k, &sh)?, as_rgba(val, &sh)?)))
        .collect()
}

/// Set or clear one state's background colour.
///
/// `Some(rgba)` writes an explicit override; `None` REMOVES the entry, which is
/// how the UI restores EVE's built-in default for that state — writing an
/// explicit default-looking colour is not the same thing.
///
/// Entries whose surface is not `background` are left exactly as found.
pub fn set_state_color(v: &mut Value, id: i64, rgba: Option<[f64; 4]>) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;

    if !ov.iter().any(|(k, _)| is_b(k, b"stateColors")) {
        if rgba.is_none() {
            return Ok(()); // nothing stored, nothing to clear
        }
        ov.push((
            Value::Bytes(b"stateColors".to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Dict(Vec::new())]),
        ));
    }
    let (_, sc) = ov.iter_mut().find(|(k, _)| is_b(k, b"stateColors")).expect("just checked");
    let entries = dict_inner_mut(sc).ok_or(OverviewTabError::NoOverview)?;

    // `inline_all` above dropped every Shared/Ref, so the write path resolves
    // against an empty slot table.
    let flat = SharedTable::new();
    match rgba {
        None => entries.retain(|(k, _)| background_color_id(k, &flat) != Some(id)),
        Some([r, g, b_, a]) => {
            let val = Value::Tuple(vec![
                Value::Float(r), Value::Float(g), Value::Float(b_), Value::Float(a),
            ]);
            match entries.iter_mut().find(|(k, _)| background_color_id(k, &flat) == Some(id)) {
                Some((_, slot)) => *slot = val,
                None => entries.push((
                    Value::Tuple(vec![Value::Bytes(BACKGROUND_SURFACE.to_vec()), Value::Int(id)]),
                    val,
                )),
            }
        }
    }
    Ok(())
}

/// The overview container's simple boolean settings, as EVE's own Overview
/// Settings window exposes them. Deliberately excludes the
/// `showCategoryInTargetRange_<id>` family, which is keyed by inventory category
/// and needs group naming to present.
pub const OVERVIEW_BOOLS: [&str; 6] = [
    "applyToStructures",
    "applyToOtherObjects",
    "useSmallColorTags",
    "useSmallText",
    "overviewBroadcastsToTop",
    "hideCorpTicker",
];

fn as_bool<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<bool> {
    match effective(v, sh) {
        Value::Bool(b) => Some(*b),
        Value::Tuple(items) => items.iter().find_map(|e| match effective(e, sh) {
            Value::Bool(b) => Some(*b),
            _ => None,
        }),
        _ => None,
    }
}

/// The known boolean settings actually present in the file. A setting absent
/// here is one EVE has never written; the UI shows it unticked.
pub fn overview_bools(v: &Value) -> Vec<(String, bool)> {
    let mut sh = SharedTable::new();
    collect_shared(v, &mut sh);
    let Some(ovd) = overview_entries(v, &sh) else { return Vec::new() };
    OVERVIEW_BOOLS
        .iter()
        .filter_map(|name| {
            let (_, val) = ovd.iter().find(|(k, _)| shared_is_b(k, name.as_bytes(), &sh))?;
            Some(((*name).to_string(), as_bool(val, &sh)?))
        })
        .collect()
}

/// Set one of the overview container's boolean settings. Preserves an existing
/// `(timestamp, bool)` wrapper and mints one — with a zero `Long`, matching the
/// rest of this module — when the key is absent.
///
/// `key` is validated against `OVERVIEW_BOOLS` so a typo cannot mint a junk key
/// into a file the client reads.
pub fn set_overview_bool(v: &mut Value, key: &str, on: bool) -> Result<(), OverviewTabError> {
    if !OVERVIEW_BOOLS.contains(&key) {
        return Err(OverviewTabError::UnknownSetting { key: key.to_string() });
    }
    inline_all(v);
    let ov = overview_mut(v)?;

    match ov.iter_mut().find(|(k, _)| is_b(k, key.as_bytes())) {
        Some((_, existing)) => match existing {
            Value::Tuple(items) => match items.iter_mut().find(|e| matches!(e, Value::Bool(_))) {
                Some(slot) => *slot = Value::Bool(on),
                None => items.push(Value::Bool(on)),
            },
            other => *other = Value::Bool(on),
        },
        None => ov.push((
            Value::Bytes(key.as_bytes().to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Bool(on)]),
        )),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }

    fn ints(v: &Value) -> Vec<i64> {
        let inner = match v {
            Value::List(l) => l,
            Value::Tuple(items) => match items.iter().find(|e| matches!(e, Value::List(_))) {
                Some(Value::List(l)) => l,
                _ => return Vec::new(),
            },
            _ => return Vec::new(),
        };
        inner.iter().filter_map(|e| if let Value::Int(n) = e { Some(*n) } else { None }).collect()
    }

    fn read(v: &Value, key: &str) -> Vec<i64> {
        let Value::Dict(root) = v else { return Vec::new() };
        let Some((_, ov)) = root.iter().find(|(k, _)| is_b(k, b"overview")) else { return Vec::new() };
        let Value::Dict(ovd) = ov else { return Vec::new() };
        ovd.iter().find(|(k, _)| is_b(k, key.as_bytes())).map(|(_, v)| ints(v)).unwrap_or_default()
    }

    /// A distinguishable non-zero timestamp, so a test can tell "the original was
    /// preserved" apart from "the code invented a fresh zero one".
    fn seeded_ts() -> Value { Value::Long(vec![7, 0, 0, 0, 0, 0, 0, 0]) }

    /// user -> overview -> the four state keys, each a (ts, [int]) tuple.
    /// The order lists carry id 68, which the client stores but never renders.
    fn user_with_states() -> Value {
        let list = |ids: &[i64]| Value::Tuple(vec![
            seeded_ts(),
            Value::List(ids.iter().map(|n| Value::Int(*n)).collect()),
        ]);
        Value::Dict(vec![(b("overview"), Value::Dict(vec![
            (b("backgroundStates2"), list(&[9, 13, 44])),
            (b("backgroundOrder2"), list(&[13, 44, 9, 68])),
            (b("flagStates2"), list(&[9, 13])),
            (b("flagOrder2"), list(&[13, 9, 44, 68])),
        ]))])
    }

    /// A clean account: an overview container with no state keys at all.
    fn user_without_states() -> Value {
        Value::Dict(vec![(b("overview"), Value::Dict(vec![
            (b("tabsettings_new"), Value::Dict(Vec::new())),
        ]))])
    }

    #[test]
    fn enabled_list_is_written_sorted() {
        let mut v = user_with_states();
        set_state_list(&mut v, StateList::Background, &[44, 9, 13]).unwrap();
        assert_eq!(read(&v, "backgroundStates2"), vec![9, 13, 44]);
    }

    #[test]
    fn order_list_keeps_caller_order() {
        let mut v = user_with_states();
        set_state_list(&mut v, StateList::BackgroundOrder, &[44, 9, 68, 13]).unwrap();
        assert_eq!(read(&v, "backgroundOrder2"), vec![44, 9, 68, 13]);
    }

    #[test]
    fn flag_lists_are_independent_of_background() {
        let mut v = user_with_states();
        set_state_list(&mut v, StateList::Flag, &[44]).unwrap();
        assert_eq!(read(&v, "flagStates2"), vec![44]);
        assert_eq!(read(&v, "backgroundStates2"), vec![9, 13, 44], "background untouched");
    }

    #[test]
    fn timestamp_wrapper_survives_the_edit() {
        let mut v = user_with_states();
        set_state_list(&mut v, StateList::Background, &[9]).unwrap();
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, val) = ovd.iter().find(|(k, _)| is_b(k, b"backgroundStates2")).unwrap();
        let Value::Tuple(items) = val else { panic!("the (ts, list) wrapper must be preserved") };
        let ts = items.iter().find(|e| matches!(e, Value::Long(_))).expect("a Long timestamp element");
        assert_eq!(ts, &seeded_ts(), "the ORIGINAL timestamp must survive the edit, not be replaced");
    }

    #[test]
    fn absent_keys_are_materialised_on_first_edit() {
        let mut v = user_without_states();
        set_state_list(&mut v, StateList::Background, &[9, 13]).unwrap();
        assert_eq!(read(&v, "backgroundStates2"), vec![9, 13]);

        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, val) = ovd.iter().find(|(k, _)| is_b(k, b"backgroundStates2")).unwrap();
        assert_eq!(
            val,
            &Value::Tuple(vec![
                Value::Long(vec![0u8; 8]),
                Value::List(vec![Value::Int(9), Value::Int(13)]),
            ]),
            "a freshly minted key must be a (zero Long timestamp, list) tuple, not a bare list"
        );
    }

    #[test]
    fn enabled_list_dedups_duplicate_ids() {
        let mut v = user_with_states();
        set_state_list(&mut v, StateList::Background, &[9, 9, 13, 9]).unwrap();
        assert_eq!(read(&v, "backgroundStates2"), vec![9, 13]);
    }

    #[test]
    fn unrendered_id_68_survives_a_toggle() {
        let mut v = user_with_states();
        // Enabling one more state must not disturb the order list that holds 68.
        set_state_list(&mut v, StateList::Background, &[9, 13, 44, 52]).unwrap();
        assert!(read(&v, "backgroundOrder2").contains(&68), "id 68 must round-trip");
    }

    #[test]
    fn no_overview_container_is_an_error() {
        let mut v = Value::Dict(vec![(b("ui"), Value::Dict(Vec::new()))]);
        assert!(matches!(
            set_state_list(&mut v, StateList::Background, &[9]),
            Err(OverviewTabError::NoOverview)
        ));
    }

    fn rgba(r: f64, g: f64, bl: f64, a: f64) -> Value {
        Value::Tuple(vec![Value::Float(r), Value::Float(g), Value::Float(bl), Value::Float(a)])
    }

    fn color_key(surface: &str, id: i64) -> Value {
        Value::Tuple(vec![b(surface), Value::Int(id)])
    }

    /// user -> overview -> stateColors: (ts, { ("background", id): (r,g,b,a) })
    /// Includes one entry on a foreign surface, which must never be touched.
    /// The timestamp is the shared `seeded_ts()` (not a zero Long), so a test
    /// can tell "the original was preserved" apart from "a fresh one was minted".
    fn user_with_colors() -> Value {
        Value::Dict(vec![(b("overview"), Value::Dict(vec![
            (b("stateColors"), Value::Tuple(vec![
                seeded_ts(),
                Value::Dict(vec![
                    (color_key("background", 44), rgba(0.75, 0.0, 0.0, 1.0)),
                    (color_key("background", 20), rgba(0.7, 0.7, 0.7, 0.5)),
                    (color_key("bracket", 44), rgba(0.1, 0.2, 0.3, 1.0)),
                ]),
            ])),
        ]))])
    }

    #[test]
    fn projects_only_the_background_surface() {
        let v = user_with_colors();
        let mut got = state_colors(&v);
        got.sort_by_key(|(id, _)| *id);
        assert_eq!(got, vec![(20, [0.7, 0.7, 0.7, 0.5]), (44, [0.75, 0.0, 0.0, 1.0])]);
    }

    /// The shape every REAL file has: `b"background"` is stored once and every
    /// later colour key carries a `Ref` to it, and a colour repeated across two
    /// states is stored once too. Reading these as bare `Bytes`/`Tuple` finds
    /// nothing, which showed up in the live smoke as "no state has a colour".
    fn user_with_shared_colors() -> Value {
        let dict = Value::Dict(vec![
            (
                Value::Tuple(vec![
                    Value::Shared { slot: 1, value: Box::new(b("background")) },
                    Value::Int(10),
                ]),
                Value::Shared { slot: 2, value: Box::new(rgba(0.7, 0.7, 0.7, 1.0)) },
            ),
            (
                Value::Tuple(vec![Value::Ref(1), Value::Int(12)]),
                Value::Ref(2),
            ),
        ]);
        Value::Dict(vec![(
            Value::Shared { slot: 3, value: Box::new(b("overview")) },
            Value::Dict(vec![(b("stateColors"), Value::Tuple(vec![seeded_ts(), dict]))]),
        )])
    }

    #[test]
    fn resolves_shared_and_ref_colour_keys_and_values() {
        let mut got = state_colors(&user_with_shared_colors());
        got.sort_by_key(|(id, _)| *id);
        assert_eq!(got, vec![(10, [0.7, 0.7, 0.7, 1.0]), (12, [0.7, 0.7, 0.7, 1.0])]);
    }

    #[test]
    fn resolves_a_ref_boolean_key() {
        // The key is stored in a sibling subtree and `Ref`d inside `overview`.
        let v = Value::Dict(vec![
            (b("restoreData"), Value::List(vec![Value::Shared {
                slot: 7,
                value: Box::new(b("useSmallText")),
            }])),
            (b("overview"), Value::Dict(vec![(
                Value::Ref(7),
                Value::Tuple(vec![seeded_ts(), Value::Bool(true)]),
            )])),
        ]);
        assert_eq!(overview_bools(&v), vec![("useSmallText".to_string(), true)]);
    }

    #[test]
    fn sets_a_colour_for_a_state_with_no_entry() {
        let mut v = user_with_colors();
        set_state_color(&mut v, 13, Some([1.0, 0.0, 0.0, 1.0])).unwrap();
        assert!(state_colors(&v).contains(&(13, [1.0, 0.0, 0.0, 1.0])));
    }

    #[test]
    fn overwrites_an_existing_colour() {
        let mut v = user_with_colors();
        set_state_color(&mut v, 44, Some([0.0, 1.0, 0.0, 1.0])).unwrap();
        assert!(state_colors(&v).contains(&(44, [0.0, 1.0, 0.0, 1.0])));
        assert_eq!(state_colors(&v).iter().filter(|(id, _)| *id == 44).count(), 1);
    }

    #[test]
    fn none_removes_the_entry_restoring_eves_default() {
        let mut v = user_with_colors();
        set_state_color(&mut v, 44, None).unwrap();
        assert!(!state_colors(&v).iter().any(|(id, _)| *id == 44));
    }

    #[test]
    fn removing_an_absent_entry_is_a_no_op() {
        let mut v = user_with_colors();
        set_state_color(&mut v, 13, None).unwrap();
        assert_eq!(state_colors(&v).len(), 2);
    }

    #[test]
    fn a_foreign_surface_entry_is_preserved() {
        let mut v = user_with_colors();
        set_state_color(&mut v, 44, Some([0.0, 0.0, 1.0, 1.0])).unwrap();
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, sc) = ovd.iter().find(|(k, _)| is_b(k, b"stateColors")).unwrap();
        let Value::Tuple(items) = sc else { panic!() };
        let Some(Value::Dict(d)) = items.iter().find(|e| matches!(e, Value::Dict(_))) else { panic!() };
        assert!(
            d.iter().any(|(k, _)| *k == color_key("bracket", 44)),
            "a non-background surface must round-trip untouched"
        );
    }

    #[test]
    fn colours_can_be_set_when_the_key_is_absent() {
        let mut v = user_without_states();
        set_state_color(&mut v, 13, Some([1.0, 0.0, 0.0, 1.0])).unwrap();
        assert_eq!(state_colors(&v), vec![(13, [1.0, 0.0, 0.0, 1.0])]);
    }

    #[test]
    fn color_timestamp_wrapper_survives_the_edit() {
        let mut v = user_with_colors();
        set_state_color(&mut v, 44, Some([0.0, 1.0, 0.0, 1.0])).unwrap();
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, val) = ovd.iter().find(|(k, _)| is_b(k, b"stateColors")).unwrap();
        let Value::Tuple(items) = val else { panic!("the (ts, dict) wrapper must be preserved") };
        let ts = items.iter().find(|e| matches!(e, Value::Long(_))).expect("a Long timestamp element");
        assert_eq!(ts, &seeded_ts(), "the ORIGINAL timestamp must survive the edit, not be replaced");
    }

    #[test]
    fn set_state_color_with_no_overview_container_is_an_error() {
        let mut v = Value::Dict(vec![(b("ui"), Value::Dict(Vec::new()))]);
        assert!(matches!(
            set_state_color(&mut v, 9, Some([0.0, 0.0, 0.0, 1.0])),
            Err(OverviewTabError::NoOverview)
        ));
    }

    /// A malformed `stateColors` value (neither a dict nor a (ts, dict) tuple)
    /// must be reported as an error, not silently treated as a no-op success.
    #[test]
    fn malformed_state_colors_value_is_an_error() {
        let mut v = Value::Dict(vec![(b("overview"), Value::Dict(vec![
            (b("stateColors"), Value::Int(1)),
        ]))]);
        assert!(matches!(
            set_state_color(&mut v, 9, Some([0.0, 0.0, 0.0, 1.0])),
            Err(OverviewTabError::NoOverview)
        ));
    }

    /// State 20's fixture entry already carries a non-1.0 alpha (0.5). An RGB-only
    /// edit passes that same alpha back through explicitly; the write path must
    /// use exactly what the caller supplied, not silently reset it to 1.0.
    #[test]
    fn editing_rgb_preserves_a_non_default_alpha() {
        let mut v = user_with_colors();
        set_state_color(&mut v, 20, Some([0.1, 0.2, 0.3, 0.5])).unwrap();
        assert!(state_colors(&v).contains(&(20, [0.1, 0.2, 0.3, 0.5])));
    }

    /// user -> overview -> a few boolean settings as (ts, bool) tuples.
    /// The timestamp is the shared `seeded_ts()` (not a zero Long), so a test
    /// can tell "the original was preserved" apart from "a fresh one was minted".
    fn user_with_bools() -> Value {
        let flag = |on: bool| Value::Tuple(vec![seeded_ts(), Value::Bool(on)]);
        Value::Dict(vec![(b("overview"), Value::Dict(vec![
            (b("applyToStructures"), flag(true)),
            (b("applyToOtherObjects"), flag(false)),
            (b("useSmallText"), flag(false)),
        ]))])
    }

    #[test]
    fn projects_the_boolean_settings_present_in_the_file() {
        let mut got = overview_bools(&user_with_bools());
        got.sort();
        assert_eq!(got, vec![
            ("applyToOtherObjects".to_string(), false),
            ("applyToStructures".to_string(), true),
            ("useSmallText".to_string(), false),
        ]);
    }

    #[test]
    fn sets_an_existing_boolean() {
        let mut v = user_with_bools();
        set_overview_bool(&mut v, "applyToOtherObjects", true).unwrap();
        assert!(overview_bools(&v).contains(&("applyToOtherObjects".to_string(), true)));
    }

    #[test]
    fn bool_timestamp_wrapper_survives_the_edit() {
        let mut v = user_with_bools();
        set_overview_bool(&mut v, "applyToStructures", false).unwrap();
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, val) = ovd.iter().find(|(k, _)| is_b(k, b"applyToStructures")).unwrap();
        let Value::Tuple(items) = val else { panic!("the (ts, bool) wrapper must be preserved") };
        let ts = items.iter().find(|e| matches!(e, Value::Long(_))).expect("a Long timestamp element");
        assert_eq!(ts, &seeded_ts(), "the ORIGINAL timestamp must survive the edit, not be replaced");
    }

    #[test]
    fn materialises_a_known_boolean_that_is_absent() {
        let mut v = user_with_bools();
        set_overview_bool(&mut v, "hideCorpTicker", true).unwrap();
        assert!(overview_bools(&v).contains(&("hideCorpTicker".to_string(), true)));

        // `as_bool` also accepts a bare `Value::Bool` (real files use both shapes
        // for OTHER settings), so the projection check above would not by itself
        // notice a missing `(ts, _)` wrapper. Assert the raw shape explicitly.
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, val) = ovd.iter().find(|(k, _)| is_b(k, b"hideCorpTicker")).unwrap();
        assert_eq!(
            val,
            &Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Bool(true)]),
            "a freshly minted key must be a (zero Long timestamp, bool) tuple, not a bare bool"
        );
    }

    #[test]
    fn rejects_a_key_outside_the_allow_list() {
        let mut v = user_with_bools();
        assert!(matches!(
            set_overview_bool(&mut v, "applyToStructuresTypo", true),
            Err(OverviewTabError::UnknownSetting { key }) if key == "applyToStructuresTypo"
        ));
        assert_eq!(overview_bools(&v).len(), 3, "nothing was minted");
        // Check the raw container too: `overview_bools` only ever projects
        // allow-listed names, so it would not notice a junk key minted alongside
        // them. Confirm the entry count in the raw dict is unchanged as well.
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        assert_eq!(ovd.len(), 3, "no junk key was pushed into the raw overview container");
    }
}
