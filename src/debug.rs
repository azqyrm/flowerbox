//! Dev helpers for manual and automated checks (see ROADMAP notes N5, N7, N9, N12, N13).
//!
//! - **F3**: toggle the debug overlay (action state, timers, buffer, stats), the cyan i-frame
//!   ring at the player's feet and the red blade hitbox during active frames.
//! - Debug builds only: **H** = take 15 damage; **1–4** = request Roll (backstep when standing still),
//!   Light (jump attack when airborne, running attack when sprinting), Heavy, Drink;
//!   **F6** = swap the weapon (Longsword ↔ Rusted Shortsword) until the inventory exists.
//! - `FLOWERBOX_SCREENSHOT=/path/shot.png` → saves a screenshot after a warm-up, then exits.
//!   `FLOWERBOX_SHOT_FRAME=n` changes the capture frame (default 120), or `FLOWERBOX_SHOT_TIME=secs`
//!   captures at a game time instead. `FLOWERBOX_OVERLAY=1` shows the overlay at start.
//! - `FLOWERBOX_AUTOPILOT=1` → walks along -Z, jumps every 2 s and runs a scripted action/damage
//!   timeline (see `autopilot_script`); position and action changes are logged.
//! - `FLOWERBOX_AUTOPILOT=combat` → fights the training dummy through the real attack-button
//!   logic (see `COMBAT_PRESSES` / `combat_movement`).

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::time::common_conditions::on_timer;

use crate::character::action::{ActionKind, ActionPhase, ActionRequest, CharacterAction};
use crate::character::controller::CharacterController;
use crate::character::stats::{DamageMessage, Health, Invulnerable, Stamina};
use crate::character::{ActionIntent, MoveIntent};
use crate::combat::{Dummy, blade_world};
use crate::weapon::{EquipWeapon, EquippedWeapon, light_sword};
use crate::humanoid::{HumanoidRig, HumanoidSpec};
use crate::input::{Autopilot, PlayerInput};
use crate::player::Player;
use crate::tuning::*;

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_overlay).add_systems(
            Update,
            (toggle_overlay, update_overlay, log_action_changes, spawn_iframe_ring, show_iframe_ring, draw_blade_hitbox),
        );
        if cfg!(debug_assertions) {
            app.add_systems(Update, debug_keys);
        }
        if let Ok(path) = std::env::var("FLOWERBOX_SCREENSHOT") {
            let at = std::env::var("FLOWERBOX_SHOT_FRAME").ok().and_then(|s| s.parse().ok()).unwrap_or(120);
            let at_time = std::env::var("FLOWERBOX_SHOT_TIME").ok().and_then(|s| s.parse().ok());
            app.insert_resource(AutoScreenshot { path, frame: 0, at, at_time, taken_frame: None })
                .add_systems(Update, auto_screenshot);
        }
        match std::env::var("FLOWERBOX_AUTOPILOT").as_deref() {
            Ok("combat") => {
                app.insert_resource(Autopilot { movement: Some(combat_movement), attack_button: Some(combat_attack_button) })
                    .add_systems(Update, (combat_script, log_dummy));
            }
            Ok("wall") => {
                app.insert_resource(Autopilot { movement: Some(wall_movement), attack_button: None })
                    .add_systems(Update, wall_script)
                    .add_systems(FixedLast, log_wall_tick);
            }
            Ok(_) => {
                app.insert_resource(Autopilot { movement: Some(default_movement), attack_button: None })
                    .init_resource::<AirborneTicks>()
                    .add_systems(FixedLast, count_airborne)
                    .add_systems(
                        Update,
                        (log_player.run_if(on_timer(std::time::Duration::from_millis(500))), autopilot_script),
                    );
            }
            Err(_) => {}
        }
    }
}

// ── Overlay ──────────────────────────────────────────────────────────────

#[derive(Component)]
struct DebugOverlay;

fn overlay_on(overlay: &Query<&Visibility, With<DebugOverlay>>) -> bool {
    overlay.iter().any(|v| *v != Visibility::Hidden)
}

fn spawn_overlay(mut commands: Commands) {
    let visible = std::env::var("FLOWERBOX_OVERLAY").is_ok();
    commands.spawn((
        Name::new("Debug Overlay"),
        DebugOverlay,
        Text::new(""),
        TextFont::from_font_size(15.0),
        TextColor(Color::srgb(0.85, 0.95, 0.85)),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            right: Val::Px(10.0),
            padding: UiRect::all(Val::Px(8.0)),
            ..default()
        },
        if visible { Visibility::Visible } else { Visibility::Hidden },
    ));
}

fn toggle_overlay(keys: Res<ButtonInput<KeyCode>>, mut q: Query<&mut Visibility, With<DebugOverlay>>) {
    if keys.just_pressed(KeyCode::F3) {
        for mut v in &mut q {
            *v = if *v == Visibility::Hidden { Visibility::Visible } else { Visibility::Hidden };
        }
    }
}

fn update_overlay(
    dummies: Query<&Health, (With<Dummy>, Without<Player>)>,
    player: Query<
        (&CharacterAction, &ActionIntent, &PlayerInput, &Health, &Stamina, &CharacterController, Has<Invulnerable>),
        With<Player>,
    >,
    mut text: Query<(&mut Text, &Visibility), With<DebugOverlay>>,
) {
    let Ok((action, intent, input, hp, st, ctrl, invuln)) = player.single() else { return };
    for (mut text, vis) in &mut text {
        if *vis == Visibility::Hidden {
            continue;
        }
        let action_line = match &action.current {
            Some(a) => format!(
                "action: {:?}  {:?}\n  {:.2} / {:.2}s  cancel@{:.2}",
                a.kind,
                a.phase(),
                a.elapsed,
                a.def.total(),
                a.def.cancel
            ),
            None => {
                let loco = if !ctrl.grounded {
                    "Airborne"
                } else if ctrl.sprinting {
                    "Sprint"
                } else if Vec2::new(ctrl.velocity.x, ctrl.velocity.z).length() > 0.2 {
                    "Walk"
                } else {
                    "Idle"
                };
                format!("action: - ({loco})")
            }
        };
        let buffer = match intent.buffered {
            Some(r) => format!("{r:?} ({:.2}s)", intent.buffer_left),
            None => "-".into(),
        };
        text.0 = format!(
            "[F3] DEBUG\n{action_line}\nbuffer: {buffer}\nHP {:.0}/{:.0}   ST {:.0}/{:.0}{}\n{}grounded {}  speed {:.1} m/s\nLMB held {:.2}s{}\ndummy HP {}",
            hp.current,
            hp.max,
            st.current,
            st.max,
            if st.regen_delay > 0.0 { format!(" (delay {:.1})", st.regen_delay) } else { String::new() },
            if invuln { "** INVULNERABLE **\n" } else { "" },
            ctrl.grounded,
            Vec2::new(ctrl.velocity.x, ctrl.velocity.z).length(),
            input.attack.held,
            action.current.map_or(String::new(), |a| if a.charge > 0.0 { format!("  charge {:.0}%", a.charge * 100.0) } else { String::new() }),
            dummies.iter().next().map_or("-".into(), |h| format!("{:.0}/{:.0}", h.current, h.max)),
        );
    }
}

/// Glowing ring at the player's feet, visible during i-frames while the overlay is on.
#[derive(Component)]
struct IframeRing;

fn spawn_iframe_ring(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    players: Query<Entity, Added<Player>>,
) {
    for player in &players {
        let ring = commands
            .spawn((
                Name::new("I-frame Ring"),
                IframeRing,
                Mesh3d(meshes.add(Annulus::new(0.5, 0.62))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(0.3, 0.9, 1.0),
                    emissive: LinearRgba::rgb(1.5, 5.0, 6.0),
                    unlit: true,
                    ..default()
                })),
                // Lies flat just above the floor (the root sits at the capsule center).
                Transform::from_xyz(0.0, -PLAYER_HALF_HEIGHT + 0.03, 0.0)
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                Visibility::Hidden,
            ))
            .id();
        commands.entity(player).add_child(ring);
    }
}

fn show_iframe_ring(
    overlay: Query<&Visibility, With<DebugOverlay>>,
    player: Query<Has<Invulnerable>, With<Player>>,
    mut rings: Query<&mut Visibility, (With<IframeRing>, Without<DebugOverlay>)>,
) {
    let show = overlay_on(&overlay) && player.iter().any(|i| i);
    for mut v in &mut rings {
        *v = if show { Visibility::Inherited } else { Visibility::Hidden };
    }
}

/// While the overlay is on, draws blade hitboxes in red during active frames (any armed humanoid).
fn draw_blade_hitbox(
    mut gizmos: Gizmos,
    overlay: Query<&Visibility, With<DebugOverlay>>,
    attackers: Query<(&Transform, &CharacterAction, &HumanoidSpec, &HumanoidRig, &EquippedWeapon)>,
) {
    if !overlay_on(&overlay) {
        return;
    }
    for (tf, action, spec, rig, equipped) in &attackers {
        let Some(cur) = action.current else { continue };
        if cur.damage_multiplier().is_none() || cur.phase() != ActionPhase::Active {
            continue;
        }
        let Some((a, b)) = blade_world(tf, spec, rig, equipped.def, &cur) else { continue };
        gizmos.line(a, b, Color::srgb(1.0, 0.1, 0.1));
        gizmos.sphere(Isometry3d::from_translation(b), BLADE_HIT_RADIUS, Color::srgb(1.0, 0.3, 0.1));
    }
}

// ── Debug keys (debug builds only) ───────────────────────────────────────

fn debug_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut damage: MessageWriter<DamageMessage>,
    mut equip: MessageWriter<EquipWeapon>,
    mut player: Query<(Entity, &mut ActionIntent, Option<&EquippedWeapon>), With<Player>>,
) {
    let Ok((entity, mut intent, equipped)) = player.single_mut() else { return };
    if keys.just_pressed(KeyCode::F6) {
        let next = match equipped.map(|e| e.def.name) {
            Some(name) if name == light_sword::LONGSWORD.name => &light_sword::SHORTSWORD,
            _ => &light_sword::LONGSWORD,
        };
        equip.write(EquipWeapon { character: entity, def: next });
    }
    if keys.just_pressed(KeyCode::KeyH) {
        damage.write(DamageMessage { target: entity, amount: 15.0, from: None });
    }
    let requests = [
        (KeyCode::Digit1, ActionRequest::Roll),
        (KeyCode::Digit2, ActionRequest::LightAttack),
        (KeyCode::Digit3, ActionRequest::HeavyAttack),
        (KeyCode::Digit4, ActionRequest::DrinkAle),
    ];
    for (key, request) in requests {
        if keys.just_pressed(key) {
            intent.buffer(request);
        }
    }
}

// ── Screenshots ──────────────────────────────────────────────────────────

#[derive(Resource)]
struct AutoScreenshot {
    path: String,
    frame: u32,
    at: u32,
    at_time: Option<f32>,
    taken_frame: Option<u32>,
}

fn auto_screenshot(
    time: Res<Time>,
    mut commands: Commands,
    mut shot: ResMut<AutoScreenshot>,
    mut exit: MessageWriter<AppExit>,
) {
    shot.frame += 1;
    let due = match shot.at_time {
        Some(t) => time.elapsed_secs() >= t,
        None => shot.frame >= shot.at,
    };
    match shot.taken_frame {
        None if due => {
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(shot.path.clone()));
            shot.taken_frame = Some(shot.frame);
        }
        Some(f) if shot.frame == f + 60 => {
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}

// ── Default autopilot (FLOWERBOX_AUTOPILOT=1) ────────────────────────────

/// True on the frame where time `at` is crossed.
fn crossed(t: f32, dt: f32, at: f32) -> bool {
    t - dt < at && t >= at
}

/// Walk along world -Z (camera-independent, so FLOWERBOX_CAM can film any side), jump every 2 s,
/// stand still from 10.8 to 11.6 s for the backstep test.
fn default_movement(t: f32, dt: f32) -> (Vec3, bool, bool) {
    let dir = if (10.8..11.6).contains(&t) { Vec3::ZERO } else { Vec3::NEG_Z };
    (dir, false, (t % 2.0) < dt && t > 1.0)
}

/// Timeline that exercises the action system. Expected log (N9): roll → damage ignored →
/// hurt (HP 90) → light combo 0,1,2 → jump attack → death → respawn with full HP →
/// backstep (standing still at 11 s).
fn autopilot_script(
    time: Res<Time>,
    mut damage: MessageWriter<DamageMessage>,
    mut player: Query<(Entity, &mut ActionIntent), With<Player>>,
) {
    let Ok((entity, mut intent)) = player.single_mut() else { return };
    let (t, dt) = (time.elapsed_secs(), time.delta_secs());
    let at = |s: f32| crossed(t, dt, s);
    if at(1.0) {
        intent.buffer(ActionRequest::Roll);
    }
    if at(1.2) {
        info!("autopilot: 10 damage during roll");
        damage.write(DamageMessage { target: entity, amount: 10.0, from: None });
    }
    if at(2.5) {
        info!("autopilot: 10 damage while free");
        damage.write(DamageMessage { target: entity, amount: 10.0, from: None });
    }
    for s in [2.9, 3.25, 3.6] {
        if at(s) {
            intent.buffer(ActionRequest::LightAttack);
        }
    }
    // Autopilot jumps at t=6.0; a light attack shortly after becomes a jump attack.
    if at(6.15) {
        intent.buffer(ActionRequest::LightAttack);
    }
    if at(7.5) {
        info!("autopilot: lethal damage");
        damage.write(DamageMessage { target: entity, amount: 500.0, from: None });
    }
    if at(11.0) {
        info!("autopilot: roll while standing still (expect Backstep)");
        intent.buffer(ActionRequest::Roll);
    }
}

/// Fixed ticks spent airborne since the last log line (should be ~0 unless jumping).
#[derive(Resource, Default)]
struct AirborneTicks(u32);

fn count_airborne(mut ticks: ResMut<AirborneTicks>, q: Query<&CharacterController, With<Player>>) {
    if q.iter().any(|c| !c.grounded) {
        ticks.0 += 1;
    }
}

fn log_player(mut ticks: ResMut<AirborneTicks>, q: Query<(&Transform, &CharacterController), With<Player>>) {
    for (tf, c) in &q {
        info!(
            "player pos={:.2} vel={:.2} grounded={} airborne_ticks={} r={:.2}",
            tf.translation,
            c.velocity,
            c.grounded,
            ticks.0,
            Vec2::new(tf.translation.x, tf.translation.z).length()
        );
    }
    ticks.0 = 0;
}

fn log_action_changes(
    mut last: Local<Option<ActionKind>>,
    q: Query<(&CharacterAction, &Health, &Stamina, &Transform), With<Player>>,
) {
    let Ok((action, hp, st, tf)) = q.single() else { return };
    let now = action.current.map(|a| a.kind);
    if now != *last {
        info!(
            "action {:?} -> {:?}  (HP {:.0}, ST {:.0}, pos {:.2})",
            *last, now, hp.current, st.current, tf.translation
        );
        *last = now;
    }
}

// ── Combat autopilot (FLOWERBOX_AUTOPILOT=combat) ────────────────────────

/// Attack button presses as (start, end) seconds. Expected (dummy has 120 HP):
/// light ×3 (combo 0,1,2: 20 each) → partial heavy (~36) → jump attack (36, breaks the dummy)
/// → dummy resets at ~8.7 s → full heavy (40) → two light taps (20 each)
/// → running attack out of a sprint (26) → walking jump attack (36, breaks it again)
/// → swap to the Rusted Shortsword at 16 s → light tap after the reset (16 = its base damage).
const COMBAT_PRESSES: [(f32, f32); 11] = [
    (2.0, 2.08),
    (2.35, 2.43),
    (2.7, 2.78),
    (3.8, 4.4),
    (5.62, 5.68),
    (9.0, 10.4),
    (11.4, 11.48),
    (11.85, 11.93),
    (13.15, 13.2),
    (14.85, 14.9),
    (18.6, 18.65),
];

fn combat_attack_button(t: f32, dt: f32) -> (bool, bool, bool) {
    let mut out = (false, false, false);
    for (start, end) in COMBAT_PRESSES {
        let pressed = (start..end).contains(&t);
        let was = (start..end).contains(&(t - dt));
        out.0 |= pressed;
        out.1 |= pressed && !was;
        out.2 |= was && !pressed;
    }
    out
}

/// Stand still, except: a standing jump at 5.5 s, a sprint toward the dummy from 12.7 s
/// (running attack), and a walk + jump toward it from 14.6 s (moving jump attack).
fn combat_movement(t: f32, dt: f32) -> (Vec3, bool, bool) {
    let jump = crossed(t, dt, 5.5) || crossed(t, dt, 14.7);
    if (12.7..13.3).contains(&t) {
        (Vec3::NEG_Z, true, false)
    } else if (14.6..15.2).contains(&t) {
        (Vec3::NEG_Z, false, jump)
    } else {
        (Vec3::ZERO, false, jump)
    }
}

/// Teleports the player to its start marks (dummy at the origin, player on +Z facing -Z).
fn combat_script(
    time: Res<Time>,
    mut equip: MessageWriter<EquipWeapon>,
    mut player: Query<(Entity, &mut Transform, &mut CharacterController, &mut MoveIntent), With<Player>>,
) {
    let Ok((entity, mut tf, mut ctrl, _)) = player.single_mut() else { return };
    let (t, dt) = (time.elapsed_secs(), time.delta_secs());
    let (x, z) = DUMMY_POSITION;
    if crossed(time.elapsed_secs(), time.delta_secs(), 16.0) {
        equip.write(EquipWeapon { character: entity, def: &light_sword::SHORTSWORD });
    }
    for (at, distance) in [(0.2, 1.5), (12.5, 4.4), (14.4, 4.0), (16.0, 1.3)] {
        if crossed(t, dt, at) {
            tf.translation = Vec3::new(x, PLAYER_HALF_HEIGHT + 0.02, z + distance);
            ctrl.velocity = Vec3::ZERO;
            ctrl.facing = 0.0;
        }
    }
}

// ── Wall autopilot (FLOWERBOX_AUTOPILOT=wall) ──────────────────────────

/// From (-1.5, -4) heading +Z: sprint (1–2.6 s), jog into the ring wall (2.6–6 s, contact
/// ≈ 4.75 s, so ~1.2 s of pushing), then diagonally along it (6–7.5 s). Logs every fixed tick
/// so stutter shows up as noise in pos/vel/grounded (N16).
fn wall_movement(t: f32, _dt: f32) -> (Vec3, bool, bool) {
    if (1.0..2.6).contains(&t) {
        (Vec3::Z, true, false)
    } else if (2.6..6.0).contains(&t) {
        (Vec3::Z, false, false)
    } else if (6.0..7.5).contains(&t) {
        (Vec3::new(1.0, 0.0, 1.0).normalize(), false, false)
    } else {
        (Vec3::ZERO, false, false)
    }
}

fn wall_script(time: Res<Time>, mut player: Query<(&mut Transform, &mut CharacterController), With<Player>>) {
    let Ok((mut tf, mut ctrl)) = player.single_mut() else { return };
    if crossed(time.elapsed_secs(), time.delta_secs(), 0.2) {
        tf.translation = Vec3::new(-1.5, PLAYER_HALF_HEIGHT + 0.02, -4.0); // a lane clear of rubble
        ctrl.velocity = Vec3::ZERO;
        ctrl.facing = std::f32::consts::PI;
    }
}

fn log_wall_tick(time: Res<Time>, q: Query<(&Transform, &CharacterController), With<Player>>) {
    let t = time.elapsed_secs();
    if !(0.9..7.7).contains(&t) {
        return;
    }
    for (tf, c) in &q {
        info!("wall t={t:.3} pos={:.3} vel={:.3} grounded={}", tf.translation, c.velocity, c.grounded);
    }
}

fn log_dummy(dummies: Query<&Health, (With<Dummy>, Changed<Health>)>) {
    for h in &dummies {
        info!("dummy HP {:.0}/{:.0}", h.current, h.max);
    }
}
