//! Batch apply and the settings-preset library's write half: plan which files a
//! chosen set of aspects would touch, then carry them out.
//!
//! Split out of `ops.rs`, which held this and the open-document command surface
//! in one 3,100-line file. The two share `AppState`, `ErrDto` and `Slot` and
//! nothing else: everything here is a pure planner over paths and categories,
//! with the disk work at the edges.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use blue_marshal::Value;
use serde::Serialize;
use settings_model::{
    apply_categories_to, discover, extract_categories, full_copy_to,
    window_layout as project_window_layout, Category, FileKind,
};

use crate::accounts;
use crate::ops::{AppState, ErrDto};
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
    if !w.char_full_copy
        && !plan.char_writes.is_empty()
        && source_side_empty(&sides.char_path, &w.char_categories)
    {
        plan.char_writes.clear();
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

/// Every discovered char/user file, by path. Unlike `scoped_files` this keeps
/// files whose name carries no parseable id: a file copy addresses files by
/// path, so a hand-made backup like `core_char_123 - old.dat` is a legitimate
/// source to restore from and a legitimate target to overwrite.
fn all_settings_files(roots: &[PathBuf]) -> HashMap<PathBuf, FileKind> {
    let mut out = HashMap::new();
    for p in discover(roots) {
        for f in &p.files {
            if f.kind != FileKind::Other {
                out.insert(f.path.clone(), f.kind);
            }
        }
    }
    out
}

/// Copy one settings file byte-for-byte onto others of the same kind — the
/// file-level copy, with no character↔account pairing involved. Every target is
/// backed up first by `full_copy_to`.
///
/// Paths come from the frontend, so both ends are checked against discovery:
/// only a real char/user file can be read, and only a real one of the SAME kind
/// can be written. A char file spliced over a user file would be a valid
/// document full of the wrong keys, which nothing downstream would flag.
pub fn copy_files(
    roots: &[PathBuf],
    source: &str,
    targets: &[String],
) -> Result<Vec<TargetResult>, ErrDto> {
    let files = all_settings_files(roots);
    let src = PathBuf::from(source);
    let Some(&kind) = files.get(&src) else {
        return Err(ErrDto::new("source", "The source is not a character or account settings file."));
    };
    let bytes = fs::read(&src).map_err(|e| ErrDto::new("io", e.to_string()))?;
    Ok(targets
        .iter()
        .map(|t| {
            let path = PathBuf::from(t);
            if path == src {
                return err_result(t, "A file cannot be copied onto itself.".into());
            }
            if files.get(&path) != Some(&kind) {
                return err_result(t, "Not a settings file of the same kind as the source.".into());
            }
            full_copy_to(&bytes, &path)
                .map(|bk| ok_result(t, bk.to_string_lossy().into_owned()))
                .unwrap_or_else(|e| err_result(t, e))
        })
        .collect())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{b, temp_file};
    use blue_marshal::{encode, Value};
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

    #[test]
    fn copy_files_clones_bytes_and_refuses_anything_but_a_same_kind_settings_file() {
        let base = std::env::temp_dir().join(format!("app-copyfiles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let prof =
            base.join("root").join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        std::fs::create_dir_all(&prof).unwrap();
        let doc = |s: &str| encode(&Value::Dict(vec![(b("who"), b(s))])).unwrap();
        std::fs::write(prof.join("core_user_500.dat"), doc("SRC")).unwrap();
        std::fs::write(prof.join("core_user_600.dat"), doc("OLD")).unwrap();
        // No parseable id, so `scoped_files` drops it — a file copy must not.
        std::fs::write(prof.join("core_user_700 - old.dat"), doc("BACKUP")).unwrap();
        std::fs::write(prof.join("core_char_100.dat"), doc("CHAR")).unwrap();
        let stray = base.join("elsewhere.dat");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(&stray, doc("STRAY")).unwrap();

        let roots = vec![base.join("root")];
        let p = |n: &str| prof.join(n).to_string_lossy().into_owned();
        let src = p("core_user_500.dat");
        let results = copy_files(
            &roots,
            &src,
            &[
                p("core_user_600.dat"),
                p("core_user_700 - old.dat"),
                p("core_char_100.dat"),
                stray.to_string_lossy().into_owned(),
                src.clone(),
            ],
        )
        .unwrap();

        assert!(results[0].ok && results[1].ok, "both account files were written");
        assert_eq!(std::fs::read(prof.join("core_user_600.dat")).unwrap(), doc("SRC"));
        assert_eq!(std::fs::read(prof.join("core_user_700 - old.dat")).unwrap(), doc("SRC"));
        assert!(results[0].backup_path.is_some(), "the target was backed up before the write");
        assert!(!results[2].ok, "a char file is not a target for a user-file source");
        assert!(!results[3].ok, "a path outside discovery is refused");
        assert!(!results[4].ok, "a file is not copied onto itself");
        assert_eq!(std::fs::read(prof.join("core_char_100.dat")).unwrap(), doc("CHAR"));

        // An undiscovered source is refused outright, not per-target.
        let err = copy_files(&roots, &stray.to_string_lossy(), &[p("core_user_600.dat")]).unwrap_err();
        assert_eq!(err.code, "source");
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

}
