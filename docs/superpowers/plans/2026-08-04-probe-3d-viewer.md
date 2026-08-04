# Probe formation 3D viewer and per-probe range — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the probe editor's two fixed orthographic panes with one perspective viewer that has a free camera and draggable probes, and make scan range editable per probe.

**Architecture:** The camera, projection and drag maths are pure functions in `app/src/lib/probes.ts`, which is deliberately rune-free so `node --test` can run it. `app/src/lib/ProbeViewer.svelte` renders the scene as SVG elements, which makes hit-testing the browser's job — every gizmo handle is an element with its own `pointerdown`, so there is no raycaster and no scene graph. No 3D library. Per-probe range is a signature change carried from `probes.rs` through `ops.rs`, `lib.rs` and `api.ts` into the table, and it deletes the `mixed_range` read-only lockout on the way.

**Tech Stack:** Rust (`blue_marshal::Value` tree editing), Tauri 2 commands, Svelte 5 runes, SVG, `node --test` for pure TS, vitest + jsdom for components.

**Spec:** `docs/superpowers/specs/2026-08-04-probe-3d-viewer-design.md`. Section references below (§4.3, §5.1, …) are to that document. It builds on `2026-08-03-probe-formation-editor-design.md`, referred to as *the editor spec*.

## Global Constraints

- **Metres are the source of truth everywhere.** No AU conversion or rounding in Rust; in the frontend only a field the user actually typed into converts back to metres. One metre is 6.7e-12 AU, so any display value round-tripping into the model displaces every probe on every save.
- **Validate before inlining.** In `probes.rs`, every rejection must happen before `inline_all(v)`, so a rejected write leaves the document byte-for-byte as it was. Tests assert this.
- **Id `-4` is the client's scratch slot.** Never projected, never written, never reachable through any public function.
- **Names are written as `Str`, never `Bytes`.**
- **Existing `(FILETIME, value)` wrappers are preserved**; an absent key is minted with a zero `Long` stamp.
- **`probes.ts` stays rune-free** — no `$state`, no `.svelte.ts`. It is imported by `probes.test.ts` under `node --test`, which strips types but does not compile Svelte.
- **Scan ranges are one of nine fixed stops**: `RANGE_STEPS_AU = [0.25, 0.5, 1, 2, 4, 8, 16, 32, 64]`. The in-game control is a slider with these stops, so the editor offers exactly these — plus, as an extra option, any value the file already holds that is not a stop.
- **A formation holds 1 to 8 probes** (`MAX_PROBES = 8`).
- Commands: `cargo test -p settings-model`, `cargo test -p app` (from `app/src-tauri`), `npm test` (in `app/` — runs `node --test` then vitest), `npm run check` (svelte-check).
- Branch: `probe-3d-viewer`, already created. Commit after every task.

---

## File Structure

| file | responsibility | change |
|---|---|---|
| `crates/settings-model/src/probes.rs` | the formation model and its read/write | modify: per-probe ranges, drop `range`/`mixed_range`, add `BadRangeCount` |
| `crates/settings-model/tests/probes_corpus.rs` | real-data guard | modify: drop the uniform-range assertions |
| `app/src-tauri/src/ops.rs` | slot-aware command bodies | modify: `ranges: Vec<f64>` |
| `app/src-tauri/src/lib.rs` | Tauri command signatures | modify: `ranges: Vec<f64>` |
| `app/src/lib/api.ts` | IPC types and wrappers | modify: `Formation` drops two fields; `setProbeFormation` takes `ranges` |
| `app/src/lib/probes.ts` | pure geometry, units, **and now camera/projection/drag** | modify: add the camera block, delete `project`/`paneScale`/`Plane` |
| `app/src/lib/probes.test.ts` | `node --test` for the above | modify |
| `app/src/lib/ProbeViewer.svelte` | **new** — the 3D scene, camera controls, gizmo | create |
| `app/src/lib/ProbeFormationsView.svelte` | list, table, IPC wiring | modify: range column, drop the lockout, swap panes for the viewer |
| `app/src/lib/ProbeFormationsView.spec.ts` | vitest component test | modify |
| `CHANGELOG.md` | release notes | modify |

---

## Task 1: Per-probe range in the model

**Files:**
- Modify: `crates/settings-model/src/probes.rs`
- Modify: `crates/settings-model/tests/probes_corpus.rs`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces:
  - `pub struct Formation { pub id: i64, pub name: String, pub probes: Vec<[f64; 3]>, pub ranges: Vec<f64> }` — `range: f64` and `mixed_range: bool` are **gone**.
  - `pub fn set_formation(v: &mut Value, id: i64, name: &str, probes: &[[f64; 3]], ranges: &[f64]) -> Result<(), ProbeError>`
  - `ProbeError::BadRangeCount`

- [ ] **Step 1: Write the failing tests**

In `crates/settings-model/src/probes.rs`, replace the whole `a_mixed_range_formation_is_flagged_not_flattened` test with this, and add the two new tests after `the_range_is_written_to_every_probe`:

```rust
    #[test]
    fn per_probe_ranges_round_trip() {
        // The client sets scan range per probe. A corpus that only ever shows a
        // uniform range says how players use the control, not what it permits
        // (spec §2.1) — so a mixed formation is ordinary data, not a file to
        // lock read-only.
        let d = Value::Dict(vec![(b("ui"), Value::Dict(vec![
            (b("probescanning.customFormations"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (Value::Int(0), formation(Value::Str("odd".into()), vec![
                    probe(1.0, 0.0, 0.0, DEFAULT_RANGE),
                    probe(2.0, 0.0, 0.0, DEFAULT_RANGE / 2.0),
                ])),
            ])])),
        ]))]);
        let p = project_formations(&d).unwrap();
        assert_eq!(p.formations[0].ranges, vec![DEFAULT_RANGE, DEFAULT_RANGE / 2.0]);
    }

    #[test]
    fn each_probe_keeps_its_own_written_range() {
        let mut v = doc();
        let probes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        set_formation(&mut v, 0, "spread", &probes, &[100.0, 200.0, 300.0]).unwrap();
        let p = project_formations(&v).unwrap();
        assert_eq!(p.formations[0].ranges, vec![100.0, 200.0, 300.0]);
    }

    #[test]
    fn a_range_count_that_does_not_match_the_probes_is_rejected() {
        let before = doc();
        let mut v = doc();
        let probes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        assert_eq!(
            set_formation(&mut v, 0, "x", &probes, &[100.0]),
            Err(ProbeError::BadRangeCount),
        );
        assert_eq!(
            set_formation(&mut v, 0, "x", &probes, &[100.0, 200.0, 300.0]),
            Err(ProbeError::BadRangeCount),
        );
        assert_eq!(v, before, "a rejected write must not inline or otherwise touch the document");
    }
```

Then replace the body of `the_range_is_written_to_every_probe` — rename it and drop the `range`/`mixed_range` reads:

```rust
    #[test]
    fn a_uniform_range_is_written_to_every_probe() {
        let mut v = doc();
        let probes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        set_formation(&mut v, 0, "even", &probes, &[123.0; 3]).unwrap();
        let p = project_formations(&v).unwrap();
        assert_eq!(p.formations[0].ranges, vec![123.0; 3]);
        assert_eq!(p.formations[0].probes.len(), 3);
    }
```

And in `coordinates_survive_the_projection_exactly`, replace the last two lines:

```rust
        assert_eq!(p.formations[0].ranges[0], DEFAULT_RANGE);
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p settings-model probes`
Expected: compile errors — `no variant named BadRangeCount`, `set_formation` takes `f64` not `&[f64]`, `no field ranges` where `range` was expected.

- [ ] **Step 3: Change the struct and the error enum**

In `crates/settings-model/src/probes.rs`, replace the `Formation` struct:

```rust
#[derive(Debug, PartialEq, Serialize)]
pub struct Formation {
    pub id: i64,
    pub name: String,
    /// Metre offsets from the formation centre. EVE's axes: X and Z are the
    /// horizontal plane, Y is up.
    pub probes: Vec<[f64; 3]>,
    /// Metres, one per probe, positionally matching `probes`. The format
    /// carries one range per entry because the client sets scan range per
    /// probe; every corpus entry agreeing on 0.5 AU is a fact about players,
    /// not about the format (spec §2.1).
    pub ranges: Vec<f64>,
}
```

Add the error variant to `ProbeError`, after `BadProbeCount`:

```rust
    /// A write whose range count does not match its probe count.
    BadRangeCount,
```

And its `Display` arm, after the `BadProbeCount` arm:

```rust
            ProbeError::BadRangeCount => write!(f, "Every probe needs a scan range."),
```

- [ ] **Step 4: Change the reader**

In `read_formation`, replace the tail from `let range = read[0].1;` to the end of the function:

```rust
    Some(Formation {
        id,
        name,
        probes: read.iter().map(|(p, _)| *p).collect(),
        ranges: read.iter().map(|(_, r)| *r).collect(),
    })
```

- [ ] **Step 5: Change the writer**

Replace `set_formation`'s doc comment, signature and validation, and the range it emits:

```rust
/// Replace the formation at `id`, or create it there.
///
/// `ranges` is one scan range per probe, positionally matching `probes` — the
/// format carries one per entry and the client sets one per probe (spec §2.1).
pub fn set_formation(
    v: &mut Value,
    id: i64,
    name: &str,
    probes: &[[f64; 3]],
    ranges: &[f64],
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
    if ranges.len() != probes.len() {
        return Err(ProbeError::BadRangeCount);
    }
    inline_all(v);
    let d = formations_mut(v)?;
    let entry = Value::Tuple(vec![
        Value::Str(name.to_string()),
        Value::List(
            probes
                .iter()
                .zip(ranges)
                .map(|(p, r)| {
                    Value::Tuple(vec![
                        Value::Tuple(vec![Value::Float(p[0]), Value::Float(p[1]), Value::Float(p[2])]),
                        Value::Float(*r),
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
```

- [ ] **Step 6: Fix the remaining call sites in the test module**

Every other `set_formation(...)` call in `mod tests` passes `DEFAULT_RANGE` as the last argument. Each becomes a slice of the right length:

- `set_replaces_an_existing_formation_and_keeps_the_stamp`: `&[[1.0, 2.0, 3.0]], &[DEFAULT_RANGE]`
- `set_creates_a_formation_at_a_new_id`: `&[[1.0, 0.0, 0.0]], &[DEFAULT_RANGE]`
- `a_written_name_is_str_never_bytes`: `&[[1.0, 0.0, 0.0]], &[DEFAULT_RANGE]`
- `the_scratch_slot_survives_a_write`: `&[[1.0, 0.0, 0.0]], &[DEFAULT_RANGE]`
- `a_key_absent_from_the_file_is_minted_wrapped`: `&[[1.0, 0.0, 0.0]], &[DEFAULT_RANGE]`
- `one_and_eight_probes_are_both_accepted`: `&[[1.0, 0.0, 0.0]], &[DEFAULT_RANGE]` then `&eight, &[DEFAULT_RANGE; 8]`

In `a_rejected_write_leaves_the_document_untouched`, the range slices must match the probe counts so each case fails for the reason it names, not for `BadRangeCount`:

```rust
    #[test]
    fn a_rejected_write_leaves_the_document_untouched() {
        let before = doc();
        let mut v = doc();
        assert_eq!(set_formation(&mut v, 0, "x", &[], &[]), Err(ProbeError::BadProbeCount));
        assert_eq!(set_formation(&mut v, 0, "  ", &[[1.0, 0.0, 0.0]], &[DEFAULT_RANGE]), Err(ProbeError::BadName));
        let nine = [[1.0, 0.0, 0.0]; 9];
        assert_eq!(set_formation(&mut v, 0, "x", &nine, &[DEFAULT_RANGE; 9]), Err(ProbeError::BadProbeCount));
        assert_eq!(set_formation(&mut v, -4, "x", &[[1.0, 0.0, 0.0]], &[DEFAULT_RANGE]), Err(ProbeError::NoSuchFormation));
        assert_eq!(v, before, "a rejected write must not inline or otherwise touch the document");
    }
```

In `next_id_fills_the_lowest_gap`, the two hand-built `Formation` literals drop their dead fields:

```rust
                Formation { id: 0, name: "a".into(), probes: vec![[0.0; 3]], ranges: vec![1.0] },
                Formation { id: 2, name: "b".into(), probes: vec![[0.0; 3]], ranges: vec![1.0] },
```

- [ ] **Step 7: Loosen the corpus test**

In `crates/settings-model/tests/probes_corpus.rs`, change the import line:

```rust
use settings_model::{project_formations, ProbeError, MAX_PROBES};
```

Replace the module doc's second paragraph:

```rust
//! It also locks in the measurement the editor's design rests on (spec §2.4):
//! every formation holds 8 probes. It does NOT lock in a uniform scan range —
//! the client sets range per probe, so a formation with differing ranges is
//! legitimate data the editor must keep working on (3d-viewer spec §2.1).
```

Replace the whole second test with:

```rust
#[test]
fn every_real_formation_holds_eight_probes() {
    // The measurement the editor's 1-8 probe range rests on (editor spec §2.4).
    // Real corpus only: the synthetic fixture is authored to these values, so
    // asserting on it proves nothing.
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
            assert_eq!(
                form.ranges.len(), form.probes.len(),
                "{}: formation {} projected {} ranges for {} probes — they must match positionally",
                f.path.display(), form.id, form.ranges.len(), form.probes.len(),
            );
        }
    }
    assert!(checked > 0, "the real corpus is present but carried no formations");
}
```

- [ ] **Step 8: Run the tests**

Run: `cargo test -p settings-model`
Expected: PASS. `probes.rs` and `probes_corpus.rs` both green.

- [ ] **Step 9: Commit**

```bash
git add crates/settings-model/src/probes.rs crates/settings-model/tests/probes_corpus.rs
git commit -m "Give every probe its own scan range in the model"
```

---

## Task 2: Per-probe range through the IPC layer

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (`set_probe_formation`, and its test at ~line 2297)
- Modify: `app/src-tauri/src/lib.rs` (`set_probe_formation` command, ~line 354)
- Modify: `app/src/lib/api.ts` (`Formation` type ~line 290, `setProbeFormation` ~line 486)

**Interfaces:**
- Consumes: `settings_model::set_formation(v, id, name, &probes, &ranges)` and the `Formation` struct from Task 1.
- Produces:
  - Rust: `ops::set_probe_formation(state, id: Option<i64>, name: &str, probes: Vec<[f64; 3]>, ranges: Vec<f64>)`
  - TS: `type Formation = { id: number; probes: [number, number, number][]; name: string; ranges: number[] }`
  - TS: `api.setProbeFormation(id, name, probes, ranges)`

- [ ] **Step 1: Write the failing test**

In `app/src-tauri/src/ops.rs`, replace `set_probe_formation_with_no_key_mints_it_at_id_zero`'s call line and add an assertion:

```rust
        let f = set_probe_formation(&state, None, "first", vec![[1.0, 0.0, 0.0]], vec![1000.0]).unwrap();
        assert_eq!(f.formations.len(), 1);
        assert_eq!(f.formations[0].id, 0, "0 is the first free id when none exist yet");
        assert_eq!(f.formations[0].name, "first");
        assert_eq!(f.formations[0].ranges, vec![1000.0]);
```

- [ ] **Step 2: Run it to verify it fails**

Run (from `app/src-tauri`): `cargo test -p app set_probe_formation_with_no_key`
Expected: FAIL — `expected f64, found Vec<f64>`.

- [ ] **Step 3: Change `ops.rs`**

Replace the signature and the closure:

```rust
pub fn set_probe_formation(
    state: &AppState,
    id: Option<i64>,
    name: &str,
    probes: Vec<[f64; 3]>,
    ranges: Vec<f64>,
) -> Result<settings_model::Formations, ErrDto> {
    let id = match id {
        Some(i) => i,
        None => match probe_formations(state) {
            Ok(f) => settings_model::next_formation_id(&f),
            // No key yet: `set_formation` mints it below, and 0 is the first
            // free id — this is the only create path, so a bare `?` here would
            // fail every first-ever formation on an account with none saved.
            Err(e) if e.code == "no_formations" => 0,
            Err(e) => return Err(e),
        },
    };
    edit_user_probes(state, |v| settings_model::set_formation(v, id, name, &probes, &ranges))
}
```

- [ ] **Step 4: Change `lib.rs`**

```rust
#[tauri::command]
fn set_probe_formation(
    state: tauri::State<'_, AppState>,
    id: Option<i64>,
    name: String,
    probes: Vec<[f64; 3]>,
    ranges: Vec<f64>,
) -> Result<settings_model::Formations, ErrDto> {
    ops::set_probe_formation(&state, id, &name, probes, ranges)
}
```

- [ ] **Step 5: Change `api.ts`**

Replace the `Formation` type:

```ts
export type Formation = {
  id: number;
  /** Metre offsets from the formation centre. X and Z are horizontal, Y is up. */
  probes: [number, number, number][];
  name: string;
  /** Metres, one per probe, positionally matching `probes`. The client sets
   * scan range per probe, so these are edited per row. */
  ranges: number[];
};
```

Replace the `setProbeFormation` wrapper:

```ts
  /** `id: null` creates at the next free id. `ranges` is one per probe. */
  setProbeFormation: (
    id: number | null,
    name: string,
    probes: [number, number, number][],
    ranges: number[],
  ) => invoke<Formations>("set_probe_formation", { id, name, probes, ranges }),
```

- [ ] **Step 6: Run the Rust tests**

Run (from `app/src-tauri`): `cargo test -p app`
Expected: PASS.

- [ ] **Step 7: Commit**

The frontend does not compile yet — `ProbeFormationsView.svelte` still reads `current.range`. That is Task 3, and this commit is the seam between them.

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Carry per-probe scan ranges across the IPC boundary"
```

---

## Task 3: Per-probe range in the editor, and the end of the read-only lockout

**Files:**
- Modify: `app/src/lib/ProbeFormationsView.svelte`
- Modify: `app/src/lib/ProbeFormationsView.spec.ts`

**Interfaces:**
- Consumes: `api.setProbeFormation(id, name, probes, ranges)` and the two-field-lighter `Formation` from Task 2.
- Produces: nothing later tasks import — this is a leaf UI change. Task 6 replaces this file's `.panes` block.

**Context for the implementer:** `mixed_range` existed to mark a formation the editor could not safely rewrite, because it only offered one range field and saving would have flattened the mix. With a range per row that risk is gone, so the flag and everything it gated are deleted (spec §5.1). Reading through the file, that is: eight `disabled={current.mixed_range}` bindings, the `.warn` paragraph, the `mixedProbeLabel` derivation, and the "Copy with uniform range" link — which existed only as an escape hatch out of the lockout.

- [ ] **Step 1: Write the failing tests**

In `app/src/lib/ProbeFormationsView.spec.ts`, replace the `FORMATIONS` fixture, the `lastSet` type and the whole `describe("mixed ranges", ...)` block (or whichever block holds the two mixed-range tests at ~lines 142 and 165).

Fixture and helper:

```ts
const FORMATIONS: Formations = {
  formations: [
    {
      id: 0,
      name: "close",
      probes: [AWKWARD, [1e9, 2e9, 3e9]],
      ranges: [74798935350, 74798935350],
    },
  ],
  selected: 0,
};

/** The arguments of the last set_probe_formation call. */
const lastSet = () => {
  const c = [...calls.log].reverse().find((x) => x.cmd === "set_probe_formation");
  return c?.args as { id: number | null; name: string; probes: number[][]; ranges: number[] };
};
```

Delete both mixed-range tests and put this in their place:

```ts
describe("per-probe range", () => {
  test("a probe's range picker sends only that probe's new range", async () => {
    // The client sets scan range per probe. A picker per row is the whole
    // point of dropping the old single field, so the other rows must not move.
    await open();
    const row = (await screen.findByLabelText("probe 2 range")) as HTMLSelectElement;
    await fireEvent.change(row, { target: { value: String(149597870700) } });

    expect(lastSet().ranges).toEqual([74798935350, 149597870700]);
  });

  test("the header picker sets every probe's range at once", async () => {
    // Uniform range is still the common case; reaching it by setting eight
    // selects by hand would be a regression on the field this replaces.
    await open();
    const all = await screen.findByLabelText("range for every probe");
    await fireEvent.change(all, { target: { value: String(149597870700) } });

    expect(lastSet().ranges).toEqual([149597870700, 149597870700]);
  });

  test("a formation with differing ranges is editable, not locked read-only", async () => {
    // This inverts the old mixed_range behaviour. That flag guarded against
    // flattening a mix through a single range field; with a field per row
    // there is nothing to flatten (spec §2.1, §5.1).
    calls.stub("probe_formations", {
      formations: [{ ...FORMATIONS.formations[0], ranges: [74798935350, 37399467675] }],
      selected: 0,
    });
    calls.stub("set_probe_formation", FORMATIONS);
    render(ProbeFormationsView, { userOpen: true, userId: 1, onUserDirty: noop });

    const nameField = await screen.findByDisplayValue("close");
    expect((nameField as HTMLInputElement).disabled).toBe(false);
    const row = (await screen.findByLabelText("probe 2 range")) as HTMLSelectElement;
    expect(row.disabled).toBe(false);
    expect(row.value).toBe(String(37399467675));
  });

  test("a range the slider cannot produce is shown on its row, not snapped", async () => {
    const odd = 12345678;
    calls.stub("probe_formations", {
      formations: [{ ...FORMATIONS.formations[0], ranges: [odd, 74798935350] }],
      selected: 0,
    });
    calls.stub("set_probe_formation", FORMATIONS);
    render(ProbeFormationsView, { userOpen: true, userId: 1, onUserDirty: noop });

    const row = (await screen.findByLabelText("probe 1 range")) as HTMLSelectElement;
    expect(row.value).toBe(String(odd));
    expect(row.selectedOptions[0].text).toMatch(/not a slider stop/);
  });
});
```

The three tests at ~lines 86–122 keyed on `findByLabelText("formation range")` also need retargeting. `range offers EVE's slider stops and nothing else` and `choosing a range sends that stop's metres` both move to the header control:

```ts
  test("range offers EVE's slider stops and nothing else", async () => {
    // In-game the scan range is a slider with fixed stops, so a free-text field
    // could write a range the client has no way to represent. A picker also
    // makes a zero-or-negative range — meaningless in EVE, and an invalid SVG
    // radius — unreachable by construction.
    await open();
    const range = (await screen.findByLabelText("range for every probe")) as HTMLSelectElement;
    const offered = [...range.options].map((o) => o.text);
    expect(offered).toEqual([
      "0.25 AU", "0.5 AU", "1 AU", "2 AU", "4 AU", "8 AU", "16 AU", "32 AU", "64 AU",
    ]);
  });

  test("choosing a range sends that stop's metres", async () => {
    await open();
    const range = await screen.findByLabelText("range for every probe");
    await fireEvent.change(range, { target: { value: String(149597870700) } });
    expect(lastSet().ranges).toEqual([149597870700, 149597870700]);
  });
```

Delete the old `a range the slider cannot produce is shown, not snapped to a neighbour` test at ~line 110 — its replacement is in the new block above.

Finally, the `Formation` literals in the create/duplicate tests at ~lines 129–131 drop their dead fields:

```ts
    const a: Formation = { id: 0, name: "a", probes: [[1, 2, 3]], ranges: [74798935350] };
    const b: Formation = { id: 2, name: "b", probes: [[4, 5, 6]], ranges: [74798935350] };
    const created: Formation = { id: 1, name: "New formation", probes: [[0, 0, 0]], ranges: [74798935350] };
```

- [ ] **Step 2: Run them to verify they fail**

Run (in `app/`): `npm run test:ui`
Expected: FAIL — no element labelled `probe 2 range` or `range for every probe`.

- [ ] **Step 3: Replace the draft state and the commit path**

In `app/src/lib/ProbeFormationsView.svelte`'s script, replace `let draftRange = $state(0);` with:

```ts
  let draftRanges = $state<number[]>([]);
```

Replace `draftChanged`:

```ts
  function draftChanged(): boolean {
    if (!current) return false;
    return (
      draftName !== current.name ||
      draftProbes.length !== current.probes.length ||
      draftRanges.some((r, i) => r !== current.ranges[i]) ||
      draftProbes.some((p, i) => p.some((v, j) => v !== current.probes[i][j]))
    );
  }
```

Delete the whole `mixedProbeLabel` derivation.

In `select`, replace the `draftRange` line and add the ranges copy:

```ts
  function select(f: Formation | null) {
    selectedId = f?.id ?? null;
    draftName = f?.name ?? "";
    draftProbes = f ? f.probes.map((p) => [...p] as [number, number, number]) : [];
    draftRanges = f ? [...f.ranges] : [];
    lastAngles = draftProbes.map((p) => { const s = toSpherical(p); return { az: s.az, el: s.el }; });
  }
```

In `commit`, change the IPC call:

```ts
      loaded = await api.setProbeFormation(id, draftName, draftProbes, draftRanges);
```

Replace `addProbe`, `removeProbe` and `createNew` so the two arrays stay the same length — a mismatch is `BadRangeCount` from the model, so this is the invariant the whole task rests on:

```ts
  function addProbe() {
    if (draftProbes.length >= MAX_PROBES) return;
    // The new probe inherits the last probe's range rather than the default:
    // a formation is normally uniform, and inheriting keeps it that way
    // without the user having to notice a picker.
    const r = draftRanges[draftRanges.length - 1] ?? DEFAULT_RANGE_M;
    draftProbes = [...draftProbes, [r / 2, 0, 0]];
    draftRanges = [...draftRanges, r];
    lastAngles = [...lastAngles, { az: 0, el: 0 }];
  }

  function removeProbe(i: number) {
    if (draftProbes.length <= 1) return;
    draftProbes = draftProbes.filter((_, j) => j !== i);
    draftRanges = draftRanges.filter((_, j) => j !== i);
    lastAngles = lastAngles.filter((_, j) => j !== i);
  }

  async function createNew() {
    draftName = "New formation";
    draftProbes = cubeFormation(DEFAULT_RANGE_M);
    draftRanges = draftProbes.map(() => DEFAULT_RANGE_M);
    lastAngles = draftProbes.map((p) => { const s = toSpherical(p); return { az: s.az, el: s.el }; });
    await commit(null);
  }
```

Add the two range setters next to `setAxis`:

```ts
  /** One probe's scan range. */
  function setRange(i: number, metres: number) {
    draftRanges = draftRanges.map((r, j) => (j === i ? metres : r));
    commit();
  }

  /** Every probe's scan range — uniform is the common case, and eight pickers
   * would be a regression on the single field this replaces. */
  function setAllRanges(metres: number) {
    draftRanges = draftRanges.map(() => metres);
    commit();
  }
```

- [ ] **Step 4: Replace the header range control**

In the markup, replace the whole `<label>Range … </label>` block with:

```svelte
          <label>
            Range (all probes)
            <!-- Always AU, and always one of EVE's slider stops: the in-game
                 control has no free value, so neither does this. A picker also
                 makes a non-positive range unwritable by construction. -->
            <select aria-label="range for every probe"
                    value={uniformRange}
                    onchange={(e) => setAllRanges(Number(e.currentTarget.value))}>
              {#each RANGE_STEPS_M as m, i}
                <option value={m}>{RANGE_STEPS_AU[i]} AU</option>
              {/each}
            </select>
          </label>
```

and add its backing derivation to the script, beside `current`:

```ts
  /** The range every probe shares, or `null` when they differ — the header
   * picker shows blank rather than claiming one of the values applies to all. */
  const uniformRange = $derived(
    draftRanges.length && draftRanges.every((r) => r === draftRanges[0]) ? draftRanges[0] : null,
  );
```

Delete the entire `{#if current.mixed_range} … {/if}` `.warn` paragraph.

- [ ] **Step 5: Add the per-probe range column**

Add a header cell to the table's `<thead>` row, between `elevation` and the empty cell:

```svelte
              <th>range</th>
```

Add the matching `<td>` in the body, immediately before the remove-button `<td>`:

```svelte
                <td>
                  <select aria-label={`probe ${n + 1} range`}
                          value={draftRanges[n]}
                          onchange={(e) => setRange(n, Number(e.currentTarget.value))}>
                    {#each RANGE_STEPS_M as m, i}
                      <option value={m}>{RANGE_STEPS_AU[i]} AU</option>
                    {/each}
                    {#if !RANGE_STEPS_M.includes(draftRanges[n])}
                      <!-- A range this file holds that EVE's slider cannot
                           produce. Offered so the value is shown rather than
                           silently snapped to a neighbour. -->
                      <option value={draftRanges[n]}>
                        {formatUnit(draftRanges[n], "au")} AU (not a slider stop)
                      </option>
                    {/if}
                  </select>
                </td>
```

- [ ] **Step 6: Delete every `mixed_range` reference**

Remove `disabled={current.mixed_range}` from: the name input, the eight table inputs (three axis, distance, azimuth, elevation), the per-probe remove button, and the `+ probe` button. The remove button keeps its `draftProbes.length <= 1` condition and `+ probe` keeps its `>= MAX_PROBES` one:

```svelte
                  <button class="mini-visible" title="Remove this probe"
                          disabled={draftProbes.length <= 1}
                          onclick={() => { removeProbe(n); commit(); }}>×</button>
```

```svelte
        <button onclick={() => { addProbe(); commit(); }}
                disabled={draftProbes.length >= MAX_PROBES}>
          + probe
        </button>
```

Also remove the now-unused `duplicate` reference from the deleted warn paragraph — `duplicate` itself stays, it is still the Duplicate button.

- [ ] **Step 7: Keep the existing panes working**

The two SVG panes still read `draftRange`. They are deleted in Task 6, but this task must leave a running app. In the script:

```ts
  const scale = $derived(paneScale(draftProbes, Math.max(0, ...draftRanges), PANE));
```

and in the pane markup, the range circle takes its own probe's range:

```svelte
                  <circle cx={c.cx} cy={c.cy} r={Math.max(0, draftRanges[n] ?? 0) / scale} class="range" />
```

- [ ] **Step 8: Run the tests and the type check**

Run (in `app/`): `npm test && npm run check`
Expected: PASS, and svelte-check reports no errors. If `check` reports `range`/`mixed_range` still referenced anywhere, that is a missed deletion — fix it rather than casting around it.

- [ ] **Step 9: Commit**

```bash
git add app/src/lib/ProbeFormationsView.svelte app/src/lib/ProbeFormationsView.spec.ts
git commit -m "Edit scan range per probe, and drop the mixed-range lockout"
```

---

## Task 4: Camera and projection maths

**Files:**
- Modify: `app/src/lib/probes.ts`
- Modify: `app/src/lib/probes.test.ts`

**Interfaces:**
- Consumes: nothing.
- Produces, all exported from `probes.ts`:
  - `type Vec3 = [number, number, number]`
  - `interface Camera { yaw: number; pitch: number; dist: number; target: Vec3 }`
  - `interface Basis { right: Vec3; up: Vec3; fwd: Vec3; eye: Vec3 }`
  - `const FOV_DEG = 50`, `const PITCH_LIMIT = 89.9`
  - `const SIDE_VIEW: { yaw: number; pitch: number }`, `const TOP_VIEW: { yaw: number; pitch: number }`
  - `cameraBasis(c: Camera): Basis`
  - `focal(size: number): number`
  - `worldPerPixel(depth: number, size: number): number`
  - `projectPoint(p: Vec3, b: Basis, size: number): { x: number; y: number; depth: number; dist: number } | null`
  - `silhouette(dist: number, radius: number, size: number): number | null`
  - `fitDistance(probes: Vec3[], ranges: number[]): number`

**Context for the implementer:** `probes.ts` is imported by `probes.test.ts` under `node --test`, which strips types but runs no bundler. Keep it free of runes, Svelte imports and browser globals.

- [ ] **Step 1: Write the failing tests**

Append to `app/src/lib/probes.test.ts`, and add the new names to the import list at the top of that file — including the two type-only ones, which `node --test` strips:

```ts
import {
  M_PER_AU,
  DEFAULT_RANGE_M,
  toUnit,
  fromUnit,
  toSpherical,
  toCartesian,
  cubeFormation,
  formatUnit,
  paneScale,
  project,
  cameraBasis,
  projectPoint,
  silhouette,
  fitDistance,
  focal,
  FOV_DEG,
  SIDE_VIEW,
  TOP_VIEW,
  type Camera,
  type Vec3,
} from "./probes.ts";
```

(`paneScale` and `project` stay for now; Task 6 deletes them and their checks.)

```ts
// --- camera and projection -------------------------------------------------
// EVE's axes: X and Z are the horizontal plane, Y is up. The `side` camera
// (yaw 90, pitch 0) is the one that puts +X to the right and +Y up, matching
// the side pane this replaces.

const SIZE = 400;
const sideCam = (dist = 1000): Camera => ({ ...SIDE_VIEW, dist, target: [0, 0, 0] });

check("the camera basis is orthonormal", (() => {
  for (const c of [
    { yaw: 0, pitch: 0, dist: 10, target: [0, 0, 0] as Vec3 },
    { yaw: 37, pitch: -22, dist: 1e10, target: [1, 2, 3] as Vec3 },
    { yaw: 200, pitch: 80, dist: 5, target: [0, 0, 0] as Vec3 },
  ]) {
    const b = cameraBasis(c);
    const dot = (u: Vec3, v: Vec3) => u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
    const unit = (u: Vec3) => near(dot(u, u), 1, 1e-9);
    if (!unit(b.right) || !unit(b.up) || !unit(b.fwd)) return false;
    if (!near(dot(b.right, b.up), 0, 1e-9)) return false;
    if (!near(dot(b.right, b.fwd), 0, 1e-9)) return false;
    if (!near(dot(b.up, b.fwd), 0, 1e-9)) return false;
  }
  return true;
})());

check("a pitch of 90 does not produce NaN", (() => {
  // The up vector degenerates when the view direction is parallel to Y, so the
  // basis clamps rather than trusting its caller to.
  const b = cameraBasis({ yaw: 0, pitch: 90, dist: 10, target: [0, 0, 0] });
  return [...b.right, ...b.up, ...b.fwd, ...b.eye].every(Number.isFinite);
})());

check("the target projects to the viewport centre", (() => {
  const p = projectPoint([0, 0, 0], cameraBasis(sideCam()), SIZE);
  return p !== null && near(p.x, SIZE / 2, 1e-6) && near(p.y, SIZE / 2, 1e-6);
})());

check("in the side view +X is to the right", (() => {
  const p = projectPoint([100, 0, 0], cameraBasis(sideCam()), SIZE);
  return p !== null && p.x > SIZE / 2 && near(p.y, SIZE / 2, 1e-6);
})());

check("in the side view +Y is above centre", (() => {
  // SVG's y grows downward, so "above" is a SMALLER y.
  const p = projectPoint([0, 100, 0], cameraBasis(sideCam()), SIZE);
  return p !== null && p.y < SIZE / 2 && near(p.x, SIZE / 2, 1e-6);
})());

check("in the top view +Z is below centre", (() => {
  // The old top-down pane drew +Z upward, the map convention. A camera above
  // the formation sees the opposite, and this is a camera (spec §4.2).
  const b = cameraBasis({ ...TOP_VIEW, dist: 1000, target: [0, 0, 0] });
  const p = projectPoint([0, 0, 100], b, SIZE);
  return p !== null && p.y > SIZE / 2;
})());

check("a point behind the eye does not project", (() => {
  // Reachable by panning, not theoretical. A projection that returned a point
  // anyway would draw it mirrored through the centre.
  const b = cameraBasis(sideCam(1000));
  return projectPoint([0, 0, 5000], b, SIZE) === null;
})());

check("the silhouette radius matches the closed form", (() => {
  // A sphere of radius R at distance d subtends a circle of projected radius
  // f*R/sqrt(d^2 - R^2).
  const r = silhouette(1000, 600, SIZE);
  const want = (focal(SIZE) * 600) / Math.sqrt(1000 * 1000 - 600 * 600);
  return r !== null && near(r, want, 1e-9);
})());

check("a sphere containing the eye has no silhouette", silhouette(500, 600, SIZE) === null);

check("fit frames the furthest probe plus its range", (() => {
  // A sphere of radius `reach` fits the vertical field of view exactly at
  // dist = reach / sin(fov/2).
  const d = fitDistance([[300, 0, 0]], [700]);
  return near(d, 1000 / Math.sin((FOV_DEG * Math.PI) / 360), 1e-6);
})());

check("fit survives a formation with nothing to frame", fitDistance([[0, 0, 0]], [0]) > 0);
```

- [ ] **Step 2: Run them to verify they fail**

Run (in `app/`): `node --test "src/lib/**/*.test.ts"`
Expected: FAIL — `cameraBasis is not defined` and friends.

- [ ] **Step 3: Add the camera block to `probes.ts`**

Append to `app/src/lib/probes.ts`:

```ts
// --- camera, projection and drag -------------------------------------------
//
// A perspective camera written as pure functions so it stays node --test-able,
// which is also why there is no 3D library here: the scene is SVG elements, so
// picking is the browser's job and nothing needs a raycaster (spec §4.1).

export type Vec3 = [number, number, number];

/** An orbit camera. `yaw` and `pitch` are degrees; the eye sits `dist` metres
 * from `target` and always looks at it. */
export interface Camera {
  yaw: number;
  pitch: number;
  dist: number;
  target: Vec3;
}

/** The camera's orthonormal axes and its position, all in world metres. */
export interface Basis {
  right: Vec3;
  up: Vec3;
  fwd: Vec3;
  eye: Vec3;
}

/** Vertical field of view, degrees. */
export const FOV_DEG = 50;

/** Pitch never reaches ±90: the up vector degenerates when the view direction
 * is parallel to Y, and the whole basis comes back NaN. */
export const PITCH_LIMIT = 89.9;

/** X to the right, Y up — the old side (X/Y) pane. */
export const SIDE_VIEW = { yaw: 90, pitch: 0 };
/** Looking down. X to the right, +Z toward the bottom of the screen, which is
 * what a camera above the formation actually sees (spec §4.2). */
export const TOP_VIEW = { yaw: 90, pitch: PITCH_LIMIT };

const RAD = Math.PI / 180;

const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: Vec3, b: Vec3): Vec3 =>
  [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const mul = (a: Vec3, k: number): Vec3 => [a[0] * k, a[1] * k, a[2] * k];
const add = (a: Vec3, b: Vec3): Vec3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const norm = (a: Vec3): Vec3 => {
  const m = Math.hypot(a[0], a[1], a[2]);
  // Only reachable if the pitch clamp is bypassed. Returning a valid axis beats
  // seeding NaN through every coordinate downstream.
  return m === 0 ? [1, 0, 0] : [a[0] / m, a[1] / m, a[2] / m];
};

/** The camera's axes and eye position. Clamps pitch itself, so no caller can
 * produce the degenerate basis. */
export function cameraBasis(c: Camera): Basis {
  const p = Math.max(-PITCH_LIMIT, Math.min(PITCH_LIMIT, c.pitch)) * RAD;
  const y = c.yaw * RAD;
  const cp = Math.cos(p);
  // Unit vector from the target toward the eye.
  const out: Vec3 = [cp * Math.cos(y), Math.sin(p), cp * Math.sin(y)];
  const fwd = mul(out, -1);
  const right = norm(cross(fwd, [0, 1, 0]));
  return { right, up: cross(right, fwd), fwd, eye: add(c.target, mul(out, c.dist)) };
}

/** Focal length in pixels for a viewport `size` px tall. */
export const focal = (size: number) => size / 2 / Math.tan((FOV_DEG * RAD) / 2);

/** World metres per screen pixel at a given camera-space depth. What makes a
 * screen-sized gizmo handle possible: its world length is this times its
 * pixel length. */
export const worldPerPixel = (depth: number, size: number) => depth / focal(size);

/** A world point in viewport pixels, or `null` when it is at or behind the eye
 * plane — reachable by panning, and a point that projected anyway would draw
 * mirrored through the centre.
 *
 * `depth` is the camera-space forward distance, for painter's-order sorting;
 * `dist` is the true distance to the eye, which is what `silhouette` needs. */
export function projectPoint(
  p: Vec3,
  b: Basis,
  size: number,
): { x: number; y: number; depth: number; dist: number } | null {
  const d = sub(p, b.eye);
  const z = dot(d, b.fwd);
  if (z <= 1e-9) return null;
  const f = focal(size);
  return {
    x: size / 2 + (f * dot(d, b.right)) / z,
    y: size / 2 - (f * dot(d, b.up)) / z, // SVG's y grows downward
    depth: z,
    dist: Math.hypot(d[0], d[1], d[2]),
  };
}

/** The projected radius of a sphere's silhouette, or `null` when the eye is
 * inside it. A sphere's silhouette is a circle from every viewpoint, so this
 * is the shape and not an approximation of it (spec §4.4).
 *
 * With eight 0.5 AU spheres, an eye inside one is the normal state at any
 * useful zoom — hence the null rather than a NaN radius. */
export function silhouette(dist: number, radius: number, size: number): number | null {
  if (!(dist > radius)) return null;
  return (focal(size) * radius) / Math.sqrt(dist * dist - radius * radius);
}

/** The camera distance that frames every probe together with its range sphere.
 * A sphere of radius `reach` fits the vertical field of view exactly at
 * `reach / sin(fov/2)`. */
export function fitDistance(probes: Vec3[], ranges: number[]): number {
  const reach = Math.max(
    0,
    ...probes.map((p, i) => Math.hypot(p[0], p[1], p[2]) + Math.abs(ranges[i] ?? 0)),
  );
  // Every probe at the centre with no range has nothing to frame; any positive
  // distance draws it as a dot.
  if (!(reach > 0)) return 1;
  return reach / Math.sin((FOV_DEG * RAD) / 2);
}
```

- [ ] **Step 4: Run the tests**

Run (in `app/`): `node --test "src/lib/**/*.test.ts"`
Expected: PASS, including the existing unit and spherical checks.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/probes.ts app/src/lib/probes.test.ts
git commit -m "Add the probe viewer's camera and projection maths"
```

---

## Task 5: Drag maths

**Files:**
- Modify: `app/src/lib/probes.ts`
- Modify: `app/src/lib/probes.test.ts`

**Interfaces:**
- Consumes: `Vec3`, `Basis`, `projectPoint`, `focal`, `worldPerPixel` from Task 4.
- Produces:
  - `pointerRay(sx: number, sy: number, b: Basis, size: number): Vec3`
  - `axisScreen(p0: Vec3, axis: Vec3, b: Basis, size: number): { dx: number; dy: number; pxPerM: number } | null`
  - `axisDrag(a: { dx: number; dy: number; pxPerM: number }, px: number, py: number): number`
  - `planeHit(sx: number, sy: number, b: Basis, size: number, p0: Vec3, n: Vec3): Vec3 | null`

- [ ] **Step 1: Write the failing tests**

Append to `app/src/lib/probes.test.ts`, adding `axisScreen`, `axisDrag`, `planeHit` and `pointerRay` to the import list:

```ts
// --- drag ------------------------------------------------------------------

check("an axis across the screen drags a pixel delta into metres", (() => {
  // Side view: +X is screen-right, so a rightward pointer delta is +X metres,
  // and the conversion is the pointer travel over the axis's pixels-per-metre.
  const b = cameraBasis(sideCam(1000));
  const a = axisScreen([0, 0, 0], [1, 0, 0], b, SIZE);
  if (!a) return false;
  if (!near(a.dx, 1, 1e-6) || !near(a.dy, 0, 1e-6)) return false;
  return near(axisDrag(a, 40, 0), 40 / a.pxPerM, 1e-9);
})());

check("a drag across an axis moves it nowhere", (() => {
  const b = cameraBasis(sideCam(1000));
  const a = axisScreen([0, 0, 0], [1, 0, 0], b, SIZE);
  return a !== null && near(axisDrag(a, 0, 40), 0, 1e-9);
})());

check("an axis pointing at the camera cannot be dragged", (() => {
  // Side view looks along -Z, so the Z axis is edge-on: its screen length is
  // near zero and the metres-per-pixel diverges. The arrow is invisible in
  // exactly this case, so there is nothing the user could have meant to grab.
  const b = cameraBasis(sideCam(1000));
  return axisScreen([0, 0, 0], [0, 0, 1], b, SIZE) === null;
})());

check("a plane drag hits the plane it was given", (() => {
  // The XY plane through the origin, seen face-on from the side camera.
  const b = cameraBasis(sideCam(1000));
  const hit = planeHit(SIZE / 2, SIZE / 2, b, SIZE, [0, 0, 0], [0, 0, 1]);
  return hit !== null && near(hit[0], 0, 1e-6) && near(hit[1], 0, 1e-6) && near(hit[2], 0, 1e-6);
})());

check("a plane drag returns the locked axis bit-for-bit", (() => {
  // THE precision guarantee (spec §4.7). The intersection maths returns the
  // locked component with float noise on it; taking that value would displace
  // the probe along an axis the user never dragged, on every drag.
  const b = cameraBasis(sideCam(1e11));
  const p0: Vec3 = [-1199120384.7, -115136512.3, -415997952.9];
  const hit = planeHit(SIZE / 2 + 30, SIZE / 2 - 10, b, SIZE, p0, [0, 0, 1]);
  if (hit === null) return false;
  // The caller keeps the locked component; this asserts the value is available
  // to keep, and that the other two actually moved.
  return Object.is([hit[0], hit[1], p0[2]][2], p0[2]) && hit[0] !== p0[0] && hit[1] !== p0[1];
})());

check("a plane seen edge-on is not hit", (() => {
  // Normal perpendicular to the view direction: the ray never meets it.
  const b = cameraBasis(sideCam(1000));
  return planeHit(SIZE / 2, SIZE / 2, b, SIZE, [0, 0, 0], [1, 0, 0]) === null;
})());
```

- [ ] **Step 2: Run them to verify they fail**

Run (in `app/`): `node --test "src/lib/**/*.test.ts"`
Expected: FAIL — `axisScreen is not defined`.

- [ ] **Step 3: Add the drag block to `probes.ts`**

Append to `app/src/lib/probes.ts`:

```ts
/** Unit world direction from the eye through a viewport pixel. */
export function pointerRay(sx: number, sy: number, b: Basis, size: number): Vec3 {
  const f = focal(size);
  return norm(
    add(
      mul(b.fwd, f),
      add(mul(b.right, sx - size / 2), mul(b.up, size / 2 - sy)),
    ),
  );
}

/** An axis at `p0` seen in screen space: a unit screen direction and how many
 * pixels one metre along it covers.
 *
 * `null` when the axis points nearly at or away from the camera. The scale
 * diverges there, and the arrow is edge-on and all but invisible, so there is
 * nothing the user could have meant to grab. */
export function axisScreen(
  p0: Vec3,
  axis: Vec3,
  b: Basis,
  size: number,
): { dx: number; dy: number; pxPerM: number } | null {
  const a = projectPoint(p0, b, size);
  if (!a) return null;
  // A step worth roughly one pixel. A fixed metre step would be ~1e-10 px at
  // formation scale and lose the direction to rounding.
  const step = worldPerPixel(a.depth, size);
  const q = projectPoint(add(p0, mul(axis, step)), b, size);
  if (!q) return null;
  const dx = q.x - a.x;
  const dy = q.y - a.y;
  const len = Math.hypot(dx, dy);
  // A one-pixel step gives len ≈ 1 across the view and ≈ 0 down it; 0.15 is
  // about 8.6° off the view direction.
  if (len < 0.15) return null;
  return { dx: dx / len, dy: dy / len, pxPerM: len / step };
}

/** Metres to move along an axis for a pointer delta in pixels. */
export const axisDrag = (
  a: { dx: number; dy: number; pxPerM: number },
  px: number,
  py: number,
) => (px * a.dx + py * a.dy) / a.pxPerM;

/** Where the ray through viewport pixel (`sx`, `sy`) meets the plane through
 * `p0` with normal `n`, or `null` when the plane is edge-on or the hit is
 * behind the eye.
 *
 * The caller must keep the locked component from `p0` rather than reading it
 * back out of the result: the intersection returns it with float noise on top,
 * which would displace the probe along an axis nobody dragged (spec §4.7). */
export function planeHit(
  sx: number,
  sy: number,
  b: Basis,
  size: number,
  p0: Vec3,
  n: Vec3,
): Vec3 | null {
  const dir = pointerRay(sx, sy, b, size);
  const den = dot(dir, n);
  if (Math.abs(den) < 1e-6) return null;
  const t = dot(sub(p0, b.eye), n) / den;
  if (t <= 0) return null;
  return add(b.eye, mul(dir, t));
}
```

- [ ] **Step 4: Run the tests**

Run (in `app/`): `node --test "src/lib/**/*.test.ts"`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/probes.ts app/src/lib/probes.test.ts
git commit -m "Add the probe viewer's drag maths"
```

---

## Task 6: The viewer — render and camera controls

**Files:**
- Create: `app/src/lib/ProbeViewer.svelte`
- Modify: `app/src/lib/ProbeFormationsView.svelte` (drop the `.panes` block and its helpers, mount the viewer)
- Modify: `app/src/lib/probes.ts` (delete `Plane`, `project`, `paneScale`)
- Modify: `app/src/lib/probes.test.ts` (delete their checks)

**Interfaces:**
- Consumes: everything Task 4 produced.
- Produces: `ProbeViewer.svelte` with props
  ```ts
  { probes: Vec3[]; ranges: number[]; selected: number | null;
    onselect: (i: number | null) => void;
    onmove: (i: number, p: Vec3) => void;
    oncommit: () => void }
  ```
  Task 7 fills in `onmove`/`oncommit`; this task wires them as props and leaves them unused by the render path.

- [ ] **Step 1: Create the viewer**

Create `app/src/lib/ProbeViewer.svelte`:

```svelte
<script lang="ts">
  // The formation in 3D, modelled on the client's own probe view (spec §4).
  //
  // The scene is SVG elements rather than a canvas or a 3D library, so every
  // probe and (in the gizmo) every handle hit-tests itself — no raycaster, no
  // picking pass. SVG paints in document order and has no z-buffer, so
  // everything drawn goes through one depth sort.
  import {
    cameraBasis, projectPoint, silhouette, fitDistance, worldPerPixel,
    PITCH_LIMIT, SIDE_VIEW, TOP_VIEW, type Camera, type Vec3,
  } from "./probes";

  let { probes, ranges, selected, onselect, onmove, oncommit }: {
    probes: Vec3[];
    ranges: number[];
    selected: number | null;
    onselect: (i: number | null) => void;
    onmove: (i: number, p: Vec3) => void;
    oncommit: () => void;
  } = $props();

  const SIZE = 520; // px, square viewport

  let cam = $state<Camera>({ ...SIDE_VIEW, dist: 1, target: [0, 0, 0] });
  const basis = $derived(cameraBasis(cam));

  /** Frame the whole formation. Also the opening view, so it starts where the
   * two panes it replaces used to. */
  function fit() {
    cam = { ...cam, target: [0, 0, 0], dist: fitDistance(probes, ranges) };
  }
  // Re-fit whenever the formation being shown changes shape, but never on a
  // drag: `onmove` mutates a probe's position, and re-fitting mid-drag would
  // move the camera out from under the pointer.
  let fitKey = $derived(`${probes.length}:${ranges.join()}`);
  let lastFitKey = "";
  $effect(() => {
    if (fitKey !== lastFitKey) {
      lastFitKey = fitKey;
      fit();
    }
  });

  /** Every probe projected, with its silhouette, sorted back to front. */
  const drawn = $derived(
    probes
      .map((p, i) => {
        const s = projectPoint(p, basis, SIZE);
        return s === null ? null : { i, p, s, r: silhouette(s.dist, ranges[i] ?? 0, SIZE) };
      })
      .filter((d) => d !== null)
      .sort((a, b) => b.s.depth - a.s.depth),
  );

  /** The three axis stubs, so a free camera's orientation is readable in the
   * picture — the fixed panes carried it in their captions (spec §4.5). */
  const AXES: { v: Vec3; label: string }[] = [
    { v: [1, 0, 0], label: "X" },
    { v: [0, 1, 0], label: "Y" },
    { v: [0, 0, 1], label: "Z" },
  ];
  const axisMarks = $derived.by(() => {
    const o = projectPoint([0, 0, 0], basis, SIZE);
    if (!o) return [];
    // 60 px long whatever the zoom.
    const len = worldPerPixel(o.depth, SIZE) * 60;
    return AXES.map(({ v, label }) => {
      const e = projectPoint([v[0] * len, v[1] * len, v[2] * len], basis, SIZE);
      return e === null ? null : { o, e, label };
    }).filter((a) => a !== null);
  });

  // --- camera controls -----------------------------------------------------
  // Left-drag orbits, right-drag pans, wheel zooms — the client's own bindings.

  let svgEl = $state<SVGSVGElement | undefined>();
  /** Which button started the current camera drag, or null. */
  let camDrag = $state<{ button: number; x: number; y: number } | null>(null);

  function onBackgroundDown(e: PointerEvent) {
    if (e.button !== 0 && e.button !== 2) return;
    // A left press on empty space clears the selection, like the canvas views.
    if (e.button === 0 && e.target === e.currentTarget) onselect(null);
    camDrag = { button: e.button, x: e.clientX, y: e.clientY };
    svgEl?.setPointerCapture(e.pointerId);
  }

  function onMove(e: PointerEvent) {
    if (!camDrag) return;
    const dx = e.clientX - camDrag.x;
    const dy = e.clientY - camDrag.y;
    camDrag = { ...camDrag, x: e.clientX, y: e.clientY };
    if (camDrag.button === 0) {
      cam = {
        ...cam,
        yaw: cam.yaw + dx * 0.4,
        pitch: Math.max(-PITCH_LIMIT, Math.min(PITCH_LIMIT, cam.pitch - dy * 0.4)),
      };
    } else {
      // Pan in the camera's own plane, scaled so the scene tracks the pointer
      // at any zoom.
      const k = worldPerPixel(cam.dist, SIZE);
      const t = cam.target;
      cam = {
        ...cam,
        target: [
          t[0] - (basis.right[0] * dx - basis.up[0] * dy) * k,
          t[1] - (basis.right[1] * dx - basis.up[1] * dy) * k,
          t[2] - (basis.right[2] * dx - basis.up[2] * dy) * k,
        ],
      };
    }
  }

  function onUp(e: PointerEvent) {
    camDrag = null;
    svgEl?.releasePointerCapture(e.pointerId);
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    // Exponential, so one wheel step feels the same at every scale — and the
    // scales here span orders of magnitude.
    cam = { ...cam, dist: cam.dist * Math.exp(Math.sign(e.deltaY) * 0.15) };
  }
</script>

<div class="viewer">
  <svg bind:this={svgEl} viewBox="0 0 {SIZE} {SIZE}" width={SIZE} height={SIZE}
       role="img" aria-label="the formation in 3D"
       onpointerdown={onBackgroundDown}
       onpointermove={onMove}
       onpointerup={onUp}
       onpointercancel={onUp}
       onwheel={onWheel}
       oncontextmenu={(e) => e.preventDefault()}>
    <rect x="0" y="0" width={SIZE} height={SIZE} class="bg" />

    {#each axisMarks as a}
      <line x1={a.o.x} y1={a.o.y} x2={a.e.x} y2={a.e.y} class="axis" />
      <text x={a.e.x} y={a.e.y} class="axis-label">{a.label}</text>
    {/each}

    {#each drawn as d (d.i)}
      {#if d.r !== null}
        <circle cx={d.s.x} cy={d.s.y} r={d.r} class="range" />
      {/if}
      <rect x={d.s.x - 5} y={d.s.y - 5} width="10" height="10"
            class="probe" class:selected={selected === d.i}
            role="button" tabindex="0"
            aria-label={`probe ${d.i + 1}`}
            onpointerdown={(e) => { e.stopPropagation(); onselect(d.i); }} />
    {/each}
  </svg>

  <div class="viewer-actions">
    <button onclick={() => (cam = { ...cam, ...TOP_VIEW })}>Top</button>
    <button onclick={() => (cam = { ...cam, ...SIDE_VIEW })}>Side</button>
    <button onclick={fit}>Fit</button>
    <span class="meta">drag to orbit · right-drag to pan · wheel to zoom</span>
  </div>
</div>

<style>
  .viewer { display: flex; flex-direction: column; gap: 0.35rem; align-items: flex-start; }
  .viewer svg {
    border: 1px solid var(--border); border-radius: 3px;
    touch-action: none; /* or a drag scrolls the page instead of orbiting */
    cursor: grab;
  }
  .bg { fill: var(--bg-panel); }
  .axis { stroke: var(--border); stroke-width: 1; }
  .axis-label { fill: var(--fg-dim); font-size: 10px; }
  .range { fill: rgba(79, 156, 240, 0.06); stroke: rgba(79, 156, 240, 0.35); stroke-width: 1; }
  .probe { fill: var(--accent); cursor: pointer; }
  .probe.selected { fill: var(--warn); stroke: var(--fg); stroke-width: 1; }
  .viewer-actions { display: flex; gap: 4px; align-items: center; }
  .meta { opacity: 0.7; font-size: 0.85em; margin-left: 0.5rem; }
</style>
```

- [ ] **Step 2: Mount it and delete the panes**

In `app/src/lib/ProbeFormationsView.svelte`:

Add the import beside the others:

```ts
  import ProbeViewer from "./ProbeViewer.svelte";
```

Trim the `probes` import to what survives — `paneScale`, `project`, `Plane` are going:

```ts
  import { fromUnit, toSpherical, toCartesian, cubeFormation, formatUnit,
           DEFAULT_RANGE_M, MAX_PROBES, RANGE_STEPS_AU, RANGE_STEPS_M,
           type Unit, type Vec3 } from "./probes";
```

Delete `const PANE`, `const scale`, the `at()` helper and the `PANES` array.

Add the drag handler the viewer calls (Task 7 uses it; wiring it now keeps the prop contract in one place):

```ts
  /** A probe moved in the viewer. Writes only that probe — every other one
   * keeps its exact f64 from the file. */
  function moveProbe(i: number, p: Vec3) {
    draftProbes = draftProbes.map((q, j) => (j === i ? p : q));
    const s = toSpherical(p);
    if (s.r !== 0) lastAngles[i] = { az: s.az, el: s.el };
  }
```

Replace the whole `<div class="panes"> … </div>` block with:

```svelte
        <ProbeViewer probes={draftProbes} ranges={draftRanges} selected={selectedProbe}
                     onselect={(i) => (selectedProbe = i)}
                     onmove={moveProbe}
                     oncommit={() => { if (draftChanged()) commit(); }} />
```

Delete the pane CSS at the bottom of the file: the `.panes`, `.pane`, `.pane figcaption`, `.pane svg`, `.axis`, `.range`, `.probe` and `.probe.selected` rules. They now live in `ProbeViewer.svelte`.

- [ ] **Step 3: Delete the dead helpers from `probes.ts`**

Remove `export type Plane`, `export function project` and `export function paneScale`, together with their doc comments.

In `probes.test.ts`, remove `paneScale` and `project` from the import list and delete the four checks that use them (the two `project` checks at ~lines 74–80 and the two `paneScale` checks at ~lines 85–90).

- [ ] **Step 4: Run everything**

Run (in `app/`): `npm test && npm run check`
Expected: PASS, no svelte-check errors. `ProbeFormationsView.spec.ts` still passes — it never touched the panes.

- [ ] **Step 5: Look at it**

Run the app, open an account file with formations, and confirm: the formation is drawn, left-drag orbits, right-drag pans, the wheel zooms, `Top`/`Side`/`Fit` do what they say, the axis stubs turn with the camera, and clicking a probe highlights both the square and its table row.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/ProbeViewer.svelte app/src/lib/ProbeFormationsView.svelte app/src/lib/probes.ts app/src/lib/probes.test.ts
git commit -m "Replace the two formation panes with a free-camera 3D viewer"
```

---

## Task 7: The gizmo

**Files:**
- Modify: `app/src/lib/ProbeViewer.svelte`

**Interfaces:**
- Consumes: `axisScreen`, `axisDrag`, `planeHit`, `pointerRay` from Task 5; the `onmove`/`oncommit` props from Task 6.
- Produces: no new exports.

**Context for the implementer:** the gizmo is drawn on the selected probe only (spec §3, §4.6). Handles are sized in screen pixels, so their world length is `worldPerPixel(depth, SIZE) * px`. The precision rule in §4.7 is the part most likely to be got wrong: an axis drag writes **one** component and a plane drag writes **two**, with the locked one copied verbatim from the position at pointerdown.

- [ ] **Step 1: Add the gizmo geometry**

In `ProbeViewer.svelte`'s script, extend the `probes` import:

```ts
  import {
    cameraBasis, projectPoint, silhouette, fitDistance, worldPerPixel,
    axisScreen, axisDrag, planeHit,
    PITCH_LIMIT, SIDE_VIEW, TOP_VIEW, type Camera, type Vec3,
  } from "./probes";
```

Add below `axisMarks`:

```ts
  // --- gizmo ---------------------------------------------------------------
  // Handles are sized in SCREEN pixels: the formation spread and the range
  // spheres differ by more than an order of magnitude in real data, so a
  // world-sized gizmo would be a speck at one zoom and fill the view at
  // another (spec §4.6).
  const ARM_PX = 46;   // arrow half-length
  const PLANE_PX = 18; // plane-handle side, offset from the probe by the same

  const UNIT: Vec3[] = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
  const AXIS_CLASS = ["gx", "gy", "gz"];
  /** The two axes each plane handle spans, and the axis it locks. */
  const PLANES: { a: 0 | 1 | 2; b: 0 | 1 | 2; lock: 0 | 1 | 2 }[] = [
    { a: 0, b: 1, lock: 2 },
    { a: 1, b: 2, lock: 0 },
    { a: 2, b: 0, lock: 1 },
  ];

  const step = (p: Vec3, axis: Vec3, k: number): Vec3 =>
    [p[0] + axis[0] * k, p[1] + axis[1] * k, p[2] + axis[2] * k];

  /** The selected probe's handles, in viewport pixels. `null` when nothing is
   * selected or the probe does not project. */
  const gizmo = $derived.by(() => {
    if (selected === null || !probes[selected]) return null;
    const p0 = probes[selected];
    const c = projectPoint(p0, basis, SIZE);
    if (!c) return null;
    const w = worldPerPixel(c.depth, SIZE);
    const arms = UNIT.map((axis, i) => {
      const pos = projectPoint(step(p0, axis, w * ARM_PX), basis, SIZE);
      const neg = projectPoint(step(p0, axis, -w * ARM_PX), basis, SIZE);
      return pos && neg ? { i, pos, neg, cls: AXIS_CLASS[i] } : null;
    }).filter((a) => a !== null);
    const quads = PLANES.map(({ a, b, lock }) => {
      const o = w * PLANE_PX;
      const corners = [
        step(step(p0, UNIT[a], o), UNIT[b], o),
        step(step(p0, UNIT[a], o * 2), UNIT[b], o),
        step(step(p0, UNIT[a], o * 2), UNIT[b], o * 2),
        step(step(p0, UNIT[a], o), UNIT[b], o * 2),
      ].map((q) => projectPoint(q, basis, SIZE));
      if (corners.some((q) => q === null)) return null;
      return {
        lock,
        cls: AXIS_CLASS[lock],
        points: corners.map((q) => `${q!.x},${q!.y}`).join(" "),
      };
    }).filter((q) => q !== null);
    return { c, arms, quads };
  });
```

- [ ] **Step 2: Add the drag state and handlers**

Add below the gizmo derivation:

```ts
  /** A handle drag in progress. `p0` is the probe's position at pointerdown —
   * the source for every locked component, so a drag never rewrites an axis it
   * does not own (spec §4.7). */
  let handleDrag = $state<
    | { kind: "axis"; i: number; comp: 0 | 1 | 2; p0: Vec3; sx: number; sy: number;
        a: { dx: number; dy: number; pxPerM: number } }
    | { kind: "plane"; i: number; lock: 0 | 1 | 2; p0: Vec3 }
    | null
  >(null);

  /** Pointer position in viewport units. The SVG is square and scales with its
   * box, so client pixels convert by the box's own width. */
  function local(e: PointerEvent): { x: number; y: number } {
    const box = svgEl!.getBoundingClientRect();
    const k = SIZE / (box.width || SIZE);
    return { x: (e.clientX - box.left) * k, y: (e.clientY - box.top) * k };
  }

  function startAxis(e: PointerEvent, comp: 0 | 1 | 2) {
    if (e.button !== 0 || selected === null) return;
    e.stopPropagation();
    const p0 = probes[selected];
    const a = axisScreen(p0, UNIT[comp], basis, SIZE);
    // Edge-on: the arrow is invisible and the scale diverges, so there is
    // nothing to drag.
    if (!a) return;
    const l = local(e);
    handleDrag = { kind: "axis", i: selected, comp, p0, sx: l.x, sy: l.y, a };
    svgEl?.setPointerCapture(e.pointerId);
  }

  function startPlane(e: PointerEvent, lock: 0 | 1 | 2) {
    if (e.button !== 0 || selected === null) return;
    e.stopPropagation();
    handleDrag = { kind: "plane", i: selected, lock, p0: probes[selected] };
    svgEl?.setPointerCapture(e.pointerId);
  }

  /** Move the dragged probe. Returns nothing — it calls `onmove`, which writes
   * one probe and leaves every other coordinate in the formation untouched. */
  function dragTo(e: PointerEvent) {
    if (!handleDrag) return;
    const l = local(e);
    if (handleDrag.kind === "axis") {
      const { i, comp, p0, a } = handleDrag;
      const m = axisDrag(a, l.x - handleDrag.sx, l.y - handleDrag.sy);
      const next: Vec3 = [...p0];
      next[comp] = p0[comp] + m; // ONLY this component
      onmove(i, next);
    } else {
      const { i, lock, p0 } = handleDrag;
      const n: Vec3 = [0, 0, 0];
      n[lock] = 1;
      const hit = planeHit(l.x, l.y, basis, SIZE, p0, n);
      if (!hit) return; // plane edge-on this frame
      const next: Vec3 = [...hit];
      // The locked component comes from p0, NOT from the intersection: the
      // maths returns it with float noise on top, which would displace the
      // probe along an axis nobody dragged, on every single drag.
      next[lock] = p0[lock];
      onmove(i, next);
    }
  }
```

- [ ] **Step 3: Route the pointer handlers**

Replace `onMove` and `onUp` so a handle drag takes precedence over a camera drag:

```ts
  function onMove(e: PointerEvent) {
    if (handleDrag) {
      dragTo(e);
      return;
    }
    if (!camDrag) return;
    const dx = e.clientX - camDrag.x;
    const dy = e.clientY - camDrag.y;
    camDrag = { ...camDrag, x: e.clientX, y: e.clientY };
    if (camDrag.button === 0) {
      cam = {
        ...cam,
        yaw: cam.yaw + dx * 0.4,
        pitch: Math.max(-PITCH_LIMIT, Math.min(PITCH_LIMIT, cam.pitch - dy * 0.4)),
      };
    } else {
      const k = worldPerPixel(cam.dist, SIZE);
      const t = cam.target;
      cam = {
        ...cam,
        target: [
          t[0] - (basis.right[0] * dx - basis.up[0] * dy) * k,
          t[1] - (basis.right[1] * dx - basis.up[1] * dy) * k,
          t[2] - (basis.right[2] * dx - basis.up[2] * dy) * k,
        ],
      };
    }
  }

  function onUp(e: PointerEvent) {
    // The file is written once, at the end of the drag — the same rule the
    // table's fields follow on blur.
    if (handleDrag) oncommit();
    handleDrag = null;
    camDrag = null;
    svgEl?.releasePointerCapture(e.pointerId);
  }
```

- [ ] **Step 4: Draw the gizmo**

In the markup, after the `{#each drawn}` block and before `</svg>`:

```svelte
    {#if gizmo}
      <g class="gizmo">
        {#each gizmo.quads as q}
          <polygon points={q.points} class="handle {q.cls}"
                   role="button" tabindex="-1" aria-label="drag in plane"
                   onpointerdown={(e) => startPlane(e, q.lock)} />
        {/each}
        {#each gizmo.arms as a}
          <line x1={a.neg.x} y1={a.neg.y} x2={a.pos.x} y2={a.pos.y} class="arm {a.cls}" />
          <!-- The grab target is a fat transparent line over the thin visible
               one, so a 1 px arrow is still catchable with a mouse. -->
          <line x1={a.neg.x} y1={a.neg.y} x2={a.pos.x} y2={a.pos.y} class="grab"
                role="button" tabindex="-1" aria-label={`drag probe along ${"XYZ"[a.i]}`}
                onpointerdown={(e) => startAxis(e, a.i as 0 | 1 | 2)} />
          <circle cx={a.pos.x} cy={a.pos.y} r="3.5" class="tip {a.cls}" />
          <circle cx={a.neg.x} cy={a.neg.y} r="3.5" class="tip {a.cls}" />
        {/each}
      </g>
    {/if}
```

- [ ] **Step 5: Style it**

Add to `ProbeViewer.svelte`'s `<style>`:

```css
  /* Axis colours, the near-universal convention: X red, Y green, Z blue. */
  .gx { stroke: #e06c6c; fill: #e06c6c; }
  .gy { stroke: #7bc47b; fill: #7bc47b; }
  .gz { stroke: #6c9ce0; fill: #6c9ce0; }
  .arm { stroke-width: 1.5; pointer-events: none; }
  .tip { stroke: none; pointer-events: none; }
  .grab { stroke: transparent; stroke-width: 12; cursor: move; }
  .handle { fill-opacity: 0.25; stroke-width: 1; cursor: move; }
  .handle:hover { fill-opacity: 0.5; }
```

- [ ] **Step 6: Run everything**

Run (in `app/`): `npm test && npm run check`
Expected: PASS.

- [ ] **Step 7: Look at it**

Run the app. Select a probe, then: drag each arrow and confirm the table's X, Y or Z field moves while the other two do not; drag each plane quad and confirm exactly two fields move; release and confirm the formation saves once (the unsaved badge lights, and reloading the file shows the new position). Orbit until an arrow is edge-on and confirm it cannot be grabbed rather than flinging the probe.

- [ ] **Step 8: Commit**

```bash
git add app/src/lib/ProbeViewer.svelte
git commit -m "Drag probes by axis and by plane in the viewer"
```

---

## Task 8: The vector view, and the release notes

**Files:**
- Modify: `app/src/lib/ProbeViewer.svelte`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: everything above.
- Produces: nothing.

**Context for the implementer:** this is the client's Alt state (spec §2, state 3 and §4.8) as a checkbox rather than a held key. The client hides the gizmos in this state, and so does this.

- [ ] **Step 1: Add the toggle**

In `ProbeViewer.svelte`'s script:

```ts
  /** The client's Alt view: each probe as a vector from the formation centre.
   * A checkbox rather than a held key — a modifier the camera also wants is a
   * bad trade in a window you type numbers into (spec §3). */
  let vectors = $state(false);
```

In the markup, add a `defs` block once, just inside `<svg>` after the background `<rect>`:

```svelte
    <defs>
      <marker id="probe-vec-head" viewBox="0 0 10 10" refX="9" refY="5"
              markerWidth="6" markerHeight="6" orient="auto-start-reverse">
        <path d="M 0 0 L 10 5 L 0 10 z" class="vec-head" />
      </marker>
    </defs>
```

Add the vector lines immediately before the `{#each drawn}` block, so they paint behind the probes:

```svelte
    {#if vectors}
      {@const o = projectPoint([0, 0, 0], basis, SIZE)}
      {#if o}
        {#each drawn as d (d.i)}
          <line x1={o.x} y1={o.y} x2={d.s.x} y2={d.s.y} class="vec"
                marker-end="url(#probe-vec-head)" />
        {/each}
      {/if}
    {/if}
```

Gate the gizmo on it — the client shows no handles in this state:

```svelte
    {#if gizmo && !vectors}
```

And add the control to `.viewer-actions`, before the `.meta` span:

```svelte
    <label class="toggle">
      <input type="checkbox" bind:checked={vectors} />
      Vectors
    </label>
```

- [ ] **Step 2: Style it**

Add to the `<style>` block:

```css
  .vec { stroke: var(--accent); stroke-width: 1; stroke-dasharray: 4 3; opacity: 0.7; }
  .vec-head { fill: var(--accent); stroke: none; }
  .toggle { display: flex; align-items: center; gap: 4px; font-size: 0.85em; color: var(--fg-dim); }
</style>
```

- [ ] **Step 3: Run everything**

Run (in `app/`): `npm test && npm run check`
Expected: PASS.

- [ ] **Step 4: Write the release notes**

In `CHANGELOG.md`, under `## [Unreleased]`, add:

```markdown
### Added
- Edit the probe formation in 3D: orbit, pan and zoom a free camera, with `Top`, `Side` and `Fit` shortcuts.
- Drag a probe along an axis or across a plane to place it.
- Show each probe as a vector from the formation's centre.
- Set scan range per probe, as the game does.

### Changed
- The formation's two flat previews are now one 3D view.
- A formation whose probes carry different scan ranges is editable, instead of read-only.
```

- [ ] **Step 5: Look at it**

Run the app. Tick `Vectors` and confirm dashed arrows run from the centre to each probe and the gizmo disappears; untick and confirm it returns.

- [ ] **Step 6: Full check and commit**

Run: `cargo test --workspace` and, in `app/`, `npm test && npm run check`
Expected: all green.

```bash
git add app/src/lib/ProbeViewer.svelte CHANGELOG.md
git commit -m "Show probes as vectors from the formation centre"
```

---

## Self-review notes

**Spec coverage.** §2.1 per-probe range → Tasks 1–3. §3 selection model → Task 6 (`onselect`) and Task 7 (gizmo gated on `selected`). §4.1 SVG, no library → Task 6. §4.2 camera → Tasks 4 and 6. §4.3 projection and depth sort → Task 4, sorted in Task 6's `drawn`. §4.4 silhouettes → Task 4, drawn in Task 6. §4.5 axis indicator → Task 6's `axisMarks`. §4.6 gizmo → Task 7. §4.7 drag and the precision rule → Tasks 5 and 7. §4.8 vector view → Task 8. §5 per-probe range → Tasks 1–3. §6 deletions → Tasks 3 and 6. §7 testing → Tasks 1, 3, 4, 5.

**Deliberately not covered by an automated test:** the drag wiring in Task 7. `getBoundingClientRect` is 0×0 under jsdom, so `local()` would divide by a fiction and the test would assert against it. The maths is covered in Tasks 4 and 5; the wiring gets the hands-on check in Task 7 Step 7. This is the spec's §7 decision, not an omission.
