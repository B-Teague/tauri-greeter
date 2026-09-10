use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ThemeError {
    #[error("Theme not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    ParseError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeData {
    pub name: String,
    pub description: Option<String>,
    pub css: String,
}

/// Get available themes from standard locations
pub fn get_themes() -> Result<Vec<ThemeInfo>, ThemeError> {
    let mut themes = Vec::new();

    // Scan user themes first
    let user_themes_dir = dirs::home_dir()
        .ok_or_else(|| ThemeError::NotFound("Home directory not found".to_string()))?
        .join(".local/share/cssdm/themes");

    if user_themes_dir.exists() {
        if let Ok(user_themes) = scan_theme_dir(&user_themes_dir) {
            themes.extend(user_themes);
        }
    }

    // Scan system themes
    let system_themes_dir = PathBuf::from("/usr/share/cssdm/themes");
    if system_themes_dir.exists() {
        if let Ok(system_themes) = scan_theme_dir(&system_themes_dir) {
            themes.extend(system_themes);
        }
    }

    // Always include default theme (from styles.css)
    if !themes.iter().any(|t| t.id == "default") {
        themes.insert(
            0,
            ThemeInfo {
                id: "default".to_string(),
                name: "Default (Dark)".to_string(),
                description: Some("Built-in dark theme".to_string()),
            },
        );
    }

    if themes.is_empty() {
        return Err(ThemeError::NotFound(
            "No themes found, but default should exist".to_string(),
        ));
    }

    Ok(themes)
}

/// Load a theme's CSS content
pub fn load_theme(theme_id: &str) -> Result<String, ThemeError> {
    // ponytail: default theme is embedded in app; custom themes loaded from filesystem
    if theme_id == "default" {
        return get_default_theme_css();
    }

    // Try user themes first
    let user_theme_path = dirs::home_dir()
        .ok_or_else(|| ThemeError::NotFound("Home directory not found".to_string()))?
        .join(format!(".local/share/cssdm/themes/{}/theme.css", theme_id));

    if user_theme_path.exists() {
        return fs::read_to_string(&user_theme_path)
            .map_err(|e| ThemeError::IoError(e));
    }

    // Try system themes
    let system_theme_path = PathBuf::from(format!("/usr/share/cssdm/themes/{}/theme.css", theme_id));
    if system_theme_path.exists() {
        return fs::read_to_string(&system_theme_path)
            .map_err(|e| ThemeError::IoError(e));
    }

    Err(ThemeError::NotFound(format!(
        "Theme '{}' not found",
        theme_id
    )))
}

fn scan_theme_dir(dir: &Path) -> Result<Vec<ThemeInfo>, ThemeError> {
    let mut themes = Vec::new();

    let entries = fs::read_dir(dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Ok(theme_info) = load_theme_metadata(&path) {
                themes.push(theme_info);
            }
        }
    }

    Ok(themes)
}

fn load_theme_metadata(theme_dir: &Path) -> Result<ThemeInfo, ThemeError> {
    let theme_json_path = theme_dir.join("theme.json");

    if theme_json_path.exists() {
        let content = fs::read_to_string(&theme_json_path)?;
        let metadata: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| ThemeError::ParseError(e.to_string()))?;

        let theme_id = theme_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let name = metadata
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&theme_id)
            .to_string();

        let description = metadata
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Ok(ThemeInfo {
            id: theme_id,
            name,
            description,
        })
    } else {
        // Fallback: use directory name as theme id
        let theme_id = theme_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(ThemeInfo {
            id: theme_id.clone(),
            name: theme_id,
            description: None,
        })
    }
}

fn get_default_theme_css() -> Result<String, ThemeError> {
    // Return embedded default theme (CSS variables format)
    Ok(include_str!("../../styles.css").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_theme_exists() {
        let result = load_theme("default");
        assert!(result.is_ok());
        let css = result.unwrap();
        assert!(css.contains("--bg-primary"));
        assert!(css.contains("--accent"));
    }

    #[test]
    fn test_theme_id_from_dir() {
        let test_dir = PathBuf::from("/tmp/test-theme");
        let id = test_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        assert_eq!(id, "test-theme");
    }
}
