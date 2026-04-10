use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::time::Duration;

use std::fs;

use arboard::Clipboard;
use eframe::egui::{self, ColorImage, FontData, FontDefinitions, FontFamily, RichText, TextureHandle, TextureOptions};
use tracing::{error, info, warn};

use crate::context_menu;
use crate::errors::LanQrError;
use crate::models::{LaunchMode, NetworkCandidate, ProcessState, ShareSession, ShareStatus, ShareTarget};
use crate::network;
use crate::qr;
use crate::share_service::ShareService;

const QR_SIZE: u32 = 320;

pub struct LanQrApp {
    launch_mode: LaunchMode,
    exe_path: PathBuf,
    service: Option<ShareService>,
    network_candidates: Vec<NetworkCandidate>,
    selected_ip_index: usize,
    share_status: ShareStatus,
    session: Option<ShareSession>,
    qr_texture: Option<TextureHandle>,
    error_message: Option<String>,
    info_message: Option<String>,
}

impl LanQrApp {
    pub fn new(cc: &eframe::CreationContext<'_>, launch_mode: LaunchMode, exe_path: PathBuf) -> Self {
        configure_fonts(&cc.egui_ctx);
        cc.egui_ctx.request_repaint_after(Duration::from_millis(500));

        let network_candidates = match network::discover_ipv4_candidates() {
            Ok(items) => items,
            Err(error) => {
                warn!(error = %error, "failed to discover ipv4 candidates");
                Vec::new()
            }
        };

        let mut app = Self {
            launch_mode,
            exe_path,
            service: None,
            network_candidates,
            selected_ip_index: 0,
            share_status: ShareStatus::Idle,
            session: None,
            qr_texture: None,
            error_message: None,
            info_message: None,
        };

        app.service = Some(ShareService::new());

        if matches!(app.launch_mode, LaunchMode::Share(_)) {
            app.start_share(&cc.egui_ctx);
        }

        app
    }

    fn start_share(&mut self, ctx: &egui::Context) {
        let Some(target) = self.current_target().cloned() else {
            return;
        };
        let Some(ip) = self.selected_ip() else {
            self.set_error(LanQrError::NoLanIpv4);
            return;
        };
        let Some(service) = self.service.as_mut() else {
            return;
        };

        self.share_status = ShareStatus::Starting;
        self.error_message = None;
        self.info_message = None;

        match service.start(&target, ip) {
            Ok(session) => {
                self.share_status = ShareStatus::Running;
                self.info_message = Some("共享已启动".to_string());
                self.update_qr(ctx, &session.url);
                info!(url = session.url.as_str(), "share started");
                self.session = Some(session);
            }
            Err(error) => self.set_error(error),
        }
    }

    fn restart_share(&mut self, ctx: &egui::Context) {
        let Some(target) = self.current_target().cloned() else {
            return;
        };
        let Some(ip) = self.selected_ip() else {
            self.set_error(LanQrError::NoLanIpv4);
            return;
        };
        let Some(service) = self.service.as_mut() else {
            return;
        };

        self.share_status = ShareStatus::Starting;
        self.error_message = None;

        match service.restart(&target, ip) {
            Ok(session) => {
                self.share_status = ShareStatus::Running;
                self.info_message = Some("共享已重新生成".to_string());
                self.update_qr(ctx, &session.url);
                self.session = Some(session);
            }
            Err(error) => self.set_error(error),
        }
    }

    fn stop_share(&mut self) {
        let Some(service) = self.service.as_mut() else {
            self.share_status = ShareStatus::Stopped;
            return;
        };

        match service.stop() {
            Ok(()) => {
                self.share_status = ShareStatus::Stopped;
                self.session = None;
                self.info_message = Some("共享已停止".to_string());
            }
            Err(error) => self.set_error(error),
        }
    }

    fn poll_process(&mut self) {
        let Some(service) = self.service.as_mut() else {
            return;
        };

        match service.poll_status() {
            ProcessState::NotStarted => {}
            ProcessState::Running => {
                if self.share_status == ShareStatus::Starting {
                    self.share_status = ShareStatus::Running;
                }
            }
            ProcessState::Exited(code) => {
                self.share_status = ShareStatus::Stopped;
                self.session = None;
                self.info_message = Some(match code {
                    Some(code) => format!("共享服务已退出，退出码：{code}"),
                    None => "共享服务已停止".to_string(),
                });
            }
        }
    }

    fn update_qr(&mut self, ctx: &egui::Context, url: &str) {
        match qr::build_qr_texture_input(url, QR_SIZE) {
            Ok(image) => self.set_texture(ctx, image),
            Err(error) => self.set_error(error),
        }
    }

    fn set_texture(&mut self, ctx: &egui::Context, image: ColorImage) {
        if let Some(texture) = self.qr_texture.as_mut() {
            texture.set(image, TextureOptions::NEAREST);
        } else {
            self.qr_texture = Some(ctx.load_texture("lanqr-qr", image, TextureOptions::NEAREST));
        }
    }

    fn copy_link(&mut self) {
        let Some(session) = self.session.as_ref() else {
            return;
        };

        match Clipboard::new()
            .and_then(|mut clipboard| clipboard.set_text(session.url.clone()))
            .map_err(|error| LanQrError::ClipboardFailed(error.to_string()))
        {
            Ok(()) => {
                self.info_message = Some("链接已复制到剪贴板".to_string());
                self.error_message = None;
            }
            Err(error) => self.set_error(error),
        }
    }

    fn install_context_menu(&mut self) {
        match context_menu::install(&self.exe_path) {
            Ok(()) => {
                self.info_message = Some("右键菜单安装成功".to_string());
                self.error_message = None;
            }
            Err(error) => self.set_error(error),
        }
    }

    fn uninstall_context_menu(&mut self) {
        match context_menu::uninstall() {
            Ok(()) => {
                self.info_message = Some("右键菜单卸载成功".to_string());
                self.error_message = None;
            }
            Err(error) => self.set_error(error),
        }
    }

    fn set_error(&mut self, error: LanQrError) {
        error!(error = %error, "application error");
        self.share_status = ShareStatus::Error;
        self.error_message = Some(error.to_string());
    }

    fn current_target(&self) -> Option<&ShareTarget> {
        match &self.launch_mode {
            LaunchMode::Idle => None,
            LaunchMode::Share(target) => Some(target),
        }
    }

    fn selected_ip(&self) -> Option<Ipv4Addr> {
        self.network_candidates.get(self.selected_ip_index).map(|candidate| candidate.ip)
    }

}

impl eframe::App for LanQrApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_process();
        ctx.request_repaint_after(Duration::from_millis(500));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            if let Some(target) = self.current_target() {
                ui.heading(format!("邻享码 - {}", target.display_name));
            } else {
                ui.heading("邻享码");
            }
            ui.label(RichText::new(status_text(&self.share_status)).strong());
            ui.add_space(8.0);

            if let Some(target) = self.current_target() {
                ui.label(format!("对象：{}", target.display_name));
                ui.label(format!("路径：{}", target.original_path.display()));
                ui.label(format!("类型：{}", if target.is_dir { "文件夹" } else { "文件" }));
            } else {
                ui.label("未选择分享对象");
                ui.label("请从资源管理器右键文件或文件夹启动，或使用命令行传入路径。");
            }

            ui.add_space(8.0);

            if !self.network_candidates.is_empty() {
                egui::ComboBox::from_label("本机局域网 IP")
                    .selected_text(
                        self.network_candidates
                            .get(self.selected_ip_index)
                            .map(|item| item.label.as_str())
                            .unwrap_or("未选择"),
                    )
                    .show_ui(ui, |ui| {
                        for (index, candidate) in self.network_candidates.iter().enumerate() {
                            ui.selectable_value(&mut self.selected_ip_index, index, candidate.label.as_str());
                        }
                    });
            } else {
                ui.label(RichText::new("未找到可用的局域网 IPv4").color(egui::Color32::RED));
            }

            ui.add_space(10.0);

            if let Some(texture) = self.qr_texture.as_ref() {
                ui.vertical_centered(|ui| {
                    ui.image(texture);
                });
            } else {
                ui.group(|ui| {
                    ui.set_min_height(220.0);
                    ui.vertical_centered(|ui| {
                        ui.add_space(88.0);
                        ui.label("二维码会显示在这里");
                    });
                });
            }

            ui.add_space(10.0);

            if let Some(session) = self.session.as_ref() {
                ui.label(format!("访问链接：{}", session.url));
                ui.label(format!("端口：{}", session.port));
                ui.label(format!("路径片段：{}", session.route));
                ui.label(format!("当前 IP：{}", session.ip));
            }

            if let Some(message) = self.error_message.as_ref() {
                ui.add_space(8.0);
                ui.colored_label(egui::Color32::RED, message);
            }

            if let Some(message) = self.info_message.as_ref() {
                ui.add_space(8.0);
                ui.colored_label(egui::Color32::from_rgb(0, 130, 80), message);
            }

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let can_share = self.current_target().is_some();
                if ui
                    .add_enabled(self.session.is_some(), egui::Button::new("复制链接"))
                    .clicked()
                {
                    self.copy_link();
                }
                if ui
                    .add_enabled(can_share, egui::Button::new("重新生成"))
                    .clicked()
                {
                    self.restart_share(ctx);
                }
                if ui
                    .add_enabled(self.service.is_some(), egui::Button::new("关闭分享"))
                    .clicked()
                {
                    self.stop_share();
                }
            });

            if matches!(self.launch_mode, LaunchMode::Idle) {
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("安装右键菜单").clicked() {
                        self.install_context_menu();
                    }
                    if ui.button("卸载右键菜单").clicked() {
                        self.uninstall_context_menu();
                    }
                });
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(service) = self.service.as_mut() {
            if let Err(error) = service.stop() {
                error!(error = %error, "failed to stop share service on exit");
            }
        }
    }
}

fn status_text(status: &ShareStatus) -> &'static str {
    match status {
        ShareStatus::Idle => "当前状态：空闲",
        ShareStatus::Starting => "当前状态：启动中",
        ShareStatus::Running => "当前状态：运行中",
        ShareStatus::Stopped => "当前状态：已停止",
        ShareStatus::Error => "当前状态：出错",
    }
}

fn configure_fonts(ctx: &egui::Context) {
    let Some(font_bytes) = load_windows_cjk_font() else {
        warn!("no suitable windows cjk font found, using egui default fonts");
        return;
    };

    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "lanqr-cjk".to_string(),
        FontData::from_owned(font_bytes).into(),
    );

    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "lanqr-cjk".to_string());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "lanqr-cjk".to_string());

    ctx.set_fonts(fonts);
}

fn load_windows_cjk_font() -> Option<Vec<u8>> {
    let windows_dir = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
    let candidates = [
        "Fonts\\msyh.ttc",
        "Fonts\\msyhbd.ttc",
        "Fonts\\msyh.ttf",
        "Fonts\\simhei.ttf",
        "Fonts\\simsun.ttc",
        "Fonts\\simsun.ttc",
    ];

    for relative in candidates {
        let path = PathBuf::from(&windows_dir).join(relative);
        match fs::read(&path) {
            Ok(bytes) => {
                info!(font = %path.display(), "loaded windows cjk font");
                return Some(bytes);
            }
            Err(_) => continue,
        }
    }

    None
}
