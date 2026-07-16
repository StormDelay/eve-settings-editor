//! Read + edit projection of the overview-columns category. Visibility and
//! order live in the `core_user` file (per overview tab, with a fallback to the
//! tab's preset); widths live in the `core_char` file (per tab). All EVE format
//! knowledge (the `(timestamp, dict)` wrappers, the `(overviewScroll2, tab)`
//! width key, column tokens as Bytes) lives here so the UI stays format-blind.
//! Dict traversal reuses the shared `crate::treewalk` helpers.

use blue_marshal::Value;
use serde::Serialize;

use crate::path::{resolve_mut, NodePath, Step};
use crate::treewalk::{
    child_dict, collect_shared, effective, is_bytes, timestamped_dict, unwrap_shared, unwrap_shared_ref,
    Entries, SharedTable,
};

#[derive(Debug, Serialize, PartialEq)]
pub struct OverviewColumns {
    pub windows: Vec<OverviewWindow>,
    pub tabs: Vec<OverviewTab>,
}

/// A physical overview window and the tab indices it shows, in order — grouped
/// from `tabsByWindowInstanceID` (a list-of-lists of tab indices, one per window).
#[derive(Debug, Serialize, PartialEq)]
pub struct OverviewWindow {
    pub index: usize,
    pub tab_indices: Vec<i64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct OverviewTab {
    pub index: i64,
    pub name: String,
    pub inherits: bool,
    pub columns: Vec<OverviewColumn>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct OverviewColumn {
    pub name: String,
    pub label: String,
    pub visible: bool,
    pub width: Option<i64>,
}

pub fn project_overview(user: &Value, char_tree: Option<&Value>) -> OverviewColumns {
    let mut sh = SharedTable::new();
    collect_shared(user, &mut sh);
    let empty = OverviewColumns { windows: vec![], tabs: vec![] };
    let Some(overview) = overview_container(user, &sh) else { return empty };

    let windows = window_groups(overview, &sh);
    let tabs = tab_dict(overview, &sh)
        .map(|d| d.iter().filter_map(|(k, v)| project_tab(k, v, overview, char_tree, &sh)).collect())
        .unwrap_or_default();
    OverviewColumns { windows, tabs }
}

/// The `overview` container dict (key resolved through Ref/Shared).
fn overview_container<'a>(user: &'a Value, sh: &SharedTable<'a>) -> Option<&'a Entries> {
    let Value::Dict(root) = effective(user, sh) else { return None };
    find_child(root, b"overview", sh).and_then(|v| as_dict(v, sh))
}

/// The tab dict from `tabsettings_new` (modern) or `tabsettings` (legacy).
fn tab_dict<'a>(overview: &'a Entries, sh: &SharedTable<'a>) -> Option<&'a Entries> {
    for key in [b"tabsettings_new".as_slice(), b"tabsettings"] {
        if let Some(v) = find_child(overview, key, sh) {
            if let Some(d) = as_dict(v, sh) {
                return Some(d);
            }
        }
    }
    None
}

/// Window groups from `tabsByWindowInstanceID` (a list of lists of tab indices).
fn window_groups(overview: &Entries, sh: &SharedTable) -> Vec<OverviewWindow> {
    let Some(v) = find_child(overview, b"tabsByWindowInstanceID", sh) else { return vec![] };
    let Some(outer) = as_list_r(v, sh) else { return vec![] };
    outer.iter().enumerate().filter_map(|(i, inner)| {
        let list = as_list_r(inner, sh)?;
        let tab_indices = list.iter().filter_map(|e| as_int(effective(e, sh))).collect();
        Some(OverviewWindow { index: i, tab_indices })
    }).collect()
}

fn project_tab(key: &Value, tab: &Value, overview: &Entries, char_tree: Option<&Value>, sh: &SharedTable) -> Option<OverviewTab> {
    let index = as_int(effective(key, sh))?;
    let fields = as_dict(tab, sh)?;
    let name = str_field_r(fields, "name", sh).unwrap_or_else(|| format!("Tab {index}"));

    let own_order = token_list(fields, b"tabColumnOrder", sh);
    let own_visible = token_list(fields, b"tabColumns", sh);
    // A tab inherits unless it owns BOTH lists; any missing half falls back to
    // the tab's PRESET, so a partial tab never silently hides or drops columns.
    let inherits = own_order.is_none() || own_visible.is_none();
    let (def_order, def_visible) = preset_columns(fields, overview, sh);
    let order = own_order.unwrap_or(def_order);
    let visible = own_visible.unwrap_or(def_visible);
    let widths = char_tree.and_then(|c| tab_widths(c, index));

    // Rows follow the order list, then any visible token not present in it, so a
    // visible-but-unordered column still appears (hidden columns are never lost).
    let mut ordered = order.clone();
    for tok in &visible {
        if !ordered.contains(tok) {
            ordered.push(tok.clone());
        }
    }
    let columns = ordered
        .iter()
        .map(|tok| OverviewColumn {
            label: prettify(tok),
            visible: visible.iter().any(|v| v == tok),
            width: widths.as_ref().and_then(|w| w.get(tok).copied()),
            name: tok.clone(),
        })
        .collect();
    Some(OverviewTab { index, name, inherits, columns })
}

/// The tab's preset columns: resolve the tab's `overview` field (a preset name)
/// to `overviewProfilePresets[name].overviewColumns`. Order == visible == list.
fn preset_columns(tab: &Entries, overview: &Entries, sh: &SharedTable) -> (Vec<String>, Vec<String>) {
    let preset_name = find_child(tab, b"overview", sh).and_then(|v| token_r(v, sh));
    let cols = preset_name.and_then(|name| {
        let presets = find_child(overview, b"overviewProfilePresets", sh).and_then(|v| as_dict(v, sh))?;
        let preset = find_child(presets, name.as_bytes(), sh).and_then(|v| as_dict(v, sh))?;
        token_list(preset, b"overviewColumns", sh)
    }).unwrap_or_default();
    (cols.clone(), cols)
}

/// String field whose key is `name` (plain or string-table), value resolved
/// through Ref/Shared. Values may be Str, UCS2, or Bytes (tab names use all).
fn str_field_r(fields: &Entries, name: &str, sh: &SharedTable) -> Option<String> {
    fields.iter().find_map(|(k, v)| {
        if !key_is(effective(k, sh), name) { return None; }
        match effective(v, sh) {
            Value::Str(t) | Value::StrUcs2(t) => Some(t.clone()),
            Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        }
    })
}

/// Account-level defaults: (overviewColumnOrder, overviewColumns) as token lists.
fn account_defaults(user: &Value) -> (Vec<String>, Vec<String>) {
    let Some((ov, _)) = child_dict(user, b"overview", Vec::new()) else { return (vec![], vec![]) };
    let order = list_field(ov, b"overviewColumnOrder").unwrap_or_default();
    let visible = list_field(ov, b"overviewColumns").unwrap_or_default();
    (order, visible)
}

/// Widths for a tab: column token -> px, from char root -> ui -> SortHeadersSizes,
/// resolving Ref/Shared indirection (deduped width dicts and Ref column tokens).
// ponytail: rebuilds the char shared-table per tab; char trees are small and tab
// counts tiny, so the O(tabs * chartree) walk is a non-issue (thread it if it isn't).
fn tab_widths(char_tree: &Value, tab_index: i64) -> Option<std::collections::HashMap<String, i64>> {
    let mut sh = SharedTable::new();
    collect_shared(char_tree, &mut sh);
    let Value::Dict(root) = effective(char_tree, &sh) else { return None };
    let ui = find_child(root, b"ui", &sh).and_then(|v| as_dict(v, &sh))?;
    let sizes = find_child(ui, b"SortHeadersSizes", &sh).and_then(|v| as_dict(v, &sh))?;
    let (_, cols) = sizes.iter().find(|(k, _)| is_width_key(effective(k, &sh), tab_index))?;
    let entries = as_dict(cols, &sh)?;
    Some(
        entries
            .iter()
            .filter_map(|(k, v)| Some((token_r(k, &sh)?, as_int(effective(v, &sh))?)))
            .collect(),
    )
}

fn is_width_key(k: &Value, tab_index: i64) -> bool {
    matches!(k, Value::Tuple(items) if items.len() == 2
        && matches!(&items[0], Value::Bytes(b) if b.as_slice() == b"overviewScroll2")
        && as_int(&items[1]) == Some(tab_index))
}

/// root -> b"overview" -> b"tabsettings_new" -> (ts, dict), returning that dict.
fn tab_settings(user: &Value) -> Option<(&Entries, NodePath)> {
    let (ov, ov_path) = child_dict(user, b"overview", Vec::new())?;
    timestamped_dict(ov, &ov_path, b"tabsettings_new")
}

fn prettify(token: &str) -> String {
    // ponytail: naive Title-case. Compound tokens (TRANSVERSALVELOCITY) are not
    // word-split — that needs a curated map (deferred). Raw token shown on hover.
    let mut c = token.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => String::new(),
    }
}

fn as_int(v: &Value) -> Option<i64> {
    match v {
        Value::Int(n) => Some(*n),
        _ => None,
    }
}

fn token(v: &Value) -> Option<String> {
    // Column tokens are Bytes, sometimes Shared-wrapped (real account lists).
    match unwrap_shared_ref(v) {
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        _ => None,
    }
}

/// True if the dict key is the string `name`, whether stored plainly or as a
/// string-table reference — real files store the `"name"` key as `t52`.
fn key_is(k: &Value, name: &str) -> bool {
    match k {
        Value::Str(s) | Value::StrUcs2(s) => s == name,
        Value::StrTable(i) => blue_marshal::string_table::STRING_TABLE
            .get(*i as usize)
            .map(|s| *s == name)
            .unwrap_or(false),
        _ => false,
    }
}

/// A list-of-tokens field (tabColumns / tabColumnOrder / overviewColumns…).
/// Accepts a bare `List` or the `(timestamp, list)` wrapper real files use for
/// the account-level lists; items may be Shared-wrapped.
fn list_field(fields: &Entries, name: &[u8]) -> Option<Vec<String>> {
    let (_, v) = fields.iter().find(|(k, _)| is_bytes(k, name))?;
    let items = as_list(unwrap_shared_ref(v))?;
    Some(items.iter().filter_map(token).collect())
}

/// The `List` inside a value: a bare list, or the list element of a
/// `(timestamp, list)` wrapper tuple.
fn as_list(v: &Value) -> Option<&Vec<Value>> {
    match v {
        Value::List(items) => Some(items),
        Value::Tuple(items) => items.iter().find_map(|e| match unwrap_shared_ref(e) {
            Value::List(l) => Some(l),
            _ => None,
        }),
        _ => None,
    }
}

/// Value of the entry whose resolved key is `Bytes(name)`, itself resolved.
fn find_child<'a>(dict: &'a Entries, name: &[u8], sh: &SharedTable<'a>) -> Option<&'a Value> {
    dict.iter()
        .find(|(k, _)| matches!(effective(k, sh), Value::Bytes(b) if b.as_slice() == name))
        .map(|(_, v)| effective(v, sh))
}

/// Resolve to a dict, unwrapping a `(timestamp, dict)` wrapper.
fn as_dict<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<&'a Entries> {
    match effective(v, sh) {
        Value::Dict(d) => Some(d),
        Value::Tuple(items) => items.iter().find_map(|e| match effective(e, sh) {
            Value::Dict(d) => Some(d),
            _ => None,
        }),
        _ => None,
    }
}

/// Resolve to a list, unwrapping a `(timestamp, list)` wrapper.
fn as_list_r<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<&'a Vec<Value>> {
    match effective(v, sh) {
        Value::List(l) => Some(l),
        Value::Tuple(items) => items.iter().find_map(|e| match effective(e, sh) {
            Value::List(l) => Some(l),
            _ => None,
        }),
        _ => None,
    }
}

fn token_r(v: &Value, sh: &SharedTable) -> Option<String> {
    match effective(v, sh) {
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        _ => None,
    }
}

/// Resolved list-of-tokens for a byte-named field within `dict`.
fn token_list(dict: &Entries, name: &[u8], sh: &SharedTable) -> Option<Vec<String>> {
    let v = find_child(dict, name, sh)?;
    Some(as_list_r(v, sh)?.iter().filter_map(|t| token_r(t, sh)).collect())
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum OverviewError {
    NoTab,
}

pub fn set_column_visible(user: &mut Value, tab_index: i64, column: &str, visible: bool) -> Result<(), OverviewError> {
    let (def_order, def_visible) = account_defaults(user);
    with_tab(user, tab_index, |tab| {
        materialize_from(tab, &def_order, &def_visible);
        let tok = Value::Bytes(column.as_bytes().to_vec());
        let vis = list_mut(tab, b"tabColumns");
        let present = vis.iter().any(|v| v == &tok);
        if visible && !present {
            vis.push(tok.clone());
        } else if !visible && present {
            vis.retain(|v| v != &tok);
        }
        // A newly-shown column must also exist in the order list.
        let order = list_mut(tab, b"tabColumnOrder");
        if visible && !order.iter().any(|v| v == &tok) {
            order.push(tok);
        }
    })
}

pub fn set_column_order(user: &mut Value, tab_index: i64, order: &[String]) -> Result<(), OverviewError> {
    let (def_order, def_visible) = account_defaults(user);
    with_tab(user, tab_index, |tab| {
        materialize_from(tab, &def_order, &def_visible);
        *list_mut(tab, b"tabColumnOrder") = order.iter().map(|t| Value::Bytes(t.as_bytes().to_vec())).collect();
    })
}

/// Resolve the mutable tab dict by its Int index and run `edit` on it.
fn with_tab<F: FnOnce(&mut Vec<(Value, Value)>)>(user: &mut Value, tab_index: i64, edit: F) -> Result<(), OverviewError> {
    let path = tab_dict_path(user, tab_index).ok_or(OverviewError::NoTab)?;
    let node = resolve_mut(user, &path).ok_or(OverviewError::NoTab)?;
    let Value::Dict(fields) = node else { return Err(OverviewError::NoTab) };
    edit(fields);
    Ok(())
}

/// Path to the mutable tab dict, resolving the account defaults for materialize
/// eagerly (read them before taking the &mut borrow).
fn tab_dict_path(user: &Value, tab_index: i64) -> Option<NodePath> {
    let (dict, base) = tab_settings(user)?;
    let (i, (_, v)) = dict.iter().enumerate().find(|(_, (k, _))| as_int(k) == Some(tab_index))?;
    let mut p = base;
    p.push(Step::DictValue(i));
    // The tab value may be Shared-wrapped (marshal dedup); thread SharedInner so
    // resolve_mut lands on the Dict, mirroring project_tab's read-side unwrap.
    let (_, p) = unwrap_shared(v, p);
    Some(p)
}

/// Create the tab's own lists from the account defaults when absent (mirrors the
/// client materializing an inheriting tab on first edit). No-op if already owned.
fn materialize_from(tab: &mut Vec<(Value, Value)>, def_order: &[String], def_visible: &[String]) {
    if !tab.iter().any(|(k, _)| is_bytes(k, b"tabColumnOrder")) {
        tab.push((Value::Bytes(b"tabColumnOrder".to_vec()), toks(def_order)));
    }
    if !tab.iter().any(|(k, _)| is_bytes(k, b"tabColumns")) {
        tab.push((Value::Bytes(b"tabColumns".to_vec()), toks(def_visible)));
    }
}

fn toks(tokens: &[String]) -> Value {
    Value::List(tokens.iter().map(|t| Value::Bytes(t.as_bytes().to_vec())).collect())
}

fn list_mut<'a>(tab: &'a mut Vec<(Value, Value)>, name: &[u8]) -> &'a mut Vec<Value> {
    let (_, v) = tab.iter_mut().find(|(k, _)| is_bytes(k, name)).expect("materialized by materialize_from");
    let Value::List(items) = v else { panic!("overview column list is not a List") };
    items
}

pub fn set_column_width(char_tree: &mut Value, tab_index: i64, column: &str, width: i64) -> Result<(), OverviewError> {
    let sizes_path = sort_headers_sizes_path(char_tree).ok_or(OverviewError::NoTab)?;
    let Some(Value::Dict(sizes)) = resolve_mut(char_tree, &sizes_path) else {
        return Err(OverviewError::NoTab);
    };
    // Find or create the tab's width dict, keyed by (overviewScroll2, tabIndex).
    let pos = sizes.iter().position(|(k, _)| is_width_key(k, tab_index));
    let cols = match pos {
        Some(i) => &mut sizes[i].1,
        None => {
            let key = Value::Tuple(vec![Value::Bytes(b"overviewScroll2".to_vec()), Value::Int(tab_index)]);
            sizes.push((key, Value::Dict(vec![])));
            &mut sizes.last_mut().unwrap().1
        }
    };
    // An existing per-tab width dict may be Shared-wrapped (marshal dedup); the
    // read side (tab_widths) already unwraps it, so the write side must too.
    let cols = match cols {
        Value::Shared { value, .. } => value.as_mut(),
        other => other,
    };
    let Value::Dict(entries) = cols else { return Err(OverviewError::NoTab) };
    let tok = column.as_bytes();
    match entries.iter_mut().find(|(k, _)| is_bytes(k, tok)) {
        Some((_, v)) => *v = Value::Int(width),
        None => entries.push((Value::Bytes(tok.to_vec()), Value::Int(width))),
    }
    Ok(())
}

/// Path to the inner dict of char root -> ui -> SortHeadersSizes -> (ts, dict).
fn sort_headers_sizes_path(char_tree: &Value) -> Option<NodePath> {
    let (ui, ui_path) = child_dict(char_tree, b"ui", Vec::new())?;
    let (_, path) = timestamped_dict(ui, &ui_path, b"SortHeadersSizes")?;
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    fn ts() -> Value { Value::Long(vec![0u8; 8]) }
    fn bytes(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }

    /// user root -> b"overview" -> b"tabsettings_new" -> (ts, { 0: tab })
    /// where the tab has its own name/order/visible lists.
    fn user_with_tab() -> Value {
        let tab = Value::Dict(vec![
            (Value::Str("name".into()), Value::Str("PvP".into())),
            (bytes("tabColumnOrder"), Value::List(vec![bytes("NAME"), bytes("TYPE"), bytes("DISTANCE")])),
            (bytes("tabColumns"), Value::List(vec![bytes("NAME"), bytes("DISTANCE")])),
        ]);
        Value::Dict(vec![(
            bytes("overview"),
            Value::Dict(vec![(
                bytes("tabsettings_new"),
                Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab)])]),
            )]),
        )])
    }

    /// char root -> b"ui" -> b"SortHeadersSizes" -> (ts, { (overviewScroll2, 0): { NAME: 120 } })
    fn char_with_widths() -> Value {
        let widths = Value::Dict(vec![(bytes("NAME"), Value::Int(120))]);
        Value::Dict(vec![(
            bytes("ui"),
            Value::Dict(vec![(
                bytes("SortHeadersSizes"),
                Value::Tuple(vec![
                    ts(),
                    Value::Dict(vec![(
                        Value::Tuple(vec![bytes("overviewScroll2"), Value::Int(0)]),
                        widths,
                    )]),
                ]),
            )]),
        )])
    }

    #[test]
    fn projects_a_tab_with_order_visibility_and_widths() {
        let oc = project_overview(&user_with_tab(), Some(&char_with_widths()));
        assert_eq!(oc.tabs.len(), 1);
        let t = &oc.tabs[0];
        assert_eq!(t.index, 0);
        assert_eq!(t.name, "PvP");
        assert!(!t.inherits, "tab has its own lists");
        // Columns are in tabColumnOrder order.
        let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["NAME", "TYPE", "DISTANCE"]);
        // Visible set is tabColumns; TYPE is not visible.
        assert!(t.columns[0].visible && !t.columns[1].visible && t.columns[2].visible);
        // Width joined from the char tree for NAME only.
        assert_eq!(t.columns[0].width, Some(120));
        assert_eq!(t.columns[1].width, None);
        // Prettified label, raw token preserved.
        assert_eq!(t.columns[0].label, "Name");
    }

    #[test]
    fn a_file_without_overview_projects_empty() {
        let empty = Value::Dict(vec![]);
        assert!(project_overview(&empty, None).tabs.is_empty());
    }

    #[test]
    fn projects_preset_fallback_and_window_grouping() {
        use blue_marshal::Value;
        fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        fn ts() -> Value { Value::Long(vec![0u8; 8]) }
        // preset P: visible [NAME, TYPE]
        let preset = Value::Dict(vec![(b("overviewColumns"), Value::List(vec![b("NAME"), b("TYPE")]))]);
        let presets = Value::Dict(vec![(b("P"), preset)]);
        let tab0 = Value::Dict(vec![
            (Value::StrTable(52), Value::Str("Alpha".into())),      // name (string-table key)
            (b("overview"), b("P")),                                // references preset P (no own lists)
        ]);
        let tab1 = Value::Dict(vec![
            (Value::StrTable(52), Value::Str("Beta".into())),
            (b("tabColumnOrder"), Value::List(vec![b("NAME"), b("TYPE"), b("DISTANCE")])),
            (b("tabColumns"), Value::List(vec![b("DISTANCE")])),
        ]);
        let tabs = Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab0), (Value::Int(1), tab1)])]);
        let overview = Value::Dict(vec![
            (b("overviewProfilePresets"), presets),
            (b("tabsettings_new"), tabs),
            (b("tabsByWindowInstanceID"), Value::List(vec![
                Value::List(vec![Value::Int(0)]),
                Value::List(vec![Value::Int(1)]),
            ])),
        ]);
        // The overview container's KEY is a Ref to a Shared("overview").
        let user = Value::Dict(vec![
            (Value::Shared { slot: 1, value: Box::new(b("overview")) }, overview),
        ]);

        let oc = project_overview(&user, None);
        // window grouping
        assert_eq!(oc.windows.len(), 2);
        assert_eq!(oc.windows[0].tab_indices, vec![0]);
        assert_eq!(oc.windows[1].tab_indices, vec![1]);
        // tab 0 inherits preset P -> [NAME(hidden? no, preset visible), TYPE]
        let t0 = oc.tabs.iter().find(|t| t.index == 0).unwrap();
        assert!(t0.inherits, "tab 0 has no own lists");
        assert_eq!(t0.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["NAME", "TYPE"]);
        assert!(t0.columns.iter().all(|c| c.visible), "preset columns are the visible set");
        // tab 1 owns its lists
        let t1 = oc.tabs.iter().find(|t| t.index == 1).unwrap();
        assert!(!t1.inherits);
        assert_eq!(t1.columns.iter().filter(|c| c.visible).map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["DISTANCE"]);
        assert_eq!(t1.columns.len(), 3, "order list of 3");
    }

    fn tab_lists(user: &Value, index: i64) -> (Vec<String>, Vec<String>) {
        let t = project_overview(user, None).tabs.into_iter().find(|t| t.index == index).unwrap();
        let order: Vec<String> = t.columns.iter().map(|c| c.name.clone()).collect();
        let visible: Vec<String> = t.columns.iter().filter(|c| c.visible).map(|c| c.name.clone()).collect();
        (order, visible)
    }

    #[test]
    fn toggle_visibility_on_an_owning_tab() {
        let mut user = user_with_tab();
        // TYPE starts hidden; show it.
        set_column_visible(&mut user, 0, "TYPE", true).unwrap();
        let (_, visible) = tab_lists(&user, 0);
        assert!(visible.contains(&"TYPE".to_string()));
        // Hide NAME again.
        set_column_visible(&mut user, 0, "NAME", false).unwrap();
        let (_, visible) = tab_lists(&user, 0);
        assert!(!visible.contains(&"NAME".to_string()));
    }

    #[test]
    fn reorder_sets_the_full_order() {
        let mut user = user_with_tab();
        set_column_order(&mut user, 0, &["DISTANCE".into(), "NAME".into(), "TYPE".into()]).unwrap();
        let (order, _) = tab_lists(&user, 0);
        assert_eq!(order, vec!["DISTANCE", "NAME", "TYPE"]);
    }

    /// A tab that inherits (no own lists) materializes from the account defaults
    /// on first edit, then applies the edit.
    fn user_inheriting_tab() -> Value {
        // The tab owns no lists; it names preset "G" so the read falls back to
        // the preset's columns (account-level lists feed the edit-side materialize).
        let tab = Value::Dict(vec![
            (Value::Str("name".into()), Value::Str("General".into())),
            (bytes("overview"), bytes("G")),
        ]);
        let preset = Value::Dict(vec![(bytes("overviewColumns"), Value::List(vec![bytes("NAME"), bytes("TYPE")]))]);
        Value::Dict(vec![(
            bytes("overview"),
            Value::Dict(vec![
                (bytes("overviewProfilePresets"), Value::Dict(vec![(bytes("G"), preset)])),
                (bytes("overviewColumnOrder"), Value::List(vec![bytes("NAME"), bytes("TYPE")])),
                (bytes("overviewColumns"), Value::List(vec![bytes("NAME")])),
                (
                    bytes("tabsettings_new"),
                    Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(1), tab)])]),
                ),
            ]),
        )])
    }

    #[test]
    fn editing_an_inheriting_tab_materializes_its_lists() {
        let mut user = user_inheriting_tab();
        assert!(project_overview(&user, None).tabs[0].inherits);
        // Show TYPE on the inheriting tab.
        set_column_visible(&mut user, 1, "TYPE", true).unwrap();
        let t = project_overview(&user, None).tabs.into_iter().find(|t| t.index == 1).unwrap();
        assert!(!t.inherits, "tab now owns its lists");
        assert_eq!(t.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>(), vec!["NAME", "TYPE"]);
        assert!(t.columns.iter().find(|c| c.name == "TYPE").unwrap().visible);
    }

    #[test]
    fn editing_a_missing_tab_errors() {
        let mut user = user_with_tab();
        assert_eq!(set_column_visible(&mut user, 99, "NAME", true), Err(OverviewError::NoTab));
    }

    fn width_of(char_tree: &Value, tab: i64, col: &str) -> Option<i64> {
        let user = user_with_tab(); // provides the order so the column appears
        project_overview(&user, Some(char_tree))
            .tabs.into_iter().find(|t| t.index == tab)?
            .columns.into_iter().find(|c| c.name == col)?.width
    }

    #[test]
    fn set_width_overwrites_existing() {
        let mut c = char_with_widths();
        set_column_width(&mut c, 0, "NAME", 200).unwrap();
        assert_eq!(width_of(&c, 0, "NAME"), Some(200));
    }

    #[test]
    fn set_width_inserts_a_new_column_entry() {
        let mut c = char_with_widths();
        set_column_width(&mut c, 0, "TYPE", 88).unwrap();
        assert_eq!(width_of(&c, 0, "TYPE"), Some(88));
        assert_eq!(width_of(&c, 0, "NAME"), Some(120), "existing width untouched");
    }

    #[test]
    fn set_width_creates_the_tab_width_dict_when_absent() {
        // char_with_widths only has tab 0; write tab 1.
        let mut c = char_with_widths();
        set_column_width(&mut c, 1, "NAME", 77).unwrap();
        // Re-project a user that has tab 1 to read it back.
        let user = user_inheriting_tab();
        let w = project_overview(&user, Some(&c)).tabs.into_iter()
            .find(|t| t.index == 1).unwrap()
            .columns.into_iter().find(|col| col.name == "NAME").unwrap().width;
        assert_eq!(w, Some(77));
    }

    #[test]
    fn set_width_unwraps_a_shared_width_dict() {
        // An existing per-tab width dict deduped into a Shared must still be editable.
        let widths = Value::Shared {
            slot: 1,
            value: Box::new(Value::Dict(vec![(bytes("NAME"), Value::Int(120))])),
        };
        let mut c = Value::Dict(vec![(
            bytes("ui"),
            Value::Dict(vec![(
                bytes("SortHeadersSizes"),
                Value::Tuple(vec![ts(), Value::Dict(vec![(
                    Value::Tuple(vec![bytes("overviewScroll2"), Value::Int(0)]),
                    widths,
                )])]),
            )]),
        )]);
        set_column_width(&mut c, 0, "NAME", 200).unwrap();
        assert_eq!(width_of(&c, 0, "NAME"), Some(200));
    }

    #[test]
    fn editing_a_shared_wrapped_tab_resolves_and_edits_it() {
        // A tab value deduplicated into a Shared must still be found and edited,
        // not misreported as NoTab.
        let tab = Value::Dict(vec![
            (Value::Str("name".into()), Value::Str("P".into())),
            (bytes("tabColumnOrder"), Value::List(vec![bytes("NAME"), bytes("TYPE")])),
            (bytes("tabColumns"), Value::List(vec![bytes("NAME")])),
        ]);
        let mut user = Value::Dict(vec![(
            bytes("overview"),
            Value::Dict(vec![(
                bytes("tabsettings_new"),
                Value::Tuple(vec![ts(), Value::Dict(vec![(
                    Value::Int(0),
                    Value::Shared { slot: 1, value: Box::new(tab) },
                )])]),
            )]),
        )]);
        set_column_visible(&mut user, 0, "TYPE", true).unwrap();
        let t = project_overview(&user, None).tabs.into_iter().find(|t| t.index == 0).unwrap();
        assert!(t.columns.iter().find(|c| c.name == "TYPE").unwrap().visible);
    }

    #[test]
    fn find_child_resolves_ref_and_shared_keys() {
        use blue_marshal::Value;
        // A dict whose "overview" key is a Ref to a Shared("overview") elsewhere.
        let doc = Value::Dict(vec![
            (Value::Shared { slot: 5, value: Box::new(Value::Bytes(b"overview".to_vec())) },
             Value::Dict(vec![(Value::Bytes(b"x".to_vec()), Value::Int(1))])),
            (Value::Ref(5), Value::Dict(vec![(Value::Bytes(b"y".to_vec()), Value::Int(2))])),
        ]);
        let mut sh = SharedTable::new();
        collect_shared(&doc, &mut sh);
        let Value::Dict(entries) = &doc else { unreachable!() };
        // Both entries resolve to key "overview"; find_child returns the FIRST.
        let got = find_child(entries, b"overview", &sh).and_then(|v| as_dict(v, &sh));
        assert!(got.is_some(), "a Shared-keyed child is found");
    }

    #[test]
    fn token_r_resolves_ref_tokens() {
        use blue_marshal::Value;
        let doc = Value::List(vec![
            Value::Shared { slot: 9, value: Box::new(Value::Bytes(b"NAME".to_vec())) },
            Value::Ref(9),
        ]);
        let mut sh = SharedTable::new();
        collect_shared(&doc, &mut sh);
        let Value::List(items) = &doc else { unreachable!() };
        assert_eq!(token_r(&items[0], &sh).as_deref(), Some("NAME"));
        assert_eq!(token_r(&items[1], &sh).as_deref(), Some("NAME"), "a Ref token resolves");
    }

    #[test]
    fn token_list_reads_ts_wrapped_ref_list() {
        use blue_marshal::Value;
        // A (timestamp, list) field whose items are Ref/Shared tokens.
        let doc = Value::Dict(vec![(
            Value::Bytes(b"cols".to_vec()),
            Value::Tuple(vec![
                Value::Long(vec![0u8; 8]),
                Value::List(vec![
                    Value::Shared { slot: 7, value: Box::new(Value::Bytes(b"NAME".to_vec())) },
                    Value::Ref(7),
                ]),
            ]),
        )]);
        let mut sh = SharedTable::new();
        collect_shared(&doc, &mut sh);
        let Value::Dict(entries) = &doc else { unreachable!() };
        assert_eq!(token_list(entries, b"cols", &sh), Some(vec!["NAME".to_string(), "NAME".to_string()]));
    }
}
