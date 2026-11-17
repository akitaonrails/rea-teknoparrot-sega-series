# Changelog

All notable changes to this launcher. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/).

## Unreleased

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
