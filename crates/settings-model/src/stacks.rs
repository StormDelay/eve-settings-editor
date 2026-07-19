//! Structural authoring for window stacks: edit `stacksWindows` and
//! `preferredIdxInStack3` under `windows`. The window-id keys/values are
//! `Shared` stores, which `mutate::apply`'s `RemoveEntry` refuses, so every
//! entry point inlines the whole tree first (drops all sharing) and edits plain
//! values; the app layer reshares before saving. Mirrors overview.rs/autofill.rs.

use blue_marshal::Value;
use serde::Serialize;

use crate::treewalk::inline_all;

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum StackError {
    /// No `windows` dict in the file.
    NoWindows,
    /// The named member is not present in `stacksWindows`.
    NotStacked { member: String },
}

fn is_b(k: &Value, name: &[u8]) -> bool { matches!(k, Value::Bytes(b) if b.as_slice() == name) }

/// Mutable `windows` dict (the file is already inlined, so no Shared wrapper).
fn windows_mut(v: &mut Value) -> Result<&mut Vec<(Value, Value)>, StackError> {
    let Value::Dict(top) = v else { return Err(StackError::NoWindows) };
    let (_, w) = top.iter_mut().find(|(k, _)| is_b(k, b"windows")).ok_or(StackError::NoWindows)?;
    match w { Value::Dict(d) => Ok(d), _ => Err(StackError::NoWindows) }
}

/// The inner dict under a `windows` child, unwrapping the `(timestamp, dict)`
/// tuple. Creates a bare dict entry if the child is absent.
fn child_inner<'a>(win: &'a mut Vec<(Value, Value)>, name: &[u8]) -> &'a mut Vec<(Value, Value)> {
    if !win.iter().any(|(k, _)| is_b(k, name)) {
        win.push((Value::Bytes(name.to_vec()), Value::Dict(Vec::new())));
    }
    let (_, v) = win.iter_mut().find(|(k, _)| is_b(k, name)).unwrap();
    match v {
        Value::Dict(d) => d,
        Value::Tuple(t) => {
            if !t.iter().any(|e| matches!(e, Value::Dict(_))) {
                t.push(Value::Dict(Vec::new()));
            }
            let Some(Value::Dict(d)) = t.iter_mut().find(|e| matches!(e, Value::Dict(_))) else { unreachable!() };
            d
        }
        other => { *other = Value::Dict(Vec::new()); let Value::Dict(d) = other else { unreachable!() }; d }
    }
}

pub fn unstack(v: &mut Value, member: &str) -> Result<(), StackError> {
    inline_all(v);
    let win = windows_mut(v)?;
    let mb = member.as_bytes();
    let sw = child_inner(win, b"stacksWindows");
    let before = sw.len();
    sw.retain(|(k, _)| !is_b(k, mb));
    if sw.len() == before {
        return Err(StackError::NotStacked { member: member.to_string() });
    }
    // Remove the member from every preferredIdxInStack3[container] dict.
    let pref = child_inner(win, b"preferredIdxInStack3");
    for (_, inner) in pref.iter_mut() {
        if let Value::Dict(d) = inner {
            d.retain(|(k, _)| !is_b(k, mb));
        }
    }
    Ok(())
}

pub fn add_to_stack(v: &mut Value, member: &str, container: &str) -> Result<(), StackError> {
    inline_all(v);
    let win = windows_mut(v)?;
    let (mb, cb) = (member.as_bytes(), container.as_bytes());
    let sw = child_inner(win, b"stacksWindows");
    sw.retain(|(k, _)| !is_b(k, mb)); // re-stack cleanly if already present
    sw.push((Value::Bytes(mb.to_vec()), Value::Bytes(cb.to_vec())));

    let pref = child_inner(win, b"preferredIdxInStack3");
    let cdict = container_dict(pref, cb);
    cdict.retain(|(k, _)| !is_b(k, mb));
    let next = cdict.iter().filter_map(|(_, v)| if let Value::Int(i) = v { Some(*i) } else { None }).max().map(|m| m + 1).unwrap_or(0);
    cdict.push((Value::Bytes(mb.to_vec()), Value::Int(next)));
    Ok(())
}

pub fn reorder_stack(v: &mut Value, container: &str, members_in_order: &[String]) -> Result<(), StackError> {
    inline_all(v);
    let win = windows_mut(v)?;
    let cb = container.as_bytes();
    let pref = child_inner(win, b"preferredIdxInStack3");
    let cdict = container_dict(pref, cb);
    *cdict = members_in_order.iter().enumerate()
        .map(|(i, m)| (Value::Bytes(m.as_bytes().to_vec()), Value::Int(i as i64)))
        .collect();
    Ok(())
}

/// The `preferredIdxInStack3[container]` inner dict, created if absent.
fn container_dict<'a>(pref: &'a mut Vec<(Value, Value)>, cb: &[u8]) -> &'a mut Vec<(Value, Value)> {
    if !pref.iter().any(|(k, _)| is_b(k, cb)) {
        pref.push((Value::Bytes(cb.to_vec()), Value::Dict(Vec::new())));
    }
    let (_, v) = pref.iter_mut().find(|(k, _)| is_b(k, cb)).unwrap();
    match v { Value::Dict(d) => d, other => { *other = Value::Dict(Vec::new()); let Value::Dict(d) = other else { unreachable!() }; d } }
}

/// Create a new stack from two free windows. `member1` is the window the action
/// started from; the stack lands at its current rect. Returns the minted
/// container id. See docs/format-notes.md ("Window stacks") for the recipe.
pub fn create_stack(v: &mut Value, member1: &str, member2: &str) -> Result<String, StackError> {
    inline_all(v);
    let container = mint_free_id(windows_mut(v)?);
    let (cb, m1b, m2b) = (container.as_bytes().to_vec(), member1.as_bytes(), member2.as_bytes());

    // Geometry: C and M2 take M1's current rect.
    let win = windows_mut(v)?;
    let geoms = child_inner(win, b"windowSizesAndPositions_1");
    let m1_rect = geoms.iter().find(|(k, _)| is_b(k, m1b)).map(|(_, r)| r.clone());
    if let Some(rect) = m1_rect {
        set_entry(geoms, &cb, rect.clone());
        set_entry(geoms, m2b, rect);
    }
    // Membership.
    let sw = child_inner(win, b"stacksWindows");
    set_entry(sw, m1b, Value::Bytes(cb.clone()));
    set_entry(sw, m2b, Value::Bytes(cb.clone()));
    let pref = child_inner(win, b"preferredIdxInStack3");
    let cdict = container_dict(pref, &cb);
    *cdict = vec![
        (Value::Bytes(m1b.to_vec()), Value::Int(0)),
        (Value::Bytes(m2b.to_vec()), Value::Int(1)),
    ];
    // Open C + both members; mark C in the three state dicts.
    for (dict, val) in [(b"openWindows".as_slice(), true)] {
        let d = child_inner(win, dict);
        set_entry(d, &cb, Value::Bool(val));
        set_entry(d, m1b, Value::Bool(val));
        set_entry(d, m2b, Value::Bool(val));
    }
    for dict in [b"isLightBackgroundWindows".as_slice(), b"isOverlayedWindows", b"minimizedWindows"] {
        let d = child_inner(win, dict);
        set_entry(d, &cb, Value::Bool(false));
    }
    Ok(container)
}

/// Lowest free integer id, at least 1000, that is not already used as a key or
/// container value anywhere in the window dicts — a high value avoids colliding
/// with EVE's own low counter (spec §7).
fn mint_free_id(win: &[(Value, Value)]) -> String {
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (_, v) in win {
        collect_ids(v, &mut used);
    }
    let mut n: i64 = 1000;
    while used.contains(&n.to_string()) { n += 1; }
    n.to_string()
}

fn collect_ids(v: &Value, out: &mut std::collections::HashSet<String>) {
    match v {
        Value::Bytes(b) => { out.insert(String::from_utf8_lossy(b).into_owned()); }
        Value::Tuple(t) => t.iter().for_each(|e| collect_ids(e, out)),
        Value::Dict(d) => d.iter().for_each(|(k, val)| { collect_ids(k, out); collect_ids(val, out); }),
        _ => {}
    }
}

/// Insert or overwrite a byte-keyed dict entry.
fn set_entry(d: &mut Vec<(Value, Value)>, key: &[u8], val: Value) {
    if let Some(slot) = d.iter_mut().find(|(k, _)| is_b(k, key)) {
        slot.1 = val;
    } else {
        d.push((Value::Bytes(key.to_vec()), val));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
    fn ts() -> Value { Value::Long(vec![0u8; 8]) }

    // root -> windows -> { stacksWindows, preferredIdxInStack3 }, with a Shared
    // container id to prove inline-first (RemoveEntry would refuse it raw).
    fn root() -> Value {
        let container = Value::Shared { slot: 3, value: Box::new(b("C")) };
        Value::Dict(vec![(b("windows"), Value::Dict(vec![
            (b("stacksWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("m1"), container),
                (b("m2"), Value::Ref(3)),
            ])])),
            (b("preferredIdxInStack3"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("C"), Value::Dict(vec![
                    (b("m1"), Value::Int(0)), (b("m2"), Value::Int(1)),
                ])),
            ])])),
        ]))])
    }

    // Read helpers: navigate the (inlined) tree to the two dicts.
    fn win<'a>(v: &'a Value) -> &'a Vec<(Value, Value)> {
        let Value::Dict(top) = v else { panic!() };
        let (_, w) = top.iter().find(|(k, _)| matches!(k, Value::Bytes(x) if x == b"windows")).unwrap();
        let Value::Dict(d) = w else { panic!() };
        d
    }
    fn inner<'a>(win: &'a [(Value, Value)], name: &[u8]) -> &'a Vec<(Value, Value)> {
        let (_, v) = win.iter().find(|(k, _)| matches!(k, Value::Bytes(x) if x == name)).unwrap();
        match v { Value::Tuple(t) => { let Value::Dict(d) = &t[1] else { panic!() }; d }, Value::Dict(d) => d, _ => panic!() }
    }
    fn sw(v: &Value) -> &Vec<(Value, Value)> { inner(win(v), b"stacksWindows") }
    fn pref(v: &Value) -> &Vec<(Value, Value)> { inner(win(v), b"preferredIdxInStack3") }
    fn keys(d: &[(Value, Value)]) -> Vec<String> {
        d.iter().map(|(k, _)| match k { Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(), _ => String::new() }).collect()
    }

    #[test]
    fn unstack_removes_the_member_from_both_dicts() {
        let mut v = root();
        unstack(&mut v, "m1").unwrap();
        assert_eq!(keys(sw(&v)), vec!["m2".to_string()]);
        // preferredIdxInStack3[C] no longer lists m1.
        let (_, cdict) = pref(&v).iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == b"C")).unwrap();
        let Value::Dict(inner) = cdict else { panic!() };
        assert_eq!(keys(inner), vec!["m2".to_string()]);
    }

    #[test]
    fn add_inserts_into_both_dicts_with_next_index() {
        let mut v = root();
        add_to_stack(&mut v, "m3", "C").unwrap();
        assert!(keys(sw(&v)).contains(&"m3".to_string()));
        let (_, cdict) = pref(&v).iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == b"C")).unwrap();
        let Value::Dict(inner) = cdict else { panic!() };
        // m3 gets the next index (2) after m1(0), m2(1).
        let (_, idx) = inner.iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == b"m3")).unwrap();
        assert_eq!(*idx, Value::Int(2));
    }

    #[test]
    fn reorder_rewrites_indices_to_clean_0_n() {
        let mut v = root();
        reorder_stack(&mut v, "C", &["m2".into(), "m1".into()]).unwrap();
        let (_, cdict) = pref(&v).iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == b"C")).unwrap();
        let Value::Dict(inner) = cdict else { panic!() };
        let idx = |id: &[u8]| { let (_, v) = inner.iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == id)).unwrap(); v.clone() };
        assert_eq!(idx(b"m2"), Value::Int(0));
        assert_eq!(idx(b"m1"), Value::Int(1));
    }

    #[test]
    fn unstack_a_missing_member_errors() {
        let mut v = root();
        assert!(matches!(unstack(&mut v, "nope"), Err(StackError::NotStacked { .. })));
    }

    #[test]
    fn unstack_that_drops_a_shared_def_still_encodes() {
        let mut v = root();
        unstack(&mut v, "m1").unwrap();
        // Without inline-first, m2's Ref to m1's dropped Shared def would dangle
        // (RefBeforeStore) — this proves inline_all runs before the edit.
        let bytes = blue_marshal::encode(&v).expect("edited tree still encodes");
        assert_eq!(blue_marshal::decode(&bytes).unwrap(), v);
    }

    fn free_windows_root() -> Value {
        // Two free windows m1 (rect x=10) and m2 (rect x=99), plus the flag dicts.
        fn geom(x: i64) -> Value {
            Value::Tuple(vec![Value::Int(x), Value::Int(0), Value::Int(100), Value::Int(80), Value::Int(2560), Value::Int(1440)])
        }
        let boolset = |ids: &[&str], val: bool| Value::Tuple(vec![ts(), Value::Dict(
            ids.iter().map(|i| (b(i), Value::Bool(val))).collect())]);
        Value::Dict(vec![(b("windows"), Value::Dict(vec![
            (b("windowSizesAndPositions_1"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("m1"), geom(10)), (b("m2"), geom(99)), (b("40"), geom(0)),
            ])])),
            (b("openWindows"), boolset(&["m1", "m2"], false)),
            (b("isLightBackgroundWindows"), boolset(&[], false)),
            (b("isOverlayedWindows"), boolset(&[], false)),
            (b("minimizedWindows"), boolset(&[], false)),
            (b("stacksWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![])])),
            (b("preferredIdxInStack3"), Value::Tuple(vec![ts(), Value::Dict(vec![])])),
        ]))])
    }

    fn geom_of(v: &Value, id: &[u8]) -> Vec<i64> {
        let g = inner(win(v), b"windowSizesAndPositions_1");
        let (_, t) = g.iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == id)).unwrap();
        let Value::Tuple(t) = t else { panic!() };
        t.iter().map(|e| if let Value::Int(i) = e { *i } else { 0 }).collect()
    }
    fn boolval(v: &Value, dict: &[u8], id: &[u8]) -> Option<bool> {
        let d = inner(win(v), dict);
        d.iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == id)).and_then(|(_, v)| if let Value::Bool(x) = v { Some(*x) } else { None })
    }

    #[test]
    fn create_mints_a_free_high_id_and_lands_at_m1_rect() {
        let mut v = free_windows_root();
        let c = create_stack(&mut v, "m1", "m2").unwrap();
        // "40" and "m1"/"m2" already exist; the minted id must be free (not "40").
        assert_ne!(c, "40");
        assert!(c.parse::<i64>().is_ok(), "container id is a numeric string");
        // Container + both members share M1's rect (x = 10).
        assert_eq!(geom_of(&v, c.as_bytes())[0], 10);
        assert_eq!(geom_of(&v, b"m2")[0], 10, "m2 moved to m1's rect");
        assert_eq!(geom_of(&v, b"m1")[0], 10);
    }

    #[test]
    fn create_links_members_opens_and_flags_the_container() {
        let mut v = free_windows_root();
        let c = create_stack(&mut v, "m1", "m2").unwrap();
        let cb = c.as_bytes();
        // stacksWindows: both members -> C.
        let get = |id: &[u8]| sw(&v).iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == id)).map(|(_, v)| v.clone());
        assert_eq!(get(b"m1"), Some(Value::Bytes(cb.to_vec())));
        assert_eq!(get(b"m2"), Some(Value::Bytes(cb.to_vec())));
        // preferredIdxInStack3[C] = {m1:0, m2:1}.
        let (_, cd) = pref(&v).iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == cb)).unwrap();
        let Value::Dict(cd) = cd else { panic!() };
        assert_eq!(keys(cd), vec!["m1".to_string(), "m2".to_string()]);
        // Open: C, m1, m2 all true.
        assert_eq!(boolval(&v, b"openWindows", cb), Some(true));
        assert_eq!(boolval(&v, b"openWindows", b"m1"), Some(true));
        // Container marked in the three state dicts (False).
        assert_eq!(boolval(&v, b"isOverlayedWindows", cb), Some(false));
        assert_eq!(boolval(&v, b"minimizedWindows", cb), Some(false));
        assert_eq!(boolval(&v, b"isLightBackgroundWindows", cb), Some(false));
    }
}
