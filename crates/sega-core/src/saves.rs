//! Per-game progress, ranking, and cabinet NVRAM saves, mirrored between
//! `<data>/saves/<slug>/` (clean, backup-friendly, player-owned) and the
//! game's private runtime folder (`rea_launcher::saves::Mirror`).

use std::io;
use std::path::{Path, PathBuf};

use rea_launcher::catalog::Host;
use rea_launcher::saves::{Deleted, FirstRun, Mirror, Moved};

use crate::catalog::{Game, Route};

/// The player's backup-friendly saves folder for `slug` (`<data>/saves/<slug>`).
pub fn dir(data: &Path, slug: &str) -> PathBuf {
    data.join("saves").join(slug)
}

/// The folder inside `runtime` where `game` keeps its save files.
pub fn runtime_dir(game: &Game, runtime: &Path, host: Host) -> PathBuf {
    match game.route {
        Route::Flycast { .. } => match host {
            Host::Linux => runtime.join("data/flycast"),
            Host::Windows => runtime.join("flycast/data"),
        },
        _ => runtime.to_path_buf(),
    }
}

/// Whether `rel` (relative to [`runtime_dir`], `/` separators) is a save or
/// cabinet backup file of `game` that belongs in `saves/<slug>/`.
pub fn include(game: &Game, rel: &str) -> bool {
    let lower = rel.replace('\\', "/").to_ascii_lowercase();
    if lower.is_empty() || lower.ends_with(".log") || lower.ends_with(".dmp") {
        return false;
    }
    match game.route {
        Route::Lindbergh => {
            lower == "sram.bin"
                || lower == "eeprom.bin"
                || lower.starts_with("jennifer/rankingdata/")
                || lower.starts_with("jennifer/recordedata/")
        }
        Route::Wal { .. } => {
            lower.starts_with("appdata/")
                || lower.starts_with("amfs/")
                || lower.starts_with("shelldata/")
                || lower.starts_with("sega/gamedata/")
                || lower.starts_with("sega/shelldata/")
        }
        Route::Xemu { .. } => {
            !lower.contains('/') && (lower == "eeprom.bin" || lower.ends_with(".qcow2"))
        }
        Route::Flycast { .. } => {
            !lower.contains('/')
                && [".card", ".nvmem", ".eeprom", ".sram", ".bin"]
                    .iter()
                    .any(|ext| lower.ends_with(ext))
        }
    }
}

fn scan_dirs(game: &Game) -> &'static [&'static str] {
    match game.route {
        Route::Lindbergh => &["", "Jennifer/rankingdata", "Jennifer/recordedata"],
        Route::Wal { .. } => &[
            "AppData",
            "AMFS",
            "ShellData",
            "Sega/GameData",
            "Sega/ShellData",
        ],
        Route::Xemu { .. } | Route::Flycast { .. } => &[""],
    }
}

fn with_mirror<T>(
    game: &Game,
    runtime: &Path,
    saves_dir: &Path,
    host: Host,
    f: impl for<'a> FnOnce(&Mirror<'a>) -> io::Result<T>,
) -> io::Result<T> {
    let rt_dir = runtime_dir(game, runtime, host);
    let state = runtime.join(".saves-state");
    let keep = |rel: &str| include(game, rel);
    let mirror = Mirror::folder(&rt_dir, saves_dir, &state, FirstRun::SavesWin)
        .deleted(Deleted::Restore)
        .scan(scan_dirs(game))
        .only(&keep);
    f(&mirror)
}

/// Mirrors `game`'s saves before the game starts.
pub fn sync_before(game: &Game, runtime: &Path, saves_dir: &Path, host: Host) -> io::Result<Moved> {
    with_mirror(game, runtime, saves_dir, host, |m| m.before())
}

/// Copies save changes written by `game` back into `saves_dir`.
pub fn sync_after(
    game: &Game,
    runtime: &Path,
    saves_dir: &Path,
    host: Host,
) -> io::Result<Vec<String>> {
    with_mirror(game, runtime, saves_dir, host, |m| m.after())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{GameId, game};
    use std::fs;

    #[test]
    fn each_route_keeps_saves_and_skips_logs_and_assets() {
        let or2 = game(GameId::OutRun2SpSdx);
        assert!(include(or2, "sram.bin"));
        assert!(include(or2, "Jennifer/rankingdata/rank.dat"));
        assert!(!include(or2, "lindbergh.ini"));

        let sr3 = game(GameId::SegaRally3);
        assert!(include(sr3, "AppData/save.bin"));
        assert!(include(sr3, "ShellData/db.bin"));
        assert!(!include(sr3, "AppData/error.log"));

        let f355 = game(GameId::F355Challenge2);
        assert!(include(f355, "f355twn2.zip.eeprom"));
        assert!(!include(f355, "naomi.zip"));
    }

    #[test]
    fn round_trip_mirrors_saves_out_and_restores_player_backup() {
        let t = tempfile::tempdir().unwrap();
        let rt = t.path().join("runtimes/sr3");
        let sv = dir(t.path(), "sr3");
        fs::create_dir_all(rt.join("AppData")).unwrap();

        sync_before(game(GameId::SegaRally3), &rt, &sv, Host::Linux).unwrap();
        fs::write(rt.join("AppData/records.dat"), b"v1").unwrap();
        let after = sync_after(game(GameId::SegaRally3), &rt, &sv, Host::Linux).unwrap();
        assert_eq!(after, vec!["AppData/records.dat"]);
        assert_eq!(fs::read(sv.join("AppData/records.dat")).unwrap(), b"v1");

        fs::write(sv.join("AppData/records.dat"), b"backup").unwrap();
        let before = sync_before(game(GameId::SegaRally3), &rt, &sv, Host::Linux).unwrap();
        assert_eq!(before.to_runtime, vec!["AppData/records.dat"]);
        assert_eq!(fs::read(rt.join("AppData/records.dat")).unwrap(), b"backup");
    }
}
