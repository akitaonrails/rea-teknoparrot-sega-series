//! Starting a Flycast board (Linux). Flycast runs a linked NAOMI cabinet as
//! one process per board, and its master starts each slave by running its
//! own program (`argv[0]`) again. The session starts the master as this
//! program with [`FLYCAST_ENV`] naming Flycast; this program then becomes
//! Flycast (same process, so a slave stays the master's child, which is how
//! it finds the master) with `argv[0]` pointing back here and the
//! environment of its board (`launch::board_env`): the master off screen,
//! the slave in the window.

use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode};

use sega_core::launch::{self, FLYCAST_ENV};

pub fn exec(flycast: OsString) -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let text: Vec<String> = args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let board = launch::flycast_board(&text);
    let me = match std::env::current_exe() {
        Ok(me) => me,
        Err(e) => {
            eprintln!("sega-launcher: can't find its own program to start Flycast's boards: {e}");
            return ExitCode::from(1);
        }
    };
    let err = Command::new(&flycast)
        .arg0(me)
        .args(&args)
        .envs(launch::board_env(board).iter().copied())
        .exec();
    eprintln!(
        "sega-launcher: couldn't start Flycast ({}): {err}; {FLYCAST_ENV} names it",
        flycast.to_string_lossy()
    );
    ExitCode::from(1)
}
