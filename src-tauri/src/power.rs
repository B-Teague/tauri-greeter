// Copyright (C) 2026 Brian Teague
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Suspend,
    Reboot,
    Shutdown,
}

impl Action {
    fn systemctl_verb(self) -> &'static str {
        match self {
            Action::Suspend => "suspend",
            Action::Reboot => "reboot",
            Action::Shutdown => "poweroff",
        }
    }
}

/// Hand the request to systemd-logind, which applies its own polkit rules.
// ponytail: systemctl only. Non-systemd init needs its own branch here.
pub fn run(action: Action) -> Result<(), String> {
    let verb = action.systemctl_verb();
    let output = Command::new("systemctl")
        .arg(verb)
        .output()
        .map_err(|e| format!("systemctl {verb}: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "systemctl {verb} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_actions_to_systemctl_verbs() {
        assert_eq!(Action::Suspend.systemctl_verb(), "suspend");
        assert_eq!(Action::Reboot.systemctl_verb(), "reboot");
        assert_eq!(Action::Shutdown.systemctl_verb(), "poweroff");
    }

    #[test]
    fn deserializes_action_names_sent_by_the_ui() {
        let action: Action = serde_json::from_str("\"shutdown\"").unwrap();
        assert_eq!(action.systemctl_verb(), "poweroff");
    }
}
