use tauri::AppHandle;

#[tauri::command]
pub fn start_auto_selection(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn stop_auto_selection() -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn request_auto_selection_emit() {}

#[tauri::command]
pub fn is_auto_selection_active() -> bool {
    false
}

#[tauri::command]
pub fn clear_auto_selection_cache() {}

