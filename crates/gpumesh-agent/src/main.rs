//! GPUMesh provider agent — accepts P2P sessions and runs sandboxed GPU jobs.

use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use gpumesh_core::MeshNode;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "gpumesh-agent", about = "GPUMesh provider agent")]
struct Args {
    /// Enable sharing immediately on start.
    #[arg(long)]
    share: bool,

    /// Max VRAM to expose (e.g. 16GB).
    #[arg(long)]
    max_vram: Option<String>,

    /// Max GPU utilization percent.
    #[arg(long)]
    max_gpu_utilization: Option<u8>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let args = Args::parse();
    let mut node = MeshNode::bootstrap()
        .await
        .context("agent requires `gpumesh init` first")?;
    node.start_network().await?;

    if args.share {
        node.enable_share(args.max_vram, args.max_gpu_utilization)
            .await?;
        print_share_banner(&node).await;
    }

    let node = Arc::new(node);
    let endpoint = node.endpoint()?;
    info!(
        "gpumesh-agent listening on {:?}; node_id={}",
        endpoint.listen_addr, node.identity.node_id
    );

    loop {
        match endpoint.accept().await {
            Ok(conn) => {
                let node = node.clone();
                tokio::spawn(async move {
                    if let Err(e) = node.handle_inbound(conn).await {
                        error!("session error: {e}");
                    }
                });
            }
            Err(e) => {
                error!("accept error: {e}");
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }
    }
}

async fn print_share_banner(node: &MeshNode) {
    let cfg = node.config.read().await.clone();
    let gpus = MeshNode::detect_gpus().unwrap_or_default();
    println!("GPUMesh\n");
    if let Some(g) = gpus.first() {
        println!("GPU: {}", g.name);
        println!("VRAM: {} GB", (g.vram_total_mb as f64 / 1024.0).round());
        let avail = cfg.max_vram_mb.unwrap_or(g.vram_free_mb);
        println!("Available: {} GB", (avail as f64 / 1024.0).round());
    } else {
        println!("GPU: (none detected)");
    }
    println!("\nSharing enabled.\nWaiting for authorized peers...");
}
