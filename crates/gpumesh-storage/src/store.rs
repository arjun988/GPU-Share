use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use gpumesh_common::{
    allowlist_path, config_dir, config_path, jobs_dir, peers_path, work_dir, GpuMeshError,
    NodeConfig, Result,
};
use gpumesh_security::AllowList;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerRecord {
    pub node_id: String,
    pub node_name: String,
    pub public_key_hex: String,
    pub addrs: Vec<String>,
    pub gpu_model: Option<String>,
    pub vram_mb: Option<u64>,
    pub paired_at: i64,
    pub last_seen: Option<i64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PeersFile {
    peers: HashMap<String, PeerRecord>,
}

pub struct PeerStore {
    path: PathBuf,
    peers: HashMap<String, PeerRecord>,
}

impl PeerStore {
    pub fn load() -> Result<Self> {
        let path = peers_path();
        let peers = if path.exists() {
            let data = fs::read_to_string(&path)?;
            let file: PeersFile =
                serde_json::from_str(&data).map_err(|e| GpuMeshError::Storage(e.to_string()))?;
            file.peers
        } else {
            HashMap::new()
        };
        Ok(Self { path, peers })
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = PeersFile {
            peers: self.peers.clone(),
        };
        let data = serde_json::to_string_pretty(&file)
            .map_err(|e| GpuMeshError::Storage(e.to_string()))?;
        fs::write(&self.path, data)?;
        Ok(())
    }

    pub fn upsert(&mut self, record: PeerRecord) {
        self.peers.insert(record.node_id.clone(), record);
    }

    pub fn get(&self, key: &str) -> Option<&PeerRecord> {
        if let Some(p) = self.peers.get(key) {
            return Some(p);
        }
        self.peers
            .values()
            .find(|p| p.node_name.eq_ignore_ascii_case(key) || p.node_id.starts_with(key))
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut PeerRecord> {
        let id = self.get(key).map(|p| p.node_id.clone())?;
        self.peers.get_mut(&id)
    }

    pub fn list(&self) -> Vec<&PeerRecord> {
        let mut v: Vec<_> = self.peers.values().collect();
        v.sort_by(|a, b| a.node_name.cmp(&b.node_name));
        v
    }

    pub fn remove(&mut self, key: &str) -> bool {
        if self.peers.remove(key).is_some() {
            return true;
        }
        if let Some(id) = self
            .peers
            .values()
            .find(|p| p.node_name.eq_ignore_ascii_case(key))
            .map(|p| p.node_id.clone())
        {
            return self.peers.remove(&id).is_some();
        }
        false
    }
}

pub struct StateStore;

impl StateStore {
    pub fn ensure_dirs() -> Result<()> {
        fs::create_dir_all(config_dir())?;
        fs::create_dir_all(jobs_dir())?;
        fs::create_dir_all(work_dir())?;
        Ok(())
    }

    pub fn is_initialized() -> bool {
        gpumesh_common::identity_path().exists()
    }

    pub fn load_config() -> Result<NodeConfig> {
        let path = config_path();
        if !path.exists() {
            return Ok(NodeConfig::default());
        }
        let data = fs::read_to_string(path)?;
        toml::from_str(&data).map_err(|e| GpuMeshError::Storage(e.to_string()))
    }

    pub fn save_config(cfg: &NodeConfig) -> Result<()> {
        Self::ensure_dirs()?;
        let data = toml::to_string_pretty(cfg).map_err(|e| GpuMeshError::Storage(e.to_string()))?;
        fs::write(config_path(), data)?;
        Ok(())
    }

    pub fn load_allowlist() -> Result<AllowList> {
        let path = allowlist_path();
        if !path.exists() {
            return Ok(AllowList::default());
        }
        let data = fs::read_to_string(path)?;
        serde_json::from_str(&data).map_err(|e| GpuMeshError::Storage(e.to_string()))
    }

    pub fn save_allowlist(list: &AllowList) -> Result<()> {
        Self::ensure_dirs()?;
        let data =
            serde_json::to_string_pretty(list).map_err(|e| GpuMeshError::Storage(e.to_string()))?;
        fs::write(allowlist_path(), data)?;
        Ok(())
    }

    pub fn job_dir(job_id: &str) -> PathBuf {
        jobs_dir().join(job_id)
    }

    pub fn ensure_job_dir(job_id: &str) -> Result<PathBuf> {
        let dir = Self::job_dir(job_id);
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}

pub fn write_bytes(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, data)?;
    Ok(())
}
