//! `gpumesh app` — R1 hybrid launcher (sync / run on peer / pull outputs).
//!
//! Honest UX: the process runs **on the peer**, next to their GPU. This is not
//! CUDA remoting of a local `.exe`.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use gpumesh_common::{parse_size_to_mb, JobState};
use gpumesh_core::{
    pull_job_outputs, run_remote_job, schedule_peer, transfer_file_from_peer, transfer_file_to_peer,
    MeshNode, ScheduleRequest,
};
use gpumesh_storage::{package_workdir, unpack_archive, JobRecord};

use crate::ui;

#[derive(Debug, clap::Subcommand)]
pub enum AppCmd {
    /// Upload a local project directory to a peer
    Sync {
        #[arg(long, env = "GPUMESH_PEER")]
        peer: String,
        /// Local project directory
        #[arg(long, default_value = ".")]
        dir: String,
        /// Remote package name (default: directory name)
        #[arg(long)]
        name: Option<String>,
    },
    /// Package the project and run the command on the peer GPU
    Run {
        #[arg(long, env = "GPUMESH_PEER")]
        peer: Option<String>,
        /// Schedule within a private group
        #[arg(long)]
        group: Option<String>,
        /// Minimum GPU memory required (e.g. 8GB)
        #[arg(long)]
        gpu_memory: Option<String>,
        #[arg(long, env = "GPUMESH_IMAGE")]
        image: Option<String>,
        #[arg(long, value_parser = parse_env)]
        env: Vec<(String, String)>,
        /// Local project directory (packaged and uploaded as the job workspace)
        #[arg(long, default_value = ".")]
        dir: String,
        /// Unpack workspace outputs here after the job
        #[arg(long)]
        out: Option<String>,
        /// Print how to attach a GPU desktop session (the app still runs on the peer)
        #[arg(long)]
        desktop: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Fetch job outputs or a remote file from a peer
    Pull {
        #[arg(long, env = "GPUMESH_PEER")]
        peer: String,
        /// Job id (default: latest job for this peer)
        #[arg(long)]
        job: Option<String>,
        /// Remote path under the peer work dir (e.g. apps/proj.gpk). If set, job outputs are skipped.
        #[arg(long)]
        remote: Option<String>,
        /// Destination directory
        #[arg(long, default_value = "./gpumesh-out")]
        dir: String,
    },
}

fn parse_env(s: &str) -> Result<(String, String), String> {
    let (k, v) = s
        .split_once('=')
        .ok_or_else(|| format!("expected KEY=VAL, got {s}"))?;
    Ok((k.to_string(), v.to_string()))
}

pub async fn dispatch(cmd: AppCmd) -> Result<()> {
    match cmd {
        AppCmd::Sync { peer, dir, name } => sync(&peer, &dir, name.as_deref()).await,
        AppCmd::Run {
            peer,
            group,
            gpu_memory,
            image,
            env,
            dir,
            out,
            desktop,
            command,
        } => {
            run(
                peer,
                group,
                gpu_memory,
                image,
                env,
                &dir,
                out.as_deref(),
                desktop,
                command,
            )
            .await
        }
        AppCmd::Pull {
            peer,
            job,
            remote,
            dir,
        } => pull(&peer, job.as_deref(), remote.as_deref(), &dir).await,
    }
}

fn print_honest_banner() {
    ui::print_banner();
    ui::warn("The process runs on the peer (next to their GPU), not as a local .exe.");
    ui::dim("This is orchestrated remote execution — not CUDA/graphics remoting.");
}

fn sanitize_name(raw: &str) -> String {
    let s: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let s = s.trim_matches('_');
    if s.is_empty() {
        "project".into()
    } else {
        s.chars().take(64).collect()
    }
}

fn project_name(dir: &Path, override_name: Option<&str>) -> String {
    if let Some(n) = override_name {
        return sanitize_name(n);
    }
    dir.file_name()
        .and_then(|s| s.to_str())
        .map(sanitize_name)
        .unwrap_or_else(|| "project".into())
}

async fn sync(peer: &str, dir: &str, name: Option<&str>) -> Result<()> {
    ui::print_banner();
    sync_project(peer, dir, name).await
}

async fn sync_project(peer: &str, dir: &str, name: Option<&str>) -> Result<()> {
    let root = PathBuf::from(dir)
        .canonicalize()
        .with_context(|| format!("project dir not found: {dir}"))?;
    let name = project_name(&root, name);

    let mut node = MeshNode::bootstrap().await?;
    node.start_network().await?;

    let pack = gpumesh_common::work_dir()
        .join("packs")
        .join(format!("{name}.gpk"));
    let spinner = ui::spinner("Packaging project…");
    let manifest = package_workdir(&root, &pack)?;
    spinner.finish_and_clear();
    ui::ok(format!(
        "Packed {} files ({} bytes)",
        manifest.files.len(),
        manifest.total_bytes
    ));

    let remote = format!("apps/{name}.gpk");
    let spinner = ui::spinner(&format!("Uploading to {peer}…"));
    transfer_file_to_peer(&node, peer, &pack, &remote).await?;
    spinner.finish_and_clear();
    ui::ok(format!("Synced {name} → {peer}:{remote}"));
    ui::dim(format!(
        "Pull later: gpumesh app pull --peer {peer} --remote {remote} --dir ."
    ));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run(
    mut peer: Option<String>,
    group: Option<String>,
    gpu_memory: Option<String>,
    image: Option<String>,
    env: Vec<(String, String)>,
    dir: &str,
    out: Option<&str>,
    desktop: bool,
    command: Vec<String>,
) -> Result<()> {
    print_honest_banner();

    if command.is_empty() && !desktop {
        bail!("pass a command, or --desktop to sync and attach a GPU desktop session");
    }

    let root = PathBuf::from(dir)
        .canonicalize()
        .with_context(|| format!("project dir not found: {dir}"))?;

    if command.is_empty() && desktop {
        let peer = peer.context("pass --peer <name> (or set GPUMESH_PEER)")?;
        sync_project(&peer, dir, None).await?;
        print_desktop_hint(&peer);
        return Ok(());
    }

    let requested_gpu_memory_mb = match gpu_memory.as_deref() {
        Some(s) => Some(parse_size_to_mb(s).map_err(|e| anyhow::anyhow!("{e}"))?),
        None => None,
    };

    let mut node = MeshNode::bootstrap().await?;
    let cfg = node.config.read().await.clone();

    if group.is_some() || gpu_memory.is_some() {
        node.start_network().await?;
        let spinner = ui::spinner("Scheduling peer…");
        let chosen = schedule_peer(
            &node,
            ScheduleRequest {
                group: group.clone(),
                gpu_memory_mb: requested_gpu_memory_mb,
                prefer_peer: peer.clone(),
            },
        )
        .await?;
        spinner.finish_and_clear();
        ui::ok(format!(
            "Scheduled → {} ({})",
            chosen.peer_name,
            chosen.gpu_model.as_deref().unwrap_or("GPU")
        ));
        peer = Some(chosen.peer_name);
    }

    let peer_name = peer.context("pass --peer <name> (or --group / set GPUMESH_PEER)")?;
    node.start_network().await?;

    ui::section("Remote app");
    ui::kv("Peer", &peer_name);
    ui::kv("Dir", root.display().to_string());
    ui::kv("Command", command.join(" "));
    if let Some(img) = image.as_deref() {
        ui::kv("Image", img);
    }
    if desktop {
        print_desktop_hint(&peer_name);
    }

    let job_id = gpumesh_protocol::short_job_id();
    let mut rec = JobRecord::new(
        job_id.clone(),
        Some(peer_name.clone()),
        image
            .clone()
            .unwrap_or_else(|| cfg.default_image.clone()),
        command.clone(),
    );
    rec.state = JobState::Running;
    let _ = rec.save();

    let pull_to = out.map(PathBuf::from);
    let code = run_remote_job(
        &node,
        &peer_name,
        image,
        command,
        root,
        env,
        Some(job_id.clone()),
        requested_gpu_memory_mb,
        pull_to.clone(),
    )
    .await;

    match code {
        Ok(0) => {
            rec.state = JobState::Succeeded;
            rec.exit_code = Some(0);
            rec.finished_at = Some(Utc::now());
            let _ = rec.save();
            ui::ok(format!("App job {job_id} succeeded on {peer_name}"));
            if let Some(dest) = pull_to {
                ui::ok(format!("Outputs → {}", dest.display()));
            } else {
                ui::dim(format!(
                    "Fetch outputs: gpumesh app pull --peer {peer_name} --job {job_id}"
                ));
            }
            Ok(())
        }
        Ok(c) => {
            rec.state = JobState::Failed;
            rec.exit_code = Some(c);
            rec.finished_at = Some(Utc::now());
            let _ = rec.save();
            if let Some(dest) = pull_to {
                ui::warn(format!(
                    "Job exited {c}; partial outputs may be in {}",
                    dest.display()
                ));
            }
            std::process::exit(c);
        }
        Err(e) => {
            rec.state = JobState::Failed;
            rec.error = Some(e.to_string());
            rec.finished_at = Some(Utc::now());
            let _ = rec.save();
            Err(e.into())
        }
    }
}

fn print_desktop_hint(peer: &str) {
    ui::info(format!(
        "GUI session (app still on the host):  gpumesh desktop connect {peer}"
    ));
    ui::dim("Requires: peer running `gpumesh desktop share` and `gpumesh desktop allow <you>`.");
}

async fn pull(peer: &str, job: Option<&str>, remote: Option<&str>, dir: &str) -> Result<()> {
    ui::print_banner();
    let dest = PathBuf::from(dir);
    std::fs::create_dir_all(&dest)?;

    let mut node = MeshNode::bootstrap().await?;
    node.start_network().await?;

    if let Some(remote) = remote {
        let local = if remote.rsplit('/').next().is_some_and(|n| n.ends_with(".gpk")) {
            dest.join(".gpumesh-pull.gpk")
        } else {
            let name = Path::new(remote)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("download.bin");
            dest.join(name)
        };
        let spinner = ui::spinner(&format!("Downloading {peer}:{remote}…"));
        transfer_file_from_peer(&node, peer, remote, &local).await?;
        spinner.finish_and_clear();
        if local.extension().and_then(|e| e.to_str()) == Some("gpk")
            || remote.ends_with(".gpk")
        {
            unpack_archive(&local, &dest)?;
            let _ = std::fs::remove_file(&local);
            ui::ok(format!("Unpacked {remote} → {}", dest.display()));
        } else {
            ui::ok(format!("Downloaded {remote} → {}", local.display()));
        }
        return Ok(());
    }

    let job_id = match job {
        Some(id) => id.to_string(),
        None => latest_job_for_peer(peer)
            .with_context(|| format!("no local job history for peer {peer}; pass --job <id>"))?,
    };

    let spinner = ui::spinner(&format!("Pulling job {job_id} outputs…"));
    pull_job_outputs(&node, peer, &job_id, &dest).await?;
    spinner.finish_and_clear();
    ui::ok(format!("Outputs → {}", dest.display()));
    Ok(())
}

fn latest_job_for_peer(peer: &str) -> Option<String> {
    let jobs = JobRecord::list().ok()?;
    jobs.into_iter()
        .find(|j| j.peer.as_deref() == Some(peer))
        .map(|j| j.job_id)
}
