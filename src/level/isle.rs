//! The Storm Isle's props (level 5): wind-worn rocks, broken walls, the storm's eye (where the
//! ring closes to, `crate::storm`) and **loot chests** in three tiers — common (grey), rare
//! (violet), epic (gold) — whose glow tells you what you might find. E opens one: it rolls an item
//! from its tier's table (fixed per chest) and puts it on the ground before it. Each chest's item
//! is found once per world (its id is a unique drop: `ClaimedDrops`); open again later, it's empty.
//! Online the host opens chests (a guest asks with `Reliable::RequestChest`) and everyone sees the
//! item appear; the lid swings on the machine of whoever opened it.

use std::f32::consts::FRAC_PI_2;

use avian3d::prelude::*;
use bevy::prelude::*;

use super::layout::{Layout, PropDef, PropKind};
use crate::hearth::DeathSequence;
use crate::input::{GameplayBlocked, PlayerInput};
use crate::interact::{InteractPrompt, InteractSystems};
use crate::items::ItemId;
use crate::items::pickup::{ClaimedDrops, ItemDropped, Pickup, PickupAssets, spawn_pickup};
use crate::net::protocol::{Reliable, encode};
use crate::net::transport::{HOST_PEER, NetCmd, NetLink};
use crate::net::{NetInbox, NetSession};
use crate::player::Player;
use crate::tuning::*;
use crate::ui::icons::NoticeIcon;
use crate::ui::notifications::Notify;
use crate::ui::strings;

pub struct IslePlugin;

impl Plugin for IslePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<OpenChest>().add_systems(Update, (chest_interaction.in_set(InteractSystems::Pickups), chest_requests, open_chests, swing_lids).chain());
    }
}

/// A loot chest (`tier` 1..=3).
#[derive(Component, Debug)]
pub struct Chest {
    pub id: u32,
    pub tier: u8,
    pub open: bool,
    /// Where its item lands.
    front: Vec3,
    lid: Entity,
    swing: f32,
}

/// Open chest `id` (this machine owns the world: offline or host).
#[derive(Message, Debug, Clone, Copy)]
pub struct OpenChest {
    pub id: u32,
}

/// What a chest of `tier` holds, rolled from its `id` (the same every time).
pub fn chest_loot(tier: u8, id: u32) -> ItemId {
    let table: &[ItemId] = match tier {
        1 => &[ItemId::Poppy, ItemId::Camomile, ItemId::HopCone, ItemId::Poppy],
        2 => &[ItemId::HopCone, ItemId::ThornbriarRing, ItemId::StuddedBuckler, ItemId::WoodcutterHatchet, ItemId::CaulHood],
        _ => &[ItemId::TempestRing],
    };
    let mut h = id.wrapping_mul(2_654_435_761) ^ 0x9E37_79B9;
    h ^= h >> 15;
    table[(h as usize) % table.len()]
}

pub struct IsleMats {
    rock: Handle<StandardMaterial>,
    wood: Handle<StandardMaterial>,
    iron: Handle<StandardMaterial>,
    glows: [Handle<StandardMaterial>; 3],
}

impl IsleMats {
    pub fn new(materials: &mut Assets<StandardMaterial>) -> Self {
        let rock = materials.add(StandardMaterial {
            base_color: Color::srgb(0.22, 0.24, 0.27),
            base_color_texture: crate::textures::get(crate::textures::Tex::Flagstones),
            perceptual_roughness: 0.6,
            reflectance: 0.4,
            ..default()
        });
        let wood = materials.add(StandardMaterial { base_color: Color::srgb(0.3, 0.2, 0.12), base_color_texture: crate::textures::get(crate::textures::Tex::Planks), perceptual_roughness: 0.8, ..default() });
        let iron = materials.add(StandardMaterial { base_color: Color::srgb(0.18, 0.18, 0.2), metallic: 0.8, perceptual_roughness: 0.45, ..default() });
        let glow = |c: LinearRgba| StandardMaterial { base_color: Color::srgb(0.9, 0.9, 0.9), emissive: c, unlit: true, ..default() };
        let glows = [
            materials.add(glow(LinearRgba::rgb(1.6, 1.8, 2.0))),
            materials.add(glow(LinearRgba::rgb(3.5, 1.0, 6.0))),
            materials.add(glow(LinearRgba::rgb(8.0, 5.0, 1.0))),
        ];
        Self { rock, wood, iron, glows }
    }
}

const TIER_LIGHT: [Color; 3] = [Color::srgb(0.75, 0.8, 0.9), Color::srgb(0.7, 0.35, 1.0), Color::srgb(1.0, 0.75, 0.3)];

#[allow(clippy::too_many_arguments)]
pub fn spawn_isle_prop(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    m: &IsleMats,
    _layout: &Layout,
    prop: &PropDef,
    n: usize,
    root: Entity,
    frame: Transform,
) {
    fn piece(commands: &mut Commands, meshes: &mut Assets<Mesh>, parent: Entity, mesh: Mesh, mat: &Handle<StandardMaterial>, t: Transform) -> Entity {
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat.clone()), t, ChildOf(parent))).id()
    }
    let floor = frame.translation;
    let t = Transform::from_xyz;
    match prop.kind {
        PropKind::Rock => {
            // A heap of weathered boulders (`size.0` = scale).
            let s = if prop.size.0 > 0.0 { prop.size.0 } else { 1.0 };
            for k in 0..4u32 {
                let a = k as f32 * 2.2 + n as f32;
                let r = s * (0.55 + 0.25 * (k % 3) as f32);
                let at = Vec3::new(a.cos() * s * 0.7 * (k > 0) as u8 as f32, r * 0.6, a.sin() * s * 0.6 * (k > 0) as u8 as f32);
                piece(commands, meshes, root, Sphere::new(r).mesh().ico(1).unwrap(), &m.rock, Transform::from_translation(at).with_scale(Vec3::new(1.2, 0.8, 1.0)).with_rotation(Quat::from_rotation_y(a)));
                commands.spawn((RigidBody::Static, Collider::sphere(r * 0.85), Transform::from_translation(at), ChildOf(root)));
            }
        }
        PropKind::Ruin => {
            // A broken stretch of wall (`size` = length, height), jagged on top.
            let (len, h) = (prop.size.0.max(1.0), prop.size.1.max(1.0));
            piece(commands, meshes, root, Cuboid::new(len, h, 0.6).into(), &m.rock, t(0.0, h * 0.5, 0.0));
            for k in 0..((len / 0.9) as u32) {
                let x = -len * 0.5 + 0.45 + 0.9 * k as f32;
                let hh = 0.2 + 0.5 * ((k * 7 % 5) as f32 / 5.0);
                piece(commands, meshes, root, Cuboid::new(0.85, hh, 0.55).into(), &m.rock, t(x, h + hh * 0.5, 0.0));
            }
            commands.spawn((RigidBody::Static, Collider::cuboid(len, h, 0.6), t(0.0, h * 0.5, 0.0), ChildOf(root)));
        }
        PropKind::Chest => {
            let tier = (prop.size.0 as u8).clamp(1, 3);
            let ti = tier as usize - 1;
            piece(commands, meshes, root, Cuboid::new(1.0, 0.5, 0.6).into(), &m.wood, t(0.0, 0.25, 0.0));
            for x in [-0.42, 0.42] {
                piece(commands, meshes, root, Cuboid::new(0.08, 0.52, 0.62).into(), &m.iron, t(x, 0.26, 0.0));
            }
            piece(commands, meshes, root, Cuboid::new(0.2, 0.06, 0.02).into(), &m.glows[ti], t(0.0, 0.42, -0.31));
            // The lid hinges at the back.
            let lid = commands.spawn((Transform::from_xyz(0.0, 0.5, 0.3), Visibility::default(), ChildOf(root))).id();
            piece(commands, meshes, lid, Cuboid::new(1.02, 0.2, 0.62).into(), &m.wood, t(0.0, 0.1, -0.3));
            piece(commands, meshes, lid, Cuboid::new(1.04, 0.05, 0.64).into(), &m.glows[ti], t(0.0, 0.03, -0.3));
            commands.spawn((PointLight { color: TIER_LIGHT[ti], intensity: 6_000.0 * (1.0 + ti as f32), range: 3.5 + ti as f32, ..default() }, t(0.0, 0.9, -0.3), ChildOf(root)));
            commands.spawn((RigidBody::Static, Collider::cuboid(1.0, 0.6, 0.6), t(0.0, 0.3, 0.0), ChildOf(root)));
            let front = floor + frame.rotation * Vec3::new(0.0, 0.0, -1.0);
            commands.entity(root).insert(Chest { id: prop.id, tier, open: false, front, lid, swing: 0.0 });
        }
        PropKind::StormEye => {
            let start = if prop.size.0 > 0.0 { prop.size.0 } else { 40.0 };
            let end = if prop.size.1 > 0.0 { prop.size.1 } else { 9.0 };
            crate::storm::spawn_storm(commands, meshes, materials, floor, start, end);
            // A ring of standing stones round the eye.
            for k in 0..8 {
                let a = k as f32 / 8.0 * std::f32::consts::TAU;
                piece(commands, meshes, root, Cuboid::new(0.8, 2.6, 0.5).into(), &m.rock, Transform::from_xyz(a.cos() * end, 1.3, a.sin() * end).with_rotation(Quat::from_rotation_y(-a + FRAC_PI_2)));
            }
        }
        _ => {}
    }
}

/// E at a closed chest.
#[allow(clippy::type_complexity)]
fn chest_interaction(
    mut prompt: ResMut<InteractPrompt>,
    blocked: Res<GameplayBlocked>,
    session: Res<NetSession>,
    link: Res<NetLink>,
    mut chests: Query<(&mut Chest, &GlobalTransform)>,
    mut player: Query<(&Transform, &mut PlayerInput, Has<DeathSequence>), With<Player>>,
    mut open: MessageWriter<OpenChest>,
) {
    let Ok((tf, mut input, dying)) = player.single_mut() else { return };
    if prompt.0.is_some() || blocked.0 || dying {
        return;
    }
    let Some((mut chest, _)) = chests
        .iter_mut()
        .filter(|(c, g)| !c.open && (g.translation() - tf.translation).with_y(0.0).length() < CHEST_RANGE && (g.translation().y - tf.translation.y).abs() < 1.6)
        .min_by(|a, b| a.1.translation().distance(tf.translation).total_cmp(&b.1.translation().distance(tf.translation)))
    else {
        return;
    };
    prompt.0 = Some(strings::PROMPT_OPEN_CHEST);
    if !std::mem::take(&mut input.interact) {
        return;
    }
    chest.open = true; // the lid swings here at once
    if session.is_client() {
        link.send(NetCmd::Reliable { to: Some(HOST_PEER), bytes: encode(&Reliable::RequestChest { id: chest.id }) });
    } else {
        open.write(OpenChest { id: chest.id });
    }
}

/// Host: guests asking to open chests.
fn chest_requests(session: Res<NetSession>, mut inbox: MessageReader<NetInbox>, mut open: MessageWriter<OpenChest>) {
    for msg in inbox.read() {
        if let NetInbox::Reliable(_, Reliable::RequestChest { id }) = msg
            && session.is_host()
        {
            open.write(OpenChest { id: *id });
        }
    }
}

/// The world's owner opens a chest: its item lands before it (once per world).
#[allow(clippy::too_many_arguments)]
fn open_chests(
    mut commands: Commands,
    mut requests: MessageReader<OpenChest>,
    assets: Option<Res<PickupAssets>>,
    claimed: Res<ClaimedDrops>,
    pickups: Query<&Pickup>,
    mut chests: Query<&mut Chest>,
    mut dropped: MessageWriter<ItemDropped>,
    mut notify: MessageWriter<Notify>,
    mut sounds: MessageWriter<crate::sound::PlaySound>,
) {
    let Some(assets) = assets else { return };
    for r in requests.read() {
        let Some(mut chest) = chests.iter_mut().find(|c| c.id == r.id) else { continue };
        chest.open = true;
        sounds.write(crate::sound::PlaySound { sound: crate::sound::Sound::Click, pos: chest.front, volume: 0.9 });
        if claimed.0.contains(&chest.id) || pickups.iter().any(|p| p.id == chest.id) {
            notify.write(Notify::new(NoticeIcon::Door, strings::CHEST_EMPTY.to_string()));
            continue;
        }
        let item = chest_loot(chest.tier, chest.id);
        spawn_pickup(&mut commands, &assets, chest.id, item, chest.front);
        dropped.write(ItemDropped { id: chest.id, item, pos: chest.front, thrown: false });
        info!("a tier-{} chest held {}", chest.tier, item.def().name);
    }
}

fn swing_lids(time: Res<Time>, mut chests: Query<&mut Chest>, mut lids: Query<&mut Transform>) {
    for mut c in &mut chests {
        let want = if c.open { 1.0 } else { 0.0 };
        c.swing += (want - c.swing) * (1.0 - (-6.0 * time.delta_secs()).exp());
        if let Ok(mut tf) = lids.get_mut(c.lid) {
            tf.rotation = Quat::from_rotation_x(-1.9 * c.swing);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chests_roll_from_their_tier_and_always_the_same() {
        for id in 500..540 {
            assert_eq!(chest_loot(1, id), chest_loot(1, id));
            assert!(!chest_loot(1, id).def().rare);
        }
        assert_eq!(chest_loot(3, 7), ItemId::TempestRing);
    }
}
