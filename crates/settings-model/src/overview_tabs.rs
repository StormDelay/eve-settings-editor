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
    /// Refused: this overview has no window mapping to add onto (windowless account).
    NoWindowMapping,
    /// Refused: only the last overview window can be removed for now.
    NotLastWindow { index: usize },
}

impl std::fmt::Display for OverviewTabError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OverviewTabError::NoOverview => write!(f, "This file has no overview settings."),
            OverviewTabError::UnknownTab { index } => write!(f, "Tab {index} does not exist."),
            OverviewTabError::UnknownWindow { index } => write!(f, "Overview window {index} does not exist."),
            OverviewTabError::LastTab => write!(f, "An overview must keep at least one tab."),
            OverviewTabError::LastWindow => write!(f, "There must be at least one overview window."),
            OverviewTabError::NoWindowMapping => write!(f, "This overview has no window layout to add to."),
            OverviewTabError::NotLastWindow { index } => write!(f, "Only the last overview window can be removed (tried {index})."),
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

/// Read-only counterpart of `list_inner_mut`, unwrapping a `(ts, list)` tuple.
fn list_inner(v: &Value) -> Option<&Vec<Value>> {
    match v {
        Value::List(l) => Some(l),
        Value::Tuple(items) => items.iter().find_map(|e| match e {
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

/// How many overview windows the account's `tabsByWindowInstanceID` maps tabs to
/// (0 when the mapping is absent — a windowless account). Reading it never
/// fabricates the mapping.
fn window_count(ov: &Entries) -> usize {
    ov.iter()
        .find(|(k, _)| is_b(k, b"tabsByWindowInstanceID"))
        .and_then(|(_, wv)| list_inner(wv))
        .map_or(0, |g| g.len())
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

/// Create a new tab by CLONING a sibling (`from_tab`, else the first tab) and
/// overriding its name. Cloning — rather than building a minimal `{name,
/// overview}` dict — is required: every real EVE tab carries `bracket` and
/// `color` keys, and EVE's "reset all overview settings" iterates tabs reading
/// them, so a tab missing them makes the reset throw. The clone also inherits
/// the sibling's preset (`overview`) and its name-key encoding; its column
/// lists are dropped so the new tab inherits columns.
pub fn create_tab(v: &mut Value, window_idx: usize, name: &str, from_tab: Option<i64>) -> Result<i64, OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    // How many overview windows the account maps tabs to. EVE honors an explicit
    // tab->window mapping only when `tabsByWindowInstanceID` exists with ≥1
    // window; absent/empty means EVE distributes tabs across its (char-side)
    // overview windows by default. We must NOT create or touch the mapping in
    // that case — the per-window distribution is char-side state we can't
    // reconstruct here, and a partial/wrong mapping hides the whole overview.
    let window_count = window_count(ov);
    if window_count > 0 && window_idx >= window_count {
        return Err(OverviewTabError::UnknownWindow { index: window_idx });
    }
    let new_idx = {
        let tabs = tabs_mut(ov);
        let new_idx = tabs.iter().filter_map(|(k, _)| as_int(k)).max().map(|m| m + 1).unwrap_or(0);
        let template = from_tab
            .and_then(|t| tabs.iter().position(|(k, _)| as_int(k) == Some(t)))
            .or(if tabs.is_empty() { None } else { Some(0) });
        let mut tab = match template {
            Some(i) => tabs[i].1.clone(),
            // Last-resort tab when there is no sibling to clone (an empty
            // overview — only reachable when the account has no tabs). Still
            // carries bracket/color so the "every created tab is a valid EVE
            // tab" invariant holds on this path too.
            None => Value::Dict(vec![
                (Value::Bytes(b"bracket".to_vec()), Value::Bytes(b"_BracketFilterShowAll".to_vec())),
                (Value::Bytes(b"color".to_vec()), Value::None),
                (Value::Bytes(b"overview".to_vec()), Value::Bytes(Vec::new())),
            ]),
        };
        if let Some(fields) = dict_inner_mut(&mut tab) {
            fields.retain(|(k, _)| !is_b(k, b"tabColumnOrder") && !is_b(k, b"tabColumns"));
            set_name(fields, name);
        }
        tabs.push((Value::Int(new_idx), tab));
        new_idx
    };
    // Attach to the named window ONLY when the account has an explicit mapping;
    // otherwise the tab lives in tabsettings_new and EVE shows it by default.
    if window_count > 0 {
        if let Some((_, wv)) = ov.iter_mut().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")) {
            if let Some(inner) = list_inner_mut(wv).and_then(|g| g.get_mut(window_idx)).and_then(list_inner_mut) {
                inner.push(Value::Int(new_idx));
            }
        }
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
    // Purge the index from every window strip — but only if a mapping already
    // exists. Do NOT fabricate an (empty) `tabsByWindowInstanceID` when it's
    // absent: EVE keys the overview off it, and an empty mapping can hide the
    // whole overview (matches create_tab's no-fabricate behavior).
    if let Some((_, wv)) = ov.iter_mut().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")) {
        if let Some(groups) = list_inner_mut(wv) {
            for g in groups.iter_mut() {
                if let Some(inner) = list_inner_mut(g) {
                    inner.retain(|e| as_int(e) != Some(tab_idx));
                }
            }
        }
    }
    Ok(())
}

pub fn reorder_tabs_in_window(v: &mut Value, window_idx: usize, order: &[i64]) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    let inner = groups_mut(ov).get_mut(window_idx).and_then(list_inner_mut)
        .ok_or(OverviewTabError::UnknownWindow { index: window_idx })?;
    *inner = order.iter().map(|&i| Value::Int(i)).collect();
    Ok(())
}

pub fn move_tab(v: &mut Value, tab_idx: i64, from_window: usize, to_window: usize, pos: usize) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    // Validate the destination window exists BEFORE mutating the source strip,
    // so an invalid to_window can't remove the tab from both windows.
    if groups_mut(ov).get_mut(to_window).and_then(list_inner_mut).is_none() {
        return Err(OverviewTabError::UnknownWindow { index: to_window });
    }
    {
        let src = groups_mut(ov).get_mut(from_window).and_then(list_inner_mut)
            .ok_or(OverviewTabError::UnknownWindow { index: from_window })?;
        src.retain(|e| as_int(e) != Some(tab_idx));
    }
    let dst = groups_mut(ov).get_mut(to_window).and_then(list_inner_mut)
        .ok_or(OverviewTabError::UnknownWindow { index: to_window })?;
    let at = pos.min(dst.len());
    dst.insert(at, Value::Int(tab_idx));
    Ok(())
}

/// Add a new overview window (user-file grouping half). Appends an empty inner
/// list to `tabsByWindowInstanceID` and seeds it with one cloned tab (a window
/// must have ≥1 tab). Refuses on a windowless account: adding positionally there
/// would fabricate a partial mapping that hides the account's existing tabs (see
/// `create_tab`). Returns the new window's index, always ≥1 here (a windowless
/// account is refused with `NoWindowMapping`), so the char key is `overview_{idx}`.
pub fn add_overview_window(v: &mut Value, name: &str, from_tab: Option<i64>) -> Result<usize, OverviewTabError> {
    inline_all(v);
    let new_window_idx = {
        let ov = overview_mut(v)?;
        let window_count = window_count(ov);
        if window_count == 0 {
            return Err(OverviewTabError::NoWindowMapping);
        }
        let groups = groups_mut(ov);
        groups.push(Value::List(Vec::new()));
        groups.len() - 1
    };
    // Seed the new (empty) window with one cloned tab. create_tab re-inlines (a
    // no-op on the already-plain tree) and appends the tab to window `new_window_idx`.
    create_tab(v, new_window_idx, name, from_tab)?;
    Ok(new_window_idx)
}

/// Remove an overview window (user-file grouping half). Reassigns the window's
/// tabs onto window 0 (no tab loss), then drops the inner list. Last-window-only:
/// the positional link to the char-file `overview_N` keys makes middle removal a
/// re-key cascade (deferred).
pub fn remove_overview_window(v: &mut Value, window_idx: usize) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    // Read the mapping WITHOUT fabricating it (a windowless account has none).
    let groups = ov.iter_mut()
        .find(|(k, _)| is_b(k, b"tabsByWindowInstanceID"))
        .and_then(|(_, wv)| list_inner_mut(wv))
        .ok_or(OverviewTabError::LastWindow)?;
    let count = groups.len();
    if count <= 1 {
        return Err(OverviewTabError::LastWindow);
    }
    if window_idx >= count {
        return Err(OverviewTabError::UnknownWindow { index: window_idx });
    }
    if window_idx != count - 1 {
        return Err(OverviewTabError::NotLastWindow { index: window_idx });
    }
    let removed: Vec<Value> = list_inner(&groups[window_idx]).cloned().unwrap_or_default();
    if let Some(w0) = groups.get_mut(0).and_then(list_inner_mut) {
        w0.extend(removed);
    }
    groups.remove(window_idx);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    /// user tree: overview -> tabsettings_new (bare dict) -> {0:{bracket,color,name,overview:"P"}}
    /// The `bracket`/`color` keys mirror real EVE tabs — every real tab carries
    /// them, and a created tab must too (EVE's "reset overview" reads them).
    fn user_with_tabs() -> Value {
        let tab = Value::Dict(vec![
            (Value::Bytes(b"bracket".to_vec()), Value::Bytes(b"_BracketFilterShowAll".to_vec())),
            (Value::Bytes(b"color".to_vec()), Value::None),
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

    fn tab_has_key(v: &Value, idx: i64, key: &[u8]) -> bool {
        let Value::Dict(root) = v else { return false };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { return false };
        let (_, tabs) = ovd.iter().find(|(k, _)| is_b(k, b"tabsettings_new")).unwrap();
        let Value::Dict(td) = tabs else { return false };
        let Some((_, tab)) = td.iter().find(|(k, _)| as_int(k) == Some(idx)) else { return false };
        let Value::Dict(fields) = tab else { return false };
        fields.iter().any(|(k, _)| is_b(k, key))
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
        let idx = create_tab(&mut v, 0, "Mining", Some(0)).unwrap();
        assert_eq!(idx, 1, "next free index after 0");
        assert_eq!(tab_name(&v, 1), "Mining");
        assert_eq!(window_indices(&v, 0), vec![0, 1], "appended to window 0's strip");
        // Regression: a created tab must clone the sibling's bracket + color,
        // else EVE's "reset all overview settings" throws on the malformed tab.
        assert!(tab_has_key(&v, 1, b"bracket"), "created tab clones the sibling's bracket");
        assert!(tab_has_key(&v, 1, b"color"), "created tab clones the sibling's color");
    }

    #[test]
    fn create_into_missing_window_errors() {
        let mut v = user_with_tabs();
        assert!(matches!(create_tab(&mut v, 5, "X", Some(0)), Err(OverviewTabError::UnknownWindow { index: 5 })));
    }

    #[test]
    fn create_in_a_windowless_account_adds_the_tab_without_a_window_mapping() {
        // An overview with tabs but NO tabsByWindowInstanceID (fresh / post-reset).
        let tab = Value::Dict(vec![
            (Value::Bytes(b"bracket".to_vec()), Value::Bytes(b"_BracketFilterShowAll".to_vec())),
            (Value::Bytes(b"color".to_vec()), Value::None),
            (Value::Str("name".into()), Value::Str("Main".into())),
            (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
        ]);
        let overview = Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()), Value::Dict(vec![(Value::Int(0), tab)])),
        ]);
        let mut v = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), overview)]);

        // window_idx 0 is a sentinel here (there are no windows to name).
        let idx = create_tab(&mut v, 0, "Mining", Some(0)).unwrap();
        assert_eq!(idx, 1);
        assert!(tab_has_key(&v, 1, b"bracket"), "the new tab still clones bracket");
        // No window mapping is fabricated — EVE distributes tabs by default, and a
        // partial/wrong mapping would hide the whole overview.
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        assert!(!ovd.iter().any(|(k, _)| is_b(k, b"tabsByWindowInstanceID")),
            "no tabsByWindowInstanceID is fabricated for a windowless account");
    }

    #[test]
    fn create_with_no_sibling_still_carries_bracket_and_color() {
        // Empty tabsettings_new -> no sibling to clone -> the fallback tab.
        let overview = Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()), Value::Dict(vec![])),
        ]);
        let mut v = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), overview)]);
        let idx = create_tab(&mut v, 0, "First", None).unwrap();
        assert_eq!(idx, 0);
        assert!(tab_has_key(&v, 0, b"bracket"), "fallback tab carries bracket");
        assert!(tab_has_key(&v, 0, b"color"), "fallback tab carries color");
    }

    #[test]
    fn delete_on_a_windowless_account_does_not_fabricate_a_mapping() {
        let mk = |n: &str| Value::Dict(vec![
            (Value::Str("name".into()), Value::Str(n.to_string())),
            (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
        ]);
        let overview = Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()),
             Value::Dict(vec![(Value::Int(0), mk("A")), (Value::Int(1), mk("B"))])),
            // no tabsByWindowInstanceID
        ]);
        let mut v = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), overview)]);
        delete_tab(&mut v, 0).unwrap();
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        assert!(!ovd.iter().any(|(k, _)| is_b(k, b"tabsByWindowInstanceID")),
            "delete must not fabricate a window mapping on a windowless account");
    }

    #[test]
    fn delete_removes_tab_and_purges_window_strips() {
        let mut v = user_with_tabs();
        create_tab(&mut v, 0, "Mining", Some(0)).unwrap(); // now tabs 0,1 in window 0
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

    #[test]
    fn reorder_replaces_the_window_strip() {
        let mut v = user_with_tabs();
        create_tab(&mut v, 0, "Mining", Some(0)).unwrap(); // window 0 = [0,1]
        reorder_tabs_in_window(&mut v, 0, &[1, 0]).unwrap();
        assert_eq!(window_indices(&v, 0), vec![1, 0]);
    }

    #[test]
    fn reorder_missing_window_errors() {
        let mut v = user_with_tabs();
        assert!(matches!(reorder_tabs_in_window(&mut v, 3, &[0]), Err(OverviewTabError::UnknownWindow { index: 3 })));
    }

    fn user_two_windows() -> Value {
        let tab = |p: &str| Value::Dict(vec![
            (Value::Str("name".into()), Value::Str(p.to_string())),
            (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
        ]);
        let overview = Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()),
             Value::Dict(vec![(Value::Int(0), tab("A")), (Value::Int(1), tab("B"))])),
            (Value::Bytes(b"tabsByWindowInstanceID".to_vec()),
             Value::List(vec![
                 Value::List(vec![Value::Int(0)]), // window 0 = [0]
                 Value::List(vec![Value::Int(1)]), // window 1 = [1]
             ])),
        ]);
        Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), overview)])
    }

    #[test]
    fn move_relocates_tab_between_windows() {
        let mut v = user_two_windows();
        move_tab(&mut v, 0, 0, 1, 0).unwrap();
        assert_eq!(window_indices(&v, 0), Vec::<i64>::new(), "removed from source");
        assert_eq!(window_indices(&v, 1), vec![0, 1], "inserted at pos 0 of target");
    }

    #[test]
    fn move_to_missing_window_errors() {
        let mut v = user_two_windows();
        assert!(matches!(move_tab(&mut v, 0, 0, 9, 0), Err(OverviewTabError::UnknownWindow { index: 9 })));
        assert_eq!(window_indices(&v, 0), vec![0], "source strip unchanged when destination is invalid");
    }

    #[test]
    fn add_window_appends_a_group_with_a_cloned_tab() {
        let mut v = user_with_tabs(); // one window [0]
        let widx = add_overview_window(&mut v, "Scan", Some(0)).unwrap();
        assert_eq!(widx, 1, "new window appended at index 1");
        let new_tabs = window_indices(&v, 1);
        assert_eq!(new_tabs.len(), 1, "new window seeded with exactly one tab");
        assert_eq!(tab_name(&v, new_tabs[0]), "Scan");
        // Seeded via create_tab -> carries bracket/color like every valid EVE tab.
        assert!(tab_has_key(&v, new_tabs[0], b"bracket"), "seeded tab clones bracket");
        assert!(tab_has_key(&v, new_tabs[0], b"color"), "seeded tab clones color");
        assert_eq!(window_indices(&v, 0), vec![0], "window 0 untouched");
    }

    #[test]
    fn add_window_on_a_windowless_account_is_refused() {
        // Overview with tabs but no tabsByWindowInstanceID: positional add can't
        // fabricate a base mapping without hiding the account's existing tabs.
        let tab = Value::Dict(vec![
            (Value::Bytes(b"bracket".to_vec()), Value::Bytes(b"_BracketFilterShowAll".to_vec())),
            (Value::Bytes(b"color".to_vec()), Value::None),
            (Value::Str("name".into()), Value::Str("Main".into())),
            (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
        ]);
        let overview = Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()), Value::Dict(vec![(Value::Int(0), tab)])),
        ]);
        let mut v = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), overview)]);
        assert!(matches!(add_overview_window(&mut v, "X", Some(0)), Err(OverviewTabError::NoWindowMapping)));
    }

    #[test]
    fn remove_last_window_reassigns_its_tabs_to_window_zero() {
        let mut v = user_two_windows(); // window 0 = [0], window 1 = [1]
        remove_overview_window(&mut v, 1).unwrap();
        assert_eq!(window_indices(&v, 0), vec![0, 1], "removed window's tab moved to window 0");
        // Only one window remains.
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, g) = ovd.iter().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")).unwrap();
        let Value::List(outer) = g else { panic!() };
        assert_eq!(outer.len(), 1, "one window left");
        assert_eq!(tab_name(&v, 1), "B", "no tab deleted");
    }

    #[test]
    fn remove_non_last_window_is_refused() {
        let mut v = user_two_windows();
        assert!(matches!(remove_overview_window(&mut v, 0), Err(OverviewTabError::NotLastWindow { index: 0 })));
    }

    #[test]
    fn remove_the_only_window_is_refused() {
        let mut v = user_with_tabs(); // one window
        assert!(matches!(remove_overview_window(&mut v, 0), Err(OverviewTabError::LastWindow)));
    }
}
