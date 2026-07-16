//! Read-only projection of the window-layout portion of a settings document:
//! per-window geometry and flags, each writable field carrying the resolved
//! `NodePath` a `set_scalar`/`insert_dict_entry` mutation targets. All EVE
//! window-format knowledge (the `(timestamp, dict)` wrappers, byte-string
//! window ids, tuple element order) lives here so the UI never reconstructs a
//! path from format details. Nothing in this module mutates.

use std::collections::HashSet;

use blue_marshal::Value;
use serde::Serialize;

use crate::mutate::NewValue;
use crate::path::{NodePath, Step};
use crate::treewalk::{child_dict, collect_shared, effective, timestamped_dict, Entries, SharedTable};

/// The seven boolean per-window flags (see docs/format-notes.md). `stacksWindows`
/// is handled separately — its value is a stack id, not a bool.
const BOOL_FLAGS: [&str; 7] = [
    "openWindows",
    "collapsedWindows",
    "minimizedWindows",
    "lockedWindows",
    "compactWindows",
    "isOverlayedWindows",
    "isLightBackgroundWindows",
];

#[derive(Debug, Serialize)]
pub struct WindowLayout {
    pub reference_w: i64,
    pub reference_h: i64,
    pub windows: Vec<WindowRect>,
}

#[derive(Debug, Serialize)]
pub struct WindowRect {
    pub id: String,
    pub label: String,
    pub open: bool,
    pub renderable: bool,
    pub resolution_matches: bool,
    pub geom: Option<Geom>,
    pub flags: Vec<BoolFlag>,
    pub stacks: Option<StackField>,
}

#[derive(Debug, Serialize)]
pub struct Geom {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    pub screen_w: i64,
    pub screen_h: i64,
    pub x_path: NodePath,
    pub y_path: NodePath,
    pub w_path: NodePath,
    pub h_path: NodePath,
    pub screen_w_path: NodePath,
    pub screen_h_path: NodePath,
}

#[derive(Debug, Serialize)]
pub struct BoolFlag {
    pub name: String,
    pub value: bool,
    pub set: SetTarget,
}

#[derive(Debug, Serialize)]
pub struct StackField {
    pub text: String,
    pub path: NodePath,
}

/// How the UI writes a flag: overwrite an existing entry, insert a missing one,
/// or (when the whole flag dict is absent from the file) nothing.
#[derive(Debug, Serialize)]
#[serde(tag = "how", rename_all = "snake_case")]
pub enum SetTarget {
    Set { path: NodePath },
    Insert { parent: NodePath, key: NewValue },
    Unavailable,
}

pub fn window_layout(root: &Value) -> WindowLayout {
    let empty = WindowLayout { reference_w: 0, reference_h: 0, windows: Vec::new() };

    let Some((windows_dict, windows_path)) = child_dict(root, b"windows", Vec::new()) else {
        return empty;
    };
    let Some((geom_dict, geom_path)) =
        timestamped_dict(windows_dict, &windows_path, b"windowSizesAndPositions_1")
    else {
        return empty;
    };

    // Optional sibling flag dicts, resolved once (each may be absent).
    let bool_dicts: Vec<Option<(&Entries, NodePath)>> = BOOL_FLAGS
        .iter()
        .map(|name| timestamped_dict(windows_dict, &windows_path, name.as_bytes()))
        .collect();
    let stacks_dict = timestamped_dict(windows_dict, &windows_path, b"stacksWindows");

    // Shared-slot table for resolving `Ref`/`Shared` window-id keys.
    let mut shared = SharedTable::new();
    collect_shared(root, &mut shared);

    let mut windows = Vec::new();
    let mut used_ids: HashSet<String> = HashSet::new();
    for (wi, (key, val)) in geom_dict.iter().enumerate() {
        // Resolve the key through Ref/Shared so the id is the real string and
        // flag lookups compare like against like.
        let rkey = effective(key, &shared);
        let mut id = decode_id(rkey);
        // Safety net: a keyed render crashes on duplicate ids, so guarantee
        // uniqueness even if two keys still resolve to the same string — loop
        // until the suffixed id is genuinely free (the suffix itself could clash).
        if !used_ids.insert(id.clone()) {
            let base = id.clone();
            let mut n = wi;
            loop {
                id = format!("{base}#{n}");
                if used_ids.insert(id.clone()) {
                    break;
                }
                n += 1;
            }
        }
        let mut entry_path = geom_path.clone();
        entry_path.push(Step::DictValue(wi));
        let geom = extract_geom(val, &entry_path);

        let mut flags = Vec::with_capacity(BOOL_FLAGS.len());
        let mut open = false;
        for (name, dict) in BOOL_FLAGS.iter().zip(&bool_dicts) {
            let (value, set) = match dict {
                Some((entries, dpath)) => bool_flag(entries, dpath, rkey, &shared),
                None => (false, SetTarget::Unavailable),
            };
            if *name == "openWindows" {
                open = value;
            }
            flags.push(BoolFlag { name: (*name).to_string(), value, set });
        }
        let stacks = stacks_dict
            .as_ref()
            .and_then(|(entries, dpath)| stack_field(entries, dpath, rkey, &shared));

        windows.push(WindowRect {
            id: id.clone(),
            label: id,
            open,
            renderable: geom.is_some(),
            resolution_matches: true, // fixed up below
            geom,
            flags,
            stacks,
        });
    }

    let (reference_w, reference_h) = reference_resolution(&windows);
    for w in &mut windows {
        if let Some(g) = &w.geom {
            w.resolution_matches = g.screen_w == reference_w && g.screen_h == reference_h;
        }
    }
    WindowLayout { reference_w, reference_h, windows }
}

fn decode_id(key: &Value) -> String {
    match key {
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        Value::Str(s) | Value::StrUcs2(s) => s.clone(),
        other => crate::projection_kind(other).to_string(),
    }
}

fn extract_geom(val: &Value, entry_path: &NodePath) -> Option<Geom> {
    let Value::Tuple(items) = val else { return None };
    if items.len() != 6 {
        return None;
    }
    let mut ints = [0i64; 6];
    for (i, e) in items.iter().enumerate() {
        match e {
            Value::Int(n) => ints[i] = *n,
            _ => return None,
        }
    }
    let path = |i: usize| {
        let mut q = entry_path.clone();
        q.push(Step::Tuple(i));
        q
    };
    Some(Geom {
        x: ints[0],
        y: ints[1],
        w: ints[2],
        h: ints[3],
        screen_w: ints[4],
        screen_h: ints[5],
        x_path: path(0),
        y_path: path(1),
        w_path: path(2),
        h_path: path(3),
        screen_w_path: path(4),
        screen_h_path: path(5),
    })
}

/// `rkey` is the geometry key already resolved through `Ref`/`Shared`; flag
/// keys are resolved the same way so like compares against like.
fn bool_flag(entries: &Entries, dpath: &NodePath, rkey: &Value, shared: &SharedTable) -> (bool, SetTarget) {
    match entries.iter().enumerate().find(|(_, (k, _))| effective(k, shared) == rkey) {
        Some((i, (_, v))) => {
            let mut p = dpath.clone();
            p.push(Step::DictValue(i));
            (matches!(v, Value::Bool(true)), SetTarget::Set { path: p })
        }
        None => match key_as_new_value(rkey) {
            Some(nv) => (false, SetTarget::Insert { parent: dpath.clone(), key: nv }),
            None => (false, SetTarget::Unavailable),
        },
    }
}

fn stack_field(entries: &Entries, dpath: &NodePath, rkey: &Value, shared: &SharedTable) -> Option<StackField> {
    let (i, (_, v)) = entries.iter().enumerate().find(|(_, (k, _))| effective(k, shared) == rkey)?;
    // Editable only when the stack id is a plain integer; anything else stays
    // raw-tree-only rather than exposing a control that cannot round-trip.
    let Value::Int(n) = v else { return None };
    let mut p = dpath.clone();
    p.push(Step::DictValue(i));
    Some(StackField { text: n.to_string(), path: p })
}

/// Reconstruct a resolved dict key as the `NewValue` an insert mutation needs.
/// Window ids are byte-strings or (parameterized) strings.
fn key_as_new_value(key: &Value) -> Option<NewValue> {
    match key {
        Value::Bytes(b) => Some(NewValue::BytesHex(hex(b))),
        Value::Str(s) => Some(NewValue::Str(s.clone())),
        Value::StrUcs2(s) => Some(NewValue::StrUcs2(s.clone())),
        _ => None,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The resolution the most windows agree on. Prefers open windows (what the
/// canvas actually draws); falls back to all renderable windows, then (0, 0).
fn reference_resolution(windows: &[WindowRect]) -> (i64, i64) {
    let res = |w: &WindowRect| w.geom.as_ref().map(|g| (g.screen_w, g.screen_h));
    mode(windows.iter().filter(|w| w.open).filter_map(res))
        .or_else(|| mode(windows.iter().filter_map(res)))
        .unwrap_or((0, 0))
}

fn mode(it: impl Iterator<Item = (i64, i64)>) -> Option<(i64, i64)> {
    let mut counts: Vec<((i64, i64), usize)> = Vec::new();
    for res in it {
        match counts.iter_mut().find(|(r, _)| *r == res) {
            Some(entry) => entry.1 += 1,
            None => counts.push((res, 1)),
        }
    }
    counts.into_iter().max_by_key(|(_, c)| *c).map(|(r, _)| r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;
    use crate::path::resolve;

    fn ts() -> Value {
        // A stand-in FILETIME timestamp — the (timestamp, dict) wrapper.
        Value::Long(vec![0u8; 8])
    }

    fn geom(x: i64, y: i64, w: i64, h: i64, sw: i64, sh: i64) -> Value {
        Value::Tuple(vec![
            Value::Int(x), Value::Int(y), Value::Int(w),
            Value::Int(h), Value::Int(sw), Value::Int(sh),
        ])
    }

    /// root -> b"windows" -> { b"windowSizesAndPositions_1": (ts, { id: 6tuple }) }
    fn doc_with(geom_entries: Vec<(Value, Value)>) -> Value {
        Value::Dict(vec![(
            Value::Bytes(b"windows".to_vec()),
            Value::Dict(vec![(
                Value::Bytes(b"windowSizesAndPositions_1".to_vec()),
                Value::Tuple(vec![ts(), Value::Dict(geom_entries)]),
            )]),
        )])
    }

    #[test]
    fn extracts_windows_values_and_paths() {
        let doc = doc_with(vec![
            (Value::Bytes(b"overview".to_vec()), geom(100, 200, 400, 1000, 2560, 1440)),
            (Value::Bytes(b"market".to_vec()), geom(16, 825, 500, 600, 2560, 1440)),
        ]);
        let wl = window_layout(&doc);
        assert_eq!(wl.reference_w, 2560);
        assert_eq!(wl.reference_h, 1440);
        assert_eq!(wl.windows.len(), 2);

        let ov = &wl.windows[0];
        assert_eq!(ov.id, "overview");
        assert_eq!(ov.label, "overview");
        assert!(ov.renderable);
        assert!(ov.resolution_matches);
        let g = ov.geom.as_ref().expect("renderable window has geom");
        assert_eq!((g.x, g.y, g.w, g.h, g.screen_w, g.screen_h), (100, 200, 400, 1000, 2560, 1440));
        // Each path resolves to the right element in the original tree.
        assert_eq!(resolve(&doc, &g.x_path), Some(&Value::Int(100)));
        assert_eq!(resolve(&doc, &g.h_path), Some(&Value::Int(1000)));
        assert_eq!(resolve(&doc, &g.screen_w_path), Some(&Value::Int(2560)));
    }

    #[test]
    fn a_malformed_tuple_is_listed_but_not_renderable() {
        let doc = doc_with(vec![
            (Value::Bytes(b"overview".to_vec()), geom(1, 2, 3, 4, 2560, 1440)),
            // Only five ints — not a valid geometry tuple.
            (Value::Bytes(b"broken".to_vec()),
             Value::Tuple(vec![Value::Int(1), Value::Int(2), Value::Int(3), Value::Int(4), Value::Int(5)])),
        ]);
        let wl = window_layout(&doc);
        assert_eq!(wl.windows.len(), 2);
        assert!(!wl.windows[1].renderable);
        assert!(wl.windows[1].geom.is_none());
    }

    #[test]
    fn reference_resolution_is_the_most_common_and_flags_mismatches() {
        let doc = doc_with(vec![
            (Value::Bytes(b"a".to_vec()), geom(0, 0, 10, 10, 2560, 1440)),
            (Value::Bytes(b"b".to_vec()), geom(0, 0, 10, 10, 2560, 1440)),
            (Value::Bytes(b"c".to_vec()), geom(0, 0, 10, 10, 1920, 1080)),
        ]);
        let wl = window_layout(&doc);
        assert_eq!((wl.reference_w, wl.reference_h), (2560, 1440));
        assert!(wl.windows[0].resolution_matches);
        assert!(!wl.windows[2].resolution_matches);
    }

    #[test]
    fn a_file_without_geometry_is_empty() {
        let doc = Value::Dict(vec![(Value::Bytes(b"ui".to_vec()), Value::Dict(vec![]))]);
        let wl = window_layout(&doc);
        assert!(wl.windows.is_empty());
    }

    /// Build root -> b"windows" -> { geometry, openWindows, lockedWindows, stacksWindows }.
    fn doc_with_flags() -> Value {
        Value::Dict(vec![(
            Value::Bytes(b"windows".to_vec()),
            Value::Dict(vec![
                (
                    Value::Bytes(b"windowSizesAndPositions_1".to_vec()),
                    Value::Tuple(vec![
                        ts(),
                        Value::Dict(vec![
                            (Value::Bytes(b"overview".to_vec()), geom(1, 2, 3, 4, 2560, 1440)),
                            (Value::Bytes(b"market".to_vec()), geom(5, 6, 7, 8, 2560, 1440)),
                        ]),
                    ]),
                ),
                (
                    Value::Bytes(b"openWindows".to_vec()),
                    Value::Tuple(vec![
                        ts(),
                        Value::Dict(vec![
                            (Value::Bytes(b"overview".to_vec()), Value::Bool(true)),
                            (Value::Bytes(b"market".to_vec()), Value::Bool(false)),
                        ]),
                    ]),
                ),
                (
                    Value::Bytes(b"lockedWindows".to_vec()),
                    // Only overview has an entry; market's locked flag is absent.
                    Value::Tuple(vec![
                        ts(),
                        Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Bool(true))]),
                    ]),
                ),
                (
                    Value::Bytes(b"stacksWindows".to_vec()),
                    Value::Tuple(vec![
                        ts(),
                        Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Int(42))]),
                    ]),
                ),
            ]),
        )])
    }

    fn flag<'a>(w: &'a WindowRect, name: &str) -> &'a BoolFlag {
        w.flags.iter().find(|f| f.name == name).expect("flag present")
    }

    #[test]
    fn open_and_present_flags_carry_set_targets() {
        let doc = doc_with_flags();
        let wl = window_layout(&doc);
        let ov = &wl.windows[0];
        assert!(ov.open, "overview is open");
        assert_eq!(ov.flags.len(), 7);
        let locked = flag(ov, "lockedWindows");
        assert!(locked.value);
        // A present flag resolves to a set path over the real Bool(true).
        match &locked.set {
            SetTarget::Set { path } => assert_eq!(resolve(&doc, path), Some(&Value::Bool(true))),
            other => panic!("expected Set, got {other:?}"),
        }
    }

    #[test]
    fn an_absent_flag_carries_insert_params() {
        let doc = doc_with_flags();
        let wl = window_layout(&doc);
        let market = &wl.windows[1];
        assert!(!market.open, "market is closed");
        let locked = flag(market, "lockedWindows");
        assert!(!locked.value);
        // market has no lockedWindows entry -> insert with its byte-string key.
        match &locked.set {
            SetTarget::Insert { key, .. } => {
                assert!(matches!(key, NewValue::BytesHex(h) if h == "6d61726b6574")); // b"market"
            }
            other => panic!("expected Insert, got {other:?}"),
        }
    }

    #[test]
    fn a_missing_flag_dict_is_unavailable() {
        // doc_with (Task 1) has geometry but no flag dicts at all.
        let doc = doc_with(vec![(Value::Bytes(b"overview".to_vec()), geom(1, 2, 3, 4, 2560, 1440))]);
        let wl = window_layout(&doc);
        assert!(matches!(flag(&wl.windows[0], "openWindows").set, SetTarget::Unavailable));
    }

    #[test]
    fn stacks_is_an_editable_value_when_numeric() {
        let doc = doc_with_flags();
        let wl = window_layout(&doc);
        let ov = &wl.windows[0];
        let s = ov.stacks.as_ref().expect("overview has a stack id");
        assert_eq!(s.text, "42");
        assert_eq!(resolve(&doc, &s.path), Some(&Value::Int(42)));
        // market has no stacks entry.
        assert!(wl.windows[1].stacks.is_none());
    }

    #[test]
    fn reference_prefers_open_windows() {
        // Two closed windows at 1920x1080, one open at 2560x1440: the open one wins.
        let doc = Value::Dict(vec![(
            Value::Bytes(b"windows".to_vec()),
            Value::Dict(vec![
                (
                    Value::Bytes(b"windowSizesAndPositions_1".to_vec()),
                    Value::Tuple(vec![
                        ts(),
                        Value::Dict(vec![
                            (Value::Bytes(b"a".to_vec()), geom(0, 0, 1, 1, 1920, 1080)),
                            (Value::Bytes(b"b".to_vec()), geom(0, 0, 1, 1, 1920, 1080)),
                            (Value::Bytes(b"c".to_vec()), geom(0, 0, 1, 1, 2560, 1440)),
                        ]),
                    ]),
                ),
                (
                    Value::Bytes(b"openWindows".to_vec()),
                    Value::Tuple(vec![
                        ts(),
                        Value::Dict(vec![(Value::Bytes(b"c".to_vec()), Value::Bool(true))]),
                    ]),
                ),
            ]),
        )]);
        let wl = window_layout(&doc);
        assert_eq!((wl.reference_w, wl.reference_h), (2560, 1440));
    }

    #[test]
    fn ref_keyed_windows_resolve_to_real_unique_ids_and_match_flags() {
        // Real files store each window-id string once as a `Shared` and key
        // other dicts by `Ref`. Here geometry keys are Refs to id strings
        // defined (as Shared) in openWindows. Without resolution both ids
        // collapse to "ref" (duplicate) and flag lookups miss.
        let doc = Value::Dict(vec![(
            Value::Bytes(b"windows".to_vec()),
            Value::Dict(vec![
                (
                    Value::Bytes(b"windowSizesAndPositions_1".to_vec()),
                    Value::Tuple(vec![
                        ts(),
                        Value::Dict(vec![
                            (Value::Ref(1), geom(1, 2, 3, 4, 2560, 1440)),
                            (Value::Ref(2), geom(5, 6, 7, 8, 2560, 1440)),
                        ]),
                    ]),
                ),
                (
                    Value::Bytes(b"openWindows".to_vec()),
                    Value::Tuple(vec![
                        ts(),
                        Value::Dict(vec![
                            (
                                Value::Shared { slot: 1, value: Box::new(Value::Bytes(b"overview".to_vec())) },
                                Value::Bool(true),
                            ),
                            (
                                Value::Shared { slot: 2, value: Box::new(Value::Bytes(b"market".to_vec())) },
                                Value::Bool(false),
                            ),
                        ]),
                    ]),
                ),
            ]),
        )]);
        let wl = window_layout(&doc);
        assert_eq!(wl.windows.len(), 2);
        assert_eq!(wl.windows[0].id, "overview", "Ref key resolves to the real id");
        assert_eq!(wl.windows[1].id, "market");
        // Flag matching works across the Ref (geometry) / Shared (openWindows) split.
        assert!(wl.windows[0].open);
        assert!(!wl.windows[1].open);
    }

    #[test]
    fn colliding_ids_are_disambiguated_so_a_keyed_render_cannot_crash() {
        // Three unresolvable Refs all fall back to "ref"; the projection must
        // still emit unique ids (a keyed each block crashes on duplicates).
        let doc = doc_with(vec![
            (Value::Ref(7), geom(1, 2, 3, 4, 2560, 1440)),
            (Value::Ref(8), geom(5, 6, 7, 8, 2560, 1440)),
            (Value::Ref(9), geom(9, 9, 9, 9, 2560, 1440)),
        ]);
        let wl = window_layout(&doc);
        assert_eq!(wl.windows.len(), 3);
        let ids: HashSet<&String> = wl.windows.iter().map(|w| &w.id).collect();
        assert_eq!(ids.len(), 3, "ids must be unique even on fallback collision");
    }
}
