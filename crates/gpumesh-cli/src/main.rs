//! GPUMesh CLI — Claude/Gemini-style developer experience (Phases 0–4).

mod commands;
mod desktop;
mod doctor;
mod group;
mod jobfile;
mod start;
mod ui;
mod update;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(
    name = "gpumesh",
    author = "GPUMesh",
    version,
    about = "GPUMesh — turn idle GPUs into a personal compute network",
    long_about = "GPUMesh is a CLI-first P2P GPU sharing tool.\n\
Share your idle NVIDIA GPU with trusted peers, or run workloads on theirs —\n\
no SSH, VPN, or port forwarding required.",
    after_help = ui::AFTER_HELP,
    styles = ui::styles(),
    propagate_version = true,
    arg_required_else_help = true,
    disable_help_subcommand = false
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    /// Create local identity and config
    Init {
        /// Node display name
        #[arg(long, env = "GPUMESH_NODE_NAME")]
        name: Option<String>,
    },
    /// Show node, GPU, and network status
    Status,
    /// Show detailed GPU inventory
    Gpu,
    /// Diagnose local setup (Docker, NVIDIA, identity, network)
    Doctor,
    /// Interactive Claude-style menu (arrow keys)
    Start,
    /// Share this node's GPU (starts accept loop)
    Share {
        #[arg(long)]
        max_vram: Option<String>,
        #[arg(long)]
        max_gpu_utilization: Option<u8>,
        /// Publish GPU metadata to the public registry (Phase 7). Does not open allowlist.
        #[arg(long)]
        public: bool,
        /// Region label for public search (e.g. us-west)
        #[arg(long, env = "GPUMESH_REGION")]
        region: Option<String>,
        #[command(subcommand)]
        action: Option<ShareAction>,
    },
    /// Search the public GPU registry (Phase 7)
    Search {
        /// Substring match on GPU model (e.g. 4090, 5060, A100)
        #[arg(long)]
        gpu: Option<String>,
        /// Minimum VRAM (e.g. 8GB or 8192)
        #[arg(long)]
        vram: Option<String>,
        /// CUDA version substring
        #[arg(long)]
        cuda: Option<String>,
        /// Region substring
        #[arg(long, env = "GPUMESH_REGION")]
        region: Option<String>,
        /// Only idle / available nodes
        #[arg(long)]
        idle: bool,
        /// JSON output
        #[arg(long)]
        json: bool,
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
    /// Interactive GPU desktop (RDP/VNC tunnel)
    Desktop {
        #[command(subcommand)]
        action: desktop::DesktopCmd,
    },
    /// Manage private GPU clusters (Phase 5)
    Group {
        #[command(subcommand)]
        action: group::GroupCmd,
    },
    /// Run a command locally or on a peer GPU
    Run {
        #[arg(long, env = "GPUMESH_PEER")]
        peer: Option<String>,
        /// Schedule within a private group (Phase 5)
        #[arg(long)]
        group: Option<String>,
        /// Minimum GPU memory required (e.g. 20GB) — scheduler picks a peer
        #[arg(long)]
        gpu_memory: Option<String>,
        #[arg(long, env = "GPUMESH_IMAGE")]
        image: Option<String>,
        #[arg(long, value_parser = parse_env)]
        env: Vec<(String, String)>,
        #[arg(long, default_value = ".")]
        workdir: String,
        /// YAML job definition (Phase 4)
        #[arg(long, short = 'f')]
        file: Option<String>,
        /// Retry failed runs N times
        #[arg(long, default_value_t = 0)]
        retries: u32,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Sync local metadata to the control plane / dashboard API
    Sync,
    /// Print dashboard URL / how to open GPUMesh Cloud UI
    Dashboard,
    /// List recent jobs
    Jobs {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Show logs for a job or the agent
    Logs {
        /// Job id (omit for agent log)
        job_id: Option<String>,
        #[arg(long, short = 'f')]
        follow: bool,
    },
    /// Cancel a running job on a peer
    Cancel {
        #[arg(long, env = "GPUMESH_PEER")]
        peer: Option<String>,
        job_id: String,
    },
    /// Get or set configuration
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
    /// Check for / apply CLI updates
    Update {
        /// Only check, do not download
        #[arg(long)]
        check: bool,
    },
    /// Generate shell completions
    Completion {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Copy files to/from a peer
    Cp { src: String, dst: String },
    /// Isolated workload shell on a peer (not host SSH)
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
        #[arg(long)]
        public: bool,
        #[arg(long, env = "GPUMESH_REGION")]
        region: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum ShareAction {
    /// Stop sharing
    Stop,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ConfigAction {
    /// Print full config
    Show,
    /// Get a single key
    Get { key: String },
    /// Set a key
    Set { key: String, value: String },
    /// Print config path
    Path,
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
    let filter = std::env::var("GPUMESH_LOG")
        .or_else(|_| std::env::var("RUST_LOG"))
        .unwrap_or_else(|_| "warn".into());
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(filter))
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Start => start::run().await,
        other => commands::dispatch(other).await,
    };
    if let Err(e) = result {
        ui::err(e.to_string());
        std::process::exit(1);
    }
}
