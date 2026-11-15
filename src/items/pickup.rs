//! Items lying in the arena: a glowing wisp you walk up to and take with E. Picked-up items go
//! into the bag (not equipped) and an [`ItemPickedUp`] message shows a toast on the HUD.

use bevy::prelude::*;

use super::ItemId;
use super::inventory::Inventory;
use crate::character::action::CharacterAction;
use crate::hearth::DeathSequence;
use crate::input::{GameplayBlocked, PlayerInput};
use crate::interact::{InteractPrompt, InteractSystems};
use crate::player::Player;
use crate::tuning::*;
use crate::ui::strings;

pub struct PickupPlugin;

impl Plugin for PickupPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ItemPickedUp>()
            .add_systems(Startup, spawn_pickups)
            .add_systems(Update, (pickup_interaction.in_set(InteractSystems::Pickups), animate_pickups));
    }
}

/// Items lying in the arena at start (floor x, z), clear of rubble and pillars.
const PICKUPS: [(ItemId, (f32, f32)); 3] = [
    (ItemId::RustedShortsword, (-7.0, -6.0)),
    (ItemId::StuddedBuckler, (9.0, -7.0)),
    (ItemId::SunseedRing, (-9.0, 7.5)),
];

/// An item waiting on the floor.
#[derive(Component, Debug)]
pub struct Pickup {
    pub item: ItemId,
}

/// The player took an item (for the HUD toast).
#[derive(Message, Debug, Clone, Copy)]
pub struct ItemPickedUp {
    pub item: ItemId,
}

#[derive(Component)]
struct Wisp {
    phase: f32,
}

fn spawn_pickups(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let glow = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.9, 0.6),
        // Bright enough to bloom: a glowing mote you can spot across the arena.
        emissive: LinearRgba::rgb(40.0, 30.0, 14.0),
        ..default()
    });
    let mote = meshes.add(Sphere::new(0.06));
    for (i, (item, (x, z))) in PICKUPS.into_iter().enumerate() {
        let root = commands
            .spawn((Name::new(format!("Pickup: {}", item.def().name)), Pickup { item }, Transform::from_xyz(x, 0.0, z), Visibility::default()))
            .id();
        commands
            .spawn((
                Wisp { phase: i as f32 * 2.3 },
                Mesh3d(mote.clone()),
                MeshMaterial3d(glow.clone()),
                Transform::from_xyz(0.0, 0.35, 0.0),
                ChildOf(root),
            ))
            .with_child(PointLight { color: Color::srgb(1.0, 0.85, 0.55), intensity: 30_000.0, range: 3.5, radius: 0.05, ..default() });
    }
}

/// Bob and pulse.
fn animate_pickups(time: Res<Time>, mut wisps: Query<(&Wisp, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (wisp, mut tf) in &mut wisps {
        tf.translation.y = 0.35 + 0.06 * (t * 1.7 + wisp.phase).sin();
        tf.scale = Vec3::splat(1.0 + 0.2 * (t * 4.3 + wisp.phase).sin());
    }
}

#[allow(clippy::type_complexity)]
fn pickup_interaction(
    mut commands: Commands,
    mut prompt: ResMut<InteractPrompt>,
    blocked: Res<GameplayBlocked>,
    mut picked: MessageWriter<ItemPickedUp>,
    pickups: Query<(Entity, &Pickup, &Transform)>,
    mut player: Query<(&Transform, &CharacterAction, &mut PlayerInput, &mut Inventory, Has<DeathSequence>), With<Player>>,
) {
    let Ok((tf, action, mut input, mut inventory, dying)) = player.single_mut() else { return };
    if blocked.0 || dying || action.current.is_some() {
        return;
    }
    let dist = |p: &Transform| (p.translation - tf.translation).with_y(0.0).length();
    let Some((entity, pickup, _)) = pickups
        .iter()
        .filter(|(_, _, p)| dist(p) < PICKUP_RANGE)
        .min_by(|a, b| dist(a.2).total_cmp(&dist(b.2)))
    else {
        return;
    };
    prompt.0 = Some(strings::PROMPT_PICK_UP);
    if std::mem::take(&mut input.interact) {
        inventory.add(pickup.item);
        picked.write(ItemPickedUp { item: pickup.item });
        commands.entity(entity).despawn();
        info!("picked up {}", pickup.item.def().name);
    }
}
