// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026 Kirsikka (GitHub: Tom039224)

/* Browser preview fixtures.
 *
 * The packaged app always runs inside the Tauri webview, where every command is
 * served by Rust. This module only answers when the webview is absent (a plain
 * `vite dev` in a browser used for layout work), so the screens can be reviewed
 * without a full desktop shell. Values here are fixtures, never product claims.
 */

import type {
  AppInfo,
  CompressRequest,
  CompressResponse,
  ConvertResponse,
  DetectedFile,
  FormatInfo,
  JobHandlers,
  MediaInfo,
  ToolInfo,
  YtInfo,
} from "./api";
import { extOf } from "./format";
import { isAndroid } from "./platform";

const allTools: ToolInfo[] = [
  ["ffmpeg", "FFmpeg", "動画・音声の変換、MP4 圧縮、ストリーム結合", true, "6.1.1"],
  ["ffprobe", "FFprobe", "メディア情報の解析", true, "6.1.1"],
  ["magick", "ImageMagick", "画像の変換（PNG / JPEG / WebP / PDF など）", true, "7.1.1"],
  ["yt-dlp", "yt-dlp", "YouTube などの動画・音声の取得", true, "2025.09.05"],
  ["libreoffice", "LibreOffice", "Office 文書・PDF の変換", true, "24.8"],
  ["pandoc", "Pandoc", "Markdown / HTML / EPUB / DOCX 間の変換", true, "3.1.11"],
  ["sevenzip", "7-Zip", "アーカイブの作成・展開（ZIP / 7z / TAR）", true, "24.07"],
  ["pdftoppm", "Poppler (pdftoppm)", "PDF → 画像", false, null],
  ["pdftotext", "Poppler (pdftotext)", "PDF → テキスト", false, null],
].map(([name, label, purpose, available, version]) => ({
  name: String(name),
  label: String(label),
  purpose: String(purpose),
  available: Boolean(available),
  path: available ? `/usr/bin/${String(name)}` : null,
  version: version === null ? null : String(version),
  bundled: false,
}));

const tools: ToolInfo[] = isAndroid
  ? allTools.filter((tool) => tool.name === "ffmpeg" || tool.name === "ffprobe")
  : allTools;

const FORMATS: [string, string, string, string][] = [
  ["png", "PNG 画像", "png", "image"],
  ["jpg", "JPEG 画像", "jpg", "image"],
  ["webp", "WebP 画像", "webp", "image"],
  ["avif", "AVIF 画像", "avif", "image"],
  ["gif", "GIF 画像", "gif", "image"],
  ["tiff", "TIFF 画像", "tiff", "image"],
  ["bmp", "BMP 画像", "bmp", "image"],
  ["pdf", "PDF 文書", "pdf", "document"],
  ["mp4", "MP4 動画", "mp4", "video"],
  ["mkv", "Matroska 動画", "mkv", "video"],
  ["webm", "WebM 動画", "webm", "video"],
  ["mov", "QuickTime 動画", "mov", "video"],
  ["avi", "AVI 動画", "avi", "video"],
  ["gifv", "アニメーション GIF", "gif", "video"],
  ["mp3", "MP3 音声", "mp3", "audio"],
  ["wav", "WAV 音声", "wav", "audio"],
  ["flac", "FLAC 音声", "flac", "audio"],
  ["m4a", "M4A 音声", "m4a", "audio"],
  ["opus", "Opus 音声", "opus", "audio"],
  ["ogg", "Ogg 音声", "ogg", "audio"],
  ["docx", "Word 文書", "docx", "document"],
  ["xlsx", "Excel ブック", "xlsx", "document"],
  ["pptx", "PowerPoint", "pptx", "document"],
  ["md", "Markdown", "md", "document"],
  ["html", "HTML", "html", "document"],
  ["epub", "EPUB", "epub", "document"],
  ["txt", "テキスト", "txt", "document"],
  ["zip", "ZIP アーカイブ", "zip", "archive"],
  ["7z", "7z アーカイブ", "7z", "archive"],
  ["tar", "TAR アーカイブ", "tar", "archive"],
  ["gz", "GZip アーカイブ", "gz", "archive"],
];

const ANDROID_CATEGORIES = ["image", "audio", "video"];
const ANDROID_HIDDEN_IDS = ["avif"];

const formats: FormatInfo[] = FORMATS.filter(
  ([id, , , category]) =>
    !isAndroid || (ANDROID_CATEGORIES.includes(category) && !ANDROID_HIDDEN_IDS.includes(id)),
).map(([id, label, ext, category], index) => ({
  id,
  label,
  ext,
  category: category as FormatInfo["category"],
  canInput: index % 7 !== 3,
  canOutput: true,
  note:
    id === "pdf"
      ? "画像から PDF への変換は 1 ページ目のみです"
      : id === "avif"
        ? "変換に時間がかかる場合があります"
        : null,
}));

const info: AppInfo = {
  version: "0.1.0",
  platform: isAndroid ? "android" : "linux",
  arch: isAndroid ? "aarch64" : "x86_64",
  defaultOutputDir: isAndroid
    ? "/data/user/0/dev.utilbox.app/cache/outputs"
    : "/home/user/Downloads/UtilBox",
  outputDirIsCustom: false,
  downloadsDir: "/home/user/Downloads",
  tempDir: "/tmp",
};

const categoryOf = (ext: string): DetectedFile["category"] => {
  if (["png", "jpg", "jpeg", "webp", "avif", "gif", "tiff", "bmp"].includes(ext)) return "image";
  if (["mp4", "mkv", "webm", "mov", "avi"].includes(ext)) return "video";
  if (["mp3", "wav", "flac", "m4a", "opus", "ogg"].includes(ext)) return "audio";
  if (["zip", "7z", "tar", "gz", "rar"].includes(ext)) return "archive";
  if (["pdf", "docx", "xlsx", "pptx", "md", "html", "epub", "txt"].includes(ext)) return "document";
  return "other";
};

function detectFiles(paths: string[]): DetectedFile[] {
  return paths.map((path, index) => {
    const name = path.split(/[\\/]/).pop() ?? path;
    const ext = extOf(name);
    const category = categoryOf(ext);
    return {
      path,
      name,
      ext,
      size: (index + 2) * 18_432_128,
      category,
      formatId: formats.find((format) => format.ext === ext)?.id ?? null,
    };
  });
}

function media(path: string): MediaInfo {
  return {
    path,
    size: 184_320_128,
    durationSec: 243.4,
    width: 1920,
    height: 1080,
    videoCodec: "h264",
    audioCodec: "aac",
    bitrateKbps: 6400,
    fps: 30,
    container: "mp4",
  };
}

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export async function mockInvoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  await wait(120);
  switch (cmd) {
    case "list_tools":
      return tools as unknown as T;
    case "list_formats":
      return formats as unknown as T;
    case "app_info":
      return info as unknown as T;
    case "set_default_output_dir": {
      const dir = args?.dir as string | null;
      info.defaultOutputDir = dir?.trim()
        ? dir.trim()
        : "/home/user/Downloads/UtilBox";
      info.outputDirIsCustom = Boolean(dir?.trim());
      return info as unknown as T;
    }
    case "detect_files":
      return detectFiles((args?.paths as string[]) ?? []) as unknown as T;
    case "probe_media":
      return media(String(args?.path ?? "clip.mp4")) as unknown as T;
    case "convert_files": {
      const req = args?.req as { inputs: string[]; target: string };
      const results = req.inputs.map((input, index) => {
        const name = input.split(/[\\/]/).pop() ?? input;
        const ok = index !== 2;
        return {
          input,
          name,
          output: ok
            ? `${isAndroid ? "Download/UtilBox" : info.defaultOutputDir}/${name.replace(/\.[^.]+$/, "")}.${req.target}`
            : null,
          ok,
          message: ok ? "変換しました" : "この形式には未対応です",
          sizeBefore: 18_432_128,
          sizeAfter: ok ? 6_291_456 : 0,
          elapsedMs: 1840 + index * 620,
        };
      });
      const failed = results.filter((item) => !item.ok).length;
      return {
        results,
        succeeded: results.length - failed,
        failed,
      } as unknown as T;
    }
    case "compress_video": {
      const req = args?.req as CompressRequest;
      return {
        output: isAndroid
          ? `Download/UtilBox/${req.output.split(/[\\/]/).pop()}`
          : req.output,
        sizeBefore: 184_320_128,
        sizeAfter: 52_428_800,
        elapsedMs: 84_200,
        media: media(req.input),
        encoder: "libx264",
      } as unknown as CompressResponse as unknown as T;
    }
    case "yt_probe":
      return {
        id: "sample",
        title: "サンプル動画のタイトル",
        uploader: "サンプルチャンネル",
        durationSec: 612,
        thumbnail: null,
        webpageUrl: String(args?.url ?? ""),
        live: false,
        viewCount: 1_204_388,
        description: null,
      } as unknown as YtInfo as unknown as T;
    case "download_media":
      return {
        file: `${info.downloadsDir}/sample.mp4`,
        title: "サンプル動画のタイトル",
        size: 88_080_384,
        elapsedMs: 42_000,
        compression: null,
      } as unknown as T;
    case "update_ytdlp":
      return { info: tools[3], message: "最新版です" } as unknown as T;
    case "pick_files":
      if (args?.kind === "video") {
        return [
          { path: "/cache/imports/2/VID_0315.mp4", name: "VID_0315.mp4", size: 184_320_128 },
        ] as unknown as T;
      }
      return [
        { path: "/cache/imports/1/IMG_2041.jpg", name: "IMG_2041.jpg", size: 4_218_000 },
        { path: "/cache/imports/2/VID_0315.mp4", name: "VID_0315.mp4", size: 184_320_128 },
      ] as unknown as T;
    case "open_output":
      return undefined as unknown as T;
    case "cancel_job":
      return true as unknown as T;
    default:
      throw new Error(`preview: unknown command ${cmd}`);
  }
}

/** Progress events for the browser preview, shaped like the Rust job events. */
export function mockListenJob(jobId: string, handlers: JobHandlers): () => void {
  let percent = 0;
  const timer = window.setInterval(() => {
    percent = Math.min(100, percent + 17);
    handlers.onProgress?.({
      jobId,
      tool: "ffmpeg",
      phase: percent < 100 ? "変換中" : "完了",
      percent,
      message: percent < 100 ? `${percent}% 処理しました` : "処理が完了しました",
      detail: percent < 100 ? "ffmpeg -i input.mp4 -c:v libx264 output.mp4" : null,
    });
    if (percent >= 100) window.clearInterval(timer);
  }, 280);
  return () => window.clearInterval(timer);
}
