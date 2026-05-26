use anyhow::Result;
use tokio::process::Command;

use super::{ClaudeMode, ClaudeSession};

pub async fn list() -> Result<Vec<ClaudeSession>> {
    let output = Command::new("pgrep")
        .args(["-a", "-x", "claude"])
        .output()
        .await;

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Ok(Vec::new()),
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let sessions = text
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(parse_line)
        .collect();

    Ok(sessions)
}

fn parse_line(line: &str) -> Option<ClaudeSession> {
    let (pid_str, cmdline) = line.split_once(' ')?;
    let pid: u32 = pid_str.trim().parse().ok()?;

    // Only match actual claude CLI invocations
    // Skip daemon, bg-spare, bg-pty-host helper processes
    if cmdline.contains("--bg-spare")
        || cmdline.contains("--bg-pty-host")
        || cmdline.contains("daemon run")
    {
        return None;
    }

    let mode = if cmdline.contains(" agents") {
        ClaudeMode::Agent
    } else {
        ClaudeMode::Interactive
    };

    let mut project = extract_project(cmdline);

    if project == "claude" {
        if let Ok(cwd) = std::fs::read_link(format!("/proc/{}/cwd", pid)) {
            if let Some(name) = cwd.file_name() {
                project = name.to_string_lossy().to_string();
            }
        }
    }

    Some(ClaudeSession {
        project,
        mode,
        pid: Some(pid),
    })
}

fn extract_project(cmdline: &str) -> String {
    // Look for --cwd <path> or cwd in JSON
    let tokens: Vec<&str> = cmdline.split_whitespace().collect();
    for (i, token) in tokens.iter().enumerate() {
        if *token == "--cwd" {
            if let Some(path) = tokens.get(i + 1) {
                if *path != "." {
                    return path.split('/').next_back().unwrap_or(path).to_string();
                }
            }
        }
    }

    // Try to find cwd in JSON like {"cwd":"/path/to/project"}
    if let Some(start) = cmdline.find("\"cwd\":\"") {
        let rest = &cmdline[start + 7..];
        if let Some(end) = rest.find('"') {
            let path = &rest[..end];
            return path.split('/').next_back().unwrap_or(path).to_string();
        }
    }

    // Try to resolve via /proc/<pid>/cwd
    "claude".to_string()
}
