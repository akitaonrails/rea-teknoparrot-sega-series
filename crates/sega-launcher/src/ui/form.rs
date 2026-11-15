//! A game's settings page as data: its rows, what each shows, why one is
//! off, and how the arrows change the [`Settings`] behind it. Labels and
//! descriptions come from the option table (`sega_core::settings::OPTIONS`),
//! so the window, `--help` and the settings file say the same thing.

use std::path::Path;

use rea_launcher::cfg::OptionSpec;
use rea_launcher::wheel::SteeringLock;
use sega_core::catalog::Host;
use sega_core::settings::{self, Look, Settings, Size, Upscaler};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Look,
    Render,
    Output,
    Upscaler,
    Sharpness,
    Fullscreen,
    SteeringLock,
    ForceFeedback,
    Deadzone,
}

impl Row {
    pub const ALL: [Row; 9] = [
        Row::Look,
        Row::Render,
        Row::Output,
        Row::Upscaler,
        Row::Sharpness,
        Row::Fullscreen,
        Row::SteeringLock,
        Row::ForceFeedback,
        Row::Deadzone,
    ];

    /// The rows the settings window shows on `host`: Windows has no
    /// gamescope, so no screen-size, upscaler or full-screen rows.
    pub fn shown(host: Host) -> Vec<Row> {
        match host {
            Host::Linux => Row::ALL.to_vec(),
            Host::Windows => Row::ALL.into_iter().filter(|r| !r.spec().helper).collect(),
        }
    }

    /// The settings key (and command-line option `--<key>`) of this row.
    pub fn key(self) -> &'static str {
        match self {
            Row::Look => "look",
            Row::Render => "render",
            Row::Output => "output",
            Row::Upscaler => "upscaler",
            Row::Sharpness => "sharpness",
            Row::Fullscreen => "fullscreen",
            Row::SteeringLock => "steering_lock",
            Row::ForceFeedback => "force_feedback",
            Row::Deadzone => "deadzone",
        }
    }

    pub fn spec(self) -> &'static OptionSpec {
        rea_launcher::cfg::spec::find(settings::OPTIONS, self.key())
            .expect("every row has an option")
    }

    pub fn label(self) -> &'static str {
        self.spec().label
    }

    pub fn description(self) -> &'static str {
        self.spec().description
    }
}

/// Output sizes offered in the window; any other saved size is kept.
pub const OUTPUTS: [Size; 3] = [
    Size::new(1920, 1080),
    Size::new(2560, 1440),
    Size::new(3840, 2160),
];

/// Render sizes (`None` = the cabinet's own size).
pub const RENDERS: [Option<Size>; 4] = [
    None,
    Some(Size::new(1920, 1080)),
    Some(Size::new(2560, 1440)),
    Some(Size::new(3840, 2160)),
];

/// Force feedback steps offered by the arrows (any saved value is kept).
const FFB_STEP: i32 = 10;

const SHARPNESS_OFF: &str = "Only FSR 1 and NVIDIA Image Scaling use sharpness.";
pub const RENDER_FIXED: &str =
    "This game always draws at its own size; the upscaler still fits it to your screen.";
pub const WHEEL_OWN: &str =
    "This game reads your wheel itself: set the lock and strength in your wheel's software.";

#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub settings: Settings,
    /// The cabinet's own screen size, for "native" and the looks.
    pub native: Size,
    /// The launcher feeds this game's controls (the wheel options apply).
    pub feeds_input: bool,
    /// The player's screen, for the looks.
    pub screen: Option<(u32, u32)>,
    /// The game draws at one size only (the render row does nothing).
    pub render_fixed: bool,
}

pub fn size_name(s: Size) -> String {
    let tag = match (s.w, s.h) {
        (1920, 1080) => " (1080p)",
        (2560, 1440) => " (1440p)",
        (3840, 2160) => " (4K)",
        _ => "",
    };
    format!("{} × {}{tag}", s.w, s.h)
}

fn area(s: &Size) -> u64 {
    u64::from(s.w) * u64::from(s.h)
}

/// The curated output sizes plus the current one, smallest first.
pub fn output_choices(current: Size) -> Vec<Size> {
    let mut v = OUTPUTS.to_vec();
    if !v.contains(&current) {
        v.push(current);
        v.sort_by_key(area);
    }
    v
}

/// The curated render sizes plus the current one.
pub fn render_choices(current: Option<Size>) -> Vec<Option<Size>> {
    let mut v = RENDERS.to_vec();
    if !v.contains(&current) {
        v.push(current);
        v[1..].sort_by_key(|s| s.as_ref().map(area));
    }
    v
}

/// The next item in `all` after `cur`, `d` steps away, clamped at the ends.
fn step<T: Copy + PartialEq>(all: &[T], cur: T, d: i32) -> T {
    let i = all.iter().position(|x| *x == cur).unwrap_or(0) as i32;
    all[(i + d).clamp(0, all.len() as i32 - 1) as usize]
}

/// [`step`], wrapping around at the ends when `wrap`.
fn pick<T: Copy + PartialEq>(all: &[T], cur: T, d: i32, wrap: bool) -> T {
    let next = step(all, cur, d);
    match (wrap && next == cur, d > 0) {
        (false, _) => next,
        (true, true) => all[0],
        (true, false) => all[all.len() - 1],
    }
}

impl Form {
    pub fn new(settings: Settings, native: Size) -> Self {
        Self {
            settings,
            native,
            feeds_input: true,
            screen: None,
            render_fixed: false,
        }
    }

    /// Why a row can't be changed now, if it can't.
    pub fn disabled(&self, row: Row) -> Option<&'static str> {
        match row {
            Row::Sharpness if !matches!(self.settings.upscaler, Upscaler::Fsr | Upscaler::Nis) => {
                Some(SHARPNESS_OFF)
            }
            Row::SteeringLock | Row::ForceFeedback if !self.feeds_input => Some(WHEEL_OWN),
            Row::Render if self.render_fixed => Some(RENDER_FIXED),
            _ => None,
        }
    }

    pub fn value(&self, row: Row) -> String {
        let s = &self.settings;
        match row {
            Row::Look => s.look.map_or("Custom".into(), |l| l.label().to_string()),
            Row::Render => match s.render {
                None => format!("Native, {}", size_name(self.native)),
                Some(z) => size_name(z),
            },
            Row::Output => size_name(s.output),
            Row::Upscaler => s.upscaler.label().to_string(),
            Row::Sharpness => s.sharpness.to_string(),
            Row::Fullscreen => if s.fullscreen { "On" } else { "Off" }.to_string(),
            Row::SteeringLock => s.steering_lock.label(),
            Row::ForceFeedback => match s.force_feedback {
                0 => "Off".into(),
                n => format!("{n}%"),
            },
            Row::Deadzone => format!("{}%", s.deadzone),
        }
    }

    /// Move a row's value `d` steps (left = -1, right = +1, Enter = +1 with
    /// wrap for lists). Returns whether anything changed. Changing a
    /// picture value by hand leaves the look as "Custom".
    pub fn adjust(&mut self, row: Row, d: i32, wrap: bool) -> bool {
        if self.disabled(row).is_some() || d == 0 {
            return false;
        }
        let before = self.settings.clone();
        let s = &mut self.settings;
        match row {
            Row::Look => {
                let looks = Look::ALL;
                let cur = s.look.unwrap_or(Look::Original);
                let next = if s.look.is_none() {
                    Look::recommended(self.screen)
                } else {
                    pick(&looks, cur, d, wrap)
                };
                next.apply(s, self.native, self.screen);
            }
            Row::Render => s.render = pick(&render_choices(s.render), s.render, d, wrap),
            Row::Output => s.output = pick(&output_choices(s.output), s.output, d, wrap),
            Row::Upscaler => s.upscaler = pick(&Upscaler::ALL, s.upscaler, d, wrap),
            Row::Fullscreen => s.fullscreen = !s.fullscreen,
            Row::Sharpness => s.sharpness = (i32::from(s.sharpness) + d).clamp(0, 20) as u8,
            Row::SteeringLock => {
                s.steering_lock = pick(&SteeringLock::STEPS, s.steering_lock, d, wrap)
            }
            Row::ForceFeedback => {
                let n = (i32::from(s.force_feedback) + d * FFB_STEP).clamp(0, 200);
                s.force_feedback = n as u8;
            }
            Row::Deadzone => s.deadzone = (i32::from(s.deadzone) + d).clamp(0, 30) as u8,
        }
        if matches!(
            row,
            Row::Render | Row::Output | Row::Upscaler | Row::Sharpness | Row::Fullscreen
        ) && self.settings != before
        {
            self.settings.look = None;
        }
        self.settings != before
    }
}

/// Write `settings` into the file at `path`, keeping the player's comments
/// and unknown keys; atomically (temporary file, then rename).
pub fn save(path: &Path, settings: &Settings) -> std::io::Result<()> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension("cfg.tmp");
    std::fs::write(&tmp, settings.save(&existing))?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> Form {
        Form::new(Settings::default(), Size::new(1280, 720))
    }

    #[test]
    fn the_window_and_the_option_table_have_the_same_options() {
        let keys: Vec<&str> = Row::ALL.iter().map(|r| r.key()).collect();
        rea_launcher::cfg::spec::assert_parity(settings::OPTIONS, &keys);
    }

    #[test]
    fn output_steps_through_the_curated_sizes_and_keeps_a_custom_one() {
        let mut f = form();
        assert_eq!(f.settings.output, Size::new(3840, 2160));
        assert!(!f.adjust(Row::Output, 1, false), "clamped at the end");
        assert!(f.adjust(Row::Output, -1, false));
        assert_eq!(f.settings.output, Size::new(2560, 1440));
        assert!(f.adjust(Row::Output, 1, true));
        assert!(f.adjust(Row::Output, 1, true), "Enter wraps around");
        assert_eq!(f.settings.output, Size::new(1920, 1080));
        let custom = Size::new(3440, 1440);
        let mut f = form();
        f.settings.output = custom;
        assert_eq!(f.value(Row::Output), "3440 × 1440");
        f.adjust(Row::Output, 1, false);
        assert_eq!(f.settings.output, Size::new(3840, 2160));
    }

    #[test]
    fn render_size_starts_native_and_steps_up() {
        let mut f = form();
        assert_eq!(f.value(Row::Render), "Native, 1280 × 720");
        f.adjust(Row::Render, 1, false);
        assert_eq!(f.settings.render, Some(Size::new(1920, 1080)));
        f.adjust(Row::Render, 9, false);
        assert_eq!(f.value(Row::Render), "3840 × 2160 (4K)");
        f.adjust(Row::Render, -9, false);
        assert_eq!(f.settings.render, None);
    }

    #[test]
    fn a_look_sets_the_picture_and_a_hand_change_makes_it_custom() {
        let mut f = form();
        f.screen = Some((2560, 1440));
        assert_eq!(f.value(Row::Look), "Custom");
        assert!(f.adjust(Row::Look, 1, true));
        assert_eq!(f.settings.look, Some(Look::Recommended));
        assert_eq!(f.settings.render, Some(Size::new(2560, 1440)));
        assert_eq!(f.settings.output, Size::new(2560, 1440));
        assert!(f.adjust(Row::Look, 1, true));
        assert_eq!(f.value(Row::Look), "Steam Deck");
        assert_eq!(f.settings.output, Size::new(1280, 800));
        f.adjust(Row::Fullscreen, 1, false);
        assert_eq!(f.value(Row::Look), "Custom");
    }

    #[test]
    fn sharpness_is_off_unless_fsr_or_nis() {
        let mut f = form();
        assert!(f.adjust(Row::Sharpness, 1, false));
        assert_eq!(f.settings.sharpness, 6);
        f.adjust(Row::Upscaler, 1, false);
        f.adjust(Row::Upscaler, 1, false);
        assert_eq!(f.value(Row::Upscaler), "Smooth");
        assert!(f.disabled(Row::Sharpness).is_some());
        assert!(!f.adjust(Row::Sharpness, 1, false));
    }

    #[test]
    fn wheel_rows_step_and_clamp() {
        let mut f = form();
        assert_eq!(f.value(Row::SteeringLock), "270°");
        f.adjust(Row::SteeringLock, 1, false);
        assert_eq!(f.value(Row::SteeringLock), "360°");
        f.adjust(Row::SteeringLock, 9, false);
        assert_eq!(f.value(Row::SteeringLock), "Full");
        for _ in 0..30 {
            f.adjust(Row::ForceFeedback, 1, false);
            f.adjust(Row::Deadzone, 1, false);
        }
        assert_eq!(f.value(Row::ForceFeedback), "200%");
        assert_eq!(f.value(Row::Deadzone), "30%");
        for _ in 0..30 {
            f.adjust(Row::ForceFeedback, -1, false);
        }
        assert_eq!(f.value(Row::ForceFeedback), "Off");
    }

    #[test]
    fn a_game_that_reads_the_wheel_itself_says_why_its_wheel_rows_are_off() {
        let mut f = form();
        f.feeds_input = false;
        assert_eq!(f.disabled(Row::SteeringLock), Some(WHEEL_OWN));
        assert!(!f.adjust(Row::ForceFeedback, 1, false));
        assert!(f.disabled(Row::Deadzone).is_none());
    }

    #[test]
    fn a_game_with_one_size_says_why_its_resolution_row_is_off() {
        let mut f = form();
        f.render_fixed = true;
        assert_eq!(f.disabled(Row::Render), Some(RENDER_FIXED));
        assert!(!f.adjust(Row::Render, 1, false));
        assert!(RENDER_FIXED.ends_with('.'));
    }

    #[test]
    fn every_row_has_words_and_disabled_rows_say_why() {
        let f = form();
        for row in Row::ALL {
            assert!(!row.label().is_empty() && row.description().ends_with('.'));
            assert!(!f.value(row).is_empty());
        }
        assert!(SHARPNESS_OFF.ends_with('.') && WHEEL_OWN.ends_with('.'));
    }

    #[test]
    fn saving_keeps_notes_and_unknown_keys() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("settings/sr3.cfg");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "# my notes\nfuture = 1\n").unwrap();
        let mut f = Form::new(
            Settings::parse(&std::fs::read_to_string(&path).unwrap()).0,
            Size::new(1280, 720),
        );
        f.adjust(Row::Output, -2, false);
        f.adjust(Row::SteeringLock, 2, false);
        save(&path, &f.settings).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# my notes\nfuture = 1\n"), "{text}");
        assert!(text.contains("steering_lock = 540"));
        let back = Settings::parse(&text).0;
        assert_eq!(back, f.settings);
        assert!(!path.with_extension("cfg.tmp").exists());
    }

    #[test]
    fn windows_leaves_out_the_gamescope_rows() {
        assert_eq!(Row::shown(Host::Linux), Row::ALL.to_vec());
        assert_eq!(
            Row::shown(Host::Windows),
            vec![
                Row::Look,
                Row::Render,
                Row::SteeringLock,
                Row::ForceFeedback,
                Row::Deadzone
            ]
        );
    }
}
