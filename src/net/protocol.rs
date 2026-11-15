//! What goes over the wire. Everything is encoded with `postcard` (compact, serde).
//!
//! - **Datagrams** (unreliable, ~30 Hz): [`Datagram::State`] from a client (its own body) and
//!   [`Datagram::Snapshot`] from the host (every body: all players and enemies).
//! - **Reliable** (an ordered QUIC stream per connection): [`Reliable`] events that must arrive
//!   (welcome, damage, parries).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::character::action::ActionKind;
use crate::items::ItemId;
use crate::weapon::WeaponClass;

/// Bumped whenever the protocol changes; peers with another version are refused.
pub const PROTOCOL_VERSION: u16 = 2;

/// Network id of a body. Players: `PLAYER_ID_BASE + slot` (the host is slot 0). Enemies:
/// `ENEMY_ID_BASE + index`.
pub type NetId = u32;
pub const PLAYER_ID_BASE: NetId = 1;
pub const ENEMY_ID_BASE: NetId = 1000;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    Player,
    Hollow,
}

/// A running action, enough to pose it identically on the other side.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct NetAction {
    pub kind: ActionKind,
    pub elapsed: f32,
    pub dir: [f32; 3],
    pub charge: f32,
    pub weapon: WeaponClass,
}

/// Everything another machine needs to draw (and fight) a character.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BodyState {
    pub id: NetId,
    pub kind: BodyKind,
    pub pos: [f32; 3],
    pub facing: f32,
    /// Displacement per second (horizontal + vertical), for the gait and extrapolation.
    pub moved: [f32; 3],
    pub vertical: f32,
    pub grounded: bool,
    pub action: Option<NetAction>,
    /// Seconds since the guard started rising (`None` = lowered).
    pub guard: Option<f32>,
    pub hp: f32,
    pub hp_max: f32,
    pub right_hand: Option<ItemId>,
    pub left_hand: Option<ItemId>,
    pub head: Option<ItemId>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Datagram {
    /// Client → host: my body.
    State(BodyState),
    /// Host → clients: every body in the world.
    Snapshot(Vec<BodyState>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Reliable {
    /// Client → host, first message.
    Hello { version: u16 },
    /// Host → client: your player id.
    Welcome { your_id: NetId },
    /// Host → client: the session can't take you.
    Refused { reason: String },
    /// Client → host: my weapon hit `target` (an enemy) for `amount`.
    Damage { target: NetId, amount: f32, from: [f32; 3] },
    /// Client → host: I parried `attacker` (stun it).
    Parried { attacker: NetId },
    /// Host → new guest: the world as it is (sunflower, items on the floor).
    World { sun_revived: bool, pickups: Vec<PickupState> },
    /// Guest → host: I'd like to take this item.
    RequestPickup { pickup: u32 },
    /// Guest → host: something happened to me / I did something (the host checks and relays).
    Report(WorldEvent),
    /// Host → guests: something happened in the world.
    Event(WorldEvent),
}

/// An item on the floor.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct PickupState {
    pub id: u32,
    pub item: ItemId,
    pub pos: [f32; 2],
}

/// Things worth telling everyone (they also become notifications).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum WorldEvent {
    Joined { who: NetId },
    Left { who: NetId },
    /// `by` revived the sunflower.
    SunRevived { by: NetId },
    /// `by` took pickup `pickup` (`item`).
    PickedUp { by: NetId, pickup: u32, item: ItemId },
    Died { who: NetId },
}

pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    postcard::to_allocvec(msg).expect("postcard encoding never fails for our types")
}

pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Option<T> {
    postcard::from_bytes(bytes).ok()
}

pub fn v3(v: Vec3) -> [f32; 3] {
    v.to_array()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(id: NetId) -> BodyState {
        BodyState {
            id,
            kind: BodyKind::Player,
            pos: [1.0, 2.0, 3.0],
            facing: 0.5,
            moved: [0.0; 3],
            vertical: 0.0,
            grounded: true,
            action: Some(NetAction { kind: ActionKind::LightAttack(1), elapsed: 0.2, dir: [0.0, 0.0, -1.0], charge: 0.0, weapon: WeaponClass::LightSword }),
            guard: None,
            hp: 80.0,
            hp_max: 100.0,
            right_hand: Some(ItemId::Longsword),
            left_hand: Some(ItemId::WoodenShield),
            head: None,
        }
    }

    #[test]
    fn messages_round_trip_and_fit_a_datagram() {
        let snap = Datagram::Snapshot((0..8).map(body).collect());
        let bytes = encode(&snap);
        assert!(bytes.len() < 1000, "{} bytes for 8 bodies", bytes.len());
        let Some(Datagram::Snapshot(back)) = decode::<Datagram>(&bytes) else { panic!() };
        assert_eq!(back[3], body(3));
        let r = encode(&Reliable::Damage { target: 1000, amount: 20.0, from: [0.0; 3] });
        assert!(matches!(decode::<Reliable>(&r), Some(Reliable::Damage { target: 1000, .. })));
    }
}
