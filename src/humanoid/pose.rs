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
    fn root_rotation_turns_around_the_pivot() {
        let mut pose = HumanoidPose::default();
        pose.root_rot = Quat::from_rotation_x(std::f32::consts::PI);
        pose.root_pivot = 0.9;
        let t = pose.model_transform(0.9);
        // The pivot (0.9 above the feet = character origin) stays in place.
        let pivot_after = t.transform_point(Vec3::Y * 0.9);
        assert!(pivot_after.length() < 1e-4, "{pivot_after}");
    }
}
