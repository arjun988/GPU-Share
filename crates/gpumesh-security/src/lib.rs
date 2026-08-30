//! Cryptographic identity, pairing codes, and peer authorization.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use gpumesh_common::{identity_path, GpuMeshError, Result, HELLO_TTL_SECS, PAIRING_TTL_SECS};
use gpumesh_protocol::ProtocolHello;
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
        let bytes =
            hex::decode(&file.secret_key_hex).map_err(|e| GpuMeshError::Identity(e.to_string()))?;
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
        let now = chrono_now();
        if self.issued_at <= 0 || now - self.issued_at > PAIRING_TTL_SECS {
            return Err(GpuMeshError::Identity(format!(
                "pairing code expired (valid {PAIRING_TTL_SECS}s)"
            )));
        }
        if self.issued_at > now + 60 {
            return Err(GpuMeshError::Identity(
                "pairing code timestamp is in the future".into(),
            ));
        }
        Ok(())
    }

    pub fn encode_code(&self) -> Result<String> {
        let json = serde_json::to_vec(self).map_err(|e| GpuMeshError::Identity(e.to_string()))?;
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

fn chrono_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Sign a ProtocolHello in place.
pub fn sign_hello(identity: &NodeIdentity, hello: &mut ProtocolHello) {
    hello.issued_at = chrono_now();
    hello.signature.clear();
    let msg = hello.canonical_bytes();
    hello.signature = identity.sign_b64(&msg);
}

/// Verify Hello signature, node_id↔key binding, and freshness.
pub fn verify_hello(hello: &ProtocolHello) -> Result<()> {
    if hello.signature.is_empty() {
        return Err(GpuMeshError::Identity("hello missing signature".into()));
    }
    let now = chrono_now();
    if hello.issued_at <= 0 || now - hello.issued_at > HELLO_TTL_SECS {
        return Err(GpuMeshError::Identity(
            "hello expired or missing timestamp".into(),
        ));
    }
    if hello.issued_at > now + 60 {
        return Err(GpuMeshError::Identity(
            "hello timestamp in the future".into(),
        ));
    }
    verify_signature(
        &hello.public_key_hex,
        &hello.canonical_bytes(),
        &hello.signature,
    )?;
    let expected = node_id_from_public_hex(&hello.public_key_hex)?;
    if expected != hello.node_id {
        return Err(GpuMeshError::Identity(
            "hello node_id does not match public key".into(),
        ));
    }
    Ok(())
}

/// Sign arbitrary UTF-8/JSON payload bytes.
pub fn sign_payload(identity: &NodeIdentity, payload_without_sig: &[u8]) -> String {
    identity.sign_b64(payload_without_sig)
}

/// Minimal interface implemented by the network crate's public listing type.
/// Keeping the interface here avoids a security↔network dependency cycle.
pub trait PublicListingPayload {
    fn node_id(&self) -> &str;
    fn public_key_hex(&self) -> &str;
    fn issued_at(&self) -> i64;
    fn set_issued_at(&mut self, value: i64);
    fn signature(&self) -> &str;
    fn set_signature(&mut self, value: String);
    fn canonical_bytes(&self) -> Vec<u8>;
}

pub fn sign_public_listing(identity: &NodeIdentity, listing: &mut impl PublicListingPayload) {
    listing.set_issued_at(chrono_now());
    listing.set_signature(String::new());
    let signature = identity.sign_b64(&listing.canonical_bytes());
    listing.set_signature(signature);
}

pub fn verify_public_listing(listing: &impl PublicListingPayload) -> Result<()> {
    if listing.signature().is_empty() {
        return Err(GpuMeshError::Identity(
            "public listing missing signature".into(),
        ));
    }
    let now = chrono_now();
    if listing.issued_at() <= 0 || now - listing.issued_at() > gpumesh_common::ANNOUNCE_TTL_SECS {
        return Err(GpuMeshError::Identity(
            "public listing expired or missing timestamp".into(),
        ));
    }
    if listing.issued_at() > now + 60 {
        return Err(GpuMeshError::Identity(
            "public listing timestamp is in the future".into(),
        ));
    }
    verify_signature(
        listing.public_key_hex(),
        &listing.canonical_bytes(),
        listing.signature(),
    )?;
    if node_id_from_public_hex(listing.public_key_hex())? != listing.node_id() {
        return Err(GpuMeshError::Identity(
            "public listing node_id does not match public key".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AllowList {
    /// Node IDs explicitly allowed to run jobs.
    pub allowed: HashSet<String>,
    /// Node IDs explicitly denied (takes precedence).
    pub denied: HashSet<String>,
    /// Node IDs allowed to open interactive desktop sessions (separate from jobs).
    #[serde(default)]
    pub desktop_allowed: HashSet<String>,
    /// Node IDs allowed to open CUDA remoting sessions (R2; stricter — does not grant jobs).
    #[serde(default)]
    pub gpu_remote_allowed: HashSet<String>,
}

impl AllowList {
    pub fn is_allowed(&self, node_id: &str) -> bool {
        if self.denied.contains(node_id) {
            return false;
        }
        self.allowed.contains(node_id)
    }

    pub fn is_desktop_allowed(&self, node_id: &str) -> bool {
        if self.denied.contains(node_id) {
            return false;
        }
        self.desktop_allowed.contains(node_id)
    }

    pub fn is_gpu_remote_allowed(&self, node_id: &str) -> bool {
        if self.denied.contains(node_id) {
            return false;
        }
        self.gpu_remote_allowed.contains(node_id)
    }

    pub fn allow(&mut self, node_id: impl Into<String>) {
        let id = node_id.into();
        self.denied.remove(&id);
        self.allowed.insert(id);
    }

    pub fn allow_desktop(&mut self, node_id: impl Into<String>) {
        let id = node_id.into();
        self.denied.remove(&id);
        self.desktop_allowed.insert(id.clone());
        // Desktop peers are also job-allowed for convenience (scripts + apps).
        self.allowed.insert(id);
    }

    pub fn allow_gpu_remote(&mut self, node_id: impl Into<String>) {
        let id = node_id.into();
        self.denied.remove(&id);
        self.gpu_remote_allowed.insert(id);
        // Intentionally does NOT grant job allow — remoting is a separate capability.
    }

    pub fn deny(&mut self, node_id: impl Into<String>) {
        let id = node_id.into();
        self.allowed.remove(&id);
        self.desktop_allowed.remove(&id);
        self.gpu_remote_allowed.remove(&id);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_valid_signature() {
        let identity = NodeIdentity::from_seed([7; 32]);
        let message = b"signed gpumesh hello";

        assert!(verify_signature(
            &identity.public_key_hex(),
            message,
            &identity.sign_b64(message)
        )
        .is_ok());
    }

    #[test]
    fn rejects_signature_for_modified_message() {
        let identity = NodeIdentity::from_seed([9; 32]);
        let signature = identity.sign_b64(b"original");

        assert!(verify_signature(&identity.public_key_hex(), b"modified", &signature).is_err());
    }

    #[test]
    fn derives_node_id_from_public_key() {
        let identity = NodeIdentity::from_seed([11; 32]);

        assert_eq!(
            node_id_from_public_hex(&identity.public_key_hex()).unwrap(),
            identity.node_id
        );
        assert!(node_id_from_public_hex("abcd").is_err());
    }

    #[test]
    fn gpu_remote_allow_does_not_grant_jobs() {
        let mut list = AllowList::default();
        list.allow_gpu_remote("node-a");
        assert!(list.is_gpu_remote_allowed("node-a"));
        assert!(!list.is_allowed("node-a"));
        assert!(!list.is_desktop_allowed("node-a"));
        list.deny("node-a");
        assert!(!list.is_gpu_remote_allowed("node-a"));
    }
}
