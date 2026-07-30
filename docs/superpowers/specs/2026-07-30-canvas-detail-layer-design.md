# The canvas detail layer — module slots, overview columns, chat splits (design)

Status: designed 2026-07-30.

Milestone context: the **layout depth** milestone, ledger item at
`docs/small-tasks.md:16` ("A drawing layer for the canvas: module slots, fighter
abilities, overview columns", added 2026-07-28 when the HUD-footprint task
shipped the sizing half). This spec covers that entry and extends it with two
data sources the entry did not know about: the neocom's real button list, and
the chat windows' stored member-list and input-box sizes.

## 1. Goal

Every rectangle the canvas draws is blank. The HUD footprint is now correct to
the pixel (2026-07-28) and the overview window sits exactly where EVE puts it,
but a blank box does not tell a player *what* they are positioning against —
recognising the thing is half of why the footprint mattered.

A **Detail** toggle draws each rectangle's internals: the capacitor ring and
module racks inside the ship HUD, the ability grid inside the fighter panel, the
real button column inside the neocom, the real columns at their real widths
inside each overview window, and the member-list / input split inside each chat
window.

It is **decoration only**. It never joins the snap lines, never starts a drag,
never hit-tests. Off, the canvas is exactly what it is today.

## 2. What the data actually says

Two kinds of detail, and the distinction runs through the whole design.

**Measured, invariant.** The ship HUD and fighter internals are not in any
settings file — they are the client's own geometry, measured 2026-07-28 from
native 2560×1440 screenshots and tabulated in `format-notes.md` §"HUD anchors":

| Element | Part | Offset from box origin | Pitch | Count |
|---|---|---|---|---|
| Ship HUD | capacitor wheel centre | x 148, y ~74 | — | 1 |
| Ship HUD | capacitor ring outer | spans x 73..231 (⌀ ~158) | — | 1 |
| Ship HUD | module slot rows | first slot x 245, row tops y ~2 / ~50 / ~94 | x 50 | 8 × 3 max |
| Fighter | ability grid | x 70, y 0 | 86 | 5 × 3 max |
| Fighter | squadron row | x 43, y ~178 | 86 | 5 max |

Every offset is from the element's own top-left, and the boxes are fixed
(`HUD_NOMINAL`), so these are constants. The drawn counts are **maxima**: slot
count is ship-dependent and squadron count is fitting-dependent, and neither is
in the file. Drawing the maximum is the honest choice — it is what the footprint
already reserves, and it is what the box's measured width was derived from
(`245 + 50 × 8 = 643`).

Note the row tops are used **verbatim**, not as a pitch: 2 → 50 → 94 is 48 then
44, so a single averaged pitch would be wrong twice.

**Stored, per-character.** Everything else comes from a real projection and
differs per file:

| Element | Source | What it shows |
|---|---|---|
| Neocom | `NeocomBar.buttons` | the actual buttons, in the actual order |
| Overview | `OverviewColumns` | visible columns, real order, real stored widths |
| Chat | new `ChatPanel` (§3) | member-list width, input-box height |

### 2.1 The chat keys (corpus-verified 2026-07-30)

Both live under the account file's **root `ui` section** — *not* `windows`,
where `neocomWidth` lives:

| what | key | value | present |
|---|---|---|---|
| Member-list width | `chatchannel_<ch>_userlistwidth` | Int — 107, 135, 126, 104, 50 | 86/184 |
| Input-box height | `chatinputsize_chatchannel_<ch>` | Int — 64, 63 | 121/184 |

Counts are real `core_user_*.dat` files carrying the key, measured by running
the projection over the corpus (184 files, 705 total sightings, **zero** under
`windows`). Both are ordinary `(timestamp, value)` leaves needing the usual
value-wrapper unwrap.

**This section name was wrong in the first draft of this spec, and the way it
was wrong is the point.** The draft said `windows`, from a text dump of one real
account file: `b"windows"` appears a few lines above the chat keys, and the
section that actually encloses them prints as `ref[240]:` — a `Ref`-keyed
section key, invisible to a grep for a byte-string name. That is the same trap
`format-notes.md` records for this file ("The account file's `ui` section key is
`Ref`-keyed") and the same class as the v0.15.0 `badge_*` bug. The corpus guard
in `tests/chat_panels_corpus.rs` is what caught it; a fixture-only test suite
would have passed on the wrong section, reading nothing from any real file.

**The key names carry the canvas window id verbatim.** `chatchannel_local` on the
canvas owns `chatchannel_local_userlistwidth` and
`chatinputsize_chatchannel_local`. There is no mapping table to build and none to
get wrong — strip the `_userlistwidth` suffix or the `chatinputsize_` prefix and
what remains is the id already in `layout.windows`.

`chatCondensedUserList_<ch>` (Bool) is deliberately **not** read. It changes how
the member list renders, not how wide it is, and its key naming is inconsistent
in the corpus — `chatCondensedUserList_corp` sits beside
`chatCondensedUserList_chatchannel_player_-78564080`, one with the window-id
prefix and one without. Decoration does not need it, and resolving that
inconsistency is not worth doing for a shading difference.

### 2.2 The overview window link

Confirmed against a real char file: overview window **0** is the window id
`overview`, window **N** is `overview_N` (`overview_1`, `overview_2` observed).
That is the same positional link `overview_tabs.rs` documents on
`add_overview_window` ("the char key is `overview_{idx}`") and enforces on
`remove_overview_window` (last-window-only, because middle removal would be a
re-key cascade). So the canvas id resolves to `OverviewColumns.windows[N]`
directly, and `windows[N].tab_indices` names the tabs that window holds.

There is **no stored active tab** — nothing in either file records which overview
tab is selected (`tabgroups` is chat-window state, not overview). The detail
layer therefore draws the columns of the **first** tab in the window, and names
every tab in a strip above them so the choice is visible rather than silent.

### 2.3 What has not been measured

Overview and chat internals have had no measuring pass — the ledger entry says so
and it is still true. A handful of numbers are therefore invented (§4.2, one
table) and corrected by the live smoke exactly the way `HUD_NOMINAL`'s were on
2026-07-28.

The **column widths themselves are not invented** — they are the stored values,
in screen pixels, the same unit as the window rect. That is what makes this
worth building: drawing real widths inside the real rect makes an overflowing
column set visible as columns running off the edge, with no overflow arithmetic
anywhere.

## 3. Backend: `crates/settings-model/src/chat.rs`

One new module, read-only, no authoring. ~90 lines with tests.

```rust
pub struct ChatPanel {
    pub window_id: String,
    pub userlist_width: Option<i64>,
    pub input_height: Option<i64>,
}

pub fn chat_panels(user_doc: &Value) -> Vec<ChatPanel>;
```

Structured like `hud.rs`: all EVE format knowledge in the module, nothing
mutates. **The read path threads `SharedTable`/`effective` end to end** — both
the section-key comparison and each entry key. This is not optional: the account
file's section keys are `Ref`-keyed (`format-notes.md`: "A section lookup that
compares a bare `Value::Bytes` … misses it entirely and projects nothing from
real account files"), and the states/colours slice already shipped that bug once.
`hud.rs::section` is the pattern to copy.

Entries are collected by scanning the `ui` section once and matching the two
patterns, keyed into a map by window id, so a channel with only one of the two
keys still produces a panel with the other field `None`.

Sorted by `window_id` before returning, so the projection is deterministic
regardless of dict order — the frontend looks up by id, but a stable order makes
the Rust tests assert on a fixed vector.

`ops.rs` gains `chat_panels() -> Vec<ChatPanel>`, locking the user slot only. A
missing account file returns an empty vec rather than an error: an unpaired
character is normal, and the frontend already treats these projections as a
bonus (§6).

**Rejected: adding these to `hud.rs`.** `Hud` is a flat `Vec<HudEntry>` of six
*named* scalars looked up by a fixed name. The chat keys are a dynamic set whose
membership depends on which channels the player has open, with no name known
ahead of time. It would be a second, differently-shaped list wearing the first
one's type.

## 4. Frontend

### 4.1 One part type

```ts
export interface DetailPart {
  kind: "ring" | "slot" | "cell" | "band" | "column" | "button";
  /** Data px, RELATIVE TO THE PARENT RECT'S ORIGIN. */
  x: number; y: number; w: number; h: number;
  label?: string;
}
```

Data px, like `FurnitureRect` and `WindowRect.geom`, so the canvas reuses
`toCanvas`/`canvasScale` unchanged and every part scales with the canvas for
free. Relative to the parent, because the parts render as children of the
rectangle's own absolutely-positioned div — which already owns the position, the
scale and the clipping.

### 4.2 Pure functions: new `app/src/lib/detail.ts`

A new file, not more of `layout.ts`. `layout.ts` is 659 lines and owns the
drag/snap/hit-test maths this layer must never touch; keeping decoration in its
own module is the cheapest way to keep the ledger's "it must not reach
`hudRects`, the snap lines, or any drag" true by construction rather than by
discipline.

| function | output |
|---|---|
| `shipHudParts(): DetailPart[]` | ring + 3 rows × 8 slots, from §2's table |
| `fighterParts(): DetailPart[]` | 3 × 5 ability grid + 5 squadron cells |
| `neocomParts(bar, w, h): DetailPart[]` | one `w`-square per real button, top-down |
| `overviewParts(cols, windowIndex, rect): DetailPart[]` | tab strip + column header band |
| `chatParts(panel, rect): DetailPart[]` | member panel + input band |
| `windowDetail(unit, cols, chats): DetailPart[]` | id → family dispatch |

Details that are decisions rather than transcription:

- **`neocomParts`** reserves the first cell for the EVE menu button, which is not
  in `neocomButtonRawData`, and stops emitting when the next square would pass
  the bar's height — a bar taller than the screen is drawn truncated, which is
  what EVE does.
- **`overviewParts`** emits only `visible` columns, in `cols.tabs[t].columns`
  order, each at its stored `width`; a column whose width is `null` (absent =
  EVE's own default) falls back to `DETAIL_NOMINAL.columnWidth`. Bands are laid
  out left to right from `x: 0` with no clamping — running past `rect.w` is the
  signal, not an error.
- **`chatParts`** right-anchors the member panel (`x = rect.w - userlist_width`)
  and bottom-anchors the input band (`y = rect.h - input_height`), matching where
  EVE draws them. Either field being `None` omits that part rather than guessing
  a default: a channel with no stored width is one the player has never resized,
  and inventing one would draw a split that is not there.
- **`windowDetail`** resolves the family from the id: `overview` → index 0,
  `overview_N` → index N, `chatchannel_*` → its panel, anything else → `[]`. For a
  **stack** it resolves from the tab carrying `selectedId`, falling back to tab 0
  — a chat stack is the common case (`ChatWindowStack`), and the whole point of a
  tab strip is that the selected tab is what you are looking at. It is a pure
  function so that resolution is unit-tested rather than inlined in markup.

**Invented numbers** live in one `DETAIL_NOMINAL` table carrying a `ponytail:`
comment naming the ceiling and the upgrade path (a measuring pass like the
2026-07-28 one), exactly as `HUD_NOMINAL` did before it was measured:

| name | why invented |
|---|---|
| `slot` cell size | pitch is measured (50 × ~46), the cell drawn inside it is not |
| `abilityCell` / `squadCell` size | same — pitch 86 measured, cell not |
| `neocomTop` | the EVE-menu cell height at the top of the bar |
| `tabStrip` / `headerBand` height | overview chrome, never measured |
| `columnWidth` | fallback for a column with no stored width |

Every *measured* constant cites its `format-notes.md` row in a comment, so a
future correction can tell the two kinds apart at a glance. This is the
distinction that mattered when `HUD_NOMINAL`'s invented `686×250` was drawing the
ship HUD 195px off.

### 4.3 Rendering: new `DetailParts.svelte`

Props `{ parts, scale }`. Renders one absolutely-positioned div per part inside a
container with `pointer-events: none`, so no part can swallow a drag on the
rectangle it decorates — the guarantee the ledger asks for, in one CSS
declaration. `.ring` is a circle via `border-radius: 50%`.

A part's `label` renders only when `w * scale > 28`; below that it is dropped
rather than ellipsised, because a column of "…"s is noise, not information.

Mounted twice in `LayoutView`: inside the existing `.furniture` div and inside
the existing `.win` div, both gated on the pref. A component rather than two
copies of an `{#each}` in markup that is already 150 lines of template.

`.tabs` gains `position: relative; z-index: 1` so a chat stack's tab strip stays
readable above its own detail parts — absolutely-positioned children otherwise
paint over static siblings.

### 4.4 The toggle

`LayoutPrefs` (`app/src-tauri/src/prefs.rs`) gains `pub detail: bool`. Every
struct there already carries `#[serde(default)]` — "the extensibility contract"
per its own doc comment — so preference files written by today's build load
unchanged, and vice versa. `prefs.svelte.ts` gains `detail()` and
`setDetail(on)`, the latter using the same chained `persist` write as
`setClutterOverride`.

The checkbox sits in the existing `.ref` line beside "reference 2560×1440",
where the other canvas-wide state (the filter counter, the override counter)
already lives. Per the dark-native-controls note it gets explicit dark styling.

One toggle, not per-element: three switches would be three prefs, three
conditions and three things to explain, for a layer whose whole purpose is to be
either on or off.

### 4.5 Loading

`load()` gains `api.overviewColumns()` and `api.chatPanels()` beside the existing
`hud` and `neocom` calls, with the same `.catch(() => null)` tolerance and for
the same reason already documented there: a character with no `overview`
container, or an account file opened on its own, must not take the canvas down
with it. Both are re-read on the same slot/token/pairing effect as the others —
a pairing that arrives while the view is open is what makes the chat panels
available at all.

## 5. Testing

**Rust (`chat.rs`)**, over synthetic trees with invented channel names (no
personal data, per the repo rule):

- both keys present for one channel → one panel with both fields;
- only one key present → a panel with the other field `None`;
- a `Shared` section key whose value is a `Ref` still projects — the
  states-slice regression, and the one that would make this empty on every real
  file;
- a malformed value (non-Int, wrong wrapper arity) is skipped, not panicked on,
  mirroring `windows.rs`'s malformed-tuple skip;
- a document with no `ui` section → empty vec.

**Frontend (`node --test`, zero-dep), new `detail.test.ts`:**

- `overviewParts` omits hidden columns and preserves stored order;
- band offsets are the running sum of stored widths, and a set wider than the
  rect produces a band starting past `rect.w` — the overflow signal, pinned;
- a `null` width falls back to `DETAIL_NOMINAL.columnWidth`;
- `chatParts` right-anchors the member panel and bottom-anchors the input band,
  and omits either when its value is absent;
- `neocomParts` truncates at the bar height and skips the top cell;
- `shipHudParts` / `fighterParts` emit the expected counts and every part lies
  within the `HUD_NOMINAL` box (the test that catches a transcription slip in
  the measured table);
- `windowDetail` resolves `overview` → 0, `overview_7` → 7, a chat id → its
  panel, an unknown id → `[]`, and a stack → its selected tab, not tab 0.

**Live smoke:** open a character with the toggle on beside the running client and
compare each element, then correct every entry in `DETAIL_NOMINAL` from what the
client actually shows and record the overview/chat metrics in
`format-notes.md` as measured values.

## 6. Non-goals

- **Any interaction.** No hit-testing, no snap lines, no drags from detail parts.
  Dragging a chat splitter or an overview column edge on the canvas would need a
  `chat.rs` setter, `set_overview_width` wired into the Layout view, new `Drag`
  variants and new hit-test exclusions — a slice of its own. Ledger it.
- **Editing chat splits at all.** This slice only exposes them.
- **`chatCondensedUserList_*`** — §2.1.
- **Badge internals.** Nothing is measured and there is nothing stored; the badge
  stays a 32×32 box.
- **Per-element toggles** — §4.4.
- **Changing what the canvas draws.** `hideClutter` still hides `chatchannel_*`;
  detail changes what is drawn *inside* a rectangle, never which rectangles
  exist.
