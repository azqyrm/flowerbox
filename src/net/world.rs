//! The shared world: the host's world is the only one.
//!
//! - **Joining:** a guest stashes its own world (sunflower, which items it had taken), clears its
//!   items, and gets the host's world ([`Reliable::World`]): the sunflower's state and the items
//!   lying around. Leaving restores the guest's own world.
//! - **Items:** pressing E sends a request; the host decides (first come, first served), and
//!   everybody hears [`WorldEvent::PickedUp`]: the taker gets the item, the item vanishes for all.
//! - **Sunflower:** whoever revives it sees the big banner at once; the host records it and the
//!   others get a notification while their sunflower stands up too.
//! - **Notifications** for joins, leaves, deaths, pickups and revivals by others.

use bevy::prelude::*;

use super::protocol::*;
use super::transport::{HOST_PEER, NetCmd, NetLink};
use super::{NetInbox, NetSession, Role};
use crate::hearth::{DeathSequence, Hearth, SunRevived};
use crate::items::inventory::Inventory;
use crate::items::pickup::{ItemPickedUp, PICKUPS, Pickup, PickupAssets, PickupAuthority, PickupRequest, PickupTaken, spawn_layout, spawn_pickup};
use crate::player::Player;
use crate::ui::icons::NoticeIcon;
use crate::ui::notifications::Notify;
use crate::ui::strings;

pub struct WorldSyncPlugin;

impl Plugin for WorldSyncPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (world_inbox, local_events).chain());
    }
}

/// "Host" or "Guest 1"… by player id.
pub fn player_name(id: NetId) -> String {
    if id == PLAYER_ID_BASE { "Host".into() } else { format!("Guest {}", id.saturating_sub(PLAYER_ID_BASE)) }
}

/// The guest's own world while it visits the host's.
#[derive(Default)]
struct Stash {
    sun_revived: bool,
    taken: Vec<u32>,
}

fn notice(notify: &mut MessageWriter<Notify>, event: &WorldEvent) {
    let n = match *event {
        WorldEvent::Joined { who } => Notify::new(NoticeIcon::Joined, strings::joined(&player_name(who))),
        WorldEvent::Left { who } => Notify::new(NoticeIcon::Left, strings::left(&player_name(who))),
        WorldEvent::SunRevived { by } => Notify::new(NoticeIcon::Sunflower, strings::revived_sun(&player_name(by))),
        WorldEvent::PickedUp { by, item, .. } => {
            Notify::new(NoticeIcon::Item(item.def().icon), strings::picked_up_by(&player_name(by), item.def().name))
        }
        WorldEvent::Died { who } => Notify::new(NoticeIcon::Died, strings::composted(&player_name(who))),
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
    mut stash: Local<Option<Stash>>,
    mut authority: ResMut<PickupAuthority>,
    mut notify: MessageWriter<Notify>,
    mut picked: MessageWriter<ItemPickedUp>,
    mut hearths: Query<&mut Hearth>,
    pickups: Query<(Entity, &Pickup, &Transform)>,
    mut player: Query<&mut Inventory, With<Player>>,
) {
    let Some(assets) = assets else { return };
    let broadcast = |event: WorldEvent| link.send(NetCmd::Reliable { to: None, bytes: encode(&Reliable::Event(event)) });
    for msg in inbox.read() {
        match msg {
            NetInbox::RoleChanged if session.role == Role::Client => {
                // Visiting: put our own world away; the host's arrives in `World`.
                let present: Vec<u32> = pickups.iter().map(|(_, p, _)| p.id).collect();
                *stash = Some(Stash {
                    sun_revived: hearths.iter().any(|h| h.kindled),
                    taken: (0..PICKUPS.len() as u32).filter(|id| !present.contains(id)).collect(),
                });
                for (e, _, _) in &pickups {
                    commands.entity(e).despawn();
                }
                authority.0 = false;
            }
            NetInbox::RoleChanged if session.role == Role::Offline => {
                // Back home: our own world as we left it.
                if let Some(own) = stash.take() {
                    for (e, _, _) in &pickups {
                        commands.entity(e).despawn();
                    }
                    spawn_layout(&mut commands, &assets, &own.taken);
                    for mut h in &mut hearths {
                        h.kindled = own.sun_revived;
                    }
                }
                authority.0 = true;
            }
            NetInbox::GuestJoined { peer, id } if session.is_host() => {
                let world = Reliable::World {
                    sun_revived: hearths.iter().any(|h| h.kindled),
                    pickups: pickups
                        .iter()
                        .map(|(_, p, tf)| PickupState { id: p.id, item: p.item, pos: [tf.translation.x, tf.translation.z] })
                        .collect(),
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
                let event = WorldEvent::PickedUp { by, pickup: p.id, item: p.item };
                broadcast(event);
                notice(&mut notify, &event);
            }
            NetInbox::Reliable(peer, Reliable::Report(report)) if session.is_host() => {
                let Some(&by) = session.peers.get(peer) else { continue };
                // Trust only what a guest says about itself.
                let event = match *report {
                    WorldEvent::SunRevived { .. } => {
                        if hearths.iter().any(|h| h.kindled) {
                            continue;
                        }
                        for mut h in &mut hearths {
                            h.kindled = true;
                        }
                        WorldEvent::SunRevived { by }
                    }
                    WorldEvent::Died { .. } => WorldEvent::Died { who: by },
                    _ => continue,
                };
                broadcast(event);
                notice(&mut notify, &event);
            }
            // ── Guest: the host's world ──
            NetInbox::Reliable(HOST_PEER, Reliable::World { sun_revived, pickups: list }) if session.role == Role::Client => {
                for mut h in &mut hearths {
                    h.kindled = *sun_revived;
                }
                for p in list {
                    spawn_pickup(&mut commands, &assets, p.id, p.item, Vec2::from_array(p.pos));
                }
            }
            NetInbox::Reliable(HOST_PEER, Reliable::Event(event)) if session.role == Role::Client => {
                let mine = |id: &NetId| *id == session.my_id;
                match event {
                    WorldEvent::Joined { who } | WorldEvent::Left { who } | WorldEvent::Died { who } if mine(who) => continue,
                    WorldEvent::SunRevived { by } => {
                        for mut h in &mut hearths {
                            h.kindled = true;
                        }
                        if mine(by) {
                            continue; // we already saw the banner
                        }
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
    died: Query<(), (With<Player>, Added<DeathSequence>)>,
) {
    let send = |to: Option<u32>, msg: Reliable| link.send(NetCmd::Reliable { to, bytes: encode(&msg) });
    let me = session.my_id;
    match session.role {
        Role::Hosting => {
            requests.clear(); // granted locally (`grant_pickups`)
            for t in taken.read() {
                send(None, Reliable::Event(WorldEvent::PickedUp { by: me, pickup: t.pickup, item: t.item }));
            }
            for _ in revived.read() {
                send(None, Reliable::Event(WorldEvent::SunRevived { by: me }));
            }
            if !died.is_empty() {
                send(None, Reliable::Event(WorldEvent::Died { who: me }));
            }
        }
        Role::Client => {
            taken.clear();
            for r in requests.read() {
                send(Some(HOST_PEER), Reliable::RequestPickup { pickup: r.pickup });
            }
            for _ in revived.read() {
                send(Some(HOST_PEER), Reliable::Report(WorldEvent::SunRevived { by: me }));
            }
            if !died.is_empty() {
                send(Some(HOST_PEER), Reliable::Report(WorldEvent::Died { who: me }));
            }
        }
        _ => {
            requests.clear();
            taken.clear();
            revived.clear();
        }
    }
}
