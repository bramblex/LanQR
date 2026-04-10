use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, LanQrError>;

#[derive(Debug, Error)]
pub enum LanQrError {
    #[error("目标路径不存在：{0}")]
    TargetNotFound(PathBuf),
    #[error("无法访问目标路径：{0}")]
    TargetAccessDenied(PathBuf),
    #[error("共享服务启动失败：{0}")]
    ShareServiceStartFailed(String),
    #[error("无法找到合适的局域网 IPv4")]
    NoLanIpv4,
    #[error("端口分配失败")]
    PortAllocationFailed,
    #[error("右键菜单安装失败：{0}")]
    ContextMenuInstallFailed(String),
    #[error("右键菜单卸载失败：{0}")]
    ContextMenuUninstallFailed(String),
    #[error("复制链接失败：{0}")]
    ClipboardFailed(String),
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
