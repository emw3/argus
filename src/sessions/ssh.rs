use anyhow::Result;
use tokio::process::Command;

use super::{ConnectionType, SshSession};

pub async fn list() -> Result<Vec<SshSession>> {
    let output = Command::new("who").output().await?;
    let text = String::from_utf8_lossy(&output.stdout);

    let mut sessions: Vec<SshSession> = text.lines().filter_map(parse_who_line).collect();

    // Enrich with current process info
    for session in &mut sessions {
        session.current_process = get_leaf_process(&session.terminal).await;
    }

    Ok(sessions)
}

async fn get_leaf_process(terminal: &str) -> String {
    let output = Command::new("ps")
        .args(["-t", terminal, "-o", "comm=", "--sort=start_time"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await;

    if let Ok(o) = output {
        let text = String::from_utf8_lossy(&o.stdout);
        text.lines().last().unwrap_or("").trim().to_string()
    } else {
        String::new()
    }
}

fn parse_who_line(line: &str) -> Option<SshSession> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }

    let user = parts[0].to_string();
    let terminal = parts[1].to_string();

    if !terminal.starts_with("pts/") {
        return None;
    }

    let login_time = format!("{} {}", parts[2], parts[3]);

    let remote_raw = parts[4..]
        .iter()
        .find(|t| t.starts_with('(') && t.ends_with(')'))
        .map(|t| t.trim_start_matches('(').trim_end_matches(')').to_string())
        .unwrap_or_default();

    // Skip tmux-internal pseudo-terminals
    if remote_raw.starts_with("tmux(") {
        return None;
    }

    let (remote_ip, connection_type) = if remote_raw.starts_with("mosh [") {
        (remote_raw.clone(), ConnectionType::Mosh)
    } else if remote_raw.is_empty() {
        ("local".to_string(), ConnectionType::Local)
    } else {
        (remote_raw, ConnectionType::Ssh)
    };

    Some(SshSession {
        user,
        terminal,
        remote_ip,
        login_time,
        connection_type,
        current_process: String::new(),
    })
}
