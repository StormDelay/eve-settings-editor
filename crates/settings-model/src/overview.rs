//! Read + edit projection of the overview-columns category. A tab's visibility
//! (`tabColumns`) and order (`tabColumnOrder`) live in the `core_user` file and
//! are INDEPENDENT — a tab can own one and inherit the other. A tab that owns
//! neither inherits the account-global `overviewColumns` / `overviewColumnOrder`
//! (both in the `overview` container). Widths live in the `core_char` file per
//! tab. All EVE format knowledge (the `(timestamp, dict)` wrappers, the
//! `(overviewScroll2, tab)` width key, column tokens as Bytes) lives here so the
//! UI stays format-blind. Dict traversal reuses the shared `crate::treewalk`
//! helpers.
//!
//! CEILING: an inheriting tab's EXACT on-screen order is EVE runtime state, not
//! stored anywhere in the file (validated live — every stored order list is
//! account-wide identical, yet inheriting tabs render a different order). So the
//! account-global `overviewColumnOrder` is the best AVAILABLE approximation for
//! an inheriting tab's order, not its true order; reordering the tab pins it.

use blue_marshal::Value;
use serde::Serialize;

use crate::path::{resolve, resolve_mut, NodePath, Step};
use crate::treewalk::{
    collect_shared, effective, is_bytes, unwrap_shared, unwrap_shared_ref, Entries, SharedTable,
};

#[derive(Debug, Serialize, PartialEq)]
pub struct OverviewColumns {
    pub windows: Vec<OverviewWindow>,
    pub tabs: Vec<OverviewTab>,
    pub presets: Vec<Preset>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Preset {
    pub name: String,
    pub groups: Vec<i64>,
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
    pub preset: String,
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
    let empty = OverviewColumns { windows: vec![], tabs: vec![], presets: vec![] };
    let Some(overview) = overview_container(user, &sh) else { return empty };

    let windows = window_groups(overview, &sh);
    let tabs = tab_dict(overview, &sh)
        .map(|d| d.iter().filter_map(|(k, v)| project_tab(k, v, overview, char_tree, &sh)).collect())
        .unwrap_or_default();
    let presets = presets_with_groups(overview, &sh);
    OverviewColumns { windows, tabs, presets }
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
    let preset = find_child(fields, b"overview", sh)
        .and_then(|v| match effective(v, sh) {
            Value::Str(t) | Value::StrUcs2(t) => Some(t.clone()),
            Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        })
        .unwrap_or_default();

    let own_order = token_list(fields, b"tabColumnOrder", sh);
    let own_visible = token_list(fields, b"tabColumns", sh);
    // Visibility and order inherit INDEPENDENTLY: a tab may own `tabColumns` but
    // not `tabColumnOrder` (or vice versa). Each missing half falls back to the
    // account default, so a partial tab never silently hides or drops columns.
    // `inherits` (for the UI note) means the tab lacks at least one own list.
    let inherits = own_order.is_none() || own_visible.is_none();
    let (def_order, def_visible) = default_columns(overview, sh);
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
    Some(OverviewTab { index, name, preset, inherits, columns })
}

/// The account-global default columns an inheriting tab shows: `(order, visible)`
/// from the `overview` container's `overviewColumnOrder` (master ~14-col order)
/// and `overviewColumns` (the visible subset). Both are Bytes token lists whose
/// items may be Ref/Shared (`token_list` resolves them). NOTE these are separate
/// systems from `overviewProfilePresets`, which are FILTER presets (which SHIPS to
/// show, integer id lists) and carry NO columns — reading columns from a preset
/// was the 2a bug. See the module CEILING note on the order approximation.
fn default_columns(overview: &Entries, sh: &SharedTable) -> (Vec<String>, Vec<String>) {
    let order = token_list(overview, b"overviewColumnOrder", sh).unwrap_or_default();
    let visible = token_list(overview, b"overviewColumns", sh).unwrap_or_default();
    (order, visible)
}

/// Each preset's name and its group IDs, sorted case-insensitively by name (the
/// SAME order the picker/neighbour logic uses). `groups` is the preset's `groups`
/// list (empty if absent); the two state lists are not read here (slice 3).
fn presets_with_groups(overview: &Entries, sh: &SharedTable) -> Vec<Preset> {
    let Some(dict) = find_child(overview, b"overviewProfilePresets", sh).and_then(|v| as_dict(v, sh))
    else { return vec![] };
    let mut out: Vec<Preset> = dict
        .iter()
        .filter_map(|(k, v)| {
            let name = preset_key_name(effective(k, sh))?;
            let groups = as_dict(v, sh)
                .and_then(|d| find_child(d, b"groups", sh))
                .and_then(|g| as_list_r(g, sh))
                .map(|l| l.iter().filter_map(|e| as_int(effective(e, sh))).collect())
                .unwrap_or_default();
            Some(Preset { name, groups })
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// A preset dict key as a string (Bytes on real files; Str/StrUcs2 defensively).
fn preset_key_name(k: &Value) -> Option<String> {
    match k {
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        Value::Str(s) | Value::StrUcs2(s) => Some(s.clone()),
        _ => None,
    }
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

/// Immutable prep for a tab edit, in ONE shared-table pass: the account-default
/// `(order, visible)` columns to seed a materialized list, plus the tab's own
/// `tabColumnOrder` / `tabColumns` entry indices resolved through Ref/Shared
/// keys (`None` = the tab does not own that list). The mutable phase mutates by
/// these indices, so a list owned under a Ref/Shared field-name key is edited in
/// place instead of being masked by a pushed duplicate. Indices address the same
/// `fields` Vec `with_tab` reaches (both descend to the tab's inner dict).
struct TabEdit {
    def_order: Vec<String>,
    def_visible: Vec<String>,
    order_idx: Option<usize>,
    visible_idx: Option<usize>,
}

fn tab_edit_prep(user: &Value, tab_index: i64) -> TabEdit {
    let mut sh = SharedTable::new();
    collect_shared(user, &mut sh);
    let empty = TabEdit { def_order: vec![], def_visible: vec![], order_idx: None, visible_idx: None };
    let Some(overview) = overview_container(user, &sh) else { return empty };
    let Some(tabs) = tab_dict(overview, &sh) else { return empty };
    let Some((_, tab)) = tabs.iter().find(|(k, _)| as_int(effective(k, &sh)) == Some(tab_index)) else {
        return empty;
    };
    let Some(fields) = as_dict(tab, &sh) else { return empty };
    let (def_order, def_visible) = default_columns(overview, &sh);
    TabEdit {
        def_order,
        def_visible,
        order_idx: find_child_entry(fields, b"tabColumnOrder", &sh).map(|(i, _)| i),
        visible_idx: find_child_entry(fields, b"tabColumns", &sh).map(|(i, _)| i),
    }
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
    let (_, cols) = sizes.iter().find(|(k, _)| is_width_key(k, tab_index, &sh))?;
    let entries = as_dict(cols, &sh)?;
    Some(
        entries
            .iter()
            .filter_map(|(k, v)| Some((token_r(k, &sh)?, as_int(effective(v, &sh))?)))
            .collect(),
    )
}

/// The width-dict key is `(overviewScroll2, tabIndex)`; on real files the tuple
/// or its elements can be Ref/Shared-wrapped, so resolve each through the table.
fn is_width_key(k: &Value, tab_index: i64, sh: &SharedTable) -> bool {
    let Value::Tuple(items) = effective(k, sh) else { return false };
    items.len() == 2
        && is_bytes(effective(&items[0], sh), b"overviewScroll2")
        && as_int(effective(&items[1], sh)) == Some(tab_index)
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

/// Upgrade a legacy overview to the modern shape on edit: if the container has
/// only the legacy `tabsettings` tab dict (no `tabsettings_new`), rename the key
/// to `tabsettings_new`. The two formats are structurally identical (validated
/// on real files — same account-default lists, `tabsByWindowInstanceID`, and
/// per-tab `tabColumns`/`tabColumnOrder`), so this is a pure key rename; EVE
/// reads `tabsettings_new`, and a new-only file is a state EVE itself produces.
/// Run AFTER `inline_all`, so the overview container and its keys are plain.
fn migrate_legacy_overview(user: &mut Value) {
    let Some(entries) = overview_entries_mut(user) else { return };
    if entries.iter().any(|(k, _)| is_bytes(k, b"tabsettings_new")) {
        return;
    }
    if let Some((k, _)) = entries.iter_mut().find(|(k, _)| is_bytes(k, b"tabsettings")) {
        *k = Value::Bytes(b"tabsettings_new".to_vec());
    }
}

/// Mutable entries of the `root → overview` container (unwrapping a
/// `(timestamp, dict)` value). Assumes a plain tree (post-`inline_all`).
fn overview_entries_mut(user: &mut Value) -> Option<&mut Entries> {
    let Value::Dict(root) = user else { return None };
    let (_, ov) = root.iter_mut().find(|(k, _)| is_bytes(k, b"overview"))?;
    match ov {
        Value::Dict(e) => Some(e),
        Value::Tuple(items) => items.iter_mut().find_map(|e| match e {
            Value::Dict(d) => Some(d),
            _ => None,
        }),
        _ => None,
    }
}

pub fn set_column_visible(user: &mut Value, tab_index: i64, column: &str, visible: bool) -> Result<(), OverviewError> {
    crate::treewalk::inline_all(user);
    migrate_legacy_overview(user);
    let prep = tab_edit_prep(user, tab_index);
    // Immutable phase: resolve the target column's position WITHIN each owned
    // list through Ref/Shared (real files store a customized tab's tabColumns/
    // tabColumnOrder ITEMS as Shared/Ref, which the mutable phase's bare `==` on
    // a `Bytes` token would miss — duplicating on show, failing to remove on
    // hide). When the tab doesn't own a list, materialize seeds it from bare-
    // Bytes defaults, so plain string membership over the defaults is correct.
    let (owned_vis_pos, owned_order_pos) = owned_column_positions(user, tab_index, column);
    let vis_pos = match prep.visible_idx {
        Some(_) => owned_vis_pos,
        None => prep.def_visible.iter().position(|c| c == column),
    };
    let in_order = match prep.order_idx {
        Some(_) => owned_order_pos.is_some(),
        None => prep.def_order.iter().any(|c| c == column),
    };
    with_tab(user, tab_index, |tab| {
        let order_i = materialize_list(tab, b"tabColumnOrder", prep.order_idx, &prep.def_order);
        let visible_i = materialize_list(tab, b"tabColumns", prep.visible_idx, &prep.def_visible);
        let vis = list_at(tab, visible_i);
        match (visible, vis_pos) {
            (true, None) => vis.push(Value::Bytes(column.as_bytes().to_vec())),
            (false, Some(i)) => { vis.remove(i); }
            _ => {} // already in the desired state
        }
        // A newly-shown column must also exist in the order list.
        if visible && !in_order {
            list_at(tab, order_i).push(Value::Bytes(column.as_bytes().to_vec()));
        }
    })
}

/// Positions of the item resolving to `column` within the tab's OWNED
/// `tabColumns` / `tabColumnOrder` lists (each `None` when the tab doesn't own
/// that list or the column isn't present), in one shared-table pass. Owned list
/// items are stored as `Shared`/`Ref` on real customized tabs, so each item's
/// `effective` value is resolved before comparison — a bare `== Bytes(column)`
/// would miss them.
fn owned_column_positions(user: &Value, tab_index: i64, column: &str) -> (Option<usize>, Option<usize>) {
    let mut sh = SharedTable::new();
    collect_shared(user, &mut sh);
    let item_pos = |name: &[u8]| -> Option<usize> {
        let overview = overview_container(user, &sh)?;
        let tabs = tab_dict(overview, &sh)?;
        let (_, tab) = tabs.iter().find(|(k, _)| as_int(effective(k, &sh)) == Some(tab_index))?;
        let fields = as_dict(tab, &sh)?;
        let items = as_list_r(find_child(fields, name, &sh)?, &sh)?;
        items.iter().position(|it| is_bytes(effective(it, &sh), column.as_bytes()))
    };
    (item_pos(b"tabColumns"), item_pos(b"tabColumnOrder"))
}

pub fn set_column_order(user: &mut Value, tab_index: i64, order: &[String]) -> Result<(), OverviewError> {
    crate::treewalk::inline_all(user);
    migrate_legacy_overview(user);
    let prep = tab_edit_prep(user, tab_index);
    with_tab(user, tab_index, |tab| {
        materialize_list(tab, b"tabColumns", prep.visible_idx, &prep.def_visible);
        let order_i = materialize_list(tab, b"tabColumnOrder", prep.order_idx, &prep.def_order);
        *list_at(tab, order_i) = order.iter().map(|t| Value::Bytes(t.as_bytes().to_vec())).collect();
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

/// Path to the mutable tab dict. Every hop (overview container, tabsettings_new/
/// tabsettings, the tab entry) is located by its RESOLVED key, so a Ref/Shared
/// key on a real file matches; `resolve_mut` then walks by concrete indices, with
/// `SharedInner` threaded wherever a value was deduped into a `Shared`.
fn tab_dict_path(user: &Value, tab_index: i64) -> Option<NodePath> {
    let mut sh = SharedTable::new();
    collect_shared(user, &mut sh);
    let (root, base) = unwrap_shared(user, Vec::new());
    let Value::Dict(root) = root else { return None };
    // user -> overview container
    let (ci, cv) = find_child_entry(root, b"overview", &sh)?;
    let mut p = base;
    p.push(Step::DictValue(ci));
    let (cv, p) = unwrap_shared(cv, p);
    let Value::Dict(ov) = cv else { return None };
    // -> tabsettings_new (modern) or tabsettings (legacy), a (ts, dict) wrapper
    let (ti, tv) = [b"tabsettings_new".as_slice(), b"tabsettings"]
        .into_iter()
        .find_map(|n| find_child_entry(ov, n, &sh))?;
    let mut p = p;
    p.push(Step::DictValue(ti));
    let (tabs, p) = dict_at(tv, p)?;
    // -> the tab entry by resolved Int index
    let (i, (_, v)) = tabs.iter().enumerate().find(|(_, (k, _))| as_int(effective(k, &sh)) == Some(tab_index))?;
    let mut p = p;
    p.push(Step::DictValue(i));
    // The tab value may itself be Shared-wrapped; thread SharedInner to the Dict.
    let (_, p) = unwrap_shared(v, p);
    Some(p)
}

/// Entry index within `dict` whose RESOLVED key is `Bytes(name)`, with its raw
/// (unresolved) value so a path built from the index stays walkable.
fn find_child_entry<'a>(dict: &'a Entries, name: &[u8], sh: &SharedTable) -> Option<(usize, &'a Value)> {
    dict.iter()
        .enumerate()
        .find(|(_, (k, _))| matches!(effective(k, sh), Value::Bytes(b) if b.as_slice() == name))
        .map(|(i, (_, v))| (i, v))
}

/// Descend a value that is a dict or a `(ts, dict)` wrapper (either possibly
/// `Shared`) to its inner dict, threading the path steps taken.
fn dict_at(v: &Value, p: NodePath) -> Option<(&Entries, NodePath)> {
    let (v, p) = unwrap_shared(v, p);
    match v {
        Value::Dict(d) => Some((d, p)),
        Value::Tuple(items) => {
            let (i, _) = items.iter().enumerate().find(|(_, e)| matches!(unwrap_shared_ref(e), Value::Dict(_)))?;
            let mut p2 = p;
            p2.push(Step::Tuple(i));
            let (e, p2) = unwrap_shared(&items[i], p2);
            let Value::Dict(d) = e else { return None };
            Some((d, p2))
        }
        _ => None,
    }
}

/// Ensure the tab owns the named list and return its entry index. `idx` is the
/// pre-resolved (Ref/Shared-key-aware) index of an already-owned list, or `None`
/// when the tab genuinely lacks it — only then do we push a list seeded from the
/// account default (mirrors the client materializing an inheriting tab on first
/// edit). Resolving
/// the key first is what stops a duplicate `Bytes` key being pushed over a list
/// the tab already owns under a Ref/Shared field-name key.
fn materialize_list(tab: &mut Entries, name: &[u8], idx: Option<usize>, defaults: &[String]) -> usize {
    match idx {
        Some(i) => i,
        None => {
            tab.push((Value::Bytes(name.to_vec()), toks(defaults)));
            tab.len() - 1
        }
    }
}

fn toks(tokens: &[String]) -> Value {
    Value::List(tokens.iter().map(|t| Value::Bytes(t.as_bytes().to_vec())).collect())
}

/// The List value at entry `i` (materialized/owned column list). Value handling
/// is unchanged from the old `list_mut` — a `(ts, list)`-wrapped column list
/// stays out of scope here; the fix is purely key resolution at the call site.
fn list_at(tab: &mut Entries, i: usize) -> &mut Vec<Value> {
    let Value::List(items) = &mut tab[i].1 else { panic!("overview column list is not a List") };
    items
}

pub fn set_column_width(char_tree: &mut Value, tab_index: i64, column: &str, width: i64) -> Result<(), OverviewError> {
    let sizes_path = sort_headers_sizes_path(char_tree).ok_or(OverviewError::NoTab)?;
    // Immutable phase: locate the tab's width entry AND, within it, the column's
    // entry — both by RESOLVED key — as owned indices, then drop the table before
    // taking the &mut path. The column token (e.g. NAME) recurs across every tab's
    // width dict, so real files Ref/Shared-dedup it; a bare `is_bytes` match would
    // miss the existing entry and PUSH A DUPLICATE key. `unwrap_shared_ref` mirrors
    // the mutable phase's Shared unwrap so the resolved index stays aligned.
    let mut sh = SharedTable::new();
    collect_shared(char_tree, &mut sh);
    let (pos, col_pos) = match resolve(char_tree, &sizes_path) {
        Some(Value::Dict(sizes)) => {
            let pos = sizes.iter().position(|(k, _)| is_width_key(k, tab_index, &sh));
            let col_pos = pos.and_then(|i| match unwrap_shared_ref(&sizes[i].1) {
                Value::Dict(entries) => {
                    entries.iter().position(|(k, _)| is_bytes(effective(k, &sh), column.as_bytes()))
                }
                _ => None,
            });
            (pos, col_pos)
        }
        _ => (None, None),
    };
    drop(sh);
    let Some(Value::Dict(sizes)) = resolve_mut(char_tree, &sizes_path) else {
        return Err(OverviewError::NoTab);
    };
    // Find or create the tab's width dict, keyed by (overviewScroll2, tabIndex).
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
    // `Some(i)`: the column already resolves — update in place, NO new key.
    // `None`: genuinely absent — push. (When `pos` was None the dict was just
    // created empty, so `col_pos` is None and we push into it.)
    match col_pos {
        Some(i) => entries[i].1 = Value::Int(width),
        None => entries.push((Value::Bytes(column.as_bytes().to_vec()), Value::Int(width))),
    }
    Ok(())
}

/// Path to the inner dict of char root -> ui -> SortHeadersSizes -> (ts, dict).
/// Every hop is located by its RESOLVED key (exactly as `tab_widths` reads it),
/// so a Ref/Shared-deduped `ui` or `SortHeadersSizes` key on a real char file
/// matches and width writes land instead of returning NoTab. Reuses the same
/// resolved-key machinery as `tab_dict_path` (`find_child_entry`/`dict_at`).
fn sort_headers_sizes_path(char_tree: &Value) -> Option<NodePath> {
    let mut sh = SharedTable::new();
    collect_shared(char_tree, &mut sh);
    let (root, base) = unwrap_shared(char_tree, Vec::new());
    let Value::Dict(root) = root else { return None };
    // char -> ui
    let (ui_i, ui_v) = find_child_entry(root, b"ui", &sh)?;
    let mut p = base;
    p.push(Step::DictValue(ui_i));
    let (ui_v, p) = unwrap_shared(ui_v, p);
    let Value::Dict(ui) = ui_v else { return None };
    // -> SortHeadersSizes, a (ts, dict) wrapper
    let (si, sv) = find_child_entry(ui, b"SortHeadersSizes", &sh)?;
    let mut p = p;
    p.push(Step::DictValue(si));
    let (_, p) = dict_at(sv, p)?;
    Some(p)
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
    fn projects_account_default_fallback_and_window_grouping() {
        use blue_marshal::Value;
        fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        fn ts() -> Value { Value::Long(vec![0u8; 8]) }
        let tab0 = Value::Dict(vec![
            (Value::StrTable(52), Value::Str("Alpha".into())),      // name (string-table key); no own lists
            (b("overview"), b("P")),                                // a FILTER preset name — irrelevant to columns
        ]);
        let tab1 = Value::Dict(vec![
            (Value::StrTable(52), Value::Str("Beta".into())),
            (b("tabColumnOrder"), Value::List(vec![b("NAME"), b("TYPE"), b("DISTANCE")])),
            (b("tabColumns"), Value::List(vec![b("DISTANCE")])),
        ]);
        let tabs = Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab0), (Value::Int(1), tab1)])]);
        let overview = Value::Dict(vec![
            // account-global default columns an inheriting tab falls back to
            (b("overviewColumnOrder"), Value::List(vec![b("NAME"), b("TYPE")])),
            (b("overviewColumns"), Value::List(vec![b("NAME"), b("TYPE")])),
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
        // tab 0 inherits the account default -> [NAME, TYPE], both visible
        let t0 = oc.tabs.iter().find(|t| t.index == 0).unwrap();
        assert!(t0.inherits, "tab 0 has no own lists");
        assert_eq!(t0.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["NAME", "TYPE"]);
        assert!(t0.columns.iter().all(|c| c.visible), "account default visible set");
        // tab 1 owns its lists
        let t1 = oc.tabs.iter().find(|t| t.index == 1).unwrap();
        assert!(!t1.inherits);
        assert_eq!(t1.columns.iter().filter(|c| c.visible).map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["DISTANCE"]);
        assert_eq!(t1.columns.len(), 3, "order list of 3");
    }

    #[test]
    fn tab_owning_visibility_but_not_order_inherits_only_the_order() {
        // Real idiom (validated live on tab "3"): hiding a column materializes
        // `tabColumns` but NOT `tabColumnOrder`, so the tab owns its visible set
        // yet still inherits order from the account default.
        use blue_marshal::Value;
        fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        fn ts() -> Value { Value::Long(vec![0u8; 8]) }
        let tab = Value::Dict(vec![
            (Value::Str("name".into()), Value::Str("3".into())),
            // owns visibility (NAME hidden out of the default) but no order list
            (b("tabColumns"), Value::List(vec![b("TYPE"), b("DISTANCE")])),
        ]);
        let overview = Value::Dict(vec![
            (b("overviewColumnOrder"), Value::List(vec![b("NAME"), b("TYPE"), b("DISTANCE")])),
            (b("overviewColumns"), Value::List(vec![b("NAME"), b("TYPE"), b("DISTANCE")])),
            (b("tabsettings_new"), Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(2), tab)])])),
        ]);
        let user = Value::Dict(vec![(b("overview"), overview)]);

        let t = project_overview(&user, None).tabs.into_iter().find(|t| t.index == 2).unwrap();
        assert!(t.inherits, "inherits because it lacks its own order list");
        // Order comes from the account default; visibility from the tab's own set.
        assert_eq!(t.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["NAME", "TYPE", "DISTANCE"]);
        let visible: Vec<_> = t.columns.iter().filter(|c| c.visible).map(|c| c.name.as_str()).collect();
        assert_eq!(visible, vec!["TYPE", "DISTANCE"], "NAME hidden by the tab's own tabColumns");
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

    #[test]
    fn edit_preserves_cross_referenced_shared_tokens_and_still_encodes() {
        // Real-file idiom: a column token is stored ONCE as a `Shared` and `Ref`'d
        // elsewhere. Here tab 0's tabColumnOrder DEFINES Shared slot 1 ("NAME");
        // the account-level overviewColumnOrder/overviewColumns REFERENCE it.
        // A column edit rebuilds/shrinks tab 0's list — without inlining sharing
        // first, that destroys the Shared(1) definition and the account lists'
        // Ref(1) dangles, so encode fails RefBeforeStore(1). This guards that fix.
        use blue_marshal::{decode, encode, Value};
        fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        fn ts() -> Value { Value::Long(vec![0u8; 8]) }
        let tab0 = Value::Dict(vec![
            (b("tabColumnOrder"), Value::List(vec![
                Value::Shared { slot: 1, value: Box::new(b("NAME")) }, b("TYPE"),
            ])),
            (b("tabColumns"), Value::List(vec![b("TYPE")])),
        ]);
        // tabsettings_new is FIRST so slot 1 is stored before the Refs below.
        let overview = Value::Dict(vec![
            (b("tabsettings_new"), Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab0)])])),
            (b("overviewColumnOrder"), Value::List(vec![Value::Ref(1), b("TYPE")])),
            (b("overviewColumns"), Value::List(vec![Value::Ref(1)])),
        ]);
        let mut user = Value::Dict(vec![(b("overview"), overview)]);
        encode(&user).expect("fixture must encode before the edit");

        set_column_order(&mut user, 0, &["TYPE".into(), "NAME".into()]).unwrap();

        let bytes = encode(&user).expect("edited tree must still encode (no dangling Ref)");
        let decoded = decode(&bytes).unwrap();
        let t0 = project_overview(&decoded, None).tabs.into_iter().find(|t| t.index == 0).unwrap();
        assert_eq!(t0.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>(), vec!["TYPE", "NAME"]);
    }

    #[test]
    fn hiding_a_shared_defining_column_still_encodes() {
        // Hiding removes a token from tabColumns; if that token was the `Shared`
        // definition and is `Ref`'d from the account lists, dropping it would
        // dangle the Ref. Inlining before the edit prevents it.
        use blue_marshal::{encode, Value};
        fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        fn ts() -> Value { Value::Long(vec![0u8; 8]) }
        let tab0 = Value::Dict(vec![
            (b("tabColumnOrder"), Value::List(vec![b("NAME"), b("TYPE")])),
            (b("tabColumns"), Value::List(vec![
                Value::Shared { slot: 1, value: Box::new(b("NAME")) }, b("TYPE"),
            ])),
        ]);
        let overview = Value::Dict(vec![
            (b("tabsettings_new"), Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab0)])])),
            (b("overviewColumns"), Value::List(vec![Value::Ref(1)])),
        ]);
        let mut user = Value::Dict(vec![(b("overview"), overview)]);
        encode(&user).expect("fixture must encode before the edit");

        set_column_visible(&mut user, 0, "NAME", false).unwrap();

        encode(&user).expect("hiding a Shared-defining column must still encode");
    }

    #[test]
    fn editing_a_legacy_overview_migrates_it_to_modern() {
        use blue_marshal::Value;
        fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        fn ts() -> Value { Value::Long(vec![0u8; 8]) }
        // Legacy: the tab container is `tabsettings` (no `tabsettings_new`).
        let tab0 = Value::Dict(vec![
            (Value::Str("name".into()), Value::Str("PvP".into())),
            (b("tabColumnOrder"), Value::List(vec![b("NAME"), b("TYPE")])),
            (b("tabColumns"), Value::List(vec![b("NAME")])),
        ]);
        let overview = Value::Dict(vec![
            (b("overviewColumnOrder"), Value::List(vec![b("NAME"), b("TYPE")])),
            (b("overviewColumns"), Value::List(vec![b("NAME")])),
            (b("tabsettings"), Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab0)])])),
        ]);
        let mut user = Value::Dict(vec![(b("overview"), overview)]);

        set_column_visible(&mut user, 0, "TYPE", true).unwrap();

        // Container now has tabsettings_new and no tabsettings.
        let mut sh = SharedTable::new();
        collect_shared(&user, &mut sh);
        let Value::Dict(root) = &user else { unreachable!() };
        let ov = find_child(root, b"overview", &sh).and_then(|v| as_dict(v, &sh)).unwrap();
        assert!(find_child(ov, b"tabsettings_new", &sh).is_some(), "migrated to tabsettings_new");
        assert!(find_child(ov, b"tabsettings", &sh).is_none(), "legacy key renamed away");
        // And the edit landed on the (now modern) tab.
        let t = project_overview(&user, None).tabs.into_iter().find(|t| t.index == 0).unwrap();
        assert!(t.columns.iter().find(|c| c.name == "TYPE").unwrap().visible);
    }

    /// A tab that inherits (no own lists) materializes from the account defaults
    /// on first edit, then applies the edit.
    fn user_inheriting_tab() -> Value {
        // The tab owns no column lists; it names a FILTER preset "G" (irrelevant
        // to columns). The account-level lists feed both the read fallback and the
        // edit-side materialize.
        let tab = Value::Dict(vec![
            (Value::Str("name".into()), Value::Str("General".into())),
            (bytes("overview"), bytes("G")),
        ]);
        Value::Dict(vec![(
            bytes("overview"),
            Value::Dict(vec![
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
    fn editing_inheriting_tab_materializes_from_account_default() {
        use blue_marshal::Value;
        fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        fn ts() -> Value { Value::Long(vec![0u8; 8]) }
        let tab0 = Value::Dict(vec![(b("overview"), b("P"))]); // names a FILTER preset; no own column lists
        let overview = Value::Dict(vec![
            (b("overviewColumnOrder"), Value::List(vec![b("NAME"), b("TYPE"), b("DISTANCE")])),
            (b("overviewColumns"), Value::List(vec![b("NAME"), b("TYPE")])),
            (b("tabsettings_new"), Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab0)])])),
        ]);
        let mut user = Value::Dict(vec![(b("overview"), overview)]);

        set_column_visible(&mut user, 0, "DISTANCE", true).unwrap();

        let oc = project_overview(&user, None);
        let t0 = oc.tabs.iter().find(|t| t.index == 0).unwrap();
        assert!(!t0.inherits, "tab now owns its lists (materialized)");
        let visible: Vec<_> = t0.columns.iter().filter(|c| c.visible).map(|c| c.name.clone()).collect();
        assert!(visible.contains(&"DISTANCE".to_string()));
        assert!(visible.contains(&"NAME".to_string()) && visible.contains(&"TYPE".to_string()),
            "account default's visible columns carried into the materialized tab");
        // account default untouched: overview.overviewColumns is still [NAME, TYPE]
        // — materialize copies from it, it must not mutate it.
        let mut sh = SharedTable::new();
        collect_shared(&user, &mut sh);
        let Value::Dict(root) = &user else { unreachable!() };
        let overview = find_child(root, b"overview", &sh).and_then(|v| as_dict(v, &sh)).unwrap();
        assert_eq!(token_list(overview, b"overviewColumns", &sh), Some(vec!["NAME".into(), "TYPE".into()]),
            "the account default column list was not mutated by materialize");
    }

    #[test]
    fn edits_a_tab_in_a_shared_keyed_overview_container() {
        // Mirrors a real file: the `overview` container key and the
        // `tabsettings_new` key are both marshal-deduped into `Shared`s. A bare
        // `is_bytes` key match would miss them and report NoTab; the resolved-key
        // path must find the tab and land the edit.
        use blue_marshal::Value;
        fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        fn ts() -> Value { Value::Long(vec![0u8; 8]) }
        let tab0 = Value::Dict(vec![(b("overview"), b("P"))]); // no own column lists
        let overview = Value::Dict(vec![
            (b("overviewColumnOrder"), Value::List(vec![b("NAME"), b("TYPE")])),
            (b("overviewColumns"), Value::List(vec![b("NAME"), b("TYPE")])),
            (
                Value::Shared { slot: 2, value: Box::new(b("tabsettings_new")) },
                Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab0)])]),
            ),
        ]);
        let mut user = Value::Dict(vec![
            (Value::Shared { slot: 1, value: Box::new(b("overview")) }, overview),
        ]);

        set_column_visible(&mut user, 0, "DISTANCE", true).unwrap();

        let oc = project_overview(&user, None);
        let t0 = oc.tabs.iter().find(|t| t.index == 0).unwrap();
        assert!(!t0.inherits, "the edit materialized the tab despite Shared keys");
        assert!(t0.columns.iter().any(|c| c.visible && c.name == "DISTANCE"),
            "the edit landed on the real tab, not a phantom");
        assert!(t0.columns.iter().any(|c| c.name == "NAME") && t0.columns.iter().any(|c| c.name == "TYPE"),
            "materialized from the account default");
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
    fn set_width_resolves_a_shared_container_key() {
        // Mirrors a real deduped char file: the `ui` and `SortHeadersSizes`
        // CONTAINER keys are marshal-deduped into `Shared`s. A bare `is_bytes`
        // key match misses them and reports NoTab even though `tab_widths`
        // (which resolves keys) reads the widths fine — the write-can't/read-can
        // asymmetry this milestone exists to eliminate.
        let widths = Value::Dict(vec![(bytes("NAME"), Value::Int(120))]);
        let mut c = Value::Dict(vec![(
            Value::Shared { slot: 1, value: Box::new(bytes("ui")) },
            Value::Dict(vec![(
                Value::Shared { slot: 2, value: Box::new(bytes("SortHeadersSizes")) },
                Value::Tuple(vec![ts(), Value::Dict(vec![(
                    Value::Tuple(vec![bytes("overviewScroll2"), Value::Int(0)]),
                    widths,
                )])]),
            )]),
        )]);
        set_column_width(&mut c, 0, "NAME", 200).unwrap();
        assert_eq!(width_of(&c, 0, "NAME"), Some(200), "the width write landed despite Shared container keys");
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

    // --- Ref/Shared-key innermost write paths: no duplicate keys ---------------
    // These assert on the RESULTING TREE, not the projection: the read resolves
    // keys and (for widths) collapses duplicates via HashMap last-wins, so a
    // projection readback MASKS a duplicate malformed key. Only a tree inspection
    // catches the append-instead-of-update-in-place bug on real deduped files.

    /// The recurring column token (NAME) is deduped: the width key is a `Shared`
    /// (first occurrence). A bare `is_bytes` match misses it and appends a
    /// duplicate `Bytes("NAME")`; the resolved-key write must update in place.
    #[test]
    fn set_width_resolves_a_shared_column_key_no_duplicate() {
        let widths = Value::Dict(vec![(
            Value::Shared { slot: 5, value: Box::new(bytes("NAME")) },
            Value::Int(80),
        )]);
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
        set_column_width(&mut c, 0, "NAME", 120).unwrap();

        let mut sh = SharedTable::new();
        collect_shared(&c, &mut sh);
        let Value::Dict(root) = &c else { unreachable!() };
        let ui = find_child(root, b"ui", &sh).and_then(|v| as_dict(v, &sh)).unwrap();
        let sizes = find_child(ui, b"SortHeadersSizes", &sh).and_then(|v| as_dict(v, &sh)).unwrap();
        let (_, cols) = sizes.iter().find(|(k, _)| is_width_key(k, 0, &sh)).unwrap();
        let entries = as_dict(cols, &sh).unwrap();
        let name_entries: Vec<_> =
            entries.iter().filter(|(k, _)| is_bytes(effective(k, &sh), b"NAME")).collect();
        assert_eq!(name_entries.len(), 1, "exactly one NAME entry — no duplicate key");
        assert_eq!(entries.len(), 1, "no phantom entry pushed");
        assert_eq!(as_int(effective(&name_entries[0].1, &sh)), Some(120), "updated in place");
    }

    /// A later tab stores the recurring NAME token as `Ref(slot)` back to a
    /// `Shared("NAME")` defined in an earlier tab's width dict. The write must
    /// resolve the Ref and update in place, not append a duplicate.
    #[test]
    fn set_width_resolves_a_ref_column_key_no_duplicate() {
        let tab0_widths = Value::Dict(vec![(
            Value::Shared { slot: 6, value: Box::new(bytes("NAME")) },
            Value::Int(80),
        )]);
        let tab1_widths = Value::Dict(vec![(Value::Ref(6), Value::Int(90))]);
        let mut c = Value::Dict(vec![(
            bytes("ui"),
            Value::Dict(vec![(
                bytes("SortHeadersSizes"),
                Value::Tuple(vec![ts(), Value::Dict(vec![
                    (Value::Tuple(vec![bytes("overviewScroll2"), Value::Int(0)]), tab0_widths),
                    (Value::Tuple(vec![bytes("overviewScroll2"), Value::Int(1)]), tab1_widths),
                ])]),
            )]),
        )]);
        set_column_width(&mut c, 1, "NAME", 150).unwrap();

        let mut sh = SharedTable::new();
        collect_shared(&c, &mut sh);
        let Value::Dict(root) = &c else { unreachable!() };
        let ui = find_child(root, b"ui", &sh).and_then(|v| as_dict(v, &sh)).unwrap();
        let sizes = find_child(ui, b"SortHeadersSizes", &sh).and_then(|v| as_dict(v, &sh)).unwrap();
        let (_, cols1) = sizes.iter().find(|(k, _)| is_width_key(k, 1, &sh)).unwrap();
        let entries1 = as_dict(cols1, &sh).unwrap();
        assert_eq!(entries1.len(), 1, "tab 1 width dict: exactly one entry, no duplicate NAME");
        assert_eq!(as_int(effective(&entries1[0].1, &sh)), Some(150), "Ref key updated in place");
        // The Shared definition in tab 0 is untouched.
        let (_, cols0) = sizes.iter().find(|(k, _)| is_width_key(k, 0, &sh)).unwrap();
        assert_eq!(as_int(effective(&as_dict(cols0, &sh).unwrap()[0].1, &sh)), Some(80));
    }

    /// A tab owns `tabColumnOrder` / `tabColumns` under Ref/Shared-deduped
    /// field-name keys. A bare `is_bytes` match misses them, so `materialize_from`
    /// pushed a default-seeded DUPLICATE list and the edit landed on the phantom
    /// while the read saw the original. The resolved-key path must edit in place.
    #[test]
    fn edit_resolves_a_shared_field_name_key_no_duplicate() {
        let tab = Value::Dict(vec![
            (Value::Str("name".into()), Value::Str("PvP".into())),
            (
                Value::Shared { slot: 3, value: Box::new(bytes("tabColumnOrder")) },
                Value::List(vec![bytes("NAME"), bytes("TYPE")]),
            ),
            (
                Value::Shared { slot: 4, value: Box::new(bytes("tabColumns")) },
                Value::List(vec![bytes("NAME")]),
            ),
        ]);
        let mut user = Value::Dict(vec![(
            bytes("overview"),
            Value::Dict(vec![(
                bytes("tabsettings_new"),
                Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab)])]),
            )]),
        )]);

        set_column_order(&mut user, 0, &["TYPE".into(), "NAME".into()]).unwrap();

        let mut sh = SharedTable::new();
        collect_shared(&user, &mut sh);
        let Value::Dict(root) = &user else { unreachable!() };
        let overview = find_child(root, b"overview", &sh).and_then(|v| as_dict(v, &sh)).unwrap();
        let tabs = tab_dict(overview, &sh).unwrap();
        let (_, tabv) = tabs.iter().find(|(k, _)| as_int(effective(k, &sh)) == Some(0)).unwrap();
        let fields = as_dict(tabv, &sh).unwrap();
        let count = |name: &[u8]| fields.iter().filter(|(k, _)| is_bytes(effective(k, &sh), name)).count();
        assert_eq!(count(b"tabColumnOrder"), 1, "exactly one tabColumnOrder key — no duplicate");
        assert_eq!(count(b"tabColumns"), 1, "exactly one tabColumns key — no duplicate");
        // The edit landed on the owned (Shared-keyed) list.
        assert_eq!(
            token_list(fields, b"tabColumnOrder", &sh),
            Some(vec!["TYPE".into(), "NAME".into()]),
            "reorder edited the owned list in place",
        );
    }

    /// A customized tab OWNS `tabColumns` / `tabColumnOrder` whose ITEMS are
    /// `Shared`/`Ref` (as real files store them). The old `set_column_visible`
    /// compared items with bare `==` against a `Bytes` token, so it MISSED the
    /// Shared/Ref item: hiding a visible one was a no-op (item left in place) and
    /// showing one already in the order list pushed a DUPLICATE. Resolving each
    /// item's `effective` value first fixes both — asserted on the TREE, since the
    /// projection resolves keys and masks a duplicate item.
    #[test]
    fn toggle_resolves_shared_ref_list_items_no_duplicate() {
        // order (encoded first) defines the token Shareds; the visible list's
        // Ref(10) then resolves to NAME, and DISTANCE is a Shared def of its own.
        fn owning_tab() -> Value {
            let tab = Value::Dict(vec![
                (Value::Str("name".into()), Value::Str("PvP".into())),
                (bytes("tabColumnOrder"), Value::List(vec![
                    Value::Shared { slot: 10, value: Box::new(bytes("NAME")) },
                    Value::Shared { slot: 12, value: Box::new(bytes("TYPE")) },
                ])),
                (bytes("tabColumns"), Value::List(vec![
                    Value::Ref(10),                                                  // NAME (Ref item)
                    Value::Shared { slot: 14, value: Box::new(bytes("DISTANCE")) },  // DISTANCE (Shared item)
                ])),
            ]);
            Value::Dict(vec![(
                bytes("overview"),
                Value::Dict(vec![(
                    bytes("tabsettings_new"),
                    Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab)])]),
                )]),
            )])
        }
        fn count_resolving(fields: &Entries, sh: &SharedTable, list: &[u8], col: &[u8]) -> usize {
            let items = as_list_r(find_child(fields, list, sh).unwrap(), sh).unwrap();
            items.iter().filter(|it| is_bytes(effective(it, sh), col)).count()
        }
        fn tab0_fields<'a>(user: &'a Value, sh: &SharedTable<'a>) -> &'a Entries {
            let Value::Dict(root) = user else { unreachable!() };
            let overview = find_child(root, b"overview", sh).and_then(|v| as_dict(v, sh)).unwrap();
            let tabs = tab_dict(overview, sh).unwrap();
            let (_, tabv) = tabs.iter().find(|(k, _)| as_int(effective(k, sh)) == Some(0)).unwrap();
            as_dict(tabv, sh).unwrap()
        }

        // Hide an already-visible Shared item (DISTANCE): it must be REMOVED, not
        // duplicated/left behind.
        let mut user = owning_tab();
        set_column_visible(&mut user, 0, "DISTANCE", false).unwrap();
        let mut sh = SharedTable::new();
        collect_shared(&user, &mut sh);
        let fields = tab0_fields(&user, &sh);
        let vis = as_list_r(find_child(fields, b"tabColumns", &sh).unwrap(), &sh).unwrap();
        assert_eq!(vis.len(), 1, "the Shared DISTANCE item was removed (list 2 -> 1), not missed");
        assert_eq!(count_resolving(fields, &sh, b"tabColumns", b"DISTANCE"), 0, "no DISTANCE left in tabColumns");
        assert_eq!(count_resolving(fields, &sh, b"tabColumns", b"NAME"), 1, "the NAME Ref item is untouched");

        // Show a not-visible column that already exists in tabColumnOrder as a
        // Shared item (TYPE): exactly ONE entry in each list, no duplicate.
        let mut user = owning_tab();
        set_column_visible(&mut user, 0, "TYPE", true).unwrap();
        let mut sh = SharedTable::new();
        collect_shared(&user, &mut sh);
        let fields = tab0_fields(&user, &sh);
        assert_eq!(count_resolving(fields, &sh, b"tabColumns", b"TYPE"), 1, "exactly one TYPE in tabColumns");
        assert_eq!(count_resolving(fields, &sh, b"tabColumnOrder", b"TYPE"), 1,
            "TYPE was already in the order list (as a Shared item) — not duplicated");
        let (_, visible) = tab_lists(&user, 0);
        assert!(visible.contains(&"TYPE".to_string()), "TYPE now reads back as visible");
    }

    #[test]
    fn project_tab_exposes_preset_name() {
        // A user tree: overview -> tabsettings_new (ts,dict) -> {0: {name, overview:"P"}}
        let tab = Value::Dict(vec![
            (Value::Str("name".into()), Value::Str("Main".into())),
            (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
        ]);
        let overview = Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()),
             Value::Dict(vec![(Value::Int(0), tab)])),
        ]);
        let user = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), overview)]);

        let cols = project_overview(&user, None);
        assert_eq!(cols.tabs.len(), 1);
        assert_eq!(cols.tabs[0].preset, "P");
    }

    #[test]
    fn project_exposes_sorted_preset_names() {
        fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        // overview -> overviewProfilePresets -> (ts, { "Zeta": {}, "alpha": {} })
        let presets = Value::Dict(vec![
            (b("Zeta"), Value::Dict(vec![])),
            (b("alpha"), Value::Dict(vec![])),
        ]);
        let overview = Value::Dict(vec![
            (b("overviewProfilePresets"), Value::Tuple(vec![Value::Int(1), presets])),
        ]);
        let user = Value::Dict(vec![(b("overview"), overview)]);
        let cols = project_overview(&user, None);
        assert_eq!(
            cols.presets.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
            vec!["alpha".to_string(), "Zeta".to_string()]
        );
    }
}
