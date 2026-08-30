//! Live node API for the local dashboard: logs, metrics, pairing, connect, run.
//! Reads `~/.gpumesh` and NVML. Outbound P2P uses an ephemeral QUIC dialer so
//! it does not fight `gpumesh share` on the agent port.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use gpumesh_common::{parse_size_to_mb, GpuMeshError, JobState, DEFAULT_IMAGE};
use gpumesh_core::{run_remote_job, MeshNode};
use gpumesh_gpu::{GpuInfo, GpuMonitor};
use gpumesh_protocol::{short_job_id, Message};
use gpumesh_security::short_fingerprint;
use gpumesh_storage::{GroupStore, JobRecord, PeerStore, StateStore};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

const LOG_TAIL_BYTES: usize = 256 * 1024;

#[derive(Clone)]
pub struct LocalState {
    pub api_token: Option<Arc<String>>,
    pub running: Arc<Mutex<HashSet<String>>>,
}

pub fn router(state: LocalState) -> Router {
    Router::new()
        .route("/v1/local/status", get(status))
        .route("/v1/local/gpus", get(gpus))
        .route("/v1/local/peers", get(peers))
        .route("/v1/local/jobs", get(jobs))
        .route("/v1/local/jobs/{id}", get(job_one))
        .route("/v1/local/jobs/{id}/logs", get(job_logs))
        .route("/v1/local/logs/agent", get(agent_logs))
        .route("/v1/local/network", get(network))
        .route("/v1/local/pair-code", get(pair_code))
        .route("/v1/local/pair", post(pair))
        .route("/v1/local/allow", post(allow))
        .route("/v1/local/allow-desktop", post(allow_desktop))
        .route("/v1/local/allow-cuda", post(allow_cuda))
        .route("/v1/local/deny", post(deny))
        .route("/v1/local/connect", post(connect))
        .route("/v1/local/run", post(run_job))
        .with_state(state)
}

fn authorized(state: &LocalState, headers: &HeaderMap) -> bool {
    let Some(token) = &state.api_token else {
        return true;
    };
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == format!("Bearer {token}"))
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

type ApiError = (StatusCode, Json<ErrorBody>);

fn err(status: StatusCode, msg: impl ToString) -> ApiError {
    (
        status,
        Json(ErrorBody {
            error: msg.to_string(),
        }),
    )
}

fn map_err(e: GpuMeshError) -> ApiError {
    let status = match e {
        GpuMeshError::NotInitialized => StatusCode::PRECONDITION_FAILED,
        GpuMeshError::PeerNotFound(_) => StatusCode::NOT_FOUND,
        GpuMeshError::PeerDenied(_) | GpuMeshError::NotAuthorized(_) => StatusCode::FORBIDDEN,
        GpuMeshError::Network(_) => StatusCode::BAD_GATEWAY,
        _ => StatusCode::BAD_REQUEST,
    };
    err(status, e)
}

fn require_auth(state: &LocalState, headers: &HeaderMap) -> Result<(), ApiError> {
    if authorized(state, headers) {
        Ok(())
    } else {
        Err(err(StatusCode::UNAUTHORIZED, "missing or invalid API token"))
    }
}

fn share_pid() -> Option<u32> {
    std::fs::read_to_string(gpumesh_common::share_pid_path())
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(windows)]
    {
        let _ = pid;
        true
    }
}

#[derive(Serialize)]
struct LocalStatus {
    initialized: bool,
    node_name: String,
    node_id: String,
    node_id_short: String,
    listen_port: u16,
    sharing_enabled: bool,
    share_pid: Option<u32>,
    share_running: bool,
    home: String,
    gpus: Vec<GpuInfo>,
    peers: usize,
    jobs_running: usize,
    jobs_total: usize,
    groups: usize,
    updated_at: String,
}

async fn status(
    State(state): State<LocalState>,
    headers: HeaderMap,
) -> Result<Json<LocalStatus>, ApiError> {
    require_auth(&state, &headers)?;
    if !StateStore::is_initialized() {
        return Ok(Json(LocalStatus {
            initialized: false,
            node_name: String::new(),
            node_id: String::new(),
            node_id_short: String::new(),
            listen_port: gpumesh_common::DEFAULT_AGENT_PORT,
            sharing_enabled: false,
            share_pid: None,
            share_running: false,
            home: gpumesh_common::config_dir().display().to_string(),
            gpus: Vec::new(),
            peers: 0,
            jobs_running: 0,
            jobs_total: 0,
            groups: 0,
            updated_at: Utc::now().to_rfc3339(),
        }));
    }
    let cfg = StateStore::load_config().map_err(map_err)?;
    let identity = gpumesh_security::NodeIdentity::load_default().map_err(map_err)?;
    let gpus = GpuMonitor::detect().unwrap_or_default();
    let peers = PeerStore::load().map(|s| s.list().len()).unwrap_or(0);
    let jobs = JobRecord::list().unwrap_or_default();
    let jobs_running = jobs
        .iter()
        .filter(|j| matches!(j.state, JobState::Running | JobState::Starting | JobState::Pending | JobState::Transferring))
        .count();
    let groups = GroupStore::load().map(|s| s.list().len()).unwrap_or(0);
    let pid = share_pid();
    let share_running = pid.map(pid_alive).unwrap_or(false);
    Ok(Json(LocalStatus {
        initialized: true,
        node_name: cfg.node_name,
        node_id: identity.node_id.clone(),
        node_id_short: short_fingerprint(&identity.node_id),
        listen_port: cfg.listen_port,
        sharing_enabled: cfg.sharing_enabled,
        share_pid: pid,
        share_running,
        home: gpumesh_common::config_dir().display().to_string(),
        gpus,
        peers,
        jobs_running,
        jobs_total: jobs.len(),
        groups,
        updated_at: Utc::now().to_rfc3339(),
    }))
}

async fn gpus(
    State(state): State<LocalState>,
    headers: HeaderMap,
) -> Result<Json<Vec<GpuInfo>>, ApiError> {
    require_auth(&state, &headers)?;
    Ok(Json(GpuMonitor::detect().unwrap_or_default()))
}

#[derive(Serialize)]
struct LocalPeer {
    node_id: String,
    node_id_short: String,
    node_name: String,
    addrs: Vec<String>,
    gpu_model: Option<String>,
    vram_mb: Option<u64>,
    vram_free_mb: Option<u64>,
    utilization: Option<u32>,
    last_seen: Option<i64>,
    paired_at: i64,
    allowed: bool,
    desktop_allowed: bool,
    gpu_remote_allowed: bool,
    live_status: Option<String>,
    sharing: Option<bool>,
}

#[derive(Deserialize)]
struct PeerQuery {
    probe: Option<String>,
}

async fn peers(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Query(q): Query<PeerQuery>,
) -> Result<Json<Vec<LocalPeer>>, ApiError> {
    require_auth(&state, &headers)?;
    if !StateStore::is_initialized() {
        return Ok(Json(Vec::new()));
    }
    let store = PeerStore::load().map_err(map_err)?;
    let allow = StateStore::load_allowlist().unwrap_or_default();
    let want_probe = matches!(
        q.probe.as_deref(),
        Some("1") | Some("true") | Some("yes")
    );
    let mut live: std::collections::HashMap<String, (String, Option<bool>, Option<String>, Option<u64>, Option<u64>, Option<u32>)> =
        std::collections::HashMap::new();
    if want_probe && !store.list().is_empty() {
        if let Ok(mut node) = MeshNode::bootstrap().await {
            if node.ensure_dialer().await.is_ok() {
                let node = Arc::new(node);
                let mut handles = Vec::new();
                for p in store.list() {
                    let id = p.node_id.clone();
                    let n = node.clone();
                    handles.push(tokio::spawn(async move {
                        let status = tokio::time::timeout(Duration::from_secs(3), probe_peer(&n, &id))
                            .await
                            .ok()
                            .and_then(|r| r.ok());
                        (id, status)
                    }));
                }
                for h in handles {
                    if let Ok((id, Some(info))) = h.await {
                        live.insert(id, info);
                    }
                }
            }
        }
    }
    let out = store
        .list()
        .into_iter()
        .map(|p| {
            let probed = live.get(&p.node_id);
            LocalPeer {
                node_id: p.node_id.clone(),
                node_id_short: short_fingerprint(&p.node_id),
                node_name: p.node_name.clone(),
                addrs: p.addrs.clone(),
                gpu_model: probed.and_then(|x| x.2.clone()).or_else(|| p.gpu_model.clone()),
                vram_mb: probed.and_then(|x| x.3).or(p.vram_mb),
                vram_free_mb: probed.and_then(|x| x.4),
                utilization: probed.and_then(|x| x.5),
                last_seen: p.last_seen,
                paired_at: p.paired_at,
                allowed: allow.is_allowed(&p.node_id),
                desktop_allowed: allow.is_desktop_allowed(&p.node_id),
                gpu_remote_allowed: allow.is_gpu_remote_allowed(&p.node_id),
                live_status: probed.map(|x| x.0.clone()),
                sharing: probed.and_then(|x| x.1),
            }
        })
        .collect();
    Ok(Json(out))
}

async fn probe_peer(
    node: &MeshNode,
    peer_id: &str,
) -> Result<(String, Option<bool>, Option<String>, Option<u64>, Option<u64>, Option<u32>), GpuMeshError> {
    let conn = node.connect_peer(peer_id).await?;
    conn.send(Message::PeerInfoRequest).await?;
    let out = match conn.recv().await? {
        Some(Message::PeerInfo(info)) => (
            info.status.to_string(),
            Some(info.sharing),
            info.gpu_model,
            info.vram_total_mb,
            info.vram_free_mb,
            info.utilization,
        ),
        _ => ("UNKNOWN".into(), None, None, None, None, None),
    };
    conn.close();
    Ok(out)
}

#[derive(Serialize)]
struct LocalJob {
    job_id: String,
    peer: Option<String>,
    state: String,
    exit_code: Option<i32>,
    image: String,
    command: Vec<String>,
    error: Option<String>,
    created_at: String,
    finished_at: Option<String>,
    attempts: u32,
    has_log: bool,
}

fn job_to_json(j: &JobRecord) -> LocalJob {
    let log_path = gpumesh_common::jobs_dir().join(&j.job_id).join("job.log");
    LocalJob {
        job_id: j.job_id.clone(),
        peer: j.peer.clone(),
        state: j.state.to_string(),
        exit_code: j.exit_code,
        image: j.image.clone(),
        command: j.command.clone(),
        error: j.error.clone(),
        created_at: j.created_at.to_rfc3339(),
        finished_at: j.finished_at.map(|t| t.to_rfc3339()),
        attempts: j.attempts,
        has_log: log_path.exists(),
    }
}

async fn jobs(
    State(state): State<LocalState>,
    headers: HeaderMap,
) -> Result<Json<Vec<LocalJob>>, ApiError> {
    require_auth(&state, &headers)?;
    let mut list = JobRecord::list().unwrap_or_default();
    list.truncate(80);
    Ok(Json(list.iter().map(job_to_json).collect()))
}

async fn job_one(
    State(state): State<LocalState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<LocalJob>, ApiError> {
    require_auth(&state, &headers)?;
    let rec = JobRecord::load(&id).map_err(map_err)?;
    Ok(Json(job_to_json(&rec)))
}

#[derive(Deserialize)]
struct LogQuery {
    offset: Option<u64>,
}

#[derive(Serialize)]
struct LogChunk {
    offset: u64,
    size: u64,
    truncated: bool,
    text: String,
    path: String,
}

fn read_log_chunk(path: &Path, offset: Option<u64>) -> LogChunk {
    let data = std::fs::read(path).unwrap_or_default();
    let size = data.len() as u64;
    let (start, truncated) = if let Some(off) = offset {
        (off.min(size) as usize, false)
    } else if data.len() > LOG_TAIL_BYTES {
        (data.len() - LOG_TAIL_BYTES, true)
    } else {
        (0, false)
    };
    let text = String::from_utf8_lossy(&data[start..]).into_owned();
    LogChunk {
        offset: size,
        size,
        truncated,
        text,
        path: path.display().to_string(),
    }
}

async fn job_logs(
    State(state): State<LocalState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    Query(q): Query<LogQuery>,
) -> Result<Json<LogChunk>, ApiError> {
    require_auth(&state, &headers)?;
    let path = gpumesh_common::jobs_dir().join(&id).join("job.log");
    Ok(Json(read_log_chunk(&path, q.offset)))
}

async fn agent_logs(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Query(q): Query<LogQuery>,
) -> Result<Json<LogChunk>, ApiError> {
    require_auth(&state, &headers)?;
    let path = gpumesh_common::agent_log_path();
    Ok(Json(read_log_chunk(&path, q.offset)))
}

#[derive(Serialize)]
struct LocalNetwork {
    listen_port: u16,
    share_running: bool,
    share_pid: Option<u32>,
    groups: Vec<LocalGroup>,
    home: String,
}

#[derive(Serialize)]
struct LocalGroup {
    id: String,
    name: String,
    members: usize,
    owner_node_id: String,
}

async fn network(
    State(state): State<LocalState>,
    headers: HeaderMap,
) -> Result<Json<LocalNetwork>, ApiError> {
    require_auth(&state, &headers)?;
    let cfg = StateStore::load_config().unwrap_or_default();
    let pid = share_pid();
    let groups = GroupStore::load()
        .map(|s| {
            s.list()
                .into_iter()
                .map(|g| LocalGroup {
                    id: g.id.clone(),
                    name: g.name.clone(),
                    members: g.members.len(),
                    owner_node_id: g.owner_node_id.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Json(LocalNetwork {
        listen_port: cfg.listen_port,
        share_running: pid.map(pid_alive).unwrap_or(false),
        share_pid: pid,
        groups,
        home: gpumesh_common::config_dir().display().to_string(),
    }))
}

#[derive(Serialize)]
struct PairCodeBody {
    code: String,
    node_name: String,
    node_id_short: String,
}

async fn pair_code(
    State(state): State<LocalState>,
    headers: HeaderMap,
) -> Result<Json<PairCodeBody>, ApiError> {
    require_auth(&state, &headers)?;
    let node = MeshNode::bootstrap().await.map_err(map_err)?;
    let code = node.pairing_code().await.map_err(map_err)?;
    let cfg = node.config.read().await;
    Ok(Json(PairCodeBody {
        code,
        node_name: cfg.node_name.clone(),
        node_id_short: short_fingerprint(&node.identity.node_id),
    }))
}

#[derive(Deserialize)]
struct CodeBody {
    code: String,
}

#[derive(Deserialize)]
struct PeerBody {
    peer: String,
}

async fn pair(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(body): Json<CodeBody>,
) -> Result<Json<LocalPeer>, ApiError> {
    require_auth(&state, &headers)?;
    let node = MeshNode::bootstrap().await.map_err(map_err)?;
    let rec = node.pair_with_code(body.code.trim()).await.map_err(map_err)?;
    let allow = StateStore::load_allowlist().unwrap_or_default();
    Ok(Json(LocalPeer {
        node_id: rec.node_id.clone(),
        node_id_short: short_fingerprint(&rec.node_id),
        node_name: rec.node_name,
        addrs: rec.addrs,
        gpu_model: rec.gpu_model,
        vram_mb: rec.vram_mb,
        vram_free_mb: None,
        utilization: None,
        last_seen: rec.last_seen,
        paired_at: rec.paired_at,
        allowed: allow.is_allowed(&rec.node_id),
        desktop_allowed: allow.is_desktop_allowed(&rec.node_id),
        gpu_remote_allowed: allow.is_gpu_remote_allowed(&rec.node_id),
        live_status: None,
        sharing: None,
    }))
}

async fn allow(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(body): Json<PeerBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&state, &headers)?;
    let node = MeshNode::bootstrap().await.map_err(map_err)?;
    node.allow_peer(&body.peer).await.map_err(map_err)?;
    Ok(Json(serde_json::json!({ "ok": true, "peer": body.peer })))
}

async fn allow_desktop(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(body): Json<PeerBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&state, &headers)?;
    let node = MeshNode::bootstrap().await.map_err(map_err)?;
    node.allow_desktop_peer(&body.peer).await.map_err(map_err)?;
    Ok(Json(serde_json::json!({ "ok": true, "peer": body.peer })))
}

async fn allow_cuda(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(body): Json<PeerBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&state, &headers)?;
    let node = MeshNode::bootstrap().await.map_err(map_err)?;
    node.allow_gpu_remote_peer(&body.peer).await.map_err(map_err)?;
    Ok(Json(serde_json::json!({ "ok": true, "peer": body.peer })))
}

async fn deny(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(body): Json<PeerBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&state, &headers)?;
    let node = MeshNode::bootstrap().await.map_err(map_err)?;
    node.deny_peer(&body.peer).await.map_err(map_err)?;
    Ok(Json(serde_json::json!({ "ok": true, "peer": body.peer })))
}

#[derive(Serialize)]
struct ConnectResult {
    ok: bool,
    peer: String,
    peer_name: Option<String>,
    remote_addr: String,
    mode: String,
}

async fn connect(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(body): Json<PeerBody>,
) -> Result<Json<ConnectResult>, ApiError> {
    require_auth(&state, &headers)?;
    let mut node = MeshNode::bootstrap().await.map_err(map_err)?;
    node.ensure_dialer().await.map_err(map_err)?;
    let conn = node.connect_peer(&body.peer).await.map_err(map_err)?;
    let result = ConnectResult {
        ok: true,
        peer: body.peer,
        peer_name: conn.peer_name.clone(),
        remote_addr: conn.remote_addr.clone(),
        mode: format!("{:?}", conn.connection_mode),
    };
    conn.close();
    Ok(Json(result))
}

#[derive(Deserialize)]
struct RunBody {
    peer: String,
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    command: Vec<String>,
    #[serde(default)]
    workdir: Option<String>,
    #[serde(default)]
    gpu_memory: Option<String>,
}

async fn run_job(
    State(state): State<LocalState>,
    headers: HeaderMap,
    Json(body): Json<RunBody>,
) -> Result<Json<LocalJob>, ApiError> {
    require_auth(&state, &headers)?;
    if body.peer.trim().is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "peer is required"));
    }
    if body.command.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "command is required"));
    }
    let image = body
        .image
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_IMAGE.to_string());
    let gpu_memory_mb = match body.gpu_memory.as_deref() {
        Some(s) if !s.trim().is_empty() => Some(parse_size_to_mb(s).map_err(map_err)?),
        _ => None,
    };
    let workdir = match body.workdir.as_deref() {
        Some(s) if !s.trim().is_empty() => std::path::PathBuf::from(s),
        _ => {
            let dir = std::env::temp_dir().join("gpumesh-dashboard-empty");
            let _ = std::fs::create_dir_all(&dir);
            dir
        }
    };
    let job_id = short_job_id();
    let rec = JobRecord::new(
        job_id.clone(),
        Some(body.peer.clone()),
        image.clone(),
        body.command.clone(),
    );
    rec.save().map_err(map_err)?;
    {
        let mut running = state.running.lock().await;
        running.insert(job_id.clone());
    }
    let running = state.running.clone();
    let peer = body.peer.clone();
    let command = body.command.clone();
    let jid = job_id.clone();
    tokio::spawn(async move {
        let outcome = async {
            let mut node = MeshNode::bootstrap().await?;
            node.ensure_dialer().await?;
            run_remote_job(
                &node,
                &peer,
                Some(image),
                command,
                workdir,
                Vec::new(),
                Some(jid.clone()),
                gpu_memory_mb,
                None,
            )
            .await
        };
        match outcome.await {
            Ok(code) => {
                if let Ok(mut rec) = JobRecord::load(&jid) {
                    rec.state = if code == 0 {
                        JobState::Succeeded
                    } else {
                        JobState::Failed
                    };
                    rec.exit_code = Some(code);
                    rec.finished_at = Some(Utc::now());
                    let _ = rec.save();
                }
            }
            Err(e) => {
                if let Ok(mut rec) = JobRecord::load(&jid) {
                    rec.state = JobState::Failed;
                    rec.error = Some(e.to_string());
                    rec.finished_at = Some(Utc::now());
                    let _ = rec.save();
                }
                let _ = JobRecord::append_log(&jid, &format!("[system] {e}"));
            }
        }
        running.lock().await.remove(&jid);
    });
    Ok(Json(job_to_json(&rec)))
}
