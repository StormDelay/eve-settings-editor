//! The spec §5 save-path invariant chain. Executed in order; ANY failure
//! aborts with the on-disk file untouched:
//!   1. encode   2. verify (decode own output, bit-level compare)
//!   3. conflict check (mtime+len vs load)   4. backup (no backup ⇒ no write)
//!   5. atomic write (temp file + rename; std's rename replaces atomically
//!      on Windows via MoveFileExW(MOVEFILE_REPLACE_EXISTING) and on POSIX
//!      via rename(2)).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use blue_marshal::{decode, encode};
use serde::Serialize;

use crate::document::{Document, Fidelity};

#[derive(Debug, Serialize)]
pub struct SaveReport {
    pub backup_path: PathBuf,
    pub bytes_written: usize,
}

#[derive(Debug, Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum SaveError {
    /// Document loaded ReadOnly — saving is refused outright (spec §7).
    ReadOnly(String),
    Encode(String),
    /// Our own output did not decode back to the in-memory tree. Writer
    /// bug — nothing was written (spec §5.2).
    VerifyMismatch,
    /// The on-disk file vanished; without it there is nothing to back up.
    MissingOriginal(String),
    /// The file changed on disk since load (mtime or length). Retry with
    /// `force_conflict = true` after explicit user confirmation.
    Conflict,
    Backup(String),
    Write(String),
}

pub fn save(doc: &mut Document, force_conflict: bool) -> Result<SaveReport, SaveError> {
    if let Fidelity::ReadOnly { reason } = &doc.fidelity {
        return Err(SaveError::ReadOnly(reason.clone()));
    }
    // 1. Encode.
    let encoded = encode(&doc.value).map_err(|e| SaveError::Encode(e.to_string()))?;
    // 2. Verify: decode our own output and compare bit-exactly.
    match decode(&encoded) {
        Ok(back) if back.bits_eq(&doc.value) => {}
        _ => return Err(SaveError::VerifyMismatch),
    }
    // 3. Conflict check.
    let meta = fs::metadata(&doc.path).map_err(|e| SaveError::MissingOriginal(e.to_string()))?;
    let changed = meta.len() != doc.loaded_len
        || match (meta.modified().ok(), doc.loaded_mtime) {
            (Some(now), Some(then)) => now != then,
            _ => false,
        };
    if changed && !force_conflict {
        return Err(SaveError::Conflict);
    }
    // 4. Backup — hard requirement.
    let backup_path = backup_current(&doc.path).map_err(SaveError::Backup)?;
    // 5. Atomic write.
    atomic_write(&doc.path, &encoded).map_err(SaveError::Write)?;
    // Refresh the conflict baseline. The write itself has already succeeded,
    // so a failure to re-read metadata here must NOT surface as an error:
    // fall back to a degraded baseline — the length is known exactly (we
    // wrote it), and an unknown mtime merely disables the mtime half of the
    // next conflict check until a later save refreshes it.
    match fs::metadata(&doc.path) {
        Ok(meta) => {
            doc.loaded_mtime = meta.modified().ok();
            doc.loaded_len = meta.len();
        }
        Err(_) => {
            doc.loaded_mtime = None;
            doc.loaded_len = encoded.len() as u64;
        }
    }
    Ok(SaveReport { backup_path, bytes_written: encoded.len() })
}

/// Copy `target` into `<dir>/eve-settings-editor-backups/<name>.<stamp>.bak`
/// (or `<name>.<stamp>_2.bak`, `_3`, ... if that name is already taken —
/// `utc_stamp()` is only second-precision) and verify the copy landed with
/// the same length. Also used by restore.
pub(crate) fn backup_current(target: &Path) -> Result<PathBuf, String> {
    let dir = target
        .parent()
        .ok_or_else(|| "target has no parent directory".to_string())?
        .join("eve-settings-editor-backups");
    fs::create_dir_all(&dir).map_err(|e| format!("create backup dir: {e}"))?;
    let name = target
        .file_name()
        .ok_or_else(|| "target has no file name".to_string())?
        .to_string_lossy();
    let stamp = utc_stamp();
    // Two backups within the same second would otherwise collide on this
    // path and fs::copy would silently clobber the earlier one. Disambiguate
    // with a _2, _3, ... suffix; "_" sorts above "." in ASCII, so
    // list_backups' descending file-name sort still puts these newest-first.
    let mut backup = dir.join(format!("{name}.{stamp}.bak"));
    let mut n = 2;
    while backup.exists() {
        backup = dir.join(format!("{name}.{stamp}_{n}.bak"));
        n += 1;
    }
    fs::copy(target, &backup).map_err(|e| format!("copy to backup: {e}"))?;
    let (src, dst) = (
        fs::metadata(target).map_err(|e| e.to_string())?.len(),
        fs::metadata(&backup).map_err(|e| e.to_string())?.len(),
    );
    if src != dst {
        return Err(format!("backup size mismatch ({dst} of {src} bytes)"));
    }
    Ok(backup)
}

pub(crate) fn atomic_write(target: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = target.parent().ok_or_else(|| "no parent dir".to_string())?;
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    let temp = temp_path(dir, &name);
    fs::write(&temp, bytes).map_err(|e| format!("write temp: {e}"))?;
    fs::rename(&temp, target).map_err(|e| {
        let _ = fs::remove_file(&temp);
        format!("rename over target: {e}")
    })
}

/// Unique temp path per call: the pid guards against other processes, the
/// counter against two saves of the same path racing within this process
/// (two `Document`s independently opened on one file).
fn temp_path(dir: &Path, name: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    dir.join(format!(".{name}.tmp-{}-{n}", std::process::id()))
}

/// UTC timestamp `YYYY-MM-DDTHHMMSSZ` — ISO-8601 with basic (colon-free)
/// time, valid in Windows file names; matches tools/sync-corpus.ps1.
pub(crate) fn utc_stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (y, m, d) = civil_from_days((secs / 86400) as i64);
    let rem = secs % 86400;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}{:02}{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Days-since-1970 to (year, month, day). Howard Hinnant's `civil_from_days`
/// algorithm — exact for the whole i64 day range we care about.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_from_days_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(11_017), (2000, 3, 1));
        assert_eq!(civil_from_days(20_647), (2026, 7, 13));
    }

    #[test]
    fn utc_stamp_shape() {
        let s = utc_stamp();
        // e.g. 2026-07-13T145959Z
        assert_eq!(s.len(), 18);
        assert!(!s.contains(':'), "colons are invalid in Windows file names");
        assert!(s.ends_with('Z'));
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[10..11], "T");
    }

    #[test]
    fn temp_paths_are_unique_within_a_process() {
        let dir = Path::new("x");
        assert_ne!(temp_path(dir, "f.dat"), temp_path(dir, "f.dat"));
    }

    #[test]
    fn backup_current_disambiguates_same_second_collisions() {
        let dir = std::env::temp_dir()
            .join(format!("settings-model-save-{}-collide", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("core_user_5.dat");

        fs::write(&target, b"v1").unwrap();
        let first = backup_current(&target).unwrap();

        fs::write(&target, b"v2").unwrap();
        let second = backup_current(&target).unwrap();

        assert_ne!(first, second, "same-second backups must not collide");
        assert_eq!(fs::read(&first).unwrap(), b"v1");
        assert_eq!(fs::read(&second).unwrap(), b"v2");
    }
}
