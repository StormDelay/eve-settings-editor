//! Batch apply: extract a projection category's subtree from one document and
//! splice it into another. The category subtree is the VALUE at a fixed key
//! path — `windows` (char file) or `ui -> editHistory` (user file). Extract
//! inlines the source's sharing first so a Ref inside the category that points
//! at a Shared defined elsewhere resolves; splice inlines the target's sharing
//! first so replacing the subtree can never dangle a Ref the rest of the file
//! still holds (the proven autofill.rs / overview.rs inline-first idiom).

use blue_marshal::Value;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::document::{Document, LoadError};
use crate::save::{save, SaveReport};
use crate::treewalk::{inline_all, is_bytes, Entries};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Layout,
    Autofill,
    Overview,
    OverviewWidths,
}

impl Category {
    /// Key path from the document root to this category's subtree VALUE.
    fn key_path(self) -> &'static [&'static [u8]] {
        match self {
            Category::Layout => &[b"windows"],
            Category::Autofill => &[b"ui", b"editHistory"],
            Category::Overview => &[b"overview"],
            Category::OverviewWidths => &[b"ui", b"SortHeadersSizes"],
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
    if let Value::Dict(root) = target {
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
    // Re-derive compact immutable-only sharing so the saved file is not the
    // ~1.5x fully-inlined blob (no reliance on EVE re-deduplicating).
    *target = blue_marshal::reshare(target);
}

/// Back up `target`, then atomically overwrite it with `source_bytes`. Byte-for-
/// byte; the source is already a valid file. Returns the backup path.
pub fn full_copy_to(source_bytes: &[u8], target: &Path) -> Result<PathBuf, String> {
    let backup = crate::save::backup_current(target)?;
    crate::save::atomic_write(target, source_bytes)?;
    Ok(backup)
}

/// Load `target`, splice each extracted category in, and run the full save chain
/// (encode -> verify -> backup -> atomic write; ReadOnly targets are refused).
/// `force_conflict = true`: the target is loaded fresh in this call, so there is
/// no genuine conflict to guard against.
pub fn apply_categories_to(
    target: &Path,
    extracted: &[(Category, Value)],
) -> Result<SaveReport, String> {
    let mut doc = Document::load(target).map_err(|e| match e {
        LoadError::Io(m) => format!("Io: {m}"),
        LoadError::Decode { message, .. } => format!("Decode: {message}"),
    })?;
    apply_to_tree(&mut doc.value, extracted);
    save(&mut doc, true).map_err(|e| format!("{e:?}"))
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

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("batch-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn full_copy_overwrites_bytes_and_backs_up() {
        let dir = temp_dir("full");
        let src = dir.join("core_char_1.dat");
        let dst = dir.join("core_char_2.dat");
        let src_bytes = encode(&user_a()).unwrap();
        let dst_bytes = encode(&user_b()).unwrap();
        std::fs::write(&src, &src_bytes).unwrap();
        std::fs::write(&dst, &dst_bytes).unwrap();

        let backup = full_copy_to(&src_bytes, &dst).unwrap();
        assert!(backup.exists(), "target backed up before overwrite");
        assert_eq!(
            std::fs::read(&backup).unwrap(),
            dst_bytes,
            "backup captured target's pre-overwrite bytes"
        );
        assert_eq!(std::fs::read(&dst).unwrap(), src_bytes, "target now byte-identical to source");
    }

    #[test]
    fn category_apply_replaces_only_the_category_on_disk() {
        let dir = temp_dir("cat");
        let dst = dir.join("core_user_2.dat");
        std::fs::write(&dst, encode(&user_b()).unwrap()).unwrap();

        let extracted = extract_categories(&user_a(), &[Category::Autofill]);
        let report = apply_categories_to(&dst, &extracted).unwrap();
        assert!(report.backup_path.exists());

        let reread = decode(&std::fs::read(&dst).unwrap()).unwrap();
        let lists = crate::autofill::project_edit_history(&reread);
        assert_eq!(lists[0].widget, "/a", "category came from the source");
        let Value::Dict(root) = &reread else { panic!() };
        assert!(root.iter().any(|(k, _)| is_bytes(k, b"keep")), "sibling key preserved on disk");
    }

    #[test]
    fn category_apply_refuses_a_read_only_target() {
        // A non-canonical stream (INT8-encoded 1) loads ReadOnly; save refuses it.
        let dir = temp_dir("ro");
        let dst = dir.join("core_user_3.dat");
        std::fs::write(&dst, [0x7E, 0, 0, 0, 0, 0x06, 0x01]).unwrap();
        let extracted = extract_categories(&user_a(), &[Category::Autofill]);
        let err = apply_categories_to(&dst, &extracted).unwrap_err();
        assert!(err.contains("ReadOnly"), "read-only target surfaced as an error: {err}");
    }

    /// user root -> overview -> { overviewColumns: ["NAME"], tabsByWindowInstanceID: [[0]] }
    fn user_overview(col: &str) -> Value {
        let overview = Value::Dict(vec![
            (b("overviewColumns"), Value::List(vec![b(col)])),
            (b("tabsByWindowInstanceID"), Value::List(vec![Value::List(vec![Value::Int(0)])])),
        ]);
        Value::Dict(vec![(b("overview"), overview), (b("keep"), Value::Int(7))])
    }

    /// char root -> ui -> SortHeadersSizes -> (ts, { (overviewScroll2, 0): { NAME: w } })
    fn char_widths(w: i64) -> Value {
        let cols = Value::Dict(vec![(b("NAME"), Value::Int(w))]);
        let sizes = Value::Dict(vec![(
            Value::Tuple(vec![b("overviewScroll2"), Value::Int(0)]),
            cols,
        )]);
        let ui = Value::Dict(vec![(b("SortHeadersSizes"), Value::Tuple(vec![ts(), sizes]))]);
        Value::Dict(vec![(b("ui"), ui), (b("other"), Value::Int(9))])
    }

    #[test]
    fn overview_category_replaces_the_overview_subtree_and_keeps_siblings() {
        let extracted = extract_categories(&user_overview("SOURCECOL"), &[Category::Overview]);
        assert_eq!(extracted.len(), 1);
        let mut target = user_overview("TARGETCOL");
        apply_to_tree(&mut target, &extracted);

        // The overview subtree is now the source's: overviewColumns == ["SOURCECOL"].
        let Value::Dict(root) = &target else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_bytes(k, b"overview")).unwrap();
        let Value::Dict(ov) = ov else { panic!() };
        let (_, cols) = ov.iter().find(|(k, _)| is_bytes(k, b"overviewColumns")).unwrap();
        assert_eq!(cols, &Value::List(vec![b("SOURCECOL")]), "overview came from the source");
        assert!(root.iter().any(|(k, v)| is_bytes(k, b"keep") && matches!(v, Value::Int(7))),
            "unrelated sibling survived");
    }

    #[test]
    fn overview_widths_category_replaces_sortheaderssizes_and_keeps_siblings() {
        let extracted = extract_categories(&char_widths(120), &[Category::OverviewWidths]);
        assert_eq!(extracted.len(), 1);
        let mut target = char_widths(999);
        apply_to_tree(&mut target, &extracted);

        // The width came from the source: NAME == 120, not the target's 999.
        let Value::Dict(root) = &target else { panic!() };
        let (_, ui) = root.iter().find(|(k, _)| is_bytes(k, b"ui")).unwrap();
        let Value::Dict(ui) = ui else { panic!() };
        let (_, shs) = ui.iter().find(|(k, _)| is_bytes(k, b"SortHeadersSizes")).unwrap();
        let Value::Tuple(items) = shs else { panic!() };
        let Value::Dict(sizes) = &items[1] else { panic!() };
        let Value::Dict(cols) = &sizes[0].1 else { panic!() };
        assert_eq!(cols.iter().find(|(k, _)| is_bytes(k, b"NAME")).unwrap().1, Value::Int(120));
        assert!(root.iter().any(|(k, v)| is_bytes(k, b"other") && matches!(v, Value::Int(9))),
            "sibling under root survived");
    }

    #[test]
    fn apply_to_tree_leaves_a_compact_shared_result() {
        use blue_marshal::encode;
        // A source Layout subtree whose window-id byte-string repeats across the
        // geometry + flag dicts (the real shape). After splicing into a target and
        // resharing, the encoded stream must carry shared objects (count > 0) and be
        // smaller than the fully-inlined encoding.
        let id = || Value::Bytes(b"overview_window".to_vec());
        let windows = Value::Dict(vec![
            (Value::Bytes(b"openWindows".to_vec()), Value::Dict(vec![(id(), Value::Bool(true))])),
            (Value::Bytes(b"lockedWindows".to_vec()), Value::Dict(vec![(id(), Value::Bool(false))])),
            (Value::Bytes(b"stacksWindows".to_vec()), Value::Dict(vec![(id(), id())])),
        ]);
        let extracted = vec![(Category::Layout, windows)];

        let mut target = Value::Dict(vec![(Value::Bytes(b"windows".to_vec()), Value::Dict(vec![]))]);
        apply_to_tree(&mut target, &extracted);

        let bytes = encode(&target).expect("resharded target encodes");
        let shared_count = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
        assert!(shared_count > 0, "reshare shared the repeated id, count={shared_count}");

        // Smaller than if we had left it fully inlined.
        let inlined_len = encode(&blue_marshal::inline(&target)).unwrap().len();
        assert!(bytes.len() < inlined_len, "{} !< {}", bytes.len(), inlined_len);
    }
}
