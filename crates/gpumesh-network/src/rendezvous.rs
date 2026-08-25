//! Optional HTTP rendezvous (signaling / metadata only — never workloads).
//! Phase 7 adds a public GPU listing + search registry.

use gpumesh_common::{GpuMeshError, Result};
use gpumesh_security::PublicListingPayload;
use serde::{Deserialize, Serialize};
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendezvousAnnounce {
    pub node_id: String,
    pub node_name: String,
    pub public_key_hex: String,
    pub addrs: Vec<String>,
    pub sharing: bool,
    pub gpu_model: Option<String>,
    pub vram_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vram_free_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utilization: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendezvousPeer {
    pub node_id: String,
    pub node_name: String,
    pub addrs: Vec<String>,
    pub gpu_model: Option<String>,
    pub vram_mb: Option<u64>,
    #[serde(default)]
    pub vram_free_mb: Option<u64>,
    #[serde(default)]
    pub utilization: Option<u32>,
    pub sharing: bool,
}

/// Public registry listing (Phase 7) — metadata only; does not grant job access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicListing {
    pub node_id: String,
    pub node_name: String,
    pub public_key_hex: String,
    #[serde(default)]
    pub addrs: Vec<String>,
    pub gpu_model: Option<String>,
    pub vram_mb: Option<u64>,
    pub vram_free_mb: Option<u64>,
    pub cuda_version: Option<String>,
    /// idle | busy | offline
    pub availability: String,
    /// Heuristic 0–100 (free VRAM + inverse util); not a marketplace price.
    pub perf_score: Option<u32>,
    /// Optional self-reported / measured RTT ms (often filled by search client).
    pub latency_ms: Option<u32>,
    pub region: Option<String>,
    pub uptime_secs: Option<u64>,
    pub sharing: bool,
    pub public: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utilization: Option<u32>,
    #[serde(default)]
    pub issued_at: i64,
    #[serde(default)]
    pub signature: String,
}

impl PublicListingPayload for PublicListing {
    fn node_id(&self) -> &str {
        &self.node_id
    }

    fn public_key_hex(&self) -> &str {
        &self.public_key_hex
    }

    fn issued_at(&self) -> i64 {
        self.issued_at
    }

    fn set_issued_at(&mut self, value: i64) {
        self.issued_at = value;
    }

    fn signature(&self) -> &str {
        &self.signature
    }

    fn set_signature(&mut self, value: String) {
        self.signature = value;
    }

    fn canonical_bytes(&self) -> Vec<u8> {
        let mut clone = self.clone();
        clone.signature.clear();
        serde_json::to_vec(&clone).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicUnannounce {
    pub node_id: String,
    pub issued_at: i64,
    pub signature: String,
}

impl PublicUnannounce {
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut clone = self.clone();
        clone.signature.clear();
        serde_json::to_vec(&clone).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Default)]
pub struct PublicSearchQuery {
    pub gpu: Option<String>,
    pub min_vram_mb: Option<u64>,
    pub cuda: Option<String>,
    pub region: Option<String>,
    pub available_only: bool,
}

pub struct RendezvousClient {
    pub base_url: String,
    client: reqwest::Client,
}

impl RendezvousClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn announce(&self, ann: &RendezvousAnnounce) -> Result<()> {
        let url = format!("{}/v1/announce", self.base_url);
        debug!("rendezvous announce → {url}");
        let mut request = self.client.post(&url);
        if let Some(token) = gpumesh_common::api_token() {
            request = request.bearer_auth(token);
        }
        request
            .json(ann)
            .send()
            .await
            .map_err(|e| GpuMeshError::Network(e.to_string()))?
            .error_for_status()
            .map_err(|e| GpuMeshError::Network(e.to_string()))?;
        Ok(())
    }

    pub async fn lookup(&self, node_id: &str) -> Result<Option<RendezvousPeer>> {
        let url = format!("{}/v1/peers/{node_id}", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| GpuMeshError::Network(e.to_string()))?;
        if resp.status().as_u16() == 404 {
            return Ok(None);
        }
        let peer = resp
            .error_for_status()
            .map_err(|e| GpuMeshError::Network(e.to_string()))?
            .json()
            .await
            .map_err(|e| GpuMeshError::Network(e.to_string()))?;
        Ok(Some(peer))
    }

    pub async fn list(&self) -> Result<Vec<RendezvousPeer>> {
        let url = format!("{}/v1/peers", self.base_url);
        let peers = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| GpuMeshError::Network(e.to_string()))?
            .error_for_status()
            .map_err(|e| GpuMeshError::Network(e.to_string()))?
            .json()
            .await
            .map_err(|e| GpuMeshError::Network(e.to_string()))?;
        Ok(peers)
    }

    pub async fn public_announce(&self, listing: &PublicListing) -> Result<()> {
        let url = format!("{}/v1/public/announce", self.base_url);
        debug!("public announce → {url}");
        let mut request = self.client.post(&url);
        if let Some(token) = gpumesh_common::api_token() {
            request = request.bearer_auth(token);
        }
        request
            .json(listing)
            .send()
            .await
            .map_err(|e| GpuMeshError::Network(e.to_string()))?
            .error_for_status()
            .map_err(|e| GpuMeshError::Network(e.to_string()))?;
        Ok(())
    }

    pub async fn public_unannounce(&self, body: &PublicUnannounce) -> Result<()> {
        let url = format!("{}/v1/public/unannounce", self.base_url);
        let mut request = self.client.post(&url);
        if let Some(token) = gpumesh_common::api_token() {
            request = request.bearer_auth(token);
        }
        request
            .json(body)
            .send()
            .await
            .map_err(|e| GpuMeshError::Network(e.to_string()))?
            .error_for_status()
            .map_err(|e| GpuMeshError::Network(e.to_string()))?;
        Ok(())
    }

    pub async fn public_search(&self, q: &PublicSearchQuery) -> Result<Vec<PublicListing>> {
        let mut url = reqwest::Url::parse(&format!("{}/v1/public/search", self.base_url))
            .map_err(|e| GpuMeshError::Network(e.to_string()))?;
        {
            let mut qp = url.query_pairs_mut();
            if let Some(gpu) = &q.gpu {
                qp.append_pair("gpu", gpu);
            }
            if let Some(v) = q.min_vram_mb {
                qp.append_pair("min_vram_mb", &v.to_string());
            }
            if let Some(cuda) = &q.cuda {
                qp.append_pair("cuda", cuda);
            }
            if let Some(region) = &q.region {
                qp.append_pair("region", region);
            }
            if q.available_only {
                qp.append_pair("available", "1");
            }
        }
        let listings = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| GpuMeshError::Network(e.to_string()))?
            .error_for_status()
            .map_err(|e| GpuMeshError::Network(e.to_string()))?
            .json()
            .await
            .map_err(|e| GpuMeshError::Network(e.to_string()))?;
        Ok(listings)
    }
}

/// Simple 0–100 score from free VRAM fraction and inverse utilization.
pub fn compute_perf_score(vram_total: u64, vram_free: u64, util: Option<u32>) -> u32 {
    if vram_total == 0 {
        return 0;
    }
    let free_pct = (vram_free as f64 / vram_total as f64).clamp(0.0, 1.0);
    let util_pct = util.unwrap_or(0) as f64 / 100.0;
    let score = (free_pct * 70.0) + ((1.0 - util_pct) * 30.0);
    score.round().clamp(0.0, 100.0) as u32
}

pub fn availability_label(sharing: bool, util: Option<u32>) -> &'static str {
    if !sharing {
        return "offline";
    }
    if util.unwrap_or(0) >= 50 {
        "busy"
    } else {
        "idle"
    }
}
