//! Command logic as plain functions over `AppState`, so it unit-tests
//! without a Tauri runtime. The `#[tauri::command]` wrappers in lib.rs are
//! one-liners delegating here.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use settings_model::{
    apply, default_roots, discover, project, project_overview, save,
    set_column_order, set_column_visible, set_column_width,
    window_layout as project_window_layout,
    clear_all_history, project_edit_history, set_list_entries, AutofillError, RememberedList,
    Document, FileKind, Fidelity, LoadError, Mutation, Node, OverviewColumns, Profile, SaveReport,
    WindowLayout,
    apply_categories_to, extract_categories, full_copy_to, Category,
    unstack, add_to_stack, reorder_stack, create_stack, StackError,
    create_tab, rename_tab, delete_tab, reorder_tabs_in_window, move_tab, OverviewTabError,
    add_overview_window, remove_overview_window, add_overview_window_geometry, remove_overview_window_geometry,
    create_preset,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aspect {
    Layout,
    Overview,
    Autofill,
    Everything,
}

/// What a chosen set of aspects writes, split by file side. Pure derivation of
/// the single routing table (plan header): the char file, the account file, or
/// both — as subtree splices or a whole-file copy (`Everything`).
#[derive(Debug, Clone, PartialEq)]
pub struct AspectWrites {
    pub char_categories: Vec<Category>,
    pub account_categories: Vec<Category>,
    pub char_full_copy: bool,
    pub account_full_copy: bool,
}

impl AspectWrites {
    pub fn writes_account(&self) -> bool {
        self.account_full_copy || !self.account_categories.is_empty()
    }
    pub fn writes_char(&self) -> bool {
        self.char_full_copy || !self.char_categories.is_empty()
    }
    /// True when the char write copies window geometry (drives the off-screen
    /// resolution warning): a full char copy, or a Layout splice.
    pub fn copies_char_geometry(&self) -> bool {
        self.char_full_copy || self.char_categories.contains(&Category::Layout)
    }
}

pub fn aspect_writes(aspects: &[Aspect]) -> AspectWrites {
    if aspects.contains(&Aspect::Everything) {
        return AspectWrites {
            char_categories: vec![],
            account_categories: vec![],
            char_full_copy: true,
            account_full_copy: true,
        };
    }
    let mut char_categories = vec![];
    let mut account_categories = vec![];
    for a in aspects {
        match a {
            Aspect::Layout => char_categories.push(Category::Layout),
            Aspect::Overview => {
                char_categories.push(Category::OverviewWidths);
                account_categories.push(Category::Overview);
            }
            Aspect::Autofill => account_categories.push(Category::Autofill),
            Aspect::Everything => unreachable!("handled above"),
        }
    }
    AspectWrites { char_categories, account_categories, char_full_copy: false, account_full_copy: false }
}

#[derive(Debug, Default, Serialize, PartialEq)]
pub struct SetupPlan {
    pub char_writes: Vec<CharWrite>,
    pub account_writes: Vec<AccountWrite>,
    pub excluded: Vec<ExcludedTarget>,
    pub source_error: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct CharWrite {
    pub char_id: u64,
    pub path: String,
    pub full_copy: bool,
    pub resolution_mismatch: bool,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct AccountWrite {
    pub user_id: u64,
    pub path: String,
    pub full_copy: bool,
    /// Characters on this account that are NOT selected targets — the write
    /// changes them too.
    pub collateral_char_ids: Vec<u64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ExcludedTarget {
    pub char_id: u64,
    pub reason: String,
}

/// The account (user id) that owns `char_id`, per the persisted pairing.
fn account_of(store: &accounts::AccountsStore, char_id: u64) -> Option<u64> {
    store.accounts.iter().find(|(_, a)| a.characters.contains(&char_id)).map(|(&uid, _)| uid)
}

/// Pure planner. All disk-dependent inputs (discovered file paths, the store,
/// each char's stored screen resolution) are passed in, so this is unit-tested
/// without a filesystem. Paths are already folder-scoped by the caller.
pub fn plan_setup(
    char_paths: &HashMap<u64, PathBuf>,
    user_paths: &HashMap<u64, PathBuf>,
    store: &accounts::AccountsStore,
    resolutions: &HashMap<u64, (i64, i64)>,
    source_char: u64,
    target_chars: &[u64],
    aspects: &[Aspect],
) -> SetupPlan {
    let w = aspect_writes(aspects);
    let mut plan = SetupPlan::default();

    let source_account = account_of(store, source_char);
    if w.writes_account() {
        match source_account {
            None => {
                plan.source_error = Some(
                    "The source character has no paired account — pair it in the Accounts view first."
                        .into(),
                );
                return plan;
            }
            Some(uid) if !user_paths.contains_key(&uid) => {
                plan.source_error = Some("The source character's account file was not found.".into());
                return plan;
            }
            _ => {}
        }
    }
    let src_res = resolutions.get(&source_char).copied();

    let mut included: Vec<u64> = Vec::new();
    for &t in target_chars {
        if t == source_char {
            continue;
        }
        if !char_paths.contains_key(&t) {
            plan.excluded.push(ExcludedTarget { char_id: t, reason: "Character file not found in this folder.".into() });
            continue;
        }
        if w.writes_account() {
            match account_of(store, t) {
                None => {
                    plan.excluded.push(ExcludedTarget { char_id: t, reason: "No account paired — pair it in the Accounts view to include.".into() });
                    continue;
                }
                Some(uid) if !user_paths.contains_key(&uid) => {
                    plan.excluded.push(ExcludedTarget { char_id: t, reason: "Account file not found in this folder.".into() });
                    continue;
                }
                _ => {}
            }
        }
        included.push(t);
    }

    if w.writes_char() {
        for &t in &included {
            let path = char_paths[&t].to_string_lossy().into_owned();
            let resolution_mismatch = w.copies_char_geometry()
                && match (src_res, resolutions.get(&t).copied()) {
                    (Some(s), Some(d)) => s != d && s != (0, 0) && d != (0, 0),
                    _ => false,
                };
            plan.char_writes.push(CharWrite { char_id: t, path, full_copy: w.char_full_copy, resolution_mismatch });
        }
    }

    if w.writes_account() {
        let mut by_account: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        for &t in &included {
            let uid = account_of(store, t).expect("included target is paired");
            by_account.entry(uid).or_default().push(t);
        }
        for (uid, selected_on_acct) in by_account {
            if Some(uid) == source_account {
                continue; // already carries the source's settings
            }
            let path = user_paths[&uid].to_string_lossy().into_owned();
            let selected: HashSet<u64> = selected_on_acct.into_iter().collect();
            let collateral: Vec<u64> = store
                .accounts
                .get(&uid)
                .map(|a| a.characters.iter().copied().filter(|c| !selected.contains(c)).collect())
                .unwrap_or_default();
            plan.account_writes.push(AccountWrite { user_id: uid, path, full_copy: w.account_full_copy, collateral_char_ids: collateral });
        }
    }

    plan
}

/// Discover, folder-scope to the source's profile (unless `allow_other_folders`),
/// and split into char/user id->path maps. Returns the source char's id too.
fn scoped_files(
    roots: &[PathBuf],
    source_char_path: &str,
    allow_other_folders: bool,
) -> Option<(u64, HashMap<u64, PathBuf>, HashMap<u64, PathBuf>)> {
    let profiles = discover(roots);
    let src = Path::new(source_char_path);
    let mut src_id = None;
    let mut src_dir = None;
    for p in &profiles {
        for f in &p.files {
            if f.path == src {
                src_id = f.id;
                src_dir = Some(p.dir.clone());
            }
        }
    }
    let src_id = src_id?;
    let mut char_paths = HashMap::new();
    let mut user_paths = HashMap::new();
    for p in &profiles {
        if !allow_other_folders && Some(&p.dir) != src_dir.as_ref() {
            continue;
        }
        for f in &p.files {
            let Some(id) = f.id else { continue };
            match f.kind {
                FileKind::Char => { char_paths.insert(id, f.path.clone()); }
                FileKind::User => { user_paths.insert(id, f.path.clone()); }
                FileKind::Other => {}
            }
        }
    }
    Some((src_id, char_paths, user_paths))
}

/// Each char's stored screen resolution (reference_w, reference_h), for the
/// resolution-mismatch warning. Only the source + requested targets are read.
fn gather_resolutions(char_paths: &HashMap<u64, PathBuf>, ids: &[u64]) -> HashMap<u64, (i64, i64)> {
    let mut out = HashMap::new();
    for &id in ids {
        let Some(path) = char_paths.get(&id) else { continue };
        let Ok(bytes) = fs::read(path) else { continue };
        let Ok(value) = blue_marshal::decode(&bytes) else { continue };
        let wl = project_window_layout(&value);
        out.insert(id, (wl.reference_w, wl.reference_h));
    }
    out
}

/// Map target file paths to char ids within the scoped char map.
fn target_ids(char_paths: &HashMap<u64, PathBuf>, target_char_paths: &[String]) -> Vec<u64> {
    target_char_paths
        .iter()
        .filter_map(|t| {
            let tp = Path::new(t);
            char_paths.iter().find(|(_, p)| p.as_path() == tp).map(|(&id, _)| id)
        })
        .collect()
}

/// True if decoding `path` and extracting `cats` yields nothing — the source has
/// none of these categories, so a splice would be a no-op. Empty `cats` or any
/// read/decode error returns false (never silently drop a write we can't verify).
fn source_side_empty(path: &Path, cats: &[Category]) -> bool {
    if cats.is_empty() {
        return false;
    }
    match fs::read(path).ok().and_then(|b| blue_marshal::decode(&b).ok()) {
        Some(v) => extract_categories(&v, cats).is_empty(),
        None => false,
    }
}

pub fn setup_preview(
    roots: &[PathBuf],
    dir: &Path,
    source_char_path: &str,
    target_char_paths: &[String],
    aspects: &[Aspect],
    allow_other_folders: bool,
) -> SetupPlan {
    let Some((src_id, char_paths, user_paths)) = scoped_files(roots, source_char_path, allow_other_folders)
    else {
        return SetupPlan { source_error: Some("Source file not found.".into()), ..Default::default() };
    };
    let targets = target_ids(&char_paths, target_char_paths);
    let store = accounts::load_store(dir);
    let resolutions = if aspect_writes(aspects).copies_char_geometry() {
        let mut ids = targets.clone();
        ids.push(src_id);
        gather_resolutions(&char_paths, &ids)
    } else {
        HashMap::new()
    };
    let mut plan = plan_setup(&char_paths, &user_paths, &store, &resolutions, src_id, &targets, aspects);

    // Drop no-op splice writes: a splice aspect whose categories are all absent
    // from the source would just back up and rewrite every target for nothing and
    // inflate the write count (e.g. an Overview copy from a char that never resized
    // its overview columns has no SortHeadersSizes to splice). Full copies always write.
    let w = aspect_writes(aspects);
    if !w.char_full_copy
        && !plan.char_writes.is_empty()
        && source_side_empty(Path::new(source_char_path), &w.char_categories)
    {
        plan.char_writes.clear();
    }
    if !w.account_full_copy && !plan.account_writes.is_empty() {
        if let Some(upath) = account_of(&store, src_id).and_then(|uid| user_paths.get(&uid)) {
            if source_side_empty(upath, &w.account_categories) {
                plan.account_writes.clear();
            }
        }
    }
    plan
}

pub fn setup_apply(
    roots: &[PathBuf],
    dir: &Path,
    source_char_path: &str,
    target_char_paths: &[String],
    aspects: &[Aspect],
    allow_other_folders: bool,
) -> Result<Vec<TargetResult>, ErrDto> {
    let plan = setup_preview(roots, dir, source_char_path, target_char_paths, aspects, allow_other_folders);
    if let Some(e) = plan.source_error {
        return Err(ErrDto::new("source", e));
    }
    let w = aspect_writes(aspects);

    // Read/decode the source's two files once, extracting each side's subtrees.
    let src_char_bytes = fs::read(source_char_path).map_err(|e| ErrDto::new("io", e.to_string()))?;
    let char_extracted = if !w.char_categories.is_empty() {
        let v = blue_marshal::decode(&src_char_bytes).map_err(|e| ErrDto::new("decode", e.to_string()))?;
        extract_categories(&v, &w.char_categories)
    } else {
        vec![]
    };
    // The account (user) file behind the source char, if any account write is needed.
    let (user_bytes, account_extracted) = if w.writes_account() {
        let Some((src_id, _cp, user_paths)) = scoped_files(roots, source_char_path, allow_other_folders) else {
            return Err(ErrDto::new("source", "Source file not found."));
        };
        let store = accounts::load_store(dir);
        let uid = account_of(&store, src_id).ok_or_else(|| ErrDto::new("source", "Source character has no paired account."))?;
        let upath = user_paths.get(&uid).ok_or_else(|| ErrDto::new("source", "Source account file not found."))?;
        let bytes = fs::read(upath).map_err(|e| ErrDto::new("io", e.to_string()))?;
        let extracted = if !w.account_categories.is_empty() {
            let v = blue_marshal::decode(&bytes).map_err(|e| ErrDto::new("decode", e.to_string()))?;
            extract_categories(&v, &w.account_categories)
        } else {
            vec![]
        };
        (bytes, extracted)
    } else {
        (vec![], vec![])
    };

    let mut results = Vec::new();
    for cw in &plan.char_writes {
        let r = if cw.full_copy {
            full_copy_to(&src_char_bytes, Path::new(&cw.path))
                .map(|bk| ok_result(&cw.path, bk.to_string_lossy().into_owned()))
        } else {
            apply_categories_to(Path::new(&cw.path), &char_extracted)
                .map(|rep| ok_result(&cw.path, rep.backup_path.to_string_lossy().into_owned()))
        };
        results.push(r.unwrap_or_else(|e| err_result(&cw.path, e)));
    }
    for aw in &plan.account_writes {
        let r = if aw.full_copy {
            full_copy_to(&user_bytes, Path::new(&aw.path))
                .map(|bk| ok_result(&aw.path, bk.to_string_lossy().into_owned()))
        } else {
            apply_categories_to(Path::new(&aw.path), &account_extracted)
                .map(|rep| ok_result(&aw.path, rep.backup_path.to_string_lossy().into_owned()))
        };
        results.push(r.unwrap_or_else(|e| err_result(&aw.path, e)));
    }
    Ok(results)
}

#[derive(Debug, Serialize)]
pub struct TargetResult {
    pub path: String,
    pub ok: bool,
    pub backup_path: Option<String>,
    pub error: Option<String>,
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

/// Batched sibling of `apply_mutation`: applies every mutation to the same
/// locked doc, then projects the tree once instead of once per mutation.
/// Non-atomic on a mid-batch failure, matching the caller's prior per-mutation
/// loop — geometry set_scalars on valid paths don't fail.
pub fn apply_mutations(state: &AppState, slot: Slot, mutations: &[Mutation]) -> Result<Node, ErrDto> {
    let mut guard = state.doc(slot).lock().unwrap();
    let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no file open"))?;
    if let Fidelity::ReadOnly { reason } = &doc.fidelity {
        return Err(ErrDto::new("read_only", reason.clone()));
    }
    for m in mutations {
        apply(&mut doc.value, m).map_err(|e| {
            let v = serde_json::to_value(&e).unwrap_or_default();
            ErrDto::new(
                v.get("code").and_then(|c| c.as_str()).unwrap_or("mutate"),
                e.to_string(),
            )
        })?;
    }
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
        // Compact the inline-first edit before it can be saved.
        doc.value = blue_marshal::reshare(&doc.value);
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

/// Map an `OverviewTabError` to a frontend `ErrDto`, carrying its `code` tag.
fn tab_err(e: OverviewTabError) -> ErrDto {
    let jv = serde_json::to_value(&e).unwrap_or_default();
    ErrDto::new(
        jv.get("code").and_then(|c| c.as_str()).unwrap_or("tab"),
        e.to_string(),
    )
}

/// Edit the user slot's overview tab structure, reshare, then re-project.
fn edit_user_tabs<F>(state: &AppState, edit: F) -> Result<OverviewColumns, ErrDto>
where
    F: FnOnce(&mut blue_marshal::Value) -> Result<(), OverviewTabError>,
{
    {
        let mut guard = state.user.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        edit(&mut doc.value).map_err(tab_err)?;
        doc.value = blue_marshal::reshare(&doc.value);
    }
    overview_columns(state)
}

pub fn tab_rename(state: &AppState, tab_idx: i64, name: String) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| rename_tab(v, tab_idx, &name))
}

pub fn tab_delete(state: &AppState, tab_idx: i64) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| delete_tab(v, tab_idx))
}

pub fn tab_reorder(state: &AppState, window_idx: usize, order: Vec<i64>) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| reorder_tabs_in_window(v, window_idx, &order))
}

pub fn tab_move(state: &AppState, tab_idx: i64, from_window: usize, to_window: usize, pos: usize) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| move_tab(v, tab_idx, from_window, to_window, pos))
}

pub fn tab_create(state: &AppState, window_idx: usize, name: String, from_tab: Option<i64>) -> Result<OverviewColumns, ErrDto> {
    // The codec clones the chosen sibling tab (else the first tab), so the new
    // tab carries every key EVE requires (bracket/color/preset). No preset
    // lookup here — cloning by index handles it.
    edit_user_tabs(state, |v| create_tab(v, window_idx, &name, from_tab).map(|_| ()))
}

pub fn preset_create(state: &AppState, from: String, new_name: String) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| create_preset(v, &from, &new_name))
}

/// Add an overview window: append the grouping (+ a cloned tab) in the user file,
/// then mint the paired `overview_N` geometry in the char file. The char write is
/// best-effort — skipped when no character is open or it is read-only; EVE
/// self-heals the window at default geometry on that character's next login.
pub fn overview_window_add(state: &AppState, name: String, from_tab: Option<i64>) -> Result<OverviewColumns, ErrDto> {
    let new_window_idx = {
        let mut guard = state.user.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        let idx = add_overview_window(&mut doc.value, &name, from_tab).map_err(tab_err)?;
        doc.value = blue_marshal::reshare(&doc.value);
        idx
    };
    {
        let mut guard = state.char.lock().unwrap();
        if let Some(doc) = guard.as_mut() {
            if !matches!(doc.fidelity, Fidelity::ReadOnly { .. }) {
                add_overview_window_geometry(&mut doc.value, new_window_idx);
                doc.value = blue_marshal::reshare(&doc.value);
            }
        }
    }
    overview_columns(state)
}

/// Remove the last overview window: drop the grouping in the user file and the
/// paired `overview_N` geometry in the char file (best-effort, as above).
pub fn overview_window_remove(state: &AppState, window_idx: usize) -> Result<OverviewColumns, ErrDto> {
    {
        let mut guard = state.user.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        remove_overview_window(&mut doc.value, window_idx).map_err(tab_err)?;
        doc.value = blue_marshal::reshare(&doc.value);
    }
    {
        let mut guard = state.char.lock().unwrap();
        if let Some(doc) = guard.as_mut() {
            if !matches!(doc.fidelity, Fidelity::ReadOnly { .. }) {
                remove_overview_window_geometry(&mut doc.value, window_idx);
                doc.value = blue_marshal::reshare(&doc.value);
            }
        }
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
        doc.value = blue_marshal::reshare(&doc.value);
    }
    autofill_lists(state)
}

pub fn set_autofill_list(state: &AppState, widget: &str, entries: Vec<String>) -> Result<Vec<RememberedList>, ErrDto> {
    edit_user_autofill(state, |v| set_list_entries(v, widget, &entries))
}

pub fn clear_all_autofill(state: &AppState) -> Result<Vec<RememberedList>, ErrDto> {
    edit_user_autofill(state, |v| clear_all_history(v))
}

/// Edit the CHAR slot's window stacks, reshare, then re-project the layout.
fn edit_char_stacks<F>(state: &AppState, edit: F) -> Result<WindowLayout, ErrDto>
where
    F: FnOnce(&mut blue_marshal::Value) -> Result<(), StackError>,
{
    {
        let mut guard = state.char.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no character file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        edit(&mut doc.value).map_err(|e| {
            let v = serde_json::to_value(&e).unwrap_or_default();
            ErrDto::new(v.get("code").and_then(|c| c.as_str()).unwrap_or("stack"), e.to_string())
        })?;
        doc.value = blue_marshal::reshare(&doc.value);
    }
    window_layout(state, Slot::Char)
}

pub fn stack_unstack(state: &AppState, member: &str) -> Result<WindowLayout, ErrDto> {
    edit_char_stacks(state, |v| unstack(v, member))
}
pub fn stack_add(state: &AppState, member: &str, container: &str) -> Result<WindowLayout, ErrDto> {
    edit_char_stacks(state, |v| add_to_stack(v, member, container))
}
pub fn stack_reorder(state: &AppState, container: &str, members: Vec<String>) -> Result<WindowLayout, ErrDto> {
    edit_char_stacks(state, |v| reorder_stack(v, container, &members))
}
pub fn stack_create(state: &AppState, member1: &str, member2: &str) -> Result<WindowLayout, ErrDto> {
    // create_stack returns the id; discard it here (the re-projection carries it).
    edit_char_stacks(state, |v| create_stack(v, member1, member2).map(|_| ()))
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
    fn apply_mutations_applies_all_in_one_call_and_projects_once() {
        let bytes = encode(&Value::Dict(vec![(
            Value::Bytes(b"k".to_vec()),
            Value::List(vec![Value::Int(1), Value::Int(2)]),
        )]))
        .unwrap();
        let path = temp_file("batch", &bytes);
        let state = AppState::new();
        open_file(&state, Slot::Char, path.to_str().unwrap()).unwrap();

        let tree = apply_mutations(
            &state,
            Slot::Char,
            &[
                Mutation::SetScalar { path: vec![Step::DictValue(0), Step::List(0)], text: "10".into() },
                Mutation::SetScalar { path: vec![Step::DictValue(0), Step::List(1)], text: "20".into() },
            ],
        )
        .unwrap();
        assert_eq!(tree.children[0].children[0].display, "10");
        assert_eq!(tree.children[0].children[1].display, "20");

        save_document(&state, Slot::Char, false).unwrap();
        let state2 = AppState::new();
        match open_file(&state2, Slot::Char, path.to_str().unwrap()).unwrap() {
            OpenOutcome::Opened { tree, .. } => {
                assert_eq!(tree.children[0].children[0].display, "10");
                assert_eq!(tree.children[0].children[1].display, "20");
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
    fn overview_edit_leaves_the_user_doc_compactly_shared() {
        let path = temp_file("ov-reshare", &overview_user_bytes());
        let state = AppState::new();
        open_file(&state, Slot::User, path.to_str().unwrap()).unwrap();

        set_overview_order(&state, 0, vec!["TYPE".into(), "NAME".into()]).unwrap();

        let guard = state.user.lock().unwrap();
        let doc = guard.as_ref().unwrap();
        let bytes = blue_marshal::encode(&doc.value).unwrap();
        // Repeated column tokens must be shared (stream shared-count > 0), not left
        // fully inlined, and the reshared doc must round-trip.
        let shared_count = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
        assert!(shared_count > 0, "overview edit should reshare repeated tokens");
        assert_eq!(blue_marshal::decode(&bytes).unwrap(), doc.value, "reshared doc round-trips");
    }

    #[test]
    fn overview_without_a_user_slot_errors() {
        let state = AppState::new();
        assert_eq!(overview_columns(&state).unwrap_err().code, "no_document");
    }

    #[test]
    fn tab_rename_then_reproject_reflects_the_new_name() {
        // Build a user file with one overview tab, open it into the user slot.
        let user = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()), Value::Dict(vec![(
                Value::Int(0),
                Value::Dict(vec![
                    (Value::Str("name".into()), Value::Str("Main".into())),
                    (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
                ]),
            )])),
            (Value::Bytes(b"tabsByWindowInstanceID".to_vec()),
             Value::List(vec![Value::List(vec![Value::Int(0)])])),
        ]))]);
        let path = temp_file("tabrename", &encode(&user).unwrap());
        let state = AppState::new();
        open_file(&state, Slot::User, path.to_str().unwrap()).unwrap();

        let cols = tab_rename(&state, 0, "Combat".into()).unwrap();
        assert_eq!(cols.tabs[0].name, "Combat");
    }

    #[test]
    fn overview_window_add_then_remove_roundtrips_the_projection() {
        // A user file with one overview window [0] holding tab 0.
        let user = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()), Value::Dict(vec![(
                Value::Int(0),
                Value::Dict(vec![
                    (Value::Bytes(b"bracket".to_vec()), Value::Bytes(b"_BracketFilterShowAll".to_vec())),
                    (Value::Bytes(b"color".to_vec()), Value::None),
                    (Value::Str("name".into()), Value::Str("Main".into())),
                    (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
                ]),
            )])),
            (Value::Bytes(b"tabsByWindowInstanceID".to_vec()),
             Value::List(vec![Value::List(vec![Value::Int(0)])])),
        ]))]);
        let path = temp_file("ovwin", &encode(&user).unwrap());
        let state = AppState::new();
        open_file(&state, Slot::User, path.to_str().unwrap()).unwrap();

        // Add a window -> two windows, the new one seeded with a cloned tab.
        let cols = overview_window_add(&state, "Scan".into(), Some(0)).unwrap();
        assert_eq!(cols.windows.len(), 2, "window added");
        assert_eq!(cols.tabs.len(), 2, "new window seeded with one cloned tab");

        // Remove the last window -> back to one, its tab reassigned to window 0.
        let cols = overview_window_remove(&state, 1).unwrap();
        assert_eq!(cols.windows.len(), 1, "window removed");
        assert_eq!(cols.windows[0].tab_indices.len(), 2, "removed window's tab moved to window 0");
        assert_eq!(cols.tabs.len(), 2, "no tabs deleted");
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

    fn autofill_user_bytes_with_repeated_ts() -> Vec<u8> {
        // Same shape as autofill_user_bytes, but the outer `ui` value is ALSO a
        // (ts, dict) tuple wrapper, reusing the identical 8-byte Long as
        // editHistory's own timestamp — a repeated shareable immutable, so the
        // post-edit reshare pass has something real to compact.
        let ts = Value::Long(vec![0u8; 8]);
        let hist = Value::Dict(vec![(
            Value::Bytes(b"/a/box".to_vec()),
            Value::List(vec![Value::Str("Jita".into()), Value::Str("Amarr".into())]),
        )]);
        let ui_inner = Value::Dict(vec![(
            Value::Bytes(b"editHistory".to_vec()),
            Value::Tuple(vec![ts.clone(), hist]),
        )]);
        encode(&Value::Dict(vec![(
            Value::Bytes(b"ui".to_vec()),
            Value::Tuple(vec![ts, ui_inner]),
        )]))
        .unwrap()
    }

    #[test]
    fn autofill_edit_leaves_the_user_doc_compactly_shared() {
        let path = temp_file("af-reshare", &autofill_user_bytes_with_repeated_ts());
        let state = AppState::new();
        open_file(&state, Slot::User, path.to_str().unwrap()).unwrap();

        set_autofill_list(&state, "/a/box", vec!["Dodixie".into()]).unwrap();

        let guard = state.user.lock().unwrap();
        let doc = guard.as_ref().unwrap();
        let bytes = blue_marshal::encode(&doc.value).unwrap();
        // The repeated timestamp Long must be shared (stream shared-count > 0),
        // not left fully inlined, and the reshared doc must round-trip — the key
        // regression guard that reshare ran without corrupting the tree.
        let shared_count = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
        assert!(shared_count > 0, "autofill edit should reshare the repeated timestamp");
        assert_eq!(blue_marshal::decode(&bytes).unwrap(), doc.value, "reshared doc round-trips");
    }

    #[test]
    fn autofill_without_a_user_slot_errors() {
        let state = AppState::new();
        assert_eq!(autofill_lists(&state).unwrap_err().code, "no_document");
    }

    #[test]
    fn everything_is_full_copy_of_both_files() {
        let w = aspect_writes(&[Aspect::Everything]);
        assert!(w.char_full_copy && w.account_full_copy);
        assert!(w.char_categories.is_empty() && w.account_categories.is_empty());
        assert!(w.writes_account() && w.writes_char() && w.copies_char_geometry());
    }

    #[test]
    fn everything_wins_even_when_mixed_with_others() {
        let w = aspect_writes(&[Aspect::Layout, Aspect::Everything]);
        assert!(w.char_full_copy && w.account_full_copy);
    }

    #[test]
    fn overview_writes_widths_to_char_and_overview_to_account() {
        let w = aspect_writes(&[Aspect::Overview]);
        assert_eq!(w.char_categories, vec![Category::OverviewWidths]);
        assert_eq!(w.account_categories, vec![Category::Overview]);
        assert!(w.writes_account() && w.writes_char());
        assert!(!w.copies_char_geometry(), "overview does not copy window geometry");
    }

    #[test]
    fn layout_is_char_only_no_account_write() {
        let w = aspect_writes(&[Aspect::Layout]);
        assert_eq!(w.char_categories, vec![Category::Layout]);
        assert!(w.account_categories.is_empty());
        assert!(!w.writes_account());
        assert!(w.copies_char_geometry());
    }

    #[test]
    fn autofill_is_account_only() {
        let w = aspect_writes(&[Aspect::Autofill]);
        assert!(w.char_categories.is_empty());
        assert_eq!(w.account_categories, vec![Category::Autofill]);
        assert!(w.writes_account() && !w.writes_char());
    }

    fn store_2accounts() -> accounts::AccountsStore {
        // account 10 has chars {1,2}; account 20 has char {3}. char 4 unpaired.
        let mut s = accounts::AccountsStore::default();
        s.accounts.insert(10, accounts::Account { alias: None, characters: vec![1, 2] });
        s.accounts.insert(20, accounts::Account { alias: None, characters: vec![3] });
        s
    }
    fn paths(ids: &[u64], prefix: &str) -> HashMap<u64, PathBuf> {
        ids.iter().map(|&i| (i, PathBuf::from(format!("{prefix}{i}.dat")))).collect()
    }

    #[test]
    fn overview_dedupes_account_write_and_lists_collateral() {
        // Source char 3 (account 20). Targets 1 and 2 both on account 10.
        let cp = paths(&[1, 2, 3], "char");
        let up = paths(&[10, 20], "user");
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), 3, &[1, 2], &[Aspect::Overview]);
        assert_eq!(plan.char_writes.len(), 2, "both targets get a char (widths) write");
        assert_eq!(plan.account_writes.len(), 1, "one account write for account 10, deduped");
        assert_eq!(plan.account_writes[0].user_id, 10);
        assert!(plan.account_writes[0].collateral_char_ids.is_empty(),
            "both chars on account 10 are selected — no collateral");
        assert!(plan.source_error.is_none());
    }

    #[test]
    fn overview_warns_collateral_for_unselected_sibling() {
        // Source char 3. Target 1 on account 10 (whose other char 2 is NOT selected).
        let cp = paths(&[1, 2, 3], "char");
        let up = paths(&[10, 20], "user");
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), 3, &[1], &[Aspect::Overview]);
        assert_eq!(plan.account_writes.len(), 1);
        assert_eq!(plan.account_writes[0].collateral_char_ids, vec![2], "char 2 is collateral");
    }

    #[test]
    fn account_aspect_excludes_an_unpaired_target() {
        let cp = paths(&[1, 3, 4], "char");
        let up = paths(&[10, 20], "user");
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), 3, &[1, 4], &[Aspect::Autofill]);
        assert_eq!(plan.excluded.len(), 1);
        assert_eq!(plan.excluded[0].char_id, 4);
        assert_eq!(plan.account_writes.len(), 1, "only the paired target's account is written");
    }

    #[test]
    fn layout_only_includes_unpaired_targets_no_account_write() {
        let cp = paths(&[1, 3, 4], "char");
        let up = paths(&[10, 20], "user");
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), 3, &[1, 4], &[Aspect::Layout]);
        assert!(plan.excluded.is_empty(), "layout needs no pairing");
        assert_eq!(plan.char_writes.len(), 2);
        assert!(plan.account_writes.is_empty());
    }

    #[test]
    fn target_on_source_account_skips_the_account_write() {
        // Source char 1 (account 10). Target char 2, same account 10.
        let cp = paths(&[1, 2], "char");
        let up = paths(&[10], "user");
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), 1, &[2], &[Aspect::Overview]);
        assert_eq!(plan.char_writes.len(), 1, "target still gets its widths");
        assert!(plan.account_writes.is_empty(), "same account already has the source's overview");
    }

    #[test]
    fn source_side_empty_detects_absent_category() {
        let with = encode(&Value::Dict(vec![(Value::Bytes(b"windows".to_vec()), Value::Dict(vec![]))])).unwrap();
        let without = encode(&Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Dict(vec![]))])).unwrap();
        let p_with = temp_file("with-windows", &with);
        let p_without = temp_file("no-windows", &without);
        assert!(!source_side_empty(&p_with, &[Category::Layout]), "windows present -> not a no-op");
        assert!(source_side_empty(&p_without, &[Category::Layout]), "windows absent -> a no-op splice");
        assert!(!source_side_empty(&p_with, &[]), "no categories -> false (guard)");
        assert!(!source_side_empty(Path::new("does-not-exist.dat"), &[Category::Layout]), "unreadable -> false");
    }

    #[test]
    fn unpaired_source_with_account_aspect_is_a_source_error() {
        let cp = paths(&[3, 4], "char");
        let up = paths(&[20], "user");
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), 4, &[3], &[Aspect::Overview]);
        assert!(plan.source_error.is_some());
        assert!(plan.char_writes.is_empty() && plan.account_writes.is_empty());
    }

    #[test]
    fn resolution_mismatch_flagged_for_layout_when_screens_differ() {
        let cp = paths(&[1, 3], "char");
        let up = paths(&[10, 20], "user");
        let mut res = HashMap::new();
        res.insert(3u64, (2560i64, 1440i64)); // source
        res.insert(1u64, (1920i64, 1080i64)); // target differs
        let plan = plan_setup(&cp, &up, &store_2accounts(), &res, 3, &[1], &[Aspect::Layout]);
        assert!(plan.char_writes[0].resolution_mismatch);
    }

    fn stacked_char_bytes() -> Vec<u8> {
        fn bb(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        fn ts() -> Value { Value::Long(vec![0u8; 8]) }
        fn geom(x: i64) -> Value { Value::Tuple(vec![Value::Int(x), Value::Int(0), Value::Int(100), Value::Int(80), Value::Int(2560), Value::Int(1440)]) }
        encode(&Value::Dict(vec![(bb("windows"), Value::Dict(vec![
            (bb("windowSizesAndPositions_1"), Value::Tuple(vec![ts(), Value::Dict(vec![(bb("m1"), geom(0)), (bb("m2"), geom(0)), (bb("C"), geom(0))])])),
            (bb("openWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![(bb("m1"), Value::Bool(true)), (bb("m2"), Value::Bool(true)), (bb("C"), Value::Bool(true))])])),
            (bb("stacksWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![(bb("m1"), bb("C")), (bb("m2"), bb("C"))])])),
            (bb("preferredIdxInStack3"), Value::Tuple(vec![ts(), Value::Dict(vec![(bb("C"), Value::Dict(vec![(bb("m1"), Value::Int(0)), (bb("m2"), Value::Int(1))]))])])),
        ]))])).unwrap()
    }

    #[test]
    fn unstack_reprojects_and_reshares() {
        let path = temp_file("stack-unstack", &stacked_char_bytes());
        let state = AppState::new();
        open_file(&state, Slot::Char, path.to_str().unwrap()).unwrap();
        let wl = stack_unstack(&state, "m1").unwrap();
        // The stack now has only m2 (m1 unstacked).
        assert_eq!(wl.stacks.len(), 1);
        assert_eq!(wl.stacks[0].members, vec!["m2".to_string()]);
        // Doc still encodes/decodes (reshare ran without corrupting the tree).
        let guard = state.char.lock().unwrap();
        let bytes = blue_marshal::encode(&guard.as_ref().unwrap().value).unwrap();
        assert_eq!(blue_marshal::decode(&bytes).unwrap(), guard.as_ref().unwrap().value);
    }

    #[test]
    fn setup_apply_overview_reports_char_and_account_writes_with_a_readonly_failure() {
        use blue_marshal::{encode, Value};
        let base = std::env::temp_dir().join(format!("app-setup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        // Discovery root with the real install/profile structure discover() expects.
        let prof = base.join("root").join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        std::fs::create_dir_all(&prof).unwrap();
        fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        fn ts() -> Value { Value::Long(vec![0u8; 8]) }
        let overview = |c: &str| Value::Dict(vec![(b("overview"),
            Value::Dict(vec![(b("overviewColumns"), Value::List(vec![b(c)]))]))]);
        let widths = || Value::Dict(vec![(b("ui"), Value::Dict(vec![(b("SortHeadersSizes"),
            Value::Tuple(vec![ts(), Value::Dict(vec![])]))]))]);
        // source char 100 on account 500; target char 200 on account 600.
        std::fs::write(prof.join("core_char_100.dat"), encode(&widths()).unwrap()).unwrap();
        std::fs::write(prof.join("core_user_500.dat"), encode(&overview("SRC")).unwrap()).unwrap();
        std::fs::write(prof.join("core_char_200.dat"), encode(&widths()).unwrap()).unwrap();
        // read-only stream (INT8-encoded) => save() refuses it => account write fails.
        std::fs::write(prof.join("core_user_600.dat"), [0x7E, 0, 0, 0, 0, 0x06, 0x01]).unwrap();

        // accounts.json lives in the app-data dir, separate from the discovery root.
        let app_dir = base.join("appdata");
        std::fs::create_dir_all(&app_dir).unwrap();
        let mut store = accounts::AccountsStore::default();
        store.accounts.insert(500, accounts::Account { alias: None, characters: vec![100] });
        store.accounts.insert(600, accounts::Account { alias: None, characters: vec![200] });
        std::fs::write(app_dir.join("accounts.json"), serde_json::to_vec(&store).unwrap()).unwrap();

        let roots = vec![base.join("root")];
        let src = prof.join("core_char_100.dat").to_string_lossy().into_owned();
        let tgt = vec![prof.join("core_char_200.dat").to_string_lossy().into_owned()];
        let results = setup_apply(&roots, &app_dir, &src, &tgt, &[Aspect::Overview], false).unwrap();

        // One char write (widths -> char 200, ok) and one account write (overview
        // -> read-only user 600, fails) — the failure did not halt the char write.
        let char_ok = results.iter().any(|r| r.path.contains("core_char_200") && r.ok);
        let acct_fail = results.iter().any(|r| r.path.contains("core_user_600") && !r.ok);
        assert!(char_ok, "char widths write succeeded");
        assert!(acct_fail, "read-only account write failed but was reported, not panicked");
    }
}
