// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026 Kirsikka (GitHub: Tom039224)

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** True inside the packaged Tauri webview; false in a plain browser preview. */
export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) return invoke<T>(cmd, args);
  const { mockInvoke } = await import("./mock");
  return mockInvoke<T>(cmd, args);
}

/* ------------------------------------------------------------------ */
/* Types (mirrors src-tauri/src/types.rs, camelCased)                  */
/* ------------------------------------------------------------------ */

export type Category =
  | "image"
  | "audio"
  | "video"
  | "document"
  | "archive"
  | "other";

export interface ToolInfo {
  name: string;
  label: string;
  purpose: string;
  available: boolean;
  path: string | null;
  version: string | null;
  bundled: boolean;
}

export interface AppInfo {
  version: string;
  platform: string;
  arch: string;
  defaultOutputDir: string;
  outputDirIsCustom: boolean;
  downloadsDir: string;
  tempDir: string;
}

export interface FormatInfo {
  id: string;
  label: string;
  ext: string;
  category: Category;
  canInput: boolean;
  canOutput: boolean;
  note: string | null;
}

export interface DetectedFile {
  path: string;
  name: string;
  ext: string;
  size: number;
  category: Category;
  formatId: string | null;
}

export interface ConvertRequest {
  inputs: string[];
  target: string;
  outputDir: string;
  overwrite: boolean;
}

export interface ConvertItemResult {
  input: string;
  name: string;
  output: string | null;
  ok: boolean;
  message: string;
  sizeBefore: number;
  sizeAfter: number;
  elapsedMs: number;
}

export interface ConvertResponse {
  results: ConvertItemResult[];
  succeeded: number;
  failed: number;
}

export interface MediaInfo {
  path: string;
  size: number;
  durationSec: number;
  width: number | null;
  height: number | null;
  videoCodec: string | null;
  audioCodec: string | null;
  bitrateKbps: number | null;
  fps: number | null;
  container: string | null;
}

export interface CompressRequest {
  input: string;
  output: string;
  mode: "crf" | "size";
  crf: number | null;
  targetMb: number | null;
  maxHeight: number | null;
  codec: "h264" | "h265" | "av1" | "av1_gpu";
  audioKbps: number | null;
  removeAudio: boolean;
  preset: string | null;
  audioMode: "encode" | "copy";
}

export interface CompressResponse {
  output: string;
  sizeBefore: number;
  sizeAfter: number;
  elapsedMs: number;
  media: MediaInfo | null;
  encoder: string;
}

export interface YtInfo {
  id: string;
  title: string;
  uploader: string | null;
  durationSec: number;
  thumbnail: string | null;
  webpageUrl: string;
  live: boolean;
  viewCount: number | null;
  description: string | null;
}

export type DownloadCompression = Omit<CompressRequest, "input" | "output">;

export interface DownloadRequest {
  url: string;
  outputDir: string;
  mode: "video" | "audio";
  quality: string;
  audioFormat: "mp3" | "m4a" | "opus" | null;
  embedThumbnail: boolean;
  embedMetadata: boolean;
  cookiesBrowser: string | null;
  compression: DownloadCompression | null;
}

export interface DownloadResponse {
  file: string;
  title: string;
  size: number;
  elapsedMs: number;
  compression: CompressResponse | null;
}

export interface ToolUpdate {
  info: ToolInfo;
  message: string;
}

export interface JobProgress {
  jobId: string;
  tool: string;
  phase: string;
  percent: number;
  message: string;
  detail: string | null;
}

export interface JobLog {
  jobId: string;
  line: string;
}

/* ------------------------------------------------------------------ */
/* Commands                                                            */
/* ------------------------------------------------------------------ */

export const api = {
  listTools: () => call<ToolInfo[]>("list_tools"),
  gpuAv1Available: () => call<boolean>("gpu_av1_available"),
  listFormats: () => call<FormatInfo[]>("list_formats"),
  detectFiles: (paths: string[]) => call<DetectedFile[]>("detect_files", { paths }),
  appInfo: () => call<AppInfo>("app_info"),
  setDefaultOutputDir: (dir: string | null) =>
    call<AppInfo>("set_default_output_dir", { dir }),
  convertFiles: (jobId: string, req: ConvertRequest) =>
    call<ConvertResponse>("convert_files", { jobId, req }),
  probeMedia: (path: string) => call<MediaInfo>("probe_media", { path }),
  compressVideo: (jobId: string, req: CompressRequest) =>
    call<CompressResponse>("compress_video", { jobId, req }),
  ytProbe: (url: string) => call<YtInfo>("yt_probe", { url }),
  downloadMedia: (jobId: string, req: DownloadRequest) =>
    call<DownloadResponse>("download_media", { jobId, req }),
  updateYtdlp: (jobId: string) => call<ToolUpdate>("update_ytdlp", { jobId }),
  cancelJob: (jobId: string) => call<boolean>("cancel_job", { jobId }),
};

/* ------------------------------------------------------------------ */
/* Job event helper                                                    */
/* ------------------------------------------------------------------ */

export interface JobHandlers {
  onProgress?: (p: JobProgress) => void;
  onLog?: (line: string) => void;
}

/** Subscribe to a job's events. Call before invoking the command. */
export async function listenJob(
  jobId: string,
  handlers: JobHandlers,
): Promise<UnlistenFn> {
  if (!isTauri) {
    const { mockListenJob } = await import("./mock");
    return mockListenJob(jobId, handlers);
  }
  const unlisten = await Promise.all([
    listen<JobProgress>("job-progress", (event) => {
      if (event.payload.jobId === jobId) handlers.onProgress?.(event.payload);
    }),
    listen<JobLog>("job-log", (event) => {
      if (event.payload.jobId === jobId) handlers.onLog?.(event.payload.line);
    }),
  ]);
  return () => unlisten.forEach((fn) => fn());
}

export function newJobId(tool: string): string {
  const rand =
    typeof crypto !== "undefined" && "randomUUID" in crypto
      ? crypto.randomUUID()
      : `${Date.now()}-${Math.random().toString(16).slice(2)}`;
  return `${tool}-${rand}`;
}

export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}
