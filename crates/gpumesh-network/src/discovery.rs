use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpumesh_common::{GpuMeshError, Result};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use tracing::{info, warn};

const SERVICE_TYPE: &str = "_gpumesh._udp.local.";

#[derive(Debug, Clone)]
pub struct DiscoveredPeer {
    pub node_id: String,
    pub node_name: String,
    pub addr: String,
}

/// LAN discovery via mDNS.
pub struct LanDiscovery {
    daemon: ServiceDaemon,
    known: Arc<Mutex<HashMap<String, DiscoveredPeer>>>,
}

impl LanDiscovery {
    pub fn start(node_id: &str, node_name: &str, port: u16) -> Result<Self> {
        let daemon = ServiceDaemon::new().map_err(|e| GpuMeshError::Network(e.to_string()))?;
        let host_ip = local_ip_address::local_ip()
            .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
        let host_name = format!("{host_ip}.local.");
        let mut props = HashMap::new();
        props.insert("node_id".to_string(), node_id.to_string());
        props.insert("node_name".to_string(), node_name.to_string());

        let service = ServiceInfo::new(
            SERVICE_TYPE,
            &format!("gpumesh-{node_id}"),
            &host_name,
            host_ip,
            port,
            props,
        )
        .map_err(|e| GpuMeshError::Network(e.to_string()))?;

        daemon
            .register(service)
            .map_err(|e| GpuMeshError::Network(e.to_string()))?;
        info!("mDNS registered as gpumesh-{node_id} on {host_ip}:{port}");

        let known = Arc::new(Mutex::new(HashMap::new()));
        let known_clone = known.clone();
        let receiver = daemon
            .browse(SERVICE_TYPE)
            .map_err(|e| GpuMeshError::Network(e.to_string()))?;

        std::thread::spawn(move || {
            while let Ok(event) = receiver.recv_timeout(Duration::from_secs(1)) {
                use mdns_sd::ServiceEvent;
                if let ServiceEvent::ServiceResolved(info) = event {
                    let node_id = info
                        .get_property_val_str("node_id")
                        .unwrap_or_default()
                        .to_string();
                    let node_name = info
                        .get_property_val_str("node_name")
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| info.get_fullname().to_string());
                    let addr = info
                        .get_addresses()
                        .iter()
                        .next()
                        .map(|a| format!("{a}:{}", info.get_port()))
                        .unwrap_or_default();
                    if node_id.is_empty() || addr.is_empty() {
                        continue;
                    }
                    let peer = DiscoveredPeer {
                        node_id: node_id.clone(),
                        node_name,
                        addr,
                    };
                    if let Ok(mut map) = known_clone.lock() {
                        map.insert(node_id, peer);
                    }
                }
            }
        });

        Ok(Self { daemon, known })
    }

    pub fn peers(&self) -> Vec<DiscoveredPeer> {
        self.known
            .lock()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn shutdown(self) {
        if let Err(e) = self.daemon.shutdown() {
            warn!("mDNS shutdown: {e}");
        }
    }
}

pub fn parse_socket(addr: &str) -> Result<SocketAddr> {
    addr.parse()
        .map_err(|e| GpuMeshError::Network(format!("bad socket addr: {e}")))
}
