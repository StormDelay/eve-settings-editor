mod accounts;
mod groups;
mod names;
mod ops;

use ops::{AppState, ErrDto, OpenOutcome};
use std::collections::HashMap;
use tauri::Manager;

fn app_dir(app: &tauri::AppHandle) -> std::path::PathBuf {
    app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir())
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
    let dir = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir());
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
    let dir = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir());
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
    let dir = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir());
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
fn setup_preview(
    app: tauri::AppHandle,
    source_char_path: String,
    target_char_paths: Vec<String>,
    aspects: Vec<ops::Aspect>,
    allow_other_folders: bool,
) -> ops::SetupPlan {
    ops::setup_preview(
        &settings_model::default_roots(),
        &app_dir(&app),
        &source_char_path,
        &target_char_paths,
        &aspects,
        allow_other_folders,
    )
}

#[tauri::command]
fn setup_apply(
    app: tauri::AppHandle,
    source_char_path: String,
    target_char_paths: Vec<String>,
    aspects: Vec<ops::Aspect>,
    allow_other_folders: bool,
) -> Result<Vec<ops::TargetResult>, ErrDto> {
    ops::setup_apply(
        &settings_model::default_roots(),
        &app_dir(&app),
        &source_char_path,
        &target_char_paths,
        &aspects,
        allow_other_folders,
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            discover_profiles, open_file, close_file,
            apply_mutation, apply_mutations, save_document, list_file_backups, restore_backup,
            window_layout, resolve_character_names, refresh_character_names, sync_group_catalog,
            account_roster, set_account_alias, confirm_pairing, unpair_character,
            begin_capture, resolve_capture,
            overview_columns, set_overview_visible, set_overview_order, set_overview_width,
            tab_create, tab_rename, tab_delete, tab_reorder, tab_move,
            overview_window_add, overview_window_remove,
            preset_create, preset_rename, preset_delete, tab_set_preset, preset_set_groups,
            autofill_lists, set_autofill_list, clear_all_autofill,
            setup_preview, setup_apply,
            stack_unstack, stack_add, stack_reorder, stack_create
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
