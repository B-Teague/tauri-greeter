mod auth;
mod session;

use auth::{authenticate, get_users, AuthResult, UserInfo};
use session::{get_sessions, set_session_env, Session};

// Auth commands

#[tauri::command]
fn authenticate_user(username: String, password: String) -> AuthResult {
    authenticate(&username, &password)
}

#[tauri::command]
fn get_available_users() -> Result<Vec<UserInfo>, String> {
    get_users().map_err(|e| e.to_string())
}

// Session commands

#[tauri::command]
fn get_available_sessions() -> Result<Vec<Session>, String> {
    get_sessions().map_err(|e| e.to_string())
}

#[tauri::command]
fn set_session(session_name: String) -> Result<(), String> {
    set_session_env(&session_name).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            authenticate_user,
            get_available_users,
            get_available_sessions,
            set_session
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
