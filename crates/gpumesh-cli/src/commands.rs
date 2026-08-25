use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use clap::CommandFactory;
use gpumesh_common::{parse_size_to_mb, JobState, PeerStatus};
use gpumesh_core::{
    collect_status, format_peers_table, format_status, run_local_job, run_remote_job,
    schedule_peer, transfer_file_from_peer, transfer_file_to_peer, MeshNode, ScheduleRequest,
};
use gpumesh_protocol::Message;
use gpumesh_security::short_fingerprint;
use gpumesh_storage::{JobRecord, StateStore};
use tracing::error;

use crate::jobfile::JobFile;
use crate::ui;
use crate::{Commands, ConfigAction, ShareAction};

pub async fn dispatch(cmd: Commands) -> Result<()> {
    match cmd {
        Commands::Init { name } => {
            ui::print_banner();
            let (id, cfg) = MeshNode::init(name)?;
            ui::ok("Initialized GPUMesh node");
            ui::kv("Name", &cfg.node_name);
            ui::kv("Node ID", &id.node_id);
            ui::kv("Config", gpumesh_common::config_dir().display().to_string());
            ui::dim("Next: gpumesh doctor   then   gpumesh share");
            Ok(())
        }
        Commands::Status => {
            ui::print_banner();
            let node = MeshNode::bootstrap().await?;
            let view = collect_status(&node).await;
            print!("{}", format_status(&view));
            Ok(())
        }
        Commands::Gpu => {
            ui::print_banner();
            let gpus = MeshNode::detect_gpus()?;
            if gpus.is_empty() {
                ui::warn("No NVIDIA GPUs detected (NVML / nvidia-smi).");
                return Ok(());
            }
            for g in gpus {
                ui::section(&format!("[{}] {}", g.index, g.name));
                ui::kv(
                    "VRAM",
                    format!(
                        "{} / {} MB ({} free)",
                        g.vram_used_mb, g.vram_total_mb, g.vram_free_mb
                    ),
                );
                ui::kv(
                    "Util",
                    format!("{}%", g.utilization_gpu.unwrap_or(0)),
                );
                if let Some(t) = g.temperature_c {
                    ui::kv("Temp", format!("{t}°C"));
                }
                if let Some(d) = &g.driver_version {
                    ui::kv("Driver", d);
                }
                if let Some(c) = &g.cuda_version {
                    ui::kv("CUDA", c);
                }
            }
            Ok(())
        }
        Commands::Doctor => crate::doctor::run().await,
        Commands::Start => unreachable!("Start is handled in main"),
        Commands::Share {
            max_vram,
            max_gpu_utilization,
            action,
        } => match action {
            Some(ShareAction::Stop) => {
                let node = MeshNode::bootstrap().await?;
                node.disable_share().await?;
                ui::ok("Sharing stopped.");
                Ok(())
            }
            None => run_share_loop(max_vram, max_gpu_utilization).await,
        },
        Commands::PairCode => {
            ui::print_banner();
            let mut node = MeshNode::bootstrap().await?;
            node.start_network().await?;
            let code = node.pairing_code().await?;
            ui::ok("Pairing code (share out-of-band):");
            println!();
            println!("{code}");
            println!();
            ui::dim("Peer runs:  gpumesh pair <code>");
            Ok(())
        }
        Commands::Pair { code } => {
            ui::print_banner();
            let node = MeshNode::bootstrap().await?;
            let rec = node.pair_with_code(&code).await?;
            ui::ok("Pairing successful");
            ui::kv("Peer", &rec.node_name);
            if let Some(g) = &rec.gpu_model {
                ui::kv("GPU", g);
            }
            if let Some(v) = rec.vram_mb {
                ui::kv("VRAM", format!("{} GB", (v as f64 / 1024.0).round()));
            }
            ui::kv("Node ID", short_fingerprint(&rec.node_id));
            ui::dim("Tip: provider should also `gpumesh pair` your code (mutual allow).");
            Ok(())
        }
        Commands::Peers => {
            ui::print_banner();
            let mut node = MeshNode::bootstrap().await?;
            let _ = node.start_network().await;
            let store = node.peers.read().await;
            let list = store.list();
            let mut live = Vec::new();
            for p in &list {
                let status = match tokio::time::timeout(
                    Duration::from_secs(3),
                    try_probe(&node, &p.node_id),
                )
                .await
                {
                    Ok(Ok((s, gpu, vram))) => {
                        live.push((p.node_id.clone(), s, gpu, vram));
                        continue;
                    }
                    _ => PeerStatus::Offline,
                };
                live.push((
                    p.node_id.clone(),
                    status,
                    p.gpu_model.clone(),
                    p.vram_mb,
                ));
            }
            print!("{}", format_peers_table(&list, &live));
            Ok(())
        }
        Commands::Connect { peer } => {
            ui::print_banner();
            let spinner = ui::spinner(&format!("Connecting to {peer}…"));
            let mut node = MeshNode::bootstrap().await?;
            node.start_network().await?;
            let conn = node.connect_peer(&peer).await?;
            spinner.finish_and_clear();
            ui::ok(format!(
                "Connected to {} ({})",
                conn.peer_name.as_deref().unwrap_or(&peer),
                conn.remote_addr
            ));
            ui::kv("Mode", format!("{:?}", conn.connection_mode));
            conn.close();
            Ok(())
        }
        Commands::Allow { peer } => {
            let node = MeshNode::bootstrap().await?;
            node.allow_peer(&peer).await?;
            ui::ok(format!("Allowed {peer}"));
            Ok(())
        }
        Commands::Deny { peer } => {
            let node = MeshNode::bootstrap().await?;
            node.deny_peer(&peer).await?;
            ui::ok(format!("Denied {peer}"));
            Ok(())
        }
        Commands::Group { action } => crate::group::dispatch(action).await,
        Commands::Sync => sync_to_control_plane().await,
        Commands::Dashboard => {
            ui::print_banner();
            ui::section("GPUMesh Cloud Dashboard");
            let cfg = StateStore::load_config().unwrap_or_default();
            let api = cfg
                .rendezvous_url
                .unwrap_or_else(|| "http://127.0.0.1:8080".into());
            ui::kv("API", &api);
            ui::kv("UI", "http://127.0.0.1:3000");
            println!();
            ui::dim("Start control plane:  cd services/control-plane && go run .");
            ui::dim("Start dashboard:      cd dashboard && npm install && npm run dev");
            ui::dim("Then:                 gpumesh sync");
            Ok(())
        }
        Commands::Run {
            peer,
            group,
            gpu_memory,
            image,
            env,
            workdir,
            file,
            retries,
            command,
        } => {
            let mut peer = peer;
            let mut group = group;
            let mut gpu_memory = gpu_memory;
            let mut image = image;
            let mut env = env;
            let mut workdir = workdir;
            let mut command = command;
            let mut retries = retries;

            if let Some(path) = file {
                let job = JobFile::load(PathBuf::from(&path).as_path())?;
                if peer.is_none() {
                    peer = job.peer.clone();
                }
                if group.is_none() {
                    group = job.group.clone();
                }
                if gpu_memory.is_none() {
                    gpu_memory = job.gpu_memory.clone();
                }
                if image.is_none() {
                    image = job.image.clone();
                }
                if command.is_empty() {
                    command = job.command.clone();
                }
                if workdir == "." {
                    workdir = job.workdir.clone();
                }
                if env.is_empty() {
                    env = job.env_pairs();
                }
                if retries == 0 && job.retries > 0 {
                    retries = job.retries;
                }
                if let Some(name) = &job.name {
                    ui::info(format!("Job file: {name}"));
                }
            }

            if command.is_empty() {
                bail!("command required (or pass --file job.yaml)");
            }

            let cfg = StateStore::load_config().unwrap_or_default();
            if retries == 0 {
                retries = cfg.default_retries;
            }

            let mut node = MeshNode::bootstrap().await?;
            let workdir_path = PathBuf::from(&workdir);

            if group.is_some() || gpu_memory.is_some() {
                node.start_network().await?;
                let mem_mb = match &gpu_memory {
                    Some(s) => Some(parse_size_to_mb(s)?),
                    None => None,
                };
                let spinner = ui::spinner("Scheduling peer…");
                let chosen = schedule_peer(
                    &node,
                    ScheduleRequest {
                        group: group.clone(),
                        gpu_memory_mb: mem_mb,
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
                if let Some(free) = chosen.vram_free_mb {
                    ui::kv("Free VRAM", format!("{free} MB"));
                }
                peer = Some(chosen.peer_name);
            }

            let mut last_err = None;
            let attempts = retries + 1;
            for attempt in 1..=attempts {
                if attempt > 1 {
                    ui::warn(format!("Retry {attempt}/{attempts}…"));
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                let job_id_hint = gpumesh_protocol::short_job_id();
                let mut rec = JobRecord::new(
                    job_id_hint.clone(),
                    peer.clone(),
                    image.clone().unwrap_or_else(|| cfg.default_image.clone()),
                    command.clone(),
                );
                rec.attempts = attempt;
                rec.state = JobState::Running;
                let _ = rec.save();

                let result = if let Some(ref peer_name) = peer {
                    node.start_network().await?;
                    run_remote_job(
                        &node,
                        peer_name,
                        image.clone(),
                        command.clone(),
                        workdir_path.clone(),
                        env.clone(),
                    )
                    .await
                } else {
                    run_local_job(
                        &node,
                        image.clone(),
                        command.clone(),
                        workdir_path.clone(),
                        env.clone(),
                    )
                    .await
                };

                match result {
                    Ok(code) => {
                        rec.state = if code == 0 {
                            JobState::Succeeded
                        } else {
                            JobState::Failed
                        };
                        rec.exit_code = Some(code);
                        rec.finished_at = Some(Utc::now());
                        let _ = rec.save();
                        if code != 0 {
                            if attempt < attempts {
                                last_err = Some(format!("exit code {code}"));
                                continue;
                            }
                            std::process::exit(code);
                        }
                        return Ok(());
                    }
                    Err(e) => {
                        rec.state = JobState::Failed;
                        rec.error = Some(e.to_string());
                        rec.finished_at = Some(Utc::now());
                        let _ = rec.save();
                        last_err = Some(e.to_string());
                        if attempt >= attempts {
                            bail!("{}", last_err.unwrap());
                        }
                    }
                }
            }
            bail!("{}", last_err.unwrap_or_else(|| "job failed".into()))
        }
        Commands::Jobs { limit } => {
            ui::print_banner();
            ui::section("Jobs");
            let jobs = JobRecord::list()?;
            if jobs.is_empty() {
                ui::dim("No jobs yet. Run: gpumesh run --peer <name> …");
                return Ok(());
            }
            println!(
                "  {:<8} {:<10} {:<16} {:<12} {}",
                "ID", "STATE", "PEER", "EXIT", "CREATED"
            );
            for j in jobs.into_iter().take(limit) {
                println!(
                    "  {:<8} {:<10} {:<16} {:<12} {}",
                    j.job_id,
                    j.state.to_string(),
                    j.peer.as_deref().unwrap_or("-"),
                    j.exit_code
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "-".into()),
                    j.created_at.format("%Y-%m-%d %H:%M")
                );
            }
            Ok(())
        }
        Commands::Logs { job_id, follow } => {
            if let Some(id) = job_id {
                if let Ok(rec) = JobRecord::load(&id) {
                    ui::kv("Job", &rec.job_id);
                    ui::kv("State", rec.state.to_string());
                    if let Some(e) = &rec.error {
                        ui::kv("Error", e);
                    }
                }
                let log = JobRecord::read_log(&id)?;
                if log.is_empty() {
                    ui::dim("No captured log for this job (stdout was streamed live).");
                } else {
                    print!("{log}");
                }
                if follow {
                    ui::warn("--follow not yet attached to live remote streams; use run.");
                }
            } else {
                let path = gpumesh_common::agent_log_path();
                if path.exists() {
                    let text = std::fs::read_to_string(&path)?;
                    print!("{text}");
                } else {
                    ui::dim(format!(
                        "No agent log at {} — sharing process logs to stderr.",
                        path.display()
                    ));
                }
            }
            Ok(())
        }
        Commands::Cancel { peer, job_id } => {
            let peer = peer.context("pass --peer <name> (or set GPUMESH_PEER)")?;
            let mut node = MeshNode::bootstrap().await?;
            node.start_network().await?;
            let conn = node.connect_peer(&peer).await?;
            conn.send(Message::CancelJob {
                job_id: job_id.clone(),
            })
            .await?;
            match conn.recv().await? {
                Some(Message::CancelAck { ok, .. }) => {
                    if ok {
                        ui::ok(format!("Cancelled {job_id}"));
                    } else {
                        bail!("cancel failed for {job_id}");
                    }
                }
                other => bail!("unexpected response: {other:?}"),
            }
            Ok(())
        }
        Commands::Config { action } => config_cmd(action).await,
        Commands::Update { check } => crate::update::run(check).await,
        Commands::Completion { shell } => {
            let mut cmd = crate::Cli::command();
            clap_complete::generate(shell, &mut cmd, "gpumesh", &mut std::io::stdout());
            Ok(())
        }
        Commands::Cp { src, dst } => {
            let mut node = MeshNode::bootstrap().await?;
            node.start_network().await?;
            if let Some((peer, remote)) = split_remote(&src) {
                transfer_file_from_peer(&node, peer, remote, &PathBuf::from(&dst)).await?;
                ui::ok(format!("Downloaded {src} → {dst}"));
            } else if let Some((peer, remote)) = split_remote(&dst) {
                transfer_file_to_peer(&node, peer, &PathBuf::from(&src), remote).await?;
                ui::ok(format!("Uploaded {src} → {dst}"));
            } else {
                bail!("one of src/dst must be peer:path (e.g. alice:/data/out.bin)");
            }
            Ok(())
        }
        Commands::Exec {
            peer,
            shell,
            image,
        } => {
            ui::warn("exec runs an isolated container shell (not host SSH)");
            let mut node = MeshNode::bootstrap().await?;
            node.start_network().await?;
            let code = run_remote_job(
                &node,
                &peer,
                image,
                vec![shell],
                PathBuf::from("."),
                Vec::new(),
            )
            .await?;
            if code != 0 {
                std::process::exit(code);
            }
            Ok(())
        }
        Commands::Agent {
            share,
            max_vram,
            max_gpu_utilization,
        } => run_agent(share, max_vram, max_gpu_utilization).await,
    }
}

async fn config_cmd(action: Option<ConfigAction>) -> Result<()> {
    let action = action.unwrap_or(ConfigAction::Show);
    match action {
        ConfigAction::Path => {
            println!("{}", gpumesh_common::config_path().display());
            Ok(())
        }
        ConfigAction::Show => {
            ui::print_banner();
            let cfg = StateStore::load_config()?;
            let text = toml::to_string_pretty(&cfg)?;
            print!("{text}");
            ui::dim(format!("\nPath: {}", gpumesh_common::config_path().display()));
            Ok(())
        }
        ConfigAction::Get { key } => {
            let cfg = StateStore::load_config()?;
            let val = config_get(&cfg, &key)?;
            println!("{val}");
            Ok(())
        }
        ConfigAction::Set { key, value } => {
            let mut cfg = StateStore::load_config()?;
            config_set(&mut cfg, &key, &value)?;
            StateStore::save_config(&cfg)?;
            ui::ok(format!("Set {key} = {value}"));
            Ok(())
        }
    }
}

fn config_get(cfg: &gpumesh_common::NodeConfig, key: &str) -> Result<String> {
    let v = match key {
        "node_name" => cfg.node_name.clone(),
        "listen_port" => cfg.listen_port.to_string(),
        "default_image" => cfg.default_image.clone(),
        "rendezvous_url" => cfg.rendezvous_url.clone().unwrap_or_default(),
        "max_concurrent_jobs" => cfg.max_concurrent_jobs.to_string(),
        "default_retries" => cfg.default_retries.to_string(),
        "sharing_enabled" => cfg.sharing_enabled.to_string(),
        "update_url" => cfg.update_url.clone().unwrap_or_default(),
        other => bail!("unknown key: {other}"),
    };
    Ok(v)
}

fn config_set(cfg: &mut gpumesh_common::NodeConfig, key: &str, value: &str) -> Result<()> {
    match key {
        "node_name" => cfg.node_name = value.into(),
        "listen_port" => cfg.listen_port = value.parse()?,
        "default_image" => cfg.default_image = value.into(),
        "rendezvous_url" => {
            cfg.rendezvous_url = if value.is_empty() {
                None
            } else {
                Some(value.into())
            }
        }
        "max_concurrent_jobs" => cfg.max_concurrent_jobs = value.parse()?,
        "default_retries" => cfg.default_retries = value.parse()?,
        "sharing_enabled" => cfg.sharing_enabled = value.parse()?,
        "update_url" => {
            cfg.update_url = if value.is_empty() {
                None
            } else {
                Some(value.into())
            }
        }
        "max_vram_mb" => cfg.max_vram_mb = Some(value.parse()?),
        other => bail!("unknown or read-only key: {other}"),
    }
    Ok(())
}

async fn run_share_loop(
    max_vram: Option<String>,
    max_gpu_utilization: Option<u8>,
) -> Result<()> {
    run_agent(true, max_vram, max_gpu_utilization).await
}

async fn run_agent(
    share: bool,
    max_vram: Option<String>,
    max_gpu_utilization: Option<u8>,
) -> Result<()> {
    ui::print_banner();
    let mut node = MeshNode::bootstrap()
        .await
        .context("run `gpumesh init` first")?;
    node.start_network().await?;
    if share {
        node.enable_share(max_vram, max_gpu_utilization).await?;
        let cfg = node.config.read().await.clone();
        let gpus = MeshNode::detect_gpus().unwrap_or_default();
        if let Some(g) = gpus.first() {
            ui::kv("GPU", &g.name);
            ui::kv(
                "VRAM",
                format!("{} GB", (g.vram_total_mb as f64 / 1024.0).round()),
            );
            let avail = cfg.max_vram_mb.unwrap_or(g.vram_free_mb);
            ui::kv(
                "Available",
                format!("{} GB", (avail as f64 / 1024.0).round()),
            );
        }
        ui::ok("Sharing enabled — waiting for authorized peers…");
        if let Ok(code) = node.pairing_code().await {
            println!();
            ui::dim("Pairing code:");
            println!("{code}");
        }
    }

    // Ensure logs dir
    let _ = std::fs::create_dir_all(gpumesh_common::logs_dir());

    let node = Arc::new(node);
    let endpoint = node.endpoint()?;
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
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }
}

async fn try_probe(
    node: &MeshNode,
    peer_id: &str,
) -> Result<(PeerStatus, Option<String>, Option<u64>)> {
    let conn = node.connect_peer(peer_id).await?;
    conn.send(Message::PeerInfoRequest).await?;
    let result = match conn.recv().await? {
        Some(Message::PeerInfo(info)) => Ok((info.status, info.gpu_model, info.vram_total_mb)),
        _ => Ok((PeerStatus::Unknown, None, None)),
    };
    conn.close();
    result
}

fn split_remote(s: &str) -> Option<(&str, &str)> {
    let (peer, path) = s.split_once(':')?;
    if peer.len() == 1 && peer.chars().next()?.is_ascii_alphabetic() {
        return None;
    }
    if peer.is_empty() || path.is_empty() {
        return None;
    }
    Some((peer, path))
}

async fn sync_to_control_plane() -> Result<()> {
    ui::print_banner();
    let node = MeshNode::bootstrap().await?;
    let cfg = node.config.read().await.clone();
    let base = cfg
        .rendezvous_url
        .clone()
        .unwrap_or_else(|| "http://127.0.0.1:8080".into());
    let gpus = MeshNode::detect_gpus().unwrap_or_default();
    let peers = {
        let store = node.peers.read().await;
        store.list().into_iter().cloned().collect::<Vec<_>>()
    };
    let jobs = JobRecord::list().unwrap_or_default();
    let groups = gpumesh_storage::GroupStore::load()
        .map(|s| {
            s.list()
                .into_iter()
                .map(|g| {
                    serde_json::json!({
                        "id": g.id,
                        "name": g.name,
                        "members": g.members.len(),
                        "owner_node_id": g.owner_node_id,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let payload = serde_json::json!({
        "node": {
            "node_id": node.identity.node_id,
            "node_name": cfg.node_name,
            "public_key_hex": node.identity.public_key_hex(),
            "sharing": cfg.sharing_enabled,
            "addrs": [],
            "gpu_model": gpus.first().map(|g| g.name.clone()),
            "vram_mb": gpus.first().map(|g| g.vram_total_mb),
            "vram_free_mb": gpus.first().map(|g| g.vram_free_mb),
            "utilization": gpus.first().and_then(|g| g.utilization_gpu),
        },
        "gpus": gpus.iter().map(|g| serde_json::json!({
            "index": g.index,
            "name": g.name,
            "vram_total_mb": g.vram_total_mb,
            "vram_used_mb": g.vram_used_mb,
            "vram_free_mb": g.vram_free_mb,
            "utilization": g.utilization_gpu,
            "temperature_c": g.temperature_c,
        })).collect::<Vec<_>>(),
        "peers": peers.iter().map(|p| serde_json::json!({
            "node_id": p.node_id,
            "node_name": p.node_name,
            "gpu_model": p.gpu_model,
            "vram_mb": p.vram_mb,
        })).collect::<Vec<_>>(),
        "jobs": jobs.iter().take(50).map(|j| serde_json::json!({
            "job_id": j.job_id,
            "peer": j.peer,
            "state": j.state.to_string(),
            "exit_code": j.exit_code,
            "image": j.image,
            "created_at": j.created_at.to_rfc3339(),
        })).collect::<Vec<_>>(),
        "groups": groups,
    });

    let url = format!("{}/v1/sync", base.trim_end_matches('/'));
    let spinner = ui::spinner(&format!("Syncing to {url}…"));
    let client = reqwest::Client::new();
    let resp = client.post(&url).json(&payload).send().await;
    spinner.finish_and_clear();
    match resp {
        Ok(r) if r.status().is_success() => {
            ui::ok("Synced metadata to control plane");
            ui::dim("Open dashboard at http://127.0.0.1:3000");
        }
        Ok(r) => bail!("sync failed: HTTP {}", r.status()),
        Err(e) => bail!("sync failed: {e} — is the control plane running?"),
    }
    Ok(())
}
