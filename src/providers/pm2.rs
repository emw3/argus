use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::Command;

use super::{LogEntry, LogLevel, ProviderType, Service, ServiceProvider, ServiceStatus};

pub struct Pm2Provider;

impl Pm2Provider {
    pub fn new() -> Self {
        Pm2Provider
    }

    pub async fn is_available() -> bool {
        Command::new("pm2")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// JSON structures from `pm2 jlist`
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Pm2Process {
    name: String,
    pm_id: u32,
    pid: Option<u32>,
    pm2_env: Pm2Env,
    monit: Option<Pm2Monit>,
}

#[derive(Debug, Deserialize)]
struct Pm2Env {
    status: Option<String>,
    pm_uptime: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct Pm2Monit {
    memory: Option<u64>,
    cpu: Option<f64>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn pm2_status_to_service_status(status: &str) -> ServiceStatus {
    match status {
        "online" => ServiceStatus::Running,
        "stopped" => ServiceStatus::Stopped,
        "errored" => ServiceStatus::Degraded,
        _ => ServiceStatus::Unknown,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

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

// ---------------------------------------------------------------------------
// ServiceProvider impl
// ---------------------------------------------------------------------------

#[async_trait]
impl ServiceProvider for Pm2Provider {
    async fn list(&self) -> Result<Vec<Service>> {
        let output = Command::new("pm2").arg("jlist").output().await?;

        let json_str = String::from_utf8_lossy(&output.stdout).into_owned();
        let processes: Vec<Pm2Process> = serde_json::from_str(json_str.trim()).unwrap_or_default();

        let current_ms = now_ms();

        let services = processes
            .into_iter()
            .map(|p| {
                let status_str = p.pm2_env.status.as_deref().unwrap_or("unknown");
                let status = pm2_status_to_service_status(status_str);

                // uptime: pm_uptime is epoch ms of when the process started
                let uptime_secs = if status == ServiceStatus::Running {
                    p.pm2_env.pm_uptime.and_then(|start_ms| {
                        if current_ms > start_ms {
                            Some((current_ms - start_ms) / 1000)
                        } else {
                            None
                        }
                    })
                } else {
                    None
                };

                let (memory_bytes, cpu_percent) =
                    p.monit.map(|m| (m.memory, m.cpu)).unwrap_or((None, None));

                Service {
                    id: p.pm_id.to_string(),
                    name: p.name,
                    status,
                    provider: ProviderType::Pm2,
                    pid: p.pid.filter(|&pid| pid != 0),
                    uptime_secs,
                    memory_bytes,
                    cpu_percent,
                }
            })
            .collect();

        Ok(services)
    }

    async fn start(&self, id: &str) -> Result<()> {
        let status = Command::new("pm2").args(["start", id]).status().await?;
        if !status.success() {
            anyhow::bail!("pm2 start {} failed", id);
        }
        Ok(())
    }

    async fn stop(&self, id: &str) -> Result<()> {
        let status = Command::new("pm2").args(["stop", id]).status().await?;
        if !status.success() {
            anyhow::bail!("pm2 stop {} failed", id);
        }
        Ok(())
    }

    async fn restart(&self, id: &str) -> Result<()> {
        let status = Command::new("pm2").args(["restart", id]).status().await?;
        if !status.success() {
            anyhow::bail!("pm2 restart {} failed", id);
        }
        Ok(())
    }

    async fn logs(&self, id: &str, lines: usize) -> Result<Vec<LogEntry>> {
        let n = lines.to_string();
        let output = Command::new("pm2")
            .args(["logs", id, "--lines", &n, "--nostream", "--raw"])
            .output()
            .await?;

        let text = String::from_utf8_lossy(&output.stdout).into_owned();

        let entries = text
            .lines()
            .filter(|l| !l.is_empty())
            .map(|line| {
                let level = detect_level(line);
                LogEntry {
                    timestamp: String::new(),
                    level,
                    message: line.to_string(),
                }
            })
            .collect();

        Ok(entries)
    }
}
