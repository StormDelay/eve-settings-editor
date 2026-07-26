# Layout depth — neocom button editor (design)

Status: designed 2026-07-27, not yet planned.

Milestone context: the **layout depth** milestone, cut into four slices (see the
HUD furniture spec §7). Slice 3 (HUD furniture) shipped as v0.15.0, slice 1a
(canvas & list usability) as v0.16.0, slice 1b (precision editing) as v0.17.0,
the names-and-noise slice as v0.19.0, and slice 2 (stack polish) is merged and
unreleased. This spec covers **slice 4 — the neocom button editor**, the last
slice of the milestone.

Builds on: `hud.rs` (which already owns `neocomWidth` and draws the neocom bar
as canvas furniture), `HudPanel.svelte`'s Neocom group, the structural-editor
pattern of `stacks.rs` / `overview_tabs.rs`, and `batch.rs`'s category model.

## 1. Goal

The neocom is the vertical button bar down the left edge of the client, and the
order of its buttons is muscle memory: a player who has clicked *wallet* in the
same spot for ten years does not want it third on a new character. Setting it up
in-game means dragging buttons one at a time, per character, with no way to copy
the result anywhere.

The editor already draws the neocom on the layout canvas and edits its width.
This slice makes the buttons themselves editable — reorder, remove, add back,
reset — and folds them into the layout batch aspect, so a bar arranged once can
be given to every other character in one operation.

## 2. What the file holds — corpus-verified

The milestone spec assumed this slice would need its own in-game capture
experiment, because `btnType` and `children` semantics were unmapped. **It does
not.** A survey of the real corpus (`crates/settings-model/src/bin` probe, since
deleted) over **4,215 character files — 4,061 of them carrying the key, 43,430
buttons** settled every question:

- **Location:** character file, `ui → neocomButtonRawData`, wrapped as
  `(timestamp, List[Instance])`. Note the asymmetry with `neocomWidth`, which is
  account-side: the *bar* is per account, its *buttons* are per character.
- **Class:** `utillib.KeyVal`, 43,430 out of 43,430. No other class appears.
- **Keyset:** exactly `btnType, children, iconPath, id` — again 43,430 out of
  43,430, with no variation at all. Authoring one is a fixed four-key dict.
- **`btnType`** takes four values, and each tracks *what the button is* rather
  than anything the user chose: `10` occurs exactly once per file and always on
  `chat`; `21` only on `airCareerProgram`; `4` on the inventory family
  (`inventory`, `InventoryStation`, `InventoryStructure`); `1` on everything
  else (34,593 of them). It is an attribute of the id, not a setting.
- **`children`** is `None` (40,339), `List[0]` (2,585) or `List[1]` (506) — never
  more than one child, and the only ids ever seen as children are
  `InventoryStation` and `InventoryStructure`. The "folder groups" the milestone
  spec worried about are the Inventory docking-context variants, not user-built
  folders.
- **25 distinct ids** across the corpus.
- **One malformed shape:** 11 buttons carry an `id` that is a `Tuple(bytes,
  None)` rather than plain bytes. Rare, real, and it has to be tolerated.
- **`neocomButtonRawDataOriginal`** is the same instance shape in a `Tuple` of
  8–14 entries.

### 2.1 `Original` is a stale snapshot, not a catalog

This is the finding that shapes §5. Only **495** of the ~4,061 files have a live
bar that is a subset of their own `Original`. Nine ids routinely appear on the
bar and *not* in it — `fleet` (3,509 files), `accessgroups` (3,465),
`corporation` and `structurebrowser` (3,454 each), `log` and `notepad` (3,443
each), plus `contracts`, `job_board` and `shipTree` — and `Original`'s timestamp
is older than the live bar's. The client wrote it once and later patches added
buttons without revisiting it.

Meanwhile **3,454 files have buttons removed** relative to `Original` (up to 9
of them), so removal is something players really do.

`Original` is therefore usable as *a* source of addable buttons and as the reset
baseline, but it is not the answer to "what buttons exist".

All of §2 belongs in `docs/format-notes.md` as the capture record.

## 3. The projection

`neocom.rs`, a new module in `settings-model`, following the shape of
`stacks.rs`:

```rust
pub struct NeocomBar {
    pub buttons: Vec<NeocomButton>,   // in bar order, top to bottom
    pub original: Vec<NeocomButton>,  // neocomButtonRawDataOriginal, as-is
}
pub struct NeocomButton {
    pub index: usize,
    pub id: String,          // the Tuple-id shape renders as its bytes half
    pub btn_type: i64,
    pub icon_path: String,
    pub children: usize,     // 0 for None or an empty list — the write path never re-authors it
}
```

The projection reports only what is **in the file**: the live bar and the
character's own baseline. It does not compute an "addable" set, because half of
that set comes from the bundled catalog, which lives in the frontend (§5) — the
union and the subtraction happen there, where both halves are in hand. Reporting
`original` as the same struct lets the UI show where an addable button came from:
one the character's own client wrote is more trustworthy than one the catalog
supplied.

## 4. The commands key by index, not by id

Four commands in `neocom.rs`, wired through `ops.rs` exactly as `stack_*` are:

```rust
pub fn reorder(v: &mut Value, order: &[usize]) -> Result<(), NeocomError>;
pub fn remove(v: &mut Value, index: usize) -> Result<(), NeocomError>;
pub fn add(v: &mut Value, id: &str, btn_type: i64, icon_path: &str) -> Result<(), NeocomError>;
pub fn reset(v: &mut Value) -> Result<(), NeocomError>;
```

**Index, not id, is the key** — the decision the corpus forced. Eleven buttons
carry a `Tuple(bytes, None)` id, so an id-keyed command would need a special case
for them, and it would break outright on the duplicate display ids that shape can
produce. An index is unambiguous for every button that exists.

`reorder` and `remove` move or drop the **whole instance**, so a button's
`children`, its icon and even a malformed id ride along untouched — nothing is
re-authored. `reorder` takes a permutation of the current indices and rejects
anything that is not one (wrong length, out of range, or a repeat), which makes a
stale index from a UI that has drifted a clean error instead of a silent
scramble.

`add` is the only command that builds a new `KeyVal`: class `utillib.KeyVal`,
state `{btnType, children: None, iconPath, id}` written in the corpus's own key
order, appended at the end of the list. `reset` replaces the live list with a
copy of `Original`'s entries (a `List`, not the `Tuple` they are stored in).

All four inline first and let the app layer reshare before saving, like every
other structural editor here. The `(timestamp, payload)` wrapper is preserved and
its timestamp left alone: the client rewrites it on logout.

`NeocomError { NoUi, NoBar, NoOriginal, BadIndex, BadOrder }`, surfaced through
the existing error-dialog path.

## 5. The catalog

`tools/gen-neocom-catalog.py` harvests the 25 known ids with their canonical
`btnType` and `iconPath` into `app/src/lib/data/neocom-buttons.json` — the same
pattern as the bundled `default-presets.json` and `command-defaults.json`. The
generated file carries only client-generic ids and texture paths; no character
data goes into it.

The frontend owns the catalog, unions it with the character's own `Original`,
and passes the three fields to `neocom_add`. The backend writes what it is told
rather than embedding and parsing a second copy of the same table. A button the
catalog does not know but the character's `Original` does is therefore still
addable, which is the point of the union.

## 6. UI

`HudPanel`'s existing Neocom group grows a **Buttons** list beneath Width: one
row per button showing its id, with ↑/↓ to move it and ✕ to remove it, an
*Add…* dropdown of the addable set, and *Reset to original* behind a confirm
(disabled when the character has no `Original`). Selecting the neocom bar on the
canvas already selects this group, so the bar the user clicks and the list they
edit are the same object.

↑/↓ rather than drag: `WindowPanel`'s stack rows already work that way, and the
side panel is 14–20rem wide. Panel drag-reorder is a later polish, not this
slice. Any native `select` gets explicit dark styling — see the standing note
about native controls rendering light in this WebView2 app.

## 7. Batch: part of the layout aspect

`Aspect::Layout` currently pushes one category, `Category::Layout`
(`[b"windows"]`). It gains a second, `Category::NeocomButtons`
(`[b"ui", b"neocomButtonRawData"]`) — the same one-aspect-to-two-categories shape
`Aspect::Overview` already uses for `Overview` + `OverviewWidths`. Both are
character-side, so nothing about the file routing changes.

There is **no separate Neocom batch aspect**: a player copying their window
layout to an alt wants the bar that goes with it.

`neocomButtonRawDataOriginal` is deliberately **not** copied. It is the target's
own client baseline, and overwriting it with the source's would corrupt what
*Reset to original* means on that character.

`copies_char_geometry()` keys off `Category::Layout` and is unaffected. The
resolution-differ warning stays accurate: neocom buttons carry no coordinates, so
a resolution mismatch has no bearing on them.

## 8. Testing

- **`neocom.rs` unit tests** on a synthetic `KeyVal` tree: the projection
  (including a `Tuple`-shaped id and a button with one child), each of the four
  commands, `reorder`'s three rejection paths, `add`'s key order and count,
  `reset` against a `Tuple`-stored `Original`, and the missing-`Original` error.
- **`neocom_realshape.rs`**, a corpus gate in the style of the existing
  `*_realshape.rs` tests: every corpus character file projects without error, and
  a reorder-then-encode round-trips.
- **`batch.rs`**: a `Category::NeocomButtons` extract/apply case, plus one
  asserting `Original` is not carried across.
- **`HudPanel.spec.ts`**: the list renders in bar order, ↑/↓ disable at the ends,
  and the Add dropdown excludes what is already on the bar.
- **Live smoke**: reorder, remove, add and reset on a real character; log the
  character out *before* saving (EVE writes its settings on logout); confirm
  in-game that the bar matches, that a removed button is gone, and that an added
  one works rather than rendering as a dead icon. Then batch-copy the layout
  aspect to a second character and confirm its bar follows.

## 9. Non-goals

- **Folder editing.** The corpus has never seen a folder with more than one
  child, and the only children are the Inventory context variants. A child rides
  with its parent through reorder and remove; nothing nests or unnests.
- **Changing a button's icon or `btnType`.** Both are attributes of what the
  button *is* (§2), not preferences.
- **Inventing ids.** Only the catalog and the character's own `Original` supply
  addable buttons.
- **Repairing the `Tuple`-shaped ids.** They are tolerated on read and preserved
  on write; rewriting them would be a guess about what the client meant.
- **Drag-to-reorder in the panel.** See §6.
