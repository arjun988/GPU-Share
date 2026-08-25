//! Cluster scheduler — pick an idle peer with enough free VRAM (Phase 5).

use std::time::Duration;

use gpumesh_common::{GpuMeshError, PeerStatus, Result};
use gpumesh_network::PeerConnection;
use gpumesh_protocol::{Message, PeerInfoMsg};
use tracing::debug;

use crate::MeshNode;

#[derive(Debug, Clone)]
pub struct ScheduleRequest {
    /// Optional group name — only schedule among group members that are paired.
    pub group: Option<String>,
    /// Minimum free VRAM in MB.
    pub gpu_memory_mb: Option<u64>,
    /// Prefer a specific peer if it still satisfies constraints.
    pub prefer_peer: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ScheduleResult {
    pub peer_name: String,
    pub peer_node_id: String,
    pub gpu_model: Option<String>,
    pub vram_free_mb: Option<u64>,
    pub status: PeerStatus,
}

/// Select the best available peer for a job.
pub async fn schedule_peer(node: &MeshNode, req: ScheduleRequest) -> Result<ScheduleResult> {
    let candidates = candidate_peers(node, req.group.as_deref()).await?;
    if candidates.is_empty() {
        return Err(GpuMeshError::Other(
            "no candidate peers — pair peers or join a group first".into(),
        ));
    }

    let mut scored: Vec<(i64, ScheduleResult)> = Vec::new();

    for (node_id, name) in candidates {
        if let Some(prefer) = &req.prefer_peer {
            if prefer != &name && prefer != &node_id && !node_id.starts_with(prefer) {
                // still consider others; preference applied via score boost
            }
        }

        let probe = tokio::time::timeout(Duration::from_secs(3), probe_peer(node, &node_id)).await;
        let info = match probe {
            Ok(Ok(info)) => info,
            Ok(Err(e)) => {
                debug!("probe {name} failed: {e}");
                continue;
            }
            Err(_) => {
                debug!("probe {name} timed out");
                continue;
            }
        };

        if !info.sharing {
            continue;
        }
        if matches!(info.status, PeerStatus::Busy | PeerStatus::Offline) {
            continue;
        }
        if let Some(need) = req.gpu_memory_mb {
            let free = info.vram_free_mb.unwrap_or(0);
            if free < need {
                debug!(
                    "peer {name} free VRAM {free} MB < required {need} MB"
                );
                continue;
            }
        }

        let mut score = info.vram_free_mb.unwrap_or(0) as i64;
        // Prefer idle
        if matches!(info.status, PeerStatus::Idle) {
            score += 10_000;
        }
        // Prefer lower utilization
        if let Some(u) = info.utilization {
            score += (100i64 - u as i64) * 10;
        }
        if let Some(prefer) = &req.prefer_peer {
            if prefer == &name || prefer == &node_id || node_id.starts_with(prefer) {
                score += 50_000;
            }
        }

        scored.push((
            score,
            ScheduleResult {
                peer_name: name,
                peer_node_id: node_id,
                gpu_model: info.gpu_model,
                vram_free_mb: info.vram_free_mb,
                status: info.status,
            },
        ));
    }

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored
        .into_iter()
        .next()
        .map(|(_, r)| r)
        .ok_or_else(|| {
            GpuMeshError::Other(
                "no idle peer with enough free VRAM — try a lower --gpu-memory or wait"
                    .into(),
            )
        })
}

async fn candidate_peers(node: &MeshNode, group: Option<&str>) -> Result<Vec<(String, String)>> {
    let store = node.peers.read().await;
    let paired: Vec<(String, String)> = store
        .list()
        .into_iter()
        .map(|p| (p.node_id.clone(), p.node_name.clone()))
        .collect();
    drop(store);

    let Some(group_name) = group else {
        return Ok(paired);
    };

    let groups = gpumesh_storage::GroupStore::load()?;
    let g = groups
        .get(group_name)
        .ok_or_else(|| GpuMeshError::Other(format!("group not found: {group_name}")))?;
    let member_ids: std::collections::HashSet<_> = g.member_ids().into_iter().collect();
    let out: Vec<_> = paired
        .into_iter()
        .filter(|(id, name)| {
            member_ids.contains(id)
                || g.members
                    .iter()
                    .any(|m| m.node_name.eq_ignore_ascii_case(name))
        })
        .collect();
    Ok(out)
}

async fn probe_peer(node: &MeshNode, peer_id: &str) -> Result<PeerInfoMsg> {
    let conn: PeerConnection = node.connect_peer(peer_id).await?;
    conn.send(Message::PeerInfoRequest).await?;
    let info = match conn.recv().await? {
        Some(Message::PeerInfo(info)) => info,
        other => {
            conn.close();
            return Err(GpuMeshError::Protocol(format!(
                "expected PeerInfo, got {other:?}"
            )));
        }
    };
    conn.close();
    Ok(info)
}
