//! CUDA remoting (R2 spike + R3 driver backend).
//!
//! Remotes a Runtime-API subset over GPUMesh pairing. Prefer `cuda-driver` when
//! libcuda loads; otherwise `host-memory`. Not a drop-in `libcuda` for arbitrary apps.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use gpumesh_common::{GpuMeshError, Result};
use gpumesh_gpu::GpuMonitor;
use gpumesh_network::PeerConnection;
use gpumesh_protocol::{CudaDeviceInfo, CudaOpKind, Message};
use tracing::{info, warn};
use uuid::Uuid;

use crate::cuda_backend::BackendSession;
use crate::MeshNode;

pub const CUDA_REMOTE_CLIENT_VER: u32 = 2;
pub const CUDA_REMOTE_API: &str = "cuda";

const DEFAULT_MAX_ALLOC: u64 = 512 * 1024 * 1024;

pub(crate) struct CudaSession {
    #[allow(dead_code)]
    session_id: String,
    backend: BackendSession,
}

impl CudaSession {
    fn new(session_id: String, max_alloc: u64, devices: Vec<CudaDeviceInfo>) -> Self {
        let backend = BackendSession::open(max_alloc, devices);
        Self {
            session_id,
            backend,
        }
    }

    fn backend_name(&self) -> &str {
        &self.backend.backend_name
    }

    fn exec(
        &mut self,
        op: CudaOpKind,
    ) -> (bool, Option<String>, crate::cuda_backend::CudaPartial) {
        self.backend.exec(op)
    }
}

fn collect_devices() -> Vec<CudaDeviceInfo> {
    GpuMonitor::detect()
        .unwrap_or_default()
        .into_iter()
        .map(|g| CudaDeviceInfo {
            index: g.index,
            name: g.name,
            vram_total_mb: g.vram_total_mb,
            vram_free_mb: g.vram_free_mb,
            compute_capability: g.compute_capability,
        })
        .collect()
}

/// Provider: handle GpuRemoteOpen.
pub async fn handle_gpu_remote_open(
    node: &MeshNode,
    conn: &PeerConnection,
    api: String,
    client_ver: u32,
    sessions: &mut HashMap<String, CudaSession>,
) -> Result<()> {
    let enabled = {
        let cfg = node.config.read().await;
        *node.cuda_remote_sharing.read().await || cfg.cuda_remote_sharing
    };
    if !enabled {
        conn.send(Message::GpuRemoteReject {
            reason: "CUDA remoting is not enabled (host: gpumesh cuda share)".into(),
        })
        .await?;
        return Ok(());
    }

    if api != CUDA_REMOTE_API {
        conn.send(Message::GpuRemoteReject {
            reason: format!("unsupported remoting api '{api}' (supports 'cuda'; OpenGL deferred)"),
        })
        .await?;
        return Ok(());
    }
    if client_ver == 0 || client_ver > CUDA_REMOTE_CLIENT_VER {
        conn.send(Message::GpuRemoteReject {
            reason: format!(
                "unsupported client_ver {client_ver} (host supports ≤ {CUDA_REMOTE_CLIENT_VER})"
            ),
        })
        .await?;
        return Ok(());
    }

    let peer_id = conn.peer_node_id.clone().unwrap_or_default();
    {
        let allow = node.allowlist.read().await;
        if !allow.is_gpu_remote_allowed(&peer_id) {
            conn.send(Message::GpuRemoteReject {
                reason: format!(
                    "CUDA remoting not allowed — host runs: gpumesh cuda allow <peer> (node {peer_id})"
                ),
            })
            .await?;
            return Ok(());
        }
    }

    let devices = collect_devices();
    if devices.is_empty() {
        conn.send(Message::GpuRemoteReject {
            reason: "no NVIDIA GPU detected on host (NVML / nvidia-smi)".into(),
        })
        .await?;
        return Ok(());
    }

    let limits = node.share_limits().await;
    if let Some(max_util) = limits.max_gpu_utilization {
        if let Some(g) = GpuMonitor::detect().ok().and_then(|g| g.into_iter().next()) {
            if g.utilization_gpu.unwrap_or(0) > u32::from(max_util) {
                conn.send(Message::GpuRemoteReject {
                    reason: format!(
                        "host GPU util {}% exceeds max {max_util}%",
                        g.utilization_gpu.unwrap_or(0)
                    ),
                })
                .await?;
                return Ok(());
            }
        }
    }

    let max_alloc = {
        let cfg = node.config.read().await;
        (cfg.cuda_remote_max_alloc_mb.max(1)) * 1024 * 1024
    }
    .min(DEFAULT_MAX_ALLOC * 4);

    let session_id = Uuid::new_v4().to_string();
    let session = CudaSession::new(session_id.clone(), max_alloc, devices.clone());
    let backend = session.backend_name().to_string();
    sessions.insert(session_id.clone(), session);

    conn.send(Message::GpuRemoteOffer {
        session_id: session_id.clone(),
        api: CUDA_REMOTE_API.into(),
        backend,
        max_alloc_bytes: max_alloc,
        devices,
        lan_warning:
            "CUDA remoting is LAN-oriented (R3). WAN Runtime remoting is usually too slow."
                .into(),
    })
    .await?;
    info!("CUDA remoting session {session_id} opened for {peer_id}");
    Ok(())
}

pub async fn handle_cuda_op(
    conn: &PeerConnection,
    sessions: &mut HashMap<String, CudaSession>,
    session_id: String,
    op_id: u64,
    op: CudaOpKind,
) -> Result<()> {
    let Some(session) = sessions.get_mut(&session_id) else {
        conn.send(Message::CudaResult {
            session_id,
            op_id,
            ok: false,
            error: Some("unknown session".into()),
            elapsed_us: 0,
            device_count: None,
            device: None,
            ptr: None,
            data: None,
            free_bytes: None,
            total_bytes: None,
            device_index: None,
            event_id: None,
            module_id: None,
            elapsed_ms: None,
        })
        .await?;
        return Ok(());
    };

    let start = Instant::now();
    let (ok, error, partial) = session.exec(op);
    let elapsed_us = start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;

    conn.send(Message::CudaResult {
        session_id,
        op_id,
        ok,
        error,
        elapsed_us,
        device_count: partial.device_count,
        device: partial.device,
        ptr: partial.ptr,
        data: partial.data,
        free_bytes: partial.free_bytes,
        total_bytes: partial.total_bytes,
        device_index: partial.device_index,
        event_id: partial.event_id,
        module_id: partial.module_id,
        elapsed_ms: partial.elapsed_ms,
    })
    .await?;
    Ok(())
}

pub fn handle_gpu_remote_close(sessions: &mut HashMap<String, CudaSession>, session_id: &str) {
    if sessions.remove(session_id).is_some() {
        info!("CUDA remoting session {session_id} closed");
    } else {
        warn!("GpuRemoteClose for unknown session {session_id}");
    }
}

struct OpResult {
    ok: bool,
    error: Option<String>,
    elapsed_us: u64,
    device_count: Option<u32>,
    device: Option<CudaDeviceInfo>,
    ptr: Option<u64>,
    data: Option<Vec<u8>>,
}

/// Client-side remoting session.
pub struct CudaRemoteClient {
    conn: PeerConnection,
    pub session_id: String,
    pub backend: String,
    pub max_alloc_bytes: u64,
    pub devices: Vec<CudaDeviceInfo>,
    pub lan_warning: String,
    next_op: AtomicU64,
}

impl CudaRemoteClient {
    pub async fn open(node: &MeshNode, peer: &str) -> Result<Self> {
        let conn = node.connect_peer(peer).await?;
        conn.send(Message::GpuRemoteOpen {
            api: CUDA_REMOTE_API.into(),
            client_ver: CUDA_REMOTE_CLIENT_VER,
        })
        .await?;

        match conn.recv().await? {
            Some(Message::GpuRemoteOffer {
                session_id,
                backend,
                max_alloc_bytes,
                devices,
                lan_warning,
                ..
            }) => Ok(Self {
                conn,
                session_id,
                backend,
                max_alloc_bytes,
                devices,
                lan_warning,
                next_op: AtomicU64::new(1),
            }),
            Some(Message::GpuRemoteReject { reason }) => Err(GpuMeshError::NotAuthorized(reason)),
            Some(Message::Error { message }) => Err(GpuMeshError::Network(message)),
            other => Err(GpuMeshError::Protocol(format!(
                "expected GpuRemoteOffer, got {other:?}"
            ))),
        }
    }

    async fn call(&mut self, op: CudaOpKind) -> Result<OpResult> {
        let op_id = self.next_op.fetch_add(1, Ordering::Relaxed);
        self.conn
            .send(Message::CudaOp {
                session_id: self.session_id.clone(),
                op_id,
                op,
            })
            .await?;
        loop {
            match self.conn.recv().await? {
                Some(Message::CudaResult {
                    session_id,
                    op_id: rid,
                    ok,
                    error,
                    elapsed_us,
                    device_count,
                    device,
                    ptr,
                    data,
                    ..
                }) if session_id == self.session_id && rid == op_id => {
                    return Ok(OpResult {
                        ok,
                        error,
                        elapsed_us,
                        device_count,
                        device,
                        ptr,
                        data,
                    });
                }
                Some(Message::Error { message }) => return Err(GpuMeshError::Network(message)),
                None => return Err(GpuMeshError::Network("connection closed".into())),
                _ => continue,
            }
        }
    }

    fn unwrap_ok(r: OpResult) -> Result<OpResult> {
        if r.ok {
            Ok(r)
        } else {
            Err(GpuMeshError::Runtime(
                r.error.unwrap_or_else(|| "cuda remoting op failed".into()),
            ))
        }
    }

    pub async fn device_count(&mut self) -> Result<u32> {
        let r = Self::unwrap_ok(self.call(CudaOpKind::DeviceCount).await?)?;
        r.device_count
            .ok_or_else(|| GpuMeshError::Protocol("missing device_count".into()))
    }

    pub async fn malloc(&mut self, bytes: u64) -> Result<u64> {
        let r = Self::unwrap_ok(self.call(CudaOpKind::Malloc { bytes }).await?)?;
        r.ptr
            .ok_or_else(|| GpuMeshError::Protocol("missing ptr".into()))
    }

    pub async fn free(&mut self, ptr: u64) -> Result<()> {
        let _ = Self::unwrap_ok(self.call(CudaOpKind::Free { ptr }).await?)?;
        Ok(())
    }

    pub async fn memcpy_htod(&mut self, dst: u64, data: &[u8]) -> Result<u64> {
        let r = Self::unwrap_ok(
            self.call(CudaOpKind::MemcpyHtoD {
                dst,
                data: data.to_vec(),
            })
            .await?,
        )?;
        Ok(r.elapsed_us)
    }

    pub async fn memcpy_dtoh(&mut self, src: u64, bytes: u64) -> Result<(Vec<u8>, u64)> {
        let r = Self::unwrap_ok(self.call(CudaOpKind::MemcpyDtoH { src, bytes }).await?)?;
        let data = r
            .data
            .ok_or_else(|| GpuMeshError::Protocol("missing data".into()))?;
        Ok((data, r.elapsed_us))
    }

    pub async fn memcpy_dtod(&mut self, dst: u64, src: u64, bytes: u64) -> Result<u64> {
        let r = Self::unwrap_ok(
            self.call(CudaOpKind::MemcpyDtoD { dst, src, bytes })
                .await?,
        )?;
        Ok(r.elapsed_us)
    }

    pub async fn vector_add_f32(&mut self, a: u64, b: u64, out: u64, n: u32) -> Result<u64> {
        let r = Self::unwrap_ok(
            self.call(CudaOpKind::VectorAddF32 { a, b, out, n })
                .await?,
        )?;
        Ok(r.elapsed_us)
    }

    pub async fn sync(&mut self) -> Result<u64> {
        let r = Self::unwrap_ok(self.call(CudaOpKind::Sync).await?)?;
        Ok(r.elapsed_us)
    }

    pub async fn close(self) -> Result<()> {
        self.conn
            .send(Message::GpuRemoteClose {
                session_id: self.session_id.clone(),
            })
            .await?;
        self.conn.close();
        Ok(())
    }
}

/// Local TCP bridge so C apps can use `libcudart` stub without embedding QUIC.
pub async fn run_bridge(node: &MeshNode, peer: &str, bind: &str) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

    let client = CudaRemoteClient::open(node, peer).await?;
    let listener = TcpListener::bind(bind)
        .await
        .map_err(|e| GpuMeshError::Network(e.to_string()))?;
    let addr = listener
        .local_addr()
        .map_err(|e| GpuMeshError::Network(e.to_string()))?;
    println!("CUDA remoting bridge listening on {addr}");
    println!("  Peer:    {peer}");
    println!("  Backend: {}", client.backend);
    println!("  Export:  GPUMESH_CUDA_BRIDGE={addr}");
    println!("  Stub:    cargo build -p gpumesh-cudart-stub");
    println!("Leave this running. Ctrl+C to stop.");

    let client = std::sync::Arc::new(Mutex::new(client));
    loop {
        let (mut sock, _) = listener
            .accept()
            .await
            .map_err(|e| GpuMeshError::Network(e.to_string()))?;
        let client = client.clone();
        tokio::spawn(async move {
            loop {
                let mut lenb = [0u8; 4];
                if sock.read_exact(&mut lenb).await.is_err() {
                    break;
                }
                let n = u32::from_be_bytes(lenb) as usize;
                if n == 0 || n > 64 * 1024 * 1024 {
                    break;
                }
                let mut buf = vec![0u8; n];
                if sock.read_exact(&mut buf).await.is_err() {
                    break;
                }
                let req: serde_json::Value = match serde_json::from_slice(&buf) {
                    Ok(v) => v,
                    Err(_) => break,
                };
                let op = req.get("op").and_then(|v| v.as_str()).unwrap_or("");
                let mut c = client.lock().await;
                let resp = match op {
                    "device_count" => match c.device_count().await {
                        Ok(n) => serde_json::json!({"ok": true, "device_count": n}),
                        Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                    },
                    "malloc" => {
                        let bytes = req.get("bytes").and_then(|v| v.as_u64()).unwrap_or(0);
                        match c.malloc(bytes).await {
                            Ok(ptr) => serde_json::json!({"ok": true, "ptr": ptr}),
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        }
                    }
                    "free" => {
                        let ptr = req.get("ptr").and_then(|v| v.as_u64()).unwrap_or(0);
                        match c.free(ptr).await {
                            Ok(()) => serde_json::json!({"ok": true}),
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        }
                    }
                    "htod" => {
                        let dst = req.get("dst").and_then(|v| v.as_u64()).unwrap_or(0);
                        let data = req
                            .get("data")
                            .and_then(|v| v.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|x| x.as_u64().map(|n| n as u8))
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        match c.memcpy_htod(dst, &data).await {
                            Ok(_) => serde_json::json!({"ok": true}),
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        }
                    }
                    "dtoh" => {
                        let src = req.get("src").and_then(|v| v.as_u64()).unwrap_or(0);
                        let bytes = req.get("bytes").and_then(|v| v.as_u64()).unwrap_or(0);
                        match c.memcpy_dtoh(src, bytes).await {
                            Ok((data, _)) => serde_json::json!({"ok": true, "data": data}),
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        }
                    }
                    "sync" => match c.sync().await {
                        Ok(_) => serde_json::json!({"ok": true}),
                        Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                    },
                    "vector_add_f32" => {
                        let a = req.get("dst").and_then(|v| v.as_u64()).unwrap_or(0);
                        let b = req.get("src").and_then(|v| v.as_u64()).unwrap_or(0);
                        let out = req.get("ptr").and_then(|v| v.as_u64()).unwrap_or(0);
                        let n = req.get("n").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        match c.vector_add_f32(a, b, out, n).await {
                            Ok(_) => serde_json::json!({"ok": true}),
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                        }
                    }
                    _ => serde_json::json!({"ok": false, "error": "unknown op"}),
                };
                drop(c);
                let out = serde_json::to_vec(&resp).unwrap_or_default();
                let _ = sock.write_all(&(out.len() as u32).to_be_bytes()).await;
                let _ = sock.write_all(&out).await;
            }
        });
    }
}

/// End-to-end vector-add demo + latency samples.
pub async fn run_demo(node: &MeshNode, peer: &str, n: u32) -> Result<DemoReport> {
    let mut client = CudaRemoteClient::open(node, peer).await?;
    let t_open = Instant::now();
    let count = client.device_count().await?;
    let open_us = t_open.elapsed().as_micros() as u64;

    let bytes = (n as u64) * 4;
    let a = client.malloc(bytes).await?;
    let b = client.malloc(bytes).await?;
    let out = client.malloc(bytes).await?;

    let mut ha = vec![0u8; bytes as usize];
    let mut hb = vec![0u8; bytes as usize];
    for i in 0..n as usize {
        let o = i * 4;
        ha[o..o + 4].copy_from_slice(&(i as f32).to_le_bytes());
        hb[o..o + 4].copy_from_slice(&((i as f32) * 2.0).to_le_bytes());
    }

    let htod_a = client.memcpy_htod(a, &ha).await?;
    let htod_b = client.memcpy_htod(b, &hb).await?;
    let kernel_us = client.vector_add_f32(a, b, out, n).await?;
    let (got, dtoh_us) = client.memcpy_dtoh(out, bytes).await?;
    let sync_us = client.sync().await?;

    // Verify a few elements
    let mut ok = got.len() == ha.len();
    if ok {
        for i in [0usize, (n as usize / 2).saturating_sub(1), (n as usize).saturating_sub(1)] {
            if i >= n as usize {
                continue;
            }
            let o = i * 4;
            let x = f32::from_le_bytes(got[o..o + 4].try_into().unwrap());
            let expect = (i as f32) + (i as f32) * 2.0;
            if (x - expect).abs() > 1e-3 {
                ok = false;
                break;
            }
        }
    }

    client.free(a).await?;
    client.free(b).await?;
    client.free(out).await?;

    let report = DemoReport {
        peer: peer.to_string(),
        session_id: client.session_id.clone(),
        backend: client.backend.clone(),
        lan_warning: client.lan_warning.clone(),
        devices: client.devices.clone(),
        device_count: count,
        n,
        verified: ok,
        open_us,
        htod_a_us: htod_a,
        htod_b_us: htod_b,
        kernel_us,
        dtoh_us,
        sync_us,
        max_alloc_bytes: client.max_alloc_bytes,
    };
    client.close().await?;
    Ok(report)
}

#[derive(Debug, Clone)]
pub struct DemoReport {
    pub peer: String,
    pub session_id: String,
    pub backend: String,
    pub lan_warning: String,
    pub devices: Vec<CudaDeviceInfo>,
    pub device_count: u32,
    pub n: u32,
    pub verified: bool,
    pub open_us: u64,
    pub htod_a_us: u64,
    pub htod_b_us: u64,
    pub kernel_us: u64,
    pub dtoh_us: u64,
    pub sync_us: u64,
    pub max_alloc_bytes: u64,
}

/// Round-trip latency bench (Sync + small memcpy).
pub async fn run_bench(node: &MeshNode, peer: &str, iters: u32) -> Result<BenchReport> {
    let mut client = CudaRemoteClient::open(node, peer).await?;
    let ptr = client.malloc(4096).await?;
    let payload = vec![0xABu8; 4096];

    let mut sync_samples = Vec::with_capacity(iters as usize);
    let mut copy_samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let t0 = Instant::now();
        let _ = client.sync().await?;
        sync_samples.push(t0.elapsed().as_micros() as u64);
        let t1 = Instant::now();
        let _ = client.memcpy_htod(ptr, &payload).await?;
        copy_samples.push(t1.elapsed().as_micros() as u64);
    }
    client.free(ptr).await?;
    let backend = client.backend.clone();
    let warning = client.lan_warning.clone();
    client.close().await?;

    Ok(BenchReport {
        peer: peer.to_string(),
        backend,
        lan_warning: warning,
        iters,
        sync_us: summarize(&sync_samples),
        memcpy_4k_us: summarize(&copy_samples),
    })
}

#[derive(Debug, Clone)]
pub struct BenchReport {
    pub peer: String,
    pub backend: String,
    pub lan_warning: String,
    pub iters: u32,
    pub sync_us: LatencySummary,
    pub memcpy_4k_us: LatencySummary,
}

#[derive(Debug, Clone)]
pub struct LatencySummary {
    pub min: u64,
    pub p50: u64,
    pub p99: u64,
    pub max: u64,
}

fn summarize(samples: &[u64]) -> LatencySummary {
    let mut s = samples.to_vec();
    s.sort_unstable();
    let n = s.len().max(1);
    LatencySummary {
        min: *s.first().unwrap_or(&0),
        p50: s[n / 2],
        p99: s[(((n as f64) * 0.99) as usize).min(n - 1)],
        max: *s.last().unwrap_or(&0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_add_session_local() {
        let mut s = CudaSession::new("t".into(), 64 * 1024 * 1024, vec![CudaDeviceInfo {
            index: 0,
            name: "test".into(),
            vram_total_mb: 8192,
            vram_free_mb: 7000,
            compute_capability: Some("8.9".into()),
        }]);
        let (ok, _, p) = s.exec(CudaOpKind::Malloc { bytes: 16 });
        assert!(ok);
        let a = p.ptr.unwrap();
        let (ok, _, p) = s.exec(CudaOpKind::Malloc { bytes: 16 });
        assert!(ok);
        let b = p.ptr.unwrap();
        let (ok, _, p) = s.exec(CudaOpKind::Malloc { bytes: 16 });
        assert!(ok);
        let out = p.ptr.unwrap();

        let mut ha = Vec::new();
        let mut hb = Vec::new();
        for i in 0..4u32 {
            ha.extend_from_slice(&(i as f32).to_le_bytes());
            hb.extend_from_slice(&((i * 10) as f32).to_le_bytes());
        }
        assert!(s.exec(CudaOpKind::MemcpyHtoD { dst: a, data: ha }).0);
        assert!(s.exec(CudaOpKind::MemcpyHtoD { dst: b, data: hb }).0);
        assert!(s
            .exec(CudaOpKind::VectorAddF32 {
                a,
                b,
                out,
                n: 4
            })
            .0);
        let (ok, _, p) = s.exec(CudaOpKind::MemcpyDtoH { src: out, bytes: 16 });
        assert!(ok);
        let data = p.data.unwrap();
        let v0 = f32::from_le_bytes(data[0..4].try_into().unwrap());
        assert!((v0 - 0.0).abs() < 1e-5);
        let v3 = f32::from_le_bytes(data[12..16].try_into().unwrap());
        assert!((v3 - 33.0).abs() < 1e-5);
    }
}
