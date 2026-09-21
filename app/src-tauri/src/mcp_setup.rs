//! The "AI access" sheet's backend: where this exe is, the config snippet
//! any MCP client takes, and a Register/Unregister for Claude Desktop's
//! config file. Spec §6.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

use crate::ops::ErrDto;

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
/// `Claude/` in each. `Some` only when a `Claude` directory exists
/// ("installed"), either there or, for the Microsoft Store build, under the
/// package's cache (see `config_in`).
pub fn claude_desktop_config() -> Option<PathBuf> {
    config_in(&dirs::config_dir()?, &dirs::data_local_dir()?)
}

/// The Store build of Claude Desktop is an MSIX package, and Windows redirects
/// a packaged app's %APPDATA% to `%LOCALAPPDATA%\Packages\<family>\LocalCache/// Roaming` — so the classic directory never exists for it and its config
/// sits at the redirected path (verified 2026-09-21 against `Claude_pzs8sxrjxfjjc`,
/// v2.2553.1). The family name's suffix is a publisher hash, so it is matched
/// by prefix. `Packages` exists nowhere but Windows, so this needs no `cfg`.
/// Takes both roots so a test can point it at a temp directory.
fn config_in(config_dir: &Path, local_dir: &Path) -> Option<PathBuf> {
    let classic = config_dir.join("Claude");
    let dir = if classic.is_dir() {
        classic
    } else {
        std::fs::read_dir(local_dir.join("Packages"))
            .ok()?
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with("Claude_"))
            .map(|e| e.path().join("LocalCache").join("Roaming").join("Claude"))
            .find(|d| d.is_dir())?
    };
    Some(dir.join("claude_desktop_config.json"))
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
    if !config.parent().is_some_and(|d| d.is_dir()) {
        return Err(ErrDto::new("not_installed", "Claude Desktop's config directory does not exist"));
    }
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

    /// The Microsoft Store build of Claude Desktop is an MSIX package, and
    /// Windows redirects its %APPDATA% under the package's LocalCache — the
    /// classic `Claude` directory never exists for it.
    #[test]
    fn the_store_build_is_found_under_its_package_local_cache() {
        let d = dir("msix");
        let (config, local) = (d.join("Roaming"), d.join("Local"));
        fs::create_dir_all(&config).unwrap();
        fs::create_dir_all(&local).unwrap();
        assert_eq!(config_in(&config, &local), None, "nothing installed");

        let msix = local.join("Packages").join("Claude_pzs8sxrjxfjjc").join("LocalCache").join("Roaming").join("Claude");
        fs::create_dir_all(&msix).unwrap();
        assert_eq!(config_in(&config, &local), Some(msix.join("claude_desktop_config.json")));

        // Another package whose name merely starts with Claude, with no
        // Claude directory in its cache, is not it.
        fs::create_dir_all(local.join("Packages").join("ClaudeSomethingElse_abc").join("LocalCache")).unwrap();
        assert_eq!(config_in(&config, &local), Some(msix.join("claude_desktop_config.json")));

        // The classic install wins when both exist.
        fs::create_dir_all(config.join("Claude")).unwrap();
        assert_eq!(config_in(&config, &local), Some(config.join("Claude").join("claude_desktop_config.json")));
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
