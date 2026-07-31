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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutPrefs {
    /// Window ids the user forced INTO the clutter set.
    pub clutter: Vec<String>,
    /// Window ids the user forced OUT of it.
    pub visible: Vec<String>,
    /// Whether the layout canvas draws each rectangle's internals. Purely a
    /// view setting — it changes nothing about any EVE settings file.
    pub detail: bool,
    /// How many locked targets the canvas draws the target list at. Also a
    /// view setting: no file records how many things a pilot locks, so the
    /// canvas has to be told. See `layout.ts`'s `hudRects`.
    pub targets: u8,
}

/// Hand-written rather than derived because `targets` is the one field whose
/// sensible default is not zero — and container-level `#[serde(default)]`
/// fills every missing field from here, so a preferences file written before
/// this field existed loads with 4 rather than a target list drawn as a
/// zero-height sliver.
impl Default for LayoutPrefs {
    fn default() -> Self {
        Self { clutter: Vec::new(), visible: Vec::new(), detail: false, targets: 4 }
    }
}

/// Read the file, or defaults. A file we cannot parse is USER DATA: move it
/// aside so a hand-edit gone wrong is recoverable, rather than silently
/// overwriting it on the next save. Rename is the normal path (atomic, no
/// extra copy); if it fails — locked file, permission denial, AV
/// interference — fall back to copying the bytes instead, which preserves
/// them just the same. If both fail, the file is inaccessible enough that a
/// later write would likely fail too, so nothing recoverable is lost by
/// giving up: this stays infallible and still returns defaults so the editor
/// can open.
pub fn load_from(path: &Path) -> Preferences {
    let Ok(raw) = std::fs::read_to_string(path) else { return Preferences::default() };
    match serde_json::from_str(&raw) {
        Ok(p) => p,
        Err(_) => {
            let bad = path.with_extension("json.bad");
            if std::fs::rename(path, &bad).is_err() {
                let _ = std::fs::copy(path, &bad);
            }
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
    fn a_file_written_before_the_targets_field_loads_with_the_default() {
        // The extensibility contract in practice: an older build's file has no
        // `targets` key at all, and 0 would draw the target list as a sliver.
        let p = temp_dir("old-shape").join("preferences.json");
        std::fs::write(&p, br#"{"layout":{"clutter":[],"visible":[],"detail":true}}"#).unwrap();
        let prefs = load_from(&p);
        assert_eq!(prefs.layout.targets, 4);
        assert!(prefs.layout.detail, "the fields it DOES carry still load");
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
        // The forward-compatibility contract has two halves, each covered by
        // a different mechanism. THIS test covers the "file is newer than the
        // build" half: serde ignores unrecognized fields by default (nothing
        // in this module opts into #[serde(deny_unknown_fields)]), so a
        // "future" section this build has never heard of doesn't break
        // parsing. `a_missing_section_defaults` below covers the other half —
        // a file OLDER than the build, missing a section entirely — which is
        // what #[serde(default)] is actually for.
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

    // Windows-only: forces the actual failure mode the copy fallback exists
    // for. Holding the corrupt file open with a share mode that grants
    // FILE_SHARE_READ/WRITE but withholds FILE_SHARE_DELETE reproduces a real
    // "locked by another process" condition — Windows then refuses the
    // rename (it needs delete access) while a plain read, and therefore a
    // copy, still succeed. This is not arrangeable on the CI (Linux) runner,
    // which is why it's cfg-gated rather than run everywhere.
    #[cfg(windows)]
    #[test]
    fn a_locked_corrupt_file_still_survives_via_the_copy_fallback() {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 0x1;
        const FILE_SHARE_WRITE: u32 = 0x2;

        let dir = temp_dir("locked");
        let p = dir.join("preferences.json");
        std::fs::write(&p, b"{ this is not json").unwrap();

        let held_open = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE) // no FILE_SHARE_DELETE
            .open(&p)
            .unwrap();

        let prefs = load_from(&p);
        drop(held_open);

        assert!(prefs.layout.clutter.is_empty(), "a corrupt file must fall back to defaults");
        // The rename failed (that's the point of the lock), so the fallback
        // is a copy, not a move: the original is left in place too. Either
        // way the user's bytes are recoverable, which is the actual contract.
        assert_eq!(
            std::fs::read(dir.join("preferences.json.bad")).unwrap(),
            b"{ this is not json",
            "the copy fallback must preserve the original bytes"
        );
    }
}
