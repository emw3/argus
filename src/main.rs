mod app;
mod config;
mod event;
mod metrics;
mod providers;
mod sessions;
mod tui;
mod ui;
mod update;

use anyhow::Result;
use app::{App, ServiceAction};
use event::Event;
use metrics::system::{MetricsCollector, SystemMetrics};
use providers::{LogEntry, ProviderType, Service};
use sessions::SessionInfo;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    use clap::{Parser, Subcommand};

    #[derive(Parser, Debug)]
    #[command(name = "argus", version, about = "The all-seeing VPS dashboard")]
    struct Cli {
        /// Filter services by provider (all, systemd, docker, pm2)
        #[arg(long, default_value = "all")]
        filter: String,

        /// Path to config file (default: ~/.config/argus/config.toml)
        #[arg(long)]
        config: Option<String>,

        #[command(subcommand)]
        command: Option<Command>,
    }

    #[derive(Subcommand, Debug)]
    enum Command {
        /// Update argus to the latest release
        Update,
    }

    let cli = Cli::parse();

    if let Some(Command::Update) = cli.command {
        return update::run().await;
    }

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
                let _ = log_tx.send(entries).await;
            }
        }
    });

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
