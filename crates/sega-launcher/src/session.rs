//! One game run: build the runtime, write the loader's files, start the
//! game (on Linux in gamescope with the library read-only), feed its input,
//! wait, and leave nothing behind.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use sega_core::catalog::{Game, Host, Route, Support};
use sega_core::input::Cabinet;
use sega_core::launch::{self, Cpu, DIRECTX_REDIST, FLYCAST_PROGRAM, Places, Plan};
use sega_core::library::Entry;
use sega_core::runtime;
use sega_core::settings::Settings;

use crate::input::{self, Script};
use crate::paths::Paths;
use crate::proc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    Screen,
    /// In a headless gamescope (tests, screenshots).
    Headless,
}

pub struct Prepared {
    pub plan: Plan,
    pub runtime: PathBuf,
}

/// What drives the game.
#[derive(Clone)]
pub enum Feed {
    /// The input thread (pads and wheels through SDL).
    Devices {
        tx: Sender<input::Command>,
        tuning: input::Tuning,
    },
    /// A timed script (tests).
    Script(Script),
    /// Nothing (the native loaders read SDL themselves).
    None,
}

pub fn check_components(game: &Game, p: &Paths) -> Result<()> {
    let c = p.components();
    let windows = Host::current() == Host::Windows;
    let need: Vec<PathBuf> = match game.route {
        Route::Lindbergh => vec![
            c.lindbergh.join("lindbergh"),
            c.lindbergh.join("lindbergh.so"),
        ],
        Route::Wal { .. } if windows => vec![
            c.wal.join("arcade-launcher.exe"),
            c.wal.join("payloads"),
            c.wal.join("systemprofiles"),
        ],
        Route::Wal { .. } => vec![
            c.wal.join("arcade-launcher"),
            c.wal.join("payloads"),
            c.wal.join("systemprofiles"),
            c.runner.join("bin/wine"),
            c.directx.join(DIRECTX_REDIST),
        ],
        Route::Flycast { .. } => vec![c.flycast.join(FLYCAST_PROGRAM)],
        Route::Cxbx => vec![],
    };
    let missing: Vec<String> = need
        .iter()
        .filter(|f| !f.exists())
        .map(|f| f.display().to_string())
        .collect();
    if !missing.is_empty() {
        bail!(
            "{} needs launcher components that are missing:\n  {}\n(the package is incomplete: unpack it again)",
            game.title,
            missing.join("\n  ")
        );
    }
    Ok(())
}

pub fn which(program: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|d| d.join(program))
            .find(|p| p.is_file())
    })
}

/// Build the runtime folder and write the game's files.
pub fn prepare(
    paths: &Paths,
    game: &Game,
    entry: &Entry,
    settings: &Settings,
    display: Display,
) -> Result<Prepared> {
    if let Support::Blocked(why) = game.support_on(Host::current()) {
        bail!("{} can't start yet: {why}", game.title);
    }
    check_components(game, paths)?;
    let rt = paths.runtime(game.slug);
    if rt.is_dir() {
        for d in runtime::dangling(&rt)? {
            runtime::remove_link(&d)?;
        }
    }
    let ops = runtime::plan(game, &entry.dir, &rt, &runtime::list)
        .with_context(|| format!("reading {}", entry.dir.display()))?;
    runtime::apply(&ops).with_context(|| format!("building {}", rt.display()))?;
    let config = paths.config();
    let mut bios = Vec::new();
    for name in launch::bios_names(game) {
        let found = launch::bios_places(config.bios.as_deref(), &entry.dir)
            .into_iter()
            .map(|d| d.join(name))
            .find(|f| f.is_file());
        match found {
            Some(f) => bios.push(f),
            None => bail!(
                "{} needs {name}, its board's BIOS file. Add the folder that holds it under \
                 Settings, Folders and BIOS (EmuDeck's bios folder works), or start the \
                 launcher once with --bios <folder>.",
                game.title
            ),
        }
    }
    let places = Places {
        bios,
        runtime: rt.clone(),
        wal_root: paths.wal_root(),
        // lindbergh-loader reads the wheel itself: tell it which one and how
        wheel: (game.route == Route::Lindbergh && display == Display::Screen)
            .then(input::find_wheel)
            .flatten(),
    };
    let nic = launch::default_interface(&fs::read_to_string("/proc/net/route").unwrap_or_default());
    let plan = launch::plan(
        game,
        &places,
        &paths.components(),
        settings,
        &nic,
        display == Display::Headless,
        Host::current(),
    )
    .map_err(anyhow::Error::msg)?;
    for (file, text) in &plan.files {
        if let Some(d) = file.parent() {
            fs::create_dir_all(d)?;
        }
        fs::write(file, text).with_context(|| format!("writing {}", file.display()))?;
    }
    let mut links = Vec::new();
    for (at, target) in &plan.links {
        if let Some(d) = at.parent() {
            links.push(runtime::Op::Dir(d.to_path_buf()));
        }
        links.push(runtime::Op::Link {
            target: target.clone(),
            at: at.clone(),
        });
    }
    runtime::apply(&links)?;
    Ok(Prepared { plan, runtime: rt })
}

/// The command line: gamescope, then bubblewrap with every library folder
/// read-only (no loader can write to the player's dumps), then the game
/// (on `cpus` only, when given: [`launch::uniform_cpus`]).
pub fn command_line(
    plan: &Plan,
    libraries: &[PathBuf],
    bwrap: bool,
    cpus: Option<&str>,
) -> Vec<String> {
    let mut v = vec!["gamescope".to_string()];
    v.extend(plan.gamescope.iter().cloned());
    v.push("--".into());
    if bwrap {
        v.extend(["bwrap", "--dev-bind", "/", "/"].map(String::from));
        for l in libraries {
            let l = l.to_string_lossy().into_owned();
            v.extend(["--ro-bind".into(), l.clone(), l]);
        }
    }
    if let Some(cpus) = cpus {
        v.extend(["taskset".into(), "-c".into(), cpus.to_string()]);
    }
    v.push(plan.game.program.to_string_lossy().into_owned());
    v.extend(plan.game.args.iter().cloned());
    v
}

/// Run the game until it exits or `stop` is set; then stop everything the
/// session started. Returns the game's exit status.
pub fn run(paths: &Paths, p: &Prepared, feed: Feed, stop: Arc<AtomicBool>) -> Result<ExitStatus> {
    let log_path = p.runtime.join("session.log");
    let log = Arc::new(Mutex::new(fs::File::create(&log_path)?));
    let tries = if p.plan.boot_hang.is_some() {
        BOOT_TRIES
    } else {
        1
    };
    let mut attempt = 1;
    loop {
        let (status, hung) = run_once(paths, p, feed.clone(), &stop, &log)?;
        if !hung || stop.load(Ordering::SeqCst) || attempt == tries {
            return Ok(status);
        }
        attempt += 1;
        writeln!(
            log.lock().unwrap(),
            "# the game's boards didn't link up at boot (a known Flycast hang): \
             starting again, try {attempt} of {tries}"
        )?;
    }
}

/// How many times a game whose boot can hang ([`Plan::boot_hang`]) is
/// started before the session gives up.
const BOOT_TRIES: u32 = 6;

/// How long after the start a boot hang is looked for: F355 links its
/// boards within about 50 s.
const BOOT_WINDOW: Duration = Duration::from_secs(90);

/// The CPUs a game that wants one cache domain runs on, from what Linux
/// says about this machine's CPUs (`launch::uniform_cpus`).
fn one_cache_cpus() -> Option<String> {
    let base = Path::new("/sys/devices/system/cpu");
    let read = |p: PathBuf| fs::read_to_string(p).ok();
    let cpus: Vec<Cpu> = launch::cpu_list(&read(base.join("online"))?)
        .into_iter()
        .map(|id| {
            let dir = base.join(format!("cpu{id}"));
            let rank = read(dir.join("acpi_cppc/highest_perf"))
                .or_else(|| read(dir.join("cpufreq/cpuinfo_max_freq")))
                .and_then(|t| t.trim().parse().ok())
                .unwrap_or(0);
            let l3 = read(dir.join("cache/index3/shared_cpu_list"))
                .map(|t| launch::cpu_list(&t))
                .unwrap_or_default();
            Cpu { id, rank, l3 }
        })
        .collect();
    let core = read(PathBuf::from("/sys/devices/cpu_core/cpus")).map(|t| launch::cpu_list(&t));
    let set = launch::uniform_cpus(&cpus, core.as_deref())?;
    Some(set.iter().map(u32::to_string).collect::<Vec<_>>().join(","))
}

/// The command the session starts: on Linux [`command_line`], on Windows
/// the game's own (no gamescope, no bubblewrap).
fn session_argv(plan: &Plan, libraries: &[PathBuf]) -> Vec<String> {
    if Host::current() == Host::Windows {
        return std::iter::once(plan.game.program.to_string_lossy().into_owned())
            .chain(plan.game.args.iter().cloned())
            .collect();
    }
    let cpus = (plan.one_cache && which("taskset").is_some())
        .then(one_cache_cpus)
        .flatten();
    command_line(plan, libraries, which("bwrap").is_some(), cpus.as_deref())
}

fn run_once(
    paths: &Paths,
    p: &Prepared,
    feed: Feed,
    stop: &AtomicBool,
    log: &Arc<Mutex<fs::File>>,
) -> Result<(ExitStatus, bool)> {
    let argv = session_argv(&p.plan, &paths.config().libraries);
    let tag = p.runtime.to_string_lossy().into_owned();
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .current_dir(&p.plan.game.cwd)
        .envs(p.plan.game.env.iter().map(|(k, v)| (k, v)))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.stdin(if p.plan.input_pipe.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    proc::configure(&mut cmd, &tag);
    writeln!(log.lock().unwrap(), "# {}", argv.join(" "))?;
    let mut child = cmd
        .spawn()
        .with_context(|| format!("starting {}", argv[0]))?;
    let group = proc::Group::of(&child, &tag).context("grouping the game's processes")?;

    // stdout: force feedback lines to the input thread, the rest to the log
    let out_tx = match &feed {
        Feed::Devices { tx, .. } => Some(tx.clone()),
        _ => None,
    };
    if let Some(stdout) = child.stdout.take() {
        let log = Arc::clone(log);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if line.starts_with("out ") {
                    if let Some(tx) = &out_tx {
                        let _ = tx.send(input::Command::Output(line));
                    }
                } else {
                    let _ = writeln!(log.lock().unwrap(), "{line}");
                }
            }
        });
    }
    // stderr to the log, watched for the line that means the boot hung
    // (and for the line that says a stopping game has saved)
    let hung = Arc::new(AtomicBool::new(false));
    let saved = Arc::new(AtomicBool::new(false));
    if let Some(stderr) = child.stderr.take() {
        let log = Arc::clone(log);
        let (hung, saved) = (Arc::clone(&hung), Arc::clone(&saved));
        let (marker, stopped) = (p.plan.boot_hang, p.plan.stopped);
        let started = Instant::now();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if marker.is_some_and(|m| line.contains(m)) && started.elapsed() < BOOT_WINDOW {
                    hung.store(true, Ordering::SeqCst);
                }
                if stopped.is_some_and(|m| line.contains(m)) {
                    saved.store(true, Ordering::SeqCst);
                }
                let _ = writeln!(log.lock().unwrap(), "{line}");
            }
        });
    }
    // the loader's input is closed to ask it to stop (it ends the game and
    // gives back what it set up)
    let detach = match &feed {
        Feed::Devices { tx, .. } => Some(tx.clone()),
        _ => None,
    };
    let mut script_sink = None;
    if let (Some(layout), Some(stdin)) = (p.plan.input_pipe, child.stdin.take()) {
        match feed {
            Feed::Devices { tx, tuning } => {
                let _ = tx.send(input::Command::Attach {
                    sink: Box::new(stdin),
                    layout,
                    tuning,
                });
            }
            Feed::Script(script) => script_sink = Some((stdin, Cabinet::new(layout), script)),
            Feed::None => drop(stdin),
        }
    }
    let start = Instant::now();
    let mut last_line = String::new();
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break st;
        }
        if stop.load(Ordering::SeqCst) || hung.load(Ordering::SeqCst) {
            if let Some((mut sink, _, _)) = script_sink.take() {
                let _ = writeln!(sink, "quit");
            }
            if let Some(tx) = &detach {
                let _ = tx.send(input::Command::Detach);
            }
            group.terminate();
            let deadline = Instant::now() + Duration::from_secs(8);
            while Instant::now() < deadline
                && !saved.load(Ordering::SeqCst)
                && child.try_wait()?.is_none()
            {
                std::thread::sleep(Duration::from_millis(100));
            }
            group.kill();
            break child.wait()?;
        }
        if let Some((sink, cabinet, script)) = script_sink.as_mut() {
            let line = cabinet.pipe_line(&script.at(start.elapsed().as_secs_f64()));
            if line != last_line {
                let _ = writeln!(sink, "{line}").and_then(|_| sink.flush());
                last_line = line;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    group.terminate();
    std::thread::sleep(Duration::from_millis(500));
    group.kill();
    let left = group.leftovers();
    writeln!(
        log.lock().unwrap(),
        "# exit: {status}; stopped {left} leftover process(es)"
    )?;
    Ok((status, hung.load(Ordering::SeqCst)))
}

// the Linux command line (gamescope, bubblewrap)
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use sega_core::catalog::{GameId, game};
    use sega_core::launch::Components;

    #[test]
    fn the_library_is_bound_read_only_around_the_game() {
        let places = Places {
            runtime: "/d/rt/src".into(),
            wal_root: "/d/wal".into(),
            wheel: None,
            bios: vec![],
        };
        let c = Components {
            lindbergh: "/c/l".into(),
            wal: "/c/wal".into(),
            runner: "/c/r".into(),
            winetricks: "/c/w".into(),
            directx: "/c/dx".into(),
            flycast: "/c/fc".into(),
            launcher: "/pkg/sega-launcher".into(),
        };
        let g = game(GameId::RacingClassic);
        let plan_with =
            |s: &Settings| launch::plan(g, &places, &c, s, "eth0", true, Host::Linux).unwrap();
        let plan = plan_with(&Settings::default());
        let v = command_line(&plan, &["/lib".into(), "/more".into()], true, None);
        let s = v.join(" ");
        assert!(s.starts_with("gamescope -w 1280 -h 720 "), "{s}");
        assert!(s.contains(" -- bwrap --dev-bind / / --ro-bind /lib /lib --ro-bind /more /more /c/wal/arcade-launcher run "), "{s}");
        let plain = command_line(&plan, &["/lib".into()], false, None);
        assert!(!plain.join(" ").contains("bwrap"));
        // F355's boards on one cache domain
        let pinned = command_line(&plan, &[], false, Some("8,9,10,11")).join(" ");
        assert!(
            pinned.contains(" -- taskset -c 8,9,10,11 /c/wal/arcade-launcher run "),
            "{pinned}"
        );
    }
}
