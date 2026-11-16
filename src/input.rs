//! Input actions and bindings (leafwing-input-manager), plus cursor grabbing.
//!
//! Raw input is turned into a [`PlayerInput`] component in `Update`; the fixed-timestep
//! gameplay systems only read [`PlayerInput`], so presses are never lost or doubled.

use bevy::prelude::*;
use crate::weapon::AttackHand;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use leafwing_input_manager::prelude::*;

use crate::camera::ThirdPersonCamera;
use crate::character::{ActionIntent, MoveIntent};
use crate::character::action::ActionRequest;
use crate::settings::{Bind, Bindings, Input, Settings};
use crate::items::inventory::TwoHandRequest;
use crate::tuning::*;
use crate::weapon::{Hand, Wielded};

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputManagerPlugin::<Action>::default())
            .init_resource::<GameplayBlocked>()
            .init_resource::<MenuInput>()
            .configure_sets(Update, (InputSystems::Menus, InputSystems::Apply, InputSystems::Cursor, InputSystems::Gather).chain())
            .add_systems(Startup, grab_cursor)
            .add_systems(
                Update,
                (
                    toggle_cursor.in_set(InputSystems::Cursor),
                    apply_menu_block.in_set(InputSystems::Apply),
                    gather_player_input.in_set(InputSystems::Gather),
                    rebind_player,
                ),
            );
    }
}

/// Every action the player can perform. Bindings: see [`input_map`] (rebindable in the settings)
/// and ROADMAP §2.
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
    /// Hold + a hand's attack button: hold that hand's weapon in both hands. Alone: lock on.
    TwoHand,
    /// The main weapon's ability.
    Ability,
    /// Gamepad Y: held + R1 / L1 two-hands that hand's weapon (Souls); tapped alone it interacts.
    PadY,
}

/// The player's input map: the rebindable keyboard/mouse bindings from the settings, plus fixed
/// extras (I for the inventory, middle mouse for lock-on, mouse look) and the gamepad layout.
pub fn input_map(bindings: &Bindings) -> InputMap<Action> {
    let mut map = InputMap::default();
    let bind = |map: &mut InputMap<Action>, action: Action, input: Input| match input {
        Input::Key(k) => {
            map.insert(action, k);
        }
        Input::Mouse(m) => {
            map.insert(action, m);
        }
    };
    for (b, action) in [
        (Bind::Jump, Action::Jump),
        (Bind::DodgeSprint, Action::DodgeSprint),
        (Bind::Attack, Action::Attack),
        (Bind::Block, Action::Block),
        (Bind::DrinkAle, Action::DrinkAle),
        (Bind::Interact, Action::Interact),
        (Bind::LockOn, Action::LockOn),
        (Bind::Inventory, Action::Inventory),
        (Bind::TwoHand, Action::TwoHand),
        (Bind::Ability, Action::Ability),
    ] {
        bind(&mut map, action, bindings.get(b));
    }
    // Movement keys (the settings only allow keys here).
    let key = |b: Bind| match bindings.get(b) {
        Input::Key(k) => k,
        Input::Mouse(_) => KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Unidentified),
    };
    let dpad = VirtualDPad::new(key(Bind::MoveForward), key(Bind::MoveBack), key(Bind::MoveLeft), key(Bind::MoveRight));
    map.with(Action::Inventory, KeyCode::KeyI)
        .with(Action::Pause, KeyCode::Escape)
        .with_dual_axis(Action::Move, dpad)
        .with_dual_axis(Action::Look, MouseMove::default())
    // Gamepad (Souls-like layout): left stick move, right stick camera, R1 attack (tap/hold),
    // L1 block, B/Circle roll/sprint, A/Cross jump, X/Square drink, Y/Triangle interact (held +
    // R1 / L1: two-hand that weapon), R3 lock-on, Start inventory.
    .with_dual_axis(Action::Move, GamepadStick::LEFT)
    .with_dual_axis(Action::LookStick, GamepadStick::RIGHT)
    .with(Action::Attack, GamepadButton::RightTrigger)
    .with(Action::Block, GamepadButton::LeftTrigger)
    .with(Action::DodgeSprint, GamepadButton::East)
    .with(Action::Jump, GamepadButton::South)
    .with(Action::DrinkAle, GamepadButton::West)
    .with(Action::PadY, GamepadButton::North)
    .with(Action::LockOn, GamepadButton::RightThumb)
    .with(Action::Inventory, GamepadButton::Start)
    .with(Action::Ability, GamepadButton::LeftTrigger2)
}

/// New key bindings take effect at once.
fn rebind_player(settings: Res<Settings>, mut maps: Query<&mut InputMap<Action>, With<crate::player::Player>>) {
    if settings.is_changed() && !settings.is_added() {
        for mut map in &mut maps {
            *map = input_map(&settings.bindings);
        }
    }
}

/// True while a menu has the input: the character gets no movement or actions, the camera doesn't
/// turn, clicks don't grab the cursor (the mouse works in the menu). The world keeps running.
/// Derived from [`MenuInput`] each frame — menus never write it themselves.
#[derive(Resource, Default, Debug)]
pub struct GameplayBlocked(pub bool);

/// Which menus are open. Each menu sets only its own flag (in [`InputSystems::Menus`]);
/// [`apply_menu_block`] then derives [`GameplayBlocked`] and frees or grabs the cursor. (Before,
/// every menu wrote `GameplayBlocked` directly and the closed inventory kept un-blocking the open
/// pause menu every frame: movement leaked through and the cursor kept being re-grabbed.)
#[derive(Resource, Default, Debug)]
pub struct MenuInput {
    pub inventory: bool,
    pub pause: bool,
    pub online: bool,
}

impl MenuInput {
    pub fn any(&self) -> bool {
        self.inventory || self.pause || self.online
    }
}

/// Order of the input systems each frame: Esc/click cursor toggle (sees last frame's block),
/// menus open/close, the block is applied, then gameplay input is gathered.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum InputSystems {
    Cursor,
    Menus,
    Apply,
    Gather,
}

/// Derives the block from the menus; the cursor follows in [`toggle_cursor`] (next frame's
/// `Cursor` set — the frame a menu closes still counts as blocked anyway).
fn apply_menu_block(menus: Res<MenuInput>, mut blocked: ResMut<GameplayBlocked>) {
    let want = menus.any();
    if blocked.0 != want {
        blocked.0 = want;
    }
}

/// Player-only input state (button timings). Gameplay reads the generic
/// [`MoveIntent`] / [`ActionIntent`] that `gather_player_input` fills from it.
#[derive(Component, Default, Debug)]
pub struct PlayerInput {
    /// How long Shift has been held (for tap-vs-hold).
    pub dodge_held: f32,
    pub attack: AttackTracker,
    /// The left hand's attacks (the right button, when the left hand holds a weapon).
    pub offhand: AttackTracker,
    /// E was pressed and nothing has used it yet (the Hearth consumes it).
    pub interact: bool,
    /// The two-hand button is down and was already used for a combo (then releasing it doesn't
    /// lock on).
    two_hand_combo: bool,
    /// The two-hand button was tapped alone: lock on (`crate::lockon` consumes it).
    pub lock_on_tap: bool,
    /// Gamepad Y was used as the two-hand modifier this press (so its release doesn't interact).
    pub pad_combo: bool,
}

/// What the right (left-hand) button does, from what the hands hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offhand {
    /// Raise the shield (or the two-handed weapon); nothing without either.
    Guard,
    /// Attack with the left weapon, or both at once (a matching pair).
    Attack(AttackHand),
}

impl Offhand {
    pub fn of(wielded: Option<&Wielded>) -> Self {
        match wielded {
            Some(w) if w.dual_class().is_some() => Offhand::Attack(AttackHand::Both),
            Some(w) if w.left_attacks() => Offhand::Attack(AttackHand::Left),
            _ => Offhand::Guard,
        }
    }
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
    pub fn update(&mut self, hand: AttackHand, pressed: bool, just_pressed: bool, just_released: bool, grabbed: bool, dt: f32) -> Option<ActionRequest> {
        let mut request = None;
        if just_pressed {
            *self = Self { armed: grabbed, ..default() };
        }
        if self.armed && pressed && !just_pressed {
            self.held += dt;
            if self.held >= ATTACK_HOLD_THRESHOLD && !self.heavy_sent {
                self.heavy_sent = true;
                request = Some(ActionRequest::HeavyAttack(hand));
            }
        }
        if just_released {
            if self.armed && !self.heavy_sent {
                request = Some(ActionRequest::LightAttack(hand));
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
    mut two_hand: MessageWriter<TwoHandRequest>,
    mut players: Query<(Entity, &ActionState<Action>, &mut PlayerInput, &mut MoveIntent, &mut ActionIntent, Option<&Wielded>)>,
) {
    let yaw = cam.iter().next().map_or(0.0, |c| c.yaw);
    // Camera-relative basis on the ground plane (camera looks along -Z at yaw 0).
    let forward = -Vec3::new(yaw.sin(), 0.0, yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let dt = time.delta_secs();
    let t = time.elapsed_secs();

    for (entity, actions, mut input, mut movement, mut intent, wielded) in &mut players {
        intent.tick(dt);
        // The frame a menu closes counts as blocked too: the click on "Resume" mustn't swing.
        if blocked.0 || blocked.is_changed() {
            // A menu is open: stand still, drop held buttons (no sprint, no charging heavy).
            movement.dir = Vec3::ZERO;
            movement.sprint = false;
            input.dodge_held = 0.0;
            input.attack = AttackTracker::default();
            input.offhand = AttackTracker::default();
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

        // Two-hand button: held + a hand's button toggles two-handing that hand's weapon (the
        // press doesn't attack); tapped alone it locks on (on release).
        if actions.just_pressed(&Action::TwoHand) {
            input.two_hand_combo = false;
        }
        // Gamepad Y does the same with R1 / L1; tapped alone it interacts (on release).
        if actions.just_pressed(&Action::PadY) {
            input.pad_combo = false;
        }
        let modifier = actions.pressed(&Action::TwoHand) || actions.pressed(&Action::PadY);
        if modifier {
            for (button, hand) in [(Action::Attack, Hand::Right), (Action::Block, Hand::Left)] {
                if actions.just_pressed(&button) {
                    two_hand.write(TwoHandRequest { character: entity, hand });
                    input.two_hand_combo = true;
                    input.pad_combo = true;
                }
            }
            input.attack = AttackTracker::default(); // swallowed: no attack on release
            input.offhand = AttackTracker::default();
        }
        if actions.just_released(&Action::TwoHand) && !input.two_hand_combo {
            input.lock_on_tap = true;
        }
        if actions.just_pressed(&Action::Ability) {
            intent.buffer(ActionRequest::Ability);
        }

        // Right hand: the attack button. Ignored while the cursor is free, so the click that
        // re-grabs it doesn't swing the sword. The autopilot may script the button instead.
        let mut attack = (
            actions.pressed(&Action::Attack) && !modifier,
            actions.just_pressed(&Action::Attack) && !modifier,
            actions.just_released(&Action::Attack),
        );
        let mut grabbed = cursor_grabbed(&cursor);
        if let Some(script) = autopilot.as_ref().and_then(|a| a.attack_button) {
            attack = script(t, dt);
            grabbed = true;
        }
        if let Some(request) = input.attack.update(AttackHand::Right, attack.0, attack.1, attack.2, grabbed, dt) {
            intent.buffer(request);
        }
        let mut held = attack.0 && input.attack.is_holding();
        // Left hand: the block button guards (shield, or a two-handed weapon) or attacks (a left
        // weapon; a matching pair attacks with both). Only while the game has the cursor.
        let offhand = Offhand::of(wielded);
        let left = (
            actions.pressed(&Action::Block) && !modifier,
            actions.just_pressed(&Action::Block) && !modifier,
            actions.just_released(&Action::Block),
        );
        if let Offhand::Attack(hand) = offhand {
            if let Some(request) = input.offhand.update(hand, left.0, left.1, left.2, grabbed, dt) {
                intent.buffer(request);
            }
            held |= left.0 && input.offhand.is_holding();
        }
        intent.attack_held = held;
        intent.guard_held = match autopilot.as_ref().and_then(|a| a.guard_button) {
            Some(script) => script(t),
            None => offhand == Offhand::Guard && left.0 && cursor_grabbed(&cursor),
        };

        if actions.just_pressed(&Action::DrinkAle) {
            intent.buffer(ActionRequest::DrinkAle);
        }
        if actions.just_pressed(&Action::Interact) || (actions.just_released(&Action::PadY) && !input.pad_combo) {
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

/// The cursor follows the game's state: free and visible in menus, captured during play — but only
/// while the window has focus (alt-tabbing away leaves it alone; coming back captures it again).
/// Checked every frame, not only when a menu opens or closes: the desktop can drop or restore a
/// pointer lock behind our back (GNOME Wayland, Shift+Tab report, ROADMAP N30), and a stale lock
/// left the inventory without a visible cursor.
pub fn toggle_cursor(
    blocked: Res<GameplayBlocked>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mut was_focused: Local<bool>,
) {
    let focused = window.focused;
    let regained = focused && !*was_focused;
    *was_focused = focused;
    if !focused {
        return;
    }
    let want_grab = !blocked.0;
    let grabbed = cursor.grab_mode != CursorGrabMode::None;
    if regained || grabbed != want_grab || cursor.visible == want_grab {
        set_grab(&mut cursor, want_grab);
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
        let h = AttackHand::Right;
        out.extend(tracker.update(h, true, true, false, grabbed, dt));
        let mut t = 0.0;
        while t < hold {
            out.extend(tracker.update(h, true, false, false, true, dt));
            t += dt;
        }
        out.extend(tracker.update(h, false, false, true, true, dt));
        out
    }

    #[test]
    fn attack_tap_is_light_hold_is_heavy() {
        let mut t = AttackTracker::default();
        assert_eq!(press(&mut t, 0.1, true), vec![ActionRequest::LightAttack(AttackHand::Right)]);
        assert_eq!(press(&mut t, 0.6, true), vec![ActionRequest::HeavyAttack(AttackHand::Right)], "heavy fires once, no light on release");
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
