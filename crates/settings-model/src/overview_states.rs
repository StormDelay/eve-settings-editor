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
use crate::treewalk::inline_all;

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

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Float(f) => Some(*f),
        Value::Int(i) => Some(*i as f64),
        _ => None,
    }
}

/// Read a `(surface, id)` colour key, returning the id only for the background
/// surface.
fn background_color_id(k: &Value) -> Option<i64> {
    let Value::Tuple(parts) = k else { return None };
    let [surface, id] = parts.as_slice() else { return None };
    match (surface, id) {
        (Value::Bytes(s), Value::Int(n)) if s.as_slice() == BACKGROUND_SURFACE => Some(*n),
        _ => None,
    }
}

fn as_rgba(v: &Value) -> Option<[f64; 4]> {
    let Value::Tuple(parts) = v else { return None };
    let [r, g, b, a] = parts.as_slice() else { return None };
    Some([as_f64(r)?, as_f64(g)?, as_f64(b)?, as_f64(a)?])
}

/// Every background-surface colour override in the file, as `(state_id, rgba)`.
/// SPARSE: a state absent from this list uses EVE's built-in default colour.
pub fn state_colors(v: &Value) -> Vec<(i64, [f64; 4])> {
    let Value::Dict(root) = v else { return Vec::new() };
    let Some((_, ov)) = root.iter().find(|(k, _)| is_b(k, b"overview")) else { return Vec::new() };
    let Value::Dict(ovd) = ov else { return Vec::new() };
    let Some((_, sc)) = ovd.iter().find(|(k, _)| is_b(k, b"stateColors")) else { return Vec::new() };
    let inner = match sc {
        Value::Dict(d) => Some(d),
        Value::Tuple(items) => items.iter().find_map(|e| match e {
            Value::Dict(d) => Some(d),
            _ => None,
        }),
        _ => None,
    };
    let Some(d) = inner else { return Vec::new() };
    d.iter()
        .filter_map(|(k, val)| Some((background_color_id(k)?, as_rgba(val)?)))
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

    match rgba {
        None => entries.retain(|(k, _)| background_color_id(k) != Some(id)),
        Some([r, g, b_, a]) => {
            let val = Value::Tuple(vec![
                Value::Float(r), Value::Float(g), Value::Float(b_), Value::Float(a),
            ]);
            match entries.iter_mut().find(|(k, _)| background_color_id(k) == Some(id)) {
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
}
