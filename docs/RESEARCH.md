# Research

What the community already made for these six games, what the launcher
takes from it, and why. Findings from the game folders themselves (read in
place, never copied) are in section 3; what ran is in
[RUNTIME_NOTES.md](RUNTIME_NOTES.md).

The player's dumps are TeknoParrot-style folders (one game each, as the
TeknoParrot community distributes them), identified by the SHA-256 of their
main file (`crates/sega-core/src/catalog.rs`).

## 1. The games and their hardware

| Game | Board | Main file | What runs it |
|---|---|---|---|
| OutRun 2 SP SDX (2006) | Sega Lindbergh Yellow (Linux, x86) | `or2g/disk0/Jennifer/Jennifer` (ELF) | lindbergh-loader, natively |
| OutRun 2 SP Special Tours (2004) | Sega Chihiro (Xbox-based) | `outrun2.xbe` | Cxbx-Reloaded (in progress) |
| Crazy Taxi High Roller (2003) | Sega Chihiro | `ctx_ac[r].xbe` | Cxbx-Reloaded (in progress) |
| SEGA Rally 3 (2008) | Sega Europa-R (Windows XP PC) | `Rally/Rally.exe` (32-bit) | windowsarcadeloader under Wine |
| SEGA Racing Classic (2009) | Sega RingWide (Windows Embedded) | `d1a.exe` (32-bit) | windowsarcadeloader under Wine |
| F355 Challenge 2, twin (2001) | Sega NAOMI (cartridge, MAME set `f355twn2`, unzipped) | `epr-23399.ic22` | Flycast (in progress): needs `naomi.zip` and `f355bios.zip` |
| Daytona Championship USA NSE (2019) | Sega PC with the Sega Shell | `Sega/Daytona/Daytona.exe` (32-bit) | windowsarcadeloader with a `segashell` profile of our own (Our own: C3) |

## 2. Community projects

| Project | What it does for these games | Licence | Maintained | Decision |
|---|---|---|---|---|
| [lindbergh-loader](https://github.com/lindbergh-loader/lindbergh-loader) | Runs Lindbergh ELFs on Linux: JVS, drive board (force feedback through SDL), EEPROM, SRAM, OutRun's ranking paths, `SKIP_OUTRUN_CABINET_CHECK` | CC BY-NC-SA 4.0 | yes (2026) | **bundled** in the Linux package, built from the pinned commit (owner decision D4) |
| [windowsarcadeloader](https://github.com/aderumier/windowsarcadeloader) | Payload DLLs that play each cabinet's I/O board, dongle and network under Wine; profiles `europar/sega-rally-3` and `ringedge/sega-racing-classic` rated "works (user: 100%)"; the `segashell` system (Showdown, Transformers) | GPL-3.0 | yes, daily (2026-11) | **bundled**, built from a pinned commit with `patches/wal` |
| [Cxbx-Reloaded](https://github.com/Cxbx-Reloaded/Cxbx-Reloaded) | Xbox/Chihiro HLE emulator, Windows only; Chihiro support needs the media board ROM and three base-board EEPROMs from MAME's `chihiro` set | GPL-2.0 | yes (CI 2026-04) | **candidate** for the Chihiro pair: runs under GE-Proton's Wine with DXVK's `d3d9` and vkd3d beside it, reaches SEGABOOT; blocked on the board files (section 4) |
| TeknoParrot's `cxbxr` build | A patched Cxbx-Reloaded fork (`/fs`, `/win`, `beta.ini`) | none published | yes | not used yet: the same board files, no licence to bundle |
| TeknoParrot core (`TeknoMacaw.exe`, `TeknoParrot.dll`) | Daytona Championship USA, SEGA Rally 3, SEGA Racing Classic in TeknoParrotUI | closed, no licence | yes | **not usable**: VMProtect refuses to run under Wine ("Sorry, this application cannot run under a Virtual Machine") on bare metal; we never bypass a protection |
| [GE-Proton](https://github.com/GloriousEggroll/proton-ge-custom) | The Wine windowsarcadeloader runs games with (its DXVK and vkd3d too) | mixed free licences (Wine LGPL-2.1+) | yes | **bundled** in the Linux package, pinned by SHA-256 |
| [winetricks](https://github.com/Winetricks/winetricks) | windowsarcadeloader installs native XACT/XAudio2 with it (the RingWide sound thread crashes on Wine's own) | LGPL-2.1+ | yes | **bundled**, pinned |
| [FFBArcadePlugin](https://github.com/Boomslangnz/FFBArcadePlugin) (Boomslangnz) | Force feedback for many TeknoParrot games through a DLL in the game folder | GPL-3.0 | moved to the closed FFBlaster | not used: windowsarcadeloader plays the games' force feedback itself (`out` lines), and lindbergh-loader the drive board |
| xemu Chihiro forks | Low-level Chihiro emulation (needs disc images, not extracted dumps) | GPL-2.0 | yes | not used: our dumps are TeknoParrot's extracted XBE folders |

## 3. What the games' own files say

- **OutRun 2 SP SDX**: the dump is the cabinet's disks (`or2g/disk0`, `disk1`…);
  `Jennifer/Jennifer` is a TeknoParrot-modified ELF (its CRC32 is not in
  lindbergh-loader's clean list, so the loader warns "not Clean"; it runs).
  The game reads `/home/disk0/...`, which lindbergh-loader turns into `..`
  from the game's folder, so the runtime mirrors `or2g/disk0` with
  `Jennifer` (and its `rankingdata`, `recordedata`) as real folders.
- **SEGA Rally 3**: `Rally/config.ini` holds the operator settings (free
  play, language, cabinet type, graphics); windowsarcadeloader overrides
  free play and resolution through its environment. `Rally/` is written
  to, so the runtime keeps it as a real folder.
- **SEGA Racing Classic**: `d1a.ini` describes the RingWide hardware
  check; windowsarcadeloader answers it. The 4-speed H lever is four held
  switches on the JVS board (profile `native_map`).
- **Daytona Championship USA**: a Sega Shell PC: `Sega/shell/Shell.exe`,
  `InpWrapper.exe`, the game in `Sega/Daytona`, settings in `Sega/ShellData`
  and `Sega/GameData`. TeknoParrotUI's (GPL-era, decompiled locally)
  `Daytona3` input writes Start, shift up/down, coin, gears 1–4 and the four
  view buttons as bits, wheel/gas/brake as bytes, into the
  `TeknoParrot_JvsState` shared memory.
- **Chihiro games**: XBE plus `Media/`, `boot.id` (makes Cxbx boot SEGABOOT
  first), small `firmware*.bin` files that are not the media board ROM.

## 4. Chihiro board files

Cxbx-Reloaded (upstream and TeknoParrot's) aborts without these, looked up
in `EmuMediaBoard/` (MAME `chihiro` BIOS set, MAME SHA-1):

| File | Size | SHA-1 |
|---|---|---|
| `fpr21042_m29w160et.bin` (media board ROM; MAME's alternative `fpr-23887_29lv160te.ic4` has the same size) | 2 MiB | `a13cf9c5cdfe8605d82150b7573652f419b30197` |
| `Chihiro/ic10_g24lc64.bin` | 8 KiB | `3ababd4334d8d57abb22dd98bd2d347df39648d9` |
| `Chihiro/pc20_g24lc64.bin` | 8 KiB | `82dad6e2a75bab4a4840dc6939462f1fb9b95101` |
| `Chihiro/ic11_24lc024.bin` | 128 B | `cc03a0650bfac4bf6cb66e414bbef121cba53efe` |

Cxbx writes into all of them (region byte at `ic10` 0x1F00), so the
launcher will copy them into its runtime, never link them. The player's
BIOS folder (2026-11-15) held the media ROM (`fpr-23887`), SEGABOOT's two
partitions and the Xbox BIOS, but not the three EEPROMs.

## 5. Wheels and force feedback

| Game | Who reads the wheel | Force feedback |
|---|---|---|
| SEGA Rally 3, SEGA Racing Classic | the launcher (SDL), fed to windowsarcadeloader's `--input-pipe` (`patches/wal/0001`) | windowsarcadeloader's `out` lines (constant, centring, rumble), played by the launcher on the wheel through SDL, scaled by the player's strength |
| OutRun 2 SP SDX | lindbergh-loader itself (SDL: `controls.ini`) | lindbergh-loader's drive board emulation (SDL haptics) |
| Daytona Championship USA | the launcher, through windowsarcadeloader's input pipe (the board is a DirectInput "SEGA IO Controller") | the Shell's InpWrapper helper, emulated by the payload: parallel-port bytes (strength and direction, SEGA Rally 3's encoding) become `out` lines |
| Chihiro | to be decided with its route | |

Steering lock defaults to 270°, the cabinets' (SDK `SteeringLock::DEFAULT`).

## 6. Owner decisions

| # | Decision | Date |
|---|---|---|
| D1 | **DRM exception**, as for the Initial D launcher: maintained community loaders that answer the cabinets' keychips, security boards and I/O boards (lindbergh-loader, windowsarcadeloader, Cxbx-Reloaded) are approved. The launcher writes no crack, never ships or points to game files. | 2026-11-14 |
| D2 | Open loaders where they make sense, TeknoParrot's binaries allowed where needed; every game tested properly. | 2026-11-14 |
| D3 | TeknoParrot's core refuses Wine (VMProtect VM check); we don't bypass it. Daytona gets a windowsarcadeloader `segashell` profile of our own, offered upstream. | 2026-11-15 |
| D4 | lindbergh-loader ships prebuilt in the Linux package (unmodified, free of charge, with its CC BY-NC-SA 4.0 licence) instead of being built on the player's machine. | 2026-11-16 |
| D5 | Microsoft's DirectX June 2010 redistributable (about 95 MB) is bundled for winetricks' `xact`, so the first start of SEGA Rally 3 and Racing Classic works offline. | 2026-11-16 |

Defaults taken without a separate decision (as in the Initial D launcher;
the owner can change them): GPL-3.0-or-later, since windowsarcadeloader,
which the package bundles, is GPL-3.0; original illustrations for the
window rather than official art.
