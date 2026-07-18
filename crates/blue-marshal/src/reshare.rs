//! Re-derive compact, valid `Shared`/`Ref` sharing for a tree by sharing
//! repeated IMMUTABLE values (structural equality). A fully-inlined edited tree
//! then encodes compactly instead of ~1.5x, without relying on the EVE client
//! re-deduplicating. Immutable-only by design: CCP shares by object identity,
//! so sharing mutable containers (list/dict/instance/reduce/stream) by
//! structural equality could alias values the client kept distinct. See
//! docs/superpowers/specs/2026-07-18-codec-reshare-foundation-design.md.
//!
//! The byte-identical replay encoder (encode.rs) is deliberately untouched; this
//! is a separate pre-encode pass used only by the inline-first editors.

use std::collections::HashMap;

use crate::encode::encode;
use crate::value::Value;

/// Canonical, compactly-shared copy of `root`. Accepts any tree (existing
/// `Shared`/`Ref` are inlined away first), so it is safe on already-shared or
/// already-reshared input. Only immutable values are shared. Slots are assigned
/// in the encoder's traversal order, so its store-before-ref and contiguous
/// `1..=count` invariants hold by construction.
pub fn reshare(root: &Value) -> Value {
    let inlined = inline(root);
    let mut counts: HashMap<Vec<u8>, usize> = HashMap::new();
    tally(&inlined, &mut counts);
    let mut slots: HashMap<Vec<u8>, u32> = HashMap::new();
    let mut next: u32 = 1;
    rebuild(&inlined, &counts, &mut slots, &mut next)
}

/// Deep-resolve every `Shared`/`Ref` into a sharing-free owned tree.
pub fn inline(root: &Value) -> Value {
    let mut table: HashMap<u32, Value> = HashMap::new();
    collect(root, &mut table);
    resolve(root, &table)
}

fn collect(v: &Value, out: &mut HashMap<u32, Value>) {
    match v {
        Value::Shared { slot, value } => {
            out.insert(*slot, (**value).clone());
            collect(value, out);
        }
        Value::Tuple(xs) | Value::List(xs) => xs.iter().for_each(|c| collect(c, out)),
        Value::Dict(es) => es.iter().for_each(|(k, val)| {
            collect(k, out);
            collect(val, out);
        }),
        Value::Stream(inner) => collect(inner, out),
        Value::Instance { class, state } => {
            collect(class, out);
            collect(state, out);
        }
        Value::Reduce { ctor, items, pairs } => {
            collect(ctor, out);
            items.iter().for_each(|c| collect(c, out));
            pairs.iter().for_each(|(k, val)| {
                collect(k, out);
                collect(val, out);
            });
        }
        _ => {}
    }
}

fn resolve(v: &Value, table: &HashMap<u32, Value>) -> Value {
    match v {
        Value::Shared { value, .. } => resolve(value, table),
        Value::Ref(slot) => match table.get(slot) {
            Some(t) => resolve(t, table),
            None => v.clone(),
        },
        Value::Tuple(xs) => Value::Tuple(xs.iter().map(|c| resolve(c, table)).collect()),
        Value::List(xs) => Value::List(xs.iter().map(|c| resolve(c, table)).collect()),
        Value::Dict(es) => {
            Value::Dict(es.iter().map(|(k, val)| (resolve(k, table), resolve(val, table))).collect())
        }
        Value::Stream(inner) => Value::Stream(Box::new(resolve(inner, table))),
        Value::Instance { class, state } => Value::Instance {
            class: Box::new(resolve(class, table)),
            state: Box::new(resolve(state, table)),
        },
        Value::Reduce { ctor, items, pairs } => Value::Reduce {
            ctor: Box::new(resolve(ctor, table)),
            items: items.iter().map(|c| resolve(c, table)).collect(),
            pairs: pairs.iter().map(|(k, val)| (resolve(k, table), resolve(val, table))).collect(),
        },
        scalar => scalar.clone(),
    }
}

/// Immutable in the Python sense: aliasing it can never be observed as a shared
/// mutation. Containers that EVE could mutate in place are excluded.
fn is_immutable(v: &Value) -> bool {
    match v {
        Value::None
        | Value::Bool(_)
        | Value::Int(_)
        | Value::Float(_)
        | Value::Long(_)
        | Value::Bytes(_)
        | Value::Str(_)
        | Value::StrUcs2(_)
        | Value::StrTable(_)
        | Value::Global(_) => true,
        Value::Tuple(xs) => xs.iter().all(is_immutable),
        _ => false, // List, Dict, Stream, Instance, Reduce, Shared, Ref
    }
}

/// A value that is both immutable AND may carry `SHARED_FLAG` on the opcode the
/// encoder will choose (mirrors encode.rs `storable_with_flag`): `Bytes` len ≥ 2
/// (len ≤ 1 emits STRING0/STRING1, which do not store), `Long`, `Global`, and a
/// non-empty all-immutable `Tuple` (TUPLE0 does not store).
fn is_shareable(v: &Value) -> bool {
    match v {
        Value::Bytes(b) => b.len() >= 2,
        Value::Long(_) | Value::Global(_) => true,
        Value::Tuple(xs) => !xs.is_empty() && xs.iter().all(is_immutable),
        _ => false,
    }
}

/// Structural dedup key: the value's own wire encoding. Deterministic and
/// injective enough — two values share iff they encode identically. A shareable
/// value always encodes, so `None` only on an unexpected error (then: don't share).
fn key(v: &Value) -> Option<Vec<u8>> {
    encode(v).ok()
}

fn tally(v: &Value, counts: &mut HashMap<Vec<u8>, usize>) {
    if is_shareable(v) {
        if let Some(k) = key(v) {
            *counts.entry(k).or_insert(0) += 1;
        }
        return; // atomic sharing unit — do not descend into it
    }
    match v {
        Value::Tuple(xs) | Value::List(xs) => xs.iter().for_each(|c| tally(c, counts)),
        Value::Dict(es) => es.iter().for_each(|(k, val)| {
            tally(val, counts);
            tally(k, counts);
        }),
        Value::Stream(inner) => tally(inner, counts),
        Value::Instance { class, state } => {
            tally(class, counts);
            tally(state, counts);
        }
        Value::Reduce { ctor, items, pairs } => {
            tally(ctor, counts);
            items.iter().for_each(|c| tally(c, counts));
            pairs.iter().for_each(|(k, val)| {
                tally(k, counts);
                tally(val, counts);
            });
        }
        _ => {}
    }
}

/// Rebuild the tree sharing repeated shareables. MUST traverse in the encoder's
/// emit order (dict value-before-key; reduce pairs key-before-value) so the
/// first occurrence — which becomes the `Shared` store — is numbered before any
/// `Ref` to it, keeping the encoder's store-before-ref invariant satisfied.
fn rebuild(
    v: &Value,
    counts: &HashMap<Vec<u8>, usize>,
    slots: &mut HashMap<Vec<u8>, u32>,
    next: &mut u32,
) -> Value {
    if is_shareable(v) {
        if let Some(k) = key(v) {
            if counts.get(&k).copied().unwrap_or(0) >= 2 {
                if let Some(&slot) = slots.get(&k) {
                    return Value::Ref(slot);
                }
                let slot = *next;
                *next += 1;
                slots.insert(k, slot);
                return Value::Shared { slot, value: Box::new(v.clone()) };
            }
        }
        return v.clone();
    }
    match v {
        Value::Tuple(xs) => Value::Tuple(xs.iter().map(|c| rebuild(c, counts, slots, next)).collect()),
        Value::List(xs) => Value::List(xs.iter().map(|c| rebuild(c, counts, slots, next)).collect()),
        Value::Dict(es) => Value::Dict(
            es.iter()
                .map(|(k, val)| {
                    let nv = rebuild(val, counts, slots, next); // value first (encode order)
                    let nk = rebuild(k, counts, slots, next);
                    (nk, nv)
                })
                .collect(),
        ),
        Value::Stream(inner) => Value::Stream(Box::new(rebuild(inner, counts, slots, next))),
        Value::Instance { class, state } => Value::Instance {
            class: Box::new(rebuild(class, counts, slots, next)),
            state: Box::new(rebuild(state, counts, slots, next)),
        },
        Value::Reduce { ctor, items, pairs } => Value::Reduce {
            ctor: Box::new(rebuild(ctor, counts, slots, next)),
            items: items.iter().map(|c| rebuild(c, counts, slots, next)).collect(),
            pairs: pairs
                .iter()
                .map(|(k, val)| {
                    let nk = rebuild(k, counts, slots, next); // key first (reduce order)
                    let nv = rebuild(val, counts, slots, next);
                    (nk, nv)
                })
                .collect(),
        },
        scalar => scalar.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::decode;
    use crate::encode::encode;
    use crate::value::Value;

    fn b(s: &str) -> Value {
        Value::Bytes(s.as_bytes().to_vec())
    }

    // A tree where the byte-string "overview" appears three times across a
    // mutable dict/list structure — exactly the real bloat shape.
    fn repeated_bytes_tree() -> Value {
        Value::Dict(vec![
            (b("openWindows"), Value::List(vec![b("overview"), b("market")])),
            (b("lockedWindows"), Value::List(vec![b("overview")])),
            (b("stacks"), Value::List(vec![b("overview")])),
        ])
    }

    #[test]
    fn shares_repeated_immutable_and_leaves_unique_alone() {
        let out = reshare(&repeated_bytes_tree());
        // "overview" repeats -> exactly one Shared definition + Refs for the rest.
        let mut shared_defs = 0usize;
        let mut refs = 0usize;
        count_share_nodes(&out, &mut shared_defs, &mut refs);
        assert_eq!(shared_defs, 1, "one Shared def for the repeated byte-string");
        assert!(refs >= 2, "later occurrences become Refs, got {refs}");
        // The unique "market" is never wrapped.
        assert!(!is_shared_value(&out, b"market"));
        // And it still encodes (store-before-ref holds) and round-trips.
        let bytes = encode(&out).expect("reshared tree encodes");
        assert_eq!(decode(&bytes).unwrap(), out, "reshared tree round-trips");
    }

    #[test]
    fn preserves_semantics() {
        let t = repeated_bytes_tree();
        // reshare is a normalizer: round-tripping the reshared tree through the
        // wire and re-normalizing lands on the same canonical value.
        let r = reshare(&t);
        let rt = decode(&encode(&r).unwrap()).unwrap();
        assert_eq!(reshare(&rt), r, "encode->decode preserves the reshared value");
        // And it agrees with the plain inlined value.
        assert_eq!(inline(&rt), inline(&t), "no value changed");
    }

    #[test]
    fn never_shares_mutable_containers() {
        // Two structurally-equal LISTS repeated; a List is mutable, so reshare
        // must NOT wrap it (only its repeated immutable *elements*).
        let list = || Value::List(vec![b("aa"), b("bb")]);
        let t = Value::Tuple(vec![list(), list()]);
        let out = reshare(&t);
        assert!(!any_shared_list(&out), "lists are never shared");
        // But the repeated byte-strings inside are shared.
        let (mut d, mut r) = (0, 0);
        count_share_nodes(&out, &mut d, &mut r);
        assert!(d >= 1 && r >= 1, "immutable elements still share");
        assert_eq!(inline(&decode(&encode(&out).unwrap()).unwrap()), inline(&t));
    }

    #[test]
    fn shares_repeated_immutable_tuples() {
        // Identical geometry tuples (all ints => immutable) repeated -> shared.
        let g = || Value::Tuple(vec![Value::Int(16), Value::Int(714), Value::Int(450)]);
        let t = Value::List(vec![g(), g(), g()]);
        let out = reshare(&t);
        let (mut d, mut r) = (0, 0);
        count_share_nodes(&out, &mut d, &mut r);
        assert_eq!(d, 1, "one shared tuple def");
        assert_eq!(r, 2, "two refs");
        assert_eq!(decode(&encode(&out).unwrap()).unwrap(), out);
    }

    #[test]
    fn is_idempotent_on_already_reshared_input() {
        let t = repeated_bytes_tree();
        let once = reshare(&t);
        let twice = reshare(&once); // reshare accepts shared input (inlines first)
        assert_eq!(once, twice);
    }

    #[test]
    fn compacts_versus_inlined() {
        let t = repeated_bytes_tree();
        let inlined_len = encode(&inline(&t)).unwrap().len();
        let reshared_len = encode(&reshare(&t)).unwrap().len();
        assert!(reshared_len < inlined_len, "{reshared_len} !< {inlined_len}");
    }

    // --- test helpers (walk the tree counting share nodes) ---
    fn count_share_nodes(v: &Value, defs: &mut usize, refs: &mut usize) {
        match v {
            Value::Shared { value, .. } => { *defs += 1; count_share_nodes(value, defs, refs); }
            Value::Ref(_) => *refs += 1,
            Value::Tuple(xs) | Value::List(xs) => xs.iter().for_each(|c| count_share_nodes(c, defs, refs)),
            Value::Dict(es) => es.iter().for_each(|(k, val)| { count_share_nodes(k, defs, refs); count_share_nodes(val, defs, refs); }),
            _ => {}
        }
    }
    fn is_shared_value(v: &Value, needle: &[u8]) -> bool {
        match v {
            Value::Shared { value, .. } => matches!(&**value, Value::Bytes(b) if b == needle),
            Value::Tuple(xs) | Value::List(xs) => xs.iter().any(|c| is_shared_value(c, needle)),
            Value::Dict(es) => es.iter().any(|(k, val)| is_shared_value(k, needle) || is_shared_value(val, needle)),
            _ => false,
        }
    }
    fn any_shared_list(v: &Value) -> bool {
        match v {
            Value::Shared { value, .. } => matches!(&**value, Value::List(_)) || any_shared_list(value),
            Value::Tuple(xs) | Value::List(xs) => xs.iter().any(any_shared_list),
            Value::Dict(es) => es.iter().any(|(k, val)| any_shared_list(k) || any_shared_list(val)),
            _ => false,
        }
    }
}
