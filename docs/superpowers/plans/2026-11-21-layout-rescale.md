# Layout Rescale Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert a character's whole window layout and pixel-stored HUD positions to another screen resolution and UI scale, in one undo step, from the Layout view and from MCP.

**Architecture:**
- A pure transform in `crates/settings-model` (`rescale.rs`) turns the projected `WindowLayout` + `Hud` + a target desktop into `Mutation`s and HUD writes.
- It is fed by a generated table of the client's per-window minimum and fixed sizes (`window_sizes.rs` + JSON).
- `ops::rescale_layout` applies the result inside `undo::group`. The Tauri command, the MCP `layout_edit` op and the Layout view's Sheet all call that one function.

**Tech Stack:** Rust (settings-model crate, Tauri 2 backend), Svelte 5 + TypeScript (vitest), Python 3 (dev-time generator over the client's `code.ccp`).

**Spec:** `docs/superpowers/specs/2026-11-21-layout-rescale-design.md`

## Global Constraints

- **Geometry units.** All geometry is in UI-scaled px. Desktop = `int(res_w / ui_scale) × int(res_h / ui_scale)`, truncated.
- **Docked edges are exact.** A left/top edge is docked when it is at 0 or 16. A right/bottom edge is docked when it is at `W`/`H` or `W−16`/`H−16`. A docked edge keeps its unscaled inset.
- **Rounding.** Edges are rounded, never sizes. Size = right edge − left edge.
- **Fixed dimensions** are never scaled.
- **Minimums.** A dimension below its minimum is raised to it. The docked edge is kept: right/bottom-docked windows grow leftwards/upwards, all others keep their left/top edge. Overlap is accepted.
- **One rescale is exactly one undo step.** Nothing reaches disk until Save.
- **Synthetic data only** in tests, fixtures and docs: ids like `90000001`, no real character or profile names (CLAUDE.md). The generated table holds only CCP code constants.
- **No AI attribution** in commits. Identity is StormDelay (CLAUDE.md, enforced by `.githooks`).
- **Verify `npm test` by its exit code**, not its summary line.

## Review Focus

1. **A layout with no usable geometry** (`reference_w/h` = 0, or a window with `screen_w`/`screen_h` 0) must not divide by zero. It must be refused with a clear error, or the window skipped. Tests: Task 2 `zero_resolution_window_is_skipped`, Task 3 `rescale_without_geometry_is_refused`.
2. **A target smaller than a window's minimum.** Example: corporation, min height 832, on a 768-high desktop. The client's `InitializeSize` lets the minimum beat the desktop size, so we do too: height stays 832 and top clamps to 0. Test: Task 2 `minimum_beats_a_smaller_desktop`.
3. **Rescaling to the size it is already at** (or twice in a row) must produce zero mutations and no undo step. Tests: Task 2 `already_at_target_is_a_no_op`, Task 4 `rescale_to_the_current_size_adds_no_undo_step`.
4. **Parameterized window ids** (`"('corpassets', 1000000000001L)"`, `chatchannel_<key>`) must find their class's limits, or none. They must never get another class's limits. Test: Task 1 `lookup_handles_tuple_and_prefix_ids`.
5. **Float HUD text round-trip.** `ship_offset` must be written as text that `set_hud_value` accepts and that reads back as the same number. Test: Task 2 `ship_offset_scales_and_stays_float`.

---

### Task 1: Window size table (generator, data, lookup)

**Files:**
- Create: `tools/gen-window-sizes.py`
- Create: `crates/settings-model/data/window-sizes.json` (generated, committed)
- Create: `crates/settings-model/src/window_sizes.rs`
- Modify: `crates/settings-model/src/lib.rs` (add `mod window_sizes;`, `pub use window_sizes::{window_limits, SizeLimits};`)
- Modify: `docs/format-notes.md`: one line under "How the client applies a saved rect…", naming the generator and the data file.

**Interfaces:**
- Produces:
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)] pub struct SizeLimits { pub min_w: i64, pub min_h: i64, pub fixed_w: Option<i64>, pub fixed_h: Option<i64> }`
  - `pub fn window_limits(id: &str) -> Option<SizeLimits>`
- JSON shape: `{ "ids": { "<windowID>": {"min_w":..,"min_h":..,"fixed_w"?:..,"fixed_h"?:..} }, "prefixes": { "<prefix>": {..} } }`

- [ ] **Step 1: Write the generator `tools/gen-window-sizes.py`.**
  - Reuse `load`, `instructions`, `imports` and `find_ccp` from `tools/gen-commands.py`, imported with `importlib.util.spec_from_file_location`, since the hyphenated name can't be imported normally.
  - Walk every module's `BUILD_CLASS` and evaluate class bodies for `default_windowID`, `default_minSize`, `default_fixedWidth`, `default_fixedHeight`, resolving:
    - names defined earlier in the same body (base `Window`: `default_minSize = (default_width, default_height)`, i.e. 256×128, evaluated once in the base, so a subclass that only sets `default_width` inherits 256×128);
    - module-level constants (`MIN_WINDOW_WIDTH`);
    - attributes of imported constant modules (`overviewConst.WINDOW_ID`, `invConst.*`).
  - Resolve inheritance by base-class name: the child's attributes override the parent's.
  - Emit only classes deriving from `Window` whose id and limits all resolved. Skip, and print, every class that didn't resolve.
  - Add these `prefixes` by hand, each mapped to its class's limits:
    - `"chatchannel_"` → the `ChatWindow` class (min 120×120);
    - `"('corpassets'"` and the other inventory tuple ids → their `Inventory*` class.
  - Write sorted JSON with a header key `"_source": "generated by tools/gen-window-sizes.py from code.ccp"`.

- [ ] **Step 2: Run the generator.**
  Run: `python -I tools/gen-window-sizes.py`
  Expected:
  - It writes `crates/settings-model/data/window-sizes.json` and prints a count of about 300 resolved and the list of skipped classes.
  - The file contains `"overview"` min 128×128, `"market"` 650×435, `"mail"` 500×350, `"directionalScannerWindow"` 300×124, `"probeScannerWindow"` 300×150, `"droneview"` 240×120, `"corporation"` 700×832, and `"fittingWnd"` with a `fixed_h`.
  - Spot-check two of these against the decompiled source before going on.

- [ ] **Step 3: Write the failing tests in `window_sizes.rs` (`#[cfg(test)] mod tests`).**

```rust
#[test]
fn the_table_carries_the_client_constants() {
    assert_eq!(window_limits("overview").map(|l| (l.min_w, l.min_h)), Some((128, 128)));
    assert_eq!(window_limits("market").map(|l| (l.min_w, l.min_h)), Some((650, 435)));
    assert_eq!(window_limits("corporation").map(|l| (l.min_w, l.min_h)), Some((700, 832)));
    assert!(window_limits("fittingWnd").and_then(|l| l.fixed_h).is_some());
    assert_eq!(window_limits("no_such_window"), None);
}

#[test]
fn lookup_handles_tuple_and_prefix_ids() {
    assert_eq!(window_limits("chatchannel_player_90000001").map(|l| (l.min_w, l.min_h)), Some((120, 120)));
    assert!(window_limits("('corpassets', 1000000000001L)").is_some());
    assert_eq!(window_limits("('unknownthing', 1)"), None);
}
```

- [ ] **Step 4: Run them to verify they fail.**
  Run: `cargo test -p settings-model window_sizes`
  Expected: FAIL to compile (`window_limits` not defined).

- [ ] **Step 5: Implement `window_limits` in `window_sizes.rs`.**
  - Parse `include_str!("../data/window-sizes.json")` once into a `std::sync::OnceLock`.
  - Lookup order:
    1. the exact id;
    2. for a tuple-string id `('name', …)`, the exact `name`;
    3. the longest matching key in `prefixes`;
    4. `None`.

- [ ] **Step 6: Run the tests to verify they pass.**
  Run: `cargo test -p settings-model window_sizes`
  Expected: PASS.

- [ ] **Step 7: Commit.**

```bash
git add tools/gen-window-sizes.py crates/settings-model/data/window-sizes.json crates/settings-model/src/window_sizes.rs crates/settings-model/src/lib.rs docs/format-notes.md
git commit -m "Generate the client's window minimum and fixed sizes"
```

---

### Task 2: The rescale transform

**Files:**
- Create: `crates/settings-model/src/rescale.rs`
- Modify: `crates/settings-model/src/lib.rs` (add `mod rescale;`, `pub use rescale::{desktop_size, rescale_layout, Rescale};`)

**Interfaces:**
- Consumes: `WindowLayout`, `WindowRect`, `Geom` (paths `x_path`…`screen_h_path`), `Stack` (`windows.rs`); `Hud`, `HudEntry` (`hud.rs`); `SizeLimits` (Task 1); `Mutation::SetScalar { path, text }`.
- Produces:
  - `pub fn desktop_size(res_w: i64, res_h: i64, ui_scale: f64) -> (i64, i64)`
  - `#[derive(Debug, Serialize)] pub struct Rescale { #[serde(skip)] pub mutations: Vec<Mutation>, #[serde(skip)] pub hud: Vec<(String, String)>, pub windows_changed: usize, pub held: Vec<String> }`. `hud` is `(field name, text)` for `set_hud_value`. `held` lists the window ids raised to a minimum or kept at a fixed size, sorted and deduplicated.
  - `pub fn rescale_layout(layout: &WindowLayout, hud: &Hud, target_w: i64, target_h: i64, limits: &dyn Fn(&str) -> Option<SizeLimits>) -> Rescale`. Callers pass `&window_limits`, and tests inject their own.

Per-axis algorithm (horizontal shown; vertical uses `y, h, H`, `min_h`, `fixed_h`). It isn't fully determined by the tests, so here it is:

```text
s = W'/W                                   (W = this window's own screen_w)
L = if x in {0,16} { x } else { round(x*s) }
R = if x+w in {W, W-16} { W' - (W-(x+w)) } else { round((x+w)*s) }
if fixed_w: size = fixed_w (and the window is "held")
else: size = R - L; if size < min_w { size = min_w; held }
if right-docked and not left-docked: L = R - size   else: R = L + size
L = clamp(L, 0, max(0, W' - size))          // the minimum beats the desktop (client order)
```

- **Stacks:** for a window in a `Stack`, `limits` is the field-wise max of `min_w`/`min_h` over the container and all members, plus any member's `fixed_*`. Every member and the container use the same source rect, so they get the same output.
- **Skip** a window when it has no `geom`, when `screen_w` or `screen_h` ≤ 0, or when it is already at `(W', H')` with an unchanged rect.
- **HUD** (source `layout.reference_w/h`; skipped when either is 0). Only entries whose `value` is `Some` are written:
  - `ship_offset` → `value × sx`, formatted with `{}` on the f64;
  - `fighter_x`/`fighter_y` → `round(× sx)`/`round(× sy)`, then `min(x, W'−32)`, `min(y, H'−266)`;
  - `badge_x`/`badge_y` → `round(× sx)`/`round(× sy)`.

- [ ] **Step 1: Write the failing tests** in `rescale.rs` `mod tests`.
  - Build `WindowLayout` values directly: the struct fields are pub, and `NodePath` can be `vec![]`-based dummy paths unique per field. Write a `fn rect(id, x, y, w, h, sw, sh) -> WindowRect` helper.
  - Compute the resulting rect per window from the emitted mutations with a helper that maps paths back to axes.
  - The source is 2560×1440 and the target 1920×1080 unless stated. `none` = `|_| None`.

```rust
#[test] fn desktop_size_truncates_like_the_client() {
    assert_eq!(desktop_size(2560, 1440, 1.25), (2048, 1152));
    assert_eq!(desktop_size(1920, 1080, 1.25), (1536, 864));
    assert_eq!(desktop_size(1366, 768, 1.1), (1241, 698));
}
#[test] fn left_docked_keeps_its_inset() {           // x=16,w=400 -> x=16,w=296
}
#[test] fn right_docked_flush_and_inset_stay_docked() { // x=2160,w=400 -> 1620,300 ; x=2144,w=400 -> 1608,296
}
#[test] fn floating_neighbour_of_an_inset_dock_still_touches() { // x=1744,w=400 -> 1308..1608, dock starts at 1608
}
#[test] fn full_width_window_spans_the_target() {     // x=0,w=2560 -> 0,1920
}
#[test] fn market_floored_right_docked_grows_leftwards() { // limits 650x435; x=2160,w=400 -> x=1270,w=650; held=["market"]
}
#[test] fn market_floored_floating_keeps_left_edge() { // x=1000,w=400 -> x=750,w=650
}
#[test] fn floored_window_is_clamped_on_screen() {    // x=2000,w=400 (right edge 2400, floating) -> x=1270,w=650
}
#[test] fn minimum_beats_a_smaller_desktop() {        // target 1024x768, limits min_h 832; y=0,h=1000 -> y=0,h=832
}
#[test] fn fixed_height_is_kept() {                   // fixed_h 760; h=760 at 1440 -> h stays 760, held
}
#[test] fn stack_members_share_the_largest_minimum() { // journal(465x300)+notepad(none) in stack C, rect (0,0,100,80) -> all three (0,0,465,300)
}
#[test] fn stale_resolution_window_converts_from_its_own_size() { // saved 1920x1080, target 1920x1080, rect unchanged -> no mutation for it, other windows still converted
}
#[test] fn already_at_target_is_a_no_op() {           // every window at 1920x1080 -> mutations empty, windows_changed 0
}
#[test] fn zero_resolution_window_is_skipped() {      // screen_w 0 -> no mutation, no panic
}
#[test] fn every_rewritten_tuple_is_restamped() {     // each changed window gets screen_w=1920, screen_h=1080 mutations
}
#[test] fn ship_offset_scales_and_stays_float() {     // "-120.5" -> "-90.375"; fighter (1000,700) -> (750,525); badge (16,16) -> (12,12)
}
#[test] fn absent_hud_values_stay_absent() {          // entries with value None produce no hud writes
}
```

  Each comment states the exact assertion values. Write them as `assert_eq!` on `(x, y, w, h)` and on `held`.

- [ ] **Step 2: Run them to verify they fail.**
  Run: `cargo test -p settings-model rescale`
  Expected: FAIL to compile.

- [ ] **Step 3: Implement `desktop_size` and `rescale_layout`** in `rescale.rs` with the algorithm above. Emit `Mutation::SetScalar` only for axes whose value changed, plus `screen_w_path`/`screen_h_path` on every changed window.

- [ ] **Step 4: Run them to verify they pass.**
  Run: `cargo test -p settings-model rescale`
  Expected: PASS (17 tests).

- [ ] **Step 5: Commit.**

```bash
git add crates/settings-model/src/rescale.rs crates/settings-model/src/lib.rs
git commit -m "Rescale transform: windows and HUD positions to a new desktop"
```

---

### Task 3: Backend command and display prefill

**Files:**
- Modify: `app/src-tauri/src/client_env.rs`: add `display` beside `scale_and_font` (`:106`).
- Modify: `app/src-tauri/src/ops.rs`: add `rescale_layout` near `set_hud_field` (`:499`).
- Modify: `app/src-tauri/src/lib.rs`: two `#[tauri::command]`s, registered in `generate_handler!` (`:809`).
- Modify: `app/src/lib/api.ts`: `rescaleLayout`, `clientDisplay` and their types, next to `setHudValue` (`:619`).

**Interfaces:**
- Consumes: `settings_model::{rescale_layout, desktop_size, window_limits, Rescale}` (Tasks 1–2); `ops::window_layout`, `ops::hud_layout`, `ops::apply_mutations`, `ops::set_hud_field`; `undo::group`; `client_env::settings_dir`.
- Produces:
  - `pub(crate) fn display(text: &str) -> Display` where `#[derive(Serialize)] pub struct Display { pub resolution: Option<(i64, i64)>, pub ui_scale: f64 }`. The resolution is `device.FullScreenResolution` when `WindowMode` is 0, else `device.WindowedResolution`. Read the real YAML shape first (Step 1).
  - `#[derive(Serialize)] pub struct RescaleReport { pub desktop_w: i64, pub desktop_h: i64, pub windows_changed: usize, pub held: Vec<String> }`
  - `pub fn rescale_layout(state: &AppState, res_w: i64, res_h: i64, ui_scale: f64) -> Result<RescaleReport, ErrDto>`
  - Tauri: `rescale_layout(width: i64, height: i64, ui_scale: f64) -> RescaleReport` and `client_display() -> Display`. `client_display` returns `{resolution: null, ui_scale: 1.0}` when no account file is open or the YAML is missing.
  - TS: `rescaleLayout(width: number, height: number, uiScale: number) => Promise<RescaleReport>`, `clientDisplay() => Promise<Display>`.

- [ ] **Step 1: Find the YAML shape of `WindowedResolution` / `FullScreenResolution`.**
  - Open any local `core_public__.yaml` under `%LOCALAPPDATA%\CCP\EVE\*\settings_*`, read-only, and read only those keys. Expected: a `[FILETIME, value]` pair like the scale keys. Note the value's form, probably `[w, h]` or `"WxH"`.
  - Use only synthetic values in the test below. Never copy the real file.

- [ ] **Step 2: Write the failing tests.**
  - In `client_env.rs` `mod tests`, following `:197-203`:
    - `display_reads_the_mode_s_resolution`: a windowed YAML with a 1920×1080 windowed and a 2560×1440 fullscreen resolution gives `Some((1920,1080))`, and fullscreen mode gives `Some((2560,1440))`;
    - `display_without_resolution_is_none`: YAML with only `UIScaleWindowed: [1, 1.25]` gives `(None, 1.25)`.
  - In `mcp.rs` `mod tests` (it has the `open_char` harness), call `ops::rescale_layout(&s.state(), …)` directly:
    - `rescale_without_geometry_is_refused`: open a char with an empty `windows` dict; expect `Err` with code `"no_geometry"`;
    - `rescale_is_one_undo_step_and_undo_restores_it`: on `layout_char_bytes()`, `rescale_layout(&st, 1920, 1080, 1.25)` gives `desktop_w == 1536`, `desktop_h == 864`, and `undo::undo_state(&st).depth == 1`. Then `undo` brings `ops::window_layout` back to overview `(100, 200, 400, 600, 2560, 1440)`.

- [ ] **Step 3: Run them to verify they fail.**
  Run: `cd app/src-tauri && cargo test display_ rescale_`
  Expected: FAIL to compile.

- [ ] **Step 4: Implement `display` and `ops::rescale_layout`.**
  - `ops::rescale_layout` opens with `let _group = undo::group(state);`. It then:
    - computes `desktop_size`;
    - reads `window_layout(Slot::Char)`, returning `ErrDto::new("no_geometry", "This character has no saved window positions to rescale.")` when no window has a geom, and `hud_layout`;
    - calls `settings_model::rescale_layout(…, &window_limits)`;
    - applies `apply_mutations(Slot::Char, &r.mutations)` when non-empty, then `set_hud_field` per HUD write.
  - On any error after the first write, call `state.history.lock().unwrap().rollback_group(&mut u, &mut c)` the way `mcp.rs:2290` does, then return the error.
  - Add the two Tauri commands and the `api.ts` functions.

- [ ] **Step 5: Run them to verify they pass.**
  Run: `cd app/src-tauri && cargo test display_ rescale_`
  Expected: PASS.

- [ ] **Step 6: Commit.**

```bash
git add app/src-tauri/src/client_env.rs app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src-tauri/src/mcp.rs app/src/lib/api.ts
git commit -m "rescale_layout command and the machine's display prefill"
```

---

### Task 4: MCP `rescale` op

**Files:**
- Modify: `app/src-tauri/src/mcp.rs`:
  - the `layout_edit` ToolDef (`:963-973`): add `"rescale"` to `op_item`, and `width`, `height`, `ui_scale` to the props;
  - the description string: `rescale {width, height, ui_scale?} (the SCREEN resolution and UI scale, e.g. 1920×1080 at 1.25; positions are converted to resolution ÷ scale; ui_scale defaults to this machine's; windows below their minimum size grow and may overlap, and are listed under rescale.held)`;
  - the `layout_edit` arm (`:1344`): handle `"rescale"` beside `chat_leave` with a `RefCell` collector.
- Modify: `app/src-tauri/src/mcp_primer.md` and the `eve_guide` `layout` topic text: one paragraph saying a rescale takes the screen resolution and UI scale, not desktop units.

**Interfaces:**
- Consumes: `ops::rescale_layout` and `client_env::display` (Task 3).
- Produces: the `layout_edit` reply carries `"rescale": {desktop_w, desktop_h, windows_changed, held}` when the batch held a rescale op.

- [ ] **Step 1: Write the failing tests** in `mcp.rs` `mod tests`.

```rust
#[test]
fn layout_edit_rescale_converts_and_reports() {
    let (s, _) = open_char(&layout_char_bytes());
    let v = s.call("layout_edit", &args(json!({ "ops": [
        { "op": "rescale", "width": 1920, "height": 1080, "ui_scale": 1.25 }
    ]}))).unwrap();
    assert_eq!(v["rescale"]["desktop_w"], 1536);
    assert_eq!(v["rescale"]["desktop_h"], 864);
    let ov = window(&v, "overview");
    assert_eq!((ov["geom"]["x"].clone(), ov["geom"]["y"].clone(), ov["geom"]["w"].clone(), ov["geom"]["h"].clone()), (json!(60), json!(120), json!(240), json!(360)));
    assert_eq!(ov["geom"]["screen_w"], 1536);
    let m = window(&v, "market");                    // saved at 1920x1080, floored to 650x435
    assert_eq!((m["geom"]["w"].clone(), m["geom"]["h"].clone()), (json!(650), json!(435)));
    assert!(v["rescale"]["held"].as_array().unwrap().contains(&json!("market")));
    assert_eq!(undo::undo_state(&s.state()).depth, 1);
}

#[test]
fn rescale_to_the_current_size_adds_no_undo_step() {
    let (s, _) = open_char(&layout_char_bytes());
    let op = json!({ "ops": [{ "op": "rescale", "width": 1920, "height": 1080, "ui_scale": 1.25 }] });
    s.call("layout_edit", &args(op.clone())).unwrap();
    let v = s.call("layout_edit", &args(op)).unwrap();
    assert_eq!(v["rescale"]["windows_changed"], 0);
    assert_eq!(undo::undo_state(&s.state()).depth, 1);
}
```

  The `window(&v, …)` helper only sees open, non-clutter windows. Overview and market are open in the fixture.

- [ ] **Step 2: Run them to verify they fail.**
  Run: `cd app/src-tauri && cargo test layout_edit_rescale rescale_to_the_current`
  Expected: FAIL (`unknown_op`).

- [ ] **Step 3: Implement the op.**
  - In the `layout_edit` arm, intercept `op == "rescale"`:
    - read `width` and `height` with `req`, and `ui_scale` with `opt`;
    - default `ui_scale` from `client_env::display` on `core_public__.yaml` in `client_env::settings_dir`, falling back to 1.0;
    - call `ops::rescale_layout(st, w, h, scale)`;
    - push its `RescaleReport` into a `RefCell<Option<RescaleReport>>`.
  - After `batch`, set `v["rescale"]` when present.
  - Update the schema props, the description, `mcp_primer.md` and `eve_guide`.

- [ ] **Step 4: Run the tests to verify they pass, along with the rest of the MCP suite.**
  Run: `cd app/src-tauri && cargo test mcp`
  Expected: PASS, including the existing `layout_edit_*` tests.

- [ ] **Step 5: Commit.**

```bash
git add app/src-tauri/src/mcp.rs app/src-tauri/src/mcp_primer.md
git commit -m "MCP: rescale op in layout_edit"
```

---

### Task 5: Rescale Sheet in the Layout view

**Files:**
- Create: `app/src/lib/RescaleSheet.svelte`
- Create: `app/src/lib/RescaleSheet.spec.ts`
- Modify: `app/src/lib/LayoutView.svelte`: a "Rescale layout…" button in `.toolbar` (`:937`), disabled with a reason when `readOnly` or `layout.reference_w === 0`; it mounts the Sheet.
- Modify: `CHANGELOG.md`: under Unreleased, "Rescale a character's whole layout to another resolution or UI scale."

Deviation from the spec: the spec suggested the command registry. `commands.ts:19-27` keeps per-view actions out of it, and in their own view's controls, so this goes in the Layout toolbar.

**Interfaces:**
- Consumes: `api.clientDisplay`, `api.rescaleLayout` (Task 3); `ui/Sheet.svelte`; `ui/Field.svelte`; LayoutView's `onDirty` and `load()`.
- Produces: `RescaleSheet` props `{ referenceW: number; referenceH: number; onApplied: (r: RescaleReport) => void; onClose: () => void }`.

The Sheet's content and copy:
- the source line: "Saved for a {referenceW}×{referenceH} desktop";
- width and height number fields, prefilled from `clientDisplay().resolution`, else `referenceW`×`referenceH`;
- a UI-scale select with options 90%, 100%, 110%, 125%, 150%, 175%, 200%, prefilled from `clientDisplay().ui_scale`. Replace these with the client's list if Task 3 Step 1 found one;
- a live line: "EVE lays this out on a {w}×{h} desktop", using the TS `Math.trunc(res / scale)`;
- Apply and Cancel buttons.

After Apply:
- show `held` as "Kept at their minimum or fixed size: …" when non-empty;
- `onApplied` marks `"char"` dirty and reloads the layout.

- [ ] **Step 1: Write the failing tests** in `RescaleSheet.spec.ts`, mocking `api` the way `LayoutView.spec.ts` does.
  - `prefills_from_this_machine`: `clientDisplay` resolves `{resolution: [1920,1080], ui_scale: 1.25}` → the fields show 1920, 1080 and 125%, and the text reads "EVE lays this out on a 1536×864 desktop".
  - `falls_back_to_the_saved_size`: `{resolution: null, ui_scale: 1}` with reference 2560×1440 → fields 2560 and 1440, "…a 2560×1440 desktop".
  - `apply_sends_the_resolution_and_scale`: changing to 1366 / 768 / 110% and clicking Apply calls `rescaleLayout(1366, 768, 1.1)`, and the live text read "1241×698" just before.
  - `lists_held_windows`: `rescaleLayout` resolves `held: ["market"]` → the text contains "market".
- Add to `LayoutView.spec.ts`:
  - `rescale_is_disabled_read_only`: with `readOnly: true`, the "Rescale layout…" button is disabled.

- [ ] **Step 2: Run them to verify they fail.**
  Run: `cd app && npx vitest run RescaleSheet LayoutView`
  Expected: FAIL (the component doesn't exist).

- [ ] **Step 3: Implement `RescaleSheet.svelte` and the toolbar button.** Follow `ShortcutsSheet.svelte` for the Sheet shell. Add the CHANGELOG line.

- [ ] **Step 4: Run the full checks.**
  Run: `cd app && npx vitest run RescaleSheet LayoutView && npx svelte-check --threshold warning`
  Expected: PASS, 0 errors and 0 warnings.

- [ ] **Step 5: Commit.**

```bash
git add app/src/lib/RescaleSheet.svelte app/src/lib/RescaleSheet.spec.ts app/src/lib/LayoutView.svelte app/src/lib/LayoutView.spec.ts CHANGELOG.md
git commit -m "Layout view: rescale the layout to another resolution or UI scale"
```

---

### Task 6: Whole-branch verification and live check

- [ ] **Step 1: Run everything.**
  Run: `cargo test --workspace` (in the background, given how long the builds take) and `cd app && npm test; echo exit=$?`
  Expected: both exit 0.

- [ ] **Step 2: Audit for real ids.**
  Run: `python .githooks/real-ids.py --all`
  Expected: no findings.

- [ ] **Step 3: Live check** (with the user, using the starting-the-app skill).
  - Open a logged-out test character, rescale to 1920×1080 at 100%, Save.
  - Launch EVE at that resolution.
  - Confirm that docked windows are still docked, floored windows sit where the canvas showed them, and nothing jumps on login.
  - Undo is not testable in game; it was covered in Task 3.
