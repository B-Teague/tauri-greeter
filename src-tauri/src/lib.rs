// Copyright (C) 2026 Brian Teague
// SPDX-License-Identifier: GPL-3.0-or-later

mod auth;
pub mod daemon;
mod power;
mod session;

use auth::User;
use tauri::Manager;
use power::Action;
use session::Session;
use std::fs;

/// Operator stylesheet, injected by the UI on startup when present.
const THEME_CSS: &str = "/etc/cssdm/theme.css";
/// Installed stylesheets the greeter's picker can switch between.
const THEME_DIR: &str = "/usr/share/cssdm/themes";

#[tauri::command]
fn users() -> Result<Vec<User>, String> {
    auth::users()
}

#[tauri::command]
fn sessions() -> Vec<Session> {
    session::sessions()
}

#[tauri::command]
fn themes() -> Vec<String> {
    let mut names = Vec::new();
    for entry in fs::read_dir(THEME_DIR).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "css") {
            if let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    names
}

/// An empty name means the operator's stylesheet, which is what the greeter
/// shows when nobody touches the picker.
#[tauri::command]
fn theme_css(name: String) -> String {
    let path = if name.is_empty() {
        THEME_CSS.to_string()
    } else if themes().contains(&name) {
        // Only a name the listing actually offers is ever opened, so a crafted
        // request cannot walk out of the theme directory and read, say, shadow.
        format!("{THEME_DIR}/{name}.css")
    } else {
        return String::new();
    };
    fs::read_to_string(path).unwrap_or_default()
}

#[tauri::command]
fn power(action: Action) -> Result<(), String> {
    power::run(action)
}

#[tauri::command]
fn login(username: String, password: String, session: String) -> Result<(), String> {
    let response = daemon::request(&daemon::Request {
        username,
        password,
        session,
    })
    .map_err(|detail| {
        // Socket paths and errno strings say more about the machine than the
        // person at the keyboard needs. The journal gets the real reason.
        eprintln!("cssdm: greeter could not reach the daemon: {detail}");
        "Login is unavailable.".to_string()
    })?;
    match response.error {
        Some(error) => Err(error),
        // The daemon holds the PAM handle and starts the desktop. The greeter's
        // only remaining job is to get off the screen, which means exiting: the
        // daemon stops the X server behind it and releases the display.
        None => std::process::exit(0),
    }
}

pub fn run() {
    // WebKitGTK renders through a DMABUF buffer it allocates with GBM. On the
    // bare Xorg the greeter runs on -- no compositor, and whatever driver the
    // machine happens to have -- that allocation fails ("Failed to create GBM
    // buffer ... Invalid argument") and WebKit answers by painting nothing:
    // a white screen, with no error from us to say why. The software path is
    // fast enough for a login form. Set the variable yourself to opt back in
    // on hardware where the fast path works.
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    tauri::Builder::default()
        .setup(|app| {
            // CSSDM_WINDOWED=1 runs the greeter in an ordinary window, so it can
            // be tested from a running desktop instead of covering the screen.
            if let Some(window) = app.get_webview_window("main") {
                if std::env::var_os("CSSDM_WINDOWED").is_some() {
                    let _ = window.set_fullscreen(false);
                    let _ = window.set_decorations(true);
                    let _ = window.set_size(tauri::LogicalSize::new(1280.0, 800.0));
                    let _ = window.center();
                } else {
                    // No window manager runs on the greeter's display, so there is
                    // nobody to honour a fullscreen request or to hand out focus:
                    // cover the screen by hand and take the keyboard directly.
                    if let Ok(Some(monitor)) = window.current_monitor() {
                        let _ = window.set_position(tauri::PhysicalPosition::new(0, 0));
                        let _ = window.set_size(*monitor.size());
                    }
                    let _ = window.set_focus();
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            users,
            sessions,
            themes,
            theme_css,
            power,
            login
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
