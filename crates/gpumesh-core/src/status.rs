use gpumesh_common::PeerStatus;
use gpumesh_gpu::GpuInfo;
use gpumesh_storage::PeerRecord;

use crate::MeshNode;

pub struct NodeStatusView {
    pub node_name: String,
    pub node_id: String,
    pub gpus: Vec<GpuInfo>,
    pub p2p: bool,
    pub peers: usize,
    pub sharing: bool,
}

pub async fn collect_status(node: &MeshNode) -> NodeStatusView {
    let cfg = node.config.read().await.clone();
    let gpus = MeshNode::detect_gpus().unwrap_or_default();
    let peers = node.peers.read().await.list().len();
    let sharing = *node.sharing.read().await || cfg.sharing_enabled;
    NodeStatusView {
        node_name: cfg.node_name,
        node_id: node.identity.node_id.clone(),
        gpus,
        p2p: node.endpoint.is_some(),
        peers,
        sharing,
    }
}

pub fn format_status(view: &NodeStatusView) -> String {
    let mut out = String::new();
    out.push_str("GPUMesh Status\n\n");
    out.push_str(&format!("Node: {}\n", view.node_name));
    out.push_str(&format!("Node ID: {}\n\n", view.node_id));
    out.push_str("GPU:\n");
    if view.gpus.is_empty() {
        out.push_str("  (no NVIDIA GPU detected)\n\n");
    } else {
        for g in &view.gpus {
            out.push_str(&format!("  {}\n", g.name));
            out.push_str(&format!(
                "  VRAM: {} GB\n",
                (g.vram_total_mb as f64 / 1024.0).round() as u64
            ));
            out.push_str(&format!(
                "  Utilization: {}%\n",
                g.utilization_gpu.unwrap_or(0)
            ));
            if let Some(t) = g.temperature_c {
                out.push_str(&format!("  Temperature: {t}°C\n"));
            }
            out.push('\n');
        }
    }
    out.push_str("Network:\n");
    out.push_str(&format!(
        "  P2P: {}\n",
        if view.p2p { "Connected" } else { "Idle" }
    ));
    out.push_str(&format!("  Peers: {}\n", view.peers));
    out.push_str(&format!(
        "  Sharing: {}\n",
        if view.sharing { "enabled" } else { "disabled" }
    ));
    out
}

pub fn format_peers_table(
    peers: &[&PeerRecord],
    live: &[(String, PeerStatus, Option<String>, Option<u64>)],
) -> String {
    let mut out = String::from("NAME          GPU              VRAM     STATUS\n");
    for p in peers {
        let (status, gpu, vram) = live
            .iter()
            .find(|(id, _, _, _)| id == &p.node_id)
            .map(|(_, s, g, v)| (*s, g.clone(), *v))
            .unwrap_or((
                PeerStatus::Unknown,
                p.gpu_model.clone(),
                p.vram_mb,
            ));
        let gpu = gpu.unwrap_or_else(|| "-".into());
        let vram_s = vram
            .map(|m| format!("{}GB", (m as f64 / 1024.0).round() as u64))
            .unwrap_or_else(|| "-".into());
        out.push_str(&format!(
            "{:<13} {:<16} {:<8} {}\n",
            truncate(&p.node_name, 13),
            truncate(&gpu, 16),
            vram_s,
            status
        ));
    }
    if peers.is_empty() {
        out.push_str("(no paired peers — run `gpumesh pair <code>`)\n");
    }
    out
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}
