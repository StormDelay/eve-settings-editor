//! Timestamped backups: enumeration and one-click restore. Restore itself
//! backs up the current file first (spec §5), so it is also reversible.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::save::{atomic_write, backup_current};

#[derive(Debug, Serialize)]
pub struct BackupInfo {
    pub path: PathBuf,
    pub file_name: String,
    pub size: u64,
}

/// Backups of `target`, newest first. The timestamp is lexically sortable
/// (`YYYY-MM-DDTHHMMSSZ`), so sorting by file name descending is by time.
pub fn list_backups(target: &Path) -> Vec<BackupInfo> {
    let Some(dir) = target.parent() else { return vec![] };
    let dir = dir.join("eve-settings-editor-backups");
    let Some(name) = target.file_name() else { return vec![] };
    let prefix = format!("{}.", name.to_string_lossy());
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if file_name.starts_with(&prefix) && file_name.ends_with(".bak") {
                out.push(BackupInfo {
                    path: entry.path(),
                    size: entry.metadata().map(|m| m.len()).unwrap_or(0),
                    file_name,
                });
            }
        }
    }
    out.sort_by(|a, b| b.file_name.cmp(&a.file_name));
    out
}

/// Replace `target` with `backup`'s content: back up the current target
/// first, then write atomically. Returns the pre-restore backup's path.
/// The backup's content is validated as a decodable stream before anything
/// is touched — restoring a corrupt backup would defeat the whole chain.
pub fn restore(backup: &Path, target: &Path) -> Result<PathBuf, String> {
    let bytes = fs::read(backup).map_err(|e| format!("read backup: {e}"))?;
    blue_marshal::decode(&bytes).map_err(|e| format!("backup does not decode: {e}"))?;
    let pre = backup_current(target)?;
    atomic_write(target, &bytes)?;
    Ok(pre)
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::{encode, Value};

    fn setup(name: &str) -> (PathBuf, PathBuf, Vec<u8>, Vec<u8>) {
        let dir = std::env::temp_dir().join(format!(
            "settings-model-backups-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let v1 = encode(&Value::Int(100)).unwrap();
        let v2 = encode(&Value::Int(200)).unwrap();
        let target = dir.join("core_user_9.dat");
        fs::write(&target, &v2).unwrap();
        let bdir = dir.join("eve-settings-editor-backups");
        fs::create_dir_all(&bdir).unwrap();
        let backup = bdir.join("core_user_9.dat.2026-07-13T000000Z.bak");
        fs::write(&backup, &v1).unwrap();
        (target, backup, v1, v2)
    }

    #[test]
    fn lists_only_matching_backups_newest_first() {
        let (target, _b, _, _) = setup("list");
        let bdir = target.parent().unwrap().join("eve-settings-editor-backups");
        fs::write(bdir.join("core_user_9.dat.2026-07-14T000000Z.bak"), b"x").unwrap();
        fs::write(bdir.join("core_char_1.dat.2026-07-13T000000Z.bak"), b"y").unwrap(); // other file
        fs::write(bdir.join("notes.txt"), b"z").unwrap(); // not a backup
        let list = list_backups(&target);
        assert_eq!(list.len(), 2);
        assert!(list[0].file_name.contains("2026-07-14"), "newest first");
        assert!(list[1].file_name.contains("2026-07-13"));
    }

    #[test]
    fn restore_backs_up_current_then_replaces() {
        let (target, backup, v1, v2) = setup("restore");
        let pre = restore(&backup, &target).unwrap();
        assert_eq!(fs::read(&target).unwrap(), v1, "target now holds the backup content");
        assert_eq!(fs::read(&pre).unwrap(), v2, "pre-restore state preserved");
    }

    #[test]
    fn restore_refuses_undecodable_backup() {
        let (target, _b, _, v2) = setup("corrupt");
        let bad = target
            .parent()
            .unwrap()
            .join("eve-settings-editor-backups")
            .join("core_user_9.dat.2026-07-15T000000Z.bak");
        fs::write(&bad, b"garbage").unwrap();
        assert!(restore(&bad, &target).unwrap_err().contains("does not decode"));
        assert_eq!(fs::read(&target).unwrap(), v2, "target untouched");
    }
}
