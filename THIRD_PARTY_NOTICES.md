# Third-party notices

UtilBox uses or ships the following software. Each keeps its own license.

## Shipped inside the Android APK

| Component | Version | License | Source |
| --- | --- | --- | --- |
| FFmpeg / FFprobe (built `--enable-gpl --enable-version3`) | 8.1.3 | GPL-3.0-or-later | https://ffmpeg.org/releases/ffmpeg-8.1.3.tar.xz |
| x264 | git `0480cb0` | GPL-2.0-or-later | https://code.videolan.org/videolan/x264 |
| x265 | 4.1 (`1d117be`) | GPL-2.0-or-later | https://bitbucket.org/multicoreware/x265_git |
| libaom | 3.15.3 | BSD-2-Clause + AOM Patent License 1.0 | https://aomedia.googlesource.com/aom |
| SVT-AV1 | 4.1.0 | BSD-3-Clause-Clear + AOM Patent License 1.0 | https://gitlab.com/AOMediaCodec/SVT-AV1 |
| dav1d | 1.5.1 | BSD-2-Clause | https://code.videolan.org/videolan/dav1d |
| LAME | 3.100 | LGPL-2.0-or-later | https://lame.sourceforge.io |
| Opus | 1.5.2 | BSD-3-Clause | https://opus-codec.org |
| libogg / libvorbis | 1.3.5 / 1.3.7 | BSD-3-Clause | https://xiph.org |
| libvpx | 1.15.0 | BSD-3-Clause | https://chromium.googlesource.com/webm/libvpx |
| libwebp | 1.5.0 | BSD-3-Clause | https://chromium.googlesource.com/webm/libwebp |
| zlib | 1.3.1 | Zlib | https://zlib.net |

The corresponding source for the GPL components is attached to each GitHub
Release (see [`LICENSING.md`](LICENSING.md)).

## Application framework and libraries

- Tauri 2 and its plugins — MIT OR Apache-2.0
- Rust crates in `src-tauri/Cargo.lock` — mostly MIT OR Apache-2.0
- React, React DOM — MIT
- lucide-react — ISC
- AndroidX / Material Components — Apache-2.0

## Desktop packages (fetched by `scripts/fetch-sidecars.sh`, not stored in this repository)

| Tool | License |
| --- | --- |
| FFmpeg / FFprobe (BtbN GPL build) | GPL-3.0-or-later |
| Pandoc | GPL-2.0-or-later |
| yt-dlp | Unlicense |
| ImageMagick, LibreOffice, 7-Zip, Poppler | used from the system if installed; see each project |
