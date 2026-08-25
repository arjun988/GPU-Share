//! QUIC relay fallback advertised as `relay://host:port` or `host:port`.
//!
//! For Phase 2/3 the relay is a thin forwarder. If `GPUMESH_RELAY` is unset, dial falls back
//! only across peer-advertised addresses.

use gpumesh_common::{GpuMeshError, Result};
use tracing::info;

use crate::endpoint::NetworkEndpoint;
use crate::peer_conn::{ConnectionMode, PeerConnection};

pub struct RelayClient {
    pub relay_addr: String,
}

impl RelayClient {
    pub fn from_env() -> Option<Self> {
        std::env::var("GPUMESH_RELAY")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|relay_addr| Self { relay_addr })
    }

    async fn connect(&self, endpoint: &NetworkEndpoint) -> Result<PeerConnection> {
        let relay_addr = self
            .relay_addr
            .strip_prefix("relay://")
            .unwrap_or(&self.relay_addr);
        let socket = tokio::net::lookup_host(relay_addr)
            .await
            .map_err(|e| GpuMeshError::Network(format!("cannot resolve relay {relay_addr}: {e}")))?
            .next()
            .ok_or_else(|| {
                GpuMeshError::Network(format!("relay {relay_addr} resolved no addresses"))
            })?;
        endpoint.dial(&socket.to_string()).await
    }

    /// Register this endpoint as the relay target for `node_id`.
    ///
    /// The returned connection becomes an inbound peer session after a dialer is matched.
    pub async fn register(
        &self,
        endpoint: &NetworkEndpoint,
        node_id: &str,
    ) -> Result<PeerConnection> {
        info!("registering node {node_id} with relay {}", self.relay_addr);
        let mut conn = self.connect(endpoint).await?;
        conn.send(gpumesh_protocol::Message::Error {
            message: format!("RELAY_REGISTER:{node_id}"),
        })
        .await?;
        conn.connection_mode = ConnectionMode::Relay;
        Ok(conn)
    }

    /// Dial a peer through the relay, then continue using normal framed messages.
    pub async fn dial_via(
        &self,
        endpoint: &NetworkEndpoint,
        peer_node_id: &str,
    ) -> Result<PeerConnection> {
        info!(
            "attempting relay {} for peer {peer_node_id}",
            self.relay_addr
        );
        let mut conn = self.connect(endpoint).await?;
        conn.send(gpumesh_protocol::Message::Error {
            message: format!("RELAY_REQUEST:{peer_node_id}"),
        })
        .await
        .map_err(|e| GpuMeshError::Network(e.to_string()))?;
        conn.connection_mode = ConnectionMode::Relay;
        Ok(conn)
    }
}

/// Try direct addresses, then optional relay.
pub async fn dial_with_fallback(
    endpoint: &NetworkEndpoint,
    addrs: &[String],
    peer_node_id: &str,
) -> Result<PeerConnection> {
    match endpoint.dial_any(addrs).await {
        Ok(c) => Ok(c),
        Err(direct_err) => {
            if let Some(relay) = RelayClient::from_env() {
                match relay.dial_via(endpoint, peer_node_id).await {
                    Ok(c) => Ok(c),
                    Err(relay_err) => Err(GpuMeshError::Network(format!(
                        "direct failed ({direct_err}); relay failed ({relay_err})"
                    ))),
                }
            } else {
                Err(direct_err)
            }
        }
    }
}
