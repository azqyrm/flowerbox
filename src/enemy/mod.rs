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
use crate::weapon::AttackHand;

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
use crate::character::Remote;
use crate::items::ItemId;
use crate::items::pickup::{ClaimedDrops, ItemDropped, Pickup, PickupAssets, spawn_pickup};

pub mod beast;
pub mod warg;

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        // The older autopilot scripts (combat, hearth, …) were written for an empty arena.
        if std::env::var("FLOWERBOX_AUTOPILOT").is_ok_and(|a| !matches!(a.as_str(), "enemy" | "net" | "netfight" | "warg" | "wargroll" | "wargkill" | "mp")) {
            return;
        }
        app.init_resource::<EnemiesSuppressed>()
            .add_systems(Startup, create_spawn_points)
            .add_systems(Update, trigger_spawn_points.before(WeaponSystems::Equip))
            .add_systems(FixedUpdate, (hollow_ai, beast::beast_ai).before(CharacterSystems::Actions))
            .add_systems(FixedUpdate, beast::pounce_tracking.in_set(CharacterSystems::Combat))
            .add_systems(Update, (enemy_death, drop_loot, revive_corpses, enemy_reset).chain());
    }
}

/// An enemy and where it lives (floor point). With `patrol` it walks between home and that
/// point while it hasn't noticed anyone.
#[derive(Component, Debug)]
pub struct Enemy {
    pub home: Vec3,
    pub patrol: Option<Vec3>,
    pub kind: EnemyKind,
}

/// Something an enemy may drop when killed. With `unique` (a pickup id) it drops only until
/// someone has taken it (`ClaimedDrops`, saved) — keys.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct DropDef {
    pub item: ItemId,
    #[serde(default = "always")]
    pub chance: f32,
    #[serde(default)]
    pub unique: Option<u32>,
}

fn always() -> f32 {
    1.0
}

/// What this enemy drops when killed (from its spawn point).
#[derive(Component, Clone, Debug, Default)]
pub struct Drops(pub Vec<DropDef>);

/// Which enemy a spawn point makes (level files name it; online it picks the puppet's body).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EnemyKind {
    /// The shambling humanoid swordsman (`HollowAi`).
    #[default]
    Hollow,
    /// A big wolf: a quadruped beast (`beast::BeastAi` with `warg::PROFILE`).
    Warg,
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
    /// Patrolling: heading for the patrol point (else home).
    outbound: bool,
    rng: u32,
}

impl HollowAi {
    fn new(seed: u32) -> Self {
        Self { state: AiState::Idle, timer: 0.0, combo_left: 0, strafe: 1.0, stuck: 0.0, detour: 0.0, outbound: true, rng: seed.max(1) }
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

/// A place in the world where an enemy lives. Its enemy exists from the start; once killed it stays
/// dead (a corpse, then gone) until the world resets — resting at the sunflower, or the world's
/// owner dying — when every enemy respawns, Souls-style. Only the world's owner (offline player or
/// host) spawns; guests see the host's enemies.
#[derive(Component, Debug)]
pub struct SpawnPoint {
    pub kind: EnemyKind,
    /// Floor point the enemy stands on.
    pub pos: Vec3,
    pub patrol: Option<Vec3>,
    pub drops: Vec<DropDef>,
    /// The enemy it spawned (alive or a corpse) since the last reset.
    pub spawned: Option<Entity>,
    seed: u32,
}

impl SpawnPoint {
    /// `n` makes each spawn point's AI rolls different (but the same every run).
    pub fn new(kind: EnemyKind, pos: Vec3, patrol: Option<Vec3>, n: u32) -> Self {
        Self { kind, pos, patrol, drops: vec![], spawned: None, seed: 0x9E37_79B9 ^ (n * 7919 + 1) }
    }

    pub fn with_drops(mut self, drops: Vec<DropDef>) -> Self {
        self.drops = drops;
        self
    }
}

fn create_spawn_points(mut commands: Commands) {
    for (i, (x, z)) in HOLLOW_SPAWNS.into_iter().enumerate() {
        commands.spawn((Name::new("Spawn point: Hollow"), SpawnPoint::new(EnemyKind::Hollow, Vec3::new(x, 0.0, z), None, i as u32)));
    }
}

/// Spawns the enemy of every spawn point that has none (world start, after a reset, back from
/// visiting another world). Not on guests (`EnemiesSuppressed`).
#[allow(clippy::too_many_arguments)]
fn trigger_spawn_points(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut equip: MessageWriter<EquipWeapon>,
    suppressed: Res<EnemiesSuppressed>,
    mut points: Query<&mut SpawnPoint>,
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
        let home = point.pos;
        let to_centre = (-home).with_y(0.0).normalize_or(Vec3::NEG_Z);
        let facing = f32::atan2(-to_centre.x, -to_centre.z);
        if point.kind == EnemyKind::Warg {
            let warg = warg::spawn_warg(&mut commands, &mut meshes, &mut materials, home, point.patrol, facing, point.seed);
            commands.entity(warg).insert(Drops(point.drops.clone()));
            point.spawned = Some(warg);
            info!("a warg prowls (spawn point at {:?})", point.pos);
            continue;
        }
        let spec = HumanoidSpec::hollow();
        let tf = Transform::from_translation(home + Vec3::Y * (PLAYER_HALF_HEIGHT + 0.05)).with_rotation(Quat::from_rotation_y(facing));
        let enemy = commands
            .spawn((
                Name::new("Hollow"),
                Enemy { home, patrol: point.patrol, kind: EnemyKind::Hollow },
                Drops(point.drops.clone()),
                HollowAi::new(point.seed),
                Hurtbox,
                Team::Hostile,
                Targetable { height: 0.55 },
                character_bundle(tf, HOLLOW_HP, HOLLOW_STAMINA),
            ))
            .id();
        let rig = spawn_humanoid(&mut commands, &mut meshes, &mut materials, enemy, &spec, PLAYER_HALF_HEIGHT);
        commands.entity(enemy).insert((spec, rig, HumanoidAnimator::default(), CharacterController { facing, ..default() }));
        equip.write(EquipWeapon::right(enemy, &hollow_blade::HOLLOW_SWORD));
        point.spawned = Some(enemy);
        info!("a Hollow rises (spawn point at {:?})", point.pos);
    }
}

#[allow(clippy::type_complexity)]
fn hollow_ai(
    time: Res<Time>,
    spatial: SpatialQuery,
    players: Query<(&Transform, &CharacterAction, &Team), Without<HollowAi>>,
    characters: Query<(), With<CharacterController>>,
    mut q: Query<(Entity, &Enemy, &mut HollowAi, &Transform, &CharacterController, &CharacterAction, &Health, &mut MoveIntent, &mut ActionIntent)>,
) {
    let dt = time.delta_secs();
    for (entity, enemy, mut ai, tf, ctrl, action, health, mut movement, mut intent) in &mut q {
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
        // Noticing needs a clear view (walls and closed doors hide you), and not a whole floor apart.
        let sees = |range: f32| {
            player_alive
                && dist < range
                && (ppos.y - tf.translation.y).abs() < 2.5
                && crate::level::line_of_sight(&spatial, tf.translation + Vec3::Y * 0.6, ppos + Vec3::Y * 0.6, &|e| {
                    e != entity && !characters.contains(e)
                })
        };
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
                if sees(HOLLOW_AGGRO_RANGE) {
                    info!("a Hollow notices you");
                    ai.set(AiState::Chase);
                } else if let Some(far) = enemy.patrol {
                    // Pace back and forth.
                    let goal = if ai.outbound { far } else { enemy.home };
                    let to = (goal - tf.translation).with_y(0.0);
                    if to.length() < 0.4 {
                        ai.outbound = !ai.outbound;
                    } else {
                        movement.dir = to.normalize() * HOLLOW_PATROL_FACTOR;
                    }
                }
            }
            AiState::Chase => {
                movement.face = Some(toward);
                if dist > HOLLOW_ATTACK_RANGE * 0.8 {
                    movement.dir = toward * HOLLOW_MOVE_FACTOR;
                }
                if dist < HOLLOW_ATTACK_RANGE && !busy && ai.timer <= 0.0 {
                    if ai.roll() < HOLLOW_HEAVY_CHANCE {
                        intent.buffer(ActionRequest::HeavyAttack(AttackHand::Right));
                        intent.attack_held = true;
                        ai.timer = 0.35 + 0.3 * ai.roll();
                        ai.combo_left = 0;
                    } else {
                        intent.buffer(ActionRequest::LightAttack(AttackHand::Right));
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
                        intent.buffer(ActionRequest::LightAttack(AttackHand::Right));
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
                if sees(HOLLOW_AGGRO_RANGE * 0.6) && to_home.length() < HOLLOW_LEASH * 0.7 {
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

/// First pickup id for chance drops (unique drops use their own ids from the level).
const DROP_ID_BASE: u32 = 50_000;

/// A freshly killed enemy (owned here, not a remote puppet) drops its loot by its body.
#[allow(clippy::too_many_arguments)]
fn drop_loot(
    mut commands: Commands,
    assets: Option<Res<PickupAssets>>,
    claimed: Res<ClaimedDrops>,
    mut next_id: Local<u32>,
    mut rng: Local<u32>,
    mut dropped: MessageWriter<ItemDropped>,
    dead: Query<(&Transform, &Drops), (Added<Corpse>, Without<Remote>)>,
    pickups: Query<&Pickup>,
) {
    let Some(assets) = assets else { return };
    for (tf, drops) in &dead {
        let floor = tf.translation - Vec3::Y * (PLAYER_HALF_HEIGHT + GROUND_SKIN);
        for (n, drop) in drops.0.iter().enumerate() {
            if let Some(u) = drop.unique
                && (claimed.0.contains(&u) || pickups.iter().any(|p| p.id == u))
            {
                continue; // already taken (or still lying here from an earlier kill)
            }
            // xorshift for the chance (deterministic per run).
            *rng = if *rng == 0 { 0x2545_F491 } else { *rng };
            *rng ^= *rng << 13;
            *rng ^= *rng >> 17;
            *rng ^= *rng << 5;
            if (*rng & 0xFFFF) as f32 / 65536.0 >= drop.chance {
                continue;
            }
            let id = drop.unique.unwrap_or_else(|| {
                *next_id += 1;
                DROP_ID_BASE + *next_id
            });
            let pos = floor + Vec3::new(0.3 * n as f32, 0.0, 0.25);
            spawn_pickup(&mut commands, &assets, id, drop.item, pos);
            dropped.write(ItemDropped { id, item: drop.item, pos });
            info!("an enemy dropped {}", drop.item.def().name);
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

/// World reset (rest / the owner respawning): every enemy goes (alive or a corpse) and respawns at
/// its spawn point next frame.
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
    info!("enemies reset: all respawn");
}
