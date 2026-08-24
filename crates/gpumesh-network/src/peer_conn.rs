use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use gpumesh_common::{GpuMeshError, Result};
use gpumesh_protocol::{JsonFrameCodec, Message};
use quinn::{Connection, RecvStream, SendStream};
use tokio::sync::Mutex;
use tokio_util::codec::{FramedRead, FramedWrite};
use tracing::debug;

/// Bidirectional framed message channel over a QUIC connection.
pub struct PeerConnection {
    pub connection: Connection,
    pub remote_addr: String,
    pub inbound: bool,
    send: Arc<Mutex<FramedWrite<SendStream, JsonFrameCodec>>>,
    recv: Arc<Mutex<FramedRead<RecvStream, JsonFrameCodec>>>,
    pub peer_node_id: Option<String>,
    pub peer_name: Option<String>,
    pub connection_mode: ConnectionMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionMode {
    Direct,
    Relay,
}

impl PeerConnection {
    pub async fn from_connection(connection: Connection, inbound: bool) -> Result<Self> {
        let remote_addr = connection.remote_address().to_string();
        let (send, recv) = if inbound {
            connection
                .accept_bi()
                .await
                .map_err(|e| GpuMeshError::Network(e.to_string()))?
        } else {
            connection
                .open_bi()
                .await
                .map_err(|e| GpuMeshError::Network(e.to_string()))?
        };
        Ok(Self {
            connection,
            remote_addr,
            inbound,
            send: Arc::new(Mutex::new(FramedWrite::new(send, JsonFrameCodec))),
            recv: Arc::new(Mutex::new(FramedRead::new(recv, JsonFrameCodec))),
            peer_node_id: None,
            peer_name: None,
            connection_mode: ConnectionMode::Direct,
        })
    }

    pub async fn send(&self, msg: Message) -> Result<()> {
        let mut send = self.send.lock().await;
        send.send(msg)
            .await
            .map_err(|e| GpuMeshError::Network(e.to_string()))
    }

    pub async fn recv(&self) -> Result<Option<Message>> {
        let mut recv = self.recv.lock().await;
        match recv.next().await {
            Some(Ok(m)) => {
                debug!("recv msg from {}", self.remote_addr);
                Ok(Some(m))
            }
            Some(Err(e)) => Err(GpuMeshError::Network(e.to_string())),
            None => Ok(None),
        }
    }

    pub fn close(&self) {
        self.connection.close(0u32.into(), b"bye");
    }
}
