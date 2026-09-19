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
