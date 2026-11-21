//! One game run: build the runtime, write the loader's files, start the
//! game (on Linux in gamescope with the library read-only), feed its input,
//! wait, and leave nothing behind.

use std::fs;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use rea_launcher::cpus;
use rea_launcher::fsx;
use rea_launcher::gamescope::CommandLine;
use rea_launcher::hub::HubPaths;
use rea_launcher::library::Entry;
use rea_launcher::linkfarm::{self, Occupied};
use rea_launcher::session::{EARLY_END, WindowWatch, default_interface, early_end, strip_ansi};
use rea_launcher::wal::free_port_block;
use rea_launcher::wheel::Script;
use rea_launcher_native::feeder::{self, Feeding, Scripted, Tuning};
#[cfg(target_os = "linux")]
use rea_launcher_native::group::Member;
use rea_launcher_native::group::{Group, Tag};
use rea_launcher_native::supervise::{self, Feed, Hold, Log, Options, Outcome, Watcher};
use sega_core::catalog::{Game, Host, Route, Support};
use sega_core::input::StickLayout;
#[cfg(target_os = "linux")]
use sega_core::launch::Board;
use sega_core::launch::{
    self, DIRECTX_REDIST, FLYCAST_EXE, FLYCAST_PROGRAM, Places, Plan, XEMU_EXE, XEMU_PROGRAM,
};
use sega_core::settings::Settings;
use sega_core::{fatx, runtime};

use crate::input::{self, Stick};
use crate::paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    Screen,
    /// In a headless gamescope (tests, screenshots).
    Headless,
}

pub struct Prepared {
    pub game: Game,
    pub plan: Plan,
    pub runtime: PathBuf,
    /// The launcher's data folder (runtimes, windowsarcadeloader's prefix):
    /// the game writes there, even when a library folder holds it.
    pub data: PathBuf,
    /// Where Flycast's Windows build sends its log ([`Places::log_port`]).
    pub log: Option<UdpSocket>,
}

/// What drives the game.
pub enum Input {
    /// The input thread (pads and wheels through SDL).
    Devices {
        tx: Sender<feeder::Command>,
        tuning: Tuning,
    },
    /// A timed script (tests).
    Script(Script),
    /// Nothing (the native loaders read SDL themselves; `--no-input`).
    None,
}

impl Input {
    /// The feed of one start: windowsarcadeloader's stick lines when the
    /// game reads them, else its input held open until the stop (a closed
    /// input tells windowsarcadeloader to quit at once).
    fn feed(&self, layout: Option<StickLayout>) -> Box<dyn Feed> {
        match (self, layout) {
            (Input::Devices { tx, tuning }, Some(l)) => {
                Box::new(Feeding::new(tx.clone(), Stick::boxed(l), *tuning))
            }
            (Input::Script(script), Some(l)) => {
                Box::new(Scripted::new(Stick::boxed(l), script.clone()))
            }
            _ => Box::new(Hold::default()),
        }
    }
}

pub fn check_components(game: &Game, p: &HubPaths) -> Result<()> {
    let c = paths::components(p);
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
        Route::Xemu { .. } if windows => vec![c.xemu.join(XEMU_EXE)],
        Route::Xemu { .. } => vec![c.xemu.join(XEMU_PROGRAM)],
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
    paths: &HubPaths,
    bios_folder: Option<&Path>,
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
    linkfarm::apply(&rt, &ops, Occupied::Refuse)
        .with_context(|| format!("building {}", rt.display()))?;
    let saves_dir = sega_core::saves::dir(&paths.data, game.slug);
    sega_core::saves::sync_before(game, &rt, &saves_dir, Host::current())
        .context("syncing save files")?;
    if matches!(game.route, Route::Xemu { .. }) {
        let image = rt.join(format!("{}.bin", game.slug));
        fatx::pack_dir(&entry.dir, &image)
            .with_context(|| format!("packing {} into {}", entry.dir.display(), image.display()))?;
    }
    let mut bios = Vec::new();
    let places_list = launch::bios_places(bios_folder, &entry.dir);
    for spec in launch::bios_specs(game) {
        let found = places_list
            .iter()
            .flat_map(|d| spec.names.iter().map(move |name| d.join(name)))
            .find(|f| f.is_file());
        match found {
            Some(f) => bios.push(f),
            None if spec.optional => {}
            None => bail!(
                "{} needs {}, its board's BIOS file. Add the folder that holds it under \
                 Settings, Folders and BIOS (EmuDeck's bios folder works), or start the \
                 launcher once with --bios <folder>.",
                game.title,
                spec.names[0]
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
        // a free block of windowsarcadeloader's ports for this start
        wal_port: match game.route {
            Route::Wal { .. } => free_port_block(launch::WAL_PORT),
            _ => launch::WAL_PORT,
        },
        runtime: rt.clone(),
        wal_root: paths::wal_root(paths),
        // lindbergh-loader reads the wheel itself: tell it which one and how
        wheel: (game.route == Route::Lindbergh && display == Display::Screen)
            .then(input::lindbergh_wheel)
            .flatten(),
    };
    let nic = default_interface(&fs::read_to_string("/proc/net/route").unwrap_or_default());
    let plan = launch::plan(
        game,
        &places,
        &paths::components(paths),
        settings,
        &nic,
        display == Display::Headless,
        Host::current(),
    )
    .map_err(anyhow::Error::msg)?;
    for (file, text) in &plan.files {
        fsx::write_atomic(file, text).with_context(|| format!("writing {}", file.display()))?;
    }
    // the plan's links: in the runtime (Flycast's), or in
    // windowsarcadeloader's root (its components and runner)
    for root in [&rt, &places.wal_root] {
        let mut links = Vec::new();
        for (at, target) in plan.links.iter().filter(|(at, _)| at.starts_with(root)) {
            if let Some(d) = at.parent() {
                links.push(linkfarm::Op::Dir(d.to_path_buf()));
            }
            links.push(linkfarm::Op::Link {
                target: target.clone(),
                at: at.clone(),
            });
        }
        linkfarm::apply(root, &links, Occupied::Refuse)?;
    }
    Ok(Prepared {
        game: *game,
        plan,
        runtime: rt,
        data: paths.data.clone(),
        log,
    })
}

/// Runs the game until it ends or `stop` is set, then ends everything the
/// session started; a game whose boot hangs ([`Plan::boot_hang`]) is
/// started again. A game that ended by itself at once didn't start: that
/// is an error, with the session log's reasons.
pub fn run(
    p: &Prepared,
    libraries: &[PathBuf],
    input: &Input,
    stop: Arc<AtomicBool>,
) -> Result<ExitStatus> {
    let log_path = p.runtime.join("session.log");
    let log: Log = Arc::new(Mutex::new(fs::File::create(&log_path)?));
    let tries = if p.plan.boot_hang.is_some() {
        BOOT_TRIES
    } else {
        1
    };
    let mut attempt = 1;
    loop {
        let (outcome, hung) = run_once(p, libraries, input, &stop, &log)?;
        let saves_dir = sega_core::saves::dir(&p.data, p.game.slug);
        let _ = sega_core::saves::sync_after(&p.game, &p.runtime, &saves_dir, Host::current());
        if let Some(why) = didnt_start(&outcome, &fs::read_to_string(&log_path).unwrap_or_default())
        {
            bail!("{why}");
        }
        if !hung || stop.load(Ordering::SeqCst) || attempt == tries {
            return Ok(outcome.status);
        }
        attempt += 1;
        say(
            &log,
            &format!(
                "# the game's boards didn't link up at boot (a known Flycast hang): \
                 starting again, try {attempt} of {tries}"
            ),
        );
    }
}

/// Why a game that ended by itself at once didn't start (Windows refused
/// the program, the loader found something missing), from its session log.
fn didnt_start(o: &Outcome, log: &str) -> Option<String> {
    o.ended_early(EARLY_END)
        .then(|| early_end(o.ran, &o.status.to_string(), log))
        .flatten()
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

/// What marks the session's processes on Linux (its value: the runtime).
const TAG: &str = "SEGA_SESSION";

fn say(log: &Log, line: &str) {
    if let Ok(mut f) = log.lock() {
        let _ = writeln!(f, "{line}");
    }
}

/// One start of the game; whether its boot hung.
fn run_once(
    p: &Prepared,
    libraries: &[PathBuf],
    input: &Input,
    stop: &AtomicBool,
    log: &Log,
) -> Result<(Outcome, bool)> {
    let argv = session_argv(&p.plan, libraries, &p.data);
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .current_dir(&p.plan.game.cwd)
        .envs(p.plan.game.env.iter().map(|(k, v)| (k, v)))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if p.plan.input_pipe.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    say(log, &format!("# {}", argv.join(" ")));
    let tag = Tag::new(TAG, p.runtime.to_string_lossy());
    #[cfg(windows)]
    let tag = match p.plan.one_cache.then(windows_cpus).flatten() {
        Some(set) => {
            say(log, &format!("# CPUs: {}", cpus::taskset_list(&set)));
            tag.cpus(&set)
        }
        None => tag,
    };
    let group = Group::spawn(&mut cmd, &tag).with_context(|| format!("starting {}", argv[0]))?;
    let mut watcher = GameWatch::new(&p.plan, log.clone(), p.log.as_ref())?;
    let mut feed = input.feed(p.plan.input_pipe);
    let outcome = supervise::run(
        group,
        feed.as_mut(),
        &mut watcher,
        Options {
            stop,
            log: log.clone(),
            grace: STOP_GRACE,
            sink: None,
        },
    )?;
    Ok((outcome, watcher.hung))
}

/// Windows: the one cache domain of alike cores F355's boards run on, as
/// taskset does on Linux; set before the master's first instruction, so
/// the slave it starts inherits it.
#[cfg(windows)]
fn windows_cpus() -> Option<Vec<u32>> {
    let (all, core) =
        cpus::windows_cpus(&rea_launcher_native::group::logical_processor_information()?);
    cpus::uniform_cpus(&all, core.as_deref())
}

/// What the session knows about a game while it runs: the line that means
/// its boot hung, the line that says a stopping game has saved, and F355's
/// two boards ([`Plan::one_window`]): the game ends, politely, when either
/// goes, and only the master (which saves) is asked to stop.
struct GameWatch<'a> {
    plan: &'a Plan,
    started: Instant,
    hung: bool,
    saved: bool,
    window: WindowWatch,
    log: Log,
    /// The lines Flycast's Windows build sends ([`read_log`]).
    network: Option<Receiver<String>>,
}

impl<'a> GameWatch<'a> {
    fn new(plan: &'a Plan, log: Log, socket: Option<&UdpSocket>) -> Result<GameWatch<'a>> {
        Ok(GameWatch {
            plan,
            started: Instant::now(),
            hung: false,
            saved: false,
            window: WindowWatch::default(),
            log,
            network: socket.map(read_log).transpose()?,
        })
    }

    /// The lines Flycast's Windows build sent since the last look, into
    /// the session log and [`GameWatch::heard`].
    fn hear_network(&mut self) {
        let lines: Vec<String> = self.network.as_ref().map_or_else(Vec::new, |rx| {
            rx.try_iter().filter(|l| !l.is_empty()).collect()
        });
        for line in lines {
            say(&self.log, &line);
            self.heard(&line);
        }
    }

    /// One line of the game's log.
    fn heard(&mut self, line: &str) {
        let booting = self.started.elapsed() < BOOT_WINDOW;
        if booting && self.plan.boot_hang.is_some_and(|m| line.contains(m)) {
            self.hung = true;
        }
        if self.plan.stopped.is_some_and(|m| line.contains(m)) {
            self.saved = true;
        }
    }

    /// Why the game must stop now: its boot hung, or its window went.
    fn reason(&self) -> Option<String> {
        if self.hung {
            Some("the game's boards didn't link up at boot".into())
        } else if self.window.gone() {
            Some("the game's window closed".into())
        } else {
            None
        }
    }

    /// One look at the game's window (at most every 200 ms). Windows: the
    /// slave's window, the master's hidden; Linux: both boards running
    /// (the slave goes with its window, the master when gamescope's reaper
    /// ends it).
    fn look(&mut self, group: &Group) {
        if !self.plan.one_window {
            return;
        }
        #[cfg(windows)]
        let first = self.window.look(Instant::now(), || {
            let windows = group.windows();
            windows
                .iter()
                .filter(|w| w.pid == group.id() && w.visible)
                .for_each(|w| w.hide());
            windows
                .into_iter()
                .find(|w| w.pid != group.id() && w.visible && w.big)
        });
        #[cfg(target_os = "linux")]
        let first = self.window.look(Instant::now(), || {
            let b = boards(&group.members(), &self.plan.game.args);
            (b.master.is_some() && b.slave.is_some()).then_some(())
        });
        if let Some(_w) = first {
            #[cfg(windows)]
            _w.focus();
            say(&self.log, "# the game's window is open");
        }
    }
}

impl Watcher for GameWatch<'_> {
    fn line(&mut self, line: &str) {
        self.heard(line);
    }

    fn check(&mut self, group: &Group) -> Option<String> {
        self.hear_network();
        self.look(group);
        self.reason()
    }

    /// Linux, F355: the master board alone, which saves and then lets the
    /// slave go; stopping gamescope with it lets gamescope's reaper kill the
    /// master before it has saved. Otherwise the whole session (Windows:
    /// its started process's windows, Flycast saves then).
    fn ask_to_stop(&mut self, group: &Group) {
        // a "saved" line from before the stop doesn't count
        self.saved = false;
        #[cfg(target_os = "linux")]
        if self.plan.one_window
            && let Some(master) = boards(&group.members(), &self.plan.game.args).master
            && group.terminate_one(master)
        {
            return;
        }
        group.terminate();
    }

    /// Whether the process that saves the game may still be running: not
    /// once its saved line came; Linux, F355: while its master board runs
    /// (the slave alone would only sit on Flycast's error screen).
    fn saving(&mut self, group: &Group) -> bool {
        self.hear_network();
        #[cfg(target_os = "linux")]
        if self.plan.one_window
            && boards(&group.members(), &self.plan.game.args)
                .master
                .is_none()
        {
            return false;
        }
        let _ = group;
        !self.saved
    }

    /// F355 ended by itself (gamescope closed): its master board, off
    /// screen, still gets its time to save.
    fn saves_after_exit(&self) -> bool {
        self.plan.stopped.is_some()
    }
}

/// Which of a linked cabinet's boards are running (Linux).
#[cfg(target_os = "linux")]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Boards {
    /// The master: it reads the controls and saves the cabinet's memory
    /// when it quits.
    master: Option<u32>,
    /// The slave: it draws the game.
    slave: Option<u32>,
}

/// The session's Flycast boards among its `members`: the master is the
/// process whose arguments are exactly the session's `args` (gamescope and
/// bubblewrap carry them too, after their own), the slave the one Flycast
/// started with `naomi:BoardId=` (`launch::flycast_board`).
#[cfg(target_os = "linux")]
fn boards(members: &[Member], args: &[String]) -> Boards {
    let mut b = Boards::default();
    for m in members {
        match launch::flycast_board(&m.argv) {
            Board::Slave => b.slave = Some(m.pid),
            Board::Master if m.argv.get(1..) == Some(args) => b.master = Some(m.pid),
            Board::Master => {}
        }
    }
    b
}

/// Reads the lines Flycast sends to `socket` on a thread of its own, until
/// the receiver goes: each datagram is one log line, coloured for a
/// terminal (the colours are dropped). What an earlier try left unread is
/// dropped first, so its hang line can't stop this one.
fn read_log(socket: &UdpSocket) -> Result<Receiver<String>> {
    let socket = socket.try_clone()?;
    socket.set_nonblocking(true)?;
    let mut buf = vec![0u8; 65536];
    while socket.recv(&mut buf).is_ok() {}
    socket.set_nonblocking(false)?;
    socket.set_read_timeout(Some(Duration::from_millis(200)))?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        loop {
            let Ok(n) = socket.recv(&mut buf) else {
                // a timeout: is anyone still listening? (an empty line is no line)
                if tx.send(String::new()).is_err() {
                    return;
                }
                continue;
            };
            let text = strip_ansi(&String::from_utf8_lossy(&buf[..n]));
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                if tx.send(line.to_string()).is_err() {
                    return;
                }
            }
        }
    });
    Ok(rx)
}

/// The command the session starts: on Linux gamescope, then bubblewrap
/// with every library folder read-only (no loader can write to the
/// player's dumps), then the game on one cache domain when it wants one
/// (F355's boards); on Windows the game's own (the CPUs are set on the
/// process once started, [`pin_windows`]).
fn session_argv(plan: &Plan, libraries: &[PathBuf], data: &Path) -> Vec<String> {
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
    command_line(plan, libraries, data, which("bwrap").is_some(), cpus)
}

/// [`session_argv`] on Linux, with what this machine has decided: the
/// library read-only (each folder only if it is there), the data folder
/// writable again when a library holds it (`--library ~`).
fn command_line(
    plan: &Plan,
    libraries: &[PathBuf],
    data: &Path,
    bwrap: bool,
    cpus: Option<Vec<u32>>,
) -> Vec<String> {
    CommandLine {
        gamescope: plan.gamescope.clone(),
        homes: None,
        read_only: bwrap.then(|| libraries.to_vec()),
        writable: vec![data.to_path_buf()],
        cpus: cpus.map(|set| cpus::taskset_list(&set)),
    }
    .argv(&plan.game.program, &plan.game.args)
}

#[cfg(test)]
mod watch_tests {
    use super::*;
    use sega_core::catalog::by_slug;
    use sega_core::launch::Components;

    fn f355_plan() -> Plan {
        let g = by_slug("f355").unwrap();
        let places = Places {
            runtime: "/d/runtimes/f355".into(),
            wal_root: "/d/wal".into(),
            wheel: None,
            bios: vec!["/bios/f355bios.zip".into()],
            log_port: Some(50123),
            wal_port: launch::WAL_PORT,
        };
        let c = Components {
            lindbergh: "/c/l".into(),
            wal: "/c/wal".into(),
            runner: "/c/r".into(),
            winetricks: "/c/w".into(),
            directx: "/c/dx".into(),
            flycast: "/c/fc".into(),
            xemu: "/c/xemu".into(),
            launcher: "/pkg/sega-launcher".into(),
        };
        launch::plan(
            g,
            &places,
            &c,
            &Settings::default(),
            "lo",
            true,
            Host::current(),
        )
        .unwrap()
    }

    fn log_in(dir: &Path) -> Log {
        Arc::new(Mutex::new(fs::File::create(dir.join("s.log")).unwrap()))
    }

    /// A sink that says when it is closed.
    struct Probe(Arc<AtomicBool>);

    impl std::io::Write for Probe {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn without_input_the_loaders_input_stays_open_until_the_stop() {
        let closed = Arc::new(AtomicBool::new(false));
        let mut feed = Input::None.feed(Some(StickLayout::Sequential));
        feed.attach(Some(Box::new(Probe(Arc::clone(&closed)))));
        feed.tick(Duration::from_secs(5));
        assert!(
            !closed.load(Ordering::SeqCst),
            "closing it quits the loader"
        );
        feed.detach();
        assert!(closed.load(Ordering::SeqCst));
    }

    #[test]
    fn a_game_that_ended_by_itself_at_once_didnt_start_and_says_why() {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;
        #[cfg(windows)]
        use std::os::windows::process::ExitStatusExt;
        let outcome = |secs, ended| Outcome {
            status: std::process::ExitStatus::from_raw(1),
            ran: Duration::from_secs(secs),
            ended,
            leftovers: 0,
        };
        let log = "# arcade-launcher run\nerror: payload missing\n";
        let why = didnt_start(&outcome(2, supervise::Ended::Itself), log).unwrap();
        assert!(why.contains("closed by itself 2 s after starting"), "{why}");
        assert!(why.contains("error: payload missing"), "{why}");
        assert_eq!(
            didnt_start(&outcome(600, supervise::Ended::Itself), log),
            None
        );
        assert_eq!(
            didnt_start(&outcome(2, supervise::Ended::Stopped), log),
            None
        );
    }

    #[test]
    fn a_hung_boot_stops_the_try_and_a_saved_line_ends_the_wait() {
        let dir = tempfile::tempdir().unwrap();
        let plan = f355_plan();
        let mut w = GameWatch::new(&plan, log_in(dir.path()), None).unwrap();
        w.heard("00:00:000 N[BOOT]: Game ID is [F355]");
        assert_eq!(w.reason(), None);
        w.heard("00:50:954 W[INTERPRETER]: TRAP #50");
        assert!(w.hung);
        assert_eq!(
            w.reason().as_deref(),
            Some("the game's boards didn't link up at boot")
        );
        w.heard("01:10:000 N[NAOMI]: Slave exiting");
        assert!(w.saved);
        // a game that hangs nothing and saves nothing
        let other = Plan {
            boot_hang: None,
            stopped: None,
            ..plan.clone()
        };
        let mut w = GameWatch::new(&other, log_in(dir.path()), None).unwrap();
        w.heard("TRAP #50 Slave exiting");
        assert!(!w.hung && !w.saved);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn the_boards_are_found_by_their_arguments_never_by_name_alone() {
        let args = vec!["/d/runtimes/f355/f355twn2.zip".to_string()];
        let member = |pid, argv: &[&str]| Member {
            pid,
            argv: argv.iter().map(|a| a.to_string()).collect(),
        };
        let members = [
            member(
                10,
                &[
                    "gamescope",
                    "-w",
                    "640",
                    "--",
                    "/pkg/sega-launcher",
                    &args[0],
                ],
            ),
            member(11, &["/pkg/sega-launcher", &args[0]]),
            member(
                12,
                &["/pkg/sega-launcher", "-config", "naomi:BoardId=1", &args[0]],
            ),
            // another Flycast the player started: not the session's master
            member(13, &["flycast", "/other/f355twn2.zip"]),
        ];
        assert_eq!(
            boards(&members, &args),
            Boards {
                master: Some(11),
                slave: Some(12)
            }
        );
        assert_eq!(boards(&members[..1], &args), Boards::default());
    }

    #[test]
    fn the_session_reads_the_hang_from_flycasts_network_log() {
        let dir = tempfile::tempdir().unwrap();
        let socket = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        let board = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        board.connect(socket.local_addr().unwrap()).unwrap();
        // the try before left a line unread: not this try's
        board
            .send(b"00:00:000 left over from the last try\n")
            .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let plan = f355_plan();
        let log = log_in(dir.path());
        let mut w = GameWatch::new(&plan, log, Some(&socket)).unwrap();
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
        while !(w.hung && w.saved) && Instant::now() < until {
            w.hear_network();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(w.hung && w.saved);
        let text = fs::read_to_string(dir.path().join("s.log")).unwrap();
        assert!(
            text.contains("00:00:000 N[BOOT]: Game ID is [F355]\n"),
            "{text}"
        );
        assert!(
            !text.contains('\x1b') && !text.contains("left over"),
            "{text}"
        );
        assert!(!text.contains("\n\n"), "no empty lines: {text}");
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
            wal_port: launch::WAL_PORT,
        };
        let c = Components {
            lindbergh: "/c/l".into(),
            wal: "/c/wal".into(),
            runner: "/c/r".into(),
            winetricks: "/c/w".into(),
            directx: "/c/dx".into(),
            flycast: "/c/fc".into(),
            xemu: "/c/xemu".into(),
            launcher: "/pkg/sega-launcher".into(),
        };
        let g = game(GameId::RacingClassic);
        let plan_with =
            |s: &Settings| launch::plan(g, &places, &c, s, "eth0", true, Host::Linux).unwrap();
        let plan = plan_with(&Settings::default());
        let v = command_line(
            &plan,
            &["/lib".into(), "/more".into()],
            Path::new("/d"),
            true,
            None,
        );
        let s = v.join(" ");
        assert!(s.starts_with("gamescope -w 1280 -h 720 "), "{s}");
        assert!(s.contains(" -- env ENABLE_GAMESCOPE_WSI=0 DISABLE_GAMESCOPE_WSI=1 bwrap --dev-bind / / --ro-bind-try /lib /lib --ro-bind-try /more /more /c/wal/arcade-launcher run "), "{s}");
        // `--library ~`: the data folder inside it is bound back writable
        let home = command_line(
            &plan,
            &["/home/p".into()],
            Path::new("/home/p/.local/share/rea-sega"),
            true,
            None,
        )
        .join(" ");
        assert!(
            home.contains("--ro-bind-try /home/p /home/p --bind /home/p/.local/share/rea-sega /home/p/.local/share/rea-sega "),
            "{home}"
        );
        let plain = command_line(&plan, &["/lib".into()], Path::new("/d"), false, None);
        assert!(!plain.join(" ").contains("bwrap"));
        // F355's boards on one cache domain
        let pinned =
            command_line(&plan, &[], Path::new("/d"), false, Some(vec![8, 9, 10, 11])).join(" ");
        assert!(
            pinned.contains(
                " -- env ENABLE_GAMESCOPE_WSI=0 DISABLE_GAMESCOPE_WSI=1 taskset -c 8,9,10,11 /c/wal/arcade-launcher run "
            ),
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
            xemu: "/c/xemu".into(),
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
                wal_port: launch::WAL_PORT,
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
                        command_line(&plan, &libraries, Path::new("/d"), bwrap, set)
                    ));
                }
            }
        }
        rea_launcher::snapshot!("command-lines.txt", out);
    }
}
