# Rescale layout to another resolution or UI scale: design

Status: approved 2026-11-21. Implementation plan to follow.

## Context

A user asked to convert their EVE UI layout to a smaller reference resolution. Today the editor can only flag windows saved at another resolution, plus a copy warning, and it cannot convert anything.

The decompiled client code shows how EVE handles a mismatch (`docs/format-notes.md`, "How the client applies a saved rect at a different resolution"):

- **Units.** Every window saves `(x, y, w, h, screenW, screenH)` in UI-scaled pixels: the resolution divided by the UI scale.
- **Positions on load.** The client repositions windows. Windows docked to an edge stay docked. Floating windows get a crude shift that depends on width, not position.
- **Sizes on load.** The client never scales sizes. It only clamps them to the desktop. On a smaller screen the layout piles up.

**Intended outcome:** in one action, the player gets the same layout proportionally smaller (or larger). EVE then loads it unchanged, because nothing is mismatched any more.

## Goals

- Convert the open character's whole layout to a target resolution and UI scale, in one undoable step, shown on the canvas before Save.
- Scale window **sizes and positions** proportionally. Sizes are not kept.
- Keep edge-docked windows docked, using the client's exact edge rule, so EVE does not treat them as floating afterwards.
- Move the pixel-stored HUD pieces with the layout.
- **Expose the same conversion through the MCP server**, so an AI client can do it on request. Same maths, same single undo step.

## Non-goals (v1)

- The copy flow. The mismatch warning stays a warning; it was already reworded on the corrective branch.
- Pixel sizes inside windows: overview column widths (`SortHeadersSizes`) and chat splits. The overview column-fit preview already covers re-fitting columns.
- Runtime-only minimum sizes. About 66 window classes change their minimum at runtime with `SetMinSize` (for example Selected Item and chat). We use their static floor, and the client's own clamp on load covers the rest.
- Using the size table for the canvas's resize handles. It's a natural follow-up, but out of scope here.

## Anti-goals

- **Turning docked windows into floating ones.** A right-docked window that ends up 1px off the edge is a regression the user will see in game.
- **Opening gaps between adjacent windows, or making them overlap,** because each window was rounded separately.
- **More than one undo step** for one rescale.

## Approaches considered

1. **Backend command (chosen).** `rescale_layout` in Rust computes every change and applies it inside `undo::group`, so it is one undo step. It is tested beside `windows.rs`, and MCP and the copy flow can reuse it.
2. **Frontend function in `layout.ts`.** This needs less backend code. But HUD fields go through `set_hud_value` one field per call, so they would become separate undo steps, and MCP would need a second copy of the maths. Rejected.
3. **A separate preview-overlay mode.** Rejected: the canvas already previews unsaved edits, and Undo reverses them.

## Decided

### Target input

- The dialog asks for a screen resolution (W×H) and a UI scale. It shows the result in EVE's units: "EVE lays this out on a 2048×1152 desktop". That size is `int(W / scale) × int(H / scale)`, truncated as the client's `UIRoot.UpdateSize` does.
- **Prefill** comes from this machine's `core_public__.yaml`:
  - The resolution is `device.FullScreenResolution` when `WindowMode` is 0, otherwise `WindowedResolution`.
  - The UI scale is the existing `client_env::scale_and_font` logic.
  - If the file is missing or has no resolution, prefill with the canvas reference size at 100%.
- The dialog also states the current source: "Saved for a 2560×1440 desktop". This is `reference_w/h`, in EVE's units, since the file does not record the source's scale.

### Transform (per window, from that window's own saved screenW/H)

Let `W, H` be the window's saved desktop and `W', H'` the target desktop, with `sx = W'/W` and `sy = H'/H`. Each axis is handled independently (horizontal shown; vertical is the same with `y, h, H`):

Each edge is computed on its own, and the size is right edge minus left edge:

| Edge | Rule |
|---|---|
| left edge with `x ∈ {0, 16}` (docked) | kept at `x` |
| right edge with `x + w ∈ {W, W−16}` (docked) | kept at the same inset: `W' − (W − (x+w))` |
| any other left edge | `round(x·sx)` |
| any other right edge | `round((x+w)·sx)` |

- **Edges are rounded, never sizes.** A docked window keeps its 0 or 16 inset unscaled, so EVE's exact edge test still matches. A floating neighbour's touching edge rounds from the same source value, so it stays touching. A window docked on both sides keeps both.
  - A first draft rounded the size of right-docked windows. That leaves a gap of `16·(1−sx)` beside 16px-inset docks, about 4px going 2560→1920.
- **Minimum and fixed sizes**, from the client's own class constants (see "What the client says about window sizes" below):
  - A **fixed** dimension (for example the fitting window's height) is never scaled. It keeps its stored value.
  - A scaled dimension below the window's **minimum** is raised to the minimum.
  - Either way, the window keeps its **docked edge**: a right-docked window grows leftwards, and any other window keeps its left edge and grows rightwards, the same direction the client grows it on load.
  - A window with no table entry is not floored. The client's clamp on load still applies.
- **Clamp, then accept overlap.** Afterwards, `w' ≤ W'` and `0 ≤ x' ≤ W' − w'`, the same as `UpdatePosition`. Then set `screenW/H` to `W', H'` on every rewritten tuple.
  - When a minimum makes windows overlap, they overlap. Overlap is accepted when there is no other choice.
  - The Sheet and the MCP reply list the windows held at their minimum or fixed size, so the player knows where to look.
- **Which windows.** Every window with a geometry, open or closed, so a window reopened later is right too. Windows saved at a stale resolution convert from their own `screenW/H`, which fixes them in the same pass. A window already at the target is left untouched.
- **Stacks.** The container and its members share one rect. Each member gets the same deterministic transform of that rect, so they stay identical.
- **Different aspect ratios** (for example 16:9 → 16:10) scale each axis separately. this keeps the tiling, so a window spanning the full height still spans it.

### What the client says about window sizes (decompiled 2026-11-21)

- **The base class.** `Window` sets `default_minSize = (256, 128)`, `default_maxSize = (None, None)`, and optional `default_fixedWidth` / `default_fixedHeight`. `InitializeSize` sets each dimension to `min(max, max(min, min(desktop, saved)))`, and a fixed dimension overrides the saved one.
- **Subclasses.** They override these as class constants, in UI-scaled pixels, the same units as the geometry:

  | Window | Minimum |
  |---|---|
  | overview | 128×128 |
  | chat | 120×120 |
  | drones | 240×120 |
  | inventory | 100×140 |
  | probe scanner | 300×150 |
  | d-scan | 300×124 |
  | journal | 465×300 |
  | mail | 500×350 |
  | market | 650×435 |
  | corporation | 700×832 |

  The fitting window has a **fixed height**. In total, 370 classes carry a window id: 58 have a fixed width and 41 a fixed height.
- **Stacks.** A stack's minimum is the largest minimum of its members (`stack.py GetMinWidth/GetMinHeight`).
- **The default trap.** `default_minSize` is evaluated once in the base class. A subclass that only changes `default_width` keeps 256×128 as its minimum.
- **Consequence.** The small windows players tile (overview, chat, drones, scanners) rarely hit their minimum when downscaling. The big ones (market, corporation, mail) and fixed-height windows do. Those are exactly where clamping with the docked edge kept beats the client's grow-rightwards-then-clamp.

### Window size table (new generated data)

- **A dev tool, `tools/gen-window-sizes.py`.** It follows the `tools/gen-command-defaults.py` precedent: it reads `code.ccp` and walks every class deriving from `Window`, resolving inheritance and module constants such as `overviewConst.WINDOW_ID`, `invConst.*` and `MIN_WINDOW_WIDTH`. It writes `window id → {min_w, min_h, fixed_w?, fixed_h?}` to a committed data file under `crates/settings-model`, embedded at build time.
- **Parameterized ids** map by prefix to their class: `chatchannel_*` to the chat window, and the inventory container tuples to their inventory class.
- **Classes it can't resolve statically** are left out rather than guessed.
- **Shipping.** The table is app-owned built-in data, refreshed with a release. No real game data is involved, only CCP's code constants.

### HUD pieces (only values stored in the file; absent values are already relative to the resolution)

| Field (`hud.rs`) | Rule |
|---|---|
| `ship_offset` (px from the screen centre) | `× sx`, kept as a float |
| `fighter_x/y` (absolute px) | `round(× sx)`, `round(× sy)`, clamped like `fighterPoint` |
| `badge_x/y` (absolute px) | `round(× sx)`, `round(× sy)` |
| `target_x/y` (fractions), `neocom_width`, flags | unchanged |

The HUD keeps its own screen size in the client, so the source for HUD values is `reference_w/h`.

### Shape of the code

- **A pure function in `crates/settings-model`** (beside `windows.rs`, e.g. `rescale.rs`). Given the `WindowLayout`, the HUD values and the target, it returns the `Mutation` list plus the HUD writes. It reuses the `Geom` paths `x_path`… `screen_h_path` that `geomMutations` already uses.
- **`ops::rescale_layout(state, target_w, target_h)`** (`app/src-tauri/src/ops.rs`):
  - It takes `undo::group`, then calls `ops::apply_mutations(Slot::Char, …)` and `ops::set_hud_field` per HUD write. The group is re-entrant (`undo.rs:454`), so the result is one undo step.
  - On error it calls `rollback_group`, the same pattern as MCP's `batch` (`mcp.rs:2279`).
- **Tauri commands** (`lib.rs`, mirrored in `api.ts`):
  - `rescale_layout {width, height, ui_scale}` takes the screen resolution and returns `{desktop_w, desktop_h, windows_changed}`.
  - Rust's `desktop_size(res, scale)` is the authority on `int(res / scale)`. the Sheet keeps a one-line TS copy for its live "EVE lays this out on…" text, pinned by a spec case at 125% that matches the Rust test's numbers.
  - `client_display` → `{resolution: [w,h] | null, ui_scale}`, added to `client_env.rs` beside `scale_and_font`.
- **MCP: a `rescale` op in `layout_edit`** (`app/src-tauri/src/mcp.rs`).
  - Add it to the `op_item` list (`:963`) and its description string.
  - Arguments: `width`, `height` (the screen resolution) and an optional `ui_scale`. When `ui_scale` is omitted, it comes from `client_display`; when that is unavailable, 1.0.
  - The EVE desktop size is computed the same way as in the UI. Put the `int(res / scale)` helper in Rust and have the UI get it through the command result or one shared rule, so the two cannot drift.
  - Dispatch goes through `layout_op`, which calls `ops::rescale_layout`. Inside `batch`'s `undo::group`, the whole call is still one undo step.
  - The reply includes the resulting desktop size and the number of windows changed. `layout_op` returns `Result<(), Value>`, so these come back through a `RefCell` collector in the `layout_edit` arm, the same way `chat_leave` reports its extras (`mcp.rs:1344`).
  - `layout_get` already returns `reference_w/h`. Document in `mcp_primer.md`, and in `eve_guide`'s `layout` topic, that a rescale targets resolution ÷ UI scale, so an AI client does not pass raw desktop units by mistake.
  - Like every edit tool, nothing reaches disk until `save`.
- **UI.** A "Rescale layout…" action in the Layout view opens a Sheet with the resolution fields, a UI-scale select, the computed EVE size and the source size. Apply runs the command and refreshes the layout the same way other edits do. It is disabled when the view is read-only or there is no geometry.
- **CHANGELOG.** One entry under Unreleased: "Rescale a character's whole layout to another resolution or UI scale."

## Left to the builder

- The exact YAML shape of `WindowedResolution` and `FullScreenResolution`: read it from a real `core_public__.yaml` at runtime or in the decompiled `carbonui/services/device.py`. Never commit a real file.
- Which UI-scale values to offer: read them from the client's device service if listed, otherwise 90–200% in EVE's steps.
- Where the action sits in the Layout toolbar or menu, following the existing Phase 5 command registry (`commands.ts`), so it also appears in the Ctrl+K palette.

## Verification

- **Rust unit tests** for the pure function, using hand-built `Value` trees like `windows.rs`'s tests:
  - a left-docked (0 and 16) window, a right-docked (flush and 16 in) window, and a floating window;
  - two adjacent windows staying adjacent after an odd ratio (e.g. 2560→1920), including a floating window beside a 16px-inset right dock;
  - a window wider than the target being clamped;
  - a market window floored at 650 wide: right-docked, it grows leftwards and stays docked; floating, it keeps its left edge;
  - a fixed-height window keeping its height;
  - a stack floored at its largest member's minimum;
  - the table generator reproducing the numbers in the table above, run against a real `code.ccp` when one is found and skipped without it, like the font tests;
  - a stale-resolution window converting from its own size;
  - stack members ending identical;
  - each HUD field rule, and absent HUD values left absent;
  - a window already at the target producing no mutation.
- **An ops-level test** that a rescale is exactly one undo step, and that undoing it restores the original bytes. This follows `layout_edit_moves_a_window_and_restamps_a_mismatched_resolution` (`mcp.rs:3843`).
- **An MCP test:** `layout_edit` with `{op: "rescale", width: 1920, height: 1080, ui_scale: 1.25}` on `open_char(&layout_char_bytes())` gives a 1536×864 desktop on every rect, `undo_state.depth == 1`, and the same mutations as the Tauri command.
- **A frontend spec** for the Sheet: prefill, the computed EVE size at 125%, and Apply calling `rescale_layout` with the computed size.
- `npm test` (judged by its exit code), `cargo test`, and `svelte-check`.
- **Live:** rescale a logged-out test character to 1920×1080, launch EVE at that resolution, and check that docked windows are still docked and nothing piles up.

