//! Critical hits, Souls-style: a **backstab** (light attack from close behind a small humanoid
//! enemy) or a **riposte** (light attack into a foe reeling from a parry or a stance break). Instead
//! of the swing the attacker plays the critical (`ActionKind::Critical`: grab, run through, wrench
//! free; invulnerable while it plays) and the victim is pinned on the blade (`ActionKind::Pinned`),
//! thrown down and gets up again. The damage (× `BACKSTAB_MULT` / `CRIT_RIPOSTE_MULT`) lands at the
//! thrust, with a hitstop, a jolt, a wet crunch and a spray of blood.
//!
//! Online the attacker's machine decides (like every hit of its own player): it pins the victim
//! through a [`PinRequest`]; a host-owned enemy's pin is forwarded to the host (`net::sync`), the
//! damage goes the usual way of forwarded hits.

use bevy::prelude::*;

use crate::character::action::{ActionKind, ActiveAction, CharacterAction, CritKind};
use crate::character::controller::CharacterController;
use crate::character::stats::{DamageMessage, Health};
use crate::character::{ActionIntent, CharacterSystems, Remote};
use crate::combat::Team;
use crate::character::action::ActionRequest;
use crate::combat::{CameraShake, Hitstop};
use crate::humanoid::HumanoidRig;
use crate::player::Player;
use crate::sound::{PlaySound, Sound};
use crate::tuning::*;
use crate::weapon::Wielded;

pub struct CriticalPlugin;

impl Plugin for CriticalPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PinRequest>()
            .add_message::<CriticalLanded>()
            .add_systems(
                FixedUpdate,
                (
                    (pick_critical, apply_pins).chain().before(crate::character::action::start_buffered_action).in_set(CharacterSystems::Actions),
                    land_critical.in_set(CharacterSystems::Combat),
                ),
            )
            .add_systems(Update, (critical_feedback, fly_droplets));
    }
}

/// A small humanoid: it can be backstabbed (enemies whose `EnemyDef::backstab` is set, and their
/// online puppets).
#[derive(Component, Debug, Clone, Copy)]
pub struct Backstabbable;

/// On an attacker while its critical plays.
#[derive(Component, Debug, Clone, Copy)]
struct CriticalStrike {
    victim: Entity,
    kind: CritKind,
    landed: bool,
}

/// Pin `victim` on a blade: it plays `Pinned(kind)` facing `facing`. Local victims are pinned at
/// once; a puppet's pin is sent to its owner.
#[derive(Message, Debug, Clone, Copy)]
pub struct PinRequest {
    pub victim: Entity,
    pub kind: CritKind,
    pub facing: f32,
}

/// The blade went in (for the effects).
#[derive(Message, Debug, Clone, Copy)]
pub struct CriticalLanded {
    pub pos: Vec3,
}

/// Horizontal forward of a controller facing.
fn forward(facing: f32) -> Vec3 {
    Vec3::new(-facing.sin(), 0.0, -facing.cos())
}

fn facing_of(dir: Vec3) -> f32 {
    f32::atan2(-dir.x, -dir.z)
}

/// Can the attacker (at `me`, facing `my_facing`) crit a target at `them` facing `their_facing`?
/// `backstab` = the target may be backstabbed, `reeling` = it's stunned. Riposte wins.
pub fn critical_kind(me: Vec3, my_facing: f32, them: Vec3, their_facing: f32, backstab: bool, reeling: bool) -> Option<CritKind> {
    let to = Vec3::new(them.x - me.x, 0.0, them.z - me.z);
    let dist = to.length();
    if dist > CRIT_RANGE || dist < 1e-3 || (them.y - me.y).abs() > 1.0 {
        return None;
    }
    let to = to / dist;
    if forward(my_facing).angle_between(to) > CRIT_AIM {
        return None;
    }
    if reeling {
        return Some(CritKind::Riposte);
    }
    // Behind it: from us to it runs along where it faces.
    (backstab && forward(their_facing).angle_between(to) < BACKSTAB_ARC).then_some(CritKind::Backstab)
}

/// A light attack with a critical in reach becomes the critical.
#[allow(clippy::type_complexity)]
fn pick_critical(
    mut commands: Commands,
    mut attackers: Query<
        (Entity, &mut ActionIntent, &mut CharacterAction, &mut CharacterController, &mut Transform, &Wielded),
        (With<Player>, Without<Remote>),
    >,
    targets: Query<
        (Entity, &Transform, &CharacterController, &CharacterAction, &Health, Option<&Team>, Has<Backstabbable>),
        (With<HumanoidRig>, Without<Player>),
    >,
    mut pins: MessageWriter<PinRequest>,
) {
    for (me, mut intent, mut action, mut ctrl, mut tf, wielded) in &mut attackers {
        let Some(ActionRequest::LightAttack(hand)) = intent.buffered else { continue };
        if !ctrl.grounded || ctrl.sprinting || action.current.is_some_and(|a| !a.can_be_cancelled()) {
            continue;
        }
        let best = targets
            .iter()
            .filter(|(e, _, _, a, h, team, _)| {
                *e != me && !h.is_dead() && team.is_none_or(|t| *t != Team::Player) && !matches!(a.current.map(|c| c.kind), Some(ActionKind::Pinned(_) | ActionKind::Grabbed | ActionKind::Roll | ActionKind::Dead))
            })
            .filter_map(|(e, ttf, tctrl, ta, _, _, backstab)| {
                let reeling = ta.current.is_some_and(|c| c.kind == ActionKind::Stunned);
                let kind = critical_kind(tf.translation, ctrl.facing, ttf.translation, tctrl.facing, backstab, reeling)?;
                Some((e, ttf.translation, tctrl.facing, kind, ttf.translation.distance_squared(tf.translation)))
            })
            .min_by(|a, b| a.4.total_cmp(&b.4));
        let Some((victim, at, their_facing, kind)) = best.map(|b| (b.0, b.1, b.2, b.3)) else { continue };
        // Line up: behind it facing the same way, or in front of it, face to face.
        let (dir, victim_facing) = match kind {
            CritKind::Backstab => (forward(their_facing), their_facing),
            CritKind::Riposte => {
                let d = Vec3::new(at.x - tf.translation.x, 0.0, at.z - tf.translation.z).normalize_or(forward(ctrl.facing));
                (d, facing_of(-d))
            }
        };
        let spot = at - dir * CRIT_DISTANCE;
        tf.translation.x = spot.x;
        tf.translation.z = spot.z;
        ctrl.facing = facing_of(dir);
        ctrl.velocity = Vec3::ZERO;
        action.start(ActiveAction::new(ActionKind::Critical(kind), dir, wielded.class_for(hand)));
        intent.buffered = None;
        commands.entity(me).insert(CriticalStrike { victim, kind, landed: false });
        pins.write(PinRequest { victim, kind, facing: victim_facing });
        info!("CRITICAL {kind:?} on {victim}");
    }
}

/// Pins local victims (puppets are pinned by their owner, see `net::sync`).
fn apply_pins(mut pins: MessageReader<PinRequest>, mut victims: Query<(&mut CharacterAction, &mut CharacterController), Without<Remote>>) {
    for pin in pins.read() {
        let Ok((mut action, mut ctrl)) = victims.get_mut(pin.victim) else { continue };
        action.force(ActionKind::Pinned(pin.kind), Vec3::ZERO);
        ctrl.facing = pin.facing;
        ctrl.velocity = Vec3::ZERO;
    }
}

/// The thrust: the damage, the feedback.
#[allow(clippy::type_complexity)]
fn land_critical(
    mut commands: Commands,
    mut attackers: Query<(Entity, &Transform, &CharacterAction, &mut CriticalStrike, &Wielded)>,
    victims: Query<&GlobalTransform>,
    mut damage: MessageWriter<DamageMessage>,
    mut landed: MessageWriter<CriticalLanded>,
) {
    for (me, tf, action, mut strike, wielded) in &mut attackers {
        let Some(cur) = action.current.filter(|c| c.kind == ActionKind::Critical(strike.kind)) else {
            commands.entity(me).remove::<CriticalStrike>();
            continue;
        };
        if strike.landed || cur.elapsed < cur.def.startup {
            continue;
        }
        strike.landed = true;
        let mult = match strike.kind {
            CritKind::Backstab => BACKSTAB_MULT,
            CritKind::Riposte => CRIT_RIPOSTE_MULT,
        };
        let two = if wielded.two_hand.is_some() { TWO_HAND_DAMAGE_MULT } else { 1.0 };
        let amount = (wielded.main().base_damage * mult * two).round();
        info!("critical {:?} lands for {amount}", strike.kind);
        damage.write(DamageMessage { target: strike.victim, amount, from: Some(tf.translation), attacker: Some(me), stance: 0.0 });
        let pos = victims.get(strike.victim).map_or(tf.translation + forward(0.0) * CRIT_DISTANCE, |g| g.translation()) + Vec3::Y * 0.35;
        landed.write(CriticalLanded { pos });
    }
}

/// A drop of blood flying off the blade.
#[derive(Component)]
struct Droplet {
    velocity: Vec3,
    life: f32,
    age: f32,
    size: f32,
}

/// Hitstop, a jolt, the sound and a spray of blood.
fn critical_feedback(
    mut commands: Commands,
    mut landed: MessageReader<CriticalLanded>,
    mut hitstop: ResMut<Hitstop>,
    mut shake: ResMut<CameraShake>,
    mut sounds: MessageWriter<PlaySound>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut seed: Local<u32>,
) {
    for hit in landed.read() {
        hitstop.remaining = hitstop.remaining.max(HITSTOP_HEAVY * 2.0);
        shake.trauma = (shake.trauma + SHAKE_HEAVY).min(1.0);
        sounds.write(PlaySound::at(Sound::Critical, hit.pos));
        let mesh = meshes.add(Sphere::new(0.014).mesh().ico(1).unwrap());
        let mat = materials.add(StandardMaterial {
            base_color: Color::srgb(0.35, 0.01, 0.02),
            emissive: LinearRgba::rgb(0.25, 0.0, 0.0),
            perceptual_roughness: 0.3,
            ..default()
        });
        for i in 0..26u32 {
            *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223 + i);
            let r = |k: u32| ((seed.rotate_left(k * 7) >> 8) as f32 / (u32::MAX >> 8) as f32) * 2.0 - 1.0;
            let velocity = Vec3::new(r(1) * 2.2, 1.2 + r(2).abs() * 2.5, r(3) * 2.2);
            commands.spawn((
                Droplet { velocity, life: 0.55 + r(4).abs() * 0.4, age: 0.0, size: 0.7 + r(5).abs() },
                Mesh3d(mesh.clone()),
                MeshMaterial3d(mat.clone()),
                Transform::from_translation(hit.pos),
            ));
        }
        // A brief glint where the blade went in.
        commands.spawn((
            Droplet { velocity: Vec3::ZERO, life: 0.12, age: 0.0, size: 1.0 },
            PointLight { color: Color::srgb(1.0, 0.55, 0.4), intensity: 60_000.0, range: 4.0, ..default() },
            Transform::from_translation(hit.pos),
        ));
    }
}

fn fly_droplets(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut Droplet, &mut Transform)>) {
    let dt = time.delta_secs();
    for (e, mut d, mut tf) in &mut q {
        d.age += dt;
        if d.age >= d.life {
            commands.entity(e).despawn();
            continue;
        }
        d.velocity.y -= 9.0 * dt;
        let v = d.velocity;
        tf.translation += v * dt;
        tf.scale = Vec3::splat(d.size * (1.0 - (d.age / d.life).powi(2)).max(0.05));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backstab_needs_to_be_behind_and_close() {
        // Target at the origin facing -Z (facing 0); we stand behind it at +Z, facing -Z.
        let me = Vec3::new(0.0, 0.0, 1.0);
        assert_eq!(critical_kind(me, 0.0, Vec3::ZERO, 0.0, true, false), Some(CritKind::Backstab));
        // Not a small humanoid: nothing (unless it reels).
        assert_eq!(critical_kind(me, 0.0, Vec3::ZERO, 0.0, false, false), None);
        assert_eq!(critical_kind(me, 0.0, Vec3::ZERO, 0.0, false, true), Some(CritKind::Riposte));
        // In front of it, face to face: no backstab.
        let front = Vec3::new(0.0, 0.0, -1.0);
        assert_eq!(critical_kind(front, std::f32::consts::PI, Vec3::ZERO, 0.0, true, false), None);
        // Behind but looking away from it.
        assert_eq!(critical_kind(me, std::f32::consts::PI, Vec3::ZERO, 0.0, true, false), None);
        // Too far.
        assert_eq!(critical_kind(Vec3::new(0.0, 0.0, 2.5), 0.0, Vec3::ZERO, 0.0, true, false), None);
    }
}
