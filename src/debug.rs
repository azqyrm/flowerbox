//! Dev helpers for manual and automated checks (see ROADMAP notes N5, N7, N9, N12, N13).
//!
//! - **F3**: toggle the debug overlay (action state, timers, buffer, stats), the cyan i-frame
//!   ring at the player's feet and the red blade hitbox during active frames.
//! - Debug builds only: **H** / **J** = take 15 damage from the front / from behind; **1–4** = request Roll (backstep when standing still),
//!   Light (jump attack when airborne, running attack when sprinting), Heavy, Drink;
//!   **K** = die (tests YOU DIED and the Hearth respawn);
//!   **F6** = equip the next weapon in the bag; **F7** = sample notifications.
//! - `FLOWERBOX_SCREENSHOT=/path/shot.png` → saves a screenshot after a warm-up, then exits.
//!   `FLOWERBOX_SHOT_FRAME=n` changes the capture frame (default 120), or `FLOWERBOX_SHOT_TIME=secs`
//!   captures at a game time instead. `FLOWERBOX_OVERLAY=1` shows the overlay at start.
//! - `FLOWERBOX_AUTOPILOT=1` → walks along -Z, jumps every 2 s and runs a scripted action/damage
//!   timeline (see `autopilot_script`); position and action changes are logged.
//! - `FLOWERBOX_AUTOPILOT=combat` → fights the training dummy through the real attack-button
//!   logic (see `COMBAT_PRESSES` / `combat_movement`).
//! - `FLOWERBOX_AUTOPILOT=inventory` → inventory screen, equipping, a pickup, armour absorption
//!   (see `inventory_script`).
//! - `FLOWERBOX_AUTOPILOT=guard` → shield block, parry, hits from behind, guard break (see
//!   `guard_script`).
//! - `FLOWERBOX_AUTOPILOT=enemy` → a Hollow fight: hits taken, lock-on, a timed parry, riposte,
//!   kill, world reset (see `enemy_script`). The only autopilot with enemies in the arena.
//! - `FLOWERBOX_AUTOPILOT=net` → for two instances (`--host` / `--join <code>`): walk, attack, walk
//!   back (see `net_movement`), with the Hollow in the arena.
//! - `FLOWERBOX_AUTOPILOT=pvp` → for two instances: both eat a Poppy, the guest attacks the host
//!   (damage), eats a Camomile, attacks again (no damage); the host dies (PvP off, Poppy back).
//! - `FLOWERBOX_AUTOPILOT=netrest` → for two instances: the guest hits the dummy (both see it), both
//!   rest at the sunflower (shared rest completes), the guest walks away (stands up).
//! - `FLOWERBOX_AUTOPILOT=hearth` → Healing Ale, Hearth rest and death/respawn (see `hearth_script`).
//! - `FLOWERBOX_AUTOPILOT=wall` → sprint/jog into the ring wall and slide along it (N16).
//! - `FLOWERBOX_AUTOPILOT=level` → walks level 1 without enemies: opens both doors, takes the
//!   stairs, revives sunflower 1, ends in the vault (see `LEVEL_ROUTE`); logs arrivals and stalls.
//! - `FLOWERBOX_AUTOPILOT=warg` (with `FLOWERBOX_POS` near the vault's warg, e.g. `10,-59.7,-1.57`)
//!   → stands still: the warg bites and pounces; logs its actions, grabs and the bite's damage.
//!   `wargroll` → rolls toward the warg as each leap starts: the i-frames carry you through it.
//!   `FLOWERBOX_WARG_TANK=1` gives the player 1000 HP (to see several pounces).
//! - `FLOWERBOX_AUTOPILOT=wargkill` (with `FLOWERBOX_POS` in the vault) → kills the vault warg,
//!   takes the dropped Forge Key, resets the world, kills it again: the unique key doesn't drop twice.
//! - `FLOWERBOX_AUTOPILOT=mp` → multiplayer soak test: enemies on, nobody moves; every 2 s logs the
//!   puppets seen (players / enemies) and how many puppets (re)appeared so far.
//!   `FLOWERBOX_MP_DIE_AT=secs` kills this player then. `FLOWERBOX_MP_FIGHT=1`: run at the nearest
//!   living enemy (within 25 m) and keep attacking it.
//! - `FLOWERBOX_AUTOPILOT=still` → no enemies, no movement, every door open (level screenshots).
//!   `FLOWERBOX_WALK="x,z"` walks that way instead (collision checks), logging the position.
//! - `FLOWERBOX_KEYS="1.0:ShiftLeft+ 1.1:Tab+ 1.2:Tab- 1.4:ShiftLeft-"` → injects real key / mouse
//!   events (`Name+` press, `Name-` release; `MouseLeft`, `MouseRight`, `MouseMiddle` for buttons) at
//!   those game times, and logs menu / input-block / cursor changes. For input bugs.
//! - `FLOWERBOX_POS="x,z[,yaw]"` → start there (on the floor below; for screenshots in the level).

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::time::common_conditions::on_timer;

use crate::character::action::{ActionKind, ActionPhase, ActionRequest, CharacterAction};
use crate::character::controller::CharacterController;
use crate::character::stats::{DamageMessage, Health, Invulnerable, Stamina};
use crate::character::ActionIntent;
use crate::character::guard::Guard;
use crate::combat::{Dummy, blade_world};
use crate::healing::HealingAle;
use crate::items::ItemId;
use crate::input::GameplayBlocked;
use crate::ui::icons::NoticeIcon;
use crate::ui::notifications::Notify;
use crate::enemy::{Enemy, HollowAi};
use crate::character::MoveIntent;
use crate::hearth::WorldReset;
use crate::lockon::LockOn;
use crate::ui::inventory_ui::{Filter, InventoryMenu};
use crate::items::inventory::{EquipRequest, EquipSlot, Equipment, Inventory};
use crate::weapon::EquippedWeapon;
use crate::humanoid::{HumanoidRig, HumanoidSpec};
use crate::input::{Autopilot, PlayerInput};
use crate::player::Player;
use crate::tuning::*;

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_overlay).add_systems(
            Update,
            (toggle_overlay, update_overlay, log_action_changes, spawn_iframe_ring, show_iframe_ring, draw_blade_hitbox, draw_guard_arc),
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
        if std::env::var("FLOWERBOX_KEYS").is_ok() {
            app.add_systems(PreUpdate, inject_keys.before(bevy::input::InputSystems)).add_systems(PostUpdate, log_input_state);
        }
        if std::env::var("FLOWERBOX_POS").is_ok() {
            app.add_systems(PostStartup, place_player);
        }
        match std::env::var("FLOWERBOX_AUTOPILOT").as_deref() {
            Ok(mode @ ("warg" | "wargroll")) => {
                app.insert_resource(Autopilot { movement: Some(|_, _| *LEVEL_DIR.lock().unwrap()), attack_button: None, guard_button: None })
                    .insert_resource(WargRun { roll: mode == "wargroll", ..default() })
                    .add_systems(Update, warg_script);
            }
            Ok("mp") => {
                if std::env::var("FLOWERBOX_MP_FIGHT").is_ok() {
                    app.insert_resource(Autopilot { movement: Some(|_, _| *LEVEL_DIR.lock().unwrap()), attack_button: None, guard_button: None })
                        .add_systems(Update, mp_fight);
                }
                app.add_systems(Update, log_puppets);
            }
            Ok("wargkill") => {
                app.add_systems(Update, wargkill_script);
            }
            Ok("still") => {
                // `FLOWERBOX_WALK="x,z"`: walk that way (world direction), logging the position.
                let walk: Vec<f32> = std::env::var("FLOWERBOX_WALK").unwrap_or_default().split(',').filter_map(|x| x.trim().parse().ok()).collect();
                if let [x, z] = walk[..] {
                    *LEVEL_DIR.lock().unwrap() = (Vec3::new(x, 0.0, z).normalize_or_zero(), false, false);
                    // Starts after 2.5 s, so swinging doors (`FLOWERBOX_SWING=1`) have finished.
                    let movement: fn(f32, f32) -> (Vec3, bool, bool) = |t, _| if t > 2.5 { *LEVEL_DIR.lock().unwrap() } else { (Vec3::ZERO, false, false) };
                    app.insert_resource(Autopilot { movement: Some(movement), attack_button: None, guard_button: None })
                        .add_systems(Update, log_player_pos.run_if(on_timer(std::time::Duration::from_millis(500))));
                }
                app.add_systems(Update, open_all_doors);
            }
            Ok("level") => {
                app.insert_resource(Autopilot { movement: Some(|_, _| *LEVEL_DIR.lock().unwrap()), attack_button: None, guard_button: None })
                    .init_resource::<LevelRun>()
                    .add_systems(Update, level_script);
            }
            Ok("combat") => {
                app.insert_resource(Autopilot { movement: Some(combat_movement), attack_button: Some(combat_attack_button), guard_button: None })
                    .add_systems(Update, (combat_script, log_dummy));
            }
            Ok("hearth") => {
                app.insert_resource(Autopilot { movement: Some(hearth_movement), attack_button: None, guard_button: None })
                    .add_systems(Update, (hearth_script, log_ale));
            }
            Ok("inventory") => {
                app.insert_resource(Autopilot { movement: Some(|_, _| (Vec3::ZERO, false, false)), attack_button: None, guard_button: None })
                    .add_systems(Update, (inventory_script, log_ale))
                    .add_systems(PostUpdate, check_menu_block);
            }
            Ok("guard") => {
                app.insert_resource(Autopilot { movement: Some(|_, _| (Vec3::ZERO, false, false)), attack_button: None, guard_button: Some(guard_held) })
                    .add_systems(Update, (guard_script, log_ale));
            }
            Ok("enemy") => {
                app.insert_resource(Autopilot {
                    movement: Some(|_, _| {
                        let walk = AUTO_WALK.load(std::sync::atomic::Ordering::Relaxed);
                        (if walk { Vec3::NEG_Z } else { Vec3::ZERO }, false, false)
                    }),
                    attack_button: None,
                    guard_button: Some(|_| AUTO_GUARD.load(std::sync::atomic::Ordering::Relaxed)),
                })
                .init_resource::<EnemyScript>()
                .add_systems(Update, (enemy_script, log_ale, log_audio));
            }
            Ok("netrest") => {
                app.insert_resource(Autopilot {
                    movement: Some(|_, _| {
                        let walk = AUTO_WALK.load(std::sync::atomic::Ordering::Relaxed);
                        (if walk { Vec3::NEG_Z } else { Vec3::ZERO }, false, false)
                    }),
                    attack_button: None,
                    guard_button: None,
                })
                .add_systems(Update, (netrest_script, log_ale));
            }
            Ok("pvp") => {
                app.insert_resource(Autopilot { movement: Some(|_, _| (Vec3::ZERO, false, false)), attack_button: None, guard_button: None })
                    .add_systems(Update, (pvp_script, log_ale));
            }
            Ok("netfight") => {
                app.insert_resource(Autopilot { movement: Some(netfight_movement), attack_button: Some(netfight_attack_button), guard_button: None })
                    .add_systems(Update, netfight_script);
            }
            Ok("menu") => {
                app.add_systems(Update, menu_script).add_systems(PostUpdate, check_menu_block);
            }
            Ok("notify") => {
                app.add_systems(Update, notify_script);
            }
            Ok("net") => {
                app.insert_resource(Autopilot { movement: Some(net_movement), attack_button: Some(net_attack_button), guard_button: None });
            }
            Ok("wall") => {
                app.insert_resource(Autopilot { movement: Some(wall_movement), attack_button: None, guard_button: None })
                    .add_systems(Update, wall_script)
                    .add_systems(FixedLast, log_wall_tick);
            }
            Ok(_) => {
                app.insert_resource(Autopilot { movement: Some(default_movement), attack_button: None, guard_button: None })
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
        (&CharacterAction, &ActionIntent, &PlayerInput, &Health, &Stamina, &CharacterController, Has<Invulnerable>, Option<&HealingAle>, &Guard),
        With<Player>,
    >,
    mut text: Query<(&mut Text, &Visibility), With<DebugOverlay>>,
    real: Res<Time<Real>>,
    mut fps: Local<f32>,
) {
    let Ok((action, intent, input, hp, st, ctrl, invuln, ale, guard)) = player.single() else { return };
    let dt = real.delta_secs().max(1e-4);
    *fps = if *fps == 0.0 { 1.0 / dt } else { *fps * 0.95 + 0.05 / dt };
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
            "[F3] DEBUG   {:.0} FPS\n{action_line}\nbuffer: {buffer}\nHP {:.0}/{:.0}   ST {:.0}/{:.0}{}   Ale {}\n{}grounded {}  speed {:.1} m/s\nLMB held {:.2}s{}\nguard {}\ndummy HP {}",
            *fps,
            hp.current,
            hp.max,
            st.current,
            st.max,
            if st.regen_delay > 0.0 { format!(" (delay {:.1})", st.regen_delay) } else { String::new() },
            ale.map_or("-".into(), |a| format!("{}/{}", a.charges, a.max)),
            if invuln { "** INVULNERABLE **\n" } else { "" },
            ctrl.grounded,
            Vec2::new(ctrl.velocity.x, ctrl.velocity.z).length(),
            input.attack.held,
            action.current.map_or(String::new(), |a| if a.charge > 0.0 { format!("  charge {:.0}%", a.charge * 100.0) } else { String::new() }),
            match guard.raised_for {
                _ if guard.strength <= 0.0 => "- (no shield)".to_string(),
                None => format!("down ({:.0}% block)", guard.strength * 100.0),
                Some(t) if guard.in_parry_window() => format!("** PARRY WINDOW ** {t:.2}s"),
                Some(t) => format!("up {t:.2}s"),
            },
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

/// With the overlay on: the guarded arc on the floor while the guard is up (gold in the parry
/// window, blue otherwise).
fn draw_guard_arc(mut gizmos: Gizmos, overlay: Query<&Visibility, With<DebugOverlay>>, q: Query<(&Transform, &CharacterController, &Guard)>) {
    if !overlay_on(&overlay) {
        return;
    }
    for (tf, ctrl, guard) in &q {
        if !guard.is_up() {
            continue;
        }
        let color = if guard.in_parry_window() { Color::srgb(1.0, 0.8, 0.2) } else { Color::srgb(0.3, 0.6, 1.0) };
        let centre = tf.translation.with_y(0.05);
        let at = |a: f32| centre + Vec3::new(-(ctrl.facing + a).sin(), 0.0, -(ctrl.facing + a).cos()) * 1.6;
        let steps = 16;
        for i in 0..steps {
            let a0 = -GUARD_ANGLE + 2.0 * GUARD_ANGLE * i as f32 / steps as f32;
            let a1 = -GUARD_ANGLE + 2.0 * GUARD_ANGLE * (i + 1) as f32 / steps as f32;
            gizmos.line(at(a0), at(a1), color);
        }
        gizmos.line(centre, at(-GUARD_ANGLE), color);
        gizmos.line(centre, at(GUARD_ANGLE), color);
    }
}

// ── Debug keys (debug builds only) ───────────────────────────────────────

#[allow(clippy::type_complexity)]
fn debug_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut damage: MessageWriter<DamageMessage>,
    mut equip: MessageWriter<EquipRequest>,
    mut notify: MessageWriter<Notify>,
    blocked: Res<GameplayBlocked>,
    mut player: Query<(Entity, &mut ActionIntent, &Inventory, &Equipment, &Transform, &CharacterController), With<Player>>,
) {
    if blocked.0 {
        return; // a menu has the keyboard
    }
    let Ok((entity, mut intent, inventory, equipment, tf, ctrl)) = player.single_mut() else { return };
    if keys.just_pressed(KeyCode::F7) {
        sample_notifications(&mut notify);
    }
    if keys.just_pressed(KeyCode::F6) {
        // Next weapon in the bag after the one in hand.
        let current = equipment.get(EquipSlot::RightHand);
        let weapons: Vec<usize> = (0..inventory.items.len()).filter(|&i| inventory.items[i].def().weapon.is_some()).collect();
        let pos = weapons.iter().position(|&i| Some(i) == current);
        if let Some(&next) = weapons.get(pos.map_or(0, |p| (p + 1) % weapons.len())) {
            equip.write(EquipRequest { character: entity, index: next, equip: true });
        }
    }
    // H = a 15-damage hit from the front, J = from behind (to test the guard).
    let forward = Vec3::new(-ctrl.facing.sin(), 0.0, -ctrl.facing.cos());
    for (key, side) in [(KeyCode::KeyH, 1.0), (KeyCode::KeyJ, -1.0)] {
        if keys.just_pressed(key) {
            let from = tf.translation + forward * side * 1.5;
            damage.write(DamageMessage { target: entity, amount: 15.0, from: Some(from), attacker: None });
        }
    }
    if keys.just_pressed(KeyCode::KeyK) {
        damage.write(DamageMessage { target: entity, amount: f32::MAX, from: None, attacker: None });
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
/// stand still from 13.3 to 14.1 s for the backstep test (after the Hearth respawn at ~12 s).
fn default_movement(t: f32, dt: f32) -> (Vec3, bool, bool) {
    let dir = if (13.3..14.1).contains(&t) { Vec3::ZERO } else { Vec3::NEG_Z };
    (dir, false, (t % 2.0) < dt && t > 1.0)
}

/// Timeline that exercises the action system. Expected log (N9): roll → damage ignored →
/// hurt (HP 90) → light combo 0,1,2 → jump attack → death → YOU DIED → respawn at the Hearth
/// with full HP (~12 s) → backstep (standing still at 13.5 s).
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
        damage.write(DamageMessage { target: entity, amount: 10.0, from: None, attacker: None });
    }
    if at(2.5) {
        info!("autopilot: 10 damage while free");
        damage.write(DamageMessage { target: entity, amount: 10.0, from: None, attacker: None });
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
        damage.write(DamageMessage { target: entity, amount: 500.0, from: None, attacker: None });
    }
    if at(13.5) {
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
    mut equip: MessageWriter<EquipRequest>,
    mut player: Query<(Entity, &mut Transform, &mut CharacterController, &mut Inventory), With<Player>>,
) {
    let Ok((entity, mut tf, mut ctrl, mut inventory)) = player.single_mut() else { return };
    let (t, dt) = (time.elapsed_secs(), time.delta_secs());
    let (x, z) = DUMMY_POSITION;
    if crossed(time.elapsed_secs(), time.delta_secs(), 16.0) {
        let index = inventory.add(ItemId::RustedShortsword);
        equip.write(EquipRequest { character: entity, index, equip: true });
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

// ── Hearth autopilot (FLOWERBOX_AUTOPILOT=hearth) ───────────────────────

/// Stand still; walk toward the Hearth (+Z from the spawn) from 8.8 to 9.4 s.
fn hearth_movement(t: f32, _dt: f32) -> (Vec3, bool, bool) {
    (if (8.8..9.4).contains(&t) { Vec3::Z } else { Vec3::ZERO }, false, false)
}

/// Healing Ale, Hearth and death flow. Expected log:
/// 1.0 dmg 50 (HP 50) → 2.0 drink (HP 90, 2 left) → 3.5 drink interrupted by a hit at 3.7
/// (HP 80, still 2 left) → 4.5 drink (HP 100, 1 left) → 6.0 drink (0 left) → 7.5 drink:
/// "the tankard is empty" → 8.5 dmg 40 (HP 60) → walk to the Hearth → 9.8 E: Rest →
/// "hearth kindled", HP 100, 3 Ale → 11.0 E: stand up → 12.0 lethal → YOU DIED → respawn at the Hearth
/// (~16.5 s) with full HP and Ale.
fn hearth_script(
    time: Res<Time>,
    mut damage: MessageWriter<DamageMessage>,
    mut player: Query<(Entity, &mut ActionIntent, &mut PlayerInput), With<Player>>,
) {
    let Ok((entity, mut intent, mut input)) = player.single_mut() else { return };
    let (t, dt) = (time.elapsed_secs(), time.delta_secs());
    let at = |s: f32| crossed(t, dt, s);
    for (when, amount) in [(1.0, 50.0), (3.7, 10.0), (8.5, 40.0), (12.0, 500.0)] {
        if at(when) {
            info!("autopilot: {amount} damage");
            damage.write(DamageMessage { target: entity, amount, from: None, attacker: None });
        }
    }
    for when in [2.0, 3.5, 4.5, 6.0, 7.5] {
        if at(when) {
            info!("autopilot: drink");
            intent.buffer(ActionRequest::DrinkAle);
        }
    }
    if at(9.8) || at(11.0) {
        info!("autopilot: press E");
        input.interact = true;
    }
}

fn log_ale(player: Query<(&HealingAle, &Health), (With<Player>, Or<(Changed<HealingAle>, Changed<Health>)>)>) {
    for (ale, hp) in &player {
        info!("ale {}/{}  HP {:.0}", ale.charges, ale.max, hp.current);
    }
}

fn log_dummy(dummies: Query<&Health, (With<Dummy>, Changed<Health>)>) {
    for h in &dummies {
        info!("dummy HP {:.0}/{:.0}", h.current, h.max);
    }
}

// ── Inventory autopilot (FLOWERBOX_AUTOPILOT=inventory) ─────────────────

/// Inventory and equipment flow. Expected log / screenshots:
/// 1.0 open the menu, Rusted Axe selected (preview: Attack 20 → 24) → 2.0 equip axe, shield, hood
/// → 2.6 shield selected (preview of unequipping it) → 3.2 close → 3.4 stand at the Rusted
/// Shortsword → 4.0 E: "picked up Rusted Shortsword" (toast) → 5.0 50 damage, 6% absorbed
/// (HP 53) → 6.0 open, Weapons filter → 6.5 equip the shortsword → 7.0 unequip it (fists) →
/// 7.5 close.
fn inventory_script(
    time: Res<Time>,
    mut menu: ResMut<InventoryMenu>,
    mut damage: MessageWriter<DamageMessage>,
    mut equip: MessageWriter<EquipRequest>,
    mut player: Query<(Entity, &mut Transform, &mut CharacterController, &mut PlayerInput, &Inventory), With<Player>>,
) {
    let Ok((entity, mut tf, mut ctrl, mut input, inventory)) = player.single_mut() else { return };
    let (t, dt) = (time.elapsed_secs(), time.delta_secs());
    let at = |s: f32| crossed(t, dt, s);
    let select = |menu: &mut InventoryMenu, filter: Filter, cursor: usize| {
        (menu.open, menu.filter, menu.cursor, menu.dirty) = (true, filter, cursor, true);
    };
    if at(1.0) {
        select(&mut menu, Filter::All, 3);
    }
    if at(2.0) {
        for index in [3, 2, 4] {
            equip.write(EquipRequest { character: entity, index, equip: true });
        }
    }
    if at(2.6) {
        select(&mut menu, Filter::All, 2);
    }
    if at(3.2) || at(7.5) {
        menu.open = false;
    }
    if at(3.4) {
        tf.translation = Vec3::new(-7.0, PLAYER_HALF_HEIGHT + 0.02, -5.0);
        ctrl.velocity = Vec3::ZERO;
        ctrl.facing = 0.0;
    }
    if at(4.0) {
        info!("autopilot: press E");
        input.interact = true;
    }
    if at(5.0) {
        info!("autopilot: 50 damage");
        damage.write(DamageMessage { target: entity, amount: 50.0, from: None, attacker: None });
    }
    if at(6.0) {
        select(&mut menu, Filter::Weapons, 2);
    }
    let shortsword = inventory.items.iter().position(|i| *i == ItemId::RustedShortsword);
    if let Some(index) = shortsword {
        if at(6.5) {
            equip.write(EquipRequest { character: entity, index, equip: true });
        }
        if at(7.0) {
            equip.write(EquipRequest { character: entity, index, equip: false });
        }
    }
}

// ── Guard autopilot (FLOWERBOX_AUTOPILOT=guard) ─────────────────────────

/// Block held 1.0–3.0 s and 4.0–5.0 s.
fn guard_held(t: f32) -> bool {
    (1.0..3.0).contains(&t) || (4.0..5.0).contains(&t)
}

/// Shield, block and parry; the shield is equipped at 0.5 s. Expected log:
/// 0.3 no shield yet: 10 → full, Hurt (HP 90) → 1.05 raising: "blocked" (4 through, HP 86) →
/// 1.26 parry window: "parried", "parry!" → 1.8 "blocked" (HP 82) → 2.6 160 damage with ~90
/// stamina: "guard broken" (64 through, stagger, HP 18) → 4.5 guard up again (4.0): 10 → "blocked"
/// (HP 14) → 4.7 5 from behind → full, stagger (HP 9) → 5.5 guard down: 5 → full (HP 4).
fn guard_script(
    time: Res<Time>,
    mut damage: MessageWriter<DamageMessage>,
    mut equip: MessageWriter<EquipRequest>,
    player: Query<(Entity, &Transform, &CharacterController, &Stamina), With<Player>>,
) {
    let Ok((entity, tf, ctrl, stamina)) = player.single() else { return };
    let (t, dt) = (time.elapsed_secs(), time.delta_secs());
    if crossed(t, dt, 0.5) {
        equip.write(EquipRequest { character: entity, index: 2, equip: true });
    }
    let forward = Vec3::new(-ctrl.facing.sin(), 0.0, -ctrl.facing.cos());
    for (at, amount, behind) in [(0.3, 10.0, false), (1.05, 10.0, false), (1.26, 10.0, false), (1.8, 10.0, false), (2.6, 160.0, false), (4.5, 10.0, false), (4.7, 5.0, true), (5.5, 5.0, false)] {
        if crossed(t, dt, at) {
            let from = tf.translation + forward * if behind { -1.5 } else { 1.5 };
            info!("autopilot: {amount} damage from the {} (stamina {:.0})", if behind { "back" } else { "front" }, stamina.current);
            damage.write(DamageMessage { target: entity, amount, from: Some(from), attacker: None });
        }
    }
}

// ── Enemy autopilot (FLOWERBOX_AUTOPILOT=enemy) ─────────────────────────

/// The enemy script's block button (the autopilot's `guard_button` is a plain fn).
static AUTO_GUARD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The enemy script's "walk forward" (-Z) switch, for the walk-over-the-corpse check.
static AUTO_WALK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Resource, Default)]
struct EnemyScript {
    guard_until: f32,
    died_at: Option<f32>,
    reset_sent: bool,
}

/// A Hollow fight. 0.1 s: the player steps into the spawn area (the Hollow rises), 1.0 s: back at
/// the start, the Hollow is moved 4 m in front of the player (its new home) and the shield equipped → it attacks; the player just takes the first swings → 3.0 lock on → from
/// 5.0 the player raises the shield timed to the Hollow's swings (parry window) → "parried",
/// "attacker stunned" → light attack: "RIPOSTE" (50) → more light attacks until "a Hollow falls"
/// → lock-on lost when it dies → 5 s later a WorldReset: "enemies reset (spawn points armed)": the
/// corpse goes, and a new Hollow only rises when someone comes near again.
#[allow(clippy::type_complexity)]
fn enemy_script(
    time: Res<Time>,
    mut state: ResMut<EnemyScript>,
    mut equip: MessageWriter<EquipRequest>,
    mut reset: MessageWriter<WorldReset>,
    mut player: Query<(Entity, &mut Transform, &CharacterAction, &mut ActionIntent, &mut LockOn), With<Player>>,
    mut hollows: Query<(Entity, &mut Enemy, &mut Transform, &CharacterAction, &Health, &HollowAi, &ActionIntent, &MoveIntent), Without<Player>>,
) {
    use std::sync::atomic::Ordering::Relaxed;
    let Ok((pe, mut ptf, paction, mut intent, mut lock)) = player.single_mut() else { return };
    let (t, dt) = (time.elapsed_secs(), time.delta_secs());
    // The Hollow only rises when a player enters its spawn area: step in, then back to the start.
    if crossed(t, dt, 0.1) {
        ptf.translation = Vec3::new(0.0, ptf.translation.y, -2.0);
    }
    if crossed(t, dt, 1.0) {
        ptf.translation = Vec3::new(0.0, ptf.translation.y, 8.0);
    }
    // The arena's tutorial Hollow (the level has more, far away behind a closed door).
    let Some((he, mut enemy, mut htf, haction, hhp, ai, hintent, hmove)) =
        hollows.iter_mut().find(|h| h.1.home.with_y(0.0).length() < ARENA_RADIUS)
    else {
        return;
    };
    if (t * 2.0).floor() != ((t - dt) * 2.0).floor() {
        let dist = (htf.translation - ptf.translation).with_y(0.0).length();
        info!(
            "hollow {:?} dist {dist:.2} action {:?} buffered {:?} move {:.2} hp {:.0}",
            ai.state,
            haction.current.map(|a| a.kind),
            hintent.buffered,
            hmove.dir.length(),
            hhp.current
        );
    }
    if crossed(t, dt, 1.0) {
        htf.translation = Vec3::new(0.0, htf.translation.y, 4.0);
        enemy.home = htf.translation.with_y(0.0);
        equip.write(EquipRequest { character: pe, index: 2, equip: true });
    }
    if crossed(t, dt, 3.0) {
        lock.target = Some(he);
        info!("autopilot: lock on");
    }
    // Parry: raise the shield so the Hollow's blade arrives in the parry window.
    if t > 5.0 && !hhp.is_dead() {
        if let Some(a) = haction.current
            && matches!(a.kind, ActionKind::LightAttack(_))
            && t > state.guard_until + 0.3
        {
            let hit_at = a.def.startup + 0.04;
            let lead = GUARD_RAISE_TIME * (PARRY_WINDOW_START + 1.0) * 0.5;
            if a.elapsed >= hit_at - lead && a.elapsed < hit_at - lead + 0.05 {
                state.guard_until = t + 0.6;
                info!("autopilot: raise shield (hollow swing at {:.2}s)", a.elapsed);
            }
        }
        // Punish: riposte a stunned Hollow, keep hitting while it reels.
        let dist = (htf.translation - ptf.translation).with_y(0.0).length();
        let opening = matches!(haction.current.map(|a| a.kind), Some(ActionKind::Stunned | ActionKind::Hurt));
        if opening && dist < 2.4 && paction.current.is_none() && intent.buffered.is_none() {
            info!("autopilot: attack");
            intent.buffer(ActionRequest::LightAttack);
        }
    }
    // Drop the shield once the Hollow reels (to riposte).
    if matches!(haction.current.map(|a| a.kind), Some(ActionKind::Stunned | ActionKind::Hurt)) {
        state.guard_until = state.guard_until.min(t);
    }
    AUTO_GUARD.store(t < state.guard_until, Relaxed);
    if hhp.is_dead() && state.died_at.is_none() {
        state.died_at = Some(t);
    }
    // Corpses don't block: stand 1.5 m behind the body and walk through it.
    if let Some(d) = state.died_at {
        if crossed(t, dt, d + 1.0) {
            ptf.translation = Vec3::new(htf.translation.x, ptf.translation.y, htf.translation.z + 1.5);
            lock.target = None;
            info!("autopilot: walk over the corpse (from z {:.2}, corpse at z {:.2})", ptf.translation.z, htf.translation.z);
            AUTO_WALK.store(true, Relaxed);
        }
        if crossed(t, dt, d + 2.2) {
            AUTO_WALK.store(false, Relaxed);
            let passed = ptf.translation.z < htf.translation.z - 0.5;
            info!("autopilot: walked to z {:.2}: {}", ptf.translation.z, if passed { "passed the corpse" } else { "BLOCKED by the corpse" });
        }
    }
    if let Some(d) = state.died_at
        && t > d + 5.0
        && !state.reset_sent
    {
        state.reset_sent = true;
        info!("autopilot: world reset");
        reset.write(WorldReset);
    }
}

// ── Net autopilot (FLOWERBOX_AUTOPILOT=net, two instances) ─────────────

/// 4–6 s walk toward the arena centre, 6–8 s light attacks, 8–10 s walk back, 12–13 s sideways.
fn net_movement(t: f32, _dt: f32) -> (Vec3, bool, bool) {
    let dir = if (4.0..6.0).contains(&t) {
        Vec3::NEG_Z
    } else if (8.0..10.0).contains(&t) {
        Vec3::Z
    } else if (12.0..13.0).contains(&t) {
        Vec3::X
    } else {
        Vec3::ZERO
    };
    (dir, false, false)
}

fn net_attack_button(t: f32, dt: f32) -> (bool, bool, bool) {
    let taps = [6.2, 6.8, 7.4];
    let pressed = taps.iter().any(|&s| (s..s + 0.08).contains(&t));
    let just_pressed = taps.iter().any(|&s| crossed(t, dt, s));
    let just_released = taps.iter().any(|&s| crossed(t, dt, s + 0.08));
    (pressed, just_pressed, just_released)
}

/// Guest variant: walk toward the Hollow's corner (4–7.5 s; it rises when the guest gets close)
/// and swing from 8 to 12 s.
fn netfight_movement(t: f32, _dt: f32) -> (Vec3, bool, bool) {
    let dir = if (4.0..7.5).contains(&t) { Vec3::new(-0.35, 0.0, -1.0).normalize() } else { Vec3::ZERO };
    (dir, false, false)
}

fn netfight_attack_button(t: f32, _dt: f32) -> (bool, bool, bool) {
    if !(8.0..12.0).contains(&t) {
        return (false, false, false);
    }
    let phase = (t - 8.0) % 0.9;
    let down = phase < 0.08;
    (down, down && phase < 0.02, !down && phase < 0.1)
}

// ── Notifications (F7 / FLOWERBOX_AUTOPILOT=notify) ─────────────────────

fn sample_notifications(notify: &mut MessageWriter<Notify>) {
    use crate::items::ItemIcon;
    for n in [
        Notify::new(NoticeIcon::Joined, "Guest 1 joined"),
        Notify::new(NoticeIcon::Item(ItemIcon::RoundShield), "Guest 1 picked up Studded Buckler"),
        Notify::new(NoticeIcon::Sunflower, "Guest 1 vigorated the Sun"),
        Notify::new(NoticeIcon::Item(ItemIcon::Poppy), "Guest 1 ate a Poppy: PvP on"),
    ] {
        notify.write(n);
    }
}

/// Four notifications in quick succession (0.5–1.1 s), for the stack screenshot (~1.5 s).
fn notify_script(time: Res<Time>, mut notify: MessageWriter<Notify>) {
    use crate::items::ItemIcon;
    let (t, dt) = (time.elapsed_secs(), time.delta_secs());
    let list = [
        (0.5, Notify::new(NoticeIcon::Joined, "Guest 1 joined")),
        (0.7, Notify::new(NoticeIcon::Item(ItemIcon::Buckler), "Guest 1 picked up Studded Buckler")),
        (0.9, Notify::new(NoticeIcon::Sunflower, "Guest 1 vigorated the Sun")),
        (1.1, Notify::new(NoticeIcon::Item(ItemIcon::Poppy), "Guest 1 ate a Poppy: PvP on")),
    ];
    for (at, n) in list {
        if crossed(t, dt, at) {
            notify.write(n);
        }
    }
}

/// Guest variant, after the fight: 12 s stand at the Rusted Shortsword and take it (12.5 s), 14 s
/// stand at the sunflower and rest (14.5 s): the host should log/notify both.
fn netfight_script(time: Res<Time>, mut player: Query<(&mut Transform, &mut CharacterController, &mut PlayerInput), With<Player>>) {
    let Ok((mut tf, mut ctrl, mut input)) = player.single_mut() else { return };
    let (t, dt) = (time.elapsed_secs(), time.delta_secs());
    for (at, x, z) in [(12.0, -7.0, -5.4), (14.0, 0.0, 10.3)] {
        if crossed(t, dt, at) {
            tf.translation = Vec3::new(x, tf.translation.y, z);
            ctrl.velocity = Vec3::ZERO;
            ctrl.facing = 0.0;
        }
    }
    if [12.4, 12.8, 13.2, 13.6, 14.5].iter().any(|&at| crossed(t, dt, at)) {
        info!("autopilot: press E");
        input.interact = true;
    }
}

// ── PvP autopilot (FLOWERBOX_AUTOPILOT=pvp, two instances) ─────────────

/// Both: 2.5 s eat a Poppy. Guest: 4 s stand 1.6 m in front of the host, facing it; light attacks
/// at 5.0 / 5.9 / 6.8 s (host takes damage); 8 s eat a Camomile; attacks at 10 / 10.9 s (no
/// damage). Host: 13 s killed (PvP off), respawns with a Poppy.
fn pvp_script(
    time: Res<Time>,
    session: Res<crate::net::NetSession>,
    mut use_item: MessageWriter<crate::pvp::UseItemRequest>,
    mut damage: MessageWriter<DamageMessage>,
    mut player: Query<(Entity, &mut Transform, &mut CharacterController, &mut ActionIntent), With<Player>>,
) {
    let Ok((entity, mut tf, mut ctrl, mut intent)) = player.single_mut() else { return };
    let (t, dt) = (time.elapsed_secs(), time.delta_secs());
    let at = |s: f32| crossed(t, dt, s);
    if at(2.5) {
        use_item.write(crate::pvp::UseItemRequest { item: ItemId::Poppy });
    }
    if session.is_host() {
        if at(13.0) {
            info!("autopilot: lethal damage");
            damage.write(DamageMessage { target: entity, amount: 1000.0, from: None, attacker: None });
        }
        return;
    }
    if at(4.0) {
        tf.translation = Vec3::new(0.0, tf.translation.y, 6.4);
        ctrl.velocity = Vec3::ZERO;
        ctrl.facing = std::f32::consts::PI;
    }
    if at(8.0) {
        use_item.write(crate::pvp::UseItemRequest { item: ItemId::Camomile });
    }
    if [5.0, 5.9, 6.8, 10.0, 10.9].iter().any(|&s| at(s)) {
        info!("autopilot: attack");
        intent.buffer(ActionRequest::LightAttack);
    }
}

// ── Shared rest + dummy autopilot (FLOWERBOX_AUTOPILOT=netrest, two instances) ──

/// Host: 3.5 s stand at the sunflower, 4.0 s E (rests: "Host waiting … 1/2"). Guest: 2.0 s stand in
/// front of the dummy; light attacks at 2.6 / 3.4 s and a heavy at 4.2 s (the host sees the
/// wobble and the numbers); 6.0 s at the sunflower, 6.5 s E → the rest completes for both
/// (enemies respawn, everyone restored); 9.0–9.6 s walks off (stands up).
fn netrest_script(
    time: Res<Time>,
    session: Res<crate::net::NetSession>,
    mut player: Query<(&mut Transform, &mut CharacterController, &mut PlayerInput, &mut ActionIntent), With<Player>>,
) {
    use std::sync::atomic::Ordering::Relaxed;
    let Ok((mut tf, mut ctrl, mut input, mut intent)) = player.single_mut() else { return };
    let (t, dt) = (time.elapsed_secs(), time.delta_secs());
    let at = |s: f32| crossed(t, dt, s);
    let mut place = |x: f32, z: f32, facing: f32| {
        tf.translation = Vec3::new(x, tf.translation.y, z);
        ctrl.velocity = Vec3::ZERO;
        ctrl.facing = facing;
    };
    let press_e = |input: &mut PlayerInput| {
        info!("autopilot: press E");
        input.interact = true;
    };
    if session.is_host() {
        if at(3.5) {
            place(-0.8, 10.3, std::f32::consts::PI);
        }
        if at(4.0) {
            press_e(&mut input);
        }
        return;
    }
    if at(2.0) {
        place(0.0, 1.5, 0.0);
    }
    if at(2.6) || at(3.4) {
        intent.buffer(ActionRequest::LightAttack);
    }
    if at(4.2) {
        intent.buffer(ActionRequest::HeavyAttack);
    }
    if at(6.0) {
        place(0.8, 10.3, std::f32::consts::PI);
    }
    if at(6.5) {
        press_e(&mut input);
    }
    if at(9.0) {
        AUTO_WALK.store(true, Relaxed);
    }
    if at(9.6) {
        AUTO_WALK.store(false, Relaxed);
    }
}

// ── Menu screenshots (FLOWERBOX_AUTOPILOT=menu, FLOWERBOX_MENU_PAGE=main|settings|controls) ──

fn menu_script(time: Res<Time>, mut menu: ResMut<crate::ui::pause_menu::PauseMenu>) {
    use crate::ui::pause_menu::Page;
    if crossed(time.elapsed_secs(), time.delta_secs(), 0.5) {
        let page = match std::env::var("FLOWERBOX_MENU_PAGE").as_deref() {
            Ok("settings") => Page::Settings,
            Ok("controls") => Page::Controls,
            _ => Page::Main,
        };
        menu.show(page);
    }
}

/// Sounds started so far (and playing now), every 2 s: shows the sound triggers work even where
/// nobody can listen.
fn log_audio(
    time: Res<Time>,
    mut started: Local<usize>,
    new: Query<(), Added<AudioPlayer>>,
    sinks: Query<(), With<bevy::audio::SpatialAudioSink>>,
) {
    *started += new.iter().count();
    if (time.elapsed_secs() / 2.0).floor() != ((time.elapsed_secs() - time.delta_secs()) / 2.0).floor() {
        info!("audio: {} started so far, {} playing", *started, sinks.iter().count());
    }
}

// ── Level walk ───────────────────────────────────────────────────────────

/// `FLOWERBOX_POS="x,z[,yaw]"`: start at that floor point.
fn place_player(layout: Option<Res<crate::level::layout::Layout>>, mut player: Query<(&mut Transform, &mut CharacterController), With<Player>>) {
    let Ok(s) = std::env::var("FLOWERBOX_POS") else { return };
    let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    let Ok((mut tf, mut ctrl)) = player.single_mut() else { return };
    if v.len() < 2 {
        return;
    }
    let floor = layout.and_then(|l| l.floor_at(Vec2::new(v[0], v[1]))).unwrap_or(0.0);
    tf.translation = Vec3::new(v[0], floor + PLAYER_HALF_HEIGHT + GROUND_SKIN, v[1]);
    ctrl.facing = v.get(2).copied().unwrap_or(0.0);
    tf.rotation = Quat::from_rotation_y(ctrl.facing);
    info!("placed at {:.2}", tf.translation);
}

/// Movement the level script wants (world space, sprint, jump).
static LEVEL_DIR: std::sync::Mutex<(Vec3, bool, bool)> = std::sync::Mutex::new((Vec3::ZERO, false, false));

/// What to do on reaching a waypoint.
#[derive(Clone, Copy, PartialEq)]
enum At {
    Go,
    /// Press E, then wait this long.
    Interact(f32),
    /// Put the Forge Key in the bag (the warg that drops it isn't there without enemies).
    GiveKey,
}

/// Drawing pixels (level 1's design) → world (x, z).
const fn px(x: f32, y: f32) -> (f32, f32) {
    ((x - 409.5) / 14.0, -18.0 + (y - 1228.0) / 14.0)
}

const LEVEL_ROUTE: [((f32, f32), At); 59] = [
    ((2.8, 7.6), At::Go), // around the rubble…
    ((2.5, 3.0), At::Go), // …and the training dummy
    ((1.5, -12.0), At::Go),
    ((0.0, -16.6), At::Interact(1.8)), // door 1, from the arena
    (px(409.5, 1180.0), At::Go),
    (px(400.0, 1070.0), At::Go),
    (px(395.0, 1038.0), At::Go),
    (px(445.0, 1038.0), At::Go), // stairs 1 (down east)
    (px(512.0, 1038.0), At::Go),
    (px(535.0, 1000.0), At::Go),
    (px(535.0, 940.0), At::Go),
    (px(530.0, 800.0), At::Go),
    (px(522.0, 789.0), At::Interact(1.8)), // door 2, from the middle hall
    (px(480.0, 789.0), At::Go),
    (px(420.0, 795.0), At::Go),
    (px(356.0, 795.0), At::Go),
    (px(356.0, 720.0), At::Go),
    (px(355.0, 650.0), At::Go), // the narrow neck into the T room
    (px(356.0, 578.0), At::Interact(0.8)), // door 5 from the T room: the wrong side
    (px(355.0, 650.0), At::Go),
    (px(356.0, 720.0), At::Go),
    (px(230.0, 716.0), At::Go),
    (px(180.0, 716.0), At::Go),
    (px(180.0, 600.0), At::Go),
    (px(73.0, 597.0), At::Go),
    (px(86.0, 694.0), At::Interact(1.2)), // a Hop Cone
    (px(73.0, 688.0), At::Interact(3.5)), // sunflower 1: rest, revive (walking on stands up)
    (px(73.0, 597.0), At::Go),
    (px(180.0, 600.0), At::Go),
    (px(180.0, 716.0), At::Go),
    (px(356.0, 716.0), At::Go),
    (px(392.0, 697.0), At::Go),
    (px(505.0, 697.0), At::Interact(1.8)), // door 3, stairs 2 → vault
    (px(540.0, 697.0), At::Go),
    (px(610.0, 650.0), At::Go),
    (px(628.0, 578.0), At::Interact(0.8)), // door 4 without the key: locked
    (px(628.0, 580.0), At::GiveKey),
    (px(628.0, 578.0), At::Interact(1.8)), // with the key
    (px(628.0, 540.0), At::Go),
    (px(602.0, 470.0), At::Go),
    (px(602.0, 430.0), At::Go), // stairs 3, climbing north
    (px(602.0, 360.0), At::Go),
    (px(560.0, 356.0), At::Go),
    (px(474.0, 356.0), At::Go),
    (px(474.0, 318.0), At::Go), // the peak
    (px(474.0, 392.0), At::Go), // the dip
    (px(420.0, 356.0), At::Go),
    (px(345.0, 352.0), At::Go),
    (px(340.0, 330.0), At::Go), // the nook
    (px(356.0, 360.0), At::Go),
    (px(356.0, 440.0), At::Go),
    (px(300.0, 452.0), At::Go), // the alcove
    (px(356.0, 452.0), At::Go),
    (px(400.0, 478.0), At::Go), // into the forgery, around the anvil
    (px(458.0, 478.0), At::Go),
    (px(454.0, 461.0), At::Interact(1.2)), // the Well-Forged Sword
    (px(356.0, 478.0), At::Go),
    (px(356.0, 548.0), At::Interact(1.8)), // door 5 from the forgery: opens
    (px(356.0, 600.0), At::Go), // back in the T room
];

#[derive(Resource, Default)]
struct LevelRun {
    next: usize,
    wait: f32,
    /// Time without progress toward the waypoint, and the best distance so far.
    stall: f32,
    best: f32,
    log: f32,
    frames: u32,
    done: bool,
}

#[allow(clippy::type_complexity)]
fn level_script(
    time: Res<Time>,
    mut run: ResMut<LevelRun>,
    mut exit: MessageWriter<AppExit>,
    hearths: Query<&crate::hearth::Hearth>,
    doors: Query<&crate::level::door::Door>,
    mut player: Query<(Entity, &Transform, &CharacterController, &mut PlayerInput, &mut Inventory), With<Player>>,
    spatial: avian3d::prelude::SpatialQuery,
    names: Query<&Name>,
    parents: Query<&ChildOf>,
) {
    let Ok((player_entity, tf, ctrl, mut input, mut inventory)) = player.single_mut() else { return };
    let dt = time.delta_secs();
    *LEVEL_DIR.lock().unwrap() = (Vec3::ZERO, false, false);
    if run.done {
        return;
    }
    run.log += dt;
    run.frames += 1;
    if run.log >= 1.0 {
        info!("level walk: at {:.2} grounded {} (waypoint {}) {:.0} FPS", tf.translation, ctrl.grounded, run.next, run.frames as f32 / run.log);
        run.log = 0.0;
        run.frames = 0;
    }
    if run.wait > 0.0 {
        run.wait -= dt;
        return;
    }
    let Some(&((x, z), at)) = LEVEL_ROUTE.get(run.next) else {
        run.done = true;
        let revived: Vec<u32> = hearths.iter().filter(|h| h.kindled).map(|h| h.id).collect();
        let open: Vec<u32> = doors.iter().filter(|d| d.open).map(|d| d.id).collect();
        let sword = inventory.items.contains(&ItemId::WellForgedSword);
        info!("level walk: done at {:.2}; revived sunflowers {revived:?}; open doors {open:?}; has the sword: {sword}", tf.translation);
        exit.write(AppExit::Success);
        return;
    };
    let to = Vec3::new(x, 0.0, z) - tf.translation.with_y(0.0);
    let dist = to.length();
    if dist < 0.35 {
        info!("level walk: reached waypoint {} at {:.2}", run.next, tf.translation);
        run.next += 1;
        run.stall = 0.0;
        run.best = f32::MAX;
        match at {
            At::Interact(wait) => {
                input.interact = true;
                run.wait = wait;
            }
            At::GiveKey => {
                inventory.add(ItemId::ForgeKey);
                info!("level walk: got the Forge Key");
            }
            At::Go => {}
        }
        return;
    }
    if dist < run.best - 0.05 {
        run.best = dist;
        run.stall = 0.0;
    } else {
        run.stall += dt;
        if run.stall > 3.0 {
            // Name what's in the way.
            let blocker = Dir3::new(to).ok().and_then(|d| {
                spatial.cast_shape(
                    &avian3d::prelude::Collider::capsule(PLAYER_RADIUS, PLAYER_CAPSULE_LENGTH),
                    tf.translation,
                    Quat::IDENTITY,
                    d,
                    &avian3d::prelude::ShapeCastConfig::from_max_distance(0.5),
                    &avian3d::prelude::SpatialQueryFilter::from_excluded_entities([player_entity]),
                )
            });
            let what = blocker.map(|h| (h.entity, names.get(h.entity).map(|n| n.to_string()).unwrap_or_default(), parents.get(h.entity).ok().and_then(|p| names.get(p.parent()).ok()).map(|n| n.to_string())));
            warn!("level walk: STUCK before waypoint {} at {:.2} ({dist:.2} m away), blocked by {what:?}", run.next, tf.translation);
            run.stall = 0.0;
            run.best = f32::MAX;
            run.next += 1; // try the next one
        }
    }
    *LEVEL_DIR.lock().unwrap() = (to / dist, false, false);
}

fn log_player_pos(q: Query<&Transform, With<Player>>) {
    if let Ok(tf) = q.single() {
        info!("player at {:.2}", tf.translation);
    }
}

fn open_all_doors(mut doors: Query<&mut crate::level::door::Door>) {
    let swing = std::env::var("FLOWERBOX_SWING").is_ok();
    for mut d in &mut doors {
        if !d.open {
            if swing {
                d.open = true; // opens with the swing animation, like in play
            } else {
                d.open_now();
            }
        }
    }
}

/// While a menu is open, gameplay input must stay blocked and the cursor free (every frame).
fn check_menu_block(
    pause: Res<crate::ui::pause_menu::PauseMenu>,
    inventory: Res<InventoryMenu>,
    blocked: Res<GameplayBlocked>,
    cursor: Single<&bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>,
    mut frames: Local<(u32, u32)>,
) {
    if !(pause.open || inventory.open) {
        return;
    }
    frames.0 += 1;
    if !blocked.0 || crate::input::cursor_grabbed(&cursor) {
        frames.1 += 1;
        warn!("menu open but input not blocked (blocked {}, cursor grabbed {})", blocked.0, crate::input::cursor_grabbed(&cursor));
    }
    if frames.0 % 120 == 0 {
        info!("menu check: {} frames open, {} leaked", frames.0, frames.1);
    }
}

// ── Warg test ────────────────────────────────────────────────────────────

#[derive(Resource, Default)]
struct WargRun {
    roll: bool,
    last_kind: Option<ActionKind>,
    rolled_for: Option<u32>,
    roll_until: f32,
    pounces: u32,
    grabs: u32,
    player_was_grabbed: bool,
    hp_before_grab: f32,
    end_at: Option<f32>,
    /// Online guest: moved back to `FLOWERBOX_POS` after joining (joining places you in the arena).
    placed_as_guest: bool,
    joined_at: Option<f32>,
}

#[allow(clippy::type_complexity)]
fn warg_script(
    time: Res<Time>,
    mut run: ResMut<WargRun>,
    mut exit: MessageWriter<AppExit>,
    wargs: Query<(&Enemy, &Transform, &CharacterAction), Without<Player>>,
    mut player: Query<(&mut Transform, &CharacterAction, &mut ActionIntent, &mut Health), With<Player>>,
    session: Res<crate::net::NetSession>,
    layout: Option<Res<crate::level::layout::Layout>>,
) {
    let t = time.elapsed_secs();
    *LEVEL_DIR.lock().unwrap() = (Vec3::ZERO, false, false);
    let Ok((mut ptf, paction, mut intent, mut hp)) = player.single_mut() else { return };
    if session.is_client() && run.joined_at.is_none() {
        run.joined_at = Some(t);
    }
    if run.joined_at.is_some_and(|j| t > j + 1.0) && !run.placed_as_guest {
        run.placed_as_guest = true;
        let v: Vec<f32> = std::env::var("FLOWERBOX_POS").unwrap_or_default().split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if let [x, z, ..] = v[..] {
            let floor = layout.as_ref().and_then(|l| l.floor_at(Vec2::new(x, z))).unwrap_or(0.0);
            ptf.translation = Vec3::new(x, floor + PLAYER_HALF_HEIGHT + GROUND_SKIN, z);
            info!("autopilot: guest placed at {:.2}", ptf.translation);
        }
    }
    let ptf = ptf.into_inner().clone();
    if (run.roll || std::env::var("FLOWERBOX_WARG_TANK").is_ok()) && hp.max < 1000.0 {
        *hp = Health::new(1000.0); // survive the bites to face several pounces
    }
    let Some((_, wtf, waction)) = wargs.iter().find(|(e, _, _)| e.kind == crate::enemy::EnemyKind::Warg) else { return };
    let kind = waction.current.map(|a| a.kind);
    if kind != run.last_kind {
        info!("warg: {:?} -> {:?} (distance {:.2})", run.last_kind, kind, (wtf.translation - ptf.translation).with_y(0.0).length());
        if kind == Some(ActionKind::Pounce) {
            run.pounces += 1;
        }
        run.last_kind = kind;
    }
    let grabbed = paction.current.is_some_and(|a| a.kind == ActionKind::Grabbed);
    if grabbed && !run.player_was_grabbed {
        run.grabs += 1;
        info!("autopilot: GRABBED at HP {}", hp.current);
    }
    if !grabbed && run.player_was_grabbed {
        info!("autopilot: released, HP {} (bite took {})", hp.current, run.hp_before_grab - hp.current);
    }
    if !grabbed {
        run.hp_before_grab = hp.current;
    }
    run.player_was_grabbed = grabbed;
    // Roll mode: roll toward the warg just before each leap.
    if run.roll {
        if let Some(cur) = waction.current.filter(|a| a.kind == ActionKind::Pounce && a.phase() == ActionPhase::Startup) {
            if cur.elapsed >= cur.def.startup - 0.12 && run.rolled_for != Some(waction.serial) {
                run.rolled_for = Some(waction.serial);
                run.roll_until = t + 0.15;
                info!("autopilot: roll into the leap");
            }
        }
        // Keep pressing until the roll starts (a bite's stagger may still be running).
        let rolling = paction.current.is_some_and(|a| a.kind == ActionKind::Roll);
        if t < run.roll_until && !rolling {
            intent.buffer(ActionRequest::Roll);
        }
        if t < run.roll_until {
            let toward = (wtf.translation - ptf.translation).with_y(0.0).normalize_or_zero();
            *LEVEL_DIR.lock().unwrap() = (toward, false, false);
        }
    }
    let dead = paction.current.is_some_and(|a| a.kind == ActionKind::Dead);
    if run.end_at.is_none() && (dead || t > 40.0 || (run.roll && run.pounces >= 3 && kind != Some(ActionKind::Pounce))) {
        run.end_at = Some(t + 1.5);
    }
    if run.end_at.is_some_and(|e| t >= e) {
        info!("warg test: {} pounces, {} grabs, HP {}, dead {dead}", run.pounces, run.grabs, hp.current);
        exit.write(AppExit::Success);
        run.end_at = Some(f32::INFINITY);
    }
}

/// Drop test: kill the warg (3 s), take its drop (5 s), world reset (7 s), kill it again (10 s).
fn wargkill_script(
    time: Res<Time>,
    mut damage: MessageWriter<DamageMessage>,
    mut reset: MessageWriter<WorldReset>,
    mut requests: MessageWriter<crate::items::pickup::PickupRequest>,
    mut exit: MessageWriter<AppExit>,
    wargs: Query<(Entity, &Enemy, &Health)>,
    pickups: Query<(&crate::items::pickup::Pickup, &Transform), Without<Player>>,
    mut player: Query<(&mut Transform, &Inventory), With<Player>>,
) {
    let (t, dt) = (time.elapsed_secs(), time.delta_secs());
    let at = |s: f32| crossed(t, dt, s);
    let warg = wargs.iter().find(|(_, e, h)| e.kind == crate::enemy::EnemyKind::Warg && e.home.y < -3.0 && !h.is_dead());
    if at(3.0) || at(10.0) {
        if let Some((e, _, _)) = warg {
            info!("autopilot: killing the vault warg");
            damage.write(DamageMessage { target: e, amount: 10_000.0, from: None, attacker: None });
        }
    }
    let key = pickups.iter().find(|(p, _)| p.item == ItemId::ForgeKey);
    if at(5.0) {
        if let (Some((p, ptf)), Ok((mut tf, _))) = (key, player.single_mut()) {
            tf.translation = ptf.translation + Vec3::Y * (PLAYER_HALF_HEIGHT + GROUND_SKIN);
            requests.write(crate::items::pickup::PickupRequest { pickup: p.id });
            info!("autopilot: taking the key (pickup {})", p.id);
        }
    }
    if at(7.0) {
        info!("autopilot: world reset (the warg comes back)");
        reset.write(WorldReset);
    }
    if at(12.5) {
        let keys = player.single().map_or(0, |(_, inv)| inv.count(ItemId::ForgeKey));
        info!("drop test: keys in the bag {keys}, key lying on the floor: {}", key.is_some());
        exit.write(AppExit::Success);
    }
}

/// Multiplayer check: what this machine sees of the others and of the host's enemies.
fn log_puppets(
    time: Res<Time>,
    mut damage: MessageWriter<DamageMessage>,
    me: Query<Entity, With<Player>>,
    mut timer: Local<f32>,
    mut appeared: Local<u32>,
    new: Query<(), Added<crate::net::sync::Puppet>>,
    puppets: Query<&crate::net::sync::Puppet>,
    session: Res<crate::net::NetSession>,
    real_enemies: Query<(), (With<Enemy>, Without<crate::character::Remote>)>,
) {
    *appeared += new.iter().count() as u32;
    // `FLOWERBOX_MP_DIE_AT=secs`: this player dies then (online: no world reset).
    if let Some(at) = std::env::var("FLOWERBOX_MP_DIE_AT").ok().and_then(|v| v.parse::<f32>().ok())
        && crossed(time.elapsed_secs(), time.delta_secs(), at)
        && let Ok(e) = me.single()
    {
        info!("autopilot: dying now");
        damage.write(DamageMessage { target: e, amount: 10_000.0, from: None, attacker: None });
    }
    *timer += time.delta_secs();
    if *timer < 2.0 {
        return;
    }
    *timer = 0.0;
    let players = puppets.iter().filter(|p| p.state.kind == crate::net::protocol::BodyKind::Player).count();
    let enemies = puppets.iter().count() - players;
    info!(
        "mp: {:?} id {}: {players} other players, {enemies} enemy puppets, {} own enemies, {} puppets appeared so far",
        session.role,
        session.my_id,
        real_enemies.iter().count(),
        *appeared
    );
}

/// Multiplayer fight test: go for the nearest living enemy (local or a puppet), swing when close.
fn mp_fight(
    time: Res<Time>,
    mut swing: Local<f32>,
    enemies: Query<(&Transform, &Health), (With<Enemy>, Without<Player>)>,
    mut player: Query<(&Transform, &mut ActionIntent), With<Player>>,
) {
    *LEVEL_DIR.lock().unwrap() = (Vec3::ZERO, false, false);
    let Ok((tf, mut intent)) = player.single_mut() else { return };
    let Some(target) = enemies
        .iter()
        .filter(|(_, h)| !h.is_dead())
        .map(|(t, _)| t.translation)
        .filter(|p| p.distance(tf.translation) < 25.0)
        .min_by(|a, b| a.distance(tf.translation).total_cmp(&b.distance(tf.translation)))
    else {
        return;
    };
    let to = (target - tf.translation).with_y(0.0);
    *swing -= time.delta_secs();
    if to.length() > 1.6 {
        *LEVEL_DIR.lock().unwrap() = (to.normalize(), false, false);
    } else if *swing <= 0.0 {
        *swing = 0.7;
        intent.buffer(ActionRequest::LightAttack);
    }
}

// ── Scripted input ───────────────────────────────────────────────────────

fn key_by_name(name: &str) -> Option<KeyCode> {
    use KeyCode::*;
    Some(match name {
        "ShiftLeft" => ShiftLeft,
        "ShiftRight" => ShiftRight,
        "ControlLeft" => ControlLeft,
        "AltLeft" => AltLeft,
        "Tab" => Tab,
        "Escape" => Escape,
        "Enter" => Enter,
        "Space" => Space,
        "KeyI" => KeyI,
        "KeyE" => KeyE,
        "KeyQ" => KeyQ,
        "KeyR" => KeyR,
        "KeyW" => KeyW,
        "KeyA" => KeyA,
        "KeyS" => KeyS,
        "KeyD" => KeyD,
        _ => return None,
    })
}

/// Injects the `FLOWERBOX_KEYS` script as real input events.
fn inject_keys(
    time: Res<Time<Real>>,
    window: Query<Entity, With<bevy::window::PrimaryWindow>>,
    mut keys: MessageWriter<bevy::input::keyboard::KeyboardInput>,
    mut mouse: MessageWriter<bevy::input::mouse::MouseButtonInput>,
) {
    use bevy::input::ButtonState;
    let Ok(window) = window.single() else { return };
    let script = std::env::var("FLOWERBOX_KEYS").unwrap_or_default();
    let (t, dt) = (time.elapsed_secs(), time.delta_secs());
    for step in script.split_whitespace() {
        let Some((at, what)) = step.split_once(':') else { continue };
        let Ok(at) = at.parse::<f32>() else { continue };
        if !(t - dt < at && at <= t) {
            continue;
        }
        let (name, state) = match what.strip_suffix('+') {
            Some(n) => (n, ButtonState::Pressed),
            None => (what.trim_end_matches('-'), ButtonState::Released),
        };
        let button = match name {
            "MouseLeft" => Some(MouseButton::Left),
            "MouseRight" => Some(MouseButton::Right),
            "MouseMiddle" => Some(MouseButton::Middle),
            _ => None,
        };
        info!("input script: {name} {state:?}");
        if let Some(button) = button {
            mouse.write(bevy::input::mouse::MouseButtonInput { button, state, window });
        } else if let Some(key_code) = key_by_name(name) {
            keys.write(bevy::input::keyboard::KeyboardInput {
                key_code,
                logical_key: bevy::input::keyboard::Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
                state,
                text: None,
                repeat: false,
                window,
            });
        }
    }
}

/// Logs menu / block / cursor state whenever it changes.
fn log_input_state(
    inventory: Res<InventoryMenu>,
    pause: Res<crate::ui::pause_menu::PauseMenu>,
    blocked: Res<GameplayBlocked>,
    cursor: Query<&bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>,
    mut last: Local<String>,
) {
    let Ok(c) = cursor.single() else { return };
    let now = format!("inventory {} pause {} blocked {} cursor {:?} visible {}", inventory.open, pause.open, blocked.0, c.grab_mode, c.visible);
    if *last != now {
        info!("input state: {now}");
        *last = now;
    }
}
