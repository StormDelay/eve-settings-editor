//! Command logic as plain functions over `AppState`, so it unit-tests
//! without a Tauri runtime. The `#[tauri::command]` wrappers in lib.rs are
//! one-liners delegating here.

use std::fs;
use std::path::Path;
use std::sync::Mutex;

use serde::Serialize;
use settings_model::{
    apply, default_roots, discover, project, save, window_layout as project_window_layout,
    Document, Fidelity, LoadError, Mutation, Node, Profile, SaveReport, WindowLayout,
};

use crate::accounts;

/// One document open at a time (V1), plus a transient guided-capture baseline.
pub struct AppState(pub Mutex<Option<Document>>, pub Mutex<Option<accounts::Snapshot>>);

impl AppState {
    pub fn new() -> Self {
        AppState(Mutex::new(None), Mutex::new(None))
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

pub fn window_layout(state: &AppState) -> Result<WindowLayout, ErrDto> {
    let guard = state.0.lock().unwrap();
    let doc = guard.as_ref().ok_or_else(|| ErrDto::new("no_document", "no file open"))?;
    Ok(project_window_layout(&doc.value))
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

use std::path::PathBuf;

/// Snapshot current file mtimes as the guided-capture baseline, excluding the
/// currently-open document (the app itself may write it).
pub fn begin_capture(state: &AppState, roots: &[PathBuf]) {
    let open_path = state.0.lock().unwrap().as_ref().map(|d| d.path.clone());
    let profiles = discover(roots);
    let snap = accounts::snapshot_from_profiles(&profiles, open_path.as_deref());
    *state.1.lock().unwrap() = Some(snap);
}

/// Diff the current files against the capture baseline (empty if none set).
/// Excludes the currently-open document from the "after" snapshot too, so it
/// never enters the diff (symmetric with `begin_capture`'s baseline exclusion).
pub fn resolve_capture(state: &AppState, roots: &[PathBuf]) -> accounts::CaptureResult {
    let open_path = state.0.lock().unwrap().as_ref().map(|d| d.path.clone());
    let baseline = state.1.lock().unwrap().clone().unwrap_or_default();
    let profiles = discover(roots);
    let after = accounts::snapshot_from_profiles(&profiles, open_path.as_deref());
    accounts::capture_diff(&baseline, &after)
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::{encode, Value};

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

    #[test]
    fn window_layout_reads_the_open_document() {
        // A minimal char-style file: one open window with geometry.
        let doc = Value::Dict(vec![(
            Value::Bytes(b"windows".to_vec()),
            Value::Dict(vec![
                (
                    Value::Bytes(b"windowSizesAndPositions_1".to_vec()),
                    Value::Tuple(vec![
                        Value::Long(vec![0u8; 8]),
                        Value::Dict(vec![(
                            Value::Bytes(b"overview".to_vec()),
                            Value::Tuple(vec![
                                Value::Int(1), Value::Int(2), Value::Int(3),
                                Value::Int(4), Value::Int(2560), Value::Int(1440),
                            ]),
                        )]),
                    ]),
                ),
                (
                    Value::Bytes(b"openWindows".to_vec()),
                    Value::Tuple(vec![
                        Value::Long(vec![0u8; 8]),
                        Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Bool(true))]),
                    ]),
                ),
            ]),
        )]);
        let path = temp_file("winlayout", &encode(&doc).unwrap());
        let state = AppState::new();
        open_file(&state, path.to_str().unwrap()).unwrap();

        let wl = window_layout(&state).unwrap();
        assert_eq!((wl.reference_w, wl.reference_h), (2560, 1440));
        assert_eq!(wl.windows.len(), 1);
        assert_eq!(wl.windows[0].id, "overview");
        assert!(wl.windows[0].open);
    }

    #[test]
    fn window_layout_without_a_document_errors() {
        let state = AppState::new();
        assert_eq!(window_layout(&state).unwrap_err().code, "no_document");
    }

    #[test]
    fn capture_detects_a_user_file_touched_after_baseline() {
        // A temp discovery tree with one char + one user file.
        let root = std::env::temp_dir().join(format!("app-cap-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let sdir = root.join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        fs::create_dir_all(&sdir).unwrap();
        let cf = sdir.join("core_char_90000001.dat");
        let uf = sdir.join("core_user_987654.dat");
        fs::write(&cf, b"x").unwrap();
        fs::write(&uf, b"x").unwrap();

        let state = AppState::new();
        begin_capture(&state, &[root.clone()]);
        // Advance both mtimes (rewrite the files a moment later).
        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(&cf, b"xy").unwrap();
        fs::write(&uf, b"xy").unwrap();

        let r = resolve_capture(&state, &[root]);
        assert_eq!(r.detected, Some((90000001, 987654)));
    }

    #[test]
    fn resolve_capture_excludes_the_open_document_even_if_its_mtime_advances() {
        // A temp discovery tree with one char (to be opened) + one user file.
        let root = std::env::temp_dir().join(format!("app-cap-open-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let sdir = root.join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        fs::create_dir_all(&sdir).unwrap();
        let cf = sdir.join("core_char_90000001.dat");
        let uf = sdir.join("core_user_987654.dat");
        fs::write(&cf, encode(&Value::Int(1)).unwrap()).unwrap();
        fs::write(&uf, b"x").unwrap();

        let state = AppState::new();
        open_file(&state, cf.to_str().unwrap()).unwrap();
        begin_capture(&state, &[root.clone()]);

        // Advance both mtimes (rewrite the files a moment later). The char
        // file isn't re-opened, so this simulates the app rewriting it while
        // the user's own file also gets touched.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(&cf, b"y").unwrap();
        fs::write(&uf, b"xy").unwrap();

        let r = resolve_capture(&state, &[root]);
        assert!(
            r.changed_chars.is_empty(),
            "the open char file is excluded even though its mtime advanced"
        );
        assert_eq!(r.changed_users, vec![987654]);
        assert_eq!(r.detected, None);
    }
}
