//! The window over the real backend, headless (`rea_launcher_ui::hub::testing`):
//! what the Sega games add to the hub (their notes, rows, sessions,
//! controls), and golden AccessKit dumps of every page.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use eframe::egui::accesskit::Role;
use eframe::egui::{Vec2, vec2};
use rea_launcher::hub::Tab;
use rea_launcher::library::Entry;
use rea_launcher::wheel::Kind;
use rea_launcher_native::feeder::DeviceInfo;
use rea_launcher_ui::hub::HubApp;
use rea_launcher_ui::hub::testing::{self, SIZE_720, SIZE_800, says};
use rea_launcher_ui::testing::{Harness, NodeT, Queryable};

use super::*;

/// The tests' controllers and next session (see [`Sega::snapshot`] and
/// [`HubBackend::session`]).
#[derive(Default)]
pub(crate) struct Fake {
    pub snapshot: Option<Snapshot>,
    pub session: Option<Session>,
}

type Hub = Harness<'static, HubApp<Sega>>;

fn paths_in(t: &Path) -> HubPaths {
    HubPaths {
        data: t.join("data"),
        components: t.join("components"),
        dev_assets: None,
    }
}

/// A window whose `launcher.cfg` is `config` (none: the first run).
fn window(t: &Path, config: Option<&str>, size: Vec2) -> Hub {
    let paths = paths_in(t);
    if let Some(text) = config {
        std::fs::create_dir_all(&paths.data).unwrap();
        std::fs::write(paths.config_file(), text).unwrap();
    }
    let backend = Sega::new(paths.clone(), None);
    let mut h = testing::harness(spec(paths), backend, size);
    testing::wait(&mut h, "the scan", |a| !a.scanning());
    h
}

/// A game folder as the dumps show it (the same text on every system).
fn shown_dir(slug: &str) -> PathBuf {
    PathBuf::from(format!("/games/{slug}"))
}

fn fake_entry(slug: &str, dir: &Path) -> Entry<Game> {
    let game = catalog::by_slug(slug).unwrap();
    Entry {
        dir: dir.to_path_buf(),
        file: dir.join(game.main_file),
        found: Known::Known {
            game,
            edition: &game.editions[0],
        },
    }
}

/// Every game, found in `/games` (or none).
fn library(every_game: bool) -> Scan<Game> {
    let entries = if every_game {
        GAMES
            .iter()
            .map(|g| fake_entry(g.slug, &shown_dir(g.slug)))
            .collect()
    } else {
        Vec::new()
    };
    Scan {
        entries,
        problems: Vec::new(),
    }
}

/// A hub at `size` whose library is `/games`, holding every game or none.
fn hub(t: &Path, size: Vec2, every_game: bool) -> Hub {
    let mut h = window(t, Some("library = /games\n"), size);
    h.state_mut().set_library(library(every_game));
    h.run_steps(3);
    h
}

fn index(slug: &str) -> usize {
    GAMES.iter().position(|g| g.slug == slug).unwrap()
}

fn select(h: &mut Hub, slug: &str) {
    let title = catalog::by_slug(slug).unwrap().title;
    h.get_by_role_and_label(Role::RadioButton, title).click();
    h.run_steps(3);
    assert_eq!(h.state().model().selected, index(slug));
}

fn click_button(h: &mut Hub, label: &str) {
    h.get_by_role_and_label(Role::Button, label).click();
    h.run_steps(3);
}

fn open_tab(h: &mut Hub, tab: Tab) {
    h.get_by_role_and_label(Role::Tab, tab.label()).click();
    h.run_steps(3);
}

/// The value a settings row shows.
fn row(h: &Hub, label: &str) -> String {
    h.get_by_role_and_label(Role::Button, label)
        .accesskit_node()
        .value()
        .unwrap_or_default()
}

fn row_disabled(h: &Hub, label: &str) -> bool {
    h.get_by_role_and_label(Role::Button, label)
        .accesskit_node()
        .is_disabled()
}

/// Why a row is off, as its description says.
fn row_reason(h: &Hub, label: &str) -> Option<String> {
    let n = h.get_by_role_and_label(Role::Button, label);
    let a = n.accesskit_node();
    a.is_disabled().then(|| a.description().unwrap_or_default())
}

// ------------------------------------------------------------ the hub

#[test]
fn the_window_is_not_named_like_a_game_and_its_colours_are_readable() {
    assert!(GAMES.iter().all(|g| g.title != THEME.title));
    let problems = THEME.contrast_problems();
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_hub_has_a_tile_for_each_of_the_seven_games() {
    let t = tempfile::tempdir().unwrap();
    let h = hub(t.path(), SIZE_720, true);
    assert_eq!(h.query_all_by_role(Role::RadioButton).count(), 7);
    for g in GAMES {
        h.get_by_role_and_label(Role::RadioButton, g.title);
    }
}

#[test]
fn a_blocked_game_cannot_be_played_and_says_why() {
    let t = tempfile::tempdir().unwrap();
    let mut h = hub(t.path(), SIZE_720, true);
    h.state_mut().backend_mut().supports[index("sr3")] = Support::Blocked("Not this one yet.");
    select(&mut h, "sr3");
    assert!(says(&h, "Not this one yet."));
    assert!(row_disabled(&h, "Play"));
}

#[test]
fn an_unverified_game_shows_its_note_and_can_be_played() {
    let t = tempfile::tempdir().unwrap();
    let mut h = hub(t.path(), SIZE_720, true);
    h.state_mut().backend_mut().supports[index("f355")] =
        Support::Unverified("Still being checked.");
    select(&mut h, "f355");
    assert!(says(&h, "Still being checked."));
    assert!(!row_disabled(&h, "Play"));
}

#[test]
fn play_and_settings_never_overlap_the_game_tiles_for_any_game() {
    let t = tempfile::tempdir().unwrap();
    let mut h = window(t.path(), Some("library = /games\nbios = /bios\n"), SIZE_720);
    h.state_mut().set_library(library(true));
    h.run_steps(3);
    for g in GAMES
        .iter()
        .filter(|g| g.support_on(Host::current()) == Support::Ready)
    {
        select(&mut h, g.slug);
        let play = h
            .get_by_role_and_label(Role::Button, "Play")
            .accesskit_node()
            .bounding_box()
            .unwrap();
        let settings = h
            .get_by_role_and_label(Role::Button, "Settings")
            .accesskit_node()
            .bounding_box()
            .unwrap();
        let tile = h
            .get_by_role_and_label(Role::RadioButton, GAMES[0].title)
            .accesskit_node()
            .bounding_box()
            .unwrap();
        // "Your games · 7 of 7 found" sits ~31 px above the tile row
        let shelf_top = tile.y0 - 31.0;
        assert!(
            play.y1 <= shelf_top && settings.y1 <= shelf_top,
            "{}: Play ({:.0}..{:.0}) / Settings ({:.0}..{:.0}) overlap shelf top {:.0}",
            g.slug,
            play.y0,
            play.y1,
            settings.y0,
            settings.y1,
            shelf_top
        );
    }
}

#[test]
fn a_game_missing_from_the_library_cannot_be_played() {
    let t = tempfile::tempdir().unwrap();
    let mut h = hub(t.path(), SIZE_720, false);
    select(&mut h, "src");
    assert!(says(&h, "Not found in your game folders"));
    assert!(row_disabled(&h, "Play"));
}

#[test]
fn a_chihiro_or_naomi_game_without_the_bios_says_so_on_its_page() {
    let t = tempfile::tempdir().unwrap();
    let mut h = hub(t.path(), SIZE_720, true);
    select(&mut h, "f355");
    assert!(says(&h, "Needs your BIOS folder"));
    select(&mut h, "sr3");
    assert!(!says(&h, "Needs your BIOS folder"));
    let t = tempfile::tempdir().unwrap();
    let mut h = window(t.path(), Some("library = /games\nbios = /bios\n"), SIZE_720);
    h.state_mut().set_library(library(true));
    select(&mut h, "f355");
    assert!(!says(&h, "Needs your BIOS folder"));
}

#[test]
fn play_runs_the_session_and_says_why_the_game_couldnt_start() {
    let t = tempfile::tempdir().unwrap();
    let mut h = hub(t.path(), SIZE_720, true);
    select(&mut h, "sr3");
    click_button(&mut h, "Play");
    testing::wait(&mut h, "the session's end", |a| a.message().is_some());
    let m = h.state().message().unwrap();
    assert_eq!(m.title, "SEGA Rally 3 couldn't start");
    assert!(
        m.body
            .contains("needs launcher components that are missing"),
        "{}",
        m.body
    );
    let log = std::fs::read_to_string(paths_in(t.path()).log()).unwrap();
    assert!(log.contains("needs launcher components"), "{log}");
}

// ------------------------------------------------------------ settings

#[test]
fn a_game_seen_for_the_first_time_gets_the_recommended_look_saved() {
    let t = tempfile::tempdir().unwrap();
    let mut h = hub(t.path(), SIZE_720, true);
    h.state_mut().set_screen(Some((1280, 800)));
    select(&mut h, "src");
    open_tab(&mut h, Tab::Settings);
    assert_eq!(row(&h, "Look"), "Steam Deck");
    let (s, _) = settings::read(&paths_in(t.path()).settings("src"))
        .unwrap()
        .unwrap();
    assert_eq!(s.look, Some(settings::Look::SteamDeck));
}

#[test]
fn a_row_change_is_saved_in_the_games_own_file() {
    let t = tempfile::tempdir().unwrap();
    let mut h = hub(t.path(), SIZE_720, true);
    select(&mut h, "sr3");
    open_tab(&mut h, Tab::Settings);
    click_button(&mut h, "Stick dead zone: next");
    let p = paths_in(t.path());
    let (sr3, _) = settings::read(&p.settings("sr3")).unwrap().unwrap();
    assert_eq!(sr3.deadzone, 6);
    assert!(says(&h, "Saved."));
    assert!(settings::read(&p.settings("src")).unwrap().is_none());
}

#[test]
fn rows_a_game_ignores_are_off_and_say_why() {
    let t = tempfile::tempdir().unwrap();
    let mut h = hub(t.path(), SIZE_720, true);
    select(&mut h, "or2sdx");
    open_tab(&mut h, Tab::Settings);
    assert_eq!(
        row_reason(&h, "Steering lock").as_deref(),
        Some(form::WHEEL_OWN)
    );
    open_tab(&mut h, Tab::Games);
    select(&mut h, "dcusa");
    open_tab(&mut h, Tab::Settings);
    assert_eq!(
        row_reason(&h, "Game resolution").as_deref(),
        Some(form::RENDER_FIXED)
    );
}

#[test]
fn the_gamescope_rows_are_shown_off_on_windows_not_hidden() {
    let t = tempfile::tempdir().unwrap();
    let mut h = hub(t.path(), SIZE_720, true);
    select(&mut h, "sr3");
    open_tab(&mut h, Tab::Settings);
    for label in ["Screen size", "Upscaler", "Full screen"] {
        assert_eq!(row_disabled(&h, label), cfg!(windows), "{label}");
    }
}

// ------------------------------------------------------------ controls

#[test]
fn the_pad_moves_through_the_hub_once_per_press() {
    let t = tempfile::tempdir().unwrap();
    let mut h = hub(t.path(), SIZE_720, true);
    let pad = |h: &mut Hub, bits: u16| {
        h.state_mut().backend_mut().fake.snapshot = Some(Snapshot {
            nav: bits,
            ..Snapshot::default()
        });
        h.run_steps(2);
    };
    assert_eq!(h.state().model().selected, 0);
    pad(&mut h, rea_launcher::pad::DPAD_RIGHT);
    pad(&mut h, rea_launcher::pad::DPAD_RIGHT);
    assert_eq!(h.state().model().selected, 1, "held: one step");
    pad(&mut h, 0);
    pad(&mut h, rea_launcher::pad::DPAD_RIGHT);
    assert_eq!(h.state().model().selected, 2);
}

#[test]
fn the_controls_page_shows_the_pad_layout_and_the_live_controls() {
    let mut snap = Snapshot {
        devices: vec![DeviceInfo {
            name: "Test Wheel".into(),
            kind: Kind::Wheel,
            haptic: true,
        }],
        driver: Some(0),
        ..Snapshot::default()
    };
    snap.controls.steer = -0.5;
    snap.controls.buttons = Button::View.bit();
    let v = controls_view(&snap);
    assert_eq!(v.devices[0].name, "Test Wheel");
    assert!(
        v.layout.contains(&("View change".into(), "Y".into())),
        "{:?}",
        v.layout
    );
    assert!(v.buttons.iter().any(|b| b.key == "view" && b.pressed));
    assert!(v.buttons.iter().any(|b| b.key == "start" && !b.pressed));
}

#[test]
fn every_control_fits_the_window_on_every_page() {
    for size in [SIZE_720, SIZE_800] {
        let t = tempfile::tempdir().unwrap();
        let mut h = hub(t.path(), size, true);
        let check = |h: &mut Hub, page: &str| {
            h.run_steps(3);
            let misfits = testing::misfits(h);
            assert!(misfits.is_empty(), "{page} at {size:?}: {misfits:#?}");
        };
        check(&mut h, "hub");
        open_tab(&mut h, Tab::Controls);
        check(&mut h, "controls");
        open_tab(&mut h, Tab::Settings);
        check(&mut h, "settings");
        click_button(&mut h, "Folders and BIOS");
        check(&mut h, "setup");
    }
}

// ------------------------------------------------------------ goldens

const GOLDEN_SIZES: [Vec2; 2] = [vec2(1280.0, 720.0), vec2(1280.0, 800.0)];

/// The window's AccessKit tree, one indented line per node.
fn dump(h: &Hub) -> String {
    fn walk(
        n: rea_launcher_ui::testing::kittest::AccessKitNode<'_>,
        depth: usize,
        out: &mut String,
    ) {
        if n.role() == Role::TextRun {
            return;
        }
        let _ = write!(out, "{:indent$}{:?}", "", n.role(), indent = depth * 2);
        if let Some(l) = n.label() {
            let _ = write!(out, " label={l:?}");
        }
        if let Some(v) = n.value() {
            let _ = write!(out, " value={v:?}");
        }
        if let Some(v) = n.numeric_value() {
            let _ = write!(out, " level={v}");
        }
        if let Some(d) = n.description() {
            let _ = write!(out, " description={d:?}");
        }
        if n.is_disabled() {
            out.push_str(" disabled");
        }
        if let Some(t) = n.toggled() {
            let _ = write!(out, " toggled={t:?}");
        }
        if let Some(b) = n.bounding_box() {
            let _ = write!(
                out,
                " [{} {} {} {}]",
                b.x0.round(),
                b.y0.round(),
                b.x1.round(),
                b.y1.round()
            );
        }
        out.push('\n');
        for c in n.children() {
            walk(c, depth + 1, out);
        }
    }
    let mut out = String::new();
    walk(h.root().accesskit_node(), 0, &mut out);
    out
}

/// Appends the window's dump under a heading naming its state.
fn section(out: &mut String, state: &str, h: &mut Hub) {
    h.run_steps(3);
    let _ = writeln!(out, "== {state}");
    out.push_str(&dump(h));
}

/// Compares with `tests/snapshots/ui/<host>-<w>x<h>/<page>.txt`.
fn golden(page: &str, size: Vec2, text: &str) {
    let host = match Host::current() {
        Host::Linux => "linux",
        Host::Windows => "windows",
    };
    let name = format!("ui/{host}-{}x{}/{page}.txt", size.x, size.y);
    rea_launcher::snapshot!(&name, text);
}

#[test]
fn golden_hub_states() {
    for size in GOLDEN_SIZES {
        let t = tempfile::tempdir().unwrap();
        let mut out = String::new();
        let mut h = window(t.path(), Some("library = /games\n"), size);
        h.state_mut().set_library(library(false));
        section(&mut out, "no game found", &mut h);
        h.state_mut().set_library(library(true));
        for g in GAMES {
            select(&mut h, g.slug);
            section(
                &mut out,
                &format!("every game found, {} selected", g.slug),
                &mut h,
            );
        }
        golden("hub", size, &out);
    }
}

#[test]
fn golden_settings_of_every_game_and_look() {
    for size in GOLDEN_SIZES {
        let t = tempfile::tempdir().unwrap();
        let mut h = hub(t.path(), size, true);
        let mut out = String::new();
        for g in GAMES {
            open_tab(&mut h, Tab::Games);
            select(&mut h, g.slug);
            open_tab(&mut h, Tab::Settings);
            section(
                &mut out,
                &format!("{} settings, first opened", g.slug),
                &mut h,
            );
        }
        let last = GAMES[GAMES.len() - 1].slug;
        for _ in settings::Look::ALL {
            click_button(&mut h, "Look: next");
            let look = row(&h, "Look");
            section(&mut out, &format!("{last} settings, look {look}"), &mut h);
        }
        golden("settings", size, &out);
    }
}

#[test]
fn golden_controls_states() {
    for size in GOLDEN_SIZES {
        let t = tempfile::tempdir().unwrap();
        let mut h = hub(t.path(), size, true);
        let mut out = String::new();
        open_tab(&mut h, Tab::Controls);
        section(&mut out, "no controller", &mut h);
        let mut controls = rea_launcher::wheel::Controls {
            steer: -0.5,
            accel: 0.25,
            brake: 0.75,
            buttons: 0,
        };
        controls.press(Button::Start);
        h.state_mut().backend_mut().fake.snapshot = Some(Snapshot {
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
            ..Snapshot::default()
        });
        section(&mut out, "a pad and a wheel, a game running", &mut h);
        h.state_mut().backend_mut().fake.snapshot = Some(Snapshot {
            error: Some("controllers unavailable: no SDL".into()),
            ..Snapshot::default()
        });
        section(&mut out, "controllers can't be read", &mut h);
        golden("controls", size, &out);
    }
}

#[test]
fn golden_setup_states() {
    for size in GOLDEN_SIZES {
        let t = tempfile::tempdir().unwrap();
        let mut out = String::new();
        let mut h = window(&t.path().join("first"), None, size);
        section(&mut out, "first run", &mut h);
        click_button(&mut h, "Add folder");
        section(&mut out, "first run, Add folder with nothing typed", &mut h);

        let config = "library = /games\nlibrary = /more games\nbios = /bios\n";
        let mut h = window(t.path(), Some(config), size);
        h.state_mut().set_library(library(true));
        open_tab(&mut h, Tab::Settings);
        click_button(&mut h, "Folders and BIOS");
        section(&mut out, "two folders and the BIOS folder", &mut h);
        click_button(&mut h, "Use BIOS");
        section(&mut out, "Use BIOS with nothing typed", &mut h);
        golden("setup", size, &out);
    }
}

/// What the test's session does next.
enum Step {
    Play,
    End,
}

#[test]
fn golden_running_overlay_and_messages() {
    for size in GOLDEN_SIZES {
        // a fixed, relative folder: the messages naming the logs wrap the
        // same way on every run and machine (a temporary folder's random
        // name moved the OK button)
        let t = PathBuf::from(format!(
            "../../target/golden-overlays-{}x{}",
            size.x, size.y
        ));
        let _ = std::fs::remove_dir_all(&t);
        let mut h = hub(&t, size, true);
        let mut out = String::new();
        select(&mut h, "sr3");
        // the dump shows the data folder as /data, spelled as the dump
        // spells it (Debug: a Windows `\` is `\\`)
        let data = format!("{:?}", display_path(&paths_in(&t).data));
        let data = data.trim_matches('"').to_string();
        let shown = |out: &mut String, state: &str, h: &mut Hub| {
            let mut s = String::new();
            section(&mut s, state, h);
            out.push_str(&s.replace(&data, "/data"));
        };
        for started in [false, true] {
            let (tx, rx) = mpsc::channel();
            h.state_mut().backend_mut().fake.session = Some(Box::new(move |r: &Reporter| {
                for step in rx {
                    match step {
                        Step::Play => r.playing(),
                        Step::End => break,
                    }
                }
                Err("starting gamescope (is it installed?).".into())
            }));
            click_button(&mut h, "Play");
            shown(&mut out, "getting the game ready", &mut h);
            if started {
                tx.send(Step::Play).unwrap();
                settle(&mut h);
                shown(&mut out, "the game running", &mut h);
                click_button(&mut h, "Stop");
                shown(&mut out, "Stop pressed", &mut h);
            }
            tx.send(Step::End).unwrap();
            testing::wait(&mut h, "the message", |a| a.message().is_some());
            shown(&mut out, &format!("an error, started: {started}"), &mut h);
            click_button(&mut h, "OK");
        }
        golden("overlays", size, &out);
    }
}

/// Frames for 300 ms: the session thread's news reaches the window.
fn settle(h: &mut Hub) {
    for _ in 0..30 {
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
