# Probe formation sharing — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a probe formation leave the account file as legible text — one formation through the clipboard with Ctrl-C/Ctrl-V, or any number through a `.yaml` file — and come back into any other account file.

**Architecture:** A new `crates/settings-model/src/probe_pack.rs` owns the text format end to end (emit and parse), mirroring how `overview_pack.rs` sits beside `overview_states.rs` — `probes.rs` keeps speaking the marshal document, `probe_pack.rs` speaks YAML. Five thin Tauri commands expose it; only the last of them, `add_probe_formations`, touches the document, and it is where the "add at a fresh id, suffix a colliding name" rule lives so Paste and Import cannot disagree. In the frontend a single `visible` derived — the loaded projection with the selected formation's uncommitted draft substituted in — is the source for everything that leaves the view, and one `FormationPicker.svelte` modal serves both Export and Import.

**Tech Stack:** Rust (`yaml-rust2`, already a `settings-model` dependency), Tauri 2 commands, Svelte 5 runes, `node --test` for pure TS, vitest + jsdom for components.

**Spec:** `docs/superpowers/specs/2026-08-04-probe-formation-sharing-design.md`. Section references below (§2.3, §4.2, …) are to that document. It builds on `2026-08-03-probe-formation-editor-design.md` (*the editor spec*) and `2026-08-04-probe-3d-viewer-design.md`.

## Global Constraints

- **Metres, exactly as stored, in the exchange format.** No AU or km conversion of a value that will be read back. One metre is 6.7e-12 AU, so a converting format displaces every probe of every formation that round-trips through it. Legibility comes from **comments**, which the parser skips (§2.1).
- **No ids in the format.** An id is the account-local key of the `customFormations` dict; an import allocates a fresh one (§2.2).
- **Imports are additive.** Nothing existing is replaced or deleted by any path in this plan (§4.3).
- **`ranges:` wins over `range:` when present.** A formation whose probes disagree must never be flattened to one value (§2.3).
- **Validate the whole batch before writing any of it.** `add_probe_formations` checks every spec before the first one is inlined, so a bad entry cannot leave half an import applied (§4.2).
- **Validate before inlining.** In `probes.rs`, every rejection happens before `inline_all(v)`, so a rejected write leaves the document byte-for-byte as it was. Existing tests assert this.
- **No new dependencies.** `yaml-rust2` is already in `crates/settings-model/Cargo.toml`; the clipboard uses DOM APIs only (§5.4).
- **A formation holds 1 to 8 probes** (`MAX_PROBES = 8`) and needs a non-empty trimmed name.
- **Commit message style is this repo's own**: an imperative sentence, no `feat:`/`fix:` prefix. See `git log`. End every commit message with:
  `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>`
- Commands: `cargo test -p settings-model`, `cargo test -p app` (from `app/src-tauri`), `npm test` (in `app/` — runs `node --test` then vitest), `npm run check` (svelte-check, in `app/`).
- Branch: `probe-formation-sharing`, already created; the spec is already committed on it. Commit after every task.

---

## File Structure

| file | responsibility | change |
|---|---|---|
| `crates/settings-model/src/probe_pack.rs` | **new** — the exchange format: emit, parse, name collisions | create |
| `crates/settings-model/src/probes.rs` | the formation model and its read/write | modify: two `ProbeError` variants, extract `check_formation` |
| `crates/settings-model/src/lib.rs` | crate re-exports | modify: `mod probe_pack;` and its `pub use` |
| `app/src-tauri/src/ops.rs` | slot-aware command bodies | modify: five new functions + tests |
| `app/src-tauri/src/lib.rs` | Tauri command signatures and registration | modify |
| `app/src/lib/api.ts` | IPC types and wrappers | modify: `FormationSpec` + five wrappers |
| `app/src/lib/FormationPicker.svelte` | **new** — the checkbox modal, used by Export and Import | create |
| `app/src/lib/FormationPicker.spec.ts` | **new** — vitest component test | create |
| `app/src/lib/ProbeFormationsView.svelte` | list, table, IPC wiring | modify: `visible`, four buttons, shortcuts, picker |
| `app/src/lib/ProbeFormationsView.spec.ts` | vitest component test | modify |
| `CHANGELOG.md` | release notes | modify |

---

## Task 1: The exchange format

**Files:**
- Create: `crates/settings-model/src/probe_pack.rs`
- Modify: `crates/settings-model/src/probes.rs`
- Modify: `crates/settings-model/src/lib.rs`

**Interfaces:**
- Consumes: `crate::probes::ProbeError`.
- Produces:
  - `pub struct FormationSpec { pub name: String, pub probes: Vec<[f64; 3]>, pub ranges: Vec<f64> }` — `Debug, Clone, PartialEq, Serialize, Deserialize`
  - `pub fn emit_formations(specs: &[FormationSpec]) -> String`
  - `pub fn parse_formations(text: &str) -> Result<Vec<FormationSpec>, ProbeError>`
  - `pub fn unique_name(existing: &[String], want: &str) -> String`
  - `pub fn check_formation(name: &str, probes: &[[f64; 3]], ranges: &[f64]) -> Result<(), ProbeError>` (in `probes.rs`)
  - `ProbeError::BadYaml { message: String }`, `ProbeError::NotFormations`

- [ ] **Step 1: Add the two error variants and extract the validation**

In `crates/settings-model/src/probes.rs`, add to the `ProbeError` enum, after `BadName`:

```rust
    /// Shared text that is not valid YAML, or valid YAML in a shape a
    /// formation cannot be read out of.
    BadYaml { message: String },
    /// Valid YAML with no top-level `formations:` list — the user picked the
    /// wrong file (sharing spec §2.4).
    NotFormations,
```

Add to the `Display` impl's match, after the `BadName` arm:

```rust
            ProbeError::BadYaml { message } => {
                write!(f, "This is not a readable formation file: {message}")
            }
            ProbeError::NotFormations => write!(f, "This file contains no probe formations."),
```

Add above `set_formation`:

```rust
/// The rules a formation must satisfy before anything is written. Split out of
/// `set_formation` so a BATCH can check every member before the first one is
/// inlined — otherwise a bad entry halfway down an import leaves half of it
/// applied (sharing spec §4.2).
pub fn check_formation(name: &str, probes: &[[f64; 3]], ranges: &[f64]) -> Result<(), ProbeError> {
    if name.trim().is_empty() {
        return Err(ProbeError::BadName);
    }
    if probes.is_empty() || probes.len() > MAX_PROBES {
        return Err(ProbeError::BadProbeCount);
    }
    if ranges.len() != probes.len() {
        return Err(ProbeError::BadRangeCount);
    }
    Ok(())
}
```

Replace the three checks inside `set_formation` (the `name.trim()`, `probes.is_empty()` and `ranges.len()` blocks) with a call, leaving the `id < 0` check where it is:

```rust
    if id < 0 {
        return Err(ProbeError::NoSuchFormation); // never the -4 scratch slot
    }
    check_formation(name, probes, ranges)?;
    inline_all(v);
```

- [ ] **Step 2: Run the existing tests to confirm nothing regressed**

Run: `cargo test -p settings-model probes`
Expected: PASS — the extraction is behaviour-preserving, and `a_rejected_write_leaves_the_document_untouched` is the guard on that.

- [ ] **Step 3: Write the failing tests for the new module**

Create `crates/settings-model/src/probe_pack.rs` with only this test module for now (the file will not compile until Step 5 — that is the point):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// The corpus's own "close" coordinates, plus a probe with bits well below
    /// what any rounded display could carry.
    fn close() -> FormationSpec {
        FormationSpec {
            name: "close".into(),
            probes: vec![
                [-1199120384.0, -115136512.0, -415997952.0],
                [-1199120384.7, 16133054464.3, -415997952.9],
            ],
            ranges: vec![74_798_935_350.0, 74_798_935_350.0],
        }
    }

    #[test]
    fn a_round_trip_preserves_every_coordinate_bit_for_bit() {
        // The whole reason the format is metres. A shared formation that comes
        // back displaced is a silent corruption, and 1e-7 of an AU is 15 km.
        let out = parse_formations(&emit_formations(&[close()])).unwrap();
        assert_eq!(out, vec![close()]);
    }

    #[test]
    fn a_uniform_formation_does_not_emit_a_ranges_key() {
        let text = emit_formations(&[close()]);
        assert!(text.contains("range: 74798935350"), "got:\n{text}");
        assert!(!text.contains("ranges:"), "a uniform formation needs no per-probe list:\n{text}");
    }

    #[test]
    fn whole_metres_are_written_without_a_decimal_point() {
        // Every corpus coordinate is integral, and legibility is this format's
        // other job — a trailing `.0` on all twenty-four of them is noise.
        let text = emit_formations(&[close()]);
        assert!(!text.contains("74798935350.0"), "got:\n{text}");
        assert!(text.contains("-1199120384,"), "got:\n{text}");
        // …and a fractional coordinate still keeps every bit it needs.
        assert!(text.contains("-1199120384.7,"), "got:\n{text}");
    }

    #[test]
    fn a_mixed_formation_emits_ranges_and_survives_the_round_trip() {
        // Flattening a mix to one value is exactly what the editor refuses to
        // do (editor spec §2.3); the exchange format must not reintroduce it.
        let mut f = close();
        f.ranges = vec![74_798_935_350.0, 149_597_870_700.0];
        let text = emit_formations(&[f.clone()]);
        assert!(text.contains("ranges: ["), "got:\n{text}");
        assert_eq!(parse_formations(&text).unwrap(), vec![f]);
    }

    #[test]
    fn a_hand_typed_document_parses() {
        // §2.1 promises the format is hand-editable: no comments, no column
        // alignment, no `ranges`, and a range that is a plain integer.
        let text = "\
formations:
  - name: quick
    range: 74798935350
    probes:
      - [1, 2, 3]
      - [-4.5, 0, 6e3]
";
        let out = parse_formations(text).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "quick");
        assert_eq!(out[0].probes, vec![[1.0, 2.0, 3.0], [-4.5, 0.0, 6000.0]]);
        assert_eq!(out[0].ranges, vec![74_798_935_350.0, 74_798_935_350.0]);
    }

    #[test]
    fn comments_are_ignored_including_ones_that_look_like_data() {
        let text = "\
# formations: not really
formations:
  - name: c   # range: 999
    range: 100   # 0.000001 AU
    probes:
      - [1, 2, 3]   # 4 km
";
        let out = parse_formations(text).unwrap();
        assert_eq!(out[0].name, "c");
        assert_eq!(out[0].ranges, vec![100.0]);
    }

    #[test]
    fn several_formations_round_trip_in_order() {
        let mut second = close();
        second.name = "on grid".into();
        second.probes = vec![[1.0, 0.0, 0.0]];
        second.ranges = vec![598_391_482_800.0];
        let both = vec![close(), second];
        assert_eq!(parse_formations(&emit_formations(&both)).unwrap(), both);
    }

    #[test]
    fn a_name_with_a_quote_survives() {
        let mut f = close();
        f.name = "bob's 'best'".into();
        assert_eq!(parse_formations(&emit_formations(&[f.clone()])).unwrap(), vec![f]);
    }

    #[test]
    fn junk_is_bad_yaml_and_the_wrong_file_is_not_formations() {
        assert!(matches!(parse_formations("\t- ["), Err(ProbeError::BadYaml { .. })));
        // A real overview pack: valid YAML, valid mapping, wrong contents.
        assert_eq!(parse_formations("presets:\n  - a\n"), Err(ProbeError::NotFormations));
        assert_eq!(parse_formations("just a string\n"), Err(ProbeError::NotFormations));
    }

    #[test]
    fn a_malformed_entry_is_reported_not_skipped() {
        // Skipping would hand the user a partial import they did not ask for.
        let missing_probe = "formations:\n  - name: a\n    range: 1\n    probes:\n      - [1, 2]\n";
        assert!(matches!(parse_formations(missing_probe), Err(ProbeError::BadYaml { .. })));
        let no_range = "formations:\n  - name: a\n    probes:\n      - [1, 2, 3]\n";
        assert!(matches!(parse_formations(no_range), Err(ProbeError::BadYaml { .. })));
        let no_name = "formations:\n  - range: 1\n    probes:\n      - [1, 2, 3]\n";
        assert!(matches!(parse_formations(no_name), Err(ProbeError::BadYaml { .. })));
    }

    #[test]
    fn unique_name_suffixes_only_on_a_collision() {
        let held = vec!["close".to_string(), "close copy".to_string()];
        assert_eq!(unique_name(&held, "on grid"), "on grid");
        assert_eq!(unique_name(&held, "close"), "close copy 2");
        assert_eq!(unique_name(&[], "close"), "close");
        assert_eq!(unique_name(&["close".to_string()], "close"), "close copy");
    }
}
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test -p settings-model probe_pack`
Expected: FAIL — `cannot find function emit_formations`, and the module is not declared yet.

- [ ] **Step 5: Write the module**

Put this **above** the `#[cfg(test)] mod tests` block in `crates/settings-model/src/probe_pack.rs`:

```rust
//! The probe-formation exchange format: the YAML text behind Copy/Paste and
//! Export/Import. `probes.rs` speaks the marshal document; this module speaks
//! YAML, and all format knowledge stays on this side of that line — the same
//! split `overview_pack.rs` has from `overview_states.rs`.
//!
//! METRES, EXACTLY AS STORED. One metre is 6.7e-12 AU, so a format that
//! converted units would displace every probe of every formation that
//! round-trips through it — the editor spec's §4.2 argument, applied to text
//! that leaves the machine. Legibility comes from COMMENTS instead: an AU
//! reading beside the range, a kilometre distance beside each probe. The
//! parser skips them.
//!
//! NO IDS. An id is the account-local key of the `customFormations` dict and is
//! reused rather than minted, so an import allocates a fresh one (sharing spec
//! §2.2).

use serde::{Deserialize, Serialize};
use yaml_rust2::{Yaml, YamlLoader};

use crate::probes::ProbeError;

/// EVE's own astronomical unit, for the emitted comments only — never for a
/// value that gets read back.
const M_PER_AU: f64 = 149_597_870_700.0;

/// A formation as it travels between files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormationSpec {
    pub name: String,
    /// Metre offsets from the formation centre. EVE's axes: X and Z are the
    /// horizontal plane, Y is up.
    pub probes: Vec<[f64; 3]>,
    /// Metres, one per probe, positionally matching `probes`.
    pub ranges: Vec<f64>,
}

/// A number that parses back to the same `f64`.
///
/// Integral values are written without a decimal point: every corpus
/// coordinate is one, and `-1199120384` reads where `-1199120384.0` only adds
/// noise across twenty-four of them. `i64` is exact well past 2^53, far beyond
/// any coordinate EVE stores. Everything else falls back to `{:?}`, Rust's
/// shortest round-tripping form and what `emit_pack` already uses for floats.
fn num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 9.0e15 {
        return format!("{}", v as i64);
    }
    format!("{v:?}")
}

/// A range in AU, for the comment beside it. Six places is plenty: the nine
/// slider stops are all clean, and this is decoration, not data.
fn au(metres: f64) -> String {
    let s = format!("{:.6}", metres / M_PER_AU);
    let t = s.trim_end_matches('0').trim_end_matches('.');
    if t.is_empty() || t == "-" { "0".to_string() } else { t.to_string() }
}

/// A YAML single-quoted scalar. Doubling the quote is the single-quoted style's
/// only escape, which is why this style is used — `emit_pack` writes names the
/// same way.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Render formations as the shared YAML text.
pub fn emit_formations(specs: &[FormationSpec]) -> String {
    let mut out = String::from(
        "# EVE probe formations. Positions and ranges are metres from the formation centre.\n",
    );
    out.push_str("formations:\n");
    for s in specs {
        out.push_str(&format!("  - name: {}\n", quote(&s.name)));
        let first = s.ranges.first().copied().unwrap_or(0.0);
        // Written even in the mixed case, so a reader that only understands
        // `range:` gets the first probe's value rather than nothing (§2.3).
        out.push_str(&format!("    range: {}          # {} AU\n", num(first), au(first)));
        if s.ranges.iter().any(|r| *r != first) {
            let list: Vec<String> = s.ranges.iter().map(|r| num(*r)).collect();
            out.push_str(&format!("    ranges: [{}]\n", list.join(", ")));
        }
        out.push_str("    probes:\n");
        // Column-align the coordinates. The point of metres is that they are
        // exact; a ragged block of eleven-digit numbers is unreadable, and
        // legibility is this format's other job.
        let cells: Vec<[String; 3]> =
            s.probes.iter().map(|p| [num(p[0]), num(p[1]), num(p[2])]).collect();
        let width = |i: usize| cells.iter().map(|c| c[i].len()).max().unwrap_or(0);
        let (w0, w1, w2) = (width(0), width(1), width(2));
        for (p, c) in s.probes.iter().zip(&cells) {
            let km = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() / 1000.0;
            out.push_str(&format!(
                "      - [{:>w0$}, {:>w1$}, {:>w2$}]   # {} km\n",
                c[0], c[1], c[2], km.round(),
            ));
        }
    }
    out
}

fn bad(what: impl Into<String>) -> ProbeError {
    ProbeError::BadYaml { message: what.into() }
}

/// A YAML scalar as a number. `Real` carries the source text; `String` covers a
/// quoted or exponent-formatted value a hand edit might produce, and refusing
/// those would fail a file the user could not see anything wrong with.
fn number(y: &Yaml) -> Option<f64> {
    match y {
        Yaml::Integer(i) => Some(*i as f64),
        Yaml::Real(s) | Yaml::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn read_spec(y: &Yaml) -> Result<FormationSpec, ProbeError> {
    let Yaml::Hash(h) = y else { return Err(bad("a formations entry is not a mapping")) };
    let get = |k: &str| h.get(&Yaml::String(k.to_string()));
    let name = match get("name") {
        Some(Yaml::String(s)) => s.clone(),
        // `name: 2026` is a YAML integer, and a formation named after a year is
        // ordinary. Refusing it would be a puzzle, not a diagnosis.
        Some(Yaml::Integer(i)) => i.to_string(),
        _ => return Err(bad("a formation has no name")),
    };
    let Some(Yaml::Array(rows)) = get("probes") else {
        return Err(bad(format!("formation '{name}' has no probes list")));
    };
    let mut probes = Vec::with_capacity(rows.len());
    for row in rows {
        let Yaml::Array(xyz) = row else {
            return Err(bad(format!("a probe of '{name}' is not an [x, y, z] list")));
        };
        if xyz.len() != 3 {
            return Err(bad(format!("a probe of '{name}' does not have three coordinates")));
        }
        let mut p = [0.0f64; 3];
        for (slot, cell) in p.iter_mut().zip(xyz) {
            *slot = number(cell)
                .ok_or_else(|| bad(format!("a coordinate of '{name}' is not a number")))?;
        }
        probes.push(p);
    }
    // `ranges` wins: it is the only shape that survives a formation whose
    // probes disagree, and flattening one is the outcome §2.3 forbids.
    let ranges = match get("ranges") {
        Some(Yaml::Array(rs)) => rs
            .iter()
            .map(|r| number(r).ok_or_else(|| bad(format!("a range of '{name}' is not a number"))))
            .collect::<Result<Vec<f64>, ProbeError>>()?,
        _ => {
            let r = get("range")
                .and_then(number)
                .ok_or_else(|| bad(format!("formation '{name}' has no range")))?;
            vec![r; probes.len()]
        }
    };
    Ok(FormationSpec { name, probes, ranges })
}

/// Read shared YAML text. Every entry must be readable: skipping a malformed
/// one silently would hand the user a partial import they did not ask for.
pub fn parse_formations(text: &str) -> Result<Vec<FormationSpec>, ProbeError> {
    let docs = YamlLoader::load_from_str(text)
        .map_err(|e| ProbeError::BadYaml { message: e.to_string() })?;
    let Some(Yaml::Hash(top)) = docs.into_iter().next() else {
        return Err(ProbeError::NotFormations);
    };
    let entries = top
        .into_iter()
        .find(|(k, _)| matches!(k, Yaml::String(s) if s == "formations"))
        .map(|(_, v)| v)
        .ok_or(ProbeError::NotFormations)?;
    let Yaml::Array(items) = entries else { return Err(ProbeError::NotFormations) };
    items.iter().map(read_spec).collect()
}

/// `want`, or `want copy`, `want copy 2`… — the first name `existing` does not
/// already hold. Matches what the editor's Duplicate button produces, so an
/// imported collision and a hand-made one read the same in EVE's menu.
pub fn unique_name(existing: &[String], want: &str) -> String {
    let taken = |c: &str| existing.iter().any(|e| e == c);
    if !taken(want) {
        return want.to_string();
    }
    let first = format!("{want} copy");
    if !taken(&first) {
        return first;
    }
    (2i64..)
        .map(|n| format!("{want} copy {n}"))
        .find(|c| !taken(c))
        .expect("i64 has a free suffix")
}
```

- [ ] **Step 6: Declare and re-export the module**

In `crates/settings-model/src/lib.rs`, add after the `mod overview_pack;` line:

```rust
mod probe_pack;
```

And after the `pub use overview_pack::{…}` line:

```rust
pub use probe_pack::{emit_formations, parse_formations, unique_name, FormationSpec};
```

In the same file, add `check_formation` to the existing probes re-export so it reads:

```rust
pub use probes::{
    check_formation, next_id as next_formation_id, project_formations, remove_formation,
    set_formation, Formation, Formations, ProbeError, DEFAULT_RANGE, MAX_PROBES,
};
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test -p settings-model`
Expected: PASS, all of it — the new `probe_pack` tests and the existing `probes` ones.

- [ ] **Step 8: Commit**

```bash
git add crates/settings-model/src/probe_pack.rs crates/settings-model/src/probes.rs crates/settings-model/src/lib.rs
git commit -F - <<'EOF'
Give probe formations a text format they can travel in

Metres exactly as stored, so a shared formation comes back where it left;
comments carry the AU and kilometre readings the numbers hide.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
```

---

## Task 2: The five commands

**Files:**
- Modify: `app/src-tauri/src/ops.rs`
- Modify: `app/src-tauri/src/lib.rs`
- Modify: `app/src/lib/api.ts`

**Interfaces:**
- Consumes: `settings_model::{emit_formations, parse_formations, unique_name, check_formation, next_formation_id, project_formations, set_formation, FormationSpec, Formations}` from Task 1; the existing `ops::edit_user_probes` and `ops::probe_err`.
- Produces:
  - Tauri commands `probe_yaml`, `probe_parse_yaml`, `probe_export`, `probe_import`, `add_probe_formations`
  - `api.ts`: `export type FormationSpec = { name: string; probes: [number, number, number][]; ranges: number[] }`, and `api.probeYaml`, `api.probeParseYaml`, `api.probeExport`, `api.probeImport`, `api.addProbeFormations`

- [ ] **Step 1: Write the failing tests**

In `app/src-tauri/src/ops.rs`, add to the existing `#[cfg(test)] mod tests`, just after `set_probe_formation_with_no_key_mints_it_at_id_zero`:

```rust
    fn spec(name: &str, x: f64) -> settings_model::FormationSpec {
        settings_model::FormationSpec {
            name: name.into(),
            probes: vec![[x, 0.0, 0.0]],
            ranges: vec![74_798_935_350.0],
        }
    }

    /// An account file holding one formation named "close" at id 0.
    fn state_with_close() -> (AppState, PathBuf) {
        let bytes = encode(&Value::Dict(vec![(b("ui"), Value::Dict(vec![]))])).unwrap();
        let path = temp_file("probes-add", &bytes);
        let state = AppState::new();
        open_file(&state, Slot::User, path.to_str().unwrap()).unwrap();
        set_probe_formation(&state, None, "close", vec![[1.0, 0.0, 0.0]], vec![74_798_935_350.0])
            .unwrap();
        (state, path)
    }

    #[test]
    fn add_probe_formations_allocates_a_distinct_id_for_each() {
        // next_id fills the lowest free gap, so allocating them all up front
        // from one projection would hand every member of the batch the same id.
        let (state, _p) = state_with_close();
        let f = add_probe_formations(&state, vec![spec("a", 2.0), spec("b", 3.0)]).unwrap();
        let ids: Vec<i64> = f.formations.iter().map(|x| x.id).collect();
        assert_eq!(ids, vec![0, 1, 2]);
        let names: Vec<&str> = f.formations.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, vec!["close", "a", "b"]);
    }

    #[test]
    fn add_probe_formations_suffixes_a_colliding_name() {
        let (state, _p) = state_with_close();
        let f = add_probe_formations(&state, vec![spec("close", 2.0), spec("close", 3.0)]).unwrap();
        let names: Vec<&str> = f.formations.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, vec!["close", "close copy", "close copy 2"]);
        assert_eq!(f.formations[0].probes, vec![[1.0, 0.0, 0.0]], "the original must not move");
    }

    #[test]
    fn an_invalid_member_writes_none_of_the_batch() {
        // Half an import is worse than none: the user would have to work out
        // which half (sharing spec §4.2).
        let (state, _p) = state_with_close();
        let empty = settings_model::FormationSpec {
            name: "bad".into(),
            probes: vec![],
            ranges: vec![],
        };
        let err = add_probe_formations(&state, vec![spec("good", 2.0), empty]).unwrap_err();
        assert_eq!(err.code, "bad_probe_count");
        let after = probe_formations(&state).unwrap();
        assert_eq!(after.formations.len(), 1, "nothing from the batch may survive");
        assert_eq!(after.formations[0].name, "close");
    }

    #[test]
    fn probe_export_then_import_round_trips_a_file() {
        let dir = std::env::temp_dir().join(format!("app-ops-{}-probe-yaml", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("formations.yaml");
        let specs = vec![spec("close", -1199120384.7)];
        probe_export(path.to_str().unwrap(), &specs).unwrap();
        assert_eq!(probe_import(path.to_str().unwrap()).unwrap(), specs);
    }

    #[test]
    fn probe_import_of_a_missing_file_is_an_io_error() {
        let missing = std::env::temp_dir().join("app-ops-no-such-formations.yaml");
        assert_eq!(probe_import(missing.to_str().unwrap()).unwrap_err().code, "io");
    }

    #[test]
    fn probe_parse_yaml_reports_the_wrong_file() {
        assert_eq!(probe_parse_yaml("presets:\n  - a\n").unwrap_err().code, "not_formations");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run (from `app/src-tauri`): `cargo test -p app add_probe_formations`
Expected: FAIL — `cannot find function add_probe_formations`.

- [ ] **Step 3: Write the ops functions**

In `app/src-tauri/src/ops.rs`, add after `remove_probe_formation`:

```rust
/// Emit the shared YAML for a set of formations.
///
/// The FRONTEND supplies the data rather than naming ids for a lookup here:
/// Copy and Export send what the user currently sees, uncommitted drafts
/// included (sharing spec §5.1), and only the view holds that.
pub fn probe_yaml(formations: &[settings_model::FormationSpec]) -> String {
    settings_model::emit_formations(formations)
}

pub fn probe_parse_yaml(text: &str) -> Result<Vec<settings_model::FormationSpec>, ErrDto> {
    settings_model::parse_formations(text).map_err(probe_err)
}

pub fn probe_export(
    path: &str,
    formations: &[settings_model::FormationSpec],
) -> Result<(), ErrDto> {
    std::fs::write(path, settings_model::emit_formations(formations))
        .map_err(|e| ErrDto::new("io", format!("{path}: {e}")))
}

pub fn probe_import(path: &str) -> Result<Vec<settings_model::FormationSpec>, ErrDto> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| ErrDto::new("io", format!("{path}: {e}")))?;
    probe_parse_yaml(&text)
}

/// Add formations at fresh ids, suffixing any name the account already holds.
///
/// One command rather than N `set_probe_formation` calls: each of those
/// reshares the whole document (sharing spec §4.1). It is also the single place
/// the collision rule lives, so Paste and Import cannot disagree about it.
pub fn add_probe_formations(
    state: &AppState,
    formations: Vec<settings_model::FormationSpec>,
) -> Result<settings_model::Formations, ErrDto> {
    edit_user_probes(state, |v| {
        // The WHOLE batch, before the first write. A bad entry halfway down
        // would otherwise leave half an import applied (spec §4.2).
        for f in &formations {
            settings_model::check_formation(&f.name, &f.probes, &f.ranges)?;
        }
        for f in formations {
            // Re-projected per formation because each write changes both the
            // free ids and the taken names.
            //
            // ponytail: O(n²) over a batch of at most a handful of formations.
            // Thread a running Formations through the loop if a source of
            // hundreds ever appears.
            let now = settings_model::project_formations(v).unwrap_or(settings_model::Formations {
                // No key yet — the first-ever formation on this account. Any
                // real problem with the document resurfaces from set_formation.
                formations: Vec::new(),
                selected: None,
            });
            let id = settings_model::next_formation_id(&now);
            let held: Vec<String> = now.formations.into_iter().map(|x| x.name).collect();
            let name = settings_model::unique_name(&held, &f.name);
            settings_model::set_formation(v, id, &name, &f.probes, &f.ranges)?;
        }
        Ok(())
    })
}
```

- [ ] **Step 4: Add the Tauri commands**

In `app/src-tauri/src/lib.rs`, add after the `remove_probe_formation` command:

```rust
#[tauri::command]
fn probe_yaml(formations: Vec<settings_model::FormationSpec>) -> String {
    ops::probe_yaml(&formations)
}

#[tauri::command]
fn probe_parse_yaml(text: String) -> Result<Vec<settings_model::FormationSpec>, ErrDto> {
    ops::probe_parse_yaml(&text)
}

#[tauri::command]
fn probe_export(
    path: String,
    formations: Vec<settings_model::FormationSpec>,
) -> Result<(), ErrDto> {
    ops::probe_export(&path, &formations)
}

#[tauri::command]
fn probe_import(path: String) -> Result<Vec<settings_model::FormationSpec>, ErrDto> {
    ops::probe_import(&path)
}

#[tauri::command]
fn add_probe_formations(
    state: tauri::State<'_, AppState>,
    formations: Vec<settings_model::FormationSpec>,
) -> Result<settings_model::Formations, ErrDto> {
    ops::add_probe_formations(&state, formations)
}
```

And extend the registration line in `generate_handler!` so it reads:

```rust
            probe_formations, set_probe_formation, remove_probe_formation,
            probe_yaml, probe_parse_yaml, probe_export, probe_import, add_probe_formations,
```

- [ ] **Step 5: Run the tests to verify they pass**

Run (from `app/src-tauri`): `cargo test -p app`
Expected: PASS.

- [ ] **Step 6: Add the frontend bindings**

In `app/src/lib/api.ts`, add just below the `Formations` type:

```ts
/** A formation as it travels between files: no id, because an id is
 * account-local and an import allocates a fresh one. */
export type FormationSpec = {
  name: string;
  probes: [number, number, number][];
  ranges: number[];
};
```

And in the `api` object, after `removeProbeFormation`:

```ts
  /** The shared YAML for these formations. Pure text — the caller supplies the
   * data, so Copy and Export can send an uncommitted draft. */
  probeYaml: (formations: FormationSpec[]) => invoke<string>("probe_yaml", { formations }),
  probeParseYaml: (text: string) => invoke<FormationSpec[]>("probe_parse_yaml", { text }),
  probeExport: (path: string, formations: FormationSpec[]) =>
    invoke<void>("probe_export", { path, formations }),
  probeImport: (path: string) => invoke<FormationSpec[]>("probe_import", { path }),
  /** Add at fresh ids, suffixing any name the account already holds. Never
   * replaces or deletes anything. */
  addProbeFormations: (formations: FormationSpec[]) =>
    invoke<Formations>("add_probe_formations", { formations }),
```

- [ ] **Step 7: Type-check**

Run (in `app/`): `npm run check`
Expected: no new errors.

- [ ] **Step 8: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -F - <<'EOF'
Wire the formation text format through to the frontend

Emit, parse, read and write it, and add a batch that lands whole imports at
fresh ids or none of them at all.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
```

---

## Task 3: The picker modal

**Files:**
- Create: `app/src/lib/FormationPicker.svelte`
- Create: `app/src/lib/FormationPicker.spec.ts`

**Interfaces:**
- Consumes: `FormationSpec` from Task 2; `formatUnit` from `app/src/lib/probes.ts`; the global `.overlay` and `.modal` classes in `app/src/app.css`.
- Produces: a component with props
  `{ title: string; items: FormationSpec[]; confirmLabel: string; onconfirm: (indices: number[]) => void; oncancel: () => void }`.
  `onconfirm` receives **indices into `items`**, ascending — not the formations themselves, so a caller can map them back onto whatever it holds alongside.

- [ ] **Step 1: Write the failing test**

Create `app/src/lib/FormationPicker.spec.ts`:

```ts
// Component test: run with `npm run test:ui` (vitest + jsdom).
import { describe, expect, test, vi } from "vitest";
import { render, fireEvent, screen } from "@testing-library/svelte";
import FormationPicker from "$lib/FormationPicker.svelte";
import type { FormationSpec } from "$lib/api";
// Imported for its afterEach cleanup: without it every render stays in the
// document and the next test's queries match two copies of everything.
import "$lib/test/setup";

const ITEMS: FormationSpec[] = [
  { name: "close", probes: [[1, 0, 0], [2, 0, 0]], ranges: [74798935350, 74798935350] },
  { name: "on grid", probes: [[3, 0, 0]], ranges: [598391482800] },
  { name: "odd", probes: [[4, 0, 0], [5, 0, 0]], ranges: [74798935350, 149597870700] },
];

function open(onconfirm = vi.fn(), oncancel = vi.fn()) {
  render(FormationPicker, {
    title: "Import formations",
    items: ITEMS,
    confirmLabel: "Import",
    onconfirm,
    oncancel,
  });
  return { onconfirm, oncancel };
}

describe("FormationPicker", () => {
  test("everything starts ticked and confirm carries the count", async () => {
    open();
    for (const f of ITEMS) {
      expect((screen.getByLabelText(f.name) as HTMLInputElement).checked).toBe(true);
    }
    expect(screen.getByText("Import 3")).toBeTruthy();
  });

  test("confirm hands back the indices that are still ticked", async () => {
    const { onconfirm } = open();
    await fireEvent.click(screen.getByLabelText("on grid"));
    await fireEvent.click(screen.getByText("Import 2"));
    expect(onconfirm).toHaveBeenCalledWith([0, 2]);
  });

  test("with nothing ticked, confirm is disabled", async () => {
    open();
    // Everything is on, so the button offers the inverse.
    await fireEvent.click(screen.getByText("Select none"));
    expect((screen.getByText("Import 0") as HTMLButtonElement).disabled).toBe(true);
  });

  test("select all re-ticks everything after a manual untick", async () => {
    open();
    await fireEvent.click(screen.getByLabelText("close"));
    await fireEvent.click(screen.getByText("Select all"));
    expect(screen.getByText("Import 3")).toBeTruthy();
  });

  test("a row shows its probe count and range, and says mixed when they differ", async () => {
    open();
    expect(screen.getByText("2 probes · 0.5 AU")).toBeTruthy();
    expect(screen.getByText("1 probe · 4 AU")).toBeTruthy();
    expect(screen.getByText("2 probes · mixed")).toBeTruthy();
  });

  test("clicking the backdrop cancels", async () => {
    const { oncancel } = open();
    await fireEvent.click(screen.getByTestId("picker-backdrop"));
    expect(oncancel).toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run (in `app/`): `npx vitest run src/lib/FormationPicker.spec.ts`
Expected: FAIL — cannot resolve `$lib/FormationPicker.svelte`.

- [ ] **Step 3: Write the component**

Create `app/src/lib/FormationPicker.svelte`:

```svelte
<script lang="ts">
  // A checkbox list over formations, used by BOTH Export (this account's set)
  // and Import (a file's). It knows nothing about ids, files or the clipboard:
  // it is handed items and hands back the indices that were ticked.
  import type { FormationSpec } from "./api";
  import { formatUnit } from "./probes";

  let { title, items, confirmLabel, onconfirm, oncancel }:
    { title: string; items: FormationSpec[]; confirmLabel: string;
      onconfirm: (indices: number[]) => void; oncancel: () => void } = $props();

  // Everything starts ticked: "all of them" is the common case both ways, and
  // unticking one is easier to discover than hunting for a select-all first.
  let picked = $state(items.map(() => true));
  const chosen = $derived(picked.flatMap((on, i) => (on ? [i] : [])));
  const allOn = $derived(picked.every(Boolean));

  /** A formation's range in AU, or "mixed" when its probes disagree — the one
   * case a single number would misreport (spec §2.3). */
  function rangeLabel(f: FormationSpec): string {
    const first = f.ranges[0] ?? 0;
    return f.ranges.every((r) => r === first) ? `${formatUnit(first, "au")} AU` : "mixed";
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="overlay" role="none" data-testid="picker-backdrop" onclick={oncancel}>
  <div class="modal" role="none" onclick={(e) => e.stopPropagation()}>
    <h2>{title}</h2>
    <ul>
      {#each items as f, i}
        <li>
          <label>
            <input type="checkbox" checked={picked[i]}
                   onchange={(e) => (picked[i] = e.currentTarget.checked)} />
            <span class="name">{f.name}</span>
            <span class="meta">
              {f.probes.length} {f.probes.length === 1 ? "probe" : "probes"} · {rangeLabel(f)}
            </span>
          </label>
        </li>
      {/each}
    </ul>
    <div class="form-actions">
      <button onclick={() => (picked = picked.map(() => !allOn))}>
        {allOn ? "Select none" : "Select all"}
      </button>
      <span class="spacer"></span>
      <button onclick={oncancel}>Cancel</button>
      <button disabled={chosen.length === 0} onclick={() => onconfirm(chosen)}>
        {confirmLabel} {chosen.length}
      </button>
    </div>
  </div>
</div>

<style>
  /* `.overlay`, `.modal`, `.form-actions` and `.spacer` are global (app.css) —
     this is the same modal the tree's insert form uses, not a second one. */
  h2 { margin: 0 0 0.6rem; font-size: 1em; font-weight: 600; }
  ul { list-style: none; margin: 0; padding: 0; max-height: 50vh; overflow-y: auto; }
  li label { display: flex; align-items: baseline; gap: 0.5rem; padding: 3px 2px; cursor: pointer; }
  .name { flex: 1; }
  .meta { opacity: 0.7; font-size: 0.85em; white-space: nowrap; }
</style>
```

The `<label>` wrapping the checkbox is what makes `getByLabelText(f.name)` find the input — the accessible name comes from the label's own text.

- [ ] **Step 4: Run the test to verify it passes**

Run (in `app/`): `npx vitest run src/lib/FormationPicker.spec.ts`
Expected: PASS.

- [ ] **Step 5: Type-check**

Run (in `app/`): `npm run check`
Expected: no new errors.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/FormationPicker.svelte app/src/lib/FormationPicker.spec.ts
git commit -F - <<'EOF'
Add the modal for choosing which formations to move

One list with checkboxes, handed items and handing back indices, so export
and import share it rather than each growing their own.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
```

---

## Task 4: Copy and paste

**Files:**
- Modify: `app/src/lib/ProbeFormationsView.svelte`
- Modify: `app/src/lib/ProbeFormationsView.spec.ts`

**Interfaces:**
- Consumes: `api.probeYaml`, `api.probeParseYaml`, `api.addProbeFormations`, `FormationSpec` from Task 2.
- Produces (used by Task 5):
  - `const visible: FormationSpec[]` — the loaded projection with the selected formation's draft substituted in
  - `async function addShared(specs: FormationSpec[])` — throws; the caller reports
  - `function inAField(t: EventTarget | null): boolean`

**Note on button placement:** the spec (§5.3) puts Copy and Paste "with the per-formation controls". Copy goes there — it copies the formation being edited. **Paste goes in the sidebar** with `New` and `Duplicate` instead, because like them it creates a formation rather than acting on the current one. This is the one placement adjustment to §5.3.

- [ ] **Step 1: Write the failing tests**

In `app/src/lib/ProbeFormationsView.spec.ts`, extend the imports at the top:

```ts
import { describe, expect, test, vi, beforeEach } from "vitest";
```

and add this whole block at the end of the file:

```ts
describe("clipboard sharing", () => {
  /** What writeText was last handed, and what readText will answer. */
  let written: string[] = [];
  let readable: string | Error = "";

  beforeEach(() => {
    written = [];
    readable = "";
    // jsdom implements no clipboard at all, so there is nothing to spy on —
    // define one. `configurable` so each test can redefine it.
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: (t: string) => { written.push(t); return Promise.resolve(); },
        readText: () => (readable instanceof Error
          ? Promise.reject(readable)
          : Promise.resolve(readable)),
      },
    });
  });

  const SHARED = "formations:\n  - name: close\n    range: 74798935350\n    probes:\n      - [1, 0, 0]\n";

  test("Copy sends the draft, not the saved projection", async () => {
    // The whole reason Copy passes data rather than an id: what the user sees
    // is the draft, and blur-commit is async (spec §5.1).
    await open();
    calls.stub("probe_yaml", SHARED);
    const x = await screen.findByLabelText("probe 1 X");
    await fireEvent.input(x, { target: { value: "999" } });

    await fireEvent.click(screen.getByText("Copy"));

    const sent = calls.of("probe_yaml").at(-1)?.args as { formations: FormationSpec[] };
    expect(sent.formations).toHaveLength(1);
    // 999 AU in metres, from the un-blurred field.
    expect(sent.formations[0].probes[0][0]).toBeCloseTo(999 * 149597870700, 0);
    expect(written).toEqual([SHARED]);
  });

  test("Ctrl-C copies the formation, but not from inside a field", async () => {
    await open();
    calls.stub("probe_yaml", SHARED);

    const x = await screen.findByLabelText("probe 1 X");
    await fireEvent.keyDown(x, { key: "c", ctrlKey: true });
    expect(calls.of("probe_yaml")).toHaveLength(0);

    await fireEvent.keyDown(window, { key: "c", ctrlKey: true });
    await vi.waitFor(() => expect(calls.of("probe_yaml")).toHaveLength(1));
  });

  test("Paste parses the clipboard and adds what it found", async () => {
    await open();
    readable = SHARED;
    calls.stub("probe_parse_yaml", [
      { name: "close", probes: [[1, 0, 0]], ranges: [74798935350] },
    ] satisfies FormationSpec[]);
    calls.stub("add_probe_formations", FORMATIONS);

    await fireEvent.click(screen.getByText("Paste"));

    await vi.waitFor(() => expect(calls.of("add_probe_formations")).toHaveLength(1));
    const sent = calls.of("add_probe_formations")[0].args as { formations: FormationSpec[] };
    expect(sent.formations[0].name).toBe("close");
    // Never set_probe_formation: the collision rule lives in the batch command.
    expect(calls.of("set_probe_formation")).toHaveLength(0);
  });

  test("a refused clipboard read does not fail silently", async () => {
    await open();
    readable = new Error("denied");
    await fireEvent.click(screen.getByText("Paste"));
    await vi.waitFor(() => expect(vi.mocked(message)).toHaveBeenCalled());
    expect(vi.mocked(message).mock.calls[0][0]).toMatch(/Ctrl\+V/);
    calls.never("probe_parse_yaml");
  });

  test("a paste event adds formations without touching the clipboard API", async () => {
    // The Ctrl-V fallback: the keypress IS the permission grant, so this path
    // must work even when readText is refused outright (spec §5.4).
    await open();
    readable = new Error("denied");
    calls.stub("probe_parse_yaml", [
      { name: "close", probes: [[1, 0, 0]], ranges: [74798935350] },
    ] satisfies FormationSpec[]);
    calls.stub("add_probe_formations", FORMATIONS);

    // fireEvent cannot attach clipboardData to a jsdom Event, so build it.
    const ev = new Event("paste", { bubbles: true });
    Object.defineProperty(ev, "clipboardData", { value: { getData: () => SHARED } });
    window.dispatchEvent(ev);

    await vi.waitFor(() => expect(calls.of("add_probe_formations")).toHaveLength(1));
  });
});
```

**Replace** the existing `import type { Formation, Formations } from "$lib/api";` line with the two the block needs — do not add a second import from the same module:

```ts
import { message } from "@tauri-apps/plugin-dialog";
import type { Formation, Formations, FormationSpec } from "$lib/api";
```

and, above the existing `const noop`, the dialog mock:

```ts
// The view raises dialogs on every failure path; jsdom has no Tauri to answer
// them, and a test asserting on WHICH message appeared needs the spy anyway.
vi.mock("@tauri-apps/plugin-dialog", () => ({
  message: vi.fn(() => Promise.resolve()),
  open: vi.fn(),
  save: vi.fn(),
}));
```

- [ ] **Step 2: Run the tests to verify they fail**

Run (in `app/`): `npx vitest run src/lib/ProbeFormationsView.spec.ts`
Expected: FAIL — `Unable to find an element with the text: Copy`.

- [ ] **Step 3: Add the sharing logic to the view**

In `app/src/lib/ProbeFormationsView.svelte`, extend the api import:

```ts
  import { api, errMessage, type Formation, type Formations, type FormationSpec } from "./api";
```

Add after the `current` derived:

```ts
  /** The formation set as the user currently sees it: the loaded projection
   * with the selected formation's uncommitted draft substituted in.
   *
   * Copy, Export and the export picker all read this, so what leaves the app is
   * what is on screen (spec §5.1). Reading the backend's projection instead
   * would race the blur-commit that the Copy button's own click fires, and
   * could return either side of it depending on timing. */
  const visible = $derived<FormationSpec[]>(
    (loaded?.formations ?? []).map((f) =>
      f.id === selectedId
        ? { name: draftName, probes: draftProbes, ranges: draftRanges }
        : { name: f.name, probes: f.probes, ranges: f.ranges },
    ),
  );
  const visibleIndex = $derived(loaded?.formations.findIndex((f) => f.id === selectedId) ?? -1);
```

Add after `remove()`:

```ts
  /** Add formations from shared text — the one path Paste and Import both end
   * on, so the collision rule (in Rust, spec §4.3) applies to both. Throws;
   * each caller reports under its own title. */
  async function addShared(specs: FormationSpec[]) {
    if (specs.length === 0) return;
    const before = new Set(loaded?.formations.map((f) => f.id) ?? []);
    loaded = await api.addProbeFormations(specs);
    onUserDirty();
    // next_id fills the lowest free gap, so an added formation can land in the
    // MIDDLE of the sorted response — diff the ids rather than reading the end.
    const added = loaded.formations.filter((f) => !before.has(f.id));
    if (added.length) select(added[added.length - 1]);
  }

  async function copyFormation() {
    if (visibleIndex < 0) return;
    try {
      await navigator.clipboard.writeText(await api.probeYaml([visible[visibleIndex]]));
    } catch (e) {
      await message(errMessage(e), { title: "Could not copy the formation", kind: "error" });
    }
  }

  async function pasteText(text: string) {
    if (!text.trim()) return;
    try {
      await addShared(await api.probeParseYaml(text));
    } catch (e) {
      await message(errMessage(e), { title: "Could not paste the formation", kind: "error" });
    }
  }

  async function pasteFormation() {
    let text: string;
    try {
      text = await navigator.clipboard.readText();
    } catch {
      // WebView2 can refuse a clipboard READ without showing a prompt. Ctrl-V
      // needs no permission — the keypress is the grant — so point the user at
      // it rather than reporting a failure they cannot act on (spec §5.4).
      await message("Press Ctrl+V to paste a formation instead.", {
        title: "The clipboard could not be read",
      });
      return;
    }
    await pasteText(text);
  }

  /** True when the event came from somewhere the OS clipboard must keep
   * behaving normally. A tab full of coordinate fields is exactly where Ctrl-C
   * has to go on copying the digits the user just selected. */
  function inAField(t: EventTarget | null): boolean {
    const el = t as HTMLElement | null;
    const tag = el?.tagName;
    return tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA" || !!el?.isContentEditable;
  }

  function onKeyDown(e: KeyboardEvent) {
    // Ctrl-V needs no branch here: the browser fires `paste`, which carries the
    // data and asks no permission.
    if (!(e.ctrlKey || e.metaKey) || e.key !== "c" || inAField(e.target)) return;
    e.preventDefault();
    void copyFormation();
  }

  function onPaste(e: ClipboardEvent) {
    if (inAField(e.target)) return;
    const text = e.clipboardData?.getData("text/plain") ?? "";
    if (!text.trim()) return;
    e.preventDefault();
    void pasteText(text);
  }
```

- [ ] **Step 4: Add the buttons and the window listeners**

In the same file's markup, add as the **first line** of the template, above `{#if !userOpen}`:

```svelte
<!-- The Probes tab is conditionally mounted (+page.svelte), so this listener
     does not exist while another view is open and cannot leak into it. -->
<svelte:window onkeydown={onKeyDown} onpaste={onPaste} />
```

Add `Paste` to the sidebar actions, so the block reads:

```svelte
      <div class="list-actions">
        <button onclick={createNew}>New</button>
        <button onclick={duplicate} disabled={!current}>Duplicate</button>
        <button onclick={pasteFormation} title="Add a formation from the clipboard (Ctrl+V)">Paste</button>
        <button class="danger" onclick={remove} disabled={!current}>Delete</button>
      </div>
```

Add `Copy` beside the unit toggle, so the `.units` span becomes:

```svelte
          <span class="units">
            <span class="meta">probe positions in</span>
            <button class:active={unit === "au"} onclick={() => (unit = "au")}>AU</button>
            <button class:active={unit === "km"} onclick={() => (unit = "km")}>km</button>
          </span>
          <button onclick={copyFormation} title="Copy this formation to the clipboard (Ctrl+C)">Copy</button>
```

- [ ] **Step 5: Run the tests to verify they pass**

Run (in `app/`): `npx vitest run src/lib/ProbeFormationsView.spec.ts`
Expected: PASS, including the tests that were already there.

- [ ] **Step 6: Type-check and run the whole suite**

Run (in `app/`): `npm run check && npm test`
Expected: no new type errors; all tests pass.

- [ ] **Step 7: Commit**

```bash
git add app/src/lib/ProbeFormationsView.svelte app/src/lib/ProbeFormationsView.spec.ts
git commit -F - <<'EOF'
Copy a formation to the clipboard, and paste one back

Ctrl+C and Ctrl+V work anywhere on the tab except inside a field, where they
still copy the digits. Copy sends what is on screen, unsaved edits included.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
```

---

## Task 5: Export and import

**Files:**
- Modify: `app/src/lib/ProbeFormationsView.svelte`
- Modify: `app/src/lib/ProbeFormationsView.spec.ts`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: `FormationPicker` (Task 3); `visible`, `addShared` (Task 4); `api.probeExport`, `api.probeImport` (Task 2).
- Produces: nothing later tasks depend on.

- [ ] **Step 1: Write the failing tests**

In `app/src/lib/ProbeFormationsView.spec.ts`, extend the dialog import to bring in the pickers:

```ts
import { message, open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
```

and add this block at the end of the file:

```ts
describe("file sharing", () => {
  test("Export writes the formations that were ticked, drafts included", async () => {
    await open();
    vi.mocked(saveDialog).mockResolvedValueOnce("C:/tmp/formations.yaml");
    const name = await screen.findByDisplayValue("close");
    await fireEvent.input(name, { target: { value: "closer" } });

    await fireEvent.click(screen.getByText("Export…"));
    await screen.findByText("Export 1");
    await fireEvent.click(screen.getByText("Export 1"));

    await vi.waitFor(() => expect(calls.of("probe_export")).toHaveLength(1));
    const sent = calls.of("probe_export")[0].args as { path: string; formations: FormationSpec[] };
    expect(sent.path).toBe("C:/tmp/formations.yaml");
    expect(sent.formations.map((f) => f.name)).toEqual(["closer"]);
  });

  test("cancelling the save dialog opens no picker and writes nothing", async () => {
    await open();
    vi.mocked(saveDialog).mockResolvedValueOnce(null);
    await fireEvent.click(screen.getByText("Export…"));
    await vi.waitFor(() => expect(vi.mocked(saveDialog)).toHaveBeenCalled());
    expect(screen.queryByTestId("picker-backdrop")).toBeNull();
    calls.never("probe_export");
  });

  test("Import adds only the formations that were ticked", async () => {
    await open();
    vi.mocked(openDialog).mockResolvedValueOnce("C:/tmp/fleet.yaml");
    calls.stub("probe_import", [
      { name: "a", probes: [[1, 0, 0]], ranges: [74798935350] },
      { name: "b", probes: [[2, 0, 0]], ranges: [74798935350] },
    ] satisfies FormationSpec[]);
    calls.stub("add_probe_formations", FORMATIONS);

    await fireEvent.click(screen.getByText("Import…"));
    await screen.findByText("Import 2");
    await fireEvent.click(screen.getByLabelText("b"));
    await fireEvent.click(screen.getByText("Import 1"));

    await vi.waitFor(() => expect(calls.of("add_probe_formations")).toHaveLength(1));
    const sent = calls.of("add_probe_formations")[0].args as { formations: FormationSpec[] };
    expect(sent.formations.map((f) => f.name)).toEqual(["a"]);
  });

  test("an unreadable file is reported and opens no picker", async () => {
    await open();
    vi.mocked(openDialog).mockResolvedValueOnce("C:/tmp/overview.yaml");
    calls.stub("probe_import", () => {
      throw { code: "not_formations", message: "This file contains no probe formations." };
    });

    await fireEvent.click(screen.getByText("Import…"));

    await vi.waitFor(() => expect(vi.mocked(message)).toHaveBeenCalled());
    expect(screen.queryByTestId("picker-backdrop")).toBeNull();
    calls.never("add_probe_formations");
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run (in `app/`): `npx vitest run src/lib/ProbeFormationsView.spec.ts`
Expected: FAIL — `Unable to find an element with the text: Export…`.

- [ ] **Step 3: Add the export and import flows**

In `app/src/lib/ProbeFormationsView.svelte`, extend the dialog import and add the component import:

```ts
  import { message, open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import FormationPicker from "./FormationPicker.svelte";
```

Add after `pasteFormation`:

```ts
  /** The open picker, or null. `items` is what to choose from; `confirm` runs
   * with the chosen formations. One slot for both flows, because only one
   * picker is ever open. */
  let picker = $state<{
    title: string;
    items: FormationSpec[];
    label: string;
    confirm: (chosen: FormationSpec[]) => Promise<void>;
  } | null>(null);

  async function exportFormations() {
    if (!visible.length) return;
    const path = await saveDialog({
      defaultPath: "probe-formations.yaml",
      filters: [{ name: "Probe formations", extensions: ["yaml"] }],
    });
    if (!path) return;
    picker = {
      title: "Export formations",
      items: visible,
      label: "Export",
      confirm: async (chosen) => {
        await api.probeExport(path, chosen);
        await message(`Exported ${chosen.length} formation(s).`, { title: "Export formations" });
      },
    };
  }

  async function importFormations() {
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: "Probe formations", extensions: ["yaml", "yml"] }],
    });
    if (typeof picked !== "string") return;
    let items: FormationSpec[];
    try {
      items = await api.probeImport(picked);
    } catch (e) {
      await message(errMessage(e), { title: "Import failed", kind: "error" });
      return;
    }
    if (!items.length) {
      await message("That file contains no formations.", { title: "Import formations" });
      return;
    }
    picker = {
      title: `Import formations from ${picked.split(/[\\/]/).pop()}`,
      items,
      label: "Import",
      confirm: async (chosen) => {
        await addShared(chosen);
        await message(
          `Imported ${chosen.length} formation(s). Save to write them to the account file.`,
          { title: "Import formations" },
        );
      },
    };
  }

  /** Close the picker, THEN run its action — a dialog raised by the action
   * would otherwise stack on top of a modal the user can no longer reach. */
  async function runPicker(indices: number[]) {
    const p = picker;
    picker = null;
    if (!p) return;
    try {
      await p.confirm(indices.map((i) => p.items[i]));
    } catch (e) {
      await message(errMessage(e), { title: p.title, kind: "error" });
    }
  }
```

- [ ] **Step 4: Add the buttons and the picker to the markup**

Extend the sidebar actions to their final form:

```svelte
      <div class="list-actions">
        <button onclick={createNew}>New</button>
        <button onclick={duplicate} disabled={!current}>Duplicate</button>
        <button onclick={pasteFormation} title="Add a formation from the clipboard (Ctrl+V)">Paste</button>
        <button class="danger" onclick={remove} disabled={!current}>Delete</button>
        <button onclick={exportFormations} disabled={!visible.length}
                title="Write formations out as a shareable file">Export…</button>
        <button onclick={importFormations} title="Add formations from a shared file">Import…</button>
      </div>
```

And add the picker at the very end of the template, after the final `{/if}`:

```svelte
{#if picker}
  <FormationPicker title={picker.title} items={picker.items} confirmLabel={picker.label}
                   onconfirm={runPicker} oncancel={() => (picker = null)} />
{/if}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run (in `app/`): `npx vitest run src/lib/ProbeFormationsView.spec.ts src/lib/FormationPicker.spec.ts`
Expected: PASS.

- [ ] **Step 6: Run everything**

Run (in `app/`): `npm run check && npm test`
Then from the repo root: `cargo test`
Expected: all pass.

- [ ] **Step 7: Write the release notes**

In `CHANGELOG.md`, under `## [Unreleased]`, add:

```markdown
### Added
- Copy a probe formation to the clipboard and paste it back, with Ctrl+C and Ctrl+V.
- Export any number of probe formations to a file, and import any number from one.
```

Keep it to one line per feature, no engineering detail — the house style.

- [ ] **Step 8: Commit**

```bash
git add app/src/lib/ProbeFormationsView.svelte app/src/lib/ProbeFormationsView.spec.ts CHANGELOG.md
git commit -F - <<'EOF'
Export probe formations to a file, and import them back

Pick as many as you like on the way out and on the way in; imports are added
alongside what the account already has, never over it.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
```

---

## Manual verification

The suite cannot reach the two things that only exist in a real WebView2:

1. **Does `navigator.clipboard.readText()` actually work in the shipped shell?** Open the Probes tab, Copy a formation, then Paste. If a "The clipboard could not be read" dialog appears, the fallback is the common path rather than the rare one — record that in the spec's §5.4 note, because it changes whether the Tauri clipboard plugin is worth its dependencies.
2. **Does Ctrl+C still copy text from a coordinate field?** Select the digits in a probe's X field and press Ctrl+C, then paste into a text editor. The digits must arrive, not a YAML document.

Both are one-minute checks and neither blocks the merge on its own.
