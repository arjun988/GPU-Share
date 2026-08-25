//! `gpumesh update` — version check / self-update helper.

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::ui;

#[derive(Debug, Deserialize)]
struct LatestInfo {
    version: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}

pub async fn run(check_only: bool) -> Result<()> {
    ui::print_banner();
    let current = gpumesh_common::VERSION;
    ui::kv("Current", current);

    let cfg = gpumesh_storage::StateStore::load_config().unwrap_or_default();
    let Some(url) = cfg.update_url.clone() else {
        ui::warn("No update_url configured in ~/.gpumesh/config.toml");
        return Ok(());
    };

    let spinner = ui::spinner("Checking for updates…");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let resp = client.get(&url).send().await;
    spinner.finish_and_clear();

    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            ui::warn(format!("Update check failed: {e}"));
            ui::info("Build from source: cargo install --path crates/gpumesh-cli");
            return Ok(());
        }
    };

    if !resp.status().is_success() {
        ui::warn(format!("Update endpoint returned {}", resp.status()));
        return Ok(());
    }

    let info: LatestInfo = resp.json().await.context("parse latest.json")?;
    ui::kv("Latest", &info.version);
    if let Some(notes) = &info.notes {
        ui::dim(notes);
    }

    if info.version == current || info.version.trim_start_matches('v') == current {
        ui::ok("You are up to date.");
        return Ok(());
    }

    ui::info(format!("Update available: {current} → {}", info.version));
    if check_only {
        return Ok(());
    }

    let Some(download) = info.url else {
        ui::warn("No download URL in latest.json — update manually.");
        ui::info("cargo install --git https://github.com/gpumesh/gpumesh --locked gpumesh-cli");
        return Ok(());
    };

    // Download to temp and instruct replace (cross-platform safe).
    let tmp = std::env::temp_dir().join(format!("gpumesh-{}", info.version));
    ui::info(format!("Downloading {download}"));
    let bytes = client
        .get(&download)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    std::fs::write(&tmp, &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tmp, perms)?;
    }

    let current_exe = std::env::current_exe()?;
    let backup = current_exe.with_extension("bak");
    let _ = std::fs::remove_file(&backup);
    std::fs::rename(&current_exe, &backup)
        .with_context(|| format!("backup {}", current_exe.display()))?;
    if let Err(e) = std::fs::copy(&tmp, &current_exe) {
        let _ = std::fs::rename(&backup, &current_exe);
        bail!("failed to install update: {e}");
    }
    ui::ok(format!(
        "Updated binary at {} (backup: {})",
        current_exe.display(),
        backup.display()
    ));
    Ok(())
}
