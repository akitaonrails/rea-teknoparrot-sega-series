# Runtime notes

What was run against the real games, how, and what was seen. Observations
only; anything not listed here has **not** been verified.

Machine: Arch (Omarchy) host, `gaming` distrobox, NVIDIA RTX 5090. Game runs
are headless (`gamescope 3.17 --backend headless`), with a private PipeWire
null sink, screenshots with `gamescopectl screenshot` on the session's own
socket, and the library bound read-only with bubblewrap: the game and every
loader see the dumps read-only, whatever they try. Probe scripts live in
`.work/probe/` (not committed). Components from `scripts/build-components.sh`:
windowsarcadeloader `4f42c6d` with `patches/wal`, GE-Proton 11-12,
winetricks 20260125, lindbergh-loader `0af606d`.

## Through the launcher (`sega-launcher --play <game> --headless`, 2026-11-14)

| Game | Result |
|---|---|
| SEGA Racing Classic | **race played**: attract (Ranking, Advanced course), 2 coins, Start, course and transmission taken with Start, accelerator held, Lap 2 of 8 at 221 km/h, steering both ways, the view button cycled the cameras, shifting through the H lever from the launcher's sequential shift. Coins needed (no free play in its profile). |
| SEGA Rally 3 | **race played**: coin, Start, car select ("Confirm with Gas Pedal"), Citroën C4 WRC, Tropical stage, 125 km/h in 4th, steering both ways, view change. |
| OutRun 2 SP SDX | without `SKIP_OUTRUN_CABINET_CHECK` the game stops 23 s in, after "Initializing the drive board" (the cabinet check fails); with it (the launcher's `lindbergh.ini`) it reaches the title screen and the attract mode. |
| OutRun 2 SP SDX, keys sent to the session's Xwayland (XTest) | **race played**: Start through driver, F50, automatic, OutRun mode (Special Tours), music select, then the coast at 111 km/h, score counting, steering both ways. Drive board emulation sends rumble to the SDL device. |
| OutRun 2 SP SDX, output | lindbergh-loader buffers its stdout when it goes to a pipe: a crash loses its last lines, so a game that seems to stop "after loading the ranking" may have gone much further. Read the game's own state, not only the log. |

## From the Linux package with everything bundled (2026-11-16)

| Game | Result |
|---|---|
| SEGA Racing Classic, fresh data folder and Wine prefix | winetricks' `xact` took `components/directx/directx9/directx_Jun2010_redist.exe` from the package (`W_CACHE`, no download line in `session.log`); full race, lap 1 at 0'26"84 |
| OutRun 2 SP SDX | the package's `components/lindbergh` (no docker, no build); race at 120 km/h |

## Chihiro (Cxbx-Reloaded `CI-585c49a`, by hand, 2026-11-15)

| Step | Result |
|---|---|
| `cxbxr-ldr.exe /load <xbe> /chihiro` under GE-Proton's Wine | "Error loading cxbxr-emu.dll": needs the VC++ 2015–2022 runtime (`MSVCP140_ATOMIC_WAIT.dll`), installed with winetricks `vcrun2022` (it fails without a display; it works inside a headless gamescope) |
| then | `d3d9.dll`/`D3DCOMPILER_47.dll` fail to load: GE-Proton's 32-bit `wined3d` needs vkd3d's DLLs, which Proton's own launch script provides. DXVK's 32-bit `d3d9.dll` and vkd3d's three DLLs beside Cxbx (`d3d9=n`) fix it |
| then | Cxbx starts, patches the XBE (HLE), needs `boot.id` beside the XBE (keep it in the runtime), extracts SEGABOOT from the media board ROM and boots it |
| with zero-filled stand-ins for the three base-board EEPROMs (a probe only) | the screen stays black for two minutes. |
| 2026-11-16, the owner's real `ic10_g24lc64` (sha1 `3ababd43…`), `pc20_g24lc64.bin`, `ic11_24lc024.bin` and `EEPROM.bin`, through `cxbxr-ldr.exe` | no window at all: SEGABOOT's reboot starts a second `cxbxr-ldr.exe … Partition3.bin /chihiro /sid …` and the first exits, so the session's shared memory goes with it and the second runs silent |
| the same through `cxbx.exe <xbe>` (its GUI keeps the session) | Chihiro mode is detected, SEGABOOT reboots into `OUTRUN2.XBE`, the emulator runs (about one CPU core busy), and its window stays black after 75 s, windowed and full screen, in gamescope's capture and in a direct X capture. **Still blocked**: next is a visible run, and Cxbx's own Chihiro compatibility for these two titles |

## Daytona Championship USA (2026-11-15)

| Step | Result |
|---|---|
| TeknoParrot core (`TeknoMacaw.exe "<Daytona.exe>"`, as TeknoParrotUI starts it) under GE-Proton's Wine, bare metal | a message box "Sorry, this application cannot run under a Virtual Machine" (VMProtect). Not bypassed, by rule. |
| windowsarcadeloader `segashell` profile (`patches/wal/0004`), by hand | boots in about 2 minutes on a new prefix (shaders), 30 s later; attract, coins, start, mode, track and car select, races with steering, gas and brake; force feedback bytes from the emulated InpWrapper arrive as `out` lines (e.g. `0x13` → +8666). The game's Shell security challenge (it quits after 5 unanswered) is switched off by the profile, as Transformers' is. |
| Through the launcher (`--play dcusa --headless`, 2026-11-15) | **race played**: coin, start, the menus taken with the gas pedal, "Start your engines", lap 1 at 44 mph in automatic, steering both ways; 0 leftover processes. The view and gear buttons were pressed but no camera change was seen: not verified. |

## F355 Challenge 2 (Flycast 2.7, the box's package, by hand, 2026-11-15)

| Step | Result |
|---|---|
| Flycast on the unzipped `f355twn2/` folder | "Can't open CD device": Flycast takes NAOMI sets only as archives. An uncompressed zip of the folder loads. |
| with `naomi.zip` only | "cannot load BIOS f355bios": the F355 cabinet BIOS (`epr-22849/22850/22851.ic27`) is its own set; the owner's `bios/dc/f355bios.zip` has it |
| with `f355bios.zip` | Flycast always runs F355 as a multiboard game: the master waits forever ("Time out waiting for multiboard vsync. Slave 0") when started as `flycast`, because it restarts itself for the slave with `execv(argv[0])`, which doesn't search `PATH`. Started as `/usr/bin/flycast`, the slave boots ("naomi multi system… found my master, being named…"). |
| then | gamescope shows the focused window, which is the slave's: the master's picture was not seen yet. **Blocked** until the two boards are shown properly (the slave hidden, or its own display). |

## Quitting and leftovers

| Way to quit | SEGA Racing Classic | SEGA Rally 3 | OutRun 2 SP SDX |
|---|---|---|---|
| The launcher stops the game (`--seconds`, the window's Stop: SIGTERM to the session's process group, then SIGKILL after 8 s, then the session tag sweep) | 0 leftover processes | 0 leftover processes | 0 leftover processes |
| Quit from the game menu | not applicable: arcade games have no quit; the test menu does not exit | not applicable | not applicable |
| Window close / Alt+F4 on the game | not verified yet (needs a visible session) | not verified yet | not verified yet |
| SIGINT/SIGTERM to the launcher (`--play` in a terminal, Ctrl+C) | the same stop path as the first row (signal-hook sets the stop flag); not run separately yet | not run separately yet | not run separately yet |

## Not verified yet

- OutRun 2 SP SDX with a wheel: the launcher writes the wheel's SDL GUID and axes into lindbergh-loader's `controls.ini` (steering on the guessed axis, pedals by where they rest, `_INVERTED` when they rest at the top); not tried on hardware.
- Any wheel on hardware: steering lock, force feedback direction and strength, pedals on a second USB device (the owner's wheel).
- The Windows build of the launcher on real Windows; Steam Deck; multi-monitor; ultrawide.
- Free play in SEGA Rally 3 through `WAL_EUROPAR_FREEPLAY=1` (the race above used a coin).
- Render sizes above the cabinets' (SEGA Rally 3 at 1920x1080, SEGA Racing Classic and OutRun 2 SP SDX at 4K).
