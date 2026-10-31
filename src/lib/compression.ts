// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026 Kirsikka (GitHub: Tom039224)

export type Codec = "h264" | "h265" | "av1" | "av1_gpu";

export const QUALITY_PRESETS: Record<
  Codec,
  { label: string; crf: number; hint: string }[]
> = {
  h264: [
    { label: "高画質", crf: 20, hint: "見た目を優先" },
    { label: "標準", crf: 24, hint: "バランス" },
    { label: "小さく", crf: 28, hint: "容量を削減" },
    { label: "最小", crf: 32, hint: "とにかく軽く" },
  ],
  h265: [
    { label: "高画質", crf: 24, hint: "見た目を優先" },
    { label: "標準", crf: 28, hint: "バランス" },
    { label: "小さく", crf: 32, hint: "容量を削減" },
    { label: "最小", crf: 36, hint: "とにかく軽く" },
  ],
  av1: [
    { label: "高画質", crf: 26, hint: "見た目を優先" },
    { label: "標準", crf: 32, hint: "バランス" },
    { label: "小さく", crf: 38, hint: "容量を削減" },
    { label: "最小", crf: 45, hint: "とにかく軽く" },
  ],
  // NVENC's cq: 40 ≈ VMAF 95, 44 ≈ 94, 48 ≈ 92 (1080p60 sample). Beyond ~46 quality drops fast.
  av1_gpu: [
    { label: "高画質", crf: 32, hint: "見た目を優先" },
    { label: "標準", crf: 38, hint: "バランス" },
    { label: "小さく", crf: 42, hint: "容量を削減" },
    { label: "最小", crf: 46, hint: "これ以上は画質が急に落ちます" },
  ],
};

export const CRF_RANGES: Record<Codec, { min: number; max: number }> = {
  h264: { min: 18, max: 34 },
  h265: { min: 18, max: 38 },
  av1: { min: 20, max: 50 },
  av1_gpu: { min: 24, max: 50 },
};

export const DEFAULT_CRF: Record<Codec, number> = {
  h264: 24,
  h265: 28,
  av1: 32,
  av1_gpu: 38,
};

export const CODEC_NOTES: Record<Codec, string> = {
  h264: "H.264 / 互換性重視",
  h265: "H.265 / 高圧縮",
  av1: "AV1 / 最小サイズ（低速）",
  av1_gpu: "AV1 / GPU で高速（同画質で CPU の AV1 より大きめ）",
};

export const CODEC_LABELS: Record<Codec, string> = {
  h264: "H.264",
  h265: "H.265",
  av1: "AV1",
  av1_gpu: "AV1 GPU",
};

export const RESOLUTIONS = [
  { label: "元のまま", value: 0 },
  { label: "1080p", value: 1080 },
  { label: "720p", value: 720 },
  { label: "480p", value: 480 },
];

export const AUDIO_OPTIONS = [
  { label: "192 kbps", value: "192" },
  { label: "128 kbps", value: "128" },
  { label: "96 kbps", value: "96" },
  { label: "音声なし", value: "none" },
];
