// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026 Kirsikka (GitHub: Tom039224)

import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./api";

declare global {
  interface Window {
    __FORCE_ANDROID__?: boolean;
  }
}

function detectAndroid(): boolean {
  try {
    if (typeof window === "undefined") return false;
    if (window.__FORCE_ANDROID__) return true;
    if (!isTauri && new URLSearchParams(window.location.search).get("android") === "1") {
      return true;
    }
    return /Android/i.test(navigator.userAgent);
  } catch {
    return false;
  }
}

/** True in the Android build (or a browser preview opened with `?android=1`). */
export const isAndroid = detectAndroid();

export interface PickedFile {
  path: string;
  name: string;
  size: number;
}

export interface PickOptions {
  multiple: boolean;
  /** Android only: which documents the system picker offers. */
  kind: "video" | "media" | "any";
  /** Desktop only. */
  filters?: { name: string; extensions: string[] }[];
  title?: string;
}

/** Opens the platform file picker and returns readable file paths. */
export async function pickFiles(opts: PickOptions): Promise<string[]> {
  if (isAndroid) {
    const files = isTauri
      ? await invoke<PickedFile[]>("pick_files", { multiple: opts.multiple, kind: opts.kind })
      : await (await import("./mock")).mockInvoke<PickedFile[]>("pick_files", {
          multiple: opts.multiple,
          kind: opts.kind,
        });
    return files.map((file) => file.path);
  }
  const { open } = await import("@tauri-apps/plugin-dialog");
  const selected = await open({
    multiple: opts.multiple,
    directory: false,
    filters: opts.filters,
    title: opts.title,
  });
  if (!selected) return [];
  return Array.isArray(selected) ? selected : [selected];
}

/** Desktop only: choose a folder. Returns null when cancelled. */
export async function pickFolder(title: string): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const dir = await open({ directory: true, multiple: false, title });
  return typeof dir === "string" ? dir : null;
}

/** Desktop only: choose where to save a file. */
export async function pickSavePath(
  defaultPath: string,
  title: string,
  filters: { name: string; extensions: string[] }[],
): Promise<string | null> {
  const { save } = await import("@tauri-apps/plugin-dialog");
  return (await save({ defaultPath, filters, title })) ?? null;
}

/** Opens a finished output file in its default app. */
export async function openOutput(path: string): Promise<void> {
  if (isAndroid) {
    if (isTauri) await invoke("open_output", { path });
    return;
  }
  const { openPath } = await import("@tauri-apps/plugin-opener");
  await openPath(path);
}

/** Shows a file (or files) in the system file manager. No-op on Android. */
export async function revealOutput(path: string | string[]): Promise<void> {
  if (isAndroid) return;
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(path);
}

export async function openExternalUrl(url: string): Promise<void> {
  if (isAndroid) return;
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}
