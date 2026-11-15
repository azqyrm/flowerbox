//! The shared world: the host's world is the only one.
//!
//! - **Joining:** a guest stashes its own world (sunflower, which items it had taken), clears its
//!   items, and gets the host's world ([`Reliable::World`]): the sunflower's state and the items
//!   lying around. Leaving restores the guest's own world.
//! - **Items:** pressing E sends a request; the host decides (first come, first served), and
//!   everybody hears [`WorldEvent::PickedUp`]: the taker gets the item, the item vanishes for all.
//! - **Sunflowers** (by id): whoever revives one sees the big banner at once; the host records it
//!   and the others get a notification while that sunflower stands up for them too.
//! - **Doors** (by id): like items — a guest asks, the host opens it and tells everyone.
//! - **Notifications** for joins, leaves, deaths, pickups and revivals by others.

use bevy::prelude::*;

use super::protocol::*;
use super::transport::{HOST_PEER, NetCmd, NetLink};
use super::{NetInbox, NetSession, Role};
use crate::character::action::{ActionKind, ActionPhase, CharacterAction};
use crate::character::Remote;
use crate::character::stats::Health;
use crate::combat::{Dummy, DummyStruck};
use crate::hearth::{DeathSequence, Hearth, RestComplete, SharedRest, SunRevived};
use crate::tuning::REST_STARTUP;
use super::sync::Puppet;
use crate::items::inventory::Inventory;
use crate::items::pickup::{
    ClaimedDrops, ItemDropped, ItemPickedUp, Pickup, PickupAssets, PickupAuthority, PickupLayout, PickupRequest, PickupTaken, UniqueDrops, claim,
    spawn_layout, spawn_pickup,
};
use crate::level::door::{Door, DoorOpened, DoorRefused, DoorRequest};
use crate::player::Player;
use crate::pvp::PvpChanged;
use crate::ui::icons::NoticeIcon;
use crate::ui::notifications::Notify;
use crate::ui::strings;

pub struct WorldSyncPlugin;

impl Plugin for WorldSyncPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (world_inbox, local_events, shared_rest_flag, host_rest_circle).chain());
    }
}

/// "Host" or "Guest 1"… by player id.
pub fn player_name(id: NetId) -> String {
    if id == PLAYER_ID_BASE { "Host".into() } else { format!("Guest {}", id.saturating_sub(PLAYER_ID_BASE)) }
}

/// The guest's own world while it visits the host's.
#[derive(Default)]
struct Stash {
    suns: Vec<u32>,
    doors: Vec<u32>,
    taken: Vec<u32>,
}

/// Ids of the revived sunflowers.
fn revived(hearths: &Query<&mut Hearth>) -> Vec<u32> {
    let mut ids: Vec<u32> = hearths.iter().filter(|h| h.kindled).map(|h| h.id).collect();
    ids.sort_unstable();
    ids
}

/// Ids of the open doors.
fn opened(doors: &Query<&mut Door>) -> Vec<u32> {
    let mut ids: Vec<u32> = doors.iter().filter(|d| d.open).map(|d| d.id).collect();
    ids.sort_unstable();
    ids
}

fn notice(notify: &mut MessageWriter<Notify>, event: &WorldEvent) {
    let n = match *event {
        WorldEvent::Joined { who } => Notify::new(NoticeIcon::Joined, strings::joined(&player_name(who))),
        WorldEvent::Left { who } => Notify::new(NoticeIcon::Left, strings::left(&player_name(who))),
        WorldEvent::SunRevived { by, .. } => Notify::new(NoticeIcon::Sunflower, strings::revived_sun(&player_name(by))),
        WorldEvent::PickedUp { by, item, .. } => {
            Notify::new(NoticeIcon::Item(item.def().icon), strings::picked_up_by(&player_name(by), item.def().name))
        }
        WorldEvent::Died { who } => Notify::new(NoticeIcon::Died, strings::composted(&player_name(who))),
        WorldEvent::Pvp { who, on, item } => Notify::new(NoticeIcon::Item(item.def().icon), strings::ate(&player_name(who), item.def().name, on)),
        WorldEvent::RestWaiting { who, resting, total } => Notify::new(NoticeIcon::Sunflower, strings::rest_waiting(&player_name(who), resting, total)),
        WorldEvent::RestComplete => Notify::new(NoticeIcon::Sunflower, strings::REST_COMPLETE),
        WorldEvent::DoorTried { who } => Notify::new(NoticeIcon::Door, strings::door_tried(&player_name(who))),
        WorldEvent::DummyHit { .. } | WorldEvent::DoorOpened { .. } | WorldEvent::Dropped { .. } => return,
    };
    info!("world event: {}", n.text);
    notify.write(n);
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn world_inbox(
    mut commands: Commands,
    mut inbox: MessageReader<NetInbox>,
    link: Res<NetLink>,
    session: Res<NetSession>,
    assets: Option<Res<PickupAssets>>,
    (layout, unique, mut claimed): (Res<PickupLayout>, Res<UniqueDrops>, ResMut<ClaimedDrops>),
    mut stash: Local<Option<Stash>>,
    mut authority: ResMut<PickupAuthority>,
    mut notify: MessageWriter<Notify>,
    mut picked: MessageWriter<ItemPickedUp>,
    (mut hearths, mut doors): (Query<&mut Hearth>, Query<&mut Door>),
    pickups: Query<(Entity, &Pickup, &Transform)>,
    mut player: Query<&mut Inventory, With<Player>>,
    mut rest_done: MessageWriter<RestComplete>,
    mut struck: MessageWriter<DummyStruck>,
    mut dummy: Query<(Entity, &mut Health), With<Dummy>>,
) {
    let Some(assets) = assets else { return };
    let broadcast = |event: WorldEvent| link.send(NetCmd::Reliable { to: None, bytes: encode(&Reliable::Event(event)) });
    for msg in inbox.read() {
        match msg {
            NetInbox::RoleChanged if session.role == Role::Client => {
                // Visiting: put our own world away; the host's arrives in `World`.
                let present: Vec<u32> = pickups.iter().map(|(_, p, _)| p.id).collect();
                *stash = Some(Stash { suns: revived(&hearths), doors: opened(&doors), taken: layout.taken(&present) });
                for (e, _, _) in &pickups {
                    commands.entity(e).despawn();
                }
                authority.0 = false;
                // The host's dummy is the real one: ours only mirrors it.
                for (e, _) in &dummy {
                    commands.entity(e).insert(Remote);
                }
            }
            NetInbox::RoleChanged if session.role == Role::Offline => {
                // Back home: our own world as we left it.
                if let Some(own) = stash.take() {
                    for (e, _, _) in &pickups {
                        commands.entity(e).despawn();
                    }
                    spawn_layout(&mut commands, &assets, &layout, &own.taken);
                    for mut h in &mut hearths {
                        h.kindled = own.suns.contains(&h.id);
                    }
                    for mut d in &mut doors {
                        let open = own.doors.contains(&d.id);
                        d.set_now(open);
                    }
                }
                authority.0 = true;
                for (e, _) in &dummy {
                    commands.entity(e).remove::<Remote>();
                }
            }
            NetInbox::GuestJoined { peer, id } if session.is_host() => {
                let world = Reliable::World {
                    dummy_hp: dummy.iter().next().map_or(0.0, |(_, h)| h.current),
                    suns: revived(&hearths),
                    doors: opened(&doors),
                    pickups: pickups.iter().map(|(_, p, tf)| PickupState { id: p.id, item: p.item, pos: tf.translation.to_array() }).collect(),
                };
                link.send(NetCmd::Reliable { to: Some(*peer), bytes: encode(&world) });
                let event = WorldEvent::Joined { who: *id };
                broadcast(event);
                notice(&mut notify, &event);
            }
            NetInbox::GuestLeft { id } if session.is_host() => {
                let event = WorldEvent::Left { who: *id };
                broadcast(event);
                notice(&mut notify, &event);
            }
            // ── Host: guests' requests and reports ──
            NetInbox::Reliable(peer, Reliable::RequestPickup { pickup }) if session.is_host() => {
                let Some(&by) = session.peers.get(peer) else { continue };
                let Some((e, p, _)) = pickups.iter().find(|(_, p, _)| p.id == *pickup) else { continue };
                commands.entity(e).despawn();
                claim(&unique, &mut claimed, p.id);
                let event = WorldEvent::PickedUp { by, pickup: p.id, item: p.item };
                broadcast(event);
                notice(&mut notify, &event);
            }
            NetInbox::Reliable(peer, Reliable::RequestDoor { id }) if session.is_host() => {
                let Some(&by) = session.peers.get(peer) else { continue };
                let Some(mut door) = doors.iter_mut().find(|d| d.id == *id && !d.open) else { continue };
                door.open = true;
                broadcast(WorldEvent::DoorOpened { by, id: *id });
                info!("world event: {} opened door {id}", player_name(by));
            }
            NetInbox::Reliable(peer, Reliable::Report(report)) if session.is_host() => {
                let Some(&by) = session.peers.get(peer) else { continue };
                // Trust only what a guest says about itself.
                let event = match *report {
                    WorldEvent::SunRevived { id, .. } => {
                        let Some(mut h) = hearths.iter_mut().find(|h| h.id == id && !h.kindled) else { continue };
                        h.kindled = true;
                        WorldEvent::SunRevived { by, id }
                    }
                    WorldEvent::Died { .. } => WorldEvent::Died { who: by },
                    WorldEvent::DoorTried { .. } => WorldEvent::DoorTried { who: by },
                    WorldEvent::Pvp { on, item, .. } => WorldEvent::Pvp { who: by, on, item },
                    _ => continue,
                };
                broadcast(event);
                notice(&mut notify, &event);
            }
            // ── Guest: the host's world ──
            NetInbox::Reliable(HOST_PEER, Reliable::World { suns, doors: open, pickups: list, dummy_hp }) if session.role == Role::Client => {
                for (_, mut h) in &mut dummy {
                    h.current = *dummy_hp;
                }
                for mut h in &mut hearths {
                    h.kindled = suns.contains(&h.id);
                }
                for mut d in &mut doors {
                    let is_open = open.contains(&d.id);
                    d.set_now(is_open);
                }
                for p in list {
                    spawn_pickup(&mut commands, &assets, p.id, p.item, Vec3::from_array(p.pos));
                }
            }
            NetInbox::Reliable(HOST_PEER, Reliable::Event(event)) if session.role == Role::Client => {
                let mine = |id: &NetId| *id == session.my_id;
                match event {
                    WorldEvent::Joined { who }
                    | WorldEvent::Left { who }
                    | WorldEvent::Died { who }
                    | WorldEvent::Pvp { who, .. }
                    | WorldEvent::DoorTried { who }
                        if mine(who) =>
                    {
                        continue
                    }
                    WorldEvent::SunRevived { by, id } => {
                        for mut h in hearths.iter_mut().filter(|h| h.id == *id) {
                            h.kindled = true;
                        }
                        if mine(by) {
                            continue; // we already saw the banner
                        }
                    }
                    WorldEvent::Dropped { id, item, pos } => {
                        info!("world event: an enemy dropped {}", item.def().name);
                        spawn_pickup(&mut commands, &assets, *id, *item, Vec3::from_array(*pos));
                        continue;
                    }
                    WorldEvent::DoorOpened { id, .. } => {
                        for mut d in doors.iter_mut().filter(|d| d.id == *id) {
                            d.open = true;
                        }
                        continue;
                    }
                    WorldEvent::RestComplete => {
                        rest_done.write(RestComplete);
                    }
                    WorldEvent::DummyHit { amount, from, killed, hp } => {
                        for (_, mut h) in &mut dummy {
                            h.current = *hp;
                        }
                        struck.write(DummyStruck { amount: *amount, from: from.map(Vec3::from_array), killed: *killed });
                        continue;
                    }
                    WorldEvent::PickedUp { by, pickup, item } => {
                        if let Some((e, _, _)) = pickups.iter().find(|(_, p, _)| p.id == *pickup) {
                            commands.entity(e).despawn();
                        }
                        if mine(by) {
                            if let Ok(mut inv) = player.single_mut() {
                                inv.add(*item);
                            }
                            picked.write(ItemPickedUp { item: *item });
                            continue;
                        }
                    }
                    _ => {}
                }
                notice(&mut notify, event);
            }
            _ => {}
        }
    }
}

/// Our own doings that the world should hear about.
#[allow(clippy::too_many_arguments)]
fn local_events(
    link: Res<NetLink>,
    session: Res<NetSession>,
    mut requests: MessageReader<PickupRequest>,
    mut taken: MessageReader<PickupTaken>,
    mut revived: MessageReader<SunRevived>,
    mut door_requests: MessageReader<DoorRequest>,
    mut doors_opened: MessageReader<DoorOpened>,
    (mut doors_refused, mut drops): (MessageReader<DoorRefused>, MessageReader<ItemDropped>),
    mut pvp: MessageReader<PvpChanged>,
    mut struck: MessageReader<DummyStruck>,
    dummy: Query<&Health, With<Dummy>>,
    died: Query<(), (With<Player>, Added<DeathSequence>)>,
) {
    // Host: every dummy hit (ours or a guest's) goes to everyone.
    let dummy_hits: Vec<WorldEvent> = struck
        .read()
        .map(|h| WorldEvent::DummyHit { amount: h.amount, from: h.from.map(|f| f.to_array()), killed: h.killed, hp: dummy.iter().next().map_or(0.0, |d| d.current) })
        .collect();
    // Eating a Poppy / Camomile is news; losing PvP by dying is covered by the death notice.
    let pvp_events: Vec<WorldEvent> =
        pvp.read().filter_map(|c| c.item.map(|item| WorldEvent::Pvp { who: session.my_id, on: c.on, item })).collect();
    let send = |to: Option<u32>, msg: Reliable| link.send(NetCmd::Reliable { to, bytes: encode(&msg) });
    let me = session.my_id;
    match session.role {
        Role::Hosting => {
            requests.clear(); // granted locally (`grant_pickups`)
            for t in taken.read() {
                send(None, Reliable::Event(WorldEvent::PickedUp { by: me, pickup: t.pickup, item: t.item }));
            }
            for r in revived.read() {
                send(None, Reliable::Event(WorldEvent::SunRevived { by: me, id: r.id }));
            }
            door_requests.clear(); // granted locally (`grant_doors`)
            for d in doors_opened.read() {
                send(None, Reliable::Event(WorldEvent::DoorOpened { by: me, id: d.id }));
            }
            for _ in doors_refused.read() {
                send(None, Reliable::Event(WorldEvent::DoorTried { who: me }));
            }
            for d in drops.read() {
                send(None, Reliable::Event(WorldEvent::Dropped { id: d.id, item: d.item, pos: d.pos.to_array() }));
            }
            if !died.is_empty() {
                send(None, Reliable::Event(WorldEvent::Died { who: me }));
            }
            for e in pvp_events.into_iter().chain(dummy_hits) {
                send(None, Reliable::Event(e));
            }
        }
        Role::Client => {
            taken.clear();
            for r in requests.read() {
                send(Some(HOST_PEER), Reliable::RequestPickup { pickup: r.pickup });
            }
            for r in revived.read() {
                send(Some(HOST_PEER), Reliable::Report(WorldEvent::SunRevived { by: me, id: r.id }));
            }
            doors_opened.clear();
            drops.clear(); // guests have no real enemies
            for d in door_requests.read() {
                send(Some(HOST_PEER), Reliable::RequestDoor { id: d.id });
            }
            for _ in doors_refused.read() {
                send(Some(HOST_PEER), Reliable::Report(WorldEvent::DoorTried { who: me }));
            }
            if !died.is_empty() {
                send(Some(HOST_PEER), Reliable::Report(WorldEvent::Died { who: me }));
            }
            for e in pvp_events {
                send(Some(HOST_PEER), Reliable::Report(e));
            }
        }
        _ => {
            requests.clear();
            taken.clear();
            revived.clear();
            door_requests.clear();
            doors_opened.clear();
            doors_refused.clear();
            drops.clear();
        }
    }
}

/// Online, resting is shared: the host decides when the rest completes.
fn shared_rest_flag(session: Res<NetSession>, mut shared: ResMut<SharedRest>) {
    shared.set_if_neq(SharedRest(session.online()));
}

/// Host: who is resting at the sunflower (us + guests' puppets). Each new rester is announced
/// ("Host waiting for other players at rest site 1/2"); when all rest, the rest completes for
/// everyone (enemies respawn, everyone restored). It can complete again once someone got up.
#[allow(clippy::too_many_arguments)]
fn host_rest_circle(
    session: Res<NetSession>,
    link: Res<NetLink>,
    mut was_resting: Local<Vec<NetId>>,
    mut completed: Local<bool>,
    mut notify: MessageWriter<Notify>,
    mut done: MessageWriter<RestComplete>,
    me: Query<&CharacterAction, With<Player>>,
    puppets: Query<&Puppet>,
) {
    if !session.is_host() {
        was_resting.clear();
        *completed = false;
        return;
    }
    let mut resting: Vec<NetId> = vec![];
    if me.single().is_ok_and(|a| matches!(a.current, Some(c) if c.kind == ActionKind::Rest && c.phase() == ActionPhase::Active)) {
        resting.push(session.my_id);
    }
    for p in &puppets {
        let down = p.state.action.is_some_and(|a| a.kind == ActionKind::Rest && a.elapsed >= REST_STARTUP);
        if down && session.peers.values().any(|id| *id == p.id) {
            resting.push(p.id);
        }
    }
    let total = 1 + session.peers.len() as u32;
    let count = resting.len() as u32;
    let broadcast = |event: WorldEvent| link.send(NetCmd::Reliable { to: None, bytes: encode(&Reliable::Event(event)) });
    let newcomers: Vec<NetId> = resting.iter().copied().filter(|id| !was_resting.contains(id)).collect();
    // A rest completes once per gathering: only someone newly sitting down starts a new one (not,
    // say, the last other player leaving the session while the rest are still kneeling).
    if !newcomers.is_empty() {
        *completed = false;
    }
    if total > 1 && count < total {
        for who in &newcomers {
            let event = WorldEvent::RestWaiting { who: *who, resting: count, total };
            broadcast(event);
            notice(&mut notify, &event);
        }
    }
    if count == total && count > 0 && !*completed {
        *completed = true;
        done.write(RestComplete);
        if total > 1 {
            broadcast(WorldEvent::RestComplete);
            notice(&mut notify, &WorldEvent::RestComplete);
        }
    }
    *was_resting = resting;
}
