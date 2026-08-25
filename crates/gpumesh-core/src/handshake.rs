use gpumesh_common::{GpuMeshError, Result};
use gpumesh_network::PeerConnection;
use gpumesh_protocol::{Message, ProtocolHello};
use gpumesh_security::verify_hello;

pub async fn perform_client_handshake(
    conn: &mut PeerConnection,
    hello: ProtocolHello,
) -> Result<ProtocolHello> {
    conn.send(Message::Hello(hello)).await?;
    match conn.recv().await? {
        Some(Message::HelloAck(ack)) => {
            ack.check_compat().map_err(GpuMeshError::Protocol)?;
            verify_hello(&ack)?;
            Ok(ack)
        }
        Some(Message::Error { message }) => Err(GpuMeshError::Network(message)),
        Some(other) => Err(GpuMeshError::Protocol(format!(
            "expected HelloAck, got {other:?}"
        ))),
        None => Err(GpuMeshError::Network(
            "connection closed during handshake".into(),
        )),
    }
}

pub async fn perform_server_handshake(
    conn: &mut PeerConnection,
    hello: ProtocolHello,
) -> Result<ProtocolHello> {
    match conn.recv().await? {
        Some(Message::Hello(peer)) => {
            peer.check_compat().map_err(GpuMeshError::Protocol)?;
            verify_hello(&peer)?;
            conn.send(Message::HelloAck(hello)).await?;
            Ok(peer)
        }
        Some(Message::Error { message }) if message.starts_with("RELAY_REQUEST:") => Err(
            GpuMeshError::Network("relay requests not handled by agent endpoint".into()),
        ),
        Some(Message::Error { message }) if message.starts_with("RELAY_REGISTER:") => Err(
            GpuMeshError::Network("relay register not handled by agent endpoint".into()),
        ),
        Some(other) => Err(GpuMeshError::Protocol(format!(
            "expected Hello, got {other:?}"
        ))),
        None => Err(GpuMeshError::Network(
            "connection closed during handshake".into(),
        )),
    }
}
