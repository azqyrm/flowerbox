//! Wearing the skin chosen in the settings (`humanoid/skins.rs`), live: changing it in the menu
//! swaps the body at once. Also used for other players' puppets when they change theirs.
//!
//! Swapping a body = despawn the old model (bones, meshes, and everything hanging off the bones:
//! weapon and armour models, the tankard), spawn the new one on the same character, then put the
//! gear back on (next frame, once the new bones exist). Gameplay state — health, actions, the
//! capsule, the animator — stays on the character and doesn't notice.

use bevy::prelude::*;

use super::Player;
use crate::healing::Tankard;
use crate::humanoid::skins::Skin;
use crate::humanoid::{HumanoidRig, HumanoidSpec, spawn_humanoid};
use crate::items::inventory::{EquipVisuals, Equipment};
use crate::settings::Settings;
use crate::weapon::{EquipWeapon, WeaponSystems, Wielded};

pub struct SkinPlugin;

impl Plugin for SkinPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (wear_chosen_skin, regear).chain().before(WeaponSystems::Equip));
    }
}

/// The body was just swapped: its gear has to be put back on.
#[derive(Component)]
struct Reskinned;

/// Replaces `entity`'s humanoid body (`rig`) with one built from `spec`.
pub fn reskin(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, entity: Entity, rig: &HumanoidRig, spec: HumanoidSpec) {
    commands.entity(rig.model).despawn();
    let new = spawn_humanoid(commands, meshes, materials, entity, &spec, rig.root_height);
    commands.entity(entity).insert((spec, new, Reskinned)).remove::<Tankard>();
}

fn wear_chosen_skin(
    settings: Res<Settings>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    player: Query<(Entity, &Skin, &HumanoidRig), With<Player>>,
) {
    if !settings.is_changed() {
        return;
    }
    for (entity, worn, rig) in &player {
        if *worn != settings.skin {
            info!("wearing the {} skin", settings.skin.name());
            reskin(&mut commands, &mut meshes, &mut materials, entity, rig, settings.skin.spec());
            commands.entity(entity).insert(settings.skin);
        }
    }
}

/// Weapons back in the hands, armour back on (the old models went with the old bones).
fn regear(
    mut commands: Commands,
    mut equip: MessageWriter<EquipWeapon>,
    mut bodies: Query<(Entity, Option<&mut Wielded>, Option<&mut Equipment>, Option<&mut EquipVisuals>), With<Reskinned>>,
) {
    for (entity, wielded, equipment, visuals) in &mut bodies {
        if let Some(mut w) = wielded {
            w.forget_models();
            equip.write(EquipWeapon { character: entity, right: w.right, left: w.left, two_hand: w.two_hand });
        }
        if let Some(mut v) = visuals {
            *v = EquipVisuals::default();
        }
        if let Some(mut e) = equipment {
            e.set_changed();
        }
        commands.entity(entity).remove::<Reskinned>();
    }
}
