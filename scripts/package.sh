#!/bin/sh
# Assembles a release package from the built launcher and the components.
#
#   scripts/package.sh linux     dist/pkg/SegaRacingLauncher-<version>-linux-x86_64.tar.gz
#   scripts/package.sh windows   dist/pkg/SegaRacingLauncher-<version>-windows-x86_64.zip
#
# Built pieces (scripts/build-launcher.sh, the release workflow):
#   linux    dist/linux/sega-launcher, dist/linux/lib/libSDL2-2.0.so.0
#   windows  target/x86_64-pc-windows-gnu/release/sega-launcher.exe, dist/sdl2/SDL2.dll
# The components are built or fetched once into dist/components with
# scripts/build-components.sh (windowsarcadeloader from source with
# patches/wal, both its launchers; GE-Proton and winetricks by SHA-256;
# the art and fonts), then copied into each package.
#
# lindbergh-loader (CC BY-NC-SA) is never packaged: the Linux package
# carries the script and its container recipe, and the
# launcher builds it on the player's machine on first use. No game file is
# ever packaged.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd -P)
os=${1:-linux}
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)
die() { echo "package: $*" >&2; exit 1; }
comps=$root/dist/components

case $os in
linux) name=SegaRacingLauncher-$version-linux-x86_64 ;;
windows) name=SegaRacingLauncher-$version-windows-x86_64 ;;
*) die "unknown target '$os' (linux or windows)" ;;
esac

out=$root/dist/pkg/$name
rm -rf "$out"
mkdir -p "$out/components/wal" "$out/licenses"

copy_wal() { # the launcher's own program of windowsarcadeloader
	cp "$comps/wal/$1" "$out/components/wal/"
	cp -r "$comps/wal/payloads" "$comps/wal/systemprofiles" "$out/components/wal/"
	cp "$comps/wal/LICENSE" "$out/licenses/windowsarcadeloader-LICENSE.txt"
	cp -r "$comps/art" "$comps/fonts" "$out/components/"
}

if [ "$os" = linux ]; then
	bin=$root/dist/linux
	[ -f "$bin/sega-launcher" ] || die "missing $bin/sega-launcher - build it first"
	[ -f "$bin/lib/libSDL2-2.0.so.0" ] || die "missing $bin/lib/libSDL2-2.0.so.0"
	"$root/scripts/build-components.sh" "$comps" wal runner winetricks assets
	install -m 755 "$bin/sega-launcher" "$out/sega-launcher"
	mkdir -p "$out/lib"
	cp "$bin"/lib/* "$out/lib/"
	[ -f "$bin/SDL2-copyright.txt" ] && cp "$bin/SDL2-copyright.txt" "$out/licenses/"
	copy_wal arcade-launcher
	cp -r "$comps/runner" "$out/components/runner"
	install -m 755 "$comps/winetricks" "$out/components/winetricks"
	# the first-use build of lindbergh-loader (OutRun 2 SP SDX)
	mkdir -p "$out/scripts"
	install -m 755 "$root/scripts/build-components.sh" "$root/scripts/lindbergh-in-container.sh" "$out/scripts/"
else
	exe=${CARGO_TARGET_DIR:-$root/target}/x86_64-pc-windows-gnu/release/sega-launcher.exe
	[ -f "$exe" ] || die "missing $exe - run scripts/build-launcher.sh windows"
	[ -f "$root/dist/sdl2/SDL2.dll" ] || die "missing dist/sdl2/SDL2.dll"
	"$root/scripts/build-components.sh" "$comps" wal assets
	cp "$exe" "$root/dist/sdl2/SDL2.dll" "$out/"
	sdl=$(sed -n 's/^dir=//p' "$root/packaging/sdl2.pin")
	cp "$root/dist/sdl2/${sdl%/*}/LICENSE.txt" "$out/licenses/SDL2-LICENSE.txt"
	copy_wal arcade-launcher.exe
fi

cp "$root/packaging/README-player.txt" "$out/README.txt"
cp "$root/packaging/SOURCES.txt" "$out/licenses/SOURCES.txt"
cp "$root/LICENSE" "$out/licenses/LICENSE-GPL-3.0.txt"
cp "$root/THIRD_PARTY.md" "$out/licenses/"
cp "$root/assets/fonts/OFL.txt" "$out/licenses/Barlow-OFL.txt"

cd "$root/dist/pkg"
if [ "$os" = linux ]; then
	tar -czf "$name.tar.gz" "$name"
	echo "package: dist/pkg/$name.tar.gz"
else
	rm -f "$name.zip"
	zip -qr "$name.zip" "$name"
	echo "package: dist/pkg/$name.zip"
fi
