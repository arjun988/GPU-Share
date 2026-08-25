//! GPUMesh wire protocol: versioned messages + length-prefixed JSON framing.

use std::io;

use bytes::{Buf, BufMut, BytesMut};
use gpumesh_common::{JobState, PeerStatus, PROTOCOL_MAJOR, PROTOCOL_MINOR};
use serde::{Deserialize, Serialize};
use tokio_util::codec::{Decoder, Encoder};
use uuid::Uuid;

pub const MAX_FRAME_SIZE: usize = 64 * 1024 * 1024; // 64 MiB

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolHello {
    pub major: u16,
    pub minor: u16,
    pub node_id: String,
    pub node_name: String,
    pub public_key_hex: String,
    pub sharing: bool,
    pub gpu_model: Option<String>,
    pub vram_total_mb: Option<u64>,
    pub vram_free_mb: Option<u64>,
    pub status: PeerStatus,
    /// Unix timestamp when this Hello was signed.
    #[serde(default)]
    pub issued_at: i64,
    /// Ed25519 signature over canonical Hello bytes (signature field empty).
    #[serde(default)]
    pub signature: String,
}

impl ProtocolHello {
    pub fn check_compat(&self) -> Result<(), String> {
        if self.major != PROTOCOL_MAJOR {
            Err(format!(
                "incompatible protocol major: peer={} local={}",
                self.major, PROTOCOL_MAJOR
            ))
        } else {
            Ok(())
        }
    }

    /// Canonical bytes for signing (signature cleared).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut clone = self.clone();
        clone.signature.clear();
        serde_json::to_vec(&clone).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Hello(ProtocolHello),
    HelloAck(ProtocolHello),
    Ping {
        nonce: u64,
    },
    Pong {
        nonce: u64,
    },
    PeerInfoRequest,
    PeerInfo(PeerInfoMsg),
    RunJob(RunJobRequest),
    JobAccepted {
        job_id: String,
    },
    JobRejected {
        reason: String,
    },
    JobLog {
        job_id: String,
        stream: LogStream,
        line: String,
    },
    JobStatus {
        job_id: String,
        state: JobState,
        exit_code: Option<i32>,
        error: Option<String>,
        gpu_util: Option<u32>,
        vram_used_mb: Option<u64>,
        vram_total_mb: Option<u64>,
    },
    CancelJob {
        job_id: String,
    },
    CancelAck {
        job_id: String,
        ok: bool,
    },
    FileOffer(FileOffer),
    FileChunk(FileChunk),
    FileAck {
        transfer_id: String,
        ok: bool,
        error: Option<String>,
        /// If set after FileOffer, sender should resume uploading from this byte offset.
        #[serde(default)]
        resume_from: Option<u64>,
    },
    GroupJoinNotify {
        group_id: String,
        group_name: String,
        member_node_id: String,
        member_name: String,
        public_key_hex: String,
        signature: String,
    },
    ExecRequest(ExecRequest),
    ExecOutput {
        session_id: String,
        data: Vec<u8>,
    },
    ExecClose {
        session_id: String,
        exit_code: Option<i32>,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfoMsg {
    pub node_id: String,
    pub node_name: String,
    pub gpu_model: Option<String>,
    pub vram_total_mb: Option<u64>,
    pub vram_free_mb: Option<u64>,
    pub utilization: Option<u32>,
    pub temperature_c: Option<u32>,
    pub status: PeerStatus,
    pub sharing: bool,
    pub addrs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunJobRequest {
    pub job_id: String,
    pub image: String,
    pub command: Vec<String>,
    pub env: Vec<(String, String)>,
    pub workdir: String,
    pub transfer_id: Option<String>,
    pub gpu_memory_mb: Option<u64>,
}

impl RunJobRequest {
    pub fn new(image: String, command: Vec<String>) -> Self {
        Self {
            job_id: short_job_id(),
            image,
            command,
            env: Vec::new(),
            workdir: "/workspace".into(),
            transfer_id: None,
            gpu_memory_mb: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogStream {
    Stdout,
    Stderr,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOffer {
    pub transfer_id: String,
    pub path: String,
    pub size: u64,
    pub sha256_hex: String,
    pub direction: FileDirection,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileDirection {
    /// Consumer → provider (upload before run)
    Upload,
    /// Provider → consumer (download artifacts)
    Download,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChunk {
    pub transfer_id: String,
    pub offset: u64,
    pub data: Vec<u8>,
    pub eof: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecRequest {
    pub session_id: String,
    pub image: String,
    pub command: Vec<String>,
}

pub fn short_job_id() -> String {
    let id = Uuid::new_v4();
    let s = id.simple().to_string();
    // 12 hex chars — short but collision-resistant enough for concurrent jobs.
    s[..12].to_string()
}

pub fn new_transfer_id() -> String {
    Uuid::new_v4().to_string()
}

/// Length-prefixed JSON frames: `[u32 BE length][utf8 json]`.
#[derive(Debug, Default)]
pub struct JsonFrameCodec;

impl Decoder for JsonFrameCodec {
    type Item = Message;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 4 {
            return Ok(None);
        }
        let mut len_bytes = [0u8; 4];
        len_bytes.copy_from_slice(&src[..4]);
        let len = u32::from_be_bytes(len_bytes) as usize;
        if len > MAX_FRAME_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("frame too large: {len}"),
            ));
        }
        if src.len() < 4 + len {
            return Ok(None);
        }
        src.advance(4);
        let data = src.split_to(len);
        let msg: Message = serde_json::from_slice(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(Some(msg))
    }
}

impl Encoder<Message> for JsonFrameCodec {
    type Error = io::Error;

    fn encode(&mut self, item: Message, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let data =
            serde_json::to_vec(&item).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        if data.len() > MAX_FRAME_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "frame too large",
            ));
        }
        dst.reserve(4 + data.len());
        dst.put_u32(data.len() as u32);
        dst.extend_from_slice(&data);
        Ok(())
    }
}

pub fn protocol_version_string() -> String {
    format!("{PROTOCOL_MAJOR}.{PROTOCOL_MINOR}")
}
