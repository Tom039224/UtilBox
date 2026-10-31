//! Universal file conversion engine.
//!
//! The engine is intentionally built on CLI tools (FFmpeg, ImageMagick,
//! LibreOffice, Pandoc, Poppler, 7-Zip) so the application keeps working
//! completely offline.  A conversion is described by a small [`Op`] list
//! ("plan") and executed step by step, using temporary files between steps.
//!
//! Sections:
//!   1. format registry
//!   2. detection / helpers
//!   3. planner (pure, unit-testable)
//!   4. batch driver
//!   5. operation executors
//!   6. filesystem helpers
//!   7. tests

use crate::job::Job;
use crate::process::{parse_timecode, run_capture, run_streaming, Cmd, Outcome, Stream};
use crate::tools::Tools;
use crate::types::{ConvertItemResult, ConvertRequest, ConvertResponse, DetectedFile, FormatInfo};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};
use uuid::Uuid;

const CANCELLED: &str = "キャンセルされました";
const MAX_STEPS: usize = 3;

// ---------------------------------------------------------------------------
// 1. Format registry
// ---------------------------------------------------------------------------

struct FormatDef {
    id: &'static str,
    label: &'static str,
    ext: &'static str,
    category: &'static str,
    can_input: bool,
    can_output: bool,
    note: Option<&'static str>,
}

const FORMATS: &[FormatDef] = &[
    // --- image -------------------------------------------------------------
    FormatDef {
        id: "png",
        label: "PNG 画像",
        ext: "png",
        category: "image",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "jpg",
        label: "JPEG 画像",
        ext: "jpg",
        category: "image",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "webp",
        label: "WebP 画像",
        ext: "webp",
        category: "image",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "gif",
        label: "GIF 画像",
        ext: "gif",
        category: "image",
        can_input: true,
        can_output: true,
        note: Some("動画からは FFmpeg で変換します"),
    },
    FormatDef {
        id: "bmp",
        label: "BMP 画像",
        ext: "bmp",
        category: "image",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "tiff",
        label: "TIFF 画像",
        ext: "tiff",
        category: "image",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "ico",
        label: "ICO アイコン",
        ext: "ico",
        category: "image",
        can_input: true,
        can_output: true,
        note: Some("複数サイズを自動生成します"),
    },
    FormatDef {
        id: "avif",
        label: "AVIF 画像",
        ext: "avif",
        category: "image",
        can_input: true,
        can_output: true,
        note: Some("ImageMagick の AVIF 対応に依存します"),
    },
    FormatDef {
        id: "heic",
        label: "HEIC 画像",
        ext: "heic",
        category: "image",
        can_input: true,
        can_output: true,
        note: Some("ImageMagick の HEIC デリゲートに依存します"),
    },
    FormatDef {
        id: "svg",
        label: "SVG 画像",
        ext: "svg",
        category: "image",
        can_input: true,
        can_output: false,
        note: Some("SVG は入力のみ対応しています"),
    },
    FormatDef {
        id: "tga",
        label: "TGA 画像",
        ext: "tga",
        category: "image",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "ppm",
        label: "PPM 画像",
        ext: "ppm",
        category: "image",
        can_input: true,
        can_output: true,
        note: None,
    },
    // --- audio -------------------------------------------------------------
    FormatDef {
        id: "mp3",
        label: "MP3 音声",
        ext: "mp3",
        category: "audio",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "wav",
        label: "WAV 音声",
        ext: "wav",
        category: "audio",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "flac",
        label: "FLAC 音声",
        ext: "flac",
        category: "audio",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "ogg",
        label: "OGG 音声",
        ext: "ogg",
        category: "audio",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "opus",
        label: "Opus 音声",
        ext: "opus",
        category: "audio",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "m4a",
        label: "M4A 音声",
        ext: "m4a",
        category: "audio",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "aac",
        label: "AAC 音声",
        ext: "aac",
        category: "audio",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "wma",
        label: "WMA 音声",
        ext: "wma",
        category: "audio",
        can_input: true,
        can_output: true,
        note: Some("再生環境によっては対応していません"),
    },
    FormatDef {
        id: "aiff",
        label: "AIFF 音声",
        ext: "aiff",
        category: "audio",
        can_input: true,
        can_output: true,
        note: None,
    },
    // --- video -------------------------------------------------------------
    FormatDef {
        id: "mp4",
        label: "MP4 動画",
        ext: "mp4",
        category: "video",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "mkv",
        label: "MKV 動画",
        ext: "mkv",
        category: "video",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "webm",
        label: "WebM 動画",
        ext: "webm",
        category: "video",
        can_input: true,
        can_output: true,
        note: Some("VP9 エンコードは時間がかかる場合があります"),
    },
    FormatDef {
        id: "avi",
        label: "AVI 動画",
        ext: "avi",
        category: "video",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "mov",
        label: "MOV 動画",
        ext: "mov",
        category: "video",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "m4v",
        label: "M4V 動画",
        ext: "m4v",
        category: "video",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "ts",
        label: "TS 動画",
        ext: "ts",
        category: "video",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "flv",
        label: "FLV 動画",
        ext: "flv",
        category: "video",
        can_input: true,
        can_output: true,
        note: Some("H.264 / AAC で出力します"),
    },
    FormatDef {
        id: "wmv",
        label: "WMV 動画",
        ext: "wmv",
        category: "video",
        can_input: true,
        can_output: true,
        note: Some("古い再生環境向けの形式です"),
    },
    // --- document ----------------------------------------------------------
    FormatDef {
        id: "pdf",
        label: "PDF 文書",
        ext: "pdf",
        category: "document",
        can_input: true,
        can_output: true,
        note: Some("画像への変換は1ページ目のみです"),
    },
    FormatDef {
        id: "docx",
        label: "Word 文書 (DOCX)",
        ext: "docx",
        category: "document",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "doc",
        label: "Word 文書 (DOC)",
        ext: "doc",
        category: "document",
        can_input: true,
        can_output: true,
        note: Some("LibreOffice が必要です"),
    },
    FormatDef {
        id: "odt",
        label: "OpenDocument 文書 (ODT)",
        ext: "odt",
        category: "document",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "rtf",
        label: "RTF 文書",
        ext: "rtf",
        category: "document",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "txt",
        label: "テキスト (TXT)",
        ext: "txt",
        category: "document",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "md",
        label: "Markdown",
        ext: "md",
        category: "document",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "html",
        label: "HTML 文書",
        ext: "html",
        category: "document",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "epub",
        label: "EPUB 電子書籍",
        ext: "epub",
        category: "document",
        can_input: true,
        can_output: true,
        note: None,
    },
    // --- archive -----------------------------------------------------------
    FormatDef {
        id: "zip",
        label: "ZIP アーカイブ",
        ext: "zip",
        category: "archive",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "7z",
        label: "7z アーカイブ",
        ext: "7z",
        category: "archive",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "tar",
        label: "TAR アーカイブ",
        ext: "tar",
        category: "archive",
        can_input: true,
        can_output: true,
        note: None,
    },
    FormatDef {
        id: "gz",
        label: "GZIP 圧縮",
        ext: "gz",
        category: "archive",
        can_input: true,
        can_output: true,
        note: Some("単一ファイルのアーカイブのみ対応しています"),
    },
    FormatDef {
        id: "xz",
        label: "XZ 圧縮",
        ext: "xz",
        category: "archive",
        can_input: true,
        can_output: true,
        note: Some("単一ファイルのアーカイブのみ対応しています"),
    },
    FormatDef {
        id: "bz2",
        label: "BZIP2 圧縮",
        ext: "bz2",
        category: "archive",
        can_input: true,
        can_output: true,
        note: Some("単一ファイルのアーカイブのみ対応しています"),
    },
];

/// Android ships FFmpeg only, so just the media formats it can read and write.
/// Documents and archives need desktop tools; AVIF / HEIC / SVG need ImageMagick
/// delegates the FFmpeg build does not include.
fn is_available(format: &FormatDef) -> bool {
    if !cfg!(target_os = "android") {
        return true;
    }
    matches!(format.category, "image" | "audio" | "video")
        && !matches!(format.id, "avif" | "heic" | "svg")
}

pub fn list_formats() -> Vec<FormatInfo> {
    FORMATS
        .iter()
        .filter(|f| is_available(f))
        .map(|f| FormatInfo {
            id: f.id.to_string(),
            label: f.label.to_string(),
            ext: f.ext.to_string(),
            category: f.category.to_string(),
            can_input: f.can_input,
            can_output: f.can_output,
            note: f.note.map(str::to_string),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 2. Detection and small helpers
// ---------------------------------------------------------------------------

/// Lowercases an extension and folds the aliases users actually have on disk.
fn normalize_ext(raw: &str) -> String {
    let s = raw
        .trim()
        .trim_start_matches('.')
        .trim()
        .to_ascii_lowercase();
    let mapped = match s.as_str() {
        "jpeg" | "jpe" | "jfif" => "jpg",
        "tif" => "tiff",
        "htm" | "xhtml" => "html",
        "mpeg" | "mpg" | "mpe" | "3gp" | "3g2" => "mp4",
        "oga" => "ogg",
        "weba" => "webm",
        "markdown" | "mdown" | "mkd" => "md",
        "text" => "txt",
        "bzip2" => "bz2",
        "aif" | "aifc" => "aiff",
        _ => s.as_str(),
    };
    mapped.to_string()
}

fn find_format(id: &str) -> Option<&'static FormatDef> {
    let norm = normalize_ext(id);
    FORMATS.iter().find(|f| f.id == norm && is_available(f))
}

fn category_of(id: &str) -> Option<&'static str> {
    find_format(id).map(|f| f.category)
}

fn is_image_id(id: &str) -> bool {
    category_of(id) == Some("image")
}

fn is_audio_id(id: &str) -> bool {
    matches!(
        id,
        "mp3" | "wav" | "flac" | "ogg" | "opus" | "m4a" | "aac" | "wma" | "aiff"
    )
}

fn is_video_id(id: &str) -> bool {
    matches!(
        id,
        "mp4" | "mkv" | "webm" | "avi" | "mov" | "m4v" | "ts" | "flv" | "wmv"
    )
}

fn is_document_id(id: &str) -> bool {
    category_of(id) == Some("document")
}

fn is_archive_id(id: &str) -> bool {
    category_of(id) == Some("archive")
}

/// GIF is an image format but behaves like a video stream for FFmpeg purposes.
fn is_media_source(id: &str) -> bool {
    is_audio_id(id) || is_video_id(id) || id == "gif"
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn stem_of(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "output".to_string())
}

fn ext_of(path: &Path) -> String {
    path.extension()
        .map(|e| normalize_ext(&e.to_string_lossy()))
        .unwrap_or_default()
}

fn pstr(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut s: String = text.chars().take(max).collect();
        s.push('…');
        s
    }
}

pub fn detect_file(path: &str) -> DetectedFile {
    let p = Path::new(path);
    let name = file_name_of(p);
    let raw_ext = p
        .extension()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let ext = normalize_ext(&raw_ext);
    let fmt = find_format(&ext).filter(|f| f.can_input);
    let size = fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    DetectedFile {
        path: path.to_string(),
        name,
        ext: ext.clone(),
        size,
        category: fmt
            .map(|f| f.category.to_string())
            .unwrap_or_else(|| "other".into()),
        format_id: fmt.map(|f| f.id.to_string()),
    }
}

// ---------------------------------------------------------------------------
// 3. Planner
// ---------------------------------------------------------------------------

/// One conversion step. Every step reads one file and writes exactly one file.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Op {
    Ffmpeg { to: String, still_image: bool },
    Magick { to: String },
    FfmpegImage { to: String },
    PdfToImage { to: String },
    PdfToText,
    LibreOffice { to: String },
    Pandoc { to: String },
    ArchiveRepack { to: String },
    Copy,
}

impl Op {
    fn output_ext(&self, input: &Path) -> String {
        match self {
            Op::Ffmpeg { to, .. }
            | Op::Magick { to }
            | Op::FfmpegImage { to }
            | Op::PdfToImage { to }
            | Op::LibreOffice { to }
            | Op::Pandoc { to }
            | Op::ArchiveRepack { to } => to.clone(),
            Op::PdfToText => "txt".to_string(),
            Op::Copy => ext_of(input),
        }
    }

    fn tool_label(&self) -> &'static str {
        match self {
            Op::Ffmpeg { .. } => "FFmpeg",
            Op::Magick { .. } => "ImageMagick",
            Op::FfmpegImage { .. } => "FFmpeg",
            Op::PdfToImage { .. } => "pdftoppm",
            Op::PdfToText => "pdftotext",
            Op::LibreOffice { .. } => "LibreOffice",
            Op::Pandoc { .. } => "Pandoc",
            Op::ArchiveRepack { .. } => "7-Zip",
            Op::Copy => "コピー",
        }
    }
}

fn ff(to: &str, still_image: bool) -> Op {
    Op::Ffmpeg {
        to: to.to_string(),
        still_image,
    }
}
fn magick(to: &str) -> Op {
    Op::Magick { to: to.to_string() }
}
fn ff_image(to: &str) -> Op {
    Op::FfmpegImage { to: to.to_string() }
}
fn pdf_image(to: &str) -> Op {
    Op::PdfToImage { to: to.to_string() }
}
fn lo(to: &str) -> Op {
    Op::LibreOffice { to: to.to_string() }
}
fn pandoc(to: &str) -> Op {
    Op::Pandoc { to: to.to_string() }
}
fn repack(to: &str) -> Op {
    Op::ArchiveRepack { to: to.to_string() }
}

/// Builds a conversion plan from two format ids. Pure: no filesystem or tool
/// access, so it can be unit tested without an app instance.
fn plan_for(from_raw: &str, to_raw: &str) -> Option<Vec<Op>> {
    let from = normalize_ext(from_raw);
    let to = normalize_ext(to_raw);
    let f = find_format(&from)?;
    let t = find_format(&to)?;
    if !f.can_input || !t.can_output {
        return None;
    }
    if f.id == t.id {
        return Some(vec![Op::Copy]);
    }

    // A. media conversion (audio / video / animated GIF)
    let from_media = is_media_source(f.id);
    let to_media = is_media_source(t.id);
    if from_media && to_media {
        return Some(vec![ff(t.id, false)]);
    }
    // Still image -> video (a 5 second clip).
    if f.category == "image" && f.id != "gif" && is_video_id(t.id) {
        return Some(vec![ff(t.id, true)]);
    }
    // Image -> image / PDF.
    if f.category == "image" {
        if t.category == "image" {
            // No ImageMagick on Android: FFmpeg handles the common still formats.
            return Some(vec![if cfg!(target_os = "android") {
                ff_image(t.id)
            } else {
                magick(t.id)
            }]);
        }
        if t.id == "pdf" {
            return Some(vec![magick("pdf")]);
        }
        return None;
    }

    // B. documents
    if f.category == "document" {
        if t.category == "image" {
            if f.id == "pdf" {
                return Some(pdf_to_image_plan(t.id));
            }
            let mut chain = document_to_pdf_chain(f.id);
            chain.push(pdf_image(t.id));
            return if chain.len() <= MAX_STEPS {
                Some(chain)
            } else {
                None
            };
        }
        if t.category == "document" {
            if f.id == "pdf" {
                return pdf_to_document_plan(t.id);
            }
            if t.id == "pdf" {
                return Some(document_to_pdf_chain(f.id));
            }
            return document_to_document_plan(f.id, t.id);
        }
        return None;
    }

    // C. archives
    if f.category == "archive" && t.category == "archive" {
        return Some(vec![repack(t.id)]);
    }

    None
}

fn document_to_pdf_chain(from: &str) -> Vec<Op> {
    match from {
        // Pandoc can read these; LibreOffice cannot (reliably) open Markdown/EPUB.
        "md" | "epub" => vec![pandoc("html"), lo("pdf")],
        _ => vec![lo("pdf")],
    }
}

fn pdf_to_image_plan(to: &str) -> Vec<Op> {
    if matches!(to, "png" | "jpg" | "tiff") {
        vec![pdf_image(to)]
    } else {
        // Poppler rasterizes first, ImageMagick handles the exotic target.
        vec![pdf_image("png"), magick(to)]
    }
}

fn pdf_to_document_plan(to: &str) -> Option<Vec<Op>> {
    match to {
        "txt" => Some(vec![Op::PdfToText]),
        "md" | "html" | "docx" | "odt" | "rtf" | "epub" => Some(vec![Op::PdfToText, pandoc(to)]),
        "doc" => Some(vec![Op::PdfToText, lo("doc")]),
        _ => None,
    }
}

fn document_to_document_plan(from: &str, to: &str) -> Option<Vec<Op>> {
    let markup_out = matches!(to, "md" | "html" | "epub" | "txt");
    match from {
        // Markdown / EPUB: Pandoc everywhere except the legacy .doc target.
        "md" | "epub" => {
            if to == "doc" {
                Some(vec![pandoc("html"), lo("doc")])
            } else {
                Some(vec![pandoc(to)])
            }
        }
        // HTML is readable by both; Pandoc produces cleaner markup output.
        "html" => {
            if to == "doc" {
                Some(vec![lo("doc")])
            } else {
                Some(vec![pandoc(to)])
            }
        }
        // Modern office / plain text: Pandoc for markup, LibreOffice for
        // binary office formats (better style fidelity).
        "docx" | "odt" | "txt" => {
            if markup_out {
                Some(vec![pandoc(to)])
            } else {
                Some(vec![lo(to)])
            }
        }
        // Legacy DOC and RTF cannot be parsed by Pandoc; route through
        // LibreOffice, adding Pandoc when the target is Markdown/EPUB.
        "doc" | "rtf" => match to {
            "txt" | "html" => Some(vec![lo(to)]),
            "md" | "epub" => Some(vec![lo("html"), pandoc(to)]),
            "docx" | "doc" | "odt" | "rtf" => Some(vec![lo(to)]),
            _ => None,
        },
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// 4. Batch driver
// ---------------------------------------------------------------------------

/// Removes a temporary tree when dropped (best effort), including on errors.
struct TempGuard(PathBuf);

impl Drop for TempGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Removes a partially written output file when a step fails. It is only
/// armed when the destination did not exist before the item started, so a
/// pre-existing file is never deleted by an unrelated failure.
struct FileGuard {
    path: PathBuf,
    armed: bool,
}

impl FileGuard {
    fn new(path: PathBuf) -> Self {
        let armed = !path.exists();
        Self { path, armed }
    }

    fn keep(mut self) {
        self.armed = false;
    }
}

impl Drop for FileGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub fn convert_batch(
    job: &Job,
    tools: &Tools,
    req: &ConvertRequest,
) -> Result<ConvertResponse, String> {
    if req.inputs.is_empty() {
        return Err("変換するファイルが指定されていません".into());
    }
    let target_id = normalize_ext(&req.target);
    let target =
        find_format(&target_id).ok_or_else(|| format!("未対応の出力形式です: {}", req.target))?;
    if !target.can_output {
        return Err(format!("{} への出力には対応していません", target.label));
    }
    if req.output_dir.trim().is_empty() {
        return Err("出力フォルダが指定されていません".into());
    }

    let out_dir_raw = PathBuf::from(req.output_dir.trim());
    fs::create_dir_all(&out_dir_raw).map_err(|e| {
        format!(
            "出力フォルダを作成できません ({}): {e}",
            out_dir_raw.display()
        )
    })?;
    // Canonical paths matter for 7-Zip, which runs with the extraction folder
    // as its working directory.
    let out_dir = fs::canonicalize(&out_dir_raw).unwrap_or(out_dir_raw);

    let batch_temp = std::env::temp_dir().join(format!(
        "utilbox-{}-{}",
        sanitize_component(&job.id),
        Uuid::new_v4()
    ));
    fs::create_dir_all(&batch_temp).map_err(|e| {
        format!(
            "作業用フォルダを作成できません ({}): {e}",
            batch_temp.display()
        )
    })?;
    let _temp_guard = TempGuard(batch_temp.clone());
    let profile_dir = batch_temp.join("libreoffice-profile");

    let total = req.inputs.len();
    let mut results: Vec<ConvertItemResult> = Vec::with_capacity(total);
    let mut succeeded = 0usize;
    let mut failed = 0usize;

    for (idx, input) in req.inputs.iter().enumerate() {
        job.check()?;
        let name = file_name_of(Path::new(input));
        let size_before = fs::metadata(input).map(|m| m.len()).unwrap_or(0);
        let started = Instant::now();
        let target_ext = target.ext.to_uppercase();

        // Overall progress = completed files + the current file's own 0..100.
        let mut report = |inner: f64| {
            let inner = inner.clamp(0.0, 100.0);
            let overall = (idx as f64 + inner / 100.0) / total as f64 * 100.0;
            job.progress(
                overall,
                "convert",
                format!("{}/{} · {} → {}", idx + 1, total, name, target_ext),
            );
        };
        report(0.0);

        match run_item(
            job,
            tools,
            input,
            target,
            &out_dir,
            req.overwrite,
            &batch_temp,
            &profile_dir,
            &mut report,
        ) {
            Ok((path, message)) => {
                let size_after = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                let message = if size_after == 0 && target.id == "txt" {
                    format!("{message}（テキストが空でした。スキャン PDF の可能性があります）")
                } else {
                    message
                };
                succeeded += 1;
                results.push(ConvertItemResult {
                    input: input.clone(),
                    name: name.clone(),
                    output: Some(path.display().to_string()),
                    ok: true,
                    message,
                    size_before,
                    size_after,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                });
            }
            Err(err) => {
                if err == CANCELLED || job.is_cancelled() {
                    return Err(CANCELLED.to_string());
                }
                let short = truncate(&err, 600);
                job.log(format!(
                    "失敗: {}: {}",
                    name,
                    short.replace(['\n', '\r'], " ")
                ));
                failed += 1;
                results.push(ConvertItemResult {
                    input: input.clone(),
                    name,
                    output: None,
                    ok: false,
                    message: short,
                    size_before,
                    size_after: 0,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                });
            }
        }
    }

    job.progress(
        100.0,
        "convert",
        format!("変換完了（成功 {succeeded} / 失敗 {failed}）"),
    );
    Ok(ConvertResponse {
        results,
        succeeded,
        failed,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_item(
    job: &Job,
    tools: &Tools,
    input_str: &str,
    target: &FormatDef,
    output_dir: &Path,
    overwrite: bool,
    batch_temp: &Path,
    profile_dir: &Path,
    progress: &mut dyn FnMut(f64),
) -> Result<(PathBuf, String), String> {
    let raw = Path::new(input_str);
    if !raw.is_file() {
        return Err("ファイルが見つかりません".into());
    }
    let input = fs::canonicalize(raw).unwrap_or_else(|_| raw.to_path_buf());
    let detected = detect_file(input_str);
    let from_norm = normalize_ext(&detected.ext);
    let from = find_format(&from_norm)
        .filter(|f| f.can_input)
        .ok_or_else(|| "未対応の入力形式です".to_string())?;

    // Same format: never re-encode, just copy (and never touch the input).
    if from.id == target.id {
        let dest = choose_output_path(output_dir, &stem_of(&input), &target.ext, overwrite, &input);
        let guard = FileGuard::new(dest.clone());
        progress(5.0);
        copy_file(&input, &dest)?;
        progress(100.0);
        guard.keep();
        job.log(format!(
            "同形式コピー: {} → {}",
            detected.name,
            dest.display()
        ));
        return Ok((dest, "同形式のためコピーしました".to_string()));
    }

    let ops = plan_for(from.id, target.id).ok_or_else(|| {
        format!(
            "この変換には対応していません: {} → {}",
            from.id.to_uppercase(),
            target.ext.to_uppercase()
        )
    })?;
    if ops.is_empty() || ops.len() > MAX_STEPS {
        return Err("この変換には対応していません".into());
    }

    job.log(format!(
        "{} → {}: {}",
        from.id.to_uppercase(),
        target.ext.to_uppercase(),
        ops.iter()
            .map(|o| o.tool_label())
            .collect::<Vec<_>>()
            .join(" → ")
    ));

    let dest = choose_output_path(output_dir, &stem_of(&input), &target.ext, overwrite, &input);
    let guard = FileGuard::new(dest.clone());

    let item_temp = batch_temp.join(format!("item-{}", Uuid::new_v4()));
    fs::create_dir_all(&item_temp).map_err(|e| {
        format!(
            "作業用フォルダを作成できません ({}): {e}",
            item_temp.display()
        )
    })?;
    let _item_guard = TempGuard(item_temp.clone());
    let ctx = ExecCtx {
        temp_dir: &item_temp,
        profile_dir,
    };

    let total_steps = ops.len();
    let mut current = input;
    for (i, op) in ops.iter().enumerate() {
        job.check()?;
        let is_last = i + 1 == total_steps;
        let out = if is_last {
            dest.clone()
        } else {
            item_temp.join(format!("step{}.{}", i + 1, op.output_ext(&current)))
        };
        let base = i as f64 * 100.0 / total_steps as f64;
        let span = 100.0 / total_steps as f64;
        let mut step_progress = |p: f64| {
            progress(base + span * p.clamp(0.0, 100.0) / 100.0);
        };
        execute_op(job, tools, op, &current, &out, &ctx, &mut step_progress)?;
        if !out.exists() {
            return Err(format!(
                "{} の出力ファイルが作成されませんでした",
                op.tool_label()
            ));
        }
        current = out;
    }

    if !dest.exists() {
        return Err("出力ファイルが作成されませんでした".into());
    }
    let message = success_message(from.id, target.id);
    guard.keep();
    Ok((dest, message))
}

fn success_message(from: &str, to: &str) -> String {
    let mut message = if from == "pdf" && is_image_id(to) {
        "PDF の1ページ目を変換しました".to_string()
    } else if is_document_id(from) && from != "pdf" && is_image_id(to) {
        "1ページ目のみ変換しました".to_string()
    } else if is_archive_id(from) && is_archive_id(to) {
        "アーカイブを再作成しました".to_string()
    } else if from == "pdf" && to == "txt" {
        "テキストを抽出しました".to_string()
    } else {
        "変換しました".to_string()
    };

    let mut notes: Vec<&str> = Vec::new();
    if from == "pdf" && is_document_id(to) && to != "txt" {
        notes.push("レイアウトは保持されない場合があります");
    }
    if from == "pdf" && is_image_id(to) {
        // The primary message already says this.
    } else if is_document_id(from) && from != "pdf" && is_image_id(to) {
        notes.push("PDF 経由で1ページ目を画像化しています");
    }
    if !notes.is_empty() {
        message = format!("{message}（{}）", notes.join("／"));
    }
    message
}

// ---------------------------------------------------------------------------
// 5. Operation executors
// ---------------------------------------------------------------------------

struct ExecCtx<'a> {
    temp_dir: &'a Path,
    profile_dir: &'a Path,
}

fn execute_op(
    job: &Job,
    tools: &Tools,
    op: &Op,
    input: &Path,
    output: &Path,
    ctx: &ExecCtx<'_>,
    progress: &mut dyn FnMut(f64),
) -> Result<(), String> {
    job.check()?;
    match op {
        Op::Ffmpeg { to, still_image } => {
            exec_ffmpeg(job, tools, to, *still_image, input, output, progress)
        }
        Op::Magick { to } => magick_convert(job, tools, to, input, output, progress),
        Op::FfmpegImage { to } => ffmpeg_image_convert(job, tools, to, input, output, progress),
        Op::PdfToImage { to } => {
            exec_pdf_to_image(job, tools, to, input, output, ctx.temp_dir, progress)
        }
        Op::PdfToText => exec_pdf_to_text(job, tools, input, output, progress),
        Op::LibreOffice { to } => exec_libreoffice(job, tools, to, input, output, ctx, progress),
        Op::Pandoc { to } => exec_pandoc(job, tools, to, input, output, progress),
        Op::ArchiveRepack { to } => {
            exec_archive(job, tools, to, input, output, ctx.temp_dir, progress)
        }
        Op::Copy => {
            progress(5.0);
            copy_file(input, output)?;
            progress(100.0);
            Ok(())
        }
    }
}

// --- process helpers -------------------------------------------------------

fn run_quiet(job: &Job, cmd: &Cmd, label: &str) -> Result<Outcome, String> {
    job.log(format!("{label} で処理中…"));
    match run_streaming(job, cmd, |_, _| {}) {
        Ok(outcome) => Ok(outcome),
        Err(e) => {
            if job.is_cancelled() {
                Err(CANCELLED.to_string())
            } else {
                Err(e)
            }
        }
    }
}

fn log_tail(job: &Job, label: &str, outcome: &Outcome) {
    let lines: Vec<&str> = outcome
        .stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    for line in lines.iter().rev().take(4).rev() {
        job.log(format!("{label}: {}", truncate(line, 240)));
    }
}

fn check_outcome(job: &Job, label: &str, outcome: Outcome) -> Result<(), String> {
    if outcome.cancelled || job.is_cancelled() {
        return Err(CANCELLED.to_string());
    }
    if !outcome.success() {
        log_tail(job, label, &outcome);
        return Err(format!(
            "{label} に失敗しました: {}",
            truncate(&outcome.error_text(), 600)
        ));
    }
    Ok(())
}

struct StreamInfo {
    has_video: bool,
    has_audio: bool,
}

fn probe_streams(tools: &Tools, input: &Path) -> Option<StreamInfo> {
    let ffprobe = tools.require("ffprobe").ok()?;
    let cmd = Cmd::new(&ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type",
            "-of",
            "default=nw=1:nk=1",
        ])
        .arg(pstr(input));
    let out = run_capture(&cmd).ok()?;
    if !out.status.map(|s| s.success()).unwrap_or(false) {
        return None;
    }
    Some(StreamInfo {
        has_video: out
            .stdout
            .lines()
            .any(|l| l.trim().eq_ignore_ascii_case("video")),
        has_audio: out
            .stdout
            .lines()
            .any(|l| l.trim().eq_ignore_ascii_case("audio")),
    })
}

fn probe_duration(tools: &Tools, input: &Path) -> Option<f64> {
    let ffprobe = tools.require("ffprobe").ok()?;
    let cmd = Cmd::new(&ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=nw=1:nk=1",
        ])
        .arg(pstr(input));
    let out = run_capture(&cmd).ok()?;
    if !out.status.map(|s| s.success()).unwrap_or(false) {
        return None;
    }
    out.stdout
        .lines()
        .find_map(|l| l.trim().parse::<f64>().ok())
        .filter(|d| d.is_finite() && *d > 0.0)
}

// --- FFmpeg ----------------------------------------------------------------

fn video_codec_args(to: &str) -> Vec<&'static str> {
    match to {
        "mp4" | "m4v" | "mov" => vec![
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            "20",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-movflags",
            "+faststart",
        ],
        "mkv" => vec![
            "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-c:a", "aac", "-b:a", "192k",
        ],
        "webm" => vec![
            "-c:v",
            "libvpx-vp9",
            "-b:v",
            "0",
            "-crf",
            "32",
            "-c:a",
            "libopus",
            "-b:a",
            "160k",
        ],
        "avi" => vec![
            "-c:v",
            "mpeg4",
            "-q:v",
            "4",
            "-c:a",
            "libmp3lame",
            "-b:a",
            "192k",
        ],
        "ts" => vec![
            "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-c:a", "aac", "-b:a", "160k",
        ],
        "flv" => vec![
            "-c:v", "libx264", "-preset", "veryfast", "-crf", "23", "-c:a", "aac", "-b:a", "128k",
            "-ar", "44100",
        ],
        "wmv" => vec!["-c:v", "wmv2", "-q:v", "4", "-c:a", "wmav2", "-b:a", "192k"],
        _ => Vec::new(),
    }
}

fn audio_codec_args(to: &str) -> Vec<&'static str> {
    match to {
        "mp3" => vec!["-vn", "-c:a", "libmp3lame", "-q:a", "2"],
        "wav" => vec!["-vn", "-c:a", "pcm_s16le"],
        "flac" => vec!["-vn", "-c:a", "flac"],
        "ogg" => vec!["-vn", "-c:a", "libvorbis", "-q:a", "6"],
        "opus" => vec!["-vn", "-c:a", "libopus", "-b:a", "160k"],
        "m4a" | "aac" => vec!["-vn", "-c:a", "aac", "-b:a", "192k"],
        "wma" => vec!["-vn", "-c:a", "wmav2", "-b:a", "192k"],
        "aiff" => vec!["-vn", "-c:a", "pcm_s16be"],
        _ => Vec::new(),
    }
}

const GIF_FILTER: &str = "fps=12,scale='min(960,iw)':-2:flags=lanczos,split[s0][s1];[s0]palettegen=max_colors=192[p];[s1][p]paletteuse=dither=bayer";

fn exec_ffmpeg(
    job: &Job,
    tools: &Tools,
    to: &str,
    still_image: bool,
    input: &Path,
    output: &Path,
    progress: &mut dyn FnMut(f64),
) -> Result<(), String> {
    let ffmpeg = tools.require("ffmpeg")?;
    let target_video = is_video_id(to) || to == "gif";
    let target_audio = is_audio_id(to);
    if !target_video && !target_audio {
        return Err(format!("FFmpeg では {to} に変換できません"));
    }

    // Check the streams up-front so users get a clear message instead of an
    // opaque encoder error.
    let mut has_audio = false;
    if !still_image {
        if let Some(info) = probe_streams(tools, input) {
            has_audio = info.has_audio;
            if target_video && !info.has_video {
                return Err("音声のみのファイルは動画形式に変換できません".into());
            }
            if target_audio && !info.has_audio {
                return Err("音声トラックが含まれていません".into());
            }
        } else {
            // No ffprobe: assume the source contains what the target needs.
            has_audio = target_video;
        }
    }

    let mut args: Vec<String> = [
        "-y",
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostats",
        "-progress",
        "pipe:1",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    if still_image {
        args.push("-loop".into());
        args.push("1".into());
    }
    args.push("-i".into());
    args.push(pstr(input));

    if still_image {
        args.push("-t".into());
        args.push("5".into());
        args.push("-vf".into());
        args.push("scale=trunc(iw/2)*2:trunc(ih/2)*2".into());
        args.push("-r".into());
        args.push("30".into());
    }

    if to == "gif" {
        args.push("-an".into());
        args.push("-loop".into());
        args.push("0".into());
        args.push("-vf".into());
        args.push(GIF_FILTER.into());
    } else if target_video {
        args.extend(video_codec_args(to).into_iter().map(str::to_string));
        if !has_audio {
            args.push("-an".into());
        }
    } else {
        args.extend(audio_codec_args(to).into_iter().map(str::to_string));
    }
    args.push(pstr(output));

    let cmd = Cmd::new(&ffmpeg).args(args);
    job.log(format!(
        "FFmpeg で変換中… ({} → {})",
        ext_of(input).to_uppercase(),
        to.to_uppercase()
    ));

    let duration = if still_image {
        Some(5.0)
    } else {
        probe_duration(tools, input)
    };
    let mut last_pct = 2.0f64;
    progress(2.0);

    let outcome = run_streaming(job, &cmd, |stream, line| {
        if stream != Stream::Out {
            return;
        }
        let line = line.trim();
        if line == "progress=end" {
            if last_pct < 100.0 {
                last_pct = 100.0;
                progress(100.0);
            }
            return;
        }
        // ffmpeg's out_time_ms is microseconds despite the name.
        let seconds = line
            .strip_prefix("out_time_us=")
            .or_else(|| line.strip_prefix("out_time_ms="))
            .and_then(|v| v.trim().parse::<f64>().ok())
            .map(|us| us / 1_000_000.0)
            .or_else(|| line.strip_prefix("out_time=").and_then(parse_timecode));
        if let (Some(seconds), Some(total)) = (seconds, duration) {
            if total > 0.0 && seconds.is_finite() {
                let pct = (seconds / total * 95.0).clamp(2.0, 95.0);
                if pct > last_pct + 0.5 {
                    last_pct = pct;
                    progress(pct);
                }
            }
        }
    })?;

    check_outcome(job, "FFmpeg", outcome)?;
    progress(100.0);
    Ok(())
}

// --- FFmpeg (still images) -------------------------------------------------

/// Encoder arguments and muxer for a still-image target.
fn still_image_args(to: &str) -> Option<(Vec<&'static str>, Option<&'static str>)> {
    let args: Vec<&'static str> = match to {
        "png" => vec!["-c:v", "png"],
        // JPEG has no alpha: flatten onto white like the ImageMagick path does.
        "jpg" => vec![
            "-vf",
            "split[a][b];[a]drawbox=x=0:y=0:w=iw:h=ih:color=white:t=fill[bg];[bg][b]overlay=format=auto,format=yuvj420p",
            "-c:v",
            "mjpeg",
            "-q:v",
            "2",
        ],
        "webp" => vec!["-c:v", "libwebp", "-quality", "92"],
        "bmp" => vec!["-c:v", "bmp"],
        "tiff" => vec!["-c:v", "tiff"],
        "ppm" => vec!["-c:v", "ppm"],
        "tga" => vec!["-c:v", "targa"],
        "gif" => vec!["-c:v", "gif"],
        // ICO holds at most 256x256.
        "ico" => vec![
            "-vf",
            "scale='min(256,iw)':'min(256,ih)':force_original_aspect_ratio=decrease",
            "-c:v",
            "png",
        ],
        _ => return None,
    };
    // Single-picture targets go through the image2 muxer; gif / ico have their own.
    let muxer = match to {
        "gif" | "ico" => None,
        _ => Some("image2"),
    };
    Some((args, muxer))
}

fn ffmpeg_image_convert(
    job: &Job,
    tools: &Tools,
    to: &str,
    input: &Path,
    output: &Path,
    progress: &mut dyn FnMut(f64),
) -> Result<(), String> {
    let ffmpeg = tools.require("ffmpeg")?;
    let (codec_args, muxer) =
        still_image_args(to).ok_or_else(|| format!("FFmpeg では {to} に変換できません"))?;

    let mut args: Vec<String> = ["-y", "-hide_banner", "-loglevel", "error", "-nostats", "-i"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    args.push(pstr(input));
    args.extend(["-frames:v", "1"].iter().map(|s| s.to_string()));
    args.extend(codec_args.into_iter().map(str::to_string));
    if let Some(muxer) = muxer {
        args.extend(["-f", muxer, "-update", "1"].iter().map(|s| s.to_string()));
    }
    args.push(pstr(output));

    progress(5.0);
    let outcome = run_quiet(job, &Cmd::new(&ffmpeg).args(args), "FFmpeg")?;
    check_outcome(job, "FFmpeg", outcome)?;
    if !output.exists() {
        return Err("FFmpeg が出力ファイルを作成できませんでした".into());
    }
    progress(100.0);
    Ok(())
}

// --- ImageMagick -----------------------------------------------------------

fn magick_convert(
    job: &Job,
    tools: &Tools,
    to: &str,
    input: &Path,
    output: &Path,
    progress: &mut dyn FnMut(f64),
) -> Result<(), String> {
    let magick = tools.require("magick")?;
    let mut args: Vec<String> = vec![pstr(input)];
    match to {
        "pdf" => {
            args.extend(
                ["-density", "150", "-quality", "90"]
                    .iter()
                    .map(|s| s.to_string()),
            );
        }
        "jpg" => {
            args.extend(
                [
                    "-background",
                    "white",
                    "-alpha",
                    "remove",
                    "-alpha",
                    "off",
                    "-quality",
                    "92",
                ]
                .iter()
                .map(|s| s.to_string()),
            );
        }
        "webp" | "avif" | "heic" => {
            args.extend(["-quality", "92"].iter().map(|s| s.to_string()));
        }
        "ico" => {
            args.extend(
                ["-define", "icon:auto-resize=256,128,64,48,32,16"]
                    .iter()
                    .map(|s| s.to_string()),
            );
        }
        "gif" => {
            args.extend(["-layers", "optimize"].iter().map(|s| s.to_string()));
        }
        "tiff" => {
            args.extend(["-compress", "lzw"].iter().map(|s| s.to_string()));
        }
        _ => {}
    }
    args.push(pstr(output));

    let cmd = Cmd::new(&magick).args(args);
    progress(5.0);
    let outcome = run_quiet(job, &cmd, "ImageMagick")?;
    check_outcome(job, "ImageMagick", outcome)?;
    if !output.exists() {
        return Err(
            "ImageMagick が出力ファイルを作成できませんでした（PDF ポリシーやデリゲートの設定を確認してください）"
                .into(),
        );
    }
    progress(100.0);
    Ok(())
}

// --- PDF tools -------------------------------------------------------------

fn exec_pdf_to_image(
    job: &Job,
    tools: &Tools,
    to: &str,
    input: &Path,
    output: &Path,
    temp_dir: &Path,
    progress: &mut dyn FnMut(f64),
) -> Result<(), String> {
    let pdftoppm = tools.require("pdftoppm")?;
    let direct = match to {
        "png" => Some("-png"),
        "jpg" => Some("-jpeg"),
        "tiff" => Some("-tiff"),
        _ => None,
    };
    let stem = temp_dir.join(format!("pdf-{}", Uuid::new_v4()));
    let stem_name = stem
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "pdf-page".to_string());

    progress(3.0);
    match direct {
        Some(flag) => {
            let cmd = Cmd::new(&pdftoppm)
                .args(["-f", "1", "-l", "1", "-r", "150", flag])
                .arg(pstr(input))
                .arg(pstr(&stem));
            let outcome = run_quiet(job, &cmd, "pdftoppm")?;
            check_outcome(job, "pdftoppm", outcome)?;
            let produced = find_produced(temp_dir, &stem_name, to)?;
            move_file(&produced, output)?;
            progress(100.0);
        }
        None => {
            // Rasterize to PNG, then let ImageMagick write the exotic target.
            let cmd = Cmd::new(&pdftoppm)
                .args(["-f", "1", "-l", "1", "-r", "150", "-png"])
                .arg(pstr(input))
                .arg(pstr(&stem));
            let outcome = run_quiet(job, &cmd, "pdftoppm")?;
            check_outcome(job, "pdftoppm", outcome)?;
            let produced = find_produced(temp_dir, &stem_name, "png")?;
            progress(40.0);
            let mut magick_progress = |p: f64| progress(40.0 + p.clamp(0.0, 100.0) * 0.6);
            magick_convert(job, tools, to, &produced, output, &mut magick_progress)?;
            progress(100.0);
        }
    }
    Ok(())
}

fn exec_pdf_to_text(
    job: &Job,
    tools: &Tools,
    input: &Path,
    output: &Path,
    progress: &mut dyn FnMut(f64),
) -> Result<(), String> {
    let pdftotext = tools.require("pdftotext")?;
    progress(5.0);

    let cmd = Cmd::new(&pdftotext)
        .arg("-layout")
        .arg(pstr(input))
        .arg(pstr(output));
    let outcome = run_quiet(job, &cmd, "pdftotext")?;
    if outcome.cancelled || job.is_cancelled() {
        return Err(CANCELLED.to_string());
    }
    if outcome.success() && output.exists() {
        progress(100.0);
        return Ok(());
    }
    log_tail(job, "pdftotext", &outcome);
    let _ = fs::remove_file(output);

    job.log("pdftotext: レイアウト保持なしで再試行します…");
    let retry = Cmd::new(&pdftotext).arg(pstr(input)).arg(pstr(output));
    let outcome = run_quiet(job, &retry, "pdftotext")?;
    check_outcome(job, "pdftotext", outcome)?;
    if !output.exists() {
        return Err("pdftotext が出力ファイルを作成できませんでした".into());
    }
    progress(100.0);
    Ok(())
}

// --- LibreOffice -----------------------------------------------------------

fn file_uri(path: &Path) -> String {
    #[cfg(windows)]
    {
        format!("file:///{}", path.to_string_lossy().replace('\\', "/"))
    }
    #[cfg(not(windows))]
    {
        format!("file://{}", path.to_string_lossy())
    }
}

fn exec_libreoffice(
    job: &Job,
    tools: &Tools,
    to: &str,
    input: &Path,
    output: &Path,
    ctx: &ExecCtx<'_>,
    progress: &mut dyn FnMut(f64),
) -> Result<(), String> {
    let libreoffice = tools.require("libreoffice")?;
    let outdir = ctx.temp_dir.join(format!("lo-{}", Uuid::new_v4()));
    fs::create_dir_all(&outdir)
        .map_err(|e| format!("LibreOffice の作業フォルダを作成できません: {e}"))?;

    // A dedicated profile avoids "LibreOffice is already running" failures
    // when the user has the desktop suite open.
    let cmd = Cmd::new(&libreoffice)
        .arg("--headless")
        .arg("--norestore")
        .arg("--invisible")
        .arg(format!(
            "-env:UserInstallation={}",
            file_uri(ctx.profile_dir)
        ))
        .arg("--convert-to")
        .arg(to)
        .arg("--outdir")
        .arg(pstr(&outdir))
        .arg(pstr(input));

    progress(3.0);
    let outcome = run_quiet(job, &cmd, "LibreOffice")?;
    check_outcome(job, "LibreOffice", outcome)?;
    progress(80.0);

    let produced = newest_with_ext(&outdir, to).ok_or_else(|| {
        format!(
            "LibreOffice が {to} ファイルを作成できませんでした。出力形式が未対応の可能性があります。"
        )
    })?;
    move_file(&produced, output)?;
    progress(100.0);
    Ok(())
}

// --- Pandoc ----------------------------------------------------------------

fn build_pandoc_cmd(
    pandoc: &Path,
    to: &str,
    input: &Path,
    output: &Path,
    embed_resources: bool,
) -> Cmd {
    let mut cmd = Cmd::new(pandoc)
        .arg(pstr(input))
        .arg("-o")
        .arg(pstr(output));
    match to {
        "html" => {
            cmd = cmd.arg("--standalone");
            if embed_resources {
                cmd = cmd.arg("--embed-resources");
            }
        }
        "txt" => {
            cmd = cmd.arg("-t").arg("plain");
        }
        _ => {}
    }
    cmd
}

fn exec_pandoc(
    job: &Job,
    tools: &Tools,
    to: &str,
    input: &Path,
    output: &Path,
    progress: &mut dyn FnMut(f64),
) -> Result<(), String> {
    let pandoc = tools.require("pandoc")?;
    progress(5.0);

    let cmd = build_pandoc_cmd(&pandoc, to, input, output, true);
    let outcome = run_quiet(job, &cmd, "Pandoc")?;
    if outcome.cancelled || job.is_cancelled() {
        return Err(CANCELLED.to_string());
    }

    if !outcome.success() && to == "html" {
        // Older Pandoc versions lack --embed-resources; retry with a plain
        // standalone document rather than failing the item.
        log_tail(job, "Pandoc", &outcome);
        let _ = fs::remove_file(output);
        job.log("Pandoc: リソース埋め込みなしで再試行します…");
        let retry = build_pandoc_cmd(&pandoc, to, input, output, false);
        let outcome = run_quiet(job, &retry, "Pandoc")?;
        check_outcome(job, "Pandoc", outcome)?;
    } else {
        check_outcome(job, "Pandoc", outcome)?;
    }

    if !output.exists() {
        return Err("Pandoc が出力ファイルを作成できませんでした".into());
    }
    progress(100.0);
    Ok(())
}

// --- 7-Zip -----------------------------------------------------------------

fn seven_type(to: &str) -> Option<&'static str> {
    match to {
        "zip" => Some("zip"),
        "7z" => Some("7z"),
        "tar" => Some("tar"),
        "gz" => Some("gzip"),
        "xz" => Some("xz"),
        "bz2" => Some("bzip2"),
        _ => None,
    }
}

fn exec_archive(
    job: &Job,
    tools: &Tools,
    to: &str,
    input: &Path,
    output: &Path,
    temp_dir: &Path,
    progress: &mut dyn FnMut(f64),
) -> Result<(), String> {
    let sevenzip = tools.require("sevenzip")?;
    let archive_type = seven_type(to).ok_or_else(|| format!("7-Zip では {to} を作成できません"))?;
    let extract = temp_dir.join(format!("extract-{}", Uuid::new_v4()));
    fs::create_dir_all(&extract).map_err(|e| format!("展開用フォルダを作成できません: {e}"))?;

    progress(3.0);
    let cmd = Cmd::new(&sevenzip)
        .arg("x")
        .arg("-y")
        .arg(format!("-o{}", pstr(&extract)))
        .arg(pstr(input));
    let outcome = run_quiet(job, &cmd, "7-Zip")?;
    check_outcome(job, "7-Zip", outcome)?;
    progress(35.0);

    let mut entries: Vec<String> = fs::read_dir(&extract)
        .map_err(|e| format!("展開結果を読み取れません: {e}"))?
        .flatten()
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()
                .map(|s| s.to_string())
                .or_else(|| Some(entry.file_name().to_string_lossy().to_string()))
        })
        .collect();
    entries.sort();
    if entries.is_empty() {
        return Err("アーカイブが空です".into());
    }

    if matches!(to, "gz" | "xz" | "bz2") {
        if entries.len() != 1 || !extract.join(&entries[0]).is_file() {
            return Err(format!(
                "複数のファイルを含むアーカイブは {} に変換できません。TAR などを使用してください。",
                to.to_uppercase()
            ));
        }
        // Compress the single extracted file.
        let cmd = Cmd::new(&sevenzip)
            .cwd(extract.clone())
            .arg("a")
            .arg(format!("-t{archive_type}"))
            .arg("-y")
            .arg(pstr(output))
            .arg(entries[0].clone());
        let outcome = run_quiet(job, &cmd, "7-Zip")?;
        check_outcome(job, "7-Zip", outcome)?;
    } else {
        // Add every top-level entry explicitly: no shell globbing, and names
        // with spaces or unicode stay intact.
        let mut cmd = Cmd::new(&sevenzip)
            .cwd(extract.clone())
            .arg("a")
            .arg(format!("-t{archive_type}"))
            .arg("-y")
            .arg(pstr(output));
        for entry in &entries {
            cmd = cmd.arg(entry.clone());
        }
        let outcome = run_quiet(job, &cmd, "7-Zip")?;
        check_outcome(job, "7-Zip", outcome)?;
    }

    if !output.exists() {
        return Err("7-Zip が出力アーカイブを作成できませんでした".into());
    }
    progress(100.0);
    Ok(())
}

// ---------------------------------------------------------------------------
// 6. Filesystem helpers
// ---------------------------------------------------------------------------

fn sanitize_component(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.trim_matches('_').is_empty() {
        "job".to_string()
    } else {
        cleaned
    }
}

fn normalize_abs(path: &Path) -> PathBuf {
    if let Ok(canonical) = fs::canonicalize(path) {
        return canonical;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => {
            if let Ok(canonical_parent) = fs::canonicalize(parent) {
                return canonical_parent.join(name);
            }
            path.to_path_buf()
        }
        _ => path.to_path_buf(),
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    normalize_abs(a) == normalize_abs(b)
}

/// Picks `stem.ext` (or `stem (N).ext`) without ever selecting the input path.
fn choose_output_path(dir: &Path, stem: &str, ext: &str, overwrite: bool, input: &Path) -> PathBuf {
    let stem = if stem.trim().is_empty() {
        "output"
    } else {
        stem
    };
    let mut counter = 0usize;
    loop {
        let candidate = if counter == 0 {
            dir.join(format!("{stem}.{ext}"))
        } else {
            dir.join(format!("{stem} ({counter}).{ext}"))
        };
        let usable = !same_file(&candidate, input) && !candidate.is_dir();
        if usable && (overwrite || !candidate.exists()) {
            return candidate;
        }
        counter += 1;
        if counter > 10_000 {
            // Practically unreachable; still avoid an infinite loop.
            return dir.join(format!("{stem} ({counter}).{ext}"));
        }
    }
}

fn copy_file(src: &Path, dst: &Path) -> Result<(), String> {
    if same_file(src, dst) {
        return Err("入力と出力が同じパスになるためコピーできません".into());
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("出力フォルダを作成できません ({}): {e}", parent.display()))?;
    }
    if dst.is_dir() {
        return Err(format!(
            "出力先に同名のフォルダがあります: {}",
            dst.display()
        ));
    }
    if dst.exists() {
        fs::remove_file(dst).map_err(|e| format!("既存の出力ファイルを削除できません: {e}"))?;
    }
    fs::copy(src, dst).map_err(|e| format!("ファイルをコピーできません: {e}"))?;
    Ok(())
}

fn move_file(src: &Path, dst: &Path) -> Result<(), String> {
    if same_file(src, dst) {
        return Err("入力と出力が同じパスになるため移動できません".into());
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("出力フォルダを作成できません ({}): {e}", parent.display()))?;
    }
    if dst.is_dir() {
        return Err(format!(
            "出力先に同名のフォルダがあります: {}",
            dst.display()
        ));
    }
    if dst.exists() {
        fs::remove_file(dst).map_err(|e| format!("既存の出力ファイルを削除できません: {e}"))?;
    }
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Different filesystems (common: /tmp -> home directory).
            fs::copy(src, dst).map_err(|e| format!("一時ファイルを出力先へ移動できません: {e}"))?;
            let _ = fs::remove_file(src);
            Ok(())
        }
    }
}

fn find_produced(dir: &Path, prefix: &str, ext: &str) -> Result<PathBuf, String> {
    let mut matches: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with(prefix) {
                continue;
            }
            let ext_matches = path
                .extension()
                .map(|e| e.to_string_lossy().eq_ignore_ascii_case(ext))
                .unwrap_or(false);
            if ext_matches {
                matches.push(path);
            }
        }
    }
    matches.sort();
    matches
        .into_iter()
        .next()
        .ok_or_else(|| format!("pdftoppm の出力ファイルが見つかりませんでした ({ext})"))
}

fn newest_with_ext(dir: &Path, ext: &str) -> Option<PathBuf> {
    let mut best: Option<(SystemTime, PathBuf)> = None;
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext_matches = path
            .extension()
            .map(|e| e.to_string_lossy().eq_ignore_ascii_case(ext))
            .unwrap_or(false);
        if !ext_matches {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        if best.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
            best = Some((modified, path));
        }
    }
    best.map(|(_, path)| path)
}

// ---------------------------------------------------------------------------
// 7. Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("utilbox-convert-test-{}", Uuid::new_v4()));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn plan(from: &str, to: &str) -> Vec<Op> {
        plan_for(from, to).unwrap_or_else(|| panic!("plan_for({from}, {to}) は None を返しました"))
    }

    #[test]
    fn list_formats_has_unique_ids_and_expected_entries() {
        let formats = list_formats();
        assert!(formats.len() >= 40, "形式が少なすぎます: {}", formats.len());

        let mut ids = HashSet::new();
        let valid_categories = ["image", "audio", "video", "document", "archive"];
        for f in &formats {
            assert!(!f.id.is_empty(), "空の ID があります");
            assert!(ids.insert(f.id.clone()), "ID が重複しています: {}", f.id);
            assert!(!f.label.is_empty(), "{} のラベルが空です", f.id);
            assert!(!f.ext.is_empty(), "{} の拡張子が空です", f.id);
            assert!(
                valid_categories.contains(&f.category.as_str()),
                "{} のカテゴリが不正です: {}",
                f.id,
                f.category
            );
            assert_eq!(f.id, f.id.to_lowercase());
            assert_eq!(f.ext, f.ext.to_lowercase());
        }

        let svg = formats.iter().find(|f| f.id == "svg").unwrap();
        assert!(svg.can_input);
        assert!(!svg.can_output, "SVG は入力のみのはずです");
        assert!(svg.note.is_some());

        let pdf = formats.iter().find(|f| f.id == "pdf").unwrap();
        assert_eq!(pdf.category, "document");
        assert!(pdf.note.is_some());
    }

    #[test]
    fn detect_file_normalizes_aliases() {
        let dir = TestDir::new();
        let cases = [
            ("photo.JPEG", "jpg", Some("jpg"), "image"),
            ("scan.tif", "tiff", Some("tiff"), "image"),
            ("movie.mpg", "mp4", Some("mp4"), "video"),
            ("clip.MPEG", "mp4", Some("mp4"), "video"),
            ("audio.oga", "ogg", Some("ogg"), "audio"),
            ("page.htm", "html", Some("html"), "document"),
            ("notes.markdown", "md", Some("md"), "document"),
            ("archive.bzip2", "bz2", Some("bz2"), "archive"),
            ("voice.aif", "aiff", Some("aiff"), "audio"),
            ("cover.weba", "webm", Some("webm"), "video"),
        ];
        for (name, ext, id, category) in cases {
            let path = dir.path().join(name);
            fs::write(&path, b"test").unwrap();
            let detected = detect_file(&path.display().to_string());
            assert_eq!(detected.ext, ext, "extension mismatch for {name}");
            assert_eq!(detected.format_id.as_deref(), id, "format id for {name}");
            assert_eq!(detected.category, category, "category for {name}");
            assert_eq!(detected.size, 4);
            assert_eq!(detected.name, name);
        }
    }

    #[test]
    fn detect_file_handles_unknown_and_missing_extensions() {
        let dir = TestDir::new();
        let unknown = dir.path().join("data.xyz");
        fs::write(&unknown, b"abc").unwrap();
        let detected = detect_file(&unknown.display().to_string());
        assert_eq!(detected.category, "other");
        assert_eq!(detected.format_id, None);
        assert_eq!(detected.ext, "xyz");

        let noext = dir.path().join("README");
        fs::write(&noext, b"abc").unwrap();
        let detected = detect_file(&noext.display().to_string());
        assert_eq!(detected.ext, "");
        assert_eq!(detected.format_id, None);

        let missing = detect_file(&dir.path().join("missing.png").display().to_string());
        assert_eq!(missing.size, 0);
        assert_eq!(missing.format_id.as_deref(), Some("png"));
    }

    #[test]
    fn planner_image_and_pdf_conversions() {
        assert_eq!(plan("png", "jpg"), vec![magick("jpg")]);
        assert_eq!(plan("PNG", "JPG"), vec![magick("jpg")]);
        assert_eq!(plan("jpeg", "png"), vec![magick("png")]);
        assert_eq!(plan("svg", "png"), vec![magick("png")]);
        assert_eq!(plan("png", "pdf"), vec![magick("pdf")]);
        assert_eq!(plan("pdf", "png"), vec![pdf_image("png")]);
        assert_eq!(plan("pdf", "jpg"), vec![pdf_image("jpg")]);
        assert_eq!(plan("pdf", "tiff"), vec![pdf_image("tiff")]);
        assert_eq!(plan("pdf", "webp"), vec![pdf_image("png"), magick("webp")]);
        assert_eq!(plan("pdf", "txt"), vec![Op::PdfToText]);
        assert_eq!(plan("pdf", "docx"), vec![Op::PdfToText, pandoc("docx")]);
        assert_eq!(plan("pdf", "doc"), vec![Op::PdfToText, lo("doc")]);
    }

    #[test]
    fn still_image_targets_have_encoder_args() {
        for to in ["png", "jpg", "webp", "gif", "bmp", "tiff", "ico", "tga", "ppm"] {
            assert!(still_image_args(to).is_some(), "{to}");
        }
        assert!(still_image_args("pdf").is_none());
        let (_, muxer) = still_image_args("png").unwrap();
        assert_eq!(muxer, Some("image2"));
        let (_, muxer) = still_image_args("gif").unwrap();
        assert_eq!(muxer, None);
    }

    #[test]
    fn planner_media_conversions() {
        assert_eq!(plan("mp4", "mp3"), vec![ff("mp3", false)]);
        assert_eq!(plan("mp4", "gif"), vec![ff("gif", false)]);
        assert_eq!(plan("gif", "mp4"), vec![ff("mp4", false)]);
        assert_eq!(plan("mp4", "webm"), vec![ff("webm", false)]);
        assert_eq!(plan("wav", "flac"), vec![ff("flac", false)]);
        assert_eq!(plan("mp3", "mp4"), vec![ff("mp4", false)]);
        assert_eq!(plan("png", "mp4"), vec![ff("mp4", true)]);
        assert_eq!(plan("jpg", "webm"), vec![ff("webm", true)]);
    }

    #[test]
    fn planner_document_conversions() {
        assert_eq!(plan("docx", "pdf"), vec![lo("pdf")]);
        assert_eq!(plan("doc", "pdf"), vec![lo("pdf")]);
        assert_eq!(plan("md", "html"), vec![pandoc("html")]);
        assert_eq!(plan("md", "docx"), vec![pandoc("docx")]);
        assert_eq!(plan("epub", "docx"), vec![pandoc("docx")]);
        assert_eq!(plan("docx", "txt"), vec![pandoc("txt")]);
        assert_eq!(plan("docx", "md"), vec![pandoc("md")]);
        assert_eq!(plan("html", "docx"), vec![pandoc("docx")]);
        assert_eq!(plan("doc", "docx"), vec![lo("docx")]);
        assert_eq!(plan("rtf", "odt"), vec![lo("odt")]);
        assert_eq!(plan("md", "pdf"), vec![pandoc("html"), lo("pdf")]);
        assert_eq!(plan("epub", "pdf"), vec![pandoc("html"), lo("pdf")]);
        assert_eq!(plan("docx", "png"), vec![lo("pdf"), pdf_image("png")]);
        assert_eq!(
            plan("md", "png"),
            vec![pandoc("html"), lo("pdf"), pdf_image("png")]
        );
        assert_eq!(plan("doc", "epub"), vec![lo("html"), pandoc("epub")]);
    }

    #[test]
    fn planner_archives_and_same_format() {
        assert_eq!(plan("zip", "7z"), vec![repack("7z")]);
        assert_eq!(plan("tar", "zip"), vec![repack("zip")]);
        assert_eq!(plan("gz", "xz"), vec![repack("xz")]);
        assert_eq!(plan("png", "png"), vec![Op::Copy]);
        assert_eq!(plan("jpeg", "jpg"), vec![Op::Copy]);
        assert_eq!(plan("TIF", "tiff"), vec![Op::Copy]);
    }

    #[test]
    fn planner_rejects_unsupported_pairs() {
        assert!(plan_for("png", "mp3").is_none());
        assert!(plan_for("zip", "png").is_none());
        assert!(plan_for("png", "svg").is_none());
        assert!(plan_for("txt", "mp3").is_none());
        assert!(plan_for("mp4", "docx").is_none());
        assert!(plan_for("pdf", "zip").is_none());
        assert!(plan_for("nonsense", "png").is_none());
        assert!(plan_for("png", "nonsense").is_none());
    }

    #[test]
    fn every_supported_plan_is_short_enough() {
        for from in FORMATS.iter().filter(|f| f.can_input) {
            for to in FORMATS.iter().filter(|f| f.can_output) {
                if let Some(ops) = plan_for(from.id, to.id) {
                    assert!(
                        !ops.is_empty() && ops.len() <= MAX_STEPS,
                        "{} → {} の手順数が不正です: {}",
                        from.id,
                        to.id,
                        ops.len()
                    );
                }
            }
        }
    }

    #[test]
    fn output_name_avoids_input_and_existing_files() {
        let dir = TestDir::new();
        fs::write(dir.path().join("photo.png"), b"a").unwrap();
        let input = fs::canonicalize(dir.path().join("photo.png")).unwrap();

        let first = choose_output_path(dir.path(), "photo", "png", false, &input);
        assert_eq!(first.file_name().unwrap(), "photo (1).png");
        fs::write(&first, b"b").unwrap();
        let second = choose_output_path(dir.path(), "photo", "png", false, &input);
        assert_eq!(second.file_name().unwrap(), "photo (2).png");

        // Overwrite mode must still never pick the input path.
        let overwrite = choose_output_path(dir.path(), "photo", "png", true, &input);
        assert_ne!(normalize_abs(&overwrite), input);
    }
}
