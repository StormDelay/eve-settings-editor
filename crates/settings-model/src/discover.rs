//! Discovery of EVE settings profiles in OS-standard locations. Layout
//! (verified against real snapshots; example ID synthetic):
//!   <root>/<install>_<server>/settings_<profile>/core_(char|user)_<id>.dat
//! e.g. c_eve_sharedcache_tq_tranquility/settings_Default/core_char_123456789.dat
//! The server name is the last `_`-separated token of the install dir.
//!
//! Library code takes caller-supplied roots so tests never touch the live
//! directory (spec §8); only the app passes `default_roots()` at runtime.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Profile {
    /// Install-directory name minus the server suffix, e.g. "c_eve_sharedcache_tq".
    pub install: String,
    /// Last underscore token of the install dir, e.g. "tranquility".
    pub server: String,
    /// The settings_<profile> suffix, e.g. "Default".
    pub profile: String,
    pub dir: PathBuf,
    pub files: Vec<SettingsFile>,
}

#[derive(Debug, Serialize)]
pub struct SettingsFile {
    pub path: PathBuf,
    pub file_name: String,
    pub kind: FileKind,
    /// Numeric id from core_char_<id>/core_user_<id>; None for anomalous
    /// names (real examples exist: `core_char__.dat`).
    pub id: Option<u64>,
    pub size: u64,
    pub modified_unix: Option<u64>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Char,
    User,
    Other,
}

/// OS-standard EVE settings roots that actually exist on this machine.
pub fn default_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if cfg!(target_os = "windows") {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            roots.push(PathBuf::from(local).join("CCP").join("EVE"));
        }
    } else if cfg!(target_os = "macos") {
        if let Ok(home) = std::env::var("HOME") {
            roots.push(
                PathBuf::from(home)
                    .join("Library/Application Support/CCP/EVE"),
            );
        }
    } else {
        if let Ok(home) = std::env::var("HOME") {
            // Steam Proton prefix (EVE app id 8500).
            roots.push(PathBuf::from(&home).join(
                ".steam/steam/steamapps/compatdata/8500/pfx/drive_c/users/steamuser/AppData/Local/CCP/EVE",
            ));
        }
        if let Ok(prefix) = std::env::var("WINEPREFIX") {
            roots.push(
                PathBuf::from(prefix).join("drive_c/users").join(
                    std::env::var("USER").unwrap_or_else(|_| "steamuser".into()),
                ).join("AppData/Local/CCP/EVE"),
            );
        }
    }
    roots.retain(|r| r.is_dir());
    roots
}

pub fn discover(roots: &[PathBuf]) -> Vec<Profile> {
    let mut profiles = Vec::new();
    for root in roots {
        let Ok(installs) = fs::read_dir(root) else { continue };
        for install in installs.flatten() {
            let install_path = install.path();
            if !install_path.is_dir() {
                continue;
            }
            let install_name = install.file_name().to_string_lossy().into_owned();
            let (install_prefix, server) = match install_name.rsplit_once('_') {
                Some((p, s)) if !s.is_empty() => (p.to_string(), s.to_string()),
                _ => (install_name.clone(), String::new()),
            };
            let Ok(settings_dirs) = fs::read_dir(&install_path) else { continue };
            for sdir in settings_dirs.flatten() {
                let sdir_path = sdir.path();
                let sdir_name = sdir.file_name().to_string_lossy().into_owned();
                let Some(profile_name) = sdir_name.strip_prefix("settings_") else {
                    continue;
                };
                if !sdir_path.is_dir() {
                    continue;
                }
                let files = collect_files(&sdir_path);
                if files.is_empty() {
                    continue;
                }
                profiles.push(Profile {
                    install: install_prefix.clone(),
                    server: server.clone(),
                    profile: profile_name.to_string(),
                    dir: sdir_path,
                    files,
                });
            }
        }
    }
    profiles.sort_by(|a, b| (&a.server, &a.profile).cmp(&(&b.server, &b.profile)));
    profiles
}

/// The character/account id encoded at the start of a `core_(char|user)_<id>`
/// stem: the leading run of digits. Tolerates trailing suffixes users add to
/// backup copies (e.g. `core_char_90000001 - old.dat` → `90000001`), and yields
/// `None` for names with no leading digits — the anomalous `core_char__.dat`
/// and tuple-keyed files still resolve to `None`.
fn leading_u64(stem_rest: &str) -> Option<u64> {
    let digits: String = stem_rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<u64>().ok()
}

/// The one definition of the `core_char_`/`core_user_` prefix rules: derive
/// kind + id from a `.dat` stem (file name minus the `.dat` extension).
/// Routed through by both the scan loop below and `file_kind`, so a caller
/// re-deriving a path's kind elsewhere (e.g. a trust-boundary check) can
/// never drift from what `discover` itself considers a char/user file.
fn kind_and_id(stem: &str) -> (FileKind, Option<u64>) {
    if let Some(rest) = stem.strip_prefix("core_char_") {
        (FileKind::Char, leading_u64(rest))
    } else if let Some(rest) = stem.strip_prefix("core_user_") {
        (FileKind::User, leading_u64(rest))
    } else {
        (FileKind::Other, None)
    }
}

/// Derive a settings file's kind from its path's file name alone (no
/// directory scan). For callers that must re-verify a path's kind at a
/// trust boundary without re-running `discover`.
pub fn file_kind(path: &Path) -> FileKind {
    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = file_name.trim_end_matches(".dat");
    kind_and_id(stem).0
}

fn collect_files(dir: &Path) -> Vec<SettingsFile> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if !file_name.ends_with(".dat") || !path.is_file() {
            continue;
        }
        let stem = file_name.trim_end_matches(".dat");
        let (kind, id) = kind_and_id(stem);
        let meta = entry.metadata().ok();
        out.push(SettingsFile {
            path,
            file_name,
            kind,
            id,
            size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
            modified_unix: meta
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
        });
    }
    out.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_the_real_layout_from_a_temp_tree() {
        let root = std::env::temp_dir().join(format!(
            "settings-model-discover-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let sdir = root
            .join("c_eve_sharedcache_tq_tranquility")
            .join("settings_Default");
        fs::create_dir_all(&sdir).unwrap();
        // Synthetic IDs only — never commit real character/account IDs.
        fs::write(sdir.join("core_char_123456789.dat"), b"x").unwrap();
        fs::write(sdir.join("core_user_987654.dat"), b"x").unwrap();
        fs::write(sdir.join("core_char__.dat"), b"x").unwrap(); // real anomaly shape
        fs::write(sdir.join("prefs.ini"), b"x").unwrap(); // not a .dat
        let sisi = root
            .join("c_eve_sharedcache_sisi_singularity")
            .join("settings_Default");
        fs::create_dir_all(&sisi).unwrap();
        fs::write(sisi.join("core_user_1.dat"), b"x").unwrap();
        fs::create_dir_all(root.join("c_eve_sharedcache_tq_tranquility").join("cache"))
            .unwrap(); // non-settings dir ignored

        let profiles = discover(&[root.clone()]);
        assert_eq!(profiles.len(), 2);
        // sorted by (server, profile): singularity first
        assert_eq!(profiles[0].server, "singularity");
        let tq = &profiles[1];
        assert_eq!(tq.server, "tranquility");
        assert_eq!(tq.install, "c_eve_sharedcache_tq");
        assert_eq!(tq.profile, "Default");
        assert_eq!(tq.files.len(), 3);
        assert_eq!(tq.files[0].file_name, "core_char_123456789.dat");
        assert_eq!(tq.files[0].kind, FileKind::Char);
        assert_eq!(tq.files[0].id, Some(123456789));
        assert_eq!(tq.files[1].file_name, "core_char__.dat");
        assert_eq!(tq.files[1].kind, FileKind::Char);
        assert_eq!(tq.files[1].id, None, "anomalous names tolerated");
        assert_eq!(tq.files[2].kind, FileKind::User);
    }

    #[test]
    fn missing_roots_yield_empty_not_error() {
        let ghost = std::env::temp_dir().join("settings-model-no-such-root");
        assert!(discover(&[ghost]).is_empty());
    }

    #[test]
    fn file_kind_derives_from_the_path_file_name_alone() {
        assert_eq!(file_kind(Path::new("/x/core_char_123456789.dat")), FileKind::Char);
        assert_eq!(file_kind(Path::new("/x/core_user_987654.dat")), FileKind::User);
        // Anomalous name: still Char kind, matching discover's own tolerance.
        assert_eq!(file_kind(Path::new("/x/core_char__.dat")), FileKind::Char);
        assert_eq!(file_kind(Path::new("/x/prefs.ini")), FileKind::Other);
    }

    #[test]
    fn leading_u64_reads_id_before_a_backup_suffix() {
        // Synthetic ids only (repo rule). Canonical name and user-added backup
        // suffixes both yield the leading id.
        assert_eq!(leading_u64("90000001"), Some(90000001));
        assert_eq!(leading_u64("90000001 - old"), Some(90000001));
        assert_eq!(leading_u64("90000002_backup"), Some(90000002));
        // No leading digits → None (preserves the anomalous-file handling).
        assert_eq!(leading_u64("_"), None);
        assert_eq!(leading_u64("('char', None, 'dat')"), None);
        assert_eq!(leading_u64(""), None);
    }
}
