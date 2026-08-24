//! Cryptographic identity, pairing codes, and peer authorization.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use gpumesh_common::{identity_path, GpuMeshError, Result};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub struct NodeIdentity {
    signing_key: SigningKey,
    pub node_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IdentityFile {
    /// Hex-encoded 32-byte secret key seed.
    secret_key_hex: String,
    node_id: String,
}

impl NodeIdentity {
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let node_id = node_id_from_verifying(&signing_key.verifying_key());
        Self {
            signing_key,
            node_id,
        }
    }

    pub fn from_seed(seed: [u8; 32]) -> Self {
        let signing_key = SigningKey::from_bytes(&seed);
        let node_id = node_id_from_verifying(&signing_key.verifying_key());
        Self {
            signing_key,
            node_id,
        }
    }

    pub fn load_or_create(path: &Path) -> Result<Self> {
        if path.exists() {
            Self::load(path)
        } else {
            let id = Self::generate();
            id.save(path)?;
            Ok(id)
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let data = fs::read_to_string(path).map_err(|e| GpuMeshError::Identity(e.to_string()))?;
        let file: IdentityFile =
            serde_json::from_str(&data).map_err(|e| GpuMeshError::Identity(e.to_string()))?;
        let bytes = hex::decode(&file.secret_key_hex)
            .map_err(|e| GpuMeshError::Identity(e.to_string()))?;
        if bytes.len() != 32 {
            return Err(GpuMeshError::Identity("invalid secret key length".into()));
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes);
        let id = Self::from_seed(seed);
        if id.node_id != file.node_id {
            return Err(GpuMeshError::Identity(
                "node_id mismatch with secret key".into(),
            ));
        }
        Ok(id)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = IdentityFile {
            secret_key_hex: hex::encode(self.signing_key.to_bytes()),
            node_id: self.node_id.clone(),
        };
        let data = serde_json::to_string_pretty(&file)
            .map_err(|e| GpuMeshError::Identity(e.to_string()))?;
        fs::write(path, data)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(path, perms)?;
        }
        Ok(())
    }

    pub fn load_default() -> Result<Self> {
        let path = identity_path();
        if !path.exists() {
            return Err(GpuMeshError::NotInitialized);
        }
        Self::load(&path)
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.verifying_key().to_bytes()
    }

    pub fn public_key_hex(&self) -> String {
        hex::encode(self.public_key_bytes())
    }

    pub fn sign(&self, message: &[u8]) -> Signature {
        self.signing_key.sign(message)
    }

    pub fn sign_b64(&self, message: &[u8]) -> String {
        URL_SAFE_NO_PAD.encode(self.sign(message).to_bytes())
    }
}

pub fn node_id_from_verifying(vk: &VerifyingKey) -> String {
    let mut hasher = Sha256::new();
    hasher.update(vk.as_bytes());
    let digest = hasher.finalize();
    hex::encode(&digest[..16])
}

pub fn node_id_from_public_hex(public_hex: &str) -> Result<String> {
    let bytes = hex::decode(public_hex).map_err(|e| GpuMeshError::Identity(e.to_string()))?;
    if bytes.len() != 32 {
        return Err(GpuMeshError::Identity("public key must be 32 bytes".into()));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    let vk = VerifyingKey::from_bytes(&arr).map_err(|e| GpuMeshError::Identity(e.to_string()))?;
    Ok(node_id_from_verifying(&vk))
}

pub fn verify_signature(public_hex: &str, message: &[u8], sig_b64: &str) -> Result<()> {
    let pk = hex::decode(public_hex).map_err(|e| GpuMeshError::Identity(e.to_string()))?;
    if pk.len() != 32 {
        return Err(GpuMeshError::Identity("bad public key".into()));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&pk);
    let vk = VerifyingKey::from_bytes(&arr).map_err(|e| GpuMeshError::Identity(e.to_string()))?;
    let sig_bytes = URL_SAFE_NO_PAD
        .decode(sig_b64)
        .map_err(|e| GpuMeshError::Identity(e.to_string()))?;
    if sig_bytes.len() != 64 {
        return Err(GpuMeshError::Identity("bad signature length".into()));
    }
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let sig = Signature::from_bytes(&sig_arr);
    vk.verify(message, &sig)
        .map_err(|e| GpuMeshError::Identity(format!("signature verify failed: {e}")))
}

/// Pairing payload exchanged out-of-band.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingPayload {
    pub version: u16,
    pub node_id: String,
    pub node_name: String,
    pub public_key_hex: String,
    pub addrs: Vec<String>,
    pub gpu_model: Option<String>,
    pub vram_mb: Option<u64>,
    pub issued_at: i64,
    pub signature: String,
}

impl PairingPayload {
    pub fn sign_with(identity: &NodeIdentity, mut payload: PairingPayload) -> PairingPayload {
        payload.signature = String::new();
        let msg = canonical_pairing_bytes(&payload);
        payload.signature = identity.sign_b64(&msg);
        payload
    }

    pub fn verify(&self) -> Result<()> {
        let mut clone = self.clone();
        let sig = std::mem::take(&mut clone.signature);
        let msg = canonical_pairing_bytes(&clone);
        verify_signature(&self.public_key_hex, &msg, &sig)?;
        let expected = node_id_from_public_hex(&self.public_key_hex)?;
        if expected != self.node_id {
            return Err(GpuMeshError::Identity("pairing node_id mismatch".into()));
        }
        Ok(())
    }

    pub fn encode_code(&self) -> Result<String> {
        let json =
            serde_json::to_vec(self).map_err(|e| GpuMeshError::Identity(e.to_string()))?;
        Ok(URL_SAFE_NO_PAD.encode(json))
    }

    pub fn decode_code(code: &str) -> Result<Self> {
        let bytes = URL_SAFE_NO_PAD
            .decode(code.trim())
            .map_err(|e| GpuMeshError::Identity(format!("invalid pairing code: {e}")))?;
        let payload: Self = serde_json::from_slice(&bytes)
            .map_err(|e| GpuMeshError::Identity(format!("invalid pairing payload: {e}")))?;
        payload.verify()?;
        Ok(payload)
    }
}

fn canonical_pairing_bytes(p: &PairingPayload) -> Vec<u8> {
    // Signature field excluded by caller; serialize stable fields.
    let mut clone = p.clone();
    clone.signature.clear();
    serde_json::to_vec(&clone).unwrap_or_default()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AllowList {
    /// Node IDs explicitly allowed.
    pub allowed: HashSet<String>,
    /// Node IDs explicitly denied (takes precedence).
    pub denied: HashSet<String>,
}

impl AllowList {
    pub fn is_allowed(&self, node_id: &str) -> bool {
        if self.denied.contains(node_id) {
            return false;
        }
        self.allowed.contains(node_id)
    }

    pub fn allow(&mut self, node_id: impl Into<String>) {
        let id = node_id.into();
        self.denied.remove(&id);
        self.allowed.insert(id);
    }

    pub fn deny(&mut self, node_id: impl Into<String>) {
        let id = node_id.into();
        self.allowed.remove(&id);
        self.denied.insert(id);
    }
}

/// Derive a deterministic self-signed-ish fingerprint for display.
pub fn short_fingerprint(node_id: &str) -> String {
    if node_id.len() <= 8 {
        node_id.to_string()
    } else {
        format!("{}…", &node_id[..8])
    }
}
