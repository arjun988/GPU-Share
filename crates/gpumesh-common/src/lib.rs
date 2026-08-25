//! Shared errors, paths, and configuration for GPUMesh.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PROTOCOL_MAJOR: u16 = 1;
pub const PROTOCOL_MINOR: u16 = 1;
pub const DEFAULT_AGENT_PORT: u16 = 47000;
pub const DEFAULT_IMAGE: &str = "nvidia/cuda:12.8.0-runtime-ubuntu22.04";
pub const APP_DIR_NAME: &str = ".gpumesh";
/// Pairing / invite codes expire after this many seconds.
pub const PAIRING_TTL_SECS: i64 = 3600;
/// Signed Hello messages older than this are rejected.
pub const HELLO_TTL_SECS: i64 = 300;
/// Public announce signatures must be fresher than this.
pub const ANNOUNCE_TTL_SECS: i64 = 180;

#[derive(Debug, Error)]
pub enum GpuMeshError {
    #[error("not initialized: run `gpumesh init` first")]
    NotInitialized,

    #[error("identity error: {0}")]
    Identity(String),

    #[error("peer not found: {0}")]
    PeerNotFound(String),

    #[error("peer denied: {0}")]
    PeerDenied(String),

    #[error("not authorized: {0}")]
    NotAuthorized(String),

    #[error("network error: {0}")]
    Network(String),

    #[error("runtime error: {0}")]
    Runtime(String),

    #[error("gpu error: {0}")]
    Gpu(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("job error: {0}")]
    Job(String),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, GpuMeshError>;

impl From<anyhow::Error> for GpuMeshError {
    fn from(value: anyhow::Error) -> Self {
        GpuMeshError::Other(value.to_string())
    }
}

/// Resolve config root.
/// Override with `GPUMESH_HOME` (full path to the `.gpumesh` directory or a parent
/// that should contain it — if the env value ends with `.gpumesh` it is used as-is).
pub fn config_dir() -> PathBuf {
    if let Ok(home) = std::env::var("GPUMESH_HOME") {
        let p = PathBuf::from(home);
        if p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n == APP_DIR_NAME)
        {
            return p;
        }
        return p.join(APP_DIR_NAME);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DIR_NAME)
}

pub fn identity_path() -> PathBuf {
    config_dir().join("identity.json")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn peers_path() -> PathBuf {
    config_dir().join("peers.json")
}

pub fn allowlist_path() -> PathBuf {
    config_dir().join("allowlist.json")
}

pub fn jobs_dir() -> PathBuf {
    config_dir().join("jobs")
}

pub fn work_dir() -> PathBuf {
    config_dir().join("work")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NodeConfig {
    pub node_name: String,
    pub listen_port: u16,
    pub default_image: String,
    pub rendezvous_url: Option<String>,
    pub max_vram_mb: Option<u64>,
    pub max_gpu_utilization: Option<u8>,
    pub max_concurrent_jobs: u32,
    pub max_runtime_secs: Option<u64>,
    pub max_cpu_cores: Option<f64>,
    pub max_ram_mb: Option<u64>,
    pub max_disk_mb: Option<u64>,
    pub sharing_enabled: bool,
    /// Publish GPU metadata to the public registry (Phase 7). Does not open allowlist.
    pub public_listing: bool,
    /// Optional region label for public search (e.g. us-west, eu, in).
    pub region: Option<String>,
    /// Default job retries for remote runs (Phase 4).
    pub default_retries: u32,
    /// Update check URL (JSON with `version` + `url` fields).
    pub update_url: Option<String>,
    /// Allowed container image prefixes/names for remote jobs.
    #[serde(default = "default_allowed_images")]
    pub allowed_images: Vec<String>,
    /// Harden Docker: drop caps, no-new-privileges, read-only rootfs.
    #[serde(default = "default_true")]
    pub docker_harden: bool,
}

fn default_true() -> bool {
    true
}

impl Default for NodeConfig {
    fn default() -> Self {
        let hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "gpumesh-node".into());
        Self {
            node_name: hostname,
            listen_port: DEFAULT_AGENT_PORT,
            default_image: DEFAULT_IMAGE.to_string(),
            rendezvous_url: None,
            max_vram_mb: None,
            max_gpu_utilization: None,
            max_concurrent_jobs: 1,
            max_runtime_secs: Some(3600),
            max_cpu_cores: Some(4.0),
            max_ram_mb: Some(8192),
            max_disk_mb: None,
            sharing_enabled: false,
            public_listing: false,
            region: None,
            default_retries: 0,
            update_url: Some(
                "https://raw.githubusercontent.com/gpumesh/gpumesh/main/dist/latest.json".into(),
            ),
            allowed_images: default_allowed_images(),
            docker_harden: true,
        }
    }
}

pub fn default_allowed_images() -> Vec<String> {
    vec![
        "nvidia/cuda".into(),
        "python".into(),
        "pytorch".into(),
        "tensorflow".into(),
        "nvcr.io".into(),
        DEFAULT_IMAGE.into(),
    ]
}

pub fn share_pid_path() -> PathBuf {
    config_dir().join("share.pid")
}

pub fn share_stop_path() -> PathBuf {
    config_dir().join("share.stop")
}

pub fn api_token() -> Option<String> {
    std::env::var("GPUMESH_API_TOKEN")
        .ok()
        .filter(|s| !s.is_empty())
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn logs_dir() -> PathBuf {
    config_dir().join("logs")
}

pub fn agent_log_path() -> PathBuf {
    logs_dir().join("agent.log")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PeerStatus {
    Idle,
    Busy,
    Offline,
    Unknown,
}

impl std::fmt::Display for PeerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerStatus::Idle => write!(f, "IDLE"),
            PeerStatus::Busy => write!(f, "BUSY"),
            PeerStatus::Offline => write!(f, "OFFLINE"),
            PeerStatus::Unknown => write!(f, "UNKNOWN"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Pending,
    Transferring,
    Starting,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl std::fmt::Display for JobState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            JobState::Pending => "PENDING",
            JobState::Transferring => "TRANSFERRING",
            JobState::Starting => "STARTING",
            JobState::Running => "RUNNING",
            JobState::Succeeded => "SUCCEEDED",
            JobState::Failed => "FAILED",
            JobState::Cancelled => "CANCELLED",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareLimits {
    pub max_vram_mb: Option<u64>,
    pub max_gpu_utilization: Option<u8>,
    pub max_concurrent_jobs: u32,
    pub max_runtime_secs: Option<u64>,
    pub max_cpu_cores: Option<f64>,
    pub max_ram_mb: Option<u64>,
    pub max_disk_mb: Option<u64>,
}

impl ShareLimits {
    pub fn from_config(cfg: &NodeConfig) -> Self {
        Self {
            max_vram_mb: cfg.max_vram_mb,
            max_gpu_utilization: cfg.max_gpu_utilization,
            max_concurrent_jobs: cfg.max_concurrent_jobs,
            max_runtime_secs: cfg.max_runtime_secs,
            max_cpu_cores: cfg.max_cpu_cores,
            max_ram_mb: cfg.max_ram_mb,
            max_disk_mb: cfg.max_disk_mb,
        }
    }
}

/// Parse human sizes like `16GB`, `512MB`, `1G`.
pub fn parse_size_to_mb(input: &str) -> Result<u64> {
    let s = input.trim().to_uppercase().replace(' ', "");
    let (num, mult) = if let Some(n) = s.strip_suffix("GB") {
        (n, 1024u64)
    } else if let Some(n) = s.strip_suffix('G') {
        (n, 1024)
    } else if let Some(n) = s.strip_suffix("MB") {
        (n, 1)
    } else if let Some(n) = s.strip_suffix('M') {
        (n, 1)
    } else if let Some(n) = s.strip_suffix("KB") {
        return Ok(n
            .parse::<u64>()
            .map_err(|_| GpuMeshError::Other(format!("invalid size: {input}")))?
            / 1024);
    } else {
        (s.as_str(), 1)
    };
    let n: f64 = num
        .parse()
        .map_err(|_| GpuMeshError::Other(format!("invalid size: {input}")))?;
    Ok((n * mult as f64) as u64)
}
