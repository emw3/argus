use anyhow::Result;
use async_trait::async_trait;
use std::fmt;

pub mod docker;
pub mod pm2;
pub mod systemd;

// ---------------------------------------------------------------------------
// ProviderType
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderType {
    Systemd,
    Docker,
    Pm2,
}

impl fmt::Display for ProviderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderType::Systemd => write!(f, "Systemd"),
            ProviderType::Docker => write!(f, "Docker"),
            ProviderType::Pm2 => write!(f, "PM2"),
        }
    }
}

// ---------------------------------------------------------------------------
// ServiceStatus
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceStatus {
    Running,
    Stopped,
    Degraded,
    Unknown,
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Service {
    pub id: String,
    pub name: String,
    pub status: ServiceStatus,
    pub provider: ProviderType,
    pub pid: Option<u32>,
    pub uptime_secs: Option<u64>,
    pub memory_bytes: Option<u64>,
    pub cpu_percent: Option<f64>,
}

// ---------------------------------------------------------------------------
// LogLevel
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

// ---------------------------------------------------------------------------
// LogEntry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
}

// ---------------------------------------------------------------------------
// ServiceProvider trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait ServiceProvider: Send + Sync {
    async fn list(&self) -> Result<Vec<Service>>;
    async fn start(&self, id: &str) -> Result<()>;
    async fn stop(&self, id: &str) -> Result<()>;
    async fn restart(&self, id: &str) -> Result<()>;
    async fn logs(&self, id: &str, lines: usize) -> Result<Vec<LogEntry>>;
}
