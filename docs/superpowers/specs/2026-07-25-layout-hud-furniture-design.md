# Layout depth — HUD furniture: ship HUD, fighter UI, neocom (design)

Status: designed 2026-07-25, not yet planned.

Milestone context: this is the **layout depth** milestone, cut into four slices
(see §7). This spec covers the **HUD furniture** slice, which the developer
chose to build first — the canvas/list usability and stack-polish slices are
scoped in §7 but not designed here, and the neocom *button* editor is a
separate slice of its own.

## 1. Goal

The layout canvas draws windows and window stacks, but nothing else. The screen
furniture a player actually arranges their windows *around* — the ship HUD
(capacitor ring plus module racks), the detached fighter UI, and the neocom — is
stored in the settings files and is invisible to the editor.

This slice projects those anchors, draws them on the canvas as non-window
furniture, and makes the movable ones draggable, so the canvas shows the whole
screen instead of two thirds of it. It also adds `pinnedWindows`, a per-window
flag the panel has always omitted.

## 2. The HUD model (confirmed from the corpus)

Verified against the 2026-07-22 corpus snapshot (384 character files, 175
account files). Counts below are files containing the key.

### 2.1 Character-scoped keys (`core_char_<id>.dat`)

All are ordinary `(timestamp, value)` leaves, so each needs the usual
value-wrapper unwrap (docs/format-notes.md, "Value-wrapper convention").

| what | path | value | present |
|---|---|---|---|
| Ship HUD horizontal offset | root → `b"windows"` → `b"shipuialignleftoffset"` | Float, e.g. `-189.0`, `-173.0` | 315/384 |
| Fighter UI position | root → `b"ui"` → `b"fightersDetachedPosition"` | Int 2-tuple, e.g. `(326, 54)` | 319/384 |
| Notification badge position | root → `b"ui"` → `b"notification_badge_offset"` | Int 2-tuple, e.g. `(2519, 131)` | 313/384 |
| Per-window pinned flag | root → `b"windows"` → `b"pinnedWindows"` | `(ts, {window_id: bool})` | 352/384 |

`shipuialignleftoffset` is a small **negative float on both a 1920-wide and a
2560-wide client**, which reads as an offset from the screen centre rather than
an absolute left edge. It is the only float in the layout surface — the value
kind matters for the write path (`NewValue::Float`).

`pinnedWindows` has exactly the shape of the seven flags already in
`windows.rs::BOOL_FLAGS` (`(ts, {window_id: bool})`, values bool). Its presence
count is identical in the oldest (2026-07-12) and newest (2026-07-22) corpus
snapshots, so it is a stable key, not one being phased in or out.

### 2.2 Account-scoped keys (`core_user_<id>.dat`)

| what | section | key | value | present |
|---|---|---|---|---|
| Ship HUD aligned to top | `ui` | `shipuialigntop` | Bool | 131/175 |
| Fighter UI detached | `ui` | `detachFighterUI` | Bool | 130/175 |
| Fighter UI shown | `ui` | `displayFighterUI` | Bool | 131/175 |
| Neocom thickness | `windows` | `neocomWidth` | Int, e.g. `37` | 130/175 |

Corpus-verified, not assumed: on the sampled real account file, `neocomWidth`
sits directly under the root `windows` section (which holds only that one key
there), and the other three sit under the root `ui` section — itself keyed by
a `Ref` whose byte-string definition appears later in the stream (the trailing
shared-object table makes that legal).

These are account-wide: changing one affects every character on the account.
The existing shared-account banner (`overview.ts` `sharedWith`) already names
the sibling characters, and its copy covers this case unchanged.

Deliberately **not** exposed, because none of them affects placement:
`hudButtonsExpanded`, `offsetUIwithCamera`, `selected_shipuicateg`,
`neocomSizeLocked`.

### 2.3 What the files do *not* store — the two assumptions

The files store anchors, never sizes, and never say what the anchor is relative
to. Two conventions are therefore assumed, drawn from corpus evidence, and
**corrected in this slice's live smoke** (the same way the states slice settled
the id-36/37 question):

1. **`shipuialignleftoffset` is centre-relative**, positive to the right. Basis:
   small negative values on clients of two different widths. Alternative: an
   absolute left-edge coordinate, which those magnitudes rule out.
2. **The two point tuples are top-left corners** in absolute client pixels.
   Basis: `(326, 54)` on a 1920×1080 client sits plausibly near the top-left.
   Counter-evidence worth naming: one character reads `(1280, 660)` on a
   2560×1440 client, where x is *exactly* half the screen width — consistent
   with a centre-anchored panel. The smoke settles it.

Nominal on-screen sizes are likewise invented (§4.1) and corrected the same way.
Both live in one table with a `ponytail:` comment naming the ceiling; nothing
else in the slice depends on their exact values, so a correction is a one-line
edit.

### 2.4 The absent-key case

A player who has never moved the ship HUD has no `shipuialignleftoffset` key at
all (69/384 files). Absent means "EVE's built-in default", so the projection
reports the field as absent with its default, the canvas draws furniture at that
default, and the first edit **mints** the key. Because these leaves are
`(timestamp, value)` tuples directly under a section, minting cannot be done
with one generic `insert_dict_entry` — hence the authoring op in §3.2. A
zero-timestamp mint is already proven on real files (the overview presets
container, slice 2b).

Assumed defaults, also smoke-confirmed: offset `0.0`, `shipuialigntop` false,
`detachFighterUI`/`displayFighterUI` false, `neocomWidth` 37 (the corpus mode).

## 3. Backend

### 3.1 New module: `crates/settings-model/src/hud.rs`

A read-only projection, structured exactly like `windows.rs`: all EVE format
knowledge lives here, every writable field carries the resolved `NodePath`, and
nothing in the module mutates.

The **read path threads `SharedTable`/`effective` end to end** — key comparison
and value reads both. This is not optional defensiveness: the states/colours
slice shipped a read path that matched bare `Bytes` only, and because real files
`Shared`/`Ref` their repeated keys, the whole projection came back empty on
every real file. `windows.rs` already does this correctly (`collect_shared` once,
then `effective(key, &shared)` at each comparison) and is the pattern to copy.

```rust
pub struct Hud { pub entries: Vec<HudEntry> }

pub struct HudEntry {
    pub name: String,          // "ship_offset", "fighter_x", "neocom_width", …
    pub kind: HudKind,         // Float | Int | Bool
    pub value: Option<String>, // None = absent, use `default`
    pub default: String,
    pub scope: HudScope,       // Char | Account — a local enum, not ops::Slot
    pub set: SetTarget,        // reused from windows.rs
}
```

`HudScope` is declared in `hud.rs`: `ops::Slot` lives in the Tauri crate above
`settings-model`, which stays dependency-free, so `ops.rs` maps scope → slot at
the boundary.

One flat `Vec<HudEntry>` looked up by name, not a struct per element: it mirrors
the existing `flags: Vec<BoolFlag>` on a window, keeps one code path for six
scalars across two files, and lets the frontend table drive the panel. The two
point tuples contribute two entries each (`fighter_x`/`fighter_y`,
`badge_x`/`badge_y`), each with its own tuple-element `NodePath` — identical to
how `Geom` already addresses the elements of the geometry 6-tuple.

Signature: `pub fn hud(char_doc: &Value, user_doc: Option<&Value>) -> Hud`. A
missing account file is normal (an unpaired character), so `user_doc` is an
`Option` and its four entries then report `SetTarget::Unavailable`.

### 3.2 Authoring: `set_hud_value`

One function, `pub fn set_hud_value(doc: &mut Value, name: &str, text: &str) ->
Result<(), HudError>`, living in the same module (the read/author split of
`overview.rs` vs `overview_presets.rs` is not worth reproducing for one setter):

- **Key present** → build the same `Mutation::SetScalar` the panel would and run
  it through `mutate::apply`. No reshare: a scalar overwrite is not a structural
  edit, exactly like the geometry drag path.
- **Key absent** → inline-first (house rule for structural edits), insert
  `Value::Tuple([Long(0), value])` under the section dict, then `reshare`.

`text` is parsed per the entry's `HudKind`, reusing `NewValue`'s existing
parse-and-validate paths (`NewValue::Float` already exists and is tested).

### 3.3 `ops.rs`

Two commands, following the shape the stack and overview-window ops already use
(no slot argument; they know which documents they need, and they return the
fresh projection so the caller never re-derives paths):

- `hud_layout() -> Hud` — locks both slots, projects char + optional user.
- `set_hud_value(name, text) -> Hud` — maps the entry's `HudScope` to the char or
  user slot, marks that slot dirty, returns the fresh projection.

This deliberately avoids teaching `+page.svelte`'s `runMutations` to target a
non-active slot. `runMutations` writes `slots[active]` today; the account-side
HUD fields are the first edit from the Layout view that has to land in the
account file, and a returned-projection op keeps that knowledge in the backend
where the rest of the cross-file editing already lives.

### 3.4 `pinnedWindows`

Add `"pinnedWindows"` to `windows.rs::BOOL_FLAGS` and widen the array to 8. The
per-window flag machinery, the panel checkbox row, the insert-when-absent path
and the reveal-in-tree right-click all then cover it with no further change.

### 3.5 Approaches considered and rejected

- **Frontend-built mutations instead of `set_hud_value`.** Rejected: minting an
  absent `(timestamp, value)` leaf would take three chained generic mutations
  (empty tuple, then two inserts), and it would need `runMutations` slot
  plumbing (§3.3). The op is smaller than either half of that.
- **Extending `WindowLayout` with a `hud` field.** Rejected: `window_layout`
  takes one slot, and half these values live in the other file.
- **Furniture as synthetic `WindowRect`s.** Rejected: furniture has no
  `(x, y, w, h, screenW, screenH)` tuple, no flags, no stack membership, and
  must not be resizable — it would be an `Option` in every field of `Geom` and
  a special case in every consumer.
- **A capture experiment before drawing** (the route the stacks and states
  slices took). Rejected by the developer for this slice: the assumptions in
  §2.3 are cosmetic-only, and the live smoke that ends every slice already
  puts the app and the client side by side.

## 4. Frontend

### 4.1 Placement maths: `layout.ts`

One pure function, unit-tested, so no geometry logic lives in the component:

```ts
export function hudRects(hud: Hud, layout: WindowLayout): FurnitureRect[]
```

`FurnitureRect` is `{ kind, label, x, y, w, h, drag: "none" | "x" | "xy" }` in
**data pixels** — the canvas then reuses the existing `toCanvas`/`canvasScale`
helpers unchanged, so furniture scales with the canvas for free.

Nominal sizes and the anchor conventions live in one `HUD_NOMINAL` table in this
file with the `ponytail:` comment from §2.3:

- **Neocom** — `x: 0, y: 0, w: neocomWidth, h: reference_h`; `drag: "none"`
  (width is a numeric field; dragging a screen-edge bar's inner edge is fiddly
  for a value the player rarely changes).
- **Ship HUD** — nominal 686×250, `x = reference_w / 2 + offset - w / 2`,
  bottom-anchored, or top-anchored when `shipuialigntop`; `drag: "x"`.
- **Fighter UI** — nominal 400×120 at the stored point; `drag: "xy"`. Emitted
  only when `detachFighterUI && displayFighterUI` (an attached or hidden fighter
  UI is not on screen as a separate element).
- **Notification badge** — nominal 32×32 at the stored point; `drag: "xy"`.

### 4.2 Canvas furniture: `LayoutView.svelte`

Furniture renders before the window rects, so windows always draw on top of it,
styled dashed and muted with no resize handles and a small label. Non-draggable
furniture gets `pointer-events: none` so it can never swallow a window drag.

Dragging **extends the existing `Drag` union** with a third variant rather than
running a second drag machine:

```ts
| { kind: "furniture"; f: FurnitureRect; startX: number; startY: number; ox: number; oy: number }
```

It reuses the same canvas pointer capture, the same `toData` delta conversion,
and the same "origin from the displayed rect, not the committed value" rule that
keeps a re-drag from jumping. On drop it calls `api.setHudValue` once per changed
axis (at most two calls) — for the ship HUD, converting the dragged rect centre
back to a centre-relative offset. `drag: "x"` clamps dy to zero at the source,
so the vertical axis simply cannot be written.

### 4.3 Fields: new `HudPanel.svelte`

A compact block above the window list in the right-hand column: a labelled
numeric input per scalar and a checkbox per bool, grouped Ship HUD / Fighter UI
/ Neocom / Notification badge, with the account-scoped rows marked as such
(they change every character on the account). Rows whose `SetTarget` is
`Unavailable` — the no-account-file case — are disabled with the panel's
existing "Not present in this file" tooltip.

A new component rather than more rows in `WindowPanel.svelte`, which is already
437 lines and has one job (the window list). Both mount inside `LayoutView`'s
existing right column; no grid change.

Per the dark-native-controls note, every new `input` gets explicit dark
background and colour.

## 5. Testing

**Rust (`hud.rs`)**, over synthetic trees with invented ids and coordinates (no
personal data, per the repo rule):

- each field present, absent, and — the states-slice regression — with a
  `Shared` key whose value is a `Ref`, asserting the projection still resolves;
- `user_doc: None` → the four account entries report `Unavailable`;
- a malformed point tuple (wrong arity, non-Int element) reports absent rather
  than panicking, mirroring `windows.rs`'s malformed-tuple skip;
- `set_hud_value` round trip: set an existing key, re-project, value changed;
  mint an absent key, re-project, value present with a zero timestamp;
- float formatting survives a round trip (`-189.0` stays a Float, not an Int).

**Frontend (`node --test`, zero-dep):** `hudRects` cases in `layout.test.ts` —
centre-relative offset ↔ canvas x in both directions, top vs bottom anchoring,
point tuple ↔ rect, neocom bar spans the reference height, fighter furniture
omitted when not detached or not shown, absent value falls back to the default.

**Live smoke** (also the slice's assumption gate): set each value in the app,
save, launch EVE, and confirm each element lands where the canvas drew it — then
correct the two §2.3 conventions and the nominal sizes from what the client
actually shows, and record the confirmed conventions in
`docs/format-notes.md` as a new "HUD anchors" section.

## 6. Non-goals

- **`dockPanels`** — a second positioning system (map, skill planner, …) stored
  as proportional 0–1 coordinates with align and width/height proportions,
  present in 370/384 char files. Different coordinate space, its own projection;
  ledger it.
- **The neocom button editor** — `ui.neocomButtonRawData`, a list of
  `utillib.KeyVal` instances with `children` folder groups. Its own slice (§7).
- **Batch-copying HUD anchors** character to character. The anchors are
  resolution-dependent and the batch aspects are already coarse; revisit after
  the resolution-rescale question is answered.
- **Resizing furniture.** EVE stores no sizes; there is nothing to write.
- Snap-to-grid and edge snapping, including snapping windows *to* furniture —
  that is the usability slice (§7), and it should land after furniture exists so
  it can snap to it.

## 7. The layout depth milestone

Four slices, each its own spec, plan, PR and release, matching how overview
depth was cut. The developer picked slice order: **HUD furniture first** (this
spec); the rest are scoped, not designed.

1. **Canvas & list usability.** Friendly window labels instead of raw ids
   (`overview_1`, `"('corpassets', 1037014587783L)"`), a filter box on the window
   list, and grouping or folding of the chat-window noise — a real character file
   carries ~200 windows, most of them `chatchannel_*` and `ChannelSettingsDlg_*`
   entries. Plus a right-click context menu replacing the direct
   jump-to-tree (the M2 spec's deferred item), and precision editing: snap-to-grid
   with a hold-to-disable modifier, arrow-key nudge, **and snapping to canvas
   edges and to other windows' edges** (the developer's choice over plain grid
   snap). Absorbs the ledger's "add a search/filter to the window list" item.
2. **Stack polish.** Canvas drag-to-stack (deferred from window stacks V1), drag
   to reorder stack tabs, and the ledger's friendlier `container_label`.
3. **HUD furniture** — this spec.
4. **Neocom button editor.** Reorder, add and remove neocom buttons via
   `ui.neocomButtonRawData`, including the `children` folder groups, with
   `ui.neocomButtonRawDataOriginal` as the reset baseline. Authoring `KeyVal`
   instances is unlike anything the editor does today and `btnType`/`children`
   semantics are unmapped, so this slice needs its own capture experiment.
