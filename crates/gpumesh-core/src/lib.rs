//! Domain orchestration: node bootstrap, sharing, pairing, local & remote jobs.

mod handshake;
mod desktop;
mod node;
mod remote;
mod scheduler;
mod status;

pub use desktop::{connect_desktop, detect_backend};
pub use node::MeshNode;
pub use remote::{run_local_job, run_remote_job, transfer_file_from_peer, transfer_file_to_peer};
pub use scheduler::{schedule_peer, ScheduleRequest, ScheduleResult};
pub use status::{collect_status, format_peers_table, format_status, NodeStatusView};
