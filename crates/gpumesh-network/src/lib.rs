//! QUIC-based P2P networking for GPUMesh.
//!
//! Architecture (Phases 0–3):
//! - Peer identity: Ed25519 (`gpumesh-security`)
//! - Transport: Quinn QUIC + TLS 1.3
//! - LAN discovery: mDNS (`_gpumesh._udp.local`)
//! - WAN assist: optional HTTP rendezvous (signaling only)
//! - Relay: TCP/QUIC fallback endpoint when direct dial fails

mod discovery;
mod endpoint;
mod peer_conn;
mod relay;
mod rendezvous;

pub use discovery::LanDiscovery;
pub use endpoint::{NetworkEndpoint, NetworkEvent};
pub use peer_conn::{ConnectionMode, PeerConnection};
pub use relay::{dial_with_fallback, RelayClient};
pub use rendezvous::{RendezvousAnnounce, RendezvousClient, RendezvousPeer};
