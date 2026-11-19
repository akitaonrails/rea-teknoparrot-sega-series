//! Where the launcher keeps things (`rea_launcher::hub::HubPaths`).
//!
//! - data (`$SEGA_DATA`, else `$XDG_DATA_HOME/rea-sega` or
//!   `~/.local/share/rea-sega`; on Windows `%LOCALAPPDATA%\rea-sega`):
//!   `launcher.cfg`, `settings/`, `runtimes/` (one per game, saves
//!   included), `wal/` (windowsarcadeloader's root: its Wine prefix and
//!   profile layers), `launcher.log`;
//! - components (`$SEGA_COMPONENTS`, else `components/` beside the
//!   program): `lindbergh/` (Linux), `wal/`, `runner/`, `winetricks`,
//!   `directx/` (Linux), `flycast/`, `art/`.

use std::path::{Path, PathBuf};

use rea_launcher::hub::{HubConfig, HubDirs, HubPaths, PathField, PathKind};
use sega_core::launch::Components;

/// The folders' names.
pub fn dirs() -> HubDirs {
    HubDirs {
        env_prefix: "SEGA",
        name: "rea-sega",
        dev_assets: cfg!(debug_assertions)
            .then_some(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets")),
    }
}

/// The player's BIOS folder: MAME's `chihiro` files, F355's
/// `f355bios.zip` (at the top or in a `chihiro`, `dc` or `naomi` folder, as
/// EmuDeck keeps them). 0.1.0 called it `chihiro_bios`.
pub const BIOS: PathField = PathField {
    key: "bios",
    aliases: &["chihiro_bios"],
    kind: PathKind::Folder,
    flag: "--bios",
    flag_aliases: &["--chihiro-bios"],
    help: "your BIOS folder (Chihiro files, f355bios.zip)",
    heading: "BIOS folder, needed for OutRun 2 SP, Crazy Taxi High Roller and F355 Challenge 2",
    label: "BIOS folder",
    placeholder: "/path/to/bios",
    button: "Use BIOS",
    empty: "Type the path of your BIOS folder first.",
    saved: "BIOS folder saved.",
};

/// The paths `launcher.cfg` keeps besides the library folders.
pub const FIELDS: &[PathField] = &[BIOS];

/// The BIOS folder `config` names.
pub fn bios(config: &HubConfig) -> Option<&Path> {
    config.paths.first().and_then(Option::as_deref)
}

/// windowsarcadeloader's root (every Windows game's prefix and links).
pub fn wal_root(p: &HubPaths) -> PathBuf {
    p.data.join("wal")
}

/// `launcher.cfg`; a file that can't be read is an error.
pub fn config(p: &HubPaths) -> std::io::Result<HubConfig> {
    HubConfig::load(&p.config_file(), FIELDS)
}

pub fn components(p: &HubPaths) -> Components {
    let c = &p.components;
    Components {
        lindbergh: c.join("lindbergh"),
        wal: c.join("wal"),
        runner: c.join("runner"),
        winetricks: c.join("winetricks"),
        directx: c.join("directx"),
        flycast: c.join("flycast"),
        xemu: c.join("xemu"),
        // Flycast starts F355's boards through this program
        launcher: std::env::current_exe().unwrap_or_else(|_| "sega-launcher".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn components_sit_in_the_components_folder() {
        let p = HubPaths {
            data: "/d".into(),
            components: "/pkg/components".into(),
            dev_assets: None,
        };
        let c = components(&p);
        assert_eq!(c.lindbergh, Path::new("/pkg/components/lindbergh"));
        assert_eq!(c.directx, Path::new("/pkg/components/directx"));
        assert_eq!(wal_root(&p), Path::new("/d/wal"));
    }

    #[test]
    fn the_bios_folder_that_0_1_0_saved_still_reads() {
        let c = HubConfig::parse(
            "# Written by the launcher.\nlibrary = /g\nchihiro_bios = /bios/chihiro\n",
            FIELDS,
        );
        assert_eq!(bios(&c), Some(Path::new("/bios/chihiro")));
    }
}
