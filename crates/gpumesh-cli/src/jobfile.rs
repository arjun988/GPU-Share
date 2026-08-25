//! YAML job definitions (Phase 4).

use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct JobFile {
    pub name: Option<String>,
    pub peer: Option<String>,
    pub group: Option<String>,
    pub gpu_memory: Option<String>,
    pub image: Option<String>,
    #[serde(default = "default_workdir")]
    pub workdir: String,
    #[serde(default)]
    pub env: Vec<EnvEntry>,
    pub command: Vec<String>,
    #[serde(default)]
    pub retries: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum EnvEntry {
    Map(std::collections::HashMap<String, String>),
    Pair { key: String, value: String },
    Str(String),
}

fn default_workdir() -> String {
    ".".into()
}

impl JobFile {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read job file {}", path.display()))?;
        let job: Self = if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e == "toml")
        {
            toml::from_str(&text)?
        } else {
            serde_yaml::from_str(&text)?
        };
        if job.command.is_empty() {
            bail!("job file must include a non-empty `command` list");
        }
        Ok(job)
    }

    pub fn env_pairs(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for e in &self.env {
            match e {
                EnvEntry::Map(m) => {
                    for (k, v) in m {
                        out.push((k.clone(), v.clone()));
                    }
                }
                EnvEntry::Pair { key, value } => out.push((key.clone(), value.clone())),
                EnvEntry::Str(s) => {
                    if let Some((k, v)) = s.split_once('=') {
                        out.push((k.to_string(), v.to_string()));
                    }
                }
            }
        }
        out
    }
}
