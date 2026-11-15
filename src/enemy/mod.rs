//! Enemies. For now one kind: the **Hollow**, a shambling melee enemy built from the same parts as
//! the player — a generic character (`crate::character`) with a humanoid body
//! (`HumanoidSpec::hollow`), a weapon with a telegraphed moveset (`weapon::hollow_blade`) and an
//! AI that writes the same `MoveIntent` / `ActionIntent` the player's input does.
//!
//! AI states: **Idle** at home → notices the player within `HOLLOW_AGGRO_RANGE` (or when hit) →
//! **Chase** → in range: **Attack** (a one- or two-swing combo, or a charged heavy) → **Recover**
//! (keeps its distance, circles) → Chase … It gives up and walks **Home** when the player dies or
//! drags it farther than `HOLLOW_LEASH` from home.
//!
//! Death: the Hollow collapses, lies there for `CORPSE_TIME`, then vanishes. Resting at the Hearth
//! or respawning ([`WorldReset`]) brings every enemy back at home with full health — Souls rules.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::character::action::{ActionKind, ActionRequest, CharacterAction};
use crate::character::controller::CharacterController;
use crate::character::stats::Health;
use crate::character::{ActionIntent, CharacterSystems, MoveIntent, character_bundle};
use crate::combat::{Hurtbox, Team};
use crate::hearth::WorldReset;
use crate::humanoid::anim::HumanoidAnimator;
use crate::humanoid::{HumanoidSpec, spawn_humanoid};
use crate::lockon::Targetable;
use crate::tuning::*;
use crate::weapon::{EquipWeapon, WeaponSystems, hollow_blade};

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        // The older autopilot scripts (combat, hearth, …) were written for an empty arena.
        if std::env::var("FLOWERBOX_AUTOPILOT").is_ok_and(|a| a != "enemy" && a != "net" && a != "netfight") {
            return;
        }
        app.init_resource::<EnemiesSuppressed>()
            .add_systems(Startup, create_spawn_points)
            .add_systems(Update, trigger_spawn_points.before(WeaponSystems::Equip))
            .add_systems(FixedUpdate, hollow_ai.before(CharacterSystems::Actions))
            .add_systems(Update, (enemy_death, revive_corpses, enemy_reset));
    }
}

/// An enemy and where it lives.
#[derive(Component, Debug)]
pub struct Enemy {
    pub home: Vec3,
}

/// Lying dead for this many seconds.
#[derive(Component)]
struct Corpse(f32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiState {
    Idle,
    Chase,
    Attack,
    Recover,
    Home,
}

#[derive(Component, Debug)]
pub struct HollowAi {
    pub state: AiState,
    /// Recover: seconds left; Attack: seconds the heavy is still held.
    timer: f32,
    /// More swings to chain onto the current combo.
    combo_left: u8,
    /// Circling direction while recovering, and the side to step around obstacles (+1 / -1).
    strafe: f32,
    /// Seconds spent walking into something (wants to move, barely moves).
    stuck: f32,
    /// Seconds left of sidestepping around an obstacle.
    detour: f32,
    rng: u32,
}

impl HollowAi {
    fn new(seed: u32) -> Self {
        Self { state: AiState::Idle, timer: 0.0, combo_left: 0, strafe: 1.0, stuck: 0.0, detour: 0.0, rng: seed.max(1) }
    }

    /// xorshift: deterministic, so autopilot runs repeat.
    fn roll(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x & 0xFFFF) as f32 / 65536.0
    }

    fn set(&mut self, state: AiState) {
        if self.state != state {
            debug!("hollow: {:?} -> {:?}", self.state, state);
            self.state = state;
        }
    }
}

/// True while this machine is a guest in someone else's world: the host's enemies are the real
/// ones (they arrive as puppets), so none are spawned here (`crate::net`).
#[derive(Resource, Default)]
pub struct EnemiesSuppressed(pub bool);

/// A place in the world where an enemy appears when a player comes near — any player, local or
/// an online guest. It spawns once; a world reset (rest / respawn of the world's owner) clears its
/// enemy and arms it again. World-centred: a player joining later doesn't trigger another one.
#[derive(Component, Debug)]
pub struct SpawnPoint {
    pub pos: Vec2,
    pub radius: f32,
    /// The enemy it spawned (alive or a corpse) since the last reset.
    pub spawned: Option<Entity>,
    seed: u32,
}

fn create_spawn_points(mut commands: Commands) {
    for (i, (x, z)) in HOLLOW_SPAWNS.into_iter().enumerate() {
        commands.spawn((
            Name::new("Spawn point: Hollow"),
            SpawnPoint { pos: Vec2::new(x, z), radius: HOLLOW_SPAWN_RADIUS, spawned: None, seed: 0x9E37_79B9 ^ (i as u32 * 7919 + 1) },
        ));
    }
}

/// Spawns a spawn point's enemy when a player enters its area. Not on guests: the host's world
/// owns all enemies (`EnemiesSuppressed`).
#[allow(clippy::too_many_arguments)]
fn trigger_spawn_points(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut equip: MessageWriter<EquipWeapon>,
    suppressed: Res<EnemiesSuppressed>,
    mut points: Query<&mut SpawnPoint>,
    players: Query<(&Transform, &Team)>,
    alive: Query<(), With<Enemy>>,
) {
    if suppressed.0 {
        return;
    }
    for mut point in &mut points {
        // Its enemy was removed (we visited another world): arm again.
        if point.spawned.is_some_and(|e| !alive.contains(e)) {
            point.spawned = None;
        }
        if point.spawned.is_some() {
            continue;
        }
        let near = players.iter().any(|(tf, team)| *team == Team::Player && Vec2::new(tf.translation.x, tf.translation.z).distance(point.pos) < point.radius);
        if !near {
            continue;
        }
        let home = Vec3::new(point.pos.x, 0.0, point.pos.y);
        let to_centre = (-home).normalize_or(Vec3::NEG_Z);
        let facing = f32::atan2(-to_centre.x, -to_centre.z);
        let spec = HumanoidSpec::hollow();
        let tf = Transform::from_translation(home + Vec3::Y * (PLAYER_HALF_HEIGHT + 0.05)).with_rotation(Quat::from_rotation_y(facing));
        let enemy = commands
            .spawn((
                Name::new("Hollow"),
                Enemy { home },
                HollowAi::new(point.seed),
                Hurtbox,
                Team::Hostile,
                Targetable { height: 0.55 },
                character_bundle(tf, HOLLOW_HP, HOLLOW_STAMINA),
            ))
            .id();
        let rig = spawn_humanoid(&mut commands, &mut meshes, &mut materials, enemy, &spec, PLAYER_HALF_HEIGHT);
        commands.entity(enemy).insert((spec, rig, HumanoidAnimator::default(), CharacterController { facing, ..default() }));
        equip.write(EquipWeapon { character: enemy, def: &hollow_blade::HOLLOW_SWORD });
        point.spawned = Some(enemy);
        info!("a Hollow rises (spawn point at {:?})", point.pos);
    }
}

#[allow(clippy::type_complexity)]
fn hollow_ai(
    time: Res<Time>,
    players: Query<(&Transform, &CharacterAction, &Team), Without<HollowAi>>,
    mut q: Query<(&Enemy, &mut HollowAi, &Transform, &CharacterController, &CharacterAction, &Health, &mut MoveIntent, &mut ActionIntent)>,
) {
    let dt = time.delta_secs();
    for (enemy, mut ai, tf, ctrl, action, health, mut movement, mut intent) in &mut q {
        // Go for the nearest living player (the local one or an online guest's puppet).
        let nearest = players
            .iter()
            .filter(|(_, a, team)| **team == Team::Player && !matches!(a.current, Some(a) if a.kind == ActionKind::Dead))
            .map(|(t, _, _)| t.translation)
            .min_by(|a, b| a.distance_squared(tf.translation).total_cmp(&b.distance_squared(tf.translation)));
        let player_alive = nearest.is_some();
        let ppos = nearest.unwrap_or(tf.translation + Vec3::Z * 1000.0);
        intent.tick(dt);
        movement.dir = Vec3::ZERO;
        movement.sprint = false;
        movement.face = None;
        if health.is_dead() {
            intent.attack_held = false;
            continue;
        }
        let to_player = (ppos - tf.translation).with_y(0.0);
        let dist = to_player.length();
        let toward = to_player.normalize_or_zero();
        let to_home = (enemy.home - tf.translation).with_y(0.0);
        let busy = action.current.is_some();
        ai.timer -= dt;

        // Hit while idle: turn on the attacker.
        if ai.state == AiState::Idle && health.current < health.max && player_alive {
            ai.set(AiState::Chase);
        }
        if matches!(ai.state, AiState::Chase | AiState::Attack | AiState::Recover)
            && (!player_alive || to_home.length() > HOLLOW_LEASH)
        {
            intent.attack_held = false;
            ai.set(AiState::Home);
        }

        match ai.state {
            AiState::Idle => {
                if player_alive && dist < HOLLOW_AGGRO_RANGE {
                    info!("a Hollow notices you");
                    ai.set(AiState::Chase);
                }
            }
            AiState::Chase => {
                movement.face = Some(toward);
                if dist > HOLLOW_ATTACK_RANGE * 0.8 {
                    movement.dir = toward * HOLLOW_MOVE_FACTOR;
                }
                if dist < HOLLOW_ATTACK_RANGE && !busy && ai.timer <= 0.0 {
                    if ai.roll() < HOLLOW_HEAVY_CHANCE {
                        intent.buffer(ActionRequest::HeavyAttack);
                        intent.attack_held = true;
                        ai.timer = 0.35 + 0.3 * ai.roll();
                        ai.combo_left = 0;
                    } else {
                        intent.buffer(ActionRequest::LightAttack);
                        ai.combo_left = u8::from(ai.roll() < 0.5);
                    }
                    ai.set(AiState::Attack);
                }
            }
            AiState::Attack => {
                movement.face = Some(toward);
                intent.attack_held = intent.attack_held && ai.timer > 0.0;
                if let Some(cur) = action.current {
                    // Chain the next swing just before the cancel window (the buffer is short).
                    let chaining = matches!(cur.kind, ActionKind::LightAttack(_))
                        && ai.combo_left > 0
                        && cur.elapsed >= cur.def.cancel - 0.1;
                    if chaining {
                        ai.combo_left -= 1;
                        intent.buffer(ActionRequest::LightAttack);
                    }
                    if matches!(cur.kind, ActionKind::Hurt | ActionKind::Stunned) {
                        intent.attack_held = false;
                        intent.buffered = None;
                        ai.combo_left = 0;
                    }
                } else if intent.buffered.is_none() {
                    ai.timer = HOLLOW_RECOVER_TIME.0 + (HOLLOW_RECOVER_TIME.1 - HOLLOW_RECOVER_TIME.0) * ai.roll();
                    ai.strafe = if ai.roll() < 0.5 { -1.0 } else { 1.0 };
                    ai.set(AiState::Recover);
                }
            }
            AiState::Recover => {
                movement.face = Some(toward);
                if !busy {
                    let side = Vec3::new(-toward.z, 0.0, toward.x) * ai.strafe;
                    movement.dir = if dist < 1.6 {
                        -toward * 0.5
                    } else if dist < 3.5 {
                        side * 0.4
                    } else {
                        toward * HOLLOW_MOVE_FACTOR
                    };
                }
                if ai.timer <= 0.0 {
                    ai.set(AiState::Chase);
                }
            }
            AiState::Home => {
                if to_home.length() > 0.4 {
                    movement.dir = to_home.normalize() * HOLLOW_MOVE_FACTOR;
                } else {
                    ai.set(AiState::Idle);
                }
                if player_alive && dist < HOLLOW_AGGRO_RANGE * 0.6 && to_home.length() < HOLLOW_LEASH * 0.7 {
                    ai.set(AiState::Chase);
                }
            }
        }

        // Walking into something (rubble, a pillar): sidestep around it for a moment.
        let wants = movement.dir.length() * WALK_SPEED;
        let moved = Vec2::new(ctrl.moved.x, ctrl.moved.z).length();
        ai.stuck = if wants > 1.0 && moved < wants * 0.3 { ai.stuck + dt } else { 0.0 };
        if ai.stuck > 0.4 {
            ai.stuck = 0.0;
            ai.detour = 0.7;
            if ai.roll() < 0.5 {
                ai.strafe = -ai.strafe;
            }
        }
        if ai.detour > 0.0 && movement.dir.length_squared() > 0.01 {
            ai.detour -= dt;
            let d = movement.dir;
            let side = Vec3::new(-d.z, 0.0, d.x) * ai.strafe;
            movement.dir = (side * 0.8 + d * 0.3).normalize_or_zero() * d.length();
        }
    }
}

/// Dead enemies stop being targets, lie there a while, then vanish (until the world resets).
#[allow(clippy::type_complexity)]
fn enemy_death(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &CharacterAction, Option<&mut Corpse>, &mut Visibility), With<Enemy>>,
) {
    for (entity, action, corpse, mut vis) in &mut q {
        let dead = matches!(action.current, Some(a) if a.kind == ActionKind::Dead);
        match corpse {
            None if dead => {
                info!("a Hollow falls");
                commands.entity(entity).insert(Corpse(0.0)).remove::<(Hurtbox, Targetable)>();
            }
            Some(mut c) => {
                c.0 += time.delta_secs();
                if c.0 >= CORPSE_TIME && *vis != Visibility::Hidden {
                    *vis = Visibility::Hidden;
                    commands.entity(entity).insert(ColliderDisabled);
                }
            }
            None => {}
        }
    }
}

/// A corpse whose body is alive again (a remote enemy the host revived): back in the fight.
#[allow(clippy::type_complexity)]
fn revive_corpses(mut commands: Commands, mut q: Query<(Entity, &CharacterAction, &mut Visibility), (With<Enemy>, With<Corpse>)>) {
    for (entity, action, mut vis) in &mut q {
        if !matches!(action.current, Some(a) if a.kind == ActionKind::Dead) {
            *vis = Visibility::Inherited;
            commands
                .entity(entity)
                .remove::<(Corpse, ColliderDisabled)>()
                .insert((Hurtbox, Targetable { height: 0.55 }));
        }
    }
}

/// Resting / respawning (the world's owner): every spawned enemy goes, every spawn point is armed
/// again — the enemies come back when someone comes near.
fn enemy_reset(
    mut resets: MessageReader<WorldReset>,
    mut commands: Commands,
    suppressed: Res<EnemiesSuppressed>,
    mut points: Query<&mut SpawnPoint>,
) {
    if resets.read().count() == 0 || suppressed.0 {
        return;
    }
    for mut point in &mut points {
        if let Some(e) = point.spawned.take() {
            commands.entity(e).despawn();
        }
    }
    info!("enemies reset (spawn points armed)");
}
