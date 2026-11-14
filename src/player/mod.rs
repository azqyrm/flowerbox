//! The player: a generic character (`crate::character`) with a humanoid body
//! (`crate::humanoid`), a longsword, keyboard/mouse input and the camera's attention.

use bevy::camera::visibility::NoFrustumCulling;
use bevy::prelude::*;

use crate::character::character_bundle;
use crate::combat::Weapon;
use crate::humanoid::anim::HumanoidAnimator;
use crate::humanoid::{Bone, HumanoidSpec, spawn_humanoid};
use crate::input::{PlayerInput, default_input_map};
use crate::tuning::*;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player);
    }
}

#[derive(Component)]
pub struct Player;

/// The sword mesh entity in the right hand. Phase 7 swaps the weapon under the hand bone.
#[derive(Component)]
pub struct WeaponModel;

fn spawn_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (sx, sy, sz) = PLAYER_SPAWN;
    let spec = HumanoidSpec::human();
    let player = commands
        .spawn((
            Name::new("Player"),
            Player,
            PlayerInput::default(),
            default_input_map(),
            character_bundle(Transform::from_xyz(sx, sy, sz), PLAYER_MAX_HP, PLAYER_MAX_STAMINA),
            Weapon::longsword(),
        ))
        .id();
    let rig = spawn_humanoid(&mut commands, &mut meshes, &mut materials, player, &spec, PLAYER_HALF_HEIGHT);

    // Longsword in the right hand; the blade runs along the hand's local +Y.
    let steel = materials.add(StandardMaterial {
        base_color: Color::srgb(0.75, 0.76, 0.78),
        perceptual_roughness: 0.3,
        metallic: 1.0,
        ..default()
    });
    let leather = materials.add(StandardMaterial { base_color: Color::srgb(0.12, 0.08, 0.05), ..default() });
    let sword = commands.spawn((Name::new("Longsword"), WeaponModel, Transform::default(), Visibility::default())).id();
    for (mesh, mat, y) in [
        (Mesh::from(Sphere::new(0.035)), &steel, -0.1),
        (Cylinder::new(0.022, 0.2).into(), &leather, 0.0),
        (Cuboid::new(0.22, 0.035, 0.04).into(), &steel, 0.11),
        (Cuboid::new(0.06, 0.9, 0.015).into(), &steel, 0.58),
    ] {
        let part = commands
            .spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat.clone()), Transform::from_xyz(0.0, y, 0.0), NoFrustumCulling))
            .id();
        commands.entity(sword).add_child(part);
    }
    commands.entity(rig.bone(Bone::HandR)).add_child(sword);
    commands.entity(player).insert((spec, rig, HumanoidAnimator::default()));
}
