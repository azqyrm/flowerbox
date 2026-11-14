//! Reusable humanoid models and animation.
//!
//! Any character can get a humanoid body: describe it with a [`HumanoidSpec`] (proportions and
//! colours), build it with [`spawn_humanoid`], and put the returned [`HumanoidRig`] plus a
//! [`anim::HumanoidAnimator`] on the character entity. The shared animation system
//! ([`anim`]) then poses it from the character's movement and actions — walk/run cycles,
//! jumping, landing, rolls, attacks, hurt, death — for the player and future enemies alike.
//!
//! **Swapping the model:** a different humanoid only needs a different `HumanoidSpec` (or, later,
//! a glTF whose bones are mapped onto [`Bone`]). Gameplay reads poses through
//! [`pose::bone_model_transform`] (forward kinematics), never the mesh, so hitboxes follow.
//!
//! Conventions: model space has its origin at the feet and faces -Z; right side is +X.
//! Limb bones hang along their local -Y. Rotating a limb bone positively about X swings it
//! forward; a negative X rotation on a shin/forearm bends the knee back, a positive one on a
//! forearm bends the elbow forward.

pub mod anim;
pub mod pose;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::prelude::*;

pub struct HumanoidPlugin;

impl Plugin for HumanoidPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, anim::animate_humanoids);
    }
}

/// The bones of the shared humanoid skeleton.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Bone {
    Pelvis,
    Chest,
    Head,
    UpperArmL,
    ForearmL,
    HandL,
    UpperArmR,
    ForearmR,
    HandR,
    ThighL,
    ShinL,
    FootL,
    ThighR,
    ShinR,
    FootR,
}

impl Bone {
    pub const ALL: [Bone; 15] = [
        Bone::Pelvis,
        Bone::Chest,
        Bone::Head,
        Bone::UpperArmL,
        Bone::ForearmL,
        Bone::HandL,
        Bone::UpperArmR,
        Bone::ForearmR,
        Bone::HandR,
        Bone::ThighL,
        Bone::ShinL,
        Bone::FootL,
        Bone::ThighR,
        Bone::ShinR,
        Bone::FootR,
    ];
    pub const COUNT: usize = Self::ALL.len();

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn parent(self) -> Option<Bone> {
        use Bone::*;
        Some(match self {
            Pelvis => return None,
            Chest => Pelvis,
            Head => Chest,
            UpperArmL | UpperArmR => Chest,
            ForearmL => UpperArmL,
            ForearmR => UpperArmR,
            HandL => ForearmL,
            HandR => ForearmR,
            ThighL | ThighR => Pelvis,
            ShinL => ThighL,
            ShinR => ThighR,
            FootL => ShinL,
            FootR => ShinR,
        })
    }

    /// Chest, head and arms: the part of the body an upper-body action (drinking, charging) poses.
    pub fn is_upper_body(self) -> bool {
        use Bone::*;
        matches!(self, Chest | Head | UpperArmL | ForearmL | HandL | UpperArmR | ForearmR | HandR)
    }
}

/// Colours of a humanoid.
#[derive(Clone, Debug)]
pub struct HumanoidLook {
    pub skin: Color,
    pub top: Color,
    pub bottom: Color,
    pub boots: Color,
    pub hair: Color,
}

/// Proportions (metres) and look of a humanoid. Swap this to get a different body.
#[derive(Component, Clone, Debug)]
pub struct HumanoidSpec {
    /// Hip joint height above the feet.
    pub pelvis_height: f32,
    /// Lower-spine pivot above the pelvis.
    pub spine: f32,
    /// Shoulder joints above the chest pivot.
    pub shoulder_height: f32,
    pub shoulder_x: f32,
    /// Head pivot (base of the skull) above the chest pivot.
    pub neck_height: f32,
    pub upper_arm: f32,
    pub forearm: f32,
    pub hip_x: f32,
    pub thigh: f32,
    pub shin: f32,
    /// Width, height, depth of the torso box.
    pub torso: Vec3,
    pub head_radius: f32,
    pub arm_radius: f32,
    pub leg_radius: f32,
    pub look: HumanoidLook,
}

impl HumanoidSpec {
    /// The player's proportions: ~1.8 m tall, matches the 1.8 m capsule.
    pub fn human() -> Self {
        Self {
            pelvis_height: 0.94,
            spine: 0.1,
            shoulder_height: 0.36,
            shoulder_x: 0.26,
            neck_height: 0.44,
            upper_arm: 0.28,
            forearm: 0.26,
            hip_x: 0.11,
            thigh: 0.43,
            shin: 0.43,
            torso: Vec3::new(0.44, 0.44, 0.25),
            head_radius: 0.13,
            arm_radius: 0.06,
            leg_radius: 0.08,
            look: HumanoidLook {
                skin: Color::srgb(0.78, 0.6, 0.48),
                top: Color::srgb(0.32, 0.22, 0.14),
                bottom: Color::srgb(0.15, 0.14, 0.13),
                boots: Color::srgb(0.12, 0.08, 0.05),
                hair: Color::srgb(0.12, 0.08, 0.05),
            },
        }
    }

    /// Height of the ankle (the foot bone's origin) above the floor in the rest pose.
    pub fn ankle_height(&self) -> f32 {
        self.pelvis_height - 0.02 - self.thigh - self.shin
    }

    /// Heel and toe of the sole in the foot bone's frame (matches the boot mesh). Locomotion
    /// keeps the lowest of these on the floor.
    pub fn sole_points(&self) -> [Vec3; 2] {
        let y = -self.ankle_height();
        [Vec3::new(0.0, y, FOOT_OFFSET + FOOT_LENGTH * 0.5), Vec3::new(0.0, y, FOOT_OFFSET - FOOT_LENGTH * 0.5)]
    }

    /// Rest translation of `bone` relative to its parent (Pelvis: relative to the feet).
    pub fn rest_offset(&self, bone: Bone) -> Vec3 {
        use Bone::*;
        match bone {
            Pelvis => Vec3::Y * self.pelvis_height,
            Chest => Vec3::Y * self.spine,
            Head => Vec3::Y * self.neck_height,
            UpperArmL => Vec3::new(-self.shoulder_x, self.shoulder_height, 0.0),
            UpperArmR => Vec3::new(self.shoulder_x, self.shoulder_height, 0.0),
            ForearmL | ForearmR => Vec3::NEG_Y * self.upper_arm,
            HandL | HandR => Vec3::NEG_Y * self.forearm,
            ThighL => Vec3::new(-self.hip_x, -0.02, 0.0),
            ThighR => Vec3::new(self.hip_x, -0.02, 0.0),
            ShinL | ShinR => Vec3::NEG_Y * self.thigh,
            FootL | FootR => Vec3::NEG_Y * self.shin,
        }
    }
}

/// Boot length and how far its centre sits in front of the ankle (-Z).
const FOOT_LENGTH: f32 = 0.27;
const FOOT_OFFSET: f32 = -0.05;

/// Entities of a spawned humanoid: the model root (child of the character) and every bone.
#[derive(Component, Clone, Debug)]
pub struct HumanoidRig {
    pub model: Entity,
    pub bones: [Entity; Bone::COUNT],
    /// Height of the character's origin above the feet (the capsule center).
    pub root_height: f32,
}

impl HumanoidRig {
    pub fn bone(&self, bone: Bone) -> Entity {
        self.bones[bone.index()]
    }
}

/// Builds the bone hierarchy and primitive meshes under `character`.
/// `root_height` = how far the character's origin sits above its feet.
pub fn spawn_humanoid(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    character: Entity,
    spec: &HumanoidSpec,
    root_height: f32,
) -> HumanoidRig {
    let mut mat = |c: Color| {
        materials.add(StandardMaterial { base_color: c, perceptual_roughness: 0.85, ..default() })
    };
    let skin = mat(spec.look.skin);
    let top = mat(spec.look.top);
    let bottom = mat(spec.look.bottom);
    let boots = mat(spec.look.boots);
    let hair = mat(spec.look.hair);
    let dark = mat(Color::srgb(0.03, 0.03, 0.03));

    let model = commands
        .spawn((Name::new("Humanoid Model"), Transform::from_xyz(0.0, -root_height, 0.0), Visibility::default()))
        .id();
    commands.entity(character).add_child(model);

    let mut bones = [Entity::PLACEHOLDER; Bone::COUNT];
    for bone in Bone::ALL {
        let e = commands
            .spawn((
                Name::new(format!("{bone:?}")),
                Transform::from_translation(spec.rest_offset(bone)),
                Visibility::default(),
            ))
            .id();
        bones[bone.index()] = e;
        let parent = bone.parent().map_or(model, |p| bones[p.index()]);
        commands.entity(parent).add_child(e);
    }

    // NoFrustumCulling: Bevy 0.20's culling hid the posed body parts mid-roll even though their
    // transforms were correct (ROADMAP N13). ~30 tiny meshes per humanoid, so skipping it is cheap.
    let mut part = |bone: Bone, mesh: Mesh, material: &Handle<StandardMaterial>, t: Transform| {
        let e = commands
            .spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.clone()), t, NoFrustumCulling))
            .id();
        commands.entity(bones[bone.index()]).add_child(e);
    };
    let t = Transform::from_xyz;
    let s = spec;
    // Torso
    part(Bone::Pelvis, Cuboid::new(s.torso.x * 0.85, 0.2, s.torso.z * 0.9).into(), &bottom, t(0.0, 0.02, 0.0));
    part(Bone::Pelvis, Cuboid::new(s.torso.x * 0.9, 0.07, s.torso.z * 0.95).into(), &boots, t(0.0, 0.1, 0.0));
    part(Bone::Chest, Cuboid::from_size(s.torso).into(), &top, t(0.0, s.torso.y * 0.5 + 0.02, 0.0));
    part(Bone::Chest, Cylinder::new(0.055, 0.1).into(), &skin, t(0.0, s.neck_height - 0.03, 0.0));
    // Head
    let hr = s.head_radius;
    part(Bone::Head, Sphere::new(hr).into(), &skin, t(0.0, hr, 0.0));
    part(Bone::Head, Sphere::new(hr * 1.04).into(), &hair, t(0.0, hr * 1.2, hr * 0.2));
    part(Bone::Head, Cuboid::new(hr, 0.025, 0.02).into(), &dark, t(0.0, hr * 1.1, -hr * 0.95));
    // Arms
    for (upper, fore, hand) in [(Bone::UpperArmL, Bone::ForearmL, Bone::HandL), (Bone::UpperArmR, Bone::ForearmR, Bone::HandR)] {
        part(upper, Sphere::new(s.arm_radius * 1.3).into(), &top, Transform::IDENTITY);
        part(upper, Capsule3d::new(s.arm_radius * 1.1, s.upper_arm - s.arm_radius).into(), &top, t(0.0, -s.upper_arm * 0.5, 0.0));
        part(fore, Capsule3d::new(s.arm_radius, s.forearm - s.arm_radius).into(), &top, t(0.0, -s.forearm * 0.5, 0.0));
        part(hand, Sphere::new(s.arm_radius * 1.15).into(), &skin, t(0.0, -0.03, 0.0));
    }
    // Legs
    for (thigh, shin, foot) in [(Bone::ThighL, Bone::ShinL, Bone::FootL), (Bone::ThighR, Bone::ShinR, Bone::FootR)] {
        part(thigh, Capsule3d::new(s.leg_radius * 1.1, s.thigh - s.leg_radius).into(), &bottom, t(0.0, -s.thigh * 0.5, 0.0));
        part(shin, Capsule3d::new(s.leg_radius, s.shin - s.leg_radius).into(), &bottom, t(0.0, -s.shin * 0.5, 0.0));
        part(foot, Cuboid::new(s.leg_radius * 2.0, 0.1, FOOT_LENGTH).into(), &boots, t(0.0, -s.ankle_height() + 0.05, FOOT_OFFSET));
    }

    HumanoidRig { model, bones, root_height }
}
