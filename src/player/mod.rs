//! The player: a generic character (`crate::character`) with a humanoid body
//! (`crate::humanoid`), a starting weapon (`crate::weapon`), keyboard/mouse input and the camera's attention.

use bevy::prelude::*;

use crate::character::character_bundle;
use crate::humanoid::anim::HumanoidAnimator;
use crate::humanoid::{HumanoidSpec, spawn_humanoid};
use crate::input::{PlayerInput, default_input_map};
use crate::tuning::*;
use crate::weapon::{EquipWeapon, light_sword};

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player);
    }
}

#[derive(Component)]
pub struct Player;

fn spawn_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut equip: MessageWriter<EquipWeapon>,
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
        ))
        .id();
    let rig = spawn_humanoid(&mut commands, &mut meshes, &mut materials, player, &spec, PLAYER_HALF_HEIGHT);

    commands.entity(player).insert((spec, rig, HumanoidAnimator::default()));
    equip.write(EquipWeapon { character: player, def: &light_sword::LONGSWORD });
}
