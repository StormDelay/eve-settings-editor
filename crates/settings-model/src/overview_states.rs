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

use crate::overview_tabs::{is_b, overview_mut, OverviewTabError};
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
    /// files, and what `set_preset_groups` does for `groups`). Order lists are
    /// a priority sequence and must keep the caller's order.
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

    /// user -> overview -> the four state keys, each a (ts, [int]) tuple.
    /// The order lists carry id 68, which the client stores but never renders.
    fn user_with_states() -> Value {
        let list = |ids: &[i64]| Value::Tuple(vec![
            Value::Long(vec![0u8; 8]),
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
        assert!(matches!(val, Value::Tuple(_)), "the (ts, list) wrapper must be preserved");
    }

    #[test]
    fn absent_keys_are_materialised_on_first_edit() {
        let mut v = user_without_states();
        set_state_list(&mut v, StateList::Background, &[9, 13]).unwrap();
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
}
