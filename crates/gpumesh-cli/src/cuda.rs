//! `gpumesh cuda` — R2 CUDA remoting spike (LAN-oriented).

use anyhow::{bail, Result};
use gpumesh_core::{run_bench, run_demo, MeshNode};
use gpumesh_gpu::GpuMonitor;

use crate::ui;

#[derive(Debug, clap::Subcommand)]
pub enum CudaCmd {
    /// Share this node's GPU for CUDA remoting (leave running)
    Share,
    /// Allow a paired peer to open CUDA remoting sessions
    Allow { peer: String },
    /// Run the end-to-end vector-add remoting demo against a peer
    Demo {
        #[arg(long, env = "GPUMESH_PEER")]
        peer: String,
        /// Number of f32 elements (default 262144 = 1 MiB)
        #[arg(long, default_value_t = 262_144)]
        n: u32,
    },
    /// Measure remoting round-trip latency (Sync + 4KiB HtoD)
    Bench {
        #[arg(long, env = "GPUMESH_PEER")]
        peer: String,
        #[arg(long, default_value_t = 50)]
        iters: u32,
    },
    /// Check local GPU + remoting readiness
    Doctor,
}

pub async fn dispatch(cmd: CudaCmd) -> Result<()> {
    match cmd {
        CudaCmd::Share => share().await,
        CudaCmd::Allow { peer } => allow(peer).await,
        CudaCmd::Demo { peer, n } => demo(peer, n).await,
        CudaCmd::Bench { peer, iters } => bench(peer, iters).await,
        CudaCmd::Doctor => doctor().await,
    }
}

async fn allow(peer: String) -> Result<()> {
    ui::print_banner();
    let node = MeshNode::bootstrap().await?;
    node.allow_gpu_remote_peer(&peer).await?;
    ui::ok(format!("CUDA remoting allowed for {peer}"));
    ui::dim("Does NOT grant job allow — use `gpumesh allow` / `desktop allow` separately.");
    ui::dim(format!("Peer can: gpumesh cuda demo --peer <you>"));
    Ok(())
}

async fn doctor() -> Result<()> {
    ui::print_banner();
    ui::section("CUDA remoting doctor (R2)");
    ui::warn("LAN-oriented spike — WAN Runtime remoting is usually too slow.");

    let gpus = GpuMonitor::detect().unwrap_or_default();
    ui::check_line(
        "nvidia-gpu",
        !gpus.is_empty(),
        if gpus.is_empty() {
            "no GPU via NVML/nvidia-smi"
        } else {
            gpus.first().map(|g| g.name.as_str()).unwrap_or("ok")
        },
    );
    if let Some(g) = gpus.first() {
        ui::kv(
            "VRAM",
            format!("{} / {} MB free/total", g.vram_free_mb, g.vram_total_mb),
        );
        if let Some(cc) = &g.compute_capability {
            ui::kv("CC", cc);
        }
    }

    let init = gpumesh_storage::StateStore::is_initialized();
    ui::check_line("identity", init, if init { "ok" } else { "run gpumesh init" });

    let cfg = gpumesh_storage::StateStore::load_config().unwrap_or_default();
    ui::check_line(
        "cuda-share-cfg",
        cfg.cuda_remote_sharing,
        if cfg.cuda_remote_sharing {
            "cuda_remote_sharing=true (or enable via `gpumesh cuda share`)"
        } else {
            "off until `gpumesh cuda share`"
        },
    );

    ui::section("Backend");
    ui::info("Spike backend: host-memory + NVML device identity");
    ui::dim("Device buffers live on the host; ops are remoted over authenticated QUIC.");
    ui::dim("Not a drop-in libcuda for arbitrary PyTorch/CUDA apps (see docs/cuda-remote.md).");
    Ok(())
}

async fn demo(peer: String, n: u32) -> Result<()> {
    ui::print_banner();
    ui::warn("CUDA remoting spike — process API is remoted; not a local .exe using peer CUDA ICD.");
    if n == 0 || n > 16_777_216 {
        bail!("--n must be between 1 and 16777216");
    }

    let mut node = MeshNode::bootstrap().await?;
    node.start_network().await?;
    ui::info(format!("Opening remoting session to {peer}…"));
    let report = run_demo(&node, &peer, n).await?;

    ui::section("Demo result");
    ui::kv("Peer", &report.peer);
    ui::kv("Session", &report.session_id);
    ui::kv("Backend", &report.backend);
    ui::kv("Devices", report.device_count.to_string());
    if let Some(d) = report.devices.first() {
        ui::kv("GPU", format!("{} ({} MB free)", d.name, d.vram_free_mb));
    }
    ui::kv("Elements", report.n.to_string());
    if report.verified {
        ui::ok("Vector add verified");
    } else {
        ui::err("Vector add verification FAILED");
    }
    ui::section("Latency (host op + network RTT)");
    ui::kv("open+count", format!("{} µs", report.open_us));
    ui::kv("HtoD a", format!("{} µs", report.htod_a_us));
    ui::kv("HtoD b", format!("{} µs", report.htod_b_us));
    ui::kv("vector_add", format!("{} µs", report.kernel_us));
    ui::kv("DtoH", format!("{} µs", report.dtoh_us));
    ui::kv("sync", format!("{} µs", report.sync_us));
    ui::dim(&report.lan_warning);
    if !report.verified {
        std::process::exit(1);
    }
    Ok(())
}

async fn bench(peer: String, iters: u32) -> Result<()> {
    ui::print_banner();
    if iters == 0 || iters > 10_000 {
        bail!("--iters must be 1..=10000");
    }
    let mut node = MeshNode::bootstrap().await?;
    node.start_network().await?;
    ui::info(format!("Benchmarking remoting to {peer} ({iters} iters)…"));
    let report = run_bench(&node, &peer, iters).await?;

    ui::section("Bench");
    ui::kv("Peer", &report.peer);
    ui::kv("Backend", &report.backend);
    ui::kv("Iters", report.iters.to_string());
    ui::section("Sync RTT (µs)");
    print_latency(&report.sync_us);
    ui::section("Memcpy HtoD 4KiB RTT (µs)");
    print_latency(&report.memcpy_4k_us);
    ui::dim(&report.lan_warning);
    ui::dim("Compare mentally to local cudaMemcpy / gpumesh run job overhead.");
    Ok(())
}

fn print_latency(s: &gpumesh_core::LatencySummary) {
    ui::kv("min", s.min.to_string());
    ui::kv("p50", s.p50.to_string());
    ui::kv("p99", s.p99.to_string());
    ui::kv("max", s.max.to_string());
}

async fn share() -> Result<()> {
    ui::print_banner();
    ui::warn("R2 CUDA remoting is LAN-oriented. Prefer same Wi‑Fi / wired LAN.");
    let mut node = MeshNode::bootstrap().await?;
    node.start_network().await?;
    node.enable_cuda_remote_share().await?;

    let gpus = MeshNode::detect_gpus().unwrap_or_default();
    if gpus.is_empty() {
        bail!("No NVIDIA GPU detected — cannot share CUDA remoting.");
    }
    ui::ok(format!(
        "CUDA remoting share enabled — {}",
        gpus.first().map(|g| g.name.as_str()).unwrap_or("GPU")
    ));
    ui::dim("Backend: host-memory + NVML (spike)");
    ui::dim("Allow a client:  gpumesh cuda allow <peer>");
    ui::dim("Client runs:     gpumesh cuda demo --peer <your-name>");
    if let Ok(code) = node.pairing_code().await {
        println!();
        ui::dim("Pairing code:");
        println!("{code}");
    }

    let _ = std::fs::create_dir_all(gpumesh_common::logs_dir());
    if let Some(parent) = gpumesh_common::share_pid_path().parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::remove_file(gpumesh_common::share_stop_path());
    let _ = std::fs::write(
        gpumesh_common::share_pid_path(),
        format!("{}\n", std::process::id()),
    );

    let node = std::sync::Arc::new(node);
    let endpoint = node.endpoint()?;
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        tokio::select! {
            _ = tick.tick() => {
                if gpumesh_common::share_stop_path().exists() {
                    ui::ok("CUDA remoting share stopped.");
                    let _ = std::fs::remove_file(gpumesh_common::share_pid_path());
                    let _ = std::fs::remove_file(gpumesh_common::share_stop_path());
                    return Ok(());
                }
            }
            accepted = endpoint.accept() => {
                match accepted {
                    Ok(conn) => {
                        let node = node.clone();
                        tokio::spawn(async move {
                            if let Err(e) = node.handle_inbound(conn).await {
                                tracing::error!("session error: {e}");
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("accept error: {e}");
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    }
                }
            }
        }
    }
}
