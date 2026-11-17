//! Enemies, built from the same parts as the player: a generic character (`crate::character`) with
//! a body (humanoid or quadruped), a weapon with its moveset, and a brain that writes the same
//! `MoveIntent` / `ActionIntent` the player's input does.
//!
//! - [`kinds`]: every enemy as data (`EnemyDef`: body, size, health, stance, Light, weapon, brain) and the
//!   one spawner for all of them.
//! - [`ai`]: what every brain shares (senses, sight, steering, combos);
//!   [`fighter`]: the humanoid brain (Hollow, Acolyte, Mourner, Thorned Saint);
//!   [`beast`]: the four-legged one (warg, Contorted).
//! - [`boss`]: boss fights (health bar, fog gate, "vanquished").
//! - [`dormancy`]: enemies far from every player sleep (no brain, animation or rendering).
//!
//! Death: the Hollow collapses, lies there for `CORPSE_TIME`, then vanishes. Resting at the Hearth
//! or respawning ([`WorldReset`]) brings every enemy back at home with full health — Souls rules.

use bevy::prelude::*;

use crate::character::action::{ActionKind, CharacterAction};
use crate::character::{CharacterSystems, Remote, SolidExt};
use crate::combat::Hurtbox;
use crate::hearth::WorldReset;
use crate::items::ItemId;
use crate::items::pickup::{ClaimedDrops, DropIds, ItemDropped, Pickup, PickupAssets, spawn_pickup};
use crate::lockon::Targetable;
use crate::tuning::*;
use crate::weapon::{EquipWeapon, WeaponSystems};

pub mod ai;
pub mod beast;
pub mod boss;
pub mod dormancy;
pub mod fighter;
pub mod kinds;
pub mod scholars;
pub mod warg;

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        // Fog gates, the boss bar and defeated bosses belong to the level, enemies or not.
        app.add_plugins(boss::BossPlugin);
        // The older autopilot scripts (combat, hearth, …) were written for an empty arena.
        if std::env::var("FLOWERBOX_AUTOPILOT").is_ok_and(|a| !matches!(a.as_str(), "enemy" | "net" | "netfight" | "warg" | "wargroll" | "wargkill" | "mp" | "bestiary" | "boss" | "crit")) {
            return;
        }
        app.init_resource::<EnemiesSuppressed>()
            .add_systems(Startup, create_spawn_points)
            .add_systems(Update, trigger_spawn_points.before(WeaponSystems::Equip))
            .add_plugins(dormancy::DormancyPlugin)
            .add_systems(FixedUpdate, (fighter::fighter_ai, beast::beast_ai).before(CharacterSystems::Actions))
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
/// What each one is: [`EnemyKind::def`] (`kinds`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum EnemyKind {
    /// The shambling humanoid swordsman.
    #[default]
    Hollow,
    /// A big wolf: a quadruped beast.
    Warg,
    /// A hooded cathedral Hollow with a hatchet, quicker and tougher.
    Acolyte,
    /// A veiled, eyeless stalker that moves only while unwatched (horror).
    Mourner,
    /// A pale body bent backwards, scuttling on all fours (horror).
    Contorted,
    /// The Thorned Saint, the cathedral's boss.
    Saint,
    /// The library's small, quick reader with a quill-knife.
    Scribe,
    /// A hound of pasted pages and ink.
    PageHound,
    /// The Archivist, the library's boss.
    Archivist,
}

pub use crate::character::Dormant;

/// Lying dead for this many seconds.
#[derive(Component)]
pub struct Corpse(f32);

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
    /// Facing while idle (default: toward the world's centre).
    pub facing: Option<f32>,
    seed: u32,
}

impl SpawnPoint {
    /// `n` makes each spawn point's AI rolls different (but the same every run).
    pub fn new(kind: EnemyKind, pos: Vec3, patrol: Option<Vec3>, n: u32) -> Self {
        Self { kind, pos, patrol, drops: vec![], spawned: None, facing: None, seed: 0x9E37_79B9 ^ (n * 7919 + 1) }
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
    defeated: Res<boss::DefeatedBosses>,
    mut points: Query<&mut SpawnPoint>,
    alive: Query<(), With<Enemy>>,
) {
    if suppressed.0 {
        return;
    }
    for mut point in &mut points {
        // A defeated boss stays dead.
        if point.kind.def().boss && defeated.0.contains(&point.kind) {
            continue;
        }
        // Its enemy was removed (we visited another world): arm again.
        if point.spawned.is_some_and(|e| !alive.contains(e)) {
            point.spawned = None;
        }
        if point.spawned.is_some() {
            continue;
        }
        let home = point.pos;
        let to_centre = (-home).with_y(0.0).normalize_or(Vec3::NEG_Z);
        let facing = point.facing.unwrap_or(f32::atan2(-to_centre.x, -to_centre.z));
        let enemy = kinds::spawn_enemy(&mut commands, &mut meshes, &mut materials, &mut equip, point.kind, home, point.patrol, facing, point.seed);
        commands.entity(enemy).insert(Drops(point.drops.clone()));
        point.spawned = Some(enemy);
        debug!("{} rises (spawn point at {:?})", point.kind.def().name, point.pos);
    }
}

/// Dead enemies stop being targets, lie there a while, then vanish (until the world resets).
#[allow(clippy::type_complexity)]
fn enemy_death(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &Enemy, &CharacterAction, Option<&mut Corpse>, &mut Visibility)>,
) {
    for (entity, enemy, action, corpse, mut vis) in &mut q {
        let dead = matches!(action.current, Some(a) if a.kind == ActionKind::Dead);
        match corpse {
            None if dead => {
                info!("a {} falls", enemy.kind.def().name);
                commands.entity(entity).insert(Corpse(0.0)).remove::<(Hurtbox, Targetable)>();
            }
            Some(mut c) => {
                c.0 += time.delta_secs();
                if c.0 >= CORPSE_TIME && *vis != Visibility::Hidden {
                    // Gone from sight; its collider has been off since it died (`set_solid`).
                    *vis = Visibility::Hidden;
                    commands.entity(entity).set_solid(false);
                }
            }
            None => {}
        }
    }
}

/// A freshly killed enemy (owned here, not a remote puppet) drops its loot by its body.
#[allow(clippy::too_many_arguments)]
fn drop_loot(
    mut commands: Commands,
    assets: Option<Res<PickupAssets>>,
    claimed: Res<ClaimedDrops>,
    mut ids: ResMut<DropIds>,
    mut rng: Local<u32>,
    mut dropped: MessageWriter<ItemDropped>,
    dead: Query<(&Transform, &Drops, &crate::character::BodySize), (Added<Corpse>, Without<Remote>)>,
    pickups: Query<&Pickup>,
) {
    let Some(assets) = assets else { return };
    for (tf, drops, size) in &dead {
        let floor = tf.translation - Vec3::Y * (size.half_height() + GROUND_SKIN);
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
            let id = drop.unique.unwrap_or_else(|| ids.next());
            let pos = floor + Vec3::new(0.3 * n as f32, 0.0, 0.25);
            spawn_pickup(&mut commands, &assets, id, drop.item, pos);
            dropped.write(ItemDropped { id, item: drop.item, pos, thrown: false });
            info!("an enemy dropped {}", drop.item.def().name);
        }
    }
}

/// A corpse whose body is alive again (a remote enemy the host revived): back in the fight.
#[allow(clippy::type_complexity)]
fn revive_corpses(mut commands: Commands, mut q: Query<(Entity, &Enemy, &CharacterAction, &mut Visibility), With<Corpse>>) {
    for (entity, enemy, action, mut vis) in &mut q {
        if !matches!(action.current, Some(a) if a.kind == ActionKind::Dead) {
            *vis = Visibility::Inherited;
            commands
                .entity(entity)
                .remove::<Corpse>()
                .set_solid(true)
                .insert((Hurtbox, Targetable { height: enemy.kind.def().target_height }));
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
