mod ops;

use ops::{AppState, ErrDto, OpenOutcome};

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![discover_profiles, open_file, close_file])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
