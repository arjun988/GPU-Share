use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use gpumesh_common::PeerStatus;
use gpumesh_core::{
    collect_status, format_peers_table, format_status, run_local_job, run_remote_job,
    transfer_file_from_peer, transfer_file_to_peer, MeshNode,
};
use gpumesh_protocol::Message;
use gpumesh_security::short_fingerprint;
use tracing::error;

use crate::{Commands, ShareAction};

pub async fn dispatch(cmd: Commands) -> Result<()> {
    match cmd {
        Commands::Init { name } => {
            let (id, cfg) = MeshNode::init(name)?;
            println!("Initialized GPUMesh node");
            println!("  Name:    {}", cfg.node_name);
            println!("  Node ID: {}", id.node_id);
            println!("  Config:  {}", gpumesh_common::config_dir().display());
            Ok(())
        }
        Commands::Status => {
            let mut node = MeshNode::bootstrap().await?;
            // status does not require listening; show whether share flag set
            let view = collect_status(&node).await;
            print!("{}", format_status(&view));
            let _ = &mut node;
            Ok(())
        }
        Commands::Gpu => {
            let gpus = MeshNode::detect_gpus()?;
            if gpus.is_empty() {
                println!("No NVIDIA GPUs detected (NVML / nvidia-smi).");
                return Ok(());
            }
            for g in gpus {
                println!("[{}] {}", g.index, g.name);
                println!(
                    "  VRAM: {} / {} MB ({} free)",
                    g.vram_used_mb, g.vram_total_mb, g.vram_free_mb
                );
                println!(
                    "  Util: {}%  Temp: {}°C  Power: {}W",
                    g.utilization_gpu.unwrap_or(0),
                    g.temperature_c
                        .map(|t| t.to_string())
                        .unwrap_or_else(|| "-".into()),
                    g.power_watts
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| "-".into()),
                );
                if let Some(d) = &g.driver_version {
                    println!("  Driver: {d}");
                }
                if let Some(c) = &g.cuda_version {
                    println!("  CUDA: {c}");
                }
                if let Some(cc) = &g.compute_capability {
                    println!("  Compute capability: {cc}");
                }
                println!();
            }
            Ok(())
        }
        Commands::Share {
            max_vram,
            max_gpu_utilization,
            action,
        } => match action {
            Some(ShareAction::Stop) => {
                let node = MeshNode::bootstrap().await?;
                node.disable_share().await?;
                println!("Sharing stopped.");
                Ok(())
            }
            None => run_share_loop(max_vram, max_gpu_utilization).await,
        },
        Commands::PairCode => {
            let mut node = MeshNode::bootstrap().await?;
            node.start_network().await?;
            let code = node.pairing_code().await?;
            println!("Pairing code (share out-of-band):\n");
            println!("{code}");
            println!(
                "\nPeer should run:\n  gpumesh pair <code>"
            );
            // Keep process alive briefly so addr is meaningful — print and exit is OK for code gen.
            Ok(())
        }
        Commands::Pair { code } => {
            let node = MeshNode::bootstrap().await?;
            let rec = node.pair_with_code(&code).await?;
            println!("Pairing successful.\n");
            println!("Peer:");
            println!("{}", rec.node_name);
            if let Some(g) = &rec.gpu_model {
                println!("{g}");
            }
            if let Some(v) = rec.vram_mb {
                println!("{} GB VRAM", (v as f64 / 1024.0).round());
            }
            println!("Node ID: {}", short_fingerprint(&rec.node_id));
            Ok(())
        }
        Commands::Peers => {
            let mut node = MeshNode::bootstrap().await?;
            let _ = node.start_network().await;
            let store = node.peers.read().await;
            let list = store.list();
            let mut live = Vec::new();
            for p in &list {
                let status = match try_probe(&node, &p.node_id).await {
                    Ok((s, gpu, vram)) => {
                        live.push((p.node_id.clone(), s, gpu, vram));
                        continue;
                    }
                    Err(_) => PeerStatus::Offline,
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
            let mut node = MeshNode::bootstrap().await?;
            node.start_network().await?;
            let conn = node.connect_peer(&peer).await?;
            println!(
                "Connected to {} ({})",
                conn.peer_name.as_deref().unwrap_or(&peer),
                conn.remote_addr
            );
            println!("Mode: {:?}", conn.connection_mode);
            conn.close();
            Ok(())
        }
        Commands::Allow { peer } => {
            let node = MeshNode::bootstrap().await?;
            node.allow_peer(&peer).await?;
            println!("Allowed {peer}");
            Ok(())
        }
        Commands::Deny { peer } => {
            let node = MeshNode::bootstrap().await?;
            node.deny_peer(&peer).await?;
            println!("Denied {peer}");
            Ok(())
        }
        Commands::Run {
            peer,
            image,
            env,
            workdir,
            command,
        } => {
            if command.is_empty() {
                bail!("command required");
            }
            let mut node = MeshNode::bootstrap().await?;
            let workdir = PathBuf::from(workdir);
            let code = if let Some(peer) = peer {
                node.start_network().await?;
                run_remote_job(&node, &peer, image, command, workdir, env).await?
            } else {
                run_local_job(&node, image, command, workdir, env).await?
            };
            if code != 0 {
                std::process::exit(code);
            }
            Ok(())
        }
        Commands::Cp { src, dst } => {
            let mut node = MeshNode::bootstrap().await?;
            node.start_network().await?;
            if let Some((peer, remote)) = split_remote(&src) {
                // download
                transfer_file_from_peer(&node, peer, remote, &PathBuf::from(&dst)).await?;
                println!("Downloaded {src} → {dst}");
            } else if let Some((peer, remote)) = split_remote(&dst) {
                transfer_file_to_peer(&node, peer, &PathBuf::from(&src), remote).await?;
                println!("Uploaded {src} → {dst}");
            } else {
                bail!("one of src/dst must be peer:path (e.g. alice:/data/out.bin)");
            }
            Ok(())
        }
        Commands::Cancel { peer, job_id } => {
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
                        println!("Cancelled {job_id}");
                    } else {
                        bail!("cancel failed for {job_id}");
                    }
                }
                other => bail!("unexpected response: {other:?}"),
            }
            Ok(())
        }
        Commands::Exec {
            peer,
            shell,
            image,
        } => {
            // Isolated shell = containerized job with interactive-ish command.
            let mut node = MeshNode::bootstrap().await?;
            node.start_network().await?;
            println!(
                "note: exec runs an isolated container shell on the peer (not host SSH)"
            );
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
    let mut node = MeshNode::bootstrap()
        .await
        .context("run `gpumesh init` first")?;
    node.start_network().await?;
    if share {
        node.enable_share(max_vram, max_gpu_utilization).await?;
        let cfg = node.config.read().await.clone();
        let gpus = MeshNode::detect_gpus().unwrap_or_default();
        println!("GPUMesh\n");
        if let Some(g) = gpus.first() {
            println!("GPU: {}", g.name);
            println!(
                "VRAM: {} GB",
                (g.vram_total_mb as f64 / 1024.0).round()
            );
            let avail = cfg.max_vram_mb.unwrap_or(g.vram_free_mb);
            println!("Available: {} GB", (avail as f64 / 1024.0).round());
        }
        println!("\nSharing enabled.\nWaiting for authorized peers...");
        if let Ok(code) = node.pairing_code().await {
            println!("\nPairing code:\n{code}");
        }
    }

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
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }
    }
}

async fn try_probe(
    node: &MeshNode,
    peer_id: &str,
) -> Result<(PeerStatus, Option<String>, Option<u64>)> {
    let mut conn = node.connect_peer(peer_id).await?;
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
    // Avoid treating Windows drive letters as peers (C:\...)
    if peer.len() == 1 && peer.chars().next()?.is_ascii_alphabetic() {
        return None;
    }
    if peer.is_empty() || path.is_empty() {
        return None;
    }
    Some((peer, path))
}
