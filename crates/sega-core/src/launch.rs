//! What to write and what to run for a game, as plain data.
//!
//! [`plan`] turns a game, its folders and the settings into the files the
//! launcher writes and the process it starts, so tests can check the whole
//! wiring (setting → file / argument / environment) without starting
//! anything. Every value here was seen to work in docs/RUNTIME_NOTES.md.

use std::path::PathBuf;

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
}

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
        Route::Cxbx | Route::Flycast { .. } => {
            return Err(format!("{} can't be started yet.", game.title));
        }
    };
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
        }
    }

    fn places(slug: &str) -> Places {
        Places {
            runtime: PathBuf::from("/d/runtimes").join(slug),
            wal_root: "/d/wal".into(),
            wheel: None,
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
}
