//! Fixture builders shared by `ops.rs` and `setup.rs`, which were one file and
//! one test module until the batch/setup half moved out.
//!
//! `#[cfg(test)]` in lib.rs, so none of this reaches a release build.

use blue_marshal::Value;
use std::fs;
use std::path::PathBuf;

/// A byte-string, the shape EVE stores nearly every dict key and name in.
pub(crate) fn b(s: &str) -> Value {
    Value::Bytes(s.as_bytes().to_vec())
}

/// Write `bytes` to a fresh directory of its own and return the path.
///
/// A counter, not just pid+name: several tests call this with the same name and
/// run concurrently by default, and two threads racing
/// `remove_dir_all`/`create_dir_all` on one shared directory intermittently
/// deletes it out from under the other (observed as a NotFound on the write).
pub(crate) fn temp_file(name: &str, bytes: &[u8]) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("app-ops-{}-{name}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let p = dir.join("core_user_5.dat");
    fs::write(&p, bytes).unwrap();
    p
}
