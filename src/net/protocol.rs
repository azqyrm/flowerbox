//! What goes over the wire. Everything is encoded with `postcard` (compact, serde).
//!
//! - **Datagrams** (unreliable, ~30 Hz): [`Datagram::State`] from a client (its own body) and
//!   [`Datagram::Snapshot`] from the host (every body: all players and enemies).
//! - **Reliable** (an ordered QUIC stream per connection): [`Reliable`] events that must arrive
//!   (welcome, damage, parries).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::character::action::ActionKind;
use crate::enemy::EnemyKind;
use crate::items::ItemId;
use crate::weapon::{AttackHand, Hand, WeaponClass};

/// Bumped whenever the protocol changes; peers with another version are refused.
pub const PROTOCOL_VERSION: u16 = 13;

/// Network id of a body. Players: `PLAYER_ID_BASE + slot` (the host is slot 0). Enemies:
/// `ENEMY_ID_BASE + index`.
pub type NetId = u32;
pub const PLAYER_ID_BASE: NetId = 1;
pub const ENEMY_ID_BASE: NetId = 1000;
/// The training dummy (one per world).
pub const DUMMY_ID: NetId = 999;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    Player,
    /// Any enemy: its body, weapon and size come from its `EnemyDef`.
    Enemy(EnemyKind),
}

/// A running action, enough to pose it identically on the other side.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct NetAction {
    pub kind: ActionKind,
    pub elapsed: f32,
    pub dir: [f32; 3],
    pub charge: f32,
    pub weapon: WeaponClass,
    pub hand: AttackHand,
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
    /// Opted into PvP (Poppy).
    pub pvp: bool,
    /// Which hand's weapon is held in both hands.
    pub two_hand: Option<Hand>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Datagram {
    /// Client → host: my body.
    State(BodyState),
    /// Host → clients: bodies in the world — every player each tick, enemies when they changed (or
    /// as a keepalive). Split over several datagrams so each fits ([`chunk_bodies`]).
    Snapshot(Vec<BodyState>),
    /// Host → clients, twice a second: the ids of every body that exists. Puppets not on it go.
    Roster(Vec<NetId>),
}

/// Largest snapshot datagram we send. QUIC datagrams over the internet (and through relays) carry
/// about 1150–1200 bytes; staying well under leaves room for headers and smaller paths.
pub const MAX_SNAPSHOT_BYTES: usize = 900;

/// Splits bodies into snapshots that each encode to at most `max` bytes.
pub fn chunk_bodies(bodies: Vec<BodyState>, max: usize) -> Vec<Vec<BodyState>> {
    let mut chunks: Vec<Vec<BodyState>> = vec![];
    let mut current: Vec<BodyState> = vec![];
    let mut size = 4; // enum tag + length prefix
    for body in bodies {
        let len = encode(&body).len();
        if !current.is_empty() && size + len > max {
            chunks.push(std::mem::take(&mut current));
            size = 4;
        }
        size += len;
        current.push(body);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Reliable {
    /// Client → host, first message (with the name it would like; empty = none chosen).
    Hello { version: u16, name: String },
    /// Host → everyone: every player's name (made unique by the host).
    Names(Vec<(NetId, String)>),
    /// Guest → host: I changed my name in the settings.
    Rename { name: String },
    /// Guest → host: I said something.
    Chat { text: String },
    /// Host → guests: `who` said something.
    Said { who: NetId, text: String },
    /// Host → client: your player id.
    Welcome { your_id: NetId },
    /// Host → client: the session can't take you.
    Refused { reason: String },
    /// Client → host: my weapon hit `target` (an enemy) for `amount`.
    Damage { target: NetId, amount: f32, from: [f32; 3] },
    /// Client → host: I parried `attacker` (stun it).
    Parried { attacker: NetId },
    /// Host → new guest: the world as it is (revived sunflowers, open doors, items on the floor).
    World { suns: Vec<u32>, doors: Vec<u32>, pickups: Vec<PickupState>, dummy_hp: f32 },
    /// Guest → host: I'd like to take this item.
    RequestPickup { pickup: u32 },
    /// Guest → host: I threw `item` out at `pos` (put it into your world).
    RequestThrow { item: ItemId, pos: [f32; 3] },
    /// Guest → host: I rang bell `id` (the host judges the rite).
    RequestBell { id: u32 },
    /// Host → new guest: bosses already defeated in this world (their fog is gone).
    Bosses { defeated: Vec<EnemyKind> },
    /// Guest → host: I'd like to open this door.
    RequestDoor { id: u32 },
    /// Guest → host: something happened to me / I did something (the host checks and relays).
    Report(WorldEvent),
    /// Host → guests: something happened in the world.
    Event(WorldEvent),
    /// Host → guest: another player parried you (you're stunned).
    Stun,
}

/// An item on the floor.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct PickupState {
    pub id: u32,
    pub item: ItemId,
    pub pos: [f32; 3],
}

/// Things worth telling everyone (they also become notifications).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum WorldEvent {
    Joined { who: NetId },
    Left { who: NetId },
    /// `by` revived sunflower `id`.
    SunRevived { by: NetId, id: u32 },
    /// `by` opened door `id`.
    DoorOpened { by: NetId, id: u32 },
    /// `who` tried a door from the side it doesn't open from.
    DoorTried { who: NetId },
    /// `item` (pickup `id`) landed at `pos`: an enemy's drop (`by: None`) or thrown out by a player.
    Dropped { id: u32, item: ItemId, pos: [f32; 3], by: Option<NetId> },
    /// `by` rang bell `id`.
    BellRung { by: NetId, id: u32 },
    /// `by` took pickup `pickup` (`item`).
    PickedUp { by: NetId, pickup: u32, item: ItemId },
    Died { who: NetId },
    /// `who` ate `item` and switched PvP `on`/off.
    Pvp { who: NetId, on: bool, item: ItemId },
    /// `who` sat down at the sunflower; `resting` of `total` players are resting now.
    RestWaiting { who: NetId, resting: u32, total: u32 },
    /// Every player rests: enemies respawn, everyone is restored.
    RestComplete,
    /// The training dummy was struck (on the host): animate it, show the number.
    DummyHit { amount: f32, from: Option<[f32; 3]>, killed: bool, hp: f32 },
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
            action: Some(NetAction {
                kind: ActionKind::LightAttack(1),
                elapsed: 0.2,
                dir: [0.0, 0.0, -1.0],
                charge: 0.0,
                weapon: WeaponClass::LightSword,
                hand: AttackHand::Both,
            }),
            guard: None,
            hp: 80.0,
            hp_max: 100.0,
            right_hand: Some(ItemId::Longsword),
            left_hand: Some(ItemId::WoodenShield),
            head: None,
            pvp: false,
            two_hand: None,
        }
    }

    #[test]
    fn big_snapshots_are_split_into_datagrams_that_fit() {
        // 5 players + the level's 21 enemies (and some to spare).
        let bodies: Vec<BodyState> = (0..40).map(body).collect();
        let chunks = chunk_bodies(bodies, MAX_SNAPSHOT_BYTES);
        assert!(chunks.len() > 1);
        assert_eq!(chunks.iter().map(Vec::len).sum::<usize>(), 40, "nothing lost");
        for c in &chunks {
            let n = encode(&Datagram::Snapshot(c.clone())).len();
            assert!(n <= MAX_SNAPSHOT_BYTES, "{n} bytes");
        }
        let roster = encode(&Datagram::Roster((0..40).map(|i| ENEMY_ID_BASE + i * 37).collect()));
        assert!(roster.len() < 200, "{} bytes", roster.len());
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
