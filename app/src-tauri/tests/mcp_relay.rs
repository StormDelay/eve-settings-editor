//! Live mode end to end (spec §4.1): this test process plays the window —
//! it listens on a private endpoint and serves MCP over it — then spawns the
//! real exe with `--mcp` and `EVE_MCP_ENDPOINT` pointing at that endpoint.
//! The exe must relay: the `initialize` reply comes back through it, and
//! `status` says `mode: live`. Without a window (Task 5's other half) the
//! same exe serves headless, which `mcp_stdio.rs` already proves.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use app_lib::mcp::EveMcp;
use app_lib::mcp_live;
use app_lib::AppState;
use rmcp::ServiceExt;

fn exe() -> String {
    std::env::var("MCP_EXE").unwrap_or_else(|_| env!("CARGO_BIN_EXE_app").to_string())
}

fn request(stdin: &mut impl Write, out: &mut impl BufRead, body: &str) -> serde_json::Value {
    writeln!(stdin, "{body}").unwrap();
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON-RPC: {e}: {line}"))
}

fn private_endpoint() -> String {
    let tag = format!("eve-mcp-relay-test-{}", std::process::id());
    #[cfg(windows)]
    {
        format!(r"\\.\pipe\{tag}")
    }
    #[cfg(not(windows))]
    {
        std::env::temp_dir().join(format!("{tag}.sock")).to_string_lossy().into_owned()
    }
}

#[test]
fn the_exe_relays_to_a_listening_window() {
    let endpoint = private_endpoint();
    let listen_on = endpoint.clone();
    // The "window": a current-thread runtime on its own thread, one in-window
    // server per connection over one shared state, exactly as `serve_in_window`.
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async move {
            let window = AppState::new();
            let workspaces = Arc::default();
            let on_conn = move |stream: mcp_live::Stream| {
                let server = EveMcp::in_window(
                    std::env::temp_dir().join("eve-mcp-relay-test"),
                    vec![],
                    None,
                    window.clone(),
                    Arc::clone(&workspaces),
                    Arc::new(|_| {}),
                );
                tokio::spawn(async move {
                    if let Ok(running) = server.serve(stream).await {
                        let _ = running.waiting().await;
                    }
                });
            };
            let _ = mcp_live::listen(&listen_on, on_conn).await;
        });
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !mcp_live::window_listening(&endpoint) {
        assert!(Instant::now() < deadline, "the listener never came up on {endpoint}");
        std::thread::sleep(Duration::from_millis(20));
    }

    let mut child = Command::new(exe())
        .arg("--mcp")
        .env("EVE_MCP_ENDPOINT", &endpoint)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn --mcp");
    // A relay that never exits hangs `child.wait()` forever — this test once
    // did, for 6.5 hours, before that bug was found. A watchdog turns a
    // regression into a fast failure instead of a stuck CI job. Harmless
    // when the test passes: the child is long gone by the time this fires,
    // so the kill just errors, which is ignored.
    let pid = child.id();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(30));
        #[cfg(windows)]
        let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/F"]).status();
        #[cfg(not(windows))]
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
    });
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    let init = request(
        &mut stdin,
        &mut out,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"relay-test","version":"0"}}}"#,
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "eve-settings-editor", "{init}");
    writeln!(stdin, r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#).unwrap();
    let status = request(&mut stdin, &mut out, r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"status","arguments":{}}}"#);
    let text = status["result"]["content"][0]["text"].as_str().unwrap_or_else(|| panic!("{status}"));
    let body: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(body["mode"], "live", "served by the listener, not headless: {body}");

    drop(stdin);
    let exit = child.wait().unwrap();
    assert!(exit.success(), "the relay exits cleanly when the client closes stdin: {exit}");
}
