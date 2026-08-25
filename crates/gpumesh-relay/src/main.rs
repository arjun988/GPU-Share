use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{bail, Context};
use gpumesh_network::{NetworkEndpoint, PeerConnection};
use gpumesh_protocol::Message;
use gpumesh_security::NodeIdentity;
use tokio::sync::Mutex;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

type Registrations = Arc<Mutex<HashMap<String, PeerConnection>>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let listen_addr: SocketAddr = std::env::var("GPUMESH_RELAY_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:4799".to_owned())
        .parse()
        .context("GPUMESH_RELAY_ADDR must be an IP socket address such as 0.0.0.0:4799")?;
    let endpoint =
        NetworkEndpoint::bind_addr(Arc::new(NodeIdentity::generate()), listen_addr).await?;
    let registrations = Registrations::default();

    info!("GPUMesh QUIC relay listening on {}", endpoint.listen_addr);
    loop {
        match endpoint.accept().await {
            Ok(conn) => {
                let registrations = registrations.clone();
                tokio::spawn(async move {
                    if let Err(error) = handle_connection(conn, registrations).await {
                        warn!("relay connection ended: {error:#}");
                    }
                });
            }
            Err(error) => warn!("relay accept failed: {error}"),
        }
    }
}

async fn handle_connection(
    conn: PeerConnection,
    registrations: Registrations,
) -> anyhow::Result<()> {
    let first = conn
        .recv()
        .await?
        .context("connection closed before relay command")?;
    let Message::Error { message } = first else {
        bail!("first frame was not a relay command");
    };

    if let Some(node_id) = message.strip_prefix("RELAY_REGISTER:") {
        if node_id.is_empty() {
            bail!("registration has an empty node id");
        }
        let replaced = registrations.lock().await.insert(node_id.to_owned(), conn);
        if let Some(old) = replaced {
            old.close();
        }
        info!("registered relay target {node_id}");
        return Ok(());
    }

    if let Some(node_id) = message.strip_prefix("RELAY_REQUEST:") {
        if node_id.is_empty() {
            bail!("request has an empty node id");
        }
        let Some(target) = registrations.lock().await.remove(node_id) else {
            conn.send(Message::Error {
                message: format!("relay target unavailable: {node_id}"),
            })
            .await?;
            bail!("relay target {node_id} is not registered");
        };
        info!("bridging relay session to {node_id}");
        return proxy_messages(conn, target).await;
    }

    bail!("unknown relay command");
}

async fn proxy_messages(left: PeerConnection, right: PeerConnection) -> anyhow::Result<()> {
    loop {
        tokio::select! {
            message = left.recv() => match message? {
                Some(message) => right.send(message).await?,
                None => return Ok(()),
            },
            message = right.recv() => match message? {
                Some(message) => left.send(message).await?,
                None => return Ok(()),
            },
        }
    }
}
