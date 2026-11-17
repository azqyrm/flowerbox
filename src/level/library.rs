//! The Drowned Library's props (level 3): bookshelves, reading desks, book piles, standing water,
//! the Orrery — and its dangers and secrets:
//! - **Dart traps**: a sunken plate in the floor; step on it and a carved face in the wall behind
//!   (`facing` = where the darts fly) spits three darts a moment later. Darts hurt anyone (lure
//!   enemies onto the plate). Each machine runs its own traps for its own player.
//! - **Censer pendulums**: a spiked iron censer on a chain swinging across a corridor (`facing` =
//!   its swing), on a fixed clock — time your run. It hits hard and breaks stance.
//! - **Illusory walls**: a wall that isn't; strike it and it fades, showing the way on (per machine).

use std::f32::consts::{PI, TAU};

use avian3d::prelude::*;
use bevy::prelude::*;

use super::layout::{Layout, PropDef, PropKind};
use crate::character::Remote;
use crate::character::stats::{DamageMessage, DamageTaken, Health};
use crate::combat::Hurtbox;
use crate::player::Player;
use crate::projectile::{ProjectileKind, SpawnProjectile};
use crate::textures::{self, Tex};
use crate::tuning::*;
use crate::ui::icons::NoticeIcon;
use crate::ui::notifications::Notify;

pub struct LibraryPlugin;

impl Plugin for LibraryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (dart_traps, swing_pendulums, reveal_illusions, spin_orrery));
    }
}

/// A pressure plate and the wall face that shoots.
#[derive(Component, Debug)]
pub struct DartTrap {
    plate: Vec3,
    launcher: Vec3,
    dir: Vec3,
    /// Seconds until it fires (pressed), then until it can fire again.
    fuse: Option<f32>,
    rearm: f32,
}

/// A censer on a chain, swinging across `axis` (the root entity sits at the pivot).
#[derive(Component, Debug)]
pub struct Pendulum {
    pivot: Vec3,
    yaw: f32,
    length: f32,
    phase: f32,
    /// Bodies it hit recently (no hit every frame while passing through).
    recent: Vec<(Entity, f32)>,
}

/// A wall that isn't one.
#[derive(Component, Debug)]
pub struct IllusoryWall;

/// Turns slowly (the Orrery's rings).
#[derive(Component, Debug)]
struct Spin {
    axis: Vec3,
    speed: f32,
}

pub struct LibraryMats {
    wood: Handle<StandardMaterial>,
    dark_wood: Handle<StandardMaterial>,
    books: Handle<StandardMaterial>,
    paper: Handle<StandardMaterial>,
    leather: Handle<StandardMaterial>,
    iron: Handle<StandardMaterial>,
    brass: Handle<StandardMaterial>,
    water: Handle<StandardMaterial>,
    wax: Handle<StandardMaterial>,
    flame: Handle<StandardMaterial>,
    ember: Handle<StandardMaterial>,
    panel: Handle<StandardMaterial>,
    star: Handle<StandardMaterial>,
}

impl LibraryMats {
    pub fn new(materials: &mut Assets<StandardMaterial>) -> Self {
        fn tex(materials: &mut Assets<StandardMaterial>, c: Color, t: Option<Tex>, metallic: f32, rough: f32) -> Handle<StandardMaterial> {
            materials.add(StandardMaterial { base_color: c, base_color_texture: t.and_then(textures::get), metallic, perceptual_roughness: rough, ..default() })
        }
        Self {
            wood: tex(materials, Color::srgb(0.36, 0.24, 0.15), Some(Tex::Planks), 0.0, 0.8),
            dark_wood: tex(materials, Color::srgb(0.2, 0.13, 0.08), Some(Tex::Planks), 0.0, 0.85),
            books: tex(materials, Color::srgb(0.8, 0.75, 0.7), Some(Tex::Books), 0.0, 0.9),
            paper: tex(materials, Color::srgb(0.85, 0.8, 0.68), None, 0.0, 0.95),
            leather: tex(materials, Color::srgb(0.32, 0.12, 0.08), Some(Tex::Leather), 0.0, 0.8),
            iron: tex(materials, Color::srgb(0.12, 0.12, 0.13), Some(Tex::Rust), 0.8, 0.5),
            brass: tex(materials, Color::srgb(0.7, 0.52, 0.25), None, 1.0, 0.35),
            water: materials.add(StandardMaterial {
                base_color: Color::srgba(0.02, 0.04, 0.05, 0.82),
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 0.04,
                reflectance: 0.7,
                ..default()
            }),
            wax: materials.add(StandardMaterial { base_color: Color::srgb(0.9, 0.85, 0.7), perceptual_roughness: 0.6, ..default() }),
            flame: materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.7, 0.3), emissive: LinearRgba::rgb(12.0, 6.0, 1.5), unlit: true, ..default() }),
            ember: materials.add(StandardMaterial { base_color: Color::srgb(0.9, 0.3, 0.1), emissive: LinearRgba::rgb(8.0, 2.0, 0.4), ..default() }),
            panel: tex(materials, Color::srgb(0.34, 0.23, 0.15), Some(Tex::Panels), 0.0, 0.92),
            star: materials.add(StandardMaterial { base_color: Color::srgb(0.7, 0.8, 1.0), emissive: LinearRgba::rgb(4.0, 5.0, 9.0), unlit: true, ..default() }),
        }
    }
}

/// Spawns one library prop under `root` (placed at its floor point, facing `prop.facing`).
#[allow(clippy::too_many_arguments)]
pub fn spawn_library_prop(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    m: &LibraryMats,
    layout: &Layout,
    prop: &PropDef,
    n: usize,
    root: Entity,
    frame: Transform,
) {
    let floor = frame.translation;
    let height = layout.ceiling_at(floor.xz()) - floor.y;
    fn piece(commands: &mut Commands, meshes: &mut Assets<Mesh>, parent: Entity, mesh: Mesh, mat: &Handle<StandardMaterial>, t: Transform) -> Entity {
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat.clone()), t, ChildOf(parent))).id()
    }
    let solid = |commands: &mut Commands, size: Vec3, at: Vec3| {
        commands.spawn((RigidBody::Static, Collider::cuboid(size.x, size.y, size.z), Transform::from_translation(at), ChildOf(root)));
    };
    let t = Transform::from_xyz;
    let candle = |commands: &mut Commands, meshes: &mut Assets<Mesh>, at: Vec3, light: bool| {
        piece(commands, meshes, root, Cylinder::new(0.025, 0.16).into(), &m.wax, Transform::from_translation(at + Vec3::Y * 0.08));
        piece(commands, meshes, root, Sphere::new(0.018).into(), &m.flame, Transform::from_translation(at + Vec3::Y * 0.19));
        if light {
            commands.spawn((
                PointLight { color: Color::srgb(1.0, 0.7, 0.38), intensity: 9_000.0, range: 4.5, radius: 0.05, ..default() },
                Transform::from_translation(at + Vec3::Y * 0.35),
                ChildOf(root),
            ));
        }
    };
    match prop.kind {
        PropKind::Bookshelf => {
            // Books on both long sides; a wooden carcass and cornice hide the ends.
            let len = if prop.size.0 > 0.0 { prop.size.0 } else { 3.0 };
            let h = if prop.size.1 > 0.0 { prop.size.1 } else { (height - 0.5).min(3.6) };
            let depth = 0.62;
            piece(commands, meshes, root, textures::tiled_box(Vec3::new(len, h, depth), Vec3::new(0.0, h * 0.5, 0.0) + floor, 2.4), &m.books, t(0.0, h * 0.5, 0.0));
            for x in [-1.0, 1.0] {
                piece(commands, meshes, root, Cuboid::new(0.08, h + 0.1, depth + 0.06).into(), &m.dark_wood, t(x * (len * 0.5 + 0.04), (h + 0.1) * 0.5, 0.0));
            }
            piece(commands, meshes, root, Cuboid::new(len + 0.25, 0.14, depth + 0.16).into(), &m.dark_wood, t(0.0, h + 0.07, 0.0));
            piece(commands, meshes, root, Cuboid::new(len + 0.1, 0.12, depth + 0.08).into(), &m.dark_wood, t(0.0, 0.06, 0.0));
            solid(commands, Vec3::new(len + 0.2, h, depth + 0.1), Vec3::new(0.0, h * 0.5, 0.0));
        }
        PropKind::Desk => {
            // A reading desk: slanted top, an open book, a candle.
            piece(commands, meshes, root, Cuboid::new(1.5, 0.08, 0.8).into(), &m.wood, t(0.0, 0.78, 0.0).with_rotation(Quat::from_rotation_x(0.12)));
            for (x, z) in [(-0.65, -0.32), (0.65, -0.32), (-0.65, 0.32), (0.65, 0.32)] {
                piece(commands, meshes, root, Cuboid::new(0.07, 0.76, 0.07).into(), &m.dark_wood, t(x, 0.38, z));
            }
            for side in [-1.0, 1.0] {
                piece(commands, meshes, root, Cuboid::new(0.34, 0.012, 0.46).into(), &m.paper, t(side * 0.18, 0.84, 0.0).with_rotation(Quat::from_rotation_x(0.12) * Quat::from_rotation_z(-side * 0.12)));
            }
            candle(commands, meshes, Vec3::new(0.6, 0.82, 0.25), true);
            solid(commands, Vec3::new(1.5, 0.85, 0.8), Vec3::new(0.0, 0.42, 0.0));
        }
        PropKind::BookPile => {
            let mut y = 0.0;
            for i in 0..5 {
                let (w, d, h) = (0.32 - 0.02 * i as f32, 0.24, 0.06 + 0.02 * (i % 2) as f32);
                piece(commands, meshes, root, Cuboid::new(w, h, d).into(), &m.leather, t(0.03 * ((i * 7) % 3) as f32, y + h * 0.5, 0.0).with_rotation(Quat::from_rotation_y(0.4 * i as f32)));
                y += h;
            }
            piece(commands, meshes, root, Cuboid::new(0.3, 0.012, 0.22).into(), &m.paper, t(0.5, 0.006, 0.2).with_rotation(Quat::from_rotation_y(0.7)));
        }
        PropKind::Water => {
            let (w, d) = (prop.size.0.max(1.0), prop.size.1.max(1.0));
            piece(commands, meshes, root, Plane3d::default().mesh().size(w, d).into(), &m.water, t(0.0, 0.06, 0.0));
        }
        PropKind::DartTrap => {
            // The plate, sunk a little; the face on the wall behind (opposite where darts fly).
            piece(commands, meshes, root, Cuboid::new(0.8, 0.03, 0.8).into(), &m.iron, t(0.0, 0.005, 0.0));
            let fly = Vec3::new(prop.facing.vec().x, 0.0, prop.facing.vec().y);
            let back = layout.wall_distance(floor.xz(), prop.facing.opposite()).unwrap_or(3.0);
            let launcher = floor - fly * (back - 0.15) + Vec3::Y * 1.15;
            let yaw = Quat::from_rotation_y(prop.facing.yaw());
            commands.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.5, 0.6, 0.12))),
                MeshMaterial3d(m.iron.clone()),
                Transform::from_translation(launcher).with_rotation(yaw),
            ));
            for x in [-0.12, 0.0, 0.12] {
                commands.spawn((
                    Mesh3d(meshes.add(Cylinder::new(0.03, 0.02))),
                    MeshMaterial3d(m.ember.clone()),
                    Transform::from_translation(launcher + yaw * Vec3::new(x, -0.08, -0.07)).with_rotation(yaw * Quat::from_rotation_x(PI * 0.5)),
                ));
            }
            commands.entity(root).insert(DartTrap { plate: floor, launcher: launcher + fly * 0.2, dir: fly, fuse: None, rearm: 0.0 });
        }
        PropKind::Pendulum => {
            let length = (height - 1.2).max(2.0);
            let pivot = floor + Vec3::Y * height;
            // The root swings at the pivot: chain down, the censer at the end.
            let swing = commands
                .spawn((
                    Name::new("Pendulum"),
                    Pendulum { pivot, yaw: prop.facing.yaw(), length, phase: n as f32 * 1.7, recent: vec![] },
                    Transform::from_translation(pivot),
                    Visibility::default(),
                ))
                .id();

            piece(commands, meshes, swing, Cylinder::new(0.03, length).into(), &m.iron, t(0.0, -length * 0.5, 0.0));
            piece(commands, meshes, swing, Sphere::new(0.38).mesh().ico(2).unwrap(), &m.iron, t(0.0, -length, 0.0));
            piece(commands, meshes, swing, Sphere::new(0.24).mesh().ico(1).unwrap(), &m.ember, t(0.0, -length, 0.0));
            for k in 0..8 {
                let a = k as f32 / 8.0 * TAU;
                let dir = Vec3::new(a.cos(), 0.0, a.sin());
                piece(commands, meshes, swing, Cone { radius: 0.07, height: 0.32 }.into(), &m.iron, Transform::from_translation(Vec3::new(0.0, -length, 0.0) + dir * 0.42).with_rotation(Quat::from_rotation_arc(Vec3::Y, dir)));
            }
            commands.spawn((PointLight { color: Color::srgb(1.0, 0.5, 0.2), intensity: 25_000.0, range: 5.0, ..default() }, Transform::from_xyz(0.0, -length, 0.0), ChildOf(swing)));
        }
        PropKind::IllusoryWall => {
            let (w, h) = (prop.size.0.max(1.0), prop.size.1.max(2.5));
            let at = floor + Vec3::Y * h * 0.5;
            commands.spawn((
                Name::new("Illusory wall"),
                IllusoryWall,
                Health::new(1.0),
                Hurtbox,
                Mesh3d(meshes.add(textures::tiled_box(Vec3::new(w, h, LEVEL_WALL_THICKNESS), at, 3.0))),
                MeshMaterial3d(m.panel.clone()),
                RigidBody::Static,
                Collider::cuboid(w, h, LEVEL_WALL_THICKNESS),
                Transform::from_translation(at).with_rotation(Quat::from_rotation_y(prop.facing.yaw())),
            ));
        }
        PropKind::Orrery => {
            // A brass armillary turning above the floor round a glowing star.
            let c = Vec3::new(0.0, (height * 0.4).clamp(3.5, 5.0), 0.0);
            piece(commands, meshes, root, Sphere::new(0.45).mesh().ico(3).unwrap(), &m.star, Transform::from_translation(c));
            commands.spawn((PointLight { color: Color::srgb(0.6, 0.7, 1.0), intensity: 250_000.0, range: 22.0, ..default() }, Transform::from_translation(c), ChildOf(root)));
            for (i, (r, axis, speed)) in [(1.6, Vec3::X, 0.25), (2.4, Vec3::Z, -0.18), (3.3, Vec3::new(1.0, 0.0, 1.0).normalize(), 0.12)].into_iter().enumerate() {
                let ring = piece(commands, meshes, root, Torus::new(r - 0.05, r + 0.05).into(), &m.brass, Transform::from_translation(c).with_rotation(Quat::from_rotation_x(0.4 * i as f32)));
                commands.entity(ring).insert(Spin { axis, speed });
                // A planet riding each ring.
                commands.spawn((Mesh3d(meshes.add(Sphere::new(0.16 + 0.05 * i as f32))), MeshMaterial3d(m.brass.clone()), Transform::from_xyz(r, 0.0, 0.0), ChildOf(ring)));
            }
            // Chains up to the ceiling.
            piece(commands, meshes, root, Cylinder::new(0.03, height - c.y).into(), &m.iron, t(0.0, (height + c.y) * 0.5, 0.0));
        }
        _ => {}
    }
}

/// Plates fire the darts (stepped on by this machine's player).
fn dart_traps(
    time: Res<Time>,
    mut traps: Query<&mut DartTrap>,
    player: Query<&Transform, (With<Player>, Without<Remote>)>,
    mut shots: MessageWriter<SpawnProjectile>,
    mut sounds: MessageWriter<crate::sound::PlaySound>,
) {
    let dt = time.delta_secs();
    let feet = player.single().ok().map(|tf| tf.translation - Vec3::Y * (PLAYER_HALF_HEIGHT + GROUND_SKIN));
    for mut trap in &mut traps {
        trap.rearm = (trap.rearm - dt).max(0.0);
        if let Some(fuse) = trap.fuse.as_mut() {
            *fuse -= dt;
            if *fuse <= 0.0 {
                trap.fuse = None;
                trap.rearm = DART_REARM;
                for (i, side) in [-0.12f32, 0.0, 0.12].into_iter().enumerate() {
                    let across = Vec3::new(-trap.dir.z, 0.0, trap.dir.x);
                    let pos = trap.launcher + across * side + Vec3::Y * 0.03 * i as f32;
                    shots.write(SpawnProjectile { pos, vel: (trap.dir + across * side * 0.15) * DART_SPEED, damage: DART_DAMAGE, stance: DART_DAMAGE, team: None, kind: ProjectileKind::Dart, by: None });
                }
                sounds.write(crate::sound::PlaySound { sound: crate::sound::Sound::Dart, pos: trap.launcher, volume: 0.9 });
                info!("a dart trap fires");
            }
            continue;
        }
        let Some(feet) = feet else { continue };
        let on = (feet - trap.plate).with_y(0.0).length() < 0.55 && (feet.y - trap.plate.y).abs() < 0.4;
        if on && trap.rearm <= 0.0 {
            trap.fuse = Some(DART_FUSE);
            sounds.write(crate::sound::PlaySound { sound: crate::sound::Sound::Click, pos: trap.plate, volume: 0.8 });
        }
    }
}

/// Swings every censer on the shared clock; it hurts the bodies this machine owns.
fn swing_pendulums(
    time: Res<Time>,
    spatial: SpatialQuery,
    mut damage: MessageWriter<DamageMessage>,
    mut pendulums: Query<(&mut Pendulum, &mut Transform)>,
    bodies: Query<(), (With<Hurtbox>, With<Health>, Without<Remote>, Without<IllusoryWall>)>,
) {
    let t = time.elapsed_secs();
    for (mut p, mut tf) in &mut pendulums {
        let angle = PENDULUM_SWING * (TAU * t / PENDULUM_PERIOD + p.phase).sin();
        let rot = Quat::from_rotation_y(p.yaw) * Quat::from_rotation_x(angle);
        tf.rotation = rot;
        let bob = p.pivot + rot * Vec3::new(0.0, -p.length, 0.0);
        p.recent.retain(|(_, until)| *until > t);
        for target in spatial.shape_intersections(&Collider::sphere(0.55), bob, Quat::IDENTITY, &SpatialQueryFilter::default()) {
            if !bodies.contains(target) || p.recent.iter().any(|(e, _)| *e == target) {
                continue;
            }
            p.recent.push((target, t + 1.0));
            info!("a censer strikes {target}");
            damage.write(DamageMessage { target, amount: PENDULUM_DAMAGE, from: Some(bob), attacker: None, stance: PENDULUM_DAMAGE * 1.5 });
        }
    }
}

/// An illusory wall struck: it fades away.
fn reveal_illusions(mut commands: Commands, mut taken: MessageReader<DamageTaken>, walls: Query<(), With<IllusoryWall>>, mut notify: MessageWriter<Notify>, mut sounds: MessageWriter<crate::sound::PlaySound>, tfs: Query<&Transform>) {
    for t in taken.read() {
        if walls.contains(t.target) {
            if let Ok(tf) = tfs.get(t.target) {
                sounds.write(crate::sound::PlaySound { sound: crate::sound::Sound::Shriek, pos: tf.translation, volume: 0.25 });
            }
            commands.entity(t.target).despawn();
            notify.write(Notify::new(NoticeIcon::Door, crate::ui::strings::ILLUSORY_WALL.to_string()));
            info!("an illusory wall fades");
        }
    }
}

fn spin_orrery(time: Res<Time>, mut q: Query<(&Spin, &mut Transform)>) {
    for (s, mut tf) in &mut q {
        tf.rotate_axis(Dir3::new(s.axis).unwrap_or(Dir3::Y), s.speed * time.delta_secs());
    }
}
