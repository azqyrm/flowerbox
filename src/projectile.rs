//! Things that fly: darts from the library's traps, the Archivist's ink bolts. A projectile flies
//! straight (a little gravity), stops at walls, and hurts the first body it touches that this
//! machine owns (`Without<Remote>`: our player; on the host / offline the real enemies too) —
//! like every hit, decided on the victim's machine, so every machine flings its own copy.
//! Hostile projectiles pass through hostile bodies (an enemy's ink doesn't hurt enemies), a trap's
//! dart hurts anyone (lure them in).

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::character::Remote;
use crate::character::stats::{DamageMessage, Health};
use crate::combat::{Hurtbox, Team};

pub struct ProjectilePlugin;

impl Plugin for ProjectilePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SpawnProjectile>().add_systems(Startup, assets).add_systems(Update, (spawn, fly).chain());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectileKind {
    /// A thin iron dart (traps).
    Dart,
    /// A glob of glowing black ink (the Archivist).
    Ink,
    /// A gob of glowing green-yellow bile (the Matron).
    Bile,
    /// A crackling blue-white spark (the Stormcaller).
    Spark,
}

/// Fling one.
#[derive(Message, Debug, Clone, Copy)]
pub struct SpawnProjectile {
    pub pos: Vec3,
    pub vel: Vec3,
    pub damage: f32,
    pub stance: f32,
    /// Its side (`None` = a trap: hurts anyone).
    pub team: Option<Team>,
    pub kind: ProjectileKind,
    /// Who flung it (never hits them).
    pub by: Option<Entity>,
}

#[derive(Component, Debug)]
struct Projectile {
    shot: SpawnProjectile,
    age: f32,
}

#[derive(Resource)]
struct ProjectileAssets {
    dart: (Handle<Mesh>, Handle<StandardMaterial>),
    ink: (Handle<Mesh>, Handle<StandardMaterial>),
    bile: (Handle<Mesh>, Handle<StandardMaterial>),
    spark: (Handle<Mesh>, Handle<StandardMaterial>),
}

const LIFE: f32 = 4.0;
const RADIUS: f32 = 0.16;

fn assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(ProjectileAssets {
        dart: (
            meshes.add(Cylinder::new(0.012, 0.35)),
            materials.add(StandardMaterial { base_color: Color::srgb(0.15, 0.15, 0.16), metallic: 0.8, perceptual_roughness: 0.4, ..default() }),
        ),
        ink: (
            meshes.add(Sphere::new(0.13).mesh().ico(2).unwrap()),
            materials.add(StandardMaterial {
                base_color: Color::srgb(0.02, 0.01, 0.04),
                emissive: LinearRgba::rgb(0.9, 0.2, 2.4),
                perceptual_roughness: 0.15,
                ..default()
            }),
        ),
        spark: (
            meshes.add(Sphere::new(0.09).mesh().ico(1).unwrap()),
            materials.add(StandardMaterial { base_color: Color::srgb(0.8, 0.9, 1.0), emissive: LinearRgba::rgb(8.0, 10.0, 18.0), unlit: true, ..default() }),
        ),
        bile: (
            meshes.add(Sphere::new(0.12).mesh().ico(2).unwrap()),
            materials.add(StandardMaterial {
                base_color: Color::srgb(0.5, 0.6, 0.1),
                emissive: LinearRgba::rgb(2.5, 3.5, 0.3),
                perceptual_roughness: 0.1,
                ..default()
            }),
        ),
    });
}

fn spawn(mut commands: Commands, assets: Option<Res<ProjectileAssets>>, mut shots: MessageReader<SpawnProjectile>) {
    let Some(assets) = assets else { return };
    for shot in shots.read() {
        let (mesh, mat) = match shot.kind {
            ProjectileKind::Dart => &assets.dart,
            ProjectileKind::Ink => &assets.ink,
            ProjectileKind::Bile => &assets.bile,
            ProjectileKind::Spark => &assets.spark,
        };
        // A dart lies along its flight (the cylinder's axis is +Y).
        let rot = Quat::from_rotation_arc(Vec3::Y, shot.vel.normalize_or(Vec3::NEG_Z));
        let mut e = commands.spawn((
            Name::new("Projectile"),
            Projectile { shot: *shot, age: 0.0 },
            Mesh3d(mesh.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(shot.pos).with_rotation(rot),
        ));
        let glow = match shot.kind {
            ProjectileKind::Ink => Some(Color::srgb(0.6, 0.3, 1.0)),
            ProjectileKind::Bile => Some(Color::srgb(0.7, 1.0, 0.2)),
            ProjectileKind::Spark => Some(Color::srgb(0.6, 0.8, 1.0)),
            ProjectileKind::Dart => None,
        };
        if let Some(color) = glow {
            e.with_child((PointLight { color, intensity: 8000.0, range: 3.0, ..default() }, Transform::default()));
        }
    }
}

#[allow(clippy::type_complexity)]
fn fly(
    mut commands: Commands,
    time: Res<Time>,
    spatial: SpatialQuery,
    mut damage: MessageWriter<DamageMessage>,
    mut flying: Query<(Entity, &mut Projectile, &mut Transform)>,
    bodies: Query<(Option<&Team>, Has<Remote>), (With<Hurtbox>, With<Health>)>,
    characters: Query<(), With<Hurtbox>>,
) {
    let dt = time.delta_secs();
    for (e, mut p, mut tf) in &mut flying {
        p.age += dt;
        let gravity = if p.shot.kind == ProjectileKind::Dart { 1.5 } else { 0.0 };
        p.shot.vel.y -= gravity * dt;
        let step = p.shot.vel * dt;
        let from = tf.translation;
        // A wall (or anything that isn't a body) in the way: gone.
        if let Ok(dir) = Dir3::new(step)
            && spatial.cast_ray_predicate(from, dir, step.length(), true, &SpatialQueryFilter::default(), &|hit| !characters.contains(hit)).is_some()
        {
            commands.entity(e).despawn();
            continue;
        }
        tf.translation += step;
        let mut hit = false;
        for target in spatial.shape_intersections(&Collider::sphere(RADIUS), tf.translation, Quat::IDENTITY, &SpatialQueryFilter::default()) {
            if Some(target) == p.shot.by {
                continue;
            }
            let Ok((team, remote)) = bodies.get(target) else { continue };
            if p.shot.team.is_some() && team == p.shot.team.as_ref() {
                continue; // its own side
            }
            hit = true;
            if !remote {
                damage.write(DamageMessage { target, amount: p.shot.damage, from: Some(from), attacker: p.shot.by, stance: p.shot.stance });
            }
            break;
        }
        if hit || p.age > LIFE {
            commands.entity(e).despawn();
        }
    }
}
