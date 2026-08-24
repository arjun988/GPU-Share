//! Workload packaging with `.gpumeshignore` support.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use gpumesh_common::{GpuMeshError, Result};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    pub files: Vec<PackagedFile>,
    pub total_bytes: u64,
    pub sha256_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackagedFile {
    pub relative_path: String,
    pub size: u64,
    pub sha256_hex: String,
}

/// Package a working directory into a simple archive (gpumesh pack v1).
/// Format: magic "GPK1" + JSON manifest length + manifest + concatenated file bytes.
pub fn package_workdir(root: &Path, out_path: &Path) -> Result<PackageManifest> {
    if !root.exists() {
        return Err(GpuMeshError::Storage(format!(
            "workdir not found: {}",
            root.display()
        )));
    }

    let mut builder = WalkBuilder::new(root);
    builder.hidden(false);
    builder.git_ignore(true);
    builder.add_custom_ignore_filename(".gpumeshignore");
    builder.filter_entry(|e| {
        let name = e.file_name().to_string_lossy();
        !matches!(
            name.as_ref(),
            ".git" | "target" | "node_modules" | "__pycache__" | ".venv" | "venv"
        )
    });

    let mut files = Vec::new();
    let mut blobs: Vec<(String, Vec<u8>)> = Vec::new();
    let mut total = 0u64;
    let mut hasher = Sha256::new();

    for entry in builder.build().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|e| GpuMeshError::Storage(e.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        let mut data = Vec::new();
        File::open(path)?.read_to_end(&mut data)?;
        let mut file_hasher = Sha256::new();
        file_hasher.update(&data);
        let file_hash = hex::encode(file_hasher.finalize());
        hasher.update(&data);
        total += data.len() as u64;
        files.push(PackagedFile {
            relative_path: rel.clone(),
            size: data.len() as u64,
            sha256_hex: file_hash,
        });
        blobs.push((rel, data));
    }

    let sha256_hex = hex::encode(hasher.finalize());
    let manifest = PackageManifest {
        files: files.clone(),
        total_bytes: total,
        sha256_hex: sha256_hex.clone(),
    };

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut out = File::create(out_path)?;
    out.write_all(b"GPK1")?;
    let man_bytes =
        serde_json::to_vec(&manifest).map_err(|e| GpuMeshError::Storage(e.to_string()))?;
    out.write_all(&(man_bytes.len() as u32).to_be_bytes())?;
    out.write_all(&man_bytes)?;
    for (_rel, data) in blobs {
        out.write_all(&data)?;
    }
    out.flush()?;
    Ok(manifest)
}

pub fn unpack_archive(archive: &Path, dest: &Path) -> Result<PackageManifest> {
    let mut f = File::open(archive)?;
    let mut magic = [0u8; 4];
    f.read_exact(&mut magic)?;
    if &magic != b"GPK1" {
        return Err(GpuMeshError::Storage("invalid package magic".into()));
    }
    let mut len_buf = [0u8; 4];
    f.read_exact(&mut len_buf)?;
    let man_len = u32::from_be_bytes(len_buf) as usize;
    let mut man_buf = vec![0u8; man_len];
    f.read_exact(&mut man_buf)?;
    let manifest: PackageManifest =
        serde_json::from_slice(&man_buf).map_err(|e| GpuMeshError::Storage(e.to_string()))?;

    fs::create_dir_all(dest)?;
    for file in &manifest.files {
        let mut data = vec![0u8; file.size as usize];
        f.read_exact(&mut data)?;
        let path = sanitize_rel(dest, &file.relative_path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, &data)?;
    }
    Ok(manifest)
}

fn sanitize_rel(dest: &Path, rel: &str) -> Result<PathBuf> {
    if rel.contains("..") {
        return Err(GpuMeshError::Storage(format!(
            "illegal path in package: {rel}"
        )));
    }
    Ok(dest.join(rel))
}
