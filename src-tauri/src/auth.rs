use serde::{Deserialize, Serialize};
use std::fs;
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Authentication failed: {0}")]
    AuthFailed(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResult {
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub uid: u32,
    pub username: String,
    pub full_name: String,
}

/// Authenticate a user via `su` command (simple, works with system PAM)
pub fn authenticate(username: &str, password: &str) -> AuthResult {
    match authenticate_su(username, password) {
        Ok(_) => AuthResult {
            success: true,
            error: None,
        },
        Err(e) => AuthResult {
            success: false,
            error: Some(e.to_string()),
        },
    }
}

fn authenticate_su(username: &str, password: &str) -> Result<(), AuthError> {
    // Use `echo password | su -l user -c /bin/true` to validate credentials
    // This works because su reads the password from stdin when not on a tty
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new("su")
        .arg("-l")
        .arg(username)
        .arg("-c")
        .arg("/bin/true")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| AuthError::AuthFailed(format!("Failed to spawn su: {}", e)))?;

    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| AuthError::AuthFailed("Failed to open stdin".to_string()))?;
        stdin
            .write_all(password.as_bytes())
            .map_err(|e| AuthError::AuthFailed(format!("Failed to write password: {}", e)))?;
        stdin
            .write_all(b"\n")
            .map_err(|e| AuthError::AuthFailed(format!("Failed to write newline: {}", e)))?;
    }

    let status = child
        .wait()
        .map_err(|e| AuthError::AuthFailed(format!("Failed to wait for su: {}", e)))?;

    if status.success() {
        Ok(())
    } else {
        Err(AuthError::AuthFailed("Invalid credentials".to_string()))
    }
}

/// Get list of available users from /etc/passwd
pub fn get_users() -> Result<Vec<UserInfo>, AuthError> {
    let passwd_content = fs::read_to_string("/etc/passwd")?;
    let mut users = Vec::new();

    for line in passwd_content.lines() {
        if let Some(user) = parse_passwd_line(line) {
            // Filter out system accounts (uid < 1000, unless it's root)
            if user.uid == 0 || user.uid >= 1000 {
                users.push(user);
            }
        }
    }

    // Sort by username for consistent ordering
    users.sort_by(|a, b| a.username.cmp(&b.username));
    Ok(users)
}

fn parse_passwd_line(line: &str) -> Option<UserInfo> {
    let parts: Vec<&str> = line.split(':').collect();
    if parts.len() < 5 {
        return None;
    }

    let username = parts[0].to_string();
    let uid: u32 = parts[2].parse().ok()?;
    let full_name = parts[4]
        .split(',')
        .next()
        .unwrap_or("")
        .to_string();

    let display_name = if full_name.is_empty() {
        username.clone()
    } else {
        full_name
    };

    Some(UserInfo {
        uid,
        username,
        full_name: display_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_passwd_line() {
        let line = "brian:x:1000:1000:Brian Teague,,,:/home/brian:/bin/bash";
        let user = parse_passwd_line(line).unwrap();
        assert_eq!(user.username, "brian");
        assert_eq!(user.uid, 1000);
        assert_eq!(user.full_name, "Brian Teague");
    }

    #[test]
    fn test_parse_root() {
        let line = "root:x:0:0:root:/root:/bin/bash";
        let user = parse_passwd_line(line).unwrap();
        assert_eq!(user.username, "root");
        assert_eq!(user.uid, 0);
    }
}
