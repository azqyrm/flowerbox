//! The cathedral's furniture (level props, see `layout::PropKind`): columns, stained-glass
//! windows, pews, the altar, a faceless weeping statue, bells, an inscribed lectern, bone heaps,
//! candles, chandeliers, sarcophagi, the cloister fountain and the boss's fog gate. Built from
//! primitives with the procedural textures (`crate::textures`).
//!
//! Bells and plaques are interactive (`super::riddle`); everything here only builds them.

use avian3d::prelude::*;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use super::layout::{Layout, PropDef, PropKind};
use super::riddle::{Bell, BellBody, Plaque};
use crate::textures::{self, Tex};
use crate::tuning::*;

/// Materials shared by every gothic prop.
pub struct GothicMats {
    stone: Handle<StandardMaterial>,
    pale: Handle<StandardMaterial>,
    marble: Handle<StandardMaterial>,
    wood: Handle<StandardMaterial>,
    iron: Handle<StandardMaterial>,
    bronze: Handle<StandardMaterial>,
    bone: Handle<StandardMaterial>,
    cloth: Handle<StandardMaterial>,
    wax: Handle<StandardMaterial>,
    flame: Handle<StandardMaterial>,
    tears: Handle<StandardMaterial>,
    glass: Handle<StandardMaterial>,
    water: Handle<StandardMaterial>,
    runes: Handle<StandardMaterial>,
}

impl GothicMats {
    pub fn new(materials: &mut Assets<StandardMaterial>) -> Self {
        let mut tex = |c: Color, t: Tex, rough: f32, metal: f32| {
            materials.add(StandardMaterial { base_color: c, base_color_texture: textures::get(t), perceptual_roughness: rough, metallic: metal, ..default() })
        };
        let stone = tex(Color::srgb(0.32, 0.31, 0.34), Tex::Bricks, 0.95, 0.0);
        let pale = tex(Color::srgb(0.62, 0.6, 0.58), Tex::Marble, 0.6, 0.0);
        let marble = tex(Color::srgb(0.42, 0.4, 0.42), Tex::Marble, 0.5, 0.0);
        let wood = tex(Color::srgb(0.28, 0.17, 0.1), Tex::Leather, 0.85, 0.0);
        let iron = tex(Color::srgb(0.2, 0.2, 0.22), Tex::Rust, 0.55, 0.8);
        let bronze = tex(Color::srgb(0.55, 0.38, 0.18), Tex::Rust, 0.4, 0.9);
        let bone = tex(Color::srgb(0.85, 0.8, 0.7), Tex::Bone, 0.8, 0.0);
        let cloth = tex(Color::srgb(0.35, 0.04, 0.06), Tex::Cloth, 0.95, 0.0);
        let wax = materials.add(StandardMaterial { base_color: Color::srgb(0.9, 0.86, 0.75), perceptual_roughness: 0.7, ..default() });
        let flame = materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.7, 0.3), emissive: LinearRgba::rgb(18.0, 8.0, 2.0), unlit: true, ..default() });
        let tears = materials.add(StandardMaterial { base_color: Color::BLACK, emissive: LinearRgba::rgb(2.0, 4.0, 9.0), ..default() });
        let glass = materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: textures::get(Tex::StainedGlass),
            emissive: LinearRgba::rgb(3.0, 3.0, 3.0),
            emissive_texture: textures::get(Tex::StainedGlass),
            unlit: true,
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        let water = materials.add(StandardMaterial { base_color: Color::srgb(0.03, 0.05, 0.07), perceptual_roughness: 0.05, reflectance: 0.7, ..default() });
        let runes = materials.add(StandardMaterial {
            base_color: Color::srgb(0.1, 0.1, 0.11),
            emissive: LinearRgba::rgb(4.0, 2.0, 0.8),
            emissive_texture: textures::get(Tex::Runes),
            ..default()
        });
        Self { stone, pale, marble, wood, iron, bronze, bone, cloth, wax, flame, tears, glass, water, runes }
    }
}

/// Builds one cathedral prop at `prop` under `root` (already placed on the floor, -Z = its facing).
#[allow(clippy::too_many_arguments)]
pub fn spawn_gothic_prop(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    m: &GothicMats,
    layout: &Layout,
    prop: &PropDef,
    root: Entity,
    frame: Transform,
) {
    let floor = frame.translation;
    let height = layout.ceiling_at(floor.xz()) - floor.y;
    let mut part = |commands: &mut Commands, mesh: Mesh, mat: &Handle<StandardMaterial>, t: Transform| {
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat.clone()), t, ChildOf(root))).id()
    };
    let solid = |commands: &mut Commands, collider: Collider, at: Vec3| {
        commands.spawn((RigidBody::Static, collider, Transform::from_translation(at), ChildOf(root)));
    };
    let candle_light = |commands: &mut Commands, at: Vec3, intensity: f32, range: f32| {
        commands.spawn((
            PointLight { color: Color::srgb(1.0, 0.72, 0.4), intensity, range, radius: 0.1, ..default() },
            Transform::from_translation(at),
            ChildOf(root),
        ));
    };
    let t = Transform::from_xyz;
    match prop.kind {
        PropKind::Column => {
            let r = if prop.size.0 > 0.0 { prop.size.0 } else { 0.55 };
            part(commands, Cylinder::new(r, height).into(), &m.marble, t(0.0, height * 0.5, 0.0));
            // Four engaged shafts, a moulded base and a carved capital.
            for k in 0..4 {
                let a = k as f32 * FRAC_PI_2 + PI * 0.25;
                part(commands, Cylinder::new(r * 0.28, height).into(), &m.marble, t(a.cos() * r * 0.95, height * 0.5, a.sin() * r * 0.95));
            }
            part(commands, Cylinder::new(r * 1.45, 0.35).into(), &m.stone, t(0.0, 0.175, 0.0));
            part(commands, Cylinder::new(r * 1.3, 0.25).into(), &m.stone, t(0.0, 0.47, 0.0));
            part(commands, ConicalFrustum { radius_top: r * 1.55, radius_bottom: r * 1.05, height: 0.6 }.into(), &m.stone, t(0.0, height - 0.9, 0.0));
            part(commands, Cuboid::new(r * 3.2, 0.25, r * 3.2).into(), &m.stone, t(0.0, height - 0.48, 0.0));
            solid(commands, Collider::cylinder(r * 1.15, height), Vec3::Y * height * 0.5);
        }
        PropKind::Window => {
            // Sits on the wall behind it: find that wall and stand the glass against it.
            let (w, h) = (prop.size.0.max(1.0), prop.size.1.max(2.0));
            let back = -prop.facing.vec();
            let p = layout.world(prop.at);
            let dist = layout.wall_distance(p, prop.facing.opposite()).unwrap_or(0.5);
            let z = dist - LEVEL_WALL_THICKNESS * 0.5 - 0.04;
            let _ = back;
            let sill = 2.2;
            part(commands, Rectangle::new(w, h).into(), &m.glass, t(0.0, sill + h * 0.5, z).with_rotation(Quat::from_rotation_y(0.0)));
            // A pointed arch: two leaning panes above, stone mullions and tracery.
            for side in [-1.0f32, 1.0] {
                let lean = side * 0.55;
                part(commands, Rectangle::new(w * 0.56, w * 0.62).into(), &m.glass, t(side * w * 0.23, sill + h + w * 0.2, z).with_rotation(Quat::from_rotation_z(-lean)));
                part(commands, Cuboid::new(0.18, w * 0.8, 0.2).into(), &m.stone, t(side * w * 0.27, sill + h + w * 0.25, z - 0.05).with_rotation(Quat::from_rotation_z(-lean)));
                part(commands, Cuboid::new(0.18, h, 0.22).into(), &m.stone, t(side * (w * 0.5 + 0.05), sill + h * 0.5, z - 0.05));
            }
            part(commands, Cuboid::new(0.08, h, 0.12).into(), &m.stone, t(0.0, sill + h * 0.5, z - 0.03));
            part(commands, Cuboid::new(w + 0.3, 0.2, 0.4).into(), &m.stone, t(0.0, sill - 0.1, z - 0.12));
            // Coloured moonlight falling in.
            commands.spawn((
                PointLight { color: Color::srgb(0.55, 0.5, 0.9), intensity: 220_000.0, range: 12.0, radius: w * 0.4, ..default() },
                Transform::from_xyz(0.0, sill + h * 0.6, z - 1.2),
                ChildOf(root),
            ));
        }
        PropKind::Pews => {
            let rows = (prop.size.0.max(1.0)) as i32;
            let len = prop.size.1.max(1.5);
            for k in 0..rows {
                let z = k as f32 * 1.15;
                part(commands, Cuboid::new(len, 0.08, 0.45).into(), &m.wood, t(0.0, 0.46, z));
                part(commands, Cuboid::new(len, 0.55, 0.06).into(), &m.wood, t(0.0, 0.78, z + 0.22));
                for x in [-len * 0.5 + 0.05, len * 0.5 - 0.05] {
                    part(commands, Cuboid::new(0.08, 0.95, 0.5).into(), &m.wood, t(x, 0.47, z + 0.02));
                }
                solid(commands, Collider::cuboid(len, 0.95, 0.55), Vec3::new(0.0, 0.47, z + 0.02));
            }
        }
        PropKind::Altar => {
            part(commands, Cuboid::new(2.4, 1.0, 1.1).into(), &m.pale, t(0.0, 0.5, 0.0));
            part(commands, Cuboid::new(2.6, 0.12, 1.25).into(), &m.marble, t(0.0, 1.06, 0.0));
            part(commands, Cuboid::new(0.7, 0.02, 1.27).into(), &m.cloth, t(0.0, 1.13, 0.0));
            part(commands, Cuboid::new(0.7, 0.8, 0.02).into(), &m.cloth, t(0.0, 0.72, -0.64));
            // An open book and a skull among the candles.
            part(commands, Cuboid::new(0.42, 0.05, 0.3).into(), &m.bone, t(-0.5, 1.15, 0.0).with_rotation(Quat::from_rotation_y(0.2)));
            part(commands, Sphere::new(0.11).into(), &m.bone, t(0.6, 1.24, 0.1));
            for (x, h) in [(-1.05, 0.35), (-0.85, 0.22), (0.95, 0.3), (1.1, 0.42), (0.2, 0.18)] {
                part(commands, Cylinder::new(0.035, h).into(), &m.wax, t(x, 1.12 + h * 0.5, 0.25));
                part(commands, Sphere::new(0.025).into(), &m.flame, t(x, 1.12 + h + 0.03, 0.25));
            }
            candle_light(commands, Vec3::new(0.0, 1.8, -0.4), 60_000.0, 7.0);
            solid(commands, Collider::cuboid(2.6, 1.2, 1.25), Vec3::Y * 0.6);
        }
        PropKind::Statue => {
            let s = if prop.size.0 > 0.0 { prop.size.0 } else { 1.0 };
            part(commands, Cuboid::new(1.2 * s, 1.0, 1.2 * s).into(), &m.stone, t(0.0, 0.5, 0.0));
            // A robed figure, hood drawn, hands clasped — and no face, only glowing tears.
            part(commands, Cone { radius: 0.5 * s, height: 2.0 * s }.into(), &m.pale, t(0.0, 1.0 + s, 0.0));
            part(commands, Sphere::new(0.2 * s).into(), &m.pale, t(0.0, 1.0 + 2.05 * s, -0.02));
            part(commands, Cone { radius: 0.26 * s, height: 0.55 * s }.into(), &m.pale, t(0.0, 1.0 + 2.25 * s, 0.04));
            part(commands, Cuboid::new(0.18 * s, 0.3 * s, 0.14 * s).into(), &m.pale, t(0.0, 1.0 + 1.35 * s, -0.32 * s).with_rotation(Quat::from_rotation_x(0.4)));
            for x in [-0.07, 0.07] {
                part(commands, Cuboid::new(0.02 * s, 0.4 * s, 0.01).into(), &m.tears, t(x * s, 1.0 + 1.88 * s, -0.19 * s));
            }
            solid(commands, Collider::cuboid(1.2 * s, 1.0 + 2.4 * s, 1.2 * s), Vec3::Y * (0.5 + 1.2 * s));
        }
        PropKind::Bell => {
            // Two posts and a beam; the bell hangs from a pivot under the beam and swings when rung.
            for x in [-0.95, 0.95] {
                part(commands, Cuboid::new(0.16, 3.0, 0.16).into(), &m.wood, t(x, 1.5, 0.0));
                solid(commands, Collider::cuboid(0.2, 3.0, 0.2), Vec3::new(x, 1.5, 0.0));
            }
            part(commands, Cuboid::new(2.2, 0.18, 0.2).into(), &m.wood, t(0.0, 3.0, 0.0));
            let pivot = commands.spawn((BellBody { swing: 0.0 }, Transform::from_xyz(0.0, 2.9, 0.0), Visibility::default(), ChildOf(root))).id();
            let mut bell_part = |commands: &mut Commands, mesh: Mesh, mat: &Handle<StandardMaterial>, t: Transform| {
                commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat.clone()), t, ChildOf(pivot)));
            };
            bell_part(commands, ConicalFrustum { radius_top: 0.18, radius_bottom: 0.48, height: 0.7 }.into(), &m.bronze, t(0.0, -0.45, 0.0));
            bell_part(commands, Torus::new(0.42, 0.52).into(), &m.bronze, t(0.0, -0.8, 0.0));
            bell_part(commands, Sphere::new(0.2).into(), &m.bronze, t(0.0, -0.1, 0.0));
            bell_part(commands, Sphere::new(0.08).into(), &m.iron, t(0.0, -0.85, 0.0));
            bell_part(commands, Cylinder::new(0.08, 0.05).into(), &m.runes, t(0.0, -0.5, -0.33).with_rotation(Quat::from_rotation_x(FRAC_PI_2 - 0.3)));
            commands.entity(root).insert(Bell { id: prop.id, name: prop.text.clone(), pivot });
        }
        PropKind::Plaque => {
            part(commands, Cuboid::new(0.5, 1.0, 0.4).into(), &m.stone, t(0.0, 0.5, 0.0));
            part(commands, Cuboid::new(0.8, 0.08, 0.6).into(), &m.marble, t(0.0, 1.05, 0.0).with_rotation(Quat::from_rotation_x(-0.45)));
            part(commands, Cuboid::new(0.66, 0.02, 0.46).into(), &m.runes, t(0.0, 1.1, -0.02).with_rotation(Quat::from_rotation_x(-0.45)));
            candle_light(commands, Vec3::new(0.0, 1.6, -0.6), 25_000.0, 4.0);
            solid(commands, Collider::cuboid(0.8, 1.2, 0.6), Vec3::Y * 0.6);
            commands.entity(root).insert(Plaque { text: prop.text.clone() });
        }
        PropKind::Bones => {
            for i in 0..26u32 {
                let h = |k: u32| (i.wrapping_mul(2_654_435_761).wrapping_add(k * 40_503) % 1000) as f32 / 1000.0;
                let a = h(1) * TAU;
                let r = h(2).sqrt() * 1.1;
                let y = (1.0 - r / 1.1) * 0.55;
                let at = Vec3::new(a.cos() * r, y + 0.08, a.sin() * r);
                if i % 3 == 0 {
                    part(commands, Sphere::new(0.12).into(), &m.bone, Transform::from_translation(at).with_scale(Vec3::new(1.0, 0.9, 1.15)));
                } else {
                    part(commands, Capsule3d::new(0.035, 0.42).into(), &m.bone, Transform::from_translation(at).with_rotation(Quat::from_euler(EulerRot::XYZ, h(3) * PI, h(4) * PI, FRAC_PI_2)));
                }
            }
        }
        PropKind::Candles => {
            for i in 0..9u32 {
                let a = i as f32 * 2.4;
                let r = 0.15 + 0.08 * (i % 4) as f32;
                let h = 0.12 + 0.07 * ((i * 5) % 7) as f32;
                part(commands, Cylinder::new(0.03 + 0.008 * (i % 3) as f32, h).into(), &m.wax, t(a.cos() * r, h * 0.5, a.sin() * r));
                part(commands, Sphere::new(0.022).into(), &m.flame, t(a.cos() * r, h + 0.03, a.sin() * r));
            }
            candle_light(commands, Vec3::new(0.0, 0.6, 0.0), 30_000.0, 5.0);
        }
        PropKind::Chandelier => {
            let y = (height - 2.6).max(2.6);
            part(commands, Torus::new(0.9, 1.0).into(), &m.iron, t(0.0, y, 0.0));
            for k in 0..3 {
                let a = k as f32 * TAU / 3.0;
                let from = Vec3::new(a.cos() * 0.95, y, a.sin() * 0.95);
                let to = Vec3::new(0.0, height, 0.0);
                let mid = (from + to) * 0.5;
                let len = from.distance(to);
                part(commands, Cylinder::new(0.012, len).into(), &m.iron, Transform::from_translation(mid).with_rotation(Quat::from_rotation_arc(Vec3::Y, (to - from).normalize())));
            }
            for k in 0..10 {
                let a = k as f32 * TAU / 10.0;
                part(commands, Cylinder::new(0.03, 0.22).into(), &m.wax, t(a.cos() * 0.95, y + 0.15, a.sin() * 0.95));
                part(commands, Sphere::new(0.025).into(), &m.flame, t(a.cos() * 0.95, y + 0.29, a.sin() * 0.95));
            }
            candle_light(commands, Vec3::new(0.0, y - 0.2, 0.0), 420_000.0, 17.0);
        }
        PropKind::Sarcophagus => {
            part(commands, Cuboid::new(1.0, 0.85, 2.3).into(), &m.stone, t(0.0, 0.425, 0.0));
            part(commands, Cuboid::new(1.1, 0.14, 2.4).into(), &m.marble, t(0.0, 0.92, 0.0));
            // The effigy on the lid: a robed body, hands folded, a sword along it.
            part(commands, Capsule3d::new(0.2, 1.3).into(), &m.pale, t(0.0, 1.12, 0.05).with_rotation(Quat::from_rotation_x(FRAC_PI_2)));
            part(commands, Sphere::new(0.15).into(), &m.pale, t(0.0, 1.13, -0.85));
            part(commands, Cuboid::new(0.05, 0.03, 1.3).into(), &m.iron, t(0.0, 1.35, 0.05));
            solid(commands, Collider::cuboid(1.1, 1.3, 2.4), Vec3::Y * 0.65);
        }
        PropKind::Fountain => {
            part(commands, Cylinder::new(1.8, 0.65).into(), &m.stone, t(0.0, 0.325, 0.0));
            part(commands, Cylinder::new(1.6, 0.04).into(), &m.water, t(0.0, 0.62, 0.0));
            part(commands, Cylinder::new(0.25, 1.6).into(), &m.pale, t(0.0, 1.3, 0.0));
            part(commands, Cylinder::new(0.65, 0.12).into(), &m.pale, t(0.0, 2.1, 0.0));
            // A drowned figure kneels in the basin, arms raised.
            part(commands, Cone { radius: 0.35, height: 1.1 }.into(), &m.pale, t(0.9, 1.1, 0.0));
            part(commands, Sphere::new(0.16).into(), &m.pale, t(0.9, 1.75, 0.0));
            for x in [0.72, 1.08] {
                part(commands, Capsule3d::new(0.05, 0.55).into(), &m.pale, t(x, 1.95, 0.0).with_rotation(Quat::from_rotation_z((x - 0.9) * -2.0)));
            }
            solid(commands, Collider::cylinder(1.8, 0.65), Vec3::Y * 0.325);
        }
        PropKind::FogGate => {
            let (w, h) = (prop.size.0.max(1.0), prop.size.1.max(2.0));
            let into = prop.facing.vec();
            let boss = prop.boss.unwrap_or(crate::enemy::EnemyKind::Saint);
            crate::enemy::boss::spawn_fog_gate(commands, meshes, materials, boss, floor, Vec3::new(into.x, 0.0, into.y), w, h);
        }
        PropKind::Anvil | PropKind::Forge | PropKind::SkyGrate => {}
    }
    let _ = NotShadowCaster;
}
