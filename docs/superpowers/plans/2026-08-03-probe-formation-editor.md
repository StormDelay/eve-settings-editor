# Probe Formation Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Edit, create and visualise the account file's custom probe scanner formations, and copy them between accounts in the batch tool.

**Architecture:** A new `settings-model` module (`probes.rs`) owns every byte of EVE format knowledge, following `neocom.rs` exactly — project through `treewalk::section`, validate before inlining, inline then write. Tauri commands in `ops.rs` expose it on the `user` slot like `keybinds`. A new Svelte view holds metres as its source of truth and derives every displayed number, so display rounding can never reach the file. The batch category is one `Category` variant plus one `Aspect`.

**Tech Stack:** Rust (`blue-marshal`, `serde`), Tauri 2, Svelte 5 (runes), vitest + `@testing-library/svelte` for components, `node --test` for pure helpers.

## Global Constraints

Copied from `docs/superpowers/specs/2026-08-03-probe-formation-editor-design.md`. Every task's requirements implicitly include this section.

- **Key path is two levels: `ui → probescanning.customFormations`.** The dot is part of the key name, not a path separator.
- **Account-file root `ui` is itself a `Ref`.** Read it through `treewalk::section`, never a bare `is_bytes` match on the root dict.
- **Negative ids are never projected and never written.** `-4` is the client's scratch copy of the formation being edited.
- **A formation name reads from `Bytes` or `Str`, and is always written as `Str`.**
- **Range is one value per formation**, written to every probe. A loaded formation whose entries disagree sets `mixed_range` and is never flattened.
- **Probe count is 1 to 8 inclusive.**
- **Metres are the unit in Rust and in the frontend's state.** AU and km exist only in displayed text.
- **Writes preserve an existing `(timestamp, value)` wrapper** and mint a zero `Long` when the key is absent.
- **Validate before `inline_all`,** so a rejected edit leaves the document byte-for-byte as it was.
- **`0.5 AU = 74798935350.0 metres`** — the range every corpus formation carries.
- Branch: `probe-formation-editor` (already created; the spec is committed on it).

---

### Task 1: `probes.rs` — the read path

**Files:**
- Create: `crates/settings-model/src/probes.rs`
- Modify: `crates/settings-model/src/lib.rs` (add `pub mod probes;` after `pub mod neocom;`, and a `pub use` line after the `neocom` one)
- Test: inline `#[cfg(test)] mod tests` in `crates/settings-model/src/probes.rs`

**Interfaces:**
- Consumes: `crate::treewalk::{collect_shared, effective, find_child, is_bytes, section, text, SharedTable}` — all `pub(crate)`, already exist.
- Produces:
  - `pub struct Formation { pub id: i64, pub name: String, pub probes: Vec<[f64; 3]>, pub range: f64, pub mixed_range: bool }`
  - `pub struct Formations { pub formations: Vec<Formation>, pub selected: Option<i64> }`
  - `pub enum ProbeError { NoUi, NoFormations, NoSuchFormation, BadProbeCount, BadName }`
  - `pub fn project_formations(v: &Value) -> Result<Formations, ProbeError>`
  - `pub const DEFAULT_RANGE: f64` (74798935350.0), `pub const MAX_PROBES: usize` (8)

- [ ] **Step 1: Write the failing test**

Create `crates/settings-model/src/probes.rs` containing ONLY the test module below plus the two `use` lines. It will not compile yet — that is the failing state.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
    fn ts() -> Value { Value::Long(vec![0u8; 8]) }
    fn f(x: f64) -> Value { Value::Float(x) }

    /// One probe entry: ((x, y, z), range).
    fn probe(x: f64, y: f64, z: f64, r: f64) -> Value {
        Value::Tuple(vec![Value::Tuple(vec![f(x), f(y), f(z)]), f(r)])
    }

    /// A formation entry: (name, [probe, ...]).
    fn formation(name: Value, probes: Vec<Value>) -> Value {
        Value::Tuple(vec![name, Value::List(probes)])
    }

    /// user -> ui -> { customFormations: (ts, dict), selectedFormationID: (ts, Int) }
    ///
    /// Holds the two ids the corpus shows (0 "close", 1 "on grid") plus the -4
    /// scratch slot, whose name is Bytes where the others are Str.
    fn doc() -> Value {
        Value::Dict(vec![(b("ui"), Value::Dict(vec![
            (b("probescanning.customFormations"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (Value::Int(0), formation(Value::Str("close".into()), vec![
                    probe(-1199120384.0, -115136512.0, -415997952.0, DEFAULT_RANGE),
                    probe(22762389504.0, -115136512.0, -122200064.0, DEFAULT_RANGE),
                ])),
                (Value::Int(1), formation(Value::Str("on grid".into()), vec![
                    probe(1.0, 2.0, 3.0, DEFAULT_RANGE),
                ])),
                (Value::Int(-4), formation(b("tempFormation"), vec![
                    probe(9.0, 9.0, 9.0, DEFAULT_RANGE),
                ])),
            ])])),
            (b("probescanning.selectedFormationID"), Value::Tuple(vec![ts(), Value::Int(0)])),
        ]))])
    }

    #[test]
    fn projects_user_formations_in_id_order() {
        let p = project_formations(&doc()).expect("projects");
        assert_eq!(p.formations.len(), 2, "the -4 scratch slot must not be projected");
        assert_eq!(p.formations[0].id, 0);
        assert_eq!(p.formations[0].name, "close");
        assert_eq!(p.formations[0].probes.len(), 2);
        assert_eq!(p.formations[1].id, 1);
        assert_eq!(p.formations[1].name, "on grid");
        assert_eq!(p.selected, Some(0));
    }

    #[test]
    fn coordinates_survive_the_projection_exactly() {
        // These are f64 read straight off the wire. Any rounding here would
        // displace every probe in the file the moment the editor saved.
        let p = project_formations(&doc()).unwrap();
        assert_eq!(p.formations[0].probes[0], [-1199120384.0, -115136512.0, -415997952.0]);
        assert_eq!(p.formations[0].range, DEFAULT_RANGE);
        assert!(!p.formations[0].mixed_range);
    }

    #[test]
    fn a_bytes_name_reads_as_text() {
        // The scratch slot's name is Bytes where user formations are Str. It is
        // not projected, but `text` handling both shapes is what keeps a reader
        // that meets one in a user slot from blanking. Assert on the helper.
        let mut sh = SharedTable::new();
        let v = b("tempFormation");
        collect_shared(&v, &mut sh);
        assert_eq!(text(&v, &sh).as_deref(), Some("tempFormation"));
    }

    #[test]
    fn a_mixed_range_formation_is_flagged_not_flattened() {
        let d = Value::Dict(vec![(b("ui"), Value::Dict(vec![
            (b("probescanning.customFormations"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (Value::Int(0), formation(Value::Str("odd".into()), vec![
                    probe(1.0, 0.0, 0.0, DEFAULT_RANGE),
                    probe(2.0, 0.0, 0.0, DEFAULT_RANGE / 2.0),
                ])),
            ])])),
        ]))]);
        let p = project_formations(&d).unwrap();
        assert!(p.formations[0].mixed_range, "differing ranges must be reported");
        assert_eq!(p.formations[0].range, DEFAULT_RANGE, "range reports the first entry");
        assert_eq!(
            p.formations[0].ranges,
            vec![DEFAULT_RANGE, DEFAULT_RANGE / 2.0],
            "the per-probe ranges must survive whole, so the view can name which rows differ",
        );
    }

    #[test]
    fn a_formation_entry_is_not_mistaken_for_a_timestamp_wrapper() {
        // Both `(FILETIME, dict)` and `(name, probes)` are 2-tuples. A wrapper
        // unwrapper that only checks the length turns every formation into its
        // probe list and projects nothing.
        let p = project_formations(&doc()).unwrap();
        assert_eq!(p.formations[0].name, "close", "the name half must survive");
    }

    #[test]
    fn a_file_without_the_key_reports_no_formations() {
        let d = Value::Dict(vec![(b("ui"), Value::Dict(vec![(b("other"), Value::Int(1))]))]);
        assert_eq!(project_formations(&d), Err(ProbeError::NoFormations));
    }

    #[test]
    fn a_file_without_ui_reports_no_ui() {
        let d = Value::Dict(vec![(b("windows"), Value::Dict(Vec::new()))]);
        assert_eq!(project_formations(&d), Err(ProbeError::NoUi));
    }

    #[test]
    fn an_absent_selected_id_is_none() {
        let d = Value::Dict(vec![(b("ui"), Value::Dict(vec![
            (b("probescanning.customFormations"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (Value::Int(0), formation(Value::Str("a".into()), vec![probe(1.0, 0.0, 0.0, DEFAULT_RANGE)])),
            ])])),
        ]))]);
        assert_eq!(project_formations(&d).unwrap().selected, None);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p settings-model probes`
Expected: FAIL — `cannot find function project_formations in this scope`, and the module is not declared.

- [ ] **Step 3: Write the implementation**

Put this ABOVE the test module in `crates/settings-model/src/probes.rs`:

```rust
//! Custom probe formations: `ui → probescanning.customFormations`, a
//! `(timestamp, {Int id: (name, List[((x, y, z), range)])})` — the saved
//! arrangements in the probe scanner's formation menu. Account-side.
//!
//! Three traps, all corpus-measured (docs/settings-field-reference.md, "Custom
//! probe formations"; placement in the editor's spec §2.1):
//!
//! - **The dot is part of the key name.** `probescanning.customFormations` is a
//!   single key under `ui`, not a path through a `probescanning` section.
//! - **Id `-4` is the client's scratch copy** of the formation being edited,
//!   not a user formation. Never projected, never written.
//! - **Its name is `Bytes` where every user formation's is `Str`**, so a reader
//!   matching one shape blanks on the other. `treewalk::text` handles both.
//!
//! A fourth trap is this module's own: a formation entry `(name, probes)` and
//! the `(FILETIME, value)` wrapper are BOTH 2-tuples. `wrapper_payload` checks
//! for the `Long` first element rather than the length, or every formation
//! unwraps into its probe list.

use blue_marshal::Value;
use serde::Serialize;

use crate::treewalk::{
    collect_shared, effective, find_child, inline_all, is_bytes, section, text, SharedTable,
};

const KEY: &[u8] = b"probescanning.customFormations";
const SELECTED_KEY: &[u8] = b"probescanning.selectedFormationID";

/// 0.5 AU in metres. Every one of the 984 corpus probe entries carries exactly
/// this, to the metre.
pub const DEFAULT_RANGE: f64 = 74_798_935_350.0;

/// A formation holds 1 to 8 probes. The corpus only ever shows 8; the client
/// lets a player launch fewer, so shorter formations are accepted (spec §2.4).
pub const MAX_PROBES: usize = 8;

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum ProbeError {
    /// No `ui` section in the document.
    NoUi,
    /// No `probescanning.customFormations` under `ui`.
    NoFormations,
    /// An id the file does not hold — including any negative id, which is never
    /// a user formation.
    NoSuchFormation,
    /// A write with no probes, or more than `MAX_PROBES`.
    BadProbeCount,
    /// A name that is empty once trimmed.
    BadName,
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeError::NoUi => write!(f, "This file has no UI section."),
            ProbeError::NoFormations => write!(f, "This account has no custom probe formations."),
            ProbeError::NoSuchFormation => write!(f, "That probe formation no longer exists."),
            ProbeError::BadProbeCount => write!(f, "A formation needs between 1 and 8 probes."),
            ProbeError::BadName => write!(f, "A formation needs a name."),
        }
    }
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Formation {
    pub id: i64,
    pub name: String,
    /// Metre offsets from the formation centre. EVE's axes: X and Z are the
    /// horizontal plane, Y is up.
    pub probes: Vec<[f64; 3]>,
    /// Metres, one per probe, positionally matching `probes` — the file's own
    /// values, kept whole. The editor writes ONE range back (spec §2.3), but
    /// reading has to stay faithful or a mixed formation could not be shown as
    /// what it is.
    pub ranges: Vec<f64>,
    /// Metres. The first probe's range — what the single range field edits.
    pub range: f64,
    /// The probes did not agree on a range. No corpus formation does this; the
    /// flag exists so the UI can show the file rather than flatten it.
    pub mixed_range: bool,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Formations {
    /// Ascending by id, negative ids excluded.
    pub formations: Vec<Formation>,
    /// `probescanning.selectedFormationID`, when the file has it.
    pub selected: Option<i64>,
}

/// The value inside a `(FILETIME, value)` wrapper, or the value itself.
///
/// The `Long` guard is load-bearing: a formation entry is also a 2-tuple, so a
/// bare length check would unwrap `(name, probes)` into `probes`.
fn wrapper_payload<'a>(v: &'a Value, sh: &SharedTable<'a>) -> &'a Value {
    match effective(v, sh) {
        Value::Tuple(t) if t.len() == 2 && matches!(effective(&t[0], sh), Value::Long(_)) => {
            effective(&t[1], sh)
        }
        other => other,
    }
}

fn float(v: &Value, sh: &SharedTable) -> Option<f64> {
    match effective(v, sh) {
        Value::Float(f) => Some(*f),
        // Not seen in the corpus, but a whole-number coordinate could encode as
        // an Int, and refusing it would drop the whole formation.
        Value::Int(i) => Some(*i as f64),
        _ => None,
    }
}

fn read_probe(v: &Value, sh: &SharedTable) -> Option<([f64; 3], f64)> {
    let Value::Tuple(t) = effective(v, sh) else { return None };
    if t.len() != 2 {
        return None;
    }
    let Value::Tuple(xyz) = effective(&t[0], sh) else { return None };
    if xyz.len() != 3 {
        return None;
    }
    let pos = [float(&xyz[0], sh)?, float(&xyz[1], sh)?, float(&xyz[2], sh)?];
    Some((pos, float(&t[1], sh)?))
}

fn read_formation(id: i64, v: &Value, sh: &SharedTable) -> Option<Formation> {
    let Value::Tuple(t) = effective(v, sh) else { return None };
    if t.len() != 2 {
        return None;
    }
    let name = text(&t[0], sh)?;
    let list = match effective(&t[1], sh) {
        Value::List(l) | Value::Tuple(l) => l,
        _ => return None,
    };
    let read: Vec<([f64; 3], f64)> = list.iter().filter_map(|p| read_probe(p, sh)).collect();
    // A partially-read formation is worse than none: the editor would save back
    // the probes it understood and silently drop the rest.
    if read.len() != list.len() || read.is_empty() {
        return None;
    }
    let range = read[0].1;
    Some(Formation {
        id,
        name,
        mixed_range: read.iter().any(|(_, r)| *r != range),
        probes: read.iter().map(|(p, _)| *p).collect(),
        ranges: read.iter().map(|(_, r)| *r).collect(),
        range,
    })
}

pub fn project_formations(v: &Value) -> Result<Formations, ProbeError> {
    let mut sh = SharedTable::new();
    collect_shared(v, &mut sh);
    // `section` resolves a Shared/Ref section key — account files store root
    // `ui` as a Ref, which a bare is_bytes match misses (neocom.rs documents
    // the same trap).
    let (entries, _) = section(v, b"ui", &sh).ok_or(ProbeError::NoUi)?;
    let raw = find_child(entries, KEY, &sh).ok_or(ProbeError::NoFormations)?;
    let Value::Dict(d) = wrapper_payload(raw, &sh) else { return Err(ProbeError::NoFormations) };
    let mut formations: Vec<Formation> = d
        .iter()
        .filter_map(|(k, val)| {
            let Value::Int(id) = effective(k, &sh) else { return None };
            // The -4 scratch slot and anything else negative.
            if *id < 0 {
                return None;
            }
            read_formation(*id, val, &sh)
        })
        .collect();
    formations.sort_by_key(|f| f.id);
    let selected = find_child(entries, SELECTED_KEY, &sh).and_then(|v| {
        match wrapper_payload(v, &sh) {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    });
    Ok(Formations { formations, selected })
}
```

Then wire the module up. In `crates/settings-model/src/lib.rs`, add after the `pub mod neocom;` line:

```rust
pub mod probes;
```

and after the `pub use neocom::{...};` line:

```rust
pub use probes::{project_formations, Formation, Formations, ProbeError, DEFAULT_RANGE, MAX_PROBES};
```

The `inline_all` and `is_bytes` imports are unused until Task 2; add `#[allow(unused_imports)]` is NOT the fix — instead, leave them out of the `use` list in this task and add them in Task 2.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p settings-model probes`
Expected: PASS, 8 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/probes.rs crates/settings-model/src/lib.rs
git commit -m "Project the account file's custom probe formations

Reads ui -> probescanning.customFormations into (id, name, probes, range),
skipping the -4 scratch slot the client uses for the formation being edited.

The wrapper unwrapper checks for a Long first element rather than a tuple
length: a formation entry (name, probes) is also a 2-tuple, so a length check
turns every formation into its probe list and projects nothing.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 2: `probes.rs` — the write path

**Files:**
- Modify: `crates/settings-model/src/probes.rs`
- Modify: `crates/settings-model/src/lib.rs` (extend the `pub use probes::{...}` line)
- Test: inline `#[cfg(test)] mod tests` in `crates/settings-model/src/probes.rs`

**Interfaces:**
- Consumes: everything Task 1 produced, plus `crate::treewalk::{inline_all, is_bytes}`.
- Produces:
  - `pub fn set_formation(v: &mut Value, id: i64, name: &str, probes: &[[f64; 3]], range: f64) -> Result<(), ProbeError>`
  - `pub fn remove_formation(v: &mut Value, id: i64) -> Result<(), ProbeError>`
  - `pub fn next_id(f: &Formations) -> i64`

- [ ] **Step 1: Write the failing tests**

Append these to the existing `mod tests` in `crates/settings-model/src/probes.rs`:

```rust
    /// The formations dict as stored, for asserting on raw shape.
    fn stored(v: &Value) -> &Vec<(Value, Value)> {
        let Value::Dict(top) = v else { panic!("not a dict") };
        let (_, ui) = top.iter().find(|(k, _)| is_bytes(k, b"ui")).expect("ui");
        let Value::Dict(entries) = ui else { panic!("ui is not a dict") };
        let (_, raw) = entries.iter().find(|(k, _)| is_bytes(k, KEY)).expect("the key");
        let Value::Tuple(t) = raw else { panic!("not wrapped") };
        let Value::Dict(d) = &t[1] else { panic!("payload is not a dict") };
        d
    }

    /// The `(timestamp, _)` stamp on the formations key, for wrapper assertions.
    fn stamp(v: &Value) -> Value {
        let Value::Dict(top) = v else { panic!("not a dict") };
        let (_, ui) = top.iter().find(|(k, _)| is_bytes(k, b"ui")).expect("ui");
        let Value::Dict(entries) = ui else { panic!("ui is not a dict") };
        let (_, raw) = entries.iter().find(|(k, _)| is_bytes(k, KEY)).expect("the key");
        let Value::Tuple(t) = raw else { panic!("not wrapped") };
        t[0].clone()
    }

    fn seeded() -> Value {
        // A distinguishable non-zero stamp, so a test can tell "the original
        // survived" from "a fresh zero one was minted".
        let mut d = doc();
        let Value::Dict(top) = &mut d else { unreachable!() };
        let (_, ui) = top.iter_mut().find(|(k, _)| is_bytes(k, b"ui")).unwrap();
        let Value::Dict(entries) = ui else { unreachable!() };
        let (_, raw) = entries.iter_mut().find(|(k, _)| is_bytes(k, KEY)).unwrap();
        let Value::Tuple(t) = raw else { unreachable!() };
        t[0] = Value::Long(vec![7, 0, 0, 0, 0, 0, 0, 0]);
        d
    }

    #[test]
    fn set_replaces_an_existing_formation_and_keeps_the_stamp() {
        let mut v = seeded();
        set_formation(&mut v, 0, "closer", &[[1.0, 2.0, 3.0]], DEFAULT_RANGE).unwrap();
        let p = project_formations(&v).unwrap();
        assert_eq!(p.formations[0].name, "closer");
        assert_eq!(p.formations[0].probes, vec![[1.0, 2.0, 3.0]]);
        assert_eq!(
            stamp(&v),
            Value::Long(vec![7, 0, 0, 0, 0, 0, 0, 0]),
            "the ORIGINAL timestamp must survive the edit, not be replaced",
        );
    }

    #[test]
    fn set_creates_a_formation_at_a_new_id() {
        let mut v = doc();
        set_formation(&mut v, 2, "new", &[[1.0, 0.0, 0.0]], DEFAULT_RANGE).unwrap();
        let p = project_formations(&v).unwrap();
        assert_eq!(p.formations.len(), 3);
        assert_eq!(p.formations[2].id, 2);
        assert_eq!(p.formations[2].name, "new");
    }

    #[test]
    fn a_written_name_is_str_never_bytes() {
        // The only Bytes name in the corpus is the scratch slot. Writing Bytes
        // would make a user formation look like one.
        let mut v = doc();
        set_formation(&mut v, 0, "close", &[[1.0, 0.0, 0.0]], DEFAULT_RANGE).unwrap();
        let d = stored(&v);
        let (_, entry) = d.iter().find(|(k, _)| matches!(k, Value::Int(0))).unwrap();
        let Value::Tuple(t) = entry else { panic!("not a formation tuple") };
        assert_eq!(t[0], Value::Str("close".into()));
    }

    #[test]
    fn the_range_is_written_to_every_probe() {
        let mut v = doc();
        let probes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        set_formation(&mut v, 0, "even", &probes, 123.0).unwrap();
        let p = project_formations(&v).unwrap();
        assert_eq!(p.formations[0].range, 123.0);
        assert!(!p.formations[0].mixed_range);
        assert_eq!(p.formations[0].probes.len(), 3);
    }

    #[test]
    fn the_scratch_slot_survives_a_write() {
        let mut v = doc();
        set_formation(&mut v, 0, "close", &[[1.0, 0.0, 0.0]], DEFAULT_RANGE).unwrap();
        let d = stored(&v);
        assert!(
            d.iter().any(|(k, _)| matches!(k, Value::Int(-4))),
            "the client's -4 scratch slot must be left untouched",
        );
    }

    #[test]
    fn a_key_absent_from_the_file_is_minted_wrapped() {
        let mut v = Value::Dict(vec![(b("ui"), Value::Dict(vec![(b("other"), Value::Int(1))]))]);
        set_formation(&mut v, 0, "first", &[[1.0, 0.0, 0.0]], DEFAULT_RANGE).unwrap();
        assert_eq!(
            stamp(&v),
            Value::Long(vec![0u8; 8]),
            "a freshly minted key must carry a zero Long stamp, not a bare dict",
        );
        assert_eq!(project_formations(&v).unwrap().formations.len(), 1);
    }

    #[test]
    fn a_rejected_write_leaves_the_document_untouched() {
        let before = doc();
        let mut v = doc();
        assert_eq!(set_formation(&mut v, 0, "x", &[], DEFAULT_RANGE), Err(ProbeError::BadProbeCount));
        assert_eq!(set_formation(&mut v, 0, "  ", &[[1.0, 0.0, 0.0]], DEFAULT_RANGE), Err(ProbeError::BadName));
        let nine = [[1.0, 0.0, 0.0]; 9];
        assert_eq!(set_formation(&mut v, 0, "x", &nine, DEFAULT_RANGE), Err(ProbeError::BadProbeCount));
        assert_eq!(set_formation(&mut v, -4, "x", &[[1.0, 0.0, 0.0]], DEFAULT_RANGE), Err(ProbeError::NoSuchFormation));
        assert_eq!(v, before, "a rejected write must not inline or otherwise touch the document");
    }

    #[test]
    fn one_and_eight_probes_are_both_accepted() {
        let mut v = doc();
        set_formation(&mut v, 0, "one", &[[1.0, 0.0, 0.0]], DEFAULT_RANGE).unwrap();
        assert_eq!(project_formations(&v).unwrap().formations[0].probes.len(), 1);
        let eight = [[1.0, 0.0, 0.0]; 8];
        set_formation(&mut v, 0, "eight", &eight, DEFAULT_RANGE).unwrap();
        assert_eq!(project_formations(&v).unwrap().formations[0].probes.len(), 8);
    }

    #[test]
    fn remove_drops_the_formation() {
        let mut v = doc();
        remove_formation(&mut v, 1).unwrap();
        let p = project_formations(&v).unwrap();
        assert_eq!(p.formations.len(), 1);
        assert_eq!(p.formations[0].id, 0);
    }

    #[test]
    fn removing_the_selected_formation_repoints_the_selection() {
        let mut v = doc(); // selected = 0
        remove_formation(&mut v, 0).unwrap();
        let p = project_formations(&v).unwrap();
        assert_eq!(p.selected, Some(1), "the selection must never name a deleted formation");
    }

    #[test]
    fn removing_an_unselected_formation_leaves_the_selection_alone() {
        let mut v = doc(); // selected = 0
        remove_formation(&mut v, 1).unwrap();
        assert_eq!(project_formations(&v).unwrap().selected, Some(0));
    }

    #[test]
    fn removing_the_last_formation_leaves_the_selection_alone() {
        let mut v = doc();
        remove_formation(&mut v, 1).unwrap();
        remove_formation(&mut v, 0).unwrap();
        let p = project_formations(&v).unwrap();
        assert!(p.formations.is_empty());
        assert_eq!(p.selected, Some(0), "nothing to repoint at, so leave the key as it was");
    }

    #[test]
    fn remove_refuses_an_unknown_or_negative_id() {
        let mut v = doc();
        assert_eq!(remove_formation(&mut v, 9), Err(ProbeError::NoSuchFormation));
        assert_eq!(remove_formation(&mut v, -4), Err(ProbeError::NoSuchFormation));
        assert!(stored(&v).iter().any(|(k, _)| matches!(k, Value::Int(-4))));
    }

    #[test]
    fn next_id_fills_the_lowest_gap() {
        let p = project_formations(&doc()).unwrap(); // ids 0 and 1
        assert_eq!(next_id(&p), 2);
        let empty = Formations { formations: Vec::new(), selected: None };
        assert_eq!(next_id(&empty), 0);
        let gapped = Formations {
            formations: vec![
                Formation { id: 0, name: "a".into(), probes: vec![[0.0; 3]], ranges: vec![1.0], range: 1.0, mixed_range: false },
                Formation { id: 2, name: "b".into(), probes: vec![[0.0; 3]], ranges: vec![1.0], range: 1.0, mixed_range: false },
            ],
            selected: None,
        };
        assert_eq!(next_id(&gapped), 1, "ids are reused in the corpus, not counted upward");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p settings-model probes`
Expected: FAIL — `cannot find function set_formation` / `remove_formation` / `next_id`.

- [ ] **Step 3: Write the implementation**

Extend the `use crate::treewalk::{...}` line in `probes.rs` to include `inline_all` and `is_bytes`:

```rust
use crate::treewalk::{
    collect_shared, effective, find_child, inline_all, is_bytes, section, text, SharedTable,
};
```

Then append, above the test module:

```rust
/// The formations dict, mutable, minting a `(zero Long, Dict)` wrapper when the
/// key is absent — the `overview_states.rs` rule. EVE re-stamps on its next
/// save. The document must already be inlined, so no `Shared` survives here.
fn formations_mut(v: &mut Value) -> Result<&mut Vec<(Value, Value)>, ProbeError> {
    let Value::Dict(top) = v else { return Err(ProbeError::NoUi) };
    let (_, ui) = top.iter_mut().find(|(k, _)| is_bytes(k, b"ui")).ok_or(ProbeError::NoUi)?;
    let Value::Dict(entries) = ui else { return Err(ProbeError::NoUi) };
    if !entries.iter().any(|(k, _)| is_bytes(k, KEY)) {
        entries.push((
            Value::Bytes(KEY.to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Dict(Vec::new())]),
        ));
    }
    let (_, raw) = entries
        .iter_mut()
        .find(|(k, _)| is_bytes(k, KEY))
        .expect("just ensured present");
    // The length check is split out of the match so the arm below is an
    // unguarded reborrow — a guarded arm does not borrow-check against a
    // move-catch-all on a `&mut` scrutinee (the neocom.rs note).
    let wrapped = matches!(raw, Value::Tuple(t) if t.len() == 2 && matches!(t[0], Value::Long(_)));
    let payload = if wrapped {
        let Value::Tuple(t) = raw else { unreachable!() };
        &mut t[1]
    } else {
        raw
    };
    match payload {
        Value::Dict(d) => Ok(d),
        _ => Err(ProbeError::NoFormations),
    }
}

/// Point `selectedFormationID` at `id`, preserving an existing stamp and
/// minting a zero one when the key is absent. The document must be inlined.
fn set_selected(v: &mut Value, id: i64) -> Result<(), ProbeError> {
    let Value::Dict(top) = v else { return Err(ProbeError::NoUi) };
    let (_, ui) = top.iter_mut().find(|(k, _)| is_bytes(k, b"ui")).ok_or(ProbeError::NoUi)?;
    let Value::Dict(entries) = ui else { return Err(ProbeError::NoUi) };
    match entries.iter_mut().find(|(k, _)| is_bytes(k, SELECTED_KEY)) {
        Some((_, slot)) => match slot {
            // Replace whichever element is not the stamp, rather than assuming
            // position — hunting for the Long and taking "the other one" is the
            // overview_states.rs idiom, and it cannot produce a malformed
            // (Long, Int, Int) the way pushing would.
            Value::Tuple(items) => match items.iter_mut().find(|e| !matches!(e, Value::Long(_))) {
                Some(inner) => *inner = Value::Int(id),
                None => *slot = Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Int(id)]),
            },
            other => *other = Value::Int(id),
        },
        None => entries.push((
            Value::Bytes(SELECTED_KEY.to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Int(id)]),
        )),
    }
    Ok(())
}

/// Replace the formation at `id`, or create it there.
///
/// `range` is written to every probe: the format carries one per entry, but all
/// 984 corpus entries agree, and the editor offers a single control (spec §2.3).
pub fn set_formation(
    v: &mut Value,
    id: i64,
    name: &str,
    probes: &[[f64; 3]],
    range: f64,
) -> Result<(), ProbeError> {
    // Validate BEFORE inlining, so a rejected write leaves the document
    // byte-for-byte as it was (the tests assert exactly this).
    if id < 0 {
        return Err(ProbeError::NoSuchFormation); // never the -4 scratch slot
    }
    if name.trim().is_empty() {
        return Err(ProbeError::BadName);
    }
    if probes.is_empty() || probes.len() > MAX_PROBES {
        return Err(ProbeError::BadProbeCount);
    }
    inline_all(v);
    let d = formations_mut(v)?;
    let entry = Value::Tuple(vec![
        Value::Str(name.to_string()),
        Value::List(
            probes
                .iter()
                .map(|p| {
                    Value::Tuple(vec![
                        Value::Tuple(vec![Value::Float(p[0]), Value::Float(p[1]), Value::Float(p[2])]),
                        Value::Float(range),
                    ])
                })
                .collect(),
        ),
    ]);
    match d.iter_mut().find(|(k, _)| matches!(k, Value::Int(i) if *i == id)) {
        Some((_, slot)) => *slot = entry,
        None => d.push((Value::Int(id), entry)),
    }
    Ok(())
}

/// Delete a formation, repointing `selectedFormationID` when it named this one.
/// Leaving the selection on a deleted formation is the one outcome that could
/// confuse the client.
pub fn remove_formation(v: &mut Value, id: i64) -> Result<(), ProbeError> {
    if id < 0 {
        return Err(ProbeError::NoSuchFormation);
    }
    let before = project_formations(v)?;
    if !before.formations.iter().any(|f| f.id == id) {
        return Err(ProbeError::NoSuchFormation);
    }
    inline_all(v);
    {
        let d = formations_mut(v)?;
        d.retain(|(k, _)| !matches!(k, Value::Int(i) if *i == id));
    }
    if before.selected == Some(id) {
        if let Some(next) = before.formations.iter().map(|f| f.id).filter(|i| *i != id).min() {
            set_selected(v, next)?;
        }
    }
    Ok(())
}

/// The smallest unused id `>= 0`. Corpus ids are small and reused rather than
/// minted, so this fills gaps rather than counting up from the maximum.
pub fn next_id(f: &Formations) -> i64 {
    (0i64..).find(|c| !f.formations.iter().any(|x| x.id == *c)).expect("i64 has a free id")
}
```

Extend the `pub use` line in `crates/settings-model/src/lib.rs`:

```rust
pub use probes::{
    next_id as next_formation_id, project_formations, remove_formation, set_formation, Formation,
    Formations, ProbeError, DEFAULT_RANGE, MAX_PROBES,
};
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p settings-model probes`
Expected: PASS, 21 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/probes.rs crates/settings-model/src/lib.rs
git commit -m "Write custom probe formations back to the account file

set_formation replaces or creates at an id, remove_formation deletes and
repoints selectedFormationID when it named the formation that went. Both
preserve the key's timestamp and mint a zero Long when the key is absent.

Names go back as Str. The only Bytes name in the corpus belongs to the -4
scratch slot, so writing Bytes would make a user formation look like the
client's working copy. Negative ids are refused by both entry points, which is
what keeps that slot out of reach.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 3: The corpus gate and its synthetic fixture

**Files:**
- Create: `crates/settings-model/tests/probes_corpus.rs`
- Modify: `crates/settings-model/src/bin/gen_fixtures.rs` (add the key to `user_modern()`)
- Regenerate: `fixtures/synthetic/profile/core_user_80000001.dat`

**Interfaces:**
- Consumes: `settings_model::{project_formations, ProbeError, DEFAULT_RANGE, MAX_PROBES}` from Tasks 1–2; `common::user_files()` from `crates/settings-model/tests/common/mod.rs`.
- Produces: nothing other tasks consume.

The real corpus is gitignored, so without a synthetic fixture carrying the key this gate asserts nothing on CI (see `fixtures/README.md`). Add the key to the synthetic account file first.

- [ ] **Step 1: Add the key to the synthetic account file**

In `crates/settings-model/src/bin/gen_fixtures.rs`, find the `user_modern()` entry list containing `(b("scanner_presetInUse"), w(b("hostile"))),` and add immediately after it:

```rust
        // Custom probe formations, in their real shape: a dict of id to
        // (Str name, 8 entries), plus the -4 scratch slot whose name is Bytes.
        // The gate in tests/probes_corpus.rs needs a file carrying the key on
        // CI, where testdata/ is absent by design.
        (b("probescanning.customFormations"), w(Value::Dict(vec![
            (i(0), tup(vec![
                Value::Str("close".into()),
                list((0..8).map(|n| tup(vec![
                    tup(vec![f(1.0e9 * n as f64), f(-115136512.0), f(-415997952.0)]),
                    f(74_798_935_350.0),
                ])).collect()),
            ])),
            // The client's scratch copy of the formation being edited: a
            // negative id AND a Bytes name, both of which a reader must skip.
            (i(-4), tup(vec![
                b("tempFormation"),
                list((0..8).map(|n| tup(vec![
                    tup(vec![f(1.0e9 * n as f64), f(-115136512.0), f(-415997952.0)]),
                    f(74_798_935_350.0),
                ])).collect()),
            ])),
        ]))),
        (b("probescanning.selectedFormationID"), w(i(0))),
```

The helpers used here all already exist in `gen_fixtures.rs`: `b` (Bytes), `i` (Int), `f` (Float), `tup` (Tuple), `list` (List), and `w` (the `(filetime, value)` wrapper, which uses a non-zero stamp — that is deliberate, and the projection must read it the same as a zero one). The loop variable is `n`, not `i`, because `i` is the Int constructor in this scope.

- [ ] **Step 2: Regenerate the fixtures**

Run: `cargo run -p settings-model --bin gen_fixtures`
Expected: rewrites `fixtures/synthetic/profile/settings_Default/core_user_80000001.dat`.

Verify the key landed:

```bash
cargo run -p blue-marshal --bin bmdump -- dump-inline fixtures/synthetic/profile/settings_Default/core_user_80000001.dat | grep -A3 customFormations
```

Expected: the key, then a `(` and a `Long`.

- [ ] **Step 3: Write the corpus gate**

Create `crates/settings-model/tests/probes_corpus.rs`:

```rust
//! Real-data guard for the probe formation projection. Account files intern
//! repeated keys and the root `ui` section as `Shared`/`Ref`, so a reader that
//! matched `Value::Bytes` directly would pass every hand-built unit test in
//! `probes.rs` and still read nothing from a real file.
//!
//! It also locks in the measurements the editor's design rests on (spec §2):
//! every formation holds 8 probes and one uniform range. If a future client
//! changes either, this fails loudly rather than the editor quietly writing a
//! shape nobody checked.
//!
//! Runs against the committed synthetic corpus always, and the real corpus when
//! testdata/ is checked out.

mod common;

use settings_model::{project_formations, ProbeError, DEFAULT_RANGE, MAX_PROBES};

#[test]
fn every_corpus_account_file_projects_or_reports_no_formations() {
    let mut projected = 0;
    let mut formations = 0;
    for f in common::user_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        match project_formations(&doc) {
            Ok(p) => {
                projected += 1;
                formations += p.formations.len();
                for form in &p.formations {
                    assert!(
                        form.id >= 0,
                        "{}: a negative id reached the projection — the -4 scratch slot is the client's, not a user formation",
                        f.path.display(),
                    );
                    assert!(
                        !form.name.is_empty(),
                        "{}: formation {} projected an empty name",
                        f.path.display(), form.id,
                    );
                    assert!(
                        !form.probes.is_empty() && form.probes.len() <= MAX_PROBES,
                        "{}: formation {} projected {} probes",
                        f.path.display(), form.id, form.probes.len(),
                    );
                }
            }
            // A file with no formations at all is legitimate: 61 of 175 corpus
            // account files have never had one saved.
            Err(ProbeError::NoFormations) | Err(ProbeError::NoUi) => {}
            Err(e) => panic!("{}: probe formation projection failed: {e}", f.path.display()),
        }
    }
    assert!(projected > 0, "no account file projected — the corpus walker found nothing");
    assert!(formations > 0, "no formation projected — the synthetic fixture should carry one");
}

#[test]
fn every_real_formation_holds_eight_probes_at_one_uniform_range() {
    // The two measurements the editor's single range field and its 1-8 probe
    // range rest on (spec §2.3, §2.4). Real corpus only: the synthetic fixture
    // is authored to these values, so asserting on it proves nothing.
    if !common::real_corpus_present() {
        return;
    }
    let mut checked = 0;
    for f in common::user_files() {
        if f.synthetic {
            continue;
        }
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        let Ok(p) = project_formations(&doc) else { continue };
        for form in &p.formations {
            checked += 1;
            assert_eq!(
                form.probes.len(), 8,
                "{}: formation {} holds {} probes, not 8 — the corpus has only ever shown 8",
                f.path.display(), form.id, form.probes.len(),
            );
            assert!(
                !form.mixed_range,
                "{}: formation {} has mixed probe ranges — no corpus formation did when this was designed",
                f.path.display(), form.id,
            );
            assert_eq!(
                form.range, DEFAULT_RANGE,
                "{}: formation {} is at {} m, not the 0.5 AU every corpus formation carries",
                f.path.display(), form.id, form.range,
            );
        }
    }
    assert!(checked > 0, "the real corpus is present but carried no formations");
}
```

- [ ] **Step 4: Run the gate**

Run: `cargo test -p settings-model --test probes_corpus`
Expected: PASS. Both tests run; the second returns early unless `testdata/` is checked out.

Then run the whole workspace to confirm the regenerated fixture broke nothing — several gates hash it:

Run: `cargo test --workspace`
Expected: PASS. If a golden-file or corpus-count test fails on the changed fixture, update that test's expected value; do not revert the fixture.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/tests/probes_corpus.rs crates/settings-model/src/bin/gen_fixtures.rs fixtures/synthetic/profile/settings_Default/core_user_80000001.dat
git commit -m "Gate the probe formation projection on real account files

The synthetic account file grows the key, including the -4 scratch slot with
its Bytes name, so the gate asserts something on CI where testdata/ is absent
by design.

The eight-probes-at-one-range assertions run on the real corpus only: the
synthetic fixture is authored to those values, so checking it would prove
nothing about the client.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 4: IPC — Tauri commands and the frontend client

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (add after the `neocom_reset` function, before `#[cfg(test)] mod tests`)
- Modify: `app/src-tauri/src/lib.rs` (three commands + the `invoke_handler` list)
- Modify: `app/src/lib/api.ts` (types + three `api` methods)

**Interfaces:**
- Consumes: `settings_model::{project_formations, set_formation, remove_formation, next_formation_id, Formations, ProbeError}` from Tasks 1–2.
- Produces:
  - Tauri commands `probe_formations`, `set_probe_formation`, `remove_probe_formation`
  - TS: `export type Formation`, `export type Formations`, `api.probeFormations()`, `api.setProbeFormation(id, name, probes, range)`, `api.removeProbeFormation(id)`
  - `api.setProbeFormation` takes `id: number | null` — `null` means "create at the next free id".

- [ ] **Step 1: Write the failing test — by writing the frontend half first**

`app/src/lib/ipc.test.ts` is already the test for this task and needs **no new
check**. It parses `api.ts` and both Rust files and pins them together in four
directions: every command `api.ts` calls exists as a `#[tauri::command]`, every
`#[tauri::command]` is in `generate_handler!`, every Rust command is reachable
from `api.ts`, and the argument names agree modulo `snake_case` → `camelCase`.

So the red state is created by adding the TypeScript side alone. In
`app/src/lib/api.ts`, add near the `Keybinds` types:

```ts
export type Formation = {
  id: number;
  /** Metre offsets from the formation centre. X and Z are horizontal, Y is up. */
  probes: [number, number, number][];
  name: string;
  /** Metres, one per probe, positionally matching `probes`. The file's own
   * values; the editor writes one range back for the whole formation. */
  ranges: number[];
  /** Metres. The first probe's range — what the single range field edits. */
  range: number;
  mixed_range: boolean;
};
export type Formations = { formations: Formation[]; selected: number | null };
```

and to the `api` object, beside `neocomBar`:

```ts
  probeFormations: () => invoke<Formations>("probe_formations"),
  /** `id: null` creates at the next free id. */
  setProbeFormation: (
    id: number | null,
    name: string,
    probes: [number, number, number][],
    range: number,
  ) => invoke<Formations>("set_probe_formation", { id, name, probes, range }),
  removeProbeFormation: (id: number) =>
    invoke<Formations>("remove_probe_formation", { id }),
```

- [ ] **Step 2: Run it to verify it fails**

Run (from `app/`): `npm test`
Expected: FAIL — `every api.ts command exists in Rust (missing: probe_formations, set_probe_formation, remove_probe_formation)`.

- [ ] **Step 3: Write the Rust half**

In `app/src-tauri/src/ops.rs`, add after `neocom_reset`:

```rust
fn probe_err(e: settings_model::ProbeError) -> ErrDto {
    let v = serde_json::to_value(&e).unwrap_or_default();
    ErrDto::new(v.get("code").and_then(|c| c.as_str()).unwrap_or("probes"), e.to_string())
}

pub fn probe_formations(state: &AppState) -> Result<settings_model::Formations, ErrDto> {
    let guard = state.user.lock().unwrap();
    let doc = guard.as_ref().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
    settings_model::project_formations(&doc.value).map_err(probe_err)
}

/// Edit the USER slot's formations, reshare, then re-project them. Mirrors
/// `edit_char_neocom`, on the account side.
fn edit_user_probes<F>(state: &AppState, edit: F) -> Result<settings_model::Formations, ErrDto>
where
    F: FnOnce(&mut blue_marshal::Value) -> Result<(), settings_model::ProbeError>,
{
    {
        let mut guard = state.user.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        edit(&mut doc.value).map_err(probe_err)?;
        doc.value = blue_marshal::reshare(&doc.value);
    }
    probe_formations(state)
}

/// `id: None` creates at the next free id. Resolving it here rather than in the
/// frontend keeps id allocation in one place, next to the rule that produced it.
pub fn set_probe_formation(
    state: &AppState,
    id: Option<i64>,
    name: &str,
    probes: Vec<[f64; 3]>,
    range: f64,
) -> Result<settings_model::Formations, ErrDto> {
    let id = match id {
        Some(i) => i,
        None => settings_model::next_formation_id(&probe_formations(state)?),
    };
    edit_user_probes(state, |v| settings_model::set_formation(v, id, name, &probes, range))
}

pub fn remove_probe_formation(
    state: &AppState,
    id: i64,
) -> Result<settings_model::Formations, ErrDto> {
    edit_user_probes(state, |v| settings_model::remove_formation(v, id))
}
```

If `probe_formations` is called on a document with no formations key at all, it returns the `no_formations` error — the view treats that as "an empty list you can add to" (Task 6), because `set_formation` mints the key.

In `app/src-tauri/src/lib.rs`, add next to the `neocom_*` commands:

```rust
#[tauri::command]
fn probe_formations(state: tauri::State<'_, AppState>) -> Result<settings_model::Formations, ErrDto> {
    ops::probe_formations(&state)
}

#[tauri::command]
fn set_probe_formation(
    state: tauri::State<'_, AppState>,
    id: Option<i64>,
    name: String,
    probes: Vec<[f64; 3]>,
    range: f64,
) -> Result<settings_model::Formations, ErrDto> {
    ops::set_probe_formation(&state, id, &name, probes, range)
}

#[tauri::command]
fn remove_probe_formation(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<settings_model::Formations, ErrDto> {
    ops::remove_probe_formation(&state, id)
}
```

and add `probe_formations, set_probe_formation, remove_probe_formation,` to the `generate_handler![...]` list beside `neocom_bar`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --workspace`
Expected: PASS.

Run (from `app/`): `npm run check && npm test`
Expected: PASS, no type errors.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts app/src/lib/ipc.test.ts
git commit -m "Expose the probe formations over IPC

Three commands on the account slot, mirroring the neocom pattern on the other
side of the file. Id allocation for a new formation resolves in ops rather than
the frontend, so the fill-the-lowest-gap rule stays next to the corpus
observation that produced it.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 5: Pure frontend geometry helpers

**Files:**
- Create: `app/src/lib/probes.ts`
- Create: `app/src/lib/probes.test.ts`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `export const M_PER_AU = 149597870700`, `export const M_PER_KM = 1000`
  - `export type Unit = "au" | "km"`
  - `export function toUnit(metres: number, u: Unit): number`
  - `export function fromUnit(value: number, u: Unit): number`
  - `export interface Spherical { r: number; az: number; el: number }`
  - `export function toSpherical(p: [number, number, number]): Spherical`
  - `export function toCartesian(s: Spherical): [number, number, number]`
  - `export function cubeFormation(range: number): [number, number, number][]`
  - `export function formatUnit(metres: number, u: Unit): string`

No runes here, so this is `node --test`-able like `keybinds.ts` and `layout.ts`.

- [ ] **Step 1: Write the failing test**

Create `app/src/lib/probes.test.ts`:

```ts
// Run: npm test (node --test; Node strips the types). Throw-based checks, no
// framework — matching keybinds.test.ts.
import {
  M_PER_AU,
  DEFAULT_RANGE_M,
  toUnit,
  fromUnit,
  toSpherical,
  toCartesian,
  cubeFormation,
  formatUnit,
} from "./probes.ts";

const check = (name: string, ok: boolean) => {
  if (!ok) throw new Error(`FAIL: ${name}`);
  console.log(`  ok - ${name}`);
};

const near = (a: number, b: number, eps = 1e-6) => Math.abs(a - b) < eps;

// The one number the whole feature is anchored on: every corpus probe entry
// carries 74798935350 m, which must read as exactly 0.5 AU.
check("0.5 AU is the corpus range", near(toUnit(DEFAULT_RANGE_M, "au"), 0.5));
check("the default range is the corpus value", DEFAULT_RANGE_M === 74798935350);
check("AU round-trips", near(fromUnit(toUnit(1234567890, "au"), "au"), 1234567890, 1e-3));
check("km is metres over 1000", toUnit(2500, "km") === 2.5);
check("km round-trips exactly", fromUnit(2.5, "km") === 2500);
check("one AU is EVE's own value", M_PER_AU === 149597870700);

// EVE's axes: X and Z are the horizontal plane, Y is up.
check("a probe on +X is azimuth 0, elevation 0", (() => {
  const s = toSpherical([100, 0, 0]);
  return near(s.r, 100) && near(s.az, 0) && near(s.el, 0);
})());
check("a probe on +Z is azimuth 90", near(toSpherical([0, 0, 100]).az, 90));
check("a probe straight up is elevation 90", near(toSpherical([0, 100, 0]).el, 90));
check("a probe straight down is elevation -90", near(toSpherical([0, -100, 0]).el, -90));

check("cartesian round-trips through spherical", (() => {
  const p: [number, number, number] = [-1199120384, -115136512, -415997952];
  const back = toCartesian(toSpherical(p));
  return p.every((v, i) => near(back[i], v, 1e-3));
})());

// r == 0 leaves the angles undefined. Reporting 0/0 rather than NaN is what
// keeps a zeroed row's fields editable instead of poisoning every derived value.
check("a probe at the centre reports finite angles", (() => {
  const s = toSpherical([0, 0, 0]);
  return s.r === 0 && Number.isFinite(s.az) && Number.isFinite(s.el);
})());

check("a cube formation has eight distinct corners", (() => {
  const c = cubeFormation(74798935350);
  const distinct = new Set(c.map((p) => p.join(",")));
  return c.length === 8 && distinct.size === 8;
})());
check("every cube corner is the same distance from the centre", (() => {
  const c = cubeFormation(74798935350);
  const rs = c.map((p) => toSpherical(p).r);
  return rs.every((r) => near(r, rs[0], 1e-3)) && rs[0] > 0;
})());

// Display text must not round a coordinate to something that reads as zero:
// one metre is 6.7e-12 AU, so a fixed 2-decimal AU display collapses a probe
// 10 000 km out to "0.00".
check("a small distance still shows a value in AU", formatUnit(1e7, "au") !== "0");
check("km formatting is readable", formatUnit(1e7, "km") === "10000");
```

- [ ] **Step 2: Run it to verify it fails**

Run (from `app/`): `node --test "src/lib/probes.test.ts"`
Expected: FAIL — `Cannot find module './probes.ts'`.

- [ ] **Step 3: Write the implementation**

Create `app/src/lib/probes.ts`:

```ts
// Pure geometry and unit helpers for the probe formation editor — no runes, so
// this is node --test-able like keybinds.ts.
//
// METRES ARE THE SOURCE OF TRUTH. Everything here converts for display only.
// One metre is 6.7e-12 AU, so a value that round-trips through a rounded AU
// string comes back displaced; the view converts a field only when the user
// actually types into it (spec §4.2).

/** EVE's own astronomical unit: 0.5 AU is exactly 74798935350 m in every
 * corpus formation, which fixes this value to the metre. */
export const M_PER_AU = 149597870700;
export const M_PER_KM = 1000;

/** 0.5 AU in metres — the range every corpus formation carries, and what a new
 * formation starts at. Mirrors `probes.rs`'s `DEFAULT_RANGE`. */
export const DEFAULT_RANGE_M = 74798935350;

export type Unit = "au" | "km";

const scale = (u: Unit) => (u === "au" ? M_PER_AU : M_PER_KM);

export const toUnit = (metres: number, u: Unit): number => metres / scale(u);
export const fromUnit = (value: number, u: Unit): number => value * scale(u);

export interface Spherical {
  /** Metres from the formation centre. */
  r: number;
  /** Horizontal bearing in degrees, from +X towards +Z. */
  az: number;
  /** Degrees above the horizontal plane. */
  el: number;
}

const DEG = 180 / Math.PI;

/** EVE's axes: X and Z are the horizontal plane, Y is up. */
export function toSpherical([x, y, z]: [number, number, number]): Spherical {
  const r = Math.hypot(x, y, z);
  // At the centre the angles are undefined. Returning 0 rather than NaN keeps a
  // zeroed row's fields editable — NaN would propagate into every derived value
  // and into the SVG, where it silently drops the element.
  return { r, az: Math.atan2(z, x) * DEG, el: r === 0 ? 0 : Math.asin(y / r) * DEG };
}

export function toCartesian({ r, az, el }: Spherical): [number, number, number] {
  const a = az / DEG;
  const e = el / DEG;
  const horizontal = r * Math.cos(e);
  return [horizontal * Math.cos(a), r * Math.sin(e), horizontal * Math.sin(a)];
}

/** The starting arrangement for a new-from-scratch formation: eight probes on
 * the corners of a cube of half-side `range / 2`.
 *
 * ponytail: arbitrary starting cube. EVE ships default formations at several
 * range increments, but none of them are stored in the settings file (spec
 * §2.5), so there is nothing here to derive them from. Replace this if a source
 * for the client's own defaults is ever found. */
export function cubeFormation(range: number): [number, number, number][] {
  const h = range / 2;
  const out: [number, number, number][] = [];
  for (const x of [-h, h]) for (const y of [-h, h]) for (const z of [-h, h]) out.push([x, y, z]);
  return out;
}

/** Display text for a metre value. Trims trailing zeros but keeps enough
 * precision that a probe 10 000 km out never reads as "0.00" in AU. */
export function formatUnit(metres: number, u: Unit): string {
  const v = toUnit(metres, u);
  if (v === 0) return "0";
  const decimals = u === "au" ? 6 : 3;
  return String(Number(v.toFixed(decimals)));
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run (from `app/`): `node --test "src/lib/probes.test.ts"`
Expected: PASS, all checks print `ok - ...`.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/probes.ts app/src/lib/probes.test.ts
git commit -m "Add the probe formation geometry and unit helpers

Cartesian to spherical in EVE's axes (X and Z horizontal, Y up), AU and km
conversion, and the cube a new-from-scratch formation starts from.

toSpherical returns 0 rather than NaN for the angles at the centre: NaN would
propagate through every derived field and silently drop the element from the
SVG rather than showing an obviously wrong number.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 6: The editor view

**Files:**
- Create: `app/src/lib/ProbeFormationsView.svelte`
- Create: `app/src/lib/ProbeFormationsView.spec.ts`
- Modify: `app/src/routes/+page.svelte` (import, `View` union, `viewAvailable`, `active`, the tab button, the render branch)

**Interfaces:**
- Consumes: `api.probeFormations`, `api.setProbeFormation`, `api.removeProbeFormation`, `type Formation`, `type Formations` (Task 4); every export of `probes.ts` (Task 5).
- Produces: a `ProbeFormationsView` component with props
  `{ userOpen: boolean; userId?: number | null; onUserDirty: () => void; onShowAccounts?: () => void; sharedLabel?: string }` — the same prop shape as `KeybindsView`.

- [ ] **Step 1: Write the failing test**

Create `app/src/lib/ProbeFormationsView.spec.ts`:

```ts
// Component test: run with `npm run test:ui` (vitest + jsdom).
import { describe, expect, test } from "vitest";
import { render, fireEvent, screen } from "@testing-library/svelte";
import ProbeFormationsView from "$lib/ProbeFormationsView.svelte";
import { calls } from "$lib/test/setup";
import type { Formations } from "$lib/api";

const noop = () => {};

// A coordinate with bits well below what any rounded AU display can carry.
const AWKWARD: [number, number, number] = [-1199120384.7, -115136512.3, -415997952.9];

const FORMATIONS: Formations = {
  formations: [
    {
      id: 0,
      name: "close",
      probes: [AWKWARD, [1e9, 2e9, 3e9]],
      ranges: [74798935350, 74798935350],
      range: 74798935350,
      mixed_range: false,
    },
  ],
  selected: 0,
};

async function open() {
  calls.stub("probe_formations", FORMATIONS);
  calls.stub("set_probe_formation", FORMATIONS);
  render(ProbeFormationsView, { userOpen: true, userId: 1, onUserDirty: noop });
  await screen.findByDisplayValue("close");
}

/** The arguments of the last set_probe_formation call. */
const lastSet = () => {
  const c = [...calls.log].reverse().find((x) => x.cmd === "set_probe_formation");
  return c?.args as { id: number | null; name: string; probes: number[][]; range: number };
};

describe("precision", () => {
  test("an untouched coordinate is sent back to the metre", async () => {
    // One metre is 6.7e-12 AU. If a displayed, rounded AU string were the
    // source of truth, saving after editing ANY field would displace every
    // other probe in the formation — silently, and on every save.
    await open();
    const nameField = await screen.findByDisplayValue("close");
    await fireEvent.input(nameField, { target: { value: "closer" } });
    await fireEvent.blur(nameField);

    const args = lastSet();
    expect(args.name).toBe("closer");
    expect(args.probes[0]).toEqual(AWKWARD);
  });
});

describe("editing", () => {
  test("typing a distance moves the probe along its existing direction", async () => {
    await open();
    // Probe 1's distance field, doubled. Its angles must not change, so the
    // new position is the old one scaled by two.
    const dist = await screen.findByLabelText("probe 1 distance");
    const before = AWKWARD;
    const r = Math.hypot(...before);
    await fireEvent.input(dist, { target: { value: String((r * 2) / 149597870700) } });
    await fireEvent.blur(dist);

    const p = lastSet().probes[0];
    for (let i = 0; i < 3; i++) expect(p[i]).toBeCloseTo(before[i] * 2, 0);
  });

  test("a mixed-range formation does not offer an edit that would flatten it", async () => {
    calls.stub("probe_formations", {
      formations: [{
        ...FORMATIONS.formations[0],
        ranges: [74798935350, 37399467675],
        mixed_range: true,
      }],
      selected: 0,
    } satisfies Formations);
    render(ProbeFormationsView, { userOpen: true, userId: 1, onUserDirty: noop });
    const range = await screen.findByLabelText("formation range");
    expect((range as HTMLInputElement).disabled).toBe(true);
    // And it says WHICH row differs, not just that one does.
    expect(await screen.findByText(/probes 2/)).toBeTruthy();
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run (from `app/`): `npm run test:ui -- ProbeFormationsView`
Expected: FAIL — the component does not exist.

- [ ] **Step 3: Write the implementation**

Create `app/src/lib/ProbeFormationsView.svelte`. Read `KeybindsView.svelte` first and mirror its shape: the same `$props()` block, the same `reload()` + `$effect` on `userOpen`/`userId`, the same error and empty-state handling, the same `onShowAccounts` nudge when the account file is not open.

```svelte
<script lang="ts">
  import { api, errMessage, type Formation, type Formations } from "./api";
  import { toUnit, fromUnit, toSpherical, toCartesian, cubeFormation, formatUnit,
           DEFAULT_RANGE_M, type Unit } from "./probes";
  import { message } from "@tauri-apps/plugin-dialog";

  let { userOpen, userId = null, onUserDirty, onShowAccounts = () => {}, sharedLabel = "" }:
    { userOpen: boolean; userId?: number | null; onUserDirty: () => void;
      onShowAccounts?: () => void; sharedLabel?: string } = $props();

  /** The projection as loaded. `null` before the first load. */
  let loaded = $state<Formations | null>(null);
  let error = $state<string | null>(null);
  let selectedId = $state<number | null>(null);
  let unit = $state<Unit>("au");

  // THE EDIT BUFFER, IN METRES. Every displayed number is derived from this;
  // nothing derived is ever read back into it. A field the user has not typed
  // into therefore keeps its exact f64 from the file, which is the whole reason
  // this is not bound straight to the inputs (spec §4.2).
  let draftName = $state("");
  let draftRange = $state(0);
  let draftProbes = $state<[number, number, number][]>([]);
  /** Last angles entered per probe, so a probe pulled to r == 0 and back does
   * not silently snap onto the X axis. */
  let lastAngles = $state<{ az: number; el: number }[]>([]);

  const current = $derived(loaded?.formations.find((f) => f.id === selectedId) ?? null);

  async function reload() {
    if (!userOpen) { loaded = null; return; }
    error = null;
    try {
      loaded = await api.probeFormations();
    } catch (e) {
      const code = (e as { code?: string }).code;
      // No formations key at all is an empty list you can add to, not an error:
      // set_probe_formation mints the key.
      if (code === "no_formations") { loaded = { formations: [], selected: null }; }
      else { error = errMessage(e); loaded = null; return; }
    }
    if (!loaded.formations.some((f) => f.id === selectedId)) {
      select(loaded.formations[0] ?? null);
    }
  }
  $effect(() => { void userOpen; void userId; reload(); });

  function select(f: Formation | null) {
    selectedId = f?.id ?? null;
    draftName = f?.name ?? "";
    draftRange = f?.range ?? 0;
    draftProbes = f ? f.probes.map((p) => [...p] as [number, number, number]) : [];
    lastAngles = draftProbes.map((p) => { const s = toSpherical(p); return { az: s.az, el: s.el }; });
  }

  async function commit(id: number | null = selectedId) {
    try {
      loaded = await api.setProbeFormation(id, draftName, draftProbes, draftRange);
      onUserDirty();
      if (id === null) select(loaded.formations[loaded.formations.length - 1] ?? null);
    } catch (e) {
      await message(errMessage(e), { title: "Could not save the formation", kind: "error" });
      await reload();
    }
  }

  /** Replace one cartesian component from a typed display value. */
  function setAxis(i: number, axis: 0 | 1 | 2, text: string) {
    const v = Number(text);
    if (!Number.isFinite(v)) return;
    const next = draftProbes.map((p) => [...p] as [number, number, number]);
    next[i][axis] = fromUnit(v, unit);
    draftProbes = next;
    const s = toSpherical(next[i]);
    if (s.r !== 0) lastAngles[i] = { az: s.az, el: s.el };
  }

  /** Scale a probe to a new distance, preserving its angles. */
  function setDistance(i: number, text: string) {
    const v = Number(text);
    if (!Number.isFinite(v)) return;
    const { az, el } = lastAngles[i] ?? toSpherical(draftProbes[i]);
    const next = draftProbes.map((p) => [...p] as [number, number, number]);
    next[i] = toCartesian({ r: fromUnit(v, unit), az, el });
    draftProbes = next;
  }

  /** Rotate a probe, preserving its distance. */
  function setAngle(i: number, which: "az" | "el", text: string) {
    const v = Number(text);
    if (!Number.isFinite(v)) return;
    const s = toSpherical(draftProbes[i]);
    const angles = { ...(lastAngles[i] ?? { az: s.az, el: s.el }), [which]: v };
    lastAngles[i] = angles;
    const next = draftProbes.map((p) => [...p] as [number, number, number]);
    next[i] = toCartesian({ r: s.r, ...angles });
    draftProbes = next;
  }

  function addProbe() {
    if (draftProbes.length >= 8) return;
    draftProbes = [...draftProbes, [draftRange / 2, 0, 0]];
    lastAngles = [...lastAngles, { az: 0, el: 0 }];
  }

  function removeProbe(i: number) {
    if (draftProbes.length <= 1) return;
    draftProbes = draftProbes.filter((_, j) => j !== i);
    lastAngles = lastAngles.filter((_, j) => j !== i);
  }

  async function createNew() {
    draftName = "New formation";
    draftRange = DEFAULT_RANGE_M;
    draftProbes = cubeFormation(DEFAULT_RANGE_M);
    lastAngles = draftProbes.map((p) => { const s = toSpherical(p); return { az: s.az, el: s.el }; });
    await commit(null);
  }

  async function duplicate() {
    if (!current) return;
    draftName = `${current.name} copy`;
    await commit(null);
  }

  async function remove() {
    if (selectedId === null) return;
    try {
      loaded = await api.removeProbeFormation(selectedId);
      onUserDirty();
      select(loaded.formations[0] ?? null);
    } catch (e) {
      await message(errMessage(e), { title: "Could not delete the formation", kind: "error" });
    }
  }
</script>
```

Then the markup. The `aria-label`s are load-bearing — the component test queries by them — and every value shown is a `formatUnit`/angle derivation of `draftProbes`, never a two-way `bind:`:

```svelte
{#if !userOpen}
  <p class="hint">
    Probe formations live in the account file.
    <button class="link" onclick={onShowAccounts}>Pair this character with its account</button>
    to edit them.
  </p>
{:else if error}
  <p class="error">{error}</p>
{:else if loaded}
  {#if sharedLabel}<p class="banner">{sharedLabel}</p>{/if}
  <div class="probes">
    <aside class="formation-list">
      <ul>
        {#each loaded.formations as f (f.id)}
          <li>
            <button class:active={f.id === selectedId} onclick={() => select(f)}>{f.name}</button>
          </li>
        {/each}
      </ul>
      <button onclick={createNew}>New</button>
      <button onclick={duplicate} disabled={!current}>Duplicate</button>
      <button onclick={remove} disabled={!current}>Delete</button>
    </aside>

    {#if current}
      <section class="formation">
        <div class="row">
          <label>
            Name
            <input value={draftName}
                   oninput={(e) => (draftName = e.currentTarget.value)}
                   onblur={() => commit()} />
          </label>
          <label>
            Range
            <input aria-label="formation range"
                   value={formatUnit(draftRange, unit)}
                   disabled={current.mixed_range}
                   oninput={(e) => {
                     const v = Number(e.currentTarget.value);
                     if (Number.isFinite(v)) draftRange = fromUnit(v, unit);
                   }}
                   onblur={() => commit()} />
          </label>
          <span class="units">
            <button class:active={unit === "au"} onclick={() => (unit = "au")}>AU</button>
            <button class:active={unit === "km"} onclick={() => (unit = "km")}>km</button>
          </span>
        </div>

        {#if current.mixed_range}
          <p class="warn">
            This formation's probes carry different ranges
            ({mixedProbeLabel}). Editing the range here would flatten them, so it is
            locked. No formation EVE has been seen to save does this.
          </p>
        {/if}

        <table>
          <thead>
            <tr>
              <th>#</th><th>X</th><th>Y</th><th>Z</th>
              <th>dist</th><th>az°</th><th>el°</th><th></th>
            </tr>
          </thead>
          <tbody>
            {#each draftProbes as p, n}
              {@const s = toSpherical(p)}
              <tr class:selected={selectedProbe === n} onfocusin={() => (selectedProbe = n)}>
                <td>{n + 1}</td>
                {#each [0, 1, 2] as axis}
                  <td>
                    <input aria-label={`probe ${n + 1} ${"XYZ"[axis]}`}
                           value={formatUnit(p[axis], unit)}
                           oninput={(e) => setAxis(n, axis as 0 | 1 | 2, e.currentTarget.value)}
                           onblur={() => commit()} />
                  </td>
                {/each}
                <td>
                  <input aria-label={`probe ${n + 1} distance`}
                         value={formatUnit(s.r, unit)}
                         oninput={(e) => setDistance(n, e.currentTarget.value)}
                         onblur={() => commit()} />
                </td>
                <td>
                  <input aria-label={`probe ${n + 1} azimuth`}
                         value={s.az.toFixed(1)}
                         oninput={(e) => setAngle(n, "az", e.currentTarget.value)}
                         onblur={() => commit()} />
                </td>
                <td>
                  <input aria-label={`probe ${n + 1} elevation`}
                         value={s.el.toFixed(1)}
                         oninput={(e) => setAngle(n, "el", e.currentTarget.value)}
                         onblur={() => commit()} />
                </td>
                <td>
                  <button class="mini" title="Remove this probe"
                          disabled={draftProbes.length <= 1}
                          onclick={() => { removeProbe(n); commit(); }}>×</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
        <button onclick={() => { addProbe(); commit(); }} disabled={draftProbes.length >= 8}>
          + probe
        </button>
        <span class="meta">{draftProbes.length} of 8</span>
      </section>
    {:else}
      <p class="hint">This account has no custom probe formations yet.</p>
    {/if}
  </div>
{/if}
```

Two things the script needs for that markup — add them beside the other state:

```ts
  /** Which probe row is focused, so a row and its dots highlight together in
   * both panes (Task 7). */
  let selectedProbe = $state<number | null>(null);

  /** The probes whose range differs from the first, 1-based to match the table.
   * `ranges` is the file's own per-probe values, which is why this can name
   * rows rather than just reporting that they disagree (spec §4.3). */
  const mixedProbeLabel = $derived.by(() => {
    const rs = current?.ranges ?? [];
    const odd = rs.map((r, n) => (r === rs[0] ? null : n + 1)).filter((n) => n !== null);
    return odd.length ? `probes ${odd.join(", ")}` : "";
  });
```

Wire it into `app/src/routes/+page.svelte`:

1. `import ProbeFormationsView from "$lib/ProbeFormationsView.svelte";`
2. `type View = "tree" | "layout" | "overview" | "autofill" | "keybinds" | "probes";`
3. In `active`, add `"probes"` beside `"autofill"` and `"keybinds"` so the account file becomes the active slot:
   `(view === "autofill" || view === "keybinds" || view === "probes") && slots.user?.status === "opened" ? "user"`
4. In `viewAvailable`, add:
   `(v === "probes" && (openCharId !== null || slots.user?.status === "opened"))`
5. A tab button beside the Keybinds one:
   `{#if openCharId !== null || slots.user?.status === "opened"}<button class:active={view === "probes"} onclick={() => (view = "probes")}>Probes</button>{/if}`
6. A render branch beside the keybinds one:

```svelte
      {:else if view === "probes"}
        <div class="tree-area">
          <ProbeFormationsView
            userOpen={slots.user?.status === "opened"}
            userId={openUserId}
            sharedLabel={sharedLabel}
            onShowAccounts={() => (mainView = "accounts")}
            onUserDirty={() => (dirtySlots.user = true)} />
        </div>
```

- [ ] **Step 4: Run the tests to verify they pass**

Run (from `app/`): `npm run test:ui -- ProbeFormationsView`
Expected: PASS, 3 tests.

Run (from `app/`): `npm run check && npm test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/ProbeFormationsView.svelte app/src/lib/ProbeFormationsView.spec.ts app/src/routes/+page.svelte
git commit -m "Add the probe formation editor view

Edit a formation's name, range and probes, create one from scratch or as a
copy, and delete. Each probe is editable three ways onto the same metres:
cartesian, distance from the centre with the angles held, and angles with the
distance held.

The edit buffer holds metres and every field is derived from it. Binding the
inputs directly would make a rounded AU string the source of truth, and since
one metre is 6.7e-12 AU, editing any one field would then displace every other
probe in the formation on save. The component test asserts an untouched
coordinate comes back to the metre.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 7: The visualiser

**Files:**
- Modify: `app/src/lib/ProbeFormationsView.svelte` (a `<svg>` block per pane, plus a `pane` helper in its script)
- Modify: `app/src/lib/probes.ts` (add `paneScale` and `project`)
- Modify: `app/src/lib/probes.test.ts` (tests for both)

**Interfaces:**
- Consumes: `toSpherical` (Task 5), `draftProbes` and `draftRange` (Task 6).
- Produces:
  - `export type Plane = "top" | "side"` — `"top"` is X/Z, `"side"` is X/Y
  - `export function paneScale(probes: [number, number, number][], range: number, size: number): number` — data metres per pane pixel
  - `export function project(p: [number, number, number], plane: Plane): [number, number]`

- [ ] **Step 1: Write the failing tests**

Append to `app/src/lib/probes.test.ts`:

```ts
import { paneScale, project } from "./probes.ts";

// Top-down is X/Z, side is X/Y. Getting these the wrong way round draws a
// plausible picture of the wrong formation, which no type check would catch.
check("top-down drops Y", (() => {
  const [a, b] = project([1, 2, 3], "top");
  return a === 1 && b === 3;
})());
check("side drops Z", (() => {
  const [a, b] = project([1, 2, 3], "side");
  return a === 1 && b === 2;
})());

check("the scale fits the widest probe plus its range", (() => {
  // A probe 100 units out with a range of 10 needs 110 of half-extent to show
  // its whole sphere, so a 200px pane fits 110 into 100px.
  const s = paneScale([[100, 0, 0]], 10, 200);
  return near(s, 110 / 100, 1e-9);
})());
check("an all-centre formation still yields a finite scale", (() => {
  const s = paneScale([[0, 0, 0]], 0, 200);
  return Number.isFinite(s) && s > 0;
})());
```

- [ ] **Step 2: Run to verify they fail**

Run (from `app/`): `node --test "src/lib/probes.test.ts"`
Expected: FAIL — `paneScale`/`project` are not exported.

- [ ] **Step 3: Write the implementation**

Append to `app/src/lib/probes.ts`:

```ts
/** Which two axes a pane shows. EVE's X and Z are the horizontal plane, Y is
 * up, so "top" is a map and "side" is an elevation. */
export type Plane = "top" | "side";

/** Drop the third axis. */
export function project(p: [number, number, number], plane: Plane): [number, number] {
  return plane === "top" ? [p[0], p[2]] : [p[0], p[1]];
}

/** Data metres per pane pixel, sized so every probe's whole range sphere fits.
 *
 * Both panes take the same scale from the same call, so a distance that looks
 * longer in one is longer. */
export function paneScale(
  probes: [number, number, number][],
  range: number,
  size: number,
): number {
  const reach = Math.max(0, ...probes.flatMap((p) => p.map(Math.abs))) + Math.abs(range);
  // A formation with every probe at the centre and no range has nothing to
  // scale to; any positive number draws it as a dot at the origin.
  if (reach === 0) return 1;
  return reach / (size / 2);
}
```

In `ProbeFormationsView.svelte`, add to the script:

```ts
  import { paneScale, project, type Plane } from "./probes";

  const PANE = 320; // px, both panes square and identical
  const scale = $derived(paneScale(draftProbes, draftRange, PANE));

  /** Pane pixel coordinates for a probe, origin at the pane centre. */
  const at = (p: [number, number, number], plane: Plane) => {
    const [a, b] = project(p, plane);
    return { cx: PANE / 2 + a / scale, cy: PANE / 2 - b / scale };
  };
```

and the markup, once per plane (`"top"` labelled *top-down (X/Z)*, `"side"` labelled *side (X/Y)*):

```svelte
<figure class="pane">
  <figcaption>top-down (X/Z)</figcaption>
  <svg viewBox="0 0 {PANE} {PANE}" width={PANE} height={PANE} role="img"
       aria-label="top-down view of the formation">
    <line x1={PANE / 2} y1="0" x2={PANE / 2} y2={PANE} class="axis" />
    <line x1="0" y1={PANE / 2} x2={PANE} y2={PANE / 2} class="axis" />
    {#each draftProbes as p, i}
      {@const c = at(p, "top")}
      <circle cx={c.cx} cy={c.cy} r={draftRange / scale} class="range" />
      <circle cx={c.cx} cy={c.cy} r="4" class="probe" class:selected={selectedProbe === i} />
    {/each}
  </svg>
</figure>
```

Add `let selectedProbe = $state<number | null>(null);` to the script and set it from each probe row's focus handler, so a row and its dots highlight together in both panes.

- [ ] **Step 4: Run the tests to verify they pass**

Run (from `app/`): `node --test "src/lib/probes.test.ts"` — Expected: PASS.
Run (from `app/`): `npm run check && npm test` — Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/probes.ts app/src/lib/probes.test.ts app/src/lib/ProbeFormationsView.svelte
git commit -m "Draw the formation in a top-down and a side view

Two orthographic SVG panes on one shared scale, each probe's range sphere drawn
as a circle. Two rather than one because a real formation is a horizontal ring
plus a vertical column: the column's probes share their X and Z, so a top-down
view alone draws three probes as one dot.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 8: The batch category

**Files:**
- Modify: `crates/settings-model/src/batch.rs` (`Category` enum + `key_path`)
- Modify: `app/src-tauri/src/ops.rs` (`Aspect` enum + `aspect_writes`)
- Modify: `app/src-tauri/src/presets.rs` (`derive_aspects`)
- Modify: `app/src/lib/api.ts` (`Aspect` union)
- Modify: `app/src/lib/presetLibrary.ts` (`LABELS`)
- Modify: `app/src/lib/BatchView.svelte` (`offered` list + `ASPECTS`)
- Modify: `app/src/lib/PresetGroup.svelte` (its aspect list)
- Test: `crates/settings-model/src/batch.rs` inline tests, `app/src/lib/presetLibrary.test.ts`

**Interfaces:**
- Consumes: nothing from earlier tasks — this is independent of the editor and could ship alone.
- Produces: `Category::ProbeFormations`, `Aspect::ProbeFormations`, TS `"probe_formations"` aspect string.

The serde rename is `snake_case` on both enums, so the wire string is `probe_formations`.

- [ ] **Step 1: Write the failing tests**

In `crates/settings-model/src/batch.rs`, inside its `mod tests`, add:

```rust
    #[test]
    fn the_probe_formation_category_is_a_two_level_ui_key() {
        // The dot is part of the key NAME. A three-level path through a
        // `probescanning` section finds nothing and silently copies nothing.
        assert_eq!(
            Category::ProbeFormations.key_path(),
            &[b"ui".as_slice(), b"probescanning.customFormations".as_slice()],
        );
    }

    #[test]
    fn an_absent_formation_set_never_deletes_the_targets() {
        // Whole-section categories skip on absence. Only the leaf HUD keys read
        // absence as "EVE's default" and delete; a source that has never saved
        // a formation must not wipe the target's.
        assert!(!Category::ProbeFormations.absent_means_default());
    }
```

In `app/src-tauri/src/ops.rs`, inside its `mod tests`, add:

```rust
    #[test]
    fn probe_formations_write_the_account_side_only() {
        let w = aspect_writes(&[Aspect::ProbeFormations]);
        assert_eq!(w.account_categories, vec![Category::ProbeFormations]);
        assert!(w.char_categories.is_empty());
        assert!(!w.char_full_copy && !w.account_full_copy);
    }
```

In `app/src/lib/presetLibrary.test.ts`, add:

```ts
check("probe formations label", aspectLabel("probe_formations") === "Probe formations");
check(
  "probe formations summarise alongside others",
  summarise(info("P", ["keybinds", "probe_formations"])) === "Keybinds · Probe formations",
);
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --workspace` — Expected: FAIL, `no variant named ProbeFormations`.
Run (from `app/`): `npm test` — Expected: FAIL on the label checks.

- [ ] **Step 3: Write the implementation**

`crates/settings-model/src/batch.rs` — add to the `Category` enum after `Keybinds`:

```rust
    /// Custom probe scanner formations, account-side. The dot in
    /// `probescanning.customFormations` is part of the key NAME, so this is a
    /// two-level path under `ui`, not three levels through a `probescanning`
    /// section (probes.rs, spec §2.1).
    ProbeFormations,
```

and to `key_path`:

```rust
            Category::ProbeFormations => &[b"ui", b"probescanning.customFormations"],
```

Leave `absent_means_default` alone — the `matches!` there does not list this variant, so it is already `false`.

`app/src-tauri/src/ops.rs` — add to `Aspect` after `Keybinds`:

```rust
    ProbeFormations,
```

and to the `match` in `aspect_writes`:

```rust
            Aspect::ProbeFormations => account_categories.push(Category::ProbeFormations),
```

`app/src-tauri/src/presets.rs` — add to `derive_aspects`, after the `Keybinds` block:

```rust
    if has_category(user_doc, Category::ProbeFormations) {
        out.push(Aspect::ProbeFormations);
    }
```

`app/src/lib/api.ts`:

```ts
export type Aspect = "layout" | "overview" | "autofill" | "keybinds" | "probe_formations" | "everything";
```

`app/src/lib/presetLibrary.ts` — add to `LABELS`:

```ts
  probe_formations: "Probe formations",
```

`app/src/lib/BatchView.svelte` — add `"probe_formations"` to the character-source `offered` array, and to `ASPECTS`:

```ts
    { key: "probe_formations", label: "Probe formations (custom scan formations)", account: true },
```

`app/src/lib/PresetGroup.svelte` — add to its aspect list:

```ts
    { key: "probe_formations", label: "Probe formations", needsUser: true },
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --workspace` — Expected: PASS.
Run (from `app/`): `npm run check && npm test` — Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/batch.rs app/src-tauri/src/ops.rs app/src-tauri/src/presets.rs app/src/lib/api.ts app/src/lib/presetLibrary.ts app/src/lib/BatchView.svelte app/src/lib/PresetGroup.svelte app/src/lib/presetLibrary.test.ts
git commit -m "Carry probe formations in the batch tool and in presets

A whole-section account category, so a source that has never saved a formation
skips rather than deleting the target's — the absent-means-default rule stays
for the leaf HUD keys only.

selectedFormationID is deliberately not carried. It is 0 in every corpus file
that has it and a copy brings the ids along with the formations, so copying it
is a no-op on today's data and an override of a per-account preference on any
data where it is not.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 9: The drifter wormhole overlay

**Files:**
- Modify: `app/src/lib/probes.ts` (the `DRIFTER` constants and `drifterMarkers`)
- Modify: `app/src/lib/probes.test.ts`
- Modify: `app/src/lib/ProbeFormationsView.svelte` (a toggle and the overlay markup)

**Interfaces:**
- Consumes: `toCartesian`, `project`, `paneScale` (Tasks 5, 7).
- Produces:
  - `export const DRIFTER: { distance: number; elevation: number; jumpRange: number }` — metres and degrees
  - `export function drifterHole(): [number, number, number]`

- [ ] **Step 1: Write the failing test**

Append to `app/src/lib/probes.test.ts`:

```ts
import { DRIFTER, drifterHole } from "./probes.ts";

check("the drifter hole sits 89 km out", near(Math.hypot(...drifterHole()), 89_000, 1e-6));
check("the drifter hole is below the warp-in", drifterHole()[1] < 0);
check("the jump sphere is 16 km across", DRIFTER.jumpRange === 16_000);
```

- [ ] **Step 2: Run to verify it fails**

Run (from `app/`): `node --test "src/lib/probes.test.ts"`
Expected: FAIL — `DRIFTER` is not exported.

- [ ] **Step 3: Write the implementation**

Append to `app/src/lib/probes.ts`:

```ts
/** Drifter wormhole k-space geometry, relative to the warp-in beacon.
 *
 * SOURCED, NOT MEASURED, and the sources disagree:
 *
 * | source                                    | warp-in to hole |
 * |-------------------------------------------|-----------------|
 * | jambeeno.com/uni                          | exactly 89 km, 14 deg outside / 26.5 deg in, 16 km jump sphere |
 * | wiki.eveuniversity.org/Wormholes          | ~80 km |
 * | patch-note summary, March 2026            | 75 km k-space side, was 88 km |
 * | randomevestuff.wordpress.com              | ~100 km, one measured at 91 km |
 *
 * Jambeeno's is taken: the only full 3D geometry and the most recent. The
 * direction of the 14 degrees is an assumption — the source says "a slight
 * downward angle" without saying from which end.
 *
 * ponytail: hardcoded k-space drifter geometry, unverified in-client. Make it a
 * measured or user-entered scenario if a second site is ever wanted. */
export const DRIFTER = {
  /** Metres from the warp-in beacon to the hole's centre. */
  distance: 89_000,
  /** Degrees below the horizontal, from the beacon. */
  elevation: -14,
  /** Metres. A hole has a 3 km radius and a 5 km jump range, so it is enterable
   * from anywhere in a 16 km-wide sphere centred on its icon. */
  jumpRange: 16_000,
} as const;

/** The hole's position, with the warp-in beacon (and the formation centre) at
 * the origin. Azimuth 0 is arbitrary: nothing orients a formation to a site. */
export function drifterHole(): [number, number, number] {
  return toCartesian({ r: DRIFTER.distance, az: 0, el: DRIFTER.elevation });
}
```

In `ProbeFormationsView.svelte`, add `let showDrifter = $state(false);` and a checkbox labelled *Drifter wormhole*. Inside each pane's `<svg>`, after the probes, render when `showDrifter`:

```svelte
      {#if showDrifter}
        {@const h = at(drifterHole(), "top")}
        <circle cx={PANE / 2} cy={PANE / 2} r="5" class="warp-in" />
        <line x1={PANE / 2} y1={PANE / 2} x2={h.cx} y2={h.cy} class="drifter-axis" />
        <circle cx={h.cx} cy={h.cy} r={DRIFTER.jumpRange / scale} class="jump-range" />
        <circle cx={h.cx} cy={h.cy} r="4" class="hole" />
      {/if}
```

Repeat in the side pane with `"side"`. Include `drifterHole` and `DRIFTER` in the `./probes` import.

Because the hole at 89 km is far smaller than a 0.5 AU formation, `paneScale` must not shrink to it. It already does not — it scales to the probes and their range, so the overlay simply draws near the origin on a wide formation. Add a note in the UI saying so rather than rescaling:

> The drifter geometry is 89 km across; a 0.5 AU formation is about 800 times wider.

- [ ] **Step 4: Run the tests to verify they pass**

Run (from `app/`): `node --test "src/lib/probes.test.ts"` — Expected: PASS.
Run (from `app/`): `npm run check && npm test` — Expected: PASS.
Run: `cargo test --workspace` — Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/probes.ts app/src/lib/probes.test.ts app/src/lib/ProbeFormationsView.svelte
git commit -m "Overlay the drifter wormhole geometry on the formation views

The warp-in beacon at the formation centre, the hole 89 km out on a 14 degree
downward axis, and its 16 km jump sphere.

These numbers are sourced, not measured, and the four sources found disagree
between 75 and 100 km. The constant block records the disagreement so 89 km is
never read back as a fact this project established, and the direction of the
14 degrees is flagged as an assumption.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

## Final verification

- [ ] Run the full suite: `cargo test --workspace` and, from `app/`, `npm run check && npm test`
- [ ] Add a CHANGELOG entry in the house style (one-line summary, single-line feature bullets, no engineering detail — see `docs/superpowers/plans/` neighbours and the existing CHANGELOG):

```markdown
- Edit the probe scanner's custom formations, in AU or km.
- Copy probe formations between accounts in the batch tool.
```

- [ ] Confirm in-client that a formation with fewer than 8 probes loads (spec §2.4 — the one shape written here that the corpus has never shown). If it does not, restrict `MAX_PROBES` handling to exactly 8 and note it in the spec.
