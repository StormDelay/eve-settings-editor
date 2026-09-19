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
