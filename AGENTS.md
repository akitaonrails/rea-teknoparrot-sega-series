# rea-teknoparrot-sega-series — instructions for coding agents

One launcher (a hub, like the Initial D and Wangan Midnight ones) for the
Sega arcade racers the owner dumped from TeknoParrot: OutRun 2 SP SDX
(Lindbergh), OutRun 2 SP Special Tours and Crazy Taxi High Roller (Chihiro),
SEGA Rally 3 (Europa-R), SEGA Racing Classic (RingWide) and Daytona
Championship USA (Sega PC). Built on
[rea-launcher-sdk](https://github.com/akitaonrails/rea-launcher-sdk) v0.9.0.

**Read `sdk/docs/STANDARDS.md` before any work; the playbooks are in
`sdk/docs/playbooks/`** (vendored from the SDK at the tag in `sdk/VERSION`).
Then `docs/RESEARCH.md` (what runs each game and why) and
`docs/RUNTIME_NOTES.md` (what was seen in the real games). Sister projects:
`rea-teknoparrot-initial-d-series` (the code this one started from) and
`rea-teknoparrot-wangan-series`.

## Always

- The game's files are read-only: never committed, copied into the repo or published.
- Everything outside this repository is the user's and read-only (game installs,
  other repos, Wine prefixes, saves, home configs, containers, the SDK checkout)
  unless the user asked for or approved that specific change: back up first,
  never delete by a broad glob, ask when unsure.
- Headless by default (`gamescope --backend headless`, a private PipeWire
  null sink); open a visible window only when a check needs a person, and say so first.
- Never print, log or commit credentials; never list other processes' command lines.
- Push, tag, release or post only with the user's go-ahead.
- Research community fixes first (`docs/RESEARCH.md`); patch only what nobody solved.
- Never edit `sdk/`; SDK gaps and bugs go to the SDK repository as issues or PRs.
- Clean up after a release or a long session: `sdk/scripts/clean`, then `--yes`.

## Owner decisions (see docs/RESEARCH.md, "Owner decisions")

- **DRM exception** (2026-11-14): maintained community loaders that answer the
  cabinets' keychips, security and I/O boards are approved (lindbergh-loader,
  windowsarcadeloader, Cxbx-Reloaded). Never write a crack of our own, never
  ship or point to game files.
- **TeknoParrot's own binaries** are allowed where an open route doesn't work
  (2026-11-14), but its core refuses Wine with a VMProtect "virtual machine"
  check, which we never bypass (2026-11-15). Daytona Championship USA gets a
  windowsarcadeloader `segashell` profile of our own instead.
- **lindbergh-loader** (CC BY-NC-SA) ships prebuilt in the Linux package,
  unmodified, free of charge and with its licence (2026-11-16, D4).
- **XAudio2**: Microsoft's DirectX June 2010 redistributable is bundled in
  the Linux package for winetricks' `xact` (2026-11-16, D5).

## Layout

| Path | What |
|---|---|
| `crates/sega-core/src/catalog.rs` | The games, their routes, known builds (SHA-256), support state |
| `crates/sega-core/src/library.rs` | Finding the games in the player's folders |
| `crates/sega-core/src/runtime.rs` | The per-game runtime folder (links, copies, opened folders) |
| `crates/sega-core/src/settings.rs` | The option table: settings files, flags, `--help`, looks |
| `crates/sega-core/src/launch.rs` | `plan()`: files, links, command, environment per game |
| `crates/sega-core/src/input.rs` | Cabinet buttons, pad layout, windowsarcadeloader stick layouts |
| `crates/sega-launcher/src/cli.rs`, `main.rs` | The command line, messages, single instance |
| `crates/sega-launcher/src/session.rs`, `proc.rs` | One game run: gamescope, bubblewrap, process group, cleanup |
| `crates/sega-launcher/src/input.rs` | The SDL thread: wheels, pads, force feedback |
| `crates/sega-launcher/src/ui.rs`, `ui/` | The hub window and its headless tests |
| `scripts/build-components.sh` | lindbergh-loader, windowsarcadeloader (patched), GE-Proton, winetricks, the DirectX redistributable |
| `patches/wal/` | Our changes to windowsarcadeloader (input pipe, Windows host, profiles) |
| `docs/RESEARCH.md`, `docs/RUNTIME_NOTES.md`, `docs/CONTRIBUTIONS.md` | Findings, verification, drafts to share |

## Checks (all must pass before a commit)

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
sdk/scripts/launcher-check                     # family rules; every error blocks the commit
# Windows target (MinGW + LLD, in the gaming distrobox):
scripts/build-launcher.sh windows
```

Game runs: `.work/probe/one.sh <game> <script|-> <seconds> <name>` inside the
`gaming` distrobox (`distrobox enter --no-workdir gaming -- sh -c '…'`),
headless, with screenshots under `.work/out/<name>/`.

Conventional Commits; every behaviour change gets a CHANGELOG "Unreleased"
entry, and every change that still needs a real-game check a line in
`docs/RUNTIME_NOTES.md`.

## What is specific to these games

- **Editions:** one TeknoParrot dump each (`catalog.rs`); unknown builds are listed with their SHA-256, never started.
- **Routes:** OutRun 2 SP SDX in lindbergh-loader (Linux only; `SKIP_OUTRUN_CABINET_CHECK` or it stops at 23 s);
  SEGA Rally 3 and SEGA Racing Classic in windowsarcadeloader under GE-Proton's Wine (native XACT);
  the Chihiro pair in Cxbx-Reloaded (needs MAME's `chihiro` board files);
  Daytona in a windowsarcadeloader profile of our own; F355 Challenge 2 in the bundled
  Flycast (Linux: the launcher's program starts each board, the slave board alone in gamescope;
  Windows: a copy of flycast.exe in the runtime, the master's window hidden, its log over local UDP).
- **Files the games write:** `Rally/`, `ShellData/` (SR3), `rom/` (SRC), `Jennifer/` and its ranking folders (OR2),
  `Sega/…` (Daytona): kept as real folders in the runtime (`runtime::opened`).
- **Wheels:** the launcher owns the wheel for the windowsarcadeloader games (lock, strength, pad fallback);
  lindbergh-loader reads it itself for OutRun 2 SP SDX.
- **Known problems:** see `docs/RUNTIME_NOTES.md` and the README's "Known limitations".
