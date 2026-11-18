//! Level props placed by the level file (`props`): the forgery's **anvil**, **forge** (stone hearth,
//! fire pit, chimney hood, quench trough) and **sky grate** (an opening in the ceiling with iron
//! bars, daylight beyond, a sunbeam and dust glinting in it). The forge and its trough give off
//! **steam**: puffs that rise, swell, drift toward the nearest grate and fade.
//!
//! (Real light shafts need Bevy's volumetric fog, which only works with a directional light — that
//! would light the open arena too. Translucent slabs for the shaft looked like grey walls; the
//! dust motes in the beam read as sunlight instead.)

use avian3d::prelude::*;
use bevy::prelude::*;

use super::layout::{Layout, PropKind};
use crate::arena::{TorchAssets, spawn_fire};


pub struct PropsPlugin;

impl Plugin for PropsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, ((emit_steam, drift_steam).chain(), float_dust));
    }
}

/// Puffs of steam come out here every `every` seconds.
#[derive(Component)]
struct SteamVent {
    every: f32,
    timer: f32,
    n: u32,
}

#[derive(Component)]
struct SteamPuff {
    age: f32,
    life: f32,
    drift: Vec3,
    size: f32,
}

/// A speck of dust floating in a sunbeam: drifts slowly around `home`, glinting.
#[derive(Component)]
struct DustMote {
    home: Vec3,
    phase: f32,
}

/// Shared steam look: a few alpha steps (puffs swap material as they fade).
#[derive(Resource)]
struct SteamAssets {
    mesh: Handle<Mesh>,
    fades: Vec<Handle<StandardMaterial>>,
}

/// Where steam goes: the openings in the ceiling (x, z).
#[derive(Resource, Default)]
struct SteamExits(Vec<Vec2>);

pub fn spawn_props(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    torches: &TorchAssets,
    layout: &Layout,
) {
    let mut mat = |c: Color, metallic: f32, rough: f32| materials.add(StandardMaterial { base_color: c, metallic, perceptual_roughness: rough, ..default() });
    let iron = mat(Color::srgb(0.13, 0.13, 0.14), 0.85, 0.45);
    let dark_iron = mat(Color::srgb(0.06, 0.06, 0.065), 0.8, 0.6);
    let wood = mat(Color::srgb(0.28, 0.18, 0.1), 0.0, 0.9);
    let stone = mat(Color::srgb(0.3, 0.27, 0.24), 0.0, 0.95);
    let brick = mat(Color::srgb(0.32, 0.17, 0.12), 0.0, 0.95);
    let soot = mat(Color::srgb(0.05, 0.045, 0.04), 0.0, 1.0);
    let water = materials.add(StandardMaterial { base_color: Color::srgb(0.03, 0.05, 0.06), perceptual_roughness: 0.08, reflectance: 0.6, ..default() });
    let coals = materials.add(StandardMaterial {
        base_color: Color::srgb(0.6, 0.15, 0.03),
        emissive: LinearRgba::rgb(9.0, 1.8, 0.2),
        perceptual_roughness: 0.9,
        ..default()
    });
    let steam_mesh = meshes.add(Sphere::new(1.0).mesh().ico(2).unwrap());
    let fades = (1..=5)
        .map(|i| {
            materials.add(StandardMaterial {
                base_color: Color::srgba(0.85, 0.85, 0.88, 0.012 * i as f32),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })
        })
        .collect();
    commands.insert_resource(SteamAssets { mesh: steam_mesh, fades });
    let sky = materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.95, 0.85), emissive: LinearRgba::rgb(14.0, 12.5, 10.0), unlit: true, ..default() });
    let dust = materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.92, 0.75), emissive: LinearRgba::rgb(6.0, 5.2, 3.8), unlit: true, ..default() });
    let mote = meshes.add(Sphere::new(0.012).mesh().ico(1).unwrap());

    let gothic = super::gothic::GothicMats::new(materials);
    let library = super::library::LibraryMats::new(materials);
    let womb = super::womb::WombMats::new(materials);
    let mut exits = vec![];
    // Sunbeams to fill with dust once the props are placed: (top of the opening, sun direction,
    // size, height above the floor).
    let mut beams: Vec<(Vec3, Vec3, f32, f32, f32)> = vec![];
    for (n, prop) in layout.file.props.iter().enumerate() {
        let floor = layout.ground(prop.at);
        // Local frame: -Z = the way the prop faces.
        let frame = Transform::from_translation(floor).with_rotation(Quat::from_rotation_y(prop.facing.yaw()));
        let root = commands.spawn((Name::new(format!("Prop: {:?}", prop.kind)), frame, Visibility::default())).id();
        let mut part = |commands: &mut Commands, mesh: Mesh, m: &Handle<StandardMaterial>, t: Transform| {
            commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(m.clone()), t, ChildOf(root))).id()
        };
        let solid = |commands: &mut Commands, size: Vec3, at: Vec3| {
            commands.spawn((RigidBody::Static, Collider::cuboid(size.x, size.y, size.z), Transform::from_translation(at), ChildOf(root)));
        };
        let t = Transform::from_xyz;
        match prop.kind {
            PropKind::Anvil | PropKind::Forge | PropKind::SkyGrate => {}
            PropKind::Bookshelf
            | PropKind::Desk
            | PropKind::BookPile
            | PropKind::Water
            | PropKind::DartTrap
            | PropKind::Pendulum
            | PropKind::IllusoryWall
            | PropKind::Orrery => {
                super::library::spawn_library_prop(commands, meshes, &library, layout, prop, n, root, frame);
                continue;
            }
            PropKind::Membrane | PropKind::BilePool | PropKind::Sinews | PropKind::Ova | PropKind::FleshPillar | PropKind::Heart => {
                super::womb::spawn_womb_prop(commands, meshes, &womb, layout, prop, n, root, frame);
                continue;
            }
            _ => {
                super::gothic::spawn_gothic_prop(commands, meshes, materials, &gothic, layout, prop, root, frame);
                continue;
            }
        }
        match prop.kind {
            PropKind::Anvil => {
                // A tree-stump block, the anvil on it (horn forward), a hammer resting on top.
                part(commands, Cylinder::new(0.32, 0.5).into(), &wood, t(0.0, 0.25, 0.0));
                part(commands, Cuboid::new(0.22, 0.12, 0.42).into(), &iron, t(0.0, 0.56, 0.0));
                part(commands, Cuboid::new(0.14, 0.18, 0.3).into(), &iron, t(0.0, 0.71, 0.0));
                part(commands, Cuboid::new(0.3, 0.12, 0.62).into(), &iron, t(0.0, 0.86, 0.05));
                part(commands, ConicalFrustum { radius_top: 0.02, radius_bottom: 0.08, height: 0.32 }.into(), &iron, t(0.0, 0.88, -0.4).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)));
                part(commands, Cylinder::new(0.018, 0.42).into(), &wood, t(0.12, 0.94, 0.15).with_rotation(Quat::from_rotation_z(1.45)));
                part(commands, Cuboid::new(0.08, 0.07, 0.14).into(), &dark_iron, t(-0.08, 0.95, 0.15));
                solid(commands, Vec3::new(0.65, 0.95, 0.75), Vec3::new(0.0, 0.47, 0.0));
            }
            PropKind::Forge => {
                // Stone hearth with a sunken fire pit, a brick hood narrowing into a chimney, and a
                // quench trough beside it (to the right, local +X).
                part(commands, Cuboid::new(1.9, 0.9, 1.3).into(), &stone, t(0.0, 0.45, 0.0));
                part(commands, Cuboid::new(1.5, 0.06, 0.9).into(), &soot, t(0.0, 0.91, 0.0));
                for i in 0..14 {
                    let a = i as f32 * 2.399;
                    let r = 0.12 + 0.3 * ((i * 7 % 10) as f32 / 10.0);
                    part(commands, Sphere::new(0.07 + 0.03 * ((i % 3) as f32)).into(), &coals, t(a.cos() * r * 1.3, 0.95, a.sin() * r * 0.8));
                }
                part(commands, Cuboid::new(2.1, 0.5, 1.5).into(), &brick, t(0.0, 2.6, 0.1));
                part(commands, Cuboid::new(1.5, 0.6, 1.1).into(), &brick, t(0.0, 3.1, 0.2));
                let ceiling = layout.ceiling_at(floor.xz()) - floor.y;
                part(commands, Cuboid::new(0.9, ceiling - 3.4 + 0.3, 0.8).into(), &brick, t(0.0, (3.4 + ceiling + 0.3) * 0.5, 0.25));
                for x in [-0.95, 0.95] {
                    part(commands, Cuboid::new(0.15, 1.7, 0.15).into(), &brick, t(x, 1.75, -0.55));
                }
                part(commands, Cuboid::new(0.9, 0.55, 0.6).into(), &wood, t(1.65, 0.28, -0.1));
                part(commands, Cuboid::new(0.78, 0.02, 0.48).into(), &water, t(1.65, 0.5, -0.1));
                solid(commands, Vec3::new(1.9, 0.9, 1.3), Vec3::new(0.0, 0.45, 0.0));
                solid(commands, Vec3::new(0.9, 0.55, 0.6), Vec3::new(1.65, 0.28, -0.1));
                let world = |p: Vec3| frame.transform_point(p);
                let flames: Vec<Vec3> = [(-0.3, 0.0), (0.0, 0.12), (0.3, -0.05), (0.12, -0.2), (-0.15, 0.2)]
                    .iter()
                    .map(|&(x, z)| world(Vec3::new(x, 1.12, z)))
                    .collect();
                spawn_fire(commands, torches, world(Vec3::new(0.0, 1.6, -0.6)), &flames, 1.9, 300 + n as u32, 700_000.0, 14.0);
                for (at, every) in [(Vec3::new(0.0, 1.4, 0.0), 0.9), (Vec3::new(1.65, 0.6, -0.1), 1.6)] {
                    commands.spawn((Name::new("Steam Vent"), SteamVent { every, timer: 0.0, n: 0 }, Transform::from_translation(world(at))));
                }
            }
            _ => {}
        }
        if prop.kind == PropKind::SkyGrate {
            // Bars across the opening, daylight above, a sunbeam down through it.
            let (w, d) = prop.size;
            let y = layout.ceiling_at(floor.xz()) + 0.15 - floor.y;
            for i in 0..=((w / 0.32) as i32) {
                let x = -w * 0.5 + i as f32 * w / (w / 0.32).floor();
                part(commands, Cylinder::new(0.035, d).into(), &dark_iron, t(x, y, 0.0).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)));
            }
            for z in [-d * 0.25, d * 0.25] {
                part(commands, Cuboid::new(w, 0.08, 0.06).into(), &dark_iron, t(0.0, y + 0.05, z));
            }
            part(commands, Cuboid::new(w * 1.6, 0.05, d * 1.6).into(), &sky, t(0.0, y + 3.0, 0.0));
            let top = frame.transform_point(Vec3::new(0.0, y, 0.0));
            // The sun stands a little to the south-east: the beam slants.
            let sun_from = Vec3::new(0.35, 1.0, 0.25).normalize();
            commands.spawn((
                Name::new("Sunbeam"),
                SpotLight {
                    color: Color::srgb(1.0, 0.93, 0.8),
                    intensity: 9_000_000.0,
                    range: 40.0,
                    radius: 0.5,
                    inner_angle: 0.08,
                    outer_angle: 0.2,
                    shadow_maps_enabled: true,
                    ..default()
                },
                Transform::from_translation(top + sun_from * 14.0).looking_at(top - sun_from * 4.0, Vec3::Y),
            ));
            exits.push(Vec2::new(floor.x, floor.z));
            beams.push((top, sun_from, w, d, y));
        }
    }
    // Dust in each sunbeam: specks scattered through the slanted column below the opening.
    for (top, sun_from, w, d, y) in beams {
        for i in 0..90u32 {
            let h = |k: u32| (i.wrapping_mul(2_654_435_761).wrapping_add(k.wrapping_mul(97_531)) % 10_000) as f32 / 10_000.0;
            let depth = 0.4 + h(1) * (y - 0.6); // below the bars, above the floor
            let home = top - sun_from * (depth / sun_from.y) + Vec3::new((h(2) - 0.5) * w * 0.8, 0.0, (h(3) - 0.5) * d * 0.8);
            commands.spawn((
                Name::new("Dust"),
                DustMote { home, phase: h(4) * 100.0 },
                Mesh3d(mote.clone()),
                MeshMaterial3d(dust.clone()),
                Transform::from_translation(home),
                bevy::light::NotShadowCaster,
            ));
        }
    }
    // Several levels add up their openings (the last level mustn't replace the others').
    commands.queue(move |world: &mut World| world.get_resource_or_init::<SteamExits>().0.extend(exits));
}

fn emit_steam(time: Res<Time>, mut commands: Commands, assets: Option<Res<SteamAssets>>, mut vents: Query<(&mut SteamVent, &Transform)>) {
    let Some(assets) = assets else { return };
    for (mut vent, tf) in &mut vents {
        vent.timer -= time.delta_secs();
        if vent.timer > 0.0 {
            continue;
        }
        vent.timer = vent.every;
        vent.n += 1;
        let h = |k: u32| (vent.n.wrapping_mul(2_654_435_761).wrapping_add(k.wrapping_mul(40_503)) % 1000) as f32 / 1000.0;
        let drift = Vec3::new(h(1) - 0.5, 0.0, h(2) - 0.5) * 0.25;
        commands.spawn((
            Name::new("Steam"),
            SteamPuff { age: 0.0, life: 4.5 + 1.5 * h(3), drift, size: 0.12 + 0.08 * h(4) },
            Mesh3d(assets.mesh.clone()),
            MeshMaterial3d(assets.fades[assets.fades.len() - 1].clone()),
            Transform::from_translation(tf.translation + drift).with_scale(Vec3::splat(0.15)),
            bevy::light::NotShadowCaster,
        ));
    }
}

/// Puffs rise, swell and fade; near the ceiling they're drawn to the nearest opening and out.
#[allow(clippy::type_complexity)]
fn drift_steam(
    time: Res<Time>,
    mut commands: Commands,
    assets: Option<Res<SteamAssets>>,
    exits: Option<Res<SteamExits>>,
    mut puffs: Query<(Entity, &mut SteamPuff, &mut Transform, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    let (Some(assets), Some(exits)) = (assets, exits) else { return };
    let dt = time.delta_secs();
    for (e, mut puff, mut tf, mut mat) in &mut puffs {
        puff.age += dt;
        let k = puff.age / puff.life;
        if k >= 1.0 {
            commands.entity(e).despawn();
            continue;
        }
        let here = Vec2::new(tf.translation.x, tf.translation.z);
        let pull = exits
            .0
            .iter()
            .min_by(|a, b| a.distance(here).total_cmp(&b.distance(here)))
            .map_or(Vec2::ZERO, |exit| (*exit - here).clamp_length_max(1.0) * 0.5);
        tf.translation += (Vec3::Y * 0.75 + puff.drift + Vec3::new(pull.x, 0.0, pull.y) * k) * dt;
        tf.scale = Vec3::splat(puff.size * (1.0 + 2.5 * k));
        // Fade in quickly, out slowly: pick the alpha step.
        let alpha = (k / 0.1).min(1.0) * (1.0 - k);
        let step = ((alpha * assets.fades.len() as f32).ceil() as usize).clamp(1, assets.fades.len()) - 1;
        if mat.0 != assets.fades[step] {
            mat.0 = assets.fades[step].clone();
        }
    }
}

/// Dust drifts in slow loops and glints (grows and shrinks).
fn float_dust(time: Res<Time>, mut motes: Query<(&DustMote, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (m, mut tf) in &mut motes {
        let p = m.phase;
        tf.translation = m.home + Vec3::new((t * 0.13 + p).sin() * 0.25, (t * 0.07 + p * 1.3).sin() * 0.3, (t * 0.11 + p * 0.7).cos() * 0.25);
        tf.scale = Vec3::splat(0.6 + 0.8 * ((t * 0.9 + p * 2.1).sin() * 0.5 + 0.5).powi(3));
    }
}
