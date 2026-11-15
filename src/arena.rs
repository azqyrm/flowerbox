//! The circular dungeon arena: floor, ring wall, pillars, rubble, torches, moonlight, fog.

use std::f32::consts::TAU;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::tuning::*;

pub struct ArenaPlugin;

impl Plugin for ArenaPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.01, 0.012, 0.02)))
            .insert_resource(GlobalAmbientLight {
                color: Color::srgb(0.5, 0.55, 0.8),
                brightness: AMBIENT_BRIGHTNESS,
                ..default()
            })
            .init_resource::<TorchAssets>()
            .init_resource::<ArenaOpenings>()
            .add_systems(Startup, spawn_arena)
            .add_systems(Update, (flicker_torches, limit_torch_shadows));
    }
}

/// A wall torch. Its light flickers around `base_intensity`.
#[derive(Component)]
pub struct Torch {
    base_intensity: f32,
    phase: f32,
}

/// The visible flame mesh of a torch (wobbles with the flicker).
#[derive(Component)]
pub struct TorchFlame {
    phase: f32,
}

/// Gaps in the ring wall (x, z) where a level joins the arena: ring segments whose middle lies
/// within `half_width` of `centre` are left out (`crate::level` fills them with its own wall).
#[derive(Resource, Default, Debug)]
pub struct ArenaOpenings(pub Vec<(Vec2, f32)>);

/// Shared torch meshes and materials (the arena's and the levels' torches).
#[derive(Resource)]
pub struct TorchAssets {
    bracket: Handle<Mesh>,
    flame: Handle<Mesh>,
    iron: Handle<StandardMaterial>,
    fire: Handle<StandardMaterial>,
}

impl FromWorld for TorchAssets {
    fn from_world(world: &mut World) -> Self {
        let (bracket, flame) = {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            (meshes.add(Cuboid::new(0.15, 0.5, 0.35)), meshes.add(Sphere::new(0.13).mesh().ico(2).unwrap()))
        };
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let iron = materials.add(StandardMaterial {
            base_color: Color::srgb(0.08, 0.08, 0.08),
            metallic: 0.8,
            perceptual_roughness: 0.5,
            ..default()
        });
        let fire = materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.6, 0.2),
            emissive: LinearRgba::rgb(18.0, 6.0, 1.2),
            unlit: true,
            ..default()
        });
        Self { bracket, flame, iron, fire }
    }
}

/// A wall torch: the bracket at `wall_point` (on the wall face, at torch height), `inward` pointing
/// away from the wall into the room. `seed` varies the flicker.
pub fn spawn_torch(commands: &mut Commands, assets: &TorchAssets, wall_point: Vec3, inward: Vec3, seed: u32, intensity: f32, range: f32) {
    let phase = hash01(seed + 500) * 100.0;
    let (tr, tg, tb) = TORCH_COLOR;
    let bracket_pos = wall_point + inward * 0.15;
    commands.spawn((
        Name::new("Torch Bracket"),
        Mesh3d(assets.bracket.clone()),
        MeshMaterial3d(assets.iron.clone()),
        Transform::from_translation(bracket_pos).looking_to(inward, Vec3::Y),
    ));
    commands.spawn((
        Name::new("Torch Flame"),
        TorchFlame { phase },
        Mesh3d(assets.flame.clone()),
        MeshMaterial3d(assets.fire.clone()),
        Transform::from_translation(bracket_pos + Vec3::Y * 0.35 + inward * 0.15).with_scale(Vec3::new(1.0, 1.6, 1.0)),
    ));
    commands.spawn((
        Name::new("Torch Light"),
        Torch { base_intensity: intensity, phase },
        PointLight {
            color: Color::srgb(tr, tg, tb),
            intensity,
            range,
            radius: 0.1,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_translation(bracket_pos + Vec3::Y * 0.45 + inward * 0.9),
    ));
}

/// Cheap deterministic pseudo-random value in [0, 1) so the layout is stable between runs.
fn hash01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0x00FF_FFFF) as f32 / 0x0100_0000 as f32
}

fn spawn_arena(
    mut commands: Commands,
    torch_assets: Res<TorchAssets>,
    openings: Res<ArenaOpenings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let stone = |c: Color| StandardMaterial {
        base_color: c,
        perceptual_roughness: 0.92,
        reflectance: 0.2,
        ..default()
    };
    let floor_mat = materials.add(stone(Color::srgb(0.20, 0.19, 0.18)));
    let inlay_mat = materials.add(stone(Color::srgb(0.13, 0.12, 0.12)));
    let wall_mats = [
        materials.add(stone(Color::srgb(0.24, 0.23, 0.22))),
        materials.add(stone(Color::srgb(0.21, 0.20, 0.20))),
    ];
    let pillar_mat = materials.add(stone(Color::srgb(0.28, 0.27, 0.25)));

    // Floor
    commands.spawn((
        Name::new("Floor"),
        Mesh3d(meshes.add(Cylinder::new(ARENA_RADIUS + WALL_THICKNESS, FLOOR_THICKNESS).mesh().resolution(96))),
        MeshMaterial3d(floor_mat),
        Transform::from_xyz(0.0, -FLOOR_THICKNESS * 0.5, 0.0),
        RigidBody::Static,
        Collider::cylinder(ARENA_RADIUS + WALL_THICKNESS, FLOOR_THICKNESS),
    ));
    // Decorative inlaid rings (visual only, no collider)
    for (r, w) in [(3.0, 0.25), (9.0, 0.2)] {
        commands.spawn((
            Name::new("Floor Inlay"),
            Mesh3d(meshes.add(Annulus::new(r - w, r).mesh().resolution(96))),
            MeshMaterial3d(inlay_mat.clone()),
            Transform::from_xyz(0.0, 0.005, 0.0)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        ));
    }

    // Ring wall built from box segments facing the center
    let wall_r = ARENA_RADIUS + WALL_THICKNESS * 0.5;
    let seg_w = TAU * wall_r / WALL_SEGMENTS as f32 + 0.15; // slight overlap hides seams
    let seg_mesh = meshes.add(Cuboid::new(seg_w, WALL_HEIGHT, WALL_THICKNESS));
    for i in 0..WALL_SEGMENTS {
        let a = i as f32 / WALL_SEGMENTS as f32 * TAU;
        let pos = Vec3::new(a.cos() * wall_r, WALL_HEIGHT * 0.5, a.sin() * wall_r);
        let inner = Vec2::new(a.cos(), a.sin()) * ARENA_RADIUS;
        if openings.0.iter().any(|(c, half)| inner.distance(*c) < *half) {
            continue; // a level's doorway
        }
        commands.spawn((
            Name::new("Wall Segment"),
            Mesh3d(seg_mesh.clone()),
            MeshMaterial3d(wall_mats[i % 2].clone()),
            Transform::from_translation(pos).looking_at(Vec3::new(0.0, pos.y, 0.0), Vec3::Y),
            RigidBody::Static,
            Collider::cuboid(seg_w, WALL_HEIGHT, WALL_THICKNESS),
        ));
    }

    // Pillars with a base and capital
    let pillar_mesh = meshes.add(Cylinder::new(PILLAR_RADIUS, WALL_HEIGHT).mesh().resolution(24));
    let cap_mesh = meshes.add(Cuboid::new(PILLAR_RADIUS * 2.6, 0.4, PILLAR_RADIUS * 2.6));
    for i in 0..PILLAR_COUNT {
        let a = i as f32 / PILLAR_COUNT as f32 * TAU;
        let base = Vec3::new(a.cos() * PILLAR_RING, 0.0, a.sin() * PILLAR_RING);
        commands
            .spawn((
                Name::new("Pillar"),
                Mesh3d(pillar_mesh.clone()),
                MeshMaterial3d(pillar_mat.clone()),
                Transform::from_translation(base + Vec3::Y * WALL_HEIGHT * 0.5),
                RigidBody::Static,
                Collider::cylinder(PILLAR_RADIUS, WALL_HEIGHT),
            ))
            .with_children(|p| {
                for y in [-WALL_HEIGHT * 0.5 + 0.2, WALL_HEIGHT * 0.5 - 0.2] {
                    p.spawn((
                        Mesh3d(cap_mesh.clone()),
                        MeshMaterial3d(pillar_mat.clone()),
                        Transform::from_xyz(0.0, y, 0.0),
                    ));
                }
            });
    }

    // Scattered rubble (deterministic)
    for i in 0..14u32 {
        let a = hash01(i * 3) * TAU;
        let r = 4.0 + hash01(i * 3 + 1) * (ARENA_RADIUS - 5.0);
        let s = 0.25 + hash01(i * 3 + 2) * 0.55;
        let size = Vec3::new(s * 1.3, s * 0.7, s);
        commands.spawn((
            Name::new("Rubble"),
            Mesh3d(meshes.add(Cuboid::from_size(size))),
            MeshMaterial3d(wall_mats[(i % 2) as usize].clone()),
            Transform::from_xyz(a.cos() * r, size.y * 0.4, a.sin() * r).with_rotation(
                Quat::from_euler(EulerRot::YXZ, hash01(i + 100) * TAU, 0.0, hash01(i + 200) * 0.3),
            ),
            RigidBody::Static,
            Collider::cuboid(size.x, size.y, size.z),
        ));
    }

    // Wall torches, placed between the pillars
    for i in 0..PILLAR_COUNT {
        let a = (i as f32 + 0.5) / PILLAR_COUNT as f32 * TAU;
        let dir = Vec3::new(a.cos(), 0.0, a.sin());
        spawn_torch(&mut commands, &torch_assets, dir * ARENA_RADIUS + Vec3::Y * TORCH_HEIGHT, -dir, i as u32, TORCH_INTENSITY, TORCH_RANGE);
    }

    // Cold moonlight falling through the open ceiling
    let (mr, mg, mb) = MOON_COLOR;
    commands.spawn((
        Name::new("Moonlight"),
        SpotLight {
            color: Color::srgb(mr, mg, mb),
            intensity: MOON_INTENSITY,
            range: 60.0,
            radius: 1.0,
            inner_angle: 0.15,
            outer_angle: 0.45,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-6.0, 30.0, -4.0).looking_at(Vec3::new(1.0, 0.0, 1.0), Vec3::Y),
    ));
}

/// Flickers torch lights and wobbles their flame meshes with layered sines.
fn flicker_torches(
    time: Res<Time>,
    mut lights: Query<(&Torch, &mut PointLight)>,
    mut flames: Query<(&TorchFlame, &mut Transform)>,
) {
    let t = time.elapsed_secs();
    let flicker = |p: f32| {
        1.0 + 0.10 * (t * 7.3 + p).sin() + 0.06 * (t * 13.7 + p * 1.7).sin() + 0.04 * (t * 23.1 + p * 2.3).sin()
    };
    for (torch, mut light) in &mut lights {
        light.intensity = torch.base_intensity * flicker(torch.phase);
    }
    for (flame, mut tf) in &mut flames {
        let f = flicker(flame.phase);
        tf.scale = Vec3::new(1.0, 1.6 * f, 1.0) * (0.9 + 0.1 * f);
    }
}

/// Shadows are costly (a point light renders the scene six times; it's the draw calls, a smaller
/// shadow map didn't help): only the torches nearest the camera cast them (`TORCH_SHADOW_LIMIT`,
/// `FLOWERBOX_TORCH_SHADOWS=n` overrides for measuring). Far torches barely reach the camera's
/// surroundings anyway.
fn limit_torch_shadows(
    mut limit: Local<Option<usize>>,
    cams: Query<&GlobalTransform, With<Camera3d>>, mut lights: Query<(&GlobalTransform, &mut PointLight), With<Torch>>) {
    let limit = limit.get_or_insert_with(|| {
        std::env::var("FLOWERBOX_TORCH_SHADOWS").ok().and_then(|v| v.parse().ok()).unwrap_or(TORCH_SHADOW_LIMIT)
    });
    let Ok(cam) = cams.single() else { return };
    let eye = cam.translation();
    let mut by_distance: Vec<(f32, Mut<PointLight>)> =
        lights.iter_mut().map(|(tf, light)| (tf.translation().distance_squared(eye), light)).collect();
    by_distance.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (i, (_, light)) in by_distance.iter_mut().enumerate() {
        let want = i < *limit;
        if light.shadow_maps_enabled != want {
            light.shadow_maps_enabled = want;
        }
    }
}
