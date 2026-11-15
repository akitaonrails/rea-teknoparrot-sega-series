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
| OutRun 2 SP SDX | without `SKIP_OUTRUN_CABINET_CHECK` the game stops 23 s in, after "Initializing the drive board" (the cabinet check fails); with it (the launcher's `lindbergh.ini`) it reaches the title screen and the attract mode and keeps running. **A race was not tried yet**: lindbergh-loader reads the controls itself, so the script input can't drive it; the next check sends keys to the session's Xwayland. |
| OutRun 2 SP SDX, output | lindbergh-loader buffers its stdout when it goes to a pipe: a crash loses its last lines, so a game that seems to stop "after loading the ranking" may have gone much further. Read the game's own state, not only the log. |

## Chihiro (Cxbx-Reloaded `CI-585c49a`, by hand, 2026-11-15)

| Step | Result |
|---|---|
| `cxbxr-ldr.exe /load <xbe> /chihiro` under GE-Proton's Wine | "Error loading cxbxr-emu.dll": needs the VC++ 2015–2022 runtime (`MSVCP140_ATOMIC_WAIT.dll`), installed with winetricks `vcrun2022` (it fails without a display; it works inside a headless gamescope) |
| then | `d3d9.dll`/`D3DCOMPILER_47.dll` fail to load: GE-Proton's 32-bit `wined3d` needs vkd3d's DLLs, which Proton's own launch script provides. DXVK's 32-bit `d3d9.dll` and vkd3d's three DLLs beside Cxbx (`d3d9=n`) fix it |
| then | Cxbx starts, patches the XBE (HLE), needs `boot.id` beside the XBE (keep it in the runtime), extracts SEGABOOT from the media board ROM and boots it |
| with zero-filled stand-ins for the three base-board EEPROMs (a probe only) | the screen stays black for two minutes. Needs the real `ic10_g24lc64.bin`, `pc20_g24lc64.bin`, `ic11_24lc024.bin` (MAME `chihiro`), which the owner's BIOS folder doesn't have yet. **Blocked.** |

## Daytona Championship USA (2026-11-15)

| Step | Result |
|---|---|
| TeknoParrot core (`TeknoMacaw.exe "<Daytona.exe>"`, as TeknoParrotUI starts it) under GE-Proton's Wine, bare metal | a message box "Sorry, this application cannot run under a Virtual Machine" (VMProtect). Not bypassed, by rule. |
| windowsarcadeloader `segashell` profile | being written |

## Quitting and leftovers

| Way to quit | SEGA Racing Classic | SEGA Rally 3 | OutRun 2 SP SDX |
|---|---|---|---|
| The launcher stops the game (`--seconds`, the window's Stop: SIGTERM to the session's process group, then SIGKILL after 8 s, then the session tag sweep) | 0 leftover processes | 0 leftover processes | 0 leftover processes |
| Quit from the game menu | not applicable: arcade games have no quit; the test menu does not exit | not applicable | not applicable |
| Window close / Alt+F4 on the game | not verified yet (needs a visible session) | not verified yet | not verified yet |
| SIGINT/SIGTERM to the launcher (`--play` in a terminal, Ctrl+C) | the same stop path as the first row (signal-hook sets the stop flag); not run separately yet | not run separately yet | not run separately yet |

## Not verified yet

- Any wheel on hardware: steering lock, force feedback direction and strength, pedals on a second USB device (the owner's wheel).
- The Windows build of the launcher on real Windows; Steam Deck; multi-monitor; ultrawide.
- Free play in SEGA Rally 3 through `WAL_EUROPAR_FREEPLAY=1` (the race above used a coin).
- Render sizes above the cabinets' (SEGA Rally 3 at 1920x1080, SEGA Racing Classic and OutRun 2 SP SDX at 4K).
