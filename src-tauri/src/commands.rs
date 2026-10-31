use crate::android;
use crate::compress;
use crate::convert;
use crate::download;
use crate::types::*;
use crate::AppState;
use tauri::{AppHandle, Manager, State};

fn prepare_output_dir(app: &AppHandle, output_dir: &str) -> Result<String, String> {
    // On Android outputs are always staged in the cache and published afterwards.
    let dir = if output_dir.trim().is_empty() || android::IS_ANDROID {
        crate::default_output_dir(app)
    } else {
        std::path::PathBuf::from(output_dir)
    };
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("出力フォルダを作成できません ({}): {e}", dir.display()))?;
    Ok(dir.display().to_string())
}

#[tauri::command]
pub fn list_tools(state: State<'_, AppState>) -> Vec<ToolInfo> {
    state.tools.list()
}

/// Whether AV1 can be encoded on the GPU (NVENC) with the active FFmpeg.
#[tauri::command]
pub async fn gpu_av1_available(state: State<'_, AppState>) -> Result<bool, String> {
    let tools = state.tools.clone();
    tauri::async_runtime::spawn_blocking(move || {
        tools
            .require("ffmpeg")
            .map(|ffmpeg| compress::av1_nvenc_usable(&ffmpeg))
            .unwrap_or(false)
    })
    .await
    .map_err(|e| format!("GPU 検出タスクが異常終了しました: {e}"))
}

#[tauri::command]
pub fn list_formats() -> Vec<FormatInfo> {
    convert::list_formats()
}

#[tauri::command]
pub fn detect_files(paths: Vec<String>) -> Vec<DetectedFile> {
    paths.iter().map(|p| convert::detect_file(p)).collect()
}

#[tauri::command]
pub fn app_info(app: AppHandle) -> AppInfo {
    build_app_info(&app)
}

fn build_app_info(app: &AppHandle) -> AppInfo {
    let default_output = crate::default_output_dir(app);
    let downloads = app
        .path()
        .download_dir()
        .unwrap_or_else(|_| default_output.clone());
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        default_output_dir: default_output.display().to_string(),
        output_dir_is_custom: crate::prefs::output_dir(app).is_some(),
        downloads_dir: downloads.display().to_string(),
        temp_dir: std::env::temp_dir().display().to_string(),
    }
}

/// Sets the default folder used when a screen has no folder of its own, or
/// clears it (`None`) to fall back to the platform default.
#[tauri::command]
pub fn set_default_output_dir(app: AppHandle, dir: Option<String>) -> Result<AppInfo, String> {
    let dir = dir.unwrap_or_default();
    let dir = dir.trim();
    if dir.is_empty() {
        crate::prefs::save_output_dir(&app, None)?;
        return Ok(build_app_info(&app));
    }

    let path = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&path)
        .map_err(|e| format!("出力フォルダを作成できません ({}): {e}", path.display()))?;
    crate::prefs::save_output_dir(&app, Some(&path.display().to_string()))?;
    Ok(build_app_info(&app))
}

#[tauri::command]
pub async fn convert_files(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
    req: ConvertRequest,
) -> Result<ConvertResponse, String> {
    let req = ConvertRequest {
        output_dir: prepare_output_dir(&app, &req.output_dir)?,
        ..req
    };
    let job = state.jobs.begin(&app, &job_id, "convert")?;
    let tools = state.tools.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || convert::convert_batch(&job, &tools, &req))
            .await
            .map_err(|e| format!("変換タスクが異常終了しました: {e}"));
    state.jobs.finish(&job_id);
    let response = result??;
    if android::IS_ANDROID {
        return Ok(publish_converted(&app, response).await);
    }
    Ok(response)
}

#[tauri::command]
pub async fn probe_media(state: State<'_, AppState>, path: String) -> Result<MediaInfo, String> {
    let tools = state.tools.clone();
    tauri::async_runtime::spawn_blocking(move || compress::probe_media(&tools, &path))
        .await
        .map_err(|e| format!("解析タスクが異常終了しました: {e}"))?
}

#[tauri::command]
pub async fn compress_video(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
    req: CompressRequest,
) -> Result<CompressResponse, String> {
    let job = state.jobs.begin(&app, &job_id, "compress")?;
    let tools = state.tools.clone();
    if let Some(parent) = std::path::Path::new(&req.output).parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let result =
        tauri::async_runtime::spawn_blocking(move || compress::compress(&job, &tools, &req))
            .await
            .map_err(|e| format!("圧縮タスクが異常終了しました: {e}"));
    state.jobs.finish(&job_id);
    let mut response = result??;
    if android::IS_ANDROID {
        response.output = publish_one(&app, response.output).await?;
    }
    Ok(response)
}

#[tauri::command]
pub async fn yt_probe(state: State<'_, AppState>, url: String) -> Result<YtInfo, String> {
    let tools = state.tools.clone();
    tauri::async_runtime::spawn_blocking(move || download::probe_url(&tools, &url))
        .await
        .map_err(|e| format!("解析タスクが異常終了しました: {e}"))?
}

#[tauri::command]
pub async fn download_media(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
    req: DownloadRequest,
) -> Result<DownloadResponse, String> {
    let req = DownloadRequest {
        output_dir: prepare_output_dir(&app, &req.output_dir)?,
        ..req
    };
    let job = state.jobs.begin(&app, &job_id, "download")?;
    let tools = state.tools.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || download::download(&job, &tools, &req))
            .await
            .map_err(|e| format!("ダウンロードタスクが異常終了しました: {e}"));
    state.jobs.finish(&job_id);
    result?
}

#[tauri::command]
pub async fn update_ytdlp(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
) -> Result<ToolUpdate, String> {
    let job = state.jobs.begin(&app, &job_id, "update")?;
    let tools = state.tools.clone();
    let result = tauri::async_runtime::spawn_blocking(move || download::update_ytdlp(&job, &tools))
        .await
        .map_err(|e| format!("更新タスクが異常終了しました: {e}"));
    state.jobs.finish(&job_id);
    result?
}

#[tauri::command]
pub fn cancel_job(state: State<'_, AppState>, job_id: String) -> bool {
    state.jobs.cancel(&job_id)
}

/// Publishes one staged output on a blocking thread (the copy can be large).
async fn publish_one(app: &AppHandle, path: String) -> Result<String, String> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || android::publish_output(&app, &path))
        .await
        .map_err(|e| format!("保存タスクが異常終了しました: {e}"))?
}

/// Publishes every converted file. A file that cannot be published is
/// reported as failed instead of pointing at a path the user cannot reach.
async fn publish_converted(app: &AppHandle, mut response: ConvertResponse) -> ConvertResponse {
    for item in response.results.iter_mut().filter(|item| item.ok) {
        let Some(staged) = item.output.clone() else {
            continue;
        };
        match publish_one(app, staged).await {
            Ok(display) => item.output = Some(display),
            Err(error) => {
                item.ok = false;
                item.output = None;
                item.message = format!("保存できませんでした: {error}");
            }
        }
    }
    response.succeeded = response.results.iter().filter(|item| item.ok).count();
    response.failed = response.results.len() - response.succeeded;
    response
}

/// Android: opens the system picker and returns app-readable copies.
#[tauri::command]
pub async fn pick_files(
    app: AppHandle,
    multiple: bool,
    kind: String,
) -> Result<Vec<android::PickedFile>, String> {
    let mime_types = android::mime_types_for(&kind);
    tauri::async_runtime::spawn_blocking(move || android::pick_files(&app, multiple, mime_types))
        .await
        .map_err(|e| format!("ファイル選択が異常終了しました: {e}"))?
}

/// Android: opens a published output in an external app.
#[tauri::command]
pub async fn open_output(app: AppHandle, path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || android::open_file(&app, &path))
        .await
        .map_err(|e| format!("ファイルを開けません: {e}"))?
}
