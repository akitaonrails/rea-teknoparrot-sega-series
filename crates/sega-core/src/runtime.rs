//! The private runtime folder each game runs from.
//!
//! The player's game folder is read-only (it may be on a network share,
//! and the launcher runs every game with the library bound read-only).
//! The runtime holds a symlink to each top-level entry of the game folder,
//! plus the files the launcher writes and the folders the loaders create
//! (saves, cabinet drives) as real files. [`plan`] decides from a listing;
//! [`apply`] carries it out without ever touching the game folder.
//!
//! On Windows, where a symlink needs administrator rights or developer
//! mode, a linked folder is a directory junction (a directory symlink for a
//! folder on a network share, which a junction cannot reach) and a linked
//! file is a copy, refreshed when the game folder's file changes.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::catalog::{Game, Route};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Dir(PathBuf),
    /// Symlink `at` → `target` (replaced when it points elsewhere).
    Link {
        target: PathBuf,
        at: PathBuf,
    },
    /// A copy the game owns from then on: made once (replacing a link an
    /// older launcher left), never overwritten.
    CopyOnce {
        from: PathBuf,
        to: PathBuf,
    },
}

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

/// One entry of the game folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub name: String,
    pub dir: bool,
    pub size: u64,
}

pub fn list(dir: &Path) -> io::Result<Vec<Item>> {
    let mut v = Vec::new();
    for e in fs::read_dir(dir)? {
        let e = e?;
        let m = fs::metadata(e.path())?;
        v.push(Item {
            name: e.file_name().to_string_lossy().into_owned(),
            dir: m.is_dir(),
            size: m.len(),
        });
    }
    v.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(v)
}

/// Names never linked: files the launcher or the loaders write in the
/// runtime, and NTFS leftovers in the dumps.
fn skip(route: Route, name: &str) -> bool {
    if name == "System Volume Information" || name.starts_with('$') {
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
        // TeknoParrot's own settings file, which the launcher writes
        Route::TeknoParrot | Route::Cxbx => name.eq_ignore_ascii_case("teknoparrot.ini"),
    }
}

/// Folders of the dump (relative to [`Game::source_folder`]) the game
/// writes into: the runtime holds them as real folders whose entries are
/// linked or copied one by one, so the game's writes stay in the runtime.
/// Every other folder is one link.
pub fn opened(game: &Game) -> &'static [&'static str] {
    use crate::catalog::GameId;
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

/// The runtime of a game: every entry of its source folder linked, small
/// files copied, the folders of [`opened`] made real. `ls` lists a folder
/// of the dump (the real [`list`], or a fake in tests).
pub fn plan(
    game: &Game,
    dump: &Path,
    rt: &Path,
    ls: &dyn Fn(&Path) -> io::Result<Vec<Item>>,
) -> io::Result<Vec<Op>> {
    let src = dump.join(game.source_folder());
    let mut ops = vec![Op::Dir(rt.to_path_buf())];
    let open = opened(game);
    let mut stack = vec![String::new()];
    while let Some(rel) = stack.pop() {
        let (from_dir, to_dir) = if rel.is_empty() {
            (src.clone(), rt.to_path_buf())
        } else {
            (src.join(&rel), rt.join(&rel))
        };
        if !rel.is_empty() {
            ops.push(Op::Dir(to_dir.clone()));
        }
        for it in ls(&from_dir)? {
            if rel.is_empty() && skip(game.route, &it.name) {
                continue;
            }
            let child = if rel.is_empty() {
                it.name.clone()
            } else {
                format!("{rel}/{}", it.name)
            };
            if it.dir && open.iter().any(|o| o.eq_ignore_ascii_case(&child)) {
                stack.push(child);
                continue;
            }
            let (from, to) = (from_dir.join(&it.name), to_dir.join(&it.name));
            if copied(game.route, &it) {
                ops.push(Op::CopyOnce { from, to });
            } else {
                ops.push(Op::Link {
                    target: from,
                    at: to,
                });
            }
        }
    }
    Ok(ops)
}

/// Carry out a plan. A link is replaced only when it points elsewhere; a
/// real file or folder where a link should be is an error (it would be
/// the player's data), never removed.
pub fn apply(ops: &[Op]) -> io::Result<()> {
    for op in ops {
        match op {
            Op::Dir(d) => fs::create_dir_all(d)?,
            Op::Link { target, at } => link(target, at)?,
            Op::CopyOnce { from, to } => match fs::symlink_metadata(to) {
                Ok(m) if !m.file_type().is_symlink() => {}
                found => {
                    if found.is_ok() {
                        fs::remove_file(to)?;
                    }
                    fs::copy(from, to)?;
                }
            },
        }
    }
    Ok(())
}

fn real_where_a_link_goes(at: &Path) -> io::Error {
    io::Error::other(format!(
        "{} is a real file or folder where the launcher expects a link; move it away",
        at.display()
    ))
}

/// A symlink `at` → `target`, replaced when it points elsewhere.
#[cfg(unix)]
fn link(target: &Path, at: &Path) -> io::Result<()> {
    match fs::symlink_metadata(at) {
        Ok(m) if m.file_type().is_symlink() => {
            if fs::read_link(at)? != *target {
                fs::remove_file(at)?;
                std::os::unix::fs::symlink(target, at)?;
            }
            Ok(())
        }
        Ok(_) => Err(real_where_a_link_goes(at)),
        Err(_) => std::os::unix::fs::symlink(target, at),
    }
}

/// Windows: a folder is a directory junction, a file a copy kept in step.
#[cfg(windows)]
fn link(target: &Path, at: &Path) -> io::Result<()> {
    let existing = fs::symlink_metadata(at).ok();
    if target.is_dir() {
        match existing {
            Some(m) if m.file_type().is_symlink() => {
                if junction::get_target(at).ok().as_deref() == Some(target) {
                    return Ok(());
                }
                remove_link(at)?;
            }
            Some(_) => return Err(real_where_a_link_goes(at)),
            None => {}
        }
        junction::create(target, at).or_else(|e| {
            std::os::windows::fs::symlink_dir(target, at).map_err(|_| {
                io::Error::new(
                    e.kind(),
                    format!(
                        "cannot link {} into the runtime folder ({e}); a game folder on a \
                         network share needs Windows developer mode",
                        target.display()
                    ),
                )
            })
        })
    } else {
        match existing {
            Some(m) if m.file_type().is_symlink() => remove_link(at)?,
            Some(m) if m.is_dir() => return Err(real_where_a_link_goes(at)),
            _ => {}
        }
        mirror_file(target, at)
    }
}

/// Keeps `at` a copy of `target`: copied when missing or when the size or
/// modification time differ; the copy gets the source's modification time.
#[cfg(windows)]
fn mirror_file(target: &Path, at: &Path) -> io::Result<()> {
    let src = fs::metadata(target)?;
    if let Ok(m) = fs::metadata(at)
        && m.len() == src.len()
        && m.modified().ok() == src.modified().ok()
    {
        return Ok(());
    }
    fs::copy(target, at)?;
    fs::File::options()
        .write(true)
        .open(at)?
        .set_modified(src.modified()?)
}

/// Removes a link the launcher made (on Windows a junction is a folder).
pub fn remove_link(at: &Path) -> io::Result<()> {
    fs::remove_file(at).or_else(|e| fs::remove_dir(at).map_err(|_| e))
}

/// Links in a runtime folder (and the real folders inside it) whose target
/// is gone (a dump moved): removed so a re-plan can link the new place.
pub fn dangling(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut v = Vec::new();
    for e in fs::read_dir(dir)? {
        let p = e?.path();
        let m = fs::symlink_metadata(&p)?;
        if m.file_type().is_symlink() {
            if fs::metadata(&p).is_err() {
                v.push(p);
            }
        } else if m.is_dir() {
            v.extend(dangling(&p)?);
        }
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{GameId, game};
    use std::collections::HashMap;

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
                Op::Link { at, .. } => Some(at.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect()
    }

    fn copied_to(ops: &[Op]) -> Vec<String> {
        ops.iter()
            .filter_map(|o| match o {
                Op::CopyOnce { to, .. } => Some(to.to_string_lossy().into_owned()),
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
        apply(&ops).unwrap();
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
        apply(&ops).unwrap();
        assert_eq!(fs::read(&ini).unwrap(), b"the game's");
        assert_eq!(fs::read(src.join("d1a.ini")).unwrap(), b"old");
    }

    #[test]
    #[cfg(unix)]
    fn dangling_links_are_found_inside_opened_folders_too() {
        let t = tempfile::tempdir().unwrap();
        let rt = t.path().join("rt");
        fs::create_dir_all(rt.join("Rally")).unwrap();
        std::os::unix::fs::symlink(t.path().join("gone"), rt.join("Rally/x")).unwrap();
        std::os::unix::fs::symlink(t.path(), rt.join("ok")).unwrap();
        assert_eq!(dangling(&rt).unwrap(), [rt.join("Rally/x")]);
    }

    #[test]
    fn apply_links_relinks_and_never_replaces_real_files() {
        let t = tempfile::tempdir().unwrap();
        let (a, b, rt) = (t.path().join("a"), t.path().join("b"), t.path().join("rt"));
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        let at = rt.join("data");
        apply(&[
            Op::Dir(rt.clone()),
            Op::Link {
                target: a.clone(),
                at: at.clone(),
            },
        ])
        .unwrap();
        apply(&[Op::Link {
            target: b.clone(),
            at: at.clone(),
        }])
        .unwrap();
        // the link follows the new target (a symlink, or a junction on Windows)
        fs::write(b.join("in-b"), b"b").unwrap();
        assert_eq!(fs::read(at.join("in-b")).unwrap(), b"b");
        fs::write(rt.join("save.bin"), b"mine").unwrap();
        let err = apply(&[Op::Link {
            target: a,
            at: rt.join("save.bin"),
        }]);
        assert!(err.is_err());
        assert_eq!(fs::read(rt.join("save.bin")).unwrap(), b"mine");
    }

    /// Windows: a linked file is a copy, refreshed when the game's file
    /// changes; a copy is never mistaken for the player's file.
    #[test]
    #[cfg(windows)]
    fn on_windows_a_linked_file_is_a_copy_kept_in_step() {
        let t = tempfile::tempdir().unwrap();
        let (src, at) = (t.path().join("game.exe"), t.path().join("rt-game.exe"));
        fs::write(&src, b"v1").unwrap();
        let op = Op::Link {
            target: src.clone(),
            at: at.clone(),
        };
        apply(std::slice::from_ref(&op)).unwrap();
        assert!(!fs::symlink_metadata(&at).unwrap().file_type().is_symlink());
        assert_eq!(fs::read(&at).unwrap(), b"v1");
        fs::write(&src, b"v2, longer").unwrap();
        apply(std::slice::from_ref(&op)).unwrap();
        assert_eq!(fs::read(&at).unwrap(), b"v2, longer");
    }
}
