use anyhow::Result;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct PortInfo {
    pub port: u16,
    pub bind_address: String,
    pub pid: Option<u32>,
    pub process_name: String,
    pub cmdline: String,
}

pub async fn scan() -> Vec<PortInfo> {
    scan_inner().await.unwrap_or_default()
}

async fn scan_inner() -> Result<Vec<PortInfo>> {
    let output = Command::new("ss")
        .args(["-tlnp"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut by_port_pid: HashMap<(u16, Option<u32>), PortInfo> = HashMap::new();

    for line in stdout.lines().skip(1) {
        if let Some(info) = parse_ss_line(line) {
            let key = (info.port, info.pid);
            match by_port_pid.entry(key) {
                Entry::Vacant(e) => {
                    e.insert(info);
                }
                Entry::Occupied(mut e) if e.get().bind_address != info.bind_address => {
                    e.get_mut().bind_address = "*".to_string();
                }
                _ => {}
            }
        }
    }

    let mut ports: Vec<PortInfo> = by_port_pid.into_values().collect();

    for port in &mut ports {
        if let Some(pid) = port.pid {
            if let Ok(cmd) = read_cmdline(pid).await {
                port.cmdline = cmd;
            }
        }
    }

    ports.sort_by_key(|p| p.port);
    Ok(ports)
}

fn parse_ss_line(line: &str) -> Option<PortInfo> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }

    let local_addr = parts[3];
    let port = parse_port(local_addr)?;
    let bind_address = parse_bind_address(local_addr);

    // Process info may contain spaces, so join parts[5..] back together
    let (pid, process_name) = if parts.len() >= 6 {
        let process_field = parts[5..].join(" ");
        parse_process_info(&process_field)
    } else {
        (None, "(unknown)".to_string())
    };

    Some(PortInfo {
        port,
        bind_address,
        pid,
        process_name,
        cmdline: String::new(),
    })
}

fn parse_port(addr: &str) -> Option<u16> {
    // Handle [::]:port and addr:port formats
    if let Some(bracket_pos) = addr.rfind("]:") {
        addr[bracket_pos + 2..].parse().ok()
    } else {
        addr.rsplit(':').next()?.parse().ok()
    }
}

fn parse_bind_address(addr: &str) -> String {
    if let Some(bracket_pos) = addr.rfind("]:") {
        addr[..bracket_pos + 1].to_string()
    } else if let Some(colon_pos) = addr.rfind(':') {
        addr[..colon_pos].to_string()
    } else {
        addr.to_string()
    }
}

fn parse_process_info(field: &str) -> (Option<u32>, String) {
    // Format: users:(("name",pid=123,fd=4))
    // or multiple: users:(("name",pid=123,fd=4),("name2",pid=456,fd=5))
    if !field.starts_with("users:") {
        return (None, "(unknown)".to_string());
    }

    let inner = &field[6..]; // strip "users:"
    // Find first process entry: ("name",pid=N,...)
    if let Some(start) = inner.find("(\"") {
        let rest = &inner[start + 2..];
        let name = rest.split('"').next().unwrap_or("unknown").to_string();

        let pid = rest
            .find("pid=")
            .and_then(|i| {
                let after = &rest[i + 4..];
                after
                    .split(|c: char| !c.is_ascii_digit())
                    .next()
                    .and_then(|s| s.parse::<u32>().ok())
            });

        (pid, name)
    } else {
        (None, "(unknown)".to_string())
    }
}

async fn read_cmdline(pid: u32) -> Result<String> {
    let path = format!("/proc/{}/cmdline", pid);
    let data = tokio::fs::read(&path).await?;
    let cmd = data
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).to_string())
        .collect::<Vec<_>>()
        .join(" ");
    Ok(cmd)
}
