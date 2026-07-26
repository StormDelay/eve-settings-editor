//! Editor preferences — the app's own settings, not EVE's. Written to a JSON
//! file in the platform config dir; nothing here ever touches a settings file.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// `#[serde(default)]` on every struct IS the extensibility contract: a later
/// build can add a field or a sibling section and files written by today's
/// build still load, and vice versa. There is deliberately no version field —
/// a version number with no migration code behind it is decoration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub layout: LayoutPrefs,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutPrefs {
    /// Window ids the user forced INTO the clutter set.
    pub clutter: Vec<String>,
    /// Window ids the user forced OUT of it.
    pub visible: Vec<String>,
}

/// Read the file, or defaults. A file we cannot parse is USER DATA: move it
/// aside so a hand-edit gone wrong is recoverable, rather than silently
/// overwriting it on the next save.
pub fn load_from(path: &Path) -> Preferences {
    let Ok(raw) = std::fs::read_to_string(path) else { return Preferences::default() };
    match serde_json::from_str(&raw) {
        Ok(p) => p,
        Err(_) => {
            let _ = std::fs::rename(path, path.with_extension("json.bad"));
            Preferences::default()
        }
    }
}

pub fn save_to(path: &Path, prefs: &Preferences) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(prefs).map_err(std::io::Error::other)?)
}

/// `<app config dir>/preferences.json` — created lazily, on first save.
pub fn path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    app.path()
        .app_config_dir()
        .map(|d| d.join("preferences.json"))
        .map_err(|e| format!("no config directory: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("eve-prefs-test-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_missing_file_loads_defaults() {
        let p = temp_dir("missing").join("preferences.json");
        let prefs = load_from(&p);
        assert!(prefs.layout.clutter.is_empty());
        assert!(prefs.layout.visible.is_empty());
        assert!(!p.exists(), "loading must not create the file");
    }

    #[test]
    fn it_round_trips() {
        let p = temp_dir("roundtrip").join("preferences.json");
        let mut prefs = Preferences::default();
        prefs.layout.clutter.push("market".into());
        prefs.layout.visible.push("chatchannel_private_x".into());
        save_to(&p, &prefs).unwrap();
        let back = load_from(&p);
        assert_eq!(back.layout.clutter, vec!["market".to_string()]);
        assert_eq!(back.layout.visible, vec!["chatchannel_private_x".to_string()]);
    }

    #[test]
    fn a_corrupt_file_is_moved_aside_not_clobbered() {
        let dir = temp_dir("corrupt");
        let p = dir.join("preferences.json");
        std::fs::write(&p, b"{ this is not json").unwrap();
        let prefs = load_from(&p);
        assert!(prefs.layout.clutter.is_empty(), "a corrupt file must fall back to defaults");
        assert!(dir.join("preferences.json.bad").exists(), "the user's bad file must be recoverable");
    }

    #[test]
    fn an_unknown_key_still_loads() {
        // The forward-compatibility contract: a file written by a LATER build
        // must not break this one, and #[serde(default)] must cover a section
        // this build has never heard of.
        let p = temp_dir("unknown").join("preferences.json");
        std::fs::write(&p, br#"{"layout":{"clutter":["market"]},"future":{"x":1}}"#).unwrap();
        let prefs = load_from(&p);
        assert_eq!(prefs.layout.clutter, vec!["market".to_string()]);
    }

    #[test]
    fn a_missing_section_defaults() {
        let p = temp_dir("partial").join("preferences.json");
        std::fs::write(&p, b"{}").unwrap();
        let prefs = load_from(&p);
        assert!(prefs.layout.clutter.is_empty());
    }
}
