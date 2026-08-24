//! Simple relay fallback: connect via an intermediate QUIC hop advertised as `relay://host:port`.
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

    /// Dial peer through relay. Protocol: connect to relay, send `RELAY <peer_id>\n`, then
    /// continue with normal GPUMesh framed messages on the same stream.
    pub async fn dial_via(
        &self,
        endpoint: &NetworkEndpoint,
        peer_node_id: &str,
    ) -> Result<PeerConnection> {
        info!(
            "attempting relay {} for peer {peer_node_id}",
            self.relay_addr
        );
        let mut conn = endpoint.dial(&self.relay_addr).await?;
        // Application-level signal that this session should be bridged.
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
