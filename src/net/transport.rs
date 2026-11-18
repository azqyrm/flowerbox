//! The network thread: an iroh endpoint on its own tokio runtime, talking to Bevy through
//! channels ([`NetLink`]). Bevy never blocks on the network.
//!
//! iroh gives every endpoint a public key (its id) and connects peers **directly** (UDP hole
//! punching), falling back to n0's free public **relay servers** when a direct path can't be
//! made. The host's [`iroh::EndpointAddr`] (id + relay + local addresses) is the join code. With
//! the `N0` preset the id alone would also do (address lookup via n0's DNS), but carrying the
//! addresses lets two machines on the same LAN connect without any internet service at all.
//!
//! Per connection: datagrams (unreliable state), plus one bidirectional stream opened by the
//! client carrying length-prefixed reliable messages.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use bevy::prelude::*;
use iroh::endpoint::{Connection, presets};
use iroh::{Endpoint, EndpointAddr};
use tokio::sync::mpsc as tmpsc;

/// QUIC application protocol name; peers speaking anything else are refused at the handshake.
pub const ALPN: &[u8] = b"flowerbox/1";

/// Who a message is from / for. On a client the host is always peer 0; on the host, clients
/// are numbered from 1 in joining order.
pub type PeerId = u32;
pub const HOST_PEER: PeerId = 0;

pub enum NetCmd {
    Host,
    Join(EndpointAddr),
    /// Unreliable; `to: None` = every peer.
    Datagram { to: Option<PeerId>, bytes: Vec<u8> },
    /// Reliable, ordered; `to: None` = every peer.
    Reliable { to: Option<PeerId>, bytes: Vec<u8> },
    /// Host: close one peer's connection (it's reported as `PeerLeft`).
    Kick(PeerId),
    Leave,
}

#[derive(Debug)]
pub enum NetEvent {
    /// Hosting; share this address (the join code).
    Hosting(EndpointAddr),
    /// Client: connected to the host.
    Connected,
    PeerJoined(PeerId),
    PeerLeft(PeerId),
    Datagram(PeerId, Vec<u8>),
    Reliable(PeerId, Vec<u8>),
    Error(String),
}

/// Bevy's handle to the network thread.
#[derive(Resource)]
pub struct NetLink {
    cmd: tmpsc::UnboundedSender<NetCmd>,
    events: Mutex<mpsc::Receiver<NetEvent>>,
}

impl NetLink {
    pub fn start() -> Self {
        let (cmd_tx, cmd_rx) = tmpsc::unbounded_channel();
        let (ev_tx, ev_rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("flowerbox-net".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("tokio runtime");
                rt.block_on(net_main(cmd_rx, ev_tx));
            })
            .expect("network thread");
        Self { cmd: cmd_tx, events: Mutex::new(ev_rx) }
    }

    pub fn send(&self, cmd: NetCmd) {
        let _ = self.cmd.send(cmd);
    }

    pub fn poll(&self) -> Vec<NetEvent> {
        self.events.lock().map(|rx| rx.try_iter().collect()).unwrap_or_default()
    }
}

struct Peer {
    conn: Connection,
    writer: tmpsc::UnboundedSender<Vec<u8>>,
}

type Peers = Arc<Mutex<HashMap<PeerId, Peer>>>;

async fn net_main(mut cmds: tmpsc::UnboundedReceiver<NetCmd>, events: mpsc::Sender<NetEvent>) {
    let peers: Peers = Arc::default();
    let mut endpoint: Option<Endpoint> = None;
    let mut datagram_failures: u64 = 0;
    while let Some(cmd) = cmds.recv().await {
        match cmd {
            NetCmd::Host => {
                let Some(ep) = bind(&events).await else { continue };
                // Wait (briefly) for a home relay so the code works across the internet too.
                let _ = tokio::time::timeout(Duration::from_secs(6), ep.online()).await;
                let _ = events.send(NetEvent::Hosting(ep.addr()));
                tokio::spawn(accept_loop(ep.clone(), peers.clone(), events.clone()));
                endpoint = Some(ep);
            }
            NetCmd::Join(addr) => {
                let Some(ep) = bind(&events).await else { continue };
                let (ep2, peers2, events2) = (ep.clone(), peers.clone(), events.clone());
                tokio::spawn(async move {
                    let conn = match tokio::time::timeout(Duration::from_secs(25), ep2.connect(addr, ALPN)).await {
                        Ok(Ok(c)) => c,
                        Ok(Err(e)) => return send_err(&events2, format!("could not connect: {e}")),
                        Err(_) => return send_err(&events2, "could not connect: timed out".into()),
                    };
                    let (send, recv) = match conn.open_bi().await {
                        Ok(s) => s,
                        Err(e) => return send_err(&events2, format!("could not open a stream: {e}")),
                    };
                    let _ = events2.send(NetEvent::Connected);
                    run_peer(HOST_PEER, conn, send, recv, peers2, events2);
                });
                endpoint = Some(ep);
            }
            NetCmd::Datagram { to, bytes } => {
                for (_, peer) in targets(&peers, to) {
                    if let Err(e) = peer.0.send_datagram(bytes.clone().into()) {
                        datagram_failures += 1;
                        if datagram_failures % 300 == 1 {
                            warn!("datagram of {} bytes not sent ({e}; max {:?}); {datagram_failures} failures so far", bytes.len(), peer.0.max_datagram_size());
                        }
                    }
                }
            }
            NetCmd::Reliable { to, bytes } => {
                for (_, peer) in targets(&peers, to) {
                    let _ = peer.1.send(bytes.clone());
                }
            }
            NetCmd::Kick(id) => {
                if let Some(conn) = peers.lock().ok().and_then(|p| p.get(&id).map(|peer| peer.conn.clone())) {
                    conn.close(1u32.into(), b"silent");
                }
            }
            NetCmd::Leave => {
                let all: Vec<Peer> = peers.lock().map(|mut p| p.drain().map(|(_, v)| v).collect()).unwrap_or_default();
                for peer in all {
                    peer.conn.close(0u32.into(), b"bye");
                }
                if let Some(ep) = endpoint.take() {
                    ep.close().await;
                }
            }
        }
    }
}

fn send_err(events: &mpsc::Sender<NetEvent>, msg: String) {
    warn!("net: {msg}");
    let _ = events.send(NetEvent::Error(msg));
}

async fn bind(events: &mpsc::Sender<NetEvent>) -> Option<Endpoint> {
    // Notice a vanished peer (crash, cable) after 10 s instead of the default 30 s.
    let idle = Duration::from_secs(10).try_into().expect("10 s fits a QUIC idle timeout");
    let transport = iroh::endpoint::QuicTransportConfig::builder().max_idle_timeout(Some(idle)).build();
    match Endpoint::builder(presets::N0).alpns(vec![ALPN.to_vec()]).transport_config(transport).bind().await {
        Ok(ep) => Some(ep),
        Err(e) => {
            send_err(events, format!("could not open the network endpoint: {e}"));
            None
        }
    }
}

type PeerHandles = (Connection, tmpsc::UnboundedSender<Vec<u8>>);

/// Connection + reliable writer of each addressed peer (cloned out of the lock).
fn targets(peers: &Peers, to: Option<PeerId>) -> Vec<(PeerId, PeerHandles)> {
    let Ok(p) = peers.lock() else { return vec![] };
    p.iter()
        .filter(|(id, _)| to.is_none_or(|t| t == **id))
        .map(|(id, peer)| (*id, (peer.conn.clone(), peer.writer.clone())))
        .collect()
}

async fn accept_loop(ep: Endpoint, peers: Peers, events: mpsc::Sender<NetEvent>) {
    let mut next_id: PeerId = 1;
    while let Some(incoming) = ep.accept().await {
        let id = next_id;
        next_id += 1;
        let (peers, events) = (peers.clone(), events.clone());
        tokio::spawn(async move {
            let conn = match incoming.await {
                Ok(c) => c,
                Err(e) => return warn!("net: incoming connection failed: {e}"),
            };
            match conn.accept_bi().await {
                Ok((send, recv)) => {
                    let _ = events.send(NetEvent::PeerJoined(id));
                    run_peer(id, conn, send, recv, peers, events);
                }
                Err(e) => warn!("net: peer {id} never opened its stream: {e}"),
            }
        });
    }
}

/// Registers the peer and spawns its reader/writer tasks. Reports `PeerLeft` once when it closes.
fn run_peer(
    id: PeerId,
    conn: Connection,
    mut send: iroh::endpoint::SendStream,
    mut recv: iroh::endpoint::RecvStream,
    peers: Peers,
    events: mpsc::Sender<NetEvent>,
) {
    let (writer, mut outbox) = tmpsc::unbounded_channel::<Vec<u8>>();
    if let Ok(mut p) = peers.lock() {
        p.insert(id, Peer { conn: conn.clone(), writer });
    }
    // Reliable out: u32 length prefix + payload.
    tokio::spawn(async move {
        while let Some(bytes) = outbox.recv().await {
            let len = (bytes.len() as u32).to_le_bytes();
            if send.write_all(&len).await.is_err() || send.write_all(&bytes).await.is_err() {
                break;
            }
        }
    });
    // Reliable in.
    let ev = events.clone();
    tokio::spawn(async move {
        loop {
            let mut len = [0u8; 4];
            if recv.read_exact(&mut len).await.is_err() {
                break;
            }
            let len = u32::from_le_bytes(len) as usize;
            if len > 1 << 20 {
                break; // nonsense: drop the peer
            }
            let mut buf = vec![0u8; len];
            if recv.read_exact(&mut buf).await.is_err() {
                break;
            }
            let _ = ev.send(NetEvent::Reliable(id, buf));
        }
    });
    // Datagrams in, until the connection closes.
    tokio::spawn(async move {
        while let Ok(d) = conn.read_datagram().await {
            let _ = events.send(NetEvent::Datagram(id, d.to_vec()));
        }
        let removed = peers.lock().ok().and_then(|mut p| p.remove(&id)).is_some();
        if removed || id == HOST_PEER {
            let _ = events.send(NetEvent::PeerLeft(id));
        }
    });
}
