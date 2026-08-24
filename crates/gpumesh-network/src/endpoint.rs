use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use gpumesh_common::{GpuMeshError, Result, DEFAULT_AGENT_PORT};
use gpumesh_protocol::{JsonFrameCodec, Message};
use gpumesh_security::NodeIdentity;
use quinn::{ClientConfig, Connection, Endpoint, RecvStream, SendStream, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer, ServerName};
use tokio_util::codec::{FramedRead, FramedWrite};
use tracing::{info, warn};

use crate::peer_conn::PeerConnection;

/// QUIC endpoint bound for listening and dialing peers.
pub struct NetworkEndpoint {
    pub endpoint: Endpoint,
    pub listen_addr: SocketAddr,
    pub identity: Arc<NodeIdentity>,
}

pub enum NetworkEvent {
    Inbound(PeerConnection),
}

impl NetworkEndpoint {
    pub async fn bind(identity: Arc<NodeIdentity>, port: u16) -> Result<Self> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
        let (server_config, _cert) = make_server_config(&identity)?;
        let mut endpoint = Endpoint::server(server_config, addr)
            .map_err(|e| GpuMeshError::Network(e.to_string()))?;
        endpoint.set_default_client_config(make_client_config()?);
        let listen_addr = endpoint
            .local_addr()
            .map_err(|e| GpuMeshError::Network(e.to_string()))?;
        info!("QUIC endpoint listening on {listen_addr}");
        Ok(Self {
            endpoint,
            listen_addr,
            identity,
        })
    }

    pub fn local_addrs(&self) -> Vec<String> {
        let port = self.listen_addr.port();
        let mut addrs = Vec::new();
        if let Ok(ip) = local_ip_address::local_ip() {
            addrs.push(format!("{ip}:{port}"));
        }
        addrs.push(format!("127.0.0.1:{port}"));
        addrs
    }

    pub async fn accept(&self) -> Result<PeerConnection> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or_else(|| GpuMeshError::Network("endpoint closed".into()))?;
        let conn = incoming
            .await
            .map_err(|e| GpuMeshError::Network(e.to_string()))?;
        PeerConnection::from_connection(conn, true).await
    }

    pub async fn dial(&self, addr: &str) -> Result<PeerConnection> {
        let socket: SocketAddr = addr
            .parse()
            .map_err(|e| GpuMeshError::Network(format!("bad addr {addr}: {e}")))?;
        let connecting = self
            .endpoint
            .connect(socket, "gpumesh")
            .map_err(|e| GpuMeshError::Network(e.to_string()))?;
        let conn = connecting
            .await
            .map_err(|e| GpuMeshError::Network(e.to_string()))?;
        PeerConnection::from_connection(conn, false).await
    }

    /// Try each address; return first successful connection.
    pub async fn dial_any(&self, addrs: &[String]) -> Result<PeerConnection> {
        let mut last_err = GpuMeshError::Network("no addresses".into());
        for addr in addrs {
            match self.dial(addr).await {
                Ok(c) => return Ok(c),
                Err(e) => {
                    warn!("dial {addr} failed: {e}");
                    last_err = e;
                }
            }
        }
        Err(last_err)
    }
}

fn make_server_config(identity: &NodeIdentity) -> Result<(ServerConfig, CertificateDer<'static>)> {
    let cert = rcgen::generate_simple_self_signed(vec![
        "gpumesh".into(),
        identity.node_id.clone(),
        "localhost".into(),
    ])
    .map_err(|e| GpuMeshError::Network(e.to_string()))?;
    let key = PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());
    let cert_der = CertificateDer::from(cert.cert);
    let mut server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der.clone()], key.into())
        .map_err(|e| GpuMeshError::Network(e.to_string()))?;
    server_crypto.alpn_protocols = vec![b"gpumesh/1".to_vec()];
    let mut server_config = ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)
            .map_err(|e| GpuMeshError::Network(e.to_string()))?,
    ));
    server_config.transport_config(Arc::new({
        let mut t = quinn::TransportConfig::default();
        t.keep_alive_interval(Some(Duration::from_secs(5)));
        t.max_idle_timeout(Some(Duration::from_secs(60).try_into().unwrap()));
        t
    }));
    Ok((server_config, cert_der))
}

fn make_client_config() -> Result<ClientConfig> {
    let crypto = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
        .with_no_client_auth();
    let mut crypto = crypto;
    crypto.alpn_protocols = vec![b"gpumesh/1".to_vec()];
    let client = ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
            .map_err(|e| GpuMeshError::Network(e.to_string()))?,
    ));
    Ok(client)
}

#[derive(Debug)]
struct SkipServerVerification;

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        // Peer auth is application-layer (Ed25519 Hello), not TLS PKI.
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

pub fn default_listen_port() -> u16 {
    DEFAULT_AGENT_PORT
}

pub async fn open_bi(
    conn: &Connection,
) -> Result<(
    FramedWrite<SendStream, JsonFrameCodec>,
    FramedRead<RecvStream, JsonFrameCodec>,
)> {
    let (send, recv) = conn
        .open_bi()
        .await
        .map_err(|e| GpuMeshError::Network(e.to_string()))?;
    Ok((
        FramedWrite::new(send, JsonFrameCodec),
        FramedRead::new(recv, JsonFrameCodec),
    ))
}

pub async fn accept_bi(
    conn: &Connection,
) -> Result<(
    FramedWrite<SendStream, JsonFrameCodec>,
    FramedRead<RecvStream, JsonFrameCodec>,
)> {
    let (send, recv) = conn
        .accept_bi()
        .await
        .map_err(|e| GpuMeshError::Network(e.to_string()))?;
    Ok((
        FramedWrite::new(send, JsonFrameCodec),
        FramedRead::new(recv, JsonFrameCodec),
    ))
}

pub async fn send_msg(
    send: &mut FramedWrite<SendStream, JsonFrameCodec>,
    msg: Message,
) -> Result<()> {
    use futures::SinkExt;
    send.send(msg)
        .await
        .map_err(|e| GpuMeshError::Network(e.to_string()))
}

pub async fn recv_msg(
    recv: &mut FramedRead<RecvStream, JsonFrameCodec>,
) -> Result<Option<Message>> {
    use futures::StreamExt;
    match recv.next().await {
        Some(Ok(m)) => Ok(Some(m)),
        Some(Err(e)) => Err(GpuMeshError::Network(e.to_string())),
        None => Ok(None),
    }
}
