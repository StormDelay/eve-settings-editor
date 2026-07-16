//! Read + edit projection of the autofill / remembered-text category. All of it
//! lives in `core_user` under `ui -> editHistory -> (timestamp, dict)`, where the
//! dict maps a UI widget path (Bytes) to a list of remembered strings (Str, with
//! the occasional empty Bytes). Reads resolve Ref/Shared and unwrap the
//! (ts, dict)/(ts, list) wrappers; edits inline all sharing first (see the write
//! functions in Task 2) so replacing a list can never dangle a Ref.

use blue_marshal::Value;
use serde::Serialize;

use crate::treewalk::{collect_shared, effective, Entries, SharedTable};

#[derive(Debug, Serialize, PartialEq)]
pub struct RememberedList {
    pub widget: String,
    pub entries: Vec<String>,
}

pub fn project_edit_history(user: &Value) -> Vec<RememberedList> {
    let mut sh = SharedTable::new();
    collect_shared(user, &mut sh);
    let Value::Dict(root) = effective(user, &sh) else { return vec![] };
    let Some(ui) = find_child(root, b"ui", &sh).and_then(|v| as_dict(v, &sh)) else { return vec![] };
    let Some(eh) = find_child(ui, b"editHistory", &sh).and_then(|v| as_dict(v, &sh)) else { return vec![] };
    eh.iter()
        .filter_map(|(k, v)| {
            let widget = bytes_str(effective(k, &sh))?;
            let entries = as_list(v, &sh)?.iter().map(|e| entry_str(effective(e, &sh))).collect();
            Some(RememberedList { widget, entries })
        })
        .collect()
}

// ponytail: these four resolvers duplicate overview.rs's private copies rather
// than lifting them into treewalk — overview.rs is the repo's most-delicate code
// (mis-modeled three times) and not worth re-touching for ~20 shared lines.

/// Value of the entry whose RESOLVED key is `Bytes(name)`, itself resolved.
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
fn as_list<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<&'a Vec<Value>> {
    match effective(v, sh) {
        Value::List(l) => Some(l),
        Value::Tuple(items) => items.iter().find_map(|e| match effective(e, sh) {
            Value::List(l) => Some(l),
            _ => None,
        }),
        _ => None,
    }
}

fn bytes_str(v: &Value) -> Option<String> {
    match v {
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        _ => None,
    }
}

/// Coerce a remembered entry to a display string. Entries are Str; the
/// occasional empty Bytes becomes "" (see the module doc); anything else "".
fn entry_str(v: &Value) -> String {
    match v {
        Value::Str(s) | Value::StrUcs2(s) => s.clone(),
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
    fn ts() -> Value { Value::Long(vec![0u8; 8]) }

    /// user root -> b"ui" -> b"editHistory" -> (ts, { widget: [entries] })
    fn user_with_history() -> Value {
        let hist = Value::Dict(vec![
            (b("/addressbook/.../SingleLineEditText"),
             Value::List(vec![Value::Str("Jita".into()), Value::Str("Amarr".into())])),
            (b("/inventory/.../quickFilter"), Value::List(vec![Value::Str("veldspar".into())])),
        ]);
        let ui = Value::Dict(vec![(b("editHistory"), Value::Tuple(vec![ts(), hist]))]);
        Value::Dict(vec![(b("ui"), ui)])
    }

    #[test]
    fn projects_widget_lists_in_order() {
        let lists = project_edit_history(&user_with_history());
        assert_eq!(lists.len(), 2);
        assert_eq!(lists[0].widget, "/addressbook/.../SingleLineEditText");
        assert_eq!(lists[0].entries, vec!["Jita", "Amarr"]);
        assert_eq!(lists[1].entries, vec!["veldspar"]);
    }

    #[test]
    fn a_file_without_edit_history_projects_empty() {
        assert!(project_edit_history(&Value::Dict(vec![])).is_empty());
    }

    #[test]
    fn resolves_ref_shared_keys_values_and_coerces_empty_bytes() {
        // Real idiom: the `editHistory` VALUE list holds a Shared-deduped string
        // and a bare Ref to it; a widget list also carries an empty Bytes entry.
        let jita = Value::Shared { slot: 1, value: Box::new(Value::Str("Jita".into())) };
        let hist = Value::Dict(vec![
            (b("/a/box"), Value::List(vec![jita, Value::Bytes(vec![])])), // "Jita", ""
            (b("/b/box"), Value::List(vec![Value::Ref(1)])),              // -> "Jita"
        ]);
        let ui = Value::Dict(vec![(b("editHistory"), Value::Tuple(vec![ts(), hist]))]);
        let user = Value::Dict(vec![(b("ui"), ui)]);
        let lists = project_edit_history(&user);
        assert_eq!(lists[0].entries, vec!["Jita", ""], "Shared resolved, empty Bytes -> \"\"");
        assert_eq!(lists[1].entries, vec!["Jita"], "Ref resolved to the Shared value");
    }
}
