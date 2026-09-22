//! Live mode (spec §4): the pieces that join the MCP server to the running
//! window. This file holds what the window hears about (`Change`), the
//! same-user endpoint, the window's listener and `--mcp`'s client relay.

use std::path::PathBuf;
use std::sync::Arc;

use rmcp::ServiceExt;

use crate::undo::UndoOutcome;

/// What a tool call changed, for the window (spec §4.3).
#[derive(Debug)]
pub enum Change {
    /// The window's document moved — an edit, a save, an undo, a restore —
    /// with the projection the frontend lands through `landUndo()`. Boxed:
    /// `UndoOutcome` carries two full document trees, and clippy flags the
    /// bare enum as heavily skewed next to `Wrote`'s `Vec<PathBuf>`.
    Edited { tool: String, outcome: Box<UndoOutcome> },
    /// Settings files written on disk behind the open documents (a batch
    /// copy, a private workspace's restore): the frontend re-reads a clean
    /// slot and flags a dirty one, as it does after a batch.
    Wrote(Vec<PathBuf>),
}

/// The window's listener for changes; `None` headless.
pub type OnChange = Arc<dyn Fn(Change) + Send + Sync>;

/// The same-user endpoint the window listens on. `EVE_MCP_ENDPOINT`
/// overrides it — tests and debugging.
///
/// Windows: a named pipe. The default DACL lets other local users open it
/// for reading, so it is "writable only by this user", which is what the
/// protocol needs (nothing is said without a request). Unix: a socket in the
/// app dir, `0600`.
pub fn endpoint() -> String {
    if let Ok(e) = std::env::var("EVE_MCP_ENDPOINT") {
        return e;
    }
    #[cfg(windows)]
    {
        format!(r"\\.\pipe\eve-settings-editor-{}", std::env::var("USERNAME").unwrap_or_else(|_| "user".into()))
    }
    #[cfg(not(windows))]
    {
        crate::app_dir_base().unwrap_or_else(std::env::temp_dir).join("mcp.sock").to_string_lossy().into_owned()
    }
}

/// Whether a window is listening on `endpoint` — without connecting, which
/// would cost the window an accept. Windows enumerates `\\.\pipe\`; Unix
/// checks for the socket file (a stale one from a crash reads as listening
/// until the next window start replaces it; `connect` then fails and `--mcp`
/// serves headless, so the cost is one wrong `window_available`).
pub fn window_listening(endpoint: &str) -> bool {
    #[cfg(windows)]
    {
        let Some(name) = endpoint.strip_prefix(r"\\.\pipe\") else { return false };
        std::fs::read_dir(r"\\.\pipe\")
            .map(|it| it.flatten().any(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name)))
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        std::path::Path::new(endpoint).exists()
    }
}

/// One accepted connection, as the OS gives it. Both halves of the protocol
/// go over it; rmcp serves anything `AsyncRead + AsyncWrite`.
#[cfg(windows)]
pub type Stream = tokio::net::windows::named_pipe::NamedPipeServer;
#[cfg(not(windows))]
pub type Stream = tokio::net::UnixStream;

/// Listen on `endpoint` forever, handing each connection to `on_conn`
/// (which spawns its server and returns at once). Returns only on a bind
/// or accept error — a second window instance, typically.
#[cfg(windows)]
pub async fn listen(endpoint: &str, on_conn: impl Fn(Stream)) -> std::io::Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;
    let mut server = ServerOptions::new().first_pipe_instance(true).create(endpoint)?;
    loop {
        server.connect().await?;
        let connected = server;
        // Hand the connected client off before creating the next instance —
        // tokio's pattern is to create it first so a client that arrives
        // meanwhile finds one to open, but doing that ahead of `on_conn`
        // means a failing `create` (the pipe instance limit, say) would
        // return before this client is ever handed off, dropping a
        // connection this loop already accepted.
        on_conn(connected);
        server = ServerOptions::new().create(endpoint)?;
    }
}

#[cfg(not(windows))]
pub async fn listen(endpoint: &str, on_conn: impl Fn(Stream)) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    // A fresh install has no app dir yet — `accounts.rs`/`groups.rs` only
    // create it lazily on first write — so `bind` would fail ENOENT and live
    // mode would be silently off.
    if let Some(parent) = std::path::Path::new(endpoint).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::remove_file(endpoint);
    let listener = tokio::net::UnixListener::bind(endpoint)?;
    std::fs::set_permissions(endpoint, std::fs::Permissions::from_mode(0o600))?;
    loop {
        let (stream, _) = listener.accept().await?;
        on_conn(stream);
    }
}

/// The client half: the stream to the window, or `None` when no window
/// listens (or it cannot be reached), in which case `--mcp` serves headless.
#[cfg(windows)]
pub async fn connect(endpoint: &str) -> Option<tokio::net::windows::named_pipe::NamedPipeClient> {
    use tokio::net::windows::named_pipe::ClientOptions;
    // ERROR_PIPE_BUSY (231): the window is between `connect` and the next
    // instance. Brief retries, then give up rather than hang the client.
    for _ in 0..20 {
        match ClientOptions::new().open(endpoint) {
            Ok(c) => return Some(c),
            Err(e) if e.raw_os_error() == Some(231) => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
            Err(_) => return None,
        }
    }
    None
}

#[cfg(not(windows))]
pub async fn connect(endpoint: &str) -> Option<tokio::net::UnixStream> {
    tokio::net::UnixStream::connect(endpoint).await.ok()
}

/// `--mcp` with a window up: copy stdin to the window and the window to
/// stdout, exiting as soon as either side ends. Not one byte of MCP is
/// parsed here.
///
/// Not `copy_bidirectional`: its notion of "done" needs a half-close that
/// means something to the peer, and a Windows named pipe's `shutdown` is a
/// no-op (tokio's `poll_shutdown` for both `NamedPipeClient` and
/// `NamedPipeServer` is literally `poll_flush` — no OS call ever closes the
/// pipe), so when stdin hits EOF the window never learns the client went
/// away and the other direction blocks forever. Racing two independent
/// copies and returning on the first to finish sidesteps that: whichever
/// end goes first (the AI client closing stdin, or the window dropping the
/// connection) tears down the whole relay and the process exits.
pub async fn relay<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(stream: S) -> std::io::Result<()> {
    let (mut from_window, mut to_window) = tokio::io::split(stream);
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    tokio::select! {
        r = tokio::io::copy(&mut stdin, &mut to_window) => r,
        r = tokio::io::copy(&mut from_window, &mut stdout) => r,
    }
    .map(|_| ())
}

/// The window's side of live mode (spec §4.2): listen, and serve one
/// `EveMcp` per connection over the window's state, forwarding `Change`s
/// and the connection count to the frontend as Tauri events.
pub async fn serve_in_window(app: tauri::AppHandle, window: crate::ops::AppState) {
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tauri::Emitter;

    let dir = crate::app_dir(&app);
    let workspaces: Arc<std::sync::Mutex<std::collections::HashMap<PathBuf, crate::ops::AppState>>> = Arc::default();
    let connections = Arc::new(AtomicUsize::new(0));
    let endpoint = endpoint();
    let on_conn = move |stream: Stream| {
        let (app, window, workspaces, connections, dir) =
            (app.clone(), window.clone(), Arc::clone(&workspaces), Arc::clone(&connections), dir.clone());
        tauri::async_runtime::spawn(async move {
            let n = connections.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = app.emit("ai-connected", json!({ "connections": n }));
            let emitter = app.clone();
            let on_change: OnChange = Arc::new(move |change| {
                let _ = match change {
                    Change::Edited { tool, outcome } => emitter.emit("ai-edit", json!({ "tool": tool, "outcome": outcome })),
                    Change::Wrote(paths) => emitter.emit("ai-wrote", json!({ "paths": paths })),
                };
            });
            let server = crate::mcp::EveMcp::in_window(dir, settings_model::default_roots(), crate::prefs::path_base(), window, workspaces, on_change);
            if let Ok(running) = server.serve(stream).await {
                let _ = running.waiting().await;
            }
            let n = connections.fetch_sub(1, Ordering::SeqCst) - 1;
            let _ = app.emit("ai-connected", json!({ "connections": n }));
        });
    };
    // A bind failure (a second window instance) means this window runs
    // without live mode; `--mcp` then serves headless for it. Not an error
    // the user can act on, so not surfaced.
    let _ = listen(&endpoint, on_conn).await;
}
