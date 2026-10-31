# Licensing

Copyright (c) 2026 Kirsikka (GitHub: Tom039224)

UtilBox consists of three kinds of material. Every source file carries an
`SPDX-License-Identifier` header, and the full texts are in [`LICENSES/`](LICENSES/)
and [`LICENSE`](LICENSE).

| Part | License |
| --- | --- |
| Everything not listed below | **BSD-3-Clause** ([`LICENSE`](LICENSE)) |
| The file-conversion feature (see below) | **GPL-2.0-only** ([`LICENSES/GPL-2.0-only.txt`](LICENSES/GPL-2.0-only.txt)) |
| Bundled third-party programs (FFmpeg and its libraries, desktop sidecars) | Their own licenses, see [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md) |

## The GPL-2.0-only part

The file-conversion feature was designed after
[p2r3/convert](https://github.com/p2r3/convert) (GPL-2.0), which is the idea of
converting between any media type by chaining tools. It does not contain code
from that project, but it is licensed under the same license as a matter of
principle. The files are:

- `src-tauri/src/convert.rs` — format registry, conversion planner, executors
- `src/pages/ConvertPage.tsx`
- `src/components/FormatPicker.tsx`
- `src/styles/convert.css`

The upstream license text is GPL version 2 without an "or later" grant, so
these files are **GPL-2.0-only**, not GPL-2.0-or-later.

A built application links those files together with the BSD-3-Clause files.
BSD-3-Clause is compatible with the GPL, so **a distributed UtilBox binary is
governed by GPL-2.0-only as a whole**. The BSD-3-Clause files can still be
taken and reused on their own under BSD-3-Clause.

## Bundled FFmpeg (Android APK and desktop packages)

The packaged applications ship FFmpeg as a **separate executable** that the app
starts as a child process (`libffmpeg.so` / `libffprobe.so` on Android). The
build is configured with `--enable-gpl --enable-version3` and includes x264 and
x265, so those executables are licensed under **GPL-3.0-or-later**.

Because they are separate programs that only talk to the app through command
line arguments and pipes, they do not change the license of the UtilBox code.
The GPL does require that whoever receives the binaries can get the matching
source. For every release that includes them, the **corresponding source** is
attached to the same GitHub Release:

- `utilbox-ffmpeg-android-source-<version>.tar.xz` — exact FFmpeg and library
  sources, the build script and the `configure` line used.

To rebuild the executables yourself, run `scripts/build-android-ffmpeg.sh`.

## Contributing

Contributions are accepted under the license of the files they change:
GPL-2.0-only for the list above, BSD-3-Clause otherwise.
