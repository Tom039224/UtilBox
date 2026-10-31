// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026 Kirsikka (GitHub: Tom039224)

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// User preferences that survive restarts.
///
/// Only the default output folder is customisable today. Everything else is
/// either detected at boot (tools) or derived from the platform paths.
#[derive(Default, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default)]
    pub output_dir: Option<String>,
}

fn preferences_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("preferences.json"))
}

/// Reads the preferences file, falling back to defaults when it is missing or
/// unreadable. A corrupt file must never stop the app from starting.
fn read(path: &Path) -> Preferences {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Preferences::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn write(path: &Path, prefs: &Preferences) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("設定フォルダを作成できません ({}): {e}", parent.display()))?;
    }
    let body =
        serde_json::to_string_pretty(prefs).map_err(|e| format!("設定を書き出せません: {e}"))?;
    std::fs::write(path, format!("{body}\n"))
        .map_err(|e| format!("設定を保存できません ({}): {e}", path.display()))
}

pub fn load(app: &AppHandle) -> Preferences {
    preferences_path(app)
        .map(|path| read(&path))
        .unwrap_or_default()
}

/// The configured folder, when it is set to something other than the built-in
/// default.
pub fn output_dir(app: &AppHandle) -> Option<PathBuf> {
    load(app)
        .output_dir
        .map(|dir| dir.trim().to_string())
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
}

/// Writes the preferences file. `None` clears the custom folder, restoring the
/// platform default.
pub fn save_output_dir(app: &AppHandle, dir: Option<&str>) -> Result<(), String> {
    let path = preferences_path(app)
        .ok_or_else(|| "設定ファイルの保存先を取得できません".to_string())?;
    let mut prefs = read(&path);
    prefs.output_dir = dir.map(|dir| dir.to_string());
    write(&path, &prefs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("utilbox-prefs-{name}-{}", std::process::id()))
    }

    #[test]
    fn missing_file_means_defaults() {
        let path = scratch("missing").join("preferences.json");
        assert!(read(&path).output_dir.is_none());
    }

    #[test]
    fn folder_round_trips_and_clears() {
        let dir = scratch("roundtrip");
        let path = dir.join("preferences.json");

        let mut prefs = Preferences::default();
        prefs.output_dir = Some("/tmp/UtilBox".into());
        write(&path, &prefs).unwrap();
        assert_eq!(read(&path).output_dir.as_deref(), Some("/tmp/UtilBox"));

        prefs.output_dir = None;
        write(&path, &prefs).unwrap();
        assert!(read(&path).output_dir.is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn corrupt_file_means_defaults() {
        let dir = scratch("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("preferences.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert!(read(&path).output_dir.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
