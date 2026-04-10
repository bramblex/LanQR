use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::time::Duration;

use std::fs;

use arboard::Clipboard;
use eframe::egui::{self, ColorImage, FontData, FontDefinitions, FontFamily, RichText, TextureHandle, TextureOptions};
use tracing::{error, info, warn};

use crate::context_menu;
use crate::errors::LanQrError;
use crate::i18n::{I18n, LanguagePreference, UiLanguage};
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
    language_preference: LanguagePreference,
    detected_language: UiLanguage,
    share_status: ShareStatus,
    session: Option<ShareSession>,
    qr_texture: Option<TextureHandle>,
    error_message: Option<String>,
    info_message: Option<String>,
}

impl LanQrApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        launch_mode: LaunchMode,
        exe_path: PathBuf,
        detected_language: UiLanguage,
    ) -> Self {
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
            language_preference: LanguagePreference::Auto,
            detected_language,
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
                self.set_info(self.i18n().share_started());
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
                self.set_info(self.i18n().share_regenerated());
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
                self.set_info(self.i18n().share_stopped());
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
                self.set_info(self.i18n().service_exited(code));
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
                self.set_info(self.i18n().link_copied());
            }
            Err(error) => self.set_error(error),
        }
    }

    fn install_context_menu(&mut self) {
        match context_menu::install(&self.exe_path, self.i18n().lang()) {
            Ok(()) => {
                self.set_info(self.i18n().context_menu_installed());
            }
            Err(error) => self.set_error(error),
        }
    }

    fn uninstall_context_menu(&mut self) {
        match context_menu::uninstall() {
            Ok(()) => {
                self.set_info(self.i18n().context_menu_uninstalled());
            }
            Err(error) => self.set_error(error),
        }
    }

    fn set_error(&mut self, error: LanQrError) {
        error!(error = %error, "application error");
        self.share_status = ShareStatus::Error;
        self.error_message = Some(self.i18n().error(&error));
    }

    fn set_info(&mut self, message: impl Into<String>) {
        self.info_message = Some(message.into());
        self.error_message = None;
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

    fn i18n(&self) -> I18n {
        I18n::new(self.language_preference, self.detected_language)
    }
}

impl eframe::App for LanQrApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_process();
        ctx.request_repaint_after(Duration::from_millis(500));

        let i18n = self.i18n();
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            ui.heading(i18n.heading(self.current_target().map(|target| target.display_name.as_str())));
            ui.label(RichText::new(i18n.status_text(&self.share_status)).strong());
            ui.add_space(8.0);

            if let Some(target) = self.current_target() {
                ui.label(format!("{}: {}", i18n.object_label(), target.display_name));
                ui.label(format!("{}: {}", i18n.path_label(), target.original_path.display()));
                ui.label(format!("{}: {}", i18n.type_label(), i18n.target_type(target.is_dir)));
            } else {
                ui.label(i18n.no_target());
                ui.label(i18n.no_target_help());
            }

            ui.add_space(8.0);
            let mut language_preference = self.language_preference;
            egui::ComboBox::from_label(i18n.language_label())
                .selected_text(i18n.language_choice(language_preference))
                .show_ui(ui, |ui| {
                    for preference in [
                        LanguagePreference::Auto,
                        LanguagePreference::Chinese,
                        LanguagePreference::English,
                    ] {
                        ui.selectable_value(
                            &mut language_preference,
                            preference,
                            i18n.language_choice(preference),
                        );
                    }
                });
            if language_preference != self.language_preference {
                self.language_preference = language_preference;
                self.info_message = None;
                self.error_message = None;
            }

            let i18n = self.i18n();

            if !self.network_candidates.is_empty() {
                egui::ComboBox::from_label(i18n.lan_ip_label())
                    .selected_text(
                        self.network_candidates
                            .get(self.selected_ip_index)
                            .map(|item| item.label.as_str())
                            .unwrap_or(i18n.not_selected()),
                    )
                    .show_ui(ui, |ui| {
                        for (index, candidate) in self.network_candidates.iter().enumerate() {
                            ui.selectable_value(&mut self.selected_ip_index, index, candidate.label.as_str());
                        }
                    });
            } else {
                ui.label(RichText::new(i18n.no_lan_ipv4()).color(egui::Color32::RED));
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
                        ui.label(i18n.qr_placeholder());
                    });
                });
            }

            ui.add_space(10.0);

            if let Some(session) = self.session.as_ref() {
                ui.label(format!("{}: {}", i18n.url_label(), session.url));
                ui.label(format!("{}: {}", i18n.port_label(), session.port));
                ui.label(format!("{}: {}", i18n.route_label(), session.route));
                ui.label(format!("{}: {}", i18n.current_ip_label(), session.ip));
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
                    .add_enabled(self.session.is_some(), egui::Button::new(i18n.copy_link()))
                    .clicked()
                {
                    self.copy_link();
                }
                if ui
                    .add_enabled(can_share, egui::Button::new(i18n.regenerate()))
                    .clicked()
                {
                    self.restart_share(ctx);
                }
                if ui
                    .add_enabled(self.service.is_some(), egui::Button::new(i18n.stop_share()))
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
                    if ui.button(i18n.install_context_menu()).clicked() {
                        self.install_context_menu();
                    }
                    if ui.button(i18n.uninstall_context_menu()).clicked() {
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
