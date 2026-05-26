use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::providers::{LogEntry, ProviderType, Service, ServiceStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceAction {
    Start,
    Stop,
    Restart,
    Enable,
    Disable,
}

#[derive(Debug, Clone)]
pub struct PendingAction {
    pub action: ServiceAction,
    pub service_id: String,
    pub service_name: String,
    pub provider_type: ProviderType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewMode {
    Services,
    Sessions,
    Ports,
    TmuxPreview,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActivePane {
    ServiceList,
    Detail,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DetailTab {
    Info,
    Logs,
    Actions,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ServiceFilter {
    All,
    Systemd,
    Docker,
    Pm2,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Overlay {
    None,
    Help,
    Palette,
    Confirm,
}

#[derive(Debug, Default)]
pub struct SystemMetricsState {
    pub cpu_percent: f64,
    pub mem_percent: f64,
    pub disk_percent: f64,
    pub net_bytes_sec: u64,
}

#[derive(Debug, Default)]
pub struct SessionCounts {
    pub ssh: usize,
    pub mosh: usize,
    pub tmux: usize,
    pub claude: usize,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub message: String,
    pub is_error: bool,
    pub created: std::time::Instant,
}

pub struct App {
    pub running: bool,
    pub view_mode: ViewMode,
    pub active_pane: ActivePane,
    pub detail_tab: DetailTab,
    pub service_filter: ServiceFilter,
    pub selected_index: usize,
    pub overlay: Overlay,
    pub services: Vec<Service>,
    pub logs: Vec<LogEntry>,
    pub log_scroll: usize,
    pub log_auto_scroll: bool,
    pub log_needs_refresh: bool,
    pub system_metrics: SystemMetricsState,
    pub session_counts: SessionCounts,
    pub sessions: Vec<crate::sessions::SessionInfo>,
    pub pending_action: Option<PendingAction>,
    pub confirm_pending: bool,
    pub notification: Option<Notification>,
    pub preview_session: Option<String>,
    pub preview_pane: (u32, u32),
    pub preview_content: String,
    pub session_cursor: usize,
    pub search_active: bool,
    pub search_query: String,
    pub palette_query: String,
    pub palette_selected: usize,
    pub ports: Vec<crate::ports::PortInfo>,
    pub port_selected: usize,
    pub port_search_active: bool,
    pub port_search_query: String,
    pub port_log_needs_refresh: bool,
    pub open_port_requested: bool,
    pub kill_port_requested: bool,
    pub pending_port_kill: Option<PendingPortKill>,
}

#[derive(Debug, Clone)]
pub struct PendingPortKill {
    pub pid: u32,
    pub process_name: String,
    pub port: u16,
}

impl App {
    pub fn new() -> Self {
        Self {
            running: true,
            view_mode: ViewMode::Services,
            active_pane: ActivePane::ServiceList,
            detail_tab: DetailTab::Info,
            service_filter: ServiceFilter::All,
            selected_index: 0,
            overlay: Overlay::None,
            services: Vec::new(),
            logs: Vec::new(),
            log_scroll: 0,
            log_auto_scroll: true,
            log_needs_refresh: false,
            system_metrics: SystemMetricsState::default(),
            session_counts: SessionCounts::default(),
            sessions: Vec::new(),
            pending_action: None,
            confirm_pending: false,
            notification: None,
            preview_session: None,
            preview_pane: (0, 0),
            preview_content: String::new(),
            session_cursor: 0,
            search_active: false,
            search_query: String::new(),
            palette_query: String::new(),
            palette_selected: 0,
            ports: Vec::new(),
            port_selected: 0,
            port_search_active: false,
            port_search_query: String::new(),
            port_log_needs_refresh: false,
            open_port_requested: false,
            kill_port_requested: false,
            pending_port_kill: None,
        }
    }

    pub fn filtered_services(&self) -> Vec<&Service> {
        let query = self.search_query.to_lowercase();
        self.services
            .iter()
            .filter(|s| match self.service_filter {
                ServiceFilter::All => true,
                ServiceFilter::Systemd => s.provider == ProviderType::Systemd,
                ServiceFilter::Docker => s.provider == ProviderType::Docker,
                ServiceFilter::Pm2 => s.provider == ProviderType::Pm2,
            })
            .filter(|s| {
                if query.is_empty() {
                    true
                } else {
                    s.name.to_lowercase().contains(&query)
                }
            })
            .collect()
    }

    pub fn filtered_ports(&self) -> Vec<&crate::ports::PortInfo> {
        let query = self.port_search_query.to_lowercase();
        self.ports
            .iter()
            .filter(|p| {
                if query.is_empty() {
                    true
                } else {
                    p.process_name.to_lowercase().contains(&query)
                        || p.cmdline.to_lowercase().contains(&query)
                        || p.port.to_string().contains(&query)
                }
            })
            .collect()
    }

    pub fn selected_port_info(&self) -> Option<&crate::ports::PortInfo> {
        let ports = self.filtered_ports();
        if ports.is_empty() {
            return None;
        }
        let idx = self.port_selected.min(ports.len() - 1);
        Some(ports[idx])
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        // 1. Overlay handling (takes priority)
        if self.overlay != Overlay::None {
            match self.overlay {
                Overlay::Confirm => match key.code {
                    KeyCode::Esc => {
                        self.overlay = Overlay::None;
                        self.pending_action = None;
                        self.pending_port_kill = None;
                    }
                    KeyCode::Enter => {
                        if self.pending_port_kill.is_some() {
                            self.kill_port_requested = true;
                        } else {
                            self.confirm_pending = true;
                        }
                        self.overlay = Overlay::None;
                    }
                    _ => {}
                },
                Overlay::Palette => match key.code {
                    KeyCode::Esc => {
                        self.overlay = Overlay::None;
                        self.palette_query.clear();
                        self.palette_selected = 0;
                    }
                    KeyCode::Enter => {
                        self.execute_palette_command();
                        self.overlay = Overlay::None;
                        self.palette_query.clear();
                        self.palette_selected = 0;
                    }
                    KeyCode::Up => {
                        self.palette_selected = self.palette_selected.saturating_sub(1);
                    }
                    KeyCode::Down => {
                        let cmds = self.palette_commands();
                        if !cmds.is_empty() {
                            self.palette_selected = (self.palette_selected + 1).min(cmds.len() - 1);
                        }
                    }
                    KeyCode::Backspace => {
                        self.palette_query.pop();
                        self.palette_selected = 0;
                    }
                    KeyCode::Char(c) => {
                        self.palette_query.push(c);
                        self.palette_selected = 0;
                    }
                    _ => {}
                },
                Overlay::Help => {
                    if key.code == KeyCode::Esc || key.code == KeyCode::Char('?') {
                        self.overlay = Overlay::None;
                    }
                }
                Overlay::None => {}
            }
            return;
        }

        // 2a. Port search mode handling
        if self.port_search_active {
            match key.code {
                KeyCode::Esc => {
                    self.port_search_active = false;
                    self.port_search_query.clear();
                    self.port_selected = 0;
                }
                KeyCode::Enter => {
                    self.port_search_active = false;
                    self.port_log_needs_refresh = true;
                }
                KeyCode::Backspace => {
                    self.port_search_query.pop();
                    self.port_selected = 0;
                }
                KeyCode::Char(c) => {
                    self.port_search_query.push(c);
                    self.port_selected = 0;
                }
                _ => {}
            }
            let len = self.filtered_ports().len();
            if len == 0 {
                self.port_selected = 0;
            } else {
                self.port_selected = self.port_selected.min(len - 1);
            }
            return;
        }

        // 2b. Service search mode handling
        if self.search_active {
            match key.code {
                KeyCode::Esc => {
                    self.search_active = false;
                    self.search_query.clear();
                    self.selected_index = 0;
                }
                KeyCode::Enter => {
                    self.search_active = false;
                    self.log_needs_refresh = true;
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.selected_index = 0;
                }
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                    self.selected_index = 0;
                }
                _ => {}
            }
            let len = self.filtered_services().len();
            if len == 0 {
                self.selected_index = 0;
            } else {
                self.selected_index = self.selected_index.min(len - 1);
            }
            return;
        }

        // 3. TmuxPreview mode
        if self.view_mode == ViewMode::TmuxPreview {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.view_mode = ViewMode::Sessions;
                    self.preview_session = None;
                    self.preview_content.clear();
                }
                KeyCode::Char('n') | KeyCode::Tab => self.next_pane(),
                KeyCode::Char('p') | KeyCode::BackTab => self.prev_pane(),
                _ => {}
            }
            return;
        }

        // 4a. Ports view navigation
        if self.view_mode == ViewMode::Ports {
            match key.code {
                KeyCode::Char('q') => self.running = false,
                KeyCode::Char('?') => self.overlay = Overlay::Help,
                KeyCode::Tab => self.view_mode = ViewMode::Services,
                KeyCode::BackTab => self.toggle_pane(),
                KeyCode::Char('/') => {
                    self.port_search_active = true;
                    self.port_search_query.clear();
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.port_selected = self.port_selected.saturating_add(1);
                    let len = self.filtered_ports().len();
                    if len > 0 {
                        self.port_selected = self.port_selected.min(len - 1);
                    }
                    self.port_log_needs_refresh = true;
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.port_selected = self.port_selected.saturating_sub(1);
                    self.port_log_needs_refresh = true;
                }
                KeyCode::Char('g') => {
                    self.port_selected = 0;
                    self.port_log_needs_refresh = true;
                }
                KeyCode::Char('G') => {
                    let len = self.filtered_ports().len();
                    self.port_selected = len.saturating_sub(1);
                    self.port_log_needs_refresh = true;
                }
                KeyCode::Char('i') => self.detail_tab = DetailTab::Info,
                KeyCode::Char('l') => {
                    self.detail_tab = DetailTab::Logs;
                    self.port_log_needs_refresh = true;
                }
                KeyCode::Char(' ') if self.detail_tab == DetailTab::Logs => {
                    self.log_auto_scroll = !self.log_auto_scroll;
                }
                KeyCode::Char('o') => {
                    self.open_port_requested = true;
                }
                KeyCode::Char('x') => {
                    if let Some(port) = self.selected_port_info() {
                        if let Some(pid) = port.pid {
                            self.pending_port_kill = Some(PendingPortKill {
                                pid,
                                process_name: port.process_name.clone(),
                                port: port.port,
                            });
                            self.overlay = Overlay::Confirm;
                        }
                    }
                }
                KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.overlay = Overlay::Palette;
                    self.palette_query.clear();
                    self.palette_selected = 0;
                }
                _ => {}
            }
            return;
        }

        // 4b. Sessions view navigation
        if self.view_mode == ViewMode::Sessions {
            match key.code {
                KeyCode::Char('q') => self.running = false,
                KeyCode::Char('?') => self.overlay = Overlay::Help,
                KeyCode::Tab => self.view_mode = ViewMode::Ports,
                KeyCode::Char('j') | KeyCode::Down => {
                    self.session_cursor = self.session_cursor.saturating_add(1);
                    self.clamp_session_cursor();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.session_cursor = self.session_cursor.saturating_sub(1);
                }
                KeyCode::Char('g') => self.session_cursor = 0,
                KeyCode::Char('G') => {
                    let count = self.tmux_session_count();
                    self.session_cursor = count.saturating_sub(1);
                }
                KeyCode::Enter => self.enter_tmux_preview(),
                KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.overlay = Overlay::Palette;
                    self.palette_query.clear();
                    self.palette_selected = 0;
                }
                _ => {}
            }
            return;
        }

        // 5. Normal key handling (Services view)
        match key.code {
            KeyCode::Char('q') => self.running = false,
            KeyCode::Char('?') => self.overlay = Overlay::Help,
            KeyCode::Char('/') => {
                self.search_active = true;
                self.search_query.clear();
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_selection(1);
                self.log_needs_refresh = true;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_selection(-1);
                self.log_needs_refresh = true;
            }
            KeyCode::Char('g') => {
                self.selected_index = 0;
                self.log_needs_refresh = true;
            }
            KeyCode::Char('G') => {
                let len = self.filtered_services().len();
                self.selected_index = len.saturating_sub(1);
                self.log_needs_refresh = true;
            }
            KeyCode::Tab => {
                self.view_mode = ViewMode::Sessions;
            }
            KeyCode::BackTab => self.toggle_pane(),
            KeyCode::Char('1') => {
                self.service_filter = ServiceFilter::All;
                self.selected_index = 0;
                self.view_mode = ViewMode::Services;
            }
            KeyCode::Char('2') => {
                self.service_filter = ServiceFilter::Systemd;
                self.selected_index = 0;
                self.view_mode = ViewMode::Services;
            }
            KeyCode::Char('3') => {
                self.service_filter = ServiceFilter::Docker;
                self.selected_index = 0;
                self.view_mode = ViewMode::Services;
            }
            KeyCode::Char('4') => {
                self.service_filter = ServiceFilter::Pm2;
                self.selected_index = 0;
                self.view_mode = ViewMode::Services;
            }
            KeyCode::Char('i') => self.detail_tab = DetailTab::Info,
            KeyCode::Char('l') => {
                self.detail_tab = DetailTab::Logs;
                self.log_needs_refresh = true;
            }
            KeyCode::Char('f') => self.cycle_filter(),
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.overlay = Overlay::Palette;
                self.palette_query.clear();
                self.palette_selected = 0;
            }
            // Space toggles log auto-scroll when on the Logs tab
            KeyCode::Char(' ') if self.detail_tab == DetailTab::Logs => {
                self.log_auto_scroll = !self.log_auto_scroll;
            }
            // --- Service control keys ---
            KeyCode::Char('s') => {
                // Stop: only for running services, show confirm
                if let Some(svc) = self.selected_service() {
                    if svc.status == ServiceStatus::Running {
                        self.pending_action = Some(PendingAction {
                            action: ServiceAction::Stop,
                            service_id: svc.id.clone(),
                            service_name: svc.name.clone(),
                            provider_type: svc.provider,
                        });
                        self.overlay = Overlay::Confirm;
                    }
                }
            }
            KeyCode::Char('r') => {
                // Restart: any status, show confirm
                if let Some(svc) = self.selected_service() {
                    self.pending_action = Some(PendingAction {
                        action: ServiceAction::Restart,
                        service_id: svc.id.clone(),
                        service_name: svc.name.clone(),
                        provider_type: svc.provider,
                    });
                    self.overlay = Overlay::Confirm;
                }
            }
            KeyCode::Char('S') => {
                // Start: only for stopped/degraded services, no confirm
                if let Some(svc) = self.selected_service() {
                    if svc.status != ServiceStatus::Running {
                        self.pending_action = Some(PendingAction {
                            action: ServiceAction::Start,
                            service_id: svc.id.clone(),
                            service_name: svc.name.clone(),
                            provider_type: svc.provider,
                        });
                        self.confirm_pending = true;
                    }
                }
            }
            KeyCode::Char('e') => {
                // Enable: systemd only, no confirm
                if let Some(svc) = self.selected_service() {
                    if svc.provider == ProviderType::Systemd {
                        self.pending_action = Some(PendingAction {
                            action: ServiceAction::Enable,
                            service_id: svc.id.clone(),
                            service_name: svc.name.clone(),
                            provider_type: svc.provider,
                        });
                        self.confirm_pending = true;
                    }
                }
            }
            KeyCode::Char('d') => {
                // Disable: systemd only, show confirm
                if let Some(svc) = self.selected_service() {
                    if svc.provider == ProviderType::Systemd {
                        self.pending_action = Some(PendingAction {
                            action: ServiceAction::Disable,
                            service_id: svc.id.clone(),
                            service_name: svc.name.clone(),
                            provider_type: svc.provider,
                        });
                        self.overlay = Overlay::Confirm;
                    }
                }
            }
            _ => {}
        }

        // Clamp selected_index to valid range
        let len = self.filtered_services().len();
        if len == 0 {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(len - 1);
        }
    }

    /// Returns a clone of the currently selected service, if any.
    fn selected_service(&self) -> Option<Service> {
        let services = self.filtered_services();
        if services.is_empty() {
            return None;
        }
        let idx = self.selected_index.min(services.len() - 1);
        Some(services[idx].clone())
    }

    fn move_selection(&mut self, delta: i32) {
        if delta > 0 {
            self.selected_index = self.selected_index.saturating_add(delta as usize);
        } else {
            self.selected_index = self.selected_index.saturating_sub((-delta) as usize);
        }
    }

    fn toggle_pane(&mut self) {
        self.active_pane = match self.active_pane {
            ActivePane::ServiceList => ActivePane::Detail,
            ActivePane::Detail => ActivePane::ServiceList,
        };
    }

    fn cycle_filter(&mut self) {
        self.service_filter = match self.service_filter {
            ServiceFilter::All => ServiceFilter::Systemd,
            ServiceFilter::Systemd => ServiceFilter::Docker,
            ServiceFilter::Docker => ServiceFilter::Pm2,
            ServiceFilter::Pm2 => ServiceFilter::All,
        };
        self.selected_index = 0;
    }

    /// Returns the (id, provider) of the currently selected service, if any.
    pub fn selected_service_info(&self) -> Option<(String, ProviderType)> {
        let services = self.filtered_services();
        if services.is_empty() {
            return None;
        }
        let idx = self.selected_index.min(services.len() - 1);
        Some((services[idx].id.clone(), services[idx].provider))
    }

    /// Returns filtered command palette entries based on palette_query.
    pub fn palette_commands(&self) -> Vec<(&'static str, &'static str)> {
        let all: Vec<(&'static str, &'static str)> = vec![
            ("start", "Start selected service"),
            ("stop", "Stop selected service"),
            ("restart", "Restart selected service"),
            ("logs", "View logs for selected service"),
            ("sessions", "Switch to sessions view"),
            ("ports", "Switch to ports view"),
            ("open", "Open selected port in browser"),
            ("kill", "Kill selected port process"),
            ("services", "Switch to services view"),
            ("filter:all", "Show all services [1]"),
            ("filter:systemd", "Show systemd services [2]"),
            ("filter:docker", "Show docker services [3]"),
            ("filter:pm2", "Show PM2 services [4]"),
            ("quit", "Quit dashboard"),
        ];
        if self.palette_query.is_empty() {
            all
        } else {
            let q = self.palette_query.to_lowercase();
            all.into_iter()
                .filter(|(cmd, desc)| {
                    cmd.contains(&q as &str) || desc.to_lowercase().contains(&q as &str)
                })
                .collect()
        }
    }

    /// Executes the selected palette command.
    pub fn execute_palette_command(&mut self) {
        let cmds = self.palette_commands();
        if cmds.is_empty() {
            return;
        }
        let idx = self.palette_selected.min(cmds.len() - 1);
        let (cmd, _) = cmds[idx];
        match cmd {
            "start" => {
                if let Some(svc) = self.selected_service() {
                    if svc.status != ServiceStatus::Running {
                        self.pending_action = Some(PendingAction {
                            action: ServiceAction::Start,
                            service_id: svc.id.clone(),
                            service_name: svc.name.clone(),
                            provider_type: svc.provider,
                        });
                        self.confirm_pending = true;
                    }
                }
            }
            "stop" => {
                if let Some(svc) = self.selected_service() {
                    if svc.status == ServiceStatus::Running {
                        self.pending_action = Some(PendingAction {
                            action: ServiceAction::Stop,
                            service_id: svc.id.clone(),
                            service_name: svc.name.clone(),
                            provider_type: svc.provider,
                        });
                        self.overlay = Overlay::Confirm;
                    }
                }
            }
            "restart" => {
                if let Some(svc) = self.selected_service() {
                    self.pending_action = Some(PendingAction {
                        action: ServiceAction::Restart,
                        service_id: svc.id.clone(),
                        service_name: svc.name.clone(),
                        provider_type: svc.provider,
                    });
                    self.overlay = Overlay::Confirm;
                }
            }
            "logs" => {
                self.detail_tab = DetailTab::Logs;
                self.log_needs_refresh = true;
            }
            "sessions" => {
                self.view_mode = ViewMode::Sessions;
            }
            "ports" => {
                self.view_mode = ViewMode::Ports;
            }
            "open" if self.view_mode == ViewMode::Ports => {
                self.open_port_requested = true;
            }
            "kill" if self.view_mode == ViewMode::Ports => {
                if let Some(port) = self.selected_port_info() {
                    if let Some(pid) = port.pid {
                        self.pending_port_kill = Some(PendingPortKill {
                            pid,
                            process_name: port.process_name.clone(),
                            port: port.port,
                        });
                        self.overlay = Overlay::Confirm;
                    }
                }
            }
            "services" => {
                self.view_mode = ViewMode::Services;
            }
            "filter:all" => {
                self.service_filter = ServiceFilter::All;
                self.selected_index = 0;
            }
            "filter:systemd" => {
                self.service_filter = ServiceFilter::Systemd;
                self.selected_index = 0;
            }
            "filter:docker" => {
                self.service_filter = ServiceFilter::Docker;
                self.selected_index = 0;
            }
            "filter:pm2" => {
                self.service_filter = ServiceFilter::Pm2;
                self.selected_index = 0;
            }
            "quit" => {
                self.running = false;
            }
            _ => {}
        }
    }

    fn tmux_session_count(&self) -> usize {
        self.sessions
            .iter()
            .filter(|s| matches!(s, crate::sessions::SessionInfo::Tmux(_)))
            .count()
    }

    fn clamp_session_cursor(&mut self) {
        let count = self.tmux_session_count();
        if count > 0 {
            self.session_cursor = self.session_cursor.min(count - 1);
        } else {
            self.session_cursor = 0;
        }
    }

    fn enter_tmux_preview(&mut self) {
        let tmux_sessions: Vec<_> = self
            .sessions
            .iter()
            .filter_map(|s| {
                if let crate::sessions::SessionInfo::Tmux(t) = s {
                    Some(t)
                } else {
                    None
                }
            })
            .collect();

        if let Some(ts) = tmux_sessions.get(self.session_cursor) {
            self.preview_session = Some(ts.name.clone());
            self.preview_pane = if let Some(pane) = ts.panes.first() {
                (pane.window_index, pane.pane_index)
            } else {
                (0, 0)
            };
            self.preview_content.clear();
            self.view_mode = ViewMode::TmuxPreview;
        }
    }

    fn next_pane(&mut self) {
        if let Some(ref session_name) = self.preview_session {
            let panes = self.get_preview_panes(session_name);
            let current = panes
                .iter()
                .position(|p| p.0 == self.preview_pane.0 && p.1 == self.preview_pane.1);
            if let Some(idx) = current {
                if idx + 1 < panes.len() {
                    self.preview_pane = panes[idx + 1];
                    self.preview_content.clear();
                }
            }
        }
    }

    fn prev_pane(&mut self) {
        if let Some(ref session_name) = self.preview_session {
            let panes = self.get_preview_panes(session_name);
            let current = panes
                .iter()
                .position(|p| p.0 == self.preview_pane.0 && p.1 == self.preview_pane.1);
            if let Some(idx) = current {
                if idx > 0 {
                    self.preview_pane = panes[idx - 1];
                    self.preview_content.clear();
                }
            }
        }
    }

    fn get_preview_panes(&self, session_name: &str) -> Vec<(u32, u32)> {
        self.sessions
            .iter()
            .filter_map(|s| {
                if let crate::sessions::SessionInfo::Tmux(t) = s {
                    if t.name == session_name {
                        Some(t)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .flat_map(|t| t.panes.iter().map(|p| (p.window_index, p.pane_index)))
            .collect()
    }

    pub fn is_previewing_self(&self) -> bool {
        if let Some(ref session_name) = self.preview_session {
            let (win, pane) = self.preview_pane;
            // Check if the pane's command is argus
            self.sessions
                .iter()
                .filter_map(|s| {
                    if let crate::sessions::SessionInfo::Tmux(t) = s {
                        Some(t)
                    } else {
                        None
                    }
                })
                .find(|t| t.name == *session_name)
                .and_then(|t| {
                    t.panes
                        .iter()
                        .find(|p| p.window_index == win && p.pane_index == pane)
                })
                .map(|p| p.command.contains("argus"))
                .unwrap_or(false)
        } else {
            false
        }
    }

    pub fn tick_notification(&mut self) {
        if let Some(ref n) = self.notification {
            if n.created.elapsed().as_secs() >= 5 {
                self.notification = None;
            }
        }
    }

    pub fn running_count(&self) -> usize {
        self.services
            .iter()
            .filter(|s| s.status == ServiceStatus::Running)
            .count()
    }

    pub fn stopped_count(&self) -> usize {
        self.services
            .iter()
            .filter(|s| s.status == ServiceStatus::Stopped || s.status == ServiceStatus::Degraded)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn make_service(name: &str, provider: ProviderType, status: ServiceStatus) -> Service {
        Service {
            id: name.to_string(),
            name: name.to_string(),
            status,
            provider,
            pid: Some(1000),
            uptime_secs: Some(3600),
            memory_bytes: Some(1024 * 1024),
            cpu_percent: Some(1.5),
        }
    }

    fn make_port(port: u16, pid: Option<u32>, name: &str, cmdline: &str) -> crate::ports::PortInfo {
        crate::ports::PortInfo {
            port,
            bind_address: "0.0.0.0".to_string(),
            pid,
            process_name: name.to_string(),
            cmdline: cmdline.to_string(),
        }
    }

    // -----------------------------------------------------------------------
    // App::new defaults
    // -----------------------------------------------------------------------

    #[test]
    fn new_app_defaults() {
        let app = App::new();
        assert!(app.running);
        assert_eq!(app.view_mode, ViewMode::Services);
        assert_eq!(app.active_pane, ActivePane::ServiceList);
        assert_eq!(app.detail_tab, DetailTab::Info);
        assert_eq!(app.service_filter, ServiceFilter::All);
        assert_eq!(app.selected_index, 0);
        assert_eq!(app.overlay, Overlay::None);
        assert!(app.services.is_empty());
        assert!(app.ports.is_empty());
        assert!(!app.search_active);
        assert!(!app.port_search_active);
        assert!(!app.open_port_requested);
        assert!(!app.kill_port_requested);
        assert!(app.pending_port_kill.is_none());
    }

    // -----------------------------------------------------------------------
    // Tab cycling: Services -> Sessions -> Ports -> Services
    // -----------------------------------------------------------------------

    #[test]
    fn tab_cycles_views() {
        let mut app = App::new();
        assert_eq!(app.view_mode, ViewMode::Services);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.view_mode, ViewMode::Sessions);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.view_mode, ViewMode::Ports);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.view_mode, ViewMode::Services);
    }

    // -----------------------------------------------------------------------
    // BackTab (Shift-Tab) pane toggle
    // -----------------------------------------------------------------------

    #[test]
    fn backtab_toggles_pane_in_services() {
        let mut app = App::new();
        assert_eq!(app.active_pane, ActivePane::ServiceList);

        app.handle_key(key(KeyCode::BackTab));
        assert_eq!(app.active_pane, ActivePane::Detail);

        app.handle_key(key(KeyCode::BackTab));
        assert_eq!(app.active_pane, ActivePane::ServiceList);
    }

    #[test]
    fn backtab_toggles_pane_in_ports() {
        let mut app = App::new();
        app.view_mode = ViewMode::Ports;
        app.active_pane = ActivePane::ServiceList;

        app.handle_key(key(KeyCode::BackTab));
        assert_eq!(app.active_pane, ActivePane::Detail);
    }

    // -----------------------------------------------------------------------
    // Search mode: enter, type, exit
    // -----------------------------------------------------------------------

    #[test]
    fn search_mode_enter_and_exit() {
        let mut app = App::new();
        app.services = vec![
            make_service("nginx", ProviderType::Systemd, ServiceStatus::Running),
            make_service("docker-app", ProviderType::Docker, ServiceStatus::Running),
        ];

        // Enter search
        app.handle_key(key(KeyCode::Char('/')));
        assert!(app.search_active);
        assert!(app.search_query.is_empty());

        // Type query
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key(KeyCode::Char('g')));
        assert_eq!(app.search_query, "ng");

        // Should filter to nginx only
        assert_eq!(app.filtered_services().len(), 1);
        assert_eq!(app.filtered_services()[0].name, "nginx");

        // Esc clears and exits
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.search_active);
        assert!(app.search_query.is_empty());
        assert_eq!(app.filtered_services().len(), 2);
    }

    #[test]
    fn search_mode_enter_confirms() {
        let mut app = App::new();
        app.services = vec![make_service(
            "nginx",
            ProviderType::Systemd,
            ServiceStatus::Running,
        )];

        app.handle_key(key(KeyCode::Char('/')));
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key(KeyCode::Enter));

        assert!(!app.search_active);
        assert_eq!(app.search_query, "n"); // query preserved after Enter
        assert!(app.log_needs_refresh);
    }

    #[test]
    fn search_backspace() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Char('/')));
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Char('b')));
        assert_eq!(app.search_query, "ab");
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.search_query, "a");
    }

    // -----------------------------------------------------------------------
    // Port search mode
    // -----------------------------------------------------------------------

    #[test]
    fn port_search_enter_exit() {
        let mut app = App::new();
        app.view_mode = ViewMode::Ports;
        app.ports = vec![
            make_port(8080, Some(100), "node", "node server.js"),
            make_port(3000, Some(200), "next-server", "next start"),
        ];

        // Enter search
        app.handle_key(key(KeyCode::Char('/')));
        assert!(app.port_search_active);

        // Type "node"
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key(KeyCode::Char('o')));
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Char('e')));
        assert_eq!(app.port_search_query, "node");

        // Esc exits and clears
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.port_search_active);
        assert!(app.port_search_query.is_empty());
    }

    // -----------------------------------------------------------------------
    // filtered_ports: search matching
    // -----------------------------------------------------------------------

    #[test]
    fn filtered_ports_by_process_name() {
        let mut app = App::new();
        app.ports = vec![
            make_port(8080, Some(100), "node", "node server.js"),
            make_port(5432, Some(200), "postgres", "/usr/bin/postgres"),
        ];

        app.port_search_query = "node".to_string();
        let filtered = app.filtered_ports();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].port, 8080);
    }

    #[test]
    fn filtered_ports_by_cmdline() {
        let mut app = App::new();
        app.ports = vec![
            make_port(8080, Some(100), "node", "node server.js"),
            make_port(5432, Some(200), "postgres", "/usr/bin/postgres"),
        ];

        app.port_search_query = "server.js".to_string();
        let filtered = app.filtered_ports();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].port, 8080);
    }

    #[test]
    fn filtered_ports_by_port_number() {
        let mut app = App::new();
        app.ports = vec![
            make_port(8080, Some(100), "node", "node server.js"),
            make_port(5432, Some(200), "postgres", "/usr/bin/postgres"),
        ];

        app.port_search_query = "5432".to_string();
        let filtered = app.filtered_ports();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].port, 5432);
    }

    #[test]
    fn filtered_ports_case_insensitive() {
        let mut app = App::new();
        app.ports = vec![make_port(8080, Some(100), "Node", "Node server.js")];
        app.port_search_query = "node".to_string();
        assert_eq!(app.filtered_ports().len(), 1);
    }

    #[test]
    fn filtered_ports_empty_query_returns_all() {
        let mut app = App::new();
        app.ports = vec![
            make_port(8080, Some(100), "node", ""),
            make_port(5432, Some(200), "postgres", ""),
        ];
        app.port_search_query.clear();
        assert_eq!(app.filtered_ports().len(), 2);
    }

    // -----------------------------------------------------------------------
    // selected_port_info bounds clamping
    // -----------------------------------------------------------------------

    #[test]
    fn selected_port_info_clamps_index() {
        let mut app = App::new();
        app.ports = vec![
            make_port(80, Some(1), "nginx", ""),
            make_port(443, Some(2), "nginx", ""),
        ];
        app.port_selected = 999; // way out of bounds
        let info = app.selected_port_info().unwrap();
        assert_eq!(info.port, 443); // clamped to last
    }

    #[test]
    fn selected_port_info_empty_returns_none() {
        let app = App::new();
        assert!(app.selected_port_info().is_none());
    }

    #[test]
    fn selected_port_info_with_filter() {
        let mut app = App::new();
        app.ports = vec![
            make_port(80, Some(1), "nginx", ""),
            make_port(3000, Some(2), "node", ""),
        ];
        app.port_search_query = "node".to_string();
        app.port_selected = 0;
        let info = app.selected_port_info().unwrap();
        assert_eq!(info.port, 3000);
    }

    // -----------------------------------------------------------------------
    // Kill confirm flow: x -> overlay -> Enter/Esc
    // -----------------------------------------------------------------------

    #[test]
    fn kill_port_x_sets_confirm_overlay() {
        let mut app = App::new();
        app.view_mode = ViewMode::Ports;
        app.ports = vec![make_port(8080, Some(1234), "node", "")];
        app.port_selected = 0;

        app.handle_key(key(KeyCode::Char('x')));

        assert_eq!(app.overlay, Overlay::Confirm);
        assert!(app.pending_port_kill.is_some());
        let pk = app.pending_port_kill.as_ref().unwrap();
        assert_eq!(pk.pid, 1234);
        assert_eq!(pk.port, 8080);
        assert_eq!(pk.process_name, "node");
    }

    #[test]
    fn kill_port_confirm_enter_triggers_kill() {
        let mut app = App::new();
        app.view_mode = ViewMode::Ports;
        app.ports = vec![make_port(8080, Some(1234), "node", "")];
        app.port_selected = 0;

        // Press x
        app.handle_key(key(KeyCode::Char('x')));
        assert_eq!(app.overlay, Overlay::Confirm);

        // Press Enter to confirm
        app.handle_key(key(KeyCode::Enter));
        assert!(app.kill_port_requested);
        assert_eq!(app.overlay, Overlay::None);
    }

    #[test]
    fn kill_port_confirm_esc_cancels() {
        let mut app = App::new();
        app.view_mode = ViewMode::Ports;
        app.ports = vec![make_port(8080, Some(1234), "node", "")];
        app.port_selected = 0;

        app.handle_key(key(KeyCode::Char('x')));
        assert!(app.pending_port_kill.is_some());

        app.handle_key(key(KeyCode::Esc));
        assert!(app.pending_port_kill.is_none());
        assert_eq!(app.overlay, Overlay::None);
        assert!(!app.kill_port_requested);
    }

    #[test]
    fn kill_port_x_with_no_pid_does_nothing() {
        let mut app = App::new();
        app.view_mode = ViewMode::Ports;
        app.ports = vec![make_port(5432, None, "(unknown)", "")];
        app.port_selected = 0;

        app.handle_key(key(KeyCode::Char('x')));
        assert_eq!(app.overlay, Overlay::None);
        assert!(app.pending_port_kill.is_none());
    }

    #[test]
    fn kill_port_x_with_empty_ports_does_nothing() {
        let mut app = App::new();
        app.view_mode = ViewMode::Ports;
        // no ports

        app.handle_key(key(KeyCode::Char('x')));
        assert_eq!(app.overlay, Overlay::None);
        assert!(app.pending_port_kill.is_none());
    }

    // -----------------------------------------------------------------------
    // Open port
    // -----------------------------------------------------------------------

    #[test]
    fn open_port_o_sets_flag() {
        let mut app = App::new();
        app.view_mode = ViewMode::Ports;
        app.ports = vec![make_port(8080, Some(1), "node", "")];

        app.handle_key(key(KeyCode::Char('o')));
        assert!(app.open_port_requested);
    }

    // -----------------------------------------------------------------------
    // g/G navigation in Ports view
    // -----------------------------------------------------------------------

    #[test]
    fn ports_g_goes_to_top() {
        let mut app = App::new();
        app.view_mode = ViewMode::Ports;
        app.ports = vec![
            make_port(80, Some(1), "a", ""),
            make_port(443, Some(2), "b", ""),
            make_port(8080, Some(3), "c", ""),
        ];
        app.port_selected = 2;

        app.handle_key(key(KeyCode::Char('g')));
        assert_eq!(app.port_selected, 0);
    }

    #[test]
    fn ports_big_g_goes_to_bottom() {
        let mut app = App::new();
        app.view_mode = ViewMode::Ports;
        app.ports = vec![
            make_port(80, Some(1), "a", ""),
            make_port(443, Some(2), "b", ""),
            make_port(8080, Some(3), "c", ""),
        ];
        app.port_selected = 0;

        app.handle_key(key(KeyCode::Char('G')));
        assert_eq!(app.port_selected, 2);
    }

    #[test]
    fn ports_j_k_navigation() {
        let mut app = App::new();
        app.view_mode = ViewMode::Ports;
        app.ports = vec![
            make_port(80, Some(1), "a", ""),
            make_port(443, Some(2), "b", ""),
            make_port(8080, Some(3), "c", ""),
        ];
        app.port_selected = 0;

        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.port_selected, 1);

        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.port_selected, 2);

        // Doesn't go past end
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.port_selected, 2);

        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.port_selected, 1);

        // Doesn't go below 0
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.port_selected, 0);
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.port_selected, 0);
    }

    // -----------------------------------------------------------------------
    // Services view: g/G, j/k, filter
    // -----------------------------------------------------------------------

    #[test]
    fn services_g_big_g_navigation() {
        let mut app = App::new();
        app.services = vec![
            make_service("a", ProviderType::Systemd, ServiceStatus::Running),
            make_service("b", ProviderType::Systemd, ServiceStatus::Running),
            make_service("c", ProviderType::Systemd, ServiceStatus::Running),
        ];
        app.selected_index = 1;

        app.handle_key(key(KeyCode::Char('g')));
        assert_eq!(app.selected_index, 0);

        app.handle_key(key(KeyCode::Char('G')));
        assert_eq!(app.selected_index, 2);
    }

    #[test]
    fn filter_cycle() {
        let mut app = App::new();
        assert_eq!(app.service_filter, ServiceFilter::All);

        app.handle_key(key(KeyCode::Char('f')));
        assert_eq!(app.service_filter, ServiceFilter::Systemd);

        app.handle_key(key(KeyCode::Char('f')));
        assert_eq!(app.service_filter, ServiceFilter::Docker);

        app.handle_key(key(KeyCode::Char('f')));
        assert_eq!(app.service_filter, ServiceFilter::Pm2);

        app.handle_key(key(KeyCode::Char('f')));
        assert_eq!(app.service_filter, ServiceFilter::All);
    }

    #[test]
    fn number_keys_set_filter() {
        let mut app = App::new();

        app.handle_key(key(KeyCode::Char('2')));
        assert_eq!(app.service_filter, ServiceFilter::Systemd);

        app.handle_key(key(KeyCode::Char('3')));
        assert_eq!(app.service_filter, ServiceFilter::Docker);

        app.handle_key(key(KeyCode::Char('4')));
        assert_eq!(app.service_filter, ServiceFilter::Pm2);

        app.handle_key(key(KeyCode::Char('1')));
        assert_eq!(app.service_filter, ServiceFilter::All);
    }

    #[test]
    fn filtered_services_by_provider() {
        let mut app = App::new();
        app.services = vec![
            make_service("nginx", ProviderType::Systemd, ServiceStatus::Running),
            make_service("myapp", ProviderType::Docker, ServiceStatus::Running),
            make_service("api", ProviderType::Pm2, ServiceStatus::Stopped),
        ];

        app.service_filter = ServiceFilter::Docker;
        let filtered = app.filtered_services();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "myapp");
    }

    #[test]
    fn filtered_services_by_search() {
        let mut app = App::new();
        app.services = vec![
            make_service("nginx", ProviderType::Systemd, ServiceStatus::Running),
            make_service("docker-nginx", ProviderType::Docker, ServiceStatus::Running),
            make_service("postgres", ProviderType::Systemd, ServiceStatus::Running),
        ];
        app.search_query = "nginx".to_string();
        let filtered = app.filtered_services();
        assert_eq!(filtered.len(), 2);
    }

    // -----------------------------------------------------------------------
    // Command palette
    // -----------------------------------------------------------------------

    #[test]
    fn palette_open_close() {
        let mut app = App::new();

        app.handle_key(key_ctrl('p'));
        assert_eq!(app.overlay, Overlay::Palette);

        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.overlay, Overlay::None);
    }

    #[test]
    fn palette_filtering() {
        let app = App::new();
        // With empty query, returns all
        let all = app.palette_commands();
        assert!(!all.is_empty());

        // Filtering by "quit"
        let mut app2 = App::new();
        app2.palette_query = "quit".to_string();
        let filtered = app2.palette_commands();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0, "quit");
    }

    #[test]
    fn palette_filter_by_description() {
        let mut app = App::new();
        app.palette_query = "browser".to_string();
        let filtered = app.palette_commands();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0, "open");
    }

    #[test]
    fn palette_navigation() {
        let mut app = App::new();
        app.overlay = Overlay::Palette;

        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.palette_selected, 1);

        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.palette_selected, 2);

        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.palette_selected, 1);

        // Up at 0 stays at 0
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.palette_selected, 0);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.palette_selected, 0);
    }

    #[test]
    fn palette_typing_resets_selection() {
        let mut app = App::new();
        app.overlay = Overlay::Palette;
        app.palette_selected = 5;

        app.handle_key(key(KeyCode::Char('q')));
        assert_eq!(app.palette_selected, 0);
        assert_eq!(app.palette_query, "q");
    }

    #[test]
    fn palette_backspace() {
        let mut app = App::new();
        app.overlay = Overlay::Palette;
        app.palette_query = "qui".to_string();

        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.palette_query, "qu");
        assert_eq!(app.palette_selected, 0);
    }

    #[test]
    fn palette_execute_quit() {
        let mut app = App::new();
        app.palette_query = "quit".to_string();
        app.palette_selected = 0;
        app.execute_palette_command();
        assert!(!app.running);
    }

    #[test]
    fn palette_execute_sessions() {
        let mut app = App::new();
        app.palette_query = "sessions".to_string();
        app.palette_selected = 0;
        app.execute_palette_command();
        assert_eq!(app.view_mode, ViewMode::Sessions);
    }

    #[test]
    fn palette_execute_ports() {
        let mut app = App::new();
        app.palette_query = "ports".to_string();
        app.palette_selected = 0;
        app.execute_palette_command();
        assert_eq!(app.view_mode, ViewMode::Ports);
    }

    #[test]
    fn palette_execute_filter_commands() {
        let mut app = App::new();

        app.palette_query.clear();
        // Find the filter:docker command
        let cmds = app.palette_commands();
        let idx = cmds
            .iter()
            .position(|(c, _)| *c == "filter:docker")
            .unwrap();
        app.palette_selected = idx;
        app.execute_palette_command();
        assert_eq!(app.service_filter, ServiceFilter::Docker);
    }

    #[test]
    fn palette_execute_open_in_ports_view() {
        let mut app = App::new();
        app.view_mode = ViewMode::Ports;
        app.ports = vec![make_port(8080, Some(1), "node", "")];
        app.palette_query = "open".to_string();
        app.palette_selected = 0;
        app.execute_palette_command();
        assert!(app.open_port_requested);
    }

    #[test]
    fn palette_execute_kill_in_ports_view() {
        let mut app = App::new();
        app.view_mode = ViewMode::Ports;
        app.ports = vec![make_port(8080, Some(1234), "node", "")];
        app.palette_query = "kill".to_string();
        app.palette_selected = 0;
        app.execute_palette_command();
        assert!(app.pending_port_kill.is_some());
        assert_eq!(app.overlay, Overlay::Confirm);
    }

    #[test]
    fn palette_execute_empty_does_nothing() {
        let mut app = App::new();
        app.palette_query = "zzzzz_nonexistent".to_string();
        app.execute_palette_command(); // should not panic
        assert!(app.running); // still running
    }

    // -----------------------------------------------------------------------
    // Help overlay
    // -----------------------------------------------------------------------

    #[test]
    fn help_overlay_opens_and_closes() {
        let mut app = App::new();

        app.handle_key(key(KeyCode::Char('?')));
        assert_eq!(app.overlay, Overlay::Help);

        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.overlay, Overlay::None);
    }

    #[test]
    fn help_overlay_closes_with_question_mark() {
        let mut app = App::new();
        app.overlay = Overlay::Help;

        app.handle_key(key(KeyCode::Char('?')));
        assert_eq!(app.overlay, Overlay::None);
    }

    // -----------------------------------------------------------------------
    // Quit
    // -----------------------------------------------------------------------

    #[test]
    fn q_quits_from_services() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Char('q')));
        assert!(!app.running);
    }

    #[test]
    fn q_quits_from_ports() {
        let mut app = App::new();
        app.view_mode = ViewMode::Ports;
        app.handle_key(key(KeyCode::Char('q')));
        assert!(!app.running);
    }

    #[test]
    fn q_quits_from_sessions() {
        let mut app = App::new();
        app.view_mode = ViewMode::Sessions;
        app.handle_key(key(KeyCode::Char('q')));
        assert!(!app.running);
    }

    // -----------------------------------------------------------------------
    // Detail tab switching
    // -----------------------------------------------------------------------

    #[test]
    fn detail_tab_switching() {
        let mut app = App::new();
        assert_eq!(app.detail_tab, DetailTab::Info);

        app.handle_key(key(KeyCode::Char('l')));
        assert_eq!(app.detail_tab, DetailTab::Logs);
        assert!(app.log_needs_refresh);

        app.handle_key(key(KeyCode::Char('i')));
        assert_eq!(app.detail_tab, DetailTab::Info);
    }

    #[test]
    fn space_toggles_auto_scroll_on_logs_tab() {
        let mut app = App::new();
        app.detail_tab = DetailTab::Logs;
        assert!(app.log_auto_scroll);

        app.handle_key(key(KeyCode::Char(' ')));
        assert!(!app.log_auto_scroll);

        app.handle_key(key(KeyCode::Char(' ')));
        assert!(app.log_auto_scroll);
    }

    #[test]
    fn space_on_info_tab_does_nothing() {
        let mut app = App::new();
        app.detail_tab = DetailTab::Info;
        app.log_auto_scroll = true;
        app.handle_key(key(KeyCode::Char(' ')));
        assert!(app.log_auto_scroll); // unchanged
    }

    // -----------------------------------------------------------------------
    // Service actions
    // -----------------------------------------------------------------------

    #[test]
    fn stop_running_service_shows_confirm() {
        let mut app = App::new();
        app.services = vec![make_service(
            "nginx",
            ProviderType::Systemd,
            ServiceStatus::Running,
        )];
        app.selected_index = 0;

        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.overlay, Overlay::Confirm);
        assert!(app.pending_action.is_some());
        let pa = app.pending_action.as_ref().unwrap();
        assert_eq!(pa.action, ServiceAction::Stop);
    }

    #[test]
    fn stop_stopped_service_does_nothing() {
        let mut app = App::new();
        app.services = vec![make_service(
            "nginx",
            ProviderType::Systemd,
            ServiceStatus::Stopped,
        )];
        app.selected_index = 0;

        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.overlay, Overlay::None);
        assert!(app.pending_action.is_none());
    }

    #[test]
    fn restart_any_service_shows_confirm() {
        let mut app = App::new();
        app.services = vec![make_service(
            "nginx",
            ProviderType::Systemd,
            ServiceStatus::Stopped,
        )];
        app.selected_index = 0;

        app.handle_key(key(KeyCode::Char('r')));
        assert_eq!(app.overlay, Overlay::Confirm);
        let pa = app.pending_action.as_ref().unwrap();
        assert_eq!(pa.action, ServiceAction::Restart);
    }

    #[test]
    fn start_stopped_service_no_confirm() {
        let mut app = App::new();
        app.services = vec![make_service(
            "nginx",
            ProviderType::Systemd,
            ServiceStatus::Stopped,
        )];
        app.selected_index = 0;

        app.handle_key(key(KeyCode::Char('S')));
        // Start doesn't show confirm overlay - directly sets confirm_pending
        assert!(app.confirm_pending);
        assert!(app.pending_action.is_some());
        assert_eq!(
            app.pending_action.as_ref().unwrap().action,
            ServiceAction::Start
        );
    }

    #[test]
    fn start_running_service_does_nothing() {
        let mut app = App::new();
        app.services = vec![make_service(
            "nginx",
            ProviderType::Systemd,
            ServiceStatus::Running,
        )];
        app.selected_index = 0;

        app.handle_key(key(KeyCode::Char('S')));
        assert!(!app.confirm_pending);
        assert!(app.pending_action.is_none());
    }

    #[test]
    fn confirm_overlay_enter_triggers_confirm_pending() {
        let mut app = App::new();
        app.services = vec![make_service(
            "nginx",
            ProviderType::Systemd,
            ServiceStatus::Running,
        )];
        app.selected_index = 0;

        // Press 's' to stop -> confirm overlay
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.overlay, Overlay::Confirm);

        // Enter confirms
        app.handle_key(key(KeyCode::Enter));
        assert!(app.confirm_pending);
        assert_eq!(app.overlay, Overlay::None);
    }

    #[test]
    fn confirm_overlay_esc_cancels() {
        let mut app = App::new();
        app.services = vec![make_service(
            "nginx",
            ProviderType::Systemd,
            ServiceStatus::Running,
        )];
        app.selected_index = 0;

        app.handle_key(key(KeyCode::Char('s')));
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.confirm_pending);
        assert!(app.pending_action.is_none());
        assert_eq!(app.overlay, Overlay::None);
    }

    // -----------------------------------------------------------------------
    // Enable/Disable (systemd only)
    // -----------------------------------------------------------------------

    #[test]
    fn enable_systemd_sets_action() {
        let mut app = App::new();
        app.services = vec![make_service(
            "nginx",
            ProviderType::Systemd,
            ServiceStatus::Running,
        )];
        app.selected_index = 0;

        app.handle_key(key(KeyCode::Char('e')));
        assert!(app.confirm_pending);
        let pa = app.pending_action.as_ref().unwrap();
        assert_eq!(pa.action, ServiceAction::Enable);
    }

    #[test]
    fn enable_docker_does_nothing() {
        let mut app = App::new();
        app.services = vec![make_service(
            "myapp",
            ProviderType::Docker,
            ServiceStatus::Running,
        )];
        app.selected_index = 0;

        app.handle_key(key(KeyCode::Char('e')));
        assert!(!app.confirm_pending);
        assert!(app.pending_action.is_none());
    }

    #[test]
    fn disable_systemd_shows_confirm() {
        let mut app = App::new();
        app.services = vec![make_service(
            "nginx",
            ProviderType::Systemd,
            ServiceStatus::Running,
        )];
        app.selected_index = 0;

        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(app.overlay, Overlay::Confirm);
        let pa = app.pending_action.as_ref().unwrap();
        assert_eq!(pa.action, ServiceAction::Disable);
    }

    // -----------------------------------------------------------------------
    // running_count / stopped_count
    // -----------------------------------------------------------------------

    #[test]
    fn running_and_stopped_counts() {
        let mut app = App::new();
        app.services = vec![
            make_service("a", ProviderType::Systemd, ServiceStatus::Running),
            make_service("b", ProviderType::Systemd, ServiceStatus::Running),
            make_service("c", ProviderType::Systemd, ServiceStatus::Stopped),
            make_service("d", ProviderType::Systemd, ServiceStatus::Degraded),
            make_service("e", ProviderType::Systemd, ServiceStatus::Unknown),
        ];
        assert_eq!(app.running_count(), 2);
        assert_eq!(app.stopped_count(), 2); // Stopped + Degraded
    }

    // -----------------------------------------------------------------------
    // Ports view detail tabs
    // -----------------------------------------------------------------------

    #[test]
    fn ports_view_tab_switching() {
        let mut app = App::new();
        app.view_mode = ViewMode::Ports;
        assert_eq!(app.detail_tab, DetailTab::Info);

        app.handle_key(key(KeyCode::Char('l')));
        assert_eq!(app.detail_tab, DetailTab::Logs);
        assert!(app.port_log_needs_refresh);

        app.handle_key(key(KeyCode::Char('i')));
        assert_eq!(app.detail_tab, DetailTab::Info);
    }

    // -----------------------------------------------------------------------
    // Search in ports view clamps port_selected
    // -----------------------------------------------------------------------

    #[test]
    fn port_search_clamps_selection() {
        let mut app = App::new();
        app.view_mode = ViewMode::Ports;
        app.ports = vec![
            make_port(80, Some(1), "nginx", ""),
            make_port(3000, Some(2), "node", ""),
            make_port(5432, Some(3), "postgres", ""),
        ];
        app.port_selected = 2;

        // Enter search, type filter that narrows to 1 result
        app.handle_key(key(KeyCode::Char('/')));
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key(KeyCode::Char('o')));
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Char('e')));
        // "node" matches only port 3000

        // port_selected should be clamped to 0 (only 1 result)
        assert_eq!(app.port_selected, 0);
    }
}
