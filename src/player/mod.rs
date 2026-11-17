//! The player: a generic character (`crate::character`) with a humanoid body
//! (`crate::humanoid`), starting gear (`crate::items`), Healing Ale (`crate::healing`),
//! keyboard/mouse input and the camera's attention. It respawns at the Hearth (`crate::hearth`).

use bevy::prelude::*;

use crate::character::character_bundle;
use crate::combat::{Hurtbox, Team};
use crate::lockon::LockOn;
use crate::pvp::Pvp;
use crate::character::stats::HitImmunity;
use crate::healing::HealingAle;
use crate::humanoid::anim::HumanoidAnimator;
use crate::humanoid::spawn_humanoid;
use crate::input::{PlayerInput, input_map};
use crate::settings::Settings;
use crate::tuning::*;
use crate::character::stats::{DamageAbsorption, Stance};
use crate::items::ItemId;
use crate::items::inventory::{EquipStats, EquipVisuals, Equipment, Inventory};

pub mod skin;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player).add_plugins(skin::SkinPlugin);
    }
}

#[derive(Component)]
pub struct Player;

/// The bag at the start; the first two (Longsword, Healing Ale) start equipped. The Poppy and
/// the Camomile switch PvP on and off (`crate::pvp`).
const STARTING_ITEMS: [ItemId; 7] =
    [ItemId::Longsword, ItemId::HealingAle, ItemId::WoodenShield, ItemId::RustedAxe, ItemId::LeatherHood, ItemId::Poppy, ItemId::Camomile];

fn spawn_player(
    settings: Res<Settings>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (sx, sy, sz) = PLAYER_SPAWN;
    let spec = settings.skin.spec();
    let player = commands
        .spawn((
            Name::new("Player"),
            Player,
            PlayerInput::default(),
            HealingAle::new(ALE_CHARGES),
            input_map(&settings.bindings),
            character_bundle(Transform::from_xyz(sx, sy, sz), PLAYER_MAX_HP, PLAYER_MAX_STAMINA),
        ))
        .id();
    let rig = spawn_humanoid(&mut commands, &mut meshes, &mut materials, player, &spec, PLAYER_HALF_HEIGHT);

    commands.entity(player).insert((spec, rig, settings.skin, HumanoidAnimator::default(), Hurtbox, Team::Player, LockOn::default(), Pvp(false), HitImmunity::default()));

    // Starting gear: Longsword in the right hand and the Healing Ale in the quick slot; a shield,
    // an axe and a hood in the bag. Equipment changes swap the weapon model (see `items`).
    let inventory = Inventory::from_items(&STARTING_ITEMS);
    let mut equipment = Equipment::default();
    equipment.equip(&inventory, 0);
    equipment.equip(&inventory, 1);
    commands.entity(player).insert((inventory, equipment, EquipStats::default(), EquipVisuals::default(), DamageAbsorption::default(), Stance::new(PLAYER_STANCE, false)));
}
