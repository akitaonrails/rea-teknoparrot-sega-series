#!/bin/sh
# Builds the launcher's components from pinned sources into a components folder:
#   lindbergh/  lindbergh-loader (CC BY-NC-SA 4.0: built here on the player's machine, never
#               shipped in our packages) with any patches/lindbergh applied, in a throwaway
#               ubuntu:22.04 container (docker or podman)
#   wal/        windowsarcadeloader (GPL-3.0) with patches/wal applied: arcade-launcher (Linux),
#               arcade-launcher.exe (its Windows build, linked with LLD), payloads/,
#               systemprofiles/  (needs Rust with the i686/x86_64-pc-windows-gnu targets, MinGW,
#               LLD and mono's mcs); not rebuilt while its commit and patches are unchanged
#   runner/     GE-Proton 11-12, checked by SHA-256
#   winetricks  pinned, checked by SHA-256 (windowsarcadeloader installs native XAudio with it)
#   art/ fonts/ the window's original illustrations and the Barlow typefaces (OFL)
# usage: scripts/build-components.sh <components dir> [lindbergh|wal|runner|winetricks|assets ...]
# LAUNCHER_FEATURES: passed to windowsarcadeloader's build (packages: sdl3/build-from-source-static)
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
out=$(mkdir -p "$1" && cd "$1" && pwd); shift
parts=${*:-lindbergh wal runner winetricks assets}
work="$out/.build"; mkdir -p "$work"

LINDBERGH_REPO=https://github.com/lindbergh-loader/lindbergh-loader
LINDBERGH_COMMIT=0af606d845b70339c335785c0eba68b47b78df3c
WAL_REPO=https://github.com/aderumier/windowsarcadeloader
WAL_COMMIT=4f42c6d86ddaaa9d2e4e8c21e0046bb3027b111d
RUNNER=GE-Proton11-12-x86_64
RUNNER_URL=https://github.com/GloriousEggroll/proton-ge-custom/releases/download/GE-Proton11-12/$RUNNER.tar.gz
RUNNER_SHA256=0595159e417c3dbbfb7cc3ae0d3ed15a617cc607aa23ef756ea79029d7af5d70
WINETRICKS_URL=https://raw.githubusercontent.com/Winetricks/winetricks/20260125/src/winetricks
WINETRICKS_SHA256=431f82fc74000e6c864409f1d8fb495d696c03928808e3e8acffc45179312a7b

checkout() { # repo commit dir
  [ -d "$3/.git" ] || git clone -q "$1" "$3"
  git -C "$3" fetch -q origin "$2" 2>/dev/null || true
  git -C "$3" checkout -q --force "$2"
  git -C "$3" clean -qfdx
}

for part in $parts; do case $part in
lindbergh)
  checkout "$LINDBERGH_REPO" "$LINDBERGH_COMMIT" "$work/lindbergh-loader"
  for p in "$here"/patches/lindbergh/*.patch; do [ -f "$p" ] && git -C "$work/lindbergh-loader" apply "$p"; done
  engine=$(command -v docker || command -v podman) || { echo "lindbergh: needs docker or podman" >&2; exit 1; }
  mkdir -p "$out/lindbergh"
  "$engine" run --rm -v "$work/lindbergh-loader:/src:ro" -v "$out/lindbergh:/out" \
    -v "$here/scripts/lindbergh-in-container.sh:/build.sh:ro" -e OWNER="$(id -u):$(id -g)" \
    ubuntu:22.04 sh /build.sh
  cp "$work/lindbergh-loader/libs/libssl.so.0.9.7" "$work/lindbergh-loader/libs/libcrypto.so.0.9.7" \
     "$work/lindbergh-loader/libs/libopenal.so.0" "$out/lindbergh/"
  cp "$work/lindbergh-loader/LICENSE.md" "$out/lindbergh/LICENSE.md"
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
  f="$work/$RUNNER.tar.gz"
  [ -f "$f" ] || curl -fsSL -o "$f" "$RUNNER_URL"
  echo "$RUNNER_SHA256  $f" | sha256sum -c -
  tar -xzf "$f" -C "$work"
  rm -rf "$out/runner"; mv "$work/$RUNNER/files" "$out/runner"
  ;;
winetricks)
  f="$work/winetricks"
  [ -f "$f" ] || curl -fsSL -o "$f" "$WINETRICKS_URL"
  echo "$WINETRICKS_SHA256  $f" | sha256sum -c -
  install -m 755 "$f" "$out/winetricks"
  ;;
assets)
  rm -rf "$out/art" "$out/fonts"; cp -r "$here/assets/art" "$out/art"; cp -r "$here/assets/fonts" "$out/fonts"
  ;;
*) echo "unknown part $part" >&2; exit 2;;
esac; done
echo "components ready in $out"
