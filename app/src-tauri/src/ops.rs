//! Command logic as plain functions over `AppState`, so it unit-tests
//! without a Tauri runtime. The `#[tauri::command]` wrappers in lib.rs are
//! one-liners delegating here.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use blue_marshal::Value;
use serde::Serialize;
use settings_model::{
    apply, default_roots, discover, project, project_overview, save,
    set_column_order, set_column_visible, set_column_width,
    window_layout as project_window_layout,
    clear_all_history, project_edit_history, set_list_entries, AutofillError, RememberedList,
    project_keybinds, Keybinds,
    Document, FileKind, Fidelity, LoadError, Mutation, Node, OverviewColumns, Profile, SaveReport,
    WindowLayout,
    apply_categories_to, extract_categories, full_copy_to, Category,
    unstack, add_to_stack, reorder_stack, create_stack, delete_orphan_frames, StackError,
    create_tab, create_window_mapping, rename_tab, delete_tab, reorder_tabs_in_window, move_tab, set_tab_preset, OverviewTabError,
    add_overview_window, remove_overview_window, add_overview_window_geometry, remove_overview_window_geometry,
    create_preset, delete_preset, fork_preset, rename_preset, set_preset_groups,
    project_hud, set_hud_value, Hud, HudScope,
    NeocomBar, NeocomError,
    project_chat, ChatPanel,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Aspect {
    Layout,
    Overview,
    Autofill,
    Keybinds,
    ProbeFormations,
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
            Aspect::Layout => {
                char_categories.push(Category::Layout);
                char_categories.push(Category::NeocomButtons);
                // The char-side HUD keys. The ship offset needs no category:
                // it lives inside the `windows` subtree Category::Layout
                // already splices whole.
                char_categories.push(Category::HudFighterPos);
                char_categories.push(Category::HudBadge);
                // The account-side four. These are what make a layout copy
                // write the account file — and therefore change every other
                // character on it. EVE stores them per account; there is no
                // per-character form to carry instead.
                account_categories.push(Category::HudShipTop);
                account_categories.push(Category::HudFighterDetached);
                account_categories.push(Category::HudFighterShown);
                account_categories.push(Category::HudNeocomWidth);
                // The target list. Most sources have never dragged theirs, and
                // absence means "at EVE's default" here as it does for the
                // four above — so most copies move the target's list back to
                // the default rather than leaving it be. That is what makes
                // the two characters match; see Category::absent_means_default.
                account_categories.push(Category::HudTargetOrigin);
                account_categories.push(Category::HudTargetAlign);
            }
            Aspect::Overview => {
                char_categories.push(Category::OverviewWidths);
                account_categories.push(Category::Overview);
            }
            Aspect::Autofill => account_categories.push(Category::Autofill),
            Aspect::Keybinds => account_categories.push(Category::Keybinds),
            Aspect::ProbeFormations => account_categories.push(Category::ProbeFormations),
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
    source_char: Option<u64>,
    target_chars: &[u64],
    aspects: &[Aspect],
) -> SetupPlan {
    let w = aspect_writes(aspects);
    let mut plan = SetupPlan::default();

    let source_account = source_char.and_then(|c| account_of(store, c));
    if w.writes_account() && source_char.is_some() {
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
    let src_res = source_char.and_then(|c| resolutions.get(&c).copied());

    let mut included: Vec<u64> = Vec::new();
    let mut seen: HashSet<u64> = HashSet::new();
    for &t in target_chars {
        // A repeated id would plan the same file twice — two writes and two
        // backups of one target. The UI passes a set, so this is a guard on the
        // command boundary rather than a fix for anything observed.
        if Some(t) == source_char || !seen.insert(t) {
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

/// The source character's id and the profile directory it lives in.
fn locate_source(roots: &[PathBuf], source_char_path: &str) -> Option<(u64, PathBuf)> {
    let src = Path::new(source_char_path);
    for p in discover(roots) {
        for f in &p.files {
            if f.path == src {
                return f.id.map(|id| (id, p.dir.clone()));
            }
        }
    }
    None
}

/// Discover, folder-scope to `anchor_dir` (unless `allow_other_folders`), and
/// split into char/user id->path maps. The anchor is passed in rather than
/// derived from a source path, because a preset source has no profile of its
/// own — the batch view supplies the profile the targets are chosen from.
fn scoped_files(
    roots: &[PathBuf],
    anchor_dir: Option<&Path>,
    allow_other_folders: bool,
) -> (HashMap<u64, PathBuf>, HashMap<u64, PathBuf>) {
    let mut char_paths = HashMap::new();
    let mut user_paths = HashMap::new();
    for p in discover(roots) {
        if !allow_other_folders && Some(p.dir.as_path()) != anchor_dir {
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
    (char_paths, user_paths)
}

/// Where a batch copy's settings come from.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BatchSource {
    Character { path: String },
    /// A preset folder, plus the profile directory whose characters the target
    /// list is drawn from (a preset belongs to no profile).
    Preset { dir: String, anchor_dir: String },
}

/// The two documents a source contributes, as (char side, account side).
/// For a preset these are its own files; for a character, its file and its
/// paired account file.
struct SourceSides {
    /// Always present: a character source is its own file and a preset source
    /// is refused unless both of its files exist.
    char_path: PathBuf,
    user_path: Option<PathBuf>,
    char_id: Option<u64>,
    anchor: Option<PathBuf>,
}

/// Each char's stored screen resolution (reference_w, reference_h), for the
/// resolution-mismatch warning. Only the source + requested targets are read.
fn gather_resolutions(char_paths: &HashMap<u64, PathBuf>, ids: &[u64]) -> HashMap<u64, (i64, i64)> {
    let mut out = HashMap::new();
    for &id in ids {
        let Some(path) = char_paths.get(&id) else { continue };
        let Ok(bytes) = fs::read(path) else { continue };
        let Ok(value) = blue_marshal::decode(&bytes) else { continue };
        let wl = project_window_layout(&value, None);
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

/// True if decoding `path` and extracting `cats` yields NOTHING AT ALL — no
/// values and no removals — so a splice would be a no-op.
///
/// "Nothing at all" is deliberately not "the source has none of these
/// categories". A leaf HUD category the source lacks comes back as
/// `(cat, None)`: an instruction to remove that key from the target, which is
/// real work and must not be suppressed. Narrowing this to count only PRESENT
/// values would silently kill the removal path and half-apply a Layout copy
/// again — the exact bug this branch exists to fix. Pinned by
/// `an_account_side_of_only_removals_is_not_suppressed_as_a_no_op`.
///
/// Empty `cats` or any read/decode error returns false (never silently drop a
/// write we can't verify).
fn source_side_empty(path: &Path, cats: &[Category]) -> bool {
    if cats.is_empty() {
        return false;
    }
    match fs::read(path).ok().and_then(|b| blue_marshal::decode(&b).ok()) {
        Some(v) => extract_categories(&v, cats).is_empty(),
        None => false,
    }
}

/// Resolve a source into the pieces the planner and the applier both need.
/// `allow_other_folders` must be the caller's real flag — it is threaded into
/// the inner `scoped_files` lookup for a character source's account file, so
/// that lookup stays anchored to the source's own profile folder exactly like
/// the outer one both callers already run.
fn resolve_source(
    roots: &[PathBuf],
    dir: &Path,
    source: &BatchSource,
    aspects: &[Aspect],
    allow_other_folders: bool,
) -> Result<SourceSides, String> {
    match source {
        BatchSource::Character { path } => {
            let Some((id, profile_dir)) = locate_source(roots, path) else {
                return Err("Source file not found.".into());
            };
            let store = accounts::load_store(dir);
            let user_path = account_of(&store, id).and_then(|uid| {
                let (_, users) = scoped_files(roots, Some(&profile_dir), allow_other_folders);
                users.get(&uid).cloned()
            });
            Ok(SourceSides {
                char_path: PathBuf::from(path),
                user_path,
                char_id: Some(id),
                anchor: Some(profile_dir),
            })
        }
        BatchSource::Preset { dir: pdir, anchor_dir } => {
            let pdir = PathBuf::from(pdir);
            // A direct child of the presets dir, not merely lexically prefixed
            // by it — `starts_with` is a component-wise prefix test, so
            // `<presets dir>/../../anything` would pass it. `presets::preset_path`
            // (the write/rename/delete side) enforces the real "exactly one
            // component" property; this read-only path now matches it instead
            // of only claiming to.
            if pdir.parent() != Some(crate::presets::presets_dir(dir).as_path()) {
                return Err("That preset could not be read.".into());
            }
            let (c, u) = (pdir.join(crate::presets::CHAR_FILE), pdir.join(crate::presets::USER_FILE));
            if !c.is_file() || !u.is_file() {
                return Err("That preset could not be read.".into());
            }
            let char_doc = crate::presets::load(&c)
                .map_err(|e| format!("The preset's character-side file could not be read: {e}"))?;
            let user_doc = crate::presets::load(&u)
                .map_err(|e| format!("The preset's account-side file could not be read: {e}"))?;
            if aspects.contains(&Aspect::Everything) {
                if !crate::presets::is_full(&pdir) {
                    return Err(
                        "This preset holds only part of a character's settings, so it cannot replace a whole file. Pick the aspects it holds instead."
                            .into(),
                    );
                }
                // `full` is set from the request at save time and never
                // re-checked against what actually got written — a Layout-only
                // preset's user.dat (or an Autofill/Overview-only preset's
                // char.dat) is an empty root dict by construction. `presets::create`
                // now refuses to mint a full preset like that, but an older
                // preset or a hand-edited imported one can still reach here.
                // A full copy of an empty document is not a complete copy —
                // it is a wipe of the target's whole file.
                if crate::presets::is_empty_root(&char_doc) || crate::presets::is_empty_root(&user_doc) {
                    return Err(
                        "This preset is marked as a complete copy, but one of its files is empty, so applying Everything would erase the target instead of replacing it. Pick the aspects it actually holds instead."
                            .into(),
                    );
                }
            }
            Ok(SourceSides {
                char_path: c,
                user_path: Some(u),
                char_id: None,
                anchor: if anchor_dir.is_empty() { None } else { Some(PathBuf::from(anchor_dir)) },
            })
        }
    }
}

pub fn setup_preview(
    roots: &[PathBuf],
    dir: &Path,
    source: &BatchSource,
    target_char_paths: &[String],
    aspects: &[Aspect],
    allow_other_folders: bool,
) -> SetupPlan {
    preview_with_sides(roots, dir, source, target_char_paths, aspects, allow_other_folders).0
}

/// The planner, handing back the resolved source alongside the plan.
///
/// `resolve_source` and `scoped_files` each walk `discover()`, and an apply used
/// to run the whole preview and then resolve the source AGAIN — five walks where
/// three do. `setup_apply` takes the sides from here instead. `None` accompanies
/// a plan carrying a `source_error`: there is no resolved source to hand over.
fn preview_with_sides(
    roots: &[PathBuf],
    dir: &Path,
    source: &BatchSource,
    target_char_paths: &[String],
    aspects: &[Aspect],
    allow_other_folders: bool,
) -> (SetupPlan, Option<SourceSides>) {
    let sides = match resolve_source(roots, dir, source, aspects, allow_other_folders) {
        Ok(s) => s,
        Err(e) => return (SetupPlan { source_error: Some(e), ..Default::default() }, None),
    };
    let (char_paths, user_paths) =
        scoped_files(roots, sides.anchor.as_deref(), allow_other_folders);
    let targets = target_ids(&char_paths, target_char_paths);
    let store = accounts::load_store(dir);
    let w = aspect_writes(aspects);
    // A preset source issues no resolution warning: plan_setup needs a source
    // resolution to compare against, and there is no source character. The
    // preset's char.dat does carry `reference_w/h`, so wiring the warning up is
    // a later, additive change — see the spec's §6. Skip the decode work
    // entirely for a preset source (char_id is None): src_res can never be
    // Some, so no mismatch could ever be reported.
    let resolutions = if w.copies_char_geometry() && sides.char_id.is_some() {
        let mut ids = targets.clone();
        if let Some(id) = sides.char_id {
            ids.push(id);
        }
        gather_resolutions(&char_paths, &ids)
    } else {
        HashMap::new()
    };
    let mut plan = plan_setup(
        &char_paths,
        &user_paths,
        &store,
        &resolutions,
        sides.char_id,
        &targets,
        aspects,
    );

    // Drop no-op splice writes: a splice aspect whose categories are all absent
    // from the source would back up and rewrite every target for nothing.
    if !w.char_full_copy && !plan.char_writes.is_empty() {
        if source_side_empty(&sides.char_path, &w.char_categories) {
            plan.char_writes.clear();
        }
    }
    if !w.account_full_copy && !plan.account_writes.is_empty() {
        if let Some(p) = sides.user_path.as_deref() {
            if source_side_empty(p, &w.account_categories) {
                plan.account_writes.clear();
            }
        }
    }
    (plan, Some(sides))
}

pub fn setup_apply(
    roots: &[PathBuf],
    dir: &Path,
    source: &BatchSource,
    target_char_paths: &[String],
    aspects: &[Aspect],
    allow_other_folders: bool,
) -> Result<Vec<TargetResult>, ErrDto> {
    let (plan, sides) =
        preview_with_sides(roots, dir, source, target_char_paths, aspects, allow_other_folders);
    if let Some(e) = plan.source_error {
        return Err(ErrDto::new("source", e));
    }
    // Set together with `source_error`: a plan without one always carries sides.
    let sides = sides.ok_or_else(|| ErrDto::new("source", "The source could not be read."))?;
    let w = aspect_writes(aspects);

    // Only the account side is optional: a character source with no paired
    // account has none. plan_setup already refuses that combination with a
    // source error, so this arm is a backstop -- worded for a user anyway,
    // since a developer-ese string in a toast helps nobody.
    let read_user_side = |p: Option<&Path>| -> Result<Vec<u8>, ErrDto> {
        match p {
            Some(p) => fs::read(p).map_err(|e| ErrDto::new("io", e.to_string())),
            None => Err(ErrDto::new("source", "The source has no account file to copy from.")),
        }
    };
    // `cats.is_empty()` alone covers the deliberate empty-Vec case (a
    // splice aspect the source lacks, already dropped by `setup_preview`'s
    // no-op suppression, or Everything's always-empty category lists). A
    // zero-length `bytes` is never legitimate for a non-empty `cats` list —
    // it must decode, or this returns a decode error instead of silently
    // treating "empty file" as "empty projection".
    let extract_side = |bytes: &[u8], cats: &[Category]| -> Result<Vec<(Category, Option<Value>)>, ErrDto> {
        if cats.is_empty() {
            return Ok(Vec::new());
        }
        let v = blue_marshal::decode(bytes).map_err(|e| ErrDto::new("decode", e.to_string()))?;
        Ok(extract_categories(&v, cats))
    };

    let src_char_bytes = fs::read(&sides.char_path).map_err(|e| ErrDto::new("io", e.to_string()))?;
    let char_extracted = extract_side(&src_char_bytes, &w.char_categories)?;
    let user_bytes = if w.writes_account() { read_user_side(sides.user_path.as_deref())? } else { Vec::new() };
    let account_extracted = extract_side(&user_bytes, &w.account_categories)?;

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

/// Create a preset from the OPEN documents, so unsaved edits are captured.
pub fn preset_save(
    state: &AppState,
    app_data: &Path,
    name: &str,
    aspects: &[Aspect],
    overwrite: bool,
) -> Result<(), ErrDto> {
    // Lock user before char, matching `hud_layout`, `overview_columns` and
    // `window_layout` — every spot in this file that holds both slots at once
    // takes them in that order, which is what rules out an AB-BA deadlock
    // between two commands running concurrently.
    let user_guard = state.user.lock().unwrap();
    let char_guard = state.char.lock().unwrap();
    crate::presets::create(
        app_data,
        name,
        aspects,
        crate::presets::CreateInput {
            char_doc: char_guard.as_ref().map(|d| &d.value),
            user_doc: user_guard.as_ref().map(|d| &d.value),
        },
        overwrite,
    )
    .map(|_| ())
    .map_err(|m| ErrDto::new("preset", m))
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
    // Lock user before the requested slot, matching hud_layout and
    // overview_columns — one consistent order across this file rules out
    // lock-order inversion between concurrent commands. When the CALLER asked
    // for the user slot there is no second document to take: locking `user`
    // twice would deadlock (std Mutex is not reentrant), and an account file
    // has no windows to project anyway.
    let uguard = matches!(slot, Slot::Char).then(|| state.user.lock().unwrap());
    let guard = state.doc(slot).lock().unwrap();
    let doc = guard.as_ref().ok_or_else(|| ErrDto::new("no_document", "no file open"))?;
    let user = uguard.as_ref().and_then(|g| g.as_ref()).map(|d| &d.value);
    Ok(project_window_layout(&doc.value, user))
}

/// Project the HUD anchors: the character document is required, the account
/// document optional (an unpaired character still has its own anchors).
pub fn hud_layout(state: &AppState) -> Result<Hud, ErrDto> {
    // Lock user before char, matching `overview_columns` — the only other spot
    // that holds both slots at once. A consistent order across the file rules
    // out lock-order-inversion deadlock between concurrently invoked commands.
    let uguard = state.user.lock().unwrap();
    let cguard = state.char.lock().unwrap();
    let cdoc = cguard.as_ref().ok_or_else(|| ErrDto::new("no_document", "no character file open"))?;
    Ok(project_hud(&cdoc.value, uguard.as_ref().map(|d| &d.value)))
}

/// Project the account document's chat window splits. An unpaired character is
/// normal, so no account file open means an empty list, NOT an error — the
/// canvas treats these as a bonus layer (design spec §4.5).
pub fn chat_panels(state: &AppState) -> Result<Vec<ChatPanel>, ErrDto> {
    let guard = state.user.lock().unwrap();
    Ok(guard.as_ref().map(|d| project_chat(&d.value)).unwrap_or_default())
}

/// Write the chat splits for one or more channels, reshare if anything was
/// minted, and return the fresh projection.
///
/// The account slot only: the character document holds the chat WINDOW, but the
/// split is account-scoped, so nothing here touches it. The frontend marks the
/// user slot dirty.
///
/// Projects from inside the same guard rather than calling `chat_panels`, which
/// would take the same lock again — `std::sync::Mutex` is not reentrant.
pub fn set_chat_splits(
    state: &AppState,
    ids: Vec<String>,
    userlist: Option<i64>,
    input: Option<i64>,
) -> Result<Vec<ChatPanel>, ErrDto> {
    let mut guard = state.user.lock().unwrap();
    let doc = guard
        .as_mut()
        .ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
    if let Fidelity::ReadOnly { reason } = &doc.fidelity {
        return Err(ErrDto::new("read_only", reason.clone()));
    }
    let minted = settings_model::set_chat_splits(&mut doc.value, &ids, userlist, input)
        .map_err(chat_err)?;
    // Only a mint de-shares the document; a scalar overwrite sets one value in
    // place, where a whole-tree reshare would buy nothing.
    if minted {
        doc.value = blue_marshal::reshare(&doc.value);
    }
    Ok(project_chat(&doc.value))
}

fn chat_err(e: settings_model::ChatError) -> ErrDto {
    let v = serde_json::to_value(&e).unwrap_or_default();
    ErrDto::new(v.get("code").and_then(|c| c.as_str()).unwrap_or("chat"), e.to_string())
}

/// Write one HUD field into whichever document its scope names, reshare, and
/// re-project. The frontend marks that slot dirty from the entry's scope.
pub fn set_hud_field(state: &AppState, name: &str, text: &str) -> Result<Hud, ErrDto> {
    let scope = {
        // The projection is the single source of truth for which file a field
        // lives in, so ops never repeats the field table.
        let hud = hud_layout(state)?;
        hud.entries
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.scope)
            .ok_or_else(|| ErrDto::new("hud", format!("unknown field {name:?}")))?
    };
    let slot = match scope {
        HudScope::Char => Slot::Char,
        HudScope::Account => Slot::User,
    };
    {
        let mut guard = state.doc(slot).lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| {
            let which = match scope {
                HudScope::Char => "character",
                HudScope::Account => "account",
            };
            ErrDto::new("no_document", format!("no {which} file open"))
        })?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        let minted = set_hud_value(&mut doc.value, name, text).map_err(|e| {
            let v = serde_json::to_value(&e).unwrap_or_default();
            ErrDto::new(v.get("code").and_then(|c| c.as_str()).unwrap_or("hud"), e.to_string())
        })?;
        // Only a mint de-shares the document, and only a de-shared document
        // needs re-sharing before it can encode. Every other write sets one
        // scalar in place, where a whole-tree reshare bought nothing.
        if minted {
            doc.value = blue_marshal::reshare(&doc.value);
        }
    }
    hud_layout(state)
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

/// Give the account an explicit tab-to-window mapping. User slot only: window
/// 0's char-side geometry key already exists on any account with an overview.
pub fn overview_create_window_mapping(state: &AppState) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| create_window_mapping(v).map(|_| ()))
}

pub fn preset_create(state: &AppState, from: String, new_name: String) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| create_preset(v, &from, &new_name))
}

pub fn preset_rename(state: &AppState, old_name: String, new_name: String) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| rename_preset(v, &old_name, &new_name))
}

pub fn preset_delete(state: &AppState, name: String) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| delete_preset(v, &name))
}

pub fn tab_set_preset(state: &AppState, tab_idx: i64, preset: String) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| set_tab_preset(v, tab_idx, &preset))
}

pub fn preset_set_groups(state: &AppState, name: String, groups: Vec<i64>) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| set_preset_groups(v, &name, &groups))
}

pub fn overview_set_states(state: &AppState, which: String, ids: Vec<i64>) -> Result<OverviewColumns, ErrDto> {
    let list = match which.as_str() {
        "background" => settings_model::StateList::Background,
        "backgroundOrder" => settings_model::StateList::BackgroundOrder,
        "flag" => settings_model::StateList::Flag,
        "flagOrder" => settings_model::StateList::FlagOrder,
        other => return Err(ErrDto::new("overview", format!("unknown state list {other}"))),
    };
    edit_user_tabs(state, |v| settings_model::set_state_list(v, list, &ids))
}

pub fn overview_set_state_color(state: &AppState, id: i64, rgba: Option<[f64; 4]>) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| settings_model::set_state_color(v, id, rgba))
}

pub fn overview_set_bool(state: &AppState, key: String, on: bool) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| settings_model::set_overview_bool(v, &key, on))
}

pub fn preset_set_states(
    state: &AppState, name: String, filtered: Vec<i64>, always_shown: Vec<i64>,
) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| settings_model::set_preset_states(v, &name, &filtered, &always_shown))
}

/// Fork a preset from explicit lists (e.g. a built-in default not stored in the
/// file) and point the tab at it, in one edit.
pub fn preset_fork(
    state: &AppState, tab_idx: i64, name: String,
    groups: Vec<i64>, filtered_states: Vec<i64>, always_shown_states: Vec<i64>,
) -> Result<OverviewColumns, ErrDto> {
    edit_user_tabs(state, |v| fork_preset(v, tab_idx, &name, &groups, &filtered_states, &always_shown_states))
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

/// What a pack file contains, for the confirm dialog — section name and the
/// number of entries in it (0 for a scalar section).
#[derive(Debug, Clone, serde::Serialize)]
pub struct PackSummary {
    pub sections: Vec<(String, usize)>,
    pub ignored: Vec<String>,
}

/// Map a `PackError` to a frontend `ErrDto`, carrying its `code` tag — the same
/// shape as `tab_err`, so the UI sees `not_a_pack` / `yaml` / `no_overview`
/// rather than one opaque code.
fn pack_err(e: settings_model::PackError) -> ErrDto {
    let jv = serde_json::to_value(&e).unwrap_or_default();
    ErrDto::new(
        jv.get("code").and_then(|c| c.as_str()).unwrap_or("pack"),
        e.to_string(),
    )
}

fn read_pack_file(path: &str) -> Result<settings_model::Pack, ErrDto> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| ErrDto::new("io", format!("{path}: {e}")))?;
    settings_model::parse_pack(&text).map_err(pack_err)
}

/// Parse a pack and describe it. Reads the file only — no lock, no mutation.
pub fn pack_preview(path: &str) -> Result<PackSummary, ErrDto> {
    let pack = read_pack_file(path)?;
    let sections = pack
        .sections
        .iter()
        .map(|(name, node)| {
            // `PackNode`, not `Node` — the crate root's `Node` is the projection type.
            let count = match node { settings_model::PackNode::Seq(items) => items.len(), _ => 0 };
            (name.clone(), count)
        })
        .collect();
    Ok(PackSummary { sections, ignored: pack.ignored })
}

/// `pack_import`'s result: the re-projected columns plus the apply report, so
/// the UI can surface warnings ("unknown colour name…", "ignored empty
/// 'presets' section…") on import the same way `pack_export` already does.
#[derive(Debug, serde::Serialize)]
pub struct PackImportResult {
    pub columns: OverviewColumns,
    pub report: settings_model::PackReport,
}

/// Apply a pack to the open account file. Marks the slot dirty like every other
/// editor — the user saves, and the normal backup chain applies.
pub fn pack_import(state: &AppState, path: &str) -> Result<PackImportResult, ErrDto> {
    let pack = read_pack_file(path)?;
    let report = {
        let mut guard = state.user.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        let report = settings_model::apply_pack(&mut doc.value, &pack).map_err(pack_err)?;
        doc.value = blue_marshal::reshare(&doc.value);
        report
    };
    let columns = overview_columns(state)?;
    Ok(PackImportResult { columns, report })
}

/// Write the open account's overview out as a pack. Exports the IN-MEMORY
/// document, so unsaved edits are included.
pub fn pack_export(state: &AppState, path: &str) -> Result<settings_model::PackReport, ErrDto> {
    let (pack, warnings) = {
        let guard = state.user.lock().unwrap();
        let doc = guard.as_ref().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
        settings_model::read_pack(&doc.value)
    };
    if pack.sections.is_empty() {
        return Err(pack_err(settings_model::PackError::NoOverview));
    }
    let text = settings_model::emit_pack(&pack);
    std::fs::write(path, text).map_err(|e| ErrDto::new("io", format!("{path}: {e}")))?;
    Ok(settings_model::PackReport::exported(&pack, warnings))
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

pub fn keybinds(state: &AppState) -> Result<Keybinds, ErrDto> {
    let user = state.user.lock().unwrap();
    Ok(project_keybinds(user.as_ref().map(|d| &d.value)))
}

#[derive(serde::Serialize)]
pub struct SetKeybindResult {
    pub keybinds: Keybinds,
    /// Commands whose binding this write cleared, so the UI can name them.
    pub stolen: Vec<String>,
}

pub fn set_keybind_cmd(
    state: &AppState,
    command: &str,
    keys: Option<Vec<i64>>,
) -> Result<SetKeybindResult, ErrDto> {
    let stolen = {
        let mut guard = state.user.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        let stolen = settings_model::set_keybind(&mut doc.value, command, keys)
            .map_err(|e| ErrDto::new("keybind", format!("{e:?}")))?;
        doc.value = blue_marshal::reshare(&doc.value);
        stolen
    };
    Ok(SetKeybindResult { keybinds: keybinds(state)?, stolen })
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
pub fn stack_delete_orphans(state: &AppState) -> Result<WindowLayout, ErrDto> {
    // Returns the ids it removed; the re-projection is what the UI reads, so
    // they are discarded here. Deleting nothing is a success, not an error.
    edit_char_stacks(state, |v| { delete_orphan_frames(v); Ok(()) })
}

/// Project the CHAR slot's neocom bar.
pub fn neocom_bar(state: &AppState) -> Result<NeocomBar, ErrDto> {
    let guard = state.char.lock().unwrap();
    let doc = guard.as_ref().ok_or_else(|| ErrDto::new("no_document", "no character file open"))?;
    settings_model::project_neocom(&doc.value).map_err(neocom_err)
}

fn neocom_err(e: NeocomError) -> ErrDto {
    let v = serde_json::to_value(&e).unwrap_or_default();
    ErrDto::new(v.get("code").and_then(|c| c.as_str()).unwrap_or("neocom"), e.to_string())
}

/// Edit the CHAR slot's neocom bar, reshare, then re-project it.
fn edit_char_neocom<F>(state: &AppState, edit: F) -> Result<NeocomBar, ErrDto>
where
    F: FnOnce(&mut blue_marshal::Value) -> Result<(), NeocomError>,
{
    {
        let mut guard = state.char.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no character file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        edit(&mut doc.value).map_err(neocom_err)?;
        doc.value = blue_marshal::reshare(&doc.value);
    }
    neocom_bar(state)
}

pub fn neocom_reorder(state: &AppState, order: Vec<usize>) -> Result<NeocomBar, ErrDto> {
    edit_char_neocom(state, |v| settings_model::neocom_reorder(v, &order))
}
pub fn neocom_remove(state: &AppState, index: usize) -> Result<NeocomBar, ErrDto> {
    edit_char_neocom(state, |v| settings_model::neocom_remove(v, index))
}
pub fn neocom_add(state: &AppState, id: &str, btn_type: i64, icon_path: &str) -> Result<NeocomBar, ErrDto> {
    edit_char_neocom(state, |v| settings_model::neocom_add(v, id, btn_type, icon_path))
}
pub fn neocom_reset(state: &AppState) -> Result<NeocomBar, ErrDto> {
    edit_char_neocom(state, settings_model::neocom_reset)
}

fn probe_err(e: settings_model::ProbeError) -> ErrDto {
    let v = serde_json::to_value(&e).unwrap_or_default();
    ErrDto::new(v.get("code").and_then(|c| c.as_str()).unwrap_or("probes"), e.to_string())
}

pub fn probe_formations(state: &AppState) -> Result<settings_model::Formations, ErrDto> {
    let guard = state.user.lock().unwrap();
    let doc = guard.as_ref().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
    settings_model::project_formations(&doc.value).map_err(probe_err)
}

/// Edit the USER slot's formations, reshare, then re-project them. Mirrors
/// `edit_char_neocom`, on the account side.
fn edit_user_probes<F>(state: &AppState, edit: F) -> Result<settings_model::Formations, ErrDto>
where
    F: FnOnce(&mut blue_marshal::Value) -> Result<(), settings_model::ProbeError>,
{
    {
        let mut guard = state.user.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        edit(&mut doc.value).map_err(probe_err)?;
        doc.value = blue_marshal::reshare(&doc.value);
    }
    probe_formations(state)
}

/// `id: None` creates at the next free id. Resolving it here rather than in the
/// frontend keeps id allocation in one place, next to the rule that produced it.
pub fn set_probe_formation(
    state: &AppState,
    id: Option<i64>,
    name: &str,
    probes: Vec<[f64; 3]>,
    range: f64,
) -> Result<settings_model::Formations, ErrDto> {
    let id = match id {
        Some(i) => i,
        None => match probe_formations(state) {
            Ok(f) => settings_model::next_formation_id(&f),
            // No key yet: `set_formation` mints it below, and 0 is the first
            // free id — this is the only create path, so a bare `?` here would
            // fail every first-ever formation on an account with none saved.
            Err(e) if e.code == "no_formations" => 0,
            Err(e) => return Err(e),
        },
    };
    edit_user_probes(state, |v| settings_model::set_formation(v, id, name, &probes, range))
}

pub fn remove_probe_formation(
    state: &AppState,
    id: i64,
) -> Result<settings_model::Formations, ErrDto> {
    edit_user_probes(state, |v| settings_model::remove_formation(v, id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::{encode, Value};

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }

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

    fn windowless_user_bytes() -> Vec<u8> {
        use blue_marshal::{encode, Value};
        fn bb(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        fn ts() -> Value { Value::Long(vec![0u8; 8]) }
        let tab = Value::Dict(vec![
            (Value::Str("name".into()), Value::StrUcs2("Default".into())),
            (bb("overview"), bb("P")),
        ]);
        encode(&Value::Dict(vec![(bb("overview"), Value::Dict(vec![
            (bb("tabsettings_new"), Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab)])])),
        ]))])).unwrap()
    }

    #[test]
    fn creating_a_window_mapping_projects_one_window_holding_every_tab() {
        let path = temp_file("ov-windowless", &windowless_user_bytes());
        let state = AppState::new();
        open_file(&state, Slot::User, path.to_str().unwrap()).unwrap();

        // Before: the account projects no windows at all.
        assert!(overview_columns(&state).unwrap().windows.is_empty());

        let cols = overview_create_window_mapping(&state).unwrap();
        assert_eq!(cols.windows.len(), 1);
        assert_eq!(cols.windows[0].tab_indices, vec![0]);

        // Doc still encodes/decodes (reshare ran without corrupting the tree).
        let guard = state.user.lock().unwrap();
        let bytes = blue_marshal::encode(&guard.as_ref().unwrap().value).unwrap();
        assert_eq!(blue_marshal::decode(&bytes).unwrap(), guard.as_ref().unwrap().value);
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
    fn layout_now_writes_the_account_side_too() {
        // Was `layout_is_char_only_no_account_write`: it pinned the exact
        // opposite of what this task makes true. Updated rather than deleted
        // or dropped — `layout_carries_the_whole_hud_across_both_files` below
        // pins the exact category lists; this keeps the narrower, still-named
        // claim that used to be false.
        let w = aspect_writes(&[Aspect::Layout]);
        assert!(w.char_categories.contains(&Category::Layout));
        assert!(!w.account_categories.is_empty(), "layout now carries account-side HUD fields too");
        assert!(w.writes_account());
        assert!(w.copies_char_geometry());
    }

    #[test]
    fn the_layout_aspect_carries_the_neocom_buttons() {
        let w = aspect_writes(&[Aspect::Layout]);
        assert!(w.char_categories.contains(&Category::NeocomButtons), "the neocom bar is character-side");
        assert!(!w.account_categories.contains(&Category::NeocomButtons), "the neocom bar is character-side");
        assert!(w.copies_char_geometry(), "the resolution warning still applies");
    }

    #[test]
    fn layout_carries_the_whole_hud_across_both_files() {
        let w = aspect_writes(&[Aspect::Layout]);
        assert_eq!(
            w.char_categories,
            vec![
                Category::Layout,
                Category::NeocomButtons,
                Category::HudFighterPos,
                Category::HudBadge
            ]
        );
        assert_eq!(
            w.account_categories,
            vec![
                Category::HudShipTop,
                Category::HudFighterDetached,
                Category::HudFighterShown,
                Category::HudNeocomWidth,
                Category::HudTargetOrigin,
                Category::HudTargetAlign
            ]
        );
        assert!(w.writes_account(), "layout writes the account file now");
        assert!(w.copies_char_geometry(), "the badge offset is absolute px, so the resolution warning must fire");
    }

    /// The `(timestamp, value)` wrapper every real settings leaf carries.
    /// Load-bearing here, not decoration: `hud.rs`'s `leaf` reads a 2-element
    /// tuple AS that wrapper, so a point stored bare as `(x, y)` projects to
    /// None on both sides of a copy and every assertion below passes vacuously.
    /// That is exactly how this test used to pass with the char-side HUD
    /// unexercised.
    fn wrapped(v: Value) -> Value {
        Value::Tuple(vec![Value::Long(vec![0u8; 8]), v])
    }
    fn point(x: i64, y: i64) -> Value {
        wrapped(Value::Tuple(vec![Value::Int(x), Value::Int(y)]))
    }

    /// A char doc carrying the char-side half of the HUD, in the shape real
    /// files use.
    fn hud_char_doc() -> Value {
        Value::Dict(vec![
            (b("windows"), Value::Dict(vec![(b("shipuialignleftoffset"), wrapped(Value::Float(-1052.0)))])),
            (b("ui"), Value::Dict(vec![(b("fightersDetachedPosition"), point(326, 54))])),
            (b("notifications"), Value::Dict(vec![(b("notification_badge_offset"), point(2519, 131))])),
        ])
    }

    /// A char doc for the TARGET of a copy: the same three sections a real
    /// character file has — `apply_to_tree` skips any category whose parent
    /// section is missing, so a target without `ui` and `notifications` never
    /// receives the char-side HUD at all — holding its own values throughout.
    fn hud_target_char_doc() -> Value {
        Value::Dict(vec![
            (b("windows"), Value::Dict(vec![(b("shipuialignleftoffset"), wrapped(Value::Float(-1.0)))])),
            (b("ui"), Value::Dict(vec![(b("fightersDetachedPosition"), point(10, 20))])),
            (b("notifications"), Value::Dict(vec![(b("notification_badge_offset"), point(1, 2))])),
        ])
    }

    /// An account doc carrying the account-side half.
    fn hud_user_doc() -> Value {
        Value::Dict(vec![
            (
                b("ui"),
                Value::Dict(vec![
                    (b("shipuialigntop"), wrapped(Value::Bool(true))),
                    (b("detachFighterUI"), wrapped(Value::Bool(true))),
                    (b("displayFighterUI"), wrapped(Value::Bool(true))),
                    (
                        b("targetOrigin"),
                        wrapped(Value::Tuple(vec![Value::Float(0.75), Value::Float(0.25)])),
                    ),
                    (b("alignHorizontally"), wrapped(Value::Bool(true))),
                ]),
            ),
            (b("windows"), Value::Dict(vec![(b("neocomWidth"), wrapped(Value::Int(72)))])),
        ])
    }

    /// The account doc for the TARGET: same shape, its own values on all four.
    fn hud_target_user_doc() -> Value {
        Value::Dict(vec![
            (
                b("ui"),
                Value::Dict(vec![
                    (b("shipuialigntop"), wrapped(Value::Bool(false))),
                    (b("detachFighterUI"), wrapped(Value::Bool(false))),
                    (b("displayFighterUI"), wrapped(Value::Bool(false))),
                    (
                        b("targetOrigin"),
                        wrapped(Value::Tuple(vec![Value::Float(0.25), Value::Float(0.75)])),
                    ),
                    (b("alignHorizontally"), wrapped(Value::Bool(false))),
                ]),
            ),
            (b("windows"), Value::Dict(vec![(b("neocomWidth"), wrapped(Value::Int(37)))])),
        ])
    }

    fn hud_values(c: &Value, u: &Value) -> Vec<(String, Option<String>)> {
        settings_model::project_hud(c, Some(u))
            .entries
            .into_iter()
            .map(|e| (e.name, e.value))
            .collect()
    }

    #[test]
    fn a_layout_copy_leaves_every_hud_field_equal() {
        // Asserted through project_hud rather than raw keys: the projection is
        // what the HUD editor shows, so this is the user-visible claim. It is
        // also the only cross-check between batch.rs's key paths and hud.rs's
        // private FIELDS table, which is why the None-guard below matters —
        // "None == None" would pass with the copy completely broken.
        let w = aspect_writes(&[Aspect::Layout]);
        let (src_char, src_user) = (hud_char_doc(), hud_user_doc());
        let (mut tgt_char, mut tgt_user) = (hud_target_char_doc(), hud_target_user_doc());

        let source = hud_values(&src_char, &src_user);
        let target_before = hud_values(&tgt_char, &tgt_user);
        // Every projected HUD field is carried, the target list included since
        // 0.26.0. `target_x` is the one worth naming: it is a FRACTION whose
        // denominator encodes the SOURCE client's neocom width, so a copy
        // between accounts with different neocoms lands the list a few tens of
        // pixels off. That is a known, accepted cost of matching the source —
        // see Category::absent_means_default.
        assert_eq!(source.len(), 12, "every HUD field is carried");
        for (name, v) in &source {
            assert!(v.is_some(), "{name} must have a value on the SOURCE, or the copy proves nothing");
        }
        for (name, v) in &target_before {
            assert!(v.is_some(), "{name} must have a value on the TARGET before the copy");
        }
        assert!(
            source.iter().zip(&target_before).all(|((_, s), (_, t))| s != t),
            "every field must start out different, or a no-op copy would pass: {source:?} vs {target_before:?}"
        );

        settings_model::apply_to_tree(&mut tgt_char, &extract_categories(&src_char, &w.char_categories));
        settings_model::apply_to_tree(&mut tgt_user, &extract_categories(&src_user, &w.account_categories));

        let after = hud_values(&tgt_char, &tgt_user);
        assert_eq!(source, after, "every one of the twelve fields came across");
    }

    #[test]
    fn a_layout_copy_from_a_source_at_the_target_lists_default_removes_the_targets() {
        // The common case, and the one that surprises: 87% of real accounts
        // have never dragged their target list, so most Layout copies DELETE
        // the target's position rather than leaving it alone. That is what
        // makes the two characters match — but it is a deletion, so it gets a
        // test of its own rather than riding on the equality above.
        let w = aspect_writes(&[Aspect::Layout]);
        let mut tgt_user = hud_target_user_doc();
        let before = settings_model::project_hud(&hud_target_char_doc(), Some(&tgt_user));
        assert!(
            before.entries.iter().any(|e| e.name == "target_x" && e.value.is_some()),
            "the target starts with a target list of its own, or this proves nothing"
        );

        // A source that has simply never moved its list: `ui` exists, the key
        // does not.
        let source = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
        settings_model::apply_to_tree(&mut tgt_user, &extract_categories(&source, &w.account_categories));

        let after = settings_model::project_hud(&hud_target_char_doc(), Some(&tgt_user));
        let val = |n: &str| after.entries.iter().find(|e| e.name == n).unwrap().value.clone();
        assert_eq!(val("target_x"), None, "the target's anchor is put back to EVE's default");
        assert_eq!(val("target_horizontal"), None, "and so is its orientation");
    }

    #[test]
    fn an_old_layout_presets_char_side_never_removes_the_targets_hud() {
        // THE DATA-LOSS CASE. A Layout preset saved before this branch has a
        // char.dat holding `windows` and `ui -> neocomButtonRawData` — not an
        // empty root, so the empty-root rule (which only ever covered the
        // account side) let it through. Its missing fightersDetachedPosition
        // and notification_badge_offset were then read as "the source is at
        // EVE's default" and DELETED from the target: any user with a Layout
        // preset from before this branch silently lost that character's
        // fighter-panel and badge positions.
        let w = aspect_writes(&[Aspect::Layout]);
        let old_preset_char = Value::Dict(vec![
            (b("windows"), Value::Dict(vec![(b("openWindows"), Value::Dict(vec![]))])),
            (
                b("ui"),
                Value::Dict(vec![(b("neocomButtonRawData"), wrapped(Value::List(vec![b("SOURCE-BAR")])))]),
            ),
        ]);

        let mut target = hud_target_char_doc();
        let extracted = extract_categories(&old_preset_char, &w.char_categories);
        settings_model::apply_to_tree(&mut target, &extracted);

        let hud = settings_model::project_hud(&target, None);
        let val = |n: &str| hud.entries.iter().find(|e| e.name == n).unwrap().value.clone();
        assert_eq!(val("fighter_x").as_deref(), Some("10"), "fighter x survives an old preset");
        assert_eq!(val("fighter_y").as_deref(), Some("20"), "fighter y survives an old preset");
        assert_eq!(val("badge_x").as_deref(), Some("1"), "badge x survives an old preset");
        assert_eq!(val("badge_y").as_deref(), Some("2"), "badge y survives an old preset");

        // Still applies what it DID capture — the old preset keeps working
        // char-only, which is the behaviour §4.4 promised all along.
        assert!(
            extracted.iter().any(|(c, v)| *c == Category::Layout && v.is_some()),
            "the preset's `windows` subtree is still copied"
        );
        assert!(
            extracted.iter().any(|(c, v)| *c == Category::NeocomButtons && v.is_some()),
            "the preset's neocom bar is still copied"
        );
    }

    #[test]
    fn an_account_side_of_only_removals_is_not_suppressed_as_a_no_op() {
        // source_side_empty feeds setup_preview's no-op suppression. A source
        // storing none of the account HUD keys yields one REMOVAL each, which
        // is real work — counting only present values here would silently kill
        // the removal path and half-apply the copy again. Routed through the
        // real function (not just extract_categories) so a "tidy-up" that
        // narrowed source_side_empty to count only present values would fail
        // this test, not just the assertion below it.
        let w = aspect_writes(&[Aspect::Layout]);
        let source_without_hud = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
        let extracted = extract_categories(&source_without_hud, &w.account_categories);
        assert_eq!(extracted.len(), 6, "one removal per account-side HUD key");

        let path = temp_file("hud-removals-only", &encode(&source_without_hud).unwrap());
        assert!(
            !source_side_empty(&path, &w.account_categories),
            "a removals-only account side must not be treated as a no-op"
        );
    }

    #[test]
    fn autofill_is_account_only() {
        let w = aspect_writes(&[Aspect::Autofill]);
        assert!(w.char_categories.is_empty());
        assert_eq!(w.account_categories, vec![Category::Autofill]);
        assert!(w.writes_account() && !w.writes_char());
    }

    #[test]
    fn keybinds_is_account_only() {
        let w = aspect_writes(&[Aspect::Keybinds]);
        assert!(w.char_categories.is_empty());
        assert_eq!(w.account_categories, vec![Category::Keybinds]);
        assert!(w.writes_account() && !w.writes_char());
    }

    #[test]
    fn probe_formations_write_the_account_side_only() {
        let w = aspect_writes(&[Aspect::ProbeFormations]);
        assert_eq!(w.account_categories, vec![Category::ProbeFormations]);
        assert!(w.char_categories.is_empty());
        assert!(!w.char_full_copy && !w.account_full_copy);
    }

    #[test]
    fn set_probe_formation_with_no_key_mints_it_at_id_zero() {
        // 61 of 175 corpus account files have no formations key at all — this
        // is the only create path, so the first-ever formation on one of them
        // must not fail before reaching `set_formation`, which mints the key.
        let bytes = encode(&Value::Dict(vec![(b("ui"), Value::Dict(vec![]))])).unwrap();
        let path = temp_file("probes-no-key", &bytes);
        let state = AppState::new();
        open_file(&state, Slot::User, path.to_str().unwrap()).unwrap();

        let f = set_probe_formation(&state, None, "first", vec![[1.0, 0.0, 0.0]], 1000.0).unwrap();
        assert_eq!(f.formations.len(), 1);
        assert_eq!(f.formations[0].id, 0, "0 is the first free id when none exist yet");
        assert_eq!(f.formations[0].name, "first");
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
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), Some(3), &[1, 2], &[Aspect::Overview]);
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
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), Some(3), &[1], &[Aspect::Overview]);
        assert_eq!(plan.account_writes.len(), 1);
        assert_eq!(plan.account_writes[0].collateral_char_ids, vec![2], "char 2 is collateral");
    }

    #[test]
    fn account_aspect_excludes_an_unpaired_target() {
        let cp = paths(&[1, 3, 4], "char");
        let up = paths(&[10, 20], "user");
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), Some(3), &[1, 4], &[Aspect::Autofill]);
        assert_eq!(plan.excluded.len(), 1);
        assert_eq!(plan.excluded[0].char_id, 4);
        assert_eq!(plan.account_writes.len(), 1, "only the paired target's account is written");
    }

    #[test]
    fn a_layout_copy_excludes_an_unpaired_target() {
        // Was `layout_only_includes_unpaired_targets_no_account_write`, which
        // asserted the reverse of the now-intended behaviour on this exact
        // setup (char 4 is the unpaired id `store_2accounts` already leaves
        // out) — updated in place rather than left contradicting the spec.
        let cp = paths(&[1, 3, 4], "char");
        let up = paths(&[10, 20], "user");
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), Some(3), &[1, 4], &[Aspect::Layout]);
        assert!(
            plan.excluded.iter().any(|e| e.char_id == 4 && e.reason.contains("No account paired")),
            "an unpaired target cannot receive the account-side HUD fields"
        );
        assert_eq!(plan.char_writes.len(), 1, "only the paired target receives a char write");
        assert_eq!(plan.char_writes[0].char_id, 1, "the paired target, not the excluded one");
    }

    #[test]
    fn target_on_source_account_skips_the_account_write() {
        // Source char 1 (account 10). Target char 2, same account 10.
        let cp = paths(&[1, 2], "char");
        let up = paths(&[10], "user");
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), Some(1), &[2], &[Aspect::Overview]);
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
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), Some(4), &[3], &[Aspect::Overview]);
        assert!(plan.source_error.is_some());
        assert!(plan.char_writes.is_empty() && plan.account_writes.is_empty());
    }

    #[test]
    fn a_source_whose_account_file_is_missing_is_a_source_error() {
        // Char 3 is paired to account 20, but this folder holds no file for it.
        let cp = paths(&[1, 3], "char");
        let up = paths(&[10], "user");
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), Some(3), &[1], &[Aspect::Overview]);
        assert!(
            plan.source_error.as_deref().unwrap_or("").contains("account file was not found"),
            "got: {:?}",
            plan.source_error
        );
        assert!(plan.char_writes.is_empty() && plan.account_writes.is_empty());
    }

    #[test]
    fn a_target_whose_account_file_is_missing_is_excluded() {
        // Target 1 is paired to account 10, whose file is not in this folder.
        let cp = paths(&[1, 3], "char");
        let up = paths(&[20], "user");
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), Some(3), &[1], &[Aspect::Overview]);
        assert_eq!(plan.excluded.len(), 1);
        assert!(plan.excluded[0].reason.contains("Account file not found"), "got: {}", plan.excluded[0].reason);
        assert!(plan.char_writes.is_empty(), "an excluded target gets no char write either");
    }

    #[test]
    fn a_target_with_no_character_file_in_the_folder_is_excluded() {
        let cp = paths(&[3], "char"); // char 1 has no file here
        let up = paths(&[10, 20], "user");
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), Some(3), &[1], &[Aspect::Overview]);
        assert_eq!(plan.excluded.len(), 1);
        assert!(plan.excluded[0].reason.contains("Character file not found"), "got: {}", plan.excluded[0].reason);
    }

    #[test]
    fn an_empty_target_list_plans_nothing_and_is_not_an_error() {
        let cp = paths(&[1, 3], "char");
        let up = paths(&[10, 20], "user");
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), Some(3), &[], &[Aspect::Overview]);
        assert!(plan.char_writes.is_empty() && plan.account_writes.is_empty() && plan.excluded.is_empty());
        assert!(plan.source_error.is_none(), "nothing to do is not a source problem");
    }

    #[test]
    fn a_repeated_target_is_planned_once() {
        let cp = paths(&[1, 3], "char");
        let up = paths(&[10, 20], "user");
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), Some(3), &[1, 1], &[Aspect::Overview]);
        assert_eq!(plan.char_writes.len(), 1, "one write, not two — each write backs the target up");
        assert_eq!(plan.account_writes.len(), 1);
    }

    #[test]
    fn resolution_mismatch_flagged_for_layout_when_screens_differ() {
        let cp = paths(&[1, 3], "char");
        let up = paths(&[10, 20], "user");
        let mut res = HashMap::new();
        res.insert(3u64, (2560i64, 1440i64)); // source
        res.insert(1u64, (1920i64, 1080i64)); // target differs
        let plan = plan_setup(&cp, &up, &store_2accounts(), &res, Some(3), &[1], &[Aspect::Layout]);
        assert!(plan.char_writes[0].resolution_mismatch);
    }

    #[test]
    fn a_preset_source_needs_no_pairing_and_excludes_nobody() {
        // Two targets on two different accounts, and NO source character.
        let cp = paths(&[1, 2], "char");
        let up = paths(&[10, 20], "user");
        let mut store = accounts::AccountsStore::default();
        store.accounts.insert(10, accounts::Account { alias: None, characters: vec![1] });
        store.accounts.insert(20, accounts::Account { alias: None, characters: vec![2] });
        let plan = plan_setup(&cp, &up, &store, &HashMap::new(), None, &[1, 2], &[Aspect::Overview]);
        assert!(plan.source_error.is_none(), "a preset source needs no paired account");
        assert_eq!(plan.char_writes.len(), 2, "both targets get their overview widths");
        assert_eq!(plan.account_writes.len(), 2, "neither account is skipped as 'the source's'");
        assert!(plan.excluded.is_empty());
    }

    #[test]
    fn a_preset_source_still_excludes_an_unpaired_target() {
        let cp = paths(&[1, 2], "char");
        let up = paths(&[10], "user");
        let mut store = accounts::AccountsStore::default();
        store.accounts.insert(10, accounts::Account { alias: None, characters: vec![1] }); // char 2 unpaired
        let plan = plan_setup(&cp, &up, &store, &HashMap::new(), None, &[1, 2], &[Aspect::Autofill]);
        assert_eq!(plan.excluded.len(), 1);
        assert_eq!(plan.excluded[0].char_id, 2);
    }

    #[test]
    fn a_preset_source_warns_on_no_resolution_mismatch() {
        // With no source character there is no source resolution, so the
        // off-screen warning is correctly silent. Target 1 is paired to
        // account 10 — layout now writes the account file too, so an
        // unpaired target would be excluded before resolution is even
        // considered (see a_layout_copy_excludes_an_unpaired_target).
        let cp = paths(&[1], "char");
        let up = paths(&[10], "user");
        let mut store = accounts::AccountsStore::default();
        store.accounts.insert(10, accounts::Account { alias: None, characters: vec![1] });
        let mut res = HashMap::new();
        res.insert(1u64, (1920i64, 1080i64));
        let plan = plan_setup(&cp, &up, &store, &res, None, &[1], &[Aspect::Layout]);
        assert_eq!(plan.char_writes.len(), 1);
        assert!(!plan.char_writes[0].resolution_mismatch);
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

    fn orphaned_char_bytes() -> Vec<u8> {
        fn bb(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        fn ts() -> Value { Value::Long(vec![0u8; 8]) }
        fn geom(x: i64) -> Value { Value::Tuple(vec![Value::Int(x), Value::Int(0), Value::Int(100), Value::Int(80), Value::Int(2560), Value::Int(1440)]) }
        // Live stack C(m1, m2) plus one orphan frame "43".
        encode(&Value::Dict(vec![(bb("windows"), Value::Dict(vec![
            (bb("windowSizesAndPositions_1"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (bb("m1"), geom(0)), (bb("m2"), geom(0)), (bb("C"), geom(0)), (bb("43"), geom(0)),
            ])])),
            (bb("openWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (bb("m1"), Value::Bool(true)), (bb("43"), Value::Bool(true)),
            ])])),
            (bb("stacksWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![(bb("m1"), bb("C")), (bb("m2"), bb("C"))])])),
        ]))])).unwrap()
    }

    #[test]
    fn delete_orphans_reprojects_and_reshares() {
        let path = temp_file("stack-orphans", &orphaned_char_bytes());
        let state = AppState::new();
        open_file(&state, Slot::Char, path.to_str().unwrap()).unwrap();

        let wl = stack_delete_orphans(&state).unwrap();
        // The orphan is gone from the projection; the live stack is untouched.
        assert!(!wl.windows.iter().any(|w| w.id == "43"));
        assert_eq!(wl.stacks.len(), 1);
        assert_eq!(wl.stacks[0].members, vec!["m1".to_string(), "m2".to_string()]);
        // Doc still encodes/decodes (reshare ran without corrupting the tree).
        let guard = state.char.lock().unwrap();
        let bytes = blue_marshal::encode(&guard.as_ref().unwrap().value).unwrap();
        assert_eq!(blue_marshal::decode(&bytes).unwrap(), guard.as_ref().unwrap().value);
    }

    #[test]
    fn delete_orphans_on_a_clean_file_is_a_no_op_not_an_error() {
        let path = temp_file("stack-noorphans", &stacked_char_bytes());
        let state = AppState::new();
        open_file(&state, Slot::Char, path.to_str().unwrap()).unwrap();
        let wl = stack_delete_orphans(&state).unwrap();
        assert_eq!(wl.stacks.len(), 1);
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
        let source = BatchSource::Character { path: src };
        let results = setup_apply(&roots, &app_dir, &source, &tgt, &[Aspect::Overview], false).unwrap();

        // One char write (widths -> char 200, ok) and one account write (overview
        // -> read-only user 600, fails) — the failure did not halt the char write.
        let char_ok = results.iter().any(|r| r.path.contains("core_char_200") && r.ok);
        let acct_fail = results.iter().any(|r| r.path.contains("core_user_600") && !r.ok);
        assert!(char_ok, "char widths write succeeded");
        assert!(acct_fail, "read-only account write failed but was reported, not panicked");
    }

    /// Minimal but non-empty documents to cut a pruned preset from. `create`
    /// treats an empty-root open document as a side that is not open, so these
    /// stand in for "the user's real files" in tests whose subject is the
    /// apply/refusal path rather than the cut itself.
    fn pruned_preset_char_side() -> Value {
        Value::Dict(vec![(b("windows"), Value::Dict(vec![(b("marker"), Value::Bool(true))]))])
    }
    fn pruned_preset_user_side() -> Value {
        Value::Dict(vec![(b("ui"), Value::Dict(vec![]))])
    }

    #[test]
    fn everything_from_a_pruned_preset_is_refused() {
        // A full copy built on a three-key document would wipe the target's
        // whole file. The UI hides the option; the backend refuses it too.
        let data = std::env::temp_dir().join(format!("eve-preset-apply-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(&data).unwrap();
        // Real (if small) documents on both sides: `create` refuses to cut a
        // preset from an empty-root slot now, so an empty doc no longer works
        // as a shortcut for "produce a pruned preset". Pruning is what makes
        // this one partial, not the source being empty.
        let (cdoc, udoc) = (pruned_preset_char_side(), pruned_preset_user_side());
        crate::presets::create(
            &data,
            "Partial",
            &[Aspect::Layout],
            crate::presets::CreateInput { char_doc: Some(&cdoc), user_doc: Some(&udoc) },
            false,
        )
        .unwrap();
        let dir = crate::presets::preset_path(&data, "Partial").unwrap();
        let source = BatchSource::Preset {
            dir: dir.to_string_lossy().into_owned(),
            anchor_dir: String::new(),
        };
        let plan = setup_preview(&[], &data, &source, &[], &[Aspect::Everything], false);
        assert!(
            plan.source_error.as_deref().unwrap_or("").contains("only part"),
            "got: {:?}",
            plan.source_error
        );
    }

    #[test]
    fn a_missing_preset_directory_is_a_source_error() {
        let source = BatchSource::Preset {
            dir: "/no/such/preset".into(),
            anchor_dir: String::new(),
        };
        let plan = setup_preview(&[], Path::new("/tmp"), &source, &[], &[Aspect::Layout], false);
        assert!(plan.source_error.is_some());
    }

    #[test]
    fn preset_apply_writes_the_presets_settings_to_the_target() {
        fn bb(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        let base = std::env::temp_dir().join(format!("app-preset-apply-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let prof = base.join("root").join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        std::fs::create_dir_all(&prof).unwrap();

        // Target character file: distinct "windows" content, to prove it gets
        // overwritten rather than merely leaving the call reporting Ok.
        let target_doc = Value::Dict(vec![(bb("windows"), Value::Dict(vec![(bb("marker"), bb("ORIGINAL"))]))]);
        std::fs::write(prof.join("core_char_700.dat"), encode(&target_doc).unwrap()).unwrap();

        let app_dir = base.join("appdata");
        std::fs::create_dir_all(&app_dir).unwrap();

        // Layout now writes the account file too, so target 700 must be
        // paired — an unpaired target is excluded outright (see
        // a_layout_copy_excludes_an_unpaired_target). Its account's file
        // just needs to exist and decode; this test's own claim is about the
        // char side only.
        let mut store = accounts::AccountsStore::default();
        store.accounts.insert(750, accounts::Account { alias: None, characters: vec![700] });
        std::fs::write(app_dir.join("accounts.json"), serde_json::to_vec(&store).unwrap()).unwrap();
        std::fs::write(prof.join("core_user_750.dat"), encode(&Value::Dict(vec![])).unwrap()).unwrap();

        // A Layout-only preset holding distinctive windows content. The
        // account side needs a real doc now that Layout writes it too —
        // `create` refuses a side that is absent OR an empty root.
        let preset_char_doc = Value::Dict(vec![(bb("windows"), Value::Dict(vec![(bb("marker"), bb("FROM_PRESET"))]))]);
        let preset_user_doc = pruned_preset_user_side();
        crate::presets::create(
            &app_dir,
            "LayoutOnly",
            &[Aspect::Layout],
            crate::presets::CreateInput { char_doc: Some(&preset_char_doc), user_doc: Some(&preset_user_doc) },
            false,
        )
        .unwrap();
        let pdir = crate::presets::preset_path(&app_dir, "LayoutOnly").unwrap();

        let roots = vec![base.join("root")];
        let source = BatchSource::Preset {
            dir: pdir.to_string_lossy().into_owned(),
            anchor_dir: prof.to_string_lossy().into_owned(),
        };
        let tgt = vec![prof.join("core_char_700.dat").to_string_lossy().into_owned()];
        let results = setup_apply(&roots, &app_dir, &source, &tgt, &[Aspect::Layout], false).unwrap();
        assert!(results.iter().all(|r| r.ok), "results: {results:?}");

        let bytes = std::fs::read(prof.join("core_char_700.dat")).unwrap();
        let val = blue_marshal::decode(&bytes).unwrap();
        let extracted = extract_categories(&val, &[Category::Layout]);
        assert_eq!(
            extracted,
            vec![(Category::Layout, Some(Value::Dict(vec![(bb("marker"), bb("FROM_PRESET"))])))],
            "target must carry the preset's windows content, not merely report ok"
        );
    }

    #[test]
    fn everything_from_a_pruned_preset_is_refused_by_setup_apply_and_leaves_target_untouched() {
        fn bb(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        let base = std::env::temp_dir().join(format!("app-preset-everything-refused-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let prof = base.join("root").join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        std::fs::create_dir_all(&prof).unwrap();

        let original_bytes =
            encode(&Value::Dict(vec![(bb("windows"), Value::Dict(vec![(bb("marker"), bb("UNTOUCHED"))]))])).unwrap();
        std::fs::write(prof.join("core_char_701.dat"), &original_bytes).unwrap();

        let app_dir = base.join("appdata");
        std::fs::create_dir_all(&app_dir).unwrap();

        // Pair 701 to an account and give that account a file. Without this the
        // target is UNPAIRED, so `Aspect::Everything` (which writes the account
        // side) makes `plan_setup` exclude it and no write is ever planned --
        // "the target is untouched" would then hold with the guard deleted.
        let account_bytes =
            encode(&Value::Dict(vec![(bb("ui"), Value::Dict(vec![(bb("marker"), bb("ACCOUNT_UNTOUCHED"))]))])).unwrap();
        std::fs::write(prof.join("core_user_970.dat"), &account_bytes).unwrap();
        let mut store = accounts::AccountsStore::default();
        store.accounts.insert(970, accounts::Account { alias: None, characters: vec![701] });
        std::fs::write(app_dir.join("accounts.json"), serde_json::to_vec(&store).unwrap()).unwrap();

        let (cdoc, udoc) = (pruned_preset_char_side(), pruned_preset_user_side());
        crate::presets::create(
            &app_dir,
            "Pruned",
            &[Aspect::Layout],
            crate::presets::CreateInput { char_doc: Some(&cdoc), user_doc: Some(&udoc) },
            false,
        )
        .unwrap();
        let pdir = crate::presets::preset_path(&app_dir, "Pruned").unwrap();

        let roots = vec![base.join("root")];
        let source = BatchSource::Preset {
            dir: pdir.to_string_lossy().into_owned(),
            anchor_dir: prof.to_string_lossy().into_owned(),
        };
        let tgt = vec![prof.join("core_char_701.dat").to_string_lossy().into_owned()];
        let err = setup_apply(&roots, &app_dir, &source, &tgt, &[Aspect::Everything], false).unwrap_err();
        assert_eq!(err.code, "source");
        assert!(err.message.contains("only part"), "must be the pruned-preset guard's message, got: {}", err.message);
        assert!(!err.message.contains("empty"), "must not be mistaken for the empty-document guard");

        let after = std::fs::read(prof.join("core_char_701.dat")).unwrap();
        assert_eq!(after, original_bytes, "target must be untouched when Everything is refused");
        let account_after = std::fs::read(prof.join("core_user_970.dat")).unwrap();
        assert_eq!(account_after, account_bytes, "the account side must be untouched too");
    }

    #[test]
    fn a_full_marked_preset_with_an_empty_side_is_refused_by_setup_apply_and_leaves_target_untouched() {
        // The data-safety bug: `full` is set from the save-time request and
        // never cross-checked against what actually got written. A Layout-only
        // preset's user.dat is Value::Dict([]) by construction (see
        // an_autofill_preset_builds_its_parent_dict's mirror case in
        // presets.rs), so a hand-built (or pre-fix, or hand-edited-import)
        // preset can claim `full: true` while one side is empty. Applying
        // Everything against it must be refused, and the target must be
        // byte-for-byte untouched -- that second assertion is load-bearing:
        // `full_copy_to` backs the target up before writing, so a bug here
        // would still report "ok" and silently wipe the file, recoverable only
        // via the backup the user was never told to look for.
        fn bb(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        let base = std::env::temp_dir().join(format!("app-preset-empty-full-refused-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let prof = base.join("root").join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        std::fs::create_dir_all(&prof).unwrap();

        let original_bytes =
            encode(&Value::Dict(vec![(bb("windows"), Value::Dict(vec![(bb("marker"), bb("UNTOUCHED"))]))])).unwrap();
        std::fs::write(prof.join("core_char_702.dat"), &original_bytes).unwrap();

        let app_dir = base.join("appdata");
        std::fs::create_dir_all(&app_dir).unwrap();

        // Pair 702 to an account, for the reason given in the pruned-preset
        // sibling above: an unpaired target is excluded from an Everything plan,
        // which would make the untouched-target assertion vacuous. It also puts
        // a real file on the account side -- the side an empty full preset wipes.
        let account_bytes =
            encode(&Value::Dict(vec![(bb("ui"), Value::Dict(vec![(bb("marker"), bb("ACCOUNT_UNTOUCHED"))]))])).unwrap();
        std::fs::write(prof.join("core_user_971.dat"), &account_bytes).unwrap();
        let mut store = accounts::AccountsStore::default();
        store.accounts.insert(971, accounts::Account { alias: None, characters: vec![702] });
        std::fs::write(app_dir.join("accounts.json"), serde_json::to_vec(&store).unwrap()).unwrap();

        // Hand-build the preset folder directly (bypassing `presets::create`,
        // which now refuses this shape itself) so this test pins the
        // independent ops.rs-side guard rather than only the belt-and-braces one.
        let pdir = crate::presets::presets_dir(&app_dir).join("BadFull");
        std::fs::create_dir_all(&pdir).unwrap();
        let char_doc = Value::Dict(vec![(bb("windows"), Value::Dict(vec![(bb("marker"), bb("FROM_PRESET"))]))]);
        std::fs::write(pdir.join(crate::presets::CHAR_FILE), encode(&char_doc).unwrap()).unwrap();
        std::fs::write(pdir.join(crate::presets::USER_FILE), encode(&Value::Dict(vec![])).unwrap()).unwrap();
        std::fs::write(pdir.join(crate::presets::MARKER_FILE), br#"{"full":true}"#).unwrap();

        let roots = vec![base.join("root")];
        let source = BatchSource::Preset {
            dir: pdir.to_string_lossy().into_owned(),
            anchor_dir: prof.to_string_lossy().into_owned(),
        };
        let tgt = vec![prof.join("core_char_702.dat").to_string_lossy().into_owned()];
        let err = setup_apply(&roots, &app_dir, &source, &tgt, &[Aspect::Everything], false).unwrap_err();
        assert_eq!(err.code, "source");
        assert!(err.message.contains("empty"), "must be the empty-document guard's message, got: {}", err.message);
        assert!(!err.message.contains("only part"), "must not be mistaken for the pruned-preset guard");

        let after = std::fs::read(prof.join("core_char_702.dat")).unwrap();
        assert_eq!(after, original_bytes, "target must be untouched when Everything is refused");
        let account_after = std::fs::read(prof.join("core_user_971.dat")).unwrap();
        assert_eq!(account_after, account_bytes, "the account side must be untouched too");
    }

    #[test]
    fn setup_apply_refuses_a_plan_that_carries_a_source_error() {
        // Overview writes account-side and the source character is paired to
        // nothing, so plan_setup reports a source error. setup_apply must
        // surface that as an Err rather than quietly applying the char side.
        let base = std::env::temp_dir().join(format!("app-apply-source-error-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let prof = base.join("root").join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        std::fs::create_dir_all(&prof).unwrap();

        let original = encode(&Value::Dict(vec![])).unwrap();
        std::fs::write(prof.join("core_char_810.dat"), &original).unwrap();
        std::fs::write(prof.join("core_char_811.dat"), &original).unwrap();

        // No accounts.json at all: neither character is paired.
        let app_dir = base.join("appdata");
        std::fs::create_dir_all(&app_dir).unwrap();

        let roots = vec![base.join("root")];
        let source =
            BatchSource::Character { path: prof.join("core_char_810.dat").to_string_lossy().into_owned() };
        let tgt = vec![prof.join("core_char_811.dat").to_string_lossy().into_owned()];
        let err = setup_apply(&roots, &app_dir, &source, &tgt, &[Aspect::Overview], false).unwrap_err();
        assert_eq!(err.code, "source");
        assert!(err.message.contains("no paired account"), "got: {}", err.message);
        assert_eq!(
            std::fs::read(prof.join("core_char_811.dat")).unwrap(),
            original,
            "a refused apply writes nothing"
        );
    }

    #[test]
    fn a_preset_source_outside_the_presets_dir_via_traversal_is_refused() {
        // The containment guard is component-wise (`pdir.parent() ==
        // presets_dir`), not the old lexical `starts_with`, which
        // `<presets dir>/../../anything` passed despite escaping the directory.
        let data = std::env::temp_dir().join(format!("app-preset-traversal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(&data).unwrap();
        let escape = crate::presets::presets_dir(&data).join("..").join("escape");
        let source = BatchSource::Preset { dir: escape.to_string_lossy().into_owned(), anchor_dir: String::new() };
        let plan = setup_preview(&[], &data, &source, &[], &[Aspect::Layout], false);
        assert!(plan.source_error.is_some(), "a preset path outside the presets dir must be refused");
    }

    #[test]
    fn character_source_account_file_comes_from_its_own_profile_not_another_ones() {
        // Pins the FIX-1 regression: `resolve_source`'s inner `scoped_files`
        // call for a character source's account file must honour the caller's
        // real `allow_other_folders`, not a hardcoded `true`. Two profile
        // folders under one root both carry an account file for the SAME
        // account id (profiles are copies of each other in real installs) —
        // with `allow_other_folders = false` the source's own profile's copy
        // must win, never the other profile's.
        fn bb(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        let base = std::env::temp_dir().join(format!("app-source-profile-scope-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let install = base.join("root").join("c_eve_sharedcache_tq_tranquility");
        let default_prof = install.join("settings_Default");
        let zulu_prof = install.join("settings_Zulu");
        std::fs::create_dir_all(&default_prof).unwrap();
        std::fs::create_dir_all(&zulu_prof).unwrap();

        fn overview_doc(marker: &str) -> Value {
            Value::Dict(vec![(
                Value::Bytes(b"overview".to_vec()),
                Value::Dict(vec![(Value::Bytes(b"marker".to_vec()), Value::Bytes(marker.as_bytes().to_vec()))]),
            )])
        }
        let minimal = || Value::Dict(vec![]);

        // Source char 800 on account 900 lives in Default. Default's own
        // account-900 file carries "FROM_DEFAULT"; Zulu also has an
        // account-900 file (a stale copy of the same account) carrying
        // different content — that copy must never be the one read.
        std::fs::write(default_prof.join("core_char_800.dat"), encode(&minimal()).unwrap()).unwrap();
        std::fs::write(default_prof.join("core_user_900.dat"), encode(&overview_doc("FROM_DEFAULT")).unwrap()).unwrap();
        std::fs::write(zulu_prof.join("core_user_900.dat"), encode(&overview_doc("FROM_ZULU")).unwrap()).unwrap();

        // Target char 801 on account 950, also in Default (so it stays
        // visible with allow_other_folders = false).
        std::fs::write(default_prof.join("core_char_801.dat"), encode(&minimal()).unwrap()).unwrap();
        std::fs::write(default_prof.join("core_user_950.dat"), encode(&minimal()).unwrap()).unwrap();

        let app_dir = base.join("appdata");
        std::fs::create_dir_all(&app_dir).unwrap();
        let mut store = accounts::AccountsStore::default();
        store.accounts.insert(900, accounts::Account { alias: None, characters: vec![800] });
        store.accounts.insert(950, accounts::Account { alias: None, characters: vec![801] });
        std::fs::write(app_dir.join("accounts.json"), serde_json::to_vec(&store).unwrap()).unwrap();

        let roots = vec![base.join("root")];
        let src = default_prof.join("core_char_800.dat").to_string_lossy().into_owned();
        let tgt = vec![default_prof.join("core_char_801.dat").to_string_lossy().into_owned()];
        let source = BatchSource::Character { path: src };
        let results = setup_apply(&roots, &app_dir, &source, &tgt, &[Aspect::Overview], false).unwrap();
        assert!(
            results.iter().any(|r| r.path.contains("core_user_950") && r.ok),
            "results: {results:?}"
        );

        let bytes = std::fs::read(default_prof.join("core_user_950.dat")).unwrap();
        let val = blue_marshal::decode(&bytes).unwrap();
        let extracted = extract_categories(&val, &[Category::Overview]);
        assert_eq!(
            extracted,
            vec![(Category::Overview, Some(Value::Dict(vec![(bb("marker"), bb("FROM_DEFAULT"))])))],
            "must carry the source's OWN profile's account settings, not another profile's"
        );
    }

    /// root -> { b"windows": {}, b"ui": {} } — sections present, no HUD keys.
    fn hud_char_bytes() -> Vec<u8> {
        let doc = blue_marshal::Value::Dict(vec![
            (blue_marshal::Value::Bytes(b"windows".to_vec()), blue_marshal::Value::Dict(vec![])),
            (blue_marshal::Value::Bytes(b"ui".to_vec()), blue_marshal::Value::Dict(vec![])),
        ]);
        blue_marshal::encode(&doc).expect("encode fixture")
    }

    #[test]
    fn hud_projects_and_sets_the_ship_offset() {
        // A character document with an empty `windows` section: the projection
        // reports ship_offset absent, and the first write mints it.
        let state = AppState::new();
        let path = temp_file("hud-char", &hud_char_bytes());
        open_file(&state, Slot::Char, &path.to_string_lossy()).expect("open");

        let hud = hud_layout(&state).expect("project");
        let e = hud.entries.iter().find(|e| e.name == "ship_offset").expect("entry");
        assert!(e.value.is_none());

        let hud = set_hud_field(&state, "ship_offset", "-77").expect("set");
        let e = hud.entries.iter().find(|e| e.name == "ship_offset").expect("entry");
        assert_eq!(e.value.as_deref(), Some("-77"));

        // The document still encodes and round-trips (reshare ran cleanly).
        let guard = state.char.lock().unwrap();
        let doc = guard.as_ref().expect("open");
        let bytes = blue_marshal::encode(&doc.value).expect("encode");
        assert_eq!(blue_marshal::decode(&bytes).unwrap(), doc.value);
    }

    #[test]
    fn hud_without_a_character_file_is_an_error() {
        let state = AppState::new();
        assert!(hud_layout(&state).is_err());
    }

    /// char -> ui -> neocomButtonRawData `(ts, List)` with two `utillib.KeyVal`
    /// buttons, plus an Original to reset to. Mirrors `neocom.rs`'s own fixture.
    fn neocom_char_bytes() -> Vec<u8> {
        let ts = || Value::Long(vec![0u8; 8]);
        let button = |id: &str, btn_type: i64, icon: &str| Value::Instance {
            class: Box::new(b("utillib.KeyVal")),
            state: Box::new(Value::Dict(vec![
                (b("btnType"), Value::Int(btn_type)),
                (b("children"), Value::None),
                (b("iconPath"), b(icon)),
                (b("id"), b(id)),
            ])),
        };
        let doc = Value::Dict(vec![(
            b("ui"),
            Value::Dict(vec![
                (
                    b("neocomButtonRawData"),
                    Value::Tuple(vec![
                        ts(),
                        Value::List(vec![button("chat", 10, "chat.png"), button("wallet", 1, "wallet.png")]),
                    ]),
                ),
                (
                    b("neocomButtonRawDataOriginal"),
                    Value::Tuple(vec![ts(), Value::Tuple(vec![button("chat", 10, "chat.png")])]),
                ),
            ]),
        )]);
        encode(&doc).expect("encode neocom fixture")
    }

    #[test]
    fn neocom_without_a_character_file_is_an_error() {
        let state = AppState::new();
        assert_eq!(neocom_bar(&state).unwrap_err().code, "no_document");
        assert_eq!(neocom_reorder(&state, vec![0]).unwrap_err().code, "no_document");
        assert_eq!(neocom_remove(&state, 0).unwrap_err().code, "no_document");
        assert_eq!(neocom_add(&state, "chat", 1, "chat.png").unwrap_err().code, "no_document");
        assert_eq!(neocom_reset(&state).unwrap_err().code, "no_document");
    }

    /// An unpaired character is normal, so this is an empty list, not an error —
    /// unlike `neocom_bar`, which needs the character document and refuses without
    /// one.
    #[test]
    fn chat_panels_is_empty_without_an_account_file() {
        let state = AppState::new();
        assert!(chat_panels(&state).unwrap().is_empty());
    }

    #[test]
    fn setting_a_chat_split_without_an_account_file_errors() {
        let state = AppState::new();
        // Contrast with chat_panels, which returns an empty list: reading an
        // unpaired character is normal, but there is nowhere to write.
        let err = set_chat_splits(&state, vec!["chatchannel_local".into()], Some(120), None).unwrap_err();
        assert_eq!(err.code, "no_document");
    }

    #[test]
    fn neocom_edits_refuse_a_read_only_document() {
        // A valid stream the encoder re-emits differently (Int 1 as INT8) loads
        // ReadOnly — the same fixture document.rs uses. What the file HOLDS is
        // irrelevant: the guard fires before the edit runs.
        let state = AppState::new();
        let path = temp_file("neocom-readonly", &[0x7E, 0, 0, 0, 0, 0x06, 0x01]);
        open_file(&state, Slot::Char, &path.to_string_lossy()).expect("open");
        assert_eq!(neocom_reorder(&state, vec![0]).unwrap_err().code, "read_only");
        assert_eq!(neocom_remove(&state, 0).unwrap_err().code, "read_only");
        assert_eq!(neocom_add(&state, "chat", 1, "chat.png").unwrap_err().code, "read_only");
        assert_eq!(neocom_reset(&state).unwrap_err().code, "read_only");
    }

    #[test]
    fn neocom_reorder_and_add_land_through_the_command_layer() {
        let state = AppState::new();
        let path = temp_file("neocom-happy", &neocom_char_bytes());
        open_file(&state, Slot::Char, &path.to_string_lossy()).expect("open");

        let bar = neocom_bar(&state).expect("project");
        assert_eq!(bar.buttons.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(), ["chat", "wallet"]);
        assert_eq!(bar.original.len(), 1, "Original feeds the addable set");

        let bar = neocom_reorder(&state, vec![1, 0]).expect("reorder");
        assert_eq!(bar.buttons.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(), ["wallet", "chat"]);

        let bar = neocom_add(&state, "market", 3, "market.png").expect("add");
        assert_eq!(bar.buttons.last().map(|b| b.id.as_str()), Some("market"));

        let bar = neocom_reset(&state).expect("reset");
        assert_eq!(bar.buttons.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(), ["chat"]);

        // And the document still round-trips after the reshare each edit runs.
        let guard = state.char.lock().unwrap();
        let doc = guard.as_ref().expect("open");
        let bytes = encode(&doc.value).expect("encode");
        assert_eq!(blue_marshal::decode(&bytes).unwrap(), doc.value);
    }

    /// A user file with one preset and one tab — enough for every pack section
    /// the command layer touches. Mirrors `user_doc()` in `overview_pack.rs`.
    fn pack_user_fixture() -> Value {
        let bb = |s: &str| Value::Bytes(s.as_bytes().to_vec());
        let ts = || Value::Long(vec![0u8; 8]);
        let preset = Value::Dict(vec![
            (bb("groups"), Value::List(vec![Value::Int(25)])),
            (bb("filteredStates"), Value::List(vec![])),
            (bb("alwaysShownStates"), Value::List(vec![])),
        ]);
        let tab = Value::Dict(vec![
            (bb("color"), Value::None),
            (bb("bracket"), bb("Friendly")),
            (bb("name"), Value::StrUcs2("Fleet".into())),
            (bb("overview"), bb("Friendly")),
        ]);
        Value::Dict(vec![(bb("overview"), Value::Dict(vec![
            (bb("overviewProfilePresets"), Value::Tuple(vec![ts(), Value::Dict(vec![(bb("Friendly"), preset)])])),
            (bb("tabsettings_new"), Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab)])])),
            (bb("overviewColumns"), Value::List(vec![bb("NAME")])),
        ]))])
    }

    #[test]
    fn pack_round_trip_through_the_commands() {
        let upath = temp_file("pack-roundtrip", &encode(&pack_user_fixture()).unwrap());
        let dir = upath.parent().unwrap().to_path_buf();
        let state = AppState::new();
        open_file(&state, Slot::User, upath.to_str().unwrap()).unwrap();

        let out = dir.join("pack.yaml");
        let report = pack_export(&state, out.to_str().unwrap()).unwrap();
        assert!(report.applied.iter().any(|s| s == "presets"), "{report:?}");

        let summary = pack_preview(out.to_str().unwrap()).unwrap();
        assert!(summary.sections.iter().any(|(name, n)| name == "presets" && *n > 0));

        // Import a pack whose preset and tab NAMES differ from what the open
        // account already has, and prove the document actually changed to
        // match it -- a stubbed pack_import that never calls apply_pack would
        // leave `before == after` and pass a same-document round trip, so the
        // fixture below must not share a name with `pack_user_fixture()`.
        let before = overview_columns(&state).unwrap();
        assert_eq!(before.presets[0].name, "Friendly");
        // `pack_user_fixture`'s tab keys its "name" field as Bytes. That used to
        // fall back to "Tab {index}" because the projection's key predicate had
        // no Bytes arm while the tab WRITER's did; one shared `treewalk::key_is`
        // now covers both, so the name reads. Real files key it StrTable(52).
        assert_eq!(before.tabs[0].name, "Fleet");

        let differing = dir.join("differing.yaml");
        fs::write(&differing, DIFFERING_PACK).unwrap();
        pack_import(&state, differing.to_str().unwrap()).unwrap();

        let after = overview_columns(&state).unwrap();
        assert_eq!(after.presets.len(), 1);
        assert_eq!(after.presets[0].name, "Neutral", "import replaced the preset with the pack's");
        assert_eq!(after.tabs[0].name, "Scouts", "import renamed the tab from the pack");
    }

    /// A hand-written pack (published shape, see `overview_pack::tests::FIXTURE`)
    /// carrying a preset and tab name `pack_user_fixture()` does not have, so
    /// importing it is only provably applied if the document's names change.
    const DIFFERING_PACK: &str = r#"presets:
- - Neutral
  - - - alwaysShownStates
      - []
    - - filteredStates
      - []
    - - groups
      - - 30
tabSetup:
- - 0
  - - - bracket
      - Neutral
    - - name
      - Scouts
    - - overview
      - Neutral
"#;

    #[test]
    fn pack_preview_rejects_a_non_pack_file() {
        let junk = temp_file("pack-junk", b"").parent().unwrap().join("junk.yaml");
        fs::write(&junk, "some: mapping\n").unwrap();
        let err = pack_preview(junk.to_str().unwrap()).unwrap_err();
        assert_eq!(err.code, "not_a_pack");
    }

    #[test]
    fn pack_preview_rejects_malformed_yaml() {
        let junk = temp_file("pack-malformed", b"").parent().unwrap().join("malformed.yaml");
        fs::write(&junk, "presets:\n- - unclosed: [\n").unwrap();
        let err = pack_preview(junk.to_str().unwrap()).unwrap_err();
        assert_eq!(err.code, "yaml", "{err:?}");
    }

    #[test]
    fn pack_preview_missing_file_is_an_io_error() {
        let dir = temp_file("pack-missing", b"").parent().unwrap().to_path_buf();
        let missing = dir.join("does-not-exist.yaml");
        let err = pack_preview(missing.to_str().unwrap()).unwrap_err();
        assert_eq!(err.code, "io");
    }

    #[test]
    fn pack_import_without_an_open_account_errors() {
        let p = temp_file("pack-nodoc", b"").parent().unwrap().join("pack.yaml");
        fs::write(&p, "backgroundStates:\n- 9\n").unwrap();
        let state = AppState::new();
        let err = pack_import(&state, p.to_str().unwrap()).unwrap_err();
        assert_eq!(err.code, "no_document");
    }

    #[test]
    fn pack_export_without_an_open_account_errors() {
        let out = temp_file("pack-export-nodoc", b"").parent().unwrap().join("pack.yaml");
        let state = AppState::new();
        let err = pack_export(&state, out.to_str().unwrap()).unwrap_err();
        assert_eq!(err.code, "no_document");
    }

    #[test]
    fn pack_export_rejects_an_account_with_no_overview_settings() {
        let bb = |s: &str| Value::Bytes(s.as_bytes().to_vec());
        let doc = Value::Dict(vec![(bb("other"), Value::Int(1))]);
        let upath = temp_file("pack-export-empty", &encode(&doc).unwrap());
        let dir = upath.parent().unwrap().to_path_buf();
        let state = AppState::new();
        open_file(&state, Slot::User, upath.to_str().unwrap()).unwrap();

        let out = dir.join("pack.yaml");
        let err = pack_export(&state, out.to_str().unwrap()).unwrap_err();
        assert_eq!(err.code, "no_overview");
        assert!(!out.exists(), "export must not write a file on rejection");
    }
}
