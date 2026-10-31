// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026 Kirsikka (GitHub: Tom039224)

//! YouTube (and other yt-dlp supported sites) downloading.
//!
//! Only the user explicitly starting a download/probe/update causes network
//! access; everything else in the app stays offline.

use crate::compress::compress_with_range;
use crate::job::Job;
use crate::process::{run_capture, run_streaming, Cmd, Stream};
use crate::tools::{find_in_path, Tools};
use crate::types::*;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

const PROGRESS_TEMPLATE: &str = "download:DL|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s|%(progress.fragment_index)s|%(progress.fragment_count)s";

/// Share of the progress bar given to the download when compression follows.
/// The encode is the slow part (AV1 especially), so it gets the larger share;
/// otherwise the bar races to ~85% and then appears to crawl.
const DOWNLOAD_SHARE: f64 = 25.0;

const COOKIE_BROWSERS: &[&str] = &[
    "chrome", "chromium", "firefox", "edge", "brave", "safari", "opera", "vivaldi",
];

// ---------------------------------------------------------------------------
// probe
// ---------------------------------------------------------------------------

/// Fetches video metadata without downloading the media itself.
pub fn probe_url(tools: &Tools, url: &str) -> Result<YtInfo, String> {
    let url = validate_url(url)?;
    let exe = tools.require("yt-dlp")?;
    let cmd = Cmd::new(exe.as_path()).args([
        "-J",
        "--no-warnings",
        "--no-playlist",
        "--skip-download",
        "--no-progress",
        url,
    ]);
    let outcome = run_capture(&cmd)?;
    if !outcome.success() {
        let detail = truncate(&outcome.error_text(), 700);
        let hint = probe_hint(&format!("{}\n{}", outcome.stderr, outcome.stdout));
        return Err(format!("動画情報を取得できませんでした: {detail}\n{hint}"));
    }
    parse_yt_json(&outcome.stdout, url)
}

fn parse_yt_json(raw: &str, fallback_url: &str) -> Result<YtInfo, String> {
    let root: Value = serde_json::from_str(raw)
        .map_err(|e| format!("yt-dlp の出力を解析できませんでした: {e}"))?;

    let entry = if root.get("_type").and_then(Value::as_str) == Some("playlist") {
        root.get("entries")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .ok_or_else(|| "プレイリストに動画が含まれていません".to_string())?
    } else {
        &root
    };

    let id = json_str(entry, "id")
        .ok_or_else(|| "動画 ID を取得できませんでした（URL を確認してください）".to_string())?;
    let title = json_str(entry, "title").unwrap_or_else(|| id.clone());
    let uploader = json_str(entry, "uploader")
        .or_else(|| json_str(entry, "channel"))
        .or_else(|| json_str(entry, "uploader_id"));
    let duration_sec = entry
        .get("duration")
        .and_then(|v| v.as_f64().or_else(|| v.as_u64().map(|n| n as f64)))
        .filter(|d| d.is_finite() && *d >= 0.0)
        .unwrap_or(0.0);
    let thumbnail = json_str(entry, "thumbnail").or_else(|| {
        entry
            .get("thumbnails")
            .and_then(Value::as_array)
            .and_then(|items| items.last())
            .and_then(|item| json_str(item, "url"))
    });
    let webpage_url = json_str(entry, "webpage_url")
        .or_else(|| json_str(entry, "url"))
        .unwrap_or_else(|| fallback_url.to_string());
    let live = entry
        .get("is_live")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || matches!(
            json_str(entry, "live_status").as_deref(),
            Some("is_live") | Some("is_live_starting") | Some("is_upcoming")
        );
    let view_count = entry.get("view_count").and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_f64().filter(|n| *n >= 0.0).map(|n| n as u64))
    });
    let description = json_str(entry, "description").map(|d| truncate(&d, 400));

    Ok(YtInfo {
        id,
        title,
        uploader,
        duration_sec,
        thumbnail,
        webpage_url,
        live,
        view_count,
        description,
    })
}

fn json_str(value: &Value, key: &str) -> Option<String> {
    let text = value.get(key)?.as_str()?.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn validate_url(raw: &str) -> Result<&str, String> {
    let url = raw.trim();
    if url.is_empty() {
        return Err("URL が指定されていません".into());
    }
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err("http:// または https:// で始まる URL を指定してください".into());
    }
    if url.chars().any(char::is_whitespace) {
        return Err("URL に空白が含まれています".into());
    }
    Ok(url)
}

// ---------------------------------------------------------------------------
// download options
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Video,
    Audio,
}

struct Plan {
    mode: Mode,
    selector: Option<String>,
    sort: Option<String>,
    audio_format: Option<&'static str>,
    cookies: Option<String>,
}

fn parse_mode(raw: &str) -> Result<Mode, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "video" => Ok(Mode::Video),
        "audio" => Ok(Mode::Audio),
        _ => Err("モードは video または audio を指定してください".into()),
    }
}

/// Returns (`-f` selector, `-S` sort).
fn video_format(quality: &str) -> Result<(String, String), String> {
    let quality = quality.trim().to_ascii_lowercase();
    if quality.is_empty() || quality == "best" {
        return Ok((
            "bv*+ba/b".to_string(),
            "res,fps,vcodec:h264,acodec:m4a".to_string(),
        ));
    }
    let height: u32 = quality.parse().map_err(|_| {
        "画質は best または 2160 / 1440 / 1080 / 720 / 480 / 360 から選択してください".to_string()
    })?;
    if !matches!(height, 2160 | 1440 | 1080 | 720 | 480 | 360) {
        return Err(format!(
            "画質「{quality}」には対応していません。best または 2160 / 1440 / 1080 / 720 / 480 / 360 を指定してください。"
        ));
    }
    Ok((
        format!("bv*[height<={height}]+ba/b[height<={height}]/b"),
        format!("res:{height},fps,vcodec:h264,acodec:m4a"),
    ))
}

fn audio_format(raw: Option<&str>) -> Result<&'static str, String> {
    match raw
        .map(str::trim)
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "" | "mp3" => Ok("mp3"),
        "m4a" => Ok("m4a"),
        "opus" => Ok("opus"),
        other => Err(format!(
            "音声形式は mp3 / m4a / opus から選択してください（指定: {other}）"
        )),
    }
}

fn normalize_cookies(raw: Option<&str>) -> Result<Option<String>, String> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(browser) => {
            let lower = browser.to_ascii_lowercase();
            if COOKIE_BROWSERS.contains(&lower.as_str()) {
                Ok(Some(lower))
            } else {
                Err(format!(
                    "Cookie の取得元「{browser}」には対応していません。{} のいずれかを指定してください。",
                    COOKIE_BROWSERS.join(" / ")
                ))
            }
        }
    }
}

fn plan_download(req: &DownloadRequest) -> Result<Plan, String> {
    let mode = parse_mode(&req.mode)?;
    let cookies = normalize_cookies(req.cookies_browser.as_deref())?;
    match mode {
        Mode::Video => {
            let (selector, sort) = video_format(&req.quality)?;
            Ok(Plan {
                mode,
                selector: Some(selector),
                sort: Some(sort),
                audio_format: None,
                cookies,
            })
        }
        Mode::Audio => Ok(Plan {
            mode,
            selector: None,
            sort: None,
            audio_format: Some(audio_format(req.audio_format.as_deref())?),
            cookies,
        }),
    }
}

fn build_download_args(
    plan: &Plan,
    req: &DownloadRequest,
    out_dir: &Path,
    ffmpeg_dir: &Path,
    url: &str,
) -> Vec<String> {
    let mut args: Vec<String> = [
        "--no-playlist",
        "--newline",
        "--no-warnings",
        "--progress",
        "--no-simulate",
        "--continue",
        "--paths",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.push(out_dir.display().to_string());
    args.push("-o".into());
    args.push("%(title).180B [%(id)s].%(ext)s".into());
    args.push("--print".into());
    args.push("after_move:filepath".into());
    args.push("--progress-template".into());
    args.push(PROGRESS_TEMPLATE.into());
    args.push("--ffmpeg-location".into());
    args.push(ffmpeg_dir.display().to_string());

    if let Some(browser) = &plan.cookies {
        args.push("--cookies-from-browser".into());
        args.push(browser.clone());
    }

    match plan.mode {
        Mode::Video => {
            if let Some(selector) = &plan.selector {
                args.push("-f".into());
                args.push(selector.clone());
            }
            if let Some(sort) = &plan.sort {
                args.push("-S".into());
                args.push(sort.clone());
            }
            args.push("--merge-output-format".into());
            args.push("mp4".into());
        }
        Mode::Audio => {
            args.push("-x".into());
            args.push("--audio-format".into());
            args.push(plan.audio_format.unwrap_or("mp3").to_string());
            args.push("--audio-quality".into());
            args.push("0".into());
        }
    }

    if req.embed_thumbnail {
        args.push("--embed-thumbnail".into());
    }
    if req.embed_metadata {
        args.push("--embed-metadata".into());
    }

    args.push(url.to_string());
    args
}

// ---------------------------------------------------------------------------
// progress / output helpers
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
struct DlProgress {
    percent: Option<f64>,
    speed: Option<String>,
    eta: Option<String>,
}

fn parse_dl_line(line: &str) -> Option<DlProgress> {
    let rest = line.trim().strip_prefix("DL|")?;
    let parts: Vec<&str> = rest.split('|').collect();
    if parts.len() < 7 {
        return None;
    }
    let downloaded = parse_num(parts[0]);
    let total = parse_num(parts[1])
        .filter(|v| *v > 0.0)
        .or_else(|| parse_num(parts[2]).filter(|v| *v > 0.0));
    let speed = parse_num(parts[3]).filter(|s| *s > 0.0).map(format_speed);
    let eta = parse_num(parts[4]).map(|s| format_eta(s.max(0.0).round() as u64));
    let frag_index = parse_num(parts[5]);
    let frag_count = parse_num(parts[6]);

    let percent = match (downloaded, total) {
        (Some(done), Some(all)) if all > 0.0 => Some((done / all * 100.0).clamp(0.0, 100.0)),
        _ => match (frag_index, frag_count) {
            (Some(done), Some(all)) if all > 0.0 => Some((done / all * 100.0).clamp(0.0, 100.0)),
            _ => None,
        },
    };

    Some(DlProgress {
        percent,
        speed,
        eta,
    })
}

fn parse_num(raw: &str) -> Option<f64> {
    let raw = raw.trim();
    if raw.is_empty()
        || raw.eq_ignore_ascii_case("na")
        || raw.eq_ignore_ascii_case("n/a")
        || raw.eq_ignore_ascii_case("none")
    {
        return None;
    }
    raw.parse::<f64>().ok().filter(|n| n.is_finite())
}

fn format_speed(bytes_per_sec: f64) -> String {
    if !bytes_per_sec.is_finite() || bytes_per_sec <= 0.0 {
        return "—".to_string();
    }
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    if bytes_per_sec >= GB {
        format!("{:.2} GB/s", bytes_per_sec / GB)
    } else if bytes_per_sec >= MB {
        format!("{:.1} MB/s", bytes_per_sec / MB)
    } else if bytes_per_sec >= KB {
        format!("{:.1} KB/s", bytes_per_sec / KB)
    } else {
        format!("{:.0} B/s", bytes_per_sec)
    }
}

fn format_eta(total_secs: u64) -> String {
    if total_secs >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            total_secs / 3600,
            (total_secs % 3600) / 60,
            total_secs % 60
        )
    } else {
        format!("{:02}:{:02}", total_secs / 60, total_secs % 60)
    }
}

fn postprocess_message(line: &str) -> Option<&'static str> {
    let line = line.trim_start();
    let tag = line.split_once(']').map(|(tag, _)| tag).unwrap_or("");
    match tag {
        "[ExtractAudio" => Some("音声を抽出しています…"),
        "[Merger" => Some("映像と音声を結合しています…"),
        "[VideoConvertor" | "[VideoRemuxer" => Some("コンテナを変換しています…"),
        "[ThumbnailsConvertor" => Some("サムネイルを変換しています…"),
        "[Metadata" => Some("メタデータを書き込んでいます…"),
        "[EmbedThumbnail" => Some("サムネイルを埋め込んでいます…"),
        "[FixupM4a" | "[FixupM3u8" => Some("ファイルを調整しています…"),
        _ => None,
    }
}

fn is_temp_artifact(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    name.ends_with(".part")
        || name.ends_with(".ytdl")
        || name.ends_with(".temp")
        || name.ends_with(".tmp")
        || name.contains(".part-frag")
        || name.ends_with(".jpg")
        || name.ends_with(".jpeg")
        || name.ends_with(".png")
        || name.ends_with(".webp")
}

fn is_media_file(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "mp4"
            | "mkv"
            | "webm"
            | "mov"
            | "avi"
            | "mp3"
            | "m4a"
            | "opus"
            | "ogg"
            | "oga"
            | "flac"
            | "wav"
            | "aac"
    )
}

fn existing_path_from(text: &str) -> Option<PathBuf> {
    text.lines().rev().find_map(|line| {
        let line = line.trim().trim_matches('"');
        if line.is_empty()
            || line.starts_with('[')
            || line.starts_with("ERROR:")
            || line.contains("DL|")
        {
            return None;
        }
        let path = PathBuf::from(line);
        path.is_file().then_some(path)
    })
}

/// Finds the newest completed output if yt-dlp did not print its path.
fn newest_output(dir: &Path, since: SystemTime) -> Option<PathBuf> {
    let threshold = since.checked_sub(Duration::from_secs(2)).unwrap_or(since);
    let mut best: Option<(bool, SystemTime, PathBuf)> = None;
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    while let Some((current, depth)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            let path = entry.path();
            if meta.is_dir() {
                if depth < 1 {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            if !meta.is_file() || is_temp_artifact(&path) {
                continue;
            }
            let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            if modified < threshold {
                continue;
            }
            let media = is_media_file(&path);
            let better = match &best {
                None => true,
                Some((best_media, best_modified, _)) => {
                    (media, modified) > (*best_media, *best_modified)
                }
            };
            if better {
                best = Some((media, modified, path));
            }
        }
    }
    best.map(|(_, _, path)| path)
}

/// Picks a non-existing `*_compressed.mp4` path next to the downloaded file.
fn compressed_output_path(input: &Path) -> PathBuf {
    let parent = input.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("video");
    let mut candidate = parent.join(format!("{stem}_compressed.mp4"));
    let mut index = 2;
    while candidate.exists() {
        candidate = parent.join(format!("{stem}_compressed-{index}.mp4"));
        index += 1;
    }
    candidate
}

fn error_hints(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut hints: Vec<&str> = Vec::new();
    if lower.contains("sign in to confirm your age") || lower.contains("age-restricted") {
        hints.push("年齢制限付きの動画です。ブラウザーの Cookie を指定して再試行してください。");
    }
    if lower.contains("sign in to confirm you're not a bot")
        || lower.contains("sign in to confirm you’re not a bot")
    {
        hints.push(
            "ボット確認が求められています。ブラウザーの Cookie を指定して再試行してください。",
        );
    }
    if lower.contains("private video") {
        hints.push("非公開の動画です。URL と公開設定を確認してください。");
    }
    if lower.contains("video unavailable")
        || lower.contains("this video is not available")
        || lower.contains("removed by the uploader")
    {
        hints.push("動画を利用できません（削除・非公開・地域制限の可能性）。");
    }
    if lower.contains("not available in your country") || lower.contains("geo restricted") {
        hints.push("地域制限付きの動画です。この環境からは取得できません。");
    }
    if lower.contains("ffmpeg") && (lower.contains("not found") || lower.contains("not installed"))
    {
        hints.push(
            "FFmpeg が見つかりません。src-tauri/binaries に配置するか PATH に追加してください。",
        );
    }
    if lower.contains("unsupported url") {
        hints.push("この URL は yt-dlp では扱えません。動画ページの URL を指定してください。");
    }
    if lower.contains("http error 429") || lower.contains("too many requests") {
        hints.push("アクセスが集中しています。しばらく待ってから再試行してください。");
    }
    hints.join("\n")
}

fn probe_hint(text: &str) -> String {
    let hints = error_hints(text);
    if hints.is_empty() {
        "URL を確認してください（非公開・年齢制限・地域制限の可能性）。".to_string()
    } else {
        hints
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    let text = text.trim();
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push('…');
    out
}

// ---------------------------------------------------------------------------
// download
// ---------------------------------------------------------------------------

/// Downloads a video or extracts audio with yt-dlp.
pub fn download(
    job: &Job,
    tools: &Tools,
    req: &DownloadRequest,
) -> Result<DownloadResponse, String> {
    let started = Instant::now();
    job.check()?;

    let url = validate_url(&req.url)?.to_string();
    let plan = plan_download(req)?;
    if req.compression.is_some() && plan.mode == Mode::Audio {
        return Err(
            "音声のみのダウンロードでは圧縮を利用できません。動画モードを選択してください。".into(),
        );
    }
    // Download occupies 0–DOWNLOAD_SHARE% when compression follows, so the
    // encode can use the remaining range.
    let download_job = if req.compression.is_some() {
        job.scaled(0.0, DOWNLOAD_SHARE)
    } else {
        job.clone()
    };

    let out_dir = PathBuf::from(&req.output_dir);
    if out_dir.as_os_str().is_empty() {
        return Err("出力フォルダが指定されていません".into());
    }
    fs::create_dir_all(&out_dir)
        .map_err(|e| format!("出力フォルダを作成できません ({}): {e}", out_dir.display()))?;
    if !out_dir.is_dir() {
        return Err(format!(
            "出力先がフォルダではありません: {}",
            out_dir.display()
        ));
    }

    let ytdlp = tools.require("yt-dlp")?;
    let ffmpeg = tools.require("ffmpeg")?;
    let ffmpeg_dir = ffmpeg
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    job.check()?;
    download_job.progress(2.0, "解析", "URL を確認しています…");
    // Metadata is only used for the title; a failed probe must not stop the
    // actual download.
    let probed = match probe_url(tools, &url) {
        Ok(info) => Some(info),
        Err(e) => {
            job.log(format!(
                "動画情報を取得できませんでした（ダウンロードは続行します）: {e}"
            ));
            None
        }
    };
    job.check()?;

    let args = build_download_args(&plan, req, &out_dir, &ffmpeg_dir, &url);
    let cmd = Cmd::new(ytdlp.as_path()).args(args);
    job.log(format!("実行: {}", cmd.display()));
    download_job.progress(10.0, "ダウンロード", "ダウンロードを開始しています…");

    let scan_since = SystemTime::now();
    let mut last_path: Option<PathBuf> = None;
    let mut post_percent = 91.0_f64;

    let outcome = run_streaming(&download_job, &cmd, |stream, line| {
        let text = line.trim();
        if text.is_empty() {
            return;
        }

        if let Some(progress) = parse_dl_line(text) {
            if let Some(percent) = progress.percent {
                let percent = percent.clamp(0.0, 90.0);
                let detail = format!(
                    "{:.0}% · {} · 残り {}",
                    download_job.map_percent(percent),
                    progress.speed.as_deref().unwrap_or("—"),
                    progress.eta.as_deref().unwrap_or("—")
                );
                download_job.progress_detail(
                    percent,
                    "ダウンロード",
                    "ダウンロード中…",
                    Some(detail),
                );
            }
            return;
        }

        if let Some(message) = postprocess_message(text) {
            post_percent = (post_percent + 2.0).min(98.0);
            download_job.progress(post_percent, "変換/結合", message);
            return;
        }

        if text.starts_with("[download] Destination:") {
            job.log(truncate(text, 240));
            return;
        }

        if stream == Stream::Err {
            if text.starts_with("ERROR") || text.contains("ffmpeg") {
                job.log(truncate(text, 260));
            }
            return;
        }

        let candidate = PathBuf::from(text.trim_matches('"'));
        if candidate.is_file() {
            job.log(format!("保存: {}", candidate.display()));
            last_path = Some(candidate);
        }
    });

    let outcome = outcome?;

    if outcome.cancelled || job.is_cancelled() {
        return Err("キャンセルされました".into());
    }
    if !outcome.success() {
        let detail = truncate(&outcome.error_text(), 800);
        let hints = error_hints(&format!(
            "{}\n{}\n{}",
            outcome.stderr, outcome.stdout, detail
        ));
        let mut message = format!("ダウンロードに失敗しました: {detail}");
        if !hints.is_empty() {
            message.push('\n');
            message.push_str(&hints);
        }
        return Err(message);
    }

    download_job.progress(96.0, "完了", "出力ファイルを確認しています…");
    let file = last_path
        .filter(|p| p.is_file())
        .or_else(|| existing_path_from(&outcome.stdout))
        .or_else(|| existing_path_from(&outcome.stderr))
        .or_else(|| newest_output(&out_dir, scan_since))
        .ok_or_else(|| "出力ファイルを特定できませんでした".to_string())?;

    let raw_size = fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
    let title = probed
        .as_ref()
        .map(|info| info.title.trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| {
            file.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "ダウンロード".to_string())
        });
    job.log(format!("出力: {}", file.display()));

    let mut final_file = file.clone();
    let mut final_size = raw_size;
    let mut compression = None;

    if let Some(config) = &req.compression {
        job.check()?;
        job.log(format!(
            "ダウンロード後に圧縮します（{} / {}）",
            config.codec,
            match config.mode.trim() {
                "size" => format!("目標 {:.1} MB（2パス）", config.target_mb.unwrap_or(0.0)),
                _ => format!("CRF {}", config.crf.unwrap_or(0)),
            }
        ));
        job.progress(DOWNLOAD_SHARE, "圧縮準備", "圧縮の準備をしています…");

        let output = compressed_output_path(&file);
        let creq = CompressRequest {
            input: file.display().to_string(),
            output: output.display().to_string(),
            mode: config.mode.clone(),
            crf: config.crf,
            target_mb: config.target_mb,
            max_height: config.max_height,
            codec: config.codec.clone(),
            audio_kbps: config.audio_kbps,
            remove_audio: config.remove_audio,
            preset: config.preset.clone(),
            audio_mode: config.audio_mode.clone(),
        };

        match compress_with_range(job, tools, &creq, DOWNLOAD_SHARE, 100.0) {
            Ok(result) => {
                job.log(format!("圧縮後のファイル: {}", result.output));
                match fs::remove_file(&file) {
                    Ok(()) => job.log("圧縮前の元ファイルを削除しました"),
                    Err(e) => job.log(format!(
                        "圧縮前の元ファイルを削除できませんでした ({}): {e}",
                        file.display()
                    )),
                }
                final_file = PathBuf::from(&result.output);
                final_size = result.size_after;
                compression = Some(result);
            }
            Err(e) => {
                if job.is_cancelled() {
                    return Err(
                        "キャンセルされました（ダウンロードした元ファイルは保持しています）".into(),
                    );
                }
                return Err(format!(
                    "ダウンロードは完了しましたが、圧縮に失敗しました。\n元ファイルは保持しています: {}\n{e}",
                    file.display()
                ));
            }
        }
    }

    let elapsed_ms = started.elapsed().as_millis() as u64;
    job.progress(
        100.0,
        "完了",
        if compression.is_some() {
            "ダウンロードと圧縮が完了しました"
        } else {
            "ダウンロードが完了しました"
        },
    );

    Ok(DownloadResponse {
        file: final_file.display().to_string(),
        title,
        size: final_size,
        elapsed_ms,
        compression,
    })
}

// ---------------------------------------------------------------------------
// yt-dlp updater
// ---------------------------------------------------------------------------

fn ytdlp_download_url() -> Result<&'static str, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok(
            "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux",
        ),
        ("linux", "aarch64") => Ok(
            "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux_aarch64",
        ),
        ("macos", _) => Ok(
            "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos",
        ),
        ("windows", _) => Ok(
            "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe",
        ),
        (os, arch) => Err(format!(
            "この環境（{os}/{arch}）用の yt-dlp は自動更新に対応していません。手動で更新してください。"
        )),
    }
}

fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| truncate(line, 120))
}

/// Downloads the latest official yt-dlp binary into the user bin directory.
pub fn update_ytdlp(job: &Job, tools: &Tools) -> Result<ToolUpdate, String> {
    job.check()?;
    job.progress(2.0, "準備", "yt-dlp の更新を準備しています…");

    let url = ytdlp_download_url()?;
    let user_bin = tools.user_bin().to_path_buf();
    fs::create_dir_all(&user_bin).map_err(|e| {
        format!(
            "更新用フォルダを作成できません ({}): {e}",
            user_bin.display()
        )
    })?;

    let exe_name = if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    };
    let target = user_bin.join(exe_name);
    let token = uuid::Uuid::new_v4();
    let tmp = user_bin.join(format!(".yt-dlp-download-{token}.tmp"));
    let backup = user_bin.join(format!(".yt-dlp-backup-{token}.tmp"));

    job.progress(
        20.0,
        "ダウンロード",
        "最新版の yt-dlp をダウンロードしています…",
    );
    let download_result = if let Some(curl) = find_in_path(&["curl"]) {
        let args: Vec<String> = vec![
            "-fL".into(),
            "--retry".into(),
            "3".into(),
            "--connect-timeout".into(),
            "20".into(),
            "-sS".into(),
            "-o".into(),
            tmp.display().to_string(),
            url.into(),
        ];
        let cmd = Cmd::new(curl.as_path()).args(args);
        run_streaming(job, &cmd, |_stream, _line| {})
    } else if let Some(wget) = find_in_path(&["wget"]) {
        let args: Vec<String> = vec![
            "-nv".into(),
            "-O".into(),
            tmp.display().to_string(),
            url.into(),
        ];
        let cmd = Cmd::new(wget.as_path()).args(args);
        run_streaming(job, &cmd, |_stream, _line| {})
    } else {
        return Err("curl または wget が必要です".into());
    };

    let outcome = match download_result {
        Ok(outcome) => outcome,
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
    };
    if outcome.cancelled || job.is_cancelled() {
        let _ = fs::remove_file(&tmp);
        return Err("キャンセルされました".into());
    }
    if !outcome.success() {
        let _ = fs::remove_file(&tmp);
        return Err(format!(
            "yt-dlp のダウンロードに失敗しました: {}",
            truncate(&outcome.error_text(), 500)
        ));
    }
    job.progress(80.0, "ダウンロード", "ダウンロードが完了しました");

    let size = fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
    if size == 0 {
        let _ = fs::remove_file(&tmp);
        return Err("ダウンロードした yt-dlp が空です".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755)) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("yt-dlp に実行権限を設定できません: {e}"));
        }
    }

    if let Err(e) = job.check() {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    job.progress(85.0, "検証", "新しい yt-dlp を確認しています…");
    let version_out = match run_capture(&Cmd::new(tmp.as_path()).arg("--version")) {
        Ok(out) => out,
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
    };
    let version = if version_out.success() {
        first_line(&version_out.stdout)
    } else {
        None
    };
    let Some(version) = version else {
        let _ = fs::remove_file(&tmp);
        return Err("新しい yt-dlp を実行できませんでした。更新を中止しました。".into());
    };

    let had_old = target.is_file();
    if had_old {
        fs::rename(&target, &backup).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            format!("既存の yt-dlp を退避できませんでした: {e}")
        })?;
    }
    if let Err(e) = fs::rename(&tmp, &target) {
        if had_old {
            let _ = fs::rename(&backup, &target);
        }
        let _ = fs::remove_file(&tmp);
        return Err(format!("yt-dlp を配置できませんでした: {e}"));
    }

    // Final sanity check on the installed path. Restore the previous binary if
    // the replacement unexpectedly cannot run.
    let installed_ok = matches!(
        run_capture(&Cmd::new(target.as_path()).arg("--version")),
        Ok(out) if out.success()
    );
    if !installed_ok {
        let _ = fs::remove_file(&target);
        if had_old {
            let _ = fs::rename(&backup, &target);
        }
        let message = if had_old {
            "更新後の yt-dlp を実行できませんでした。以前のバージョンに戻しました。"
        } else {
            "更新後の yt-dlp を実行できませんでした。更新を中止しました。"
        };
        return Err(message.into());
    }
    let _ = fs::remove_file(&backup);

    let message = format!("yt-dlp を {version} に更新しました");
    job.progress(100.0, "完了", message.clone());

    Ok(ToolUpdate {
        info: ToolInfo {
            name: "yt-dlp".to_string(),
            label: "yt-dlp".to_string(),
            purpose: "YouTube などの動画・音声の取得".to_string(),
            available: true,
            path: Some(target.display().to_string()),
            version: Some(version),
            bundled: false,
        },
        message,
    })
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_urls() {
        assert_eq!(
            validate_url("  https://www.youtube.com/watch?v=abc  ").unwrap(),
            "https://www.youtube.com/watch?v=abc"
        );
        assert!(validate_url("http://example.com/a.mp4").is_ok());
        assert!(validate_url("").is_err());
        assert!(validate_url("ftp://example.com/x").is_err());
        assert!(validate_url("youtube.com/watch?v=abc").is_err());
        assert!(validate_url("https://example.com/a b").is_err());
    }

    #[test]
    fn builds_video_format_selectors() {
        let (selector, sort) = video_format("best").unwrap();
        assert_eq!(selector, "bv*+ba/b");
        assert_eq!(sort, "res,fps,vcodec:h264,acodec:m4a");

        let (selector, sort) = video_format("1080").unwrap();
        assert_eq!(selector, "bv*[height<=1080]+ba/b[height<=1080]/b");
        assert_eq!(sort, "res:1080,fps,vcodec:h264,acodec:m4a");

        assert!(video_format("1081").is_err());
        assert!(video_format("4k").is_err());
    }

    #[test]
    fn builds_download_arguments() {
        let req = DownloadRequest {
            url: "https://example.com/v".into(),
            output_dir: "/tmp/out".into(),
            mode: "video".into(),
            quality: "720".into(),
            audio_format: None,
            embed_thumbnail: true,
            embed_metadata: true,
            cookies_browser: Some("Firefox".into()),
            compression: None,
        };
        let plan = plan_download(&req).unwrap();
        let args = build_download_args(
            &plan,
            &req,
            Path::new("/tmp/out"),
            Path::new("/opt/ffmpeg"),
            "https://example.com/v",
        );
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--paths" && w[1] == "/tmp/out"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-f" && w[1] == "bv*[height<=720]+ba/b[height<=720]/b"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-S" && w[1] == "res:720,fps,vcodec:h264,acodec:m4a"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--merge-output-format" && w[1] == "mp4"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--cookies-from-browser" && w[1] == "firefox"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--ffmpeg-location" && w[1] == "/opt/ffmpeg"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--print" && w[1] == "after_move:filepath"));
        assert!(args.contains(&"--embed-thumbnail".to_string()));
        assert!(args.contains(&"--embed-metadata".to_string()));
        assert_eq!(
            args.last().map(String::as_str),
            Some("https://example.com/v")
        );
    }

    #[test]
    fn builds_audio_download_arguments() {
        let req = DownloadRequest {
            url: "https://example.com/a".into(),
            output_dir: "/tmp/out".into(),
            mode: "audio".into(),
            quality: "best".into(),
            audio_format: Some("m4a".into()),
            embed_thumbnail: false,
            embed_metadata: false,
            cookies_browser: None,
            compression: None,
        };
        let plan = plan_download(&req).unwrap();
        let args = build_download_args(
            &plan,
            &req,
            Path::new("/tmp/out"),
            Path::new("/opt/ffmpeg"),
            "https://example.com/a",
        );
        assert!(args.contains(&"-x".to_string()));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--audio-format" && w[1] == "m4a"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--audio-quality" && w[1] == "0"));
        assert!(!args.contains(&"--merge-output-format".to_string()));
        assert!(!args.contains(&"--embed-thumbnail".to_string()));
    }

    #[test]
    fn rejects_invalid_mode() {
        let mut req = DownloadRequest {
            url: "https://example.com/v".into(),
            output_dir: "/tmp/out".into(),
            mode: "video".into(),
            quality: "best".into(),
            audio_format: None,
            embed_thumbnail: false,
            embed_metadata: false,
            cookies_browser: None,
            compression: None,
        };
        req.mode = "subtitles".into();
        assert!(plan_download(&req).is_err());
    }

    #[test]
    fn validates_audio_format_and_cookies() {
        assert_eq!(audio_format(None).unwrap(), "mp3");
        assert_eq!(audio_format(Some("")).unwrap(), "mp3");
        assert_eq!(audio_format(Some("M4A")).unwrap(), "m4a");
        assert_eq!(audio_format(Some("opus")).unwrap(), "opus");
        assert!(audio_format(Some("wav")).is_err());

        assert_eq!(normalize_cookies(None).unwrap(), None);
        assert_eq!(
            normalize_cookies(Some(" Chrome ")).unwrap().as_deref(),
            Some("chrome")
        );
        assert!(normalize_cookies(Some("netscape")).is_err());
        assert!(normalize_cookies(Some("firefox ")).is_ok());
    }

    #[test]
    fn parses_download_progress_lines() {
        let p = parse_dl_line("DL|500|1000|NA|1048576|30|NA|NA").unwrap();
        assert_eq!(p.percent, Some(50.0));
        assert_eq!(p.speed.as_deref(), Some("1.0 MB/s"));
        assert_eq!(p.eta.as_deref(), Some("00:30"));

        // Falls back to the estimated total.
        let p = parse_dl_line("DL|250|NA|1000|NA|NA|NA|NA").unwrap();
        assert_eq!(p.percent, Some(25.0));
        assert_eq!(p.speed, None);
        assert_eq!(p.eta, None);

        // Falls back to fragment counts.
        let p = parse_dl_line("DL|NA|NA|NA|NA|NA|3|10").unwrap();
        assert_eq!(p.percent, Some(30.0));

        // Nothing usable, but the line is still recognised.
        let p = parse_dl_line("DL|NA|NA|NA|NA|NA|NA|NA").unwrap();
        assert_eq!(p.percent, None);

        assert!(parse_dl_line("[download] 10% of 1MB").is_none());
        assert!(parse_dl_line("DL|1|2").is_none());
    }

    #[test]
    fn formats_speeds_and_etas() {
        assert_eq!(format_speed(1048576.0), "1.0 MB/s");
        assert_eq!(format_speed(2048.0), "2.0 KB/s");
        assert_eq!(format_speed(512.0), "512 B/s");
        assert_eq!(format_speed(0.0), "—");
        assert_eq!(format_eta(0), "00:00");
        assert_eq!(format_eta(90), "01:30");
        assert_eq!(format_eta(3725), "1:02:05");
    }

    #[test]
    fn detects_postprocessing_lines() {
        assert_eq!(
            postprocess_message("[ExtractAudio] Destination: x.mp3"),
            Some("音声を抽出しています…")
        );
        assert_eq!(
            postprocess_message("[Merger] Merging formats into \"x.mp4\""),
            Some("映像と音声を結合しています…")
        );
        assert_eq!(
            postprocess_message("[Metadata] Adding metadata"),
            Some("メタデータを書き込んでいます…")
        );
        assert_eq!(postprocess_message("[download] Destination: x"), None);
    }

    #[test]
    fn parses_probe_json() {
        let json = r#"{
            "id": "abc123",
            "title": "テスト動画",
            "uploader": "チャンネル",
            "duration": 125,
            "thumbnail": "https://i.ytimg.com/vi/abc123/hqdefault.jpg",
            "webpage_url": "https://www.youtube.com/watch?v=abc123",
            "is_live": false,
            "view_count": 12345,
            "description": "説明"
        }"#;
        let info = parse_yt_json(json, "https://example.com").unwrap();
        assert_eq!(info.id, "abc123");
        assert_eq!(info.title, "テスト動画");
        assert_eq!(info.uploader.as_deref(), Some("チャンネル"));
        assert_eq!(info.duration_sec, 125.0);
        assert_eq!(info.view_count, Some(12345));
        assert!(!info.live);
        assert_eq!(info.webpage_url, "https://www.youtube.com/watch?v=abc123");
    }

    #[test]
    fn parses_playlist_wrapper() {
        let json = r#"{
            "_type": "playlist",
            "entries": [
                {"id": "v1", "title": "One", "webpage_url": "https://youtu.be/v1"}
            ]
        }"#;
        let info = parse_yt_json(json, "https://youtube.com/playlist?list=x").unwrap();
        assert_eq!(info.id, "v1");
        assert_eq!(info.title, "One");
        assert_eq!(info.duration_sec, 0.0);
    }

    #[test]
    fn rejects_invalid_probe_json() {
        assert!(parse_yt_json("not json", "https://example.com").is_err());
        assert!(parse_yt_json("{}", "https://example.com").is_err());
        assert!(parse_yt_json(
            r#"{"_type":"playlist","entries":[]}"#,
            "https://example.com"
        )
        .is_err());
    }

    #[test]
    fn detects_error_hints() {
        assert!(error_hints("ERROR: Sign in to confirm your age").contains("年齢制限"));
        assert!(error_hints("ERROR: Private video").contains("非公開"));
        assert!(error_hints("ERROR: Video unavailable").contains("利用できません"));
        assert!(error_hints("ffmpeg not found").contains("FFmpeg"));
        assert!(error_hints("ERROR: Unsupported URL").contains("URL"));
        assert_eq!(error_hints("something else"), "");
        assert!(probe_hint("anything").contains("URL を確認"));
    }

    #[test]
    fn extracts_existing_final_path() {
        let dir = std::env::temp_dir();
        let file = dir.join(format!("utilbox-test-{}.mp4", uuid::Uuid::new_v4()));
        fs::write(&file, b"x").unwrap();
        let text = format!(
            "DL|1|1|NA|NA|NA|NA|NA\n[download] Destination: foo.mp4\n{}\n",
            file.display()
        );
        let found = existing_path_from(&text).unwrap();
        assert_eq!(found, file);
        let _ = fs::remove_file(&file);

        assert!(existing_path_from("[download] Destination: nope.mp4\n").is_none());
    }

    #[test]
    fn ignores_temporary_files_when_scanning() {
        assert!(is_temp_artifact(Path::new("video.mp4.part")));
        assert!(is_temp_artifact(Path::new("video.mp4.ytdl")));
        assert!(is_temp_artifact(Path::new("thumb.webp")));
        assert!(!is_temp_artifact(Path::new("video.mp4")));
        assert!(is_media_file(Path::new("video.MP4")));
        assert!(is_media_file(Path::new("audio.mp3")));
        assert!(!is_media_file(Path::new("video.txt")));
    }

    #[test]
    fn compressed_output_path_is_unique() {
        let dir = std::env::temp_dir().join(format!("utilbox-out-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let input = dir.join("clip.webm");
        fs::write(&input, b"x").unwrap();

        let first = compressed_output_path(&input);
        assert_eq!(first.file_name().unwrap(), "clip_compressed.mp4");
        assert_eq!(first.parent(), Some(dir.as_path()));

        fs::write(&first, b"x").unwrap();
        let second = compressed_output_path(&input);
        assert_eq!(second.file_name().unwrap(), "clip_compressed-2.mp4");

        let _ = fs::remove_dir_all(&dir);
    }
}
