use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpumesh_common::{GpuMeshError, JobState, Result, DEFAULT_IMAGE};
use gpumesh_gpu::GpuMonitor;
use gpumesh_network::PeerConnection;
use gpumesh_protocol::{
    new_transfer_id, short_job_id, FileChunk, FileDirection, FileOffer, LogStream, Message,
    RunJobRequest,
};
use gpumesh_runtime::{DockerRuntime, JobRequest, LogEvent, LogStreamKind};
use gpumesh_storage::{package_workdir, unpack_archive, StateStore};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::MeshNode;

pub async fn run_remote_job(
    node: &MeshNode,
    peer: &str,
    image: Option<String>,
    command: Vec<String>,
    workdir: PathBuf,
    env: Vec<(String, String)>,
) -> Result<i32> {
    let cfg = node.config.read().await.clone();
    let image = image.unwrap_or(cfg.default_image.clone());
    let mut conn = node.connect_peer(peer).await?;

    // Package & upload workload
    let job_id = short_job_id();
    let transfer_id = new_transfer_id();
    let pack_path = StateStore::ensure_job_dir(&job_id)?.join("upload.gpk");
    let manifest = package_workdir(&workdir, &pack_path)?;
    info!(
        "packaged {} bytes ({} files) for job {job_id}",
        manifest.total_bytes,
        manifest.files.len()
    );

    upload_file(
        &conn,
        &transfer_id,
        &pack_path,
        "workload.gpk",
        FileDirection::Upload,
    )
    .await?;

    let req = RunJobRequest {
        job_id: job_id.clone(),
        image,
        command,
        env,
        workdir: "/workspace".into(),
        transfer_id: Some(transfer_id),
        gpu_memory_mb: None,
    };
    conn.send(Message::RunJob(req)).await?;

    let mut exit_code = 1;
    loop {
        match conn.recv().await? {
            Some(Message::JobAccepted { job_id: id }) => {
                println!("Job: {id}");
            }
            Some(Message::JobRejected { reason }) => {
                return Err(GpuMeshError::Job(reason));
            }
            Some(Message::JobLog {
                stream,
                line,
                ..
            }) => match stream {
                LogStream::Stderr => eprintln!("{line}"),
                LogStream::System => eprintln!("[{line}]"),
                LogStream::Stdout => println!("{line}"),
            },
            Some(Message::JobStatus {
                state,
                exit_code: code,
                error,
                gpu_util,
                vram_used_mb,
                vram_total_mb,
                ..
            }) => {
                if let (Some(u), Some(used), Some(total)) =
                    (gpu_util, vram_used_mb, vram_total_mb)
                {
                    eprintln!("GPU util: {u}%  VRAM: {used} / {total} MB");
                }
                match state {
                    JobState::Succeeded | JobState::Failed | JobState::Cancelled => {
                        if let Some(e) = error {
                            eprintln!("error: {e}");
                        }
                        exit_code = code.unwrap_or(1);
                        break;
                    }
                    _ => {}
                }
            }
            Some(Message::Error { message }) => {
                return Err(GpuMeshError::Job(message));
            }
            None => break,
            _ => {}
        }
    }
    conn.close();
    Ok(exit_code)
}

pub async fn transfer_file_to_peer(
    node: &MeshNode,
    peer: &str,
    local: &Path,
    remote_path: &str,
) -> Result<()> {
    let conn = node.connect_peer(peer).await?;
    let transfer_id = new_transfer_id();
    upload_file(
        &conn,
        &transfer_id,
        local,
        remote_path,
        FileDirection::Upload,
    )
    .await?;
    conn.close();
    Ok(())
}

pub async fn transfer_file_from_peer(
    node: &MeshNode,
    peer: &str,
    remote_path: &str,
    local: &Path,
) -> Result<()> {
    let conn = node.connect_peer(peer).await?;
    let transfer_id = new_transfer_id();
    conn.send(Message::FileOffer(FileOffer {
        transfer_id: transfer_id.clone(),
        path: remote_path.to_string(),
        size: 0,
        sha256_hex: String::new(),
        direction: FileDirection::Download,
    }))
    .await?;

    let mut file_data = Vec::new();
    let mut expected_size = 0u64;
    loop {
        match conn.recv().await? {
            Some(Message::FileOffer(offer)) => {
                expected_size = offer.size;
            }
            Some(Message::FileChunk(chunk)) => {
                file_data.extend_from_slice(&chunk.data);
                if chunk.eof {
                    break;
                }
            }
            Some(Message::FileAck { ok: false, error, .. }) => {
                return Err(GpuMeshError::Storage(
                    error.unwrap_or_else(|| "download failed".into()),
                ));
            }
            Some(Message::Error { message }) => {
                return Err(GpuMeshError::Storage(message));
            }
            None => break,
            _ => {}
        }
    }
    if expected_size > 0 && file_data.len() as u64 != expected_size {
        warn!(
            "size mismatch: expected {expected_size}, got {}",
            file_data.len()
        );
    }
    if let Some(parent) = local.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(local, &file_data)?;
    Ok(())
}

async fn upload_file(
    conn: &PeerConnection,
    transfer_id: &str,
    path: &Path,
    remote_path: &str,
    direction: FileDirection,
) -> Result<()> {
    let data = std::fs::read(path)?;
    let sha = {
        let mut h = Sha256::new();
        h.update(&data);
        hex::encode(h.finalize())
    };
    conn.send(Message::FileOffer(FileOffer {
        transfer_id: transfer_id.to_string(),
        path: remote_path.to_string(),
        size: data.len() as u64,
        sha256_hex: sha,
        direction,
    }))
    .await?;

    const CHUNK: usize = 256 * 1024;
    let mut offset = 0u64;
    for chunk in data.chunks(CHUNK) {
        let eof = offset + chunk.len() as u64 >= data.len() as u64;
        conn.send(Message::FileChunk(FileChunk {
            transfer_id: transfer_id.to_string(),
            offset,
            data: chunk.to_vec(),
            eof,
        }))
        .await?;
        offset += chunk.len() as u64;
    }
    if data.is_empty() {
        conn.send(Message::FileChunk(FileChunk {
            transfer_id: transfer_id.to_string(),
            offset: 0,
            data: Vec::new(),
            eof: true,
        }))
        .await?;
    }

    match conn.recv().await? {
        Some(Message::FileAck { ok, error, .. }) => {
            if ok {
                Ok(())
            } else {
                Err(GpuMeshError::Storage(
                    error.unwrap_or_else(|| "upload rejected".into()),
                ))
            }
        }
        other => Err(GpuMeshError::Protocol(format!(
            "expected FileAck, got {other:?}"
        ))),
    }
}

/// Provider-side session loop after handshake + auth.
pub async fn serve_peer_session(node: &MeshNode, conn: PeerConnection) -> Result<()> {
    let conn = Arc::new(conn);
    let mut uploads: std::collections::HashMap<String, (String, Vec<u8>)> =
        std::collections::HashMap::new();

    loop {
        let msg = match conn.recv().await? {
            Some(m) => m,
            None => break,
        };
        match msg {
            Message::Ping { nonce } => {
                conn.send(Message::Pong { nonce }).await?;
            }
            Message::PeerInfoRequest => {
                conn.send(Message::PeerInfo(node.peer_info_msg().await))
                    .await?;
            }
            Message::FileOffer(offer) => match offer.direction {
                FileDirection::Upload => {
                    uploads.insert(offer.transfer_id.clone(), (offer.path.clone(), Vec::new()));
                }
                FileDirection::Download => {
                    serve_download(&conn, &offer.path, &offer.transfer_id).await?;
                }
            },
            Message::FileChunk(chunk) => {
                if let Some((_path, buf)) = uploads.get_mut(&chunk.transfer_id) {
                    if chunk.offset as usize != buf.len() {
                        // allow sparse-ish append for sequential MVP
                    }
                    buf.extend_from_slice(&chunk.data);
                    if chunk.eof {
                        let (path, data) = uploads.remove(&chunk.transfer_id).unwrap();
                        let dest = if path == "workload.gpk" {
                            let dir = StateStore::ensure_job_dir(&chunk.transfer_id)?;
                            dir.join("workload.gpk")
                        } else {
                            let base = gpumesh_common::work_dir().join("incoming");
                            std::fs::create_dir_all(&base)?;
                            let clean = path.trim_start_matches('/').replace("..", "_");
                            base.join(clean)
                        };
                        if let Some(parent) = dest.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::write(&dest, &data)?;
                        conn.send(Message::FileAck {
                            transfer_id: chunk.transfer_id,
                            ok: true,
                            error: None,
                        })
                        .await?;
                    }
                }
            }
            Message::RunJob(req) => {
                if !*node.sharing.read().await
                    && !node.config.read().await.sharing_enabled
                {
                    conn.send(Message::JobRejected {
                        reason: "provider is not sharing".into(),
                    })
                    .await?;
                    continue;
                }
                let conn2 = conn.clone();
                let runtime = node.runtime.clone();
                let limits = node.share_limits().await;
                tokio::spawn(async move {
                    if let Err(e) =
                        execute_remote_job(runtime, conn2, req, limits).await
                    {
                        warn!("job execution error: {e}");
                    }
                });
            }
            Message::CancelJob { job_id } => {
                let name = format!("gpumesh-{job_id}");
                let ok = node.runtime.cancel(&name).await.is_ok();
                conn.send(Message::CancelAck { job_id, ok }).await?;
            }
            Message::Error { message } => {
                warn!("peer error: {message}");
                break;
            }
            _ => {}
        }
    }
    Ok(())
}

async fn serve_download(conn: &PeerConnection, path: &str, transfer_id: &str) -> Result<()> {
    let base = gpumesh_common::work_dir().join("incoming");
    let clean = path.trim_start_matches('/').replace("..", "_");
    let full = base.join(&clean);
    if !full.exists() {
        // also try absolute-looking under work dir
        let alt = gpumesh_common::work_dir().join(&clean);
        if !alt.exists() {
            conn.send(Message::FileAck {
                transfer_id: transfer_id.to_string(),
                ok: false,
                error: Some(format!("file not found: {path}")),
            })
            .await?;
            return Ok(());
        }
        return send_file_chunks(conn, transfer_id, path, &alt).await;
    }
    send_file_chunks(conn, transfer_id, path, &full).await
}

async fn send_file_chunks(
    conn: &PeerConnection,
    transfer_id: &str,
    path: &str,
    full: &Path,
) -> Result<()> {
    let data = std::fs::read(full)?;
    let sha = {
        let mut h = Sha256::new();
        h.update(&data);
        hex::encode(h.finalize())
    };
    conn.send(Message::FileOffer(FileOffer {
        transfer_id: transfer_id.to_string(),
        path: path.to_string(),
        size: data.len() as u64,
        sha256_hex: sha,
        direction: FileDirection::Download,
    }))
    .await?;
    const CHUNK: usize = 256 * 1024;
    let mut offset = 0u64;
    for chunk in data.chunks(CHUNK) {
        let eof = offset + chunk.len() as u64 >= data.len() as u64;
        conn.send(Message::FileChunk(FileChunk {
            transfer_id: transfer_id.to_string(),
            offset,
            data: chunk.to_vec(),
            eof,
        }))
        .await?;
        offset += chunk.len() as u64;
    }
    if data.is_empty() {
        conn.send(Message::FileChunk(FileChunk {
            transfer_id: transfer_id.to_string(),
            offset: 0,
            data: Vec::new(),
            eof: true,
        }))
        .await?;
    }
    Ok(())
}

async fn execute_remote_job(
    runtime: Arc<DockerRuntime>,
    conn: Arc<PeerConnection>,
    req: RunJobRequest,
    limits: gpumesh_common::ShareLimits,
) -> Result<()> {
    conn.send(Message::JobAccepted {
        job_id: req.job_id.clone(),
    })
    .await?;

    let job_dir = StateStore::ensure_job_dir(&req.job_id)?;
    let work = job_dir.join("workspace");
    std::fs::create_dir_all(&work)?;

    if let Some(tid) = &req.transfer_id {
        let pack = StateStore::job_dir(tid).join("workload.gpk");
        let pack2 = job_dir.join("workload.gpk");
        let src = if pack.exists() {
            pack
        } else if pack2.exists() {
            pack2
        } else {
            // transfer_id used as job folder in upload path
            StateStore::job_dir(tid).join("workload.gpk")
        };
        if src.exists() {
            unpack_archive(&src, &work)?;
        } else {
            warn!("workload package not found for transfer {tid}");
        }
    }

    let (tx, mut rx) = mpsc::channel::<LogEvent>(256);
    let conn_logs = conn.clone();
    let job_id_logs = req.job_id.clone();
    tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            let stream = match ev.stream {
                LogStreamKind::Stdout => LogStream::Stdout,
                LogStreamKind::Stderr => LogStream::Stderr,
                LogStreamKind::System => LogStream::System,
            };
            let _ = conn_logs
                .send(Message::JobLog {
                    job_id: job_id_logs.clone(),
                    stream,
                    line: ev.line,
                })
                .await;
        }
    });

    let gpus = GpuMonitor::detect().unwrap_or_default();
    let _ = conn
        .send(Message::JobStatus {
            job_id: req.job_id.clone(),
            state: JobState::Running,
            exit_code: None,
            error: None,
            gpu_util: gpus.first().and_then(|g| g.utilization_gpu),
            vram_used_mb: gpus.first().map(|g| g.vram_used_mb),
            vram_total_mb: gpus.first().map(|g| g.vram_total_mb),
        })
        .await;

    let image = if req.image.is_empty() {
        DEFAULT_IMAGE.to_string()
    } else {
        req.image.clone()
    };

    let job_req = JobRequest {
        job_id: req.job_id.clone(),
        image,
        command: req.command,
        env: req.env,
        host_workdir: work,
        container_workdir: req.workdir,
        limits,
        gpu_memory_mb: req.gpu_memory_mb,
    };

    let (_handle, result) = runtime.run_job(job_req, tx).await?;
    conn.send(Message::JobStatus {
        job_id: result.job_id,
        state: result.state,
        exit_code: result.exit_code,
        error: result.error,
        gpu_util: None,
        vram_used_mb: None,
        vram_total_mb: None,
    })
    .await?;
    Ok(())
}

/// Local Phase 1 run through the same Docker runtime.
pub async fn run_local_job(
    node: &MeshNode,
    image: Option<String>,
    command: Vec<String>,
    workdir: PathBuf,
    env: Vec<(String, String)>,
) -> Result<i32> {
    DockerRuntime::ensure_docker().await?;
    let cfg = node.config.read().await.clone();
    let image = image.unwrap_or(cfg.default_image);
    let job_id = short_job_id();
    println!("Job: {job_id}");
    let limits = node.share_limits().await;
    let (tx, mut rx) = mpsc::channel::<LogEvent>(256);
    tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            match ev.stream {
                LogStreamKind::Stderr => eprintln!("{}", ev.line),
                LogStreamKind::System => eprintln!("[{}]", ev.line),
                LogStreamKind::Stdout => println!("{}", ev.line),
            }
        }
    });
    let req = JobRequest {
        job_id,
        image,
        command,
        env,
        host_workdir: workdir,
        container_workdir: "/workspace".into(),
        limits,
        gpu_memory_mb: None,
    };
    let (_h, result) = node.runtime.run_job(req, tx).await?;
    Ok(result.exit_code.unwrap_or(1))
}
