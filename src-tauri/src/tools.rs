// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026 Kirsikka (GitHub: Tom039224)

use crate::types::ToolInfo;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager};

#[derive(Clone)]
pub struct Tool {
    pub name: &'static str,
    pub label: &'static str,
    pub purpose: &'static str,
    pub path: Option<PathBuf>,
}

#[derive(Clone)]
pub struct Tools {
    map: HashMap<&'static str, Tool>,
    user_bin: PathBuf,
    bundled_dirs: Vec<PathBuf>,
}

const DEFS: &[(&str, &str, &str, &[&str])] = &[
    (
        "ffmpeg",
        "FFmpeg",
        "動画・音声の変換、MP4 圧縮、ストリーム結合",
        &["ffmpeg"],
    ),
    ("ffprobe", "FFprobe", "メディア情報の解析", &["ffprobe"]),
    (
        "magick",
        "ImageMagick",
        "画像の変換（PNG / JPEG / WebP / PDF など）",
        &["magick", "convert"],
    ),
    (
        "yt-dlp",
        "yt-dlp",
        "YouTube などの動画・音声の取得",
        &["yt-dlp"],
    ),
    (
        "libreoffice",
        "LibreOffice",
        "Office 文書・PDF の変換",
        &["libreoffice", "soffice"],
    ),
    (
        "pandoc",
        "Pandoc",
        "Markdown / HTML / EPUB / DOCX 間の変換",
        &["pandoc"],
    ),
    (
        "sevenzip",
        "7-Zip",
        "アーカイブの作成・展開（ZIP / 7z / TAR）",
        &["7z", "7zz", "7za"],
    ),
    (
        "pdftoppm",
        "Poppler (pdftoppm)",
        "PDF → 画像",
        &["pdftoppm"],
    ),
    (
        "pdftotext",
        "Poppler (pdftotext)",
        "PDF → テキスト",
        &["pdftotext"],
    ),
];

/// Tools that can exist on this platform. Android ships FFmpeg only.
fn supported(name: &str) -> bool {
    !cfg!(target_os = "android") || matches!(name, "ffmpeg" | "ffprobe")
}

/// Directory Android extracted this app's native libraries to.
///
/// Found through the memory map of our own library, which avoids a JNI call.
#[cfg(target_os = "android")]
fn native_lib_dir() -> Option<PathBuf> {
    let maps = std::fs::read_to_string("/proc/self/maps").ok()?;
    maps.lines()
        .filter_map(|line| line.split_whitespace().last())
        .find(|path| path.ends_with("/libutilbox_lib.so"))
        .and_then(|path| Path::new(path).parent().map(Path::to_path_buf))
}

#[cfg(not(target_os = "android"))]
fn native_lib_dir() -> Option<PathBuf> {
    None
}

impl Tools {
    pub fn discover(app: &AppHandle) -> Self {
        let resource_bin = app
            .path()
            .resource_dir()
            .ok()
            .map(|d| d.join("binaries"))
            .unwrap_or_default();
        let user_bin = app
            .path()
            .app_data_dir()
            .ok()
            .map(|d| d.join("bin"))
            .unwrap_or_else(|| PathBuf::from("."));
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_default();
        let dev_bin = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries");

        let mut bundled_dirs = vec![resource_bin, dev_bin];
        if !exe_dir.as_os_str().is_empty() {
            bundled_dirs.push(exe_dir.join("binaries"));
            bundled_dirs.push(exe_dir);
        }
        bundled_dirs.extend(native_lib_dir());

        Self::from_dirs(user_bin, bundled_dirs)
    }

    /// Tool discovery without a Tauri app handle (CLI harness / tests).
    pub fn headless() -> Self {
        let user_bin = std::env::temp_dir().join("utilbox-bin");
        let dev_bin = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries");
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_default();
        let mut bundled_dirs = vec![dev_bin];
        if !exe_dir.as_os_str().is_empty() {
            bundled_dirs.push(exe_dir.join("binaries"));
            bundled_dirs.push(exe_dir);
        }
        Self::from_dirs(user_bin, bundled_dirs)
    }

    fn from_dirs(user_bin: PathBuf, bundled_dirs: Vec<PathBuf>) -> Self {
        if !user_bin.exists() {
            let _ = std::fs::create_dir_all(&user_bin);
        }

        let mut map = HashMap::new();
        for (name, label, purpose, candidates) in DEFS.iter().filter(|d| supported(d.0)) {
            let path = resolve_tool(candidates, &user_bin, &bundled_dirs);
            map.insert(
                *name,
                Tool {
                    name,
                    label,
                    purpose,
                    path,
                },
            );
        }

        Self {
            map,
            user_bin,
            bundled_dirs,
        }
    }

    /// Prefer a user-updatable copy (e.g. updated yt-dlp) over the bundled one.
    pub fn require(&self, name: &str) -> Result<PathBuf, String> {
        let candidates: Vec<&str> = match name {
            "sevenzip" => vec!["7z", "7zz", "7za"],
            "magick" => vec!["magick", "convert"],
            "libreoffice" => vec!["libreoffice", "soffice"],
            _ => vec![name],
        };
        if let Some(p) = find_user_tool(&candidates, &self.user_bin) {
            return Ok(p);
        }
        if let Some(tool) = self.map.get(name) {
            if let Some(path) = &tool.path {
                return Ok(path.clone());
            }
        }
        Err(missing_tool_message(name))
    }

    pub fn optional(&self, name: &str) -> Option<PathBuf> {
        self.require(name).ok()
    }

    pub fn path(&self, name: &str) -> Option<PathBuf> {
        self.map.get(name).and_then(|t| t.path.clone())
    }

    pub fn dir_of(&self, name: &str) -> Option<PathBuf> {
        self.path(name)
            .or_else(|| self.require(name).ok())
            .and_then(|p| p.parent().map(Path::to_path_buf))
    }

    pub fn list(&self) -> Vec<ToolInfo> {
        let mut out: Vec<ToolInfo> = DEFS
            .iter()
            .filter(|(name, _, _, _)| supported(name))
            .map(|(name, label, purpose, _)| {
                let path = self.require(name).ok();
                let version = path.as_ref().and_then(|p| version_of(name, p));
                let bundled = path
                    .as_ref()
                    .map(|p| {
                        self.bundled_dirs
                            .iter()
                            .any(|d| !d.as_os_str().is_empty() && p.starts_with(d))
                    })
                    .unwrap_or(false);
                ToolInfo {
                    name: (*name).to_string(),
                    label: (*label).to_string(),
                    purpose: (*purpose).to_string(),
                    available: path.is_some(),
                    path: path.map(|p| p.display().to_string()),
                    version,
                    bundled,
                }
            })
            .collect();
        out.sort_by(|a, b| {
            b.available
                .cmp(&a.available)
                .then_with(|| a.label.cmp(&b.label))
        });
        out
    }

    pub fn user_bin(&self) -> &Path {
        &self.user_bin
    }

    pub fn available_count(&self) -> (usize, usize) {
        let mut ok = 0;
        let mut total = 0;
        for (name, _, _, _) in DEFS.iter().filter(|d| supported(d.0)) {
            total += 1;
            if self.require(name).is_ok() {
                ok += 1;
            }
        }
        (ok, total)
    }
}

pub fn find_in_path(names: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in names {
            let candidate = dir.join(name);
            if is_executable(&candidate) {
                return Some(candidate);
            }
            #[cfg(windows)]
            {
                let exe = dir.join(format!("{name}.exe"));
                if is_executable(&exe) {
                    return Some(exe);
                }
            }
        }
    }
    None
}

fn resolve_tool(names: &[&str], user_bin: &Path, bundled_dirs: &[PathBuf]) -> Option<PathBuf> {
    // 1) user-updatable tools
    if let Some(p) = find_user_tool(names, user_bin) {
        return Some(p);
    }
    // 2) bundled sidecars
    for dir in bundled_dirs {
        if dir.as_os_str().is_empty() {
            continue;
        }
        for name in names {
            for candidate in tool_filenames(name) {
                let p = dir.join(candidate);
                if is_executable(&p) {
                    return Some(p);
                }
            }
        }
    }
    // 3) system PATH
    find_in_path(names)
}

fn find_user_tool(names: &[&str], user_bin: &Path) -> Option<PathBuf> {
    for name in names {
        for candidate in tool_filenames(name) {
            let p = user_bin.join(candidate);
            if is_executable(&p) {
                return Some(p);
            }
        }
    }
    None
}

fn tool_filenames(name: &str) -> Vec<String> {
    #[allow(unused_mut)]
    let mut v = vec![name.to_string()];
    #[cfg(windows)]
    v.push(format!("{name}.exe"));
    // Android only executes files from nativeLibraryDir, where binaries are
    // packaged as lib<name>.so.
    #[cfg(target_os = "android")]
    v.push(format!("lib{name}.so"));
    v
}

fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn version_of(name: &str, path: &Path) -> Option<String> {
    let mut cmd = Command::new(path);
    match name {
        "sevenzip" => {
            cmd.arg("i");
        }
        "libreoffice" => {
            cmd.arg("--version");
        }
        _ => {
            cmd.arg("--version");
        }
    }
    let out = cmd.output().ok()?;
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    if text.trim().is_empty() {
        text = String::from_utf8_lossy(&out.stderr).to_string();
    }
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("WARNING"))?;
    let mut short: String = line.chars().take(72).collect();
    if line.chars().count() > 72 {
        short.push('…');
    }
    Some(short)
}

fn missing_tool_message(name: &str) -> String {
    let label = DEFS
        .iter()
        .find(|(n, _, _, _)| *n == name)
        .map(|(_, label, _, _)| *label)
        .unwrap_or(name);
    format!("{label} が見つかりません。src-tauri/binaries に配置するか、PATH に追加してください。")
}
