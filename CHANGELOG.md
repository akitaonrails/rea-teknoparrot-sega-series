# Changelog

All notable changes to this launcher. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/).

## Unreleased

### Added

- One launcher for six Sega arcade racers on TeknoParrot dumps: OutRun 2 SP
  SDX, OutRun 2 SP Special Tours, Crazy Taxi High Roller, SEGA Rally 3,
  SEGA Racing Classic and Daytona Championship USA, each known by the
  SHA-256 of its main file.
- SEGA Rally 3 and SEGA Racing Classic in windowsarcadeloader under the
  bundled GE-Proton, with the launcher feeding the cabinet's controls and
  playing the force feedback: full races verified.
- OutRun 2 SP SDX in lindbergh-loader (built on first use), with the
  cabinet check skipped so it gets past "Initializing the drive board": full
  race verified. A wheel found before the start is written into
  lindbergh-loader's controls (steering and pedal axes, player 1).
- Daytona Championship USA in windowsarcadeloader, with a `segashell`
  profile written for this launcher (`patches/wal/0004`: the game's Shell
  protocol, its force feedback helper): full race verified.
- Wheel options per game: steering lock (270° by default, as the cabinets)
  and force feedback strength; a sequential shift drives SEGA Racing
  Classic's H lever, and one button walks through its four views.
- Looks (Original, Recommended, Steam Deck), the option table for every
  setting (`--help`, settings files, the window), `--list`, `--check`,
  `--set`.
