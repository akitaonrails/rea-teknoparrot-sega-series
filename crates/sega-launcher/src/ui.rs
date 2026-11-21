//! The launcher window: the SDK's hub (`rea_launcher_ui::hub`) for the
//! seven games, dressed as a mountain pass at night. This file is what the
//! hub needs to know about the Sega games ([`Sega`], a `HubBackend`): the
//! table, the scan, each game's settings ([`form::Form`]), its session and
//! the controllers.

pub(crate) mod form;

use std::path::PathBuf;

use eframe::egui::Color32;
use rea_launcher::catalog::Support;
use rea_launcher::cfg::OptionSpec;
use rea_launcher::hub::bindings::Bindings;
use rea_launcher::hub::{HubConfig, HubPaths, Input, display_path};
use rea_launcher::library::{Found as Known, HashCache, Scan};
use rea_launcher::log::Log;
use rea_launcher::pad::PadNav;
use rea_launcher::wheel::{Control, PadInput};
use rea_launcher_native::feeder::{Command, InputService, Snapshot};
use rea_launcher_ui::hub::{
    self, ButtonView, ControlsView, Device, Found, HubBackend, HubGame, HubLayout, HubSpec,
    HubTheme, HubWords, Note, Reporter, Scanner, Session,
};
use sega_core::catalog::{self, GAMES, Game, Host, Route};
use sega_core::input::{Button, default_pad_map};
use sega_core::settings::{self, Settings};

use crate::paths;
use crate::session::{self, Display};

use form::Form;

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// A mountain pass at night: dark asphalt, headlight amber and the
/// blue-and-white of Japanese mountain road signs.
pub const THEME: HubTheme = HubTheme {
    base: rgb(0x0B121A),
    panel: rgb(0x141E2A),
    line: rgb(0x6B7A8C),
    lamp: rgb(0xFFC857),
    lamp_hover: rgb(0xFFD784),
    sign: rgb(0x1D4F8F),
    ink: rgb(0xF4F7F2),
    muted: rgb(0x8A97A6),
    signal: rgb(0xB3261E),
    error: rgb(0xFF8A80),
    disabled: rgb(0x5E6B7A),
    focus_fill: rgb(0x1F2C3B),
    scrim: 240,
    brand: "SR",
    title: crate::WINDOW_TITLE,
    app_id: "rea-sega",
};

/// The words that differ from the other hubs.
const WORDS: HubWords = HubWords {
    setup_intro: "Type the path of a folder that holds your Sega game folders and add it. \
         The launcher looks inside for each game and never changes what's there.",
    folders_button: "Folders and BIOS",
    ..HubWords::DEFAULT
};

/// What the hub is, besides its backend.
pub fn spec(paths: HubPaths) -> HubSpec {
    HubSpec {
        theme: THEME,
        // nine settings rows fit the 720 lines
        layout: HubLayout::COMPACT_ROWS,
        words: WORDS,
        fields: paths::FIELDS,
        paths,
        version: env!("CARGO_PKG_VERSION"),
    }
}

/// Opens the window.
pub fn run(paths: HubPaths) -> Result<(), String> {
    let log = Log::new(paths.log());
    let bindings = crate::input::bindings(&paths, |w| log.warn(w));
    let mut backend = Sega::new(paths.clone(), Some(crate::input::service(&bindings)));
    backend.bindings = bindings;
    hub::run(spec(paths), backend)
}

/// A game of the table as the hub shows it.
const fn hub_game(g: &Game) -> HubGame {
    HubGame {
        slug: g.slug,
        title: g.title,
        short: g.short,
        board: g.board,
        year: g.year,
    }
}

static HUB_GAMES: [HubGame; GAMES.len()] = {
    let mut v = [hub_game(&GAMES[0]); GAMES.len()];
    let mut i = 1;
    while i < v.len() {
        v[i] = hub_game(&GAMES[i]);
        i += 1;
    }
    v
};

/// The Sega games, for the hub.
pub struct Sega {
    paths: HubPaths,
    log: Log,
    /// Each game's support on this computer (the tests set others).
    supports: Vec<Support>,
    /// Shared by every scan: a rescan during a scan reuses its digests.
    cache: HashCache,
    /// Owns SDL; `None` in the headless tests.
    input: Option<InputService>,
    /// The pads' menu buttons held at the last frame.
    pad: PadNav,
    /// The player's pad layout (`controls.cfg`).
    bindings: Bindings<Button>,
    /// The tests' controllers and next session.
    #[cfg(test)]
    fake: tests::Fake,
}

impl Sega {
    pub fn new(paths: HubPaths, input: Option<InputService>) -> Sega {
        Sega {
            log: Log::new(paths.log()),
            paths,
            supports: GAMES
                .iter()
                .map(|g| g.support_on(Host::current()))
                .collect(),
            cache: HashCache::default(),
            input,
            pad: PadNav::default(),
            bindings: Bindings::new(default_pad_map()),
            #[cfg(test)]
            fake: tests::Fake::default(),
        }
    }

    /// Hands the changed pad layout to the input thread; what the
    /// Controls page says (a save that failed also goes to `launcher.log`).
    fn bindings_changed(&self, done: Result<String, String>) -> Note {
        if let Some(i) = &self.input {
            i.set_pad_map(self.bindings.map().clone());
        }
        match done {
            Ok(said) => Note::ok(said),
            Err(e) => {
                self.log.error(&e);
                Note::error(e)
            }
        }
    }

    /// Game `i`'s settings ([`crate::load_settings`]); the lines its file
    /// couldn't read go to `launcher.log`.
    fn settings(&self, i: usize, screen: Option<(u32, u32)>) -> Result<Settings, String> {
        crate::load_settings(&self.paths, &GAMES[i], screen, |w| self.log.warn(w))
    }

    fn snapshot(&self) -> Snapshot {
        #[cfg(test)]
        if let Some(s) = &self.fake.snapshot {
            return s.clone();
        }
        self.input
            .as_ref()
            .map(InputService::snapshot)
            .unwrap_or_default()
    }

    /// The session of game `i`, found in `library`, to run on the session thread.
    fn real_session(
        &self,
        i: usize,
        library: &Scan<Game>,
        config: &HubConfig,
        screen: Option<(u32, u32)>,
    ) -> Result<Session, String> {
        let game = &GAMES[i];
        let entry = library
            .by_game()
            .get(&game.id)
            .map(|e| (*e).clone())
            .ok_or_else(|| format!("{} is not in your game folders", game.title))?;
        let settings = self.settings(i, screen)?;
        let paths = self.paths.clone();
        let config = config.clone();
        let input = match (game.route, &self.input) {
            (Route::Wal { .. }, Some(service)) => session::Input::Devices {
                tx: service.sender(),
                tuning: crate::input::tuning(&settings),
            },
            _ => session::Input::None,
        };
        Ok(Box::new(move |reporter: &Reporter| {
            let bios = paths::bios(&config);
            let prepared = session::prepare(&paths, bios, game, &entry, &settings, Display::Screen)
                .map_err(|e| format!("{e:#}"))?;
            reporter.playing();
            session::run(&prepared, &config.libraries, &input, reporter.stop_flag())
                .map(|_| ())
                .map_err(|e| format!("{e:#}"))
        }))
    }
}

/// What the Controls page shows of the input thread's `snap`.
fn controls_view(snap: &Snapshot, bindings: &Bindings<Button>) -> ControlsView {
    let c = snap.controls;
    ControlsView {
        error: snap.error.clone(),
        devices: snap
            .devices
            .iter()
            .map(|d| Device {
                name: d.name.clone(),
                kind: d.kind,
            })
            .collect(),
        driver: snap.driver,
        layout: bindings.layout(),
        layout_note: "A wheel steers with its wheel and pedals, and its paddles shift.".into(),
        steer: c.steer,
        accel: c.accel,
        brake: c.brake,
        buttons: Button::ALL
            .iter()
            .map(|b| ButtonView {
                key: b.key(),
                label: b.label().to_string(),
                pressed: c.buttons & b.bit() != 0,
            })
            .collect(),
        feeding: snap.feeding,
        lines: Vec::new(),
    }
}

impl HubBackend for Sega {
    type Library = Scan<Game>;
    type Form = Form;

    fn games(&self) -> &[HubGame] {
        &HUB_GAMES
    }

    fn blurb(&self, i: usize) -> Option<&'static str> {
        Some(GAMES[i].blurb)
    }

    fn options(&self) -> &'static [OptionSpec] {
        settings::OPTIONS
    }

    fn support(&self, game: usize) -> Support {
        self.supports[game]
    }

    /// The Chihiro and NAOMI games need the player's BIOS folder.
    fn note(&self, game: usize, config: &HubConfig) -> Option<Note> {
        let needs_bios = matches!(
            GAMES[game].route,
            Route::Xemu { .. } | Route::Flycast { .. }
        );
        (needs_bios && paths::bios(config).is_none())
            .then(|| Note::error("Needs your BIOS folder: add it in Settings."))
    }

    fn scanner(&self) -> Scanner<Scan<Game>> {
        let library = catalog::library(self.cache.clone());
        Box::new(move |roots: &[PathBuf]| library.scan(roots))
    }

    fn found(library: &Scan<Game>, game: usize) -> Option<Found> {
        let e = *library.by_game().get(&GAMES[game].id)?;
        let edition = match &e.found {
            Known::Known { edition, .. } => Some(edition.label.to_string()),
            Known::Unknown { .. } => None,
        };
        Some(Found {
            dir: e.dir.clone(),
            edition,
        })
    }

    fn unknown(library: &Scan<Game>) -> Vec<String> {
        let unreadable = library
            .problems
            .iter()
            .map(|p| format!("couldn't read {p}"));
        let unknown = library.entries.iter().filter_map(|e| match &e.found {
            Known::Unknown { file, digest } => Some(format!(
                "unknown build {file} {digest} in {}",
                display_path(&e.dir)
            )),
            Known::Known { .. } => None,
        });
        unreadable.chain(unknown).collect()
    }

    fn form(&mut self, game: usize, screen: Option<(u32, u32)>) -> Form {
        let g = &GAMES[game];
        let settings = self.settings(game, screen).unwrap_or_else(|e| {
            self.log.error(&e);
            Settings::default()
        });
        let mut form = Form::new(settings, g.native);
        // lindbergh-loader reads the wheel itself (no lock or strength from us)
        form.feeds_input = !matches!(g.route, Route::Lindbergh);
        form.render_fixed = g.render_fixed();
        form.screen = screen;
        form
    }

    fn session(
        &mut self,
        game: usize,
        library: &Scan<Game>,
        config: &HubConfig,
        screen: Option<(u32, u32)>,
    ) -> Result<Session, String> {
        #[cfg(test)]
        if let Some(s) = self.fake.session.take() {
            return Ok(s);
        }
        self.real_session(game, library, config, screen)
    }

    fn game_log(&self, game: usize) -> Option<PathBuf> {
        Some(self.paths.runtime(GAMES[game].slug).join("session.log"))
    }

    fn ended(&mut self) {
        if let Some(i) = &self.input {
            let _ = i.sender().send(Command::Detach);
        }
    }

    fn controls(&self) -> ControlsView {
        controls_view(&self.snapshot(), &self.bindings)
    }

    fn pad(&mut self) -> Vec<Input> {
        let bits = self.snapshot().nav;
        Input::from_pad(&mut self.pad, bits)
    }

    fn pad_now(&self) -> Option<(u16, Option<PadInput>)> {
        let snap = self.snapshot();
        Some((snap.nav, snap.held))
    }

    fn rebindable(&self) -> Vec<usize> {
        self.bindings.rebindable()
    }

    fn rebind(&mut self, row: usize, input: PadInput) -> Note {
        let file = self.paths.controls_file();
        let done = self.bindings.rebind_saved(row, input, &file);
        self.bindings_changed(done)
    }

    fn reset_bindings(&mut self) -> Note {
        let file = self.paths.controls_file();
        let done = self.bindings.reset_saved(&file);
        self.bindings_changed(done)
    }
}

#[cfg(test)]
mod tests;
