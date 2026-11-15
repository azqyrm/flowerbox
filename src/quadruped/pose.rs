//! A full-body quadruped pose plus forward kinematics. Pure data and maths, no ECS.

use bevy::prelude::*;

use super::{QBone, QuadrupedSpec};

/// Local bone rotations (relative to the rest pose) plus whole-body offsets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuadrupedPose {
    pub bones: [Quat; QBone::COUNT],
    /// Added to the hips' rest position (crouching, shifting back, bobbing).
    pub hips_offset: Vec3,
    /// Added to the chest's rest position (rearing the front up).
    pub chest_offset: Vec3,
    /// Whole-body rotation around a point `root_pivot` (model space) — falls, pitching.
    pub root_rot: Quat,
    pub root_pivot: Vec3,
    /// Whole-body translation (leaps).
    pub root_offset: Vec3,
}

impl Default for QuadrupedPose {
    fn default() -> Self {
        Self {
            bones: [Quat::IDENTITY; QBone::COUNT],
            hips_offset: Vec3::ZERO,
            chest_offset: Vec3::ZERO,
            root_rot: Quat::IDENTITY,
            root_pivot: Vec3::new(0.0, 0.5, 0.0),
            root_offset: Vec3::ZERO,
        }
    }
}

impl QuadrupedPose {
    pub fn get(&self, bone: QBone) -> Quat {
        self.bones[bone.index()]
    }

    pub fn set(&mut self, bone: QBone, rot: Quat) -> &mut Self {
        self.bones[bone.index()] = rot;
        self
    }

    /// Blend toward `other` by `t` (0 = self, 1 = other).
    pub fn blend(&self, other: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let mut out = *self;
        for bone in QBone::ALL {
            out.bones[bone.index()] = self.get(bone).slerp(other.get(bone), t);
        }
        out.hips_offset = self.hips_offset.lerp(other.hips_offset, t);
        out.chest_offset = self.chest_offset.lerp(other.chest_offset, t);
        out.root_rot = self.root_rot.slerp(other.root_rot, t);
        out.root_pivot = self.root_pivot.lerp(other.root_pivot, t);
        out.root_offset = self.root_offset.lerp(other.root_offset, t);
        out
    }

    /// Local transform of the model root relative to the character origin (`root_height` above the feet).
    pub fn model_transform(&self, root_height: f32) -> Transform {
        let pivot = self.root_pivot;
        Transform {
            translation: Vec3::Y * -root_height + self.root_offset + pivot - self.root_rot * pivot,
            rotation: self.root_rot,
            scale: Vec3::ONE,
        }
    }

    /// Local transform of `bone` relative to its parent.
    pub fn bone_local(&self, spec: &QuadrupedSpec, bone: QBone) -> Transform {
        let mut translation = spec.rest_offset(bone);
        match bone {
            QBone::Hips => translation += self.hips_offset,
            QBone::Chest => translation += self.chest_offset,
            _ => {}
        }
        Transform { translation, rotation: self.get(bone), scale: Vec3::ONE }
    }
}

/// Forward kinematics: `bone`'s transform in model space (origin at the feet).
pub fn bone_model_transform(spec: &QuadrupedSpec, pose: &QuadrupedPose, bone: QBone) -> Transform {
    let local = pose.bone_local(spec, bone);
    match bone.parent() {
        Some(parent) => bone_model_transform(spec, pose, parent) * local,
        None => local,
    }
}

/// Transform of `bone` relative to the character origin.
pub fn bone_character_transform(spec: &QuadrupedSpec, pose: &QuadrupedPose, root_height: f32, bone: QBone) -> Transform {
    pose.model_transform(root_height) * bone_model_transform(spec, pose, bone)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rest_pose_stands_on_four_paws_facing_forward() {
        let spec = QuadrupedSpec::warg();
        let pose = QuadrupedPose::default();
        for leg in QBone::LEGS {
            let paw = bone_model_transform(&spec, &pose, leg.paw).translation;
            assert!((0.0..0.1).contains(&paw.y), "{:?} paw at {paw}", leg.paw);
            assert!(paw.x.signum() == leg.side, "{:?} on its side", leg.paw);
        }
        let head = bone_model_transform(&spec, &pose, QBone::Head);
        let hips = bone_model_transform(&spec, &pose, QBone::Hips).translation;
        assert!(head.translation.z < hips.z - 1.0, "head well in front of the hips");
        let snout = head.transform_point(spec.snout_tip());
        assert!(snout.z < head.translation.z - 0.3 && (0.7..1.4).contains(&snout.y), "{snout}");
    }

    #[test]
    fn swinging_a_leg_forward_moves_the_paw_forward() {
        let spec = QuadrupedSpec::warg();
        let mut pose = QuadrupedPose::default();
        let before = bone_model_transform(&spec, &pose, QBone::PawFL).translation;
        pose.set(QBone::UpperFL, Quat::from_rotation_x(0.5));
        let after = bone_model_transform(&spec, &pose, QBone::PawFL).translation;
        assert!(after.z < before.z - 0.2, "{before} -> {after}");
    }
}
