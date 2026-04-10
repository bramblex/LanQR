use std::net::Ipv4Addr;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum LaunchMode {
    Idle,
    Share(ShareTarget),
}

#[derive(Debug, Clone)]
pub struct ShareTarget {
    pub original_path: PathBuf,
    pub display_name: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShareStatus {
    Idle,
    Starting,
    Running,
    Stopped,
    Error,
}

#[derive(Debug, Clone)]
pub struct NetworkCandidate {
    pub ip: Ipv4Addr,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct ShareSession {
    pub ip: Ipv4Addr,
    pub port: u16,
    pub route: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessState {
    NotStarted,
    Running,
    Exited(Option<i32>),
}
