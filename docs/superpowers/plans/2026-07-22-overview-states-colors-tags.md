# Overview states, colours and tags — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make EVE's overview state settings editable — account-wide background
colours and colortags, and each filter preset's hide / always-show exceptions.

**Architecture:** A new `overview_states.rs` in `settings-model` authors the four
account-scoped state lists, the sparse `stateColors` map and the overview
container's boolean settings; `overview_presets.rs` gains one function for the
per-preset state lists. All of it reaches the container through the existing
`overview_tabs::overview_mut` helper and is driven from `ops.rs` through the
existing `edit_user_tabs` wrapper, so no new plumbing is introduced. The
frontend splits the 518-line `OverviewView.svelte` into three sub-tab components
named after EVE's own Overview Settings window.

**Tech Stack:** Rust (dependency-free `settings-model` + `blue-marshal` crates),
Tauri commands, SvelteKit 5 (runes) frontend, `node --test` for frontend tests.

**Spec:** `docs/superpowers/specs/2026-07-22-overview-states-colors-tags-design.md`

## Global Constraints

- **Zero new dependencies.** `settings-model` and `blue-marshal` are
  dependency-free by policy; the frontend adds no packages (the colour picker is
  a native `<input type="color">`).
- **No personal data in committed files.** Never commit a real character id,
  account id, character name or account alias into source, tests, fixtures or
  docs. Test fixtures are hand-built `Value` trees, not corpus copies.
- **Never write to the live EVE directory.** `tools/sync-corpus.ps1` is the only
  code permitted to touch it, and only for reading.
- **Commit messages are sentence-case with no attribution trailers.** No
  `Co-Authored-By`, no `Generated with` footer. (Repo convention — this overrides
  the default commit format.)
- **Inline-first idiom.** Every structural edit calls `inline_all(v)` first; the
  app layer reshares afterwards. Never hand-build `Shared`/`Ref` nodes.
- **Preserve the `(timestamp, value)` wrapper.** Edit the inner value in place;
  never replace the tuple or invent a timestamp for an existing key. Freshly
  minted containers use a zero `Long`, matching `presets_mut_or_create`.
- **Native form controls need explicit dark styling** — `input[type=color]`,
  checkboxes and radios render light in the dark WebView2 shell otherwise.

**Commands** (npm is not on the Bash PATH — run npm via PowerShell):

| Purpose | Command |
|---|---|
| Rust tests | `cargo test -p settings-model` |
| Frontend tests | `npm test` (from `app/`) |
| Type check | `npm run check` (from `app/`) |
| Frontend build | `npm run build` (from `app/`) |

---

## File Structure

**Create:**
- `crates/settings-model/src/overview_states.rs` — account-scoped state lists,
  `stateColors`, boolean settings. Tasks 2–4.
- `app/src/lib/data/overview-states.json` — bundled id→label vocabulary and
  default arrays. Task 1.
- `app/src/lib/states.ts` + `app/src/lib/states.test.ts` — vocabulary lookup and
  tri-state ↔ two-list mapping. Tasks 1, 9.
- `app/src/lib/OverviewColumnsTab.svelte` — existing column editor, moved. Task 8.
- `app/src/lib/OverviewFiltersTab.svelte` — preset picker + Types Shown +
  Exceptions. Tasks 8, 9.
- `app/src/lib/OverviewAppearanceTab.svelte` — booleans + Colortag/Background.
  Tasks 8, 10.

**Modify:**
- `crates/settings-model/src/overview_presets.rs` — add `set_preset_states`. Task 5.
- `crates/settings-model/src/overview.rs` — extend the projection. Task 6.
- `crates/settings-model/src/lib.rs` — module declaration and re-exports. Tasks 2, 5, 6.
- `app/src-tauri/src/ops.rs`, `app/src-tauri/src/lib.rs` — commands. Task 7.
- `app/src/lib/api.ts` — types and invoke wrappers. Task 7.
- `app/src/lib/OverviewView.svelte` — becomes the sub-tab host. Task 8.
- `docs/format-notes.md` — record the state model. Task 11.

---

### Task 1: Resolve ids 36/37 and author the state vocabulary

The 22 pilot-state labels are derived and colour-verified (spec §2.3), and the
`36`/`37` ambiguity was **resolved by live experiment on 2026-07-22 — Step 1 is
already done**. Setting "Wreck is empty" to Hide in-game moved `37` into a
preset's `filteredStates` while `36` stayed put, giving **36 = "Wreck is already
viewed", 37 = "Wreck is empty"**. Step 1 is kept below as the record of how the
fact was established; start this task at Step 2.

**Files:**
- Create: `app/src/lib/data/overview-states.json`
- Create: `app/src/lib/states.ts`
- Create: `app/src/lib/states.test.ts`

**Interfaces:**
- Consumes: nothing.
- Produces: `stateLabel(id: number): string | null`,
  `EXCEPTION_STATES: number[]`, `DEFAULT_BACKGROUND_ORDER: number[]`,
  `DEFAULT_BACKGROUND_STATES: number[]`, `DEFAULT_FLAG_ORDER: number[]`,
  `DEFAULT_FLAG_STATES: number[]` from `app/src/lib/states.ts`.

- [x] **Step 1: Run the live experiment to pin ids 36 and 37** — DONE 2026-07-22.
  Result: **37 = "Wreck is empty", 36 = "Wreck is already viewed".** Exactly one
  account file changed; its diff was timestamp churn plus one preset's
  `filteredStates` going `[] → [37]`, with `36` untouched in a sibling preset's
  list. The dump also surfaced a new `filterOut` `(ts, None)` sibling key in the
  overview container — not a state key, not touched by this slice.

This step needs the EVE client and a human. It follows the project's existing
experiment pattern (`testdata/exp*.diff`).

1. Capture a before-snapshot:
   `powershell -File tools/sync-corpus.ps1 -Label states-before`
2. In EVE: **Overview Settings → Filters**, select any filter, open the
   **Exceptions** sub-tab.
3. Set **"Wreck is empty"** — and only that row — to the middle column (hide).
   Leave every other row alone. Save the filter.
4. Log out fully so the client flushes settings to disk.
5. Capture an after-snapshot:
   `powershell -File tools/sync-corpus.ps1 -Label states-after`
6. Dump both copies of the same account file and diff the changed preset's
   `filteredStates`:
   ```bash
   cargo run -q -p blue-marshal --bin bmdump -- dump <before>/core_user_<id>.dat > /tmp/before.txt
   cargo run -q -p blue-marshal --bin bmdump -- dump <after>/core_user_<id>.dat  > /tmp/after.txt
   diff /tmp/before.txt /tmp/after.txt
   ```

Expected: exactly one `filteredStates` list gains a single integer. **That
integer is "Wreck is empty"**; the other of `{36, 37}` is "Wreck is already
viewed". Record which.

- [ ] **Step 2: Write the vocabulary JSON**

Create `app/src/lib/data/overview-states.json` exactly as below. The `36`/`37`
labels are the experiment's verified result — do not swap them.

```json
{
  "_note": "Hand-authored. State ids carry no label in the settings file and there is no ESI or fsdbinary source; the id->label map lives in client script. Derived positionally: EVE's Overview Settings > Appearance > Background list renders in exactly backgroundOrder2 order, so a screenshot maps row-by-row onto that array. Verified against stateColors: all 13 stored colours match the swatch on their mapped row. Ids 36/37 pinned by live experiment (see docs/superpowers/plans/2026-07-22-overview-states-colors-tags.md Task 1). Id 68 is present in the order arrays but never rendered by the client and is deliberately unnamed.",
  "states": {
    "9": "Pilot has a security status below -5",
    "10": "Pilot has a security status below 0",
    "11": "Pilot is in your fleet",
    "12": "Pilot is in your Capsuleer corporation",
    "13": "Pilot is at war with your corporation/alliance",
    "14": "Pilot is in your alliance",
    "15": "Pilot has Excellent Standing",
    "16": "Pilot has Good Standing",
    "17": "Pilot has No Standing",
    "18": "Pilot has Bad Standing",
    "19": "Pilot has Terrible Standing",
    "20": "Pilot (agent) is interactable",
    "21": "Pilot has Neutral Standing",
    "36": "Wreck is already viewed",
    "37": "Wreck is empty",
    "44": "Pilot is at war with your militia",
    "45": "Pilot is in your militia or allied to your militia",
    "48": "Pilot is in your Non Capsuleer corporation",
    "49": "Pilot is an ally in one or more of your wars",
    "50": "Pilot is a suspect",
    "51": "Pilot is a criminal",
    "52": "Pilot has a limited engagement with you",
    "53": "Pilot has a kill right on them that you can activate",
    "66": "Pilot has retribution timer"
  },
  "exceptionStates": [9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 36, 37, 44, 45, 48, 49, 50, 51, 52, 53, 66],
  "defaultBackgroundOrder": [13, 44, 52, 11, 12, 14, 15, 16, 45, 49, 19, 18, 9, 51, 50, 53, 10, 20, 21, 17, 48, 66, 68],
  "defaultBackgroundStates": [9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 44, 45, 48, 49, 50, 51, 52, 53, 66],
  "defaultFlagOrder": [13, 44, 52, 11, 12, 14, 15, 16, 45, 9, 51, 50, 53, 19, 18, 49, 10, 17, 48, 21, 20, 66, 68],
  "defaultFlagStates": [9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 44, 45, 48, 49, 50, 51, 52, 53, 66]
}
```

The two `default*Order` arrays are real observed orders (any permutation is
valid — it is a priority list). The two `default*States` arrays are
"everything enabled", which is EVE's out-of-the-box behaviour, and deliberately
exclude `68` since it is not a real state.

- [ ] **Step 3: Write the failing test**

Create `app/src/lib/states.test.ts`:

```ts
import { test } from "node:test";
import assert from "node:assert/strict";
import { stateLabel, EXCEPTION_STATES, DEFAULT_BACKGROUND_ORDER } from "./states";

test("stateLabel resolves a known id", () => {
  assert.equal(stateLabel(51), "Pilot is a criminal");
});

test("stateLabel returns null for the unrendered id 68", () => {
  assert.equal(stateLabel(68), null);
});

test("stateLabel returns null for an unknown id", () => {
  assert.equal(stateLabel(9999), null);
});

test("the exception vocabulary includes the wreck states and excludes 68", () => {
  assert.ok(EXCEPTION_STATES.includes(36));
  assert.ok(EXCEPTION_STATES.includes(37));
  assert.ok(!EXCEPTION_STATES.includes(68));
});

test("the default background order carries the unrendered id 68", () => {
  assert.ok(DEFAULT_BACKGROUND_ORDER.includes(68));
});
```

- [ ] **Step 4: Run the test to verify it fails**

Run (PowerShell, from `app/`): `npm test`
Expected: FAIL — cannot resolve module `./states`.

- [ ] **Step 5: Write the implementation**

Create `app/src/lib/states.ts`:

```ts
import bundle from "./data/overview-states.json";

const STATES: Record<string, string> = bundle.states;

/** Human label for a state id, or null when EVE stores the id but never shows
 *  it (id 68) or the bundle predates it. Callers render `#<id>` for null. */
export function stateLabel(id: number): string | null {
  return STATES[String(id)] ?? null;
}

/** States offered on a preset's Exceptions list — the 22 pilot states plus the
 *  two Wreck states. Excludes 68, which the client never renders. */
export const EXCEPTION_STATES: number[] = bundle.exceptionStates;

export const DEFAULT_BACKGROUND_ORDER: number[] = bundle.defaultBackgroundOrder;
export const DEFAULT_BACKGROUND_STATES: number[] = bundle.defaultBackgroundStates;
export const DEFAULT_FLAG_ORDER: number[] = bundle.defaultFlagOrder;
export const DEFAULT_FLAG_STATES: number[] = bundle.defaultFlagStates;
```

- [ ] **Step 6: Run the test to verify it passes**

Run (from `app/`): `npm test`
Expected: PASS, including the pre-existing suites.

- [ ] **Step 7: Commit**

```bash
git add app/src/lib/data/overview-states.json app/src/lib/states.ts app/src/lib/states.test.ts
git commit -m "Add the bundled overview state vocabulary"
```

---

### Task 2: Backend — the four account-scoped state lists

**Files:**
- Create: `crates/settings-model/src/overview_states.rs`
- Modify: `crates/settings-model/src/lib.rs`

**Interfaces:**
- Consumes: `overview_tabs::{overview_mut, is_b, dict_inner_mut, OverviewTabError}`
  and `treewalk::inline_all` (all already `pub(crate)`).
- Produces: `pub enum StateList { Background, BackgroundOrder, Flag, FlagOrder }`
  and `pub fn set_state_list(v: &mut Value, which: StateList, ids: &[i64]) -> Result<(), OverviewTabError>`.

- [ ] **Step 1: Write the failing tests**

Create `crates/settings-model/src/overview_states.rs` with only the test module
plus the imports it needs:

```rust
//! Structural authoring for the account-scoped overview *state* settings: which
//! pilot states tint an overview row (`backgroundStates2`) or carry a colortag
//! (`flagStates2`), the priority order of each (`backgroundOrder2` /
//! `flagOrder2`), the sparse per-state colour overrides (`stateColors`), and the
//! container's boolean settings. All live directly in the user file's `overview`
//! container. Edits use the same inline-first idiom as `overview_tabs.rs` and
//! reuse its `pub(crate)` helpers; the app layer reshares before saving.
//!
//! The enabled lists and the order lists are INDEPENDENT: an order list
//! enumerates every state the client knows regardless of whether it is ticked,
//! and can contain an id the client never renders (id 68 on current files), so
//! writes must preserve unknown ids rather than rebuild from what is on screen.

use blue_marshal::Value;

use crate::overview_tabs::{dict_inner_mut, is_b, overview_mut, OverviewTabError};
use crate::treewalk::inline_all;

#[cfg(test)]
mod tests {
    use super::*;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }

    fn ints(v: &Value) -> Vec<i64> {
        let inner = match v {
            Value::List(l) => l,
            Value::Tuple(items) => match items.iter().find(|e| matches!(e, Value::List(_))) {
                Some(Value::List(l)) => l,
                _ => return Vec::new(),
            },
            _ => return Vec::new(),
        };
        inner.iter().filter_map(|e| if let Value::Int(n) = e { Some(*n) } else { None }).collect()
    }

    fn read(v: &Value, key: &str) -> Vec<i64> {
        let Value::Dict(root) = v else { return Vec::new() };
        let Some((_, ov)) = root.iter().find(|(k, _)| is_b(k, b"overview")) else { return Vec::new() };
        let Value::Dict(ovd) = ov else { return Vec::new() };
        ovd.iter().find(|(k, _)| is_b(k, key.as_bytes())).map(|(_, v)| ints(v)).unwrap_or_default()
    }

    /// user -> overview -> the four state keys, each a (ts, [int]) tuple.
    /// The order lists carry id 68, which the client stores but never renders.
    fn user_with_states() -> Value {
        let list = |ids: &[i64]| Value::Tuple(vec![
            Value::Long(vec![0u8; 8]),
            Value::List(ids.iter().map(|n| Value::Int(*n)).collect()),
        ]);
        Value::Dict(vec![(b("overview"), Value::Dict(vec![
            (b("backgroundStates2"), list(&[9, 13, 44])),
            (b("backgroundOrder2"), list(&[13, 44, 9, 68])),
            (b("flagStates2"), list(&[9, 13])),
            (b("flagOrder2"), list(&[13, 9, 44, 68])),
        ]))])
    }

    /// A clean account: an overview container with no state keys at all.
    fn user_without_states() -> Value {
        Value::Dict(vec![(b("overview"), Value::Dict(vec![
            (b("tabsettings_new"), Value::Dict(Vec::new())),
        ]))])
    }

    #[test]
    fn enabled_list_is_written_sorted() {
        let mut v = user_with_states();
        set_state_list(&mut v, StateList::Background, &[44, 9, 13]).unwrap();
        assert_eq!(read(&v, "backgroundStates2"), vec![9, 13, 44]);
    }

    #[test]
    fn order_list_keeps_caller_order() {
        let mut v = user_with_states();
        set_state_list(&mut v, StateList::BackgroundOrder, &[44, 9, 68, 13]).unwrap();
        assert_eq!(read(&v, "backgroundOrder2"), vec![44, 9, 68, 13]);
    }

    #[test]
    fn flag_lists_are_independent_of_background() {
        let mut v = user_with_states();
        set_state_list(&mut v, StateList::Flag, &[44]).unwrap();
        assert_eq!(read(&v, "flagStates2"), vec![44]);
        assert_eq!(read(&v, "backgroundStates2"), vec![9, 13, 44], "background untouched");
    }

    #[test]
    fn timestamp_wrapper_survives_the_edit() {
        let mut v = user_with_states();
        set_state_list(&mut v, StateList::Background, &[9]).unwrap();
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, val) = ovd.iter().find(|(k, _)| is_b(k, b"backgroundStates2")).unwrap();
        assert!(matches!(val, Value::Tuple(_)), "the (ts, list) wrapper must be preserved");
    }

    #[test]
    fn absent_keys_are_materialised_on_first_edit() {
        let mut v = user_without_states();
        set_state_list(&mut v, StateList::Background, &[9, 13]).unwrap();
        assert_eq!(read(&v, "backgroundStates2"), vec![9, 13]);
    }

    #[test]
    fn unrendered_id_68_survives_a_toggle() {
        let mut v = user_with_states();
        // Enabling one more state must not disturb the order list that holds 68.
        set_state_list(&mut v, StateList::Background, &[9, 13, 44, 52]).unwrap();
        assert!(read(&v, "backgroundOrder2").contains(&68), "id 68 must round-trip");
    }

    #[test]
    fn no_overview_container_is_an_error() {
        let mut v = Value::Dict(vec![(b("ui"), Value::Dict(Vec::new()))]);
        assert!(matches!(
            set_state_list(&mut v, StateList::Background, &[9]),
            Err(OverviewTabError::NoOverview)
        ));
    }
}
```

Declare the module in `crates/settings-model/src/lib.rs`, beside the other
`overview_*` modules:

```rust
pub mod overview_states;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p settings-model overview_states`
Expected: FAIL to compile — `set_state_list` and `StateList` not found.

- [ ] **Step 3: Write the implementation**

Add above the test module in `overview_states.rs`:

```rust
/// Which of the four account-scoped state lists to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateList {
    /// `backgroundStates2` — states that tint a row (the ticked subset).
    Background,
    /// `backgroundOrder2` — every known state in row-tint priority order.
    BackgroundOrder,
    /// `flagStates2` — states that carry a colortag (the ticked subset).
    Flag,
    /// `flagOrder2` — every known state in colortag priority order.
    FlagOrder,
}

impl StateList {
    fn key(self) -> &'static [u8] {
        match self {
            StateList::Background => b"backgroundStates2",
            StateList::BackgroundOrder => b"backgroundOrder2",
            StateList::Flag => b"flagStates2",
            StateList::FlagOrder => b"flagOrder2",
        }
    }

    /// Enabled lists are stored sorted ascending (EVE's own convention on real
    /// files, and what `set_preset_groups` does for `groups`). Order lists are
    /// a priority sequence and must keep the caller's order.
    fn sorted(self) -> bool {
        matches!(self, StateList::Background | StateList::Flag)
    }
}

/// Replace one of the four account-scoped state lists.
///
/// Preserves an existing `(timestamp, list)` wrapper, and mints one — with a
/// zero `Long`, matching `presets_mut_or_create` — when the key is absent, which
/// is the case on an account that has never customised its overview states.
///
/// The caller owns the contents: `ids` is written as given (modulo the sort for
/// enabled lists), so a caller rebuilding an order list from a UI MUST carry
/// over ids the client does not render, or they are silently dropped.
pub fn set_state_list(v: &mut Value, which: StateList, ids: &[i64]) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;

    let mut out = ids.to_vec();
    if which.sorted() {
        out.sort_unstable();
        out.dedup();
    }
    let list = Value::List(out.into_iter().map(Value::Int).collect());

    match ov.iter_mut().find(|(k, _)| is_b(k, which.key())) {
        // Existing key: replace the inner list, leaving the (ts, _) wrapper.
        Some((_, existing)) => match existing {
            Value::Tuple(items) => {
                match items.iter_mut().find(|e| matches!(e, Value::List(_))) {
                    Some(slot) => *slot = list,
                    None => items.push(list),
                }
            }
            other => *other = list,
        },
        // Absent: mint a fresh (ts, list). EVE re-timestamps on its next save.
        None => ov.push((
            Value::Bytes(which.key().to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), list]),
        )),
    }
    Ok(())
}
```

Note `dict_inner_mut` is imported for Tasks 3–4; if the compiler warns it is
unused at this point, leave the import and let Task 3 consume it.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p settings-model overview_states`
Expected: PASS — 7 tests.

- [ ] **Step 5: Export from the crate root**

In `crates/settings-model/src/lib.rs`, add beside the `overview_presets`
re-export:

```rust
pub use overview_states::{set_state_list, StateList};
```

- [ ] **Step 6: Run the full crate test suite**

Run: `cargo test -p settings-model`
Expected: PASS — no regressions in the existing suites.

- [ ] **Step 7: Commit**

```bash
git add crates/settings-model/src/overview_states.rs crates/settings-model/src/lib.rs
git commit -m "Author the account-scoped overview state lists"
```

---

### Task 3: Backend — sparse per-state colours

`stateColors` is a dict keyed by a **tuple** `(surface_bytes, state_id)` mapping
to an RGBA 4-tuple of floats. It is **sparse**: an absent entry means EVE's
built-in default colour for that state, not black. Across the whole corpus the
surface is always `b"background"`; any other surface must round-trip untouched.

**Files:**
- Modify: `crates/settings-model/src/overview_states.rs`
- Modify: `crates/settings-model/src/lib.rs`

**Interfaces:**
- Consumes: Task 2's module, imports and test helpers.
- Produces: `pub fn set_state_color(v: &mut Value, id: i64, rgba: Option<[f64; 4]>) -> Result<(), OverviewTabError>`
  and `pub fn state_colors(v: &Value) -> Vec<(i64, [f64; 4])>`.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `overview_states.rs`:

```rust
    fn rgba(r: f64, g: f64, bl: f64, a: f64) -> Value {
        Value::Tuple(vec![Value::Float(r), Value::Float(g), Value::Float(bl), Value::Float(a)])
    }

    fn color_key(surface: &str, id: i64) -> Value {
        Value::Tuple(vec![b(surface), Value::Int(id)])
    }

    /// user -> overview -> stateColors: (ts, { ("background", id): (r,g,b,a) })
    /// Includes one entry on a foreign surface, which must never be touched.
    fn user_with_colors() -> Value {
        Value::Dict(vec![(b("overview"), Value::Dict(vec![
            (b("stateColors"), Value::Tuple(vec![
                Value::Long(vec![0u8; 8]),
                Value::Dict(vec![
                    (color_key("background", 44), rgba(0.75, 0.0, 0.0, 1.0)),
                    (color_key("background", 20), rgba(0.7, 0.7, 0.7, 0.5)),
                    (color_key("bracket", 44), rgba(0.1, 0.2, 0.3, 1.0)),
                ]),
            ])),
        ]))])
    }

    #[test]
    fn projects_only_the_background_surface() {
        let v = user_with_colors();
        let mut got = state_colors(&v);
        got.sort_by_key(|(id, _)| *id);
        assert_eq!(got, vec![(20, [0.7, 0.7, 0.7, 0.5]), (44, [0.75, 0.0, 0.0, 1.0])]);
    }

    #[test]
    fn sets_a_colour_for_a_state_with_no_entry() {
        let mut v = user_with_colors();
        set_state_color(&mut v, 13, Some([1.0, 0.0, 0.0, 1.0])).unwrap();
        assert!(state_colors(&v).contains(&(13, [1.0, 0.0, 0.0, 1.0])));
    }

    #[test]
    fn overwrites_an_existing_colour() {
        let mut v = user_with_colors();
        set_state_color(&mut v, 44, Some([0.0, 1.0, 0.0, 1.0])).unwrap();
        assert!(state_colors(&v).contains(&(44, [0.0, 1.0, 0.0, 1.0])));
        assert_eq!(state_colors(&v).iter().filter(|(id, _)| *id == 44).count(), 1);
    }

    #[test]
    fn none_removes_the_entry_restoring_eves_default() {
        let mut v = user_with_colors();
        set_state_color(&mut v, 44, None).unwrap();
        assert!(!state_colors(&v).iter().any(|(id, _)| *id == 44));
    }

    #[test]
    fn removing_an_absent_entry_is_a_no_op() {
        let mut v = user_with_colors();
        set_state_color(&mut v, 13, None).unwrap();
        assert_eq!(state_colors(&v).len(), 2);
    }

    #[test]
    fn a_foreign_surface_entry_is_preserved() {
        let mut v = user_with_colors();
        set_state_color(&mut v, 44, Some([0.0, 0.0, 1.0, 1.0])).unwrap();
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, sc) = ovd.iter().find(|(k, _)| is_b(k, b"stateColors")).unwrap();
        let Value::Tuple(items) = sc else { panic!() };
        let Some(Value::Dict(d)) = items.iter().find(|e| matches!(e, Value::Dict(_))) else { panic!() };
        assert!(
            d.iter().any(|(k, _)| *k == color_key("bracket", 44)),
            "a non-background surface must round-trip untouched"
        );
    }

    #[test]
    fn colours_can_be_set_when_the_key_is_absent() {
        let mut v = user_without_states();
        set_state_color(&mut v, 13, Some([1.0, 0.0, 0.0, 1.0])).unwrap();
        assert_eq!(state_colors(&v), vec![(13, [1.0, 0.0, 0.0, 1.0])]);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p settings-model overview_states`
Expected: FAIL to compile — `set_state_color` and `state_colors` not found.

- [ ] **Step 3: Write the implementation**

Add to `overview_states.rs`:

```rust
/// The surface component of a `stateColors` key. Only this surface is edited;
/// any other is read past and written back untouched.
const BACKGROUND_SURFACE: &[u8] = b"background";

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Float(f) => Some(*f),
        Value::Int(i) => Some(*i as f64),
        _ => None,
    }
}

/// Read a `(surface, id)` colour key, returning the id only for the background
/// surface.
fn background_color_id(k: &Value) -> Option<i64> {
    let Value::Tuple(parts) = k else { return None };
    let [surface, id] = parts.as_slice() else { return None };
    match (surface, id) {
        (Value::Bytes(s), Value::Int(n)) if s.as_slice() == BACKGROUND_SURFACE => Some(*n),
        _ => None,
    }
}

fn as_rgba(v: &Value) -> Option<[f64; 4]> {
    let Value::Tuple(parts) = v else { return None };
    let [r, g, b, a] = parts.as_slice() else { return None };
    Some([as_f64(r)?, as_f64(g)?, as_f64(b)?, as_f64(a)?])
}

/// Every background-surface colour override in the file, as `(state_id, rgba)`.
/// SPARSE: a state absent from this list uses EVE's built-in default colour.
pub fn state_colors(v: &Value) -> Vec<(i64, [f64; 4])> {
    let Value::Dict(root) = v else { return Vec::new() };
    let Some((_, ov)) = root.iter().find(|(k, _)| is_b(k, b"overview")) else { return Vec::new() };
    let Value::Dict(ovd) = ov else { return Vec::new() };
    let Some((_, sc)) = ovd.iter().find(|(k, _)| is_b(k, b"stateColors")) else { return Vec::new() };
    let inner = match sc {
        Value::Dict(d) => Some(d),
        Value::Tuple(items) => items.iter().find_map(|e| match e {
            Value::Dict(d) => Some(d),
            _ => None,
        }),
        _ => None,
    };
    let Some(d) = inner else { return Vec::new() };
    d.iter()
        .filter_map(|(k, val)| Some((background_color_id(k)?, as_rgba(val)?)))
        .collect()
}

/// Set or clear one state's background colour.
///
/// `Some(rgba)` writes an explicit override; `None` REMOVES the entry, which is
/// how the UI restores EVE's built-in default for that state — writing an
/// explicit default-looking colour is not the same thing.
///
/// Entries whose surface is not `background` are left exactly as found.
pub fn set_state_color(v: &mut Value, id: i64, rgba: Option<[f64; 4]>) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;

    if !ov.iter().any(|(k, _)| is_b(k, b"stateColors")) {
        if rgba.is_none() {
            return Ok(()); // nothing stored, nothing to clear
        }
        ov.push((
            Value::Bytes(b"stateColors".to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Dict(Vec::new())]),
        ));
    }
    let (_, sc) = ov.iter_mut().find(|(k, _)| is_b(k, b"stateColors")).expect("just checked");
    let Some(entries) = dict_inner_mut(sc) else { return Ok(()) };

    match rgba {
        None => entries.retain(|(k, _)| background_color_id(k) != Some(id)),
        Some([r, g, b_, a]) => {
            let val = Value::Tuple(vec![
                Value::Float(r), Value::Float(g), Value::Float(b_), Value::Float(a),
            ]);
            match entries.iter_mut().find(|(k, _)| background_color_id(k) == Some(id)) {
                Some((_, slot)) => *slot = val,
                None => entries.push((
                    Value::Tuple(vec![Value::Bytes(BACKGROUND_SURFACE.to_vec()), Value::Int(id)]),
                    val,
                )),
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p settings-model overview_states`
Expected: PASS — 14 tests.

- [ ] **Step 5: Export from the crate root**

In `crates/settings-model/src/lib.rs`, extend the `overview_states` re-export:

```rust
pub use overview_states::{set_state_color, set_state_list, state_colors, StateList};
```

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/src/overview_states.rs crates/settings-model/src/lib.rs
git commit -m "Author the sparse per-state overview colours"
```

---

### Task 4: Backend — the overview container's boolean settings

**Files:**
- Modify: `crates/settings-model/src/overview_states.rs`
- Modify: `crates/settings-model/src/lib.rs`

**Interfaces:**
- Consumes: Task 2's module and imports.
- Produces: `pub const OVERVIEW_BOOLS: [&str; 6]`,
  `pub fn overview_bools(v: &Value) -> Vec<(String, bool)>`,
  `pub fn set_overview_bool(v: &mut Value, key: &str, on: bool) -> Result<(), OverviewTabError>`.

An allow-list, not an enum-per-setting: the settings are homogeneous, and the
list keeps a typo'd key from minting junk into the container. The
`showCategoryInTargetRange_<id>` family is deliberately excluded — those are
keyed by inventory category and would need group naming to present.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `overview_states.rs`:

```rust
    /// user -> overview -> a few boolean settings as (ts, bool) tuples.
    fn user_with_bools() -> Value {
        let flag = |on: bool| Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Bool(on)]);
        Value::Dict(vec![(b("overview"), Value::Dict(vec![
            (b("applyToStructures"), flag(true)),
            (b("applyToOtherObjects"), flag(false)),
            (b("useSmallText"), flag(false)),
        ]))])
    }

    #[test]
    fn projects_the_boolean_settings_present_in_the_file() {
        let mut got = overview_bools(&user_with_bools());
        got.sort();
        assert_eq!(got, vec![
            ("applyToOtherObjects".to_string(), false),
            ("applyToStructures".to_string(), true),
            ("useSmallText".to_string(), false),
        ]);
    }

    #[test]
    fn sets_an_existing_boolean() {
        let mut v = user_with_bools();
        set_overview_bool(&mut v, "applyToOtherObjects", true).unwrap();
        assert!(overview_bools(&v).contains(&("applyToOtherObjects".to_string(), true)));
    }

    #[test]
    fn materialises_a_known_boolean_that_is_absent() {
        let mut v = user_with_bools();
        set_overview_bool(&mut v, "hideCorpTicker", true).unwrap();
        assert!(overview_bools(&v).contains(&("hideCorpTicker".to_string(), true)));
    }

    #[test]
    fn rejects_a_key_outside_the_allow_list() {
        let mut v = user_with_bools();
        assert!(set_overview_bool(&mut v, "applyToStructuresTypo", true).is_err());
        assert_eq!(overview_bools(&v).len(), 3, "nothing was minted");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p settings-model overview_states`
Expected: FAIL to compile — `overview_bools` and `set_overview_bool` not found.

- [ ] **Step 3: Write the implementation**

Add to `overview_states.rs`:

```rust
/// The overview container's simple boolean settings, as EVE's own Overview
/// Settings window exposes them. Deliberately excludes the
/// `showCategoryInTargetRange_<id>` family, which is keyed by inventory category
/// and needs group naming to present.
pub const OVERVIEW_BOOLS: [&str; 6] = [
    "applyToStructures",
    "applyToOtherObjects",
    "useSmallColorTags",
    "useSmallText",
    "overviewBroadcastsToTop",
    "hideCorpTicker",
];

fn as_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::Tuple(items) => items.iter().find_map(|e| match e {
            Value::Bool(b) => Some(*b),
            _ => None,
        }),
        _ => None,
    }
}

/// The known boolean settings actually present in the file. A setting absent
/// here is one EVE has never written; the UI shows it unticked.
pub fn overview_bools(v: &Value) -> Vec<(String, bool)> {
    let Value::Dict(root) = v else { return Vec::new() };
    let Some((_, ov)) = root.iter().find(|(k, _)| is_b(k, b"overview")) else { return Vec::new() };
    let Value::Dict(ovd) = ov else { return Vec::new() };
    OVERVIEW_BOOLS
        .iter()
        .filter_map(|name| {
            let (_, val) = ovd.iter().find(|(k, _)| is_b(k, name.as_bytes()))?;
            Some(((*name).to_string(), as_bool(val)?))
        })
        .collect()
}

/// Set one of the overview container's boolean settings. Preserves an existing
/// `(timestamp, bool)` wrapper and mints one when the key is absent.
///
/// `key` is validated against `OVERVIEW_BOOLS` so a typo cannot mint a junk key
/// into a file the client reads.
pub fn set_overview_bool(v: &mut Value, key: &str, on: bool) -> Result<(), OverviewTabError> {
    if !OVERVIEW_BOOLS.contains(&key) {
        return Err(OverviewTabError::NoOverview);
    }
    inline_all(v);
    let ov = overview_mut(v)?;

    match ov.iter_mut().find(|(k, _)| is_b(k, key.as_bytes())) {
        Some((_, existing)) => match existing {
            Value::Tuple(items) => match items.iter_mut().find(|e| matches!(e, Value::Bool(_))) {
                Some(slot) => *slot = Value::Bool(on),
                None => items.push(Value::Bool(on)),
            },
            other => *other = Value::Bool(on),
        },
        None => ov.push((
            Value::Bytes(key.as_bytes().to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Bool(on)]),
        )),
    }
    Ok(())
}
```

`OverviewTabError::NoOverview` is reused for the rejected-key case rather than
adding a variant: the command layer never sends an unknown key (the UI renders
from `OVERVIEW_BOOLS`), so this is a defensive guard, not a user-facing error.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p settings-model overview_states`
Expected: PASS — 18 tests.

- [ ] **Step 5: Export and run the full suite**

In `crates/settings-model/src/lib.rs`:

```rust
pub use overview_states::{
    overview_bools, set_overview_bool, set_state_color, set_state_list, state_colors,
    StateList, OVERVIEW_BOOLS,
};
```

Run: `cargo test -p settings-model`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/src/overview_states.rs crates/settings-model/src/lib.rs
git commit -m "Author the overview container boolean settings"
```

---

### Task 5: Backend — per-preset state exceptions

**Files:**
- Modify: `crates/settings-model/src/overview_presets.rs`
- Modify: `crates/settings-model/src/lib.rs`

**Interfaces:**
- Consumes: the module's existing `presets_mut`, `as_str`, `dict_inner_mut`,
  `is_b`, `overview_mut`, `inline_all`.
- Produces: `pub fn set_preset_states(v: &mut Value, name: &str, filtered: &[i64], always_shown: &[i64]) -> Result<(), OverviewTabError>`.

Both lists are written in one call so a tri-state move (hide → always-show) is
atomic rather than a remove plus an add.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `overview_presets.rs`:

```rust
    fn preset_states(v: &Value, name: &str, field: &str) -> Vec<i64> {
        let Value::Dict(root) = v else { return Vec::new() };
        let Some((_, ov)) = root.iter().find(|(k, _)| is_b(k, b"overview")) else { return Vec::new() };
        let Value::Dict(ovd) = ov else { return Vec::new() };
        let Some((_, p)) = ovd.iter().find(|(k, _)| is_b(k, b"overviewProfilePresets")) else { return Vec::new() };
        let inner = match p {
            Value::Tuple(items) => items.iter().find_map(|e| match e { Value::Dict(d) => Some(d), _ => None }),
            Value::Dict(d) => Some(d),
            _ => None,
        };
        let Some(pd) = inner else { return Vec::new() };
        let Some((_, blob)) = pd.iter().find(|(k, _)| as_str(k).as_deref() == Some(name)) else { return Vec::new() };
        let Value::Dict(fields) = blob else { return Vec::new() };
        let Some((_, list)) = fields.iter().find(|(k, _)| is_b(k, field.as_bytes())) else { return Vec::new() };
        let Value::List(l) = list else { return Vec::new() };
        l.iter().filter_map(|e| if let Value::Int(n) = e { Some(*n) } else { None }).collect()
    }

    #[test]
    fn writes_both_state_lists_sorted() {
        let mut v = user_with_presets();
        set_preset_states(&mut v, "alpha", &[52, 9, 13], &[11]).unwrap();
        assert_eq!(preset_states(&v, "alpha", "filteredStates"), vec![9, 13, 52]);
        assert_eq!(preset_states(&v, "alpha", "alwaysShownStates"), vec![11]);
    }

    #[test]
    fn leaves_groups_untouched() {
        let mut v = user_with_presets();
        set_preset_states(&mut v, "alpha", &[9], &[]).unwrap();
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, p) = ovd.iter().find(|(k, _)| is_b(k, b"overviewProfilePresets")).unwrap();
        let Value::Tuple(items) = p else { panic!() };
        let Some(Value::Dict(pd)) = items.iter().find(|e| matches!(e, Value::Dict(_))) else { panic!() };
        let (_, blob) = pd.iter().find(|(k, _)| as_str(k).as_deref() == Some("alpha")).unwrap();
        let Value::Dict(fields) = blob else { panic!() };
        let (_, g) = fields.iter().find(|(k, _)| is_b(k, b"groups")).unwrap();
        assert_eq!(*g, Value::List(vec![Value::Int(1)]), "groups must survive a state edit");
    }

    #[test]
    fn moving_a_state_between_lists_is_atomic() {
        let mut v = user_with_presets();
        set_preset_states(&mut v, "alpha", &[9, 13], &[]).unwrap();
        set_preset_states(&mut v, "alpha", &[13], &[9]).unwrap();
        assert_eq!(preset_states(&v, "alpha", "filteredStates"), vec![13]);
        assert_eq!(preset_states(&v, "alpha", "alwaysShownStates"), vec![9]);
    }

    #[test]
    fn empty_lists_clear_both_fields() {
        let mut v = user_with_presets();
        set_preset_states(&mut v, "alpha", &[9], &[13]).unwrap();
        set_preset_states(&mut v, "alpha", &[], &[]).unwrap();
        assert!(preset_states(&v, "alpha", "filteredStates").is_empty());
        assert!(preset_states(&v, "alpha", "alwaysShownStates").is_empty());
    }

    #[test]
    fn unknown_preset_is_an_error() {
        let mut v = user_with_presets();
        assert!(matches!(
            set_preset_states(&mut v, "nope", &[9], &[]),
            Err(OverviewTabError::UnknownPreset { .. })
        ));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p settings-model overview_presets`
Expected: FAIL to compile — `set_preset_states` not found.

- [ ] **Step 3: Write the implementation**

Add to `overview_presets.rs`, directly below `set_preset_groups`:

```rust
/// Replace the named preset's two state lists, both sorted ascending for a
/// deterministic on-disk order. `groups` is untouched.
///
/// Both lists are written in one call so that moving a state from "hide" to
/// "always show" is atomic — EVE's Exceptions tab models this as one three-way
/// choice per state, and the two lists are disjoint on real files.
pub fn set_preset_states(
    v: &mut Value, name: &str, filtered: &[i64], always_shown: &[i64],
) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    let presets = presets_mut(ov).ok_or(OverviewTabError::UnknownPreset { name: name.to_string() })?;
    let (_, blob) = presets
        .iter_mut()
        .find(|(k, _)| as_str(k).as_deref() == Some(name))
        .ok_or(OverviewTabError::UnknownPreset { name: name.to_string() })?;
    let fields = dict_inner_mut(blob).ok_or(OverviewTabError::UnknownPreset { name: name.to_string() })?;

    for (key, ids) in [
        (&b"filteredStates"[..], filtered),
        (&b"alwaysShownStates"[..], always_shown),
    ] {
        let mut sorted = ids.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        let list = Value::List(sorted.into_iter().map(Value::Int).collect());
        match fields.iter_mut().find(|(k, _)| is_b(k, key)) {
            Some((_, slot)) => *slot = list,
            None => fields.push((Value::Bytes(key.to_vec()), list)),
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p settings-model overview_presets`
Expected: PASS.

- [ ] **Step 5: Export and run the full suite**

In `crates/settings-model/src/lib.rs`, extend the `overview_presets` re-export
with `set_preset_states`.

Run: `cargo test -p settings-model`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/src/overview_presets.rs crates/settings-model/src/lib.rs
git commit -m "Author the per-preset overview state exceptions"
```

---

### Task 6: Backend — extend the read projection

**Files:**
- Modify: `crates/settings-model/src/overview.rs`
- Modify: `crates/settings-model/src/lib.rs`

**Interfaces:**
- Consumes: `overview_states::{overview_bools, state_colors}`.
- Produces: `OverviewColumns.appearance: Appearance`, `Preset.filtered_states`,
  `Preset.always_shown_states`, and the `Appearance` / `StateSurface` structs.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/settings-model/src/overview.rs`:

```rust
    #[test]
    fn projects_state_surfaces_from_the_file() {
        let user = user_with_state_keys();
        let out = project_overview(&user, None);
        assert_eq!(out.appearance.background.enabled, vec![9, 13]);
        assert_eq!(out.appearance.background.order, vec![13, 9, 68]);
        assert!(!out.appearance.defaulted, "keys were present");
    }

    #[test]
    fn flags_a_clean_account_as_defaulted() {
        let user = user_without_state_keys();
        let out = project_overview(&user, None);
        assert!(out.appearance.defaulted, "no state keys means the UI shows bundled defaults");
        assert!(out.appearance.background.order.is_empty());
    }

    #[test]
    fn projects_preset_state_lists() {
        let user = user_with_state_keys();
        let alpha = out_preset(&project_overview(&user, None), "alpha");
        assert_eq!(alpha.filtered_states, vec![9, 13]);
        assert_eq!(alpha.always_shown_states, vec![11]);
    }
```

Add the fixtures and helper alongside them:

```rust
    fn ob(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }

    fn ts_list(ids: &[i64]) -> Value {
        Value::Tuple(vec![
            Value::Long(vec![0u8; 8]),
            Value::List(ids.iter().map(|n| Value::Int(*n)).collect()),
        ])
    }

    fn alpha_blob() -> Value {
        let list = |ids: &[i64]| Value::List(ids.iter().map(|n| Value::Int(*n)).collect());
        Value::Dict(vec![
            (ob("groups"), list(&[1])),
            (ob("filteredStates"), list(&[9, 13])),
            (ob("alwaysShownStates"), list(&[11])),
        ])
    }

    fn overview_with(mut extra: Vec<(Value, Value)>) -> Value {
        let mut entries = vec![
            (ob("tabsettings_new"), Value::Dict(vec![(
                Value::Int(0),
                Value::Dict(vec![(ob("overview"), ob("alpha"))]),
            )])),
            (ob("overviewProfilePresets"), Value::Tuple(vec![
                Value::Long(vec![0u8; 8]),
                Value::Dict(vec![(ob("alpha"), alpha_blob())]),
            ])),
        ];
        entries.append(&mut extra);
        Value::Dict(vec![(ob("overview"), Value::Dict(entries))])
    }

    /// An account that HAS customised its overview states. The order lists carry
    /// id 68, which the client stores but never renders.
    fn user_with_state_keys() -> Value {
        overview_with(vec![
            (ob("backgroundStates2"), ts_list(&[9, 13])),
            (ob("backgroundOrder2"), ts_list(&[13, 9, 68])),
            (ob("flagStates2"), ts_list(&[9])),
            (ob("flagOrder2"), ts_list(&[9, 13, 68])),
        ])
    }

    /// A clean account: presets and tabs, but none of the four state keys.
    fn user_without_state_keys() -> Value {
        overview_with(Vec::new())
    }

    fn out_preset<'a>(out: &'a OverviewColumns, name: &str) -> &'a Preset {
        out.presets.iter().find(|p| p.name == name).expect("preset present")
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p settings-model overview::`
Expected: FAIL to compile — no `appearance` field on `OverviewColumns`.

- [ ] **Step 3: Extend the projection types**

In `crates/settings-model/src/overview.rs`:

```rust
#[derive(Debug, Serialize, PartialEq, Default)]
pub struct StateSurface {
    /// The ticked subset — states that actually tint a row / carry a tag.
    pub enabled: Vec<i64>,
    /// Every state the client knows, in priority order (first match wins).
    /// May contain an id the client never renders; callers MUST preserve it.
    pub order: Vec<i64>,
}

#[derive(Debug, Serialize, PartialEq, Default)]
pub struct Appearance {
    pub background: StateSurface,
    pub flag: StateSurface,
    /// Sparse background-surface colour overrides. A state absent here uses
    /// EVE's built-in default colour.
    pub colors: Vec<(i64, [f64; 4])>,
    pub bools: Vec<(String, bool)>,
    /// True when the file carried none of the four state keys — the account has
    /// never customised its overview states, so the UI shows bundled defaults
    /// and the first edit materialises the keys.
    pub defaulted: bool,
}
```

Add `pub appearance: Appearance` to `OverviewColumns`, and
`pub filtered_states: Vec<i64>` / `pub always_shown_states: Vec<i64>` to
`Preset`. Update the `empty` literal in `project_overview` to
`appearance: Appearance::default()`.

- [ ] **Step 4: Populate it in `project_overview`**

Add one reader beside `presets_with_groups`, built from the module's existing
`find_child` / `as_list_r` / `as_int` / `effective` helpers — `as_list_r`
already unwraps the `(timestamp, list)` wrapper, so no new unwrapping is needed:

```rust
/// One account-scoped state list. Empty when the key is absent, which is the
/// case on an account that has never customised its overview states.
fn state_ids(overview: &Entries, key: &[u8], sh: &SharedTable) -> Vec<i64> {
    find_child(overview, key, sh)
        .and_then(|v| as_list_r(v, sh))
        .map(|l| l.iter().filter_map(|e| as_int(effective(e, sh))).collect())
        .unwrap_or_default()
}

fn appearance(overview: &Entries, user: &Value, sh: &SharedTable) -> Appearance {
    const KEYS: [&[u8]; 4] = [
        b"backgroundStates2", b"backgroundOrder2", b"flagStates2", b"flagOrder2",
    ];
    Appearance {
        background: StateSurface {
            enabled: state_ids(overview, b"backgroundStates2", sh),
            order: state_ids(overview, b"backgroundOrder2", sh),
        },
        flag: StateSurface {
            enabled: state_ids(overview, b"flagStates2", sh),
            order: state_ids(overview, b"flagOrder2", sh),
        },
        colors: crate::overview_states::state_colors(user),
        bools: crate::overview_states::overview_bools(user),
        defaulted: KEYS.iter().all(|k| find_child(overview, k, sh).is_none()),
    }
}
```

Call it in `project_overview` and put the result in the returned
`OverviewColumns`.

For `Preset`, extend `presets_with_groups` to read the two state lists from the
same blob the `groups` read already has in hand. Lift the repeated
read-a-named-int-list-from-the-blob expression into a closure so the three reads
share one code path:

```rust
            let d = as_dict(v, sh);
            let ids = |name: &[u8]| {
                d.and_then(|d| find_child(d, name, sh))
                    .and_then(|g| as_list_r(g, sh))
                    .map(|l| l.iter().filter_map(|e| as_int(effective(e, sh))).collect())
                    .unwrap_or_default()
            };
            Some(Preset {
                name,
                groups: ids(b"groups"),
                filtered_states: ids(b"filteredStates"),
                always_shown_states: ids(b"alwaysShownStates"),
            })
```

Rename the function to `presets_with_states` and update its doc comment — the
existing one ends "the two state lists are not read here (slice 3)", which this
task makes false.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p settings-model`
Expected: PASS. Existing `overview` tests that construct `OverviewColumns`
literals need `appearance: Appearance::default()` and the two new `Preset`
fields added — update them as part of this step.

- [ ] **Step 6: Export and commit**

Add `Appearance` and `StateSurface` to the `overview` re-export in
`crates/settings-model/src/lib.rs`.

```bash
git add crates/settings-model/src/overview.rs crates/settings-model/src/lib.rs
git commit -m "Project the overview state surfaces and preset exceptions"
```

---

### Task 7: Commands and the TypeScript client

**Files:**
- Modify: `app/src-tauri/src/ops.rs`
- Modify: `app/src-tauri/src/lib.rs`
- Modify: `app/src/lib/api.ts`

**Interfaces:**
- Consumes: Tasks 2–6.
- Produces: `api.overviewSetStates`, `api.overviewSetStateColor`,
  `api.overviewSetBool`, `api.presetSetStates`, and the extended
  `OverviewColumns` / `Preset` / `Appearance` TypeScript types.

All four commands go through `edit_user_tabs`, not `edit_user_overview` — every
new backend function reaches the container via `overview_tabs::overview_mut` and
so returns `OverviewTabError`, exactly like `preset_set_groups`.

- [ ] **Step 1: Add the ops functions**

In `app/src-tauri/src/ops.rs`, beside `preset_set_groups`:

```rust
pub fn overview_set_states(state: &AppState, which: String, ids: Vec<i64>) -> Result<OverviewColumns, ErrDto> {
    let list = match which.as_str() {
        "background" => settings_model::StateList::Background,
        "backgroundOrder" => settings_model::StateList::BackgroundOrder,
        "flag" => settings_model::StateList::Flag,
        "flagOrder" => settings_model::StateList::FlagOrder,
        other => return Err(ErrDto::new("overview", format!("unknown state list {other}"))),
    };
    edit_user_tabs(state, |v| settings_model::set_state_list(v, list, &ids))
}

pub fn overview_set_state_color(state: &AppState, id: i64, rgba: Option<[f64; 4]>) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| settings_model::set_state_color(v, id, rgba))
}

pub fn overview_set_bool(state: &AppState, key: String, on: bool) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| settings_model::set_overview_bool(v, &key, on))
}

pub fn preset_set_states(
    state: &AppState, name: String, filtered: Vec<i64>, always_shown: Vec<i64>,
) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| settings_model::set_preset_states(v, &name, &filtered, &always_shown))
}
```

- [ ] **Step 2: Add the Tauri command wrappers**

In `app/src-tauri/src/lib.rs`, beside the `preset_set_groups` command:

```rust
#[tauri::command]
fn overview_set_states(state: tauri::State<'_, AppState>, which: String, ids: Vec<i64>) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::overview_set_states(&state, which, ids)
}

#[tauri::command]
fn overview_set_state_color(state: tauri::State<'_, AppState>, id: i64, rgba: Option<[f64; 4]>) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::overview_set_state_color(&state, id, rgba)
}

#[tauri::command]
fn overview_set_bool(state: tauri::State<'_, AppState>, key: String, on: bool) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::overview_set_bool(&state, key, on)
}

#[tauri::command]
fn preset_set_states(state: tauri::State<'_, AppState>, name: String, filtered: Vec<i64>, always_shown: Vec<i64>) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::preset_set_states(&state, name, filtered, always_shown)
}
```

The Rust parameter is `always_shown` (snake_case) while the TypeScript wrapper
sends `alwaysShown` — Tauri converts camelCase arguments to snake_case
parameters. Writing `alwaysShown` in the Rust signature would both trip the
`non_snake_case` lint and break the mapping.

Register all four in the `invoke_handler!` list at
`app/src-tauri/src/lib.rs:306`, on the line that already carries
`preset_set_groups, preset_fork`.

- [ ] **Step 3: Extend the TypeScript types and wrappers**

In `app/src/lib/api.ts`, extend `Preset` and `OverviewColumns` and add the
`Appearance` types:

```ts
export interface Preset {
  name: string;
  groups: number[];
  filtered_states: number[];
  always_shown_states: number[];
}

export interface StateSurface {
  enabled: number[];
  order: number[];
}

export interface Appearance {
  background: StateSurface;
  flag: StateSurface;
  colors: [number, [number, number, number, number]][];
  bools: [string, boolean][];
  defaulted: boolean;
}

export interface OverviewColumns {
  tabs: OverviewTab[];
  windows: OverviewWindow[];
  presets: Preset[];
  appearance: Appearance;
}
```

Add the invoke wrappers beside `presetSetGroups`:

```ts
  overviewSetStates: (which: "background" | "backgroundOrder" | "flag" | "flagOrder", ids: number[]) =>
    invoke<OverviewColumns>("overview_set_states", { which, ids }),
  overviewSetStateColor: (id: number, rgba: [number, number, number, number] | null) =>
    invoke<OverviewColumns>("overview_set_state_color", { id, rgba }),
  overviewSetBool: (key: string, on: boolean) =>
    invoke<OverviewColumns>("overview_set_bool", { key, on }),
  presetSetStates: (name: string, filtered: number[], alwaysShown: number[]) =>
    invoke<OverviewColumns>("preset_set_states", { name, filtered, alwaysShown }),
```

- [ ] **Step 4: Verify it builds and type-checks**

Run: `cargo build -p eve-settings-editor` (or `cargo check --workspace`)
Expected: builds clean.

Run (from `app/`): `npm run check`
Expected: no type errors.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Wire the overview state commands through to the client"
```

---

### Task 8: Frontend — split the Overview view into sub-tabs

**Pure move, no behaviour change.** The view is 518 lines carrying tabs,
windows, presets, the group checklist and the column editor, and the
small-tasks ledger flags its UI/UX as "rough — defer the polish to the later
overview-depth slices". Do the restructure on its own so Tasks 9 and 10 land as
readable diffs.

**Files:**
- Create: `app/src/lib/OverviewColumnsTab.svelte`
- Create: `app/src/lib/OverviewFiltersTab.svelte`
- Create: `app/src/lib/OverviewAppearanceTab.svelte`
- Modify: `app/src/lib/OverviewView.svelte`

**Interfaces:**
- Consumes: Task 7's `OverviewColumns` type.
- Produces: three components, each taking `{ data: OverviewColumns; tabIndex: number | null; onChanged: (next: OverviewColumns) => void }`
  plus whatever it already needs; `OverviewView.svelte` keeps the tab/window
  strip and hosts a `Columns | Filters | Appearance` selector.

Sub-tab names follow EVE's own Overview Settings window so the screen reads as
familiar rather than as a parallel vocabulary.

- [ ] **Step 1: Extract the column editor unchanged**

Move the column list markup, its `toggle` / `setWidth` / `drop` / `dragFrom`
logic and its styles into `OverviewColumnsTab.svelte`. Pass `data`, `tabIndex`
and `onChanged` as props. Change no behaviour.

- [ ] **Step 2: Extract the preset picker and group checklist unchanged**

Move the preset dropdown, duplicate / rename / delete actions, the group
catalog checklist and its `setPresetGroup` fork path into
`OverviewFiltersTab.svelte`. Render the group checklist under a **Types Shown**
heading — EVE's own name for it — leaving room for **Exceptions** in Task 9.

- [ ] **Step 3: Create the Appearance shell**

`OverviewAppearanceTab.svelte` renders only a heading for now; Task 10 fills it.

- [ ] **Step 4: Host the sub-tabs**

In `OverviewView.svelte`, keep the tab/window strip (create/rename/delete/
reorder/move, add/remove window) and add a sub-tab selector:

```svelte
<div class="subtabs" role="tablist">
  {#each ["Columns", "Filters", "Appearance"] as name}
    <button role="tab" aria-selected={sub === name} class:active={sub === name}
            onclick={() => (sub = name)}>{name}</button>
  {/each}
</div>
```

with `let sub = $state("Columns")`.

- [ ] **Step 5: Verify nothing regressed**

Run (from `app/`): `npm run check` then `npm test` then `npm run build`
Expected: all pass. Manually confirm the column editor, preset picker and group
checklist still behave exactly as before the split.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/OverviewView.svelte app/src/lib/OverviewColumnsTab.svelte app/src/lib/OverviewFiltersTab.svelte app/src/lib/OverviewAppearanceTab.svelte
git commit -m "Split the Overview view into Columns, Filters and Appearance sub-tabs"
```

---

### Task 9: Frontend — the Exceptions editor

**Files:**
- Modify: `app/src/lib/states.ts`, `app/src/lib/states.test.ts`
- Modify: `app/src/lib/OverviewFiltersTab.svelte`

**Interfaces:**
- Consumes: Task 1's `stateLabel` / `EXCEPTION_STATES`, Task 7's
  `api.presetSetStates`, Task 8's `OverviewFiltersTab.svelte`.
- Produces: `type Exception = "show" | "hide" | "always"`,
  `exceptionOf(filtered, alwaysShown, id): Exception`,
  `applyException(filtered, alwaysShown, id, choice): { filtered: number[]; alwaysShown: number[] }`.

- [ ] **Step 1: Write the failing tests**

Add to `app/src/lib/states.test.ts`:

```ts
import { exceptionOf, applyException } from "./states";

test("a state in neither list shows normally", () => {
  assert.equal(exceptionOf([9], [11], 13), "show");
});

test("a state in filteredStates is hidden", () => {
  assert.equal(exceptionOf([9], [11], 9), "hide");
});

test("a state in alwaysShownStates is always shown", () => {
  assert.equal(exceptionOf([9], [11], 11), "always");
});

test("choosing hide moves a state out of alwaysShown", () => {
  const next = applyException([], [11], 11, "hide");
  assert.deepEqual(next.filtered, [11]);
  assert.deepEqual(next.alwaysShown, []);
});

test("choosing always moves a state out of filtered", () => {
  const next = applyException([9], [], 9, "always");
  assert.deepEqual(next.filtered, []);
  assert.deepEqual(next.alwaysShown, [9]);
});

test("choosing show removes a state from both lists", () => {
  const next = applyException([9], [], 9, "show");
  assert.deepEqual(next.filtered, []);
  assert.deepEqual(next.alwaysShown, []);
});

test("applying a choice leaves other states alone", () => {
  const next = applyException([9, 13], [11], 13, "show");
  assert.deepEqual(next.filtered, [9]);
  assert.deepEqual(next.alwaysShown, [11]);
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run (from `app/`): `npm test`
Expected: FAIL — `exceptionOf` is not exported.

- [ ] **Step 3: Write the implementation**

Add to `app/src/lib/states.ts`:

```ts
/** EVE's Exceptions tab offers exactly three choices per state. The two stored
 *  lists are disjoint on real files, and this tri-state is what keeps them so. */
export type Exception = "show" | "hide" | "always";

export function exceptionOf(filtered: number[], alwaysShown: number[], id: number): Exception {
  if (filtered.includes(id)) return "hide";
  if (alwaysShown.includes(id)) return "always";
  return "show";
}

export function applyException(
  filtered: number[], alwaysShown: number[], id: number, choice: Exception,
): { filtered: number[]; alwaysShown: number[] } {
  const f = filtered.filter((n) => n !== id);
  const a = alwaysShown.filter((n) => n !== id);
  if (choice === "hide") f.push(id);
  if (choice === "always") a.push(id);
  return { filtered: f, alwaysShown: a };
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run (from `app/`): `npm test`
Expected: PASS.

- [ ] **Step 5: Build the Exceptions UI**

In `OverviewFiltersTab.svelte`, below Types Shown, add an **Exceptions**
section: one row per id in `EXCEPTION_STATES`, **sorted alphabetically by
label** (EVE's own order — the Appearance lists are priority-ordered, this one
is not), each row a three-way radio group bound to
`exceptionOf(preset.filtered_states, preset.always_shown_states, id)`.

On change, compute the next pair with `applyException` and call
`api.presetSetStates(name, next.filtered, next.alwaysShown)`. Editing a
**built-in default preset** must auto-fork first, reusing the exact path
`setPresetGroup` already takes (`forkName` + `api.presetFork`, which already
accepts `filteredStates` and `alwaysShownStates`) so built-ins stay read-only.

Render any id present in the preset's lists but absent from `EXCEPTION_STATES`
as a raw `#<id>` row so it round-trips. Give the radios explicit dark
background and colour per the WebView2 gotcha.

- [ ] **Step 6: Verify**

Run (from `app/`): `npm run check` then `npm test` then `npm run build`
Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add app/src/lib/states.ts app/src/lib/states.test.ts app/src/lib/OverviewFiltersTab.svelte
git commit -m "Add the per-preset overview Exceptions editor"
```

---

### Task 10: Frontend — the Appearance editor

**Files:**
- Modify: `app/src/lib/states.ts`, `app/src/lib/states.test.ts`
- Modify: `app/src/lib/OverviewAppearanceTab.svelte`

**Interfaces:**
- Consumes: Task 1's default arrays and `stateLabel`; Task 7's
  `api.overviewSetStates` / `overviewSetStateColor` / `overviewSetBool`.
- Produces: `rgbaToHex(rgba): string`, `hexToRgba(hex, alpha): [number, number, number, number]`,
  `moveInOrder(order, from, to): number[]`.

- [ ] **Step 1: Write the failing tests**

Add to `app/src/lib/states.test.ts`:

```ts
import { rgbaToHex, hexToRgba, moveInOrder } from "./states";

test("rgbaToHex converts EVE's 0..1 floats to a hex colour", () => {
  assert.equal(rgbaToHex([1, 0.35, 0, 1]), "#ff5900");
});

test("hexToRgba round-trips through rgbaToHex", () => {
  assert.equal(rgbaToHex(hexToRgba("#ff5900", 1)), "#ff5900");
});

test("hexToRgba preserves the alpha it is given", () => {
  assert.deepEqual(hexToRgba("#000000", 0.5)[3], 0.5);
});

test("moveInOrder reorders without dropping any id", () => {
  const next = moveInOrder([13, 44, 9, 68], 0, 2);
  assert.deepEqual(next, [44, 9, 13, 68]);
  assert.equal(next.length, 4);
});

test("moveInOrder keeps an unrendered id in place", () => {
  assert.ok(moveInOrder([13, 44, 68], 0, 1).includes(68));
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run (from `app/`): `npm test`
Expected: FAIL — `rgbaToHex` is not exported.

- [ ] **Step 3: Write the implementation**

Add to `app/src/lib/states.ts`:

```ts
const clamp = (n: number) => Math.max(0, Math.min(255, Math.round(n * 255)));

/** EVE stores colours as 0..1 floats; <input type="color"> speaks #rrggbb. */
export function rgbaToHex(rgba: [number, number, number, number]): string {
  const [r, g, b] = rgba;
  return "#" + [r, g, b].map((c) => clamp(c).toString(16).padStart(2, "0")).join("");
}

/** Alpha is not exposed in the UI — every observed entry is 1.0 — so the
 *  caller passes the stored alpha through unchanged rather than resetting it. */
export function hexToRgba(hex: string, alpha: number): [number, number, number, number] {
  const n = parseInt(hex.slice(1), 16);
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255, alpha];
}

/** Move one entry of a priority order. Length is invariant, so an id the client
 *  never renders (68) rides along instead of being dropped. */
export function moveInOrder(order: number[], from: number, to: number): number[] {
  const next = [...order];
  const [moved] = next.splice(from, 1);
  next.splice(to, 0, moved);
  return next;
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run (from `app/`): `npm test`
Expected: PASS.

- [ ] **Step 5: Build the Appearance UI**

In `OverviewAppearanceTab.svelte`, mirroring EVE's own Appearance tab layout —
the booleans **above** a Colortag/Background sub-tab pair:

1. **Boolean settings**, using EVE's own labels:

   | Key | Label |
   |---|---|
   | `useSmallColorTags` | Use small colortags |
   | `useSmallText` | Use small font |
   | `applyToStructures` | Also apply to structures |
   | `applyToOtherObjects` | Also apply to other objects in space |
   | `overviewBroadcastsToTop` | Show fleet broadcasts at the top |
   | `hideCorpTicker` | Hide corporation ticker |

   Group the two `applyTo*` rows under EVE's own note, "The Colortag and
   Background settings apply to ships and drones by default". A key absent from
   `appearance.bools` renders unticked. Each change calls
   `api.overviewSetBool(key, on)`.

2. **Background / Colortag sub-tabs.** Each renders `surface.order` in order —
   **never a re-sorted or filtered copy**, so id 68 keeps its slot. Per row: a
   checkbox bound to `surface.enabled.includes(id)`, the label from
   `stateLabel(id)` (falling back to `#<id>`), and a drag handle reusing the
   existing tab-strip drag pattern.
   - Toggling calls `api.overviewSetStates("background" | "flag", nextEnabled)`.
   - Dragging calls `api.overviewSetStates("backgroundOrder" | "flagOrder", moveInOrder(...))`.
   - **Background rows only:** an `<input type="color">` showing the stored
     colour, or EVE's default when the state has no entry in
     `appearance.colors`. A row with no stored colour is visually marked unset
     so "unset" and "explicitly set" stay distinguishable. Changing it calls
     `api.overviewSetStateColor(id, hexToRgba(hex, storedAlpha ?? 1))`; a
     **Reset** action calls `api.overviewSetStateColor(id, null)`.

3. When `appearance.defaulted` is true, drive both lists from
   `DEFAULT_BACKGROUND_ORDER` / `DEFAULT_BACKGROUND_STATES` /
   `DEFAULT_FLAG_ORDER` / `DEFAULT_FLAG_STATES` and show a note that these are
   EVE's defaults, not yet saved. The first edit materialises the keys.

The whole tab writes account-wide, so render it behind the existing
shared-account banner (`sharedLabel`, already a prop on `OverviewView.svelte`).
Style the colour input, checkboxes and radios explicitly per the WebView2
gotcha.

- [ ] **Step 6: Verify**

Run (from `app/`): `npm run check` then `npm test` then `npm run build`
Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add app/src/lib/states.ts app/src/lib/states.test.ts app/src/lib/OverviewAppearanceTab.svelte
git commit -m "Add the overview Appearance editor"
```

---

### Task 11: Live smoke and format notes

**Files:**
- Modify: `docs/format-notes.md`

- [ ] **Step 1: Run the full gate**

```bash
cargo test --workspace
```
Then from `app/`: `npm test`, `npm run check`, `npm run build`.
Expected: all green.

- [ ] **Step 2: Live smoke in-game**

Back up the live settings first (the app's own backup panel counts). Then, on a
real account:

1. Toggle a background state off, reorder two states, set one state's colour and
   Reset another.
2. Toggle a colortag state and reorder one.
3. Flip two boolean settings.
4. On one preset, set a state to Hide and another to Always show.
5. On a **built-in default** preset, set any exception — confirm it forks to an
   editable copy and the built-in is unchanged.
6. Save, launch EVE, and confirm: the client opens the account without
   complaint, Overview Settings → Appearance shows the new toggles / order /
   colours, and Filters → Exceptions shows the new choices.
7. **Confirm id 68 survived**: dump the saved file and check the order arrays
   still contain it.
8. Repeat step 1 on an account that had **no** state keys (§2.4) and confirm EVE
   accepts the materialised container.

Record any fix the smoke surfaces as its own commit.

- [ ] **Step 3: Record the model in the format notes**

Add a section to `docs/format-notes.md` covering: the four account-scoped state
keys and the enabled/order independence; `stateColors` as a sparse
`(surface, id) → rgba` map where absent means EVE's default and `background` is
the only observed surface; the three differing vocabularies (22 rendered / 23
stored / 24 in Exceptions) and id 68; and that the unsuffixed `backgroundOrder`
/ `backgroundStates` exist only inside `restoreData` so there is no legacy
migration. No character ids, account ids or names.

- [ ] **Step 4: Commit**

```bash
git add docs/format-notes.md
git commit -m "Document the overview state model in the format notes"
```

---

## Deferred

Recorded here rather than built, per the spec's non-goals:

- **Colortag colours / tag graphics** — no corpus evidence of a `stateColors`
  surface other than `background`.
- **`showCategoryInTargetRange_<id>`** — category-keyed, needs group naming.
- **Alpha editing** — preserved on write, not exposed.
- **Import/export of state settings** — slice 4.

Add any ship-as-debt minors from the whole-branch review to
`docs/small-tasks.md` before the release, per the ledger convention. Note that
slice 2b's review minors were never written up — worth closing that gap in the
same pass.
