# Codec re-share foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `blue_marshal::reshare` pass that re-introduces compact, valid, immutable-only `Shared`/`Ref` sharing, and run it before encode on the inline-first structural editors so edited files stop shipping the ~1.5× inlined blob that relies on EVE re-deduplicating.

**Architecture:** A pure `reshare(&Value) -> Value` in `blue-marshal` inlines the tree, then shares each repeated *immutable* subtree (byte-strings, longs, globals, all-immutable tuples) by structural equality, assigning slots in the encoder's own traversal order so store-before-ref holds by construction. The corpus-critical byte-identical replay encoder is untouched. The three inline-first paths (`overview`, `autofill`, `batch`) call `reshare` after their edit; the in-place `SetScalar`/geometry/width paths are left alone.

**Tech Stack:** Rust (workspace crates `blue-marshal`, `settings-model`, `app_lib`), `cargo test`. Tests are plain `#[test]` (no new dependencies).

## Global Constraints

- **Commits:** sentence-case subject, **no** attribution/`Co-Authored-By` trailers (repo convention).
- **No new dependencies** in any crate — `blue-marshal` is dependency-free; `reshare` uses only `std` + existing `crate::encode`/`crate::value`.
- **No personal data in committed code/tests** — all fixtures use synthetic ids/values, never real character/account ids.
- **Corpus tests skip-if-missing** — mirror the existing pattern (`eprintln!` + `return` when `testdata/corpus` is absent), never fail on a missing corpus.
- **`cargo` is on the Bash tool PATH** here; `gh`/`npm` are not (use PowerShell for those, not needed in this plan).
- **Do not touch** `blue-marshal`'s `encode.rs` replay logic or the `every_corpus_file_reencodes_byte_identically` gate — both must stay exactly as-is and green.

---

### Task 1: `blue_marshal::reshare` pass

**Files:**
- Create: `crates/blue-marshal/src/reshare.rs`
- Modify: `crates/blue-marshal/src/lib.rs:4-15` (declare module + re-export)

**Interfaces:**
- Consumes: `crate::value::Value`, `crate::encode::encode`.
- Produces: `pub fn reshare(root: &Value) -> Value` and `pub fn inline(root: &Value) -> Value`. `reshare` returns a semantically-equal tree whose repeated immutable subtrees are shared (`Shared`/`Ref`) and which always `encode`s successfully. `inline` returns a sharing-free deep copy. Both are re-exported at the crate root (`blue_marshal::reshare`, `blue_marshal::inline`).

- [ ] **Step 1: Write the failing test**

Create `crates/blue-marshal/src/reshare.rs` with only its test module for now:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p blue-marshal reshare 2>&1 | tail -20`
Expected: FAIL to compile — `reshare`/`inline` not defined.

- [ ] **Step 3: Write the implementation**

Prepend the implementation above the test module in `crates/blue-marshal/src/reshare.rs`:

```rust
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
```

Then wire it into `crates/blue-marshal/src/lib.rs`: add `pub mod reshare;` to the module list and `pub use reshare::{inline, reshare};` to the re-exports.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p blue-marshal reshare 2>&1 | tail -20`
Expected: PASS (all six `reshare::tests::*`).

- [ ] **Step 5: Commit**

```bash
git add crates/blue-marshal/src/reshare.rs crates/blue-marshal/src/lib.rs
git commit -m "Add blue_marshal reshare immutable-only canonicalization pass"
```

---

### Task 2: Corpus semantic-preservation gate for reshare

**Files:**
- Modify: `crates/blue-marshal/tests/corpus.rs` (append a new `#[test]`)

**Interfaces:**
- Consumes: `blue_marshal::{decode, encode, reshare}` from Task 1, and the file-walk helper `collect_dat_files` already in this file.
- Produces: nothing consumed by later tasks — a standing regression gate.

- [ ] **Step 1: Write the failing test**

Append to `crates/blue-marshal/tests/corpus.rs`:

```rust
/// Codec re-share gate: for every corpus file, `reshare` must preserve the
/// value and produce a stream that encodes and round-trips. `reshare` inlines
/// internally and is a normalizer, so re-normalizing after a wire round-trip
/// must land on the identical value — that proves no value was dropped or
/// corrupted and that the emitted sharing satisfies store-before-ref. The
/// byte-identical replay gate above is unchanged and still guards the read path.
#[test]
fn reshare_preserves_every_corpus_file() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/corpus");
    if !corpus.is_dir() {
        eprintln!("corpus missing at {corpus:?} — skipping (run tools/sync-corpus.ps1)");
        return;
    }
    let mut files = Vec::new();
    collect_dat_files(&corpus, &mut files);
    if files.is_empty() {
        eprintln!("corpus empty at {corpus:?} — skipping (run tools/sync-corpus.ps1)");
        return;
    }
    let mut failures = Vec::new();
    for f in &files {
        let data = fs::read(f).unwrap();
        let Ok(value) = blue_marshal::decode(&data) else {
            failures.push(format!("{}: decode", f.display()));
            continue;
        };
        let reshared = blue_marshal::reshare(&value);
        let bytes = match blue_marshal::encode(&reshared) {
            Ok(b) => b,
            Err(e) => {
                failures.push(format!("{}: reshared encode: {e}", f.display()));
                continue;
            }
        };
        match blue_marshal::decode(&bytes) {
            Ok(back) if blue_marshal::reshare(&back) == reshared => {}
            Ok(_) => failures.push(format!("{}: reshare not preserved by round-trip", f.display())),
            Err(e) => failures.push(format!("{}: reshared decode: {e}", f.display())),
        }
    }
    assert!(
        failures.is_empty(),
        "{}/{} corpus files failed the reshare gate:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
}
```

- [ ] **Step 2: Run test to verify it passes (gate is expected green with Task 1 done)**

Run: `cargo test -p blue-marshal --test corpus reshare_preserves 2>&1 | tail -20`
Expected: PASS if the corpus is present, or the "corpus missing — skipping" line and PASS if not. (This gate is written after its dependency exists, so it should pass immediately; if it fails, the failure names the first offending file and reason — fix Task 1, do not weaken the gate.)

- [ ] **Step 3: Commit**

```bash
git add crates/blue-marshal/tests/corpus.rs
git commit -m "Gate reshare semantic preservation across the corpus"
```

---

### Task 3: Fold `overview.rs::inline_user` into the shared helper

**Files:**
- Modify: `crates/settings-model/src/overview.rs:313-320` (delete `inline_user`), `:355` and `:410` (call `inline_all`), and the `use` at the top of the file.

**Interfaces:**
- Consumes: `crate::treewalk::inline_all` (already `pub(crate)`).
- Produces: no signature change — `set_column_visible`/`set_column_order` behave identically; this is the deferred dedup cleanup, gated by the existing overview tests.

- [ ] **Step 1: Delete the private copy and switch the call sites**

In `crates/settings-model/src/overview.rs`, remove the `inline_user` fn (lines 313-320, keep the doc-comment intent by moving nothing — `inline_all` is already documented in treewalk). Replace its two call sites:

```rust
// was: inline_user(user);  (in set_column_visible, ~line 355)
crate::treewalk::inline_all(user);
```

```rust
// was: inline_user(user);  (in set_column_order, ~line 410)
crate::treewalk::inline_all(user);
```

Update the module's `use` line so `collect_shared`/`inline_shares`/`SharedTable` are dropped if now unused (the compiler warns; remove exactly the newly-unused names). Fix the two doc-comment references "post-`inline_user`" → "post-`inline_all`" in `migrate_legacy_overview` and `overview_entries_mut`.

- [ ] **Step 2: Run the overview tests**

Run: `cargo test -p settings-model overview 2>&1 | tail -25`
Expected: PASS — same behavior, `inline_user` gone. Also confirm no unused-import warnings: `cargo build -p settings-model 2>&1 | tail -5` shows no warnings from `overview.rs`.

- [ ] **Step 3: Run the overview realshape gate**

Run: `cargo test -p settings-model --test overview_realshape 2>&1 | tail -15`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/settings-model/src/overview.rs
git commit -m "Replace overview inline_user with the shared inline_all helper"
```

---

### Task 4: Reshare batch category splices

**Files:**
- Modify: `crates/settings-model/src/batch.rs:57-69` (`apply_to_tree`), and its test module.

**Interfaces:**
- Consumes: `blue_marshal::reshare` (Task 1).
- Produces: `apply_to_tree` now leaves `target` compactly reshared instead of fully inlined. `apply_categories_to` (unchanged signature) therefore saves a compact file.

- [ ] **Step 1: Write the failing test**

Append to `crates/settings-model/src/batch.rs`'s `#[cfg(test)] mod tests`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p settings-model apply_to_tree_leaves_a_compact 2>&1 | tail -20`
Expected: FAIL — `shared_count` is 0 (currently the tree is left inlined).

- [ ] **Step 3: Add the reshare step**

In `crates/settings-model/src/batch.rs`, at the end of `apply_to_tree`, after the splice loop and before the function returns, reshare the target:

```rust
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
```

(Note the early `return` on a non-dict target is replaced by the `if let` so the reshare still runs; a non-dict target has nothing to splice and reshare is a harmless normalize.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p settings-model apply_to_tree_leaves_a_compact 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Run the full batch + realshape suites**

Run: `cargo test -p settings-model batch 2>&1 | tail -25` then `cargo test -p settings-model --test batch_realshape 2>&1 | tail -15`
Expected: PASS (existing splice/encode tests unaffected — a resharded tree still encodes and re-opens Editable).

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/src/batch.rs
git commit -m "Reshare batch category splices into a compact file"
```

---

### Task 5: Reshare overview and autofill edits at the app boundary

**Files:**
- Modify: `app/src-tauri/src/ops.rs:607-620` (`edit_user_overview`) and `:650-662` (`edit_user_autofill`), and the ops test module.

**Interfaces:**
- Consumes: `blue_marshal::reshare` (Task 1). `blue_marshal` is already a dependency of the app crate.
- Produces: after any overview visibility/order edit or any autofill list edit, the in-memory user document is compactly reshared, so the next save writes a compact file. The width path (`set_overview_width`) and the generic `save`/raw-tree path are deliberately NOT changed.

- [ ] **Step 1: Write the failing test**

Append to `app/src-tauri/src/ops.rs`'s `#[cfg(test)] mod tests`, reusing the existing `overview_user_bytes`/`temp_file` helpers and the `AppState::new()` + `open_file` open pattern (as in `overview_reads_and_edits_the_user_slot`). The overview fixture repeats the byte-string `b"NAME"` across `tabColumnOrder` and `tabColumns`, so resharing must emit at least one shared object:

```rust
#[test]
fn overview_edit_leaves_the_user_doc_compactly_shared() {
    let path = temp_file("ov-reshare", &overview_user_bytes());
    let state = AppState::new();
    open_file(&state, Slot::User, path.to_str().unwrap()).unwrap();

    set_overview_order(&state, 0, vec!["TYPE".into(), "NAME".into()]).unwrap();

    let guard = state.user.lock().unwrap();
    let doc = guard.as_ref().unwrap();
    let bytes = blue_marshal::encode(&doc.value).unwrap();
    // Repeated column tokens must be shared (stream shared-count > 0), not left
    // fully inlined, and the reshared doc must round-trip.
    let shared_count = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
    assert!(shared_count > 0, "overview edit should reshare repeated tokens");
    assert_eq!(blue_marshal::decode(&bytes).unwrap(), doc.value, "reshared doc round-trips");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p app_lib overview_edit_leaves 2>&1 | tail -20`
Expected: FAIL — without reshare `doc.value` is fully inlined, so `shared_count == 0`.

- [ ] **Step 3: Add reshare to both edit boundaries**

In `app/src-tauri/src/ops.rs`, in `edit_user_overview`, after the edit closure runs and before the lock is released:

```rust
        edit(&mut doc.value).map_err(|e| ErrDto::new("overview", format!("{e:?}")))?;
        // Compact the inline-first edit before it can be saved.
        doc.value = blue_marshal::reshare(&doc.value);
```

In `edit_user_autofill`, the same insertion after its edit closure:

```rust
        edit(&mut doc.value).map_err(|e| ErrDto::new("autofill", format!("{e:?}")))?;
        doc.value = blue_marshal::reshare(&doc.value);
```

Do **not** add reshare to `set_overview_width` (it edits widths in place and preserves sharing) or to `save_document`/`save` (the generic byte-faithful path).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p app_lib overview_edit_leaves 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Run the overview + autofill ops suites**

Run: `cargo test -p app_lib overview 2>&1 | tail -20` then `cargo test -p app_lib autofill 2>&1 | tail -20`
Expected: PASS — projection after reshare still resolves the shared tokens (existing `overview_reads_and_edits_the_user_slot` / `autofill_reads_edits_and_clears_the_user_slot` unaffected).

- [ ] **Step 6: Commit**

```bash
git add app/src-tauri/src/ops.rs
git commit -m "Reshare overview and autofill edits before save"
```

---

### Task 6: Whole-workspace verification and live smoke

**Files:** none (verification gate).

**Interfaces:** none.

- [ ] **Step 1: Run the full workspace test suite**

Run: `cargo test --workspace 2>&1 | tail -40`
Expected: PASS for `blue-marshal` (incl. both corpus gates: `every_corpus_file_reencodes_byte_identically` still green, `reshare_preserves_every_corpus_file` green), `settings-model` (incl. `corpus_load`, `batch`, all realshape gates), and `app_lib`.

- [ ] **Step 2: Confirm the byte-identical gate is untouched**

Run: `cargo test -p blue-marshal --test corpus every_corpus_file_reencodes_byte_identically 2>&1 | tail -8`
Expected: PASS — the replay encoder still reproduces every corpus file exactly. (If this ever fails, `encode.rs` was wrongly modified — revert that; reshare must never touch it.)

- [ ] **Step 3: Frontend check (no frontend change expected, but confirm nothing regressed)**

Run (PowerShell tool, since `npm` is not on the Bash PATH): `npm --prefix app run check`
Expected: svelte-check 0 errors (this milestone is Rust-only; this is a sanity gate).

- [ ] **Step 4: Live smoke (project exit gate — record in docs/format-notes.md)**

Drive the real app against a real file (per the project's live-smoke convention). Manually:
1. Open a real `core_user` file, make an **overview** column edit (show/hide or reorder) and Save. Confirm the app reports a smaller `bytes_written` than the pre-reshare build would have, the file re-opens **Editable**, and EVE accepts it in-game.
2. Repeat with an **autofill** edit (add/clear a remembered list) and Save.
3. Run a **batch** category copy (Layout or Overview) onto a target and confirm the written target re-opens Editable and is compact.

Record the outcome (compact sizes, Editable on reopen, in-game acceptance) under `docs/format-notes.md` `## Status` with a dated entry, using no real ids.

- [ ] **Step 5: Commit the format-notes entry**

```bash
git add docs/format-notes.md
git commit -m "Record the codec re-share live smoke result"
```

---

## Notes for the implementer

- **Why reshare lives in `blue-marshal`, not `settings-model`:** it is a wire-level transform that needs `storable_with_flag` knowledge and must mirror the encoder's traversal order. Keeping it beside `encode` is correct; `settings-model` depends on `blue-marshal`, not the reverse.
- **The one correctness invariant to protect:** `rebuild` must visit children in the encoder's emit order (dict value-before-key; reduce pairs key-before-value). If that drifts, the encoder rejects the output with `RefBeforeStore` — which is why every task encodes-and-round-trips its result. Do not "fix" such a failure by weakening the encoder; fix the traversal order.
- **Do not** bake reshare into `save`/`save_document`: the in-place edit paths (M2 geometry, raw tree, overview widths) retain CCP's original container sharing via the replay encoder, and resharing them immutable-only would *drop* that container sharing and enlarge those files.
