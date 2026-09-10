use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SessionError {
    #[error("Session not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    ParseError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub name: String,
    pub exec: String,
    pub is_wayland: bool,
}

/// Get available desktop sessions (X11 and Wayland)
pub fn get_sessions() -> Result<Vec<Session>, SessionError> {
    let mut sessions = Vec::new();

    // Scan X11 sessions
    if let Ok(x11_sessions) = scan_sessions("/usr/share/xsessions", false) {
        sessions.extend(x11_sessions);
    }

    // Scan Wayland sessions
    if let Ok(wayland_sessions) = scan_sessions("/usr/share/wayland-sessions", true) {
        sessions.extend(wayland_sessions);
    }

    if sessions.is_empty() {
        return Err(SessionError::NotFound(
            "No sessions found".to_string(),
        ));
    }

    // Sort: Wayland first, then X11, alphabetically within each
    sessions.sort_by(|a, b| {
        if a.is_wayland != b.is_wayland {
            b.is_wayland.cmp(&a.is_wayland) // Wayland first (true > false)
        } else {
            a.name.cmp(&b.name)
        }
    });

    Ok(sessions)
}

fn scan_sessions(dir: &str, is_wayland: bool) -> Result<Vec<Session>, SessionError> {
    let path = PathBuf::from(dir);
    let mut sessions = Vec::new();

    if !path.exists() {
        return Ok(sessions); // Directory doesn't exist, return empty
    }

    let entries = fs::read_dir(path)?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "desktop") {
            if let Ok(session) = parse_desktop_file(&path, is_wayland) {
                sessions.push(session);
            }
        }
    }

    Ok(sessions)
}

fn parse_desktop_file(path: &PathBuf, is_wayland: bool) -> Result<Session, SessionError> {
    let content = fs::read_to_string(path)?;
    let mut name = String::new();
    let mut exec = String::new();

    for line in content.lines() {
        if line.starts_with("Name=") {
            name = line.trim_start_matches("Name=").to_string();
        } else if line.starts_with("Exec=") {
            exec = line.trim_start_matches("Exec=").to_string();
        }
    }

    if name.is_empty() || exec.is_empty() {
        return Err(SessionError::ParseError(
            format!("Missing Name or Exec in {:?}", path),
        ));
    }

    Ok(Session {
        name,
        exec,
        is_wayland,
    })
}

/// Set the session environment for next login
pub fn set_session_env(session_name: &str) -> Result<(), SessionError> {
    let xsession_path = dirs::home_dir()
        .ok_or_else(|| SessionError::NotFound("Home directory not found".to_string()))?
        .join(".xsession");

    fs::write(&xsession_path, format!("exec {}\n", session_name))
        .map_err(|e| SessionError::IoError(e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_desktop_line() {
        let content = "Name=GNOME\nExec=gnome-session\nComment=GNOME Desktop";
        let mut name = String::new();
        let mut exec = String::new();

        for line in content.lines() {
            if line.starts_with("Name=") {
                name = line.trim_start_matches("Name=").to_string();
            } else if line.starts_with("Exec=") {
                exec = line.trim_start_matches("Exec=").to_string();
            }
        }

        assert_eq!(name, "GNOME");
        assert_eq!(exec, "gnome-session");
    }

    #[test]
    fn test_session_sorting() {
        let mut sessions = vec![
            Session {
                name: "GNOME".to_string(),
                exec: "gnome-session".to_string(),
                is_wayland: false,
            },
            Session {
                name: "Hyprland".to_string(),
                exec: "Hyprland".to_string(),
                is_wayland: true,
            },
            Session {
                name: "KDE Plasma".to_string(),
                exec: "plasmawayland".to_string(),
                is_wayland: true,
            },
        ];

        sessions.sort_by(|a, b| {
            if a.is_wayland != b.is_wayland {
                b.is_wayland.cmp(&a.is_wayland)
            } else {
                a.name.cmp(&b.name)
            }
        });

        // Wayland sessions first, alphabetically
        assert_eq!(sessions[0].name, "Hyprland");
        assert_eq!(sessions[1].name, "KDE Plasma");
        assert_eq!(sessions[2].name, "GNOME");
    }
}
