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

use crate::character::guard::Parried;
use crate::character::action::{ActionKind, ActionPhase, ActiveAction, CharacterAction};
use crate::character::{CharacterSystems, Remote, SolidExt};
use crate::character::stats::{DamageMessage, DamageTaken, Health};
use crate::humanoid::anim::action_pose;
use crate::humanoid::pose::bone_character_transform;
use crate::humanoid::{Bone, HumanoidRig, HumanoidSpec};
use crate::quadruped::anim::action_pose as quadruped_action_pose;
use crate::quadruped::pose::bone_character_transform as quadruped_bone;
use crate::quadruped::{QBone, QuadrupedRig, QuadrupedSpec};
use crate::hearth::WorldReset;
use crate::lockon::Targetable;
use crate::player::Player;
use crate::pvp::Pvp;
use crate::ability::AttackBuff;
use crate::weapon::{WeaponDef, Wielded};
use crate::tuning::*;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Hitstop>()
            .init_resource::<CameraShake>()
            .add_systems(Startup, spawn_dummy)
            .add_systems(FixedUpdate, (sword_hits, bite_hits).in_set(CharacterSystems::Combat))
            .add_message::<DummyStruck>()
            .add_systems(Update, (hit_feedback, parry_feedback, run_hitstop, dummy_struck, dummy_react, dummy_reset, dummy_animate).chain());
    }
}

/// Marks an entity weapons can hit. Needs a `Collider` and `Health`.
#[derive(Component)]
pub struct Hurtbox;

/// Sides in a fight: weapons don't hit their own team. Entities without a team (the training
/// dummy) can be hit by anyone.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Team {
    Player,
    Hostile,
}

/// What the current swing already hit: (target, blade index) — each blade of a dual attack can
/// hit a target once.
#[derive(Component, Default)]
pub struct SwingHits {
    serial: u32,
    hit: Vec<(Entity, usize)>,
}

/// Remaining real-time hitstop (game time runs at `HITSTOP_TIME_SCALE` meanwhile).
#[derive(Resource, Default)]
pub struct Hitstop {
    pub remaining: f32,
}

/// Camera shake "trauma" 0..1; the camera reads and decays it.
#[derive(Resource, Default)]
pub struct CameraShake {
    pub trauma: f32,
}

/// Blade (crossguard, tip) in world space for the given action state, held in `hand_bone`.
pub fn blade_world(
    tf: &Transform,
    spec: &HumanoidSpec,
    rig: &HumanoidRig,
    weapon: &WeaponDef,
    hand_bone: Bone,
    action: &ActiveAction,
) -> Option<(Vec3, Vec3)> {
    let (pose, _) = action_pose(action)?;
    let hand = bone_character_transform(spec, &pose, rig.root_height, hand_bone);
    let world = |along: f32| tf.transform_point(hand.transform_point(Vec3::Y * along));
    Some((world(weapon.blade.0), world(weapon.blade.1)))
}

#[allow(clippy::type_complexity)]
fn sword_hits(
    time: Res<Time>,
    spatial: SpatialQuery,
    hurtboxes: Query<(Option<&Team>, Option<&CharacterAction>, Has<Remote>, Has<Player>, Option<&Pvp>), With<Hurtbox>>,
    mut damage: MessageWriter<DamageMessage>,
    mut attackers: Query<(
        Entity,
        &Transform,
        &CharacterAction,
        &HumanoidSpec,
        &HumanoidRig,
        &Wielded,
        Option<&Team>,
        Option<&mut SwingHits>,
        Has<Player>,
        Has<Remote>,
        Option<&Pvp>,
        Option<&AttackBuff>,
    )>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    for (entity, tf, action, spec, rig, wielded, team, hits, local_player, remote, pvp, buff) in &mut attackers {
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
        // Held in both hands, or under a Warcry: harder hits.
        let bonus = if wielded.two_hand.is_some() { TWO_HAND_DAMAGE_MULT } else { 1.0 } * buff.map_or(1.0, |b| b.mult);
        // Sweep from the previous tick (clamped to the active window start) to now, sampling
        // the blade(s) every ~4 ms of action time so fast swings can't pass through a target.
        let from = (cur.elapsed - dt).max(cur.def.startup);
        let steps = (((cur.elapsed - from) / 0.004).ceil() as usize).clamp(1, 64);
        let filter = SpatialQueryFilter::from_excluded_entities([entity]);
        // A spinning attack may hit each target once per turn.
        let turn = match cur.kind {
            ActionKind::Ability(id) if id.turns() > 0.0 => (((cur.elapsed - cur.def.startup) / cur.def.active.max(1e-3)) * id.turns()).floor().max(0.0) as usize,
            _ => 0,
        };
        for (blade, (bone, weapon)) in wielded.blades(cur.hand).into_iter().enumerate() {
            let blade_index = blade + 2 * turn;
            for i in 0..=steps {
                let mut sample = cur;
                sample.elapsed = from + (cur.elapsed - from) * i as f32 / steps as f32;
                let Some((a, b)) = blade_world(tf, spec, rig, weapon, bone, &sample) else { continue };
                let blade = Collider::capsule_endpoints(BLADE_HIT_RADIUS, a, b);
                for target in spatial.shape_intersections(&blade, Vec3::ZERO, Quat::IDENTITY, &filter) {
                    let Ok((target_team, target_action, target_remote, target_is_us, target_pvp)) = hurtboxes.get(target) else { continue };
                    let me = Striker { team, pvp, local_player, remote };
                    let them = Struck { team: target_team, pvp: target_pvp, remote: target_remote, is_us: target_is_us };
                    if hits.hit.contains(&(target, blade_index)) || !may_hit(&me, &them) {
                        continue;
                    }
                    hits.hit.push((target, blade_index));
                    // Hitting a parried (stunned) character is a riposte.
                    let riposte = target_action.and_then(|a| a.current).is_some_and(|a| a.kind == ActionKind::Stunned);
                    let amount = (weapon.base_damage * mult * bonus * if riposte { RIPOSTE_MULT } else { 1.0 }).round();
                    info!("{} {target} with {:?} ({:?}) for {amount}", if riposte { "RIPOSTE on" } else { "hit" }, cur.kind, cur.hand);
                    damage.write(DamageMessage { target, amount, from: Some(tf.translation), attacker: Some(entity) });
                }
            }
        }
    }
}

/// The attacking side of a hit check.
struct Striker<'a> {
    team: Option<&'a Team>,
    pvp: Option<&'a Pvp>,
    /// The attacker is this machine's player.
    local_player: bool,
    /// The attacker is simulated on another machine (a puppet).
    remote: bool,
}

/// The side being hit.
struct Struck<'a> {
    team: Option<&'a Team>,
    pvp: Option<&'a Pvp>,
    remote: bool,
    /// The target is this machine's player.
    is_us: bool,
}

/// Whether this machine decides that `me` hits `them` (teams, PvP, online ownership). Shared by
/// sword and bite hits.
fn may_hit(me: &Striker, them: &Struck) -> bool {
    // Same side never hurts each other, except two players who both opted into PvP.
    let duel = me.pvp.is_some_and(|p| p.0) && them.pvp.is_some_and(|p| p.0);
    if me.team.is_some() && me.team == them.team && !duel {
        return false;
    }
    // Online PvP: a hit on another player is decided on *their* machine (from our puppet).
    if them.remote && them.team == Some(&Team::Player) {
        return false;
    }
    // Online: a remote body is only hit by *our* player (the damage is forwarded to its owner),
    // and a remote body's attack only hits *our* player (its own machine handles the rest). So
    // every hit is detected exactly once, by the machine of the victim or of the local attacker.
    !((them.remote && !me.local_player) || (me.remote && !them.is_us))
}

/// Radius of the sphere swept with a biting creature's snout.
const BITE_RADIUS: f32 = 0.28;

/// Snout tip in world space for the given action state (where bites land).
pub fn snout_world(tf: &Transform, spec: &QuadrupedSpec, rig: &QuadrupedRig, action: &ActiveAction) -> Option<Vec3> {
    let pose = quadruped_action_pose(action)?;
    let head = quadruped_bone(spec, &pose, rig.root_height, QBone::Head);
    Some(tf.transform_point(head.transform_point(spec.snout_tip())))
}

/// Bites of quadrupeds: like [`sword_hits`], with a sphere swept along the snout's path during an
/// attack's active phase (the same pose the animation shows). Natural weapons give the damage.
#[allow(clippy::type_complexity)]
fn bite_hits(
    time: Res<Time>,
    spatial: SpatialQuery,
    hurtboxes: Query<(Option<&Team>, Option<&CharacterAction>, Has<Remote>, Has<Player>, Option<&Pvp>), With<Hurtbox>>,
    mut damage: MessageWriter<DamageMessage>,
    mut biters: Query<(Entity, &Transform, &CharacterAction, &QuadrupedSpec, &QuadrupedRig, &Wielded, Option<&Team>, Option<&mut SwingHits>, Has<Remote>)>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    for (entity, tf, action, spec, rig, equipped, team, hits, remote) in &mut biters {
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
        let from = (cur.elapsed - dt).max(cur.def.startup);
        let steps = (((cur.elapsed - from) / 0.004).ceil() as usize).clamp(1, 64);
        let filter = SpatialQueryFilter::from_excluded_entities([entity]);
        for i in 0..=steps {
            let mut sample = cur;
            sample.elapsed = from + (cur.elapsed - from) * i as f32 / steps as f32;
            let Some(snout) = snout_world(tf, spec, rig, &sample) else { continue };
            for target in spatial.shape_intersections(&Collider::sphere(BITE_RADIUS), snout, Quat::IDENTITY, &filter) {
                let Ok((target_team, target_action, target_remote, target_is_us, target_pvp)) = hurtboxes.get(target) else { continue };
                let me = Striker { team, pvp: None, local_player: false, remote };
                let them = Struck { team: target_team, pvp: target_pvp, remote: target_remote, is_us: target_is_us };
                if hits.hit.contains(&(target, 0)) || !may_hit(&me, &them) {
                    continue;
                }
                hits.hit.push((target, 0));
                let riposte = target_action.and_then(|a| a.current).is_some_and(|a| a.kind == ActionKind::Stunned);
                let amount = (equipped.right.base_damage * mult * if riposte { RIPOSTE_MULT } else { 1.0 }).round();
                info!("bite on {target} with {:?} for {amount}", cur.kind);
                damage.write(DamageMessage { target, amount, from: Some(tf.translation), attacker: Some(entity) });
            }
        }
    }
}

/// A parry: a hard hitstop and a jolt (the attacker's stun happens in `guard::stun_parried`).
fn parry_feedback(mut parries: MessageReader<Parried>, mut hitstop: ResMut<Hitstop>, mut shake: ResMut<CameraShake>) {
    for p in parries.read() {
        info!("parry! defender {} attacker {:?}", p.defender, p.attacker);
        hitstop.remaining = hitstop.remaining.max(HITSTOP_HEAVY * 1.5);
        shake.trauma = (shake.trauma + SHAKE_HEAVY).min(1.0);
    }
}

/// Hitstop + camera shake whenever damage is dealt to a hurtbox by the player.
fn hit_feedback(
    mut taken: MessageReader<DamageTaken>,
    hurtboxes: Query<(), With<Hurtbox>>,
    player: Query<(Entity, &CharacterAction), With<Player>>,
    mut hitstop: ResMut<Hitstop>,
    mut shake: ResMut<CameraShake>,
) {
    let me = player.single().ok().map(|(e, _)| e);
    for hit in taken.read() {
        // Only our own hits and hits on us (not, online, a guest's hit applied on the host).
        let ours = me.is_some() && (hit.attacker == me || Some(hit.target) == me);
        if !hurtboxes.contains(hit.target) || !ours {
            continue;
        }
        let heavy = player
            .iter()
            .any(|(_, a)| matches!(a.current.map(|c| c.kind), Some(ActionKind::HeavyAttack | ActionKind::JumpAttack | ActionKind::RunningAttack)));
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
            Targetable { height: 0.5 },
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

/// The training dummy was struck: what every machine needs to animate it and show the number.
/// Offline / host: from the applied damage; online guests: from the host's broadcast.
#[derive(Message, Debug, Clone, Copy)]
pub struct DummyStruck {
    pub amount: f32,
    pub from: Option<Vec3>,
    pub killed: bool,
}

fn dummy_struck(mut taken: MessageReader<DamageTaken>, dummies: Query<(), With<Dummy>>, mut struck: MessageWriter<DummyStruck>) {
    for hit in taken.read() {
        if dummies.contains(hit.target) {
            struck.write(DummyStruck { amount: hit.amount, from: hit.from, killed: hit.killed });
        }
    }
}

fn dummy_react(mut commands: Commands, mut struck: MessageReader<DummyStruck>, mut dummies: Query<(Entity, &mut Dummy, &Transform)>) {
    for hit in struck.read() {
        let Ok((entity, mut dummy, tf)) = dummies.single_mut() else { continue };
        dummy.flash = 1.0;
        // Tilt away from the attacker.
        let away = hit.from.map_or(Vec3::Z, |f| (tf.translation - f).with_y(0.0).normalize_or(Vec3::Z));
        // Tilt about X moves the top toward +Z, tilt about Z moves it toward -X.
        let tilt_away = Vec2::new(away.z, -away.x);
        // Bigger hits knock it much further (a heavy swings it nearly flat).
        let strength = (1.2 + hit.amount * DUMMY_PUSH_PER_DAMAGE).min(DUMMY_PUSH_MAX);
        dummy.tilt_vel += tilt_away * strength;
        if hit.killed {
            dummy.broken = Some(DUMMY_RESET_TIME);
            dummy.fallen_tilt = tilt_away * 1.45;
            // Lying on the floor: walk over it (and it can't be hit) until it stands up again.
            commands.entity(entity).set_solid(false);
            info!("training dummy broken, resets in {DUMMY_RESET_TIME}s");
        }
    }
}

/// Resting or respawning stands the dummy back up at full health.
fn dummy_reset(mut commands: Commands, mut resets: MessageReader<WorldReset>, mut dummies: Query<(Entity, &mut Dummy, &mut Health)>) {
    if resets.read().count() == 0 {
        return;
    }
    for (entity, mut dummy, mut health) in &mut dummies {
        commands.entity(entity).set_solid(true);
        dummy.broken = None;
        dummy.fallen_tilt = Vec2::ZERO;
        *health = Health::new(health.max);
    }
}

fn dummy_animate(
    time: Res<Time>,
    mut commands: Commands,
    mut dummies: Query<(Entity, &mut Dummy, &mut Health, &Children)>,
    mut models: Query<(&DummyModel, &mut Transform)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs();
    for (entity, mut dummy, mut health, children) in &mut dummies {
        // Broken: fall over, then stand back up with full health.
        let target = if let Some(t) = dummy.broken.as_mut() {
            *t -= dt;
            if *t <= 0.0 {
                dummy.broken = None;
                *health = Health::new(health.max);
                commands.entity(entity).set_solid(true);
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
