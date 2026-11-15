//! The cabinet's buttons, the default pad layout, and the step from the
//! SDK's device-independent controls to what each loader reads.
//!
//! windowsarcadeloader's payloads read a virtual arcade stick per player
//! (its `wal_protocol`): buttons as bits, six axes. Which bit is which
//! cabinet control depends on the game's system profile; [`Cabinet`] holds
//! the layouts of the games here (read from their profiles' `controls`,
//! `native_map` and `gamepad` sections at the pinned commit) and the state
//! a sequential shift needs to drive an H-pattern lever.

use rea_launcher::wheel::{Action, Control, Controls as Read, PadInput, PadMap};

/// The buttons of the Sega cabinets here (besides wheel and pedals). A
/// pad's shoulders or a wheel's paddles shift up and down; the games with
/// an H-pattern lever get the gear that sequence selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Button {
    GearUp,
    GearDown,
    View,
    Handbrake,
    Start,
    Coin,
    Test,
    Service,
}

impl Control for Button {
    const ALL: &'static [Self] = &[
        Button::GearUp,
        Button::GearDown,
        Button::View,
        Button::Handbrake,
        Button::Start,
        Button::Coin,
        Button::Test,
        Button::Service,
    ];

    fn key(self) -> &'static str {
        match self {
            Button::GearUp => "gear_up",
            Button::GearDown => "gear_down",
            Button::View => "view",
            Button::Handbrake => "handbrake",
            Button::Start => "start",
            Button::Coin => "coin",
            Button::Test => "test",
            Button::Service => "service",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Button::GearUp => "Shift up",
            Button::GearDown => "Shift down",
            Button::View => "View change",
            Button::Handbrake => "Handbrake",
            Button::Start => "Start",
            Button::Coin => "Insert coin",
            Button::Test => "Operator test menu",
            Button::Service => "Service",
        }
    }

    fn bit(self) -> u32 {
        1 << Self::ALL.iter().position(|&b| b == self).expect("in ALL")
    }
}

/// The pad layout a player gets before changing anything: triggers for the
/// pedals, the left stick steers, shoulders shift, Y changes the view, B
/// pulls the handbrake.
pub fn default_pad_map() -> PadMap<Button> {
    PadMap::with(&[
        (Action::Accelerate, &[PadInput::RightTrigger]),
        (Action::Brake, &[PadInput::LeftTrigger]),
        (Action::Press(Button::GearUp), &[PadInput::RightShoulder]),
        (Action::Press(Button::GearDown), &[PadInput::LeftShoulder]),
        (Action::Press(Button::View), &[PadInput::Y]),
        (Action::Press(Button::Handbrake), &[PadInput::B]),
        (
            Action::Press(Button::Start),
            &[PadInput::Start, PadInput::A],
        ),
        (Action::Press(Button::Coin), &[PadInput::Back]),
    ])
}

/// windowsarcadeloader's virtual stick bits (`wal_protocol::button`).
pub mod wal {
    pub const UP: u32 = 1 << 0;
    pub const DOWN: u32 = 1 << 1;
    pub const LEFT: u32 = 1 << 2;
    pub const RIGHT: u32 = 1 << 3;
    pub const B1: u32 = 1 << 4;
    pub const B2: u32 = 1 << 5;
    pub const B3: u32 = 1 << 6;
    pub const B4: u32 = 1 << 7;
    pub const START: u32 = 1 << 12;
    pub const COIN: u32 = 1 << 13;
    pub const SERVICE: u32 = 1 << 14;
    pub const TEST: u32 = 1 << 15;
}

/// Which cabinet a windowsarcadeloader game has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StickLayout {
    /// SEGA Rally 3 (`europar/sega-rally-3`): b1/b2 shift up/down, b3 view
    /// change, b4 handbrake.
    Sequential,
    /// SEGA Racing Classic (`ringedge/sega-racing-classic`): a 4-speed H
    /// lever on b1–b4 (gear 1–4, held while in that gear; none is
    /// neutral) and four view buttons on up/down/left/right.
    HPattern4,
    /// Daytona Championship USA (`segashell/daytona-championship-usa`,
    /// patches/wal/0004): shift on b1/b2, the four view buttons on
    /// up/down/left/right.
    Shell,
}

/// Turns the player's controls into one game's pipe lines, keeping the
/// state a sequential shift and a view button need on an H-pattern
/// cabinet: the gear selected and the view last chosen.
#[derive(Debug, Clone)]
pub struct Cabinet {
    layout: StickLayout,
    /// 1–4 on an H lever (the race starts in first).
    gear: u8,
    /// 0–3: which of the four view buttons the view button presses next.
    view: u8,
    prev: u32,
}

const VIEWS: [u32; 4] = [wal::UP, wal::DOWN, wal::LEFT, wal::RIGHT];
const GEARS: [u32; 4] = [wal::B1, wal::B2, wal::B3, wal::B4];

impl Cabinet {
    pub fn new(layout: StickLayout) -> Self {
        Cabinet {
            layout,
            gear: 1,
            view: 0,
            prev: 0,
        }
    }

    pub fn layout(&self) -> StickLayout {
        self.layout
    }

    /// The gear an H-pattern cabinet is in (1–4).
    pub fn gear(&self) -> u8 {
        self.gear
    }

    /// The stick's button bits for these controls (one step: call once
    /// per read, in order).
    pub fn buttons(&mut self, r: &Read) -> u32 {
        let pressed = |b: Button| r.buttons & b.bit() != 0;
        let edge = |b: Button| pressed(b) && self.prev & b.bit() == 0;
        let mut bits = 0;
        for (b, bit) in [
            (Button::Start, wal::START),
            (Button::Coin, wal::COIN),
            (Button::Test, wal::TEST),
            (Button::Service, wal::SERVICE),
        ] {
            if pressed(b) {
                bits |= bit;
            }
        }
        let view = |bits: &mut u32, prev: u32, view: &mut u8| {
            if pressed(Button::View) {
                *bits |= VIEWS[usize::from(*view)];
            } else if prev & Button::View.bit() != 0 {
                // released: the next press chooses the next view
                *view = (*view + 1) % 4;
            }
        };
        match self.layout {
            StickLayout::Shell => {
                for (b, bit) in [(Button::GearUp, wal::B1), (Button::GearDown, wal::B2)] {
                    if pressed(b) {
                        bits |= bit;
                    }
                }
                view(&mut bits, self.prev, &mut self.view);
            }
            StickLayout::Sequential => {
                for (b, bit) in [
                    (Button::GearUp, wal::B1),
                    (Button::GearDown, wal::B2),
                    (Button::View, wal::B3),
                    (Button::Handbrake, wal::B4),
                ] {
                    if pressed(b) {
                        bits |= bit;
                    }
                }
            }
            StickLayout::HPattern4 => {
                if edge(Button::GearUp) {
                    self.gear = (self.gear + 1).min(4);
                }
                if edge(Button::GearDown) {
                    self.gear = self.gear.saturating_sub(1).max(1);
                }
                bits |= GEARS[usize::from(self.gear - 1)];
                view(&mut bits, self.prev, &mut self.view);
            }
        }
        self.prev = r.buttons;
        bits
    }

    /// Player 1's line for `arcade-launcher --input-pipe`
    /// (`p1 <buttons hex> <lx> <ly> <rx> <ry> <accel> <brake>`).
    pub fn pipe_line(&mut self, r: &Read) -> String {
        let buttons = self.buttons(r);
        let axis = |v: f32| (v.clamp(-1.0, 1.0) * 32767.0).round() as i32;
        let pedal = |v: f32| (v.clamp(0.0, 1.0) * 32767.0).round() as i32;
        format!(
            "p1 {buttons:x} {} 0 0 0 {} {}",
            axis(r.steer),
            pedal(r.accel),
            pedal(r.brake)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rea_launcher::wheel::{PadSettings, PadState, read_pad};

    fn read(buttons: &[Button]) -> Read {
        Read {
            buttons: buttons.iter().fold(0, |a, b| a | b.bit()),
            ..Read::default()
        }
    }

    #[test]
    fn bits_are_distinct() {
        let mut seen = 0u32;
        for b in Button::ALL {
            assert_eq!(seen & b.bit(), 0);
            seen |= b.bit();
        }
    }

    #[test]
    fn sega_rally_gets_its_shifts_view_and_handbrake() {
        let mut c = Cabinet::new(StickLayout::Sequential);
        let r = read(&[Button::GearUp, Button::View, Button::Handbrake]);
        assert_eq!(c.buttons(&r), wal::B1 | wal::B3 | wal::B4);
        assert_eq!(c.buttons(&read(&[Button::GearDown])), wal::B2);
        assert_eq!(c.buttons(&read(&[])), 0);
    }

    #[test]
    fn a_sequential_shift_moves_the_h_lever_one_gear_per_press() {
        let mut c = Cabinet::new(StickLayout::HPattern4);
        // first gear is held from the start
        assert_eq!(c.buttons(&read(&[])), wal::B1);
        // held paddle: one gear, not one per read
        for _ in 0..5 {
            assert_eq!(c.buttons(&read(&[Button::GearUp])), wal::B2);
        }
        c.buttons(&read(&[]));
        c.buttons(&read(&[Button::GearUp]));
        c.buttons(&read(&[]));
        c.buttons(&read(&[Button::GearUp]));
        c.buttons(&read(&[]));
        assert_eq!(c.gear(), 4);
        // past fourth stays in fourth
        c.buttons(&read(&[Button::GearUp]));
        assert_eq!(c.buttons(&read(&[])), wal::B4);
        for _ in 0..6 {
            c.buttons(&read(&[Button::GearDown]));
            c.buttons(&read(&[]));
        }
        assert_eq!(c.gear(), 1, "never below first");
    }

    #[test]
    fn the_view_button_walks_through_the_four_view_buttons() {
        let mut c = Cabinet::new(StickLayout::HPattern4);
        let mut seen = Vec::new();
        for _ in 0..5 {
            seen.push(c.buttons(&read(&[Button::View])) & !wal::B1);
            c.buttons(&read(&[]));
        }
        assert_eq!(seen, [wal::UP, wal::DOWN, wal::LEFT, wal::RIGHT, wal::UP]);
    }

    #[test]
    fn daytona_shifts_on_b1_b2_and_walks_the_views() {
        let mut c = Cabinet::new(StickLayout::Shell);
        assert_eq!(c.buttons(&read(&[])), 0, "no gear held");
        assert_eq!(c.buttons(&read(&[Button::GearUp])), wal::B1);
        assert_eq!(c.buttons(&read(&[Button::GearDown])), wal::B2);
        c.buttons(&read(&[]));
        assert_eq!(c.buttons(&read(&[Button::View])), wal::UP);
        c.buttons(&read(&[]));
        assert_eq!(c.buttons(&read(&[Button::View])), wal::DOWN);
    }

    #[test]
    fn a_pad_steers_and_presses_the_pedals() {
        let settings = PadSettings::new(default_pad_map());
        let pad = PadState {
            left_x: -32768,
            right_trigger: 32767,
            start: true,
            ..PadState::default()
        };
        let r = read_pad(&settings, &pad, None);
        let line = Cabinet::new(StickLayout::Sequential).pipe_line(&r);
        assert_eq!(line, "p1 1000 -32767 0 0 0 32767 0");
    }
}
