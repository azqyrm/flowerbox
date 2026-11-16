//! Primitive models for equipped shields and armour. Each builder adds parts under a root that
//! sits on the item's bone (see [`super::ItemModel`]). Bone frames: limbs hang along -Y, the
//! character faces -Z, its left side is -X.

use bevy::camera::visibility::NoFrustumCulling;
use bevy::prelude::*;
use std::f32::consts::FRAC_PI_2;

use crate::humanoid::HumanoidSpec;
use crate::textures::{self, Tex};

pub struct ModelBuilder<'a, 'w, 's> {
    pub commands: &'a mut Commands<'w, 's>,
    pub meshes: &'a mut Assets<Mesh>,
    pub materials: &'a mut Assets<StandardMaterial>,
    pub root: Entity,
    /// +1 on the right side, -1 for the mirrored copy of a pair (gauntlets, greaves): multiply
    /// sideways offsets (x) by it.
    pub side: f32,
}

impl ModelBuilder<'_, '_, '_> {
    /// A part with a procedural texture.
    pub fn textured(&mut self, mesh: impl Into<Mesh>, color: Color, tex: Tex, metallic: f32, roughness: f32, t: Transform) {
        let material = self.materials.add(StandardMaterial {
            base_color: color,
            base_color_texture: textures::get(tex),
            metallic,
            perceptual_roughness: roughness,
            ..default()
        });
        self.commands.spawn((Mesh3d(self.meshes.add(mesh.into())), MeshMaterial3d(material), t, NoFrustumCulling, ChildOf(self.root)));
    }

    pub fn part(&mut self, mesh: impl Into<Mesh>, color: Color, metallic: f32, roughness: f32, t: Transform) {
        let material = self.materials.add(StandardMaterial { base_color: color, metallic, perceptual_roughness: roughness, ..default() });
        // NoFrustumCulling: like the body parts (ROADMAP N13).
        self.commands.spawn((Mesh3d(self.meshes.add(mesh.into())), MeshMaterial3d(material), t, NoFrustumCulling, ChildOf(self.root)));
    }
}

const OAK: Color = Color::srgb(0.4, 0.26, 0.13);
const OAK_DARK: Color = Color::srgb(0.2, 0.12, 0.06);
const IRON: Color = Color::srgb(0.32, 0.32, 0.34);
const LEATHER: Color = Color::srgb(0.3, 0.19, 0.1);

/// A disc on the outside of the left forearm, facing outward (-X).
fn shield_disc(x: f32, y: f32) -> Transform {
    Transform::from_xyz(x, y, 0.0).with_rotation(Quat::from_rotation_z(FRAC_PI_2))
}

pub fn wooden_shield(b: &mut ModelBuilder, spec: &HumanoidSpec) {
    let y = -spec.forearm * 0.55;
    let face = -0.085;
    b.part(Cylinder::new(0.29, 0.02), IRON, 0.7, 0.5, shield_disc(-0.06, y));
    b.part(Cylinder::new(0.27, 0.035), OAK, 0.0, 0.85, shield_disc(-0.075, y));
    // Plank seams.
    for z in [-0.09, 0.0, 0.09] {
        let len = 2.0 * (0.27f32 * 0.27 - z * z).sqrt() - 0.02;
        b.part(Cuboid::new(0.004, len, 0.008), OAK_DARK, 0.0, 0.9, Transform::from_xyz(face - 0.008, y, z + 0.045));
    }
    b.part(Sphere::new(0.06), IRON, 0.8, 0.4, Transform::from_xyz(face - 0.01, y, 0.0).with_scale(Vec3::new(0.6, 1.0, 1.0)));
}

pub fn studded_buckler(b: &mut ModelBuilder, spec: &HumanoidSpec) {
    let y = -spec.forearm * 0.6;
    b.part(Cylinder::new(0.17, 0.025), IRON, 0.85, 0.35, shield_disc(-0.07, y));
    b.part(Sphere::new(0.05), IRON, 0.9, 0.3, Transform::from_xyz(-0.085, y, 0.0).with_scale(Vec3::new(0.7, 1.0, 1.0)));
    for i in 0..8 {
        let a = i as f32 / 8.0 * std::f32::consts::TAU;
        b.part(Sphere::new(0.013), Color::srgb(0.55, 0.45, 0.28), 0.9, 0.35, Transform::from_xyz(-0.085, y + a.cos() * 0.125, a.sin() * 0.125));
    }
}

pub fn leather_hood(b: &mut ModelBuilder, spec: &HumanoidSpec) {
    let hr = spec.head_radius;
    // Shell around the back and top of the head, open at the face.
    b.part(Sphere::new(hr * 1.18), LEATHER, 0.0, 0.9, Transform::from_xyz(0.0, hr * 1.05, hr * 0.3));
    // The hood's tip, slumped down the back.
    b.part(Sphere::new(hr * 0.55), LEATHER, 0.0, 0.9, Transform::from_xyz(0.0, hr * 1.3, hr * 1.2).with_scale(Vec3::new(1.0, 0.8, 1.2)));
    // Cowl around the neck and shoulders.
    b.part(Cylinder::new(hr * 1.3, 0.08), LEATHER, 0.0, 0.9, Transform::from_xyz(0.0, -0.02, hr * 0.15));
}

const MAIL: Color = Color::srgb(0.52, 0.52, 0.55);
const TABARD: Color = Color::srgb(0.42, 0.05, 0.06);

/// Mail over the torso, a red tabard, a belt and mail sleeves at the shoulders.
pub fn hauberk(b: &mut ModelBuilder, spec: &HumanoidSpec) {
    let t = spec.torso;
    b.textured(Cuboid::new(t.x * 1.08, t.y * 1.05, t.z * 1.15), MAIL, Tex::Chainmail, 0.8, 0.45, Transform::from_xyz(0.0, t.y * 0.5 + 0.02, 0.0));
    b.textured(Cuboid::new(t.x * 0.62, t.y * 1.25, 0.02), TABARD, Tex::Cloth, 0.0, 0.95, Transform::from_xyz(0.0, t.y * 0.42, -t.z * 0.6));
    b.textured(Cuboid::new(t.x * 0.62, t.y * 1.0, 0.02), TABARD, Tex::Cloth, 0.0, 0.95, Transform::from_xyz(0.0, t.y * 0.5, t.z * 0.6));
    b.part(Cuboid::new(t.x * 1.12, 0.05, t.z * 1.2), LEATHER, 0.0, 0.9, Transform::from_xyz(0.0, 0.04, 0.0));
    for x in [-1.0, 1.0] {
        b.textured(Sphere::new(t.z * 0.62), MAIL, Tex::Chainmail, 0.8, 0.45, Transform::from_xyz(x * spec.shoulder_x, spec.shoulder_height, 0.0));
    }
}

/// An iron cuff on the forearm with a flared rim and knuckle plates over the hand.
pub fn gauntlet(b: &mut ModelBuilder, spec: &HumanoidSpec) {
    let r = spec.arm_radius;
    let f = spec.forearm;
    b.textured(Cylinder::new(r * 1.45, f * 0.6), IRON, Tex::Rust, 0.75, 0.5, Transform::from_xyz(0.0, -f * 0.55, 0.0));
    b.textured(ConicalFrustum { radius_top: r * 1.9, radius_bottom: r * 1.45, height: 0.05 }, IRON, Tex::Rust, 0.75, 0.5, Transform::from_xyz(0.0, -f * 0.25, 0.0));
    b.part(Cuboid::new(r * 2.2, 0.05, r * 2.0), Color::srgb(0.2, 0.2, 0.22), 0.8, 0.45, Transform::from_xyz(0.0, -f - 0.02, -0.01));
    // A spike on the outside of the cuff.
    b.part(Cone { radius: 0.018, height: 0.07 }, IRON, 0.8, 0.4, Transform::from_xyz(b.side * r * 1.5, -f * 0.5, 0.0).with_rotation(Quat::from_rotation_z(-b.side * FRAC_PI_2)));
}

/// A shin plate with a knee cop.
pub fn greave(b: &mut ModelBuilder, spec: &HumanoidSpec) {
    let r = spec.leg_radius;
    let s = spec.shin;
    b.textured(Cylinder::new(r * 1.35, s * 0.75), IRON, Tex::Rust, 0.7, 0.5, Transform::from_xyz(0.0, -s * 0.5, -0.01));
    b.textured(Sphere::new(r * 1.4), IRON, Tex::Rust, 0.75, 0.45, Transform::from_xyz(0.0, 0.0, -r * 0.4).with_scale(Vec3::new(1.0, 0.8, 0.8)));
    b.part(Cuboid::new(r * 0.5, s * 0.6, 0.02), Color::srgb(0.55, 0.42, 0.22), 0.9, 0.35, Transform::from_xyz(b.side * 0.0, -s * 0.5, -r * 1.36));
}
