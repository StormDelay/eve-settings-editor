# M0 — Format Validation & Mapping Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Validate that the current EVE client's settings files are CCP blue-marshal, build a complete Rust decoder proven against a real-file corpus, and document exactly where window geometry, overview columns, autofill suggestions, and character/account names live.

**Architecture:** Cargo workspace with a single dependency-free library crate `blue-marshal` (decoder + `bmdump` CLI). Real settings files are copied from the live EVE directory into a gitignored corpus by a sync script; all tests and experiments run only on those copies. Format knowledge is verified against the vendored [ntt/reverence](https://github.com/ntt/reverence) C implementation (`src/blue/marshal.c` / `marshal.h`) and recorded in `docs/format-notes.md`.

**Tech Stack:** Rust stable (std only, no external crates), PowerShell (dev-machine sync script), reverence C source as read-only reference.

**Scope note:** This plan covers **Milestone 0 only** (spec §9). M1–M4 get their own plans after M0's findings, because the spec explicitly allows M0 to revise it.

## Global Constraints

- **Live-directory rule (spec §8):** tests, experiments, and all code in this plan never read from or write to `%LOCALAPPDATA%\CCP\EVE\…`. Sole exception: `tools/sync-corpus.ps1` **reads** it to copy files into `testdata/corpus/`. Nothing in M0 writes to the live directory.
- `testdata/corpus/` and `vendor/` are gitignored. Never commit real settings files. Committed docs must not contain character or account **names** (numeric IDs are acceptable).
- Commit messages: sentence-case summary line, matching existing repo style (`git log` shows the pattern). **No attribution trailers of any kind** (no `Co-Authored-By`, no "Generated with").
- Rust: stable toolchain updated via `rustup update stable` (Task 1), edition 2021. The `blue-marshal` crate uses **no external dependencies**.
- Decoding must be exact — no lossy conversions. In M0, constructs the decoder doesn't understand are **hard errors** carrying the byte offset (opaque-span preservation arrives with the M1 encoder).
- Some tasks are marked **[USER REQUIRED]** — they need the human to perform in-game actions or answer questions. The orchestrator pauses and coordinates with the user; do not dispatch those to a subagent.

## Verified format facts (do not re-derive)

From `marshal.h` of ntt/reverence (fetched 2026-07-12), cross-checked against a real `core_char_*.dat` hex dump:

- Stream: magic byte `0x7E`, then `u32` LE shared-object count, then the object stream. A shared-object index table sits at the **tail** of the stream (exact mechanics verified in Task 4).
- Length/count encoding: one byte; if `0xFF`, a `u32` LE follows (observed: `16 FF 76 02 00 00` = dict, 630 entries).
- Dict entries are stored **value first, then key** (observed: `16 0A`, then value `16 00` = empty dict, then key `13 0A "autoreload"`).
- Opcode byte carries flag `SHARED_FLAG = 0x40` (strip before dispatch; object is stored in the shared table).
- Opcodes (name = value — meaning):
  `NONE=0x01`, `GLOBAL=0x02` (type/class name ref), `INT64=0x03`, `INT32=0x04`, `INT16=0x05`, `INT8=0x06`, `MINUSONE=0x07`, `ZERO=0x08`, `ONE=0x09`, `FLOAT=0x0A` (f64), `FLOAT0=0x0B`, `STRINGL=0x0D`, `STRING0=0x0E`, `STRING1=0x0F`, `STRING=0x10`, `STRINGR=0x11` (global string-table ref), `UNICODE=0x12`, `BUFFER=0x13`, `TUPLE=0x14`, `LIST=0x15`, `DICT=0x16`, `INSTANCE=0x17`, `BLUE=0x18`, `CALLBACK=0x19`, `REF=0x1B` (shared-object ref), `CHECKSUM=0x1C`, `TRUE=0x1F`, `FALSE=0x20`, `PICKLER=0x21`, `REDUCE=0x22`, `NEWOBJ=0x23`, `TUPLE0=0x24`, `TUPLE1=0x25`, `LIST0=0x26`, `LIST1=0x27`, `UNICODE0=0x28`, `UNICODE1=0x29`, `DBROW=0x2A`, `STREAM=0x2B` (embedded marshal stream), `TUPLE2=0x2C`, `MARK=0x2D`, `UTF8=0x2E`, `LONG=0x2F` (big integer).

## File Structure

```
eve-settings-editor/
├── Cargo.toml                          # workspace root
├── crates/blue-marshal/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs                      # public API re-exports
│   │   ├── error.rs                    # DecodeError { offset, kind }
│   │   ├── reader.rs                   # byte cursor with offset tracking
│   │   ├── value.rs                    # Value enum + canonical text dump
│   │   ├── opcodes.rs                  # opcode constants (table above)
│   │   ├── decode.rs                   # header, shared table, dispatch
│   │   ├── string_table.rs             # STRINGR table extracted from reverence (Task 4)
│   │   └── bin/bmdump.rs               # CLI: dump <file> | scan <dir>
│   └── tests/corpus.rs                 # decode every corpus file (skips if corpus absent)
├── tools/sync-corpus.ps1               # live dir -> testdata/corpus/<stamp>_<label>/
├── testdata/corpus/                    # gitignored, real settings snapshots
├── vendor/reverence/                   # gitignored, read-only reference clone
└── docs/format-notes.md                # living format/mapping documentation
```

---

### Task 1: Workspace scaffold

**Files:**
- Create: `Cargo.toml`, `crates/blue-marshal/Cargo.toml`, `crates/blue-marshal/src/lib.rs`
- Modify: `.gitignore`

**Interfaces:**
- Consumes: nothing (first code task).
- Produces: compiling workspace; crate name `blue-marshal` (lib target `blue_marshal`) that later tasks add modules to.

- [ ] **Step 1: Update the Rust toolchain**

Run: `rustup update stable`
Expected: ends with `stable-x86_64-pc-windows-msvc updated` (or `unchanged`); `cargo --version` reports ≥ 1.78.

- [ ] **Step 2: Create the workspace**

`Cargo.toml` (repo root):

```toml
[workspace]
resolver = "2"
members = ["crates/blue-marshal"]
```

`crates/blue-marshal/Cargo.toml`:

```toml
[package]
name = "blue-marshal"
version = "0.1.0"
edition = "2021"

[dependencies]
```

`crates/blue-marshal/src/lib.rs`:

```rust
//! Decoder for CCP's "blue marshal" serialization used by EVE Online
//! settings files. Reference: ntt/reverence src/blue/marshal.{h,c}.

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_builds() {}
}
```

- [ ] **Step 3: Add vendor/ to .gitignore**

Append to `.gitignore`:

```
# Read-only reference clone of ntt/reverence (Task 4)
vendor/
```

- [ ] **Step 4: Verify build and test**

Run: `cargo test`
Expected: `test tests::workspace_builds ... ok`, `1 passed`.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates .gitignore
git commit -m "Scaffold cargo workspace with blue-marshal crate"
```

---

### Task 2: Corpus sync script + historical snapshot

**Files:**
- Create: `tools/sync-corpus.ps1`

**Interfaces:**
- Consumes: live EVE settings directory (read-only — the one sanctioned read).
- Produces: `testdata/corpus/<UTCstamp>_<label>/<profile-dir>/<settings-dir>/core_*.dat` layout that Tasks 8–11 iterate over.

- [ ] **Step 1: Write the sync script**

`tools/sync-corpus.ps1`:

```powershell
# Copies EVE settings files from the live directory into testdata/corpus/.
# This is the ONLY code in the project allowed to touch the live directory,
# and it only ever reads from it (spec section 8).
param(
    [Parameter(Mandatory = $true)][string]$Label,
    [string]$Source = "$env:LOCALAPPDATA\CCP\EVE"
)
$stamp = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHHmmssZ")
$destRoot = Join-Path $PSScriptRoot "..\testdata\corpus\${stamp}_$Label"

$files = Get-ChildItem -Path $Source -Directory |
    ForEach-Object { Get-ChildItem $_.FullName -Directory -Filter "settings_*" -ErrorAction SilentlyContinue } |
    ForEach-Object { Get-ChildItem $_.FullName -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^core_(char|user|public)_.*\.(dat|yaml)$' -or $_.Name -eq 'prefs.ini' } }

if (-not $files) { Write-Error "No settings files found under $Source"; exit 1 }

foreach ($f in $files) {
    # <profile>/<settings folder>/<file>, e.g. c_eve_sharedcache_tq_tranquility/settings_Default/core_char_123.dat
    $settingsDir = $f.Directory
    $profileDir = $settingsDir.Parent
    $dest = Join-Path $destRoot (Join-Path $profileDir.Name $settingsDir.Name)
    New-Item -ItemType Directory -Force $dest | Out-Null
    Copy-Item $f.FullName -Destination $dest
}
$count = ($files | Measure-Object).Count
Write-Output "Copied $count files to $destRoot"
```

- [ ] **Step 2: Run it for the historical snapshot**

Run: `powershell -File tools\sync-corpus.ps1 -Label historical`
Expected: `Copied N files to ...testdata\corpus\<stamp>_historical` with N ≥ 50 (the machine has ~50 char/user files from 2020–2022 plus sisi/other profiles).

- [ ] **Step 3: Verify git ignores the corpus**

Run: `git status --short`
Expected: only `tools/sync-corpus.ps1` appears. Nothing under `testdata/` is listed (it is gitignored). If any corpus file appears, STOP and fix `.gitignore` before committing anything.

- [ ] **Step 4: Commit**

```bash
git add tools/sync-corpus.ps1
git commit -m "Add corpus sync script and document snapshot layout"
```

---

### Task 3: [USER REQUIRED] Fresh files from the current client + format sanity check

**Files:**
- Create: `docs/format-notes.md`

**Interfaces:**
- Consumes: `tools/sync-corpus.ps1` (Task 2).
- Produces: a `<stamp>_fresh-baseline` corpus snapshot from the 2026 client; `docs/format-notes.md` that Tasks 4, 8–12 append to. **This task is the spec's format-drift gate** — if fresh files are not blue-marshal, STOP and revise the spec before continuing.

- [ ] **Step 1: Ask the user to generate fresh settings**

Ask the user to: launch the current EVE client, log in with one character they can name to us later (Task 11), optionally move any window slightly (forces a settings write), then quit the client fully.

- [ ] **Step 2: Snapshot**

Run: `powershell -File tools\sync-corpus.ps1 -Label fresh-baseline`
Expected: `Copied N files ...`. Then verify recency:

Run: `powershell -Command "Get-ChildItem testdata\corpus\*fresh-baseline* -Recurse -Filter core_*.dat | Sort-Object LastWriteTime -Descending | Select-Object -First 5 Name, LastWriteTime"`
Expected: at least one `core_char_*.dat` and one `core_user_*.dat` with today's date.

- [ ] **Step 3: Magic-byte check on the fresh files**

Run:

```powershell
Get-ChildItem testdata\corpus\*fresh-baseline* -Recurse -Filter core_*.dat |
  Where-Object { $_.LastWriteTime -gt (Get-Date).AddDays(-1) } |
  ForEach-Object { $b = Get-Content $_.FullName -Encoding Byte -TotalCount 1; "{0}: 0x{1:X2}" -f $_.Name, $b[0] }
```

Expected: every line ends `0x7E`. **If not**, the current client changed formats: STOP, record findings in `docs/format-notes.md`, and report to the user — the spec's §2 risk fired and M0 continues as pure investigation of the new format.

- [ ] **Step 4: Start docs/format-notes.md**

```markdown
# EVE settings file format notes

Living document. Sources: ntt/reverence (src/blue/marshal.{h,c}), our own
corpus diffing. No character/account names in this file — numeric IDs only.

## Status
- 2026-07-12: fresh files from current Tranquility client confirmed
  blue-marshal (magic 0x7E). Historical (2020-2022) and fresh snapshots
  both in corpus.

## Opcode table (from reverence marshal.h)
(copied from the plan's "Verified format facts" — extend with encoding
details per opcode as they are verified in Tasks 4 and 9)

## Mappings (filled by Tasks 10-11)
```

- [ ] **Step 5: Commit**

```bash
git add docs/format-notes.md
git commit -m "Confirm current client still writes blue-marshal settings"
```

---

### Task 4: Vendor reverence and verify encoding details

**Files:**
- Create: `vendor/reverence/` (clone, gitignored), `crates/blue-marshal/src/string_table.rs`
- Modify: `docs/format-notes.md`

**Interfaces:**
- Consumes: opcode table from the plan header.
- Produces: verified per-opcode encoding rules in `docs/format-notes.md` (Tasks 7/9 implement from them); `string_table.rs` exposing `pub static STRING_TABLE: &[&str]` used by the `STRINGR` opcode; confirmed shared-map mechanics used in Task 7.

- [ ] **Step 1: Clone the reference**

Run: `git clone --depth 1 https://github.com/ntt/reverence vendor/reverence`
Expected: clone succeeds; `vendor/reverence/src/blue/marshal.h` and `marshal.c` exist. Run `git status --short` — vendor/ must NOT appear.

- [ ] **Step 2: Verify the opcode table**

Open `vendor/reverence/src/blue/marshal.h`. Compare every constant against the plan's "Verified format facts" table. Fix any discrepancy in the plan table's copy inside `docs/format-notes.md` (the plan header stays as-written; format-notes.md is the living truth).

- [ ] **Step 3: Extract and document per-opcode encodings from marshal.c**

For each opcode, read its `case` in `marshal.c`'s load loop and record in `docs/format-notes.md`: payload layout (endianness, length encoding, element order). Non-negotiable items to pin down exactly:

1. **Shared-object mechanics:** where the shared index table lives (expected: `shared_count * 4` bytes at stream tail), how `SHARED_FLAG`-marked objects populate it (store order vs. mapped slot), and what `REF`'s payload indexes (0- or 1-based).
2. **`LONG` (0x2F):** length encoding, byte order, sign handling.
3. **`UNICODE*` (0x12/0x28/0x29):** length in chars vs bytes; UTF-16LE confirmation.
4. **`STRINGL` vs `STRING` vs `BUFFER`:** exact differences.
5. **`STREAM` (0x2B):** embedded-stream framing (settings values are likely nested streams).
6. **`CHECKSUM` (0x1C):** adler32 coverage range (needed later by the M1 encoder).
7. **`INSTANCE`/`GLOBAL`/`NEWOBJ`/`REDUCE`:** payload shape (record even if we hope not to need them).

- [ ] **Step 4: Extract the STRINGR string table**

Locate the built-in string table in the vendor tree (search: `grep -rn "stringTable\|string_table" vendor/reverence/src/`). Convert it to `crates/blue-marshal/src/string_table.rs`:

```rust
//! Fixed global string table referenced by opcode STRINGR (0x11).
//! Extracted verbatim from ntt/reverence (BSD-licensed); order matters.
pub static STRING_TABLE: &[&str] = &[
    // ... entries copied in original order from reverence ...
];
```

Record in format-notes.md whether STRINGR indexes are 0- or 1-based (per marshal.c).

- [ ] **Step 5: Commit**

```bash
git add docs/format-notes.md crates/blue-marshal/src/string_table.rs
git commit -m "Document verified opcode encodings and vendor string table"
```

(`string_table.rs` is not yet referenced by lib.rs — that lands with the decoder in Task 7.)

---

### Task 5: Reader primitives (TDD)

**Files:**
- Create: `crates/blue-marshal/src/error.rs`, `crates/blue-marshal/src/reader.rs`
- Modify: `crates/blue-marshal/src/lib.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `Reader<'a>` with `new(&[u8])`, `pos() -> usize`, `remaining() -> usize`, `read_u8/read_u16/read_u32/read_i64/read_f64`, `read_bytes(n) -> &'a [u8]`, `read_len() -> usize` (blue length encoding), all returning `Result<_, DecodeError>`; `DecodeError { offset: usize, kind: ErrorKind }` with `ErrorKind::{BadMagic(u8), UnexpectedEof, UnknownOpcode(u8), UnknownFlags(u8), BadRef(usize), BadStringRef(usize), BadUtf8, Unsupported(&'static str)}`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/blue-marshal/src/reader.rs` (tests first — the impl in Step 3 goes above them in the same file):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    #[test]
    fn reads_scalars_little_endian() {
        let data = [0x2A, 0x01, 0x02, 0xEF, 0xBE, 0xAD, 0xDE];
        let mut r = Reader::new(&data);
        assert_eq!(r.read_u8().unwrap(), 0x2A);
        assert_eq!(r.read_u16().unwrap(), 0x0201);
        assert_eq!(r.read_u32().unwrap(), 0xDEADBEEF);
        assert_eq!(r.pos(), 7);
        assert_eq!(r.remaining(), 0);
    }

    #[test]
    fn read_len_single_byte_and_extended() {
        let mut r = Reader::new(&[0x0A]);
        assert_eq!(r.read_len().unwrap(), 10);
        // 0xFF escape -> u32 LE follows (observed in real files: 16 FF 76 02 00 00)
        let mut r = Reader::new(&[0xFF, 0x76, 0x02, 0x00, 0x00]);
        assert_eq!(r.read_len().unwrap(), 0x0276);
    }

    #[test]
    fn eof_error_carries_offset() {
        let mut r = Reader::new(&[0x01]);
        r.read_u8().unwrap();
        let err = r.read_u32().unwrap_err();
        assert_eq!(err.offset, 1);
        assert_eq!(err.kind, ErrorKind::UnexpectedEof);
    }

    #[test]
    fn read_bytes_slices_without_copy() {
        let data = [1, 2, 3, 4];
        let mut r = Reader::new(&data);
        assert_eq!(r.read_bytes(3).unwrap(), &[1, 2, 3]);
        assert!(r.read_bytes(2).is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail to compile**

Run: `cargo test -p blue-marshal`
Expected: compile error — `Reader`, `error` module not found.

- [ ] **Step 3: Implement**

`crates/blue-marshal/src/error.rs`:

```rust
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    /// Byte offset in the input where decoding failed.
    pub offset: usize,
    pub kind: ErrorKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    BadMagic(u8),
    UnexpectedEof,
    UnknownOpcode(u8),
    UnknownFlags(u8),
    BadRef(usize),
    BadStringRef(usize),
    BadUtf8,
    Unsupported(&'static str),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "decode error at offset {:#x}: {:?}", self.offset, self.kind)
    }
}

impl std::error::Error for DecodeError {}
```

`crates/blue-marshal/src/reader.rs` (above the tests from Step 1):

```rust
use crate::error::{DecodeError, ErrorKind};

pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn err(&self, kind: ErrorKind) -> DecodeError {
        DecodeError { offset: self.pos, kind }
    }

    pub fn read_bytes(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        if self.remaining() < n {
            return Err(self.err(ErrorKind::UnexpectedEof));
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn read_u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.read_bytes(1)?[0])
    }

    pub fn read_u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_le_bytes(self.read_bytes(2)?.try_into().unwrap()))
    }

    pub fn read_u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.read_bytes(4)?.try_into().unwrap()))
    }

    pub fn read_i64(&mut self) -> Result<i64, DecodeError> {
        Ok(i64::from_le_bytes(self.read_bytes(8)?.try_into().unwrap()))
    }

    pub fn read_f64(&mut self) -> Result<f64, DecodeError> {
        Ok(f64::from_le_bytes(self.read_bytes(8)?.try_into().unwrap()))
    }

    /// Blue length encoding: one byte, or 0xFF followed by u32 LE.
    pub fn read_len(&mut self) -> Result<usize, DecodeError> {
        let b = self.read_u8()?;
        if b == 0xFF {
            Ok(self.read_u32()? as usize)
        } else {
            Ok(b as usize)
        }
    }
}
```

`crates/blue-marshal/src/lib.rs` — replace contents:

```rust
//! Decoder for CCP's "blue marshal" serialization used by EVE Online
//! settings files. Reference: ntt/reverence src/blue/marshal.{h,c}.

pub mod error;
pub mod reader;

pub use error::{DecodeError, ErrorKind};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p blue-marshal`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/blue-marshal/src
git commit -m "Add byte reader with blue length encoding and offset-carrying errors"
```

---

### Task 6: Value model and canonical text dump (TDD)

**Files:**
- Create: `crates/blue-marshal/src/value.rs`
- Modify: `crates/blue-marshal/src/lib.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:

```rust
pub enum Value {
    None,
    Bool(bool),
    Int(i64),
    Long(Vec<u8>),            // LONG magnitude bytes, LE, sign per Task 4 findings
    Float(f64),
    Bytes(Vec<u8>),           // STRING*/BUFFER (raw, may be non-UTF8)
    Str(String),              // UNICODE*/UTF8/STRINGR (decoded text)
    Tuple(Vec<Value>),
    List(Vec<Value>),
    Dict(Vec<(Value, Value)>), // (key, value), file order preserved
    Stream(Box<Value>),        // embedded marshal stream, recursively decoded
}
pub fn dump_text(v: &Value) -> String  // deterministic, dict keys sorted
```

Later tasks rely on: `Dict` stores `(key, value)` pairs (wire order is value-then-key, decode normalizes); `dump_text` output is stable across runs so `git diff --no-index` on dumps is the corpus-diff tool.

- [ ] **Step 1: Write the failing tests**

`crates/blue-marshal/src/value.rs` (tests at bottom; impl from Step 3 goes above):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_scalars() {
        assert_eq!(dump_text(&Value::None), "None");
        assert_eq!(dump_text(&Value::Bool(true)), "True");
        assert_eq!(dump_text(&Value::Int(-7)), "-7");
        assert_eq!(dump_text(&Value::Float(2.5)), "2.5");
        assert_eq!(dump_text(&Value::Str("abc".into())), "\"abc\"");
    }

    #[test]
    fn dump_bytes_printable_and_hex() {
        assert_eq!(dump_text(&Value::Bytes(b"overview".to_vec())), "b\"overview\"");
        assert_eq!(dump_text(&Value::Bytes(vec![0x00, 0xFF])), "hex:00ff");
    }

    #[test]
    fn dump_dict_sorted_by_key_rendering() {
        let d = Value::Dict(vec![
            (Value::Bytes(b"zeta".to_vec()), Value::Int(1)),
            (Value::Bytes(b"alpha".to_vec()), Value::Int(2)),
        ]);
        let text = dump_text(&d);
        let alpha = text.find("alpha").unwrap();
        let zeta = text.find("zeta").unwrap();
        assert!(alpha < zeta, "dict dump must sort keys for stable diffs");
    }

    #[test]
    fn dump_nested_containers_indent() {
        let v = Value::Tuple(vec![Value::Int(1), Value::List(vec![Value::None])]);
        assert_eq!(dump_text(&v), "(\n  1\n  [\n    None\n  ]\n)");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail to compile**

Run: `cargo test -p blue-marshal`
Expected: compile error — `Value`, `dump_text` not found.

- [ ] **Step 3: Implement**

Top of `crates/blue-marshal/src/value.rs`:

```rust
use std::fmt::Write;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    None,
    Bool(bool),
    Int(i64),
    Long(Vec<u8>),
    Float(f64),
    Bytes(Vec<u8>),
    Str(String),
    Tuple(Vec<Value>),
    List(Vec<Value>),
    Dict(Vec<(Value, Value)>),
    Stream(Box<Value>),
}

/// Deterministic text rendering. Dict keys are sorted by their rendered
/// form so that two dumps of semantically equal data diff cleanly.
pub fn dump_text(v: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, v, 0);
    out
}

fn write_value(out: &mut String, v: &Value, indent: usize) {
    let pad = "  ".repeat(indent);
    match v {
        Value::None => out.push_str("None"),
        Value::Bool(true) => out.push_str("True"),
        Value::Bool(false) => out.push_str("False"),
        Value::Int(i) => {
            let _ = write!(out, "{i}");
        }
        Value::Long(bytes) => {
            // Interpret as unsigned LE when it fits; adjust after Task 4
            // pins down sign semantics.
            if bytes.len() <= 16 {
                let mut buf = [0u8; 16];
                buf[..bytes.len()].copy_from_slice(bytes);
                let _ = write!(out, "{}L", u128::from_le_bytes(buf));
            } else {
                let _ = write!(out, "longhex:{}", hex(bytes));
            }
        }
        Value::Float(f) => {
            let _ = write!(out, "{f:?}");
        }
        Value::Str(s) => {
            let _ = write!(out, "{s:?}");
        }
        Value::Bytes(b) => {
            if !b.is_empty() && b.iter().all(|c| (0x20..0x7F).contains(c)) {
                let _ = write!(out, "b\"{}\"", String::from_utf8_lossy(b));
            } else if b.is_empty() {
                out.push_str("b\"\"");
            } else {
                let _ = write!(out, "hex:{}", hex(b));
            }
        }
        Value::Tuple(items) => write_seq(out, items, indent, '(', ')'),
        Value::List(items) => write_seq(out, items, indent, '[', ']'),
        Value::Dict(entries) => {
            if entries.is_empty() {
                out.push_str("{}");
                return;
            }
            let mut rendered: Vec<(String, &Value)> = entries
                .iter()
                .map(|(k, v)| (dump_text(k), v))
                .collect();
            rendered.sort_by(|a, b| a.0.cmp(&b.0));
            out.push_str("{\n");
            for (key, val) in rendered {
                let _ = write!(out, "{pad}  {key}: ");
                write_value(out, val, indent + 1);
                out.push('\n');
            }
            let _ = write!(out, "{pad}}}");
        }
        Value::Stream(inner) => {
            out.push_str("stream:");
            write_value(out, inner, indent);
        }
    }
}

fn write_seq(out: &mut String, items: &[Value], indent: usize, open: char, close: char) {
    let pad = "  ".repeat(indent);
    if items.is_empty() {
        out.push(open);
        out.push(close);
        return;
    }
    out.push(open);
    out.push('\n');
    for item in items {
        out.push_str(&pad);
        out.push_str("  ");
        write_value(out, item, indent + 1);
        out.push('\n');
    }
    out.push_str(&pad);
    out.push(close);
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
```

Add to `lib.rs`:

```rust
pub mod value;
pub use value::{dump_text, Value};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p blue-marshal`
Expected: 8 passed (4 from Task 5 + 4 new).

- [ ] **Step 5: Commit**

```bash
git add crates/blue-marshal/src
git commit -m "Add value model with deterministic text dump"
```

---

### Task 7: Decoder core (TDD)

**Files:**
- Create: `crates/blue-marshal/src/opcodes.rs`, `crates/blue-marshal/src/decode.rs`
- Modify: `crates/blue-marshal/src/lib.rs`

**Interfaces:**
- Consumes: `Reader` (Task 5), `Value` (Task 6), encoding rules in `docs/format-notes.md` (Task 4), `string_table::STRING_TABLE` (Task 4).
- Produces: `pub fn decode(data: &[u8]) -> Result<Value, DecodeError>` — the single entry point every later task and the M1 encoder round-trip use.

- [ ] **Step 1: Write the failing tests**

Tests in `crates/blue-marshal/src/decode.rs`. Synthetic buffers use shared-count 0 so they don't depend on shared-map details; those are exercised on real files in Tasks 8–9.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    // header: magic 0x7E + u32 shared-count 0
    fn stream(body: &[u8]) -> Vec<u8> {
        let mut v = vec![0x7E, 0, 0, 0, 0];
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn rejects_bad_magic() {
        let err = decode(&[0x00, 0, 0, 0, 0, 0x01]).unwrap_err();
        assert_eq!(err.kind, crate::ErrorKind::BadMagic(0x00));
    }

    #[test]
    fn decodes_scalar_opcodes() {
        assert_eq!(decode(&stream(&[0x01])).unwrap(), Value::None);
        assert_eq!(decode(&stream(&[0x1F])).unwrap(), Value::Bool(true));
        assert_eq!(decode(&stream(&[0x20])).unwrap(), Value::Bool(false));
        assert_eq!(decode(&stream(&[0x07])).unwrap(), Value::Int(-1));
        assert_eq!(decode(&stream(&[0x08])).unwrap(), Value::Int(0));
        assert_eq!(decode(&stream(&[0x09])).unwrap(), Value::Int(1));
        assert_eq!(decode(&stream(&[0x06, 0x2A])).unwrap(), Value::Int(42));
        assert_eq!(decode(&stream(&[0x05, 0x34, 0x12])).unwrap(), Value::Int(0x1234));
        assert_eq!(
            decode(&stream(&[0x04, 0x78, 0x56, 0x34, 0x12])).unwrap(),
            Value::Int(0x12345678)
        );
        assert_eq!(
            decode(&stream(&[0x0A, 0, 0, 0, 0, 0, 0, 0x04, 0x40])).unwrap(),
            Value::Float(2.5)
        );
        assert_eq!(decode(&stream(&[0x0B])).unwrap(), Value::Float(0.0));
    }

    #[test]
    fn decodes_buffer_and_short_strings() {
        // BUFFER 0x13, len 3, "foo" — observed encoding in real files
        assert_eq!(
            decode(&stream(&[0x13, 0x03, b'f', b'o', b'o'])).unwrap(),
            Value::Bytes(b"foo".to_vec())
        );
        assert_eq!(decode(&stream(&[0x0E])).unwrap(), Value::Bytes(vec![])); // STRING0
        assert_eq!(
            decode(&stream(&[0x0F, b'x'])).unwrap(),
            Value::Bytes(b"x".to_vec())
        ); // STRING1
    }

    #[test]
    fn decodes_containers_dict_wire_order_value_then_key() {
        // TUPLE2(ZERO, ONE)
        assert_eq!(
            decode(&stream(&[0x2C, 0x08, 0x09])).unwrap(),
            Value::Tuple(vec![Value::Int(0), Value::Int(1)])
        );
        // TUPLE0 / TUPLE1 / LIST0 / LIST1
        assert_eq!(decode(&stream(&[0x24])).unwrap(), Value::Tuple(vec![]));
        assert_eq!(
            decode(&stream(&[0x25, 0x01])).unwrap(),
            Value::Tuple(vec![Value::None])
        );
        assert_eq!(decode(&stream(&[0x26])).unwrap(), Value::List(vec![]));
        assert_eq!(
            decode(&stream(&[0x27, 0x09])).unwrap(),
            Value::List(vec![Value::Int(1)])
        );
        // DICT len 1: value ZERO, then key BUFFER "foo" (wire order observed
        // in real core_char files) -> normalized to (key, value)
        let d = decode(&stream(&[0x16, 0x01, 0x08, 0x13, 0x03, b'f', b'o', b'o'])).unwrap();
        assert_eq!(
            d,
            Value::Dict(vec![(Value::Bytes(b"foo".to_vec()), Value::Int(0))])
        );
    }

    #[test]
    fn unknown_opcode_reports_offset() {
        let err = decode(&stream(&[0x3D])).unwrap_err();
        assert_eq!(err.kind, crate::ErrorKind::UnknownOpcode(0x3D));
        assert_eq!(err.offset, 5); // right after the 5-byte header
    }
}
```

- [ ] **Step 2: Run tests to verify they fail to compile**

Run: `cargo test -p blue-marshal`
Expected: compile error — `decode` not found.

- [ ] **Step 3: Implement opcodes.rs and decode.rs**

`crates/blue-marshal/src/opcodes.rs`: constants exactly as listed in "Verified format facts" (`pub const NONE: u8 = 0x01;` … `pub const LONG: u8 = 0x2F;` plus `pub const PROTOCOL: u8 = 0x7E;` and `pub const SHARED_FLAG: u8 = 0x40;`).

`crates/blue-marshal/src/decode.rs`:

```rust
use crate::error::{DecodeError, ErrorKind};
use crate::opcodes as op;
use crate::reader::Reader;
use crate::string_table::STRING_TABLE;
use crate::value::Value;

pub fn decode(data: &[u8]) -> Result<Value, DecodeError> {
    let mut r = Reader::new(data);
    let magic = r.read_u8()?;
    if magic != op::PROTOCOL {
        return Err(DecodeError { offset: 0, kind: ErrorKind::BadMagic(magic) });
    }
    let shared_count = r.read_u32()? as usize;
    // Shared-object index map: shared_count u32 slots at the stream tail.
    // VERIFY in Task 4 notes: exact population rule from marshal.c. The
    // implementation below stores shared objects in order of completion at
    // the slot given by the tail map; adjust if marshal.c differs.
    let mut dec = Decoder {
        shared: vec![None; shared_count],
        shared_map_consumed: 0,
        data,
    };
    dec.load(&mut r)
}

struct Decoder<'a> {
    shared: Vec<Option<Value>>,
    shared_map_consumed: usize,
    data: &'a [u8],
}

impl<'a> Decoder<'a> {
    fn shared_slot(&mut self) -> usize {
        // Tail map entry for the next shared object (1-based slot indices
        // per reverence; VERIFY in Task 4).
        let n = self.shared.len();
        let tail = self.data.len() - n * 4 + self.shared_map_consumed * 4;
        self.shared_map_consumed += 1;
        let idx = u32::from_le_bytes(self.data[tail..tail + 4].try_into().unwrap());
        idx as usize
    }

    fn load(&mut self, r: &mut Reader<'a>) -> Result<Value, DecodeError> {
        let raw = r.read_u8()?;
        let opcode_offset = r.pos() - 1;
        let shared = raw & op::SHARED_FLAG != 0;
        let code = raw & !op::SHARED_FLAG;
        let value = self.load_op(code, r, opcode_offset)?;
        if shared {
            let slot = self.shared_slot();
            if slot == 0 || slot > self.shared.len() {
                return Err(DecodeError { offset: opcode_offset, kind: ErrorKind::BadRef(slot) });
            }
            self.shared[slot - 1] = Some(value.clone());
        }
        Ok(value)
    }

    fn load_op(&mut self, code: u8, r: &mut Reader<'a>, at: usize) -> Result<Value, DecodeError> {
        Ok(match code {
            op::NONE => Value::None,
            op::TRUE => Value::Bool(true),
            op::FALSE => Value::Bool(false),
            op::MINUSONE => Value::Int(-1),
            op::ZERO => Value::Int(0),
            op::ONE => Value::Int(1),
            op::INT8 => Value::Int(r.read_u8()? as i8 as i64),
            op::INT16 => Value::Int(r.read_u16()? as i16 as i64),
            op::INT32 => Value::Int(r.read_u32()? as i32 as i64),
            op::INT64 => Value::Int(r.read_i64()?),
            op::FLOAT => Value::Float(r.read_f64()?),
            op::FLOAT0 => Value::Float(0.0),
            op::LONG => {
                let n = r.read_len()?;
                Value::Long(r.read_bytes(n)?.to_vec())
            }
            op::STRING0 => Value::Bytes(vec![]),
            op::STRING1 => Value::Bytes(r.read_bytes(1)?.to_vec()),
            op::STRING | op::STRINGL | op::BUFFER => {
                let n = r.read_len()?;
                Value::Bytes(r.read_bytes(n)?.to_vec())
            }
            op::STRINGR => {
                let idx = r.read_len()?;
                // Index base (0 or 1) per Task 4 findings; 1-based shown.
                let s = STRING_TABLE
                    .get(idx.wrapping_sub(1))
                    .ok_or(DecodeError { offset: at, kind: ErrorKind::BadStringRef(idx) })?;
                Value::Str((*s).to_string())
            }
            op::UNICODE0 => Value::Str(String::new()),
            op::UNICODE1 | op::UNICODE => {
                let chars = if code == op::UNICODE1 { 1 } else { r.read_len()? };
                let bytes = r.read_bytes(chars * 2)?;
                let units: Vec<u16> = bytes
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                String::from_utf16(&units)
                    .map(Value::Str)
                    .map_err(|_| DecodeError { offset: at, kind: ErrorKind::BadUtf8 })?
            }
            op::UTF8 => {
                let n = r.read_len()?;
                let bytes = r.read_bytes(n)?;
                std::str::from_utf8(bytes)
                    .map(|s| Value::Str(s.to_string()))
                    .map_err(|_| DecodeError { offset: at, kind: ErrorKind::BadUtf8 })?
            }
            op::TUPLE0 => Value::Tuple(vec![]),
            op::TUPLE1 => Value::Tuple(vec![self.load(r)?]),
            op::TUPLE2 => {
                let a = self.load(r)?;
                let b = self.load(r)?;
                Value::Tuple(vec![a, b])
            }
            op::TUPLE => {
                let n = r.read_len()?;
                let mut items = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    items.push(self.load(r)?);
                }
                Value::Tuple(items)
            }
            op::LIST0 => Value::List(vec![]),
            op::LIST1 => Value::List(vec![self.load(r)?]),
            op::LIST => {
                let n = r.read_len()?;
                let mut items = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    items.push(self.load(r)?);
                }
                Value::List(items)
            }
            op::DICT => {
                let n = r.read_len()?;
                let mut entries = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    let value = self.load(r)?; // wire order: value first
                    let key = self.load(r)?;
                    entries.push((key, value));
                }
                Value::Dict(entries)
            }
            op::REF => {
                let idx = r.read_len()?;
                self.shared
                    .get(idx.wrapping_sub(1))
                    .and_then(|s| s.clone())
                    .ok_or(DecodeError { offset: at, kind: ErrorKind::BadRef(idx) })?
            }
            op::STREAM => {
                let n = r.read_len()?;
                let bytes = r.read_bytes(n)?;
                Value::Stream(Box::new(decode(bytes)?))
            }
            // Implemented in Task 9 if the corpus needs them:
            op::GLOBAL => return Err(unsupported(at, "GLOBAL")),
            op::INSTANCE => return Err(unsupported(at, "INSTANCE")),
            op::BLUE => return Err(unsupported(at, "BLUE")),
            op::CALLBACK => return Err(unsupported(at, "CALLBACK")),
            op::CHECKSUM => return Err(unsupported(at, "CHECKSUM")),
            op::PICKLER => return Err(unsupported(at, "PICKLER")),
            op::REDUCE => return Err(unsupported(at, "REDUCE")),
            op::NEWOBJ => return Err(unsupported(at, "NEWOBJ")),
            op::DBROW => return Err(unsupported(at, "DBROW")),
            op::MARK => return Err(unsupported(at, "MARK")),
            other => {
                return Err(DecodeError { offset: at, kind: ErrorKind::UnknownOpcode(other) })
            }
        })
    }
}

fn unsupported(at: usize, name: &'static str) -> DecodeError {
    DecodeError { offset: at, kind: ErrorKind::Unsupported(name) }
}
```

Add to `lib.rs`:

```rust
pub mod decode;
pub mod opcodes;
pub mod string_table;
pub use decode::decode;
```

**Correctness checkpoint (not optional):** before running tests, re-read the Task 4 notes in `docs/format-notes.md` and reconcile: shared-map slot rule, REF/STRINGR index base, UNICODE char-vs-byte length, LONG sign. Where marshal.c differs from the code above, marshal.c wins — update the code and, if a test asserted the wrong thing, the test.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p blue-marshal`
Expected: 13 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/blue-marshal/src
git commit -m "Add blue-marshal decoder covering scalar, string, and container opcodes"
```

---

### Task 8: bmdump CLI + corpus integration test

**Files:**
- Create: `crates/blue-marshal/src/bin/bmdump.rs`, `crates/blue-marshal/tests/corpus.rs`
- Modify: `docs/format-notes.md`

**Interfaces:**
- Consumes: `decode`, `dump_text`.
- Produces: `bmdump dump <file>` (canonical text to stdout, exit 0/1) and `bmdump scan <dir>` (per-file OK/error + summary line `scanned N, ok K, failed M`, exit 0 iff M=0) — the instruments Tasks 9–11 use. Corpus test `cargo test -p blue-marshal --test corpus` that later serves as M0's exit gate.

- [ ] **Step 1: Write the corpus integration test**

`crates/blue-marshal/tests/corpus.rs`:

```rust
use std::fs;
use std::path::{Path, PathBuf};

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
#[ignore = "M0 gate: un-ignore when Task 9 reaches full corpus coverage"]
fn every_corpus_file_decodes() {
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
        if let Err(e) = blue_marshal::decode(&data) {
            failures.push(format!("{}: {e}", f.display()));
        }
    }
    assert!(
        failures.is_empty(),
        "{}/{} corpus files failed to decode:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
}
```

- [ ] **Step 2: Write bmdump**

`crates/blue-marshal/src/bin/bmdump.rs`:

```rust
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd, path] if cmd == "dump" => dump(Path::new(path)),
        [cmd, path] if cmd == "scan" => scan(Path::new(path)),
        _ => {
            eprintln!("usage: bmdump dump <file.dat> | bmdump scan <dir>");
            ExitCode::from(2)
        }
    }
}

fn dump(path: &Path) -> ExitCode {
    let data = match fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    match blue_marshal::decode(&data) {
        Ok(v) => {
            println!("{}", blue_marshal::dump_text(&v));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            ExitCode::FAILURE
        }
    }
}

fn scan(dir: &Path) -> ExitCode {
    let mut files = Vec::new();
    collect(dir, &mut files);
    let (mut ok, mut failed) = (0u32, 0u32);
    for f in &files {
        let data = fs::read(f).unwrap_or_default();
        match blue_marshal::decode(&data) {
            Ok(_) => ok += 1,
            Err(e) => {
                failed += 1;
                println!("FAIL {}: {e}", f.display());
            }
        }
    }
    println!("scanned {}, ok {ok}, failed {failed}", files.len());
    if failed == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "dat") {
            out.push(path);
        }
    }
}
```

- [ ] **Step 3: Build and take the baseline coverage measurement**

Run: `cargo run -p blue-marshal --bin bmdump -- scan testdata/corpus`
Expected: builds cleanly; prints per-file `FAIL` lines and a summary like `scanned 60, ok 12, failed 48`. **Failures are expected here** — this is the baseline for Task 9. Record the summary line and the distinct error kinds (offsets/opcodes from the FAIL lines) in `docs/format-notes.md` under a new `## Decoder coverage log` heading.

- [ ] **Step 4: Run the test suite**

Run: `cargo test -p blue-marshal`
Expected: all tests pass; `every_corpus_file_decodes` is reported as ignored (it is the M0 gate — Task 9 un-ignores it once coverage is complete). Take the gate's baseline explicitly:

Run: `cargo test -p blue-marshal --test corpus -- --ignored`
Expected: fails listing the same files as the scan (or passes if Task 7 already covers everything). This red run is a measurement, not a commit gate — the committed suite stays green because the test is ignored.

- [ ] **Step 5: Commit**

```bash
git add crates/blue-marshal/src/bin crates/blue-marshal/tests docs/format-notes.md
git commit -m "Add bmdump CLI and corpus coverage test with baseline results"
```

---

### Task 9: Decode coverage loop to 100%

**Files:**
- Modify: `crates/blue-marshal/src/decode.rs`, `crates/blue-marshal/src/value.rs` (only if a new opcode needs a new variant), `docs/format-notes.md`

**Interfaces:**
- Consumes: baseline failure list (Task 8), per-opcode encodings from marshal.c (Task 4 notes; consult `vendor/reverence/src/blue/marshal.c` directly for anything under-documented).
- Produces: `decode` handling every construct present in real settings files; `cargo test -p blue-marshal --test corpus` green on both snapshots. This is the foundation the M1 encoder round-trips against.

- [ ] **Step 1: Fix the most common failure**

Run: `cargo run -p blue-marshal --bin bmdump -- scan testdata/corpus`
Take the most frequent error kind (e.g. `Unsupported("GLOBAL")` at some offset). Read that opcode's `case` in `vendor/reverence/src/blue/marshal.c`, document its exact payload layout in `docs/format-notes.md`, then implement it in `decode.rs`.

Representation guidance for the likely stragglers (decide final shape from what the corpus actually contains):
- `GLOBAL` (a Python type/class name): decode payload (length-encoded name bytes) as `Value::Bytes` wrapped in a new `Value::Global(Vec<u8>)` variant — M1's encoder must re-emit it distinctly, so it needs its own variant, not a lossy merge into `Bytes`.
- `CHECKSUM`: read the u32, verify with adler32 over the range marshal.c specifies (implement adler32 inline, ~10 lines, no dependency), and continue decoding the wrapped value.
- `INSTANCE`/`NEWOBJ`/`REDUCE`: if present in settings files, add `Value::Instance { class: Box<Value>, state: Vec<Value> }` mirroring marshal.c's load order. If absent from the corpus, leave them as `Unsupported` errors and note that in format-notes.md.
- Nested streams: if `Value::Bytes` payloads that start with `0x7E` show up (double-marshaled settings values), do **not** auto-decode them in `decode` — keep bytes exact; instead teach `dump_text` to attempt `decode` on such payloads and render `stream?{...}` on success, raw bytes otherwise. Lossless data, readable dumps.

- [ ] **Step 2: Add a unit test for the newly implemented opcode**

Cut the minimal reproducing byte sequence from a real file (use the error offset; `bmdump dump` prints it) into a synthetic unit test in `decode.rs` following the Task 7 test style — real bytes, no personal strings (pick numeric/structural payloads).

- [ ] **Step 3: Re-scan**

Run: `cargo run -p blue-marshal --bin bmdump -- scan testdata/corpus`
Expected: `failed` count strictly decreases. Append the new summary line to the coverage log in `docs/format-notes.md`.

- [ ] **Step 4: Commit the increment**

```bash
git add crates/blue-marshal/src docs/format-notes.md
git commit -m "Decode <OPCODE> per reverence marshal.c"
```

- [ ] **Step 5: Repeat Steps 1–4 until the scan reports `failed 0`**

Exit criterion: `bmdump scan testdata/corpus` prints `failed 0` across **both** the historical and fresh snapshots, and `cargo test -p blue-marshal --test corpus -- --ignored` passes. Then remove the `#[ignore]` attribute from `every_corpus_file_decodes` (the gate is now a permanent regression test) and confirm `cargo test -p blue-marshal` is fully green. Also spot-check one large file end-to-end:

Run: `cargo run -p blue-marshal --bin bmdump -- dump testdata/corpus/<fresh snapshot>/<profile>/settings_Default/core_char_<id>.dat | Select-Object -First 40`
Expected: readable tree with recognizable keys (e.g. `b"autoreload"`, window-ish structures), no `hex:` soup at the top level.

- [ ] **Step 6: Final commit for the milestone-gate test**

```bash
git add -A -- crates docs
git commit -m "Reach full decode coverage over historical and fresh corpus"
```

---

### Task 10: [USER REQUIRED] Map geometry, columns, and suggestions

**Files:**
- Modify: `docs/format-notes.md` (new `## Mappings` content)

**Interfaces:**
- Consumes: `bmdump dump`, `tools/sync-corpus.ps1`, a user at the keyboard playing EVE.
- Produces: documented key paths (exact dict-key sequences from file root) for: overview window x/y/width/height, overview column set/order/widths, search-suggestion lists, screen-resolution keys, plus general window-geometry structure. These key paths are the contract the M1 `settings-model` crate is built against.

**Diff protocol (used for every experiment below):**

```powershell
# 1. before-snapshot
powershell -File tools\sync-corpus.ps1 -Label exp<N>-before
# 2. user performs EXACTLY ONE in-game change, quits client
# 3. after-snapshot
powershell -File tools\sync-corpus.ps1 -Label exp<N>-after
# 4. dump both versions of the active character's file and diff
cargo run -p blue-marshal --bin bmdump -- dump testdata\corpus\<before>\...\core_char_<id>.dat > before.txt
cargo run -p blue-marshal --bin bmdump -- dump testdata\corpus\<after>\...\core_char_<id>.dat > after.txt
git diff --no-index before.txt after.txt
```

(`before.txt`/`after.txt` live in the repo root; add `*.txt` at repo root to `.gitignore` in this task, or write them into `testdata/`.)

- [ ] **Step 1: Experiment 1 — move the overview window**
User: move the overview a clearly measurable amount (e.g. drag to a corner). Run the diff protocol. Record in format-notes.md: the key path holding x/y, value units (pixels? proportion? the file has keys like `widthProportion` — note which applies), and whether values live in `core_char` or `core_user`.

- [ ] **Step 2: Experiment 2 — resize the overview window**
Same protocol; record width/height keys and units.

- [ ] **Step 3: Experiment 3 — add/remove an overview column, then reorder columns**
Two changes, two diffs (run the protocol twice). Record where the column list lives, its element format (column ids? names?), and how order and per-column width are encoded.

- [ ] **Step 4: Experiment 4 — run a search that produces a remembered suggestion**
User: open People & Places (or the universal search), search a distinctive string like `zzztestmapping`, quit. Diff both `core_char` and `core_user` dumps (unknown which file holds it). Record the suggestion-list key path and its structure. Also grep the dumps for other list-of-strings structures nearby and record candidates for the autofill editor (spec: "discover them").

- [ ] **Step 5: Experiment 5 — move a non-overview window (e.g. the market window)**
Record whether all windows share one geometry structure (generic mapping the canvas can enumerate) or are per-window special cases. Also record where the client stores the screen resolution the geometry is relative to.

- [ ] **Step 6: Commit**

```bash
git add docs/format-notes.md .gitignore
git commit -m "Map geometry, overview column, and suggestion key paths"
```

---

### Task 11: [USER REQUIRED] Name-presence investigation

**Files:**
- Modify: `docs/format-notes.md`; possibly `docs/superpowers/specs/2026-07-12-eve-settings-editor-design.md` §6

**Interfaces:**
- Consumes: fresh-baseline dumps, the user (who tells us the character name used in Task 3 and which account it belongs to — used interactively, never written to committed files).
- Produces: a recorded decision for spec §6: is local name extraction viable as the primary source? Key paths if yes; ESI-fallback-only if no. Plus findings on account↔character correlation.

- [ ] **Step 1: Search the fresh character file for its own character's name**

Ask the user for the character name used in Task 3. Run:

```powershell
cargo run -p blue-marshal --bin bmdump -- dump testdata\corpus\<fresh>\...\core_char_<id>.dat > char.txt
Select-String -Path char.txt -Pattern "<CharacterName>" -SimpleMatch
```

Record (names redacted): whether the file's own character name is present, and at which key path; whether that path looks structural (e.g. a profile/session key) or incidental (e.g. a chat window title).

- [ ] **Step 2: Search the matching user file**

Same procedure against `core_user_<id>.dat`. Additionally search for the names of the account's *other* characters (user lists them verbally). Expected per earlier string-scans: character names appear in user files (plausibly the character-select screen); determine whether they're keyed by character ID (that would give us both name resolution AND account↔character correlation in one structure).

- [ ] **Step 3: Check correlation signals**

For the fresh snapshot, compare `LastWriteTime` of the played character's `core_char` file and its account's `core_user` file (same login session → near-identical timestamps). Record whether mtime correlation is corroborated by in-file cross-references found in Step 2.

- [ ] **Step 4: Record the decision and update the spec if warranted**

In `docs/format-notes.md` `## Mappings`, write the decision block: local-extraction key paths (if viable) for char names and account→characters, or the determination that ESI fallback is required. If findings change spec §6's resolution-priority description materially, edit the spec accordingly (M0 is explicitly allowed to).

- [ ] **Step 5: Commit**

```bash
git add docs
git commit -m "Record name-extraction findings and update spec"
```

---

### Task 12: M0 gate review

**Files:**
- Modify: `docs/format-notes.md` (status section), possibly the spec

**Interfaces:**
- Consumes: everything above.
- Produces: go/no-go record for M1 planning.

- [ ] **Step 1: Verify the exit criteria**

Run: `cargo test -p blue-marshal` → all green including corpus test.
Run: `cargo run -p blue-marshal --bin bmdump -- scan testdata/corpus` → `failed 0`.
Check `docs/format-notes.md` contains: verified opcode encodings, geometry key paths + units, column structure, suggestion-list paths, resolution keys, name-extraction decision.

- [ ] **Step 2: Write the M0 summary**

Update format-notes.md `## Status` with: format confirmed for current client (date), coverage 100% over N files, mappings complete, name decision, and any spec revisions made. List any surprises that affect M1 (e.g. nested streams, INSTANCE objects, proportion-based geometry).

- [ ] **Step 3: Report to the user and commit**

Summarize findings to the user; flag anything that should change the spec before M1 is planned.

```bash
git add docs
git commit -m "Close out M0 with format validation summary"
```

---

## Plan self-review (completed)

- **Spec coverage (M0 scope):** format validation (Task 3), decoder against real corpus (Tasks 5–9), geometry/column/suggestion mapping (Task 10), name presence + correlation (Task 11), spec-revision gate (Tasks 11–12). Live-directory rule enforced structurally (Task 2 script is the only live-dir reader). ✔
- **Placeholder scan:** the two intentionally open items — shared-map slot rule and STRINGR/REF index base — are not placeholders but explicitly-flagged verification points with a designated authoritative source (Task 4 / marshal.c) and a correctness checkpoint in Task 7. Everything else has concrete code/commands. ✔
- **Type consistency:** `decode(data: &[u8]) -> Result<Value, DecodeError>`, `dump_text(&Value) -> String`, `Reader` method set, and `ErrorKind` variants are used identically across Tasks 5–9; corpus test and bmdump both consume `blue_marshal::decode`/`dump_text`. ✔
