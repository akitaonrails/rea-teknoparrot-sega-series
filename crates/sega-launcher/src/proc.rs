//! A game's processes, started and stopped as one group: on Linux a
//! process group plus the session's tag in every environment (Wine
//! reparents its processes out of the group), on Windows a job object (every
//! process the game starts belongs to it, the loader's own children too).

use std::process::{Child, Command};

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

/// The processes `pid` started, and theirs, from (process, parent) pairs
/// (F355's slave board on Windows is its master's child).
#[cfg_attr(unix, allow(dead_code))]
pub fn descendants(processes: &[(u32, u32)], pid: u32) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    let mut next = vec![pid];
    while let Some(p) = next.pop() {
        for &(child, parent) in processes {
            // Windows' idle process is its own parent
            if parent == p && child != pid && child != parent && !out.contains(&child) {
                out.push(child);
                next.push(child);
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
}

#[cfg(unix)]
impl Group {
    pub fn of(child: &Child, tag: &str) -> std::io::Result<Group> {
        Ok(Group {
            pgid: child.id() as i32,
            tag: tag.to_string(),
        })
    }

    /// Asks the group to end.
    pub fn terminate(&self) {
        signal_group(self.pgid, "TERM");
    }

    pub fn kill(&self) {
        signal_group(self.pgid, "KILL");
    }

    /// Ends whatever is left (processes that left the group); how many.
    pub fn leftovers(&self) -> usize {
        kill_tagged(&self.tag)
    }
}

#[cfg(unix)]
fn signal_group(pgid: i32, sig: &str) {
    let _ = Command::new("kill")
        .args([format!("-{sig}"), "--".into(), format!("-{pgid}")])
        .stderr(std::process::Stdio::null())
        .status();
}

/// Kill every process whose environment carries this session's tag.
#[cfg(unix)]
pub fn kill_tagged(tag: &str) -> usize {
    let want = format!("{TAG}={tag}");
    let mut n = 0;
    let Ok(rd) = std::fs::read_dir("/proc") else {
        return 0;
    };
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
        if env.split(|b| *b == 0).any(|kv| kv == want.as_bytes()) {
            let _ = Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .status();
            n += 1;
        }
    }
    n
}

#[cfg(windows)]
impl Group {
    pub fn of(child: &Child, _tag: &str) -> std::io::Result<Group> {
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
            })
        }
    }

    /// Windows has no polite signal: the session's process is asked to
    /// close by its windows (Flycast saves, then quits). The loaders have
    /// none; they end when their input ends (the session closes it before
    /// this).
    pub fn terminate(&self) {
        crate::win32::close_windows(self.pid);
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

    #[test]
    fn the_boards_flycast_starts_are_its_descendants() {
        // 340: the master; 576: the slave it started; 900: a helper the
        // slave started; 12: another program; 0: Windows' idle process
        let ps = [
            (0, 0),
            (12, 4),
            (340, 100),
            (576, 340),
            (900, 576),
            (13, 12),
        ];
        assert_eq!(descendants(&ps, 340), vec![576, 900]);
        assert_eq!(descendants(&ps, 576), vec![900]);
        assert!(descendants(&ps, 900).is_empty());
        assert_eq!(descendants(&ps, 0), Vec::<u32>::new());
        // a loop in stale records doesn't hang
        assert_eq!(descendants(&[(1, 2), (2, 1)], 1), vec![2]);
    }
}
