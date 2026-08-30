//! `gpumesh desktop` — interactive GPU desktop tunnels (RDP/VNC).

use anyhow::{bail, Result};
use gpumesh_core::{connect_desktop, detect_backend, MeshNode};

use crate::ui;

#[derive(Debug, clap::Subcommand)]
pub enum DesktopCmd {
    /// Share this PC's desktop/GPU apps (requires RDP or VNC listening locally)
    Share,
    /// Connect to a peer's GPU desktop (opens local tunnel)
    Connect {
        peer: String,
        /// Local bind port (default: backend suggestion, e.g. 13389 for RDP)
        #[arg(long)]
        port: Option<u16>,
    },
    /// Allow a paired peer to use desktop (and jobs)
    Allow { peer: String },
    /// Probe local desktop backends (RDP/VNC)
    Doctor,
}

pub async fn dispatch(cmd: DesktopCmd) -> Result<()> {
    match cmd {
        DesktopCmd::Share => share().await,
        DesktopCmd::Connect { peer, port } => {
            ui::print_banner();
            let mut node = MeshNode::bootstrap().await?;
            node.start_network().await?;
            ui::info(format!("Connecting desktop to {peer}…"));
            connect_desktop(&node, &peer, port).await?;
            Ok(())
        }
        DesktopCmd::Allow { peer } => {
            ui::print_banner();
            let node = MeshNode::bootstrap().await?;
            node.allow_desktop_peer(&peer).await?;
            ui::ok(format!("Desktop (+ jobs) allowed for {peer}"));
            ui::dim("Peer can: gpumesh desktop connect <you>  and  gpumesh run --peer <you> …");
            Ok(())
        }
        DesktopCmd::Doctor => {
            ui::print_banner();
            ui::section("Desktop backends");
            match detect_backend(&[]).await {
                Some(b) => {
                    ui::ok(format!("Found {} on 127.0.0.1:{}", b.name, b.host_port));
                    ui::dim(&b.viewer_hint.replace("LOCAL_PORT", &b.suggest_local_port.to_string()));
                }
                None => {
                    ui::warn("No RDP (:3389) or VNC (:5900) detected on localhost.");
                    ui::dim("Windows: Settings → System → Remote Desktop → enable");
                    ui::dim("Linux: run a VNC server (e.g. wayvnc / x11vnc) on :5900");
                }
            }
            Ok(())
        }
    }
}

async fn share() -> Result<()> {
    ui::print_banner();
    let mut node = MeshNode::bootstrap().await?;
    node.start_network().await?;
    node.enable_desktop_share().await?;

    match detect_backend(&[]).await {
        Some(b) => {
            ui::ok(format!("Desktop backend: {} → 127.0.0.1:{}", b.name, b.host_port));
            ui::dim(&b.viewer_hint.replace("LOCAL_PORT", "(client local port)"));
        }
        None => {
            bail!(
                "No desktop backend on this machine.\n\
                 Enable Windows Remote Desktop (port 3389) or start VNC on 5900,\n\
                 then re-run: gpumesh desktop share\n\
                 Check with: gpumesh desktop doctor"
            );
        }
    }

    ui::ok("Desktop sharing enabled — waiting for authorized peers…");
    ui::dim("Allow a client:  gpumesh desktop allow <peer>");
    ui::dim("Client runs:     gpumesh desktop connect <your-name>");
    ui::dim("Scripts still:   gpumesh run --peer <your-name> …");
    if let Ok(code) = node.pairing_code().await {
        println!();
        ui::dim("Pairing code:");
        println!("{code}");
    }

    // PID for stop compatibility with share stop
    let _ = std::fs::create_dir_all(gpumesh_common::logs_dir());
    if let Some(parent) = gpumesh_common::share_pid_path().parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::remove_file(gpumesh_common::share_stop_path());
    let _ = std::fs::write(
        gpumesh_common::share_pid_path(),
        format!("{}\n", std::process::id()),
    );

    let node = std::sync::Arc::new(node);
    let endpoint = node.endpoint()?;
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        tokio::select! {
            _ = tick.tick() => {
                if gpumesh_common::share_stop_path().exists() {
                    ui::ok("Desktop share stopped.");
                    let _ = std::fs::remove_file(gpumesh_common::share_pid_path());
                    let _ = std::fs::remove_file(gpumesh_common::share_stop_path());
                    return Ok(());
                }
            }
            accepted = endpoint.accept() => {
                match accepted {
                    Ok(conn) => {
                        let node = node.clone();
                        tokio::spawn(async move {
                            if let Err(e) = node.handle_inbound(conn).await {
                                tracing::error!("session error: {e}");
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("accept error: {e}");
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    }
                }
            }
        }
    }
}
