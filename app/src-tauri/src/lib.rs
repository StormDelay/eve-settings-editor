mod accounts;
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
fn open_file(state: tauri::State<'_, AppState>, path: String) -> Result<OpenOutcome, ErrDto> {
    ops::open_file(&state, &path)
}

#[tauri::command]
fn close_file(state: tauri::State<'_, AppState>) {
    ops::close_file(&state)
}

#[tauri::command]
fn apply_mutation(
    state: tauri::State<'_, AppState>,
    mutation: settings_model::Mutation,
) -> Result<settings_model::Node, ErrDto> {
    ops::apply_mutation(&state, &mutation)
}

#[tauri::command]
fn save_document(
    state: tauri::State<'_, AppState>,
    force: bool,
) -> Result<settings_model::SaveReport, ErrDto> {
    ops::save_document(&state, force)
}

#[tauri::command]
fn list_file_backups(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<settings_model::BackupInfo>, ErrDto> {
    ops::list_file_backups(&state)
}

#[tauri::command]
fn restore_backup(
    state: tauri::State<'_, AppState>,
    backup_path: String,
) -> Result<OpenOutcome, ErrDto> {
    ops::restore_backup(&state, &backup_path)
}

#[tauri::command]
fn window_layout(
    state: tauri::State<'_, AppState>,
) -> Result<settings_model::WindowLayout, ErrDto> {
    ops::window_layout(&state)
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            discover_profiles, open_file, close_file,
            apply_mutation, save_document, list_file_backups, restore_backup,
            window_layout, resolve_character_names, refresh_character_names,
            account_roster, set_account_alias, confirm_pairing, unpair_character,
            begin_capture, resolve_capture
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
