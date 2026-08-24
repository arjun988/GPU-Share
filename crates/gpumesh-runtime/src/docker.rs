use std::process::Stdio;
use std::sync::Arc;

use gpumesh_common::{GpuMeshError, JobState, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, Mutex};
use tracing::{info, warn};

use crate::job::{JobHandle, JobRequest, JobResult, LogEvent, LogStreamKind};

pub struct DockerRuntime {
    active: Arc<Mutex<Vec<String>>>,
}

impl Default for DockerRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl DockerRuntime {
    pub fn new() -> Self {
        Self {
            active: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn active_count(&self) -> usize {
        self.active.lock().await.len()
    }

    pub async fn ensure_docker() -> Result<()> {
        let status = Command::new("docker")
            .arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map_err(|e| GpuMeshError::Runtime(format!("docker not available: {e}")))?;
        if !status.success() {
            return Err(GpuMeshError::Runtime("docker version failed".into()));
        }
        Ok(())
    }

    pub async fn run_job(
        &self,
        req: JobRequest,
        log_tx: mpsc::Sender<LogEvent>,
    ) -> Result<(JobHandle, JobResult)> {
        {
            let active = self.active.lock().await;
            if active.len() as u32 >= req.limits.max_concurrent_jobs {
                return Err(GpuMeshError::Runtime(format!(
                    "max concurrent jobs reached ({})",
                    req.limits.max_concurrent_jobs
                )));
            }
        }

        let container_name = format!("gpumesh-{}", req.job_id);
        let mut args = vec![
            "run".into(),
            "--rm".into(),
            "--name".into(),
            container_name.clone(),
            "--gpus".into(),
            "all".into(),
            "-v".into(),
            format!(
                "{}:{}",
                req.host_workdir.to_string_lossy(),
                req.container_workdir
            ),
            "-w".into(),
            req.container_workdir.clone(),
        ];

        if let Some(cpus) = req.limits.max_cpu_cores {
            args.push("--cpus".into());
            args.push(cpus.to_string());
        }
        if let Some(ram) = req.limits.max_ram_mb {
            args.push("--memory".into());
            args.push(format!("{ram}m"));
        }
        // Network: restrict by default to bridge (not host)
        args.push("--network".into());
        args.push("bridge".into());

        for (k, v) in &req.env {
            args.push("-e".into());
            args.push(format!("{k}={v}"));
        }

        args.push(req.image.clone());
        args.extend(req.command.clone());

        let _ = log_tx
            .send(LogEvent {
                stream: LogStreamKind::System,
                line: format!("starting container {container_name}"),
            })
            .await;

        self.active.lock().await.push(container_name.clone());

        let mut child = Command::new("docker")
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                GpuMeshError::Runtime(format!("failed to spawn docker: {e}"))
            })?;

        let handle = JobHandle {
            job_id: req.job_id.clone(),
            container_name: container_name.clone(),
        };

        let result = self
            .wait_with_logs(&mut child, &req, log_tx.clone())
            .await;

        self.active
            .lock()
            .await
            .retain(|n| n != &container_name);

        // Best-effort cleanup if still running
        let _ = Command::new("docker")
            .args(["rm", "-f", &container_name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        match result {
            Ok(exit_code) => {
                let state = if exit_code == 0 {
                    JobState::Succeeded
                } else {
                    JobState::Failed
                };
                Ok((
                    handle,
                    JobResult {
                        job_id: req.job_id,
                        state,
                        exit_code: Some(exit_code),
                        error: None,
                        container_id: Some(container_name),
                    },
                ))
            }
            Err(e) => Ok((
                handle,
                JobResult {
                    job_id: req.job_id,
                    state: JobState::Failed,
                    exit_code: None,
                    error: Some(e.to_string()),
                    container_id: Some(container_name),
                },
            )),
        }
    }

    async fn wait_with_logs(
        &self,
        child: &mut Child,
        req: &JobRequest,
        log_tx: mpsc::Sender<LogEvent>,
    ) -> Result<i32> {
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let tx_out = log_tx.clone();
        let out_task = tokio::spawn(async move {
            if let Some(out) = stdout {
                let mut lines = BufReader::new(out).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = tx_out
                        .send(LogEvent {
                            stream: LogStreamKind::Stdout,
                            line,
                        })
                        .await;
                }
            }
        });

        let tx_err = log_tx.clone();
        let err_task = tokio::spawn(async move {
            if let Some(err) = stderr {
                let mut lines = BufReader::new(err).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = tx_err
                        .send(LogEvent {
                            stream: LogStreamKind::Stderr,
                            line,
                        })
                        .await;
                }
            }
        });

        let status = if let Some(secs) = req.limits.max_runtime_secs {
            match tokio::time::timeout(std::time::Duration::from_secs(secs), child.wait()).await {
                Ok(r) => r.map_err(|e| GpuMeshError::Runtime(e.to_string()))?,
                Err(_) => {
                    warn!("job {} timed out after {}s", req.job_id, secs);
                    let _ = child.kill().await;
                    let _ = log_tx
                        .send(LogEvent {
                            stream: LogStreamKind::System,
                            line: format!("job timed out after {secs}s"),
                        })
                        .await;
                    return Err(GpuMeshError::Runtime("job timed out".into()));
                }
            }
        } else {
            child
                .wait()
                .await
                .map_err(|e| GpuMeshError::Runtime(e.to_string()))?
        };

        let _ = out_task.await;
        let _ = err_task.await;
        Ok(status.code().unwrap_or(1))
    }

    pub async fn cancel(&self, container_name: &str) -> Result<()> {
        info!("cancelling container {container_name}");
        let status = Command::new("docker")
            .args(["rm", "-f", container_name])
            .status()
            .await
            .map_err(|e| GpuMeshError::Runtime(e.to_string()))?;
        self.active
            .lock()
            .await
            .retain(|n| n != container_name);
        if status.success() {
            Ok(())
        } else {
            Err(GpuMeshError::Runtime("failed to cancel container".into()))
        }
    }
}
