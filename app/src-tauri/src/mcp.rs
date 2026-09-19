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

use crate::ops::{AppState, ErrDto, Slot};
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

fn fail(e: ErrDto) -> Value {
    serde_json::to_value(e).unwrap_or_default()
}
fn err(code: &str, message: impl Into<String>) -> Value {
    fail(ErrDto::new(code, message))
}
#[allow(dead_code)]
fn ok<T: serde::Serialize>(v: T) -> ToolResult {
    serde_json::to_value(v).map_err(|e| err("serialize", e.to_string()))
}
fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

/// A required argument, deserialised into whatever the op wants.
#[allow(dead_code)]
fn req<T: serde::de::DeserializeOwned>(args: &Args, key: &str) -> Result<T, Value> {
    match args.get(key) {
        None => Err(err("missing_field", format!("`{key}` is required"))),
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| err("bad_arguments", format!("`{key}`: {e}"))),
    }
}
/// An optional argument. Absent and JSON null both mean "not given".
#[allow(dead_code)]
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
    vec![ToolDef {
        name: "status",
        description: "What is open right now: the character and account file paths, whether each has unsaved edits, and whether undo is possible. Call it to re-orient in a long conversation.",
        schema: || obj(json!({}), &[]),
    }]
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
    fn call(&self, name: &str, _args: &Args) -> ToolResult {
        match name {
            "status" => self.status(),
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
}

impl ServerHandler for EveMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("eve-settings-editor", env!("CARGO_PKG_VERSION")))
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
}
