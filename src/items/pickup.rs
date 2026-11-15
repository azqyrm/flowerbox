//! Items lying in the arena: a glowing wisp you walk up to and take with E. Picked-up items go
//! into the bag (not equipped) and an [`ItemPickedUp`] message shows a notification. Online, the
//! host owns the items: a guest's [`PickupRequest`] goes to the host (`crate::net::world`).

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
            .add_message::<PickupRequest>()
            .add_message::<PickupTaken>()
            .init_resource::<PickupAuthority>()
            .init_resource::<PickupLayout>()
            .add_systems(Startup, (load_pickup_assets, spawn_pickups).chain())
            .add_systems(Update, (pickup_interaction.in_set(InteractSystems::Pickups), grant_pickups, animate_pickups));
    }
}

/// Items lying in the arena at start (floor x, z), clear of rubble and pillars. Their index is
/// their pickup id (the same on every machine).
const ARENA_PICKUPS: [(ItemId, (f32, f32)); 3] = [
    (ItemId::RustedShortsword, (-7.0, -6.0)),
    (ItemId::StuddedBuckler, (9.0, -7.0)),
    (ItemId::SunseedRing, (-9.0, 7.5)),
];

/// Every item lying in a fresh world: (pickup id, item, floor position). The arena's come first
/// (ids 0..); levels add theirs (`crate::level`). Ids are the same on every machine.
#[derive(Resource, Debug, Clone)]
pub struct PickupLayout(pub Vec<(u32, ItemId, Vec3)>);

impl Default for PickupLayout {
    fn default() -> Self {
        Self(ARENA_PICKUPS.iter().enumerate().map(|(i, (item, (x, z)))| (i as u32, *item, Vec3::new(*x, 0.0, *z))).collect())
    }
}

impl PickupLayout {
    /// Layout ids not in `present` (taken in this world).
    pub fn taken(&self, present: &[u32]) -> Vec<u32> {
        let mut ids: Vec<u32> = self.0.iter().map(|(id, _, _)| *id).filter(|id| !present.contains(id)).collect();
        ids.sort_unstable();
        ids
    }
}

/// An item waiting on the floor. `id` is stable across machines (layout index, or a host id for
/// later drops).
#[derive(Component, Debug)]
pub struct Pickup {
    pub id: u32,
    pub item: ItemId,
}

/// The local player took an item (for the notification).
#[derive(Message, Debug, Clone, Copy)]
pub struct ItemPickedUp {
    pub item: ItemId,
}

/// The local player pressed E at pickup `pickup`. Whoever owns the world decides (offline: us;
/// online: the host, see `crate::net::world`).
#[derive(Message, Debug, Clone, Copy)]
pub struct PickupRequest {
    pub pickup: u32,
}

/// This machine handed a pickup to its own player (the host tells the others).
#[derive(Message, Debug, Clone, Copy)]
pub struct PickupTaken {
    pub pickup: u32,
    pub item: ItemId,
}

/// True when this machine decides who gets which pickup (offline, or hosting). A guest's
/// requests go to the host instead.
#[derive(Resource, Debug)]
pub struct PickupAuthority(pub bool);

impl Default for PickupAuthority {
    fn default() -> Self {
        Self(true)
    }
}

/// Shared mesh/material of the glowing motes.
#[derive(Resource, Clone)]
pub struct PickupAssets {
    mote: Handle<Mesh>,
    glow: Handle<StandardMaterial>,
}

#[derive(Component)]
struct Wisp {
    phase: f32,
}

fn load_pickup_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let glow = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.9, 0.6),
        // Bright enough to bloom: a glowing mote you can spot across the arena.
        emissive: LinearRgba::rgb(40.0, 30.0, 14.0),
        ..default()
    });
    commands.insert_resource(PickupAssets { mote: meshes.add(Sphere::new(0.06)), glow });
}

/// Spawns one item on the floor at `pos`.
pub fn spawn_pickup(commands: &mut Commands, assets: &PickupAssets, id: u32, item: ItemId, pos: Vec3) {
    let root = commands
        .spawn((Name::new(format!("Pickup: {}", item.def().name)), Pickup { id, item }, Transform::from_translation(pos), Visibility::default()))
        .id();
    commands
        .spawn((
            Wisp { phase: id as f32 * 2.3 },
            Mesh3d(assets.mote.clone()),
            MeshMaterial3d(assets.glow.clone()),
            Transform::from_xyz(0.0, 0.35, 0.0),
            ChildOf(root),
        ))
        .with_child(PointLight { color: Color::srgb(1.0, 0.85, 0.55), intensity: 30_000.0, range: 3.5, radius: 0.05, ..default() });
}

/// The world's own items (`taken` = ids already picked up in this world).
pub fn spawn_layout(commands: &mut Commands, assets: &PickupAssets, layout: &PickupLayout, taken: &[u32]) {
    for &(id, item, pos) in &layout.0 {
        if !taken.contains(&id) {
            spawn_pickup(commands, assets, id, item, pos);
        }
    }
}

fn spawn_pickups(mut commands: Commands, assets: Res<PickupAssets>, layout: Res<PickupLayout>) {
    spawn_layout(&mut commands, &assets, &layout, &[]);
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
    mut prompt: ResMut<InteractPrompt>,
    blocked: Res<GameplayBlocked>,
    mut requests: MessageWriter<PickupRequest>,
    pickups: Query<(&Pickup, &Transform)>,
    mut player: Query<(&Transform, &CharacterAction, &mut PlayerInput, Has<DeathSequence>), With<Player>>,
) {
    let Ok((tf, action, mut input, dying)) = player.single_mut() else { return };
    if blocked.0 || dying || action.current.is_some() {
        return;
    }
    let dist = |p: &Transform| (p.translation - tf.translation).with_y(0.0).length();
    let level = |p: &Transform| (p.translation.y - tf.translation.y).abs() < 2.0;
    let Some((pickup, _)) = pickups.iter().filter(|(_, p)| dist(p) < PICKUP_RANGE && level(p)).min_by(|a, b| dist(a.1).total_cmp(&dist(b.1))) else {
        return;
    };
    prompt.0 = Some(strings::PROMPT_PICK_UP);
    if std::mem::take(&mut input.interact) {
        requests.write(PickupRequest { pickup: pickup.id });
    }
}

/// Offline / host: our own requests are granted right here (if the item is still there).
fn grant_pickups(
    mut commands: Commands,
    authority: Res<PickupAuthority>,
    mut requests: MessageReader<PickupRequest>,
    mut picked: MessageWriter<ItemPickedUp>,
    mut taken: MessageWriter<PickupTaken>,
    pickups: Query<(Entity, &Pickup)>,
    mut player: Query<&mut Inventory, With<Player>>,
) {
    if !authority.0 {
        return; // a guest: `net::world` forwards the requests to the host
    }
    for req in requests.read() {
        let Some((entity, pickup)) = pickups.iter().find(|(_, p)| p.id == req.pickup) else { continue };
        let Ok(mut inventory) = player.single_mut() else { continue };
        inventory.add(pickup.item);
        picked.write(ItemPickedUp { item: pickup.item });
        taken.write(PickupTaken { pickup: pickup.id, item: pickup.item });
        commands.entity(entity).despawn();
        info!("picked up {}", pickup.item.def().name);
    }
}
