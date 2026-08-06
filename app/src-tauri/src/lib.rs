mod accounts;
mod groups;
mod names;
mod ops;
mod prefs;
mod presets;
mod scenes;

use ops::{AppState, ErrDto, OpenOutcome};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::Manager;

/// The one folder name this app owns, under both the data and the config dir.
/// Deliberately NOT Tauri's `app_data_dir()`/`app_config_dir()`, which append
/// the bundle identifier and so would name the folder
/// `io.github.stormdelay.eve-settings-editor`. The identifier itself stays as
/// it is — it is installer and OS-level app identity, not a display name.
pub(crate) const APP_DIR: &str = "EVE Settings Editor";

pub(crate) fn app_dir(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .data_dir()
        .map(|d| d.join(APP_DIR))
        .unwrap_or_else(|_| std::env::temp_dir())
}

/// Move a pre-0.32 identifier-named folder to its new name, once. Runs before
/// any command, so on a normal upgrade `new` cannot exist yet.
///
/// ponytail: one attempt, no merge. If the rename fails — a second instance
/// holding a file open is the realistic cause on Windows — the old folder is
/// left intact and the app starts empty beside it; renaming it by hand
/// recovers everything. Merging two populated folders is the only thing that
/// would do better, and that is a lot of code for a case that needs someone to
/// launch two copies at once.
fn migrate_dir(old: &Path, new: &Path) {
    if old.is_dir() && !new.exists() {
        let _ = std::fs::rename(old, new);
    }
}

#[tauri::command]
fn discover_profiles() -> Vec<settings_model::Profile> {
    ops::discover_profiles()
}

#[tauri::command]
fn open_file(state: tauri::State<'_, AppState>, slot: ops::Slot, path: String) -> Result<OpenOutcome, ErrDto> {
    ops::open_file(&state, slot, &path)
}

#[tauri::command]
fn close_file(state: tauri::State<'_, AppState>, slot: ops::Slot) {
    ops::close_file(&state, slot)
}

#[tauri::command]
fn apply_mutation(
    state: tauri::State<'_, AppState>,
    slot: ops::Slot,
    mutation: settings_model::Mutation,
) -> Result<settings_model::Node, ErrDto> {
    ops::apply_mutation(&state, slot, &mutation)
}

#[tauri::command]
fn apply_mutations(
    state: tauri::State<'_, AppState>,
    slot: ops::Slot,
    mutations: Vec<settings_model::Mutation>,
) -> Result<settings_model::Node, ErrDto> {
    ops::apply_mutations(&state, slot, &mutations)
}

#[tauri::command]
fn save_document(
    state: tauri::State<'_, AppState>,
    slot: ops::Slot,
    force: bool,
) -> Result<settings_model::SaveReport, ErrDto> {
    ops::save_document(&state, slot, force)
}

#[tauri::command]
fn list_file_backups(
    state: tauri::State<'_, AppState>,
    slot: ops::Slot,
) -> Result<Vec<settings_model::BackupInfo>, ErrDto> {
    ops::list_file_backups(&state, slot)
}

#[tauri::command]
fn restore_backup(
    state: tauri::State<'_, AppState>,
    slot: ops::Slot,
    backup_path: String,
) -> Result<OpenOutcome, ErrDto> {
    ops::restore_backup(&state, slot, &backup_path)
}

#[tauri::command]
fn window_layout(
    state: tauri::State<'_, AppState>,
    slot: ops::Slot,
) -> Result<settings_model::WindowLayout, ErrDto> {
    ops::window_layout(&state, slot)
}

#[tauri::command]
async fn resolve_character_names(
    app: tauri::AppHandle,
    ids: Vec<u64>,
) -> HashMap<u64, names::ResolvedName> {
    let dir = app_dir(&app);
    // Blocking ESI/file work off the async runtime; empty map on join failure.
    tauri::async_runtime::spawn_blocking(move || names::resolve_blocking(&dir, &ids, false))
        .await
        .unwrap_or_default()
}

#[tauri::command]
async fn refresh_character_names(
    app: tauri::AppHandle,
    ids: Vec<u64>,
) -> HashMap<u64, names::ResolvedName> {
    let dir = app_dir(&app);
    tauri::async_runtime::spawn_blocking(move || names::resolve_blocking(&dir, &ids, true))
        .await
        .unwrap_or_default()
}

#[tauri::command]
async fn sync_group_catalog(
    app: tauri::AppHandle,
    known_ids: Vec<i64>,
    relevant_categories: Vec<i64>,
) -> Vec<groups::GroupEntry> {
    let dir = app_dir(&app);
    tauri::async_runtime::spawn_blocking(move || groups::sync_blocking(&dir, &known_ids, &relevant_categories))
        .await
        .unwrap_or_default()
}

#[tauri::command]
fn account_roster(app: tauri::AppHandle) -> accounts::AccountRoster {
    accounts::load_roster(&settings_model::default_roots(), &app_dir(&app))
}

#[tauri::command]
fn set_account_alias(
    app: tauri::AppHandle,
    user_id: u64,
    alias: Option<String>,
) -> accounts::AccountRoster {
    accounts::set_account_alias(&settings_model::default_roots(), &app_dir(&app), user_id, alias)
}

#[tauri::command]
fn confirm_pairing(
    app: tauri::AppHandle,
    char_id: u64,
    user_id: u64,
) -> Result<accounts::AccountRoster, ErrDto> {
    accounts::confirm_pairing(&settings_model::default_roots(), &app_dir(&app), char_id, user_id)
        .map_err(|m| ErrDto { code: "cap".into(), message: m })
}

#[tauri::command]
fn unpair_character(app: tauri::AppHandle, char_id: u64) -> accounts::AccountRoster {
    accounts::unpair_character(&settings_model::default_roots(), &app_dir(&app), char_id)
}

#[tauri::command]
fn begin_capture(state: tauri::State<'_, AppState>) {
    ops::begin_capture(&state, &settings_model::default_roots());
}

#[tauri::command]
fn resolve_capture(state: tauri::State<'_, AppState>) -> accounts::CaptureResult {
    ops::resolve_capture(&state, &settings_model::default_roots())
}

#[tauri::command]
fn overview_columns(state: tauri::State<'_, AppState>) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::overview_columns(&state)
}
#[tauri::command]
fn set_overview_visible(state: tauri::State<'_, AppState>, tab_index: i64, column: String, visible: bool) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::set_overview_visible(&state, tab_index, &column, visible)
}
#[tauri::command]
fn set_overview_order(state: tauri::State<'_, AppState>, tab_index: i64, order: Vec<String>) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::set_overview_order(&state, tab_index, order)
}
#[tauri::command]
fn set_overview_width(state: tauri::State<'_, AppState>, tab_index: i64, column: String, width: i64) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::set_overview_width(&state, tab_index, &column, width)
}

#[tauri::command]
fn tab_create(state: tauri::State<'_, AppState>, window_idx: usize, name: String, from_tab: Option<i64>) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::tab_create(&state, window_idx, name, from_tab)
}
#[tauri::command]
fn tab_rename(state: tauri::State<'_, AppState>, tab_idx: i64, name: String) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::tab_rename(&state, tab_idx, name)
}
#[tauri::command]
fn tab_delete(state: tauri::State<'_, AppState>, tab_idx: i64) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::tab_delete(&state, tab_idx)
}
#[tauri::command]
fn tab_reorder(state: tauri::State<'_, AppState>, window_idx: usize, order: Vec<i64>) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::tab_reorder(&state, window_idx, order)
}
#[tauri::command]
fn tab_move(state: tauri::State<'_, AppState>, tab_idx: i64, from_window: usize, to_window: usize, pos: usize) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::tab_move(&state, tab_idx, from_window, to_window, pos)
}
#[tauri::command]
fn overview_window_add(state: tauri::State<'_, AppState>, name: String, from_tab: Option<i64>) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::overview_window_add(&state, name, from_tab)
}
#[tauri::command]
fn overview_window_remove(state: tauri::State<'_, AppState>, window_idx: usize) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::overview_window_remove(&state, window_idx)
}
#[tauri::command]
fn overview_create_window_mapping(state: tauri::State<'_, AppState>) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::overview_create_window_mapping(&state)
}
#[tauri::command]
fn preset_create(state: tauri::State<'_, AppState>, from: String, new_name: String) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::preset_create(&state, from, new_name)
}
#[tauri::command]
fn preset_rename(state: tauri::State<'_, AppState>, old_name: String, new_name: String) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::preset_rename(&state, old_name, new_name)
}
#[tauri::command]
fn preset_delete(state: tauri::State<'_, AppState>, name: String) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::preset_delete(&state, name)
}
#[tauri::command]
fn tab_set_preset(state: tauri::State<'_, AppState>, tab_idx: i64, preset: String) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::tab_set_preset(&state, tab_idx, preset)
}
#[tauri::command]
fn preset_set_groups(state: tauri::State<'_, AppState>, name: String, groups: Vec<i64>) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::preset_set_groups(&state, name, groups)
}
#[tauri::command]
fn preset_fork(state: tauri::State<'_, AppState>, tab_idx: i64, name: String, groups: Vec<i64>, filtered_states: Vec<i64>, always_shown_states: Vec<i64>) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::preset_fork(&state, tab_idx, name, groups, filtered_states, always_shown_states)
}

#[tauri::command]
fn overview_set_states(state: tauri::State<'_, AppState>, which: String, ids: Vec<i64>) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::overview_set_states(&state, which, ids)
}

#[tauri::command]
fn overview_set_state_color(state: tauri::State<'_, AppState>, id: i64, rgba: Option<[f64; 4]>) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::overview_set_state_color(&state, id, rgba)
}

#[tauri::command]
fn overview_set_bool(state: tauri::State<'_, AppState>, key: String, on: bool) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::overview_set_bool(&state, key, on)
}

#[tauri::command]
fn pack_preview(path: String) -> Result<ops::PackSummary, ErrDto> {
    ops::pack_preview(&path)
}

#[tauri::command]
fn pack_import(state: tauri::State<'_, AppState>, path: String) -> Result<ops::PackImportResult, ErrDto> {
    ops::pack_import(&state, &path)
}

#[tauri::command]
fn pack_export(state: tauri::State<'_, AppState>, path: String) -> Result<settings_model::PackReport, ErrDto> {
    ops::pack_export(&state, &path)
}

#[tauri::command]
fn preset_set_states(state: tauri::State<'_, AppState>, name: String, filtered: Vec<i64>, always_shown: Vec<i64>) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::preset_set_states(&state, name, filtered, always_shown)
}

#[tauri::command]
fn autofill_lists(state: tauri::State<'_, AppState>) -> Result<Vec<settings_model::RememberedList>, ErrDto> {
    ops::autofill_lists(&state)
}
#[tauri::command]
fn set_autofill_list(state: tauri::State<'_, AppState>, widget: String, entries: Vec<String>) -> Result<Vec<settings_model::RememberedList>, ErrDto> {
    ops::set_autofill_list(&state, &widget, entries)
}
#[tauri::command]
fn clear_all_autofill(state: tauri::State<'_, AppState>) -> Result<Vec<settings_model::RememberedList>, ErrDto> {
    ops::clear_all_autofill(&state)
}

#[tauri::command]
fn keybinds(state: tauri::State<'_, AppState>) -> Result<settings_model::Keybinds, ErrDto> {
    ops::keybinds(&state)
}
#[tauri::command]
fn set_keybind(
    state: tauri::State<'_, AppState>,
    command: String,
    keys: Option<Vec<i64>>,
) -> Result<ops::SetKeybindResult, ErrDto> {
    ops::set_keybind_cmd(&state, &command, keys)
}

#[tauri::command]
fn stack_unstack(state: tauri::State<'_, AppState>, member: String) -> Result<settings_model::WindowLayout, ErrDto> {
    ops::stack_unstack(&state, &member)
}
#[tauri::command]
fn stack_add(state: tauri::State<'_, AppState>, member: String, container: String) -> Result<settings_model::WindowLayout, ErrDto> {
    ops::stack_add(&state, &member, &container)
}
#[tauri::command]
fn stack_reorder(state: tauri::State<'_, AppState>, container: String, members: Vec<String>) -> Result<settings_model::WindowLayout, ErrDto> {
    ops::stack_reorder(&state, &container, members)
}
#[tauri::command]
fn stack_create(state: tauri::State<'_, AppState>, member1: String, member2: String) -> Result<settings_model::WindowLayout, ErrDto> {
    ops::stack_create(&state, &member1, &member2)
}
#[tauri::command]
fn stack_delete_orphans(state: tauri::State<'_, AppState>) -> Result<settings_model::WindowLayout, ErrDto> {
    ops::stack_delete_orphans(&state)
}

#[tauri::command]
fn neocom_bar(state: tauri::State<'_, AppState>) -> Result<settings_model::NeocomBar, ErrDto> {
    ops::neocom_bar(&state)
}
#[tauri::command]
fn chat_panels(state: tauri::State<'_, AppState>) -> Result<Vec<settings_model::ChatPanel>, ErrDto> {
    ops::chat_panels(&state)
}
#[tauri::command]
#[allow(non_snake_case)]
fn set_chat_splits(
    state: tauri::State<'_, AppState>,
    ids: Vec<String>,
    userlistWidth: Option<i64>,
    inputHeight: Option<i64>,
) -> Result<Vec<settings_model::ChatPanel>, ErrDto> {
    ops::set_chat_splits(&state, ids, userlistWidth, inputHeight)
}
#[tauri::command]
fn neocom_reorder(state: tauri::State<'_, AppState>, order: Vec<usize>) -> Result<settings_model::NeocomBar, ErrDto> {
    ops::neocom_reorder(&state, order)
}
#[tauri::command]
fn neocom_remove(state: tauri::State<'_, AppState>, index: usize) -> Result<settings_model::NeocomBar, ErrDto> {
    ops::neocom_remove(&state, index)
}
#[tauri::command]
fn neocom_add(state: tauri::State<'_, AppState>, id: String, btn_type: i64, icon_path: String) -> Result<settings_model::NeocomBar, ErrDto> {
    ops::neocom_add(&state, &id, btn_type, &icon_path)
}
#[tauri::command]
fn neocom_reset(state: tauri::State<'_, AppState>) -> Result<settings_model::NeocomBar, ErrDto> {
    ops::neocom_reset(&state)
}

#[tauri::command]
fn probe_formations(state: tauri::State<'_, AppState>) -> Result<settings_model::Formations, ErrDto> {
    ops::probe_formations(&state)
}

#[tauri::command]
fn set_probe_formation(
    state: tauri::State<'_, AppState>,
    id: Option<i64>,
    name: String,
    probes: Vec<[f64; 3]>,
    ranges: Vec<f64>,
) -> Result<settings_model::Formations, ErrDto> {
    ops::set_probe_formation(&state, id, &name, probes, ranges)
}

#[tauri::command]
fn remove_probe_formation(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<settings_model::Formations, ErrDto> {
    ops::remove_probe_formation(&state, id)
}

#[tauri::command]
fn probe_yaml(formations: Vec<settings_model::FormationSpec>) -> String {
    ops::probe_yaml(&formations)
}

#[tauri::command]
fn probe_parse_yaml(text: String) -> Result<Vec<settings_model::FormationSpec>, ErrDto> {
    ops::probe_parse_yaml(&text)
}

#[tauri::command]
fn probe_export(
    path: String,
    formations: Vec<settings_model::FormationSpec>,
) -> Result<(), ErrDto> {
    ops::probe_export(&path, &formations)
}

#[tauri::command]
fn probe_import(path: String) -> Result<Vec<settings_model::FormationSpec>, ErrDto> {
    ops::probe_import(&path)
}

#[tauri::command]
fn add_probe_formations(
    state: tauri::State<'_, AppState>,
    formations: Vec<settings_model::FormationSpec>,
) -> Result<settings_model::Formations, ErrDto> {
    ops::add_probe_formations(&state, formations)
}

/// Read-only. There is no write path: a scene is a file the user edits, and an
/// in-app editor is a later slice.
#[tauri::command]
fn scene_list(app: tauri::AppHandle) -> scenes::SceneList {
    scenes::list(&app_dir(&app))
}

#[tauri::command]
fn hud_layout(state: tauri::State<'_, AppState>) -> Result<settings_model::Hud, ErrDto> {
    ops::hud_layout(&state)
}
#[tauri::command]
fn set_hud_value(
    state: tauri::State<'_, AppState>,
    name: String,
    text: String,
) -> Result<settings_model::Hud, ErrDto> {
    ops::set_hud_field(&state, &name, &text)
}

#[tauri::command]
fn setup_preview(
    app: tauri::AppHandle,
    source: ops::BatchSource,
    target_char_paths: Vec<String>,
    aspects: Vec<ops::Aspect>,
    allow_other_folders: bool,
) -> ops::SetupPlan {
    ops::setup_preview(
        &settings_model::default_roots(),
        &app_dir(&app),
        &source,
        &target_char_paths,
        &aspects,
        allow_other_folders,
    )
}

#[tauri::command]
fn setup_apply(
    app: tauri::AppHandle,
    source: ops::BatchSource,
    target_char_paths: Vec<String>,
    aspects: Vec<ops::Aspect>,
    allow_other_folders: bool,
) -> Result<Vec<ops::TargetResult>, ErrDto> {
    ops::setup_apply(
        &settings_model::default_roots(),
        &app_dir(&app),
        &source,
        &target_char_paths,
        &aspects,
        allow_other_folders,
    )
}

// The overview view already owns `preset_create`/`preset_rename`/`preset_delete`
// for EVE's own overview filter presets — these are the settings-preset
// library (Task 8+), hence the longer `settings_preset_*` names.
#[tauri::command]
fn settings_preset_list(app: tauri::AppHandle) -> Vec<presets::PresetInfo> {
    presets::list(&app_dir(&app))
}

#[tauri::command]
fn settings_preset_create(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    name: String,
    aspects: Vec<ops::Aspect>,
    overwrite: bool,
) -> Result<Vec<presets::PresetInfo>, ErrDto> {
    ops::preset_save(&state, &app_dir(&app), &name, &aspects, overwrite)?;
    Ok(presets::list(&app_dir(&app)))
}

#[tauri::command]
fn settings_preset_rename(
    app: tauri::AppHandle,
    old_name: String,
    new_name: String,
) -> Result<Vec<presets::PresetInfo>, ErrDto> {
    presets::rename(&app_dir(&app), &old_name, &new_name)
        .map_err(|m| ErrDto { code: "preset".into(), message: m })?;
    Ok(presets::list(&app_dir(&app)))
}

#[tauri::command]
fn settings_preset_delete(
    app: tauri::AppHandle,
    name: String,
) -> Result<Vec<presets::PresetInfo>, ErrDto> {
    presets::delete(&app_dir(&app), &name)
        .map_err(|m| ErrDto { code: "preset".into(), message: m })?;
    Ok(presets::list(&app_dir(&app)))
}

#[tauri::command]
fn settings_preset_export(app: tauri::AppHandle, name: String, path: String) -> Result<(), ErrDto> {
    presets::export_to(&app_dir(&app), &name, std::path::Path::new(&path))
        .map_err(|m| ErrDto { code: "preset".into(), message: m })
}

/// An import, plus the refreshed library. The name is carried back because
/// `import_from` may have suffixed it to avoid a collision — without it the
/// caller cannot say which row is the one that just arrived, nor that it was
/// renamed.
#[derive(serde::Serialize)]
struct ImportResult {
    name: String,
    presets: Vec<presets::PresetInfo>,
}

#[tauri::command]
fn settings_preset_import(app: tauri::AppHandle, path: String) -> Result<ImportResult, ErrDto> {
    let name = presets::import_from(&app_dir(&app), std::path::Path::new(&path))
        .map_err(|m| ErrDto { code: "preset".into(), message: m })?;
    Ok(ImportResult { name, presets: presets::list(&app_dir(&app)) })
}

#[tauri::command]
fn preferences(app: tauri::AppHandle) -> Result<prefs::Preferences, ErrDto> {
    let path = prefs::path(&app).map_err(|m| ErrDto { code: "no_config_dir".into(), message: m })?;
    Ok(prefs::load_from(&path))
}

#[tauri::command]
fn set_preferences(app: tauri::AppHandle, prefs: prefs::Preferences) -> Result<(), ErrDto> {
    let path = crate::prefs::path(&app).map_err(|m| ErrDto { code: "no_config_dir".into(), message: m })?;
    crate::prefs::save_to(&path, &prefs)
        .map_err(|e| ErrDto { code: "write_failed".into(), message: e.to_string() })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let p = app.path();
            // Three trees, one name. On Windows and macOS data and config are
            // the same folder, so the second pass finds nothing left to move;
            // on Linux they are separate and `preferences.json` is in the
            // config one. The local data dir is separate everywhere and holds
            // only WebView2's `EBWebView` cache — machine-specific and
            // regenerable, which is exactly what belongs there rather than in
            // the roaming folder beside the user's presets.
            for (old, base) in [
                (p.app_data_dir(), p.data_dir()),
                (p.app_config_dir(), p.config_dir()),
                (p.app_local_data_dir(), p.local_data_dir()),
            ] {
                if let (Ok(old), Ok(base)) = (old, base) {
                    migrate_dir(&old, &base.join(APP_DIR));
                }
            }

            // The window is `"create": false` in tauri.conf.json so it can be
            // built here instead: `data_directory` takes an absolute path only
            // from the builder — a value in the config file is forced relative
            // to `<local data dir>/<window label>`, which is neither the old
            // folder nor the new one.
            if let Some(window) = app.config().app.windows.first() {
                let mut builder = tauri::WebviewWindowBuilder::from_config(app.handle(), window)?;
                // An unresolvable local data dir is not worth losing the window
                // over — Tauri's identifier-named default is a fine fallback.
                if let Ok(dir) = p.local_data_dir() {
                    builder = builder.data_directory(dir.join(APP_DIR));
                }
                builder.build()?;
            }
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            discover_profiles, open_file, close_file,
            apply_mutation, apply_mutations, save_document, list_file_backups, restore_backup,
            window_layout, resolve_character_names, refresh_character_names, sync_group_catalog,
            account_roster, set_account_alias, confirm_pairing, unpair_character,
            begin_capture, resolve_capture,
            overview_columns, set_overview_visible, set_overview_order, set_overview_width,
            tab_create, tab_rename, tab_delete, tab_reorder, tab_move,
            overview_window_add, overview_window_remove, overview_create_window_mapping,
            preset_create, preset_rename, preset_delete, tab_set_preset, preset_set_groups, preset_fork,
            overview_set_states, overview_set_state_color, overview_set_bool, preset_set_states,
            pack_preview, pack_import, pack_export,
            autofill_lists, set_autofill_list, clear_all_autofill,
            keybinds, set_keybind,
            setup_preview, setup_apply,
            settings_preset_list, settings_preset_create, settings_preset_rename,
            settings_preset_delete, settings_preset_export, settings_preset_import,
            stack_unstack, stack_add, stack_reorder, stack_create, stack_delete_orphans,
            neocom_bar, neocom_reorder, neocom_remove, neocom_add, neocom_reset, chat_panels, set_chat_splits,
            probe_formations, set_probe_formation, remove_probe_formation,
            probe_yaml, probe_parse_yaml, probe_export, probe_import, add_probe_formations,
            scene_list,
            hud_layout, set_hud_value,
            preferences, set_preferences
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("eve-appdir-test-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn an_identifier_named_folder_moves_with_its_contents() {
        let base = temp_dir("moves");
        let old = base.join("io.github.stormdelay.eve-settings-editor");
        std::fs::create_dir_all(old.join("presets")).unwrap();
        std::fs::write(old.join("accounts.json"), b"{}").unwrap();

        let new = base.join(APP_DIR);
        migrate_dir(&old, &new);

        assert!(!old.exists(), "the old folder is gone, not copied");
        assert_eq!(std::fs::read(new.join("accounts.json")).unwrap(), b"{}");
        assert!(new.join("presets").is_dir(), "and subdirectories came with it");
    }

    /// A populated destination survives. Note this passes on Windows even
    /// without the `!new.exists()` guard, because renaming onto a non-empty
    /// directory fails there anyway — the guard is what makes it true on Unix,
    /// where an EMPTY destination would otherwise be replaced.
    #[test]
    fn an_existing_new_folder_is_never_clobbered() {
        let base = temp_dir("clobber");
        let old = base.join("io.github.stormdelay.eve-settings-editor");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("accounts.json"), b"old").unwrap();

        let new = base.join(APP_DIR);
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("accounts.json"), b"new").unwrap();

        migrate_dir(&old, &new);

        assert_eq!(std::fs::read(new.join("accounts.json")).unwrap(), b"new");
        assert!(old.is_dir(), "and the old one is left where a human can find it");
    }
}
