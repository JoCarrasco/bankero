use anyhow::{Context, Result, anyhow};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::blocking::Client;
use semver::Version;
use serde::Deserialize;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::cli::UpgradeArgs;

const GITHUB_REPO: &str = "JoCarrasco/bankero";

#[derive(Debug, Deserialize)]
struct LatestRelease {
    tag_name: String,
    html_url: Option<String>,
}

pub fn handle_upgrade(args: UpgradeArgs) -> Result<()> {
    let current = Version::parse(env!("CARGO_PKG_VERSION"))
        .context("Invalid current version (CARGO_PKG_VERSION)")?;

    let latest = if args.skip_check {
        None
    } else {
        Some(fetch_latest_release()?)
    };

    if let Some(latest) = &latest {
        let latest_version = parse_tag_version(&latest.tag_name)?;
        println!("Current: v{current}");
        println!("Latest:  {}", latest.tag_name);
        if let Some(url) = latest.html_url.as_deref() {
            println!("Release: {url}");
        }

        if latest_version <= current {
            println!("Already up to date.");
            return Ok(());
        }

        println!("Update available: v{current} -> {}", latest.tag_name);
    } else {
        println!("Current: v{current}");
        println!("(Skipping remote check; running local upgrade path.)");
    }

    if !args.apply {
        print_upgrade_instructions(&args);
        return Ok(());
    }

    ensure_apt_available()?;

    if args.setup_apt {
        setup_apt_repo(&args)?;
    } else {
        let keyring_path = Path::new(&args.keyring_path);
        let sources_path = Path::new(&args.sources_path);
        if !keyring_path.exists() || !sources_path.exists() {
            println!("APT repo is not configured yet.");
            print_upgrade_instructions(&args);
            return Ok(());
        }
    }

    run_apt_upgrade(args.yes)
}

fn fetch_latest_release() -> Result<LatestRelease> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb.set_message("Checking GitHub for latest release...");

    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let client = Client::builder()
        .build()
        .context("Failed to build HTTP client")?;
    let resp = client
        .get(url)
        .header("User-Agent", "bankero-upgrade")
        .header("Accept", "application/vnd.github+json")
        .send()
        .context("Failed to request latest release")?;

    if !resp.status().is_success() {
        pb.finish_and_clear();
        return Err(anyhow!(
            "GitHub latest release request failed: HTTP {}",
            resp.status()
        ));
    }

    let parsed: LatestRelease = resp.json().context("Invalid GitHub release JSON")?;
    pb.finish_and_clear();
    Ok(parsed)
}

fn parse_tag_version(tag: &str) -> Result<Version> {
    let raw = tag.trim();
    let raw = raw.strip_prefix('v').unwrap_or(raw);
    Version::parse(raw).with_context(|| format!("Invalid release tag version: {tag}"))
}

fn print_upgrade_instructions(args: &UpgradeArgs) {
    println!();
    println!("To configure APT + upgrade:");
    println!(
        "  bankero upgrade --setup-apt --apply{}",
        if args.yes { " --yes" } else { "" }
    );
    println!();
    println!("Manual steps (Debian/Ubuntu):");
    println!(
        "  curl -fsSL {}/public.gpg | sudo gpg --dearmor -o {}",
        args.repo_url, args.keyring_path
    );
    println!(
        "  echo \"deb [signed-by={}] {} {} {}\" | sudo tee {}",
        args.keyring_path, args.repo_url, args.suite, args.component, args.sources_path
    );
    println!("  sudo apt-get update");
    println!("  sudo apt-get install bankero");
}

fn ensure_apt_available() -> Result<()> {
    let has_apt = Command::new("apt-get")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok();
    if !has_apt {
        return Err(anyhow!(
            "apt-get not found. The built-in upgrader currently supports Debian/Ubuntu via APT only."
        ));
    }

    let has_sudo = Command::new("sudo")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok();
    if !has_sudo {
        return Err(anyhow!(
            "sudo not found. Re-run as root or install sudo to use the upgrader."
        ));
    }

    Ok(())
}

fn setup_apt_repo(args: &UpgradeArgs) -> Result<()> {
    let keyring_path = Path::new(&args.keyring_path);
    let sources_path = Path::new(&args.sources_path);

    if keyring_path.exists() {
        println!("Keyring already exists: {}", keyring_path.display());
    } else {
        install_keyring(args)?;
    }

    if sources_path.exists() {
        println!("APT source already exists: {}", sources_path.display());
    } else {
        write_sources_list(args)?;
    }

    Ok(())
}

fn install_keyring(args: &UpgradeArgs) -> Result<()> {
    let url = format!("{}/public.gpg", args.repo_url.trim_end_matches('/'));
    let client = Client::builder()
        .build()
        .context("Failed to build HTTP client")?;

    let resp = client
        .get(url)
        .header("User-Agent", "bankero-upgrade")
        .send()
        .context("Failed to download public.gpg")?;

    if !resp.status().is_success() {
        return Err(anyhow!(
            "Failed to download public.gpg: HTTP {}",
            resp.status()
        ));
    }

    let total = resp.content_length().unwrap_or(0);

    let pb = if total > 0 {
        ProgressBar::new(total)
    } else {
        ProgressBar::new_spinner()
    };

    pb.set_style(
        ProgressStyle::with_template("{spinner} {msg} {bytes}/{total_bytes}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    if total == 0 {
        pb.enable_steady_tick(std::time::Duration::from_millis(80));
    }
    pb.set_message("Downloading signing key...");

    let mut cmd = Command::new("sudo");
    cmd.arg("gpg")
        .arg("--dearmor")
        .arg("-o")
        .arg(&args.keyring_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let mut child = cmd.spawn().context("Failed to run sudo gpg --dearmor")?;
    let mut stdin = child.stdin.take().context("Failed to open stdin for gpg")?;

    let mut reader = resp;
    let mut buf = [0u8; 16 * 1024];
    loop {
        let n = std::io::Read::read(&mut reader, &mut buf)
            .context("Failed reading key download stream")?;
        if n == 0 {
            break;
        }
        stdin
            .write_all(&buf[..n])
            .context("Failed writing key to gpg")?;
        pb.inc(n as u64);
    }
    drop(stdin);

    let status = child.wait().context("Failed waiting for gpg")?;
    pb.finish_and_clear();

    if !status.success() {
        return Err(anyhow!(
            "Failed to install keyring (gpg exited with {status})"
        ));
    }

    Ok(())
}

fn write_sources_list(args: &UpgradeArgs) -> Result<()> {
    let line = format!(
        "deb [signed-by={}] {} {} {}\n",
        args.keyring_path, args.repo_url, args.suite, args.component
    );

    println!("Writing APT source: {}", args.sources_path);
    let mut child = Command::new("sudo")
        .arg("tee")
        .arg(&args.sources_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .context("Failed to run sudo tee")?;

    {
        let mut stdin = child.stdin.take().context("Failed to open stdin for tee")?;
        stdin
            .write_all(line.as_bytes())
            .context("Failed to write to sudo tee")?;
    }

    let status = child.wait().context("Failed waiting for tee")?;
    if !status.success() {
        return Err(anyhow!(
            "Failed to write sources list (tee exited with {status})"
        ));
    }

    Ok(())
}

fn run_apt_upgrade(assume_yes: bool) -> Result<()> {
    println!("Running: sudo apt-get update");
    let status = Command::new("sudo")
        .arg("apt-get")
        .arg("update")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("Failed to run apt-get update")?;
    if !status.success() {
        return Err(anyhow!("apt-get update failed: {status}"));
    }

    println!(
        "Running: sudo apt-get install bankero{}",
        if assume_yes { " -y" } else { "" }
    );
    let mut cmd = Command::new("sudo");
    cmd.arg("apt-get").arg("install");
    if assume_yes {
        cmd.arg("-y");
    }
    cmd.arg("bankero")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = cmd.status().context("Failed to run apt-get install")?;
    if !status.success() {
        return Err(anyhow!("apt-get install failed: {status}"));
    }

    Ok(())
}
