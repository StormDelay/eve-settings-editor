# Canvas detail layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Behind one `Detail` toggle, draw each canvas rectangle's internals — capacitor ring and module racks in the ship HUD, ability grid in the fighter panel, the real neocom buttons, the real overview columns at their stored widths, and the chat member-list / input split.

**Architecture:** One new read-only Rust projection (`chat.rs`) for the two chat keys nothing reads today; one new pure TypeScript module (`detail.ts`) that turns projections and the measured HUD table into `DetailPart[]` in data px relative to the parent rect; one new component (`DetailParts.svelte`) that renders those parts as `pointer-events: none` children of the rectangles `LayoutView` already draws. Decoration only — nothing here reaches `hudRects`, the snap lines, or any drag.

**Tech Stack:** Rust (`settings-model` crate, `serde`), Tauri commands, TypeScript, Svelte 5 (runes: `$state`, `$derived`, `$props`), `cargo test` for Rust, `node --test` for `*.test.ts`, vitest + jsdom for `*.spec.ts`.

Design spec: `docs/superpowers/specs/2026-07-30-canvas-detail-layer-design.md`.

## Global Constraints

- **Decoration only.** No detail part may join `snapLines`, be returned by `unitAt` / `rectsAt`, start a drag, or be hit-tested. The single mechanism that guarantees this is `pointer-events: none` on the `DetailParts` container — do not remove it, and do not add `pointer-events: auto` to any part.
- **`detail.ts` stays pure** — no DOM, no Svelte, no Tauri. It may import types from `api.ts` and `layout.ts`, and the `toCanvas` helper stays in `layout.ts` (the component imports it from there, not from `detail.ts`).
- **Do not modify `layout.ts`'s existing functions.** `hudRects`, `snapLines`, `linesFor` and the `Drag` union are out of bounds for this work.
- **Measured vs invented.** Every constant that came from `docs/format-notes.md` §"HUD anchors" carries a comment citing it. Every invented constant lives in the single `DETAIL_NOMINAL` table with a `ponytail:` comment. Never mix the two.
- **Rust read paths thread `SharedTable`/`effective` end to end** — section key AND entry keys. A read path that matches bare `Value::Bytes` projects nothing from real account files (`format-notes.md`: "The account file's `ui` section key is `Ref`-keyed"). This has shipped as a bug before.
- **No personal data in fixtures.** Rust unit tests build synthetic trees with invented channel names.
- Run Rust tests with `cargo test -p settings-model` from the repo root. Run frontend tests with `npm test` from `app/`. Type-check with `npm run check` from `app/`.
- All work lands on branch `worktree-canvas-detail-layer`.

---

### Task 1: `chat.rs` — project the chat window splits

**Files:**
- Create: `crates/settings-model/src/chat.rs`
- Modify: `crates/settings-model/src/lib.rs` (add `pub mod chat;` beside `pub mod hud;` on line 15, and a `pub use` beside line 35)
- Create: `crates/settings-model/tests/chat_panels_corpus.rs`

**Interfaces:**
- Consumes: `crate::treewalk::{collect_shared, effective, section, text, Entries, SharedTable}` — all `pub(crate)`, all already exist. `section(root, name, shared) -> Option<(&Entries, NodePath)>`; `text(v, shared) -> Option<String>`.
- Produces:
  - `pub struct ChatPanel { pub window_id: String, pub userlist_width: Option<i64>, pub input_height: Option<i64> }`
  - `pub fn project_chat(user_root: &Value) -> Vec<ChatPanel>`
  - Re-exported from the crate root as `settings_model::{project_chat, ChatPanel}`.

- [ ] **Step 1: Write the failing tests**

Create `crates/settings-model/src/chat.rs` containing ONLY the test module for now (the implementation lands in Step 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    fn ts() -> Value {
        Value::Long(vec![0u8; 8])
    }

    fn b(s: &str) -> Value {
        Value::Bytes(s.as_bytes().to_vec())
    }

    /// (timestamp, value) — the file-wide value-wrapper convention.
    fn wrapped(v: Value) -> Value {
        Value::Tuple(vec![ts(), v])
    }

    /// An account document whose root `windows` section holds `entries`.
    fn user_doc(entries: Vec<(Value, Value)>) -> Value {
        Value::Dict(vec![(b("windows"), Value::Dict(entries))])
    }

    fn panel<'a>(panels: &'a [ChatPanel], id: &str) -> &'a ChatPanel {
        panels.iter().find(|p| p.window_id == id).expect("panel present")
    }

    #[test]
    fn projects_both_keys_for_one_channel() {
        let doc = user_doc(vec![
            (b("chatchannel_local_userlistwidth"), wrapped(Value::Int(135))),
            (b("chatinputsize_chatchannel_local"), wrapped(Value::Int(64))),
        ]);
        let panels = project_chat(&doc);
        assert_eq!(panels.len(), 1);
        let p = panel(&panels, "chatchannel_local");
        assert_eq!(p.userlist_width, Some(135));
        assert_eq!(p.input_height, Some(64));
    }

    #[test]
    fn a_channel_with_only_one_key_projects_the_other_as_none() {
        let doc = user_doc(vec![
            (b("chatchannel_fleet_userlistwidth"), wrapped(Value::Int(107))),
            (b("chatinputsize_chatchannel_corp"), wrapped(Value::Int(63))),
        ]);
        let panels = project_chat(&doc);
        assert_eq!(panels.len(), 2);
        assert_eq!(panel(&panels, "chatchannel_fleet").userlist_width, Some(107));
        assert_eq!(panel(&panels, "chatchannel_fleet").input_height, None);
        assert_eq!(panel(&panels, "chatchannel_corp").userlist_width, None);
        assert_eq!(panel(&panels, "chatchannel_corp").input_height, Some(63));
    }

    /// The states-slice regression, and the one that would make this project
    /// nothing from every real account file: real files `Shared`/`Ref` their
    /// repeated keys, and the account file's section key is itself `Ref`-keyed.
    #[test]
    fn resolves_a_shared_section_key_and_a_ref_entry_key() {
        let doc = Value::Dict(vec![(
            Value::Shared { slot: 1, value: Box::new(b("windows")) },
            Value::Dict(vec![
                (Value::Shared { slot: 2, value: Box::new(b("chatchannel_local_userlistwidth")) },
                 wrapped(Value::Int(135))),
                (Value::Ref(2), wrapped(Value::Int(999))),
            ]),
        )]);
        let panels = project_chat(&doc);
        // Both keys resolve to the same id; the FIRST wins, matching how every
        // other read path here uses `.find()`.
        assert_eq!(panels.len(), 1);
        assert_eq!(panel(&panels, "chatchannel_local").userlist_width, Some(135));
    }

    #[test]
    fn a_malformed_value_is_skipped_not_panicked_on() {
        let doc = user_doc(vec![
            // Not an Int.
            (b("chatchannel_local_userlistwidth"), wrapped(b("wide"))),
            // Wrapper of the wrong arity.
            (b("chatchannel_corp_userlistwidth"), Value::Tuple(vec![ts()])),
            // Readable, so the projection is not simply empty.
            (b("chatchannel_fleet_userlistwidth"), wrapped(Value::Int(107))),
        ]);
        let panels = project_chat(&doc);
        assert_eq!(panels.len(), 1);
        assert_eq!(panel(&panels, "chatchannel_fleet").userlist_width, Some(107));
    }

    /// Only chat windows. `_userlistwidth` on anything else is not a chat
    /// window id and would produce a panel the canvas can never match.
    #[test]
    fn ignores_keys_that_are_not_chat_windows() {
        let doc = user_doc(vec![
            (b("neocomWidth"), wrapped(Value::Int(37))),
            (b("someotherwindow_userlistwidth"), wrapped(Value::Int(90))),
        ]);
        assert!(project_chat(&doc).is_empty());
    }

    #[test]
    fn a_document_with_no_windows_section_projects_nothing() {
        let doc = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
        assert!(project_chat(&doc).is_empty());
    }

    #[test]
    fn panels_come_back_sorted_by_window_id() {
        let doc = user_doc(vec![
            (b("chatchannel_local_userlistwidth"), wrapped(Value::Int(135))),
            (b("chatchannel_alliance_userlistwidth"), wrapped(Value::Int(80))),
            (b("chatchannel_fleet_userlistwidth"), wrapped(Value::Int(107))),
        ]);
        let ids: Vec<&str> = project_chat(&doc).iter().map(|p| p.window_id.as_str()).collect();
        assert_eq!(ids, ["chatchannel_alliance", "chatchannel_fleet", "chatchannel_local"]);
    }
}
```

Add `pub mod chat;` to `crates/settings-model/src/lib.rs` immediately after `pub mod hud;` (line 15).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p settings-model chat::`
Expected: FAIL to compile — `cannot find function 'project_chat' in this scope`, `cannot find type 'ChatPanel' in this scope`.

- [ ] **Step 3: Write the implementation**

Prepend to `crates/settings-model/src/chat.rs`, above the `#[cfg(test)]` module:

```rust
//! Read-only projection of the per-channel chat window splits: the member-list
//! width and the input-box height. Both live in the ACCOUNT document, under the
//! root `windows` section — the same section as `neocomWidth`, not `ui`. See
//! docs/format-notes.md, "Chat window splits".
//!
//! Nothing here mutates. The canvas detail layer draws these; no editor writes
//! them (design spec §6).

use std::collections::BTreeMap;

use blue_marshal::Value;
use serde::Serialize;

use crate::treewalk::{collect_shared, effective, section, text, SharedTable};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChatPanel {
    /// The canvas window id, e.g. `chatchannel_local` — taken verbatim out of
    /// the key name, so there is no mapping table to get wrong.
    pub window_id: String,
    /// `None` when the player has never resized this channel's member list.
    /// The canvas then draws no split rather than inventing one.
    pub userlist_width: Option<i64>,
    pub input_height: Option<i64>,
}

/// Key shapes. Both carry the window id verbatim: `chatchannel_local` owns
/// `chatchannel_local_userlistwidth` and `chatinputsize_chatchannel_local`.
const WIDTH_SUFFIX: &str = "_userlistwidth";
const INPUT_PREFIX: &str = "chatinputsize_";
/// Only chat windows. A `_userlistwidth` key on some other window would produce
/// a panel no canvas rectangle can ever match.
const CHAT_PREFIX: &str = "chatchannel_";

/// Project every chat window that has at least one of the two keys.
///
/// `BTreeMap` rather than a `Vec` scan: the two keys for one channel are not
/// adjacent in the section, and the ordering it gives for free is what makes
/// the output deterministic regardless of dict order.
pub fn project_chat(user_root: &Value) -> Vec<ChatPanel> {
    let mut shared = SharedTable::new();
    collect_shared(user_root, &mut shared);
    let Some((entries, _)) = section(user_root, b"windows", &shared) else {
        return Vec::new();
    };
    let mut by_id: BTreeMap<String, ChatPanel> = BTreeMap::new();
    for (k, v) in entries {
        // Keys are resolved through Ref/Shared: real files dedup repeated
        // strings, so a bare Bytes match reads nothing from them.
        let Some(name) = text(k, &shared) else { continue };
        let (id, is_width) = match name.strip_suffix(WIDTH_SUFFIX) {
            Some(id) => (id, true),
            None => match name.strip_prefix(INPUT_PREFIX) {
                Some(id) => (id, false),
                None => continue,
            },
        };
        if !id.starts_with(CHAT_PREFIX) {
            continue;
        }
        let Some(n) = leaf_int(v, &shared) else { continue };
        let e = by_id.entry(id.to_string()).or_insert_with(|| ChatPanel {
            window_id: id.to_string(),
            userlist_width: None,
            input_height: None,
        });
        // First wins, matching every other read path here (`.find()` semantics):
        // a duplicate key must not silently override the entry reads land on.
        let field = if is_width { &mut e.userlist_width } else { &mut e.input_height };
        if field.is_none() {
            *field = Some(n);
        }
    }
    by_id.into_values().collect()
}

/// The `Int` inside a `(timestamp, value)` leaf. A bare value is tolerated the
/// way `hud.rs::leaf` tolerates one; anything else reads as absent rather than
/// panicking, mirroring `windows.rs`'s malformed-tuple skip.
fn leaf_int(v: &Value, shared: &SharedTable) -> Option<i64> {
    let v = match effective(v, shared) {
        Value::Tuple(items) if items.len() == 2 => effective(&items[1], shared),
        other => other,
    };
    match v {
        Value::Int(i) => Some(*i),
        _ => None,
    }
}
```

Add the re-export to `crates/settings-model/src/lib.rs`, immediately after the `pub use hud::{...}` line (line 35):

```rust
pub use chat::{project_chat, ChatPanel};
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p settings-model chat::`
Expected: PASS, 7 tests.

- [ ] **Step 5: Write the corpus guard**

The unit tests above all build their own fixture, so a wrong section name (`ui` instead of `windows`) passes every one of them while reading nothing from a real file — exactly the class of bug that shipped in v0.15.0 for `badge_*`. Create `crates/settings-model/tests/chat_panels_corpus.rs`:

```rust
//! Real-data guard for the chat-split SECTION. Every unit test in `chat.rs`
//! builds its own fixture, so naming the wrong section passes them all while
//! projecting nothing from a real account file — the v0.15.0 `badge_*` class of
//! bug. Only real files catch it.
//!
//! Skips silently when the corpus is not checked out.

mod common;

use settings_model::project_chat;

/// Enough sightings to mean "the section is right", not "one odd file". The
/// 2026-07-28 snapshot had 23 of 58 account files carrying a member-list width.
const ENOUGH: usize = 5;

#[test]
fn chat_splits_read_from_real_account_files() {
    if !common::real_corpus_present() {
        return;
    }
    let mut with_width = 0usize;
    let mut with_input = 0usize;
    for f in common::user_files() {
        if with_width >= ENOUGH && with_input >= ENOUGH {
            break;
        }
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        for p in project_chat(&doc) {
            assert!(
                p.window_id.starts_with("chatchannel_"),
                "projected a non-chat window id: {}",
                p.window_id,
            );
            if p.userlist_width.is_some() {
                with_width += 1;
            }
            if p.input_height.is_some() {
                with_input += 1;
            }
        }
    }
    assert!(with_width >= ENOUGH, "only {with_width} member-list widths read from the real corpus");
    assert!(with_input >= ENOUGH, "only {with_input} input heights read from the real corpus");
}
```

- [ ] **Step 6: Run the corpus guard**

Run: `cargo test -p settings-model --test chat_panels_corpus`
Expected: PASS. If the real corpus is not checked out it returns early and still passes — confirm which happened by running with `-- --nocapture` and checking it did not silently skip on a machine that HAS the corpus (`testdata/corpus/` non-empty).

- [ ] **Step 7: Document the keys in `format-notes.md`**

Add a new `### Chat window splits` section immediately after the `### HUD anchors` section (which ends around line 913, before the next `###`):

```markdown
### Chat window splits

Corpus-verified 2026-07-30 from the `2026-07-28T170701Z_c-after` snapshot (58
account files). Both keys live in the ACCOUNT file under the **root `windows`
section** — the same section as `neocomWidth`, not `ui` — as ordinary
`(timestamp, value)` leaves.

| what | key | value | present |
|---|---|---|---|
| Member-list width | `chatchannel_<ch>_userlistwidth` | Int — 107, 135, 126, 104, 50 | 23/58 |
| Input-box height | `chatinputsize_chatchannel_<ch>` | Int — 64, 63 | 33/58 |

**Both key names carry the canvas window id verbatim.** `chatchannel_local` owns
`chatchannel_local_userlistwidth` and `chatinputsize_chatchannel_local`, so
`chat.rs` strips the suffix or the prefix and what remains is already an id in
`windowSizesAndPositions_1`. No mapping table.

`chatCondensedUserList_<ch>` (Bool) is deliberately not read: it changes how the
member list renders, not how wide it is, and its key naming is inconsistent —
`chatCondensedUserList_corp` sits beside
`chatCondensedUserList_chatchannel_player_-78564080`, one with the window-id
prefix and one without.

Whether the input box spans the full window width or only the message pane is
NOT captured — the editor draws it under the message pane only, pending a live
smoke.
```

- [ ] **Step 8: Commit**

```bash
git add crates/settings-model/src/chat.rs crates/settings-model/src/lib.rs \
        crates/settings-model/tests/chat_panels_corpus.rs docs/format-notes.md
git commit -m "Project the chat window member-list and input splits"
```

---

### Task 2: Expose `chat_panels` to the frontend

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (add after `hud_layout`, ~line 805)
- Modify: `app/src-tauri/src/lib.rs` (command fn beside `neocom_bar` ~line 313; registration in the `invoke_handler` list ~line 484)
- Modify: `app/src/lib/api.ts` (type beside `NeocomBar` ~line 192; binding beside `neocomBar` ~line 445)

**Interfaces:**
- Consumes: `settings_model::{project_chat, ChatPanel}` from Task 1. `AppState`, `ErrDto` — already in `ops.rs`.
- Produces:
  - Rust: `pub fn chat_panels(state: &AppState) -> Result<Vec<ChatPanel>, ErrDto>`
  - TS: `export interface ChatPanel { window_id: string; userlist_width: number | null; input_height: number | null }`
  - TS: `api.chatPanels(): Promise<ChatPanel[]>`

- [ ] **Step 1: Write the failing test**

Append to the `mod tests` block at the bottom of `app/src-tauri/src/ops.rs`, beside the existing neocom test around line 2859 (`assert_eq!(neocom_bar(&state).unwrap_err().code, "no_document");`).

`AppState` has no `Default` impl — it is constructed with `AppState::new()`:

```rust
/// An unpaired character is normal, so this is an empty list, not an error —
/// unlike `neocom_bar`, which needs the character document and refuses without
/// one.
#[test]
fn chat_panels_is_empty_without_an_account_file() {
    let state = AppState::new();
    assert!(chat_panels(&state).unwrap().is_empty());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p eve-settings-editor chat_panels_is_empty`
Expected: FAIL to compile — `cannot find function 'chat_panels' in this scope`.

(If the tauri crate's package name differs, get it from `app/src-tauri/Cargo.toml` and use that; `cargo test --workspace chat_panels_is_empty` also works.)

- [ ] **Step 3: Write the op**

Add to `app/src-tauri/src/ops.rs` immediately after `hud_layout` (which ends ~line 805):

```rust
/// Project the account document's chat window splits. An unpaired character is
/// normal, so no account file open means an empty list, NOT an error — the
/// canvas treats these as a bonus layer (design spec §4.5).
pub fn chat_panels(state: &AppState) -> Result<Vec<ChatPanel>, ErrDto> {
    let guard = state.user.lock().unwrap();
    Ok(guard.as_ref().map(|d| settings_model::project_chat(&d.value)).unwrap_or_default())
}
```

Add `ChatPanel` to the `use settings_model::{...}` list at the top of `ops.rs` (the same list that already imports `Hud`, `NeocomBar`, `WindowLayout`).

Note it takes only the `user` lock, so it does not participate in the user-before-char ordering the file documents for two-slot commands.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p eve-settings-editor chat_panels_is_empty`
Expected: PASS.

- [ ] **Step 5: Wire the Tauri command**

Add to `app/src-tauri/src/lib.rs`, immediately after the `neocom_bar` command fn (~line 316):

```rust
#[tauri::command]
fn chat_panels(state: tauri::State<'_, AppState>) -> Result<Vec<settings_model::ChatPanel>, ErrDto> {
    ops::chat_panels(&state)
}
```

Add `chat_panels,` to the `invoke_handler![...]` list — put it on the line that already reads `neocom_bar, neocom_reorder, neocom_remove, neocom_add, neocom_reset,` (~line 484).

- [ ] **Step 6: Add the frontend binding**

In `app/src/lib/api.ts`, add after the `NeocomBar` interface (~line 192):

```ts
/** Per-channel chat window splits, from the ACCOUNT document. Read-only — the
 * canvas detail layer draws these; nothing writes them. */
export interface ChatPanel {
  window_id: string;
  /** null = the player has never resized this channel's member list. */
  userlist_width: number | null;
  input_height: number | null;
}
```

And add to the `api` object beside `neocomBar` (~line 445):

```ts
  chatPanels: () => invoke<ChatPanel[]>("chat_panels"),
```

- [ ] **Step 7: Type-check**

Run from `app/`: `npm run check`
Expected: no new errors.

- [ ] **Step 8: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Expose the chat window splits to the frontend"
```

---

### Task 3: `detail.ts` — part type, nominals, ship HUD and fighter internals

**Files:**
- Create: `app/src/lib/detail.ts`
- Create: `app/src/lib/detail.test.ts`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces:
  - `export interface DetailPart { kind: "ring" | "slot" | "cell" | "band" | "column" | "button"; x: number; y: number; w: number; h: number; label?: string }`
  - `export const DETAIL_NOMINAL` — object literal, fields listed in the code below.
  - `export function shipHudParts(): DetailPart[]`
  - `export function fighterParts(): DetailPart[]`

- [ ] **Step 1: Write the failing test**

Create `app/src/lib/detail.test.ts`:

```ts
// Run: npm test (node --test; Node strips the types). Throw-based checks, no
// framework — matching layout.test.ts.
import { DETAIL_NOMINAL, shipHudParts, fighterParts } from "./detail.ts";
import { HUD_NOMINAL } from "./layout.ts";

const check = (name: string, ok: boolean) => {
  if (!ok) throw new Error(`FAIL: ${name}`);
  console.log(`  ok - ${name}`);
};

// --- ship HUD --------------------------------------------------------------
{
  const parts = shipHudParts();
  const ring = parts.filter((p) => p.kind === "ring");
  const slots = parts.filter((p) => p.kind === "slot");

  check("one capacitor ring", ring.length === 1);
  // Measured: the ring spans x 73..231, so it is 158 wide and its left edge is
  // at 73 — NOT centred on the box.
  check("the ring sits at the measured span", ring[0].x === 73 && ring[0].w === 158);
  check("the ring is round", ring[0].h === ring[0].w);

  check("8 columns x 3 rows of module slots", slots.length === 24);
  // Measured: first slot x 245, column pitch 50.
  const row0 = slots.filter((p) => p.y === 2).sort((a, b) => a.x - b.x);
  check("the first slot is at the measured x", row0[0].x === 245);
  check("slots step by the measured column pitch", row0[1].x - row0[0].x === 50);
  // Measured verbatim, NOT as an averaged pitch: 2 -> 50 is 48, 50 -> 94 is 44.
  const tops = [...new Set(slots.map((p) => p.y))].sort((a, b) => a - b);
  check("the three row tops are the measured ones", tops.join(",") === "2,50,94");

  // The whole point of the measured box: everything drawn inside it must fit.
  // The ring is the one exception — its measured centre puts it 5px above the
  // box top, which the rectangle's own `overflow: hidden` clips.
  check(
    "every slot lies inside the measured ship HUD box",
    slots.every((p) => p.x >= 0 && p.x + p.w <= HUD_NOMINAL.shipui.w
      && p.y >= 0 && p.y + p.h <= HUD_NOMINAL.shipui.h),
  );
}

// --- fighter UI ------------------------------------------------------------
{
  const parts = fighterParts();
  const cells = parts.filter((p) => p.kind === "cell");

  // 5 x 3 ability grid plus a 5-cell squadron row = 20.
  check("20 fighter cells", cells.length === 20);

  // 178 is the MEASURED squadron row top (format-notes.md, "HUD anchors").
  const grid = cells.filter((p) => p.y < 178);
  const squad = cells.filter((p) => p.y >= 178);
  check("15 ability cells", grid.length === 15);
  check("5 squadron cells", squad.length === 5);

  // Measured: ability grid starts at x 70, squadron row at x 43, both on an
  // 86px column pitch.
  const top = grid.filter((p) => p.y === 0).sort((a, b) => a.x - b.x);
  check("the ability grid starts at the measured x", top[0].x === 70);
  check("ability columns step by the measured pitch", top[1].x - top[0].x === 86);
  const sq = squad.sort((a, b) => a.x - b.x);
  check("the squadron row starts at the measured x", sq[0].x === 43);
  check("squadron columns step by the measured pitch", sq[1].x - sq[0].x === 86);

  check(
    "every fighter cell lies inside the measured fighter box",
    cells.every((p) => p.x >= 0 && p.x + p.w <= HUD_NOMINAL.fighter.w
      && p.y >= 0 && p.y + p.h <= HUD_NOMINAL.fighter.h),
  );
}

console.log("detail.test.ts ok");
```

- [ ] **Step 2: Run the test to verify it fails**

Run from `app/`: `node --test src/lib/detail.test.ts`
Expected: FAIL — `Cannot find module './detail.ts'`.

- [ ] **Step 3: Write the implementation**

Create `app/src/lib/detail.ts`:

```ts
// The canvas detail layer: what each rectangle looks like INSIDE. Pure — no
// DOM, no Svelte — unit-tested in detail.test.ts, rendered by DetailParts.svelte.
//
// Decoration only. Nothing here may reach hudRects, snapLines, or any drag:
// that separation is why this is its own module and not more of layout.ts.

/**
 * One drawn piece of a rectangle's internals.
 *
 * Coordinates are DATA PX RELATIVE TO THE PARENT RECT'S ORIGIN — the same unit
 * as WindowRect.geom and FurnitureRect, so the canvas reuses `toCanvas` and the
 * parts scale with it for free. They are relative because the parts render as
 * children of the rectangle's own absolutely-positioned div, which already owns
 * the position and the clipping.
 */
export interface DetailPart {
  kind: "ring" | "slot" | "cell" | "band" | "column" | "button";
  x: number;
  y: number;
  w: number;
  h: number;
  label?: string;
}

/**
 * ponytail: every number here is INVENTED, and correcting them is a one-line
 * edit each. They are the sizes of things drawn inside a measured pitch, plus
 * the overview chrome, which has never had a measuring pass. Upgrade path: a
 * screenshot session like the 2026-07-28 one that produced the measured table
 * below, then move each corrected value out of this object and into a named
 * constant citing format-notes.md.
 *
 * The distinction matters. HUD_NOMINAL's invented 686x250 drew the ship HUD
 * 195px off its real position for three releases.
 */
export const DETAIL_NOMINAL = {
  /** Module slot cell, drawn inside the measured 50 x ~46 pitch. */
  slot: { w: 44, h: 40 },
  /** Fighter ability cell. The COLUMN pitch (86) is measured; the row pitch is
   *  not — 3 rows spanning y 0..178 gives ~59. */
  abilityCell: { w: 78, h: 52 },
  abilityRowPitch: 59,
  /** Fighter squadron cell, on the same measured 86 column pitch. */
  squadCell: { w: 78, h: 70 },
  /** The EVE-menu button at the top of the neocom. It is not in
   *  neocomButtonRawData, so the button column starts below it. */
  neocomTop: 40,
  /** Overview chrome. Never measured. */
  tabStrip: 18,
  headerBand: 16,
  /** Width for an overview column whose width key is absent (= EVE's own
   *  default, which the file does not record). */
  columnWidth: 80,
};

// --- ship HUD ---------------------------------------------------------------
// MEASURED 2026-07-28, docs/format-notes.md "HUD anchors", internal-geometry
// table. All offsets are from the element's own top-left.

/** Capacitor ring: spans x 73..231 (diameter ~158), centred on y ~74. */
const RING = { left: 73, diameter: 158, centreY: 74 };
/** Module slots: first slot x 245, column pitch 50, 8 columns x 3 rows max. */
const SLOTS = { firstX: 245, pitchX: 50, cols: 8 };
/**
 * Row tops used VERBATIM, not as a pitch: 2 -> 50 is 48 and 50 -> 94 is 44, so
 * a single averaged pitch would be wrong on two of the three rows.
 */
const SLOT_ROW_TOPS = [2, 50, 94];

/**
 * The ship HUD's internals. Constant: the box is fixed (HUD_NOMINAL.shipui) and
 * none of this is in any settings file.
 *
 * The slot count is the MAXIMUM (8), because it is ship-dependent and nothing
 * records it — and because the box's measured width was derived from it
 * (245 + 50 x 8 = 643). Drawing fewer would under-draw the footprint the canvas
 * already reserves.
 */
export function shipHudParts(): DetailPart[] {
  const out: DetailPart[] = [
    {
      kind: "ring",
      x: RING.left,
      // The measured centre puts the ring 5px above the box top. That is not a
      // transcription slip — the element's own `overflow: hidden` clips it, the
      // same way EVE's capacitor overhangs the rack block.
      y: RING.centreY - RING.diameter / 2,
      w: RING.diameter,
      h: RING.diameter,
    },
  ];
  for (const top of SLOT_ROW_TOPS) {
    for (let c = 0; c < SLOTS.cols; c++) {
      out.push({
        kind: "slot",
        x: SLOTS.firstX + SLOTS.pitchX * c,
        y: top,
        ...DETAIL_NOMINAL.slot,
      });
    }
  }
  return out;
}

// --- fighter UI -------------------------------------------------------------
// MEASURED 2026-07-28, same table: ability grid at x 70 / y 0, squadron row at
// x 43 / y ~178, both on an 86px column pitch, 5 columns max (a carrier's
// maximum squadron count, which is also what HUD_NOMINAL.fighter's width is).

const FIGHTER = { abilityX: 70, squadX: 43, squadY: 178, pitch: 86, cols: 5, rows: 3 };

/** The fighter panel's internals. Constant, and maxima, for the same reasons
 * as shipHudParts. */
export function fighterParts(): DetailPart[] {
  const out: DetailPart[] = [];
  for (let r = 0; r < FIGHTER.rows; r++) {
    for (let c = 0; c < FIGHTER.cols; c++) {
      out.push({
        kind: "cell",
        x: FIGHTER.abilityX + FIGHTER.pitch * c,
        y: DETAIL_NOMINAL.abilityRowPitch * r,
        ...DETAIL_NOMINAL.abilityCell,
      });
    }
  }
  for (let c = 0; c < FIGHTER.cols; c++) {
    out.push({
      kind: "cell",
      x: FIGHTER.squadX + FIGHTER.pitch * c,
      y: FIGHTER.squadY,
      ...DETAIL_NOMINAL.squadCell,
    });
  }
  return out;
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run from `app/`: `node --test src/lib/detail.test.ts`
Expected: PASS, every `ok - …` line printed and `detail.test.ts ok` at the end.

If the "lies inside the measured box" checks fail, the invented cell size is too big for the measured box — shrink the cell in `DETAIL_NOMINAL`, do not widen the box.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/detail.ts app/src/lib/detail.test.ts
git commit -m "Draw the ship HUD and fighter internals from the measured table"
```

---

### Task 4: `detail.ts` — neocom buttons and overview columns

**Files:**
- Modify: `app/src/lib/detail.ts` (append)
- Modify: `app/src/lib/detail.test.ts` (append, before the final `console.log`)

**Interfaces:**
- Consumes: `DetailPart`, `DETAIL_NOMINAL` from Task 3. Types `NeocomBar`, `OverviewColumns` from `api.ts` — `NeocomBar.buttons: NeocomButton[]` where `NeocomButton = { index, id, btn_type, icon_path, children }`; `OverviewColumns = { tabs: OverviewTab[]; windows: OverviewWindow[]; … }`, `OverviewTab = { index, name, preset, inherits, columns: OverviewColumn[] }`, `OverviewWindow = { index, tab_indices: number[] }`, `OverviewColumn = { name, label, visible, width: number | null }`.
- Produces:
  - `export function neocomParts(bar: NeocomBar, w: number, h: number): DetailPart[]`
  - `export function overviewParts(cols: OverviewColumns, windowIndex: number, rect: { w: number; h: number }): DetailPart[]`

- [ ] **Step 1: Write the failing test**

Append to `app/src/lib/detail.test.ts`, immediately before the final `console.log("detail.test.ts ok");`. Add `neocomParts, overviewParts` to the existing import from `./detail.ts`, and add this import line beside the others:

```ts
import type { NeocomBar, OverviewColumns } from "./api.ts";
```

```ts
// --- neocom ----------------------------------------------------------------
{
  const bar = (n: number): NeocomBar => ({
    buttons: Array.from({ length: n }, (_, i) => ({
      index: i, id: `btn${i}`, btn_type: 0, icon_path: "", children: 0,
    })),
    original: [],
  });

  // Bar 37 wide, 1440 tall: 40 reserved at the top, then 37px squares.
  const parts = neocomParts(bar(5), 37, 1440);
  check("one part per button", parts.length === 5);
  check("buttons are square, the bar's own width", parts[0].w === 37 && parts[0].h === 37);
  check("the column starts below the EVE menu cell", parts[0].y === DETAIL_NOMINAL.neocomTop);
  check("buttons stack by their own height", parts[1].y - parts[0].y === 37);
  check("buttons are labelled with their id", parts[2].label === "btn2");

  // A bar taller than the screen draws truncated, which is what EVE does.
  const short = neocomParts(bar(50), 37, 200);
  check("the column stops at the bar's height", short.length === 4);
  check(
    "no button is drawn past the bottom edge",
    short.every((p) => p.y + p.h <= 200),
  );
}

// --- overview columns ------------------------------------------------------
{
  const col = (name: string, visible: boolean, width: number | null) =>
    ({ name, label: name.toUpperCase(), visible, width });
  const cols: OverviewColumns = {
    tabs: [
      { index: 0, name: "General", preset: "p", inherits: false,
        columns: [col("icon", true, 30), col("distance", true, 90), col("name", false, 200), col("type", true, null)] },
      { index: 1, name: "Mining", preset: "p", inherits: false, columns: [col("icon", true, 30)] },
    ],
    windows: [{ index: 0, tab_indices: [0, 1] }, { index: 1, tab_indices: [] }],
    presets: [],
    appearance: { background: { enabled: [], order: [] }, flag: { enabled: [], order: [] }, colors: [], bools: [], defaulted: false },
  };

  const parts = overviewParts(cols, 0, { w: 400, h: 300 });
  const tabs = parts.filter((p) => p.kind === "cell");
  const bands = parts.filter((p) => p.kind === "column");

  check("one strip cell per tab in the window", tabs.length === 2);
  check("tab cells split the rect width", tabs[0].w === 200 && tabs[1].x === 200);
  check("tab cells are labelled with the tab name", tabs[0].label === "General");

  // Only visible columns, in stored order — `name` is hidden and must be gone.
  check("hidden columns are omitted", bands.length === 3);
  check("columns keep their stored order", bands.map((b) => b.label).join(",") === "ICON,DISTANCE,TYPE");
  check("columns use their stored widths", bands[0].w === 30 && bands[1].w === 90);
  check("an absent width falls back to the nominal", bands[2].w === DETAIL_NOMINAL.columnWidth);

  // Offsets are the running sum, and the band sits below the tab strip.
  check("bands start at the running sum of widths", bands[1].x === 30 && bands[2].x === 120);
  check("bands sit below the tab strip", bands.every((b) => b.y === DETAIL_NOMINAL.tabStrip));

  // THE payoff: a column set wider than its window runs off the edge, and that
  // is the signal. No clamping.
  const narrow = overviewParts(cols, 0, { w: 100, h: 300 }).filter((p) => p.kind === "column");
  check("an overflowing column set runs past the rect", narrow[2].x + narrow[2].w > 100);

  // A window with no tabs, and an index no window has.
  check("a window with no tabs draws nothing", overviewParts(cols, 1, { w: 400, h: 300 }).length === 0);
  check("an unknown window index draws nothing", overviewParts(cols, 9, { w: 400, h: 300 }).length === 0);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run from `app/`: `node --test src/lib/detail.test.ts`
Expected: FAIL — `neocomParts is not a function` (or an import error naming it).

- [ ] **Step 3: Write the implementation**

Append to `app/src/lib/detail.ts`. Add the type import at the top of the file, beside the existing imports:

```ts
import type { NeocomBar, OverviewColumns } from "./api";
```

```ts
// --- neocom -----------------------------------------------------------------

/**
 * The real buttons on the neocom, top-down. `w` is the bar's own width
 * (`neocomWidth`), which is also the cell size — EVE's neocom buttons are
 * square and fill the bar.
 *
 * Stops emitting when the next square would pass the bar's height, so a bar
 * with more buttons than fit draws truncated. That is what the client does; it
 * is not an error to report.
 */
export function neocomParts(bar: NeocomBar, w: number, h: number): DetailPart[] {
  const out: DetailPart[] = [];
  let y = DETAIL_NOMINAL.neocomTop;
  for (const b of bar.buttons) {
    if (y + w > h) break;
    out.push({ kind: "button", x: 0, y, w, h: w, label: b.id });
    y += w;
  }
  return out;
}

// --- overview ---------------------------------------------------------------

/**
 * An overview window's tab strip and the column header band of its FIRST tab.
 *
 * The first tab, because nothing in either settings file records which tab is
 * selected (`tabgroups` is chat-window state). Naming every tab in the strip is
 * what keeps that choice visible instead of silent.
 *
 * Column bands are laid out left to right from x 0 at their STORED widths, with
 * no clamping. A set wider than the window therefore runs off the edge and gets
 * clipped by the rectangle — which is the whole point: it makes an overflowing
 * overview visible without any overflow arithmetic.
 */
export function overviewParts(
  cols: OverviewColumns,
  windowIndex: number,
  rect: { w: number; h: number },
): DetailPart[] {
  const win = cols.windows.find((w) => w.index === windowIndex);
  const tabs = (win?.tab_indices ?? [])
    .map((i) => cols.tabs.find((t) => t.index === i))
    .filter((t): t is OverviewColumns["tabs"][number] => !!t);
  if (tabs.length === 0) return [];

  const out: DetailPart[] = [];
  // Equal-width tab cells. EVE sizes them by their text, but the information
  // here is which tabs the window holds, not how wide their labels render, and
  // an equal split always fits the rect it is drawn in.
  const tw = rect.w / tabs.length;
  tabs.forEach((t, i) => {
    out.push({ kind: "cell", x: i * tw, y: 0, w: tw, h: DETAIL_NOMINAL.tabStrip, label: t.name });
  });

  let x = 0;
  for (const c of tabs[0].columns) {
    if (!c.visible) continue;
    // width null = the key is absent = EVE's own default, which the file does
    // not record. The nominal is the only thing available.
    const w = c.width ?? DETAIL_NOMINAL.columnWidth;
    out.push({ kind: "column", x, y: DETAIL_NOMINAL.tabStrip, w, h: DETAIL_NOMINAL.headerBand, label: c.label });
    x += w;
  }
  return out;
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run from `app/`: `node --test src/lib/detail.test.ts`
Expected: PASS.

- [ ] **Step 5: Type-check**

Run from `app/`: `npm run check`
Expected: no new errors.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/detail.ts app/src/lib/detail.test.ts
git commit -m "Draw the real neocom buttons and overview columns"
```

---

### Task 5: `detail.ts` — chat splits and the id dispatcher

**Files:**
- Modify: `app/src/lib/detail.ts` (append)
- Modify: `app/src/lib/detail.test.ts` (append, before the final `console.log`)

**Interfaces:**
- Consumes: `DetailPart`, `DETAIL_NOMINAL`, `overviewParts` from Tasks 3-4. `ChatPanel` from `api.ts` (Task 2). `DrawUnit` from `layout.ts` — `{ key, anchor: WindowRect, stack: Stack | null, tabs: WindowRect[], fanTargets: WindowRect[] }`.
- Produces:
  - `export function chatParts(panel: ChatPanel, rect: { w: number; h: number }): DetailPart[]`
  - `export function overviewIndex(id: string): number | null`
  - `export function windowDetail(unit: DrawUnit, selectedId: string | null, cols: OverviewColumns | null, chats: ChatPanel[], rect: { w: number; h: number }): DetailPart[]`

- [ ] **Step 1: Write the failing test**

Append to `app/src/lib/detail.test.ts` before the final `console.log`. Add `chatParts, overviewIndex, windowDetail` to the `./detail.ts` import, `ChatPanel` and `WindowRect` to the `./api.ts` type import, and add:

```ts
import type { DrawUnit } from "./layout.ts";
```

```ts
// --- chat splits -----------------------------------------------------------
{
  const rect = { w: 256, h: 424 };
  const both: ChatPanel = { window_id: "chatchannel_local", userlist_width: 135, input_height: 64 };
  const parts = chatParts(both, rect);
  check("two bands when both values are stored", parts.length === 2);

  const members = parts[0];
  check("the member list is right-anchored", members.x === 256 - 135 && members.w === 135);
  check("the member list is full height", members.y === 0 && members.h === 424);

  const input = parts[1];
  check("the input is bottom-anchored", input.y === 424 - 64 && input.h === 64);
  // Drawn under the message pane only, not under the member list. NOT captured
  // in-game — the live smoke settles it.
  check("the input spans the message pane only", input.x === 0 && input.w === 256 - 135);

  // Absent means "never resized". Inventing a default would draw a split that
  // is not there.
  const widthOnly: ChatPanel = { window_id: "c", userlist_width: 135, input_height: null };
  check("no input band without a stored height", chatParts(widthOnly, rect).length === 1);
  const inputOnly: ChatPanel = { window_id: "c", userlist_width: null, input_height: 64 };
  const io = chatParts(inputOnly, rect);
  check("no member band without a stored width", io.length === 1);
  check("the input spans the full width with no member list", io[0].w === 256);
  const neither: ChatPanel = { window_id: "c", userlist_width: null, input_height: null };
  check("nothing drawn when neither is stored", chatParts(neither, rect).length === 0);
}

// --- id dispatch -----------------------------------------------------------
{
  check("the bare overview window is index 0", overviewIndex("overview") === 0);
  check("a numbered overview window is its number", overviewIndex("overview_7") === 7);
  check("the overview settings window is not an overview", overviewIndex("overviewsettings") === null);
  check("an unrelated id is not an overview", overviewIndex("market") === null);

  const w = (id: string): WindowRect => ({
    id, label: id, name: null, open: true, renderable: true,
    resolution_matches: true, geom: null, flags: [], stack: null,
  } as unknown as WindowRect);
  const free = (id: string): DrawUnit =>
    ({ key: id, anchor: w(id), stack: null, tabs: [w(id)], fanTargets: [w(id)] });

  const rect = { w: 400, h: 300 };
  const chats: ChatPanel[] = [
    { window_id: "chatchannel_local", userlist_width: 135, input_height: 64 },
  ];
  const cols: OverviewColumns = {
    tabs: [{ index: 0, name: "General", preset: "p", inherits: false,
             columns: [{ name: "icon", label: "ICON", visible: true, width: 30 }] }],
    windows: [{ index: 0, tab_indices: [0] }],
    presets: [],
    appearance: { background: { enabled: [], order: [] }, flag: { enabled: [], order: [] }, colors: [], bools: [], defaulted: false },
  };

  check("an overview window gets column parts",
    windowDetail(free("overview"), null, cols, chats, rect).some((p) => p.kind === "column"));
  check("an overview window with no projection draws nothing",
    windowDetail(free("overview"), null, null, chats, rect).length === 0);
  check("a chat window gets its splits",
    windowDetail(free("chatchannel_local"), null, cols, chats, rect).length === 2);
  check("a chat window with no stored panel draws nothing",
    windowDetail(free("chatchannel_corp"), null, cols, chats, rect).length === 0);
  check("an unrelated window draws nothing",
    windowDetail(free("market"), null, cols, chats, rect).length === 0);

  // A stack resolves from the SELECTED tab — a chat stack is the common case,
  // and the selected tab is the one you are looking at.
  const stack: DrawUnit = {
    key: "ChatWindowStack",
    anchor: w("ChatWindowStack"),
    stack: { container_id: "ChatWindowStack", anchor_id: "chatchannel_corp", members: ["chatchannel_corp", "chatchannel_local"] } as unknown as DrawUnit["stack"],
    tabs: [w("chatchannel_corp"), w("chatchannel_local")],
    fanTargets: [],
  };
  check("a stack resolves from its selected tab",
    windowDetail(stack, "chatchannel_local", cols, chats, rect).length === 2);
  check("a stack falls back to its first tab",
    windowDetail(stack, null, cols, chats, rect).length === 0);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run from `app/`: `node --test src/lib/detail.test.ts`
Expected: FAIL — `chatParts is not a function` (or an import error naming it).

- [ ] **Step 3: Write the implementation**

Append to `app/src/lib/detail.ts`. Extend the type imports at the top:

```ts
import type { ChatPanel, NeocomBar, OverviewColumns } from "./api";
import type { DrawUnit } from "./layout";
```

```ts
// --- chat -------------------------------------------------------------------

/**
 * A chat window's member-list and input splits, from the stored widths.
 *
 * Either field being null means the player has never resized that split, so the
 * part is OMITTED rather than drawn at a guessed default — a split that is not
 * in the file is a split the canvas has nothing to say about.
 *
 * The input band spans the message pane only, not the full window width. That
 * is the one thing here NOT confirmed against the client (format-notes.md,
 * "Chat window splits") — the live smoke settles it, and it is a one-line
 * change either way.
 */
export function chatParts(panel: ChatPanel, rect: { w: number; h: number }): DetailPart[] {
  const out: DetailPart[] = [];
  const members = panel.userlist_width;
  if (members !== null) {
    out.push({ kind: "band", x: rect.w - members, y: 0, w: members, h: rect.h, label: "Members" });
  }
  if (panel.input_height !== null) {
    out.push({
      kind: "band",
      x: 0,
      y: rect.h - panel.input_height,
      w: rect.w - (members ?? 0),
      h: panel.input_height,
      label: "Input",
    });
  }
  return out;
}

// --- dispatch ---------------------------------------------------------------

/**
 * The overview window index a canvas window id names: `overview` is window 0,
 * `overview_N` is window N — the positional link `overview_tabs.rs` documents
 * on `add_overview_window` and enforces on `remove_overview_window`.
 *
 * Anchored at both ends so `overviewsettings` cannot match.
 */
export function overviewIndex(id: string): number | null {
  if (id === "overview") return 0;
  const m = /^overview_(\d+)$/.exec(id);
  return m ? Number(m[1]) : null;
}

/**
 * The detail parts for a drawn window unit, or `[]` when the unit is not a
 * family this layer knows about (which is most of them).
 *
 * For a STACK the family is resolved from the tab carrying the selection,
 * falling back to the first tab. A chat stack (`ChatWindowStack`) is the common
 * case, and the selected tab is the one the player is looking at — resolving
 * from the anchor instead would show one channel's splits while another's tab
 * is active.
 *
 * A pure function rather than a ternary in markup, so the id resolution is
 * unit-tested.
 */
export function windowDetail(
  unit: DrawUnit,
  selectedId: string | null,
  cols: OverviewColumns | null,
  chats: ChatPanel[],
  rect: { w: number; h: number },
): DetailPart[] {
  const id = unit.stack
    ? (unit.tabs.find((t) => t.id === selectedId)?.id ?? unit.tabs[0]?.id ?? unit.anchor.id)
    : unit.anchor.id;

  const ov = overviewIndex(id);
  if (ov !== null) return cols ? overviewParts(cols, ov, rect) : [];

  const panel = chats.find((c) => c.window_id === id);
  return panel ? chatParts(panel, rect) : [];
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run from `app/`: `node --test src/lib/detail.test.ts`
Expected: PASS.

- [ ] **Step 5: Run the whole frontend suite and type-check**

Run from `app/`: `npm test` then `npm run check`
Expected: PASS, no new type errors.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/detail.ts app/src/lib/detail.test.ts
git commit -m "Draw the chat splits and resolve which detail a window gets"
```

---

### Task 6: The `Detail` preference and its toggle

**Files:**
- Modify: `app/src-tauri/src/prefs.rs:17-23` (the `LayoutPrefs` struct)
- Modify: `app/src/lib/api.ts:194-197` (the `LayoutPrefs` interface)
- Modify: `app/src/lib/prefs.ts:12-15` (`withoutIn`)
- Modify: `app/src/lib/prefs.svelte.ts:16` (the `$state` default), `:71-81` (`setClutterOverride`), and append the new accessors
- Modify: `app/src/lib/prefs.test.ts` (two existing fixtures on lines 12 and 23, plus an appended block)
- Modify: `app/src/lib/LayoutView.svelte` (the `.ref` paragraph, ~line 853)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces:
  - Rust: `LayoutPrefs { clutter: Vec<String>, visible: Vec<String>, detail: bool }`
  - TS: `LayoutPrefs { clutter: string[]; visible: string[]; detail: boolean }`
  - TS: `export const detailOn = (): boolean` and `export function setDetail(on: boolean): void` from `prefs.svelte.ts`

**THE TRAP in this task:** `withoutIn` and `setClutterOverride` both build a *fresh* `LayoutPrefs` object literal listing `clutter` and `visible` explicitly. Adding a third field without spreading the original silently drops it — clicking "clear overrides" would turn Detail off. TypeScript catches this at `npm run check` (the literal is missing a required property), but only if you run it. Both must spread the source object.

- [ ] **Step 1: Write the failing test**

Append to `app/src/lib/prefs.test.ts` (read the file first — it defines its own `check` helper and fixtures; reuse them, do not redeclare):

```ts
// A third field on LayoutPrefs must survive the helpers that rebuild the
// object. Both `withoutIn` and `setClutterOverride` construct a fresh literal;
// without a spread, clearing overrides would silently turn Detail off.
{
  const stored = { clutter: ["a", "b"], visible: ["c"], detail: true };
  const out = withoutIn(stored, new Set(["a"]));
  check("withoutIn drops only the in-scope ids", out.clutter.join(",") === "b");
  check("withoutIn preserves the detail flag", out.detail === true);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run from `app/`: `node --test src/lib/prefs.test.ts`
Expected: FAIL — `FAIL: withoutIn preserves the detail flag` (`out.detail` is `undefined`).

- [ ] **Step 3: Add the field, back to front**

`app/src-tauri/src/prefs.rs`, inside `LayoutPrefs`:

```rust
    /// Whether the layout canvas draws each rectangle's internals. Purely a
    /// view setting — it changes nothing about any EVE settings file.
    pub detail: bool,
```

`app/src/lib/api.ts`, inside `LayoutPrefs`:

```ts
  /** Whether the layout canvas draws each rectangle's internals. */
  detail: boolean;
```

`app/src/lib/prefs.ts`, `withoutIn` — spread, do not re-list:

```ts
export const withoutIn = (stored: LayoutPrefs, ids: ReadonlySet<string>): LayoutPrefs => ({
  ...stored,
  clutter: stored.clutter.filter((id) => !ids.has(id)),
  visible: stored.visible.filter((id) => !ids.has(id)),
});
```

`app/src/lib/prefs.svelte.ts` — the `$state` default on line 16:

```ts
let prefs = $state<Preferences>({ layout: { clutter: [], visible: [], detail: false } });
```

and `setClutterOverride`'s literal, which has the same trap:

```ts
  prefs = {
    ...prefs,
    layout: {
      ...l,
      clutter: l.clutter.filter((x) => x !== id).concat(mode === "clutter" ? [id] : []),
      visible: l.visible.filter((x) => x !== id).concat(mode === "visible" ? [id] : []),
    },
  };
```

and append the accessors at the end of the file:

```ts
/** Whether the layout canvas draws each rectangle's internals. */
export const detailOn = (): boolean => prefs.layout.detail;

/** Same chained write as setClutterOverride — only the value written changed. */
export function setDetail(on: boolean): void {
  prefs = { ...prefs, layout: { ...prefs.layout, detail: on } };
  persist(prefs);
}
```

Finally, the two EXISTING `LayoutPrefs` fixtures in `app/src/lib/prefs.test.ts` (lines 12 and 23) are now missing a required property. `node --test` strips types without checking them, so the tests keep passing — only `npm run check` catches this. Add the field to both:

```ts
  const stored = { clutter: ["market", "chatchannel_corp"], visible: ["overview"], detail: false };
```

- [ ] **Step 4: Run the test to verify it passes**

Run from `app/`: `node --test src/lib/prefs.test.ts` then `npm run check`
Expected: PASS, and no type errors — the type-check is what proves no other literal in the codebase drops the new field.

- [ ] **Step 5: Add the checkbox**

In `app/src/lib/LayoutView.svelte`, import the accessors on the existing `prefs.svelte` import line (~line 12):

```ts
  import { clutterOverrides, overrideCount, clearClutterOverrides, setClutterOverride, detailOn, setDetail } from "$lib/prefs.svelte";
```

Add to the `.ref` paragraph, immediately after the `reference {layout.reference_w}×{layout.reference_h}` line and before the `{#if !readOnly}` block:

```svelte
        <label class="det">
          <input type="checkbox" checked={detailOn()} onchange={(e) => setDetail(e.currentTarget.checked)} />
          Detail
        </label>
```

and the style, beside `.hintish` in the `<style>` block:

```css
  /* Explicit colours per the dark-native-controls note: an unstyled checkbox
     renders light-on-light in this theme. */
  .det {
    color: #888;
    cursor: pointer;
    margin-left: 0.4rem;
  }
  .det input {
    accent-color: var(--accent);
    vertical-align: -1px;
  }
```

- [ ] **Step 6: Verify it persists**

Run from `app/`: `npm run tauri dev`
- Open a character file, go to Layout, tick **Detail**. Nothing draws yet — that is Task 7.
- Close the app, reopen it, open Layout: the box is still ticked.
- Tick a clutter override on a window, then click **clear** on the override counter: **Detail stays ticked** (this is the trap from the task header, verified end to end).

- [ ] **Step 7: Commit**

```bash
git add app/src-tauri/src/prefs.rs app/src/lib/api.ts app/src/lib/prefs.ts \
        app/src/lib/prefs.svelte.ts app/src/lib/prefs.test.ts app/src/lib/LayoutView.svelte
git commit -m "Add the Detail toggle and persist it"
```

---

### Task 7: Render the detail parts on the canvas

**Files:**
- Create: `app/src/lib/DetailParts.svelte`
- Modify: `app/src/lib/LayoutView.svelte` (imports, `load()`, state, the furniture `{#each}` ~line 797, the units `{#each}` ~line 810, `.tabs` style ~line 1030)

**Interfaces:**
- Consumes: `DetailPart`, `shipHudParts`, `fighterParts`, `neocomParts`, `windowDetail` (Tasks 3-5); `detailOn` (Task 6); `api.chatPanels` (Task 2); `toCanvas` from `layout.ts`.
- Produces: `DetailParts.svelte` with props `{ parts: DetailPart[]; scale: number }`.

- [ ] **Step 1: Write the component**

Create `app/src/lib/DetailParts.svelte`:

```svelte
<script lang="ts">
  import { toCanvas } from "$lib/layout";
  import type { DetailPart } from "$lib/detail";

  let { parts, scale }: { parts: DetailPart[]; scale: number } = $props();

  /** A label is only worth drawing when it has room to be read. Below this it
   * is dropped rather than ellipsised — a row of "…" is noise, not information. */
  const LABEL_MIN = 28;
</script>

<!-- pointer-events: none is the ONE mechanism that keeps this layer decoration:
     no part can swallow a drag on the rectangle it decorates, be hit-tested, or
     reach any of the canvas's gesture code. Do not remove it. -->
<div class="detail">
  {#each parts as p, i (i)}
    {@const w = toCanvas(p.w, scale)}
    <div
      class="part {p.kind}"
      style="left: {toCanvas(p.x, scale)}px; top: {toCanvas(p.y, scale)}px;
             width: {w}px; height: {toCanvas(p.h, scale)}px;">
      {#if p.label && w > LABEL_MIN}<span>{p.label}</span>{/if}
    </div>
  {/each}
</div>

<style>
  .detail {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .part {
    position: absolute;
    box-sizing: border-box;
    border: 1px solid rgba(148, 163, 184, 0.45);
    color: #94a3b8;
    font-size: 9px;
    line-height: 1;
    overflow: hidden;
    white-space: nowrap;
  }
  .ring {
    border-radius: 50%;
  }
  /* The two data-driven bands read as panels, not outlines — they are the parts
     whose SIZE is the information. */
  .band,
  .column {
    background: rgba(148, 163, 184, 0.14);
  }
  .part span {
    padding: 0 2px;
  }
</style>
```

- [ ] **Step 2: Load the two projections**

In `app/src/lib/LayoutView.svelte`, extend the `api.ts` type import (line 3) with `OverviewColumns, ChatPanel`, and add the new state beside `neocom` (~line 54):

```ts
  let columns = $state<OverviewColumns | null>(null);
  let chats = $state<ChatPanel[]>([]);
```

Add to the end of `load()` (~line 123), after the `neocom` line:

```ts
    // Same tolerance as the HUD and the neocom: a character with no overview
    // container, or an account file opened on its own, must not take the canvas
    // down with it. Detail is a bonus layer.
    columns = await api.overviewColumns().catch(() => null);
    chats = await api.chatPanels().catch(() => []);
```

- [ ] **Step 3: Wire the furniture**

Add to the imports:

```ts
  import DetailParts from "$lib/DetailParts.svelte";
  import { shipHudParts, fighterParts, neocomParts, windowDetail } from "$lib/detail";
```

Add beside the other small helpers (near `fRectOf`, ~line 358):

```ts
  /** The internals of a furniture element. The ship HUD and fighter are
   * constant (measured, not stored); the neocom is drawn from its real button
   * list. The badge has neither, so it stays a plain box. */
  const furnitureDetail = (f: FurnitureRect) =>
    f.kind === "shipui" ? shipHudParts()
    : f.kind === "fighter" ? fighterParts()
    : f.kind === "neocom" && neocom ? neocomParts(neocom, f.w, f.h)
    : [];
```

In the furniture `{#each}` block (~line 806), add inside the `<div class="furniture">`, before the `<span class="furniture-label">`:

```svelte
            {#if detailOn()}
              <DetailParts parts={furnitureDetail(f)} {scale} />
            {/if}
```

- [ ] **Step 4: Wire the windows**

In the units `{#each}` block, add inside the `<div class="win">` as its FIRST child — before the `{#if unit.stack}` tab strip — so the strip's own `z-index` wins:

```svelte
            {#if detailOn()}
              <DetailParts parts={windowDetail(unit, selectedId, columns, chats, r)} {scale} />
            {/if}
```

And in the `<style>` block, extend `.tabs` so a chat stack's strip stays above its own detail parts (absolutely-positioned children otherwise paint over static siblings):

```css
  .tabs {
    display: flex;
    gap: 1px;
    background: #11141a;
    overflow: hidden;
    /* Above the detail layer, which is an absolutely-positioned sibling. */
    position: relative;
    z-index: 1;
  }
```

Do the same for `.win-label` (a free overview window's label must not be buried under its own tab strip):

```css
  .win-label {
    padding: 1px 3px;
    display: block;
    box-sizing: border-box;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    pointer-events: none;
    position: relative;
    z-index: 1;
  }
```

- [ ] **Step 5: Type-check and run the suite**

Run from `app/`: `npm run check` then `npm test`
Expected: no new type errors, all tests pass.

- [ ] **Step 6: Verify in the app**

Run from `app/`: `npm run tauri dev`

With **Detail** on, confirm each of these — this is the acceptance list:

1. The ship HUD box shows a circle on its left and three rows of eight cells to its right.
2. The fighter box (if `detachFighterUI` and `displayFighterUI` are both on) shows a 5×3 grid and a row of five below it.
3. The neocom bar shows a column of squares — **count them against the HUD panel's neocom list**, they must match.
4. The overview window shows a tab strip naming its real tabs and a header band of its real columns; toggling a column off in the Overview view and returning shows it gone.
5. A chat window (turn `Hide clutter` OFF, or the chat windows are filtered out) shows a right-hand member panel and a bottom input band.
6. **Dragging still works everywhere.** Grab the ship HUD, a window, a stack tab — the detail parts must never intercept the pointer. Grab a window by a spot covered by a detail part specifically.
7. Turning **Detail** off returns the canvas to plain boxes.

- [ ] **Step 7: Commit**

```bash
git add app/src/lib/DetailParts.svelte app/src/lib/LayoutView.svelte
git commit -m "Render the detail layer on the canvas"
```

---

### Task 8: Ledger, changelog, and the follow-up entry

**Files:**
- Modify: `docs/small-tasks.md` (the entry at line 16, and the Shipped section)
- Modify: `CHANGELOG.md` (the `## [Unreleased]` → `### Added` block)

**Interfaces:** none.

- [ ] **Step 1: Close the ledger entry and open its successor**

In `docs/small-tasks.md`, remove the "A drawing layer for the canvas" entry from **Open** and add it to **Shipped** in that section's existing format (read the top of the Shipped section for the exact shape before writing).

Then add a NEW entry to **Open**, because the spec deferred it explicitly (design spec §6):

```markdown
- [ ] **Draggable chat splits and overview column edges on the canvas.** The
  detail layer draws the chat member-list width, the chat input height and the
  overview column widths from their real stored values, but they are decoration
  — `DetailParts.svelte` is `pointer-events: none` by design. Making them
  draggable needs a `chat.rs` setter (the projection is read-only today),
  `set_overview_width` wired into the Layout view (it exists, but only the
  Overview view calls it), new `Drag` variants in `LayoutView.svelte`, and
  hit-test exclusions so a split drag does not start a window move underneath.
  Split out of the detail-layer slice, which shipped the read half.
  _Added 2026-07-30._

- [ ] **The overview and chat internals have never been measured.** Every
  number in `detail.ts`'s `DETAIL_NOMINAL` is invented — the module slot cell
  size, the fighter ability/squadron cell sizes and row pitch, the neocom's top
  EVE-menu cell, the overview tab-strip and header-band heights, and the
  fallback width for a column with no stored width. The HUD and fighter PITCHES
  around them are measured (`format-notes.md`, "HUD anchors"); only what is
  drawn inside them is guessed. One screenshot session like the 2026-07-28 one
  settles all of it, and each is a one-line edit. Also open from the same
  session: whether the chat input box spans the full window width or only the
  message pane — the editor draws the latter.
  _Added 2026-07-30._
```

- [ ] **Step 2: Add the changelog entry**

In `CHANGELOG.md`, add to the existing `## [Unreleased]` → `### Added` block, above the right-click entry:

```markdown
- **A `Detail` toggle that draws what is inside each rectangle on the layout
  canvas.** The canvas has always drawn blank boxes, so a correctly-placed
  overview window still told you nothing about what it holds. With Detail on,
  the ship HUD shows its capacitor ring and module racks, the fighter panel its
  ability grid and squadron row, the neocom your actual buttons in your actual
  order, each overview window its real tabs and its real columns at their
  stored widths — so a column set too wide for its window is now visible as
  columns running off the edge — and each chat window its member-list and input
  splits. The HUD and fighter geometry is measured from the client; the rest
  comes from your own settings. It is decoration only: nothing here can be
  dragged, and nothing snaps to it.
```

- [ ] **Step 3: Commit**

```bash
git add docs/small-tasks.md CHANGELOG.md
git commit -m "Note the canvas detail layer in the changelog and ledger"
```

- [ ] **Step 4: Full verification before opening the PR**

Run, and paste the output rather than summarising it:

```bash
cargo test --workspace
cd app && npm test && npm run check
```

Expected: all green. Then open the PR from `worktree-canvas-detail-layer`.
