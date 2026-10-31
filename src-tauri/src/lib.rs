// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026 Kirsikka (GitHub: Tom039224)

pub mod android;
mod commands;
pub mod compress;
pub mod convert;
pub mod download;
pub mod job;
pub mod prefs;
pub mod process;
pub mod tools;
pub mod types;

use job::JobManager;
use tauri::Manager;
use tools::Tools;

pub struct AppState {
    pub tools: Tools,
    pub jobs: JobManager,
}

pub fn default_output_dir(app: &tauri::AppHandle) -> std::path::PathBuf {
    if let Some(dir) = prefs::output_dir(app) {
        return dir;
    }
    if android::IS_ANDROID {
        // Staging area; finished files are published to Download/UtilBox.
        return app
            .path()
            .app_cache_dir()
            .unwrap_or_else(|_| std::env::temp_dir())
            .join("outputs");
    }
    app.path()
        .download_dir()
        .or_else(|_| app.path().home_dir())
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("UtilBox")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(android::plugin())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            if android::IS_ANDROID {
                prepare_android_dirs(app.handle());
            }
            let tools = Tools::discover(app.handle());
            app.manage(AppState {
                tools,
                jobs: JobManager::new(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_tools,
            commands::gpu_av1_available,
            commands::list_formats,
            commands::detect_files,
            commands::app_info,
            commands::set_default_output_dir,
            commands::convert_files,
            commands::probe_media,
            commands::compress_video,
            commands::yt_probe,
            commands::download_media,
            commands::update_ytdlp,
            commands::cancel_job,
            commands::pick_files,
            commands::open_output,
        ])
        .run(tauri::generate_context!())
        .expect("error while running UtilBox");
}

/// Android defaults `temp_dir()` to /data/local/tmp, which apps cannot write.
/// Point it at the app cache and drop leftovers from earlier sessions.
fn prepare_android_dirs(app: &tauri::AppHandle) {
    let Ok(cache) = app.path().app_cache_dir() else {
        return;
    };
    let tmp = cache.join("tmp");
    for stale in ["tmp", "imports", "outputs"] {
        let _ = std::fs::remove_dir_all(cache.join(stale));
    }
    if std::fs::create_dir_all(&tmp).is_ok() {
        std::env::set_var("TMPDIR", &tmp);
    }
}
