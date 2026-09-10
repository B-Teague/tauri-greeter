use serde::{Deserialize, Serialize};
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PowerError {
    #[error("Power command failed: {0}")]
    CommandFailed(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PowerAction {
    Shutdown,
    Reboot,
    Suspend,
    Logout,
}

impl PowerAction {
    fn systemctl_arg(&self) -> &'static str {
        match self {
            PowerAction::Shutdown => "poweroff",
            PowerAction::Reboot => "reboot",
            PowerAction::Suspend => "suspend",
            PowerAction::Logout => "exit", // Not a systemctl command
        }
    }
}

/// Execute a power action via systemd or session exit
pub fn execute_power(action: PowerAction) -> Result<(), PowerError> {
    match action {
        PowerAction::Logout => {
            // Logout: just exit the display manager (handled by caller)
            Ok(())
        }
        _ => execute_systemctl(action),
    }
}

fn execute_systemctl(action: PowerAction) -> Result<(), PowerError> {
    // ponytail: uses systemctl which is universal on modern systemd systems
    // Fallback to direct commands (shutdown, reboot, etc) possible for non-systemd
    let arg = action.systemctl_arg();

    let output = Command::new("systemctl")
        .arg(arg)
        .output()
        .map_err(|e| PowerError::CommandFailed(format!("Failed to execute systemctl: {}", e)))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(PowerError::CommandFailed(format!(
            "systemctl {} failed: {}",
            arg, stderr
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_action_args() {
        assert_eq!(PowerAction::Shutdown.systemctl_arg(), "poweroff");
        assert_eq!(PowerAction::Reboot.systemctl_arg(), "reboot");
        assert_eq!(PowerAction::Suspend.systemctl_arg(), "suspend");
    }

    #[test]
    fn test_logout_is_noop() {
        let result = execute_power(PowerAction::Logout);
        assert!(result.is_ok());
    }
}
