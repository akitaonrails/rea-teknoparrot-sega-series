# Changelog

All notable changes to this launcher. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/).

## Unreleased

## 0.4.0 - 2026-11-19

### Added

- OutRun 2 SP Special Tours and Crazy Taxi High Roller (Sega Chihiro) are now
  playable on Linux and Windows through Tovarichtch's xemu Chihiro fork
  (`Playtest-v2`, GPL-2.0). The launcher packs each extracted dump into a FATX
  netboot image in its runtime folder, configures the Chihiro BIOS and
  network firmware from your BIOS folder, and maps your render resolution,
  steering lock and force-feedback strength into `xemu.toml`.

### Fixed

- SEGA Rally 3, SEGA Racing Classic and Daytona Championship USA no longer
  close right after opening on NVIDIA + Wayland desktops: the launcher sets
  `DISABLE_GAMESCOPE_WSI=1` on Linux so 32-bit DXVK presents cleanly through
  gamescope instead of hitting gamescope 3.17's `SyncobjFdToHandle` crash.
- F355 Challenge 2 now honours your Game resolution setting in Flycast
  (scaled in 4:3 up to 2880x2160 with 16x anisotropic filtering) instead of
  staying at 640x480.
- The Play and Settings buttons no longer overlap "Your games" and the game
  tiles when F355 Challenge 2 is selected.

## 0.3.0 - 2026-11-18

### Fixed

- Play before ever opening Settings no longer starts the game at
  3840x2160: the recommended look for your screen is decided the first
  time a game's settings are read. On the command line on Linux, where the
  screen isn't known, `--play` uses the look for 1920x1080 for that run
  without saving it, and `--set` saves only the options you gave, so the
  window still decides the look from your real screen; on Windows the
  command line asks Windows for the screen.
- A setting the launcher can't read (`fullscreen = off # mine`) stays as
  you wrote it; saving another setting no longer replaces it with the
  default. A look you moved away from no longer stays in the file, and a
  picture flag (`--set src --render 1280x720`, also after `--look`) makes
  the look Custom, as a picture row in the window does. A settings file or
  `launcher.cfg` that can't be read is never overwritten, and saving writes
  only what changed.
- `launcher.cfg` starting with a byte-order mark no longer loses its first
  game folder; its comments and other lines are kept on save.
- On Linux a game no longer outlives a launcher that was killed, and the
  launcher never signals processes that aren't the game's.
- A game folder in your list that isn't there (an unplugged drive) no
  longer stops the games; a library folder that holds the launcher's own
  data folder (`--library ~`) no longer makes the games' saves and the
  Wine prefix read-only.
- Each start takes a free block of windowsarcadeloader's ports (from
  38000): the Initial D launcher's games and these no longer collide.
- `--no-input` no longer makes windowsarcadeloader's games quit at once.
- Windows: Stop asks every window of the game's processes to close; a
  failed hand-over of the game to the launcher's job no longer stops the
  game, and programs it starts at once can't escape the job; a read-only
  file in a game folder no longer breaks the next start; a real file the
  game wrote in its runtime folder is refused with a message instead of
  overwritten.
- Every error is also written to `launcher.log`; a game that stopped with
  an error names its own `session.log`; paths are shown with the system's
  separator; `~` works in typed paths on Windows.
- The loader's files in the runtime (`lindbergh.ini`, the profile layer,
  Flycast's `emu.cfg`) are written as whole-file swaps: a crash never
  leaves half a file.
- Command line: `--check <game>` reports only that game (its unknown
  builds, its exit code); `--list` works while the window is open, and
  `--list --library <folder>` takes the lock like any command that saves;
  `--set <game>` without options is refused instead of doing nothing;
  `--game <game>` works as `--set`'s old name, as the settings files'
  heading said (it now names `--set` and `--play`); `--input-script` is
  refused for OutRun 2 SP SDX and F355 Challenge 2, which read their
  controls themselves.
- A download cut short while building the packages no longer breaks every
  later build.

### Added

- A pad works the launcher's window: d-pad or left stick to move, A to
  choose, B to go back, Start to play, Y for Settings, the shoulder buttons
  change tabs. While a game runs the pad is the game's.
- On Windows the Screen size, Upscaler, Sharpness and Full screen rows are
  shown greyed out with the reason (they are for gamescope on Linux)
  instead of hidden.
- A game that closes by itself within seconds of its start says so, with
  the lines of its log that tell why.

### Changed

- Updated rea-launcher-sdk to v0.10.0: the window, the command line, the
  library scan, the runtime folder, the game's processes and the input
  thread are now the SDK's TeknoParrot hub modules, shared with the
  Initial D, Nintendo and Wangan launchers. The window looks the same.
- The Barlow typefaces are inside the program; the packages no longer
  carry a `components/fonts` folder.

## 0.2.0 - 2026-11-17

### Added

- F355 Challenge 2 (twin) starts on Linux in Flycast 2.7, which the Linux
  package now carries. The cabinet's two linked boards run in one window:
  the second board, which draws the race, gets the screen, and the main
  board runs out of sight and reads the controls. The launcher packs the
  game's chips into the archive Flycast reads, links `f355bios.zip` from your
  BIOS folder, and keeps the cabinet's memory (records, operator settings)
  in the game's runtime folder.
- When F355's boards fail to find each other at boot (a known Flycast
  problem), the launcher notices and starts the game again by itself. It
  also runs both boards on one group of matching CPU cores, which makes that
  failure rarer on CPUs with two kinds of cores.
- F355 Challenge 2 also starts in the Windows version, which now carries
  Flycast 2.7's Windows build. Only the second board's window shows (full
  screen by default, as the Full screen setting says); the main board's
  window is hidden. Closing the game's window ends both boards, and the
  records are saved first. The Windows version also restarts a hung boot
  and keeps both boards on one group of matching cores.
- Closing F355's window (or gamescope's) ends the game and saves the
  records, on both systems. A stop asks the main board first, so it saves
  before the rest closes.

### Changed

- One BIOS folder for every game that needs one (`bios` in `launcher.cfg`,
  `--bios <folder>`): the Chihiro files and F355's `f355bios.zip`, also found
  in a `chihiro`, `dc` or `naomi` folder inside it, as EmuDeck keeps them.
  The `chihiro_bios` key and `--chihiro-bios` flag from 0.1.0 still work.
- A stopping game gets 10 seconds, up from 8, to save and quit before the
  launcher ends it.

## 0.1.0 - 2026-11-16

### Added

- The Linux package carries lindbergh-loader, built from its pinned commit,
  so OutRun 2 SP SDX's first Play no longer needs docker, podman or git.
- The Linux package carries Microsoft's DirectX June 2010 runtime for
  winetricks' `xact`, so SEGA Rally 3 and SEGA Racing Classic start the first
  time without a download.

- One launcher for six Sega arcade racers on TeknoParrot dumps: OutRun 2 SP
  SDX, OutRun 2 SP Special Tours, Crazy Taxi High Roller, SEGA Rally 3,
  SEGA Racing Classic and Daytona Championship USA, each known by the
  SHA-256 of its main file.
- SEGA Rally 3 and SEGA Racing Classic in windowsarcadeloader under the
  bundled GE-Proton, with the launcher feeding the cabinet's controls and
  playing the force feedback: full races verified.
- OutRun 2 SP SDX in lindbergh-loader (shipped in the Linux package), with the
  cabinet check skipped so it gets past "Initializing the drive board": full
  race verified. A wheel found before the start is written into
  lindbergh-loader's controls (steering and pedal axes, player 1).
- Daytona Championship USA in windowsarcadeloader, with a `segashell`
  profile written for this launcher (`patches/wal/0004`: the game's Shell
  protocol, its force feedback helper): full race verified.
- F355 Challenge 2 (MAME set `f355twn2`) is found and listed; it can't
  start yet (Flycast's two linked boards are still being brought up).
- Wheel options per game: steering lock (270° by default, as the cabinets)
  and force feedback strength; a sequential shift drives SEGA Racing
  Classic's H lever, and one button walks through its four views.
- A game's settings start from the recommended look for your screen, saved
  the first time the window shows them.
- Looks (Original, Recommended, Steam Deck), the option table for every
  setting (`--help`, settings files, the window), `--list`, `--check`,
  `--set`.
