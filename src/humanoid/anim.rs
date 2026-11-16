//! Shared humanoid animations, driven by any character's movement and actions.
//!
//! Layers: **locomotion** (idle / walk / run cycle, jump, fall, landing crouch) is always
//! computed; an **action layer** (roll, attacks, hurt, death, drinking…) is blended on top,
//! either full-body or upper-body only. Attack poses are full-body so the sword hitbox
//! (`crate::combat`, via [`action_pose`] + forward kinematics) matches what's on screen.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::prelude::*;

use super::pose::{HumanoidPose, PoseMask, bone_model_transform, reach_left_hand};
use super::{Bone, HumanoidRig, HumanoidSpec};
use crate::character::action::{ActionKind, ActionPhase, ActiveAction, CharacterAction};
use crate::character::controller::CharacterController;
use crate::character::guard::Guard;
use crate::tuning::*;
use crate::weapon::{AttackHand, Stance, SwingKey, WeaponClass, Wielded};

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
    /// Gait speed (m/s): how fast the legs animate. Usually the actual horizontal speed;
    /// at least a slow walk while pushing against a wall.
    pub speed: f32,
    /// Gait cycle phase (radians), advanced by distance travelled. 0 = left heel strikes.
    pub phase: f32,
    /// 0 = on the ground, 1 = fully airborne (smoothed).
    pub air: f32,
    /// Vertical velocity (m/s), for rising vs falling.
    pub vertical: f32,
    /// 0..1 landing crouch.
    pub landing: f32,
    /// 0..1 pressing against an obstacle: short braced steps, free hand on the wall.
    pub blocked: f32,
    /// Seconds, for idle breathing.
    pub time: f32,
    /// Held weapon class (its guard stance).
    pub weapon: WeaponClass,
    /// Direction of travel relative to facing (radians, + = to the left; 0 = forward, ±π =
    /// backward). Strafing (locked on) turns the hips toward it; walking backward plays the cycle
    /// in reverse. Smoothed by the animator.
    pub heading: f32,
}

/// Hips turned toward the direction of travel, and whether the cycle runs backward: a sideways or
/// backward walk with the upper body still facing forward.
pub fn strafe(heading: f32) -> (f32, bool) {
    let h = (heading + PI).rem_euclid(TAU) - PI;
    if h.abs() <= FRAC_PI_2 + 0.15 {
        (h.clamp(-MAX_HIP_TWIST, MAX_HIP_TWIST), false)
    } else {
        // Backward: face the hips the opposite way and step backward.
        let back = if h > 0.0 { h - PI } else { h + PI };
        (back.clamp(-MAX_HIP_TWIST, MAX_HIP_TWIST), true)
    }
}

/// Direction of `moved` (world) relative to `facing`: 0 = forward, + = to the character's left.
pub fn travel_heading(facing: f32, moved: Vec3) -> f32 {
    let forward = Vec2::new(-facing.sin(), -facing.cos());
    let left = Vec2::new(-facing.cos(), facing.sin());
    let v = Vec2::new(moved.x, moved.z);
    v.dot(left).atan2(v.dot(forward))
}

/// Most the hips turn away from the facing while strafing (rad).
const MAX_HIP_TWIST: f32 = 1.35;

/// Idle guard of a weapon class (its `GuardSpec`), knees soft.
pub fn guard_pose(weapon: WeaponClass) -> HumanoidPose {
    let g = weapon.moveset().guard;
    let mut p = HumanoidPose::default();
    set_sword_arm(&mut p, g.arm_pitch, g.elbow, g.grip);
    p.set(Bone::UpperArmL, rx(g.off_arm_pitch) * rz(-0.1))
        .set(Bone::ForearmL, rx(g.off_elbow))
        .set(Bone::ThighL, rx(0.06))
        .set(Bone::ShinL, rx(-0.12))
        .set(Bone::FootL, rx(0.06))
        .set(Bone::ThighR, rx(0.03))
        .set(Bone::ShinR, rx(-0.08))
        .set(Bone::FootR, rx(0.05));
    p
}

/// Right arm holding the weapon: `grip` is given relative to the shoulder frame, so the arm
/// and elbow rotation are undone at the hand.
fn set_sword_arm(p: &mut HumanoidPose, arm_pitch: f32, elbow: f32, grip: Vec3) {
    let (arm, elbow) = (rx(arm_pitch), rx(elbow));
    p.set(Bone::UpperArmR, arm)
        .set(Bone::ForearmR, elbow)
        .set(Bone::HandR, (arm * elbow).inverse() * Quat::from_euler(EulerRot::XYZ, grip.x, grip.y, grip.z));
}

/// Shape of one gait. Angles in radians; slow walk, jog and sprint are blended by speed.
#[derive(Clone, Copy, Debug)]
struct Gait {
    /// Fraction of the cycle a foot is on the ground (> 0.5 walking, < 0.5 = flight phases).
    duty: f32,
    /// Thigh angle at heel strike (forward) and at toe-off (back).
    hip_fwd: f32,
    hip_back: f32,
    /// Knee bend absorbing the step (mid-stance) and lifting the foot (swing).
    knee_stance: f32,
    knee_swing: f32,
    /// Toe push at the end of stance; heel-first contact is 40 % of it.
    toe_off: f32,
    /// Forward lean of the pelvis and chest.
    lean: f32,
    /// Free arm swing and elbow bend.
    arm_swing: f32,
    elbow: f32,
    /// Hip/shoulder counter-rotation and side-to-side pelvis roll.
    twist: f32,
    sway: f32,
    /// Extra lift in the flight phases.
    bounce: f32,
    /// Distance covered by one full cycle (two steps).
    stride: f32,
}

impl Gait {
    fn lerp(&self, o: &Gait, t: f32) -> Gait {
        let l = |a: f32, b: f32| a + (b - a) * t;
        Gait {
            duty: l(self.duty, o.duty),
            hip_fwd: l(self.hip_fwd, o.hip_fwd),
            hip_back: l(self.hip_back, o.hip_back),
            knee_stance: l(self.knee_stance, o.knee_stance),
            knee_swing: l(self.knee_swing, o.knee_swing),
            toe_off: l(self.toe_off, o.toe_off),
            lean: l(self.lean, o.lean),
            arm_swing: l(self.arm_swing, o.arm_swing),
            elbow: l(self.elbow, o.elbow),
            twist: l(self.twist, o.twist),
            sway: l(self.sway, o.sway),
            bounce: l(self.bounce, o.bounce),
            stride: l(self.stride, o.stride),
        }
    }
}

/// Careful, short steps (pushing against a wall, charging a heavy, drinking on the move).
const SLOW_WALK: Gait = Gait {
    duty: 0.66,
    hip_fwd: 0.26,
    hip_back: 0.2,
    knee_stance: 0.08,
    knee_swing: 0.7,
    toe_off: 0.2,
    lean: 0.04,
    arm_swing: 0.12,
    elbow: 0.2,
    twist: 0.05,
    sway: 0.045,
    bounce: 0.0,
    stride: 1.0,
};
/// Default movement (WALK_SPEED): a light, guarded jog.
const JOG: Gait = Gait {
    duty: 0.5,
    hip_fwd: 0.5,
    hip_back: 0.36,
    knee_stance: 0.28,
    knee_swing: 1.25,
    toe_off: 0.4,
    lean: 0.1,
    arm_swing: 0.32,
    elbow: 0.55,
    twist: 0.11,
    sway: 0.04,
    bounce: 0.02,
    stride: 1.9,
};
/// Sprint (SPRINT_SPEED): leaning in, high knees, arms pumping, weapon trailing.
const SPRINT: Gait = Gait {
    duty: 0.36,
    hip_fwd: 0.9,
    hip_back: 0.5,
    knee_stance: 0.45,
    knee_swing: 2.0,
    toe_off: 0.55,
    lean: 0.26,
    arm_swing: 0.7,
    elbow: 1.4,
    twist: 0.15,
    sway: 0.025,
    bounce: 0.05,
    stride: 2.7,
};
/// Below this speed the gait is a slow walk.
const SLOW_WALK_SPEED: f32 = 1.4;

/// The gait for a speed, and how much the character is moving at all (0 = idle).
fn gait_for(speed: f32) -> (Gait, f32) {
    let moving = ease(speed / SLOW_WALK_SPEED);
    let to_jog = ((speed - SLOW_WALK_SPEED) / (WALK_SPEED - SLOW_WALK_SPEED)).clamp(0.0, 1.0);
    let to_sprint = ((speed - WALK_SPEED) / (SPRINT_SPEED - WALK_SPEED)).clamp(0.0, 1.0);
    (SLOW_WALK.lerp(&JOG, to_jog).lerp(&SPRINT, to_sprint), moving)
}

/// Distance of one full gait cycle at this speed (the animator advances the phase by it).
pub fn stride_length(speed: f32) -> f32 {
    gait_for(speed).0.stride
}

/// One leg at cycle position `u` (0..1, 0 = heel strike): (thigh, knee bend, foot angle
/// relative to a level foot; + = toes up).
fn leg_cycle(u: f32, g: &Gait) -> (f32, f32, f32) {
    let u = u.rem_euclid(1.0);
    if u < g.duty {
        // Stance: the planted foot travels back at constant speed; the knee gives a little;
        // the heel lands first and the toes push off at the end.
        let s = u / g.duty;
        let thigh = g.hip_fwd - (g.hip_fwd + g.hip_back) * s;
        let knee = 0.06 + g.knee_stance * bell(s.powf(0.8));
        let heel = g.toe_off * 0.4 * (1.0 - s / 0.2).max(0.0);
        let push = -g.toe_off * ((s - 0.65) / 0.35).max(0.0).powi(2);
        (thigh, knee, heel + push)
    } else {
        // Swing: the knee folds early to clear the floor, then reaches forward.
        let s = (u - g.duty) / (1.0 - g.duty);
        let thigh = -g.hip_back + (g.hip_fwd + g.hip_back) * ease(s.powf(0.85));
        let knee = 0.06 + g.knee_swing * bell(s.powf(0.7));
        let foot = -g.toe_off * (1.0 - s / 0.4).max(0.0) + g.toe_off * 0.4 * ease(s);
        (thigh, knee, foot)
    }
}

/// Idle / slow walk / jog / sprint cycle, braced steps against walls, airborne and landing
/// poses, with the lowest point of the soles kept on the floor.
pub fn locomotion_pose(spec: &HumanoidSpec, input: &LocomotionInput) -> HumanoidPose {
    let guard = guard_pose(input.weapon);
    let mut p = guard;
    let (g, moving) = gait_for(input.speed);
    let sprint = ((input.speed - WALK_SPEED) / (SPRINT_SPEED - WALK_SPEED)).clamp(0.0, 1.0);
    let (twist, backward) = strafe(input.heading);
    let twist = twist * moving;
    let u = if backward { 1.0 - input.phase / TAU } else { input.phase / TAU };

    // Idle breathing (fades out while moving).
    p.add(Bone::Chest, rx((input.time * 1.8).sin() * 0.025 * (1.0 - moving)));

    // Legs. Foot angle = knee − thigh keeps the foot level (bones chain pelvis → thigh →
    // shin → foot, all pitching about X), plus heel strike / toe-off.
    let lean = g.lean * moving;
    let mut thighs = [0.0; 2];
    for (i, (thigh, shin, foot, offset)) in
        [(Bone::ThighL, Bone::ShinL, Bone::FootL, 0.0), (Bone::ThighR, Bone::ShinR, Bone::FootR, 0.5)].into_iter().enumerate()
    {
        let (t, k, f) = leg_cycle(u + offset, &g);
        thighs[i] = t;
        let rest = (guard.get(thigh), guard.get(shin), guard.get(foot));
        // The pelvis leans forward by 40 % of the lean; thighs compensate so feet stay under.
        let lean_comp = lean * 0.4;
        p.set(thigh, rest.0.slerp(rx(t + lean_comp), moving))
            .set(shin, rest.1.slerp(rx(-k), moving))
            .set(foot, rest.2.slerp(rx(k - t + f), moving));
    }
    // −1 = right leg forward … +1 = left leg forward.
    let span = (g.hip_fwd + g.hip_back).max(1e-3);
    let side = ((thighs[0] - thighs[1]) / span).clamp(-1.0, 1.0) * moving;

    // Pelvis: lean, counter-rotate with the legs, roll toward the standing leg.
    p.add(Bone::Pelvis, rx(-lean * 0.4) * ry(-side * g.twist * 0.7) * rz(side * g.sway));
    // Chest: more lean, twist against the hips; head stays level.
    p.add(Bone::Chest, rx(-lean * 0.6) * ry(side * g.twist) * rz(-side * g.sway * 0.6));
    p.set(Bone::Head, rx(lean * 0.8));
    // Strafing: the hips (and legs) turn toward the travel, the chest turns back to the front.
    if twist.abs() > 1e-3 {
        p.add(Bone::Pelvis, ry(twist));
        p.add(Bone::Chest, ry(-twist));
    }

    // Free arm swings with the opposite leg; elbow bends more the faster you go.
    p.add(Bone::UpperArmL, rx(-side * g.arm_swing) * rz(-0.08 * sprint))
        .set(Bone::ForearmL, guard.get(Bone::ForearmL).slerp(rx(g.elbow), moving));
    // Sword arm: a small swing in the guard; sprinting trails the weapon low behind.
    let gs = input.weapon.moveset().guard;
    let mut trail = HumanoidPose::default();
    set_sword_arm(&mut trail, gs.run_arm_pitch + side * g.arm_swing * 0.5, gs.run_elbow, gs.run_grip);
    p.add(Bone::UpperArmR, rx(side * g.arm_swing * 0.25 * moving));
    for bone in [Bone::UpperArmR, Bone::ForearmR, Bone::HandR] {
        p.set(bone, p.get(bone).slerp(trail.get(bone), ease(sprint)));
    }

    // Flight phases (both feet up) lift the body a little: running looks springy.
    // Per step (half cycle) a foot leaves at 2·duty and the other lands at 1.
    let step = (u * 2.0).rem_euclid(1.0);
    let flight_start = 2.0 * g.duty;
    let flight = if flight_start < 1.0 && step > flight_start { bell((step - flight_start) / (1.0 - flight_start)) } else { 0.0 };
    let bounce = g.bounce * flight * moving;

    // Pushing against a wall: hunched in, free hand braced forward against it.
    if input.blocked > 0.0 {
        let b = ease(input.blocked);
        p.add(Bone::Chest, rx(-0.15 * b)).add(Bone::Pelvis, rx(-0.05 * b));
        let (arm, fore) = (p.get(Bone::UpperArmL), p.get(Bone::ForearmL));
        p.set(Bone::UpperArmL, arm.slerp(rx(1.35) * rz(0.15), b))
            .set(Bone::ForearmL, fore.slerp(rx(0.35), b))
            .set(Bone::HandL, rx(-0.9 * b))
            .add(Bone::Head, rx(0.12 * b));
    }

    // Plant the feet: drop (or raise) the body so the lowest heel/toe touches the floor.
    // Applied before the air pose blends in, so jumps aren't pulled down.
    let grounded_feet = |p: &HumanoidPose| {
        [Bone::FootL, Bone::FootR]
            .into_iter()
            .flat_map(|foot| {
                let t = bone_model_transform(spec, p, foot);
                spec.sole_points().map(|q| t.transform_point(q).y)
            })
            .fold(f32::INFINITY, f32::min)
    };

    // Landing: knees absorb the fall (planting lowers the body to match).
    if input.landing > 0.0 {
        let k = input.landing;
        for (thigh, shin, foot) in [(Bone::ThighL, Bone::ShinL, Bone::FootL), (Bone::ThighR, Bone::ShinR, Bone::FootR)] {
            p.add(thigh, rx(0.5 * k)).add(shin, rx(-0.95 * k)).add(foot, rx(0.45 * k));
        }
        p.add(Bone::Chest, rx(-0.22 * k));
    }
    p.pelvis_offset.y -= grounded_feet(&p);
    p.pelvis_offset.y += bounce;

    // Airborne: rising tucks one knee up, falling spreads arms and reaches the legs down.
    if input.air > 0.0 {
        let mut air = guard;
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
///
/// Attacks swung with the left hand are the mirror image of the right-handed swing; a dual attack
/// swings both arms at once (the right swing and its mirror). So every weapon class gets left and
/// dual animations from its right-hand swing keys.
pub fn action_pose(a: &ActiveAction) -> Option<(HumanoidPose, PoseMask)> {
    let (pose, mask) = right_handed_pose(a)?;
    Some(match a.hand {
        AttackHand::Right => (pose, mask),
        AttackHand::Left => (pose.mirrored(), mask),
        AttackHand::Both => (dual(&pose), mask),
    })
}

/// Both arms swing: the right arm as posed, the left as its mirror; the chest keeps only what
/// both sides agree on (the lean, not the twist).
fn dual(p: &HumanoidPose) -> HumanoidPose {
    let m = p.mirrored();
    let mut out = *p;
    for bone in [Bone::UpperArmL, Bone::ForearmL, Bone::HandL] {
        out.set(bone, m.get(bone));
    }
    out.set(Bone::Chest, p.get(Bone::Chest).slerp(m.get(Bone::Chest), 0.5));
    out
}

fn right_handed_pose(a: &ActiveAction) -> Option<(HumanoidPose, PoseMask)> {
    let d = &a.def;
    let total = d.total();
    let p01 = a.elapsed / total.max(1e-3);
    let mut p = guard_pose(a.weapon);
    match a.kind {
        ActionKind::Roll => {
            // Tucked forward somersault around the body's middle, finished a bit before recovery ends.
            let t = a.elapsed / (d.startup + d.active + ROLL_RECOVERY * 0.5);
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
        ActionKind::Ability(id) if id.strike().is_none() => {
            // A war cry: chest thrown back, both arms flung out and up, head back — then settle.
            let k = match a.phase() {
                ActionPhase::Startup => ease(a.elapsed / d.startup.max(1e-3)),
                ActionPhase::Active => 1.0,
                _ => 1.0 - ease((a.elapsed - d.startup - d.active) / d.recovery.max(1e-3)),
            };
            let shake = if a.phase() == ActionPhase::Active { (a.elapsed * 30.0).sin() * 0.04 } else { 0.0 };
            p.add(Bone::Chest, rx(0.3 * k + shake)).set(Bone::Head, rx(-0.45 * k));
            p.set(Bone::UpperArmL, Quat::IDENTITY.slerp(rz(-2.5) * rx(0.35), k)).set(Bone::ForearmL, rx(0.35 * k));
            p.set(Bone::UpperArmR, p.get(Bone::UpperArmR).slerp(rz(2.5) * rx(0.35), k)).set(Bone::ForearmR, rx(0.35 * k));
            p.set(Bone::ThighL, rx(0.3 * k)).set(Bone::ShinL, rx(-0.35 * k));
            p.set(Bone::ThighR, rx(-0.2 * k)).set(Bone::ShinR, rx(-0.3 * k));
            p.pelvis_offset.y = -0.08 * k;
            Some((p, PoseMask::Full))
        }
        ActionKind::LightAttack(_)
        | ActionKind::HeavyCharge
        | ActionKind::HeavyAttack
        | ActionKind::JumpAttack
        | ActionKind::RunningAttack
        | ActionKind::Ability(_) => {
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
        ActionKind::DrinkAle | ActionKind::UseItem => {
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
        ActionKind::Rest | ActionKind::StandUp => {
            // Down on the right knee, left foot planted forward, hands held to the flower.
            let k = match (a.kind, a.phase()) {
                (ActionKind::StandUp, _) => 1.0 - ease(a.elapsed / d.recovery.max(1e-3)),
                (_, ActionPhase::Startup) => ease(a.elapsed / d.startup),
                _ => 1.0,
            };
            p.pelvis_offset.y = -0.42 * k;
            p.set(Bone::ThighR, rx(-0.08 * k)).set(Bone::ShinR, rx(-1.5 * k)).set(Bone::FootR, rx(-0.35 * k));
            p.set(Bone::ThighL, rx(1.55 * k)).set(Bone::ShinL, rx(-1.55 * k));
            p.add(Bone::Chest, rx(-0.18 * k)).set(Bone::Head, rx(0.12 * k));
            p.set(Bone::UpperArmL, Quat::IDENTITY.slerp(rx(0.85) * rz(0.2), k)).set(Bone::ForearmL, rx(0.45 * k));
            // Sword arm drops, the blade resting point-down beside the knee.
            let g = guard_pose(a.weapon);
            p.set(Bone::UpperArmR, g.get(Bone::UpperArmR).slerp(rx(0.15) * rz(0.12), k));
            Some((p, PoseMask::Full))
        }
        ActionKind::Hurt => {
            let b = bell(p01.powf(0.6));
            p.add(Bone::Chest, rx(0.4 * b)).set(Bone::Head, rx(0.3 * b));
            p.add(Bone::UpperArmL, rz(-0.6 * b)).add(Bone::UpperArmR, rz(0.4 * b));
            p.set(Bone::ShinL, rx(-0.4 * b)).set(Bone::ShinR, rx(-0.4 * b));
            p.pelvis_offset.y = -0.06 * b;
            Some((p, PoseMask::Full))
        }
        ActionKind::Stunned => {
            // Parried: thrown off balance, weapon arm flung out, reeling back on bent knees.
            let k = (a.elapsed / 0.15).min(1.0).min((total - a.elapsed) / 0.4).clamp(0.0, 1.0);
            let k = ease(k);
            let sway = (a.elapsed * 5.0).sin() * 0.08 * k;
            p.add(Bone::Chest, rx(-0.35 * k) * rz(sway)).set(Bone::Head, rx(-0.35 * k));
            p.set(Bone::UpperArmR, Quat::IDENTITY.slerp(rz(1.15) * rx(0.5), k)).set(Bone::ForearmR, rx(0.3 * k));
            p.set(Bone::UpperArmL, Quat::IDENTITY.slerp(rz(-0.9) * rx(0.3), k)).set(Bone::ForearmL, rx(0.4 * k));
            p.set(Bone::ThighL, rx(0.45 * k)).set(Bone::ShinL, rx(-0.7 * k));
            p.set(Bone::ThighR, rx(-0.15 * k)).set(Bone::ShinR, rx(-0.45 * k));
            p.pelvis_offset.y = -0.12 * k;
            p.pelvis_offset.z = 0.08 * k;
            Some((p, PoseMask::Full))
        }
        ActionKind::Grabbed => {
            // Caught in a beast's jaws: bent back over it, arms flailing; slack after the bite.
            let k = ease(a.elapsed / 0.12).min(ease((total - a.elapsed) / 0.3));
            let bitten = ease((a.elapsed - GRAB_BITE_AT) / 0.15);
            let thrash = (a.elapsed * 15.0).sin() * (1.0 - 0.7 * bitten);
            p.add(Bone::Chest, rx(-0.45 * k) * rz(0.15 * thrash * k)).set(Bone::Head, rx(-0.5 * k + 0.3 * bitten));
            p.set(Bone::UpperArmL, Quat::IDENTITY.slerp(rz(-1.3 + 0.35 * thrash) * rx(0.6), k)).set(Bone::ForearmL, rx(0.9 * k));
            p.set(Bone::UpperArmR, Quat::IDENTITY.slerp(rz(1.2 - 0.35 * thrash) * rx(0.4), k)).set(Bone::ForearmR, rx(0.7 * k));
            p.set(Bone::ThighL, rx(0.3 * k)).set(Bone::ShinL, rx(-0.6 * k));
            p.set(Bone::ThighR, rx(-0.1 * k)).set(Bone::ShinR, rx(-0.5 * k));
            p.pelvis_offset.y = -0.1 * k;
            Some((p, PoseMask::Full))
        }
        ActionKind::OpenDoor => {
            // The free (left) palm against the door and the left shoulder leaning in, the right foot
            // pushing off; the weapon arm keeps its guard. Then back.
            let reach = match a.phase() {
                ActionPhase::Startup => ease(a.elapsed / d.startup),
                ActionPhase::Active => 1.0,
                _ => 1.0 - ease((a.elapsed - d.startup - d.active) / d.recovery),
            };
            let push = if a.phase() == ActionPhase::Startup { 0.0 } else { reach };
            p.set(Bone::UpperArmL, Quat::IDENTITY.slerp(rx(1.4 + 0.15 * push) * rz(0.25), reach)).set(Bone::ForearmL, rx(0.75 * reach - 0.5 * push));
            p.add(Bone::Chest, ry(-0.35 * reach) * rx(-0.2 * reach - 0.15 * push));
            p.set(Bone::ThighL, rx(0.35 * reach)).set(Bone::ShinL, rx(-0.3 * reach));
            p.set(Bone::ThighR, rx(-0.35 * push)).set(Bone::ShinR, rx(-0.2 * push)).set(Bone::FootR, rx(0.3 * push));
            Some((p, PoseMask::Full))
        }
        // Creatures only (`crate::quadruped`).
        ActionKind::Pounce => None,
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
    /// Seconds since the controller last reported ground (hysteresis for the air pose).
    air_time: f32,
    landing: f32,
    speed: f32,
    blocked: f32,
    time: f32,
    /// Blend weight of the action layer and the last action pose (for fading out).
    action_weight: f32,
    last_action: Option<(HumanoidPose, PoseMask)>,
    /// Blend weight of the raised-shield arm (follows `Guard::raise_progress`).
    guard: f32,
    /// Smoothed travel direction relative to facing (see `LocomotionInput::heading`).
    heading: f32,
}

/// Raised guard: the left upper arm forward and a little across the body, the forearm upright in
/// front of the chest with its outer side (where shields sit, local -X) facing forward.
pub fn guard_arm(p: &mut HumanoidPose, weight: f32) {
    if weight <= 0.0 {
        return;
    }
    let upper = rx(1.2) * rz(0.35);
    // Forearm orientation in the chest frame: local -Y (along the forearm) points up, local -X
    // (the shield face) points forward (-Z).
    let forearm = Quat::from_mat3(&Mat3::from_cols(Vec3::Z, Vec3::NEG_Y, Vec3::X));
    let fore_local = upper.inverse() * forearm;
    let (u, f, h) = (p.get(Bone::UpperArmL), p.get(Bone::ForearmL), p.get(Bone::HandL));
    p.set(Bone::UpperArmL, u.slerp(upper, weight))
        .set(Bone::ForearmL, f.slerp(fore_local, weight))
        .set(Bone::HandL, h.slerp(Quat::IDENTITY, weight));
}

/// Actions that pose the left arm themselves (it isn't free to hold a weapon or a grip).
fn left_arm_busy(action: &CharacterAction) -> bool {
    let Some(a) = action.current else { return false };
    match a.kind {
        ActionKind::LightAttack(_) | ActionKind::HeavyCharge | ActionKind::HeavyAttack | ActionKind::JumpAttack | ActionKind::RunningAttack => {
            a.hand != AttackHand::Right
        }
        ActionKind::Ability(id) => id.strike().is_none(),
        // A flinch keeps the grip; rolls, drinking, resting, doors… pose the arm themselves.
        ActionKind::Hurt | ActionKind::Stunned => false,
        _ => true,
    }
}

/// What the hands hold: a left-hand weapon is carried in the mirror image of its class's guard
/// (unless an action uses the left arm); a weapon held in both hands gets the free hand on its
/// handle (IK, just below the right fist toward the pommel).
fn hands_layer(spec: &HumanoidSpec, pose: &mut HumanoidPose, action: &CharacterAction, wielded: Option<&Wielded>) {
    let Some(w) = wielded else { return };
    if left_arm_busy(action) {
        return;
    }
    match (w.two_hand, w.left) {
        (Some(_), _) => {
            let hand = bone_model_transform(spec, pose, Bone::HandR);
            let grip = hand.transform_point(Vec3::NEG_Y * 0.12);
            reach_left_hand(spec, pose, grip);
        }
        (None, Some(left)) => {
            let attacking_right = action.current.is_some_and(|a| a.damage_multiplier().is_some() || a.kind == ActionKind::HeavyCharge);
            // During a right-hand swing the left weapon is held back a little (still in guard).
            let g = guard_pose(left.class).mirrored();
            let k = if attacking_right { 0.7 } else { 1.0 };
            for bone in [Bone::UpperArmL, Bone::ForearmL, Bone::HandL] {
                pose.set(bone, pose.get(bone).slerp(g.get(bone), k));
            }
        }
        (None, None) => {}
    }
}

/// Guarding with a two-handed weapon: the blade raised across the body (the left hand follows by IK).
fn weapon_guard_arm(p: &mut HumanoidPose, weight: f32) {
    if weight <= 0.0 {
        return;
    }
    let mut g = *p;
    set_sword_arm(&mut g, 1.05, 1.25, Vec3::new(-0.35, 0.0, 1.35));
    for bone in [Bone::UpperArmR, Bone::ForearmR, Bone::HandR] {
        p.set(bone, p.get(bone).slerp(g.get(bone), weight));
    }
}

/// Brief losses of ground shorter than this (stepping over rubble, a missed probe) don't
/// switch to the air pose or trigger a landing.
const AIR_GRACE: f32 = 0.1;
/// Gait speed while pushing against a wall.
const BLOCKED_GAIT_SPEED: f32 = 1.1;

pub fn animate_humanoids(
    time: Res<Time>,
    mut characters: Query<(
        &HumanoidSpec,
        &HumanoidRig,
        &mut HumanoidAnimator,
        &CharacterController,
        &CharacterAction,
        Option<&Wielded>,
        Option<&Guard>,
    ), Without<crate::character::Dormant>>,
    mut transforms: Query<&mut Transform>,
) {
    let dt = time.delta_secs();
    let smooth = |rate: f32| 1.0 - (-rate * dt).exp();
    for (spec, rig, mut anim, ctrl, action, wielded, guard) in &mut characters {
        anim.time += dt;
        // Airborne only after a short grace period, or right away when clearly jumping.
        if ctrl.grounded {
            if anim.air_time > AIR_GRACE {
                anim.landing = 1.0;
            }
            anim.air_time = 0.0;
        } else {
            anim.air_time += dt;
        }
        let airborne = anim.air_time > AIR_GRACE || (!ctrl.grounded && ctrl.velocity.y > 1.0);
        anim.air += (if airborne { 1.0 } else { 0.0 } - anim.air) * smooth(14.0);
        anim.landing = (anim.landing - dt / 0.22).max(0.0);

        // Speed actually achieved, not the controller's velocity (which jitters against walls).
        let actual = Vec2::new(ctrl.moved.x, ctrl.moved.z).length().min(SPRINT_SPEED * 1.5);
        // Pushing against something: wants to move, but gets almost nowhere.
        let pushing = !airborne && ctrl.wish_speed > 1.0 && actual < ctrl.wish_speed * 0.35;
        anim.blocked += (if pushing { 1.0 } else { 0.0 } - anim.blocked) * smooth(if pushing { 6.0 } else { 10.0 });
        let target = actual.max(BLOCKED_GAIT_SPEED * anim.blocked);
        anim.speed += (target - anim.speed) * smooth(10.0);
        if !airborne {
            anim.phase = (anim.phase + anim.speed / stride_length(anim.speed) * TAU * dt) % TAU;
        }
        // Where we go relative to where we face (forward = -Z at facing 0).
        if actual > 0.3 {
            let want = travel_heading(ctrl.facing, ctrl.moved);
            let d = (want - anim.heading + PI).rem_euclid(TAU) - PI;
            anim.heading += d * smooth(8.0);
        } else {
            anim.heading *= 1.0 - smooth(4.0);
        }

        let loco = locomotion_pose(
            spec,
            &LocomotionInput {
                speed: anim.speed,
                phase: anim.phase,
                air: anim.air,
                vertical: ctrl.velocity.y,
                landing: bell(1.0 - anim.landing) * anim.landing.min(1.0).sqrt(),
                blocked: anim.blocked,
                time: anim.time,
                weapon: wielded.map_or_else(WeaponClass::default, |w| w.class()),
                heading: anim.heading,
            },
        );

        // Action layer: snaps in fast (so swings and hitboxes agree), fades out a bit slower.
        let current = action.current.as_ref().and_then(action_pose);
        let mut pose = match current {
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

        // Guard layer: the shield arm follows the raise (eased); lowering takes ~0.15 s.
        let raise = guard.map_or(0.0, Guard::raise_progress);
        anim.guard = if guard.is_some_and(Guard::is_up) { raise * raise * (3.0 - 2.0 * raise) } else { (anim.guard - dt / 0.15).max(0.0) };
        let two_handed = wielded.is_some_and(|w| w.two_hand.is_some());
        if two_handed {
            weapon_guard_arm(&mut pose, anim.guard); // guarding with the weapon held in both hands
        } else {
            guard_arm(&mut pose, anim.guard);
        }
        hands_layer(spec, &mut pose, action, wielded);

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

    #[test]
    fn strafing_turns_the_hips_and_backs_up() {
        // Facing north (-Z): moving west is to the left, east to the right, south is backward.
        assert!((travel_heading(0.0, Vec3::NEG_X) - FRAC_PI_2).abs() < 1e-4);
        assert!((travel_heading(0.0, Vec3::X) + FRAC_PI_2).abs() < 1e-4);
        assert!(travel_heading(0.0, Vec3::NEG_Z).abs() < 1e-4);
        // Facing west (+π/2): moving south is to the left.
        assert!((travel_heading(FRAC_PI_2, Vec3::Z) - FRAC_PI_2).abs() < 1e-4);
        let (twist, back) = strafe(FRAC_PI_2);
        assert!(!back && twist > 1.0 && twist <= MAX_HIP_TWIST, "sideways: hips turned left");
        let (twist, back) = strafe(PI);
        assert!(back && twist.abs() < 1e-4, "straight back: hips square, cycle reversed");
        let (twist, back) = strafe(-2.4);
        assert!(back && twist > 0.0, "back and to the right: hips turn the other way");
        // The pose actually turns the pelvis.
        let spec = HumanoidSpec::human();
        let input = LocomotionInput { speed: 3.0, phase: 1.0, heading: FRAC_PI_2, ..default() };
        let p = locomotion_pose(&spec, &input);
        let yaw = (p.get(Bone::Pelvis) * Vec3::NEG_Z).x;
        assert!(yaw < -0.5, "the pelvis turns toward -X (left): {yaw}");
    }
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

    fn hand_tip(class: WeaponClass, kind: ActionKind, hand: AttackHand, bone: Bone, elapsed: f32) -> Vec3 {
        let spec = HumanoidSpec::human();
        let a = ActiveAction { elapsed, ..ActiveAction::new(kind, Vec3::NEG_Z, class).with_hand(hand) };
        let (pose, _) = action_pose(&a).unwrap();
        bone_model_transform(&spec, &pose, bone).transform_point(Vec3::Y * 1.03)
    }

    #[test]
    fn left_swings_are_the_mirror_of_right_swings() {
        let d = at(ActionKind::LightAttack(0), 0.0).def;
        for t in [d.startup * 0.5, d.startup + d.active * 0.5, d.startup + d.active] {
            let r = hand_tip(WeaponClass::LightSword, ActionKind::LightAttack(0), AttackHand::Right, Bone::HandR, t);
            let l = hand_tip(WeaponClass::LightSword, ActionKind::LightAttack(0), AttackHand::Left, Bone::HandL, t);
            assert!((l - Vec3::new(-r.x, r.y, r.z)).length() < 1e-3, "{r} ↔ {l}");
        }
    }

    #[test]
    fn dual_swings_bring_both_blades_in_front() {
        for class in [WeaponClass::LightSword, WeaponClass::Axe] {
            let d = at_with(class, ActionKind::LightAttack(0), 0.0).def;
            let best = |bone| {
                (0..=10)
                    .map(|i| hand_tip(class, ActionKind::LightAttack(0), AttackHand::Both, bone, d.startup + d.active * i as f32 / 10.0).z)
                    .fold(f32::INFINITY, f32::min)
            };
            assert!(best(Bone::HandR) < -0.9 && best(Bone::HandL) < -0.9, "{class:?}: both blades reach forward");
        }
    }

    #[test]
    fn axe_chops_come_down_vertically() {
        let d = at_with(WeaponClass::Axe, ActionKind::LightAttack(0), 0.0).def;
        let top = tip_with(WeaponClass::Axe, ActionKind::LightAttack(0), d.startup);
        let bottom = tip_with(WeaponClass::Axe, ActionKind::LightAttack(0), d.startup + d.active);
        assert!(top.y > 2.0, "raised over the head: {top}");
        assert!(bottom.y < 1.0 && bottom.z < -0.5, "down in front: {bottom}");
        assert!((top.x - bottom.x).abs() < 0.5, "a vertical arc, not a sideways sweep: {top} → {bottom}");
    }

    /// Every class's strikes must end in front of the character, or they can't hit anything.
    #[test]
    fn every_class_strikes_in_front() {
        // Natural weapons (a beast's jaws) are posed by `crate::quadruped`, tested there.
        for class in WeaponClass::ALL.into_iter().filter(|c| !c.is_natural()) {
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

    /// Lowest sole point of the pose, in model space.
    fn lowest_sole(spec: &HumanoidSpec, pose: &HumanoidPose) -> f32 {
        let model = pose.model_transform(0.0);
        [Bone::FootL, Bone::FootR]
            .into_iter()
            .flat_map(|f| {
                let t = model * bone_model_transform(spec, pose, f);
                spec.sole_points().map(|q| t.transform_point(q).y)
            })
            .fold(f32::INFINITY, f32::min)
    }

    #[test]
    fn walking_moves_the_feet_apart() {
        let spec = HumanoidSpec::human();
        for speed in [SLOW_WALK_SPEED, WALK_SPEED, SPRINT_SPEED] {
            // Phase 0: left heel strikes in front, right foot behind.
            let pose = locomotion_pose(&spec, &LocomotionInput { speed, phase: 0.0, ..default() });
            let l = bone_model_transform(&spec, &pose, Bone::FootL).translation;
            let r = bone_model_transform(&spec, &pose, Bone::FootR).translation;
            assert!(l.z < -0.1 && r.z > 0.1, "{speed} m/s: left forward, right back: {l} {r}");
        }
    }

    /// Through the whole cycle of every gait, a foot touches the floor and nothing sinks in
    /// (flight phases may lift off a little).
    #[test]
    fn feet_stay_planted() {
        let spec = HumanoidSpec::human();
        for speed in [0.0, SLOW_WALK_SPEED, 3.0, WALK_SPEED, SPRINT_SPEED] {
            for blocked in [0.0, 1.0] {
                for i in 0..40 {
                    let phase = TAU * i as f32 / 40.0;
                    let input = LocomotionInput { speed, phase, blocked, ..default() };
                    let low = lowest_sole(&spec, &locomotion_pose(&spec, &input));
                    assert!((-0.005..0.07).contains(&low), "{speed} m/s, phase {phase:.2}: sole at {low:.3}");
                }
            }
        }
    }

    #[test]
    fn sprint_trails_the_blade_behind() {
        let spec = HumanoidSpec::human();
        let pose = locomotion_pose(&spec, &LocomotionInput { speed: SPRINT_SPEED, ..default() });
        let hand = bone_model_transform(&spec, &pose, Bone::HandR);
        let tip = hand.transform_point(Vec3::Y * 1.03);
        assert!(tip.z > hand.translation.z + 0.4 && tip.y < hand.translation.y, "hand {} tip {tip}", hand.translation);
    }

    #[test]
    fn raised_guard_puts_the_shield_in_front_of_the_chest() {
        let spec = HumanoidSpec::human();
        let mut pose = locomotion_pose(&spec, &LocomotionInput::default());
        guard_arm(&mut pose, 1.0);
        let fore = bone_model_transform(&spec, &pose, Bone::ForearmL);
        // Shield centre and face direction as built by `items::models::wooden_shield`.
        let centre = fore.transform_point(Vec3::new(-0.075, -spec.forearm * 0.55, 0.0));
        let face = fore.rotation * Vec3::NEG_X;
        assert!(centre.z < -0.25 && (1.1..1.6).contains(&centre.y) && centre.x.abs() < 0.3, "{centre}");
        assert!(face.z < -0.9, "faces forward: {face}");
    }

    #[test]
    fn braced_hand_reaches_forward() {
        let spec = HumanoidSpec::human();
        let pose = locomotion_pose(&spec, &LocomotionInput { speed: BLOCKED_GAIT_SPEED, blocked: 1.0, ..default() });
        let hand = bone_model_transform(&spec, &pose, Bone::HandL).translation;
        assert!(hand.z < -0.4 && hand.y > 1.0, "{hand}");
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

    /// Kneeling at the Hearth puts a knee near the floor, but nothing below it.
    #[test]
    fn rest_kneels_on_the_floor() {
        let spec = HumanoidSpec::human();
        let d = at(ActionKind::Rest, 0.0).def;
        let (pose, _) = action_pose(&at(ActionKind::Rest, d.startup + 0.1)).unwrap();
        let model = pose.model_transform(0.0);
        let y = |b: Bone| (model * bone_model_transform(&spec, &pose, b)).translation.y;
        let ankle = spec.ankle_height();
        assert!((0.04..0.16).contains(&y(Bone::ShinR)), "right knee down: {}", y(Bone::ShinR));
        assert!((y(Bone::FootL) - ankle).abs() < 0.04, "left foot on the floor: ankle at {}", y(Bone::FootL));
        for bone in Bone::ALL {
            assert!(y(bone) > -0.05, "{bone:?} below the floor: {}", y(bone));
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
            ActionKind::UseItem,
            ActionKind::Rest,
            ActionKind::StandUp,
            ActionKind::Hurt,
            ActionKind::Stunned,
            ActionKind::Dead,
        ] {
            assert!(action_pose(&at(kind, 0.1)).is_some(), "{kind:?}");
        }
    }
}

