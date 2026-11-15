//! Primitive models for equipped shields and armour. Each builder adds parts under a root that
//! sits on the item's bone (see [`super::ItemModel`]). Bone frames: limbs hang along -Y, the
//! character faces -Z, its left side is -X.

use bevy::camera::visibility::NoFrustumCulling;
use bevy::prelude::*;
use std::f32::consts::FRAC_PI_2;

use crate::humanoid::HumanoidSpec;

pub struct ModelBuilder<'a, 'w, 's> {
    pub commands: &'a mut Commands<'w, 's>,
    pub meshes: &'a mut Assets<Mesh>,
    pub materials: &'a mut Assets<StandardMaterial>,
    pub root: Entity,
}

impl ModelBuilder<'_, '_, '_> {
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
