//! Online co-op over iroh (peer to peer, no game server). See ROADMAP N21.
//!
//! **Topology:** one player **hosts** (their game owns the world: enemies, their AI and health),
//! the others **join** with the host's code (up to `MAX_PLAYERS`). Connections are direct
//! (hole punching) with n0's free public relays as fallback — nobody runs a server.
//!
//! **What is sent** (`protocol.rs`): every machine sends its own player's [`BodyState`] ~30×/s;
//! the host sends a snapshot of every body (all players + enemies) to everyone. Other machines'
//! characters appear as **puppets** (`sync.rs`): the same humanoid, posed by the shared animation
//! from the replicated movement/action/guard state.
//!
//! **Who decides hits:** your own weapon hitting an enemy is detected on your machine and sent to
//! the host (`Reliable::Damage`); an enemy hitting *you* is detected on your machine from its
//! puppet (so blocking and parrying feel local), and a parry is sent to the host, which stuns the
//! real enemy.
//!
//! **UI:** `O` opens the online panel (host / join from clipboard / copy code / leave).
//! Command line: `--host`, `--join <code>`.

pub mod protocol;
pub mod sync;
pub mod transport;
pub mod ui;
pub mod world;

use std::collections::HashMap;
use std::sync::Mutex;

use bevy::prelude::*;
use data_encoding::BASE64URL_NOPAD;
use iroh::EndpointAddr;

use protocol::*;
use transport::{HOST_PEER, NetCmd, NetEvent, NetLink, PeerId};

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(NetLink::start())
            .init_resource::<NetSession>()
            .insert_resource(SystemClipboard(Mutex::new(arboard::Clipboard::new().ok())))
            .add_message::<NetInbox>()
            .add_systems(Startup, command_line)
            .add_plugins((sync::SyncPlugin, ui::NetUiPlugin, world::WorldSyncPlugin))
            .add_systems(PreUpdate, pump_events)
            .add_systems(Last, close_on_exit);
    }
}

/// Host + 4 guests.
pub const MAX_PLAYERS: u32 = 5;
const CODE_PREFIX: &str = "FBX1-";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Role {
    #[default]
    Offline,
    /// Asked the network thread to host; waiting for the endpoint.
    StartingHost,
    Hosting,
    /// Connecting to a host / waiting for the welcome.
    Joining,
    Client,
}

/// The online session as Bevy sees it.
#[derive(Resource, Default, Debug)]
pub struct NetSession {
    pub role: Role,
    /// This machine's player id (the host is always `PLAYER_ID_BASE`).
    pub my_id: NetId,
    /// Host: the join code to share.
    pub code: Option<String>,
    /// Last thing worth telling the player ("Connected", errors…).
    pub status: String,
    /// Host: connected peers and their player ids.
    pub peers: HashMap<PeerId, NetId>,
}

impl NetSession {
    pub fn is_host(&self) -> bool {
        self.role == Role::Hosting
    }
    pub fn is_client(&self) -> bool {
        matches!(self.role, Role::Client | Role::Joining)
    }
    pub fn online(&self) -> bool {
        matches!(self.role, Role::Hosting | Role::Client)
    }
}

/// Decoded messages for the sync systems: (from peer, message).
#[derive(Message, Debug, Clone)]
pub enum NetInbox {
    Datagram(PeerId, Datagram),
    Reliable(PeerId, Reliable),
    /// Host: a guest was admitted with this player id.
    GuestJoined { peer: PeerId, id: NetId },
    /// Host: a guest left.
    GuestLeft { id: NetId },
    /// Became a client (welcomed) or went offline: rebuild what's local vs. remote.
    RoleChanged,
}

/// The OS clipboard (kept alive: on Linux the copied text lives as long as this does).
#[derive(Resource)]
pub struct SystemClipboard(pub Mutex<Option<arboard::Clipboard>>);

impl SystemClipboard {
    pub fn copy(&self, text: &str) -> bool {
        self.0.lock().ok().and_then(|mut c| c.as_mut().map(|c| c.set_text(text.to_string()).is_ok())).unwrap_or(false)
    }
    pub fn paste(&self) -> Option<String> {
        self.0.lock().ok()?.as_mut()?.get_text().ok()
    }
}

pub fn encode_code(addr: &EndpointAddr) -> String {
    format!("{CODE_PREFIX}{}", BASE64URL_NOPAD.encode(&encode(addr)))
}

pub fn decode_code(code: &str) -> Option<EndpointAddr> {
    let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let body = code.strip_prefix(CODE_PREFIX)?;
    decode(&BASE64URL_NOPAD.decode(body.as_bytes()).ok()?)
}

/// Starts hosting (if offline).
pub fn host(link: &NetLink, session: &mut NetSession) {
    if session.role == Role::Offline {
        session.role = Role::StartingHost;
        session.status = "Opening the connection...".into();
        link.send(NetCmd::Host);
    }
}

/// Joins the game behind `code` (if offline).
pub fn join(link: &NetLink, session: &mut NetSession, code: &str) {
    if session.role != Role::Offline {
        return;
    }
    match decode_code(code) {
        Some(mut addr) => {
            // Testing: drop the direct addresses, so the connection has to start via the relay
            // (what happens between two homes behind routers).
            if std::env::var("FLOWERBOX_NET_RELAY_ONLY").is_ok() {
                let relays: Vec<_> = addr.relay_urls().cloned().map(iroh::TransportAddr::Relay).collect();
                addr = EndpointAddr::from_parts(addr.id, relays);
            }
            session.role = Role::Joining;
            session.status = "Connecting...".into();
            link.send(NetCmd::Join(addr));
        }
        None => session.status = "That's not a FlowerBox code (they start with FBX1-).".into(),
    }
}

pub fn leave(link: &NetLink, session: &mut NetSession, inbox: &mut MessageWriter<NetInbox>) {
    if session.role != Role::Offline {
        link.send(NetCmd::Leave);
        *session = NetSession { status: "Left the session.".into(), ..default() };
        inbox.write(NetInbox::RoleChanged);
    }
}

/// Quitting: say goodbye properly so the others see us leave at once.
fn close_on_exit(mut exits: MessageReader<AppExit>, link: Res<NetLink>, session: Res<NetSession>) {
    if exits.read().count() > 0 && session.role != Role::Offline {
        link.send(NetCmd::Leave);
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
}

fn command_line(link: Res<NetLink>, mut session: ResMut<NetSession>) {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--host") {
        host(&link, &mut session);
    }
    if let Some(i) = args.iter().position(|a| a == "--join")
        && let Some(code) = args.get(i + 1)
    {
        join(&link, &mut session, code);
    }
}

/// Network thread → Bevy: session bookkeeping here, game messages to [`NetInbox`].
fn pump_events(
    link: Res<NetLink>,
    clipboard: Res<SystemClipboard>,
    mut session: ResMut<NetSession>,
    mut inbox: MessageWriter<NetInbox>,
) {
    for event in link.poll() {
        match event {
            NetEvent::Hosting(addr) => {
                let code = encode_code(&addr);
                info!("hosting; join code: {code}");
                if let Ok(path) = std::env::var("FLOWERBOX_NET_CODE_FILE") {
                    let _ = std::fs::write(path, &code);
                }
                let copied = clipboard.copy(&code);
                session.role = Role::Hosting;
                session.my_id = PLAYER_ID_BASE;
                session.status = if copied { "Hosting. Code copied to the clipboard." } else { "Hosting. Share the code below." }.into();
                session.code = Some(code);
            }
            NetEvent::Connected => {
                info!("connected to the host");
                link.send(NetCmd::Reliable { to: Some(HOST_PEER), bytes: encode(&Reliable::Hello { version: PROTOCOL_VERSION }) });
                session.status = "Connected, waiting for the host...".into();
            }
            NetEvent::PeerJoined(peer) => info!("peer {peer} connected"),
            NetEvent::PeerLeft(peer) => {
                if session.is_host() {
                    if let Some(id) = session.peers.remove(&peer) {
                        info!("player {id} left");
                        session.status = format!("A player left ({} connected).", session.peers.len());
                        inbox.write(NetInbox::GuestLeft { id });
                    }
                } else if session.is_client() && peer == HOST_PEER {
                    info!("disconnected from the host");
                    *session = NetSession { status: "Disconnected from the host.".into(), ..default() };
                    link.send(NetCmd::Leave);
                    inbox.write(NetInbox::RoleChanged);
                }
            }
            NetEvent::Error(msg) => {
                if matches!(session.role, Role::Joining | Role::StartingHost) {
                    session.role = Role::Offline;
                    link.send(NetCmd::Leave);
                }
                session.status = msg;
            }
            NetEvent::Datagram(peer, bytes) => {
                if let Some(d) = decode::<Datagram>(&bytes) {
                    inbox.write(NetInbox::Datagram(peer, d));
                }
            }
            NetEvent::Reliable(peer, bytes) => {
                let Some(msg) = decode::<Reliable>(&bytes) else { continue };
                match msg {
                    // Host: admit or refuse a new player.
                    Reliable::Hello { version } if session.is_host() => {
                        let refuse = |reason: &str| {
                            link.send(NetCmd::Reliable { to: Some(peer), bytes: encode(&Reliable::Refused { reason: reason.into() }) });
                        };
                        if version != PROTOCOL_VERSION {
                            refuse("Different game version.");
                            continue;
                        }
                        let used: Vec<NetId> = session.peers.values().copied().collect();
                        let Some(id) = (1..MAX_PLAYERS).map(|s| PLAYER_ID_BASE + s).find(|id| !used.contains(id)) else {
                            refuse("The session is full.");
                            continue;
                        };
                        session.peers.insert(peer, id);
                        link.send(NetCmd::Reliable { to: Some(peer), bytes: encode(&Reliable::Welcome { your_id: id }) });
                        session.status = format!("A player joined ({} connected).", session.peers.len());
                        info!("player {id} joined (peer {peer})");
                        inbox.write(NetInbox::GuestJoined { peer, id });
                    }
                    Reliable::Welcome { your_id } if session.role == Role::Joining => {
                        session.role = Role::Client;
                        session.my_id = your_id;
                        session.status = "Joined the host's world.".into();
                        info!("joined as player {your_id}");
                        inbox.write(NetInbox::RoleChanged);
                    }
                    Reliable::Refused { reason } => {
                        session.role = Role::Offline;
                        session.status = format!("The host refused: {reason}");
                        link.send(NetCmd::Leave);
                    }
                    other => {
                        inbox.write(NetInbox::Reliable(peer, other));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_codes_round_trip() {
        let id = iroh::SecretKey::from_bytes(&[7u8; 32]).public();
        let addr = EndpointAddr::new(id).with_ip_addr("192.168.1.20:4433".parse().unwrap());
        let code = encode_code(&addr);
        assert!(code.starts_with(CODE_PREFIX));
        assert_eq!(decode_code(&format!("  {code}\n")), Some(addr));
        assert_eq!(decode_code("hello"), None);
    }
}
