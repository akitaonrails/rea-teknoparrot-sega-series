//! sega-launcher: one launcher for the Sega arcade racers on TeknoParrot
//! dumps. This file wires the command line (`cli`) to the window, the
//! sessions and the messages; the decisions live in `sega-core`.

// a GUI program on Windows: no console window on a double click; commands
// reach the terminal that started them (console::attach_parent)
#![cfg_attr(windows, windows_subsystem = "windows")]

mod cli;
mod input;
mod paths;
mod proc;
mod session;
mod ui;

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rea_launcher_win32::report::{Audience, Mode as Says, Run};
use sega_core::catalog::{self, Game, Host, Route, Support};
use sega_core::library::{self, Entry, Found, HashCache};
use sega_core::settings::Settings;

use crate::cli::{Invocation, Mode};
use crate::paths::Paths;
use crate::session::{Display, Feed};

/// The window's title; it differs from every game's (STANDARDS "One instance").
pub const WINDOW_TITLE: &str = "Sega Racing Launcher";
/// The single-instance lock's name.
const INSTANCE: &str = "rea-sega-launcher";
const ISSUES: &str =
    "https://github.com/akitaonrails/rea-teknoparrot-sega-series/issues/new/choose";

fn load_settings(paths: &Paths, slug: &str) -> Settings {
    let text = std::fs::read_to_string(paths.settings(slug)).unwrap_or_default();
    Settings::parse(&text).0
}

/// Write `settings` to a game's settings file, keeping the player's lines.
fn save_settings(paths: &Paths, slug: &str, settings: &Settings) -> std::io::Result<()> {
    ui::form::save(&paths.settings(slug), settings)
}

/// Where messages go: stderr always, a message window when the run's
/// audience may be watching without a terminal (`rea_launcher_ui::report`).
struct Out {
    says: Says,
}

impl Out {
    fn fail(&self, text: &str) {
        let message = rea_launcher_ui::report::Message {
            title: WINDOW_TITLE,
            text,
            link: Some(rea_launcher_ui::report::Link {
                label: "Report a problem",
                url: ISSUES,
            }),
            details: None,
        };
        // the SDK decides who is watching (win32::report), the native
        // message window shows it (ui::report)
        let mode = match self.says {
            Says::Command { interactive } => rea_launcher_ui::report::Mode::Command { interactive },
            Says::Play { interactive } => rea_launcher_ui::report::Mode::Play { interactive },
        };
        rea_launcher_ui::report::fatal(
            &ui::theme::palette(),
            &rea_launcher_ui::theme::Metrics::DEFAULT,
            &message,
            mode,
        );
    }
}

fn main() -> ExitCode {
    #[cfg(windows)]
    if std::env::args().len() > 1 {
        rea_launcher_win32::console::attach_parent();
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = cli::parse(args.iter().cloned());
    let run = match &parsed {
        Ok(inv) => inv.run(),
        Err(_) => Run::bad_command_line(&args),
    };
    let out = Out {
        says: Audience::of(run, false).mode(),
    };
    let inv = match parsed {
        Ok(inv) => inv,
        Err(e) => {
            eprintln!("{e}\n\n{}", cli::usage());
            return ExitCode::from(cli::BAD_COMMAND_LINE);
        }
    };
    if inv.mode == Mode::Help {
        print!("{}", cli::usage());
        return ExitCode::from(cli::OK);
    }
    let _instance = match rea_launcher::instance::single(INSTANCE) {
        Ok(guard) => guard,
        Err(running) => {
            if inv.is_action() {
                out.fail(
                    "The Sega Racing Launcher is already open. Close it first, then try again.",
                );
                return ExitCode::from(running.exit_code() as u8);
            }
            eprintln!("The Sega Racing Launcher is already open.");
            return ExitCode::from(cli::OK);
        }
    };
    match run_mode(&inv) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            out.fail(&e);
            ExitCode::from(cli::REFUSED)
        }
    }
}

fn run_mode(inv: &Invocation) -> Result<u8, String> {
    let paths = Paths::discover();
    let mut config = paths.config();
    if !inv.libraries.is_empty() || inv.bios.is_some() {
        for dir in &inv.libraries {
            if !dir.is_dir() {
                return Err(format!(
                    "{} is not a folder this computer can open.",
                    dir.display()
                ));
            }
            config.add_library(&std::path::absolute(dir).unwrap_or(dir.clone()));
        }
        if let Some(b) = &inv.bios {
            if !b.is_dir() {
                return Err(format!(
                    "{} is not a folder this computer can open.",
                    b.display()
                ));
            }
            config.chihiro_bios = Some(std::path::absolute(b).unwrap_or(b.clone()));
        }
        paths
            .save_config(&config)
            .map_err(|e| format!("Could not save the settings: {e}"))?;
    }
    match &inv.mode {
        Mode::Window => ui::run(paths)
            .map(|()| cli::OK)
            .map_err(|e| format!("{e:#}")),
        Mode::Help | Mode::Configure => Ok(cli::OK),
        Mode::List => list(&config),
        Mode::Check(game) => check(&paths, &config, game.as_deref()),
        Mode::Set(slug) => {
            let game = catalog::by_slug(slug).expect("checked by the parser");
            set(&paths, game, inv)?;
            Ok(cli::OK)
        }
        Mode::Play(slug) => {
            let game = catalog::by_slug(slug).expect("checked by the parser");
            set(&paths, game, inv)?;
            play(&paths, game, inv)
        }
    }
}

/// The options of the command line saved into the game's settings.
fn set(paths: &Paths, game: &Game, inv: &Invocation) -> Result<(), String> {
    if inv.options.is_empty() {
        return Ok(());
    }
    let mut s = load_settings(paths, game.slug);
    inv.apply(&mut s, game.native, None);
    save_settings(paths, game.slug, &s).map_err(|e| format!("Could not save the options: {e}"))
}

fn state(game: &Game) -> String {
    match game.support_on(Host::current()) {
        Support::Ready => "ready".to_string(),
        Support::Unverified(note) => format!("starts; {note}"),
        Support::Blocked(why) => format!("not yet: {why}"),
    }
}

fn scan(config: &paths::Config) -> Result<Vec<Entry>, String> {
    if config.libraries.is_empty() {
        return Err("No game folder yet: run with --library <folder>.".into());
    }
    Ok(library::scan(&config.libraries, &mut HashCache::default()))
}

fn list(config: &paths::Config) -> Result<u8, String> {
    for e in scan(config)? {
        match &e.found {
            Found::Known { game, edition } => println!(
                "{:<7} {:<30} {:<36} {}\n        {}",
                game.slug,
                game.title,
                edition.label,
                state(game),
                e.dir.display()
            ),
            Found::Unknown { file, digest } => println!(
                "unknown {file} (SHA-256 {digest}): report it at {ISSUES}\n        {}",
                e.dir.display()
            ),
        }
    }
    Ok(cli::OK)
}

/// Whether each game (or one) could start now: found in the library, a
/// known edition, supported here, its components in place.
fn check(paths: &Paths, config: &paths::Config, only: Option<&str>) -> Result<u8, String> {
    let found = scan(config)?;
    let by_game = library::by_game(&found);
    let mut problems = 0;
    for game in catalog::GAMES
        .iter()
        .filter(|g| only.is_none_or(|s| s == g.slug))
    {
        let verdict = match by_game.get(&game.id) {
            None => Err("not in your game folders".to_string()),
            Some(_) => match game.support_on(Host::current()) {
                Support::Blocked(why) => Err(format!("not yet: {why}")),
                _ => session::check_components(game, paths)
                    .map(|()| state(game))
                    .map_err(|e| format!("{e:#}")),
            },
        };
        match verdict {
            Ok(s) => println!("{:<7} ok: {s}", game.slug),
            Err(e) => {
                problems += 1;
                println!("{:<7} {e}", game.slug);
            }
        }
    }
    for e in &found {
        if let Found::Unknown { file, digest } = &e.found {
            problems += 1;
            println!(
                "unknown {file} (SHA-256 {digest}) in {}: report it at {ISSUES}",
                e.dir.display()
            );
        }
    }
    Ok(if problems == 0 { cli::OK } else { cli::REFUSED })
}

fn play(paths: &Paths, game: &'static Game, inv: &Invocation) -> Result<u8, String> {
    let config = paths.config();
    let found = scan(&config)?;
    let entry = library::by_game(&found)
        .get(&game.id)
        .map(|e| (*e).clone())
        .ok_or_else(|| format!("{} is not in your game folders.", game.title))?;
    let settings = load_settings(paths, game.slug);
    let display = if inv.test.headless {
        Display::Headless
    } else {
        Display::Screen
    };
    if session::needs_lindbergh_build(paths, game) {
        eprintln!("building lindbergh-loader (OutRun 2 SP SDX's first start; a few minutes)…");
        session::build_lindbergh(paths).map_err(|e| format!("{e:#}"))?;
    }
    let prepared =
        session::prepare(paths, game, &entry, &settings, display).map_err(|e| format!("{e:#}"))?;
    let stop = Arc::new(AtomicBool::new(false));
    for sig in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        let _ = signal_hook::flag::register(sig, Arc::clone(&stop));
    }
    if let Some(secs) = inv.test.seconds {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(secs));
            stop.store(true, Ordering::SeqCst);
        });
    }
    let input = (!inv.test.no_input
        && inv.test.script.is_none()
        && matches!(game.route, Route::Wal { .. }))
    .then(input::InputService::start);
    let feed = if let Some(f) = &inv.test.script {
        let text = std::fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display()))?;
        Feed::Script(input::Script::parse(&text).map_err(|e| format!("{}: {e}", f.display()))?)
    } else if let Some(i) = &input {
        Feed::Devices {
            tx: i.sender(),
            tuning: input::Tuning::of(&settings),
        }
    } else {
        Feed::None
    };
    let status = session::run(paths, &prepared, feed, stop).map_err(|e| format!("{e:#}"))?;
    eprintln!(
        "{} ended ({status}); log: {}",
        game.title,
        prepared.runtime.join("session.log").display()
    );
    Ok(cli::OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_saves_the_flags_and_keeps_the_players_lines() {
        let t = tempfile::tempdir().unwrap();
        let p = Paths {
            data: t.path().into(),
            components: t.path().join("c"),
        };
        std::fs::create_dir_all(t.path().join("settings")).unwrap();
        std::fs::write(p.settings("sr3"), "# mine\nupscaler = nis\n").unwrap();
        let inv = cli::parse(
            [
                "--set",
                "sr3",
                "--render",
                "2560x1440",
                "--steering-lock",
                "900",
            ]
            .map(String::from),
        )
        .unwrap();
        set(&p, catalog::by_slug("sr3").unwrap(), &inv).unwrap();
        let text = std::fs::read_to_string(p.settings("sr3")).unwrap();
        assert!(text.starts_with("# mine\nupscaler = nis\n"), "{text}");
        assert!(text.contains("render = 2560x1440"));
        assert!(text.contains("steering_lock = 900"));
    }

    #[test]
    fn the_window_title_is_no_games_title() {
        for g in catalog::GAMES {
            assert_ne!(g.title, WINDOW_TITLE);
        }
    }
}
