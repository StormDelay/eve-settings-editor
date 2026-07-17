//! Batch apply: extract a projection category's subtree from one document and
//! splice it into another. The category subtree is the VALUE at a fixed key
//! path — `windows` (char file) or `ui -> editHistory` (user file). Extract
//! inlines the source's sharing first so a Ref inside the category that points
//! at a Shared defined elsewhere resolves; splice inlines the target's sharing
//! first so replacing the subtree can never dangle a Ref the rest of the file
//! still holds (the proven autofill.rs / overview.rs inline-first idiom).

use blue_marshal::Value;
use serde::{Deserialize, Serialize};

use crate::treewalk::{inline_all, is_bytes, Entries};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Layout,
    Autofill,
}

impl Category {
    /// Key path from the document root to this category's subtree VALUE.
    fn key_path(self) -> &'static [&'static [u8]] {
        match self {
            Category::Layout => &[b"windows"],
            Category::Autofill => &[b"ui", b"editHistory"],
        }
    }
}

/// Inline the source's sharing, then clone each requested category's subtree.
/// Categories the source lacks are skipped (absent from the result).
pub fn extract_categories(source: &Value, cats: &[Category]) -> Vec<(Category, Value)> {
    let mut s = source.clone();
    inline_all(&mut s);
    let Value::Dict(root) = &s else { return Vec::new() };
    cats.iter()
        .filter_map(|&cat| {
            let keys = cat.key_path();
            let (parent_keys, last) = keys.split_at(keys.len() - 1);
            let parent = descend_ref(root, parent_keys)?;
            let (_, v) = parent.iter().find(|(k, _)| is_bytes(k, last[0]))?;
            Some((cat, v.clone()))
        })
        .collect()
}

/// Inline the target's sharing, then replace (or insert) each category's subtree.
/// A missing intermediate parent dict (e.g. no `ui`) skips that category.
pub fn apply_to_tree(target: &mut Value, extracted: &[(Category, Value)]) {
    inline_all(target);
    let Value::Dict(root) = target else { return };
    for (cat, subtree) in extracted {
        let keys = cat.key_path();
        let (parent_keys, last) = keys.split_at(keys.len() - 1);
        let Some(parent) = descend_mut(root, parent_keys) else { continue };
        match parent.iter_mut().find(|(k, _)| is_bytes(k, last[0])) {
            Some((_, v)) => *v = subtree.clone(),
            None => parent.push((Value::Bytes(last[0].to_vec()), subtree.clone())),
        }
    }
}

/// Inner dict of a plain (post-inline) value, unwrapping a `(ts, dict)` tuple.
fn dict_inner(v: &Value) -> Option<&Entries> {
    match v {
        Value::Dict(d) => Some(d),
        Value::Tuple(items) => items.iter().find_map(|e| match e {
            Value::Dict(d) => Some(d),
            _ => None,
        }),
        _ => None,
    }
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

fn descend_ref<'a>(root: &'a Entries, keys: &[&[u8]]) -> Option<&'a Entries> {
    let mut cur = root;
    for &key in keys {
        let (_, v) = cur.iter().find(|(k, _)| is_bytes(k, key))?;
        cur = dict_inner(v)?;
    }
    Some(cur)
}

fn descend_mut<'a>(root: &'a mut Entries, keys: &[&[u8]]) -> Option<&'a mut Entries> {
    let mut cur = root;
    for &key in keys {
        let (_, v) = cur.iter_mut().find(|(k, _)| is_bytes(k, key))?;
        cur = dict_inner_mut(v)?;
    }
    Some(cur)
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::{decode, encode};

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
    fn ts() -> Value { Value::Long(vec![0u8; 8]) }

    /// user root -> ui -> editHistory -> (ts, { "/a": ["Jita"] })
    fn user_a() -> Value {
        let hist = Value::Dict(vec![(b("/a"), Value::List(vec![Value::Str("Jita".into())]))]);
        let ui = Value::Dict(vec![(b("editHistory"), Value::Tuple(vec![ts(), hist]))]);
        Value::Dict(vec![(b("ui"), ui)])
    }

    /// user root -> ui -> editHistory -> (ts, { "/b": ["Amarr"] }) plus a sibling key.
    fn user_b() -> Value {
        let hist = Value::Dict(vec![(b("/b"), Value::List(vec![Value::Str("Amarr".into())]))]);
        let ui = Value::Dict(vec![(b("editHistory"), Value::Tuple(vec![ts(), hist]))]);
        Value::Dict(vec![(b("ui"), ui), (b("keep"), Value::Int(7))])
    }

    #[test]
    fn extract_then_apply_replaces_the_category_and_keeps_siblings() {
        let extracted = extract_categories(&user_a(), &[Category::Autofill]);
        assert_eq!(extracted.len(), 1);
        let mut target = user_b();
        apply_to_tree(&mut target, &extracted);

        // The autofill category is now A's; the unrelated sibling survived.
        let lists = crate::autofill::project_edit_history(&target);
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].widget, "/a");
        assert_eq!(lists[0].entries, vec!["Jita"]);
        let Value::Dict(root) = &target else { panic!() };
        assert!(root.iter().any(|(k, v)| is_bytes(k, b"keep") && matches!(v, Value::Int(7))));
    }

    #[test]
    fn apply_inserts_the_category_when_the_target_lacks_it() {
        let extracted = extract_categories(&user_a(), &[Category::Autofill]);
        // Target has a `ui` dict but no editHistory entry.
        let mut target = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
        apply_to_tree(&mut target, &extracted);
        let lists = crate::autofill::project_edit_history(&target);
        assert_eq!(lists[0].entries, vec!["Jita"]);
    }

    #[test]
    fn extract_resolves_a_ref_into_a_shared_defined_outside_the_category() {
        // The category's list holds a Ref; the Shared it points at is defined
        // OUTSIDE editHistory. Without inlining the whole source first, the
        // extracted subtree would carry a dangling Ref that fails to encode.
        let jita = Value::Shared { slot: 1, value: Box::new(Value::Bytes(b"Jita".to_vec())) };
        let hist = Value::Dict(vec![(b("/a"), Value::List(vec![Value::Ref(1)]))]);
        let ui = Value::Dict(vec![
            (b("shareDef"), Value::List(vec![jita])), // Shared def, sibling of editHistory
            (b("editHistory"), Value::Tuple(vec![ts(), hist])),
        ]);
        let source = Value::Dict(vec![(b("ui"), ui)]);
        encode(&source).expect("fixture encodes (def precedes ref)");

        let extracted = extract_categories(&source, &[Category::Autofill]);
        // Put the extracted subtree in a bare target and prove it encodes alone.
        let mut target = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
        apply_to_tree(&mut target, &extracted);
        let bytes = encode(&target).expect("extracted subtree has no dangling Ref");
        let lists = crate::autofill::project_edit_history(&decode(&bytes).unwrap());
        assert_eq!(lists[0].entries, vec!["Jita"]);
    }

    #[test]
    fn apply_inlines_the_target_so_an_outside_ref_into_the_old_category_survives() {
        // Target: the OLD editHistory holds a Shared def; a sibling Ref points at
        // it. Replacing editHistory drops the def — so apply_to_tree must inline
        // the target first or the sibling Ref dangles on encode.
        let jita = Value::Shared { slot: 1, value: Box::new(Value::Bytes(b"Jita".to_vec())) };
        let old_hist = Value::Dict(vec![(b("/old"), Value::List(vec![jita]))]);
        let ui = Value::Dict(vec![
            (b("editHistory"), Value::Tuple(vec![ts(), old_hist])), // def, encoded first
            (b("sibling"), Value::List(vec![Value::Ref(1)])),       // ref outside the category
        ]);
        let mut target = Value::Dict(vec![(b("ui"), ui)]);
        encode(&target).expect("target fixture encodes before the splice");

        let extracted = extract_categories(&user_a(), &[Category::Autofill]);
        apply_to_tree(&mut target, &extracted);
        encode(&target).expect("post-splice target encodes (outside Ref inlined, not dangled)");
    }
}
