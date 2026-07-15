# M2 — Layout canvas + properties panel (design)

Date: 2026-07-15
Status: approved, pre-plan
Builds on: M1 (codec + raw tree editor + save chain), design spec §6 "Layout canvas".

## 1. Goal

A visual editor for EVE window geometry: a scaled mock of the game screen with
one draggable/resizable rectangle per open window, and a per-window detail view
for exact values and stored flags. Everything edits the same in-memory document
the raw tree editor edits and saves through the same M1 save chain.

## 2. Data source (from `docs/format-notes.md`)

Character files (`core_char_<id>.dat`) only; user files have no geometry.

- Geometry: root → `b"windows"` → `b"windowSizesAndPositions_1"` →
  `(timestamp, dict)`; the inner dict maps window id → 6-tuple
  `(x, y, width, height, screenW, screenH)`, all absolute pixels. `screenW/H`
  is the client resolution the geometry was saved at, embedded **per window**
  — there is no global resolution key.
- Window ids: plain byte-strings for singletons (`b"overview"`, `b"fitting"`)
  and stringified Python tuples for parameterized windows
  (`"('corpassets', <synthetic-id>)"`). Both are keys of the same dict.
- Flags: sibling `(timestamp, dict-by-window-id)` entries under `b"windows"`:
  `openWindows`, `collapsedWindows`, `minimizedWindows`, `lockedWindows`,
  `compactWindows`, `isOverlayedWindows`, `isLightBackgroundWindows` (bool),
  and `stacksWindows` (stack id). These are the WindowLayout flag fields.

## 3. Scope

In scope for M2:

- Canvas draws only **open** windows (`openWindows == true`), scaled to a
  reference resolution.
- A window panel lists **all** windows; each row toggles the window's `open`
  state (adds/removes it from the canvas).
- Selecting a window expands inline detail: `x/y/w/h` number inputs, toggles
  for the seven boolean flags, and a value input for `stacksWindows` (a stack
  id, not on/off).
- Drag to move, handles to resize; canvas and detail share one working model.
- Defer: snap-to-grid (free-drag only in M2; number inputs give exact
  placement). A background game-screen image is not part of M2 — plain scaled
  rectangles on a neutral canvas.

## 4. Architecture — Rust read projection, existing write path

Chosen over (A) deriving windows client-side from the tree, and (B) a full
typed read+write windows subsystem. Rationale: format knowledge already lives
in `settings-model`; there must remain exactly one write path (`mutate.rs`) and
one save chain. So the canvas is a thin renderer over a read-only projection,
and every edit produces the same `Mutation`s the tree editor already produces.

### 4.1 `window_layout()` command (new, read-only)

Lives beside `crates/settings-model/src/projection.rs`, exposed as an `ops.rs`
command mirrored in `app/src/lib/api.ts`. Walks the `Value` tree once and
returns a typed model. It **only resolves paths** — never mutates.

Per top level:

- `reference`: `(screenW, screenH)` that the most open windows agree on (the
  mode). Used as the canvas bounds. If no window is open, fall back to the mode
  across all windows; the canvas is then simply empty.
- `windows`: list, each:
  - `id`: decoded window id string.
  - `label`: human display; falls back to the raw id for unrecognized windows.
  - `x, y, w, h, screen_w, screen_h`: current integer values.
  - `resolution_matches_reference`: bool (drives the mismatch badge).
  - For **each writable field** (the six geometry elements and each flag), a
    ready-to-use mutation target: the resolved `NodePath` for a `set_scalar`,
    or, when a flag has no entry yet for this window, the `insert_dict_entry`
    params (parent path + key). The frontend picks a pre-built mutation and
    never constructs a path from byte-string keys or tuple indices.
  - `renderable`: false when the tuple is not six ints; such a window is listed
    but flagged, editable only via the raw tree.

Malformed or missing dicts are skipped by the projection, never panic.

### 4.2 Writes reuse `apply_mutation`

- Move/resize/number-input → `set_scalar` on the geometry element path(s).
- Flag toggle → `set_scalar` on the flag's entry path, or `insert_dict_entry`
  when the projection reported the flag absent.
- No new command writes to the document; the M1 dirty-tracking, backup, verify,
  and atomic-write chain apply unchanged.

## 5. Frontend

`+page.svelte` gains a **Tree / Layout** view switch, shown only when
`window_layout()` returns windows (char files). Two new components in `src/lib`:

- **`LayoutView.svelte`** — orchestrates. Fetches `window_layout()`, holds the
  working model and the current selection, lays out canvas + window panel.
- **`WindowPanel.svelte`** — master-detail accordion. Each row: the open/closed
  checkbox (`openWindows`), the window name, a resolution-mismatch badge. The
  selected row expands inline to `x/y/w/h` inputs, the seven boolean flag
  toggles, and the `stacksWindows` value input. "Expanded" is the selection,
  shared with the canvas.

### 5.1 Interaction & edit flow

- Working model seeded from `window_layout()` on view entry; re-seeded after
  save/restore or a file switch. Canvas rectangles and the expanded row bind to
  the same working-model entry.
- Scale: `scale = containerWidth / referenceWidth`, aspect preserved. Each open
  window is an absolutely-positioned rectangle at `x·scale, y·scale, w·scale,
  h·scale`. Negative/off-screen coords are drawn as-is (overflow visible),
  never clamped.
- Drag/resize: update the working model live (no backend call per mousemove);
  commit changed fields on mouseup. If the window's saved resolution differed
  from the reference, the commit also writes `screenW/screenH` to the reference
  (the new coords are in that space).
- Number input: live-updates the working model; commits on blur/Enter.
- Flag toggle: commits immediately.
- On commit success the working model already holds the new value, so no
  refetch churn; Rust document and working model move in lockstep.

## 6. Error handling & edge cases

- No geometry (user files, or char file missing `windowSizesAndPositions_1`):
  no Layout tab; the file shows only the tree.
- Read-only file (parse-failed or non-editable fidelity): canvas renders but
  drag/resize/inputs are disabled, matching M1's read-only tree.
- Malformed window (tuple not six ints): listed as unrenderable, editable via
  the raw tree only; the rest of the canvas is unaffected.
- Resolution mismatch: mode wins for canvas bounds; disagreeing windows badged.
  No silent coordinate mixing.
- Values: `x/y` accept negatives; `w/h` constrained to ≥ 0. Type validation is
  enforced by `mutate.rs`.

## 7. Testing

- **Rust (`settings-model`):** unit tests for the projection over a synthetic
  Value tree (invented ids/coords — no personal data, per repo rule): window
  extraction, per-field NodePath resolution, mode-resolution selection,
  malformed-tuple skipping. One round-trip test: a returned geometry path fed to
  `mutate::apply` changes the intended element.
- **Frontend (`node --test`, zero-dep per convention):** pure logic only — scale
  math (data px ↔ canvas px round-trips), reference-resolution selection,
  open-window filtering. DOM drag is not unit-tested, consistent with M1.
- **Manual smoke:** open a real char file, move/resize/toggle on the canvas,
  save, reopen, confirm the raw tree reflects the change. No new live in-game
  gate — M1 proved the save chain; this checks canvas→document consistency.

## 8. Out of scope / deferred

- Snap-to-grid.
- Background game-screen reference image.
- Overview columns and autofill editors (M3).
- Anything in user files (no geometry there).
