//! Golden snapshots (docs/STANDARDS.md, "Golden snapshots"): everything the
//! launcher produces, compared with the files in `tests/snapshots/`, so an
//! SDK bump or a refactor can't change it silently. A diff is reviewed like
//! code: it must match an intended change or an SDK Migration item.
//!
//! `REA_UPDATE_SNAPSHOTS=1 cargo test --test snapshots` rewrites them.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use rea_launcher::linkfarm;
use sega_core::catalog::{GAMES, Game, Host};
use sega_core::launch::{self, Components, Places};
use sega_core::runtime;
use sega_core::settings::{self, Look, Settings};

/// A path as the snapshot spells it on every system (Windows joins with `\`).
fn p(path: &std::path::Path) -> String {
    path.display().to_string().replace('\\', "/")
}

#[test]
fn help() {
    let out = Command::new(env!("CARGO_BIN_EXE_sega-launcher"))
        .arg("--help")
        .env("SEGA_DATA", env!("CARGO_TARGET_TMPDIR"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .expect("run the launcher");
    assert!(out.status.success());
    rea_launcher::snapshot!("help.txt", String::from_utf8_lossy(&out.stdout));
}

#[test]
fn the_settings_template() {
    rea_launcher::snapshot!("settings.cfg.txt", settings::template());
}

/// What each look plans for each game (files, command, environment,
/// gamescope), on a 2560x1440 screen, on Linux and Windows.
#[test]
fn every_look_of_every_game() {
    let c = components();
    let mut out = String::new();
    for host in [Host::Linux, Host::Windows] {
        for g in GAMES {
            let places = places(g);
            for look in Look::ALL {
                let mut s = Settings::default();
                look.apply(&mut s, g.native, Some((2560, 1440)));
                let _ = writeln!(out, "== {host:?} {} {}", g.slug, look.key());
                match launch::plan(g, &places, &c, &s, "eth0", false, host) {
                    Ok(plan) => {
                        let _ = writeln!(out, "size {}", plan.game_size);
                        let _ = writeln!(
                            out,
                            "run {} {}",
                            p(&plan.game.program),
                            plan.game.args.join(" ").replace('\\', "/")
                        );
                        let _ = writeln!(out, "cwd {}", p(&plan.game.cwd));
                        for (k, v) in &plan.game.env {
                            let _ = writeln!(out, "env {k}={}", v.replace('\\', "/"));
                        }
                        let _ = writeln!(out, "gamescope {}", plan.gamescope.join(" "));
                        let _ = writeln!(out, "input {:?}", plan.input_pipe);
                        if let Some(m) = plan.boot_hang {
                            let _ = writeln!(out, "restart on {m:?}");
                        }
                        if plan.one_cache {
                            let _ = writeln!(out, "one cache domain");
                        }
                        if let Some(m) = plan.stopped {
                            let _ = writeln!(out, "saved on {m:?}");
                        }
                        if plan.one_window {
                            let _ = writeln!(out, "one window (the slave board's)");
                        }
                        for (at, target) in &plan.links {
                            let _ = writeln!(out, "link {} -> {}", p(at), p(target));
                        }
                        for (f, text) in &plan.files {
                            let _ = writeln!(out, "file {}\n{text}", p(f));
                        }
                    }
                    Err(e) => {
                        let _ = writeln!(out, "not yet: {e}");
                    }
                }
            }
        }
    }
    rea_launcher::snapshot!("plans.txt", out);
}

fn components() -> Components {
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

fn places(g: &Game) -> Places {
    Places {
        runtime: PathBuf::from("/d/runtimes").join(g.slug),
        wal_root: "/d/wal".into(),
        wheel: None,
        bios: launch::bios_names(g)
            .iter()
            .map(|n| PathBuf::from("/bios").join(n))
            .collect(),
        // the port the Windows session opens for Flycast's log
        log_port: Some(50123),
        wal_port: launch::WAL_PORT,
    }
}

/// A plan as the snapshots spell it.
fn write_plan(out: &mut String, plan: Result<launch::Plan, launch::NotYet>) {
    let plan = match plan {
        Ok(plan) => plan,
        Err(e) => {
            let _ = writeln!(out, "not yet: {e}");
            return;
        }
    };
    let _ = writeln!(out, "size {}", plan.game_size);
    let _ = writeln!(
        out,
        "run {} {}",
        p(&plan.game.program),
        plan.game.args.join(" ").replace('\\', "/")
    );
    for (k, v) in &plan.game.env {
        let _ = writeln!(out, "env {k}={}", v.replace('\\', "/"));
    }
    let _ = writeln!(out, "gamescope {}", plan.gamescope.join(" "));
    for (at, target) in &plan.links {
        let _ = writeln!(out, "link {} -> {}", p(at), p(target));
    }
    for (f, text) in &plan.files {
        let _ = writeln!(out, "file {}\n{text}", p(f));
    }
}

/// The cases `every_look_of_every_game` leaves out: a headless run, the
/// default settings, a wheel lindbergh-loader reads, F355 without its BIOS
/// or (Windows) without a log port, and what each Flycast board runs with.
#[test]
fn launch_plan_variants() {
    let c = components();
    let mut out = String::new();
    for host in [Host::Linux, Host::Windows] {
        for g in GAMES {
            let s = Settings::default();
            let _ = writeln!(out, "== {host:?} {} default settings, headless", g.slug);
            write_plan(
                &mut out,
                launch::plan(g, &places(g), &c, &s, "eth0", true, host),
            );
        }
        let or2 = sega_core::catalog::by_slug("or2sdx").unwrap();
        let wheel = Places {
            wheel: Some(launch::LindberghWheel {
                guid: "030000006d04000062c2000011010000".into(),
                steer_axis: 0,
                gas: Some((2, true)),
                brake: Some((3, false)),
                paddles: Some((4, 5)),
            }),
            ..places(or2)
        };
        let _ = writeln!(out, "== {host:?} or2sdx with a wheel");
        write_plan(
            &mut out,
            launch::plan(or2, &wheel, &c, &Settings::default(), "lo", false, host),
        );
        let f355 = sega_core::catalog::by_slug("f355").unwrap();
        let bare = Places {
            bios: vec![],
            log_port: None,
            ..places(f355)
        };
        let _ = writeln!(out, "== {host:?} f355 without its BIOS or a log port");
        write_plan(
            &mut out,
            launch::plan(f355, &bare, &c, &Settings::default(), "eth0", false, host),
        );
    }
    let _ = writeln!(out, "== Flycast boards");
    for args in [
        vec!["/d/runtimes/f355/f355twn2.zip".to_string()],
        vec![
            "-config".into(),
            "naomi:BoardId=1".into(),
            "/d/runtimes/f355/f355twn2.zip".into(),
        ],
    ] {
        let board = launch::flycast_board(&args);
        let _ = writeln!(
            out,
            "{} -> {board:?} {:?}",
            args.join(" "),
            launch::board_env(board)
        );
    }
    let _ = writeln!(out, "== the BIOS is looked for in");
    for d in launch::bios_places(Some(Path::new("/bios")), Path::new("/games/f355")) {
        let _ = writeln!(out, "{}", p(&d));
    }
    rea_launcher::snapshot!("plan-variants.txt", out);
}

/// A game dump as `runtime::plan` lists it: the main file, small and large
/// files, code, the names the routes skip, and each folder the game writes
/// into with a file, a large file and a subfolder.
fn fake_dump(g: &Game) -> Vec<(String, u64)> {
    let mut files: Vec<(String, u64)> = vec![
        (g.runtime_main_file().to_string(), 8 << 20),
        ("small.ini".into(), 100),
        ("big.bin".into(), 64 << 20),
        ("code.dll".into(), 100),
        ("libfoo.so.1".into(), 100),
        ("lindbergh.ini".into(), 100),
        ("AMFS/x".into(), 100),
        ("old.windowsloader".into(), 10),
        ("TeknoParrot.ini".into(), 100),
        ("$RECYCLE.BIN/x".into(), 1),
        ("System Volume Information/x".into(), 1),
        ("data/deep/x.bin".into(), 100),
    ];
    for o in runtime::opened(g) {
        files.push((format!("{o}/config.ini"), 100));
        files.push((format!("{o}/data.bin"), 64 << 20));
        files.push((format!("{o}/sub/deep.txt"), 100));
    }
    files
}

/// Every game's runtime folder planned from [`fake_dump`].
#[test]
fn runtime_plans_of_every_game() {
    let mut out = String::new();
    for g in GAMES {
        let dump = PathBuf::from("/dump").join(g.slug);
        let src = dump.join(g.source_folder());
        let files = fake_dump(g);
        let ls = |dir: &Path| -> std::io::Result<Vec<linkfarm::Item>> {
            let rel = p(dir.strip_prefix(&src).expect("inside the dump"));
            let mut items: Vec<linkfarm::Item> = Vec::new();
            for (f, size) in &files {
                let below = if rel.is_empty() {
                    Some(f.as_str())
                } else {
                    f.strip_prefix(&format!("{rel}/"))
                };
                let Some(below) = below else { continue };
                let (name, dir) = match below.split_once('/') {
                    Some((name, _)) => (name, true),
                    None => (below, false),
                };
                if !items.iter().any(|i| i.name == name) {
                    items.push(linkfarm::Item {
                        name: name.into(),
                        dir,
                        size: if dir { 0 } else { *size },
                    });
                }
            }
            items.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(items)
        };
        let rt = PathBuf::from("/rt").join(g.slug);
        let _ = writeln!(out, "== {}", g.slug);
        for op in runtime::plan(g, &dump, &rt, &ls).expect("a plan") {
            let _ = match op {
                linkfarm::Op::Dir(d) => writeln!(out, "dir {}", p(&d)),
                linkfarm::Op::Link { target, at } => {
                    writeln!(out, "link {} -> {}", p(&at), p(&target))
                }
                linkfarm::Op::CopyOnce { from, to } => {
                    writeln!(out, "copy {} -> {}", p(&from), p(&to))
                }
                linkfarm::Op::Archive { files, to } => {
                    let names: Vec<String> = files.iter().map(|f| p(f)).collect();
                    writeln!(out, "zip {} <- {}", p(&to), names.join(" "))
                }
                other => writeln!(out, "{other:?}"),
            };
        }
    }
    rea_launcher::snapshot!("runtime-plans.txt", out);
}

/// A settings file as the player may have left it: a byte-order mark,
/// CRLF line ends, comments, unknown and repeated keys, bad values.
const MESSY_SETTINGS: &str = "\u{feff}# my notes\r\nupscaler = nis\r\nfuture_key = 7\r\n\
    sharpness = 99\r\nrender = 1920x1080\r\nrender = 640x480\r\n[section]\r\n\
    fullscreen = off # trailing\r\nsteering_lock = 333\r\n";

/// Line ends and the byte-order mark made visible.
fn visible(text: &str) -> String {
    text.replace('\u{feff}', "<BOM>").replace('\r', "<CR>")
}

#[test]
fn settings_files_round_trip() {
    let mut out = String::new();
    for (name, text) in [
        ("empty", ""),
        ("messy", MESSY_SETTINGS),
        ("template", &settings::template()),
    ] {
        let (s, problems) = Settings::parse(text);
        let _ = writeln!(out, "== {name}\n{}", visible(text));
        let _ = writeln!(out, "-- problems");
        for pr in &problems {
            let _ = writeln!(out, "{pr}");
        }
        let _ = writeln!(out, "-- read\n{s:#?}");
        let _ = writeln!(out, "-- saved unchanged\n{}", visible(&s.save(text)));
        let mut changed = s.clone();
        changed.steering_lock = rea_launcher::wheel::SteeringLock::Degrees(900);
        changed.look = Some(Look::SteamDeck);
        changed.render = None;
        let _ = writeln!(
            out,
            "-- saved with steering_lock 900, look steam_deck, render native\n{}",
            visible(&changed.save(text))
        );
    }
    rea_launcher::snapshot!("settings-round-trip.txt", out);
}

/// The command line against a library of look-alike game files (right
/// names, unknown builds), from a fresh data folder: what each command
/// prints and exits with, and what it leaves in the data folder.
#[test]
fn command_line_runs() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    let games = root.join("games");
    for (dir, file) in [("rally", "Rally/Rally.exe"), ("f355", "epr-23399.ic22")] {
        let at = games.join(dir).join(file);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(&at, format!("not {file}")).unwrap();
    }
    std::fs::create_dir_all(games.join("empty")).unwrap();
    std::fs::create_dir_all(root.join("bios")).unwrap();
    std::fs::create_dir_all(root.join("run")).unwrap();
    let data = root.join("data");
    let shown = |text: &str| {
        text.replace(&root.display().to_string(), "<tmp>")
            .replace('\\', "/")
    };
    let mut out = String::new();
    let mut run = |args: &[&str]| {
        let args: Vec<String> = args
            .iter()
            .map(|a| a.replace("<tmp>", &root.display().to_string()))
            .collect();
        let o = Command::new(env!("CARGO_BIN_EXE_sega-launcher"))
            .args(&args)
            .env("SEGA_DATA", &data)
            .env("SEGA_COMPONENTS", root.join("components"))
            .env("XDG_RUNTIME_DIR", root.join("run"))
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .expect("run the launcher");
        let _ = writeln!(
            out,
            "$ sega-launcher {}\nexit {}\n-- stdout\n{}-- stderr\n{}",
            shown(&args.join(" ")),
            o.status.code().unwrap_or(-1),
            shown(&String::from_utf8_lossy(&o.stdout)),
            shown(&String::from_utf8_lossy(&o.stderr)),
        );
    };
    // an error of --list or --check opens a message window on Windows
    if cfg!(unix) {
        run(&["--list"]);
        run(&["--check"]);
    }
    run(&["--library", "<tmp>/nowhere"]);
    run(&["--bios", "<tmp>/nowhere"]);
    run(&["--library", "<tmp>/games", "--bios", "<tmp>/bios"]);
    run(&["--list"]);
    run(&["--check"]);
    run(&["--check", "sr3"]);
    run(&["--check", "id9"]);
    run(&["--check", "--linux"]);
    run(&[
        "--set",
        "sr3",
        "--render",
        "1920x1080",
        "--steering-lock",
        "900",
    ]);
    run(&["--set", "src"]);
    run(&["--play", "sr3", "--headless", "--seconds", "1"]);
    run(&["--frobnicate"]);
    run(&["--headless"]);
    run(&["--render", "1920x1080"]);
    run(&["--list", "--check"]);
    run(&["--steering-lock", "333", "--play", "sr3"]);
    run(&["--seconds", "soon", "--play", "sr3"]);
    run(&["--set"]);
    run(&["--play", "f355", "--input-script", "<tmp>/s.txt"]);
    #[cfg(unix)]
    {
        let lock = root.join("run/rea-launcher-rea-sega-launcher.lock");
        let _held = rea_launcher::instance::single_at(&lock).expect("the lock");
        run(&["--check"]);
        run(&["--list"]);
        run(&["--list", "--library", "<tmp>/games"]);
    }
    for f in ["launcher.cfg", "settings/sr3.cfg", "settings/src.cfg"] {
        let text = std::fs::read_to_string(data.join(f)).unwrap_or_else(|_| "(none)\n".into());
        let _ = writeln!(out, "== {f}\n{}", shown(&text));
    }
    // the Windows run leaves out what opens a window or needs flock
    let host = if cfg!(windows) { "windows" } else { "linux" };
    rea_launcher::snapshot!(&format!("cli-{host}.txt"), out);
}

/// `launcher.cfg` written by an older launcher or by hand: the 0.1.0
/// BIOS key, comments, unknown keys, a byte-order mark and CRLF line ends,
/// and what adding a library through the command line makes of it.
#[test]
fn an_old_launcher_cfg_and_a_new_library() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    let data = root.join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(root.join("games")).unwrap();
    let mut out = String::new();
    for (name, old) in [
        (
            "0.1.0",
            "# Written by the launcher.\nlibrary = /old/games\nchihiro_bios = /old/bios/chihiro\n",
        ),
        (
            "by hand",
            "\u{feff}library = /first\r\n# my notes\r\nfuture_key = 1\r\nlibrary = /second\r\n\
             bios = /bios\r\nchihiro_bios = /legacy\r\nlibrary =\r\n",
        ),
    ] {
        std::fs::write(data.join("launcher.cfg"), old).unwrap();
        let o = Command::new(env!("CARGO_BIN_EXE_sega-launcher"))
            .arg("--library")
            .arg(root.join("games"))
            .env("SEGA_DATA", &data)
            .env("XDG_RUNTIME_DIR", root)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .expect("run the launcher");
        let new = std::fs::read_to_string(data.join("launcher.cfg")).unwrap();
        let _ = writeln!(
            out,
            "== {name}\n{}-- exit {}, then\n{}",
            visible(old),
            o.status.code().unwrap_or(-1),
            visible(
                &new.replace(&root.display().to_string(), "<tmp>")
                    .replace('\\', "/")
            )
        );
    }
    rea_launcher::snapshot!("launcher-cfg.txt", out);
}
