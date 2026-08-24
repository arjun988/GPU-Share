//! GPUMesh CLI — Phases 0–3 surface.

mod commands;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(
    name = "gpumesh",
    about = "GPUMesh — P2P GPU sharing for developers",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    /// Create local identity and config (~/.gpumesh)
    Init {
        #[arg(long)]
        name: Option<String>,
    },
    /// Show node, GPU, and network status
    Status,
    /// Show detailed GPU inventory
    Gpu,
    /// Share this node's GPU (starts agent accept loop)
    Share {
        #[arg(long)]
        max_vram: Option<String>,
        #[arg(long)]
        max_gpu_utilization: Option<u8>,
        #[command(subcommand)]
        action: Option<ShareAction>,
    },
    /// Print a pairing code for others to `gpumesh pair`
    PairCode,
    /// Pair with a peer using their pairing code
    Pair { code: String },
    /// List paired peers
    Peers,
    /// Ensure/refresh a connection to a peer
    Connect { peer: String },
    /// Allow a paired peer to run jobs
    Allow { peer: String },
    /// Deny a peer
    Deny { peer: String },
    /// Run a command locally or on a peer GPU
    Run {
        #[arg(long)]
        peer: Option<String>,
        #[arg(long)]
        image: Option<String>,
        #[arg(long, value_parser = parse_env)]
        env: Vec<(String, String)>,
        #[arg(long, default_value = ".")]
        workdir: String,
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Copy files to/from a peer (`local peer:/path` or `peer:/path local`)
    Cp { src: String, dst: String },
    /// Cancel a running job on a peer
    Cancel {
        #[arg(long)]
        peer: String,
        job_id: String,
    },
    /// Isolated workload shell on a peer (containerized — not host SSH)
    Exec {
        peer: String,
        #[arg(default_value = "bash")]
        shell: String,
        #[arg(long)]
        image: Option<String>,
    },
    /// Run the provider agent daemon
    Agent {
        #[arg(long)]
        share: bool,
        #[arg(long)]
        max_vram: Option<String>,
        #[arg(long)]
        max_gpu_utilization: Option<u8>,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum ShareAction {
    /// Stop sharing
    Stop,
}

fn parse_env(s: &str) -> Result<(String, String), String> {
    let (k, v) = s
        .split_once('=')
        .ok_or_else(|| format!("expected KEY=VAL, got {s}"))?;
    Ok((k.to_string(), v.to_string()))
}

#[tokio::main]
async fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("warn".parse().unwrap()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    if let Err(e) = commands::dispatch(cli.command).await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
