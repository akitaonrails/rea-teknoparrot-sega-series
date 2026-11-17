//! A game's processes, started and stopped as one group: on Linux a
//! process group plus the session's tag in every environment (Wine
//! reparents its processes out of the group), on Windows a job object (every
//! process the game starts belongs to it, the loader's own children too).

use std::process::{Child, Command};

/// Which of a linked cabinet's boards are running.
#[cfg(unix)]
#[derive(Debug, Default, Clone, Copy)]
pub struct Boards {
    /// The master: it reads the controls and saves the cabinet's memory
    /// when it quits.
    pub master: Option<i32>,
    /// The slave: it draws the game.
    pub slave: Option<i32>,
}

/// The environment variable every process of a session carries on Linux,
/// so leftovers can be found and stopped.
#[cfg(unix)]
pub const TAG: &str = "SEGA_SESSION";

/// Before spawning: the game's processes in their own group.
pub fn configure(cmd: &mut Command, tag: &str) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.env(TAG, tag).process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = tag;
        // no console window of its own (the loaders are console programs)
        cmd.creation_flags(0x0800_0000);
    }
}

/// The processes `pid` started, and theirs (F355's slave board on Windows
/// is its master's child), from the system's (process, parent) list.
/// Windows keeps a dead parent's id on its children and reuses ids, so a
/// process counts only when it was `created` after its parent: a program
/// the player started from some long-gone process that had our id is never
/// taken for the game's. `created` is asked only about those candidates
/// (None when it can't be read: not counted).
#[cfg_attr(unix, allow(dead_code))]
pub fn descendants(
    processes: &[(u32, u32)],
    pid: u32,
    created: impl Fn(u32) -> Option<u64>,
) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    let mut next = vec![(pid, created(pid))];
    while let Some((parent, born)) = next.pop() {
        let Some(born) = born else { continue };
        for &(child, of) in processes {
            if of != parent || child == parent || child == pid || out.contains(&child) {
                continue;
            }
            let at = created(child);
            if at.is_some_and(|c| c >= born) {
                out.push(child);
                next.push((child, at));
            }
        }
    }
    out
}

pub struct Group {
    #[cfg(unix)]
    pgid: i32,
    #[cfg(unix)]
    tag: String,
    #[cfg(windows)]
    job: windows_sys::Win32::Foundation::HANDLE,
    /// The process the session started.
    #[cfg(windows)]
    pid: u32,
    /// Its windows are asked to close on [`Group::terminate`] (F355's
    /// master board; the loaders have no windows to ask).
    #[cfg(windows)]
    by_window: bool,
}

#[cfg(unix)]
impl Group {
    pub fn of(child: &Child, tag: &str, _by_window: bool) -> std::io::Result<Group> {
        Ok(Group {
            pgid: child.id() as i32,
            tag: tag.to_string(),
        })
    }

    /// Asks the group to end.
    pub fn terminate(&self) {
        signal_group(self.pgid, "TERM");
    }

    /// Asks one of its processes to end.
    pub fn terminate_one(&self, pid: i32) {
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status();
    }

    pub fn kill(&self) {
        signal_group(self.pgid, "KILL");
    }

    /// Ends whatever is left (processes that left the group); how many.
    pub fn leftovers(&self) -> usize {
        kill_tagged(&self.tag)
    }

    /// Whether any of the session's processes still runs.
    pub fn any_left(&self) -> bool {
        !tagged(&self.tag).is_empty()
    }

    /// Which of the session's Flycast boards run: the master is the process
    /// whose arguments are exactly the session's `args` (gamescope and
    /// bubblewrap carry them too, after their own), the slave the one
    /// Flycast started with `naomi:BoardId=`
    /// ([`sega_core::launch::flycast_board`]; it gets the set in another
    /// form).
    pub fn boards(&self, args: &[String]) -> Boards {
        use sega_core::launch::{Board, flycast_board};
        let mut b = Boards::default();
        for pid in tagged(&self.tag) {
            let Ok(c) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
                continue;
            };
            let argv: Vec<String> = c
                .split(|b| *b == 0)
                .filter(|a| !a.is_empty())
                .map(|a| String::from_utf8_lossy(a).into_owned())
                .collect();
            match flycast_board(&argv) {
                Board::Slave => b.slave = Some(pid),
                Board::Master if argv.get(1..) == Some(args) => b.master = Some(pid),
                Board::Master => {}
            }
        }
        b
    }
}

#[cfg(unix)]
fn signal_group(pgid: i32, sig: &str) {
    let _ = Command::new("kill")
        .args([format!("-{sig}"), "--".into(), format!("-{pgid}")])
        .stderr(std::process::Stdio::null())
        .status();
}

/// The running processes (not zombies) whose environment carries this
/// session's tag, besides this one.
#[cfg(unix)]
fn tagged(tag: &str) -> Vec<i32> {
    let want = format!("{TAG}={tag}");
    let Ok(rd) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut v = Vec::new();
    for e in rd.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<i32>().ok()) else {
            continue;
        };
        if pid as u32 == std::process::id() {
            continue;
        }
        let Ok(env) = std::fs::read(e.path().join("environ")) else {
            continue;
        };
        // a zombie's state, after its name in parentheses, is Z
        let zombie = std::fs::read_to_string(e.path().join("stat"))
            .ok()
            .and_then(|s| {
                s.rsplit_once(')')
                    .map(|(_, r)| r.trim_start().starts_with('Z'))
            })
            .unwrap_or(false);
        if !zombie && env.split(|b| *b == 0).any(|kv| kv == want.as_bytes()) {
            v.push(pid);
        }
    }
    v
}

/// Kill every process whose environment carries this session's tag.
#[cfg(unix)]
pub fn kill_tagged(tag: &str) -> usize {
    let pids = tagged(tag);
    for pid in &pids {
        let _ = Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status();
    }
    pids.len()
}

#[cfg(windows)]
impl Group {
    pub fn of(child: &Child, _tag: &str, by_window: bool) -> std::io::Result<Group> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::*;
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(std::io::Error::last_os_error());
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            // the launcher closed: its game goes with it
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if AssignProcessToJobObject(job, child.as_raw_handle() as _) == 0 {
                let e = std::io::Error::last_os_error();
                windows_sys::Win32::Foundation::CloseHandle(job);
                return Err(e);
            }
            Ok(Group {
                job,
                pid: child.id(),
                by_window,
            })
        }
    }

    /// Windows has no polite signal: the session's process is asked to
    /// close by its windows (Flycast saves, then quits). The loaders have
    /// none; they end when their input ends (the session closes it before
    /// this).
    pub fn terminate(&self) {
        if self.by_window {
            crate::win32::close_windows(self.pid);
        }
    }

    /// Whether processes the session's process started are still running
    /// (it saves itself before it quits).
    pub fn any_left(&self) -> bool {
        crate::win32::any_descendants(self.pid)
    }

    /// One look at the game's windows ([`sega_core::launch::Plan::one_window`]):
    /// hides those of the session's process and returns the shown windows
    /// of the processes it started (the game's).
    pub fn game_windows(&self) -> Vec<windows_sys::Win32::Foundation::HWND> {
        crate::win32::tidy(self.pid)
    }

    pub fn kill(&self) {
        // the processes it started, too: Wine's jobs don't always hold them
        crate::win32::end_descendants(self.pid);
        unsafe { windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job, 1) };
    }

    /// Ends whatever is left in the job; how many.
    pub fn leftovers(&self) -> usize {
        use windows_sys::Win32::System::JobObjects::*;
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { std::mem::zeroed() };
        let ok = unsafe {
            QueryInformationJobObject(
                self.job,
                JobObjectBasicAccountingInformation,
                &mut info as *mut _ as *mut std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                std::ptr::null_mut(),
            )
        };
        let n = if ok != 0 {
            info.ActiveProcesses as usize
        } else {
            0
        };
        self.kill();
        n
    }
}

#[cfg(windows)]
impl Drop for Group {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.job) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The process list and a start-time lookup from (pid, parent, created).
    fn listed(rows: &[(u32, u32, Option<u64>)]) -> (Vec<(u32, u32)>, impl Fn(u32) -> Option<u64>) {
        let times: Vec<(u32, Option<u64>)> = rows.iter().map(|r| (r.0, r.2)).collect();
        (rows.iter().map(|r| (r.0, r.1)).collect(), move |id| {
            times.iter().find(|t| t.0 == id).and_then(|t| t.1)
        })
    }

    #[test]
    fn the_boards_flycast_starts_are_its_descendants() {
        // 340: the master (created at 100); 576: the slave it started;
        // 900: a helper the slave started; 13: another program; 0: Windows'
        // idle process, its own parent
        let (ps, at) = listed(&[
            (0, 0, Some(0)),
            (12, 4, Some(5)),
            (340, 100, Some(100)),
            (576, 340, Some(110)),
            (900, 576, Some(120)),
            (13, 12, Some(6)),
        ]);
        assert_eq!(descendants(&ps, 340, &at), vec![576, 900]);
        assert_eq!(descendants(&ps, 576, &at), vec![900]);
        assert!(descendants(&ps, 900, &at).is_empty());
        assert!(descendants(&ps, 0, &at).is_empty());
        // a loop in stale records doesn't hang
        let (ring, at) = listed(&[(1, 2, Some(1)), (2, 1, Some(1))]);
        assert_eq!(descendants(&ring, 1, &at), vec![2]);
    }

    #[test]
    fn a_program_whose_dead_parent_had_our_id_is_not_the_games() {
        // the player's browser (700) was started at 50 by a process that
        // had id 340 and has since quit; our master got id 340 at 100
        let (ps, at) = listed(&[
            (340, 100, Some(100)),
            (700, 340, Some(50)),
            (701, 700, Some(60)),
            (576, 340, Some(110)),
            // a process whose start time can't be read: left alone
            (800, 340, None),
        ]);
        assert_eq!(descendants(&ps, 340, &at), vec![576]);
        // without the master's own time nothing counts
        let (ps, at) = listed(&[(340, 100, None), (576, 340, Some(110))]);
        assert!(descendants(&ps, 340, &at).is_empty());
    }
}
