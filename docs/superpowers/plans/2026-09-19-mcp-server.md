# MCP Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An MCP server inside the existing executable (`--mcp`) so an AI client can edit overview settings and probe formations through the app's own `ops` layer, plus an "AI access" sheet that registers it with Claude Desktop or hands any other client the config.

**Architecture:** `main.rs` branches on `--mcp` into `mcp::serve()`, which never touches Tauri: a `tokio` current-thread runtime runs an `rmcp` stdio server whose handler owns one `AppState`. Every tool is a plain function over that state calling `ops::*`; `mcp.rs` only parses arguments and serialises results. Batched edit tools hold an `undo::group`, so a failing op rolls the whole batch back through the mechanism `edit_reshared` already has. A second file, `mcp_setup.rs`, writes the Claude Desktop config; a Svelte sheet fronts it.

**Tech Stack:** Rust (Tauri 2 app crate `app`, lib `app_lib`), `rmcp` 3.x (`server`, `transport-io`), `tokio`, `dirs`, `serde_json`; Svelte 5 + vitest for the sheet.

**Spec:** `docs/superpowers/specs/2026-09-19-mcp-server-design.md` — read it first; every task below cites the section it implements.

## Global Constraints

- Branch: `feat/mcp-server` (already exists; the spec is committed on it). Work in the main checkout — `target/` is warm there, and a fresh worktree's build cache has filled the disk before.
- Tool names: `^[a-z][a-z0-9_]*$`, ≤ 40 chars (spec §3.1).
- Input schemas: hand-written `json!`, `type: object`, `additionalProperties: false`, explicit `required`; **no** `oneOf`, `anyOf`, `allOf`, `$ref`, `"null"` types, or `type` arrays. Optional arguments are omitted, never null (spec §3.1).
- Domain errors are tool results with `isError: true` carrying `{"code", "message"}`; protocol codes `unknown_tool`, `bad_arguments`, `missing_field` (spec §3.1).
- Nothing in `crates/` changes; nothing in `ops.rs` changes (spec §9).
- Nothing may write to stdout in `--mcp` mode except rmcp (stdout *is* the protocol). Use `eprintln!` if you must print.
- CI gates: `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and in `app/`: `npm run check`, `npm test`. Verify by **exit code** — the test runner has exited 1 with every test passing before.
- Commits end with `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`.
- Tool count at the end: **24** (7 session, 9 overview, 6 probes, 2 context).

---

## File map

| File | Responsibility |
|---|---|
| `app/src-tauri/Cargo.toml` | `rmcp`, `tokio`, `dirs` |
| `app/src-tauri/src/main.rs` | the `--mcp` branch |
| `app/src-tauri/src/lib.rs` | `pub mod mcp; mod mcp_setup;`, `app_dir_base()`, the two setup commands registered |
| `app/src-tauri/src/mcp.rs` | **new** — `EveMcp`, `serve()`, the `ServerHandler` impl, the tool table, every tool handler, catalog `include_str!`s, unit tests |
| `app/src-tauri/src/mcp_primer.md` | **new** — five-section EVE primer; `## workflow` doubles as `instructions` |
| `app/src-tauri/src/mcp_setup.rs` | **new** — exe path, Claude Desktop config read/merge/write, two Tauri commands, unit tests |
| `app/src-tauri/src/undo.rs` | `group()` becomes re-entrant (Task 7) |
| `app/src-tauri/src/groups.rs` | `pub fn cached(dir)` |
| `app/src-tauri/tests/mcp_stdio.rs` | **new** — handshake smoke test over real pipes |
| `app/src/lib/api.ts` | `mcpSetupInfo`, `mcpSetClaudeDesktop` |
| `app/src/lib/AiAccessPanel.svelte` + `.spec.ts` | **new** — the sheet |
| `app/src/lib/commands.ts`, `AppMenu.svelte`, `AppMenu.spec.ts`, `routes/+page.svelte` | the `help.aiAccess` command and its mount |
| `README.md`, `CHANGELOG.md` | docs |

---

### Task 1: Boot, handshake, and the Windows stdio probe

Spec §2.1, §2.3, §2.4, §10 item 1. Delivers a binary that answers `initialize` over stdio with zero tools, plus the smoke test that proves it — and the manual release-build probe that gates everything after.

**Files:**
- Modify: `app/src-tauri/Cargo.toml`
- Modify: `app/src-tauri/src/main.rs`
- Modify: `app/src-tauri/src/lib.rs` (module list, `app_dir`)
- Create: `app/src-tauri/src/mcp.rs`
- Create: `app/src-tauri/tests/mcp_stdio.rs`

**Interfaces:**
- Produces: `pub struct EveMcp { pub state: AppState, pub dir: PathBuf, pub roots: Vec<PathBuf> }`, `EveMcp::new(dir: PathBuf, roots: Vec<PathBuf>) -> Self`, `pub fn serve()`, `pub(crate) fn app_dir_base() -> Option<PathBuf>` in `lib.rs`.

- [ ] **Step 1: Add the dependencies**

In `app/src-tauri/Cargo.toml`, under `[dependencies]`, after the `yaml-rust2` line:

```toml
# MCP server (`--mcp`, mcp.rs): a second adapter over `ops`, beside the Tauri
# commands. Hand-written schemas, so rmcp's macros and schemars are unused.
rmcp = { version = "3", features = ["server", "transport-io"] }
tokio = { version = "1", features = ["rt", "time", "io-std"] }
# What Tauri's `data_dir()` calls, so the MCP process resolves the same app dir
# without a Tauri handle (lib.rs `app_dir_base`).
dirs = "6"
```

- [ ] **Step 2: Write the failing smoke test**

Create `app/src-tauri/tests/mcp_stdio.rs`:

```rust
//! Handshake smoke test over real pipes. The tools are covered without a
//! process in mcp.rs; this only proves the binary speaks MCP on stdio.
//!
//! `MCP_EXE=<path> cargo test --test mcp_stdio` runs it against another build.
//! The Windows *release* exe is the one that needs it: it is built with
//! `windows_subsystem = "windows"` and this is the only check that its piped
//! stdio works (spec §2.1).

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn exe() -> String {
    std::env::var("MCP_EXE").unwrap_or_else(|_| env!("CARGO_BIN_EXE_app").to_string())
}

fn request(stdin: &mut impl Write, out: &mut impl BufRead, body: &str) -> serde_json::Value {
    writeln!(stdin, "{body}").unwrap();
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON-RPC: {e}: {line}"))
}

#[test]
fn initialize_then_tools_list() {
    let mut child = Command::new(exe())
        .arg("--mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn --mcp");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    let init = request(
        &mut stdin,
        &mut out,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}"#,
    );
    assert_eq!(init["id"], 1, "{init}");
    assert_eq!(init["result"]["serverInfo"]["name"], "eve-settings-editor", "{init}");

    writeln!(stdin, r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#).unwrap();
    let list = request(&mut stdin, &mut out, r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
    assert_eq!(list["id"], 2, "{list}");
    assert!(list["result"]["tools"].is_array(), "{list}");

    drop(stdin);
    let _ = child.wait();
}
```

- [ ] **Step 3: Run it to verify it fails**

Run (from `app/src-tauri`): `cargo test --test mcp_stdio`
Expected: compile error — `app_lib::mcp` does not exist yet (the binary builds `main.rs`, which is unchanged, so the test compiles but the spawned process opens a window; if it hangs, Ctrl-C — the next steps fix it).

- [ ] **Step 4: Branch `main.rs`**

Replace `app/src-tauri/src/main.rs` with:

```rust
// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `--mcp`: speak MCP over stdio instead of opening the window. The parent
    // process (an AI client) owns the pipes — see mcp.rs.
    if std::env::args().any(|a| a == "--mcp") {
        app_lib::mcp::serve()
    } else {
        app_lib::run()
    }
}
```

- [ ] **Step 5: `app_dir_base()` and the module in `lib.rs`**

In `app/src-tauri/src/lib.rs`, add `pub mod mcp;` to the module list (alphabetically, after `mod launcher;`). Then replace the `app_dir` function with:

```rust
/// The app dir resolved without a Tauri handle. `dirs::data_dir()` is what
/// Tauri's `data_dir()` calls, so the MCP process (`mcp.rs`) lands in the same
/// folder as the window — names cache, accounts.json, groups cache shared.
pub(crate) fn app_dir_base() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(APP_DIR))
}

pub(crate) fn app_dir(app: &tauri::AppHandle) -> PathBuf {
    app_dir_base()
        .or_else(|| app.path().data_dir().ok().map(|d| d.join(APP_DIR)))
        .unwrap_or_else(std::env::temp_dir)
}
```

- [ ] **Step 6: Create `mcp.rs` with the handler and `serve()`**

Create `app/src-tauri/src/mcp.rs`:

```rust
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

use crate::ops::AppState;

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
        Ok(ListToolsResult::with_all_items(vec![]))
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
```

If `async fn list_tools` is rejected by the compiler (the trait declares `impl Future<…> + MaybeSendFuture + '_`), write it as `fn list_tools(…) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ { std::future::ready(Ok(ListToolsResult::with_all_items(vec![]))) }` — rmcp's own examples use `async fn`, so expect the first form to work.

- [ ] **Step 7: Build and run the smoke test**

Run: `cargo test --test mcp_stdio` (from `app/src-tauri`)
Expected: PASS. If `enable_all` is not found, replace it with `.enable_time()`; if rmcp complains about the tokio `io-std` feature, it is already enabled transitively — remove it from our line.

- [ ] **Step 8: Clippy**

Run: `cargo clippy --workspace --all-targets -- -D warnings` (from repo root)
Expected: clean. Fix anything it names.

- [ ] **Step 9: Commit**

```bash
git add app/src-tauri/Cargo.toml Cargo.lock app/src-tauri/src/main.rs app/src-tauri/src/lib.rs app/src-tauri/src/mcp.rs app/src-tauri/tests/mcp_stdio.rs
git commit -m "MCP: --mcp boots a stdio server that answers initialize

Same exe, no window, no Tauri runtime; rmcp over tokio. Zero tools yet.
The smoke test speaks the handshake over real pipes and takes MCP_EXE so
the Windows release build can be probed with it.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

- [ ] **Step 10: The Windows release probe (spec §10 item 1) — manual, Windows only**

Check free space first: `Get-PSDrive D` in PowerShell; a release build needs several GB. Then, from `app/`:

```powershell
npm run build            # only if app/build is missing — generate_context! embeds it
cd src-tauri
cargo build --release
$env:MCP_EXE = (Resolve-Path ..\..\target\release\app.exe)
cargo test --test mcp_stdio -- --nocapture
Remove-Item Env:MCP_EXE
```

Expected: PASS against the release exe — the `windows_subsystem = "windows"` binary answered over piped stdio. **If this fails, stop and report**: the fallback is re-attaching a console in `--mcp` mode or a separate bin target, and that is a spec decision, not a plan step.

Record the outcome in the commit message of Task 2 ("Release probe on Windows: passed").

---

### Task 2: Tool plumbing, argument helpers, schema invariants, and `status`

Spec §3.1, §3.2 (`status`), §8 (schema invariants). Delivers the table every later task appends to, the dispatch, and the compatibility guard as a test.

**Files:**
- Modify: `app/src-tauri/src/mcp.rs`

**Interfaces:**
- Produces: `type Args = serde_json::Map<String, Value>`, `type ToolResult = Result<Value, Value>`, `fn fail(ErrDto) -> Value`, `fn err(code, msg) -> Value`, `fn ok<T: Serialize>(T) -> ToolResult`, `fn req<T: DeserializeOwned>(&Args, &str) -> Result<T, Value>`, `fn opt<T>(&Args, &str) -> Result<Option<T>, Value>`, `fn obj(properties: Value, required: &[&str]) -> Value`, `struct ToolDef { name, description, schema: fn() -> Value }`, `fn tool_defs() -> Vec<ToolDef>`, `impl EveMcp { fn call(&self, name: &str, args: &Args) -> ToolResult; fn status(&self) -> ToolResult }`, `#[cfg(test)] EveMcp::for_tests() -> Self`.

- [ ] **Step 1: Write the failing tests**

Append to `app/src-tauri/src/mcp.rs`:

```rust
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

    fn assert_single_string_types(v: &Value, tool: &str) {
        match v {
            Value::Object(m) => {
                if let Some(t) = m.get("type") {
                    assert!(t.is_string(), "{tool}: `type` must be one string, got {t}");
                }
                m.values().for_each(|c| assert_single_string_types(c, tool));
            }
            Value::Array(a) => a.iter().for_each(|c| assert_single_string_types(c, tool)),
            _ => {}
        }
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p app mcp::` (from repo root)
Expected: compile errors — `for_tests`, `call`, `req`, `opt`, `tool_defs` undefined.

- [ ] **Step 3: Implement the plumbing**

In `mcp.rs`, extend the imports and add, between the `EveMcp` struct and the `ServerHandler` impl:

```rust
use serde_json::{json, Map, Value};

use crate::ops::{self, AppState, ErrDto, Slot};
use crate::undo;

type Args = Map<String, Value>;
/// A tool's outcome: the JSON the model reads, or the JSON error it reads.
type ToolResult = Result<Value, Value>;

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
    fn call(&self, name: &str, args: &Args) -> ToolResult {
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
```

Then replace the `list_tools` body and add `call_tool` in the `ServerHandler` impl:

```rust
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
```

`ErrDto` needs `Deserialize`? No — only `Serialize`, which it has. `Slot` already derives `Deserialize` with `snake_case`, so `req::<Slot>(args, "slot")` accepts `"char"` / `"user"`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p app mcp::`
Expected: 5 passed.

- [ ] **Step 5: Clippy, then commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings`

```bash
git add app/src-tauri/src/mcp.rs
git commit -m "MCP: tool table, argument helpers, schema invariants, status

Release probe on Windows: <passed | see Task 1 note>.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: The primer, `eve_guide`, and `instructions`

Spec §3.5, §3.6, §8 (primer tests). Delivers the EVE context the model reads.

**Files:**
- Create: `app/src-tauri/src/mcp_primer.md`
- Modify: `app/src-tauri/src/mcp.rs`

**Interfaces:**
- Produces: `const PRIMER: &str`, `const STATES_JSON: &str`, `pub(crate) const TOPICS: [&str; 5]`, `fn primer_sections() -> Vec<(&'static str, &'static str)>`, `fn primer(topic: &str) -> Option<&'static str>`, `fn instructions() -> &'static str`, tool `eve_guide`.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `mcp.rs`:

```rust
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
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p app mcp::`
Expected: compile errors — `primer_sections`, `TOPICS`, `instructions`, `STATES_JSON` undefined.

- [ ] **Step 3: Write the primer**

Create `app/src-tauri/src/mcp_primer.md`. It must start with `## workflow` on line 1 (no preamble) and contain exactly these five `## ` headings in this order. This text is what the model reads — it is reviewed like UI copy.

```markdown
## workflow

EVE Online keeps each player's client settings in two files this server edits. `core_user_<id>.dat` is the **account** file: overview presets and appearance, and probe scan formations live there. `core_char_<id>.dat` is the **character** file: overview column widths and window layout. One account owns up to three characters, so several characters share one account file.

Sequence: `list_characters` (which files exist and who they belong to) → `open` (the account file is required; add the character file for column widths) → `overview_get` or `probes_get` → the edit tools → `save`.

Rules:

1. The EVE client rewrites its settings when a character logs out. Only edit files of characters that are logged out, or the edit is overwritten.
2. Nothing reaches disk until `save`. Every edit tool changes memory and returns the new state. `restore_backup` is the one exception and says so.
3. `save` fails with `conflict` when the file changed on disk since `open` — the game or the editor wrote it. Ask the user before retrying with `force: true`.
4. Every save is backed up first. `list_backups` and `restore_backup` undo a save; `undo` reverts an unsaved edit.
5. Group ids come from `groups_search`; state ids and labels come from `overview_get` under `names.states`; EVE's built-in presets come from `builtin_presets`.

Unsure about a concept: `eve_guide` with `overview`, `presets`, `states` or `probes`.

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
```

- [ ] **Step 4: Implement the primer accessors and `eve_guide`**

In `mcp.rs`, after the `type` aliases:

```rust
/// The EVE context the model reads (spec §3.6). One file, two exposures: the
/// `## workflow` section is the server's `instructions`; `eve_guide` returns
/// any one section. Compiled in so it ships with the tool.
const PRIMER: &str = include_str!("mcp_primer.md");
/// State ids → labels, the same file the UI ships. The primer test pins that
/// every id here appears in the `states` section.
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
```

Add to `tool_defs()`:

```rust
        ToolDef {
            name: "eve_guide",
            description: "Explains this server's model of EVE settings. Topics: workflow (files, sequence, rules), overview (windows, tabs, columns, indices), presets (groups, filtered and always-shown states, built-ins), states (ids and labels, background and flag lists), probes (formations, metres, axes, YAML). Call it before your first edit of a kind you have not done in this conversation.",
            schema: || obj(json!({ "topic": { "type": "string", "enum": TOPICS } }), &["topic"]),
        },
```

Add to `call`: `"eve_guide" => self.eve_guide(args),` and the handler:

```rust
    fn eve_guide(&self, args: &Args) -> ToolResult {
        let topic: String = req(args, "topic")?;
        match primer(&topic) {
            Some(text) => Ok(json!({ "topic": topic, "text": text })),
            None => Err(err("unknown_topic", format!("no topic `{topic}`; one of {}", TOPICS.join(", ")))),
        }
    }
```

And in `get_info`, append `.with_instructions(instructions())` to the builder chain.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p app mcp::`
Expected: 9 passed.

- [ ] **Step 6: Extend the smoke test**

In `tests/mcp_stdio.rs`, after the `serverInfo` assert:

```rust
    assert!(
        init["result"]["instructions"].as_str().map_or(false, |s| s.contains("core_user_")),
        "instructions missing: {init}"
    );
```

Run: `cargo test --test mcp_stdio` (from `app/src-tauri`). Expected: PASS.

- [ ] **Step 7: Clippy, commit**

```bash
git add app/src-tauri/src/mcp.rs app/src-tauri/src/mcp_primer.md app/src-tauri/tests/mcp_stdio.rs
git commit -m "MCP: the EVE primer, as instructions and as eve_guide

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Session tools — `open`, `save`, `undo`, `list_backups`, `restore_backup`

Spec §3.2 (all but `list_characters`), §7. Delivers the file lifecycle.

**Files:**
- Modify: `app/src-tauri/src/mcp.rs`

**Interfaces:**
- Consumes: `req`, `opt`, `fail`, `err`, `ok`, `status`, `doc_path`.
- Produces: `fn locate(char_id: u64, profiles: &[Profile], roster: &AccountRoster) -> Option<(PathBuf, Option<PathBuf>)>`, `fn open_slot(state: &AppState, slot: Slot, path: &str) -> Result<Value, Value>`, tools `open`, `save`, `undo`, `list_backups`, `restore_backup`. Test helper `fn open_user(bytes: &[u8]) -> (EveMcp, PathBuf)`.

Note for the spec: `ops::restore_backup` re-opens the file and does not return the pre-restore backup's path, so `restore_backup` here returns `{path, status}` and its description says the pre-restore backup is `list_backups`' newest entry. Update spec §3.2's row to match in Task 11.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests`:

```rust
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
```

`AccountView` and `AccountRoster` must be constructible from the test — check `accounts.rs` lines 109–120: both are `pub struct` with `pub` fields. If `AccountView` lacks `pub` on a field, add it.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p app mcp::`
Expected: compile errors (`locate` undefined) and `unknown_tool` failures.

- [ ] **Step 3: Implement**

Add the imports at the top of `mcp.rs`:

```rust
use settings_model::{discover, FileKind, Profile};

use crate::accounts::{self, AccountRoster};
use crate::ops::OpenOutcome;
```

Add to `tool_defs()`:

```rust
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
            description: "Write every slot with unsaved edits to disk: encode, verify by decoding, back up the current file, then replace it atomically. Returns each backup path. Fails with `conflict` if the file changed on disk since open (the EVE client or the editor wrote it) — ask the user before retrying with force. Only edit a character that is logged out.",
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
```

Add to `call`:

```rust
            "open" => self.open(args),
            "save" => self.save(args),
            "undo" => self.undo(),
            "list_backups" => ok(ops::list_file_backups(&self.state, req(args, "slot")?).map_err(fail)?),
            "restore_backup" => self.restore_backup(args),
```

Add the free function and the methods:

```rust
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
            let report = ops::save_document(&self.state, slot, force).map_err(|e| match e.code.as_str() {
                "conflict" => err(
                    "conflict",
                    "the file changed on disk since it was opened (the EVE client or the editor wrote it). Ask the user before retrying with force: true.",
                ),
                _ => fail(e),
            })?;
            saved.push(json!({ "slot": name, "path": path, "backup_path": report.backup_path }));
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
```

`report.backup_path` is a `PathBuf`; `json!` serialises it as a string. `SaveReport`'s fields are `pub` (`crates/settings-model/src/save.rs:19`).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p app mcp::`
Expected: all pass (17).

- [ ] **Step 5: Clippy, commit**

```bash
git add app/src-tauri/src/mcp.rs
git commit -m "MCP: open, save, undo, list_backups, restore_backup

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: `list_characters`

Spec §3.2. Delivers discovery + roster + names in one call.

**Files:**
- Modify: `app/src-tauri/src/mcp.rs`

**Interfaces:**
- Consumes: `locate`'s inputs (`discover`, `accounts::load_roster`), `crate::names::{resolve_blocking, Cache, ResolvedName}`.
- Produces: `fn characters(profiles: &[Profile], roster: &AccountRoster, names: &names::Cache) -> Value`, tool `list_characters`.

- [ ] **Step 1: Write the failing test**

```rust
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
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p app mcp::characters_lists`
Expected: compile error — `characters` undefined.

- [ ] **Step 3: Implement**

Add `use crate::names;` to the imports. Add the tool def:

```rust
        ToolDef {
            name: "list_characters",
            description: "Every EVE profile on this machine with its characters (id, name when known, character file) and, where the editor has paired them, the account file each belongs to. Accounts with no paired character are listed as unpaired_accounts. Start here; then open.",
            schema: || obj(json!({}), &[]),
        },
```

`call`: `"list_characters" => self.list_characters(),`. The function and method:

```rust
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

impl EveMcp {
    fn list_characters(&self) -> ToolResult {
        let profiles = discover(&self.roots);
        let roster = accounts::load_roster(&self.roots, &self.dir);
        let ids: Vec<u64> = profiles
            .iter()
            .flat_map(|p| p.files.iter().filter(|f| f.kind == FileKind::Char).filter_map(|f| f.id))
            .collect();
        // Cache first, ESI for the rest — the window's first launch does the same.
        let names = names::resolve_blocking(&self.dir, &ids, false);
        Ok(characters(&profiles, &roster, &names))
    }
}
```

`names::Cache` is `pub type Cache = HashMap<u64, ResolvedName>` and `ResolvedName`'s fields are `pub` (`names.rs:15`, `:36`). `Profile.install/server/profile` are `pub String`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p app mcp::`
Expected: all pass (18).

- [ ] **Step 5: Clippy, commit**

```bash
git add app/src-tauri/src/mcp.rs
git commit -m "MCP: list_characters

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: Overview reads — `overview_get`, `groups_search`, `builtin_presets`

Spec §3.3 (`overview_get`, `groups_search`), §3.6 (`builtin_presets`), §8. Delivers self-describing data and the catalogs as tools.

**Files:**
- Modify: `app/src-tauri/src/groups.rs` (`pub fn cached`)
- Modify: `app/src-tauri/src/mcp.rs`

**Interfaces:**
- Produces: `groups::cached(dir: &Path) -> Vec<GroupEntry>`; in `mcp.rs`: `const GROUPS_JSON`, `const PRESETS_JSON`, `const PRESET_NAMES_JSON`, `struct GroupRow { id: i64, name: String, category: String }`, `fn group_catalog(dir: &Path) -> Vec<GroupRow>`, `fn state_labels() -> HashMap<i64, String>`, `struct BuiltinPreset { key, name, display_name, era, groups, filtered_states, always_shown_states }`, `fn builtin_catalog() -> Vec<BuiltinPreset>`, `impl EveMcp { fn overview_get(&self) -> ToolResult; fn overview_with_names(&self, oc: OverviewColumns) -> ToolResult }`, tools `overview_get`, `groups_search`, `builtin_presets`.

- [ ] **Step 1: `groups::cached` — test and implementation**

In `app/src-tauri/src/groups.rs`, add after `load_cache`:

```rust
/// The delta cache's entries, read-only — no network. For a caller that only
/// needs names for ids it already has (the MCP server's catalogs).
pub fn cached(dir: &Path) -> Vec<GroupEntry> {
    load_cache(dir).groups.into_values().collect()
}
```

And in its `mod tests` (find the existing test module; it writes caches to temp dirs — reuse its temp-dir helper if it has one, else the inline one below):

```rust
    #[test]
    fn cached_reads_the_delta_without_a_fetcher() {
        let dir = std::env::temp_dir().join(format!("groups-cached-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert!(cached(&dir).is_empty());
        let entry = GroupEntry { id: 424242, name: "Future Frigate".into(), category_id: 6, category_name: "Ship".into() };
        let cache = GroupCache { version: Some("v".into()), groups: HashMap::from([(424242, entry.clone())]) };
        fs::write(cache_path(&dir), serde_json::to_vec(&cache).unwrap()).unwrap();
        assert_eq!(cached(&dir), vec![entry]);
    }
```

Run: `cargo test -p app groups::cached_reads` — expected PASS.

- [ ] **Step 2: Write the failing MCP tests**

Add to `mod tests` in `mcp.rs`:

```rust
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
```

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test -p app mcp::`
Expected: the four new tests fail with `unknown_tool`.

- [ ] **Step 4: Implement**

Catalogs and helpers, after `STATES_JSON`:

```rust
use std::collections::HashMap;
use std::path::Path;

use settings_model::OverviewColumns;

use crate::groups;

/// The bundled group catalog the UI ships: `{categories: [{id, name, groups: [{id, name}]}]}`.
const GROUPS_JSON: &str = include_str!("../../src/lib/data/overview-groups.json");
/// EVE's built-in presets, `{modern: [...], legacy: [...]}`, each
/// `{key, name, groups, filteredStates, alwaysShownStates}`.
const PRESETS_JSON: &str = include_str!("../../src/lib/data/default-presets.json");
/// Display names for the modern built-ins, keyed by the numeric part of `key`.
const PRESET_NAMES_JSON: &str = include_str!("../../src/lib/data/default-preset-names.json");

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
    let convert = |era: &'static str| {
        move |r: Raw| {
            let display_name = names.get(r.key.trim_start_matches("DefaultPreset_")).cloned().unwrap_or_else(|| r.name.clone());
            BuiltinPreset {
                key: r.key, name: r.name, display_name, era,
                groups: r.groups, filtered_states: r.filtered_states, always_shown_states: r.always_shown_states,
            }
        }
    };
    let mut out: Vec<BuiltinPreset> = file.modern.into_iter().map(convert("modern")).collect();
    out.extend(file.legacy.into_iter().map(convert("legacy")));
    out
}
```

The `convert` closure captures `names` by reference twice — if the borrow checker objects, clone `names` into each closure (`let names = names.clone();` inside a small `fn convert(era, names: &HashMap<..>) -> impl Fn(Raw) -> BuiltinPreset + '_`).

Tool defs:

```rust
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
```

`call` arms:

```rust
            "overview_get" => self.overview_get(),
            "groups_search" => self.groups_search(args),
            "builtin_presets" => self.builtin_presets(args),
```

Methods:

```rust
impl EveMcp {
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
        let by_display: Vec<&BuiltinPreset> = catalog.iter().filter(|p| p.display_name.to_lowercase() == q).collect();
        let hits = if by_display.is_empty() {
            catalog.iter().filter(|p| p.name.to_lowercase() == q || p.key.to_lowercase() == q).collect()
        } else {
            by_display
        };
        match hits.as_slice() {
            [one] => ok(one),
            [] => Err(err("unknown_preset", format!("no built-in preset `{name}`; call builtin_presets without a name for the list"))),
            many => Err(err(
                "ambiguous_preset",
                format!("`{name}` matches {}; use a display_name", many.iter().map(|p| format!("\"{}\"", p.display_name)).collect::<Vec<_>>().join(", ")),
            )),
        }
    }
}
```

`OverviewColumns` derives `Serialize` (its Tauri command returns it). `StateSurface.enabled/order` and `Appearance.colors/flag_colors` are `pub` (`overview.rs:43–58`).

- [ ] **Step 5: Run the tests**

Run: `cargo test -p app mcp::`
Expected: all pass (22).

- [ ] **Step 6: Clippy, commit**

```bash
git add app/src-tauri/src/groups.rs app/src-tauri/src/mcp.rs
git commit -m "MCP: overview_get with names, groups_search, builtin_presets

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: Re-entrant undo groups, and the four batched overview edit tools

Spec §3.1 (batch atomicity), §3.3 (edit tools), §8. `edit_reshared` already rolls an open group back on any failing write, so a batch under one group is atomic for free — **but** `undo::group` is not re-entrant, and `overview_copy_columns`, `tab_delete`, `overview_window_add` and `overview_window_remove` each open their own group. An inner group's `Drop` would close the outer one mid-batch. This task fixes that first.

**Files:**
- Modify: `app/src-tauri/src/undo.rs` (`Group`, `group()`)
- Modify: `app/src-tauri/src/mcp.rs`

**Interfaces:**
- Produces: `impl EveMcp { fn batch(&self, args: &Args, apply: impl FnMut(&AppState, &Args) -> Result<(), ErrDto>) -> ToolResult }`, tools `overview_columns_edit`, `overview_tabs_edit`, `overview_presets_edit`, `overview_appearance_edit`.

- [ ] **Step 1: Failing test for group re-entrancy**

`undo.rs`'s own test module has no state builder (the behavioural undo tests live in `ops.rs`, which this plan leaves untouched), so this test goes in `mcp.rs`'s `mod tests`, which has `open_user`. Without the fix, the inner guard's drop closes the group and the third write pushes a second entry — depth grows by 2, not 1.

```rust
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
```

Run: `cargo test -p app mcp::a_nested_undo_group` — expected FAIL: depth is `before + 2`.

- [ ] **Step 2: Make `group()` re-entrant**

In `undo.rs`, replace the `Group` struct, `group()` and its `Drop`:

```rust
/// See `group()`. `owner` is false for a nested guard, which must not close
/// the group the outer one opened.
pub struct Group<'a> {
    history: &'a Mutex<History>,
    owner: bool,
}

pub fn group(state: &AppState) -> Group<'_> {
    // Takes and RELEASES the history lock. It must not hold it: the next thing
    // the command does is call `edit_reshared`, which takes user → char →
    // history, and a held history lock would deadlock on the third.
    let mut h = state.history.lock().unwrap();
    let owner = h.group.is_none();
    if owner {
        h.group = Some(false);
    }
    drop(h);
    Group { history: &state.history, owner }
}

impl Drop for Group<'_> {
    fn drop(&mut self) {
        if !self.owner {
            return;
        }
        // `unwrap_or_else(into_inner)` rather than `unwrap()`: a panic inside a
        // Drop during an unwind aborts the process, and a poisoned History means
        // the app is already dead — closing the group should not turn that into
        // an abort.
        self.history.lock().unwrap_or_else(|e| e.into_inner()).group = None;
    }
}
```

Keep the existing doc comment above `group()` and add one sentence to it: "Re-entrant: a nested `group()` rides the open one and its drop is a no-op."

Run: `cargo test -p app mcp::a_nested_undo_group undo::` — expected all PASS.

- [ ] **Step 3: Failing MCP tests**

Add to `mod tests` in `mcp.rs`:

```rust
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
            { "op": "set_visible", "tab": 0, "column": "NO_SUCH_COLUMN", "visible": true }
        ]}))).unwrap_err();
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
```

If `tabs_edit_creates_...` fails because the one-tab fixture has no window mapping even after `overview_create_window_mapping`, read `ops::tests::overview_window_add_then_remove_roundtrips_the_projection` for the fixture shape it uses and reuse those bytes for this test.

- [ ] **Step 4: Run to verify they fail**

Run: `cargo test -p app mcp::`
Expected: six new failures with `unknown_tool`.

- [ ] **Step 5: Implement the batch runner and the four tools**

Schemas first. Because every op of one tool shares one flat item schema, write a helper:

```rust
/// The item schema of a batched edit tool: `op` from `ops`, plus the union
/// of every op's fields, all optional. Which op takes which field is in the
/// tool description — that text is what the model reads.
fn op_item(ops: &[&str], fields: Value) -> Value {
    let mut props = fields;
    props["op"] = json!({ "type": "string", "enum": ops });
    json!({
        "ops": {
            "type": "array",
            "items": { "type": "object", "additionalProperties": false, "properties": props, "required": ["op"] }
        }
    })
}
```

Tool defs (the descriptions list fields per op; keep them exactly this shape):

```rust
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
            description: "Edit windows and tabs, as a batch (one undo step; first failure rolls back). Ops: create {window, name, from_tab?} (clone from_tab's columns); rename {tab, name}; delete {tab}; reorder {window, order: [tab indices]}; move {tab, from_window, to_window, pos}; set_preset {tab, preset} (a preset name from overview_get); window_add {name, from_tab?}; window_remove {window}; create_window_mapping {} (for an account whose file has no window list yet). Returns the overview as overview_get does. Nothing reaches disk until save. Unsure: eve_guide overview.",
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
```

`call` arms:

```rust
            "overview_columns_edit" => self.batch(args, columns_op),
            "overview_tabs_edit" => self.batch(args, tabs_op),
            "overview_presets_edit" => self.batch(args, presets_op),
            "overview_appearance_edit" => self.batch(args, appearance_op),
```

The runner and the four op appliers. Each applier is a free function `fn(&AppState, &Args) -> Result<(), Value>` that reads its fields with `req`/`opt` (so a missing field is `missing_field`) and calls one `ops` function, discarding the projection (the batch re-projects once at the end):

```rust
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
    fn batch(&self, args: &Args, apply: fn(&AppState, &Args) -> Result<(), Value>) -> ToolResult {
        let ops_list: Vec<Args> = req(args, "ops")?;
        {
            let _group = undo::group(&self.state);
            for (i, op) in ops_list.iter().enumerate() {
                if let Err(mut e) = apply(&self.state, op) {
                    e["op_index"] = json!(i);
                    return Err(e);
                }
            }
        }
        let oc = ops::overview_columns(&self.state).map_err(fail)?;
        self.overview_with_names(oc)
    }
}
```

Check the exact `ops` signatures before wiring (`grep -n "pub fn tab_create\|pub fn preset_fork\|pub fn overview_copy_columns" app/src-tauri/src/ops.rs`): `tab_create(state, window_idx: usize, name: String, from_tab: Option<i64>)`, `preset_fork(state, tab_idx: i64, name: String, groups: Vec<i64>, filtered_states: Vec<i64>, always_shown_states: Vec<i64>)`, `overview_copy_columns(state, from_tab: i64, to_tabs: Vec<i64>, order: bool, visible: bool, widths: bool)`. `req` infers each type from the parameter.

One subtlety in `batch`: a `missing_field` / `bad_arguments` error from `req` inside an applier happens *before* any write for that op, and the writes before it are already in the group — `edit_reshared` only rolls back on a failing *write*. So on a parse error mid-batch, roll back explicitly: replace the `if let Err` body with

```rust
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
```

(Lock order user → char → history, as everywhere.) Add a test for it:

```rust
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
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p app mcp:: undo::`
Expected: all pass (29 in `mcp::`).

- [ ] **Step 7: Clippy, commit**

```bash
git add app/src-tauri/src/undo.rs app/src-tauri/src/mcp.rs
git commit -m "MCP: batched overview edit tools; undo groups re-enter

A batch holds one undo group, so edit_reshared's rollback makes it atomic
and one undo step. group() now rides an already-open group instead of
closing it on the inner drop — copy_columns, tab_delete and the window
ops open their own.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: Overview packs and the six probe tools

Spec §3.3 (packs), §3.4. Delivers the remaining overview tools and all of probes.

**Files:**
- Modify: `app/src-tauri/src/mcp.rs`

**Interfaces:**
- Consumes: `ops::{pack_preview, pack_import, pack_export, probe_formations, set_probe_formation, remove_probe_formation, reorder_probe_formations, probe_yaml, probe_parse_yaml, add_probe_formations}`, `settings_model::FormationSpec`.
- Produces: tools `overview_pack_preview`, `overview_pack_import`, `overview_pack_export`, `probes_get`, `probes_set`, `probes_remove`, `probes_reorder`, `probes_add_yaml`, `probes_export_yaml`.

- [ ] **Step 1: Failing tests**

```rust
    fn empty_ui_bytes() -> Vec<u8> {
        encode(&BmValue::Dict(vec![(b("ui"), BmValue::Dict(vec![]))])).unwrap()
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
        let e = s.call("probes_set", &args(json!({ "name": "nine", "probes": [[0.0,0.0,0.0]; 9], "ranges": [1.0; 9] }))).unwrap_err();
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
        assert!(preview.to_string().contains("Frigs"), "{preview}");
        s.call("overview_pack_import", &args(json!({ "path": out.to_string_lossy() }))).unwrap();
    }
```

If `overview_pack_export` refuses the one-tab fixture ("rejects an account with no overview settings" exists in `ops` tests), build the fixture the way `ops::tests::pack_export_*`'s positive test does and reuse it here.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p app mcp::` — expected five new `unknown_tool` failures.

- [ ] **Step 3: Implement**

Tool defs:

```rust
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
```

`call` arms:

```rust
            "overview_pack_preview" => ok(ops::pack_preview(&req::<String>(args, "path")?).map_err(fail)?),
            "overview_pack_import" => ok(ops::pack_import(&self.state, &req::<String>(args, "path")?).map_err(fail)?),
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
```

Check the exact signatures of `set_probe_formation`, `remove_probe_formation`, `reorder_probe_formations` in `ops.rs` (lines ~1075–1120) and match them; `set_probe_formation(state, id: Option<i64>, name: &str, probes: Vec<[f64; 3]>, ranges: Vec<f64>)` is the shape seen at spec time. `FormationSpec`'s fields are `pub` (`probe_pack.rs:28`).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p app mcp::` — expected all pass (34).

- [ ] **Step 5: Update the smoke test to the final tool list**

In `tests/mcp_stdio.rs`, replace the `is_array` assert with:

```rust
    let mut names: Vec<&str> =
        list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "builtin_presets", "eve_guide", "groups_search", "list_backups", "list_characters", "open",
            "overview_appearance_edit", "overview_columns_edit", "overview_get", "overview_pack_export",
            "overview_pack_import", "overview_pack_preview", "overview_presets_edit", "overview_tabs_edit",
            "probes_add_yaml", "probes_export_yaml", "probes_get", "probes_remove", "probes_reorder", "probes_set",
            "restore_backup", "save", "status", "undo",
        ]
    );
```

Run: `cargo test --test mcp_stdio` (from `app/src-tauri`) — expected PASS with 24 names.

- [ ] **Step 6: Clippy, commit**

```bash
git add app/src-tauri/src/mcp.rs app/src-tauri/tests/mcp_stdio.rs
git commit -m "MCP: overview packs and the probe formation tools; 24 tools

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 9: `mcp_setup.rs` — exe path and Claude Desktop registration

Spec §4, §6, §8. Delivers the two Tauri commands the sheet calls.

**Files:**
- Create: `app/src-tauri/src/mcp_setup.rs`
- Modify: `app/src-tauri/src/lib.rs` (module, command registration)

**Interfaces:**
- Produces: `pub struct McpSetup { pub command: String, pub args: Vec<String>, pub snippet: String, pub claude_desktop: Option<ClaudeDesktop> }`, `pub struct ClaudeDesktop { pub config_path: String, pub registered: bool }`, `pub fn exe_path() -> PathBuf`, `pub fn claude_desktop_config() -> Option<PathBuf>`, `pub fn info_at(exe: &Path, config: Option<&Path>) -> McpSetup`, `pub fn set_claude_desktop_at(exe: &Path, config: &Path, on: bool) -> Result<(), ErrDto>`, commands `mcp_setup_info`, `mcp_set_claude_desktop`.

- [ ] **Step 1: Failing tests**

Create `app/src-tauri/src/mcp_setup.rs` with the tests first:

```rust
//! The "AI access" sheet's backend: where this exe is, the config snippet
//! any MCP client takes, and a Register/Unregister for Claude Desktop's
//! config file. Spec §6.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

use crate::ops::ErrDto;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mcp-setup-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_snippet_is_the_mcp_servers_shape_with_the_exe_and_flag() {
        let info = info_at(Path::new("C:/Program Files/EVE Settings Editor/eve-settings-editor.exe"), None);
        let v: Value = serde_json::from_str(&info.snippet).unwrap();
        assert_eq!(v["mcpServers"]["eve-settings-editor"]["command"], "C:/Program Files/EVE Settings Editor/eve-settings-editor.exe");
        assert_eq!(v["mcpServers"]["eve-settings-editor"]["args"], json!(["--mcp"]));
        assert_eq!(info.args, vec!["--mcp"]);
        assert!(info.claude_desktop.is_none(), "no Claude dir → no Claude Desktop row");
    }

    #[test]
    fn register_merges_beside_other_servers_and_unregister_leaves_them() {
        let d = dir("merge");
        let cfg = d.join("claude_desktop_config.json");
        fs::write(&cfg, r#"{"mcpServers":{"other":{"command":"x"}},"theme":"dark"}"#).unwrap();
        let exe = Path::new("/Applications/EVE Settings Editor.app/Contents/MacOS/eve-settings-editor");

        set_claude_desktop_at(exe, &cfg, true).unwrap();
        let v: Value = serde_json::from_slice(&fs::read(&cfg).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["other"]["command"], "x");
        assert_eq!(v["theme"], "dark");
        assert_eq!(v["mcpServers"]["eve-settings-editor"]["args"], json!(["--mcp"]));
        assert!(info_at(exe, Some(&cfg)).claude_desktop.unwrap().registered);

        set_claude_desktop_at(exe, &cfg, false).unwrap();
        let v: Value = serde_json::from_slice(&fs::read(&cfg).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["other"]["command"], "x");
        assert!(v["mcpServers"].get("eve-settings-editor").is_none());
        assert!(!info_at(exe, Some(&cfg)).claude_desktop.unwrap().registered);
    }

    #[test]
    fn register_creates_the_file_when_the_dir_exists_but_the_file_does_not() {
        let d = dir("create");
        let cfg = d.join("claude_desktop_config.json");
        set_claude_desktop_at(Path::new("/usr/bin/eve-settings-editor"), &cfg, true).unwrap();
        let v: Value = serde_json::from_slice(&fs::read(&cfg).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["eve-settings-editor"]["command"], "/usr/bin/eve-settings-editor");
    }

    #[test]
    fn a_missing_dir_is_not_installed_and_a_non_json_file_is_never_overwritten() {
        let d = dir("guards");
        let missing = d.join("nope").join("claude_desktop_config.json");
        assert_eq!(set_claude_desktop_at(Path::new("x"), &missing, true).unwrap_err().code, "not_installed");
        let cfg = d.join("claude_desktop_config.json");
        fs::write(&cfg, "not json {").unwrap();
        assert_eq!(set_claude_desktop_at(Path::new("x"), &cfg, true).unwrap_err().code, "parse");
        assert_eq!(fs::read_to_string(&cfg).unwrap(), "not json {");
    }

    #[test]
    fn exe_path_prefers_appimage() {
        // Serialised through the env var; other tests do not touch it.
        std::env::set_var("APPIMAGE", "/home/me/EVE.AppImage");
        assert_eq!(exe_path(), PathBuf::from("/home/me/EVE.AppImage"));
        std::env::remove_var("APPIMAGE");
        assert_eq!(exe_path(), std::env::current_exe().unwrap());
    }
}
```

Add `mod mcp_setup;` to `lib.rs`. Run: `cargo test -p app mcp_setup::` — expected compile errors.

- [ ] **Step 2: Implement**

Above the tests in `mcp_setup.rs`:

```rust
const SERVER_KEY: &str = "eve-settings-editor";

#[derive(Debug, Serialize)]
pub struct ClaudeDesktop {
    pub config_path: String,
    pub registered: bool,
}

#[derive(Debug, Serialize)]
pub struct McpSetup {
    pub command: String,
    pub args: Vec<String>,
    /// The `mcpServers` JSON any client takes, pretty-printed.
    pub snippet: String,
    /// `None` when Claude Desktop's config directory does not exist.
    pub claude_desktop: Option<ClaudeDesktop>,
}

/// This executable, as a client config must name it. An AppImage's
/// `current_exe()` is the per-launch mount under /tmp; the runtime sets
/// `APPIMAGE` to the image's own, stable path.
pub fn exe_path() -> PathBuf {
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_exe().unwrap_or_default())
}

/// `dirs::config_dir()` is %APPDATA% on Windows, ~/Library/Application Support
/// on macOS and ~/.config on Linux — Claude Desktop's config lives under
/// `Claude/` in each, so this is one expression with no `cfg`. `Some` only
/// when the `Claude` directory exists ("installed").
pub fn claude_desktop_config() -> Option<PathBuf> {
    let dir = dirs::config_dir()?.join("Claude");
    dir.is_dir().then(|| dir.join("claude_desktop_config.json"))
}

fn entry(exe: &Path) -> Value {
    json!({ "command": exe.to_string_lossy(), "args": ["--mcp"] })
}

fn read_config(config: &Path) -> Result<Value, ErrDto> {
    match std::fs::read(config) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| ErrDto::new("parse", format!("{} is not JSON ({e}); not touching it", config.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(ErrDto::new("io", e.to_string())),
    }
}

fn registered(config: &Path) -> bool {
    read_config(config).ok().and_then(|v| v.get("mcpServers")?.get(SERVER_KEY).cloned()).is_some()
}

pub fn info_at(exe: &Path, config: Option<&Path>) -> McpSetup {
    let snippet = serde_json::to_string_pretty(&json!({ "mcpServers": { SERVER_KEY: entry(exe) } })).unwrap_or_default();
    McpSetup {
        command: exe.to_string_lossy().into_owned(),
        args: vec!["--mcp".into()],
        snippet,
        claude_desktop: config.map(|c| ClaudeDesktop { config_path: c.to_string_lossy().into_owned(), registered: registered(c) }),
    }
}

/// Merge our entry into (or remove it from) `config`, leaving every other
/// key as it was. Never overwrites a file that does not parse.
pub fn set_claude_desktop_at(exe: &Path, config: &Path, on: bool) -> Result<(), ErrDto> {
    let Some(dir) = config.parent().filter(|d| d.is_dir()) else {
        return Err(ErrDto::new("not_installed", "Claude Desktop's config directory does not exist"));
    };
    let mut root = read_config(config)?;
    if !root.is_object() {
        return Err(ErrDto::new("parse", format!("{} is not a JSON object; not touching it", config.display())));
    }
    let servers = root
        .as_object_mut()
        .expect("checked")
        .entry("mcpServers")
        .or_insert_with(|| json!({}));
    let Some(servers) = servers.as_object_mut() else {
        return Err(ErrDto::new("parse", "`mcpServers` is not an object; not touching it"));
    };
    if on {
        servers.insert(SERVER_KEY.into(), entry(exe));
    } else {
        servers.remove(SERVER_KEY);
    }
    let text = serde_json::to_string_pretty(&root).map_err(|e| ErrDto::new("io", e.to_string()))?;
    let _ = dir; // existence checked above; the write creates the file
    std::fs::write(config, text).map_err(|e| ErrDto::new("io", e.to_string()))
}

#[tauri::command]
pub fn mcp_setup_info() -> McpSetup {
    info_at(&exe_path(), claude_desktop_config().as_deref())
}

#[tauri::command]
pub fn mcp_set_claude_desktop(on: bool) -> Result<McpSetup, ErrDto> {
    let config = claude_desktop_config()
        .ok_or_else(|| ErrDto::new("not_installed", "Claude Desktop's config directory does not exist"))?;
    set_claude_desktop_at(&exe_path(), &config, on)?;
    Ok(info_at(&exe_path(), Some(&config)))
}
```

Register both in `lib.rs`'s `generate_handler!` list (after `preferences, set_preferences,`): `mcp_setup::mcp_setup_info, mcp_setup::mcp_set_claude_desktop,`. `ErrDto` derives `Serialize`, so the command's error reaches the frontend as `{code, message}` like every other.

- [ ] **Step 3: Run the tests**

Run: `cargo test -p app mcp_setup::` — expected 5 passed.

- [ ] **Step 4: Clippy, commit**

```bash
git add app/src-tauri/src/mcp_setup.rs app/src-tauri/src/lib.rs
git commit -m "MCP: setup info and Claude Desktop register/unregister commands

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 10: The "AI access" sheet

Spec §6. Delivers the user-facing setup.

**Files:**
- Modify: `app/src/lib/api.ts`
- Create: `app/src/lib/AiAccessPanel.svelte`, `app/src/lib/AiAccessPanel.spec.ts`
- Modify: `app/src/lib/commands.ts`, `app/src/lib/AppMenu.svelte` (LAYOUT), `app/src/lib/AppMenu.spec.ts` (ctx mock), `app/src/routes/+page.svelte`

**Interfaces:**
- Produces: `api.mcpSetupInfo(): Promise<McpSetup>`, `api.mcpSetClaudeDesktop(on: boolean): Promise<McpSetup>`, `interface McpSetup { command: string; args: string[]; snippet: string; claude_desktop: { config_path: string; registered: boolean } | null }`, `Ctx.showAiAccess: () => void`, command id `help.aiAccess`.

- [ ] **Step 1: api.ts**

In `app/src/lib/api.ts`, add the type near the other interfaces and the two calls at the end of the `api` object:

```ts
export interface McpSetup {
  command: string;
  args: string[];
  /** The `mcpServers` JSON any MCP client takes, pretty-printed. */
  snippet: string;
  /** null when Claude Desktop's config directory does not exist. */
  claude_desktop: { config_path: string; registered: boolean } | null;
}
```

```ts
  mcpSetupInfo: () => invoke<McpSetup>("mcp_setup_info"),
  mcpSetClaudeDesktop: (on: boolean) => invoke<McpSetup>("mcp_set_claude_desktop", { on }),
```

- [ ] **Step 2: The failing component test**

Create `app/src/lib/AiAccessPanel.spec.ts`:

```ts
// Component test: vitest + jsdom.
import { describe, expect, test, vi } from "vitest";
import { render, fireEvent, screen } from "@testing-library/svelte";
import AiAccessPanel from "$lib/AiAccessPanel.svelte";
import { api, type McpSetup } from "$lib/api";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import "$lib/test/setup";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({ writeText: vi.fn(() => Promise.resolve()) }));

const base: McpSetup = {
  command: "C:\\Program Files\\EVE Settings Editor\\eve-settings-editor.exe",
  args: ["--mcp"],
  snippet: '{\n  "mcpServers": {\n    "eve-settings-editor": {}\n  }\n}',
  claude_desktop: null,
};

describe("AiAccessPanel", () => {
  test("shows the snippet and copies it", async () => {
    vi.spyOn(api, "mcpSetupInfo").mockResolvedValue(base);
    render(AiAccessPanel, { onClose: () => {} });

    expect(await screen.findByText(/"eve-settings-editor"/)).toBeTruthy();
    await fireEvent.click(screen.getByText("Copy"));
    expect(vi.mocked(writeText)).toHaveBeenCalledWith(base.snippet);
  });

  test("without a Claude Desktop config dir there is no Register row", async () => {
    vi.spyOn(api, "mcpSetupInfo").mockResolvedValue(base);
    render(AiAccessPanel, { onClose: () => {} });
    await screen.findByText(/"eve-settings-editor"/);
    expect(screen.queryByText("Register")).toBeNull();
    expect(screen.getByText(/Claude Desktop is not installed/)).toBeTruthy();
  });

  test("Register calls the command and re-renders from its result", async () => {
    const off = { ...base, claude_desktop: { config_path: "C:\\x\\claude_desktop_config.json", registered: false } };
    const on = { ...off, claude_desktop: { ...off.claude_desktop!, registered: true } };
    vi.spyOn(api, "mcpSetupInfo").mockResolvedValue(off);
    const set = vi.spyOn(api, "mcpSetClaudeDesktop").mockResolvedValue(on);
    render(AiAccessPanel, { onClose: () => {} });

    await fireEvent.click(await screen.findByText("Register"));
    expect(set).toHaveBeenCalledWith(true);
    expect(await screen.findByText("Unregister")).toBeTruthy();
    expect(screen.getByText(/Registered/)).toBeTruthy();
  });

  test("a failed register shows the error and keeps the row", async () => {
    const off = { ...base, claude_desktop: { config_path: "C:\\x\\claude_desktop_config.json", registered: false } };
    vi.spyOn(api, "mcpSetupInfo").mockResolvedValue(off);
    vi.spyOn(api, "mcpSetClaudeDesktop").mockRejectedValue({ code: "parse", message: "not JSON" });
    render(AiAccessPanel, { onClose: () => {} });

    await fireEvent.click(await screen.findByText("Register"));
    expect(await screen.findByText(/not JSON/)).toBeTruthy();
    expect(screen.getByText("Register")).toBeTruthy();
  });

  test("Close dismisses it", async () => {
    vi.spyOn(api, "mcpSetupInfo").mockResolvedValue(base);
    const onClose = vi.fn();
    render(AiAccessPanel, { onClose });
    await screen.findByText(/"eve-settings-editor"/);
    await fireEvent.click(screen.getByText("Close"));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
```

Run (from `app/`): `npx vitest run src/lib/AiAccessPanel.spec.ts` — expected FAIL (module not found).

- [ ] **Step 3: The component**

Create `app/src/lib/AiAccessPanel.svelte`, modelled on `AboutPanel.svelte` (same `Sheet`, `Button`, `onClose` prop):

```svelte
<script lang="ts">
  // The "AI access" sheet: hands the user a working MCP config with zero
  // typing. Register/Unregister writes Claude Desktop's file; every other
  // client takes the snippet. Spec §6 of the MCP design.
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { api, errMessage, type McpSetup } from "./api";
  import Button from "./ui/Button.svelte";
  import Sheet from "./ui/Sheet.svelte";

  let { onClose }: { onClose: () => void } = $props();

  let info = $state<McpSetup | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let copied = $state(false);

  api
    .mcpSetupInfo()
    .then((i) => (info = i))
    .catch((e) => (error = errMessage(e)));

  async function setClaude(on: boolean) {
    busy = true;
    error = null;
    try {
      info = await api.mcpSetClaudeDesktop(on);
    } catch (e) {
      error = errMessage(e);
    } finally {
      busy = false;
    }
  }

  async function copy() {
    if (!info) return;
    await writeText(info.snippet).catch(() => {});
    copied = true;
  }
</script>

<Sheet title="AI access" width="min(560px, 92vw)" onclose={onClose} data-testid="ai-access-backdrop">
  <p>
    Let an AI assistant edit your overview and probe formations through this app. Every change goes
    through the same backups and checks as editing here; nothing is written until the assistant saves.
  </p>

  {#if info}
    <h3>Claude Desktop</h3>
    {#if info.claude_desktop}
      <p class="row">
        <span>{info.claude_desktop.registered ? "Registered" : "Not registered"}</span>
        <Button disabled={busy} onclick={() => void setClaude(!info!.claude_desktop!.registered)}>
          {info.claude_desktop.registered ? "Unregister" : "Register"}
        </Button>
      </p>
      <p class="meta">Restart Claude Desktop to pick this up.</p>
    {:else}
      <p class="meta">Claude Desktop is not installed on this machine — use the snippet below in another client.</p>
    {/if}

    <h3>Any other MCP client</h3>
    <pre>{info.snippet}</pre>
    <p class="row">
      <Button variant="ghost" onclick={() => void copy()}>Copy</Button>
      {#if copied}<span class="meta">Copied</span>{/if}
    </p>
    <p class="meta">
      Paste it into the client's MCP config. VS Code uses a <code>servers</code> key and Codex a
      <code>[mcp_servers.…]</code> TOML table — same command and args.
    </p>
  {/if}

  {#if error}<p class="error">{error}</p>{/if}
</Sheet>

<style>
  h3 { margin: 1rem 0 0.25rem; font-size: 0.95rem; }
  pre { overflow-x: auto; padding: 0.5rem; background: var(--surface-2, rgba(128, 128, 128, 0.12)); border-radius: 6px; font-size: 0.8rem; }
  .row { display: flex; align-items: center; gap: 0.75rem; }
  .meta { opacity: 0.75; font-size: 0.85rem; }
  .error { color: var(--danger, #c33); }
</style>
```

Match `AboutPanel.svelte`'s actual `Sheet`/`Button` usage (prop names, the `Close` button the Sheet renders, CSS variable names) — copy what it does rather than what is sketched above where they differ.

- [ ] **Step 4: Register the command, the menu row, the ctx, the mount**

`commands.ts` — add to `Ctx`: `showAiAccess: () => void;` and, in the Help group after `help.shortcuts`:

```ts
  {
    id: "help.aiAccess",
    label: "AI access…",
    group: "Help",
    keywords: "mcp claude assistant ai",
    enabled: () => true,
    homes: [{ at: "app-menu" }],
    run: (ctx) => ctx.showAiAccess(),
  },
```

`AppMenu.svelte` — in `LAYOUT`, insert `"help.aiAccess",` before `"file.about"`.

`AppMenu.spec.ts` — add `showAiAccess: noop,` to `baseCtx`.

`+page.svelte` — next to `aboutOpen`: `let aiAccessOpen = $state(false);`; in the ctx object: `showAiAccess: () => (aiAccessOpen = true),`; import `AiAccessPanel from "$lib/AiAccessPanel.svelte"`; next to the About mount: `{#if aiAccessOpen}<AiAccessPanel onClose={() => (aiAccessOpen = false)} />{/if}`.

If `commands.spec.ts` or `keymap.spec.ts` enumerate command ids or ctx keys, extend them the way they cover `help.shortcuts`.

- [ ] **Step 5: Run the frontend checks**

From `app/`: `npm run check` then `npm test`. Expected: both exit 0. Fix type errors `check` names (the `info!` non-null assertions may need a local `const cd = info.claude_desktop` instead).

- [ ] **Step 6: See it in the app**

Invoke the `starting-the-app` skill; open the app menu → "AI access…". Expected: the sheet renders with your exe path in the snippet; on this Windows machine with Claude Desktop installed, the Register row shows. Do not click Register yet (Task 11 does, as the DoD run).

- [ ] **Step 7: Commit**

```bash
git add app/src/lib/api.ts app/src/lib/AiAccessPanel.svelte app/src/lib/AiAccessPanel.spec.ts app/src/lib/commands.ts app/src/lib/AppMenu.svelte app/src/lib/AppMenu.spec.ts app/src/routes/+page.svelte
git commit -m "AI access sheet: Claude Desktop register, snippet for any client

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 11: Docs, spec reconciliation, full verification, and the end-to-end run

Spec §9 (README, CHANGELOG), §10 items 2–5.

**Files:**
- Modify: `README.md`, `CHANGELOG.md`, `docs/superpowers/specs/2026-09-19-mcp-server-design.md` (§3.2 `restore_backup` row; status line)

- [ ] **Step 1: README**

Add a section after "Features" (before "Install"):

```markdown
## AI access

The app can be driven by an AI assistant through the
[Model Context Protocol](https://modelcontextprotocol.io): the same executable
started with `--mcp` is an MCP server exposing the overview and probe-formation
editors. Every edit goes through the same backup → verify → atomic-write chain as
the app itself, and nothing is written until the assistant saves.

Open **AI access…** from the app menu. With Claude Desktop installed, **Register**
adds the server to its config (restart Claude Desktop afterwards). For any other
MCP client, copy the snippet shown there into the client's config — it is the
standard `mcpServers` shape:

```json
{ "mcpServers": { "eve-settings-editor": { "command": "<path to the app>", "args": ["--mcp"] } } }
```

Edit a character only while it is logged out; the EVE client rewrites its
settings on logout.
```

- [ ] **Step 2: CHANGELOG**

Under `## [Unreleased]`:

```markdown
### Added
- **AI access.** An MCP server built into the app (`--mcp`) lets an AI assistant edit your overview and probe formations, with the same backups and checks as editing by hand. Register it with Claude Desktop from the new AI access… menu entry, or copy the config snippet for any other MCP client.
```

- [ ] **Step 3: Spec reconciliation**

In the spec, change the `restore_backup` row's "Returns" to: "`{path, status}`; the pre-restore backup is `list_backups`' newest entry (`ops::restore_backup` re-opens the file and does not return that path)." Change the status line to `Status: designed 2026-09-19, planned 2026-09-19 (docs/superpowers/plans/2026-09-19-mcp-server.md).`

- [ ] **Step 4: Full verification — by exit code**

From the repo root:

```powershell
cargo clippy --workspace --all-targets -- -D warnings; echo "clippy exit $LASTEXITCODE"
cargo test --workspace; echo "cargo test exit $LASTEXITCODE"
cd app; npm run check; echo "check exit $LASTEXITCODE"; npm test; echo "npm test exit $LASTEXITCODE"; cd ..
```

Expected: four zeros. Anything else is a stop.

- [ ] **Step 5: End-to-end on this machine (spec §10 item 3)**

1. Build the release: from `app/`, `npm run tauri build` (or reuse Task 1's release exe if nothing native changed since — it has; rebuild).
2. Run the built exe, open AI access…, click **Register**. Confirm `%APPDATA%\Claude\claude_desktop_config.json` gained the `eve-settings-editor` entry and kept everything else.
3. Restart Claude Desktop. In a new chat: "Which EVE characters do I have?" → it calls `list_characters`. Then, naming a logged-out character and a tab: "Hide the Corporation column on the Default tab and save." → `open`, `overview_get`, `overview_columns_edit`, `save`; the reply cites a backup path.
4. Confirm: the backup file exists next to the settings file; the GUI opens that character and shows the column hidden; a save from the GUI afterwards is not blocked (no conflict dialog).
5. Optional Linux/macOS: no machine → note it as not run.

Record what happened, verbatim, in the final commit message.

- [ ] **Step 6: Commit and hand off**

```bash
git add README.md CHANGELOG.md docs/superpowers/specs/2026-09-19-mcp-server-design.md
git commit -m "MCP: README and changelog; spec reconciled with the build

End-to-end on Windows with Claude Desktop: <what happened>.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

Then invoke `superpowers:finishing-a-development-branch` — the branch is `feat/mcp-server`, the PR target is `master`.

---

## Self-review against the spec

- §2.1 boot → Task 1. §2.2 state → Tasks 2, 7. §2.3 app dir → Task 1. §2.4 deps → Task 1.
- §3.1 conventions → Task 2 (helpers, invariants test), Task 7 (batch). §3.2 → Tasks 2 (`status`), 4, 5. §3.3 → Tasks 6, 7, 8. §3.4 → Task 8. §3.5/§3.6 → Task 3 (primer, `eve_guide`, `instructions`), Task 6 (`builtin_presets`), descriptions throughout.
- §4 portability → Task 9 (`$APPIMAGE`, `dirs::config_dir`), Task 1 (`app_dir_base`). §5 → Task 2's invariants test, Task 10's copy. §6 → Tasks 9, 10. §7 → Task 4 (conflict, force, save path). §8 → each task's tests; the stdio smoke test in Tasks 1, 3, 8. §9 file list → all files appear above. §10 → Task 1 step 10, Task 11. §11 deferred → untouched.
- Deviation recorded: `restore_backup` returns `{path, status}` (Task 4 note, Task 11 step 3).
- Type consistency: `Args`/`ToolResult`/`req`/`opt`/`fail`/`err`/`ok` defined in Task 2 and used unchanged after; `overview_with_names` defined in Task 6, used by Task 7's `batch`; `open_user`/`args`/`overview_user_bytes` test helpers defined in Task 4, used by 6–8; `McpSetup` field names match between `mcp_setup.rs` and `api.ts`.
