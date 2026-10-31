import type { Category } from "./api";

export function formatBytes(bytes: number): string {
  if (!bytes || bytes < 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / Math.pow(1024, i);
  const digits = value >= 100 || i === 0 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(digits)} ${units[i]}`;
}

export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "0:00";
  const total = Math.round(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  return `${m}:${String(s).padStart(2, "0")}`;
}

export function formatCount(count: number | null): string {
  if (count === null || !Number.isFinite(count)) return "—";
  if (count >= 100_000_000) return `${(count / 100_000_000).toFixed(1)}億`;
  if (count >= 10_000) return `${(count / 10_000).toFixed(1)}万`;
  return count.toLocaleString("ja-JP");
}

export function formatMs(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} 秒`;
  const m = Math.floor(ms / 60_000);
  const s = Math.round((ms % 60_000) / 1000);
  return `${m} 分 ${s} 秒`;
}

export function baseName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

export function stripExt(name: string): string {
  const idx = name.lastIndexOf(".");
  return idx > 0 ? name.slice(0, idx) : name;
}

export function extOf(path: string): string {
  const name = baseName(path);
  const idx = name.lastIndexOf(".");
  return idx > 0 ? name.slice(idx + 1).toLowerCase() : "";
}

export function dirName(path: string): string {
  const idx = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return idx > 0 ? path.slice(0, idx) : "";
}

export function joinPath(dir: string, file: string): string {
  if (!dir) return file;
  const sep = dir.includes("\\") && !dir.includes("/") ? "\\" : "/";
  return `${dir.replace(/[\\/]+$/, "")}${sep}${file}`;
}

export function categoryLabel(category: Category): string {
  switch (category) {
    case "image":
      return "画像";
    case "audio":
      return "音声";
    case "video":
      return "動画";
    case "document":
      return "文書";
    case "archive":
      return "アーカイブ";
    default:
      return "その他";
  }
}

export const CATEGORY_ORDER: Category[] = [
  "image",
  "document",
  "audio",
  "video",
  "archive",
];

export function suggestTarget(category: Category, ext: string): string {
  switch (category) {
    case "image":
      return ext === "png" ? "jpg" : "png";
    case "audio":
      return "mp3";
    case "video":
      return "mp4";
    case "document":
      return "pdf";
    case "archive":
      return ext === "zip" ? "7z" : "zip";
    default:
      return "pdf";
  }
}

export function reductionPercent(before: number, after: number): number {
  if (!before) return 0;
  return Math.round(((before - after) / before) * 100);
}

/** Media extensions accepted by the compressor. */
export const VIDEO_FILTERS = {
  name: "動画ファイル",
  extensions: ["mp4", "mkv", "mov", "webm", "avi", "m4v", "ts", "flv", "wmv"],
};
