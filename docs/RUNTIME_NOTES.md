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
| the same through `cxbx.exe <xbe>` (its GUI keeps the session) | Chihiro mode is detected, SEGABOOT reboots into `OUTRUN2.XBE`, the emulator runs (about one CPU core busy), and its window stays black after 75 s, windowed and full screen, in gamescope's capture and in a direct X capture. Still blocked (see the next row) |
| 2026-11-16, visible run on the owner's display (`cxbx.exe <xbe>`, windowed), with DXVK's `d3d9.dll` and then Wine's own `d3d9` (wined3d, OpenGL) | the game runs: its sound plays and Cxbx's counter reads about 15 FPS. With DXVK the render window keeps Cxbx's splash logo; with wined3d it is black (the owner watched it and confirmed). The game's shaders compile, but nothing is drawn. **Blocked on Cxbx-Reloaded's emulation of this title**, not on Wine or our setup. (The owner's Hyprland rules send `steam_proton` windows to workspace 9, which is why the earlier visible attempt seemed to show no window.) |

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
| 2026-11-16, both boards visible on the owner's display | in F355 Twin the **slave draws the game** (its window is titled "Ferrari F355 Challenge 2"); the master shows only the BIOS's screens. The cabinet had one screen per board, so one window is the slave's. |
| the master with `SDL_VIDEODRIVER=offscreen`, the slave re-run through a wrapper as `argv[0]` with `SDL_VIDEODRIVER=x11`, headless gamescope, private null sink | one window, the slave's: "naomi multi system", then the attract mode and "PRESS START BUTTON / FREE PLAY". The slave must be the master's direct child (it opens `/naomi_multiboard_mem<getppid()>`), so the wrapper `exec`s. Flycast sets SDL's joystick background-events hint itself; the slave forwards its keyboard to the master. |
| boot reliability, Flycast 2.7 AppImage and the box's 2.7 git build, unpinned | 1 boot in 6 reached the game; the others stopped at "being named..." with `W[INTERPRETER]: TRAP #50` (10 s in, or after the second `System reset` near 45 s). Upstream knows it: flycast#1909, #1630 (the author needed 6 or 7 tries for Twin 2; worse on CPUs with cores of different speeds). |
| pinned with `taskset` to one CCD of the Ryzen 9 7950X3D | CCD0 (0-7,16-23): 3 boots in 4; CCD1 (8-15,24-31, the one Linux ranks fastest): 2 in 4. The launcher pins to the fastest CPU's cache domain (P-cores only on hybrid Intel CPUs) and restarts on `TRAP #50` in the first 90 s, up to 6 tries. |
| through the launcher (`--play f355 --headless`, bundled Flycast 2.7, BIOS folder = EmuDeck's `bios`, 2026-11-16) | `taskset -c 8-15,24-31` in the session, `f355bios.zip` found in `bios/dc/` (`naomi.zip` isn't needed), the set packed into `runtimes/f355/f355twn2.zip` (stored): **attract mode and "PRESS START BUTTON" on the first try**; another run hung at 46 s, was stopped and started again by itself (try 2 of 6). No sound reached the host. Not played with controls yet. |
| stopping (`--seconds`) | SDL turns SIGTERM into a quit event: the master saves the cabinet's memory (`flycast/data/flycast/f355twn2.zip.nvmem`, written at the stop) and lets its slave go; the slave prints "Slave exiting" and stays on Flycast's error screen, so the session ends what's left at that line: the stop takes about 2 s, 0 leftover processes. Making the slave die on SIGTERM instead was tried and is wrong: the master then waits for its vsync and is killed without saving. A stop during the boards' first seconds still waits for the kill (8 s then, 10 s since the Windows work). |
| closing gamescope (TERM to the session's gamescope, Linux, 2026-11-17) | before the fix the master, which runs off screen, kept waiting for its vanished slave (or the slave sat on its error screen after gamescope's reaper killed the master), so gamescope never exited and the session waited until its own stop. Now the session watches both boards (the master by its exact arguments, the slave by `naomi:BoardId`) and stops when either goes: closed during the attract mode, the session ended 7 s later with the records saved 5 s in; closed during a boot, 5 s; 0 leftover processes. |
| stopping, revisited (2026-11-17) | a TERM to the whole group could reach gamescope first: its reaper then killed the master, which once crashed ("double free") without saving. The session now sends TERM to the master board alone; it saves, the slave prints "Slave exiting", and the rest is ended: two runs, records written at the stop, about 4 s, 0 leftovers. |

## F355 Challenge 2 on Windows (the Windows launcher under Wine, 2026-11-17)

Flycast's Windows build (official `flycast-win64-2.7.zip`) starts its slave board by its own path (`GetModuleFileNameW`), so the Linux trick (the launcher as `argv[0]`) can't apply. Run as `sega-launcher.exe --play f355 --seconds N` in GE-Proton 11-12's Wine, headless gamescope, private null sink. The probe runtime carried vkd3d and DXVK's `d3d11`/`dxgi` beside `flycast.exe`, because that Wine's `wined3d` couldn't load its vkd3d libraries outside Proton; real Windows has its own `d3d11`.

| Step | Result |
|---|---|
| Flycast's log | `[log] LogServer = 127.0.0.1:<port>` in `emu.cfg`: both boards send each line as one UDP datagram (with terminal colours) to the port the session opened; "TRAP #50" and "Slave exiting" arrive as on Linux. |
| one window | the session hides the master's window (`ShowWindow(SW_HIDE)`, its windows found through the threads of its process) and focuses the slave's (the master's child process). Attract mode and "FREE PLAY" on screen. The hidden master runs at full speed and saves. |
| CPUs | `GetLogicalProcessorInformationEx` under Wine: one efficiency class, two L3 domains; the session set the master's affinity to 0-7,16-23 right after the start, and the slave inherited it. 12 boots in 13 reached the game on the first try; one hung ("TRAP #50" at 48 s), was stopped (the "Slave exiting" line about 2 s after the stop) and booted on try 2. The plain Flycast probe, unpinned, hung once in its one run. |
| stopping (`--seconds`) | `WM_CLOSE` to the master's hidden window: SDL quits, the master saves `flycast/data/f355twn2.zip.nvmem` (written at the stop), the slave prints "Slave exiting", the session ends: exit 0, 0 leftover processes. |
| closing the game's window (`taskkill /PID <slave>` without `/F`, which posts `WM_CLOSE`) | the slave quits; a second later the session sees no game window and closes the master, which waits out one 5 s multiboard timeout, then saves and quits by itself (about 6 s in all; the session allows 10 s): exit 0, records saved, 0 leftovers. |
| Wine's job objects | `IsProcessInJob` says the slave isn't in the session's job under Wine, so the slave is found as the master's child (ToolHelp), and the session's last kill ends the master's children too. A child counts only when it was created after its parent (`GetProcessTimes`): Windows keeps a dead parent's id on its children and reuses ids, so a program the player started from a long-gone process with the master's id is never taken for the game's. Rerun after that change: stop and window close as above, records saved, 0 leftovers. |
| which cores | Windows reports one efficiency class for both CCDs of the 7950X3D, so the tie goes to the lowest CPU ids (CCD0), where Linux picks the CCD it ranks fastest (CCD1). Either way both boards share one cache domain. |

## Quitting and leftovers

| Way to quit | SEGA Racing Classic | SEGA Rally 3 | OutRun 2 SP SDX |
|---|---|---|---|
| The launcher stops the game (`--seconds`, the window's Stop: SIGTERM to the session's process group, then SIGKILL after 10 s (8 s until the F355 Windows work), then the session tag sweep) | 0 leftover processes | 0 leftover processes | 0 leftover processes |
| Quit from the game menu | not applicable: arcade games have no quit; the test menu does not exit | not applicable | not applicable |
| Window close / Alt+F4 on the game | not verified yet (needs a visible session) | not verified yet | not verified yet |
| SIGINT/SIGTERM to the launcher (`--play` in a terminal, Ctrl+C) | the same stop path as the first row (signal-hook sets the stop flag): exit 0, 0 leftovers (2026-11-17) | not run separately yet | not run separately yet |

## After the move onto rea-launcher-sdk's hub modules (v0.10.0, 2026-11-17)

The launcher built from `refactor/hub-sdk` against the SDK's `feat/hub`
(c5a2430), with the v0.2.0 Linux package's components (same pins), the
owner's library and EmuDeck BIOS folder read-only, headless gamescope,
a private PipeWire null sink. Leftovers were looked for in the launcher's
own process tree (`ps -o pid,ppid,stat,comm` every second; after the end,
any of those pids still running under the same name).

| Run | Result |
|---|---|
| F355 Challenge 2, `--seconds 90` (the Stop path) | `taskset -c 8-15,24-31`, one window (the slave's), attract mode and "PRESS START BUTTON / FREE PLAY" at 80 s; at the stop the master was asked alone, "Slave exiting" ~1 s later, `f355twn2.zip.nvmem` written at the stop; launcher done 2–3 s after the stop, 0 leftovers. Run twice. |
| F355 Challenge 2, gamescope ended at 70 s and 75 s (the game's window closed) | "the game's window closed: stopping", records saved, launcher done 7 s later, 0 leftovers. Run twice. |
| SEGA Racing Classic, input script, `--seconds 150` | race played (lap 2 of 8, 168 km/h, steering both ways); its layer had `port: 38000`, the free port block; exit 0, 0 leftovers. |
| SEGA Racing Classic, SIGTERM to the launcher at 120 s (Ctrl+C) | the same stop path: the loader's input closed, exit 0 within 1 s, 0 leftovers (Wine's processes included). |
| OutRun 2 SP SDX, `--seconds 90` | attract mode ("2 Driver System", FREE PLAY); exit 0, 0 leftovers. |
| A game that can't start (a probe with a wrong environment) | "the game closed by itself 0 s after starting (exit status: 1)" with the log's reason line, also in `launcher.log`. |

Again with the SDK at 860e4da (the library bound `--ro-bind-try`, the
session's sentinel), 2026-11-17; the games' sound checked on the private
server (`pactl list short sink-inputs`: F355's two boards and SEGA Racing
Classic's Wine stream on its null sink, none on the desktop's):

| Run | Result |
|---|---|
| F355, Stop (`--seconds 90`) | attract mode, master asked alone, records saved; 0 leftovers |
| F355, the launcher killed (SIGKILL) at 70 s | the session's sentinel ended both boards and gamescope; 0 leftovers |
| SEGA Racing Classic, input script, Stop at 120 s | race played; exit 0; 0 leftovers |
| SEGA Racing Classic, the launcher killed at 90 s | Wine, the loader and gamescope ended with it; 0 leftovers |
| OutRun 2 SP SDX, Stop at 80 s | attract mode; the game crashed on the stop's SIGTERM (exit SIGSEGV); 0 leftovers. The game's own exit: Stop at 80 s, three times each, crashed 3 of 3 on this build, 3 of 3 on the SDK at a7bb340 and 2 of 3 with the released 0.2.0 launcher (its own process code). Its SRAM (`Jennifer/sram.bin`, the operator settings and records) is written at the stop, a second before the crash. |
| OutRun 2 SP SDX, the launcher killed at 60 s | 0 leftovers |

## Chihiro through xemu (`Tovarichtch/xemu` `Playtest-v2`, 2026-11-19)

The launcher packs each extracted TeknoParrot Chihiro dump into a FATX `.bin` netboot image (`sega-core::fatx::pack_dir`, read-only dump, skipped when unchanged) and starts `Tovarichtch/xemu` (`Playtest-v2`, GPL-2.0) with `xemu.toml` pointing at the player's BIOS folder (`chihiro_xbox_bios.bin`, `fpr21042_m29w160et.bin`, `ic10_g24lc64`, `ic11_24lc024.bin`, `pc20_g24lc64.bin`, `ver1305.bin`).

| Run | Result |
|---|---|
| Crazy Taxi High Roller (`--play cthr --render 3840x2160 --headless --seconds 42`) | FATX `.bin` packed (397.5 MiB), Type-1 board auto-selected, SEGABOOT Ver 2.13.0 -> Hitmaker / game boot, `surface_scale = 4`, `crtaxihr.sav` flushed on SIGTERM; exit 0, 0 leftovers. Note: the short `.mvs` attract movies (`Media/Movies/movie_*.mvs`) render black with audio and "FREE PLAY" on screen; 3D attract mode and gameplay render normally (verified in both OpenGL and Vulkan at 4x scale). |
| OutRun 2 SP Special Tours (`--play or2sp --render 3840x2160 --headless --seconds 46`) | FATX `.bin` packed (481.3 MiB), Type-3 (ASIC) board auto-selected with `ver1305.bin` (13.05), V850 booted, SEGABOOT -> game boot, `surface_scale = 4`, `outr2st.sav` flushed on SIGTERM; exit 0, 0 leftovers. |

## Gamescope WSI on NVIDIA + Wayland & F355 at 4K (2026-11-19)

| Run | Result |
|---|---|
| SEGA Rally 3, SEGA Racing Classic, Daytona Championship USA on visible Wayland (Hyprland + NVIDIA RTX 5090 + gamescope 3.17.0) | Before the fix, all three 32-bit DXVK games crashed gamescope (`SIGSEGV` after 3–6 s) with `timeline: SyncobjFdToHandle failed with: ret = -1: Invalid argument` / `vulkan: vkImportSemaphoreFdKHR failed (VkResult: -3)` right after `Gamescope WSI` created the swapchain. Setting `DISABLE_GAMESCOPE_WSI=1` on `Route::Wal` (Linux) prevents the WSI layer from loading (`sr3 --headless --seconds 25`: 0 `[Gamescope WSI]` lines, exit 0, 0 leftovers). Visible desktop check on the owner's monitor pending. |
| F355 Challenge 2 (`--play f355 --render 3840x2160 --headless --seconds 75`) | `gamescope -w 2880 -h 2160`, `rend.Resolution = 2160`, `rend.AnisotropicFiltering = 16`: both boards booted past both `System reset requested` cycles, saved cleanly on stop (`Slave exiting`), 0 leftovers. |

Since rea-launcher-sdk v0.11.2 the SDK's `gamescope::CommandLine` starts
every game with `ENABLE_GAMESCOPE_WSI=0 DISABLE_GAMESCOPE_WSI=1`, so the
launcher's own `DISABLE_GAMESCOPE_WSI=1` on `Route::Wal` was dropped: the
layer stays out for every route (Lindbergh and xemu draw with OpenGL,
which the Vulkan layer never touched).

Headless on SDK v0.11.2 (2026-11-19, release build, the 0.4.0 package's
components): `sr3 --headless --seconds 30` exit 0, 0 `[Gamescope WSI]`
lines, 0 leftovers; `cthr --headless --seconds 45` played the attract
(FREE PLAY), flushed `crtaxihr.sav`, 0 leftovers, but xemu ended with
`SIGSEGV` after the flush (`XIO: fatal IO error` once gamescope's reaper
ended its Xwayland). The released 0.4.0 does the same on this machine, so
it isn't this update's; the save is written first.

## Not verified yet

- Visible desktop check of `DISABLE_GAMESCOPE_WSI=1` on SEGA Rally 3, SEGA Racing Classic and Daytona Championship USA on the owner's NVIDIA + Hyprland display.
- Crazy Taxi High Roller and OutRun 2 SP Special Tours played with controls (pad and wheel/FFB) in `Tovarichtch/xemu`, and on real Windows.
- F355 Challenge 2 played with controls: a pad (Flycast's own layout, read by the master board off screen) and the keyboard (the slave forwards it); wheels aren't mapped for Flycast yet. A visible window and closing it.

- OutRun 2 SP SDX with a wheel: the launcher writes the wheel's SDL GUID and axes into lindbergh-loader's `controls.ini` (steering on the guessed axis, pedals by where they rest, `_INVERTED` when they rest at the top); not tried on hardware.
- Any wheel on hardware: steering lock, force feedback direction and strength, pedals on a second USB device (the owner's wheel).
- The Windows build of the launcher on real Windows; Steam Deck; multi-monitor; ultrawide. F355 on real Windows: the hidden master window, the focus going to the game's window, `SetProcessAffinityMask` on a hybrid Intel CPU.
- Free play in SEGA Rally 3 through `WAL_EUROPAR_FREEPLAY=1` (the race above used a coin).
- Render sizes above the cabinets' (SEGA Rally 3 at 1920x1080, SEGA Racing Classic and OutRun 2 SP SDX at 4K).
