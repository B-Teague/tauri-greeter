// Copyright (C) 2026 Brian Teague
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::auth::Passwd;
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    /// `.desktop` file stem, e.g. "plasma" — what `DESKTOP_SESSION` expects.
    pub id: String,
    pub name: String,
    pub exec: String,
    /// `DesktopNames=`, e.g. "KDE". Empty when the file omits it.
    pub desktop_names: String,
    pub wayland: bool,
}

impl Session {
    pub fn session_type(&self) -> &'static str {
        if self.wayland {
            "wayland"
        } else {
            "x11"
        }
    }

    /// Environment for the desktop process. PAM contributes the rest
    /// (`XDG_RUNTIME_DIR`, `XDG_SESSION_ID`) and overrides what it owns.
    pub fn env(&self, user: &Passwd) -> Vec<(String, String)> {
        let mut env = vec![
            ("HOME".to_string(), user.home.clone()),
            ("PWD".to_string(), user.home.clone()),
            ("USER".to_string(), user.name.clone()),
            ("LOGNAME".to_string(), user.name.clone()),
            ("SHELL".to_string(), user.shell.clone()),
            (
                "PATH".to_string(),
                "/usr/local/sbin:/usr/local/bin:/usr/bin".to_string(),
            ),
            ("XDG_SESSION_TYPE".to_string(), self.session_type().to_string()),
            ("XDG_SESSION_DESKTOP".to_string(), self.id.clone()),
            ("DESKTOP_SESSION".to_string(), self.id.clone()),
        ];
        // Plasma and GNOME key their portal and autostart behaviour off this.
        if !self.desktop_names.is_empty() {
            env.push(("XDG_CURRENT_DESKTOP".to_string(), self.desktop_names.clone()));
        }
        env
    }
}

/// Desktop sessions installed on this machine, Wayland first then X11,
/// alphabetical within each. An empty list is a normal result, not an error.
pub fn sessions() -> Vec<Session> {
    let mut sessions = scan("/usr/share/wayland-sessions", true);
    sessions.extend(scan("/usr/share/xsessions", false));
    sessions.sort_by(|a, b| (!a.wayland, &a.name).cmp(&(!b.wayland, &b.name)));
    sessions
}

fn scan(dir: &str, wayland: bool) -> Vec<Session> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new(); // Missing directory just means no sessions of that kind.
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "desktop"))
        .filter_map(|path| parse_desktop_file(&path, wayland))
        .collect()
}

fn parse_desktop_file(path: &Path, wayland: bool) -> Option<Session> {
    let content = fs::read_to_string(path).ok()?;
    let value = |key: &str| {
        content
            .lines()
            .find_map(|line| line.strip_prefix(key))
            .unwrap_or("")
            .trim()
    };

    if value("Hidden=") == "true" || value("NoDisplay=") == "true" {
        return None;
    }

    let name = value("Name=");
    let exec = strip_field_codes(value("Exec="));
    if name.is_empty() || exec.is_empty() {
        return None;
    }

    Some(Session {
        id: path.file_stem()?.to_string_lossy().into_owned(),
        name: name.to_string(),
        exec,
        desktop_names: value("DesktopNames=").to_string(),
        wayland,
    })
}

/// `Exec=` may carry `%f`/`%U` style field codes for a file the launcher passes
/// in. A session gets no arguments, so drop them; `%%` is a literal percent.
fn strip_field_codes(exec: &str) -> String {
    let mut out = String::with_capacity(exec.len());
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        match c {
            '%' if chars.next() == Some('%') => out.push('%'),
            '%' => {}
            c => out.push(c),
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(name: &str, wayland: bool) -> Session {
        Session {
            id: name.to_lowercase(),
            name: name.to_string(),
            exec: name.to_lowercase(),
            desktop_names: String::new(),
            wayland,
        }
    }

    #[test]
    fn sorts_wayland_first_then_alphabetically() {
        let mut sessions = [
            session("GNOME", false),
            session("KDE Plasma", true),
            session("Awesome", false),
            session("Hyprland", true),
        ];
        sessions.sort_by(|a, b| (!a.wayland, &a.name).cmp(&(!b.wayland, &b.name)));

        let names: Vec<&str> = sessions.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Hyprland", "KDE Plasma", "Awesome", "GNOME"]);
    }

    #[test]
    fn reads_name_exec_and_desktop_names() {
        let path = std::env::temp_dir().join("cssdm-test.desktop");
        fs::write(
            &path,
            "[Desktop Entry]\nName=GNOME\nComment=Nice\nExec=gnome-session %U\nDesktopNames=GNOME\n",
        )
        .unwrap();

        let parsed = parse_desktop_file(&path, true).unwrap();
        assert_eq!(parsed.id, "cssdm-test");
        assert_eq!(parsed.name, "GNOME");
        assert_eq!(parsed.exec, "gnome-session"); // field code stripped
        assert_eq!(parsed.desktop_names, "GNOME");
        assert!(parsed.wayland);

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn rejects_desktop_file_without_exec_and_hidden_ones() {
        for body in [
            "[Desktop Entry]\nName=Broken\n",
            "[Desktop Entry]\nName=Gone\nExec=x\nNoDisplay=true\n",
            "[Desktop Entry]\nName=Old\nExec=x\nHidden=true\n",
        ] {
            let path = std::env::temp_dir().join("cssdm-test-reject.desktop");
            fs::write(&path, body).unwrap();
            assert!(parse_desktop_file(&path, false).is_none(), "should skip: {body}");
            fs::remove_file(&path).unwrap();
        }
    }

    #[test]
    fn strips_field_codes_but_keeps_literal_percent() {
        assert_eq!(strip_field_codes("startplasma-wayland %U"), "startplasma-wayland");
        assert_eq!(strip_field_codes("sh -c 'x %% y'"), "sh -c 'x % y'");
    }

    /// The desktop is unusable without these: no `XDG_CURRENT_DESKTOP` and
    /// Plasma loads the wrong portal, no `HOME` and nothing starts at all.
    #[test]
    fn builds_the_session_environment() {
        let user = Passwd {
            name: "brian".into(),
            uid: 1000,
            gid: 1000,
            home: "/home/brian".into(),
            shell: "/bin/bash".into(),
        };
        let mut plasma = session("KDE Plasma", true);
        plasma.id = "plasma".into();
        plasma.desktop_names = "KDE".into();

        let env = plasma.env(&user);
        let get = |key: &str| env.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());

        assert_eq!(get("HOME"), Some("/home/brian"));
        assert_eq!(get("USER"), Some("brian"));
        assert_eq!(get("SHELL"), Some("/bin/bash"));
        assert_eq!(get("XDG_SESSION_TYPE"), Some("wayland"));
        assert_eq!(get("XDG_CURRENT_DESKTOP"), Some("KDE"));
        assert_eq!(get("DESKTOP_SESSION"), Some("plasma"));

        // X11 sessions must not claim to be Wayland.
        let x11 = session("i3", false);
        assert_eq!(x11.env(&user).iter().find(|(k, _)| k == "XDG_SESSION_TYPE").unwrap().1, "x11");
        // No DesktopNames= means the variable is left unset, not set empty.
        assert!(x11.env(&user).iter().all(|(k, _)| k != "XDG_CURRENT_DESKTOP"));
    }
}
