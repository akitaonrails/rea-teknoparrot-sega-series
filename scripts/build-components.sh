#!/bin/sh
# Builds the launcher's components from pinned sources into a components folder:
#   lindbergh/  lindbergh-loader (CC BY-NC-SA 4.0, shipped in the Linux package by owner
#               decision 2026-11-16) with any patches/lindbergh applied, in a throwaway
#               ubuntu:22.04 container (docker or podman)
#   wal/        windowsarcadeloader (GPL-3.0) with patches/wal applied: arcade-launcher (Linux),
#               arcade-launcher.exe (its Windows build, linked with LLD), payloads/,
#               systemprofiles/  (needs Rust with the i686/x86_64-pc-windows-gnu targets, MinGW,
#               LLD and mono's mcs); not rebuilt while its commit and patches are unchanged
#   runner/     GE-Proton 11-12
# (every download: packaging/components.pins, fetched and checked by sdk/packaging/pins.sh)
#   winetricks  pinned, checked by SHA-256 (windowsarcadeloader installs native XAudio with it)
#   directx/    Microsoft's DirectX June 2010 redistributable, checked by SHA-256: winetricks'
#               xact verb reads it from here (W_CACHE), so the first start needs no download
#   flycast/    Flycast 2.7 (GPL-2.0), its official Linux AppImage unpacked (usr/: the program
#               and the libraries it carries), checked by SHA-256, with its licence (F355)
#   flycast-win/ Flycast 2.7's official Windows build (one flycast.exe), checked by SHA-256,
#               with its licence: the Windows package's components/flycast
#   art/        the window's original illustrations (the Barlow typefaces are in the program)
# usage: scripts/build-components.sh <components dir> [lindbergh|wal|runner|winetricks|directx|flycast|flycast-win|assets ...]
# LAUNCHER_FEATURES: passed to windowsarcadeloader's build (packages: sdl3/build-from-source-static)
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
out=$(mkdir -p "$1" && cd "$1" && pwd); shift
parts=${*:-lindbergh wal runner winetricks directx flycast flycast-win xemu xemu-win assets}
work="$out/.build"; mkdir -p "$work"

LINDBERGH_REPO=https://github.com/lindbergh-loader/lindbergh-loader
LINDBERGH_COMMIT=0af606d845b70339c335785c0eba68b47b78df3c
WAL_REPO=https://github.com/aderumier/windowsarcadeloader
WAL_COMMIT=4f42c6d86ddaaa9d2e4e8c21e0046bb3027b111d
RUNNER=GE-Proton11-12-x86_64
# the downloads, checked by SHA-256 (packaging/components.pins)
. "$here/sdk/packaging/pins.sh"
fetch() { # pin dir: the file's path
  mkdir -p "$2"
  PIN_FILE="$here/packaging/components.pins" PIN_DIR='' pin_fetch "$1" "$2"
}

checkout() { # repo commit dir
  [ -d "$3/.git" ] || git clone -q "$1" "$3"
  git -C "$3" fetch -q origin "$2" 2>/dev/null || true
  git -C "$3" checkout -q --force "$2"
  git -C "$3" clean -qfdx
}

for part in $parts; do case $part in
lindbergh)
  stamp="$LINDBERGH_COMMIT $(cat "$here"/scripts/lindbergh-in-container.sh "$here"/patches/lindbergh/*.patch 2>/dev/null | sha256sum | cut -c1-16)"
  if [ -f "$out/lindbergh/.stamp" ] && [ "$(cat "$out/lindbergh/.stamp")" = "$stamp" ]; then
    echo "lindbergh: up to date ($stamp)"
    continue
  fi
  checkout "$LINDBERGH_REPO" "$LINDBERGH_COMMIT" "$work/lindbergh-loader"
  for p in "$here"/patches/lindbergh/*.patch; do [ -f "$p" ] && git -C "$work/lindbergh-loader" apply "$p"; done
  engine=$(command -v docker || command -v podman) || { echo "lindbergh: needs docker or podman" >&2; exit 1; }
  rm -rf "$out/lindbergh"; mkdir -p "$out/lindbergh"
  "$engine" run --rm -v "$work/lindbergh-loader:/src:ro" -v "$out/lindbergh:/out" \
    -v "$here/scripts/lindbergh-in-container.sh:/build.sh:ro" -e OWNER="$(id -u):$(id -g)" \
    ubuntu:22.04 sh /build.sh
  cp "$work/lindbergh-loader/libs/libssl.so.0.9.7" "$work/lindbergh-loader/libs/libcrypto.so.0.9.7" \
     "$work/lindbergh-loader/libs/libopenal.so.0" "$out/lindbergh/"
  cp "$work/lindbergh-loader/LICENSE.md" "$out/lindbergh/LICENSE.md"
  f=$(fetch openssl-license "$work/openssl")
  cp "$f" "$out/lindbergh/notices/OpenSSL-0.9.7-LICENSE.txt"
  echo "$stamp" > "$out/lindbergh/.stamp"
  ;;
wal)
  stamp="$WAL_COMMIT $(cat "$here"/patches/wal/*.patch | sha256sum | cut -c1-16)"
  if [ -f "$out/wal/.stamp" ] && [ "$(cat "$out/wal/.stamp")" = "$stamp" ]; then
    echo "wal: up to date ($stamp)"
    continue
  fi
  checkout "$WAL_REPO" "$WAL_COMMIT" "$work/windowsarcadeloader"
  for p in "$here"/patches/wal/*.patch; do git -C "$work/windowsarcadeloader" apply "$p"; done
  (cd "$work/windowsarcadeloader" && ./build.sh)
  # the Windows host build (patches/wal/0003), linked with LLD (GNU ld 2.47 mis-links Rust's
  # Windows import tables)
  (cd "$work/windowsarcadeloader" && \
    CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-C link-arg=-fuse-ld=lld" \
    cargo build --release --target x86_64-pc-windows-gnu -p wal-winhost)
  mkdir -p "$out/wal"
  cp "$work/windowsarcadeloader/dist/arcade-launcher" "$out/wal/"
  cp "$work/windowsarcadeloader/target/x86_64-pc-windows-gnu/release/arcade-launcher.exe" "$out/wal/"
  rm -rf "$out/wal/payloads" "$out/wal/systemprofiles"
  cp -r "$work/windowsarcadeloader/dist/payloads" "$out/wal/payloads"
  cp -r "$work/windowsarcadeloader/systemprofiles" "$out/wal/systemprofiles"
  cp "$work/windowsarcadeloader/LICENSE" "$out/wal/LICENSE"
  echo "$stamp" > "$out/wal/.stamp"
  ;;
runner)
  f=$(fetch runner "$work")
  tar -xzf "$f" -C "$work"
  rm -rf "$out/runner"; mv "$work/$RUNNER/files" "$out/runner"
  ;;
winetricks)
  f=$(fetch winetricks "$work")
  install -m 755 "$f" "$out/winetricks"
  ;;
directx)
  f=$(fetch directx "$work")
  # winetricks' cache layout: <W_CACHE>/directx9/<file>
  mkdir -p "$out/directx/directx9"; cp "$f" "$out/directx/directx9/"
  ;;
flycast)
  f=$(fetch flycast "$work")
  # the AppImage unpacks itself (no FUSE needed); its program finds the libraries beside it
  # (RUNPATH $ORIGIN/../lib)
  rm -rf "$work/squashfs-root"; chmod +x "$f"
  (cd "$work" && "$f" --appimage-extract >/dev/null)
  rm -rf "$out/flycast"; mkdir -p "$out/flycast"
  mv "$work/squashfs-root/usr" "$out/flycast/usr"; rm -rf "$work/squashfs-root"
  l=$(fetch flycast-license "$work/flycast-license")
  cp "$l" "$out/flycast/LICENSE"
  ;;
flycast-win)
  f=$(fetch flycast-win "$work")
  rm -rf "$out/flycast-win"; mkdir -p "$out/flycast-win"
  unzip -q "$f" flycast.exe -d "$out/flycast-win"
  l=$(fetch flycast-license "$work/flycast-license")
  cp "$l" "$out/flycast-win/LICENSE"
  ;;
xemu)
  f=$(fetch xemu "$work")
  rm -rf "$work/squashfs-root"; chmod +x "$f"
  (cd "$work" && "$f" --appimage-extract >/dev/null)
  rm -rf "$out/xemu"; mkdir -p "$out/xemu"
  mv "$work/squashfs-root/usr" "$out/xemu/usr"; rm -rf "$work/squashfs-root"
  l=$(fetch xemu-license "$work/xemu-license")
  cp "$l" "$out/xemu/LICENSE"
  ;;
xemu-win)
  f=$(fetch xemu-win "$work")
  rm -rf "$out/xemu-win"; mkdir -p "$out/xemu-win"
  unzip -q "$f" xemu.exe -d "$out/xemu-win"
  l=$(fetch xemu-license "$work/xemu-license")
  cp "$l" "$out/xemu-win/LICENSE"
  ;;
assets)
  rm -rf "$out/art" "$out/fonts"; cp -r "$here/assets/art" "$out/art"
  ;;
*) echo "unknown part $part" >&2; exit 2;;
esac; done
echo "components ready in $out"
