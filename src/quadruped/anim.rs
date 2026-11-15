//! Shared quadruped animations, driven by any character's movement and actions.
//!
//! **Locomotion:** a gait cycle advanced by distance travelled. The gait follows the speed: a
//! **walk** (one paw at a time: hind left, front left, hind right, front right), a **trot**
//! (diagonal pairs) and a **gallop** (front pair, then hind pair, the body rocking). Each leg swings
//! its upper bone and folds its lower bone while lifted.
//!
//! **Actions** (a full-body layer on top): bites (`LightAttack`), the **pounce** (lean back and
//! crouch → leap stretched out → bite and shake), hurt, parry stagger and death (onto the side).
//! [`action_pose`] is pure, so combat samples the same pose for the bite hitbox.

use std::f32::consts::{PI, TAU};

use bevy::prelude::*;

use super::pose::QuadrupedPose;
use super::{QBone, QuadrupedRig, QuadrupedSpec};
use crate::character::action::{ActionKind, ActionPhase, ActiveAction, CharacterAction};
use crate::character::controller::CharacterController;

fn rx(a: f32) -> Quat {
    Quat::from_rotation_x(a)
}
fn ry(a: f32) -> Quat {
    Quat::from_rotation_y(a)
}
fn rz(a: f32) -> Quat {
    Quat::from_rotation_z(a)
}
fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
/// 0 → 1 → 0 over t = 0..1.
fn bell(t: f32) -> f32 {
    (PI * t.clamp(0.0, 1.0)).sin()
}

// ── Locomotion ───────────────────────────────────────────────────────────

/// A gait: when each leg (FL, FR, HL, HR) starts its cycle, and how far the legs swing.
#[derive(Clone, Copy, Debug)]
struct Gait {
    offsets: [f32; 4],
    swing: f32,
    fold: f32,
    /// Body rock (pitch) amplitude, a gallop's.
    rock: f32,
}

const WALK: Gait = Gait { offsets: [0.25, 0.75, 0.0, 0.5], swing: 0.32, fold: 0.7, rock: 0.0 };
const TROT: Gait = Gait { offsets: [0.0, 0.5, 0.5, 0.0], swing: 0.45, fold: 1.0, rock: 0.02 };
const GALLOP: Gait = Gait { offsets: [0.0, 0.1, 0.55, 0.65], swing: 0.7, fold: 1.3, rock: 0.09 };

fn mix_gait(a: &Gait, b: &Gait, t: f32) -> Gait {
    let l = |x: f32, y: f32| x + (y - x) * t;
    // Offsets don't blend well; switch halfway.
    Gait { offsets: if t < 0.5 { a.offsets } else { b.offsets }, swing: l(a.swing, b.swing), fold: l(a.fold, b.fold), rock: l(a.rock, b.rock) }
}

/// Gait and its blend from standing (0) to moving (1) for a speed (m/s).
fn gait_for(speed: f32) -> (Gait, f32) {
    let moving = (speed / 0.8).clamp(0.0, 1.0);
    let gait = if speed < 2.5 {
        WALK
    } else if speed < 4.0 {
        mix_gait(&WALK, &TROT, (speed - 2.5) / 1.5)
    } else if speed < 6.0 {
        TROT
    } else {
        mix_gait(&TROT, &GALLOP, ((speed - 6.0) / 1.0).min(1.0))
    };
    (gait, moving)
}

/// Distance covered per full gait cycle at `speed`.
pub fn stride_length(speed: f32) -> f32 {
    (0.9 + speed * 0.32).clamp(0.9, 3.2)
}

/// Everything the locomotion layer needs.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocomotionInput {
    pub speed: f32,
    /// Gait cycle phase (radians).
    pub phase: f32,
    pub time: f32,
}

pub fn locomotion_pose(input: &LocomotionInput) -> QuadrupedPose {
    let (gait, moving) = gait_for(input.speed);
    let mut p = QuadrupedPose::default();
    let u = input.phase / TAU;
    for (i, leg) in QBone::LEGS.iter().enumerate() {
        let c = (u + gait.offsets[i]).fract() * TAU;
        // Forward at touch-down, back at lift-off; folded while it swings forward again.
        let swing = gait.swing * c.cos() * moving;
        let lift = (-c.sin()).max(0.0) * gait.fold * moving;
        // Hind legs stand with a slight angle at the hock.
        let (rest_upper, rest_lower) = if leg.front { (0.0, 0.0) } else { (0.18, -0.3) };
        // Paws a little wider than the shoulders: a planted, predatory stance.
        p.set(leg.upper, rx(rest_upper + swing) * rz(-0.04 * leg.side)).set(leg.lower, rx(rest_lower - lift));
        p.set(leg.paw, rx(-rest_upper - rest_lower - swing * 0.5 + lift * 0.6));
    }
    // Body bob twice per cycle, a gallop's rock, idle breathing, head and tail.
    let bob = (2.0 * input.phase).sin().abs();
    p.hips_offset.y = -0.03 * bob * moving + 0.01 * (input.time * 1.8).sin() * (1.0 - moving);
    p.root_rot = rx(gait.rock * input.phase.sin() * moving);
    p.root_pivot = Vec3::new(0.0, 0.8, 0.0);
    p.set(QBone::Neck, rx(-0.15 * moving + 0.04 * (input.phase * 2.0).sin() * moving));
    p.set(QBone::Head, rx(0.1 * moving));
    let tail_up = 0.2 + 0.5 * (input.speed / 7.0).min(1.0);
    p.set(QBone::Tail, rx(-0.9 + tail_up) * ry(0.25 * (input.time * 1.7).sin() * (1.0 - moving * 0.6)));
    p
}

// ── Actions ──────────────────────────────────────────────────────────────

/// Startup → active → recovery as a 0 → 1 → 0 weight.
fn phase_weight(a: &ActiveAction) -> f32 {
    let d = &a.def;
    match a.phase() {
        ActionPhase::Startup => ease(a.elapsed / d.startup.max(1e-3)),
        ActionPhase::Active => 1.0,
        _ => 1.0 - ease((a.elapsed - d.startup - d.active) / d.recovery.max(1e-3)),
    }
}

/// Front legs braced forward, hind legs pushing: the stance of a lunge.
fn brace(p: &mut QuadrupedPose, k: f32) {
    p.set(QBone::UpperFL, rx(0.35 * k)).set(QBone::UpperFR, rx(0.25 * k));
    p.set(QBone::UpperHL, rx(-0.25 * k)).set(QBone::UpperHR, rx(-0.35 * k));
}

/// The full-body pose of an action, or `None` for actions a quadruped doesn't have.
pub fn action_pose(a: &ActiveAction) -> Option<QuadrupedPose> {
    let d = &a.def;
    let total = d.total();
    let mut p = QuadrupedPose { root_pivot: Vec3::new(0.0, 0.8, 0.0), ..default() };
    // Hind legs keep their hock angle in every action pose.
    for leg in QBone::LEGS.iter().filter(|l| !l.front) {
        p.set(leg.upper, rx(0.18)).set(leg.lower, rx(-0.3)).set(leg.paw, rx(0.12));
    }
    match a.kind {
        ActionKind::LightAttack(_) | ActionKind::HeavyAttack | ActionKind::RunningAttack | ActionKind::JumpAttack | ActionKind::HeavyCharge => {
            // Head drawn back and up with the jaws opening, then thrust forward and snapped shut.
            let k = phase_weight(a);
            let strike = match a.phase() {
                ActionPhase::Startup => 0.0,
                ActionPhase::Active => ease((a.elapsed - d.startup) / d.active.max(1e-3)),
                _ => 1.0,
            };
            let draw = k * (1.0 - strike);
            let thrust = k * strike;
            p.set(QBone::Neck, rx(0.35 * draw - 0.55 * thrust));
            p.set(QBone::Head, rx(0.15 * draw - 0.15 * thrust));
            p.set(QBone::Jaw, rx(0.75 * draw * (1.0 - strike) + 0.6 * draw));
            p.hips_offset.z = 0.12 * draw - 0.1 * thrust;
            p.chest_offset.y = -0.06 * thrust;
            brace(&mut p, k);
            Some(p)
        }
        ActionKind::Pounce => {
            let windup = ease(a.elapsed / d.startup.max(1e-3));
            match a.phase() {
                ActionPhase::Startup => {
                    // Leaning back for the leap: weight on the crouched hind legs, the front rearing
                    // up, head high, jaws starting to open. A slow growl-shiver at the end.
                    let shiver = (a.elapsed * 40.0).sin() * 0.02 * ease((a.elapsed - d.startup * 0.6) / (d.startup * 0.4));
                    p.root_rot = rx(0.28 * windup);
                    p.root_pivot = Vec3::new(0.0, 0.5, 0.45);
                    p.hips_offset += Vec3::new(0.0, -0.22, 0.12) * windup + Vec3::X * shiver;
                    p.chest_offset.y = 0.05 * windup;
                    for leg in QBone::LEGS {
                        if leg.front {
                            p.set(leg.upper, rx(0.55 * windup)).set(leg.lower, rx(-0.5 * windup)).set(leg.paw, rx(0.3 * windup));
                        } else {
                            p.set(leg.upper, rx(0.18 + 0.8 * windup)).set(leg.lower, rx(-0.3 - 1.3 * windup)).set(leg.paw, rx(0.12 + 0.5 * windup));
                        }
                    }
                    p.set(QBone::Neck, rx(0.25 * windup)).set(QBone::Head, rx(-0.1 * windup));
                    p.set(QBone::Jaw, rx(0.5 * ease((a.elapsed - d.startup * 0.5) / (d.startup * 0.5))));
                    p.set(QBone::Tail, rx(-0.4) * ry(shiver * 4.0));
                }
                ActionPhase::Active => {
                    // The leap: stretched out in the air, front paws reaching, jaws wide.
                    let t = (a.elapsed - d.startup) / d.active.max(1e-3);
                    p.root_offset.y = 0.55 * bell(t);
                    p.root_rot = rx(0.18 * (1.0 - 2.0 * t));
                    for leg in QBone::LEGS {
                        if leg.front {
                            p.set(leg.upper, rx(1.1)).set(leg.lower, rx(-0.2)).set(leg.paw, rx(0.4));
                        } else {
                            p.set(leg.upper, rx(-0.9)).set(leg.lower, rx(-0.2)).set(leg.paw, rx(0.6));
                        }
                    }
                    p.set(QBone::Neck, rx(-0.35)).set(QBone::Head, rx(0.1)).set(QBone::Jaw, rx(0.9 * (1.0 - t) + 0.3));
                    p.set(QBone::Tail, rx(-0.1));
                }
                _ => {
                    // Landed: head low, jaws clamped, shaking whatever it caught; then it lets go.
                    let r = (a.elapsed - d.startup - d.active) / d.recovery.max(1e-3);
                    let hold = 1.0 - ease((r - 0.7) / 0.3);
                    let shake = (a.elapsed * 22.0).sin() * 0.35 * bell((r / 0.7).min(1.0)) * hold;
                    p.set(QBone::Neck, rx(-0.5 * hold) * ry(shake)).set(QBone::Head, rx(0.25 * hold) * rz(shake * 0.6));
                    p.set(QBone::Jaw, rx(0.08 * hold));
                    p.hips_offset.z = 0.1 * hold;
                    brace(&mut p, hold);
                    p.set(QBone::Tail, rx(-0.5) * ry(shake * 0.5));
                }
            }
            Some(p)
        }
        ActionKind::Hurt => {
            let b = bell((a.elapsed / total.max(1e-3)).powf(0.6));
            p.set(QBone::Neck, rx(0.4 * b) * rz(0.2 * b)).set(QBone::Head, rx(0.2 * b));
            p.hips_offset += Vec3::new(0.0, -0.05, 0.06) * b;
            p.set(QBone::UpperFL, rx(-0.2 * b)).set(QBone::UpperFR, rx(-0.15 * b));
            Some(p)
        }
        ActionKind::Stunned => {
            // Parried: knocked back on its haunches, head shaking.
            let k = ease((a.elapsed / 0.15).min((total - a.elapsed) / 0.4).clamp(0.0, 1.0));
            let sway = (a.elapsed * 6.0).sin() * 0.15 * k;
            p.root_rot = rx(0.15 * k);
            p.root_pivot = Vec3::new(0.0, 0.4, 0.45);
            p.hips_offset.y = -0.12 * k;
            p.set(QBone::Neck, rx(0.35 * k) * ry(sway)).set(QBone::Head, rx(-0.2 * k) * rz(sway));
            p.set(QBone::UpperFL, rx(0.3 * k)).set(QBone::UpperFR, rx(0.35 * k));
            Some(p)
        }
        ActionKind::Dead => {
            // Legs give way, it rolls onto its side; legs stiffen.
            let fall = ease(a.elapsed / 0.8);
            p.root_rot = rz(1.45 * fall);
            p.root_pivot = Vec3::new(0.0, 0.05, 0.0);
            p.root_offset.y = -0.08 * fall;
            for leg in QBone::LEGS {
                let spread = if leg.front { 0.35 } else { -0.35 };
                p.set(leg.upper, rx(spread * fall)).set(leg.lower, rx(-0.15 * fall));
            }
            p.set(QBone::Neck, rx(-0.3 * fall)).set(QBone::Jaw, rx(0.25 * fall)).set(QBone::Tail, rx(-1.0 * fall));
            Some(p)
        }
        // Rolls, drinking, resting, being grabbed: not things a beast does.
        _ => None,
    }
}

// ── Animator (ECS) ───────────────────────────────────────────────────────

/// Per-character animation state. Put it next to a [`QuadrupedRig`] and a `CharacterController`.
#[derive(Component, Debug, Default)]
pub struct QuadrupedAnimator {
    phase: f32,
    speed: f32,
    time: f32,
    action_weight: f32,
    last_action: Option<QuadrupedPose>,
}

pub fn animate_quadrupeds(
    time: Res<Time>,
    mut characters: Query<(&QuadrupedSpec, &QuadrupedRig, &mut QuadrupedAnimator, &CharacterController, &CharacterAction)>,
    mut transforms: Query<&mut Transform>,
) {
    let dt = time.delta_secs();
    let smooth = |rate: f32| 1.0 - (-rate * dt).exp();
    for (spec, rig, mut anim, ctrl, action) in &mut characters {
        anim.time += dt;
        let actual = Vec2::new(ctrl.moved.x, ctrl.moved.z).length().min(12.0);
        anim.speed += (actual - anim.speed) * smooth(8.0);
        anim.phase = (anim.phase + anim.speed / stride_length(anim.speed) * TAU * dt) % TAU;
        let loco = locomotion_pose(&LocomotionInput { speed: anim.speed, phase: anim.phase, time: anim.time });
        // Action layer: snaps in fast (bites and their hitbox agree), fades out a bit slower.
        let pose = match action.current.as_ref().and_then(action_pose) {
            Some(p) => {
                anim.action_weight = (anim.action_weight + dt / 0.06).min(1.0);
                anim.last_action = Some(p);
                loco.blend(&p, anim.action_weight)
            }
            None => {
                anim.action_weight = (anim.action_weight - dt / 0.15).max(0.0);
                match anim.last_action {
                    Some(p) if anim.action_weight > 0.0 => loco.blend(&p, anim.action_weight),
                    _ => loco,
                }
            }
        };
        if let Ok(mut tf) = transforms.get_mut(rig.model) {
            *tf = pose.model_transform(rig.root_height);
        }
        for bone in QBone::ALL {
            if let Ok(mut tf) = transforms.get_mut(rig.bone(bone)) {
                *tf = pose.bone_local(spec, bone);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quadruped::pose::bone_character_transform;
    use crate::weapon::WeaponClass;

    fn at(kind: ActionKind, elapsed: f32) -> ActiveAction {
        ActiveAction { elapsed, ..ActiveAction::new(kind, Vec3::NEG_Z, WeaponClass::WargFangs) }
    }

    /// Snout tip relative to the feet, with the whole-body offsets and lean.
    fn snout(pose: &QuadrupedPose) -> Vec3 {
        let spec = QuadrupedSpec::warg();
        bone_character_transform(&spec, pose, 0.0, QBone::Head).transform_point(spec.snout_tip())
    }

    #[test]
    fn a_bite_thrusts_the_snout_forward() {
        let a = at(ActionKind::LightAttack(0), 0.0);
        let drawn = snout(&action_pose(&at(ActionKind::LightAttack(0), a.def.startup * 0.95)).unwrap());
        let bitten = snout(&action_pose(&at(ActionKind::LightAttack(0), a.def.startup + a.def.active)).unwrap());
        assert!(bitten.z < drawn.z - 0.2, "drawn {drawn} → bitten {bitten}");
    }

    #[test]
    fn the_pounce_leans_back_then_leaps() {
        let a = at(ActionKind::Pounce, 0.0);
        let rest = snout(&QuadrupedPose::default());
        let leaning = action_pose(&at(ActionKind::Pounce, a.def.startup * 0.95)).unwrap();
        assert!(leaning.hips_offset.y < -0.15 && leaning.hips_offset.z > 0.05, "crouched and shifted back");
        assert!(snout(&leaning).y > rest.y, "head up while leaning back: {} vs rest {}", snout(&leaning), rest);
        let mid_air = action_pose(&at(ActionKind::Pounce, a.def.startup + a.def.active * 0.5)).unwrap();
        assert!(mid_air.root_offset.y > 0.4, "in the air");
    }

    #[test]
    fn gaits_move_the_legs_and_stand_still_at_rest() {
        let standing = locomotion_pose(&LocomotionInput { speed: 0.0, phase: 1.0, time: 0.0 });
        let standing_later = locomotion_pose(&LocomotionInput { speed: 0.0, phase: 3.0, time: 0.0 });
        assert!(standing.get(QBone::UpperFL).angle_between(standing_later.get(QBone::UpperFL)) < 1e-3, "no stepping in place");
        for speed in [1.5, 4.5, 7.0] {
            let a = locomotion_pose(&LocomotionInput { speed, phase: 0.0, time: 0.0 });
            let b = locomotion_pose(&LocomotionInput { speed, phase: PI * 0.5, time: 0.0 });
            assert!(a.get(QBone::UpperFL).angle_between(b.get(QBone::UpperFL)) > 0.3, "{speed} m/s swings the legs");
        }
    }
}
