# MCP: the remaining editors (design)

Status: designed 2026-09-20, not yet planned.

Milestone context: slice 2 of the MCP server
(`docs/superpowers/specs/2026-09-19-mcp-server-design.md`, shipped in PR #96
as 24 tools over the overview and probe editors). This slice puts every other
per-file editor — layout, autofill, keybinds, neocom, HUD, fleet, chat — and
the two cross-file operations — copy settings and settings presets — behind
the same server. **22 new tools, 46 in total.** Accounts (roster, aliases,
pairing, guided capture), preferences, the update check and the raw-tree tool
stay out, as decided on 2026-09-20.

## 1. Conventions — unchanged from slice 1

Everything §3.1 of the slice-1 spec says still binds: `snake_case` tool names
matching `^[a-z][a-z0-9_]*$`; hand-written flat `json!` schemas with no
`oneOf`/`anyOf`/`allOf`/`$ref`/`"null"`/type arrays (the invariants test covers
every tool in `tool_defs()`, so the new ones are pinned for free); optional
arguments omitted, never null; results as pretty JSON text; domain errors as
tool results `{"code","message"}`; batched `_edit` tools take `ops: [{op, …}]`
(`minItems: 1`), run under one `undo::group`, are atomic through
`edit_reshared`'s rollback plus the runner's own `rollback_group` on a parse
error, and return the editor's re-projected model; descriptions are the
product. Nothing in `ops.rs` or `crates/` changes. Two direct dependencies
that are already in the lockfile through Tauri (`png` 0.18, `base64` 0.22) —
nothing new is downloaded.

Two additions to the conventions:

- **Paths never reach the model.** Several projections carry `NodePath`s
  (`Geom.x_path`, `BoolFlag.set`, `HudEntry.set`). Every `_get` in this slice
  serialises a *view* without them; the server keeps the paths for its own
  writes.
- **Immediate writes are named as such.** `copy_apply`, `copy_files` and the
  settings-preset ops write files directly (with backups), exactly as the GUI's
  batch does — there is no in-memory staging for cross-file work. Their
  descriptions say "WRITES TO DISK IMMEDIATELY" in those words, and tell the
  model to `open` a target again afterwards if it was open, because the
  in-memory document is then stale and the next `save` would report `conflict`.

Decisions taken on 2026-09-20, with what they beat:

| Decision | Over | Because |
|---|---|---|
| Per-editor `_get` + batched `_edit`, 46 tools | Fewer, wider tools (~30); one tool per op (~70) | Same convention as slice 1; small specific schemas; 46 is well inside what Claude Desktop/Code handle. |
| `layout_render` returns a PNG of the layout as an MCP image block | Deferring images; an SVG the model reads as text | The user's call: a model can only place windows sensibly if it can see them, and the first ask will be "show me my layout". PNG is what every image-capable client displays; the legend text block serves clients that cannot. |
| Per-file editors + copy settings + settings presets | Per-file only; everything but capture | Copy settings is the highest-value AI ask ("copy my main's overview onto my alts"); accounts is app config, not game settings, and capture is interactive. |
| The key-code table moves to `app/src/lib/data/vk-labels.json`, read by both sides | A Rust-owned table served by a command; a second copy in Rust | Zero plumbing, one source, and exactly how `overview-groups.json`, `overview-states.json` and `default-presets.json` already work. When catalog data moves to Rust, all four move together. |
| Layout geometry and flags are written by building the canvas's own mutations server-side | Exposing `apply_mutations` with paths | The paths stay inside the server; the tool is `{window, x, y, w, h}`. Still no raw-tree tool. |

## 2. Tool surface

### 2.1 Layout (3) — needs the character file; the account file adds nothing here

| Tool | Input | Backed by | Returns |
|---|---|---|---|
| `layout_get` | — | `ops::window_layout(state, Slot::Char)` | `{reference_w, reference_h, windows: [{id, label, name?, open, renderable, resolution_matches, geom?: {x, y, w, h, screen_w, screen_h}, flags: [{name, value, settable}], stack?: {container_id, index?}}], stacks: [{container_id, container_label, anchor_id, members}]}` — the projection minus every path; `settable` is `set != Unavailable`. Units are pixels at the file's reference resolution. |
| `layout_edit` | `ops: [...]` | see below | `layout_get`'s shape |
| `layout_render` | `width?` (px, default 1024, 320–2048), `include_closed?` (default false) | `layout_get`'s data → `mcp_render::layout_png` | **Two content blocks**: a PNG image (base64, `image/png`) of the screen at the file's reference aspect ratio, every open renderable window drawn as a filled box with a 1-px border and its `label` in a 5×7 bitmap font (uppercased, truncated to the box), stacks drawn once at their anchor with a tab strip and the member labels; plus a text block `{width, height, reference_w, reference_h, scale, windows: [{id, label, x, y, w, h, drawn, stack?}]}` — the legend, which is also what a client without image support sees. |

Ops of `layout_edit`, each one flat object with `op` plus the union of fields:

- `set_geometry {window, x?, y?, w?, h?}` — at least one of x/y/w/h (`missing_field` otherwise). Builds exactly what `LayoutView.svelte`'s `geomMutations` builds: one `set_scalar` per changed axis on the projection's `x_path`/`y_path`/`w_path`/`h_path`, plus `set_scalar` on `screen_w_path`/`screen_h_path` with the layout's `reference_w`/`reference_h` when `resolution_matches` is false and something changed. A window with no `geom` → error `no_geometry`. Unknown id → `unknown_window`.
- `set_flag {window, flag, on}` — `flagMutation`'s twin: `Set{path}` → `set_scalar` `"true"`/`"false"`; `Insert{parent, key}` → `insert_dict_entry` with `NewValue::Bool`; `Unavailable` → error `flag_unavailable`. Unknown flag name → `unknown_flag` listing the window's flag names.
- `stack_create {a, b}` → `ops::stack_create(state, a, b)`; `stack_add {window, container}` → `ops::stack_add`; `stack_unstack {window}` → `ops::stack_unstack`; `stack_reorder {container, members}` → `ops::stack_reorder`; `stack_delete_orphans {}` → `ops::stack_delete_orphans`.

The geometry and flag ops collect their mutations and apply them with one
`ops::apply_mutations(state, Slot::Char, &mutations)` per op — inside the
batch's group, so the batch stays one undo step. The projection is re-read
before each op (paths can move after a structural stack edit).

**Rendering** (`app/src-tauri/src/mcp_render.rs`, `pub(crate) fn layout_png(&WindowLayout, width: u32, include_closed: bool) -> (Vec<u8>, Legend)`):
pure Rust over an RGB buffer — a dark ground, a 1-px grid every 10 % of the
reference size, one fill colour per window cycling through eight muted hues,
a darker border, and labels from a hand-coded 5×7 bitmap font covering
`A–Z 0–9 - _ . /` (anything else draws as `.`), scaled ×2 when the box is
tall enough. Encoded with the `png` crate (RGB, default compression); a
1024-wide flat-colour image is a few KB, ~10–30 KB as base64. Closed windows
are skipped unless `include_closed`, in which case they draw as an outline
only. No HUD furniture, no neocom — windows and stacks only, and the
description says so. The tool result carries the image and the legend as
two content blocks, which means `EveMcp::call`'s return type grows a variant
for tools that attach images (`Reply { json, images: Vec<Png> }` or the
like — the plan decides the exact shape; `call_tool` maps it to
`CallToolResult::success(vec![text, image])`).

### 2.2 Autofill (3) — account file

| Tool | Input | Backed by |
|---|---|---|
| `autofill_get` | — | `ops::autofill_lists` → `[{widget, entries}]` |
| `autofill_set` | `widget`, `entries: [string]` | `ops::set_autofill_list` (an empty list clears one widget) |
| `autofill_clear_all` | — | `ops::clear_all_autofill` — "every remembered text on this account" |

### 2.3 Keybinds (2) — account file

| Tool | Input | Backed by |
|---|---|---|
| `keybinds_get` | — | `ops::keybinds`; each entry becomes `{command, label, group, keys?: [codes], combo?: "Ctrl+Q", malformed}` — label/group from `command-names.json` (unknown command → label = command, group "Other"), `combo` rendered as `keysToLabel` does (modifiers `Ctrl`/`Alt`/`Shift` then the key name; unknown code → `VK<n>`). Plus `available` from the projection. |
| `keybind_set` | `command`, `key?`, `ctrl?`, `alt?`, `shift?` | key name (case-insensitive against `vk-labels.json`'s values, e.g. `"Q"`, `"F1"`, `"Num 5"`, `"Page Up"`) → code; modifiers → `[17?, 18?, 16?, code]` in that order (`MOD_CTRL`, `MOD_ALT`, `MOD_SHIFT`); omit `key` to unbind (`None`). Unknown key name → `unknown_key` listing a few valid names. → `ops::set_keybind_cmd` → `{keybinds: <keybinds_get shape>, stolen: [commands that lost this combo]}`. |

`vk-labels.json`: `{"8": "Backspace", …}` — the `VK_LABELS` object literal
moved verbatim out of `app/src/lib/keybinds.ts`, which imports it as it does
`command-names.json`. `keysToLabel`/`eventToKeys` behaviour unchanged; their
tests pass untouched.

### 2.4 Neocom (2) — character file

| Tool | Input | Backed by |
|---|---|---|
| `neocom_get` | — | `ops::neocom_bar` → `{buttons: [{index, id, btn_type, icon_path, children}], available: [{id, btn_type, icon_path}]}` where `available` = bundled `neocom-buttons.json` ∪ the file's `original`, by id (the union the UI makes; bundled entry wins on conflict). |
| `neocom_edit` | `ops: [...]` | `reorder {order: [indices]}` → `ops::neocom_reorder`; `remove {index}` → `ops::neocom_remove`; `add {id}` → `ops::neocom_add(id, btn_type, icon_path)` with the two looked up in `available` (unknown id → `unknown_button`); `reset {}` → `ops::neocom_reset`. Returns `neocom_get`'s shape. |

### 2.5 HUD (2) — character file; account file optional

| Tool | Input | Backed by |
|---|---|---|
| `hud_get` | — | `ops::hud_layout` → `{entries: [{name, kind, value?, default, scope}]}` (no `set`) |
| `hud_set` | `name`, `value` (string, as the UI sends it: `"1"`, `"0.5"`, `"true"`) | `ops::set_hud_field` → `hud_get`'s shape |

### 2.6 Fleet (3) — both files

| Tool | Input | Backed by |
|---|---|---|
| `fleet_get` | — | `ops::fleet_settings` → `{fields: [{name, kind, value?, default, scope}], colours: [{broadcast, state: absent\|cleared\|set, rgb?, default?}], watchlist: [{char_id, name?, rgb?}], palette: [{name, rgb}], char_open, user_open}` — watchlist names from the names cache when known. |
| `fleet_edit` | `ops: [...]` | `set_field {name, value}` → `ops::set_fleet_field`; `set_colour {broadcast, rgb?}` (omit `rgb` to clear, EVE's ✕) → `ops::set_fleet_colour`; `set_watchlist_colour {char_id, rgb?}` → `ops::set_watchlist_colour`. `rgb` is `[r, g, b]` 0–1. Returns `fleet_get`'s shape. |
| `lookup_character` | `query` (name or id) | `names::lookup_blocking(dir, query)` on `off_runtime` → `{id, name}` or error `not_found`; ESI errors → `esi`. For "add X to my watchlist in red". |

### 2.7 Chat (2) — account file (the chat *window* is character-side; its splits are account-scoped, per `ops::set_chat_splits`)

| Tool | Input | Backed by |
|---|---|---|
| `chat_get` | — | `ops::chat_panels` → `[{window_id, userlist_width?, input_height?}]` |
| `chat_set_splits` | `window_ids: [string]`, `userlist_width?`, `input_height?` (at least one) | `ops::set_chat_splits(ids, userlist, input)` → `chat_get`'s shape |

### 2.8 Copy settings (3) — no open file needed; **writes to disk immediately**

| Tool | Input | Backed by |
|---|---|---|
| `copy_preview` | `source_char_id?` \| `source_char_file?` \| `source_settings_preset?`; `target_char_ids?` \| `target_char_files?`; `aspects: [enum]`; `allow_other_folders?` | `setup::setup_preview(roots, dir, source, target_paths, aspects, allow)` → the `SetupPlan`: `char_writes [{char_id, path, full_copy, resolution_mismatch}]`, `account_writes [{user_id, path, full_copy, collateral_char_ids}]`, `excluded [{char_id, reason}]`, `source_error?`. Changes nothing. |
| `copy_apply` | same | `setup::setup_apply` → `[{path, ok, backup_path?, error?}]` |
| `copy_files` | `source` (path), `targets: [paths]` | `setup::copy_files(roots, source, targets)` → the same result shape. File-as-is onto files of the same kind, no pairing needed. |

Resolution: exactly one source and at least one target (`missing_field` /
`bad_arguments` otherwise). `char_id`s resolve to paths through `discover` +
`locate` (already in `mcp.rs`); unknown → `unknown_character`.
`source_settings_preset` → `BatchSource::Preset { dir: presets::preset_path(app_dir, name), anchor_dir: <the first target's parent directory> }`; the others → `BatchSource::Character { path }`.
`aspects` enum: `layout, overview, autofill, keybinds, probe_formations, fleet, everything` (`setup::Aspect`'s serde names). Descriptions explain `collateral_char_ids` (an account file is shared by every character on it — copying an account-side aspect onto one alt changes its siblings too) and `allow_other_folders` (targets in a different profile folder are excluded unless set).

### 2.9 Settings presets (2) — app-owned bundles, not overview presets

| Tool | Input | Backed by |
|---|---|---|
| `settings_presets_list` | — | `presets::list(app_dir)` → `[{name, aspects, full, modified_unix?, error?}]` (no paths) |
| `settings_preset_edit` | `op` + fields | `create {name, aspects, overwrite?}` → `setup::preset_save(state, app_dir, name, aspects, overwrite)` from the OPEN files (needs them open; the description says so); `rename {name, new_name}` → `presets::rename`; `delete {name}` → `presets::delete`; `export {name, path}` → `presets::export_to`; `import {path}` → `presets::import_from` (returns the new name). Every op returns `settings_presets_list`'s shape (plus `imported_as` for import). Not a batch — one op per call, since each writes app-owned folders immediately. |

Both descriptions open with "A settings preset is a saved bundle of one
character's settings kept by this app — not an overview preset (see
`overview_presets_edit`)."

## 3. Primer

`mcp_primer.md` gains three sections; `TOPICS` becomes
`[workflow, overview, presets, states, probes, layout, keybinds, copy]`;
`eve_guide`'s enum follows.

- `workflow`: one added paragraph naming the editors and which file each
  lives in (character: layout, neocom, HUD; account: autofill, keybinds, chat
  splits; both: fleet), and that copy settings and settings presets write files
  directly.
- `layout`: `layout_render` first — look before moving anything; window ids
  are EVE's internal names (`overview`, `market`, `chatchannel_local`, …) with
  `label` as the readable name; geometry is
  pixels at `reference_w × reference_h`, and a window whose stored screen size
  differs is re-stamped on edit; a stack is a tabbed container drawn at its
  anchor member's geometry; `open`/`pinned`/`locked`/`compact` are the usual
  flags, listed per window with whether they can be set.
- `keybinds`: a binding is modifiers + one key, named as `Ctrl+Alt+Q`; setting
  a combo another command holds steals it and the result lists the losers;
  key names are the labels in `keybinds_get`'s `combo` field.
- `copy`: aspects and what each carries; char-side vs account-side; collateral
  characters; `copy_preview` before `copy_apply`, always; backups per target;
  reopen a target that was open. Settings presets vs overview presets.

The state-coverage test is unchanged; a new test pins the three headings and
that `workflow` names every editor tool prefix.

## 4. Tests

- `mcp.rs`, per editor, over fixture bytes in slice 1's style (`open_user`,
  and a new `open_char(bytes)` helper for character-side editors that writes a
  `core_char_5.dat`): `_get` shape has no `_path` key anywhere (a recursive
  assertion, run over every `_get` result); each `_edit` op applies and
  re-projects; batch atomicity once per batched tool on a failing second op.
- Layout: `set_geometry` on a fixture with a mismatched resolution produces
  the screen-size stamp; `set_flag` on an `Insert` target mints the key;
  `flag_unavailable` and `unknown_window` surface as errors with the batch
  rolled back.
- Render (`mcp_render.rs`): the PNG decodes (via the `png` crate's decoder)
  to the requested width and the reference aspect ratio; a pixel at the centre
  of an open window's box has that window's fill colour and a pixel outside
  every box has the ground colour; a closed window is absent unless
  `include_closed`; a stack draws once, at its anchor; the legend lists every
  window with `drawn` true/false; the bitmap font renders each supported glyph
  as a non-empty 5×7 pattern and an unsupported character as `.`. Through the
  tool: `layout_render` on the fixture returns one text block and one
  `image/png` block whose base64 decodes to a PNG signature.
- Keybinds: `"ctrl+q"` → `[17, 81]`; unbind; `unknown_key`; `stolen` populated
  when a combo moves; `vk-labels.json` has every code `keysToLabel`'s tests
  expect (the frontend tests cover that side untouched).
- Neocom: `add` by id resolves `btn_type`/`icon_path` from `available`;
  `unknown_button`.
- Copy settings, on temp copies of `fixtures/synthetic/profile`: `copy_preview`
  reports the account write's collateral characters; `copy_apply` writes,
  backs up, and a fresh `open` of a target shows the copied aspect; a source
  that is not a settings file → `source_error`.
- Settings presets, on a temp app dir: create from open files → list shows it
  with its aspects; rename; export → import round-trips under the same name;
  delete.
- `lookup_character`: `names::lookup_with(dir, query, fetch_names, fetch_ids)`
  is the existing injectable seam (private today; becomes `pub(crate)`). The
  tool's body is a thin `lookup_character_with(dir, query, …)` the test drives
  with fetchers that return a fixed hit and, separately, nothing → `{id, name}`
  and `not_found`; the ESI-backed `lookup_blocking` is only called by the tool
  arm, on `off_runtime`. No network in tests.
- Invariants and smoke: the schema test covers the new tools automatically;
  the stdio smoke test's sorted name list grows to 46.
- Frontend: `keybinds.test.ts` passes unchanged after the JSON move; `npm run
  check` clean.

## 5. File-by-file change list

| File | Change |
|---|---|
| `app/src/lib/data/vk-labels.json` | **new** — `VK_LABELS` moved verbatim |
| `app/src/lib/keybinds.ts` | import the JSON instead of the literal |
| `app/src-tauri/src/mcp.rs` | 22 tool defs, handlers, views (path stripping), the mutation builders for layout, catalog `include_str!`s (`neocom-buttons.json`, `command-names.json`, `vk-labels.json`), the image-carrying reply variant, tests |
| `app/src-tauri/src/mcp_render.rs` | **new** — `layout_png`, the bitmap font, the legend; unit tests |
| `app/src-tauri/Cargo.toml` | `png = "0.18"`, `base64 = "0.22"` (both already in the lockfile) |
| `app/src-tauri/src/mcp_primer.md` | three sections + the workflow paragraph |
| `app/src-tauri/tests/mcp_stdio.rs` | 46 names |
| `README.md` | the "AI access" section lists what the assistant can edit |
| `CHANGELOG.md` | one `[Unreleased]` line |
| spec §3.6 of slice 1 | unchanged; this document is the addendum |

`mcp.rs` will pass ~2,500 lines. If the plan finds a clean seam, the layout
mutation builders and the copy/preset wiring may live in `mcp_layout.rs` /
`mcp_copy.rs` as `pub(crate)` helpers that `mcp.rs`'s table calls — the tool
table and dispatch stay in one file so the invariants test keeps one source.

## 6. Definition of done

1. All tests in §4 green; `cargo test --workspace`, clippy `-D warnings`,
   `npm run check`, `npm test` — exit code 0 each.
2. The stdio smoke test lists 46 tools against the release exe.
3. Model-in-the-loop, with the server registered in Claude Code as it is now
   (rebuild release first): "show me my layout" (the model describes the
   image), "move my market window to the top-left", "bind
   Q to approach", "add <name> to my watchlist in red", "copy my main's overview
   onto <alt>" — each completes through the descriptions as written, and
   `copy_preview` is called before `copy_apply` without being told.
4. README and CHANGELOG updated.

## 7. Deferred

- Accounts: roster, aliases, pairing, launcher proposals — app config, and
  guided capture is interactive.
- Everything in slice 1's §11 (HTTP transport, raw tree, read-only knob).
