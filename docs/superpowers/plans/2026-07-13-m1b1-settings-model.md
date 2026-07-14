# M1b-1 — settings-model Library & Save Chain Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `settings-model` crate: load a settings file with a byte-fidelity guarantee, project it as a JSON tree for the raw editor, apply mutations, and save through the full backup → verify → atomic-write chain — everything the Tauri app (M1b-2) calls into.

**Architecture:** `settings-model` sits on top of `blue-marshal` and owns EVE-side semantics-free document handling: `Document` (bytes + `Value` tree + load-time fidelity baseline), index-based `NodePath` addressing, a serde JSON projection for the UI, a small mutation set with shared-subtree guards, the spec §5 save-path invariant chain, backups/restore, and profile discovery. One small `blue-marshal` addition first: `bits_eq` (NaN-safe structural equality) and duplicate-slot rejection (corpus-proven zero).

**Tech Stack:** Rust stable (`serde`/`serde_json` allowed in `settings-model`; `blue-marshal` stays dependency-free).

**Scope note:** First half of M1b. M1b-2 (Tauri app + Svelte UI + CI) has its own plan and consumes exactly the interfaces produced here. The categories model (`WindowLayout` etc., spec §3) is M2/M3 — NOT built here.

## Global Constraints

- **Live-directory rule (spec §8):** tests and library code in this plan never read from or write to `%LOCALAPPDATA%\CCP\EVE\…`. Library functions take caller-supplied paths; only the (M1b-2) app passes live paths at runtime. Tests use `testdata/corpus/` copies and per-test temp dirs.
- `testdata/` stays gitignored. **No character/account names AND no real numeric character/account IDs in committed files.** Nothing in this plan needs a real ID: test fixtures and doc examples use obviously synthetic IDs (e.g. `core_char_123456789.dat`). If a step ever *seems* to require committing a real ID (copied from the corpus, a real filename, a real dump), STOP and ask the user first — do not commit it on your own judgment.
- Commit messages: sentence-case summary line. **No attribution trailers of any kind.**
- `blue-marshal` keeps **zero dependencies**. `settings-model` may use `serde` (with `derive`) and `serde_json` only.
- Corpus tests skip with a stderr note when `testdata/corpus/` is absent (CI has no corpus).
- Save-path invariant order (spec §5, binding): encode → verify → conflict check → backup → atomic write; **any failure aborts with the on-disk file untouched; no successful backup ⇒ no write, ever.**
- Load-time fidelity baseline (M1a final-review requirement, binding): a file is Editable **only if** `encode(decode(bytes)) == bytes` byte-for-byte at load; otherwise ReadOnly with a reason, and `save` must refuse it.

## Measured facts (2026-07-13, do not re-derive)

- **Duplicate tail-map slots: 0** across all 4,986 corpus files that have a shared map (5,022 total; the rest declare count 0). Promoting duplicates to a hard `DecodeError` is corpus-proven safe and makes `Ref(slot)` an unambiguous identifier for the mutation layer.
- Corpus directory layout (from real snapshots; the ID below is synthetic): `<EVE root>/<install>_<server>/settings_<profile>/core_(char|user)_<id>.dat`, e.g. `c_eve_sharedcache_tq_tranquility/settings_Default/core_char_123456789.dat`. The server name is the last `_`-separated token of the install dir; anomalous file names exist (`core_char__.dat`, `core_char_('char', None, 'dat').dat`) and must be tolerated (kind detected, `id: None`).
- `std::fs::rename` on Windows uses `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING` — temp-file-then-rename is a valid atomic replace on all three OSes.
- Backup timestamp format: `YYYY-MM-DDTHHMMSSZ` (ISO-8601 basic time — **no colons**, which are invalid in Windows file names; same convention as `tools/sync-corpus.ps1`).

## File Structure

```
Cargo.toml                                # workspace: add crates/settings-model
crates/blue-marshal/src/
├── value.rs                              # modify: add Value::bits_eq (Task 1)
├── decode.rs                             # modify: reject duplicate tail-map slots (Task 1)
└── error.rs                              # modify: DuplicateSharedSlot variant (Task 1)
crates/settings-model/
├── Cargo.toml
├── src/
│   ├── lib.rs                            # module decls + re-exports
│   ├── path.rs                           # Step / NodePath + resolve / resolve_mut (Task 2)
│   ├── document.rs                       # Document::load + Fidelity (Task 3)
│   ├── projection.rs                     # Node JSON tree + display strings (Task 4)
│   ├── mutate.rs                         # Mutation / NewValue / apply (Task 5)
│   ├── save.rs                           # save chain + timestamp helper (Task 6)
│   ├── backups.rs                        # list_backups + restore (Task 7)
│   └── discover.rs                       # profile discovery (Task 8)
└── tests/
    ├── corpus_load.rs                    # every corpus file loads Editable (Task 3)
    └── save_chain.rs                     # temp-dir integration tests (Task 6)
docs/format-notes.md                      # status bullet (Task 9)
```

---

### Task 1: blue-marshal additions — `bits_eq` and duplicate-slot rejection

Two small, corpus-gated changes to `blue-marshal` that the library above needs: NaN-safe structural equality for the save-chain verify step, and unambiguous `Ref` targets for the mutation layer.

**Files:**
- Modify: `crates/blue-marshal/src/value.rs`, `crates/blue-marshal/src/decode.rs`, `crates/blue-marshal/src/error.rs`

**Interfaces:**
- Consumes: the M1a `Value` model as merged on `master`.
- Produces: `Value::bits_eq(&self, &Value) -> bool` (Tasks 3/6 call it) and `ErrorKind::DuplicateSharedSlot(usize)`.

- [ ] **Step 1: Add `bits_eq` to `value.rs`**

Append to the existing `impl Value` block (after `unshared`):

```rust
    /// Structural equality with floats compared by bit pattern. The derived
    /// `PartialEq` uses `f64::eq`, under which `NaN != NaN` — so a tree
    /// containing a NaN payload would compare unequal to itself and fail the
    /// save-path verify step spuriously. Wire fidelity is bit-level, so
    /// equality here is too ( +0.0 and -0.0 are *different*, matching the
    /// encoder's FLOAT0 rule).
    pub fn bits_eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Float(a), Value::Float(b)) => a.to_bits() == b.to_bits(),
            (Value::Tuple(a), Value::Tuple(b)) | (Value::List(a), Value::List(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.bits_eq(y))
            }
            (Value::Dict(a), Value::Dict(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b)
                        .all(|((ak, av), (bk, bv))| ak.bits_eq(bk) && av.bits_eq(bv))
            }
            (Value::Stream(a), Value::Stream(b)) => a.bits_eq(b),
            (
                Value::Instance { class: ac, state: as_ },
                Value::Instance { class: bc, state: bs },
            ) => ac.bits_eq(bc) && as_.bits_eq(bs),
            (
                Value::Reduce { ctor: ac, items: ai, pairs: ap },
                Value::Reduce { ctor: bc, items: bi, pairs: bp },
            ) => {
                ac.bits_eq(bc)
                    && ai.len() == bi.len()
                    && ai.iter().zip(bi).all(|(x, y)| x.bits_eq(y))
                    && ap.len() == bp.len()
                    && ap
                        .iter()
                        .zip(bp)
                        .all(|((xk, xv), (yk, yv))| xk.bits_eq(yk) && xv.bits_eq(yv))
            }
            (
                Value::Shared { slot: a, value: av },
                Value::Shared { slot: b, value: bv },
            ) => a == b && av.bits_eq(bv),
            // Every remaining variant either has no f64 inside (scalars,
            // strings, bytes, Global, Ref) or differs in kind — the derived
            // PartialEq is exact for those.
            (a, b) => a == b,
        }
    }
```

- [ ] **Step 2: Add `bits_eq` tests to `value.rs`'s test module**

```rust
    #[test]
    fn bits_eq_handles_nan_and_signed_zero() {
        let nan = Value::Float(f64::NAN);
        assert!(nan.bits_eq(&nan.clone()), "NaN tree must equal its own clone");
        assert!(nan != nan.clone(), "derived PartialEq disagrees — that is the point");
        assert!(!Value::Float(0.0).bits_eq(&Value::Float(-0.0)));
        let t = Value::Tuple(vec![Value::Float(f64::NAN), Value::Int(1)]);
        assert!(t.bits_eq(&t.clone()));
        assert!(!t.bits_eq(&Value::Tuple(vec![Value::Float(f64::NAN), Value::Int(2)])));
    }
```

Run: `cargo test -p blue-marshal bits_eq`
Expected: PASS.

- [ ] **Step 3: Reject duplicate tail-map slots in the decoder**

In `crates/blue-marshal/src/error.rs`, add to `ErrorKind` after `UnconsumedSharedMap`:

```rust
    /// A tail-map slot number appeared more than once. The reference decoder
    /// tolerates this (last store wins), but it makes REF targets ambiguous;
    /// corpus-proven never to happen (0 duplicates across 4,986 files with
    /// shared maps), so it is rejected to keep `Ref(slot)` a unique
    /// identifier for the editing layer.
    DuplicateSharedSlot(usize),
```

In `crates/blue-marshal/src/decode.rs`, in `reserve_slot`, after the existing range validation (`if slot < 1 || slot as usize > n { ... }`), add:

```rust
        // Duplicate detection: a slot may be designated by only one map
        // entry. `shared` doubles as the seen-set — `true` means an earlier
        // entry already claimed the slot (stores complete before any REF can
        // read them, so no ordering hazard). Checked at reservation, i.e.
        // the second SHARED-flagged object fails at its own opcode offset.
        if self.shared[slot as usize - 1] {
            return Err(DecodeError { offset: at, kind: ErrorKind::DuplicateSharedSlot(slot as usize) });
        }
        self.shared[slot as usize - 1] = true;
```

Then in `load`, the store-completion write `self.shared[slot - 1] = true;` becomes redundant-but-harmless — **replace** the `Ok(match slot { ... })` block's `Some(slot)` arm so the flag is set at reservation only and completion just wraps:

```rust
        Ok(match slot {
            Some(slot) => Value::Shared { slot: slot as u32, value: Box::new(value) },
            None => value,
        })
```

**Wait — this changes REF semantics:** with the flag set at reservation (before children decode), a REF *inside* the still-open shared container would now pass the populated check that used to reject it. That must not happen. Keep store-on-completion semantics intact by using a **separate** seen-set instead. Final shape — in the `Decoder` struct add one field:

```rust
    /// Tail-map slots already designated by an earlier map entry (duplicate
    /// detection); distinct from `shared`, which flips only on completion.
    reserved: Vec<bool>,
```

construct it with `reserved: vec![false; shared_count]` in `decode_at_depth`, and in `reserve_slot` (after range validation):

```rust
        if self.reserved[slot as usize - 1] {
            return Err(DecodeError { offset: at, kind: ErrorKind::DuplicateSharedSlot(slot as usize) });
        }
        self.reserved[slot as usize - 1] = true;
```

`load`'s completion write to `self.shared` stays exactly as it is. (Do NOT implement the first, rejected variant — it is shown only to document why the second field exists.)

- [ ] **Step 4: Add the duplicate-slot test to `decode.rs`'s test module**

```rust
    #[test]
    fn duplicate_shared_map_slots_are_rejected() {
        // Two SHARED-flagged objects whose map entries both designate slot 1
        // (count = 2, map = [1, 1]). The reference lets the second store win;
        // we reject at the second reservation so Ref(slot) stays unique.
        let data = [
            0x7E, 0x02, 0x00, 0x00, 0x00, // header, shared_count = 2
            0x2C, // TUPLE2
            0x6F, 0x01, 0x2A, // LONG|SHARED, len 1 — reserves map[0] = 1
            0x6F, 0x01, 0x2B, // LONG|SHARED, len 1 — map[1] = 1 duplicate!
            0x01, 0x00, 0x00, 0x00, // tail map entry 0 = slot 1
            0x01, 0x00, 0x00, 0x00, // tail map entry 1 = slot 1 (duplicate)
        ];
        let err = decode(&data).unwrap_err();
        assert_eq!(err.kind, crate::ErrorKind::DuplicateSharedSlot(1));
        assert_eq!(err.offset, 9); // the second LONG|SHARED opcode byte
    }
```

- [ ] **Step 5: Full suite green (corpus gates prove the promotion safe), then commit**

Run: `cargo test`
Expected: all tests PASS including both corpus gates (5022/5022). If any corpus file fails with `DuplicateSharedSlot`, STOP — the measurement was wrong; report instead of weakening.

```powershell
git add -A
git commit -m "Add bit-level Value equality and reject duplicate shared-map slots"
```

---

### Task 2: settings-model scaffold and NodePath

**Files:**
- Create: `crates/settings-model/Cargo.toml`, `crates/settings-model/src/lib.rs`, `crates/settings-model/src/path.rs`
- Modify: `Cargo.toml` (workspace root)

**Interfaces:**
- Consumes: `blue_marshal::Value`.
- Produces: `Step` / `NodePath` (exact serde shape below — the UI sends these back verbatim), `resolve(&Value, &[Step]) -> Option<&Value>`, `resolve_mut(&mut Value, &[Step]) -> Option<&mut Value>`. Every later task and the M1b-2 app depend on these exact names.

- [ ] **Step 1: Create the crate and join the workspace**

Workspace root `Cargo.toml` becomes:

```toml
[workspace]
resolver = "2"
members = ["crates/blue-marshal", "crates/settings-model"]
```

`crates/settings-model/Cargo.toml`:

```toml
[package]
name = "settings-model"
version = "0.1.0"
edition = "2021"
license = "MIT"

[dependencies]
blue-marshal = { path = "../blue-marshal" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

`crates/settings-model/src/lib.rs`:

```rust
//! EVE settings document handling on top of the `blue-marshal` codec:
//! fidelity-checked loading, JSON tree projection, mutations, the
//! backup/verify/atomic save chain, backups, and profile discovery.
//! No EVE *semantics* live here yet (categories arrive in M2/M3).

pub mod backups;
pub mod discover;
pub mod document;
pub mod mutate;
pub mod path;
pub mod projection;
pub mod save;

pub use document::{Document, Fidelity, LoadError};
pub use mutate::{apply, Mutation, MutateError, NewValue};
pub use path::{resolve, resolve_mut, NodePath, Step};
pub use projection::{project, Node};
pub use save::{save, SaveError, SaveReport};
```

(Modules that don't exist yet would break the build — create each of `backups.rs`, `discover.rs`, `document.rs`, `mutate.rs`, `projection.rs`, `save.rs` as an empty file containing only `// Task N` placeholder comments naming their task, so `cargo build` works from this step onward. The re-export lines for not-yet-written items must be commented out with `// enabled in Task N` and uncommented by that task.)

- [ ] **Step 2: Write `path.rs`**

```rust
//! Index-based addressing of nodes in a `Value` tree. Paths are sequences
//! of steps from the root; indices (not keys) because dict keys are
//! arbitrary `Value`s (tuples are real keys in these files) and entry order
//! is wire order, which mutations preserve.

use blue_marshal::Value;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "s", content = "i", rename_all = "snake_case")]
pub enum Step {
    Tuple(usize),
    List(usize),
    DictKey(usize),
    DictValue(usize),
    InstanceClass,
    InstanceState,
    ReduceCtor,
    ReduceItem(usize),
    ReducePairKey(usize),
    ReducePairValue(usize),
    SharedInner,
    StreamInner,
}

pub type NodePath = Vec<Step>;

pub fn resolve<'a>(root: &'a Value, path: &[Step]) -> Option<&'a Value> {
    let mut cur = root;
    for step in path {
        cur = match (step, cur) {
            (Step::Tuple(i), Value::Tuple(items)) => items.get(*i)?,
            (Step::List(i), Value::List(items)) => items.get(*i)?,
            (Step::DictKey(i), Value::Dict(entries)) => &entries.get(*i)?.0,
            (Step::DictValue(i), Value::Dict(entries)) => &entries.get(*i)?.1,
            (Step::InstanceClass, Value::Instance { class, .. }) => class,
            (Step::InstanceState, Value::Instance { state, .. }) => state,
            (Step::ReduceCtor, Value::Reduce { ctor, .. }) => ctor,
            (Step::ReduceItem(i), Value::Reduce { items, .. }) => items.get(*i)?,
            (Step::ReducePairKey(i), Value::Reduce { pairs, .. }) => &pairs.get(*i)?.0,
            (Step::ReducePairValue(i), Value::Reduce { pairs, .. }) => &pairs.get(*i)?.1,
            (Step::SharedInner, Value::Shared { value, .. }) => value,
            (Step::StreamInner, Value::Stream(inner)) => inner,
            _ => return None,
        };
    }
    Some(cur)
}

pub fn resolve_mut<'a>(root: &'a mut Value, path: &[Step]) -> Option<&'a mut Value> {
    let mut cur = root;
    for step in path {
        cur = match (step, cur) {
            (Step::Tuple(i), Value::Tuple(items)) => items.get_mut(*i)?,
            (Step::List(i), Value::List(items)) => items.get_mut(*i)?,
            (Step::DictKey(i), Value::Dict(entries)) => &mut entries.get_mut(*i)?.0,
            (Step::DictValue(i), Value::Dict(entries)) => &mut entries.get_mut(*i)?.1,
            (Step::InstanceClass, Value::Instance { class, .. }) => class,
            (Step::InstanceState, Value::Instance { state, .. }) => state,
            (Step::ReduceCtor, Value::Reduce { ctor, .. }) => ctor,
            (Step::ReduceItem(i), Value::Reduce { items, .. }) => items.get_mut(*i)?,
            (Step::ReducePairKey(i), Value::Reduce { pairs, .. }) => &mut pairs.get_mut(*i)?.0,
            (Step::ReducePairValue(i), Value::Reduce { pairs, .. }) => &mut pairs.get_mut(*i)?.1,
            (Step::SharedInner, Value::Shared { value, .. }) => value,
            (Step::StreamInner, Value::Stream(inner)) => inner,
            _ => return None,
        };
    }
    Some(cur)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Value {
        // { b"windows": ( 1, [ 2.5 ] ) } with the tuple shared as slot 1
        Value::Dict(vec![(
            Value::Bytes(b"windows".to_vec()),
            Value::Shared {
                slot: 1,
                value: Box::new(Value::Tuple(vec![
                    Value::Int(1),
                    Value::List(vec![Value::Float(2.5)]),
                ])),
            },
        )])
    }

    #[test]
    fn resolve_walks_every_step_kind_used_in_real_files() {
        let v = sample();
        assert_eq!(
            resolve(&v, &[Step::DictKey(0)]),
            Some(&Value::Bytes(b"windows".to_vec()))
        );
        let deep = [
            Step::DictValue(0),
            Step::SharedInner,
            Step::Tuple(1),
            Step::List(0),
        ];
        assert_eq!(resolve(&v, &deep), Some(&Value::Float(2.5)));
        assert_eq!(resolve(&v, &[Step::DictValue(1)]), None); // out of range
        assert_eq!(resolve(&v, &[Step::List(0)]), None); // kind mismatch
    }

    #[test]
    fn resolve_mut_reaches_the_same_node() {
        let mut v = sample();
        let deep = [
            Step::DictValue(0),
            Step::SharedInner,
            Step::Tuple(0),
        ];
        *resolve_mut(&mut v, &deep).unwrap() = Value::Int(42);
        assert_eq!(resolve(&v, &deep), Some(&Value::Int(42)));
    }

    #[test]
    fn step_serde_shape_is_stable() {
        // The UI stores and replays these — the wire shape is a contract.
        let json = serde_json::to_string(&Step::DictValue(3)).unwrap();
        assert_eq!(json, r#"{"s":"dict_value","i":3}"#);
        let json = serde_json::to_string(&Step::SharedInner).unwrap();
        assert_eq!(json, r#"{"s":"shared_inner"}"#);
        let back: Step = serde_json::from_str(r#"{"s":"tuple","i":2}"#).unwrap();
        assert_eq!(back, Step::Tuple(2));
    }
}
```

- [ ] **Step 3: Build and test**

Run: `cargo test -p settings-model`
Expected: the three `path` tests PASS.

- [ ] **Step 4: Commit**

```powershell
git add -A
git commit -m "Scaffold settings-model with index-based node paths"
```

---

### Task 3: Document loading with the fidelity baseline

**Files:**
- Create: `crates/settings-model/src/document.rs` (replace placeholder), `crates/settings-model/tests/corpus_load.rs`
- Modify: `crates/settings-model/src/lib.rs` (uncomment the document re-export)

**Interfaces:**
- Consumes: `blue_marshal::{decode, encode, Value}`.
- Produces (exact — save chain, mutations, and the app all use these):

```rust
pub struct Document {
    pub path: PathBuf,
    pub value: Value,
    pub fidelity: Fidelity,
    pub(crate) loaded_mtime: Option<SystemTime>,
    pub(crate) loaded_len: u64,
}
pub enum Fidelity { Editable, ReadOnly { reason: String } }
pub enum LoadError { Io(String), Decode { offset: usize, message: String } }
pub fn Document::load(path: &Path) -> Result<Document, LoadError>
```

- [ ] **Step 1: Write `document.rs`**

```rust
//! A loaded settings file: original bytes, decoded tree, and the load-time
//! fidelity baseline that gates every save.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use blue_marshal::{decode, encode, Value};
use serde::Serialize;

/// Whether the document may be saved. Decided once, at load:
/// `encode(decode(bytes))` must reproduce the on-disk bytes exactly —
/// otherwise a save would write a file that differs from what the client
/// wrote in ways the user never asked for. This is the M1a final review's
/// load-bearing recommendation: the corpus gate proves the codec, this
/// check proves *this* file.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Fidelity {
    Editable,
    ReadOnly { reason: String },
}

#[derive(Debug)]
pub enum LoadError {
    Io(String),
    /// The file is not a decodable blue-marshal stream. The app shows a hex
    /// view (spec §7: never writable) — offset points at the failure.
    Decode { offset: usize, message: String },
}

pub struct Document {
    pub path: PathBuf,
    pub value: Value,
    pub fidelity: Fidelity,
    /// Conflict-check reference: what the file looked like at load
    /// (mtime + length, per spec §5.3). The bytes themselves are not kept —
    /// backups are taken from disk at save time.
    pub(crate) loaded_mtime: Option<SystemTime>,
    pub(crate) loaded_len: u64,
}

impl Document {
    pub fn load(path: &Path) -> Result<Document, LoadError> {
        let bytes = fs::read(path).map_err(|e| LoadError::Io(e.to_string()))?;
        let meta = fs::metadata(path).map_err(|e| LoadError::Io(e.to_string()))?;
        let value = decode(&bytes).map_err(|e| LoadError::Decode {
            offset: e.offset,
            message: e.to_string(),
        })?;
        let fidelity = match encode(&value) {
            Ok(out) if out == bytes => Fidelity::Editable,
            Ok(out) => Fidelity::ReadOnly {
                reason: format!(
                    "re-encode differs from on-disk bytes ({} vs {} bytes) — \
                     editing disabled to avoid unintended changes",
                    out.len(),
                    bytes.len()
                ),
            },
            Err(e) => Fidelity::ReadOnly { reason: format!("re-encode failed: {e}") },
        };
        Ok(Document {
            path: path.to_path_buf(),
            value,
            fidelity,
            loaded_mtime: meta.modified().ok(),
            loaded_len: meta.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    fn temp_file(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "settings-model-doc-{}-{}",
            std::process::id(),
            name
        ));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("core_char_1.dat");
        fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn canonical_file_loads_editable() {
        let bytes = encode(&Value::Dict(vec![(
            Value::Bytes(b"k".to_vec()),
            Value::Int(5),
        )]))
        .unwrap();
        let doc = Document::load(&temp_file("editable", &bytes)).unwrap();
        assert_eq!(doc.fidelity, Fidelity::Editable);
        assert_eq!(doc.loaded_len, bytes.len() as u64);
    }

    #[test]
    fn noncanonical_file_loads_read_only() {
        // A valid stream the encoder would not produce: Int 1 written as
        // INT8 (canonical form is the ONE constant). Decodes fine,
        // re-encodes shorter -> ReadOnly.
        let bytes = [0x7E, 0, 0, 0, 0, 0x06, 0x01];
        let doc = Document::load(&temp_file("readonly", &bytes)).unwrap();
        match doc.fidelity {
            Fidelity::ReadOnly { ref reason } => {
                assert!(reason.contains("re-encode differs"), "reason: {reason}")
            }
            ref other => panic!("expected ReadOnly, got {other:?}"),
        }
    }

    #[test]
    fn undecodable_file_is_a_decode_error_with_offset() {
        let bytes = [0x7E, 0, 0, 0, 0, 0x3D]; // unknown opcode at offset 5
        match Document::load(&temp_file("bad", &bytes)) {
            Err(LoadError::Decode { offset, .. }) => assert_eq!(offset, 5),
            other => panic!("expected Decode error, got {other:?}"),
        }
    }
}
```

- [ ] **Step 2: Uncomment the `document` re-export in `lib.rs`, run the unit tests**

Run: `cargo test -p settings-model document`
Expected: 3 tests PASS.

- [ ] **Step 3: Add the corpus fidelity gate**

`crates/settings-model/tests/corpus_load.rs`:

```rust
//! Every real corpus file must load Editable: the fidelity baseline is the
//! byte-identity gate applied through the Document API. A regression here
//! with the blue-marshal gates green means Document::load itself broke.

use std::fs;
use std::path::{Path, PathBuf};

use settings_model::{Document, Fidelity};

fn collect_dat_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_dat_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "dat") {
            out.push(path);
        }
    }
}

#[test]
fn every_corpus_file_loads_editable() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/corpus");
    let mut files = Vec::new();
    collect_dat_files(&corpus, &mut files);
    if files.is_empty() {
        eprintln!("corpus empty at {corpus:?} — skipping (run tools/sync-corpus.ps1)");
        return;
    }
    let mut failures = Vec::new();
    for f in &files {
        match Document::load(f) {
            Ok(doc) => {
                if let Fidelity::ReadOnly { reason } = doc.fidelity {
                    failures.push(format!("{}: ReadOnly: {reason}", f.display()));
                }
            }
            Err(e) => failures.push(format!("{}: {e:?}", f.display())),
        }
    }
    assert!(
        failures.is_empty(),
        "{}/{} corpus files did not load Editable:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
}
```

- [ ] **Step 4: Run the gate, full suite, commit**

Run: `cargo test -p settings-model --test corpus_load`
Expected: PASS (5022 files, a couple of minutes in debug — it decodes AND re-encodes every file).

Run: `cargo test`
Expected: everything green.

```powershell
git add -A
git commit -m "Load documents with a byte-fidelity baseline gating editability"
```

---

### Task 4: JSON tree projection

**Files:**
- Create: `crates/settings-model/src/projection.rs` (replace placeholder)
- Modify: `crates/settings-model/src/lib.rs` (uncomment re-export)

**Interfaces:**
- Consumes: `Value`, `NodePath`/`Step`.
- Produces (exact — the UI renders this and echoes `path` back in mutations):

```rust
pub struct Node {
    pub label: Option<String>,  // dict-key display / "[i]" / "class" / ...
    pub kind: &'static str,     // "none" "bool" "int" "long" "float" "bytes" "str"
                                // "str_ucs2" "str_table" "tuple" "list" "dict"
                                // "stream" "global" "instance" "reduce" "shared" "ref"
    pub display: String,        // scalar rendering, or "dict (34)" style summary
    pub path: NodePath,
    pub editable: bool,         // SetScalar works on this node
    pub edit_text: Option<String>, // raw text to seed an inline edit; feeding it
                                //   back through SetScalar unchanged is a no-op
    pub removable: bool,        // RemoveEntry works on this node
    pub in_shared: bool,        // inside a Shared subtree: edits alias into every Ref
    pub children: Vec<Node>,
}
pub fn project(root: &Value) -> Node
```

- [ ] **Step 1: Write `projection.rs`**

```rust
//! One-shot projection of a `Value` tree into a JSON-serializable node tree
//! for the raw editor. Rendering conventions match `blue_marshal::dump_text`
//! where both exist, so dumps and the UI read the same way.

use blue_marshal::{string_table::STRING_TABLE, Value};
use serde::Serialize;

use crate::mutate::subtree_contains_shared;
use crate::path::{NodePath, Step};

#[derive(Debug, Serialize)]
pub struct Node {
    pub label: Option<String>,
    pub kind: &'static str,
    pub display: String,
    pub path: NodePath,
    pub editable: bool,
    pub edit_text: Option<String>,
    pub removable: bool,
    pub in_shared: bool,
    pub children: Vec<Node>,
}

pub fn project(root: &Value) -> Node {
    build(root, None, Vec::new(), false, false)
}

fn build(
    v: &Value,
    label: Option<String>,
    path: NodePath,
    removable: bool,
    in_shared: bool,
) -> Node {
    let kind = kind_name(v);
    let editable = match v {
        // Non-finite floats have no text form that set_scalar can round-trip
        // without rewriting the payload's NaN bits — shown read-only.
        Value::Float(f) => f.is_finite(),
        Value::Bool(_)
        | Value::Int(_)
        | Value::Long(_)
        | Value::Bytes(_)
        | Value::Str(_)
        | Value::StrUcs2(_)
        | Value::StrTable(_) => true,
        _ => false,
    };
    let mut children = Vec::new();
    let child = |v: &Value, label: Option<String>, step: Step, removable: bool| {
        let mut p = path.clone();
        p.push(step);
        build(v, label, p, removable, in_shared)
    };
    match v {
        Value::Tuple(items) => {
            for (i, item) in items.iter().enumerate() {
                children.push(child(item, Some(format!("[{i}]")), Step::Tuple(i), false));
            }
        }
        Value::List(items) => {
            for (i, item) in items.iter().enumerate() {
                let removable = !subtree_contains_shared(item);
                children.push(child(item, Some(format!("[{i}]")), Step::List(i), removable));
            }
        }
        Value::Dict(entries) => {
            for (i, (key, value)) in entries.iter().enumerate() {
                let removable =
                    !subtree_contains_shared(key) && !subtree_contains_shared(value);
                children.push(child(
                    value,
                    Some(compact_display(key, 2)),
                    Step::DictValue(i),
                    removable,
                ));
            }
        }
        Value::Instance { class, state } => {
            children.push(child(class, Some("class".into()), Step::InstanceClass, false));
            children.push(child(state, Some("state".into()), Step::InstanceState, false));
        }
        Value::Reduce { ctor, items, pairs } => {
            children.push(child(ctor, Some("ctor".into()), Step::ReduceCtor, false));
            for (i, item) in items.iter().enumerate() {
                children.push(child(item, Some(format!("item[{i}]")), Step::ReduceItem(i), false));
            }
            for (i, (k, val)) in pairs.iter().enumerate() {
                children.push(child(k, Some(format!("pair[{i}].key")), Step::ReducePairKey(i), false));
                children.push(child(val, Some(format!("pair[{i}].value")), Step::ReducePairValue(i), false));
            }
        }
        Value::Shared { value, .. } => {
            let mut p = path.clone();
            p.push(Step::SharedInner);
            children.push(build(value, None, p, false, true));
        }
        Value::Stream(inner) => {
            let mut p = path.clone();
            p.push(Step::StreamInner);
            children.push(build(inner, None, p, false, in_shared));
        }
        _ => {}
    }
    Node {
        label,
        kind,
        display: node_display(v),
        path,
        editable,
        edit_text: edit_text(v),
        removable,
        in_shared,
        children,
    }
}

/// Raw text seeding an inline edit — chosen so that echoing it back through
/// `mutate::set_scalar` unchanged reproduces the same value. `None` for
/// non-editable kinds.
fn edit_text(v: &Value) -> Option<String> {
    Some(match v {
        Value::Bool(true) => "true".into(),
        Value::Bool(false) => "false".into(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) if f.is_finite() => format!("{f:?}"),
        Value::Str(s) | Value::StrUcs2(s) => s.clone(),
        Value::Bytes(b) => {
            // Printable bytes edit as plain text — EXCEPT text that itself
            // starts with "hex:", which must round-trip through the hex form
            // or set_scalar would reinterpret it.
            if !b.is_empty()
                && b.iter().all(|c| (0x20..0x7F).contains(c))
                && !b.starts_with(b"hex:")
            {
                String::from_utf8_lossy(b).into_owned()
            } else {
                format!("hex:{}", hex(b))
            }
        }
        Value::Long(b) => format!("hex:{}", hex(b)),
        Value::StrTable(i) => i.to_string(),
        _ => return None,
    })
}

fn kind_name(v: &Value) -> &'static str {
    match v {
        Value::None => "none",
        Value::Bool(_) => "bool",
        Value::Int(_) => "int",
        Value::Long(_) => "long",
        Value::Float(_) => "float",
        Value::Bytes(_) => "bytes",
        Value::Str(_) => "str",
        Value::StrUcs2(_) => "str_ucs2",
        Value::StrTable(_) => "str_table",
        Value::Tuple(_) => "tuple",
        Value::List(_) => "list",
        Value::Dict(_) => "dict",
        Value::Stream(_) => "stream",
        Value::Global(_) => "global",
        Value::Instance { .. } => "instance",
        Value::Reduce { .. } => "reduce",
        Value::Shared { .. } => "shared",
        Value::Ref(_) => "ref",
    }
}

/// Scalar rendering (and container summaries) for a node's own line.
fn node_display(v: &Value) -> String {
    match v {
        Value::None => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Int(i) => i.to_string(),
        Value::Long(bytes) => format!("hex:{}", hex(bytes)),
        Value::Float(f) => format!("{f:?}"),
        Value::Bytes(b) => bytes_display(b),
        Value::Str(s) => format!("{s:?}"),
        Value::StrUcs2(s) => format!("u{s:?}"),
        Value::StrTable(i) => format!("t{i}:{:?}", STRING_TABLE[*i as usize]),
        Value::Global(name) => format!("global:{}", bytes_display(name)),
        Value::Ref(n) => format!("ref[{n}]"),
        Value::Tuple(items) => format!("tuple ({})", items.len()),
        Value::List(items) => format!("list ({})", items.len()),
        Value::Dict(entries) => format!("dict ({})", entries.len()),
        Value::Stream(_) => "stream".into(),
        Value::Instance { .. } => "instance".into(),
        Value::Reduce { .. } => "reduce".into(),
        Value::Shared { slot, .. } => format!("shared[{slot}]"),
    }
}

/// One-line rendering for dict-key labels; containers render inline to
/// `depth` levels (tuple keys like ("overviewScroll2", 1) are real keys).
fn compact_display(v: &Value, depth: usize) -> String {
    match v {
        Value::Tuple(items) | Value::List(items) if depth > 0 => {
            let inner: Vec<String> =
                items.iter().map(|i| compact_display(i, depth - 1)).collect();
            let (open, close) = if matches!(v, Value::Tuple(_)) { ("(", ")") } else { ("[", "]") };
            format!("{open}{}{close}", inner.join(", "))
        }
        other => node_display(other),
    }
}

fn bytes_display(b: &[u8]) -> String {
    if b.iter().all(|c| (0x20..0x7F).contains(c)) {
        let mut out = String::from("b\"");
        for &c in b {
            if c == b'"' || c == b'\\' {
                out.push('\\');
            }
            out.push(c as char);
        }
        out.push('"');
        out
    } else {
        format!("hex:{}", hex(b))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_dict_with_labels_paths_and_flags() {
        let v = Value::Dict(vec![
            (Value::Bytes(b"geom".to_vec()), Value::Tuple(vec![Value::Int(5)])),
            (
                Value::Tuple(vec![Value::Bytes(b"overviewScroll2".to_vec()), Value::Int(1)]),
                Value::Int(7),
            ),
        ]);
        let n = project(&v);
        assert_eq!(n.kind, "dict");
        assert_eq!(n.display, "dict (2)");
        assert_eq!(n.children.len(), 2);
        assert_eq!(n.children[0].label.as_deref(), Some("b\"geom\""));
        assert_eq!(n.children[0].path, vec![Step::DictValue(0)]);
        assert!(n.children[0].removable);
        assert_eq!(
            n.children[1].label.as_deref(),
            Some("(b\"overviewScroll2\", 1)")
        );
        assert!(n.children[1].editable, "int value is editable");
        // tuple child of first entry
        let t = &n.children[0];
        assert_eq!(t.children[0].path, vec![Step::DictValue(0), Step::Tuple(0)]);
        assert!(!t.children[0].removable, "tuple elements are not removable");
    }

    #[test]
    fn shared_subtree_is_flagged_and_not_removable() {
        let v = Value::Dict(vec![(
            Value::Bytes(b"k".to_vec()),
            Value::Shared { slot: 1, value: Box::new(Value::List(vec![Value::Int(1)])) },
        )]);
        let n = project(&v);
        let entry = &n.children[0];
        assert_eq!(entry.kind, "shared");
        assert!(!entry.removable, "entries containing Shared cannot be removed");
        let inner = &entry.children[0];
        assert!(inner.in_shared);
        assert_eq!(inner.path, vec![Step::DictValue(0), Step::SharedInner]);
        assert!(inner.children[0].in_shared, "flag propagates down");
    }

    #[test]
    fn node_serializes_to_json() {
        let v = Value::List(vec![Value::Str("hi".into())]);
        let json = serde_json::to_value(project(&v)).unwrap();
        assert_eq!(json["kind"], "list");
        assert_eq!(json["children"][0]["display"], "\"hi\"");
        assert_eq!(json["children"][0]["path"][0]["s"], "list");
    }

    // NOTE: the edit_text ↔ SetScalar round-trip contract is tested in
    // mutate.rs (Task 5), which owns the other half of that contract.
}
```

**Note:** this file uses `crate::mutate::subtree_contains_shared`, which Task 5 writes. To keep Task 4 independently buildable, add to the still-placeholder `mutate.rs` ONLY this function now (Task 5 keeps it):

```rust
use blue_marshal::Value;

/// True if any node in the subtree is a `Shared` store. Removing such a
/// subtree would orphan its slot (encode fails SlotOutOfRange) or dangle
/// Refs elsewhere — so removal is blocked at the mutation layer.
pub fn subtree_contains_shared(v: &Value) -> bool {
    match v {
        Value::Shared { .. } => true,
        Value::Tuple(items) | Value::List(items) => items.iter().any(subtree_contains_shared),
        Value::Dict(entries) => entries
            .iter()
            .any(|(k, val)| subtree_contains_shared(k) || subtree_contains_shared(val)),
        Value::Stream(inner) => subtree_contains_shared(inner),
        Value::Instance { class, state } => {
            subtree_contains_shared(class) || subtree_contains_shared(state)
        }
        Value::Reduce { ctor, items, pairs } => {
            subtree_contains_shared(ctor)
                || items.iter().any(subtree_contains_shared)
                || pairs
                    .iter()
                    .any(|(k, v)| subtree_contains_shared(k) || subtree_contains_shared(v))
        }
        _ => false,
    }
}
```

Also note the `blue_marshal::string_table` import: that module is already `pub` in blue-marshal — no blue-marshal change needed.

- [ ] **Step 2: Run tests and commit**

Run: `cargo test -p settings-model projection`
Expected: 3 tests PASS.

```powershell
git add -A
git commit -m "Project documents as a JSON node tree for the raw editor"
```

---

### Task 5: Mutations

**Files:**
- Create: `crates/settings-model/src/mutate.rs` (extend the file that already holds `subtree_contains_shared`)
- Modify: `crates/settings-model/src/lib.rs` (uncomment re-export)

**Interfaces:**
- Consumes: `NodePath`/`Step`, `resolve_mut`, `Value`.
- Produces (exact — the UI sends `Mutation` JSON; the serde shape is a contract):

```rust
pub enum Mutation {
    SetScalar { path: NodePath, text: String },
    RemoveEntry { path: NodePath },
    InsertDictEntry { parent: NodePath, key: NewValue, value: NewValue },
    InsertListItem { parent: NodePath, index: usize, value: NewValue },
}
pub enum NewValue { None, Bool(bool), Int(String), Float(String), Str(String),
                    StrUcs2(String), BytesHex(String), EmptyDict, EmptyList }
pub fn apply(root: &mut Value, m: &Mutation) -> Result<(), MutateError>
```

- [ ] **Step 1: Write the rest of `mutate.rs`**

Append below `subtree_contains_shared` (keep its existing `use blue_marshal::Value;`):

```rust
use serde::Deserialize;

use crate::path::{resolve_mut, NodePath, Step};

/// The raw editor's mutation set. Deliberately small for V1:
/// - scalar edits keep the node's wire kind (no kind changes);
/// - removal is dict entries and list items only (tuples are fixed wire
///   shapes) and refuses subtrees containing `Shared` stores;
/// - inserts go into dicts (appended, wire order) and lists.
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Mutation {
    SetScalar { path: NodePath, text: String },
    RemoveEntry { path: NodePath },
    InsertDictEntry { parent: NodePath, key: NewValue, value: NewValue },
    InsertListItem { parent: NodePath, index: usize, value: NewValue },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum NewValue {
    None,
    Bool(bool),
    Int(String),
    Float(String),
    Str(String),
    StrUcs2(String),
    /// Raw bytes as hex digits (e.g. "6f76657276696577" = b"overview").
    BytesHex(String),
    EmptyDict,
    EmptyList,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum MutateError {
    BadPath,
    NotScalar(&'static str),
    Parse(String),
    /// Removal refused: the subtree contains a `Shared` store whose slot
    /// the encoder needs (and Refs elsewhere may point at).
    SharedSubtree,
    NotRemovable,
    NotAContainer(&'static str),
    BadIndex(usize),
}

pub fn apply(root: &mut Value, m: &Mutation) -> Result<(), MutateError> {
    match m {
        Mutation::SetScalar { path, text } => {
            let node = resolve_mut(root, path).ok_or(MutateError::BadPath)?;
            set_scalar(node, text)
        }
        Mutation::RemoveEntry { path } => remove_entry(root, path),
        Mutation::InsertDictEntry { parent, key, value } => {
            let key = build_value(key)?;
            let value = build_value(value)?;
            match resolve_mut(root, parent).ok_or(MutateError::BadPath)? {
                Value::Dict(entries) => {
                    entries.push((key, value));
                    Ok(())
                }
                other => Err(MutateError::NotAContainer(crate::projection_kind(other))),
            }
        }
        Mutation::InsertListItem { parent, index, value } => {
            let value = build_value(value)?;
            match resolve_mut(root, parent).ok_or(MutateError::BadPath)? {
                Value::List(items) => {
                    if *index > items.len() {
                        return Err(MutateError::BadIndex(*index));
                    }
                    items.insert(*index, value);
                    Ok(())
                }
                other => Err(MutateError::NotAContainer(crate::projection_kind(other))),
            }
        }
    }
}

/// Edit a scalar in place, keeping its wire kind. Parse rules per kind:
/// int: decimal i64 · float: finite f64 · str/str_ucs2: raw text ·
/// bytes/long: "hex:"-prefixed hex OR (bytes only) plain text taken as its
/// UTF-8 bytes · str_table: table index 1..=255 · bool: "true"/"false".
fn set_scalar(node: &mut Value, text: &str) -> Result<(), MutateError> {
    let parse_err = |what: &str| MutateError::Parse(format!("{what}: {text:?}"));
    match node {
        Value::Bool(b) => {
            *b = match text {
                "true" | "True" => true,
                "false" | "False" => false,
                _ => return Err(parse_err("expected true/false")),
            };
        }
        Value::Int(i) => *i = text.trim().parse::<i64>().map_err(|e| parse_err(&e.to_string()))?,
        Value::Float(f) => {
            let v = text.trim().parse::<f64>().map_err(|e| parse_err(&e.to_string()))?;
            if !v.is_finite() {
                return Err(parse_err("must be finite"));
            }
            *f = v;
        }
        Value::Str(s) => *s = text.to_string(),
        Value::StrUcs2(s) => *s = text.to_string(),
        Value::Bytes(b) => {
            *b = match text.strip_prefix("hex:") {
                Some(h) => parse_hex(h).ok_or_else(|| parse_err("bad hex"))?,
                None => text.as_bytes().to_vec(),
            };
        }
        Value::Long(b) => {
            let h = text.strip_prefix("hex:").unwrap_or(text);
            *b = parse_hex(h).ok_or_else(|| parse_err("long edits take hex bytes"))?;
        }
        Value::StrTable(idx) => {
            let v: u8 = text.trim().parse().map_err(|_| parse_err("table index 1-255"))?;
            if v == 0 {
                return Err(parse_err("table index 1-255"));
            }
            *idx = v;
        }
        other => return Err(MutateError::NotScalar(crate::projection_kind(other))),
    }
    Ok(())
}

fn remove_entry(root: &mut Value, path: &NodePath) -> Result<(), MutateError> {
    let Some((last, parent_path)) = path.split_last() else {
        return Err(MutateError::NotRemovable); // the root itself
    };
    // Guard BEFORE mutating: the node being removed (for dict entries: key
    // AND value) must not contain a Shared store.
    match last {
        Step::DictValue(i) | Step::DictKey(i) => {
            let parent = resolve_mut(root, parent_path).ok_or(MutateError::BadPath)?;
            let Value::Dict(entries) = parent else { return Err(MutateError::BadPath) };
            let (k, v) = entries.get(*i).ok_or(MutateError::BadPath)?;
            if subtree_contains_shared(k) || subtree_contains_shared(v) {
                return Err(MutateError::SharedSubtree);
            }
            entries.remove(*i);
            Ok(())
        }
        Step::List(i) => {
            let parent = resolve_mut(root, parent_path).ok_or(MutateError::BadPath)?;
            let Value::List(items) = parent else { return Err(MutateError::BadPath) };
            let item = items.get(*i).ok_or(MutateError::BadPath)?;
            if subtree_contains_shared(item) {
                return Err(MutateError::SharedSubtree);
            }
            items.remove(*i);
            Ok(())
        }
        _ => Err(MutateError::NotRemovable),
    }
}

fn build_value(nv: &NewValue) -> Result<Value, MutateError> {
    let parse_err = |what: &str, t: &str| MutateError::Parse(format!("{what}: {t:?}"));
    Ok(match nv {
        NewValue::None => Value::None,
        NewValue::Bool(b) => Value::Bool(*b),
        NewValue::Int(t) => {
            Value::Int(t.trim().parse::<i64>().map_err(|e| parse_err(&e.to_string(), t))?)
        }
        NewValue::Float(t) => {
            let v = t.trim().parse::<f64>().map_err(|e| parse_err(&e.to_string(), t))?;
            if !v.is_finite() {
                return Err(parse_err("must be finite", t));
            }
            Value::Float(v)
        }
        NewValue::Str(t) => Value::Str(t.clone()),
        NewValue::StrUcs2(t) => Value::StrUcs2(t.clone()),
        NewValue::BytesHex(h) => Value::Bytes(parse_hex(h).ok_or_else(|| parse_err("bad hex", h))?),
        NewValue::EmptyDict => Value::Dict(vec![]),
        NewValue::EmptyList => Value::List(vec![]),
    })
}

fn parse_hex(s: &str) -> Option<Vec<u8>> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if !s.is_ascii() || s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}
```

And in `lib.rs`, add the tiny shared helper both `mutate.rs` and `projection.rs` need (a kind name for arbitrary values in error messages) — add at the bottom of `lib.rs`:

```rust
/// Kind name for error messages; mirrors projection::Node.kind.
pub(crate) fn projection_kind(v: &blue_marshal::Value) -> &'static str {
    use blue_marshal::Value;
    match v {
        Value::None => "none",
        Value::Bool(_) => "bool",
        Value::Int(_) => "int",
        Value::Long(_) => "long",
        Value::Float(_) => "float",
        Value::Bytes(_) => "bytes",
        Value::Str(_) => "str",
        Value::StrUcs2(_) => "str_ucs2",
        Value::StrTable(_) => "str_table",
        Value::Tuple(_) => "tuple",
        Value::List(_) => "list",
        Value::Dict(_) => "dict",
        Value::Stream(_) => "stream",
        Value::Global(_) => "global",
        Value::Instance { .. } => "instance",
        Value::Reduce { .. } => "reduce",
        Value::Shared { .. } => "shared",
        Value::Ref(_) => "ref",
    }
}
```

and in `projection.rs`, replace its private `fn kind_name` with calls to `crate::projection_kind` (delete the local function) so the two never drift.

- [ ] **Step 2: Add mutation tests to `mutate.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::Step;

    fn doc() -> Value {
        // { b"lists": [ "a", "b" ], b"geom": (1, 2), b"shared": shared[1]:[9] }
        Value::Dict(vec![
            (
                Value::Bytes(b"lists".to_vec()),
                Value::List(vec![Value::Str("a".into()), Value::Str("b".into())]),
            ),
            (
                Value::Bytes(b"geom".to_vec()),
                Value::Tuple(vec![Value::Int(1), Value::Int(2)]),
            ),
            (
                Value::Bytes(b"shared".to_vec()),
                Value::Shared { slot: 1, value: Box::new(Value::List(vec![Value::Int(9)])) },
            ),
        ])
    }

    #[test]
    fn set_scalar_per_kind() {
        let mut v = doc();
        // int inside the tuple
        apply(&mut v, &Mutation::SetScalar {
            path: vec![Step::DictValue(1), Step::Tuple(0)],
            text: "424".into(),
        }).unwrap();
        // str inside the list
        apply(&mut v, &Mutation::SetScalar {
            path: vec![Step::DictValue(0), Step::List(1)],
            text: "edited".into(),
        }).unwrap();
        // int inside the SHARED list — allowed (edits alias by design)
        apply(&mut v, &Mutation::SetScalar {
            path: vec![Step::DictValue(2), Step::SharedInner, Step::List(0)],
            text: "7".into(),
        }).unwrap();
        let Value::Dict(entries) = &v else { unreachable!() };
        assert_eq!(entries[1].1, Value::Tuple(vec![Value::Int(424), Value::Int(2)]));
        assert_eq!(
            entries[0].1,
            Value::List(vec![Value::Str("a".into()), Value::Str("edited".into())])
        );
    }

    #[test]
    fn set_scalar_rejects_bad_input_without_mutating() {
        let mut v = doc();
        let before = v.clone();
        let err = apply(&mut v, &Mutation::SetScalar {
            path: vec![Step::DictValue(1), Step::Tuple(0)],
            text: "not-a-number".into(),
        }).unwrap_err();
        assert!(matches!(err, MutateError::Parse(_)));
        assert_eq!(v, before);
        // wrong kind: the dict itself is not a scalar
        let err = apply(&mut v, &Mutation::SetScalar { path: vec![], text: "5".into() })
            .unwrap_err();
        assert_eq!(err, MutateError::NotScalar("dict"));
    }

    #[test]
    fn remove_list_item_and_dict_entry() {
        let mut v = doc();
        apply(&mut v, &Mutation::RemoveEntry {
            path: vec![Step::DictValue(0), Step::List(0)],
        }).unwrap();
        apply(&mut v, &Mutation::RemoveEntry { path: vec![Step::DictValue(1)] }).unwrap();
        let Value::Dict(entries) = &v else { unreachable!() };
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].1, Value::List(vec![Value::Str("b".into())]));
        assert_eq!(entries[1].0, Value::Bytes(b"shared".to_vec()));
    }

    #[test]
    fn remove_refuses_shared_subtrees_and_tuple_elements() {
        let mut v = doc();
        assert_eq!(
            apply(&mut v, &Mutation::RemoveEntry { path: vec![Step::DictValue(2)] }),
            Err(MutateError::SharedSubtree)
        );
        assert_eq!(
            apply(&mut v, &Mutation::RemoveEntry {
                path: vec![Step::DictValue(1), Step::Tuple(0)],
            }),
            Err(MutateError::NotRemovable)
        );
    }

    #[test]
    fn inserts_into_dict_and_list() {
        let mut v = doc();
        apply(&mut v, &Mutation::InsertDictEntry {
            parent: vec![],
            key: NewValue::BytesHex("6b32".into()), // b"k2"
            value: NewValue::EmptyList,
        }).unwrap();
        apply(&mut v, &Mutation::InsertListItem {
            parent: vec![Step::DictValue(0)],
            index: 1,
            value: NewValue::Str("mid".into()),
        }).unwrap();
        let Value::Dict(entries) = &v else { unreachable!() };
        assert_eq!(entries[3].0, Value::Bytes(b"k2".to_vec()));
        assert_eq!(entries[3].1, Value::List(vec![]));
        assert_eq!(
            entries[0].1,
            Value::List(vec![
                Value::Str("a".into()),
                Value::Str("mid".into()),
                Value::Str("b".into()),
            ])
        );
        // bad index
        assert_eq!(
            apply(&mut v, &Mutation::InsertListItem {
                parent: vec![Step::DictValue(0)],
                index: 99,
                value: NewValue::None,
            }),
            Err(MutateError::BadIndex(99))
        );
    }

    #[test]
    fn edit_text_round_trips_through_set_scalar() {
        // For every editable kind: applying SetScalar with the node's own
        // projection edit_text must be a no-op — the inline-edit seed
        // contract shared between projection.rs and this module.
        let scalars = vec![
            Value::Bool(false),
            Value::Int(-42),
            Value::Float(2.5),
            Value::Str("plain".into()),
            Value::StrUcs2("u".into()),
            Value::Bytes(b"overview".to_vec()),
            Value::Bytes(vec![0x00, 0xFF]),
            Value::Bytes(b"hex:trap".to_vec()), // printable but ambiguous
            Value::Long(vec![0x2A, 0x00]),
            Value::StrTable(7),
        ];
        for s in scalars {
            let mut v = Value::List(vec![s.clone()]);
            let n = crate::projection::project(&v);
            let text = n.children[0].edit_text.clone().expect("editable");
            apply(&mut v, &Mutation::SetScalar { path: vec![Step::List(0)], text }).unwrap();
            assert_eq!(v, Value::List(vec![s]), "edit_text must be a no-op seed");
        }
    }

    #[test]
    fn mutation_json_shape_is_stable() {
        // The UI sends exactly this JSON — the serde shape is a contract.
        let m: Mutation = serde_json::from_str(
            r#"{"op":"set_scalar","path":[{"s":"dict_value","i":0}],"text":"5"}"#,
        ).unwrap();
        assert!(matches!(m, Mutation::SetScalar { .. }));
        let m: Mutation = serde_json::from_str(
            r#"{"op":"insert_dict_entry","parent":[],
                "key":{"kind":"str","v":"name"},"value":{"kind":"empty_dict"}}"#,
        ).unwrap();
        assert!(matches!(m, Mutation::InsertDictEntry { .. }));
    }
}
```

- [ ] **Step 3: Run tests, full crate suite, commit**

Run: `cargo test -p settings-model`
Expected: all settings-model tests PASS (path + document + projection + mutate).

```powershell
git add -A
git commit -m "Apply raw-tree mutations with shared-subtree guards"
```

---

### Task 6: The save chain

**Files:**
- Create: `crates/settings-model/src/save.rs` (replace placeholder), `crates/settings-model/tests/save_chain.rs`
- Modify: `crates/settings-model/src/lib.rs` (uncomment re-export)

**Interfaces:**
- Consumes: `Document` (its `pub(crate)` fields), `Value::bits_eq`, `blue_marshal::{decode, encode}`.
- Produces (exact — the app calls these; `SaveError` serializes with a `code` tag the UI switches on):

```rust
pub fn save(doc: &mut Document, force_conflict: bool) -> Result<SaveReport, SaveError>
pub struct SaveReport { backup_path, bytes_written, recent_sibling_writes }
pub enum SaveError { ReadOnly, Encode, VerifyMismatch, MissingOriginal, Conflict, Backup, Write }
pub(crate) fn utc_stamp() -> String            // "2026-07-13T145959Z"
pub(crate) fn backup_current(target: &Path) -> Result<PathBuf, String>  // reused by restore
```

- [ ] **Step 1: Write `save.rs`**

```rust
//! The spec §5 save-path invariant chain. Executed in order; ANY failure
//! aborts with the on-disk file untouched:
//!   1. encode   2. verify (decode own output, bit-level compare)
//!   3. conflict check (mtime+len vs load)   4. backup (no backup ⇒ no write)
//!   5. atomic write (temp file + rename; std's rename replaces atomically
//!      on Windows via MoveFileExW(MOVEFILE_REPLACE_EXISTING) and on POSIX
//!      via rename(2)).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use blue_marshal::{decode, encode};
use serde::Serialize;

use crate::document::{Document, Fidelity};

/// Sibling files modified within this window trigger the "client may be
/// running" standing warning (spec §5.3).
const RECENT_WRITE_WINDOW: Duration = Duration::from_secs(300);

#[derive(Debug, Serialize)]
pub struct SaveReport {
    pub backup_path: PathBuf,
    pub bytes_written: usize,
    /// File names in the same settings folder modified within the last
    /// 5 minutes (the client is likely running) — a warning, not an error.
    pub recent_sibling_writes: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum SaveError {
    /// Document loaded ReadOnly — saving is refused outright (spec §7).
    ReadOnly(String),
    Encode(String),
    /// Our own output did not decode back to the in-memory tree. Writer
    /// bug — nothing was written (spec §5.2).
    VerifyMismatch,
    /// The on-disk file vanished; without it there is nothing to back up.
    MissingOriginal(String),
    /// The file changed on disk since load (mtime or length). Retry with
    /// `force_conflict = true` after explicit user confirmation.
    Conflict,
    Backup(String),
    Write(String),
}

pub fn save(doc: &mut Document, force_conflict: bool) -> Result<SaveReport, SaveError> {
    if let Fidelity::ReadOnly { reason } = &doc.fidelity {
        return Err(SaveError::ReadOnly(reason.clone()));
    }
    // 1. Encode.
    let encoded = encode(&doc.value).map_err(|e| SaveError::Encode(e.to_string()))?;
    // 2. Verify: decode our own output and compare bit-exactly.
    match decode(&encoded) {
        Ok(back) if back.bits_eq(&doc.value) => {}
        _ => return Err(SaveError::VerifyMismatch),
    }
    // 3. Conflict check.
    let meta = fs::metadata(&doc.path).map_err(|e| SaveError::MissingOriginal(e.to_string()))?;
    let changed = meta.len() != doc.loaded_len
        || match (meta.modified().ok(), doc.loaded_mtime) {
            (Some(now), Some(then)) => now != then,
            _ => false,
        };
    if changed && !force_conflict {
        return Err(SaveError::Conflict);
    }
    let recent_sibling_writes = recent_writes(&doc.path);
    // 4. Backup — hard requirement.
    let backup_path = backup_current(&doc.path).map_err(SaveError::Backup)?;
    // 5. Atomic write.
    atomic_write(&doc.path, &encoded).map_err(SaveError::Write)?;
    // Refresh the conflict baseline so the next save diffs against what we
    // just wrote.
    let meta = fs::metadata(&doc.path).map_err(|e| SaveError::Write(e.to_string()))?;
    doc.loaded_mtime = meta.modified().ok();
    doc.loaded_len = meta.len();
    Ok(SaveReport { backup_path, bytes_written: encoded.len(), recent_sibling_writes })
}

/// Copy `target` into `<dir>/eve-settings-editor-backups/<name>.<stamp>.bak`
/// and verify the copy landed with the same length. Also used by restore.
pub(crate) fn backup_current(target: &Path) -> Result<PathBuf, String> {
    let dir = target
        .parent()
        .ok_or_else(|| "target has no parent directory".to_string())?
        .join("eve-settings-editor-backups");
    fs::create_dir_all(&dir).map_err(|e| format!("create backup dir: {e}"))?;
    let name = target
        .file_name()
        .ok_or_else(|| "target has no file name".to_string())?
        .to_string_lossy();
    let backup = dir.join(format!("{name}.{}.bak", utc_stamp()));
    fs::copy(target, &backup).map_err(|e| format!("copy to backup: {e}"))?;
    let (src, dst) = (
        fs::metadata(target).map_err(|e| e.to_string())?.len(),
        fs::metadata(&backup).map_err(|e| e.to_string())?.len(),
    );
    if src != dst {
        return Err(format!("backup size mismatch ({dst} of {src} bytes)"));
    }
    Ok(backup)
}

pub(crate) fn atomic_write(target: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = target.parent().ok_or_else(|| "no parent dir".to_string())?;
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    let temp = dir.join(format!(".{name}.tmp-{}", std::process::id()));
    fs::write(&temp, bytes).map_err(|e| format!("write temp: {e}"))?;
    fs::rename(&temp, target).map_err(|e| {
        let _ = fs::remove_file(&temp);
        format!("rename over target: {e}")
    })
}

fn recent_writes(target: &Path) -> Vec<String> {
    let Some(dir) = target.parent() else { return vec![] };
    let now = SystemTime::now();
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p == target || p.extension().is_none_or(|e| e != "dat") {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    if now.duration_since(modified).unwrap_or_default() < RECENT_WRITE_WINDOW {
                        out.push(entry.file_name().to_string_lossy().into_owned());
                    }
                }
            }
        }
    }
    out.sort();
    out
}

/// UTC timestamp `YYYY-MM-DDTHHMMSSZ` — ISO-8601 with basic (colon-free)
/// time, valid in Windows file names; matches tools/sync-corpus.ps1.
pub(crate) fn utc_stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (y, m, d) = civil_from_days((secs / 86400) as i64);
    let rem = secs % 86400;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}{:02}{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Days-since-1970 to (year, month, day). Howard Hinnant's `civil_from_days`
/// algorithm — exact for the whole i64 day range we care about.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_from_days_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(11_017), (2000, 3, 1));
        assert_eq!(civil_from_days(20_647), (2026, 7, 13));
    }

    #[test]
    fn utc_stamp_shape() {
        let s = utc_stamp();
        // e.g. 2026-07-13T145959Z
        assert_eq!(s.len(), 18);
        assert!(!s.contains(':'), "colons are invalid in Windows file names");
        assert!(s.ends_with('Z'));
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[10..11], "T");
    }
}
```

- [ ] **Step 2: Write the integration tests**

`crates/settings-model/tests/save_chain.rs`:

```rust
//! Full save-chain integration tests on temp directories. These verify the
//! spec §5 invariants: backup-before-write, abort-leaves-file-untouched,
//! conflict detection, and the ReadOnly refusal.

use std::fs;
use std::path::PathBuf;

use blue_marshal::{encode, Value};
use settings_model::{apply, save, Document, Mutation, SaveError, Step};

fn temp_settings_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "settings-model-save-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_canonical_file(dir: &PathBuf) -> (PathBuf, Vec<u8>) {
    let value = Value::Dict(vec![(
        Value::Bytes(b"suggestions".to_vec()),
        Value::List(vec![Value::Str("alpha".into()), Value::Str("beta".into())]),
    )]);
    let bytes = encode(&value).unwrap();
    let path = dir.join("core_user_42.dat");
    fs::write(&path, &bytes).unwrap();
    (path, bytes)
}

#[test]
fn save_backs_up_then_writes_atomically() {
    let dir = temp_settings_dir("happy");
    let (path, original) = write_canonical_file(&dir);
    let mut doc = Document::load(&path).unwrap();
    apply(&mut doc.value, &Mutation::SetScalar {
        path: vec![Step::DictValue(0), Step::List(0)],
        text: "edited".into(),
    })
    .unwrap();
    let report = save(&mut doc, false).unwrap();
    // Backup holds the ORIGINAL bytes.
    assert_eq!(fs::read(&report.backup_path).unwrap(), original);
    assert!(report.backup_path.parent().unwrap().ends_with("eve-settings-editor-backups"));
    // Target holds the new encode, which reloads Editable with the edit.
    let reloaded = Document::load(&path).unwrap();
    assert_eq!(reloaded.fidelity, settings_model::Fidelity::Editable);
    let json = serde_json::to_value(settings_model::project(&reloaded.value)).unwrap();
    assert_eq!(json["children"][0]["children"][0]["display"], "\"edited\"");
    // A second save after the first must not be a conflict (baseline refreshed).
    save(&mut doc, false).unwrap();
}

#[test]
fn conflict_detected_and_forcible() {
    let dir = temp_settings_dir("conflict");
    let (path, _) = write_canonical_file(&dir);
    let mut doc = Document::load(&path).unwrap();
    // Simulate the client rewriting the file after our load: different
    // length guarantees detection even on coarse-mtime filesystems.
    let other = encode(&Value::Dict(vec![])).unwrap();
    fs::write(&path, &other).unwrap();
    match save(&mut doc, false) {
        Err(SaveError::Conflict) => {}
        other => panic!("expected Conflict, got {other:?}"),
    }
    // Forced save proceeds — and the backup preserves the CURRENT on-disk
    // (conflicting) bytes, so nothing is ever lost.
    let report = save(&mut doc, true).unwrap();
    assert_eq!(fs::read(&report.backup_path).unwrap(), other);
}

#[test]
fn backup_failure_aborts_with_file_untouched() {
    let dir = temp_settings_dir("nobackup");
    let (path, original) = write_canonical_file(&dir);
    // Occupy the backup-dir NAME with a file, so create_dir_all fails.
    fs::write(dir.join("eve-settings-editor-backups"), b"not a dir").unwrap();
    let mut doc = Document::load(&path).unwrap();
    apply(&mut doc.value, &Mutation::SetScalar {
        path: vec![Step::DictValue(0), Step::List(0)],
        text: "edited".into(),
    })
    .unwrap();
    match save(&mut doc, false) {
        Err(SaveError::Backup(_)) => {}
        other => panic!("expected Backup error, got {other:?}"),
    }
    assert_eq!(fs::read(&path).unwrap(), original, "no backup => no write, ever");
}

#[test]
fn encode_failure_aborts_before_touching_disk() {
    let dir = temp_settings_dir("badtree");
    let (path, original) = write_canonical_file(&dir);
    let mut doc = Document::load(&path).unwrap();
    doc.value = Value::Tuple(vec![Value::Ref(1)]); // dangling ref: unencodable
    match save(&mut doc, false) {
        Err(SaveError::Encode(_)) => {}
        other => panic!("expected Encode error, got {other:?}"),
    }
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(!dir.join("eve-settings-editor-backups").exists(), "no backup taken either");
}

#[test]
fn read_only_document_refuses_to_save() {
    let dir = temp_settings_dir("readonly");
    // Non-canonical stream: Int 1 as INT8 -> loads ReadOnly.
    let path = dir.join("core_char_7.dat");
    fs::write(&path, [0x7E, 0, 0, 0, 0, 0x06, 0x01]).unwrap();
    let mut doc = Document::load(&path).unwrap();
    match save(&mut doc, false) {
        Err(SaveError::ReadOnly(_)) => {}
        other => panic!("expected ReadOnly, got {other:?}"),
    }
}
```

- [ ] **Step 3: Run the integration tests, full suite, commit**

Run: `cargo test -p settings-model --test save_chain`
Expected: 5 tests PASS.

Run: `cargo test`
Expected: everything green (blue-marshal gates included).

```powershell
git add -A
git commit -m "Implement the backup-verify-atomic save chain"
```

---

### Task 7: Backups listing and restore

**Files:**
- Create: `crates/settings-model/src/backups.rs` (replace placeholder)
- Modify: `crates/settings-model/src/lib.rs` (uncomment re-export; export `backups::{list_backups, restore, BackupInfo}`)

**Interfaces:**
- Consumes: `save::{backup_current, atomic_write}`.
- Produces (exact):

```rust
pub struct BackupInfo { pub path: PathBuf, pub file_name: String, pub size: u64 }
pub fn list_backups(target: &Path) -> Vec<BackupInfo>          // newest first
pub fn restore(backup: &Path, target: &Path) -> Result<PathBuf, String>  // returns pre-restore backup
```

- [ ] **Step 1: Write `backups.rs`**

```rust
//! Timestamped backups: enumeration and one-click restore. Restore itself
//! backs up the current file first (spec §5), so it is also reversible.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::save::{atomic_write, backup_current};

#[derive(Debug, Serialize)]
pub struct BackupInfo {
    pub path: PathBuf,
    pub file_name: String,
    pub size: u64,
}

/// Backups of `target`, newest first. The timestamp is lexically sortable
/// (`YYYY-MM-DDTHHMMSSZ`), so sorting by file name descending is by time.
pub fn list_backups(target: &Path) -> Vec<BackupInfo> {
    let Some(dir) = target.parent() else { return vec![] };
    let dir = dir.join("eve-settings-editor-backups");
    let Some(name) = target.file_name() else { return vec![] };
    let prefix = format!("{}.", name.to_string_lossy());
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if file_name.starts_with(&prefix) && file_name.ends_with(".bak") {
                out.push(BackupInfo {
                    path: entry.path(),
                    size: entry.metadata().map(|m| m.len()).unwrap_or(0),
                    file_name,
                });
            }
        }
    }
    out.sort_by(|a, b| b.file_name.cmp(&a.file_name));
    out
}

/// Replace `target` with `backup`'s content: back up the current target
/// first, then write atomically. Returns the pre-restore backup's path.
/// The backup's content is validated as a decodable stream before anything
/// is touched — restoring a corrupt backup would defeat the whole chain.
pub fn restore(backup: &Path, target: &Path) -> Result<PathBuf, String> {
    let bytes = fs::read(backup).map_err(|e| format!("read backup: {e}"))?;
    blue_marshal::decode(&bytes).map_err(|e| format!("backup does not decode: {e}"))?;
    let pre = backup_current(target)?;
    atomic_write(target, &bytes)?;
    Ok(pre)
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::{encode, Value};

    fn setup(name: &str) -> (PathBuf, PathBuf, Vec<u8>, Vec<u8>) {
        let dir = std::env::temp_dir().join(format!(
            "settings-model-backups-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let v1 = encode(&Value::Int(100)).unwrap();
        let v2 = encode(&Value::Int(200)).unwrap();
        let target = dir.join("core_user_9.dat");
        fs::write(&target, &v2).unwrap();
        let bdir = dir.join("eve-settings-editor-backups");
        fs::create_dir_all(&bdir).unwrap();
        let backup = bdir.join("core_user_9.dat.2026-07-13T000000Z.bak");
        fs::write(&backup, &v1).unwrap();
        (target, backup, v1, v2)
    }

    #[test]
    fn lists_only_matching_backups_newest_first() {
        let (target, _b, _, _) = setup("list");
        let bdir = target.parent().unwrap().join("eve-settings-editor-backups");
        fs::write(bdir.join("core_user_9.dat.2026-07-14T000000Z.bak"), b"x").unwrap();
        fs::write(bdir.join("core_char_1.dat.2026-07-13T000000Z.bak"), b"y").unwrap(); // other file
        fs::write(bdir.join("notes.txt"), b"z").unwrap(); // not a backup
        let list = list_backups(&target);
        assert_eq!(list.len(), 2);
        assert!(list[0].file_name.contains("2026-07-14"), "newest first");
        assert!(list[1].file_name.contains("2026-07-13"));
    }

    #[test]
    fn restore_backs_up_current_then_replaces() {
        let (target, backup, v1, v2) = setup("restore");
        let pre = restore(&backup, &target).unwrap();
        assert_eq!(fs::read(&target).unwrap(), v1, "target now holds the backup content");
        assert_eq!(fs::read(&pre).unwrap(), v2, "pre-restore state preserved");
    }

    #[test]
    fn restore_refuses_undecodable_backup() {
        let (target, _b, _, v2) = setup("corrupt");
        let bad = target
            .parent()
            .unwrap()
            .join("eve-settings-editor-backups")
            .join("core_user_9.dat.2026-07-15T000000Z.bak");
        fs::write(&bad, b"garbage").unwrap();
        assert!(restore(&bad, &target).unwrap_err().contains("does not decode"));
        assert_eq!(fs::read(&target).unwrap(), v2, "target untouched");
    }
}
```

- [ ] **Step 2: Run tests and commit**

Run: `cargo test -p settings-model backups`
Expected: 3 tests PASS.

```powershell
git add -A
git commit -m "List and restore timestamped backups"
```

---

### Task 8: Profile discovery

**Files:**
- Create: `crates/settings-model/src/discover.rs` (replace placeholder)
- Modify: `crates/settings-model/src/lib.rs` (uncomment re-export; export `discover::{discover, default_roots, Profile, SettingsFile, FileKind}`)

**Interfaces:**
- Consumes: nothing from earlier tasks (std only).
- Produces (exact):

```rust
pub fn default_roots() -> Vec<PathBuf>     // OS-standard EVE locations that exist
pub fn discover(roots: &[Path]) -> Vec<Profile>
pub struct Profile { install, server, profile, dir, files }
pub struct SettingsFile { path, file_name, kind, id, size, modified_unix }
pub enum FileKind { Char, User, Other }
```

- [ ] **Step 1: Write `discover.rs`**

```rust
//! Discovery of EVE settings profiles in OS-standard locations. Layout
//! (verified against real snapshots; example ID synthetic):
//!   <root>/<install>_<server>/settings_<profile>/core_(char|user)_<id>.dat
//! e.g. c_eve_sharedcache_tq_tranquility/settings_Default/core_char_123456789.dat
//! The server name is the last `_`-separated token of the install dir.
//!
//! Library code takes caller-supplied roots so tests never touch the live
//! directory (spec §8); only the app passes `default_roots()` at runtime.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Profile {
    /// Install-directory name minus the server suffix, e.g. "c_eve_sharedcache_tq".
    pub install: String,
    /// Last underscore token of the install dir, e.g. "tranquility".
    pub server: String,
    /// The settings_<profile> suffix, e.g. "Default".
    pub profile: String,
    pub dir: PathBuf,
    pub files: Vec<SettingsFile>,
}

#[derive(Debug, Serialize)]
pub struct SettingsFile {
    pub path: PathBuf,
    pub file_name: String,
    pub kind: FileKind,
    /// Numeric id from core_char_<id>/core_user_<id>; None for anomalous
    /// names (real examples exist: `core_char__.dat`).
    pub id: Option<u64>,
    pub size: u64,
    pub modified_unix: Option<u64>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Char,
    User,
    Other,
}

/// OS-standard EVE settings roots that actually exist on this machine.
pub fn default_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if cfg!(target_os = "windows") {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            roots.push(PathBuf::from(local).join("CCP").join("EVE"));
        }
    } else if cfg!(target_os = "macos") {
        if let Ok(home) = std::env::var("HOME") {
            roots.push(
                PathBuf::from(home)
                    .join("Library/Application Support/CCP/EVE"),
            );
        }
    } else {
        if let Ok(home) = std::env::var("HOME") {
            // Steam Proton prefix (EVE app id 8500).
            roots.push(PathBuf::from(&home).join(
                ".steam/steam/steamapps/compatdata/8500/pfx/drive_c/users/steamuser/AppData/Local/CCP/EVE",
            ));
        }
        if let Ok(prefix) = std::env::var("WINEPREFIX") {
            roots.push(
                PathBuf::from(prefix).join("drive_c/users").join(
                    std::env::var("USER").unwrap_or_else(|_| "steamuser".into()),
                ).join("AppData/Local/CCP/EVE"),
            );
        }
    }
    roots.retain(|r| r.is_dir());
    roots
}

pub fn discover(roots: &[PathBuf]) -> Vec<Profile> {
    let mut profiles = Vec::new();
    for root in roots {
        let Ok(installs) = fs::read_dir(root) else { continue };
        for install in installs.flatten() {
            let install_path = install.path();
            if !install_path.is_dir() {
                continue;
            }
            let install_name = install.file_name().to_string_lossy().into_owned();
            let (install_prefix, server) = match install_name.rsplit_once('_') {
                Some((p, s)) if !s.is_empty() => (p.to_string(), s.to_string()),
                _ => (install_name.clone(), String::new()),
            };
            let Ok(settings_dirs) = fs::read_dir(&install_path) else { continue };
            for sdir in settings_dirs.flatten() {
                let sdir_path = sdir.path();
                let sdir_name = sdir.file_name().to_string_lossy().into_owned();
                let Some(profile_name) = sdir_name.strip_prefix("settings_") else {
                    continue;
                };
                if !sdir_path.is_dir() {
                    continue;
                }
                let files = collect_files(&sdir_path);
                if files.is_empty() {
                    continue;
                }
                profiles.push(Profile {
                    install: install_prefix.clone(),
                    server: server.clone(),
                    profile: profile_name.to_string(),
                    dir: sdir_path,
                    files,
                });
            }
        }
    }
    profiles.sort_by(|a, b| (&a.server, &a.profile).cmp(&(&b.server, &b.profile)));
    profiles
}

fn collect_files(dir: &Path) -> Vec<SettingsFile> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if !file_name.ends_with(".dat") || !path.is_file() {
            continue;
        }
        let stem = file_name.trim_end_matches(".dat");
        let (kind, id) = if let Some(rest) = stem.strip_prefix("core_char_") {
            (FileKind::Char, rest.parse::<u64>().ok())
        } else if let Some(rest) = stem.strip_prefix("core_user_") {
            (FileKind::User, rest.parse::<u64>().ok())
        } else {
            (FileKind::Other, None)
        };
        let meta = entry.metadata().ok();
        out.push(SettingsFile {
            path,
            file_name,
            kind,
            id,
            size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
            modified_unix: meta
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
        });
    }
    out.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_the_real_layout_from_a_temp_tree() {
        let root = std::env::temp_dir().join(format!(
            "settings-model-discover-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let sdir = root
            .join("c_eve_sharedcache_tq_tranquility")
            .join("settings_Default");
        fs::create_dir_all(&sdir).unwrap();
        // Synthetic IDs only — never commit real character/account IDs.
        fs::write(sdir.join("core_char_123456789.dat"), b"x").unwrap();
        fs::write(sdir.join("core_user_987654.dat"), b"x").unwrap();
        fs::write(sdir.join("core_char__.dat"), b"x").unwrap(); // real anomaly shape
        fs::write(sdir.join("prefs.ini"), b"x").unwrap(); // not a .dat
        let sisi = root
            .join("c_eve_sharedcache_sisi_singularity")
            .join("settings_Default");
        fs::create_dir_all(&sisi).unwrap();
        fs::write(sisi.join("core_user_1.dat"), b"x").unwrap();
        fs::create_dir_all(root.join("c_eve_sharedcache_tq_tranquility").join("cache"))
            .unwrap(); // non-settings dir ignored

        let profiles = discover(&[root.clone()]);
        assert_eq!(profiles.len(), 2);
        // sorted by (server, profile): singularity first
        assert_eq!(profiles[0].server, "singularity");
        let tq = &profiles[1];
        assert_eq!(tq.server, "tranquility");
        assert_eq!(tq.install, "c_eve_sharedcache_tq");
        assert_eq!(tq.profile, "Default");
        assert_eq!(tq.files.len(), 3);
        assert_eq!(tq.files[0].file_name, "core_char_123456789.dat");
        assert_eq!(tq.files[0].kind, FileKind::Char);
        assert_eq!(tq.files[0].id, Some(123456789));
        assert_eq!(tq.files[1].file_name, "core_char__.dat");
        assert_eq!(tq.files[1].kind, FileKind::Char);
        assert_eq!(tq.files[1].id, None, "anomalous names tolerated");
        assert_eq!(tq.files[2].kind, FileKind::User);
    }

    #[test]
    fn missing_roots_yield_empty_not_error() {
        let ghost = std::env::temp_dir().join("settings-model-no-such-root");
        assert!(discover(&[ghost]).is_empty());
    }
}
```

- [ ] **Step 2: Run tests, full suite, commit**

Run: `cargo test -p settings-model`
Expected: all settings-model tests PASS.

Run: `cargo test`
Expected: everything green.

```powershell
git add -A
git commit -m "Discover settings profiles in OS-standard locations"
```

---

### Task 9: Close out M1b-1

**Files:**
- Modify: `docs/format-notes.md`

**Interfaces:**
- Consumes: the duplicate-slot measurement from this plan's header; the corpus fidelity gate result (Task 3).
- Produces: format-notes as the standing reference for M1b-2.

- [ ] **Step 1: Add the status bullet**

Append at the end of the `## Status` bullet list in `docs/format-notes.md`:

```markdown
- **2026-07-13 — M1b-1 complete.** `settings-model` crate shipped: fidelity-
  gated `Document::load` (Editable only when `encode(decode(bytes))` is
  byte-identical — corpus gate `every_corpus_file_loads_editable`, 5022/5022),
  JSON tree projection, guarded mutations, the spec §5 save chain
  (verify → backup → atomic write, all abort paths integration-tested),
  backups/restore, and profile discovery. blue-marshal additions:
  `Value::bits_eq` (NaN-safe verify) and `DuplicateSharedSlot` promoted to a
  hard decode error (measured: 0 duplicates across 4,986 corpus files with
  shared maps), making `Ref(slot)` unambiguous for the mutation layer.
```

- [ ] **Step 2: Final verification and commit**

Run: `cargo test`
Expected: full suite green — blue-marshal (unit + 2 corpus gates + property) and settings-model (unit + corpus_load + save_chain).

```powershell
git add -A
git commit -m "Record M1b-1 completion in format notes"
```

---

## Completion

After Task 9: use superpowers:finishing-a-development-branch. M1b-1 is done when the whole suite is green with the corpus present and the branch review confirms:

1. `every_corpus_file_loads_editable`: 5022/5022 Editable.
2. All five save-chain abort/consistency integration tests pass.
3. `settings-model` depends only on `blue-marshal`, `serde`, `serde_json`; `blue-marshal` still has zero dependencies.
4. No personal data in committed files; no code path reads the live EVE directory (only `default_roots()` *names* it, and only the app calls that).

Then M1b-2 (Tauri app + UI + CI) executes against exactly the interfaces produced here.