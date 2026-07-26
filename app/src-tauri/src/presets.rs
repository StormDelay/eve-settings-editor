//! The preset library: a folder of presets, each holding a char-side and an
//! account-side settings document. Nothing here invents a file format — a
//! preset's two files are ordinary settings documents, which is exactly what
//! lets the editors open one as if it were a character.

use std::path::{Component, Path, PathBuf};

pub const CHAR_FILE: &str = "char.dat";
pub const USER_FILE: &str = "user.dat";
/// Written only for a full (Everything) preset. Its absence means "pruned",
/// which is the safe reading: fewer aspects offered, never a destructive full
/// copy built on a partial document.
pub const MARKER_FILE: &str = "preset.json";
/// Claimed inside every preset folder by the save chain's backup step.
const BACKUP_DIR: &str = "eve-settings-editor-backups";

const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL",
    "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
    "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// A rejected preset name, carrying the message shown to the user.
#[derive(Debug)]
pub struct NameError(pub String);

/// `<app data dir>/presets` — alongside accounts.json rather than
/// preferences.json: presets are user data, not configuration.
pub fn presets_dir(app_data: &Path) -> PathBuf {
    app_data.join("presets")
}

/// Validate a user-typed preset name. Rejects rather than silently rewriting,
/// so the name the user sees is the name on disk.
pub fn sanitize_name(raw: &str) -> Result<String, NameError> {
    let bad = |m: &str| Err(NameError(m.to_string()));
    if raw.chars().all(char::is_whitespace) {
        return bad("A preset needs a name.");
    }
    if raw.chars().count() > 100 {
        return bad("A preset name can be at most 100 characters.");
    }
    if let Some(c) = raw.chars().find(|c| "/\\:*?\"<>|".contains(*c) || c.is_control()) {
        return Err(NameError(format!("A preset name cannot contain {c:?}.")));
    }
    if raw.starts_with('.') || raw.ends_with('.') || raw.starts_with(' ') || raw.ends_with(' ') {
        return bad("A preset name cannot start or end with a dot or a space.");
    }
    let stem = raw.split('.').next().unwrap_or(raw);
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem)) {
        return bad("That name is reserved by Windows. Pick another.");
    }
    if raw.eq_ignore_ascii_case(BACKUP_DIR) {
        return bad("That name is used by the editor's own backups.");
    }
    Ok(raw.to_string())
}

/// True when `name` is exactly one ordinary path component — no separators, no
/// `.`/`..`, no drive prefix or root. Checked independently of `sanitize_name`
/// so that a gap in the name rules still cannot escape the presets directory.
fn is_single_normal_component(name: &str) -> bool {
    let mut comps = Path::new(name).components();
    matches!(comps.next(), Some(Component::Normal(_))) && comps.next().is_none()
}

/// The folder a preset lives in. Two independent guards: the name rules above,
/// and a containment check that the name is exactly one ordinary path
/// component — so a gap in the first cannot escape the presets directory.
pub fn preset_path(app_data: &Path, name: &str) -> Result<PathBuf, NameError> {
    let name = sanitize_name(name)?;
    if !is_single_normal_component(&name) {
        return Err(NameError("Invalid preset name.".into()));
    }
    Ok(presets_dir(app_data).join(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn accepts_ordinary_names() {
        assert_eq!(sanitize_name("PvP layout").unwrap(), "PvP layout");
        assert_eq!(sanitize_name("Mining — Ørca").unwrap(), "Mining — Ørca");
        assert_eq!(sanitize_name("v2.1 setup").unwrap(), "v2.1 setup");
    }

    #[test]
    fn rejects_empty_and_whitespace() {
        assert!(sanitize_name("").is_err());
        assert!(sanitize_name("   ").is_err());
    }

    #[test]
    fn rejects_path_separators_and_wildcards() {
        for bad in ["a/b", "a\\b", "C:", "a*b", "a?b", "a\"b", "a<b", "a>b", "a|b"] {
            assert!(sanitize_name(bad).is_err(), "{bad} must be rejected");
        }
    }

    #[test]
    fn rejects_control_characters() {
        assert!(sanitize_name("a\nb").is_err());
        assert!(sanitize_name("a\0b").is_err());
    }

    #[test]
    fn rejects_leading_or_trailing_dot_or_space() {
        // Windows strips these, so "foo." and "foo" would collide on disk.
        for bad in [".hidden", "trailing.", " lead", "trail ", ".", ".."] {
            assert!(sanitize_name(bad).is_err(), "{bad} must be rejected");
        }
    }

    #[test]
    fn rejects_windows_reserved_device_names() {
        for bad in ["CON", "con", "NUL", "com1", "LPT9", "aux"] {
            assert!(sanitize_name(bad).is_err(), "{bad} must be rejected");
        }
        // Only the stem matters to Windows, so an extension does not save it.
        assert!(sanitize_name("CON.txt").is_err());
        // But a name that merely starts with those letters is fine.
        assert!(sanitize_name("Console setup").is_ok());
    }

    #[test]
    fn rejects_the_backup_directory_name() {
        assert!(sanitize_name("eve-settings-editor-backups").is_err());
    }

    #[test]
    fn rejects_over_long_names() {
        assert!(sanitize_name(&"a".repeat(101)).is_err());
        assert!(sanitize_name(&"a".repeat(100)).is_ok());
    }

    #[test]
    fn preset_path_is_a_direct_child_of_the_presets_dir() {
        let root = Path::new("/data");
        let p = preset_path(root, "PvP layout").unwrap();
        assert_eq!(p, Path::new("/data").join("presets").join("PvP layout"));
    }

    #[test]
    fn preset_path_refuses_traversal_and_absolute_names() {
        // sanitize_name already rejects these; preset_path asserts containment
        // independently, so a future sanitiser gap still cannot escape.
        assert!(preset_path(Path::new("/data"), "../escape").is_err());
        assert!(preset_path(Path::new("/data"), "/etc/passwd").is_err());
        assert!(preset_path(Path::new("/data"), "a/b").is_err());
    }

    #[test]
    fn the_containment_guard_stands_alone() {
        // These never reach the guard through preset_path, because
        // sanitize_name rejects them first. Testing the guard directly is what
        // makes the second line of defence real rather than decorative.
        //
        // Only cases that behave the same on every platform belong here: CI is
        // Linux, where `\` and `:` are ordinary filename characters, so `a\b`
        // and `C:\Windows` ARE single components there. Both are already
        // covered as literal bad characters by rejects_path_separators_and_wildcards.
        for bad in ["..", ".", "a/b", "/etc/passwd", ""] {
            assert!(!is_single_normal_component(bad), "{bad:?} must not be a single component");
        }
        for good in ["PvP layout", "Mining", "v2.1 setup", "a/"] {
            assert!(is_single_normal_component(good), "{good:?} is a single component");
        }
    }
}
