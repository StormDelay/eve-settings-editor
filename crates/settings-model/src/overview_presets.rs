//! Structural authoring for overview *filter presets*: the named filter
//! definitions in the user file's `overview` container under
//! `overviewProfilePresets` (a `(timestamp, dict)` keyed by preset name; each
//! value is an opaque `{groups, filteredStates, alwaysShownStates}` blob 2a
//! copies wholesale but never inspects). A tab points at a preset by name in its
//! `overview` field. Edits use the same inline-first idiom as `overview_tabs.rs`
//! and reuse its `pub(crate)` helpers; the app layer reshares before saving.
//!
//! `overviewProfilePresets_notSaved` is a parallel, name-keyed buffer holding
//! EVE's unsaved working copy of a preset. It is populated on most real files, so
//! rename/delete mirror into it to avoid stranding a stale entry that could
//! resurrect a phantom preset on next login.

use blue_marshal::Value;

use crate::overview_tabs::{dict_inner_mut, is_b, overview_mut, set_tab_preset, tabs_mut, OverviewTabError};
use crate::treewalk::{inline_all, Entries};

/// String form of a preset dict key or a tab's `overview` value (Bytes on real
/// files; Str/StrUcs2 defensively). Used for name comparison after inlining.
pub(crate) fn as_str(v: &Value) -> Option<String> {
    match v {
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        Value::Str(s) | Value::StrUcs2(s) => Some(s.clone()),
        _ => None,
    }
}

/// Mutable inner dict of `overviewProfilePresets` (unwrapping `(ts, dict)`).
/// None when the container is absent.
pub(crate) fn presets_mut(ov: &mut Entries) -> Option<&mut Entries> {
    let (_, v) = ov.iter_mut().find(|(k, _)| is_b(k, b"overviewProfilePresets"))?;
    dict_inner_mut(v)
}

/// Mutable inner dict of `overviewProfilePresets`, MINTING an empty
/// `(timestamp, dict)` container if the key is absent (a clean account stores no
/// presets at all). The timestamp is a zero `Long`, matching how the codebase
/// mints fresh `(ts, dict)` containers; EVE re-timestamps on its next save.
pub(crate) fn presets_mut_or_create(ov: &mut Entries) -> &mut Entries {
    if !ov.iter().any(|(k, _)| is_b(k, b"overviewProfilePresets")) {
        ov.push((
            Value::Bytes(b"overviewProfilePresets".to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Dict(Vec::new())]),
        ));
    }
    let (_, v) = ov.iter_mut().find(|(k, _)| is_b(k, b"overviewProfilePresets")).unwrap();
    dict_inner_mut(v).expect("just-created or existing (ts, dict)")
}

/// Create a NEW user preset `name` from explicit lists — used to fork a built-in
/// default that may not be stored in the file (so `create_preset`, which clones
/// an existing key, cannot be used). Groups are sorted ascending; the state lists
/// are stored as given (opaque; slice 3 edits them).
pub fn create_preset_from_lists(
    v: &mut Value, name: &str,
    groups: &[i64], filtered_states: &[i64], always_shown_states: &[i64],
) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    let presets = presets_mut_or_create(ov);
    if presets.iter().any(|(k, _)| as_str(k).as_deref() == Some(name)) {
        return Err(OverviewTabError::PresetExists { name: name.to_string() });
    }
    let list = |xs: &[i64], sorted: bool| {
        let mut xs = xs.to_vec();
        if sorted { xs.sort_unstable(); }
        Value::List(xs.into_iter().map(Value::Int).collect())
    };
    let blob = Value::Dict(vec![
        (Value::Bytes(b"groups".to_vec()), list(groups, true)),
        (Value::Bytes(b"filteredStates".to_vec()), list(filtered_states, false)),
        (Value::Bytes(b"alwaysShownStates".to_vec()), list(always_shown_states, false)),
    ]);
    presets.push((Value::Bytes(name.as_bytes().to_vec()), blob));
    Ok(())
}

/// Fork a built-in default into a NEW user preset `name` from explicit lists AND
/// retarget tab `tab_idx` to it. Validates the tab index BEFORE creating anything,
/// so a bad index cannot strand a half-made preset (create-then-retarget is
/// otherwise non-atomic). `PresetExists` if `name` is taken; `UnknownTab` if the
/// tab is absent (checked first — nothing is created in that case).
pub fn fork_preset(
    v: &mut Value, tab_idx: i64, name: &str,
    groups: &[i64], filtered_states: &[i64], always_shown_states: &[i64],
) -> Result<(), OverviewTabError> {
    inline_all(v);
    {
        let ov = overview_mut(v)?;
        if !tabs_mut(ov).iter().any(|(k, _)| matches!(k, Value::Int(i) if *i == tab_idx)) {
            return Err(OverviewTabError::UnknownTab { index: tab_idx });
        }
    }
    create_preset_from_lists(v, name, groups, filtered_states, always_shown_states)?;
    set_tab_preset(v, tab_idx, name)
}

/// Mutable inner dict of `overviewProfilePresets_notSaved`, if present (it may be
/// absent or empty — callers do nothing then).
pub(crate) fn not_saved_mut(ov: &mut Entries) -> Option<&mut Entries> {
    let (_, v) = ov.iter_mut().find(|(k, _)| is_b(k, b"overviewProfilePresets_notSaved"))?;
    dict_inner_mut(v)
}

/// Preset names sorted case-insensitively — the SAME order the projection shows,
/// so the delete-neighbour the UI names matches the one the model reassigns to.
pub(crate) fn sorted_names(presets: &Entries) -> Vec<String> {
    let mut names: Vec<String> = presets.iter().filter_map(|(k, _)| as_str(k)).collect();
    names.sort_by_key(|s| s.to_lowercase());
    names
}

/// Repoint every tab whose `overview` field equals `old` to `new` (Bytes value,
/// matching real files). No-op for tabs pointing elsewhere.
pub(crate) fn retarget_tabs(tabs: &mut Entries, old: &str, new: &str) {
    for (_, tab) in tabs.iter_mut() {
        if let Some(fields) = dict_inner_mut(tab) {
            if let Some((_, val)) = fields.iter_mut().find(|(k, _)| is_b(k, b"overview")) {
                if as_str(val).as_deref() == Some(old) {
                    *val = Value::Bytes(new.as_bytes().to_vec());
                }
            }
        }
    }
}

/// Duplicate the `from` preset's whole value blob under a new key `new_name`.
/// Cloning keeps the required `{groups, filteredStates, alwaysShownStates}` shape
/// correct by construction (2a never inspects it).
pub fn create_preset(v: &mut Value, from: &str, new_name: &str) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    let presets = presets_mut(ov).ok_or(OverviewTabError::UnknownPreset { name: from.to_string() })?;
    if presets.iter().any(|(k, _)| as_str(k).as_deref() == Some(new_name)) {
        return Err(OverviewTabError::PresetExists { name: new_name.to_string() });
    }
    let blob = presets
        .iter()
        .find(|(k, _)| as_str(k).as_deref() == Some(from))
        .map(|(_, val)| val.clone())
        .ok_or(OverviewTabError::UnknownPreset { name: from.to_string() })?;
    presets.push((Value::Bytes(new_name.as_bytes().to_vec()), blob));
    Ok(())
}

/// Rename a preset: the `overviewProfilePresets` key, every tab that references
/// it, and any matching `overviewProfilePresets_notSaved` buffer entry.
pub fn rename_preset(v: &mut Value, old: &str, new: &str) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    {
        let presets = presets_mut(ov).ok_or(OverviewTabError::UnknownPreset { name: old.to_string() })?;
        if old != new && presets.iter().any(|(k, _)| as_str(k).as_deref() == Some(new)) {
            return Err(OverviewTabError::PresetExists { name: new.to_string() });
        }
        let entry = presets.iter_mut().find(|(k, _)| as_str(k).as_deref() == Some(old))
            .ok_or(OverviewTabError::UnknownPreset { name: old.to_string() })?;
        entry.0 = Value::Bytes(new.as_bytes().to_vec());
    }
    if old == new {
        return Ok(());
    }
    if let Some(ns) = not_saved_mut(ov) {
        if let Some(entry) = ns.iter_mut().find(|(k, _)| as_str(k).as_deref() == Some(old)) {
            entry.0 = Value::Bytes(new.as_bytes().to_vec());
        }
    }
    retarget_tabs(tabs_mut(ov), old, new);
    Ok(())
}

/// Delete a preset, reassigning every tab that used it to the immediately
/// preceding preset in sorted order (the successor when deleting the first).
/// Refuses the last preset. Also drops any matching `_notSaved` buffer entry.
pub fn delete_preset(v: &mut Value, name: &str) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    let target = {
        let presets = presets_mut(ov).ok_or(OverviewTabError::UnknownPreset { name: name.to_string() })?;
        if !presets.iter().any(|(k, _)| as_str(k).as_deref() == Some(name)) {
            return Err(OverviewTabError::UnknownPreset { name: name.to_string() });
        }
        if presets.len() <= 1 {
            return Err(OverviewTabError::LastPreset);
        }
        let names = sorted_names(presets);
        let pos = names.iter().position(|n| n == name).expect("name is present");
        if pos > 0 { names[pos - 1].clone() } else { names[pos + 1].clone() }
    };
    retarget_tabs(tabs_mut(ov), name, &target);
    if let Some(ns) = not_saved_mut(ov) {
        ns.retain(|(k, _)| as_str(k).as_deref() != Some(name));
    }
    if let Some(presets) = presets_mut(ov) {
        presets.retain(|(k, _)| as_str(k).as_deref() != Some(name));
    }
    Ok(())
}

/// Replace the named preset's `groups` list with `groups`, sorted ascending for a
/// deterministic on-disk order (EVE re-sorts for display). Inserts the `groups`
/// key if somehow absent (real presets always carry it). The two state lists
/// (`filteredStates` / `alwaysShownStates`) are untouched — slice 3.
pub fn set_preset_groups(v: &mut Value, name: &str, groups: &[i64]) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    let presets = presets_mut(ov).ok_or(OverviewTabError::UnknownPreset { name: name.to_string() })?;
    let (_, blob) = presets
        .iter_mut()
        .find(|(k, _)| as_str(k).as_deref() == Some(name))
        .ok_or(OverviewTabError::UnknownPreset { name: name.to_string() })?;
    let fields = dict_inner_mut(blob).ok_or(OverviewTabError::UnknownPreset { name: name.to_string() })?;
    let mut sorted = groups.to_vec();
    sorted.sort_unstable();
    let list = Value::List(sorted.into_iter().map(Value::Int).collect());
    if let Some((_, g)) = fields.iter_mut().find(|(k, _)| is_b(k, b"groups")) {
        *g = list;
    } else {
        fields.push((Value::Bytes(b"groups".to_vec()), list));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }

    /// user -> overview -> {
    ///   tabsettings_new: { 0: {overview:"alpha"}, 1: {overview:"beta"} },
    ///   overviewProfilePresets: (ts, { "alpha": {groups:[1]}, "beta": {groups:[2]} }),
    ///   overviewProfilePresets_notSaved: (ts, { "alpha": {groups:[9]} }),
    /// }
    fn user_with_presets() -> Value {
        let tab0 = Value::Dict(vec![(b("overview"), b("alpha"))]);
        let tab1 = Value::Dict(vec![(b("overview"), b("beta"))]);
        let preset = |g: i64| Value::Dict(vec![(b("groups"), Value::List(vec![Value::Int(g)]))]);
        let overview = Value::Dict(vec![
            (b("tabsettings_new"), Value::Dict(vec![
                (Value::Int(0), tab0), (Value::Int(1), tab1),
            ])),
            (b("overviewProfilePresets"), Value::Tuple(vec![
                Value::Int(1),
                Value::Dict(vec![(b("alpha"), preset(1)), (b("beta"), preset(2))]),
            ])),
            (b("overviewProfilePresets_notSaved"), Value::Tuple(vec![
                Value::Int(1),
                Value::Dict(vec![(b("alpha"), preset(9))]),
            ])),
        ]);
        Value::Dict(vec![(b("overview"), overview)])
    }

    fn preset_names(v: &Value) -> Vec<String> {
        let Value::Dict(root) = v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, p) = ovd.iter().find(|(k, _)| is_b(k, b"overviewProfilePresets")).unwrap();
        let Value::Tuple(items) = p else { panic!() };
        let Value::Dict(pd) = &items[1] else { panic!() };
        pd.iter().filter_map(|(k, _)| as_str(k)).collect()
    }

    #[test]
    fn duplicate_clones_the_blob_under_the_new_key() {
        let mut v = user_with_presets();
        create_preset(&mut v, "alpha", "gamma").unwrap();
        let names = preset_names(&v);
        assert!(names.contains(&"gamma".to_string()));
        // The clone carries alpha's blob: groups == [1].
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, p) = ovd.iter().find(|(k, _)| is_b(k, b"overviewProfilePresets")).unwrap();
        let Value::Tuple(items) = p else { panic!() };
        let Value::Dict(pd) = &items[1] else { panic!() };
        let (_, gamma) = pd.iter().find(|(k, _)| as_str(k).as_deref() == Some("gamma")).unwrap();
        let Value::Dict(gf) = gamma else { panic!() };
        let (_, groups) = gf.iter().find(|(k, _)| is_b(k, b"groups")).unwrap();
        assert_eq!(groups, &Value::List(vec![Value::Int(1)]));
    }

    #[test]
    fn duplicate_unknown_source_errors() {
        let mut v = user_with_presets();
        assert!(matches!(
            create_preset(&mut v, "nope", "gamma"),
            Err(OverviewTabError::UnknownPreset { .. })
        ));
    }

    #[test]
    fn duplicate_existing_target_errors() {
        let mut v = user_with_presets();
        assert!(matches!(
            create_preset(&mut v, "alpha", "beta"),
            Err(OverviewTabError::PresetExists { .. })
        ));
    }

    fn tab_preset(v: &Value, idx: i64) -> String {
        let Value::Dict(root) = v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, tabs) = ovd.iter().find(|(k, _)| is_b(k, b"tabsettings_new")).unwrap();
        let Value::Dict(td) = tabs else { panic!() };
        let (_, tab) = td.iter().find(|(k, _)| matches!(k, Value::Int(i) if *i == idx)).unwrap();
        let Value::Dict(fields) = tab else { panic!() };
        let (_, val) = fields.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        as_str(val).unwrap()
    }

    fn not_saved_names(v: &Value) -> Vec<String> {
        let Value::Dict(root) = v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, p) = ovd.iter().find(|(k, _)| is_b(k, b"overviewProfilePresets_notSaved")).unwrap();
        let Value::Tuple(items) = p else { panic!() };
        let Value::Dict(pd) = &items[1] else { panic!() };
        pd.iter().filter_map(|(k, _)| as_str(k)).collect()
    }

    #[test]
    fn rename_renames_key_retargets_tabs_and_mirrors_notsaved() {
        let mut v = user_with_presets();
        rename_preset(&mut v, "alpha", "alpha2").unwrap();
        let names = preset_names(&v);
        assert!(names.contains(&"alpha2".to_string()) && !names.contains(&"alpha".to_string()));
        assert_eq!(tab_preset(&v, 0), "alpha2", "tab 0 followed the rename");
        assert_eq!(tab_preset(&v, 1), "beta", "tab 1 unaffected");
        assert!(not_saved_names(&v).contains(&"alpha2".to_string()), "notSaved buffer followed");
    }

    #[test]
    fn rename_unknown_source_errors() {
        let mut v = user_with_presets();
        assert!(matches!(rename_preset(&mut v, "nope", "x"), Err(OverviewTabError::UnknownPreset { .. })));
    }

    #[test]
    fn rename_to_existing_name_errors() {
        let mut v = user_with_presets();
        assert!(matches!(rename_preset(&mut v, "alpha", "beta"), Err(OverviewTabError::PresetExists { .. })));
    }

    #[test]
    fn delete_reassigns_tabs_to_preceding_preset_and_pops_notsaved() {
        // Delete "beta" (pos 1) -> preceding is "alpha"; tab 1 moves to "alpha".
        let mut v = user_with_presets();
        delete_preset(&mut v, "beta").unwrap();
        let names = preset_names(&v);
        assert!(!names.contains(&"beta".to_string()));
        assert_eq!(tab_preset(&v, 1), "alpha", "tab moved to the preceding preset");
    }

    #[test]
    fn delete_first_reassigns_to_successor() {
        // Delete "alpha" (pos 0) -> successor "beta"; tab 0 moves to "beta".
        let mut v = user_with_presets();
        delete_preset(&mut v, "alpha").unwrap();
        assert!(!preset_names(&v).contains(&"alpha".to_string()));
        assert_eq!(tab_preset(&v, 0), "beta");
        assert!(!not_saved_names(&v).contains(&"alpha".to_string()), "notSaved entry removed");
    }

    #[test]
    fn delete_last_preset_is_refused() {
        // A tree with a single preset.
        let overview = Value::Dict(vec![
            (b("overviewProfilePresets"), Value::Tuple(vec![
                Value::Int(1), Value::Dict(vec![(b("only"), Value::Dict(vec![]))]),
            ])),
        ]);
        let mut v = Value::Dict(vec![(b("overview"), overview)]);
        assert!(matches!(delete_preset(&mut v, "only"), Err(OverviewTabError::LastPreset)));
    }

    #[test]
    fn delete_unknown_preset_errors() {
        let mut v = user_with_presets();
        assert!(matches!(delete_preset(&mut v, "nope"), Err(OverviewTabError::UnknownPreset { .. })));
    }

    fn preset_groups(v: &Value, name: &str) -> Vec<i64> {
        let Value::Dict(root) = v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, p) = ovd.iter().find(|(k, _)| is_b(k, b"overviewProfilePresets")).unwrap();
        let Value::Tuple(items) = p else { panic!() };
        let Value::Dict(pd) = &items[1] else { panic!() };
        let (_, blob) = pd.iter().find(|(k, _)| as_str(k).as_deref() == Some(name)).unwrap();
        let Value::Dict(bf) = blob else { panic!() };
        let (_, groups) = bf.iter().find(|(k, _)| is_b(k, b"groups")).unwrap();
        let Value::List(l) = groups else { panic!() };
        l.iter().filter_map(|e| if let Value::Int(n) = e { Some(*n) } else { None }).collect()
    }

    #[test]
    fn set_groups_replaces_with_sorted_list() {
        let mut v = user_with_presets();
        set_preset_groups(&mut v, "alpha", &[30, 10, 20]).unwrap();
        assert_eq!(preset_groups(&v, "alpha"), vec![10, 20, 30]);
    }

    #[test]
    fn set_groups_unknown_preset_errors() {
        let mut v = user_with_presets();
        assert!(matches!(
            set_preset_groups(&mut v, "nope", &[1]),
            Err(OverviewTabError::UnknownPreset { .. })
        ));
    }

    #[test]
    fn set_groups_inserts_groups_key_when_absent() {
        // A preset blob with no `groups` key at all.
        let overview = Value::Dict(vec![(b("overviewProfilePresets"), Value::Tuple(vec![
            Value::Int(1),
            Value::Dict(vec![(b("solo"), Value::Dict(vec![]))]),
        ]))]);
        let mut v = Value::Dict(vec![(b("overview"), overview)]);
        set_preset_groups(&mut v, "solo", &[5, 1]).unwrap();
        assert_eq!(preset_groups(&v, "solo"), vec![1, 5]);
    }

    #[test]
    fn rename_and_delete_no_op_when_notsaved_absent() {
        // Like user_with_presets but WITHOUT overviewProfilePresets_notSaved.
        let tab0 = Value::Dict(vec![(b("overview"), b("alpha"))]);
        let tab1 = Value::Dict(vec![(b("overview"), b("beta"))]);
        let preset = |g: i64| Value::Dict(vec![(b("groups"), Value::List(vec![Value::Int(g)]))]);
        let overview = Value::Dict(vec![
            (b("tabsettings_new"), Value::Dict(vec![
                (Value::Int(0), tab0), (Value::Int(1), tab1),
            ])),
            (b("overviewProfilePresets"), Value::Tuple(vec![
                Value::Int(1),
                Value::Dict(vec![(b("alpha"), preset(1)), (b("beta"), preset(2))]),
            ])),
            // deliberately no overviewProfilePresets_notSaved
        ]);
        let mut v = Value::Dict(vec![(b("overview"), overview)]);
        // rename succeeds; the _notSaved mirror is a no-op (buffer absent).
        rename_preset(&mut v, "alpha", "alpha2").unwrap();
        assert!(preset_names(&v).contains(&"alpha2".to_string()));
        assert_eq!(tab_preset(&v, 0), "alpha2");
        // delete succeeds; the _notSaved removal is a no-op. Presets now {alpha2, beta};
        // deleting "beta" reassigns its tab to the preceding preset "alpha2".
        delete_preset(&mut v, "beta").unwrap();
        assert!(!preset_names(&v).contains(&"beta".to_string()));
        assert_eq!(tab_preset(&v, 1), "alpha2");
    }

    fn overview_container_absent_presets() -> Value {
        // A clean account: overview has tabs but NO overviewProfilePresets.
        let tab0 = Value::Dict(vec![(b("overview"), b("DefaultPreset_1"))]);
        let overview = Value::Dict(vec![
            (b("tabsettings_new"), Value::Dict(vec![(Value::Int(0), tab0)])),
        ]);
        Value::Dict(vec![(b("overview"), overview)])
    }

    #[test]
    fn create_from_lists_materializes_absent_container() {
        let mut v = overview_container_absent_presets();
        create_preset_from_lists(&mut v, "All copy", &[30, 10], &[5], &[]).unwrap();
        assert_eq!(preset_groups(&v, "All copy"), vec![10, 30]); // sorted
    }

    #[test]
    fn create_from_lists_errors_on_existing_name() {
        let mut v = user_with_presets(); // has "alpha"
        assert!(matches!(
            create_preset_from_lists(&mut v, "alpha", &[1], &[], &[]),
            Err(OverviewTabError::PresetExists { .. })
        ));
    }

    #[test]
    fn create_from_lists_stores_state_lists() {
        let mut v = overview_container_absent_presets();
        create_preset_from_lists(&mut v, "PvP copy", &[1], &[8, 7], &[9]).unwrap();
        // read filteredStates back
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, p) = ovd.iter().find(|(k, _)| is_b(k, b"overviewProfilePresets")).unwrap();
        let inner = match p { Value::Tuple(it) => it.iter().find_map(|e| if let Value::Dict(d)=e {Some(d)} else {None}).unwrap(), Value::Dict(d)=>d, _=>panic!() };
        let (_, blob) = inner.iter().find(|(k, _)| as_str(k).as_deref() == Some("PvP copy")).unwrap();
        let Value::Dict(bf) = blob else { panic!() };
        let (_, fs) = bf.iter().find(|(k, _)| is_b(k, b"filteredStates")).unwrap();
        // stored as-given, NOT sorted: [8, 7] stays [8, 7].
        assert_eq!(fs, &Value::List(vec![Value::Int(8), Value::Int(7)]));
    }

    #[test]
    fn fork_creates_preset_and_retargets_the_tab() {
        let mut v = overview_container_absent_presets(); // tab 0 -> a default; NO presets container
        fork_preset(&mut v, 0, "All copy", &[9, 1], &[], &[]).unwrap();
        assert_eq!(preset_groups(&v, "All copy"), vec![1, 9]); // sorted
        assert_eq!(tab_preset(&v, 0), "All copy");             // tab now uses the fork
    }

    #[test]
    fn fork_with_unknown_tab_creates_no_preset() {
        let mut v = overview_container_absent_presets();
        assert!(matches!(fork_preset(&mut v, 99, "X", &[1], &[], &[]), Err(OverviewTabError::UnknownTab { .. })));
        // atomicity: a failed fork must NOT strand a preset "X".
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let stranded = ovd.iter().find(|(k, _)| is_b(k, b"overviewProfilePresets")).is_some_and(|(_, p)| {
            let inner = match p { Value::Tuple(it) => it.iter().find_map(|e| if let Value::Dict(d) = e { Some(d) } else { None }), Value::Dict(d) => Some(d), _ => None };
            inner.is_some_and(|d| d.iter().any(|(k, _)| as_str(k).as_deref() == Some("X")))
        });
        assert!(!stranded, "a failed fork must not strand a preset");
    }
}
