//! Combat: sword hit detection, hit feedback (hitstop, camera shake) and the training dummy.
//!
//! Hit detection doesn't use physics sensors. Each fixed tick of an attack's active phase,
//! the blade — the equipped weapon (`crate::weapon`) in the humanoid's right hand, posed by the shared animation
//! (`humanoid::anim::action_pose` + forward kinematics, exactly what's on screen) — is swept
//! between the previous and current tick, and a capsule along it is tested against colliders
//! with a [`Hurtbox`]. Each swing (`CharacterAction::serial`) hits a target once. Works for any
//! armed humanoid character, not just the player.

use std::f32::consts::FRAC_PI_2;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::character::action::{ActionKind, ActionPhase, ActiveAction, CharacterAction};
use crate::character::CharacterSystems;
use crate::character::stats::{DamageMessage, DamageTaken, Health};
use crate::humanoid::anim::action_pose;
use crate::humanoid::pose::bone_character_transform;
use crate::humanoid::{Bone, HumanoidRig, HumanoidSpec};
use crate::hearth::WorldReset;
use crate::player::Player;
use crate::weapon::{EquippedWeapon, WeaponDef};
use crate::tuning::*;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Hitstop>()
            .init_resource::<CameraShake>()
            .add_systems(Startup, spawn_dummy)
            .add_systems(FixedUpdate, sword_hits.in_set(CharacterSystems::Combat))
            .add_systems(Update, (hit_feedback, run_hitstop, dummy_react, dummy_reset, dummy_animate).chain());
    }
}

/// Marks an entity the player's weapon can hit. Needs a `Collider` and `Health`.
#[derive(Component)]
pub struct Hurtbox;

/// Entities already hit by the current swing.
#[derive(Component, Default)]
pub struct SwingHits {
    serial: u32,
    hit: Vec<Entity>,
}

/// Remaining real-time hitstop (game time runs at `HITSTOP_TIME_SCALE` meanwhile).
#[derive(Resource, Default)]
pub struct Hitstop {
    remaining: f32,
}

/// Camera shake "trauma" 0..1; the camera reads and decays it.
#[derive(Resource, Default)]
pub struct CameraShake {
    pub trauma: f32,
}

/// Blade (crossguard, tip) in world space for the given action state.
pub fn blade_world(
    tf: &Transform,
    spec: &HumanoidSpec,
    rig: &HumanoidRig,
    weapon: &WeaponDef,
    action: &ActiveAction,
) -> Option<(Vec3, Vec3)> {
    let (pose, _) = action_pose(action)?;
    let hand = bone_character_transform(spec, &pose, rig.root_height, Bone::HandR);
    let world = |along: f32| tf.transform_point(hand.transform_point(Vec3::Y * along));
    Some((world(weapon.blade.0), world(weapon.blade.1)))
}

fn sword_hits(
    time: Res<Time>,
    spatial: SpatialQuery,
    hurtboxes: Query<(), With<Hurtbox>>,
    mut damage: MessageWriter<DamageMessage>,
    mut attackers: Query<(Entity, &Transform, &CharacterAction, &HumanoidSpec, &HumanoidRig, &EquippedWeapon, Option<&mut SwingHits>)>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    for (entity, tf, action, spec, rig, equipped, hits) in &mut attackers {
        let weapon = equipped.def;
        let Some(mut hits) = hits else {
            commands.entity(entity).insert(SwingHits::default());
            continue;
        };
        if hits.serial != action.serial {
            *hits = SwingHits { serial: action.serial, hit: vec![] };
        }
        let Some(cur) = action.current else { continue };
        let Some(mult) = cur.damage_multiplier() else { continue };
        if cur.phase() != ActionPhase::Active {
            continue;
        }
        // Sweep from the previous tick (clamped to the active window start) to now, sampling
        // the blade every ~4 ms of action time so fast swings can't pass through a target.
        let from = (cur.elapsed - dt).max(cur.def.startup);
        let steps = (((cur.elapsed - from) / 0.004).ceil() as usize).clamp(1, 64);
        let filter = SpatialQueryFilter::from_excluded_entities([entity]);
        for i in 0..=steps {
            let mut sample = cur;
            sample.elapsed = from + (cur.elapsed - from) * i as f32 / steps as f32;
            let Some((a, b)) = blade_world(tf, spec, rig, weapon, &sample) else { continue };
            let blade = Collider::capsule_endpoints(BLADE_HIT_RADIUS, a, b);
            for target in spatial.shape_intersections(&blade, Vec3::ZERO, Quat::IDENTITY, &filter) {
                if hurtboxes.contains(target) && !hits.hit.contains(&target) {
                    hits.hit.push(target);
                    let amount = (weapon.base_damage * mult).round();
                    info!("hit {target} with {:?} for {amount}", cur.kind);
                    damage.write(DamageMessage { target, amount, from: Some(tf.translation) });
                }
            }
        }
    }
}

/// Hitstop + camera shake whenever damage is dealt to a hurtbox by the player.
fn hit_feedback(
    mut taken: MessageReader<DamageTaken>,
    hurtboxes: Query<(), With<Hurtbox>>,
    player: Query<&CharacterAction, With<Player>>,
    mut hitstop: ResMut<Hitstop>,
    mut shake: ResMut<CameraShake>,
) {
    for hit in taken.read() {
        if !hurtboxes.contains(hit.target) {
            continue;
        }
        let heavy = player
            .iter()
            .any(|a| matches!(a.current.map(|c| c.kind), Some(ActionKind::HeavyAttack | ActionKind::JumpAttack | ActionKind::RunningAttack)));
        let (stop, trauma) = if heavy { (HITSTOP_HEAVY, SHAKE_HEAVY) } else { (HITSTOP_LIGHT, SHAKE_LIGHT) };
        hitstop.remaining = hitstop.remaining.max(stop);
        shake.trauma = (shake.trauma + trauma).min(1.0);
    }
}

fn run_hitstop(real: Res<Time<Real>>, mut hitstop: ResMut<Hitstop>, mut virt: ResMut<Time<Virtual>>) {
    if hitstop.remaining > 0.0 {
        hitstop.remaining -= real.delta_secs();
        virt.set_relative_speed(if hitstop.remaining > 0.0 { HITSTOP_TIME_SCALE } else { 1.0 });
    }
}

// ── Training dummy ───────────────────────────────────────────────────────

#[derive(Component)]
pub struct Dummy {
    /// Tilt (x = about X, y = about Z) and its velocity: a damped spring.
    tilt: Vec2,
    tilt_vel: Vec2,
    flash: f32,
    /// Counts down while broken; resets the dummy at 0.
    broken: Option<f32>,
    /// Tilt the broken dummy falls to (away from the killing blow).
    fallen_tilt: Vec2,
}

/// The dummy's visual part (wobbles around its base); its material flashes on hits.
#[derive(Component)]
struct DummyModel {
    material: Handle<StandardMaterial>,
}

const DUMMY_HALF_HEIGHT: f32 = 0.9;

fn spawn_dummy(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let straw = materials.add(StandardMaterial {
        base_color: Color::srgb(0.62, 0.5, 0.28),
        perceptual_roughness: 0.95,
        ..default()
    });
    let wood = materials.add(StandardMaterial {
        base_color: Color::srgb(0.3, 0.2, 0.12),
        perceptual_roughness: 0.9,
        ..default()
    });
    let (x, z) = DUMMY_POSITION;
    commands
        .spawn((
            Name::new("Training Dummy"),
            Dummy { tilt: Vec2::ZERO, tilt_vel: Vec2::ZERO, flash: 0.0, broken: None, fallen_tilt: Vec2::ZERO },
            Hurtbox,
            Health::new(DUMMY_HP),
            Transform::from_xyz(x, DUMMY_HALF_HEIGHT, z),
            Visibility::default(),
            RigidBody::Static,
            Collider::capsule(0.35, 1.1),
        ))
        .with_children(|root| {
            root.spawn((
                Name::new("Dummy Model"),
                DummyModel { material: straw.clone() },
                Transform::from_xyz(0.0, -DUMMY_HALF_HEIGHT, 0.0),
                Visibility::default(),
            ))
            .with_children(|m| {
                let mut part = |mesh: Mesh, mat: &Handle<StandardMaterial>, t: Transform| {
                    m.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat.clone()), t));
                };
                part(Cylinder::new(0.06, 0.8).into(), &wood, Transform::from_xyz(0.0, 0.4, 0.0));
                part(Capsule3d::new(0.3, 0.55).into(), &straw, Transform::from_xyz(0.0, 1.1, 0.0));
                part(Sphere::new(0.2).into(), &straw, Transform::from_xyz(0.0, 1.72, 0.0));
                part(
                    Cylinder::new(0.05, 1.3).into(),
                    &wood,
                    Transform::from_xyz(0.0, 1.35, 0.0).with_rotation(Quat::from_rotation_z(FRAC_PI_2)),
                );
                part(Cylinder::new(0.32, 0.06).into(), &wood, Transform::from_xyz(0.0, 0.85, 0.0));
            });
        });
}

fn dummy_react(mut taken: MessageReader<DamageTaken>, mut dummies: Query<(&mut Dummy, &Transform)>) {
    for hit in taken.read() {
        let Ok((mut dummy, tf)) = dummies.get_mut(hit.target) else { continue };
        dummy.flash = 1.0;
        // Tilt away from the attacker.
        let away = hit.from.map_or(Vec3::Z, |f| (tf.translation - f).with_y(0.0).normalize_or(Vec3::Z));
        // Tilt about X moves the top toward +Z, tilt about Z moves it toward -X.
        let tilt_away = Vec2::new(away.z, -away.x);
        let strength = 2.0 + hit.amount * 0.08;
        dummy.tilt_vel += tilt_away * strength;
        if hit.killed {
            dummy.broken = Some(DUMMY_RESET_TIME);
            dummy.fallen_tilt = tilt_away * 1.45;
            info!("training dummy broken, resets in {DUMMY_RESET_TIME}s");
        }
    }
}

/// Resting or respawning stands the dummy back up at full health.
fn dummy_reset(mut resets: MessageReader<WorldReset>, mut dummies: Query<(&mut Dummy, &mut Health)>) {
    if resets.read().count() == 0 {
        return;
    }
    for (mut dummy, mut health) in &mut dummies {
        dummy.broken = None;
        dummy.fallen_tilt = Vec2::ZERO;
        *health = Health::new(health.max);
    }
}

fn dummy_animate(
    time: Res<Time>,
    mut dummies: Query<(&mut Dummy, &mut Health, &Children)>,
    mut models: Query<(&DummyModel, &mut Transform)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs();
    for (mut dummy, mut health, children) in &mut dummies {
        // Broken: fall over, then stand back up with full health.
        let target = if let Some(t) = dummy.broken.as_mut() {
            *t -= dt;
            if *t <= 0.0 {
                dummy.broken = None;
                *health = Health::new(health.max);
                info!("training dummy reset");
            }
            dummy.fallen_tilt
        } else {
            Vec2::ZERO
        };
        // Damped spring toward the target tilt.
        let accel = (target - dummy.tilt) * 60.0 - dummy.tilt_vel * 7.0;
        dummy.tilt_vel += accel * dt;
        let vel = dummy.tilt_vel;
        dummy.tilt += vel * dt;
        dummy.flash = (dummy.flash - dt * 6.0).max(0.0);

        for child in children.iter() {
            let Ok((model, mut tf)) = models.get_mut(child) else { continue };
            tf.rotation = Quat::from_rotation_x(dummy.tilt.x) * Quat::from_rotation_z(dummy.tilt.y);
            if let Some(mut mat) = materials.get_mut(&model.material) {
                let f = dummy.flash * dummy.flash;
                mat.emissive = LinearRgba::rgb(4.0 * f, 3.5 * f, 3.0 * f);
            }
        }
    }
}
