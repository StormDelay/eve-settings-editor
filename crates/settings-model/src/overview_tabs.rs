//! Structural authoring for overview tabs: edit the user file's `overview`
//! container — `tabsettings_new` (index-keyed tab dict) and
//! `tabsByWindowInstanceID` (window -> tab indices). Window-id/name keys and
//! tab tokens are `Shared`/`Ref` on real files, so every entry point inlines
//! the whole tree first (drops all sharing) and edits plain values; the app
//! layer reshares before saving. Mirrors stacks.rs / overview.rs.

use blue_marshal::Value;
use serde::Serialize;

use crate::treewalk::{inline_all, Entries};

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum OverviewTabError {
    /// No `overview` container in the file.
    NoOverview,
    /// No tab with this index in `tabsettings_new`.
    UnknownTab { index: i64 },
    /// No overview window at this position in `tabsByWindowInstanceID`.
    UnknownWindow { index: usize },
    /// Refused: would delete the last remaining tab.
    LastTab,
    /// Refused: would remove the last overview window.
    LastWindow,
}

impl std::fmt::Display for OverviewTabError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OverviewTabError::NoOverview => write!(f, "This file has no overview settings."),
            OverviewTabError::UnknownTab { index } => write!(f, "Tab {index} does not exist."),
            OverviewTabError::UnknownWindow { index } => write!(f, "Overview window {index} does not exist."),
            OverviewTabError::LastTab => write!(f, "An overview must keep at least one tab."),
            OverviewTabError::LastWindow => write!(f, "There must be at least one overview window."),
        }
    }
}

pub(crate) fn is_b(k: &Value, name: &[u8]) -> bool {
    matches!(k, Value::Bytes(b) if b.as_slice() == name)
}

pub(crate) fn as_int(v: &Value) -> Option<i64> {
    match v { Value::Int(i) => Some(*i), _ => None }
}

/// Inner dict of a plain (post-inline) value, unwrapping a `(ts, dict)` tuple.
fn dict_inner_mut(v: &mut Value) -> Option<&mut Entries> {
    match v {
        Value::Dict(d) => Some(d),
        Value::Tuple(items) => items.iter_mut().find_map(|e| match e {
            Value::Dict(d) => Some(d),
            _ => None,
        }),
        _ => None,
    }
}

/// Inner list of a plain (post-inline) value, unwrapping a `(ts, list)` tuple.
fn list_inner_mut(v: &mut Value) -> Option<&mut Vec<Value>> {
    match v {
        Value::List(l) => Some(l),
        Value::Tuple(items) => items.iter_mut().find_map(|e| match e {
            Value::List(l) => Some(l),
            _ => None,
        }),
        _ => None,
    }
}

/// Mutable `overview` container dict (tree already inlined).
fn overview_mut(v: &mut Value) -> Result<&mut Entries, OverviewTabError> {
    let Value::Dict(root) = v else { return Err(OverviewTabError::NoOverview) };
    let (_, ov) = root.iter_mut().find(|(k, _)| is_b(k, b"overview")).ok_or(OverviewTabError::NoOverview)?;
    dict_inner_mut(ov).ok_or(OverviewTabError::NoOverview)
}

/// Mutable tab dict under `tabsettings_new`, migrating a legacy `tabsettings`
/// key first (the two are structurally identical; EVE reads `tabsettings_new`).
/// Created empty if neither key exists.
fn tabs_mut(ov: &mut Entries) -> &mut Entries {
    if !ov.iter().any(|(k, _)| is_b(k, b"tabsettings_new")) {
        if let Some((k, _)) = ov.iter_mut().find(|(k, _)| is_b(k, b"tabsettings")) {
            *k = Value::Bytes(b"tabsettings_new".to_vec());
        }
    }
    if !ov.iter().any(|(k, _)| is_b(k, b"tabsettings_new")) {
        ov.push((Value::Bytes(b"tabsettings_new".to_vec()), Value::Dict(Vec::new())));
    }
    let (_, v) = ov.iter_mut().find(|(k, _)| is_b(k, b"tabsettings_new")).unwrap();
    dict_inner_mut(v).expect("tabsettings_new is a dict or (ts,dict)")
}

/// Mutable window-groups list under `tabsByWindowInstanceID`. Created empty if absent.
fn groups_mut(ov: &mut Entries) -> &mut Vec<Value> {
    if !ov.iter().any(|(k, _)| is_b(k, b"tabsByWindowInstanceID")) {
        ov.push((Value::Bytes(b"tabsByWindowInstanceID".to_vec()), Value::List(Vec::new())));
    }
    let (_, v) = ov.iter_mut().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")).unwrap();
    list_inner_mut(v).expect("tabsByWindowInstanceID is a list or (ts,list)")
}

/// Set a tab's name, preserving an existing name entry's value variant (real
/// files store names as Str / StrUcs2 / Bytes), inserting a plain `name` key
/// (unicode-safe `StrUcs2`) if the tab has none. The name KEY may itself be a
/// string-table token (`StrTable(52)`); we match it the same way the reader does.
fn set_name(fields: &mut Entries, name: &str) {
    if let Some((_, val)) = fields.iter_mut().find(|(k, _)| key_is_name(k)) {
        *val = match val {
            Value::Bytes(_) => Value::Bytes(name.as_bytes().to_vec()),
            Value::Str(_) => Value::Str(name.to_string()),
            _ => Value::StrUcs2(name.to_string()),
        };
        return;
    }
    fields.push((Value::Str("name".into()), Value::StrUcs2(name.to_string())));
}

/// True if a dict key is the tab-name key, whether stored as `Str("name")`,
/// `Bytes("name")`, or the string-table token `StrTable(52)` real files use.
fn key_is_name(k: &Value) -> bool {
    match k {
        Value::Str(s) => s == "name",
        Value::Bytes(b) => b.as_slice() == b"name",
        Value::StrTable(52) => true,
        _ => false,
    }
}

pub fn rename_tab(v: &mut Value, tab_idx: i64, name: &str) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    let tabs = tabs_mut(ov);
    let (_, tab) = tabs.iter_mut().find(|(k, _)| as_int(k) == Some(tab_idx))
        .ok_or(OverviewTabError::UnknownTab { index: tab_idx })?;
    let fields = dict_inner_mut(tab).ok_or(OverviewTabError::UnknownTab { index: tab_idx })?;
    set_name(fields, name);
    Ok(())
}

pub fn create_tab(v: &mut Value, window_idx: usize, name: &str, preset: &str) -> Result<i64, OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    if window_idx >= groups_mut(ov).len() {
        return Err(OverviewTabError::UnknownWindow { index: window_idx });
    }
    let new_idx = {
        let tabs = tabs_mut(ov);
        let new_idx = tabs.iter().filter_map(|(k, _)| as_int(k)).max().map(|m| m + 1).unwrap_or(0);
        let tab = Value::Dict(vec![
            (Value::Str("name".into()), Value::StrUcs2(name.to_string())),
            (Value::Bytes(b"overview".to_vec()), Value::Bytes(preset.as_bytes().to_vec())),
        ]);
        tabs.push((Value::Int(new_idx), tab));
        new_idx
    };
    if let Some(inner) = groups_mut(ov).get_mut(window_idx).and_then(list_inner_mut) {
        inner.push(Value::Int(new_idx));
    }
    Ok(new_idx)
}

pub fn delete_tab(v: &mut Value, tab_idx: i64) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    {
        let tabs = tabs_mut(ov);
        if !tabs.iter().any(|(k, _)| as_int(k) == Some(tab_idx)) {
            return Err(OverviewTabError::UnknownTab { index: tab_idx });
        }
        if tabs.len() <= 1 {
            return Err(OverviewTabError::LastTab);
        }
        tabs.retain(|(k, _)| as_int(k) != Some(tab_idx));
    }
    for g in groups_mut(ov).iter_mut() {
        if let Some(inner) = list_inner_mut(g) {
            inner.retain(|e| as_int(e) != Some(tab_idx));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    /// user tree: overview -> tabsettings_new (bare dict) -> {0:{name,overview:"P"}}
    fn user_with_tabs() -> Value {
        let tab = Value::Dict(vec![
            (Value::Str("name".into()), Value::Str("Main".into())),
            (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
        ]);
        let overview = Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()),
             Value::Dict(vec![(Value::Int(0), tab)])),
            (Value::Bytes(b"tabsByWindowInstanceID".to_vec()),
             Value::List(vec![Value::List(vec![Value::Int(0)])])),
        ]);
        Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), overview)])
    }

    fn tab_name(v: &Value, idx: i64) -> String {
        let Value::Dict(root) = v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, tabs) = ovd.iter().find(|(k, _)| is_b(k, b"tabsettings_new")).unwrap();
        let Value::Dict(td) = tabs else { panic!() };
        let (_, tab) = td.iter().find(|(k, _)| as_int(k) == Some(idx)).unwrap();
        let Value::Dict(fields) = tab else { panic!() };
        fields.iter().find_map(|(k, val)| match (k, val) {
            (Value::Str(s), Value::Str(name)) if s == "name" => Some(name.clone()),
            (Value::Str(s), Value::StrUcs2(name)) if s == "name" => Some(name.clone()),
            _ => None,
        }).unwrap()
    }

    fn window_indices(v: &Value, window: usize) -> Vec<i64> {
        let Value::Dict(root) = v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, g) = ovd.iter().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")).unwrap();
        let Value::List(outer) = g else { panic!() };
        let Value::List(inner) = &outer[window] else { panic!() };
        inner.iter().filter_map(as_int).collect()
    }

    #[test]
    fn rename_sets_the_name_field() {
        let mut v = user_with_tabs();
        rename_tab(&mut v, 0, "Combat").unwrap();
        assert_eq!(tab_name(&v, 0), "Combat");
    }

    #[test]
    fn rename_unknown_tab_errors() {
        let mut v = user_with_tabs();
        assert!(matches!(rename_tab(&mut v, 9, "X"), Err(OverviewTabError::UnknownTab { index: 9 })));
    }

    #[test]
    fn create_allocates_next_index_and_joins_the_window() {
        let mut v = user_with_tabs(); // has tab 0 in window 0
        let idx = create_tab(&mut v, 0, "Mining", "P").unwrap();
        assert_eq!(idx, 1, "next free index after 0");
        assert_eq!(tab_name(&v, 1), "Mining");
        assert_eq!(window_indices(&v, 0), vec![0, 1], "appended to window 0's strip");
    }

    #[test]
    fn create_into_missing_window_errors() {
        let mut v = user_with_tabs();
        assert!(matches!(create_tab(&mut v, 5, "X", "P"), Err(OverviewTabError::UnknownWindow { index: 5 })));
    }

    #[test]
    fn delete_removes_tab_and_purges_window_strips() {
        let mut v = user_with_tabs();
        create_tab(&mut v, 0, "Mining", "P").unwrap(); // now tabs 0,1 in window 0
        delete_tab(&mut v, 0).unwrap();
        assert_eq!(window_indices(&v, 0), vec![1], "0 purged from the strip");
        assert!(matches!(rename_tab(&mut v, 0, "X"), Err(OverviewTabError::UnknownTab { index: 0 })),
            "tab 0 is gone from tabsettings_new");
    }

    #[test]
    fn delete_last_tab_is_refused() {
        let mut v = user_with_tabs(); // single tab 0
        assert!(matches!(delete_tab(&mut v, 0), Err(OverviewTabError::LastTab)));
    }
}
