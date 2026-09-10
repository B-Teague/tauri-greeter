mod auth;
mod session;
mod power;

use auth::{authenticate, get_users, AuthResult, UserInfo};
use session::{get_sessions, set_session_env, Session};
use power::{execute_power, PowerAction};

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

// Power commands

#[tauri::command]
fn power_shutdown() -> Result<(), String> {
    execute_power(PowerAction::Shutdown).map_err(|e| e.to_string())
}

#[tauri::command]
fn power_reboot() -> Result<(), String> {
    execute_power(PowerAction::Reboot).map_err(|e| e.to_string())
}

#[tauri::command]
fn power_suspend() -> Result<(), String> {
    execute_power(PowerAction::Suspend).map_err(|e| e.to_string())
}

#[tauri::command]
fn power_logout() -> Result<(), String> {
    execute_power(PowerAction::Logout).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            authenticate_user,
            get_available_users,
            get_available_sessions,
            set_session,
            power_shutdown,
            power_reboot,
            power_suspend,
            power_logout
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
