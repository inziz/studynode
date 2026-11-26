//! Automatic update checker and self-updater backed by GitHub Releases.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use rune::Waker;
use serde::Deserialize;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPO_RELEASES_API: &str =
    "https://api.github.com/repos/inziz/studynode/releases/latest";
pub const MIN_BINARY_BYTES: u64 = 512 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateInfo {
    pub version: String,
    pub tag_name: String,
    pub release_url: String,
    pub asset_name: String,
    pub download_url: String,
    pub notes: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(UpdateInfo),
    Downloading(UpdateInfo),
    ReadyToRestart(String),
    Failed(String),
}

pub enum UpdateJobResult {
    Checked(Result<Option<UpdateInfo>, String>),
    Installed(Result<String, String>),
}

#[derive(Debug, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct LatestReleaseResponse {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

/// Parse a `major.minor[.patch]` semver string, tolerating a leading `v`/`V` and trailing
/// `-prerelease`/`+build` metadata.
pub fn parse_semver(raw: &str) -> Option<(u64, u64, u64)> {
    let trimmed = raw.trim().trim_start_matches(['v', 'V']);
    let core = trimmed
        .split(['-', '+'])
        .next()
        .unwrap_or("")
        .trim();
    if core.is_empty() {
        return None;
    }
    let mut parts = core.split('.');
    let major: u64 = parts.next()?.parse().ok()?;
    let minor: u64 = parts.next()?.parse().ok()?;
    let patch: u64 = match parts.next() {
        Some(p) => p.parse().ok()?,
        None => 0,
    };
    Some((major, minor, patch))
}

/// Returns `true` when `latest` represents a strictly newer version than `current`.
pub fn is_newer_version(latest: &str, current: &str) -> bool {
    match (parse_semver(latest), parse_semver(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// Pick the standalone Windows x86_64 app executable asset from a GitHub Release,
/// skipping setup/installer executables so self-replacement updates `studynodes.exe` directly.
pub fn select_windows_binary_asset(assets: &[ReleaseAsset]) -> Option<&ReleaseAsset> {
    assets.iter().find(|a| {
        let lower = a.name.to_ascii_lowercase();
        lower.ends_with(".exe")
            && !lower.contains("setup")
            && !lower.contains("installer")
            && (lower.contains("windows") || lower.contains("studynodes"))
    })
}

/// Validate that downloaded bytes are a plausible Windows PE executable (`MZ` magic + minimum size).
pub fn validate_pe_binary(bytes: &[u8]) -> Result<(), String> {
    if (bytes.len() as u64) < MIN_BINARY_BYTES {
        return Err(format!(
            "Downloaded update binary is unexpectedly small ({} bytes)",
            bytes.len()
        ));
    }
    if !bytes.starts_with(b"MZ") {
        return Err("Downloaded update binary does not have a valid Windows PE header".into());
    }
    Ok(())
}

/// Delete any leftover `<exe>.old` binary left behind by a previous atomic self-update.
pub fn cleanup_old_binary() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let old_path = old_binary_path(&exe);
    if old_path.exists() {
        let _ = std::fs::remove_file(&old_path);
    }
}

fn old_binary_path(exe: &std::path::Path) -> PathBuf {
    let mut os = exe.as_os_str().to_os_string();
    os.push(".old");
    PathBuf::from(os)
}

fn new_binary_path(exe: &std::path::Path) -> PathBuf {
    let mut os = exe.as_os_str().to_os_string();
    os.push(".new");
    PathBuf::from(os)
}

fn build_http_client(timeout_secs: u64) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(format!("StudyNodes/{CURRENT_VERSION}"))
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))
}

async fn fetch_latest_release() -> Result<Option<UpdateInfo>, String> {
    let client = build_http_client(12)?;
    let resp = client
        .get(REPO_RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("Could not check for updates: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("GitHub Releases returned HTTP {}", resp.status()));
    }

    let release: LatestReleaseResponse = resp
        .json()
        .await
        .map_err(|e| format!("Invalid release response: {e}"))?;

    if release.draft || release.prerelease {
        return Ok(None);
    }
    if !is_newer_version(&release.tag_name, CURRENT_VERSION) {
        return Ok(None);
    }

    let Some(asset) = select_windows_binary_asset(&release.assets) else {
        return Ok(None);
    };

    let version = release.tag_name.trim().trim_start_matches(['v', 'V']).to_string();
    Ok(Some(UpdateInfo {
        version,
        tag_name: release.tag_name,
        release_url: release.html_url,
        asset_name: asset.name.clone(),
        download_url: asset.browser_download_url.clone(),
        notes: release.body,
    }))
}

async fn download_and_swap_binary(info: &UpdateInfo) -> Result<String, String> {
    if !cfg!(target_os = "windows") {
        return Err("Automatic binary self-update is currently supported on Windows only".into());
    }

    let client = build_http_client(180)?;
    let resp = client
        .get(&info.download_url)
        .send()
        .await
        .map_err(|e| format!("Update download failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Update download returned HTTP {}", resp.status()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed reading update payload: {e}"))?;
    validate_pe_binary(&bytes)?;

    let exe_path =
        std::env::current_exe().map_err(|e| format!("Could not resolve current executable: {e}"))?;
    let new_path = new_binary_path(&exe_path);
    let old_path = old_binary_path(&exe_path);

    std::fs::write(&new_path, &bytes)
        .map_err(|e| format!("Could not write update binary ({}): {e}", new_path.display()))?;

    if old_path.exists() {
        let _ = std::fs::remove_file(&old_path);
    }

    if let Err(e) = std::fs::rename(&exe_path, &old_path) {
        let _ = std::fs::remove_file(&new_path);
        return Err(format!("Could not stage existing executable for swap: {e}"));
    }

    if let Err(e) = std::fs::rename(&new_path, &exe_path) {
        // Roll back immediately so the user is never left without an executable.
        let _ = std::fs::rename(&old_path, &exe_path);
        let _ = std::fs::remove_file(&new_path);
        return Err(format!("Could not install updated executable: {e}"));
    }

    let _ = std::process::Command::new(&exe_path).spawn();
    Ok(info.version.clone())
}

/// Run a non-blocking GitHub Releases check on a background thread.
pub fn spawn_update_check(tx: mpsc::Sender<UpdateJobResult>, waker: Option<Waker>) {
    let _ = std::thread::Builder::new()
        .name("studynodes-update-check".into())
        .spawn(move || {
            let result = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt.block_on(fetch_latest_release()),
                Err(e) => Err(format!("Runtime init failed: {e}")),
            };
            let _ = tx.send(UpdateJobResult::Checked(result));
            if let Some(w) = waker {
                w();
            }
        });
}

/// Download and atomically swap the running executable, then relaunch and exit cleanly.
pub fn spawn_download_and_apply(
    info: UpdateInfo,
    tx: mpsc::Sender<UpdateJobResult>,
    waker: Option<Waker>,
) {
    let _ = std::thread::Builder::new()
        .name("studynodes-update-apply".into())
        .spawn(move || {
            let result = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt.block_on(download_and_swap_binary(&info)),
                Err(e) => Err(format!("Runtime init failed: {e}")),
            };
            let succeeded = result.is_ok();
            let _ = tx.send(UpdateJobResult::Installed(result));
            if let Some(w) = waker {
                w();
            }
            if succeeded {
                std::thread::sleep(Duration::from_millis(300));
                std::process::exit(0);
            }
        });
}
