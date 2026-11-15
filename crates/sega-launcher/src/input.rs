//! The input thread: it owns SDL, reads wheels and pads, turns them into
//! the cabinet's controls and, while a Windows game runs, writes them to
//! windowsarcadeloader's input pipe and plays the force feedback it sends
//! back. Between games (or for the native loaders, which read SDL
//! themselves) it only reports what it sees.

use std::io::Write;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rea_launcher::ffb::FfbOutput;
use rea_launcher::wheel::{
    Control, Controls as Read, DeviceId, Kind, PadSettings, SteeringLock, WheelProfile,
    WheelSettings, read_pad,
};
use rea_launcher_native::policy::HapticPolicy;
use rea_launcher_native::sdl::{Devices, Sdl};
use sega_core::input::{Button, Cabinet, StickLayout, default_pad_map};
use sega_core::settings::Settings;

const TICK: Duration = Duration::from_millis(4);
/// The pipe is refreshed at least this often even without a change.
const REFRESH: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInfo {
    pub name: String,
    pub kind: Kind,
    pub haptic: bool,
}

/// What the window shows.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub devices: Vec<DeviceInfo>,
    /// The device driving the car, by index into `devices`.
    pub driver: Option<usize>,
    pub controls: Read,
    pub feeding: bool,
    pub error: Option<String>,
}

/// The player's wheel and pad options for one game run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tuning {
    /// Stick dead zone, fraction of the travel.
    pub deadzone: f32,
    pub lock: SteeringLock,
    /// Force feedback strength (1.0 = the game's).
    pub ffb_gain: f32,
}

impl Tuning {
    pub fn of(s: &Settings) -> Tuning {
        Tuning {
            deadzone: s.deadzone_fraction(),
            lock: s.steering_lock,
            ffb_gain: s.force_feedback_gain(),
        }
    }
}

impl Default for Tuning {
    fn default() -> Self {
        Tuning::of(&Settings::default())
    }
}

pub enum Command {
    /// Start feeding a game: `sink` is the loader's stdin.
    Attach {
        sink: Box<dyn Write + Send>,
        layout: StickLayout,
        tuning: Tuning,
    },
    /// A force feedback line from the loader (`out <player> <id> <value>`).
    Output(String),
    Detach,
}

pub struct InputService {
    tx: Sender<Command>,
    snapshot: Arc<Mutex<Snapshot>>,
    thread: Option<JoinHandle<()>>,
}

impl InputService {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        let shared = Arc::clone(&snapshot);
        let thread = std::thread::Builder::new()
            .name("input".into())
            .spawn(move || run(rx, shared))
            .expect("spawn input thread");
        Self {
            tx,
            snapshot,
            thread: Some(thread),
        }
    }

    pub fn sender(&self) -> Sender<Command> {
        self.tx.clone()
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

impl Drop for InputService {
    fn drop(&mut self) {
        let (tx, _) = mpsc::channel();
        drop(std::mem::replace(&mut self.tx, tx));
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The game's force feedback state, from windowsarcadeloader's outputs
/// (`wal_protocol::output`: 1 constant, 2 centering spring, 3 vibration,
/// each ±10000). The SDK has no spring effect: the spring is played as a
/// damper, which also resists the wheel.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Ffb(pub FfbOutput);

impl Ffb {
    /// The state scaled by the player's strength (`gain`, 0–2), each
    /// effect kept within its range.
    pub fn scaled(&self, gain: f32) -> FfbOutput {
        let o = self.0;
        FfbOutput {
            constant: (o.constant * gain).clamp(-1.0, 1.0),
            damper: (o.damper * gain).clamp(0.0, 1.0),
            rumble: (o.rumble * gain).clamp(0.0, 1.0),
            ..o
        }
    }

    /// Apply one `out <player> <id> <value>` line; `false` if it isn't one.
    pub fn apply(&mut self, line: &str) -> bool {
        let mut w = line.split_whitespace();
        if w.next() != Some("out") {
            return false;
        }
        let (Some(_player), Some(id), Some(value)) = (
            w.next(),
            w.next().and_then(|v| v.parse::<u16>().ok()),
            w.next().and_then(|v| v.parse::<i32>().ok()),
        ) else {
            return false;
        };
        let v = value as f32 / 10000.0;
        match id {
            1 => self.0.constant = v.clamp(-1.0, 1.0),
            2 => self.0.damper = v.clamp(0.0, 1.0),
            3 => self.0.rumble = v.clamp(0.0, 1.0),
            _ => return false,
        }
        true
    }
}

struct Feed {
    sink: Box<dyn Write + Send>,
    cabinet: Cabinet,
    last: String,
    sent: Instant,
}

fn run(rx: Receiver<Command>, shared: Arc<Mutex<Snapshot>>) {
    let sdl = match Sdl::init(&[]) {
        Ok(s) => s,
        Err(e) => {
            if let Ok(mut s) = shared.lock() {
                s.error = Some(format!("controllers unavailable: {e}"));
            }
            // still feed a game (neutral input) so it never waits on us
            idle_without_sdl(rx);
            return;
        }
    };
    let mut devices = Devices::new(&sdl, HapticPolicy::default());
    let mut pad_settings = PadSettings::new(default_pad_map());
    let mut wheel_settings = WheelSettings::default();
    let mut gain = 1.0f32;
    let mut profile: Option<(DeviceId, WheelProfile<Button>)> = None;
    let mut feed: Option<Feed> = None;
    let mut ffb = Ffb::default();
    loop {
        loop {
            match rx.try_recv() {
                Ok(Command::Attach {
                    sink,
                    layout,
                    tuning,
                }) => {
                    pad_settings.stick_deadzone = stick_deadzone(tuning.deadzone);
                    wheel_settings.lock = tuning.lock;
                    gain = tuning.ffb_gain;
                    ffb = Ffb::default();
                    feed = Some(Feed {
                        sink,
                        cabinet: Cabinet::new(layout),
                        last: String::new(),
                        sent: Instant::now() - REFRESH,
                    });
                }
                Ok(Command::Output(line)) => {
                    ffb.apply(&line);
                }
                Ok(Command::Detach) => {
                    feed = None;
                    devices.release_force_feedback();
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        devices.pump();
        let list = devices.list();
        let wheel = list.iter().position(|d| d.kind == Kind::Wheel);
        let mut read = Read::default();
        if let Some(w) = wheel {
            let d = &list[w];
            let fresh = profile
                .as_ref()
                .is_none_or(|(id, _)| id.vid != d.id.vid || id.pid != d.id.pid);
            if fresh {
                profile = Some((
                    d.id.clone(),
                    WheelProfile::guess(
                        d.id.clone(),
                        &d.initial,
                        Some((Button::GearUp, Button::GearDown)),
                    ),
                ));
            }
            if let Some((_, p)) = &profile {
                read = p.read(&wheel_settings, &d.state, None);
            }
        }
        let mut pad_driver = None;
        for (i, d) in list.iter().enumerate() {
            if let Some(pad) = d.pad {
                read = read.merge(read_pad(&pad_settings, &pad, None));
                pad_driver.get_or_insert(i);
            }
        }
        let driver = wheel.or(pad_driver);
        let infos: Vec<DeviceInfo> = list
            .iter()
            .map(|d| DeviceInfo {
                name: d.id.name.clone(),
                kind: d.kind,
                haptic: d.haptic,
            })
            .collect();
        if let Some(f) = feed.as_mut() {
            let line = f.cabinet.pipe_line(&read);
            if line != f.last || f.sent.elapsed() >= REFRESH {
                if writeln!(f.sink, "{line}")
                    .and_then(|_| f.sink.flush())
                    .is_err()
                {
                    feed = None; // the loader is gone
                } else {
                    f.last = line;
                    f.sent = Instant::now();
                }
            }
            if let Some(d) = driver {
                devices.force_feedback(d, &ffb.scaled(gain));
            }
        }
        if let Ok(mut s) = shared.lock() {
            *s = Snapshot {
                devices: infos,
                driver,
                controls: read,
                feeding: feed.is_some(),
                error: None,
            };
        }
        std::thread::sleep(TICK);
    }
}

fn idle_without_sdl(rx: Receiver<Command>) {
    let mut feed: Option<(Box<dyn Write + Send>, Cabinet)> = None;
    loop {
        match rx.recv_timeout(REFRESH) {
            Ok(Command::Attach { sink, layout, .. }) => feed = Some((sink, Cabinet::new(layout))),
            Ok(Command::Detach) => feed = None,
            Ok(Command::Output(_)) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        if let Some((sink, cabinet)) = feed.as_mut() {
            let line = cabinet.pipe_line(&Read::default());
            if writeln!(sink, "{line}").and_then(|_| sink.flush()).is_err() {
                feed = None;
            }
        }
    }
}

/// The settings' dead zone (0–0.3 of the stick's travel) in raw stick units.
pub fn stick_deadzone(fraction: f32) -> u16 {
    (fraction.clamp(0.0, 0.3) * 32767.0).round() as u16
}

/// A timed input script for tests (`--input-script`): `<seconds> <inputs…>`
/// per line, inputs held until the next line; `-` releases everything.
/// Inputs are the cabinet's buttons by key (`start`, `view`, `coin`,
/// `gear_up`…), `accel`, `brake`, `steer=<-1..1>`.
#[derive(Clone)]
pub struct Script {
    steps: Vec<(f64, Read)>,
}

impl Script {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut steps = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut words = line.split_whitespace();
            let t: f64 = words
                .next()
                .and_then(|w| w.parse().ok())
                .ok_or(format!("line {}: no time", n + 1))?;
            let mut r = Read::default();
            for w in words.filter(|w| *w != "-") {
                if w == "accel" {
                    r.accel = 1.0;
                } else if w == "brake" {
                    r.brake = 1.0;
                } else if let Some(v) = w.strip_prefix("steer=") {
                    r.steer = v.parse().map_err(|_| format!("line {}: {w}", n + 1))?;
                } else {
                    let b = Button::ALL
                        .iter()
                        .find(|b| b.key() == w)
                        .ok_or(format!("line {}: unknown input {w}", n + 1))?;
                    r.buttons |= b.bit();
                }
            }
            steps.push((t, r));
        }
        steps.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(Self { steps })
    }

    pub fn at(&self, seconds: f64) -> Read {
        self.steps
            .iter()
            .rev()
            .find(|(t, _)| *t <= seconds)
            .map(|(_, r)| *r)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dead_zone_converts_to_raw_units() {
        assert_eq!(stick_deadzone(0.0), 0);
        assert_eq!(stick_deadzone(0.05), 1638);
        assert_eq!(stick_deadzone(0.9), 9830);
    }

    #[test]
    fn force_feedback_lines_update_the_state() {
        let mut f = Ffb::default();
        assert!(f.apply("out 0 1 -5000"));
        assert!(f.apply("out 0 2 10000"));
        assert!(f.apply("out 0 3 2500"));
        assert!(!f.apply("out 0 9 1"));
        assert!(!f.apply("wine: hello"));
        assert_eq!(f.0.constant, -0.5);
        assert_eq!(f.0.damper, 1.0);
        assert_eq!(f.0.rumble, 0.25);
        let half = f.scaled(0.5);
        assert_eq!(
            (half.constant, half.damper, half.rumble),
            (-0.25, 0.5, 0.125)
        );
        let off = f.scaled(0.0);
        assert_eq!((off.constant, off.damper, off.rumble), (0.0, 0.0, 0.0));
        assert_eq!(f.scaled(2.0).damper, 1.0, "kept within range");
    }

    #[test]
    fn the_settings_reach_the_input_thread() {
        let s = Settings {
            steering_lock: SteeringLock::Degrees(540),
            force_feedback: 50,
            deadzone: 10,
            ..Settings::default()
        };
        let t = Tuning::of(&s);
        assert_eq!(t.lock, SteeringLock::Degrees(540));
        assert_eq!(t.ffb_gain, 0.5);
        assert!((t.deadzone - 0.10).abs() < 1e-6);
    }

    #[test]
    fn a_script_holds_each_step_until_the_next() {
        let s = Script::parse("10 start\n10.5 -\n20 accel steer=-0.5 view\n").unwrap();
        assert_eq!(s.at(5.0), Read::default());
        assert_eq!(s.at(10.2).buttons, Button::Start.bit());
        assert_eq!(s.at(12.0), Read::default());
        let r = s.at(25.0);
        assert_eq!((r.accel, r.steer), (1.0, -0.5));
        assert_eq!(r.buttons, Button::View.bit());
        assert!(Script::parse("1 jump").is_err());
    }
}
