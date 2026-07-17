//! Command logic as plain functions over `AppState`, so it unit-tests
//! without a Tauri runtime. The `#[tauri::command]` wrappers in lib.rs are
//! one-liners delegating here.

use std::fs;
use std::path::Path;
use std::sync::Mutex;

use serde::Serialize;
use settings_model::{
    apply, default_roots, discover, project, project_overview, save,
    set_column_order, set_column_visible, set_column_width,
    window_layout as project_window_layout,
    clear_all_history, project_edit_history, set_list_entries, AutofillError, RememberedList,
    Document, FileKind, Fidelity, LoadError, Mutation, Node, OverviewColumns, Profile, SaveReport,
    WindowLayout,
    apply_categories_to, extract_categories, file_kind, full_copy_to, Category,
};

use crate::accounts;

/// Two open documents (char + user, for the two-file overview category) plus a
/// transient guided-capture baseline. Each document keeps its own save chain.
pub struct AppState {
    pub char: Mutex<Option<Document>>,
    pub user: Mutex<Option<Document>>,
    pub capture: Mutex<Option<accounts::Snapshot>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState { char: Mutex::new(None), user: Mutex::new(None), capture: Mutex::new(None) }
    }
    fn doc(&self, slot: Slot) -> &Mutex<Option<Document>> {
        match slot {
            Slot::Char => &self.char,
            Slot::User => &self.user,
        }
    }
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    Char,
    User,
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
pub struct Candidate {
    pub path: String,
    pub file_name: String,
    pub id: Option<u64>,
    pub folder: String,
    pub same_folder: bool,
}

/// Discovered files eligible as batch targets for `source_path`: same file type,
/// and (unless `allow_other_folders`) the source's own profile folder. The source
/// itself is never a candidate.
pub fn batch_targets(roots: &[PathBuf], source_path: &str, allow_other_folders: bool) -> Vec<Candidate> {
    let profiles = discover(roots);
    let src = Path::new(source_path);
    let mut src_kind: Option<&FileKind> = None;
    let mut src_dir: Option<PathBuf> = None;
    for p in &profiles {
        for f in &p.files {
            if f.path == src {
                src_kind = Some(&f.kind);
                src_dir = Some(p.dir.clone());
            }
        }
    }
    let Some(kind) = src_kind else { return Vec::new() };
    let mut out = Vec::new();
    for p in &profiles {
        let same = Some(&p.dir) == src_dir.as_ref();
        if !same && !allow_other_folders {
            continue;
        }
        for f in &p.files {
            if &f.kind != kind || f.path == src {
                continue;
            }
            out.push(Candidate {
                path: f.path.to_string_lossy().into_owned(),
                file_name: f.file_name.clone(),
                id: f.id,
                folder: format!("{}/{}", p.server, p.profile),
                same_folder: same,
            });
        }
    }
    out
}

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BatchOp {
    FullCopy,
    Categories { categories: Vec<Category> },
}

#[derive(Debug, Serialize)]
pub struct TargetResult {
    pub path: String,
    pub ok: bool,
    pub backup_path: Option<String>,
    pub error: Option<String>,
}

/// Apply `op` from `source_path` to each target. The source is read (and, for a
/// category copy, decoded and extracted) exactly once. One target's failure is
/// recorded and never halts the rest. A source that cannot be read/decoded, or
/// whose selected categories are entirely absent from the source, fails the
/// whole op up front, before any target is touched. Each target's kind is
/// re-derived here (never trusted from the caller, which is a Tauri command
/// boundary) and a mismatch is refused as that target's own failure, without
/// touching the file.
pub fn batch_apply(source_path: &str, op: BatchOp, targets: &[String]) -> Result<Vec<TargetResult>, ErrDto> {
    let source = Path::new(source_path);
    let bytes = fs::read(source).map_err(|e| ErrDto::new("io", e.to_string()))?;
    let src_kind = file_kind(source);
    match op {
        BatchOp::FullCopy => Ok(targets
            .iter()
            .map(|t| {
                if let Some(r) = kind_mismatch(t, &src_kind) {
                    return r;
                }
                match full_copy_to(&bytes, Path::new(t)) {
                    Ok(bk) => ok_result(t, bk.to_string_lossy().into_owned()),
                    Err(e) => err_result(t, e),
                }
            })
            .collect()),
        BatchOp::Categories { categories } => {
            let value = blue_marshal::decode(&bytes).map_err(|e| ErrDto::new("decode", e.to_string()))?;
            let extracted = extract_categories(&value, &categories);
            if extracted.is_empty() {
                return Err(ErrDto::new("no_categories", "the source file has none of the selected categories"));
            }
            Ok(targets
                .iter()
                .map(|t| {
                    if let Some(r) = kind_mismatch(t, &src_kind) {
                        return r;
                    }
                    match apply_categories_to(Path::new(t), &extracted) {
                        Ok(r) => ok_result(t, r.backup_path.to_string_lossy().into_owned()),
                        Err(e) => err_result(t, e),
                    }
                })
                .collect())
        }
    }
}

/// The write path must never trust the caller's target list: a target of a
/// different kind than the source is refused before any write (spec §6 — the
/// type match is enforced regardless of the folder toggle). Shared by both
/// `BatchOp` arms so they cannot drift on the comparison or its message.
fn kind_mismatch(t: &str, src_kind: &FileKind) -> Option<TargetResult> {
    let tkind = file_kind(Path::new(t));
    (&tkind != src_kind).then(|| {
        err_result(t, format!("target is {tkind:?} but source is {src_kind:?} (type mismatch)"))
    })
}

fn ok_result(path: &str, backup: String) -> TargetResult {
    TargetResult { path: path.to_string(), ok: true, backup_path: Some(backup), error: None }
}
fn err_result(path: &str, error: String) -> TargetResult {
    TargetResult { path: path.to_string(), ok: false, backup_path: None, error: Some(error) }
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

pub fn open_file(state: &AppState, slot: Slot, path: &str) -> Result<OpenOutcome, ErrDto> {
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
            *state.doc(slot).lock().unwrap() = Some(doc);
            Ok(outcome)
        }
        Err(LoadError::Decode { offset, message }) => {
            let bytes = fs::read(p).map_err(|e| ErrDto::new("io", e.to_string()))?;
            *state.doc(slot).lock().unwrap() = None;
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

pub fn close_file(state: &AppState, slot: Slot) {
    *state.doc(slot).lock().unwrap() = None;
}

pub fn apply_mutation(state: &AppState, slot: Slot, mutation: &Mutation) -> Result<Node, ErrDto> {
    let mut guard = state.doc(slot).lock().unwrap();
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

pub fn save_document(state: &AppState, slot: Slot, force: bool) -> Result<SaveReport, ErrDto> {
    let mut guard = state.doc(slot).lock().unwrap();
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

pub fn list_file_backups(state: &AppState, slot: Slot) -> Result<Vec<settings_model::BackupInfo>, ErrDto> {
    let guard = state.doc(slot).lock().unwrap();
    let doc = guard.as_ref().ok_or_else(|| ErrDto::new("no_document", "no file open"))?;
    Ok(settings_model::list_backups(&doc.path))
}

pub fn window_layout(state: &AppState, slot: Slot) -> Result<WindowLayout, ErrDto> {
    let guard = state.doc(slot).lock().unwrap();
    let doc = guard.as_ref().ok_or_else(|| ErrDto::new("no_document", "no file open"))?;
    Ok(project_window_layout(&doc.value))
}

pub fn restore_backup(state: &AppState, slot: Slot, backup_path: &str) -> Result<OpenOutcome, ErrDto> {
    let target = {
        let guard = state.doc(slot).lock().unwrap();
        let doc = guard.as_ref().ok_or_else(|| ErrDto::new("no_document", "no file open"))?;
        doc.path.clone()
    };
    settings_model::restore(Path::new(backup_path), &target)
        .map_err(|e| ErrDto::new("restore", e))?;
    // Re-open so the UI reflects the restored content and a fresh baseline.
    open_file(state, slot, &target.to_string_lossy())
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

/// Snapshot current file mtimes as the guided-capture baseline, excluding
/// both open documents (the app itself may write them).
pub fn begin_capture(state: &AppState, roots: &[PathBuf]) {
    let profiles = discover(roots);
    let mut snap = accounts::snapshot_from_profiles(&profiles, None);
    for p in open_paths(state) {
        snap.remove(&p);
    }
    *state.capture.lock().unwrap() = Some(snap);
}

/// Diff the current files against the capture baseline (empty if none set).
/// Excludes both open documents from the "after" snapshot too, so they never
/// enter the diff (symmetric with `begin_capture`'s baseline exclusion).
pub fn resolve_capture(state: &AppState, roots: &[PathBuf]) -> accounts::CaptureResult {
    let baseline = state.capture.lock().unwrap().clone().unwrap_or_default();
    let profiles = discover(roots);
    let mut after = accounts::snapshot_from_profiles(&profiles, None);
    for p in open_paths(state) {
        after.remove(&p);
    }
    accounts::capture_diff(&baseline, &after)
}

/// Paths of whatever documents are open (either slot) — excluded from capture
/// diffs since the app itself may write them.
fn open_paths(state: &AppState) -> Vec<PathBuf> {
    [Slot::Char, Slot::User]
        .into_iter()
        .filter_map(|s| state.doc(s).lock().unwrap().as_ref().map(|d| d.path.clone()))
        .collect()
}

pub fn overview_columns(state: &AppState) -> Result<OverviewColumns, ErrDto> {
    let user = state.user.lock().unwrap();
    let udoc = user.as_ref().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
    let char_guard = state.char.lock().unwrap();
    let char_val = char_guard.as_ref().map(|d| &d.value);
    Ok(project_overview(&udoc.value, char_val))
}

/// Edit the user slot (visibility/order), then re-project including char widths.
fn edit_user_overview<F>(state: &AppState, edit: F) -> Result<OverviewColumns, ErrDto>
where
    F: FnOnce(&mut blue_marshal::Value) -> Result<(), settings_model::OverviewError>,
{
    {
        let mut guard = state.user.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        edit(&mut doc.value).map_err(|e| ErrDto::new("overview", format!("{e:?}")))?;
    }
    overview_columns(state)
}

pub fn set_overview_visible(state: &AppState, tab_index: i64, column: &str, visible: bool) -> Result<OverviewColumns, ErrDto> {
    edit_user_overview(state, |v| set_column_visible(v, tab_index, column, visible))
}

pub fn set_overview_order(state: &AppState, tab_index: i64, order: Vec<String>) -> Result<OverviewColumns, ErrDto> {
    edit_user_overview(state, |v| set_column_order(v, tab_index, &order))
}

pub fn set_overview_width(state: &AppState, tab_index: i64, column: &str, width: i64) -> Result<OverviewColumns, ErrDto> {
    {
        let mut guard = state.char.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no character file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        set_column_width(&mut doc.value, tab_index, column, width)
            .map_err(|e| ErrDto::new("overview", format!("{e:?}")))?;
    }
    overview_columns(state)
}

pub fn autofill_lists(state: &AppState) -> Result<Vec<RememberedList>, ErrDto> {
    let user = state.user.lock().unwrap();
    let udoc = user.as_ref().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
    Ok(project_edit_history(&udoc.value))
}

/// Edit the user slot's editHistory, then re-project.
fn edit_user_autofill<F>(state: &AppState, edit: F) -> Result<Vec<RememberedList>, ErrDto>
where
    F: FnOnce(&mut blue_marshal::Value) -> Result<(), AutofillError>,
{
    {
        let mut guard = state.user.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        edit(&mut doc.value).map_err(|e| ErrDto::new("autofill", format!("{e:?}")))?;
    }
    autofill_lists(state)
}

pub fn set_autofill_list(state: &AppState, widget: &str, entries: Vec<String>) -> Result<Vec<RememberedList>, ErrDto> {
    edit_user_autofill(state, |v| set_list_entries(v, widget, &entries))
}

pub fn clear_all_autofill(state: &AppState) -> Result<Vec<RememberedList>, ErrDto> {
    edit_user_autofill(state, |v| clear_all_history(v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::{decode, encode, Value};

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
        let outcome = open_file(&state, Slot::Char, path.to_str().unwrap()).unwrap();
        match outcome {
            OpenOutcome::Opened { fidelity, tree, file_name, .. } => {
                assert_eq!(fidelity, Fidelity::Editable);
                assert_eq!(file_name, "core_user_5.dat");
                assert_eq!(tree.kind, "dict");
            }
            _ => panic!("expected Opened"),
        }
        assert!(state.char.lock().unwrap().is_some());
        close_file(&state, Slot::Char);
        assert!(state.char.lock().unwrap().is_none());
    }

    #[test]
    fn open_undecodable_file_returns_hex_preview() {
        let path = temp_file("bad", &[0x7E, 0, 0, 0, 0, 0x3D]);
        let state = AppState::new();
        match open_file(&state, Slot::Char, path.to_str().unwrap()).unwrap() {
            OpenOutcome::ParseFailed { offset, hex_preview, .. } => {
                assert_eq!(offset, 5);
                assert!(hex_preview.starts_with("00000000  7e 00 00 00 00 3d"));
            }
            _ => panic!("expected ParseFailed"),
        }
        assert!(state.char.lock().unwrap().is_none());
    }

    #[test]
    fn open_missing_file_is_an_io_error() {
        let state = AppState::new();
        let err = open_file(&state, Slot::Char, "Z:/no/such/file.dat").unwrap_err();
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
        open_file(&state, Slot::Char, path.to_str().unwrap()).unwrap();
        (state, path)
    }

    #[test]
    fn mutate_then_save_round_trips_through_disk() {
        let (state, path) = open_sample("mutsave");
        let tree = apply_mutation(
            &state,
            Slot::Char,
            &Mutation::SetScalar {
                path: vec![Step::DictValue(0), Step::List(0)],
                text: "edited".into(),
            },
        )
        .unwrap();
        assert_eq!(tree.children[0].children[0].display, "\"edited\"");
        let report = save_document(&state, Slot::Char, false).unwrap();
        assert!(report.backup_path.exists());
        // Re-open from disk in a fresh state: the edit persisted, Editable.
        let state2 = AppState::new();
        match open_file(&state2, Slot::Char, path.to_str().unwrap()).unwrap() {
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
        let err = save_document(&state, Slot::Char, false).unwrap_err();
        assert_eq!(err.code, "conflict");
        save_document(&state, Slot::Char, true).unwrap();
    }

    #[test]
    fn backups_list_and_restore_reopen() {
        let (state, _path) = open_sample("backups");
        apply_mutation(
            &state,
            Slot::Char,
            &Mutation::SetScalar {
                path: vec![Step::DictValue(0), Step::List(0)],
                text: "v2".into(),
            },
        )
        .unwrap();
        save_document(&state, Slot::Char, false).unwrap();
        let backups = list_file_backups(&state, Slot::Char).unwrap();
        assert_eq!(backups.len(), 1, "the pre-save backup");
        // Restore the original -> the reopened tree shows "a" again.
        match restore_backup(&state, Slot::Char, backups[0].path.to_str().unwrap()).unwrap() {
            OpenOutcome::Opened { tree, .. } => {
                assert_eq!(tree.children[0].children[0].display, "\"a\"");
            }
            _ => panic!("expected Opened"),
        }
        // Restore itself took a pre-restore backup.
        assert_eq!(list_file_backups(&state, Slot::Char).unwrap().len(), 2);
    }

    #[test]
    fn mutation_errors_carry_their_code() {
        let (state, _path) = open_sample("badmut");
        let err = apply_mutation(
            &state,
            Slot::Char,
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
        open_file(&state, Slot::Char, path.to_str().unwrap()).unwrap();

        let wl = window_layout(&state, Slot::Char).unwrap();
        assert_eq!((wl.reference_w, wl.reference_h), (2560, 1440));
        assert_eq!(wl.windows.len(), 1);
        assert_eq!(wl.windows[0].id, "overview");
        assert!(wl.windows[0].open);
    }

    #[test]
    fn window_layout_without_a_document_errors() {
        let state = AppState::new();
        assert_eq!(window_layout(&state, Slot::Char).unwrap_err().code, "no_document");
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
        open_file(&state, Slot::Char, cf.to_str().unwrap()).unwrap();
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

    #[test]
    fn two_slots_hold_independent_documents() {
        let ubytes = encode(&Value::Dict(vec![(Value::Bytes(b"u".to_vec()), Value::Int(1))])).unwrap();
        let cbytes = encode(&Value::Dict(vec![(Value::Bytes(b"c".to_vec()), Value::Int(2))])).unwrap();
        let upath = temp_file("slot-user", &ubytes);
        let cpath = temp_file("slot-char", &cbytes);
        let state = AppState::new();
        open_file(&state, Slot::User, upath.to_str().unwrap()).unwrap();
        open_file(&state, Slot::Char, cpath.to_str().unwrap()).unwrap();
        assert!(state.user.lock().unwrap().is_some());
        assert!(state.char.lock().unwrap().is_some());
        // Closing one leaves the other.
        close_file(&state, Slot::User);
        assert!(state.user.lock().unwrap().is_none());
        assert!(state.char.lock().unwrap().is_some());
    }

    fn overview_user_bytes() -> Vec<u8> {
        // root -> b"overview" -> b"tabsettings_new" -> (ts, { 0: {name, order, visible} })
        let tab = Value::Dict(vec![
            (Value::Str("name".into()), Value::Str("PvP".into())),
            (Value::Bytes(b"tabColumnOrder".to_vec()),
             Value::List(vec![Value::Bytes(b"NAME".to_vec()), Value::Bytes(b"TYPE".to_vec())])),
            (Value::Bytes(b"tabColumns".to_vec()), Value::List(vec![Value::Bytes(b"NAME".to_vec())])),
        ]);
        encode(&Value::Dict(vec![(
            Value::Bytes(b"overview".to_vec()),
            Value::Dict(vec![(
                Value::Bytes(b"tabsettings_new".to_vec()),
                Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Dict(vec![(Value::Int(0), tab)])]),
            )]),
        )])).unwrap()
    }

    #[test]
    fn overview_reads_and_edits_the_user_slot() {
        let path = temp_file("ov-user", &overview_user_bytes());
        let state = AppState::new();
        open_file(&state, Slot::User, path.to_str().unwrap()).unwrap();

        let oc = overview_columns(&state).unwrap();
        assert_eq!(oc.tabs.len(), 1);
        assert_eq!(oc.tabs[0].columns.iter().filter(|c| c.visible).count(), 1);

        // Show TYPE, then reorder.
        let oc = set_overview_visible(&state, 0, "TYPE", true).unwrap();
        assert_eq!(oc.tabs[0].columns.iter().filter(|c| c.visible).count(), 2);
        let oc = set_overview_order(&state, 0, vec!["TYPE".into(), "NAME".into()]).unwrap();
        assert_eq!(oc.tabs[0].columns[0].name, "TYPE");
    }

    #[test]
    fn overview_without_a_user_slot_errors() {
        let state = AppState::new();
        assert_eq!(overview_columns(&state).unwrap_err().code, "no_document");
    }

    fn autofill_user_bytes() -> Vec<u8> {
        // root -> b"ui" -> b"editHistory" -> (ts, { "/a/box": ["Jita", "Amarr"] })
        let hist = Value::Dict(vec![(
            Value::Bytes(b"/a/box".to_vec()),
            Value::List(vec![Value::Str("Jita".into()), Value::Str("Amarr".into())]),
        )]);
        let ui = Value::Dict(vec![(
            Value::Bytes(b"editHistory".to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), hist]),
        )]);
        encode(&Value::Dict(vec![(Value::Bytes(b"ui".to_vec()), ui)])).unwrap()
    }

    #[test]
    fn autofill_reads_edits_and_clears_the_user_slot() {
        let path = temp_file("af-user", &autofill_user_bytes());
        let state = AppState::new();
        open_file(&state, Slot::User, path.to_str().unwrap()).unwrap();

        let lists = autofill_lists(&state).unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].entries, vec!["Jita", "Amarr"]);

        let lists = set_autofill_list(&state, "/a/box", vec!["Dodixie".into()]).unwrap();
        assert_eq!(lists[0].entries, vec!["Dodixie"]);

        let lists = clear_all_autofill(&state).unwrap();
        assert!(lists[0].entries.is_empty(), "list emptied, widget kept");
    }

    #[test]
    fn autofill_without_a_user_slot_errors() {
        let state = AppState::new();
        assert_eq!(autofill_lists(&state).unwrap_err().code, "no_document");
    }

    fn discovery_tree() -> PathBuf {
        // Two profile folders, each with char + user files.
        let root = std::env::temp_dir().join(format!("app-batchtargets-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let tq = root.join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        let sisi = root.join("c_eve_sharedcache_sisi_singularity").join("settings_Default");
        fs::create_dir_all(&tq).unwrap();
        fs::create_dir_all(&sisi).unwrap();
        for p in [&tq, &sisi] {
            fs::write(p.join("core_char_90000001.dat"), b"x").unwrap();
            fs::write(p.join("core_char_90000002.dat"), b"x").unwrap();
            fs::write(p.join("core_user_987654.dat"), b"x").unwrap();
        }
        root
    }

    #[test]
    fn batch_targets_same_folder_same_type_excludes_source() {
        let root = discovery_tree();
        let src = root
            .join("c_eve_sharedcache_tq_tranquility/settings_Default/core_char_90000001.dat");
        let out = batch_targets(&[root], src.to_str().unwrap(), false);
        // Only the OTHER char in the same folder — not the user file, not sisi, not itself.
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].file_name, "core_char_90000002.dat");
        assert!(out[0].same_folder);
    }

    #[test]
    fn batch_targets_allow_other_folders_adds_matching_type_elsewhere() {
        let root = discovery_tree();
        let src = root
            .join("c_eve_sharedcache_tq_tranquility/settings_Default/core_char_90000001.dat");
        let out = batch_targets(&[root], src.to_str().unwrap(), true);
        // Same-folder other char + both chars in sisi = 3.
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|c| c.file_name.starts_with("core_char_")));
        assert!(out.iter().any(|c| !c.same_folder), "cross-folder candidate present");
    }

    fn af_bytes(widget: &str, entry: &str) -> Vec<u8> {
        let hist = Value::Dict(vec![(
            Value::Bytes(widget.as_bytes().to_vec()),
            Value::List(vec![Value::Str(entry.into())]),
        )]);
        let ui = Value::Dict(vec![(
            Value::Bytes(b"editHistory".to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), hist]),
        )]);
        encode(&Value::Dict(vec![(Value::Bytes(b"ui".to_vec()), ui)])).unwrap()
    }

    #[test]
    fn batch_apply_categories_reports_per_target_including_a_read_only_failure() {
        let dir = std::env::temp_dir().join(format!("app-batchapply-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("core_user_1.dat");
        let good = dir.join("core_user_2.dat");
        let bad = dir.join("core_user_3.dat");
        fs::write(&src, af_bytes("/from_source", "Jita")).unwrap();
        fs::write(&good, af_bytes("/old", "Amarr")).unwrap();
        fs::write(&bad, [0x7E, 0, 0, 0, 0, 0x06, 0x01]).unwrap(); // non-canonical -> ReadOnly

        let op = BatchOp::Categories { categories: vec![Category::Autofill] };
        // `bad` comes FIRST and `good` comes AFTER it: a break-on-failure
        // regression would leave `good` unprocessed, so this ordering is what
        // makes the test able to catch that regression (a [good, bad] order
        // would look identical whether or not the batch halted on `bad`).
        let bad_path = bad.to_string_lossy().into_owned();
        let good_path = good.to_string_lossy().into_owned();
        let targets = vec![bad_path.clone(), good_path.clone()];
        let results = batch_apply(src.to_str().unwrap(), op, &targets).unwrap();

        assert_eq!(results.len(), 2);
        let bd = results.iter().find(|r| r.path == bad_path).unwrap();
        assert!(!bd.ok && bd.error.is_some(), "read-only target failed but did not halt the batch");
        let g = results.iter().find(|r| r.path == good_path).unwrap();
        assert!(g.ok && g.backup_path.is_some());

        // The good target — processed AFTER the failing one — actually
        // received the source's category; a break-on-failure regression
        // would never reach it.
        let reread = decode(&fs::read(&good).unwrap()).unwrap();
        assert_eq!(project_edit_history(&reread)[0].widget, "/from_source");
    }

    #[test]
    fn batch_apply_full_copy_makes_targets_byte_identical() {
        let dir = std::env::temp_dir().join(format!("app-batchfull-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("core_char_1.dat");
        let dst = dir.join("core_char_2.dat");
        let src_bytes = af_bytes("/x", "y");
        fs::write(&src, &src_bytes).unwrap();
        fs::write(&dst, af_bytes("/other", "z")).unwrap();

        let results = batch_apply(src.to_str().unwrap(), BatchOp::FullCopy, &[dst.to_string_lossy().into_owned()]).unwrap();
        assert!(results[0].ok);
        assert_eq!(fs::read(&dst).unwrap(), src_bytes);
    }

    #[test]
    fn batch_apply_categories_aborts_when_source_lacks_the_category() {
        let dir = std::env::temp_dir().join(format!("app-batchnocat-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("core_user_1.dat");
        let target = dir.join("core_user_2.dat");
        // Source has no `ui`/`editHistory` at all.
        let src_bytes =
            encode(&Value::Dict(vec![(Value::Bytes(b"other".to_vec()), Value::Int(1))])).unwrap();
        let target_bytes = af_bytes("/keep", "Amarr");
        fs::write(&src, &src_bytes).unwrap();
        fs::write(&target, &target_bytes).unwrap();

        let err = batch_apply(
            src.to_str().unwrap(),
            BatchOp::Categories { categories: vec![Category::Autofill] },
            &[target.to_string_lossy().into_owned()],
        )
        .unwrap_err();
        assert_eq!(err.code, "no_categories");

        // Byte-identical: no pointless de-dup/re-encode rewrite, and no backup.
        assert_eq!(
            fs::read(&target).unwrap(),
            target_bytes,
            "target must be untouched when the source lacks the selected category"
        );
        assert!(
            !dir.join("eve-settings-editor-backups").exists(),
            "no backup should have been created"
        );
    }

    #[test]
    fn batch_apply_full_copy_refuses_a_mismatched_target_kind() {
        let dir = std::env::temp_dir().join(format!("app-batchkindmismatch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("core_char_1.dat"); // char source
        let target = dir.join("core_user_2.dat"); // user target: mismatched kind
        let src_bytes = af_bytes("/x", "y");
        let target_bytes = af_bytes("/other", "z");
        fs::write(&src, &src_bytes).unwrap();
        fs::write(&target, &target_bytes).unwrap();

        let results = batch_apply(
            src.to_str().unwrap(),
            BatchOp::FullCopy,
            &[target.to_string_lossy().into_owned()],
        )
        .unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].ok, "mismatched-kind target is refused, not written");
        assert!(results[0].error.is_some());
        assert_eq!(
            fs::read(&target).unwrap(),
            target_bytes,
            "target bytes unchanged: the write path never trusted the caller's kind"
        );
    }

    #[test]
    fn batch_apply_categories_refuses_a_mismatched_target_kind() {
        let dir = std::env::temp_dir().join(format!("app-batchkindmismatch-cat-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("core_char_1.dat"); // char source, content has the autofill category
        let target = dir.join("core_user_2.dat"); // user target: mismatched kind
        let src_bytes = af_bytes("/x", "y");
        let target_bytes = af_bytes("/other", "z");
        fs::write(&src, &src_bytes).unwrap();
        fs::write(&target, &target_bytes).unwrap();

        let results = batch_apply(
            src.to_str().unwrap(),
            BatchOp::Categories { categories: vec![Category::Autofill] },
            &[target.to_string_lossy().into_owned()],
        )
        .unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].ok, "mismatched-kind target is refused, not written");
        assert!(results[0].error.is_some());
        assert_eq!(
            fs::read(&target).unwrap(),
            target_bytes,
            "target bytes unchanged: the write path never trusted the caller's kind"
        );
    }

    #[test]
    fn batch_apply_undecodable_source_fails_the_whole_op() {
        let dir = std::env::temp_dir().join(format!("app-batchbadsrc-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("core_user_1.dat");
        fs::write(&src, [0xFF, 0xFF]).unwrap(); // undecodable
        let target = dir.join("core_user_2.dat");
        let target_bytes = af_bytes("/untouched", "Dodixie");
        fs::write(&target, &target_bytes).unwrap();

        let err = batch_apply(
            src.to_str().unwrap(),
            BatchOp::Categories { categories: vec![Category::Autofill] },
            &[target.to_string_lossy().into_owned()],
        )
        .unwrap_err();
        assert_eq!(err.code, "decode");

        // The target must be byte-identical to before the call: this proves
        // the op aborted before touching any target, not merely that it
        // surfaced an error (an empty target list would prove that trivially).
        assert_eq!(
            fs::read(&target).unwrap(),
            target_bytes,
            "target must be untouched when the source fails to decode"
        );
        assert!(
            !dir.join("eve-settings-editor-backups").exists(),
            "no backup should have been created"
        );
    }
}
