//! Opt-in PvP and usable consumables.
//!
//! - Players can only hurt each other when **both** have PvP on ([`Pvp`]). Eating a **Poppy**
//!   turns it on, a **Camomile** off (`UseItem` action: hand to mouth, the effect lands at the
//!   start of the active phase; using it in a fight is a risk, like drinking).
//! - Being composted turns PvP off. Respawning gives you a Poppy if you have none left.
//! - PvP players wear a red poppy over their head (everyone sees it); your own state shows next to
//!   the HP bar.
//! - Online, the [`PvpChanged`] messages become notifications for the others (`crate::net`).
//!   Who hits whom between players is decided on the victim's machine (`combat::sword_hits`).

use bevy::prelude::*;

use crate::character::action::{ActionKind, ActionPhase, ActionRequest, CharacterAction};
use crate::character::{ActionIntent, CharacterSystems};
use crate::hearth::DeathSequence;
use crate::humanoid::{HumanoidRig, HumanoidSpec};
use crate::items::inventory::Inventory;
use crate::items::{ItemEffect, ItemIcon, ItemId};
use crate::player::Player;
use crate::ui::icons::{NoticeIcon, spawn_icon};
use crate::ui::notifications::Notify;
use crate::ui::strings;

pub struct PvpPlugin;

impl Plugin for PvpPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<UseItemRequest>()
            .add_message::<PvpChanged>()
            .add_systems(Startup, spawn_pvp_indicator)
            .add_systems(FixedUpdate, apply_item_use.after(CharacterSystems::Actions).before(CharacterSystems::Stats))
            .add_systems(Update, (start_item_use, pvp_ends_on_death, notify_own_pvp, poppy_on_respawn, pvp_markers, update_pvp_indicator));
    }
}

/// Whether this character takes part in PvP.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Pvp(pub bool);

/// The local player wants to use `item` from the bag (inventory screen).
#[derive(Message, Debug, Clone, Copy)]
pub struct UseItemRequest {
    pub item: ItemId,
}

/// The local player's PvP state changed (`item` = what caused it; `None` = being composted).
#[derive(Message, Debug, Clone, Copy)]
pub struct PvpChanged {
    pub on: bool,
    pub item: Option<ItemId>,
}

/// The consumable being eaten (set when the `UseItem` action is requested).
#[derive(Component, Debug)]
struct UsingItem {
    item: ItemId,
    /// Action serial whose effect was applied.
    applied: Option<u32>,
}

fn start_item_use(
    mut commands: Commands,
    mut requests: MessageReader<UseItemRequest>,
    mut notify: MessageWriter<Notify>,
    mut player: Query<(Entity, &Inventory, &mut ActionIntent), With<Player>>,
) {
    let Ok((entity, inventory, mut intent)) = player.single_mut() else { return };
    for req in requests.read() {
        if req.item == ItemId::HealingAle {
            intent.buffer(ActionRequest::DrinkAle);
            continue;
        }
        if req.item.def().effect.is_none() {
            continue;
        }
        if inventory.count(req.item) == 0 {
            notify.write(Notify::new(NoticeIcon::Item(req.item.def().icon), strings::none_left(req.item.def().name)));
            continue;
        }
        commands.entity(entity).insert(UsingItem { item: req.item, applied: None });
        intent.buffer(ActionRequest::UseItem);
    }
}

fn apply_item_use(
    mut changed: MessageWriter<PvpChanged>,
    mut q: Query<(&CharacterAction, &mut UsingItem, &mut Inventory, &mut Pvp)>,
) {
    for (action, mut using, mut inventory, mut pvp) in &mut q {
        let Some(cur) = action.current.filter(|a| a.kind == ActionKind::UseItem) else { continue };
        if cur.phase() != ActionPhase::Active || using.applied == Some(action.serial) {
            continue;
        }
        using.applied = Some(action.serial);
        if !inventory.consume(using.item) {
            continue;
        }
        let on = match using.item.def().effect {
            Some(ItemEffect::PvpOn) => true,
            Some(ItemEffect::PvpOff) => false,
            None => continue,
        };
        info!("ate a {}: PvP {}", using.item.def().name, if on { "on" } else { "off" });
        pvp.0 = on;
        changed.write(PvpChanged { on, item: Some(using.item) });
    }
}

/// Our own PvP changes as notifications.
fn notify_own_pvp(mut own: MessageReader<PvpChanged>, mut notify: MessageWriter<Notify>) {
    for c in own.read() {
        let (icon, text) = match c.item {
            Some(item) => (NoticeIcon::Item(item.def().icon), strings::ate("You", item.def().name, c.on)),
            None => (NoticeIcon::Item(ItemIcon::Camomile), strings::PVP_OFF_BY_DEATH.to_string()),
        };
        notify.write(Notify::new(icon, text));
    }
}

/// Being composted ends PvP.
fn pvp_ends_on_death(mut changed: MessageWriter<PvpChanged>, mut player: Query<(&CharacterAction, &mut Pvp), With<Player>>) {
    let Ok((action, mut pvp)) = player.single_mut() else { return };
    if pvp.0 && matches!(action.current, Some(a) if a.kind == ActionKind::Dead) {
        pvp.0 = false;
        changed.write(PvpChanged { on: false, item: None });
    }
}

/// Back from the dead: a Poppy if the bag has none.
fn poppy_on_respawn(mut removed: RemovedComponents<DeathSequence>, mut player: Query<&mut Inventory, With<Player>>) {
    for e in removed.read() {
        let Ok(mut inventory) = player.get_mut(e) else { continue };
        if inventory.count(ItemId::Poppy) == 0 {
            inventory.add(ItemId::Poppy);
            info!("respawned with a fresh Poppy");
        }
    }
}

// ── The poppy over PvP players' heads ────────────────────────────────────

#[derive(Component)]
struct PvpMarker(Entity);

#[allow(clippy::type_complexity)]
fn pvp_markers(
    mut commands: Commands,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    new: Query<(Entity, &HumanoidRig, &HumanoidSpec), (With<Pvp>, Without<PvpMarker>)>,
    players: Query<(&Pvp, &PvpMarker)>,
    mut markers: Query<(&mut Visibility, &mut Transform)>,
) {
    for (entity, rig, spec) in &new {
        let red = materials.add(StandardMaterial { base_color: Color::srgb(0.85, 0.08, 0.06), emissive: LinearRgba::rgb(1.6, 0.1, 0.05), ..default() });
        let dark = materials.add(StandardMaterial { base_color: Color::srgb(0.05, 0.04, 0.03), ..default() });
        let marker = commands
            .spawn((
                Name::new("PvP marker"),
                // On the body (not the head bone), so it floats steadily above the head.
                Transform::from_xyz(0.0, spec.pelvis_height + spec.spine + spec.neck_height + spec.head_radius * 2.0 + 0.18 - rig.root_height, 0.0),
                Visibility::Hidden,
                ChildOf(entity),
            ))
            .id();
        for i in 0..4 {
            let a = i as f32 / 4.0 * std::f32::consts::TAU;
            commands.spawn((
                Mesh3d(meshes.add(Sphere::new(0.055))),
                MeshMaterial3d(red.clone()),
                Transform::from_xyz(a.cos() * 0.05, 0.0, a.sin() * 0.05).with_scale(Vec3::new(1.0, 0.35, 1.0)),
                ChildOf(marker),
            ));
        }
        commands.spawn((Mesh3d(meshes.add(Sphere::new(0.03))), MeshMaterial3d(dark), Transform::from_xyz(0.0, 0.02, 0.0), ChildOf(marker)));
        commands.entity(entity).insert(PvpMarker(marker));
    }
    let spin = time.elapsed_secs() * 1.5;
    for (pvp, marker) in &players {
        if let Ok((mut vis, mut tf)) = markers.get_mut(marker.0) {
            vis.set_if_neq(if pvp.0 { Visibility::Inherited } else { Visibility::Hidden });
            // Tilted toward the camera-ish and slowly turning, so it reads as a flower.
            tf.rotation = Quat::from_rotation_y(spin) * Quat::from_rotation_x(0.5);
        }
    }
}

// ── HUD: own PvP state ───────────────────────────────────────────────────

#[derive(Component)]
struct PvpIndicator;

fn spawn_pvp_indicator(mut commands: Commands) {
    let row = commands
        .spawn((
            Name::new("PvP indicator"),
            PvpIndicator,
            Node { position_type: PositionType::Absolute, left: Val::Px(70.0), top: Val::Px(64.0), align_items: AlignItems::Center, column_gap: Val::Px(2.0), ..default() },
            Visibility::Hidden,
        ))
        .id();
    let icon = spawn_icon(&mut commands, row, ItemIcon::Poppy, 0.55);
    commands.entity(icon).insert(Node { width: Val::Px(31.0), height: Val::Px(31.0), ..default() });
    commands.spawn((
        Text::new(strings::PVP_ON),
        TextFont::from_font_size(16.0),
        TextColor(Color::srgb(0.95, 0.35, 0.3)),
        TextShadow { offset: Vec2::splat(1.5), color: Color::BLACK },
        ChildOf(row),
    ));
}

fn update_pvp_indicator(player: Query<&Pvp, (With<Player>, Changed<Pvp>)>, mut indicator: Query<&mut Visibility, With<PvpIndicator>>) {
    let Ok(pvp) = player.single() else { return };
    for mut v in &mut indicator {
        *v = if pvp.0 { Visibility::Inherited } else { Visibility::Hidden };
    }
}
