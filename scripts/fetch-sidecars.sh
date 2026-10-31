#!/usr/bin/env bash
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026 Kirsikka (GitHub: Tom039224)

# Fetch the portable sidecars used by UtilBox and place them in src-tauri/binaries/.
# Network access is only needed for this setup step.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT/src-tauri/binaries"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$BIN"

need() { command -v "$1" >/dev/null 2>&1 || { echo "error: $1 is required" >&2; exit 1; }; }
need curl
need tar

OS="$(uname -s)"
ARCH="$(uname -m)"

echo "==> yt-dlp"
case "$OS-$ARCH" in
  Linux-x86_64)   YT_URL="https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux" ;;
  Linux-aarch64)  YT_URL="https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux_aarch64" ;;
  Darwin-*)       YT_URL="https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos" ;;
  *) echo "未対応の OS/arch: $OS-$ARCH" >&2; exit 1 ;;
esac
curl -fL --retry 3 -o "$BIN/yt-dlp" "$YT_URL"
chmod +x "$BIN/yt-dlp"
"$BIN/yt-dlp" --version

echo "==> Pandoc"
PANDOC_VER="3.12.1"
case "$OS-$ARCH" in
  Linux-x86_64)  PANDOC_PKG="pandoc-$PANDOC_VER-linux-amd64.tar.gz" ;;
  Linux-aarch64) PANDOC_PKG="pandoc-$PANDOC_VER-linux-arm64.tar.gz" ;;
  Darwin-arm64)  PANDOC_PKG="pandoc-$PANDOC_VER-arm64-macOS.zip" ;;
  Darwin-x86_64) PANDOC_PKG="pandoc-$PANDOC_VER-x86_64-macOS.zip" ;;
  *) echo "未対応の OS/arch: $OS-$ARCH" >&2; exit 1 ;;
esac
curl -fL --retry 3 -o "$TMP/pandoc.pkg" \
  "https://github.com/jgm/pandoc/releases/download/$PANDOC_VER/$PANDOC_PKG"
case "$PANDOC_PKG" in
  *.tar.gz) tar xzf "$TMP/pandoc.pkg" -C "$TMP" ;;
  *.zip)    (cd "$TMP" && unzip -q pandoc.pkg) ;;
esac
PANDOC_BIN="$(find "$TMP" -type f -name pandoc -perm -u+x | head -n1)"
if [[ -z "$PANDOC_BIN" ]]; then echo "pandoc binary not found" >&2; exit 1; fi
cp "$PANDOC_BIN" "$BIN/pandoc"
chmod +x "$BIN/pandoc"
"$BIN/pandoc" --version | head -n1

echo "==> FFmpeg (static, with SVT-AV1 / libaom / NVENC)"
if [[ "$OS" == "Linux" && "$ARCH" == "x86_64" ]]; then
  curl -fL --retry 3 -o "$TMP/ffmpeg.tar.xz" \
    "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linux64-gpl.tar.xz"
  tar xf "$TMP/ffmpeg.tar.xz" -C "$TMP"
  SRC="$(find "$TMP" -maxdepth 1 -type d -name 'ffmpeg-*-gpl' | head -n1)/bin"
  cp "$SRC/ffmpeg" "$SRC/ffprobe" "$BIN/"
  chmod +x "$BIN/ffmpeg" "$BIN/ffprobe"
  "$BIN/ffmpeg" -version | head -n1
else
  echo "注意: このプラットフォームでは FFmpeg を自動取得しません。"
  echo "      PATH 上の ffmpeg / ffprobe を使用するか、手動で $BIN に配置してください。"
fi

echo
echo "完了: $BIN"
ls -lh "$BIN"
