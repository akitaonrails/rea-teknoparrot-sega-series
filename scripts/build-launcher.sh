#!/bin/sh
# Builds the launcher.
#
#   scripts/build-launcher.sh linux     target/release/sega-launcher
#   scripts/build-launcher.sh windows   target/x86_64-pc-windows-gnu/release/sega-launcher.exe
#                                       (+ dist/sdl2/SDL2.dll)
#   scripts/build-launcher.sh sdl2      only fetch the pinned SDL2 MinGW package into dist/sdl2
#                                       (for the Windows-target tests)
#
# The Windows build needs MinGW-w64 and LLD (.cargo/config.toml).
set -eu

root=$(cd "$(dirname "$0")/.." && pwd -P)
what=${1:-linux}
die() { echo "build-launcher: $*" >&2; exit 1; }
target_dir=${CARGO_TARGET_DIR:-$root/target}

pin() { sed -n "s/^$1=//p" "$root/packaging/sdl2.pin"; }

fetch_sdl2() {
	url=$(pin url)
	sum=$(pin sha256)
	dir="$root/dist/sdl2"
	archive="$dir/$(basename "$url")"
	mkdir -p "$dir"
	if [ ! -f "$archive" ] || ! echo "$sum  $archive" | sha256sum -c --status; then
		curl -fsSL --retry 3 -o "$archive.part" "$url"
		echo "$sum  $archive.part" | sha256sum -c --status || die "SDL2 download does not match its pin"
		mv "$archive.part" "$archive"
	fi
	[ -d "$dir/$(pin dir)" ] || tar xzf "$archive" -C "$dir"
	cp "$dir/$(pin dir)/bin/SDL2.dll" "$dir/SDL2.dll"
}

case $what in
sdl2)
	fetch_sdl2
	;;
linux)
	cargo build --release --locked -p sega-launcher --manifest-path "$root/Cargo.toml"
	echo "build-launcher: $target_dir/release/sega-launcher"
	;;
windows)
	command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1 || die "x86_64-w64-mingw32-gcc not found (MinGW-w64)"
	fetch_sdl2
	sdl="$root/dist/sdl2/$(pin dir)/lib"
	# lld as in .cargo/config.toml (this variable replaces its rustflags), plus SDL2
	export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-C link-arg=-fuse-ld=lld -L $sdl"
	cargo build --release --locked -p sega-launcher --target x86_64-pc-windows-gnu \
		--manifest-path "$root/Cargo.toml"
	echo "build-launcher: $target_dir/x86_64-pc-windows-gnu/release/sega-launcher.exe"
	;;
*)
	die "unknown target '$what' (linux, windows or sdl2)"
	;;
esac
