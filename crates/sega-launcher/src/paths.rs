//! Where the launcher keeps things.
//!
//! - data (`$SEGA_DATA`, else `$XDG_DATA_HOME/rea-sega` or
//!   `~/.local/share/rea-sega`; on Windows `%LOCALAPPDATA%\rea-sega`):
//!   `launcher.cfg`, `settings/`,
//!   `runtimes/` (one per game, saves included), `wal/` (windowsarcadeloader's
//!   root: its Wine prefix and profile layers), `launcher.log`;
//! - components (`$SEGA_COMPONENTS`, else `components/` beside the
//!   program): `wal/`, `runner/`, `shaders/`, `art/`, `fonts/`, and
//!   `lindbergh/` from a developer's build; the player's lindbergh-loader is
//!   built on first use into the data folder's `components/lindbergh/`
//!   (CC BY-NC-SA: never inside a package).

use std::path::{Path, PathBuf};

use sega_core::launch::Components;

#[derive(Debug, Clone)]
pub struct Paths {
    pub data: PathBuf,
    pub components: PathBuf,
}

/// What `launcher.cfg` remembers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    /// Folders the games are looked for in.
    pub libraries: Vec<PathBuf>,
    /// The Chihiro BIOS folder (MAME `chihiro` files).
    pub chihiro_bios: Option<PathBuf>,
}

impl Config {
    pub fn parse(text: &str) -> Self {
        let mut c = Config::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = v.trim();
            match k.trim() {
                "library" if !v.is_empty() => c.libraries.push(v.into()),
                "chihiro_bios" if !v.is_empty() => c.chihiro_bios = Some(v.into()),
                _ => {}
            }
        }
        c
    }

    pub fn save(&self) -> String {
        let mut s = String::from("# Written by the launcher.\n");
        for l in &self.libraries {
            s.push_str(&format!("library = {}\n", l.display()));
        }
        if let Some(b) = &self.chihiro_bios {
            s.push_str(&format!("chihiro_bios = {}\n", b.display()));
        }
        s
    }

    pub fn add_library(&mut self, dir: &Path) {
        if !self.libraries.iter().any(|l| l == dir) {
            self.libraries.push(dir.to_path_buf());
        }
    }
}

impl Paths {
    pub fn discover() -> Self {
        let data = std::env::var_os("SEGA_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(default_data);
        let components = std::env::var_os("SEGA_COMPONENTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::current_exe()
                    .ok()
                    .and_then(|e| e.parent().map(|p| p.join("components")))
                    .unwrap_or_else(|| PathBuf::from("components"))
            });
        Self { data, components }
    }

    pub fn config_file(&self) -> PathBuf {
        self.data.join("launcher.cfg")
    }
    pub fn settings(&self, slug: &str) -> PathBuf {
        self.data.join("settings").join(format!("{slug}.cfg"))
    }
    pub fn runtime(&self, slug: &str) -> PathBuf {
        self.data.join("runtimes").join(slug)
    }
    pub fn wal_root(&self) -> PathBuf {
        self.data.join("wal")
    }
    pub fn log(&self) -> PathBuf {
        self.data.join("launcher.log")
    }
    /// A picture of the hub (`<slug>.jpg`, `<slug>-tile.jpg`).
    pub fn art(&self, name: &str) -> PathBuf {
        self.asset("art").join(name)
    }
    pub fn fonts(&self) -> PathBuf {
        self.asset("fonts")
    }
    /// `components/<dir>`; from a development build also the repository's
    /// own `assets/<dir>` when the program sits beside no `components/`.
    fn asset(&self, dir: &str) -> PathBuf {
        let packaged = self.components.join(dir);
        if cfg!(debug_assertions) && !packaged.is_dir() {
            let dev = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../assets")
                .join(dir);
            if dev.is_dir() {
                return dev;
            }
        }
        packaged
    }
    /// Where lindbergh-loader is: the components' `lindbergh/` when it is
    /// there, else the one built on first use in the data folder.
    pub fn lindbergh(&self) -> PathBuf {
        let packaged = self.components.join("lindbergh");
        if packaged.join("lindbergh.so").is_file() {
            packaged
        } else {
            self.lindbergh_build_target().join("lindbergh")
        }
    }

    /// The components folder lindbergh-loader is built into on first use.
    pub fn lindbergh_build_target(&self) -> PathBuf {
        self.data.join("components")
    }

    /// `scripts/build-components.sh`: beside the program in a package, the
    /// repository's own from a development build.
    pub fn build_script(&self) -> PathBuf {
        let beside = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|p| p.join("scripts/build-components.sh")));
        match beside {
            Some(s) if s.is_file() => s,
            _ => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/build-components.sh"),
        }
    }

    pub fn components(&self) -> Components {
        Components {
            lindbergh: self.lindbergh(),
            wal: self.components.join("wal"),
            runner: self.components.join("runner"),
            winetricks: self.components.join("winetricks"),
        }
    }

    pub fn config(&self) -> Config {
        Config::parse(&std::fs::read_to_string(self.config_file()).unwrap_or_default())
    }

    pub fn save_config(&self, c: &Config) -> std::io::Result<()> {
        let f = self.config_file();
        std::fs::create_dir_all(&self.data)?;
        let tmp = f.with_extension("tmp");
        std::fs::write(&tmp, c.save())?;
        std::fs::rename(tmp, f)
    }
}

#[cfg(windows)]
fn default_data() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rea-sega")
}

#[cfg(not(windows))]
fn default_data() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rea-sega")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trips_several_libraries_and_the_bios() {
        let mut c = Config::default();
        c.add_library(Path::new("/roms/TeknoParrot"));
        c.add_library(Path::new("/roms/arcade"));
        c.add_library(Path::new("/roms/arcade"));
        c.chihiro_bios = Some("/bios/chihiro".into());
        assert_eq!(Config::parse(&c.save()), c);
        assert_eq!(c.libraries.len(), 2);
    }

    #[test]
    fn lindbergh_is_the_packaged_one_else_the_one_built_in_the_data_folder() {
        let t = tempfile::tempdir().unwrap();
        let p = Paths {
            data: t.path().join("data"),
            components: t.path().join("pkg/components"),
        };
        // a package never carries it: the first start builds it in the data folder
        assert_eq!(p.lindbergh(), t.path().join("data/components/lindbergh"));
        assert_eq!(p.components().lindbergh, p.lindbergh());
        let dev = t.path().join("pkg/components/lindbergh");
        std::fs::create_dir_all(&dev).unwrap();
        std::fs::write(dev.join("lindbergh.so"), b"").unwrap();
        assert_eq!(p.lindbergh(), dev);
    }
}
