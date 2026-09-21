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
    assert!(
        init["result"]["instructions"].as_str().is_some_and(|s| s.contains("core_user_")),
        "instructions missing: {init}"
    );

    writeln!(stdin, r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#).unwrap();
    let list = request(&mut stdin, &mut out, r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
    assert_eq!(list["id"], 2, "{list}");
    let mut names: Vec<&str> =
        list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "autofill_clear_all", "autofill_get", "autofill_set", "builtin_presets", "chat_get", "chat_set_splits",
            "copy_apply", "copy_files", "copy_preview",
            "eve_guide", "fleet_edit", "fleet_get", "groups_search", "hud_get", "hud_set", "keybind_set", "keybinds_get", "layout_edit", "layout_get",
            "layout_render", "list_backups", "list_characters", "lookup_character", "neocom_edit", "neocom_get", "open", "overview_appearance_edit", "overview_columns_edit", "overview_get",
            "overview_pack_export", "overview_pack_import", "overview_pack_preview", "overview_presets_edit",
            "overview_tabs_edit", "probes_add_yaml", "probes_export_yaml", "probes_get", "probes_remove",
            "probes_reorder", "probes_set", "restore_backup", "save", "settings_preset_edit", "settings_presets_list", "status", "undo",
        ]
    );

    drop(stdin);
    let _ = child.wait();
}
