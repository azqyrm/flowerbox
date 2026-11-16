//! A full-body humanoid pose plus forward kinematics. Pure data and maths, no ECS.

use bevy::prelude::*;

use super::{Bone, HumanoidSpec};

/// Local bone rotations (relative to the rest pose) plus whole-body offsets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanoidPose {
    pub bones: [Quat; Bone::COUNT],
    /// Added to the pelvis rest position (crouching, bobbing).
    pub pelvis_offset: Vec3,
    /// Whole-body rotation around a point `root_pivot` metres above the feet (rolls, falls).
    pub root_rot: Quat,
    pub root_pivot: f32,
    /// Whole-body translation (hops, dips).
    pub root_offset: Vec3,
}

impl Default for HumanoidPose {
    fn default() -> Self {
        Self {
            bones: [Quat::IDENTITY; Bone::COUNT],
            pelvis_offset: Vec3::ZERO,
            root_rot: Quat::IDENTITY,
            root_pivot: 0.9,
            root_offset: Vec3::ZERO,
        }
    }
}

/// Which bones a layered pose controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoseMask {
    Full,
    UpperBody,
}

impl HumanoidPose {
    pub fn get(&self, bone: Bone) -> Quat {
        self.bones[bone.index()]
    }

    pub fn set(&mut self, bone: Bone, rot: Quat) -> &mut Self {
        self.bones[bone.index()] = rot;
        self
    }

    /// Applies `rot` after the bone's current rotation.
    pub fn add(&mut self, bone: Bone, rot: Quat) -> &mut Self {
        self.bones[bone.index()] *= rot;
        self
    }

    /// Blend toward `other` by `t` (0 = self, 1 = other) on the bones in `mask`.
    pub fn blend(&self, other: &Self, t: f32, mask: PoseMask) -> Self {
        let t = t.clamp(0.0, 1.0);
        let mut out = *self;
        for bone in Bone::ALL {
            if mask == PoseMask::Full || bone.is_upper_body() {
                out.bones[bone.index()] = self.get(bone).slerp(other.get(bone), t);
            }
        }
        if mask == PoseMask::Full {
            out.pelvis_offset = self.pelvis_offset.lerp(other.pelvis_offset, t);
            out.root_rot = self.root_rot.slerp(other.root_rot, t);
            out.root_pivot = self.root_pivot + (other.root_pivot - self.root_pivot) * t;
            out.root_offset = self.root_offset.lerp(other.root_offset, t);
        }
        out
    }

    /// Local transform of the model root relative to the character origin (`root_height` above the feet).
    pub fn model_transform(&self, root_height: f32) -> Transform {
        let pivot = Vec3::Y * self.root_pivot;
        Transform {
            translation: Vec3::Y * -root_height + self.root_offset + pivot - self.root_rot * pivot,
            rotation: self.root_rot,
            scale: Vec3::ONE,
        }
    }

    /// Local transform of `bone` relative to its parent.
    pub fn bone_local(&self, spec: &HumanoidSpec, bone: Bone) -> Transform {
        let mut translation = spec.rest_offset(bone);
        if bone == Bone::Pelvis {
            translation += self.pelvis_offset;
        }
        Transform { translation, rotation: self.get(bone), scale: Vec3::ONE }
    }
}

impl HumanoidPose {
    /// The mirror image across the body's middle plane (x → −x): left and right bones swap, and
    /// their rotations are reflected. A left-handed swing is the mirrored right-handed one.
    pub fn mirrored(&self) -> Self {
        let reflect = |q: Quat| Quat::from_xyzw(q.x, -q.y, -q.z, q.w);
        let flip = |v: Vec3| Vec3::new(-v.x, v.y, v.z);
        let mut out = *self;
        for bone in Bone::ALL {
            out.bones[bone.mirror().index()] = reflect(self.get(bone));
        }
        out.pelvis_offset = flip(self.pelvis_offset);
        out.root_rot = reflect(self.root_rot);
        out.root_offset = flip(self.root_offset);
        out
    }
}

/// Two-bone IK: turns the left upper arm and bends the elbow so the left hand reaches `target`
/// (model space), the elbow pointing down and out. Unreachable targets are approached as far as
/// the arm goes. (Two-handed grips: the free hand on the handle.)
pub fn reach_left_hand(spec: &HumanoidSpec, pose: &mut HumanoidPose, target: Vec3) {
    let chest = bone_model_transform(spec, pose, Bone::Chest);
    let shoulder = chest.transform_point(spec.rest_offset(Bone::UpperArmL));
    let (a, b) = (spec.upper_arm, spec.forearm);
    let to = target - shoulder;
    let d = to.length().clamp((a - b).abs() + 1e-3, a + b - 1e-3);
    // Elbow: the forearm bends forward (+X) by `bend`.
    let inner = ((a * a + b * b - d * d) / (2.0 * a * b)).clamp(-1.0, 1.0).acos();
    let bend = std::f32::consts::PI - inner;
    let hand_in_arm = Vec3::NEG_Y * a + Quat::from_rotation_x(bend) * (Vec3::NEG_Y * b);
    let want = (chest.rotation.inverse() * to).normalize_or(Vec3::NEG_Y);
    let aim = Quat::from_rotation_arc(hand_in_arm.normalize(), want);
    // Any twist about the shoulder→hand line keeps the hand in place: pick the one that puts the
    // elbow lowest and most to the outside (−X for the left arm).
    let best = (0..12)
        .map(|k| Quat::from_axis_angle(want, k as f32 * std::f32::consts::TAU / 12.0) * aim)
        .max_by(|p, q| {
            let score = |r: &Quat| {
                let elbow = *r * (Vec3::NEG_Y * a);
                -elbow.y - 0.6 * elbow.x
            };
            score(p).total_cmp(&score(q))
        })
        .unwrap_or(aim);
    pose.set(Bone::UpperArmL, best).set(Bone::ForearmL, Quat::from_rotation_x(bend));
}

/// Forward kinematics: `bone`'s transform in model space (origin at the feet).
pub fn bone_model_transform(spec: &HumanoidSpec, pose: &HumanoidPose, bone: Bone) -> Transform {
    let local = pose.bone_local(spec, bone);
    match bone.parent() {
        Some(parent) => bone_model_transform(spec, pose, parent) * local,
        None => local,
    }
}

/// Transform of `bone` relative to the character origin.
pub fn bone_character_transform(spec: &HumanoidSpec, pose: &HumanoidPose, root_height: f32, bone: Bone) -> Transform {
    pose.model_transform(root_height) * bone_model_transform(spec, pose, bone)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rest_pose_proportions() {
        let spec = HumanoidSpec::human();
        let pose = HumanoidPose::default();
        let foot = bone_model_transform(&spec, &pose, Bone::FootL).translation;
        assert!(foot.y > 0.0 && foot.y < 0.1, "ankle just above the floor: {foot}");
        let hand = bone_model_transform(&spec, &pose, Bone::HandR).translation;
        assert!(hand.x > 0.2 && (0.75..0.95).contains(&hand.y), "hand hangs at the hip: {hand}");
        let head = bone_model_transform(&spec, &pose, Bone::Head).translation;
        assert!(head.y + 2.0 * spec.head_radius < 1.85, "fits the 1.8 m capsule: {head}");
    }

    #[test]
    fn swinging_the_thigh_forward_moves_the_foot_forward() {
        let spec = HumanoidSpec::human();
        let mut pose = HumanoidPose::default();
        pose.set(Bone::ThighL, Quat::from_rotation_x(0.6));
        let foot = bone_model_transform(&spec, &pose, Bone::FootL).translation;
        assert!(foot.z < -0.3, "{foot}");
    }

    #[test]
    fn mirroring_swaps_sides() {
        let spec = HumanoidSpec::human();
        let mut pose = HumanoidPose::default();
        pose.set(Bone::UpperArmR, Quat::from_rotation_y(0.5) * Quat::from_rotation_x(1.2));
        let right = bone_model_transform(&spec, &pose, Bone::HandR).translation;
        let left = bone_model_transform(&spec, &pose.mirrored(), Bone::HandL).translation;
        assert!((left - Vec3::new(-right.x, right.y, right.z)).length() < 1e-4, "{right} ↔ {left}");
        assert_eq!(pose.mirrored().mirrored(), pose);
    }

    #[test]
    fn left_hand_reaches_its_target() {
        let spec = HumanoidSpec::human();
        let mut pose = HumanoidPose::default();
        // Reachable (the arm is 0.54 m, the shoulder at about (-0.26, 1.4, 0)).
        for target in [Vec3::new(0.0, 1.15, -0.3), Vec3::new(-0.05, 1.25, -0.32), Vec3::new(-0.2, 0.98, -0.2)] {
            reach_left_hand(&spec, &mut pose, target);
            let hand = bone_model_transform(&spec, &pose, Bone::HandL).translation;
            assert!(hand.distance(target) < 0.02, "{target} → {hand}");
            let elbow = bone_model_transform(&spec, &pose, Bone::ForearmL).translation;
            assert!(elbow.y < hand.y.max(1.3), "elbow not stuck up: {elbow}");
        }
        // Out of reach: the arm stretches straight toward it.
        let far = Vec3::new(0.5, 1.0, -0.8);
        reach_left_hand(&spec, &mut pose, far);
        let shoulder = bone_model_transform(&spec, &pose, Bone::UpperArmL).translation;
        let hand = bone_model_transform(&spec, &pose, Bone::HandL).translation;
        assert!((hand - shoulder).normalize().dot((far - shoulder).normalize()) > 0.99);
    }

    #[test]
    fn root_rotation_turns_around_the_pivot() {
        let pose = HumanoidPose { root_rot: Quat::from_rotation_x(std::f32::consts::PI), root_pivot: 0.9, ..Default::default() };
        let t = pose.model_transform(0.9);
        // The pivot (0.9 above the feet = character origin) stays in place.
        let pivot_after = t.transform_point(Vec3::Y * 0.9);
        assert!(pivot_after.length() < 1e-4, "{pivot_after}");
    }
}
