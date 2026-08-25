//! Private GPU clusters / groups (Phase 5).

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use gpumesh_common::{config_dir, GpuMeshError, Result};
use gpumesh_security::{verify_signature, NodeIdentity};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GroupRole {
    Owner,
    Member,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMember {
    pub node_id: String,
    pub node_name: String,
    pub role: GroupRole,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub owner_node_id: String,
    pub created_at: DateTime<Utc>,
    pub members: Vec<GroupMember>,
}

impl Group {
    pub fn member_ids(&self) -> Vec<String> {
        self.members.iter().map(|m| m.node_id.clone()).collect()
    }

    pub fn contains(&self, node_id: &str) -> bool {
        self.members.iter().any(|m| m.node_id == node_id)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct GroupsFile {
    groups: HashMap<String, Group>,
}

pub struct GroupStore {
    path: PathBuf,
    groups: HashMap<String, Group>,
}

impl GroupStore {
    fn path() -> PathBuf {
        config_dir().join("groups.json")
    }

    pub fn load() -> Result<Self> {
        let path = Self::path();
        let groups = if path.exists() {
            let data = fs::read_to_string(&path)?;
            let file: GroupsFile = serde_json::from_str(&data)
                .map_err(|e| GpuMeshError::Storage(e.to_string()))?;
            file.groups
        } else {
            HashMap::new()
        };
        Ok(Self { path, groups })
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = GroupsFile {
            groups: self.groups.clone(),
        };
        let data = serde_json::to_string_pretty(&file)
            .map_err(|e| GpuMeshError::Storage(e.to_string()))?;
        fs::write(&self.path, data)?;
        Ok(())
    }

    pub fn create(&mut self, name: &str, owner: &NodeIdentity, owner_name: &str) -> Result<Group> {
        let key = name.to_lowercase();
        if self.groups.contains_key(&key) {
            return Err(GpuMeshError::Other(format!("group already exists: {name}")));
        }
        let id = format!("grp-{}", &owner.node_id[..8.min(owner.node_id.len())]);
        let group = Group {
            id: format!("{id}-{key}"),
            name: name.to_string(),
            owner_node_id: owner.node_id.clone(),
            created_at: Utc::now(),
            members: vec![GroupMember {
                node_id: owner.node_id.clone(),
                node_name: owner_name.to_string(),
                role: GroupRole::Owner,
                joined_at: Utc::now(),
            }],
        };
        self.groups.insert(key, group.clone());
        self.save()?;
        Ok(group)
    }

    pub fn get(&self, name: &str) -> Option<&Group> {
        self.groups
            .get(&name.to_lowercase())
            .or_else(|| self.groups.values().find(|g| g.id == name || g.name == name))
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Group> {
        let key = self
            .groups
            .iter()
            .find(|(k, g)| *k == &name.to_lowercase() || g.id == name || g.name == name)
            .map(|(k, _)| k.clone())?;
        self.groups.get_mut(&key)
    }

    pub fn list(&self) -> Vec<&Group> {
        let mut v: Vec<_> = self.groups.values().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    pub fn add_member(
        &mut self,
        group: &str,
        node_id: &str,
        node_name: &str,
        role: GroupRole,
    ) -> Result<()> {
        let g = self
            .get_mut(group)
            .ok_or_else(|| GpuMeshError::Other(format!("group not found: {group}")))?;
        if g.contains(node_id) {
            return Ok(());
        }
        g.members.push(GroupMember {
            node_id: node_id.to_string(),
            node_name: node_name.to_string(),
            role,
            joined_at: Utc::now(),
        });
        self.save()?;
        Ok(())
    }

    pub fn remove_member(&mut self, group: &str, node_id: &str) -> Result<()> {
        let g = self
            .get_mut(group)
            .ok_or_else(|| GpuMeshError::Other(format!("group not found: {group}")))?;
        if g.owner_node_id == node_id {
            return Err(GpuMeshError::Other(
                "cannot remove group owner — delete the group instead".into(),
            ));
        }
        g.members.retain(|m| m.node_id != node_id);
        self.save()?;
        Ok(())
    }

    pub fn delete(&mut self, name: &str) -> Result<()> {
        let key = name.to_lowercase();
        if self.groups.remove(&key).is_none() {
            return Err(GpuMeshError::Other(format!("group not found: {name}")));
        }
        self.save()?;
        Ok(())
    }

    /// Insert or replace a full group record (used when joining via invite).
    pub fn upsert(&mut self, group: Group) -> Result<()> {
        let key = group.name.to_lowercase();
        self.groups.insert(key, group);
        self.save()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInvite {
    pub version: u16,
    pub group_id: String,
    pub group_name: String,
    pub owner_node_id: String,
    pub owner_name: String,
    pub owner_public_key_hex: String,
    pub issued_at: i64,
    pub signature: String,
}

impl GroupInvite {
    pub fn create(identity: &NodeIdentity, group: &Group, owner_name: &str) -> Self {
        let mut invite = Self {
            version: 1,
            group_id: group.id.clone(),
            group_name: group.name.clone(),
            owner_node_id: group.owner_node_id.clone(),
            owner_name: owner_name.to_string(),
            owner_public_key_hex: identity.public_key_hex(),
            issued_at: Utc::now().timestamp(),
            signature: String::new(),
        };
        let msg = canonical_invite_bytes(&invite);
        invite.signature = identity.sign_b64(&msg);
        invite
    }

    pub fn verify(&self) -> Result<()> {
        let mut clone = self.clone();
        let sig = std::mem::take(&mut clone.signature);
        let msg = canonical_invite_bytes(&clone);
        verify_signature(&self.owner_public_key_hex, &msg, &sig)?;
        Ok(())
    }

    pub fn encode(&self) -> Result<String> {
        let json = serde_json::to_vec(self).map_err(|e| GpuMeshError::Storage(e.to_string()))?;
        Ok(URL_SAFE_NO_PAD.encode(json))
    }

    pub fn decode(code: &str) -> Result<Self> {
        let bytes = URL_SAFE_NO_PAD
            .decode(code.trim())
            .map_err(|e| GpuMeshError::Storage(format!("invalid invite: {e}")))?;
        let invite: Self = serde_json::from_slice(&bytes)
            .map_err(|e| GpuMeshError::Storage(format!("invalid invite payload: {e}")))?;
        invite.verify()?;
        Ok(invite)
    }
}

fn canonical_invite_bytes(i: &GroupInvite) -> Vec<u8> {
    let mut c = i.clone();
    c.signature.clear();
    serde_json::to_vec(&c).unwrap_or_default()
}
