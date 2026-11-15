//! Doors: a wooden leaf on a hinge. Near a closed door, E opens it; it swings open over
//! `DOOR_OPEN_TIME` and stays open (saved, and shared online: the world's owner decides, like
//! pickups).
//!
//! - **One way:** a door only opens from the side it swings away from (`opens` points away from
//!   the opener). From the other side E still works but only tells you so (a notification; online
//!   everyone hears that you tried: [`DoorRefused`]).
//! - **Locked doors** need their key item in the bag (it isn't used up); without it: "Locked".
//! - **Collision:** the leaf blocks through a separate static [`DoorBlocker`] collider that exists
//!   only while the door is at rest (closed or open), spawned fresh where it rests. (Moving a
//!   static collider and toggling `ColliderDisabled` left the open door's collider behind: you
//!   could walk through opened doors.) While swinging nothing blocks, so it can't trap anyone.
//!
//! [`DoorRequest`] (local E, allowed) → offline/host: [`DoorOpened`] at once (net tells everyone);
//! a guest's request goes to the host (`crate::net::world`), whose answer opens it here.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::layout::DoorPlan;
use crate::character::action::CharacterAction;
use crate::character::controller::CharacterController;
use crate::hearth::DeathSequence;
use crate::input::{GameplayBlocked, PlayerInput};
use crate::interact::{InteractPrompt, InteractSystems};
use crate::items::ItemId;
use crate::items::inventory::Inventory;
use crate::items::pickup::PickupAuthority;
use crate::ui::icons::NoticeIcon;
use crate::ui::notifications::Notify;
use crate::player::Player;
use crate::tuning::*;
use crate::ui::strings;

pub struct DoorPlugin;

impl Plugin for DoorPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DoorRequest>()
            .add_message::<DoorOpened>()
            .add_message::<DoorRefused>()
            .add_systems(Update, (door_interaction.in_set(InteractSystems::Doors), grant_doors, swing_doors).chain());
    }
}

#[derive(Component, Debug)]
pub struct Door {
    pub id: u32,
    pub open: bool,
    /// The key item it needs, if locked.
    pub key: Option<ItemId>,
    /// Shown opening 0 (closed) … 1 (open).
    swing: f32,
    hinge: Vec3,
    width: f32,
    closed_yaw: f32,
    open_yaw: f32,
    /// Direction the leaf swings to (x, z): it opens only from the other side.
    opens: Vec2,
    /// The collider while at rest.
    blocker: Option<Entity>,
}

/// The collider of a door at rest (see the module docs).
#[derive(Component, Debug)]
pub struct DoorBlocker;

impl Door {
    /// Open without the swing (loading a save, joining a world).
    pub fn open_now(&mut self) {
        self.set_now(true);
    }

    /// Open or closed at once, without the swing.
    pub fn set_now(&mut self, open: bool) {
        self.open = open;
        self.swing = if open { 1.0 } else { 0.0 };
    }

    /// Middle of the doorway on the floor.
    fn centre(&self) -> Vec3 {
        self.hinge + Quat::from_rotation_y(self.closed_yaw) * Vec3::X * self.width * 0.5
    }

    /// Whether someone standing at `pos` is on the side the door can be opened from (the side it
    /// swings away from).
    pub fn opens_from(&self, pos: Vec3) -> bool {
        let to = (pos - self.centre()).with_y(0.0);
        Vec2::new(to.x, to.z).dot(self.opens) <= 0.0
    }

    fn transform(&self) -> Transform {
        let k = ease(self.swing);
        let d = (self.open_yaw - self.closed_yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        let rot = Quat::from_rotation_y(self.closed_yaw + d * k);
        Transform::from_translation(self.hinge + rot * Vec3::new(self.width * 0.5, DOOR_HEIGHT * 0.5, 0.0)).with_rotation(rot)
    }
}

/// Heavy start, soft stop.
fn ease(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// The local player wants door `id` opened (from the right side, with the key if it needs one).
#[derive(Message, Debug, Clone, Copy)]
pub struct DoorRequest {
    pub id: u32,
}

/// This machine opened door `id` for its own player (the host tells the others).
#[derive(Message, Debug, Clone, Copy)]
pub struct DoorOpened {
    pub id: u32,
}

/// The local player tried a door from the side it doesn't open from (others hear about it).
#[derive(Message, Debug, Clone, Copy)]
pub struct DoorRefused;

/// Yaw that turns local +X toward `v` (x, z).
fn yaw_of(v: Vec2) -> f32 {
    f32::atan2(-v.y, v.x)
}

const LEAF_THICKNESS: f32 = 0.12;

pub fn spawn_door(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, plan: &DoorPlan) {
    let wood = materials.add(StandardMaterial { base_color: Color::srgb(0.3, 0.19, 0.1), perceptual_roughness: 0.85, ..default() });
    let dark = materials.add(StandardMaterial { base_color: Color::srgb(0.2, 0.12, 0.06), perceptual_roughness: 0.9, ..default() });
    let iron = materials.add(StandardMaterial { base_color: Color::srgb(0.1, 0.1, 0.1), metallic: 0.7, perceptual_roughness: 0.5, ..default() });
    let h = plan.hinge_point();
    let width = plan.opening;
    let door = Door {
        id: plan.id,
        open: false,
        key: plan.key,
        swing: 0.0,
        hinge: Vec3::new(h.x, plan.floor, h.y),
        width,
        closed_yaw: yaw_of(-plan.hinge.vec()),
        open_yaw: yaw_of(plan.opens.vec()),
        opens: plan.opens.vec(),
        blocker: None,
    };
    let thick = LEAF_THICKNESS;
    let tf = door.transform();
    let locked = door.key.is_some();
    let leaf = commands.spawn((Name::new(format!("Door {}", plan.id)), door, tf, Visibility::default())).id();
    let mut part = |mesh: Mesh, mat: &Handle<StandardMaterial>, at: Vec3| {
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat.clone()), Transform::from_translation(at), ChildOf(leaf)));
    };
    // Vertical planks with dark gaps, two iron bands, a ring pull on both faces.
    let planks = (width / 0.32).round().max(2.0) as usize;
    let pw = width / planks as f32;
    for i in 0..planks {
        let x = -width * 0.5 + pw * (i as f32 + 0.5);
        let mat = if i % 2 == 0 { &wood } else { &dark };
        part(Cuboid::new(pw - 0.015, DOOR_HEIGHT - 0.02, thick).into(), mat, Vec3::new(x, 0.0, 0.0));
    }
    for y in [-DOOR_HEIGHT * 0.3, DOOR_HEIGHT * 0.3] {
        part(Cuboid::new(width - 0.04, 0.1, thick + 0.03).into(), &iron, Vec3::new(0.0, y, 0.0));
    }
    for z in [-1.0, 1.0] {
        part(Torus::new(0.05, 0.075).into(), &iron, Vec3::new(width * 0.35, -0.25, z * (thick * 0.5 + 0.03)));
        if locked {
            // A big iron lock plate with a keyhole.
            part(Cuboid::new(0.18, 0.26, 0.04).into(), &iron, Vec3::new(width * 0.35, 0.05, z * (thick * 0.5 + 0.02)));
        }
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn door_interaction(
    mut prompt: ResMut<InteractPrompt>,
    blocked: Res<GameplayBlocked>,
    mut requests: MessageWriter<DoorRequest>,
    mut refused: MessageWriter<DoorRefused>,
    mut notify: MessageWriter<Notify>,
    doors: Query<&Door>,
    mut player: Query<(&Transform, &CharacterAction, &CharacterController, &mut PlayerInput, &Inventory, Has<DeathSequence>), With<Player>>,
) {
    let Ok((tf, action, ctrl, mut input, inventory, dying)) = player.single_mut() else { return };
    if prompt.0.is_some() || blocked.0 || dying || action.current.is_some() || !ctrl.grounded {
        return;
    }
    let feet = tf.translation - Vec3::Y * PLAYER_HALF_HEIGHT;
    let near = doors.iter().filter(|d| !d.open).find(|d| {
        let c = d.centre();
        (c - feet).with_y(0.0).length() < DOOR_RANGE && (c.y - feet.y).abs() < 1.0
    });
    let Some(door) = near else { return };
    prompt.0 = Some(strings::PROMPT_OPEN_DOOR);
    if !std::mem::take(&mut input.interact) {
        return;
    }
    if !door.opens_from(feet) {
        notify.write(Notify::new(NoticeIcon::Door, strings::DOOR_OTHER_SIDE.to_string()));
        refused.write(DoorRefused);
        info!("door {} only opens from the other side", door.id);
    } else if let Some(key) = door.key.filter(|k| inventory.count(*k) == 0) {
        notify.write(Notify::new(NoticeIcon::Door, strings::door_locked(key.def().name)));
        info!("door {} is locked ({} needed)", door.id, key.def().name);
    } else {
        if let Some(key) = door.key {
            notify.write(Notify::new(NoticeIcon::Item(key.def().icon), strings::door_unlocked(key.def().name)));
        }
        requests.write(DoorRequest { id: door.id });
    }
}

/// Offline / host: our own requests open the door right here.
fn grant_doors(
    authority: Res<PickupAuthority>,
    mut requests: MessageReader<DoorRequest>,
    mut opened: MessageWriter<DoorOpened>,
    mut doors: Query<&mut Door>,
) {
    if !authority.0 {
        return; // a guest: `net::world` forwards the request to the host
    }
    for req in requests.read() {
        if let Some(mut door) = doors.iter_mut().find(|d| d.id == req.id && !d.open) {
            door.open = true;
            opened.write(DoorOpened { id: req.id });
            info!("opened door {}", req.id);
        }
    }
}

/// Swings opening doors; at rest a fresh blocker collider stands where the leaf is.
fn swing_doors(time: Res<Time>, mut commands: Commands, mut doors: Query<(&mut Door, &mut Transform)>) {
    for (mut door, mut tf) in &mut doors {
        let target = if door.open { 1.0 } else { 0.0 };
        if door.swing == target {
            if door.is_changed() || door.blocker.is_none() {
                *tf = door.transform(); // also: opened at once (a save, a joined world)
                if let Some(old) = door.blocker.take() {
                    commands.entity(old).despawn();
                }
                let blocker = commands
                    .spawn((
                        Name::new(format!("Door {} blocker", door.id)),
                        DoorBlocker,
                        *tf,
                        RigidBody::Static,
                        Collider::cuboid(door.width, DOOR_HEIGHT, LEAF_THICKNESS),
                    ))
                    .id();
                door.blocker = Some(blocker);
            }
            continue;
        }
        if let Some(old) = door.blocker.take() {
            commands.entity(old).despawn(); // nothing blocks while it swings
        }
        let step = time.delta_secs() / DOOR_OPEN_TIME;
        door.swing = if door.swing < target { (door.swing + step).min(target) } else { (door.swing - step).max(target) };
        *tf = door.transform();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::layout::Dir;

    #[test]
    fn door_swings_from_the_jamb_to_the_open_side() {
        // Hinge on the east end of an east-west wall at z = 0, opening north (-Z).
        let mut d = Door {
            id: 1,
            open: false,
            swing: 0.0,
            hinge: Vec3::new(1.0, 0.0, 0.0),
            width: 2.0,
            closed_yaw: yaw_of(-Dir::East.vec()),
            open_yaw: yaw_of(Dir::North.vec()),
            opens: Dir::North.vec(),
            key: None,
            blocker: None,
        };
        let closed = d.transform().translation;
        assert!((closed - Vec3::new(0.0, DOOR_HEIGHT * 0.5, 0.0)).length() < 1e-4, "{closed}");
        assert!((d.centre() - Vec3::ZERO).length() < 1e-4);
        d.open_now();
        let open = d.transform().translation;
        assert!((open - Vec3::new(1.0, DOOR_HEIGHT * 0.5, -1.0)).length() < 1e-4, "{open}");
        // It swings north, so it opens from the south side only.
        assert!(d.opens_from(Vec3::new(0.0, 0.0, 1.5)));
        assert!(!d.opens_from(Vec3::new(0.0, 0.0, -1.5)));
    }
}
