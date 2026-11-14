//! Shared humanoid animations, driven by any character's movement and actions.
//!
//! Layers: **locomotion** (idle / walk / run cycle, jump, fall, landing crouch) is always
//! computed; an **action layer** (roll, attacks, hurt, death, drinking…) is blended on top,
//! either full-body or upper-body only. Attack poses are full-body so the sword hitbox
//! (`crate::combat`, via [`action_pose`] + forward kinematics) matches what's on screen.

use std::f32::consts::{PI, TAU};

use bevy::prelude::*;

use super::pose::{HumanoidPose, PoseMask};
use super::{Bone, HumanoidRig, HumanoidSpec};
use crate::character::action::{ActionKind, ActionPhase, ActiveAction, CharacterAction};
use crate::character::controller::CharacterController;
use crate::tuning::*;
use crate::weapon::{EquippedWeapon, Stance, SwingKey, WeaponClass};

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

/// Everything the locomotion layer needs to know about the character this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocomotionInput {
    /// Horizontal speed (m/s).
    pub speed: f32,
    /// Gait cycle phase (radians), advanced by distance travelled.
    pub phase: f32,
    /// 0 = on the ground, 1 = fully airborne (smoothed).
    pub air: f32,
    /// Vertical velocity (m/s), for rising vs falling.
    pub vertical: f32,
    /// 0..1 landing crouch.
    pub landing: f32,
    /// Seconds, for idle breathing.
    pub time: f32,
    /// Held weapon class (its guard stance).
    pub weapon: WeaponClass,
}

/// Idle guard of a weapon class (its `GuardSpec`), knees soft.
pub fn guard_pose(weapon: WeaponClass) -> HumanoidPose {
    let g = weapon.moveset().guard;
    let (arm, elbow) = (rx(g.arm_pitch), rx(g.elbow));
    let mut p = HumanoidPose::default();
    p.set(Bone::UpperArmR, arm)
        .set(Bone::ForearmR, elbow)
        // The grip is given relative to the shoulder frame; undo the arm and elbow rotation.
        .set(Bone::HandR, (arm * elbow).inverse() * Quat::from_euler(EulerRot::XYZ, g.grip.x, g.grip.y, g.grip.z))
        .set(Bone::UpperArmL, rx(g.off_arm_pitch) * rz(-0.1))
        .set(Bone::ForearmL, rx(g.off_elbow))
        .set(Bone::ThighL, rx(0.06))
        .set(Bone::ShinL, rx(-0.12))
        .set(Bone::ThighR, rx(0.03))
        .set(Bone::ShinR, rx(-0.08));
    p
}

/// Walk / run cycle blended by speed, plus airborne and landing poses.
pub fn locomotion_pose(input: &LocomotionInput) -> HumanoidPose {
    let mut p = guard_pose(input.weapon);
    let walk = (input.speed / WALK_SPEED).clamp(0.0, 1.0);
    let run = ((input.speed - WALK_SPEED) / (SPRINT_SPEED - WALK_SPEED)).clamp(0.0, 1.0);
    let ph = input.phase;

    // Idle breathing (fades out while moving).
    let breathe = (input.time * 1.8).sin() * 0.025 * (1.0 - walk);
    p.add(Bone::Chest, rx(breathe));

    // Legs: thighs swing opposite each other; the knee bends while the leg swings forward.
    let hip_amp = (0.42 + 0.33 * run) * walk;
    let knee_amp = (0.55 + 0.75 * run) * walk;
    for (thigh, shin, foot, offset) in [(Bone::ThighL, Bone::ShinL, Bone::FootL, 0.0), (Bone::ThighR, Bone::ShinR, Bone::FootR, PI)] {
        let swing = (ph + offset).sin() * hip_amp;
        let bend = (ph + offset).cos().max(0.0) * knee_amp + 0.1 * walk;
        p.add(thigh, rx(swing)).add(shin, rx(-bend));
        // Keep the foot roughly level with the floor.
        p.set(foot, rx(-(swing - bend) * 0.4));
    }
    // Arms swing against the legs; the sword arm swings less and lifts the blade back when running.
    let arm_amp = (0.3 + 0.35 * run) * walk;
    p.add(Bone::UpperArmL, rx(-ph.sin() * arm_amp)).add(Bone::ForearmL, rx(0.25 * walk + 0.7 * run));
    p.add(Bone::UpperArmR, rx(ph.sin() * arm_amp * 0.4 - 0.25 * run)).add(Bone::ForearmR, rx(0.3 * run));
    // Torso: lean into the run, counter-twist the hips, bob down twice per cycle.
    p.add(Bone::Chest, rx(-0.06 * walk - 0.18 * run) * ry(ph.sin() * 0.12 * walk));
    p.add(Bone::Pelvis, ry(-ph.sin() * 0.08 * walk));
    p.pelvis_offset.y -= (2.0 * ph).cos().mul_add(0.5, 0.5) * (0.02 + 0.03 * run) * walk;

    // Airborne: rising tucks one knee up, falling spreads arms and reaches the legs down.
    if input.air > 0.0 {
        let mut air = guard_pose(input.weapon);
        let rising = (input.vertical / 4.0).clamp(-1.0, 1.0) * 0.5 + 0.5;
        air.set(Bone::ThighL, rx(0.3 + 0.5 * rising))
            .set(Bone::ShinL, rx(-0.4 - 0.7 * rising))
            .set(Bone::ThighR, rx(-0.15 + 0.3 * (1.0 - rising)))
            .set(Bone::ShinR, rx(-0.3 - 0.2 * rising))
            .add(Bone::UpperArmL, rz(-0.5 - 0.3 * (1.0 - rising)) * rx(0.3))
            .add(Bone::UpperArmR, rz(0.25) * rx(0.1))
            .add(Bone::Chest, rx(-0.08));
        p = p.blend(&air, input.air, PoseMask::Full);
    }

    // Landing: a quick crouch absorbing the fall.
    if input.landing > 0.0 {
        let k = input.landing;
        p.pelvis_offset.y -= 0.14 * k;
        p.add(Bone::ThighL, rx(0.45 * k)).add(Bone::ShinL, rx(-0.85 * k)).add(Bone::FootL, rx(0.4 * k));
        p.add(Bone::ThighR, rx(0.45 * k)).add(Bone::ShinR, rx(-0.85 * k)).add(Bone::FootR, rx(0.4 * k));
        p.add(Bone::Chest, rx(-0.2 * k));
    }
    p
}

// ── Actions ──────────────────────────────────────────────────────────────

/// A swing keyframe as rotations: chest twist/lean and the right arm (straight, blade extending it).
#[derive(Clone, Copy)]
struct Key {
    chest: Quat,
    shoulder: Quat,
}

fn key(k: &SwingKey) -> Key {
    Key { chest: ry(k.chest_yaw) * rx(k.chest_pitch), shoulder: ry(k.arm_yaw) * rx(k.arm_pitch) }
}

/// Blade continues the arm line during swings.
fn extended_grip() -> Quat {
    rx(PI)
}

/// (wind-up, strike end) of the action's attack, from the weapon class's moveset.
fn swing_keys(a: &ActiveAction) -> Option<(Key, Key)> {
    let spec = a.attack()?;
    Some((key(&spec.windup), key(&spec.strike)))
}

/// Time of the active phase over which the swing travels (the jump attack strikes quickly,
/// then holds the blade down while plunging).
fn strike_duration(a: &ActiveAction) -> f32 {
    match a.kind {
        ActionKind::JumpAttack => a.weapon.moveset().jump_strike_time,
        _ => a.def.active,
    }
}

/// Swing progress → (from, to, t) keys for the current phase.
fn swing_state(a: &ActiveAction, rest: Key) -> Option<(Key, Key, f32)> {
    let (windup, strike) = swing_keys(a)?;
    let d = &a.def;
    if a.kind == ActionKind::HeavyCharge {
        return Some((rest, windup, a.elapsed / 0.2));
    }
    // The heavy swing starts from the raised charge pose.
    let start = if a.kind == ActionKind::HeavyAttack { windup } else { rest };
    Some(match a.phase() {
        ActionPhase::Startup => (start, windup, a.elapsed / d.startup.max(1e-3)),
        ActionPhase::Active => (windup, strike, (a.elapsed - d.startup) / strike_duration(a)),
        ActionPhase::Recovery => (strike, rest, (a.elapsed - d.startup - d.active) / d.recovery),
        ActionPhase::Done => (rest, rest, 1.0),
    })
}

/// Pose for a running action, and which bones it controls. `None` = no action pose.
pub fn action_pose(a: &ActiveAction) -> Option<(HumanoidPose, PoseMask)> {
    let d = &a.def;
    let total = d.total();
    let p01 = a.elapsed / total.max(1e-3);
    let mut p = guard_pose(a.weapon);
    match a.kind {
        ActionKind::Roll => {
            // Tucked forward somersault around the body's middle, finished a bit before recovery ends.
            let t = a.elapsed / (ROLL_STARTUP + ROLL_ACTIVE + ROLL_RECOVERY * 0.5);
            let tuck = bell(t);
            // Curl into a ball (hips drop) and turn around the ball's centre so nothing dips
            // below the floor.
            p.root_rot = rx(-TAU * ease(t));
            p.root_pivot = 0.75;
            p.pelvis_offset.y = -0.42 * tuck;
            p.root_offset.y = -0.1 * tuck;
            for (thigh, shin) in [(Bone::ThighL, Bone::ShinL), (Bone::ThighR, Bone::ShinR)] {
                p.set(thigh, rx(1.7 * tuck)).set(shin, rx(-2.2 * tuck));
            }
            p.add(Bone::Chest, rx(-0.6 * tuck)).set(Bone::Head, rx(-0.4 * tuck));
            p.add(Bone::UpperArmL, rx(1.1 * tuck)).add(Bone::ForearmL, rx(1.2 * tuck));
            p.add(Bone::UpperArmR, rx(0.8 * tuck));
            Some((p, PoseMask::Full))
        }
        ActionKind::Backstep => {
            let b = bell(p01);
            p.root_rot = rx(0.3 * b);
            p.root_offset.y = 0.12 * b;
            p.set(Bone::ThighL, rx(0.35 * b)).set(Bone::ShinL, rx(-0.6 * b));
            p.set(Bone::ThighR, rx(-0.2 * b)).set(Bone::ShinR, rx(-0.4 * b));
            p.add(Bone::UpperArmL, rz(-0.4 * b));
            Some((p, PoseMask::Full))
        }
        ActionKind::LightAttack(_)
        | ActionKind::HeavyCharge
        | ActionKind::HeavyAttack
        | ActionKind::JumpAttack
        | ActionKind::RunningAttack => {
            let rest = Key { chest: Quat::IDENTITY, shoulder: p.get(Bone::UpperArmR) };
            let (from, to, t) = swing_state(a, rest)?;
            let t = ease(t);
            let swinging = !(matches!(a.phase(), ActionPhase::Recovery | ActionPhase::Done)) || a.kind == ActionKind::HeavyCharge;
            p.add(Bone::Chest, from.chest.slerp(to.chest, t));
            p.set(Bone::UpperArmR, from.shoulder.slerp(to.shoulder, t));
            // Straighten the arm and extend the blade during the swing, back to the guard in recovery.
            let guard = guard_pose(a.weapon);
            let (fore, grip) = if swinging {
                let k = if a.kind == ActionKind::HeavyAttack { 1.0 } else { ease(a.elapsed / 0.08) };
                (guard.get(Bone::ForearmR).slerp(Quat::IDENTITY, k), guard.get(Bone::HandR).slerp(extended_grip(), k))
            } else {
                (Quat::IDENTITY.slerp(guard.get(Bone::ForearmR), t), extended_grip().slerp(guard.get(Bone::HandR), t))
            };
            p.set(Bone::ForearmR, fore).set(Bone::HandR, grip);
            // Free arm balances the swing.
            p.set(Bone::UpperArmL, rz(-0.35) * rx(0.4)).set(Bone::ForearmL, rx(0.6));

            if a.kind == ActionKind::HeavyCharge {
                return Some((p, PoseMask::UpperBody)); // legs keep walking (slowly)
            }
            // Lower body: lunge stance (left foot forward), tucked legs in the air, split legs in a leap.
            let stance = match a.phase() {
                ActionPhase::Startup => ease(a.elapsed / d.startup.max(1e-3)),
                ActionPhase::Active => 1.0,
                _ => 1.0 - ease((a.elapsed - d.startup - d.active) / d.recovery),
            };
            match a.attack().map(|spec| spec.stance) {
                Some(Stance::Tucked) => {
                    p.set(Bone::ThighL, rx(0.7 * stance)).set(Bone::ShinL, rx(-1.1 * stance));
                    p.set(Bone::ThighR, rx(0.4 * stance)).set(Bone::ShinR, rx(-1.2 * stance));
                }
                Some(Stance::Leap) => {
                    p.set(Bone::ThighL, rx(0.85 * stance)).set(Bone::ShinL, rx(-0.9 * stance));
                    p.set(Bone::ThighR, rx(-0.55 * stance)).set(Bone::ShinR, rx(-0.7 * stance));
                }
                _ => {
                    p.set(Bone::ThighL, rx(0.45 * stance)).set(Bone::ShinL, rx(-0.4 * stance)).set(Bone::FootL, rx(-0.05 * stance));
                    p.set(Bone::ThighR, rx(-0.35 * stance)).set(Bone::ShinR, rx(-0.25 * stance)).set(Bone::FootR, rx(0.5 * stance));
                    p.pelvis_offset.y = -0.07 * stance;
                }
            }
            Some((p, PoseMask::Full))
        }
        ActionKind::DrinkAle => {
            // Left hand up to the mouth, head tilts back while swallowing.
            let up = match a.phase() {
                ActionPhase::Startup => ease(a.elapsed / d.startup),
                ActionPhase::Active => 1.0,
                _ => 1.0 - ease((a.elapsed - d.startup - d.active) / d.recovery),
            };
            p.set(Bone::UpperArmL, (rx(1.15) * rz(0.45)).slerp(Quat::IDENTITY, 1.0 - up))
                .set(Bone::ForearmL, rx(2.0 * up + 0.3 * (1.0 - up)))
                .set(Bone::Head, rx(0.35 * up * if a.phase() == ActionPhase::Active { 1.0 } else { 0.4 }));
            Some((p, PoseMask::UpperBody))
        }
        ActionKind::Hurt => {
            let b = bell(p01.powf(0.6));
            p.add(Bone::Chest, rx(0.4 * b)).set(Bone::Head, rx(0.3 * b));
            p.add(Bone::UpperArmL, rz(-0.6 * b)).add(Bone::UpperArmR, rz(0.4 * b));
            p.set(Bone::ShinL, rx(-0.4 * b)).set(Bone::ShinR, rx(-0.4 * b));
            p.pelvis_offset.y = -0.06 * b;
            Some((p, PoseMask::Full))
        }
        ActionKind::Dead => {
            // Knees buckle, then fall onto the back.
            let fall = ease(a.elapsed / 0.9);
            p.root_rot = rx(1.5 * fall);
            p.root_pivot = 0.05;
            p.root_offset.y = -0.05 * fall;
            p.set(Bone::ThighL, rx(0.6 * fall)).set(Bone::ShinL, rx(-0.9 * fall));
            p.set(Bone::ThighR, rx(0.3 * fall)).set(Bone::ShinR, rx(-0.5 * fall));
            p.set(Bone::UpperArmL, rz(-1.2 * fall)).set(Bone::UpperArmR, rz(1.1 * fall));
            p.set(Bone::Head, rx(0.3 * fall));
            Some((p, PoseMask::Full))
        }
    }
}

// ── Animator (ECS) ───────────────────────────────────────────────────────

/// Per-character animation state. Put it next to a [`HumanoidRig`] and a `CharacterController`.
#[derive(Component, Debug, Default)]
pub struct HumanoidAnimator {
    phase: f32,
    air: f32,
    landing: f32,
    was_grounded: bool,
    speed: f32,
    time: f32,
    /// Blend weight of the action layer and the last action pose (for fading out).
    action_weight: f32,
    last_action: Option<(HumanoidPose, PoseMask)>,
}

/// Distance covered by one full gait cycle (two steps), walking → running.
const STRIDE_WALK: f32 = 1.5;
const STRIDE_RUN: f32 = 2.4;

pub fn animate_humanoids(
    time: Res<Time>,
    mut characters: Query<(
        &HumanoidSpec,
        &HumanoidRig,
        &mut HumanoidAnimator,
        &CharacterController,
        &CharacterAction,
        Option<&EquippedWeapon>,
    )>,
    mut transforms: Query<&mut Transform>,
) {
    let dt = time.delta_secs();
    for (spec, rig, mut anim, ctrl, action, equipped) in &mut characters {
        anim.time += dt;
        let speed = Vec2::new(ctrl.velocity.x, ctrl.velocity.z).length();
        // Smooth speed so the gait blends instead of popping.
        anim.speed += (speed - anim.speed) * (1.0 - (-10.0 * dt).exp());
        let run = ((anim.speed - WALK_SPEED) / (SPRINT_SPEED - WALK_SPEED)).clamp(0.0, 1.0);
        let stride = STRIDE_WALK + (STRIDE_RUN - STRIDE_WALK) * run;
        if ctrl.grounded {
            anim.phase = (anim.phase + anim.speed / stride * TAU * dt) % TAU;
        }
        let air_target = if ctrl.grounded { 0.0 } else { 1.0 };
        anim.air += (air_target - anim.air) * (1.0 - (-14.0 * dt).exp());
        if ctrl.grounded && !anim.was_grounded {
            anim.landing = 1.0;
        }
        anim.was_grounded = ctrl.grounded;
        anim.landing = (anim.landing - dt / 0.22).max(0.0);

        let loco = locomotion_pose(&LocomotionInput {
            speed: anim.speed,
            phase: anim.phase,
            air: anim.air,
            vertical: ctrl.velocity.y,
            landing: bell(1.0 - anim.landing) * anim.landing.min(1.0).sqrt(),
            time: anim.time,
            weapon: equipped.map_or_else(WeaponClass::default, |e| e.class()),
        });

        // Action layer: snaps in fast (so swings and hitboxes agree), fades out a bit slower.
        let current = action.current.as_ref().and_then(action_pose);
        let pose = match current {
            Some((p, mask)) => {
                anim.action_weight = (anim.action_weight + dt / 0.06).min(1.0);
                anim.last_action = Some((p, mask));
                loco.blend(&p, anim.action_weight, mask)
            }
            None => {
                anim.action_weight = (anim.action_weight - dt / 0.12).max(0.0);
                match anim.last_action {
                    Some((p, mask)) if anim.action_weight > 0.0 => loco.blend(&p, anim.action_weight, mask),
                    _ => loco,
                }
            }
        };

        if let Ok(mut tf) = transforms.get_mut(rig.model) {
            *tf = pose.model_transform(rig.root_height);
        }
        for bone in Bone::ALL {
            if let Ok(mut tf) = transforms.get_mut(rig.bone(bone)) {
                *tf = pose.bone_local(spec, bone);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::humanoid::pose::bone_model_transform;

    fn at(kind: ActionKind, elapsed: f32) -> ActiveAction {
        at_with(WeaponClass::LightSword, kind, elapsed)
    }

    fn at_with(class: WeaponClass, kind: ActionKind, elapsed: f32) -> ActiveAction {
        ActiveAction { elapsed, ..ActiveAction::new(kind, Vec3::NEG_Z, class) }
    }

    /// Tip of a 1.03 m blade (along the hand's +Y) in model space.
    fn tip_with(class: WeaponClass, kind: ActionKind, elapsed: f32) -> Vec3 {
        let spec = HumanoidSpec::human();
        let (pose, _) = action_pose(&at_with(class, kind, elapsed)).unwrap();
        bone_model_transform(&spec, &pose, Bone::HandR).transform_point(Vec3::Y * 1.03)
    }

    fn tip(kind: ActionKind, elapsed: f32) -> Vec3 {
        tip_with(WeaponClass::LightSword, kind, elapsed)
    }

    #[test]
    fn guard_holds_the_sword_forward_and_down() {
        let spec = HumanoidSpec::human();
        let hand = bone_model_transform(&spec, &guard_pose(WeaponClass::LightSword), Bone::HandR);
        let tip = hand.transform_point(Vec3::Y * 1.03);
        assert!(tip.z < -0.5 && tip.y < hand.translation.y, "{tip}");
    }

    #[test]
    fn light_slash_sweeps_right_to_left_in_front() {
        let d = at(ActionKind::LightAttack(0), 0.0).def;
        let a = tip(ActionKind::LightAttack(0), d.startup);
        let m = tip(ActionKind::LightAttack(0), d.startup + d.active * 0.5);
        let b = tip(ActionKind::LightAttack(0), d.startup + d.active);
        assert!(a.x > 0.8, "starts on the right: {a}");
        assert!(b.x < -0.5, "ends on the left: {b}");
        assert!(m.z < -1.2, "passes in front: {m}");
    }

    #[test]
    fn overheads_come_down_in_front() {
        for kind in [ActionKind::LightAttack(2), ActionKind::HeavyAttack, ActionKind::RunningAttack] {
            let d = at(kind, 0.0).def;
            let high = tip(kind, d.startup);
            let low = tip(kind, d.startup + d.active);
            assert!(high.y > 2.0, "{kind:?} raised: {high}");
            assert!(low.y < 1.1 && low.z < -0.8, "{kind:?} lands in front: {low}");
        }
    }

    /// Every class's strikes must end in front of the character, or they can't hit anything.
    #[test]
    fn every_class_strikes_in_front() {
        for class in WeaponClass::ALL {
            let n = class.moveset().light.len() as u8;
            let kinds = (0..n).map(ActionKind::LightAttack).chain([ActionKind::HeavyAttack, ActionKind::RunningAttack]);
            for kind in kinds {
                let d = at_with(class, kind, 0.0).def;
                // Sample the active window; some point of the swing must be well in front.
                let best_z = (0..=10)
                    .map(|i| tip_with(class, kind, d.startup + d.active * i as f32 / 10.0).z)
                    .fold(f32::INFINITY, f32::min);
                assert!(best_z < -0.9, "{class:?} {kind:?} never reaches forward (min z {best_z})");
            }
        }
    }

    #[test]
    fn walking_moves_the_feet_apart() {
        let spec = HumanoidSpec::human();
        let pose = locomotion_pose(&LocomotionInput { speed: WALK_SPEED, phase: PI / 2.0, ..default() });
        let l = bone_model_transform(&spec, &pose, Bone::FootL).translation;
        let r = bone_model_transform(&spec, &pose, Bone::FootR).translation;
        assert!(l.z < -0.2 && r.z > 0.2, "left forward, right back: {l} {r}");
    }

    #[test]
    fn roll_stays_above_the_floor() {
        let spec = HumanoidSpec::human();
        let d = at(ActionKind::Roll, 0.0).def;
        let mut t = 0.0;
        while t < d.total() {
            let (pose, _) = action_pose(&at(ActionKind::Roll, t)).unwrap();
            let model = pose.model_transform(0.0);
            for bone in Bone::ALL {
                let y = (model * bone_model_transform(&spec, &pose, bone)).translation.y;
                assert!(y > -0.1, "{bone:?} at {y:.2} m, {t:.2}s into the roll");
            }
            t += 0.02;
        }
    }

    #[test]
    fn every_action_has_a_pose() {
        for kind in [
            ActionKind::Roll,
            ActionKind::Backstep,
            ActionKind::LightAttack(0),
            ActionKind::RunningAttack,
            ActionKind::HeavyCharge,
            ActionKind::HeavyAttack,
            ActionKind::JumpAttack,
            ActionKind::DrinkAle,
            ActionKind::Hurt,
            ActionKind::Dead,
        ] {
            assert!(action_pose(&at(kind, 0.1)).is_some(), "{kind:?}");
        }
    }
}

