//! Pure pose maths for the sword arm, shared by the animation (visuals.rs) and the
//! hitbox (combat.rs) so what you see is exactly what hits.
//!
//! Model space: origin at the feet, facing -Z, right hand on +X.
//! The right arm hangs from the shoulder along its local -Y; the sword grip sits at the hand
//! and the blade runs along the grip's local +Y.

use std::f32::consts::PI;

use bevy::prelude::*;

use super::action::{ActionKind, ActionPhase, ActiveAction};
use crate::tuning::*;

pub const SHOULDER: Vec3 = Vec3::new(0.33, 1.42, 0.0);
pub const ARM_LENGTH: f32 = 0.57;
/// Blade span along the grip's local +Y (from the crossguard to the tip).
pub const BLADE_START: f32 = 0.13;
pub const BLADE_END: f32 = 1.03;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArmPose {
    pub shoulder: Quat,
    pub grip: Quat,
}

impl ArmPose {
    fn new(shoulder: Quat, grip: Quat) -> Self {
        Self { shoulder, grip }
    }
    fn lerp(self, other: Self, t: f32) -> Self {
        let t = ease(t);
        Self { shoulder: self.shoulder.slerp(other.shoulder, t), grip: self.grip.slerp(other.grip, t) }
    }
}

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Arm hanging at the side, sword in a low guard pointing forward and down.
pub fn rest() -> ArmPose {
    ArmPose::new(Quat::IDENTITY, Quat::from_euler(EulerRot::XYZ, -1.95, 0.0, -0.15))
}

/// Blade continues the arm line (used for all swings).
fn extended() -> Quat {
    Quat::from_rotation_x(PI)
}

/// Arm pointing forward (`pitch` = π/2), up (π) or down (0), swept sideways by `yaw`
/// (positive = toward the left).
fn arm(yaw: f32, pitch: f32) -> ArmPose {
    ArmPose::new(Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch), extended())
}

/// (wind-up, strike end) poses of each swing.
fn swing_keys(kind: ActionKind) -> Option<(ArmPose, ArmPose)> {
    Some(match kind {
        ActionKind::LightAttack(0) => (arm(-1.4, 1.45), arm(1.3, 1.45)), // right → left
        ActionKind::LightAttack(1) => (arm(1.3, 1.5), arm(-1.4, 1.4)),   // left → right
        ActionKind::LightAttack(_) => (arm(0.0, 2.7), arm(0.0, 0.7)),    // overhead
        ActionKind::HeavyCharge | ActionKind::HeavyAttack => (arm(0.15, 3.3), arm(0.0, 0.75)),
        ActionKind::JumpAttack => (arm(0.0, 3.2), arm(0.0, 0.35)),
        _ => return None,
    })
}

/// Time of the active phase over which the swing travels (the jump attack strikes
/// quickly and then holds the blade down while plunging).
fn strike_duration(a: &ActiveAction) -> f32 {
    match a.kind {
        ActionKind::JumpAttack => JUMP_ATTACK_STRIKE,
        _ => a.def.active,
    }
}

pub fn arm_pose(action: Option<&ActiveAction>) -> ArmPose {
    let Some(a) = action else { return rest() };
    let Some((windup, strike)) = swing_keys(a.kind) else { return rest() };
    let d = &a.def;
    if a.kind == ActionKind::HeavyCharge {
        return rest().lerp(windup, a.elapsed / 0.2);
    }
    // The heavy attack starts from the raised charge pose.
    let start = if a.kind == ActionKind::HeavyAttack { windup } else { rest() };
    match a.phase() {
        ActionPhase::Startup => start.lerp(windup, a.elapsed / d.startup.max(1e-3)),
        ActionPhase::Active => windup.lerp(strike, (a.elapsed - d.startup) / strike_duration(a)),
        ActionPhase::Recovery => strike.lerp(rest(), (a.elapsed - d.startup - d.active) / d.recovery),
        ActionPhase::Done => rest(),
    }
}

/// A point on the blade (`along` = distance along the grip's +Y) in model space.
pub fn blade_point(pose: ArmPose, along: f32) -> Vec3 {
    SHOULDER + pose.shoulder * (Vec3::NEG_Y * ARM_LENGTH + pose.grip * (Vec3::Y * along))
}

/// The blade as a segment (crossguard, tip) in model space.
pub fn blade_segment(pose: ArmPose) -> (Vec3, Vec3) {
    (blade_point(pose, BLADE_START), blade_point(pose, BLADE_END))
}

/// Damage multiplier of an attacking action (None = this action doesn't hit).
pub fn damage_multiplier(a: &ActiveAction) -> Option<f32> {
    match a.kind {
        ActionKind::LightAttack(_) => Some(LIGHT_DAMAGE_MULT),
        ActionKind::HeavyAttack => Some(HEAVY_DAMAGE_MULT + (HEAVY_FULL_CHARGE_MULT - HEAVY_DAMAGE_MULT) * a.charge),
        ActionKind::JumpAttack => Some(JUMP_ATTACK_DAMAGE_MULT),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(kind: ActionKind, elapsed: f32) -> ActiveAction {
        ActiveAction { elapsed, ..ActiveAction::new(kind, Vec3::NEG_Z) }
    }

    #[test]
    fn rest_pose_matches_old_hand_position() {
        // The sword used to hang from a hand socket at (0.33, 0.85, -0.02).
        let grip = blade_point(rest(), 0.0);
        assert!((grip - Vec3::new(0.33, 0.85, 0.0)).length() < 0.03, "{grip}");
        let (_, tip) = blade_segment(rest());
        assert!(tip.z < -0.5 && tip.y < grip.y, "rests forward and down: {tip}");
    }

    #[test]
    fn light_slash_sweeps_right_to_left_in_front() {
        let d = at(ActionKind::LightAttack(0), 0.0).def;
        let (_, tip_a) = blade_segment(arm_pose(Some(&at(ActionKind::LightAttack(0), d.startup))));
        let (_, tip_mid) = blade_segment(arm_pose(Some(&at(ActionKind::LightAttack(0), d.startup + d.active * 0.5))));
        let (_, tip_b) = blade_segment(arm_pose(Some(&at(ActionKind::LightAttack(0), d.startup + d.active))));
        assert!(tip_a.x > 1.0, "starts on the right: {tip_a}");
        assert!(tip_b.x < -0.5, "ends on the left: {tip_b}");
        assert!(tip_mid.z < -1.2, "passes in front: {tip_mid}");
    }

    #[test]
    fn overhead_and_heavy_come_down_in_front() {
        for kind in [ActionKind::LightAttack(2), ActionKind::HeavyAttack] {
            let a = at(kind, 0.0);
            let (_, high) = blade_segment(arm_pose(Some(&at(kind, a.def.startup))));
            let (_, low) = blade_segment(arm_pose(Some(&at(kind, a.def.startup + a.def.active))));
            assert!(high.y > 2.2, "{kind:?} raised: {high}");
            assert!(low.y < 1.0 && low.z < -0.8, "{kind:?} lands in front: {low}");
        }
    }

    #[test]
    fn heavy_damage_scales_with_charge() {
        let mut a = at(ActionKind::HeavyAttack, 0.0);
        assert_eq!(damage_multiplier(&a), Some(HEAVY_DAMAGE_MULT));
        a.charge = 1.0;
        assert_eq!(damage_multiplier(&a), Some(HEAVY_FULL_CHARGE_MULT));
        assert_eq!(damage_multiplier(&at(ActionKind::Roll, 0.0)), None);
    }
}
