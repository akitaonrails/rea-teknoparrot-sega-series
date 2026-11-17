//! The games, what runs each one, and the builds we know.
//!
//! An edition is recognised by the SHA-256 of its main file. Only builds
//! listed here are launched; anything else is reported with its digest so
//! the player can file it (STANDARDS: "Exact binary identity"). The table's
//! types and the library scan are the SDK's (`rea_launcher::catalog`,
//! `rea_launcher::library`); the games, their routes and their rules are here.

use rea_launcher::catalog::Title;
pub use rea_launcher::catalog::{Edition, Host, Support};
use rea_launcher::gamescope::Size;
use rea_launcher::library::{HashCache, Library};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GameId {
    OutRun2SpSdx,
    OutRun2Sp,
    CrazyTaxi,
    SegaRally3,
    RacingClassic,
    Daytona,
    F355Challenge2,
}

/// How a game is run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Sega Lindbergh Linux build, natively, with lindbergh-loader preloaded.
    Lindbergh,
    /// Windows build under Wine with windowsarcadeloader's payload for this
    /// system profile (`systemprofiles/<system>/<profile>.yaml`).
    Wal {
        system: &'static str,
        profile: &'static str,
    },
    /// Sega Chihiro title (an Xbox `.xbe`) in Cxbx-Reloaded.
    Cxbx,
    /// Sega NAOMI cartridge (a MAME set, here unzipped) in Flycast.
    Flycast { set: &'static str },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Game {
    pub id: GameId,
    /// Stable id for files and the command line.
    pub slug: &'static str,
    pub title: &'static str,
    /// Short name on a tile.
    pub short: &'static str,
    pub year: u16,
    pub board: &'static str,
    pub route: Route,
    /// The file that identifies the edition, relative to the game folder.
    pub main_file: &'static str,
    pub editions: &'static [Edition],
    /// The size the cabinet's screen had.
    pub native: Size,
    pub support: Support,
}

impl Game {
    /// Whether the game can start on `host`: the Sega Lindbergh games are
    /// Linux programs (lindbergh-loader preloads itself into them).
    pub fn support_on(&self, host: Host) -> Support {
        match (host, self.route) {
            (Host::Windows, Route::Lindbergh) => Support::Blocked(
                "A Sega Lindbergh game: its Linux build runs in the Linux version of the launcher.",
            ),
            _ => self.support,
        }
    }

    /// The folder of the dump whose entries make up the runtime, relative
    /// to the game folder. OutRun 2 SP SDX's dump is the cabinet's disks:
    /// the game reads `/home/disk0`, which lindbergh-loader turns into
    /// `..` from the game's own folder (`disk0/Jennifer`).
    pub fn source_folder(&self) -> &'static str {
        match self.id {
            GameId::OutRun2SpSdx => "or2g/disk0",
            _ => "",
        }
    }

    /// The folder inside the runtime the loader starts the game in.
    pub fn start_folder(&self) -> &'static str {
        match self.id {
            GameId::OutRun2SpSdx => "Jennifer",
            _ => "",
        }
    }

    /// The main file inside the runtime (the runtime mirrors
    /// [`Game::source_folder`]).
    pub fn runtime_main_file(&self) -> &'static str {
        let src = self.source_folder();
        if src.is_empty() {
            self.main_file
        } else {
            self.main_file
                .strip_prefix(src)
                .map(|f| f.trim_start_matches('/'))
                .unwrap_or(self.main_file)
        }
    }

    /// The program for windowsarcadeloader's `.windowsloader` file, a
    /// Windows path relative to the runtime.
    pub fn loader_path(&self) -> String {
        self.runtime_main_file().replace('/', "\\")
    }
}

impl Title for Game {
    type Id = GameId;
    fn id(&self) -> GameId {
        self.id
    }
    fn main_file(&self) -> &'static str {
        self.main_file
    }
    fn editions(&self) -> &'static [Edition] {
        self.editions
    }
}

pub const GAMES: &[Game] = &[
    Game {
        id: GameId::OutRun2SpSdx,
        slug: "or2sdx",
        title: "OutRun 2 SP SDX",
        short: "OR2SDX",
        year: 2006,
        board: "Sega Lindbergh",
        route: Route::Lindbergh,
        main_file: "or2g/disk0/Jennifer/Jennifer",
        editions: &[Edition::sha256(
            "f3c4918a268c918b125793bfc1307d6523a8028bda523e8807707229d6bcbff5",
            "1.003 (DVP-0015, TeknoParrot dump)",
        )],
        native: Size::new(800, 480),
        support: Support::Ready,
    },
    Game {
        id: GameId::OutRun2Sp,
        slug: "or2sp",
        title: "OutRun 2 SP Special Tours",
        short: "OR2 SP",
        year: 2004,
        board: "Sega Chihiro",
        route: Route::Cxbx,
        main_file: "outrun2.xbe",
        editions: &[Edition::sha256(
            "0e177363ae40c8beedcde1f7a7a5ccd75c962f0bd495f44932a9d50683a170a7",
            "Japan (TeknoParrot dump)",
        )],
        native: Size::new(640, 480),
        support: Support::Blocked("Needs the Chihiro emulator, which is still being brought up."),
    },
    Game {
        id: GameId::CrazyTaxi,
        slug: "cthr",
        title: "Crazy Taxi High Roller",
        short: "CT HR",
        year: 2003,
        board: "Sega Chihiro",
        route: Route::Cxbx,
        main_file: "ctx_ac[r].xbe",
        editions: &[Edition::sha256(
            "fca9dddb942ddb190b2faf5fe58ebdb5a77a570bb235334cc22aee159ed9dc2f",
            "3.000 Japan (TeknoParrot dump)",
        )],
        native: Size::new(640, 480),
        support: Support::Blocked("Needs the Chihiro emulator, which is still being brought up."),
    },
    Game {
        id: GameId::SegaRally3,
        slug: "sr3",
        title: "SEGA Rally 3",
        short: "SR3",
        year: 2008,
        board: "Sega Europa-R",
        route: Route::Wal {
            system: "europar",
            profile: "sega-rally-3",
        },
        main_file: "Rally/Rally.exe",
        editions: &[Edition::sha256(
            "8eb7bdf01b8dc30ba40a43ac3d4065b9035cc59d58dd01b3c03113a1403171b5",
            "Europa-R (TeknoParrot dump)",
        )],
        native: Size::new(1280, 720),
        support: Support::Ready,
    },
    Game {
        id: GameId::RacingClassic,
        slug: "src",
        title: "SEGA Racing Classic",
        short: "SRC",
        year: 2009,
        board: "Sega RingWide",
        route: Route::Wal {
            system: "ringedge",
            profile: "sega-racing-classic",
        },
        main_file: "d1a.exe",
        editions: &[Edition::sha256(
            "0926a4880b386367a2e0902afbcc87d95e1b9ff42b3709c1decbac32f5fd0644",
            "1.01.00 (TeknoParrot dump)",
        )],
        native: Size::new(1280, 720),
        support: Support::Ready,
    },
    Game {
        id: GameId::Daytona,
        slug: "dcusa",
        title: "Daytona Championship USA",
        short: "DCUSA",
        year: 2017,
        board: "Sega PC (Shell)",
        route: Route::Wal {
            system: "segashell",
            profile: "daytona-championship-usa",
        },
        main_file: "Sega/Daytona/Daytona.exe",
        editions: &[Edition::sha256(
            "8ba4048f9a6920e3ecdc991059301e17c53c3c605c0cc639454c14d2665d379e",
            "New Season Edition (TeknoParrot dump)",
        )],
        native: Size::new(1920, 1080),
        support: Support::Ready,
    },
    Game {
        id: GameId::F355Challenge2,
        slug: "f355",
        title: "F355 Challenge 2",
        short: "F355",
        year: 2001,
        board: "Sega NAOMI",
        route: Route::Flycast { set: "f355twn2" },
        main_file: "epr-23399.ic22",
        editions: &[Edition::sha256(
            "a01e88c2621f8a70fe0161ef26fed9d16b0890e038464628e473336af37de9e7",
            "International Course Edition, twin (MAME f355twn2)",
        )],
        native: Size::new(640, 480),
        support: Support::Unverified(
            "Runs in Flycast as two linked boards in one window. When the boards fail \
             to find each other at boot, the launcher starts the game again by itself. \
             Wheels aren't set up for it yet.",
        ),
    },
];

pub fn game(id: GameId) -> &'static Game {
    GAMES
        .iter()
        .find(|g| g.id == id)
        .expect("every id is in GAMES")
}

pub fn by_slug(slug: &str) -> Option<&'static Game> {
    GAMES.iter().find(|g| g.slug == slug)
}

/// The library scan over this table: one per launcher, its clones share
/// `cache` (a rescan reuses the digests).
pub fn library(cache: HashCache) -> Library<Game> {
    Library::new(GAMES).with_cache(cache)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_slugs_and_digests_are_unique_and_well_formed() {
        assert_eq!(rea_launcher::catalog::check_table(GAMES), Ok(()));
        let slugs: HashSet<_> = GAMES.iter().map(|g| g.slug).collect();
        assert_eq!(slugs.len(), GAMES.len());
    }

    #[test]
    fn identify_finds_the_right_game_in_any_case() {
        let digest = "8EB7BDF01B8DC30BA40A43AC3D4065B9035CC59D58DD01B3C03113A1403171B5";
        let (g, _) = rea_launcher::catalog::identify(GAMES, digest).unwrap();
        assert_eq!(g.id, GameId::SegaRally3);
        assert!(rea_launcher::catalog::identify(GAMES, "00").is_none());
    }

    /// The owner's real library, when present
    /// (`SEGA_LIBRARIES=dir1:dir2 cargo test -- --ignored`).
    #[test]
    #[ignore]
    fn real_library_finds_every_game() {
        let Ok(roots) = std::env::var("SEGA_LIBRARIES") else {
            return;
        };
        let roots: Vec<std::path::PathBuf> = std::env::split_paths(&roots).collect();
        let scan = library(HashCache::default()).scan(&roots);
        assert_eq!(scan.by_game().len(), GAMES.len(), "{scan:#?}");
    }

    #[test]
    fn windowsarcadeloader_gets_windows_paths_relative_to_the_runtime() {
        assert_eq!(game(GameId::SegaRally3).loader_path(), "Rally\\Rally.exe");
        assert_eq!(game(GameId::RacingClassic).loader_path(), "d1a.exe");
    }

    #[test]
    fn outrun_sdx_runs_from_its_jennifer_folder() {
        let g = game(GameId::OutRun2SpSdx);
        assert_eq!(g.source_folder(), "or2g/disk0");
        assert_eq!(g.runtime_main_file(), "Jennifer/Jennifer");
        assert!(g.runtime_main_file().starts_with(g.start_folder()));
        assert!(g.main_file.starts_with(g.source_folder()));
        for g in GAMES.iter().filter(|g| g.source_folder().is_empty()) {
            assert_eq!(g.runtime_main_file(), g.main_file);
        }
    }

    #[test]
    fn the_lindbergh_games_are_linux_only() {
        for g in GAMES {
            let windows = g.support_on(Host::Windows);
            assert_eq!(g.support_on(Host::Linux), g.support, "{}", g.slug);
            if matches!(g.route, Route::Lindbergh) {
                assert!(matches!(windows, Support::Blocked(_)), "{}", g.slug);
            } else {
                assert_eq!(windows, g.support, "{}", g.slug);
            }
        }
    }
}
