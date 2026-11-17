//! One game run: build the runtime, write the loader's files, start the
//! game (on Linux in gamescope with the library read-only), feed its input,
//! wait, and leave nothing behind.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use rea_launcher::cpus;
use rea_launcher::gamescope::CommandLine;
use rea_launcher::library::Entry;
use rea_launcher::linkfarm::{self, Occupied};
use rea_launcher::session::{WindowWatch, default_interface, strip_ansi};
use sega_core::catalog::{Game, Host, Route, Support};
use sega_core::input::Cabinet;
use sega_core::launch::{self, DIRECTX_REDIST, FLYCAST_EXE, FLYCAST_PROGRAM, Places, Plan};
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
    /// Where Flycast's Windows build sends its log ([`Places::log_port`]).
    pub log: Option<UdpSocket>,
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
        Route::Flycast { .. } if windows => vec![c.flycast.join(FLYCAST_EXE)],
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
    entry: &Entry<Game>,
    settings: &Settings,
    display: Display,
) -> Result<Prepared> {
    if let Support::Blocked(why) = game.support_on(Host::current()) {
        bail!("{} can't start yet: {why}", game.title);
    }
    check_components(game, paths)?;
    let rt = paths.runtime(game.slug);
    if rt.is_dir() {
        for d in linkfarm::dangling(&rt)? {
            linkfarm::remove_link(&d)?;
        }
    }
    let ops = runtime::plan(game, &entry.dir, &rt, &linkfarm::list)
        .with_context(|| format!("reading {}", entry.dir.display()))?;
    linkfarm::apply(&ops, Occupied::Refuse)
        .with_context(|| format!("building {}", rt.display()))?;
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
    // Flycast's Windows build has no output to read: it sends its log here
    let log = match game.route {
        Route::Flycast { .. } if Host::current() == Host::Windows => Some(
            UdpSocket::bind(("127.0.0.1", 0)).context("opening a local port for Flycast's log")?,
        ),
        _ => None,
    };
    let places = Places {
        log_port: log
            .as_ref()
            .map(|s| s.local_addr().map(|a| a.port()))
            .transpose()?,
        bios,
        runtime: rt.clone(),
        wal_root: paths.wal_root(),
        // lindbergh-loader reads the wheel itself: tell it which one and how
        wheel: (game.route == Route::Lindbergh && display == Display::Screen)
            .then(input::find_wheel)
            .flatten(),
    };
    let nic = default_interface(&fs::read_to_string("/proc/net/route").unwrap_or_default());
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
            links.push(linkfarm::Op::Dir(d.to_path_buf()));
        }
        links.push(linkfarm::Op::Link {
            target: target.clone(),
            at: at.clone(),
        });
    }
    linkfarm::apply(&links, Occupied::Refuse)?;
    Ok(Prepared {
        plan,
        runtime: rt,
        log,
    })
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

/// How long a stopping game gets to save and quit before it is ended. F355's
/// master board on Windows, its game window closed, first waits out one
/// 5 s timeout for the slave board (about 6 s in all, docs/RUNTIME_NOTES.md).
const STOP_GRACE: Duration = Duration::from_secs(10);

/// How long after the start a boot hang is looked for: F355 links its
/// boards within about 50 s.
const BOOT_WINDOW: Duration = Duration::from_secs(90);

/// The command the session starts: on Linux gamescope, then bubblewrap
/// with every library folder read-only (no loader can write to the
/// player's dumps), then the game on one cache domain when it wants one
/// (F355's boards); on Windows the game's own (the CPUs are set on the
/// process once started, [`pin_windows`]).
fn session_argv(plan: &Plan, libraries: &[PathBuf]) -> Vec<String> {
    if Host::current() == Host::Windows {
        return std::iter::once(plan.game.program.to_string_lossy().into_owned())
            .chain(plan.game.args.iter().cloned())
            .collect();
    }
    let cpus = (plan.one_cache && which("taskset").is_some())
        .then(|| {
            let (all, core) = cpus::linux_cpus(Path::new("/sys"));
            cpus::uniform_cpus(&all, core.as_deref())
        })
        .flatten();
    command_line(plan, libraries, which("bwrap").is_some(), cpus)
}

/// [`session_argv`] on Linux, with what this machine has decided.
fn command_line(
    plan: &Plan,
    libraries: &[PathBuf],
    bwrap: bool,
    cpus: Option<Vec<u32>>,
) -> Vec<String> {
    CommandLine {
        gamescope: plan.gamescope.clone(),
        homes: None,
        read_only: bwrap.then(|| libraries.to_vec()),
        cpus: cpus.map(|set| cpus::taskset_list(&set)),
    }
    .argv(&plan.game.program, &plan.game.args)
}

/// Windows: keeps the game on one cache domain of alike cores, as taskset
/// does on Linux. Set right after the start, before Flycast's master
/// starts its slave (which then inherits it).
#[cfg(windows)]
fn pin_windows(child: &std::process::Child) -> Option<String> {
    use std::os::windows::io::AsRawHandle;
    let (all, core) = crate::win32::cpus();
    let set = cpus::uniform_cpus(&all, core.as_deref())?;
    crate::win32::pin(child.as_raw_handle(), &set).then(|| cpus::taskset_list(&set))
}

/// What the session reads in the game's log: the line that means the boot
/// hung, and the line that says a stopping game has saved.
#[derive(Clone)]
struct Watch {
    hang: Option<&'static str>,
    stopped: Option<&'static str>,
    started: Instant,
    hung: Arc<AtomicBool>,
    saved: Arc<AtomicBool>,
    log: Arc<Mutex<fs::File>>,
}

impl Watch {
    fn line(&self, line: &str) {
        if self.hang.is_some_and(|m| line.contains(m)) && self.started.elapsed() < BOOT_WINDOW {
            self.hung.store(true, Ordering::SeqCst);
        }
        if self.stopped.is_some_and(|m| line.contains(m)) {
            self.saved.store(true, Ordering::SeqCst);
        }
        let _ = writeln!(self.log.lock().unwrap(), "{line}");
    }
}

/// Reads the lines Flycast sends to `socket` until `done`: each datagram
/// is one log line, coloured for a terminal (the colours are dropped).
/// What an earlier try left unread is dropped first, so its hang line
/// can't stop this one.
fn read_log(socket: &UdpSocket, watch: Watch, done: Arc<AtomicBool>) -> Result<()> {
    let socket = socket.try_clone()?;
    socket.set_nonblocking(true)?;
    let mut buf = vec![0u8; 65536];
    while socket.recv(&mut buf).is_ok() {}
    socket.set_nonblocking(false)?;
    socket.set_read_timeout(Some(Duration::from_millis(200)))?;
    std::thread::spawn(move || {
        while !done.load(Ordering::SeqCst) {
            if let Ok(n) = socket.recv(&mut buf) {
                let text = strip_ansi(&String::from_utf8_lossy(&buf[..n]));
                for line in text.lines().filter(|l| !l.trim().is_empty()) {
                    watch.line(line);
                }
            }
        }
    });
    Ok(())
}

/// Asks the game to stop. Linux, F355: its master board alone, which saves
/// and then lets the slave go; stopping gamescope with it lets gamescope's
/// reaper kill the master before it has saved. Otherwise the whole group
/// (Windows: by the started process's windows, [`proc::Group::terminate`]).
fn ask_to_stop(group: &proc::Group, plan: &Plan) {
    #[cfg(unix)]
    if plan.one_window
        && let Some(master) = group.boards(&plan.game.args).master
    {
        group.terminate_one(master);
        return;
    }
    let _ = plan;
    group.terminate();
}

/// Whether the process that saves the game when it quits may still be
/// running. Linux, F355: its master board (once it is gone, the slave alone
/// would only sit on Flycast's error screen). Otherwise the session's own
/// process, which the caller watches.
fn saver_running(group: &proc::Group, plan: &Plan) -> bool {
    #[cfg(unix)]
    if plan.one_window {
        return group.boards(&plan.game.args).master.is_some();
    }
    let _ = (group, plan);
    true
}

/// Sets its flag when dropped.
struct SetOnDrop(Arc<AtomicBool>);

impl Drop for SetOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
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
    let group = proc::Group::of(&child, &tag, p.plan.one_window)
        .context("grouping the game's processes")?;
    #[cfg(windows)]
    if p.plan.one_cache {
        let on = pin_windows(&child).unwrap_or_else(|| "all".into());
        writeln!(log.lock().unwrap(), "# CPUs: {on}")?;
    }

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
    // stderr (or the log Flycast sends) to the log, watched for the line
    // that means the boot hung and the one that says a stopping game saved
    let watch = Watch {
        hang: p.plan.boot_hang,
        stopped: p.plan.stopped,
        started: Instant::now(),
        hung: Arc::new(AtomicBool::new(false)),
        saved: Arc::new(AtomicBool::new(false)),
        log: Arc::clone(log),
    };
    let (hung, saved) = (Arc::clone(&watch.hung), Arc::clone(&watch.saved));
    let log_done = Arc::new(AtomicBool::new(false));
    // the log reader ends with this run, however it ends
    let _log_done = SetOnDrop(Arc::clone(&log_done));
    if let Some(socket) = &p.log {
        read_log(socket, watch.clone(), Arc::clone(&log_done))?;
    }
    if let Some(stderr) = child.stderr.take() {
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                watch.line(&line);
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
    let mut stopped = false;
    // F355: the game's window, once it has opened
    let mut window = WindowWatch::default();
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break st;
        }
        if p.plan.one_window && !window.gone() {
            // Windows: the slave's window (hiding the master's); Linux: the
            // slave board's process, which ends with gamescope's window
            #[cfg(windows)]
            let first = window.look(Instant::now(), || group.game_windows().first().copied());
            #[cfg(unix)]
            let first = window.look(Instant::now(), || {
                let b = group.boards(&p.plan.game.args);
                // either board gone ends the game: the slave goes with its
                // window, the master when gamescope's reaper ends it
                (b.master.is_some() && b.slave.is_some()).then_some(())
            });
            if let Some(_w) = first {
                #[cfg(windows)]
                crate::win32::focus(_w);
                writeln!(log.lock().unwrap(), "# the game's window is open")?;
            }
            if window.gone() {
                writeln!(log.lock().unwrap(), "# the game's window closed: stopping")?;
            }
        }
        if stop.load(Ordering::SeqCst) || hung.load(Ordering::SeqCst) || window.gone() {
            if let Some((mut sink, _, _)) = script_sink.take() {
                let _ = writeln!(sink, "quit");
            }
            if let Some(tx) = &detach {
                let _ = tx.send(input::Command::Detach);
            }
            // a "saved" line from before the stop doesn't count
            saved.store(false, Ordering::SeqCst);
            ask_to_stop(&group, &p.plan);
            let deadline = Instant::now() + STOP_GRACE;
            while Instant::now() < deadline
                && !saved.load(Ordering::SeqCst)
                && child.try_wait()?.is_none()
                && saver_running(&group, &p.plan)
            {
                std::thread::sleep(Duration::from_millis(100));
            }
            group.kill();
            stopped = true;
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
    if p.plan.stopped.is_some() && !stopped {
        // the game ended by itself (gamescope closed): what's left of it
        // still gets its time to save (F355's master board, off screen)
        let deadline = Instant::now() + STOP_GRACE;
        while Instant::now() < deadline && !saved.load(Ordering::SeqCst) && group.any_left() {
            std::thread::sleep(Duration::from_millis(100));
        }
    } else {
        std::thread::sleep(Duration::from_millis(500));
    }
    group.kill();
    let left = group.leftovers();
    writeln!(
        log.lock().unwrap(),
        "# exit: {status} after {} s; stopped {left} leftover process(es)",
        start.elapsed().as_secs()
    )?;
    Ok((status, hung.load(Ordering::SeqCst)))
}

#[cfg(test)]
mod window_tests {
    use super::*;

    #[test]
    fn the_session_reads_the_hang_from_flycasts_network_log() {
        let dir = tempfile::tempdir().unwrap();
        let file = Arc::new(Mutex::new(
            fs::File::create(dir.path().join("s.log")).unwrap(),
        ));
        let watch = Watch {
            hang: Some(launch::FLYCAST_BOOT_HANG),
            stopped: Some(launch::FLYCAST_SAVED),
            started: Instant::now(),
            hung: Arc::new(AtomicBool::new(false)),
            saved: Arc::new(AtomicBool::new(false)),
            log: file,
        };
        let socket = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        let board = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        board.connect(socket.local_addr().unwrap()).unwrap();
        // the try before left a line unread: not this try's
        board
            .send(b"00:00:000 left over from the last try\n")
            .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let done = Arc::new(AtomicBool::new(false));
        read_log(&socket, watch.clone(), Arc::clone(&done)).unwrap();
        // what a Flycast board sends: one coloured line per datagram
        board
            .send(b"\x1b[92m00:00:000 N[BOOT]: Game ID is [F355]\n\x1b[0m")
            .unwrap();
        board
            .send(b"\x1b[93m00:50:954 W[INTERPRETER]: TRAP #50\n\x1b[0m")
            .unwrap();
        board
            .send(b"\x1b[92m01:10:000 N[NAOMI]: Slave exiting\n\x1b[0m")
            .unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while !(watch.hung.load(Ordering::SeqCst) && watch.saved.load(Ordering::SeqCst))
            && Instant::now() < until
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        done.store(true, Ordering::SeqCst);
        assert!(watch.hung.load(Ordering::SeqCst));
        assert!(watch.saved.load(Ordering::SeqCst));
        let text = fs::read_to_string(dir.path().join("s.log")).unwrap();
        assert!(
            text.contains("00:00:000 N[BOOT]: Game ID is [F355]\n"),
            "{text}"
        );
        assert!(!text.contains('\x1b'), "{text}");
        assert!(!text.contains("left over"), "{text}");
    }
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
            log_port: None,
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
        let pinned = command_line(&plan, &[], false, Some(vec![8, 9, 10, 11])).join(" ");
        assert!(
            pinned.contains(" -- taskset -c 8,9,10,11 /c/wal/arcade-launcher run "),
            "{pinned}"
        );
    }

    /// The full Linux command line of every game, windowed and headless,
    /// with and without bubblewrap and pinned CPUs.
    #[test]
    fn every_games_command_line() {
        let c = Components {
            lindbergh: "/c/lindbergh".into(),
            wal: "/c/wal".into(),
            runner: "/c/runner".into(),
            winetricks: "/c/winetricks".into(),
            directx: "/c/directx".into(),
            flycast: "/c/flycast".into(),
            launcher: "/pkg/sega-launcher".into(),
        };
        let libraries = ["/games".into(), "/more games".into()];
        let mut out = String::new();
        for g in sega_core::catalog::GAMES {
            let places = Places {
                runtime: PathBuf::from("/d/runtimes").join(g.slug),
                wal_root: "/d/wal".into(),
                wheel: None,
                bios: vec![],
                log_port: None,
            };
            for headless in [false, true] {
                let s = Settings::default();
                let plan = match launch::plan(g, &places, &c, &s, "eth0", headless, Host::Linux) {
                    Ok(plan) => plan,
                    Err(e) => {
                        out.push_str(&format!("== {}: {e}\n", g.slug));
                        break;
                    }
                };
                for (bwrap, set) in [(false, None), (true, Some(vec![8, 9, 10, 11]))] {
                    let shown = set.as_deref().map(cpus::taskset_list);
                    out.push_str(&format!(
                        "== {} headless {headless} bwrap {bwrap} cpus {shown:?}\n{:?}\n",
                        g.slug,
                        command_line(&plan, &libraries, bwrap, set)
                    ));
                }
            }
        }
        rea_launcher::snapshot!("command-lines.txt", out);
    }
}
