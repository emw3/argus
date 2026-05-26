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
                    }
                    KeyCode::Enter => {
                        self.confirm_pending = true;
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
