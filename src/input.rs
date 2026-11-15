//! Input actions and bindings (leafwing-input-manager), plus cursor grabbing.
//!
//! Raw input is turned into a [`PlayerInput`] component in `Update`; the fixed-timestep
//! gameplay systems only read [`PlayerInput`], so presses are never lost or doubled.

use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use leafwing_input_manager::prelude::*;

use crate::camera::ThirdPersonCamera;
use crate::character::{ActionIntent, MoveIntent};
use crate::character::action::ActionRequest;
use crate::tuning::*;

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputManagerPlugin::<Action>::default())
            .init_resource::<GameplayBlocked>()
            .add_systems(Startup, grab_cursor)
            .add_systems(Update, (toggle_cursor, gather_player_input));
    }
}

/// Every action the player can perform. Bindings: see [`default_input_map`] and ROADMAP §2.
#[derive(Actionlike, PartialEq, Eq, Hash, Clone, Copy, Debug, Reflect)]
pub enum Action {
    #[actionlike(DualAxis)]
    Move,
    #[actionlike(DualAxis)]
    Look,
    /// Gamepad right stick (rate-based look; the mouse is `Look`).
    #[actionlike(DualAxis)]
    LookStick,
    Jump,
    /// Tap = roll, hold = sprint.
    DodgeSprint,
    /// Tap = light, hold = heavy, airborne = jump attack.
    Attack,
    DrinkAle,
    Interact,
    /// Hold to raise the shield (tap at the right time to parry).
    Block,
    LockOn,
    Inventory,
    Pause,
}

pub fn default_input_map() -> InputMap<Action> {
    InputMap::new([
        (Action::Jump, KeyCode::Space),
        (Action::DodgeSprint, KeyCode::ShiftLeft),
        (Action::DrinkAle, KeyCode::KeyR),
        (Action::Interact, KeyCode::KeyE),
        (Action::LockOn, KeyCode::KeyQ),
        (Action::Inventory, KeyCode::Tab),
        (Action::Inventory, KeyCode::KeyI),
        (Action::Pause, KeyCode::Escape),
    ])
    .with(Action::Attack, MouseButton::Left)
    .with(Action::Block, MouseButton::Right)
    .with(Action::LockOn, MouseButton::Middle)
    .with_dual_axis(Action::Move, VirtualDPad::wasd())
    .with_dual_axis(Action::Look, MouseMove::default())
    // Gamepad (Souls-like layout): left stick move, right stick camera, R1 attack (tap/hold),
    // L1 block, B/Circle roll/sprint, A/Cross jump, X/Square drink, Y/Triangle interact,
    // R3 lock-on, Start inventory.
    .with_dual_axis(Action::Move, GamepadStick::LEFT)
    .with_dual_axis(Action::LookStick, GamepadStick::RIGHT)
    .with(Action::Attack, GamepadButton::RightTrigger)
    .with(Action::Block, GamepadButton::LeftTrigger)
    .with(Action::DodgeSprint, GamepadButton::East)
    .with(Action::Jump, GamepadButton::South)
    .with(Action::DrinkAle, GamepadButton::West)
    .with(Action::Interact, GamepadButton::North)
    .with(Action::LockOn, GamepadButton::RightThumb)
    .with(Action::Inventory, GamepadButton::Start)
}

/// True while a menu (the inventory) has the input: the character gets no movement or actions,
/// the camera doesn't turn and clicks don't grab the cursor. The world keeps running.
#[derive(Resource, Default, Debug)]
pub struct GameplayBlocked(pub bool);

/// Player-only input state (button timings). Gameplay reads the generic
/// [`MoveIntent`] / [`ActionIntent`] that `gather_player_input` fills from it.
#[derive(Component, Default, Debug)]
pub struct PlayerInput {
    /// How long Shift has been held (for tap-vs-hold).
    pub dodge_held: f32,
    pub attack: AttackTracker,
    /// E was pressed and nothing has used it yet (the Hearth consumes it).
    pub interact: bool,
}

/// Tap-vs-hold logic of the attack button. Pure so it can be unit tested.
#[derive(Default, Debug, Clone)]
pub struct AttackTracker {
    /// How long the current press has lasted.
    pub held: f32,
    /// The current press started while the game had the cursor (so it's a real attack press).
    pub armed: bool,
    heavy_sent: bool,
}

impl AttackTracker {
    /// Feed one frame of button state; returns a request when one should be queued.
    /// Tap (released before the threshold) = light, holding past the threshold = heavy (charge).
    pub fn update(&mut self, pressed: bool, just_pressed: bool, just_released: bool, grabbed: bool, dt: f32) -> Option<ActionRequest> {
        let mut request = None;
        if just_pressed {
            *self = Self { armed: grabbed, ..default() };
        }
        if self.armed && pressed && !just_pressed {
            self.held += dt;
            if self.held >= ATTACK_HOLD_THRESHOLD && !self.heavy_sent {
                self.heavy_sent = true;
                request = Some(ActionRequest::HeavyAttack);
            }
        }
        if just_released {
            if self.armed && !self.heavy_sent {
                request = Some(ActionRequest::LightAttack);
            }
            *self = Self::default();
        }
        request
    }

    pub fn is_holding(&self) -> bool {
        self.armed
    }
}

/// `FLOWERBOX_AUTOPILOT=...` makes the player act on its own (automated testing, see ROADMAP N7, N12).
/// Scripts are defined in `debug.rs`.
#[derive(Resource, Default)]
pub struct Autopilot {
    /// Scripted movement: (time, dt) -> (world direction, sprint, jump pressed).
    pub movement: Option<fn(f32, f32) -> (Vec3, bool, bool)>,
    /// Scripted attack button: (time, dt) -> (pressed, just_pressed, just_released).
    pub attack_button: Option<fn(f32, f32) -> (bool, bool, bool)>,
    /// Scripted block button: time -> held.
    pub guard_button: Option<fn(f32) -> bool>,
}

fn gather_player_input(
    time: Res<Time>,
    blocked: Res<GameplayBlocked>,
    autopilot: Option<Res<Autopilot>>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    cam: Query<&ThirdPersonCamera>,
    mut players: Query<(&ActionState<Action>, &mut PlayerInput, &mut MoveIntent, &mut ActionIntent)>,
) {
    let yaw = cam.iter().next().map_or(0.0, |c| c.yaw);
    // Camera-relative basis on the ground plane (camera looks along -Z at yaw 0).
    let forward = -Vec3::new(yaw.sin(), 0.0, yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let dt = time.delta_secs();
    let t = time.elapsed_secs();

    for (actions, mut input, mut movement, mut intent) in &mut players {
        intent.tick(dt);
        if blocked.0 {
            // A menu is open: stand still, drop held buttons (no sprint, no charging heavy).
            movement.dir = Vec3::ZERO;
            movement.sprint = false;
            input.dodge_held = 0.0;
            input.attack = AttackTracker::default();
            input.interact = false;
            intent.attack_held = false;
            intent.guard_held = false;
            continue;
        }

        // Movement: WASD relative to the camera (or the autopilot's script).
        let stick = actions.clamped_axis_pair(&Action::Move);
        let mut dir = (right * stick.x + forward * stick.y).clamp_length_max(1.0);
        let mut jump = actions.just_pressed(&Action::Jump);
        let mut scripted_sprint = None;
        if let Some(script) = autopilot.as_ref().and_then(|a| a.movement) {
            let (d, sprint, j) = script(t, dt);
            (dir, jump, scripted_sprint) = (d, j, Some(sprint));
        }
        movement.dir = dir;
        movement.jump_buffer = if jump { JUMP_BUFFER } else { (movement.jump_buffer - dt).max(0.0) };

        // Shift: a short tap rolls on release, holding sprints.
        if actions.just_released(&Action::DodgeSprint) && is_dodge_tap(input.dodge_held) {
            intent.buffer(ActionRequest::Roll);
        }
        input.dodge_held = if actions.pressed(&Action::DodgeSprint) { input.dodge_held + dt } else { 0.0 };
        movement.sprint = scripted_sprint.unwrap_or(input.dodge_held >= SPRINT_HOLD_THRESHOLD);

        // Attack button. Ignored while the cursor is free, so the click that re-grabs it
        // doesn't swing the sword. The autopilot may script the button instead.
        let mut attack = (
            actions.pressed(&Action::Attack),
            actions.just_pressed(&Action::Attack),
            actions.just_released(&Action::Attack),
        );
        let mut grabbed = cursor_grabbed(&cursor);
        if let Some(script) = autopilot.as_ref().and_then(|a| a.attack_button) {
            attack = script(t, dt);
            grabbed = true;
        }
        if let Some(request) = input.attack.update(attack.0, attack.1, attack.2, grabbed, dt) {
            intent.buffer(request);
        }
        intent.attack_held = attack.0 && input.attack.is_holding();
        // Block: only while the game has the cursor (a right click on a free cursor isn't a block).
        intent.guard_held = match autopilot.as_ref().and_then(|a| a.guard_button) {
            Some(script) => script(t),
            None => actions.pressed(&Action::Block) && cursor_grabbed(&cursor),
        };

        if actions.just_pressed(&Action::DrinkAle) {
            intent.buffer(ActionRequest::DrinkAle);
        }
        if actions.just_pressed(&Action::Interact) {
            input.interact = true;
        }
    }
}

/// A Shift press released before sprint kicked in counts as a roll (Souls-style: roll on release).
pub fn is_dodge_tap(held_seconds: f32) -> bool {
    held_seconds < SPRINT_HOLD_THRESHOLD
}

pub fn set_grab(options: &mut CursorOptions, grab: bool) {
    options.grab_mode = if grab { CursorGrabMode::Locked } else { CursorGrabMode::None };
    options.visible = !grab;
}

fn grab_cursor(mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>) {
    set_grab(&mut cursor, true);
}

/// Esc releases the cursor, a left click grabs it again.
pub fn toggle_cursor(
    blocked: Res<GameplayBlocked>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if blocked.0 {
        return; // the menu owns the cursor
    }
    if keys.just_pressed(KeyCode::Escape) {
        set_grab(&mut cursor, false);
    } else if mouse.just_pressed(MouseButton::Left) && cursor.grab_mode == CursorGrabMode::None {
        set_grab(&mut cursor, true);
    }
}

/// True while the cursor is captured by the game (camera look should only apply then).
pub fn cursor_grabbed(cursor: &CursorOptions) -> bool {
    cursor.grab_mode != CursorGrabMode::None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simulates pressing for `hold` seconds at 60 fps; returns all requests produced.
    fn press(tracker: &mut AttackTracker, hold: f32, grabbed: bool) -> Vec<ActionRequest> {
        let dt = 1.0 / 60.0;
        let mut out = vec![];
        out.extend(tracker.update(true, true, false, grabbed, dt));
        let mut t = 0.0;
        while t < hold {
            out.extend(tracker.update(true, false, false, true, dt));
            t += dt;
        }
        out.extend(tracker.update(false, false, true, true, dt));
        out
    }

    #[test]
    fn attack_tap_is_light_hold_is_heavy() {
        let mut t = AttackTracker::default();
        assert_eq!(press(&mut t, 0.1, true), vec![ActionRequest::LightAttack]);
        assert_eq!(press(&mut t, 0.6, true), vec![ActionRequest::HeavyAttack], "heavy fires once, no light on release");
    }

    #[test]
    fn click_that_grabs_the_cursor_does_not_attack() {
        let mut t = AttackTracker::default();
        assert!(press(&mut t, 0.1, false).is_empty());
        assert!(press(&mut t, 0.6, false).is_empty());
    }

    #[test]
    fn short_shift_press_rolls_long_press_sprints() {
        assert!(is_dodge_tap(0.0));
        assert!(is_dodge_tap(SPRINT_HOLD_THRESHOLD - 0.01));
        assert!(!is_dodge_tap(SPRINT_HOLD_THRESHOLD));
        assert!(!is_dodge_tap(2.0));
    }
}
