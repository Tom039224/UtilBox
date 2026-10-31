#!/usr/bin/env bash
# Cross-compile static FFmpeg + FFprobe CLI for Android arm64-v8a (API 26) with the NDK.
# Idempotent: downloads are cached in $ROOT/dl, each library leaves a stamp in $ROOT/stamps.
# Output: $ROOT/out/libffmpeg.so and $ROOT/out/libffprobe.so (stripped PIE executables).
# Optional libs (svtav1, dav1d, vorbis, vpx, webp) are skipped on failure; set FORCE=1 to rebuild all.
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ROOT="${ROOT:-$HOME/ffmpeg-android-build}"
NDK="${NDK:-${NDK_HOME:-$HOME/Android/Sdk/ndk/27.2.12479018}}"
API=26
JOBS="${JOBS:-16}"
FFMPEG_VER="${FFMPEG_VER:-8.1.3}"
AOM_TAG=v3.15.3; X265_TAG=4.1; SVT_TAG=v4.1.0; DAV1D_VER=1.5.1
LAME_VER=3.100; OPUS_VER=1.5.2; OGG_VER=1.3.5; VORBIS_VER=1.3.7
VPX_VER=1.15.0; WEBP_VER=1.5.0; ZLIB_VER=1.3.1

TC="$NDK/toolchains/llvm/prebuilt/linux-x86_64"
BIN="$TC/bin"
TRIPLE=aarch64-linux-android
CC="$BIN/${TRIPLE}${API}-clang"; CXX="$BIN/${TRIPLE}${API}-clang++"
AR="$BIN/llvm-ar"; RANLIB="$BIN/llvm-ranlib"; STRIP="$BIN/llvm-strip"
NM="$BIN/llvm-nm"; READELF="$BIN/llvm-readelf"
PREFIX="$ROOT/prefix"; DL="$ROOT/dl"; SRC="$ROOT/src"; BLD="$ROOT/bld"; ST="$ROOT/stamps"; LOG="$ROOT/logs"
mkdir -p "$PREFIX/lib/pkgconfig" "$PREFIX/include" "$DL" "$SRC" "$BLD" "$ST" "$LOG" "$ROOT/out"

CFLAGS_COMMON="-O2 -fPIC -I$PREFIX/include"
LDFLAGS_COMMON="-L$PREFIX/lib -Wl,-z,max-page-size=16384 -Wl,-z,common-page-size=16384"
export PKG_CONFIG_LIBDIR="$PREFIX/lib/pkgconfig" PKG_CONFIG_PATH="" PKG_CONFIG_SYSROOT_DIR=""
export PATH="$BIN:$PATH"
[ "${FORCE:-0}" = 1 ] && rm -rf "$ST"/* && mkdir -p "$ST"

fetch() { # url file
  [ -s "$DL/$2" ] || curl -fL --retry 3 -o "$DL/$2.part" "$1" && mv -f "$DL/$2.part" "$DL/$2" 2>/dev/null; [ -s "$DL/$2" ]
}
unpack() { # file dir
  [ -d "$SRC/$2" ] || { mkdir -p "$SRC/$2.tmp" && tar -xf "$DL/$1" -C "$SRC/$2.tmp" --strip-components=1 && mv "$SRC/$2.tmp" "$SRC/$2"; }
}
gitget() { # url tag dir
  [ -d "$SRC/$3/.git" ] || git clone --depth 1 ${2:+-b "$2"} "$1" "$SRC/$3"
}
# run a step once; $1 name, $2 required|optional, rest = function
step() {
  local name=$1 kind=$2; shift 2
  if [ -f "$ST/$name" ]; then echo "[skip] $name"; return 0; fi
  echo "[build] $name"
  if ( set -e; "$@" ) >"$LOG/$name.log" 2>&1; then touch "$ST/$name"; echo "[ok] $name"; return 0; fi
  echo "[FAIL] $name (see $LOG/$name.log)"
  [ "$kind" = required ] && { tail -30 "$LOG/$name.log"; exit 1; }
  touch "$ST/$name.failed"; return 1
}

CMAKE_COMMON=(-G Ninja -DCMAKE_TOOLCHAIN_FILE="$NDK/build/cmake/android.toolchain.cmake"
  -DANDROID_ABI=arm64-v8a -DANDROID_PLATFORM=android-$API -DCMAKE_BUILD_TYPE=Release
  -DCMAKE_INSTALL_PREFIX="$PREFIX" -DCMAKE_POSITION_INDEPENDENT_CODE=ON -DBUILD_SHARED_LIBS=OFF
  -DCMAKE_POLICY_VERSION_MINIMUM=3.5
  "-DCMAKE_SHARED_LINKER_FLAGS=-Wl,-z,max-page-size=16384"
  "-DCMAKE_EXE_LINKER_FLAGS=-Wl,-z,max-page-size=16384"
  -DCMAKE_FIND_ROOT_PATH="$PREFIX")

AUTOCONF_ENV() { export CC="$CC" CXX="$CXX" AR="$AR" RANLIB="$RANLIB" STRIP="$STRIP" NM="$NM" \
  CFLAGS="$CFLAGS_COMMON" CXXFLAGS="$CFLAGS_COMMON" LDFLAGS="$LDFLAGS_COMMON"; }

# ---------------- libraries ----------------
b_zlib() {
  fetch https://github.com/madler/zlib/releases/download/v$ZLIB_VER/zlib-$ZLIB_VER.tar.xz zlib-$ZLIB_VER.tar.xz
  unpack zlib-$ZLIB_VER.tar.xz zlib; rm -rf "$BLD/zlib"; cp -r "$SRC/zlib" "$BLD/zlib"; cd "$BLD/zlib"
  AUTOCONF_ENV; ./configure --prefix="$PREFIX" --static; make -j$JOBS libz.a; make install
}
b_x264() {
  gitget https://code.videolan.org/videolan/x264.git master x264
  rm -rf "$BLD/x264"; mkdir -p "$BLD/x264"; cd "$BLD/x264"; AUTOCONF_ENV
  "$SRC/x264/configure" --prefix="$PREFIX" --host=aarch64-linux-android --cross-prefix="$BIN/llvm-" \
    --enable-static --enable-pic --disable-cli --disable-opencl --bit-depth=8 \
    --extra-cflags="$CFLAGS_COMMON" --extra-ldflags="$LDFLAGS_COMMON"
  make -j$JOBS; make install
}
b_x265() {
  gitget https://bitbucket.org/multicoreware/x265_git.git $X265_TAG x265
  sed -i 's/cmake_policy(SET CMP0025 OLD)/cmake_policy(SET CMP0025 NEW)/; s/cmake_policy(SET CMP0054 OLD)/cmake_policy(SET CMP0054 NEW)/' "$SRC/x265/source/CMakeLists.txt"
  # asm custom commands call clang++ without --target (would assemble for the host) -> add it
  sed -i 's|ARGS ${ARM_ARGS} ${ASM_FLAGS}|ARGS --target=aarch64-linux-android26 ${ARM_ARGS} ${ASM_FLAGS}|' "$SRC/x265/source/CMakeLists.txt"
  rm -rf "$BLD/x265"; mkdir -p "$BLD/x265"; cd "$BLD/x265"
  cmake "${CMAKE_COMMON[@]}" -DENABLE_SHARED=OFF -DENABLE_CLI=OFF -DHIGH_BIT_DEPTH=OFF -DEXPORT_C_API=ON \
    -DENABLE_ASSEMBLY=${X265_ASM:-ON} -DCROSS_COMPILE_ARM64=ON -DNEON_ANDROID=OFF "$SRC/x265/source"
  ninja -j$JOBS; ninja install
  # Android has no libstdc++; static libc++ is linked by the final ffmpeg link
}
b_aom() {
  gitget https://aomedia.googlesource.com/aom $AOM_TAG aom
  rm -rf "$BLD/aom"; mkdir -p "$BLD/aom"; cd "$BLD/aom"
  cmake "${CMAKE_COMMON[@]}" -DENABLE_TESTS=0 -DENABLE_DOCS=0 -DENABLE_EXAMPLES=0 -DENABLE_TOOLS=0 \
    -DENABLE_TESTDATA=0 -DCONFIG_RUNTIME_CPU_DETECT=1 -DCONFIG_PIC=1 "$SRC/aom"
  ninja -j$JOBS; ninja install
}
b_svtav1() {
  gitget https://gitlab.com/AOMediaCodec/SVT-AV1.git $SVT_TAG svtav1
  rm -rf "$BLD/svtav1"; mkdir -p "$BLD/svtav1"; cd "$BLD/svtav1"
  cmake "${CMAKE_COMMON[@]}" -DBUILD_APPS=OFF -DBUILD_TESTING=OFF -DBUILD_DEC=OFF "$SRC/svtav1"
  ninja -j$JOBS; ninja install
}
b_dav1d() {
  fetch https://code.videolan.org/videolan/dav1d/-/archive/$DAV1D_VER/dav1d-$DAV1D_VER.tar.gz dav1d-$DAV1D_VER.tar.gz
  unpack dav1d-$DAV1D_VER.tar.gz dav1d
  cat >"$BLD/meson-android.txt" <<EOF
[binaries]
c = '$CC'
cpp = '$CXX'
ar = '$AR'
strip = '$STRIP'
pkg-config = 'pkg-config'
[host_machine]
system = 'android'
cpu_family = 'aarch64'
cpu = 'aarch64'
endian = 'little'
[built-in options]
c_args = ['-fPIC']
c_link_args = ['-Wl,-z,max-page-size=16384']
EOF
  rm -rf "$BLD/dav1d"
  meson setup "$BLD/dav1d" "$SRC/dav1d" --cross-file "$BLD/meson-android.txt" --prefix="$PREFIX" --libdir=lib \
    --default-library=static -Denable_tools=false -Denable_tests=false -Dbuildtype=release
  ninja -C "$BLD/dav1d" -j$JOBS; ninja -C "$BLD/dav1d" install
}
b_lame() {
  fetch https://downloads.sourceforge.net/project/lame/lame/$LAME_VER/lame-$LAME_VER.tar.gz lame-$LAME_VER.tar.gz
  unpack lame-$LAME_VER.tar.gz lame; rm -rf "$BLD/lame"; mkdir -p "$BLD/lame"; cd "$BLD/lame"; AUTOCONF_ENV
  "$SRC/lame/configure" --prefix="$PREFIX" --host=aarch64-linux-android --disable-shared --enable-static \
    --with-pic --disable-frontend --disable-decoder --disable-gtktest
  make -j$JOBS; make install
}
b_opus() {
  fetch https://downloads.xiph.org/releases/opus/opus-$OPUS_VER.tar.gz opus-$OPUS_VER.tar.gz
  unpack opus-$OPUS_VER.tar.gz opus; rm -rf "$BLD/opus"; mkdir -p "$BLD/opus"; cd "$BLD/opus"; AUTOCONF_ENV
  "$SRC/opus/configure" --prefix="$PREFIX" --host=aarch64-linux-android --disable-shared --enable-static \
    --with-pic --disable-doc --disable-extra-programs
  make -j$JOBS; make install
}
b_ogg() {
  fetch https://downloads.xiph.org/releases/ogg/libogg-$OGG_VER.tar.xz libogg-$OGG_VER.tar.xz
  unpack libogg-$OGG_VER.tar.xz ogg; rm -rf "$BLD/ogg"; mkdir -p "$BLD/ogg"; cd "$BLD/ogg"; AUTOCONF_ENV
  "$SRC/ogg/configure" --prefix="$PREFIX" --host=aarch64-linux-android --disable-shared --enable-static --with-pic
  make -j$JOBS; make install
}
b_vorbis() {
  fetch https://downloads.xiph.org/releases/vorbis/libvorbis-$VORBIS_VER.tar.xz libvorbis-$VORBIS_VER.tar.xz
  unpack libvorbis-$VORBIS_VER.tar.xz vorbis; rm -rf "$BLD/vorbis"; mkdir -p "$BLD/vorbis"; cd "$BLD/vorbis"; AUTOCONF_ENV
  "$SRC/vorbis/configure" --prefix="$PREFIX" --host=aarch64-linux-android --disable-shared --enable-static \
    --with-pic --disable-examples --disable-docs --disable-oggtest
  make -j$JOBS; make install
}
b_vpx() {
  fetch https://github.com/webmproject/libvpx/archive/refs/tags/v$VPX_VER.tar.gz libvpx-$VPX_VER.tar.gz
  unpack libvpx-$VPX_VER.tar.gz libvpx; rm -rf "$BLD/libvpx"; mkdir -p "$BLD/libvpx"; cd "$BLD/libvpx"
  export CC="$CC" CXX="$CXX" AR="$AR" LD="$CC" NM="$NM" STRIP="$STRIP" AS="$CC" RANLIB="$RANLIB"
  export LDFLAGS="$LDFLAGS_COMMON"
  "$SRC/libvpx/configure" --prefix="$PREFIX" --target=arm64-android-gcc --enable-pic --enable-static --disable-shared \
    --disable-examples --disable-tools --disable-docs --disable-unit-tests --enable-vp8 --enable-vp9 \
    --enable-runtime-cpu-detect --enable-multithread --disable-install-bins --disable-install-srcs \
    --extra-cflags="-fPIC -O2"
  make -j$JOBS; make install
}
b_webp() {
  fetch https://github.com/webmproject/libwebp/archive/refs/tags/v$WEBP_VER.tar.gz libwebp-$WEBP_VER.tar.gz
  unpack libwebp-$WEBP_VER.tar.gz libwebp; rm -rf "$BLD/libwebp"; mkdir -p "$BLD/libwebp"; cd "$BLD/libwebp"
  cmake "${CMAKE_COMMON[@]}" -DWEBP_BUILD_ANIM_UTILS=OFF -DWEBP_BUILD_CWEBP=OFF -DWEBP_BUILD_DWEBP=OFF \
    -DWEBP_BUILD_GIF2WEBP=OFF -DWEBP_BUILD_IMG2WEBP=OFF -DWEBP_BUILD_VWEBP=OFF -DWEBP_BUILD_WEBPINFO=OFF \
    -DWEBP_BUILD_WEBPMUX=OFF -DWEBP_BUILD_EXTRAS=OFF -DWEBP_BUILD_LIBWEBPMUX=ON "$SRC/libwebp"
  ninja -j$JOBS; ninja install
}

step zlib   required b_zlib
step x264   required b_x264
if ! step x265 optional b_x265; then
  echo "x265 with asm failed; retrying without asm"; rm -f "$ST/x265.failed"
  X265_ASM=OFF step x265 required b_x265
fi
step aom    required b_aom
step lame   required b_lame
step opus   required b_opus
step svtav1 optional b_svtav1 || true
step dav1d  optional b_dav1d  || true
step ogg    optional b_ogg    || true
[ -f "$ST/ogg" ] && { step vorbis optional b_vorbis || true; }
step vpx    optional b_vpx    || true
step webp   optional b_webp   || true

# ---------------- FFmpeg ----------------
# x265.pc: Android has no libstdc++; link libc++ statically (no libc++_shared.so dependency)
sed -i -E 's/-lstdc\+\+/-lc++/g; s/ -lpthread//g; s/-l-l:libunwind.a//g; s/-lc\+\+( |$)/-l:libc++_static.a -l:libc++abi.a\1/g' "$PREFIX/lib/pkgconfig/x265.pc"
fetch https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VER.tar.xz ffmpeg-$FFMPEG_VER.tar.xz || { echo "ffmpeg download failed"; exit 1; }
unpack ffmpeg-$FFMPEG_VER.tar.xz ffmpeg-$FFMPEG_VER

EXT=(--enable-libx264 --enable-libx265 --enable-libaom --enable-libmp3lame --enable-libopus --enable-zlib)
[ -f "$ST/svtav1" ] && EXT+=(--enable-libsvtav1)
[ -f "$ST/dav1d" ] && EXT+=(--enable-libdav1d)
[ -f "$ST/vorbis" ] && EXT+=(--enable-libvorbis)
[ -f "$ST/vpx" ] && EXT+=(--enable-libvpx)
[ -f "$ST/webp" ] && EXT+=(--enable-libwebp)

PAGE="-Wl,-z,max-page-size=16384 -Wl,-z,common-page-size=16384"
CONF=(--prefix="$PREFIX/ffmpeg" --enable-cross-compile --target-os=android --arch=aarch64 --cpu=armv8-a
  --cc="$CC" --cxx="$CXX" --ar="$AR" --ranlib="$RANLIB" --strip="$STRIP" --nm="$NM"
  --pkg-config=pkg-config --pkg-config-flags=--static
  --enable-gpl --enable-version3 --enable-pic --enable-static --disable-shared
  --disable-doc --disable-debug --enable-ffmpeg --enable-ffprobe --disable-ffplay
  --disable-autodetect --disable-network --disable-vulkan --disable-jni --disable-mediacodec --disable-indev=android_camera
  --disable-symver
  "${EXT[@]}"
  --extra-cflags="-I$PREFIX/include -fPIC"
  --extra-ldflags="-L$PREFIX/lib -pie $PAGE"
  --extra-libs="-lm")

build_ffmpeg() {
  rm -rf "$BLD/ffmpeg"; mkdir -p "$BLD/ffmpeg"; cd "$BLD/ffmpeg"
  echo "CONFIGURE: ${CONF[*]}" > "$ROOT/configure-line.txt"
  "$SRC/ffmpeg-$FFMPEG_VER/configure" "${CONF[@]}" || { tail -40 ffbuild/config.log; exit 1; }
  make -j$JOBS
}
rm -f "$ST/ffmpeg"
step ffmpeg required build_ffmpeg

cp "$BLD/ffmpeg/ffmpeg" "$ROOT/out/libffmpeg.so"; cp "$BLD/ffmpeg/ffprobe" "$ROOT/out/libffprobe.so"
"$STRIP" --strip-all "$ROOT/out/libffmpeg.so" "$ROOT/out/libffprobe.so"
for f in libffmpeg.so libffprobe.so; do
  echo "== $f"; file "$ROOT/out/$f"; ls -l "$ROOT/out/$f"
  "$READELF" -d "$ROOT/out/$f" | grep NEEDED; "$READELF" -lW "$ROOT/out/$f" | grep -E 'LOAD|INTERP|interpreter'
done
echo "Dropped optional libs:"; ls "$ST"/*.failed 2>/dev/null || echo " (none)"

DEST="$REPO/src-tauri/android-libs/arm64-v8a"
mkdir -p "$DEST"
cp "$ROOT/out/libffmpeg.so" "$ROOT/out/libffprobe.so" "$DEST/"
echo "Installed into $DEST"
