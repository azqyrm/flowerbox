//! Reusable four-legged bodies and their animation — the quadruped counterpart of
//! `crate::humanoid`.
//!
//! Any character can get a quadruped body: describe it with a [`QuadrupedSpec`] (proportions and
//! colours), build it with [`spawn_quadruped`], and put the returned [`QuadrupedRig`] plus an
//! [`anim::QuadrupedAnimator`] on the character entity. The shared animation system ([`anim`])
//! poses it from the character's movement and actions: walk / trot / gallop gaits, bites, the
//! pounce, hurt, stagger and death. A wolf, a boar or a hound is just another spec (plus a
//! moveset for its natural weapon, `crate::weapon`, and an AI profile, `crate::enemy::beast`).
//!
//! Gameplay reads poses through forward kinematics ([`pose::bone_character_transform`]), never the
//! mesh, so the bite hitbox (`crate::combat::bite_hits`) matches what's on screen.
//!
//! Conventions (as for humanoids): model space has its origin at the feet and faces -Z; right is
//! +X. Legs hang along their local -Y; rotating a leg bone positively about X swings it forward,
//! a negative X rotation on the lower leg folds the paw back.

pub mod anim;
pub mod pose;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::prelude::*;

pub struct QuadrupedPlugin;

impl Plugin for QuadrupedPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, anim::animate_quadrupeds);
    }
}

/// The bones of the shared quadruped skeleton.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QBone {
    /// The hindquarters; root of the skeleton.
    Hips,
    /// The front of the body (shoulders), child of the hips.
    Chest,
    Neck,
    Head,
    /// The lower jaw (opens by rotating about X).
    Jaw,
    Tail,
    UpperFL,
    LowerFL,
    PawFL,
    UpperFR,
    LowerFR,
    PawFR,
    UpperHL,
    LowerHL,
    PawHL,
    UpperHR,
    LowerHR,
    PawHR,
}

/// One leg's three bones, from the body down.
#[derive(Clone, Copy, Debug)]
pub struct Leg {
    pub upper: QBone,
    pub lower: QBone,
    pub paw: QBone,
    pub front: bool,
    /// -1 = left, +1 = right.
    pub side: f32,
}

impl QBone {
    pub const ALL: [QBone; 18] = [
        QBone::Hips,
        QBone::Chest,
        QBone::Neck,
        QBone::Head,
        QBone::Jaw,
        QBone::Tail,
        QBone::UpperFL,
        QBone::LowerFL,
        QBone::PawFL,
        QBone::UpperFR,
        QBone::LowerFR,
        QBone::PawFR,
        QBone::UpperHL,
        QBone::LowerHL,
        QBone::PawHL,
        QBone::UpperHR,
        QBone::LowerHR,
        QBone::PawHR,
    ];
    pub const COUNT: usize = Self::ALL.len();

    /// Front left, front right, hind left, hind right.
    pub const LEGS: [Leg; 4] = [
        Leg { upper: QBone::UpperFL, lower: QBone::LowerFL, paw: QBone::PawFL, front: true, side: -1.0 },
        Leg { upper: QBone::UpperFR, lower: QBone::LowerFR, paw: QBone::PawFR, front: true, side: 1.0 },
        Leg { upper: QBone::UpperHL, lower: QBone::LowerHL, paw: QBone::PawHL, front: false, side: -1.0 },
        Leg { upper: QBone::UpperHR, lower: QBone::LowerHR, paw: QBone::PawHR, front: false, side: 1.0 },
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn parent(self) -> Option<QBone> {
        use QBone::*;
        Some(match self {
            Hips => return None,
            Chest | Tail | UpperHL | UpperHR => Hips,
            Neck | UpperFL | UpperFR => Chest,
            Head => Neck,
            Jaw => Head,
            LowerFL => UpperFL,
            LowerFR => UpperFR,
            LowerHL => UpperHL,
            LowerHR => UpperHR,
            PawFL => LowerFL,
            PawFR => LowerFR,
            PawHL => LowerHL,
            PawHR => LowerHR,
        })
    }
}

/// Colours of a quadruped.
#[derive(Clone, Debug)]
pub struct QuadrupedLook {
    pub fur: Color,
    /// Mane / back ridge.
    pub dark: Color,
    pub belly: Color,
    pub claws: Color,
    pub eye_glow: LinearRgba,
}

/// Proportions (metres) and look of a quadruped. Swap this to get a different beast.
#[derive(Component, Clone, Debug)]
pub struct QuadrupedSpec {
    /// Hips → chest along the back.
    pub body_length: f32,
    /// How far the hips sit behind the character's centre (the collider covers the chest).
    pub hips_back: f32,
    pub front_upper: f32,
    pub front_lower: f32,
    pub hind_upper: f32,
    pub hind_lower: f32,
    pub leg_x: f32,
    pub chest_radius: f32,
    pub hips_radius: f32,
    /// Chest → base of the head (up, forward).
    pub neck: Vec2,
    pub head_radius: f32,
    /// Snout length in front of the skull.
    pub snout: f32,
    pub tail: f32,
    pub leg_radius: f32,
    pub look: QuadrupedLook,
}

/// Paws rest this far above the floor (their mesh reaches down to it).
const PAW_LIFT: f32 = 0.05;

impl QuadrupedSpec {
    /// A warg: a wolf the size of a pony, dark and shaggy, with smouldering eyes.
    pub fn warg() -> Self {
        Self {
            body_length: 0.82,
            hips_back: 0.45,
            front_upper: 0.4,
            front_lower: 0.38,
            hind_upper: 0.38,
            hind_lower: 0.39,
            leg_x: 0.16,
            chest_radius: 0.31,
            hips_radius: 0.25,
            neck: Vec2::new(0.24, 0.3),
            head_radius: 0.16,
            snout: 0.3,
            tail: 0.55,
            leg_radius: 0.065,
            look: QuadrupedLook {
                fur: Color::srgb(0.22, 0.2, 0.19),
                dark: Color::srgb(0.09, 0.08, 0.08),
                belly: Color::srgb(0.36, 0.33, 0.29),
                claws: Color::srgb(0.75, 0.72, 0.65),
                eye_glow: LinearRgba::rgb(7.0, 3.0, 0.3),
            },
        }
    }

    /// Height of the shoulders / hips above the floor in the rest pose.
    pub fn chest_height(&self) -> f32 {
        PAW_LIFT + self.front_upper + self.front_lower + 0.05
    }
    pub fn hips_height(&self) -> f32 {
        PAW_LIFT + self.hind_upper + self.hind_lower + 0.05
    }

    /// The tip of the snout in the head bone's frame (where bites land).
    pub fn snout_tip(&self) -> Vec3 {
        Vec3::new(0.0, -0.02, -(self.head_radius * 0.5 + self.snout))
    }

    /// Rest translation of `bone` relative to its parent (Hips: relative to the feet).
    pub fn rest_offset(&self, bone: QBone) -> Vec3 {
        use QBone::*;
        match bone {
            Hips => Vec3::new(0.0, self.hips_height(), self.hips_back),
            Chest => Vec3::new(0.0, self.chest_height() - self.hips_height(), -self.body_length),
            Neck => Vec3::new(0.0, self.chest_radius * 0.45, -self.chest_radius * 0.6),
            Head => Vec3::new(0.0, self.neck.y, -self.neck.x),
            Jaw => Vec3::new(0.0, -self.head_radius * 0.45, -self.head_radius * 0.3),
            Tail => Vec3::new(0.0, self.hips_radius * 0.5, self.hips_radius * 0.8),
            UpperFL => Vec3::new(-self.leg_x, -0.05, 0.0),
            UpperFR => Vec3::new(self.leg_x, -0.05, 0.0),
            UpperHL => Vec3::new(-self.leg_x, -0.05, 0.0),
            UpperHR => Vec3::new(self.leg_x, -0.05, 0.0),
            LowerFL | LowerFR => Vec3::NEG_Y * self.front_upper,
            LowerHL | LowerHR => Vec3::NEG_Y * self.hind_upper,
            PawFL | PawFR => Vec3::NEG_Y * self.front_lower,
            PawHL | PawHR => Vec3::NEG_Y * self.hind_lower,
        }
    }
}

/// Entities of a spawned quadruped: the model root (child of the character) and every bone.
#[derive(Component, Clone, Debug)]
pub struct QuadrupedRig {
    pub model: Entity,
    pub bones: [Entity; QBone::COUNT],
    /// Height of the character's origin above the feet (the capsule centre).
    pub root_height: f32,
}

impl QuadrupedRig {
    pub fn bone(&self, bone: QBone) -> Entity {
        self.bones[bone.index()]
    }
}

/// Builds the bone hierarchy and primitive meshes under `character`.
pub fn spawn_quadruped(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    character: Entity,
    spec: &QuadrupedSpec,
    root_height: f32,
) -> QuadrupedRig {
    let mut mat = |c: Color| materials.add(StandardMaterial { base_color: c, perceptual_roughness: 0.95, ..default() });
    let fur = mat(spec.look.fur);
    let dark = mat(spec.look.dark);
    let belly = mat(spec.look.belly);
    let claws = mat(spec.look.claws);
    let mouth = mat(Color::srgb(0.25, 0.05, 0.05));
    let eyes = materials.add(StandardMaterial { base_color: Color::BLACK, emissive: spec.look.eye_glow, ..default() });

    let model = commands
        .spawn((Name::new("Quadruped Model"), Transform::from_xyz(0.0, -root_height, 0.0), Visibility::default()))
        .id();
    commands.entity(character).add_child(model);
    let mut bones = [Entity::PLACEHOLDER; QBone::COUNT];
    for bone in QBone::ALL {
        let e = commands
            .spawn((Name::new(format!("{bone:?}")), Transform::from_translation(spec.rest_offset(bone)), Visibility::default()))
            .id();
        bones[bone.index()] = e;
        let parent = bone.parent().map_or(model, |p| bones[p.index()]);
        commands.entity(parent).add_child(e);
    }
    // NoFrustumCulling for the same reason as humanoids (posed parts got culled, ROADMAP N13).
    let mut part = |bone: QBone, mesh: Mesh, material: &Handle<StandardMaterial>, t: Transform| {
        let e = commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.clone()), t, NoFrustumCulling)).id();
        commands.entity(bones[bone.index()]).add_child(e);
    };
    let s = spec;
    let t = Transform::from_xyz;
    let along_z = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    // Body: a barrel from the hips to the chest, the chest deeper, a dark ridge along the back.
    let mid = Vec3::new(0.0, (s.chest_height() - s.hips_height()) * 0.5, -s.body_length * 0.5);
    let barrel = (s.chest_radius + s.hips_radius) * 0.5;
    part(QBone::Hips, Capsule3d::new(barrel * 0.95, s.body_length).into(), &fur, Transform::from_translation(mid).with_rotation(along_z));
    part(QBone::Hips, Sphere::new(s.hips_radius).into(), &fur, t(0.0, 0.0, 0.0));
    part(QBone::Hips, Capsule3d::new(barrel * 0.7, s.body_length * 0.8).into(), &belly, Transform::from_translation(mid - Vec3::Y * barrel * 0.35).with_rotation(along_z));
    part(QBone::Hips, Cuboid::new(0.12, 0.08, s.body_length * 0.9).into(), &dark, t(0.0, barrel * 0.95, mid.z));
    part(QBone::Chest, Sphere::new(s.chest_radius).into(), &fur, t(0.0, 0.0, 0.0));
    // Shaggy mane over the shoulders and neck.
    part(QBone::Chest, Sphere::new(1.0).into(), &dark, t(0.0, s.chest_radius * 0.45, -0.06).with_scale(Vec3::new(s.chest_radius * 0.95, s.chest_radius * 0.8, s.chest_radius * 1.1)));
    part(QBone::Neck, Capsule3d::new(0.13, s.neck.length() * 0.7).into(), &dark, Transform::from_translation(Vec3::new(0.0, s.neck.y * 0.5, -s.neck.x * 0.5)).with_rotation(Quat::from_rotation_x(-(s.neck.x.atan2(s.neck.y)))));
    // Head: skull, snout, ears, eyes; the lower jaw on its own bone.
    let hr = s.head_radius;
    part(QBone::Head, Sphere::new(hr).into(), &fur, t(0.0, 0.0, 0.0));
    part(QBone::Head, Cuboid::new(hr * 0.95, hr * 0.7, s.snout).into(), &fur, t(0.0, -0.02, -(hr * 0.5 + s.snout * 0.5)));
    part(QBone::Head, Cuboid::new(hr * 0.5, hr * 0.35, 0.08).into(), &dark, t(0.0, 0.0, -(hr * 0.5 + s.snout)));
    for x in [-1.0, 1.0] {
        part(QBone::Head, Cuboid::new(0.07, 0.15, 0.05).into(), &dark, Transform::from_xyz(x * hr * 0.55, hr * 0.85, hr * 0.15).with_rotation(Quat::from_rotation_z(-x * 0.25)));
        part(QBone::Head, Sphere::new(0.028).into(), &eyes, t(x * hr * 0.5, hr * 0.25, -hr * 0.75));
        // Upper fangs.
        part(QBone::Head, Cuboid::new(0.02, 0.06, 0.02).into(), &claws, t(x * hr * 0.32, -hr * 0.42, -(hr * 0.5 + s.snout * 0.85)));
    }
    part(QBone::Jaw, Cuboid::new(hr * 0.75, 0.05, s.snout * 0.95).into(), &fur, t(0.0, -0.01, -(hr * 0.2 + s.snout * 0.5)));
    part(QBone::Jaw, Cuboid::new(hr * 0.6, 0.02, s.snout * 0.8).into(), &mouth, t(0.0, 0.02, -(hr * 0.2 + s.snout * 0.5)));
    // Tail: hangs back and down from the hips (rotated by the animator).
    part(QBone::Tail, Capsule3d::new(0.07, s.tail).into(), &dark, t(0.0, 0.0, s.tail * 0.5).with_rotation(along_z));
    // Legs: thick upper, thinner lower, a paw with claws.
    for leg in QBone::LEGS {
        let (upper, lower) = if leg.front { (s.front_upper, s.front_lower) } else { (s.hind_upper, s.hind_lower) };
        let r = s.leg_radius;
        let upper_r = if leg.front { r * 1.5 } else { r * 1.8 };
        part(leg.upper, Capsule3d::new(upper_r, upper * 0.8).into(), &fur, t(0.0, -upper * 0.45, 0.0));
        part(leg.lower, Capsule3d::new(r, lower * 0.9).into(), &fur, t(0.0, -lower * 0.5, 0.0));
        part(leg.paw, Sphere::new(1.0).into(), &dark, t(0.0, -0.01, -0.03).with_scale(Vec3::new(r * 1.4, r * 0.9, r * 1.9)));
        part(leg.paw, Cuboid::new(r * 1.6, 0.02, 0.04).into(), &claws, t(0.0, -0.04, -0.03 - r * 1.7));
    }
    QuadrupedRig { model, bones, root_height }
}
