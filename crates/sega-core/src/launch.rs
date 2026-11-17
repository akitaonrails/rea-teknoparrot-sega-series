//! What to write and what to run for a game, as plain data.
//!
//! [`plan`] turns a game, its folders and the settings into the files the
//! launcher writes and the process it starts, so tests can check the whole
//! wiring (setting → file / argument / environment) without starting
//! anything. Every value here was seen to work in docs/RUNTIME_NOTES.md.

use std::path::{Path, PathBuf};

use crate::catalog::{Game, GameId, Host, Route};
use crate::input::StickLayout;
use crate::settings::{Settings, Size};

/// A process to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Files written (replaced) before every launch.
    pub files: Vec<(PathBuf, String)>,
    /// Symlinks made before every launch (`at` → `target`).
    pub links: Vec<(PathBuf, PathBuf)>,
    pub game: Command,
    /// Arguments for gamescope, which wraps the game.
    pub gamescope: Vec<String>,
    /// The launcher feeds the game's input through the loader's stdin.
    pub input_pipe: Option<StickLayout>,
    /// The size the game draws at.
    pub game_size: Size,
    /// A line in the game's output that means its boot hung: the session
    /// starts the game again.
    pub boot_hang: Option<&'static str>,
    /// Run the game on one cache domain of alike cores ([`uniform_cpus`]).
    pub one_cache: bool,
    /// A line in the game's output that means, once a stop was asked for,
    /// that the game has saved and what's left can be ended.
    pub stopped: Option<&'static str>,
    /// Windows: the program the session starts opens a window that isn't
    /// the game's (F355's master board). The session hides it, gives the
    /// other window the focus, and ends the game when that window closes.
    pub one_window: bool,
}

/// The launcher's own components (built or downloaded on request).
#[derive(Debug, Clone)]
pub struct Components {
    /// lindbergh-loader's build: `lindbergh`, `lindbergh.so`, its 32-bit libraries.
    pub lindbergh: PathBuf,
    /// windowsarcadeloader: `arcade-launcher`, `payloads/`, `systemprofiles/`.
    pub wal: PathBuf,
    /// The Wine runner windowsarcadeloader runs games with (`bin/wine`).
    pub runner: PathBuf,
    /// The winetricks script windowsarcadeloader installs native XAudio with
    /// (looked for beside the runner, else on PATH).
    pub winetricks: PathBuf,
    /// winetricks' download cache with Microsoft's DirectX June 2010
    /// redistributable in it ([`DIRECTX_REDIST`]), so the XACT install
    /// needs no network (Linux).
    pub directx: PathBuf,
    /// Flycast: on Linux its AppImage unpacked ([`FLYCAST_PROGRAM`] and the
    /// libraries beside it), on Windows its one program ([`FLYCAST_EXE`]).
    pub flycast: PathBuf,
    /// This launcher's own program: Flycast starts each NAOMI board through
    /// it ([`flycast_board`]).
    pub launcher: PathBuf,
}

/// Flycast's program inside [`Components::flycast`].
pub const FLYCAST_PROGRAM: &str = "usr/bin/flycast";

/// Flycast's Windows program inside [`Components::flycast`]. It keeps its
/// settings beside itself (`emu.cfg`, `data/`), so each game's runtime
/// gets its own copy.
pub const FLYCAST_EXE: &str = "flycast.exe";

/// The variable that makes the launcher's program start a Flycast board
/// (its value: Flycast's program) instead of the launcher.
pub const FLYCAST_ENV: &str = "SEGA_FLYCAST";

/// What Flycast prints when F355's linked boards fail to find each other at
/// boot (flycast#1909, #1630: the game then waits forever at "being
/// named..."); the session starts the game again.
pub const FLYCAST_BOOT_HANG: &str = "TRAP #50";

/// What F355's slave board prints once its master has quit. The master
/// saves the cabinet's memory (records, operator settings) when it stops,
/// before it lets the slave go; the slave then stays on Flycast's error
/// screen, so the session ends it at this line instead of waiting.
pub const FLYCAST_SAVED: &str = "Slave exiting";

/// The DirectX redistributable inside [`Components::directx`], where
/// winetricks' `xact` verb looks for it (`<W_CACHE>/directx9/`).
pub const DIRECTX_REDIST: &str = "directx9/directx_Jun2010_redist.exe";

/// Where the launcher keeps per-game state.
#[derive(Debug, Clone)]
pub struct Places {
    /// The game's runtime folder.
    pub runtime: PathBuf,
    /// windowsarcadeloader's root (shared by every Windows game: prefix, links).
    pub wal_root: PathBuf,
    /// The player's wheel, for the loaders that read it themselves.
    pub wheel: Option<LindberghWheel>,
    /// The board BIOS files the player's BIOS folder holds for this game
    /// (F355's `f355bios.zip`), linked where the emulator reads them.
    pub bios: Vec<PathBuf>,
    /// Windows: the local UDP port the session reads Flycast's log on
    /// (its `LogServer`; the Windows build has no output to read).
    pub log_port: Option<u16>,
}

/// How this machine reaches the network (lindbergh-loader's `NIC_NAME`):
/// the default route's interface, else loopback.
pub fn default_interface(proc_net_route: &str) -> String {
    proc_net_route
        .lines()
        .skip(1)
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let (name, dest) = (f.next()?, f.next()?);
            (dest == "00000000" && name != "lo").then(|| name.to_string())
        })
        .next()
        .unwrap_or_else(|| "lo".into())
}

/// The size the game draws at: the setting, within what the game can do.
/// SEGA Rally 3 draws at 1280x720 (the cabinet's) or 1920x1080
/// (windowsarcadeloader's `WAL_EUROPAR_RESOLUTION`), nothing else;
/// Daytona Championship USA always at its own 1920x1080.
pub fn game_size(game: &Game, s: &Settings) -> Size {
    let want = s.render.unwrap_or(game.native);
    match game.id {
        GameId::SegaRally3 if want.h >= 1080 => Size::new(1920, 1080),
        GameId::SegaRally3 => Size::new(1280, 720),
        // its profile has no resolution setting: the game's own 1920x1080
        GameId::Daytona => game.native,
        // only the cabinet's 640x480 was checked in Flycast
        GameId::F355Challenge2 => game.native,
        _ => want,
    }
}

fn gamescope_args(s: &Settings, size: Size, headless: bool) -> Vec<String> {
    let mut a: Vec<String> = [
        "-w",
        &size.w.to_string(),
        "-h",
        &size.h.to_string(),
        "-W",
        &s.output.w.to_string(),
        "-H",
        &s.output.h.to_string(),
        "-r",
        "60",
        "-F",
        if headless { "linear" } else { s.upscaler.key() },
    ]
    .iter()
    .map(|x| x.to_string())
    .collect();
    if !headless && matches!(s.upscaler.key(), "fsr" | "nis") {
        a.extend(["--sharpness".into(), s.sharpness.to_string()]);
    }
    if headless {
        a.extend(["--backend".into(), "headless".into()]);
    } else if s.fullscreen {
        a.push("-f".into());
    }
    a
}

/// lindbergh-loader's `lindbergh.ini`. OutRun 2 SP SDX stops at its
/// cabinet check (drive board, motion tower) about 23 s in unless
/// `SKIP_OUTRUN_CABINET_CHECK` is on (docs/RUNTIME_NOTES.md).
pub fn lindbergh_ini(size: Size, nic: &str) -> String {
    format!(
        "# Written by the launcher before every start.\n\
         [Display]\nWIDTH = {w}\nHEIGHT = {h}\nBOOST_RENDER_RES = true\nFULLSCREEN = true\n\
         KEEP_ASPECT_RATIO = true\nHIDE_CURSOR = true\nBORDER_ENABLED = false\n\
         [Input]\nINPUT_MODE = 0\n\
         [Emulation]\nREGION = EX\nFREEPLAY = true\nEMULATE_JVS = true\nEMULATE_DRIVEBOARD = AUTO\n\
         EMULATE_MOTIONBOARD = AUTO\n\
         [GameSpecific]\nSKIP_OUTRUN_CABINET_CHECK = true\n\
         [System]\nDEBUG_MSGS = false\n\
         [Paths]\nSRAM_PATH = \"sram.bin\"\nEEPROM_PATH = \"eeprom.bin\"\n\
         [Graphics]\nGPU_VENDOR = 0\nFPS_LIMITER_ENABLED = true\nFPS_TARGET = 60.0\n\
         [Network]\nNIC_NAME = \"{nic}\"\n",
        w = size.w,
        h = size.h
    )
}

/// The player's wheel as lindbergh-loader reads it (SDL joystick axes):
/// the launcher finds it before the game starts, from the SDK's guess
/// (`WheelProfile::guess`: steering on axis 0, pedals where they rest).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LindberghWheel {
    /// SDL's GUID string: lindbergh-loader's `[ControllerGUIDs] P1_GUID`
    /// makes this device player 1 (its `JOY0`).
    pub guid: String,
    pub steer_axis: usize,
    /// A pedal axis and whether it rests at the top of its range (it then
    /// reads `_INVERTED`).
    pub gas: Option<(usize, bool)>,
    pub brake: Option<(usize, bool)>,
    /// The paddles' button numbers, when the wheel is a known model.
    pub paddles: Option<(u32, u32)>,
}

/// lindbergh-loader's `controls.ini`: the launcher's pad layout for SDL game
/// controllers, the keyboard as a fallback, and the player's wheel when one
/// was found (OutRun 2 SP SDX's drive board force feedback then goes to it).
pub fn lindbergh_controls(wheel: Option<&LindberghWheel>) -> String {
    let axis =
        |a: (usize, bool)| format!(", JOY0_AXIS_{}{}", a.0, if a.1 { "_INVERTED" } else { "" });
    let (steer, gas, brake, up, down, guids) = match wheel {
        Some(w) => (
            format!(", JOY0_AXIS_{}", w.steer_axis),
            w.gas.map(axis).unwrap_or_default(),
            w.brake.map(axis).unwrap_or_default(),
            w.paddles
                .map(|p| format!(", JOY0_BUTTON_{}", p.0))
                .unwrap_or_default(),
            w.paddles
                .map(|p| format!(", JOY0_BUTTON_{}", p.1))
                .unwrap_or_default(),
            format!("[ControllerGUIDs]\nP1_GUID = {}\n", w.guid),
        ),
        None => Default::default(),
    };
    format!(
        "# Written by the launcher before every start.\n\
         {guids}\
         [Common]\n\
         Test = KEY_T\n\
         P1_Coin = KEY_5, GC0_BUTTON_BACK\n\
         P1_Start = KEY_1, GC0_BUTTON_START, GC0_BUTTON_A\n\
         P1_Service = KEY_S\n\
         [Driving]\n\
         P1_Steer_Left = KEY_Left\n\
         P1_Steer_Right = KEY_Right\n\
         P1_Gas_Digital = KEY_Up\n\
         P1_Brake_Digital = KEY_Down\n\
         ViewChange = KEY_V, GC0_BUTTON_Y\n\
         MusicChange = KEY_M, GC0_BUTTON_X\n\
         GearUp = KEY_A, GC0_BUTTON_RIGHTSHOULDER{up}\n\
         GearDown = KEY_Z, GC0_BUTTON_LEFTSHOULDER{down}\n\
         P1_Steer = GC0_AXIS_LEFTX{steer}\n\
         P1_Gas = GC0_AXIS_RIGHTTRIGGER{gas}\n\
         P1_Brake = GC0_AXIS_LEFTTRIGGER{brake}\n"
    )
}

/// The profile layer the launcher gives windowsarcadeloader for one game:
/// the runner, a prefix with native XACT (the RingEdge games' sound thread
/// crashes on Wine's own), no ReShade or gamescope of its own (the launcher
/// wraps the game), and the render size.
pub fn wal_layer(game: &Game, size: Size, ffb: u8) -> String {
    let mut env = Vec::new();
    match game.route {
        Route::Wal {
            system: "ringedge", ..
        } => env.push(("WAL_RINGEDGE_GL_SCALE", format!("{}x{}", size.w, size.h))),
        Route::Wal {
            system: "europar", ..
        } => {
            env.push(("WAL_EUROPAR_RESOLUTION", format!("{}x{}", size.w, size.h)));
            // free play: the cabinet's operator setting, as on the other games
            env.push(("WAL_EUROPAR_FREEPLAY", "1".into()));
            env.push(("WAL_EUROPAR_FFB", ffb.to_string()));
        }
        Route::Wal {
            system: "segashell",
            ..
        } => env.push(("WAL_SEGASHELL_FFB", ffb.to_string())),
        _ => {}
    }
    let mut out = String::from(
        "# Written by the launcher before every start.\n\
         runner: runner\nprefix: wine-prefix/sega\nprefix_tricks: [xact]\nreshade: false\n\
         gamescope:\n  enabled: false\n",
    );
    if !env.is_empty() {
        out.push_str("env:\n");
        for (k, v) in env {
            out.push_str(&format!("  {k}: \"{v}\"\n"));
        }
    }
    out
}

/// The BIOS files a game needs from the player's BIOS folder (MAME's
/// names): F355's NAOMI board BIOS.
pub fn bios_names(game: &Game) -> &'static [&'static str] {
    match game.route {
        Route::Flycast { .. } => &["f355bios.zip"],
        _ => &[],
    }
}

/// Where a BIOS file is looked for, in order: the BIOS folder, the board
/// folders EmuDeck and MAME front ends keep inside it, then the game's own
/// folder and the folder holding it.
pub fn bios_places(bios: Option<&Path>, game_dir: &Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(b) = bios {
        v.push(b.to_path_buf());
        for sub in ["dc", "naomi", "chihiro"] {
            v.push(b.join(sub));
        }
    }
    v.push(game_dir.to_path_buf());
    v.extend(game_dir.parent().map(Path::to_path_buf));
    v
}

/// Which NAOMI board a Flycast command line starts. Flycast runs a linked
/// cabinet as one process per board: the master starts each slave by
/// running its own program (`argv[0]`) again with
/// `-config naomi:BoardId=<n>`, and the slave finds the master by its
/// parent's process id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Board {
    Master,
    Slave,
}

pub fn flycast_board(args: &[String]) -> Board {
    if args.iter().any(|a| a.starts_with("naomi:BoardId=")) {
        Board::Slave
    } else {
        Board::Master
    }
}

/// The environment each board runs with. In F355 Challenge Twin the slave
/// board draws the game and the master shows only the BIOS's screens, so
/// the master runs off screen (still reading the controls) and the slave
/// gets the session's one window (docs/RUNTIME_NOTES.md).
pub fn board_env(board: Board) -> &'static [(&'static str, &'static str)] {
    match board {
        Board::Master => &[
            ("SDL_VIDEODRIVER", "offscreen"),
            ("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS", "1"),
        ],
        Board::Slave => &[("SDL_VIDEODRIVER", "x11")],
    }
}

/// Flycast's `emu.cfg`: the render height and the window the slave board
/// opens. On Linux gamescope scales a plain window to the screen; on
/// Windows the window covers the screen itself when `fullscreen` (Flycast's
/// borderless desktop size), and each board sends its log to `log_port`.
pub fn flycast_cfg(size: Size, fullscreen: bool, log_port: Option<u16>) -> String {
    let mut s = format!(
        "# Written by the launcher before every start.\n\
         [config]\nrend.Resolution = {h}\n\
         [window]\nfullscreen = {f}\nmaximized = no\nwidth = {w}\nheight = {h}\n",
        w = size.w,
        h = size.h,
        f = if fullscreen { "yes" } else { "no" },
    );
    if let Some(port) = log_port {
        s.push_str(&format!(
            "[log]\nLogToConsole = no\nLogToFile = no\nLogServer = 127.0.0.1:{port}\n"
        ));
    }
    s
}

/// A CPU as Linux describes it in `/sys/devices/system/cpu/cpu<id>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cpu {
    pub id: u32,
    /// `acpi_cppc/highest_perf`, else `cpufreq/cpuinfo_max_freq`.
    pub rank: u64,
    /// The CPUs sharing its last-level cache (`cache/index3/shared_cpu_list`).
    pub l3: Vec<u32>,
}

/// Parses a Linux CPU list (`0-7,16-23`).
pub fn cpu_list(text: &str) -> Vec<u32> {
    let mut v = Vec::new();
    for part in text.trim().split(',').filter(|p| !p.is_empty()) {
        match part.split_once('-') {
            Some((a, b)) => {
                if let (Ok(a), Ok(b)) = (a.trim().parse::<u32>(), b.trim().parse::<u32>()) {
                    v.extend(a..=b);
                }
            }
            None => v.extend(part.trim().parse::<u32>().ok()),
        }
    }
    v
}

/// The CPUs F355's boards should share: the cache domain of the CPU Linux
/// ranks fastest, and on a hybrid Intel CPU only its performance cores
/// (`core`, from `/sys/devices/cpu_core/cpus`). Flycast's linked boards
/// often fail to boot when they run on cores of different speeds
/// (flycast#1630; on a Ryzen 9 7950X3D, 1 boot in 6 unpinned, 3 in 4 on
/// one CCD). None when every CPU is alike or too few remain.
pub fn uniform_cpus(cpus: &[Cpu], core: Option<&[u32]>) -> Option<Vec<u32>> {
    let best = cpus
        .iter()
        .max_by_key(|c| (c.rank, std::cmp::Reverse(c.id)))?;
    let mut set: Vec<u32> = cpus
        .iter()
        .map(|c| c.id)
        .filter(|id| best.l3.contains(id))
        .filter(|id| core.is_none_or(|core| core.contains(id)))
        .collect();
    set.sort_unstable();
    (set.len() >= 4 && set.len() < cpus.len()).then_some(set)
}

/// The CPUs as Windows describes them: the buffer
/// `GetLogicalProcessorInformationEx(RelationAll)` fills, read for
/// processor group 0 (`Cpu::rank` is the core's efficiency class). The
/// second value is the performance cores of a hybrid CPU, for
/// [`uniform_cpus`]'s `core`; None when every core is alike.
pub fn windows_cpus(buf: &[u8]) -> (Vec<Cpu>, Option<Vec<u32>>) {
    const CORE: u32 = 0; // RelationProcessorCore
    const CACHE: u32 = 2; // RelationCache
    let u16_at = |i: usize| buf.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32_at = |i: usize| {
        buf.get(i..i + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    let u64_at = |i: usize| {
        buf.get(i..i + 8)
            .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
    };
    let bits = |mask: u64| (0..64).filter(move |b| mask & (1 << b) != 0);
    // (efficiency class, CPUs) per core; the CPUs of each level-3 cache
    let mut cores: Vec<(u8, u64)> = Vec::new();
    let mut l3s: Vec<u64> = Vec::new();
    let mut at = 0;
    while let (Some(kind), Some(size)) = (u32_at(at), u32_at(at + 4)) {
        let size = size as usize;
        if size == 0 {
            break;
        }
        match kind {
            // PROCESSOR_RELATIONSHIP: EfficiencyClass at 9, GroupMask at 32
            CORE if u16_at(at + 40) == Some(0) => {
                if let (Some(class), Some(mask)) = (buf.get(at + 9), u64_at(at + 32)) {
                    cores.push((*class, mask));
                }
            }
            // CACHE_RELATIONSHIP: Level at 8, GroupMask at 40
            CACHE if buf.get(at + 8) == Some(&3) && u16_at(at + 48) == Some(0) => {
                l3s.extend(u64_at(at + 40));
            }
            _ => {}
        }
        at += size;
    }
    let mut cpus: Vec<Cpu> = cores
        .iter()
        .flat_map(|&(class, mask)| {
            let l3 = l3s
                .iter()
                .copied()
                .find(|l3| l3 & mask != 0)
                .unwrap_or(mask);
            bits(mask).map(move |id| Cpu {
                id,
                rank: class as u64,
                l3: bits(l3).collect(),
            })
        })
        .collect();
    cpus.sort_by_key(|c| c.id);
    let top = cpus.iter().map(|c| c.rank).max();
    let core = (cpus.iter().any(|c| Some(c.rank) != top)).then(|| {
        cpus.iter()
            .filter(|c| Some(c.rank) == top)
            .map(|c| c.id)
            .collect()
    });
    (cpus, core)
}

/// Why a game can't be planned yet (a route still being brought up).
pub type NotYet = String;

pub fn plan(
    game: &Game,
    places: &Places,
    c: &Components,
    s: &Settings,
    nic: &str,
    headless: bool,
    host: Host,
) -> Result<Plan, NotYet> {
    let size = game_size(game, s);
    let rt = &places.runtime;
    let mut files = Vec::new();
    let mut links = Vec::new();
    let mut input_pipe = None;
    let game_cmd = match game.route {
        Route::Lindbergh => {
            files.push((rt.join("lindbergh.ini"), lindbergh_ini(size, nic)));
            files.push((
                rt.join("controls.ini"),
                lindbergh_controls(places.wheel.as_ref()),
            ));
            Command {
                // the loader finds lindbergh.so next to the folder it starts in
                program: c.lindbergh.join("lindbergh"),
                args: vec![
                    "-c".into(),
                    rt.join("lindbergh.ini").to_string_lossy().into_owned(),
                    "-o".into(),
                    rt.join("controls.ini").to_string_lossy().into_owned(),
                    "-g".into(),
                    rt.join(game.start_folder()).to_string_lossy().into_owned(),
                ],
                env: vec![(
                    "LD_LIBRARY_PATH".into(),
                    format!("{}:{}", c.lindbergh.display(), rt.join("lib").display()),
                )],
                cwd: c.lindbergh.clone(),
            }
        }
        Route::Wal { system, profile } => {
            let root = &places.wal_root;
            files.push((
                rt.join(format!("{profile}.windowsloader")),
                format!("{}\n", game.loader_path()),
            ));
            let layer = root.join(format!("layer-{}.yaml", game.slug));
            files.push((layer.clone(), wal_layer(game, size, s.force_feedback)));
            links.push((root.join("systemprofiles"), c.wal.join("systemprofiles")));
            links.push((root.join("dist/payloads"), c.wal.join("payloads")));
            if host == Host::Linux {
                links.push((root.join("wine-runners/runner"), c.runner.clone()));
                links.push((root.join("wine-runners/winetricks"), c.winetricks.clone()));
            }
            input_pipe = Some(match system {
                "ringedge" => StickLayout::HPattern4,
                "segashell" => StickLayout::Shell,
                _ => StickLayout::Sequential,
            });
            Command {
                // Windows: windowsarcadeloader's own run without Wine (patches/wal)
                program: c.wal.join(match host {
                    Host::Linux => "arcade-launcher",
                    Host::Windows => "arcade-launcher.exe",
                }),
                args: vec![
                    "run".into(),
                    rt.to_string_lossy().into_owned(),
                    "--root".into(),
                    root.to_string_lossy().into_owned(),
                    "--profile".into(),
                    layer.to_string_lossy().into_owned(),
                    "--input-pipe".into(),
                ],
                // winetricks (run by windowsarcadeloader) takes the DirectX
                // redistributable for `xact` from the package, not the network
                env: match host {
                    Host::Linux => {
                        vec![("W_CACHE".into(), c.directx.to_string_lossy().into_owned())]
                    }
                    Host::Windows => vec![],
                },
                cwd: root.clone(),
            }
        }
        // Flycast's Windows build starts its boards by its own path: a copy
        // in the runtime keeps its settings and saves there
        Route::Flycast { set } if host == Host::Windows => {
            let home = rt.join("flycast");
            let Some(port) = places.log_port else {
                return Err(format!("{} needs a port for Flycast's log.", game.title));
            };
            files.push((
                home.join("emu.cfg"),
                flycast_cfg(size, s.fullscreen, Some(port)),
            ));
            links.push((home.join(FLYCAST_EXE), c.flycast.join(FLYCAST_EXE)));
            for b in &places.bios {
                if let Some(name) = b.file_name() {
                    links.push((home.join("data").join(name), b.clone()));
                }
            }
            Command {
                program: home.join(FLYCAST_EXE),
                args: vec![rt.join(format!("{set}.zip")).to_string_lossy().into_owned()],
                // the master board reads the controls while its window is hidden
                env: vec![("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS".into(), "1".into())],
                cwd: rt.clone(),
            }
        }
        Route::Flycast { set } => {
            let config = rt.join("flycast/config");
            let data = rt.join("flycast/data");
            files.push((
                config.join("flycast/emu.cfg"),
                flycast_cfg(size, false, None),
            ));
            for b in &places.bios {
                if let Some(name) = b.file_name() {
                    links.push((data.join("flycast").join(name), b.clone()));
                }
            }
            Command {
                // the launcher's program starts each board (flycast_board)
                program: c.launcher.clone(),
                args: vec![rt.join(format!("{set}.zip")).to_string_lossy().into_owned()],
                env: vec![
                    (
                        FLYCAST_ENV.into(),
                        c.flycast
                            .join(FLYCAST_PROGRAM)
                            .to_string_lossy()
                            .into_owned(),
                    ),
                    (
                        "XDG_CONFIG_HOME".into(),
                        config.to_string_lossy().into_owned(),
                    ),
                    ("XDG_DATA_HOME".into(), data.to_string_lossy().into_owned()),
                ],
                cwd: rt.clone(),
            }
        }
        Route::Cxbx => {
            return Err(format!("{} can't be started yet.", game.title));
        }
    };
    // F355's linked boards: restart a hung boot, alike cores, stop once saved
    let flycast = matches!(game.route, Route::Flycast { .. });
    Ok(Plan {
        files,
        links,
        game: game_cmd,
        gamescope: match host {
            Host::Linux => gamescope_args(s, size, headless),
            Host::Windows => Vec::new(),
        },
        input_pipe,
        game_size: size,
        boot_hang: flycast.then_some(FLYCAST_BOOT_HANG),
        one_cache: flycast,
        stopped: flycast.then_some(FLYCAST_SAVED),
        one_window: flycast && host == Host::Windows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{GameId, game};

    fn comps() -> Components {
        Components {
            lindbergh: "/c/lindbergh".into(),
            wal: "/c/wal".into(),
            runner: "/c/runner".into(),
            winetricks: "/c/winetricks".into(),
            directx: "/c/directx".into(),
            flycast: "/c/flycast".into(),
            launcher: "/pkg/sega-launcher".into(),
        }
    }

    fn places(slug: &str) -> Places {
        Places {
            runtime: PathBuf::from("/d/runtimes").join(slug),
            wal_root: "/d/wal".into(),
            wheel: None,
            bios: vec![],
            log_port: None,
        }
    }

    fn plan_for(id: GameId, s: &Settings, headless: bool, host: Host) -> Plan {
        let g = game(id);
        plan(g, &places(g.slug), &comps(), s, "eth0", headless, host).unwrap()
    }

    fn arg_after(args: &[String], flag: &str) -> Option<String> {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1).cloned())
    }

    fn file<'a>(p: &'a Plan, name: &str) -> &'a str {
        &p.files.iter().find(|(f, _)| f.ends_with(name)).unwrap().1
    }

    #[test]
    fn the_default_route_interface_is_picked() {
        let route = "Iface\tDestination\tGateway\n\
                     lo\t0000007F\t00000000\n\
                     eth0\t00000000\t010011AC\n";
        assert_eq!(default_interface(route), "eth0");
        assert_eq!(default_interface("Iface\tDestination\n"), "lo");
    }

    #[test]
    fn racing_classic_render_size_reaches_the_payload_and_gamescope() {
        let s = Settings {
            render: Some(Size::new(3840, 2160)),
            ..Settings::default()
        };
        let p = plan_for(GameId::RacingClassic, &s, false, Host::Linux);
        let layer = file(&p, "layer-src.yaml");
        assert!(
            layer.contains("WAL_RINGEDGE_GL_SCALE: \"3840x2160\""),
            "{layer}"
        );
        assert!(layer.contains("prefix_tricks: [xact]"));
        assert_eq!(arg_after(&p.gamescope, "-w").as_deref(), Some("3840"));
        assert_eq!(p.input_pipe, Some(StickLayout::HPattern4));
        assert!(p.game.args.contains(&"--input-pipe".to_string()));
        assert_eq!(file(&p, "sega-racing-classic.windowsloader"), "d1a.exe\n");
    }

    #[test]
    fn sega_rally_starts_its_program_in_the_rally_folder() {
        let p = plan_for(GameId::SegaRally3, &Settings::default(), false, Host::Linux);
        assert_eq!(file(&p, "sega-rally-3.windowsloader"), "Rally\\Rally.exe\n");
        // windowsarcadeloader looks for winetricks beside its runner
        let runners = PathBuf::from("/d/wal/wine-runners");
        assert!(
            p.links
                .contains(&(runners.join("runner"), "/c/runner".into()))
        );
        assert!(
            p.links
                .contains(&(runners.join("winetricks"), "/c/winetricks".into()))
        );
        // winetricks finds the bundled DirectX redistributable, offline
        assert!(
            p.game
                .env
                .contains(&("W_CACHE".into(), "/c/directx".into()))
        );
        let w = plan_for(
            GameId::SegaRally3,
            &Settings::default(),
            false,
            Host::Windows,
        );
        assert!(w.game.env.iter().all(|(k, _)| k != "W_CACHE"));
    }

    #[test]
    fn lindbergh_gets_its_size_interface_and_controls() {
        let s = Settings {
            render: Some(Size::new(2560, 1440)),
            ..Settings::default()
        };
        let p = plan_for(GameId::OutRun2SpSdx, &s, false, Host::Linux);
        let ini = file(&p, "lindbergh.ini");
        assert!(ini.contains("WIDTH = 2560\nHEIGHT = 1440"));
        assert!(ini.contains("NIC_NAME = \"eth0\""));
        assert!(ini.contains("GPU_VENDOR = 0"));
        assert!(ini.contains("SKIP_OUTRUN_CABINET_CHECK = true"));
        assert_eq!(p.game.cwd, PathBuf::from("/c/lindbergh"));
        let start = places("or2sdx").runtime.join("Jennifer");
        assert_eq!(
            arg_after(&p.game.args, "-g"),
            Some(start.to_string_lossy().into_owned())
        );
        assert!(p.input_pipe.is_none());
    }

    #[test]
    fn a_wheel_found_before_the_start_reaches_lindbergh_controls() {
        let pad_only = lindbergh_controls(None);
        assert!(pad_only.contains("P1_Steer = GC0_AXIS_LEFTX\n"));
        assert!(!pad_only.contains("JOY0") && !pad_only.contains("ControllerGUIDs"));
        let w = LindberghWheel {
            guid: "030000006d04000067c2000011010000".into(),
            steer_axis: 0,
            gas: Some((2, true)),
            brake: Some((3, false)),
            paddles: Some((4, 5)),
        };
        let mut places = places("or2sdx");
        places.wheel = Some(w.clone());
        let p = plan(
            game(GameId::OutRun2SpSdx),
            &places,
            &comps(),
            &Settings::default(),
            "eth0",
            false,
            Host::Linux,
        )
        .unwrap();
        let ini = file(&p, "controls.ini");
        assert!(
            ini.contains("[ControllerGUIDs]\nP1_GUID = 030000006d04000067c2000011010000\n"),
            "{ini}"
        );
        assert!(ini.contains("P1_Steer = GC0_AXIS_LEFTX, JOY0_AXIS_0\n"));
        assert!(ini.contains("P1_Gas = GC0_AXIS_RIGHTTRIGGER, JOY0_AXIS_2_INVERTED\n"));
        assert!(ini.contains("P1_Brake = GC0_AXIS_LEFTTRIGGER, JOY0_AXIS_3\n"));
        assert!(ini.contains("GearUp = KEY_A, GC0_BUTTON_RIGHTSHOULDER, JOY0_BUTTON_4\n"));
    }

    #[test]
    fn upscaler_and_headless_reach_gamescope() {
        let p = plan_for(GameId::SegaRally3, &Settings::default(), false, Host::Linux);
        assert_eq!(arg_after(&p.gamescope, "-F").as_deref(), Some("fsr"));
        assert_eq!(arg_after(&p.gamescope, "--sharpness").as_deref(), Some("5"));
        assert!(p.gamescope.contains(&"-f".to_string()));
        let h = plan_for(GameId::SegaRally3, &Settings::default(), true, Host::Linux);
        assert_eq!(
            arg_after(&h.gamescope, "--backend").as_deref(),
            Some("headless")
        );
        assert_eq!(arg_after(&h.gamescope, "-F").as_deref(), Some("linear"));
        assert!(!h.gamescope.contains(&"-f".to_string()));
    }

    #[test]
    fn sega_rally_draws_at_one_of_its_two_sizes() {
        for (want, got) in [
            (Some(Size::new(3840, 2160)), Size::new(1920, 1080)),
            (Some(Size::new(1600, 900)), Size::new(1280, 720)),
            (None, Size::new(1280, 720)),
        ] {
            let s = Settings {
                render: want,
                force_feedback: 150,
                ..Settings::default()
            };
            let p = plan_for(GameId::SegaRally3, &s, false, Host::Linux);
            assert_eq!(p.game_size, got);
            let layer = file(&p, "layer-sr3.yaml");
            assert!(
                layer.contains(&format!("WAL_EUROPAR_RESOLUTION: \"{got}\"")),
                "{layer}"
            );
            assert!(layer.contains("WAL_EUROPAR_FFB: \"150\""), "{layer}");
            assert!(layer.contains("WAL_EUROPAR_FREEPLAY: \"1\""), "{layer}");
        }
    }

    #[test]
    fn on_windows_windowsarcadeloader_runs_its_windows_build_without_wine() {
        let p = plan_for(
            GameId::SegaRally3,
            &Settings::default(),
            false,
            Host::Windows,
        );
        assert_eq!(p.game.program, PathBuf::from("/c/wal/arcade-launcher.exe"));
        assert!(p.game.args.contains(&"--input-pipe".to_string()));
        let runners = PathBuf::from("/d/wal/wine-runners");
        assert!(
            !p.links.iter().any(|(at, _)| at.starts_with(&runners)),
            "no Wine runner"
        );
        assert!(
            p.links
                .contains(&("/d/wal/dist/payloads".into(), "/c/wal/payloads".into()))
        );
        assert!(p.gamescope.is_empty());
    }

    #[test]
    fn f355_starts_its_master_board_through_the_launcher_with_the_bios_linked() {
        let g = game(GameId::F355Challenge2);
        let mut pl = places(g.slug);
        pl.bios = vec!["/bios/dc/f355bios.zip".into()];
        let s = Settings {
            render: Some(Size::new(3840, 2160)),
            ..Settings::default()
        };
        let p = plan(g, &pl, &comps(), &s, "eth0", false, Host::Linux).unwrap();
        let rt = PathBuf::from("/d/runtimes/f355");
        assert_eq!(p.game.program, PathBuf::from("/pkg/sega-launcher"));
        assert_eq!(
            p.game.args,
            vec!["/d/runtimes/f355/f355twn2.zip".to_string()]
        );
        let env = |k: &str| {
            p.game
                .env
                .iter()
                .find(|(n, _)| n == k)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(
            env(FLYCAST_ENV).as_deref(),
            Some("/c/flycast/usr/bin/flycast")
        );
        assert_eq!(
            env("XDG_DATA_HOME").as_deref(),
            Some("/d/runtimes/f355/flycast/data")
        );
        assert_eq!(
            env("XDG_CONFIG_HOME").as_deref(),
            Some("/d/runtimes/f355/flycast/config")
        );
        assert!(p.links.contains(&(
            rt.join("flycast/data/flycast/f355bios.zip"),
            "/bios/dc/f355bios.zip".into()
        )));
        // the cabinet's size whatever the setting: only 640x480 was checked
        assert_eq!(p.game_size, Size::new(640, 480));
        assert_eq!(arg_after(&p.gamescope, "-w").as_deref(), Some("640"));
        let cfg = file(&p, "flycast/config/flycast/emu.cfg");
        assert!(cfg.contains("rend.Resolution = 480\n"), "{cfg}");
        assert!(cfg.contains("width = 640\nheight = 480\n"), "{cfg}");
        assert_eq!(p.boot_hang, Some(FLYCAST_BOOT_HANG));
        assert!(p.one_cache);
        assert_eq!(p.stopped, Some(FLYCAST_SAVED));
        assert!(p.input_pipe.is_none());
        assert!(!p.one_window, "gamescope shows only the slave's window");
        assert!(!cfg.contains("LogServer"), "{cfg}");
        let other = plan_for(GameId::SegaRally3, &Settings::default(), false, Host::Linux);
        assert!(other.boot_hang.is_none() && !other.one_cache && other.stopped.is_none());
        assert!(!other.one_window);
    }

    /// A `GetLogicalProcessorInformationEx` buffer: cores as (efficiency
    /// class, CPU mask), level-3 caches as CPU masks.
    fn slpi(cores: &[(u8, u64)], l3s: &[u64]) -> Vec<u8> {
        let mut b = Vec::new();
        for &(class, mask) in cores {
            let mut r = vec![0u8; 48];
            r[4..8].copy_from_slice(&48u32.to_le_bytes());
            r[9] = class;
            r[30..32].copy_from_slice(&1u16.to_le_bytes());
            r[32..40].copy_from_slice(&mask.to_le_bytes());
            b.extend(r);
        }
        for &mask in l3s {
            // a level-1 cache first, which doesn't count
            for level in [1u8, 3] {
                let mut r = vec![0u8; 56];
                r[0..4].copy_from_slice(&2u32.to_le_bytes());
                r[4..8].copy_from_slice(&56u32.to_le_bytes());
                r[8] = level;
                r[38..40].copy_from_slice(&1u16.to_le_bytes());
                r[40..48].copy_from_slice(&mask.to_le_bytes());
                b.extend(r);
            }
        }
        // a NUMA node record, skipped
        let mut r = vec![0u8; 80];
        r[0..4].copy_from_slice(&1u32.to_le_bytes());
        r[4..8].copy_from_slice(&80u32.to_le_bytes());
        b.extend(r);
        b
    }

    #[test]
    fn windows_cpu_records_pick_one_cache_domain() {
        // two CCDs of 4 cores with SMT (CPUs 0-7 and 8-15), all one class:
        // the first CCD
        let cores: Vec<(u8, u64)> = (0..8).map(|c| (0, 0b11u64 << (2 * c))).collect();
        let (cpus, core) = windows_cpus(&slpi(&cores, &[0x00ff, 0xff00]));
        assert_eq!(cpus.len(), 16);
        assert_eq!(cpus[9].l3, (8..16).collect::<Vec<u32>>());
        assert_eq!(core, None);
        assert_eq!(uniform_cpus(&cpus, core.as_deref()), Some((0..8).collect()));
        // hybrid: 4 performance cores with SMT (0-7), 8 efficiency cores
        // (8-15), one shared L3: the performance cores
        let mut cores: Vec<(u8, u64)> = (0..4).map(|c| (1, 0b11u64 << (2 * c))).collect();
        cores.extend((8..16).map(|c| (0, 1u64 << c)));
        let (cpus, core) = windows_cpus(&slpi(&cores, &[0xffff]));
        assert_eq!(core.as_deref(), Some(&(0..8).collect::<Vec<u32>>()[..]));
        assert_eq!(uniform_cpus(&cpus, core.as_deref()), Some((0..8).collect()));
        // one CCD, all alike: nothing to pin; a short or empty buffer: nothing
        let cores: Vec<(u8, u64)> = (0..4).map(|c| (0, 0b11u64 << (2 * c))).collect();
        let (cpus, core) = windows_cpus(&slpi(&cores, &[0xff]));
        assert_eq!(uniform_cpus(&cpus, core.as_deref()), None);
        assert!(windows_cpus(&[1, 2, 3]).0.is_empty());
        assert!(windows_cpus(&[]).0.is_empty());
    }

    #[test]
    fn on_windows_f355_runs_a_copy_of_flycast_that_logs_to_the_session() {
        let g = game(GameId::F355Challenge2);
        let mut pl = places(g.slug);
        pl.bios = vec!["/bios/dc/f355bios.zip".into()];
        // the session opens the port first
        assert!(
            plan(
                g,
                &pl,
                &comps(),
                &Settings::default(),
                "eth0",
                false,
                Host::Windows
            )
            .is_err()
        );
        pl.log_port = Some(50123);
        let rt = PathBuf::from("/d/runtimes/f355");
        let p = plan(
            g,
            &pl,
            &comps(),
            &Settings::default(),
            "eth0",
            false,
            Host::Windows,
        )
        .unwrap();
        // Flycast keeps emu.cfg and data\ beside its program
        assert_eq!(p.game.program, rt.join("flycast/flycast.exe"));
        assert!(p.links.contains(&(
            rt.join("flycast/flycast.exe"),
            "/c/flycast/flycast.exe".into()
        )));
        assert!(p.links.contains(&(
            rt.join("flycast/data/f355bios.zip"),
            "/bios/dc/f355bios.zip".into()
        )));
        assert_eq!(
            p.game.args,
            vec![rt.join("f355twn2.zip").to_string_lossy().into_owned()]
        );
        assert_eq!(p.game.cwd, rt);
        assert!(
            p.game
                .env
                .contains(&("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS".into(), "1".into()))
        );
        let cfg = file(&p, "flycast/emu.cfg");
        assert!(
            cfg.contains("[log]\nLogToConsole = no\nLogToFile = no\nLogServer = 127.0.0.1:50123\n"),
            "{cfg}"
        );
        // the fullscreen setting (on by default) is Flycast's own here
        assert!(cfg.contains("fullscreen = yes\n"), "{cfg}");
        let windowed = Settings {
            fullscreen: false,
            ..Settings::default()
        };
        let p2 = plan(g, &pl, &comps(), &windowed, "eth0", false, Host::Windows).unwrap();
        assert!(file(&p2, "flycast/emu.cfg").contains("fullscreen = no\n"));
        assert!(p.one_window && p.one_cache);
        assert_eq!(p.boot_hang, Some(FLYCAST_BOOT_HANG));
        assert_eq!(p.stopped, Some(FLYCAST_SAVED));
        assert!(p.gamescope.is_empty());
    }

    #[test]
    fn flycast_slave_boards_get_the_window_and_the_master_runs_off_screen() {
        let master: Vec<String> = vec!["/d/runtimes/f355/f355twn2.zip".into()];
        assert_eq!(flycast_board(&master), Board::Master);
        // what Flycast's master passes when it starts the slave
        let slave: Vec<String> = [
            "-config",
            "naomi:BoardId=1",
            "-config",
            "naomi:SlaveCount=1",
            "f355bios",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(flycast_board(&slave), Board::Slave);
        assert!(board_env(Board::Master).contains(&("SDL_VIDEODRIVER", "offscreen")));
        assert!(board_env(Board::Master).contains(&("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS", "1")));
        assert_eq!(board_env(Board::Slave), &[("SDL_VIDEODRIVER", "x11")]);
    }

    fn cpu(id: u32, rank: u64, l3: &str) -> Cpu {
        Cpu {
            id,
            rank,
            l3: cpu_list(l3),
        }
    }

    #[test]
    fn cpu_lists_parse_like_linux_writes_them() {
        assert_eq!(cpu_list("0-3,8,10-11\n"), vec![0, 1, 2, 3, 8, 10, 11]);
        assert_eq!(cpu_list(""), Vec::<u32>::new());
        assert_eq!(cpu_list("x,2"), vec![2]);
    }

    #[test]
    fn the_boards_share_the_fastest_cache_domain() {
        // a Ryzen 9 7950X3D: two CCDs, the faster one ranked higher
        let mut cpus = Vec::new();
        for id in 0..32u32 {
            let ccd1 = (8..16).contains(&(id % 16));
            let rank = if ccd1 {
                206 + (id % 8) as u64 * 4
            } else {
                166 + (id % 8) as u64 * 5
            };
            cpus.push(cpu(id, rank, if ccd1 { "8-15,24-31" } else { "0-7,16-23" }));
        }
        assert_eq!(uniform_cpus(&cpus, None), Some(cpu_list("8-15,24-31")));
        // a hybrid Intel CPU: one shared cache, only the performance cores
        let hybrid: Vec<Cpu> = (0..12)
            .map(|id| cpu(id, if id < 8 { 70 } else { 40 }, "0-11"))
            .collect();
        assert_eq!(
            uniform_cpus(&hybrid, Some(&cpu_list("0-7"))),
            Some(cpu_list("0-7"))
        );
        // alike cores: nothing to pin
        let plain: Vec<Cpu> = (0..8).map(|id| cpu(id, 100, "0-7")).collect();
        assert_eq!(uniform_cpus(&plain, None), None);
        // too few left
        let small: Vec<Cpu> = (0..4)
            .map(|id| cpu(id, 100, if id < 2 { "0-1" } else { "2-3" }))
            .collect();
        assert_eq!(uniform_cpus(&small, None), None);
        assert_eq!(uniform_cpus(&[], None), None);
    }

    #[test]
    fn the_naomi_bios_is_looked_for_where_emudeck_and_mame_keep_it() {
        let g = game(GameId::F355Challenge2);
        assert_eq!(bios_names(g), ["f355bios.zip"]);
        assert!(bios_names(game(GameId::SegaRally3)).is_empty());
        let at = bios_places(Some(Path::new("/emu/bios")), Path::new("/roms/tp/f355twn2"));
        let want: Vec<PathBuf> = [
            "/emu/bios",
            "/emu/bios/dc",
            "/emu/bios/naomi",
            "/emu/bios/chihiro",
            "/roms/tp/f355twn2",
            "/roms/tp",
        ]
        .map(PathBuf::from)
        .to_vec();
        assert_eq!(at, want);
        assert_eq!(
            bios_places(None, Path::new("/g")),
            [PathBuf::from("/g"), PathBuf::from("/")]
        );
    }
}
