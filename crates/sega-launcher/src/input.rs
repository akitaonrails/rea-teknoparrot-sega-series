//! What this launcher hands the SDK's input thread
//! (`rea_launcher_native::feeder`): the cabinets' stick lines for
//! windowsarcadeloader ([`Cabinet`] as a [`Wire`]), the player's wheel
//! options ([`tuning`]), and the wheel lindbergh-loader reads itself
//! ([`lindbergh_wheel`]).

use rea_launcher::wheel::PedalAxis;
use rea_launcher_native::feeder::{self, Frame, InputService, Tuning, Wire};
use sega_core::input::{Button, Cabinet, StickLayout, default_pad_map};
use sega_core::launch::LindberghWheel;
use sega_core::settings::Settings;

/// The input thread with the cabinets' pad layout; a wheel's paddles shift.
pub fn service() -> InputService {
    InputService::start(default_pad_map(), Some((Button::GearUp, Button::GearDown)))
}

/// A windowsarcadeloader game's input: one `p1` line per change.
pub struct Stick(pub Cabinet);

impl Stick {
    pub fn boxed(layout: StickLayout) -> Box<dyn Wire> {
        Box::new(Stick(Cabinet::new(layout)))
    }
}

impl Wire for Stick {
    fn encode(&mut self, frame: &Frame) -> String {
        format!("{}\n", self.0.pipe_line(&frame.controls))
    }

    /// windowsarcadeloader ends the game and gives back what it set up.
    fn quit(&self) -> Option<&'static str> {
        Some("quit")
    }
}

/// The player's wheel and pad options for one run.
pub fn tuning(s: &Settings) -> Tuning {
    Tuning {
        deadzone: s.deadzone_fraction(),
        lock: s.steering_lock,
        ffb_gain: s.force_feedback_gain(),
    }
}

/// The first wheel SDL sees, as lindbergh-loader reads it (a short look
/// before the game starts; `None` while the input thread holds SDL).
pub fn lindbergh_wheel() -> Option<LindberghWheel> {
    let w = feeder::find_wheel()?;
    // a pedal resting at the top of its range reads backwards
    let pedal = |p: Option<PedalAxis>| p.map(|a| (a.axis, a.rest > 0));
    Some(LindberghWheel {
        guid: w.guid,
        steer_axis: w.steer_axis,
        gas: pedal(w.gas),
        brake: pedal(w.brake),
        paddles: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rea_launcher::wheel::{Control, Controls, SteeringLock};

    #[test]
    fn the_settings_reach_the_input_thread() {
        let s = Settings {
            steering_lock: SteeringLock::Degrees(540),
            force_feedback: 50,
            deadzone: 10,
            ..Settings::default()
        };
        let t = tuning(&s);
        assert_eq!(t.lock, SteeringLock::Degrees(540));
        assert_eq!(t.ffb_gain, 0.5);
        assert!((t.deadzone - 0.10).abs() < 1e-6);
    }

    #[test]
    fn the_stick_writes_one_line_per_frame_and_quits_politely() {
        let mut wire = Stick(Cabinet::new(StickLayout::Sequential));
        let frame = Frame {
            controls: Controls {
                buttons: Button::Start.bit(),
                ..Controls::default()
            },
            ..Frame::default()
        };
        assert_eq!(wire.encode(&frame), "p1 1000 0 0 0 0 0 0\n");
        assert_eq!(wire.quit(), Some("quit"));
    }
}
