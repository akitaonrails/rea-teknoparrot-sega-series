# Third-party components and credits

Nothing from the games is in this repository or its packages. The player
points the launcher at their own dumps; the launcher reads them in place,
read-only.

## Components the launcher uses

| Component | What it does here | Licence | How it reaches the player | Pin |
|---|---|---|---|---|
| [windowsarcadeloader](https://github.com/aderumier/windowsarcadeloader) (aderumier) | Runs SEGA Rally 3 and SEGA Racing Classic (under Wine on Linux; on Windows through its Windows host build, `patches/wal/0003`): plays each cabinet's I/O board, security and force feedback (owner decision D1, `docs/RESEARCH.md`) | GPL-3.0 | built from source with `patches/wal` (`scripts/build-components.sh wal`), inside both packages | commit `4f42c6d86ddaaa9d2e4e8c21e0046bb3027b111d` |
| [lindbergh-loader](https://github.com/lindbergh-loader/lindbergh-loader) | Runs OutRun 2 SP SDX (Sega Lindbergh Linux build) natively: JVS, security board, drive board, resolution | CC BY-NC-SA 4.0 (non-commercial, attribution, share-alike: shipped unmodified and free of charge, with its licence, by owner decision D4) | built in an ubuntu:22.04 container (`scripts/build-components.sh lindbergh`, the workflow's `lindbergh` job), inside the Linux package | commit `0af606d845b70339c335785c0eba68b47b78df3c` |
| SDL 3.2.26, SDL_ttf 3.2.2, SDL_image 3.2.4, FAudio 25.11 (32-bit) | lindbergh-loader's dependencies, built with it | zlib | same build | release tags |
| Ubuntu 22.04 i386 libraries (freeglut, libstdc++5, libXmu, libXi, libGLU, libXt, libSM, libICE, libbsd, libmd) | 32-bit libraries the Lindbergh games load that current desktops lack | their own: MIT/X11 (freeglut, libXmu, libXi, libXt, libSM, libICE), SGI Free Software B (libGLU), GPL-2 with the libstdc++ runtime exception (libstdc++5), BSD (libbsd, libmd); each package's `copyright` file ships in `licenses/lindbergh-libraries/` | copied out of the same build container | Ubuntu 22.04 archive; the exact package versions of each build are in `licenses/lindbergh-libraries/VERSIONS.txt` |
| libssl/libcrypto 0.9.7, libopenal.so.0 | Libraries Lindbergh games link against | OpenSSL/SSLeay licence (libssl, libcrypto), LGPL-2 (OpenAL) | from lindbergh-loader's `libs/` in the same build; the licence texts ship in `licenses/lindbergh-libraries/` | same commit; OpenSSL's text from tag `OpenSSL_0_9_7m`, sha256 `81d5872403d0f080f0e2ba0ff305023eab07245691704e93b6bd1b144a20d9e1` |
| [Flycast](https://github.com/flyinghead/flycast) 2.7 (flyinghead) | Runs F355 Challenge 2 (Sega NAOMI): both linked boards (on Linux the launcher's program starts each one; on Windows the launcher hides the master's window) | GPL-2.0 (its `LICENSE` ships as `licenses/Flycast-LICENSE.txt`) | Linux package: the official AppImage, unpacked (`scripts/build-components.sh flycast`), with the libraries the AppImage carries; Windows package: the official Windows build's `flycast.exe` (`flycast-win`) | `flycast-x86_64-2.7.AppImage` sha256 `5b9f8a636a6acb8446bc78fe96650d8cb8233ac9f8ce69665dab3f39a1cbaae1`; `flycast-win64-2.7.zip` sha256 `7edf489ba6b3053703a18ce8233767eb71a2d89e4bda0421b312c2167a739161`; licence from tag `v2.7`, sha256 `8177f97513213526df2cf6184d8ff986c675afb514d4e68a404010521b880643` |
| [GE-Proton](https://github.com/GloriousEggroll/proton-ge-custom) 11-12 | The Wine windowsarcadeloader runs the Windows games with (the runner it is tested on) | BSD-3-Clause / LGPL (Wine) and others, see its `LICENSE` | inside the Linux package (downloaded at build time, checked by SHA-256) | `GE-Proton11-12-x86_64.tar.gz` sha256 `0595159e417c3dbbfb7cc3ae0d3ed15a617cc607aa23ef756ea79029d7af5d70` |
| Microsoft DirectX XACT / XAudio2 runtime | Installed into the launcher's Wine prefix by winetricks (`xact`): SEGA Racing Classic's sound thread crashes on Wine's own XAudio2 | Microsoft DirectX End-User Runtime redistributable (its EULA allows redistribution with an application) | `directx_Jun2010_redist.exe` inside the Linux package (`components/directx/directx9/`), handed to winetricks through `W_CACHE`, so the first start needs no download | winetricks' own mirror and digest: sha256 `8746ee1a84a083a90e37899d71d50d5c7c015e69688a466aa80447f011780c0d` (Microsoft's current download is re-signed, and winetricks refuses it) |
| [winetricks](https://github.com/Winetricks/winetricks) | Installs the XACT runtime into the Wine prefix (windowsarcadeloader runs it) | LGPL-2.1 | inside the Linux package | tag `20260125`, sha256 `431f82fc74000e6c864409f1d8fb495d696c03928808e3e8acffc45179312a7b` |
| SDL 2.32.10 (MinGW) | The Windows launcher's pads and wheels | zlib | `SDL2.dll` inside the Windows package | `sdk/packaging/sdl2.pins` (the SDK's pin) |
| gamescope, bubblewrap | Full-screen display, scaling (FSR/NIS); the library bound read-only around every game | BSD-2-Clause, LGPL-2.0 | the player's system | — |

## Code we wrote on top of others

| What | Where | Licence |
|---|---|---|
| `--input-pipe` for windowsarcadeloader's launcher (the hub feeds the virtual stick and receives force feedback) | `patches/wal/0001-launcher-input-pipe.patch` | GPL-3.0 (as upstream) |
| windowsarcadeloader's Windows host build (`crates/winhost`: its run on Windows itself, the cabinet's drives with `DefineDosDevice`, junctions instead of symlinks) | `patches/wal/0003-windows-host.patch` | GPL-3.0 (as upstream) |
| Each game's cabinet layout for the input pipe (SEGA Rally 3's sequential shift and handbrake; SEGA Racing Classic's H lever and four view buttons from a sequential shift) | `crates/sega-core/src/input.rs` | GPL-3.0-or-later |

## Images and fonts

| What | Author | Licence | Source |
|---|---|---|---|
| The hub's scenes, `assets/art/*.jpg` (a coast at sunset and at midday, a city avenue, a jungle dirt stage, a speedway by day and by night) | drawn for this launcher from shapes by `scripts/art/scenes.py`; no photos, game files or official art | GPL-3.0-or-later | this repository |
| README screenshots, `docs/images/launcher-*.jpg` | the launcher's own window, taken headless | GPL-3.0-or-later | this repository |
| The launcher's icon, `packaging/appimage/sega-launcher.png` | the rea-launcher-sdk template | MIT OR Apache-2.0 | rea-launcher-sdk |
| Barlow and Barlow Condensed, embedded in the program by rea-launcher-ui | Jeremy Tribby | SIL Open Font License 1.1 (`assets/fonts/OFL.txt`, shipped as `licenses/Barlow-OFL.txt`) | <https://github.com/jpt/barlow> |

## Code we follow

| Project | What we used | Licence |
|---|---|---|
| TeknoParrotUI (teknogods; GPL-era source and a locally decompiled build) | Each cabinet's controls and how TeknoParrot starts each game (read to learn; nothing copied) | GPL-3.0 |

## Thanks

aderumier (windowsarcadeloader), the Lindbergh Development Team
(lindbergh-loader), the Cxbx-Reloaded team, GloriousEggroll (GE-Proton),
Boomslangnz (FFBArcadePlugin, for showing how these cabinets' force feedback
works) and teknogods.
