// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026 Kirsikka (GitHub: Tom039224)

//! Android integration: file picking and publishing results.
//!
//! The engine only works with plain filesystem paths. On Android, user files
//! live behind content URIs and public folders behind MediaStore, so a small
//! Kotlin plugin (`UtilPlugin`) copies picked files into the app cache and
//! publishes finished outputs to `Download/UtilBox`.
//!
//! On every other platform the same API exists but reports that it is not
//! supported, which lets the command layer stay free of `cfg` noise.

use serde::{Deserialize, Serialize};
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{AppHandle, Runtime};

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PickedFile {
    pub path: String,
    pub name: String,
    pub size: u64,
}

#[cfg(target_os = "android")]
mod imp {
    use super::*;
    use tauri::plugin::PluginHandle;
    use tauri::Manager;

    pub struct UtilAndroid<R: Runtime>(PluginHandle<R>);

    #[derive(Deserialize)]
    struct PickResult {
        files: Vec<PickedFile>,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct PickArgs {
        multiple: bool,
        mime_types: Vec<String>,
    }

    #[derive(Serialize)]
    struct ExportArgs<'a> {
        path: &'a str,
    }

    #[derive(Serialize)]
    struct OpenArgs<'a> {
        key: &'a str,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ExportResult {
        display_path: String,
    }

    pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
        Builder::new("utilbox-android")
            .setup(|app, api| {
                let handle = api.register_android_plugin("dev.utilbox.app", "UtilPlugin")?;
                app.manage(UtilAndroid(handle));
                Ok(())
            })
            .build()
    }

    fn handle<R: Runtime>(app: &AppHandle<R>) -> Result<tauri::State<'_, UtilAndroid<R>>, String> {
        app.try_state::<UtilAndroid<R>>()
            .ok_or_else(|| "Android プラグインが初期化されていません".to_string())
    }

    pub fn pick_files<R: Runtime>(
        app: &AppHandle<R>,
        multiple: bool,
        mime_types: Vec<String>,
    ) -> Result<Vec<PickedFile>, String> {
        let result: PickResult = handle(app)?
            .0
            .run_mobile_plugin("pickFiles", PickArgs { multiple, mime_types })
            .map_err(|e| e.to_string())?;
        Ok(result.files)
    }

    /// Copies `path` into Download/UtilBox; returns the display path.
    pub fn export_file<R: Runtime>(app: &AppHandle<R>, path: &str) -> Result<String, String> {
        let result: ExportResult = handle(app)?
            .0
            .run_mobile_plugin("exportFile", ExportArgs { path })
            .map_err(|e| e.to_string())?;
        Ok(result.display_path)
    }

    pub fn open_file<R: Runtime>(app: &AppHandle<R>, key: &str) -> Result<(), String> {
        handle(app)?
            .0
            .run_mobile_plugin::<serde_json::Value>("openFile", OpenArgs { key })
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[cfg(not(target_os = "android"))]
mod imp {
    use super::*;

    const UNSUPPORTED: &str = "この操作は Android 版でのみ利用できます";

    pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
        Builder::new("utilbox-android").build()
    }

    pub fn pick_files<R: Runtime>(
        _app: &AppHandle<R>,
        _multiple: bool,
        _mime_types: Vec<String>,
    ) -> Result<Vec<PickedFile>, String> {
        Err(UNSUPPORTED.into())
    }

    pub fn export_file<R: Runtime>(_app: &AppHandle<R>, _path: &str) -> Result<String, String> {
        Err(UNSUPPORTED.into())
    }

    pub fn open_file<R: Runtime>(_app: &AppHandle<R>, _key: &str) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }
}

pub use imp::{export_file, open_file, pick_files, plugin};

/// MIME filters for the system picker, by the UI's `kind`.
pub fn mime_types_for(kind: &str) -> Vec<String> {
    let list: &[&str] = match kind {
        "video" => &["video/*"],
        "media" => &["image/*", "audio/*", "video/*"],
        _ => &["*/*"],
    };
    list.iter().map(|s| s.to_string()).collect()
}

/// Whether this build runs on Android (outputs must be published to MediaStore).
pub const IS_ANDROID: bool = cfg!(target_os = "android");

/// Publishes a finished output and removes the staged copy.
///
/// Returns the path to show the user. A failure keeps the staged file and the
/// original path, so the result is never lost.
pub fn publish_output<R: Runtime>(app: &AppHandle<R>, path: &str) -> Result<String, String> {
    let display = export_file(app, path)?;
    let _ = std::fs::remove_file(path);
    Ok(display)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_filters_follow_kind() {
        assert_eq!(mime_types_for("video"), vec!["video/*"]);
        assert_eq!(mime_types_for("media").len(), 3);
        assert_eq!(mime_types_for("anything"), vec!["*/*"]);
    }
}
