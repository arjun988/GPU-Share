//! `gpumesh doctor` — local environment diagnostics.

use anyhow::Result;
use gpumesh_core::MeshNode;
use gpumesh_runtime::DockerRuntime;
use gpumesh_storage::StateStore;

use crate::ui;

pub async fn run() -> Result<()> {
    ui::print_banner();
    ui::section("Doctor");
    let mut failures = 0u32;

    // Identity
    let init_ok = StateStore::is_initialized();
    ui::check_line(
        "identity",
        init_ok,
        if init_ok {
            "gpumesh init complete"
        } else {
            "run: gpumesh init"
        },
    );
    if !init_ok {
        failures += 1;
    }

    // Config dir
    let cfg = gpumesh_common::config_dir();
    ui::check_line("config", cfg.exists(), &cfg.display().to_string());

    // Docker
    let docker_ok = DockerRuntime::ensure_docker().await.is_ok();
    ui::check_line(
        "docker",
        docker_ok,
        if docker_ok {
            "docker available"
        } else {
            "install Docker + start daemon"
        },
    );
    if !docker_ok {
        failures += 1;
    }

    // GPU
    let gpus = MeshNode::detect_gpus().unwrap_or_default();
    let gpu_ok = !gpus.is_empty();
    ui::check_line(
        "nvidia-gpu",
        gpu_ok,
        if gpu_ok {
            gpus.first().map(|g| g.name.as_str()).unwrap_or("ok")
        } else {
            "no GPU via NVML/nvidia-smi (required to share)"
        },
    );
    // GPU is warning for consumers, fail only if we want strict — treat as warn
    if !gpu_ok {
        ui::warn("No NVIDIA GPU detected — you can still consume remote GPUs.");
    }

    // NVIDIA container toolkit (best-effort)
    let toolkit = std::process::Command::new("docker")
        .args(["info"])
        .output()
        .ok()
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            Some(s.to_lowercase().contains("nvidia") || s.contains("Runtimes"))
        })
        .unwrap_or(false);
    ui::check_line(
        "gpu-runtime",
        toolkit || !docker_ok,
        if toolkit {
            "docker reports GPU-capable runtime hints"
        } else {
            "ensure NVIDIA Container Toolkit is installed for GPU jobs"
        },
    );

    // Listen port configured (QUIC uses UDP; presence check only)
    if init_ok {
        let node = MeshNode::bootstrap().await?;
        let port = node.config.read().await.listen_port;
        ui::check_line(
            "listen-port",
            port > 0,
            &format!("configured UDP/QUIC port {port}"),
        );
    }

    ui::section("CUDA remoting (R2)");
    ui::dim("LAN spike: gpumesh cuda doctor | cuda share | cuda demo --peer <name>");
    ui::warn("Not a drop-in libcuda — see docs/cuda-remote.md");

    ui::section("Summary");
    if failures == 0 {
        ui::ok("Environment looks ready.");
    } else {
        ui::err(format!("{failures} critical check(s) failed."));
        std::process::exit(1);
    }
    Ok(())
}
