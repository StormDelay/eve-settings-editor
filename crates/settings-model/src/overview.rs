//! Read + edit projection of the overview-columns category. Visibility and
//! order live in the `core_user` file (per overview tab, with account-default
//! inheritance); widths live in the `core_char` file (per tab). All EVE format
//! knowledge (the `(timestamp, dict)` wrappers, the `(overviewScroll2, tab)`
//! width key, column tokens as Bytes) lives here so the UI stays format-blind.
//! Dict traversal reuses the shared `crate::treewalk` helpers.

use blue_marshal::Value;
use serde::Serialize;

use crate::path::NodePath;
use crate::treewalk::{child_dict, is_bytes, timestamped_dict, unwrap_shared_ref, Entries};

#[derive(Debug, Serialize, PartialEq)]
pub struct OverviewColumns {
    pub tabs: Vec<OverviewTab>,
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
    let tabs = tab_settings(user)
        .map(|(dict, _)| dict.iter().filter_map(|(k, v)| project_tab(k, v, user, char_tree)).collect())
        .unwrap_or_default();
    OverviewColumns { tabs }
}

fn project_tab(key: &Value, tab: &Value, user: &Value, char_tree: Option<&Value>) -> Option<OverviewTab> {
    let index = as_int(key)?;
    let Value::Dict(fields) = unwrap_shared_ref(tab) else { return None };
    let name = str_field(fields, "name").unwrap_or_else(|| format!("Tab {index}"));

    let own_order = list_field(fields, b"tabColumnOrder");
    let own_visible = list_field(fields, b"tabColumns");
    let inherits = own_order.is_none() && own_visible.is_none();

    // Effective order/visible: the tab's own lists, else the account defaults.
    let (order, visible) = match (own_order, own_visible) {
        (Some(o), v) => (o, v.unwrap_or_default()),
        (None, Some(v)) => (v.clone(), v),
        (None, None) => account_defaults(user),
    };
    let widths = char_tree.and_then(|c| tab_widths(c, index));

    let columns = order
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

/// Account-level defaults: (overviewColumnOrder, overviewColumns) as token lists.
fn account_defaults(user: &Value) -> (Vec<String>, Vec<String>) {
    let Some((ov, _)) = child_dict(user, b"overview", Vec::new()) else { return (vec![], vec![]) };
    let order = list_field(ov, b"overviewColumnOrder").unwrap_or_default();
    let visible = list_field(ov, b"overviewColumns").unwrap_or_default();
    (order, visible)
}

/// Widths for a tab: column token -> px, from char root -> ui -> SortHeadersSizes.
fn tab_widths(char_tree: &Value, tab_index: i64) -> Option<std::collections::HashMap<String, i64>> {
    let (ui, ui_path) = child_dict(char_tree, b"ui", Vec::new())?;
    let (sizes, _) = timestamped_dict(ui, &ui_path, b"SortHeadersSizes")?;
    let (_, cols) = sizes.iter().find(|(k, _)| is_width_key(k, tab_index))?;
    let Value::Dict(entries) = unwrap_shared_ref(cols) else { return None };
    Some(
        entries
            .iter()
            .filter_map(|(k, v)| Some((token(k)?, as_int(v)?)))
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
    match v {
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        _ => None,
    }
}

fn str_field(fields: &Entries, name: &str) -> Option<String> {
    fields.iter().find_map(|(k, v)| match k {
        Value::Str(s) | Value::StrUcs2(s) if s == name => match v {
            Value::Str(t) | Value::StrUcs2(t) => Some(t.clone()),
            _ => None,
        },
        _ => None,
    })
}

/// A list-of-Bytes field (tabColumns / tabColumnOrder / overviewColumns…) as tokens.
fn list_field(fields: &Entries, name: &[u8]) -> Option<Vec<String>> {
    let (_, v) = fields.iter().find(|(k, _)| is_bytes(k, name))?;
    let Value::List(items) = unwrap_shared_ref(v) else { return None };
    Some(items.iter().filter_map(token).collect())
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
}
