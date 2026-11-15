//! The command line: what the launcher was asked to do, parsed without
//! side effects so every flag can be tested. The per-game options come
//! from the option table (`sega_core::settings::OPTIONS`), so `--help`,
//! the settings files and the window agree.

use std::path::PathBuf;

use sega_core::catalog::{self, GAMES};
use sega_core::settings::{self, Settings};

/// Exit codes (docs/STANDARDS.md, "Family consistency").
pub const OK: u8 = 0;
pub const REFUSED: u8 = 1;
pub const BAD_COMMAND_LINE: u8 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// The window (no arguments).
    Window,
    Help,
    /// List the games found and their state.
    List,
    /// Check that a game (or every game found) can start, without starting it.
    Check(Option<String>),
    /// Save options for a game without starting it.
    Set(String),
    /// Start a game without the window.
    Play(String),
    /// Only the library or BIOS folder was given: save it.
    Configure,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Test {
    pub headless: bool,
    pub no_input: bool,
    pub script: Option<PathBuf>,
    pub seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Invocation {
    pub mode: Mode,
    pub libraries: Vec<PathBuf>,
    pub bios: Option<PathBuf>,
    /// `(key, value)` per option flag, values already checked.
    pub options: Vec<(&'static str, String)>,
    pub test: Test,
    /// `--linux`: started by the Linux helper or the AppImage.
    pub linux: bool,
}

impl Invocation {
    /// The family's view of this run, for who sees its messages.
    pub fn run(&self) -> rea_launcher_win32::report::Run {
        rea_launcher_win32::report::Run {
            command: matches!(self.mode, Mode::Help | Mode::List | Mode::Check(_)),
            no_window: matches!(self.mode, Mode::Play(_) | Mode::Set(_) | Mode::Configure),
            settings_only: false,
            linux: self.linux,
        }
    }

    /// An action that must not run while another launcher is open.
    pub fn is_action(&self) -> bool {
        matches!(
            self.mode,
            Mode::Check(_) | Mode::Set(_) | Mode::Play(_) | Mode::Configure
        )
    }

    /// The options applied over `s` (a look first, so the options after it
    /// win), for the game whose cabinet screen is `native`.
    pub fn apply(&self, s: &mut Settings, native: settings::Size, screen: Option<(u32, u32)>) {
        for (key, value) in &self.options {
            if *key == "look"
                && let Some(look) = settings::Look::from_key(value)
            {
                look.apply(s, native, screen);
            }
        }
        for (key, value) in &self.options {
            if *key == "look" {
                continue;
            }
            if let Some(spec) = rea_launcher::cfg::spec::find(settings::OPTIONS, key)
                && let Ok(v) = spec.parse(value)
            {
                let _ = s.apply(key, v);
            }
        }
    }
}

pub fn usage() -> String {
    let ids: Vec<&str> = GAMES.iter().map(|g| g.slug).collect();
    format!(
        "sega-launcher — the Sega arcade racers on TeknoParrot dumps\n\n\
         \x20 sega-launcher                       open the window (games, settings, controls)\n\
         \x20 sega-launcher --library <folder>    add a folder with your games (repeatable)\n\
         \x20 sega-launcher --chihiro-bios <folder>  your Chihiro BIOS folder\n\
         \x20 sega-launcher --list                list the games found and their state\n\
         \x20 sega-launcher --check [<game>]      check a game (or all) can start, without starting\n\
         \x20 sega-launcher --play <game> [options]  start a game ({})\n\
         \x20 sega-launcher --set <game> <options>   save options for a game without starting it\n\n\
         Options (saved in that game's settings):\n{}\n\
         Test options for --play:\n\
         \x20 --headless               run in an invisible display\n\
         \x20 --no-input               no controller input\n\
         \x20 --input-script <file>    timed inputs (windowsarcadeloader games)\n\
         \x20 --seconds <n>            stop the game after n seconds\n\n\
         \x20 sega-launcher --help                this text\n",
        ids.join(", "),
        settings::usage()
    )
}

fn game_id(v: &str) -> Result<String, String> {
    catalog::by_slug(v)
        .map(|g| g.slug.to_string())
        .ok_or_else(|| {
            let ids: Vec<&str> = GAMES.iter().map(|g| g.slug).collect();
            format!("unknown game {v}; the games are {}", ids.join(", "))
        })
}

/// Records the one mode a command line asks for; a second, different one is an error.
fn set_mode(mode: &mut Option<Mode>, m: Mode) -> Result<(), String> {
    match mode {
        Some(old) if *old != m => Err("give one of --list, --check, --play or --set".into()),
        _ => {
            *mode = Some(m);
            Ok(())
        }
    }
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Invocation, String> {
    let mut it = args.into_iter().peekable();
    let mut mode = None;
    let mut inv = Invocation {
        mode: Mode::Window,
        libraries: Vec::new(),
        bios: None,
        options: Vec::new(),
        test: Test::default(),
        linux: false,
    };
    while let Some(arg) = it.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
            _ => (arg.clone(), None),
        };
        let mut value = |what: &str| -> Result<String, String> {
            inline
                .clone()
                .or_else(|| it.next())
                .ok_or_else(|| format!("{flag} needs {what}"))
        };
        match flag.as_str() {
            "--help" | "-h" => {
                return Ok(Invocation {
                    mode: Mode::Help,
                    ..inv
                });
            }
            "--library" => inv.libraries.push(value("a folder")?.into()),
            "--chihiro-bios" => inv.bios = Some(value("a folder")?.into()),
            "--list" => set_mode(&mut mode, Mode::List)?,
            "--check" => {
                let game = match it.peek() {
                    Some(next) if !next.starts_with("--") => {
                        Some(game_id(&it.next().unwrap_or_default())?)
                    }
                    _ => None,
                };
                set_mode(&mut mode, Mode::Check(game))?;
            }
            "--play" => set_mode(&mut mode, Mode::Play(game_id(&value("a game")?)?))?,
            "--set" => set_mode(&mut mode, Mode::Set(game_id(&value("a game")?)?))?,
            "--linux" => inv.linux = true,
            "--headless" => inv.test.headless = true,
            "--no-input" => inv.test.no_input = true,
            "--input-script" => inv.test.script = Some(value("a file")?.into()),
            "--seconds" => {
                inv.test.seconds = Some(
                    value("a number")?
                        .parse()
                        .map_err(|_| "--seconds needs a number".to_string())?,
                )
            }
            f => {
                let spec = settings::OPTIONS
                    .iter()
                    .find(|s| s.flag_name(settings::STYLE) == f)
                    .ok_or_else(|| format!("unknown option {f}"))?;
                let v = value(&spec.placeholder())?;
                spec.parse(&v)
                    .map_err(|e| format!("bad value for {f}: {v} ({e:?})"))?;
                inv.options.push((spec.key, v));
            }
        }
    }
    inv.mode = match mode {
        Some(m) => m,
        None if !inv.options.is_empty() => {
            return Err("options need --play <game> or --set <game>".into());
        }
        None if !inv.libraries.is_empty() || inv.bios.is_some() => Mode::Configure,
        None => Mode::Window,
    };
    if inv.test != Test::default() && !matches!(inv.mode, Mode::Play(_)) {
        return Err("the test options go with --play <game>".into());
    }
    Ok(inv)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rea_launcher::wheel::SteeringLock;
    use sega_core::settings::{Look, Size, Upscaler};

    fn parse_v(v: &[&str]) -> Result<Invocation, String> {
        parse(v.iter().map(|s| s.to_string()))
    }

    #[test]
    fn no_arguments_open_the_window() {
        assert_eq!(parse_v(&[]).unwrap().mode, Mode::Window);
        assert_eq!(parse_v(&["--help"]).unwrap().mode, Mode::Help);
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
        ])
        .unwrap();
        assert_eq!(inv.mode, Mode::Set("sr3".into()));
        let mut s = Settings::default();
        inv.apply(&mut s, Size::new(1280, 720), None);
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
    fn a_look_sets_its_values_and_later_flags_win() {
        let inv = parse_v(&["--play", "src", "--look", "steam_deck", "--upscaler=fsr"]).unwrap();
        let mut s = Settings::default();
        inv.apply(&mut s, Size::new(1280, 720), Some((3840, 2160)));
        assert_eq!(s.look, Some(Look::SteamDeck));
        assert_eq!(s.output, Size::new(1280, 800));
        assert_eq!(s.upscaler, Upscaler::Fsr, "the flag after the look wins");
    }

    #[test]
    fn bad_command_lines_say_what_is_wrong() {
        for (args, words) in [
            (&["--play", "id9"][..], "unknown game id9"),
            (
                &["--steering-lock", "333", "--play", "sr3"][..],
                "bad value for --steering-lock",
            ),
            (&["--frobnicate"][..], "unknown option --frobnicate"),
            (&["--render", "1920x1080"][..], "options need --play"),
            (&["--headless"][..], "the test options go with --play"),
            (&["--play"][..], "--play needs a game"),
            (&["--list", "--play", "sr3"][..], "give one of"),
        ] {
            let e = parse_v(args).unwrap_err();
            assert!(e.contains(words), "{args:?}: {e}");
        }
    }

    #[test]
    fn library_and_bios_alone_configure() {
        let inv = parse_v(&["--library", "/games", "--chihiro-bios", "/bios"]).unwrap();
        assert_eq!(inv.mode, Mode::Configure);
        assert_eq!(inv.libraries, [PathBuf::from("/games")]);
        assert!(inv.is_action());
    }

    #[test]
    fn check_takes_an_optional_game() {
        assert_eq!(parse_v(&["--check"]).unwrap().mode, Mode::Check(None));
        assert_eq!(
            parse_v(&["--check", "or2sdx"]).unwrap().mode,
            Mode::Check(Some("or2sdx".into()))
        );
        let run = parse_v(&["--check", "--linux"]).unwrap().run();
        assert!(run.command && run.linux);
    }

    #[test]
    fn play_runs_are_play_runs_for_the_messages() {
        let run = parse_v(&["--play", "sr3", "--headless", "--seconds", "5"])
            .unwrap()
            .run();
        assert!(!run.command && run.no_window);
    }

    #[test]
    fn the_usage_lists_every_game_and_option() {
        let u = usage();
        for g in GAMES {
            assert!(u.contains(g.slug));
        }
        for spec in settings::OPTIONS {
            assert!(u.contains(&spec.flag_name(settings::STYLE)), "{}", spec.key);
        }
    }
}
