//! Job history records (Phase 4).

use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use gpumesh_common::{jobs_dir, GpuMeshError, JobState, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRecord {
    pub job_id: String,
    pub peer: Option<String>,
    pub image: String,
    pub command: Vec<String>,
    pub state: JobState,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub attempts: u32,
    pub log_path: Option<PathBuf>,
}

impl JobRecord {
    pub fn new(job_id: String, peer: Option<String>, image: String, command: Vec<String>) -> Self {
        Self {
            job_id,
            peer,
            image,
            command,
            state: JobState::Pending,
            exit_code: None,
            error: None,
            created_at: Utc::now(),
            finished_at: None,
            attempts: 1,
            log_path: None,
        }
    }

    pub fn path(job_id: &str) -> PathBuf {
        jobs_dir().join(job_id).join("meta.json")
    }

    pub fn save(&self) -> Result<()> {
        let dir = jobs_dir().join(&self.job_id);
        fs::create_dir_all(&dir)?;
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| GpuMeshError::Storage(e.to_string()))?;
        fs::write(Self::path(&self.job_id), data)?;
        Ok(())
    }

    pub fn load(job_id: &str) -> Result<Self> {
        let data = fs::read_to_string(Self::path(job_id))?;
        serde_json::from_str(&data).map_err(|e| GpuMeshError::Storage(e.to_string()))
    }

    pub fn list() -> Result<Vec<Self>> {
        let dir = jobs_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().to_string();
            if let Ok(rec) = Self::load(&id) {
                out.push(rec);
            }
        }
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(out)
    }

    pub fn append_log(job_id: &str, line: &str) -> Result<()> {
        let dir = jobs_dir().join(job_id);
        fs::create_dir_all(&dir)?;
        let path = dir.join("job.log");
        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        writeln!(f, "{line}")?;
        Ok(())
    }

    pub fn read_log(job_id: &str) -> Result<String> {
        let path = jobs_dir().join(job_id).join("job.log");
        if !path.exists() {
            return Ok(String::new());
        }
        Ok(fs::read_to_string(path)?)
    }
}
