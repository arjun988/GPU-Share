//! Interactive GPU desktop sessions — tunnel RDP/VNC/Sunshine TCP to the provider.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use gpumesh_common::{GpuMeshError, Result};
use gpumesh_network::PeerConnection;
use gpumesh_protocol::Message;
use quinn::{RecvStream, SendStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{info, warn};
use uuid::Uuid;

use crate::MeshNode;

#[derive(Debug, Clone)]
pub struct DesktopBackend {
    pub name: &'static str,
    pub host_port: u16,
    pub suggest_local_port: u16,
    pub viewer_hint: String,
}

/// Probe localhost for a usable desktop backend.
pub async fn detect_backend(prefer: &[String]) -> Option<DesktopBackend> {
    let candidates: Vec<DesktopBackend> = vec![
        DesktopBackend {
            name: "rdp",
            host_port: 3389,
            suggest_local_port: 13389,
            viewer_hint: "Open Remote Desktop to 127.0.0.1:LOCAL_PORT (Windows: mstsc)".into(),
        },
        DesktopBackend {
            name: "vnc",
            host_port: 5900,
            suggest_local_port: 15900,
            viewer_hint: "Open a VNC client to 127.0.0.1:LOCAL_PORT".into(),
        },
        DesktopBackend {
            name: "vnc",
            host_port: 5901,
            suggest_local_port: 15901,
            viewer_hint: "Open a VNC client to 127.0.0.1:LOCAL_PORT".into(),
        },
        DesktopBackend {
            name: "sunshine",
            // Sunshine web UI / HTTPS — TCP only helper; full Moonlight needs UDP.
            host_port: 47990,
            suggest_local_port: 47990,
            viewer_hint: "Sunshine TCP helper on 127.0.0.1:LOCAL_PORT (prefer RDP for full apps)"
                .into(),
        },
    ];

    let order: Vec<&DesktopBackend> = if prefer.is_empty() {
        candidates.iter().collect()
    } else {
        let mut ordered = Vec::new();
        for p in prefer {
            for c in &candidates {
                if c.name.eq_ignore_ascii_case(p) && !ordered.iter().any(|x: &&DesktopBackend| x.host_port == c.host_port) {
                    ordered.push(c);
                }
            }
        }
        for c in &candidates {
            if !ordered.iter().any(|x| x.host_port == c.host_port) {
                ordered.push(c);
            }
        }
        ordered
    };

    for c in order {
        if port_open(c.host_port).await {
            return Some(c.clone());
        }
    }
    None
}

async fn port_open(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    matches!(
        tokio::time::timeout(Duration::from_millis(200), TcpStream::connect(addr)).await,
        Ok(Ok(_))
    )
}

/// Provider: accept DesktopRequest, offer backend, spawn QUIC→TCP tunnel acceptor.
pub async fn handle_desktop_request(
    node: &MeshNode,
    conn: Arc<PeerConnection>,
    prefer: Vec<String>,
) -> Result<()> {
    let desktop_on = {
        let cfg = node.config.read().await;
        *node.desktop_sharing.read().await || cfg.desktop_sharing
    };
    if !desktop_on {
        conn.send(Message::DesktopReject {
            reason: "desktop sharing is not enabled on this node (host: gpumesh desktop share)"
                .into(),
        })
        .await?;
        return Ok(());
    }

    let peer_id = conn
        .peer_node_id
        .clone()
        .unwrap_or_default();
    {
        let allow = node.allowlist.read().await;
        if !allow.is_desktop_allowed(&peer_id) {
            conn.send(Message::DesktopReject {
                reason: format!(
                    "desktop not allowed for this peer — host runs: gpumesh desktop allow {peer_id}"
                ),
            })
            .await?;
            return Ok(());
        }
    }

    let Some(backend) = detect_backend(&prefer).await else {
        conn.send(Message::DesktopReject {
            reason: "no desktop backend found on localhost (enable Windows RDP on :3389, or VNC on :5900)".into(),
        })
        .await?;
        return Ok(());
    };

    let session_id = Uuid::new_v4().to_string();
    conn.send(Message::DesktopOffer {
        session_id: session_id.clone(),
        backend: backend.name.to_string(),
        host_port: backend.host_port,
        suggest_local_port: backend.suggest_local_port,
        viewer_hint: backend.viewer_hint.clone(),
    })
    .await?;

    info!(
        "desktop session {session_id} offered ({}/:{})",
        backend.name, backend.host_port
    );

    // Accept additional QUIC bi-streams as TCP tunnels until control connection closes.
    let quic = conn.connection.clone();
    let host_port = backend.host_port;
    tokio::spawn(async move {
        loop {
            match quic.accept_bi().await {
                Ok((send, recv)) => {
                    tokio::spawn(async move {
                        if let Err(e) = provider_tunnel(send, recv, host_port).await {
                            warn!("desktop tunnel ended: {e}");
                        }
                    });
                }
                Err(_) => break,
            }
        }
    });

    Ok(())
}

async fn provider_tunnel(mut send: SendStream, mut recv: RecvStream, host_port: u16) -> Result<()> {
    let mut magic = [0u8; 4];
    recv.read_exact(&mut magic)
        .await
        .map_err(|e| GpuMeshError::Network(e.to_string()))?;
    if &magic != b"GMDT" {
        return Err(GpuMeshError::Protocol("bad desktop tunnel magic".into()));
    }
    send.write_all(b"GMDT")
        .await
        .map_err(|e| GpuMeshError::Network(e.to_string()))?;

    let addr = SocketAddr::from(([127, 0, 0, 1], host_port));
    let tcp = TcpStream::connect(addr)
        .await
        .map_err(|e| GpuMeshError::Network(format!("connect desktop backend :{host_port}: {e}")))?;
    let (mut tcp_r, mut tcp_w) = tcp.into_split();

    tokio::select! {
        r = tokio::io::copy(&mut recv, &mut tcp_w) => { let _ = r; }
        r = tokio::io::copy(&mut tcp_r, &mut send) => { let _ = r; }
    }
    Ok(())
}

/// Client: request desktop, then listen locally and forward TCP over QUIC.
pub async fn connect_desktop(node: &MeshNode, peer: &str, local_port: Option<u16>) -> Result<()> {
    let mut conn = node.connect_peer(peer).await?;
    conn.send(Message::DesktopRequest {
        prefer: vec!["rdp".into(), "vnc".into()],
    })
    .await?;

    let offer = match conn.recv().await? {
        Some(Message::DesktopOffer {
            session_id,
            backend,
            host_port,
            suggest_local_port,
            viewer_hint,
        }) => {
            info!("desktop offer session={session_id} backend={backend} host_port={host_port}");
            (session_id, backend, host_port, suggest_local_port, viewer_hint)
        }
        Some(Message::DesktopReject { reason }) => {
            return Err(GpuMeshError::NotAuthorized(reason));
        }
        Some(Message::Error { message }) => {
            return Err(GpuMeshError::Network(message));
        }
        other => {
            return Err(GpuMeshError::Protocol(format!(
                "expected DesktopOffer, got {other:?}"
            )));
        }
    };

    let bind_port = local_port.unwrap_or(offer.3);
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], bind_port)))
        .await
        .map_err(|e| GpuMeshError::Network(format!("bind local desktop port: {e}")))?;
    let local = listener.local_addr().map_err(|e| GpuMeshError::Network(e.to_string()))?;

    let hint = offer.4.replace("LOCAL_PORT", &local.port().to_string());
    println!();
    println!("GPU desktop tunnel ready");
    println!("  Peer:     {peer}");
    println!("  Backend:  {}", offer.1);
    println!("  Local:    127.0.0.1:{}", local.port());
    println!("  Viewer:   {hint}");
    if offer.1 == "rdp" {
        println!();
        println!("  Windows:  mstsc /v:127.0.0.1:{}", local.port());
    }
    println!();
    println!("Leave this running. Ctrl+C to disconnect.");
    println!("Scripts still use: gpumesh run --peer {peer} …");

    let quic = conn.connection.clone();
    loop {
        let (tcp, _) = listener
            .accept()
            .await
            .map_err(|e| GpuMeshError::Network(e.to_string()))?;
        let quic = quic.clone();
        tokio::spawn(async move {
            if let Err(e) = client_tunnel(quic, tcp).await {
                warn!("client desktop tunnel: {e}");
            }
        });
    }
}

async fn client_tunnel(quic: quinn::Connection, tcp: TcpStream) -> Result<()> {
    let (mut send, mut recv) = quic
        .open_bi()
        .await
        .map_err(|e| GpuMeshError::Network(e.to_string()))?;
    send.write_all(b"GMDT")
        .await
        .map_err(|e| GpuMeshError::Network(e.to_string()))?;
    let mut magic = [0u8; 4];
    recv.read_exact(&mut magic)
        .await
        .map_err(|e| GpuMeshError::Network(e.to_string()))?;
    if &magic != b"GMDT" {
        return Err(GpuMeshError::Protocol("bad desktop tunnel ack".into()));
    }

    let (mut tcp_r, mut tcp_w) = tcp.into_split();
    tokio::select! {
        r = tokio::io::copy(&mut tcp_r, &mut send) => { let _ = r; }
        r = tokio::io::copy(&mut recv, &mut tcp_w) => { let _ = r; }
    }
    Ok(())
}
