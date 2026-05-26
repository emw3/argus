use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use super::{LogEntry, LogLevel, ProviderType, Service, ServiceProvider, ServiceStatus};

pub struct DockerProvider;

impl DockerProvider {
    pub fn new() -> Self {
        DockerProvider
    }

    pub async fn is_available() -> bool {
        UnixStream::connect("/var/run/docker.sock").await.is_ok()
    }
}

// ---------------------------------------------------------------------------
// JSON structures from Docker API
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct ContainerJson {
    Id: String,
    Names: Vec<String>,
    State: String,
}

// ---------------------------------------------------------------------------
// HTTP helpers over Unix socket
// ---------------------------------------------------------------------------

const DOCKER_SOCK: &str = "/var/run/docker.sock";

async fn docker_get(path: &str) -> Result<String> {
    let mut stream = UnixStream::connect(DOCKER_SOCK).await?;

    let request = format!(
        "GET {} HTTP/1.0\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        path
    );
    stream.write_all(request.as_bytes()).await?;

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;

    let response = String::from_utf8_lossy(&buf).into_owned();

    // Split headers from body
    if let Some(pos) = response.find("\r\n\r\n") {
        Ok(response[pos + 4..].to_string())
    } else {
        Ok(response)
    }
}

async fn docker_post(path: &str) -> Result<String> {
    let mut stream = UnixStream::connect(DOCKER_SOCK).await?;

    let request = format!(
        "POST {} HTTP/1.0\r\nHost: localhost\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        path
    );
    stream.write_all(request.as_bytes()).await?;

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;

    let response = String::from_utf8_lossy(&buf).into_owned();

    if let Some(pos) = response.find("\r\n\r\n") {
        Ok(response[pos + 4..].to_string())
    } else {
        Ok(response)
    }
}

// ---------------------------------------------------------------------------
// Docker log frame stripping
// Docker multiplexed log format: 8-byte header per frame
//   byte 0: stream type (1=stdout, 2=stderr)
//   bytes 1-3: padding (zeros)
//   bytes 4-7: frame size (big-endian u32)
//   followed by <size> bytes of payload
// ---------------------------------------------------------------------------

fn strip_docker_log_frames(raw: &[u8]) -> String {
    let mut out = String::new();
    let mut pos = 0;
    while pos + 8 <= raw.len() {
        let size =
            u32::from_be_bytes([raw[pos + 4], raw[pos + 5], raw[pos + 6], raw[pos + 7]]) as usize;
        pos += 8;
        if pos + size > raw.len() {
            // Truncated frame — take what we have
            let payload = &raw[pos..raw.len()];
            out.push_str(&String::from_utf8_lossy(payload));
            break;
        }
        let payload = &raw[pos..pos + size];
        out.push_str(&String::from_utf8_lossy(payload));
        pos += size;
    }
    out
}

fn docker_state_to_status(state: &str) -> ServiceStatus {
    match state {
        "running" => ServiceStatus::Running,
        "dead" | "restarting" => ServiceStatus::Degraded,
        _ => ServiceStatus::Stopped,
    }
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
impl ServiceProvider for DockerProvider {
    async fn list(&self) -> Result<Vec<Service>> {
        let body = docker_get("/containers/json?all=true").await?;

        let containers: Vec<ContainerJson> = serde_json::from_str(body.trim()).unwrap_or_default();

        let services = containers
            .into_iter()
            .map(|c| {
                // Strip leading '/' from name
                let name = c
                    .Names
                    .first()
                    .map(|n| n.trim_start_matches('/').to_string())
                    .unwrap_or_else(|| c.Id[..12].to_string());

                let status = docker_state_to_status(&c.State);

                Service {
                    id: c.Id.clone(),
                    name,
                    status,
                    provider: ProviderType::Docker,
                    pid: None,
                    uptime_secs: None,
                    memory_bytes: None,
                    cpu_percent: None,
                }
            })
            .collect();

        Ok(services)
    }

    async fn start(&self, id: &str) -> Result<()> {
        docker_post(&format!("/containers/{}/start", id)).await?;
        Ok(())
    }

    async fn stop(&self, id: &str) -> Result<()> {
        docker_post(&format!("/containers/{}/stop", id)).await?;
        Ok(())
    }

    async fn restart(&self, id: &str) -> Result<()> {
        docker_post(&format!("/containers/{}/restart", id)).await?;
        Ok(())
    }

    async fn logs(&self, id: &str, lines: usize) -> Result<Vec<LogEntry>> {
        let path = format!(
            "/containers/{}/logs?stdout=true&stderr=true&tail={}",
            id, lines
        );

        let mut stream = UnixStream::connect(DOCKER_SOCK).await?;
        let request = format!(
            "GET {} HTTP/1.0\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            path
        );
        stream.write_all(request.as_bytes()).await?;

        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await?;

        // Split off HTTP headers (find \r\n\r\n)
        let body_start = buf
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .map(|p| p + 4)
            .unwrap_or(0);
        let body = &buf[body_start..];

        let text = strip_docker_log_frames(body);

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
