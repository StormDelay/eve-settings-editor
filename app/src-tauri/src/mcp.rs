//! MCP server — the `--mcp` mode of the same executable. A second adapter over
//! `ops`, beside the Tauri commands in lib.rs: every tool is a plain function
//! over `AppState`, and this file only parses arguments and serialises results.
//! Spec: docs/superpowers/specs/2026-09-19-mcp-server-design.md.
//!
//! stdout IS the protocol here. Nothing in this process may print to it.

use std::path::PathBuf;

use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{ErrorData as McpError, ServerHandler, ServiceExt};
use serde_json::{json, Map, Value};

use settings_model::{discover, FileKind, Profile};

use crate::accounts::{self, AccountRoster};
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
#[allow(dead_code)]
const STATES_JSON: &str = include_str!("../../src/lib/data/overview-states.json");

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
            description: "Revert the last unsaved edit (one tool call = one step; a batch is one step). Returns status. Fails with `nothing_to_undo` when there is nothing to revert. For a saved change, use list_backups and restore_backup instead.",
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
        let user = open_slot(&self.state, Slot::User, &user_file)?;
        let char = match char_file {
            Some(p) => Some(open_slot(&self.state, Slot::Char, &p)?),
            None => {
                ops::close_file(&self.state, Slot::Char);
                None
            }
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
            Ok(v) => CallToolResult::success(vec![ContentBlock::text(pretty(&v))]),
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
}
