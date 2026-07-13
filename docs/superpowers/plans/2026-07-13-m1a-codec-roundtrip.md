# M1a — Encoder & Byte-Identical Round-Trip Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give `blue-marshal` a native encoder whose output is byte-identical to the client's own bytes for every unmodified decoded file, gated by the full 5022-file corpus, plus the fidelity-tagged `Value` model that makes byte-identity possible.

**Architecture:** The existing `Value` enum is reworked into a fidelity-preserving model: shared-object identity becomes explicit `Shared { slot }` / `Ref(slot)` nodes, `INSTANCE`/`REDUCE` split into separate variants, and text splits into `Str` (UTF8) / `StrUcs2` (UNICODE family) / `StrTable` (STRINGR index) because those are the only wire degeneracies the client actually exercises (see measurements below). Everything else encodes by *canonical rules proven exact against the corpus*, so no other tags are needed. A new `encode` module mirrors `decode` opcode-for-opcode.

**Tech Stack:** Rust stable, edition 2021, `blue-marshal` stays dependency-free (std only).

**Scope note:** This is the first half of Milestone 1 (spec §9). M1 splits into two plans, each shipping working software: **M1a (this plan)** = codec round-trip; **M1b** (planned after M1a merges) = Tauri app shell, load/save chain with backups, raw tree editor, CI. The split exists because M1b's JSON projection and mutation layer consume the exact `Value` shape this plan finalizes.

## Global Constraints

- **Live-directory rule (spec §8):** nothing in this plan reads from or writes to `%LOCALAPPDATA%\CCP\EVE\…`. All tests run on `testdata/corpus/` copies only.
- `testdata/` is gitignored. Never commit real settings files. Committed files must not contain character or account **names** (numeric IDs are acceptable).
- Commit messages: sentence-case summary line, matching existing repo style. **No attribution trailers of any kind** (no `Co-Authored-By`, no "Generated with").
- The `blue-marshal` crate uses **no external dependencies** (including dev-dependencies).
- Lossless codec: `decode` must keep rejecting anything it cannot represent exactly; `encode` must never silently normalize — every canonical rule below is corpus-proven, and anything outside them is carried by an explicit tag.
- Corpus tests skip (with a stderr note) when `testdata/corpus/` is absent, exactly like the existing `every_corpus_file_decodes` gate — CI machines have no corpus.
- Run the full suite with `cargo test` from the repo root; corpus tests take a few minutes in debug — that is expected.

## Measured corpus facts (2026-07-13, do not re-derive)

Instrumented decoder run over **all 5022 corpus files** (historical + fresh-baseline + exp1–exp5 snapshots; 0 decode failures). These numbers justify every canonicalization decision below; if a future client patch breaks one, the corpus gate (Task 4) fails loudly.

| Measurement | Count | Consequence |
|---|---|---|
| `READ_LENGTH` 0xFF escapes total | 228,549 | — |
| … of which non-minimal (value < 255) | **0** | encoder always emits minimal length encoding, no tag |
| Slack bytes between root object and tail map | **0** streams | promote to hard `DecodeError` (`TrailingBytes`, Task 2) |
| Tail-map entries where slot ≠ encounter-index+1 | **963,660** | tail maps are NOT identity permutations → slot numbers must be preserved explicitly (`Shared { slot }`, `Ref(slot)`) |
| SHARED_FLAG on opcodes the reference ignores it for | **0** | decoder keeps ignoring; encoder never emits it there |
| `STREAM` (0x2B) opcodes | **0** | keep recursive support, corpus never exercises it |
| `STRING` (0x10) / `STRINGL` (0x0D) opcodes | **0** / **0** | `Bytes` encodes canonically: len 0 → STRING0, 1 → STRING1, else BUFFER |
| `BUFFER` with payload ≤ 1 byte | **0** | confirms the `Bytes` canonical-by-length rule is exact |
| Counted `TUPLE` with n ≤ 2 / counted `LIST` with n ≤ 1 | **0** / **0** | `Tuple`/`List` encode canonically by element count, no tag |
| `INT8/16/32/64` holding a value a narrower opcode could hold | **0** | `Int(i64)` encodes canonically by magnitude, no width tag |
| `LONG` with 0 payload bytes | **0** | (wire-legal; `Long(Vec<u8>)` already preserves payload exactly) |
| `FLOAT` whose payload is +0.0 bits | **0** | `Float` encodes canonically: +0.0 bits → FLOAT0, else FLOAT (−0.0 stays FLOAT) |
| `UTF8` (0x2E) opcodes | **11,030,207** | dominant text form → `Value::Str` = UTF8 |
| `UTF8` with 0 bytes | 14,355 | empty string exists as UTF8 **and** UNICODE0 → content cannot pick the opcode; split variants |
| `UNICODE` (0x12) opcodes | **0** | UCS-2 family appears only as UNICODE0/UNICODE1; `StrUcs2` encodes by UTF-16 unit count (0/1/n) |
| `UNICODE0` / `UNICODE1` opcodes | 1,458 / 15,480 | → `Value::StrUcs2` variant |
| `STRINGR` (0x11) opcodes | **71,847** | → `Value::StrTable(u8)` keeps the index |
| Non-STRINGR strings whose content equals a table entry | **1,269** | the client writes table-content strings as UTF8 too → "emit STRINGR when in table" would be WRONG; only the tag decides |

## File Structure

```
crates/blue-marshal/
├── src/
│   ├── lib.rs            # modify: crate doc, re-export encode/EncodeError
│   ├── error.rs          # modify: drop UnknownFlags, add TrailingBytes; add EncodeError
│   ├── reader.rs         # modify: comment fix + i64/f64 tests (Task 1)
│   ├── value.rs          # modify: fidelity model (Task 2), dump updates, escaping fix (Task 1)
│   ├── decode.rs         # modify: emit new variants, Shared/Ref, TrailingBytes (Task 2)
│   ├── encode.rs         # create: Encoder (Task 3)
│   ├── opcodes.rs        # unchanged
│   ├── string_table.rs   # unchanged
│   └── bin/bmdump.rs     # modify: empty-scan exit code (Task 1)
├── tests/
│   ├── corpus.rs         # modify: add byte-identity gate (Task 4)
│   └── prop_roundtrip.rs # create: std-only property test (Task 5)
LICENSE                   # create (Task 1)
docs/format-notes.md      # modify: measurements + model notes (Task 6)
```

---

### Task 1: Kickoff cleanup batch (carried M0 minors) + LICENSE

All the small findings M0's reviews deferred, batched into **one commit** per the recorded user decision, plus the MIT license spec §11 requires from M1.

**Files:**
- Modify: `crates/blue-marshal/src/error.rs`, `crates/blue-marshal/src/reader.rs`, `crates/blue-marshal/src/value.rs`, `crates/blue-marshal/src/decode.rs`, `crates/blue-marshal/src/bin/bmdump.rs`, `docs/format-notes.md`
- Create: `LICENSE`

**Interfaces:**
- Consumes: current `Value`/`decode` as on `master`.
- Produces: no API changes except `ErrorKind::UnknownFlags` removed. Tests added here are updated (not removed) by Task 2.

- [ ] **Step 1: Remove the dead `UnknownFlags` variant**

In `crates/blue-marshal/src/error.rs`, delete the line `UnknownFlags(u8),` from `ErrorKind`. Nothing constructs it (the 0x80-flag case surfaces as `UnknownOpcode`).

Run: `cargo build`
Expected: compiles with no "unused variant" or missing-variant errors.

- [ ] **Step 2: Fix the stale reader comment and add i64/f64 read tests**

In `crates/blue-marshal/src/reader.rs`, in the test `read_len_single_byte_and_extended`, change the comment
`// 0xFF escape -> u32 LE follows (observed in real files: 16 FF 76 02 00 00)` to
`// 0xFF escape -> i32 LE follows (signed per marshal.c:99-108, non-negative in practice; observed in real files: 16 FF 76 02 00 00)`.

Add to the same `tests` module:

```rust
#[test]
fn reads_i64_and_f64_little_endian() {
    let mut r = Reader::new(&[
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // i64 -1
        0, 0, 0, 0, 0, 0, 0x04, 0x40, // f64 2.5
    ]);
    assert_eq!(r.read_i64().unwrap(), -1);
    assert_eq!(r.read_f64().unwrap(), 2.5);
}
```

Run: `cargo test -p blue-marshal reads_i64`
Expected: PASS.

- [ ] **Step 3: Make `bmdump scan` fail on an empty directory**

In `crates/blue-marshal/src/bin/bmdump.rs`, in `scan`, right after `collect(dir, &mut files);` insert:

```rust
    if files.is_empty() {
        eprintln!("no .dat files found under {}", dir.display());
        return ExitCode::from(2);
    }
```

Run: `cargo run --bin bmdump -- scan docs`
Expected: `no .dat files found under docs`, exit code 2 (`$LASTEXITCODE` is 2).

- [ ] **Step 4: Escape quotes/backslashes in dumps and split the hex prefix**

In `crates/blue-marshal/src/value.rs`, replace `write_bytes_body` with:

```rust
/// Shared rendering for byte strings (`Bytes` and `Global`): printable ASCII
/// (and empty) renders quoted with `quoted_prefix` (embedded `"` and `\`
/// escaped with a backslash); anything else renders as hex with `hex_prefix`,
/// so `Bytes` and `Global` stay distinguishable in both branches.
fn write_bytes_body(out: &mut String, b: &[u8], quoted_prefix: &str, hex_prefix: &str) {
    if b.iter().all(|c| (0x20..0x7F).contains(c)) {
        out.push_str(quoted_prefix);
        out.push('"');
        for &c in b {
            if c == b'"' || c == b'\\' {
                out.push('\\');
            }
            out.push(c as char);
        }
        out.push('"');
    } else {
        let _ = write!(out, "{hex_prefix}{}", hex(b));
    }
}
```

Update the two call sites in `write_value`:

```rust
        Value::Bytes(b) => {
```
…keep the nested-stream attempt unchanged, but change its fallback line to:
```rust
            write_bytes_body(out, b, "b", "hex:");
```
and:
```rust
        Value::Global(name) => write_bytes_body(out, name, "global:", "global-hex:"),
```

Add tests to `value.rs`'s `tests` module:

```rust
    #[test]
    fn dump_bytes_escapes_quotes_and_backslashes() {
        assert_eq!(
            dump_text(&Value::Bytes(b"a\"b\\c".to_vec())),
            r#"b"a\"b\\c""#
        );
    }

    #[test]
    fn dump_global_nonprintable_uses_global_hex_prefix() {
        assert_eq!(dump_text(&Value::Global(vec![0x00, 0xFF])), "global-hex:00ff");
        assert_eq!(dump_text(&Value::Bytes(vec![0x00, 0xFF])), "hex:00ff");
    }
```

Run: `cargo test -p blue-marshal dump_`
Expected: all dump tests PASS (existing `dump_bytes_printable_and_hex` and `dump_global_and_instance` must still pass — the `("b", "hex:")` pair reproduces the old `Bytes` output exactly).

- [ ] **Step 5: Add isolated SHARED_FLAG+REF tests for GLOBAL / INSTANCE / REDUCE**

These pin the encounter-order slot mechanics for the three opcodes whose sharing was previously only covered indirectly by the corpus gate — and they double as the encoder's spec (Task 3 reuses these exact byte strings in reverse). Add to `decode.rs`'s `tests` module:

```rust
    #[test]
    fn shared_global_stored_then_ref() {
        // 7E | count=1 | TUPLE2( GLOBAL|SHARED "__builtin__.set", REF 1 ) | map[0]=1
        let mut data = vec![
            0x7E, 0x01, 0x00, 0x00, 0x00, // header, shared_count = 1
            0x2C, // TUPLE2
            0x42, 15, // GLOBAL(0x02)|SHARED_FLAG, len 15
        ];
        data.extend_from_slice(b"__builtin__.set");
        data.extend_from_slice(&[0x1B, 0x01]); // REF -> slot 1
        data.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]); // tail map: slot 1
        let g = Value::Global(b"__builtin__.set".to_vec());
        assert_eq!(decode(&data).unwrap(), Value::Tuple(vec![g.clone(), g]));
    }

    #[test]
    fn shared_instance_stored_then_ref() {
        // 7E | count=1 | TUPLE2( INSTANCE|SHARED(class "M.Cls", state ONE), REF 1 ) | map[0]=1
        let data = [
            0x7E, 0x01, 0x00, 0x00, 0x00, // header, shared_count = 1
            0x2C, // TUPLE2
            0x57, // INSTANCE(0x17)|SHARED_FLAG — reserves slot 1 at open
            0x13, 0x05, b'M', b'.', b'C', b'l', b's', // class BUFFER "M.Cls"
            0x09, // state: ONE
            0x1B, 0x01, // REF -> slot 1
            0x01, 0x00, 0x00, 0x00, // tail map: slot 1
        ];
        let inst = Value::Instance {
            class: Box::new(Value::Bytes(b"M.Cls".to_vec())),
            state: vec![Value::Int(1)],
        };
        assert_eq!(decode(&data).unwrap(), Value::Tuple(vec![inst.clone(), inst]));
    }

    #[test]
    fn shared_reduce_stored_then_ref() {
        // 7E | count=1 | TUPLE2( REDUCE|SHARED((M.f, ()), empty tail), REF 1 ) | map[0]=1
        let data = [
            0x7E, 0x01, 0x00, 0x00, 0x00, // header, shared_count = 1
            0x2C, // TUPLE2
            0x62, // REDUCE(0x22)|SHARED_FLAG — reserves slot 1 at open
            0x2C, // ctor TUPLE2
            0x02, 3, b'M', b'.', b'f', // GLOBAL "M.f"
            0x24, // args TUPLE0
            0x2D, 0x2D, // empty list-then-dict iterator tail
            0x1B, 0x01, // REF -> slot 1
            0x01, 0x00, 0x00, 0x00, // tail map: slot 1
        ];
        let ctor = Value::Tuple(vec![Value::Global(b"M.f".to_vec()), Value::Tuple(vec![])]);
        let red = Value::Instance { class: Box::new(ctor), state: vec![] };
        assert_eq!(decode(&data).unwrap(), Value::Tuple(vec![red.clone(), red]));
    }
```

Run: `cargo test -p blue-marshal shared_`
Expected: all PASS (including the pre-existing `shared_*` tests).

- [ ] **Step 6: Document the no-cycles limitation**

Append to the `decode.rs` module doc comment (the `//!` block at the top):

```rust
//!
//! Known deviation: the reference stores containers into their shared slot at
//! container *open* (NEW_SEQUENCE/RESERVE_SLOT), which lets a REF inside a
//! container point back at the container itself — a cyclic reference. This
//! decoder stores the completed value *after* its children decode, so such a
//! self-referential REF fails with `BadRef` instead. An owned `Value` tree
//! cannot represent a cycle anyway, and no corpus file contains one (all 5022
//! decode cleanly).
```

Add under `## Status` in `docs/format-notes.md`:

```markdown
- Known decoder deviation (documented 2026-07-13): cyclic shared references
  (a REF back into its own still-open container — legal for the reference
  decoder, which stores container slots at open) fail with `BadRef`. No
  corpus file contains a cycle; a Rust `Value` tree could not represent one.
```

- [ ] **Step 7: Add the MIT license**

Create `LICENSE` at the repo root (spec §11: license chosen up front, MIT):

```
MIT License

Copyright (c) 2026 Antoine Jacquin-Ravot

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

Also add `license = "MIT"` to `[package]` in `crates/blue-marshal/Cargo.toml`.

- [ ] **Step 8: Full suite green, then the single batch commit**

Run: `cargo test`
Expected: all tests PASS (including the corpus decode gate if the corpus is present).

```powershell
git add -A
git commit -m "Batch M0 review minors: dead variant, dump escaping, shared-opcode tests, no-cycles note, MIT license"
```

---

### Task 2: Fidelity-tagged Value model and decoder rework

Reshape `Value` so a decoded tree carries everything needed to re-emit the original bytes: explicit shared slots, split INSTANCE/REDUCE, tagged text variants, and a hard error on trailing bytes. `decode`'s wire *parsing* does not change — only what it builds.

**Files:**
- Modify: `crates/blue-marshal/src/value.rs`, `crates/blue-marshal/src/decode.rs`, `crates/blue-marshal/src/error.rs`

**Interfaces:**
- Consumes: Task 1's escaped `write_bytes_body(out, b, quoted_prefix, hex_prefix)`.
- Produces: the `Value` enum below (exact shape — Task 3's encoder and M1b both consume it), `Value::unshared(&self) -> &Value`, and `ErrorKind::TrailingBytes(usize)`.

- [ ] **Step 1: Replace the `Value` enum**

In `crates/blue-marshal/src/value.rs`, replace the enum (keep the file's other items):

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    None,
    Bool(bool),
    /// Encoded canonically by magnitude: -1/0/1 → MINUSONE/ZERO/ONE, then the
    /// narrowest of INT8/INT16/INT32/INT64. Corpus-proven exact: the client
    /// never writes a non-minimal width (int_nonminimal = 0 over 5022 files).
    Int(i64),
    /// LONG (0x2F) payload: raw little-endian two's-complement bytes.
    Long(Vec<u8>),
    /// Encoded canonically: exact +0.0 bit pattern → FLOAT0, else FLOAT
    /// (−0.0 has a different bit pattern and stays FLOAT). Corpus-proven:
    /// FLOAT never carries +0.0 (float_pluszero = 0).
    Float(f64),
    /// Byte string. Encoded canonically by length: 0 → STRING0, 1 → STRING1,
    /// else BUFFER. Corpus-proven: BUFFER never holds ≤ 1 bytes and the
    /// deprecated STRING/STRINGL opcodes never occur.
    Bytes(Vec<u8>),
    /// Text stored as UTF8 (0x2E) — the dominant wire form (11M occurrences).
    Str(String),
    /// Text stored in the UCS-2 family, encoded by UTF-16 unit count:
    /// 0 → UNICODE0, 1 → UNICODE1, n → UNICODE. Separate from `Str` because
    /// the client emits BOTH UTF8("") and UNICODE0 — content alone cannot
    /// recover the opcode.
    StrUcs2(String),
    /// STRINGR (0x11): index 1..=255 into [`crate::string_table::STRING_TABLE`].
    /// Kept as the index because the client also writes table-content strings
    /// as UTF8 (1269 corpus collisions) — content alone cannot recover the
    /// opcode.
    StrTable(u8),
    Tuple(Vec<Value>),
    List(Vec<Value>),
    /// Entries in wire order (each entry is encoded value-first on the wire;
    /// normalized to (key, value) here).
    Dict(Vec<(Value, Value)>),
    /// STREAM (0x2B): the payload is a complete nested marshal stream,
    /// decoded recursively and re-encoded recursively.
    Stream(Box<Value>),
    /// GLOBAL (0x02): a dotted Python type/function name.
    Global(Vec<u8>),
    /// INSTANCE (0x17): class-name object, then exactly one state object.
    Instance { class: Box<Value>, state: Box<Value> },
    /// REDUCE (0x22): ctor tuple `(callable, args[, state])`, then the
    /// MARK-terminated list items, then the MARK-terminated (key, value)
    /// pairs — the wire framing kept verbatim (both empty in every corpus
    /// occurrence).
    Reduce { ctor: Box<Value>, items: Vec<Value>, pairs: Vec<(Value, Value)> },
    /// A SHARED_FLAG-ed store: wraps exactly the node whose opcode carried
    /// the flag; `slot` is the 1-based tail-map slot it was stored into.
    /// Slots are explicit because corpus tail maps are heavily non-identity
    /// (963,660 out-of-order entries).
    Shared { slot: u32, value: Box<Value> },
    /// REF (0x1B): points at the `Shared` node with the same slot number,
    /// which must appear earlier in the stream.
    Ref(u32),
}

impl Value {
    /// Peel a `Shared` wrapper (if any) to reach the stored node — the
    /// wrapper is wire bookkeeping, not data. Does NOT resolve `Ref`.
    pub fn unshared(&self) -> &Value {
        match self {
            Value::Shared { value, .. } => value,
            other => other,
        }
    }
}
```

- [ ] **Step 2: Update `write_value` for the new variants**

In the same file, adjust `write_value`'s match:

- `Value::Str(s)` — unchanged (`write!(out, "{s:?}")`).
- Add after the `Str` arm:

```rust
        Value::StrUcs2(s) => {
            out.push('u');
            let _ = write!(out, "{s:?}");
        }
        Value::StrTable(idx) => {
            let _ = write!(
                out,
                "t{idx}:{:?}",
                crate::string_table::STRING_TABLE[*idx as usize]
            );
        }
```

- Replace the `Value::Instance` arm (state is a single value now) and add `Reduce`, `Shared`, `Ref` arms:

```rust
        Value::Instance { class, state } => {
            out.push_str("instance{\n");
            let _ = write!(out, "{pad}  class: ");
            write_value(out, class, indent + 1, stream_depth);
            out.push('\n');
            let _ = write!(out, "{pad}  state: ");
            write_value(out, state, indent + 1, stream_depth);
            out.push('\n');
            let _ = write!(out, "{pad}}}");
        }
        Value::Reduce { ctor, items, pairs } => {
            out.push_str("reduce{\n");
            let _ = write!(out, "{pad}  ctor: ");
            write_value(out, ctor, indent + 1, stream_depth);
            out.push('\n');
            let _ = write!(out, "{pad}  items: ");
            write_seq(out, items, indent + 1, '[', ']', stream_depth);
            out.push('\n');
            let _ = write!(out, "{pad}  pairs: ");
            if pairs.is_empty() {
                out.push_str("{}");
            } else {
                // Wire order, NOT sorted — this is a fidelity view of the
                // REDUCE iterator tail, unlike Dict's sorted diff view.
                out.push_str("{\n");
                for (k, v) in pairs {
                    let _ = write!(out, "{pad}    ");
                    write_value(out, k, indent + 2, stream_depth);
                    out.push_str(": ");
                    write_value(out, v, indent + 2, stream_depth);
                    out.push('\n');
                }
                let _ = write!(out, "{pad}  }}");
            }
            out.push('\n');
            let _ = write!(out, "{pad}}}");
        }
        Value::Shared { slot, value } => {
            let _ = write!(out, "shared[{slot}]:");
            write_value(out, value, indent, stream_depth);
        }
        Value::Ref(slot) => {
            let _ = write!(out, "ref[{slot}]");
        }
```

- [ ] **Step 3: Update `value.rs` tests for the new shapes**

Update `dump_global_and_instance`'s instance half to the new shape and expected text:

```rust
        let inst = Value::Instance {
            class: Box::new(Value::Bytes(b"M.Cls".to_vec())),
            state: Box::new(Value::Int(1)),
        };
        assert_eq!(
            dump_text(&inst),
            "instance{\n  class: b\"M.Cls\"\n  state: 1\n}"
        );
```

Add new dump tests:

```rust
    #[test]
    fn dump_new_string_variants() {
        assert_eq!(dump_text(&Value::StrUcs2("hi".into())), "u\"hi\"");
        // Expected text derives from the table itself so the test does not
        // hardcode table content.
        assert_eq!(
            dump_text(&Value::StrTable(7)),
            format!("t7:{:?}", crate::string_table::STRING_TABLE[7])
        );
    }

    #[test]
    fn dump_shared_ref_and_reduce() {
        let v = Value::Tuple(vec![
            Value::Shared { slot: 2, value: Box::new(Value::List(vec![])) },
            Value::Ref(2),
        ]);
        assert_eq!(dump_text(&v), "(\n  shared[2]:[]\n  ref[2]\n)");
        let r = Value::Reduce {
            ctor: Box::new(Value::Global(b"M.f".to_vec())),
            items: vec![Value::Int(1)],
            pairs: vec![(Value::Int(0), Value::Int(1))],
        };
        assert_eq!(
            dump_text(&r),
            "reduce{\n  ctor: global:\"M.f\"\n  items: [\n    1\n  ]\n  pairs: {\n    0: 1\n  }\n}"
        );
    }

    #[test]
    fn unshared_peels_exactly_one_wrapper() {
        let inner = Value::Int(5);
        let shared = Value::Shared { slot: 1, value: Box::new(inner.clone()) };
        assert_eq!(shared.unshared(), &inner);
        assert_eq!(inner.unshared(), &inner);
    }
```

- [ ] **Step 4: Add `TrailingBytes` and rework the decoder**

In `crates/blue-marshal/src/error.rs`, add to `ErrorKind`:

```rust
    /// Bytes remained between the end of the root object and the tail map.
    /// Corpus-proven never to happen (slack_streams = 0 over 5022 files), so
    /// it is a hard error: it would mean we mis-parsed the stream.
    TrailingBytes(usize),
```

In `crates/blue-marshal/src/decode.rs`:

1. `Decoder.shared` becomes a populated-flag vector (`Vec<bool>` — the stored values now live in the tree as `Shared` nodes, so nothing is cloned into a side table anymore):

```rust
struct Decoder<'a> {
    /// Whether each shared slot (indexed by map entry - 1) has been stored.
    shared: Vec<bool>,
    /// Encounter counter: index of the next tail-map entry to consume.
    shared_next: usize,
    /// Full stream, used only to read tail-map slot numbers.
    data: &'a [u8],
    /// Current object-nesting depth; guarded against [`MAX_DEPTH`].
    depth: usize,
}
```

and construct it with `shared: vec![false; shared_count]`.

2. In `decode_at_depth`, capture the result and reject slack (the payload `Reader` is bounded to `data[..map_start]`, so `r.remaining()` after the root object is exactly the slack):

```rust
    let value = dec.load(&mut r)?;
    if r.remaining() > 0 {
        return Err(DecodeError {
            offset: r.pos(),
            kind: ErrorKind::TrailingBytes(r.remaining()),
        });
    }
    Ok(value)
```

3. In `load`, wrap stored objects instead of cloning them into a side table:

```rust
        let value = self.load_op(code, r, opcode_offset)?;

        self.depth -= 1;
        Ok(match slot {
            Some(slot) => {
                self.shared[slot - 1] = true;
                Value::Shared { slot: slot as u32, value: Box::new(value) }
            }
            None => value,
        })
```

4. In `load_op`, change these arms (all other arms stay byte-for-byte identical):

```rust
            op::STRINGR => {
                let idx = r.read_len()?;
                if idx < 1 || idx >= STRING_TABLE.len() {
                    return Err(DecodeError { offset: at, kind: ErrorKind::BadStringRef(idx) });
                }
                Value::StrTable(idx as u8)
            }
            op::UNICODE0 => Value::StrUcs2(String::new()),
            op::UNICODE1 | op::UNICODE => {
                let units = if code == op::UNICODE1 { 1 } else { r.read_len()? };
                let bytes = r.read_bytes(units * 2)?;
                let u16s: Vec<u16> = bytes
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                String::from_utf16(&u16s)
                    .map(Value::StrUcs2)
                    .map_err(|_| DecodeError { offset: at, kind: ErrorKind::BadUtf8 })?
            }
```

(UTF8 keeps producing `Value::Str`.)

```rust
            op::REF => {
                let idx = r.read_len()?;
                if idx < 1 || idx > self.shared.len() || !self.shared[idx - 1] {
                    return Err(DecodeError { offset: at, kind: ErrorKind::BadRef(idx) });
                }
                Value::Ref(idx as u32)
            }
```

```rust
            op::INSTANCE => {
                let class = self.load(r)?;
                let state = self.load(r)?;
                Value::Instance { class: Box::new(class), state: Box::new(state) }
            }
```

```rust
            op::REDUCE => {
                let ctor = self.load(r)?;
                let mut items = Vec::new();
                loop {
                    let next = r.peek_u8()?;
                    if next & !op::SHARED_FLAG == op::MARK {
                        r.read_u8()?;
                        break;
                    }
                    items.push(self.load(r)?);
                }
                let mut pairs = Vec::new();
                loop {
                    let next = r.peek_u8()?;
                    if next & !op::SHARED_FLAG == op::MARK {
                        r.read_u8()?;
                        break;
                    }
                    let key = self.load(r)?;
                    let value = self.load(r)?;
                    pairs.push((key, value));
                }
                Value::Reduce { ctor: Box::new(ctor), items, pairs }
            }
```

- [ ] **Step 5: Update the decoder tests to the new expected shapes**

In `decode.rs` tests, update expectations (wire bytes unchanged everywhere):

- `shared_buffer_stored_then_ref`:
```rust
        assert_eq!(
            decode(&data).unwrap(),
            Value::Tuple(vec![
                Value::Shared { slot: 1, value: Box::new(Value::Bytes(b"hi".to_vec())) },
                Value::Ref(1),
            ])
        );
```
- `shared_dict_stored_then_ref`:
```rust
        let dict = Value::Dict(vec![(Value::Bytes(b"k".to_vec()), Value::Int(1))]);
        assert_eq!(
            decode(&data).unwrap(),
            Value::Tuple(vec![
                Value::Shared { slot: 1, value: Box::new(dict) },
                Value::Ref(1),
            ])
        );
```
- `shared_slots_assigned_in_encounter_order_not_completion_order` (same bytes; the tree now shows the non-identity map directly — outer TUPLE1 gets slot 2, inner LONG slot 1):
```rust
        let long = Value::Shared { slot: 1, value: Box::new(Value::Long(vec![0x2A])) };
        let inner = Value::Shared {
            slot: 2,
            value: Box::new(Value::Tuple(vec![long])),
        };
        assert_eq!(
            decode(&data).unwrap(),
            Value::Tuple(vec![inner, Value::Ref(1), Value::Ref(2)])
        );
```
- `decodes_instance_class_then_state`:
```rust
            Value::Instance {
                class: Box::new(Value::Bytes(b"M.Cls".to_vec())),
                state: Box::new(Value::Int(1)),
            }
```
- `decodes_reduce_ctor_then_double_mark_tail`:
```rust
            Value::Reduce { ctor: Box::new(ctor), items: vec![], pairs: vec![] }
```
- `decodes_reduce_with_nonempty_iterator_tail`:
```rust
            Value::Reduce {
                ctor: Box::new(ctor),
                items: vec![Value::Int(1)],
                pairs: vec![(Value::Int(0), Value::Int(1))],
            }
```
- Task 1's `shared_global_stored_then_ref`:
```rust
        let g = Value::Shared {
            slot: 1,
            value: Box::new(Value::Global(b"__builtin__.set".to_vec())),
        };
        assert_eq!(decode(&data).unwrap(), Value::Tuple(vec![g, Value::Ref(1)]));
```
- Task 1's `shared_instance_stored_then_ref`:
```rust
        let inst = Value::Shared {
            slot: 1,
            value: Box::new(Value::Instance {
                class: Box::new(Value::Bytes(b"M.Cls".to_vec())),
                state: Box::new(Value::Int(1)),
            }),
        };
        assert_eq!(decode(&data).unwrap(), Value::Tuple(vec![inst, Value::Ref(1)]));
```
- Task 1's `shared_reduce_stored_then_ref`:
```rust
        let ctor = Value::Tuple(vec![Value::Global(b"M.f".to_vec()), Value::Tuple(vec![])]);
        let red = Value::Shared {
            slot: 1,
            value: Box::new(Value::Reduce { ctor: Box::new(ctor), items: vec![], pairs: vec![] }),
        };
        assert_eq!(decode(&data).unwrap(), Value::Tuple(vec![red, Value::Ref(1)]));
```

Add new coverage for the changed arms:

```rust
    #[test]
    fn decodes_text_variants_distinctly() {
        // UTF8 "abc"
        assert_eq!(
            decode(&stream(&[0x2E, 0x03, b'a', b'b', b'c'])).unwrap(),
            Value::Str("abc".into())
        );
        // UNICODE0 / UNICODE1 'x' / UNICODE "hi" (2 UTF-16LE units)
        assert_eq!(decode(&stream(&[0x28])).unwrap(), Value::StrUcs2(String::new()));
        assert_eq!(
            decode(&stream(&[0x29, b'x', 0x00])).unwrap(),
            Value::StrUcs2("x".into())
        );
        assert_eq!(
            decode(&stream(&[0x12, 0x02, b'h', 0x00, b'i', 0x00])).unwrap(),
            Value::StrUcs2("hi".into())
        );
        // STRINGR keeps the index, not the content
        assert_eq!(decode(&stream(&[0x11, 0x07])).unwrap(), Value::StrTable(7));
        // UTF8 "" and UNICODE0 stay distinguishable — the corpus contains both
        assert_ne!(
            decode(&stream(&[0x2E, 0x00])).unwrap(),
            decode(&stream(&[0x28])).unwrap()
        );
    }

    #[test]
    fn trailing_bytes_after_root_are_a_hard_error() {
        // Valid ONE root followed by a stray NONE byte before the (empty)
        // tail map — previously silently ignored, now a decode error.
        let err = decode(&stream(&[0x09, 0x01])).unwrap_err();
        assert_eq!(err.kind, crate::ErrorKind::TrailingBytes(1));
        assert_eq!(err.offset, 6); // header(5) + the ONE opcode
    }
```

- [ ] **Step 6: Full suite green, then commit**

Run: `cargo test`
Expected: all unit tests PASS and the corpus gate `every_corpus_file_decodes` still passes (the `TrailingBytes` promotion is corpus-proven safe; if any file fails with `TrailingBytes`, STOP — the measurement was wrong; report it instead of weakening the error).

```powershell
git add -A
git commit -m "Rework Value into a fidelity-tagged model with explicit shared slots"
```

---

### Task 3: The encoder

A new `encode` module, the exact inverse of `decode`: canonical opcode selection (corpus-proven), minimal length encoding, encounter-order tail-map assembly from `Shared` nodes, and the same `MAX_DEPTH` guard.

**Files:**
- Create: `crates/blue-marshal/src/encode.rs`
- Modify: `crates/blue-marshal/src/error.rs`, `crates/blue-marshal/src/lib.rs`

**Interfaces:**
- Consumes: Task 2's `Value` (exact shape), `crate::decode::MAX_DEPTH` (already `pub(crate)`), `crate::opcodes`.
- Produces: `pub fn encode(root: &Value) -> Result<Vec<u8>, EncodeError>` re-exported from the crate root, plus `EncodeError { kind: EncodeErrorKind }` — Tasks 4–5 and M1b's save chain call exactly this.

- [ ] **Step 1: Add the encode error type**

In `crates/blue-marshal/src/error.rs`, append:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodeError {
    pub kind: EncodeErrorKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeErrorKind {
    /// Object hierarchy deeper than MAX_DEPTH (mirrors the decode guard, so
    /// any tree that decoded successfully re-encodes within the bound).
    TooDeep,
    /// A length, count, or shared-map size exceeds the wire format's i32 range.
    TooLong(usize),
    /// SHARED_FLAG requested (via `Value::Shared`) on a node whose emitted
    /// opcode the reference decoder ignores the flag for — emitting it would
    /// desynchronize the tail map. Carries the node's kind name.
    NotStorable(&'static str),
    /// A tail-map slot number outside 1..=shared_count (e.g. after deleting
    /// a `Shared` node while keeping higher slot numbers).
    SlotOutOfRange { slot: u32, count: usize },
    /// A `Ref` appeared before the `Shared` node storing its slot completed —
    /// includes self-referential (cyclic) refs, which this codec rejects on
    /// both sides.
    RefBeforeStore(u32),
    /// `StrTable(0)` — wire index 0 is rejected by the reference decoder.
    BadTableIndex,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "encode error: {:?}", self.kind)
    }
}

impl std::error::Error for EncodeError {}
```

- [ ] **Step 2: Write the encoder**

Create `crates/blue-marshal/src/encode.rs`:

```rust
//! Encoder for a blue-marshal stream — the exact inverse of [`crate::decode`].
//! For any tree produced by `decode`, the output is byte-identical to the
//! original stream (gated over the full corpus in tests/corpus.rs). Every
//! canonical opcode choice below is corpus-proven exact; see
//! `docs/format-notes.md`, "Corpus canonicality measurements (2026-07-13)".

use std::collections::HashSet;

use crate::decode::MAX_DEPTH;
use crate::error::{EncodeError, EncodeErrorKind};
use crate::opcodes as op;
use crate::value::Value;

/// Encode a [`Value`] into a complete blue-marshal stream.
pub fn encode(root: &Value) -> Result<Vec<u8>, EncodeError> {
    encode_at_depth(root, 0)
}

/// Inner entry point threading the nesting depth through embedded STREAM
/// encodes, mirroring `decode_at_depth`.
fn encode_at_depth(root: &Value, depth: usize) -> Result<Vec<u8>, EncodeError> {
    let mut e = Encoder {
        out: vec![op::PROTOCOL, 0, 0, 0, 0], // count patched below
        map: Vec::new(),
        stored: HashSet::new(),
        depth,
    };
    e.emit(root)?;
    let count = e.map.len();
    for &slot in &e.map {
        if slot < 1 || slot as usize > count {
            return Err(EncodeError { kind: EncodeErrorKind::SlotOutOfRange { slot, count } });
        }
    }
    // The reference reads the header count and every map entry as signed i32.
    let count32 =
        i32::try_from(count).map_err(|_| EncodeError { kind: EncodeErrorKind::TooLong(count) })?;
    e.out[1..5].copy_from_slice(&count32.to_le_bytes());
    let map = std::mem::take(&mut e.map);
    for slot in map {
        e.out.extend_from_slice(&(slot as i32).to_le_bytes());
    }
    Ok(e.out)
}

struct Encoder {
    out: Vec<u8>,
    /// Tail-map slot numbers in encounter order (one entry per `Shared`).
    map: Vec<u32>,
    /// Slots whose `Shared` node has fully emitted — the only valid `Ref`
    /// targets, mirroring the decoder's store-on-completion semantics.
    stored: HashSet<u32>,
    depth: usize,
}

impl Encoder {
    fn emit(&mut self, v: &Value) -> Result<(), EncodeError> {
        // Depth guard symmetric with decode's (`load` charges one level per
        // node), so any tree that decoded successfully re-encodes in bound.
        if self.depth >= MAX_DEPTH {
            return Err(EncodeError { kind: EncodeErrorKind::TooDeep });
        }
        self.depth += 1;
        let result = match v {
            Value::Shared { slot, value } => {
                // Map entry claimed at *encounter* (open) time, before the
                // children emit — marshal.c's STORE/RESERVE_SLOT order. The
                // slot becomes REF-able only after the body completes.
                self.map.push(*slot);
                let r = self.emit_body(value, op::SHARED_FLAG);
                if r.is_ok() {
                    self.stored.insert(*slot);
                }
                r
            }
            other => self.emit_body(other, 0),
        };
        self.depth -= 1;
        result
    }

    /// Emit one node's opcode, payload, and children. `flag` is 0 or
    /// SHARED_FLAG; the flag is only legal on nodes whose emitted opcode the
    /// reference actually stores (decode.rs `stores_shared`) — anywhere else
    /// the reference ignores it, the tail map would desynchronize, and we
    /// reject instead.
    fn emit_body(&mut self, v: &Value, flag: u8) -> Result<(), EncodeError> {
        if flag != 0 && !storable_with_flag(v) {
            return Err(EncodeError { kind: EncodeErrorKind::NotStorable(kind_name(v)) });
        }
        match v {
            Value::None => self.out.push(op::NONE),
            Value::Bool(true) => self.out.push(op::TRUE),
            Value::Bool(false) => self.out.push(op::FALSE),
            Value::Int(n) => match *n {
                -1 => self.out.push(op::MINUSONE),
                0 => self.out.push(op::ZERO),
                1 => self.out.push(op::ONE),
                n if i8::try_from(n).is_ok() => {
                    self.out.push(op::INT8);
                    self.out.push(n as i8 as u8);
                }
                n if i16::try_from(n).is_ok() => {
                    self.out.push(op::INT16);
                    self.out.extend_from_slice(&(n as i16).to_le_bytes());
                }
                n if i32::try_from(n).is_ok() => {
                    self.out.push(op::INT32);
                    self.out.extend_from_slice(&(n as i32).to_le_bytes());
                }
                n => {
                    self.out.push(op::INT64);
                    self.out.extend_from_slice(&n.to_le_bytes());
                }
            },
            Value::Long(bytes) => {
                self.out.push(op::LONG | flag);
                self.write_len(bytes.len())?;
                self.out.extend_from_slice(bytes);
            }
            Value::Float(f) => {
                if f.to_bits() == 0f64.to_bits() {
                    self.out.push(op::FLOAT0);
                } else {
                    // Bit-exact via to_le_bytes; −0.0 and NaN payloads pass
                    // through untouched.
                    self.out.push(op::FLOAT);
                    self.out.extend_from_slice(&f.to_le_bytes());
                }
            }
            Value::Bytes(b) => match b.len() {
                0 => self.out.push(op::STRING0),
                1 => {
                    self.out.push(op::STRING1);
                    self.out.push(b[0]);
                }
                n => {
                    self.out.push(op::BUFFER | flag);
                    self.write_len(n)?;
                    self.out.extend_from_slice(b);
                }
            },
            Value::Str(s) => {
                self.out.push(op::UTF8);
                self.write_len(s.len())?;
                self.out.extend_from_slice(s.as_bytes());
            }
            Value::StrUcs2(s) => {
                let units: Vec<u16> = s.encode_utf16().collect();
                match units.len() {
                    0 => self.out.push(op::UNICODE0),
                    1 => {
                        self.out.push(op::UNICODE1);
                        self.out.extend_from_slice(&units[0].to_le_bytes());
                    }
                    n => {
                        self.out.push(op::UNICODE);
                        self.write_len(n)?;
                        for u in &units {
                            self.out.extend_from_slice(&u.to_le_bytes());
                        }
                    }
                }
            }
            Value::StrTable(idx) => {
                if *idx == 0 {
                    return Err(EncodeError { kind: EncodeErrorKind::BadTableIndex });
                }
                self.out.push(op::STRINGR);
                self.write_len(*idx as usize)?;
            }
            Value::Tuple(items) => {
                match items.len() {
                    0 => self.out.push(op::TUPLE0), // does not store; flag rejected above
                    1 => self.out.push(op::TUPLE1 | flag),
                    2 => self.out.push(op::TUPLE2 | flag),
                    n => {
                        self.out.push(op::TUPLE | flag);
                        self.write_len(n)?;
                    }
                }
                for item in items {
                    self.emit(item)?;
                }
            }
            Value::List(items) => {
                match items.len() {
                    0 => self.out.push(op::LIST0 | flag), // LIST0 stores (the reference asymmetry)
                    1 => self.out.push(op::LIST1 | flag),
                    n => {
                        self.out.push(op::LIST | flag);
                        self.write_len(n)?;
                    }
                }
                for item in items {
                    self.emit(item)?;
                }
            }
            Value::Dict(entries) => {
                self.out.push(op::DICT | flag);
                self.write_len(entries.len())?;
                for (key, value) in entries {
                    self.emit(value)?; // wire order: value first (marshal.c:182)
                    self.emit(key)?;
                }
            }
            Value::Stream(inner) => {
                let bytes = encode_at_depth(inner, self.depth)?;
                self.out.push(op::STREAM | flag);
                self.write_len(bytes.len())?;
                self.out.extend_from_slice(&bytes);
            }
            Value::Global(name) => {
                self.out.push(op::GLOBAL | flag);
                self.write_len(name.len())?;
                self.out.extend_from_slice(name);
            }
            Value::Instance { class, state } => {
                self.out.push(op::INSTANCE | flag);
                self.emit(class)?;
                self.emit(state)?;
            }
            Value::Reduce { ctor, items, pairs } => {
                self.out.push(op::REDUCE | flag);
                self.emit(ctor)?;
                for item in items {
                    self.emit(item)?;
                }
                self.out.push(op::MARK);
                for (key, value) in pairs {
                    self.emit(key)?; // iterated order: key first (marshal.c:182)
                    self.emit(value)?;
                }
                self.out.push(op::MARK);
            }
            Value::Ref(slot) => {
                if !self.stored.contains(slot) {
                    return Err(EncodeError { kind: EncodeErrorKind::RefBeforeStore(*slot) });
                }
                self.out.push(op::REF);
                self.write_len(*slot as usize)?;
            }
            Value::Shared { .. } => {
                // Only reachable as the direct child of another `Shared` (the
                // outer wrapper peels in `emit`); one opcode byte carries one
                // flag, so this cannot exist on the wire. The flag!=0 check
                // above already rejected it (Shared is not storable), so this
                // arm exists for match exhaustiveness on the flag==0 path,
                // which `emit` makes unreachable.
                return Err(EncodeError { kind: EncodeErrorKind::NotStorable("shared") });
            }
        }
        Ok(())
    }

    /// Blue length encoding, always minimal (corpus-proven: zero non-minimal
    /// escapes among 228,549): 0..=254 as one byte, 255.. as 0xFF + i32 LE.
    fn write_len(&mut self, n: usize) -> Result<(), EncodeError> {
        if n < 255 {
            self.out.push(n as u8);
        } else {
            let v = i32::try_from(n)
                .map_err(|_| EncodeError { kind: EncodeErrorKind::TooLong(n) })?;
            self.out.push(0xFF);
            self.out.extend_from_slice(&v.to_le_bytes());
        }
        Ok(())
    }
}

/// Whether SHARED_FLAG may be emitted on this node — true exactly when the
/// opcode `emit_body` will choose is in decode.rs's `stores_shared` set.
/// Length-dependent: `Bytes` of len ≤ 1 emits STRING0/STRING1 and an empty
/// `Tuple` emits TUPLE0, none of which store (while LIST0 does).
fn storable_with_flag(v: &Value) -> bool {
    match v {
        Value::Long(_)
        | Value::List(_)
        | Value::Dict(_)
        | Value::Stream(_)
        | Value::Global(_)
        | Value::Instance { .. }
        | Value::Reduce { .. } => true,
        Value::Bytes(b) => b.len() >= 2,
        Value::Tuple(items) => !items.is_empty(),
        _ => false,
    }
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
        Value::StrUcs2(_) => "str-ucs2",
        Value::StrTable(_) => "str-table",
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

- [ ] **Step 3: Wire it into the crate root**

In `crates/blue-marshal/src/lib.rs`: change the crate doc first line to
`//! Decoder and encoder for CCP's "blue marshal" serialization used by EVE Online`,
add `pub mod encode;` to the module list (alphabetical, after `decode`), and extend the re-exports:

```rust
pub use decode::decode;
pub use encode::encode;
pub use error::{DecodeError, EncodeError, EncodeErrorKind, ErrorKind};
pub use value::{dump_text, Value};
```

Run: `cargo build`
Expected: compiles clean.

- [ ] **Step 4: Encoder unit tests**

Append to `encode.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::decode;

    // header: magic 0x7E + i32 shared-count 0 (same helper as decode tests)
    fn stream(body: &[u8]) -> Vec<u8> {
        let mut v = vec![0x7E, 0, 0, 0, 0];
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn encodes_scalars_canonically() {
        assert_eq!(encode(&Value::None).unwrap(), stream(&[0x01]));
        assert_eq!(encode(&Value::Bool(true)).unwrap(), stream(&[0x1F]));
        assert_eq!(encode(&Value::Bool(false)).unwrap(), stream(&[0x20]));
        assert_eq!(encode(&Value::Int(-1)).unwrap(), stream(&[0x07]));
        assert_eq!(encode(&Value::Int(0)).unwrap(), stream(&[0x08]));
        assert_eq!(encode(&Value::Int(1)).unwrap(), stream(&[0x09]));
        assert_eq!(encode(&Value::Int(42)).unwrap(), stream(&[0x06, 0x2A]));
        assert_eq!(encode(&Value::Int(-2)).unwrap(), stream(&[0x06, 0xFE]));
        // Width boundaries: 127 is the last INT8, 128 the first INT16, etc.
        assert_eq!(encode(&Value::Int(127)).unwrap(), stream(&[0x06, 0x7F]));
        assert_eq!(encode(&Value::Int(128)).unwrap(), stream(&[0x05, 0x80, 0x00]));
        assert_eq!(encode(&Value::Int(0x1234)).unwrap(), stream(&[0x05, 0x34, 0x12]));
        assert_eq!(
            encode(&Value::Int(0x12345678)).unwrap(),
            stream(&[0x04, 0x78, 0x56, 0x34, 0x12])
        );
        assert_eq!(
            encode(&Value::Int(0x0102030405060708)).unwrap(),
            stream(&[0x03, 0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01])
        );
        assert_eq!(
            encode(&Value::Float(2.5)).unwrap(),
            stream(&[0x0A, 0, 0, 0, 0, 0, 0, 0x04, 0x40])
        );
        assert_eq!(encode(&Value::Float(0.0)).unwrap(), stream(&[0x0B]));
        // −0.0 has its own bit pattern and must NOT collapse to FLOAT0.
        assert_eq!(
            encode(&Value::Float(-0.0)).unwrap(),
            stream(&[0x0A, 0, 0, 0, 0, 0, 0, 0, 0x80])
        );
        assert_eq!(
            encode(&Value::Long(vec![0x2A])).unwrap(),
            stream(&[0x2F, 0x01, 0x2A])
        );
    }

    #[test]
    fn encodes_string_family_by_tag() {
        assert_eq!(encode(&Value::Bytes(vec![])).unwrap(), stream(&[0x0E]));
        assert_eq!(
            encode(&Value::Bytes(b"x".to_vec())).unwrap(),
            stream(&[0x0F, b'x'])
        );
        assert_eq!(
            encode(&Value::Bytes(b"foo".to_vec())).unwrap(),
            stream(&[0x13, 0x03, b'f', b'o', b'o'])
        );
        assert_eq!(
            encode(&Value::Str("abc".into())).unwrap(),
            stream(&[0x2E, 0x03, b'a', b'b', b'c'])
        );
        // Empty string: the tag picks the opcode (both exist in the corpus).
        assert_eq!(encode(&Value::Str(String::new())).unwrap(), stream(&[0x2E, 0x00]));
        assert_eq!(encode(&Value::StrUcs2(String::new())).unwrap(), stream(&[0x28]));
        assert_eq!(
            encode(&Value::StrUcs2("x".into())).unwrap(),
            stream(&[0x29, b'x', 0x00])
        );
        assert_eq!(
            encode(&Value::StrUcs2("hi".into())).unwrap(),
            stream(&[0x12, 0x02, b'h', 0x00, b'i', 0x00])
        );
        assert_eq!(encode(&Value::StrTable(7)).unwrap(), stream(&[0x11, 0x07]));
        assert_eq!(
            encode(&Value::StrTable(0)).unwrap_err().kind,
            EncodeErrorKind::BadTableIndex
        );
    }

    #[test]
    fn long_lengths_use_the_ff_escape_minimally() {
        // 254 bytes: single-byte count. 255 bytes: 0xFF + i32 LE.
        let b254 = Value::Bytes(vec![b'a'; 254]);
        let e254 = encode(&b254).unwrap();
        assert_eq!(&e254[5..7], &[0x13, 254]);
        let b255 = Value::Bytes(vec![b'a'; 255]);
        let e255 = encode(&b255).unwrap();
        assert_eq!(&e255[5..11], &[0x13, 0xFF, 255, 0, 0, 0]);
        assert_eq!(decode(&e254).unwrap(), b254);
        assert_eq!(decode(&e255).unwrap(), b255);
    }

    #[test]
    fn encodes_containers_and_dict_value_first() {
        assert_eq!(encode(&Value::Tuple(vec![])).unwrap(), stream(&[0x24]));
        assert_eq!(
            encode(&Value::Tuple(vec![Value::None])).unwrap(),
            stream(&[0x25, 0x01])
        );
        assert_eq!(
            encode(&Value::Tuple(vec![Value::Int(0), Value::Int(1)])).unwrap(),
            stream(&[0x2C, 0x08, 0x09])
        );
        assert_eq!(
            encode(&Value::Tuple(vec![Value::None, Value::None, Value::None])).unwrap(),
            stream(&[0x14, 0x03, 0x01, 0x01, 0x01])
        );
        assert_eq!(encode(&Value::List(vec![])).unwrap(), stream(&[0x26]));
        assert_eq!(
            encode(&Value::List(vec![Value::Int(1)])).unwrap(),
            stream(&[0x27, 0x09])
        );
        let d = Value::Dict(vec![(Value::Bytes(b"foo".to_vec()), Value::Int(0))]);
        assert_eq!(
            encode(&d).unwrap(),
            stream(&[0x16, 0x01, 0x08, 0x13, 0x03, b'f', b'o', b'o'])
        );
    }

    #[test]
    fn reencodes_shared_fixtures_byte_identically() {
        // The exact fixture streams from decode.rs's shared tests round-trip
        // decode → encode unchanged — non-identity tail map included.
        let shared_buffer: Vec<u8> = vec![
            0x7E, 0x01, 0x00, 0x00, 0x00, // header, shared_count = 1
            0x2C, // TUPLE2
            0x53, 0x02, b'h', b'i', // BUFFER|SHARED, len 2
            0x1B, 0x01, // REF -> slot 1
            0x01, 0x00, 0x00, 0x00, // tail map: slot 1
        ];
        let nonidentity_map: Vec<u8> = vec![
            0x7E, 0x02, 0x00, 0x00, 0x00, // header, shared_count = 2
            0x14, 0x03, // TUPLE, 3 elements
            0x65, // TUPLE1|SHARED -> map[0] = 2
            0x6F, 0x01, 0x2A, // LONG|SHARED, len 1 -> map[1] = 1
            0x1B, 0x01, // REF -> slot 1
            0x1B, 0x02, // REF -> slot 2
            0x02, 0x00, 0x00, 0x00, // tail map: entry 0 = slot 2
            0x01, 0x00, 0x00, 0x00, // tail map: entry 1 = slot 1
        ];
        let shared_instance: Vec<u8> = vec![
            0x7E, 0x01, 0x00, 0x00, 0x00,
            0x2C, // TUPLE2
            0x57, // INSTANCE|SHARED
            0x13, 0x05, b'M', b'.', b'C', b'l', b's', // class BUFFER
            0x09, // state ONE
            0x1B, 0x01, // REF -> slot 1
            0x01, 0x00, 0x00, 0x00,
        ];
        let shared_reduce: Vec<u8> = vec![
            0x7E, 0x01, 0x00, 0x00, 0x00,
            0x2C, // TUPLE2
            0x62, // REDUCE|SHARED
            0x2C, 0x02, 3, b'M', b'.', b'f', 0x24, // ctor (GLOBAL "M.f", ())
            0x2D, 0x2D, // empty iterator tail
            0x1B, 0x01, // REF -> slot 1
            0x01, 0x00, 0x00, 0x00,
        ];
        for data in [shared_buffer, nonidentity_map, shared_instance, shared_reduce] {
            let v = decode(&data).unwrap();
            assert_eq!(encode(&v).unwrap(), data);
        }
    }

    #[test]
    fn nested_stream_reencodes_recursively() {
        // STREAM (0x2B) payload is itself a complete stream holding ONE.
        let data = stream(&[0x2B, 0x06, 0x7E, 0x00, 0x00, 0x00, 0x00, 0x09]);
        let v = decode(&data).unwrap();
        assert_eq!(v, Value::Stream(Box::new(Value::Int(1))));
        assert_eq!(encode(&v).unwrap(), data);
    }

    #[test]
    fn encode_rejects_invalid_sharing() {
        // Flag on a node whose opcode does not store.
        let bad = Value::Shared { slot: 1, value: Box::new(Value::Int(5)) };
        assert_eq!(
            encode(&bad).unwrap_err().kind,
            EncodeErrorKind::NotStorable("int")
        );
        // Empty tuple emits TUPLE0, which does not store either.
        let bad_t0 = Value::Shared { slot: 1, value: Box::new(Value::Tuple(vec![])) };
        assert_eq!(
            encode(&bad_t0).unwrap_err().kind,
            EncodeErrorKind::NotStorable("tuple")
        );
        // ...while LIST0 does store (the reference asymmetry).
        let ok_l0 = Value::Tuple(vec![
            Value::Shared { slot: 1, value: Box::new(Value::List(vec![])) },
            Value::Ref(1),
        ]);
        assert_eq!(decode(&encode(&ok_l0).unwrap()).unwrap(), ok_l0);
        // Ref with no Shared anywhere.
        let dangling = Value::Tuple(vec![Value::Ref(1)]);
        assert_eq!(
            encode(&dangling).unwrap_err().kind,
            EncodeErrorKind::RefBeforeStore(1)
        );
        // Slot number beyond the shared count.
        let far = Value::Shared { slot: 3, value: Box::new(Value::List(vec![])) };
        assert_eq!(
            encode(&far).unwrap_err().kind,
            EncodeErrorKind::SlotOutOfRange { slot: 3, count: 1 }
        );
        // A self-referential Ref inside its own Shared subtree is rejected,
        // matching the decoder's store-on-completion semantics.
        let cyclic = Value::Shared {
            slot: 1,
            value: Box::new(Value::List(vec![Value::Ref(1)])),
        };
        assert_eq!(
            encode(&cyclic).unwrap_err().kind,
            EncodeErrorKind::RefBeforeStore(1)
        );
    }

    #[test]
    fn encode_depth_guard_matches_decode() {
        let mut v = Value::Tuple(vec![Value::None]);
        for _ in 0..200 {
            v = Value::Tuple(vec![v]);
        }
        assert_eq!(encode(&v).unwrap_err().kind, EncodeErrorKind::TooDeep);
    }
}
```

- [ ] **Step 5: Run the encoder tests**

Run: `cargo test -p blue-marshal encode`
Expected: all `encode.rs` tests PASS.

- [ ] **Step 6: Full suite green, then commit**

Run: `cargo test`
Expected: everything PASSes (corpus decode gate included).

```powershell
git add -A
git commit -m "Add blue-marshal encoder with corpus-canonical opcode selection"
```

---

### Task 4: Byte-identical corpus gate

The milestone gate: every one of the 5022 corpus files must survive decode → encode with output equal to the input, byte for byte.

**Files:**
- Modify: `crates/blue-marshal/tests/corpus.rs`

**Interfaces:**
- Consumes: `blue_marshal::decode`, `blue_marshal::encode` (Task 3), the existing `collect_dat_files` helper.
- Produces: the permanent regression gate `every_corpus_file_reencodes_byte_identically`.

- [ ] **Step 1: Add the round-trip gate test**

Append to `crates/blue-marshal/tests/corpus.rs`:

```rust
/// M1a gate: decode → encode must reproduce every corpus file byte-for-byte.
/// This is the strongest writer-correctness proof available without the game
/// client: any drift in opcode choice, length encoding, shared-slot order, or
/// tail-map content fails here with the first differing offset. If a future
/// client patch breaks a canonical rule, this is where it shows up.
#[test]
fn every_corpus_file_reencodes_byte_identically() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/corpus");
    let mut files = Vec::new();
    collect_dat_files(&corpus, &mut files);
    if files.is_empty() {
        eprintln!("corpus empty at {corpus:?} — skipping (run tools/sync-corpus.ps1)");
        return;
    }
    let mut failures = Vec::new();
    for f in &files {
        let data = fs::read(f).unwrap();
        let value = match blue_marshal::decode(&data) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!("{}: decode: {e}", f.display()));
                continue;
            }
        };
        match blue_marshal::encode(&value) {
            Err(e) => failures.push(format!("{}: encode: {e}", f.display())),
            Ok(out) if out != data => {
                let at = out
                    .iter()
                    .zip(data.iter())
                    .position(|(a, b)| a != b)
                    .unwrap_or_else(|| out.len().min(data.len()));
                failures.push(format!(
                    "{}: first byte diff at {:#x} (encoded {} bytes, original {} bytes)",
                    f.display(),
                    at,
                    out.len(),
                    data.len()
                ));
            }
            Ok(_) => {}
        }
    }
    assert!(
        failures.is_empty(),
        "{}/{} corpus files failed byte-identical re-encode:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
}
```

- [ ] **Step 2: Run the gate**

Run: `cargo test --test corpus`
Expected: both corpus tests PASS — `5022/5022` files round-trip. Several minutes in debug is normal. **If files fail:** do NOT weaken the gate or special-case files; each failure is a fidelity bug — take the first failing file's diff offset, inspect the bytes around it with `cargo run --bin bmdump -- dump <file>`, and fix the encoder/model. The measured facts table says exactly which canonical rule each byte pattern should follow.

- [ ] **Step 3: Commit**

```powershell
git add -A
git commit -m "Gate the corpus on byte-identical decode-encode round-trips"
```

---

### Task 5: Property round-trip test (std-only)

Spec §8 requires property-based tests on encode/decode. The crate takes no dependencies, so this uses a tiny deterministic xorshift PRNG.

**Files:**
- Create: `crates/blue-marshal/tests/prop_roundtrip.rs`

**Interfaces:**
- Consumes: `blue_marshal::{decode, encode, Value}`.
- Produces: nothing new — a standing randomized regression test.

- [ ] **Step 1: Write the property test**

Create `crates/blue-marshal/tests/prop_roundtrip.rs`:

```rust
//! Property-style round-trip test with a std-only PRNG (the crate takes no
//! dependencies, dev-dependencies included). Generates random fidelity-tagged
//! trees, encodes, decodes, and requires structural equality.
//!
//! Shared/Ref nodes are deliberately NOT generated: valid sharing has
//! stream-order constraints (a Ref must follow its completed Shared) that a
//! naive random generator would mostly violate; sharing is covered by the
//! encode.rs unit fixtures and by every real file in the corpus gate.

use blue_marshal::{decode, encode, Value};

/// xorshift64* — deterministic, seedable, no dependencies.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn gen_string(rng: &mut Rng, max_len: u64) -> String {
    // Mix of ASCII, a 2-byte UTF-8 char, a 3-byte char, and an astral
    // (surrogate-pair) char so UTF-8 byte counts and UTF-16 unit counts vary
    // independently.
    const ALPHABET: [char; 8] = ['a', 'Z', '0', ' ', '_', 'é', '☃', '🚀'];
    (0..rng.below(max_len))
        .map(|_| ALPHABET[rng.below(8) as usize])
        .collect()
}

fn gen_value(rng: &mut Rng, depth: u32) -> Value {
    // Containers only above depth 4 keeps trees small and far from MAX_DEPTH.
    let n_kinds = if depth >= 4 { 9 } else { 13 };
    match rng.below(n_kinds) {
        0 => Value::None,
        1 => Value::Bool(rng.below(2) == 0),
        // Arithmetic shift by 0..63 bits hits every canonical width bucket.
        2 => Value::Int((rng.next() as i64) >> rng.below(64)),
        3 => Value::Long((0..rng.below(6)).map(|_| rng.next() as u8).collect()),
        // Finite by construction; NaN is excluded because NaN != NaN would
        // fail the equality assert (bit-level float fidelity is proven by
        // the corpus gate instead).
        4 => Value::Float((rng.next() as i32 as f64) / 16.0),
        5 => Value::Bytes((0..rng.below(10)).map(|_| rng.next() as u8).collect()),
        6 => Value::Str(gen_string(rng, 10)),
        7 => Value::StrUcs2(gen_string(rng, 6)),
        8 => Value::StrTable((1 + rng.below(255)) as u8),
        9 => Value::Tuple((0..rng.below(4)).map(|_| gen_value(rng, depth + 1)).collect()),
        10 => Value::List((0..rng.below(4)).map(|_| gen_value(rng, depth + 1)).collect()),
        11 => Value::Dict(
            (0..rng.below(3))
                .map(|_| (gen_value(rng, depth + 1), gen_value(rng, depth + 1)))
                .collect(),
        ),
        _ => match rng.below(4) {
            0 => Value::Global(b"__builtin__.set".to_vec()),
            1 => Value::Stream(Box::new(gen_value(rng, depth + 1))),
            2 => Value::Instance {
                class: Box::new(Value::Bytes(b"utillib.KeyVal".to_vec())),
                state: Box::new(gen_value(rng, depth + 1)),
            },
            _ => Value::Reduce {
                ctor: Box::new(Value::Tuple(vec![
                    Value::Global(b"__builtin__.set".to_vec()),
                    Value::Tuple(vec![gen_value(rng, depth + 1)]),
                ])),
                items: (0..rng.below(2)).map(|_| gen_value(rng, depth + 1)).collect(),
                pairs: (0..rng.below(2))
                    .map(|_| (gen_value(rng, depth + 1), gen_value(rng, depth + 1)))
                    .collect(),
            },
        },
    }
}

#[test]
fn random_trees_roundtrip_through_encode_decode() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for case in 0..2000 {
        let v = gen_value(&mut rng, 0);
        let bytes =
            encode(&v).unwrap_or_else(|e| panic!("case {case}: encode failed: {e}\n{v:?}"));
        let back =
            decode(&bytes).unwrap_or_else(|e| panic!("case {case}: decode failed: {e}\n{v:?}"));
        assert_eq!(back, v, "case {case} round-trip mismatch");
    }
}
```

- [ ] **Step 2: Run it**

Run: `cargo test --test prop_roundtrip`
Expected: PASS (2000 cases, well under a second). If a case fails, the panic message contains the full generating `Value` — reproduce it as a named unit test in `encode.rs` before fixing.

- [ ] **Step 3: Commit**

```powershell
git add -A
git commit -m "Add std-only property round-trip test over random value trees"
```

---

### Task 6: Record the measurements and close out M1a

**Files:**
- Modify: `docs/format-notes.md`

**Interfaces:**
- Consumes: the Measured corpus facts table from this plan's header; Task 4's gate result.
- Produces: format-notes.md as the standing reference for M1b (which builds mutation + save logic on these guarantees).

- [ ] **Step 1: Add the measurements section to format-notes.md**

Insert a new section immediately before `## Mappings`:

```markdown
## Corpus canonicality measurements (2026-07-13, M1a)

Instrumented decoder run over all 5022 corpus files (0 decode failures),
taken before the encoder was designed; these facts justify the encoder's
canonical opcode rules and the fidelity tags on `Value`. The byte-identical
round-trip gate (tests/corpus.rs) re-proves all of them on every run, so a
future client patch that breaks one fails loudly there.

| Measurement | Count |
|---|---|
| READ_LENGTH 0xFF escapes / of which non-minimal (< 255) | 228,549 / **0** |
| Streams with slack between root object and tail map | **0** |
| Tail-map entries with slot ≠ encounter-index+1 | **963,660** |
| SHARED_FLAG on opcodes the reference ignores it for | **0** |
| STREAM (0x2B) / STRING (0x10) / STRINGL (0x0D) opcodes | **0** / **0** / **0** |
| BUFFER with payload ≤ 1 byte | **0** |
| Counted TUPLE with n ≤ 2 / counted LIST with n ≤ 1 | **0** / **0** |
| INTn holding a value a narrower opcode could hold | **0** |
| LONG with 0 payload bytes | **0** |
| FLOAT whose payload is +0.0 bits | **0** |
| UTF8 opcodes / with 0 bytes | 11,030,207 / 14,355 |
| UNICODE (0x12) / UNICODE0 / UNICODE1 opcodes | **0** / 1,458 / 15,480 |
| STRINGR opcodes | 71,847 |
| Non-STRINGR strings whose content equals a table entry | 1,269 |

Encoder consequences:

- **Canonical (no tag needed):** length encoding always minimal; `Int` by
  magnitude (constants, then narrowest INTn); `Float` +0.0 bits → FLOAT0;
  `Bytes` by length (STRING0/STRING1/BUFFER); `Tuple`/`List` by count.
- **Tagged (content cannot recover the opcode):** `Str` (UTF8) vs
  `StrUcs2` (UNICODE0/UNICODE1/UNICODE by UTF-16 unit count) vs
  `StrTable` (STRINGR index) — the client emits both UTF8("") and UNICODE0,
  and writes table-content strings as UTF8 too (1,269 collisions), so
  "look it up in the table" would mis-encode.
- **Explicit sharing:** tail maps are heavily non-identity, so decoded trees
  carry `Shared { slot }` wrappers and `Ref(slot)` nodes; the encoder
  replays slots in encounter order. SHARED_FLAG is never emitted on
  non-storing opcodes (and never observed there).
- **Slack promoted to hard error:** `ErrorKind::TrailingBytes` — measured
  zero occurrences, so any slack now means a mis-parse.
```

- [ ] **Step 2: Add the status bullet**

Append under `## Status` in `docs/format-notes.md`:

```markdown
- **2026-07-13 — M1a complete.** Native encoder shipped; corpus gate proves
  decode → encode reproduces all 5022 corpus files **byte-identically**
  (tests/corpus.rs `every_corpus_file_reencodes_byte_identically`). The
  `Value` model is fidelity-tagged (Str/StrUcs2/StrTable split, explicit
  Shared/Ref slots, Instance/Reduce split); see "Corpus canonicality
  measurements" below.
```

If Task 4's gate needed encoder fixes to pass, also record what they were (one bullet each) — anything the measurements table missed is exactly the kind of fact M1b must know about.

- [ ] **Step 3: Final verification and commit**

Run: `cargo test`
Expected: full suite green, including both corpus gates.

```powershell
git add -A
git commit -m "Record wire-canonicality measurements and close out M1a"
```

---

## Completion

After Task 6: use superpowers:finishing-a-development-branch. M1a is done when the whole suite is green with the corpus present, and the branch review confirms:

1. Corpus gate output shows 5022/5022 byte-identical round-trips.
2. `blue-marshal` still has zero dependencies (`crates/blue-marshal/Cargo.toml` `[dependencies]` and no `[dev-dependencies]`).
3. No personal data in any committed file.

Then M1b (app shell, load/save chain with backups, raw tree editor, CI packaging) gets its own plan, consuming `Value`, `decode`, `encode`, and `EncodeError` exactly as produced here.
