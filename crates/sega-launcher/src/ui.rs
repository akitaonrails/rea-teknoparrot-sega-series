//! The launcher window: a hub for the six games, dressed as a mountain
//! pass at night. Full screen, laid out for 720 lines and zoomed to the
//! window (it runs on 4K TVs), driven by the mouse or the keyboard. Games
//! run on a worker thread, so the window never blocks.
//!
//! Everything is painted, so each control and each line a player needs to
//! read is also named for assistive technology ([`semantic`]); the headless
//! tests find the window's parts through that tree.

pub(crate) mod form;
mod model;
pub(crate) mod theme;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use eframe::egui::accesskit::{self, Role, Toggled};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontId, Id, Mesh, Pos2, Rect, Sense, Shape,
    Stroke, StrokeKind, TextureHandle, Vec2, pos2, vec2,
};
use rea_launcher::wheel::{Action, Control, Kind};
use sega_core::catalog::{GAMES, Game, Host, Route, Support};
use sega_core::input::{Button, default_pad_map};
use sega_core::library::{self, Entry, Found, HashCache};

use crate::input::{Command, InputService, Snapshot};
use crate::paths::{Config, Paths};
use crate::session::{self, Display, Feed};

use form::{Form, Row};
use model::{Effect, Focus, Input, Item, Model, Page, Tab, TileState, World};
use theme::{BASE, DISABLED, ERROR, INK, LAMP, LINE, MUTED, PANEL, SIGN, SIGNAL, alpha, size};

const TITLE: &str = "Sega Racing Launcher";
/// A settings row's height: nine rows fit the 720-line layout.
const ROW_H: f32 = 50.0;
const MARGIN: f32 = 48.0;
const BAR_H: f32 = 56.0;

pub fn run(paths: Paths) -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(TITLE)
            .with_app_id("rea-sega")
            .with_inner_size([1280.0, 720.0])
            .with_min_inner_size([640.0, 360.0])
            .with_fullscreen(true),
        ..Default::default()
    };
    eframe::run_native(
        TITLE,
        options,
        Box::new(move |cc| {
            theme::install_fonts(&cc.egui_ctx, &paths.fonts());
            theme::install_style(&cc.egui_ctx);
            Ok(Box::new(App::new(paths, Some(InputService::start()))))
        }),
    )
    .map_err(|e| anyhow::anyhow!("the window could not open: {e}"))
}

// ---------------------------------------------------------------- state

/// What the game thread reports.
enum Progress {
    /// lindbergh-loader is being built (OutRun 2 SP SDX's first start).
    Building,
    /// The runtime folder is built; the game is starting.
    Playing,
    Ended(Result<(), String>),
}

struct Running {
    game: &'static Game,
    building: bool,
    playing: bool,
    progress: Receiver<Progress>,
    /// Asks the session to stop the game.
    stop: Arc<AtomicBool>,
}

struct Message {
    title: String,
    body: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArtKind {
    Hero,
    Tile,
}

/// A line under a setup field: what happened, and whether it worked.
type Note = Option<(String, bool)>;

struct App {
    paths: Paths,
    config: Config,
    /// Owns SDL; `None` in the headless tests.
    input: Option<InputService>,
    snap: Snapshot,
    model: Model,
    /// The last scan; `None` while looking.
    entries: Option<Vec<Entry>>,
    scan: Option<Receiver<(Vec<Entry>, HashCache)>>,
    cache: Option<HashCache>,
    form: Option<(usize, Form)>,
    /// The last save: text, and whether it worked.
    note: Note,
    heroes: Vec<Option<TextureHandle>>,
    tiles: Vec<Option<TextureHandle>>,
    art: Receiver<(usize, ArtKind, egui::ColorImage)>,
    /// The hero picture being faded from, and when the fade started.
    fade: Option<(usize, f64)>,
    running: Option<Running>,
    message: Option<Message>,
    folder: String,
    folder_note: Note,
    bios: String,
    bios_note: Note,
    /// Each game's support, from the catalog (the tests set others).
    supports: Vec<Support>,
    /// The monitor's size in pixels, for the looks.
    screen: Option<(u32, u32)>,
}

impl App {
    fn new(paths: Paths, input: Option<InputService>) -> Self {
        let config = paths.config();
        let art = load_art(&paths);
        let mut app = Self {
            model: Model::new(GAMES.len(), !config.libraries.is_empty()),
            config,
            input,
            snap: Snapshot::default(),
            entries: None,
            scan: None,
            cache: Some(HashCache::default()),
            form: None,
            note: None,
            heroes: vec![None; GAMES.len()],
            tiles: vec![None; GAMES.len()],
            art,
            fade: None,
            running: None,
            message: None,
            folder: String::new(),
            folder_note: None,
            bios: String::new(),
            bios_note: None,
            supports: GAMES
                .iter()
                .map(|g| g.support_on(Host::current()))
                .collect(),
            screen: None,
            paths,
        };
        app.start_scan();
        app
    }

    fn start_scan(&mut self) {
        if self.config.libraries.is_empty() {
            return;
        }
        let roots = self.config.libraries.clone();
        let mut cache = self.cache.take().unwrap_or_default();
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("scan".into())
            .spawn(move || {
                let found = library::scan(&roots, &mut cache);
                let _ = tx.send((found, cache));
            })
            .expect("spawn scan thread");
        self.entries = None;
        self.scan = Some(rx);
    }

    fn poll_scan(&mut self) {
        let Some(rx) = &self.scan else { return };
        match rx.try_recv() {
            Ok((found, cache)) => {
                for e in &found {
                    if let Found::Unknown { file, digest } = &e.found {
                        log_line(
                            &self.paths,
                            &format!("unknown build {file} {digest} in {}", e.dir.display()),
                        );
                    }
                }
                self.entries = Some(found);
                self.cache = Some(cache);
                self.scan = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.entries = Some(vec![]);
                self.scan = None;
            }
        }
    }

    fn poll_art(&mut self, ctx: &egui::Context) {
        for (i, kind, img) in self.art.try_iter() {
            let name = format!("{}-{}", GAMES[i].slug, kind as u8);
            let tex = ctx.load_texture(name, img, egui::TextureOptions::LINEAR);
            match kind {
                ArtKind::Hero => self.heroes[i] = Some(tex),
                ArtKind::Tile => self.tiles[i] = Some(tex),
            }
        }
    }

    fn entry(&self, i: usize) -> Option<&Entry> {
        self.entries
            .as_ref()?
            .iter()
            .find(|e| e.game_id() == Some(GAMES[i].id))
    }

    fn state(&self, i: usize) -> TileState {
        let game = Game {
            support: self.supports[i],
            ..GAMES[i]
        };
        TileState::of(&game, self.entries.is_some(), self.entry(i).is_some())
    }

    fn world(&self) -> World {
        World {
            playable: self.state(self.model.selected).playable() && self.running.is_none(),
            has_library: !self.config.libraries.is_empty(),
        }
    }

    fn found_count(&self) -> usize {
        (0..GAMES.len())
            .filter(|i| self.entry(*i).is_some())
            .count()
    }

    fn ensure_form(&mut self) {
        let i = self.model.selected;
        if self.form.as_ref().map(|(at, _)| *at) != Some(i) {
            let g = &GAMES[i];
            let mut form = Form::new(crate::load_settings(&self.paths, g.slug), g.native);
            // lindbergh-loader reads the wheel itself (no lock or strength from us)
            form.feeds_input = !matches!(g.route, Route::Lindbergh);
            form.screen = self.screen;
            self.form = Some((i, form));
            self.note = None;
        }
    }

    // ------------------------------------------------------------ input

    fn gather(&mut self, ctx: &egui::Context) -> Vec<Input> {
        if self.running.is_some() {
            return vec![];
        }
        let typing = self.model.page == Page::FirstRun
            && matches!(self.model.focus, Focus::Field | Focus::BiosField);
        let mut out = vec![];
        ctx.input(|i| {
            let keys: &[(egui::Key, Input)] = if typing {
                &[
                    (egui::Key::Enter, Input::Press),
                    (egui::Key::Escape, Input::Escape),
                    (egui::Key::ArrowUp, Input::Up),
                    (egui::Key::ArrowDown, Input::Down),
                ]
            } else {
                &[
                    (egui::Key::ArrowUp, Input::Up),
                    (egui::Key::ArrowDown, Input::Down),
                    (egui::Key::ArrowLeft, Input::Left),
                    (egui::Key::ArrowRight, Input::Right),
                    (egui::Key::Enter, Input::Press),
                    (egui::Key::Space, Input::Press),
                    (egui::Key::Escape, Input::Escape),
                    (egui::Key::Backspace, Input::Back),
                    (egui::Key::PageUp, Input::PreviousTab),
                    (egui::Key::PageDown, Input::NextTab),
                ]
            };
            for (k, inp) in keys {
                if i.key_pressed(*k) {
                    out.push(*inp);
                }
            }
        });
        out
    }

    fn handle(&mut self, input: Input, ctx: &egui::Context) {
        if self.message.is_some() {
            if matches!(input, Input::Press | Input::Back | Input::Escape) {
                self.message = None;
            }
            return;
        }
        let before = self.model.selected;
        let effect = self.model.input(input, self.world());
        self.apply(effect, ctx);
        if self.model.selected != before {
            self.selected_changed(before, ctx);
        }
    }

    fn selected_changed(&mut self, from: usize, ctx: &egui::Context) {
        self.fade = Some((from, ctx.input(|i| i.time)));
        let w = self.world();
        self.model.settle(w);
    }

    fn apply(&mut self, effect: Effect, ctx: &egui::Context) {
        match effect {
            Effect::None => {}
            Effect::Play => self.play(),
            Effect::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Effect::Adjust(row, d, wrap) => self.adjust(row, d, wrap),
            Effect::Rescan => self.start_scan(),
            Effect::SubmitFolder => self.submit_folder(),
            Effect::SubmitBios => self.submit_bios(),
            Effect::Finish => {
                self.model.open(Page::Hub);
                if self.entries.is_none() && self.scan.is_none() {
                    self.start_scan();
                }
            }
            Effect::OpenedSettings => self.ensure_form(),
            Effect::OpenedFolder => {
                self.folder.clear();
                self.bios = self
                    .config
                    .chihiro_bios
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                self.folder_note = None;
                self.bios_note = None;
            }
        }
    }

    fn adjust(&mut self, row: Row, d: i32, wrap: bool) {
        self.ensure_form();
        let Some((i, f)) = self.form.as_mut() else {
            return;
        };
        if !f.adjust(row, d, wrap) {
            return;
        }
        let path = self.paths.settings(GAMES[*i].slug);
        self.note = Some(match form::save(&path, &f.settings) {
            Ok(()) => (
                "Saved. The game uses it the next time you press Play.".to_string(),
                true,
            ),
            Err(e) => (
                format!(
                    "Couldn't save to {}: {e}. Check that the folder can be written.",
                    path.display()
                ),
                false,
            ),
        });
    }

    fn save_config(&self) -> Result<(), String> {
        self.paths.save_config(&self.config).map_err(|e| {
            format!(
                "Couldn't save the choice ({e}). Check that {} can be written.",
                self.paths.data.display()
            )
        })
    }

    fn submit_folder(&mut self) {
        let dir = typed_path(&self.folder);
        if self.folder.trim().is_empty() {
            self.folder_note = Some(("Type the folder's path first.".into(), false));
            return;
        }
        if !dir.is_dir() {
            self.folder_note = Some((
                format!(
                    "{} isn't a folder this computer can open. Check the path and try again.",
                    dir.display()
                ),
                false,
            ));
            return;
        }
        let dir = std::path::absolute(&dir).unwrap_or(dir);
        self.config.add_library(&dir);
        if let Err(e) = self.save_config() {
            self.folder_note = Some((e, false));
            return;
        }
        log_line(&self.paths, &format!("library added: {}", dir.display()));
        self.folder_note = Some((format!("Added {}.", dir.display()), true));
        self.folder.clear();
        self.start_scan();
    }

    fn submit_bios(&mut self) {
        let file = typed_path(&self.bios);
        if self.bios.trim().is_empty() {
            self.bios_note = Some((
                "Type the path of your Chihiro BIOS folder first.".into(),
                false,
            ));
            return;
        }
        if !file.is_dir() {
            self.bios_note = Some((
                format!(
                    "{} isn't a folder this computer can open. Check the path and try again.",
                    file.display()
                ),
                false,
            ));
            return;
        }
        self.config.chihiro_bios = Some(std::path::absolute(&file).unwrap_or(file));
        self.bios_note = Some(match self.save_config() {
            Ok(()) => ("Chihiro BIOS folder saved.".to_string(), true),
            Err(e) => (e, false),
        });
    }

    // ------------------------------------------------------------ a game

    fn play(&mut self) {
        let i = self.model.selected;
        if self.running.is_some() || !self.state(i).playable() {
            return;
        }
        let Some(entry) = self.entry(i).cloned() else {
            return;
        };
        let game = &GAMES[i];
        let settings = crate::load_settings(&self.paths, game.slug);
        let paths = self.paths.clone();
        let sender = self.input.as_ref().map(InputService::sender);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_for_session = Arc::clone(&stop);
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("session".into())
            .spawn(move || {
                if session::needs_lindbergh_build(&paths, game) {
                    let _ = tx.send(Progress::Building);
                    if let Err(e) = session::build_lindbergh(&paths) {
                        let _ = tx.send(Progress::Ended(Err(format!("{e:#}"))));
                        return;
                    }
                }
                let prepared =
                    match session::prepare(&paths, game, &entry, &settings, Display::Screen) {
                        Ok(p) => p,
                        Err(e) => {
                            let _ = tx.send(Progress::Ended(Err(format!("{e:#}"))));
                            return;
                        }
                    };
                let _ = tx.send(Progress::Playing);
                let feed = match (game.route, sender) {
                    (Route::Wal { .. }, Some(tx)) => Feed::Devices {
                        tx,
                        tuning: crate::input::Tuning::of(&settings),
                    },
                    _ => Feed::None,
                };
                let r = session::run(&paths, &prepared, feed, stop_for_session)
                    .map(|_| ())
                    .map_err(|e| format!("{e:#}"));
                let _ = tx.send(Progress::Ended(r));
            })
            .expect("spawn session thread");
        self.running = Some(Running {
            game,
            building: false,
            playing: false,
            progress: rx,
            stop,
        });
    }

    fn poll_running(&mut self, ctx: &egui::Context) {
        let Some(run) = self.running.as_mut() else {
            return;
        };
        if ctx.input(|i| i.viewport().close_requested()) {
            // closing the window must not leave the game behind
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            run.stop.store(true, Ordering::SeqCst);
        }
        let game = run.game;
        let log_hint = format!("Details are in {}.", self.paths.log().display());
        let ended = loop {
            match run.progress.try_recv() {
                Ok(Progress::Building) => run.building = true,
                Ok(Progress::Playing) => run.playing = true,
                Ok(Progress::Ended(r)) => break Some(r),
                Err(TryRecvError::Empty) => break None,
                Err(TryRecvError::Disconnected) => {
                    break Some(Err("the game ended unexpectedly".into()));
                }
            }
        };
        let Some(r) = ended else { return };
        let started = run.playing;
        self.running = None;
        if let Some(i) = &self.input {
            let _ = i.sender().send(Command::Detach);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        if let Err(e) = r {
            self.message = Some(if started {
                Message {
                    title: format!("{} stopped with an error", game.title),
                    body: format!(
                        "{}.\n\nTry again. If it happens again: {log_hint}",
                        sentence(&e)
                    ),
                }
            } else {
                Message {
                    title: format!("{} couldn't start", game.title),
                    body: format!(
                        "{}.\n\nCheck that the game folder is complete, then try again. {log_hint}",
                        sentence(&e)
                    ),
                }
            });
        }
    }
}

/// A path as typed: `~/` is the home folder.
fn typed_path(typed: &str) -> PathBuf {
    let typed = typed.trim();
    match typed.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(rest),
        None => PathBuf::from(typed),
    }
}

/// Append a line to `launcher.log`.
fn log_line(paths: &Paths, line: &str) {
    use std::io::Write;
    let _ = std::fs::create_dir_all(&paths.data);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.log())
    {
        let _ = writeln!(f, "{line}");
    }
}

/// An error message as a sentence: capital first letter, no final stop
/// (the caller adds one).
fn sentence(e: &str) -> String {
    let e = e.trim().trim_end_matches('.');
    let mut c = e.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => "Something went wrong".into(),
    }
}

/// Decode the pictures off the window's thread: the tiles first, then the
/// big ones.
fn load_art(paths: &Paths) -> Receiver<(usize, ArtKind, egui::ColorImage)> {
    let (tx, rx) = mpsc::channel();
    let jobs: Vec<(usize, ArtKind, PathBuf)> = (0..GAMES.len())
        .map(|i| {
            (
                i,
                ArtKind::Tile,
                paths.art(&format!("{}-tile.jpg", GAMES[i].slug)),
            )
        })
        .chain((0..GAMES.len()).map(|i| {
            (
                i,
                ArtKind::Hero,
                paths.art(&format!("{}.jpg", GAMES[i].slug)),
            )
        }))
        .collect();
    std::thread::Builder::new()
        .name("art".into())
        .spawn(move || {
            for (i, kind, path) in jobs {
                let Ok(img) = image::open(&path) else {
                    continue;
                };
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let c = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                if tx.send((i, kind, c)).is_err() {
                    return;
                }
            }
        })
        .expect("spawn art thread");
    rx
}

// ---------------------------------------------------------------- accessibility

/// What a painted control or line of text is, for assistive technology and
/// the headless tests.
struct Semantics<'a> {
    role: Role,
    label: &'a str,
    description: Option<&'a str>,
    /// A text field's text.
    value: Option<&'a str>,
    disabled: bool,
    /// A tile or tab that is the current one.
    selected: Option<bool>,
    /// A level (a meter's percent).
    number: Option<f64>,
}

impl<'a> Semantics<'a> {
    fn new(role: Role, label: &'a str) -> Self {
        Self {
            role,
            label,
            description: None,
            value: None,
            disabled: false,
            selected: None,
            number: None,
        }
    }
    fn described(mut self, d: &'a str) -> Self {
        self.description = Some(d);
        self
    }
    fn disabled(mut self, off: bool) -> Self {
        self.disabled = off;
        self
    }
    fn selected(mut self, on: bool) -> Self {
        self.selected = Some(on);
        self
    }
    fn valued(mut self, v: &'a str) -> Self {
        self.value = Some(v);
        self
    }
    fn level(mut self, n: f64) -> Self {
        self.number = Some(n);
        self
    }
}

/// Make `r` a widget with `id` and describe it. Text is a `Role::Label`
/// (AccessKit keeps its words in the value).
fn semantic(ui: &egui::Ui, id: Id, r: Rect, sense: Sense, s: Semantics) -> egui::Response {
    let resp = ui.interact(r, id, sense);
    ui.ctx().accesskit_node_builder(resp.id, |b| {
        b.set_role(s.role);
        if s.role == Role::Label {
            b.set_value(s.label);
        } else {
            b.set_label(s.label);
            if let Some(v) = s.value {
                b.set_value(v);
            }
        }
        if let Some(d) = s.description {
            b.set_description(d);
        }
        if let Some(n) = s.number {
            b.set_numeric_value(n);
        }
        if s.disabled {
            b.set_disabled();
        }
        if let Some(on) = s.selected {
            b.set_toggled(if on { Toggled::True } else { Toggled::False });
        }
        if sense.senses_click() && !s.disabled {
            b.add_action(accesskit::Action::Click);
        }
    });
    resp
}

/// A stable id for a part of the window.
fn id_of(_ui: &egui::Ui, what: impl std::hash::Hash + std::fmt::Debug) -> Id {
    Id::unique(("sega", what))
}

/// Name a painted line of text.
fn say(ui: &egui::Ui, key: &str, r: Rect, words: &str) {
    if !words.is_empty() {
        semantic(
            ui,
            id_of(ui, ("say", key)),
            r,
            Sense::hover(),
            Semantics::new(Role::Label, words),
        );
    }
}

// ---------------------------------------------------------------- frame

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame(ui);
    }
}

impl App {
    /// One frame of the window (also what the headless tests drive).
    fn frame(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.screen.is_none() {
            self.screen = ctx.input(|i| {
                let px = i.viewport().native_pixels_per_point.unwrap_or(1.0);
                i.viewport()
                    .monitor_size
                    .map(|m| ((m.x * px).round() as u32, (m.y * px).round() as u32))
            });
        }
        // laid out for 720 lines, zoomed to the window
        let native = ctx.native_pixels_per_point().unwrap_or(1.0);
        let pixels = ctx.input(|i| i.viewport_rect().height() * i.pixels_per_point);
        let want = (pixels / native / theme::DESIGN_HEIGHT).clamp(0.75, 4.0);
        if (want - ctx.zoom_factor()).abs() > 0.001 {
            ctx.set_zoom_factor(want);
        }

        if let Some(input) = &self.input {
            self.snap = input.snapshot();
        }
        self.poll_scan();
        self.poll_art(&ctx);
        self.poll_running(&ctx);
        if self.model.page == Page::Settings {
            self.ensure_form();
        }

        // typed text first, so Enter in the same frame submits all of it
        if self.model.page == Page::FirstRun && self.message.is_none() && self.running.is_none() {
            let events = ctx.input(|i| i.events.clone());
            match self.model.focus {
                Focus::Field if edit_text(&mut self.folder, &events) => self.folder_note = None,
                Focus::BiosField if edit_text(&mut self.bios, &events) => self.bios_note = None,
                _ => {}
            }
        }
        for input in self.gather(&ctx) {
            self.handle(input, &ctx);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F11)) {
            let full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!full));
        }

        let r = ui.max_rect();
        let p = ui.painter().clone();
        p.rect_filled(r, CornerRadius::ZERO, BASE);
        self.draw_hero(&p, r, &ctx);

        let mut clicks: Vec<Click> = Vec::new();
        match self.model.page {
            Page::Hub => self.draw_hub(ui, &p, r, &mut clicks),
            Page::Settings => self.draw_settings(ui, &p, r, &mut clicks),
            Page::Controls => self.draw_controls(ui, &p, r),
            Page::FirstRun => self.draw_first_run(ui, &p, r, &mut clicks),
        }
        if self.model.page != Page::FirstRun {
            self.draw_top_bar(ui, &p, r, &mut clicks);
        } else {
            draw_brand_bar(&p, r);
        }
        self.draw_footer(&p, r);
        if self.running.is_some() {
            self.draw_running(ui, &p, r, &mut clicks);
        }
        if self.message.is_some() {
            self.draw_message(ui, &p, r, &mut clicks);
        }

        for c in clicks {
            self.click(c, &ctx);
        }
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

/// A mouse click, turned into the same model inputs the keyboard sends.
enum Click {
    Focus(Focus, Option<Input>),
    Tile(usize, bool),
    Dismiss,
    StopGame,
}

impl App {
    fn click(&mut self, c: Click, ctx: &egui::Context) {
        if let Click::StopGame = c {
            if let Some(run) = &self.running {
                run.stop.store(true, Ordering::SeqCst);
            }
            return;
        }
        // while a game runs or a message is up, only its own buttons work
        if self.running.is_some() || (self.message.is_some() && !matches!(c, Click::Dismiss)) {
            return;
        }
        match c {
            Click::Dismiss => self.message = None,
            Click::StopGame => {}
            Click::Focus(f, input) => {
                self.model.focus = f;
                if let Some(i) = input {
                    self.handle(i, ctx);
                }
            }
            Click::Tile(i, double) => {
                let before = self.model.selected;
                self.model.select(i);
                self.model.focus = Focus::Tile;
                if before != i {
                    self.selected_changed(before, ctx);
                }
                if double {
                    self.handle(Input::Press, ctx);
                }
            }
        }
    }
}

// ---------------------------------------------------------------- drawing helpers

fn text(
    p: &egui::Painter,
    pos: Pos2,
    align: Align2,
    s: &str,
    font: FontId,
    color: Color32,
) -> Rect {
    p.text(pos, align, s, font, color)
}

/// Text wrapped to `width`, top-left at `pos`; returns its rect.
fn wrapped(
    p: &egui::Painter,
    pos: Pos2,
    s: &str,
    font: FontId,
    color: Color32,
    width: f32,
) -> Rect {
    let g = p.layout(s.to_string(), font, color, width);
    let r = Rect::from_min_size(pos, g.size());
    p.galley(pos, g, color);
    r
}

fn text_width(p: &egui::Painter, s: &str, font: FontId) -> f32 {
    p.layout_no_wrap(s.to_string(), font, INK).size().x
}

fn gradient(p: &egui::Painter, r: Rect, [tl, tr, bl, br]: [Color32; 4]) {
    let mut m = Mesh::default();
    m.colored_vertex(r.left_top(), tl);
    m.colored_vertex(r.right_top(), tr);
    m.colored_vertex(r.left_bottom(), bl);
    m.colored_vertex(r.right_bottom(), br);
    m.add_triangle(0, 1, 2);
    m.add_triangle(1, 3, 2);
    p.add(Shape::mesh(m));
}

fn focus_ring(p: &egui::Painter, r: Rect, radius: u8) {
    p.rect_stroke(
        r.expand(4.0),
        CornerRadius::same(radius.saturating_add(4)),
        Stroke::new(3.0, LAMP),
        StrokeKind::Outside,
    );
}

/// The part of a picture that covers `r`, centred.
fn cover_uv(img: Vec2, r: Rect) -> Rect {
    let ia = img.x / img.y.max(1.0);
    let ra = r.width() / r.height().max(1.0);
    if ra > ia {
        let h = ia / ra;
        Rect::from_min_max(pos2(0.0, (1.0 - h) / 2.0), pos2(1.0, (1.0 + h) / 2.0))
    } else {
        let w = ra / ia;
        Rect::from_min_max(pos2((1.0 - w) / 2.0, 0.0), pos2((1.0 + w) / 2.0, 1.0))
    }
}

fn picture(p: &egui::Painter, tex: &TextureHandle, r: Rect, tint: Color32) {
    p.image(tex.id(), r, cover_uv(tex.size_vec2(), r), tint);
}

/// A route shield, as on Japanese road signs: blue, white rim, flat top, a
/// point at the bottom. Returns its rect.
fn shield(p: &egui::Painter, left_bottom: Pos2, label: &str, font_size: f32) -> Rect {
    let font = theme::display_semi(font_size);
    let w = text_width(p, label, font.clone()).max(font_size * 0.8) + font_size * 1.0;
    let h = font_size * 1.75;
    let r = Rect::from_min_size(pos2(left_bottom.x, left_bottom.y - h), vec2(w, h));
    let point = h * 0.22;
    let pts = |r: Rect| {
        vec![
            r.left_top(),
            r.right_top(),
            pos2(r.right(), r.bottom() - point),
            pos2(r.center().x, r.bottom()),
            pos2(r.left(), r.bottom() - point),
        ]
    };
    p.add(Shape::convex_polygon(pts(r), INK, Stroke::NONE));
    p.add(Shape::convex_polygon(
        pts(r.shrink(2.0)),
        SIGN,
        Stroke::NONE,
    ));
    text(
        p,
        pos2(r.center().x, r.top() + (h - point) / 2.0 + 1.0),
        Align2::CENTER_CENTER,
        label,
        font,
        INK,
    );
    r
}

const ARROWS: &str = "◀ ▶";

/// A key cap and what it does, for the footer; returns the next x.
fn hint(p: &egui::Painter, x: f32, cy: f32, key: &str, what: &str) -> f32 {
    let kf = theme::semibold(size::NOTE);
    // "◀ ▶" is drawn: the fonts have no such glyphs
    let arrows = key == ARROWS;
    let kw = if arrows {
        44.0
    } else {
        text_width(p, key, kf.clone()) + 14.0
    };
    let kr = Rect::from_center_size(pos2(x + kw / 2.0, cy), vec2(kw, 22.0));
    p.rect_stroke(
        kr,
        CornerRadius::same(4),
        Stroke::new(1.5, LINE),
        StrokeKind::Inside,
    );
    if arrows {
        let half = |c: Pos2, d: f32| {
            let s = 4.5;
            vec![
                pos2(c.x + d * s, c.y),
                pos2(c.x - d * s * 0.6, c.y - s),
                pos2(c.x - d * s * 0.6, c.y + s),
            ]
        };
        let c = kr.center();
        p.add(Shape::convex_polygon(
            half(pos2(c.x - 9.0, c.y), -1.0),
            INK,
            Stroke::NONE,
        ));
        p.add(Shape::convex_polygon(
            half(pos2(c.x + 9.0, c.y), 1.0),
            INK,
            Stroke::NONE,
        ));
    } else {
        text(p, kr.center(), Align2::CENTER_CENTER, key, kf, INK);
    }
    let r = text(
        p,
        pos2(kr.right() + 8.0, cy),
        Align2::LEFT_CENTER,
        what,
        theme::body(size::NOTE + 1.0),
        INK,
    );
    r.right() + 26.0
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ButtonStyle {
    Primary,
    Outline,
}

/// A button; returns its response.
#[allow(clippy::too_many_arguments)]
fn button(
    ui: &egui::Ui,
    p: &egui::Painter,
    r: Rect,
    id: &str,
    label: &str,
    style: ButtonStyle,
    enabled: bool,
    focused: bool,
    why_not: Option<&str>,
) -> egui::Response {
    let mut sem = Semantics::new(Role::Button, label).disabled(!enabled);
    if let Some(why) = why_not.filter(|_| !enabled) {
        sem = sem.described(why);
    }
    let resp = semantic(ui, id_of(ui, ("button", id)), r, Sense::click(), sem);
    let hovered = enabled && resp.hovered();
    let radius = CornerRadius::same(6);
    let font = theme::display_semi(size::BUTTON);
    match (style, enabled) {
        (ButtonStyle::Primary, true) => {
            let fill = if hovered { theme::LAMP_HOVER } else { LAMP };
            p.rect_filled(r, radius, fill);
            text(p, r.center(), Align2::CENTER_CENTER, label, font, BASE);
        }
        (ButtonStyle::Outline, true) => {
            p.rect_filled(r, radius, alpha(BASE, 200));
            let stroke = if hovered { INK } else { LINE };
            p.rect_stroke(r, radius, Stroke::new(2.0, stroke), StrokeKind::Inside);
            text(p, r.center(), Align2::CENTER_CENTER, label, font, INK);
        }
        (_, false) => {
            p.rect_stroke(r, radius, Stroke::new(2.0, DISABLED), StrokeKind::Inside);
            text(p, r.center(), Align2::CENTER_CENTER, label, font, DISABLED);
        }
    }
    if focused {
        focus_ring(p, r, 6);
    }
    if enabled {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    }
}

fn card(p: &egui::Painter, r: Rect) {
    p.rect_filled(r, CornerRadius::same(8), alpha(PANEL, 245));
    p.rect_stroke(
        r,
        CornerRadius::same(8),
        Stroke::new(1.0, alpha(LINE, 90)),
        StrokeKind::Inside,
    );
}

/// A card's heading on a sign-blue plate, as a road direction sign.
fn card_heading(p: &egui::Painter, r: Rect, title: &str) -> f32 {
    let h = 40.0;
    let plate = Rect::from_min_size(r.min, vec2(r.width(), h));
    p.rect_filled(
        plate,
        CornerRadius {
            nw: 8,
            ne: 8,
            sw: 0,
            se: 0,
        },
        SIGN,
    );
    text(
        p,
        pos2(plate.left() + 20.0, plate.center().y),
        Align2::LEFT_CENTER,
        title,
        theme::semibold(size::LABEL),
        INK,
    );
    plate.bottom()
}

fn chip(p: &egui::Painter, pos: Pos2, label: &str, fill: Option<Color32>, color: Color32) -> Rect {
    let font = theme::semibold(size::BODY);
    let w = text_width(p, label, font.clone()) + 24.0;
    let r = Rect::from_min_size(pos, vec2(w, 28.0));
    match fill {
        Some(f) => {
            p.rect_filled(r, CornerRadius::same(4), f);
        }
        None => {
            p.rect_stroke(
                r,
                CornerRadius::same(4),
                Stroke::new(1.5, LINE),
                StrokeKind::Inside,
            );
        }
    }
    text(p, r.center(), Align2::CENTER_CENTER, label, font, color);
    r
}

/// A small padlock, drawn (the tile badge's icon).
fn padlock(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let body = Rect::from_center_size(pos2(c.x, c.y + s * 0.2), vec2(s, s * 0.7));
    p.rect_filled(body, CornerRadius::same(2), color);
    let r = s * 0.3;
    let top = pos2(c.x, body.top());
    let pts: Vec<Pos2> = (0..=12)
        .map(|k| {
            let a = std::f32::consts::PI * (k as f32 / 12.0);
            pos2(top.x - r * a.cos(), top.y - r * a.sin() * 1.1)
        })
        .collect();
    p.add(Shape::line(pts, Stroke::new(s * 0.14, color)));
}

fn draw_brand_bar(p: &egui::Painter, r: Rect) -> Rect {
    let bar = Rect::from_min_size(r.min, vec2(r.width(), BAR_H));
    p.rect_filled(bar, CornerRadius::ZERO, SIGN);
    p.rect_filled(
        Rect::from_min_size(pos2(bar.left(), bar.bottom() - 2.0), vec2(bar.width(), 2.0)),
        CornerRadius::ZERO,
        alpha(INK, 70),
    );
    let s = shield(
        p,
        pos2(bar.left() + MARGIN, bar.center().y + 14.0),
        "SR",
        size::SHIELD,
    );
    text(
        p,
        pos2(s.right() + 12.0, bar.center().y),
        Align2::LEFT_CENTER,
        "Sega Racing Launcher",
        theme::display_semi(size::BRAND),
        INK,
    );
    bar
}

// ---------------------------------------------------------------- pages

impl App {
    fn draw_hero(&mut self, p: &egui::Painter, r: Rect, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let sel = self.model.selected;
        let t = match self.fade {
            Some((_, t0)) => ((now - t0) / 0.3).clamp(0.0, 1.0) as f32,
            None => 1.0,
        };
        if t >= 1.0 {
            self.fade = None;
        }
        if let Some((from, _)) = self.fade
            && let Some(tex) = &self.heroes[from]
        {
            picture(p, tex, r, Color32::WHITE);
        }
        if let Some(tex) = &self.heroes[sel] {
            picture(p, tex, r, Color32::WHITE.gamma_multiply(t));
        }
        let w = r.width();
        match self.model.page {
            Page::Hub => {
                // a solid backing behind the text column, fading to the picture
                let solid = alpha(BASE, theme::SCRIM);
                let clear = alpha(BASE, 0);
                let col = Rect::from_min_max(r.min, pos2(r.left() + w * 0.40, r.bottom()));
                p.rect_filled(col, CornerRadius::ZERO, solid);
                gradient(
                    p,
                    Rect::from_min_max(
                        pos2(col.right(), r.top()),
                        pos2(r.left() + w * 0.78, r.bottom()),
                    ),
                    [solid, clear, solid, clear],
                );
                let fade_top = r.top() + 470.0;
                let full = r.top() + 520.0;
                gradient(
                    p,
                    Rect::from_min_max(pos2(r.left(), fade_top), pos2(r.right(), full)),
                    [clear, clear, solid, solid],
                );
                p.rect_filled(
                    Rect::from_min_max(pos2(r.left(), full), r.max),
                    CornerRadius::ZERO,
                    solid,
                );
            }
            _ => {
                p.rect_filled(r, CornerRadius::ZERO, alpha(BASE, 225));
            }
        }
        if self.running.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn draw_top_bar(&mut self, ui: &egui::Ui, p: &egui::Painter, r: Rect, clicks: &mut Vec<Click>) {
        let bar = draw_brand_bar(p, r);
        let current = match self.model.page {
            Page::Controls => Tab::Controls,
            Page::Settings => Tab::Settings,
            _ => Tab::Games,
        };
        let mut x = bar.left() + 330.0;
        for tab in Tab::ALL {
            let font = theme::semibold(size::NAV);
            let tw = text_width(p, tab.label(), font.clone());
            let tr =
                Rect::from_center_size(pos2(x + tw / 2.0, bar.center().y), vec2(tw + 24.0, 38.0));
            let resp = semantic(
                ui,
                id_of(ui, ("tab", tab.label())),
                tr,
                Sense::click(),
                Semantics::new(Role::Tab, tab.label()).selected(tab == current),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
            if resp.hovered() {
                p.rect_filled(tr, CornerRadius::same(4), alpha(BASE, 60));
            }
            text(
                p,
                tr.center(),
                Align2::CENTER_CENTER,
                tab.label(),
                font,
                INK,
            );
            if tab == current {
                p.rect_filled(
                    Rect::from_min_size(
                        pos2(tr.left() + 8.0, bar.bottom() - 6.0),
                        vec2(tr.width() - 16.0, 4.0),
                    ),
                    CornerRadius::same(1),
                    LAMP,
                );
            }
            if self.model.focus == Focus::Tab(tab) {
                p.rect_stroke(
                    tr,
                    CornerRadius::same(4),
                    Stroke::new(3.0, LAMP),
                    StrokeKind::Outside,
                );
            }
            if resp.clicked() {
                clicks.push(Click::Focus(Focus::Tab(tab), Some(Input::Press)));
            }
            x = tr.right() + 20.0;
        }
        // the controller that drives the car
        let (label, kind) = match (&self.snap.error, self.snap.driver) {
            (Some(_), _) => ("Controllers unavailable".to_string(), None),
            (None, Some(d)) => self
                .snap
                .devices
                .get(d)
                .map(|d| (d.name.clone(), Some(d.kind)))
                .unwrap_or(("No controller".into(), None)),
            (None, None) => ("No controller".into(), None),
        };
        let font = theme::body(size::BODY);
        let maxw = 300.0;
        let shown = ellipsize(p, &label, font.clone(), maxw);
        let tr = text(
            p,
            pos2(bar.right() - MARGIN, bar.center().y),
            Align2::RIGHT_CENTER,
            &shown,
            font,
            INK,
        );
        say(ui, "driver", tr, &format!("Controller: {label}"));
        draw_device_glyph(p, pos2(tr.left() - 20.0, bar.center().y), kind);
    }

    fn draw_hub(&mut self, ui: &egui::Ui, p: &egui::Painter, r: Rect, clicks: &mut Vec<Click>) {
        let sel = self.model.selected;
        let g = &GAMES[sel];
        let state = self.state(sel);
        let x = r.left() + MARGIN;
        let colw = (r.width() * 0.40 - MARGIN).max(360.0);

        // title, anchored at its bottom
        let title_g = p.layout(
            g.title.to_string(),
            theme::display(size::TITLE),
            INK,
            colw + 80.0,
        );
        let th = title_g.size().y;
        let title_bottom = r.top() + 330.0;
        let title_at = pos2(x, title_bottom - th);
        say(
            ui,
            "title",
            Rect::from_min_size(title_at, title_g.size()),
            g.title,
        );
        p.galley(title_at, title_g, INK);

        let edition = match self.entry(sel).map(|e| &e.found) {
            Some(Found::Known { edition, .. }) => Some(edition.label),
            _ => None,
        };
        let mut line2 = format!("{} · {}", g.board, g.year);
        if let Some(e) = edition {
            line2.push_str(&format!(" · {e}"));
        }
        let meta = text(
            p,
            pos2(x + 2.0, title_bottom + 6.0),
            Align2::LEFT_TOP,
            &line2,
            theme::body(size::LABEL),
            MUTED,
        );
        say(ui, "meta", meta, &line2);

        let chip_y = r.top() + 372.0;
        let (fill, color) = match state {
            TileState::Ready => (Some(SIGN), INK),
            TileState::Unverified(_) => (Some(SIGN), INK),
            TileState::Blocked(_) => (Some(SIGNAL), INK),
            TileState::NotInLibrary => (None, INK),
            TileState::Looking => (None, MUTED),
        };
        let cr = chip(p, pos2(x, chip_y), state.chip(), fill, color);
        say(ui, "state", cr, state.chip());
        let folder = self.entry(sel).map(|e| e.dir.display().to_string());
        let reason = match state {
            TileState::Blocked(why) => why.to_string(),
            TileState::Unverified(note) => note.to_string(),
            TileState::NotInLibrary => {
                "Not found in your game folders. Add its folder in Settings, then choose Rescan."
                    .into()
            }
            TileState::Ready | TileState::Looking => String::new(),
        };
        let mut y = chip_y + 38.0;
        if !reason.is_empty() {
            let rr = wrapped(
                p,
                pos2(x, y),
                &reason,
                theme::body(size::BODY),
                INK,
                colw + 40.0,
            );
            say(ui, "reason", rr, &reason);
            y = rr.bottom() + 4.0;
        }
        if let Some(f) = &folder {
            let line = format!("Folder: {f}");
            let shown = ellipsize(p, &line, theme::body(size::BODY), colw + 40.0);
            let fr = text(
                p,
                pos2(x, y),
                Align2::LEFT_TOP,
                &shown,
                theme::body(size::BODY),
                MUTED,
            );
            say(ui, "folder", fr, &line);
            y = fr.bottom();
        }
        if matches!(g.route, Route::Cxbx) && self.config.chihiro_bios.is_none() && state.playable()
        {
            let note = "Needs the Chihiro BIOS folder: add it in Settings.";
            let nr = wrapped(
                p,
                pos2(x, y + 4.0),
                note,
                theme::body(size::BODY),
                ERROR,
                colw + 40.0,
            );
            say(ui, "bios-needed", nr, note);
            y = nr.bottom();
        }

        // the buttons follow the text above (a two-line title pushes them down)
        let by = (r.top() + 446.0).max(y + 14.0);
        let play_r = Rect::from_min_size(pos2(x, by), vec2(170.0, 50.0));
        let set_r = Rect::from_min_size(pos2(play_r.right() + 16.0, by), vec2(170.0, 50.0));
        let playable = state.playable() && self.running.is_none();
        let why_not = match state {
            TileState::Blocked(why) => Some(why),
            TileState::NotInLibrary => Some("Not found in your game folders."),
            TileState::Looking => Some("Still looking for your games."),
            _ => None,
        };
        if button(
            ui,
            p,
            play_r,
            "play",
            "Play",
            ButtonStyle::Primary,
            playable,
            self.model.focus == Focus::Play,
            why_not,
        )
        .clicked()
            && playable
        {
            clicks.push(Click::Focus(Focus::Play, Some(Input::Press)));
        }
        if button(
            ui,
            p,
            set_r,
            "game-settings",
            "Settings",
            ButtonStyle::Outline,
            true,
            self.model.focus == Focus::GameSettings,
            None,
        )
        .clicked()
        {
            clicks.push(Click::Focus(Focus::GameSettings, Some(Input::Press)));
        }

        // the tile row
        let found = self.found_count();
        let row_label = match &self.entries {
            None => "Your games · looking…".to_string(),
            Some(_) => format!("Your games · {found} of {} found", GAMES.len()),
        };
        let rl = text(
            p,
            pos2(x, r.top() + 528.0),
            Align2::LEFT_BOTTOM,
            &row_label,
            theme::semibold(size::BODY),
            MUTED,
        );
        say(ui, "found", rl, &row_label);
        let gap = 10.0;
        let n = GAMES.len() as f32;
        let tw = (r.width() - 2.0 * MARGIN - gap * (n - 1.0)) / n;
        let th = tw * 9.0 / 16.0;
        let ty = r.top() + 540.0;
        for (i, game) in GAMES.iter().enumerate() {
            let tr = Rect::from_min_size(pos2(x + i as f32 * (tw + gap), ty), vec2(tw, th));
            let st = self.state(i);
            let dim = matches!(st, TileState::Blocked(_) | TileState::NotInLibrary);
            let about = format!("{}, {}, {}", game.year, game.board, st.chip());
            let resp = semantic(
                ui,
                id_of(ui, ("tile", i)),
                tr,
                Sense::click(),
                Semantics::new(Role::RadioButton, game.title)
                    .described(&about)
                    .selected(i == sel),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
            let radius = CornerRadius::same(6);
            p.rect_filled(tr, radius, PANEL);
            if let Some(tex) = &self.tiles[i] {
                let tint = if dim {
                    Color32::from_gray(95)
                } else if resp.hovered() || i == sel {
                    Color32::WHITE
                } else {
                    Color32::from_gray(200)
                };
                p.image(tex.id(), tr, cover_uv(tex.size_vec2(), tr), tint);
            }
            p.rect_stroke(
                tr,
                radius,
                Stroke::new(1.0, alpha(LINE, 120)),
                StrokeKind::Inside,
            );
            shield(
                p,
                pos2(tr.left() + 6.0, tr.bottom() - 6.0),
                game.short,
                size::SHIELD,
            );
            if let Some(badge) = st.badge() {
                let bf = theme::semibold(size::BADGE);
                let bw = text_width(p, badge, bf.clone()) + 26.0;
                let br = Rect::from_min_size(
                    pos2(tr.right() - bw - 5.0, tr.top() + 5.0),
                    vec2(bw, 20.0),
                );
                p.rect_filled(br, CornerRadius::same(3), alpha(BASE, 235));
                let c = pos2(br.left() + 10.0, br.center().y);
                match st {
                    TileState::Blocked(_) => padlock(p, pos2(c.x, c.y - 1.0), 8.0, INK),
                    TileState::Unverified(_) => {
                        p.circle_stroke(c, 4.5, Stroke::new(1.5, INK));
                        text(p, c, Align2::CENTER_CENTER, "?", theme::semibold(8.0), INK);
                    }
                    _ => {
                        p.circle_stroke(c, 4.0, Stroke::new(1.5, INK));
                        p.line_segment(
                            [pos2(c.x - 3.0, c.y + 3.0), pos2(c.x + 3.0, c.y - 3.0)],
                            Stroke::new(1.5, INK),
                        );
                    }
                }
                text(
                    p,
                    pos2(br.left() + 19.0, br.center().y),
                    Align2::LEFT_CENTER,
                    badge,
                    bf,
                    INK,
                );
            }
            if i == sel {
                p.rect_filled(
                    Rect::from_min_size(
                        pos2(tr.left() + tw * 0.3, tr.bottom() + 10.0),
                        vec2(tw * 0.4, 4.0),
                    ),
                    CornerRadius::same(2),
                    LAMP,
                );
                if self.model.focus == Focus::Tile {
                    focus_ring(p, tr, 6);
                }
            }
            if resp.clicked() || resp.double_clicked() {
                clicks.push(Click::Tile(i, resp.double_clicked()));
            }
        }
    }

    fn draw_page_heading(&self, ui: &egui::Ui, p: &egui::Painter, r: Rect, title: &str, sub: &str) {
        let x = r.left() + MARGIN;
        let t = text(
            p,
            pos2(x, r.top() + BAR_H + 22.0),
            Align2::LEFT_TOP,
            title,
            theme::display(size::PAGE_TITLE),
            INK,
        );
        say(ui, "heading", t, title);
        let s = text(
            p,
            pos2(x, r.top() + BAR_H + 66.0),
            Align2::LEFT_TOP,
            sub,
            theme::body(size::BODY),
            MUTED,
        );
        say(ui, "sub-heading", s, sub);
    }

    fn draw_settings(
        &mut self,
        ui: &egui::Ui,
        p: &egui::Painter,
        r: Rect,
        clicks: &mut Vec<Click>,
    ) {
        let sel = self.model.selected;
        let g = &GAMES[sel];
        self.draw_page_heading(
            ui,
            p,
            r,
            &format!("{} settings", g.title),
            "Changes are saved as you make them and used the next time you press Play.",
        );
        let Some((_, form)) = &self.form else {
            return;
        };
        let top = r.top() + 162.0;
        let right_w = 340.0;
        let left = Rect::from_min_max(
            pos2(r.left() + MARGIN, top),
            pos2(
                r.right() - MARGIN - right_w - 24.0,
                top + 40.0 + Row::shown(Host::current()).len() as f32 * ROW_H + 8.0,
            ),
        );
        card(p, left);
        let mut y = card_heading(p, left, "Picture and controls") + 4.0;
        for (idx, item) in self.model.items.iter().enumerate() {
            let Item::Option(row) = *item else { continue };
            let rr =
                Rect::from_min_size(pos2(left.left() + 4.0, y), vec2(left.width() - 8.0, ROW_H));
            y += ROW_H;
            let focused = self.model.focus == Focus::Item(idx);
            let off = form.disabled(row);
            let value = form.value(row);
            let resp = semantic(
                ui,
                id_of(ui, ("row", idx)),
                rr,
                Sense::click(),
                Semantics::new(Role::Button, row.label())
                    .valued(&value)
                    .described(off.unwrap_or(row.description()))
                    .disabled(off.is_some()),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
            if focused {
                p.rect_filled(
                    rr,
                    CornerRadius::same(4),
                    Color32::from_rgb(0x1F, 0x2C, 0x3B),
                );
                p.rect_stroke(
                    rr,
                    CornerRadius::same(4),
                    Stroke::new(3.0, LAMP),
                    StrokeKind::Inside,
                );
            } else if resp.hovered() {
                p.rect_filled(rr, CornerRadius::same(4), alpha(INK, 10));
            }
            let lx = rr.left() + 18.0;
            let label_color = if off.is_some() { MUTED } else { INK };
            text(
                p,
                pos2(lx, rr.top() + 6.0),
                Align2::LEFT_TOP,
                row.label(),
                theme::semibold(size::LABEL),
                label_color,
            );
            let desc = off.unwrap_or(row.description());
            let valw = 270.0;
            ellipsized_text(
                p,
                pos2(lx, rr.top() + 27.0),
                desc,
                theme::body(size::NOTE),
                MUTED,
                rr.width() - valw - 40.0,
            );
            // < value >
            let vr = Rect::from_min_max(
                pos2(rr.right() - valw - 12.0, rr.top()),
                pos2(rr.right() - 12.0, rr.bottom()),
            );
            let vc = if off.is_some() { DISABLED } else { INK };
            text(
                p,
                vr.center(),
                Align2::CENTER_CENTER,
                &value,
                theme::semibold(size::LABEL),
                vc,
            );
            if off.is_none() {
                let ac = if focused { LAMP } else { MUTED };
                let lr =
                    Rect::from_center_size(pos2(vr.left() + 12.0, vr.center().y), vec2(28.0, 40.0));
                let rr2 = Rect::from_center_size(
                    pos2(vr.right() - 12.0, vr.center().y),
                    vec2(28.0, 40.0),
                );
                arrow(p, lr.center(), -1.0, ac);
                arrow(p, rr2.center(), 1.0, ac);
                let l = semantic(
                    ui,
                    id_of(ui, ("left", idx)),
                    lr,
                    Sense::click(),
                    Semantics::new(Role::Button, &format!("{}: previous", row.label())),
                );
                let rt = semantic(
                    ui,
                    id_of(ui, ("right", idx)),
                    rr2,
                    Sense::click(),
                    Semantics::new(Role::Button, &format!("{}: next", row.label())),
                );
                if l.clicked() {
                    clicks.push(Click::Focus(Focus::Item(idx), Some(Input::Left)));
                    continue;
                }
                if rt.clicked() {
                    clicks.push(Click::Focus(Focus::Item(idx), Some(Input::Right)));
                    continue;
                }
            } else {
                // "off" shown by shape too: a lock beside the value
                padlock(
                    p,
                    pos2(vr.right() - 12.0, vr.center().y - 1.0),
                    9.0,
                    DISABLED,
                );
            }
            if resp.clicked() {
                let input = if off.is_none() {
                    Some(Input::Press)
                } else {
                    None
                };
                clicks.push(Click::Focus(Focus::Item(idx), input));
            }
        }

        // the library card
        let rc = Rect::from_min_max(
            pos2(left.right() + 24.0, top),
            pos2(r.right() - MARGIN, left.bottom()),
        );
        card(p, rc);
        let mut y = card_heading(p, rc, "Your game folders") + 16.0;
        let ix = rc.left() + 20.0;
        let iw = rc.width() - 40.0;
        let folders: Vec<String> = self
            .config
            .libraries
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        if folders.is_empty() {
            let fr = wrapped(
                p,
                pos2(ix, y),
                "Not chosen yet",
                theme::body(size::BODY),
                INK,
                iw,
            );
            say(ui, "folders", fr, "Not chosen yet");
            y = fr.bottom() + 8.0;
        }
        for (k, f) in folders.iter().enumerate().take(3) {
            let shown = ellipsize(p, f, theme::body(size::BODY), iw);
            let fr = text(
                p,
                pos2(ix, y),
                Align2::LEFT_TOP,
                &shown,
                theme::body(size::BODY),
                INK,
            );
            say(ui, &format!("folder-{k}"), fr, f);
            y += 24.0;
        }
        if folders.len() > 3 {
            let more = format!("and {} more", folders.len() - 3);
            let mr = text(
                p,
                pos2(ix, y),
                Align2::LEFT_TOP,
                &more,
                theme::body(size::BODY),
                MUTED,
            );
            say(ui, "folders-more", mr, &more);
            y += 24.0;
        }
        y += 8.0;
        let status = match &self.entries {
            None => "Looking for your games…".to_string(),
            Some(e) => {
                let unknown = e.iter().filter(|e| e.game_id().is_none()).count();
                let mut s = format!("{} of {} games found.", self.found_count(), GAMES.len());
                if unknown > 0 {
                    s.push_str(&format!(
                        " {unknown} folder{} holds a build this launcher doesn't know; launcher.log has its digest.",
                        if unknown == 1 { "" } else { "s" }
                    ));
                }
                s
            }
        };
        let sr = wrapped(p, pos2(ix, y), &status, theme::body(size::BODY), MUTED, iw);
        say(ui, "library-status", sr, &status);
        y = sr.bottom() + 18.0;
        let n = self.model.items.len();
        let b1 = Rect::from_min_size(pos2(ix, y), vec2(iw, 46.0));
        let b2 = Rect::from_min_size(pos2(ix, y + 60.0), vec2(iw, 46.0));
        let scanning = self.scan.is_some();
        if button(
            ui,
            p,
            b1,
            "rescan",
            if scanning { "Looking…" } else { "Rescan" },
            ButtonStyle::Outline,
            !scanning,
            self.model.focus == Focus::Item(n - 2),
            Some("Already looking."),
        )
        .clicked()
            && !scanning
        {
            clicks.push(Click::Focus(Focus::Item(n - 2), Some(Input::Press)));
        }
        if button(
            ui,
            p,
            b2,
            "folder",
            "Folders and BIOS",
            ButtonStyle::Outline,
            true,
            self.model.focus == Focus::Item(n - 1),
            None,
        )
        .clicked()
        {
            clicks.push(Click::Focus(Focus::Item(n - 1), Some(Input::Press)));
        }
        if let Some((note, ok)) = &self.note {
            let at = pos2(ix, b2.bottom() + 22.0);
            let color = if *ok { INK } else { ERROR };
            let nr = wrapped(p, at, note, theme::body(size::BODY), color, iw);
            say(ui, "note", nr, note);
        }
    }

    fn draw_controls(&self, ui: &egui::Ui, p: &egui::Painter, r: Rect) {
        self.draw_page_heading(
            ui,
            p,
            r,
            "Controls",
            "Turn the wheel or move the stick, press the pedals and buttons: the bars should follow.",
        );
        let top = r.top() + 162.0;
        let bottom = r.bottom() - 62.0;
        let split = r.left() + MARGIN + (r.width() - 2.0 * MARGIN - 24.0) * 0.46;

        // devices
        let dev = Rect::from_min_max(pos2(r.left() + MARGIN, top), pos2(split, top + 196.0));
        card(p, dev);
        let mut y = card_heading(p, dev, "Your controllers") + 14.0;
        let ix = dev.left() + 20.0;
        let iw = dev.width() - 40.0;
        if let Some(e) = &self.snap.error {
            let msg = format!(
                "Controllers can't be read: {e}. Plug the controller in again, then restart the launcher."
            );
            let mr = wrapped(p, pos2(ix, y), &msg, theme::body(size::BODY), ERROR, iw);
            say(ui, "devices-error", mr, &msg);
        } else if self.snap.devices.is_empty() {
            let msg = "No controller found. Plug in a wheel or a pad; it shows up here.";
            let mr = wrapped(p, pos2(ix, y), msg, theme::body(size::BODY), INK, iw);
            say(ui, "devices-none", mr, msg);
        }
        for (i, d) in self.snap.devices.iter().enumerate().take(4) {
            let kind = match d.kind {
                Kind::Wheel => "Wheel",
                Kind::Pad => "Pad",
                Kind::Other => "Other",
            };
            let k = chip(p, pos2(ix, y), kind, None, INK);
            let mut right = dev.right() - 20.0;
            let drives = self.snap.driver == Some(i);
            if drives {
                let label = "Drives the car";
                let w = text_width(p, label, theme::semibold(size::BODY)) + 24.0;
                chip(p, pos2(right - w, y), label, Some(SIGN), INK);
                right -= w + 10.0;
            }
            let name = ellipsize(
                p,
                &d.name,
                theme::body(size::BODY),
                right - k.right() - 14.0,
            );
            let nr = text(
                p,
                pos2(k.right() + 12.0, k.center().y),
                Align2::LEFT_CENTER,
                &name,
                theme::body(size::BODY),
                INK,
            );
            let words = format!(
                "{kind}: {}{}",
                d.name,
                if drives { " (drives the car)" } else { "" }
            );
            say(ui, &format!("device-{i}"), nr.union(k), &words);
            y += 38.0;
        }

        // default pad layout
        let lay = Rect::from_min_max(
            pos2(r.left() + MARGIN, dev.bottom() + 16.0),
            pos2(split, bottom),
        );
        card(p, lay);
        let mut y = card_heading(p, lay, "Pad layout") + 10.0;
        let map = default_pad_map();
        let mut rows: Vec<(String, String)> = vec![
            ("Steer".into(), "Left stick".into()),
            ("Gas".into(), map.describe(Action::Accelerate)),
            ("Brake".into(), map.describe(Action::Brake)),
        ];
        for b in Button::ALL {
            let keys = map.describe(Action::Press(*b));
            if keys != "-" {
                rows.push((b.label().to_string(), keys));
            }
        }
        let ix = lay.left() + 20.0;
        let colw = (lay.width() - 40.0) / 2.0;
        for (k, (what, keys)) in rows.iter().enumerate() {
            let col = (k / 6) as f32;
            let row = (k % 6) as f32;
            let at = pos2(ix + col * colw, y + row * 28.0);
            let kf = theme::semibold(size::NOTE);
            let kw = text_width(p, keys, kf.clone()) + 14.0;
            let kr = Rect::from_min_size(at, vec2(kw.max(36.0), 22.0));
            p.rect_stroke(
                kr,
                CornerRadius::same(4),
                Stroke::new(1.5, LINE),
                StrokeKind::Inside,
            );
            text(p, kr.center(), Align2::CENTER_CENTER, keys, kf, INK);
            let wr = ellipsized_text(
                p,
                pos2(kr.right() + 10.0, at.y + 2.0),
                what,
                theme::body(size::BODY),
                INK,
                colw - kr.width() - 18.0,
            );
            say(
                ui,
                &format!("layout-{k}"),
                wr.union(kr),
                &format!("{what}: {keys}"),
            );
        }
        y += 6.0 * 28.0 + 8.0;
        wrapped(
            p,
            pos2(ix, y),
            "A wheel steers with its wheel and pedals, and its paddles shift.",
            theme::body(size::NOTE),
            MUTED,
            lay.width() - 40.0,
        );

        // live check
        let live = Rect::from_min_max(pos2(split + 24.0, top), pos2(r.right() - MARGIN, bottom));
        card(p, live);
        let mut y = card_heading(p, live, "Live check") + 18.0;
        let c = &self.snap.controls;
        let c_feeding = self.snap.feeding;
        let ix = live.left() + 20.0;
        let iw = live.width() - 40.0;
        let label_w = 90.0;
        let num_w = 64.0;
        for (name, v, centred) in [
            ("Steering", c.steer, true),
            ("Gas", c.accel, false),
            ("Brake", c.brake, false),
        ] {
            text(
                p,
                pos2(ix, y + 12.0),
                Align2::LEFT_CENTER,
                name,
                theme::semibold(size::LABEL),
                INK,
            );
            let br = Rect::from_min_size(pos2(ix + label_w, y), vec2(iw - label_w - num_w, 24.0));
            bar(p, br, v, centred);
            let pct = format!("{:.0}%", v * 100.0);
            text(
                p,
                pos2(live.right() - 20.0, y + 12.0),
                Align2::RIGHT_CENTER,
                &pct,
                theme::body(size::BODY),
                INK,
            );
            semantic(
                ui,
                id_of(ui, ("meter", name)),
                br,
                Sense::hover(),
                Semantics::new(Role::Meter, name)
                    .valued(&pct)
                    .level(f64::from(v * 100.0).round()),
            );
            y += 44.0;
        }
        y += 8.0;
        text(
            p,
            pos2(ix, y),
            Align2::LEFT_TOP,
            "Buttons",
            theme::semibold(size::LABEL),
            INK,
        );
        y += 30.0;
        let mut x = ix;
        for b in Button::ALL {
            let on = c.is_pressed(*b);
            let label = b.label();
            let w = text_width(p, label, theme::semibold(size::BODY)) + 24.0;
            if x + w > live.right() - 20.0 {
                x = ix;
                y += 38.0;
            }
            let cr = if on {
                chip(p, pos2(x, y), label, Some(LAMP), BASE)
            } else {
                chip(p, pos2(x, y), label, None, MUTED)
            };
            say(
                ui,
                &format!("button-{}", b.key()),
                cr,
                &format!("{label}: {}", if on { "pressed" } else { "released" }),
            );
            x += w + 10.0;
        }
        y += 44.0;
        let nr = wrapped(
            p,
            pos2(ix, y),
            "A pressed button turns solid.",
            theme::body(size::NOTE),
            MUTED,
            iw,
        );
        if c_feeding {
            let words = "A game is running: it reads these controls.";
            let fr = wrapped(
                p,
                pos2(ix, nr.bottom() + 8.0),
                words,
                theme::body(size::NOTE),
                INK,
                iw,
            );
            say(ui, "feeding", fr, words);
        }
    }

    fn draw_first_run(
        &mut self,
        ui: &mut egui::Ui,
        p: &egui::Painter,
        r: Rect,
        clicks: &mut Vec<Click>,
    ) {
        let w = 900.0_f32.min(r.width() - 2.0 * MARGIN - 80.0);
        let x = r.left() + MARGIN + 40.0;
        let mut y = r.top() + BAR_H + 26.0;
        let has_library = !self.config.libraries.is_empty();
        let title = if has_library {
            "Your game folders"
        } else {
            "Choose your game folders"
        };
        let tr = text(
            p,
            pos2(x, y),
            Align2::LEFT_TOP,
            title,
            theme::display(40.0),
            INK,
        );
        say(ui, "heading", tr, title);
        y += 54.0;
        let intro = "Type the path of a folder that holds your Sega game folders and add it. \
             The launcher looks inside for each game and never changes what's there.";
        let ir = wrapped(p, pos2(x, y), intro, theme::body(size::LABEL), INK, w);
        say(ui, "intro", ir, intro);
        y = ir.bottom() + 14.0;

        // the folders so far
        if self.config.libraries.is_empty() {
            let fr = text(
                p,
                pos2(x, y),
                Align2::LEFT_TOP,
                "No folder yet.",
                theme::body(size::BODY),
                MUTED,
            );
            say(ui, "folders", fr, "No folder yet.");
            y += 24.0;
        }
        for (k, f) in self.config.libraries.iter().enumerate().take(3) {
            let line = f.display().to_string();
            let shown = ellipsize(p, &line, theme::body(size::BODY), w);
            let fr = text(
                p,
                pos2(x, y),
                Align2::LEFT_TOP,
                &shown,
                theme::body(size::BODY),
                INK,
            );
            say(ui, &format!("folder-{k}"), fr, &line);
            y += 24.0;
        }
        if self.config.libraries.len() > 3 {
            let more = format!("and {} more", self.config.libraries.len() - 3);
            let mr = text(
                p,
                pos2(x, y),
                Align2::LEFT_TOP,
                &more,
                theme::body(size::BODY),
                MUTED,
            );
            say(ui, "folders-more", mr, &more);
            y += 24.0;
        }
        y += 10.0;

        // add a folder
        let btn_w = 180.0;
        let field_w = w - btn_w - 16.0;
        let fr = Rect::from_min_size(pos2(x, y), vec2(field_w, 46.0));
        text_field(
            ui,
            p,
            fr,
            TextField {
                id: "folder-field",
                label: "Game folder",
                text: &self.folder,
                placeholder: "/path/to/your/games",
                focused: self.model.focus == Focus::Field,
            },
            || clicks.push(Click::Focus(Focus::Field, None)),
        );
        let br = Rect::from_min_size(pos2(fr.right() + 16.0, y - 2.0), vec2(btn_w, 50.0));
        if button(
            ui,
            p,
            br,
            "add-folder",
            "Add folder",
            ButtonStyle::Primary,
            true,
            self.model.focus == Focus::AddFolder,
            None,
        )
        .clicked()
        {
            clicks.push(Click::Focus(Focus::AddFolder, Some(Input::Press)));
        }
        y = fr.bottom() + 8.0;
        y = self.note_line(ui, p, "folder-note", &self.folder_note, x, y, w) + 12.0;

        // the Chihiro BIOS folder
        let hr = text(
            p,
            pos2(x, y),
            Align2::LEFT_TOP,
            "Chihiro BIOS folder, needed for OutRun 2 SP and Crazy Taxi High Roller",
            theme::semibold(size::LABEL),
            INK,
        );
        say(
            ui,
            "bios-heading",
            hr,
            "Chihiro BIOS folder, needed for OutRun 2 SP and Crazy Taxi High Roller",
        );
        y += 30.0;
        let fr = Rect::from_min_size(pos2(x, y), vec2(field_w, 46.0));
        text_field(
            ui,
            p,
            fr,
            TextField {
                id: "bios-field",
                label: "Chihiro BIOS folder",
                text: &self.bios,
                placeholder: "/path/to/bios/chihiro",
                focused: self.model.focus == Focus::BiosField,
            },
            || clicks.push(Click::Focus(Focus::BiosField, None)),
        );
        let br = Rect::from_min_size(pos2(fr.right() + 16.0, y - 2.0), vec2(btn_w, 50.0));
        if button(
            ui,
            p,
            br,
            "use-bios",
            "Use BIOS",
            ButtonStyle::Primary,
            true,
            self.model.focus == Focus::UseBios,
            None,
        )
        .clicked()
        {
            clicks.push(Click::Focus(Focus::UseBios, Some(Input::Press)));
        }
        y = fr.bottom() + 8.0;
        if self.bios_note.is_none()
            && let Some(b) = &self.config.chihiro_bios
        {
            let line = format!("In use: {}", b.display());
            let lr = text(
                p,
                pos2(x, y),
                Align2::LEFT_TOP,
                &ellipsize(p, &line, theme::body(size::BODY), w),
                theme::body(size::BODY),
                MUTED,
            );
            say(ui, "bios-current", lr, &line);
            y += 26.0;
        }
        y = self.note_line(ui, p, "bios-note", &self.bios_note, x, y, w) + 12.0;

        let dr = Rect::from_min_size(pos2(x, y), vec2(btn_w, 50.0));
        if button(
            ui,
            p,
            dr,
            "done",
            "Done",
            ButtonStyle::Outline,
            has_library,
            self.model.focus == Focus::Done,
            Some("Add a game folder first."),
        )
        .clicked()
            && has_library
        {
            clicks.push(Click::Focus(Focus::Done, Some(Input::Press)));
        }
    }

    /// A one-line result under a field; returns the y below it.
    #[allow(clippy::too_many_arguments)]
    fn note_line(
        &self,
        ui: &egui::Ui,
        p: &egui::Painter,
        key: &str,
        note: &Note,
        x: f32,
        y: f32,
        w: f32,
    ) -> f32 {
        let Some((words, ok)) = note else { return y };
        let color = if *ok { INK } else { ERROR };
        let nr = wrapped(p, pos2(x, y), words, theme::body(size::BODY), color, w);
        say(ui, key, nr, words);
        nr.bottom().max(y + 24.0)
    }

    fn draw_footer(&self, p: &egui::Painter, r: Rect) {
        let cy = r.bottom() - 30.0;
        let x = r.left() + MARGIN;
        let hints: &[(&str, &str)] = match self.model.page {
            Page::Hub => &[(ARROWS, "Choose"), ("Enter", "Play"), ("Esc", "Quit")],
            Page::Settings => &[
                (ARROWS, "Change"),
                ("Enter", "Next choice"),
                ("Esc", "Back"),
                ("PgUp PgDn", "Switch page"),
            ],
            Page::Controls => &[("Esc", "Back"), ("PgUp PgDn", "Switch page")],
            Page::FirstRun if !self.config.libraries.is_empty() => {
                &[("Enter", "Add"), ("Up Down", "Move"), ("Esc", "Back")]
            }
            Page::FirstRun => &[("Enter", "Add"), ("Up Down", "Move"), ("Esc", "Quit")],
        };
        let mut hx = x;
        for (k, w) in hints {
            hx = hint(p, hx, cy, k, w);
        }
        text(
            p,
            pos2(r.right() - MARGIN, cy),
            Align2::RIGHT_CENTER,
            &format!("Version {}", env!("CARGO_PKG_VERSION")),
            theme::body(size::NOTE),
            MUTED,
        );
    }

    fn draw_running(&self, ui: &egui::Ui, p: &egui::Painter, r: Rect, clicks: &mut Vec<Click>) {
        let Some(run) = &self.running else { return };
        p.rect_filled(r, CornerRadius::ZERO, alpha(BASE, 248));
        let c = r.center();
        let (head, body) = if run.playing {
            (
                format!("{} is running", run.game.title),
                "Close the game window or press Stop to come back.",
            )
        } else if run.building {
            (
                "Building lindbergh-loader…".to_string(),
                "OutRun 2 SP SDX runs in lindbergh-loader, built once on your computer \
                 (its licence forbids shipping it). This takes a few minutes.",
            )
        } else {
            (
                format!("Getting {} ready…", run.game.title),
                "This takes a few seconds the first time.",
            )
        };
        shield(p, pos2(c.x - 30.0, c.y - 50.0), run.game.short, 20.0);
        let hr = text(
            p,
            pos2(c.x, c.y),
            Align2::CENTER_CENTER,
            &head,
            theme::display(size::PAGE_TITLE),
            INK,
        );
        say(ui, "running", hr, &head);
        let br = text(
            p,
            pos2(c.x, c.y + 44.0),
            Align2::CENTER_CENTER,
            body,
            theme::body(size::LABEL),
            MUTED,
        );
        say(ui, "running-body", br, body);
        if run.playing {
            let sr = Rect::from_center_size(pos2(c.x, c.y + 120.0), vec2(180.0, 50.0));
            if button(
                ui,
                p,
                sr,
                "stop",
                "Stop",
                ButtonStyle::Outline,
                !run.stop.load(Ordering::SeqCst),
                true,
                Some("Stopping the game…"),
            )
            .clicked()
            {
                clicks.push(Click::StopGame);
            }
        }
    }

    fn draw_message(&mut self, ui: &egui::Ui, p: &egui::Painter, r: Rect, clicks: &mut Vec<Click>) {
        let Some(m) = &self.message else { return };
        p.rect_filled(r, CornerRadius::ZERO, alpha(BASE, 200));
        let w = 620.0;
        let body = p.layout(m.body.clone(), theme::body(size::BODY), INK, w - 48.0);
        let h = 40.0 + 24.0 + body.size().y + 24.0 + 50.0 + 24.0;
        let cr = Rect::from_center_size(r.center(), vec2(w, h));
        card(p, cr);
        let y = card_heading(p, cr, &m.title) + 24.0;
        say(
            ui,
            "message-title",
            Rect::from_min_size(cr.min, vec2(cr.width(), 40.0)),
            &m.title,
        );
        let body_at = pos2(cr.left() + 24.0, y);
        say(
            ui,
            "message-body",
            Rect::from_min_size(body_at, body.size()),
            &m.body,
        );
        p.galley(body_at, body.clone(), INK);
        let br = Rect::from_min_size(
            pos2(cr.left() + 24.0, y + body.size().y + 24.0),
            vec2(140.0, 50.0),
        );
        if button(
            ui,
            p,
            br,
            "dismiss",
            "OK",
            ButtonStyle::Primary,
            true,
            true,
            None,
        )
        .clicked()
        {
            clicks.push(Click::Dismiss);
        }
    }
}

struct TextField<'a> {
    id: &'static str,
    label: &'a str,
    text: &'a str,
    placeholder: &'a str,
    focused: bool,
}

/// A one-line field: the text, its end kept in view, and a caret. The
/// window edits the text itself ([`edit_text`]).
fn text_field(
    ui: &egui::Ui,
    p: &egui::Painter,
    field: Rect,
    f: TextField,
    on_click: impl FnOnce(),
) {
    p.rect_filled(field, CornerRadius::same(6), PANEL);
    let resp = semantic(
        ui,
        id_of(ui, f.id),
        field,
        Sense::click(),
        Semantics::new(Role::TextInput, f.label).valued(f.text),
    )
    .on_hover_cursor(CursorIcon::Text);
    if resp.clicked() {
        on_click();
    }
    let font = theme::body(size::LABEL);
    let inner = field.shrink2(vec2(16.0, 0.0));
    if f.text.is_empty() {
        text(
            p,
            inner.left_center(),
            Align2::LEFT_CENTER,
            f.placeholder,
            font.clone(),
            MUTED,
        );
    }
    let g = p.layout_no_wrap(f.text.to_string(), font, INK);
    let tw = g.size().x;
    let tx = inner.left() - (tw - inner.width() + 6.0).max(0.0);
    p.with_clip_rect(inner)
        .galley(pos2(tx, inner.center().y - g.size().y / 2.0), g, INK);
    let blink = ui.input(|i| i.time).fract() < 0.6;
    if f.focused && blink {
        let cx = (tx + tw + 2.0).min(inner.right());
        p.line_segment(
            [
                pos2(cx, inner.center().y - 11.0),
                pos2(cx, inner.center().y + 11.0),
            ],
            Stroke::new(2.0, LAMP),
        );
    }
    p.rect_stroke(
        field,
        CornerRadius::same(6),
        Stroke::new(2.0, if f.focused { INK } else { LINE }),
        StrokeKind::Inside,
    );
    if f.focused {
        focus_ring(p, field, 6);
    }
}

fn edit_text(s: &mut String, events: &[egui::Event]) -> bool {
    let before = s.clone();
    for e in events {
        match e {
            egui::Event::Text(t) | egui::Event::Paste(t) => {
                s.extend(t.chars().filter(|c| !c.is_control()));
            }
            egui::Event::Key {
                key: egui::Key::Backspace,
                pressed: true,
                modifiers,
                ..
            } => {
                if modifiers.ctrl {
                    // a whole path part
                    let keep = s.trim_end_matches('/').rfind('/').map_or(0, |i| i + 1);
                    s.truncate(keep);
                } else {
                    s.pop();
                }
            }
            _ => {}
        }
    }
    *s != before
}

fn arrow(p: &egui::Painter, c: Pos2, dir: f32, color: Color32) {
    let s = 7.0;
    let pts = vec![
        pos2(c.x + dir * s, c.y),
        pos2(c.x - dir * s * 0.6, c.y - s),
        pos2(c.x - dir * s * 0.6, c.y + s),
    ];
    p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// A level bar: 0…1, or -1…1 from the centre.
fn bar(p: &egui::Painter, r: Rect, v: f32, centred: bool) {
    p.rect_filled(r, CornerRadius::same(4), BASE);
    p.rect_stroke(
        r,
        CornerRadius::same(4),
        Stroke::new(1.5, LINE),
        StrokeKind::Inside,
    );
    let inner = r.shrink(3.0);
    let fill = if centred {
        let c = inner.center().x;
        let x = c + v.clamp(-1.0, 1.0) * inner.width() / 2.0;
        Rect::from_min_max(pos2(c.min(x), inner.top()), pos2(c.max(x), inner.bottom()))
    } else {
        Rect::from_min_size(
            inner.min,
            vec2(inner.width() * v.clamp(0.0, 1.0), inner.height()),
        )
    };
    p.rect_filled(fill, CornerRadius::same(2), LAMP);
    if centred {
        p.line_segment(
            [
                pos2(r.center().x, r.top() - 3.0),
                pos2(r.center().x, r.bottom() + 3.0),
            ],
            Stroke::new(2.0, INK),
        );
    }
}

fn draw_device_glyph(p: &egui::Painter, c: Pos2, kind: Option<Kind>) {
    let st = Stroke::new(2.0, INK);
    match kind {
        Some(Kind::Wheel) => {
            p.circle_stroke(c, 8.0, st);
            p.circle_filled(c, 2.5, INK);
            p.line_segment([pos2(c.x - 8.0, c.y), pos2(c.x + 8.0, c.y)], st);
        }
        Some(_) => {
            let r = Rect::from_center_size(c, vec2(20.0, 12.0));
            p.rect_stroke(r, CornerRadius::same(5), st, StrokeKind::Inside);
            p.circle_filled(pos2(c.x + 4.5, c.y), 1.6, INK);
            p.line_segment(
                [pos2(c.x - 7.0, c.y), pos2(c.x - 2.0, c.y)],
                Stroke::new(1.5, INK),
            );
        }
        None => {
            let r = Rect::from_center_size(c, vec2(20.0, 12.0));
            p.rect_stroke(
                r,
                CornerRadius::same(5),
                Stroke::new(2.0, MUTED),
                StrokeKind::Inside,
            );
            p.line_segment(
                [pos2(c.x - 10.0, c.y + 8.0), pos2(c.x + 10.0, c.y - 8.0)],
                Stroke::new(2.0, INK),
            );
        }
    }
}

fn ellipsize(p: &egui::Painter, s: &str, font: FontId, max: f32) -> String {
    if text_width(p, s, font.clone()) <= max {
        return s.to_string();
    }
    let mut chars: Vec<char> = s.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let t: String = chars.iter().collect::<String>() + "…";
        if text_width(p, &t, font.clone()) <= max {
            return t;
        }
    }
    "…".into()
}

fn ellipsized_text(
    p: &egui::Painter,
    pos: Pos2,
    s: &str,
    font: FontId,
    color: Color32,
    max: f32,
) -> Rect {
    let t = ellipsize(p, s, font.clone(), max);
    text(p, pos, Align2::LEFT_TOP, &t, font, color)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rea_launcher_ui::testing::kittest::NodeT as _;
    use rea_launcher_ui::testing::{self, Harness, Queryable};
    use sega_core::settings::{Settings, Size};
    use std::path::Path;

    use crate::input::DeviceInfo;

    const SCREEN: Vec2 = vec2(1280.0, 720.0);

    fn paths_in(t: &Path) -> Paths {
        Paths {
            data: t.join("data"),
            components: t.join("components"),
        }
    }

    /// A window over an app that has `libraries` saved, with no controllers.
    fn window(paths: &Paths, libraries: &[PathBuf]) -> Harness<'static, App> {
        let mut config = Config::default();
        for l in libraries {
            config.add_library(l);
        }
        if !libraries.is_empty() {
            paths.save_config(&config).unwrap();
        }
        let fonts = paths.fonts();
        let mut h = Harness::builder()
            .with_size(SCREEN)
            .with_theme(egui::Theme::Dark)
            .with_context_setup(move |ctx| {
                theme::install_fonts(ctx, &fonts);
                theme::install_style(ctx);
            })
            .build_ui_state(
                |ui, app: &mut App| app.frame(ui),
                App::new(paths.clone(), None),
            );
        h.run_steps(3);
        h
    }

    fn wait(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
        for _ in 0..500 {
            h.run_steps(1);
            if done(h.state()) {
                h.run_steps(2);
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for {what}");
    }

    /// A window over an empty library folder, once its scan is done.
    fn hub(t: &Path) -> Harness<'static, App> {
        let lib = t.join("games");
        std::fs::create_dir_all(&lib).unwrap();
        let mut h = window(&paths_in(t), &[lib]);
        wait(&mut h, "the scan", |a| a.entries.is_some());
        h
    }

    /// The first game in the catalog that is `want`. The catalog changes as
    /// games are verified, so the tests ask for a kind, not a name.
    fn game_where(what: &str, want: impl Fn(&Game) -> bool) -> &'static Game {
        GAMES
            .iter()
            .find(|g| want(g))
            .unwrap_or_else(|| panic!("the catalog has no {what} game"))
    }

    fn ready() -> &'static Game {
        game_where("verified Windows", |g| {
            g.support == Support::Ready && matches!(g.route, Route::Wal { .. })
        })
    }

    /// A catalog game shown with `support` (every game in the catalog may be
    /// verified, so the tests set the other states themselves).
    fn shown_as(h: &mut Harness<'_, App>, support: Support) -> &'static Game {
        let g = ready();
        let i = GAMES.iter().position(|x| x.id == g.id).unwrap();
        h.state_mut().supports[i] = support;
        g
    }

    fn chihiro() -> &'static Game {
        game_where("Chihiro", |g| matches!(g.route, Route::Cxbx))
    }

    fn fake_entry(slug: &str, dir: &Path) -> Entry {
        let game = sega_core::catalog::by_slug(slug).unwrap();
        Entry {
            dir: dir.to_path_buf(),
            found: Found::Known {
                game,
                edition: &game.editions[0],
            },
        }
    }

    fn tile<'h>(h: &'h Harness<'_, App>, slug: &str) -> testing::Node<'h> {
        let game = sega_core::catalog::by_slug(slug).unwrap();
        h.get_by_role_and_label(Role::RadioButton, game.title)
    }

    fn select(h: &mut Harness<'_, App>, slug: &str) {
        tile(h, slug).click();
        h.run_steps(3);
        let game = sega_core::catalog::by_slug(slug).unwrap();
        assert_eq!(GAMES[h.state().model.selected].slug, game.slug);
    }

    fn says(h: &Harness<'_, App>, words: &str) -> bool {
        h.query_by_label_contains(words).is_some()
    }

    fn click_button(h: &mut Harness<'_, App>, label: &str) {
        h.get_by_role_and_label(Role::Button, label).click();
        h.run_steps(3);
    }

    fn open_tab(h: &mut Harness<'_, App>, tab: Tab) {
        h.get_by_role_and_label(Role::Tab, tab.label()).click();
        h.run_steps(3);
    }

    fn settings_of(paths: &Paths, slug: &str) -> (Settings, String) {
        let text = std::fs::read_to_string(paths.settings(slug)).unwrap_or_default();
        (Settings::parse(&text).0, text)
    }

    // ------------------------------------------------------------ hub

    #[test]
    fn the_window_is_not_named_like_a_game() {
        assert_eq!(TITLE, "Sega Racing Launcher");
        assert!(GAMES.iter().all(|g| g.title != TITLE));
    }

    #[test]
    fn the_hub_has_a_tile_for_each_of_the_six_games() {
        let t = tempfile::tempdir().unwrap();
        let h = hub(t.path());
        assert_eq!(GAMES.len(), 6);
        assert_eq!(h.get_all_by_role(Role::RadioButton).count(), 6);
        for g in GAMES {
            let node = h.get_by_role_and_label(Role::RadioButton, g.title);
            let about = testing::description(&node).unwrap();
            assert!(about.contains(&g.year.to_string()), "{about}");
            assert!(about.contains(g.board), "{about}");
        }
        assert!(says(&h, "Your games · 0 of 6 found"));
        assert!(testing::checked(&tile(&h, GAMES[0].slug)));
        assert!(!testing::checked(&tile(&h, GAMES[1].slug)));
    }

    #[test]
    fn selecting_a_tile_shows_that_games_page() {
        let t = tempfile::tempdir().unwrap();
        let mut h = hub(t.path());
        let g = ready();
        let dir = t.path().join("games/ready");
        std::fs::create_dir_all(&dir).unwrap();
        h.state_mut().entries = Some(vec![fake_entry(g.slug, &dir)]);
        select(&mut h, g.slug);
        assert!(testing::checked(&tile(&h, g.slug)));
        assert!(!testing::checked(&tile(&h, GAMES[0].slug)) || GAMES[0].slug == g.slug);
        assert!(h.query_by_role_and_label(Role::Label, g.title).is_some());
        let Found::Known { edition, .. } = &h.state().entries.as_ref().unwrap()[0].found else {
            unreachable!()
        };
        assert!(says(
            &h,
            &format!("{} · {} · {}", g.board, g.year, edition.label)
        ));
        assert!(says(&h, "Ready"));
        assert!(says(&h, &format!("Folder: {}", dir.display())));
        let play = h.get_by_role_and_label(Role::Button, "Play");
        assert!(!testing::disabled(&play));
        assert!(says(&h, "Your games · 1 of 6 found"));
    }

    #[test]
    fn a_blocked_game_cannot_be_played_and_says_why() {
        let t = tempfile::tempdir().unwrap();
        let mut h = hub(t.path());
        let why = "Not yet: it crashes.";
        let blocked = shown_as(&mut h, Support::Blocked(why));
        // even a game found on disk stays locked
        let dir = t.path().join("games/blocked");
        std::fs::create_dir_all(&dir).unwrap();
        h.state_mut().entries = Some(vec![fake_entry(blocked.slug, &dir)]);
        select(&mut h, blocked.slug);
        let play = h.get_by_role_and_label(Role::Button, "Play");
        assert!(testing::disabled(&play));
        assert_eq!(testing::description(&play).as_deref(), Some(why));
        assert!(says(&h, why), "the reason is on the page");
        assert!(says(&h, "Not supported yet"));
        // pressing it does nothing
        click_button(&mut h, "Play");
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert!(h.state().running.is_none() && h.state().message.is_none());
    }

    #[test]
    fn an_unverified_game_shows_its_note_and_can_be_played() {
        let t = tempfile::tempdir().unwrap();
        let mut h = hub(t.path());
        let note = "Boots; no race verified.";
        let g = shown_as(&mut h, Support::Unverified(note));
        let dir = t.path().join("games/unverified");
        std::fs::create_dir_all(&dir).unwrap();
        h.state_mut().entries = Some(vec![fake_entry(g.slug, &dir)]);
        select(&mut h, g.slug);
        assert!(says(&h, note));
        assert!(says(&h, "Starts, not verified"));
        assert!(!testing::disabled(
            &h.get_by_role_and_label(Role::Button, "Play")
        ));
    }

    #[test]
    fn a_game_missing_from_the_library_cannot_be_played() {
        let t = tempfile::tempdir().unwrap();
        let mut h = hub(t.path());
        select(&mut h, ready().slug);
        let play = h.get_by_role_and_label(Role::Button, "Play");
        assert!(testing::disabled(&play));
        assert!(says(&h, "Not in your library"));
        assert!(says(&h, "Add its folder in Settings"));
        assert!(testing::description(&play).unwrap().contains("Not found"));
    }

    #[test]
    fn play_runs_the_session_on_a_worker_and_reports_why_it_could_not_start() {
        let t = tempfile::tempdir().unwrap();
        let mut h = hub(t.path());
        let g = ready();
        let dir = t.path().join("games/ready");
        std::fs::create_dir_all(&dir).unwrap();
        h.state_mut().entries = Some(vec![fake_entry(g.slug, &dir)]);
        select(&mut h, g.slug);
        click_button(&mut h, "Play");
        // the launcher's components are not installed in the test folder
        wait(&mut h, "the failure", |a| a.message.is_some());
        assert!(h.state().running.is_none(), "the window is free again");
        assert!(says(&h, &format!("{} couldn't start", g.title)));
        assert!(says(&h, "needs launcher components that are missing"));
        click_button(&mut h, "OK");
        assert!(h.state().message.is_none());
        assert!(h.query_by_label_contains("couldn't start").is_none());
    }

    #[test]
    fn a_running_game_shows_the_overlay_and_stop_asks_the_session_to_end() {
        let t = tempfile::tempdir().unwrap();
        let mut h = hub(t.path());
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        h.state_mut().running = Some(Running {
            game: ready(),
            building: false,
            playing: false,
            progress: rx,
            stop: Arc::clone(&stop),
        });
        h.run_steps(3);
        assert!(says(&h, &format!("Getting {} ready", ready().title)));
        assert!(h.query_by_role_and_label(Role::Button, "Stop").is_none());
        // OutRun 2 SP SDX's first start builds lindbergh-loader first
        tx.send(Progress::Building).unwrap();
        h.run_steps(3);
        assert!(says(&h, "Building lindbergh-loader"));
        assert!(says(&h, "This takes a few minutes"));
        assert!(h.query_by_role_and_label(Role::Button, "Stop").is_none());
        tx.send(Progress::Playing).unwrap();
        h.run_steps(3);
        assert!(says(&h, &format!("{} is running", ready().title)));
        click_button(&mut h, "Stop");
        assert!(stop.load(Ordering::SeqCst));
        // the session ends; the hub is back
        tx.send(Progress::Ended(Ok(()))).unwrap();
        h.run_steps(3);
        assert!(h.state().running.is_none());
        assert!(h.query_by_label_contains("is running").is_none());
    }

    #[test]
    fn the_keyboard_moves_between_tiles_and_pages() {
        let t = tempfile::tempdir().unwrap();
        let mut h = hub(t.path());
        for _ in 0..2 {
            h.key_press(egui::Key::ArrowRight);
            h.run_steps(2);
        }
        assert_eq!(h.state().model.selected, 2);
        assert!(testing::checked(&tile(&h, GAMES[2].slug)));
        h.key_press(egui::Key::PageDown);
        h.run_steps(3);
        assert_eq!(h.state().model.page, Page::Controls);
        h.key_press(egui::Key::PageDown);
        h.run_steps(3);
        assert_eq!(h.state().model.page, Page::Settings);
        assert!(says(&h, &format!("{} settings", GAMES[2].title)));
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        assert_eq!(h.state().model.page, Page::Hub);
    }

    // ------------------------------------------------------------ settings

    #[test]
    #[cfg_attr(windows, ignore = "the gamescope rows are Linux-only")]
    fn every_option_in_the_settings_window_is_saved_through_settings_save() {
        let t = tempfile::tempdir().unwrap();
        let mut h = hub(t.path());
        let paths = h.state().paths.clone();
        let g = sega_core::catalog::by_slug("sr3").unwrap();
        std::fs::create_dir_all(paths.data.join("settings")).unwrap();
        std::fs::write(paths.settings(g.slug), "# my notes\nfuture_key = 7\n").unwrap();
        select(&mut h, g.slug);
        open_tab(&mut h, Tab::Settings);
        assert!(says(&h, "SEGA Rally 3 settings"));
        for row in Row::ALL {
            assert!(
                h.query_by_role_and_label(Role::Button, row.label())
                    .is_some(),
                "{row:?}"
            );
        }
        let mut expect = Settings::default();
        type Change = Box<dyn Fn(&mut Settings)>;
        let steps: [(Row, &str, Change); 8] = [
            (
                Row::Render,
                "next",
                Box::new(|s| s.render = Some(Size::new(1920, 1080))),
            ),
            (
                Row::Output,
                "previous",
                Box::new(|s| s.output = Size::new(2560, 1440)),
            ),
            (
                Row::Upscaler,
                "next",
                Box::new(|s| s.upscaler = sega_core::settings::Upscaler::Nis),
            ),
            (Row::Sharpness, "next", Box::new(|s| s.sharpness = 6)),
            (Row::Fullscreen, "next", Box::new(|s| s.fullscreen = false)),
            (
                Row::SteeringLock,
                "next",
                Box::new(|s| s.steering_lock = rea_launcher::wheel::SteeringLock::Degrees(360)),
            ),
            (
                Row::ForceFeedback,
                "previous",
                Box::new(|s| s.force_feedback = 90),
            ),
            (Row::Deadzone, "next", Box::new(|s| s.deadzone = 6)),
        ];
        for (row, way, apply) in &steps {
            h.get_by_role_and_label(Role::Button, &format!("{}: {way}", row.label()))
                .click();
            h.run_steps(3);
            apply(&mut expect);
            let (saved, text) = settings_of(&paths, g.slug);
            assert_eq!(saved, expect, "{row:?}\n{text}");
            assert!(
                text.starts_with("# my notes\nfuture_key = 7\n"),
                "notes and unknown keys stay put:\n{text}"
            );
            assert!(says(&h, "Saved."), "{row:?}");
        }
        // what the window shows is what was saved
        let shown = |h: &Harness<'_, App>, row: Row| {
            testing::value(&h.get_by_role_and_label(Role::Button, row.label())).unwrap()
        };
        assert_eq!(shown(&h, Row::Render), "1920 × 1080 (1080p)");
        assert_eq!(shown(&h, Row::Output), "2560 × 1440 (1440p)");
        assert_eq!(shown(&h, Row::Upscaler), "NVIDIA Image Scaling");
        assert_eq!(shown(&h, Row::Fullscreen), "Off");
        assert_eq!(shown(&h, Row::SteeringLock), "360°");
        assert_eq!(shown(&h, Row::ForceFeedback), "90%");
        assert_eq!(shown(&h, Row::Deadzone), "6%");
        assert_eq!(shown(&h, Row::Look), "Custom");
        // a look sets the picture at once
        h.get_by_role_and_label(Role::Button, "Look: next").click();
        h.run_steps(3);
        let (saved, _) = settings_of(&paths, g.slug);
        assert!(saved.look.is_some(), "the look reached the file");
    }

    #[test]
    fn a_game_that_reads_the_wheel_itself_says_why_its_wheel_rows_are_off() {
        let t = tempfile::tempdir().unwrap();
        let mut h = hub(t.path());
        select(&mut h, "or2sdx");
        open_tab(&mut h, Tab::Settings);
        let lock = h.get_by_role_and_label(Role::Button, Row::SteeringLock.label());
        assert!(testing::disabled(&lock));
        assert!(
            testing::description(&lock)
                .unwrap()
                .contains("reads your wheel itself")
        );
    }

    #[test]
    #[cfg(windows)]
    fn on_windows_the_settings_offer_the_render_size_and_dead_zone_only() {
        let t = tempfile::tempdir().unwrap();
        let mut h = hub(t.path());
        open_tab(&mut h, Tab::Settings);
        for row in Row::shown(Host::Windows) {
            assert!(
                h.query_by_role_and_label(Role::Button, row.label())
                    .is_some(),
                "{row:?}"
            );
        }
        for row in [Row::Output, Row::Upscaler, Row::Sharpness, Row::Fullscreen] {
            assert!(
                h.query_by_role_and_label(Role::Button, row.label())
                    .is_none(),
                "{row:?}"
            );
        }
    }

    #[test]
    #[cfg_attr(windows, ignore = "the gamescope rows are Linux-only")]
    fn settings_belong_to_the_selected_game() {
        let t = tempfile::tempdir().unwrap();
        let mut h = hub(t.path());
        let paths = h.state().paths.clone();
        let g = GAMES.last().unwrap();
        select(&mut h, g.slug);
        open_tab(&mut h, Tab::Settings);
        assert!(says(&h, &format!("{} settings", g.title)));
        h.get_by_role_and_label(Role::Button, "Full screen: next")
            .click();
        h.run_steps(3);
        assert!(!settings_of(&paths, g.slug).0.fullscreen);
        assert!(!paths.settings(GAMES[0].slug).exists());
        // the cabinet's own size is named for this game
        assert_eq!(
            testing::value(&h.get_by_role_and_label(Role::Button, "Game resolution")).unwrap(),
            format!("Native, {}", form::size_name(g.native))
        );
    }

    #[test]
    #[cfg_attr(windows, ignore = "the gamescope rows are Linux-only")]
    fn a_row_click_changes_the_value_and_sharpness_goes_off_without_fsr() {
        let t = tempfile::tempdir().unwrap();
        let mut h = hub(t.path());
        let paths = h.state().paths.clone();
        open_tab(&mut h, Tab::Settings);
        // clicking the row itself is Enter: next choice, wrapping
        h.get_by_role_and_label(Role::Button, "Upscaler").click();
        h.run_steps(3);
        assert_eq!(
            settings_of(&paths, GAMES[0].slug).0.upscaler,
            sega_core::settings::Upscaler::Nis
        );
        h.get_by_role_and_label(Role::Button, "Upscaler").click();
        h.run_steps(3);
        let sharp = h.get_by_role_and_label(Role::Button, "Sharpness");
        assert!(testing::disabled(&sharp));
        assert!(testing::description(&sharp).unwrap().contains("FSR 1"));
        assert!(h.query_by_label_contains("Sharpness: next").is_none());
        // wraps back to the first upscaler
        for _ in 0..2 {
            h.get_by_role_and_label(Role::Button, "Upscaler").click();
            h.run_steps(3);
        }
        assert_eq!(
            settings_of(&paths, GAMES[0].slug).0.upscaler,
            sega_core::settings::Upscaler::Fsr
        );
        assert!(!testing::disabled(
            &h.get_by_role_and_label(Role::Button, "Sharpness")
        ));
    }

    #[test]
    fn rescan_looks_again_and_finds_a_new_game() {
        let t = tempfile::tempdir().unwrap();
        let mut h = hub(t.path());
        open_tab(&mut h, Tab::Settings);
        assert!(says(&h, "0 of 6 games found."));
        click_button(&mut h, "Rescan");
        wait(&mut h, "the rescan", |a| a.entries.is_some());
        assert!(says(&h, "0 of 6 games found."));
        let lib = t.path().join("games");
        assert!(says(&h, &lib.display().to_string()));
    }

    // ------------------------------------------------------------ setup

    #[test]
    fn the_first_run_adds_a_library_folder_and_opens_the_hub() {
        let t = tempfile::tempdir().unwrap();
        let paths = paths_in(t.path());
        let mut h = window(&paths, &[]);
        assert_eq!(h.state().model.page, Page::FirstRun);
        assert!(says(&h, "Choose your game folders"));
        assert!(says(&h, "No folder yet."));
        assert!(h.query_by_role_and_label(Role::Tab, "Games").is_none());
        let done = h.get_by_role_and_label(Role::Button, "Done");
        assert!(testing::disabled(&done));
        assert!(
            testing::description(&done)
                .unwrap()
                .contains("Add a game folder")
        );

        // nothing typed
        click_button(&mut h, "Add folder");
        assert!(says(&h, "Type the folder's path first."));
        // a path that is not a folder
        let field = h.get_by_role_and_label(Role::TextInput, "Game folder");
        field.click();
        field.type_text("/definitely/not/here");
        h.run_steps(3);
        click_button(&mut h, "Add folder");
        assert!(says(&h, "isn't a folder this computer can open"));
        assert!(paths.config().libraries.is_empty());

        // a real one, typed after clearing the wrong path
        let lib = t.path().join("my games");
        std::fs::create_dir_all(&lib).unwrap();
        h.state_mut().folder.clear();
        let field = h.get_by_role_and_label(Role::TextInput, "Game folder");
        field.click();
        field.type_text(&lib.display().to_string());
        h.run_steps(3);
        assert_eq!(
            testing::value(&h.get_by_role_and_label(Role::TextInput, "Game folder")).unwrap(),
            lib.display().to_string()
        );
        click_button(&mut h, "Add folder");
        assert_eq!(paths.config().libraries, std::slice::from_ref(&lib));
        assert!(
            h.query_by_role_and_label(Role::Label, &format!("Added {}.", lib.display()))
                .is_some()
        );
        assert!(
            h.query_by_role_and_label(Role::Label, &lib.display().to_string())
                .is_some(),
            "the folder is listed"
        );
        // it is read, and the page can be left
        wait(&mut h, "the scan", |a| a.entries.is_some());
        assert!(!testing::disabled(
            &h.get_by_role_and_label(Role::Button, "Done")
        ));
        click_button(&mut h, "Done");
        assert_eq!(h.state().model.page, Page::Hub);
        assert!(says(&h, "Your games · 0 of 6 found"));
        assert_eq!(h.get_all_by_role(Role::RadioButton).count(), 6);
    }

    #[test]
    fn enter_adds_the_typed_folder_and_a_second_folder_is_kept() {
        let t = tempfile::tempdir().unwrap();
        let paths = paths_in(t.path());
        let first = t.path().join("a");
        let second = t.path().join("b");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        let mut h = window(&paths, std::slice::from_ref(&first));
        wait(&mut h, "the scan", |a| a.entries.is_some());
        // Settings > Folders and BIOS opens the same page
        open_tab(&mut h, Tab::Settings);
        click_button(&mut h, "Folders and BIOS");
        assert_eq!(h.state().model.page, Page::FirstRun);
        assert!(says(&h, "Your game folders"));
        h.get_by_role_and_label(Role::TextInput, "Game folder")
            .type_text(&second.display().to_string());
        h.run_steps(2);
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert_eq!(paths.config().libraries, [first.clone(), second.clone()]);
        // the same folder twice is one folder
        h.get_by_role_and_label(Role::TextInput, "Game folder")
            .type_text(&second.display().to_string());
        h.run_steps(2);
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert_eq!(paths.config().libraries.len(), 2);
        wait(&mut h, "the scan", |a| a.entries.is_some());
        // Esc goes back to Settings, which lists both
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        assert_eq!(h.state().model.page, Page::Settings);
        assert!(says(&h, &first.display().to_string()));
        assert!(says(&h, &second.display().to_string()));
    }

    #[test]
    fn the_first_run_takes_the_chihiro_bios_path() {
        let t = tempfile::tempdir().unwrap();
        let paths = paths_in(t.path());
        let lib = t.path().join("games");
        std::fs::create_dir_all(&lib).unwrap();
        let mut h = window(&paths, &[lib]);
        wait(&mut h, "the scan", |a| a.entries.is_some());
        open_tab(&mut h, Tab::Settings);
        click_button(&mut h, "Folders and BIOS");

        click_button(&mut h, "Use BIOS");
        assert!(says(&h, "Type the path of your Chihiro BIOS folder first."));
        let field = h.get_by_role_and_label(Role::TextInput, "Chihiro BIOS folder");
        field.click();
        field.type_text("/no/such/chihiro");
        h.run_steps(3);
        click_button(&mut h, "Use BIOS");
        assert!(says(&h, "isn't a folder this computer can open"));
        assert_eq!(paths.config().chihiro_bios, None);

        // a file is not the BIOS folder
        let file = t.path().join("chihiro.zip");
        std::fs::write(&file, b"zip").unwrap();
        h.state_mut().bios = file.display().to_string();
        click_button(&mut h, "Use BIOS");
        assert!(says(&h, "isn't a folder this computer can open"));

        let bios = t.path().join("bios/chihiro");
        std::fs::create_dir_all(&bios).unwrap();
        h.state_mut().bios.clear();
        let field = h.get_by_role_and_label(Role::TextInput, "Chihiro BIOS folder");
        field.click();
        field.type_text(&bios.display().to_string());
        h.run_steps(3);
        click_button(&mut h, "Use BIOS");
        assert!(says(&h, "Chihiro BIOS folder saved."));
        assert_eq!(paths.config().chihiro_bios, Some(bios.clone()));
        // the folders are kept next to it
        assert_eq!(paths.config().libraries.len(), 1);
    }

    #[test]
    fn a_chihiro_game_without_the_bios_says_so_on_its_page() {
        let t = tempfile::tempdir().unwrap();
        let mut h = hub(t.path());
        let g = chihiro();
        let i = GAMES.iter().position(|x| x.id == g.id).unwrap();
        h.state_mut().supports[i] = Support::Unverified("test");
        let dir = t.path().join("games/chihiro");
        std::fs::create_dir_all(&dir).unwrap();
        h.state_mut().entries = Some(vec![fake_entry(g.slug, &dir)]);
        select(&mut h, g.slug);
        assert!(says(&h, "Needs the Chihiro BIOS folder"));
        select(&mut h, ready().slug);
        assert!(
            h.query_by_label_contains("Needs the Chihiro BIOS")
                .is_none()
        );
        select(&mut h, g.slug);
        h.state_mut().config.chihiro_bios = Some(t.path().into());
        h.run_steps(3);
        assert!(
            h.query_by_label_contains("Needs the Chihiro BIOS")
                .is_none()
        );
    }

    // ------------------------------------------------------------ controls

    #[test]
    fn the_controls_page_shows_the_wheel_and_live_steering_and_pedals() {
        let t = tempfile::tempdir().unwrap();
        let mut h = hub(t.path());
        open_tab(&mut h, Tab::Controls);
        assert!(says(&h, "No controller found."));
        assert!(says(&h, "Controller: No controller"));

        let mut controls = rea_launcher::wheel::Controls {
            steer: -0.5,
            accel: 0.25,
            brake: 0.75,
            buttons: 0,
        };
        controls.press(Button::Start);
        h.state_mut().snap = Snapshot {
            devices: vec![
                DeviceInfo {
                    name: "Test Pad".into(),
                    kind: Kind::Pad,
                    haptic: false,
                },
                DeviceInfo {
                    name: "Test Wheel".into(),
                    kind: Kind::Wheel,
                    haptic: true,
                },
            ],
            driver: Some(1),
            controls,
            feeding: true,
            error: None,
        };
        h.run_steps(3);
        assert!(says(&h, "A game is running: it reads these controls."));
        assert!(says(&h, "Wheel: Test Wheel (drives the car)"));
        assert!(says(&h, "Pad: Test Pad"));
        assert!(h.query_by_label_contains("Test Pad (drives").is_none());
        assert!(says(&h, "Controller: Test Wheel"));
        let level = |h: &Harness<'_, App>, name: &str| {
            let m = h.get_by_role_and_label(Role::Meter, name);
            (
                m.accesskit_node().numeric_value().unwrap(),
                testing::value(&m).unwrap(),
            )
        };
        assert_eq!(level(&h, "Steering"), (-50.0, "-50%".to_string()));
        assert_eq!(level(&h, "Gas"), (25.0, "25%".to_string()));
        assert_eq!(level(&h, "Brake"), (75.0, "75%".to_string()));
        assert!(says(&h, "Start: pressed"));
        assert!(says(&h, "Insert coin: released"));
        assert!(says(&h, "Shift up: released"));
        // the pad layout the launcher gives a pad
        assert!(says(&h, "Gas: RT"));

        // the readings follow
        h.state_mut().snap.controls = rea_launcher::wheel::Controls::default();
        h.run_steps(3);
        assert_eq!(level(&h, "Steering").0, 0.0);
        assert!(says(&h, "Start: released"));
    }

    #[test]
    fn a_controller_error_is_explained() {
        let t = tempfile::tempdir().unwrap();
        let mut h = hub(t.path());
        h.state_mut().snap.error = Some("controllers unavailable: no SDL".into());
        open_tab(&mut h, Tab::Controls);
        assert!(says(
            &h,
            "Controllers can't be read: controllers unavailable: no SDL"
        ));
        assert!(says(&h, "Controller: Controllers unavailable"));
    }

    // ------------------------------------------------------------ layout

    #[test]
    fn every_control_fits_the_window_on_every_page() {
        let t = tempfile::tempdir().unwrap();
        let mut h = hub(t.path());
        let screen = Rect::from_min_size(Pos2::ZERO, SCREEN).expand(1.0);
        let check = |h: &mut Harness<'_, App>, page: &str| {
            h.run_steps(3);
            for role in [
                Role::Button,
                Role::Tab,
                Role::RadioButton,
                Role::TextInput,
                Role::Meter,
                Role::Label,
            ] {
                for n in h.query_all_by_role(role) {
                    let Some(b) = n.accesskit_node().bounding_box() else {
                        continue;
                    };
                    let r = Rect::from_min_max(
                        pos2(b.x0 as f32, b.y0 as f32),
                        pos2(b.x1 as f32, b.y1 as f32),
                    );
                    if r.width() * r.height() > 0.0 {
                        assert!(screen.contains_rect(r), "{page}: {role:?} {r:?}");
                    }
                }
            }
        };
        check(&mut h, "hub");
        open_tab(&mut h, Tab::Controls);
        check(&mut h, "controls");
        open_tab(&mut h, Tab::Settings);
        check(&mut h, "settings");
        click_button(&mut h, "Folders and BIOS");
        check(&mut h, "setup");
    }

    #[test]
    fn cover_fit_crops_the_long_side_and_centres() {
        let wide = Rect::from_min_size(Pos2::ZERO, vec2(2000.0, 500.0));
        let uv = cover_uv(vec2(1920.0, 1080.0), wide);
        assert!((uv.width() - 1.0).abs() < 1e-6);
        assert!(uv.height() < 1.0 && (uv.center().y - 0.5).abs() < 1e-6);
        let tall = Rect::from_min_size(Pos2::ZERO, vec2(500.0, 1000.0));
        let uv = cover_uv(vec2(1920.0, 1080.0), tall);
        assert!((uv.height() - 1.0).abs() < 1e-6 && uv.width() < 1.0);
        let same = Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 720.0));
        assert!((cover_uv(vec2(1920.0, 1080.0), same).width() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn the_text_fields_take_text_paste_and_backspace() {
        let key = |ctrl| egui::Event::Key {
            key: egui::Key::Backspace,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl,
                ..Default::default()
            },
        };
        let mut s = String::new();
        assert!(edit_text(
            &mut s,
            &[
                egui::Event::Text("/mnt/".into()),
                egui::Event::Paste("games/Teknoparrot\n".into()),
            ]
        ));
        assert_eq!(s, "/mnt/games/Teknoparrot");
        edit_text(&mut s, &[key(false)]);
        assert_eq!(s, "/mnt/games/Teknoparro");
        edit_text(&mut s, &[key(true)]);
        assert_eq!(s, "/mnt/games/");
        edit_text(&mut s, &[key(true)]);
        assert_eq!(s, "/mnt/");
        assert!(!edit_text(&mut s, &[egui::Event::Copy]));
    }

    #[test]
    fn errors_read_as_sentences() {
        assert_eq!(
            sentence("starting gamescope (is it installed?)."),
            "Starting gamescope (is it installed?)"
        );
        assert_eq!(sentence(""), "Something went wrong");
    }

    #[test]
    fn every_game_has_a_short_name_for_its_shield() {
        for g in GAMES {
            assert!(!g.short.is_empty() && g.short.len() <= 6, "{}", g.slug);
        }
    }

    #[test]
    fn a_typed_home_path_is_expanded() {
        assert_eq!(typed_path("  /a/b "), PathBuf::from("/a/b"));
        if let Some(home) = std::env::var_os("HOME") {
            assert_eq!(typed_path("~/x"), PathBuf::from(home).join("x"));
        }
    }
}
