use anyhow::Result;
use tokio::process::Command;

use super::MoshSession;

pub async fn list() -> Result<Vec<MoshSession>> {
    let output = Command::new("pgrep")
        .args(["-a", "mosh-server"])
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

fn parse_line(line: &str) -> Option<MoshSession> {
    let (pid_str, cmdline) = line.split_once(' ')?;
    let pid: u32 = pid_str.trim().parse().ok()?;

    // Extract tmux target from "attach-session -t 'name'" in the cmdline
    let tmux_target = extract_tmux_target(cmdline);

    Some(MoshSession { pid, tmux_target })
}

fn extract_tmux_target(cmdline: &str) -> Option<String> {
    // Look for: attach-session -t 'name' or attach-session -t "name"
    let marker = "attach-session -t ";
    let idx = cmdline.find(marker)?;
    let rest = &cmdline[idx + marker.len()..];
    let name = rest.trim_start_matches('\'').trim_start_matches('"');
    let end = name.find(|c: char| c == '\'' || c == '"' || c.is_whitespace())?;
    Some(name[..end].to_string())
}
