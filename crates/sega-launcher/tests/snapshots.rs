//! Golden snapshots (docs/STANDARDS.md, "Golden snapshots"): everything the
//! launcher produces, compared with the files in `tests/snapshots/`, so an
//! SDK bump or a refactor can't change it silently. A diff is reviewed like
//! code: it must match an intended change or an SDK Migration item.
//!
//! `REA_UPDATE_SNAPSHOTS=1 cargo test --test snapshots` rewrites them.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Command;

use sega_core::catalog::{GAMES, Host};
use sega_core::launch::{self, Components, Places};
use sega_core::settings::{self, Look, Settings};

/// A path as the snapshot spells it on every system (Windows joins with `\`).
fn p(path: &std::path::Path) -> String {
    path.display().to_string().replace('\\', "/")
}

#[test]
fn help() {
    let out = Command::new(env!("CARGO_BIN_EXE_sega-launcher"))
        .arg("--help")
        .env("SEGA_DATA", env!("CARGO_TARGET_TMPDIR"))
        .output()
        .expect("run the launcher");
    assert!(out.status.success());
    rea_launcher::snapshot!("help.txt", String::from_utf8_lossy(&out.stdout));
}

#[test]
fn the_settings_template() {
    rea_launcher::snapshot!("settings.cfg.txt", settings::template());
}

/// What each look plans for each game (files, command, environment,
/// gamescope), on a 2560x1440 screen, on Linux and Windows.
#[test]
fn every_look_of_every_game() {
    let c = Components {
        lindbergh: "/c/lindbergh".into(),
        wal: "/c/wal".into(),
        runner: "/c/runner".into(),
        winetricks: "/c/winetricks".into(),
    };
    let mut out = String::new();
    for host in [Host::Linux, Host::Windows] {
        for g in GAMES {
            let places = Places {
                runtime: PathBuf::from("/d/runtimes").join(g.slug),
                wal_root: "/d/wal".into(),
                wheel: None,
            };
            for look in Look::ALL {
                let mut s = Settings::default();
                look.apply(&mut s, g.native, Some((2560, 1440)));
                let _ = writeln!(out, "== {host:?} {} {}", g.slug, look.key());
                match launch::plan(g, &places, &c, &s, "eth0", false, host) {
                    Ok(plan) => {
                        let _ = writeln!(out, "size {}", plan.game_size);
                        let _ = writeln!(
                            out,
                            "run {} {}",
                            p(&plan.game.program),
                            plan.game.args.join(" ").replace('\\', "/")
                        );
                        let _ = writeln!(out, "cwd {}", p(&plan.game.cwd));
                        for (k, v) in &plan.game.env {
                            let _ = writeln!(out, "env {k}={}", v.replace('\\', "/"));
                        }
                        let _ = writeln!(out, "gamescope {}", plan.gamescope.join(" "));
                        let _ = writeln!(out, "input {:?}", plan.input_pipe);
                        for (at, target) in &plan.links {
                            let _ = writeln!(out, "link {} -> {}", p(at), p(target));
                        }
                        for (f, text) in &plan.files {
                            let _ = writeln!(out, "file {}\n{text}", p(f));
                        }
                    }
                    Err(e) => {
                        let _ = writeln!(out, "not yet: {e}");
                    }
                }
            }
        }
    }
    rea_launcher::snapshot!("plans.txt", out);
}
