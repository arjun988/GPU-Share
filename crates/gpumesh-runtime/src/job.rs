use std::path::PathBuf;

use gpumesh_common::{JobState, ShareLimits};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRequest {
    pub job_id: String,
    pub image: String,
    pub command: Vec<String>,
    pub env: Vec<(String, String)>,
    pub host_workdir: PathBuf,
    pub container_workdir: String,
    pub limits: ShareLimits,
    pub gpu_memory_mb: Option<u64>,
    /// Drop capabilities / read-only rootfs when true.
    #[serde(default = "default_harden")]
    pub harden: bool,
}

fn default_harden() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub job_id: String,
    pub state: JobState,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
    pub container_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct JobHandle {
    pub job_id: String,
    pub container_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogStreamKind {
    Stdout,
    Stderr,
    System,
}

#[derive(Debug, Clone)]
pub struct LogEvent {
    pub stream: LogStreamKind,
    pub line: String,
}
