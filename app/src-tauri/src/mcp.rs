//! MCP server — the `--mcp` mode of the same executable. A second adapter over
//! `ops`, beside the Tauri commands in lib.rs: every tool is a plain function
//! over `AppState`, and this file only parses arguments and serialises results.
//! Spec: docs/superpowers/specs/2026-09-19-mcp-server-design.md.
//!
//! stdout IS the protocol here. Nothing in this process may print to it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{ErrorData as McpError, ServerHandler, ServiceExt};
use serde_json::{json, Map, Value};

use settings_model::{discover, FileKind, OverviewColumns, Profile};

use crate::accounts::{self, AccountRoster};
use crate::groups;
use crate::names;
use crate::ops::{self, AppState, ErrDto, OpenOutcome, Slot};
use crate::undo;

pub struct EveMcp {
    pub state: AppState,
    /// The app dir — names cache, accounts.json, groups cache. Shared with
    /// the window, read-mostly.
    pub dir: PathBuf,
    /// EVE settings roots to discover profiles under (`default_roots()` in
    /// production; a fixture dir in tests).
    pub roots: Vec<PathBuf>,
}

impl EveMcp {
    pub fn new(dir: PathBuf, roots: Vec<PathBuf>) -> Self {
        EveMcp { state: AppState::new(), dir, roots }
    }
}

type Args = Map<String, Value>;
/// A tool's outcome: the JSON the model reads, or the JSON error it reads.
type ToolResult = Result<Value, Value>;

/// The EVE context the model reads (spec §3.6). One file, two exposures: the
/// `## workflow` section is the server's `instructions`; `eve_guide` returns
/// any one section. Compiled in so it ships with the tool.
const PRIMER: &str = include_str!("mcp_primer.md");
/// State ids → labels, the same file the UI ships. The primer test pins that
/// every id here appears in the `states` section.
const STATES_JSON: &str = include_str!("../../src/lib/data/overview-states.json");
/// The bundled group catalog the UI ships: `{categories: [{id, name, groups: [{id, name}]}]}`.
const GROUPS_JSON: &str = include_str!("../../src/lib/data/overview-groups.json");
/// EVE's built-in presets, `{modern: [...], legacy: [...]}`, each
/// `{key, name, groups, filteredStates, alwaysShownStates}`.
const PRESETS_JSON: &str = include_str!("../../src/lib/data/default-presets.json");
/// Display names for the modern built-ins, keyed by the numeric part of `key`.
const PRESET_NAMES_JSON: &str = include_str!("../../src/lib/data/default-preset-names.json");

/// Windows virtual-key codes → EVE's key names, the table `keybinds.ts`
/// renders and validates with. One file, two readers.
const VK_LABELS_JSON: &str = include_str!("../../src/lib/data/vk-labels.json");

fn vk_labels() -> HashMap<i64, String> {
    let raw: HashMap<String, String> = serde_json::from_str(VK_LABELS_JSON).expect("vk-labels.json");
    raw.into_iter().filter_map(|(k, v)| Some((k.parse().ok()?, v))).collect()
}

/// A key by the name a person types — "Q", "f1", "page up" — or `None`.
fn vk_code(name: &str) -> Option<i64> {
    let want = name.trim();
    vk_labels().into_iter().find(|(_, label)| label.eq_ignore_ascii_case(want)).map(|(code, _)| code)
}

/// Command → (label, group), the UI's `command-names.json`.
const COMMAND_NAMES_JSON: &str = include_str!("../../src/lib/data/command-names.json");

fn command_names() -> HashMap<String, (String, String)> {
    #[derive(serde::Deserialize)]
    struct Entry { label: String, group: String }
    let raw: HashMap<String, Entry> = serde_json::from_str(COMMAND_NAMES_JSON).expect("command-names.json");
    raw.into_iter().map(|(k, e)| (k, (e.label, e.group))).collect()
}

/// `[17, 81]` → "Ctrl+Q", as `keybinds.ts`'s `keysToLabel` renders it.
fn combo_label(keys: &[i64]) -> String {
    let labels = vk_labels();
    keys.iter()
        .map(|&c| match c {
            settings_model::MOD_CTRL => "Ctrl".to_string(),
            settings_model::MOD_ALT => "Alt".to_string(),
            settings_model::MOD_SHIFT => "Shift".to_string(),
            _ => labels.get(&c).cloned().unwrap_or_else(|| format!("VK{c}")),
        })
        .collect::<Vec<_>>()
        .join("+")
}

fn keybinds_view(k: &settings_model::Keybinds) -> Value {
    let names = command_names();
    let entries: Vec<Value> = k.entries.iter().map(|e| {
        let (label, group) = names.get(&e.command).cloned().unwrap_or_else(|| (e.command.clone(), "Other".into()));
        json!({
            "command": e.command, "label": label, "group": group,
            "keys": e.keys, "combo": e.keys.as_deref().map(combo_label), "malformed": e.malformed,
        })
    }).collect();
    json!({ "entries": entries, "available": k.available })
}

/// A HUD-style entry without its `set` target (paths stay server-side).
fn hud_entry_view(e: &settings_model::HudEntry) -> Value {
    json!({ "name": e.name, "kind": e.kind, "value": e.value, "default": e.default, "scope": e.scope })
}

#[derive(serde::Serialize, Clone, PartialEq, Debug)]
struct GroupRow {
    id: i64,
    name: String,
    category: String,
}

/// Bundled catalog plus the ESI delta cache — no network.
fn group_catalog(dir: &Path) -> Vec<GroupRow> {
    #[derive(serde::Deserialize)]
    struct Cat { name: String, groups: Vec<Grp> }
    #[derive(serde::Deserialize)]
    struct Grp { id: i64, name: String }
    #[derive(serde::Deserialize)]
    struct File { categories: Vec<Cat> }
    let file: File = serde_json::from_str(GROUPS_JSON).expect("overview-groups.json");
    let mut rows: Vec<GroupRow> = file
        .categories
        .into_iter()
        .flat_map(|c| c.groups.into_iter().map(move |g| GroupRow { id: g.id, name: g.name, category: c.name.clone() }))
        .collect();
    rows.extend(groups::cached(dir).into_iter().map(|g| GroupRow { id: g.id, name: g.name, category: g.category_name }));
    // A bundled group can also appear in the ESI delta cache (the delta
    // widens over time) — keep one row per id.
    rows.sort_by_key(|g| g.id);
    rows.dedup_by_key(|g| g.id);
    rows
}

fn state_labels() -> HashMap<i64, String> {
    let v: Value = serde_json::from_str(STATES_JSON).expect("overview-states.json");
    v["states"]
        .as_object()
        .expect("states map")
        .iter()
        .filter_map(|(k, v)| Some((k.parse().ok()?, v.as_str()?.to_string())))
        .collect()
}

#[derive(serde::Serialize, Clone, Debug)]
struct BuiltinPreset {
    key: String,
    name: String,
    display_name: String,
    era: &'static str,
    groups: Vec<i64>,
    filtered_states: Vec<i64>,
    always_shown_states: Vec<i64>,
}

fn builtin_catalog() -> Vec<BuiltinPreset> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Raw { key: String, name: String, groups: Vec<i64>, filtered_states: Vec<i64>, always_shown_states: Vec<i64> }
    #[derive(serde::Deserialize)]
    struct File { modern: Vec<Raw>, legacy: Vec<Raw> }
    let file: File = serde_json::from_str(PRESETS_JSON).expect("default-presets.json");
    let names: HashMap<String, String> = serde_json::from_str(PRESET_NAMES_JSON).expect("default-preset-names.json");
    // A closure capturing `names` by reference and returning a `move` closure
    // over that reference can't be called twice (the borrow checker ties the
    // returned closure's lifetime to one call) — a free fn sidesteps it.
    fn convert<'a>(era: &'static str, names: &'a HashMap<String, String>) -> impl Fn(Raw) -> BuiltinPreset + 'a {
        move |r: Raw| {
            let display_name = names.get(r.key.trim_start_matches("DefaultPreset_")).cloned().unwrap_or_else(|| r.name.clone());
            BuiltinPreset {
                key: r.key, name: r.name, display_name, era,
                groups: r.groups, filtered_states: r.filtered_states, always_shown_states: r.always_shown_states,
            }
        }
    }
    let mut out: Vec<BuiltinPreset> = file.modern.into_iter().map(convert("modern", &names)).collect();
    out.extend(file.legacy.into_iter().map(convert("legacy", &names)));
    out
}

pub(crate) const TOPICS: [&str; 5] = ["workflow", "overview", "presets", "states", "probes"];

/// `(slug, body)` per `## ` heading, in file order.
fn primer_sections() -> Vec<(&'static str, &'static str)> {
    PRIMER
        .split("\n## ")
        .map(|chunk| chunk.strip_prefix("## ").unwrap_or(chunk))
        .filter(|chunk| !chunk.trim().is_empty())
        .map(|chunk| {
            let (slug, body) = chunk.split_once('\n').unwrap_or((chunk, ""));
            (slug.trim(), body.trim())
        })
        .collect()
}

/// Run blocking work off the tokio thread `serve()` drives the protocol on.
/// `names::resolve_blocking` does reqwest::blocking I/O; on the runtime
/// thread that panics in debug builds (a runtime dropped inside a runtime)
/// and stalls the reader in release. A scoped thread keeps the borrow.
fn off_runtime<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|s| s.spawn(work).join().expect("blocking worker panicked"))
}

fn primer(topic: &str) -> Option<&'static str> {
    primer_sections().into_iter().find(|(s, _)| *s == topic).map(|(_, b)| b)
}

fn instructions() -> &'static str {
    primer("workflow").expect("mcp_primer.md starts with `## workflow`")
}

fn fail(e: ErrDto) -> Value {
    serde_json::to_value(e).unwrap_or_default()
}
fn err(code: &str, message: impl Into<String>) -> Value {
    fail(ErrDto::new(code, message))
}
fn ok<T: serde::Serialize>(v: T) -> ToolResult {
    serde_json::to_value(v).map_err(|e| err("serialize", e.to_string()))
}
fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

/// A tool that returns a picture puts it under this key as base64 PNG;
/// `blocks` lifts it into an MCP image block so the text never carries it.
const PNG_KEY: &str = "png_base64";

/// The content blocks for one tool result: the JSON as text, plus an image
/// block when the result carries one.
fn blocks(mut v: Value) -> Vec<ContentBlock> {
    let png = v.as_object_mut().and_then(|m| m.remove(PNG_KEY)).and_then(|p| p.as_str().map(str::to_owned));
    let mut out = vec![ContentBlock::text(pretty(&v))];
    if let Some(data) = png {
        out.push(ContentBlock::image(data, "image/png"));
    }
    out
}

/// A required argument, deserialised into whatever the op wants.
fn req<T: serde::de::DeserializeOwned>(args: &Args, key: &str) -> Result<T, Value> {
    match args.get(key) {
        None => Err(err("missing_field", format!("`{key}` is required"))),
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| err("bad_arguments", format!("`{key}`: {e}"))),
    }
}
/// An optional argument. Absent and JSON null both mean "not given".
fn opt<T: serde::de::DeserializeOwned>(args: &Args, key: &str) -> Result<Option<T>, Value> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => serde_json::from_value(v.clone())
            .map(Some)
            .map_err(|e| err("bad_arguments", format!("`{key}`: {e}"))),
    }
}

/// An object schema. Flat and explicit on purpose: no unions, refs, nulls or
/// type arrays anywhere — the strictest clients reject them (spec §3.1), and
/// `schema_invariants_hold_for_every_tool` pins it.
fn obj(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "additionalProperties": false, "properties": properties, "required": required })
}

/// The item schema of a batched edit tool: `op` from `ops`, plus the union
/// of every op's fields, all optional. Which op takes which field is in the
/// tool description — that text is what the model reads.
fn op_item(ops: &[&str], fields: Value) -> Value {
    let mut props = fields;
    props["op"] = json!({ "type": "string", "enum": ops });
    json!({
        "ops": {
            "type": "array",
            "minItems": 1,
            "items": { "type": "object", "additionalProperties": false, "properties": props, "required": ["op"] }
        }
    })
}

/// One tool's wire definition. The description is what the model reads —
/// it is the product; keep each under ~80 words (they load every turn).
struct ToolDef {
    name: &'static str,
    description: &'static str,
    schema: fn() -> Value,
}

fn tool_defs() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "status",
            description: "What is open right now: the character and account file paths, whether each has unsaved edits, and whether undo is possible. Call it to re-orient in a long conversation.",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "eve_guide",
            description: "Explains this server's model of EVE settings. Topics: workflow (files, sequence, rules), overview (windows, tabs, columns, indices), presets (groups, filtered and always-shown states, built-ins), states (ids and labels, background and flag lists), probes (formations, metres, axes, YAML). Call it before your first edit of a kind you have not done in this conversation.",
            schema: || obj(json!({ "topic": { "type": "string", "enum": TOPICS } }), &["topic"]),
        },
        ToolDef {
            name: "open",
            description: "Open the files to edit. The account file (core_user_<id>.dat) is required: overview presets, appearance and probe formations live there. The character file adds column widths. Give char_id (from list_characters) to resolve both from the roster, or paths directly. Opening replaces what was open and clears undo. Only edit a character that is logged out: the EVE client overwrites its settings on logout.",
            schema: || obj(json!({
                "char_id": { "type": "integer", "description": "Character id; resolves char_file and, if paired, user_file." },
                "char_file": { "type": "string", "description": "Path to core_char_<id>.dat. Overrides char_id's lookup." },
                "user_file": { "type": "string", "description": "Path to core_user_<id>.dat. Required unless char_id resolves it." }
            }), &[]),
        },
        ToolDef {
            name: "save",
            description: "Write every slot with unsaved edits to disk: encode, verify by decoding, back up the current file, then replace it atomically. Returns each backup path. Fails with `conflict` if the file changed on disk since open (the EVE client or the editor wrote it) — ask the user before retrying with force. If a later slot fails, the result still lists what was already saved. Only edit a character that is logged out.",
            schema: || obj(json!({
                "force": { "type": "boolean", "description": "Overwrite a file that changed on disk since open. Only after the user agrees." }
            }), &[]),
        },
        ToolDef {
            name: "undo",
            description: "Revert the last edit. The stack survives a save, so undoing past one re-dirties the slot; status shows it. One tool call is one step and a batch is one step. Returns status. Fails with `nothing_to_undo` when there is nothing to revert. For a saved change, use list_backups and restore_backup instead.",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "list_backups",
            description: "The backups of the open file in a slot, newest first. Every save and every restore creates one.",
            schema: || obj(json!({ "slot": { "type": "string", "enum": ["char", "user"] } }), &["slot"]),
        },
        ToolDef {
            name: "restore_backup",
            description: "Replace the open file in a slot with a backup and reopen it. WRITES TO DISK IMMEDIATELY: the backup is checked to decode, the current file is backed up first (it becomes list_backups' newest entry), then replaced atomically. Unsaved edits in that slot are discarded.",
            schema: || obj(json!({
                "slot": { "type": "string", "enum": ["char", "user"] },
                "backup_path": { "type": "string", "description": "A path from list_backups." }
            }), &["slot", "backup_path"]),
        },
        ToolDef {
            name: "list_characters",
            description: "Every EVE profile on this machine with its characters (id, name when known, character file) and, where the editor has paired them, the account file each belongs to. Accounts with no paired character are listed as unpaired_accounts. Start here; then open.",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "overview_get",
            description: "The open account's overview: windows (which tabs each shows), tabs (index, name, preset, columns with order/visible/width), presets (groups, filtered_states, always_shown_states), appearance (background and flag state lists, colours, bools), plus names.states and names.groups labelling every id used. Needs the account file open; column widths need the character file too.",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "groups_search",
            description: "Find overview group ids by name: case-insensitive substring match on the group name or its category (Ship, Structure, Celestial, Drone, Entity, …). Returns [{id, name, category}]. Use the ids in presets.",
            schema: || obj(json!({
                "query": { "type": "string" },
                "limit": { "type": "integer", "description": "Max results, default 50." }
            }), &["query"]),
        },
        ToolDef {
            name: "builtin_presets",
            description: "EVE's built-in overview presets, which the files do not store. Without name: every preset's name, display_name and era. With name (display name, name or key, case-insensitive): that preset's groups, filtered_states and always_shown_states — the three lists overview_presets_edit's fork takes.",
            schema: || obj(json!({
                "name": { "type": "string", "description": "e.g. \"Friendly: Fleet\" or \"Target Capsuleer: All\"." }
            }), &[]),
        },
        ToolDef {
            name: "overview_columns_edit",
            description: "Edit a tab's columns, as a batch applied in order (one undo step; the first failure rolls everything back). Ops: set_visible {tab, column, visible}; set_order {tab, order: [column names, all of them]}; set_width {tab, column, width} (needs the character file); copy_columns {from_tab, to_tabs, order, visible, widths} (booleans choose what to copy). Column names as overview_get lists them. Returns the overview as overview_get does. Nothing reaches disk until save. Unsure: eve_guide overview.",
            schema: || obj(op_item(&["set_visible", "set_order", "set_width", "copy_columns"], json!({
                "tab": { "type": "integer" }, "column": { "type": "string" }, "visible": { "type": "boolean" },
                "order": { "type": "array", "items": { "type": "string" } }, "width": { "type": "integer" },
                "from_tab": { "type": "integer" }, "to_tabs": { "type": "array", "items": { "type": "integer" } },
                "widths": { "type": "boolean" }
            })), &["ops"]),
        },
        ToolDef {
            name: "overview_tabs_edit",
            description: "Edit windows and tabs, as a batch (one undo step; first failure rolls back). Ops: create {window, name, from_tab?} (clone from_tab's columns); rename {tab, name}; delete {tab}; reorder {window, order: [tab indices]}; move {tab, from_window, to_window, pos}; set_preset {tab, preset} (a preset name from overview_get); window_add {name, from_tab?}; window_remove {window} (only the last window can be removed); create_window_mapping {} (for an account whose file has no window list yet). Returns the overview as overview_get does. Nothing reaches disk until save. Unsure: eve_guide overview.",
            schema: || obj(op_item(&["create", "rename", "delete", "reorder", "move", "set_preset", "window_add", "window_remove", "create_window_mapping"], json!({
                "tab": { "type": "integer" }, "window": { "type": "integer" }, "name": { "type": "string" },
                "from_tab": { "type": "integer" }, "order": { "type": "array", "items": { "type": "integer" } },
                "from_window": { "type": "integer" }, "to_window": { "type": "integer" }, "pos": { "type": "integer" },
                "preset": { "type": "string" }
            })), &["ops"]),
        },
        ToolDef {
            name: "overview_presets_edit",
            description: "Edit presets, as a batch (one undo step; first failure rolls back). Ops: create {from, name} (copy an existing preset); rename {name, new_name}; delete {name}; set_groups {name, groups: [group ids]}; set_states {name, filtered_states, always_shown_states}; fork {tab, name, groups, filtered_states, always_shown_states} (new preset from explicit lists — e.g. builtin_presets' — and point the tab at it). Group ids from groups_search; state ids from overview_get names.states. Returns the overview as overview_get does. Nothing reaches disk until save. Unsure: eve_guide presets.",
            schema: || obj(op_item(&["create", "rename", "delete", "set_groups", "set_states", "fork"], json!({
                "name": { "type": "string" }, "from": { "type": "string" }, "new_name": { "type": "string" },
                "tab": { "type": "integer" },
                "groups": { "type": "array", "items": { "type": "integer" } },
                "filtered_states": { "type": "array", "items": { "type": "integer" } },
                "always_shown_states": { "type": "array", "items": { "type": "integer" } }
            })), &["ops"]),
        },
        ToolDef {
            name: "overview_appearance_edit",
            description: "Edit overview appearance, as a batch (one undo step; first failure rolls back). Ops: set_states {list, ids} where list is background or flag (the enabled states) or backgroundOrder or flagOrder (priority order, first match wins); set_state_color {surface: background|flag, id, rgba?: [r,g,b,a] 0–1} (omit rgba to restore EVE's default); set_bool {key, on} for applyToStructures, applyToOtherObjects, useSmallColorTags, useSmallText, overviewBroadcastsToTop, hideCorpTicker. State ids and labels from overview_get names.states. Returns the overview as overview_get does. Nothing reaches disk until save. Unsure: eve_guide states.",
            schema: || obj(op_item(&["set_states", "set_state_color", "set_bool"], json!({
                "list": { "type": "string", "enum": ["background", "backgroundOrder", "flag", "flagOrder"] },
                "ids": { "type": "array", "items": { "type": "integer" } },
                "surface": { "type": "string", "enum": ["background", "flag"] },
                "id": { "type": "integer" },
                "rgba": { "type": "array", "items": { "type": "number" }, "minItems": 4, "maxItems": 4 },
                "key": { "type": "string", "enum": settings_model::OVERVIEW_BOOLS },
                "on": { "type": "boolean" }
            })), &["ops"]),
        },
        ToolDef {
            name: "overview_pack_preview",
            description: "Summarise an overview pack file (the YAML players share) without changing anything: which presets, tabs and appearance it carries. Preview before overview_pack_import.",
            schema: || obj(json!({ "path": { "type": "string", "description": "Path to the .yaml pack file." } }), &["path"]),
        },
        ToolDef {
            name: "overview_pack_import",
            description: "Import an overview pack file into the open account: its presets, tabs and appearance replace or extend the current ones, as the editor's Import does. One undo step. Nothing reaches disk until save.",
            schema: || obj(json!({ "path": { "type": "string" } }), &["path"]),
        },
        ToolDef {
            name: "overview_pack_export",
            description: "Write the open account's overview as a pack file others can import. Writes that YAML file only — the settings files are untouched. Returns what was included and any colour the pack format cannot name.",
            schema: || obj(json!({ "path": { "type": "string", "description": "Where to write the .yaml file." } }), &["path"]),
        },
        ToolDef {
            name: "probes_get",
            description: "The open account's probe scanner formations: id, name, probes as [x, y, z] metre offsets from the formation centre (X and Z horizontal, Y up), and one scan range in metres per probe; plus the selected formation id if any. Needs the account file open.",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "probes_set",
            description: "Create a formation (omit id: next free id) or replace one (give id). 1 to 8 probes as [x, y, z] in METRES from the centre — not AU; 1 AU = 149597870700 m. X and Z are the horizontal plane, Y is up. ranges: one metre value per probe (0.5 AU = 74798935350 is the default; the ladder doubles 0.25 → 32 AU). Returns all formations. Nothing reaches disk until save. Unsure: eve_guide probes.",
            schema: || obj(json!({
                "id": { "type": "integer" },
                "name": { "type": "string" },
                "probes": { "type": "array", "items": { "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3 }, "minItems": 1, "maxItems": 8 },
                "ranges": { "type": "array", "items": { "type": "number" }, "minItems": 1, "maxItems": 8 }
            }), &["name", "probes", "ranges"]),
        },
        ToolDef {
            name: "probes_remove",
            description: "Delete a formation by id. Ids stay dense: later formations shift down. Returns all formations. Nothing reaches disk until save.",
            schema: || obj(json!({ "id": { "type": "integer" } }), &["id"]),
        },
        ToolDef {
            name: "probes_reorder",
            description: "Reorder formations: order lists every current id in the wanted sequence; they are renumbered 0.. in that order (the in-game menu follows it). Returns all formations. Nothing reaches disk until save.",
            schema: || obj(json!({ "order": { "type": "array", "items": { "type": "integer" } } }), &["order"]),
        },
        ToolDef {
            name: "probes_add_yaml",
            description: "Add formations from the editor's YAML exchange format (formations: - name, range or ranges, probes: [[x,y,z]…] in metres). Colliding names get a numeric suffix. Returns all formations. Nothing reaches disk until save. Unsure about the format: eve_guide probes.",
            schema: || obj(json!({ "yaml": { "type": "string" } }), &["yaml"]),
        },
        ToolDef {
            name: "probes_export_yaml",
            description: "The open account's formations as the editor's YAML exchange text, to share or to edit and feed back through probes_add_yaml. Changes nothing.",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "chat_get",
            description: "Per chat channel, the member-list width and input-box height the player set, as [{window_id, userlist_width, input_height}] — window_id is the layout id (chatchannel_local, …); a null means never resized. Account-side: needs the account file open (empty list otherwise).",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "chat_set_splits",
            description: "Set the member-list width and/or input-box height (pixels) for one or more chat channels by window_id; at least one of the two. Returns chat_get's shape. Nothing reaches disk until save.",
            schema: || obj(json!({
                "window_ids": { "type": "array", "items": { "type": "string" }, "minItems": 1 },
                "userlist_width": { "type": "integer" }, "input_height": { "type": "integer" }
            }), &["window_ids"]),
        },
        ToolDef {
            name: "hud_get",
            description: "The character's HUD furniture settings — ship HUD offset and scale, target row position, effect icons and the like — as {entries: [{name, kind, value, default, scope}]}: kind is float/int/bool, value null means EVE's default applies, scope says which file holds it (char or account). Needs the character file open; the account file adds its own entries.",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "hud_set",
            description: "Set one HUD entry by name to a value written as text (\"-77\", \"0.5\", \"true\"), matching its kind. Returns hud_get's shape. Nothing reaches disk until save.",
            schema: || obj(json!({ "name": { "type": "string" }, "value": { "type": "string" } }), &["name", "value"]),
        },
        ToolDef {
            name: "autofill_get",
            description: "The account's remembered texts — what EVE autocompletes in search boxes, chat, market, and so on — as [{widget, entries}]. widget is EVE's internal box id. Needs the account file open.",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "autofill_set",
            description: "Replace one widget's remembered entries (an empty list clears that one box). Returns every list. Nothing reaches disk until save.",
            schema: || obj(json!({ "widget": { "type": "string" }, "entries": { "type": "array", "items": { "type": "string" } } }), &["widget", "entries"]),
        },
        ToolDef {
            name: "autofill_clear_all",
            description: "Clear every remembered text on this account — every search box, every chat, every market query. Returns the (now empty) lists. Nothing reaches disk until save.",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "keybinds_get",
            description: "The account's key bindings: [{command, label, group, keys, combo, malformed}] where combo reads like \"Ctrl+Q\" and keys are the stored codes; available is false when the account never opened the in-game keybinding screen. Needs the account file open. Key names for keybind_set are the ones you see in combo.",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "keybind_set",
            description: "Bind a command to a key: key is a name as keybinds_get shows it (\"Q\", \"F1\", \"Num 5\", \"Page Up\"), with ctrl/alt/shift as needed; omit key to unbind. A combo another command holds is taken from it — stolen lists the losers, so tell the user. Returns keybinds and stolen. Nothing reaches disk until save. Unsure: eve_guide keybinds.",
            schema: || obj(json!({
                "command": { "type": "string" },
                "key": { "type": "string" },
                "ctrl": { "type": "boolean" }, "alt": { "type": "boolean" }, "shift": { "type": "boolean" }
            }), &["command"]),
        },
    ]
}

fn tools() -> Vec<Tool> {
    tool_defs()
        .into_iter()
        .map(|t| Tool::new(t.name, t.description, object((t.schema)())))
        .collect()
}

impl EveMcp {
    #[cfg(test)]
    pub(crate) fn for_tests() -> Self {
        EveMcp::new(std::env::temp_dir().join("eve-mcp-tests"), vec![])
    }

    /// Every tool, by name. Sync: the ops are mutex-guarded functions that
    /// finish in microseconds. (`list_characters` may block on ESI once for
    /// unknown names, exactly as the window's first launch does.)
    fn call(&self, name: &str, args: &Args) -> ToolResult {
        match name {
            "status" => self.status(),
            "eve_guide" => self.eve_guide(args),
            "open" => self.open(args),
            "save" => self.save(args),
            "undo" => self.undo(),
            "list_backups" => ok(ops::list_file_backups(&self.state, req(args, "slot")?).map_err(fail)?),
            "restore_backup" => self.restore_backup(args),
            "list_characters" => self.list_characters(),
            "overview_get" => self.overview_get(),
            "groups_search" => self.groups_search(args),
            "builtin_presets" => self.builtin_presets(args),
            "overview_columns_edit" => self.batch(args, columns_op, |s| s.overview_get()),
            "overview_tabs_edit" => self.batch(args, tabs_op, |s| s.overview_get()),
            "overview_presets_edit" => self.batch(args, presets_op, |s| s.overview_get()),
            "overview_appearance_edit" => self.batch(args, appearance_op, |s| s.overview_get()),
            "overview_pack_preview" => ok(ops::pack_preview(&req::<String>(args, "path")?).map_err(fail)?),
            "overview_pack_import" => {
                let r = ops::pack_import(&self.state, &req::<String>(args, "path")?).map_err(fail)?;
                Ok(json!({ "columns": self.overview_with_names(r.columns)?, "report": r.report }))
            }
            "overview_pack_export" => ok(ops::pack_export(&self.state, &req::<String>(args, "path")?).map_err(fail)?),
            "probes_get" => ok(ops::probe_formations(&self.state).map_err(fail)?),
            "probes_set" => ok(ops::set_probe_formation(&self.state, opt(args, "id")?, &req::<String>(args, "name")?, req(args, "probes")?, req(args, "ranges")?).map_err(fail)?),
            "probes_remove" => ok(ops::remove_probe_formation(&self.state, req(args, "id")?).map_err(fail)?),
            "probes_reorder" => ok(ops::reorder_probe_formations(&self.state, req(args, "order")?).map_err(fail)?),
            "probes_add_yaml" => {
                let specs = ops::probe_parse_yaml(&req::<String>(args, "yaml")?).map_err(fail)?;
                ok(ops::add_probe_formations(&self.state, specs).map_err(fail)?)
            }
            "probes_export_yaml" => {
                let f = ops::probe_formations(&self.state).map_err(fail)?;
                let specs: Vec<settings_model::FormationSpec> = f
                    .formations
                    .into_iter()
                    .map(|f| settings_model::FormationSpec { name: f.name, probes: f.probes, ranges: f.ranges })
                    .collect();
                Ok(json!({ "yaml": ops::probe_yaml(&specs) }))
            }
            "chat_get" => ok(ops::chat_panels(&self.state).map_err(fail)?),
            "chat_set_splits" => {
                let ids: Vec<String> = req(args, "window_ids")?;
                let userlist: Option<i64> = opt(args, "userlist_width")?;
                let input: Option<i64> = opt(args, "input_height")?;
                if userlist.is_none() && input.is_none() {
                    return Err(err("missing_field", "give userlist_width and/or input_height"));
                }
                ok(ops::set_chat_splits(&self.state, ids, userlist, input).map_err(fail)?)
            }
            "hud_get" => {
                let h = ops::hud_layout(&self.state).map_err(fail)?;
                Ok(json!({ "entries": h.entries.iter().map(hud_entry_view).collect::<Vec<_>>() }))
            }
            "hud_set" => {
                let h = ops::set_hud_field(&self.state, &req::<String>(args, "name")?, &req::<String>(args, "value")?).map_err(fail)?;
                Ok(json!({ "entries": h.entries.iter().map(hud_entry_view).collect::<Vec<_>>() }))
            }
            "autofill_get" => ok(ops::autofill_lists(&self.state).map_err(fail)?),
            "autofill_set" => ok(ops::set_autofill_list(&self.state, &req::<String>(args, "widget")?, req(args, "entries")?).map_err(fail)?),
            "autofill_clear_all" => ok(ops::clear_all_autofill(&self.state).map_err(fail)?),
            "keybinds_get" => Ok(keybinds_view(&ops::keybinds(&self.state).map_err(fail)?)),
            "keybind_set" => self.keybind_set(args),
            _ => Err(err("unknown_tool", format!("no tool named `{name}`"))),
        }
    }

    fn doc_path(&self, slot: Slot) -> Option<String> {
        let guard = match slot {
            Slot::Char => self.state.char.lock(),
            Slot::User => self.state.user.lock(),
        }
        .unwrap();
        guard.as_ref().map(|d| d.path.to_string_lossy().into_owned())
    }

    fn status(&self) -> ToolResult {
        let slot = |s: Slot| {
            self.doc_path(s).map(|path| json!({ "path": path, "dirty": self.state.history.lock().unwrap().dirty(s) }))
        };
        Ok(json!({
            "char": slot(Slot::Char),
            "user": slot(Slot::User),
            "can_undo": undo::undo_state(&self.state).can_undo,
        }))
    }

    fn eve_guide(&self, args: &Args) -> ToolResult {
        let topic: String = req(args, "topic")?;
        match primer(&topic) {
            Some(text) => Ok(json!({ "topic": topic, "text": text })),
            None => Err(err("unknown_topic", format!("no topic `{topic}`; one of {}", TOPICS.join(", ")))),
        }
    }
}

/// The character file with `char_id`, and the account file the roster pairs it
/// with when that file sits in the same profile directory.
fn locate(char_id: u64, profiles: &[Profile], roster: &AccountRoster) -> Option<(PathBuf, Option<PathBuf>)> {
    let user_id = roster.accounts.iter().find(|a| a.characters.contains(&char_id)).map(|a| a.user_id);
    profiles.iter().find_map(|p| {
        let c = p.files.iter().find(|f| f.kind == FileKind::Char && f.id == Some(char_id))?;
        let u = user_id
            .and_then(|uid| p.files.iter().find(|f| f.kind == FileKind::User && f.id == Some(uid)))
            .map(|f| f.path.clone());
        Some((c.path.clone(), u))
    })
}

/// The `list_characters` payload, pure: profiles as discovered, pairings as
/// the roster has them, names as the cache has them.
fn characters(profiles: &[Profile], roster: &AccountRoster, names: &names::Cache) -> Value {
    let account_of = |char_id: u64| roster.accounts.iter().find(|a| a.characters.contains(&char_id));
    let profiles: Vec<Value> = profiles
        .iter()
        .map(|p| {
            let user_path = |uid: u64| {
                p.files.iter().find(|f| f.kind == FileKind::User && f.id == Some(uid)).map(|f| f.path.to_string_lossy().into_owned())
            };
            let characters: Vec<Value> = p
                .files
                .iter()
                .filter(|f| f.kind == FileKind::Char)
                .filter_map(|f| f.id.map(|id| (id, f)))
                .map(|(id, f)| {
                    let acct = account_of(id);
                    json!({
                        "char_id": id,
                        "name": names.get(&id).map(|n| n.name.clone()),
                        "char_file": f.path.to_string_lossy(),
                        "user_id": acct.map(|a| a.user_id),
                        "user_file": acct.and_then(|a| user_path(a.user_id)),
                        "account_alias": acct.and_then(|a| a.alias.clone()),
                    })
                })
                .collect();
            let paired: std::collections::HashSet<u64> =
                roster.accounts.iter().filter(|a| !a.characters.is_empty()).map(|a| a.user_id).collect();
            let unpaired: Vec<Value> = p
                .files
                .iter()
                .filter(|f| f.kind == FileKind::User)
                .filter_map(|f| f.id.map(|id| (id, f)))
                .filter(|(id, _)| !paired.contains(id))
                .map(|(id, f)| {
                    let alias = roster.accounts.iter().find(|a| a.user_id == id).and_then(|a| a.alias.clone());
                    json!({ "user_id": id, "user_file": f.path.to_string_lossy(), "alias": alias })
                })
                .collect();
            json!({
                "install": p.install, "server": p.server, "profile": p.profile,
                "characters": characters, "unpaired_accounts": unpaired,
            })
        })
        .collect();
    json!({ "profiles": profiles })
}

fn open_slot(state: &AppState, slot: Slot, path: &str) -> Result<Value, Value> {
    match ops::open_file(state, slot, path).map_err(fail)? {
        OpenOutcome::Opened { path, fidelity, .. } => Ok(json!({ "path": path, "fidelity": fidelity })),
        OpenOutcome::ParseFailed { path, offset, message, .. } => {
            Err(err("parse_failed", format!("{path}: {message} at byte {offset}")))
        }
    }
}

/// The word a save error should use for a slot, matching how the tool
/// descriptions already talk about the two files.
fn slot_label(slot: Slot) -> &'static str {
    match slot {
        Slot::User => "account",
        Slot::Char => "character",
    }
}

impl EveMcp {
    fn open(&self, args: &Args) -> ToolResult {
        let char_id: Option<u64> = opt(args, "char_id")?;
        let mut char_file: Option<String> = opt(args, "char_file")?;
        let mut user_file: Option<String> = opt(args, "user_file")?;
        if let Some(id) = char_id {
            let profiles = discover(&self.roots);
            let roster = accounts::load_roster(&self.roots, &self.dir);
            let (c, u) = locate(id, &profiles, &roster).ok_or_else(|| {
                err("unknown_character", format!("no core_char_{id}.dat in any profile; call list_characters"))
            })?;
            char_file.get_or_insert_with(|| c.to_string_lossy().into_owned());
            if user_file.is_none() {
                user_file = u.map(|p| p.to_string_lossy().into_owned());
            }
        }
        let Some(user_file) = user_file else {
            return Err(err(
                "no_account_file",
                "user_file is required: the account file (core_user_<id>.dat) holds presets and formations. list_characters shows which one pairs with a character, or its unpaired_accounts.",
            ));
        };
        // Open the user slot FIRST: `open_file` replaces a slot on success
        // but leaves it untouched on an io error, so a failing open here (a
        // bad or missing path) returns via `?` with the previous session —
        // BOTH slots — still intact.
        let user = open_slot(&self.state, Slot::User, &user_file)?;
        // Only once the new account is open do we drop the old character:
        // clearing it unconditionally before this point would discard a
        // still-valid pairing if the user open above had failed.
        ops::close_file(&self.state, Slot::Char);
        let char = match char_file {
            Some(p) => Some(open_slot(&self.state, Slot::Char, &p)?),
            None => None,
        };
        Ok(json!({ "char": char, "user": user }))
    }

    fn save(&self, args: &Args) -> ToolResult {
        let force: bool = opt(args, "force")?.unwrap_or(false);
        let mut saved = Vec::new();
        let mut skipped = Vec::new();
        for (slot, name) in [(Slot::User, "user"), (Slot::Char, "char")] {
            let Some(path) = self.doc_path(slot) else { continue };
            if !self.state.history.lock().unwrap().dirty(slot) {
                skipped.push(name);
                continue;
            }
            match ops::save_document(&self.state, slot, force) {
                Ok(report) => saved.push(json!({ "slot": name, "path": path, "backup_path": report.backup_path })),
                // A slot after this one in the loop may never be tried — attach
                // what already made it to disk, or the caller can't tell a
                // half-saved batch from a save that touched nothing.
                Err(e) => {
                    let mut v = match e.code.as_str() {
                        "conflict" => err(
                            "conflict",
                            format!(
                                "the {} file changed on disk since it was opened (the EVE client or the editor wrote it). Ask the user before retrying with force: true.",
                                slot_label(slot)
                            ),
                        ),
                        _ => err(&e.code, format!("{} file: {}", slot_label(slot), e.message)),
                    };
                    v["saved"] = json!(saved);
                    v["skipped"] = json!(skipped);
                    return Err(v);
                }
            }
        }
        Ok(json!({ "saved": saved, "skipped": skipped }))
    }

    fn undo(&self) -> ToolResult {
        match undo::undo(&self.state) {
            Some(_) => self.status(),
            None => Err(err("nothing_to_undo", "the undo stack is empty")),
        }
    }

    fn restore_backup(&self, args: &Args) -> ToolResult {
        let slot: Slot = req(args, "slot")?;
        let backup: String = req(args, "backup_path")?;
        match ops::restore_backup(&self.state, slot, &backup).map_err(fail)? {
            OpenOutcome::Opened { path, .. } => Ok(json!({ "path": path, "status": self.status()? })),
            // Unreachable in practice: `restore` refuses a backup that does not decode.
            OpenOutcome::ParseFailed { path, message, .. } => Err(err("parse_failed", format!("{path}: {message}"))),
        }
    }

    fn list_characters(&self) -> ToolResult {
        let profiles = discover(&self.roots);
        let roster = accounts::load_roster(&self.roots, &self.dir);
        let ids: Vec<u64> = profiles
            .iter()
            .flat_map(|p| p.files.iter().filter(|f| f.kind == FileKind::Char).filter_map(|f| f.id))
            .collect();
        // Cache first, ESI for the rest — the window's first launch does the same.
        let names = off_runtime(|| names::resolve_blocking(&self.dir, &ids, false));
        Ok(characters(&profiles, &roster, &names))
    }

    fn overview_get(&self) -> ToolResult {
        let oc = ops::overview_columns(&self.state).map_err(fail)?;
        self.overview_with_names(oc)
    }

    /// The projection plus a label for every state and group id it mentions,
    /// so ids never reach the model naked (spec §3.3).
    fn overview_with_names(&self, oc: OverviewColumns) -> ToolResult {
        let mut state_ids: Vec<i64> = Vec::new();
        let mut group_ids: Vec<i64> = Vec::new();
        for p in &oc.presets {
            group_ids.extend(&p.groups);
            state_ids.extend(&p.filtered_states);
            state_ids.extend(&p.always_shown_states);
        }
        let a = &oc.appearance;
        state_ids.extend(a.background.enabled.iter().chain(&a.background.order).chain(&a.flag.enabled).chain(&a.flag.order));
        state_ids.extend(a.colors.iter().chain(&a.flag_colors).map(|(id, _)| id));
        let labels = state_labels();
        let catalog = group_catalog(&self.dir);
        let states: HashMap<String, &String> =
            state_ids.iter().filter_map(|id| labels.get(id).map(|l| (id.to_string(), l))).collect();
        let groups: HashMap<String, &str> = group_ids
            .iter()
            .filter_map(|id| catalog.iter().find(|g| g.id == *id).map(|g| (id.to_string(), g.name.as_str())))
            .collect();
        let mut v = serde_json::to_value(&oc).map_err(|e| err("serialize", e.to_string()))?;
        v["names"] = json!({ "states": states, "groups": groups });
        Ok(v)
    }

    fn groups_search(&self, args: &Args) -> ToolResult {
        let query: String = req(args, "query")?;
        let limit: usize = opt::<usize>(args, "limit")?.unwrap_or(50);
        let q = query.to_lowercase();
        let hits: Vec<GroupRow> = group_catalog(&self.dir)
            .into_iter()
            .filter(|g| g.name.to_lowercase().contains(&q) || g.category.to_lowercase().contains(&q))
            .take(limit)
            .collect();
        ok(hits)
    }

    fn builtin_presets(&self, args: &Args) -> ToolResult {
        let catalog = builtin_catalog();
        let Some(name) = opt::<String>(args, "name")? else {
            let list: Vec<Value> =
                catalog.iter().map(|p| json!({ "name": p.name, "display_name": p.display_name, "era": p.era })).collect();
            return Ok(Value::Array(list));
        };
        let q = name.to_lowercase();
        // One combined match, not display_name-then-fallback: a legacy preset's
        // display_name defaults to its bare name (no entry in the names file), so
        // preferring display_name matches first silently shadowed the real
        // ambiguity — e.g. "All" names two MODERN presets ("Target Capsuleer:
        // All", "General: All") and legacy's own "All" is a third; matching
        // display_name alone found only the legacy one and returned it as if
        // unambiguous.
        let hits: Vec<&BuiltinPreset> = catalog
            .iter()
            .filter(|p| p.display_name.to_lowercase() == q || p.name.to_lowercase() == q || p.key.to_lowercase() == q)
            .collect();
        match hits.as_slice() {
            [one] => ok(one),
            [] => Err(err("unknown_preset", format!("no built-in preset `{name}`; call builtin_presets without a name for the list"))),
            many => Err(err(
                "ambiguous_preset",
                format!("`{name}` matches {}; use a display_name or key", many.iter().map(|p| format!("\"{}\"", p.display_name)).collect::<Vec<_>>().join(", ")),
            )),
        }
    }
}

fn unknown_op(op: &str) -> Value {
    err("bad_arguments", format!("unknown op `{op}`"))
}

fn columns_op(state: &AppState, a: &Args) -> Result<(), Value> {
    let op: String = req(a, "op")?;
    match op.as_str() {
        "set_visible" => ops::set_overview_visible(state, req(a, "tab")?, &req::<String>(a, "column")?, req(a, "visible")?),
        "set_order" => ops::set_overview_order(state, req(a, "tab")?, req(a, "order")?),
        "set_width" => ops::set_overview_width(state, req(a, "tab")?, &req::<String>(a, "column")?, req(a, "width")?),
        "copy_columns" => ops::overview_copy_columns(state, req(a, "from_tab")?, req(a, "to_tabs")?, req(a, "order")?, req(a, "visible")?, req(a, "widths")?),
        _ => return Err(unknown_op(&op)),
    }
    .map(drop)
    .map_err(fail)
}

fn tabs_op(state: &AppState, a: &Args) -> Result<(), Value> {
    let op: String = req(a, "op")?;
    match op.as_str() {
        "create" => ops::tab_create(state, req(a, "window")?, req(a, "name")?, opt(a, "from_tab")?),
        "rename" => ops::tab_rename(state, req(a, "tab")?, req(a, "name")?),
        "delete" => ops::tab_delete(state, req(a, "tab")?),
        "reorder" => ops::tab_reorder(state, req(a, "window")?, req(a, "order")?),
        "move" => ops::tab_move(state, req(a, "tab")?, req(a, "from_window")?, req(a, "to_window")?, req(a, "pos")?),
        "set_preset" => ops::tab_set_preset(state, req(a, "tab")?, req(a, "preset")?),
        "window_add" => ops::overview_window_add(state, req(a, "name")?, opt(a, "from_tab")?),
        "window_remove" => ops::overview_window_remove(state, req(a, "window")?),
        "create_window_mapping" => ops::overview_create_window_mapping(state),
        _ => return Err(unknown_op(&op)),
    }
    .map(drop)
    .map_err(fail)
}

fn presets_op(state: &AppState, a: &Args) -> Result<(), Value> {
    let op: String = req(a, "op")?;
    match op.as_str() {
        "create" => ops::preset_create(state, req(a, "from")?, req(a, "name")?),
        "rename" => ops::preset_rename(state, req(a, "name")?, req(a, "new_name")?),
        "delete" => ops::preset_delete(state, req(a, "name")?),
        "set_groups" => ops::preset_set_groups(state, req(a, "name")?, req(a, "groups")?),
        "set_states" => ops::preset_set_states(state, req(a, "name")?, req(a, "filtered_states")?, req(a, "always_shown_states")?),
        "fork" => ops::preset_fork(state, req(a, "tab")?, req(a, "name")?, req(a, "groups")?, req(a, "filtered_states")?, req(a, "always_shown_states")?),
        _ => return Err(unknown_op(&op)),
    }
    .map(drop)
    .map_err(fail)
}

fn appearance_op(state: &AppState, a: &Args) -> Result<(), Value> {
    let op: String = req(a, "op")?;
    match op.as_str() {
        "set_states" => ops::overview_set_states(state, req(a, "list")?, req(a, "ids")?),
        "set_state_color" => ops::overview_set_state_color(state, req(a, "surface")?, req(a, "id")?, opt(a, "rgba")?),
        "set_bool" => ops::overview_set_bool(state, req(a, "key")?, req(a, "on")?),
        _ => return Err(unknown_op(&op)),
    }
    .map(drop)
    .map_err(fail)
}

impl EveMcp {
    /// Apply `ops` in order under one undo group, so the batch is one undo
    /// step and — because `edit_reshared` rolls an open group back on any
    /// failing write — atomic. A failure carries the op's index.
    fn batch(
        &self,
        args: &Args,
        apply: fn(&AppState, &Args) -> Result<(), Value>,
        finish: impl FnOnce(&EveMcp) -> ToolResult,
    ) -> ToolResult {
        let ops_list: Vec<Args> = req(args, "ops")?;
        {
            let _group = undo::group(&self.state);
            for (i, op) in ops_list.iter().enumerate() {
                if let Err(mut e) = apply(&self.state, op) {
                    // A write failure already rolled the group back inside
                    // `edit_reshared`; a parse failure did not. `rollback_group`
                    // is a no-op when nothing was written, so call it always.
                    let mut u = self.state.user.lock().unwrap();
                    let mut c = self.state.char.lock().unwrap();
                    self.state.history.lock().unwrap().rollback_group(&mut u, &mut c);
                    e["op_index"] = json!(i);
                    return Err(e);
                }
            }
        }
        finish(self)
    }

    fn keybind_set(&self, args: &Args) -> ToolResult {
        let command: String = req(args, "command")?;
        let keys = match opt::<String>(args, "key")? {
            None => None,
            Some(name) => {
                let code = vk_code(&name).ok_or_else(|| {
                    err("unknown_key", format!("no key named `{name}`; names are the ones keybinds_get shows, e.g. Q, F1, Num 5, Page Up"))
                })?;
                let mut keys = Vec::new();
                if opt::<bool>(args, "ctrl")?.unwrap_or(false) { keys.push(settings_model::MOD_CTRL); }
                if opt::<bool>(args, "alt")?.unwrap_or(false) { keys.push(settings_model::MOD_ALT); }
                if opt::<bool>(args, "shift")?.unwrap_or(false) { keys.push(settings_model::MOD_SHIFT); }
                keys.push(code);
                Some(keys)
            }
        };
        let r = ops::set_keybind_cmd(&self.state, &command, keys).map_err(fail)?;
        Ok(json!({ "keybinds": keybinds_view(&r.keybinds), "stolen": r.stolen }))
    }
}

impl ServerHandler for EveMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("eve-settings-editor", env!("CARGO_PKG_VERSION")))
            .with_instructions(instructions())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(tools()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let args = request.arguments.unwrap_or_default();
        // Both outcomes are *results*, not protocol errors: the model must be
        // able to read a domain failure and react to it.
        let result = match self.call(&request.name, &args) {
            Ok(v) => CallToolResult::success(blocks(v)),
            Err(e) => CallToolResult::error(vec![ContentBlock::text(pretty(&e))]),
        };
        Ok(result.into())
    }
}

/// Run until the client closes the pipe. Never builds a Tauri app.
pub fn serve() {
    let dir = crate::app_dir_base().unwrap_or_else(std::env::temp_dir);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(async {
        let server = EveMcp::new(dir, settings_model::default_roots());
        let running = server.serve(rmcp::transport::stdio()).await.expect("mcp initialize");
        let _ = running.waiting().await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::testkit::{b, temp_file};
    use blue_marshal::{encode, Value as BmValue};
    use std::path::PathBuf;

    #[test]
    fn the_key_table_parses_and_maps_names_both_ways() {
        let labels = vk_labels();
        assert_eq!(labels.get(&81).map(String::as_str), Some("Q"));
        assert_eq!(labels.get(&112).map(String::as_str), Some("F1"));
        assert!(labels.len() >= 80);
        assert_eq!(vk_code("q"), Some(81));
        assert_eq!(vk_code("Page Up"), Some(33));
        assert_eq!(vk_code("num 5"), Some(101));
        assert_eq!(vk_code("Hyper"), None);
    }

    /// An account file with one overview tab named PvP, columns NAME (visible)
    /// and TYPE (hidden). Mirrors `ops::tests::overview_user_bytes`.
    fn overview_user_bytes() -> Vec<u8> {
        let tab = BmValue::Dict(vec![
            (BmValue::Str("name".into()), BmValue::Str("PvP".into())),
            (b("tabColumnOrder"), BmValue::List(vec![b("NAME"), b("TYPE")])),
            (b("tabColumns"), BmValue::List(vec![b("NAME")])),
        ]);
        encode(&BmValue::Dict(vec![(
            b("overview"),
            BmValue::Dict(vec![(
                b("tabsettings_new"),
                BmValue::Tuple(vec![BmValue::Long(vec![0u8; 8]), BmValue::Dict(vec![(BmValue::Int(0), tab)])]),
            )]),
        )]))
        .unwrap()
    }

    /// A server with `bytes` written to a temp account file and opened in the
    /// user slot.
    fn open_user(bytes: &[u8]) -> (EveMcp, PathBuf) {
        let path = temp_file("mcp", bytes);
        let s = EveMcp::for_tests();
        let mut a = Args::new();
        a.insert("user_file".into(), json!(path.to_string_lossy()));
        s.call("open", &a).unwrap();
        (s, path)
    }

    fn args(v: Value) -> Args {
        v.as_object().unwrap().clone()
    }

    /// Dropping a tokio runtime inside `block_on` is the panic reqwest's
    /// blocking client triggers in debug builds; `off_runtime` must make it
    /// safe, because `list_characters` does exactly that for uncached names.
    #[test]
    fn blocking_work_runs_off_the_runtime_thread() {
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(async {
            let n = off_runtime(|| {
                let inner = tokio::runtime::Builder::new_current_thread().build().unwrap();
                drop(inner);
                42
            });
            assert_eq!(n, 42);
        });
    }

    #[test]
    fn open_requires_the_account_file() {
        let s = EveMcp::for_tests();
        let e = s.call("open", &Args::new()).unwrap_err();
        assert_eq!(e["code"], "no_account_file");
    }

    #[test]
    fn open_reports_both_slots_and_status_sees_them() {
        let (s, path) = open_user(&overview_user_bytes());
        let st = s.call("status", &Args::new()).unwrap();
        assert_eq!(st["user"]["path"], json!(path.to_string_lossy()));
        assert_eq!(st["user"]["dirty"], false);
        assert_eq!(st["char"], Value::Null);
    }

    #[test]
    fn a_failed_char_reopen_does_not_leave_the_previous_character_paired() {
        let (s, path) = open_user(&overview_user_bytes());
        let cpath = temp_file("mcp-char", &encode(&BmValue::Dict(vec![])).unwrap());
        s.call("open", &args(json!({ "user_file": path.to_string_lossy(), "char_file": cpath.to_string_lossy() }))).unwrap();
        assert!(s.call("status", &Args::new()).unwrap()["char"].is_object());

        let missing = cpath.with_file_name("does-not-exist.dat");
        let e = s
            .call("open", &args(json!({ "user_file": path.to_string_lossy(), "char_file": missing.to_string_lossy() })))
            .unwrap_err();
        assert_eq!(e["code"], "io");
        assert_eq!(s.call("status", &Args::new()).unwrap()["char"], Value::Null);
    }

    /// `open_file` leaves a slot untouched on an io error — a failing NEW
    /// account open must not discard the session that was already open,
    /// unsaved edits included.
    #[test]
    fn a_failed_account_open_leaves_the_previous_session_intact() {
        let (s, path) = open_user(&overview_user_bytes());
        let cpath = temp_file("mcp-char", &encode(&BmValue::Dict(vec![])).unwrap());
        s.call("open", &args(json!({ "user_file": path.to_string_lossy(), "char_file": cpath.to_string_lossy() }))).unwrap();
        ops::set_overview_visible(&s.state, 0, "TYPE", true).unwrap();

        let e = s
            .call("open", &args(json!({ "user_file": "Z:/no/such/core_user_1.dat", "char_file": cpath.to_string_lossy() })))
            .unwrap_err();
        assert_eq!(e["code"], "io");

        let st = s.call("status", &Args::new()).unwrap();
        assert_eq!(st["user"]["path"], json!(path.to_string_lossy()), "the original account is still open");
        assert_eq!(st["user"]["dirty"], true, "the unsaved edit survived");
        assert_eq!(st["char"]["path"], json!(cpath.to_string_lossy()), "the original character is still open");
    }

    #[test]
    fn open_of_an_undecodable_file_is_parse_failed() {
        let path = temp_file("mcp-bad", &[0x7E, 0, 0, 0, 0, 0x3D]);
        let s = EveMcp::for_tests();
        let e = s.call("open", &args(json!({ "user_file": path.to_string_lossy() }))).unwrap_err();
        assert_eq!(e["code"], "parse_failed");
    }

    #[test]
    fn save_skips_a_clean_slot_and_writes_a_dirty_one_with_a_backup() {
        let (s, path) = open_user(&overview_user_bytes());
        let v = s.call("save", &Args::new()).unwrap();
        assert_eq!(v["saved"], json!([]));
        assert_eq!(v["skipped"], json!(["user"]));

        ops::set_overview_visible(&s.state, 0, "TYPE", true).unwrap();
        assert_eq!(s.call("status", &Args::new()).unwrap()["user"]["dirty"], true);
        let v = s.call("save", &Args::new()).unwrap();
        assert_eq!(v["saved"][0]["slot"], "user");
        assert_eq!(v["saved"][0]["path"], json!(path.to_string_lossy()));
        assert!(PathBuf::from(v["saved"][0]["backup_path"].as_str().unwrap()).exists());
        assert_eq!(s.call("status", &Args::new()).unwrap()["user"]["dirty"], false);
    }

    #[test]
    fn save_conflict_is_an_error_until_forced() {
        let (s, path) = open_user(&overview_user_bytes());
        ops::set_overview_visible(&s.state, 0, "TYPE", true).unwrap();
        std::fs::write(&path, encode(&BmValue::Dict(vec![])).unwrap()).unwrap();
        let e = s.call("save", &Args::new()).unwrap_err();
        assert_eq!(e["code"], "conflict");
        assert!(e["message"].as_str().unwrap().contains("force"));
        s.call("save", &args(json!({ "force": true }))).unwrap();
    }

    #[test]
    fn save_reports_what_was_already_saved_when_a_later_slot_fails() {
        let (s, _) = open_user(&overview_user_bytes());
        let cpath = temp_file("mcp-char", &encode(&BmValue::Dict(vec![])).unwrap());
        ops::open_file(&s.state, Slot::Char, cpath.to_str().unwrap()).unwrap();

        ops::set_overview_visible(&s.state, 0, "TYPE", true).unwrap();
        ops::apply_mutation(
            &s.state,
            Slot::Char,
            &settings_model::Mutation::InsertDictEntry {
                parent: vec![],
                key: settings_model::NewValue::Str("x".into()),
                value: settings_model::NewValue::Int("1".into()),
            },
        )
        .unwrap();

        // The char file changes on disk after load, so its save conflicts —
        // but only after the user slot already went through.
        std::fs::write(&cpath, encode(&BmValue::Dict(vec![(b("k"), BmValue::Int(1))])).unwrap()).unwrap();

        let e = s.call("save", &Args::new()).unwrap_err();
        assert_eq!(e["code"], "conflict");
        assert_eq!(e["saved"][0]["slot"], "user");
        assert!(PathBuf::from(e["saved"][0]["backup_path"].as_str().unwrap()).exists());
        assert!(e["message"].as_str().unwrap().contains("character"));

        let v = s.call("save", &args(json!({ "force": true }))).unwrap();
        assert_eq!(v["saved"].as_array().unwrap().len(), 1);
        assert_eq!(v["saved"][0]["slot"], "char");
        assert_eq!(v["skipped"], json!(["user"]));
    }

    #[test]
    fn undo_reverts_an_edit_and_errors_on_an_empty_stack() {
        let (s, _) = open_user(&overview_user_bytes());
        assert_eq!(s.call("undo", &Args::new()).unwrap_err()["code"], "nothing_to_undo");
        ops::set_overview_visible(&s.state, 0, "TYPE", true).unwrap();
        let st = s.call("undo", &Args::new()).unwrap();
        assert_eq!(st["user"]["dirty"], false);
        assert_eq!(st["can_undo"], false);
    }

    #[test]
    fn list_backups_then_restore_puts_the_saved_bytes_back() {
        let (s, path) = open_user(&overview_user_bytes());
        ops::set_overview_visible(&s.state, 0, "TYPE", true).unwrap();
        s.call("save", &Args::new()).unwrap();
        let backups = s.call("list_backups", &args(json!({ "slot": "user" }))).unwrap();
        let newest = backups[0]["path"].as_str().unwrap().to_string();

        let v = s.call("restore_backup", &args(json!({ "slot": "user", "backup_path": newest }))).unwrap();
        assert_eq!(v["path"], json!(path.to_string_lossy()));
        let oc = ops::overview_columns(&s.state).unwrap();
        assert_eq!(oc.tabs[0].columns.iter().filter(|c| c.visible).count(), 1, "TYPE hidden again");
    }

    #[test]
    fn locate_finds_the_char_file_and_its_paired_account_file() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic");
        let profiles = settings_model::discover(&[root]);
        let mut roster = crate::accounts::AccountRoster { accounts: vec![], unassigned: vec![] };
        assert_eq!(locate(1, &profiles, &roster), None);
        let (c, u) = locate(90000001, &profiles, &roster).expect("synthetic char 90000001");
        assert!(c.ends_with("core_char_90000001.dat"));
        assert_eq!(u, None, "no pairing without a roster entry");

        roster.accounts.push(crate::accounts::AccountView { user_id: 80000001, alias: None, characters: vec![90000001] });
        let (_, u) = locate(90000001, &profiles, &roster).unwrap();
        assert!(u.unwrap().ends_with("core_user_80000001.dat"));
    }

    #[test]
    fn status_with_nothing_open_is_empty_and_cannot_undo() {
        let s = EveMcp::for_tests();
        let v = s.call("status", &Args::new()).unwrap();
        assert_eq!(v, json!({ "char": null, "user": null, "can_undo": false }));
    }

    #[test]
    fn an_unknown_tool_is_a_tool_error_the_model_can_read() {
        let s = EveMcp::for_tests();
        let e = s.call("no_such_tool", &Args::new()).unwrap_err();
        assert_eq!(e["code"], "unknown_tool");
    }

    #[test]
    fn a_png_base64_field_becomes_an_image_block_and_leaves_the_text() {
        let v = json!({ "legend": 1, "png_base64": "iVBORw0KGgo=" });
        let result = blocks(v);
        assert_eq!(result.len(), 2);
        match &result[0] {
            ContentBlock::Text(t) => {
                assert!(t.text.contains("\"legend\": 1"));
                assert!(!t.text.contains("png_base64"), "the image data is not repeated as text");
            }
            other => panic!("first block should be text, got {other:?}"),
        }
        match &result[1] {
            ContentBlock::Image(i) => {
                assert_eq!(i.mime_type, "image/png");
                assert_eq!(i.data, "iVBORw0KGgo=");
            }
            other => panic!("second block should be an image, got {other:?}"),
        }
        assert_eq!(blocks(json!({ "a": 1 })).len(), 1, "no image field, one text block");
    }

    #[test]
    fn a_missing_required_argument_names_the_field() {
        let e = req::<String>(&Args::new(), "path").unwrap_err();
        assert_eq!(e["code"], "missing_field");
        assert!(e["message"].as_str().unwrap().contains("`path`"));
    }

    #[test]
    fn a_wrongly_typed_argument_is_bad_arguments() {
        let mut a = Args::new();
        a.insert("tab".into(), json!("two"));
        assert_eq!(req::<i64>(&a, "tab").unwrap_err()["code"], "bad_arguments");
        assert_eq!(opt::<i64>(&a, "missing").unwrap(), None);
    }

    /// The client-compatibility guard from spec §5, as a test: the strictest
    /// MCP clients reject unions, refs, nulls and type arrays in input schemas.
    #[test]
    fn schema_invariants_hold_for_every_tool() {
        let banned = ["oneOf", "anyOf", "allOf", "$ref", "\"null\""];
        let defs = tool_defs();
        assert!(!defs.is_empty());
        for t in &defs {
            assert!(
                t.name.len() <= 40
                    && t.name.starts_with(|c: char| c.is_ascii_lowercase())
                    && t.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "bad tool name {}",
                t.name
            );
            assert!(!t.description.trim().is_empty(), "{} has no description", t.name);
            let s = (t.schema)();
            assert_eq!(s["type"], "object", "{}", t.name);
            assert_eq!(s["additionalProperties"], false, "{}", t.name);
            let props = s["properties"].as_object().unwrap_or_else(|| panic!("{}: properties", t.name));
            for r in s["required"].as_array().unwrap_or_else(|| panic!("{}: required", t.name)) {
                assert!(props.contains_key(r.as_str().unwrap()), "{}: required `{r}` is not a property", t.name);
            }
            let text = s.to_string();
            for b in banned {
                assert!(!text.contains(b), "{}: schema uses {b}", t.name);
            }
            assert_single_string_types(&s, t.name);
        }
        let mut names: Vec<&str> = defs.iter().map(|t| t.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), defs.len(), "duplicate tool name");
    }

    fn assert_single_string_types(v: &serde_json::Value, tool: &str) {
        match v {
            serde_json::Value::Object(m) => {
                if let Some(t) = m.get("type") {
                    assert!(t.is_string(), "{tool}: `type` must be one string, got {t}");
                }
                m.values().for_each(|c| assert_single_string_types(c, tool));
            }
            serde_json::Value::Array(a) => a.iter().for_each(|c| assert_single_string_types(c, tool)),
            _ => {}
        }
    }

    #[test]
    fn primer_has_the_five_topics_in_order_and_none_is_empty() {
        let sections = primer_sections();
        let slugs: Vec<&str> = sections.iter().map(|(s, _)| *s).collect();
        assert_eq!(slugs, TOPICS);
        for (slug, body) in &sections {
            assert!(body.len() > 100, "{slug} is too short to be a primer section");
        }
    }

    #[test]
    fn instructions_are_the_workflow_section() {
        assert_eq!(instructions(), primer("workflow").unwrap());
        assert!(instructions().contains("save"));
    }

    #[test]
    fn eve_guide_returns_a_section_and_rejects_an_unknown_topic() {
        let s = EveMcp::for_tests();
        let mut a = Args::new();
        a.insert("topic".into(), json!("probes"));
        let v = s.call("eve_guide", &a).unwrap();
        assert_eq!(v["topic"], "probes");
        assert!(v["text"].as_str().unwrap().contains("metres"));
        a.insert("topic".into(), json!("mining"));
        assert_eq!(s.call("eve_guide", &a).unwrap_err()["code"], "unknown_topic");
    }

    /// The primer cannot drift from the state catalog the UI ships.
    #[test]
    fn states_section_names_every_catalog_state() {
        let catalog: Value = serde_json::from_str(STATES_JSON).unwrap();
        let states = primer("states").unwrap();
        for (id, label) in catalog["states"].as_object().unwrap() {
            assert!(states.contains(&format!("- {id} — ")), "state {id} ({label}) is missing from the primer");
        }
    }

    #[test]
    fn overview_get_inlines_state_labels_and_group_names() {
        let (s, _) = open_user(&overview_user_bytes());
        ops::preset_fork(&s.state, 0, "Frigs".into(), vec![25, 26], vec![11], vec![]).unwrap();
        let v = s.call("overview_get", &Args::new()).unwrap();
        assert_eq!(v["tabs"][0]["preset"], "Frigs");
        assert_eq!(v["names"]["states"]["11"], "Pilot is in your fleet");
        assert_eq!(v["names"]["groups"]["25"], "Frigate");
        assert_eq!(v["names"]["groups"]["26"], "Cruiser");
    }

    #[test]
    fn overview_get_without_an_account_file_is_no_document() {
        let s = EveMcp::for_tests();
        assert_eq!(s.call("overview_get", &Args::new()).unwrap_err()["code"], "no_document");
    }

    #[test]
    fn groups_search_matches_group_and_category_names_case_insensitively() {
        let s = EveMcp::for_tests();
        let hits = s.call("groups_search", &args(json!({ "query": "FRIGATE", "limit": 100 }))).unwrap();
        let hits = hits.as_array().unwrap();
        assert!(hits.iter().any(|g| g["id"] == 25 && g["name"] == "Frigate" && g["category"] == "Ship"));
        assert!(hits.len() >= 10);
        let cat = s.call("groups_search", &args(json!({ "query": "celestial" }))).unwrap();
        assert!(cat.as_array().unwrap().iter().all(|g| g["category"] == "Celestial"));
        let capped = s.call("groups_search", &args(json!({ "query": "frigate" }))).unwrap();
        assert_eq!(capped.as_array().unwrap().len(), 50, "default limit");
    }

    #[test]
    fn group_catalog_returns_each_id_once_even_when_the_delta_cache_repeats_a_bundled_id() {
        let dir = std::env::temp_dir().join(format!("mcp-groups-dedup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // id 25 ("Frigate") is already in the bundled overview-groups.json;
        // the delta cache can also carry it (a widened delta over time).
        std::fs::write(
            dir.join("groups-cache.json"),
            r#"{"version": null, "groups": {"25": {"id": 25, "name": "Frigate", "category_id": 6, "category_name": "Ship"}}}"#,
        )
        .unwrap();
        let rows = group_catalog(&dir);
        // `Vec::dedup` only removes ADJACENT duplicates; the bundled id 25
        // sits mid-catalog and the cache copy is appended at the end, so a
        // set comparison is the check that actually fails without the
        // production sort+dedup.
        let ids: std::collections::HashSet<i64> = rows.iter().map(|g| g.id).collect();
        assert_eq!(ids.len(), rows.len(), "duplicate id in {rows:?}");
        assert_eq!(rows.iter().filter(|g| g.id == 25).count(), 1, "id 25 must appear exactly once: {rows:?}");
    }

    #[test]
    fn builtin_presets_lists_43_and_returns_one_by_display_name() {
        let s = EveMcp::for_tests();
        let all = s.call("builtin_presets", &Args::new()).unwrap();
        assert_eq!(all.as_array().unwrap().len(), 43);
        assert!(all.as_array().unwrap().iter().any(|p| p["display_name"] == "Friendly: Fleet" && p["era"] == "modern"));
        let fleet = s.call("builtin_presets", &args(json!({ "name": "friendly: fleet" }))).unwrap();
        assert_eq!(fleet["name"], "Fleet");
        assert!(fleet["groups"].as_array().unwrap().contains(&json!(25)));
        assert_eq!(s.call("builtin_presets", &args(json!({ "name": "nope" }))).unwrap_err()["code"], "unknown_preset");
        assert_eq!(s.call("builtin_presets", &args(json!({ "name": "All" }))).unwrap_err()["code"], "ambiguous_preset");
    }

    #[test]
    fn characters_lists_each_profile_with_names_pairings_and_unpaired_accounts() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic");
        let profiles = settings_model::discover(&[root]);
        let roster = crate::accounts::AccountRoster {
            accounts: vec![crate::accounts::AccountView { user_id: 80000001, alias: Some("main".into()), characters: vec![90000001] }],
            unassigned: vec![80000002, 80000003, 80000004],
        };
        let mut names = crate::names::Cache::new();
        names.insert(90000001, crate::names::ResolvedName { name: "Synthetic One".into(), category: "character".into() });

        let v = characters(&profiles, &roster, &names);
        let p = &v["profiles"][0];
        assert_eq!(p["profile"], "Default");
        let chars = p["characters"].as_array().unwrap();
        assert_eq!(chars.len(), 3);
        let one = chars.iter().find(|c| c["char_id"] == 90000001).unwrap();
        assert_eq!(one["name"], "Synthetic One");
        assert_eq!(one["user_id"], 80000001);
        assert_eq!(one["account_alias"], "main");
        assert!(one["user_file"].as_str().unwrap().ends_with("core_user_80000001.dat"));
        let two = chars.iter().find(|c| c["char_id"] == 90000002).unwrap();
        assert_eq!(two["name"], Value::Null);
        assert_eq!(two["user_file"], Value::Null);
        let unpaired = p["unpaired_accounts"].as_array().unwrap();
        assert_eq!(unpaired.len(), 3, "80000002..4 have no character");
    }

    /// `undo::group` must be re-entrant: the batched tools wrap ops that open
    /// their own group (copy_columns, tab_delete, window_add/remove), and an
    /// inner guard's drop must not close the batch's group.
    #[test]
    fn a_nested_undo_group_rides_the_outer_one() {
        let (s, _) = open_user(&overview_user_bytes());
        let before = undo::undo_state(&s.state).depth;
        {
            let _outer = undo::group(&s.state);
            ops::set_overview_visible(&s.state, 0, "TYPE", true).unwrap();
            {
                let _inner = undo::group(&s.state);
                ops::set_overview_order(&s.state, 0, vec!["TYPE".into(), "NAME".into()]).unwrap();
            }
            ops::set_overview_visible(&s.state, 0, "TYPE", false).unwrap();
        }
        assert_eq!(undo::undo_state(&s.state).depth, before + 1, "three writes under one group, one entry");
    }

    fn visible_count(v: &Value) -> usize {
        v["tabs"][0]["columns"].as_array().unwrap().iter().filter(|c| c["visible"] == true).count()
    }

    #[test]
    fn columns_edit_applies_a_batch_as_one_undo_step() {
        let (s, _) = open_user(&overview_user_bytes());
        let before = undo::undo_state(&s.state).depth;
        let v = s.call("overview_columns_edit", &args(json!({ "ops": [
            { "op": "set_visible", "tab": 0, "column": "TYPE", "visible": true },
            { "op": "set_order", "tab": 0, "order": ["TYPE", "NAME"] }
        ]}))).unwrap();
        assert_eq!(visible_count(&v), 2);
        assert_eq!(v["tabs"][0]["columns"][0]["name"], "TYPE");
        assert_eq!(undo::undo_state(&s.state).depth, before + 1);
        assert!(v["names"].is_object(), "edits return the same self-describing shape as overview_get");
    }

    #[test]
    fn a_failing_op_rolls_the_whole_batch_back_and_names_its_index() {
        let (s, _) = open_user(&overview_user_bytes());
        let before = s.call("overview_get", &Args::new()).unwrap();
        let depth = undo::undo_state(&s.state).depth;
        let e = s.call("overview_columns_edit", &args(json!({ "ops": [
            { "op": "set_visible", "tab": 0, "column": "TYPE", "visible": true },
            { "op": "set_visible", "tab": 99, "column": "TYPE", "visible": true }
        ]}))).unwrap_err();
        // An unknown COLUMN is not an error (`set_column_visible` adds it); an
        // unknown TAB is `NoTab`, so that is the failing op.
        assert_eq!(e["op_index"], 1);
        assert_eq!(s.call("overview_get", &Args::new()).unwrap(), before);
        assert_eq!(undo::undo_state(&s.state).depth, depth);
    }

    #[test]
    fn a_missing_op_field_names_the_op_and_field() {
        let (s, _) = open_user(&overview_user_bytes());
        let e = s.call("overview_columns_edit", &args(json!({ "ops": [{ "op": "set_visible", "tab": 0 }] }))).unwrap_err();
        assert_eq!(e["code"], "missing_field");
        assert_eq!(e["op_index"], 0);
        let e = s.call("overview_columns_edit", &args(json!({ "ops": [{ "op": "explode" }] }))).unwrap_err();
        assert_eq!(e["code"], "bad_arguments");
    }

    #[test]
    fn presets_edit_forks_a_builtin_onto_the_tab() {
        let (s, _) = open_user(&overview_user_bytes());
        let fleet = s.call("builtin_presets", &args(json!({ "name": "Friendly: Fleet" }))).unwrap();
        let v = s.call("overview_presets_edit", &args(json!({ "ops": [{
            "op": "fork", "tab": 0, "name": "My Fleet",
            "groups": fleet["groups"], "filtered_states": fleet["filtered_states"], "always_shown_states": fleet["always_shown_states"]
        }]}))).unwrap();
        assert_eq!(v["tabs"][0]["preset"], "My Fleet");
        let p = v["presets"].as_array().unwrap().iter().find(|p| p["name"] == "My Fleet").unwrap();
        assert_eq!(p["groups"], fleet["groups"]);
        assert_eq!(p["filtered_states"], fleet["filtered_states"]);
    }

    #[test]
    fn tabs_edit_creates_renames_and_deletes_in_one_batch() {
        let (s, _) = open_user(&overview_user_bytes());
        ops::overview_create_window_mapping(&s.state).unwrap();
        let v = s.call("overview_tabs_edit", &args(json!({ "ops": [
            { "op": "create", "window": 0, "name": "Mining", "from_tab": 0 },
            { "op": "rename", "tab": 1, "name": "Rocks" }
        ]}))).unwrap();
        assert_eq!(v["tabs"].as_array().unwrap().len(), 2);
        assert_eq!(v["tabs"][1]["name"], "Rocks");
        let v = s.call("overview_tabs_edit", &args(json!({ "ops": [{ "op": "delete", "tab": 1 }] }))).unwrap();
        assert_eq!(v["tabs"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn appearance_edit_sets_states_and_a_colour_then_clears_it() {
        let (s, _) = open_user(&overview_user_bytes());
        let v = s.call("overview_appearance_edit", &args(json!({ "ops": [
            { "op": "set_states", "list": "background", "ids": [11, 13] },
            { "op": "set_state_color", "surface": "background", "id": 13, "rgba": [1.0, 0.0, 0.0, 1.0] },
            { "op": "set_bool", "key": "useSmallText", "on": true }
        ]}))).unwrap();
        assert_eq!(v["appearance"]["background"]["enabled"], json!([11, 13]));
        assert!(v["appearance"]["colors"].as_array().unwrap().iter().any(|c| c[0] == 13));
        assert!(v["appearance"]["bools"].as_array().unwrap().iter().any(|b| b[0] == "useSmallText" && b[1] == true));
        let v = s.call("overview_appearance_edit", &args(json!({ "ops": [
            { "op": "set_state_color", "surface": "background", "id": 13 }
        ]}))).unwrap();
        assert!(!v["appearance"]["colors"].as_array().unwrap().iter().any(|c| c[0] == 13));
    }

    #[test]
    fn a_parse_error_after_a_successful_op_still_rolls_the_batch_back() {
        let (s, _) = open_user(&overview_user_bytes());
        let before = s.call("overview_get", &Args::new()).unwrap();
        let e = s.call("overview_columns_edit", &args(json!({ "ops": [
            { "op": "set_visible", "tab": 0, "column": "TYPE", "visible": true },
            { "op": "set_order", "tab": 0 }
        ]}))).unwrap_err();
        assert_eq!(e["code"], "missing_field");
        assert_eq!(s.call("overview_get", &Args::new()).unwrap(), before);
    }

    fn empty_ui_bytes() -> Vec<u8> {
        encode(&BmValue::Dict(vec![(b("ui"), BmValue::Dict(vec![]))])).unwrap()
    }

    /// A character file with empty `windows` and `ui` sections — enough for
    /// the HUD projection to report every field absent and mint on write.
    fn hud_char_bytes() -> Vec<u8> {
        encode(&BmValue::Dict(vec![(b("windows"), BmValue::Dict(vec![])), (b("ui"), BmValue::Dict(vec![]))])).unwrap()
    }

    /// Like `open_user`, for the character slot. `temp_file` names every file
    /// core_user_5.dat; the name does not matter to `open_file`.
    fn open_char(bytes: &[u8]) -> (EveMcp, PathBuf) {
        let path = temp_file("mcp-char", bytes);
        let s = EveMcp::for_tests();
        ops::open_file(&s.state, Slot::Char, path.to_str().unwrap()).unwrap();
        (s, path)
    }

    fn assert_no_paths(v: &Value, tool: &str) {
        match v {
            Value::Object(m) => {
                for (k, c) in m {
                    assert!(!k.ends_with("_path") && k != "set" && k != "path" || tool.starts_with("copy") || tool == "save" || tool == "open", "{tool}: key `{k}` leaks a path");
                    assert_no_paths(c, tool);
                }
            }
            Value::Array(a) => a.iter().for_each(|c| assert_no_paths(c, tool)),
            _ => {}
        }
    }

    #[test]
    fn hud_get_strips_paths_and_hud_set_mints_a_field() {
        let (s, _) = open_char(&hud_char_bytes());
        let v = s.call("hud_get", &Args::new()).unwrap();
        assert_no_paths(&v, "hud_get");
        let ship = v["entries"].as_array().unwrap().iter().find(|e| e["name"] == "ship_offset").unwrap();
        assert_eq!(ship["value"], Value::Null);
        assert_eq!(ship["scope"], "char");
        let v = s.call("hud_set", &args(json!({ "name": "ship_offset", "value": "-77" }))).unwrap();
        let ship = v["entries"].as_array().unwrap().iter().find(|e| e["name"] == "ship_offset").unwrap();
        assert_eq!(ship["value"], "-77");
        assert!(s.call("hud_set", &args(json!({ "name": "no_such_field", "value": "1" }))).is_err());
    }

    #[test]
    fn hud_get_without_a_character_file_is_no_document() {
        let s = EveMcp::for_tests();
        assert_eq!(s.call("hud_get", &Args::new()).unwrap_err()["code"], "no_document");
    }

    #[test]
    fn chat_get_is_empty_on_a_bare_account_and_set_splits_mints_both_keys() {
        let (s, _) = open_user(&empty_ui_bytes());
        assert_eq!(s.call("chat_get", &Args::new()).unwrap(), json!([]));
        let v = s.call("chat_set_splits", &args(json!({ "window_ids": ["chatchannel_local"], "userlist_width": 135, "input_height": 64 }))).unwrap();
        let p = v.as_array().unwrap().iter().find(|p| p["window_id"] == "chatchannel_local").unwrap();
        assert_eq!(p["userlist_width"], 135);
        assert_eq!(p["input_height"], 64);
        assert_eq!(s.call("chat_set_splits", &args(json!({ "window_ids": ["chatchannel_local"] }))).unwrap_err()["code"], "missing_field");
    }

    #[test]
    fn probes_set_creates_at_the_next_free_id_and_get_shows_it() {
        let (s, _) = open_user(&empty_ui_bytes());
        let v = s.call("probes_set", &args(json!({ "name": "pair", "probes": [[1.0, 0.0, 0.0], [2.0, 0.0, 0.0]], "ranges": [1000.0, 2000.0] }))).unwrap();
        assert_eq!(v["formations"][0]["id"], 0);
        assert_eq!(v["formations"][0]["name"], "pair");
        let v = s.call("probes_set", &args(json!({ "id": 0, "name": "renamed", "probes": [[1.0, 0.0, 0.0]], "ranges": [1000.0] }))).unwrap();
        assert_eq!(v["formations"].as_array().unwrap().len(), 1);
        assert_eq!(s.call("probes_get", &Args::new()).unwrap()["formations"][0]["name"], "renamed");
    }

    #[test]
    fn probes_set_with_nine_probes_is_a_probe_error() {
        let (s, _) = open_user(&empty_ui_bytes());
        let e = s.call("probes_set", &args(json!({ "name": "nine", "probes": vec![[0.0, 0.0, 0.0]; 9], "ranges": vec![1.0; 9] }))).unwrap_err();
        assert!(e["message"].as_str().unwrap().contains("between 1 and 8"), "{e}");
    }

    #[test]
    fn probes_export_yaml_then_add_yaml_round_trips_and_reorder_remove_work() {
        let (s, _) = open_user(&empty_ui_bytes());
        s.call("probes_set", &args(json!({ "name": "a", "probes": [[1.0, 0.0, 0.0]], "ranges": [1000.0] }))).unwrap();
        s.call("probes_set", &args(json!({ "name": "b", "probes": [[0.0, 1.0, 0.0]], "ranges": [2000.0] }))).unwrap();
        let yaml = s.call("probes_export_yaml", &Args::new()).unwrap()["yaml"].as_str().unwrap().to_string();
        assert!(yaml.contains("name: 'a'"));

        let v = s.call("probes_add_yaml", &args(json!({ "yaml": yaml }))).unwrap();
        let names: Vec<&str> = v["formations"].as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap()).collect();
        assert_eq!(names.len(), 4, "two added, names de-duplicated: {names:?}");

        let v = s.call("probes_reorder", &args(json!({ "order": [3, 2, 1, 0] }))).unwrap();
        assert_eq!(v["formations"][0]["name"], names[3]);
        let v = s.call("probes_remove", &args(json!({ "id": 0 }))).unwrap();
        assert_eq!(v["formations"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn probes_add_yaml_rejects_text_that_is_not_a_formation_file() {
        let (s, _) = open_user(&empty_ui_bytes());
        assert!(s.call("probes_add_yaml", &args(json!({ "yaml": "not: [a formation" }))).is_err());
    }

    #[test]
    fn pack_preview_of_a_missing_file_is_an_error_and_export_then_preview_round_trips() {
        let (s, _) = open_user(&overview_user_bytes());
        assert!(s.call("overview_pack_preview", &args(json!({ "path": "Z:/nope.yaml" }))).is_err());
        ops::preset_fork(&s.state, 0, "Frigs".into(), vec![25], vec![], vec![]).unwrap();
        let out = std::env::temp_dir().join(format!("mcp-pack-{}.yaml", std::process::id()));
        s.call("overview_pack_export", &args(json!({ "path": out.to_string_lossy() }))).unwrap();
        let preview = s.call("overview_pack_preview", &args(json!({ "path": out.to_string_lossy() }))).unwrap();
        // `PackSummary` carries section names and counts, not preset names --
        // the pack's "presets" section is what proves the fork made it in.
        assert!(
            preview["sections"].as_array().unwrap().iter().any(|s| s[0] == "presets" && s[1] == 1),
            "{preview}"
        );
        let v = s.call("overview_pack_import", &args(json!({ "path": out.to_string_lossy() }))).unwrap();
        assert!(v["columns"]["names"].is_object(), "the same self-describing shape as overview_get: {v}");
    }

    /// root -> ui -> editHistory -> (ts, { "/a/box": ["Jita", "Amarr"] }) —
    /// `ops::tests::autofill_user_bytes`'s shape.
    fn autofill_user_bytes() -> Vec<u8> {
        let hist = BmValue::Dict(vec![(b("/a/box"), BmValue::List(vec![BmValue::Str("Jita".into()), BmValue::Str("Amarr".into())]))]);
        let ui = BmValue::Dict(vec![(b("editHistory"), BmValue::Tuple(vec![BmValue::Long(vec![0u8; 8]), hist]))]);
        encode(&BmValue::Dict(vec![(b("ui"), ui)])).unwrap()
    }

    /// root -> cmd -> customCmds -> (ts, { command: codes }) — the model
    /// crate's `user_with_binds` shape: `cmd` bare, leaves bare tuples.
    fn keybinds_user_bytes() -> Vec<u8> {
        let codes = |v: &[i64]| BmValue::Tuple(v.iter().map(|&n| BmValue::Int(n)).collect());
        let table = BmValue::Dict(vec![
            (b("CmdActivateHighPowerSlot1"), codes(&[81])),
            (b("CmdActivateMediumPowerSlot1"), codes(&[17, 83])),
            (b("CmdToggleAutopilot"), BmValue::None),
        ]);
        let cmd = BmValue::Dict(vec![(b("customCmds"), BmValue::Tuple(vec![BmValue::Long(vec![0u8; 8]), table]))]);
        encode(&BmValue::Dict(vec![(b("cmd"), cmd)])).unwrap()
    }

    #[test]
    fn autofill_get_set_and_clear_all() {
        let (s, _) = open_user(&autofill_user_bytes());
        let v = s.call("autofill_get", &Args::new()).unwrap();
        assert_eq!(v[0]["widget"], "/a/box");
        assert_eq!(v[0]["entries"], json!(["Jita", "Amarr"]));
        let v = s.call("autofill_set", &args(json!({ "widget": "/a/box", "entries": ["Dodixie"] }))).unwrap();
        assert_eq!(v[0]["entries"], json!(["Dodixie"]));
        let v = s.call("autofill_clear_all", &Args::new()).unwrap();
        assert!(v.as_array().unwrap().iter().all(|l| l["entries"].as_array().unwrap().is_empty()));
        assert_eq!(s.call("status", &Args::new()).unwrap()["user"]["dirty"], true);
    }

    #[test]
    fn keybinds_get_labels_commands_and_renders_combos() {
        let (s, _) = open_user(&keybinds_user_bytes());
        let v = s.call("keybinds_get", &Args::new()).unwrap();
        assert_eq!(v["available"], true);
        let e = v["entries"].as_array().unwrap();
        let high = e.iter().find(|x| x["command"] == "CmdActivateHighPowerSlot1").unwrap();
        assert_eq!(high["label"], "Activate High Power Slot 1");
        assert_eq!(high["group"], "Modules");
        assert_eq!(high["combo"], "Q");
        assert_eq!(high["keys"], json!([81]));
        let med = e.iter().find(|x| x["command"] == "CmdActivateMediumPowerSlot1").unwrap();
        assert_eq!(med["combo"], "Ctrl+S");
        let ap = e.iter().find(|x| x["command"] == "CmdToggleAutopilot").unwrap();
        assert_eq!(ap["combo"], Value::Null);
        assert_eq!(ap["keys"], Value::Null);
    }

    #[test]
    fn keybind_set_binds_by_name_steals_and_unbinds() {
        let (s, _) = open_user(&keybinds_user_bytes());
        // Ctrl+S is CmdActivateMediumPowerSlot1's; giving it to autopilot steals it.
        let v = s.call("keybind_set", &args(json!({ "command": "CmdToggleAutopilot", "key": "s", "ctrl": true }))).unwrap();
        assert_eq!(v["stolen"], json!(["CmdActivateMediumPowerSlot1"]));
        let e = v["keybinds"]["entries"].as_array().unwrap();
        assert_eq!(e.iter().find(|x| x["command"] == "CmdToggleAutopilot").unwrap()["keys"], json!([17, 83]));
        let v = s.call("keybind_set", &args(json!({ "command": "CmdToggleAutopilot" }))).unwrap();
        let e = v["keybinds"]["entries"].as_array().unwrap();
        assert_eq!(e.iter().find(|x| x["command"] == "CmdToggleAutopilot").unwrap()["keys"], Value::Null);
        let err = s.call("keybind_set", &args(json!({ "command": "CmdToggleAutopilot", "key": "Hyper" }))).unwrap_err();
        assert_eq!(err["code"], "unknown_key");
    }

    #[test]
    fn combo_label_orders_modifiers_and_names_unknown_codes() {
        assert_eq!(combo_label(&[17, 18, 16, 68]), "Ctrl+Alt+Shift+D");
        assert_eq!(combo_label(&[999]), "VK999");
    }
}
