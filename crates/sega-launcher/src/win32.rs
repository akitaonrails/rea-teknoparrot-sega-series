//! The few Windows calls F355's session needs beyond the job object
//! (`proc.rs`): its boards' windows, and the CPUs to keep them on.
//!
//! Flycast's Windows build opens a window for every board. The session
//! hides the master's (it shows only the BIOS's screens), keeps the
//! slave's in front, and asks the master to close by its window, which is
//! how Flycast saves before it quits. Windows are found by the threads of
//! the game's own processes, never by title. Nothing is injected into the
//! game: these are the calls any program makes to other programs' windows.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::{CloseHandle, HWND, INVALID_HANDLE_VALUE, LPARAM, RECT};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, RelationAll,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_TERMINATE, SetProcessAffinityMask, TerminateProcess,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumThreadWindows, GetWindowRect, IsWindowVisible, PostMessageW, SW_HIDE, SetForegroundWindow,
    ShowWindow, WM_CLOSE,
};
use windows_sys::core::BOOL;

use sega_core::launch::{self, Cpu};

/// The system's processes (with their parents) and threads (with their
/// processes), from one snapshot.
struct Snapshot {
    processes: Vec<(u32, u32)>,
    threads: Vec<(u32, u32)>,
}

fn snapshot() -> Snapshot {
    let mut s = Snapshot {
        processes: Vec::new(),
        threads: Vec::new(),
    };
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS | TH32CS_SNAPTHREAD, 0);
        if snap == INVALID_HANDLE_VALUE {
            return s;
        }
        let mut p: PROCESSENTRY32W = std::mem::zeroed();
        p.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snap, &mut p) != 0;
        while more {
            s.processes.push((p.th32ProcessID, p.th32ParentProcessID));
            more = Process32NextW(snap, &mut p) != 0;
        }
        let mut t: THREADENTRY32 = std::mem::zeroed();
        t.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
        let mut more = Thread32First(snap, &mut t) != 0;
        while more {
            s.threads.push((t.th32ThreadID, t.th32OwnerProcessID));
            more = Thread32Next(snap, &mut t) != 0;
        }
        CloseHandle(snap);
    }
    s
}

/// A top-level window of one of the game's processes.
struct Window {
    hwnd: HWND,
    pid: u32,
    visible: bool,
    /// Big enough to be a game's window (not a tooltip or an IME box).
    big: bool,
}

/// The top-level windows of the processes `pids`, hidden ones too.
fn windows_of(snap: &Snapshot, pids: &[u32]) -> Vec<Window> {
    unsafe extern "system" fn each(hwnd: HWND, out: LPARAM) -> BOOL {
        let out = unsafe { &mut *(out as *mut Vec<HWND>) };
        out.push(hwnd);
        1
    }
    let mut out = Vec::new();
    for &(tid, pid) in snap.threads.iter().filter(|(_, pid)| pids.contains(pid)) {
        let mut hwnds: Vec<HWND> = Vec::new();
        unsafe { EnumThreadWindows(tid, Some(each), &mut hwnds as *mut Vec<HWND> as LPARAM) };
        for hwnd in hwnds {
            let mut r = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            unsafe { GetWindowRect(hwnd, &mut r) };
            out.push(Window {
                hwnd,
                pid,
                visible: unsafe { IsWindowVisible(hwnd) } != 0,
                big: r.right - r.left >= 200 && r.bottom - r.top >= 150,
            });
        }
    }
    out
}

/// Asks `pid`'s windows to close, as the window's close button does
/// (hidden ones too: F355's master, which the session hid).
pub fn close_windows(pid: u32) {
    for w in windows_of(&snapshot(), &[pid]) {
        unsafe { PostMessageW(w.hwnd, WM_CLOSE, 0, 0) };
    }
}

/// One look at the game's windows: hides `master`'s, and returns the
/// shown windows of the processes it started (the game's; F355's slave).
pub fn tidy(master: u32) -> Vec<HWND> {
    let snap = snapshot();
    let children = crate::proc::descendants(&snap.processes, master);
    for w in windows_of(&snap, &[master]).iter().filter(|w| w.visible) {
        unsafe { ShowWindow(w.hwnd, SW_HIDE) };
    }
    windows_of(&snap, &children)
        .into_iter()
        .filter(|w| w.visible && w.big && w.pid != master)
        .map(|w| w.hwnd)
        .collect()
}

/// Ends the processes `pid` started (what a job object may not hold: Wine
/// doesn't always put a process's children in its job).
pub fn end_descendants(pid: u32) {
    for child in crate::proc::descendants(&snapshot().processes, pid) {
        unsafe {
            let h = OpenProcess(PROCESS_TERMINATE, 0, child);
            if !h.is_null() {
                TerminateProcess(h, 1);
                CloseHandle(h);
            }
        }
    }
}

/// Brings a window to the front, with the keyboard's focus.
pub fn focus(hwnd: HWND) {
    unsafe { SetForegroundWindow(hwnd) };
}

/// The CPUs as Windows describes them (processor group 0), and the
/// performance cores of a hybrid CPU: [`launch::windows_cpus`].
pub fn cpus() -> (Vec<Cpu>, Option<Vec<u32>>) {
    let mut len = 0u32;
    unsafe { GetLogicalProcessorInformationEx(RelationAll, std::ptr::null_mut(), &mut len) };
    if len == 0 {
        return (Vec::new(), None);
    }
    // u64 elements: the records hold 8-byte masks
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    let ok =
        unsafe { GetLogicalProcessorInformationEx(RelationAll, buf.as_mut_ptr().cast(), &mut len) };
    if ok == 0 {
        return (Vec::new(), None);
    }
    let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), len as usize) };
    launch::windows_cpus(bytes)
}

/// Keeps a process (and the processes it starts after this) on `cpus`.
pub fn pin(process: *mut c_void, cpus: &[u32]) -> bool {
    let mask = cpus
        .iter()
        .filter(|&&c| c < usize::BITS)
        .fold(0usize, |m, &c| m | (1 << c));
    mask != 0 && unsafe { SetProcessAffinityMask(process, mask) } != 0
}
