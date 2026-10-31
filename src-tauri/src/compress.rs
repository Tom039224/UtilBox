// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026 Kirsikka (GitHub: Tom039224)

//! MP4 compression with FFmpeg.
//!
//! Two modes are supported:
//! * `crf`  — constant quality, single pass
//! * `size` — target file size, two pass
//!
//! Everything is executed through [`crate::process`] so cancellation and
//! progress reporting behave the same as the rest of the app.

use crate::job::Job;
use crate::process::{parse_timecode, run_capture, run_streaming, Cmd, Stream};
use crate::tools::Tools;
use crate::types::*;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

// ---------------------------------------------------------------------------
// probe
// ---------------------------------------------------------------------------

/// Reads media metadata with ffprobe. Any container is accepted; the caller
/// decides whether a video stream is required.
pub fn probe_media(tools: &Tools, path: &str) -> Result<MediaInfo, String> {
    if path.trim().is_empty() {
        return Err("ファイルパスが指定されていません".into());
    }
    let input = Path::new(path);
    if !input.is_file() {
        return Err(format!("ファイルが見つかりません: {path}"));
    }
    let ffprobe = tools.require("ffprobe")?;
    let cmd = Cmd::new(ffprobe.as_path()).args([
        "-v",
        "error",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
        path,
    ]);
    let outcome = run_capture(&cmd)?;
    if !outcome.success() {
        return Err(format!(
            "メディア情報を取得できませんでした ({}): {}",
            display_name(input),
            truncate(&outcome.error_text(), 400)
        ));
    }
    parse_media_json(path, &outcome.stdout)
}

fn parse_media_json(path: &str, raw: &str) -> Result<MediaInfo, String> {
    let root: Value = serde_json::from_str(raw)
        .map_err(|e| format!("ffprobe の出力を解析できませんでした: {e}"))?;

    let format = root.get("format").unwrap_or(&Value::Null);
    let streams: Vec<Value> = root
        .get("streams")
        .and_then(Value::as_array)
        .map(|a| a.to_vec())
        .unwrap_or_default();

    let video = streams
        .iter()
        .find(|s| stream_type(s) == Some("video") && !is_attached_pic(s));
    let audio = streams.iter().find(|s| stream_type(s) == Some("audio"));

    let duration_sec = value_num(format, "duration")
        .or_else(|| video.and_then(|s| value_num(s, "duration")))
        .or_else(|| {
            streams
                .iter()
                .filter_map(|s| value_num(s, "duration"))
                .find(|d| *d > 0.0)
        })
        .unwrap_or(0.0)
        .max(0.0);

    let size = value_num(format, "size")
        .map(|n| n.max(0.0) as u64)
        .filter(|n| *n > 0)
        .or_else(|| std::fs::metadata(path).ok().map(|m| m.len()))
        .unwrap_or(0);

    let bitrate_kbps = value_num(format, "bit_rate")
        .or_else(|| video.and_then(|s| value_num(s, "bit_rate")))
        .or_else(|| audio.and_then(|s| value_num(s, "bit_rate")))
        .filter(|n| *n > 0.0)
        .map(|n| (n / 1000.0).round() as u64);

    let fps = video
        .and_then(|s| value_str(s, "avg_frame_rate"))
        .and_then(|raw| parse_fps(&raw))
        .or_else(|| {
            video
                .and_then(|s| value_str(s, "r_frame_rate"))
                .and_then(|raw| parse_fps(&raw))
        });

    Ok(MediaInfo {
        path: path.to_string(),
        size,
        duration_sec,
        width: video
            .and_then(|s| value_u64(s, "width"))
            .map(|n| n.min(u32::MAX as u64) as u32),
        height: video
            .and_then(|s| value_u64(s, "height"))
            .map(|n| n.min(u32::MAX as u64) as u32),
        video_codec: video.and_then(|s| value_str(s, "codec_name")),
        audio_codec: audio.and_then(|s| value_str(s, "codec_name")),
        bitrate_kbps,
        fps,
        container: value_str(format, "format_name"),
    })
}

fn stream_type(stream: &Value) -> Option<&str> {
    stream.get("codec_type").and_then(Value::as_str)
}

fn is_attached_pic(stream: &Value) -> bool {
    stream
        .get("disposition")
        .and_then(|d| d.get("attached_pic"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
        == 1
}

fn value_str(value: &Value, key: &str) -> Option<String> {
    let text = value.get(key)?.as_str()?.trim();
    if text.is_empty() || text.eq_ignore_ascii_case("n/a") || text == "unknown" {
        None
    } else {
        Some(text.to_string())
    }
}

fn value_num(value: &Value, key: &str) -> Option<f64> {
    let raw = value.get(key)?;
    if let Some(n) = raw.as_f64() {
        return n.is_finite().then_some(n);
    }
    let text = raw.as_str()?.trim();
    if text.is_empty() || text.eq_ignore_ascii_case("n/a") {
        return None;
    }
    text.parse::<f64>().ok().filter(|n| n.is_finite())
}

fn value_u64(value: &Value, key: &str) -> Option<u64> {
    let raw = value.get(key)?;
    raw.as_u64()
        .or_else(|| raw.as_f64().map(|n| n.max(0.0) as u64))
}

/// Parses `avg_frame_rate`-style values such as `30000/1001`.
fn parse_fps(raw: &str) -> Option<f64> {
    let raw = raw.trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("n/a") || raw == "0/0" {
        return None;
    }
    let fps = if let Some((num, den)) = raw.split_once('/') {
        let num: f64 = num.trim().parse().ok()?;
        let den: f64 = den.trim().parse().ok()?;
        if den == 0.0 || num <= 0.0 {
            return None;
        }
        num / den
    } else {
        raw.parse::<f64>().ok()?
    };
    (fps.is_finite() && fps > 0.0 && fps < 1000.0).then_some(fps)
}

// ---------------------------------------------------------------------------
// compression
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Codec {
    H264,
    H265,
    Av1,
    /// AV1 on the GPU (NVENC). Much faster than the CPU encoders, but files
    /// are noticeably larger at the same quality.
    Av1Gpu,
}

impl Codec {
    /// Fallback encoder used when the encoder list cannot be queried.
    /// AV1 prefers libaom here because it is present in the bundled FFmpeg;
    /// `ensure_encoder` upgrades to SVT-AV1 when the build provides it.
    fn default_encoder(self) -> &'static str {
        match self {
            Codec::H264 => "libx264",
            Codec::H265 => "libx265",
            Codec::Av1 => "libaom-av1",
            Codec::Av1Gpu => "av1_nvenc",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Codec::H264 => "H.264",
            Codec::H265 => "H.265",
            Codec::Av1 => "AV1",
            Codec::Av1Gpu => "AV1 (GPU)",
        }
    }

    fn default_crf(self) -> u32 {
        match self {
            Codec::H264 => 24,
            Codec::H265 => 28,
            Codec::Av1 => 32,
            // NVENC's -cq is not on the same scale as CRF; ~38 gives a
            // "standard" quality (VMAF ≈ 95 on a 1080p60 sample).
            Codec::Av1Gpu => 38,
        }
    }

    fn max_crf(self) -> u32 {
        match self {
            // AV1 uses the full 0-63 CRF scale.
            Codec::Av1 | Codec::Av1Gpu => 63,
            _ => 51,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RateMode {
    Crf,
    Size,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AudioMode {
    Encode,
    Copy,
}

struct Options {
    codec: Codec,
    /// Resolved at runtime: libsvtav1 / libaom-av1 for AV1, libx264/5 otherwise.
    encoder: String,
    mode: RateMode,
    crf: u32,
    target_mb: f64,
    max_height: Option<u32>,
    preset: String,
    audio_kbps: u32,
    audio_mode: AudioMode,
    remove_audio: bool,
    has_audio: bool,
    /// Target video bitrate for single-pass VBR (GPU encoder in size mode).
    video_kbps: Option<u64>,
}

const PRESETS: &[&str] = &[
    "ultrafast",
    "superfast",
    "veryfast",
    "faster",
    "fast",
    "medium",
    "slow",
    "slower",
    "veryslow",
];

const MAX_HEIGHTS: &[u32] = &[360, 480, 720, 1080, 1440, 2160];

fn validate_options(req: &CompressRequest, has_audio: bool) -> Result<Options, String> {
    let codec = match req.codec.trim().to_ascii_lowercase().as_str() {
        "h264" => Codec::H264,
        "h265" | "hevc" => Codec::H265,
        "av1" | "av01" => Codec::Av1,
        "av1_gpu" | "av1-gpu" => Codec::Av1Gpu,
        other => {
            return Err(format!(
                "コーデックは h264 / h265 / av1 / av1_gpu を指定してください（指定: {other}）"
            ))
        }
    };

    let mode = match req.mode.trim().to_ascii_lowercase().as_str() {
        "crf" => RateMode::Crf,
        "size" => RateMode::Size,
        other => {
            return Err(format!(
                "圧縮モードは crf または size を指定してください（指定: {other}）"
            ))
        }
    };

    let crf = req
        .crf
        .unwrap_or_else(|| codec.default_crf())
        .min(codec.max_crf());

    let target_mb = req.target_mb.unwrap_or(100.0);
    if mode == RateMode::Size && (!target_mb.is_finite() || target_mb <= 0.0) {
        return Err("目標サイズは 0 より大きい MB で指定してください".into());
    }

    let max_height = match req.max_height.unwrap_or(0) {
        0 => None,
        h if MAX_HEIGHTS.contains(&h) => Some(h),
        h => {
            return Err(format!(
                "解像度は 360 / 480 / 720 / 1080 / 1440 / 2160 のいずれかで指定してください（指定: {h}）"
            ))
        }
    };

    let preset = match req.preset.as_deref().map(str::trim).unwrap_or("") {
        "" => "medium".to_string(),
        raw => {
            let lower = raw.to_ascii_lowercase();
            if PRESETS.contains(&lower.as_str()) {
                lower
            } else {
                return Err(format!(
                    "プリセット「{raw}」には対応していません。{} のいずれかを指定してください。",
                    PRESETS.join(" / ")
                ));
            }
        }
    };

    let audio_mode = match req
        .audio_mode
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "" | "encode" => AudioMode::Encode,
        "copy" => AudioMode::Copy,
        other => {
            return Err(format!(
                "音声モードは encode または copy を指定してください（指定: {other}）"
            ))
        }
    };

    Ok(Options {
        codec,
        encoder: codec.default_encoder().to_string(),
        mode,
        crf,
        target_mb,
        max_height,
        preset,
        audio_kbps: req.audio_kbps.unwrap_or(128).clamp(32, 320),
        audio_mode,
        remove_audio: req.remove_audio,
        has_audio,
        video_kbps: None,
    })
}

/// Two-pass video bitrate in kbps. 16 kbps is reserved for muxing overhead.
fn two_pass_video_kbps(
    target_mb: f64,
    duration_sec: f64,
    audio_kbps: u32,
    remove_audio: bool,
) -> u64 {
    let duration = duration_sec.max(0.1);
    let total_kbps = (target_mb * 8192.0) / duration;
    let audio = if remove_audio { 0.0 } else { audio_kbps as f64 };
    (total_kbps - audio - 16.0).max(80.0).round() as u64
}

fn target_video_kbps(opts: &Options, duration_sec: f64) -> u64 {
    let no_audio = opts.remove_audio || !opts.has_audio;
    two_pass_video_kbps(
        opts.target_mb,
        duration_sec,
        if no_audio { 0 } else { opts.audio_kbps },
        no_audio,
    )
}

fn scale_filter(max_height: Option<u32>, source_height: Option<u32>) -> Option<String> {
    match (max_height, source_height) {
        (Some(target), Some(source)) if source > target => {
            // `force_divisible_by=2` keeps yuv420p happy even when the source
            // has odd dimensions.
            Some(format!(
                "scale=-2:'min(ih,{target})':flags=lanczos:force_divisible_by=2"
            ))
        }
        _ => None,
    }
}

/// FFmpeg builds differ in which encoders they ship. Cache the encoder list for
/// the lifetime of the process and fail with an actionable message up front.
static ENCODERS: OnceLock<Vec<String>> = OnceLock::new();

fn available_encoders(ffmpeg: &Path) -> &'static [String] {
    ENCODERS.get_or_init(|| {
        let cmd = Cmd::new(ffmpeg).args(["-hide_banner", "-encoders"]);
        match run_capture(&cmd) {
            Ok(out) if out.success() => parse_encoder_list(&out.stdout),
            _ => Vec::new(),
        }
    })
}

fn parse_encoder_list(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let flags = parts.next()?;
            if flags.len() < 6 || flags.starts_with('-') {
                return None;
            }
            if !flags.chars().any(|c| matches!(c, 'V' | 'A' | 'S' | 'D')) {
                return None;
            }
            parts.next().map(ToString::to_string)
        })
        .collect()
}

/// True when this FFmpeg lists `av1_nvenc` *and* a short test encode succeeds,
/// i.e. a supported GPU and driver are actually present.
pub fn av1_nvenc_usable(ffmpeg: &Path) -> bool {
    static USABLE: OnceLock<bool> = OnceLock::new();
    *USABLE.get_or_init(|| {
        if !available_encoders(ffmpeg).iter().any(|e| e == "av1_nvenc") {
            return false;
        }
        let cmd = Cmd::new(ffmpeg).args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=640x360:d=0.2",
            "-c:v",
            "av1_nvenc",
            "-f",
            "null",
            "-",
        ]);
        matches!(run_capture(&cmd), Ok(out) if out.success())
    })
}

fn ensure_encoder(ffmpeg: &Path, codec: Codec) -> Result<&'static str, String> {
    let list = available_encoders(ffmpeg);
    let has = |name: &str| list.iter().any(|entry| entry == name);

    // An empty list means `ffmpeg -encoders` failed; let FFmpeg itself report
    // the real problem instead of blocking a working encoder.
    if list.is_empty() {
        return Ok(codec.default_encoder());
    }

    match codec {
        Codec::Av1Gpu => {
            if has("av1_nvenc") && av1_nvenc_usable(ffmpeg) {
                Ok("av1_nvenc")
            } else {
                Err(
                    "GPU の AV1 エンコード（av1_nvenc）を利用できません。RTX 40 以降の NVIDIA GPU と対応ドライバー、NVENC 対応の FFmpeg が必要です。コーデックを AV1 に変更してください。"
                        .into(),
                )
            }
        }
        Codec::H264 => {
            if has("libx264") {
                Ok("libx264")
            } else {
                Err(
                    "この FFmpeg には H.264 エンコーダー（libx264）が含まれていません。別の FFmpeg を用意してください。"
                        .into(),
                )
            }
        }
        Codec::H265 => {
            if has("libx265") {
                Ok("libx265")
            } else {
                Err(
                    "この FFmpeg には H.265 エンコーダー（libx265）が含まれていません。コーデックを H.264 または AV1 に変更してください。"
                        .into(),
                )
            }
        }
        Codec::Av1 => {
            if has("libsvtav1") {
                Ok("libsvtav1")
            } else if has("libaom-av1") {
                Ok("libaom-av1")
            } else {
                Err(
                    "この FFmpeg には AV1 エンコーダー（libsvtav1 / libaom-av1）が含まれていません。コーデックを H.264 / H.265 に変更するか、AV1 対応の FFmpeg を用意してください。"
                        .into(),
                )
            }
        }
    }
}

/// x264-style preset names mapped to SVT-AV1's 0 (slowest) – 13 (fastest).
fn svt_preset(preset: &str) -> &'static str {
    match preset {
        "ultrafast" => "12",
        "superfast" => "11",
        "veryfast" => "10",
        "faster" => "9",
        "fast" => "8",
        "medium" => "6",
        "slow" => "4",
        "slower" => "2",
        "veryslow" => "0",
        _ => "6",
    }
}

/// x264-style preset names mapped to libaom's `-cpu-used` (0 slowest – 8).
fn aom_cpu_used(preset: &str) -> &'static str {
    match preset {
        "ultrafast" => "8",
        "superfast" => "8",
        "veryfast" => "7",
        "faster" => "6",
        "fast" => "6",
        "medium" => "5",
        "slow" => "4",
        "slower" => "2",
        "veryslow" => "0",
        _ => "5",
    }
}

struct EncodeProgress<'a> {
    job: &'a Job,
    duration: f64,
    from: f64,
    to: f64,
    speed: Option<f64>,
    speed_text: Option<String>,
}

impl<'a> EncodeProgress<'a> {
    fn new(job: &'a Job, duration: f64, from: f64, to: f64) -> Self {
        Self {
            job,
            duration,
            from,
            to,
            speed: None,
            speed_text: None,
        }
    }

    fn feed(&mut self, line: &str) {
        let line = line.trim();
        if let Some(raw) = line.strip_prefix("speed=") {
            let raw = raw.trim();
            if !raw.is_empty() && !raw.eq_ignore_ascii_case("n/a") {
                self.speed_text = Some(raw.to_string());
                self.speed = raw
                    .trim_end_matches('x')
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|s| s.is_finite() && *s > 0.0);
            }
            return;
        }

        let seconds = if let Some(raw) = line.strip_prefix("out_time_us=") {
            parse_micros(raw)
        } else if let Some(raw) = line.strip_prefix("out_time_ms=") {
            parse_micros(raw)
        } else if let Some(raw) = line.strip_prefix("out_time=") {
            parse_timecode(raw)
        } else {
            None
        };

        let Some(seconds) = seconds else { return };
        let ratio = if self.duration > 0.0 {
            (seconds / self.duration).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let percent = (self.from + ratio * (self.to - self.from)).clamp(0.0, 100.0);
        let eta = self.eta(seconds);
        let eta_text = match eta {
            Some(text) => format!("残り {text}"),
            None => "残り —".to_string(),
        };
        let detail = format!(
            "{:.0}% · {} · {}",
            self.job.map_percent(percent),
            self.speed_text.as_deref().unwrap_or("—"),
            eta_text
        );
        self.job
            .progress_detail(percent, "エンコード", "MP4 を圧縮中…", Some(detail));
    }

    fn eta(&self, seconds: f64) -> Option<String> {
        let speed = self.speed?;
        let remaining = ((self.duration - seconds) / speed).max(0.0);
        Some(format_eta(remaining.round() as u64))
    }
}

fn parse_micros(raw: &str) -> Option<f64> {
    let value = raw.trim().parse::<f64>().ok()?;
    (value.is_finite() && value >= 0.0).then_some(value / 1_000_000.0)
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

fn base_encode_args(opts: &Options, input: &Path, video_filter: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostats",
        "-progress",
        "pipe:1",
        "-i",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.push(input.display().to_string());
    args.extend(["-map", "0:v:0", "-map", "0:a:0?"].map(String::from));
    if let Some(filter) = video_filter {
        args.push("-vf".into());
        args.push(filter.to_string());
    }
    args.push("-c:v".into());
    args.push(opts.encoder.clone());

    match opts.codec {
        Codec::H264 => {
            args.push("-preset".into());
            args.push(opts.preset.clone());
        }
        Codec::H265 => {
            args.push("-preset".into());
            args.push(opts.preset.clone());
        }
        Codec::Av1Gpu => {
            // p5 is as small as p7 at ~25% less time (benchmarked).
            args.extend(
                [
                    "-preset",
                    "p5",
                    "-tune",
                    "hq",
                    "-rc",
                    "vbr",
                    "-multipass",
                    "fullres",
                    "-rc-lookahead",
                    "32",
                ]
                .map(String::from),
            );
        }
        Codec::Av1 => {
            if opts.encoder == "libsvtav1" {
                args.push("-preset".into());
                args.push(svt_preset(&opts.preset).into());
            } else {
                // libaom swaps a preset for `-cpu-used` and benefits from
                // multithreaded row encoding.
                args.push("-cpu-used".into());
                args.push(aom_cpu_used(&opts.preset).into());
                args.push("-row-mt".into());
                args.push("1".into());
            }
        }
    }

    args.push("-pix_fmt".into());
    args.push("yuv420p".into());
    match opts.codec {
        Codec::H265 => {
            args.push("-tag:v".into());
            args.push("hvc1".into());
        }
        Codec::Av1 | Codec::Av1Gpu => {
            args.push("-tag:v".into());
            args.push("av01".into());
        }
        Codec::H264 => {}
    }
    args
}

/// Quality-targeted arguments for a single-pass CRF encode.
fn quality_args(opts: &Options) -> Vec<String> {
    if opts.codec == Codec::Av1Gpu {
        // NVENC has no two-pass log files; size mode is a single VBR pass.
        return match (opts.mode, opts.video_kbps) {
            (RateMode::Size, Some(kbps)) => vec![
                "-b:v".to_string(),
                format!("{kbps}k"),
                "-maxrate".to_string(),
                format!("{}k", kbps + kbps / 2),
            ],
            _ => vec![
                "-cq".to_string(),
                opts.crf.to_string(),
                "-b:v".to_string(),
                "0".to_string(),
            ],
        };
    }
    let mut args = vec!["-crf".to_string(), opts.crf.to_string()];
    // libaom only enters constant-quality mode when the bitrate is zero.
    if opts.codec == Codec::Av1 && opts.encoder == "libaom-av1" {
        args.push("-b:v".into());
        args.push("0".into());
    }
    args
}

fn audio_args(opts: &Options) -> Vec<String> {
    if opts.remove_audio || !opts.has_audio {
        return vec!["-an".into()];
    }
    match opts.audio_mode {
        AudioMode::Copy => vec!["-c:a".into(), "copy".into()],
        AudioMode::Encode => vec![
            "-c:a".into(),
            "aac".into(),
            "-b:a".into(),
            format!("{}k", opts.audio_kbps),
        ],
    }
}

fn finish_args() -> Vec<String> {
    ["-movflags", "+faststart", "-map_metadata", "0"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

struct PassArtifacts {
    prefix: PathBuf,
    null_output: PathBuf,
}

impl Drop for PassArtifacts {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.null_output);
        remove_passlog(&self.prefix);
    }
}

fn remove_passlog(prefix: &Path) {
    let (Some(dir), Some(name)) = (prefix.parent(), prefix.file_name().and_then(|n| n.to_str()))
    else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if file_name.starts_with(name) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn sanitize_token(raw: &str) -> String {
    let mut out: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(32)
        .collect();
    if out.is_empty() {
        out.push_str("job");
    }
    out
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    comparable(a) == comparable(b)
}

fn comparable(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
        if let Ok(parent) = parent.canonicalize() {
            return parent.join(name);
        }
    }
    path.to_path_buf()
}

fn log_stderr(job: &Job, line: &str) {
    let text = line.trim();
    if text.is_empty() {
        return;
    }
    let lower = text.to_ascii_lowercase();
    if ["error", "invalid", "failed", "unable"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        job.log(truncate(text, 240));
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

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

fn cleanup_output(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// Compresses `req.input` to `req.output` with FFmpeg.
pub fn compress(
    job: &Job,
    tools: &Tools,
    req: &CompressRequest,
) -> Result<CompressResponse, String> {
    let started = Instant::now();
    job.check()?;

    if req.input.trim().is_empty() {
        return Err("入力ファイルが指定されていません".into());
    }
    if req.output.trim().is_empty() {
        return Err("出力ファイルが指定されていません".into());
    }

    let input = PathBuf::from(&req.input);
    if !input.exists() {
        return Err(format!("入力ファイルが見つかりません: {}", input.display()));
    }
    if !input.is_file() {
        return Err(format!(
            "入力パスがファイルではありません: {}",
            input.display()
        ));
    }

    job.check()?;
    job.progress(1.0, "解析", "入力ファイルを確認しています…");
    let info = probe_media(tools, &input.display().to_string())?;
    if info.video_codec.is_none() || info.duration_sec <= 0.0 {
        return Err("映像ストリームが見つかりません".into());
    }

    let opts = validate_options(req, info.audio_codec.is_some())?;
    let ffmpeg = tools.require("ffmpeg")?;
    let encoder = ensure_encoder(&ffmpeg, opts.codec)?;
    let mut opts = opts;
    opts.encoder = encoder.to_string();

    let mut output = PathBuf::from(&req.output);
    if output.extension().is_none() {
        output.set_extension("mp4");
    }
    if output.is_dir() {
        return Err(format!(
            "出力先がフォルダになっています: {}",
            output.display()
        ));
    }
    if paths_equal(&input, &output) {
        return Err("出力ファイルが入力ファイルと同じです。別の出力先を指定してください。".into());
    }
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("出力フォルダを作成できません ({}): {e}", parent.display()))?;
        }
    }

    let filter = scale_filter(opts.max_height, info.height);
    let size_before = std::fs::metadata(&input)
        .map(|m| m.len())
        .unwrap_or(info.size);
    if let Some(height) = opts.max_height {
        if filter.is_some() {
            job.log(format!("解像度を高さ {height}px 以下に縮小します"));
        }
    }
    job.log(format!(
        "エンコーダー: {} ({}) / モード: {}",
        opts.codec.label(),
        opts.encoder,
        match opts.mode {
            RateMode::Crf => format!("CRF {}", opts.crf),
            RateMode::Size => format!("目標 {:.1} MB（2パス）", opts.target_mb),
        }
    ));
    job.progress(5.0, "解析", "圧縮設定を準備しています…");

    if opts.mode == RateMode::Size && opts.codec == Codec::Av1Gpu {
        opts.video_kbps = Some(target_video_kbps(&opts, info.duration_sec));
    }

    match (opts.mode, opts.codec) {
        (RateMode::Crf, _) | (RateMode::Size, Codec::Av1Gpu) => {
            let mut args = base_encode_args(&opts, &input, filter.as_deref());
            args.extend(quality_args(&opts));
            args.extend(audio_args(&opts));
            args.extend(finish_args());
            args.push("-y".into());
            args.push(output.display().to_string());

            let cmd = Cmd::new(ffmpeg.as_path()).args(args);
            job.log(format!("実行: {}", cmd.display()));
            job.progress(8.0, "エンコード", "MP4 を圧縮中…");
            let mut progress = EncodeProgress::new(job, info.duration_sec, 0.0, 98.0);
            let outcome = match run_streaming(job, &cmd, |stream, line| match stream {
                Stream::Out => progress.feed(line),
                Stream::Err => log_stderr(job, line),
            }) {
                Ok(outcome) => outcome,
                Err(e) => {
                    cleanup_output(&output);
                    return Err(e);
                }
            };

            if outcome.cancelled || job.is_cancelled() {
                cleanup_output(&output);
                return Err("キャンセルされました".into());
            }
            if !outcome.success() {
                cleanup_output(&output);
                return Err(format!(
                    "圧縮に失敗しました: {}",
                    truncate(&outcome.error_text(), 800)
                ));
            }
        }
        (RateMode::Size, _) => {
            let audio_kbps = if opts.remove_audio || !opts.has_audio {
                0
            } else {
                opts.audio_kbps
            };
            let video_kbps = target_video_kbps(&opts, info.duration_sec);
            job.log(format!(
                "目標ビットレート: 映像 {video_kbps} kbps / 音声 {audio_kbps} kbps"
            ));

            let token = format!(
                "utilbox-{}-{}",
                sanitize_token(&job.id),
                uuid::Uuid::new_v4()
            );
            let temp_dir = std::env::temp_dir();
            let prefix = temp_dir.join(format!("{token}-pass"));
            let null_output = temp_dir.join(format!("{token}-pass1.mp4"));
            let artifacts = PassArtifacts {
                prefix: prefix.clone(),
                null_output: null_output.clone(),
            };

            // ---- pass 1 -------------------------------------------------
            let mut args = base_encode_args(&opts, &input, filter.as_deref());
            args.extend([
                "-b:v".to_string(),
                format!("{video_kbps}k"),
                "-pass".to_string(),
                "1".to_string(),
                "-passlogfile".to_string(),
                prefix.display().to_string(),
                "-an".to_string(),
                "-f".to_string(),
                "mp4".to_string(),
                "-y".to_string(),
                null_output.display().to_string(),
            ]);
            let cmd = Cmd::new(ffmpeg.as_path()).args(args);
            job.log(format!("パス 1/2: {}", cmd.display()));
            job.progress(8.0, "エンコード", "2パス（1/2）解析中…");
            let mut progress = EncodeProgress::new(job, info.duration_sec, 0.0, 50.0);
            let pass1 = match run_streaming(job, &cmd, |stream, line| match stream {
                Stream::Out => progress.feed(line),
                Stream::Err => log_stderr(job, line),
            }) {
                Ok(outcome) => outcome,
                Err(e) => {
                    drop(artifacts);
                    cleanup_output(&output);
                    return Err(e);
                }
            };
            if pass1.cancelled || job.is_cancelled() {
                drop(artifacts);
                cleanup_output(&output);
                return Err("キャンセルされました".into());
            }
            if !pass1.success() {
                drop(artifacts);
                cleanup_output(&output);
                return Err(format!(
                    "2パス（1/2）に失敗しました: {}",
                    truncate(&pass1.error_text(), 800)
                ));
            }

            // ---- pass 2 -------------------------------------------------
            let mut args = base_encode_args(&opts, &input, filter.as_deref());
            args.extend([
                "-b:v".to_string(),
                format!("{video_kbps}k"),
                "-pass".to_string(),
                "2".to_string(),
                "-passlogfile".to_string(),
                prefix.display().to_string(),
            ]);
            args.extend(audio_args(&opts));
            args.extend(finish_args());
            args.push("-y".into());
            args.push(output.display().to_string());
            let cmd = Cmd::new(ffmpeg.as_path()).args(args);
            job.log(format!("パス 2/2: {}", cmd.display()));
            job.progress(50.0, "エンコード", "2パス（2/2）エンコード中…");
            let mut progress = EncodeProgress::new(job, info.duration_sec, 50.0, 98.0);
            let pass2 = match run_streaming(job, &cmd, |stream, line| match stream {
                Stream::Out => progress.feed(line),
                Stream::Err => log_stderr(job, line),
            }) {
                Ok(outcome) => outcome,
                Err(e) => {
                    drop(artifacts);
                    cleanup_output(&output);
                    return Err(e);
                }
            };
            drop(artifacts);
            if pass2.cancelled || job.is_cancelled() {
                cleanup_output(&output);
                return Err("キャンセルされました".into());
            }
            if !pass2.success() {
                cleanup_output(&output);
                return Err(format!(
                    "2パス（2/2）に失敗しました: {}",
                    truncate(&pass2.error_text(), 800)
                ));
            }
        }
    }

    if let Err(e) = job.check() {
        cleanup_output(&output);
        return Err(e);
    }
    job.progress(98.0, "エンコード", "エンコードが完了しました");
    job.progress(99.0, "解析", "出力を確認しています…");

    let size_after = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
    if size_after == 0 {
        cleanup_output(&output);
        return Err("出力ファイルが作成されませんでした".into());
    }
    let media = probe_media(tools, &output.display().to_string()).ok();
    let elapsed_ms = started.elapsed().as_millis() as u64;

    job.progress(
        100.0,
        "完了",
        format!(
            "圧縮が完了しました（{:.1} MB → {:.1} MB）",
            bytes_to_mb(size_before),
            bytes_to_mb(size_after)
        ),
    );

    Ok(CompressResponse {
        output: output.display().to_string(),
        size_before,
        size_after,
        elapsed_ms,
        media,
        encoder: format!("{} ({})", opts.codec.label(), opts.encoder),
    })
}

fn bytes_to_mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

/// Same as [`compress`], but reports progress inside `from`–`to` of the
/// current job. Used when compression follows another step (download) under a
/// single progress bar.
pub fn compress_with_range(
    job: &Job,
    tools: &Tools,
    req: &CompressRequest,
    from: f64,
    to: f64,
) -> Result<CompressResponse, String> {
    compress(&job.scaled(from, to), tools, req)
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn options_for(req: &CompressRequest) -> Result<Options, String> {
        validate_options(req, true)
    }

    fn req(mode: &str, codec: &str) -> CompressRequest {
        CompressRequest {
            input: "in.mp4".into(),
            output: "out.mp4".into(),
            mode: mode.into(),
            crf: None,
            target_mb: None,
            max_height: None,
            codec: codec.into(),
            audio_kbps: None,
            remove_audio: false,
            preset: None,
            audio_mode: None,
        }
    }

    #[test]
    fn builds_gpu_av1_arguments() {
        let mut r = req("crf", "av1_gpu");
        r.crf = Some(40);
        let opts = options_for(&r).unwrap();
        assert_eq!(opts.codec, Codec::Av1Gpu);
        assert_eq!(opts.encoder, "av1_nvenc");
        let args = quality_args(&opts);
        assert_eq!(args, ["-cq", "40", "-b:v", "0"]);
        let base = base_encode_args(&opts, Path::new("in.mp4"), None);
        assert!(base.windows(2).any(|w| w[0] == "-preset" && w[1] == "p5"));
        assert!(base.windows(2).any(|w| w[0] == "-tag:v" && w[1] == "av01"));

        // Size mode is one VBR pass, not the two-pass log-file flow.
        let mut size = options_for(&req("size", "av1_gpu")).unwrap();
        size.video_kbps = Some(1000);
        assert_eq!(quality_args(&size), ["-b:v", "1000k", "-maxrate", "1500k"]);
    }

    #[test]
    fn parses_fractional_frame_rate() {
        let fps = parse_fps("30000/1001").unwrap();
        assert!((fps - 29.970029).abs() < 0.001);
        assert_eq!(parse_fps("25/1"), Some(25.0));
        assert_eq!(parse_fps("24"), Some(24.0));
        assert_eq!(parse_fps("0/0"), None);
        assert_eq!(parse_fps("N/A"), None);
        assert_eq!(parse_fps("bad"), None);
    }

    #[test]
    fn parses_media_json() {
        let json = r#"{
            "streams": [
                {"codec_type":"video","codec_name":"h264","width":1920,"height":1080,
                 "avg_frame_rate":"30000/1001","r_frame_rate":"30/1","duration":"9.5"},
                {"codec_type":"audio","codec_name":"aac"}
            ],
            "format": {"size":"1234567","duration":"10.0","bit_rate":"5000123",
                       "format_name":"mov,mp4,m4a,3gp,3g2,mj2"}
        }"#;
        let info = parse_media_json("/tmp/x.mp4", json).unwrap();
        assert_eq!(info.size, 1_234_567);
        assert_eq!(info.duration_sec, 10.0);
        assert_eq!(info.width, Some(1920));
        assert_eq!(info.height, Some(1080));
        assert_eq!(info.video_codec.as_deref(), Some("h264"));
        assert_eq!(info.audio_codec.as_deref(), Some("aac"));
        assert_eq!(info.bitrate_kbps, Some(5000));
        assert!((info.fps.unwrap() - 29.97).abs() < 0.01);
        assert_eq!(info.container.as_deref(), Some("mov,mp4,m4a,3gp,3g2,mj2"));
    }

    #[test]
    fn skips_attached_picture_and_falls_back_to_stream_duration() {
        let json = r#"{
            "streams": [
                {"codec_type":"video","codec_name":"mjpeg","width":600,"height":600,
                 "disposition":{"attached_pic":1}},
                {"codec_type":"video","codec_name":"hevc","width":1280,"height":720,
                 "avg_frame_rate":"0/0","r_frame_rate":"50/1","duration":"12.5"},
                {"codec_type":"audio","codec_name":"opus"}
            ],
            "format": {"duration":"N/A","format_name":"matroska,webm"}
        }"#;
        let info = parse_media_json("x.mkv", json).unwrap();
        assert_eq!(info.width, Some(1280));
        assert_eq!(info.height, Some(720));
        assert_eq!(info.video_codec.as_deref(), Some("hevc"));
        assert_eq!(info.fps, Some(50.0));
        assert_eq!(info.duration_sec, 12.5);
    }

    #[test]
    fn two_pass_bitrate_math() {
        // 100 MB over 600 s = 1365.33 kbps; minus 128 audio and 16 overhead.
        assert_eq!(two_pass_video_kbps(100.0, 600.0, 128, false), 1221);
        // Audio removed, overhead only.
        assert_eq!(two_pass_video_kbps(100.0, 600.0, 128, true), 1349);
        // Never below the 80 kbps floor.
        assert_eq!(two_pass_video_kbps(1.0, 3600.0, 128, false), 80);
    }

    #[test]
    fn option_defaults_and_clamps() {
        let opts = options_for(&req("crf", "H264")).unwrap();
        assert_eq!(opts.crf, 24);
        assert_eq!(opts.preset, "medium");
        assert_eq!(opts.audio_kbps, 128);
        assert_eq!(opts.audio_mode, AudioMode::Encode);
        assert_eq!(opts.max_height, None);

        let opts = options_for(&req("crf", "h265")).unwrap();
        assert_eq!(opts.crf, 28);
        assert_eq!(opts.codec, Codec::H265);

        let opts = options_for(&req("crf", "av1")).unwrap();
        assert_eq!(opts.crf, 32);
        assert_eq!(opts.codec, Codec::Av1);
        assert_eq!(opts.encoder, "libaom-av1");

        let mut r = req("crf", "av1");
        r.crf = Some(999);
        assert_eq!(options_for(&r).unwrap().crf, 63);

        let mut r = req("crf", "h264");
        r.crf = Some(999);
        r.audio_kbps = Some(8);
        assert_eq!(options_for(&r).unwrap().crf, 51);
        assert_eq!(options_for(&r).unwrap().audio_kbps, 32);

        let mut r = req("crf", "h264");
        r.audio_kbps = Some(999);
        r.preset = Some("FAST".into());
        r.max_height = Some(720);
        let opts = options_for(&r).unwrap();
        assert_eq!(opts.audio_kbps, 320);
        assert_eq!(opts.preset, "fast");
        assert_eq!(opts.max_height, Some(720));
    }

    #[test]
    fn rejects_invalid_options() {
        assert!(options_for(&req("crf", "vp9")).is_err());
        assert!(options_for(&req("quality", "h264")).is_err());
        assert!(options_for(&req("crf", "h264")).is_ok());

        let mut r = req("size", "h264");
        r.target_mb = Some(0.0);
        assert!(options_for(&r).is_err());

        let mut r = req("crf", "h264");
        r.preset = Some("insane".into());
        assert!(options_for(&r).is_err());

        r.preset = None;
        r.max_height = Some(1000);
        assert!(options_for(&r).is_err());

        r.max_height = None;
        r.audio_mode = Some("drop".into());
        assert!(options_for(&r).is_err());
    }

    #[test]
    fn scale_only_when_downscaling() {
        assert_eq!(scale_filter(None, Some(1080)), None);
        assert_eq!(scale_filter(Some(1080), Some(1080)), None);
        assert_eq!(scale_filter(Some(720), Some(480)), None);
        let filter = scale_filter(Some(720), Some(1080)).unwrap();
        assert!(filter.contains("min(ih,720)"));
        assert!(filter.contains("force_divisible_by=2"));
    }

    #[test]
    fn parses_encoder_listing() {
        let text = "Encoders:\n V..... = Video\n ------\n V....D libx264      H.264\n V....D libx265      H.265\n V..... libsvtav1    SVT-AV1\n V....D libaom-av1   libaom AV1\n A....D aac          AAC\n";
        let names = parse_encoder_list(text);
        assert!(names.iter().any(|n| n == "libx264"));
        assert!(names.iter().any(|n| n == "libx265"));
        assert!(names.iter().any(|n| n == "libsvtav1"));
        assert!(names.iter().any(|n| n == "libaom-av1"));
        assert!(names.iter().any(|n| n == "aac"));
        assert!(!names.iter().any(|n| n == "Video"));
    }

    #[test]
    fn builds_encoder_arguments() {
        let opts = options_for(&req("crf", "h265")).unwrap();
        let args = base_encode_args(
            &opts,
            Path::new("in.mp4"),
            Some("scale=-2:'min(ih,720)':flags=lanczos"),
        );
        assert!(args.windows(2).any(|w| w[0] == "-c:v" && w[1] == "libx265"));
        assert!(args.windows(2).any(|w| w[0] == "-tag:v" && w[1] == "hvc1"));
        assert!(args.windows(2).any(|w| w[0] == "-vf"));
        assert!(args.windows(2).any(|w| w[0] == "-map" && w[1] == "0:v:0"));
    }

    #[test]
    fn builds_av1_arguments() {
        // SVT-AV1: x264 preset names are translated to 0-13.
        let mut opts = options_for(&req("crf", "av1")).unwrap();
        opts.encoder = "libsvtav1".into();
        let args = base_encode_args(&opts, Path::new("in.mp4"), None);
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-c:v" && w[1] == "libsvtav1"));
        assert!(args.windows(2).any(|w| w[0] == "-preset" && w[1] == "6"));
        assert!(args.windows(2).any(|w| w[0] == "-tag:v" && w[1] == "av01"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-pix_fmt" && w[1] == "yuv420p"));
        assert!(!args.iter().any(|a| a == "-b:v"));
        assert_eq!(
            quality_args(&opts),
            vec!["-crf".to_string(), "32".to_string()]
        );

        // libaom: cpu-used + row threading, and zero bitrate for CRF mode.
        opts.encoder = "libaom-av1".into();
        let args = base_encode_args(&opts, Path::new("in.mp4"), None);
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-c:v" && w[1] == "libaom-av1"));
        assert!(args.windows(2).any(|w| w[0] == "-cpu-used" && w[1] == "5"));
        assert!(args.windows(2).any(|w| w[0] == "-row-mt" && w[1] == "1"));
        assert_eq!(
            quality_args(&opts),
            vec![
                "-crf".to_string(),
                "32".to_string(),
                "-b:v".to_string(),
                "0".to_string()
            ]
        );
    }

    #[test]
    fn maps_presets_for_av1_encoders() {
        assert_eq!(svt_preset("ultrafast"), "12");
        assert_eq!(svt_preset("medium"), "6");
        assert_eq!(svt_preset("veryslow"), "0");
        assert_eq!(svt_preset("unknown"), "6");
        assert_eq!(aom_cpu_used("ultrafast"), "8");
        assert_eq!(aom_cpu_used("medium"), "5");
        assert_eq!(aom_cpu_used("veryslow"), "0");
        assert_eq!(aom_cpu_used("unknown"), "5");
    }

    #[test]
    fn audio_arguments_follow_mode() {
        let mut r = req("crf", "h264");
        r.remove_audio = true;
        let opts = options_for(&r).unwrap();
        assert_eq!(audio_args(&opts), vec!["-an".to_string()]);

        let mut r = req("crf", "h264");
        r.audio_mode = Some("copy".into());
        let opts = options_for(&r).unwrap();
        assert_eq!(
            audio_args(&opts),
            vec!["-c:a".to_string(), "copy".to_string()]
        );

        let opts = options_for(&req("crf", "h264")).unwrap();
        assert_eq!(
            audio_args(&opts),
            vec![
                "-c:a".to_string(),
                "aac".to_string(),
                "-b:a".to_string(),
                "128k".to_string()
            ]
        );

        let no_audio = validate_options(&req("crf", "h264"), false).unwrap();
        assert_eq!(audio_args(&no_audio), vec!["-an".to_string()]);
    }

    #[test]
    fn formats_eta() {
        assert_eq!(format_eta(0), "00:00");
        assert_eq!(format_eta(65), "01:05");
        assert_eq!(format_eta(3725), "1:02:05");
    }

    #[test]
    fn micros_and_truncation() {
        assert_eq!(parse_micros("1500000"), Some(1.5));
        assert_eq!(parse_micros("N/A"), None);
        assert_eq!(parse_micros("-5"), None);
        assert_eq!(truncate("あいうえお", 3), "あいう…");
        assert_eq!(truncate("abc", 5), "abc");
        assert_eq!(sanitize_token("job/../x"), "job____x");
        assert_eq!(sanitize_token(""), "job");
    }

    #[test]
    fn progress_tracks_speed_and_percent() {
        // EncodeProgress emits through Job; the pure pieces are exercised here.
        assert_eq!(parse_timecode("00:01:02.5"), Some(62.5));
        assert_eq!(parse_micros("62500000"), Some(62.5));
    }
}
