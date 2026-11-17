//! sega-launcher: one launcher for the Sega arcade racers on TeknoParrot
//! dumps. This file wires the command line (`rea_launcher::hub::cli`) to
//! the window, the sessions and the messages; the decisions live in
//! `sega-core`.

// a GUI program on Windows: no console window on a double click; commands
// reach the terminal that started them (console::attach_parent)
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(unix)]
mod board;
mod input;
mod paths;
mod session;
mod ui;

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rea_launcher::cfg::Value;
use rea_launcher::hub::cli::{self, BAD_COMMAND_LINE, CliSpec, Invocation, Mode, OK, REFUSED};
use rea_launcher::hub::{HubConfig, HubPaths, display_path, typed_path};
use rea_launcher::library::{Found, HashCache, Scan};
use rea_launcher::log::Log;
use rea_launcher::wheel::Script;
use rea_launcher_ui::report::{self, Link};
use rea_launcher_win32::report::{Audience, Mode as Says, Run};
use sega_core::catalog::{self, GAMES, Game, Host, Route, Support};
use sega_core::input::Button;
use sega_core::settings::{self, Look, Settings};

use crate::session::{Display, Input};

/// The window's title; it differs from every game's (STANDARDS "One instance").
pub const WINDOW_TITLE: &str = "Sega Racing Launcher";
/// The single-instance lock's name.
const INSTANCE: &str = "rea-sega-launcher";
const ISSUES: &str =
    "https://github.com/akitaonrails/rea-teknoparrot-sega-series/issues/new/choose";

/// The games' ids, as `--play` takes them.
const SLUGS: [&str; GAMES.len()] = {
    let mut v = [""; GAMES.len()];
    let mut i = 0;
    while i < v.len() {
        v[i] = GAMES[i].slug;
        i += 1;
    }
    v
};

/// What the command line knows.
const CLI: CliSpec = CliSpec {
    program: "sega-launcher",
    about: "the Sega arcade racers on TeknoParrot dumps",
    games: &SLUGS,
    options: settings::OPTIONS,
    style: settings::STYLE,
    paths: paths::FIELDS,
    offers: None,
};

/// A game's settings ([`settings::load`]: the first time, the recommended
/// look for `screen`); each line its file couldn't read goes to `warn`.
fn load_settings(
    paths: &HubPaths,
    game: &Game,
    screen: Option<(u32, u32)>,
    warn: impl Fn(&str),
) -> Result<Settings, String> {
    let file = paths.settings(game.slug);
    let (s, problems) = settings::load(&file, game, screen)
        .map_err(|e| format!("couldn't read {}: {e}", display_path(&file)))?;
    problems.iter().for_each(|p| warn(p));
    Ok(s)
}

/// A warning of the command line, on stderr.
fn warn(text: &str) {
    eprintln!("warning: {text}");
}

/// The family's view of this run, for who sees its messages.
fn run_of(inv: &Invocation) -> Run {
    Run {
        command: inv.is_command(),
        no_window: inv.no_window(),
        settings_only: false,
        linux: inv.linux,
    }
}

fn main() -> ExitCode {
    // a Flycast board the session started (F355): become Flycast
    #[cfg(unix)]
    if let Some(flycast) = std::env::var_os(sega_core::launch::FLYCAST_ENV) {
        return board::exec(flycast);
    }
    #[cfg(windows)]
    if std::env::args().len() > 1 {
        rea_launcher_win32::console::attach_parent();
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let inv = match cli::parse(&CLI, args.iter().cloned()) {
        Ok(inv) => inv,
        Err(e) => {
            eprintln!("{e}\n\n{}", cli::usage(&CLI));
            return ExitCode::from(BAD_COMMAND_LINE);
        }
    };
    if inv.mode == Mode::Help {
        print!("{}", cli::usage(&CLI));
        return ExitCode::from(OK);
    }
    let paths = HubPaths::discover(&paths::dirs());
    let log = Log::new(paths.log());
    let says = match Audience::of(run_of(&inv), false).mode() {
        Says::Command { interactive } => report::Mode::Command { interactive },
        Says::Play { interactive } => report::Mode::Play { interactive },
    };
    let fail = |text: &str| {
        let link = Link {
            label: "Report a problem",
            url: ISSUES,
        };
        rea_launcher_ui::hub::fatal(&ui::THEME, &log, text, Some(link), says);
    };
    // --list only reads: it works while the window is open
    let _instance = if inv.mode == Mode::List {
        None
    } else {
        match rea_launcher::instance::single(INSTANCE) {
            Ok(guard) => Some(guard),
            Err(running) if inv.is_action() => {
                fail("The Sega Racing Launcher is already open. Close it first, then try again.");
                return ExitCode::from(running.exit_code() as u8);
            }
            Err(_) => {
                eprintln!("The Sega Racing Launcher is already open.");
                return ExitCode::from(OK);
            }
        }
    };
    match run_mode(&paths, &inv) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            fail(&e);
            ExitCode::from(REFUSED)
        }
    }
}

fn run_mode(paths: &HubPaths, inv: &Invocation) -> Result<u8, String> {
    let mut config = paths::config(paths)
        .map_err(|e| format!("couldn't read {}: {e}", display_path(&paths.config_file())))?;
    if !inv.libraries.is_empty() || inv.paths.iter().any(Option::is_some) {
        configure(paths, &mut config, inv)?;
    }
    match &inv.mode {
        Mode::Window => ui::run(paths.clone()).map(|()| OK),
        Mode::Help | Mode::Configure => Ok(OK),
        Mode::List => list(&config),
        Mode::Check(game) => check(paths, &config, game.as_deref()),
        Mode::Set(slug) => {
            set(paths, game_of(slug), inv)?;
            Ok(OK)
        }
        Mode::Play(slug) => {
            let game = game_of(slug);
            set(paths, game, inv)?;
            play(paths, &config, game, inv)
        }
    }
}

/// A game the parser checked.
fn game_of(slug: &str) -> &'static Game {
    catalog::by_slug(slug).expect("checked by the parser")
}

/// The library folders and paths of the command line saved in `launcher.cfg`.
fn configure(paths: &HubPaths, config: &mut HubConfig, inv: &Invocation) -> Result<(), String> {
    for dir in &inv.libraries {
        let typed = dir.to_string_lossy();
        config.add_library(&typed_path(
            &typed,
            rea_launcher::hub::PathKind::Folder,
            "",
        )?);
    }
    for (n, (given, field)) in inv.paths.iter().zip(paths::FIELDS).enumerate() {
        if let Some(p) = given {
            config.paths[n] = Some(typed_path(&p.to_string_lossy(), field.kind, field.empty)?);
        }
    }
    config
        .save(&paths.config_file(), paths::FIELDS)
        .map_err(|e| format!("Could not save the settings: {e}"))
}

/// The options of the command line over `s`: a look first, so the flags
/// after it win; a picture flag without a look makes the look custom, as a
/// picture row does in the window.
fn apply_options(inv: &Invocation, s: &mut Settings, game: &Game) {
    let keys = || inv.options.iter().map(|(k, _)| *k);
    if !keys().any(|k| k == "look") && keys().any(|k| settings::PICTURE.contains(&k)) {
        s.look = None;
    }
    inv.apply_options(settings::OPTIONS, &["look"], |spec, v| match v {
        Value::Choice(key) if spec.key == "look" => {
            if let Some(look) = Look::from_key(key) {
                look.apply(s, game.native, None);
            }
        }
        v => {
            let _ = s.apply(spec.key, v);
        }
    });
}

/// The options of the command line saved into the game's settings.
fn set(paths: &HubPaths, game: &Game, inv: &Invocation) -> Result<(), String> {
    if inv.options.is_empty() {
        return Ok(());
    }
    let mut s = load_settings(paths, game, None, warn)?;
    apply_options(inv, &mut s, game);
    settings::write(&paths.settings(game.slug), &s)
        .map_err(|e| format!("Could not save the options: {e}"))
}

fn state(game: &Game) -> String {
    match game.support_on(Host::current()) {
        Support::Ready => "ready".to_string(),
        Support::Unverified(note) => format!("starts; {note}"),
        Support::Blocked(why) => format!("not yet: {why}"),
    }
}

/// The games in the library folders; what couldn't be read is said on stderr.
fn scan(config: &HubConfig) -> Result<Scan<Game>, String> {
    if config.libraries.is_empty() {
        return Err("No game folder yet: run with --library <folder>.".into());
    }
    let scan = catalog::library(HashCache::default()).scan(&config.libraries);
    for problem in &scan.problems {
        warn(&format!("couldn't read {problem}"));
    }
    Ok(scan)
}

fn list(config: &HubConfig) -> Result<u8, String> {
    for e in scan(config)?.entries {
        let dir = display_path(&e.dir);
        match &e.found {
            Found::Known { game, edition } => println!(
                "{:<7} {:<30} {:<36} {}\n        {dir}",
                game.slug,
                game.title,
                edition.label,
                state(game)
            ),
            Found::Unknown { file, digest } => {
                println!("unknown {file} (SHA-256 {digest}): report it at {ISSUES}\n        {dir}")
            }
        }
    }
    Ok(OK)
}

/// Whether each game (or the one named) could start now: found in the
/// library, a known edition, supported here, its components in place. The
/// unknown builds reported are the named game's only.
fn check(paths: &HubPaths, config: &HubConfig, only: Option<&str>) -> Result<u8, String> {
    let found = scan(config)?;
    let by_game = found.by_game();
    let asked = |g: &Game| only.is_none_or(|s| s == g.slug);
    let mut problems = 0;
    for game in GAMES.iter().filter(|g| asked(g)) {
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
    for e in &found.entries {
        if let Found::Unknown { file, digest } = &e.found
            && GAMES.iter().any(|g| asked(g) && g.main_file == file)
        {
            problems += 1;
            println!(
                "unknown {file} (SHA-256 {digest}) in {}: report it at {ISSUES}",
                display_path(&e.dir)
            );
        }
    }
    Ok(if problems == 0 { OK } else { REFUSED })
}

fn play(
    paths: &HubPaths,
    config: &HubConfig,
    game: &'static Game,
    inv: &Invocation,
) -> Result<u8, String> {
    let found = scan(config)?;
    let entry = found
        .by_game()
        .get(&game.id)
        .map(|e| (*e).clone())
        .ok_or_else(|| format!("{} is not in your game folders.", game.title))?;
    let settings = load_settings(paths, game, None, warn)?;
    let display = if inv.test.headless {
        Display::Headless
    } else {
        Display::Screen
    };
    let prepared = session::prepare(paths, paths::bios(config), game, &entry, &settings, display)
        .map_err(|e| format!("{e:#}"))?;
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
    let devices = (!inv.test.no_input
        && inv.test.script.is_none()
        && matches!(game.route, Route::Wal { .. }))
    .then(input::service);
    let feed = if let Some(f) = &inv.test.script {
        let text = std::fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display()))?;
        let script = Script::parse::<Button>(&text).map_err(|e| format!("{}: {e}", f.display()))?;
        Input::Script(script)
    } else if let Some(i) = &devices {
        Input::Devices {
            tx: i.sender(),
            tuning: input::tuning(&settings),
        }
    } else {
        Input::None
    };
    let status =
        session::run(&prepared, &config.libraries, &feed, stop).map_err(|e| format!("{e:#}"))?;
    eprintln!(
        "{} ended ({status}); log: {}",
        game.title,
        display_path(&prepared.runtime.join("session.log"))
    );
    Ok(OK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rea_launcher::gamescope::{Size, Upscaler};
    use rea_launcher::wheel::SteeringLock;

    fn parse_v(v: &[&str]) -> Invocation {
        cli::parse(&CLI, v.iter().map(|s| s.to_string())).unwrap()
    }

    fn paths_in(t: &std::path::Path) -> HubPaths {
        HubPaths {
            data: t.into(),
            components: t.join("c"),
            dev_assets: None,
        }
    }

    #[test]
    fn set_saves_the_flags_and_keeps_the_players_lines() {
        let t = tempfile::tempdir().unwrap();
        let p = paths_in(t.path());
        std::fs::create_dir_all(t.path().join("settings")).unwrap();
        std::fs::write(p.settings("sr3"), "# mine\nupscaler = nis\n").unwrap();
        let inv = parse_v(&[
            "--set",
            "sr3",
            "--render",
            "2560x1440",
            "--steering-lock",
            "900",
        ]);
        set(&p, game_of("sr3"), &inv).unwrap();
        let text = std::fs::read_to_string(p.settings("sr3")).unwrap();
        assert!(text.starts_with("# mine\nupscaler = nis\n"), "{text}");
        assert!(text.contains("render = 2560x1440"));
        assert!(text.contains("steering_lock = 900"));
    }

    #[test]
    fn every_option_flag_reaches_its_settings_key() {
        let inv = parse_v(&[
            "--set",
            "sr3",
            "--render",
            "1920x1080",
            "--output",
            "2560x1440",
            "--upscaler",
            "nis",
            "--sharpness",
            "3",
            "--fullscreen",
            "off",
            "--steering-lock",
            "540",
            "--force-feedback",
            "80",
            "--deadzone",
            "9",
        ]);
        let mut s = Settings::default();
        apply_options(&inv, &mut s, game_of("sr3"));
        assert_eq!(s.render, Some(Size::new(1920, 1080)));
        assert_eq!(s.output, Size::new(2560, 1440));
        assert_eq!(s.upscaler, Upscaler::Nis);
        assert_eq!(s.sharpness, 3);
        assert!(!s.fullscreen);
        assert_eq!(s.steering_lock, SteeringLock::Degrees(540));
        assert_eq!(s.force_feedback, 80);
        assert_eq!(s.deadzone, 9);
        let keys: Vec<_> = inv.options.iter().map(|(k, _)| *k).collect();
        for spec in settings::OPTIONS.iter().filter(|s| s.key != "look") {
            assert!(keys.contains(&spec.key), "{} untested", spec.key);
        }
    }

    #[test]
    fn a_picture_flag_without_a_look_makes_the_look_custom() {
        let mut s = Settings::default();
        Look::Recommended.apply(&mut s, Size::new(1280, 720), None);
        let inv = parse_v(&["--set", "src", "--render", "1280x720"]);
        apply_options(&inv, &mut s, game_of("src"));
        assert_eq!(s.look, None, "the values no longer match the look");
        assert_eq!(s.render, Some(Size::new(1280, 720)));
        // a wheel flag leaves the picture, and its look, alone
        Look::Recommended.apply(&mut s, Size::new(1280, 720), None);
        apply_options(
            &parse_v(&["--set", "src", "--deadzone", "3"]),
            &mut s,
            game_of("src"),
        );
        assert_eq!(s.look, Some(Look::Recommended));
    }

    #[test]
    fn a_look_sets_its_values_and_later_flags_win() {
        let inv = parse_v(&["--play", "src", "--upscaler=fsr", "--look", "steam_deck"]);
        let mut s = Settings::default();
        apply_options(&inv, &mut s, game_of("src"));
        assert_eq!(s.look, Some(Look::SteamDeck));
        assert_eq!(s.output, Size::new(1280, 800));
        assert_eq!(s.upscaler, Upscaler::Fsr, "the flag wins over the look");
    }

    #[test]
    fn the_settings_heading_names_commands_the_usage_has() {
        let usage = cli::usage(&CLI);
        let words: Vec<&str> = settings::HEADING.split_whitespace().collect();
        let named: Vec<&str> = words
            .windows(2)
            .filter(|w| w[0].starts_with("--") && w[1].starts_with("<id>"))
            .map(|w| w[0])
            .collect();
        assert!(!named.is_empty());
        for flag in named {
            assert!(usage.contains(&format!("{flag} <game>")), "{flag}");
        }
    }

    #[test]
    fn the_usage_lists_every_game_the_bios_and_every_option() {
        let u = cli::usage(&CLI);
        for g in GAMES {
            assert!(u.contains(g.slug));
        }
        assert!(u.contains("--bios <folder>"));
        for spec in settings::OPTIONS {
            assert!(u.contains(&spec.flag_name(settings::STYLE)), "{}", spec.key);
        }
    }

    #[test]
    fn the_window_title_is_no_games_title() {
        for g in GAMES {
            assert_ne!(g.title, WINDOW_TITLE);
        }
    }
}
