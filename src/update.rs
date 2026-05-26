use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use tokio::process::Command;

const REPO: &str = "https://github.com/emw3/argus";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub async fn run() -> Result<()> {
    let current_exe = std::env::current_exe().context("cannot determine binary path")?;
    check_write_permission(&current_exe)?;

    let target = detect_target().await?;
    println!("Current version: {}", CURRENT_VERSION);

    let latest = get_latest_version().await?;
    println!("Latest version:  {}", latest);

    if latest == CURRENT_VERSION {
        println!("Already up to date.");
        return Ok(());
    }

    println!("Downloading argus v{}...", latest);
    let tmp = tempdir(&current_exe)?;
    download_and_verify(&tmp, &target, &latest).await?;
    replace_binary(&tmp, &current_exe)?;

    println!("Updated to v{}", latest);
    Ok(())
}

fn check_write_permission(exe_path: &Path) -> Result<()> {
    let parent = exe_path
        .parent()
        .context("cannot determine binary directory")?;
    let test_path = parent.join(".argus-update-check");
    match std::fs::File::create(&test_path) {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_path);
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            bail!(
                "Permission denied writing to {}. Try: sudo argus update",
                parent.display()
            );
        }
        Err(e) => Err(e).context(format!("cannot write to {}", parent.display())),
    }
}

async fn detect_target() -> Result<String> {
    let output = Command::new("uname")
        .arg("-m")
        .output()
        .await
        .context("failed to run uname")?;
    let arch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    match arch.as_str() {
        "x86_64" => Ok("x86_64-unknown-linux-musl".to_string()),
        "aarch64" | "arm64" => Ok("aarch64-unknown-linux-musl".to_string()),
        other => bail!("Unsupported architecture: {}", other),
    }
}

async fn get_latest_version() -> Result<String> {
    let output = Command::new("curl")
        .args(["-sI", &format!("{}/releases/latest", REPO)])
        .output()
        .await
        .context("failed to run curl")?;
    let headers = String::from_utf8_lossy(&output.stdout);
    for line in headers.lines() {
        let lower = line.to_lowercase();
        if lower.starts_with("location:") {
            let url = line.split_once(':').unwrap().1.trim();
            if let Some(tag) = url.rsplit('/').next() {
                return Ok(tag.trim_start_matches('v').to_string());
            }
        }
    }
    bail!("Could not determine latest version. Check {}/releases", REPO);
}

fn tempdir(exe_path: &Path) -> Result<PathBuf> {
    // Place temp dir adjacent to the binary so rename stays on the same filesystem
    let parent = exe_path
        .parent()
        .context("cannot determine binary directory")?;
    let dir = parent.join(format!(".argus-update-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

async fn download_and_verify(tmp: &Path, target: &str, version: &str) -> Result<()> {
    let tarball_name = format!("argus-{}-v{}.tar.gz", target, version);
    let tarball_url = format!("{}/releases/download/v{}/{}", REPO, version, tarball_name);
    let sha_url = format!("{}.sha256", tarball_url);

    let tarball_path = tmp.join(&tarball_name);
    let sha_path = tmp.join(format!("{}.sha256", tarball_name));

    // Download tarball
    let status = Command::new("curl")
        .args(["-sSfL", &tarball_url, "-o"])
        .arg(&tarball_path)
        .status()
        .await
        .context("failed to run curl")?;
    if !status.success() {
        bail!("Download failed. Check {}/releases", REPO);
    }

    // Download checksum
    let status = Command::new("curl")
        .args(["-sSfL", &sha_url, "-o"])
        .arg(&sha_path)
        .status()
        .await
        .context("failed to download checksum")?;
    if !status.success() {
        bail!("Checksum download failed");
    }

    // Verify checksum: extract expected hash from .sha256 file, compare with actual
    let expected_line = std::fs::read_to_string(&sha_path).context("failed to read checksum file")?;
    let expected_hash = expected_line
        .split_whitespace()
        .next()
        .context("checksum file is empty")?;

    let output = Command::new("sha256sum")
        .arg(&tarball_path)
        .output()
        .await
        .context("failed to run sha256sum")?;
    let actual_hash = String::from_utf8_lossy(&output.stdout);
    let actual_hash = actual_hash
        .split_whitespace()
        .next()
        .unwrap_or_default();

    if actual_hash != expected_hash {
        bail!("Checksum verification failed — download may be corrupted");
    }

    println!("Checksum verified.");

    // Extract
    let status = Command::new("tar")
        .args(["-xzf"])
        .arg(&tarball_path)
        .arg("-C")
        .arg(tmp)
        .status()
        .await
        .context("failed to extract tarball")?;
    if !status.success() {
        bail!("Failed to extract tarball");
    }

    Ok(())
}

fn replace_binary(tmp: &Path, current_exe: &Path) -> Result<()> {
    let new_binary = tmp.join("argus");
    if !new_binary.exists() {
        bail!("Extracted archive does not contain 'argus' binary");
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&new_binary, std::fs::Permissions::from_mode(0o755))?;
    }

    std::fs::rename(&new_binary, current_exe)
        .context("failed to replace binary — try: sudo argus update")?;

    // Clean up temp dir
    let _ = std::fs::remove_dir_all(tmp);
    Ok(())
}
