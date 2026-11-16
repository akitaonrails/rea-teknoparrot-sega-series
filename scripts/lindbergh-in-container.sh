#!/bin/sh
# Runs inside ubuntu:22.04 (scripts/build-components.sh): lindbergh-loader's own CI recipe
# (32-bit SDL3, SDL3_ttf, SDL3_image, FAudio from source), then make, then the runtime libraries.
set -e
export DEBIAN_FRONTEND=noninteractive
dpkg --add-architecture i386
apt-get update -qq
apt-get install -y -qq --no-install-recommends sudo ca-certificates git build-essential gcc-multilib g++-multilib cmake \
  freeglut3-dev:i386 libstdc++5:i386 libxmu6:i386 libasound2-dev:i386 libfreetype6-dev:i386 libdbus-1-dev:i386 \
  libudev-dev:i386 libxcursor-dev:i386 libxfixes-dev:i386 libxi-dev:i386 libxrandr-dev:i386 libxss-dev:i386 \
  libxxf86vm-dev:i386 libxtst-dev:i386 libpulse-dev libsndio-dev:i386 libgl-dev:i386 libx11-dev:i386 >/dev/null
cp -a /src /work && cd /work
git config --global --add safe.directory '*'
mkdir -p build-deps && cd build-deps
for r in libsdl-org/SDL@release-3.2.26 libsdl-org/SDL_ttf@release-3.2.2 libsdl-org/SDL_image@release-3.2.4 FNA-XNA/FAudio@25.11; do
  repo=${r%@*}; tag=${r#*@}; name=${repo#*/}
  git clone -q --depth 1 --branch "$tag" https://github.com/$repo.git $name
  extra=""; [ "$name" = FAudio ] && extra="-DBUILD_SHARED_LIBS=off"
  cmake -S $name -B $name/build -DCMAKE_C_FLAGS=-m32 -DCMAKE_CXX_FLAGS=-m32 -DCMAKE_INSTALL_PREFIX=/usr -DCMAKE_INSTALL_LIBDIR=lib/i386-linux-gnu -DSDLTTF_VENDORED=OFF -DSDLTTF_HARFBUZZ=OFF -DSDLTTF_PLUTOSVG=OFF -DSDLIMAGE_VENDORED=OFF -DSDLIMAGE_AVIF=OFF -DSDLIMAGE_WEBP=OFF -DSDLIMAGE_TIF=OFF -DSDLIMAGE_JXL=OFF $extra >/dev/null
  make -s -C $name/build -j"$(nproc)" >/dev/null && make -s -C $name/build install >/dev/null
  echo "built $name $tag"
done
cd /work
make -j"$(nproc)"
mkdir -p /out && cp -a build/. /out/
cp -a /usr/lib/i386-linux-gnu/libSDL3*.so* /out/ 2>/dev/null || true
# 32-bit libraries the Lindbergh games and the loader need that desktop distributions lack
apt-get install -y -qq --no-install-recommends freeglut3:i386 libstdc++5:i386 libxmu6:i386 libxi6:i386 \
  libglu1-mesa:i386 libxt6:i386 libsm6:i386 libice6:i386 libbsd0:i386 libmd0:i386 >/dev/null
for l in libglut.so.3 libstdc++.so.5 libXmu.so.6 libXi.so.6 libGLU.so.1 libXt.so.6 libSM.so.6 libICE.so.6 libbsd.so.0 libmd.so.0; do
  cp -L /usr/lib/i386-linux-gnu/$l /out/
done
# their notices and exact versions (THIRD_PARTY.md, licenses/ in the package), plus the
# notices of what was built from source here and of lindbergh-loader's own libs/
mkdir -p /out/notices
pkgs="freeglut3 libstdc++5 libxmu6 libxi6 libglu1-mesa libxt6 libsm6 libice6 libbsd0 libmd0"
for p in $pkgs; do cp /usr/share/doc/$p/copyright /out/notices/$p.copyright; done
{ echo "Ubuntu 22.04 i386 packages (source: the Ubuntu archive's source packages of these versions):"
  for p in $pkgs; do dpkg-query -W -f='  ${Package} ${Version} (source ${source:Package} ${source:Version})\n' "$p:i386"; done
  echo "Built from source (release tags): SDL 3.2.26, SDL_ttf 3.2.2, SDL_image 3.2.4, FAudio 25.11"
} > /out/notices/VERSIONS.txt
for n in SDL SDL_ttf SDL_image FAudio; do cp "$(ls /work/build-deps/$n/LICENSE* | head -1)" /out/notices/$n-LICENSE.txt; done
cp /usr/share/common-licenses/LGPL-2 /out/notices/libopenal-LGPL-2.txt
chown -R "${OWNER:-0:0}" /out
echo DONE
