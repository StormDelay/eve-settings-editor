//! Command logic as plain functions over `AppState`, so it unit-tests
//! without a Tauri runtime. The `#[tauri::command]` wrappers in lib.rs are
//! one-liners delegating here.

use std::fs;
use std::path::Path;
use std::sync::Mutex;

use serde::Serialize;
use settings_model::{
    apply, default_roots, discover, project, save, Document, Fidelity, LoadError, Mutation,
    Node, Profile, SaveReport,
};

/// One document open at a time in V1 (spec batch-apply arrives in M4).
pub struct AppState(pub Mutex<Option<Document>>);

impl AppState {
    pub fn new() -> Self {
        AppState(Mutex::new(None))
    }
}

#[derive(Debug, Serialize)]
pub struct ErrDto {
    pub code: String,
    pub message: String,
}

impl ErrDto {
    fn new(code: &str, message: impl Into<String>) -> Self {
        ErrDto { code: code.into(), message: message.into() }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum OpenOutcome {
    Opened {
        path: String,
        file_name: String,
        fidelity: Fidelity,
        tree: Node,
    },
    /// Undecodable file: shown as a read-only hex view (spec §7 — never
    /// writable).
    ParseFailed {
        path: String,
        offset: usize,
        message: String,
        hex_preview: String,
    },
}

pub fn discover_profiles() -> Vec<Profile> {
    discover(&default_roots())
}

pub fn open_file(state: &AppState, path: &str) -> Result<OpenOutcome, ErrDto> {
    let p = Path::new(path);
    match Document::load(p) {
        Ok(doc) => {
            let outcome = OpenOutcome::Opened {
                path: path.to_string(),
                file_name: p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                fidelity: doc.fidelity.clone(),
                tree: project(&doc.value),
            };
            *state.0.lock().unwrap() = Some(doc);
            Ok(outcome)
        }
        Err(LoadError::Decode { offset, message }) => {
            let bytes = fs::read(p).map_err(|e| ErrDto::new("io", e.to_string()))?;
            *state.0.lock().unwrap() = None;
            Ok(OpenOutcome::ParseFailed {
                path: path.to_string(),
                offset,
                message,
                hex_preview: hex_preview(&bytes, offset),
            })
        }
        Err(LoadError::Io(e)) => Err(ErrDto::new("io", e)),
    }
}

pub fn close_file(state: &AppState) {
    *state.0.lock().unwrap() = None;
}

pub fn apply_mutation(state: &AppState, mutation: &Mutation) -> Result<Node, ErrDto> {
    let mut guard = state.0.lock().unwrap();
    let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no file open"))?;
    if let Fidelity::ReadOnly { reason } = &doc.fidelity {
        return Err(ErrDto::new("read_only", reason.clone()));
    }
    apply(&mut doc.value, mutation).map_err(|e| {
        // MutateError serializes as {"code": ..., "detail": ...}; flatten it.
        // The code drives UI behaviour (e.g. parse_key anchors the message to
        // the key field); the message is its Display form.
        let v = serde_json::to_value(&e).unwrap_or_default();
        ErrDto::new(
            v.get("code").and_then(|c| c.as_str()).unwrap_or("mutate"),
            e.to_string(),
        )
    })?;
    Ok(project(&doc.value))
}

pub fn save_document(state: &AppState, force: bool) -> Result<SaveReport, ErrDto> {
    let mut guard = state.0.lock().unwrap();
    let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no file open"))?;
    save(doc, force).map_err(|e| {
        let v = serde_json::to_value(&e).unwrap_or_default();
        ErrDto::new(
            v.get("code").and_then(|c| c.as_str()).unwrap_or("save"),
            match v.get("detail").and_then(|d| d.as_str()) {
                Some(d) => d.to_string(),
                None => format!("{e:?}"),
            },
        )
    })
}

pub fn list_file_backups(state: &AppState) -> Result<Vec<settings_model::BackupInfo>, ErrDto> {
    let guard = state.0.lock().unwrap();
    let doc = guard.as_ref().ok_or_else(|| ErrDto::new("no_document", "no file open"))?;
    Ok(settings_model::list_backups(&doc.path))
}

pub fn restore_backup(state: &AppState, backup_path: &str) -> Result<OpenOutcome, ErrDto> {
    let target = {
        let guard = state.0.lock().unwrap();
        let doc = guard.as_ref().ok_or_else(|| ErrDto::new("no_document", "no file open"))?;
        doc.path.clone()
    };
    settings_model::restore(Path::new(backup_path), &target)
        .map_err(|e| ErrDto::new("restore", e))?;
    // Re-open so the UI reflects the restored content and a fresh baseline.
    open_file(state, &target.to_string_lossy())
}

/// Hex dump of up to 16 lines x 16 bytes centred on `around` (clamped),
/// with offsets and an ASCII gutter — enough context to eyeball a parse
/// failure without a real hex editor.
fn hex_preview(bytes: &[u8], around: usize) -> String {
    const LINES: usize = 16;
    let start = (around.saturating_sub(LINES / 2 * 16) / 16) * 16;
    let mut out = String::new();
    for line in 0..LINES {
        let off = start + line * 16;
        if off >= bytes.len() {
            break;
        }
        let chunk = &bytes[off..bytes.len().min(off + 16)];
        out.push_str(&format!("{off:08x}  "));
        for i in 0..16 {
            match chunk.get(i) {
                Some(b) => out.push_str(&format!("{b:02x} ")),
                None => out.push_str("   "),
            }
        }
        out.push(' ');
        for &b in chunk {
            out.push(if (0x20..0x7F).contains(&b) { b as char } else { '.' });
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::{encode, Value};
    use std::path::PathBuf;

    fn temp_file(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("app-ops-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("core_user_5.dat");
        fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn open_editable_file_projects_and_stores_state() {
        let bytes = encode(&Value::Dict(vec![(
            Value::Bytes(b"k".to_vec()),
            Value::Int(5),
        )]))
        .unwrap();
        let path = temp_file("open", &bytes);
        let state = AppState::new();
        let outcome = open_file(&state, path.to_str().unwrap()).unwrap();
        match outcome {
            OpenOutcome::Opened { fidelity, tree, file_name, .. } => {
                assert_eq!(fidelity, Fidelity::Editable);
                assert_eq!(file_name, "core_user_5.dat");
                assert_eq!(tree.kind, "dict");
            }
            _ => panic!("expected Opened"),
        }
        assert!(state.0.lock().unwrap().is_some());
        close_file(&state);
        assert!(state.0.lock().unwrap().is_none());
    }

    #[test]
    fn open_undecodable_file_returns_hex_preview() {
        let path = temp_file("bad", &[0x7E, 0, 0, 0, 0, 0x3D]);
        let state = AppState::new();
        match open_file(&state, path.to_str().unwrap()).unwrap() {
            OpenOutcome::ParseFailed { offset, hex_preview, .. } => {
                assert_eq!(offset, 5);
                assert!(hex_preview.starts_with("00000000  7e 00 00 00 00 3d"));
            }
            _ => panic!("expected ParseFailed"),
        }
        assert!(state.0.lock().unwrap().is_none());
    }

    #[test]
    fn open_missing_file_is_an_io_error() {
        let state = AppState::new();
        let err = open_file(&state, "Z:/no/such/file.dat").unwrap_err();
        assert_eq!(err.code, "io");
    }

    use settings_model::Mutation;
    use settings_model::path::Step;

    fn open_sample(name: &str) -> (AppState, PathBuf) {
        let bytes = encode(&Value::Dict(vec![(
            Value::Bytes(b"list".to_vec()),
            Value::List(vec![Value::Str("a".into())]),
        )]))
        .unwrap();
        let path = temp_file(name, &bytes);
        let state = AppState::new();
        open_file(&state, path.to_str().unwrap()).unwrap();
        (state, path)
    }

    #[test]
    fn mutate_then_save_round_trips_through_disk() {
        let (state, path) = open_sample("mutsave");
        let tree = apply_mutation(
            &state,
            &Mutation::SetScalar {
                path: vec![Step::DictValue(0), Step::List(0)],
                text: "edited".into(),
            },
        )
        .unwrap();
        assert_eq!(tree.children[0].children[0].display, "\"edited\"");
        let report = save_document(&state, false).unwrap();
        assert!(report.backup_path.exists());
        // Re-open from disk in a fresh state: the edit persisted, Editable.
        let state2 = AppState::new();
        match open_file(&state2, path.to_str().unwrap()).unwrap() {
            OpenOutcome::Opened { fidelity, tree, .. } => {
                assert_eq!(fidelity, Fidelity::Editable);
                assert_eq!(tree.children[0].children[0].display, "\"edited\"");
            }
            _ => panic!("expected Opened"),
        }
    }

    #[test]
    fn save_conflict_surfaces_the_conflict_code() {
        let (state, path) = open_sample("conflict");
        fs::write(&path, encode(&Value::Dict(vec![])).unwrap()).unwrap();
        let err = save_document(&state, false).unwrap_err();
        assert_eq!(err.code, "conflict");
        save_document(&state, true).unwrap();
    }

    #[test]
    fn backups_list_and_restore_reopen() {
        let (state, _path) = open_sample("backups");
        apply_mutation(
            &state,
            &Mutation::SetScalar {
                path: vec![Step::DictValue(0), Step::List(0)],
                text: "v2".into(),
            },
        )
        .unwrap();
        save_document(&state, false).unwrap();
        let backups = list_file_backups(&state).unwrap();
        assert_eq!(backups.len(), 1, "the pre-save backup");
        // Restore the original -> the reopened tree shows "a" again.
        match restore_backup(&state, backups[0].path.to_str().unwrap()).unwrap() {
            OpenOutcome::Opened { tree, .. } => {
                assert_eq!(tree.children[0].children[0].display, "\"a\"");
            }
            _ => panic!("expected Opened"),
        }
        // Restore itself took a pre-restore backup.
        assert_eq!(list_file_backups(&state).unwrap().len(), 2);
    }

    #[test]
    fn mutation_errors_carry_their_code() {
        let (state, _path) = open_sample("badmut");
        let err = apply_mutation(
            &state,
            &Mutation::SetScalar { path: vec![], text: "5".into() },
        )
        .unwrap_err();
        assert_eq!(err.code, "not_scalar");
    }
}
