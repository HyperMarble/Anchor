//! Self-updater — checks for new versions and updates the binary.

use anyhow::Result;
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::PathBuf;

/// Current version from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// GitHub repository for releases
const GITHUB_REPO: &str = "Tharun-10Dragneel/Anchor";

/// GitHub release API response
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

/// Check if a newer version is available.
/// Returns Some(version) if update available, None if current.
pub fn check_for_update() -> Option<String> {
    let latest = get_latest_version().ok()?;
    let latest_clean = latest.trim_start_matches('v');

    if version_is_newer(latest_clean, VERSION) {
        Some(latest)
    } else {
        None
    }
}

/// Get the latest version from GitHub releases (includes pre-releases).
fn get_latest_version() -> Result<String> {
    let url = format!("https://api.github.com/repos/{}/releases", GITHUB_REPO);

    let client = reqwest::blocking::Client::builder()
        .user_agent("anchor-updater")
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let response = client.get(&url).send()?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!("No releases found"));
    }

    let releases: Vec<GitHubRelease> = response.json()?;
    releases
        .first()
        .map(|r| r.tag_name.clone())
        .ok_or_else(|| anyhow::anyhow!("No releases found"))
}

/// Compare version strings (simple semver comparison).
fn version_is_newer(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|s| s.parse().ok())
            .collect()
    };

    let latest_parts = parse(latest);
    let current_parts = parse(current);

    for i in 0..3 {
        let l = latest_parts.get(i).copied().unwrap_or(0);
        let c = current_parts.get(i).copied().unwrap_or(0);
        if l > c {
            return true;
        }
        if l < c {
            return false;
        }
    }
    false
}

/// Download and install the latest version.
pub fn update() -> Result<()> {
    println!("Checking for updates...");

    let url = format!("https://api.github.com/repos/{}/releases", GITHUB_REPO);

    let client = reqwest::blocking::Client::builder()
        .user_agent("anchor-updater")
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let response = client.get(&url).send()?;

    if response.status() == 404 {
        println!("No releases found. You're on the latest version (v{}).", VERSION);
        return Ok(());
    }

    let releases: Vec<GitHubRelease> = response.json().map_err(|_| {
        anyhow::anyhow!("No releases available yet. You're on v{}.", VERSION)
    })?;

    let release = releases.first().ok_or_else(|| {
        anyhow::anyhow!("No releases available yet. You're on v{}.", VERSION)
    })?;

    // Check if we're already on the latest version
    let latest_clean = release.tag_name.trim_start_matches('v');
    if !version_is_newer(latest_clean, VERSION) {
        println!("Already on latest version (v{}).", VERSION);
        return Ok(());
    }

    // Determine which asset to download based on platform
    let asset_name = get_asset_name();
    let asset = release.assets.iter()
        .find(|a| a.name.contains(&asset_name))
        .ok_or_else(|| anyhow::anyhow!("No compatible release found for this platform"))?;

    println!("Downloading {}...", release.tag_name);

    // Download to temp file
    let response = client.get(&asset.browser_download_url).send()?;
    let bytes = response.bytes()?;

    // Extract if tar.gz
    let exe_path = std::env::current_exe()?;
    let temp_dir = env::temp_dir().join("anchor-update");
    fs::create_dir_all(&temp_dir)?;

    if asset.name.ends_with(".tar.gz") {
        let tar_path = temp_dir.join(&asset.name);
        fs::write(&tar_path, &bytes)?;

        // Extract
        let tar_gz = fs::File::open(&tar_path)?;
        let tar = flate2::read::GzDecoder::new(tar_gz);
        let mut archive = tar::Archive::new(tar);
        archive.unpack(&temp_dir)?;

        // Find the anchor binary
        let new_binary = temp_dir.join("anchor");
        if !new_binary.exists() {
            return Err(anyhow::anyhow!("anchor binary not found in archive"));
        }

        // Replace current binary
        replace_binary(&new_binary, &exe_path)?;
    } else {
        // Direct binary download
        let new_binary = temp_dir.join("anchor-new");
        fs::write(&new_binary, &bytes)?;
        replace_binary(&new_binary, &exe_path)?;
    }

    // Cleanup
    let _ = fs::remove_dir_all(&temp_dir);

    println!("Updated to {}!", release.tag_name);
    Ok(())
}

/// Get the asset name for the current platform.
fn get_asset_name() -> String {
    let os = env::consts::OS;
    let arch = env::consts::ARCH;

    match (os, arch) {
        ("macos", "aarch64") => "anchor-macos-arm".to_string(),
        ("macos", "x86_64") => "anchor-macos-intel".to_string(),
        ("linux", "x86_64") => "anchor-linux-x64".to_string(),
        _ => format!("anchor-{}-{}", os, arch),
    }
}

/// Replace the current binary with the new one.
fn replace_binary(new: &PathBuf, current: &PathBuf) -> Result<()> {
    // On Unix, we can replace a running binary by renaming
    let backup = current.with_extension("old");

    // Remove old backup if exists
    let _ = fs::remove_file(&backup);

    // Rename current to backup
    fs::rename(current, &backup)?;

    // Copy new to current location
    fs::copy(new, current)?;

    // Set executable permission on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(current, fs::Permissions::from_mode(0o755))?;
    }

    // Remove backup
    let _ = fs::remove_file(&backup);

    Ok(())
}

/// Print update notification if available (non-blocking check).
pub fn notify_if_update_available() {
    // Run check in background to not slow down CLI
    std::thread::spawn(|| {
        if let Some(version) = check_for_update() {
            eprintln!("\n  New version available: {}. Run 'anchor update' to upgrade.\n", version);
        }
    });
}
