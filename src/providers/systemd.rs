use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use tokio::process::Command;

use super::{LogEntry, LogLevel, ProviderType, Service, ServiceProvider, ServiceStatus};

pub struct SystemdProvider;

// ---------------------------------------------------------------------------
// JSON structures from systemctl list-units --output=json
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct UnitJson {
    unit: String,
    #[serde(rename = "active")]
    active_state: String,
    // load, sub fields exist but we don't need them
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn run(args: &[&str]) -> Result<String> {
    let output = Command::new(args[0]).args(&args[1..]).output().await?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn active_to_status(active: &str) -> ServiceStatus {
    match active {
        "active" => ServiceStatus::Running,
        "failed" => ServiceStatus::Degraded,
        _ => ServiceStatus::Stopped,
    }
}

async fn run_systemctl(args: &[&str]) -> Result<()> {
    let mut cmd_args = vec!["--no-ask-password"];
    cmd_args.extend_from_slice(args);
    let output = Command::new("systemctl")
        .args(&cmd_args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = stderr.trim();
        if msg.is_empty() {
            anyhow::bail!(
                "systemctl {} failed (exit {})",
                args.join(" "),
                output.status
            );
        } else {
            anyhow::bail!("{}", msg);
        }
    }
    Ok(())
}

fn is_filtered_systemd_unit(name: &str) -> bool {
    // Keep journald and resolved, drop everything else starting with systemd-
    if name.starts_with("systemd-") {
        return !matches!(
            name,
            "systemd-journald.service" | "systemd-resolved.service"
        );
    }
    false
}

// ---------------------------------------------------------------------------
// Unit detail: PID and memory
// ---------------------------------------------------------------------------

struct UnitDetails {
    pid: Option<u32>,
    memory_bytes: Option<u64>,
}

async fn get_unit_details(unit: &str) -> UnitDetails {
    let output = Command::new("systemctl")
        .args(["show", unit, "--property=MainPID,MemoryCurrent"])
        .output()
        .await;

    let mut pid: Option<u32> = None;
    let mut memory_bytes: Option<u64> = None;

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if let Some(val) = line.strip_prefix("MainPID=") {
                if let Ok(p) = val.trim().parse::<u32>() {
                    if p != 0 {
                        pid = Some(p);
                    }
                }
            } else if let Some(val) = line.strip_prefix("MemoryCurrent=") {
                let val = val.trim();
                // Guard against "[not set]" and u64::MAX sentinel
                if let Ok(m) = val.parse::<u64>() {
                    if m != u64::MAX {
                        memory_bytes = Some(m);
                    }
                }
            }
        }
    }

    UnitDetails { pid, memory_bytes }
}

// ---------------------------------------------------------------------------
// ServiceProvider impl
// ---------------------------------------------------------------------------

impl SystemdProvider {
    pub fn new() -> Self {
        SystemdProvider
    }
}

#[async_trait]
impl ServiceProvider for SystemdProvider {
    async fn list(&self) -> Result<Vec<Service>> {
        let json_str = run(&[
            "systemctl",
            "list-units",
            "--type=service",
            "--all",
            "--output=json",
            "--no-pager",
        ])
        .await?;

        let units: Vec<UnitJson> = serde_json::from_str(json_str.trim()).unwrap_or_default();

        let mut services = Vec::new();
        for unit in units {
            if is_filtered_systemd_unit(&unit.unit) {
                continue;
            }
            let status = active_to_status(&unit.active_state);

            // Hide stopped/inactive services unless they're failed
            if status == ServiceStatus::Stopped {
                continue;
            }

            let name = unit
                .unit
                .strip_suffix(".service")
                .unwrap_or(&unit.unit)
                .to_string();

            // Fetch details only for running services to avoid slow startup
            let (pid, memory_bytes) = if status == ServiceStatus::Running {
                let details = get_unit_details(&unit.unit).await;
                (details.pid, details.memory_bytes)
            } else {
                (None, None)
            };

            services.push(Service {
                id: unit.unit.clone(),
                name,
                status,
                provider: ProviderType::Systemd,
                pid,
                uptime_secs: None, // systemctl JSON doesn't expose uptime easily
                memory_bytes,
                cpu_percent: None,
            });
        }

        Ok(services)
    }

    async fn start(&self, id: &str) -> Result<()> {
        run_systemctl(&["start", id]).await
    }

    async fn stop(&self, id: &str) -> Result<()> {
        run_systemctl(&["stop", id]).await
    }

    async fn restart(&self, id: &str) -> Result<()> {
        run_systemctl(&["restart", id]).await
    }

    async fn logs(&self, id: &str, lines: usize) -> Result<Vec<LogEntry>> {
        let n = lines.to_string();
        let output = Command::new("journalctl")
            .args(["-u", id, "-n", &n, "--no-pager", "--output=short-iso"])
            .output()
            .await?;

        let text = String::from_utf8_lossy(&output.stdout);
        let entries = text.lines().filter_map(parse_log_line).collect();

        Ok(entries)
    }
}

// ---------------------------------------------------------------------------
// Log line parser
// ---------------------------------------------------------------------------

fn detect_level(msg: &str) -> LogLevel {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("error") || lower.contains("fatal") || lower.contains("crit") {
        LogLevel::Error
    } else if lower.contains("warn") {
        LogLevel::Warn
    } else if lower.contains("debug") {
        LogLevel::Debug
    } else {
        LogLevel::Info
    }
}

/// Parse a `short-iso` journalctl line into a LogEntry.
/// Format: `2024-01-15T10:23:45+0000 hostname unit[pid]: message`
fn parse_log_line(line: &str) -> Option<LogEntry> {
    // The timestamp is the first whitespace-delimited token
    let mut iter = line.splitn(3, ' ');
    let timestamp = iter.next()?.to_string();
    let _host_unit = iter.next()?;
    let rest = iter.next().unwrap_or("").to_string();

    // Strip "unit[pid]: " prefix from rest if present
    let message = if let Some(pos) = rest.find(": ") {
        rest[pos + 2..].to_string()
    } else {
        rest
    };

    let level = detect_level(&message);

    Some(LogEntry {
        timestamp,
        level,
        message,
    })
}
