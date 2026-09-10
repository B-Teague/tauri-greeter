mod auth;

use auth::{authenticate, get_users, AuthResult, UserInfo};

// Tauri command: authenticate user
#[tauri::command]
fn authenticate_user(username: String, password: String) -> AuthResult {
    authenticate(&username, &password)
}

// Tauri command: get available users
#[tauri::command]
fn get_available_users() -> Result<Vec<UserInfo>, String> {
    get_users().map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            authenticate_user,
            get_available_users
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
