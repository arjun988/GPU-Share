use std::sync::Arc;

use chrono::Utc;
use gpumesh_common::{
    parse_size_to_mb, GpuMeshError, NodeConfig, PeerStatus, Result, ShareLimits, PROTOCOL_MAJOR,
    PROTOCOL_MINOR,
};
use gpumesh_gpu::{GpuInfo, GpuMonitor};
use gpumesh_network::{
    availability_label, compute_perf_score, dial_with_fallback, LanDiscovery, NetworkEndpoint,
    PeerConnection, PublicListing, PublicUnannounce, RendezvousAnnounce, RendezvousClient,
};
use gpumesh_protocol::{Message, PeerInfoMsg, ProtocolHello};
use gpumesh_runtime::DockerRuntime;
use gpumesh_security::{
    sign_hello, sign_public_listing, verify_hello, AllowList, NodeIdentity, PairingPayload,
};
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
    /// Unix timestamp when sharing started (for public uptime).
    pub sharing_since: Arc<RwLock<Option<i64>>>,
    /// Interactive desktop tunnels enabled.
    pub desktop_sharing: Arc<RwLock<bool>>,
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
            sharing_since: Arc::new(RwLock::new(None)),
            desktop_sharing: Arc::new(RwLock::new(false)),
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
        if self.endpoint.is_some() {
            return Ok(());
        }
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
        public: bool,
        region: Option<String>,
    ) -> Result<()> {
        let mut cfg = self.config.write().await;
        if let Some(v) = max_vram {
            cfg.max_vram_mb = Some(parse_size_to_mb(&v)?);
        }
        if let Some(u) = max_util {
            cfg.max_gpu_utilization = Some(u);
        }
        if let Some(r) = region {
            cfg.region = if r.is_empty() { None } else { Some(r) };
        }
        cfg.sharing_enabled = true;
        cfg.public_listing = public;
        StateStore::save_config(&cfg)?;
        *self.sharing.write().await = true;
        *self.sharing_since.write().await = Some(Utc::now().timestamp());

        if let Some(url) = &cfg.rendezvous_url {
            let gpus = GpuMonitor::detect().unwrap_or_default();
            let g = gpus.first();
            let ann = RendezvousAnnounce {
                node_id: self.identity.node_id.clone(),
                node_name: cfg.node_name.clone(),
                public_key_hex: self.identity.public_key_hex(),
                addrs: self
                    .endpoint
                    .as_ref()
                    .map(|e| e.local_addrs())
                    .unwrap_or_default(),
                sharing: true,
                gpu_model: g.map(|g| g.name.clone()),
                vram_mb: g.map(|g| g.vram_total_mb),
                vram_free_mb: g.map(|g| g.vram_free_mb),
                utilization: g.and_then(|g| g.utilization_gpu.map(|u| u as u32)),
            };
            let client = RendezvousClient::new(url);
            if let Err(e) = client.announce(&ann).await {
                warn!("rendezvous announce failed: {e}");
            }
            if public {
                let listing = self.build_public_listing_inner(&cfg, g).await;
                if let Err(e) = client.public_announce(&listing).await {
                    warn!("public announce failed: {e}");
                }
            }
        } else if public {
            warn!("--public requires rendezvous_url (gpumesh config set rendezvous_url …)");
        }
        info!("sharing enabled (public={public})");
        Ok(())
    }

    pub async fn disable_share(&self) -> Result<()> {
        let mut cfg = self.config.write().await;
        let was_public = cfg.public_listing;
        let url = cfg.rendezvous_url.clone();
        cfg.sharing_enabled = false;
        cfg.public_listing = false;
        StateStore::save_config(&cfg)?;
        *self.sharing.write().await = false;
        *self.sharing_since.write().await = None;

        if was_public {
            if let Some(url) = url {
                let client = RendezvousClient::new(url);
                let mut body = PublicUnannounce {
                    node_id: self.identity.node_id.clone(),
                    issued_at: Utc::now().timestamp(),
                    signature: String::new(),
                };
                body.signature = self.identity.sign_b64(&body.canonical_bytes());
                if let Err(e) = client.public_unannounce(&body).await {
                    warn!("public unannounce failed: {e}");
                }
            }
        }
        Ok(())
    }

    pub async fn publish_public_listing(&self) -> Result<()> {
        let cfg = self.config.read().await.clone();
        if !cfg.public_listing {
            return Ok(());
        }
        let Some(url) = cfg.rendezvous_url.clone() else {
            return Ok(());
        };
        let gpus = GpuMonitor::detect().unwrap_or_default();
        let listing = self.build_public_listing_inner(&cfg, gpus.first()).await;
        RendezvousClient::new(url).public_announce(&listing).await
    }

    async fn build_public_listing_inner(
        &self,
        cfg: &NodeConfig,
        g: Option<&GpuInfo>,
    ) -> PublicListing {
        let sharing = *self.sharing.read().await;
        let util = g.and_then(|g| g.utilization_gpu.map(|u| u as u32));
        let vram_total = g.map(|g| g.vram_total_mb).unwrap_or(0);
        let vram_free = g.map(|g| g.vram_free_mb).unwrap_or(0);
        let since = *self.sharing_since.read().await;
        let uptime = since.map(|t| (Utc::now().timestamp() - t).max(0) as u64);
        let mut listing = PublicListing {
            node_id: self.identity.node_id.clone(),
            node_name: cfg.node_name.clone(),
            public_key_hex: self.identity.public_key_hex(),
            addrs: vec![],
            gpu_model: g.map(|g| g.name.clone()),
            vram_mb: g.map(|g| g.vram_total_mb),
            vram_free_mb: g.map(|g| g.vram_free_mb),
            cuda_version: g.and_then(|g| g.cuda_version.clone()),
            availability: availability_label(sharing, util).to_string(),
            perf_score: Some(compute_perf_score(vram_total, vram_free, util)),
            latency_ms: None,
            region: cfg.region.clone(),
            uptime_secs: uptime,
            sharing,
            public: true,
            utilization: util,
            issued_at: 0,
            signature: String::new(),
        };
        sign_public_listing(&self.identity, &mut listing);
        listing
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

    pub async fn allow_desktop_peer(&self, peer: &str) -> Result<()> {
        let store = self.peers.read().await;
        let rec = store
            .get(peer)
            .ok_or_else(|| GpuMeshError::PeerNotFound(peer.into()))?;
        let id = rec.node_id.clone();
        drop(store);
        let mut allow = self.allowlist.write().await;
        allow.allow_desktop(id);
        StateStore::save_allowlist(&allow)?;
        Ok(())
    }

    pub async fn enable_desktop_share(&self) -> Result<()> {
        let mut cfg = self.config.write().await;
        cfg.desktop_sharing = true;
        // Desktop share also accepts job connections so scripts work in the same session.
        cfg.sharing_enabled = true;
        StateStore::save_config(&cfg)?;
        *self.desktop_sharing.write().await = true;
        *self.sharing.write().await = true;
        if self.sharing_since.read().await.is_none() {
            *self.sharing_since.write().await = Some(Utc::now().timestamp());
        }
        info!("desktop sharing enabled");
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
        let mut addrs = rec.addrs.clone();
        // Merge LAN discovery addrs for this peer if available.
        if let Some(lan) = &self.lan {
            for d in lan.peers() {
                if d.node_id == rec.node_id && !d.addr.is_empty() && !addrs.contains(&d.addr) {
                    addrs.push(d.addr);
                }
            }
        }
        let mut conn = dial_with_fallback(&endpoint, &addrs, &rec.node_id).await?;
        let hello = self.make_hello().await;
        let peer_hello = perform_client_handshake(&mut conn, hello).await?;
        verify_hello(&peer_hello)?;
        if peer_hello.node_id != rec.node_id {
            return Err(GpuMeshError::Network(format!(
                "peer identity mismatch: expected {} got {}",
                rec.node_id, peer_hello.node_id
            )));
        }
        if !rec.public_key_hex.is_empty() && peer_hello.public_key_hex != rec.public_key_hex {
            return Err(GpuMeshError::Network(
                "peer public key does not match paired record".into(),
            ));
        }
        conn.peer_node_id = Some(peer_hello.node_id.clone());
        conn.peer_name = Some(peer_hello.node_name.clone());
        if let Some(p) = self.peers.write().await.get_mut(&rec.node_id) {
            p.last_seen = Some(Utc::now().timestamp());
            p.gpu_model = peer_hello.gpu_model.clone();
            p.vram_mb = peer_hello.vram_total_mb;
            p.public_key_hex = peer_hello.public_key_hex.clone();
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
        let mut hello = ProtocolHello {
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
            issued_at: 0,
            signature: String::new(),
        };
        sign_hello(&self.identity, &mut hello);
        hello
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

    pub async fn authorize_inbound(&self, hello: &ProtocolHello) -> Result<()> {
        verify_hello(hello)?;
        let allow = self.allowlist.read().await;
        let ok = allow.is_allowed(&hello.node_id) || allow.is_desktop_allowed(&hello.node_id);
        if !ok {
            return Err(GpuMeshError::NotAuthorized(hello.node_id.clone()));
        }
        // If we have a stored peer record, public key must match (prevents ID spoof with new key).
        if let Some(rec) = self.peers.read().await.get(&hello.node_id) {
            if !rec.public_key_hex.is_empty() && rec.public_key_hex != hello.public_key_hex {
                return Err(GpuMeshError::NotAuthorized(
                    "public key does not match paired peer".into(),
                ));
            }
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
        if let Err(e) = self.authorize_inbound(&peer_hello).await {
            conn.send(Message::Error {
                message: format!("denied: {e}"),
            })
            .await?;
            conn.close();
            return Err(e);
        }
        conn.peer_node_id = Some(peer_hello.node_id.clone());
        conn.peer_name = Some(peer_hello.node_name.clone());
        // Refresh stored key from verified hello
        if let Some(p) = self.peers.write().await.get_mut(&peer_hello.node_id) {
            p.public_key_hex = peer_hello.public_key_hex.clone();
            p.last_seen = Some(Utc::now().timestamp());
        }
        let _ = self.peers.read().await.save();
        crate::remote::serve_peer_session(self, conn).await
    }
}
