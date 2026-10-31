use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Tools / diagnostics
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub label: String,
    pub purpose: String,
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub bundled: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub platform: String,
    pub arch: String,
    pub default_output_dir: String,
    pub output_dir_is_custom: bool,
    pub downloads_dir: String,
    pub temp_dir: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToolUpdate {
    pub info: ToolInfo,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FormatInfo {
    pub id: String,
    pub label: String,
    pub ext: String,
    pub category: String,
    pub can_input: bool,
    pub can_output: bool,
    pub note: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DetectedFile {
    pub path: String,
    pub name: String,
    pub ext: String,
    pub size: u64,
    pub category: String,
    pub format_id: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConvertRequest {
    pub inputs: Vec<String>,
    /// Target format id (e.g. "png", "mp4", "pdf").
    pub target: String,
    pub output_dir: String,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConvertItemResult {
    pub input: String,
    pub name: String,
    pub output: Option<String>,
    pub ok: bool,
    pub message: String,
    pub size_before: u64,
    pub size_after: u64,
    pub elapsed_ms: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConvertResponse {
    pub results: Vec<ConvertItemResult>,
    pub succeeded: usize,
    pub failed: usize,
}

// ---------------------------------------------------------------------------
// Media / compression
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub path: String,
    pub size: u64,
    pub duration_sec: f64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub bitrate_kbps: Option<u64>,
    pub fps: Option<f64>,
    pub container: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CompressRequest {
    pub input: String,
    pub output: String,
    /// "crf" (quality based) or "size" (two-pass target size).
    pub mode: String,
    pub crf: Option<u32>,
    pub target_mb: Option<f64>,
    /// None / 0 means keep source resolution.
    pub max_height: Option<u32>,
    /// "h264", "h265", or "av1"
    pub codec: String,
    pub audio_kbps: Option<u32>,
    #[serde(default)]
    pub remove_audio: bool,
    pub preset: Option<String>,
    /// "encode" (default) or "copy".
    pub audio_mode: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CompressResponse {
    pub output: String,
    pub size_before: u64,
    pub size_after: u64,
    pub elapsed_ms: u64,
    pub media: Option<MediaInfo>,
    pub encoder: String,
}

// ---------------------------------------------------------------------------
// YouTube
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct YtInfo {
    pub id: String,
    pub title: String,
    pub uploader: Option<String>,
    pub duration_sec: f64,
    pub thumbnail: Option<String>,
    pub webpage_url: String,
    pub live: bool,
    pub view_count: Option<u64>,
    pub description: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadCompression {
    /// "crf" (quality based) or "size" (two-pass target size).
    pub mode: String,
    pub crf: Option<u32>,
    pub target_mb: Option<f64>,
    /// None / 0 means keep source resolution.
    pub max_height: Option<u32>,
    /// "h264", "h265", or "av1"
    pub codec: String,
    pub audio_kbps: Option<u32>,
    #[serde(default)]
    pub remove_audio: bool,
    pub preset: Option<String>,
    /// "encode" (default) or "copy".
    pub audio_mode: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    pub url: String,
    pub output_dir: String,
    /// "video" or "audio"
    pub mode: String,
    /// Target height for video ("2160", "1440", "1080", "720", "480", "360", "best").
    pub quality: String,
    /// "mp3", "m4a", "opus" — only for audio mode.
    pub audio_format: Option<String>,
    #[serde(default)]
    pub embed_thumbnail: bool,
    #[serde(default)]
    pub embed_metadata: bool,
    pub cookies_browser: Option<String>,
    /// Optional MP4 compression applied after the download finishes.
    pub compression: Option<DownloadCompression>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResponse {
    pub file: String,
    pub title: String,
    pub size: u64,
    pub elapsed_ms: u64,
    /// Present when post-download compression was requested and succeeded.
    pub compression: Option<CompressResponse>,
}

// ---------------------------------------------------------------------------
// Job events
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub job_id: String,
    pub tool: String,
    pub phase: String,
    /// 0.0 .. 100.0
    pub percent: f64,
    pub message: String,
    pub detail: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LogEvent {
    pub job_id: String,
    pub line: String,
}
