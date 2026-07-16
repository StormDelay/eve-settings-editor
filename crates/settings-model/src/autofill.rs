//! Read + edit projection of the autofill / remembered-text category. All of it
//! lives in `core_user` under `ui -> editHistory -> (timestamp, dict)`, where the
//! dict maps a UI widget path (Bytes) to a list of remembered strings (Str, with
//! the occasional empty Bytes). Reads resolve Ref/Shared and unwrap the
//! (ts, dict)/(ts, list) wrappers; edits inline all sharing first (see the write
//! functions in Task 2) so replacing a list can never dangle a Ref.

use blue_marshal::Value;
use serde::Serialize;

use crate::treewalk::{collect_shared, effective, inline_all, is_bytes, Entries, SharedTable};

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

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum AutofillError {
    /// The file has no `ui -> editHistory` structure at all.
    NoHistory,
    /// No remembered-string list for that widget path.
    NoList,
}

/// Replace one widget's remembered-string list with `entries` (written as Str).
/// An empty slice clears the list. Inlines all sharing first so the wholesale
/// replacement cannot dangle a Ref (see `inline_all`).
pub fn set_list_entries(user: &mut Value, widget: &str, entries: &[String]) -> Result<(), AutofillError> {
    inline_all(user);
    let eh = edit_history_mut(user).ok_or(AutofillError::NoHistory)?;
    let (_, v) = eh.iter_mut().find(|(k, _)| is_bytes(k, widget.as_bytes())).ok_or(AutofillError::NoList)?;
    let list = list_inner_mut(v).ok_or(AutofillError::NoList)?;
    *list = entries.iter().map(|s| Value::Str(s.clone())).collect();
    Ok(())
}

/// Mutable inner dict of root -> ui -> editHistory -> (ts, dict). Assumes a plain
/// tree (post-`inline_all`), so keys are plain Bytes and values plain wrappers.
fn edit_history_mut(user: &mut Value) -> Option<&mut Entries> {
    let Value::Dict(root) = user else { return None };
    let ui = child_dict_mut(root, b"ui")?;
    child_dict_mut(ui, b"editHistory")
}

fn child_dict_mut<'a>(dict: &'a mut Entries, name: &[u8]) -> Option<&'a mut Entries> {
    let (_, v) = dict.iter_mut().find(|(k, _)| is_bytes(k, name))?;
    dict_inner_mut(v)
}

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

/// Empty every remembered-string list in the file (widget keys kept). A no-op
/// success when the file has no editHistory. Inlines sharing first, like
/// `set_list_entries`, so a Shared entry never leaves a dangling Ref.
pub fn clear_all_history(user: &mut Value) -> Result<(), AutofillError> {
    inline_all(user);
    let Some(eh) = edit_history_mut(user) else { return Ok(()) };
    for (_, v) in eh.iter_mut() {
        if let Some(list) = list_inner_mut(v) {
            list.clear();
        }
    }
    Ok(())
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

    #[test]
    fn set_list_entries_replaces_a_widget_list() {
        let mut user = user_with_history();
        set_list_entries(&mut user, "/inventory/.../quickFilter", &["scordite".into(), "pyroxeres".into()]).unwrap();
        let lists = project_edit_history(&user);
        let l = lists.iter().find(|l| l.widget == "/inventory/.../quickFilter").unwrap();
        assert_eq!(l.entries, vec!["scordite", "pyroxeres"]);
    }

    #[test]
    fn set_list_entries_can_clear_and_reports_missing() {
        let mut user = user_with_history();
        set_list_entries(&mut user, "/inventory/.../quickFilter", &[]).unwrap();
        let l = project_edit_history(&user).into_iter().find(|l| l.widget == "/inventory/.../quickFilter").unwrap();
        assert!(l.entries.is_empty());
        assert_eq!(set_list_entries(&mut user, "/nope", &["x".into()]), Err(AutofillError::NoList));
        assert_eq!(set_list_entries(&mut Value::Dict(vec![]), "/a", &[]), Err(AutofillError::NoHistory));
    }

    #[test]
    fn clearing_a_list_with_a_shared_entry_still_encodes() {
        // Real idiom: an identical remembered string in two widget lists is
        // deduped into one Shared, Ref'd from the other list. Replacing the list
        // that holds the Shared DEFINITION would dangle the Ref (RefBeforeStore on
        // encode) — inline_all before the edit prevents it. This is exactly the
        // case the raw apply_mutation refuses (SharedSubtree), which is why this
        // milestone uses a dedicated inline-first write.
        use blue_marshal::{decode, encode};
        // Shared payload must be Bytes: the encoder's storable_with_flag only
        // allows Bytes/Long/List/Dict/Tuple/etc under a Shared flag, never Str
        // (matches the Bytes-only convention windows.rs/overview.rs already use
        // for their Shared fixtures) — Str there is EncodeErrorKind::NotStorable.
        let jita = Value::Shared { slot: 1, value: Box::new(Value::Bytes(b"Jita".to_vec())) };
        let hist = Value::Dict(vec![
            (b("/a/box"), Value::List(vec![jita, Value::Str("Amarr".into())])),
            (b("/b/box"), Value::List(vec![Value::Ref(1)])),
        ]);
        let ui = Value::Dict(vec![(b("editHistory"), Value::Tuple(vec![ts(), hist]))]);
        let mut user = Value::Dict(vec![(b("ui"), ui)]);
        encode(&user).expect("fixture must encode before the edit");

        set_list_entries(&mut user, "/a/box", &[]).unwrap(); // clears the Shared def holder

        let bytes = encode(&user).expect("edited tree must still encode (no dangling Ref)");
        let lists = project_edit_history(&decode(&bytes).unwrap());
        assert!(lists.iter().find(|l| l.widget == "/a/box").unwrap().entries.is_empty());
        assert_eq!(lists.iter().find(|l| l.widget == "/b/box").unwrap().entries, vec!["Jita"],
            "widget B keeps its formerly-Ref'd value, now inlined");
    }

    #[test]
    fn clear_all_history_empties_every_list() {
        let mut user = user_with_history();
        let widgets_before: Vec<String> = project_edit_history(&user).into_iter().map(|l| l.widget).collect();
        clear_all_history(&mut user).unwrap();
        let lists = project_edit_history(&user);
        assert_eq!(lists.len(), 2, "widget keys are kept");
        assert_eq!(lists.iter().map(|l| l.widget.clone()).collect::<Vec<_>>(), widgets_before, "widget paths unchanged");
        assert!(lists.iter().all(|l| l.entries.is_empty()), "every list emptied");
    }

    #[test]
    fn clear_all_history_is_a_noop_without_edit_history() {
        assert_eq!(clear_all_history(&mut Value::Dict(vec![])), Ok(()));
    }

    #[test]
    fn clear_all_history_survives_shared_entries() {
        // The Shared DEFINITION lives inside editHistory (which clear_all_history
        // clears); the Ref to it lives OUTSIDE editHistory, as a sibling of it
        // under "ui" — a spot clear_all_history never touches. So this only
        // encodes after clearing if inline_all resolved that outside Ref to an
        // owned copy first: without it, clearing drops the Shared def, the
        // outside Ref dangles, and encode fails RefBeforeStore. Shared payload
        // must be Bytes (marshal never shares Str — see the sibling test above).
        use blue_marshal::encode;
        let jita = Value::Shared { slot: 1, value: Box::new(Value::Bytes(b"Jita".to_vec())) };
        let hist = Value::Dict(vec![
            (b("/a/box"), Value::List(vec![jita, Value::Str("Amarr".into())])),
        ]);
        let ui = Value::Dict(vec![
            (b("editHistory"), Value::Tuple(vec![ts(), hist])), // def, encoded first
            (b("otherWidgetRef"), Value::List(vec![Value::Ref(1)])), // outside editHistory
        ]);
        let mut user = Value::Dict(vec![(b("ui"), ui)]);
        encode(&user).expect("fixture must encode before the edit (def precedes ref)");

        clear_all_history(&mut user).unwrap();

        encode(&user).expect("cleared tree must still encode (outside Ref was inlined, not dangled)");
        assert!(project_edit_history(&user).iter().all(|l| l.entries.is_empty()));
    }
}
