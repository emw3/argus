pub mod claude;
pub mod mosh;
pub mod ssh;
pub mod tmux;

#[derive(Debug, Clone)]
pub enum SessionInfo {
    Ssh(SshSession),
    Mosh(MoshSession),
    Tmux(TmuxSession),
    Claude(ClaudeSession),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConnectionType {
    Ssh,
    Mosh,
    Local,
}

#[derive(Debug, Clone)]
pub struct SshSession {
    pub user: String,
    pub terminal: String,
    pub remote_ip: String,
    pub login_time: String,
    pub connection_type: ConnectionType,
    pub current_process: String,
}

#[derive(Debug, Clone)]
pub struct MoshSession {
    pub pid: u32,
    pub tmux_target: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TmuxSession {
    pub name: String,
    pub attached: bool,
    pub attached_count: u32,
    pub windows: u32,
    pub created: String,
    pub mosh_clients: u32,
    pub panes: Vec<TmuxPane>,
}

#[derive(Debug, Clone)]
pub struct TmuxPane {
    pub window_index: u32,
    pub pane_index: u32,
    pub command: String,
    pub size: String,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClaudeMode {
    Interactive,
    Agent,
}

#[derive(Debug, Clone)]
pub struct ClaudeSession {
    pub project: String,
    pub mode: ClaudeMode,
    pub pid: Option<u32>,
}
