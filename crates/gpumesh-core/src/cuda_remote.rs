//! R2 CUDA remoting spike — authenticated QUIC session with a Runtime-API subset.
//!
//! Honest scope: this remotes a **small** op set over GPUMesh pairing. Device buffers in
//! the `host-memory` backend live in host RAM; device *identity* comes from NVML when
//! present. This is **not** a drop-in `libcuda` replacement for arbitrary apps.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use gpumesh_common::{GpuMeshError, Result};
use gpumesh_gpu::GpuMonitor;
use gpumesh_network::PeerConnection;
use gpumesh_protocol::{CudaDeviceInfo, CudaOpKind, Message};
use tracing::{info, warn};
use uuid::Uuid;

use crate::MeshNode;

pub const CUDA_REMOTE_CLIENT_VER: u32 = 1;
pub const CUDA_REMOTE_API: &str = "cuda";

const DEFAULT_MAX_ALLOC: u64 = 512 * 1024 * 1024;
const MAX_SINGLE_ALLOC: u64 = 256 * 1024 * 1024;
const MAX_MEMCPY: u64 = 64 * 1024 * 1024;

pub(crate) struct CudaSession {
    #[allow(dead_code)]
    session_id: String,
    max_alloc: u64,
    used: u64,
    next_ptr: u64,
    buffers: HashMap<u64, Vec<u8>>,
    devices: Vec<CudaDeviceInfo>,
    backend: String,
}

impl CudaSession {
    fn new(session_id: String, max_alloc: u64, devices: Vec<CudaDeviceInfo>) -> Self {
        Self {
            session_id,
            max_alloc,
            used: 0,
            next_ptr: 0x1000, // opaque non-null style ids
            buffers: HashMap::new(),
            devices,
            backend: "host-memory".into(),
        }
    }

    fn exec(&mut self, op: CudaOpKind) -> (bool, Option<String>, CudaPartial) {
        let mut partial = CudaPartial::default();
        match op {
            CudaOpKind::DeviceCount => {
                partial.device_count = Some(self.devices.len() as u32);
                (true, None, partial)
            }
            CudaOpKind::DeviceProps { device } => match self.devices.get(device as usize) {
                Some(d) => {
                    partial.device = Some(d.clone());
                    (true, None, partial)
                }
                None => (false, Some(format!("invalid device index {device}")), partial),
            },
            CudaOpKind::Malloc { bytes } => {
                if bytes == 0 {
                    return (false, Some("cudaMalloc size 0".into()), partial);
                }
                if bytes > MAX_SINGLE_ALLOC {
                    return (
                        false,
                        Some(format!("alloc {bytes} exceeds per-buffer cap {MAX_SINGLE_ALLOC}")),
                        partial,
                    );
                }
                if self.used.saturating_add(bytes) > self.max_alloc {
                    return (
                        false,
                        Some(format!(
                            "session alloc cap exceeded (used {} + {bytes} > {})",
                            self.used, self.max_alloc
                        )),
                        partial,
                    );
                }
                let ptr = self.next_ptr;
                self.next_ptr = self.next_ptr.saturating_add(bytes.max(64)).saturating_add(64);
                self.buffers.insert(ptr, vec![0u8; bytes as usize]);
                self.used = self.used.saturating_add(bytes);
                partial.ptr = Some(ptr);
                (true, None, partial)
            }
            CudaOpKind::Free { ptr } => {
                if let Some(buf) = self.buffers.remove(&ptr) {
                    self.used = self.used.saturating_sub(buf.len() as u64);
                    (true, None, partial)
                } else {
                    (false, Some(format!("invalid device ptr {ptr:#x}")), partial)
                }
            }
            CudaOpKind::MemcpyHtoD { dst, data } => {
                if data.len() as u64 > MAX_MEMCPY {
                    return (false, Some("memcpy too large".into()), partial);
                }
                match self.buffers.get_mut(&dst) {
                    Some(buf) if data.len() <= buf.len() => {
                        buf[..data.len()].copy_from_slice(&data);
                        (true, None, partial)
                    }
                    Some(_) => (false, Some("HtoD overflows buffer".into()), partial),
                    None => (false, Some(format!("invalid dst ptr {dst:#x}")), partial),
                }
            }
            CudaOpKind::MemcpyDtoH { src, bytes } => {
                if bytes > MAX_MEMCPY {
                    return (false, Some("memcpy too large".into()), partial);
                }
                match self.buffers.get(&src) {
                    Some(buf) if (bytes as usize) <= buf.len() => {
                        partial.data = Some(buf[..bytes as usize].to_vec());
                        (true, None, partial)
                    }
                    Some(_) => (false, Some("DtoH overflows buffer".into()), partial),
                    None => (false, Some(format!("invalid src ptr {src:#x}")), partial),
                }
            }
            CudaOpKind::Memset { ptr, value, bytes } => match self.buffers.get_mut(&ptr) {
                Some(buf) if (bytes as usize) <= buf.len() => {
                    buf[..bytes as usize].fill(value);
                    (true, None, partial)
                }
                Some(_) => (false, Some("memset overflows buffer".into()), partial),
                None => (false, Some(format!("invalid ptr {ptr:#x}")), partial),
            },
            CudaOpKind::Sync => (true, None, partial),
            CudaOpKind::VectorAddF32 { a, b, out, n } => {
                let need = (n as usize).saturating_mul(4);
                let Some(ab) = self.buffers.get(&a).cloned() else {
                    return (false, Some(format!("invalid a ptr {a:#x}")), partial);
                };
                let Some(bb) = self.buffers.get(&b).cloned() else {
                    return (false, Some(format!("invalid b ptr {b:#x}")), partial);
                };
                let Some(ob) = self.buffers.get_mut(&out) else {
                    return (false, Some(format!("invalid out ptr {out:#x}")), partial);
                };
                if ab.len() < need || bb.len() < need || ob.len() < need {
                    return (false, Some("vector_add buffer too small".into()), partial);
                }
                for i in 0..n as usize {
                    let o = i * 4;
                    let x = f32::from_le_bytes(ab[o..o + 4].try_into().unwrap());
                    let y = f32::from_le_bytes(bb[o..o + 4].try_into().unwrap());
                    ob[o..o + 4].copy_from_slice(&(x + y).to_le_bytes());
                }
                (true, None, partial)
            }
        }
    }
}

#[derive(Default)]
struct CudaPartial {
    device_count: Option<u32>,
    device: Option<CudaDeviceInfo>,
    ptr: Option<u64>,
    data: Option<Vec<u8>>,
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
            reason: format!("unsupported remoting api '{api}' (spike supports 'cuda')"),
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

    // VRAM / util gates (same spirit as jobs).
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
    let backend = session.backend.clone();
    sessions.insert(session_id.clone(), session);

    conn.send(Message::GpuRemoteOffer {
        session_id: session_id.clone(),
        api: CUDA_REMOTE_API.into(),
        backend,
        max_alloc_bytes: max_alloc,
        devices,
        lan_warning: "R2 CUDA remoting is LAN-oriented; WAN latency makes Runtime remoting painful."
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
