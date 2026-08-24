//! Optional HTTP rendezvous (signaling / metadata only — never workloads).

use gpumesh_common::{GpuMeshError, Result};
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendezvousPeer {
    pub node_id: String,
    pub node_name: String,
    pub addrs: Vec<String>,
    pub gpu_model: Option<String>,
    pub vram_mb: Option<u64>,
    pub sharing: bool,
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
        self.client
            .post(&url)
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
}
