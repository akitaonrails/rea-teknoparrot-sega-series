//! Finding the player's games in their library folders.
//!
//! A library is a folder of game folders (any names, TeknoParrot-style:
//! one game each). Each folder whose main file matches a known edition
//! becomes an [`Entry`]; a known file name with an unknown digest is
//! reported, never launched.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::catalog::{self, Edition, Game, GameId};

#[derive(Debug, Clone, PartialEq)]
pub enum Found {
    Known {
        game: &'static Game,
        edition: &'static Edition,
    },
    /// A file named like a game's main file, with a digest we don't know.
    Unknown { file: String, digest: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The game folder.
    pub dir: PathBuf,
    pub found: Found,
}

impl Entry {
    pub fn game_id(&self) -> Option<GameId> {
        match self.found {
            Found::Known { game, .. } => Some(game.id),
            Found::Unknown { .. } => None,
        }
    }
}

/// Digests by (path, size, mtime), so a rescan doesn't read large files again.
#[derive(Debug, Default, Clone)]
pub struct HashCache {
    map: HashMap<PathBuf, (u64, Option<SystemTime>, String)>,
}

impl HashCache {
    fn digest(&mut self, path: &Path) -> std::io::Result<String> {
        let meta = fs::metadata(path)?;
        let stamp = (meta.len(), meta.modified().ok());
        if let Some((len, mt, d)) = self.map.get(path)
            && (*len, *mt) == stamp
        {
            return Ok(d.clone());
        }
        let d = sha256_file(path)?;
        self.map
            .insert(path.to_path_buf(), (stamp.0, stamp.1, d.clone()));
        Ok(d)
    }
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

/// Every main file name, each once.
fn probes() -> Vec<&'static str> {
    let mut v: Vec<_> = catalog::GAMES.iter().map(|g| g.main_file).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Every game in `dir` itself.
pub fn probe_dir(dir: &Path, cache: &mut HashCache) -> Vec<Entry> {
    let mut out = Vec::new();
    for name in probes() {
        let file = dir.join(name);
        if !file.is_file() {
            continue;
        }
        let Ok(digest) = cache.digest(&file) else {
            continue;
        };
        let found = match catalog::identify(&digest) {
            Some((game, edition)) if game.main_file == name => Found::Known { game, edition },
            _ => Found::Unknown {
                file: name.to_string(),
                digest,
            },
        };
        out.push(Entry {
            dir: dir.to_path_buf(),
            found,
        });
    }
    out
}

/// Every game in each root and the folders directly inside it, in catalog
/// order, unknown builds last.
pub fn scan(roots: &[PathBuf], cache: &mut HashCache) -> Vec<Entry> {
    let mut out = Vec::new();
    for root in roots {
        out.extend(probe_dir(root, cache));
        if let Ok(rd) = fs::read_dir(root) {
            let mut dirs: Vec<PathBuf> = rd
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            dirs.sort();
            for d in dirs {
                out.extend(probe_dir(&d, cache));
            }
        }
    }
    out.sort_by_key(|e| (e.game_id().is_none(), e.game_id()));
    out
}

/// The first folder found for each game (a library may hold duplicates).
pub fn by_game(entries: &[Entry]) -> HashMap<GameId, &Entry> {
    let mut m = HashMap::new();
    for e in entries {
        if let Some(id) = e.game_id() {
            m.entry(id).or_insert(e);
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_digest_is_reported_with_its_hash() {
        let t = tempfile::tempdir().unwrap();
        let g = t.path().join("Some Game [TP]");
        fs::create_dir(&g).unwrap();
        fs::write(g.join("d1a.exe"), b"not the game").unwrap();
        fs::create_dir(t.path().join("empty")).unwrap();
        let found = scan(&[t.path().to_path_buf()], &mut HashCache::default());
        assert_eq!(found.len(), 1);
        match &found[0].found {
            Found::Unknown { file, digest } => {
                assert_eq!(file, "d1a.exe");
                assert_eq!(digest, &sha256_file(&g.join("d1a.exe")).unwrap());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_game_in_a_subfolder_of_its_dump_is_found_by_its_path() {
        let t = tempfile::tempdir().unwrap();
        let g = t.path().join("SEGA Rally 3 (2008) [Sega Europa-R] [TP]");
        fs::create_dir_all(g.join("Rally")).unwrap();
        fs::write(g.join("Rally/Rally.exe"), b"x").unwrap();
        let found = scan(&[t.path().to_path_buf()], &mut HashCache::default());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].dir, g);
        assert!(
            matches!(&found[0].found, Found::Unknown { file, .. } if file == "Rally/Rally.exe")
        );
    }

    #[test]
    fn cache_rehashes_only_when_the_file_changes() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("main");
        fs::write(&f, b"one").unwrap();
        let mut c = HashCache::default();
        let a = c.digest(&f).unwrap();
        assert_eq!(c.digest(&f).unwrap(), a);
        fs::write(&f, b"three").unwrap();
        assert_ne!(c.digest(&f).unwrap(), a);
    }

    /// The owner's real library, when present
    /// (`SEGA_LIBRARIES=dir1:dir2 cargo test -- --ignored`).
    #[test]
    #[ignore]
    fn real_library_finds_every_game() {
        let Ok(roots) = std::env::var("SEGA_LIBRARIES") else {
            return;
        };
        let roots: Vec<PathBuf> = std::env::split_paths(&roots).collect();
        let found = scan(&roots, &mut HashCache::default());
        let ids = by_game(&found);
        assert_eq!(ids.len(), catalog::GAMES.len(), "{found:#?}");
    }
}
