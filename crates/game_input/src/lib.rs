//! Device-neutral player input.
//!
//! Platform hosts map keys, controller controls, touch controls, or remote
//! gestures into [`DigitalInput`] values. This crate owns held state and repeat
//! timing, then emits semantic [`InputAction`] events. The simulation still
//! receives only resolved `GameCommand` values from the client context.

use std::{collections::VecDeque, time::Duration};

use ultimate_fate_core::Direction;

const DIRECTION_COUNT: usize = 4;
const BUTTON_COUNT: usize = 4;
const MAX_REPEATS_PER_UPDATE: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GameplayButton {
    Primary = 0,
    Back = 1,
    Inspect = 2,
    Journal = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DigitalInput {
    Move(Direction),
    Button(GameplayButton),
    Menu,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputAction {
    Move(Direction),
    Button(GameplayButton),
    Menu,
}

impl From<DigitalInput> for InputAction {
    fn from(value: DigitalInput) -> Self {
        match value {
            DigitalInput::Move(direction) => Self::Move(direction),
            DigitalInput::Button(button) => Self::Button(button),
            DigitalInput::Menu => Self::Menu,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionEvent {
    pub action: InputAction,
    pub repeated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RepeatTiming {
    pub initial_delay: Duration,
    pub interval: Duration,
}

impl Default for RepeatTiming {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_millis(180),
            interval: Duration::from_millis(85),
        }
    }
}

#[derive(Debug)]
pub struct InputController {
    digital_directions: [bool; DIRECTION_COUNT],
    buttons: [bool; BUTTON_COUNT],
    menu: bool,
    most_recent_direction: Option<Direction>,
    axis_direction: Option<Direction>,
    repeat_remaining: Option<Duration>,
    repeat_timing: RepeatTiming,
    events: VecDeque<ActionEvent>,
}

impl Default for InputController {
    fn default() -> Self {
        Self::new(RepeatTiming::default())
    }
}

impl InputController {
    pub fn new(repeat_timing: RepeatTiming) -> Self {
        assert!(
            !repeat_timing.interval.is_zero(),
            "input repeat interval must be non-zero"
        );
        Self {
            digital_directions: [false; DIRECTION_COUNT],
            buttons: [false; BUTTON_COUNT],
            menu: false,
            most_recent_direction: None,
            axis_direction: None,
            repeat_remaining: None,
            repeat_timing,
            events: VecDeque::new(),
        }
    }

    /// Sets a digital control state. Repeated platform key-down events are safe:
    /// only the transition from released to pressed emits an action.
    pub fn set_digital(&mut self, input: DigitalInput, pressed: bool) {
        match input {
            DigitalInput::Move(direction) => self.set_direction(direction, pressed),
            DigitalInput::Button(button) => {
                let held = &mut self.buttons[button_index(button)];
                if *held == pressed {
                    return;
                }
                *held = pressed;
                if pressed {
                    self.push(input.into(), false);
                }
            }
            DigitalInput::Menu => {
                if self.menu == pressed {
                    return;
                }
                self.menu = pressed;
                if pressed {
                    self.push(InputAction::Menu, false);
                }
            }
        }
    }

    /// Quantizes a virtual joystick or controller stick to the dominant cardinal
    /// direction. Values inside `dead_zone` release the axis direction.
    pub fn set_movement_axis(&mut self, x: f32, y: f32, dead_zone: f32) {
        let direction = dominant_direction(x, y, dead_zone);
        if direction == self.axis_direction {
            return;
        }
        let previous = self.axis_direction;
        self.axis_direction = direction;
        if let Some(direction) = direction {
            self.most_recent_direction = Some(direction);
            self.repeat_remaining = Some(self.repeat_timing.initial_delay);
            if previous != Some(direction) && !self.digital_directions[direction_index(direction)] {
                self.push(InputAction::Move(direction), false);
            }
        } else if previous == self.most_recent_direction {
            self.most_recent_direction = None;
            self.repeat_remaining = self.active_direction().map(|_| self.repeat_timing.interval);
        }
    }

    /// Advances platform-independent repeat timing.
    pub fn update(&mut self, mut elapsed: Duration) {
        let Some(mut remaining) = self.repeat_remaining else {
            return;
        };

        let mut repeats = 0;
        while elapsed >= remaining && repeats < MAX_REPEATS_PER_UPDATE {
            elapsed -= remaining;
            let Some(direction) = self.active_direction() else {
                self.repeat_remaining = None;
                return;
            };
            self.push(InputAction::Move(direction), true);
            remaining = self.repeat_timing.interval;
            repeats += 1;
        }
        self.repeat_remaining = Some(remaining.saturating_sub(elapsed));
    }

    pub fn drain_events(&mut self) -> impl Iterator<Item = ActionEvent> + '_ {
        self.events.drain(..)
    }

    pub fn next_repeat_in(&self) -> Option<Duration> {
        self.repeat_remaining
    }

    pub fn clear(&mut self) {
        self.digital_directions = [false; DIRECTION_COUNT];
        self.buttons = [false; BUTTON_COUNT];
        self.menu = false;
        self.most_recent_direction = None;
        self.axis_direction = None;
        self.repeat_remaining = None;
        self.events.clear();
    }

    fn set_direction(&mut self, direction: Direction, pressed: bool) {
        let index = direction_index(direction);
        if self.digital_directions[index] == pressed {
            return;
        }
        let was_held = self.is_direction_held(direction);
        self.digital_directions[index] = pressed;

        if pressed {
            self.most_recent_direction = Some(direction);
            self.repeat_remaining = Some(self.repeat_timing.initial_delay);
            if !was_held {
                self.push(InputAction::Move(direction), false);
            }
        } else if self.most_recent_direction == Some(direction)
            && !self.is_direction_held(direction)
        {
            self.most_recent_direction = None;
            self.repeat_remaining = self.active_direction().map(|_| self.repeat_timing.interval);
        }
    }

    fn active_direction(&self) -> Option<Direction> {
        self.most_recent_direction
            .filter(|direction| self.is_direction_held(*direction))
            .or_else(|| {
                Direction::ALL
                    .into_iter()
                    .find(|direction| self.is_direction_held(*direction))
            })
    }

    fn is_direction_held(&self, direction: Direction) -> bool {
        self.digital_directions[direction_index(direction)]
            || self.axis_direction == Some(direction)
    }

    fn push(&mut self, action: InputAction, repeated: bool) {
        self.events.push_back(ActionEvent { action, repeated });
    }
}

fn direction_index(direction: Direction) -> usize {
    match direction {
        Direction::North => 0,
        Direction::East => 1,
        Direction::South => 2,
        Direction::West => 3,
    }
}

fn button_index(button: GameplayButton) -> usize {
    button as usize
}

fn dominant_direction(x: f32, y: f32, dead_zone: f32) -> Option<Direction> {
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    let dead_zone = dead_zone.clamp(0.0, 1.0);
    if x.abs().max(y.abs()) < dead_zone {
        return None;
    }
    if x.abs() > y.abs() {
        Some(if x > 0.0 {
            Direction::East
        } else {
            Direction::West
        })
    } else {
        Some(if y > 0.0 {
            Direction::South
        } else {
            Direction::North
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actions(input: &mut InputController) -> Vec<ActionEvent> {
        input.drain_events().collect()
    }

    #[test]
    fn held_movement_emits_initial_and_timed_repeat_events() {
        let mut input = InputController::default();
        input.set_digital(DigitalInput::Move(Direction::East), true);
        assert_eq!(
            actions(&mut input),
            vec![ActionEvent {
                action: InputAction::Move(Direction::East),
                repeated: false,
            }]
        );

        input.update(Duration::from_millis(179));
        assert!(actions(&mut input).is_empty());
        input.update(Duration::from_millis(1));
        assert_eq!(
            actions(&mut input),
            vec![ActionEvent {
                action: InputAction::Move(Direction::East),
                repeated: true,
            }]
        );

        input.set_digital(DigitalInput::Move(Direction::East), false);
        input.update(Duration::from_secs(1));
        assert!(actions(&mut input).is_empty());
    }

    #[test]
    fn latest_direction_wins_and_release_falls_back() {
        let mut input = InputController::default();
        input.set_digital(DigitalInput::Move(Direction::North), true);
        input.set_digital(DigitalInput::Move(Direction::East), true);
        let _ = actions(&mut input);

        input.update(Duration::from_millis(180));
        assert_eq!(
            actions(&mut input)[0].action,
            InputAction::Move(Direction::East)
        );
        input.set_digital(DigitalInput::Move(Direction::East), false);
        input.update(Duration::from_millis(85));
        assert_eq!(
            actions(&mut input)[0].action,
            InputAction::Move(Direction::North)
        );
    }

    #[test]
    fn buttons_emit_only_on_press_edges() {
        let mut input = InputController::default();
        let primary = DigitalInput::Button(GameplayButton::Primary);
        input.set_digital(primary, true);
        input.set_digital(primary, true);
        input.set_digital(primary, false);
        input.set_digital(primary, true);

        assert_eq!(actions(&mut input).len(), 2);
    }

    #[test]
    fn analog_axis_uses_dead_zone_and_dominant_direction() {
        let mut input = InputController::default();
        input.set_movement_axis(0.2, 0.1, 0.3);
        assert!(actions(&mut input).is_empty());

        input.set_movement_axis(-0.8, 0.4, 0.3);
        assert_eq!(
            actions(&mut input)[0].action,
            InputAction::Move(Direction::West)
        );
        input.set_movement_axis(0.0, 0.0, 0.3);
        input.update(Duration::from_secs(1));
        assert!(actions(&mut input).is_empty());
    }
}
