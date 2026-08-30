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

#[allow(clippy::too_many_arguments)]
pub async fn run_remote_job(
    node: &MeshNode,
    peer: &str,
    image: Option<String>,
    command: Vec<String>,
    workdir: PathBuf,
    env: Vec<(String, String)>,
    job_id: Option<String>,
    gpu_memory_mb: Option<u64>,
    pull_artifacts: Option<PathBuf>,
) -> Result<i32> {
    let cfg = node.config.read().await.clone();
    let image = image.unwrap_or(cfg.default_image.clone());
    let conn = node.connect_peer(peer).await?;

    // Package & upload workload
    let job_id = job_id.unwrap_or_else(short_job_id);
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
        gpu_memory_mb,
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
            Some(Message::JobLog { stream, line, .. }) => {
                let stored = match stream {
                    LogStream::Stderr => {
                        eprintln!("{line}");
                        format!("[stderr] {line}")
                    }
                    LogStream::System => {
                        eprintln!("[{line}]");
                        format!("[system] {line}")
                    }
                    LogStream::Stdout => {
                        println!("{line}");
                        line
                    }
                };
                if let Err(e) = gpumesh_storage::JobRecord::append_log(&job_id, &stored) {
                    warn!("failed to capture job log for {job_id}: {e}");
                }
            }
            Some(Message::JobStatus {
                state,
                exit_code: code,
                error,
                gpu_util,
                vram_used_mb,
                vram_total_mb,
                ..
            }) => {
                if let (Some(u), Some(used), Some(total)) = (gpu_util, vram_used_mb, vram_total_mb)
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

    if let Some(dest) = pull_artifacts {
        match pull_job_outputs(node, peer, &job_id, &dest).await {
            Ok(()) => info!("pulled job {job_id} outputs to {}", dest.display()),
            Err(e) if exit_code == 0 => return Err(e),
            Err(e) => warn!("could not pull job outputs: {e}"),
        }
    }

    Ok(exit_code)
}

/// Download a completed job's `outputs.gpk` from the peer and unpack it into `dest`.
pub async fn pull_job_outputs(
    node: &MeshNode,
    peer: &str,
    job_id: &str,
    dest: &Path,
) -> Result<()> {
    if !is_safe_job_id(job_id) {
        return Err(GpuMeshError::Storage("invalid job id".into()));
    }
    std::fs::create_dir_all(dest)?;
    let pack = dest.join(".gpumesh-outputs.gpk");
    transfer_file_from_peer(
        node,
        peer,
        &format!("jobs/{job_id}/outputs.gpk"),
        &pack,
    )
    .await?;
    unpack_archive(&pack, dest)?;
    let _ = std::fs::remove_file(&pack);
    Ok(())
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
            Some(Message::FileAck {
                ok: false, error, ..
            }) => {
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

    // Phase 4: wait for resume handshake before sending chunks.
    let start = match conn.recv().await? {
        Some(Message::FileAck {
            ok: true,
            resume_from,
            ..
        }) => {
            let start = resume_from.unwrap_or(0).min(data.len() as u64);
            if start > 0 {
                tracing::info!("resuming upload of {remote_path} from byte {start}");
            }
            start
        }
        Some(Message::FileAck {
            ok: false, error, ..
        }) => {
            return Err(GpuMeshError::Storage(
                error.unwrap_or_else(|| "upload rejected".into()),
            ));
        }
        other => {
            return Err(GpuMeshError::Protocol(format!(
                "expected FileAck after offer, got {other:?}"
            )));
        }
    };

    const CHUNK: usize = 256 * 1024;
    let mut offset = start;
    let slice = &data[start as usize..];
    if slice.is_empty() {
        conn.send(Message::FileChunk(FileChunk {
            transfer_id: transfer_id.to_string(),
            offset,
            data: Vec::new(),
            eof: true,
        }))
        .await?;
    } else {
        for chunk in slice.chunks(CHUNK) {
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
    let mut uploads: std::collections::HashMap<String, (FileOffer, Vec<u8>)> =
        std::collections::HashMap::new();
    let mut cuda_sessions: std::collections::HashMap<String, crate::cuda_remote::CudaSession> =
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
            Message::DesktopRequest { prefer } => {
                crate::desktop::handle_desktop_request(node, conn.clone(), prefer).await?;
            }
            Message::GpuRemoteOpen { api, client_ver } => {
                crate::cuda_remote::handle_gpu_remote_open(
                    node,
                    &conn,
                    api,
                    client_ver,
                    &mut cuda_sessions,
                )
                .await?;
            }
            Message::CudaOp {
                session_id,
                op_id,
                op,
            } => {
                crate::cuda_remote::handle_cuda_op(
                    &conn,
                    &mut cuda_sessions,
                    session_id,
                    op_id,
                    op,
                )
                .await?;
            }
            Message::GpuRemoteClose { session_id } => {
                crate::cuda_remote::handle_gpu_remote_close(&mut cuda_sessions, &session_id);
            }
            Message::FileOffer(offer) => match offer.direction {
                FileDirection::Upload => {
                    let dest = upload_dest(&offer.path, &offer.transfer_id)?;
                    let partial = PathBuf::from(format!("{}.partial", dest.display()));
                    let mut resume_from = 0u64;
                    let mut buf = Vec::new();
                    if partial.exists() {
                        if let Ok(existing) = std::fs::read(&partial) {
                            if (existing.len() as u64) < offer.size {
                                resume_from = existing.len() as u64;
                                buf = existing;
                            }
                        }
                    }
                    uploads.insert(offer.transfer_id.clone(), (offer.clone(), buf));
                    conn.send(Message::FileAck {
                        transfer_id: offer.transfer_id,
                        ok: true,
                        error: None,
                        resume_from: Some(resume_from),
                    })
                    .await?;
                }
                FileDirection::Download => {
                    serve_download(&conn, &offer.path, &offer.transfer_id).await?;
                }
            },
            Message::FileChunk(chunk) => {
                let tid = chunk.transfer_id.clone();
                let (path_clone, snapshot, eof) = {
                    let Some((offer, buf)) = uploads.get_mut(&tid) else {
                        continue;
                    };
                    if chunk.offset as usize > buf.len() {
                        buf.resize(chunk.offset as usize, 0);
                    }
                    if chunk.offset as usize == buf.len() {
                        buf.extend_from_slice(&chunk.data);
                    } else if chunk.offset as usize + chunk.data.len() <= buf.len() {
                        let start = chunk.offset as usize;
                        buf[start..start + chunk.data.len()].copy_from_slice(&chunk.data);
                    } else {
                        buf.truncate(chunk.offset as usize);
                        buf.extend_from_slice(&chunk.data);
                    }
                    (offer.path.clone(), buf.clone(), chunk.eof)
                };
                if let Ok(dest) = upload_dest(&path_clone, &tid) {
                    if let Some(parent) = dest.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let partial = PathBuf::from(format!("{}.partial", dest.display()));
                    let _ = std::fs::write(&partial, &snapshot);
                }
                if eof {
                    let (offer, data) = uploads.remove(&tid).unwrap();
                    let dest = upload_dest(&offer.path, &tid)?;
                    if let Some(parent) = dest.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&dest, &data)?;
                    let actual_sha = {
                        let mut h = Sha256::new();
                        h.update(&data);
                        hex::encode(h.finalize())
                    };
                    let hash_ok = offer.sha256_hex.is_empty()
                        || actual_sha.eq_ignore_ascii_case(&offer.sha256_hex);
                    if !hash_ok {
                        let _ = std::fs::remove_file(&dest);
                    }
                    let _ = std::fs::remove_file(format!("{}.partial", dest.display()));
                    conn.send(Message::FileAck {
                        transfer_id: tid,
                        ok: hash_ok,
                        error: (!hash_ok).then(|| {
                            format!(
                                "sha256 mismatch: expected {}, got {actual_sha}",
                                offer.sha256_hex
                            )
                        }),
                        resume_from: None,
                    })
                    .await?;
                }
            }
            Message::RunJob(req) => {
                if !*node.sharing.read().await && !node.config.read().await.sharing_enabled {
                    conn.send(Message::JobRejected {
                        reason: "provider is not sharing".into(),
                    })
                    .await?;
                    continue;
                }
                let cfg = StateStore::load_config()?;
                let image = if req.image.is_empty() {
                    DEFAULT_IMAGE
                } else {
                    &req.image
                };
                if !image_allowed(image, &cfg.allowed_images) {
                    conn.send(Message::JobRejected {
                        reason: format!("container image is not allowed: {image}"),
                    })
                    .await?;
                    continue;
                }
                let gpus = GpuMonitor::detect().unwrap_or_default();
                let gpu = gpus.first();
                if let (Some(max), Some(util)) =
                    (cfg.max_gpu_utilization, gpu.and_then(|g| g.utilization_gpu))
                {
                    if util >= max as u32 {
                        conn.send(Message::JobRejected {
                            reason: format!(
                                "GPU utilization {util}% is at or above configured maximum {max}%"
                            ),
                        })
                        .await?;
                        continue;
                    }
                }
                let need = match (cfg.max_vram_mb, req.gpu_memory_mb) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (Some(a), None) | (None, Some(a)) => Some(a),
                    (None, None) => None,
                };
                if let Some(need) = need {
                    let free = gpu.map(|g| g.vram_free_mb).unwrap_or(0);
                    if free < need {
                        conn.send(Message::JobRejected {
                            reason: format!(
                                "insufficient free VRAM: need {need} MB, available {free} MB"
                            ),
                        })
                        .await?;
                        continue;
                    }
                }
                let conn2 = conn.clone();
                let runtime = node.runtime.clone();
                let limits = node.share_limits().await;
                let harden = cfg.docker_harden;
                tokio::spawn(async move {
                    if let Err(e) = execute_remote_job(runtime, conn2, req, limits, harden).await {
                        warn!("job execution error: {e}");
                    }
                });
            }
            Message::CancelJob { job_id } => {
                let name = format!("gpumesh-{job_id}");
                let ok = node.runtime.cancel(&name).await.is_ok();
                conn.send(Message::CancelAck { job_id, ok }).await?;
            }
            Message::GroupJoinNotify {
                group_id,
                group_name,
                member_node_id,
                member_name,
                public_key_hex,
                signature,
            } => {
                let expected_node_id = gpumesh_security::node_id_from_public_hex(&public_key_hex)?;
                if expected_node_id != member_node_id
                    || conn.peer_node_id.as_deref() != Some(member_node_id.as_str())
                {
                    return Err(GpuMeshError::NotAuthorized(
                        "group join identity mismatch".into(),
                    ));
                }
                let canonical = serde_json::to_vec(&(
                    &group_id,
                    &group_name,
                    &member_node_id,
                    &member_name,
                    &public_key_hex,
                ))
                .map_err(|e| GpuMeshError::Protocol(e.to_string()))?;
                gpumesh_security::verify_signature(&public_key_hex, &canonical, &signature)?;
                let mut groups = gpumesh_storage::GroupStore::load()?;
                let group = groups
                    .get(&group_id)
                    .ok_or_else(|| GpuMeshError::Other("group not found".into()))?;
                if group.owner_node_id != node.identity.node_id || group.name != group_name {
                    return Err(GpuMeshError::NotAuthorized(
                        "only the local group owner can accept joins".into(),
                    ));
                }
                groups.add_member(
                    &group_id,
                    &member_node_id,
                    &member_name,
                    gpumesh_storage::GroupRole::Member,
                )?;
                info!("added {member_name} to group {group_name}");
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

fn upload_dest(path: &str, transfer_id: &str) -> Result<PathBuf> {
    if path == "workload.gpk" {
        let transfer_path = Path::new(transfer_id);
        if transfer_id.is_empty()
            || transfer_path.is_absolute()
            || transfer_path.components().count() != 1
            || !matches!(
                transfer_path.components().next(),
                Some(std::path::Component::Normal(_))
            )
        {
            return Err(GpuMeshError::Storage("unsafe transfer id rejected".into()));
        }
        let dir = StateStore::ensure_job_dir(transfer_id)?;
        Ok(dir.join("workload.gpk"))
    } else {
        let base = gpumesh_common::work_dir().join("incoming");
        std::fs::create_dir_all(&base)?;
        let relative = Path::new(path);
        if relative.is_absolute()
            || relative.components().any(|c| {
                matches!(
                    c,
                    std::path::Component::ParentDir | std::path::Component::Prefix(_)
                )
            })
        {
            return Err(GpuMeshError::Storage(format!(
                "unsafe upload path rejected: {path}"
            )));
        }
        let base = std::fs::canonicalize(&base)?;
        let dest = base.join(relative);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
            let canonical_parent = std::fs::canonicalize(parent)?;
            if !canonical_parent.starts_with(&base) {
                return Err(GpuMeshError::Storage(format!(
                    "upload path escapes incoming directory: {path}"
                )));
            }
        }
        Ok(dest)
    }
}

fn image_allowed(image: &str, allowed: &[String]) -> bool {
    allowed.iter().any(|entry| {
        let entry = entry.trim_end_matches('/');
        image == entry
            || image
                .strip_prefix(entry)
                .is_some_and(|suffix| suffix.starts_with(':') || suffix.starts_with('/'))
    })
}

fn is_safe_job_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn resolve_download_path(path: &str) -> Result<PathBuf> {
    let clean = path.trim_start_matches(['/', '\\']).replace('\\', "/");
    if clean.is_empty() || clean.contains("..") {
        return Err(GpuMeshError::Storage(format!(
            "unsafe download path rejected: {path}"
        )));
    }
    let parts: Vec<&str> = clean.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() == 3 && parts[0] == "jobs" && parts[2] == "outputs.gpk" {
        if !is_safe_job_id(parts[1]) {
            return Err(GpuMeshError::Storage("invalid job id".into()));
        }
        return Ok(StateStore::job_dir(parts[1]).join("outputs.gpk"));
    }
    if parts
        .iter()
        .any(|p| *p == ".." || *p == "." || p.contains(':'))
    {
        return Err(GpuMeshError::Storage(format!(
            "unsafe download path rejected: {path}"
        )));
    }
    let mut full = gpumesh_common::work_dir().join("incoming");
    for part in parts {
        full.push(part);
    }
    Ok(full)
}

async fn serve_download(conn: &PeerConnection, path: &str, transfer_id: &str) -> Result<()> {
    let full = match resolve_download_path(path) {
        Ok(p) => p,
        Err(e) => {
            conn.send(Message::FileAck {
                transfer_id: transfer_id.to_string(),
                ok: false,
                error: Some(e.to_string()),
                resume_from: None,
            })
            .await?;
            return Ok(());
        }
    };
    if !full.exists() {
        // Fallback: relative path under the work directory (legacy).
        let clean = path.trim_start_matches('/').replace("..", "_");
        let alt = gpumesh_common::work_dir().join(&clean);
        if alt.exists() {
            return send_file_chunks(conn, transfer_id, path, &alt).await;
        }
        conn.send(Message::FileAck {
            transfer_id: transfer_id.to_string(),
            ok: false,
            error: Some(format!("file not found: {path}")),
            resume_from: None,
        })
        .await?;
        return Ok(());
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
    harden: bool,
) -> Result<()> {
    conn.send(Message::JobAccepted {
        job_id: req.job_id.clone(),
    })
    .await?;

    let job_dir = StateStore::ensure_job_dir(&req.job_id)?;
    let work = job_dir.join("workspace");
    std::fs::create_dir_all(&work)?;
    let work_for_pack = work.clone();

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
        harden,
    };

    let (_handle, result) = runtime.run_job(job_req, tx).await?;

    // Pack before the terminal status so the client can pull immediately after.
    let out = job_dir.join("outputs.gpk");
    match package_workdir(&work_for_pack, &out) {
        Ok(m) => info!(
            "packed job {} outputs: {} files, {} bytes",
            result.job_id, m.files.len(), m.total_bytes
        ),
        Err(e) => warn!("failed to pack job {} outputs: {e}", result.job_id),
    }

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
        harden: true,
    };
    let (_h, result) = node.runtime.run_job(req, tx).await?;
    Ok(result.exit_code.unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_id_rejects_path_injection() {
        assert!(is_safe_job_id("abc123def456"));
        assert!(!is_safe_job_id(""));
        assert!(!is_safe_job_id("../etc"));
        assert!(!is_safe_job_id("a/b"));
        assert!(!is_safe_job_id("a\\b"));
    }

    #[test]
    fn download_path_rejects_traversal() {
        assert!(resolve_download_path("../secret").is_err());
        assert!(resolve_download_path("jobs/../outputs.gpk").is_err());
        assert!(resolve_download_path("jobs/not valid/outputs.gpk").is_err());
        let ok = resolve_download_path("jobs/abc123def456/outputs.gpk").unwrap();
        assert!(ok.ends_with("outputs.gpk"));
        let incoming = resolve_download_path("apps/proj.gpk").unwrap();
        assert!(incoming.ends_with("proj.gpk"));
    }
}
