//! MCP server — the `--mcp` mode of the same executable. A second adapter over
//! `ops`, beside the Tauri commands in lib.rs: every tool is a plain function
//! over `AppState`, and this file only parses arguments and serialises results.
//! Spec: docs/superpowers/specs/2026-09-19-mcp-server-design.md.
//!
//! stdout IS the protocol here. Nothing in this process may print to it.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{ErrorData as McpError, ServerHandler, ServiceExt};
use serde_json::{json, Map, Value};

use settings_model::{discover, FileKind, Mutation, NewValue, OverviewColumns, Profile, SetTarget, WindowLayout, WindowRect};

use crate::accounts::{self, AccountRoster};
use crate::groups;
use crate::mcp_filter::{self, Env, HiddenCounts, Overrides, WindowFilter};
use crate::names;
use crate::ops::{self, AppState, ErrDto, OpenOutcome, Slot};
use crate::prefs;
use crate::undo;

pub struct EveMcp {
    /// One workspace per account file, by canonical `core_user` path
    /// (spec §3.1). `Arc` so PR 2 can share it between connections.
    workspaces: Arc<Mutex<HashMap<PathBuf, AppState>>>,
    /// What `open` last selected. `None` until then; `state()` lends a
    /// scratch workspace so an early tool fails with `no_document` as before.
    current: Mutex<Option<AppState>>,
    /// The app dir — names cache, accounts.json, groups cache. Shared with
    /// the window, read-mostly.
    pub dir: PathBuf,
    /// EVE settings roots to discover profiles under (`default_roots()` in
    /// production; a fixture dir in tests).
    pub roots: Vec<PathBuf>,
    /// Where preferences.json lives — the user's own clutter overrides
    /// (`prefs::path_base()` in production; `None`, or a private path a test
    /// points at, otherwise).
    pub prefs_path: Option<PathBuf>,
}

impl EveMcp {
    pub fn new(dir: PathBuf, roots: Vec<PathBuf>, prefs_path: Option<PathBuf>) -> Self {
        EveMcp { workspaces: Arc::default(), current: Mutex::new(None), dir, roots, prefs_path }
    }

    /// The workspace tools act on: the one `open` last selected, or a scratch
    /// workspace before any `open`. Cheap — an `Arc` clone.
    fn state(&self) -> AppState {
        self.current.lock().unwrap().get_or_insert_with(AppState::new).clone()
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

/// The bundled neocom button catalog (`{id, btnType, iconPath}`), the base
/// the UI unions with a file's stale `Original` snapshot.
const NEOCOM_JSON: &str = include_str!("../../src/lib/data/neocom-buttons.json");

/// Parsed once per process — every call reads the same JSON — and cached
/// behind a `OnceLock` rather than re-parsed on every `combo_label`/`vk_code`
/// call, which `keybinds_get` makes once per bound command in the file.
fn vk_labels() -> &'static HashMap<i64, String> {
    static CACHE: std::sync::OnceLock<HashMap<i64, String>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let raw: HashMap<String, String> = serde_json::from_str(VK_LABELS_JSON).expect("vk-labels.json");
        raw.into_iter().filter_map(|(k, v)| Some((k.parse().ok()?, v))).collect()
    })
}

/// A key by the name a person types — "Q", "f1", "page up" — or `None`.
fn vk_code(name: &str) -> Option<i64> {
    let want = name.trim();
    vk_labels().iter().find(|(_, label)| label.eq_ignore_ascii_case(want)).map(|(&code, _)| code)
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

pub(crate) const TOPICS: [&str; 8] =
    ["workflow", "overview", "presets", "states", "probes", "layout", "keybinds", "copy"];

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

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
struct NeocomEntry { id: String, btn_type: i64, icon_path: String }

/// Bundled ∪ the file's Original, by id; the bundled entry wins a conflict.
fn neocom_available(bar: &settings_model::NeocomBar) -> Vec<NeocomEntry> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Raw { id: String, btn_type: i64, icon_path: String }
    let bundled: Vec<Raw> = serde_json::from_str(NEOCOM_JSON).expect("neocom-buttons.json");
    let mut out: Vec<NeocomEntry> = bundled.into_iter().map(|r| NeocomEntry { id: r.id, btn_type: r.btn_type, icon_path: r.icon_path }).collect();
    for o in &bar.original {
        if !out.iter().any(|e| e.id == o.id) {
            out.push(NeocomEntry { id: o.id.clone(), btn_type: o.btn_type, icon_path: o.icon_path.clone() });
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn neocom_view(bar: &settings_model::NeocomBar) -> Value {
    json!({ "buttons": bar.buttons, "available": neocom_available(bar) })
}

fn neocom_op(state: &AppState, a: &Args) -> Result<(), Value> {
    let op: String = req(a, "op")?;
    match op.as_str() {
        "reorder" => ops::neocom_reorder(state, req(a, "order")?),
        "remove" => ops::neocom_remove(state, req(a, "index")?),
        "add" => {
            let id: String = req(a, "id")?;
            let bar = ops::neocom_bar(state).map_err(fail)?;
            let e = neocom_available(&bar).into_iter().find(|e| e.id == id)
                .ok_or_else(|| err("unknown_button", format!("no button `{id}`; ids are neocom_get's available list")))?;
            ops::neocom_add(state, &e.id, e.btn_type, &e.icon_path)
        }
        "reset" => ops::neocom_reset(state),
        _ => return Err(unknown_op(&op)),
    }
    .map(drop)
    .map_err(fail)
}

/// The fleet projection without `set` targets, watch-list ids named from the
/// cache when known, colours as {state, rgb?, default?}.
fn fleet_view(f: &settings_model::Fleet, names: &names::Cache) -> Value {
    let colours: Vec<Value> = f.colours.iter().map(|c| {
        let (state, rgb) = match &c.state {
            settings_model::Colour::Absent => ("absent", None),
            settings_model::Colour::Cleared => ("cleared", None),
            settings_model::Colour::Set { rgb } => ("set", Some(*rgb)),
            settings_model::Colour::Unreadable => ("unreadable", None),
        };
        json!({ "broadcast": c.broadcast, "state": state, "rgb": rgb, "default": c.default })
    }).collect();
    let watchlist: Vec<Value> = f.watchlist.iter().map(|w| json!({
        "char_id": w.char_id, "name": names.get(&w.char_id).map(|n| n.name.clone()), "rgb": w.rgb,
    })).collect();
    json!({
        "fields": f.fields.iter().map(hud_entry_view).collect::<Vec<_>>(),
        "colours": colours, "watchlist": watchlist,
        "palette": f.palette.iter().map(|(n, rgb)| json!({ "name": n, "rgb": rgb })).collect::<Vec<_>>(),
        "char_open": f.char_open, "user_open": f.user_open,
    })
}

fn fleet_op(state: &AppState, a: &Args) -> Result<(), Value> {
    let op: String = req(a, "op")?;
    match op.as_str() {
        "set_field" => ops::set_fleet_field(state, &req::<String>(a, "name")?, &req::<String>(a, "value")?),
        "set_colour" => ops::set_fleet_colour(state, &req::<String>(a, "broadcast")?, opt(a, "rgb")?),
        "set_watchlist_colour" => ops::set_watchlist_colour(state, req(a, "char_id")?, opt(a, "rgb")?),
        _ => return Err(unknown_op(&op)),
    }
    .map(drop)
    .map_err(fail)
}

/// The layout without a single path: what the model sees. A real character
/// file runs to ~381 windows, ~1.3 KB each pretty-printed and most of that
/// the old per-window `flags` array — ~495 KB total, well past a client's
/// tool-result cap — so closed and clutter windows are dropped by default
/// (see `mcp_filter`), and `flags` is just the names that are ON rather than
/// a {name,value,settable} triple per flag. `settable_flags` is file-level (a
/// flag dict is present or absent for the whole file, never per window), so
/// it is read off any one window — the UNFILTERED set, a view filter must not
/// change what a file can set.
fn layout_view(wl: &WindowLayout, f: &WindowFilter, o: &Overrides) -> Value {
    // A flag is settable per FILE (its dict is present or absent for the
    // whole file), but one window alone can under-report it: a window whose
    // own key can't be reconstructed as a mintable dict key reads Unavailable
    // for every flag regardless of whether the file's dict exists
    // (`windows.rs`'s `bool_flag`/`key_as_new_value`). The union over every
    // window is what a single window's flags cannot lie about.
    let settable_flags: Vec<String> = wl.windows.iter()
        .flat_map(|w| w.flags.iter().filter(|f| !matches!(f.set, SetTarget::Unavailable)).map(|f| f.name.clone()))
        .collect::<std::collections::BTreeSet<String>>()
        .into_iter()
        .collect();
    let mut hidden = HiddenCounts::default();
    let windows: Vec<Value> = wl.windows.iter()
        .filter(|w| match mcp_filter::hidden_by(w, f, o) {
            Some(reason) => { hidden.add(reason); false }
            None => true,
        })
        .map(|w| json!({
            "id": w.id, "label": w.label, "name": w.name, "open": w.open, "renderable": w.renderable,
            "resolution_matches": w.resolution_matches,
            "geom": w.geom.as_ref().map(|g| json!({ "x": g.x, "y": g.y, "w": g.w, "h": g.h, "screen_w": g.screen_w, "screen_h": g.screen_h })),
            "flags": w.flags.iter().filter(|f| f.value).map(|f| &f.name).collect::<Vec<_>>(),
            "stack": w.stack.as_ref().map(|s| json!({ "container_id": s.container_id, "role": s.role })),
        })).collect();
    json!({
        "reference_w": wl.reference_w, "reference_h": wl.reference_h,
        "windows": windows, "stacks": wl.stacks, "settable_flags": settable_flags,
        "hidden": hidden,
    })
}

fn find_window<'a>(wl: &'a WindowLayout, id: &str) -> Result<&'a WindowRect, Value> {
    wl.windows.iter().find(|w| w.id == id).ok_or_else(|| err("unknown_window", format!("no window `{id}`; ids are layout_get's")))
}

fn set_int(path: &settings_model::NodePath, v: i64) -> Mutation {
    Mutation::SetScalar { path: path.clone(), text: v.to_string() }
}

/// One window's share of a `set_geometry`: a `set_scalar` per changed axis,
/// plus the reference screen size when its own differs. `None` (no `geom`)
/// contributes nothing — a stack member the file never gave a rect is simply
/// skipped, not an error, since it is the target window's own `no_geometry`
/// check that gates the op.
fn one_window_geometry_mutations(wl: &WindowLayout, win: &WindowRect, x: Option<i64>, y: Option<i64>, w: Option<i64>, h: Option<i64>) -> Vec<Mutation> {
    let Some(g) = win.geom.as_ref() else { return Vec::new() };
    let mut ms = Vec::new();
    for (next, cur, path) in [(x, g.x, &g.x_path), (y, g.y, &g.y_path), (w, g.w, &g.w_path), (h, g.h, &g.h_path)] {
        if let Some(n) = next {
            if n != cur { ms.push(set_int(path, n)); }
        }
    }
    if !ms.is_empty() && !win.resolution_matches {
        ms.push(set_int(&g.screen_w_path, wl.reference_w));
        ms.push(set_int(&g.screen_h_path, wl.reference_h));
    }
    ms
}

/// `LayoutView.svelte`'s `geomMutations`, in Rust — fanned out over a stack.
/// EVE keeps a stack's container and every member at one identical rect
/// (docs/format-notes.md ~line 735: the canvas drags one rectangle over all
/// three), so moving a stacked window must write the same axes to the whole
/// stack — members plus the container id, deduplicated, the target included —
/// not just the window asked for.
fn geometry_mutations(wl: &WindowLayout, id: &str, x: Option<i64>, y: Option<i64>, w: Option<i64>, h: Option<i64>) -> Result<Vec<Mutation>, Value> {
    let win = find_window(wl, id)?;
    if win.geom.is_none() {
        return Err(err("no_geometry", format!("window `{id}` has no stored geometry")));
    }
    let mut ids = vec![id.to_string()];
    if let Some(sref) = &win.stack {
        if let Some(stack) = wl.stacks.iter().find(|s| s.container_id == sref.container_id) {
            ids.push(stack.container_id.clone());
            ids.extend(stack.members.iter().cloned());
        }
    }
    let mut seen = HashSet::new();
    let mut ms = Vec::new();
    for wid in ids {
        if !seen.insert(wid.clone()) { continue; }
        if let Some(target) = wl.windows.iter().find(|w| w.id == wid) {
            ms.extend(one_window_geometry_mutations(wl, target, x, y, w, h));
        }
    }
    Ok(ms)
}

/// `flagMutation`'s twin.
fn flag_mutation(wl: &WindowLayout, id: &str, flag: &str, on: bool) -> Result<Mutation, Value> {
    let win = find_window(wl, id)?;
    let f = win.flags.iter().find(|f| f.name == flag).ok_or_else(|| {
        let names: Vec<&str> = win.flags.iter().map(|f| f.name.as_str()).collect();
        err("unknown_flag", format!("window `{id}` has no flag `{flag}`; it has {}", names.join(", ")))
    })?;
    match &f.set {
        SetTarget::Set { path } => Ok(Mutation::SetScalar { path: path.clone(), text: on.to_string() }),
        SetTarget::Insert { parent, key } => {
            // NewValue is not Clone; a serde round trip copies it.
            let key: NewValue = serde_json::from_value(serde_json::to_value(key).unwrap()).unwrap();
            Ok(Mutation::InsertDictEntry { parent: parent.clone(), key, value: NewValue::Bool(on) })
        }
        SetTarget::Unavailable => Err(err("flag_unavailable", format!("`{flag}` cannot be set on `{id}` in this file"))),
    }
}

fn layout_op(state: &AppState, a: &Args) -> Result<(), Value> {
    let op: String = req(a, "op")?;
    match op.as_str() {
        "set_geometry" => {
            let id: String = req(a, "window")?;
            let (x, y, w, h) = (opt(a, "x")?, opt(a, "y")?, opt(a, "w")?, opt(a, "h")?);
            if x.is_none() && y.is_none() && w.is_none() && h.is_none() {
                return Err(err("missing_field", "set_geometry needs at least one of x, y, w, h"));
            }
            let wl = ops::window_layout(state, Slot::Char).map_err(fail)?;
            let ms = geometry_mutations(&wl, &id, x, y, w, h)?;
            if ms.is_empty() { return Ok(()); }
            ops::apply_mutations(state, Slot::Char, &ms).map(drop).map_err(fail)
        }
        "set_flag" => {
            let wl = ops::window_layout(state, Slot::Char).map_err(fail)?;
            let m = flag_mutation(&wl, &req::<String>(a, "window")?, &req::<String>(a, "flag")?, req(a, "on")?)?;
            ops::apply_mutations(state, Slot::Char, &[m]).map(drop).map_err(fail)
        }
        "stack_create" => ops::stack_create(state, &req::<String>(a, "a")?, &req::<String>(a, "b")?).map(drop).map_err(fail),
        "stack_add" => ops::stack_add(state, &req::<String>(a, "window")?, &req::<String>(a, "container")?).map(drop).map_err(fail),
        "stack_unstack" => ops::stack_unstack(state, &req::<String>(a, "window")?).map(drop).map_err(fail),
        "stack_reorder" => ops::stack_reorder(state, &req::<String>(a, "container")?, req(a, "members")?).map(drop).map_err(fail),
        "stack_delete_orphans" => ops::stack_delete_orphans(state).map(drop).map_err(fail),
        _ => Err(unknown_op(&op)),
    }
}

fn lookup_result(r: Result<Option<names::Found>, names::FetchError>) -> ToolResult {
    match r {
        Ok(Some(f)) => Ok(json!({ "id": f.id, "name": f.name })),
        Ok(None) => Err(err("not_found", "no character by that name or id")),
        Err(e) => Err(err("esi", format!("ESI lookup failed: {}", e.0))),
    }
}

impl EveMcp {
    fn fleet_get(&self) -> ToolResult {
        let f = ops::fleet_settings(&self.state()).map_err(fail)?;
        // Names from the cache only — no network on a read. `resolve_blocking`
        // with an empty id list is NOT "the whole cache": it selects only the
        // ids passed in (here, none), so it always comes back empty regardless
        // of what is on disk. `load_cache` is the actual cache-only accessor —
        // already `pub`, so no visibility change was needed for it.
        let names = names::load_cache(&self.dir);
        Ok(fleet_view(&f, &names))
    }

    /// The user's own clutter overrides, from preferences.json — empty when
    /// there is no prefs path (a test that never sets one), no file yet, or
    /// the file doesn't parse. Deliberately NOT `prefs::load_from`: that
    /// renames an unparsable file to `.bad`, which is the right recovery for
    /// the GUI's OWN writes (a hand-edit gone wrong should be recoverable)
    /// but wrong for this read-only consumer — the GUI's `save_to` is a
    /// plain `fs::write`, so a read racing it could see a torn file and
    /// quarantine config the GUI never actually corrupted. A view filter must
    /// not have that side effect; falling back to no overrides is enough.
    fn overrides(&self) -> Overrides {
        self.prefs_path
            .as_deref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str::<prefs::Preferences>(&s).ok())
            .map(|p| Overrides::from_prefs(&p))
            .unwrap_or_default()
    }
}

/// `layout_get`/`layout_render`'s view filter, from tool arguments.
fn window_filter(args: &Args) -> Result<WindowFilter, Value> {
    Ok(WindowFilter {
        include_closed: opt(args, "include_closed")?.unwrap_or(false),
        hide_clutter: opt(args, "hide_clutter")?.unwrap_or(true),
        env: opt::<Env>(args, "environment")?.unwrap_or_default(),
        matches: opt(args, "match")?,
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
            description: "Explains this server's model of EVE settings. Topics: workflow (files, sequence, rules), overview (windows, tabs, columns, indices), presets (groups, filtered and always-shown states, built-ins), states (ids and labels, background and flag lists), probes (formations, metres, axes, YAML), layout (windows, geometry, stacks, flags), keybinds (combos, key names, stealing), copy (aspects, collateral characters, settings presets). Call it before your first edit of a kind you have not done in this conversation.",
            schema: || obj(json!({ "topic": { "type": "string", "enum": TOPICS } }), &["topic"]),
        },
        ToolDef {
            name: "open",
            description: "Select the files to edit. The account file (core_user_<id>.dat) is required: overview presets, appearance and probe formations live there. The character file holds the window layout, Neocom, HUD and the fleet watch list; open it too unless you only need account-side editors. Give char_id (from list_characters) to resolve both from the roster, or paths directly. Each account keeps its own workspace: opening another account keeps this one, edits and all; opening another character of the same account needs this one saved or undone first. Only edit a character that is logged out: the EVE client overwrites its settings on logout.",
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
            name: "settings_presets_list",
            description: "A settings preset is a saved bundle of one character's settings kept by this app — not an overview preset (see overview_presets_edit). Lists them: [{name, aspects, full, modified_unix, error}]. full means whole files were saved (aspects: everything).",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "settings_preset_edit",
            description: "A settings preset is a saved bundle of one character's settings kept by this app — not an overview preset. One op per call. create {name, aspects, overwrite?} saves the OPEN files' chosen aspects (open the character and its account first); rename {name, new_name}; delete {name}; export {name, path} writes a shareable file; import {path} adds one (returns imported_as). WRITES TO DISK IMMEDIATELY — the app's own preset folders, never a settings file. Returns {presets: [...]}. To put a preset onto characters, use copy_preview/copy_apply with source_settings_preset.",
            schema: || obj(json!({
                "op": { "type": "string", "enum": ["create", "rename", "delete", "export", "import"] },
                "name": { "type": "string" }, "new_name": { "type": "string" }, "path": { "type": "string" },
                "aspects": { "type": "array", "items": { "type": "string", "enum": ["layout", "overview", "autofill", "keybinds", "probe_formations", "fleet", "everything"] } },
                "overwrite": { "type": "boolean" }
            }), &["op"]),
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
            name: "layout_get",
            description: "The character's window layout: reference_w/h (the screen size the file was saved at), windows [{id, label, name, open, renderable, resolution_matches, geom: {x, y, w, h, screen_w, screen_h}, flags, stack}] in pixels, and stacks [{container_id, container_label, anchor_id, members}] — tabbed groups drawn at their anchor window. flags lists the flag names that are ON for that window; the top-level settable_flags lists every flag name this file can set at all. Closed windows are omitted unless include_closed. Clutter — per-chat, per-item and dialog windows, most of a real file — is omitted unless hide_clutter is false; environment and match narrow further; hidden counts what was left out. A window you were asked about that is not listed is probably filtered, not missing: relax the filter before concluding. Needs the character file open. To SEE it, call layout_render.",
            schema: || obj(json!({
                "include_closed": { "type": "boolean", "description": "Include closed windows. Default false." },
                "hide_clutter": { "type": "boolean", "description": "Default true: omit windows EVE spawns per chat, item or dialog, and dead stack frames." },
                "environment": { "type": "string", "enum": ["all", "docked", "space"], "description": "Default all. docked hides space-only windows (overview, d-scan, drones); space hides docked-only ones (station services, hangars)." },
                "match": { "type": "string", "description": "Only windows whose label, detail or id contains this text." }
            }), &[]),
        },
        ToolDef {
            name: "layout_render",
            description: "A picture of the character's window layout: every open window as a labelled box on the screen at the file's reference aspect ratio, stacks drawn once at their anchor with a tab strip, plus a legend {width, height, reference_w, reference_h, scale, windows: [{id, label, x, y, w, h, drawn, stack}]} — drawn windows only, unless include_closed. Clutter — per-chat, per-item and dialog windows, most of a real file — is omitted unless hide_clutter is false; environment and match narrow further; hidden counts what was left out. A window you were asked about that is not listed is probably filtered, not missing: relax the filter before concluding. Screen furniture — Neocom bar, ship HUD, fighter panel, badge, target list — is drawn in grey beneath the windows, outside the filter, and listed as furniture [{kind, label, x, y, w, h}]; hud_set moves it. Call it before moving anything. Needs the character file open.",
            schema: || obj(json!({
                "width": { "type": "integer", "minimum": 320, "maximum": 2048, "description": "Image width in pixels, default 1024." },
                "include_closed": { "type": "boolean", "description": "Also outline closed windows. Default false." },
                "hide_clutter": { "type": "boolean", "description": "Default true: omit windows EVE spawns per chat, item or dialog, and dead stack frames." },
                "environment": { "type": "string", "enum": ["all", "docked", "space"], "description": "Default all. docked hides space-only windows (overview, d-scan, drones); space hides docked-only ones (station services, hangars)." },
                "match": { "type": "string", "description": "Only windows whose label, detail or id contains this text." }
            }), &[]),
        },
        ToolDef {
            name: "layout_edit",
            description: "Edit the layout as a batch (one undo step; first failure rolls back). Ops: set_geometry {window, x?, y?, w?, h?} (pixels at reference_w/h; unmentioned axes keep their value; a window saved at another resolution is re-stamped to the reference; on a stacked window this moves the whole stack, as EVE keeps them together); set_flag {window, flag, on} (flag is one of layout_get's flag names — openWindows, pinnedWindows, lockedWindows, compactWindows, … — settable only when layout_get's settable_flags lists it); stack_create {a, b} (the stack lands at a's geometry); stack_add {window, container}; stack_unstack {window}; stack_reorder {container, members: [every member id in tab order]}; stack_delete_orphans {}. Returns the open, non-clutter windows as layout_get does by default. Nothing reaches disk until save. Unsure: eve_guide layout.",
            schema: || obj(op_item(&["set_geometry", "set_flag", "stack_create", "stack_add", "stack_unstack", "stack_reorder", "stack_delete_orphans"], json!({
                "window": { "type": "string" },
                "x": { "type": "integer" }, "y": { "type": "integer" }, "w": { "type": "integer" }, "h": { "type": "integer" },
                "flag": { "type": "string" }, "on": { "type": "boolean" },
                "a": { "type": "string" }, "b": { "type": "string" }, "container": { "type": "string" },
                "members": { "type": "array", "items": { "type": "string" } }
            })), &["ops"]),
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
            description: "Returns {entries: [...], available}: each entry is {command, label, group, keys, combo, malformed} where combo reads like \"Ctrl+Q\" and keys are the stored codes; available is false when the account never opened the in-game keybinding screen. Needs the account file open. Key names for keybind_set are the ones you see in combo.",
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
        ToolDef {
            name: "neocom_edit",
            description: "Edit the Neocom bar as a batch (one undo step; first failure rolls back). Ops: reorder {order: [every current index, in the wanted sequence]}; remove {index}; add {id} (an id from neocom_get's available, appended at the end); reset {} (back to EVE's original bar). Returns neocom_get's shape. Nothing reaches disk until save.",
            schema: || obj(op_item(&["reorder", "remove", "add", "reset"], json!({
                "order": { "type": "array", "items": { "type": "integer" } },
                "index": { "type": "integer" }, "id": { "type": "string" }
            })), &["ops"]),
        },
        ToolDef {
            name: "neocom_get",
            description: "The character's Neocom bar (the vertical button strip): buttons in order [{index, id, btn_type, icon_path, children}] and available — every button that can be added, by id. Needs the character file open.",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "fleet_get",
            description: "The fleet settings across both files: fields (broadcast toggles, formation, fleet finder — {name, kind, value, default, scope}), colours per broadcast type ({broadcast, state: absent|cleared|set|unreadable, rgb, default}), the watch list ({char_id, name, rgb}) and EVE's nine-colour palette. Needs at least one file open; char_open/user_open say which sides are present.",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "fleet_edit",
            description: "Edit fleet settings as a batch (one undo step; first failure rolls back). Ops: set_field {name, value} (value as text, matching the field's kind); set_colour {broadcast, rgb?} (rgb [r,g,b] 0–1 from the palette; omit rgb to clear to EVE's ✕); set_watchlist_colour {char_id, rgb?} (omit rgb to remove the colour; find ids with lookup_character). Returns fleet_get's shape. Nothing reaches disk until save.",
            schema: || obj(op_item(&["set_field", "set_colour", "set_watchlist_colour"], json!({
                "name": { "type": "string" }, "value": { "type": "string" },
                "broadcast": { "type": "string" },
                "rgb": { "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3 },
                "char_id": { "type": "integer" }
            })), &["ops"]),
        },
        ToolDef {
            name: "lookup_character",
            description: "Find a character's id by name, or confirm an id, through EVE's ESI (cached afterwards). Returns {id, name}; not_found when ESI knows no such character. Use it for watch-list colours.",
            schema: || obj(json!({ "query": { "type": "string" } }), &["query"]),
        },
        ToolDef {
            name: "copy_preview",
            description: "Plan a copy of one character's settings onto others without changing anything: which files would be written (char_writes, account_writes — an account file is shared by every character on it, so collateral_char_ids names the siblings that change too), which targets are excluded and why, and source_error if the source cannot be used. Source: source_char_id, source_char_file, or source_settings_preset (a saved bundle from settings_presets_list). Targets: target_char_ids and/or target_char_files. aspects: layout, overview, autofill, keybinds, probe_formations, fleet, or everything (whole files). allow_other_folders lets targets in another profile folder in — without it they land in excluded. ALWAYS call this before copy_apply and show the user the plan. With several profile folders, ids resolve in the first one list_characters shows; use source_char_file/target_char_files for another.",
            schema: || obj(json!({
                "source_char_id": { "type": "integer" }, "source_char_file": { "type": "string" }, "source_settings_preset": { "type": "string" },
                "target_char_ids": { "type": "array", "items": { "type": "integer" } },
                "target_char_files": { "type": "array", "items": { "type": "string" } },
                "aspects": { "type": "array", "items": { "type": "string", "enum": ["layout", "overview", "autofill", "keybinds", "probe_formations", "fleet", "everything"] }, "minItems": 1 },
                "allow_other_folders": { "type": "boolean" }
            }), &["aspects"]),
        },
        ToolDef {
            name: "copy_apply",
            description: "Perform the copy copy_preview planned, with the same arguments. WRITES TO DISK IMMEDIATELY: every target file is backed up, then replaced atomically; returns [{path, ok, backup_path, error}] per file — a target copy_preview would have excluded reports ok: false instead. Only copy onto characters that are logged out. If a target file is open here, open it again afterwards — the in-memory copy is stale. With several profile folders, ids resolve in the first one list_characters shows; use source_char_file/target_char_files for another.",
            schema: || obj(json!({
                "source_char_id": { "type": "integer" }, "source_char_file": { "type": "string" }, "source_settings_preset": { "type": "string" },
                "target_char_ids": { "type": "array", "items": { "type": "integer" } },
                "target_char_files": { "type": "array", "items": { "type": "string" } },
                "aspects": { "type": "array", "items": { "type": "string", "enum": ["layout", "overview", "autofill", "keybinds", "probe_formations", "fleet", "everything"] }, "minItems": 1 },
                "allow_other_folders": { "type": "boolean" }
            }), &["aspects"]),
        },
        ToolDef {
            name: "copy_files",
            description: "Copy one settings file's bytes as-is onto other files of the same kind (character onto characters, account onto accounts) — no pairing, no aspects. WRITES TO DISK IMMEDIATELY with a backup per target; returns [{path, ok, backup_path, error}]. Only onto logged-out characters; reopen a target if it was open here.",
            schema: || obj(json!({ "source": { "type": "string" }, "targets": { "type": "array", "items": { "type": "string" }, "minItems": 1 } }), &["source", "targets"]),
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
        EveMcp::new(std::env::temp_dir().join("eve-mcp-tests"), vec![], None)
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
            "settings_presets_list" => Ok(settings_presets_view(&self.dir)),
            "settings_preset_edit" => self.settings_preset_edit(args),
            "undo" => self.undo(),
            "list_backups" => ok(ops::list_file_backups(&self.state(), req(args, "slot")?).map_err(fail)?),
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
                let r = ops::pack_import(&self.state(), &req::<String>(args, "path")?).map_err(fail)?;
                Ok(json!({ "columns": self.overview_with_names(r.columns)?, "report": r.report }))
            }
            "overview_pack_export" => ok(ops::pack_export(&self.state(), &req::<String>(args, "path")?).map_err(fail)?),
            "probes_get" => ok(ops::probe_formations(&self.state()).map_err(fail)?),
            "probes_set" => ok(ops::set_probe_formation(&self.state(), opt(args, "id")?, &req::<String>(args, "name")?, req(args, "probes")?, req(args, "ranges")?).map_err(fail)?),
            "probes_remove" => ok(ops::remove_probe_formation(&self.state(), req(args, "id")?).map_err(fail)?),
            "probes_reorder" => ok(ops::reorder_probe_formations(&self.state(), req(args, "order")?).map_err(fail)?),
            "probes_add_yaml" => {
                let specs = ops::probe_parse_yaml(&req::<String>(args, "yaml")?).map_err(fail)?;
                ok(ops::add_probe_formations(&self.state(), specs).map_err(fail)?)
            }
            "probes_export_yaml" => {
                let f = ops::probe_formations(&self.state()).map_err(fail)?;
                let specs: Vec<settings_model::FormationSpec> = f
                    .formations
                    .into_iter()
                    .map(|f| settings_model::FormationSpec { name: f.name, probes: f.probes, ranges: f.ranges })
                    .collect();
                Ok(json!({ "yaml": ops::probe_yaml(&specs) }))
            }
            "chat_get" => ok(ops::chat_panels(&self.state()).map_err(fail)?),
            "chat_set_splits" => {
                let ids: Vec<String> = req(args, "window_ids")?;
                let userlist: Option<i64> = opt(args, "userlist_width")?;
                let input: Option<i64> = opt(args, "input_height")?;
                if userlist.is_none() && input.is_none() {
                    return Err(err("missing_field", "give userlist_width and/or input_height"));
                }
                ok(ops::set_chat_splits(&self.state(), ids, userlist, input).map_err(fail)?)
            }
            "hud_get" => {
                let h = ops::hud_layout(&self.state()).map_err(fail)?;
                Ok(json!({ "entries": h.entries.iter().map(hud_entry_view).collect::<Vec<_>>() }))
            }
            "hud_set" => {
                let h = ops::set_hud_field(&self.state(), &req::<String>(args, "name")?, &req::<String>(args, "value")?).map_err(fail)?;
                Ok(json!({ "entries": h.entries.iter().map(hud_entry_view).collect::<Vec<_>>() }))
            }
            "layout_get" => {
                let wl = ops::window_layout(&self.state(), Slot::Char).map_err(fail)?;
                Ok(layout_view(&wl, &window_filter(args)?, &self.overrides()))
            }
            "layout_render" => {
                use base64::Engine as _;
                let wl = ops::window_layout(&self.state(), Slot::Char).map_err(fail)?;
                let width: u32 = opt::<u32>(args, "width")?.unwrap_or(1024);
                let filter = window_filter(args)?;
                let overrides = self.overrides();
                // The character file is open (window_layout just needed it), so
                // this cannot fail on that; the account file is optional and
                // only costs the account-scoped furniture when absent.
                let hud = ops::hud_layout(&self.state()).map_err(fail)?;
                let (png, legend) = crate::mcp_render::layout_png(&wl, Some(&hud), width, &filter, &overrides);
                let mut v = serde_json::to_value(legend).map_err(|e| err("serialize", e.to_string()))?;
                v[PNG_KEY] = json!(base64::engine::general_purpose::STANDARD.encode(png));
                Ok(v)
            }
            "layout_edit" => self.batch(args, layout_op, |s| {
                let wl = ops::window_layout(&s.state(), Slot::Char).map_err(fail)?;
                Ok(layout_view(&wl, &window_filter(&Args::new())?, &s.overrides()))
            }),
            "autofill_get" => ok(ops::autofill_lists(&self.state()).map_err(fail)?),
            "autofill_set" => ok(ops::set_autofill_list(&self.state(), &req::<String>(args, "widget")?, req(args, "entries")?).map_err(fail)?),
            "autofill_clear_all" => ok(ops::clear_all_autofill(&self.state()).map_err(fail)?),
            "keybinds_get" => Ok(keybinds_view(&ops::keybinds(&self.state()).map_err(fail)?)),
            "keybind_set" => self.keybind_set(args),
            "neocom_edit" => self.batch(args, neocom_op, |s| Ok(neocom_view(&ops::neocom_bar(&s.state()).map_err(fail)?))),
            "neocom_get" => Ok(neocom_view(&ops::neocom_bar(&self.state()).map_err(fail)?)),
            "fleet_get" => self.fleet_get(),
            "fleet_edit" => self.batch(args, fleet_op, |s| s.fleet_get()),
            "lookup_character" => {
                let q: String = req(args, "query")?;
                lookup_result(off_runtime(|| names::lookup_blocking(&self.dir, &q)))
            }
            "copy_preview" => {
                let c = self.copy_args(args)?;
                let mut plan = setup::setup_preview(&self.roots, &self.dir, &c.source, &c.targets, &c.aspects, c.allow_other_folders);
                // A target the plan never saw for any other reason (unpaired,
                // missing file, …) already has its own excluded entry — never
                // double-list it.
                if plan.source_error.is_none() {
                    let already: HashSet<u64> = plan.excluded.iter().map(|e| e.char_id).collect();
                    for (char_id, _) in &c.other_folder {
                        if !already.contains(char_id) {
                            plan.excluded.push(setup::ExcludedTarget { char_id: *char_id, reason: OTHER_FOLDER_REASON.into() });
                        }
                    }
                }
                ok(plan)
            }
            "copy_apply" => {
                let c = self.copy_args(args)?;
                let mut results = setup::setup_apply(&self.roots, &self.dir, &c.source, &c.targets, &c.aspects, c.allow_other_folders).map_err(fail)?;
                for (_, path) in &c.other_folder {
                    if !results.iter().any(|r| Path::new(&r.path) == Path::new(path)) {
                        results.push(setup::TargetResult { path: path.clone(), ok: false, backup_path: None, error: Some(OTHER_FOLDER_REASON.into()) });
                    }
                }
                ok(results)
            }
            "copy_files" => ok(setup::copy_files(&self.roots, &req::<String>(args, "source")?, &req::<Vec<String>>(args, "targets")?).map_err(fail)?),
            _ => Err(err("unknown_tool", format!("no tool named `{name}`"))),
        }
    }

    fn doc_path(&self, slot: Slot) -> Option<String> {
        let st = self.state();
        let guard = match slot {
            Slot::Char => st.char.lock(),
            Slot::User => st.user.lock(),
        }
        .unwrap();
        guard.as_ref().map(|d| d.path.to_string_lossy().into_owned())
    }

    fn status(&self) -> ToolResult {
        let slot = |s: Slot| {
            self.doc_path(s).map(|path| json!({ "path": path, "dirty": self.state().history.lock().unwrap().dirty(s) }))
        };
        Ok(json!({
            "char": slot(Slot::Char),
            "user": slot(Slot::User),
            "can_undo": undo::undo_state(&self.state()).can_undo,
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

use crate::presets;
use crate::setup::{self, Aspect, BatchSource};

/// The app's saved bundles, without their folder paths.
fn settings_presets_view(dir: &Path) -> Value {
    let list: Vec<Value> = presets::list(dir).into_iter().map(|p| json!({
        "name": p.name, "aspects": p.aspects, "full": p.full, "modified_unix": p.modified_unix, "error": p.error,
    })).collect();
    Value::Array(list)
}

struct CopyArgs {
    source: BatchSource,
    targets: Vec<String>,
    aspects: Vec<Aspect>,
    allow_other_folders: bool,
    /// Requested targets whose OWN profile folder differs from the anchor's —
    /// the exact test `setup::scoped_files` applies (spec §2.8) — computed
    /// directly from `discover`, never inferred from what a plan happened to
    /// produce (an account-only aspect's plan carries no `char_writes` at
    /// all, so "absent from char_writes" is not a safe signal for "wrong
    /// folder"). Always empty when `allow_other_folders`. `(char_id, path)`,
    /// deduplicated by id; the source's own id never appears here.
    other_folder: Vec<(u64, String)>,
}

/// A target path's discovered profile directory and char id, reached the same
/// way `locate`/`setup::locate_source` reach across every profile (not
/// folder-scoped) — `None` when the path names no discovered character file.
/// Compares as `Path`, not text, so `/` vs `\` in a caller-supplied
/// `target_char_files` path never causes a false "different folder".
fn char_file_info(profiles: &[Profile], path: &Path) -> Option<(PathBuf, u64)> {
    profiles.iter().find_map(|p| {
        let f = p.files.iter().find(|f| f.kind == FileKind::Char && f.path == path)?;
        Some((p.dir.clone(), f.id?))
    })
}

/// Every requested target whose own profile folder is not the anchor's — the
/// same test `setup::scoped_files` uses to decide what a plan even sees,
/// computed independently of any plan output. The anchor is the source
/// character's own profile folder for a `Character` source (mirroring
/// `setup::locate_source`), or the first target's parent directory for a
/// `Preset` source (mirroring `setup`'s own anchor derivation, carried in
/// `BatchSource::Preset::anchor_dir`). A target that fails to resolve at all,
/// or an unresolved source, contributes nothing here — those are a
/// `source_error`/existing-`excluded`-reason concern, not this one's.
fn other_folder_targets(profiles: &[Profile], source: &BatchSource, targets: &[String]) -> Vec<(u64, String)> {
    let (anchor_dir, source_char_id) = match source {
        BatchSource::Character { path } => match char_file_info(profiles, Path::new(path)) {
            Some((dir, id)) => (dir, Some(id)),
            None => return Vec::new(),
        },
        BatchSource::Preset { anchor_dir, .. } => {
            if anchor_dir.is_empty() {
                return Vec::new();
            }
            (PathBuf::from(anchor_dir), None)
        }
    };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for t in targets {
        let Some((dir, id)) = char_file_info(profiles, Path::new(t)) else { continue };
        if Some(id) == source_char_id || !seen.insert(id) {
            continue;
        }
        if dir != anchor_dir {
            out.push((id, t.clone()));
        }
    }
    out
}

impl EveMcp {
    /// One source (by char id, char file, or settings-preset name), one or
    /// more targets (by char id or char file), the aspects, the folder flag.
    fn copy_args(&self, args: &Args) -> Result<CopyArgs, Value> {
        let profiles = discover(&self.roots);
        let roster = accounts::load_roster(&self.roots, &self.dir);
        let char_path = |id: u64| -> Result<String, Value> {
            locate(id, &profiles, &roster)
                .map(|(c, _)| c.to_string_lossy().into_owned())
                .ok_or_else(|| err("unknown_character", format!("no core_char_{id}.dat in any profile; call list_characters")))
        };

        let src_id: Option<u64> = opt(args, "source_char_id")?;
        let src_file: Option<String> = opt(args, "source_char_file")?;
        let src_preset: Option<String> = opt(args, "source_settings_preset")?;
        let given = src_id.is_some() as u8 + src_file.is_some() as u8 + src_preset.is_some() as u8;
        if given == 0 { return Err(err("missing_field", "one of source_char_id, source_char_file, source_settings_preset")); }
        if given > 1 { return Err(err("bad_arguments", "give exactly one source")); }

        let mut targets: Vec<String> = opt::<Vec<String>>(args, "target_char_files")?.unwrap_or_default();
        for id in opt::<Vec<u64>>(args, "target_char_ids")?.unwrap_or_default() {
            targets.push(char_path(id)?);
        }
        if targets.is_empty() { return Err(err("missing_field", "target_char_ids and/or target_char_files, at least one target")); }

        let source = if let Some(id) = src_id {
            BatchSource::Character { path: char_path(id)? }
        } else if let Some(p) = src_file {
            BatchSource::Character { path: p }
        } else {
            let name = src_preset.expect("checked");
            let dir = presets::preset_path(&self.dir, &name).map_err(|e| err("unknown_settings_preset", e.0))?;
            if !dir.is_dir() { return Err(err("unknown_settings_preset", format!("no settings preset `{name}`; call settings_presets_list"))); }
            let anchor_dir = Path::new(&targets[0]).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
            BatchSource::Preset { dir: dir.to_string_lossy().into_owned(), anchor_dir }
        };

        let allow_other_folders = opt(args, "allow_other_folders")?.unwrap_or(false);
        let other_folder = if allow_other_folders { Vec::new() } else { other_folder_targets(&profiles, &source, &targets) };

        Ok(CopyArgs {
            source, targets, other_folder, allow_other_folders,
            aspects: req(args, "aspects")?,
        })
    }
}

/// A target the plan never saw at all is reported this way — `setup::target_ids`
/// keeps only paths in the anchor profile folder, so a target resolved
/// somewhere else would otherwise be silently absent from both `char_writes`
/// and `excluded` (or, for an account-only aspect, absent from `excluded`
/// despite never being written) rather than being reported.
const OTHER_FOLDER_REASON: &str = "in another profile folder; set allow_other_folders to include it";

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

/// The workspace key for a settings file. Canonical, so the spelling the
/// model chooses and the one `locate` produces name one workspace. A path
/// that does not exist is the same `io` error `open_file` would give.
fn canonical(path: &str) -> Result<PathBuf, Value> {
    std::fs::canonicalize(path).map_err(|e| err("io", format!("{path}: {e}")))
}

/// `{path, fidelity}` of an open slot — what `open` reports per slot — or
/// `None` for an empty one.
fn slot_view(state: &AppState, slot: Slot) -> Option<Value> {
    let guard = match slot {
        Slot::Char => state.char.lock(),
        Slot::User => state.user.lock(),
    }
    .unwrap();
    guard.as_ref().map(|d| json!({ "path": d.path.to_string_lossy(), "fidelity": d.fidelity }))
}

/// Spec §3.3, rule 3: re-read a clean slot whose file changed on disk (the
/// window saved it, the game client rewrote it, a batch copy landed). A
/// dirty slot is never re-read — its edits are the point, and `save`'s
/// conflict check will name the situation.
///
/// ponytail: `open_file` clears the undo stack when it reloads, so a dirty
/// sibling slot keeps its edits but loses undo. Rare (one slot dirty, the
/// other clean and stale, on a re-open); the edits are what matters.
fn reload_stale(state: &AppState) {
    for slot in [Slot::User, Slot::Char] {
        let path = {
            let guard = match slot {
                Slot::Char => state.char.lock(),
                Slot::User => state.user.lock(),
            }
            .unwrap();
            match guard.as_ref() {
                Some(d) if d.changed_on_disk() => d.path.to_string_lossy().into_owned(),
                _ => continue,
            }
        };
        if state.history.lock().unwrap().dirty(slot) {
            continue;
        }
        // A failing re-read leaves the slot as it was (`open_file`'s own
        // guarantee); the next `save` then reports the conflict.
        let _ = ops::open_file(state, slot, &path);
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
        let user_key = canonical(&user_file)?;
        let char_key = char_file.as_deref().map(canonical).transpose()?;

        let existing = self.workspaces.lock().unwrap().get(&user_key).cloned();
        let ws = match existing {
            Some(ws) => {
                let open_char = ws.char.lock().unwrap().as_ref().map(|d| d.path.clone());
                // Same character, or none asked for: nothing to load. `open`
                // never discards a character to satisfy an account-only call.
                let swap = match (&open_char, &char_key) {
                    (Some(have), Some(want)) => std::fs::canonicalize(have).ok().as_ref() != Some(want),
                    (None, Some(_)) => true,
                    (_, None) => false,
                };
                if swap {
                    let h = ws.history.lock().unwrap();
                    if h.dirty(Slot::Char) || h.dirty(Slot::User) {
                        let who = open_char
                            .as_ref()
                            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                            .unwrap_or_else(|| "the open character".into());
                        return Err(err(
                            "unsaved_edits",
                            format!("{who} or its account has unsaved edits; save or undo them before opening another character of this account"),
                        ));
                    }
                    drop(h);
                    // Both slots are clean, so closing first loses nothing, and
                    // a failing open leaves the slot empty rather than pointing
                    // at the previous sibling.
                    ops::close_file(&ws, Slot::Char);
                    if let Some(p) = &char_file {
                        open_slot(&ws, Slot::Char, p)?;
                    }
                }
                reload_stale(&ws);
                ws
            }
            None => {
                // A fresh workspace is built aside and inserted only once both
                // opens succeeded, so a bad path changes nothing — not the map,
                // not `current`.
                let ws = AppState::new();
                open_slot(&ws, Slot::User, &user_file)?;
                if let Some(p) = &char_file {
                    open_slot(&ws, Slot::Char, p)?;
                }
                self.workspaces.lock().unwrap().insert(user_key, ws.clone());
                ws
            }
        };
        *self.current.lock().unwrap() = Some(ws.clone());
        Ok(json!({ "char": slot_view(&ws, Slot::Char), "user": slot_view(&ws, Slot::User) }))
    }

    fn save(&self, args: &Args) -> ToolResult {
        let force: bool = opt(args, "force")?.unwrap_or(false);
        let mut saved = Vec::new();
        let mut skipped = Vec::new();
        for (slot, name) in [(Slot::User, "user"), (Slot::Char, "char")] {
            let Some(path) = self.doc_path(slot) else { continue };
            if !self.state().history.lock().unwrap().dirty(slot) {
                skipped.push(name);
                continue;
            }
            match ops::save_document(&self.state(), slot, force) {
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
        match undo::undo(&self.state()) {
            Some(_) => self.status(),
            None => Err(err("nothing_to_undo", "the undo stack is empty")),
        }
    }

    fn restore_backup(&self, args: &Args) -> ToolResult {
        let slot: Slot = req(args, "slot")?;
        let backup: String = req(args, "backup_path")?;
        match ops::restore_backup(&self.state(), slot, &backup).map_err(fail)? {
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
        let oc = ops::overview_columns(&self.state()).map_err(fail)?;
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
    fn settings_preset_edit(&self, args: &Args) -> ToolResult {
        let op: String = req(args, "op")?;
        let mut extra = Map::new();
        match op.as_str() {
            "create" => {
                let name: String = req(args, "name")?;
                let aspects: Vec<Aspect> = req(args, "aspects")?;
                let overwrite = opt::<bool>(args, "overwrite")?.unwrap_or(false);
                setup::preset_save(&self.state(), &self.dir, &name, &aspects, overwrite).map_err(fail)?;
            }
            "rename" => presets::rename(&self.dir, &req::<String>(args, "name")?, &req::<String>(args, "new_name")?).map_err(|e| err("preset", e))?,
            "delete" => presets::delete(&self.dir, &req::<String>(args, "name")?).map_err(|e| err("preset", e))?,
            "export" => presets::export_to(&self.dir, &req::<String>(args, "name")?, Path::new(&req::<String>(args, "path")?)).map_err(|e| err("preset", e))?,
            "import" => {
                let name = presets::import_from(&self.dir, Path::new(&req::<String>(args, "path")?)).map_err(|e| err("preset", e))?;
                extra.insert("imported_as".into(), json!(name));
            }
            _ => return Err(unknown_op(&op)),
        }
        let mut v = json!({ "presets": settings_presets_view(&self.dir) });
        v.as_object_mut().unwrap().extend(extra);
        Ok(v)
    }

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
        let st = self.state();
        {
            let _group = undo::group(&st);
            for (i, op) in ops_list.iter().enumerate() {
                if let Err(mut e) = apply(&st, op) {
                    // A write failure already rolled the group back inside
                    // `edit_reshared`; a parse failure did not. `rollback_group`
                    // is a no-op when nothing was written, so call it always.
                    let mut u = st.user.lock().unwrap();
                    let mut c = st.char.lock().unwrap();
                    st.history.lock().unwrap().rollback_group(&mut u, &mut c);
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
        let r = ops::set_keybind_cmd(&self.state(), &command, keys).map_err(fail)?;
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
        let server = EveMcp::new(dir, settings_model::default_roots(), prefs::path_base());
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

    /// A failing open must not touch the workspace it would have changed:
    /// the character that was open stays open, paired with the same account.
    #[test]
    fn a_failed_char_reopen_leaves_the_workspace_untouched() {
        let (s, path) = open_user(&overview_user_bytes());
        let cpath = temp_file("mcp-char", &encode(&BmValue::Dict(vec![])).unwrap());
        s.call("open", &args(json!({ "user_file": path.to_string_lossy(), "char_file": cpath.to_string_lossy() }))).unwrap();
        assert!(s.call("status", &Args::new()).unwrap()["char"].is_object());

        let missing = cpath.with_file_name("does-not-exist.dat");
        let e = s
            .call("open", &args(json!({ "user_file": path.to_string_lossy(), "char_file": missing.to_string_lossy() })))
            .unwrap_err();
        assert_eq!(e["code"], "io");
        assert_eq!(s.call("status", &Args::new()).unwrap()["char"]["path"], json!(cpath.to_string_lossy()));
    }

    /// `open_file` leaves a slot untouched on an io error — a failing NEW
    /// account open must not discard the session that was already open,
    /// unsaved edits included.
    #[test]
    fn a_failed_account_open_leaves_the_previous_session_intact() {
        let (s, path) = open_user(&overview_user_bytes());
        let cpath = temp_file("mcp-char", &encode(&BmValue::Dict(vec![])).unwrap());
        s.call("open", &args(json!({ "user_file": path.to_string_lossy(), "char_file": cpath.to_string_lossy() }))).unwrap();
        ops::set_overview_visible(&s.state(), 0, "TYPE", true).unwrap();

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

    fn open_args(user: &Path, char: Option<&Path>) -> Args {
        let mut a = Args::new();
        a.insert("user_file".into(), json!(user.to_string_lossy()));
        if let Some(c) = char {
            a.insert("char_file".into(), json!(c.to_string_lossy()));
        }
        a
    }

    fn empty_char_bytes() -> Vec<u8> {
        encode(&BmValue::Dict(vec![])).unwrap()
    }

    /// Two accounts are two workspaces: opening the second keeps the first,
    /// unsaved edits and all, and opening the first again is a switch back,
    /// not a reload.
    #[test]
    fn open_of_a_second_account_keeps_the_first() {
        let (s, a) = open_user(&overview_user_bytes());
        ops::set_overview_visible(&s.state(), 0, "TYPE", true).unwrap();
        let first = s.state();

        let b = temp_file("mcp-b", &overview_user_bytes());
        s.call("open", &open_args(&b, None)).unwrap();
        let st = s.call("status", &Args::new()).unwrap();
        assert_eq!(st["user"]["path"], json!(b.to_string_lossy()));
        assert_eq!(st["user"]["dirty"], false, "a fresh workspace");
        assert!(!s.state().ptr_eq(&first));

        s.call("open", &open_args(&a, None)).unwrap();
        assert!(s.state().ptr_eq(&first), "the same workspace, not a reload");
        assert_eq!(s.call("status", &Args::new()).unwrap()["user"]["dirty"], true, "the edit survived");
    }

    /// Siblings share the account: opening a second character of the same
    /// account swaps the character slot inside the one workspace.
    #[test]
    fn open_of_a_sibling_swaps_the_character_in_the_same_workspace() {
        let (s, a) = open_user(&overview_user_bytes());
        let c1 = temp_file("mcp-c1", &empty_char_bytes());
        let c2 = temp_file("mcp-c2", &empty_char_bytes());
        s.call("open", &open_args(&a, Some(&c1))).unwrap();
        let ws = s.state();

        let v = s.call("open", &open_args(&a, Some(&c2))).unwrap();
        assert_eq!(v["char"]["path"], json!(c2.to_string_lossy()));
        assert_eq!(v["user"]["path"], json!(a.to_string_lossy()));
        assert!(s.state().ptr_eq(&ws));
        assert_eq!(s.call("status", &Args::new()).unwrap()["char"]["path"], json!(c2.to_string_lossy()));
    }

    /// The forced workflow (spec §3.3): finish one character of an account
    /// before starting the next. Either dirty slot blocks; save or undo clears.
    #[test]
    fn a_sibling_swap_is_blocked_while_either_slot_is_dirty() {
        let (s, a) = open_user(&overview_user_bytes());
        let c1 = temp_file("mcp-c1", &empty_char_bytes());
        let c2 = temp_file("mcp-c2", &empty_char_bytes());
        s.call("open", &open_args(&a, Some(&c1))).unwrap();

        // Account side dirty.
        ops::set_overview_visible(&s.state(), 0, "TYPE", true).unwrap();
        let e = s.call("open", &open_args(&a, Some(&c2))).unwrap_err();
        assert_eq!(e["code"], "unsaved_edits");
        assert!(e["message"].as_str().unwrap().contains("save"));
        assert_eq!(s.call("status", &Args::new()).unwrap()["char"]["path"], json!(c1.to_string_lossy()), "untouched");
        s.call("save", &Args::new()).unwrap();
        s.call("open", &open_args(&a, Some(&c2))).unwrap();

        // Character side dirty; undo clears it too.
        ops::apply_mutation(
            &s.state(),
            Slot::Char,
            &settings_model::Mutation::InsertDictEntry {
                parent: vec![],
                key: settings_model::NewValue::Str("x".into()),
                value: settings_model::NewValue::Int("1".into()),
            },
        )
        .unwrap();
        assert_eq!(s.call("open", &open_args(&a, Some(&c1))).unwrap_err()["code"], "unsaved_edits");
        s.call("undo", &Args::new()).unwrap();
        s.call("open", &open_args(&a, Some(&c1))).unwrap();
        assert_eq!(s.call("status", &Args::new()).unwrap()["char"]["path"], json!(c1.to_string_lossy()));
    }

    /// Asking for the account alone on a workspace with a character open
    /// keeps the character: `open` never discards.
    #[test]
    fn open_with_only_the_account_keeps_the_open_character() {
        let (s, a) = open_user(&overview_user_bytes());
        let c1 = temp_file("mcp-c1", &empty_char_bytes());
        s.call("open", &open_args(&a, Some(&c1))).unwrap();
        let v = s.call("open", &open_args(&a, None)).unwrap();
        assert_eq!(v["char"]["path"], json!(c1.to_string_lossy()));
    }

    /// Spec §3.3, rule 3: `open` re-reads a clean slot whose file moved on
    /// disk (the window saved, the client wrote) and never a dirty one.
    #[test]
    fn open_rereads_a_clean_stale_slot_and_never_a_dirty_one() {
        let (s, a) = open_user(&overview_user_bytes());
        // Clean: the rewrite is picked up. The fixture's one tab has TYPE hidden;
        // an empty dict has no tabs at all.
        std::fs::write(&a, encode(&BmValue::Dict(vec![])).unwrap()).unwrap();
        s.call("open", &open_args(&a, None)).unwrap();
        let v = s.call("overview_get", &Args::new()).unwrap();
        assert!(v["tabs"].as_array().unwrap().is_empty(), "the rewrite (no tabs) was picked up");

        // Dirty: the in-memory edit wins and the later save conflicts, as today.
        let (s, a) = open_user(&overview_user_bytes());
        ops::set_overview_visible(&s.state(), 0, "TYPE", true).unwrap();
        std::fs::write(&a, encode(&BmValue::Dict(vec![])).unwrap()).unwrap();
        s.call("open", &open_args(&a, None)).unwrap();
        let v = s.call("overview_get", &Args::new()).unwrap();
        assert_eq!(visible_count(&v), 2, "the edit is still there");
        assert_eq!(s.call("save", &Args::new()).unwrap_err()["code"], "conflict");
    }

    /// The key is the canonical path, so a spelling the model chooses finds
    /// the workspace a roster lookup created.
    #[test]
    fn explicit_paths_with_different_spelling_hit_the_same_workspace() {
        let (s, a) = open_user(&overview_user_bytes());
        let ws = s.state();
        let respelled = a.to_string_lossy().replace('\\', "/");
        let mut args = Args::new();
        args.insert("user_file".into(), json!(respelled));
        s.call("open", &args).unwrap();
        assert!(s.state().ptr_eq(&ws));
    }

    #[test]
    fn save_skips_a_clean_slot_and_writes_a_dirty_one_with_a_backup() {
        let (s, path) = open_user(&overview_user_bytes());
        let v = s.call("save", &Args::new()).unwrap();
        assert_eq!(v["saved"], json!([]));
        assert_eq!(v["skipped"], json!(["user"]));

        ops::set_overview_visible(&s.state(), 0, "TYPE", true).unwrap();
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
        ops::set_overview_visible(&s.state(), 0, "TYPE", true).unwrap();
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
        ops::open_file(&s.state(), Slot::Char, cpath.to_str().unwrap()).unwrap();

        ops::set_overview_visible(&s.state(), 0, "TYPE", true).unwrap();
        ops::apply_mutation(
            &s.state(),
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
        ops::set_overview_visible(&s.state(), 0, "TYPE", true).unwrap();
        let st = s.call("undo", &Args::new()).unwrap();
        assert_eq!(st["user"]["dirty"], false);
        assert_eq!(st["can_undo"], false);
    }

    #[test]
    fn list_backups_then_restore_puts_the_saved_bytes_back() {
        let (s, path) = open_user(&overview_user_bytes());
        ops::set_overview_visible(&s.state(), 0, "TYPE", true).unwrap();
        s.call("save", &Args::new()).unwrap();
        let backups = s.call("list_backups", &args(json!({ "slot": "user" }))).unwrap();
        let newest = backups[0]["path"].as_str().unwrap().to_string();

        let v = s.call("restore_backup", &args(json!({ "slot": "user", "backup_path": newest }))).unwrap();
        assert_eq!(v["path"], json!(path.to_string_lossy()));
        let oc = ops::overview_columns(&s.state()).unwrap();
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
    fn primer_has_every_topic_in_order_and_none_is_empty() {
        let sections = primer_sections();
        let slugs: Vec<&str> = sections.iter().map(|(s, _)| *s).collect();
        assert_eq!(slugs, TOPICS);
        for (slug, body) in &sections {
            assert!(body.len() > 100, "{slug} is too short to be a primer section");
        }
    }

    #[test]
    fn the_workflow_section_names_every_editor() {
        let w = primer("workflow").unwrap();
        for prefix in ["layout_", "autofill_", "keybind", "neocom_", "hud_", "fleet_", "chat_", "copy_", "settings_preset"] {
            assert!(w.contains(prefix), "workflow does not mention {prefix}");
        }
        assert!(w.contains("layout_render"));
    }

    #[test]
    fn eve_guide_serves_the_three_new_topics() {
        let s = EveMcp::for_tests();
        for (t, word) in [("layout", "anchor"), ("keybinds", "stolen"), ("copy", "collateral")] {
            let v = s.call("eve_guide", &args(json!({ "topic": t }))).unwrap();
            assert!(v["text"].as_str().unwrap().contains(word), "{t} should mention {word}");
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
        ops::preset_fork(&s.state(), 0, "Frigs".into(), vec![25, 26], vec![11], vec![]).unwrap();
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
        let st = s.state();
        let before = undo::undo_state(&st).depth;
        {
            let _outer = undo::group(&st);
            ops::set_overview_visible(&st, 0, "TYPE", true).unwrap();
            {
                let _inner = undo::group(&st);
                ops::set_overview_order(&st, 0, vec!["TYPE".into(), "NAME".into()]).unwrap();
            }
            ops::set_overview_visible(&st, 0, "TYPE", false).unwrap();
        }
        assert_eq!(undo::undo_state(&st).depth, before + 1, "three writes under one group, one entry");
    }

    fn visible_count(v: &Value) -> usize {
        v["tabs"][0]["columns"].as_array().unwrap().iter().filter(|c| c["visible"] == true).count()
    }

    #[test]
    fn columns_edit_applies_a_batch_as_one_undo_step() {
        let (s, _) = open_user(&overview_user_bytes());
        let before = undo::undo_state(&s.state()).depth;
        let v = s.call("overview_columns_edit", &args(json!({ "ops": [
            { "op": "set_visible", "tab": 0, "column": "TYPE", "visible": true },
            { "op": "set_order", "tab": 0, "order": ["TYPE", "NAME"] }
        ]}))).unwrap();
        assert_eq!(visible_count(&v), 2);
        assert_eq!(v["tabs"][0]["columns"][0]["name"], "TYPE");
        assert_eq!(undo::undo_state(&s.state()).depth, before + 1);
        assert!(v["names"].is_object(), "edits return the same self-describing shape as overview_get");
    }

    #[test]
    fn a_failing_op_rolls_the_whole_batch_back_and_names_its_index() {
        let (s, _) = open_user(&overview_user_bytes());
        let before = s.call("overview_get", &Args::new()).unwrap();
        let depth = undo::undo_state(&s.state()).depth;
        let e = s.call("overview_columns_edit", &args(json!({ "ops": [
            { "op": "set_visible", "tab": 0, "column": "TYPE", "visible": true },
            { "op": "set_visible", "tab": 99, "column": "TYPE", "visible": true }
        ]}))).unwrap_err();
        // An unknown COLUMN is not an error (`set_column_visible` adds it); an
        // unknown TAB is `NoTab`, so that is the failing op.
        assert_eq!(e["op_index"], 1);
        assert_eq!(s.call("overview_get", &Args::new()).unwrap(), before);
        assert_eq!(undo::undo_state(&s.state()).depth, depth);
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
        ops::overview_create_window_mapping(&s.state()).unwrap();
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
        ops::open_file(&s.state(), Slot::Char, path.to_str().unwrap()).unwrap();
        (s, path)
    }

    fn assert_no_paths(v: &Value, tool: &str) {
        match v {
            Value::Object(m) => {
                for (k, c) in m {
                    // icon_path is a neocom entry's EVE resource path (res:/ui/…), not
                    // a filesystem path — it is meant to reach the model.
                    let is_path_key = (k.ends_with("_path") && k != "icon_path") || k == "set" || k == "path";
                    assert!(!is_path_key || tool.starts_with("copy") || tool == "save" || tool == "open", "{tool}: key `{k}` leaks a path");
                    assert_no_paths(c, tool);
                }
            }
            Value::Array(a) => a.iter().for_each(|c| assert_no_paths(c, tool)),
            _ => {}
        }
    }

    /// Every read-only `*_get` tool, over one document open in both slots at
    /// once (`layout_char_bytes` in char, `overview_user_bytes` in user) — a
    /// single fixture wide enough to sweep the whole `_get` surface without a
    /// document per tool. A tool this fixture has nothing for is allowed to
    /// error `no_document` (a slot genuinely absent) or `no_ui` (neither
    /// fixture carries a `ui` section, which `probes_get`/`neocom_get`
    /// require) and nothing else.
    #[test]
    fn every_get_result_carries_no_paths() {
        let s = EveMcp::for_tests();
        let cpath = temp_file("mcp-nopaths-char", &layout_char_bytes());
        ops::open_file(&s.state(), Slot::Char, cpath.to_str().unwrap()).unwrap();
        let upath = temp_file("mcp-nopaths-user", &overview_user_bytes());
        ops::open_file(&s.state(), Slot::User, upath.to_str().unwrap()).unwrap();

        for tool in [
            "layout_get", "hud_get", "fleet_get", "neocom_get", "chat_get", "autofill_get",
            "keybinds_get", "overview_get", "probes_get", "settings_presets_list",
        ] {
            match s.call(tool, &Args::new()) {
                Ok(v) => assert_no_paths(&v, tool),
                Err(e) => assert!(
                    matches!(e["code"].as_str(), Some("no_document") | Some("no_ui")),
                    "{tool} failed unexpectedly: {e}"
                ),
            }
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
        let v = s.call("probes_get", &Args::new()).unwrap();
        assert_eq!(v["formations"][0]["name"], "renamed");
        assert_no_paths(&v, "probes_get");
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
        ops::preset_fork(&s.state(), 0, "Frigs".into(), vec![25], vec![], vec![]).unwrap();
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

    /// `ops::tests::neocom_char_bytes`'s shape: two buttons on the bar, two in
    /// the Original snapshot (one conflicting with bundled, one legacy-only).
    fn neocom_char_bytes() -> Vec<u8> {
        let ts = || BmValue::Long(vec![0u8; 8]);
        let button = |id: &str, btn_type: i64, icon: &str| BmValue::Instance {
            class: Box::new(b("utillib.KeyVal")),
            state: Box::new(BmValue::Dict(vec![
                (b("btnType"), BmValue::Int(btn_type)), (b("children"), BmValue::None),
                (b("iconPath"), b(icon)), (b("id"), b(id)),
            ])),
        };
        encode(&BmValue::Dict(vec![(b("ui"), BmValue::Dict(vec![
            (b("neocomButtonRawData"), BmValue::Tuple(vec![ts(), BmValue::List(vec![button("chat", 10, "res:/ui/Texture/WindowIcons/chatchannel.png"), button("wallet", 1, "res:/ui/Texture/WindowIcons/wallet.png")])])),
            (b("neocomButtonRawDataOriginal"), BmValue::Tuple(vec![ts(), BmValue::Tuple(vec![button("chat", 10, "chat.png"), button("legacyprobe", 7, "legacy.png")])])),
        ]))])).unwrap()
    }

    #[test]
    fn neocom_get_lists_the_bar_and_the_union_catalog() {
        let (s, _) = open_char(&neocom_char_bytes());
        let v = s.call("neocom_get", &Args::new()).unwrap();
        assert_no_paths(&v, "neocom_get");
        let ids: Vec<&str> = v["buttons"].as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["chat", "wallet"]);
        let avail = v["available"].as_array().unwrap();
        assert!(avail.iter().any(|e| e["id"] == "market"), "bundled catalog present");
        assert!(avail.iter().any(|e| e["id"] == "chat" && e["icon_path"] == "res:/ui/Texture/WindowIcons/chatchannel.png"), "bundled wins conflict: chat uses bundled icon, not original's chat.png");
        assert!(avail.iter().any(|e| e["id"] == "legacyprobe" && e["btn_type"] == 7 && e["icon_path"] == "legacy.png"), "union includes original-only entries: legacyprobe");
        assert!(avail.len() >= 23, "catalog has bundled (22) plus original-only legacyprobe");
    }

    #[test]
    fn neocom_edit_adds_by_id_from_the_catalog_reorders_and_resets() {
        let (s, _) = open_char(&neocom_char_bytes());
        let v = s.call("neocom_edit", &args(json!({ "ops": [
            { "op": "add", "id": "legacyprobe" },
            { "op": "add", "id": "market" },
            { "op": "reorder", "order": [3, 0, 1, 2] }
        ]}))).unwrap();
        let ids: Vec<&str> = v["buttons"].as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["market", "chat", "wallet", "legacyprobe"]);
        let legacy = v["buttons"].as_array().unwrap().iter().find(|b| b["id"] == "legacyprobe").unwrap();
        assert_eq!(legacy["btn_type"], 7);
        assert_eq!(legacy["icon_path"], "legacy.png");
        let e = s.call("neocom_edit", &args(json!({ "ops": [{ "op": "add", "id": "no_such_button" }] }))).unwrap_err();
        assert_eq!(e["code"], "unknown_button");
        let v = s.call("neocom_edit", &args(json!({ "ops": [{ "op": "reset" }] }))).unwrap();
        assert_eq!(v["buttons"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn fleet_get_projects_both_sides_and_strips_paths() {
        let (s, _) = open_user(&empty_ui_bytes());
        let cpath = temp_file("mcp-fleet-char", &empty_ui_bytes());
        ops::open_file(&s.state(), Slot::Char, cpath.to_str().unwrap()).unwrap();
        let v = s.call("fleet_get", &Args::new()).unwrap();
        assert_no_paths(&v, "fleet_get");
        assert_eq!(v["user_open"], true);
        assert_eq!(v["char_open"], true);
        assert!(v["fields"].as_array().unwrap().iter().any(|f| f["name"] == "listen_show_own"));
        assert!(v["colours"].as_array().unwrap().iter().any(|c| c["broadcast"] == "Target"));
        assert!(v["palette"].as_array().unwrap().len() >= 8);
    }

    #[test]
    fn fleet_edit_sets_a_field_a_colour_and_a_watchlist_entry_then_clears_the_colour() {
        let (s, _) = open_user(&empty_ui_bytes());
        let cpath = temp_file("mcp-fleet-char2", &empty_ui_bytes());
        ops::open_file(&s.state(), Slot::Char, cpath.to_str().unwrap()).unwrap();
        let v = s.call("fleet_edit", &args(json!({ "ops": [
            { "op": "set_field", "name": "listen_show_own", "value": "1" },
            { "op": "set_colour", "broadcast": "Target", "rgb": [0.2, 0.5, 1.0] },
            { "op": "set_watchlist_colour", "char_id": 90000001, "rgb": [1.0, 0.0, 0.0] }
        ]}))).unwrap();
        let f = v["fields"].as_array().unwrap().iter().find(|f| f["name"] == "listen_show_own").unwrap();
        assert_eq!(f["value"], "1");
        let c = v["colours"].as_array().unwrap().iter().find(|c| c["broadcast"] == "Target").unwrap();
        assert_eq!(c["state"], "set");
        assert_eq!(c["rgb"], json!([0.2, 0.5, 1.0]));
        let w = v["watchlist"].as_array().unwrap().iter().find(|w| w["char_id"] == 90000001).unwrap();
        assert_eq!(w["rgb"], json!([1.0, 0.0, 0.0]));
        let v = s.call("fleet_edit", &args(json!({ "ops": [{ "op": "set_colour", "broadcast": "Target" }] }))).unwrap();
        let c = v["colours"].as_array().unwrap().iter().find(|c| c["broadcast"] == "Target").unwrap();
        assert_eq!(c["state"], "cleared");
        assert_eq!(undo::undo_state(&s.state()).depth, 2, "two batches, two undo steps");
    }

    #[test]
    fn lookup_result_maps_a_hit_a_miss_and_a_transport_error() {
        let dir = std::env::temp_dir().join(format!("mcp-lookup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let hit = names::lookup_with(&dir, "Some Pilot", |_| Ok(vec![]), |_| Ok(Some(names::Found { id: 42, name: "Some Pilot".into() })));
        assert_eq!(lookup_result(hit).unwrap(), json!({ "id": 42, "name": "Some Pilot" }));
        // Now cached: the fetchers must not be consulted.
        let cached = names::lookup_with(&dir, "some pilot", |_| panic!("cache miss"), |_| panic!("cache miss"));
        assert_eq!(lookup_result(cached).unwrap()["id"], 42);
        let miss = names::lookup_with(&dir, "Nobody", |_| Ok(vec![]), |_| Ok(None));
        assert_eq!(lookup_result(miss).unwrap_err()["code"], "not_found");
        let down = names::lookup_with(&dir, "Anyone", |_| Ok(vec![]), |_| Err(names::FetchError("timeout".into())));
        assert_eq!(lookup_result(down).unwrap_err()["code"], "esi");
    }

    /// A character file with three windows — `overview` open at a matching
    /// resolution, `market` open at a different one (so a move re-stamps the
    /// screen size), `fitting` closed — and a stack of m1+m2 in container C
    /// (`ops::tests::stacked_char_bytes`'s shape). Flags: `pinnedWindows` has
    /// an entry for overview only, so market's pinned flag is an Insert.
    fn layout_char_bytes() -> Vec<u8> {
        let ts = || BmValue::Long(vec![0u8; 8]);
        let geom = |x: i64, y: i64, w: i64, h: i64, sw: i64, sh: i64| {
            BmValue::Tuple(vec![BmValue::Int(x), BmValue::Int(y), BmValue::Int(w), BmValue::Int(h), BmValue::Int(sw), BmValue::Int(sh)])
        };
        encode(&BmValue::Dict(vec![(b("windows"), BmValue::Dict(vec![
            (b("windowSizesAndPositions_1"), BmValue::Tuple(vec![ts(), BmValue::Dict(vec![
                (b("overview"), geom(100, 200, 400, 600, 2560, 1440)),
                (b("market"), geom(10, 10, 300, 300, 1920, 1080)),
                (b("fitting"), geom(0, 0, 500, 500, 2560, 1440)),
                (b("m1"), geom(0, 0, 100, 80, 2560, 1440)), (b("m2"), geom(0, 0, 100, 80, 2560, 1440)), (b("C"), geom(0, 0, 100, 80, 2560, 1440)),
            ])])),
            (b("openWindows"), BmValue::Tuple(vec![ts(), BmValue::Dict(vec![
                (b("overview"), BmValue::Bool(true)), (b("market"), BmValue::Bool(true)), (b("fitting"), BmValue::Bool(false)),
                (b("m1"), BmValue::Bool(true)), (b("m2"), BmValue::Bool(true)), (b("C"), BmValue::Bool(true)),
            ])])),
            (b("pinnedWindows"), BmValue::Tuple(vec![ts(), BmValue::Dict(vec![(b("overview"), BmValue::Bool(true))])])),
            (b("stacksWindows"), BmValue::Tuple(vec![ts(), BmValue::Dict(vec![(b("m1"), b("C")), (b("m2"), b("C"))])])),
            (b("preferredIdxInStack3"), BmValue::Tuple(vec![ts(), BmValue::Dict(vec![(b("C"), BmValue::Dict(vec![(b("m1"), BmValue::Int(0)), (b("m2"), BmValue::Int(1))]))])])),
        ]))])).unwrap()
    }

    fn window<'a>(v: &'a Value, id: &str) -> &'a Value {
        v["windows"].as_array().unwrap().iter().find(|w| w["id"] == id).unwrap_or_else(|| panic!("window {id}"))
    }

    #[test]
    fn layout_get_strips_every_path_and_reports_geometry_flags_and_stacks() {
        let (s, _) = open_char(&layout_char_bytes());
        let v = s.call("layout_get", &Args::new()).unwrap();
        assert_no_paths(&v, "layout_get");
        assert_eq!(v["reference_w"], 2560);
        let ov = window(&v, "overview");
        assert_eq!(ov["geom"], json!({ "x": 100, "y": 200, "w": 400, "h": 600, "screen_w": 2560, "screen_h": 1440 }));
        assert_eq!(ov["open"], true);
        assert_eq!(ov["resolution_matches"], true);
        // flags is now just the names that are ON for this window.
        assert!(ov["flags"].as_array().unwrap().contains(&json!("pinnedWindows")), "{ov}");
        // settable_flags is file-level: pinnedWindows has a dict in the file,
        // lockedWindows does not.
        let settable = v["settable_flags"].as_array().unwrap();
        assert!(settable.contains(&json!("pinnedWindows")), "{settable:?}");
        assert!(!settable.contains(&json!("lockedWindows")), "no lockedWindows dict in the file → not settable: {settable:?}");
        assert_eq!(window(&v, "market")["resolution_matches"], false);
        assert!(v["windows"].as_array().unwrap().iter().all(|w| w["id"] != "fitting"), "closed window omitted by default: {v}");
        assert_eq!(v["stacks"][0]["members"], json!(["m1", "m2"]));

        let v = s.call("layout_get", &args(json!({ "include_closed": true }))).unwrap();
        assert_eq!(window(&v, "fitting")["open"], false, "present once include_closed is true");
    }

    #[test]
    fn layout_edit_moves_a_window_and_restamps_a_mismatched_resolution() {
        let (s, _) = open_char(&layout_char_bytes());
        let v = s.call("layout_edit", &args(json!({ "ops": [
            { "op": "set_geometry", "window": "overview", "x": 0, "y": 0 },
            { "op": "set_geometry", "window": "market", "w": 640 }
        ]}))).unwrap();
        assert_eq!(window(&v, "overview")["geom"]["x"], 0);
        assert_eq!(window(&v, "overview")["geom"]["h"], 600, "untouched axis kept");
        let m = window(&v, "market");
        assert_eq!(m["geom"]["w"], 640);
        assert_eq!(m["geom"]["screen_w"], 2560, "re-stamped to the reference");
        assert_eq!(m["resolution_matches"], true);
        assert_eq!(undo::undo_state(&s.state()).depth, 1);
    }

    #[test]
    fn layout_edit_sets_flags_including_an_insert_and_refuses_the_unknown() {
        let (s, _) = open_char(&layout_char_bytes());
        let before = s.call("layout_get", &Args::new()).unwrap();
        let v = s.call("layout_edit", &args(json!({ "ops": [
            { "op": "set_flag", "window": "market", "flag": "pinnedWindows", "on": true },
            { "op": "set_flag", "window": "overview", "flag": "openWindows", "on": false }
        ]}))).unwrap();
        assert!(window(&v, "market")["flags"].as_array().unwrap().contains(&json!("pinnedWindows")), "an Insert target minted the key");
        // overview is now closed, so the open-only result no longer lists it.
        assert!(v["windows"].as_array().unwrap().iter().all(|w| w["id"] != "overview"), "{v}");
        let full = s.call("layout_get", &args(json!({ "include_closed": true }))).unwrap();
        assert_eq!(window(&full, "overview")["open"], false);
        let e = s.call("layout_edit", &args(json!({ "ops": [{ "op": "set_flag", "window": "market", "flag": "lockedWindows", "on": true }] }))).unwrap_err();
        assert_eq!(e["code"], "flag_unavailable");
        let e = s.call("layout_edit", &args(json!({ "ops": [
            { "op": "set_geometry", "window": "market", "x": 5 },
            { "op": "set_flag", "window": "market", "flag": "no_such_flag", "on": true }
        ]}))).unwrap_err();
        assert_eq!(e["code"], "unknown_flag");
        assert_eq!(e["op_index"], 1);
        let e = s.call("layout_edit", &args(json!({ "ops": [{ "op": "set_geometry", "window": "nope", "x": 1 }] }))).unwrap_err();
        assert_eq!(e["code"], "unknown_window");
        let e = s.call("layout_edit", &args(json!({ "ops": [{ "op": "set_geometry", "window": "overview" }] }))).unwrap_err();
        assert_eq!(e["code"], "missing_field");
        // The two failed batches left nothing behind (the first batch's edits stand).
        let after = s.call("layout_get", &Args::new()).unwrap();
        assert_eq!(window(&after, "market")["geom"]["x"], window(&before, "market")["geom"]["x"]);
    }

    #[test]
    fn layout_edit_stack_ops_round_trip() {
        let (s, _) = open_char(&layout_char_bytes());
        let v = s.call("layout_edit", &args(json!({ "ops": [
            { "op": "stack_unstack", "window": "m1" },
            { "op": "stack_add", "window": "m1", "container": "C" },
            { "op": "stack_reorder", "container": "C", "members": ["m2", "m1"] }
        ]}))).unwrap();
        assert_eq!(v["stacks"][0]["members"], json!(["m2", "m1"]));
        assert_eq!(undo::undo_state(&s.state()).depth, 1, "stack ops open their own group; the batch is still one step");
    }

    /// EVE keeps a stack's container and every member at one identical rect
    /// (docs/format-notes.md ~line 735), so moving one member must move the
    /// whole stack, not just the window asked for.
    #[test]
    fn set_geometry_on_a_stacked_window_moves_the_whole_stack() {
        let (s, _) = open_char(&layout_char_bytes());
        let v = s.call("layout_edit", &args(json!({ "ops": [
            { "op": "set_geometry", "window": "m1", "x": 500 }
        ]}))).unwrap();
        for id in ["m1", "m2", "C"] {
            assert_eq!(window(&v, id)["geom"]["x"], 500, "{id} should have moved with the stack: {v}");
        }
    }

    #[test]
    fn layout_render_returns_a_png_and_a_legend_through_the_tool() {
        use base64::Engine as _;
        let (s, _) = open_char(&layout_char_bytes());
        let v = s.call("layout_render", &args(json!({ "width": 400 }))).unwrap();
        assert_eq!(v["width"], 400);
        // Drawn only, by default: overview, market, and C (the stack's anchor)
        // — fitting (closed) and the non-anchor stack members m1/m2 are omitted.
        assert_eq!(v["windows"].as_array().unwrap().len(), 3, "{v}");
        // No account file and no `notifications` section: of the furniture,
        // only the ship HUD (character-scoped, with a default) is placeable.
        let furniture = v["furniture"].as_array().unwrap();
        assert_eq!(furniture.len(), 1, "{v}");
        assert_eq!(furniture[0]["kind"], "shipui");
        assert_eq!(furniture[0]["y"], 1440 - 12 - 176, "bottom-aligned at the file's reference height");
        let png = base64::engine::general_purpose::STANDARD.decode(v["png_base64"].as_str().unwrap()).unwrap();
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        let blocks = blocks(v);
        assert_eq!(blocks.len(), 2);
        assert!(matches!(blocks[1], ContentBlock::Image(_)));
    }

    /// The layout fixture plus one private chat (clutter), one docked-only
    /// window and an orphan numeric frame, all open; `fitting` closed.
    fn cluttered_layout_char_bytes() -> Vec<u8> {
        let ts = || BmValue::Long(vec![0u8; 8]);
        let geom = |x: i64| BmValue::Tuple(vec![BmValue::Int(x), BmValue::Int(0), BmValue::Int(300), BmValue::Int(200), BmValue::Int(2560), BmValue::Int(1440)]);
        let ids = ["overview", "market", "chatchannel_player_-78564080", "lobbyWnd", "9009", "fitting"];
        let sizes: Vec<(BmValue, BmValue)> = ids.iter().enumerate().map(|(i, id)| (b(id), geom(i as i64 * 100))).collect();
        let open: Vec<(BmValue, BmValue)> = ids.iter().map(|id| (b(id), BmValue::Bool(*id != "fitting"))).collect();
        encode(&BmValue::Dict(vec![(b("windows"), BmValue::Dict(vec![
            (b("windowSizesAndPositions_1"), BmValue::Tuple(vec![ts(), BmValue::Dict(sizes)])),
            (b("openWindows"), BmValue::Tuple(vec![ts(), BmValue::Dict(open)])),
        ]))])).unwrap()
    }

    fn ids_of(v: &Value) -> Vec<String> {
        v["windows"].as_array().unwrap().iter().map(|w| w["id"].as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn layout_get_hides_clutter_by_default_and_counts_what_it_hid() {
        let (s, _) = open_char(&cluttered_layout_char_bytes());
        let v = s.call("layout_get", &Args::new()).unwrap();
        let ids = ids_of(&v);
        assert!(ids.contains(&"overview".into()) && ids.contains(&"market".into()) && ids.contains(&"lobbyWnd".into()));
        assert!(!ids.contains(&"chatchannel_player_-78564080".into()), "a private chat is clutter");
        assert!(!ids.contains(&"9009".into()), "an orphan frame is clutter");
        assert!(!ids.contains(&"fitting".into()), "closed by default");
        assert_eq!(v["hidden"], json!({ "closed": 1, "clutter": 2, "environment": 0, "matched": 0 }));
        let v = s.call("layout_get", &args(json!({ "hide_clutter": false, "include_closed": true }))).unwrap();
        assert_eq!(ids_of(&v).len(), 6);
        assert_eq!(v["hidden"]["clutter"], 0);
    }

    #[test]
    fn layout_get_filters_by_environment_and_text() {
        let (s, _) = open_char(&cluttered_layout_char_bytes());
        let v = s.call("layout_get", &args(json!({ "environment": "space" }))).unwrap();
        let ids = ids_of(&v);
        assert!(ids.contains(&"overview".into()));
        assert!(!ids.contains(&"lobbyWnd".into()), "docked-only hidden in space");
        assert_eq!(v["hidden"]["environment"], 1);
        let v = s.call("layout_get", &args(json!({ "environment": "docked" }))).unwrap();
        assert!(!ids_of(&v).contains(&"overview".into()), "space-only hidden when docked");
        let v = s.call("layout_get", &args(json!({ "match": "MARK" }))).unwrap();
        assert_eq!(ids_of(&v), vec!["market".to_string()]);
        assert!(v["hidden"]["matched"].as_u64().unwrap() >= 2);
        assert_eq!(s.call("layout_get", &args(json!({ "environment": "orbit" }))).unwrap_err()["code"], "bad_arguments");
    }

    #[test]
    fn layout_render_legend_and_picture_use_the_same_filter() {
        let (s, _) = open_char(&cluttered_layout_char_bytes());
        let v = s.call("layout_render", &args(json!({ "width": 400 }))).unwrap();
        let drawn: Vec<&str> = v["windows"].as_array().unwrap().iter().filter(|w| w["drawn"] == true).map(|w| w["id"].as_str().unwrap()).collect();
        assert!(drawn.contains(&"market") && !drawn.contains(&"chatchannel_player_-78564080"));
        assert_eq!(v["hidden"]["clutter"], 2);
        let v = s.call("layout_render", &args(json!({ "width": 400, "hide_clutter": false, "environment": "docked" }))).unwrap();
        let drawn: Vec<&str> = v["windows"].as_array().unwrap().iter().filter(|w| w["drawn"] == true).map(|w| w["id"].as_str().unwrap()).collect();
        assert!(drawn.contains(&"chatchannel_player_-78564080") && !drawn.contains(&"overview"));
    }

    /// The stack bug: `C` (the anchor of m1+m2) does not itself contain "m1"
    /// in its id/label, so under `match: "m1"` the anchor alone would be
    /// filtered out — the fix must still draw and list the stack because a
    /// member (m1) matched.
    #[test]
    fn layout_render_draws_a_stack_whose_anchor_did_not_match_but_a_member_did() {
        let (s, _) = open_char(&layout_char_bytes());
        let v = s.call("layout_render", &args(json!({ "match": "m1" }))).unwrap();
        assert_eq!(window(&v, "C")["drawn"], true, "{v}");
    }

    #[test]
    fn the_users_clutter_overrides_are_honoured() {
        let (s, _) = open_char(&cluttered_layout_char_bytes());
        // A private preferences.json for this server: force market into
        // clutter and the private chat out of it.
        let prefs_dir = std::env::temp_dir().join(format!("mcp-prefs-{}", std::process::id()));
        std::fs::create_dir_all(&prefs_dir).unwrap();
        let prefs_path = prefs_dir.join("preferences.json");
        std::fs::write(&prefs_path, r#"{"layout":{"clutter":["market"],"visible":["chatchannel_player_-78564080"]}}"#).unwrap();
        let s = EveMcp { prefs_path: Some(prefs_path), ..s };
        let ids = ids_of(&s.call("layout_get", &Args::new()).unwrap());
        assert!(!ids.contains(&"market".into()) && ids.contains(&"chatchannel_player_-78564080".into()));
    }

    /// `overrides()` must read `preferences.json` without `prefs::load_from`'s
    /// quarantine behaviour: that rename-to-`.bad` recovery is right for the
    /// GUI's own writes but wrong for this read-only consumer, which could
    /// otherwise race the GUI's plain `fs::write` and quarantine a file the
    /// GUI never actually corrupted.
    #[test]
    fn a_corrupt_preferences_file_reads_as_defaults_and_is_left_untouched() {
        let (s, _) = open_char(&cluttered_layout_char_bytes());
        let prefs_dir = std::env::temp_dir().join(format!("mcp-prefs-corrupt-{}", std::process::id()));
        std::fs::create_dir_all(&prefs_dir).unwrap();
        let prefs_path = prefs_dir.join("preferences.json");
        std::fs::write(&prefs_path, b"not json {").unwrap();
        let s = EveMcp { prefs_path: Some(prefs_path.clone()), ..s };
        let v = s.call("layout_get", &args(json!({ "hide_clutter": false, "include_closed": true }))).unwrap();
        // No overrides applied: layout_get succeeds and lists every window,
        // as if preferences.json were simply absent.
        assert_eq!(ids_of(&v).len(), 6, "{v}");
        assert_eq!(std::fs::read(&prefs_path).unwrap(), b"not json {", "must not be quarantined");
        assert!(!prefs_path.with_extension("json.bad").exists(), "must not create a .bad file");
    }

    /// A discovery root with one install/profile holding source char 100 on
    /// account 500 and target char 200 on account 600, paired in accounts.json
    /// under a separate app dir — `setup::tests`' layout. Plus a SECOND
    /// install/profile folder (char 300 on account 700) for the "target in
    /// another profile folder" cases — `locate` (all profiles) resolves it,
    /// but `setup::target_ids` (folder-scoped) does not, unless asked.
    fn temp_profile() -> (PathBuf, PathBuf, PathBuf) {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!("mcp-copy-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let prof = base.join("root").join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        std::fs::create_dir_all(&prof).unwrap();
        let ts = || BmValue::Long(vec![0u8; 8]);
        let overview = |c: &str| BmValue::Dict(vec![(b("overview"), BmValue::Dict(vec![(b("overviewColumns"), BmValue::List(vec![b(c)]))]))]);
        let widths = || BmValue::Dict(vec![(b("ui"), BmValue::Dict(vec![(b("SortHeadersSizes"), BmValue::Tuple(vec![ts(), BmValue::Dict(vec![])]))]))]);
        std::fs::write(prof.join("core_char_100.dat"), encode(&widths()).unwrap()).unwrap();
        std::fs::write(prof.join("core_user_500.dat"), encode(&overview("SRC")).unwrap()).unwrap();
        std::fs::write(prof.join("core_char_200.dat"), encode(&widths()).unwrap()).unwrap();
        std::fs::write(prof.join("core_user_600.dat"), encode(&overview("TGT")).unwrap()).unwrap();

        let other = base.join("root").join("c_eve_sharedcache_tq_other").join("settings_Default");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("core_char_300.dat"), encode(&widths()).unwrap()).unwrap();
        std::fs::write(other.join("core_user_700.dat"), encode(&overview("OTH")).unwrap()).unwrap();

        let app_dir = base.join("appdata");
        std::fs::create_dir_all(&app_dir).unwrap();
        let mut store = crate::accounts::AccountsStore::default();
        store.accounts.insert(500, crate::accounts::Account { alias: None, characters: vec![100] });
        store.accounts.insert(600, crate::accounts::Account { alias: None, characters: vec![200] });
        store.accounts.insert(700, crate::accounts::Account { alias: None, characters: vec![300] });
        std::fs::write(app_dir.join("accounts.json"), serde_json::to_vec(&store).unwrap()).unwrap();
        (base.join("root"), prof, app_dir)
    }

    fn copy_server() -> (EveMcp, PathBuf) {
        let (root, prof, app_dir) = temp_profile();
        (EveMcp::new(app_dir, vec![root], None), prof)
    }

    #[test]
    fn copy_preview_plans_by_char_id_and_names_the_account_write() {
        let (s, _) = copy_server();
        let v = s.call("copy_preview", &args(json!({ "source_char_id": 100, "target_char_ids": [200], "aspects": ["overview"] }))).unwrap();
        assert_eq!(v["char_writes"][0]["char_id"], 200);
        assert_eq!(v["account_writes"][0]["user_id"], 600);
        assert_eq!(v["excluded"], json!([]));
        assert_eq!(v["source_error"], Value::Null);
    }

    #[test]
    fn copy_apply_writes_backs_up_and_a_fresh_open_shows_the_copy() {
        let (s, prof) = copy_server();
        let v = s.call("copy_apply", &args(json!({ "source_char_id": 100, "target_char_ids": [200], "aspects": ["overview"] }))).unwrap();
        let results = v.as_array().unwrap();
        assert!(results.iter().all(|r| r["ok"] == true), "{v}");
        let acct = results.iter().find(|r| r["path"].as_str().unwrap().contains("core_user_600")).unwrap();
        assert!(PathBuf::from(acct["backup_path"].as_str().unwrap()).exists());
        let bytes = std::fs::read(prof.join("core_user_600.dat")).unwrap();
        assert!(bytes.windows(3).any(|w| w == b"SRC"), "the source's overview subtree landed in the target file");
        assert!(!bytes.windows(3).any(|w| w == b"TGT"), "and replaced the target's");
        s.call("open", &args(json!({ "user_file": prof.join("core_user_600.dat").to_string_lossy() }))).unwrap();
    }

    #[test]
    fn copy_args_need_exactly_one_source_and_a_target() {
        let (s, _) = copy_server();
        assert_eq!(s.call("copy_preview", &args(json!({ "target_char_ids": [200], "aspects": ["overview"] }))).unwrap_err()["code"], "missing_field");
        assert_eq!(s.call("copy_preview", &args(json!({ "source_char_id": 100, "source_char_file": "x", "target_char_ids": [200], "aspects": ["overview"] }))).unwrap_err()["code"], "bad_arguments");
        assert_eq!(s.call("copy_preview", &args(json!({ "source_char_id": 100, "aspects": ["overview"] }))).unwrap_err()["code"], "missing_field");
        assert_eq!(s.call("copy_preview", &args(json!({ "source_char_id": 999, "target_char_ids": [200], "aspects": ["overview"] }))).unwrap_err()["code"], "unknown_character");
    }

    /// `copy_args` resolves `target_char_ids` across every profile (`locate`),
    /// but `setup::target_ids` only keeps paths in the source's own folder —
    /// without the fix, char 300 (a different install/profile) would vanish
    /// from both `char_writes` and `excluded` and `copy_apply` would silently
    /// skip it.
    #[test]
    fn copy_preview_names_a_target_in_another_profile_folder_and_allow_other_folders_includes_it() {
        let (s, _) = copy_server();
        let v = s.call("copy_preview", &args(json!({ "source_char_id": 100, "target_char_ids": [200, 300], "aspects": ["overview"] }))).unwrap();
        assert_eq!(v["char_writes"].as_array().unwrap().len(), 1, "300 is outside the anchor folder: {v}");
        assert_eq!(v["char_writes"][0]["char_id"], 200);
        let excl = v["excluded"].as_array().unwrap().iter().find(|e| e["char_id"] == 300).unwrap_or_else(|| panic!("300 named in excluded: {v}"));
        assert!(excl["reason"].as_str().unwrap().contains("allow_other_folders"), "{excl}");

        let v = s.call("copy_preview", &args(json!({
            "source_char_id": 100, "target_char_ids": [200, 300], "aspects": ["overview"], "allow_other_folders": true
        }))).unwrap();
        assert!(v["char_writes"].as_array().unwrap().iter().any(|w| w["char_id"] == 300), "{v}");
        assert!(!v["excluded"].as_array().unwrap().iter().any(|e| e["char_id"] == 300), "{v}");
    }

    #[test]
    fn copy_apply_reports_a_target_in_another_profile_folder_as_not_ok() {
        let (s, _) = copy_server();
        let v = s.call("copy_apply", &args(json!({ "source_char_id": 100, "target_char_ids": [200, 300], "aspects": ["overview"] }))).unwrap();
        let results = v.as_array().unwrap();
        let bad = results.iter().find(|r| r["path"].as_str().unwrap().contains("core_char_300")).unwrap_or_else(|| panic!("300 in results: {v}"));
        assert_eq!(bad["ok"], false);
        assert_eq!(bad["backup_path"], Value::Null);
        assert!(bad["error"].as_str().unwrap().contains("allow_other_folders"), "{bad}");
        assert!(results.iter().any(|r| r["ok"] == true), "200 still applied: {v}");
    }

    /// `keybinds` is account-only (`setup::aspect_writes`: no `char_categories`
    /// at all), so a plan for it never populates `char_writes` — the OLD
    /// plan-inference mechanism read "absent from char_writes" as "another
    /// folder" and wrongly excluded every in-folder target too. Pins the fix:
    /// folder membership is now computed directly from `discover`, not from
    /// what a plan happened to produce.
    ///
    /// `copy_server`'s source account file carries only `overview` data, which
    /// would make a `keybinds` splice extract nothing and get suppressed as a
    /// no-op by `setup.rs` itself — passing for the wrong reason (no writes at
    /// all, rather than a correctly-planned in-folder write). This gives the
    /// source real `cmd` data so the write is genuine.
    fn keybinds_copy_server() -> (EveMcp, PathBuf) {
        let (s, prof) = copy_server();
        let codes = |v: &[i64]| BmValue::Tuple(v.iter().map(|&n| BmValue::Int(n)).collect());
        let table = BmValue::Dict(vec![(b("CmdToggleAutopilot"), codes(&[81]))]);
        let cmd = BmValue::Dict(vec![(b("customCmds"), BmValue::Tuple(vec![BmValue::Long(vec![0u8; 8]), table]))]);
        std::fs::write(prof.join("core_user_500.dat"), encode(&BmValue::Dict(vec![(b("cmd"), cmd)])).unwrap()).unwrap();
        (s, prof)
    }

    #[test]
    fn copy_preview_with_an_account_only_aspect_does_not_exclude_an_in_folder_target() {
        let (s, _) = keybinds_copy_server();
        let v = s.call("copy_preview", &args(json!({ "source_char_id": 100, "target_char_ids": [200], "aspects": ["keybinds"] }))).unwrap();
        assert_eq!(v["excluded"], json!([]), "{v}");
        assert_eq!(v["account_writes"][0]["user_id"], 600, "{v}");
    }

    #[test]
    fn copy_apply_with_an_account_only_aspect_does_not_mark_an_in_folder_target_failed() {
        let (s, _) = keybinds_copy_server();
        let v = s.call("copy_apply", &args(json!({ "source_char_id": 100, "target_char_ids": [200], "aspects": ["keybinds"] }))).unwrap();
        let results = v.as_array().unwrap();
        assert!(!results.is_empty(), "{v}");
        assert!(results.iter().all(|r| r["ok"] == true), "{v}");
        assert!(
            !results.iter().any(|r| r["error"].as_str().is_some_and(|e| e.contains("allow_other_folders"))),
            "{v}"
        );
    }

    /// A `target_char_files` path spelled with forward slashes must still be
    /// recognised as the same in-folder file `discover` reports with
    /// backslashes — folder membership compares as `Path`, not text.
    #[test]
    fn copy_preview_is_not_confused_by_a_differently_spelled_in_folder_target_path() {
        let (s, prof) = copy_server();
        let spelled = prof.join("core_char_200.dat").to_string_lossy().replace('\\', "/");
        let v = s.call("copy_preview", &args(json!({ "source_char_id": 100, "target_char_files": [spelled], "aspects": ["overview"] }))).unwrap();
        assert_eq!(v["excluded"], json!([]), "{v}");
        assert_eq!(v["char_writes"][0]["char_id"], 200, "{v}");
    }

    #[test]
    fn copy_preview_from_a_non_settings_file_is_a_source_error() {
        let (s, prof) = copy_server();
        let bad = prof.join("not-settings.txt");
        std::fs::write(&bad, b"hello").unwrap();
        let v = s.call("copy_preview", &args(json!({ "source_char_file": bad.to_string_lossy(), "target_char_ids": [200], "aspects": ["overview"] }))).unwrap();
        assert!(v["source_error"].is_string(), "{v}");
    }

    #[test]
    fn copy_preview_from_a_settings_preset_names_unknown_then_succeeds_with_a_real_one() {
        let (root, prof, app_dir) = temp_profile();
        let s = EveMcp::new(app_dir, vec![root], None);

        let e = s.call("copy_preview", &args(json!({ "source_settings_preset": "nope", "target_char_ids": [200], "aspects": ["overview"] }))).unwrap_err();
        assert_eq!(e["code"], "unknown_settings_preset");

        s.call("open", &args(json!({
            "user_file": prof.join("core_user_500.dat").to_string_lossy(),
            "char_file": prof.join("core_char_100.dat").to_string_lossy()
        }))).unwrap();
        s.call("settings_preset_edit", &args(json!({ "op": "create", "name": "PvP kit", "aspects": ["overview"] }))).unwrap();

        let v = s.call("copy_preview", &args(json!({ "source_settings_preset": "PvP kit", "target_char_ids": [200], "aspects": ["overview"] }))).unwrap();
        assert_eq!(v["account_writes"][0]["user_id"], 600, "{v}");
        assert_eq!(v["source_error"], Value::Null);
    }

    #[test]
    fn copy_files_clones_a_file_onto_another_of_the_same_kind() {
        let (s, prof) = copy_server();
        let src = prof.join("core_user_500.dat");
        let tgt = prof.join("core_user_600.dat");
        let v = s.call("copy_files", &args(json!({ "source": src.to_string_lossy(), "targets": [tgt.to_string_lossy()] }))).unwrap();
        assert_eq!(v[0]["ok"], true);
        assert_eq!(std::fs::read(&src).unwrap(), std::fs::read(&tgt).unwrap());
        let e = s.call("copy_files", &args(json!({ "source": src.to_string_lossy(), "targets": [prof.join("core_char_200.dat").to_string_lossy()] }))).unwrap();
        assert_eq!(e[0]["ok"], false, "a different kind is refused per target: {e}");
    }

    #[test]
    fn settings_presets_create_from_the_open_files_then_rename_export_import_delete() {
        let (root, prof, app_dir) = temp_profile();
        let s = EveMcp::new(app_dir.clone(), vec![root], None);
        s.call("open", &args(json!({ "user_file": prof.join("core_user_500.dat").to_string_lossy(), "char_file": prof.join("core_char_100.dat").to_string_lossy() }))).unwrap();
        assert_eq!(s.call("settings_presets_list", &Args::new()).unwrap(), json!([]));

        let v = s.call("settings_preset_edit", &args(json!({ "op": "create", "name": "PvP kit", "aspects": ["overview"] }))).unwrap();
        assert_eq!(v["presets"][0]["name"], "PvP kit");
        assert_eq!(v["presets"][0]["aspects"], json!(["overview"]));
        assert_no_paths(&v, "settings_preset_edit");
        assert_eq!(s.call("settings_preset_edit", &args(json!({ "op": "create", "name": "PvP kit", "aspects": ["overview"] }))).unwrap_err()["code"], "preset");

        let v = s.call("settings_preset_edit", &args(json!({ "op": "rename", "name": "PvP kit", "new_name": "Fleet kit" }))).unwrap();
        assert_eq!(v["presets"][0]["name"], "Fleet kit");

        let out = app_dir.join("fleet-kit.esp");
        s.call("settings_preset_edit", &args(json!({ "op": "export", "name": "Fleet kit", "path": out.to_string_lossy() }))).unwrap();
        assert!(out.exists());
        s.call("settings_preset_edit", &args(json!({ "op": "delete", "name": "Fleet kit" }))).unwrap();
        assert_eq!(s.call("settings_presets_list", &Args::new()).unwrap(), json!([]));
        let v = s.call("settings_preset_edit", &args(json!({ "op": "import", "path": out.to_string_lossy() }))).unwrap();
        assert_eq!(v["imported_as"], "Fleet kit");
        assert_eq!(v["presets"].as_array().unwrap().len(), 1);
    }
}
