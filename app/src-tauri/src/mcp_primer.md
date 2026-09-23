## workflow

EVE Online keeps each player's client settings in two files this server edits. `core_user_<id>.dat` is the **account** file: overview presets and appearance, and probe scan formations live there. `core_char_<id>.dat` is the **character** file: overview column widths and window layout. One account owns up to three characters, so several characters share one account file.

Sequence: `list_characters` (which files exist and who they belong to) → `open` (the account file is required; add the character file for column widths) → `overview_get` or `probes_get` → the edit tools → `save`.

`open` selects: each account keeps its own workspace for the whole conversation, so opening another character keeps the first one's unsaved edits, and reading five characters is five `open` calls with no reloads. An account's settings are edited through whichever of its characters is open; opening another character of the *same* account needs the current one saved (or undone) first — `unsaved_edits` says so. `status` lists every workspace and what is unsaved in it; `save` writes the current workspace, `save {all: true}` all of them. `open` with `discard: true` throws the unsaved edits away and reloads both files from disk, the window's own Discard — only when the user asks for it.

When the app window is open, this server runs inside it: `status` says `mode: live` and shows what the window has open. Opening the window's character works in the user's own session — every edit appears there at once and the user can Ctrl+Z it; `undo` from here works only while your own step is on top (`window_edited` otherwise). Account-side settings of the window's account are edited through the window's character; a sibling opened privately has a read-only account slot that names the character to open instead. If the window switches files under you, the next call says `switched` — call `open` again.

Rules:

1. The EVE client rewrites its settings when a character logs out. Only edit files of characters that are logged out, or the edit is overwritten.
2. Nothing reaches disk until `save`. Every edit tool changes memory and returns the new state. `restore_backup` is the one exception and says so.
3. `save` fails with `conflict` when the file changed on disk since `open` — the game or the editor wrote it. Ask the user before retrying with `force: true`.
4. Every save is backed up first. `list_backups` and `restore_backup` undo a save; `undo` reverts an unsaved edit.
5. Group ids come from `groups_search`; state ids and labels come from `overview_get` under `names.states`; EVE's built-in presets come from `builtin_presets`.

Beyond the overview and probes, the same files hold: the window **layout** (`layout_get`, `layout_render` for a picture, `layout_edit`), the **Neocom** bar (`neocom_get`/`neocom_edit`) and **HUD** (`hud_get`/`hud_set`) — character file; **autofill** (`autofill_get`/`autofill_set`/`autofill_clear_all`), **keybinds** (`keybinds_get`/`keybind_set`) and **chat** splits (`chat_get`/`chat_set_splits`) — account file; **fleet** settings (`fleet_get`/`fleet_edit`, `lookup_character`) — both. Two operations work across files and WRITE IMMEDIATELY rather than through `save`: **copy settings** (`copy_preview`, then `copy_apply`; `copy_files` for a whole file) and **settings presets** (`settings_presets_list`, `settings_preset_edit`) — saved bundles this app keeps, unrelated to overview presets.

Unsure about a concept: `eve_guide` with `overview`, `presets`, `states`, `probes`, `layout`, `keybinds` or `copy`.

## overview

The overview is EVE's list of objects in space. The account file holds one or more overview **windows**; each window holds **tabs**; each tab shows one **preset** (which objects appear) and has its own column order, visibility and widths.

Indices: `tab` is a tab's global index — unique across all windows, ascending in the order EVE draws them. `window` is the position in `overview_get`'s `windows` list. Columns are named by the `name` field `overview_get` lists (for example `NAME`, `TYPE`, `DISTANCE`); `label` is the text EVE shows.

The four `overview_*_edit` tools take `ops`, applied in order as one undo step. The first failing op rolls the whole batch back and reports its index.

`overview_pack_preview`, `overview_pack_import` and `overview_pack_export` handle EVE's shareable overview packs (YAML files players exchange). Preview before importing.

## presets

A preset is a named filter. `groups`: which object types show, by group id. `filtered_states`: a row shows only if at least one of these states applies to it (for example "Pilot is in your fleet"). `always_shown_states`: a row shows regardless of its group if one of these applies. Presets live in the account file and are shared by every tab that names them; `overview_tabs_edit` with `set_preset` points a tab at one.

EVE ships built-in presets — "Target Capsuleer: All", "Friendly: Fleet", "Mining", and more — that are not stored in the file. `builtin_presets` lists them and returns one's three lists; `overview_presets_edit` with `fork` copies those lists into a new named preset and points a tab at it in one step.

Find group ids with `groups_search` ("frigate", "wreck", "structure"). Groups belong to categories (Ship, Structure, Celestial, …); searching a category name returns every group in it.

## states

A state is a condition on a pilot or object. Ids and labels:

- 9 — Pilot has a security status below -5
- 10 — Pilot has a security status below 0
- 11 — Pilot is in your fleet
- 12 — Pilot is in your Capsuleer corporation
- 13 — Pilot is at war with your corporation/alliance
- 14 — Pilot is in your alliance
- 15 — Pilot has Excellent Standing
- 16 — Pilot has Good Standing
- 17 — Pilot has No Standing
- 18 — Pilot has Bad Standing
- 19 — Pilot has Terrible Standing
- 20 — Pilot (agent) is interactable
- 21 — Pilot has Neutral Standing
- 36 — Wreck is already viewed
- 37 — Wreck is empty
- 44 — Pilot is at war with your militia
- 45 — Pilot is in your militia or allied to your militia
- 48 — Pilot is in your Non Capsuleer corporation
- 49 — Pilot is an ally in one or more of your wars
- 50 — Pilot is a suspect
- 51 — Pilot is a criminal
- 52 — Pilot has a limited engagement with you
- 53 — Pilot has a kill right on them that you can activate
- 66 — Pilot has retribution timer

Id 68 may appear in `order` lists without a label; the client never renders it — leave it in place.

`appearance` in `overview_get` has two independent lists: `background` (the row's background colour) and `flag` (the small colour tag before the name). Each has `enabled` — the ticked states — and `order` — the priority: the first enabled state that applies wins. In `overview_appearance_edit`, `set_states` with `list` `background` or `flag` sets the enabled subset; `backgroundOrder` or `flagOrder` sets the priority order. `set_state_color` overrides one state's colour on one surface as `rgba` (four floats 0–1); omit `rgba` to return to EVE's default.

## probes

A probe formation is the arrangement of 1 to 8 scanner probes the client places when the formation is selected. Each probe is an offset from the formation centre in **metres — not AU**. 1 AU = 149,597,870,700 m. Axes: X and Z are the horizontal plane, Y is up. Each probe has its own scan range, also in metres. EVE's range ladder doubles from 0.25 AU to 32 AU: 0.25 AU = 37,399,467,675 m; 0.5 AU = 74,798,935,350 m (the default); 1 AU = 149,597,870,700 m; 2 AU = 299,195,741,400 m; 4 AU = 598,391,482,800 m; 8 AU = 1,196,782,965,600 m; 16 AU = 2,393,565,931,200 m; 32 AU = 4,787,131,862,400 m.

Formations live in the account file. `probes_set` without `id` creates one at the next free id; with `id` it replaces that formation. `probes_add_yaml` takes the editor's exchange format, so formations shared as text paste straight in. One 7-probe spread at 4 AU (a centre probe and six around it, one range for all):

    formations:
      - name: 'Spread 7 at 4 AU'
        range: 598391482800
        probes:
          - [0, 0, 0]
          - [598391482800, 0, 0]
          - [-598391482800, 0, 0]
          - [0, 0, 598391482800]
          - [0, 0, -598391482800]
          - [0, 598391482800, 0]
          - [0, -598391482800, 0]

`ranges: [...]` (one per probe) replaces `range:` when probes differ.

## layout

Call `layout_render` first — look before moving anything. The picture shows every open window as a labelled box on the screen at the file's reference size (`reference_w` × `reference_h`, the resolution the settings were saved at); the legend carries each window's id and pixel geometry. Window ids are EVE's internal names (`overview`, `market`, `fitting`, `chatchannel_local`, …); `label` is the readable one. Geometry is `x, y, w, h` in pixels from the top-left, and `layout_edit`'s `set_geometry` takes any subset of the four. A window saved at a different resolution (`resolution_matches` false) is re-stamped to the reference when you move it, so the client places it where you put it.

By default `layout_get` and `layout_render` hide closed windows and clutter — the windows EVE spawns per chat, item or dialog, and dead stack frames — which is most of a real file. `hidden` counts what was left out. `environment: docked` or `space` narrows to what can be on screen there; `match` finds a window by name. A window that is not listed is usually filtered, not missing: relax the filter before concluding. Filters never limit `layout_edit`. When you work in the app window's own workspace and the user has the Layout view open, every filter you omit follows theirs instead, and `window_view` shows it with `selected`, the window they clicked — "this window" means that one. Name their filter when your list differs from what they describe.

A **stack** is a tabbed container: its `members` are windows shown as tabs, drawn at the `anchor_id` window's geometry — moving any window in a stack moves the whole stack, since EVE keeps the container and every member at one identical rect. `stack_create {a, b}` starts one, `stack_add` / `stack_unstack` change membership, `stack_reorder` sets the tab order (list every member), `stack_delete_orphans` removes containers with no members. Flags per window are the file's own keys — `openWindows`, `pinnedWindows`, `lockedWindows`, `compactWindows`, `collapsedWindows`, `minimizedWindows`, … — `layout_get`'s per-window `flags` names the ones that are on, and its top-level `settable_flags` names every flag this file has a table for; a flag missing from `settable_flags` cannot be set. Closed windows are left out of `layout_get` and `layout_render`'s legend unless `include_closed`; `layout_render` outlines a closed window when asked. The picture shows windows and stacks only — not the HUD or the Neocom.

## keybinds

A binding is optional modifiers plus one key, shown as `Ctrl+Alt+Q`. `keybinds_get` lists every command with a label and group; `combo` is what the player reads, `keys` the stored codes. `keybind_set` takes the key by name (`Q`, `F1`, `Num 5`, `Page Up` — the names that appear in `combo`) and `ctrl` / `alt` / `shift` flags; omit the key to unbind. A combo another command already holds moves to the new command and `stolen` names the losers — tell the user, since that command is now unbound. `available` false means the account never opened the in-game keybinding screen and has no table to edit yet.

## copy

Copy settings moves aspects from one source onto other characters. Aspects: `layout` (windows, stacks, Neocom, HUD), `overview` (presets and appearance — account side — plus column widths — character side), `autofill`, `keybinds`, `probe_formations`, `fleet`, or `everything` (whole files). Character-side aspects write the target's `core_char` file; account-side aspects write the target's `core_user` file — and an account file is shared by every character on that account, so `collateral_char_ids` lists the siblings that change too. Always `copy_preview` first and show the user the plan: which files, which collateral characters, which targets are excluded and why (an unpaired target, a file in another profile folder unless `allow_other_folders`). `copy_apply` then writes each target with a backup; it does not go through `save`, so a target that was open here must be opened again. The source can be a character or a **settings preset** — a bundle saved by `settings_preset_edit` from the open files, listed by `settings_presets_list`, exportable as a file; not an overview preset.
