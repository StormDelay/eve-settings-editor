# MCP server (design)

Status: designed 2026-09-19, not yet planned.

Milestone context: an **MCP server** so a player can drive the editor through
an AI client — "hide the Corporation column on my main's PvP tab", "add a
7-probe spread formation at 4 AU" — instead of clicking through the views. The
AI gets the same write path the UI has and nothing more: every save goes
through `settings_model::save`'s backup → verify → conflict-check → atomic-write
chain, because there is no other way to reach the disk. v1 covers the
**overview** and **probe formations** editors; everything else is deferred.

## 1. Goal

`app/src-tauri/src/ops.rs` is already a Tauri-free API: 96 functions shaped
`fn(&AppState, …) -> Result<T, ErrDto>`, and every `#[tauri::command]` in
`lib.rs` is a one-line wrapper over one of them. This slice adds a **second
adapter over the same functions**, speaking MCP over stdio, and a sheet in the
app that tells the user how to plug it into their AI client.

Decisions taken during the study (2026-09-19), each with the alternative it
beat:

| Decision | Over | Because |
|---|---|---|
| **Headless**: the server is its own process with its own `AppState`, editing files on disk | A server inside the running app editing the document the UI has open | No backend→frontend refresh channel exists today (the UI only re-fetches after its *own* calls); a localhost HTTP port would need a token; Claude Desktop can't reach local HTTP without a bridge. Headless gives the same guarantees for a fraction of the build. |
| **Same exe, `--mcp` flag** | A second binary shipped as a Tauri sidecar | No sidecar config, no target-triple-suffixed files, installer untouched, an app update can't orphan the client's config path. |
| **stdio transport** | Streamable HTTP | Every local MCP client speaks stdio; HTTP is a later add-on (§11), not a redesign, because the tool set is transport-agnostic. |
| **Claude Desktop first**, others by snippet | Auto-registering every client | Claude Desktop is the non-developer surface. Every other client takes the same `mcpServers` JSON or a trivially different shape (§5). |
| **Overview + probes only, no raw-tree tool** | A generic `apply_mutations` escape hatch | The user's call: the AI can only do what the domain tools allow. The verify chain proves a file *decodes*, not that the game likes an arbitrary value. |
| **Flat op schemas, no `oneOf`/`anyOf`** | Tagged-union op schemas | Gemini's function-calling subset and OpenAI strict mode don't reliably accept unions. Claude reads a flat `{op, …optional fields}` object just as well. |
| **Catalogs as tools** (`groups_search`, names inlined in `overview_get`) | MCP resources | Several clients only implement tools. A search tool is also *better* than a 1500-row dump. |
| **`rmcp`** (official Rust SDK) for transport + protocol | Hand-rolled JSON-RPC over stdio (~150 lines, zero deps) | The protocol still moves (version negotiation, capabilities, notifications); Claude Desktop is the strict client. Schemas stay hand-written (§3.1), so rmcp's macros and `schemars` are not used. |
| **EVE context in layers**: `initialize.instructions` primer + `eve_guide(topic)` tool + self-describing data (§3.6) | Long tool descriptions; MCP resources | Descriptions are loaded every turn, so they stay terse; resources are not universal; a guide *tool* reaches every client and costs nothing until called. |

## 2. Architecture

```
AI client ──stdio, JSON-RPC──▶ eve-settings-editor(.exe) --mcp
                                  └─ mcp.rs        rmcp ServerHandler, 24 tools, primer as `instructions`
                                       └─ ops::*   → settings-model → core_*.dat
                                       └─ names / accounts / groups  (app dir, read-mostly)
```

### 2.1 Boot — `main.rs`

```rust
fn main() {
    if std::env::args().any(|a| a == "--mcp") { app_lib::mcp::serve() } else { app_lib::run() }
}
```

`mcp::serve` never touches `tauri::Builder`: no window, no WebView, no Tauri
runtime, no plugins. It builds a `tokio` current-thread runtime, constructs
`AppState::new()`, and runs `rmcp`'s stdio transport until the client closes
the pipe. Process exit is the client's hang-up; nothing is persisted between
runs except what `save` wrote.

Windows release builds carry `windows_subsystem = "windows"` (no console). A
parent that spawns the exe with pipes — every MCP client does — hands it real
stdin/stdout handles, and Rust's `std::io` uses them. This is the one claim in
the design that reading cannot verify; it is the first task of the plan (§10).

### 2.2 State

One `AppState` per process, the same struct the GUI uses: `char` and `user`
document slots, the undo `History`, and the (unused here) capture slot. Every
tool is a plain `fn(&AppState, …)` that calls `ops::*`; the rmcp layer only
parses arguments and serialises results. Nothing in `ops.rs` changes.

The undo history matters in two places: `save` uses `History::dirty(slot)` to
skip clean slots, and batched edit tools (§3.1) use `undo::group` + one
`undo::undo` to make a batch atomic without a second rollback mechanism.

### 2.3 App dir without a Tauri handle

`lib.rs::app_dir(app)` resolves `app.path().data_dir().join(APP_DIR)`. Tauri's
`data_dir()` is `dirs::data_dir()`, so the MCP process resolves
`dirs::data_dir().join(APP_DIR)` and lands in the same folder — names cache,
`accounts.json`, `groups-cache.json` are shared with the GUI, read-mostly
(`list_characters` may append to the names cache exactly as the GUI's first
launch does).

To make "same folder" true by construction rather than by coincidence,
`app_dir` is split: `pub(crate) fn app_dir_base() -> Option<PathBuf>` on
`dirs`, and the Tauri `app_dir(app)` calls it, falling back to
`app.path().data_dir()` only if `dirs` returns `None`.

### 2.4 Dependencies (`app/src-tauri/Cargo.toml`)

```toml
rmcp  = { version = "3", features = ["server", "transport-io"] }  # macros / schemars unused (§3.1)
tokio = { version = "1", features = ["rt"] }                        # already in the lockfile via reqwest
dirs  = "6"                                                         # already in the lockfile via tauri
```

`rmcp` is the only crate new to the lockfile. Pin whatever 3.x resolves at
implementation time; the lockfile records it. Whether rmcp's default features
can be switched off without losing `server` is settled at implementation —
the decision here is only that no macro or schema generation is *used*.

## 3. Tool surface

### 3.1 Conventions

- **Names**: `snake_case`, ≤ 40 chars, matching `^[a-z][a-z0-9_]*$` — inside
  the strictest client pattern even after a `server__` prefix.
- **Input schemas are hand-written `serde_json::json!` objects**, one per tool,
  `type: object`, `additionalProperties: false`, explicit `required`. No
  `oneOf`, `anyOf`, `allOf`, `$ref`, `null` types, or type arrays anywhere —
  a unit test enforces this (§8). Optional fields are *omitted*, never null;
  "omit `rgba` to clear the colour" replaces `rgba: null`.
- **Results** are JSON text in a single `text` content block
  (`serde_json::to_string_pretty`). Every mutating tool returns the
  re-projected model (`OverviewColumns` or `Formations`) so the model verifies
  its own work without a second call.
- **Domain errors are tool results with `isError: true`** carrying
  `{"code": …, "message": …}` — the `ErrDto` the GUI already gets — so the
  model sees them and can react. Protocol-level mistakes (unknown tool,
  malformed arguments, missing required field) are also tool errors, with
  codes `unknown_tool`, `bad_arguments`, `missing_field`. rmcp's own JSON-RPC
  errors are reserved for malformed JSON-RPC.
- **Batched edit tools** (`overview_*_edit`) take `ops: [ {op, …} ]` and are
  **atomic**: an `undo::group` guard is held for the loop; on the first
  failing op the guard is dropped and, if `History` depth grew during the
  batch, `undo::undo` is called once. The error carries `op_index`. The undo
  stack is the rollback mechanism — no second one.
- **Slot naming in arguments**: `"char"` / `"user"` — the same strings
  `ops::Slot` deserialises for the GUI.
- **Descriptions are the product.** Each tool's description states what it
  changes, which slot it needs open, and the rule that matters at that step
  (§3.5). The model reads nothing else.

### 3.2 Session tools (7)

| Tool | Input | Backed by | Returns |
|---|---|---|---|
| `list_characters` | — | `discover_profiles()`, `accounts::load_roster(roots, dir)`, `names::resolve_blocking(dir, ids, false)` | `{profiles: [{install, server, profile, characters: [{char_id, name?, char_file, user_id?, user_file?, account_alias?}], unpaired_accounts: [{user_id, user_file, alias?}]}]}`. Names from the cache, ESI for unknown ids (as the GUI's first launch). Files with `id: None` are skipped. |
| `open` | `char_id?`, `char_file?`, `user_file?` | `ops::open_file` per slot | Resolves `char_id` → char file and its paired account file from the same discovery + roster; explicit paths override. **The account (`user`) file is required** — overview presets and probe formations live there; the char file is optional (column widths). `ParseFailed` → error `parse_failed` with `offset` and `message`. Returns `{char: {path, fidelity}?, user: {path, fidelity}}`. Opening replaces whatever was open and clears history for that slot (as the GUI). |
| `status` | — | doc paths, `undo::undo_state`, `History::dirty` | `{char: {path, dirty}?, user: {path, dirty}?, can_undo}` — for the model to re-orient in a long conversation. |
| `save` | `force?: bool` | `ops::save_document` per **dirty** slot | `{saved: [{slot, path, backup_path}], skipped: [slot]}`. `Conflict` → error `conflict`: "the file changed on disk since it was opened (the EVE client or the editor wrote it) — ask the user before retrying with `force: true`". `ReadOnly` → error `read_only` with the reason. |
| `undo` | — | `undo::undo` | `status` output; error `nothing_to_undo` when the stack is empty. One undo = one tool call's worth of change (a batch is one entry). |
| `list_backups` | `slot` | `ops::list_file_backups` | the existing `BackupInfo` list |
| `restore_backup` | `slot`, `backup_path` | `ops::restore_backup` | **Writes to disk immediately** (as the GUI): the backup is validated as decodable, the live file is backed up first, then replaced atomically and reopened. Returns `{path, pre_restore_backup}` + `status`. The description says all of that. |

### 3.3 Overview tools (9)

All but `groups_search` and `overview_pack_preview` require the `user` slot
open; column widths additionally need `char`. Every edit returns
`OverviewColumns` as `overview_get` does.

| Tool | Input | Backed by |
|---|---|---|
| `overview_get` | — | `ops::overview_columns` + inlined names: `names: {states: {id: label}, groups: {id: name}}` for every state and group id the projection references. State labels from `overview-states.json` (`states` ∪ `exceptionStates`), group names from the bundled `overview-groups.json` plus the on-disk ESI delta cache (`groups::cached(dir)`, new, read-only). Both JSON files are `include_str!`-ed from `app/src/lib/data/`. |
| `groups_search` | `query`, `limit?` (default 50) | the same catalog: case-insensitive substring on group name **or** category name → `[{id, name, category}]`. No network. |
| `overview_columns_edit` | `ops: [...]` | `set_visible {tab, column, visible}` → `set_overview_visible`; `set_order {tab, order}` → `set_overview_order`; `set_width {tab, column, width}` → `set_overview_width`; `copy_columns {from_tab, to_tabs, order, visible, widths}` → `overview_copy_columns` |
| `overview_tabs_edit` | `ops: [...]` | `create {window, name, from_tab?}` → `tab_create`; `rename {tab, name}`; `delete {tab}`; `reorder {window, order}`; `move {tab, from_window, to_window, pos}`; `set_preset {tab, preset}`; `window_add {name, from_tab?}` → `overview_window_add`; `window_remove {window}`; `create_window_mapping {}` |
| `overview_presets_edit` | `ops: [...]` | `create {from, name}` → `preset_create`; `rename {name, new_name}`; `delete {name}`; `set_groups {name, groups}`; `set_states {name, filtered_states, always_shown_states}`; `fork {tab, name, groups, filtered_states, always_shown_states}` |
| `overview_appearance_edit` | `ops: [...]` | `set_states {list, ids}` with `list ∈ background \| backgroundOrder \| flag \| flagOrder` → `overview_set_states`; `set_state_color {surface, id, rgba?}` with `surface ∈ background \| flag` (`COLOR_SURFACES`; omit `rgba` to clear) → `overview_set_state_color`; `set_bool {key, on}` with `key ∈ OVERVIEW_BOOLS` (`applyToStructures`, `applyToOtherObjects`, `useSmallColorTags`, `useSmallText`, `overviewBroadcastsToTop`, `hideCorpTicker`) → `overview_set_bool`. The schema uses `enum` for all three, so a typo is a schema error, not a file edit. |
| `overview_pack_preview` | `path` | `ops::pack_preview` — look before importing |
| `overview_pack_import` | `path` | `ops::pack_import` |
| `overview_pack_export` | `path` | `ops::pack_export` |

Each `ops[]` item schema is one flat object: `op` (string enum of that tool's
ops, required) plus the union of every op's fields, all optional. The handler
checks the fields its `op` needs and returns `missing_field` naming
`op_index` and the field. The description lists, per op, which fields it takes
— that text is what the model reads, so it is complete and terse.

### 3.4 Probe tools (6)

All require the `user` slot open. Edits return `Formations`.

| Tool | Input | Backed by |
|---|---|---|
| `probes_get` | — | `ops::probe_formations` → `{formations: [{id, name, probes: [[x,y,z]], ranges: [m]}], selected?}` |
| `probes_set` | `id?`, `name`, `probes`, `ranges` | `ops::set_probe_formation` — omit `id` to create at the next free id, give it to update |
| `probes_remove` | `id` | `ops::remove_probe_formation` |
| `probes_reorder` | `order: [id]` | `ops::reorder_probe_formations` |
| `probes_add_yaml` | `yaml` (text) | `ops::probe_parse_yaml` then `ops::add_probe_formations` — the app's own exchange format, so formations shared as YAML paste straight in |
| `probes_export_yaml` | — | `ops::probe_yaml` over the current formations (each `Formation` mapped to a `FormationSpec` by dropping its id) → `{yaml}` |

Axes, per the existing docs: X and Z are the horizontal plane, Y is up; all
metres. The `probes_set` description says so.

### 3.5 Rules the descriptions carry

Baked into the tool text because the model has no other channel:

1. **Log the character out first.** The EVE client rewrites its settings on
   logout; edits made while logged in are lost. In `open` and `save`.
2. **Nothing reaches disk until `save`** (except `restore_backup`, which says
   so). In every edit tool's last sentence.
3. **`conflict` means ask, not force.** In `save`.
4. **Every save is backed up; `list_backups` + `restore_backup` undo it.** In
   `save`.
5. **Group ids come from `groups_search`; state ids come from `overview_get`'s
   `names.states`.** In the presets and appearance tools.
6. **Unsure about a concept: `eve_guide`.** The last sentence of every edit
   tool's description.

### 3.6 EVE context for the model

The model knows EVE at player level — what a frigate is, what the overview
does, what probes are. What it does not know is **this app's model of the
files**, and the file semantics that are not obvious from the game. That is
what the context carries, in three layers with different reach and cost:

| Layer | Reach | Cost | Carries |
|---|---|---|---|
| Tool descriptions (§3.5) | every client | loaded every turn — 24 tools × ~80 words ≈ 2.5k tokens, so they stay terse | the rule that matters at that step; one worked `ops` example per edit tool |
| `initialize.instructions` — MCP's server hint, which Claude Desktop and Claude Code inject as system context | most clients; some drop it | ~600 tokens/turn | the primer's first section: files, model, workflow |
| `eve_guide(topic)` tool | every client | nothing until called | any one primer section, for depth and as the fallback where `instructions` is dropped |
| Self-describing data | every client | only when fetched | `overview_get` inlines state labels and group names; ids never travel naked (§3.3) |

**One source, two exposures.** `app/src-tauri/src/mcp_primer.md`, compiled
in with `include_str!`, split on `## ` headings. The first section is the
`instructions` text; `eve_guide(topic)` returns one section by its heading
slug. Topics, and what each states:

- `workflow` (the first section, doubles as `instructions`): `core_user_<id>`
  is the **account** file — overview presets, appearance, probe formations
  live there; `core_char_<id>` is the **character** file — column widths.
  Hence `open` requires the account file. The sequence: `list_characters` →
  `open` → `*_get` → edit → `save`. Rules 1–5 of §3.5, once.
- `overview`: windows → tabs; each tab shows one **preset** by name and has its
  own column order/visibility/widths. Tab indices are global across windows,
  window indices are positional. Columns are named as `overview_get` lists
  them.
- `presets`: a preset = `groups` (object types by id — `groups_search`) +
  `filtered_states` (a row shows *only if* one of these applies) +
  `always_shown_states` (a row shows *regardless*). EVE ships built-in presets
  ("Target Capsuleer: All", "Friendly: Fleet", …) that are not stored in the
  file; `builtin_presets` returns their lists, and `overview_presets_edit`'s
  `fork` op copies one onto a tab under a new name.
- `states`: the state ids with their labels (from `overview-states.json`),
  and that `background` and `flag` are two independent lists, each an enabled
  subset plus a priority order where the first match wins.
- `probes`: a formation is 1–8 probes (`MAX_PROBES`), offsets from the
  formation centre in **metres, not AU** (1 AU = 149,597,870,700 m,
  `M_PER_AU`), X and Z the horizontal plane, Y up; one scan range per probe,
  also in metres; the default is 0.5 AU (`DEFAULT_RANGE`); the in-game
  ladder doubles from 0.25 AU to 32 AU. A worked 7-probe spread in YAML, so
  `probes_add_yaml` has a template.

The primer is ~600 words in total; the `workflow` section is ~200. It is
written from the overview and probe design specs and the constants named
above — compilation, not research — and it is the only prose the model ever
sees, so it is reviewed like UI copy.

**Two tools this adds** (both need no slot open):

| Tool | Input | Returns |
|---|---|---|
| `eve_guide` | `topic ∈ workflow \| overview \| presets \| states \| probes` (schema `enum`) | `{topic, text}` |
| `builtin_presets` | `name?` | without `name`: `[{name, display_name, era}]` for every entry of `default-presets.json` (`modern` and `legacy`; display names from `default-preset-names.json`); with `name`: that preset's `{name, groups, filtered_states, always_shown_states}` — the three lists `fork` takes. Error `unknown_preset` otherwise. Both JSON files `include_str!`-ed from `app/src/lib/data/`. |

Tool count is therefore 7 session + 9 overview + 6 probes + 2 context = **24**.

## 4. Portability

Nothing in `ops` or `settings-model` is OS-specific, and the two places that
could have been are already three-OS: `discover::default_roots()` handles
Windows, macOS, and Linux Wine/Proton prefixes; `release.yml` builds all three.
What remains is at the edges:

| Concern | Windows | macOS | Linux |
|---|---|---|---|
| `--mcp` on the same exe | works; the one place needing the stdio probe (§2.1) | works; the binary is `EVE Settings Editor.app/Contents/MacOS/…` and `current_exe()` returns that inner path, which is the path the client config must use | `.deb`/`.rpm`: `/usr/bin/…`, stable. **AppImage: `current_exe()` is the per-launch mount (`/tmp/.mount_XXXX/…`), unstable.** Use `$APPIMAGE`, which the runtime sets to the image's own path; fall back to `current_exe()`. |
| App dir | `dirs::data_dir()` — what Tauri calls (§2.3) | same | same (`$XDG_DATA_HOME` or `~/.local/share`) |
| Claude Desktop config | `%APPDATA%\Claude\claude_desktop_config.json` | `~/Library/Application Support/Claude/…` | no official Claude Desktop; community builds use `~/.config/Claude/`. |
| stdio framing | Rust does no `\n` → `\r\n` translation; rmcp frames | — | — |

`dirs::config_dir()` is `%APPDATA%` on Windows, `~/Library/Application
Support` on macOS and `~/.config` on Linux, so the Claude Desktop path is the
**one expression** `dirs::config_dir()?.join("Claude").join("claude_desktop_config.json")`
on every OS, with no `cfg`. "Installed" means the `Claude` directory exists.

The whole OS-specific surface is therefore one env-var lookup (`$APPIMAGE`).

CI: the stdio smoke test (§8) runs on the Linux runner and catches protocol
breakage. It cannot catch the Windows `windows_subsystem` case, because debug
builds keep the console; that is a once-per-feature manual check on the
release build (§10).

## 5. Client compatibility

The server is plain MCP over stdio — `initialize`, `tools/list`, `tools/call`.
Nothing in the protocol or the tool set is Claude-specific. Clients differ in
where the config goes and which schema features they tolerate; §3.1 already
took the strictest schema dialect. Config, by client, all pointing at the same
exe with `["--mcp"]`:

| Client | Where | Shape |
|---|---|---|
| Claude Desktop | `claude_desktop_config.json` | `mcpServers: {name: {command, args}}` — auto-registered by the sheet (§6) |
| Claude Code | `claude mcp add eve-settings -- "<exe>" --mcp`, or project `.mcp.json` | same `mcpServers` shape |
| Cursor, Windsurf, Gemini CLI, LM Studio, most others | their `mcp.json` / `settings.json` | same `mcpServers` shape |
| VS Code (Copilot) | `.vscode/mcp.json` | `servers:` instead of `mcpServers` |
| Zed | `settings.json` | `context_servers:` |
| Codex CLI | `~/.codex/config.toml` | TOML `[mcp_servers.eve-settings]` |
| ChatGPT desktop, hosted agents | remote HTTP only | **not reachable over stdio** — §11 |

`command` is an argv array in every one of these, so spaces in
`C:\Program Files\EVE Settings Editor\…` and `…/Contents/MacOS/…` are fine.

`initialize.instructions` is honoured by Claude Desktop and Claude Code and
by most current clients, but it is a hint the client *may* surface; a client
that drops it still gets the full primer through `eve_guide`, which every
edit tool's description points at (§3.6).

Per-call permission prompts are a client feature, not a protocol one: a
full-auto agent will not ask before `save`. What holds regardless of client is
the chain itself — every save backed up, the conflict guard, `restore_backup`
one call away (§7).

## 6. Setup — the "AI access" sheet

A `Sheet` like `AboutPanel.svelte`, opened from `AppMenu` ("AI access…"),
`AiAccessPanel.svelte`. Its job is to hand the user a working config with zero
typing, for any client, on any OS.

Contents, top to bottom:

1. Two sentences: what it is, and that edits go through the same backups as the
   app.
2. **Claude Desktop** row (only when `claude_desktop` is present in the info,
   i.e. the `Claude` config directory exists): status "Registered" /
   "Not registered", and one button — **Register** / **Unregister**. Register
   merges `mcpServers["eve-settings-editor"] = {command, args}` into the file,
   creating it as `{mcpServers: {…}}` if absent, never touching other keys;
   Unregister removes that one key. A note: "Restart Claude Desktop to pick
   this up."
3. **Any other MCP client** block: the `mcpServers` snippet as text, with a
   **Copy** button (the clipboard plugin is already a dependency), and one
   line: "VS Code uses a `servers` key, Codex a `[mcp_servers.…]` TOML table —
   same command and args."

Backend (`mcp_setup.rs`, two commands):

```rust
pub struct McpSetup {
    pub command: String,            // $APPIMAGE or current_exe()
    pub args: Vec<String>,          // ["--mcp"]
    pub snippet: String,            // the mcpServers JSON, pretty-printed
    pub claude_desktop: Option<ClaudeDesktop>, // None when the Claude dir does not exist
}
pub struct ClaudeDesktop { pub config_path: String, pub registered: bool }

#[tauri::command] fn mcp_setup_info() -> McpSetup
#[tauri::command] fn mcp_set_claude_desktop(on: bool) -> Result<McpSetup, ErrDto>
```

`mcp_set_claude_desktop` reads the file as `serde_json::Value` (or starts from
`{}`), inserts or removes the key, writes it back pretty-printed. Unknown keys
round-trip untouched. Errors: `not_installed` (no `Claude` dir), `io`,
`parse` (the existing file is not JSON — never overwrite it; tell the user).

## 7. Safety

- **One write path.** `save` → `ops::save_document` → `settings_model::save`:
  encode, decode-verify bit-exact, mtime/length conflict check, backup, atomic
  write. `restore_backup` → `settings_model::restore`: validate the backup
  decodes, back up the live file, atomic write. There is no third.
- **No raw-tree tool.** The AI's reach is the union of the domain ops listed
  in §3 — each of which the GUI already ships and tests.
- **Conflict is an error, force is explicit.** If the GUI or the game wrote
  the file since `open`, `save` refuses; only `force: true` overrides, and the
  description tells the model to ask first. The same guard protects the GUI:
  its next save after an MCP save hits its existing conflict dialog. No silent
  clobber in either direction.
- **Process isolation.** The MCP process never talks to the GUI process. They
  share the app dir (read-mostly) and the settings files (guarded).
- **Network**: `list_characters` may call ESI for unknown names, as the GUI
  does; nothing else does. `groups_search` reads the bundled catalog and the
  delta cache only.
- **Recoverability is the floor**, not the client's prompt: every save returns
  its backup path, and `list_backups` / `restore_backup` are tools.
- Deferred, not designed (§11): a read-only knob that turns `save` into an
  error for users who want a hard floor under a full-auto agent.

## 8. Tests

`mcp.rs` unit tests over the synthetic corpus via the existing `testkit`
(no rmcp, no process — the tool handlers are plain functions):

- `open` with a user file: `overview_get` succeeds and `names.states` has a
  label for every id in `appearance`; without a user file: `no_document`.
- `overview_columns_edit` with two ops applies both and `History` depth grows
  by exactly one (the batch is one undo step).
- `overview_columns_edit` where the second op fails (unknown column): the
  error carries `op_index: 1`, the projection equals the pre-batch projection,
  and depth is unchanged (atomicity via the undo stack).
- `overview_presets_edit` → `set_groups` with ids from `groups_search`
  ("frigate") round-trips through `overview_get`.
- `probes_set` without `id` allocates the next free id; `probes_get` shows it;
  `probes_add_yaml` of `probes_export_yaml`'s output adds the same shapes.
- `save` on a clean slot: `skipped`; after an edit: `saved` with a
  `backup_path` that exists; touching the file's mtime between `open` and
  `save`: `conflict`, and `force: true` then saves.
- `status` reports dirty flags and `can_undo` truthfully across an edit, an
  `undo`, and a `save`.
- **Primer**: `mcp_primer.md` splits into exactly the five topics of §3.6, in
  that order, each non-empty; `eve_guide` returns the section whose slug
  matches and `unknown_topic` otherwise; the `instructions` string equals the
  `workflow` section; and every state id in `overview-states.json` appears in
  the `states` section (so the primer cannot drift from the catalog).
- **Built-in presets**: `builtin_presets` without a name lists 43 entries
  (36 modern + 7 legacy) each with a display name; with "Fleet" returns lists
  equal to the catalog's; `overview_presets_edit` `fork` with those lists
  produces a preset `overview_get` shows with the same three lists.
- **Schema invariants**: for every tool — name matches `^[a-z][a-z0-9_]*$`,
  description non-empty, schema `type: object`, `additionalProperties: false`,
  `required ⊆ properties`, and the serialised schema contains none of
  `oneOf`, `anyOf`, `allOf`, `$ref`, `"null"`, or a `type` array. This is the
  client-compatibility guard from §5, as a test.

`mcp_setup.rs` unit tests on temp paths:

- register into a config holding another server → both present, other keys
  untouched; unregister → the other server remains; `registered` reflects
  each state.
- no `Claude` directory → `not_installed`; existing file that is not JSON →
  `parse`, file unchanged.
- `APPIMAGE` set → `command` is that path; unset → `current_exe()`.

Integration (`app/src-tauri/tests/mcp_stdio.rs`): spawn
`env!("CARGO_BIN_EXE_app")` with `--mcp`, send `initialize`,
`notifications/initialized`, `tools/list`; assert the 24 tool names and that
`initialize` returned a non-empty `instructions`. Runs in
`cargo test` on CI (Linux runner). Kept to the handshake on purpose — the
tools themselves are covered without a process.

Frontend: `AiAccessPanel.spec.ts` — renders the snippet, the Claude Desktop
row appears only when `claude_desktop` is present, Register/Unregister call
the command and re-render from its result, Copy writes the clipboard.

## 9. File-by-file change list

| File | Change |
|---|---|
| `app/src-tauri/Cargo.toml` | `rmcp`, `tokio`, `dirs` (§2.4) |
| `app/src-tauri/src/main.rs` | the `--mcp` branch (§2.1) |
| `app/src-tauri/src/lib.rs` | `pub mod mcp; mod mcp_setup;`, `app_dir_base()` (§2.3), register the two setup commands |
| `app/src-tauri/src/mcp.rs` | **new**: `serve()`, the `ServerHandler` (with `instructions`), the 24 tool schemas and handlers, catalog and primer `include_str!`s, unit tests |
| `app/src-tauri/src/mcp_primer.md` | **new**: the five-section EVE primer (§3.6) |
| `app/src-tauri/src/mcp_setup.rs` | **new**: `McpSetup`, exe path, Claude Desktop config read/merge/write, the two commands, unit tests |
| `app/src-tauri/src/groups.rs` | `pub fn cached(dir) -> Vec<GroupEntry>` — read the delta cache without network |
| `app/src-tauri/tests/mcp_stdio.rs` | **new**: handshake smoke test |
| `app/src/lib/api.ts` | `mcpSetupInfo()`, `mcpSetClaudeDesktop(on)` |
| `app/src/lib/AiAccessPanel.svelte` + `.spec.ts` | **new**: the sheet (§6) |
| `app/src/lib/AppMenu.svelte` | the "AI access…" row, wired like About |
| `app/src/routes/+page.svelte` | mount the sheet like `AboutPanel` |
| `README.md` | a short "AI access" section: what it is, the sheet, the snippet for other clients |
| `CHANGELOG.md` | one feature line |

Nothing in `crates/`, nothing in `ops.rs`.

## 10. Definition of done

1. **Stdio probe first**: a Windows *release* build, spawned with pipes and
   `--mcp`, answers `initialize`. Done before any tool is written; if it
   fails, the fallback is a `--mcp` console re-attach or a separate bin, and
   the plan stops to re-decide.
2. All tests in §8 green; `cargo test` exit code 0 (not "every test passed" —
   the runner can exit 1 with every test passing).
3. Manual, Windows: Register in the sheet, restart Claude Desktop, ask it to
   list characters, open one, hide a column on a named tab, save. The file's
   backup exists, the GUI opens the saved file with the column hidden, and the
   GUI's own save afterwards is not blocked.
4. Manual, macOS if a machine is available: the same, with the
   `Contents/MacOS` path in the snippet. Linux: the AppImage snippet shows the
   `$APPIMAGE` path.
5. README and CHANGELOG updated.

## 11. Deferred

- **Streamable-HTTP transport** (`--mcp-http <port>` + token) for HTTP-only
  clients, and the **live mode** (server inside the running app, UI refresh
  events) it would enable. The tool set carries over unchanged.
- **Raw-tree tool** (`apply_mutations` + the field reference as a resource).
- **Read-only knob** (preference or env var) for full-auto agents.
- **Other domains**: layout, stacks, autofill, keybinds, neocom, HUD, fleet,
  chat, accounts, copy-settings, presets — each a handful of tools over ops
  that already exist.
- **Auto-register other clients** (Cursor, Claude Code project files) — the
  same JSON shape, one more path in a table.
- **`.mcpb` desktop-extension bundle** for one-click Claude Desktop install.
