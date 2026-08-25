//! `gpumesh group` — private GPU clusters (Phase 5).

use anyhow::{bail, Result};
use chrono::Utc;
use gpumesh_core::MeshNode;
use gpumesh_storage::{Group, GroupInvite, GroupMember, GroupRole, GroupStore};

use crate::ui;

#[derive(Debug, clap::Subcommand)]
pub enum GroupCmd {
    /// Create a private GPU cluster
    Create { name: String },
    /// List local groups
    List,
    /// Show group members
    Members { name: String },
    /// Add a paired peer to a group
    Add { group: String, peer: String },
    /// Remove a peer from a group
    Remove { group: String, peer: String },
    /// Generate an invite code
    Invite { name: String },
    /// Join a group via invite code
    Join { code: String },
    /// Delete a group you own
    Delete { name: String },
}

pub async fn dispatch(cmd: GroupCmd) -> Result<()> {
    match cmd {
        GroupCmd::Create { name } => {
            ui::print_banner();
            let node = MeshNode::bootstrap().await?;
            let cfg = node.config.read().await.clone();
            let mut store = GroupStore::load()?;
            let g = store.create(&name, &node.identity, &cfg.node_name)?;
            ui::ok(format!("Created group '{}'", g.name));
            ui::kv("ID", &g.id);
            ui::kv("Members", g.members.len().to_string());
            ui::dim("Invite others: gpumesh group invite <name>");
            Ok(())
        }
        GroupCmd::List => {
            ui::print_banner();
            ui::section("Groups");
            let store = GroupStore::load()?;
            let list = store.list();
            if list.is_empty() {
                ui::dim("No groups yet. Create one: gpumesh group create research");
                return Ok(());
            }
            println!("  {:<16} {:<8} {}", "NAME", "MEMBERS", "ID");
            for g in list {
                println!("  {:<16} {:<8} {}", g.name, g.members.len(), g.id);
            }
            Ok(())
        }
        GroupCmd::Members { name } => {
            let store = GroupStore::load()?;
            let g = store
                .get(&name)
                .ok_or_else(|| anyhow::anyhow!("group not found: {name}"))?;
            ui::section(&format!("Group: {}", g.name));
            for m in &g.members {
                let role = match m.role {
                    GroupRole::Owner => "owner",
                    GroupRole::Member => "member",
                };
                println!(
                    "  {:<16} {:<8} {}",
                    m.node_name,
                    role,
                    &m.node_id[..8.min(m.node_id.len())]
                );
            }
            Ok(())
        }
        GroupCmd::Add { group, peer } => {
            let node = MeshNode::bootstrap().await?;
            let peers = node.peers.read().await;
            let rec = peers
                .get(&peer)
                .ok_or_else(|| anyhow::anyhow!("peer not found / not paired: {peer}"))?;
            let id = rec.node_id.clone();
            let name = rec.node_name.clone();
            drop(peers);
            let mut store = GroupStore::load()?;
            store.add_member(&group, &id, &name, GroupRole::Member)?;
            ui::ok(format!("Added {name} to group {group}"));
            Ok(())
        }
        GroupCmd::Remove { group, peer } => {
            let node = MeshNode::bootstrap().await?;
            let peers = node.peers.read().await;
            let mut id = peers
                .get(&peer)
                .map(|p| p.node_id.clone())
                .unwrap_or_else(|| peer.clone());
            drop(peers);
            let store = GroupStore::load()?;
            if let Some(g) = store.get(&group) {
                if let Some(m) = g.members.iter().find(|m| {
                    m.node_name.eq_ignore_ascii_case(&peer) || m.node_id == id
                }) {
                    id = m.node_id.clone();
                }
            }
            let mut store = GroupStore::load()?;
            store.remove_member(&group, &id)?;
            ui::ok(format!("Removed {peer} from {group}"));
            Ok(())
        }
        GroupCmd::Invite { name } => {
            let node = MeshNode::bootstrap().await?;
            let cfg = node.config.read().await.clone();
            let store = GroupStore::load()?;
            let g = store
                .get(&name)
                .ok_or_else(|| anyhow::anyhow!("group not found: {name}"))?
                .clone();
            if g.owner_node_id != node.identity.node_id {
                bail!("only the group owner can create invites");
            }
            let invite = GroupInvite::create(&node.identity, &g, &cfg.node_name);
            let code = invite.encode()?;
            ui::ok(format!("Invite for group '{}'", g.name));
            println!();
            println!("{code}");
            println!();
            ui::dim("Peer runs:  gpumesh group join <code>");
            Ok(())
        }
        GroupCmd::Join { code } => {
            let node = MeshNode::bootstrap().await?;
            let cfg = node.config.read().await.clone();
            let invite = GroupInvite::decode(&code)?;
            let mut store = GroupStore::load()?;
            if let Some(_) = store.get(&invite.group_name) {
                store.add_member(
                    &invite.group_name,
                    &node.identity.node_id,
                    &cfg.node_name,
                    GroupRole::Member,
                )?;
            } else {
                let g = Group {
                    id: invite.group_id.clone(),
                    name: invite.group_name.clone(),
                    owner_node_id: invite.owner_node_id.clone(),
                    created_at: Utc::now(),
                    members: vec![
                        GroupMember {
                            node_id: invite.owner_node_id.clone(),
                            node_name: invite.owner_name.clone(),
                            role: GroupRole::Owner,
                            joined_at: Utc::now(),
                        },
                        GroupMember {
                            node_id: node.identity.node_id.clone(),
                            node_name: cfg.node_name.clone(),
                            role: GroupRole::Member,
                            joined_at: Utc::now(),
                        },
                    ],
                };
                store.upsert(g)?;
            }
            ui::ok(format!("Joined group '{}'", invite.group_name));
            ui::dim("Also pair with the owner if you have not already.");
            Ok(())
        }
        GroupCmd::Delete { name } => {
            let node = MeshNode::bootstrap().await?;
            let store = GroupStore::load()?;
            let owner = store
                .get(&name)
                .ok_or_else(|| anyhow::anyhow!("group not found: {name}"))?
                .owner_node_id
                .clone();
            if owner != node.identity.node_id {
                bail!("only the owner can delete the group");
            }
            let mut store = GroupStore::load()?;
            store.delete(&name)?;
            ui::ok(format!("Deleted group {name}"));
            Ok(())
        }
    }
}
