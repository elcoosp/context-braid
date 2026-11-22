use crate::macos;
use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

static SYNC_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Deserialize)]
struct DropConfig {
    repo: String,
    release_id: u64,
    token: String,
}

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    id: u64,
    name: String,
    size: u64,
}

fn load_config() -> Option<DropConfig> {
    let path = dirs::data_dir()?.join("ContextBraid").join("drop.json");
    let raw = fs::read_to_string(path).ok()?;
    let cfg: DropConfig = serde_json::from_str(&raw).ok()?;
    if cfg.repo.trim().is_empty() || cfg.token.trim().is_empty() || cfg.release_id == 0 {
        return None;
    }
    Some(cfg)
}

fn sanitize_zip_name(raw: &str) -> Option<String> {
    let file_name = Path::new(raw).file_name()?.to_str()?.trim();
    if file_name.is_empty()
        || file_name.starts_with('.')
        || !file_name.to_ascii_lowercase().ends_with(".zip")
    {
        return None;
    }
    Some(file_name.to_string())
}

fn unique_download_path(downloads_dir: &Path, clean_name: &str) -> PathBuf {
    let candidate = downloads_dir.join(clean_name);
    if !candidate.exists() {
        return candidate;
    }
    let stem = clean_name.strip_suffix(".zip").unwrap_or(clean_name);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    downloads_dir.join(format!("{stem}-{ts}.zip"))
}

fn infer_repo_label(clean_name: &str) -> String {
    clean_name
        .split_once("--")
        .map(|(prefix, _)| prefix.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("Drop Inbox")
        .to_string()
}

fn fetch_release_assets(cfg: &DropConfig) -> Result<Vec<ReleaseAsset>> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/{}/assets",
        cfg.repo.trim().trim_start_matches('/'),
        cfg.release_id
    );
    let out = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "15",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "X-GitHub-Api-Version: 2022-11-28",
            "-H",
            &format!("Authorization: Bearer {}", cfg.token.trim()),
            &url,
        ])
        .output()
        .context("failed to run curl for release assets")?;

    if !out.status.success() {
        return Err(anyhow!(
            "github assets list failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    serde_json::from_slice(&out.stdout).context("invalid GitHub release assets JSON")
}

fn download_and_delete_asset(
    cfg: &DropConfig,
    asset: &ReleaseAsset,
    downloads_dir: &Path,
) -> Result<PathBuf> {
    let clean_name = sanitize_zip_name(&asset.name)
        .ok_or_else(|| anyhow!("ignoring non-zip or unsafe asset name: {}", asset.name))?;
    if asset.size == 0 || asset.size > 512 * 1024 * 1024 {
        return Err(anyhow!("asset {clean_name} out of allowed size bounds"));
    }

    let final_path = unique_download_path(downloads_dir, &clean_name);
    let tmp_path = final_path.with_extension("zip.part");
    let asset_url = format!(
        "https://api.github.com/repos/{}/releases/assets/{}",
        cfg.repo.trim().trim_start_matches('/'),
        asset.id
    );

    let dl = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "60",
            "-H",
            "Accept: application/octet-stream",
            "-H",
            "X-GitHub-Api-Version: 2022-11-28",
            "-H",
            &format!("Authorization: Bearer {}", cfg.token.trim()),
            "-o",
            &tmp_path.to_string_lossy(),
            &asset_url,
        ])
        .output()
        .context("failed to download drop asset")?;

    if !dl.status.success() {
        let _ = fs::remove_file(&tmp_path);
        return Err(anyhow!(
            "asset download failed: {}",
            String::from_utf8_lossy(&dl.stderr).trim()
        ));
    }

    let is_zip =
        fs::read(&tmp_path).is_ok_and(|bytes| bytes.starts_with(&[0x50, 0x4B, 0x03, 0x04]));
    if !is_zip {
        let _ = fs::remove_file(&tmp_path);
        return Err(anyhow!("rejected {clean_name}: invalid ZIP magic bytes"));
    }

    fs::rename(&tmp_path, &final_path).context("failed to move ZIP into Downloads")?;

    let _ = Command::new("curl")
        .args([
            "-fsSL",
            "-X",
            "DELETE",
            "--max-time",
            "15",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "X-GitHub-Api-Version: 2022-11-28",
            "-H",
            &format!("Authorization: Bearer {}", cfg.token.trim()),
            &asset_url,
        ])
        .output();

    Ok(final_path)
}

pub fn sync_drop_inbox_blocking() -> Result<usize> {
    let Some(cfg) = load_config() else {
        return Ok(0);
    };
    let downloads_dir =
        dirs::download_dir().ok_or_else(|| anyhow!("could not resolve ~/Downloads"))?;
    fs::create_dir_all(&downloads_dir)?;

    let assets = fetch_release_assets(&cfg)?;
    let mut arrived = 0usize;
    for asset in assets {
        match download_and_delete_asset(&cfg, &asset, &downloads_dir) {
            Ok(saved_path) => {
                arrived += 1;
                let file_label = saved_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("archive.zip");
                let repo_label = infer_repo_label(file_label);
                info!(
                    "drop_inbox: saved {} -> {}",
                    asset.name,
                    saved_path.display()
                );
                macos::notify_agent_alert(
                    &repo_label,
                    "github",
                    0,
                    0,
                    "Archive ZIP reçue dans Téléchargements",
                    file_label,
                );
            }
            Err(err) => warn!("drop_inbox: skipped {}: {err}", asset.name),
        }
    }
    Ok(arrived)
}

pub fn spawn_sync_drop_inbox() {
    if SYNC_IN_FLIGHT
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    tokio::spawn(async move {
        match tokio::task::spawn_blocking(sync_drop_inbox_blocking).await {
            Ok(Err(err)) => warn!("drop_inbox sync error (pass 1): {err:#}"),
            Err(err) => warn!("drop_inbox join error (pass 1): {err}"),
            Ok(Ok(n)) => info!("drop_inbox pass 1 complete: downloaded {n} archive(s)"),
        }
        tokio::time::sleep(Duration::from_secs(12)).await;
        match tokio::task::spawn_blocking(sync_drop_inbox_blocking).await {
            Ok(Err(err)) => warn!("drop_inbox sync error (pass 2): {err:#}"),
            Err(err) => warn!("drop_inbox join error (pass 2): {err}"),
            Ok(Ok(n)) => info!("drop_inbox pass 2 complete: downloaded {n} archive(s)"),
        }
        SYNC_IN_FLIGHT.store(false, Ordering::SeqCst);
    });
}
