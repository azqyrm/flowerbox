//! Input actions and bindings (leafwing-input-manager), plus cursor grabbing.
//!
//! Raw input is turned into a [`PlayerInput`] component in `Update`; the fixed-timestep
//! gameplay systems only read [`PlayerInput`], so presses are never lost or doubled.

use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use leafwing_input_manager::prelude::*;

use crate::camera::ThirdPersonCamera;
use crate::tuning::*;

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputManagerPlugin::<Action>::default())
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
    Jump,
    /// Tap = roll, hold = sprint.
    DodgeSprint,
    /// Tap = light, hold = heavy, airborne = jump attack.
    Attack,
    DrinkAle,
    Interact,
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
    .with(Action::LockOn, MouseButton::Middle)
    .with_dual_axis(Action::Move, VirtualDPad::wasd())
    .with_dual_axis(Action::Look, MouseMove::default())
}

/// Per-frame input already translated into gameplay terms. Lives on the player entity.
#[derive(Component, Default, Debug)]
pub struct PlayerInput {
    /// World-space, camera-relative, horizontal move direction (length 0..=1).
    pub move_dir: Vec3,
    pub sprint: bool,
    /// Seconds left in which a jump press is still honoured (input buffer).
    pub jump_buffer: f32,
    /// How long Shift has been held (for tap-vs-hold).
    pub dodge_held: f32,
}

/// `FLOWERBOX_AUTOPILOT=1` makes the player walk on its own (for automated testing, see ROADMAP N7).
#[derive(Resource)]
pub struct Autopilot;

fn gather_player_input(
    time: Res<Time>,
    autopilot: Option<Res<Autopilot>>,
    cam: Query<&ThirdPersonCamera>,
    mut players: Query<(&ActionState<Action>, &mut PlayerInput)>,
) {
    let yaw = cam.iter().next().map_or(0.0, |c| c.yaw);
    // Camera-relative basis on the ground plane (camera looks along -Z at yaw 0).
    let forward = -Vec3::new(yaw.sin(), 0.0, yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let dt = time.delta_secs();

    for (actions, mut input) in &mut players {
        let mut stick = actions.clamped_axis_pair(&Action::Move);
        let mut jump = actions.just_pressed(&Action::Jump);
        if autopilot.is_some() {
            let t = time.elapsed_secs();
            stick = Vec2::new(0.0, 1.0); // walk forward (toward the arena center, then the wall)
            jump = (t % 2.0) < dt && t > 1.0;
        }
        input.move_dir = (right * stick.x + forward * stick.y).clamp_length_max(1.0);

        input.jump_buffer = if jump { JUMP_BUFFER } else { (input.jump_buffer - dt).max(0.0) };

        input.dodge_held = if actions.pressed(&Action::DodgeSprint) { input.dodge_held + dt } else { 0.0 };
        input.sprint = input.dodge_held >= SPRINT_HOLD_THRESHOLD;
    }
}

fn set_grab(options: &mut CursorOptions, grab: bool) {
    options.grab_mode = if grab { CursorGrabMode::Locked } else { CursorGrabMode::None };
    options.visible = !grab;
}

fn grab_cursor(mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>) {
    set_grab(&mut cursor, true);
}

/// Esc releases the cursor, a left click grabs it again.
fn toggle_cursor(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
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
