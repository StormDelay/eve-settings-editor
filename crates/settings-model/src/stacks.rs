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
}
