mod app;
mod config;
mod event;
mod metrics;
mod ports;
mod providers;
mod sessions;
mod tui;
mod ui;

use anyhow::Result;
use app::{App, ServiceAction};
use event::Event;
use metrics::system::{MetricsCollector, SystemMetrics};
use providers::{LogEntry, ProviderType, Service};
use sessions::SessionInfo;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    use clap::Parser;

    #[derive(Parser, Debug)]
    #[command(name = "argus", version, about = "The all-seeing VPS dashboard")]
    struct Cli {
        /// Filter services by provider (all, systemd, docker, pm2)
        #[arg(long, default_value = "all")]
        filter: String,

        /// Path to config file (default: ~/.config/argus/config.toml)
        #[arg(long)]
        config: Option<String>,
    }

    let cli = Cli::parse();

    // Load config
    let cfg = config::Config::load(cli.config.as_deref());

    let mut terminal = tui::init()?;
    let mut events = event::EventLoop::new(100);
    let mut app = App::new();

    // Apply CLI filter override
    match cli.filter.to_lowercase().as_str() {
        "systemd" => app.service_filter = app::ServiceFilter::Systemd,
        "docker" => app.service_filter = app::ServiceFilter::Docker,
        "pm2" => app.service_filter = app::ServiceFilter::Pm2,
        _ => app.service_filter = app::ServiceFilter::All,
    }

    // Apply config default filter (CLI takes priority if not "all")
    if cli.filter == "all" {
        match cfg.general.default_filter.to_lowercase().as_str() {
            "systemd" => app.service_filter = app::ServiceFilter::Systemd,
            "docker" => app.service_filter = app::ServiceFilter::Docker,
            "pm2" => app.service_filter = app::ServiceFilter::Pm2,
            _ => {}
        }
    }

    let refresh_ms = cfg.general.refresh_interval_ms;

    // -----------------------------------------------------------------------
    // Log fetcher: on-demand per-service log fetching
    // -----------------------------------------------------------------------
    let (log_tx, mut log_rx) = mpsc::channel::<Vec<LogEntry>>(4);
    let log_tx_for_services = log_tx.clone();
    let (log_request_tx, mut log_request_rx) = mpsc::channel::<(String, ProviderType)>(4);

    tokio::spawn(async move {
        use providers::docker::DockerProvider;
        use providers::pm2::Pm2Provider;
        use providers::systemd::SystemdProvider;
        use providers::ServiceProvider;

        while let Some((id, provider_type)) = log_request_rx.recv().await {
            let entries = match provider_type {
                ProviderType::Systemd => SystemdProvider::new().logs(&id, 50).await,
                ProviderType::Docker => DockerProvider::new().logs(&id, 50).await,
                ProviderType::Pm2 => Pm2Provider::new().logs(&id, 50).await,
            };
            if let Ok(entries) = entries {
                let _ = log_tx_for_services.send(entries).await;
            }
        }
    });

    // -----------------------------------------------------------------------
    // Port log fetcher: on-demand per-PID log fetching via journalctl
    // -----------------------------------------------------------------------
    let log_tx_for_ports = log_tx.clone();
    let (port_log_request_tx, mut port_log_request_rx) = mpsc::channel::<u32>(4);

    tokio::spawn(async move {
        use providers::LogLevel;
        use tokio::process::Command;

        while let Some(pid) = port_log_request_rx.recv().await {
            // Try journalctl first
            let mut entries: Vec<LogEntry> = Vec::new();
            if let Ok(o) = Command::new("journalctl")
                .args(["--no-pager", "-n", "50", &format!("_PID={}", pid)])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output()
                .await
            {
                let stdout = String::from_utf8_lossy(&o.stdout);
                entries = stdout
                    .lines()
                    .filter(|l| !l.is_empty() && !l.starts_with("-- "))
                    .map(|line| {
                        let (timestamp, message) = if line.len() > 15 {
                            (line[..15].to_string(), line[16..].to_string())
                        } else {
                            (String::new(), line.to_string())
                        };
                        LogEntry {
                            timestamp,
                            level: LogLevel::Info,
                            message,
                        }
                    })
                    .collect();
            }

            // Fallback: read process stdout/stderr via /proc
            if entries.is_empty() {
                for fd in [1u8, 2] {
                    let fd_path = format!("/proc/{}/fd/{}", pid, fd);
                    let link = tokio::fs::read_link(&fd_path).await;
                    if let Ok(target) = link {
                        let target_str = target.to_string_lossy();
                        // Only read regular files (including deleted), skip pipes/sockets/ptys
                        if target_str.contains("/dev/") || target_str.starts_with("pipe:") || target_str.starts_with("socket:") {
                            continue;
                        }
                        if let Ok(data) = tokio::fs::read(&fd_path).await {
                            let text = String::from_utf8_lossy(&data);
                            let lines: Vec<&str> = text.lines().collect();
                            let start = lines.len().saturating_sub(50);
                            for line in &lines[start..] {
                                if !line.is_empty() {
                                    entries.push(LogEntry {
                                        timestamp: String::new(),
                                        level: LogLevel::Info,
                                        message: line.to_string(),
                                    });
                                }
                            }
                            if !entries.is_empty() {
                                break;
                            }
                        }
                    }
                }
            }

            let _ = log_tx_for_ports.send(entries).await;
        }
    });

    drop(log_tx);

    // -----------------------------------------------------------------------
    // Metrics poller: every 2 seconds
    // -----------------------------------------------------------------------
    let (metrics_tx, mut metrics_rx) = mpsc::channel::<SystemMetrics>(1);
    tokio::spawn(async move {
        let mut collector = MetricsCollector::new();
        loop {
            if let Ok(m) = collector.collect() {
                let _ = metrics_tx.send(m).await;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(refresh_ms)).await;
        }
    });

    // -----------------------------------------------------------------------
    // Services poller: configurable interval
    // -----------------------------------------------------------------------
    let services_refresh_ms = cfg.general.refresh_interval_ms * 2; // services at 2x interval
    let cfg_providers = cfg.providers.clone();
    let (services_tx, mut services_rx) = mpsc::channel::<Vec<Service>>(1);
    tokio::spawn(async move {
        use providers::docker::DockerProvider;
        use providers::pm2::Pm2Provider;
        use providers::systemd::SystemdProvider;
        use providers::ServiceProvider;
        use tokio::time::Duration;

        let systemd = SystemdProvider::new();
        let docker = DockerProvider::new();
        let pm2 = Pm2Provider::new();
        let docker_available = cfg_providers.docker && DockerProvider::is_available().await;
        let pm2_available = cfg_providers.pm2 && Pm2Provider::is_available().await;

        loop {
            let mut all_services = Vec::new();
            if cfg_providers.systemd {
                if let Ok(svcs) = systemd.list().await {
                    all_services.extend(svcs);
                }
            }
            if docker_available {
                if let Ok(svcs) = docker.list().await {
                    all_services.extend(svcs);
                }
            }
            if pm2_available {
                if let Ok(svcs) = pm2.list().await {
                    all_services.extend(svcs);
                }
            }
            let _ = services_tx.send(all_services).await;
            tokio::time::sleep(Duration::from_millis(services_refresh_ms)).await;
        }
    });

    // -----------------------------------------------------------------------
    // Sessions poller: configurable interval
    // -----------------------------------------------------------------------
    let sessions_refresh_ms = cfg.general.refresh_interval_ms * 2;
    let cfg_sessions = cfg.sessions.clone();
    let (sessions_tx, mut sessions_rx) = mpsc::channel::<Vec<SessionInfo>>(1);
    tokio::spawn(async move {
        use tokio::time::Duration;

        loop {
            let mut all_sessions: Vec<SessionInfo> = Vec::new();

            // Collect mosh sessions first — we'll use them to annotate tmux
            let mosh_sessions = sessions::mosh::list().await.unwrap_or_default();

            if cfg_sessions.ssh {
                if let Ok(ssh_sessions) = sessions::ssh::list().await {
                    all_sessions.extend(ssh_sessions.into_iter().map(SessionInfo::Ssh));
                }
            }

            // Add mosh sessions
            all_sessions.extend(mosh_sessions.iter().cloned().map(SessionInfo::Mosh));

            if cfg_sessions.tmux {
                if let Ok(mut tmux_sessions) = sessions::tmux::list().await {
                    for ts in &mut tmux_sessions {
                        ts.mosh_clients = mosh_sessions
                            .iter()
                            .filter(|ms| ms.tmux_target.as_deref() == Some(ts.name.as_str()))
                            .count() as u32;
                    }
                    all_sessions.extend(tmux_sessions.into_iter().map(SessionInfo::Tmux));
                }
            }
            if cfg_sessions.claude {
                if let Ok(claude_sessions) = sessions::claude::list().await {
                    all_sessions.extend(claude_sessions.into_iter().map(SessionInfo::Claude));
                }
            }

            let _ = sessions_tx.send(all_sessions).await;
            tokio::time::sleep(Duration::from_millis(sessions_refresh_ms)).await;
        }
    });

    // -----------------------------------------------------------------------
    // Ports poller: configurable interval
    // -----------------------------------------------------------------------
    let ports_refresh_ms = cfg.general.refresh_interval_ms * 2;
    let (ports_tx, mut ports_rx) = mpsc::channel::<Vec<ports::PortInfo>>(1);
    if cfg.ports.enabled {
        tokio::spawn(async move {
            use tokio::time::Duration;
            loop {
                let port_list = ports::scan().await;
                let _ = ports_tx.send(port_list).await;
                tokio::time::sleep(Duration::from_millis(ports_refresh_ms)).await;
            }
        });
    }

    // -----------------------------------------------------------------------
    // Action result channel: receives success/error from service actions
    // -----------------------------------------------------------------------
    let (action_result_tx, mut action_result_rx) = mpsc::channel::<ActionResult>(4);

    // -----------------------------------------------------------------------
    // Main event loop
    // -----------------------------------------------------------------------
    loop {
        terminal.draw(|frame| {
            ui::render(&app, frame);
        })?;

        tokio::select! {
            // Keyboard / tick events
            result = events.next() => {
                match result? {
                    Event::Key(key) => app.handle_key(key),
                    Event::Tick => {}
                }
            }

            // Action result (success/error from service control)
            Some(result) = action_result_rx.recv() => {
                app.notification = Some(app::Notification {
                    message: result.message,
                    is_error: result.is_error,
                    created: std::time::Instant::now(),
                });
            }

            // System metrics update
            Some(m) = metrics_rx.recv() => {
                app.system_metrics.cpu_percent = m.cpu_percent;
                app.system_metrics.mem_percent = m.mem_percent;
                app.system_metrics.disk_percent = m.disk_percent;
                app.system_metrics.net_bytes_sec = m.net_total_bytes_sec;
            }

            // Services update
            Some(svcs) = services_rx.recv() => {
                app.services = svcs;
                let len = app.filtered_services().len();
                if len == 0 {
                    app.selected_index = 0;
                } else {
                    app.selected_index = app.selected_index.min(len - 1);
                }
                // Refresh logs when service list changes (new service in view)
                app.log_needs_refresh = true;
            }

            // Sessions update
            Some(sess) = sessions_rx.recv() => {
                app.session_counts.ssh = sess.iter().filter(|s| matches!(s, SessionInfo::Ssh(_))).count();
                app.session_counts.mosh = sess.iter().filter(|s| matches!(s, SessionInfo::Mosh(_))).count();
                app.session_counts.tmux = sess.iter().filter(|s| matches!(s, SessionInfo::Tmux(_))).count();
                app.session_counts.claude = sess.iter().filter(|s| matches!(s, SessionInfo::Claude(_))).count();
                app.sessions = sess;
            }

            // Ports update
            Some(port_list) = ports_rx.recv() => {
                app.ports = port_list;
                let len = app.filtered_ports().len();
                if len == 0 {
                    app.port_selected = 0;
                } else {
                    app.port_selected = app.port_selected.min(len - 1);
                }
            }

            // Log entries received from fetcher
            Some(entries) = log_rx.recv() => {
                app.logs = entries;
                if app.log_auto_scroll {
                    // Scroll to the end
                    app.log_scroll = app.logs.len().saturating_sub(1);
                }
            }
        }

        // Dismiss expired notifications
        app.tick_notification();

        // -----------------------------------------------------------------------
        // Tmux preview capture (when in preview mode)
        // -----------------------------------------------------------------------
        if app.view_mode == app::ViewMode::TmuxPreview {
            if app.is_previewing_self() {
                app.preview_content = String::new();
            } else if let Some(ref session) = app.preview_session {
                let (win, pane) = app.preview_pane;
                if let Ok(content) = sessions::tmux::capture_pane(session, win, pane).await {
                    app.preview_content = content;
                }
            }
        }

        // -----------------------------------------------------------------------
        // Log refresh dispatch
        // -----------------------------------------------------------------------
        if app.log_needs_refresh {
            app.log_needs_refresh = false;
            if let Some((id, provider)) = app.selected_service_info() {
                let _ = log_request_tx.try_send((id, provider));
            }
        }

        // -----------------------------------------------------------------------
        // Port log refresh dispatch
        // -----------------------------------------------------------------------
        if app.port_log_needs_refresh && app.view_mode == app::ViewMode::Ports {
            app.port_log_needs_refresh = false;
            if let Some(port_info) = app.selected_port_info() {
                // Check if port's process matches a managed service
                let matched_service = port_info.pid.and_then(|pid| {
                    app.services
                        .iter()
                        .find(|s| s.pid == Some(pid))
                        .map(|s| (s.id.clone(), s.provider))
                });

                if let Some((id, provider)) = matched_service {
                    let _ = log_request_tx.try_send((id, provider));
                } else if let Some(pid) = port_info.pid {
                    let _ = port_log_request_tx.try_send(pid);
                }
            }
        }

        // -----------------------------------------------------------------------
        // Browser open dispatch
        // -----------------------------------------------------------------------
        if app.open_port_requested {
            app.open_port_requested = false;
            if let Some(port_info) = app.selected_port_info() {
                let port = port_info.port;
                let scheme = if port == 443 { "https" } else { "http" };
                let url = format!("{}://localhost:{}", scheme, port);
                let action_tx = action_result_tx.clone();
                tokio::spawn(async move {
                    let candidates: &[&str] = &[
                        "/usr/bin/wslview",
                        "/usr/bin/xdg-open",
                        "/mnt/c/Windows/explorer.exe",
                        "/usr/bin/sensible-browser",
                        "/usr/bin/x-www-browser",
                        "open",
                    ];
                    let cmd = candidates
                        .iter()
                        .find(|c| std::path::Path::new(c).exists())
                        .copied();
                    let (message, is_error) = if let Some(cmd) = cmd {
                        let result = tokio::process::Command::new(cmd)
                            .arg(&url)
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status()
                            .await;
                        let is_explorer = cmd.ends_with("explorer.exe");
                        match result {
                            // explorer.exe returns exit code 1 even on success
                            Ok(_) if is_explorer => (format!("Opened {}", url), false),
                            Ok(s) if s.success() => (format!("Opened {}", url), false),
                            Ok(_) => (format!("Failed to open {}", url), true),
                            Err(e) => (format!("Failed to open {}: {}", url, e), true),
                        }
                    } else {
                        (format!("No browser found to open {}", url), true)
                    };
                    let _ = action_tx
                        .send(ActionResult { message, is_error })
                        .await;
                });
            }
        }

        // -----------------------------------------------------------------------
        // Port kill dispatch
        // -----------------------------------------------------------------------
        if app.kill_port_requested {
            app.kill_port_requested = false;
            if let Some(pk) = app.pending_port_kill.take() {
                let action_tx = action_result_tx.clone();
                tokio::spawn(async move {
                    let result = tokio::process::Command::new("kill")
                        .arg(pk.pid.to_string())
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::piped())
                        .output()
                        .await;
                    let (message, is_error) = match result {
                        Ok(o) if o.status.success() => {
                            (format!("Killed {} (:{}, pid {})", pk.process_name, pk.port, pk.pid), false)
                        }
                        Ok(o) => {
                            let stderr = String::from_utf8_lossy(&o.stderr);
                            (format!("Failed to kill pid {}: {}", pk.pid, stderr.trim()), true)
                        }
                        Err(e) => (format!("Failed to kill pid {}: {}", pk.pid, e), true),
                    };
                    let _ = action_tx.send(ActionResult { message, is_error }).await;
                });
            }
        }

        // -----------------------------------------------------------------------
        // Service control dispatch (runs after select!, before running check)
        // -----------------------------------------------------------------------
        if app.confirm_pending {
            app.confirm_pending = false;
            if let Some(action) = app.pending_action.take() {
                let action_tx = action_result_tx.clone();
                tokio::spawn(async move {
                    let result = dispatch_service_action(&action).await;
                    let _ = action_tx.send(result).await;
                });
            }
        }

        if !app.running {
            break;
        }
    }

    tui::restore()?;
    Ok(())
}

struct ActionResult {
    message: String,
    is_error: bool,
}

async fn dispatch_service_action(action: &app::PendingAction) -> ActionResult {
    use providers::docker::DockerProvider;
    use providers::pm2::Pm2Provider;
    use providers::systemd::SystemdProvider;
    use providers::ServiceProvider;
    use tokio::process::Command;

    let id = &action.service_id;
    let verb = match action.action {
        ServiceAction::Start => "start",
        ServiceAction::Stop => "stop",
        ServiceAction::Restart => "restart",
        ServiceAction::Enable => "enable",
        ServiceAction::Disable => "disable",
    };

    let result = match action.action {
        ServiceAction::Enable | ServiceAction::Disable => {
            if action.provider_type == ProviderType::Systemd {
                let output = Command::new("systemctl")
                    .args(["--no-ask-password", verb, id])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .output()
                    .await;
                match output {
                    Ok(o) if o.status.success() => Ok(()),
                    Ok(o) => {
                        let stderr = String::from_utf8_lossy(&o.stderr).to_string();
                        Err(anyhow::anyhow!("{}", stderr.trim()))
                    }
                    Err(e) => Err(e.into()),
                }
            } else {
                Ok(())
            }
        }
        _ => match action.provider_type {
            ProviderType::Systemd => {
                let p = SystemdProvider::new();
                match action.action {
                    ServiceAction::Start => p.start(id).await,
                    ServiceAction::Stop => p.stop(id).await,
                    ServiceAction::Restart => p.restart(id).await,
                    _ => Ok(()),
                }
            }
            ProviderType::Docker => {
                let p = DockerProvider::new();
                match action.action {
                    ServiceAction::Start => p.start(id).await,
                    ServiceAction::Stop => p.stop(id).await,
                    ServiceAction::Restart => p.restart(id).await,
                    _ => Ok(()),
                }
            }
            ProviderType::Pm2 => {
                let p = Pm2Provider::new();
                match action.action {
                    ServiceAction::Start => p.start(id).await,
                    ServiceAction::Stop => p.stop(id).await,
                    ServiceAction::Restart => p.restart(id).await,
                    _ => Ok(()),
                }
            }
        },
    };

    match result {
        Ok(()) => ActionResult {
            message: format!("{} {} — ok", verb, action.service_name),
            is_error: false,
        },
        Err(e) => ActionResult {
            message: format!("{} {} — {}", verb, action.service_name, e),
            is_error: true,
        },
    }
}
