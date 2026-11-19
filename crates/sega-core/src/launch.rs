//! What to write and what to run for a game, as plain data.
//!
//! [`plan`] turns a game, its folders and the settings into the files the
//! launcher writes and the process it starts, so tests can check the whole
//! wiring (setting → file / argument / environment) without starting
//! anything. Every value here was seen to work in docs/RUNTIME_NOTES.md.

use std::path::{Path, PathBuf};

use crate::catalog::{Game, GameId, Host, Route};
use crate::input::StickLayout;
use rea_launcher::gamescope::{Crt, Picture, Size, fit};
use rea_launcher::wheel::SteeringLock;

use crate::settings::Settings;

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
    /// Run the game on one cache domain of alike cores
    /// (`rea_launcher::cpus::uniform_cpus`).
    pub one_cache: bool,
    /// A line in the game's output that means, once a stop was asked for,
    /// that the game has saved and what's left can be ended.
    pub stopped: Option<&'static str>,
    /// The game's picture comes from a second process the started one
    /// launches (F355's slave board), while the started one (the master
    /// board) runs without a screen of its own and would wait for the other
    /// forever. The session ends the game, politely, when the second one
    /// goes (its window closed, or gamescope with it). On Windows it also
    /// hides the master's window and gives the slave's the focus.
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
    /// xemu's Chihiro fork (`Tovarichtch/xemu`): on Linux its AppImage unpacked
    /// ([`XEMU_PROGRAM`]), on Windows its program ([`XEMU_EXE`]).
    pub xemu: PathBuf,
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

/// xemu's program inside [`Components::xemu`] on Linux.
pub const XEMU_PROGRAM: &str = "usr/bin/xemu";

/// xemu's program inside [`Components::xemu`] on Windows.
pub const XEMU_EXE: &str = "xemu.exe";

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
    /// windowsarcadeloader's first port this start
    /// (`rea_launcher::wal::free_port_block(WAL_PORT)`).
    pub wal_port: u16,
}

/// Where the free block of windowsarcadeloader's ports is looked for
/// from: above the blocks the Initial D and Nintendo launchers try (from
/// 33700 and 33790), so two hubs' games never start on the same ports.
pub const WAL_PORT: u16 = 38000;

/// The size the game draws at: the setting, within what the game can do.
/// SEGA Rally 3 draws at 1280x720 (the cabinet's) or 1920x1080
/// (windowsarcadeloader's `WAL_EUROPAR_RESOLUTION`), nothing else;
/// Daytona Championship USA always at its own 1920x1080; the 4:3 NAOMI and
/// Chihiro cabinets scale cleanly inside the requested size in 4:3.
pub fn game_size(game: &Game, s: &Settings) -> Size {
    let want = s.render.unwrap_or(game.native);
    match game.id {
        GameId::SegaRally3 if want.h >= 1080 => Size::new(1920, 1080),
        GameId::SegaRally3 => Size::new(1280, 720),
        // its profile has no resolution setting: the game's own 1920x1080
        GameId::Daytona => game.native,
        GameId::F355Challenge2 | GameId::OutRun2Sp | GameId::CrazyTaxi => fit(game.native, want),
        _ => want,
    }
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
pub fn wal_layer(game: &Game, size: Size, ffb: u8, port: u16) -> String {
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
    out.push_str(&format!("port: {port}\n"));
    if !env.is_empty() {
        out.push_str("env:\n");
        for (k, v) in env {
            out.push_str(&format!("  {k}: \"{v}\"\n"));
        }
    }
    out
}

/// One BIOS file a game needs from the player's BIOS folder, with accepted
/// filenames (MAME and EmuDeck spellings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BiosSpec {
    pub key: &'static str,
    pub names: &'static [&'static str],
    pub optional: bool,
}

const F355_BIOS: &[BiosSpec] = &[BiosSpec {
    key: "f355bios",
    names: &["f355bios.zip"],
    optional: false,
}];

const CHIHIRO_BIOS_NET: &[BiosSpec] = &[
    BiosSpec {
        key: "bios",
        names: &["chihiro_xbox_bios.bin"],
        optional: false,
    },
    BiosSpec {
        key: "mediaboard",
        names: &["fpr21042_m29w160et.bin", "fpr-23887_29lv160te.ic4"],
        optional: false,
    },
    BiosSpec {
        key: "ic10",
        names: &["ic10_g24lc64", "ic10_g24lc64.bin"],
        optional: false,
    },
    BiosSpec {
        key: "ic11",
        names: &["ic11_24lc024.bin"],
        optional: false,
    },
    BiosSpec {
        key: "pc20",
        names: &["pc20_g24lc64.bin"],
        optional: false,
    },
    BiosSpec {
        key: "net_firmware",
        names: &["ver1305.bin"],
        optional: false,
    },
];

const CHIHIRO_BIOS_OPT_NET: &[BiosSpec] = &[
    CHIHIRO_BIOS_NET[0],
    CHIHIRO_BIOS_NET[1],
    CHIHIRO_BIOS_NET[2],
    CHIHIRO_BIOS_NET[3],
    CHIHIRO_BIOS_NET[4],
    BiosSpec {
        key: "net_firmware",
        names: &["ver1305.bin"],
        optional: true,
    },
];

/// The BIOS files a game needs from the player's BIOS folder.
pub fn bios_specs(game: &Game) -> &'static [BiosSpec] {
    match game.route {
        Route::Flycast { .. } => F355_BIOS,
        Route::Xemu { net_firmware: true } => CHIHIRO_BIOS_NET,
        Route::Xemu {
            net_firmware: false,
        } => CHIHIRO_BIOS_OPT_NET,
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
    let af = if size.h > 480 {
        "rend.AnisotropicFiltering = 16\n"
    } else {
        ""
    };
    let mut s = format!(
        "# Written by the launcher before every start.\n\
         [config]\nrend.Resolution = {h}\n{af}\
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

/// Formats `path` as a TOML basic string, safe for spaces, apostrophes and
/// Windows backslashes.
fn toml_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let mut out = String::with_capacity(raw.len() + 2);
    out.push('"');
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// xemu's `xemu.toml` for a Sega Chihiro game (`Tovarichtch/xemu`).
pub fn xemu_toml(
    size: Size,
    fullscreen: bool,
    image: &Path,
    bios: &[PathBuf],
    s: &Settings,
) -> String {
    let scale = (size.h / 480).clamp(1, 10);
    let wheel_rot = match s.steering_lock {
        SteeringLock::Full => 0,
        SteeringLock::Degrees(d) => u32::from(d),
    };
    let ffb_on = s.force_feedback > 0;
    let ffb_strength = s.force_feedback;
    let rom = |names: &[&str]| -> Option<&Path> {
        bios.iter()
            .find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| names.iter().any(|want| n.eq_ignore_ascii_case(want)))
            })
            .map(PathBuf::as_path)
    };
    let mut out = format!(
        "# Written by the launcher before every start.\n\
         [general]\nshow_welcome = false\n\
         [general.updates]\ncheck = false\n\
         [display]\nrenderer = 'OPENGL'\n\
         [display.quality]\nsurface_scale = {scale}\n\
         [display.window]\nstartup_size = '{w}x{h}'\nfullscreen_on_startup = {fullscreen}\n\
         [sys]\ndefault_machine = 'chihiro'\nlast_machine = 'chihiro'\nmem_limit = '128'\n\
         [sys.files]\ndvd_path = {img}\n\
         [chihiro.settings]\nfreeplay = true\nregion = 'ex'\ndimm_size = 'auto'\n\
         board_type = 'auto'\nforce_feedback = {ffb_on}\nffb_strength = {ffb_strength}\n\
         wheel_rotation = {wheel_rot}\nwheel_autocenter = true\n\
         [chihiro.roms]\n",
        w = size.w,
        h = size.h,
        img = toml_path(image),
    );
    for (key, names) in [
        ("bios_path", CHIHIRO_BIOS_NET[0].names),
        ("mediaboard_path", CHIHIRO_BIOS_NET[1].names),
        ("ic10_path", CHIHIRO_BIOS_NET[2].names),
        ("ic11_path", CHIHIRO_BIOS_NET[3].names),
        ("pc20_path", CHIHIRO_BIOS_NET[4].names),
        ("net_firmware_path", CHIHIRO_BIOS_NET[5].names),
    ] {
        if let Some(p) = rom(names) {
            out.push_str(&format!("{key} = {}\n", toml_path(p)));
        }
    }
    out
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
            files.push((
                layer.clone(),
                wal_layer(game, size, s.force_feedback, places.wal_port),
            ));
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
                // redistributable for `xact` from the package, not the network;
                // DISABLE_GAMESCOPE_WSI=1 avoids a gamescope 3.17 SIGSEGV on
                // NVIDIA + Wayland when 32-bit DXVK creates a swapchain
                // (SyncobjFdToHandle EINVAL / vkImportSemaphoreFdKHR -3).
                env: match host {
                    Host::Linux => vec![
                        ("W_CACHE".into(), c.directx.to_string_lossy().into_owned()),
                        ("DISABLE_GAMESCOPE_WSI".into(), "1".into()),
                    ],
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
        Route::Xemu { .. } => {
            let xemu_dir = rt.join("xemu");
            let cfg_path = xemu_dir.join("xemu.toml");
            let image = rt.join(format!("{}.bin", game.slug));
            let fullscreen = host == Host::Windows && s.fullscreen;
            files.push((
                cfg_path.clone(),
                xemu_toml(size, fullscreen, &image, &places.bios, s),
            ));
            let program = match host {
                Host::Linux => c.xemu.join(XEMU_PROGRAM),
                Host::Windows => {
                    // xemu enters portable mode when xemu.toml sits beside xemu.exe,
                    // keeping saves, EEPROM and shaders inside this game's runtime.
                    let local = xemu_dir.join(XEMU_EXE);
                    links.push((local.clone(), c.xemu.join(XEMU_EXE)));
                    local
                }
            };
            let env = match host {
                Host::Linux => vec![
                    (
                        "XDG_DATA_HOME".into(),
                        xemu_dir.join("data").to_string_lossy().into_owned(),
                    ),
                    (
                        "XDG_CONFIG_HOME".into(),
                        xemu_dir.join("config").to_string_lossy().into_owned(),
                    ),
                    (
                        "XDG_CACHE_HOME".into(),
                        xemu_dir.join("cache").to_string_lossy().into_owned(),
                    ),
                ],
                Host::Windows => vec![],
            };
            Command {
                program,
                args: vec![
                    "-config_path".into(),
                    cfg_path.to_string_lossy().into_owned(),
                ],
                env,
                cwd: rt.clone(),
            }
        }
    };
    // F355's linked boards: restart a hung boot, alike cores, stop once saved
    let flycast = matches!(game.route, Route::Flycast { .. });
    Ok(Plan {
        files,
        links,
        game: game_cmd,
        gamescope: match host {
            Host::Linux => Picture {
                game: size,
                output: s.output,
                upscaler: s.upscaler,
                sharpness: s.sharpness,
                fullscreen: s.fullscreen,
                crt: Crt::Off,
            }
            .args(headless),
            Host::Windows => Vec::new(),
        },
        input_pipe,
        game_size: size,
        boot_hang: flycast.then_some(FLYCAST_BOOT_HANG),
        one_cache: flycast,
        stopped: flycast.then_some(FLYCAST_SAVED),
        one_window: flycast,
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
            xemu: "/c/xemu".into(),
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
            wal_port: WAL_PORT,
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
        // the session's free block of ports, never the loader's default 33700
        assert!(layer.contains("\nport: 38000\n"), "{layer}");
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
        // winetricks finds the bundled DirectX redistributable, offline, and
        // gamescope's WSI layer is disabled so 32-bit DXVK doesn't crash gamescope on NVIDIA
        assert!(
            p.game
                .env
                .contains(&("W_CACHE".into(), "/c/directx".into()))
        );
        assert!(
            p.game
                .env
                .contains(&("DISABLE_GAMESCOPE_WSI".into(), "1".into()))
        );
        let w = plan_for(
            GameId::SegaRally3,
            &Settings::default(),
            false,
            Host::Windows,
        );
        assert!(
            w.game
                .env
                .iter()
                .all(|(k, _)| k != "W_CACHE" && k != "DISABLE_GAMESCOPE_WSI")
        );
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
        // the runtime as the places give it (Windows joins with `\\`)
        let rt = pl.runtime.clone();
        assert_eq!(p.game.program, PathBuf::from("/pkg/sega-launcher"));
        // the paths in the command line, with `/` whatever the host joins with
        let slash = |s: &str| s.replace('\\', "/");
        assert_eq!(
            p.game.args.iter().map(|a| slash(a)).collect::<Vec<_>>(),
            vec!["/d/runtimes/f355/f355twn2.zip".to_string()]
        );
        let env = |k: &str| {
            p.game
                .env
                .iter()
                .find(|(n, _)| n == k)
                .map(|(_, v)| slash(v))
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
        // 4:3 scaled inside the requested 4K render size
        assert_eq!(p.game_size, Size::new(2880, 2160));
        assert_eq!(arg_after(&p.gamescope, "-w").as_deref(), Some("2880"));
        let cfg = file(&p, "flycast/config/flycast/emu.cfg");
        assert!(cfg.contains("rend.Resolution = 2160\n"), "{cfg}");
        assert!(cfg.contains("rend.AnisotropicFiltering = 16\n"), "{cfg}");
        assert!(cfg.contains("width = 2880\nheight = 2160\n"), "{cfg}");
        assert_eq!(p.boot_hang, Some(FLYCAST_BOOT_HANG));
        assert!(p.one_cache);
        assert_eq!(p.stopped, Some(FLYCAST_SAVED));
        assert!(p.input_pipe.is_none());
        assert!(p.one_window, "the session watches the slave board");
        assert!(!cfg.contains("LogServer"), "{cfg}");
        let other = plan_for(GameId::SegaRally3, &Settings::default(), false, Host::Linux);
        assert!(other.boot_hang.is_none() && !other.one_cache && other.stopped.is_none());
        assert!(!other.one_window);
    }

    /// A `GetLogicalProcessorInformationEx` buffer: cores as (efficiency
    /// class, CPU mask), level-3 caches as CPU masks.
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
        // the runtime as the places give it (Windows joins with `\\`)
        let rt = pl.runtime.clone();
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

    #[test]
    fn the_naomi_and_chihiro_bios_are_looked_for_where_emudeck_and_mame_keep_it() {
        let g = game(GameId::F355Challenge2);
        assert_eq!(bios_specs(g).len(), 1);
        assert_eq!(bios_specs(g)[0].names, ["f355bios.zip"]);
        assert!(bios_specs(game(GameId::SegaRally3)).is_empty());
        assert_eq!(bios_specs(game(GameId::OutRun2Sp)).len(), 6);
        assert!(!bios_specs(game(GameId::OutRun2Sp))[5].optional);
        assert!(bios_specs(game(GameId::CrazyTaxi))[5].optional);
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

    #[test]
    fn chihiro_uses_xemu_with_fatx_image_and_bios_roms() {
        let g = game(GameId::OutRun2Sp);
        let mut pl = places(g.slug);
        pl.bios = vec![
            "/bios/chihiro/chihiro_xbox_bios.bin".into(),
            "/bios/chihiro/fpr21042_m29w160et.bin".into(),
            "/bios/chihiro/ic10_g24lc64".into(),
            "/bios/chihiro/ic11_24lc024.bin".into(),
            "/bios/chihiro/pc20_g24lc64.bin".into(),
            "/bios/chihiro/ver1305.bin".into(),
        ];
        let s = Settings {
            render: Some(Size::new(3840, 2160)),
            force_feedback: 120,
            ..Settings::default()
        };
        let p = plan(g, &pl, &comps(), &s, "eth0", false, Host::Linux).unwrap();
        assert_eq!(p.game.program, PathBuf::from("/c/xemu/usr/bin/xemu"));
        assert_eq!(p.game_size, Size::new(2880, 2160));
        let toml = file(&p, "xemu/xemu.toml");
        assert!(toml.contains("default_machine = 'chihiro'"), "{toml}");
        assert!(toml.contains("surface_scale = 4\n"), "{toml}");
        assert!(toml.contains("startup_size = '2880x2160'"), "{toml}");
        assert!(toml.contains("fullscreen_on_startup = false"), "{toml}");
        assert!(toml.contains("ffb_strength = 120"), "{toml}");
        assert!(toml.contains("wheel_rotation = 270"), "{toml}");
        assert!(
            toml.contains("net_firmware_path = \"/bios/chihiro/ver1305.bin\""),
            "{toml}"
        );
        let w = plan(g, &pl, &comps(), &s, "eth0", false, Host::Windows).unwrap();
        assert_eq!(w.game.program, pl.runtime.join("xemu/xemu.exe"));
        assert!(
            w.links
                .contains(&(pl.runtime.join("xemu/xemu.exe"), "/c/xemu/xemu.exe".into()))
        );
        assert!(file(&w, "xemu/xemu.toml").contains("fullscreen_on_startup = true"));
        assert!(w.gamescope.is_empty());
    }
}
