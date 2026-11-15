//! The games, what runs each one, and the builds we know.
//!
//! An edition is recognised by the SHA-256 of its main file. Only builds
//! listed here are launched; anything else is reported with its digest so
//! the player can file it (STANDARDS: "Exact binary identity").

use crate::settings::Size;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GameId {
    OutRun2SpSdx,
    OutRun2Sp,
    CrazyTaxi,
    SegaRally3,
    RacingClassic,
    Daytona,
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
    /// Windows build under Wine with TeknoParrot's own loader
    /// (OpenParrotLoader + `TeknoParrot.dll`), fed by the launcher's I/O
    /// helper through TeknoParrot's shared memory.
    TeknoParrot,
    /// Sega Chihiro title (an Xbox `.xbe`) in Cxbx-Reloaded.
    Cxbx,
}

/// Whether the launcher can start a game today, and why not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    /// Verified in the real game (a race played), see docs/RUNTIME_NOTES.md.
    Ready,
    /// Starts, but no race was verified yet; the text says what is known.
    Unverified(&'static str),
    /// Known not to work yet; the text says why, for the player.
    Blocked(&'static str),
}

/// The system the launcher runs on: what it can start depends on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Host {
    Linux,
    Windows,
}

impl Host {
    pub const fn current() -> Host {
        if cfg!(windows) {
            Host::Windows
        } else {
            Host::Linux
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edition {
    /// SHA-256 of the main file.
    pub sha256: &'static str,
    pub label: &'static str,
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

const fn sha(d: &'static str, label: &'static str) -> Edition {
    Edition { sha256: d, label }
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
        editions: &[sha(
            "f3c4918a268c918b125793bfc1307d6523a8028bda523e8807707229d6bcbff5",
            "1.003 (DVP-0015, TeknoParrot dump)",
        )],
        native: Size::new(800, 480),
        support: Support::Unverified(
            "Starts and shows its attract mode; a race was not tried yet.",
        ),
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
        editions: &[sha(
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
        editions: &[sha(
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
        editions: &[sha(
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
        editions: &[sha(
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
        route: Route::TeknoParrot,
        main_file: "Sega/Daytona/Daytona.exe",
        editions: &[sha(
            "8ba4048f9a6920e3ecdc991059301e17c53c3c605c0cc639454c14d2665d379e",
            "New Season Edition (TeknoParrot dump)",
        )],
        native: Size::new(1920, 1080),
        support: Support::Blocked("Its loader is still being written."),
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

/// The game and edition a digest belongs to.
pub fn identify(digest: &str) -> Option<(&'static Game, &'static Edition)> {
    GAMES.iter().find_map(|g| {
        g.editions
            .iter()
            .find(|e| e.sha256.eq_ignore_ascii_case(digest))
            .map(|e| (g, e))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_slugs_and_digests_are_unique_and_well_formed() {
        let ids: HashSet<_> = GAMES.iter().map(|g| g.id).collect();
        let slugs: HashSet<_> = GAMES.iter().map(|g| g.slug).collect();
        assert_eq!(ids.len(), GAMES.len());
        assert_eq!(slugs.len(), GAMES.len());
        let mut digests = HashSet::new();
        for g in GAMES {
            for e in g.editions {
                assert_eq!(e.sha256.len(), 64, "{}", g.slug);
                assert!(e.sha256.chars().all(|c| c.is_ascii_hexdigit()));
                assert!(digests.insert(e.sha256), "duplicate digest {}", e.sha256);
            }
        }
    }

    #[test]
    fn identify_finds_the_right_game_in_any_case() {
        let (g, _) =
            identify("8EB7BDF01B8DC30BA40A43AC3D4065B9035CC59D58DD01B3C03113A1403171B5").unwrap();
        assert_eq!(g.id, GameId::SegaRally3);
        assert!(identify("00").is_none());
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
            if g.route == Route::Lindbergh {
                assert!(matches!(windows, Support::Blocked(_)), "{}", g.slug);
            } else {
                assert_eq!(windows, g.support, "{}", g.slug);
            }
        }
    }
}
