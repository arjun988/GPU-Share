//! GPUMesh control plane — rendezvous, dashboard metadata, and local console API.
//! This process never runs GPU workloads; `/v1/local/*` reads `~/.gpumesh` and
//! may dial peers (pair/connect/run) using an ephemeral QUIC endpoint.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use gpumesh_network::{PublicListing, PublicUnannounce};
use gpumesh_security::{verify_public_listing, verify_signature};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use tower_http::cors::CorsLayer;

mod local;

#[derive(Clone)]
struct AppState {
    inner: Arc<RwLock<Store>>,
    state_path: Arc<PathBuf>,
    api_token: Option<Arc<String>>,
}

#[derive(Default, Serialize, Deserialize)]
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
    tracing_subscriber::fmt().with_env_filter("info").init();

    let state_path = PathBuf::from(
        std::env::var("GPUMESH_CONTROL_STATE")
            .unwrap_or_else(|_| "./.gpumesh-control-state.json".into()),
    );
    let store = if state_path.exists() {
        let data = std::fs::read(&state_path)?;
        serde_json::from_slice(&data)?
    } else {
        Store::default()
    };
    let state = AppState {
        inner: Arc::new(RwLock::new(store)),
        state_path: Arc::new(state_path),
        api_token: gpumesh_common::api_token().map(Arc::new),
    };
    let cors = CorsLayer::new()
        .allow_origin([
            HeaderValue::from_static("http://127.0.0.1:3000"),
            HeaderValue::from_static("http://localhost:3000"),
            HeaderValue::from_static("http://127.0.0.1:3001"),
        ])
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([CONTENT_TYPE, AUTHORIZATION]);

    let local_state = local::LocalState {
        api_token: state.api_token.clone(),
        running: Arc::new(Mutex::new(std::collections::HashSet::new())),
    };

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
        .with_state(state)
        .merge(local::router(local_state))
        .layer(cors);

    let addr: SocketAddr = std::env::var("GPUMESH_API_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()?;
    tracing::info!("gpumesh-control listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn authorized(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(token) = &state.api_token else {
        return true;
    };
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == format!("Bearer {token}"))
}

async fn persist_state(state: &AppState) -> Result<(), StatusCode> {
    let data = {
        let store = state.inner.read().await;
        serde_json::to_vec_pretty(&*store).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };
    if let Some(parent) = state.state_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    tokio::fs::write(&*state.state_path, data)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn announce(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(ann): Json<Announce>,
) -> StatusCode {
    if !authorized(&state, &headers) {
        return StatusCode::UNAUTHORIZED;
    }
    if ann.node_id.is_empty() {
        return StatusCode::BAD_REQUEST;
    }
    {
        let mut s = state.inner.write().await;
        upsert_node(&mut s, ann);
    }
    persist_state(&state)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .unwrap_or_else(|status| status)
}

async fn sync(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<SyncPayload>,
) -> StatusCode {
    if !authorized(&state, &headers) {
        return StatusCode::UNAUTHORIZED;
    }
    if payload.node.node_id.is_empty() {
        return StatusCode::BAD_REQUEST;
    }
    {
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
    }
    persist_state(&state)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .unwrap_or_else(|status| status)
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
        "product": "GPUMesh",
        "phase": "7",
        "api_version": "v1",
        "local_console": true,
        "security": "Ed25519 peer identity; public listing is metadata only; workloads stay allowlisted",
        "public_registry": true,
    }))
}

const PUBLIC_TTL_SECS: i64 = 180;

async fn public_announce(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(listing): Json<PublicListing>,
) -> StatusCode {
    if !authorized(&state, &headers) {
        return StatusCode::UNAUTHORIZED;
    }
    if listing.node_id.is_empty() || verify_public_listing(&listing).is_err() {
        return StatusCode::BAD_REQUEST;
    }
    {
        let mut s = state.inner.write().await;
        s.public.insert(
            listing.node_id.clone(),
            PublicEntry {
                listing,
                updated_at: Utc::now().timestamp(),
            },
        );
    }
    persist_state(&state)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .unwrap_or_else(|status| status)
}

async fn public_unannounce(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PublicUnannounce>,
) -> StatusCode {
    if !authorized(&state, &headers) {
        return StatusCode::UNAUTHORIZED;
    }
    let now = Utc::now().timestamp();
    if body.node_id.is_empty()
        || body.issued_at <= 0
        || now - body.issued_at > gpumesh_common::ANNOUNCE_TTL_SECS
        || body.issued_at > now + 60
    {
        return StatusCode::BAD_REQUEST;
    }
    {
        let mut s = state.inner.write().await;
        let Some(entry) = s.public.get(&body.node_id) else {
            return StatusCode::NOT_FOUND;
        };
        if verify_signature(
            &entry.listing.public_key_hex,
            &body.canonical_bytes(),
            &body.signature,
        )
        .is_err()
        {
            return StatusCode::UNAUTHORIZED;
        }
        s.public.remove(&body.node_id);
    }
    persist_state(&state)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .unwrap_or_else(|status| status)
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
                e.listing.vram_mb.unwrap_or(0) >= min || e.listing.vram_free_mb.unwrap_or(0) >= min
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
