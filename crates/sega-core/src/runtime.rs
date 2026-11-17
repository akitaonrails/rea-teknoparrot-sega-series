//! The private runtime folder each game runs from.
//!
//! The player's game folder is read-only (it may be on a network share,
//! and the launcher runs every game with the library bound read-only).
//! The runtime holds a link to each top-level entry of the game folder,
//! plus the files the launcher writes and the folders the loaders create
//! (saves, cabinet drives) as real files. The SDK's `linkfarm` builds it
//! (links, junctions on Windows, writable copies); [`Sega`] is what this
//! launcher knows about each game's folder, and [`plan`] adds F355's
//! cartridge, packed into one stored zip.

use std::io;
use std::path::Path;

use rea_launcher::linkfarm::{self, Item, Op, Place, Policy};

use crate::catalog::{Game, GameId, Route};

/// Files below this size are copied, not linked: the games write their
/// logs, settings and saves next to their data (SEGA Rally 3's
/// `config.ini`, OutRun's ranking files), and the library is read-only.
/// Lindbergh programs are extensionless ELFs of a few MB, so the Linux
/// games copy only what is smaller than any of them.
fn small(route: Route) -> u64 {
    match route {
        Route::Lindbergh => 1024 * 1024,
        _ => 4 * 1024 * 1024,
    }
}

fn copied(route: Route, it: &Item) -> bool {
    let lower = it.name.to_ascii_lowercase();
    let code = [".exe", ".dll", ".manifest", ".so", ".xbe", ".asi"]
        .iter()
        .any(|x| lower.ends_with(x))
        || lower.contains(".so.");
    !it.dir && it.size < small(route) && !code
}

/// Names at the top of the game folder never linked: files the launcher
/// or the loaders write in the runtime, and NTFS leftovers in the dumps.
fn skip(route: Route, name: &str) -> bool {
    if linkfarm::ntfs_leftover(name) {
        return true;
    }
    match route {
        Route::Lindbergh => matches!(
            name,
            "lindbergh.ini" | "controls.ini" | "sram.bin" | "eeprom.bin"
        ),
        // windowsarcadeloader's dump marker and the cabinet drives it creates
        Route::Wal { .. } => {
            name.ends_with(".windowsloader")
                || matches!(name, "AMFS" | "amfs" | "AppData" | "Y" | "WindowsLoader")
        }
        // TeknoParrot's own settings file, of no use to Cxbx
        Route::Cxbx => name.eq_ignore_ascii_case("teknoparrot.ini"),
        Route::Flycast { .. } => false,
    }
}

/// Folders of the dump (relative to [`Game::source_folder`]) the game
/// writes into: the runtime holds them as real folders whose entries are
/// linked or copied one by one, so the game's writes stay in the runtime.
/// Every other folder is one link.
pub fn opened(game: &Game) -> &'static [&'static str] {
    match game.id {
        // config.ini, logs and the arcade database beside the program
        GameId::SegaRally3 => &["Rally", "ShellData", "Shell"],
        // the Shell's and the game's settings, logs and bookkeeping
        GameId::Daytona => &[
            "Sega",
            "Sega/Daytona",
            "Sega/GameData",
            "Sega/ShellData",
            "Sega/shell",
            "Sega/Launcher",
        ],
        GameId::RacingClassic => &["rom"],
        // the game's folder (its settings files beside the program) and the
        // ranking and replay folders it empties and rewrites
        GameId::OutRun2SpSdx => &["Jennifer", "Jennifer/rankingdata", "Jennifer/recordedata"],
        _ => &[],
    }
}

/// What the launcher knows about one game's folder.
pub struct Sega<'a>(pub &'a Game);

impl Policy for Sega<'_> {
    fn place(&self, rel: &str, it: &Item) -> Place {
        let route = self.0.route;
        if !rel.contains('/') && skip(route, rel) {
            Place::Skip
        } else if it.dir && opened(self.0).iter().any(|o| o.eq_ignore_ascii_case(rel)) {
            Place::Open
        } else if copied(route, it) {
            Place::CopyOnce
        } else {
            Place::Link
        }
    }
}

/// The runtime `rt` of a game whose dump is `dump`: every entry of its
/// source folder linked, small files copied, the folders of [`opened`]
/// made real; a NAOMI cartridge is its files packed into one archive,
/// which Flycast reads. `ls` lists a folder of the dump
/// ([`linkfarm::list`], or a fake in tests).
pub fn plan(
    game: &Game,
    dump: &Path,
    rt: &Path,
    ls: &dyn Fn(&Path) -> io::Result<Vec<Item>>,
) -> io::Result<Vec<Op>> {
    let src = dump.join(game.source_folder());
    let Route::Flycast { set } = game.route else {
        return linkfarm::plan(&src, rt, &Sega(game), ls);
    };
    let files = ls(&src)?
        .into_iter()
        .filter(|it| !it.dir && !skip(game.route, &it.name))
        .map(|it| src.join(it.name))
        .collect();
    Ok(vec![
        Op::Dir(rt.to_path_buf()),
        Op::Archive {
            files,
            to: rt.join(format!("{set}.zip")),
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::game;
    use rea_launcher::linkfarm::{Occupied, apply, list};
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;

    fn item(name: &str, dir: bool, size: u64) -> Item {
        Item {
            name: name.into(),
            dir,
            size,
        }
    }

    /// A fake dump: folder → entries.
    fn fake(tree: &[(&str, Vec<Item>)]) -> impl Fn(&Path) -> io::Result<Vec<Item>> + use<> {
        let map: HashMap<PathBuf, Vec<Item>> = tree
            .iter()
            .map(|(d, v)| (PathBuf::from(d), v.clone()))
            .collect();
        move |p: &Path| {
            map.get(p)
                .cloned()
                .ok_or_else(|| io::Error::other(format!("not listed: {}", p.display())))
        }
    }

    fn linked(ops: &[Op]) -> Vec<String> {
        ops.iter()
            .filter_map(|o| match o {
                Op::Link { at, .. } => Some(at.to_string_lossy().replace('\\', "/")),
                _ => None,
            })
            .collect()
    }

    fn copied_to(ops: &[Op]) -> Vec<String> {
        ops.iter()
            .filter_map(|o| match o {
                Op::CopyOnce { to, .. } => Some(to.to_string_lossy().replace('\\', "/")),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn sega_rally_writes_beside_its_program_so_that_folder_is_opened() {
        let ls = fake(&[
            (
                "/lib/sr3",
                vec![
                    item("Rally", true, 0),
                    item("Shell", true, 0),
                    item("ShellData", true, 0),
                    item("old.windowsloader", false, 10),
                    item("AppData", true, 0),
                ],
            ),
            (
                "/lib/sr3/Rally",
                vec![
                    item("Rally.exe", false, 9 << 20),
                    item("config.ini", false, 900),
                    item("Main_release", true, 0),
                    item("fmodex.dll", false, 400_000),
                ],
            ),
            ("/lib/sr3/Shell", vec![item("inpout32.dll", false, 50_000)]),
            ("/lib/sr3/ShellData", vec![item("Game.ini", false, 300)]),
        ]);
        let ops = plan(
            game(GameId::SegaRally3),
            Path::new("/lib/sr3"),
            Path::new("/rt/sr3"),
            &ls,
        )
        .unwrap();
        assert!(ops.contains(&Op::Dir("/rt/sr3/Rally".into())));
        let l = linked(&ops);
        assert!(l.contains(&"/rt/sr3/Rally/Rally.exe".into()));
        assert!(l.contains(&"/rt/sr3/Rally/Main_release".into()));
        assert!(
            l.contains(&"/rt/sr3/Rally/fmodex.dll".into()),
            "code is linked"
        );
        assert!(l.contains(&"/rt/sr3/Shell/inpout32.dll".into()));
        let c = copied_to(&ops);
        assert!(c.contains(&"/rt/sr3/Rally/config.ini".into()));
        assert!(c.contains(&"/rt/sr3/ShellData/Game.ini".into()));
        // the loader's own marker and cabinet drives are never taken from the dump
        assert!(
            !l.iter()
                .chain(&c)
                .any(|p| p.contains("windowsloader") || p.ends_with("AppData"))
        );
    }

    #[test]
    fn outrun_sdx_runtime_is_its_disk_with_the_game_folder_writable() {
        let ls = fake(&[
            (
                "/lib/or2/or2g/disk0",
                vec![
                    item("Jennifer", true, 0),
                    item("JenTest", true, 0),
                    item("drv_reload", false, 4844),
                ],
            ),
            (
                "/lib/or2/or2g/disk0/Jennifer",
                vec![
                    item("Jennifer", false, 2_729_110),
                    item("ranking_data.bin", false, 210_008),
                    item("media", true, 0),
                    item("rankingdata", true, 0),
                ],
            ),
            (
                "/lib/or2/or2g/disk0/Jennifer/rankingdata",
                vec![item("ranking_time_data.bin", false, 5000)],
            ),
        ]);
        let ops = plan(
            game(GameId::OutRun2SpSdx),
            Path::new("/lib/or2"),
            Path::new("/rt/or2"),
            &ls,
        )
        .unwrap();
        let l = linked(&ops);
        assert!(l.contains(&"/rt/or2/JenTest".into()));
        assert!(
            l.contains(&"/rt/or2/Jennifer/Jennifer".into()),
            "the program is linked"
        );
        assert!(l.contains(&"/rt/or2/Jennifer/media".into()));
        let c = copied_to(&ops);
        assert!(c.contains(&"/rt/or2/Jennifer/ranking_data.bin".into()));
        assert!(c.contains(&"/rt/or2/Jennifer/rankingdata/ranking_time_data.bin".into()));
        assert!(ops.contains(&Op::Dir("/rt/or2/Jennifer/rankingdata".into())));
    }

    #[test]
    fn a_folder_that_cannot_be_read_is_an_error_not_a_silent_gap() {
        let ls = fake(&[("/lib/src", vec![item("rom", true, 0)])]);
        let err = plan(
            game(GameId::RacingClassic),
            Path::new("/lib/src"),
            Path::new("/rt/src"),
            &ls,
        );
        assert!(err.is_err());
    }

    #[test]
    #[cfg(unix)]
    fn small_files_are_copied_once_and_then_belong_to_the_game() {
        let t = tempfile::tempdir().unwrap();
        let (src, rt) = (t.path().join("dump"), t.path().join("rt"));
        fs::create_dir_all(src.join("rom")).unwrap();
        fs::write(src.join("d1a.ini"), b"old").unwrap();
        fs::write(src.join("d1a.exe"), b"exe").unwrap();
        fs::write(src.join("rom/SBTN_Table.dat"), b"tbl").unwrap();
        let ops = plan(game(GameId::RacingClassic), &src, &rt, &list).unwrap();
        // an older launcher linked the ini: replaced by a copy
        fs::create_dir_all(&rt).unwrap();
        std::os::unix::fs::symlink(src.join("d1a.ini"), rt.join("d1a.ini")).unwrap();
        apply(&ops, Occupied::Refuse).unwrap();
        let ini = rt.join("d1a.ini");
        assert!(!fs::symlink_metadata(&ini).unwrap().file_type().is_symlink());
        assert!(
            fs::symlink_metadata(rt.join("d1a.exe"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            rt.join("rom").is_dir()
                && !fs::symlink_metadata(rt.join("rom"))
                    .unwrap()
                    .file_type()
                    .is_symlink()
        );
        fs::write(&ini, b"the game's").unwrap();
        apply(&ops, Occupied::Refuse).unwrap();
        assert_eq!(fs::read(&ini).unwrap(), b"the game's");
        assert_eq!(fs::read(src.join("d1a.ini")).unwrap(), b"old");
    }

    #[test]
    fn f355_runtime_is_its_set_packed_into_one_archive_made_once() {
        let dump = tempfile::tempdir().unwrap();
        let rt = tempfile::tempdir().unwrap();
        let rt = rt.path().join("f355");
        fs::write(dump.path().join("epr-23399.ic22"), b"program").unwrap();
        fs::write(dump.path().join("f355twn2-key.bin"), b"key!").unwrap();
        fs::create_dir(dump.path().join("notes")).unwrap();
        let g = game(GameId::F355Challenge2);
        let ops = plan(g, dump.path(), &rt, &list).unwrap();
        let zip = rt.join("f355twn2.zip");
        assert_eq!(
            ops,
            vec![
                Op::Dir(rt.clone()),
                Op::Archive {
                    files: vec![
                        dump.path().join("epr-23399.ic22"),
                        dump.path().join("f355twn2-key.bin")
                    ],
                    to: zip.clone(),
                }
            ]
        );
        apply(&ops, Occupied::Refuse).unwrap();
        let first = fs::metadata(&zip).unwrap().modified().unwrap();
        // unchanged dump: the archive stays as it is
        std::thread::sleep(std::time::Duration::from_millis(20));
        apply(&ops, Occupied::Refuse).unwrap();
        assert_eq!(fs::metadata(&zip).unwrap().modified().unwrap(), first);
        // a chip replaced by one of another size: made again
        fs::write(dump.path().join("epr-23399.ic22"), b"program, v2").unwrap();
        apply(&ops, Occupied::Refuse).unwrap();
        let bytes = fs::read(&zip).unwrap();
        assert!(bytes.windows(11).any(|w| w == b"program, v2"));
        // the dump itself is untouched
        assert_eq!(fs::read_dir(dump.path()).unwrap().count(), 3);
    }
}
