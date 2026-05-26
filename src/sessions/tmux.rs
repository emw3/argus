use anyhow::Result;
use tokio::process::Command;

use super::{TmuxPane, TmuxSession};

pub async fn list() -> Result<Vec<TmuxSession>> {
    let socket = find_user_tmux_socket();
    let fmt = "#{session_name}:#{session_attached}:#{session_created}:#{session_windows}";

    let output = tmux_cmd(&socket, &["list-sessions", "-F", fmt]).await;

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Ok(Vec::new()),
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut sessions: Vec<TmuxSession> = text
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(parse_session_line)
        .collect();

    // Fetch panes for each session
    let pane_fmt = "#{session_name}:#{window_index}:#{pane_index}:#{pane_current_command}:#{pane_width}x#{pane_height}:#{pane_active}:#{pane_title}";
    let pane_output = tmux_cmd(&socket, &["list-panes", "-a", "-F", pane_fmt]).await;

    if let Ok(o) = pane_output {
        if o.status.success() {
            let pane_text = String::from_utf8_lossy(&o.stdout);
            for line in pane_text.lines() {
                if let Some((session_name, pane)) = parse_pane_line(line) {
                    if let Some(s) = sessions.iter_mut().find(|s| s.name == session_name) {
                        s.panes.push(pane);
                    }
                }
            }
        }
    }

    Ok(sessions)
}

pub async fn capture_pane(session: &str, window: u32, pane: u32) -> Result<String> {
    let socket = find_user_tmux_socket();
    let target = format!("{}:{}.{}", session, window, pane);
    let output = tmux_cmd(&socket, &["capture-pane", "-t", &target, "-p"]).await?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn tmux_cmd(socket: &Option<String>, args: &[&str]) -> std::io::Result<std::process::Output> {
    if let Some(ref s) = socket {
        let mut all_args = vec!["-S", s.as_str()];
        all_args.extend_from_slice(args);
        Command::new("tmux").args(&all_args).output().await
    } else {
        Command::new("tmux").args(args).output().await
    }
}

fn find_user_tmux_socket() -> Option<String> {
    if let Ok(sudo_uid) = std::env::var("SUDO_UID") {
        let socket = format!("/tmp/tmux-{}/default", sudo_uid);
        if std::path::Path::new(&socket).exists() {
            return Some(socket);
        }
    }

    for uid in 1000..=1010 {
        let socket = format!("/tmp/tmux-{}/default", uid);
        if std::path::Path::new(&socket).exists() {
            return Some(socket);
        }
    }

    None
}

fn parse_session_line(line: &str) -> Option<TmuxSession> {
    let parts: Vec<&str> = line.splitn(4, ':').collect();
    if parts.len() < 4 {
        return None;
    }

    let name = parts[0].to_string();
    let attached_count = parts[1].parse::<u32>().unwrap_or(0);
    let attached = attached_count > 0;
    let created_epoch = parts[2].parse::<u64>().unwrap_or(0);
    let created = format_epoch(created_epoch);
    let windows = parts[3].parse::<u32>().unwrap_or(0);

    Some(TmuxSession {
        name,
        attached,
        attached_count,
        windows,
        created,
        mosh_clients: 0,
        panes: Vec::new(),
    })
}

fn parse_pane_line(line: &str) -> Option<(String, TmuxPane)> {
    // Format: session:window:pane:command:WxH:active:title
    let parts: Vec<&str> = line.splitn(7, ':').collect();
    if parts.len() < 6 {
        return None;
    }

    let session_name = parts[0].to_string();
    let pane = TmuxPane {
        window_index: parts[1].parse().unwrap_or(0),
        pane_index: parts[2].parse().unwrap_or(0),
        command: parts[3].to_string(),
        size: parts[4].to_string(),
        active: parts[5] == "1",
    };

    Some((session_name, pane))
}

fn format_epoch(secs: u64) -> String {
    if secs == 0 {
        return "unknown".to_string();
    }
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if now >= secs {
        let diff = now - secs;
        if diff < 3600 {
            format!("{}m ago", diff / 60)
        } else if diff < 86400 {
            format!("{}h ago", diff / 3600)
        } else {
            format!("{}d ago", diff / 86400)
        }
    } else {
        "just now".to_string()
    }
}
