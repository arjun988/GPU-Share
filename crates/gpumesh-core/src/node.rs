use std::sync::Arc;

use chrono::Utc;
use gpumesh_common::{
    parse_size_to_mb, GpuMeshError, NodeConfig, PeerStatus, Result, ShareLimits, PROTOCOL_MAJOR,
    PROTOCOL_MINOR,
};
use gpumesh_gpu::{GpuInfo, GpuMonitor};
use gpumesh_network::{dial_with_fallback, LanDiscovery, NetworkEndpoint, PeerConnection, RendezvousClient};
use gpumesh_protocol::{Message, PeerInfoMsg, ProtocolHello};
use gpumesh_runtime::DockerRuntime;
use gpumesh_security::{AllowList, NodeIdentity, PairingPayload};
use gpumesh_storage::{PeerRecord, PeerStore, StateStore};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::handshake::{perform_client_handshake, perform_server_handshake};

pub struct MeshNode {
    pub identity: Arc<NodeIdentity>,
    pub config: Arc<RwLock<NodeConfig>>,
    pub allowlist: Arc<RwLock<AllowList>>,
    pub peers: Arc<RwLock<PeerStore>>,
    pub runtime: Arc<DockerRuntime>,
    pub endpoint: Option<Arc<NetworkEndpoint>>,
    pub lan: Option<LanDiscovery>,
    pub sharing: Arc<RwLock<bool>>,
}

impl MeshNode {
    pub async fn bootstrap() -> Result<Self> {
        StateStore::ensure_dirs()?;
        if !StateStore::is_initialized() {
            return Err(GpuMeshError::NotInitialized);
        }
        let identity = Arc::new(NodeIdentity::load_default()?);
        let config = StateStore::load_config()?;
        let allowlist = StateStore::load_allowlist()?;
        let peers = PeerStore::load()?;
        Ok(Self {
            identity,
            config: Arc::new(RwLock::new(config)),
            allowlist: Arc::new(RwLock::new(allowlist)),
            peers: Arc::new(RwLock::new(peers)),
            runtime: Arc::new(DockerRuntime::new()),
            endpoint: None,
            lan: None,
            sharing: Arc::new(RwLock::new(false)),
        })
    }

    pub fn init(node_name: Option<String>) -> Result<(NodeIdentity, NodeConfig)> {
        StateStore::ensure_dirs()?;
        let identity = NodeIdentity::load_or_create(&gpumesh_common::identity_path())?;
        let mut cfg = NodeConfig::default();
        if let Some(name) = node_name {
            cfg.node_name = name;
        }
        StateStore::save_config(&cfg)?;
        StateStore::save_allowlist(&AllowList::default())?;
        let _ = PeerStore::load()?.save();
        Ok((identity, cfg))
    }

    pub async fn start_network(&mut self) -> Result<()> {
        let cfg = self.config.read().await.clone();
        let endpoint = NetworkEndpoint::bind(self.identity.clone(), cfg.listen_port).await?;
        let lan = LanDiscovery::start(
            &self.identity.node_id,
            &cfg.node_name,
            endpoint.listen_addr.port(),
        )
        .ok();
        self.endpoint = Some(Arc::new(endpoint));
        self.lan = lan;
        Ok(())
    }

    pub fn endpoint(&self) -> Result<Arc<NetworkEndpoint>> {
        self.endpoint
            .clone()
            .ok_or_else(|| GpuMeshError::Network("network not started".into()))
    }

    pub async fn enable_share(
        &self,
        max_vram: Option<String>,
        max_util: Option<u8>,
    ) -> Result<()> {
        let mut cfg = self.config.write().await;
        if let Some(v) = max_vram {
            cfg.max_vram_mb = Some(parse_size_to_mb(&v)?);
        }
        if let Some(u) = max_util {
            cfg.max_gpu_utilization = Some(u);
        }
        cfg.sharing_enabled = true;
        StateStore::save_config(&cfg)?;
        *self.sharing.write().await = true;

        if let Some(url) = &cfg.rendezvous_url {
            let gpus = GpuMonitor::detect().unwrap_or_default();
            let ann = gpumesh_network::RendezvousAnnounce {
                node_id: self.identity.node_id.clone(),
                node_name: cfg.node_name.clone(),
                public_key_hex: self.identity.public_key_hex(),
                addrs: self
                    .endpoint
                    .as_ref()
                    .map(|e| e.local_addrs())
                    .unwrap_or_default(),
                sharing: true,
                gpu_model: gpus.first().map(|g| g.name.clone()),
                vram_mb: gpus.first().map(|g| g.vram_total_mb),
            };
            let client = RendezvousClient::new(url);
            if let Err(e) = client.announce(&ann).await {
                warn!("rendezvous announce failed: {e}");
            }
        }
        info!("sharing enabled");
        Ok(())
    }

    pub async fn disable_share(&self) -> Result<()> {
        let mut cfg = self.config.write().await;
        cfg.sharing_enabled = false;
        StateStore::save_config(&cfg)?;
        *self.sharing.write().await = false;
        Ok(())
    }

    pub async fn pairing_code(&self) -> Result<String> {
        let cfg = self.config.read().await.clone();
        let gpus = GpuMonitor::detect().unwrap_or_default();
        let addrs = self
            .endpoint
            .as_ref()
            .map(|e| e.local_addrs())
            .unwrap_or_else(|| vec![format!("127.0.0.1:{}", cfg.listen_port)]);
        let payload = PairingPayload {
            version: 1,
            node_id: self.identity.node_id.clone(),
            node_name: cfg.node_name.clone(),
            public_key_hex: self.identity.public_key_hex(),
            addrs,
            gpu_model: gpus.first().map(|g| g.name.clone()),
            vram_mb: gpus.first().map(|g| g.vram_total_mb),
            issued_at: Utc::now().timestamp(),
            signature: String::new(),
        };
        let signed = PairingPayload::sign_with(&self.identity, payload);
        signed.encode_code()
    }

    pub async fn pair_with_code(&self, code: &str) -> Result<PeerRecord> {
        let payload = PairingPayload::decode_code(code)?;
        let record = PeerRecord {
            node_id: payload.node_id.clone(),
            node_name: payload.node_name.clone(),
            public_key_hex: payload.public_key_hex.clone(),
            addrs: payload.addrs.clone(),
            gpu_model: payload.gpu_model.clone(),
            vram_mb: payload.vram_mb,
            paired_at: Utc::now().timestamp(),
            last_seen: None,
        };
        {
            let mut store = self.peers.write().await;
            store.upsert(record.clone());
            store.save()?;
        }
        // Pairing establishes candidate trust; auto-allow the peer for MVP UX.
        {
            let mut allow = self.allowlist.write().await;
            allow.allow(&record.node_id);
            StateStore::save_allowlist(&allow)?;
        }
        Ok(record)
    }

    pub async fn allow_peer(&self, peer: &str) -> Result<()> {
        let store = self.peers.read().await;
        let rec = store
            .get(peer)
            .ok_or_else(|| GpuMeshError::PeerNotFound(peer.into()))?;
        let id = rec.node_id.clone();
        drop(store);
        let mut allow = self.allowlist.write().await;
        allow.allow(id);
        StateStore::save_allowlist(&allow)?;
        Ok(())
    }

    pub async fn deny_peer(&self, peer: &str) -> Result<()> {
        let store = self.peers.read().await;
        let rec = store
            .get(peer)
            .ok_or_else(|| GpuMeshError::PeerNotFound(peer.into()))?;
        let id = rec.node_id.clone();
        drop(store);
        let mut allow = self.allowlist.write().await;
        allow.deny(id);
        StateStore::save_allowlist(&allow)?;
        Ok(())
    }

    pub async fn connect_peer(&self, peer: &str) -> Result<PeerConnection> {
        let store = self.peers.read().await;
        let rec = store
            .get(peer)
            .ok_or_else(|| GpuMeshError::PeerNotFound(peer.into()))?
            .clone();
        drop(store);
        let endpoint = self.endpoint()?;
        let mut conn = dial_with_fallback(&endpoint, &rec.addrs, &rec.node_id).await?;
        let hello = self.make_hello().await;
        let peer_hello = perform_client_handshake(&mut conn, hello).await?;
        conn.peer_node_id = Some(peer_hello.node_id.clone());
        conn.peer_name = Some(peer_hello.node_name.clone());
        if let Some(p) = self.peers.write().await.get_mut(&rec.node_id) {
            p.last_seen = Some(Utc::now().timestamp());
            p.gpu_model = peer_hello.gpu_model.clone();
            p.vram_mb = peer_hello.vram_total_mb;
        }
        let _ = self.peers.read().await.save();
        Ok(conn)
    }

    pub async fn make_hello(&self) -> ProtocolHello {
        let cfg = self.config.read().await.clone();
        let gpus = GpuMonitor::detect().unwrap_or_default();
        let sharing = *self.sharing.read().await || cfg.sharing_enabled;
        let status = if sharing {
            let active = self.runtime.active_count().await;
            if active > 0 {
                PeerStatus::Busy
            } else {
                PeerStatus::Idle
            }
        } else {
            PeerStatus::Unknown
        };
        ProtocolHello {
            major: PROTOCOL_MAJOR,
            minor: PROTOCOL_MINOR,
            node_id: self.identity.node_id.clone(),
            node_name: cfg.node_name,
            public_key_hex: self.identity.public_key_hex(),
            sharing,
            gpu_model: gpus.first().map(|g| g.name.clone()),
            vram_total_mb: gpus.first().map(|g| g.vram_total_mb),
            vram_free_mb: gpus.first().map(|g| g.vram_free_mb),
            status,
        }
    }

    pub async fn peer_info_msg(&self) -> PeerInfoMsg {
        let hello = self.make_hello().await;
        let gpus = GpuMonitor::detect().unwrap_or_default();
        PeerInfoMsg {
            node_id: hello.node_id,
            node_name: hello.node_name,
            gpu_model: hello.gpu_model,
            vram_total_mb: hello.vram_total_mb,
            vram_free_mb: hello.vram_free_mb,
            utilization: gpus.first().and_then(|g| g.utilization_gpu),
            temperature_c: gpus.first().and_then(|g| g.temperature_c),
            status: hello.status,
            sharing: hello.sharing,
            addrs: self
                .endpoint
                .as_ref()
                .map(|e| e.local_addrs())
                .unwrap_or_default(),
        }
    }

    pub async fn authorize_inbound(&self, node_id: &str) -> Result<()> {
        let allow = self.allowlist.read().await;
        if !allow.is_allowed(node_id) {
            return Err(GpuMeshError::NotAuthorized(node_id.into()));
        }
        Ok(())
    }

    pub async fn share_limits(&self) -> ShareLimits {
        let cfg = self.config.read().await;
        ShareLimits::from_config(&cfg)
    }

    pub fn detect_gpus() -> Result<Vec<GpuInfo>> {
        GpuMonitor::detect()
    }

    pub async fn handle_inbound(&self, mut conn: PeerConnection) -> Result<()> {
        let hello = self.make_hello().await;
        let peer_hello = perform_server_handshake(&mut conn, hello).await?;
        if let Err(e) = self.authorize_inbound(&peer_hello.node_id).await {
            conn.send(Message::Error {
                message: format!("denied: {e}"),
            })
            .await?;
            conn.close();
            return Err(e);
        }
        conn.peer_node_id = Some(peer_hello.node_id.clone());
        conn.peer_name = Some(peer_hello.node_name.clone());
        crate::remote::serve_peer_session(self, conn).await
    }
}
