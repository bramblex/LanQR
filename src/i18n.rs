use crate::errors::LanQrError;
use crate::models::ShareStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguagePreference {
    Auto,
    Chinese,
    English,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiLanguage {
    Chinese,
    English,
}

#[derive(Debug, Clone, Copy)]
pub struct I18n {
    preference: LanguagePreference,
    detected: UiLanguage,
}

impl I18n {
    pub fn new(preference: LanguagePreference, detected: UiLanguage) -> Self {
        Self {
            preference,
            detected,
        }
    }

    pub fn lang(self) -> UiLanguage {
        match self.preference {
            LanguagePreference::Auto => self.detected,
            LanguagePreference::Chinese => UiLanguage::Chinese,
            LanguagePreference::English => UiLanguage::English,
        }
    }

    fn pick(self, zh: &'static str, en: &'static str) -> &'static str {
        match self.lang() {
            UiLanguage::Chinese => zh,
            UiLanguage::English => en,
        }
    }

    pub fn app_title(self) -> &'static str {
        self.pick("邻享码", "LanQR")
    }

    pub fn heading(self, target_name: Option<&str>) -> String {
        match target_name {
            Some(target_name) => format!("{} - {target_name}", self.app_title()),
            None => self.app_title().to_string(),
        }
    }

    pub fn status_text(self, status: &ShareStatus) -> &'static str {
        match status {
            ShareStatus::Idle => self.pick("当前状态：空闲", "Status: Idle"),
            ShareStatus::Starting => self.pick("当前状态：启动中", "Status: Starting"),
            ShareStatus::Running => self.pick("当前状态：运行中", "Status: Running"),
            ShareStatus::Stopped => self.pick("当前状态：已停止", "Status: Stopped"),
            ShareStatus::Error => self.pick("当前状态：出错", "Status: Error"),
        }
    }

    pub fn object_label(self) -> &'static str {
        self.pick("对象", "Target")
    }

    pub fn path_label(self) -> &'static str {
        self.pick("路径", "Path")
    }

    pub fn type_label(self) -> &'static str {
        self.pick("类型", "Type")
    }

    pub fn target_type(self, is_dir: bool) -> &'static str {
        if is_dir {
            self.pick("文件夹", "Folder")
        } else {
            self.pick("文件", "File")
        }
    }

    pub fn no_target(self) -> &'static str {
        self.pick("未选择分享对象", "No share target selected")
    }

    pub fn no_target_help(self) -> &'static str {
        self.pick(
            "请从资源管理器右键文件或文件夹启动，或使用命令行传入路径。",
            "Launch from the Explorer context menu or pass a file/folder path on the command line.",
        )
    }

    pub fn language_label(self) -> &'static str {
        self.pick("语言", "Language")
    }

    pub fn language_choice(self, preference: LanguagePreference) -> &'static str {
        match preference {
            LanguagePreference::Auto => self.pick("跟随系统", "Follow system"),
            LanguagePreference::Chinese => "中文",
            LanguagePreference::English => "English",
        }
    }

    pub fn lan_ip_label(self) -> &'static str {
        self.pick("本机局域网 IP", "LAN IP")
    }

    pub fn not_selected(self) -> &'static str {
        self.pick("未选择", "Not selected")
    }

    pub fn no_lan_ipv4(self) -> &'static str {
        self.pick("未找到可用的局域网 IPv4", "No available LAN IPv4 address found")
    }

    pub fn qr_placeholder(self) -> &'static str {
        self.pick("二维码会显示在这里", "The QR code will appear here")
    }

    pub fn url_label(self) -> &'static str {
        self.pick("访问链接", "URL")
    }

    pub fn port_label(self) -> &'static str {
        self.pick("端口", "Port")
    }

    pub fn route_label(self) -> &'static str {
        self.pick("路径片段", "Route token")
    }

    pub fn current_ip_label(self) -> &'static str {
        self.pick("当前 IP", "Active IP")
    }

    pub fn copy_link(self) -> &'static str {
        self.pick("复制链接", "Copy Link")
    }

    pub fn regenerate(self) -> &'static str {
        self.pick("重新生成", "Regenerate")
    }

    pub fn stop_share(self) -> &'static str {
        self.pick("关闭分享", "Stop Share")
    }

    pub fn install_context_menu(self) -> &'static str {
        self.pick("安装右键菜单", "Install Context Menu")
    }

    pub fn uninstall_context_menu(self) -> &'static str {
        self.pick("卸载右键菜单", "Uninstall Context Menu")
    }

    pub fn share_started(self) -> &'static str {
        self.pick("共享已启动", "Share started")
    }

    pub fn share_regenerated(self) -> &'static str {
        self.pick("共享已重新生成", "Share regenerated")
    }

    pub fn share_stopped(self) -> &'static str {
        self.pick("共享已停止", "Share stopped")
    }

    pub fn link_copied(self) -> &'static str {
        self.pick("链接已复制到剪贴板", "Link copied to clipboard")
    }

    pub fn context_menu_installed(self) -> &'static str {
        self.pick("右键菜单安装成功", "Context menu installed")
    }

    pub fn context_menu_uninstalled(self) -> &'static str {
        self.pick("右键菜单卸载成功", "Context menu uninstalled")
    }

    pub fn service_exited(self, code: Option<i32>) -> String {
        match code {
            Some(code) => match self.lang() {
                UiLanguage::Chinese => format!("共享服务已退出，退出码：{code}"),
                UiLanguage::English => format!("Share service exited with code: {code}"),
            },
            None => self.share_stopped().to_string(),
        }
    }

    pub fn menu_text(self) -> &'static str {
        self.pick("生成局域网二维码", "Generate LAN QR")
    }

    pub fn error(self, error: &LanQrError) -> String {
        match error {
            LanQrError::TargetNotFound(path) => match self.lang() {
                UiLanguage::Chinese => format!("目标路径不存在：{}", path.display()),
                UiLanguage::English => format!("Target path not found: {}", path.display()),
            },
            LanQrError::TargetAccessDenied(path) => match self.lang() {
                UiLanguage::Chinese => format!("无法访问目标路径：{}", path.display()),
                UiLanguage::English => format!("Cannot access target path: {}", path.display()),
            },
            LanQrError::ShareServiceStartFailed(message) => match self.lang() {
                UiLanguage::Chinese => format!("共享服务启动失败：{message}"),
                UiLanguage::English => format!("Failed to start share service: {message}"),
            },
            LanQrError::NoLanIpv4 => self.no_lan_ipv4().to_string(),
            LanQrError::PortAllocationFailed => self.pick("端口分配失败", "Failed to allocate port").to_string(),
            LanQrError::ContextMenuInstallFailed(message) => match self.lang() {
                UiLanguage::Chinese => format!("右键菜单安装失败：{message}"),
                UiLanguage::English => format!("Failed to install context menu: {message}"),
            },
            LanQrError::ContextMenuUninstallFailed(message) => match self.lang() {
                UiLanguage::Chinese => format!("右键菜单卸载失败：{message}"),
                UiLanguage::English => format!("Failed to uninstall context menu: {message}"),
            },
            LanQrError::ClipboardFailed(message) => match self.lang() {
                UiLanguage::Chinese => format!("复制链接失败：{message}"),
                UiLanguage::English => format!("Failed to copy link: {message}"),
            },
            LanQrError::Message(message) => message.clone(),
            LanQrError::Io(error) => match self.lang() {
                UiLanguage::Chinese => format!("I/O 错误：{error}"),
                UiLanguage::English => format!("I/O error: {error}"),
            },
        }
    }
}

pub fn detect_system_language() -> UiLanguage {
    let locale = sys_locale::get_locale()
        .or_else(|| std::env::var("LC_ALL").ok())
        .or_else(|| std::env::var("LANG").ok())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if locale.starts_with("zh") {
        UiLanguage::Chinese
    } else {
        UiLanguage::English
    }
}
