//! The player: spawning, the primitive human model and the sword.

pub mod action;
pub mod controller;
pub mod stats;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::input::{PlayerInput, default_input_map};
use crate::tuning::*;
use action::PlayerAction;
use controller::CharacterController;
use stats::{DamageMessage, Health, Stamina};

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DamageMessage>()
            .configure_sets(
                FixedUpdate,
                (PlayerSystems::Damage, PlayerSystems::Actions, PlayerSystems::Stats, PlayerSystems::Movement).chain(),
            )
            .add_systems(Startup, spawn_player)
            .add_systems(
                FixedUpdate,
                (
                    stats::apply_damage.in_set(PlayerSystems::Damage),
                    (action::start_buffered_action, action::tick_action).chain().in_set(PlayerSystems::Actions),
                    stats::update_stamina.in_set(PlayerSystems::Stats),
                ),
            )
            .add_plugins(controller::ControllerPlugin);
    }
}

/// Fixed-timestep order for player logic: damage → actions → stats → movement.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlayerSystems {
    Damage,
    Actions,
    Stats,
    Movement,
}

#[derive(Component)]
pub struct Player;

/// Child entity holding all meshes; its local origin is at the feet.
#[derive(Component)]
pub struct PlayerModel;

/// Where the held weapon attaches. Phase 7 swaps the weapon mesh under this entity.
#[derive(Component)]
pub struct RightHandSocket;

/// Where a shield / off-hand item attaches.
#[derive(Component)]
pub struct LeftHandSocket;

fn spawn_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mat = |c: Color, rough: f32, metal: f32| StandardMaterial {
        base_color: c,
        perceptual_roughness: rough,
        metallic: metal,
        ..default()
    };
    let skin = materials.add(mat(Color::srgb(0.78, 0.6, 0.48), 0.7, 0.0));
    let tunic = materials.add(mat(Color::srgb(0.32, 0.22, 0.14), 0.9, 0.0));
    let trousers = materials.add(mat(Color::srgb(0.15, 0.14, 0.13), 0.9, 0.0));
    let leather = materials.add(mat(Color::srgb(0.12, 0.08, 0.05), 0.8, 0.0));
    let steel = materials.add(mat(Color::srgb(0.75, 0.76, 0.78), 0.3, 1.0));
    let dark = materials.add(mat(Color::srgb(0.03, 0.03, 0.03), 0.6, 0.0));

    let (sx, sy, sz) = PLAYER_SPAWN;
    commands
        .spawn((
            Name::new("Player"),
            Player,
            PlayerInput::default(),
            CharacterController::default(),
            PlayerAction::default(),
            Health::new(PLAYER_MAX_HP),
            Stamina::new(PLAYER_MAX_STAMINA),
            default_input_map(),
            Transform::from_xyz(sx, sy, sz),
            Visibility::default(),
            RigidBody::Kinematic,
            Collider::capsule(PLAYER_RADIUS, PLAYER_CAPSULE_LENGTH),
            TransformInterpolation,
        ))
        .with_children(|root| {
            root.spawn((
                Name::new("Player Model"),
                PlayerModel,
                Transform::from_xyz(0.0, -PLAYER_HALF_HEIGHT, 0.0),
                Visibility::default(),
            ))
            .with_children(|m| {
                let mut part = |name: &'static str, mesh: Mesh, material: &Handle<StandardMaterial>, t: Transform| {
                    m.spawn((Name::new(name), Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.clone()), t));
                };
                // Model faces -Z (Bevy's forward).
                for x in [-0.12, 0.12] {
                    part("Leg", Capsule3d::new(0.11, 0.6).into(), &trousers, Transform::from_xyz(x, 0.45, 0.0));
                    part("Boot", Cuboid::new(0.16, 0.12, 0.28).into(), &leather, Transform::from_xyz(x, 0.06, -0.04));
                }
                part("Torso", Cuboid::new(0.5, 0.62, 0.28).into(), &tunic, Transform::from_xyz(0.0, 1.17, 0.0));
                part("Belt", Cuboid::new(0.52, 0.08, 0.3).into(), &leather, Transform::from_xyz(0.0, 0.9, 0.0));
                part("Neck", Cylinder::new(0.06, 0.1).into(), &skin, Transform::from_xyz(0.0, 1.52, 0.0));
                part("Head", Sphere::new(0.14).into(), &skin, Transform::from_xyz(0.0, 1.66, 0.0));
                part("Hair", Sphere::new(0.145).into(), &leather, Transform::from_xyz(0.0, 1.69, 0.025));
                part("Eyes", Cuboid::new(0.14, 0.03, 0.02).into(), &dark, Transform::from_xyz(0.0, 1.68, -0.13));
                for x in [-0.33, 0.33] {
                    part("Arm", Capsule3d::new(0.07, 0.45).into(), &tunic, Transform::from_xyz(x, 1.15, 0.0));
                    part("Hand", Sphere::new(0.07).into(), &skin, Transform::from_xyz(x, 0.85, -0.02));
                }

                // Right hand socket + longsword. Sword rests pointing forward and down.
                m.spawn((
                    Name::new("Right Hand Socket"),
                    RightHandSocket,
                    Transform::from_xyz(0.33, 0.85, -0.02).with_rotation(Quat::from_euler(EulerRot::XYZ, -1.95, 0.0, -0.15)),
                    Visibility::default(),
                ))
                .with_children(|s| {
                    // Blade extends along local +Y from the grip.
                    let mut sword = |name: &'static str, mesh: Mesh, material: &Handle<StandardMaterial>, y: f32| {
                        s.spawn((Name::new(name), Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.clone()), Transform::from_xyz(0.0, y, 0.0)));
                    };
                    sword("Pommel", Sphere::new(0.035).into(), &steel, -0.1);
                    sword("Grip", Cylinder::new(0.022, 0.2).into(), &leather, 0.0);
                    sword("Crossguard", Cuboid::new(0.22, 0.035, 0.04).into(), &steel, 0.11);
                    sword("Blade", Cuboid::new(0.06, 0.9, 0.015).into(), &steel, 0.58);
                });

                m.spawn((
                    Name::new("Left Hand Socket"),
                    LeftHandSocket,
                    Transform::from_xyz(-0.33, 0.85, -0.02),
                    Visibility::default(),
                ));
            });
        });
}
