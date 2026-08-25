//! GPUMesh control plane — rendezvous + dashboard metadata API.
//! Never executes GPU workloads.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{Method, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone, Default)]
struct AppState {
    inner: Arc<RwLock<Store>>,
}

#[derive(Default)]
struct Store {
    peers: HashMap<String, PeerEntry>,
    nodes: HashMap<String, Announce>,
    public: HashMap<String, PublicEntry>,
    gpus: Vec<GpuInfo>,
    jobs: Vec<JobInfo>,
    groups: Vec<GroupInfo>,
}

#[derive(Clone, Serialize, Deserialize)]
struct PublicEntry {
    listing: PublicListing,
    updated_at: i64,
}

/// Public GPU registry listing (Phase 7) — metadata only.
#[derive(Clone, Serialize, Deserialize, Default)]
struct PublicListing {
    node_id: String,
    node_name: String,
    #[serde(default)]
    public_key_hex: String,
    #[serde(default)]
    addrs: Vec<String>,
    #[serde(default)]
    gpu_model: Option<String>,
    #[serde(default)]
    vram_mb: Option<u64>,
    #[serde(default)]
    vram_free_mb: Option<u64>,
    #[serde(default)]
    cuda_version: Option<String>,
    #[serde(default)]
    availability: String,
    #[serde(default)]
    perf_score: Option<u32>,
    #[serde(default)]
    latency_ms: Option<u32>,
    #[serde(default)]
    region: Option<String>,
    #[serde(default)]
    uptime_secs: Option<u64>,
    #[serde(default)]
    sharing: bool,
    #[serde(default)]
    public: bool,
    #[serde(default)]
    utilization: Option<u32>,
}

#[derive(Deserialize)]
struct UnannounceBody {
    node_id: String,
}

#[derive(Deserialize)]
struct SearchQuery {
    gpu: Option<String>,
    min_vram_mb: Option<u64>,
    cuda: Option<String>,
    region: Option<String>,
    available: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct PeerEntry {
    peer: Peer,
    updated_at: i64,
}

#[derive(Clone, Serialize, Deserialize, Default)]
struct Announce {
    node_id: String,
    node_name: String,
    #[serde(default)]
    public_key_hex: String,
    #[serde(default)]
    addrs: Vec<String>,
    #[serde(default)]
    sharing: bool,
    #[serde(default)]
    gpu_model: Option<String>,
    #[serde(default)]
    vram_mb: Option<u64>,
    #[serde(default)]
    vram_free_mb: Option<u64>,
    #[serde(default)]
    utilization: Option<u32>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
struct Peer {
    node_id: String,
    node_name: String,
    #[serde(default)]
    addrs: Vec<String>,
    #[serde(default)]
    gpu_model: Option<String>,
    #[serde(default)]
    vram_mb: Option<u64>,
    #[serde(default)]
    vram_free_mb: Option<u64>,
    #[serde(default)]
    utilization: Option<u32>,
    #[serde(default)]
    sharing: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct GpuInfo {
    index: u32,
    name: String,
    vram_total_mb: u64,
    vram_used_mb: u64,
    vram_free_mb: u64,
    #[serde(default)]
    utilization: Option<u32>,
    #[serde(default)]
    temperature_c: Option<u32>,
    #[serde(default)]
    node_id: String,
    #[serde(default)]
    node_name: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct JobInfo {
    job_id: String,
    #[serde(default)]
    peer: Option<String>,
    state: String,
    #[serde(default)]
    exit_code: Option<i32>,
    #[serde(default)]
    image: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    node_id: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct GroupInfo {
    id: String,
    name: String,
    members: usize,
    owner_node_id: String,
}

#[derive(Deserialize)]
struct SyncPayload {
    node: Announce,
    #[serde(default)]
    gpus: Vec<GpuInfo>,
    #[serde(default)]
    peers: Vec<Peer>,
    #[serde(default)]
    jobs: Vec<JobInfo>,
    #[serde(default)]
    groups: Vec<GroupInfo>,
}

#[derive(Serialize)]
struct Overview {
    gpus_online: usize,
    gpus_available: usize,
    running_jobs: usize,
    total_vram_gb: u64,
    peers: usize,
    groups: usize,
    nodes: usize,
    updated_at: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    let state = AppState::default();
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any);

    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/v1/announce", post(announce))
        .route("/v1/sync", post(sync))
        .route("/v1/peers", get(list_peers))
        .route("/v1/peers/{id}", get(get_peer))
        .route("/v1/overview", get(overview))
        .route("/v1/gpus", get(gpus))
        .route("/v1/jobs", get(jobs))
        .route("/v1/groups", get(groups))
        .route("/v1/nodes", get(nodes))
        .route("/v1/network", get(network))
        .route("/v1/usage", get(usage))
        .route("/v1/settings", get(settings))
        .route("/v1/public/announce", post(public_announce))
        .route("/v1/public/unannounce", post(public_unannounce))
        .route("/v1/public/search", get(public_search))
        .route("/v1/public/nodes/{id}", get(public_get))
        .layer(cors)
        .with_state(state);

    let addr: SocketAddr = std::env::var("GPUMESH_API_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()?;
    tracing::info!("gpumesh-control listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn announce(
    State(state): State<AppState>,
    Json(ann): Json<Announce>,
) -> StatusCode {
    if ann.node_id.is_empty() {
        return StatusCode::BAD_REQUEST;
    }
    let mut s = state.inner.write().await;
    upsert_node(&mut s, ann);
    StatusCode::NO_CONTENT
}

async fn sync(State(state): State<AppState>, Json(payload): Json<SyncPayload>) -> StatusCode {
    if payload.node.node_id.is_empty() {
        return StatusCode::BAD_REQUEST;
    }
    let mut s = state.inner.write().await;
    let node_id = payload.node.node_id.clone();
    let node_name = payload.node.node_name.clone();
    upsert_node(&mut s, payload.node);

    s.gpus.retain(|g| g.node_id != node_id);
    let mut gpus = payload.gpus;
    for g in &mut gpus {
        g.node_id = node_id.clone();
        g.node_name = node_name.clone();
    }
    s.gpus.extend(gpus);

    s.jobs.retain(|j| j.node_id != node_id);
    let mut jobs = payload.jobs;
    for j in &mut jobs {
        j.node_id = node_id.clone();
    }
    s.jobs.extend(jobs);
    s.groups = payload.groups;

    // Merge paired peer metadata
    for p in payload.peers {
        s.peers.insert(
            p.node_id.clone(),
            PeerEntry {
                peer: p,
                updated_at: Utc::now().timestamp(),
            },
        );
    }
    StatusCode::NO_CONTENT
}

fn upsert_node(s: &mut Store, ann: Announce) {
    let peer = Peer {
        node_id: ann.node_id.clone(),
        node_name: ann.node_name.clone(),
        addrs: ann.addrs.clone(),
        gpu_model: ann.gpu_model.clone(),
        vram_mb: ann.vram_mb,
        vram_free_mb: ann.vram_free_mb,
        utilization: ann.utilization,
        sharing: ann.sharing,
    };
    s.peers.insert(
        ann.node_id.clone(),
        PeerEntry {
            peer,
            updated_at: Utc::now().timestamp(),
        },
    );
    s.nodes.insert(ann.node_id.clone(), ann);
}

async fn list_peers(State(state): State<AppState>) -> Json<Vec<Peer>> {
    let s = state.inner.read().await;
    let now = Utc::now().timestamp();
    let out = s
        .peers
        .values()
        .filter(|e| now - e.updated_at < 1800)
        .map(|e| e.peer.clone())
        .collect();
    Json(out)
}

async fn get_peer(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Peer>, StatusCode> {
    let s = state.inner.read().await;
    s.peers
        .get(&id)
        .map(|e| Json(e.peer.clone()))
        .ok_or(StatusCode::NOT_FOUND)
}

async fn overview(State(state): State<AppState>) -> Json<Overview> {
    let s = state.inner.read().await;
    let mut total = 0u64;
    let mut available = 0usize;
    for g in &s.gpus {
        total += g.vram_total_mb;
        if g.utilization.unwrap_or(0) < 50 {
            available += 1;
        }
    }
    let running = s
        .jobs
        .iter()
        .filter(|j| j.state == "RUNNING" || j.state == "STARTING")
        .count();
    Json(Overview {
        gpus_online: s.gpus.len(),
        gpus_available: available,
        running_jobs: running,
        total_vram_gb: total / 1024,
        peers: s.peers.len(),
        groups: s.groups.len(),
        nodes: s.nodes.len(),
        updated_at: Utc::now().to_rfc3339(),
    })
}

async fn gpus(State(state): State<AppState>) -> Json<Vec<GpuInfo>> {
    Json(state.inner.read().await.gpus.clone())
}

async fn jobs(State(state): State<AppState>) -> Json<Vec<JobInfo>> {
    Json(state.inner.read().await.jobs.clone())
}

async fn groups(State(state): State<AppState>) -> Json<Vec<GroupInfo>> {
    Json(state.inner.read().await.groups.clone())
}

async fn nodes(State(state): State<AppState>) -> Json<Vec<Announce>> {
    Json(state.inner.read().await.nodes.values().cloned().collect())
}

async fn network(State(state): State<AppState>) -> Json<serde_json::Value> {
    let s = state.inner.read().await;
    Json(serde_json::json!({
        "nodes": s.nodes.len(),
        "peers": s.peers.len(),
        "groups": s.groups.len(),
        "control": "rendezvous+metadata",
        "workloads": false,
        "description": "Control plane never runs GPU workloads",
    }))
}

async fn usage(State(state): State<AppState>) -> Json<serde_json::Value> {
    let s = state.inner.read().await;
    let succeeded = s.jobs.iter().filter(|j| j.state == "SUCCEEDED").count();
    let failed = s.jobs.iter().filter(|j| j.state == "FAILED").count();
    Json(serde_json::json!({
        "jobs_total": s.jobs.len(),
        "jobs_succeeded": succeeded,
        "jobs_failed": failed,
        "nodes": s.nodes.len(),
    }))
}

async fn settings() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "product": "GPUMesh Cloud",
        "phase": "7",
        "api_version": "v1",
        "security": "Ed25519 peer identity; public listing is metadata only; workloads stay allowlisted",
        "public_registry": true,
    }))
}

const PUBLIC_TTL_SECS: i64 = 180;

async fn public_announce(
    State(state): State<AppState>,
    Json(mut listing): Json<PublicListing>,
) -> StatusCode {
    if listing.node_id.is_empty() {
        return StatusCode::BAD_REQUEST;
    }
    listing.public = true;
    if listing.availability.is_empty() {
        listing.availability = if listing.sharing {
            "idle".into()
        } else {
            "offline".into()
        };
    }
    let mut s = state.inner.write().await;
    s.public.insert(
        listing.node_id.clone(),
        PublicEntry {
            listing,
            updated_at: Utc::now().timestamp(),
        },
    );
    StatusCode::NO_CONTENT
}

async fn public_unannounce(
    State(state): State<AppState>,
    Json(body): Json<UnannounceBody>,
) -> StatusCode {
    if body.node_id.is_empty() {
        return StatusCode::BAD_REQUEST;
    }
    state.inner.write().await.public.remove(&body.node_id);
    StatusCode::NO_CONTENT
}

async fn public_search(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> Json<Vec<PublicListing>> {
    let s = state.inner.read().await;
    let now = Utc::now().timestamp();
    let gpu = q.gpu.as_deref().map(|g| g.to_ascii_lowercase());
    let cuda = q.cuda.as_deref().map(|c| c.to_ascii_lowercase());
    let region = q.region.as_deref().map(|r| r.to_ascii_lowercase());
    let available_only = matches!(
        q.available.as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("idle")
    );
    let mut out: Vec<PublicListing> = s
        .public
        .values()
        .filter(|e| now - e.updated_at < PUBLIC_TTL_SECS)
        .filter(|e| e.listing.public && e.listing.sharing)
        .filter(|e| {
            if let Some(ref g) = gpu {
                e.listing
                    .gpu_model
                    .as_deref()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .contains(g)
            } else {
                true
            }
        })
        .filter(|e| {
            if let Some(min) = q.min_vram_mb {
                e.listing.vram_mb.unwrap_or(0) >= min
                    || e.listing.vram_free_mb.unwrap_or(0) >= min
            } else {
                true
            }
        })
        .filter(|e| {
            if let Some(ref c) = cuda {
                e.listing
                    .cuda_version
                    .as_deref()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .contains(c)
            } else {
                true
            }
        })
        .filter(|e| {
            if let Some(ref r) = region {
                e.listing
                    .region
                    .as_deref()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .contains(r)
            } else {
                true
            }
        })
        .filter(|e| {
            if available_only {
                e.listing.availability.eq_ignore_ascii_case("idle")
            } else {
                true
            }
        })
        .map(|e| e.listing.clone())
        .collect();
    out.sort_by(|a, b| {
        b.perf_score
            .unwrap_or(0)
            .cmp(&a.perf_score.unwrap_or(0))
            .then_with(|| {
                b.vram_free_mb
                    .unwrap_or(0)
                    .cmp(&a.vram_free_mb.unwrap_or(0))
            })
    });
    Json(out)
}

async fn public_get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PublicListing>, StatusCode> {
    let s = state.inner.read().await;
    let now = Utc::now().timestamp();
    s.public
        .get(&id)
        .filter(|e| now - e.updated_at < PUBLIC_TTL_SECS)
        .map(|e| Json(e.listing.clone()))
        .ok_or(StatusCode::NOT_FOUND)
}
