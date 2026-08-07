//! The open-document command surface as plain functions over `AppState`, so it
//! unit-tests without a Tauri runtime. The `#[tauri::command]` wrappers in
//! lib.rs are one-liners delegating here.
//!
//! Batch apply and the preset write half live in `setup.rs`, which shares this
//! module's `AppState`, `ErrDto` and `Slot` and nothing else.

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
    Document, Fidelity, LoadError, Mutation, Node, OverviewColumns, Profile, SaveReport,
    WindowLayout,
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
    pub(crate) fn new(code: &str, message: impl Into<String>) -> Self {
        ErrDto { code: code.into(), message: message.into() }
    }
}

/// Flatten a model error that serializes as `{"code": …, …}` into an `ErrDto`,
/// keeping its `code` tag so the UI can branch on it (e.g. `parse_key` anchors
/// the message to the key field). `fallback` is used when the variant carries no
/// code. The message is the error's `Display` form.
///
/// One function for what were ten hand-written copies of the same three lines —
/// `mutate`, `chat`, `hud`, `tab`, `stack`, `neocom`, `probes` and the two
/// `apply_mutation*` paths. `save_document` keeps its own, because it reads
/// `detail` rather than `Display` for the message.
fn coded_err<E: Serialize + std::fmt::Display>(fallback: &str, e: E) -> ErrDto {
    let v = serde_json::to_value(&e).unwrap_or_default();
    ErrDto::new(v.get("code").and_then(|c| c.as_str()).unwrap_or(fallback), e.to_string())
}

/// The "nothing open in this slot" error, named for the file the user would
/// recognise. Every write path derives its slot, so this replaces the per-call
/// message each one used to spell out.
fn no_document(slot: Slot) -> ErrDto {
    ErrDto::new(
        "no_document",
        match slot {
            Slot::Char => "no character file open",
            Slot::User => "no account file open",
        },
    )
}

/// Lock `slot`, require the document open and writable, run `edit`, and reshare
/// the tree when `edit` reports it changed the document's shape.
///
/// The read-only guard lives here and in `save_document`'s sibling checks only:
/// every mutating command in this file routes through this function or
/// `edit_slot`, so a new one cannot ship without the guard. That is what the
/// fifteen hand-copied preambles this replaces could not promise.
fn edit_reshared<T, E>(
    state: &AppState,
    slot: Slot,
    edit: impl FnOnce(&mut Value) -> Result<(T, bool), E>,
    err: impl FnOnce(E) -> ErrDto,
) -> Result<T, ErrDto> {
    let mut guard = state.doc(slot).lock().unwrap();
    let doc = guard.as_mut().ok_or_else(|| no_document(slot))?;
    if let Fidelity::ReadOnly { reason } = &doc.fidelity {
        return Err(ErrDto::new("read_only", reason.clone()));
    }
    let (out, changed_shape) = edit(&mut doc.value).map_err(err)?;
    if changed_shape {
        doc.value = blue_marshal::reshare(&doc.value);
    }
    Ok(out)
}

/// `edit_reshared` for a structural edit — one that replaces list or dict
/// structure and so always leaves an inline-first tree that must be reshared
/// before it can encode. Nearly every caller wants this one.
fn edit_slot<T, E>(
    state: &AppState,
    slot: Slot,
    edit: impl FnOnce(&mut Value) -> Result<T, E>,
    err: impl FnOnce(E) -> ErrDto,
) -> Result<T, ErrDto> {
    edit_reshared(state, slot, |v| edit(v).map(|t| (t, true)), err)
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

/// A tree edit through the generic mutation path. Never reshares: `apply` edits
/// one node in place and leaves the document's sharing exactly as it found it.
pub fn apply_mutation(state: &AppState, slot: Slot, mutation: &Mutation) -> Result<Node, ErrDto> {
    edit_reshared(
        state,
        slot,
        |v| apply(v, mutation).map(|_| (project(v), false)),
        |e| coded_err("mutate", e),
    )
}

/// Batched sibling of `apply_mutation`: applies every mutation to the same
/// locked doc, then projects the tree once instead of once per mutation.
/// Non-atomic on a mid-batch failure, matching the caller's prior per-mutation
/// loop — geometry set_scalars on valid paths don't fail.
pub fn apply_mutations(state: &AppState, slot: Slot, mutations: &[Mutation]) -> Result<Node, ErrDto> {
    edit_reshared(
        state,
        slot,
        |v| {
            for m in mutations {
                apply(v, m)?;
            }
            Ok::<_, settings_model::MutateError>((project(v), false))
        },
        |e| coded_err("mutate", e),
    )
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
/// Re-projects after the write's guard has dropped, as every other command here
/// does — calling `chat_panels` while still holding the lock would deadlock,
/// since `std::sync::Mutex` is not reentrant.
pub fn set_chat_splits(
    state: &AppState,
    ids: Vec<String>,
    userlist: Option<i64>,
    input: Option<i64>,
) -> Result<Vec<ChatPanel>, ErrDto> {
    // Only a mint de-shares the document; a scalar overwrite sets one value in
    // place, where a whole-tree reshare would buy nothing — so the model's
    // "minted" answer IS the reshare decision `edit_reshared` takes.
    edit_reshared(
        state,
        Slot::User,
        |v| settings_model::set_chat_splits(v, &ids, userlist, input).map(|minted| ((), minted)),
        chat_err,
    )?;
    chat_panels(state)
}

fn chat_err(e: settings_model::ChatError) -> ErrDto {
    coded_err("chat", e)
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
    // Only a mint de-shares the document, and only a de-shared document needs
    // re-sharing before it can encode. Every other write sets one scalar in
    // place, where a whole-tree reshare bought nothing — so the model's
    // "minted" answer IS the reshare decision.
    edit_reshared(
        state,
        slot,
        |v| set_hud_value(v, name, text).map(|minted| ((), minted)),
        |e| coded_err("hud", e),
    )?;
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
/// `OverviewError` carries no `code` tag, so it is rendered with `Debug` under a
/// fixed one rather than through `coded_err`.
fn edit_user_overview<F>(state: &AppState, edit: F) -> Result<OverviewColumns, ErrDto>
where
    F: FnOnce(&mut Value) -> Result<(), settings_model::OverviewError>,
{
    edit_slot(state, Slot::User, edit, |e| ErrDto::new("overview", format!("{e:?}")))?;
    overview_columns(state)
}

pub fn set_overview_visible(state: &AppState, tab_index: i64, column: &str, visible: bool) -> Result<OverviewColumns, ErrDto> {
    edit_user_overview(state, |v| set_column_visible(v, tab_index, column, visible))
}

pub fn set_overview_order(state: &AppState, tab_index: i64, order: Vec<String>) -> Result<OverviewColumns, ErrDto> {
    edit_user_overview(state, |v| set_column_order(v, tab_index, &order))
}

/// The width lives in the CHARACTER file, so this is the one overview write
/// that takes the char slot. It is a scalar overwrite in place, not a structural
/// edit, so it does not reshare — hence `edit_reshared` with a constant `false`
/// rather than `edit_slot`.
pub fn set_overview_width(state: &AppState, tab_index: i64, column: &str, width: i64) -> Result<OverviewColumns, ErrDto> {
    edit_reshared(
        state,
        Slot::Char,
        |v| set_column_width(v, tab_index, column, width).map(|_| ((), false)),
        |e| ErrDto::new("overview", format!("{e:?}")),
    )?;
    overview_columns(state)
}

/// Edit the user slot's overview tab structure, reshare, then re-project.
fn edit_user_tabs<F>(state: &AppState, edit: F) -> Result<OverviewColumns, ErrDto>
where
    F: FnOnce(&mut Value) -> Result<(), OverviewTabError>,
{
    edit_slot(state, Slot::User, edit, |e| coded_err("tab", e))?;
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
    let new_window_idx = edit_slot(
        state,
        Slot::User,
        |v| add_overview_window(v, &name, from_tab),
        |e| coded_err("tab", e),
    )?;
    try_edit_char(state, |v| add_overview_window_geometry(v, new_window_idx));
    overview_columns(state)
}

/// Remove the last overview window: drop the grouping in the user file and the
/// paired `overview_N` geometry in the char file (best-effort, as above).
pub fn overview_window_remove(state: &AppState, window_idx: usize) -> Result<OverviewColumns, ErrDto> {
    edit_slot(
        state,
        Slot::User,
        |v| remove_overview_window(v, window_idx),
        |e| coded_err("tab", e),
    )?;
    try_edit_char(state, |v| remove_overview_window_geometry(v, window_idx));
    overview_columns(state)
}

/// A char-slot edit that is allowed to do nothing: skipped when no character is
/// open or it is read-only, and it cannot fail. Both overview-window commands
/// pair a required user write with one of these, because EVE self-heals the
/// window geometry at its default on that character's next login — a character
/// that is not open is not a reason to fail the command.
fn try_edit_char(state: &AppState, edit: impl FnOnce(&mut Value)) {
    let mut guard = state.char.lock().unwrap();
    if let Some(doc) = guard.as_mut() {
        if !matches!(doc.fidelity, Fidelity::ReadOnly { .. }) {
            edit(&mut doc.value);
            doc.value = blue_marshal::reshare(&doc.value);
        }
    }
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
    coded_err("pack", e)
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
    let report = edit_slot(
        state,
        Slot::User,
        |v| settings_model::apply_pack(v, &pack),
        pack_err,
    )?;
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

/// Edit the user slot's editHistory, then re-project. `AutofillError` carries no
/// `code` tag, so it is rendered with `Debug` under a fixed one.
fn edit_user_autofill<F>(state: &AppState, edit: F) -> Result<Vec<RememberedList>, ErrDto>
where
    F: FnOnce(&mut Value) -> Result<(), AutofillError>,
{
    edit_slot(state, Slot::User, edit, |e| ErrDto::new("autofill", format!("{e:?}")))?;
    autofill_lists(state)
}

pub fn set_autofill_list(state: &AppState, widget: &str, entries: Vec<String>) -> Result<Vec<RememberedList>, ErrDto> {
    edit_user_autofill(state, |v| set_list_entries(v, widget, &entries))
}

pub fn clear_all_autofill(state: &AppState) -> Result<Vec<RememberedList>, ErrDto> {
    edit_user_autofill(state, clear_all_history)
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
    // The edit hands back the commands whose binding it cleared; `edit_slot`
    // carries that value out rather than the caller taking the lock again.
    let stolen = edit_slot(
        state,
        Slot::User,
        |v| settings_model::set_keybind(v, command, keys),
        |e| ErrDto::new("keybind", format!("{e:?}")),
    )?;
    Ok(SetKeybindResult { keybinds: keybinds(state)?, stolen })
}

/// Edit the CHAR slot's window stacks, reshare, then re-project the layout.
fn edit_char_stacks<F>(state: &AppState, edit: F) -> Result<WindowLayout, ErrDto>
where
    F: FnOnce(&mut Value) -> Result<(), StackError>,
{
    edit_slot(state, Slot::Char, edit, |e| coded_err("stack", e))?;
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
    coded_err("neocom", e)
}

/// Edit the CHAR slot's neocom bar, reshare, then re-project it.
fn edit_char_neocom<F>(state: &AppState, edit: F) -> Result<NeocomBar, ErrDto>
where
    F: FnOnce(&mut Value) -> Result<(), NeocomError>,
{
    edit_slot(state, Slot::Char, edit, neocom_err)?;
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
    coded_err("probes", e)
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
    F: FnOnce(&mut Value) -> Result<(), settings_model::ProbeError>,
{
    edit_slot(state, Slot::User, edit, probe_err)?;
    probe_formations(state)
}

/// `id: None` creates at the next free id. Resolving it here rather than in the
/// frontend keeps id allocation in one place, next to the rule that produced it.
pub fn set_probe_formation(
    state: &AppState,
    id: Option<i64>,
    name: &str,
    probes: Vec<[f64; 3]>,
    ranges: Vec<f64>,
) -> Result<settings_model::Formations, ErrDto> {
    // ponytail: this create path allocates from the PROJECTION, which drops any
    // entry `read_formation` rejects — so a create can land on, and overwrite,
    // an entry the user was never shown. `next_free_id` (probes.rs) is the fix,
    // but it needs the document, and this resolves the id OUTSIDE
    // `edit_user_probes`. Restructure to allocate inside the edit closure, as
    // `add_probe_formations` does, if this becomes a real report.
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
    edit_user_probes(state, |v| settings_model::set_formation(v, id, name, &probes, &ranges))
}

pub fn remove_probe_formation(
    state: &AppState,
    id: i64,
) -> Result<settings_model::Formations, ErrDto> {
    edit_user_probes(state, |v| settings_model::remove_formation(v, id))
}

/// Emit the shared YAML for a set of formations.
///
/// The FRONTEND supplies the data rather than naming ids for a lookup here:
/// Copy and Export send what the user currently sees, uncommitted drafts
/// included (sharing spec §5.1), and only the view holds that.
pub fn probe_yaml(formations: &[settings_model::FormationSpec]) -> String {
    settings_model::emit_formations(formations)
}

pub fn probe_parse_yaml(text: &str) -> Result<Vec<settings_model::FormationSpec>, ErrDto> {
    settings_model::parse_formations(text).map_err(probe_err)
}

pub fn probe_export(
    path: &str,
    formations: &[settings_model::FormationSpec],
) -> Result<(), ErrDto> {
    std::fs::write(path, settings_model::emit_formations(formations))
        .map_err(|e| ErrDto::new("io", format!("{path}: {e}")))
}

pub fn probe_import(path: &str) -> Result<Vec<settings_model::FormationSpec>, ErrDto> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| ErrDto::new("io", format!("{path}: {e}")))?;
    probe_parse_yaml(&text)
}

/// Add formations at fresh ids, suffixing any name the account already holds.
///
/// One command rather than N `set_probe_formation` calls: each of those
/// reshares the whole document (sharing spec §4.1). It is also the single place
/// the collision rule lives, so Paste and Import cannot disagree about it.
pub fn add_probe_formations(
    state: &AppState,
    formations: Vec<settings_model::FormationSpec>,
) -> Result<settings_model::Formations, ErrDto> {
    edit_user_probes(state, |v| {
        // The WHOLE batch, before the first write. A bad entry halfway down
        // would otherwise leave half an import applied (spec §4.2).
        for f in &formations {
            settings_model::check_formation(&f.name, &f.probes, &f.ranges)?;
        }
        for f in formations {
            // Re-projected per formation because each write changes both the
            // free ids and the taken names.
            //
            // ponytail: O(n²) over a batch of at most a handful of formations.
            // Thread a running Formations through the loop if a source of
            // hundreds ever appears.
            let now = settings_model::project_formations(v).unwrap_or(settings_model::Formations {
                // No key yet — the first-ever formation on this account. Any
                // real problem with the document resurfaces from set_formation.
                formations: Vec::new(),
                selected: None,
            });
            // From the STORED dict, not the projection: an entry the projection
            // could not read keeps its key, and `set_formation` REPLACES an id
            // it finds. An import is additive (spec §4.3), so it must not land
            // on one. The collision names still come from the projection —
            // that is the only place a name can be read from safely, and a
            // clash with an unreadable entry is cosmetic rather than
            // destructive.
            let id = settings_model::next_free_id(v);
            let held: Vec<String> = now.formations.into_iter().map(|x| x.name).collect();
            let name = settings_model::unique_name(&held, &f.name);
            settings_model::set_formation(v, id, &name, &f.probes, &f.ranges)?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{b, temp_file};
    use blue_marshal::{encode, Value};

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
    use settings_model::Step;

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
        begin_capture(&state, std::slice::from_ref(&root));
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
        begin_capture(&state, std::slice::from_ref(&root));

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
    fn set_probe_formation_with_no_key_mints_it_at_id_zero() {
        // 61 of 175 corpus account files have no formations key at all — this
        // is the only create path, so the first-ever formation on one of them
        // must not fail before reaching `set_formation`, which mints the key.
        let bytes = encode(&Value::Dict(vec![(b("ui"), Value::Dict(vec![]))])).unwrap();
        let path = temp_file("probes-no-key", &bytes);
        let state = AppState::new();
        open_file(&state, Slot::User, path.to_str().unwrap()).unwrap();

        let f = set_probe_formation(&state, None, "first", vec![[1.0, 0.0, 0.0]], vec![1000.0]).unwrap();
        assert_eq!(f.formations.len(), 1);
        assert_eq!(f.formations[0].id, 0, "0 is the first free id when none exist yet");
        assert_eq!(f.formations[0].name, "first");
        assert_eq!(f.formations[0].ranges, vec![1000.0]);
    }

    fn spec(name: &str, x: f64) -> settings_model::FormationSpec {
        settings_model::FormationSpec {
            name: name.into(),
            probes: vec![[x, 0.0, 0.0]],
            ranges: vec![74_798_935_350.0],
        }
    }

    /// An account file holding one formation named "close" at id 0.
    fn state_with_close() -> (AppState, PathBuf) {
        let bytes = encode(&Value::Dict(vec![(b("ui"), Value::Dict(vec![]))])).unwrap();
        let path = temp_file("probes-add", &bytes);
        let state = AppState::new();
        open_file(&state, Slot::User, path.to_str().unwrap()).unwrap();
        set_probe_formation(&state, None, "close", vec![[1.0, 0.0, 0.0]], vec![74_798_935_350.0])
            .unwrap();
        (state, path)
    }

    #[test]
    fn add_probe_formations_allocates_a_distinct_id_for_each() {
        // next_id fills the lowest free gap, so allocating them all up front
        // from one projection would hand every member of the batch the same id.
        let (state, _p) = state_with_close();
        let f = add_probe_formations(&state, vec![spec("a", 2.0), spec("b", 3.0)]).unwrap();
        let ids: Vec<i64> = f.formations.iter().map(|x| x.id).collect();
        assert_eq!(ids, vec![0, 1, 2]);
        let names: Vec<&str> = f.formations.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, vec!["close", "a", "b"]);
    }

    #[test]
    fn add_probe_formations_suffixes_a_colliding_name() {
        let (state, _p) = state_with_close();
        let f = add_probe_formations(&state, vec![spec("close", 2.0), spec("close", 3.0)]).unwrap();
        let names: Vec<&str> = f.formations.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, vec!["close", "close copy", "close copy 2"]);
        assert_eq!(f.formations[0].probes, vec![[1.0, 0.0, 0.0]], "the original must not move");
    }

    #[test]
    fn an_invalid_member_writes_none_of_the_batch() {
        // Half an import is worse than none: the user would have to work out
        // which half (sharing spec §4.2).
        let (state, _p) = state_with_close();
        let empty = settings_model::FormationSpec {
            name: "bad".into(),
            probes: vec![],
            ranges: vec![],
        };
        let err = add_probe_formations(&state, vec![spec("good", 2.0), empty]).unwrap_err();
        assert_eq!(err.code, "bad_probe_count");
        let after = probe_formations(&state).unwrap();
        assert_eq!(after.formations.len(), 1, "nothing from the batch may survive");
        assert_eq!(after.formations[0].name, "close");
    }

    #[test]
    fn probe_export_then_import_round_trips_a_file() {
        let dir = std::env::temp_dir().join(format!("app-ops-{}-probe-yaml", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("formations.yaml");
        let specs = vec![spec("close", -1199120384.7)];
        probe_export(path.to_str().unwrap(), &specs).unwrap();
        assert_eq!(probe_import(path.to_str().unwrap()).unwrap(), specs);
    }

    #[test]
    fn probe_import_of_a_missing_file_is_an_io_error() {
        let missing = std::env::temp_dir().join("app-ops-no-such-formations.yaml");
        assert_eq!(probe_import(missing.to_str().unwrap()).unwrap_err().code, "io");
    }

    #[test]
    fn probe_parse_yaml_reports_the_wrong_file() {
        assert_eq!(probe_parse_yaml("presets:\n  - a\n").unwrap_err().code, "not_formations");
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
