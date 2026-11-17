//! Per-game settings, stored as `key = value` lines in
//! `<data>/settings/<slug>.cfg`.
//!
//! [`OPTIONS`] names every setting once (`rea_launcher::cfg` option specs):
//! its key, its flag (`--steering-lock` for `steering_lock`), the values it
//! takes, and the label and description the window and `--help` show. The
//! file parser, the command line, `--help`, the template and the window rows
//! all come from it. Unknown keys and comments are kept when the launcher
//! rewrites the file, so a player's notes survive.

use rea_launcher::cfg::{self, KeyStyle, Kind, OptionSpec, Rejected, Resolution, Value};
use rea_launcher::display;
use rea_launcher::gamescope::{SIZE_MAX, SIZE_MIN, Size, UPSCALERS, Upscaler, fit};
use rea_launcher::wheel::SteeringLock;

/// Flags spell keys with hyphens: `--steering-lock` is `steering_lock`.
pub const STYLE: KeyStyle = KeyStyle::Underscore;

/// A look: a named set of picture values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Look {
    /// The cabinet's own picture: the game's size, smooth scaling.
    Original,
    /// Drawn sharper for the player's screen, FSR to fill it.
    Recommended,
    SteamDeck,
}

pub const LOOKS: &[(&str, &str)] = &[
    ("original", "Original"),
    ("recommended", "Recommended"),
    ("steam_deck", "Steam Deck"),
];

impl Look {
    pub const ALL: [Look; 3] = [Look::Original, Look::Recommended, Look::SteamDeck];

    pub fn key(self) -> &'static str {
        match self {
            Look::Original => "original",
            Look::Recommended => "recommended",
            Look::SteamDeck => "steam_deck",
        }
    }

    pub fn from_key(key: &str) -> Option<Look> {
        Look::ALL.into_iter().find(|l| l.key() == key)
    }

    pub fn label(self) -> &'static str {
        LOOKS
            .iter()
            .find(|(k, _)| *k == self.key())
            .map_or("", |(_, l)| l)
    }

    /// The look for `screen`: Steam Deck on a Deck's screen.
    pub fn recommended(screen: Option<(u32, u32)>) -> Look {
        if display::is_steam_deck(screen) {
            Look::SteamDeck
        } else {
            Look::Recommended
        }
    }

    /// Applies this look's values to `s` on `screen` (`native` is the
    /// cabinet's size; the game's own limits are applied when planning).
    pub fn apply(self, s: &mut Settings, native: Size, screen: Option<(u32, u32)>) {
        let screen = screen
            .and_then(display::landscape)
            .map_or(Size::new(1920, 1080), |(w, h)| Size::new(w, h));
        match self {
            Look::Original => {
                s.render = None;
                s.output = screen;
                s.upscaler = Upscaler::Linear;
            }
            Look::Recommended => {
                s.render = Some(fit(native, screen));
                s.output = screen;
                s.upscaler = Upscaler::Fsr;
            }
            Look::SteamDeck => {
                let (w, h) = display::STEAM_DECK_SCREEN;
                let deck = Size::new(w, h);
                s.render = Some(fit(native, deck));
                s.output = deck;
                s.upscaler = Upscaler::Linear;
            }
        }
        s.look = Some(self);
    }
}

const NATIVE: &cfg::Aliases = &[("native", Resolution::Original)];

pub const LOCKS: &[(&str, &str)] = &[
    ("180", "180°"),
    ("270", "270° (arcade cabinet)"),
    ("360", "360°"),
    ("540", "540°"),
    ("900", "900°"),
    ("full", "The wheel's full rotation"),
];

/// Every per-game option: settings key, flag and window control.
pub const OPTIONS: &[OptionSpec] = &[
    OptionSpec::new(
        "look",
        Kind::Choice(LOOKS),
        "Look",
        "Sets the picture options at once: the cabinet's picture, sharper for your \
         screen, or for the Steam Deck. Options set after it win.",
    ),
    OptionSpec::new(
        "render",
        Kind::Size {
            min: SIZE_MIN,
            max: SIZE_MAX,
            aliases: NATIVE,
        },
        "Game resolution",
        "The size the game draws at, or native for the cabinet's own. Sharper but heavier \
         when larger.",
    )
    .with_example("1920x1080"),
    OptionSpec::new(
        "output",
        Kind::Size {
            min: SIZE_MIN,
            max: SIZE_MAX,
            aliases: cfg::SIZES_ONLY,
        },
        "Screen size",
        "The size the picture is shown at on your screen (Linux).",
    )
    .with_example("3840x2160")
    .helper_owned(),
    OptionSpec::new(
        "upscaler",
        Kind::Choice(UPSCALERS),
        "Upscaler",
        "How the picture is enlarged to the screen size (Linux).",
    )
    .helper_owned(),
    OptionSpec::new(
        "sharpness",
        Kind::Range { min: 0, max: 20 },
        "Sharpness",
        "How sharp FSR and NVIDIA Image Scaling make the picture; 0 is sharpest (Linux).",
    )
    .helper_owned(),
    OptionSpec::new(
        "fullscreen",
        Kind::Toggle,
        "Full screen",
        "Fills the screen; off shows the game in a window (Linux).",
    )
    .helper_owned(),
    OptionSpec::new(
        "steering_lock",
        Kind::Choice(LOCKS),
        "Steering lock",
        "How far your wheel turns from full left to full right in the game; the \
         cabinets turned 270°.",
    ),
    OptionSpec::new(
        "force_feedback",
        Kind::Range { min: 0, max: 200 },
        "Force feedback",
        "The strength of the wheel's force feedback, in percent of the game's; 0 turns it off.",
    ),
    OptionSpec::new(
        "deadzone",
        Kind::Range { min: 0, max: 30 },
        "Stick dead zone",
        "How far, in percent, a pad's stick moves before the car steers.",
    ),
];

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// The look last chosen; its values are in the fields below.
    pub look: Option<Look>,
    /// The size the game itself draws at; `None`: the cabinet's own.
    pub render: Option<Size>,
    /// What gamescope shows on the screen.
    pub output: Size,
    pub upscaler: Upscaler,
    /// 0–20, gamescope's FSR/NIS sharpness (0 = sharpest).
    pub sharpness: u8,
    pub fullscreen: bool,
    pub steering_lock: SteeringLock,
    /// Force feedback strength, percent of the game's (0–200).
    pub force_feedback: u8,
    /// Pad stick dead zone, percent of the stick's travel (0–30).
    pub deadzone: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            look: None,
            render: None,
            output: Size::new(3840, 2160),
            upscaler: Upscaler::Fsr,
            sharpness: 5,
            fullscreen: true,
            steering_lock: SteeringLock::DEFAULT,
            force_feedback: 100,
            deadzone: 5,
        }
    }
}

impl Settings {
    /// Sets one key from a checked value (the option table's parser, the
    /// command line and the window all come through here).
    pub fn apply(&mut self, key: &str, value: Value) -> Result<(), Rejected> {
        match (key, value) {
            ("look", Value::Choice(v)) => {
                self.look = Look::from_key(v);
            }
            ("render", Value::Size(Resolution::Size(w, h))) => self.render = Some(Size::new(w, h)),
            ("render", Value::Size(_)) => self.render = None,
            ("output", Value::Size(Resolution::Size(w, h))) => self.output = Size::new(w, h),
            ("upscaler", Value::Choice(v)) => {
                self.upscaler = Upscaler::parse(v).ok_or(Rejected::Unknown)?;
            }
            ("sharpness", Value::Number(n)) => self.sharpness = n as u8,
            ("fullscreen", Value::Toggle(b)) => self.fullscreen = b,
            ("steering_lock", Value::Choice(v)) => {
                self.steering_lock = match v {
                    "full" => SteeringLock::Full,
                    d => SteeringLock::Degrees(d.parse().map_err(|_| Rejected::Unknown)?),
                };
            }
            ("force_feedback", Value::Number(n)) => self.force_feedback = n as u8,
            ("deadzone", Value::Number(n)) => self.deadzone = n as u8,
            _ => return Err(Rejected::Unknown),
        }
        Ok(())
    }

    /// Read a settings file; bad values fall back to the default and are reported.
    pub fn parse(text: &str) -> (Self, Vec<String>) {
        let mut s = Settings::default();
        let problems = cfg::spec::parse_file(text, "settings", OPTIONS, |k, v| s.apply(k, v));
        (s, problems)
    }

    /// The file value of `key`.
    pub fn value(&self, key: &str) -> Option<String> {
        Some(match key {
            "look" => self.look?.key().into(),
            "render" => self.render.map_or("native".into(), |z| z.to_string()),
            "output" => self.output.to_string(),
            "upscaler" => self.upscaler.key().into(),
            "sharpness" => self.sharpness.to_string(),
            "fullscreen" => cfg::on_off(self.fullscreen).into(),
            "steering_lock" => match self.steering_lock {
                SteeringLock::Full => "full".into(),
                SteeringLock::Degrees(d) => d.to_string(),
            },
            "force_feedback" => self.force_feedback.to_string(),
            "deadzone" => self.deadzone.to_string(),
            _ => return None,
        })
    }

    /// Rewrite `existing` with these settings' values, keeping comments and
    /// unknown keys in place and adding missing keys.
    pub fn save(&self, existing: &str) -> String {
        let mut out = if existing.trim().is_empty() {
            cfg::spec::template(HEADING, OPTIONS)
        } else {
            existing.to_string()
        };
        for spec in OPTIONS {
            if let Some(v) = self.value(spec.key) {
                out = cfg::spec::set_key(&out, spec, &v);
            }
        }
        out
    }

    /// The stick dead zone as a fraction of the travel.
    pub fn deadzone_fraction(&self) -> f32 {
        f32::from(self.deadzone) / 100.0
    }

    /// Force feedback strength as a factor (1.0 = the game's).
    pub fn force_feedback_gain(&self) -> f32 {
        f32::from(self.force_feedback) / 100.0
    }
}

/// Comment lines at the top of a new settings file.
pub const HEADING: &str = "Settings for one game of the Sega racing launcher.\n\
    Every key is also a command-line flag with --game <id>: steering_lock = 540 is \
    --steering-lock 540.";

/// The template a new settings file starts from (also `--help`'s source).
pub fn template() -> String {
    cfg::spec::template(HEADING, OPTIONS)
}

/// The option lines of `--help`.
pub fn usage() -> String {
    cfg::spec::usage(OPTIONS, STYLE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_keeps_comments_and_unknown_keys() {
        let text = "# my notes\nupscaler = nis\nfuture_key = 7\noutput=2560x1440\n";
        let (s, problems) = Settings::parse(text);
        assert!(
            problems.is_empty() || problems.iter().all(|p| p.contains("future_key")),
            "{problems:?}"
        );
        assert_eq!(s.upscaler, Upscaler::Nis);
        assert_eq!(s.output, Size::new(2560, 1440));
        let saved = s.save(text);
        assert!(
            saved.starts_with("# my notes\nupscaler = nis\nfuture_key = 7\n"),
            "{saved}"
        );
        assert!(saved.contains("render = native"));
        assert_eq!(Settings::parse(&saved).0, s);
    }

    #[test]
    fn bad_values_fall_back_and_are_reported() {
        let (s, p) = Settings::parse(
            "output = 99x99\nsharpness = 50\nfullscreen = maybe\nsteering_lock = 333\ndeadzone = 90",
        );
        assert_eq!(s, Settings::default());
        assert_eq!(p.len(), 5, "{p:?}");
    }

    #[test]
    fn wheel_options_round_trip() {
        let (s, p) = Settings::parse("steering_lock = full\nforce_feedback = 150\ndeadzone = 12\n");
        assert!(p.is_empty(), "{p:?}");
        assert_eq!(s.steering_lock, SteeringLock::Full);
        assert_eq!(s.force_feedback_gain(), 1.5);
        assert!((s.deadzone_fraction() - 0.12).abs() < 1e-6);
        let again = Settings::parse(&s.save("")).0;
        assert_eq!(again, s);
    }

    #[test]
    fn looks_fit_the_cabinet_shape_to_the_screen() {
        let mut s = Settings::default();
        Look::Recommended.apply(&mut s, Size::new(800, 480), Some((3840, 2160)));
        assert_eq!(s.render, Some(Size::new(3600, 2160)), "5:3 inside 16:9");
        assert_eq!(s.upscaler, Upscaler::Fsr);
        assert_eq!(s.look, Some(Look::Recommended));
        Look::SteamDeck.apply(&mut s, Size::new(1280, 720), None);
        assert_eq!(s.output, Size::new(1280, 800));
        assert_eq!(s.render, Some(Size::new(1280, 720)));
        Look::Original.apply(&mut s, Size::new(1280, 720), Some((2560, 1440)));
        assert_eq!(s.render, None);
        assert_eq!(s.output, Size::new(2560, 1440));
        assert_eq!(s.upscaler, Upscaler::Linear);
    }

    #[test]
    fn the_template_lists_every_option_commented() {
        let t = template();
        for spec in OPTIONS {
            assert!(t.contains(&format!("#{} =", spec.key)), "{}", spec.key);
        }
        assert_eq!(Settings::parse(&t).0, Settings::default());
    }
}
